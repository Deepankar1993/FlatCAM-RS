//! Contour-parallel pocketing (concentric offsets).
//!
//! An alternative to the scanline-based [`crate::paint`] infill: instead of
//! filling a region with parallel straight lines, this clears it with a series
//! of concentric ring tool-paths, each one stepped inward from the boundary by
//! `tool_dia·(1−overlap)`. This mirrors FlatCAM's "Standard"/contour-parallel
//! paint method, where the cutter follows the shape of the region.
//!
//! The region is first inset by `tool_dia/2` so the cutter edge stays inside,
//! then repeatedly inset by increasing multiples of the step. Every non-empty
//! offset contributes its rings (exterior + interiors) as closed polylines.
//! Rings are returned outermost first, working inward.

use fc_gcode::Polyline;
use fc_geo::{offset, MultiPolygon};

/// Maximum number of inward offset iterations before bailing out, guarding
/// against pathological geometry that never reduces to the empty set.
const MAX_ITERS: usize = 1000;

/// Generate contour-parallel (concentric) pocketing tool-paths for a region.
///
/// `tool_dia` is the cutter diameter and `overlap` is the fractional overlap
/// (0.0..1.0) between adjacent rings. The first ring sits `tool_dia/2` inside
/// the region boundary; each subsequent ring steps inward by
/// `tool_dia·(1−overlap)`. Returns all ring polylines ordered outer to inner;
/// an empty region yields an empty vector.
pub fn contour_parallel(region: &MultiPolygon<f64>, tool_dia: f64, overlap: f64) -> Vec<Polyline> {
    let start = tool_dia / 2.0;
    let step = (tool_dia * (1.0 - overlap.clamp(0.0, 0.999))).max(1e-6);

    let mut out: Vec<Polyline> = Vec::new();
    for i in 0..MAX_ITERS {
        let dist = start + (i as f64) * step;
        let inner = offset(region, -dist);
        if inner.0.is_empty() {
            break;
        }
        rings_into(&inner, &mut out);
    }
    out
}

/// Append every ring (exterior + interiors) of `mp` as a closed polyline.
fn rings_into(mp: &MultiPolygon<f64>, out: &mut Vec<Polyline>) {
    for poly in &mp.0 {
        let ext: Polyline = poly.exterior().coords().map(|c| (c.x, c.y)).collect();
        if ext.len() >= 2 {
            out.push(ext);
        }
        for hole in poly.interiors() {
            let h: Polyline = hole.coords().map(|c| (c.x, c.y)).collect();
            if h.len() >= 2 {
                out.push(h);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::centered_rect;

    #[test]
    fn concentric_rings_for_a_square() {
        // 20x20 square, 1mm tool, no overlap -> several concentric rings.
        let region = MultiPolygon::new(vec![centered_rect(10.0, 10.0, 20.0, 20.0)]);
        let rings = contour_parallel(&region, 1.0, 0.0);
        assert!(rings.len() > 3, "expected concentric rings, got {}", rings.len());
        // Every ring should be a closed loop with several vertices.
        for r in &rings {
            assert!(r.len() >= 4, "ring too small: {} pts", r.len());
        }
    }

    #[test]
    fn outer_ring_is_larger_than_inner() {
        let region = MultiPolygon::new(vec![centered_rect(10.0, 10.0, 20.0, 20.0)]);
        let rings = contour_parallel(&region, 1.0, 0.0);
        let span = |p: &Polyline| -> f64 {
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for &(x, _) in p {
                lo = lo.min(x);
                hi = hi.max(x);
            }
            hi - lo
        };
        let first = span(&rings[0]);
        let last = span(rings.last().unwrap());
        assert!(first > last, "outer ring {first} should exceed inner ring {last}");
    }

    #[test]
    fn empty_region_yields_nothing() {
        let region = MultiPolygon::new(vec![]);
        let rings = contour_parallel(&region, 1.0, 0.0);
        assert!(rings.is_empty());
    }

    #[test]
    fn tiny_region_relative_to_tool_yields_nothing() {
        // A 0.5x0.5 region with a 1mm tool insets to empty immediately.
        let region = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 0.5, 0.5)]);
        let rings = contour_parallel(&region, 1.0, 0.0);
        assert!(rings.is_empty());
    }
}
