//! `fc-gerber` — a RS-274X (Gerber) parser that produces `geo` geometry.
//!
//! Port of the geometry-producing core of FlatCAM's
//! `appParsers/ParseGerber.py`. It implements the command subset that real
//! PCB CAM files use: format spec (FS), units (MO), aperture definitions
//! (AD: C/R/O/P + macros AM), aperture macros (primitives 1/4/5/20-22),
//! draw/move/flash (D01/D02/D03), linear and circular interpolation
//! (G01/G02/G03, single- and multi-quadrant), region fill (G36/G37), and
//! level polarity (LP D/C).
//!
//! The result is a single solid [`geo::MultiPolygon`] (the union of all dark
//! geometry minus all clear geometry) plus the "follow" centre-line geometry
//! used for trace-following tool paths.

use fc_geo::{buffer_path, circle, difference, obround, regular_polygon, union, union_all};
use geo::{Coord, LineString, MultiPolygon, Polygon};
use std::collections::HashMap;
use std::f64::consts::PI;

mod macros;
use macros::MacroDef;

#[derive(thiserror::Error, Debug)]
pub enum GerberError {
    #[error("no format specification (FS) found before coordinate data")]
    NoFormat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Inch,
    Mm,
}

#[derive(Clone, Copy, Debug)]
enum Zeros {
    Leading,  // L: leading zeros omitted
    Trailing, // T: trailing zeros omitted
    None,     // D: no suppression
}

/// A parsed aperture (`AD` definition).
#[derive(Clone, Debug)]
pub struct Aperture {
    pub kind: String, // "C", "R", "O", "P", "AM", "REG"
    pub size: f64,
    pub width: f64,
    pub height: f64,
    pub n_vertices: usize,
    pub rotation: f64,
    pub macro_name: Option<String>,
    pub macro_mods: Vec<f64>,
}

impl Default for Aperture {
    fn default() -> Self {
        Aperture {
            kind: "C".into(),
            size: 0.0,
            width: 0.0,
            height: 0.0,
            n_vertices: 0,
            rotation: 0.0,
            macro_name: None,
            macro_mods: vec![],
        }
    }
}

/// Result of parsing a Gerber file.
#[derive(Debug)]
pub struct Gerber {
    pub units: Units,
    pub apertures: HashMap<i32, Aperture>,
    pub solid_geometry: MultiPolygon<f64>,
    pub follow_geometry: Vec<LineString<f64>>,
}

impl Gerber {
    /// Bounding box `(min_x, min_y, max_x, max_y)` of the solid geometry.
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        use geo::BoundingRect;
        self.solid_geometry.bounding_rect().map(|r| {
            (r.min().x, r.min().y, r.max().x, r.max().y)
        })
    }
}

const STEPS: usize = 64;

/// Parse Gerber source text into a [`Gerber`].
pub fn parse(content: &str) -> Result<Gerber, GerberError> {
    let mut p = Parser::new();
    p.run(content)?;
    Ok(Gerber {
        units: p.units,
        apertures: p.apertures,
        solid_geometry: p.solid,
        follow_geometry: p.follow,
    })
}

struct Parser {
    units: Units,
    int_digits: i32,
    frac_digits: i32,
    zeros: Zeros,
    have_format: bool,

    apertures: HashMap<i32, Aperture>,
    macros: HashMap<String, MacroDef>,

    // state machine
    cur_aperture: i32,
    last_path_aperture: i32,
    interp: u8,        // 1 linear, 2 cw, 3 ccw
    quadrant_multi: bool,
    x: f64,
    y: f64,
    polarity_dark: bool,
    making_region: bool,
    path: Vec<Coord<f64>>,

    // accumulators
    dark: Vec<Polygon<f64>>,
    clear: Vec<Polygon<f64>>,
    solid: MultiPolygon<f64>,
    follow: Vec<LineString<f64>>,
}

impl Parser {
    fn new() -> Self {
        Parser {
            units: Units::Inch,
            int_digits: 3,
            frac_digits: 4,
            zeros: Zeros::Leading,
            have_format: false,
            apertures: HashMap::new(),
            macros: HashMap::new(),
            cur_aperture: -1,
            last_path_aperture: -1,
            interp: 1,
            quadrant_multi: true,
            x: 0.0,
            y: 0.0,
            polarity_dark: true,
            making_region: false,
            path: vec![],
            dark: vec![],
            clear: vec![],
            solid: MultiPolygon::new(vec![]),
            follow: vec![],
        }
    }

    fn run(&mut self, content: &str) -> Result<(), GerberError> {
        for cmd in tokenize(content) {
            match cmd {
                Cmd::Extended(parts) => self.handle_extended(parts),
                Cmd::Word(w) => self.handle_word(&w)?,
            }
        }
        // flush any trailing path
        self.flush_path();
        self.merge_polarity();
        Ok(())
    }

    // ----- extended commands (%...%) -----
    fn handle_extended(&mut self, parts: Vec<String>) {
        let mut i = 0;
        while i < parts.len() {
            let s = parts[i].trim().to_string();
            if s.starts_with("FS") {
                self.parse_fs(&s);
            } else if s.starts_with("MO") {
                self.parse_mo(&s);
            } else if s.starts_with("AD") {
                self.parse_ad(&s);
            } else if s.starts_with("AM") {
                // macro: name = rest of this token, body = following tokens
                let name = s[2..].trim().to_string();
                let mut body: Vec<String> = vec![];
                i += 1;
                while i < parts.len() {
                    body.push(parts[i].clone());
                    i += 1;
                }
                self.macros.insert(name.clone(), MacroDef::new(body));
                continue;
            } else if s.starts_with("LP") {
                self.flush_path();
                self.merge_polarity();
                self.polarity_dark = !s.contains('C');
            } else if s.starts_with("IP") || s.starts_with("AS") || s.starts_with("IR")
                || s.starts_with("MI") || s.starts_with("OF") || s.starts_with("SF")
                || s.starts_with("IN") || s.starts_with("TF") || s.starts_with("TA")
                || s.starts_with("TO") || s.starts_with("TD") || s.starts_with("LM")
                || s.starts_with("LR") || s.starts_with("LS")
            {
                // ignored attribute / deprecated extended commands
            }
            i += 1;
        }
    }

    fn parse_fs(&mut self, s: &str) {
        // e.g. FSLAX34Y34
        self.zeros = if s.contains("FSL") {
            Zeros::Leading
        } else if s.contains("FST") {
            Zeros::Trailing
        } else {
            Zeros::None
        };
        if let Some(xpos) = s.find('X') {
            let bytes = s.as_bytes();
            if xpos + 2 < bytes.len() {
                let a = (bytes[xpos + 1] as char).to_digit(10);
                let b = (bytes[xpos + 2] as char).to_digit(10);
                if let (Some(a), Some(b)) = (a, b) {
                    self.int_digits = a as i32;
                    self.frac_digits = b as i32;
                }
            }
        }
        self.have_format = true;
    }

    fn parse_mo(&mut self, s: &str) {
        if s.contains("MM") {
            self.units = Units::Mm;
        } else if s.contains("IN") {
            self.units = Units::Inch;
        }
    }

    fn parse_ad(&mut self, s: &str) {
        // ADD<code><template>[,<mods>]
        let body = &s[3..]; // strip "ADD"
        // code is leading digits
        let code_len = body.chars().take_while(|c| c.is_ascii_digit()).count();
        if code_len == 0 {
            return;
        }
        let code: i32 = body[..code_len].parse().unwrap_or(-1);
        let rest = &body[code_len..];
        let (template, mods_str) = match rest.find(',') {
            Some(p) => (&rest[..p], &rest[p + 1..]),
            None => (rest, ""),
        };
        let mods: Vec<f64> = mods_str
            .split('X')
            .filter_map(|t| t.trim().parse::<f64>().ok())
            .collect();
        let mut ap = Aperture::default();
        match template {
            "C" => {
                ap.kind = "C".into();
                ap.size = mods.first().copied().unwrap_or(0.0);
            }
            "R" => {
                ap.kind = "R".into();
                ap.width = mods.first().copied().unwrap_or(0.0);
                ap.height = mods.get(1).copied().unwrap_or(ap.width);
                ap.size = (ap.width * ap.width + ap.height * ap.height).sqrt();
            }
            "O" => {
                ap.kind = "O".into();
                ap.width = mods.first().copied().unwrap_or(0.0);
                ap.height = mods.get(1).copied().unwrap_or(ap.width);
                ap.size = (ap.width * ap.width + ap.height * ap.height).sqrt();
            }
            "P" => {
                ap.kind = "P".into();
                ap.size = mods.first().copied().unwrap_or(0.0);
                ap.n_vertices = mods.get(1).copied().unwrap_or(3.0) as usize;
                ap.rotation = mods.get(2).copied().unwrap_or(0.0);
            }
            name => {
                ap.kind = "AM".into();
                ap.macro_name = Some(name.to_string());
                ap.macro_mods = mods;
            }
        }
        self.apertures.insert(code, ap);
    }

    // ----- function-code words -----
    fn handle_word(&mut self, w: &str) -> Result<(), GerberError> {
        let w = w.trim();
        if w.is_empty() {
            return Ok(());
        }
        if w.starts_with("G04") || w.starts_with("G4") {
            return Ok(()); // comment
        }
        if w == "G36" {
            self.flush_path();
            self.making_region = true;
            return Ok(());
        }
        if w == "G37" {
            self.finish_region();
            self.making_region = false;
            return Ok(());
        }
        if w == "G01" || w == "G1" {
            self.interp = 1;
            return Ok(());
        }
        if w == "G02" || w == "G2" {
            self.interp = 2;
            return Ok(());
        }
        if w == "G03" || w == "G3" {
            self.interp = 3;
            return Ok(());
        }
        if w == "G74" {
            self.quadrant_multi = false;
            return Ok(());
        }
        if w == "G75" {
            self.quadrant_multi = true;
            return Ok(());
        }
        if w.starts_with("G70") {
            self.units = Units::Inch;
            return Ok(());
        }
        if w.starts_with("G71") {
            self.units = Units::Mm;
            return Ok(());
        }
        if w.starts_with("M02") || w.starts_with("M0") {
            return Ok(());
        }
        // aperture select: optional G54 then D<code> with code >= 10
        let core = w.strip_prefix("G54").unwrap_or(w);
        if let Some(rest) = core.strip_prefix('D') {
            if let Ok(code) = rest.parse::<i32>() {
                if code >= 10 {
                    self.flush_path();
                    self.cur_aperture = code;
                    return Ok(());
                }
            }
        }
        // otherwise: a coordinate/operation block
        self.handle_coord_block(w)
    }

    fn handle_coord_block(&mut self, w: &str) -> Result<(), GerberError> {
        // possible leading interpolation code inside the block, e.g. G01X..D01
        let mut s = w;
        if let Some(r) = s.strip_prefix("G01") { self.interp = 1; s = r; }
        else if let Some(r) = s.strip_prefix("G02") { self.interp = 2; s = r; }
        else if let Some(r) = s.strip_prefix("G03") { self.interp = 3; s = r; }
        else if let Some(r) = s.strip_prefix("G1") { self.interp = 1; s = r; }
        else if let Some(r) = s.strip_prefix("G2") { self.interp = 2; s = r; }
        else if let Some(r) = s.strip_prefix("G3") { self.interp = 3; s = r; }

        let (xs, ys, is, js, d) = split_coords(s);
        if d.is_none() && xs.is_none() && ys.is_none() {
            return Ok(());
        }
        if !self.have_format {
            return Err(GerberError::NoFormat);
        }
        let nx = xs.map(|v| self.decode(v)).unwrap_or(self.x);
        let ny = ys.map(|v| self.decode(v)).unwrap_or(self.y);
        let op = d.unwrap_or(if self.interp >= 2 { 1 } else { 0 });

        match op {
            1 => {
                // draw
                if self.path.is_empty() {
                    self.path.push(Coord { x: self.x, y: self.y });
                    self.last_path_aperture = self.cur_aperture;
                }
                if self.interp == 1 {
                    self.path.push(Coord { x: nx, y: ny });
                } else {
                    let i = is.map(|v| self.decode_unsigned(v)).unwrap_or(0.0);
                    let j = js.map(|v| self.decode_unsigned(v)).unwrap_or(0.0);
                    let arc = self.make_arc(self.x, self.y, nx, ny, i, j);
                    self.path.extend(arc);
                }
                self.x = nx;
                self.y = ny;
            }
            2 => {
                // move (pen up): flush current path, start anew
                self.flush_path();
                self.x = nx;
                self.y = ny;
            }
            3 => {
                // flash
                self.flush_path();
                self.x = nx;
                self.y = ny;
                self.flash(nx, ny);
            }
            _ => {}
        }
        Ok(())
    }

    fn make_arc(&self, x0: f64, y0: f64, x1: f64, y1: f64, i: f64, j: f64) -> Vec<Coord<f64>> {
        // Multi-quadrant: center is current + (i,j).
        let dir_ccw = self.interp == 3;
        if self.quadrant_multi {
            let cx = x0 + i;
            let cy = y0 + j;
            let r = (i * i + j * j).sqrt();
            let start = (y0 - cy).atan2(x0 - cx);
            let mut stop = (y1 - cy).atan2(x1 - cx);
            if (x0 - x1).abs() < 1e-9 && (y0 - y1).abs() < 1e-9 {
                stop = start + if dir_ccw { 2.0 * PI } else { -2.0 * PI };
            }
            arc_points(cx, cy, r, start, stop, dir_ccw)
        } else {
            // single-quadrant: try 4 sign combos, pick valid (<=90deg, equal radii)
            for (si, sj) in [(i, j), (-i, j), (i, -j), (-i, -j)] {
                let cx = x0 + si;
                let cy = y0 + sj;
                let r0 = ((x0 - cx).powi(2) + (y0 - cy).powi(2)).sqrt();
                let r1 = ((x1 - cx).powi(2) + (y1 - cy).powi(2)).sqrt();
                if r0 < 1e-9 || (r1 - r0).abs() / r0 > 0.05 {
                    continue;
                }
                let start = (y0 - cy).atan2(x0 - cx);
                let stop = (y1 - cy).atan2(x1 - cx);
                let mut sweep = stop - start;
                while sweep <= -PI { sweep += 2.0 * PI; }
                while sweep > PI { sweep -= 2.0 * PI; }
                if sweep.abs() > PI / 2.0 + 1e-3 {
                    continue;
                }
                return arc_points(cx, cy, r0, start, start + sweep, sweep > 0.0);
            }
            vec![Coord { x: x1, y: y1 }]
        }
    }

    fn flash(&mut self, x: f64, y: f64) {
        let geo = self.aperture_geometry(self.cur_aperture, x, y);
        self.push_geo(geo);
    }

    fn aperture_geometry(&self, code: i32, x: f64, y: f64) -> MultiPolygon<f64> {
        let Some(ap) = self.apertures.get(&code) else {
            return MultiPolygon::new(vec![]);
        };
        match ap.kind.as_str() {
            "C" => MultiPolygon::new(vec![circle(x, y, ap.size / 2.0, STEPS)]),
            "R" => MultiPolygon::new(vec![fc_geo::centered_rect(x, y, ap.width, ap.height)]),
            "O" => obround(x, y, ap.width, ap.height, STEPS),
            "P" => MultiPolygon::new(vec![regular_polygon(
                x, y, ap.size, ap.n_vertices.max(3), ap.rotation,
            )]),
            "AM" => {
                if let Some(name) = &ap.macro_name {
                    if let Some(m) = self.macros.get(name) {
                        let mut g = m.evaluate(&ap.macro_mods, STEPS);
                        // translate to flash location
                        use geo::Translate;
                        g = g.translate(x, y);
                        return g;
                    }
                }
                MultiPolygon::new(vec![])
            }
            _ => MultiPolygon::new(vec![]),
        }
    }

    fn flush_path(&mut self) {
        if self.path.len() < 2 {
            self.path.clear();
            return;
        }
        let pts = std::mem::take(&mut self.path);
        if self.making_region {
            // handled by finish_region; restore for region accumulation
            self.path = pts;
            return;
        }
        let ap_code = self.last_path_aperture;
        let width = self
            .apertures
            .get(&ap_code)
            .map(|a| if a.kind == "R" { a.height.max(a.width) } else { a.size })
            .unwrap_or(0.0);
        let geo = buffer_path(&pts, (width / 2.0).max(1e-9), STEPS);
        self.follow.push(LineString::new(pts));
        self.push_geo(geo);
    }

    fn finish_region(&mut self) {
        if self.path.len() >= 3 {
            let mut ring = std::mem::take(&mut self.path);
            // close
            if ring.first() != ring.last() {
                ring.push(ring[0]);
            }
            let poly = Polygon::new(LineString::new(ring), vec![]);
            let mp = MultiPolygon::new(vec![poly]);
            self.push_geo(mp);
        }
        self.path.clear();
    }

    fn push_geo(&mut self, mp: MultiPolygon<f64>) {
        if self.polarity_dark {
            self.dark.extend(mp.0);
        } else {
            self.clear.extend(mp.0);
        }
    }

    fn merge_polarity(&mut self) {
        if self.dark.is_empty() && self.clear.is_empty() {
            return;
        }
        let dark = union_all(std::mem::take(&mut self.dark));
        let merged = union(&self.solid, &dark);
        let merged = if self.clear.is_empty() {
            merged
        } else {
            let clear = union_all(std::mem::take(&mut self.clear));
            difference(&merged, &clear)
        };
        self.solid = merged;
    }

    fn decode(&self, s: &str) -> f64 {
        let neg = s.starts_with('-');
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return 0.0;
        }
        let v: i64 = digits.parse().unwrap_or(0);
        let val = match self.zeros {
            Zeros::Trailing => {
                let total = self.int_digits + self.frac_digits;
                let len = digits.len() as i32;
                (v as f64) * 10f64.powi(total - len) / 10f64.powi(self.frac_digits)
            }
            _ => (v as f64) / 10f64.powi(self.frac_digits),
        };
        if neg { -val } else { val }
    }

    fn decode_unsigned(&self, s: &str) -> f64 {
        self.decode(s)
    }
}

fn arc_points(cx: f64, cy: f64, r: f64, start: f64, stop: f64, ccw: bool) -> Vec<Coord<f64>> {
    let start = start;
    let mut stop = stop;
    if ccw && stop <= start {
        stop += 2.0 * PI;
    }
    if !ccw && stop >= start {
        stop -= 2.0 * PI;
    }
    let sweep = (stop - start).abs();
    let steps = ((sweep / (2.0 * PI) * STEPS as f64).ceil() as usize).max(2);
    let delta = if ccw { sweep / steps as f64 } else { -sweep / steps as f64 };
    let mut out = Vec::with_capacity(steps + 1);
    for k in 0..=steps {
        let t = start + delta * k as f64;
        out.push(Coord { x: cx + r * t.cos(), y: cy + r * t.sin() });
    }
    out
}

/// Extract X, Y, I, J coordinate strings and the D-operation from a block.
fn split_coords(s: &str) -> (Option<&str>, Option<&str>, Option<&str>, Option<&str>, Option<u8>) {
    let mut x = None;
    let mut y = None;
    let mut i = None;
    let mut j = None;
    let mut d = None;
    let bytes = s.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        let c = bytes[idx] as char;
        if matches!(c, 'X' | 'Y' | 'I' | 'J' | 'D') {
            let start = idx + 1;
            let mut end = start;
            while end < bytes.len() {
                let cc = bytes[end] as char;
                if cc.is_ascii_digit() || cc == '-' || cc == '+' {
                    end += 1;
                } else {
                    break;
                }
            }
            let val = &s[start..end];
            match c {
                'X' => x = Some(val),
                'Y' => y = Some(val),
                'I' => i = Some(val),
                'J' => j = Some(val),
                'D' => d = val.parse::<u8>().ok(),
                _ => {}
            }
            idx = end;
        } else {
            idx += 1;
        }
    }
    (x, y, i, j, d)
}

// ----- tokenizer -----
enum Cmd {
    Extended(Vec<String>), // inner tokens of a %...% block, split on '*'
    Word(String),          // a function-code block (without trailing '*')
}

fn tokenize(content: &str) -> Vec<Cmd> {
    let mut cmds = Vec::new();
    let chars: Vec<char> = content.chars().collect();
    let n = chars.len();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '%' {
            i += 1;
            let mut inner = String::new();
            while i < n && chars[i] != '%' {
                inner.push(chars[i]);
                i += 1;
            }
            i += 1; // skip closing %
            let parts: Vec<String> = inner
                .split('*')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if !parts.is_empty() {
                cmds.push(Cmd::Extended(parts));
            }
        } else {
            let mut word = String::new();
            while i < n && chars[i] != '*' && chars[i] != '%' {
                if !chars[i].is_whitespace() {
                    word.push(chars[i]);
                }
                i += 1;
            }
            if i < n && chars[i] == '*' {
                i += 1;
            }
            if !word.is_empty() {
                cmds.push(Cmd::Word(word));
            }
        }
    }
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::area;

    const SIMPLE: &str = "\
%FSLAX24Y24*%
%MOMM*%
%ADD10C,1.0*%
D10*
X0Y0D03*
X50000Y0D03*
M02*
";

    #[test]
    fn parses_two_flashes() {
        let g = parse(SIMPLE).unwrap();
        assert_eq!(g.units, Units::Mm);
        assert_eq!(g.apertures.len(), 1);
        // two circles dia 1.0 (r=0.5) => area ~ 2 * pi * 0.25 = 1.5708
        let a = area(&g.solid_geometry);
        assert!((a - 1.5708).abs() < 0.05, "area {a}");
    }

    #[test]
    fn parses_trace() {
        let src = "\
%FSLAX24Y24*%
%MOMM*%
%ADD10C,1.0*%
D10*
X0Y0D02*
X100000Y0D01*
M02*
";
        let g = parse(src).unwrap();
        let a = area(&g.solid_geometry);
        // 10mm long, 1mm wide trace ~ 10 + pi*0.25
        assert!(a > 10.0 && a < 11.0, "trace area {a}");
        assert_eq!(g.follow_geometry.len(), 1);
    }

    #[test]
    fn rectangle_aperture_flash() {
        let src = "\
%FSLAX24Y24*%
%MOMM*%
%ADD10R,2.0X3.0*%
D10*
X0Y0D03*
M02*
";
        let g = parse(src).unwrap();
        let a = area(&g.solid_geometry);
        assert!((a - 6.0).abs() < 1e-3, "rect area {a}");
    }

    #[test]
    fn region_fill() {
        let src = "\
%FSLAX24Y24*%
%MOMM*%
G36*
X0Y0D02*
X100000Y0D01*
X100000Y100000D01*
X0Y100000D01*
X0Y0D01*
G37*
M02*
";
        let g = parse(src).unwrap();
        let a = area(&g.solid_geometry);
        assert!((a - 100.0).abs() < 0.5, "region area {a}");
    }
}
