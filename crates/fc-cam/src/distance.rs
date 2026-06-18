//! Distance measurement utilities.
//!
//! Port of FlatCAM's `ToolDistance` / `ToolObjectDistance` measurement helpers.
//! `ToolDistance` measures the straight-line distance between two clicked
//! points; `ToolObjectDistance` reports the minimum distance between two
//! selected objects. Both are reduced here to pure geometric functions over the
//! shared `fc_geo` primitives.

use fc_geo::MultiPolygon;
use geo::{Distance, Euclidean};

/// Straight-line (Euclidean) distance between two points.
///
/// Mirrors the "Measure Distance" readout of FlatCAM's `ToolDistance`.
pub fn point_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    (dx * dx + dy * dy).sqrt()
}

/// Minimum Euclidean distance between two multipolygons.
///
/// This is the core of `ToolObjectDistance`: the closest approach between any
/// part of `a` and any part of `b`. Returns `0.0` when the geometries touch or
/// overlap, and [`f64::INFINITY`] when either is empty.
pub fn geometry_distance(a: &MultiPolygon<f64>, b: &MultiPolygon<f64>) -> f64 {
    if a.0.is_empty() || b.0.is_empty() {
        return f64::INFINITY;
    }
    let mut best = f64::INFINITY;
    for pa in &a.0 {
        for pb in &b.0 {
            let d = Euclidean::distance(pa, pb);
            if d < best {
                best = d;
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_distance_3_4_5() {
        let d = point_distance((0.0, 0.0), (3.0, 4.0));
        assert!((d - 5.0).abs() < 1e-12, "expected 5, got {d}");
    }

    #[test]
    fn point_distance_is_symmetric() {
        let a = (1.5, -2.0);
        let b = (4.0, 6.5);
        assert!((point_distance(a, b) - point_distance(b, a)).abs() < 1e-12);
    }

    #[test]
    fn separated_squares_distance() {
        // Two unit squares with a 2-unit gap between their facing edges.
        let left = MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 1.0, 1.0)]);
        // left square spans x in [-0.5, 0.5]; place the right one's left edge at 2.5
        // so the gap is 2.0.
        let right = MultiPolygon::new(vec![fc_geo::centered_rect(3.0, 0.0, 1.0, 1.0)]);
        let d = geometry_distance(&left, &right);
        assert!((d - 2.0).abs() < 1e-9, "expected ~2, got {d}");
    }

    #[test]
    fn overlapping_squares_distance_zero() {
        let a = MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 2.0, 2.0)]);
        let b = MultiPolygon::new(vec![fc_geo::centered_rect(1.0, 0.0, 2.0, 2.0)]);
        let d = geometry_distance(&a, &b);
        assert!(d.abs() < 1e-12, "overlapping squares should be distance 0, got {d}");
    }

    #[test]
    fn empty_geometry_is_infinite() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let sq = MultiPolygon::new(vec![fc_geo::centered_rect(0.0, 0.0, 1.0, 1.0)]);
        assert_eq!(geometry_distance(&empty, &sq), f64::INFINITY);
        assert_eq!(geometry_distance(&sq, &empty), f64::INFINITY);
        assert_eq!(geometry_distance(&empty, &empty), f64::INFINITY);
    }
}
