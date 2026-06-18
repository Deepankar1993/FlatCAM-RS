//! `flatcam-gui` — the desktop front-end for the FlatCAM Rust port.
//!
//! This is the Phase-7 scaffold: a native `eframe`/`egui` application that opens
//! Gerber/Excellon files, renders their geometry on a pan/zoom 2D canvas, runs
//! the CAM operations from `fc-cam`, and overlays the resulting tool-paths. It
//! replaces the PyQt6 + VisPy/matplotlib stack that makes the Python app
//! sluggish; all geometry and CAM work runs through the native Rust crates.
//!
//! The canvas renders ring outlines (robust for any polygon) for copper and
//! colours tool-paths distinctly. The architecture keeps all compute in the
//! library crates, so the UI thread only does layout + drawing.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use fc_cam::{CutoutParams, IsolationParams, NccParams, PaintParams};
use fc_gcode::JobKind;
use fc_geo::MultiPolygon;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 750.0]),
        ..Default::default()
    };
    // Optional file path as first CLI arg.
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

/// A drawable layer: a set of polylines plus a colour.
struct Layer {
    name: String,
    rings: Vec<Vec<(f64, f64)>>,
    color: egui::Color32,
    closed: bool,
}

#[derive(Default)]
struct Camera {
    center: (f64, f64),
    scale: f32, // screen px per world unit
    initialized: bool,
}

impl Camera {
    fn fit(&mut self, bounds: (f64, f64, f64, f64), rect: egui::Rect) {
        let (minx, miny, maxx, maxy) = bounds;
        let w = (maxx - minx).max(1e-6);
        let h = (maxy - miny).max(1e-6);
        let sx = rect.width() as f64 / w;
        let sy = rect.height() as f64 / h;
        self.scale = (sx.min(sy) * 0.85) as f32;
        self.center = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
        self.initialized = true;
    }
    fn to_screen(&self, p: (f64, f64), rect: egui::Rect) -> egui::Pos2 {
        let c = rect.center();
        egui::pos2(
            c.x + ((p.0 - self.center.0) as f32) * self.scale,
            c.y - ((p.1 - self.center.1) as f32) * self.scale, // flip Y
        )
    }
    /// Inverse of [`to_screen`]: screen point → world coordinates.
    fn to_world(&self, p: egui::Pos2, rect: egui::Rect) -> (f64, f64) {
        let c = rect.center();
        let s = self.scale.max(1e-6);
        (
            self.center.0 + ((p.x - c.x) / s) as f64,
            self.center.1 - ((p.y - c.y) / s) as f64, // flip Y
        )
    }
}

/// Which interactive editor is active.
#[derive(Default)]
enum Editor {
    #[default]
    None,
    Geo(fc_editor::GeoEditor),
    Gerber(fc_editor::GerberEditor),
    Exc(fc_editor::ExcEditor),
}

/// The current edit action (what a canvas click does).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum EditTool {
    #[default]
    Select,
    Pad,
    Drill,
    Point,
    Circle,
    Rect,
    Path, // accumulate points; Finish commits as track/line/region
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
    gerber: Option<fc_gerber::Gerber>,
    excellon: Option<fc_excellon::Excellon>,
    layers: Vec<Layer>,
    camera: Camera,
    params: CamParams,
    status: String,
    last_gcode: Option<String>,
    preproc: String,
    // --- editor state ---
    editor: Editor,
    edit_tool: EditTool,
    edit_size: f64,
    pending_path: Vec<(f64, f64)>,
    exc_selected: Option<(i32, usize)>,
    /// Geometry baked from the active editor; used as the CAM region when set.
    edit_region: Option<MultiPolygon<f64>>,
}

fn map_units(u: fc_gerber::Units) -> fc_gcode::Units {
    match u {
        fc_gerber::Units::Mm => fc_gcode::Units::Mm,
        fc_gerber::Units::Inch => fc_gcode::Units::Inch,
    }
}

impl FlatCamApp {
    fn load_path(&mut self, path: &str) {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                self.status = format!("Failed to read {path}: {e}");
                return;
            }
        };
        let lower = path.to_lowercase();
        if lower.ends_with(".svg") {
            match fc_svg::parse(&text) {
                Ok(svg) => {
                    let mut rings = rings_of(&svg.polygons);
                    for l in &svg.polylines {
                        rings.push(l.coords().map(|c| (c.x, c.y)).collect());
                    }
                    self.layers = vec![Layer {
                        name: "SVG".into(),
                        rings,
                        color: egui::Color32::from_rgb(160, 200, 90),
                        closed: false,
                    }];
                    self.camera.initialized = false;
                    self.gerber = None;
                    self.status = format!(
                        "Loaded {} ({} shapes, {} paths)",
                        path,
                        svg.polygons.0.len(),
                        svg.polylines.len()
                    );
                }
                Err(err) => self.status = format!("SVG parse error: {err}"),
            }
            return;
        }
        let is_drill = lower.ends_with(".drl")
            || lower.ends_with(".nc")
            || lower.ends_with(".xln")
            || lower.ends_with(".exc");
        if is_drill {
            match fc_excellon::parse(&text) {
                Ok(e) => {
                    let mut rings = Vec::new();
                    for (_t, tool) in &e.tools {
                        for &(x, y) in &tool.drills {
                            rings.push(circle_ring(x, y, tool.diameter / 2.0, 24));
                        }
                    }
                    self.layers = vec![Layer {
                        name: "Drills".into(),
                        rings,
                        color: egui::Color32::from_rgb(80, 160, 255),
                        closed: true,
                    }];
                    self.camera.initialized = false;
                    self.status = format!("Loaded {} ({} drills)", path, e.drill_count());
                    self.excellon = Some(e);
                    self.gerber = None;
                }
                Err(err) => self.status = format!("Excellon parse error: {err}"),
            }
        } else {
            match fc_gerber::parse(&text) {
                Ok(g) => {
                    let rings = rings_of(&g.solid_geometry);
                    self.layers = vec![Layer {
                        name: "Copper".into(),
                        rings,
                        color: egui::Color32::from_rgb(200, 140, 40),
                        closed: true,
                    }];
                    self.camera.initialized = false;
                    self.status = format!(
                        "Loaded {} ({} apertures, {} polygons)",
                        path,
                        g.apertures.len(),
                        g.solid_geometry.0.len()
                    );
                    self.gerber = Some(g);
                }
                Err(err) => self.status = format!("Gerber parse error: {err}"),
            }
        }
    }

    /// The active CAM region: a baked editor geometry takes priority, else the
    /// loaded Gerber's copper.
    fn current_region(&self) -> Option<(MultiPolygon<f64>, fc_gcode::Units)> {
        if let Some(r) = &self.edit_region {
            return Some((r.clone(), fc_gcode::Units::Mm));
        }
        self.gerber
            .as_ref()
            .map(|g| (g.solid_geometry.clone(), map_units(g.units)))
    }

    fn run_isolation(&mut self) {
        let Some((geom, units)) = self.current_region() else {
            self.status = "Load a Gerber or bake an editor first".into();
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
        self.add_toolpath_layer("Isolation", &job, egui::Color32::from_rgb(60, 220, 120));
    }

    fn run_paint(&mut self) {
        let Some((geom, units)) = self.current_region() else {
            self.status = "Load a Gerber or bake an editor first".into();
            return;
        };
        let p = &self.params;
        let pp = PaintParams {
            tool_diameter: p.tool_dia,
            overlap: p.overlap.max(0.1),
            ..Default::default()
        };
        let job = fc_cam::paint_job(&geom, &pp, units);
        self.add_toolpath_layer("Paint", &job, egui::Color32::from_rgb(230, 90, 200));
    }

    fn run_ncc(&mut self) {
        let Some((geom, units)) = self.current_region() else {
            self.status = "Load a Gerber or bake an editor first".into();
            return;
        };
        let p = &self.params;
        let params = NccParams {
            tool_diameter: p.tool_dia,
            overlap: p.overlap.max(0.1),
            ..Default::default()
        };
        let job = fc_cam::ncc_job(&geom, &params, units);
        self.add_toolpath_layer("NCC", &job, egui::Color32::from_rgb(120, 200, 230));
    }

    fn run_cutout(&mut self) {
        let Some((geom, units)) = self.current_region() else {
            self.status = "Load a Gerber or bake an editor first".into();
            return;
        };
        let Some((minx, miny, maxx, maxy)) = geo_bounds(&geom) else {
            self.status = "Empty geometry".into();
            return;
        };
        let p = &self.params;
        let cp = CutoutParams { tool_diameter: p.tool_dia, ..Default::default() };
        let paths = fc_cam::cutout_rectangular(minx, miny, maxx, maxy, &cp);
        let mut jp = cp.job.clone();
        jp.units = units;
        jp.tool_diameter = cp.tool_diameter;
        let job = fc_gcode::CncJob { params: jp, kind: JobKind::Mill { paths } };
        self.add_toolpath_layer("Cutout", &job, egui::Color32::from_rgb(240, 200, 60));
    }

    fn run_drilling(&mut self) {
        let Some(e) = &self.excellon else {
            self.status = "Load an Excellon (drill) file first".into();
            return;
        };
        let units = match e.units {
            fc_excellon::Units::Mm => fc_gcode::Units::Mm,
            fc_excellon::Units::Inch => fc_gcode::Units::Inch,
        };
        let base = fc_gcode::JobParams { units, ..Default::default() };
        let jobs = fc_cam::drilling_all(e, base);
        let pp = fc_gcode::dialects::by_name(&self.preproc)
            .unwrap_or_else(|| Box::new(fc_gcode::Grbl));
        let mut all_points: Vec<(f64, f64)> = Vec::new();
        let mut gcode = String::new();
        for (tool, job) in &jobs {
            if let JobKind::Drill { points } = &job.kind {
                all_points.extend(points.iter().copied());
            }
            gcode.push_str(&format!("(--- tool T{tool} ---)\n"));
            gcode.push_str(&job.to_gcode(pp.as_ref()));
        }
        let n = all_points.len();
        let rings: Vec<Vec<(f64, f64)>> = all_points.into_iter().map(|p| vec![p]).collect();
        self.layers.push(Layer {
            name: "Drilling".into(),
            rings,
            color: egui::Color32::from_rgb(80, 160, 255),
            closed: false,
        });
        self.last_gcode = Some(gcode);
        self.status = format!("Drilling: {n} holes — G-code ready ({})", pp.name());
    }

    fn add_toolpath_layer(&mut self, name: &str, job: &fc_gcode::CncJob, color: egui::Color32) {
        let rings = match &job.kind {
            JobKind::Mill { paths } => paths.clone(),
            JobKind::Drill { points } => points.iter().map(|&p| vec![p]).collect(),
        };
        let n = rings.len();
        self.layers.push(Layer { name: name.into(), rings, color, closed: false });
        // Generate G-code with the selected preprocessor and keep it for export.
        let pp = fc_gcode::dialects::by_name(&self.preproc)
            .unwrap_or_else(|| Box::new(fc_gcode::Grbl));
        self.last_gcode = Some(job.to_gcode(pp.as_ref()));
        self.status = format!("{name}: {n} path(s) — G-code ready ({})", pp.name());
    }

    fn save_gcode(&mut self) {
        let Some(gcode) = &self.last_gcode else {
            self.status = "Nothing to save — run Isolation or Paint first".into();
            return;
        };
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("output.gcode")
            .save_file()
        {
            match std::fs::write(&path, gcode) {
                Ok(()) => self.status = format!("Saved {}", path.to_string_lossy()),
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
    }

    // ----- interactive editors -----

    fn start_editor(&mut self, kind: &str) {
        self.edit_region = None;
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
        self.status = format!("{kind} editor — click to add, Bake to use for CAM");
    }

    fn editor_active(&self) -> bool {
        !matches!(self.editor, Editor::None)
    }

    /// 0 none, 1 geo, 2 gerber, 3 excellon.
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
        let tol = (self.edit_size.max(0.5)) * 1.5;
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

    fn delete_selected(&mut self) {
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
        let rings = rings_of(&mp);
        self.edit_region = Some(mp);
        self.layers.retain(|l| l.name != "Edit");
        self.layers.push(Layer {
            name: "Edit".into(),
            rings,
            color: egui::Color32::from_rgb(150, 220, 150),
            closed: true,
        });
        self.camera.initialized = false;
        self.status = "Baked editor geometry — CAM ops now use it".into();
    }

    fn editor_overlay(&self) -> Vec<Vec<(f64, f64)>> {
        let mut rings = self.editor_geometry().map(|m| rings_of(&m)).unwrap_or_default();
        if self.pending_path.len() >= 2 {
            rings.push(self.pending_path.clone());
        }
        rings
    }
}

impl eframe::App for FlatCamApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open Gerber/Drill…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_file() {
                        self.load_path(&path.to_string_lossy());
                    }
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
                if ui.button("Clear tool-paths").clicked() {
                    self.layers.retain(|l| l.name == "Copper" || l.name == "Drills");
                    self.status = "Cleared tool-paths".into();
                }
            });
        });

        egui::SidePanel::left("params").resizable(true).show(ctx, |ui| {
            ui.heading("Parameters");
            let p = &mut self.params;
            ui.add(egui::Slider::new(&mut p.tool_dia, 0.05..=3.0).text("Tool Ø"));
            ui.add(egui::Slider::new(&mut p.passes, 1..=8).text("Passes"));
            ui.add(egui::Slider::new(&mut p.overlap, 0.0..=0.9).text("Overlap"));
            ui.separator();
            ui.label("Preprocessor");
            if self.preproc.is_empty() {
                self.preproc = "grbl".into();
            }
            egui::ComboBox::from_id_salt("preproc")
                .selected_text(self.preproc.clone())
                .show_ui(ui, |ui| {
                    for name in ["grbl", "marlin", "default", "grbl_no_m6", "grbl_laser", "roland"] {
                        ui.selectable_value(&mut self.preproc, name.to_string(), name);
                    }
                });
            ui.separator();
            ui.heading("Layers");
            for l in &self.layers {
                ui.colored_label(l.color, format!("{} ({} paths)", l.name, l.rings.len()));
            }

            ui.separator();
            egui::CollapsingHeader::new("Editor").default_open(true).show(ui, |ui| {
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
                            self.delete_selected();
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Bake → region").clicked() {
                            self.bake_editor();
                        }
                        if ui.button("Close").clicked() {
                            self.editor = Editor::None;
                            self.pending_path.clear();
                        }
                    });
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.label(if self.status.is_empty() {
                "Ready — open a Gerber or Excellon file."
            } else {
                &self.status
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
            let rect = response.rect;
            painter.rect_filled(rect, 0.0, egui::Color32::from_gray(18));

            // Initialize / fit camera once geometry is present.
            if !self.camera.initialized {
                if let Some(b) = layers_bounds(&self.layers) {
                    self.camera.fit(b, rect);
                }
            }
            // Pan with drag.
            if response.dragged() {
                let d = response.drag_delta();
                self.camera.center.0 -= (d.x / self.camera.scale.max(1e-6)) as f64;
                self.camera.center.1 += (d.y / self.camera.scale.max(1e-6)) as f64;
            }
            // Zoom with scroll.
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let factor = (scroll * 0.002).exp();
                self.camera.scale *= factor;
            }

            // Edit interaction: click to add/select, Delete to remove.
            if self.editor_active() {
                if response.clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let world = self.camera.to_world(pos, rect);
                        self.handle_edit_click(world);
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                    self.delete_selected();
                }
            }

            for layer in &self.layers {
                let stroke = egui::Stroke::new(1.0, layer.color);
                for ring in &layer.rings {
                    if ring.len() == 1 {
                        let s = self.camera.to_screen(ring[0], rect);
                        painter.circle_stroke(s, 2.0, stroke);
                        continue;
                    }
                    let pts: Vec<egui::Pos2> =
                        ring.iter().map(|&p| self.camera.to_screen(p, rect)).collect();
                    for w in pts.windows(2) {
                        painter.line_segment([w[0], w[1]], stroke);
                    }
                    if layer.closed && pts.len() >= 3 {
                        painter.line_segment([pts[pts.len() - 1], pts[0]], stroke);
                    }
                }
            }

            // Editor overlay: live geometry + pending path + vertices.
            if self.editor_active() {
                let rings = self.editor_overlay();
                let line = egui::Color32::from_rgb(120, 220, 255);
                let vert = egui::Color32::from_rgb(255, 230, 120);
                let stroke = egui::Stroke::new(1.5, line);
                for ring in &rings {
                    if ring.len() == 1 {
                        let s = self.camera.to_screen(ring[0], rect);
                        painter.circle_filled(s, 3.0, line);
                        continue;
                    }
                    let pts: Vec<egui::Pos2> =
                        ring.iter().map(|&p| self.camera.to_screen(p, rect)).collect();
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

fn circle_ring(cx: f64, cy: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * (i as f64) / (n as f64);
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn geo_bounds(mp: &MultiPolygon<f64>) -> Option<(f64, f64, f64, f64)> {
    fc_geo::bounds(mp)
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

fn layers_bounds(layers: &[Layer]) -> Option<(f64, f64, f64, f64)> {
    let mut b: Option<(f64, f64, f64, f64)> = None;
    for l in layers {
        for ring in &l.rings {
            for &(x, y) in ring {
                b = Some(match b {
                    None => (x, y, x, y),
                    Some((minx, miny, maxx, maxy)) => {
                        (minx.min(x), miny.min(y), maxx.max(x), maxy.max(y))
                    }
                });
            }
        }
    }
    b
}

