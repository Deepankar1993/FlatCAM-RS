//! `io` command group — loading source files (Gerber/Excellon/SVG/DXF) into the
//! script context and exporting generated G-code.
//!
//! Parity with FlatCAM's Tcl file commands (`open_gerber`, `open_excellon`,
//! `write_gcode`, …). Files are read from disk, parsed by the matching
//! `fc_*` parser crate, and stored as [`crate::Obj`] entries in the context.

use crate::{sarg, Obj, ScriptContext, ScriptError};
use fc_gcode::Units;
use std::fs;

/// Read a file to a string, mapping any IO failure to [`ScriptError::Io`].
fn read(path: &str) -> Result<String, ScriptError> {
    fs::read_to_string(path).map_err(|e| ScriptError::Io(e.to_string()))
}

fn gerber_units(u: fc_gerber::Units) -> Units {
    match u {
        fc_gerber::Units::Mm => Units::Mm,
        fc_gerber::Units::Inch => Units::Inch,
    }
}

/// `open_gerber <path> <name>` — parse a Gerber file into a Region object.
fn open_gerber(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "open_gerber <path> <name>";
    let path = sarg(args, 0, USAGE)?;
    let name = sarg(args, 1, USAGE)?.to_string();
    let text = read(path)?;
    let g = fc_gerber::parse(&text).map_err(|e| ScriptError::Parse(e.to_string()))?;
    let units = gerber_units(g.units);
    ctx.put(name.clone(), Obj::Region(g.solid_geometry, units));
    Ok(format!("loaded {name} (gerber)"))
}

/// `open_excellon <path> <name>` — parse an Excellon file into an Excellon object.
fn open_excellon(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "open_excellon <path> <name>";
    let path = sarg(args, 0, USAGE)?;
    let name = sarg(args, 1, USAGE)?.to_string();
    let text = read(path)?;
    let e = fc_excellon::parse(&text).map_err(|e| ScriptError::Parse(e.to_string()))?;
    let drills: usize = e.tools.values().map(|t| t.drills.len()).sum();
    ctx.put(name.clone(), Obj::Excellon(e));
    Ok(format!("loaded {name} (excellon, {drills} drills)"))
}

/// `open_svg <path> <name>` — parse an SVG file into a Region object (mm).
fn open_svg(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "open_svg <path> <name>";
    let path = sarg(args, 0, USAGE)?;
    let name = sarg(args, 1, USAGE)?.to_string();
    let text = read(path)?;
    let svg = fc_svg::parse(&text).map_err(|e| ScriptError::Parse(e.to_string()))?;
    ctx.put(name.clone(), Obj::Region(svg.polygons, Units::Mm));
    Ok(format!("loaded {name} (svg)"))
}

/// `open_dxf <path> <name>` — parse a DXF file into a Region object (mm).
fn open_dxf(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "open_dxf <path> <name>";
    let path = sarg(args, 0, USAGE)?;
    let name = sarg(args, 1, USAGE)?.to_string();
    let text = read(path)?;
    let d = fc_dxf::parse(&text).map_err(|e| ScriptError::Parse(e.to_string()))?;
    ctx.put(name.clone(), Obj::Region(d.polygons, Units::Mm));
    Ok(format!("loaded {name} (dxf)"))
}

/// `export_gcode <name> <path>` — write a CNC object's G-code to disk.
fn export_gcode(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "export_gcode <name> <path>";
    let name = sarg(args, 0, USAGE)?;
    let path = sarg(args, 1, USAGE)?.to_string();
    match ctx.get(name)? {
        Obj::Cnc { gcode, .. } => {
            fs::write(&path, gcode).map_err(|e| ScriptError::Io(e.to_string()))?;
            Ok(format!("wrote {path}"))
        }
        _ => Err(ScriptError::Other("not a cnc object".into())),
    }
}

/// Register the `io` command group.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("open_gerber", open_gerber),
        ("open_excellon", open_excellon),
        ("open_svg", open_svg),
        ("open_dxf", open_dxf),
        ("export_gcode", export_gcode),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a unique temp path so parallel test runs don't collide.
    fn temp_path(stem: &str, ext: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("fc_script_io_{stem}_{pid}_{n}.{ext}"))
    }

    #[test]
    fn open_svg_loads_region() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg">
            <rect x="0" y="0" width="10" height="5"/>
        </svg>"#;
        let p = temp_path("rect", "svg");
        fs::write(&p, svg).unwrap();

        let mut ctx = ScriptContext::new();
        let path_s = p.to_string_lossy().to_string();
        let args = vec![path_s, "board".to_string()];
        let msg = open_svg(&mut ctx, &args).unwrap();
        assert_eq!(msg, "loaded board (svg)");

        let obj = ctx.get("board").unwrap();
        assert_eq!(obj.kind(), "region");
        match obj {
            Obj::Region(mp, u) => {
                assert_eq!(*u, Units::Mm);
                assert_eq!(mp.0.len(), 1, "rect should yield one polygon");
            }
            _ => panic!("expected region"),
        }

        let _ = fs::remove_file(&p);
    }

    #[test]
    fn open_svg_missing_args_errors() {
        let mut ctx = ScriptContext::new();
        let args = vec!["only_one".to_string()];
        assert!(matches!(open_svg(&mut ctx, &args), Err(ScriptError::Usage(_))));
    }

    #[test]
    fn open_missing_file_errors() {
        let mut ctx = ScriptContext::new();
        let p = temp_path("does_not_exist", "svg");
        let args = vec![p.to_string_lossy().to_string(), "x".to_string()];
        assert!(matches!(open_svg(&mut ctx, &args), Err(ScriptError::Io(_))));
    }

    #[test]
    fn export_gcode_rejects_region() {
        let mut ctx = ScriptContext::new();
        // Put a region object directly, then try to export it as g-code.
        ctx.put("board", Obj::Region(fc_geo::MultiPolygon::new(vec![]), Units::Mm));
        let out = temp_path("never", "nc");
        let args = vec!["board".to_string(), out.to_string_lossy().to_string()];
        let r = export_gcode(&mut ctx, &args);
        assert!(matches!(r, Err(ScriptError::Other(_))));
        // No file should have been written.
        assert!(!out.exists());
    }

    #[test]
    fn export_gcode_writes_cnc() {
        let mut ctx = ScriptContext::new();
        let cnc = Obj::Cnc {
            paths: vec![vec![(0.0, 0.0), (1.0, 0.0)]],
            units: Units::Mm,
            gcode: "G21\nM2\n".to_string(),
        };
        ctx.put("job", cnc);
        let out = temp_path("export", "nc");
        let out_s = out.to_string_lossy().to_string();
        let args = vec!["job".to_string(), out_s.clone()];
        let msg = export_gcode(&mut ctx, &args).unwrap();
        assert_eq!(msg, format!("wrote {out_s}"));
        let written = fs::read_to_string(&out).unwrap();
        assert_eq!(written, "G21\nM2\n");
        let _ = fs::remove_file(&out);
    }

    #[test]
    fn export_gcode_unknown_object_errors() {
        let mut ctx = ScriptContext::new();
        let args = vec!["nope".to_string(), "x.nc".to_string()];
        assert!(matches!(export_gcode(&mut ctx, &args), Err(ScriptError::NotFound(_))));
    }
}
