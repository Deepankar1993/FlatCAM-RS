//! PcbWizard drill import.
//!
//! PcbWizard (the schematic/PCB tool) exports drilling data as a *pair* of
//! files rather than a single self-describing Excellon file:
//!
//! * a `.DRL` data file — the drill coordinates: `Tnn` tool-select lines
//!   followed by `Xnnn Ynnn` hit lines. Crucially the `.DRL` body carries **no**
//!   tool diameters and (in the classic export) **no** units / decimal point —
//!   coordinates are integers in an *implied* format.
//! * an `.INF` info file — the legend: it maps each tool code (`T01`, `T02`, …)
//!   to a real diameter, and states the units and the implied coordinate format.
//!
//! [`parse_pcbwizard`] reads the `.INF` first to build the tool table and learn
//! the units/format, then parses the `.DRL` coordinates against that table,
//! producing an [`Excellon`] identical in shape to what [`crate::parse`] yields
//! for a normal single-file drill program.
//!
//! ## Format assumptions
//!
//! The `.INF` is line-oriented; we accept (case-insensitively) the documented
//! and commonly-seen forms:
//!
//! * `Units, MM` / `Units, INCH` (also bare `METRIC` / `INCH`) — sets units.
//!   Defaults to inch (Excellon's historical default) when unstated.
//! * `Format, 3.3` (integer.fraction digit counts) — the implied decimal layout
//!   used to decode the integer `.DRL` coordinates. Defaults to `2.4` for inch
//!   and `3.3` for mm when unstated (the usual PcbWizard defaults).
//! * Tool legend lines: `T01 0.8mm`, `T01,0.8mm`, `T01 = 0.032in`, or
//!   `T01 0.032` (bare number → interpreted in the file's units). A trailing
//!   `mm`/`in`/`mil`/`inch` unit suffix on the diameter is honoured and converted
//!   to the file units. Lines that don't start with a `T<digits>` token and
//!   aren't a Units/Format directive are ignored (comments, headers).
//!
//! The `.DRL` is parsed permissively: `Tnn` selects a tool (it must exist in the
//! `.INF`, else [`ExcellonError::UnknownTool`]); a line containing `X`/`Y`
//! records a hit for the current tool. Coordinates with an explicit `.` are
//! taken literally; integer coordinates are decoded with the implied format.
//! Empty inputs yield [`ExcellonError::Empty`].

use crate::{Excellon, ExcellonError, Tool, Units};
use std::collections::BTreeMap;

/// Units/format learned from the `.INF` file.
struct Format {
    units: Units,
    /// fractional digit count for the implied integer coordinate format.
    frac: i32,
    format_seen: bool,
}

impl Format {
    fn default_for_unit(units: Units) -> i32 {
        // PcbWizard defaults: inch 2.4, mm 3.3.
        match units {
            Units::Inch => 4,
            Units::Mm => 3,
        }
    }
}

/// Parse a PcbWizard `.INF` + `.DRL` pair into an [`Excellon`].
///
/// See the module docs for the accepted `.INF`/`.DRL` forms and the format
/// assumptions. Returns:
/// * [`ExcellonError::Empty`] if both files are blank,
/// * [`ExcellonError::UnknownTool`] if the `.DRL` selects a code absent from the
///   `.INF`.
pub fn parse_pcbwizard(drl: &str, inf: &str) -> Result<Excellon, ExcellonError> {
    if drl.trim().is_empty() && inf.trim().is_empty() {
        return Err(ExcellonError::Empty);
    }

    let (mut tools, fmt) = parse_inf(inf)?;

    // ----- parse the .DRL body against the legend -----
    let mut cur_tool: Option<i32> = None;
    let (mut x, mut y) = (0.0_f64, 0.0_f64);

    for raw in drl.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        // Tool select: T<digits> (PcbWizard .DRL never carries diameters here).
        if line.starts_with('T') || line.starts_with('t') {
            let digits: String = line[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(num) = digits.parse::<i32>() {
                if !tools.contains_key(&num) {
                    return Err(ExcellonError::UnknownTool(format!("T{digits}")));
                }
                cur_tool = Some(num);
            }
            continue;
        }
        // Skip common header/footer/control words.
        if line.starts_with('M')
            || line.starts_with('%')
            || line.starts_with("G90")
            || line.starts_with("G91")
            || line.starts_with("INCH")
            || line.starts_with("METRIC")
            || line.starts_with("FMAT")
        {
            continue;
        }
        // Coordinate line.
        if line.contains('X') || line.contains('Y') {
            if let Some(nx) = extract_coord(line, 'X') {
                x = decode(&nx, fmt.frac);
            }
            if let Some(ny) = extract_coord(line, 'Y') {
                y = decode(&ny, fmt.frac);
            }
            if let Some(t) = cur_tool {
                tools.get_mut(&t).unwrap().drills.push((x, y));
            }
        }
    }

    Ok(Excellon { units: fmt.units, tools })
}

/// Parse the `.INF` legend into a tool table + format.
fn parse_inf(inf: &str) -> Result<(BTreeMap<i32, Tool>, Format), ExcellonError> {
    // Provisional units (default inch) until a directive says otherwise.
    let mut fmt = Format { units: Units::Inch, frac: 4, format_seen: false };
    let mut tools: BTreeMap<i32, Tool> = BTreeMap::new();
    // Diameters captured verbatim (value + optional unit suffix) so we can
    // convert them after the file's own units are known.
    let mut raw_dias: Vec<(i32, f64, Option<DiaUnit>)> = Vec::new();

    for raw in inf.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let lower = line.to_ascii_lowercase();

        // Units directive.
        if lower.starts_with("units") || lower == "metric" || lower == "mm" || lower == "inch" {
            if lower.contains("mm") || lower.contains("metric") {
                fmt.units = Units::Mm;
            } else if lower.contains("inch") || lower.contains("in") {
                fmt.units = Units::Inch;
            }
            continue;
        }
        // Format directive: "Format, 3.3" / "FORMAT 2.4". The notation is
        // `<integer-digits>.<fractional-digits>` where each side is the *count*
        // of digits (so "3.3" => 3 fractional digits). We read the number after
        // the dot as that count.
        if lower.starts_with("format") {
            if let Some(dot) = line.find('.') {
                let after: String = line[dot + 1..]
                    .chars()
                    .skip_while(|c| c.is_whitespace())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(frac) = after.parse::<i32>() {
                    if frac > 0 {
                        fmt.frac = frac;
                        fmt.format_seen = true;
                    }
                }
            }
            continue;
        }
        // Tool legend line: must start with T<digits>.
        if (line.starts_with('T') || line.starts_with('t')) && line[1..].starts_with(|c: char| c.is_ascii_digit())
        {
            let digits: String = line[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
            let num: i32 = digits.parse().map_err(|_| {
                ExcellonError::Parse(format!("bad tool code in .INF: {line}"))
            })?;
            // Diameter: first numeric token after the code, with optional unit suffix.
            let after = &line[1 + digits.len()..];
            tools.entry(num).or_default();
            if let Some((val, unit)) = parse_diameter(after) {
                raw_dias.push((num, val, unit));
            }
            // A tool code with no diameter stays a valid (zero-dia) entry.
            continue;
        }
        // Anything else: header/comment — ignore.
    }

    // If no explicit format was given, fall back to the per-unit default.
    if !fmt.format_seen {
        fmt.frac = Format::default_for_unit(fmt.units);
    }

    // Convert captured diameters into the file's units.
    for (num, val, unit) in raw_dias {
        let dia = convert_diameter(val, unit, fmt.units);
        tools.entry(num).or_default().diameter = dia;
    }

    Ok((tools, fmt))
}

/// A diameter's explicit unit suffix in an `.INF` legend line.
#[derive(Clone, Copy)]
enum DiaUnit {
    Mm,
    Inch,
    Mil,
}

/// Parse a diameter token like `0.8mm`, `,0.032in`, `= 0.5`, `31.5mil`.
///
/// Returns the numeric value and the explicit unit suffix if present (`None`
/// means "no suffix" → take the value in the file's units).
fn parse_diameter(s: &str) -> Option<(f64, Option<DiaUnit>)> {
    let s = s.trim_start_matches([',', '=', ' ', '\t']).trim();
    if s.is_empty() {
        return None;
    }
    // Numeric prefix.
    let num: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    let val: f64 = num.parse().ok()?;
    let rest = s[num.len()..].trim().to_ascii_lowercase();
    let unit = if rest.starts_with("mm") {
        Some(DiaUnit::Mm)
    } else if rest.starts_with("mil") {
        Some(DiaUnit::Mil)
    } else if rest.starts_with("in") {
        Some(DiaUnit::Inch)
    } else {
        None
    };
    Some((val, unit))
}

/// Convert a captured diameter to the target file units.
fn convert_diameter(val: f64, unit: Option<DiaUnit>, file_units: Units) -> f64 {
    match unit {
        Some(DiaUnit::Mm) => match file_units {
            Units::Mm => val,
            Units::Inch => val / 25.4,
        },
        Some(DiaUnit::Inch) => match file_units {
            Units::Inch => val,
            Units::Mm => val * 25.4,
        },
        Some(DiaUnit::Mil) => {
            let inch = val / 1000.0;
            match file_units {
                Units::Inch => inch,
                Units::Mm => inch * 25.4,
            }
        }
        // No explicit suffix: value is already in the file's units.
        None => val,
    }
}

/// Decode a coordinate string into a real number.
///
/// An explicit decimal point is taken literally. Otherwise the integer is
/// scaled by the implied fractional digit count (`frac`).
fn decode(s: &str, frac: i32) -> f64 {
    let s = s.trim();
    if s.contains('.') {
        return s.parse::<f64>().unwrap_or(0.0);
    }
    let neg = s.starts_with('-');
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return 0.0;
    }
    let v: f64 = digits.parse().unwrap_or(0.0);
    let val = v / 10f64.powi(frac);
    if neg { -val } else { val }
}

/// Extract the numeric token following `key` (e.g. the `X` value) from a line.
fn extract_coord(l: &str, key: char) -> Option<String> {
    let chars: Vec<char> = l.chars().collect();
    let pos = chars.iter().position(|&c| c == key)?;
    let mut out = String::new();
    for &c in &chars[pos + 1..] {
        if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            out.push(c);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mm_pair_with_explicit_format() {
        // .INF: mm units, 3.3 format, two tools with mm diameters.
        let inf = "\
; PcbWizard drill legend
Units, MM
Format, 3.3
T01 0.8mm
T02 1.2mm
";
        // .DRL: integer coords in 3.3 implied format. 010000 -> 10.000, 020500 -> 20.500
        let drl = "\
%
T01
X010000Y010000
X020500Y010000
T02
X030000Y030000
M30
";
        let e = parse_pcbwizard(drl, inf).unwrap();
        assert_eq!(e.units, Units::Mm);
        assert_eq!(e.tools.len(), 2);
        assert!((e.tools[&1].diameter - 0.8).abs() < 1e-9);
        assert!((e.tools[&2].diameter - 1.2).abs() < 1e-9);
        assert_eq!(e.tools[&1].drills.len(), 2);
        assert_eq!(e.tools[&2].drills.len(), 1);
        assert_eq!(e.drill_count(), 3);
        // Coordinate decode within tolerance.
        let (x0, y0) = e.tools[&1].drills[0];
        assert!((x0 - 10.0).abs() < 1e-6, "x0={x0}");
        assert!((y0 - 10.0).abs() < 1e-6, "y0={y0}");
        let (x1, _) = e.tools[&1].drills[1];
        assert!((x1 - 20.5).abs() < 1e-6, "x1={x1}");
    }

    #[test]
    fn explicit_decimal_coords_taken_literally() {
        let inf = "Units, MM\nFormat, 3.3\nT01 1.0mm\n";
        let drl = "T01\nX12.34Y5.67\nM30\n";
        let e = parse_pcbwizard(drl, inf).unwrap();
        let (x, y) = e.tools[&1].drills[0];
        assert!((x - 12.34).abs() < 1e-6, "x={x}");
        assert!((y - 5.67).abs() < 1e-6, "y={y}");
    }

    #[test]
    fn inch_with_mil_diameter_suffix() {
        // Tool given in mils, file in inch: 32mil -> 0.032in.
        let inf = "Units, INCH\nFormat, 2.4\nT01 32mil\n";
        let drl = "T01\nX012345Y000000\nM30\n";
        let e = parse_pcbwizard(drl, inf).unwrap();
        assert_eq!(e.units, Units::Inch);
        assert!((e.tools[&1].diameter - 0.032).abs() < 1e-9, "dia={}", e.tools[&1].diameter);
        // 2.4 format: 012345 -> 1.2345
        let (x, _) = e.tools[&1].drills[0];
        assert!((x - 1.2345).abs() < 1e-6, "x={x}");
    }

    #[test]
    fn comma_separated_legend_and_bare_diameter() {
        // "T03,0.5" bare number in file (mm) units.
        let inf = "Units,MM\nFormat,3.3\nT03,0.5\n";
        let drl = "T03\nX001000Y001000\nM30\n";
        let e = parse_pcbwizard(drl, inf).unwrap();
        assert!((e.tools[&3].diameter - 0.5).abs() < 1e-9);
        let (x, _) = e.tools[&3].drills[0];
        assert!((x - 1.0).abs() < 1e-6, "x={x}");
    }

    #[test]
    fn default_format_when_unstated_mm() {
        // No Format line -> mm default 3.3.
        let inf = "Units, MM\nT01 0.8mm\n";
        let drl = "T01\nX005000Y005000\nM30\n";
        let e = parse_pcbwizard(drl, inf).unwrap();
        let (x, _) = e.tools[&1].drills[0];
        assert!((x - 5.0).abs() < 1e-6, "x={x}");
    }

    #[test]
    fn unknown_tool_in_drl_errors() {
        let inf = "Units, MM\nFormat, 3.3\nT01 0.8mm\n";
        // .DRL selects T05 which the .INF never defined.
        let drl = "T05\nX010000Y010000\nM30\n";
        let err = parse_pcbwizard(drl, inf).unwrap_err();
        assert!(matches!(err, ExcellonError::UnknownTool(_)), "got {err:?}");
    }

    #[test]
    fn both_empty_errors() {
        assert!(matches!(parse_pcbwizard("", ""), Err(ExcellonError::Empty)));
        assert!(matches!(parse_pcbwizard("   \n", "\n  "), Err(ExcellonError::Empty)));
    }

    #[test]
    fn empty_drl_yields_zero_drill_excellon() {
        // Legend present, no coordinates -> tools defined but no hits.
        let inf = "Units, MM\nFormat, 3.3\nT01 0.8mm\n";
        let e = parse_pcbwizard("", inf).unwrap();
        assert_eq!(e.tools.len(), 1);
        assert_eq!(e.drill_count(), 0);
    }

    #[test]
    fn shape_matches_standard_parser_geometry() {
        // Produced Excellon should drive tool_geometry like any other.
        let inf = "Units, MM\nFormat, 3.3\nT01 2.0mm\n";
        let drl = "T01\nX000000Y000000\nM30\n";
        let e = parse_pcbwizard(drl, inf).unwrap();
        let geo = e.tool_geometry(1, 256);
        let a = fc_geo::area(&geo);
        // 2mm dia drill -> area ~ pi * 1^2.
        assert!((a - std::f64::consts::PI).abs() < 1e-2, "area={a}");
    }
}
