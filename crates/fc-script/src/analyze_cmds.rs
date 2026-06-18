//! Design-rule / analysis commands (parity with FlatCAM's `ToolRulesCheck`,
//! `ToolOptimal` and `ToolReport` Tcl-shell surfaces).
//!
//! These commands are read-only: they take a Region object from the
//! [`ScriptContext`] and return a human-readable text result. They never create
//! or mutate objects in the collection.

use crate::{farg, sarg, ScriptContext, ScriptError};

/// Register the analysis command group.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("drc", cmd_drc),
        ("min_spacing", cmd_min_spacing),
        ("report", cmd_report),
    ]
}

/// `drc <name> <min_clearance>` — pass/fail design-rule clearance check.
fn cmd_drc(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    let name = sarg(args, 0, "drc <name> <min_clearance>")?.to_string();
    let min = farg(args, 1, "drc <name> <min_clearance>")?;
    let (mp, _units) = ctx.region(&name)?;
    let ok = fc_cam::min_clearance_ok(&mp, min);
    Ok(if ok {
        "DRC pass"
    } else {
        "DRC FAIL: features closer than min_clearance"
    }
    .to_string())
}

/// `min_spacing <name>` — smallest gap between distinct copper features.
fn cmd_min_spacing(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    let name = sarg(args, 0, "min_spacing <name>")?.to_string();
    let (mp, _units) = ctx.region(&name)?;
    match fc_cam::minimum_spacing(&mp) {
        Some(d) => Ok(format!("min spacing {:.4}", d)),
        None => Ok("min spacing: n/a (need >=2 features)".into()),
    }
}

/// `report <name>` — geometric summary of a Region object.
fn cmd_report(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    let name = sarg(args, 0, "report <name>")?.to_string();
    let (mp, _units) = ctx.region(&name)?;
    let r = fc_cam::report::report(&mp);
    Ok(format!(
        "polygons {} area {:.4} width {:.4} height {:.4}",
        r.polygons, r.area, r.width, r.height
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Obj;
    use fc_gcode::Units;

    /// Two separated unit circles, centers 6 apart (so the gap between them is
    /// roughly 4 units), as a single Region object named `board`.
    fn two_circles_ctx() -> ScriptContext {
        let mut ctx = ScriptContext::new();
        let a = fc_geo::circle(0.0, 0.0, 1.0, 64);
        let b = fc_geo::circle(6.0, 0.0, 1.0, 64);
        let mp = fc_geo::MultiPolygon::new(vec![a, b]);
        ctx.put("board", Obj::Region(mp, Units::Mm));
        ctx
    }

    /// A 3x4 axis-aligned rectangle Region named `rect` (area 12).
    fn rect_ctx() -> ScriptContext {
        let mut ctx = ScriptContext::new();
        let rect = fc_geo::centered_rect(0.0, 0.0, 3.0, 4.0);
        let mp = fc_geo::MultiPolygon::new(vec![rect]);
        ctx.put("rect", Obj::Region(mp, Units::Mm));
        ctx
    }

    #[test]
    fn min_spacing_two_circles_is_some() {
        let mut ctx = two_circles_ctx();
        let out = cmd_min_spacing(&mut ctx, &["board".to_string()]).unwrap();
        assert!(out.starts_with("min spacing "), "got {out}");
        // parse the reported distance and sanity-check it is the ~4 unit gap.
        let d: f64 = out.trim_start_matches("min spacing ").parse().unwrap();
        assert!(d > 3.0 && d < 5.0, "unexpected spacing {d}");
    }

    #[test]
    fn min_spacing_single_feature_is_na() {
        let mut ctx = rect_ctx();
        let out = cmd_min_spacing(&mut ctx, &["rect".to_string()]).unwrap();
        assert_eq!(out, "min spacing: n/a (need >=2 features)");
    }

    #[test]
    fn drc_passes_at_small_clearance() {
        let mut ctx = two_circles_ctx();
        let out = cmd_drc(&mut ctx, &["board".to_string(), "0.5".to_string()]).unwrap();
        assert_eq!(out, "DRC pass");
    }

    #[test]
    fn drc_fails_at_large_clearance() {
        let mut ctx = two_circles_ctx();
        // 10 units of required clearance is far larger than the ~4 unit gap.
        let out = cmd_drc(&mut ctx, &["board".to_string(), "10".to_string()]).unwrap();
        assert_eq!(out, "DRC FAIL: features closer than min_clearance");
    }

    #[test]
    fn report_of_3x4_rect_mentions_area_12() {
        let mut ctx = rect_ctx();
        let out = cmd_report(&mut ctx, &["rect".to_string()]).unwrap();
        assert!(out.contains("polygons 1"), "got {out}");
        assert!(out.contains("area 12.0000"), "got {out}");
        assert!(out.contains("width 3.0000"), "got {out}");
        assert!(out.contains("height 4.0000"), "got {out}");
    }

    #[test]
    fn commands_register_expected_names() {
        let names: Vec<&str> = commands().into_iter().map(|(n, _)| n).collect();
        assert!(names.contains(&"drc"));
        assert!(names.contains(&"min_spacing"));
        assert!(names.contains(&"report"));
    }

    #[test]
    fn non_region_object_errors() {
        let mut ctx = ScriptContext::new();
        let job = crate::make_cnc(vec![vec![(0.0, 0.0), (1.0, 0.0)]], Units::Mm, 0.4);
        ctx.put("job", job);
        assert!(cmd_report(&mut ctx, &["job".to_string()]).is_err());
        assert!(cmd_drc(&mut ctx, &["job".to_string(), "0.1".to_string()]).is_err());
    }
}
