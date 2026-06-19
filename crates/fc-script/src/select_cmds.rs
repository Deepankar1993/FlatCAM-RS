//! `select_cmds` — selection/active-object and plot commands.
//!
//! Parity with FlatCAM's selection and plotting Tcl commands. In the headless
//! engine there is no GUI canvas, so "plotting" is implemented as a real,
//! testable query: it validates the requested objects and returns a summary
//! line describing what would be drawn. The active/selected object is tracked on
//! the [`ScriptContext::active`] field (cooperating with `delete`/`rename` in
//! the `query` module, which keep it consistent).
//!
//! Commands:
//! * `get_active` — name of the active object (error if none selected).
//! * `set_active <name>` — mark a named object active (error if it doesn't exist).
//! * `plot_all` — summary listing every object (count + `name:kind` per line).
//! * `plot_objects <name1> [name2 ...]` — same, scoped to the named objects
//!   (each must exist).
//! * `quit_app` — headless quit acknowledgement (returns a sentinel message).

use crate::{sarg, ScriptContext, ScriptError};

/// Register the selection/plot command group.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("get_active", get_active),
        ("set_active", set_active),
        ("plot_all", plot_all),
        ("plot_objects", plot_objects),
        ("quit_app", quit_app),
    ]
}

/// `get_active` — return the name of the active/selected object.
///
/// Errors if no object is currently active.
fn get_active(ctx: &mut ScriptContext, _args: &[String]) -> Result<String, ScriptError> {
    ctx.active
        .clone()
        .ok_or_else(|| ScriptError::Other("no active object".into()))
}

/// `set_active <name>` — mark the named object active (error if it doesn't exist).
fn set_active(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "set_active <name>";
    let name = sarg(args, 0, USAGE)?.to_string();
    // Validate existence; `get` returns NotFound for missing objects.
    ctx.get(&name)?;
    ctx.active = Some(name.clone());
    Ok(format!("active = {name}"))
}

/// Format a `name:kind` line for one object (used by the plot summaries).
fn obj_line(ctx: &ScriptContext, name: &str) -> String {
    let kind = ctx.get(name).map(|o| o.kind()).unwrap_or("?");
    format!("{name}:{kind}")
}

/// `plot_all` — headless "plot everything": return a summary of all objects.
///
/// First line is `plotted N object(s)`; one `name:kind` line follows per object,
/// in collection (sorted) order. With no objects, returns `plotted 0 object(s)`.
fn plot_all(ctx: &mut ScriptContext, _args: &[String]) -> Result<String, ScriptError> {
    let names = ctx.names();
    let mut lines = vec![format!("plotted {} object(s)", names.len())];
    for name in &names {
        lines.push(obj_line(ctx, name));
    }
    Ok(lines.join("\n"))
}

/// `plot_objects <name1> [name2 ...]` — plot only the named objects.
///
/// Every name must exist (otherwise a `NotFound` error). Returns the same
/// summary shape as `plot_all`, scoped to the requested objects.
fn plot_objects(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "plot_objects <name1> [name2 ...]";
    if args.is_empty() {
        return Err(ScriptError::Usage(USAGE.into()));
    }
    // Validate all names up-front so a partial plot can't be reported.
    for name in args {
        ctx.get(name)?;
    }
    let mut lines = vec![format!("plotted {} object(s)", args.len())];
    for name in args {
        lines.push(obj_line(ctx, name));
    }
    Ok(lines.join("\n"))
}

/// `quit_app` — request application quit.
///
/// The headless engine has no run loop to stop, so this returns a clear
/// sentinel message acknowledging the request (parity with FlatCAM's `quit`/
/// `quit_flatcam`).
fn quit_app(_ctx: &mut ScriptContext, _args: &[String]) -> Result<String, ScriptError> {
    Ok("quit requested".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Obj;
    use fc_gcode::Units;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn square_region(cx: f64, cy: f64, side: f64) -> Obj {
        let poly = fc_geo::centered_rect(cx, cy, side, side);
        Obj::Region(fc_geo::MultiPolygon::new(vec![poly]), Units::Mm)
    }

    fn setup() -> ScriptContext {
        let mut ctx = ScriptContext::new();
        ctx.put("board", square_region(0.0, 0.0, 10.0));
        ctx.put("job", Obj::Cnc { paths: vec![], units: Units::Mm, gcode: String::new() });
        ctx
    }

    #[test]
    fn commands_registered() {
        let names: Vec<&str> = commands().into_iter().map(|(n, _)| n).collect();
        for c in ["get_active", "set_active", "plot_all", "plot_objects", "quit_app"] {
            assert!(names.contains(&c), "missing {c}");
        }
    }

    // ----- get_active / set_active -----

    #[test]
    fn get_active_none_errors() {
        let mut ctx = setup();
        assert!(matches!(get_active(&mut ctx, &[]), Err(ScriptError::Other(_))));
    }

    #[test]
    fn set_then_get_active() {
        let mut ctx = setup();
        let msg = set_active(&mut ctx, &s(&["board"])).unwrap();
        assert!(msg.contains("board"));
        assert_eq!(get_active(&mut ctx, &[]).unwrap(), "board");
    }

    #[test]
    fn set_active_missing_errors() {
        let mut ctx = setup();
        assert!(matches!(set_active(&mut ctx, &s(&["nope"])), Err(ScriptError::NotFound(_))));
    }

    #[test]
    fn set_active_usage_error() {
        let mut ctx = setup();
        assert!(matches!(set_active(&mut ctx, &[]), Err(ScriptError::Usage(_))));
    }

    #[test]
    fn delete_clears_active() {
        // Cooperation with query::cmd_delete: deleting the active object clears it.
        use crate::Registry;
        let r = Registry::new();
        let mut ctx = setup();
        r.run_line(&mut ctx, "set_active board").unwrap();
        assert_eq!(r.run_line(&mut ctx, "get_active").unwrap(), "board");
        r.run_line(&mut ctx, "delete board").unwrap();
        assert!(r.run_line(&mut ctx, "get_active").is_err());
    }

    #[test]
    fn rename_follows_active() {
        // Cooperation with query::cmd_rename: active follows the new name.
        use crate::Registry;
        let r = Registry::new();
        let mut ctx = setup();
        r.run_line(&mut ctx, "set_active board").unwrap();
        r.run_line(&mut ctx, "rename board panel").unwrap();
        assert_eq!(r.run_line(&mut ctx, "get_active").unwrap(), "panel");
    }

    // ----- plot_all -----

    #[test]
    fn plot_all_lists_objects() {
        let mut ctx = setup();
        let out = plot_all(&mut ctx, &[]).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "plotted 2 object(s)");
        assert!(lines.contains(&"board:region"));
        assert!(lines.contains(&"job:cnc"));
    }

    #[test]
    fn plot_all_empty_context() {
        let mut ctx = ScriptContext::new();
        let out = plot_all(&mut ctx, &[]).unwrap();
        assert_eq!(out, "plotted 0 object(s)");
    }

    // ----- plot_objects -----

    #[test]
    fn plot_objects_scoped() {
        let mut ctx = setup();
        let out = plot_objects(&mut ctx, &s(&["board"])).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "plotted 1 object(s)");
        assert!(lines.contains(&"board:region"));
        assert!(!lines.contains(&"job:cnc"));
    }

    #[test]
    fn plot_objects_multiple() {
        let mut ctx = setup();
        let out = plot_objects(&mut ctx, &s(&["board", "job"])).unwrap();
        assert!(out.starts_with("plotted 2 object(s)"));
    }

    #[test]
    fn plot_objects_missing_errors() {
        let mut ctx = setup();
        assert!(matches!(
            plot_objects(&mut ctx, &s(&["board", "nope"])),
            Err(ScriptError::NotFound(_))
        ));
    }

    #[test]
    fn plot_objects_no_args_usage() {
        let mut ctx = setup();
        assert!(matches!(plot_objects(&mut ctx, &[]), Err(ScriptError::Usage(_))));
    }

    // ----- quit_app -----

    #[test]
    fn quit_app_returns_message() {
        let mut ctx = ScriptContext::new();
        let msg = quit_app(&mut ctx, &[]).unwrap();
        assert!(msg.contains("quit"));
    }
}
