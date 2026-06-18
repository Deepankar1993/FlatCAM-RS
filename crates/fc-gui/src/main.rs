//! `flatcam-gui` — the desktop front-end for the FlatCAM Rust port.
//!
//! An `eframe`/`egui` application built around an object-centric project model
//! (`fc-app`): loaded files and CAM results are objects in a project tree with
//! per-object visibility and selection. CAM operations act on the selected
//! object and add their results back as new CNCJob objects. Interactive editors
//! (`fc-editor`) let you build/modify geometry that can be baked into a project
//! object. All geometry/CAM/parse work runs through the native Rust crates,
//! replacing the sluggish PyQt6 + VisPy/matplotlib stack.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use fc_app::{ObjectKind, Project, ProjectObject};
use fc_cam::{CutoutParams, IsolationParams, NccParams, PaintParams};
use fc_gcode::{JobKind, Polyline, Units};
use fc_geo::MultiPolygon;
use std::collections::HashMap;

mod icons;
mod theme;
mod viewport;
use theme::{Palette, Theme};
use viewport::format_tick;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    let initial = std::env::args().nth(1);
    eframe::run_native(
        "FlatCAM-RS",
        native_options,
        Box::new(move |_cc| {
            let mut app = FlatCamApp::default();
            app.fill_on = true;
            app.laser_kerf = true;
            app.laser_dynamic = true;
            app.sim_feed = 800.0;
            app.sim_power = 1.0;
            app.units_label = "mm".into();
            if let Some(path) = initial {
                app.load_path(&path);
            }
            Ok(Box::new(app))
        }),
    )
}

// ----- runtime object store (the geometry behind each project object) -----

enum StoredGeom {
    Region(MultiPolygon<f64>),
    Excellon(fc_excellon::Excellon),
    Cnc(Vec<Polyline>),
}

struct StoredObj {
    kind: ObjectKind,
    units: Units,
    geom: StoredGeom,
    /// Cached fill triangles (region kinds only), for filled rendering.
    fill: Vec<[(f64, f64); 3]>,
    /// G-code for CNCJob objects (the result of a CAM op).
    gcode: Option<String>,
}

fn make_stored(kind: ObjectKind, units: Units, geom: StoredGeom) -> StoredObj {
    let fill = match &geom {
        StoredGeom::Region(mp) => fc_geo::triangulate(mp),
        _ => Vec::new(),
    };
    StoredObj { kind, units, geom, fill, gcode: None }
}

// ----- camera -----

#[derive(Default)]
struct Camera {
    center: (f64, f64),
    scale: f32,
    initialized: bool,
}

impl Camera {
    fn fit(&mut self, bounds: (f64, f64, f64, f64), rect: egui::Rect) {
        let (minx, miny, maxx, maxy) = bounds;
        let w = (maxx - minx).max(1e-6);
        let h = (maxy - miny).max(1e-6);
        let s = (rect.width() as f64 / w).min(rect.height() as f64 / h);
        self.scale = (s * 0.85) as f32;
        self.center = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
        self.initialized = true;
    }
    fn to_screen(&self, p: (f64, f64), rect: egui::Rect) -> egui::Pos2 {
        let c = rect.center();
        egui::pos2(
            c.x + ((p.0 - self.center.0) as f32) * self.scale,
            c.y - ((p.1 - self.center.1) as f32) * self.scale,
        )
    }
    fn to_world(&self, p: egui::Pos2, rect: egui::Rect) -> (f64, f64) {
        let c = rect.center();
        let s = self.scale.max(1e-6);
        (
            self.center.0 + ((p.x - c.x) / s) as f64,
            self.center.1 - ((p.y - c.y) / s) as f64,
        )
    }
    /// A sane default view for an empty project: roughly a 45×25 mm window with
    /// the origin toward the lower-left, like the stock FlatCAM empty canvas.
    /// Critically, this also gives the camera a non-zero `scale` so the cursor
    /// world-coordinate readout is valid before any file is opened.
    fn default_view(&mut self, rect: egui::Rect) {
        self.fit((-12.0, -3.0, 33.0, 20.0), rect);
    }
}

/// A vertical icon-over-label toolbar button drawn with vector [`icons`]
/// (no emoji-font dependency). The whole 52×44 cell is clickable and shows a
/// hover/active background from the active theme.
fn tool_button(ui: &mut egui::Ui, icon: &str, label: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(52.0, 44.0), egui::Sense::click());
    let wv = ui.style().interact(&resp);
    if resp.hovered() || resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, egui::Rounding::same(4.0), wv.bg_fill);
    }
    let color = wv.fg_stroke.color;
    let icon_rect =
        egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.top() + 15.0), egui::vec2(22.0, 22.0));
    icons::draw_tool_icon(icon, ui.painter(), icon_rect, color);
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 3.0),
        egui::Align2::CENTER_BOTTOM,
        label,
        egui::FontId::proportional(9.5),
        color,
    );
    resp
}

// ----- editor state -----

#[derive(Default)]
enum Editor {
    #[default]
    None,
    Geo(fc_editor::GeoEditor),
    Gerber(fc_editor::GerberEditor),
    Exc(fc_editor::ExcEditor),
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum EditTool {
    #[default]
    Select,
    Pad,
    Drill,
    Point,
    Circle,
    Rect,
    Path,
}

struct CamParams {
    tool_dia: f64,
    passes: i32,
    overlap: f64,
    lead: f64,
}
impl Default for CamParams {
    fn default() -> Self {
        CamParams { tool_dia: 0.4, passes: 1, overlap: 0.1, lead: 0.0 }
    }
}

#[derive(Default)]
struct FlatCamApp {
    project: Project,
    store: HashMap<String, StoredObj>,
    camera: Camera,
    params: CamParams,
    status: String,
    last_gcode: Option<String>,
    preproc: String,
    rename_buf: String,
    fill_on: bool,
    prefs: fc_app::Preferences,
    show_settings: bool,
    show_gcode: bool,
    cursor_world: Option<(f64, f64)>,
    theme: Theme,
    /// Theme last pushed to egui, so we only call `set_visuals` on a change.
    applied_theme: Option<Theme>,
    /// Status-bar grid-snap toggle (display state; snapping not yet enforced).
    grid_snap: bool,
    /// Status-bar units selector label ("mm" / "inch").
    units_label: String,
    // --- laser ---
    beam: fc_laser::BeamShape,
    /// Z-dependent astigmatic beam model; used when `use_astig` is set.
    astig: fc_laser::AstigmaticBeam,
    /// When set, the working beam is `astig.at(focus_z)` instead of `beam`.
    use_astig: bool,
    /// Focus height (machine Z) at which the astigmatic beam is evaluated.
    focus_z: f64,
    laser_kerf: bool,
    laser_dynamic: bool,
    show_burn: bool,
    burn_tex: Option<egui::TextureHandle>,
    burn_rect: Option<(f64, f64, f64, f64)>,
    /// Feed/power used for the burn simulation + fill-angle optimiser.
    sim_feed: f64,
    sim_power: f64,
    /// Pasted focus-ramp kerf measurements (`z,width_x,width_y` per line) for fit_astig.
    cal_text: String,
    /// Pasted power-curve samples (`power,depth` per line).
    curve_text: String,
    /// Apply the measured power curve to the emitted S values.
    use_curve: bool,
    // editor
    editor: Editor,
    edit_tool: EditTool,
    edit_size: f64,
    pending_path: Vec<(f64, f64)>,
    exc_selected: Option<(i32, usize)>,
}

fn map_gerber_units(u: fc_gerber::Units) -> Units {
    match u {
        fc_gerber::Units::Mm => Units::Mm,
        fc_gerber::Units::Inch => Units::Inch,
    }
}
fn map_exc_units(u: fc_excellon::Units) -> Units {
    match u {
        fc_excellon::Units::Mm => Units::Mm,
        fc_excellon::Units::Inch => Units::Inch,
    }
}

impl FlatCamApp {
    // ----- object management -----

    fn add_object(
        &mut self,
        base: &str,
        kind: ObjectKind,
        units: Units,
        geom: StoredGeom,
        parent: Option<String>,
        source: Option<String>,
    ) -> String {
        let name = self.project.unique_name(base);
        let mut obj = ProjectObject::new(name.clone(), kind);
        obj.parent = parent;
        obj.source_path = source;
        let _ = self.project.add(obj);
        self.store.insert(name.clone(), make_stored(kind, units, geom));
        self.project.selected = Some(name.clone());
        self.camera.initialized = false;
        name
    }

    /// Parse a file into a runtime object (kind/units/geometry), or None.
    fn parse_file(path: &str) -> Option<StoredObj> {
        let lower = path.to_lowercase();
        if lower.ends_with(".pdf") {
            let bytes = std::fs::read(path).ok()?;
            let pdf = fc_pdf::parse(&bytes).ok()?;
            return Some(make_stored(ObjectKind::Geometry, Units::Mm, StoredGeom::Region(pdf.polygons)));
        }
        let text = std::fs::read_to_string(path).ok()?;
        if lower.ends_with(".svg") {
            let svg = fc_svg::parse(&text).ok()?;
            Some(make_stored(ObjectKind::Svg, Units::Mm, StoredGeom::Region(svg.polygons)))
        } else if lower.ends_with(".dxf") {
            let d = fc_dxf::parse(&text).ok()?;
            Some(make_stored(ObjectKind::Geometry, Units::Mm, StoredGeom::Region(d.polygons)))
        } else if lower.ends_with(".drl") || lower.ends_with(".nc") || lower.ends_with(".xln") || lower.ends_with(".exc") {
            let e = fc_excellon::parse(&text).ok()?;
            let u = map_exc_units(e.units);
            Some(make_stored(ObjectKind::Excellon, u, StoredGeom::Excellon(e)))
        } else {
            let g = fc_gerber::parse(&text).ok()?;
            let u = map_gerber_units(g.units);
            Some(make_stored(ObjectKind::Gerber, u, StoredGeom::Region(g.solid_geometry)))
        }
    }

    fn load_path(&mut self, path: &str) {
        let base = std::path::Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("object")
            .to_string();
        match Self::parse_file(path) {
            Some(obj) => {
                let kind = obj.kind;
                let units = obj.units;
                let name = self.add_object(&base, kind, units, obj.geom, None, Some(path.to_string()));
                self.status = format!("Loaded {name} ({:?})", kind);
            }
            None => self.status = format!("Failed to load {path} (parse error or unreadable)"),
        }
    }

    fn save_project(&mut self) {
        if let Some(path) = rfd::FileDialog::new().add_filter("FlatCAM-RS project", &["json"]).set_file_name("project.json").save_file() {
            match self.project.save(&path) {
                Ok(()) => self.status = format!("Saved project {}", path.to_string_lossy()),
                Err(e) => self.status = format!("Save project failed: {e}"),
            }
        }
    }

    fn open_project(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("FlatCAM-RS project", &["json"]).pick_file() else {
            return;
        };
        match Project::load(&path) {
            Ok(proj) => {
                self.store.clear();
                let mut missing = 0;
                // Re-generate geometry for file-backed objects.
                for obj in &proj.objects {
                    if let Some(src) = &obj.source_path {
                        if let Some(stored) = Self::parse_file(src) {
                            self.store.insert(obj.name.clone(), stored);
                            continue;
                        }
                    }
                    missing += 1;
                }
                self.project = proj;
                self.camera.initialized = false;
                self.status = format!(
                    "Opened project {} ({} objects, {} without geometry)",
                    path.to_string_lossy(),
                    self.project.objects.len(),
                    missing
                );
            }
            Err(e) => self.status = format!("Open project failed: {e}"),
        }
    }

    fn selected_region(&self) -> Option<(MultiPolygon<f64>, Units, String)> {
        let name = self.project.selected.clone()?;
        let obj = self.store.get(&name)?;
        if let StoredGeom::Region(mp) = &obj.geom {
            Some((mp.clone(), obj.units, name))
        } else {
            None
        }
    }

    // ----- CAM operations (act on the selected object) -----

    fn finalize_job(&mut self, suffix: &str, source: &str, units: Units, paths: Vec<Polyline>) {
        let pp = fc_gcode::dialects::by_name(&self.preproc).unwrap_or_else(|| Box::new(fc_gcode::Grbl));
        let mut jp = fc_gcode::JobParams { units, tool_diameter: self.params.tool_dia, ..Default::default() };
        jp.units = units;
        let job = fc_gcode::CncJob { params: jp, kind: JobKind::Mill { paths: paths.clone() } };
        self.last_gcode = Some(job.to_gcode(pp.as_ref()));
        let n = paths.len();
        let keep = self.project.selected.clone();
        let name = self.add_object(
            &format!("{source}_{suffix}"),
            ObjectKind::CncJob,
            units,
            StoredGeom::Cnc(paths),
            Some(source.to_string()),
            None,
        );
        if let Some(o) = self.store.get_mut(&name) {
            o.gcode = self.last_gcode.clone();
        }
        self.project.selected = keep; // keep source selected for chaining
        self.status = format!("{name}: {n} path(s) — G-code ready ({})", pp.name());
    }

    fn run_isolation(&mut self) {
        let Some((geom, units, src)) = self.selected_region() else {
            self.status = "Select a Gerber/Geometry object first".into();
            return;
        };
        let p = &self.params;
        let params = IsolationParams {
            tool_diameter: p.tool_dia,
            passes: p.passes.max(1) as usize,
            overlap: p.overlap,
            ..Default::default()
        };
        let job = fc_cam::isolation_geo(&geom, units, &params);
        if let JobKind::Mill { paths } = job.kind {
            let lead = self.params.lead;
            let paths = if lead > 0.0 {
                paths.iter().map(|p| fc_cam::add_lead(p, lead)).collect()
            } else {
                paths
            };
            self.finalize_job("iso", &src, units, paths);
        }
    }

    fn run_paint(&mut self) {
        let Some((geom, units, src)) = self.selected_region() else {
            self.status = "Select a Gerber/Geometry object first".into();
            return;
        };
        let pp = PaintParams { tool_diameter: self.params.tool_dia, overlap: self.params.overlap.max(0.1), ..Default::default() };
        let paths = fc_cam::paint_region(&geom, &pp);
        self.finalize_job("paint", &src, units, paths);
    }

    fn run_ncc(&mut self) {
        let Some((geom, units, src)) = self.selected_region() else {
            self.status = "Select a Gerber/Geometry object first".into();
            return;
        };
        let params = NccParams { tool_diameter: self.params.tool_dia, overlap: self.params.overlap.max(0.1), ..Default::default() };
        let job = fc_cam::ncc_job(&geom, &params, units);
        if let JobKind::Mill { paths } = job.kind {
            self.finalize_job("ncc", &src, units, paths);
        }
    }

    fn run_cutout(&mut self) {
        let Some((geom, units, src)) = self.selected_region() else {
            self.status = "Select a Gerber/Geometry object first".into();
            return;
        };
        let Some((minx, miny, maxx, maxy)) = fc_geo::bounds(&geom) else {
            self.status = "Empty geometry".into();
            return;
        };
        let cp = CutoutParams { tool_diameter: self.params.tool_dia, ..Default::default() };
        let paths = fc_cam::cutout_rectangular(minx, miny, maxx, maxy, &cp);
        self.finalize_job("cutout", &src, units, paths);
    }

    fn run_drilling(&mut self) {
        let Some(name) = self.project.selected.clone() else {
            self.status = "Select an Excellon object first".into();
            return;
        };
        let (units, jobs) = {
            let Some(StoredObj { geom: StoredGeom::Excellon(e), units, .. }) = self.store.get(&name) else {
                self.status = "Select an Excellon object first".into();
                return;
            };
            let base = fc_gcode::JobParams { units: *units, ..Default::default() };
            (*units, fc_cam::drilling_all(e, base))
        };
        let pp = fc_gcode::dialects::by_name(&self.preproc).unwrap_or_else(|| Box::new(fc_gcode::Grbl));
        let mut all: Vec<Polyline> = Vec::new();
        let mut gcode = String::new();
        for (tool, job) in &jobs {
            if let JobKind::Drill { points } = &job.kind {
                for &pt in points {
                    all.push(vec![pt]);
                }
            }
            gcode.push_str(&format!("(--- tool T{tool} ---)\n"));
            gcode.push_str(&job.to_gcode(pp.as_ref()));
        }
        self.last_gcode = Some(gcode);
        let n = all.len();
        let keep = self.project.selected.clone();
        let dname = self.add_object(&format!("{name}_drill"), ObjectKind::CncJob, units, StoredGeom::Cnc(all), Some(name.clone()), None);
        if let Some(o) = self.store.get_mut(&dname) {
            o.gcode = self.last_gcode.clone();
        }
        self.project.selected = keep;
        self.status = format!("Drilling: {n} holes — G-code ready ({})", pp.name());
    }

    /// G-code of the selected CNCJob object, else the most recent.
    fn current_gcode(&self) -> Option<String> {
        if let Some(sel) = &self.project.selected {
            if let Some(o) = self.store.get(sel) {
                if let Some(g) = &o.gcode {
                    return Some(g.clone());
                }
            }
        }
        self.last_gcode.clone()
    }

    fn save_gcode(&mut self) {
        let Some(gcode) = self.current_gcode() else {
            self.status = "Nothing to save — run a CAM op first".into();
            return;
        };
        let gcode = &gcode;
        if let Some(path) = rfd::FileDialog::new().set_file_name("output.gcode").save_file() {
            match std::fs::write(&path, gcode) {
                Ok(()) => self.status = format!("Saved {}", path.to_string_lossy()),
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
    }

    /// Apply a positioning transform (mirror bottom / move to origin) to the
    /// selected Region object, rebuilding its cached geometry.
    fn transform_selected(&mut self, op: &str) {
        let Some(sel) = self.project.selected.clone() else { return };
        let (kind, units, mp) = match self.store.get(&sel) {
            Some(StoredObj { kind, units, geom: StoredGeom::Region(mp), .. }) => {
                (*kind, *units, mp.clone())
            }
            _ => {
                self.status = "Select a region object (Gerber/Geometry/SVG) first".into();
                return;
            }
        };
        let new_mp = match op {
            "mirror" => fc_geo::transform::mirror_bottom(&mp),
            "origin" => fc_geo::transform::normalize_origin(&mp),
            _ => return,
        };
        self.store.insert(sel.clone(), make_stored(kind, units, StoredGeom::Region(new_mp)));
        self.camera.initialized = false;
        self.status = format!("{op} applied to {sel}");
    }

    /// Duplicate the selected object (and its stored geometry) — toolbar Copy.
    fn copy_selected(&mut self) {
        let Some(sel) = self.project.selected.clone() else {
            self.status = "Select an object to copy".into();
            return;
        };
        if let Some(n) = self.project.duplicate(&sel) {
            if let Some(s) = self.store.get(&sel) {
                let clone = StoredObj {
                    kind: s.kind,
                    units: s.units,
                    geom: clone_geom(&s.geom),
                    fill: s.fill.clone(),
                    gcode: s.gcode.clone(),
                };
                self.store.insert(n, clone);
            }
            self.status = format!("Copied {sel}");
        }
    }

    /// Delete the selected object and its descendants — toolbar Delete.
    fn delete_selected(&mut self) {
        let Some(sel) = self.project.selected.clone() else {
            self.status = "Select an object to delete".into();
            return;
        };
        for removed in self.project.descendants(&sel) {
            self.store.remove(&removed);
        }
        self.project.remove_cascade(&sel);
        self.store.remove(&sel);
        self.camera.initialized = false;
        self.status = format!("Deleted {sel}");
    }

    /// Open a file picker, optionally filtered, and load the chosen file.
    fn open_file_dialog(&mut self, filter_name: &str, exts: &[&str]) {
        let mut dlg = rfd::FileDialog::new();
        if !exts.is_empty() {
            dlg = dlg.add_filter(filter_name, exts);
        }
        if let Some(path) = dlg.pick_file() {
            self.load_path(&path.to_string_lossy());
        }
    }

    /// The flat working beam: the astigmatic model evaluated at `focus_z` when
    /// astigmatic mode is on, otherwise the directly-edited [`fc_laser::BeamShape`].
    fn effective_beam(&self) -> fc_laser::BeamShape {
        if self.use_astig {
            self.astig.at(self.focus_z)
        } else {
            self.beam
        }
    }

    /// The measured power curve, when enabled and parseable from `curve_text`.
    fn active_curve(&self) -> Option<fc_laser::PowerCurve> {
        if !self.use_curve {
            return None;
        }
        let samples = fc_laser::parse_power_csv(&self.curve_text);
        if samples.is_empty() {
            None
        } else {
            Some(fc_laser::PowerCurve::from_samples(&samples))
        }
    }

    /// Beam-shape-compensated laser isolation of the selected region.
    fn run_laser_iso(&mut self) {
        let Some((geom, units, src)) = self.selected_region() else {
            self.status = "Select a Gerber/Geometry region first".into();
            return;
        };
        let beam = self.effective_beam();
        let passes = self.params.passes.max(1) as usize;
        let pwr = fc_laser::laser_isolation(&geom, &beam, passes, self.params.overlap, self.laser_kerf);
        let pwr = match self.active_curve() {
            Some(c) => fc_laser::recompensate_with_curve(&pwr, &c),
            None => pwr,
        };
        let jp = fc_gcode::JobParams { units, ..Default::default() };
        let gcode = fc_laser::laser_gcode(&pwr, &jp, self.laser_dynamic);
        let paths: Vec<Polyline> =
            pwr.iter().map(|r| r.iter().map(|&(x, y, _)| (x, y)).collect()).collect();
        self.last_gcode = Some(gcode.clone());
        let keep = self.project.selected.clone();
        let name = self.add_object(
            &format!("{src}_laser"),
            ObjectKind::CncJob,
            units,
            StoredGeom::Cnc(paths),
            Some(src.clone()),
            None,
        );
        if let Some(o) = self.store.get_mut(&name) {
            o.gcode = Some(gcode);
        }
        self.project.selected = keep;
        self.status = format!(
            "laser-iso: {} paths, beam {:.2}×{:.2} @ {:.0}°{} (kerf-comp {})",
            pwr.len(), beam.width_x, beam.width_y, beam.angle_deg,
            if self.use_astig { format!(" @Z{:.3}", self.focus_z) } else { String::new() },
            self.laser_kerf
        );
    }

    /// Cross-hatch area-fill (beam-orthogonal) of the selected region.
    fn run_laser_fill(&mut self) {
        let Some((geom, units, src)) = self.selected_region() else {
            self.status = "Select a Gerber/Geometry region first".into();
            return;
        };
        let beam = self.effective_beam();
        let pwr = fc_laser::laser_fill_for_beam(&geom, &beam, self.params.overlap.max(0.0));
        let pwr = match self.active_curve() {
            Some(c) => fc_laser::recompensate_with_curve(&pwr, &c),
            None => pwr,
        };
        let jp = fc_gcode::JobParams { units, ..Default::default() };
        let gcode = fc_laser::laser_gcode(&pwr, &jp, self.laser_dynamic);
        let paths: Vec<Polyline> =
            pwr.iter().map(|r| r.iter().map(|&(x, y, _)| (x, y)).collect()).collect();
        self.last_gcode = Some(gcode.clone());
        let keep = self.project.selected.clone();
        let name = self.add_object(
            &format!("{src}_fill"),
            ObjectKind::CncJob,
            units,
            StoredGeom::Cnc(paths),
            Some(src.clone()),
            None,
        );
        if let Some(o) = self.store.get_mut(&name) {
            o.gcode = Some(gcode);
        }
        self.project.selected = keep;
        self.status = format!("laser-fill: {} cross-hatch line(s)", pwr.len());
    }

    /// Fit the astigmatic beam from the pasted focus-ramp kerf measurements.
    fn fit_astig_from_text(&mut self) {
        let meas = fc_laser::parse_kerf_csv(&self.cal_text);
        if meas.is_empty() {
            self.status = "No valid measurements (lines: z,width_x,width_y)".into();
            return;
        }
        self.astig = fc_laser::fit_astig(&meas, self.astig.angle_deg);
        self.use_astig = true;
        self.focus_z = self.astig.round_spot_z().unwrap_or_else(|| self.astig.best_focus());
        self.status = format!(
            "Fitted astig from {} pts: waist {:.3}/{:.3}, focus {:.3}/{:.3}",
            meas.len(), self.astig.waist_x, self.astig.waist_y, self.astig.focus_x, self.astig.focus_y
        );
    }

    fn optimize_fill(&mut self) {
        let Some((geom, _, _)) = self.selected_region() else {
            self.status = "Select a region first".into();
            return;
        };
        let beam = self.effective_beam();
        let spacing = beam.min_extent().max(0.1);
        let (angle, cv) = fc_laser::optimal_fill_angle(&geom, &beam, spacing, self.sim_feed, self.sim_power);
        self.status = format!("Best fill angle: {angle:.0}° (burn-uniformity CV {cv:.3})");
    }

    /// Build a burn-heatmap texture for the selected CNCJob's paths.
    fn compute_burn(&mut self, ctx: &egui::Context) {
        let Some(sel) = self.project.selected.clone() else { return };
        let paths = match self.store.get(&sel) {
            Some(StoredObj { geom: StoredGeom::Cnc(p), .. }) => p.clone(),
            _ => {
                self.status = "Select a CNCJob (run Laser Iso) for burn preview".into();
                return;
            }
        };
        let beam = self.effective_beam();
        let cell = beam.max_extent().max(0.12);
        let map = fc_laser::simulate(&paths, &beam, self.sim_feed, self.sim_power, cell);
        if map.cols == 0 || map.rows == 0 {
            return;
        }
        let max = map.max().max(1e-9);
        let mut px = vec![egui::Color32::TRANSPARENT; map.cols * map.rows];
        for row in 0..map.rows {
            for col in 0..map.cols {
                let f = (map.at(col, row) / max).clamp(0.0, 1.0);
                if f > 0.0 {
                    // dark-red -> yellow heatmap; alpha grows with fluence.
                    let r = 255u8;
                    let g = (255.0 * f) as u8;
                    let a = (60.0 + 180.0 * f) as u8;
                    // image is y-down; flip so row 0 is world max-y (top).
                    px[(map.rows - 1 - row) * map.cols + col] = egui::Color32::from_rgba_unmultiplied(r, g, 0, a);
                }
            }
        }
        let img = egui::ColorImage { size: [map.cols, map.rows], pixels: px };
        let tex = ctx.load_texture("burn", img, egui::TextureOptions::NEAREST);
        self.burn_rect = Some((map.min_x, map.min_y, map.min_x + map.cols as f64 * map.cell, map.min_y + map.rows as f64 * map.cell));
        self.burn_tex = Some(tex);
        self.show_burn = true;
        self.status = "Burn preview updated".into();
    }

    /// Draw a small polar plot of kerf (orange) and power-factor (cyan) vs travel
    /// direction for the current working beam, so the anisotropy is visible.
    fn draw_polar_plot(&self, ui: &mut egui::Ui) {
        let beam = self.effective_beam();
        let samples = fc_laser::polar_samples(&beam, 72);
        let (min_k, max_k, _min_p, _max_p) = fc_laser::polar::polar_extents(&samples);
        let (resp, painter) = ui.allocate_painter(egui::vec2(150.0, 150.0), egui::Sense::hover());
        let rect = resp.rect;
        let c = rect.center();
        let r_px = (rect.width().min(rect.height()) / 2.0) - 8.0;
        // Axes + unit/power reference circle.
        let axis = egui::Stroke::new(1.0, egui::Color32::from_gray(90));
        painter.line_segment([egui::pos2(c.x - r_px, c.y), egui::pos2(c.x + r_px, c.y)], axis);
        painter.line_segment([egui::pos2(c.x, c.y - r_px), egui::pos2(c.x, c.y + r_px)], axis);
        painter.circle_stroke(c, r_px, egui::Stroke::new(1.0, egui::Color32::from_gray(60)));
        let to_screen = |pts: &[(f64, f64)], scale: f64| -> Vec<egui::Pos2> {
            pts.iter()
                .map(|&(x, y)| egui::pos2(c.x + (x * scale) as f32, c.y - (y * scale) as f32))
                .collect()
        };
        // Kerf loop: radius = kerf, scaled so max kerf reaches the plot edge.
        let kerf_scale = if max_k > 1e-9 { (r_px as f64) / max_k } else { 0.0 };
        let kpts = to_screen(&fc_laser::polar::polar_kerf_points(&samples), kerf_scale);
        if kpts.len() >= 2 {
            painter.add(egui::Shape::closed_line(kpts, egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 150, 0))));
        }
        // Dwell loop (spot extent along travel) shares the kerf scale for comparison.
        let dwell_pts: Vec<(f64, f64)> = samples
            .iter()
            .map(|s| {
                let t = s.angle_deg.to_radians();
                (s.dwell * t.cos(), s.dwell * t.sin())
            })
            .collect();
        let dpts = to_screen(&dwell_pts, kerf_scale);
        if dpts.len() >= 2 {
            painter.add(egui::Shape::closed_line(dpts, egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 220, 120))));
        }
        // Power-factor loop: radius in (0,1] -> fraction of the reference circle.
        let ppts = to_screen(&fc_laser::polar::polar_power_points(&samples), r_px as f64);
        if ppts.len() >= 2 {
            painter.add(egui::Shape::closed_line(ppts, egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 200, 220))));
        }
        ui.label(
            egui::RichText::new(format!(
                "kerf {min_k:.3}–{max_k:.3} mm   ● kerf  ● power  ● dwell",
            ))
            .small(),
        );
    }

    /// A compact horizontal legend strip for the burn heatmap (low→high fluence).
    fn draw_burn_legend(&self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(egui::vec2(150.0, 12.0), egui::Sense::hover());
        let rect = resp.rect;
        let n = 32usize;
        for i in 0..n {
            let f = i as f32 / (n - 1) as f32;
            let x0 = rect.left() + rect.width() * (i as f32) / (n as f32);
            let x1 = rect.left() + rect.width() * ((i + 1) as f32) / (n as f32);
            let col = egui::Color32::from_rgb(255, (255.0 * f) as u8, 0);
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())), 0.0, col);
        }
        ui.label(egui::RichText::new("burn: low → high fluence").small());
    }

    /// Copy persisted preferences into the active CAM parameters.
    fn apply_prefs(&mut self) {
        self.params.tool_dia = self.prefs.default_tool_dia;
        self.params.passes = self.prefs.iso_passes.max(1) as i32;
        self.params.overlap = self.prefs.iso_overlap;
        self.preproc = self.prefs.default_preproc.clone();
        self.status = "Applied preferences to parameters".into();
    }

    // ----- editors -----

    fn start_editor(&mut self, kind: &str) {
        self.pending_path.clear();
        self.exc_selected = None;
        self.edit_tool = EditTool::Select;
        if self.edit_size <= 0.0 {
            self.edit_size = 1.0;
        }
        match kind {
            "geo" => self.editor = Editor::Geo(Default::default()),
            "gerber" => self.editor = Editor::Gerber(Default::default()),
            "exc" => {
                let mut e = fc_editor::ExcEditor::default();
                e.add_tool(1, self.params.tool_dia.max(0.1));
                self.editor = Editor::Exc(e);
            }
            _ => {}
        }
        self.status = format!("{kind} editor — click to add, Bake to make an object");
    }

    fn editor_active(&self) -> bool {
        !matches!(self.editor, Editor::None)
    }
    fn editor_kind(&self) -> u8 {
        match self.editor {
            Editor::None => 0,
            Editor::Geo(_) => 1,
            Editor::Gerber(_) => 2,
            Editor::Exc(_) => 3,
        }
    }

    fn handle_edit_click(&mut self, world: (f64, f64)) {
        let tool = self.edit_tool;
        let size = self.edit_size.max(0.1);
        let dia = self.params.tool_dia.max(0.1);
        let tol = self.edit_size.max(0.5) * 1.5;
        match &mut self.editor {
            Editor::Geo(ed) => match tool {
                EditTool::Select => {
                    ed.select_at(world, tol);
                }
                EditTool::Point => {
                    ed.add_point(world);
                }
                EditTool::Circle => {
                    ed.add_circle(world.0, world.1, size, 48);
                }
                EditTool::Rect => {
                    ed.add_rect(world.0 - size / 2.0, world.1 - size / 2.0, size, size);
                }
                EditTool::Path => {
                    self.pending_path.push(world);
                }
                _ => {}
            },
            Editor::Gerber(ed) => match tool {
                EditTool::Select => {
                    ed.select_at(world, tol);
                }
                EditTool::Pad => {
                    ed.add_pad(world, dia);
                }
                EditTool::Path => {
                    self.pending_path.push(world);
                }
                _ => {}
            },
            Editor::Exc(ed) => match tool {
                EditTool::Select => {
                    self.exc_selected = ed.hit_test_drill(world, tol);
                }
                EditTool::Drill => {
                    ed.add_drill(world);
                }
                _ => {}
            },
            Editor::None => {}
        }
    }

    fn finish_path(&mut self) {
        if self.pending_path.len() < 2 {
            self.pending_path.clear();
            return;
        }
        let path = std::mem::take(&mut self.pending_path);
        let w = self.params.tool_dia.max(0.1);
        match &mut self.editor {
            Editor::Gerber(ed) => {
                ed.add_track(path, w);
            }
            Editor::Geo(ed) => {
                ed.add_line(path);
            }
            _ => {}
        }
    }

    fn delete_selected_edit(&mut self) {
        match &mut self.editor {
            Editor::Geo(ed) => {
                if let Some(i) = ed.selected {
                    ed.delete(i);
                }
            }
            Editor::Gerber(ed) => {
                if let Some(i) = ed.selected {
                    ed.delete(i);
                }
            }
            Editor::Exc(ed) => {
                if let Some((t, i)) = self.exc_selected.take() {
                    ed.delete_drill(t, i);
                }
            }
            Editor::None => {}
        }
    }

    fn editor_geometry(&self) -> Option<MultiPolygon<f64>> {
        match &self.editor {
            Editor::Geo(ed) => Some(ed.to_multipolygon()),
            Editor::Gerber(ed) => Some(ed.to_geometry(48)),
            Editor::Exc(ed) => Some(ed.to_geometry(24)),
            Editor::None => None,
        }
    }

    fn bake_editor(&mut self) {
        let Some(mp) = self.editor_geometry() else {
            self.status = "No editor active".into();
            return;
        };
        self.add_object("Edit", ObjectKind::Geometry, Units::Mm, StoredGeom::Region(mp), None, None);
        self.editor = Editor::None;
        self.pending_path.clear();
        self.status = "Baked editor into a Geometry object".into();
    }

    fn editor_overlay(&self) -> Vec<Vec<(f64, f64)>> {
        let mut rings = self.editor_geometry().map(|m| rings_of(&m)).unwrap_or_default();
        if self.pending_path.len() >= 2 {
            rings.push(self.pending_path.clone());
        }
        rings
    }

    // ----- rendering helpers -----

    fn object_rings(&self, obj: &StoredObj) -> (Vec<(Vec<(f64, f64)>, bool)>, egui::Color32) {
        let (r, g, b) = fc_app::kind_color(obj.kind);
        let color = egui::Color32::from_rgb(r, g, b);
        let mut out = Vec::new();
        match &obj.geom {
            StoredGeom::Region(mp) => {
                for ring in rings_of(mp) {
                    out.push((ring, true));
                }
            }
            StoredGeom::Excellon(e) => {
                for tool in e.tools.values() {
                    for &(x, y) in &tool.drills {
                        out.push((circle_ring(x, y, tool.diameter / 2.0, 20), true));
                    }
                    for &(a, bb) in &tool.slots {
                        out.push((vec![a, bb], false));
                    }
                }
            }
            StoredGeom::Cnc(paths) => {
                for p in paths {
                    out.push((p.clone(), false));
                }
            }
        }
        (out, color)
    }

    fn all_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut b: Option<(f64, f64, f64, f64)> = None;
        for obj in self.project.objects.iter().filter(|o| o.visible) {
            if let Some(s) = self.store.get(&obj.name) {
                let (rings, _) = self.object_rings(s);
                for (ring, _) in &rings {
                    for &(x, y) in ring {
                        b = Some(match b {
                            None => (x, y, x, y),
                            Some((mnx, mny, mxx, mxy)) => (mnx.min(x), mny.min(y), mxx.max(x), mxy.max(y)),
                        });
                    }
                }
            }
        }
        b
    }

    /// Draw the measurement grid, the red X/Y origin axes, an origin crosshair,
    /// and numeric ruler labels in left/bottom margin gutters (stock-FlatCAM
    /// style) — the CAD canvas backdrop.
    fn draw_grid(&self, painter: &egui::Painter, rect: egui::Rect, pal: &Palette) {
        let scale = self.camera.scale.max(1e-6) as f64;
        let tl = self.camera.to_world(rect.left_top(), rect);
        let br = self.camera.to_world(rect.right_bottom(), rect);
        let (min_x, max_x) = (tl.0.min(br.0), tl.0.max(br.0));
        let (min_y, max_y) = (tl.1.min(br.1), tl.1.max(br.1));

        let major = viewport::major_step(scale, 80.0);
        let minor = viewport::nice_step(major / 5.0);

        // Grid lines for a given step; `viewport::ticks` caps the count so an
        // extreme zoom-out can never schedule a runaway number of draws.
        let grid = |step: f64, color: egui::Color32, width: f32| {
            let stroke = egui::Stroke::new(width, color);
            for x in viewport::ticks(min_x, max_x, step, 600) {
                let sx = self.camera.to_screen((x, 0.0), rect).x;
                painter.line_segment([egui::pos2(sx, rect.top()), egui::pos2(sx, rect.bottom())], stroke);
            }
            for y in viewport::ticks(min_y, max_y, step, 600) {
                let sy = self.camera.to_screen((0.0, y), rect).y;
                painter.line_segment([egui::pos2(rect.left(), sy), egui::pos2(rect.right(), sy)], stroke);
            }
        };
        grid(minor, pal.grid_minor, 1.0);
        grid(major, pal.grid_major, 1.0);

        // Red origin axes (drawn only when 0 is within view).
        let o = self.camera.to_screen((0.0, 0.0), rect);
        let axis = egui::Stroke::new(1.2, pal.axis);
        let x_on = o.x >= rect.left() && o.x <= rect.right();
        let y_on = o.y >= rect.top() && o.y <= rect.bottom();
        if x_on {
            painter.line_segment([egui::pos2(o.x, rect.top()), egui::pos2(o.x, rect.bottom())], axis);
        }
        if y_on {
            painter.line_segment([egui::pos2(rect.left(), o.y), egui::pos2(rect.right(), o.y)], axis);
        }
        // Bolder origin crosshair where the axes meet.
        if x_on && y_on {
            let cs = egui::Stroke::new(1.6, pal.axis);
            painter.line_segment([egui::pos2(o.x - 7.0, o.y), egui::pos2(o.x + 7.0, o.y)], cs);
            painter.line_segment([egui::pos2(o.x, o.y - 7.0), egui::pos2(o.x, o.y + 7.0)], cs);
        }

        // Ruler gutters: opaque strips along the left (Y) and bottom (X) edges,
        // with the major-step numbers centred on each grid line — like stock.
        let gut_l = 40.0_f32;
        let gut_b = 18.0_f32;
        painter.rect_filled(
            egui::Rect::from_min_max(rect.left_top(), egui::pos2(rect.left() + gut_l, rect.bottom())),
            egui::Rounding::ZERO,
            pal.margin_bg,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - gut_b), rect.right_bottom()),
            egui::Rounding::ZERO,
            pal.margin_bg,
        );
        let font = egui::FontId::proportional(10.0);
        let by = rect.bottom() - gut_b / 2.0;
        for x in viewport::ticks(min_x, max_x, major, 200) {
            let sx = self.camera.to_screen((x, 0.0), rect).x;
            // +14 so a centred 3-digit label never straddles the left gutter edge.
            if sx > rect.left() + gut_l + 14.0 {
                painter.text(egui::pos2(sx, by), egui::Align2::CENTER_CENTER, format_tick(x), font.clone(), pal.ruler_text);
            }
        }
        let lx = rect.left() + gut_l - 3.0;
        for y in viewport::ticks(min_y, max_y, major, 200) {
            let sy = self.camera.to_screen((0.0, y), rect).y;
            if sy < rect.bottom() - gut_b && sy > rect.top() + 8.0 {
                painter.text(egui::pos2(lx, sy), egui::Align2::RIGHT_CENTER, format_tick(y), font.clone(), pal.ruler_text);
            }
        }
    }
}

impl eframe::App for FlatCamApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply the active theme + typography only when it changes.
        if self.applied_theme != Some(self.theme) {
            self.theme.apply_style(ctx);
            self.applied_theme = Some(self.theme);
        }

        // --- menu bar ---
        egui::TopBottomPanel::top("menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            self.load_path(&path.to_string_lossy());
                        }
                        ui.close_menu();
                    }
                    if ui.button("Open Project").clicked() {
                        self.open_project();
                        ui.close_menu();
                    }
                    if ui.button("Save Project").clicked() {
                        self.save_project();
                        ui.close_menu();
                    }
                    if ui.button("Save G-code…").clicked() {
                        self.save_gcode();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Settings…").clicked() {
                        self.show_settings = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.button("Zoom Fit").clicked() {
                        self.camera.initialized = false;
                        ui.close_menu();
                    }
                    ui.checkbox(&mut self.fill_on, "Fill regions");
                    ui.separator();
                    ui.label("Theme");
                    ui.radio_value(&mut self.theme, Theme::Light, "Light");
                    ui.radio_value(&mut self.theme, Theme::Dark, "Dark");
                });
                ui.menu_button("Plugins", |ui| {
                    if ui.button("G-code viewer").clicked() {
                        self.show_gcode = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.status = "FlatCAM-RS — a Rust port of FlatCAM Evo".into();
                        ui.close_menu();
                    }
                });
            });
        });

        // --- icon toolbar ---
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                // File group.
                if tool_button(ui, "gerber", "Gerber").clicked() {
                    self.open_file_dialog("Gerber", &["gbr", "ger", "gtl", "gbl", "gto", "gbo", "gts", "gbs"]);
                }
                if tool_button(ui, "excellon", "Excellon").clicked() {
                    self.open_file_dialog("Excellon", &["drl", "xln", "exc", "txt"]);
                }
                if tool_button(ui, "open", "Open").clicked() {
                    self.open_file_dialog("", &[]);
                }
                if tool_button(ui, "project", "Project").clicked() {
                    self.open_project();
                }
                if tool_button(ui, "save", "Save").clicked() {
                    self.save_project();
                }
                ui.separator();
                // Edit group.
                if tool_button(ui, "editor", "Editor").clicked() {
                    self.start_editor("geo");
                }
                if tool_button(ui, "copy", "Copy").clicked() {
                    self.copy_selected();
                }
                if tool_button(ui, "delete", "Delete").clicked() {
                    self.delete_selected();
                }
                if tool_button(ui, "setorigin", "Set Origin").clicked() {
                    self.transform_selected("origin");
                }
                if tool_button(ui, "mirror", "Mirror").clicked() {
                    self.transform_selected("mirror");
                }
                ui.separator();
                // CAM group.
                if tool_button(ui, "isolation", "Isolation").clicked() {
                    self.run_isolation();
                }
                if tool_button(ui, "paint", "Paint").clicked() {
                    self.run_paint();
                }
                if tool_button(ui, "ncc", "NCC").clicked() {
                    self.run_ncc();
                }
                if tool_button(ui, "cutout", "Cutout").clicked() {
                    self.run_cutout();
                }
                if tool_button(ui, "drilling", "Drilling").clicked() {
                    self.run_drilling();
                }
                ui.separator();
                // Output / view group.
                if tool_button(ui, "gcode", "G-code").clicked() {
                    self.show_gcode = !self.show_gcode;
                }
                if tool_button(ui, "savegcode", "Save G").clicked() {
                    self.save_gcode();
                }
                if tool_button(ui, "zoomfit", "Zoom Fit").clicked() {
                    self.camera.initialized = false;
                }
                if tool_button(ui, "settings", "Settings").clicked() {
                    self.show_settings = !self.show_settings;
                }
                ui.separator();
                ui.checkbox(&mut self.fill_on, "Fill");
            });
        });

        // --- "Plot Area" tab strip (stock FlatCAM has a tabbed plot view) ---
        egui::TopBottomPanel::top("plot_tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(egui::RichText::new("Plot Area").strong()).sense(egui::Sense::hover()));
            });
        });

        egui::SidePanel::left("tree").resizable(true).default_width(240.0).show(ctx, |ui| {
            ui.heading("Project");
            self.draw_tree(ui);
            ui.separator();
            ui.heading("Parameters");
            ui.add(egui::Slider::new(&mut self.params.tool_dia, 0.05..=3.0).text("Tool Ø"));
            ui.add(egui::Slider::new(&mut self.params.passes, 1..=8).text("Passes"));
            ui.add(egui::Slider::new(&mut self.params.overlap, 0.0..=0.9).text("Overlap"));
            ui.add(egui::Slider::new(&mut self.params.lead, 0.0..=5.0).text("Lead in/out"));
            if self.preproc.is_empty() {
                self.preproc = "grbl".into();
            }
            egui::ComboBox::from_id_salt("preproc")
                .selected_text(self.preproc.clone())
                .show_ui(ui, |ui| {
                    for name in ["grbl", "marlin", "default", "grbl_no_m6", "grbl_laser", "roland", "smoothie", "tinyg"] {
                        ui.selectable_value(&mut self.preproc, name.to_string(), name);
                    }
                });
            ui.separator();
            self.draw_editor_panel(ui);
            ui.separator();
            egui::CollapsingHeader::new("Laser (diode beam)").show(ui, |ui| {
                ui.checkbox(&mut self.use_astig, "Astigmatic (Z-dependent)");
                if self.use_astig {
                    ui.add(egui::Slider::new(&mut self.astig.waist_x, 0.02..=1.0).text("Waist X"));
                    ui.add(egui::Slider::new(&mut self.astig.waist_y, 0.02..=1.0).text("Waist Y"));
                    ui.add(egui::Slider::new(&mut self.astig.focus_x, -2.0..=2.0).text("Focus X (Z)"));
                    ui.add(egui::Slider::new(&mut self.astig.focus_y, -2.0..=2.0).text("Focus Y (Z)"));
                    ui.add(egui::Slider::new(&mut self.astig.rayleigh_x, 0.05..=3.0).text("Rayleigh X"));
                    ui.add(egui::Slider::new(&mut self.astig.rayleigh_y, 0.05..=3.0).text("Rayleigh Y"));
                    ui.add(egui::Slider::new(&mut self.astig.angle_deg, 0.0..=180.0).text("Mount angle"));
                    ui.add(egui::Slider::new(&mut self.focus_z, -2.0..=2.0).text("Focus Z"));
                    ui.horizontal(|ui| {
                        if ui.button("Round-spot Z").clicked() {
                            self.focus_z = self.astig.round_spot_z().unwrap_or_else(|| self.astig.best_focus());
                        }
                        if ui.button("Best-focus Z").clicked() {
                            self.focus_z = self.astig.best_focus();
                        }
                    });
                    let b = self.astig.at(self.focus_z);
                    ui.label(egui::RichText::new(format!("→ beam {:.3}×{:.3} @ {:.0}°", b.width_x, b.width_y, b.angle_deg)).small());
                } else {
                    ui.add(egui::Slider::new(&mut self.beam.width_x, 0.02..=1.0).text("Beam X"));
                    ui.add(egui::Slider::new(&mut self.beam.width_y, 0.02..=1.0).text("Beam Y"));
                    ui.add(egui::Slider::new(&mut self.beam.angle_deg, 0.0..=180.0).text("Beam angle"));
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.laser_kerf, "Kerf comp");
                    ui.checkbox(&mut self.laser_dynamic, "M4 dyn");
                });
                // Directional anisotropy at a glance.
                self.draw_polar_plot(ui);
                ui.horizontal(|ui| {
                    if ui.button("Laser Iso").clicked() {
                        self.run_laser_iso();
                    }
                    if ui.button("Cross-hatch fill").clicked() {
                        self.run_laser_fill();
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.sim_feed).speed(10.0).prefix("feed ").range(1.0..=20000.0));
                    ui.add(egui::DragValue::new(&mut self.sim_power).speed(0.01).prefix("power ").range(0.0..=1.0));
                });
                ui.horizontal(|ui| {
                    if ui.button("Optimize fill∠").clicked() {
                        self.optimize_fill();
                    }
                    if ui.button("Burn preview").clicked() {
                        let ctx = ui.ctx().clone();
                        self.compute_burn(&ctx);
                    }
                });
                ui.checkbox(&mut self.show_burn, "Show burn");
                if self.show_burn {
                    self.draw_burn_legend(ui);
                }
                // Measured power curve (visually-uniform burn).
                egui::CollapsingHeader::new("Power curve (power,depth)").show(ui, |ui| {
                    ui.checkbox(&mut self.use_curve, "Apply curve to S values");
                    ui.add(egui::TextEdit::multiline(&mut self.curve_text).desired_rows(3).hint_text("0,0\n0.5,0.25\n1,1"));
                });
                // Astig fit from a pasted focus-ramp kerf table.
                egui::CollapsingHeader::new("Fit astig (z,width_x,width_y)").show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.cal_text).desired_rows(3).hint_text("-0.2,0.12,0.07\n0,0.06,0.10\n0.2,0.11,0.06"));
                    if ui.button("Fit astig").clicked() {
                        self.fit_astig_from_text();
                    }
                });
            });
        });

        egui::SidePanel::right("props").resizable(true).default_width(220.0).show(ctx, |ui| {
            ui.heading("Properties");
            if let Some(obj) = self.project.selected_object() {
                for (k, v) in fc_app::properties(obj) {
                    ui.horizontal(|ui| {
                        ui.strong(k);
                        ui.label(v);
                    });
                }
            } else {
                ui.label("No selection");
            }
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let msg = if self.status.is_empty() {
                    "Ready — Open a Gerber/Excellon/SVG/DXF/PDF file."
                } else {
                    self.status.as_str()
                };
                ui.label(msg);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Idle/busy indicator dot.
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 4.0, egui::Color32::from_rgb(60, 180, 75));
                    ui.label("Idle");
                    ui.separator();
                    // Units selector.
                    egui::ComboBox::from_id_salt("units")
                        .selected_text(self.units_label.clone())
                        .width(54.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.units_label, "mm".to_string(), "mm");
                            ui.selectable_value(&mut self.units_label, "inch".to_string(), "inch");
                        });
                    ui.separator();
                    // Grid-snap toggle.
                    ui.checkbox(&mut self.grid_snap, "Snap");
                    ui.separator();
                    match self.cursor_world {
                        Some((x, y)) => ui.monospace(format!("X {x:8.3}   Y {y:8.3}")),
                        None => ui.monospace("X    —       Y    —"),
                    };
                    ui.separator();
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let pal = self.theme.palette();
            let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
            let rect = response.rect;
            painter.rect_filled(rect, 0.0, pal.plot_bg);

            if !self.camera.initialized {
                match self.all_bounds() {
                    Some(b) => self.camera.fit(b, rect),
                    None => self.camera.default_view(rect),
                }
            }
            // Measurement grid + axes + rulers, behind everything.
            self.draw_grid(&painter, rect, &pal);
            if response.dragged() {
                let d = response.drag_delta();
                self.camera.center.0 -= (d.x / self.camera.scale.max(1e-6)) as f64;
                self.camera.center.1 += (d.y / self.camera.scale.max(1e-6)) as f64;
            }
            // Zoom only when the pointer is over the canvas; clamp the scale so
            // it can never reach 0 (which would re-break the coordinate readout).
            let scroll = if response.hovered() { ui.input(|i| i.smooth_scroll_delta.y) } else { 0.0 };
            if scroll.abs() > 0.0 {
                self.camera.scale = (self.camera.scale * (scroll * 0.002).exp()).clamp(1e-4, 1e6);
            }
            self.cursor_world = response.hover_pos().map(|p| self.camera.to_world(p, rect));
            if self.editor_active() {
                if response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let w = self.camera.to_world(pos, rect);
                        self.handle_edit_click(w);
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                    self.delete_selected_edit();
                }
            }

            // Draw all visible objects (selected one highlighted).
            let sel = self.project.selected.clone();
            for obj in &self.project.objects {
                if !obj.visible {
                    continue;
                }
                let Some(s) = self.store.get(&obj.name) else { continue };
                let (rings, color) = self.object_rings(s);
                let is_sel = sel.as_deref() == Some(obj.name.as_str());
                // Filled rendering (triangulated regions), drawn under outlines.
                if self.fill_on && !s.fill.is_empty() {
                    let fill = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 70);
                    for tri in &s.fill {
                        let p: Vec<egui::Pos2> = tri.iter().map(|&pt| self.camera.to_screen(pt, rect)).collect();
                        painter.add(egui::Shape::convex_polygon(p, fill, egui::Stroke::NONE));
                    }
                }
                let stroke = egui::Stroke::new(if is_sel { 2.0 } else { 1.0 }, color);
                for (ring, closed) in &rings {
                    if ring.len() == 1 {
                        painter.circle_stroke(self.camera.to_screen(ring[0], rect), 2.0, stroke);
                        continue;
                    }
                    let pts: Vec<egui::Pos2> = ring.iter().map(|&p| self.camera.to_screen(p, rect)).collect();
                    for w in pts.windows(2) {
                        painter.line_segment([w[0], w[1]], stroke);
                    }
                    if *closed && pts.len() >= 3 {
                        painter.line_segment([pts[pts.len() - 1], pts[0]], stroke);
                    }
                }
            }

            // Laser burn-heatmap overlay (the visual optimization view).
            if self.show_burn {
                if let (Some(tex), Some((minx, miny, maxx, maxy))) = (&self.burn_tex, self.burn_rect) {
                    let tl = self.camera.to_screen((minx, maxy), rect); // world max-y = screen top
                    let br = self.camera.to_screen((maxx, miny), rect);
                    let img_rect = egui::Rect::from_two_pos(tl, br);
                    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                    painter.image(tex.id(), img_rect, uv, egui::Color32::WHITE);
                }
            }

            // Editor overlay.
            if self.editor_active() {
                let line = egui::Color32::from_rgb(120, 220, 255);
                let vert = egui::Color32::from_rgb(255, 230, 120);
                let stroke = egui::Stroke::new(1.5, line);
                for ring in &self.editor_overlay() {
                    if ring.len() == 1 {
                        painter.circle_filled(self.camera.to_screen(ring[0], rect), 3.0, line);
                        continue;
                    }
                    let pts: Vec<egui::Pos2> = ring.iter().map(|&p| self.camera.to_screen(p, rect)).collect();
                    for w in pts.windows(2) {
                        painter.line_segment([w[0], w[1]], stroke);
                    }
                    for p in &pts {
                        painter.circle_filled(*p, 2.0, vert);
                    }
                }
            }

            // Cursor crosshair + live coordinate tag.
            if let Some(p) = response.hover_pos() {
                let cs = egui::Stroke::new(1.0, pal.cursor);
                painter.line_segment([egui::pos2(p.x - 8.0, p.y), egui::pos2(p.x + 8.0, p.y)], cs);
                painter.line_segment([egui::pos2(p.x, p.y - 8.0), egui::pos2(p.x, p.y + 8.0)], cs);
                if let Some((wx, wy)) = self.cursor_world {
                    let lx = (p.x + 10.0).max(rect.left() + 44.0).min(rect.right() - 70.0);
                    let ly = (p.y - 10.0).max(rect.top() + 14.0).min(rect.bottom() - 22.0);
                    painter.text(
                        egui::pos2(lx, ly),
                        egui::Align2::LEFT_BOTTOM,
                        format!("{}, {}", format_tick(wx), format_tick(wy)),
                        egui::FontId::proportional(11.0),
                        pal.ruler_text,
                    );
                }
            }
        });

        // Settings window (binds the fc_app::Preferences model).
        let mut open = self.show_settings;
        egui::Window::new("Settings").open(&mut open).resizable(false).show(ctx, |ui| {
            ui.horizontal(|ui| { ui.label("Units"); ui.text_edit_singleline(&mut self.prefs.units); });
            ui.add(egui::Slider::new(&mut self.prefs.default_tool_dia, 0.05..=6.0).text("Tool Ø"));
            ui.add(egui::Slider::new(&mut self.prefs.default_cut_z, -5.0..=0.0).text("Cut Z"));
            ui.add(egui::Slider::new(&mut self.prefs.default_travel_z, 0.0..=10.0).text("Travel Z"));
            ui.add(egui::Slider::new(&mut self.prefs.default_feed_xy, 10.0..=2000.0).text("Feed XY"));
            ui.add(egui::Slider::new(&mut self.prefs.default_feed_z, 10.0..=1000.0).text("Feed Z"));
            ui.add(egui::Slider::new(&mut self.prefs.default_spindle, 0.0..=30000.0).text("Spindle"));
            ui.add(egui::Slider::new(&mut self.prefs.iso_passes, 1..=8).text("Iso passes"));
            ui.add(egui::Slider::new(&mut self.prefs.iso_overlap, 0.0..=0.9).text("Iso overlap"));
            ui.horizontal(|ui| { ui.label("Preprocessor"); ui.text_edit_singleline(&mut self.prefs.default_preproc); });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Apply to params").clicked() {
                    self.apply_prefs();
                }
                if ui.button("Save…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("prefs", &["json"]).set_file_name("prefs.json").save_file() {
                        let _ = self.prefs.save(&path);
                    }
                }
                if ui.button("Load…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("prefs", &["json"]).pick_file() {
                        if let Ok(p) = fc_app::Preferences::load(&path) {
                            self.prefs = p;
                        }
                    }
                }
            });
        });
        self.show_settings = open;

        // G-code viewer window (selected CNCJob, else most recent).
        let mut gopen = self.show_gcode;
        egui::Window::new("G-code").open(&mut gopen).default_size([440.0, 520.0]).show(ctx, |ui| {
            match self.current_gcode() {
                Some(text) => {
                    ui.label(format!("{} lines", text.lines().count()));
                    if ui.button("Save…").clicked() {
                        self.save_gcode();
                    }
                    ui.separator();
                    egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                        ui.monospace(text);
                    });
                }
                None => {
                    ui.label("No G-code yet — run a CAM op or select a CNCJob object.");
                }
            }
        });
        self.show_gcode = gopen;
    }
}

impl FlatCamApp {
    fn draw_tree(&mut self, ui: &mut egui::Ui) {
        let rows = self.project.tree_rows();
        if rows.is_empty() {
            ui.weak("(empty — open a file)");
        }
        let mut to_select: Option<String> = None;
        let mut to_toggle: Option<String> = None;
        for row in &rows {
            ui.horizontal(|ui| {
                ui.add_space((row.depth as f32) * 14.0);
                let mut vis = row.visible;
                if ui.checkbox(&mut vis, "").changed() {
                    to_toggle = Some(row.name.clone());
                }
                let label = format!("{} {}", fc_app::kind_icon(row.kind), row.name);
                if ui.selectable_label(row.selected, label).clicked() {
                    to_select = Some(row.name.clone());
                }
            });
        }
        if let Some(n) = to_toggle {
            self.project.toggle_visible(&n);
            self.camera.initialized = false;
        }
        if let Some(n) = to_select {
            self.project.select(&n);
            self.rename_buf = n;
        }

        // Selected-object actions.
        if let Some(sel) = self.project.selected.clone() {
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Up").clicked() {
                    self.project.move_up(&sel);
                }
                if ui.button("Down").clicked() {
                    self.project.move_down(&sel);
                }
                if ui.button("Dup").clicked() {
                    if let Some(n) = self.project.duplicate(&sel) {
                        if let Some(s) = self.store.get(&sel) {
                            let clone = StoredObj { kind: s.kind, units: s.units, geom: clone_geom(&s.geom), fill: s.fill.clone(), gcode: s.gcode.clone() };
                            self.store.insert(n, clone);
                        }
                    }
                }
                if ui.button("Del").clicked() {
                    for removed in self.project.descendants(&sel) {
                        self.store.remove(&removed);
                    }
                    self.project.remove_cascade(&sel);
                    self.store.remove(&sel);
                    self.camera.initialized = false;
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Mirror").clicked() {
                    self.transform_selected("mirror");
                }
                if ui.button("Origin").clicked() {
                    self.transform_selected("origin");
                }
                if ui.button("Export G-code…").clicked() {
                    self.save_gcode();
                }
            });
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.rename_buf);
                if ui.button("Rename").clicked() && !self.rename_buf.is_empty() && self.rename_buf != sel {
                    if self.project.rename(&sel, &self.rename_buf).is_ok() {
                        if let Some(s) = self.store.remove(&sel) {
                            self.store.insert(self.rename_buf.clone(), s);
                        }
                    } else {
                        self.status = "Rename failed (name taken?)".into();
                    }
                }
            });
        }
    }

    fn draw_editor_panel(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Editor").default_open(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Geo").clicked() {
                    self.start_editor("geo");
                }
                if ui.button("Gerber").clicked() {
                    self.start_editor("gerber");
                }
                if ui.button("Excellon").clicked() {
                    self.start_editor("exc");
                }
            });
            let kind = self.editor_kind();
            if kind != 0 {
                if self.edit_size <= 0.0 {
                    self.edit_size = 1.0;
                }
                ui.add(egui::Slider::new(&mut self.edit_size, 0.1..=20.0).text("Edit size"));
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.edit_tool, EditTool::Select, "Select");
                    match kind {
                        1 => {
                            ui.selectable_value(&mut self.edit_tool, EditTool::Point, "Point");
                            ui.selectable_value(&mut self.edit_tool, EditTool::Rect, "Rect");
                            ui.selectable_value(&mut self.edit_tool, EditTool::Circle, "Circle");
                            ui.selectable_value(&mut self.edit_tool, EditTool::Path, "Line");
                        }
                        2 => {
                            ui.selectable_value(&mut self.edit_tool, EditTool::Pad, "Pad");
                            ui.selectable_value(&mut self.edit_tool, EditTool::Path, "Track");
                        }
                        3 => {
                            ui.selectable_value(&mut self.edit_tool, EditTool::Drill, "Drill");
                        }
                        _ => {}
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Finish path").clicked() {
                        self.finish_path();
                    }
                    if ui.button("Delete sel").clicked() {
                        self.delete_selected_edit();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Bake → object").clicked() {
                        self.bake_editor();
                    }
                    if ui.button("Close").clicked() {
                        self.editor = Editor::None;
                        self.pending_path.clear();
                    }
                });
            }
        });
    }
}

fn clone_geom(g: &StoredGeom) -> StoredGeom {
    match g {
        StoredGeom::Region(mp) => StoredGeom::Region(mp.clone()),
        StoredGeom::Cnc(p) => StoredGeom::Cnc(p.clone()),
        StoredGeom::Excellon(e) => StoredGeom::Excellon(e.clone()),
    }
}

fn circle_ring(cx: f64, cy: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * (i as f64) / (n as f64);
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn rings_of(mp: &MultiPolygon<f64>) -> Vec<Vec<(f64, f64)>> {
    let mut out = Vec::new();
    for poly in &mp.0 {
        out.push(poly.exterior().coords().map(|c| (c.x, c.y)).collect());
        for hole in poly.interiors() {
            out.push(hole.coords().map(|c| (c.x, c.y)).collect());
        }
    }
    out
}
