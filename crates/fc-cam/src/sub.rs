//! Geometry boolean subtraction (port of ToolSub core).
//!
//! Subtracts one or more "tool"/clear geometries from a base geometry,
//! mirroring the behaviour of FlatCAM's ToolSub: the result is whatever
//! remains of `a` after every region in `b` (or `others`) is removed.

use fc_geo::MultiPolygon;

/// Subtract `b` from `a`, returning the remaining geometry.
pub fn subtract(a: &MultiPolygon<f64>, b: &MultiPolygon<f64>) -> MultiPolygon<f64> {
    fc_geo::difference(a, b)
}

/// Subtract every geometry in `others` from `a`, folding the difference.
///
/// Equivalent to `subtract(... subtract(subtract(a, others[0]), others[1]) ...)`.
/// With an empty `others` slice this is a clone of `a`.
pub fn subtract_all(a: &MultiPolygon<f64>, others: &[MultiPolygon<f64>]) -> MultiPolygon<f64> {
    let mut acc = a.clone();
    for other in others {
        acc = fc_geo::difference(&acc, other);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{centered_rect, MultiPolygon};

    const EPS: f64 = 1e-6;

    #[test]
    fn subtract_hole_reduces_area() {
        // 4x4 outer box, minus a centered 2x2 box => area 16 - 4 = 12.
        let a = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 4.0, 4.0)]);
        let b = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 2.0, 2.0)]);

        let result = subtract(&a, &b);
        assert!(
            (fc_geo::area(&result) - 12.0).abs() < EPS,
            "expected area 12, got {}",
            fc_geo::area(&result)
        );
    }

    #[test]
    fn subtract_all_two_holes_reduces_further() {
        // 4x4 outer box (area 16) minus two disjoint 1x1 holes => 16 - 1 - 1 = 14.
        let a = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 4.0, 4.0)]);
        let hole1 = MultiPolygon::new(vec![centered_rect(-1.0, -1.0, 1.0, 1.0)]);
        let hole2 = MultiPolygon::new(vec![centered_rect(1.0, 1.0, 1.0, 1.0)]);

        let single = subtract(&a, &hole1);
        let both = subtract_all(&a, &[hole1.clone(), hole2.clone()]);

        let area_a = fc_geo::area(&a);
        let area_single = fc_geo::area(&single);
        let area_both = fc_geo::area(&both);

        // Each subtraction strictly shrinks the area.
        assert!(area_single < area_a, "single subtraction did not shrink area");
        assert!(
            area_both < area_single,
            "subtract_all did not reduce area further"
        );
        assert!(
            (area_both - 14.0).abs() < EPS,
            "expected area 14, got {}",
            area_both
        );
    }

    #[test]
    fn subtract_all_empty_is_clone() {
        let a = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 4.0, 4.0)]);
        let result = subtract_all(&a, &[]);
        assert!((fc_geo::area(&result) - fc_geo::area(&a)).abs() < EPS);
    }
}
