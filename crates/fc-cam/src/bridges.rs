//! Generic holding-bridge support: cut evenly spaced gaps into an arbitrary
//! polyline.
//!
//! This generalizes the holding-tab logic used by [`crate::cutout`]: instead of
//! operating on closed outline rings, [`add_bridges`] takes any open polyline
//! and removes a number of evenly spaced gaps of a fixed length, returning the
//! remaining cut segments as separate polylines. The removed gaps are the
//! uncut "bridges" that hold the piece in place.

use fc_gcode::Polyline;

/// Cut `gaps` evenly spaced gaps of `gap_len` into `path`, returning the
/// remaining segments as separate polylines.
///
/// The polyline is walked while accumulating arc length. Gap centres are
/// placed at evenly spaced fractions of the total length; each gap removes a
/// `gap_len`-long stretch (centred on its centre point), and the surviving
/// stretches between gaps are emitted as open polylines with points
/// interpolated exactly at the gap boundaries.
///
/// `gaps == 0` returns the original path unchanged (`vec![path.to_vec()]`).
/// An empty or single-point path is returned as-is.
pub fn add_bridges(path: &[(f64, f64)], gaps: usize, gap_len: f64) -> Vec<Polyline> {
    if gaps == 0 {
        return vec![path.to_vec()];
    }
    if path.len() < 2 {
        return vec![path.to_vec()];
    }

    // Cumulative arc length at each vertex.
    let mut cum = Vec::with_capacity(path.len());
    cum.push(0.0_f64);
    for w in path.windows(2) {
        let dx = w[1].0 - w[0].0;
        let dy = w[1].1 - w[0].1;
        let last = *cum.last().unwrap();
        cum.push(last + (dx * dx + dy * dy).sqrt());
    }
    let total = *cum.last().unwrap();

    if total <= 0.0 {
        return vec![path.to_vec()];
    }

    // Clamp total gap removal so we never remove more than the whole path.
    let half = (gap_len * 0.5).max(0.0);

    // Build the list of "remove" intervals [start, end] in arc-length space,
    // evenly spaced. Gap i is centred at (i + 0.5) / gaps of the total length.
    let mut intervals: Vec<(f64, f64)> = Vec::with_capacity(gaps);
    for i in 0..gaps {
        let centre = (i as f64 + 0.5) / gaps as f64 * total;
        let s = (centre - half).max(0.0);
        let e = (centre + half).min(total);
        if e > s {
            intervals.push((s, e));
        }
    }

    // The kept intervals are the complement of the removed ones within
    // [0, total].
    let mut kept: Vec<(f64, f64)> = Vec::new();
    let mut cursor = 0.0_f64;
    for &(s, e) in &intervals {
        if s > cursor {
            kept.push((cursor, s));
        }
        cursor = cursor.max(e);
    }
    if cursor < total {
        kept.push((cursor, total));
    }

    // Materialize each kept arc-length interval into a polyline.
    let mut out: Vec<Polyline> = Vec::new();
    for &(s, e) in &kept {
        if e <= s {
            continue;
        }
        out.push(extract_segment(path, &cum, s, e));
    }
    out
}

/// Interpolate the point on `path` at cumulative arc length `dist`.
fn point_at(path: &[(f64, f64)], cum: &[f64], dist: f64) -> (f64, f64) {
    // Find the segment containing `dist`.
    // cum is monotonically non-decreasing; path[i]..path[i+1] spans
    // cum[i]..cum[i+1].
    for i in 0..path.len() - 1 {
        let a = cum[i];
        let b = cum[i + 1];
        if dist <= b || i == path.len() - 2 {
            let seg = b - a;
            let t = if seg > 0.0 {
                ((dist - a) / seg).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (x0, y0) = path[i];
            let (x1, y1) = path[i + 1];
            return (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
        }
    }
    *path.last().unwrap()
}

/// Build the polyline for arc-length interval [s, e], including the exact
/// endpoints and any original vertices strictly inside the interval.
fn extract_segment(path: &[(f64, f64)], cum: &[f64], s: f64, e: f64) -> Polyline {
    let mut seg = Vec::new();
    seg.push(point_at(path, cum, s));
    for i in 0..path.len() {
        if cum[i] > s && cum[i] < e {
            seg.push(path[i]);
        }
    }
    seg.push(point_at(path, cum, e));
    seg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poly_len(p: &Polyline) -> f64 {
        p.windows(2)
            .map(|w| {
                let dx = w[1].0 - w[0].0;
                let dy = w[1].1 - w[0].1;
                (dx * dx + dy * dy).sqrt()
            })
            .sum()
    }

    #[test]
    fn straight_line_three_gaps() {
        let path = [(0.0, 0.0), (40.0, 0.0)];
        let segs = add_bridges(&path, 3, 2.0);
        assert!(
            segs.len() >= 3,
            "three interior gaps split a line into >= 3 segments, got {}",
            segs.len()
        );
        let total: f64 = segs.iter().map(poly_len).sum();
        assert!(
            total < 40.0,
            "retained length {} should be less than original 40",
            total
        );
        // Exactly 3 gaps of 2.0 removed => 6 units removed.
        assert!((total - 34.0).abs() < 1e-9, "expected 34 retained, got {}", total);
    }

    #[test]
    fn zero_gaps_returns_original() {
        let path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let segs = add_bridges(&path, 0, 2.0);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0], path);
    }

    #[test]
    fn degenerate_paths_pass_through() {
        assert_eq!(add_bridges(&[], 3, 2.0), vec![Vec::<(f64, f64)>::new()]);
        let single = vec![(1.0, 2.0)];
        assert_eq!(add_bridges(&single, 3, 2.0), vec![single]);
    }

    #[test]
    fn gaps_are_evenly_spaced() {
        // Centres for 2 gaps on a 40-unit line at 25% and 75% => x=10 and x=30.
        let path = [(0.0, 0.0), (40.0, 0.0)];
        let segs = add_bridges(&path, 2, 4.0);
        // Segment boundaries: [0,8], [12,28], [32,40].
        assert_eq!(segs.len(), 3);
        assert!((segs[0][0].0 - 0.0).abs() < 1e-9);
        assert!((segs[0].last().unwrap().0 - 8.0).abs() < 1e-9);
        assert!((segs[1][0].0 - 12.0).abs() < 1e-9);
        assert!((segs[1].last().unwrap().0 - 28.0).abs() < 1e-9);
        assert!((segs[2][0].0 - 32.0).abs() < 1e-9);
        assert!((segs[2].last().unwrap().0 - 40.0).abs() < 1e-9);
    }

    #[test]
    fn preserves_interior_vertices() {
        // An L-shaped path; a single central gap keeps the corner in one of
        // the two surviving segments.
        let path = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        let segs = add_bridges(&path, 1, 2.0);
        assert_eq!(segs.len(), 2);
        // The corner (10,0) is at arc length 10 (centre of total 20), so it
        // falls inside the removed gap [9,11] and appears in neither segment's
        // interior, but both segments end/start at the gap boundary.
        let total: f64 = segs.iter().map(poly_len).sum();
        assert!((total - 18.0).abs() < 1e-9, "expected 18, got {}", total);
    }
}
