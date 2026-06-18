//! `fc-script` — a headless scripting/batch engine for the FlatCAM Rust port.
//!
//! The parity equivalent of FlatCAM's Tcl shell (`tclCommands/`): a small command
//! interpreter over a [`ScriptContext`] (a named collection of geometry/CAM
//! objects). Each command is a `fn(&mut ScriptContext, &[String]) -> Result<…>`;
//! command groups live in separate modules and register themselves, so they can
//! be authored independently. A script is a sequence of whitespace-tokenised
//! lines (`#` starts a comment).

use fc_gcode::{Polyline, Units};
use fc_geo::MultiPolygon;
use std::collections::BTreeMap;

mod analyze_cmds;
mod cam;
mod edit_cmds;
mod gen;
mod geo_ops;
mod io;
mod query;
mod transform_cmds;

#[derive(thiserror::Error, Debug)]
pub enum ScriptError {
    #[error("io error: {0}")]
    Io(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("unknown command: {0}")]
    Unknown(String),
    #[error("usage: {0}")]
    Usage(String),
    #[error("object not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

/// An object held by a script run.
pub enum Obj {
    Region(MultiPolygon<f64>, Units),
    Excellon(fc_excellon::Excellon),
    Cnc {
        paths: Vec<Polyline>,
        units: Units,
        gcode: String,
    },
}

impl Obj {
    pub fn kind(&self) -> &'static str {
        match self {
            Obj::Region(..) => "region",
            Obj::Excellon(_) => "excellon",
            Obj::Cnc { .. } => "cnc",
        }
    }
}

/// Build a CNC object from milling paths, rendering G-code with GRBL.
pub fn make_cnc(paths: Vec<Polyline>, units: Units, tool_dia: f64) -> Obj {
    let params = fc_gcode::JobParams { units, tool_diameter: tool_dia, ..Default::default() };
    let job = fc_gcode::CncJob {
        params,
        kind: fc_gcode::JobKind::Mill { paths: paths.clone() },
    };
    let gcode = job.to_gcode(&fc_gcode::Grbl);
    Obj::Cnc { paths, units, gcode }
}

/// The mutable state a script operates on.
#[derive(Default)]
pub struct ScriptContext {
    pub objects: BTreeMap<String, Obj>,
}

impl ScriptContext {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn put(&mut self, name: impl Into<String>, obj: Obj) {
        self.objects.insert(name.into(), obj);
    }
    pub fn get(&self, name: &str) -> Result<&Obj, ScriptError> {
        self.objects.get(name).ok_or_else(|| ScriptError::NotFound(name.to_string()))
    }
    /// Clone the geometry of a Region object (errors if missing/not a region).
    pub fn region(&self, name: &str) -> Result<(MultiPolygon<f64>, Units), ScriptError> {
        match self.get(name)? {
            Obj::Region(mp, u) => Ok((mp.clone(), *u)),
            other => Err(ScriptError::Other(format!("{name} is a {}, expected region", other.kind()))),
        }
    }
    pub fn names(&self) -> Vec<String> {
        self.objects.keys().cloned().collect()
    }
}

/// Signature of a script command.
pub type CmdFn = fn(&mut ScriptContext, &[String]) -> Result<String, ScriptError>;

/// Command registry.
pub struct Registry {
    map: BTreeMap<&'static str, CmdFn>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        let mut map = BTreeMap::new();
        for group in [
            io::commands(),
            cam::commands(),
            geo_ops::commands(),
            query::commands(),
            gen::commands(),
            transform_cmds::commands(),
            analyze_cmds::commands(),
            edit_cmds::commands(),
        ] {
            for (name, f) in group {
                map.insert(name, f);
            }
        }
        Registry { map }
    }

    pub fn command_names(&self) -> Vec<&'static str> {
        self.map.keys().copied().collect()
    }

    /// Run one command line (whitespace-tokenised). Blank/`#` lines return "".
    pub fn run_line(&self, ctx: &mut ScriptContext, line: &str) -> Result<String, ScriptError> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return Ok(String::new());
        }
        let toks: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
        let cmd = toks[0].as_str();
        let f = self.map.get(cmd).ok_or_else(|| ScriptError::Unknown(cmd.to_string()))?;
        f(ctx, &toks[1..])
    }

    /// Run a whole script, returning each command's (non-empty) output line.
    pub fn run_script(&self, ctx: &mut ScriptContext, text: &str) -> Result<Vec<String>, ScriptError> {
        let mut out = Vec::new();
        for line in text.lines() {
            let r = self.run_line(ctx, line)?;
            if !r.is_empty() {
                out.push(r);
            }
        }
        Ok(out)
    }
}

// ----- argument helpers for command modules -----

pub fn sarg<'a>(args: &'a [String], i: usize, usage: &str) -> Result<&'a str, ScriptError> {
    args.get(i).map(|s| s.as_str()).ok_or_else(|| ScriptError::Usage(usage.to_string()))
}

pub fn farg(args: &[String], i: usize, usage: &str) -> Result<f64, ScriptError> {
    let s = sarg(args, i, usage)?;
    s.parse::<f64>().map_err(|_| ScriptError::Parse(format!("'{s}' is not a number")))
}

pub fn iarg(args: &[String], i: usize, usage: &str) -> Result<i64, ScriptError> {
    let s = sarg(args, i, usage)?;
    s.parse::<i64>().map_err(|_| ScriptError::Parse(format!("'{s}' is not an integer")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_core_commands() {
        let r = Registry::new();
        let names = r.command_names();
        for c in ["list", "isolate", "new_rect", "offset", "open_gerber"] {
            assert!(names.contains(&c), "missing command {c}");
        }
    }

    #[test]
    fn unknown_command_errors() {
        let r = Registry::new();
        let mut ctx = ScriptContext::new();
        assert!(r.run_line(&mut ctx, "nope_cmd").is_err());
        assert!(r.run_line(&mut ctx, "# comment").unwrap().is_empty());
    }

    #[test]
    fn end_to_end_rect_isolate() {
        let r = Registry::new();
        let mut ctx = ScriptContext::new();
        r.run_line(&mut ctx, "new_rect board 0 0 10 10").unwrap();
        r.run_line(&mut ctx, "isolate board board_iso 0.4").unwrap();
        assert!(ctx.get("board_iso").is_ok());
        assert_eq!(ctx.get("board_iso").unwrap().kind(), "cnc");
    }
}
