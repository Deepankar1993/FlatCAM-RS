//! Raster *timing* compensation: scan-latency banding + overscan.
//!
//! This module corrects **timing** artefacts of raster / bidirectional laser
//! engraving, as opposed to [`crate::beam`], which corrects beam **shape**.
//! Nothing here models the spot geometry — it is pure polyline manipulation.
//!
//! ## Scanning / latency offset (banding)
//!
//! A controller + laser has a finite latency `t` (seconds) between commanding
//! power and the beam actually firing. While the head moves at feed `v`
//! (mm/min), this latency drags the burned mark *forward along the travel
//! direction* by `d = (v / 60) · t` mm. On a **bidirectional** raster,
//! alternate scan lines travel in opposite directions, so the mark shifts the
//! opposite way on each — the two interleaved sets of lines no longer register,
//! producing the visible "banding" stripes.
//!
//! The fix is purely geometric: shift every commanded point *backward* along
//! its own local travel direction by `d`. Because the shift follows the local
//! direction, lines going left and lines going right are corrected in opposite
//! senses, which is exactly what cancels the direction-dependent displacement.
//!
//! ## Overscan
//!
//! At the ends of a raster line the head must accelerate / decelerate; firing
//! the laser during that velocity ramp gives uneven darkening at the line ends.
//! Overscan extends the travel a fixed `margin` past each end (with the laser
//! held **off** over the extension) so the powered portion runs at constant
//! velocity. Acceleration is *not* modelled here — overscan simply prepends and
//! appends a point at the requested margin along the end segments; the caller is
//! responsible for keeping the laser off over the added span.
//!
//! All functions are pure, deterministic, std-only, and guard against
//! zero-length segments so they never panic or produce `NaN`.

/// Distance (mm) a mark is displaced by a latency of `latency_s` seconds at
/// feed `feed` (mm/**min**). Equals `feed / 60 * latency_s`. Returns `0.0` for
/// any non-positive input.
pub fn scan_offset_distance(feed: f64, latency_s: f64) -> f64 {
    if feed <= 0.0 || latency_s <= 0.0 {
        return 0.0;
    }
    (feed / 60.0) * latency_s
}

/// Unit vector of the segment `a -> b`, or `None` for a (near-)zero-length step.
fn unit(a: (f64, f64), b: (f64, f64)) -> Option<(f64, f64)> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 {
        None
    } else {
        Some((dx / len, dy / len))
    }
}

/// Shift every point of every path **backward** along its local travel
/// direction by `dist` mm, cancelling a forward latency displacement.
///
/// For point `i` the travel direction is that of the incoming segment
/// (`i-1 -> i`); point `0` uses the first segment's direction. A zero-length
/// segment inherits the previous valid direction. Paths with fewer than two
/// points are returned unchanged. Pure: returns freshly allocated vectors and
/// does not mutate the input.
pub fn apply_scan_offset(paths: &[Vec<(f64, f64)>], dist: f64) -> Vec<Vec<(f64, f64)>> {
    paths
        .iter()
        .map(|path| {
            if path.len() < 2 {
                return path.clone();
            }
            // Precompute the per-point travel direction. Point 0 uses the first
            // segment; point i (>0) uses segment (i-1 -> i). Zero-length
            // segments inherit the previous direction.
            let mut dirs: Vec<(f64, f64)> = Vec::with_capacity(path.len());
            let first_dir = unit(path[0], path[1]).unwrap_or((0.0, 0.0));
            dirs.push(first_dir);
            let mut last_dir = first_dir;
            for i in 1..path.len() {
                let d = unit(path[i - 1], path[i]).unwrap_or(last_dir);
                last_dir = d;
                dirs.push(d);
            }
            path.iter()
                .zip(dirs.iter())
                .map(|(&(x, y), &(ux, uy))| (x - ux * dist, y - uy * dist))
                .collect()
        })
        .collect()
}

/// Convenience: compensate `paths` for `latency_s` seconds of latency at feed
/// `feed` (mm/min). Equivalent to
/// `apply_scan_offset(paths, scan_offset_distance(feed, latency_s))`.
pub fn compensate_banding(
    paths: &[Vec<(f64, f64)>],
    feed: f64,
    latency_s: f64,
) -> Vec<Vec<(f64, f64)>> {
    apply_scan_offset(paths, scan_offset_distance(feed, latency_s))
}

/// Extend each open polyline outward at **both** ends by `margin` mm along the
/// direction of its first / last segment (overscan).
///
/// A new point is prepended at `first_point - unit(first_seg) * margin` and a
/// new point appended at `last_point + unit(last_seg) * margin`. Callers keep
/// the laser off over these extensions. Paths with fewer than two points are
/// returned unchanged. Pure: returns freshly allocated vectors.
pub fn overscan(paths: &[Vec<(f64, f64)>], margin: f64) -> Vec<Vec<(f64, f64)>> {
    paths
        .iter()
        .map(|path| {
            if path.len() < 2 {
                return path.clone();
            }
            let n = path.len();
            let mut out = Vec::with_capacity(n + 2);
            // Leading extension along the first segment direction.
            if let Some((ux, uy)) = unit(path[0], path[1]) {
                out.push((path[0].0 - ux * margin, path[0].1 - uy * margin));
            }
            out.extend_from_slice(path);
            // Trailing extension along the last segment direction.
            if let Some((ux, uy)) = unit(path[n - 2], path[n - 1]) {
                out.push((path[n - 1].0 + ux * margin, path[n - 1].1 + uy * margin));
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Euclidean length of a polyline.
    fn path_len(p: &[(f64, f64)]) -> f64 {
        p.windows(2)
            .map(|w| {
                let (dx, dy) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
                (dx * dx + dy * dy).sqrt()
            })
            .sum()
    }

    #[test]
    fn scan_offset_distance_basic() {
        // 600 mm/min = 10 mm/s; * 0.01 s = 0.1 mm.
        assert!((scan_offset_distance(600.0, 0.01) - 0.1).abs() < 1e-12);
    }

    #[test]
    fn scan_offset_distance_non_positive_is_zero() {
        assert_eq!(scan_offset_distance(-600.0, 0.01), 0.0);
        assert_eq!(scan_offset_distance(600.0, -0.01), 0.0);
        assert_eq!(scan_offset_distance(0.0, 0.01), 0.0);
        assert_eq!(scan_offset_distance(600.0, 0.0), 0.0);
    }

    #[test]
    fn scan_offset_shifts_rightward_path_left() {
        // Travelling +x; backward shift is -x.
        let paths = vec![vec![(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)]];
        let out = apply_scan_offset(&paths, 0.1);
        for (p, orig) in out[0].iter().zip(paths[0].iter()) {
            assert!((p.0 - (orig.0 - 0.1)).abs() < 1e-12); // x shifted left
            assert!((p.1 - orig.1).abs() < 1e-12); // y unchanged
        }
    }

    #[test]
    fn scan_offset_shifts_leftward_path_right() {
        // Travelling -x; backward shift is +x. Proves direction dependence —
        // the basis of banding cancellation on bidirectional raster.
        let paths = vec![vec![(20.0, 0.0), (10.0, 0.0), (0.0, 0.0)]];
        let out = apply_scan_offset(&paths, 0.1);
        for (p, orig) in out[0].iter().zip(paths[0].iter()) {
            assert!((p.0 - (orig.0 + 0.1)).abs() < 1e-12); // x shifted right
            assert!((p.1 - orig.1).abs() < 1e-12);
        }
    }

    #[test]
    fn scan_offset_preserves_point_count() {
        let paths = vec![vec![(0.0, 0.0), (1.0, 1.0), (2.0, 0.0), (3.0, 1.0)]];
        let out = apply_scan_offset(&paths, 0.05);
        assert_eq!(out[0].len(), paths[0].len());
    }

    #[test]
    fn scan_offset_leaves_short_paths_unchanged() {
        let paths = vec![vec![], vec![(1.0, 2.0)]];
        let out = apply_scan_offset(&paths, 0.1);
        assert_eq!(out, paths);
    }

    #[test]
    fn scan_offset_zero_length_segment_inherits_direction() {
        // Middle segment is zero-length; it must inherit the +x direction
        // rather than collapse to a zero shift (which would leave a NaN-free
        // but inconsistent point).
        let paths = vec![vec![(0.0, 0.0), (10.0, 0.0), (10.0, 0.0), (20.0, 0.0)]];
        let out = apply_scan_offset(&paths, 0.1);
        // All points shifted -x by 0.1.
        for (p, orig) in out[0].iter().zip(paths[0].iter()) {
            assert!((p.0 - (orig.0 - 0.1)).abs() < 1e-12);
            assert!(p.0.is_finite() && p.1.is_finite());
        }
    }

    #[test]
    fn compensate_banding_matches_components() {
        let paths = vec![vec![(0.0, 0.0), (10.0, 0.0)]];
        let combined = compensate_banding(&paths, 600.0, 0.01);
        let manual = apply_scan_offset(&paths, scan_offset_distance(600.0, 0.01));
        assert_eq!(combined, manual);
    }

    #[test]
    fn overscan_extends_horizontal_segment() {
        let margin = 2.0;
        let paths = vec![vec![(0.0, 5.0), (10.0, 5.0)]];
        let out = overscan(&paths, margin);
        // Two extra points added.
        assert_eq!(out[0].len(), paths[0].len() + 2);
        // New first x < old first x; new last x > old last x.
        assert!(out[0].first().unwrap().0 < paths[0].first().unwrap().0);
        assert!(out[0].last().unwrap().0 > paths[0].last().unwrap().0);
        // Extension is ~margin at each end.
        assert!((out[0].first().unwrap().0 - (0.0 - margin)).abs() < 1e-12);
        assert!((out[0].last().unwrap().0 - (10.0 + margin)).abs() < 1e-12);
        // Total length grows by ~2*margin.
        let grown = path_len(&out[0]) - path_len(&paths[0]);
        assert!((grown - 2.0 * margin).abs() < 1e-9);
    }

    #[test]
    fn overscan_leaves_short_paths_unchanged() {
        let paths = vec![vec![], vec![(3.0, 4.0)]];
        let out = overscan(&paths, 1.0);
        assert_eq!(out, paths);
    }

    #[test]
    fn overscan_no_nan_on_degenerate_ends() {
        // Degenerate end segments are skipped (no extension) rather than NaN.
        let paths = vec![vec![(1.0, 1.0), (1.0, 1.0)]];
        let out = overscan(&paths, 1.0);
        for &(x, y) in &out[0] {
            assert!(x.is_finite() && y.is_finite());
        }
    }
}
