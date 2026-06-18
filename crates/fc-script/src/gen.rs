//! Generator commands — create new objects from scratch (Units::Mm).
//!
//! Parity with FlatCAM's geometry-creation Tcl commands: rectangles,
//! circles, and rectangular drill arrays. Each command builds a fresh
//! object and stores it in the [`ScriptContext`].

use crate::{farg, iarg, sarg, Obj, ScriptContext, ScriptError};
use fc_gcode::Units;

/// Register the generator commands.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("new_rect", new_rect),
        ("new_circle", new_circle),
        ("drill_array", drill_array),
    ]
}

/// `new_rect <name> <x> <y> <w> <h>` — axis-aligned rectangle whose
/// lower-left corner is at (x, y) with the given width/height.
fn new_rect(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "new_rect <name> <x> <y> <w> <h>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let x = farg(args, 1, USAGE)?;
    let y = farg(args, 2, USAGE)?;
    let w = farg(args, 3, USAGE)?;
    let h = farg(args, 4, USAGE)?;

    let poly = fc_geo::centered_rect(x + w / 2.0, y + h / 2.0, w, h);
    let mp = fc_geo::MultiPolygon::new(vec![poly]);
    ctx.put(name.clone(), Obj::Region(mp, Units::Mm));
    Ok(format!("Created region '{name}' ({w} x {h} mm)"))
}

/// `new_circle <name> <cx> <cy> <r>` — filled circle centered at (cx, cy).
fn new_circle(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "new_circle <name> <cx> <cy> <r>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let cx = farg(args, 1, USAGE)?;
    let cy = farg(args, 2, USAGE)?;
    let r = farg(args, 3, USAGE)?;

    let poly = fc_geo::circle(cx, cy, r, 48);
    let mp = fc_geo::MultiPolygon::new(vec![poly]);
    ctx.put(name.clone(), Obj::Region(mp, Units::Mm));
    Ok(format!("Created region '{name}' (circle r={r} mm)"))
}

/// `drill_array <name> <ox> <oy> <dx> <dy> <nx> <ny> <dia>` — build an
/// Excellon object holding a single tool (number 1) whose drills form an
/// nx-by-ny grid starting at (ox, oy) with pitch (dx, dy).
fn drill_array(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "drill_array <name> <ox> <oy> <dx> <dy> <nx> <ny> <dia>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let ox = farg(args, 1, USAGE)?;
    let oy = farg(args, 2, USAGE)?;
    let dx = farg(args, 3, USAGE)?;
    let dy = farg(args, 4, USAGE)?;
    let nx = iarg(args, 5, USAGE)?;
    let ny = iarg(args, 6, USAGE)?;
    let dia = farg(args, 7, USAGE)?;

    if nx < 0 || ny < 0 {
        return Err(ScriptError::Other("nx and ny must be non-negative".into()));
    }

    let mut drills: Vec<(f64, f64)> = Vec::with_capacity((nx * ny) as usize);
    for row in 0..ny {
        for col in 0..nx {
            drills.push((ox + col as f64 * dx, oy + row as f64 * dy));
        }
    }
    let count = drills.len();

    let tool = fc_excellon::Tool { diameter: dia, drills, slots: vec![] };
    let mut tools = std::collections::BTreeMap::new();
    tools.insert(1, tool);
    let exc = fc_excellon::Excellon { units: fc_excellon::Units::Mm, tools };

    ctx.put(name.clone(), Obj::Excellon(exc));
    Ok(format!("Created excellon '{name}' with {count} drills (dia={dia} mm)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ctx: &mut ScriptContext, line: &str) -> Result<String, ScriptError> {
        let toks: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
        let f = commands()
            .into_iter()
            .find(|(n, _)| *n == toks[0])
            .map(|(_, f)| f)
            .expect("command exists");
        f(ctx, &toks[1..])
    }

    #[test]
    fn new_rect_area_matches() {
        let mut ctx = ScriptContext::new();
        run(&mut ctx, "new_rect r 1 2 4 3").unwrap();
        let (mp, units) = ctx.region("r").unwrap();
        assert_eq!(units, Units::Mm);
        let a = fc_geo::area(&mp);
        assert!((a - 12.0).abs() < 1e-9, "area was {a}");
        // lower-left corner is (1,2), upper-right (5,5)
        let (minx, miny, maxx, maxy) = fc_geo::bounds(&mp).unwrap();
        assert!((minx - 1.0).abs() < 1e-9);
        assert!((miny - 2.0).abs() < 1e-9);
        assert!((maxx - 5.0).abs() < 1e-9);
        assert!((maxy - 5.0).abs() < 1e-9);
    }

    #[test]
    fn new_circle_area_approx_pi_r2() {
        let mut ctx = ScriptContext::new();
        run(&mut ctx, "new_circle c 0 0 2").unwrap();
        let (mp, _) = ctx.region("c").unwrap();
        let a = fc_geo::area(&mp);
        let expected = std::f64::consts::PI * 4.0;
        // 48-gon under-approximates the circle slightly.
        assert!((a - expected).abs() < 0.05, "area was {a}, expected ~{expected}");
    }

    #[test]
    fn drill_array_builds_excellon() {
        let mut ctx = ScriptContext::new();
        run(&mut ctx, "drill_array d 0 0 2 2 3 4 0.8").unwrap();
        match ctx.get("d").unwrap() {
            Obj::Excellon(exc) => {
                let total: usize = exc.tools.values().map(|t| t.drills.len()).sum();
                assert_eq!(total, 12);
                let tool = exc.tools.get(&1).expect("tool 1 exists");
                assert!((tool.diameter - 0.8).abs() < 1e-9);
                assert_eq!(tool.drills.len(), 12);
                // first drill at origin, last at (col2*2, row3*2) = (4,6)
                assert_eq!(tool.drills[0], (0.0, 0.0));
                assert_eq!(tool.drills[11], (4.0, 6.0));
            }
            other => panic!("expected excellon, got {}", other.kind()),
        }
    }

    #[test]
    fn missing_arg_is_usage_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(run(&mut ctx, "new_rect r 1 2"), Err(ScriptError::Usage(_))));
    }
}
