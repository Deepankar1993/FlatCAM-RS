//! Object-introspection and management commands (parity with FlatCAM's
//! `list_sys`, `bbox`, `get_bounds`, `delete`, etc. Tcl commands).
//!
//! These operate purely on the [`ScriptContext`] object collection: listing,
//! measuring, counting, deleting and renaming the named objects produced by the
//! generative / I/O / CAM command groups.

use crate::{sarg, Obj, ScriptContext, ScriptError};

/// Register the query/management command group.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("list", cmd_list),
        ("bounds", cmd_bounds),
        ("area", cmd_area),
        ("count", cmd_count),
        ("delete", cmd_delete),
        ("rename", cmd_rename),
    ]
}

/// `list` — one `name:kind` line per object, in collection (sorted) order.
fn cmd_list(ctx: &mut ScriptContext, _args: &[String]) -> Result<String, ScriptError> {
    let lines: Vec<String> = ctx
        .names()
        .into_iter()
        .map(|name| {
            let kind = ctx.get(&name).map(|o| o.kind()).unwrap_or("?");
            format!("{name}:{kind}")
        })
        .collect();
    Ok(lines.join("\n"))
}

/// `bounds <name>` — bounding box of a Region as `minx miny maxx maxy`.
fn cmd_bounds(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    let name = sarg(args, 0, "bounds <name>")?;
    let (mp, _units) = ctx.region(name)?;
    match fc_geo::bounds(&mp) {
        Some((minx, miny, maxx, maxy)) => Ok(format!("{minx} {miny} {maxx} {maxy}")),
        None => Err(ScriptError::Other(format!("{name} is empty, no bounds"))),
    }
}

/// `area <name>` — unsigned area of a Region.
fn cmd_area(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    let name = sarg(args, 0, "area <name>")?;
    let (mp, _units) = ctx.region(name)?;
    Ok(format!("{:.4}", fc_geo::area(&mp)))
}

/// `count <name>` — element count, meaning depends on object kind:
/// Region -> polygon count, Excellon -> total drills, Cnc -> path count.
fn cmd_count(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    let name = sarg(args, 0, "count <name>")?;
    let n = match ctx.get(name)? {
        Obj::Region(mp, _) => mp.0.len(),
        Obj::Excellon(exc) => exc.tools.values().map(|t| t.drills.len()).sum(),
        Obj::Cnc { paths, .. } => paths.len(),
    };
    Ok(n.to_string())
}

/// `delete <name>` — remove an object (errors if it does not exist).
fn cmd_delete(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    let name = sarg(args, 0, "delete <name>")?;
    match ctx.objects.remove(name) {
        Some(_) => Ok("deleted".into()),
        None => Err(ScriptError::NotFound(name.to_string())),
    }
}

/// `rename <old> <new>` — move an object to a new name (errors if absent).
fn cmd_rename(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    let old = sarg(args, 0, "rename <old> <new>")?.to_string();
    let new = sarg(args, 1, "rename <old> <new>")?.to_string();
    let obj = ctx
        .objects
        .remove(&old)
        .ok_or_else(|| ScriptError::NotFound(old.clone()))?;
    ctx.put(new, obj);
    Ok("renamed".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_gcode::Units;

    /// A 3x4 axis-aligned rectangle region centered at the origin, plus a
    /// small CNC object, to exercise the introspection commands.
    fn setup() -> ScriptContext {
        let mut ctx = ScriptContext::new();
        let rect = fc_geo::centered_rect(0.0, 0.0, 3.0, 4.0);
        let mp = fc_geo::MultiPolygon::new(vec![rect]);
        ctx.put("board", Obj::Region(mp, Units::Mm));

        let paths: Vec<fc_gcode::Polyline> =
            vec![vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]];
        ctx.put("job", crate::make_cnc(paths, Units::Mm, 0.4));
        ctx
    }

    #[test]
    fn list_contains_names_and_kinds() {
        let mut ctx = setup();
        let out = cmd_list(&mut ctx, &[]).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.contains(&"board:region"));
        assert!(lines.contains(&"job:cnc"));
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn bounds_of_3x4_rect() {
        let mut ctx = setup();
        let out = cmd_bounds(&mut ctx, &["board".to_string()]).unwrap();
        // centered at origin -> [-1.5, -2.0, 1.5, 2.0]
        assert_eq!(out, "-1.5 -2 1.5 2");
    }

    #[test]
    fn area_of_3x4_rect() {
        let mut ctx = setup();
        let out = cmd_area(&mut ctx, &["board".to_string()]).unwrap();
        assert_eq!(out, "12.0000");
    }

    #[test]
    fn count_region_polygons_and_cnc_paths() {
        let mut ctx = setup();
        assert_eq!(cmd_count(&mut ctx, &["board".to_string()]).unwrap(), "1");
        assert_eq!(cmd_count(&mut ctx, &["job".to_string()]).unwrap(), "1");
    }

    #[test]
    fn count_excellon_drills() {
        use fc_excellon::{Excellon, Tool, Units as ExUnits};
        use std::collections::BTreeMap;

        let mut tools: BTreeMap<i32, Tool> = BTreeMap::new();
        tools.insert(
            1,
            Tool {
                diameter: 0.8,
                drills: vec![(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)],
                slots: vec![],
            },
        );
        let exc = Excellon { units: ExUnits::Mm, tools };
        let mut ctx = ScriptContext::new();
        ctx.put("drl", Obj::Excellon(exc));
        assert_eq!(cmd_count(&mut ctx, &["drl".to_string()]).unwrap(), "3");
    }

    #[test]
    fn delete_removes_object() {
        let mut ctx = setup();
        assert_eq!(cmd_delete(&mut ctx, &["board".to_string()]).unwrap(), "deleted");
        assert!(ctx.get("board").is_err());
        // deleting again errors
        assert!(cmd_delete(&mut ctx, &["board".to_string()]).is_err());
    }

    #[test]
    fn rename_moves_object() {
        let mut ctx = setup();
        assert_eq!(
            cmd_rename(&mut ctx, &["board".to_string(), "panel".to_string()]).unwrap(),
            "renamed"
        );
        assert!(ctx.get("board").is_err());
        assert_eq!(ctx.get("panel").unwrap().kind(), "region");
        // renaming a missing object errors
        assert!(cmd_rename(&mut ctx, &["nope".to_string(), "x".to_string()]).is_err());
    }

    #[test]
    fn bounds_and_area_reject_non_region() {
        let mut ctx = setup();
        assert!(cmd_bounds(&mut ctx, &["job".to_string()]).is_err());
        assert!(cmd_area(&mut ctx, &["job".to_string()]).is_err());
    }
}
