//! CSV/whitespace text parsing for calibration tables.
//!
//! Lets the CLI/GUI accept measured focus-ramp kerf tables and power-curve
//! sample tables that the user typed or pasted, then feed them to
//! [`crate::calfit::fit_astig`] / [`crate::powercurve::PowerCurve::from_samples`].
//!
//! Pure text parsing — there is **no file I/O** here. Callers read the file (or
//! capture the pasted text) and hand the string to these functions.
//!
//! Each line carries a fixed number of numeric fields, comma **or** whitespace
//! separated. Parsing is lenient: blank lines, `#` comments, header rows (e.g.
//! `z,width_x,width_y`), and any line whose required fields don't all parse to a
//! finite `f64` are silently skipped. Trailing extra columns are ignored.

use crate::calfit::KerfMeasurement;

/// Split a line into numeric tokens, returning the first `need` finite `f64`
/// values if (and only if) the line yields at least that many up front.
///
/// Commas are tried first; if comma splitting doesn't supply enough numeric
/// fields, the whole line is re-split on whitespace. Tokens are trimmed. Returns
/// `None` for comment/blank lines or any line whose leading `need` fields are
/// not all finite numbers (this naturally rejects header rows).
fn parse_fields(line: &str, need: usize) -> Option<Vec<f64>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    // Try comma separation first, then fall back to whitespace.
    let try_split = |sep_comma: bool| -> Option<Vec<f64>> {
        let toks: Vec<&str> = if sep_comma {
            trimmed.split(',').collect()
        } else {
            trimmed.split_whitespace().collect()
        };
        let mut out = Vec::with_capacity(need);
        for tok in toks {
            if out.len() == need {
                break; // ignore trailing extra columns
            }
            let v: f64 = tok.trim().parse().ok()?;
            if !v.is_finite() {
                return None;
            }
            out.push(v);
        }
        if out.len() == need {
            Some(out)
        } else {
            None
        }
    };

    try_split(true).or_else(|| try_split(false))
}

/// Parse a kerf-measurement table from CSV/whitespace text. Each non-empty,
/// non-comment line holds three numbers: `z, width_x, width_y` (comma OR
/// whitespace separated). Lines whose first non-space char is '#' or that
/// start with a non-numeric header (e.g. "z,width_x,width_y") are skipped.
/// Returns the parsed measurements; malformed lines are skipped (lenient).
pub fn parse_kerf_csv(text: &str) -> Vec<KerfMeasurement> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(f) = parse_fields(line, 3) {
            out.push(KerfMeasurement {
                z: f[0],
                width_x: f[1],
                width_y: f[2],
            });
        }
    }
    out
}

/// Parse a power-curve sample table: each line `power, depth` (two numbers,
/// comma or whitespace separated). Same comment/header/lenient rules. Returns
/// `(power, depth)` pairs suitable for `crate::powercurve::PowerCurve::from_samples`.
pub fn parse_power_csv(text: &str) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(f) = parse_fields(line, 2) {
            out.push((f[0], f[1]));
        }
    }
    out
}

/// Serialize kerf measurements back to CSV (with a header line
/// `z,width_x,width_y`), one row per measurement, 6-dp formatting. Useful for
/// round-trip / export.
pub fn kerf_to_csv(measurements: &[KerfMeasurement]) -> String {
    let mut s = String::from("z,width_x,width_y\n");
    for m in measurements {
        s.push_str(&format!("{:.6},{:.6},{:.6}\n", m.z, m.width_x, m.width_y));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    #[test]
    fn parse_kerf_mixed_separators_and_skips() {
        let text = "\
z,width_x,width_y
# focus ramp measured by hand
-0.5, 0.090, 0.110

0.0,0.060,0.100
0.5 0.085 0.120
this,is,garbage
1.0,0.130,0.150,extra_ignored
";
        let m = parse_kerf_csv(text);
        assert_eq!(m.len(), 4, "expected 4 good rows, got {}", m.len());

        assert!((m[0].z - -0.5).abs() < EPS);
        assert!((m[0].width_x - 0.090).abs() < EPS);
        assert!((m[0].width_y - 0.110).abs() < EPS);

        assert!((m[1].z - 0.0).abs() < EPS);
        assert!((m[1].width_x - 0.060).abs() < EPS);
        assert!((m[1].width_y - 0.100).abs() < EPS);

        // Whitespace-separated row.
        assert!((m[2].z - 0.5).abs() < EPS);
        assert!((m[2].width_x - 0.085).abs() < EPS);
        assert!((m[2].width_y - 0.120).abs() < EPS);

        // Trailing extra column ignored.
        assert!((m[3].z - 1.0).abs() < EPS);
        assert!((m[3].width_x - 0.130).abs() < EPS);
        assert!((m[3].width_y - 0.150).abs() < EPS);
    }

    #[test]
    fn crlf_parses_identically() {
        let lf = "z,width_x,width_y\n-0.5,0.09,0.11\n0.0,0.06,0.10\n0.5 0.085 0.12\n";
        let crlf = "z,width_x,width_y\r\n-0.5,0.09,0.11\r\n0.0,0.06,0.10\r\n0.5 0.085 0.12\r\n";
        let a = parse_kerf_csv(lf);
        let b = parse_kerf_csv(crlf);
        assert_eq!(a.len(), b.len());
        assert_eq!(a.len(), 3);
        for (x, y) in a.iter().zip(b.iter()) {
            assert!((x.z - y.z).abs() < EPS);
            assert!((x.width_x - y.width_x).abs() < EPS);
            assert!((x.width_y - y.width_y).abs() < EPS);
        }
    }

    #[test]
    fn parse_power_skips_header_and_bad_row() {
        let text = "\
power,depth
# diode power curve
10, 0.02
50,0.11
80 0.18
oops,bad
100,0.25,junk
";
        let p = parse_power_csv(text);
        assert_eq!(p.len(), 4, "expected 4 good rows, got {}", p.len());
        assert!((p[0].0 - 10.0).abs() < EPS && (p[0].1 - 0.02).abs() < EPS);
        assert!((p[1].0 - 50.0).abs() < EPS && (p[1].1 - 0.11).abs() < EPS);
        assert!((p[2].0 - 80.0).abs() < EPS && (p[2].1 - 0.18).abs() < EPS);
        assert!((p[3].0 - 100.0).abs() < EPS && (p[3].1 - 0.25).abs() < EPS);
    }

    #[test]
    fn round_trip_recovers_measurements() {
        let m = vec![
            KerfMeasurement { z: -0.5, width_x: 0.090123, width_y: 0.110456 },
            KerfMeasurement { z: 0.0, width_x: 0.060000, width_y: 0.100000 },
            KerfMeasurement { z: 0.75, width_x: 0.123456, width_y: 0.150789 },
        ];
        let csv = kerf_to_csv(&m);
        assert!(csv.starts_with("z,width_x,width_y\n"));
        let back = parse_kerf_csv(&csv);
        assert_eq!(back.len(), m.len());
        for (o, r) in m.iter().zip(back.iter()) {
            assert!((o.z - r.z).abs() < EPS, "z {} vs {}", o.z, r.z);
            assert!((o.width_x - r.width_x).abs() < EPS);
            assert!((o.width_y - r.width_y).abs() < EPS);
        }
    }

    #[test]
    fn empty_and_all_comment_input_is_empty() {
        assert!(parse_kerf_csv("").is_empty());
        assert!(parse_power_csv("").is_empty());

        let comments = "# header\n\n   \n# another comment\n";
        assert!(parse_kerf_csv(comments).is_empty());
        assert!(parse_power_csv(comments).is_empty());

        // Header-only kerf export round-trips to empty.
        assert!(parse_kerf_csv(&kerf_to_csv(&[])).is_empty());
    }
}
