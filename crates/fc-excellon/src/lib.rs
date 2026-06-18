//! `fc-excellon` — an Excellon (drill/route) parser producing `geo` geometry.
//!
//! Port of the parsing core of FlatCAM's `appParsers/ParseExcellon.py`. It
//! handles the header (`M48` … `%`/`M95`), units and zero-suppression
//! (`INCH`/`METRIC`, `LZ`/`TZ`, inline format), tool definitions (`Tnn C…`),
//! tool selection, plain drill hits (`X… Y…`), `G85` slots, and `G00/G01`
//! routed slots. Coordinate decoding follows the Excellon
//! leading/trailing-zero rules exactly.

use fc_geo::{buffer_path, circle, MultiPolygon};
use geo::Coord;
use std::collections::BTreeMap;

pub mod writer;
pub use writer::write_excellon;

#[derive(thiserror::Error, Debug)]
pub enum ExcellonError {
    #[error("empty drill file")]
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Inch,
    Mm,
}

#[derive(Clone, Copy, Debug)]
enum Zeros {
    Leading,  // LZ: leading zeros kept, trailing may be omitted
    Trailing, // TZ: trailing zeros kept, leading may be omitted
}

/// A drilling tool and its hits.
#[derive(Clone, Debug, Default)]
pub struct Tool {
    pub diameter: f64,
    pub drills: Vec<(f64, f64)>,
    pub slots: Vec<((f64, f64), (f64, f64))>,
}

/// Parsed Excellon file.
#[derive(Debug)]
pub struct Excellon {
    pub units: Units,
    pub tools: BTreeMap<i32, Tool>,
}

impl Excellon {
    /// Total number of drill hits across all tools.
    pub fn drill_count(&self) -> usize {
        self.tools.values().map(|t| t.drills.len()).sum()
    }

    /// Build solid geometry (circles for drills, rounded slots for routes) for
    /// a given tool, using `steps` segments per circle.
    pub fn tool_geometry(&self, tool: i32, steps: usize) -> MultiPolygon<f64> {
        let Some(t) = self.tools.get(&tool) else {
            return MultiPolygon::new(vec![]);
        };
        // A non-positive/non-finite diameter (malformed tool def) yields no geometry.
        if !(t.diameter > 0.0) {
            return MultiPolygon::new(vec![]);
        }
        let r = t.diameter / 2.0;
        let mut polys = Vec::new();
        for &(x, y) in &t.drills {
            polys.push(circle(x, y, r, steps));
        }
        let mut mp = fc_geo::union_all(polys);
        for &(a, b) in &t.slots {
            let line = vec![Coord { x: a.0, y: a.1 }, Coord { x: b.0, y: b.1 }];
            let slot = buffer_path(&line, r, steps);
            mp = fc_geo::union(&mp, &slot);
        }
        mp
    }
}

/// Parse Excellon source text.
pub fn parse(content: &str) -> Result<Excellon, ExcellonError> {
    if content.trim().is_empty() {
        return Err(ExcellonError::Empty);
    }
    let mut p = Parser::new();
    for raw in content.lines() {
        p.line(raw.trim());
    }
    p.finish_units_inference();
    Ok(Excellon {
        units: p.units,
        tools: p.tools,
    })
}

struct Parser {
    units: Units,
    zeros: Zeros,
    in_header: bool,
    // format: integer digits / fractional digits per unit
    int_in: i32,
    frac_in: i32,
    int_mm: i32,
    frac_mm: i32,
    units_seen: bool,

    tools: BTreeMap<i32, Tool>,
    cur_tool: i32,
    x: f64,
    y: f64,
    // routed slot state
    routing: bool,
    slot_start: (f64, f64),
}

impl Parser {
    fn new() -> Self {
        Parser {
            units: Units::Inch,
            zeros: Zeros::Leading,
            in_header: false,
            int_in: 2,
            frac_in: 4,
            int_mm: 3,
            frac_mm: 3,
            units_seen: false,
            tools: BTreeMap::new(),
            cur_tool: 0,
            x: 0.0,
            y: 0.0,
            routing: false,
            slot_start: (0.0, 0.0),
        }
    }

    fn line(&mut self, l: &str) {
        if l.is_empty() {
            return;
        }
        if l.starts_with(';') {
            return; // comment
        }
        if l == "M48" {
            self.in_header = true;
            return;
        }
        if l == "%" || l == "M95" {
            self.in_header = false;
            return;
        }
        if l.starts_with("INCH") || l.starts_with("METRIC") {
            self.parse_units(l);
            return;
        }
        if l == "M71" {
            self.units = Units::Mm;
            self.units_seen = true;
            return;
        }
        if l == "M72" {
            self.units = Units::Inch;
            self.units_seen = true;
            return;
        }
        if l.starts_with("FMAT") || l == "M30" || l.starts_with("G90") || l.starts_with("G91") {
            return;
        }
        if l.starts_with("G05") {
            return;
        }
        // tool definition or selection
        if l.starts_with('T') {
            self.parse_tool_line(l);
            return;
        }
        // routing mode
        if l.starts_with("G00") {
            self.routing = true;
            let (x, y) = self.coords(l);
            self.x = x;
            self.y = y;
            self.slot_start = (x, y);
            return;
        }
        if l.starts_with("G01") {
            if self.routing {
                let (x, y) = self.coords(l);
                self.x = x;
                self.y = y;
                let start = self.slot_start;
                self.add_slot(start, (x, y));
                self.slot_start = (x, y);
            }
            return;
        }
        // a coordinate line (drill or slot via G85)
        if l.contains('X') || l.contains('Y') {
            self.parse_coord_line(l);
        }
    }

    fn parse_units(&mut self, l: &str) {
        if l.starts_with("METRIC") {
            self.units = Units::Mm;
        } else {
            self.units = Units::Inch;
        }
        self.units_seen = true;
        if l.contains("TZ") {
            self.zeros = Zeros::Trailing;
        } else if l.contains("LZ") {
            self.zeros = Zeros::Leading;
        }
        // inline format like "METRIC,000.000" or "INCH,00.0000"
        if let Some(comma) = l.find(',') {
            let rest = &l[comma + 1..];
            if let Some(fmt) = rest.split(',').find(|t| t.contains('.')) {
                if let Some(dot) = fmt.find('.') {
                    let upper = fmt[..dot].chars().filter(|c| c.is_ascii_digit()).count() as i32;
                    let lower = fmt[dot + 1..].chars().filter(|c| c.is_ascii_digit()).count() as i32;
                    if upper + lower > 0 {
                        if self.units == Units::Mm {
                            self.int_mm = upper;
                            self.frac_mm = lower;
                        } else {
                            self.int_in = upper;
                            self.frac_in = lower;
                        }
                    }
                }
            }
        }
    }

    fn parse_tool_line(&mut self, l: &str) {
        // T<num>[C<dia>][F..][S..]  — header def or body select
        let num_str: String = l[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
        let Ok(num) = num_str.parse::<i32>() else {
            return;
        };
        let dia = extract_after(l, 'C');
        if let Some(d) = dia {
            let t = self.tools.entry(num).or_default();
            t.diameter = d;
        } else {
            // selection
            self.tools.entry(num).or_default();
            self.cur_tool = num;
            self.routing = false;
        }
    }

    fn parse_coord_line(&mut self, l: &str) {
        // G85 slot: "X..Y..G85X..Y.."
        if let Some(idx) = l.find("G85") {
            let (a, b) = l.split_at(idx);
            let start = self.coords(a);
            let end = self.coords(&b[3..]);
            self.x = end.0;
            self.y = end.1;
            self.add_slot(start, end);
            return;
        }
        let (x, y) = self.coords(l);
        self.x = x;
        self.y = y;
        if self.cur_tool != 0 {
            self.tools.entry(self.cur_tool).or_default().drills.push((x, y));
        }
    }

    fn add_slot(&mut self, a: (f64, f64), b: (f64, f64)) {
        if self.cur_tool != 0 {
            self.tools.entry(self.cur_tool).or_default().slots.push((a, b));
        }
    }

    /// Decode X/Y from a block, reusing the previous coordinate when absent.
    fn coords(&self, l: &str) -> (f64, f64) {
        let x = extract_coord(l, 'X').map(|s| self.decode(&s)).unwrap_or(self.x);
        let y = extract_coord(l, 'Y').map(|s| self.decode(&s)).unwrap_or(self.y);
        (x, y)
    }

    fn decode(&self, s: &str) -> f64 {
        let neg = s.starts_with('-');
        if s.contains('.') {
            return s.parse::<f64>().unwrap_or(0.0);
        }
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() {
            return 0.0;
        }
        let v: f64 = digits.parse().unwrap_or(0.0);
        let (upper, frac) = if self.units == Units::Mm {
            (self.int_mm, self.frac_mm)
        } else {
            (self.int_in, self.frac_in)
        };
        let nr_len = digits.len() as i32;
        let val = match self.zeros {
            // Leading zeros kept => the supplied digits are the most-significant;
            // missing trailing zeros are implied. Scale by digits-vs-integer.
            Zeros::Leading => v / 10f64.powi(nr_len - upper),
            // Trailing zeros kept => digits are the least-significant; scale by frac.
            Zeros::Trailing => v / 10f64.powi(frac),
        };
        if neg { -val } else { val }
    }

    fn finish_units_inference(&mut self) {
        if self.units_seen {
            return;
        }
        // infer from diameter distribution: many small (<=0.1) => inches
        let mut small = 0;
        let mut large = 0;
        for t in self.tools.values() {
            if t.diameter <= 0.1 {
                small += 1;
            } else {
                large += 1;
            }
        }
        self.units = if small > large { Units::Inch } else { Units::Mm };
    }
}

fn extract_coord(l: &str, key: char) -> Option<String> {
    let bytes: Vec<char> = l.chars().collect();
    let pos = bytes.iter().position(|&c| c == key)?;
    let mut out = String::new();
    let mut i = pos + 1;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            out.push(c);
            i += 1;
        } else {
            break;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn extract_after(l: &str, key: char) -> Option<f64> {
    extract_coord(l, key).and_then(|s| s.parse::<f64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_metric_drills() {
        let src = "\
M48
METRIC,TZ
T1C0.8
T2C1.2
%
T1
X10.0Y10.0
X20.0Y10.0
T2
X30.0Y30.0
M30
";
        let e = parse(src).unwrap();
        assert_eq!(e.units, Units::Mm);
        assert_eq!(e.tools.len(), 2);
        assert_eq!(e.tools[&1].diameter, 0.8);
        assert_eq!(e.tools[&1].drills.len(), 2);
        assert_eq!(e.tools[&2].drills.len(), 1);
        assert_eq!(e.drill_count(), 3);
    }

    #[test]
    fn decode_leading_zeros_no_period() {
        // INCH, leading-zero (LZ): format 2:4. "012345" -> 1.2345
        let src = "\
M48
INCH,LZ
T1C0.5
%
T1
X012345Y012345
M30
";
        let e = parse(src).unwrap();
        let (x, _y) = e.tools[&1].drills[0];
        assert!((x - 1.2345).abs() < 1e-6, "x decoded {x}");
    }

    #[test]
    fn decode_trailing_zeros() {
        // METRIC, TZ, format 3:3. "123000" -> 123.000
        let src = "\
M48
METRIC,TZ,000.000
T1C1.0
%
T1
X123000Y045000
M30
";
        let e = parse(src).unwrap();
        let (x, y) = e.tools[&1].drills[0];
        assert!((x - 123.0).abs() < 1e-6, "x {x}");
        assert!((y - 45.0).abs() < 1e-6, "y {y}");
    }

    #[test]
    fn g85_slot() {
        let src = "\
M48
METRIC,LZ
T1C1.0
%
T1
X10.0Y10.0G85X20.0Y10.0
M30
";
        let e = parse(src).unwrap();
        assert_eq!(e.tools[&1].slots.len(), 1);
        let geo = e.tool_geometry(1, 32);
        let a = fc_geo::area(&geo);
        // 10mm slot, 1mm tool => 10 + pi*0.25
        assert!(a > 10.0 && a < 11.0, "slot area {a}");
    }

    #[test]
    fn drill_geometry_area() {
        let src = "\
M48
METRIC,LZ
T1C2.0
%
T1
X0.0Y0.0
M30
";
        let e = parse(src).unwrap();
        let geo = e.tool_geometry(1, 256);
        let a = fc_geo::area(&geo);
        assert!((a - std::f64::consts::PI).abs() < 1e-2, "drill area {a}");
    }
}
