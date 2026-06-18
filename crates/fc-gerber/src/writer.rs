//! Gerber (RS-274X) writer — export `geo` geometry back to Gerber text.
//!
//! Companion to the parser in this crate. Given a solid [`geo::MultiPolygon`],
//! [`write_gerber`] produces a minimal but valid RS-274X file: a fixed
//! `34`-format coordinate spec, the chosen unit, a single tiny circular
//! aperture, and one filled region (`G36`/`G37`) per polygon exterior ring.
//!
//! Coordinate encoding uses the `34` format (3 integer + 4 fractional digits)
//! with leading-zero suppression: each value `v` is emitted as the integer
//! `round(v * 10000)`. With 4 fractional digits the parser's `FSLAX34Y34`
//! reader divides by `10^4`, round-tripping the value.
//!
//! Interior holes (polygon interiors) are emitted as clear-polarity (`%LPC*%`)
//! regions so that round-tripping preserves the cut-out area; polarity is reset
//! to dark (`%LPD*%`) afterwards.

use geo::MultiPolygon;
use std::fmt::Write as _;

/// Encode a single coordinate value into a `34`-format Gerber integer string
/// (value scaled by 10^4, leading zeros suppressed). A leading `-` is kept for
/// negatives; zero encodes as `"0"`.
fn encode_coord(v: f64) -> String {
    let scaled = (v * 10_000.0).round() as i64;
    scaled.to_string()
}

/// Serialize a [`MultiPolygon`] to RS-274X Gerber text.
///
/// * `metric` — `true` for millimetres (`%MOMM*%`), `false` for inches
///   (`%MOIN*%`).
///
/// Each polygon's exterior ring is emitted as a dark filled region; each
/// interior ring is emitted as a clear (cut-out) region.
pub fn write_gerber(mp: &MultiPolygon<f64>, metric: bool) -> String {
    let mut out = String::new();

    // Format spec: 3 integer + 4 fractional digits, absolute, leading-zero omit.
    out.push_str("%FSLAX34Y34*%\n");
    out.push_str(if metric { "%MOMM*%\n" } else { "%MOIN*%\n" });

    // One small circular aperture (0.001 unit) — required even for regions.
    out.push_str("%ADD10C,0.00100*%\n");
    out.push_str("D10*\n");

    let emit_ring = |out: &mut String, ring: &geo::LineString<f64>| {
        // Drop a duplicated closing point so we emit each vertex once; the
        // region is implicitly closed by G37.
        let coords: Vec<&geo::Coord<f64>> = {
            let mut c: Vec<&geo::Coord<f64>> = ring.0.iter().collect();
            if c.len() >= 2 && c.first() == c.last() {
                c.pop();
            }
            c
        };
        if coords.len() < 3 {
            return;
        }
        out.push_str("G36*\n");
        for (idx, pt) in coords.iter().enumerate() {
            let op = if idx == 0 { "D02" } else { "D01" };
            let _ = writeln!(
                out,
                "X{}Y{}{}*",
                encode_coord(pt.x),
                encode_coord(pt.y),
                op
            );
        }
        out.push_str("G37*\n");
    };

    for poly in &mp.0 {
        // Exterior: dark region.
        emit_ring(&mut out, poly.exterior());

        // Interiors: clear-polarity cut-outs.
        let interiors = poly.interiors();
        if !interiors.is_empty() {
            out.push_str("%LPC*%\n");
            for hole in interiors {
                emit_ring(&mut out, hole);
            }
            out.push_str("%LPD*%\n");
        }
    }

    out.push_str("M02*\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn header_and_region_present() {
        let mp = MultiPolygon::new(vec![fc_geo::centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let s = write_gerber(&mp, true);
        assert!(s.contains("%FSLAX34Y34*%"), "missing format spec:\n{s}");
        assert!(s.contains("G36"), "missing region open:\n{s}");
        assert!(s.contains("G37"), "missing region close");
        assert!(s.contains("%MOMM*%"), "missing metric units");
        assert!(s.contains("M02*"), "missing end of file");
        assert!(s.contains("%ADD10C,0.00100*%"), "missing aperture def");
    }

    #[test]
    fn imperial_unit_token() {
        let mp = MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 1.0, 1.0)]);
        let s = write_gerber(&mp, false);
        assert!(s.contains("%MOIN*%"), "missing inch units:\n{s}");
    }

    #[test]
    fn encode_coord_round_trip_scale() {
        // 5.0 -> 50000 in 34-format.
        assert_eq!(encode_coord(5.0), "50000");
        assert_eq!(encode_coord(-2.5), "-25000");
        assert_eq!(encode_coord(0.0), "0");
    }

    #[test]
    fn round_trip_area_within_one_percent() {
        let mp = MultiPolygon::new(vec![fc_geo::centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let s = write_gerber(&mp, true);
        let parsed = parse(&s).expect("re-parse written gerber");
        let a = fc_geo::area(&parsed.solid_geometry);
        let expected = 100.0;
        assert!(
            (a - expected).abs() / expected < 0.01,
            "round-trip area {a}, expected ~{expected}\n{s}"
        );
    }
}
