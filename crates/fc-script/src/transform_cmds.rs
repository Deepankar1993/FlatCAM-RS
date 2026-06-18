//! `transform_cmds` — array/panelize/transform script commands.
//!
//! Parity with FlatCAM's Tcl transform helpers (rotate / mirror / array /
//! panelize). Every command reads one [`crate::Obj::Region`], applies a
//! Shapely-style placement or transform, and stores the result as a new
//! Region carrying the source's [`fc_gcode::Units`].
//!
//! Note: the linear `array` command is implemented directly over the
//! `fc_geo` primitives (translate + union) rather than depending on an
//! `fc_cam::array` helper, so the command group is self-contained.

use crate::{farg, iarg, sarg, Obj, ScriptContext, ScriptError};
use fc_geo::{transform, MultiPolygon, Polygon};

/// Register the transform/array/panelize commands.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("rotate", rotate),
        ("mirror_y", mirror_y),
        ("array", array),
        ("panelize", panelize),
    ]
}

/// `rotate <src> <dst> <deg>` — rotate a region about the origin.
fn rotate(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "rotate <src> <dst> <deg>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let deg = farg(args, 2, USAGE)?;
    let (mp, units) = ctx.region(&src)?;
    let out = transform::rotate(&mp, deg, (0.0, 0.0));
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("rotated '{src}' by {deg} deg -> '{dst}'"))
}

/// `mirror_y <src> <dst> <axis>` — mirror across a vertical line x = axis.
fn mirror_y(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "mirror_y <src> <dst> <axis>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let axis = farg(args, 2, USAGE)?;
    let (mp, units) = ctx.region(&src)?;
    let out = transform::mirror_y(&mp, axis);
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("mirrored '{src}' about y={axis} -> '{dst}'"))
}

/// Build a linear array of `n` copies of `mp` stepped by `(dx, dy)`.
fn linear_array(mp: &MultiPolygon<f64>, dx: f64, dy: f64, n: usize) -> MultiPolygon<f64> {
    let mut parts: Vec<Polygon<f64>> = Vec::new();
    for i in 0..n {
        let copy = transform::translate(mp, i as f64 * dx, i as f64 * dy);
        parts.extend(copy.0);
    }
    fc_geo::union_all(parts)
}

/// `array <src> <dst> <dx> <dy> <n>` — linear array of n copies.
fn array(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "array <src> <dst> <dx> <dy> <n>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let dx = farg(args, 2, USAGE)?;
    let dy = farg(args, 3, USAGE)?;
    let n_raw = iarg(args, 4, USAGE)?;
    if n_raw < 1 {
        return Err(ScriptError::Other(format!("array count must be >= 1, got {n_raw}")));
    }
    let n = n_raw as usize;
    let (mp, units) = ctx.region(&src)?;
    let out = linear_array(&mp, dx, dy, n);
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("array '{src}' x{n} step ({dx}, {dy}) -> '{dst}'"))
}

/// `panelize <src> <dst> <nx> <ny> <gutter>` — nx x ny panel with gutter.
///
/// Pitch is derived from the source bounds plus the gutter, then handed to
/// [`fc_cam::panelize`].
fn panelize(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "panelize <src> <dst> <nx> <ny> <gutter>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let nx_raw = iarg(args, 2, USAGE)?;
    let ny_raw = iarg(args, 3, USAGE)?;
    let gutter = farg(args, 4, USAGE)?;
    if nx_raw < 1 || ny_raw < 1 {
        return Err(ScriptError::Other(format!(
            "panel counts must be >= 1, got {nx_raw}x{ny_raw}"
        )));
    }
    let nx = nx_raw as usize;
    let ny = ny_raw as usize;
    let (mp, units) = ctx.region(&src)?;
    let (dx, dy) = match fc_geo::bounds(&mp) {
        Some((minx, miny, maxx, maxy)) => (maxx - minx + gutter, maxy - miny + gutter),
        None => return Err(ScriptError::Other(format!("'{src}' is empty"))),
    };
    let out = fc_cam::panelize(&mp, nx, ny, dx, dy);
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("panelize '{src}' {nx}x{ny} gutter {gutter} -> '{dst}'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_gcode::Units;

    fn seed(ctx: &mut ScriptContext) {
        ctx.put(
            "r",
            Obj::Region(
                MultiPolygon::new(vec![fc_geo::centered_rect(5.0, 5.0, 10.0, 10.0)]),
                Units::Mm,
            ),
        );
    }

    fn area_of(ctx: &ScriptContext, name: &str) -> f64 {
        let (mp, _) = ctx.region(name).unwrap();
        fc_geo::area(&mp)
    }

    #[test]
    fn rotate_creates_region_and_preserves_area() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        let base = area_of(&ctx, "r");
        rotate(&mut ctx, &["r".into(), "d".into(), "90".into()]).unwrap();
        let (_, u) = ctx.region("d").unwrap();
        assert_eq!(u, Units::Mm);
        assert_eq!(ctx.get("d").unwrap().kind(), "region");
        assert!((area_of(&ctx, "d") - base).abs() < 1e-6);
    }

    #[test]
    fn mirror_y_creates_region_and_preserves_area() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        let base = area_of(&ctx, "r");
        mirror_y(&mut ctx, &["r".into(), "d".into(), "0".into()]).unwrap();
        assert_eq!(ctx.get("d").unwrap().kind(), "region");
        assert!((area_of(&ctx, "d") - base).abs() < 1e-6);
    }

    #[test]
    fn array_n3_increases_area() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        let base = area_of(&ctx, "r");
        // step 20mm in x so the three 10mm squares are disjoint => 3x area.
        array(
            &mut ctx,
            &["r".into(), "d".into(), "20".into(), "0".into(), "3".into()],
        )
        .unwrap();
        assert_eq!(ctx.get("d").unwrap().kind(), "region");
        let got = area_of(&ctx, "d");
        assert!(got > base, "array area {got} should exceed base {base}");
        assert!((got - base * 3.0).abs() < 1e-6, "expected {}, got {got}", base * 3.0);
    }

    #[test]
    fn panelize_2x2_increases_area() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        let base = area_of(&ctx, "r");
        panelize(
            &mut ctx,
            &["r".into(), "d".into(), "2".into(), "2".into(), "5".into()],
        )
        .unwrap();
        assert_eq!(ctx.get("d").unwrap().kind(), "region");
        let got = area_of(&ctx, "d");
        // 2x2 disjoint copies (gutter > 0) => ~4x area.
        assert!(got > base, "panel area {got} should exceed base {base}");
        assert!((got - base * 4.0).abs() < 1e-6, "expected {}, got {got}", base * 4.0);
    }

    #[test]
    fn array_count_zero_errors() {
        let mut ctx = ScriptContext::new();
        seed(&mut ctx);
        let err = array(
            &mut ctx,
            &["r".into(), "d".into(), "1".into(), "0".into(), "0".into()],
        )
        .unwrap_err();
        assert!(matches!(err, ScriptError::Other(_)));
    }

    #[test]
    fn missing_args_yield_usage_error() {
        let mut ctx = ScriptContext::new();
        let err = rotate(&mut ctx, &["r".into()]).unwrap_err();
        assert!(matches!(err, ScriptError::Usage(_)));
    }

    #[test]
    fn commands_registered() {
        let names: Vec<&str> = commands().into_iter().map(|(n, _)| n).collect();
        for c in ["rotate", "mirror_y", "array", "panelize"] {
            assert!(names.contains(&c), "missing {c}");
        }
    }
}
