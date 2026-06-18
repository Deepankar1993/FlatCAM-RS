//! Excellon text writer — serializes an [`Excellon`] back to drill-file text.
//!
//! Produces a minimal but round-trippable Excellon program: an `M48` header
//! declaring units and leading-zero suppression (`LZ`), one `T<n>C<dia>` tool
//! definition per tool (sorted by tool number), the `%` header terminator, then
//! a `T<n>` selection followed by `X<x>Y<y>` drill lines (and `G85` slot lines)
//! for each tool, finishing with `M30`.
//!
//! Coordinates and diameters are written in explicit decimal form (with a
//! decimal point), so the values survive a re-parse regardless of the
//! zero-suppression / format rules used on input.

use crate::{Excellon, Units};

/// Serialize an [`Excellon`] to Excellon program text.
pub fn write_excellon(e: &Excellon) -> String {
    let mut s = String::new();

    s.push_str("M48\n");
    match e.units {
        Units::Mm => s.push_str("METRIC,LZ\n"),
        Units::Inch => s.push_str("INCH,LZ\n"),
    }

    // Tool definitions, sorted by tool number (BTreeMap iterates in order).
    for (&num, tool) in &e.tools {
        s.push_str(&format!("T{}C{}\n", num, fmt_dia(tool.diameter)));
    }

    s.push_str("%\n");

    // Drill / slot data, per tool in sorted order.
    for (&num, tool) in &e.tools {
        s.push_str(&format!("T{num}\n"));
        for &(x, y) in &tool.drills {
            s.push_str(&format!("X{}Y{}\n", fmt_coord(x), fmt_coord(y)));
        }
        for &((sx, sy), (ex, ey)) in &tool.slots {
            s.push_str(&format!(
                "X{}Y{}G85X{}Y{}\n",
                fmt_coord(sx),
                fmt_coord(sy),
                fmt_coord(ex),
                fmt_coord(ey),
            ));
        }
    }

    s.push_str("M30\n");
    s
}

/// Format a coordinate with 3 decimal places.
fn fmt_coord(v: f64) -> String {
    format!("{v:.3}")
}

/// Format a tool diameter with 3 decimal places.
fn fmt_dia(v: f64) -> String {
    format!("{v:.3}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn round_trips_drills_and_diameter() {
        let src = "\
M48
METRIC,TZ
T1C0.800
T2C1.200
%
T1
X10.0Y10.0
X20.0Y10.0
T2
X30.0Y30.0
M30
";
        let original = parse(src).unwrap();
        let text = write_excellon(&original);
        let reparsed = parse(&text).unwrap();

        // Drill counts preserved overall and per tool.
        assert_eq!(reparsed.drill_count(), original.drill_count());
        assert_eq!(reparsed.tools.len(), original.tools.len());
        assert_eq!(reparsed.tools[&1].drills.len(), 2);
        assert_eq!(reparsed.tools[&2].drills.len(), 1);

        // Tool diameters preserved.
        assert!((reparsed.tools[&1].diameter - 0.8).abs() < 1e-6);
        assert!((reparsed.tools[&2].diameter - 1.2).abs() < 1e-6);

        // Units preserved.
        assert_eq!(reparsed.units, original.units);
    }

    #[test]
    fn drill_coordinates_preserved() {
        let src = "\
M48
METRIC,LZ
T1C1.000
%
T1
X12.345Y67.890
M30
";
        let e = parse(src).unwrap();
        let text = write_excellon(&e);
        let re = parse(&text).unwrap();
        let (x, y) = re.tools[&1].drills[0];
        assert!((x - 12.345).abs() < 1e-6, "x {x}");
        assert!((y - 67.890).abs() < 1e-6, "y {y}");
    }

    #[test]
    fn slots_round_trip_via_g85() {
        let src = "\
M48
METRIC,LZ
T1C1.000
%
T1
X10.0Y10.0G85X20.0Y10.0
M30
";
        let e = parse(src).unwrap();
        let text = write_excellon(&e);
        assert!(text.contains("G85"), "expected G85 in output: {text}");
        let re = parse(&text).unwrap();
        assert_eq!(re.tools[&1].slots.len(), 1);
        let ((sx, sy), (ex, ey)) = re.tools[&1].slots[0];
        assert!((sx - 10.0).abs() < 1e-6);
        assert!((sy - 10.0).abs() < 1e-6);
        assert!((ex - 20.0).abs() < 1e-6);
        assert!((ey - 10.0).abs() < 1e-6);
    }

    #[test]
    fn header_declares_inch_units() {
        let src = "\
M48
INCH,LZ
T1C0.040
%
T1
X1.0Y1.0
M30
";
        let e = parse(src).unwrap();
        let text = write_excellon(&e);
        assert!(text.starts_with("M48\n"), "{text}");
        assert!(text.contains("INCH,LZ"), "{text}");
        assert!(text.trim_end().ends_with("M30"), "{text}");
    }
}
