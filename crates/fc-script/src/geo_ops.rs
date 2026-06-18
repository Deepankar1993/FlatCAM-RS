//! `geo_ops` — geometry transformation/boolean commands for the script engine.
//!
//! Parity with FlatCAM's Tcl geometry helpers: every command here reads one or
//! two [`crate::Obj::Region`] objects, applies a Shapely-style operation, and
//! stores the result as a new Region carrying the source's [`fc_gcode::Units`].

use crate::{farg, sarg, Obj, ScriptContext, ScriptError};
use fc_geo::transform;

/// Register the geometry-operation commands.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("offset", offset),
        ("scale", scale),
        ("translate", translate),
        ("mirror_x", mirror_x),
        ("subtract", subtract),
        ("union", union),
    ]
}

fn offset(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "offset <src> <dst> <distance>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let distance = farg(args, 2, USAGE)?;
    let (mp, units) = ctx.region(&src)?;
    let out = fc_geo::offset(&mp, distance);
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("offset '{src}' by {distance} -> '{dst}'"))
}

fn scale(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "scale <src> <dst> <factor>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let factor = farg(args, 2, USAGE)?;
    let (mp, units) = ctx.region(&src)?;
    let out = transform::scale(&mp, factor, factor, (0.0, 0.0));
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("scaled '{src}' by {factor} -> '{dst}'"))
}

fn translate(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "translate <src> <dst> <dx> <dy>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let dx = farg(args, 2, USAGE)?;
    let dy = farg(args, 3, USAGE)?;
    let (mp, units) = ctx.region(&src)?;
    let out = transform::translate(&mp, dx, dy);
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("translated '{src}' by ({dx}, {dy}) -> '{dst}'"))
}

fn mirror_x(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "mirror_x <src> <dst> <axis>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let axis = farg(args, 2, USAGE)?;
    let (mp, units) = ctx.region(&src)?;
    let out = transform::mirror_x(&mp, axis);
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("mirrored '{src}' about x={axis} -> '{dst}'"))
}

fn subtract(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "subtract <a> <b> <dst>";
    let a = sarg(args, 0, USAGE)?.to_string();
    let b = sarg(args, 1, USAGE)?.to_string();
    let dst = sarg(args, 2, USAGE)?.to_string();
    let (mp_a, units) = ctx.region(&a)?;
    let (mp_b, _) = ctx.region(&b)?;
    let out = fc_geo::difference(&mp_a, &mp_b);
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("subtracted '{b}' from '{a}' -> '{dst}'"))
}

fn union(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "union <a> <b> <dst>";
    let a = sarg(args, 0, USAGE)?.to_string();
    let b = sarg(args, 1, USAGE)?.to_string();
    let dst = sarg(args, 2, USAGE)?.to_string();
    let (mp_a, units) = ctx.region(&a)?;
    let (mp_b, _) = ctx.region(&b)?;
    let out = fc_geo::union(&mp_a, &mp_b);
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("union of '{a}' and '{b}' -> '{dst}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_gcode::Units;
    use fc_geo::MultiPolygon;

    /// Helper: build a Region object from a centered axis-aligned square.
    fn square(cx: f64, cy: f64, side: f64) -> Obj {
        let poly = fc_geo::centered_rect(cx, cy, side, side);
        Obj::Region(MultiPolygon::new(vec![poly]), Units::Mm)
    }

    fn area_of(ctx: &ScriptContext, name: &str) -> f64 {
        let (mp, _) = ctx.region(name).unwrap();
        fc_geo::area(&mp)
    }

    #[test]
    fn subtract_4x4_minus_centered_2x2_is_12() {
        let mut ctx = ScriptContext::new();
        ctx.put("a", square(0.0, 0.0, 4.0)); // area 16
        ctx.put("b", square(0.0, 0.0, 2.0)); // area 4, fully inside a
        let msg = subtract(
            &mut ctx,
            &["a".into(), "b".into(), "d".into()],
        )
        .unwrap();
        assert!(msg.contains("'d'"));
        let area = area_of(&ctx, "d");
        assert!((area - 12.0).abs() < 1e-6, "expected 12, got {area}");
    }

    #[test]
    fn union_area_at_least_each_input() {
        let mut ctx = ScriptContext::new();
        ctx.put("a", square(0.0, 0.0, 2.0)); // area 4
        ctx.put("b", square(1.0, 0.0, 2.0)); // area 4, overlaps a
        let area_a = area_of(&ctx, "a");
        let area_b = area_of(&ctx, "b");
        union(&mut ctx, &["a".into(), "b".into(), "u".into()]).unwrap();
        let area_u = area_of(&ctx, "u");
        assert!(area_u >= area_a - 1e-9, "union < a");
        assert!(area_u >= area_b - 1e-9, "union < b");
        // Overlapping squares: union strictly less than the sum.
        assert!(area_u < area_a + area_b - 1e-9);
    }

    #[test]
    fn union_disjoint_sums_areas() {
        let mut ctx = ScriptContext::new();
        ctx.put("a", square(0.0, 0.0, 2.0)); // area 4
        ctx.put("b", square(10.0, 10.0, 2.0)); // area 4, disjoint
        union(&mut ctx, &["a".into(), "b".into(), "u".into()]).unwrap();
        let area_u = area_of(&ctx, "u");
        assert!((area_u - 8.0).abs() < 1e-6, "expected 8, got {area_u}");
    }

    #[test]
    fn scale_by_2_quadruples_area() {
        let mut ctx = ScriptContext::new();
        ctx.put("a", square(0.0, 0.0, 3.0)); // area 9
        let base = area_of(&ctx, "a");
        scale(&mut ctx, &["a".into(), "s".into(), "2".into()]).unwrap();
        let scaled = area_of(&ctx, "s");
        assert!((scaled - base * 4.0).abs() < 1e-6, "expected {}, got {scaled}", base * 4.0);
    }

    #[test]
    fn offset_outward_grows_area() {
        let mut ctx = ScriptContext::new();
        ctx.put("a", square(0.0, 0.0, 4.0)); // area 16
        let base = area_of(&ctx, "a");
        offset(&mut ctx, &["a".into(), "o".into(), "1".into()]).unwrap();
        let grown = area_of(&ctx, "o");
        assert!(grown > base, "offset out should grow area: {grown} <= {base}");
    }

    #[test]
    fn translate_preserves_area_and_moves_bounds() {
        let mut ctx = ScriptContext::new();
        ctx.put("a", square(0.0, 0.0, 2.0));
        let base = area_of(&ctx, "a");
        translate(
            &mut ctx,
            &["a".into(), "t".into(), "5".into(), "0".into()],
        )
        .unwrap();
        let (mp, _) = ctx.region("t").unwrap();
        assert!((fc_geo::area(&mp) - base).abs() < 1e-6);
        let (minx, _, maxx, _) = fc_geo::bounds(&mp).unwrap();
        assert!((minx - 4.0).abs() < 1e-6, "minx={minx}");
        assert!((maxx - 6.0).abs() < 1e-6, "maxx={maxx}");
    }

    #[test]
    fn mirror_x_preserves_area() {
        let mut ctx = ScriptContext::new();
        ctx.put("a", square(0.0, 3.0, 2.0));
        let base = area_of(&ctx, "a");
        mirror_x(&mut ctx, &["a".into(), "m".into(), "0".into()]).unwrap();
        let mirrored = area_of(&ctx, "m");
        assert!((mirrored - base).abs() < 1e-6);
    }

    #[test]
    fn output_region_keeps_source_units() {
        let mut ctx = ScriptContext::new();
        let poly = fc_geo::centered_rect(0.0, 0.0, 2.0, 2.0);
        ctx.put("a", Obj::Region(MultiPolygon::new(vec![poly]), Units::Inch));
        scale(&mut ctx, &["a".into(), "s".into(), "2".into()]).unwrap();
        match ctx.get("s").unwrap() {
            Obj::Region(_, u) => assert_eq!(*u, Units::Inch),
            _ => panic!("expected region"),
        }
    }

    #[test]
    fn missing_args_yield_usage_error() {
        let mut ctx = ScriptContext::new();
        let err = offset(&mut ctx, &["a".into()]).unwrap_err();
        assert!(matches!(err, ScriptError::Usage(_)));
    }

    #[test]
    fn non_region_source_errors() {
        let mut ctx = ScriptContext::new();
        ctx.put(
            "c",
            Obj::Cnc { paths: vec![], units: Units::Mm, gcode: String::new() },
        );
        let err = scale(&mut ctx, &["c".into(), "d".into(), "2".into()]).unwrap_err();
        assert!(matches!(err, ScriptError::Other(_)));
    }

    #[test]
    fn commands_registered() {
        let names: Vec<&str> = commands().into_iter().map(|(n, _)| n).collect();
        for c in ["offset", "scale", "translate", "mirror_x", "subtract", "union"] {
            assert!(names.contains(&c), "missing {c}");
        }
    }
}
