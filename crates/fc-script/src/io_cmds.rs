//! `io_cmds` command group — exporting/saving script objects to disk.
//!
//! Parity with FlatCAM's Tcl export commands (`export_gerber`,
//! `export_excellon`, `export_svg`, `export_dxf`, `write_gcode`,
//! `save_project`). Geometry/CAM objects held on the [`ScriptContext`] are
//! serialized via the matching `fc_*` writer crate and written to a path; the
//! `open_gcode` reader loads raw G-code text into a Cnc object.
//!
//! Each command follows the house pattern: parse args with `sarg`, read the
//! object off the context, write the file, and return a short human-readable
//! message.

use crate::{sarg, Obj, ScriptContext, ScriptError};
use fc_gcode::Units;
use fc_geo::{LineString, MultiPolygon};
use std::fs;

/// Map the script/gcode unit enum to the gerber/excellon writer's `metric` flag.
fn is_metric(u: Units) -> bool {
    matches!(u, Units::Mm)
}

/// Collect a Region's geometry as (polygons, open-rings) for SVG/DXF writers.
///
/// Region objects only hold solid polygons, so the polyline list is empty.
fn region_geometry(obj: &Obj) -> Result<(MultiPolygon<f64>, Units), ScriptError> {
    match obj {
        Obj::Region(mp, u) => Ok((mp.clone(), *u)),
        other => Err(ScriptError::Other(format!(
            "expected a region/gerber object, got {}",
            other.kind()
        ))),
    }
}

/// Write `text` to `path`, mapping IO failure to [`ScriptError::Io`].
fn write(path: &str, text: &str) -> Result<(), ScriptError> {
    fs::write(path, text).map_err(|e| ScriptError::Io(e.to_string()))
}

/// `export_gerber <obj> <path>` — write a Region (gerber copper) to a Gerber file.
fn export_gerber(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "export_gerber <obj> <path>";
    let name = sarg(args, 0, USAGE)?;
    let path = sarg(args, 1, USAGE)?.to_string();
    let (mp, units) = region_geometry(ctx.get(name)?)?;
    let text = fc_gerber::write_gerber(&mp, is_metric(units));
    write(&path, &text)?;
    Ok(format!("wrote {path} (gerber)"))
}

/// `export_excellon <obj> <path>` — write an Excellon object to a drill file.
fn export_excellon(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "export_excellon <obj> <path>";
    let name = sarg(args, 0, USAGE)?;
    let path = sarg(args, 1, USAGE)?.to_string();
    match ctx.get(name)? {
        Obj::Excellon(e) => {
            let text = fc_excellon::write_excellon(e);
            write(&path, &text)?;
            Ok(format!("wrote {path} (excellon)"))
        }
        other => Err(ScriptError::Other(format!(
            "{name} is a {}, expected excellon",
            other.kind()
        ))),
    }
}

/// `export_svg <obj> <path>` — write a Region's geometry to an SVG file.
fn export_svg(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "export_svg <obj> <path>";
    let name = sarg(args, 0, USAGE)?;
    let path = sarg(args, 1, USAGE)?.to_string();
    let (mp, _units) = region_geometry(ctx.get(name)?)?;
    let text = fc_svg::write_svg(&mp, &[]);
    write(&path, &text)?;
    Ok(format!("wrote {path} (svg)"))
}

/// `export_dxf <obj> <path>` — write a Region's geometry to a DXF file.
fn export_dxf(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "export_dxf <obj> <path>";
    let name = sarg(args, 0, USAGE)?;
    let path = sarg(args, 1, USAGE)?.to_string();
    let (mp, _units) = region_geometry(ctx.get(name)?)?;
    let text = fc_dxf::write_dxf(&mp, &[]);
    write(&path, &text)?;
    Ok(format!("wrote {path} (dxf)"))
}

/// `write_gcode <cncobj> <path>` — write a Cnc object's rendered G-code to disk.
fn write_gcode(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "write_gcode <cncobj> <path>";
    let name = sarg(args, 0, USAGE)?;
    let path = sarg(args, 1, USAGE)?.to_string();
    match ctx.get(name)? {
        Obj::Cnc { gcode, .. } => {
            write(&path, gcode)?;
            Ok(format!("wrote {path} (gcode)"))
        }
        other => Err(ScriptError::Other(format!(
            "{name} is a {}, expected cnc",
            other.kind()
        ))),
    }
}

/// `open_gcode <path> <name>` — read a G-code file into a Cnc object.
///
/// The raw text is stored verbatim; there is no G-code geometry reader in the
/// workspace, so `paths` is left empty (the loaded text is the source of truth
/// for re-export via [`write_gcode`]).
fn open_gcode(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "open_gcode <path> <name>";
    let path = sarg(args, 0, USAGE)?;
    let name = sarg(args, 1, USAGE)?.to_string();
    let text = fs::read_to_string(path).map_err(|e| ScriptError::Io(e.to_string()))?;
    let lines = text.lines().count();
    ctx.put(
        name.clone(),
        Obj::Cnc { paths: Vec::new(), units: Units::Mm, gcode: text },
    );
    Ok(format!("loaded {name} (gcode, {lines} lines)"))
}

/// Escape a string for embedding in a minimal JSON document.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Append a ring's coordinates as a JSON array of `[x, y]` pairs.
fn ring_json(ring: &LineString<f64>) -> String {
    let pts: Vec<String> = ring
        .0
        .iter()
        .map(|c| format!("[{},{}]", c.x, c.y))
        .collect();
    format!("[{}]", pts.join(","))
}

/// Serialize one object's geometry to a minimal JSON value.
fn obj_json(obj: &Obj) -> String {
    match obj {
        Obj::Region(mp, u) => {
            let polys: Vec<String> = mp
                .0
                .iter()
                .map(|p| {
                    let ext = ring_json(p.exterior());
                    let holes: Vec<String> = p.interiors().iter().map(ring_json).collect();
                    format!("{{\"exterior\":{},\"interiors\":[{}]}}", ext, holes.join(","))
                })
                .collect();
            format!(
                "{{\"kind\":\"region\",\"units\":\"{}\",\"polygons\":[{}]}}",
                if is_metric(*u) { "mm" } else { "inch" },
                polys.join(",")
            )
        }
        Obj::Excellon(e) => {
            let tools: Vec<String> = e
                .tools
                .iter()
                .map(|(n, t)| {
                    let drills: Vec<String> =
                        t.drills.iter().map(|d| format!("[{},{}]", d.0, d.1)).collect();
                    format!(
                        "{{\"tool\":{},\"diameter\":{},\"drills\":[{}]}}",
                        n,
                        t.diameter,
                        drills.join(",")
                    )
                })
                .collect();
            format!(
                "{{\"kind\":\"excellon\",\"tools\":[{}]}}",
                tools.join(",")
            )
        }
        Obj::Cnc { gcode, .. } => {
            format!("{{\"kind\":\"cnc\",\"gcode\":\"{}\"}}", json_escape(gcode))
        }
    }
}

/// `save_project <path>` — serialize every object in the context to a minimal
/// JSON document (object name -> geometry) and write it to disk.
fn save_project(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "save_project <path>";
    let path = sarg(args, 0, USAGE)?.to_string();
    let entries: Vec<String> = ctx
        .objects
        .iter()
        .map(|(name, obj)| format!("\"{}\":{}", json_escape(name), obj_json(obj)))
        .collect();
    let n = entries.len();
    let json = format!("{{\"objects\":{{{}}}}}", entries.join(","));
    write(&path, &json)?;
    Ok(format!("saved {n} object(s) to {path}"))
}

/// Register the `io_cmds` command group.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("export_gerber", export_gerber),
        ("export_grb", export_gerber),
        ("egr", export_gerber),
        ("export_excellon", export_excellon),
        ("export_exc", export_excellon),
        ("ee", export_excellon),
        ("export_svg", export_svg),
        ("export_dxf", export_dxf),
        ("edxf", export_dxf),
        ("write_gcode", write_gcode),
        ("save_project", save_project),
        ("open_gcode", open_gcode),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::MultiPolygon;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    /// Build a unique temp path so parallel test runs don't collide.
    fn temp_path(stem: &str, ext: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("fc_script_iocmds_{stem}_{pid}_{n}.{ext}"))
    }

    fn square_region() -> Obj {
        let poly = fc_geo::centered_rect(5.0, 5.0, 10.0, 10.0);
        Obj::Region(MultiPolygon::new(vec![poly]), Units::Mm)
    }

    fn excellon() -> Obj {
        let tool = fc_excellon::Tool { diameter: 0.8, drills: vec![(10.0, 10.0), (20.0, 10.0)], slots: vec![] };
        let mut tools = std::collections::BTreeMap::new();
        tools.insert(1, tool);
        Obj::Excellon(fc_excellon::Excellon { units: fc_excellon::Units::Mm, tools })
    }

    #[test]
    fn commands_registered() {
        let names: Vec<&str> = commands().into_iter().map(|(n, _)| n).collect();
        for c in [
            "export_gerber", "export_grb", "egr", "export_excellon", "export_exc", "ee",
            "export_svg", "export_dxf", "edxf", "write_gcode", "save_project", "open_gcode",
        ] {
            assert!(names.contains(&c), "missing {c}");
        }
    }

    #[test]
    fn export_gerber_round_trips() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", square_region());
        let p = temp_path("g", "gbr");
        let ps = p.to_string_lossy().to_string();
        let msg = export_gerber(&mut ctx, &s(&["g", &ps])).unwrap();
        assert!(msg.contains("gerber"));
        assert!(p.exists());
        let text = fs::read_to_string(&p).unwrap();
        let parsed = fc_gerber::parse(&text).unwrap();
        let a = fc_geo::area(&parsed.solid_geometry);
        assert!((a - 100.0).abs() / 100.0 < 0.01, "round-trip area {a}");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn export_gerber_rejects_excellon() {
        let mut ctx = ScriptContext::new();
        ctx.put("e", excellon());
        let p = temp_path("never", "gbr");
        let ps = p.to_string_lossy().to_string();
        assert!(matches!(export_gerber(&mut ctx, &s(&["e", &ps])), Err(ScriptError::Other(_))));
        assert!(!p.exists());
    }

    #[test]
    fn export_excellon_round_trips() {
        let mut ctx = ScriptContext::new();
        ctx.put("e", excellon());
        let p = temp_path("e", "drl");
        let ps = p.to_string_lossy().to_string();
        let msg = export_excellon(&mut ctx, &s(&["e", &ps])).unwrap();
        assert!(msg.contains("excellon"));
        let text = fs::read_to_string(&p).unwrap();
        let re = fc_excellon::parse(&text).unwrap();
        assert_eq!(re.drill_count(), 2);
        assert!((re.tools[&1].diameter - 0.8).abs() < 1e-6);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn export_excellon_rejects_region() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", square_region());
        let p = temp_path("never", "drl");
        let ps = p.to_string_lossy().to_string();
        assert!(matches!(export_excellon(&mut ctx, &s(&["g", &ps])), Err(ScriptError::Other(_))));
        assert!(!p.exists());
    }

    #[test]
    fn export_svg_round_trips() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", square_region());
        let p = temp_path("g", "svg");
        let ps = p.to_string_lossy().to_string();
        export_svg(&mut ctx, &s(&["g", &ps])).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        let doc = fc_svg::parse(&text).unwrap();
        assert_eq!(doc.polygons.0.len(), 1);
        assert!((fc_geo::area(&doc.polygons) - 100.0).abs() < 1e-3);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn export_svg_usage_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(export_svg(&mut ctx, &s(&["onlyone"])), Err(ScriptError::Usage(_))));
    }

    #[test]
    fn export_dxf_round_trips() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", square_region());
        let p = temp_path("g", "dxf");
        let ps = p.to_string_lossy().to_string();
        export_dxf(&mut ctx, &s(&["g", &ps])).unwrap();
        let text = fs::read_to_string(&p).unwrap();
        let doc = fc_dxf::parse(&text).unwrap();
        assert_eq!(doc.polygons.0.len(), 1);
        assert!((fc_geo::area(&doc.polygons) - 100.0).abs() < 1e-3);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn write_gcode_writes_and_reopens() {
        let mut ctx = ScriptContext::new();
        ctx.put("job", Obj::Cnc { paths: vec![], units: Units::Mm, gcode: "G21\nG90\nM30\n".into() });
        let p = temp_path("job", "nc");
        let ps = p.to_string_lossy().to_string();
        let msg = write_gcode(&mut ctx, &s(&["job", &ps])).unwrap();
        assert!(msg.contains("gcode"));
        assert_eq!(fs::read_to_string(&p).unwrap(), "G21\nG90\nM30\n");

        // Reopen the file into a fresh Cnc object.
        let open_msg = open_gcode(&mut ctx, &s(&[&ps, "reopened"])).unwrap();
        assert!(open_msg.contains("3 lines"));
        if let Obj::Cnc { gcode, .. } = ctx.get("reopened").unwrap() {
            assert!(gcode.contains("M30"));
        } else {
            panic!("expected cnc");
        }
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn write_gcode_rejects_region() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", square_region());
        let p = temp_path("never", "nc");
        let ps = p.to_string_lossy().to_string();
        assert!(matches!(write_gcode(&mut ctx, &s(&["g", &ps])), Err(ScriptError::Other(_))));
        assert!(!p.exists());
    }

    #[test]
    fn open_gcode_missing_file_errors() {
        let mut ctx = ScriptContext::new();
        let p = temp_path("missing", "nc");
        let ps = p.to_string_lossy().to_string();
        assert!(matches!(open_gcode(&mut ctx, &s(&[&ps, "x"])), Err(ScriptError::Io(_))));
    }

    #[test]
    fn save_project_writes_json() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", square_region());
        ctx.put("e", excellon());
        ctx.put("job", Obj::Cnc { paths: vec![], units: Units::Mm, gcode: "G21\n".into() });
        let p = temp_path("proj", "json");
        let ps = p.to_string_lossy().to_string();
        let msg = save_project(&mut ctx, &s(&[&ps])).unwrap();
        assert!(msg.contains("3 object"));
        let text = fs::read_to_string(&p).unwrap();
        assert!(text.contains("\"objects\""));
        assert!(text.contains("\"kind\":\"region\""));
        assert!(text.contains("\"kind\":\"excellon\""));
        assert!(text.contains("\"kind\":\"cnc\""));
        assert!(text.contains("\"g\""));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn save_project_usage_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(save_project(&mut ctx, &[]), Err(ScriptError::Usage(_))));
    }
}
