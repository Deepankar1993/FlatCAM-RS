//! CAM command group for the script engine.
//!
//! The parity equivalent of FlatCAM's CAM-producing Tcl commands
//! (`isolate`, `paint`, `ncc`, `cutout`, `drillcncjob`). Each command consumes
//! a region (or Excellon) object from the [`ScriptContext`] and stores a freshly
//! built CNC object — a set of tool-paths plus rendered GRBL G-code — back into
//! the context under the destination name.

use crate::{farg, iarg, make_cnc, sarg, Obj, ScriptContext, ScriptError};
use fc_gcode::{JobKind, JobParams, Polyline};

/// Register the CAM commands.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("isolate", isolate),
        ("paint", paint),
        ("ncc", ncc),
        ("cutout", cutout),
        ("drill", drill),
    ]
}

/// `isolate <src> <dst> <tool_dia> [passes] [overlap]`
fn isolate(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "isolate <src> <dst> <tool_dia> [passes] [overlap]";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let tool_dia = farg(args, 2, USAGE)?;
    let passes = if args.len() > 3 { iarg(args, 3, USAGE)?.max(1) as usize } else { 1 };
    let overlap = if args.len() > 4 { farg(args, 4, USAGE)? } else { 0.0 };

    let (mp, units) = ctx.region(&src)?;
    let params = fc_cam::IsolationParams {
        tool_diameter: tool_dia,
        passes,
        overlap,
        ..Default::default()
    };
    let job = fc_cam::isolation_geo(&mp, units, &params);
    let paths = mill_paths(job.kind)?;
    let n = paths.len();
    ctx.put(dst.clone(), make_cnc(paths, units, tool_dia));
    Ok(format!("{dst}: {n} paths"))
}

/// `paint <src> <dst> <tool_dia> [overlap]`
fn paint(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "paint <src> <dst> <tool_dia> [overlap]";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let tool_dia = farg(args, 2, USAGE)?;

    let (mp, units) = ctx.region(&src)?;
    let mut params = fc_cam::PaintParams {
        tool_diameter: tool_dia,
        ..Default::default()
    };
    if args.len() > 3 {
        params.overlap = farg(args, 3, USAGE)?;
    }
    let paths = fc_cam::paint_region(&mp, &params);
    let n = paths.len();
    ctx.put(dst.clone(), make_cnc(paths, units, tool_dia));
    Ok(format!("{dst}: {n} paths"))
}

/// `ncc <src> <dst> <tool_dia> [overlap]`
fn ncc(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "ncc <src> <dst> <tool_dia> [overlap]";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let tool_dia = farg(args, 2, USAGE)?;

    let (mp, units) = ctx.region(&src)?;
    let mut params = fc_cam::NccParams {
        tool_diameter: tool_dia,
        ..Default::default()
    };
    if args.len() > 3 {
        params.overlap = farg(args, 3, USAGE)?;
    }
    let job = fc_cam::ncc_job(&mp, &params, units);
    let paths = mill_paths(job.kind)?;
    let n = paths.len();
    ctx.put(dst.clone(), make_cnc(paths, units, tool_dia));
    Ok(format!("{dst}: {n} paths"))
}

/// `cutout <src> <dst> <tool_dia> [tabs]`
fn cutout(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "cutout <src> <dst> <tool_dia> [tabs]";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();
    let tool_dia = farg(args, 2, USAGE)?;
    let tabs = if args.len() > 3 { iarg(args, 3, USAGE)?.max(0) as usize } else { 4 };

    let (mp, units) = ctx.region(&src)?;
    let (minx, miny, maxx, maxy) = fc_geo::bounds(&mp)
        .ok_or_else(|| ScriptError::Other(format!("{src} has no extent")))?;
    let params = fc_cam::CutoutParams {
        tool_diameter: tool_dia,
        tabs,
        ..Default::default()
    };
    let paths = fc_cam::cutout_rectangular(minx, miny, maxx, maxy, &params);
    let n = paths.len();
    ctx.put(dst.clone(), make_cnc(paths, units, tool_dia));
    Ok(format!("{dst}: {n} paths"))
}

/// `drill <src> <dst>` — build a drilling CNC job from an Excellon object.
fn drill(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "drill <src> <dst>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let dst = sarg(args, 1, USAGE)?.to_string();

    let exc = match ctx.get(&src)? {
        Obj::Excellon(e) => e,
        other => {
            return Err(ScriptError::Other(format!(
                "{src} is a {}, expected excellon",
                other.kind()
            )))
        }
    };

    let units = match exc.units {
        fc_excellon::Units::Mm => fc_gcode::Units::Mm,
        fc_excellon::Units::Inch => fc_gcode::Units::Inch,
    };

    let jobs = fc_cam::drilling_all(
        &exc,
        JobParams {
            units,
            ..Default::default()
        },
    );

    let mut paths: Vec<Polyline> = Vec::new();
    let mut gcode = String::new();
    for (_tool, job) in &jobs {
        if let JobKind::Drill { points } = &job.kind {
            for &pt in points {
                paths.push(vec![pt]);
            }
        }
        gcode.push_str(&job.to_gcode(&fc_gcode::Grbl));
    }

    let holes = paths.len();
    ctx.put(dst.clone(), Obj::Cnc { paths, units, gcode });
    Ok(format!("{dst}: {holes} holes"))
}

/// Pull the milling paths out of a Mill job, erroring on a non-mill job.
fn mill_paths(kind: JobKind) -> Result<Vec<Polyline>, ScriptError> {
    match kind {
        JobKind::Mill { paths } => Ok(paths),
        JobKind::Drill { .. } => Err(ScriptError::Other("expected a milling job".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_gcode::Units;
    use fc_geo::{centered_rect, MultiPolygon};

    fn ctx_with_region() -> ScriptContext {
        let mut ctx = ScriptContext::new();
        ctx.put(
            "r",
            Obj::Region(
                MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]),
                Units::Mm,
            ),
        );
        ctx
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn commands_registered() {
        let names: Vec<&str> = commands().iter().map(|(n, _)| *n).collect();
        for c in ["isolate", "paint", "ncc", "cutout", "drill"] {
            assert!(names.contains(&c), "missing {c}");
        }
    }

    #[test]
    fn isolate_makes_cnc() {
        let mut ctx = ctx_with_region();
        let msg = isolate(&mut ctx, &s(&["r", "iso", "0.4"])).unwrap();
        assert!(msg.starts_with("iso:"));
        assert_eq!(ctx.get("iso").unwrap().kind(), "cnc");
    }

    #[test]
    fn isolate_multipass() {
        let mut ctx = ctx_with_region();
        isolate(&mut ctx, &s(&["r", "iso", "0.4", "3", "0.1"])).unwrap();
        if let Obj::Cnc { paths, .. } = ctx.get("iso").unwrap() {
            assert_eq!(paths.len(), 3, "three passes => three rings");
        } else {
            panic!("expected cnc");
        }
    }

    #[test]
    fn paint_makes_cnc() {
        let mut ctx = ctx_with_region();
        paint(&mut ctx, &s(&["r", "pnt", "0.5"])).unwrap();
        assert_eq!(ctx.get("pnt").unwrap().kind(), "cnc");
    }

    #[test]
    fn ncc_makes_cnc() {
        let mut ctx = ctx_with_region();
        ncc(&mut ctx, &s(&["r", "nc", "0.5"])).unwrap();
        assert_eq!(ctx.get("nc").unwrap().kind(), "cnc");
    }

    #[test]
    fn cutout_makes_cnc() {
        let mut ctx = ctx_with_region();
        let msg = cutout(&mut ctx, &s(&["r", "cut", "1.0", "4"])).unwrap();
        assert!(msg.starts_with("cut:"));
        assert_eq!(ctx.get("cut").unwrap().kind(), "cnc");
    }

    #[test]
    fn drill_from_excellon() {
        let mut ctx = ScriptContext::new();
        let src = "\
M48
METRIC,LZ
T1C0.8
%
T1
X10.0Y10.0
X20.0Y10.0
M30
";
        let e = fc_excellon::parse(src).unwrap();
        ctx.put("e", Obj::Excellon(e));
        let msg = drill(&mut ctx, &s(&["e", "drl"])).unwrap();
        assert_eq!(msg, "drl: 2 holes");
        let obj = ctx.get("drl").unwrap();
        assert_eq!(obj.kind(), "cnc");
        if let Obj::Cnc { paths, gcode, .. } = obj {
            assert_eq!(paths.len(), 2);
            assert!(paths.iter().all(|p| p.len() == 1));
            assert!(gcode.contains("M30"));
        } else {
            panic!("expected cnc");
        }
    }

    #[test]
    fn drill_rejects_region() {
        let mut ctx = ctx_with_region();
        assert!(drill(&mut ctx, &s(&["r", "drl"])).is_err());
    }

    #[test]
    fn isolate_usage_error_when_missing_args() {
        let mut ctx = ctx_with_region();
        assert!(isolate(&mut ctx, &s(&["r"])).is_err());
    }
}
