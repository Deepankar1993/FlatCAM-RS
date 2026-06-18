//! Lead-in / lead-out path extension for milling tool-paths.
//!
//! Adds a short straight segment before the first point and after the last
//! point of a tool-path, collinear with the path's start/end direction. This
//! gives the cutter a tangential approach and retract instead of plunging
//! straight onto the contour, which improves edge quality and tool life.

use fc_gcode::Polyline;

/// Return a copy of `path` with a straight lead-in prepended and a lead-out
/// appended.
///
/// * Lead-in point = `path[0]` moved *backward* along the direction
///   `path[1] - path[0]` by `lead_len` (i.e. `start - dir * lead_len`).
/// * Lead-out point = `last` moved *forward* along the direction
///   `last - prev` by `lead_len` (i.e. `last + dir * lead_len`).
///
/// If `path` has fewer than two points, or `lead_len <= 0.0`, the path is
/// returned unchanged.
pub fn add_lead(path: &[(f64, f64)], lead_len: f64) -> Polyline {
    if path.len() < 2 || lead_len <= 0.0 {
        return path.to_vec();
    }

    let lead_in = lead_point(path[1], path[0], lead_len);
    let n = path.len();
    let lead_out = lead_point(path[n - 2], path[n - 1], lead_len);

    let mut out = Polyline::with_capacity(n + 2);
    out.push(lead_in);
    out.extend_from_slice(path);
    out.push(lead_out);
    out
}

/// Extend from `anchor` past `tip` by `len` along the (tip - from) direction,
/// where `from` is the point preceding `tip`. Returns `tip` extended outward.
fn lead_point(from: (f64, f64), tip: (f64, f64), len: f64) -> (f64, f64) {
    let dx = tip.0 - from.0;
    let dy = tip.1 - from.1;
    let mag = (dx * dx + dy * dy).sqrt();
    if mag == 0.0 {
        return tip;
    }
    let ux = dx / mag;
    let uy = dy / mag;
    (tip.0 + ux * len, tip.1 + uy * len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
    }

    #[test]
    fn horizontal_two_point_path() {
        let path = [(0.0, 0.0), (10.0, 0.0)];
        let result = add_lead(&path, 2.0);
        let expected = [(-2.0, 0.0), (0.0, 0.0), (10.0, 0.0), (12.0, 0.0)];
        assert_eq!(result.len(), expected.len());
        for (r, e) in result.iter().zip(expected.iter()) {
            assert!(close(*r, *e), "got {:?}, expected {:?}", r, e);
        }
    }

    #[test]
    fn zero_lead_len_unchanged() {
        let path = [(0.0, 0.0), (10.0, 0.0)];
        assert_eq!(add_lead(&path, 0.0), path.to_vec());
    }

    #[test]
    fn negative_lead_len_unchanged() {
        let path = [(0.0, 0.0), (10.0, 0.0)];
        assert_eq!(add_lead(&path, -1.0), path.to_vec());
    }

    #[test]
    fn single_point_unchanged() {
        let path = [(5.0, 5.0)];
        assert_eq!(add_lead(&path, 2.0), path.to_vec());
    }

    #[test]
    fn empty_unchanged() {
        let path: [(f64, f64); 0] = [];
        assert!(add_lead(&path, 2.0).is_empty());
    }

    #[test]
    fn diagonal_path_is_collinear() {
        // 3-4-5 triangle direction; lead_len 10 => extend by (6, 8).
        let path = [(0.0, 0.0), (3.0, 4.0)];
        let result = add_lead(&path, 10.0);
        assert!(close(result[0], (-6.0, -8.0)));
        assert!(close(result[result.len() - 1], (9.0, 12.0)));
    }
}
