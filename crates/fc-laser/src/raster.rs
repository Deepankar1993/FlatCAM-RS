//! Bidirectional (boustrophedon) raster scan-fill generation.
//!
//! Raster engraving fills a region by sweeping straight scan lines across it.
//! A *unidirectional* raster always travels in the same direction (e.g. always
//! left-to-right), flying back to the start of every line with the laser off —
//! correct but slow. A **boustrophedon** ("as the ox ploughs") raster instead
//! reverses direction on alternate lines: line 0 runs left-to-right, line 1
//! runs right-to-left, line 2 left-to-right, and so on. No wasted return moves,
//! so it is the standard fast raster pattern.
//!
//! [`fc_geo::hatch_lines`] already produces the parallel fill spans clipped to
//! the region, but it does **not** guarantee they come back in scan order, and
//! every span runs in whatever direction the clipping happened to emit. To turn
//! those raw spans into a real boustrophedon raster this module:
//!
//! 1. Sorts the spans by their perpendicular offset so physically adjacent
//!    lines are adjacent in the list.
//! 2. Orients every span consistently in the `+travel` direction.
//! 3. Reverses every other line so consecutive lines travel opposite ways.
//!
//! ## Why banding / overscan apply here
//!
//! The bidirectional pattern is exactly what makes a laser's command→fire
//! latency visible: the latency drags the burned mark *forward along travel*,
//! and because alternate lines travel opposite ways the two interleaved line
//! sets shift in opposite senses and stop registering — the classic "banding"
//! stripes. [`crate::banding::compensate_banding`] cancels this by shifting each
//! point backward along its own local travel direction, which only works once
//! the lines actually alternate direction (as they do here). [`crate::banding::overscan`]
//! likewise extends each line past its ends so the powered portion runs at
//! constant velocity. Both are wired in by [`raster_fill_banded`].
//!
//! All functions are pure, deterministic, std-only, and guard empty input so
//! they never panic.

use crate::banding::{compensate_banding, overscan};

/// Bidirectional (boustrophedon) raster scan of `region` at line `spacing`
/// and `angle_deg`. Produces the hatch spans SORTED by their perpendicular
/// offset (so adjacent lines are spatially adjacent), with every OTHER line's
/// two points reversed so consecutive lines travel in OPPOSITE directions
/// (the classic back-and-forth raster). Returns one 2-point polyline per scan
/// line. Empty region or non-positive spacing -> empty Vec.
pub fn raster_scan(
    region: &fc_geo::MultiPolygon<f64>,
    spacing: f64,
    angle_deg: f64,
) -> Vec<Vec<(f64, f64)>> {
    // `hatch_lines` already guards spacing <= 0 and empty regions, but we guard
    // too so the sort/orient logic below never runs on garbage.
    if spacing <= 0.0 || region.0.is_empty() {
        return Vec::new();
    }

    let mut spans = fc_geo::hatch_lines(region, spacing, angle_deg);
    if spans.is_empty() {
        return Vec::new();
    }

    // Travel unit `u = (cos, sin)`; perpendicular unit `p = (-sin, cos)`.
    let theta = angle_deg.to_radians();
    let (sin_t, cos_t) = theta.sin_cos();
    let u = (cos_t, sin_t);
    let p = (-sin_t, cos_t);

    // Sort by perpendicular offset of the span midpoint so adjacent list
    // entries are physically adjacent scan lines.
    spans.sort_by(|a, b| {
        let ka = perp_key(a, p);
        let kb = perp_key(b, p);
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (i, span) in spans.iter_mut().enumerate() {
        // First orient every span consistently in the `+u` direction so the
        // boustrophedon is well-defined regardless of how `hatch_lines` emitted
        // the endpoints.
        if span.len() == 2 {
            let dx = span[1].0 - span[0].0;
            let dy = span[1].1 - span[0].1;
            if dx * u.0 + dy * u.1 < 0.0 {
                span.swap(0, 1);
            }
            // Then reverse odd-indexed lines so they run opposite to neighbours.
            if i % 2 == 1 {
                span.swap(0, 1);
            }
        }
    }

    spans
}

/// `raster_scan` plus timing compensation: each line gets `overscan(margin)`
/// at its ends, then the whole set is run through `compensate_banding(feed,
/// latency_s)` so the bidirectional latency offset is cancelled. Returns the
/// compensated polylines. `margin`/`latency_s`/`feed` of 0 are valid no-ops
/// for that stage.
pub fn raster_fill_banded(
    region: &fc_geo::MultiPolygon<f64>,
    spacing: f64,
    angle_deg: f64,
    feed: f64,
    latency_s: f64,
    overscan_margin: f64,
) -> Vec<Vec<(f64, f64)>> {
    let lines = raster_scan(region, spacing, angle_deg);
    let lines = overscan(&lines, overscan_margin);
    compensate_banding(&lines, feed, latency_s)
}

/// Perpendicular-offset sort key: midpoint of `span` dotted with `p`.
fn perp_key(span: &[(f64, f64)], p: (f64, f64)) -> f64 {
    if span.len() < 2 {
        // Degenerate span: use whatever point exists, else 0.
        return span.first().map_or(0.0, |&(x, y)| x * p.0 + y * p.1);
    }
    let mx = (span[0].0 + span[1].0) * 0.5;
    let my = (span[0].1 + span[1].1) * 0.5;
    mx * p.0 + my * p.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_20() -> fc_geo::MultiPolygon<f64> {
        // 20x20 square centred at origin => spans [-10,10] x [-10,10].
        fc_geo::MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 20.0, 20.0)])
    }

    fn perp_key_pub(span: &[(f64, f64)], p: (f64, f64)) -> f64 {
        perp_key(span, p)
    }

    /// Horizontal extent (max x - min x) across all points of all lines.
    fn horiz_extent(lines: &[Vec<(f64, f64)>]) -> f64 {
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        for line in lines {
            for &(x, _) in line {
                if x < min_x {
                    min_x = x;
                }
                if x > max_x {
                    max_x = x;
                }
            }
        }
        if min_x.is_finite() && max_x.is_finite() {
            max_x - min_x
        } else {
            0.0
        }
    }

    #[test]
    fn boustrophedon_alternates_direction_at_angle_zero() {
        let lines = raster_scan(&square_20(), 2.0, 0.0);
        assert!(lines.len() >= 2, "expected multiple lines, got {}", lines.len());

        // Core property: the sign of (end.x - start.x) flips between
        // consecutive lines.
        for w in lines.windows(2) {
            let a = w[0][1].0 - w[0][0].0;
            let b = w[1][1].0 - w[1][0].0;
            assert!(a.abs() > 1e-9 && b.abs() > 1e-9, "lines must have x extent");
            assert!(
                a.signum() != b.signum(),
                "consecutive lines must travel opposite x directions: {a} then {b}"
            );
        }
    }

    #[test]
    fn lines_sorted_by_perpendicular_key() {
        let angle = 0.0_f64;
        let lines = raster_scan(&square_20(), 2.0, angle);
        let theta = angle.to_radians();
        let (sin_t, cos_t) = theta.sin_cos();
        let p = (-sin_t, cos_t);

        let mut prev = f64::NEG_INFINITY;
        for line in &lines {
            let k = perp_key_pub(line, p);
            assert!(
                k >= prev - 1e-9,
                "perpendicular keys must be non-decreasing: {prev} then {k}"
            );
            prev = k;
        }
    }

    #[test]
    fn overscan_widens_horizontal_extent() {
        let plain = raster_scan(&square_20(), 2.0, 0.0);
        let banded = raster_fill_banded(&square_20(), 2.0, 0.0, 600.0, 0.01, 3.0);
        assert!(!plain.is_empty() && !banded.is_empty());
        assert!(
            horiz_extent(&banded) >= horiz_extent(&plain) - 1e-9,
            "overscan should widen extent: plain {} vs banded {}",
            horiz_extent(&plain),
            horiz_extent(&banded)
        );
    }

    #[test]
    fn all_zero_compensation_preserves_line_count() {
        let plain = raster_scan(&square_20(), 2.0, 0.0);
        let banded = raster_fill_banded(&square_20(), 2.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(
            plain.len(),
            banded.len(),
            "zero feed/latency/margin must be a pure no-op on line count"
        );
    }

    #[test]
    fn empty_region_yields_no_lines() {
        let empty = fc_geo::MultiPolygon::new(vec![]);
        assert!(raster_scan(&empty, 2.0, 0.0).is_empty());
        assert!(raster_fill_banded(&empty, 2.0, 0.0, 600.0, 0.01, 3.0).is_empty());
    }

    #[test]
    fn nonpositive_spacing_yields_no_lines() {
        assert!(raster_scan(&square_20(), 0.0, 0.0).is_empty());
        assert!(raster_scan(&square_20(), -1.0, 0.0).is_empty());
    }

    #[test]
    fn angle_ninety_still_alternates() {
        // Sanity at a different angle: spans run vertically, alternation is in y.
        let lines = raster_scan(&square_20(), 2.0, 90.0);
        assert!(lines.len() >= 2);
        for w in lines.windows(2) {
            let a = w[0][1].1 - w[0][0].1;
            let b = w[1][1].1 - w[1][0].1;
            assert!(a.abs() > 1e-9 && b.abs() > 1e-9);
            assert!(a.signum() != b.signum(), "should alternate in y at 90deg");
        }
    }
}
