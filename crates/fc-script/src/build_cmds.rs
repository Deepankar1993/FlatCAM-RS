//! `build_cmds` command group — object construction and geometry editing.
//!
//! Parity with FlatCAM's interactive/Tcl geometry-building commands: adding
//! primitives (`add_circle`, `add_polygon`, `add_polyline`, `add_rectangle`) to
//! a geometry object, subtracting shapes (`subtract_poly`,
//! `subtract_rectangle`), drilling (`add_drill`, `add_slot`), unioning
//! (`geo_union`), milling holes/slots (`milldrills`, `millslots`), and joining
//! objects (`join_geometries`, `join_excellon`).
//!
//! Objects are created on first reference where it makes sense (`add_circle`
//! etc. create an empty Region/Excellon if the target is missing), matching
//! upstream "draw into a (possibly new) geometry object" behaviour.

use crate::{farg, sarg, Obj, ScriptContext, ScriptError};
use fc_excellon::{Excellon, Tool};
use fc_gcode::{Polyline, Units};
use fc_geo::{circle, union, Coord, LineString, MultiPolygon, Polygon};

const STEPS: usize = 64;

/// Fetch the target Region's geometry+units, creating an empty mm Region when
/// the object does not yet exist. Errors if the object exists but is not a
/// region.
fn region_or_new(ctx: &ScriptContext, name: &str) -> Result<(MultiPolygon<f64>, Units), ScriptError> {
    match ctx.objects.get(name) {
        None => Ok((MultiPolygon::new(vec![]), Units::Mm)),
        Some(Obj::Region(mp, u)) => Ok((mp.clone(), *u)),
        Some(other) => Err(ScriptError::Other(format!(
            "{name} is a {}, expected region",
            other.kind()
        ))),
    }
}

/// Parse a flat list of `x y x y …` coordinate args (starting at `from`) into a
/// vector of points. Errors on an odd count or a non-numeric token.
fn parse_coord_pairs(args: &[String], from: usize, usage: &str) -> Result<Vec<Coord<f64>>, ScriptError> {
    let coords = &args[from.min(args.len())..];
    if coords.is_empty() || coords.len() % 2 != 0 {
        return Err(ScriptError::Usage(usage.to_string()));
    }
    let mut pts = Vec::with_capacity(coords.len() / 2);
    let mut i = 0;
    while i < coords.len() {
        let x = farg(args, from + i, usage)?;
        let y = farg(args, from + i + 1, usage)?;
        pts.push(Coord { x, y });
        i += 2;
    }
    Ok(pts)
}

/// Insert a polygon into the named Region, creating it if missing.
fn add_polygon_to(ctx: &mut ScriptContext, name: &str, poly: Polygon<f64>) -> Result<(), ScriptError> {
    let (mut mp, units) = region_or_new(ctx, name)?;
    mp.0.push(poly);
    ctx.put(name.to_string(), Obj::Region(mp, units));
    Ok(())
}

/// `add_circle <obj> <x> <y> <radius>` — add a circle to a Region (create if missing).
fn add_circle(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "add_circle <obj> <x> <y> <radius>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let x = farg(args, 1, USAGE)?;
    let y = farg(args, 2, USAGE)?;
    let r = farg(args, 3, USAGE)?;
    if r <= 0.0 {
        return Err(ScriptError::Other("radius must be positive".into()));
    }
    add_polygon_to(ctx, &name, circle(x, y, r, STEPS))?;
    Ok(format!("added circle to '{name}'"))
}

/// `add_polygon` / `add_poly <obj> <x1> <y1> <x2> <y2> …` — add a closed polygon.
fn add_polygon(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "add_polygon <obj> <x1> <y1> <x2> <y2> ...";
    let name = sarg(args, 0, USAGE)?.to_string();
    let mut pts = parse_coord_pairs(args, 1, USAGE)?;
    if pts.len() < 3 {
        return Err(ScriptError::Other("a polygon needs at least 3 points".into()));
    }
    if pts.first() != pts.last() {
        pts.push(pts[0]);
    }
    let poly = Polygon::new(LineString::new(pts), vec![]);
    add_polygon_to(ctx, &name, poly)?;
    Ok(format!("added polygon to '{name}'"))
}

/// `add_polyline <obj> <x1> <y1> …` — add an open polyline (stored as a
/// degenerate, zero-area polygon ring so it lives in the Region object).
///
/// Region objects only hold polygons; an open polyline is kept as a polygon
/// whose ring is the line (not auto-closed), preserving the vertices for export.
fn add_polyline(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "add_polyline <obj> <x1> <y1> ...";
    let name = sarg(args, 0, USAGE)?.to_string();
    let pts = parse_coord_pairs(args, 1, USAGE)?;
    if pts.len() < 2 {
        return Err(ScriptError::Other("a polyline needs at least 2 points".into()));
    }
    let poly = Polygon::new(LineString::new(pts), vec![]);
    add_polygon_to(ctx, &name, poly)?;
    Ok(format!("added polyline to '{name}'"))
}

/// Build an axis-aligned rectangle polygon from two opposite corners.
fn rect_polygon(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon<f64> {
    let (lx, hx) = (x0.min(x1), x0.max(x1));
    let (ly, hy) = (y0.min(y1), y0.max(y1));
    let ring = vec![
        Coord { x: lx, y: ly },
        Coord { x: hx, y: ly },
        Coord { x: hx, y: hy },
        Coord { x: lx, y: hy },
        Coord { x: lx, y: ly },
    ];
    Polygon::new(LineString::new(ring), vec![])
}

/// `add_rect` / `add_rectangle <obj> <x0> <y0> <x1> <y1>` — add a rectangle.
fn add_rect(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "add_rect <obj> <x0> <y0> <x1> <y1>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let x0 = farg(args, 1, USAGE)?;
    let y0 = farg(args, 2, USAGE)?;
    let x1 = farg(args, 3, USAGE)?;
    let y1 = farg(args, 4, USAGE)?;
    add_polygon_to(ctx, &name, rect_polygon(x0, y0, x1, y1))?;
    Ok(format!("added rectangle to '{name}'"))
}

/// Subtract a tool MultiPolygon from an existing Region (must already exist).
fn subtract_from(ctx: &mut ScriptContext, name: &str, tool: MultiPolygon<f64>) -> Result<(), ScriptError> {
    let (mp, units) = ctx.region(name)?;
    let out = fc_geo::difference(&mp, &tool);
    ctx.put(name.to_string(), Obj::Region(out, units));
    Ok(())
}

/// `subtract_poly <obj> <x1> <y1> …` — subtract a polygon from a Region.
fn subtract_poly(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "subtract_poly <obj> <x1> <y1> ...";
    let name = sarg(args, 0, USAGE)?.to_string();
    let mut pts = parse_coord_pairs(args, 1, USAGE)?;
    if pts.len() < 3 {
        return Err(ScriptError::Other("a polygon needs at least 3 points".into()));
    }
    if pts.first() != pts.last() {
        pts.push(pts[0]);
    }
    let tool = MultiPolygon::new(vec![Polygon::new(LineString::new(pts), vec![])]);
    subtract_from(ctx, &name, tool)?;
    Ok(format!("subtracted polygon from '{name}'"))
}

/// `subtract_rectangle <obj> <x0> <y0> <x1> <y1>` — subtract a rectangle.
fn subtract_rectangle(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "subtract_rectangle <obj> <x0> <y0> <x1> <y1>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let x0 = farg(args, 1, USAGE)?;
    let y0 = farg(args, 2, USAGE)?;
    let x1 = farg(args, 3, USAGE)?;
    let y1 = farg(args, 4, USAGE)?;
    let tool = MultiPolygon::new(vec![rect_polygon(x0, y0, x1, y1)]);
    subtract_from(ctx, &name, tool)?;
    Ok(format!("subtracted rectangle from '{name}'"))
}

/// Fetch the target Excellon, creating an empty mm Excellon if missing. Errors
/// if the object exists but is not an excellon.
fn excellon_or_new(ctx: &ScriptContext, name: &str) -> Result<Excellon, ScriptError> {
    match ctx.objects.get(name) {
        None => Ok(Excellon { units: fc_excellon::Units::Mm, tools: std::collections::BTreeMap::new() }),
        Some(Obj::Excellon(e)) => Ok(e.clone()),
        Some(other) => Err(ScriptError::Other(format!(
            "{name} is a {}, expected excellon",
            other.kind()
        ))),
    }
}

/// Find an existing tool with matching diameter, or allocate the next tool
/// number. Returns the tool number.
fn tool_for_dia(e: &mut Excellon, dia: f64) -> i32 {
    for (&num, t) in e.tools.iter() {
        if (t.diameter - dia).abs() < 1e-9 {
            return num;
        }
    }
    let next = e.tools.keys().copied().max().unwrap_or(0) + 1;
    e.tools.insert(next, Tool { diameter: dia, drills: vec![], slots: vec![] });
    next
}

/// `add_drill <excobj> <x> <y> <dia>` — add a drill to an Excellon (create if missing).
fn add_drill(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "add_drill <excobj> <x> <y> <dia>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let x = farg(args, 1, USAGE)?;
    let y = farg(args, 2, USAGE)?;
    let dia = farg(args, 3, USAGE)?;
    if dia <= 0.0 {
        return Err(ScriptError::Other("drill diameter must be positive".into()));
    }
    let mut e = excellon_or_new(ctx, &name)?;
    let tool = tool_for_dia(&mut e, dia);
    e.tools.get_mut(&tool).unwrap().drills.push((x, y));
    ctx.put(name.clone(), Obj::Excellon(e));
    Ok(format!("added drill (dia {dia}) to '{name}'"))
}

/// `add_slot <excobj> <x0> <y0> <x1> <y1> <dia>` — add a slot.
fn add_slot(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "add_slot <excobj> <x0> <y0> <x1> <y1> <dia>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let x0 = farg(args, 1, USAGE)?;
    let y0 = farg(args, 2, USAGE)?;
    let x1 = farg(args, 3, USAGE)?;
    let y1 = farg(args, 4, USAGE)?;
    let dia = farg(args, 5, USAGE)?;
    if dia <= 0.0 {
        return Err(ScriptError::Other("slot diameter must be positive".into()));
    }
    let mut e = excellon_or_new(ctx, &name)?;
    let tool = tool_for_dia(&mut e, dia);
    e.tools.get_mut(&tool).unwrap().slots.push(((x0, y0), (x1, y1)));
    ctx.put(name.clone(), Obj::Excellon(e));
    Ok(format!("added slot (dia {dia}) to '{name}'"))
}

/// `geo_union <obj>` — union all polygons of a Region in place.
fn geo_union(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "geo_union <obj>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let (mp, units) = ctx.region(&name)?;
    let before = mp.0.len();
    let merged = fc_geo::union_all(mp.0);
    let after = merged.0.len();
    ctx.put(name.clone(), Obj::Region(merged, units));
    Ok(format!("union of '{name}': {before} -> {after} polygon(s)"))
}

/// Build a milling Cnc object from a set of paths.
fn mill_paths_to_cnc(ctx: &mut ScriptContext, dst: &str, paths: Vec<Polyline>, units: Units, tool_dia: f64) -> usize {
    let n = paths.len();
    let obj = crate::make_cnc(paths, units, tool_dia);
    ctx.put(dst.to_string(), obj);
    n
}

/// Collect every drill point of an Excellon as `(x, y)` pairs.
fn all_drills(e: &Excellon) -> Vec<(f64, f64)> {
    e.tools.values().flat_map(|t| t.drills.iter().copied()).collect()
}

/// Map the excellon unit enum onto the gcode unit enum.
fn exc_units(u: fc_excellon::Units) -> Units {
    match u {
        fc_excellon::Units::Mm => Units::Mm,
        fc_excellon::Units::Inch => Units::Inch,
    }
}

/// `milldrills` / `milld <excobj> <dst> <tooldia>` — mill all holes of an
/// Excellon into a Region (a milling Cnc job tracing each hole).
fn milldrills(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "milldrills <excobj> <dst> <tooldia>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let tool_dia = farg(args, 2, USAGE)?;
    let (points, units, hole_dia) = match ctx.get(&name)? {
        Obj::Excellon(e) => {
            // Use the largest tool diameter as the representative hole size.
            let hole = e.tools.values().map(|t| t.diameter).fold(0.0_f64, f64::max);
            (all_drills(e), exc_units(e.units), hole)
        }
        other => {
            return Err(ScriptError::Other(format!(
                "{name} is a {}, expected excellon",
                other.kind()
            )))
        }
    };
    if points.is_empty() {
        return Err(ScriptError::Other(format!("'{name}' has no drills to mill")));
    }
    let paths = fc_cam::mill_holes(&points, hole_dia, tool_dia, STEPS);
    let n = mill_paths_to_cnc(ctx, &dst, paths, units, tool_dia);
    Ok(format!("{dst}: milled {n} hole(s) (tool {tool_dia})"))
}

/// `millslots` / `mills <excobj> <dst> <tooldia>` — mill all slots of an
/// Excellon into a milling Cnc job (each slot traced as an open path).
fn millslots(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "millslots <excobj> <dst> <tooldia>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let tool_dia = farg(args, 2, USAGE)?;
    let (slots, units) = match ctx.get(&name)? {
        Obj::Excellon(e) => {
            let slots: Vec<((f64, f64), (f64, f64))> =
                e.tools.values().flat_map(|t| t.slots.iter().copied()).collect();
            (slots, exc_units(e.units))
        }
        other => {
            return Err(ScriptError::Other(format!(
                "{name} is a {}, expected excellon",
                other.kind()
            )))
        }
    };
    if slots.is_empty() {
        return Err(ScriptError::Other(format!("'{name}' has no slots to mill")));
    }
    // Each slot is milled as a straight centre-line path between its endpoints.
    let paths: Vec<Polyline> = slots.iter().map(|&(a, b)| vec![a, b]).collect();
    let n = mill_paths_to_cnc(ctx, &dst, paths, units, tool_dia);
    Ok(format!("{dst}: milled {n} slot(s) (tool {tool_dia})"))
}

/// `join_geometries` / `join_geometry <dst> <src1> <src2> …` — merge Regions
/// (union their geometry) into a single Region object.
fn join_geometries(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "join_geometries <dst> <src1> <src2> ...";
    let dst = sarg(args, 0, USAGE)?.to_string();
    if args.len() < 2 {
        return Err(ScriptError::Usage(USAGE.to_string()));
    }
    let mut acc = MultiPolygon::new(vec![]);
    let mut units = Units::Mm;
    let mut first = true;
    for src in &args[1..] {
        let (mp, u) = ctx.region(src)?;
        if first {
            units = u;
            first = false;
        }
        acc = union(&acc, &mp);
    }
    let n = args.len() - 1;
    ctx.put(dst.clone(), Obj::Region(acc, units));
    Ok(format!("joined {n} geometr(ies) -> '{dst}'"))
}

/// `join_excellon` / `join_excellons <dst> <src1> <src2> …` — merge Excellon
/// objects (concatenate tools/drills, merging tools of equal diameter).
fn join_excellon(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "join_excellon <dst> <src1> <src2> ...";
    let dst = sarg(args, 0, USAGE)?.to_string();
    if args.len() < 2 {
        return Err(ScriptError::Usage(USAGE.to_string()));
    }
    let mut acc = Excellon { units: fc_excellon::Units::Mm, tools: std::collections::BTreeMap::new() };
    let mut first = true;
    for src in &args[1..] {
        match ctx.get(src)? {
            Obj::Excellon(e) => {
                if first {
                    acc.units = e.units;
                    first = false;
                }
                for t in e.tools.values() {
                    let num = tool_for_dia(&mut acc, t.diameter);
                    let dst_tool = acc.tools.get_mut(&num).unwrap();
                    dst_tool.drills.extend(t.drills.iter().copied());
                    dst_tool.slots.extend(t.slots.iter().copied());
                }
            }
            other => {
                return Err(ScriptError::Other(format!(
                    "{src} is a {}, expected excellon",
                    other.kind()
                )))
            }
        }
    }
    let n = args.len() - 1;
    let drills = acc.drill_count();
    ctx.put(dst.clone(), Obj::Excellon(acc));
    Ok(format!("joined {n} excellon(s) ({drills} drills) -> '{dst}'"))
}

/// Register the `build_cmds` command group.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("add_circle", add_circle),
        ("add_polygon", add_polygon),
        ("add_poly", add_polygon),
        ("add_polyline", add_polyline),
        ("add_rect", add_rect),
        ("add_rectangle", add_rect),
        ("subtract_poly", subtract_poly),
        ("subtract_rectangle", subtract_rectangle),
        ("add_drill", add_drill),
        ("add_slot", add_slot),
        ("geo_union", geo_union),
        ("milldrills", milldrills),
        ("milld", milldrills),
        ("millslots", millslots),
        ("mills", millslots),
        ("join_geometries", join_geometries),
        ("join_geometry", join_geometries),
        ("join_excellon", join_excellon),
        ("join_excellons", join_excellon),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn area_of(ctx: &ScriptContext, name: &str) -> f64 {
        let (mp, _) = ctx.region(name).unwrap();
        fc_geo::area(&mp)
    }

    #[test]
    fn commands_registered() {
        let names: Vec<&str> = commands().into_iter().map(|(n, _)| n).collect();
        for c in [
            "add_circle", "add_polygon", "add_poly", "add_polyline", "add_rect", "add_rectangle",
            "subtract_poly", "subtract_rectangle", "add_drill", "add_slot", "geo_union",
            "milldrills", "milld", "millslots", "mills", "join_geometries", "join_geometry",
            "join_excellon", "join_excellons",
        ] {
            assert!(names.contains(&c), "missing {c}");
        }
    }

    #[test]
    fn add_circle_creates_region() {
        let mut ctx = ScriptContext::new();
        let msg = add_circle(&mut ctx, &s(&["g", "0", "0", "2"])).unwrap();
        assert!(msg.contains("'g'"));
        assert_eq!(ctx.get("g").unwrap().kind(), "region");
        assert!((area_of(&ctx, "g") - std::f64::consts::PI * 4.0).abs() < 0.05);
    }

    #[test]
    fn add_circle_negative_radius_errors() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(add_circle(&mut ctx, &s(&["g", "0", "0", "-1"])), Err(ScriptError::Other(_))));
    }

    #[test]
    fn add_polygon_triangle_area() {
        let mut ctx = ScriptContext::new();
        add_polygon(&mut ctx, &s(&["g", "0", "0", "10", "0", "0", "10"])).unwrap();
        assert!((area_of(&ctx, "g") - 50.0).abs() < 1e-6);
    }

    #[test]
    fn add_polygon_odd_coords_usage_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(
            add_polygon(&mut ctx, &s(&["g", "0", "0", "10"])),
            Err(ScriptError::Usage(_))
        ));
    }

    #[test]
    fn add_polyline_keeps_points() {
        let mut ctx = ScriptContext::new();
        add_polyline(&mut ctx, &s(&["g", "0", "0", "5", "0", "5", "5"])).unwrap();
        let (mp, _) = ctx.region("g").unwrap();
        assert_eq!(mp.0.len(), 1);
        assert!(mp.0[0].exterior().0.len() >= 3);
    }

    #[test]
    fn add_rect_and_subtract_rect() {
        let mut ctx = ScriptContext::new();
        add_rect(&mut ctx, &s(&["g", "0", "0", "10", "10"])).unwrap();
        assert!((area_of(&ctx, "g") - 100.0).abs() < 1e-6);
        subtract_rectangle(&mut ctx, &s(&["g", "2", "2", "6", "6"])).unwrap();
        // 100 - 16 = 84
        assert!((area_of(&ctx, "g") - 84.0).abs() < 1e-6);
    }

    #[test]
    fn subtract_poly_reduces_area() {
        let mut ctx = ScriptContext::new();
        add_rect(&mut ctx, &s(&["g", "0", "0", "10", "10"])).unwrap();
        subtract_poly(&mut ctx, &s(&["g", "0", "0", "5", "0", "5", "5", "0", "5"])).unwrap();
        // remove a 5x5 square = 25
        assert!((area_of(&ctx, "g") - 75.0).abs() < 1e-6);
    }

    #[test]
    fn subtract_rectangle_missing_object_errors() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(
            subtract_rectangle(&mut ctx, &s(&["nope", "0", "0", "1", "1"])),
            Err(ScriptError::NotFound(_))
        ));
    }

    #[test]
    fn add_drill_creates_excellon() {
        let mut ctx = ScriptContext::new();
        add_drill(&mut ctx, &s(&["e", "10", "10", "0.8"])).unwrap();
        add_drill(&mut ctx, &s(&["e", "20", "10", "0.8"])).unwrap();
        add_drill(&mut ctx, &s(&["e", "30", "30", "1.2"])).unwrap();
        if let Obj::Excellon(e) = ctx.get("e").unwrap() {
            assert_eq!(e.tools.len(), 2, "two distinct diameters");
            assert_eq!(e.drill_count(), 3);
        } else {
            panic!("expected excellon");
        }
    }

    #[test]
    fn add_drill_bad_dia_errors() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(add_drill(&mut ctx, &s(&["e", "0", "0", "0"])), Err(ScriptError::Other(_))));
    }

    #[test]
    fn add_slot_records_slot() {
        let mut ctx = ScriptContext::new();
        add_slot(&mut ctx, &s(&["e", "0", "0", "10", "0", "1.0"])).unwrap();
        if let Obj::Excellon(e) = ctx.get("e").unwrap() {
            let total_slots: usize = e.tools.values().map(|t| t.slots.len()).sum();
            assert_eq!(total_slots, 1);
        } else {
            panic!("expected excellon");
        }
    }

    #[test]
    fn geo_union_merges_overlapping() {
        let mut ctx = ScriptContext::new();
        add_rect(&mut ctx, &s(&["g", "0", "0", "10", "10"])).unwrap();
        add_rect(&mut ctx, &s(&["g", "5", "0", "15", "10"])).unwrap();
        let (mp_before, _) = ctx.region("g").unwrap();
        assert_eq!(mp_before.0.len(), 2);
        geo_union(&mut ctx, &s(&["g"])).unwrap();
        let (mp_after, _) = ctx.region("g").unwrap();
        assert_eq!(mp_after.0.len(), 1, "overlapping rects union to one");
        // union area = 15 wide x 10 tall = 150
        assert!((area_of(&ctx, "g") - 150.0).abs() < 1e-6);
    }

    #[test]
    fn geo_union_rejects_non_region() {
        let mut ctx = ScriptContext::new();
        ctx.put("c", Obj::Cnc { paths: vec![], units: Units::Mm, gcode: String::new() });
        assert!(geo_union(&mut ctx, &s(&["c"])).is_err());
    }

    #[test]
    fn milldrills_makes_cnc() {
        let mut ctx = ScriptContext::new();
        add_drill(&mut ctx, &s(&["e", "0", "0", "3.0"])).unwrap();
        add_drill(&mut ctx, &s(&["e", "10", "0", "3.0"])).unwrap();
        let msg = milldrills(&mut ctx, &s(&["e", "m", "1.0"])).unwrap();
        assert!(msg.contains("milled 2 hole"));
        assert_eq!(ctx.get("m").unwrap().kind(), "cnc");
        if let Obj::Cnc { paths, gcode, .. } = ctx.get("m").unwrap() {
            assert_eq!(paths.len(), 2);
            assert!(gcode.contains("G01"));
        } else {
            panic!("expected cnc");
        }
    }

    #[test]
    fn milldrills_no_drills_errors() {
        let mut ctx = ScriptContext::new();
        ctx.put("e", Obj::Excellon(Excellon { units: fc_excellon::Units::Mm, tools: std::collections::BTreeMap::new() }));
        assert!(milldrills(&mut ctx, &s(&["e", "m", "1.0"])).is_err());
    }

    #[test]
    fn millslots_makes_cnc() {
        let mut ctx = ScriptContext::new();
        add_slot(&mut ctx, &s(&["e", "0", "0", "10", "0", "1.0"])).unwrap();
        let msg = millslots(&mut ctx, &s(&["e", "m", "1.0"])).unwrap();
        assert!(msg.contains("milled 1 slot"));
        if let Obj::Cnc { paths, .. } = ctx.get("m").unwrap() {
            assert_eq!(paths.len(), 1);
            assert_eq!(paths[0].len(), 2);
        } else {
            panic!("expected cnc");
        }
    }

    #[test]
    fn millslots_no_slots_errors() {
        let mut ctx = ScriptContext::new();
        add_drill(&mut ctx, &s(&["e", "0", "0", "1.0"])).unwrap();
        assert!(millslots(&mut ctx, &s(&["e", "m", "1.0"])).is_err());
    }

    #[test]
    fn join_geometries_unions_two_regions() {
        let mut ctx = ScriptContext::new();
        add_rect(&mut ctx, &s(&["a", "0", "0", "10", "10"])).unwrap();
        add_rect(&mut ctx, &s(&["b", "20", "0", "30", "10"])).unwrap();
        let msg = join_geometries(&mut ctx, &s(&["j", "a", "b"])).unwrap();
        assert!(msg.contains("'j'"));
        // disjoint rects => 100 + 100 = 200
        assert!((area_of(&ctx, "j") - 200.0).abs() < 1e-6);
    }

    #[test]
    fn join_geometries_usage_error() {
        let mut ctx = ScriptContext::new();
        add_rect(&mut ctx, &s(&["a", "0", "0", "10", "10"])).unwrap();
        assert!(matches!(join_geometries(&mut ctx, &s(&["j"])), Err(ScriptError::Usage(_))));
    }

    #[test]
    fn join_excellon_merges_drills() {
        let mut ctx = ScriptContext::new();
        add_drill(&mut ctx, &s(&["a", "0", "0", "0.8"])).unwrap();
        add_drill(&mut ctx, &s(&["b", "10", "0", "0.8"])).unwrap();
        add_drill(&mut ctx, &s(&["b", "20", "0", "1.2"])).unwrap();
        let msg = join_excellon(&mut ctx, &s(&["j", "a", "b"])).unwrap();
        assert!(msg.contains("3 drills"));
        if let Obj::Excellon(e) = ctx.get("j").unwrap() {
            assert_eq!(e.drill_count(), 3);
            // 0.8 tool merged, 1.2 separate => 2 tools
            assert_eq!(e.tools.len(), 2);
        } else {
            panic!("expected excellon");
        }
    }

    #[test]
    fn join_excellon_rejects_region() {
        let mut ctx = ScriptContext::new();
        add_drill(&mut ctx, &s(&["a", "0", "0", "0.8"])).unwrap();
        add_rect(&mut ctx, &s(&["b", "0", "0", "1", "1"])).unwrap();
        assert!(matches!(join_excellon(&mut ctx, &s(&["j", "a", "b"])), Err(ScriptError::Other(_))));
    }
}
