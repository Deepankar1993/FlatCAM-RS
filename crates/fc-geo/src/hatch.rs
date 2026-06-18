//! Parallel / cross-hatch fill lines for a region.
//!
//! This is the Rust analogue of FlatCAM's `ToolPaint` "lines" / hatch fill and
//! the Gerber follow-fill hatch used to pour copper-clearance or solder-mask
//! style fills. Given a region (a `MultiPolygon`), [`hatch_lines`] produces a
//! set of straight line segments that fill the interior, spaced `spacing`
//! apart and running at `angle_deg`.
//!
//! The algorithm is a classic even-odd scanline fill:
//!
//! 1. Rotate the region by `-angle` about the origin so the requested hatch
//!    direction becomes horizontal.
//! 2. Sweep horizontal scanlines spaced `spacing` apart across the rotated
//!    bounds. For each scanline, intersect against every ring (exterior and
//!    interior) of every polygon, collect the X crossings, sort them, and pair
//!    them up (even-odd rule) into interior spans.
//! 3. Rotate each span's endpoints back by `+angle` so the segments line up
//!    with the original, un-rotated region.

use crate::{Coord, LineString, MultiPolygon, Polygon};

/// Compute hatch fill line segments for `region`.
///
/// * `spacing` — distance between adjacent hatch lines (working units). Values
///   `<= 0` yield no lines.
/// * `angle_deg` — hatch direction in degrees, CCW from +X.
///
/// Returns a `Vec` of line segments, each `vec![(x0, y0), (x1, y1)]`, in the
/// original (un-rotated) coordinate frame.
pub fn hatch_lines(
    region: &MultiPolygon<f64>,
    spacing: f64,
    angle_deg: f64,
) -> Vec<Vec<(f64, f64)>> {
    if spacing <= 0.0 || region.0.is_empty() {
        return Vec::new();
    }

    // Step 1: rotate the region so the hatch direction is horizontal.
    let rotated = crate::transform::rotate(region, -angle_deg, (0.0, 0.0));

    // Bounds of the rotated region; if it is degenerate there is nothing to do.
    let (min_x, min_y, max_x, max_y) = match crate::bounds(&rotated) {
        Some(b) => b,
        None => return Vec::new(),
    };
    if !(max_x > min_x) || !(max_y > min_y) {
        return Vec::new();
    }

    // Precompute the rotation that maps rotated-frame points back to the
    // original frame (rotate by +angle about origin).
    let theta = angle_deg.to_radians();
    let (sin_t, cos_t) = theta.sin_cos();
    let back = |x: f64, y: f64| -> (f64, f64) {
        (x * cos_t - y * sin_t, x * sin_t + y * cos_t)
    };

    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();

    // Step 2: sweep scanlines. Start half a spacing inside the lower bound so
    // lines sit nicely within the region rather than on its edge.
    let mut y = min_y + spacing * 0.5;
    while y < max_y {
        let mut crossings: Vec<f64> = Vec::new();
        for poly in &rotated.0 {
            collect_crossings(poly, y, &mut crossings);
        }
        if crossings.len() >= 2 {
            crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            // Pair up consecutive crossings (even-odd rule) into spans.
            let mut i = 0;
            while i + 1 < crossings.len() {
                let x0 = crossings[i];
                let x1 = crossings[i + 1];
                if x1 > x0 {
                    let p0 = back(x0, y);
                    let p1 = back(x1, y);
                    out.push(vec![p0, p1]);
                }
                i += 2;
            }
        }
        y += spacing;
    }

    out
}

/// Append the X coordinates where the horizontal line `y = scan_y` crosses any
/// ring (exterior + interiors) of `poly`.
fn collect_crossings(poly: &Polygon<f64>, scan_y: f64, out: &mut Vec<f64>) {
    ring_crossings(poly.exterior(), scan_y, out);
    for hole in poly.interiors() {
        ring_crossings(hole, scan_y, out);
    }
}

/// Append the X coordinates where `y = scan_y` crosses the edges of a ring.
fn ring_crossings(ring: &LineString<f64>, scan_y: f64, out: &mut Vec<f64>) {
    let coords: &Vec<Coord<f64>> = &ring.0;
    if coords.len() < 2 {
        return;
    }
    for w in coords.windows(2) {
        let a = w[0];
        let b = w[1];
        let (y0, y1) = (a.y, b.y);
        // Skip horizontal edges (no single crossing point).
        if y0 == y1 {
            continue;
        }
        // Half-open interval [min, max) so shared vertices are counted once,
        // keeping the even-odd parity correct at polygon corners.
        let (lo, hi) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
        if scan_y >= lo && scan_y < hi {
            let t = (scan_y - a.y) / (b.y - a.y);
            out.push(a.x + t * (b.x - a.x));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::centered_rect;

    fn seg_len(s: &[(f64, f64)]) -> f64 {
        let (x0, y0) = s[0];
        let (x1, y1) = s[1];
        ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt()
    }

    fn square_10() -> MultiPolygon<f64> {
        // 10x10 square centred at (5,5) => spans [0,10] x [0,10].
        MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)])
    }

    #[test]
    fn square_angle_zero_yields_full_length_lines() {
        let lines = hatch_lines(&square_10(), 1.0, 0.0);
        assert!(
            lines.len() >= 9 && lines.len() <= 10,
            "expected ~9-10 lines, got {}",
            lines.len()
        );
        for s in &lines {
            let l = seg_len(s);
            assert!((l - 10.0).abs() < 1e-6, "segment length was {l}");
        }
    }

    #[test]
    fn square_angle_ninety_yields_lines() {
        let lines = hatch_lines(&square_10(), 1.0, 90.0);
        assert!(
            lines.len() >= 9 && lines.len() <= 10,
            "expected ~9-10 lines at 90deg, got {}",
            lines.len()
        );
        for s in &lines {
            let l = seg_len(s);
            assert!((l - 10.0).abs() < 1e-6, "segment length was {l}");
        }
    }

    #[test]
    fn empty_region_yields_no_lines() {
        let empty = MultiPolygon::new(vec![]);
        assert!(hatch_lines(&empty, 1.0, 0.0).is_empty());
    }

    #[test]
    fn nonpositive_spacing_yields_no_lines() {
        assert!(hatch_lines(&square_10(), 0.0, 0.0).is_empty());
        assert!(hatch_lines(&square_10(), -1.0, 0.0).is_empty());
    }

    #[test]
    fn diagonal_hatch_produces_lines() {
        let lines = hatch_lines(&square_10(), 1.0, 45.0);
        assert!(!lines.is_empty(), "45deg hatch should produce lines");
        // Every returned segment must have positive length.
        for s in &lines {
            assert!(seg_len(s) > 0.0);
        }
    }

    #[test]
    fn region_with_hole_splits_spans() {
        // Outer 10x10 with a 4x4 hole in the middle => some scanlines must
        // produce two separate spans (4 crossings).
        let outer = centered_rect(5.0, 5.0, 10.0, 10.0);
        let hole = LineString::new(vec![
            Coord { x: 3.0, y: 3.0 },
            Coord { x: 7.0, y: 3.0 },
            Coord { x: 7.0, y: 7.0 },
            Coord { x: 3.0, y: 7.0 },
            Coord { x: 3.0, y: 3.0 },
        ]);
        let poly = Polygon::new(outer.exterior().clone(), vec![hole]);
        let region = MultiPolygon::new(vec![poly]);
        let lines = hatch_lines(&region, 1.0, 0.0);
        // At least one scanline crossing the hole yields two segments at the
        // same y, so total line count exceeds the simple square's count.
        assert!(lines.len() > 10, "hole should add extra spans, got {}", lines.len());
    }
}
