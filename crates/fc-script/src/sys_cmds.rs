//! `sys_cmds` — system/options, project, and remaining object-creation commands.
//!
//! Fills the final coverage gaps versus upstream FlatCAM's `tclCommands/`:
//!
//! * System/options: `set_sys`/`setsys`, `get_sys`/`getsys`, `list_sys`/`listsys`,
//!   `options`, `get_path`, `set_path` — a simple per-context settings map plus a
//!   fallback folder path (both stored on the [`ScriptContext`]).
//! * Project/object: `open_project` (inverse of `io_cmds::save_project`),
//!   `add_aperture`, `aligndrill`/`aligndrillgrid` (alignment drills, reusing the
//!   `build_cmds` add-drill logic), and `split_geometries`/`split_geometry`.
//!
//! Each command follows the house pattern: parse args with the `sarg`/`farg`
//! helpers, read/insert [`crate::Obj`] entries on the [`ScriptContext`], and
//! return a short human-readable message.

use crate::{farg, iarg, sarg, Obj, ScriptContext, ScriptError};
use fc_excellon::{Excellon, Tool};
use fc_gcode::Units;
use fc_geo::{Coord, LineString, MultiPolygon, Polygon};
use std::collections::BTreeMap;
use std::fs;

/// Register the system/options + project command group.
pub fn commands() -> Vec<(&'static str, crate::CmdFn)> {
    vec![
        ("set_sys", set_sys),
        ("setsys", set_sys),
        ("get_sys", get_sys),
        ("getsys", get_sys),
        ("list_sys", list_sys),
        ("listsys", list_sys),
        ("options", options),
        ("get_path", get_path),
        ("set_path", set_path),
        ("open_project", open_project),
        ("add_aperture", add_aperture),
        ("aligndrill", aligndrill),
        ("aligndrillgrid", aligndrillgrid),
        ("split_geometries", split_geometries),
        ("split_geometry", split_geometries),
    ]
}

// ----- system / options -----

/// `set_sys <name> <value>` / `setsys` — set a system variable on the context.
fn set_sys(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "set_sys <name> <value>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let value = sarg(args, 1, USAGE)?.to_string();
    ctx.sysvars.insert(name.clone(), value.clone());
    Ok(format!("{name} = {value}"))
}

/// `get_sys <name>` / `getsys` — return a system variable's value (error if unset).
fn get_sys(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "get_sys <name>";
    let name = sarg(args, 0, USAGE)?;
    ctx.sysvars
        .get(name)
        .cloned()
        .ok_or_else(|| ScriptError::Other(format!("system variable '{name}' is not set")))
}

/// `list_sys [filter]` / `listsys` — list variable names, optionally substring-filtered.
fn list_sys(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    let filter = args.first().map(|s| s.as_str());
    let names: Vec<&str> = ctx
        .sysvars
        .keys()
        .map(|k| k.as_str())
        .filter(|k| filter.is_none_or(|f| k.contains(f)))
        .collect();
    Ok(names.join("\n"))
}

/// `options <obj>` — return an object's stored options/metadata as text.
///
/// For a Region this reports kind, bounds, area and polygon count, reusing the
/// existing `fc_geo` query helpers.
fn options(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "options <obj>";
    let name = sarg(args, 0, USAGE)?;
    let obj = ctx.get(name)?;
    let mut lines = vec![format!("name: {name}"), format!("kind: {}", obj.kind())];
    match obj {
        Obj::Region(mp, units) => {
            lines.push(format!(
                "units: {}",
                if matches!(units, Units::Mm) { "mm" } else { "inch" }
            ));
            match fc_geo::bounds(mp) {
                Some((minx, miny, maxx, maxy)) => {
                    lines.push(format!("bounds: {minx} {miny} {maxx} {maxy}"))
                }
                None => lines.push("bounds: (empty)".to_string()),
            }
            lines.push(format!("area: {:.4}", fc_geo::area(mp)));
            lines.push(format!("polygons: {}", mp.0.len()));
        }
        Obj::Excellon(e) => {
            lines.push(format!("tools: {}", e.tools.len()));
            lines.push(format!("drills: {}", e.drill_count()));
        }
        Obj::Cnc { paths, units, gcode } => {
            lines.push(format!(
                "units: {}",
                if matches!(units, Units::Mm) { "mm" } else { "inch" }
            ));
            lines.push(format!("paths: {}", paths.len()));
            lines.push(format!("gcode_lines: {}", gcode.lines().count()));
        }
    }
    Ok(lines.join("\n"))
}

/// `get_path` — return the context's fallback folder path (empty if unset).
fn get_path(ctx: &mut ScriptContext, _args: &[String]) -> Result<String, ScriptError> {
    Ok(ctx.path.clone())
}

/// `set_path <path>` — set the fallback folder path.
fn set_path(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "set_path <path>";
    let path = sarg(args, 0, USAGE)?.to_string();
    ctx.path = path.clone();
    Ok(format!("path = {path}"))
}

// ----- project: open_project (inverse of io_cmds::save_project) -----

/// A tiny hand-rolled JSON reader, sufficient for the minimal document written
/// by `io_cmds::save_project`. We avoid a serde dependency to match the
/// hand-written serializer in `io_cmds`.
mod json {
    /// A parsed JSON value (only the subset save_project emits).
    #[derive(Debug, Clone)]
    pub enum Value {
        Str(String),
        Num(f64),
        Arr(Vec<Value>),
        Obj(Vec<(String, Value)>),
    }

    impl Value {
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::Str(s) => Some(s),
                _ => None,
            }
        }
        pub fn as_num(&self) -> Option<f64> {
            match self {
                Value::Num(n) => Some(*n),
                _ => None,
            }
        }
        pub fn as_arr(&self) -> Option<&[Value]> {
            match self {
                Value::Arr(a) => Some(a),
                _ => None,
            }
        }
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }
        /// Iterate object members in order (for the top-level object map).
        pub fn entries(&self) -> Option<&[(String, Value)]> {
            match self {
                Value::Obj(pairs) => Some(pairs),
                _ => None,
            }
        }
    }

    struct Parser<'a> {
        bytes: &'a [u8],
        i: usize,
    }

    impl<'a> Parser<'a> {
        fn new(s: &'a str) -> Self {
            Parser { bytes: s.as_bytes(), i: 0 }
        }
        fn skip_ws(&mut self) {
            while self.i < self.bytes.len() && self.bytes[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
        }
        fn peek(&self) -> Option<u8> {
            self.bytes.get(self.i).copied()
        }
        fn expect(&mut self, c: u8) -> Result<(), String> {
            self.skip_ws();
            if self.peek() == Some(c) {
                self.i += 1;
                Ok(())
            } else {
                Err(format!("expected '{}' at byte {}", c as char, self.i))
            }
        }
        fn parse_value(&mut self) -> Result<Value, String> {
            self.skip_ws();
            match self.peek() {
                Some(b'"') => self.parse_string().map(Value::Str),
                Some(b'{') => self.parse_object(),
                Some(b'[') => self.parse_array(),
                Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
                other => Err(format!("unexpected token {:?} at byte {}", other.map(|c| c as char), self.i)),
            }
        }
        fn parse_string(&mut self) -> Result<String, String> {
            self.expect(b'"')?;
            let mut out = String::new();
            while let Some(c) = self.peek() {
                self.i += 1;
                match c {
                    b'"' => return Ok(out),
                    b'\\' => {
                        let e = self.peek().ok_or("unterminated escape")?;
                        self.i += 1;
                        match e {
                            b'"' => out.push('"'),
                            b'\\' => out.push('\\'),
                            b'/' => out.push('/'),
                            b'n' => out.push('\n'),
                            b'r' => out.push('\r'),
                            b't' => out.push('\t'),
                            b'u' => {
                                let hex = self
                                    .bytes
                                    .get(self.i..self.i + 4)
                                    .ok_or("bad \\u escape")?;
                                let code = u32::from_str_radix(
                                    std::str::from_utf8(hex).map_err(|e| e.to_string())?,
                                    16,
                                )
                                .map_err(|e| e.to_string())?;
                                self.i += 4;
                                out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                            }
                            o => return Err(format!("bad escape \\{}", o as char)),
                        }
                    }
                    _ => {
                        // Re-decode the full UTF-8 char starting at the byte we consumed.
                        let start = self.i - 1;
                        let mut end = self.i;
                        while end < self.bytes.len() && (self.bytes[end] & 0xC0) == 0x80 {
                            end += 1;
                        }
                        out.push_str(std::str::from_utf8(&self.bytes[start..end]).map_err(|e| e.to_string())?);
                        self.i = end;
                    }
                }
            }
            Err("unterminated string".to_string())
        }
        fn parse_number(&mut self) -> Result<Value, String> {
            let start = self.i;
            while let Some(c) = self.peek() {
                if c == b'-' || c == b'+' || c == b'.' || c == b'e' || c == b'E' || c.is_ascii_digit()
                {
                    self.i += 1;
                } else {
                    break;
                }
            }
            let s = std::str::from_utf8(&self.bytes[start..self.i]).map_err(|e| e.to_string())?;
            s.parse::<f64>().map(Value::Num).map_err(|e| e.to_string())
        }
        fn parse_array(&mut self) -> Result<Value, String> {
            self.expect(b'[')?;
            let mut items = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b']') {
                self.i += 1;
                return Ok(Value::Arr(items));
            }
            loop {
                items.push(self.parse_value()?);
                self.skip_ws();
                match self.peek() {
                    Some(b',') => {
                        self.i += 1;
                    }
                    Some(b']') => {
                        self.i += 1;
                        break;
                    }
                    other => {
                        return Err(format!("expected ',' or ']' got {:?}", other.map(|c| c as char)))
                    }
                }
            }
            Ok(Value::Arr(items))
        }
        fn parse_object(&mut self) -> Result<Value, String> {
            self.expect(b'{')?;
            let mut pairs = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.i += 1;
                return Ok(Value::Obj(pairs));
            }
            loop {
                self.skip_ws();
                let key = self.parse_string()?;
                self.expect(b':')?;
                let val = self.parse_value()?;
                pairs.push((key, val));
                self.skip_ws();
                match self.peek() {
                    Some(b',') => {
                        self.i += 1;
                    }
                    Some(b'}') => {
                        self.i += 1;
                        break;
                    }
                    other => {
                        return Err(format!("expected ',' or '}}' got {:?}", other.map(|c| c as char)))
                    }
                }
            }
            Ok(Value::Obj(pairs))
        }
    }

    /// Parse a JSON document into a [`Value`].
    pub fn parse(s: &str) -> Result<Value, String> {
        let mut p = Parser::new(s);
        let v = p.parse_value()?;
        Ok(v)
    }
}

/// Reconstruct a ring (LineString) from a JSON array of `[x, y]` pairs.
fn ring_from_json(v: &json::Value) -> Result<LineString<f64>, ScriptError> {
    let arr = v.as_arr().ok_or_else(|| ScriptError::Parse("ring is not an array".into()))?;
    let mut pts = Vec::with_capacity(arr.len());
    for p in arr {
        let pair = p.as_arr().ok_or_else(|| ScriptError::Parse("point is not an array".into()))?;
        if pair.len() != 2 {
            return Err(ScriptError::Parse("point must have 2 coords".into()));
        }
        let x = pair[0].as_num().ok_or_else(|| ScriptError::Parse("x is not a number".into()))?;
        let y = pair[1].as_num().ok_or_else(|| ScriptError::Parse("y is not a number".into()))?;
        pts.push(Coord { x, y });
    }
    Ok(LineString::new(pts))
}

/// Reconstruct a single [`Obj`] from its JSON value (as written by save_project).
fn obj_from_json(v: &json::Value) -> Result<Obj, ScriptError> {
    let kind = v
        .get("kind")
        .and_then(|k| k.as_str())
        .ok_or_else(|| ScriptError::Parse("object missing 'kind'".into()))?;
    match kind {
        "region" => {
            let units = match v.get("units").and_then(|u| u.as_str()) {
                Some("inch") => Units::Inch,
                _ => Units::Mm,
            };
            let polys_v = v
                .get("polygons")
                .and_then(|p| p.as_arr())
                .ok_or_else(|| ScriptError::Parse("region missing 'polygons'".into()))?;
            let mut polys = Vec::with_capacity(polys_v.len());
            for pv in polys_v {
                let ext = pv
                    .get("exterior")
                    .ok_or_else(|| ScriptError::Parse("polygon missing 'exterior'".into()))?;
                let exterior = ring_from_json(ext)?;
                let interiors_v = pv.get("interiors").and_then(|i| i.as_arr()).unwrap_or(&[]);
                let mut interiors = Vec::with_capacity(interiors_v.len());
                for iv in interiors_v {
                    interiors.push(ring_from_json(iv)?);
                }
                polys.push(Polygon::new(exterior, interiors));
            }
            Ok(Obj::Region(MultiPolygon::new(polys), units))
        }
        "excellon" => {
            let tools_v = v
                .get("tools")
                .and_then(|t| t.as_arr())
                .ok_or_else(|| ScriptError::Parse("excellon missing 'tools'".into()))?;
            let mut tools: BTreeMap<i32, Tool> = BTreeMap::new();
            for tv in tools_v {
                let num = tv
                    .get("tool")
                    .and_then(|n| n.as_num())
                    .ok_or_else(|| ScriptError::Parse("tool missing 'tool' number".into()))?
                    as i32;
                let diameter = tv
                    .get("diameter")
                    .and_then(|d| d.as_num())
                    .ok_or_else(|| ScriptError::Parse("tool missing 'diameter'".into()))?;
                let drills_v = tv.get("drills").and_then(|d| d.as_arr()).unwrap_or(&[]);
                let mut drills = Vec::with_capacity(drills_v.len());
                for dv in drills_v {
                    let pair = dv
                        .as_arr()
                        .ok_or_else(|| ScriptError::Parse("drill is not an array".into()))?;
                    if pair.len() != 2 {
                        return Err(ScriptError::Parse("drill must have 2 coords".into()));
                    }
                    let x = pair[0].as_num().ok_or_else(|| ScriptError::Parse("drill x".into()))?;
                    let y = pair[1].as_num().ok_or_else(|| ScriptError::Parse("drill y".into()))?;
                    drills.push((x, y));
                }
                tools.insert(num, Tool { diameter, drills, slots: vec![] });
            }
            Ok(Obj::Excellon(Excellon { units: fc_excellon::Units::Mm, tools }))
        }
        "cnc" => {
            let gcode = v
                .get("gcode")
                .and_then(|g| g.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Obj::Cnc { paths: vec![], units: Units::Mm, gcode })
        }
        other => Err(ScriptError::Parse(format!("unknown object kind '{other}'"))),
    }
}

/// `open_project <path>` — load a project written by `save_project`, restoring
/// every object (name + geometry) into the context. Inverse of save_project.
fn open_project(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "open_project <path>";
    let path = sarg(args, 0, USAGE)?;
    let text = fs::read_to_string(path).map_err(|e| ScriptError::Io(e.to_string()))?;
    let doc = json::parse(&text).map_err(ScriptError::Parse)?;
    let objects = doc
        .get("objects")
        .ok_or_else(|| ScriptError::Parse("project missing 'objects'".into()))?;
    let entries = objects
        .entries()
        .ok_or_else(|| ScriptError::Parse("'objects' is not an object".into()))?;
    let mut n = 0usize;
    for (name, ov) in entries {
        let obj = obj_from_json(ov)?;
        ctx.put(name.clone(), obj);
        n += 1;
    }
    Ok(format!("loaded {n} object(s) from {path}"))
}

// ----- aperture + alignment drills + split -----

/// `add_aperture <gerberobj> <apid> <type> <size>` — record an aperture
/// definition on a Region/Gerber object.
///
/// The script `Obj::Region` variant carries no aperture table (it stores rendered
/// solid geometry), so the aperture is recorded in the context's `sysvars` map
/// under a namespaced key (`aperture.<obj>.<apid>`), which both validates the
/// request and makes the definition queryable via `get_sys`/`list_sys`. This is
/// the closest real storage available without changing the `Obj` enum.
fn add_aperture(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "add_aperture <gerberobj> <apid> <type> <size>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let apid = iarg(args, 1, USAGE)?;
    let ap_type = sarg(args, 2, USAGE)?.to_string();
    let size = farg(args, 3, USAGE)?;
    // Validate the target is a region/gerber object.
    match ctx.get(&name)? {
        Obj::Region(..) => {}
        other => {
            return Err(ScriptError::Other(format!(
                "{name} is a {}, expected region/gerber",
                other.kind()
            )))
        }
    }
    if size <= 0.0 {
        return Err(ScriptError::Other("aperture size must be positive".into()));
    }
    let key = format!("aperture.{name}.{apid}");
    ctx.sysvars.insert(key, format!("{ap_type} {size}"));
    Ok(format!("aperture {apid} ({ap_type} {size}) added to '{name}'"))
}

/// Append a single drill of the given diameter to an Excellon (create if
/// missing), reusing the diameter->tool allocation logic shape from `build_cmds`.
fn append_drill(ctx: &mut ScriptContext, name: &str, x: f64, y: f64, dia: f64) -> Result<(), ScriptError> {
    let mut e = match ctx.objects.get(name) {
        None => Excellon { units: fc_excellon::Units::Mm, tools: BTreeMap::new() },
        Some(Obj::Excellon(e)) => e.clone(),
        Some(other) => {
            return Err(ScriptError::Other(format!(
                "{name} is a {}, expected excellon",
                other.kind()
            )))
        }
    };
    // Find an existing tool of matching diameter, else allocate the next number.
    let tool = e
        .tools
        .iter()
        .find(|(_, t)| (t.diameter - dia).abs() < 1e-9)
        .map(|(&n, _)| n)
        .unwrap_or_else(|| {
            let next = e.tools.keys().copied().max().unwrap_or(0) + 1;
            e.tools.insert(next, Tool { diameter: dia, drills: vec![], slots: vec![] });
            next
        });
    e.tools.get_mut(&tool).unwrap().drills.push((x, y));
    ctx.put(name.to_string(), Obj::Excellon(e));
    Ok(())
}

/// `aligndrill <excobj> <x> <y> <dia>` — create/append an alignment drill in an
/// Excellon object (create the object if missing).
fn aligndrill(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "aligndrill <excobj> <x> <y> <dia>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let x = farg(args, 1, USAGE)?;
    let y = farg(args, 2, USAGE)?;
    let dia = farg(args, 3, USAGE)?;
    if dia <= 0.0 {
        return Err(ScriptError::Other("drill diameter must be positive".into()));
    }
    append_drill(ctx, &name, x, y, dia)?;
    Ok(format!("alignment drill (dia {dia}) at ({x}, {y}) -> '{name}'"))
}

/// `aligndrillgrid <excobj> <x0> <y0> <dx> <dy> <cols> <rows> <dia>` — create a
/// grid of alignment drills.
fn aligndrillgrid(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "aligndrillgrid <excobj> <x0> <y0> <dx> <dy> <cols> <rows> <dia>";
    let name = sarg(args, 0, USAGE)?.to_string();
    let x0 = farg(args, 1, USAGE)?;
    let y0 = farg(args, 2, USAGE)?;
    let dx = farg(args, 3, USAGE)?;
    let dy = farg(args, 4, USAGE)?;
    let cols = iarg(args, 5, USAGE)?;
    let rows = iarg(args, 6, USAGE)?;
    let dia = farg(args, 7, USAGE)?;
    if cols <= 0 || rows <= 0 {
        return Err(ScriptError::Other("cols and rows must be positive".into()));
    }
    if dia <= 0.0 {
        return Err(ScriptError::Other("drill diameter must be positive".into()));
    }
    for r in 0..rows {
        for c in 0..cols {
            let x = x0 + dx * c as f64;
            let y = y0 + dy * r as f64;
            append_drill(ctx, &name, x, y, dia)?;
        }
    }
    let n = cols * rows;
    Ok(format!("{n} alignment drill(s) (dia {dia}) -> '{name}'"))
}

/// `split_geometries` / `split_geometry <srcobj> <prefix>` — create one new
/// Region per disjoint polygon (named `<prefix>_<n>`, 1-based).
fn split_geometries(ctx: &mut ScriptContext, args: &[String]) -> Result<String, ScriptError> {
    const USAGE: &str = "split_geometries <srcobj> <prefix>";
    let src = sarg(args, 0, USAGE)?.to_string();
    let prefix = sarg(args, 1, USAGE)?.to_string();
    let (mp, units) = ctx.region(&src)?;
    if mp.0.is_empty() {
        return Err(ScriptError::Other(format!("'{src}' has no geometry to split")));
    }
    let mut names = Vec::with_capacity(mp.0.len());
    for (i, poly) in mp.0.into_iter().enumerate() {
        let name = format!("{prefix}_{}", i + 1);
        ctx.put(name.clone(), Obj::Region(MultiPolygon::new(vec![poly]), units));
        names.push(name);
    }
    let n = names.len();
    Ok(format!("split '{src}' into {n} object(s): {}", names.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn square_region(cx: f64, cy: f64, side: f64) -> Obj {
        let poly = fc_geo::centered_rect(cx, cy, side, side);
        Obj::Region(MultiPolygon::new(vec![poly]), Units::Mm)
    }

    /// Build a unique temp path so parallel test runs don't collide.
    fn temp_path(stem: &str, ext: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("fc_script_syscmds_{stem}_{pid}_{n}.{ext}"))
    }

    #[test]
    fn commands_registered() {
        let names: Vec<&str> = commands().into_iter().map(|(n, _)| n).collect();
        for c in [
            "set_sys", "setsys", "get_sys", "getsys", "list_sys", "listsys", "options",
            "get_path", "set_path", "open_project", "add_aperture", "aligndrill",
            "aligndrillgrid", "split_geometries", "split_geometry",
        ] {
            assert!(names.contains(&c), "missing {c}");
        }
    }

    // ----- system / options -----

    #[test]
    fn set_get_list_sys() {
        let mut ctx = ScriptContext::new();
        set_sys(&mut ctx, &s(&["units", "MM"])).unwrap();
        set_sys(&mut ctx, &s(&["feedrate", "120"])).unwrap();
        assert_eq!(get_sys(&mut ctx, &s(&["units"])).unwrap(), "MM");
        assert_eq!(get_sys(&mut ctx, &s(&["feedrate"])).unwrap(), "120");
        let all = list_sys(&mut ctx, &[]).unwrap();
        assert!(all.contains("units"));
        assert!(all.contains("feedrate"));
        // filtered
        let filtered = list_sys(&mut ctx, &s(&["feed"])).unwrap();
        assert!(filtered.contains("feedrate"));
        assert!(!filtered.contains("units"));
    }

    #[test]
    fn get_sys_unset_errors() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(get_sys(&mut ctx, &s(&["nope"])), Err(ScriptError::Other(_))));
    }

    #[test]
    fn set_sys_usage_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(set_sys(&mut ctx, &s(&["onlyname"])), Err(ScriptError::Usage(_))));
    }

    #[test]
    fn options_reports_region_metadata() {
        let mut ctx = ScriptContext::new();
        ctx.put("r", square_region(0.0, 0.0, 10.0));
        let out = options(&mut ctx, &s(&["r"])).unwrap();
        assert!(out.contains("kind: region"));
        assert!(out.contains("area: 100.0000"));
        assert!(out.contains("polygons: 1"));
        assert!(out.contains("bounds:"));
    }

    #[test]
    fn options_missing_object_errors() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(options(&mut ctx, &s(&["nope"])), Err(ScriptError::NotFound(_))));
    }

    #[test]
    fn get_set_path() {
        let mut ctx = ScriptContext::new();
        assert_eq!(get_path(&mut ctx, &[]).unwrap(), "");
        set_path(&mut ctx, &s(&["/tmp/work"])).unwrap();
        assert_eq!(get_path(&mut ctx, &[]).unwrap(), "/tmp/work");
    }

    #[test]
    fn set_path_usage_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(set_path(&mut ctx, &[]), Err(ScriptError::Usage(_))));
    }

    // ----- open_project round-trip with save_project -----

    #[test]
    fn open_project_round_trips_save_project() {
        use crate::Registry;
        // Build a context with a region, an excellon, and a cnc object.
        let mut ctx = ScriptContext::new();
        ctx.put("board", square_region(5.0, 5.0, 10.0));
        let tool = Tool { diameter: 0.8, drills: vec![(1.0, 2.0), (3.0, 4.0)], slots: vec![] };
        let mut tools = BTreeMap::new();
        tools.insert(1, tool);
        ctx.put("drl", Obj::Excellon(Excellon { units: fc_excellon::Units::Mm, tools }));
        ctx.put("job", Obj::Cnc { paths: vec![], units: Units::Mm, gcode: "G21\nM30\n".into() });

        let area_before = {
            let (mp, _) = ctx.region("board").unwrap();
            fc_geo::area(&mp)
        };

        let p = temp_path("proj", "json");
        let ps = p.to_string_lossy().to_string();

        // Save via the registered save_project command.
        let r = Registry::new();
        r.run_line(&mut ctx, &format!("save_project {ps}")).unwrap();

        // Restore into a fresh context.
        let mut ctx2 = ScriptContext::new();
        let msg = open_project(&mut ctx2, &s(&[&ps])).unwrap();
        assert!(msg.contains("3 object"), "msg: {msg}");

        // Names restored.
        assert_eq!(ctx2.get("board").unwrap().kind(), "region");
        assert_eq!(ctx2.get("drl").unwrap().kind(), "excellon");
        assert_eq!(ctx2.get("job").unwrap().kind(), "cnc");

        // Region geometry restored (area preserved).
        let (mp, _) = ctx2.region("board").unwrap();
        assert!((fc_geo::area(&mp) - area_before).abs() < 1e-6, "area {}", fc_geo::area(&mp));

        // Excellon drills restored.
        if let Obj::Excellon(e) = ctx2.get("drl").unwrap() {
            assert_eq!(e.drill_count(), 2);
            assert!((e.tools[&1].diameter - 0.8).abs() < 1e-9);
        } else {
            panic!("expected excellon");
        }

        // Cnc gcode restored.
        if let Obj::Cnc { gcode, .. } = ctx2.get("job").unwrap() {
            assert!(gcode.contains("M30"));
        } else {
            panic!("expected cnc");
        }
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn open_project_restores_polygon_with_hole() {
        // exterior + interior survive the round-trip.
        let outer = fc_geo::centered_rect(0.0, 0.0, 10.0, 10.0);
        let inner = fc_geo::centered_rect(0.0, 0.0, 4.0, 4.0);
        let holed = fc_geo::difference(
            &MultiPolygon::new(vec![outer]),
            &MultiPolygon::new(vec![inner]),
        );
        let mut ctx = ScriptContext::new();
        ctx.put("h", Obj::Region(holed, Units::Mm));

        let p = temp_path("hole", "json");
        let ps = p.to_string_lossy().to_string();
        let r = crate::Registry::new();
        r.run_line(&mut ctx, &format!("save_project {ps}")).unwrap();

        let mut ctx2 = ScriptContext::new();
        open_project(&mut ctx2, &s(&[&ps])).unwrap();
        let (mp, _) = ctx2.region("h").unwrap();
        // 100 - 16 = 84 with the hole preserved.
        assert!((fc_geo::area(&mp) - 84.0).abs() < 1e-6, "area {}", fc_geo::area(&mp));
        assert!(!mp.0[0].interiors().is_empty(), "hole preserved");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn open_project_missing_file_errors() {
        let mut ctx = ScriptContext::new();
        let p = temp_path("missing", "json");
        let ps = p.to_string_lossy().to_string();
        assert!(matches!(open_project(&mut ctx, &s(&[&ps])), Err(ScriptError::Io(_))));
    }

    // ----- add_aperture -----

    #[test]
    fn add_aperture_records_on_region() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", square_region(0.0, 0.0, 10.0));
        let msg = add_aperture(&mut ctx, &s(&["g", "10", "C", "0.5"])).unwrap();
        assert!(msg.contains("aperture 10"));
        // Stored and queryable.
        assert_eq!(get_sys(&mut ctx, &s(&["aperture.g.10"])).unwrap(), "C 0.5");
    }

    #[test]
    fn add_aperture_rejects_excellon() {
        let mut ctx = ScriptContext::new();
        ctx.put("e", Obj::Excellon(Excellon { units: fc_excellon::Units::Mm, tools: BTreeMap::new() }));
        assert!(matches!(
            add_aperture(&mut ctx, &s(&["e", "10", "C", "0.5"])),
            Err(ScriptError::Other(_))
        ));
    }

    #[test]
    fn add_aperture_bad_size_errors() {
        let mut ctx = ScriptContext::new();
        ctx.put("g", square_region(0.0, 0.0, 10.0));
        assert!(matches!(
            add_aperture(&mut ctx, &s(&["g", "10", "C", "0"])),
            Err(ScriptError::Other(_))
        ));
    }

    // ----- aligndrill / aligndrillgrid -----

    #[test]
    fn aligndrill_creates_and_appends() {
        let mut ctx = ScriptContext::new();
        aligndrill(&mut ctx, &s(&["a", "0", "0", "3.0"])).unwrap();
        aligndrill(&mut ctx, &s(&["a", "50", "0", "3.0"])).unwrap();
        if let Obj::Excellon(e) = ctx.get("a").unwrap() {
            assert_eq!(e.drill_count(), 2);
            assert_eq!(e.tools.len(), 1, "same diameter merges to one tool");
        } else {
            panic!("expected excellon");
        }
    }

    #[test]
    fn aligndrill_bad_dia_errors() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(aligndrill(&mut ctx, &s(&["a", "0", "0", "0"])), Err(ScriptError::Other(_))));
    }

    #[test]
    fn aligndrillgrid_makes_grid() {
        let mut ctx = ScriptContext::new();
        // 3 cols x 2 rows = 6 drills
        let msg = aligndrillgrid(&mut ctx, &s(&["a", "0", "0", "10", "10", "3", "2", "1.0"])).unwrap();
        assert!(msg.contains("6 alignment"));
        if let Obj::Excellon(e) = ctx.get("a").unwrap() {
            assert_eq!(e.drill_count(), 6);
            // Check a corner drill exists at (20, 10).
            let all: Vec<(f64, f64)> = e.tools.values().flat_map(|t| t.drills.iter().copied()).collect();
            assert!(all.iter().any(|&(x, y)| (x - 20.0).abs() < 1e-9 && (y - 10.0).abs() < 1e-9));
        } else {
            panic!("expected excellon");
        }
    }

    #[test]
    fn aligndrillgrid_bad_dims_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(
            aligndrillgrid(&mut ctx, &s(&["a", "0", "0", "10", "10", "0", "2", "1.0"])),
            Err(ScriptError::Other(_))
        ));
    }

    #[test]
    fn aligndrillgrid_usage_error() {
        let mut ctx = ScriptContext::new();
        assert!(matches!(
            aligndrillgrid(&mut ctx, &s(&["a", "0", "0"])),
            Err(ScriptError::Usage(_))
        ));
    }

    // ----- split_geometries -----

    #[test]
    fn split_geometries_n_polys_to_n_objects() {
        let mut ctx = ScriptContext::new();
        // Three disjoint squares in one region.
        let mut polys = vec![];
        for i in 0..3 {
            polys.push(fc_geo::centered_rect(i as f64 * 20.0, 0.0, 5.0, 5.0));
        }
        ctx.put("multi", Obj::Region(MultiPolygon::new(polys), Units::Mm));

        let msg = split_geometries(&mut ctx, &s(&["multi", "part"])).unwrap();
        assert!(msg.contains("into 3 object"), "msg: {msg}");
        for n in ["part_1", "part_2", "part_3"] {
            assert_eq!(ctx.get(n).unwrap().kind(), "region");
            let (mp, _) = ctx.region(n).unwrap();
            assert_eq!(mp.0.len(), 1, "each split holds one polygon");
            assert!((fc_geo::area(&mp) - 25.0).abs() < 1e-6);
        }
    }

    #[test]
    fn split_geometries_empty_errors() {
        let mut ctx = ScriptContext::new();
        ctx.put("e", Obj::Region(MultiPolygon::new(vec![]), Units::Mm));
        assert!(matches!(
            split_geometries(&mut ctx, &s(&["e", "p"])),
            Err(ScriptError::Other(_))
        ));
    }

    #[test]
    fn split_geometries_rejects_non_region() {
        let mut ctx = ScriptContext::new();
        ctx.put("c", Obj::Cnc { paths: vec![], units: Units::Mm, gcode: String::new() });
        assert!(split_geometries(&mut ctx, &s(&["c", "p"])).is_err());
    }
}
