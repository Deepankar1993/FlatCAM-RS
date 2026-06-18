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
}
impl Default for CamParams {
    fn default() -> Self {
        CamParams { tool_dia: 0.4, passes: 1, overlap: 0.1 }
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
        self.store.insert(name.clone(), StoredObj { kind, units, geom });
        self.project.selected = Some(name.clone());
        self.camera.initialized = false;
        name
    }

    /// Parse a file into a runtime object (kind/units/geometry), or None.
    fn parse_file(path: &str) -> Option<StoredObj> {
        let text = std::fs::read_to_string(path).ok()?;
        let lower = path.to_lowercase();
        if lower.ends_with(".svg") {
            let svg = fc_svg::parse(&text).ok()?;
            Some(StoredObj { kind: ObjectKind::Svg, units: Units::Mm, geom: StoredGeom::Region(svg.polygons) })
        } else if lower.ends_with(".dxf") {
            let d = fc_dxf::parse(&text).ok()?;
            Some(StoredObj { kind: ObjectKind::Geometry, units: Units::Mm, geom: StoredGeom::Region(d.polygons) })
        } else if lower.ends_with(".drl") || lower.ends_with(".nc") || lower.ends_with(".xln") || lower.ends_with(".exc") {
            let e = fc_excellon::parse(&text).ok()?;
            let u = map_exc_units(e.units);
            Some(StoredObj { kind: ObjectKind::Excellon, units: u, geom: StoredGeom::Excellon(e) })
        } else {
            let g = fc_gerber::parse(&text).ok()?;
            let u = map_gerber_units(g.units);
            Some(StoredObj { kind: ObjectKind::Gerber, units: u, geom: StoredGeom::Region(g.solid_geometry) })
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
        self.add_object(&format!("{name}_drill"), ObjectKind::CncJob, units, StoredGeom::Cnc(all), Some(name.clone()), None);
        self.project.selected = keep;
        self.status = format!("Drilling: {n} holes — G-code ready ({})", pp.name());
    }

    fn save_gcode(&mut self) {
        let Some(gcode) = &self.last_gcode else {
            self.status = "Nothing to save — run a CAM op first".into();
            return;
        };
        if let Some(path) = rfd::FileDialog::new().set_file_name("output.gcode").save_file() {
            match std::fs::write(&path, gcode) {
                Ok(()) => self.status = format!("Saved {}", path.to_string_lossy()),
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
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
}

impl eframe::App for FlatCamApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Open…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        self.load_path(&path.to_string_lossy());
                    }
                }
                if ui.button("Open Project").clicked() {
                    self.open_project();
                }
                if ui.button("Save Project").clicked() {
                    self.save_project();
                }
                ui.separator();
                if ui.button("Isolation").clicked() {
                    self.run_isolation();
                }
                if ui.button("Paint").clicked() {
                    self.run_paint();
                }
                if ui.button("NCC").clicked() {
                    self.run_ncc();
                }
                if ui.button("Cutout").clicked() {
                    self.run_cutout();
                }
                if ui.button("Drilling").clicked() {
                    self.run_drilling();
                }
                ui.separator();
                if ui.button("Save G-code…").clicked() {
                    self.save_gcode();
                }
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
            ui.label(if self.status.is_empty() { "Ready — Open a Gerber/Excellon/SVG/DXF file." } else { &self.status });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
            let rect = response.rect;
            painter.rect_filled(rect, 0.0, egui::Color32::from_gray(18));

            if !self.camera.initialized {
                if let Some(b) = self.all_bounds() {
                    self.camera.fit(b, rect);
                }
            }
            if response.dragged() {
                let d = response.drag_delta();
                self.camera.center.0 -= (d.x / self.camera.scale.max(1e-6)) as f64;
                self.camera.center.1 += (d.y / self.camera.scale.max(1e-6)) as f64;
            }
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.camera.scale *= (scroll * 0.002).exp();
            }
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
        });
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
                            let clone = StoredObj { kind: s.kind, units: s.units, geom: clone_geom(&s.geom) };
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
        StoredGeom::Excellon(_) => StoredGeom::Cnc(Vec::new()), // drill objects duplicate as empty cnc
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
