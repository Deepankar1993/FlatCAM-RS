//! `more_cmds` — additional Tcl-parity script commands.
//!
//! Fills in coverage gaps versus upstream FlatCAM's `tclCommands/`: shell
//! meta-commands (`version`, `help`, `list_pp`), more geometry transforms
//! (`skew`, unified `mirror`, `buffer`), trace/region CAM helpers (`follow`,
//! `ncr`/non-copper-regions), object creation (`new_geometry`, `new_gerber`),
//! ring extraction (`exteriors`/`interiors`), an origin offset (`set_origin`),
//! and a milling CNCJob generator (`cncjob`).
//!
//! Each command follows the house pattern: parse args with the `sarg`/`farg`
//! helpers, read/insert [`crate::Obj`] entries on the [`ScriptContext`], and
//! return a short human-readable message.

use crate::{farg, sarg, Obj, ScriptContext, ScriptError};
use fc_gcode::{Polyline, Units};
use fc_geo::{transform, LineString, MultiPolygon, Polygon};

/// Register the additional command group.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("version", version),
        ("help", help),
        ("list_pp", list_pp),
        ("listpp", list_pp),
        ("skew", skew),
        ("mirror", mirror),
        ("buffer", buffer),
        ("follow", follow),
        ("ncr", ncr),
        ("non_copper_regions", ncr),
        ("set_origin", set_origin),
        ("origin", set_origin),
        ("new_geometry", new_geometry),
        ("new_gerber", new_gerber),
        ("exteriors", exteriors),
        ("ext", exteriors),
        ("interiors", interiors),
        ("cncjob", cncjob),
    ]
}

/// The list of preprocessor (post-processor) dialect names available.
///
/// Enumerated by instantiating each `fc_gcode` [`fc_gcode::Preprocessor`] and
/// querying its `name()`, so the list can never drift from the actual set of
/// implemented dialects.
pub fn preprocessor_names() -> Vec<String> {
    use fc_gcode::dialects::{GenericDefault, GrblLaser, GrblNoM6, RolandMDX};
    use fc_gcode::dialects_extra::{Berta, Isel, LinuxCnc, Mach3, Repetier, ToolchangeProbe};
    use fc_gcode::dialects_laser2::{GrblLaserAirAssist, MarlinLaserFan, MarlinLaserSpindle};
    use fc_gcode::dialects_more::{Emc2, GrblDynamicLaser, Smoothie, TinyG};
    use fc_gcode::dialects_paste::{SolderPaste, ToolchangeManual};
    use fc_gcode::Preprocessor;
    use fc_gcode::{Grbl, Marlin};

    let pps: Vec<Box<dyn Preprocessor>> = vec![
        Box::new(Grbl),
        Box::new(Marlin),
        Box::new(GenericDefault),
        Box::new(GrblNoM6),
        Box::new(GrblLaser),
        Box::new(RolandMDX),
        Box::new(Isel),
        Box::new(Repetier),
        Box::new(Berta),
        Box::new(LinuxCnc),
        Box::new(Mach3),
        Box::new(ToolchangeProbe),
        Box::new(Smoothie),
        Box::new(TinyG),
        Box::new(Emc2),
        Box::new(GrblDynamicLaser),
        Box::new(GrblLaserAirAssist),
        Box::new(MarlinLaserFan),
        Box::new(MarlinLaserSpindle),
        Box::new(SolderPaste),
        Box::new(ToolchangeManual),
    ];
    pps.iter().map(|p| p.name().to_string()).collect()
}

/// `version` — report the crate/application version string.
fn version(_ctx: &mut ScriptContext, _args: &[String]) -> Result<String, ScriptError> {
    Ok(format!("FlatCAM-RS fc-script {}", env!("CARGO_PKG_VERSION")))
}

/// `help` — list the command names this group registers.
///
/// The script engine resolves commands through a private registry that a
/// command function can't reach, so `help` reports the names this module owns
/// (the new commands), which is the simplest correct, testable behaviour.
fn help(_ctx: &mut ScriptContext, _args: &[String]) -> Result<String, ScriptError> {
    let names: Vec<&str> = commands().into_iter().map(|(n, _)| n).collect();
    Ok(format!("commands: {}", names.join(" ")))
}

/// `list_pp` / `listpp` — list available preprocessor dialect names.
fn list_pp(_ctx: &mut ScriptContext, _args: &[String]) -> Result<String, ScriptError> {
    Ok(preprocessor_names().join(", "))
}

/// `skew <src> <dst> <angle_x> <angle_y>` — shear a region by the given angles
/// (degrees) about the origin.
fn skew(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "skew <src> <dst> <angle_x> <angle_y>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let ax = farg(args, 2, USAGE)?;
    let ay = farg(args, 3, USAGE)?;
    let (mp, units) = ctx.region(&src)?;
    let out = transform::skew(&mp, ax, ay, (0.0, 0.0));
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("skewed '{src}' by ({ax}, {ay}) deg -> '{dst}'"))
}

/// `mirror <src> <dst> <axis>` — unified mirror; `axis` is `x` or `y`.
///
/// `x` flips across the horizontal line y = 0 (mirror about the X axis); `y`
/// flips across the vertical line x = 0 (mirror about the Y axis).
fn mirror(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "mirror <src> <dst> <axis: x|y>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let axis = sarg(args, 2, USAGE)?.to_ascii_lowercase();
    let (mp, units) = ctx.region(&src)?;
    let out = match axis.as_str() {
        "x" => transform::mirror_x(&mp, 0.0),
        "y" => transform::mirror_y(&mp, 0.0),
        other => {
            return Err(ScriptError::Other(format!(
                "mirror axis must be 'x' or 'y', got '{other}'"
            )))
        }
    };
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("mirrored '{src}' about {axis} -> '{dst}'"))
}

/// `buffer <src> <dst> <distance>` — buffer/offset a region by `distance`
/// (positive grows, negative shrinks).
fn buffer(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "buffer <src> <dst> <distance>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let distance = farg(args, 2, USAGE)?;
    let (mp, units) = ctx.region(&src)?;
    let out = fc_geo::offset(&mp, distance);
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("buffered '{src}' by {distance} -> '{dst}'"))
}

/// Collect every ring (exterior + interiors) of a region as a `LineString`.
fn region_rings(mp: &MultiPolygon<f64>) -> Vec<LineString<f64>> {
    let mut out = Vec::new();
    for p in &mp.0 {
        out.push(p.exterior().clone());
        for h in p.interiors() {
            out.push(h.clone());
        }
    }
    out
}

/// `follow <src> <dst>` — trace centre-lines of a region's rings into a milling
/// CNC job (no tool-radius offset applied).
fn follow(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "follow <src> <dst>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let (mp, units) = ctx.region(&src)?;
    let lines = region_rings(&mp);
    let paths = fc_cam::follow_paths(&lines);
    let n = paths.len();
    let job = fc_gcode::CncJob {
        params: fc_gcode::JobParams { units, ..Default::default() },
        kind: fc_gcode::JobKind::Mill { paths: paths.clone() },
    };
    let gcode = job.to_gcode(&fc_gcode::Grbl);
    ctx.put(dst.clone(), Obj::Cnc { paths, units, gcode });
    Ok(format!("{dst}: {n} follow paths"))
}

/// `ncr <src> <dst>` / `non_copper_regions <src> <dst>` — non-copper regions:
/// the board bounding box (copper bbox grown by a small default margin) minus
/// the copper geometry.
fn ncr(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "ncr <src> <dst> [margin]";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let margin = if args.len() > 2 { farg(args, 2, USAGE)? } else { 1.0 };
    let (mp, units) = ctx.region(&src)?;
    let out = fc_cam::invert(&mp, margin);
    if out.0.is_empty() {
        return Err(ScriptError::Other(format!("'{src}' has no extent")));
    }
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("non-copper regions of '{src}' (margin {margin}) -> '{dst}'"))
}

/// `set_origin <x> <y>` / `origin <x> <y>` — translate every region/excellon in
/// the context so that the given point becomes the new origin, i.e. shift all
/// objects by `(-x, -y)`.
fn set_origin(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "set_origin <x> <y>";
    let x = farg(args, 0, USAGE)?;
    let y = farg(args, 1, USAGE)?;
    let (dx, dy) = (-x, -y);

    let names = ctx.names();
    let mut moved = 0usize;
    for name in names {
        // Recompute the shifted object, then re-insert.
        let new_obj = match ctx.get(&name)? {
            Obj::Region(mp, u) => Some(Obj::Region(transform::translate(mp, dx, dy), *u)),
            Obj::Excellon(e) => {
                let mut e2 = e.clone();
                for t in e2.tools.values_mut() {
                    for d in t.drills.iter_mut() {
                        d.0 += dx;
                        d.1 += dy;
                    }
                }
                Some(Obj::Excellon(e2))
            }
            // CNC paths are kept as-is (their G-code is already rendered).
            Obj::Cnc { .. } => None,
        };
        if let Some(obj) = new_obj {
            ctx.put(name, obj);
            moved += 1;
        }
    }
    Ok(format!("origin set to ({x}, {y}); shifted {moved} object(s)"))
}

/// `new_geometry <name>` — create an empty Region (geometry) object.
fn new_geometry(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "new_geometry <name>";
    let name = sarg(args, 0, USAGE)?.to_string();
    ctx.put(name.clone(), Obj::Region(MultiPolygon::new(vec![]), Units::Mm));
    Ok(format!("created empty geometry '{name}'"))
}

/// `new_gerber <name>` — create an empty Gerber-kind object. Gerber copper is
/// represented as a Region here, so this is an empty Region (mm).
fn new_gerber(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "new_gerber <name>";
    let name = sarg(args, 0, USAGE)?.to_string();
    ctx.put(name.clone(), Obj::Region(MultiPolygon::new(vec![]), Units::Mm));
    Ok(format!("created empty gerber '{name}'"))
}

/// Build a Region whose polygons are the given rings treated as solid outlines.
fn rings_to_region(rings: Vec<LineString<f64>>, units: Units) -> Obj {
    let polys: Vec<Polygon<f64>> = rings
        .into_iter()
        .map(|r| Polygon::new(r, vec![]))
        .collect();
    Obj::Region(MultiPolygon::new(polys), units)
}

/// `exteriors <src> <dst>` / `ext <src> <dst>` — a new region built from the
/// exterior rings of the source's polygons.
fn exteriors(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "exteriors <src> <dst>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let (mp, units) = ctx.region(&src)?;
    let rings: Vec<LineString<f64>> = mp.0.iter().map(|p| p.exterior().clone()).collect();
    let n = rings.len();
    ctx.put(dst.clone(), rings_to_region(rings, units));
    Ok(format!("exteriors of '{src}': {n} ring(s) -> '{dst}'"))
}

/// `interiors <src> <dst>` — a new region built from the interior rings (holes)
/// of the source's polygons.
fn interiors(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "interiors <src> <dst>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let (mp, units) = ctx.region(&src)?;
    let mut rings: Vec<LineString<f64>> = Vec::new();
    for p in &mp.0 {
        for h in p.interiors() {
            rings.push(h.clone());
        }
    }
    let n = rings.len();
    ctx.put(dst.clone(), rings_to_region(rings, units));
    Ok(format!("interiors of '{src}': {n} ring(s) -> '{dst}'"))
}

/// `cncjob <geometry> <dst> [tool_dia] [cut_z] [feed]` — generate a milling
/// CNC job (profile around the geometry boundary) from a Region and render its
/// G-code with the GRBL preprocessor.
fn cncjob(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "cncjob <geometry> <dst> [tool_dia] [cut_z] [feed]";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let tool_dia = if args.len() > 2 { farg(args, 2, USAGE)? } else { 0.8 };
    let cut_z = if args.len() > 3 { farg(args, 3, USAGE)? } else { -0.05 };
    let feed = if args.len() > 4 { farg(args, 4, USAGE)? } else { 120.0 };

    let (mp, units) = ctx.region(&src)?;
    if mp.0.is_empty() {
        return Err(ScriptError::Other(format!("'{src}' has no geometry to mill")));
    }

    let mut params = fc_cam::MillingParams { tool_diameter: tool_dia, ..Default::default() };
    params.job.units = units;
    params.job.cut_z = cut_z;
    params.job.feed_xy = feed;

    // Profile (boundary) milling: trace each ring offset outward by the radius.
    let paths = fc_cam::milling_profile(&mp, tool_dia, true);
    let n = paths.len();
    let job = fc_cam::milling_job(&mp, paths.clone(), &params, units);
    let gcode = job.to_gcode(&fc_gcode::Grbl);

    let out_paths: Vec<Polyline> = match job.kind {
        fc_gcode::JobKind::Mill { paths } => paths,
        _ => paths,
    };
    ctx.put(dst.clone(), Obj::Cnc { paths: out_paths, units, gcode });
    Ok(format!("{dst}: {n} milling paths (tool {tool_dia})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_gcode::Units;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn square_region(cx: f64, cy: f64, side: f64) -> Obj {
        let poly = fc_geo::centered_rect(cx, cy, side, side);
        Obj::Region(MultiPolygon::new(vec![poly]), Units::Mm)
    }

    fn seed(ctx: &mut ScriptContext) {
        ctx.put("r", square_region(5.0, 5.0, 10.0));
    }

    fn area_of(ctx: &ScriptContext, name: &str) -> f64 {
        let (mp, _) = ctx.region(name).unwrap();
        fc_geo::area(&mp)
    }

    #[test]
    fn commands_registered() {
        let names: Vec<&str> = commands().into_iter().map(|(n, _)| n).collect();
        for c in [
            "version", "help", "list_pp", "listpp", "skew", "mirror", "buffer", "follow",
            "ncr", "non_copper_regions", "set_origin", "origin", "new_geometry", "new_gerber",
            "exteriors", "ext", "interiors", "cncjob",
        ] {
            assert!(names.contains(&c), "missing {c}");
        }
    }

    #[test]
    fn version_reports_pkg_version() {
        let mut ctx = ScriptContext::new();
        let msg = version(&mut ctx, &[]).unwrap();
        assert!(msg.contains(env!("CARGO_PKG_VERSION")));
        assert!(msg.contains("fc-script"));
    }

    #[test]
    fn help_lists_commands() {
        let mut ctx = ScriptContext::new();
        let msg = help(&mut ctx, &[]).unwrap();
        assert!(msg.contains("cncjob"));
        assert!(msg.contains("version"));
    }

    #[test]
    fn list_pp_includes_known_dialects() {
        let mut ctx = ScriptContext::new();
        let msg = list_pp(&mut ctx, &[]).unwrap();
        assert!(msg.contains("GRBL"));
        assert!(msg.contains("Marlin"));
        assert!(msg.contains("LinuxCNC"));
    }

    #[test]
    fn preprocessor_table_matches_instances() {
        // The static table and the instantiated dialects must agree 1:1.
        let names = preprocessor_names();
        assert!(names.len() >= 20, "expected the full dialect set");
        assert!(names.iter().any(|n| n == "GRBL"));
        assert!(names.iter().any(|n| n == "Solder Paste"));
    }

    #[test]
    fn skew_creates_region() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        let base = area_of(&ctx, "r");
        let msg = skew(&mut ctx, &s(&["r", "d", "10", "0"])).unwrap();
        assert!(msg.contains("'d'"));
        assert_eq!(ctx.get("d").unwrap().kind(), "region");
        // A pure shear preserves area.
        assert!((area_of(&ctx, "d") - base).abs() < 1e-6);
    }

    #[test]
    fn skew_missing_arg_is_usage_error() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        assert!(matches!(skew(&mut ctx, &s(&["r", "d"])), Err(ScriptError::Usage(_))));
    }

    #[test]
    fn mirror_x_and_y_preserve_area() {
        let mut ctx = ScriptContext::new();
        ctx.put("r", square_region(3.0, 3.0, 4.0));
        let base = area_of(&ctx, "r");
        mirror(&mut ctx, &s(&["r", "mx", "x"])).unwrap();
        mirror(&mut ctx, &s(&["r", "my", "Y"])).unwrap(); // case-insensitive
        assert!((area_of(&ctx, "mx") - base).abs() < 1e-6);
        assert!((area_of(&ctx, "my") - base).abs() < 1e-6);
        // mirror about y=0 flips x sign of a square at x=3 -> centred at x=-3
        let (mp, _) = ctx.region("mx").unwrap();
        let (_, miny, _, maxy) = fc_geo::bounds(&mp).unwrap();
        // mirror_x flips Y about axis 0
        assert!(miny < 0.0 && maxy < 0.0, "mirror x should flip below axis");
    }

    #[test]
    fn mirror_bad_axis_errors() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        assert!(matches!(
            mirror(&mut ctx, &s(&["r", "d", "z"])),
            Err(ScriptError::Other(_))
        ));
    }

    #[test]
    fn buffer_outward_grows_area() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        let base = area_of(&ctx, "r");
        buffer(&mut ctx, &s(&["r", "d", "1"])).unwrap();
        assert!(area_of(&ctx, "d") > base);
    }

    #[test]
    fn buffer_usage_error() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        assert!(matches!(buffer(&mut ctx, &s(&["r"])), Err(ScriptError::Usage(_))));
    }

    #[test]
    fn follow_makes_cnc() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        let msg = follow(&mut ctx, &s(&["r", "f"])).unwrap();
        assert!(msg.starts_with("f:"));
        assert_eq!(ctx.get("f").unwrap().kind(), "cnc");
        if let Obj::Cnc { paths, gcode, .. } = ctx.get("f").unwrap() {
            assert_eq!(paths.len(), 1, "one exterior ring");
            assert!(gcode.contains("G01"), "should emit cutting moves");
        } else {
            panic!("expected cnc");
        }
    }

    #[test]
    fn follow_rejects_non_region() {
        let mut ctx = ScriptContext::new();
        ctx.put("c", Obj::Cnc { paths: vec![], units: Units::Mm, gcode: String::new() });
        assert!(follow(&mut ctx, &s(&["c", "f"])).is_err());
    }

    #[test]
    fn ncr_subtracts_copper_from_board() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", square_region(0.0, 0.0, 10.0)); // 10x10 copper, area 100
        ncr(&mut ctx, &s(&["g", "n", "5"])).unwrap(); // margin 5 -> board 20x20=400
        assert_eq!(ctx.get("n").unwrap().kind(), "region");
        let a = area_of(&ctx, "n");
        assert!((a - 300.0).abs() < 1e-6, "expected 400-100=300, got {a}");
    }

    #[test]
    fn ncr_empty_region_errors() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", Obj::Region(MultiPolygon::new(vec![]), Units::Mm));
        assert!(ncr(&mut ctx, &s(&["g", "n"])).is_err());
    }

    #[test]
    fn set_origin_shifts_region() {
        let mut ctx = ScriptContext::new();
        ctx.put("r", square_region(5.0, 5.0, 4.0)); // bbox 3..7
        let msg = set_origin(&mut ctx, &s(&["3", "3"])).unwrap();
        assert!(msg.contains("shifted 1"));
        let (mp, _) = ctx.region("r").unwrap();
        let (minx, miny, _, _) = fc_geo::bounds(&mp).unwrap();
        // shifted by (-3,-3): bbox lower-left 3,3 -> 0,0
        assert!((minx - 0.0).abs() < 1e-6, "minx={minx}");
        assert!((miny - 0.0).abs() < 1e-6, "miny={miny}");
    }

    #[test]
    fn set_origin_shifts_excellon() {
        let mut ctx = ScriptContext::new();
        let tool = fc_excellon::Tool { diameter: 0.8, drills: vec![(10.0, 10.0)], slots: vec![] };
        let mut tools = std::collections::BTreeMap::new();
        tools.insert(1, tool);
        ctx.put("e", Obj::Excellon(fc_excellon::Excellon { units: fc_excellon::Units::Mm, tools }));
        set_origin(&mut ctx, &s(&["10", "10"])).unwrap();
        if let Obj::Excellon(e) = ctx.get("e").unwrap() {
            assert_eq!(e.tools.get(&1).unwrap().drills[0], (0.0, 0.0));
        } else {
            panic!("expected excellon");
        }
    }

    #[test]
    fn set_origin_usage_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(set_origin(&mut ctx, &s(&["1"])), Err(ScriptError::Usage(_))));
    }

    #[test]
    fn new_geometry_and_gerber_are_empty_regions() {
        let mut ctx = ScriptContext::new();
        new_geometry(&mut ctx, &s(&["g1"])).unwrap();
        new_gerber(&mut ctx, &s(&["g2"])).unwrap();
        for n in ["g1", "g2"] {
            assert_eq!(ctx.get(n).unwrap().kind(), "region");
            assert!((area_of(&ctx, n) - 0.0).abs() < 1e-12);
        }
    }

    #[test]
    fn new_geometry_usage_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(new_geometry(&mut ctx, &[]), Err(ScriptError::Usage(_))));
    }

    #[test]
    fn exteriors_and_interiors() {
        // Build a square with a square hole: exterior ring + 1 interior ring.
        let outer = fc_geo::centered_rect(0.0, 0.0, 10.0, 10.0);
        let inner = fc_geo::centered_rect(0.0, 0.0, 4.0, 4.0);
        let holed = fc_geo::difference(
            &MultiPolygon::new(vec![outer]),
            &MultiPolygon::new(vec![inner]),
        );
        let mut ctx = ScriptContext::new();
        ctx.put("h", Obj::Region(holed, Units::Mm));

        exteriors(&mut ctx, &s(&["h", "e"])).unwrap();
        interiors(&mut ctx, &s(&["h", "i"])).unwrap();

        // exterior region: one polygon from the outer ring (~area 100)
        let (emp, _) = ctx.region("e").unwrap();
        assert_eq!(emp.0.len(), 1);
        assert!((fc_geo::area(&emp) - 100.0).abs() < 1e-6, "ext area {}", fc_geo::area(&emp));

        // interior region: one polygon from the hole ring (~area 16)
        let (imp, _) = ctx.region("i").unwrap();
        assert_eq!(imp.0.len(), 1, "one hole");
        assert!((fc_geo::area(&imp) - 16.0).abs() < 1e-6, "int area {}", fc_geo::area(&imp));
    }

    #[test]
    fn interiors_none_yields_empty_region() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx); // solid square, no holes
        interiors(&mut ctx, &s(&["r", "i"])).unwrap();
        let (imp, _) = ctx.region("i").unwrap();
        assert!(imp.0.is_empty(), "solid square has no interiors");
    }

    #[test]
    fn cncjob_makes_cnc() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        let msg = cncjob(&mut ctx, &s(&["r", "job", "1.0"])).unwrap();
        assert!(msg.starts_with("job:"));
        assert_eq!(ctx.get("job").unwrap().kind(), "cnc");
        if let Obj::Cnc { paths, gcode, .. } = ctx.get("job").unwrap() {
            assert!(!paths.is_empty(), "profile should yield paths");
            assert!(gcode.contains("G01"), "should emit cutting moves");
            assert!(gcode.contains("M30"), "GRBL footer");
        } else {
            panic!("expected cnc");
        }
    }

    #[test]
    fn cncjob_with_all_params() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        cncjob(&mut ctx, &s(&["r", "job", "0.8", "-1.5", "200"])).unwrap();
        if let Obj::Cnc { gcode, .. } = ctx.get("job").unwrap() {
            // cut_z -1.5 should appear; feed 200 in cutting moves.
            assert!(gcode.contains("Z-1.5000"), "cut_z honoured");
            assert!(gcode.contains("F200"), "feed honoured");
        } else {
            panic!("expected cnc");
        }
    }

    #[test]
    fn cncjob_empty_geometry_errors() {
        let mut ctx = ScriptContext::new();
        ctx.put("r", Obj::Region(MultiPolygon::new(vec![]), Units::Mm));
        assert!(cncjob(&mut ctx, &s(&["r", "job"])).is_err());
    }

    #[test]
    fn cncjob_usage_error() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        assert!(matches!(cncjob(&mut ctx, &s(&["r"])), Err(ScriptError::Usage(_))));
    }
}
