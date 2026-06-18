//! Board-editing command group for the script engine.
//!
//! The parity equivalent of FlatCAM's copper-editing tools (Etch Compensation,
//! Copper Thieving / pour, Fiducials, Thermal Relief). Each command consumes a
//! source region from the [`ScriptContext`] (where applicable) and stores a
//! freshly built [`Obj::Region`] back into the context under the destination
//! name. Region results inherit the source's units; the purely synthetic
//! generators (`fiducials`, `thermal`) emit millimetre regions.

use crate::{farg, iarg, sarg, Obj, ScriptContext, ScriptError};
use fc_gcode::Units;

/// Register the board-editing commands.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("etch", etch),
        ("copper_pour", copper_pour),
        ("fiducials", fiducials),
        ("thermal", thermal),
    ]
}

/// `etch <src> <dst> <factor>` — widen copper to counteract etchant undercut.
fn etch(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "etch <src> <dst> <factor>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let factor = farg(args, 2, USAGE)?;

    let (mp, units) = ctx.region(&src)?;
    let out = fc_cam::etch::compensate(&mp, &fc_cam::etch::EtchParams { factor });
    let n = out.0.len();
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("{dst}: {n} polygons"))
}

/// `copper_pour <src> <dst> <clearance>` — flood the board with copper around
/// the existing tracks, keeping `clearance` away from them.
fn copper_pour(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "copper_pour <src> <dst> <clearance>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let clearance = farg(args, 2, USAGE)?;

    let (mp, units) = ctx.region(&src)?;
    let board = fc_geo::bounds(&mp)
        .ok_or_else(|| ScriptError::Other(format!("{src} has no extent")))?;
    let out = fc_cam::copper_pour::copper_pour(board, &mp, clearance);
    let n = out.0.len();
    ctx.put(dst.clone(), Obj::Region(out, units));
    Ok(format!("{dst}: {n} polygons"))
}

/// `fiducials <src> <dst> <margin> <dia>` — place corner fiducial dots inset
/// `margin` from the source's bounding box.
fn fiducials(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "fiducials <src> <dst> <margin> <dia>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let margin = farg(args, 2, USAGE)?;
    let dia = farg(args, 3, USAGE)?;

    let (mp, _units) = ctx.region(&src)?;
    let bounds = fc_geo::bounds(&mp)
        .ok_or_else(|| ScriptError::Other(format!("{src} has no extent")))?;
    let out = fc_cam::fiducials::corner_fiducials(bounds, margin, dia, 24);
    let n = out.0.len();
    ctx.put(dst.clone(), Obj::Region(out, Units::Mm));
    Ok(format!("{dst}: {n} fiducials"))
}

/// `thermal <dst> <cx> <cy> <pad_dia> <hole_dia> <gap> <spokes>` — synthesise a
/// single thermal-relief pad.
fn thermal(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "thermal <dst> <cx> <cy> <pad_dia> <hole_dia> <gap> <spokes>";
    let dst = sarg(args, 0, USAGE)?.to_string();
    let cx = farg(args, 1, USAGE)?;
    let cy = farg(args, 2, USAGE)?;
    let pad_dia = farg(args, 3, USAGE)?;
    let hole_dia = farg(args, 4, USAGE)?;
    let gap = farg(args, 5, USAGE)?;
    let spokes = iarg(args, 6, USAGE)?.max(0) as usize;

    let out = fc_cam::thermal::thermal_relief(cx, cy, pad_dia, hole_dia, gap, spokes, 24);
    let n = out.0.len();
    ctx.put(dst.clone(), Obj::Region(out, Units::Mm));
    Ok(format!("{dst}: {n} polygons"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect, MultiPolygon};

    fn ctx_with_region() -> ScriptContext {
        let mut ctx = ScriptContext::new();
        ctx.put(
            "r",
            Obj::Region(
                MultiPolygon::new(vec![centered_rect(10.0, 10.0, 8.0, 8.0)]),
                Units::Mm,
            ),
        );
        ctx
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    /// Pull a Region's geometry, panicking if the object is missing or wrong kind.
    fn region_of<'a>(ctx: &'a ScriptContext, name: &str) -> &'a MultiPolygon<f64> {
        match ctx.get(name).unwrap() {
            Obj::Region(mp, _) => mp,
            other => panic!("{name} is a {}, expected region", other.kind()),
        }
    }

    #[test]
    fn commands_registered() {
        let names: Vec<&str> = commands().iter().map(|(n, _)| *n).collect();
        for c in ["etch", "copper_pour", "fiducials", "thermal"] {
            assert!(names.contains(&c), "missing {c}");
        }
    }

    #[test]
    fn etch_grows_area() {
        let mut ctx = ctx_with_region();
        let before = area(region_of(&ctx, "r"));
        etch(&mut ctx, &s(&["r", "etched", "0.5"])).unwrap();
        let after = area(region_of(&ctx, "etched"));
        assert!(after > before, "positive factor should grow area ({after} > {before})");
    }

    #[test]
    fn copper_pour_makes_region() {
        // Use copper that does NOT fill its bounding box (a small square in a
        // larger spread), so the pour leaves a positive-area gap.
        let mut ctx = ScriptContext::new();
        ctx.put(
            "spread",
            Obj::Region(
                MultiPolygon::new(vec![
                    centered_rect(0.0, 0.0, 2.0, 2.0),
                    centered_rect(20.0, 20.0, 2.0, 2.0),
                ]),
                Units::Mm,
            ),
        );
        copper_pour(&mut ctx, &s(&["spread", "pour", "0.3"])).unwrap();
        let mp = region_of(&ctx, "pour");
        assert!(area(mp) > 0.0, "pour should have positive area");
    }

    #[test]
    fn fiducials_makes_region() {
        let mut ctx = ctx_with_region();
        fiducials(&mut ctx, &s(&["r", "fids", "1.0", "1.5"])).unwrap();
        let mp = region_of(&ctx, "fids");
        assert!(area(mp) > 0.0, "fiducials should have positive area");
    }

    #[test]
    fn thermal_makes_region() {
        let mut ctx = ScriptContext::new();
        thermal(
            &mut ctx,
            &s(&["pad", "0", "0", "4.0", "1.5", "0.4", "4"]),
        )
        .unwrap();
        let mp = region_of(&ctx, "pad");
        assert!(area(mp) > 0.0, "thermal pad should have positive area");
    }

    #[test]
    fn usage_error_when_missing_args() {
        let mut ctx = ctx_with_region();
        assert!(etch(&mut ctx, &s(&["r"])).is_err());
        assert!(thermal(&mut ctx, &s(&["pad"])).is_err());
    }
}
