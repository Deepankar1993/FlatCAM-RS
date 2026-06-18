//! GUI-agnostic plugin framework surfacing **every** stock-FlatCAM tool.
//!
//! This module is the bridge between the egui desktop shell (`main.rs`) and the
//! pure `fc_cam` CAM backend. It is deliberately **egui-free**: it contains only
//! data ([`ParamSpec`], [`PluginKind`], [`PluginOutput`]) plus calls into
//! `fc_cam` / `fc_geo` / `fc_gcode`, so it can be unit-tested without a window.
//!
//! `main.rs` uses it like so:
//!  - [`PluginKind::all`] enumerates the menu (grouped by [`PluginKind::category`]).
//!  - For the selected plugin, [`PluginKind::params`] drives a generic panel
//!    (one slider per [`ParamSpec`]).
//!  - When the user runs the plugin, `main.rs` collects the slider values in
//!    declaration order and calls [`PluginKind::apply`] with the currently
//!    selected region geometry, then dispatches the [`PluginOutput`].
//!
//! Tools that map cleanly onto a single selected region are wired to real
//! `fc_cam` functions; tools that need a second object, an Excellon file, an
//! interactive pick, or are already on the toolbar return a
//! [`PluginOutput::Message`] stub so every menu item still responds.

use fc_geo::{bounds, MultiPolygon};
use fc_gcode::Polyline;

/// One tunable numeric parameter of a plugin.
#[derive(Clone, Copy, Debug)]
pub struct ParamSpec {
    pub name: &'static str,
    pub default: f64,
    pub min: f64,
    pub max: f64,
}

impl ParamSpec {
    const fn new(name: &'static str, default: f64, min: f64, max: f64) -> Self {
        ParamSpec { name, default, min, max }
    }
}

/// The result of running a plugin against the selected region.
pub enum PluginOutput {
    /// A new region geometry -> main.rs adds it as a Geometry object.
    Region(MultiPolygon<f64>),
    /// Tool-paths -> main.rs adds them as a CNCJob object.
    Paths(Vec<Polyline>),
    /// A textual report -> main.rs shows it in the Tool panel.
    Report(String),
    /// An informational message -> main.rs shows it in the status bar.
    Message(String),
}

/// Every stock-FlatCAM tool surfaced in the Plugins menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PluginKind {
    TwoSided,
    Panelize,
    Film,
    Align,
    Subtract,
    ExtractDrills,
    Punch,
    Invert,
    Etch,
    Thieving,
    CopperFill,
    Fiducials,
    Corners,
    Markers,
    Follow,
    SolderPaste,
    Milling,
    NccTool,
    PaintTool,
    CutoutTool,
    Optimal,
    Distance,
    RulesCheck,
    Report,
    Calculators,
    Levelling,
    QrCode,
    Teardrops,
    Bridges,
    Dogbone,
    ScaleFit,
}

use PluginKind::*;

/// Menu order: grouped by category (Fabrication, Modify, Analysis, CAM, Utility).
const ALL: &[PluginKind] = &[
    // Fabrication
    TwoSided, Panelize, Film, Align, Fiducials, Corners, Markers,
    // Modify
    Subtract, Invert, Etch, Thieving, CopperFill, Teardrops, Bridges, Dogbone, ScaleFit,
    // CAM
    Follow, SolderPaste, Milling, NccTool, PaintTool, CutoutTool, ExtractDrills, Punch,
    // Analysis
    Optimal, Distance, RulesCheck, Report,
    // Utility
    Calculators, Levelling, QrCode,
];

impl PluginKind {
    /// Every variant, in menu order.
    pub fn all() -> &'static [PluginKind] {
        ALL
    }

    /// Short user-facing label.
    pub fn label(self) -> &'static str {
        match self {
            TwoSided => "2-Sided",
            Panelize => "Panelize",
            Film => "Film",
            Align => "Align Objects",
            Subtract => "Subtract",
            ExtractDrills => "Extract Drills",
            Punch => "Punch Gerber",
            Invert => "Invert Gerber",
            Etch => "Etch Compensation",
            Thieving => "Copper Thieving",
            CopperFill => "Copper Fill",
            Fiducials => "Fiducials",
            Corners => "Corner Markers",
            Markers => "Markers",
            Follow => "Follow",
            SolderPaste => "Solder Paste",
            Milling => "Milling",
            NccTool => "NCC",
            PaintTool => "Paint",
            CutoutTool => "Cutout",
            Optimal => "Optimal",
            Distance => "Distance",
            RulesCheck => "Rules Check",
            Report => "Report",
            Calculators => "Calculators",
            Levelling => "Levelling",
            QrCode => "QR Code",
            Teardrops => "Teardrops",
            Bridges => "Bridges",
            Dogbone => "Dogbone",
            ScaleFit => "Scale to Fit",
        }
    }

    /// Menu group.
    pub fn category(self) -> &'static str {
        match self {
            TwoSided | Panelize | Film | Align | Fiducials | Corners | Markers => "Fabrication",
            Subtract | Invert | Etch | Thieving | CopperFill | Teardrops | Bridges | Dogbone
            | ScaleFit => "Modify",
            Follow | SolderPaste | Milling | NccTool | PaintTool | CutoutTool | ExtractDrills
            | Punch => "CAM",
            Optimal | Distance | RulesCheck | Report => "Analysis",
            Calculators | Levelling | QrCode => "Utility",
        }
    }

    /// An `icons.rs` glyph name (it has a fallback for unknown names).
    pub fn icon(self) -> &'static str {
        match self {
            TwoSided => "twosided",
            Panelize => "panel",
            Film => "film",
            Align => "align",
            Subtract => "subtract",
            ExtractDrills => "extractdrills",
            Punch => "punch",
            Invert => "invert",
            Etch => "mirror",
            Thieving => "thieving",
            CopperFill => "copperfill",
            Fiducials => "fiducials",
            Corners => "corners",
            Markers => "markers",
            Follow => "follow",
            SolderPaste => "solderpaste",
            Milling => "milling",
            NccTool => "ncc",
            PaintTool => "paint",
            CutoutTool => "cutout",
            Optimal => "optimal",
            Distance => "distance",
            RulesCheck => "rulescheck",
            Report => "report",
            Calculators => "calculators",
            Levelling => "levelling",
            QrCode => "qrcode",
            Teardrops => "teardrops",
            Bridges => "bridges",
            Dogbone => "cutout",
            ScaleFit => "scalefit",
        }
    }

    /// Numeric params (declaration order matches the `vals` slice in `apply`).
    pub fn params(self) -> Vec<ParamSpec> {
        match self {
            TwoSided => vec![ParamSpec::new("mirror axis x", 0.0, -1000.0, 1000.0)],
            Panelize => vec![
                ParamSpec::new("columns", 2.0, 1.0, 50.0),
                ParamSpec::new("rows", 2.0, 1.0, 50.0),
                ParamSpec::new("gap", 1.0, 0.0, 100.0),
            ],
            Invert => vec![ParamSpec::new("margin", 1.0, 0.0, 100.0)],
            Etch => vec![ParamSpec::new("bias", 0.05, -1.0, 1.0)],
            Thieving => vec![
                ParamSpec::new("dot diameter", 1.0, 0.1, 10.0),
                ParamSpec::new("spacing", 2.0, 0.5, 20.0),
                ParamSpec::new("clearance", 0.5, 0.0, 10.0),
                ParamSpec::new("margin", 1.0, 0.0, 50.0),
            ],
            CopperFill => vec![
                ParamSpec::new("clearance", 0.5, 0.0, 10.0),
                ParamSpec::new("margin", 1.0, 0.0, 50.0),
            ],
            Fiducials => vec![
                ParamSpec::new("diameter", 1.0, 0.1, 10.0),
                ParamSpec::new("margin", 2.0, 0.0, 50.0),
            ],
            Corners => vec![
                ParamSpec::new("diameter", 1.0, 0.1, 10.0),
                ParamSpec::new("margin", 2.0, 0.0, 50.0),
            ],
            SolderPaste => vec![
                ParamSpec::new("nozzle diameter", 0.3, 0.05, 5.0),
                ParamSpec::new("margin", 0.0, 0.0, 5.0),
            ],
            Milling => vec![
                ParamSpec::new("tool diameter", 0.8, 0.05, 10.0),
                ParamSpec::new("outside (1) / inside (0)", 1.0, 0.0, 1.0),
            ],
            RulesCheck => vec![ParamSpec::new("min clearance", 0.2, 0.0, 10.0)],
            Teardrops => vec![
                ParamSpec::new("pad radius", 1.0, 0.1, 10.0),
                ParamSpec::new("trace width", 0.5, 0.05, 5.0),
            ],
            Bridges => vec![
                ParamSpec::new("gaps", 4.0, 0.0, 32.0),
                ParamSpec::new("gap length", 2.0, 0.1, 20.0),
            ],
            Dogbone => vec![ParamSpec::new("tool radius", 0.5, 0.0, 5.0)],
            ScaleFit => vec![
                ParamSpec::new("target width", 100.0, 1.0, 1000.0),
                ParamSpec::new("target height", 100.0, 1.0, 1000.0),
            ],
            PaintTool => vec![
                ParamSpec::new("tool diameter", 0.5, 0.05, 10.0),
                ParamSpec::new("overlap", 0.2, 0.0, 0.95),
                ParamSpec::new("margin", 0.0, 0.0, 50.0),
            ],
            NccTool => vec![
                ParamSpec::new("tool diameter", 0.5, 0.05, 10.0),
                ParamSpec::new("overlap", 0.4, 0.0, 0.95),
                ParamSpec::new("margin", 1.0, 0.0, 50.0),
            ],
            CutoutTool => vec![
                ParamSpec::new("tool diameter", 1.0, 0.05, 10.0),
                ParamSpec::new("gap size", 2.0, 0.1, 20.0),
                ParamSpec::new("gaps", 4.0, 0.0, 32.0),
                ParamSpec::new("outside (1) / on-line (0)", 1.0, 0.0, 1.0),
            ],
            Calculators => vec![
                ParamSpec::new("v-bit tip diameter", 0.2, 0.0, 5.0),
                ParamSpec::new("v-bit angle (deg)", 30.0, 1.0, 179.0),
                ParamSpec::new("cut depth", 0.05, 0.0, 5.0),
            ],
            // Stubs and parameter-free tools.
            Follow | Optimal | Distance | Report | Film | Align | Subtract | ExtractDrills
            | Punch | Markers | Levelling | QrCode => Vec::new(),
        }
    }

    /// Run against the selected region with the param values (same order as
    /// [`PluginKind::params`]). Never panics; missing values fall back to the
    /// param default, and empty regions yield a graceful [`PluginOutput`].
    pub fn apply(self, region: &MultiPolygon<f64>, vals: &[f64]) -> PluginOutput {
        // Resolve a parameter by index, falling back to its declared default
        // (and to 0.0 if the index is somehow out of range).
        let specs = self.params();
        let get = |i: usize| -> f64 {
            vals.get(i)
                .copied()
                .unwrap_or_else(|| specs.get(i).map(|s| s.default).unwrap_or(0.0))
        };

        let empty = region.0.is_empty();

        match self {
            // --- Modify: region in, region out --------------------------------
            TwoSided => {
                // Mirror about a vertical axis for the bottom side.
                let axis = get(0);
                PluginOutput::Region(fc_cam::mirror_for_bottom(region, axis))
            }
            Panelize => {
                if empty {
                    return PluginOutput::Message("Panelize: select a region first".into());
                }
                let cols = (get(0).round() as i64).max(1) as usize;
                let rows = (get(1).round() as i64).max(1) as usize;
                let gap = get(2);
                PluginOutput::Region(fc_cam::panelize_auto(region, cols, rows, gap))
            }
            Invert => {
                if empty {
                    return PluginOutput::Message("Invert: select a region first".into());
                }
                PluginOutput::Region(fc_cam::invert(region, get(0)))
            }
            Etch => {
                if empty {
                    return PluginOutput::Message("Etch Compensation: select a region first".into());
                }
                let p = fc_cam::EtchParams { factor: get(0) };
                PluginOutput::Region(fc_cam::compensate(region, &p))
            }
            Thieving => {
                if empty {
                    return PluginOutput::Message("Copper Thieving: select a region first".into());
                }
                let p = fc_cam::ThievingParams {
                    dot_dia: get(0),
                    spacing: get(1),
                    clearance: get(2),
                    margin: get(3),
                };
                PluginOutput::Region(fc_cam::thieving(region, &p))
            }
            CopperFill => {
                // Copper pour over the region's own bounding box, kept clear of
                // the existing copper by `clearance`, with the board grown by
                // `margin`.
                let clearance = get(0);
                let margin = get(1);
                match bounds(region) {
                    Some((minx, miny, maxx, maxy)) => {
                        let board = (minx - margin, miny - margin, maxx + margin, maxy + margin);
                        PluginOutput::Region(fc_cam::copper_pour(board, region, clearance))
                    }
                    None => PluginOutput::Message("Copper Fill: select a region first".into()),
                }
            }
            Fiducials => {
                // Place fiducial dots at the region's corners.
                let dia = get(0);
                let margin = get(1);
                match bounds(region) {
                    Some(b) => {
                        PluginOutput::Region(fc_cam::corner_fiducials(b, margin, dia, 32))
                    }
                    None => PluginOutput::Message("Fiducials: select a region first".into()),
                }
            }
            Corners => {
                // Corner markers: same corner-dot geometry as fiducials.
                let dia = get(0);
                let margin = get(1);
                match bounds(region) {
                    Some(b) => {
                        PluginOutput::Region(fc_cam::corner_fiducials(b, margin, dia, 16))
                    }
                    None => PluginOutput::Message("Corner Markers: select a region first".into()),
                }
            }
            Teardrops => {
                // Add a teardrop fillet at each polygon centroid->corner. With a
                // single selected region we approximate one teardrop per polygon
                // pointing from its bbox centre toward its bbox max corner, then
                // union them into the region.
                if empty {
                    return PluginOutput::Message("Teardrops: select a region first".into());
                }
                let pad_radius = get(0);
                let trace_width = get(1);
                let mut polys: Vec<fc_geo::Polygon<f64>> = region.0.clone();
                for poly in &region.0 {
                    if let Some((minx, miny, maxx, maxy)) =
                        bounds(&MultiPolygon::new(vec![poly.clone()]))
                    {
                        let pad = ((minx + maxx) / 2.0, (miny + maxy) / 2.0);
                        let trace_end = (maxx, maxy);
                        polys.push(fc_cam::teardrop(pad, pad_radius, trace_end, trace_width));
                    }
                }
                PluginOutput::Region(fc_geo::union_all(polys))
            }
            Dogbone => {
                if empty {
                    return PluginOutput::Message("Dogbone: select a region first".into());
                }
                PluginOutput::Region(fc_cam::corner_relief(region, get(0)))
            }
            ScaleFit => {
                if empty {
                    return PluginOutput::Message("Scale to Fit: select a region first".into());
                }
                PluginOutput::Region(fc_cam::scale_to_fit(region, get(0), get(1)))
            }

            // --- CAM: region in, paths out ------------------------------------
            Follow => {
                if empty {
                    return PluginOutput::Message("Follow: select a region first".into());
                }
                // `follow_paths` traces centre-lines: feed it every exterior and
                // interior ring of the region.
                let mut rings: Vec<fc_geo::LineString<f64>> = Vec::new();
                for poly in &region.0 {
                    rings.push(poly.exterior().clone());
                    for hole in poly.interiors() {
                        rings.push(hole.clone());
                    }
                }
                PluginOutput::Paths(fc_cam::follow_paths(&rings))
            }
            SolderPaste => {
                if empty {
                    return PluginOutput::Message("Solder Paste: select a region first".into());
                }
                let p = fc_cam::PasteParams {
                    nozzle_dia: get(0),
                    margin: get(1),
                    ..Default::default()
                };
                let paths = fc_cam::paste_paths(region, &p);
                if paths.is_empty() {
                    PluginOutput::Message(
                        "Solder Paste: pads too small for the nozzle (no paths)".into(),
                    )
                } else {
                    PluginOutput::Paths(paths)
                }
            }
            Milling => {
                if empty {
                    return PluginOutput::Message("Milling: select a region first".into());
                }
                let tool_diameter = get(0);
                let outside = get(1) >= 0.5;
                PluginOutput::Paths(fc_cam::milling_profile(region, tool_diameter, outside))
            }
            Bridges => {
                // Cut holding bridges into the region's outer rings.
                if empty {
                    return PluginOutput::Message("Bridges: select a region first".into());
                }
                let gaps = (get(0).round() as i64).max(0) as usize;
                let gap_len = get(1);
                let mut out: Vec<Polyline> = Vec::new();
                for poly in &region.0 {
                    let ring: Polyline =
                        poly.exterior().coords().map(|c| (c.x, c.y)).collect();
                    out.extend(fc_cam::add_bridges(&ring, gaps, gap_len));
                }
                PluginOutput::Paths(out)
            }

            // --- Analysis: region in, report/message out ----------------------
            Optimal => match fc_cam::minimum_spacing(region) {
                Some(d) => PluginOutput::Report(format!("Minimum copper spacing: {:.4}", d)),
                None => PluginOutput::Report(
                    "Minimum copper spacing: n/a (need >= 2 distinct features)".into(),
                ),
            },
            RulesCheck => {
                let min = get(0);
                let violations = fc_cam::check_clearance(region, min);
                if violations.is_empty() {
                    PluginOutput::Report(format!(
                        "Rules Check: PASS — no features closer than {:.4}",
                        min
                    ))
                } else {
                    let mut s = format!("Rules Check: {} violation(s)\n", violations.len());
                    for v in &violations {
                        s.push_str(&format!("  [{}] {}\n", v.kind, v.detail));
                    }
                    PluginOutput::Report(s)
                }
            }
            Report => {
                let r = fc_cam::report(region);
                let bounds_str = match r.bounds {
                    Some((minx, miny, maxx, maxy)) => {
                        format!("({:.3}, {:.3}) .. ({:.3}, {:.3})", minx, miny, maxx, maxy)
                    }
                    None => "none".into(),
                };
                PluginOutput::Report(format!(
                    "Geometry Report\n  polygons: {}\n  area:     {:.4}\n  bounds:   {}\n  width:    {:.4}\n  height:   {:.4}",
                    r.polygons, r.area, bounds_str, r.width, r.height
                ))
            }
            Distance => {
                // ToolDistance needs two interactive picks; with a single region
                // we report its overall extent instead.
                match bounds(region) {
                    Some((minx, miny, maxx, maxy)) => PluginOutput::Report(format!(
                        "Region extent: width {:.4}, height {:.4}, diagonal {:.4}\n(Distance tool needs two picks for a point-to-point measure.)",
                        maxx - minx,
                        maxy - miny,
                        ((maxx - minx).powi(2) + (maxy - miny).powi(2)).sqrt()
                    )),
                    None => PluginOutput::Message(
                        "Distance: needs two picks (select a region for its extent)".into(),
                    ),
                }
            }

            // --- Stubs: need a second object, an Excellon, or non-region input -
            Film => PluginOutput::Message("Film: needs a target object + tracing setup (not yet wired)".into()),
            Align => PluginOutput::Message("Align Objects: needs two objects to align (not yet wired)".into()),
            Subtract => PluginOutput::Message("Subtract: needs a second (tool) object to subtract (not yet wired)".into()),
            ExtractDrills => PluginOutput::Message("Extract Drills: needs a Gerber pad source (not yet wired)".into()),
            Punch => PluginOutput::Message("Punch Gerber: needs an Excellon drill file (not yet wired)".into()),
            Markers => PluginOutput::Message("Markers: needs interactive marker placement (not yet wired)".into()),
            // --- CAM: wired to single-region fc_cam functions -----------------
            PaintTool => {
                if empty {
                    return PluginOutput::Message("Paint: select a region first".into());
                }
                let p = fc_cam::PaintParams {
                    tool_diameter: get(0),
                    overlap: get(1),
                    margin: get(2),
                    add_contour: true,
                    ..Default::default()
                };
                let paths = fc_cam::paint_region(region, &p);
                if paths.is_empty() {
                    PluginOutput::Message(
                        "Paint: region too small for the tool (no paths)".into(),
                    )
                } else {
                    PluginOutput::Paths(paths)
                }
            }
            NccTool => {
                if empty {
                    return PluginOutput::Message("NCC: select a region first".into());
                }
                let p = fc_cam::NccParams {
                    tool_diameter: get(0),
                    overlap: get(1),
                    boundary_margin: get(2),
                    ..Default::default()
                };
                let paths = fc_cam::ncc_paths(region, &p);
                if paths.is_empty() {
                    PluginOutput::Message("NCC: nothing to clear (no paths)".into())
                } else {
                    PluginOutput::Paths(paths)
                }
            }
            CutoutTool => {
                if empty {
                    return PluginOutput::Message("Cutout: select a region first".into());
                }
                let p = fc_cam::CutoutParams {
                    tool_diameter: get(0),
                    tab_gap: get(1),
                    tabs: (get(2).round() as i64).max(0) as usize,
                    outside: get(3) >= 0.5,
                    ..Default::default()
                };
                let paths = fc_cam::cutout_geometry(region, &p);
                if paths.is_empty() {
                    PluginOutput::Message("Cutout: outline produced no cut arcs".into())
                } else {
                    PluginOutput::Paths(paths)
                }
            }
            Calculators => {
                // V-bit cut-width geometry — independent of the selected region.
                let tip_dia = get(0);
                let angle_deg = get(1);
                let depth = get(2);
                let width = fc_cam::calculators::v_bit_cut_width(depth, tip_dia, angle_deg);
                PluginOutput::Report(format!(
                    "V-Bit Calculator\n  tip diameter: {:.4}\n  included angle: {:.2} deg\n  cut depth:    {:.4}\n  => cut width: {:.4}",
                    tip_dia, angle_deg, depth, width
                ))
            }
            Levelling => PluginOutput::Message("Levelling: needs a probe grid + height map (not yet wired)".into()),
            QrCode => PluginOutput::Message("QR Code: needs text input to encode (not yet wired)".into()),
        }
    }

    /// True for plugins that are listed for parity but not yet functional from a
    /// single selected region (they always return [`PluginOutput::Message`]).
    /// The GUI greys these out in the Plugins menu.
    pub fn is_stub(self) -> bool {
        use PluginKind::*;
        matches!(
            self,
            Film | Align | Subtract | ExtractDrills | Punch | Markers | Levelling | QrCode
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region() -> MultiPolygon<f64> {
        MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 20.0, 20.0)])
    }

    #[test]
    fn all_is_nonempty_and_labelled() {
        let all = PluginKind::all();
        assert!(!all.is_empty(), "plugin list must not be empty");
        for &k in all {
            assert!(!k.label().is_empty(), "{k:?} has an empty label");
            assert!(!k.category().is_empty(), "{k:?} has an empty category");
            assert!(!k.icon().is_empty(), "{k:?} has an empty icon name");
            // params() must not panic and must be self-consistent.
            for spec in k.params() {
                assert!(spec.min <= spec.max, "{k:?} param {} has min > max", spec.name);
            }
        }
    }

    #[test]
    fn invert_returns_region() {
        let r = region();
        match Invert.apply(&r, &[1.0]) {
            PluginOutput::Region(_) => {}
            _ => panic!("Invert should return a Region"),
        }
    }

    #[test]
    fn optimal_returns_report() {
        let r = region();
        match Optimal.apply(&r, &[]) {
            PluginOutput::Report(_) => {}
            _ => panic!("Optimal should return a Report"),
        }
    }

    #[test]
    fn follow_returns_paths() {
        let r = region();
        match Follow.apply(&r, &[]) {
            PluginOutput::Paths(p) => assert!(!p.is_empty(), "follow should trace the ring(s)"),
            _ => panic!("Follow should return Paths"),
        }
    }

    #[test]
    fn paint_returns_paths() {
        let r = region();
        match PaintTool.apply(&r, &PaintTool.params().iter().map(|s| s.default).collect::<Vec<_>>()) {
            PluginOutput::Paths(p) => assert!(!p.is_empty(), "paint should produce infill paths"),
            _ => panic!("PaintTool should return Paths"),
        }
        assert!(!PaintTool.is_stub(), "PaintTool should no longer be a stub");
    }

    #[test]
    fn ncc_returns_paths() {
        let r = region();
        match NccTool.apply(&r, &NccTool.params().iter().map(|s| s.default).collect::<Vec<_>>()) {
            PluginOutput::Paths(p) => assert!(!p.is_empty(), "ncc should produce clearing paths"),
            _ => panic!("NccTool should return Paths"),
        }
        assert!(!NccTool.is_stub(), "NccTool should no longer be a stub");
    }

    #[test]
    fn cutout_returns_paths_or_region() {
        let r = region();
        match CutoutTool.apply(&r, &CutoutTool.params().iter().map(|s| s.default).collect::<Vec<_>>()) {
            PluginOutput::Paths(p) => assert!(!p.is_empty(), "cutout should produce cut arcs"),
            PluginOutput::Region(_) => {}
            _ => panic!("CutoutTool should return Paths or a Region"),
        }
        assert!(!CutoutTool.is_stub(), "CutoutTool should no longer be a stub");
    }

    #[test]
    fn calculators_returns_report() {
        let r = region();
        match Calculators.apply(&r, &Calculators.params().iter().map(|s| s.default).collect::<Vec<_>>()) {
            PluginOutput::Report(_) => {}
            _ => panic!("Calculators should return a Report"),
        }
        assert!(!Calculators.is_stub(), "Calculators should no longer be a stub");
    }

    #[test]
    fn stub_returns_message() {
        let r = region();
        match QrCode.apply(&r, &[]) {
            PluginOutput::Message(_) => {}
            _ => panic!("QrCode is a stub and should return a Message"),
        }
    }

    #[test]
    fn apply_tolerates_missing_values() {
        // No values supplied: every kind must fall back to defaults and not panic.
        let r = region();
        for &k in PluginKind::all() {
            let _ = k.apply(&r, &[]);
        }
    }

    #[test]
    fn apply_tolerates_empty_region() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        for &k in PluginKind::all() {
            // Must never panic on an empty selection.
            let out = k.apply(&empty, &[]);
            // A handful guard with a Message; others return an (empty) Region.
            match out {
                PluginOutput::Region(mp) => {
                    // Empty in usually means empty/degenerate out; just sanity-check area is finite.
                    assert!(fc_geo::area(&mp).is_finite());
                }
                PluginOutput::Paths(_) | PluginOutput::Report(_) | PluginOutput::Message(_) => {}
            }
        }
    }
}
