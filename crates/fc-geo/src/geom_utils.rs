//! `geom_utils` — small geometry helpers built on the `geo` crate.
//!
//! These mirror common Shapely conveniences used across FlatCAM's `camlib.py`:
//!
//! * `convex_hull` — Shapely `MultiPolygon.convex_hull`, used for bounding
//!   shapes (e.g. obround/aperture-macro hulls, board outlines).
//! * `simplify` — Shapely `simplify(tolerance)`, Douglas–Peucker decimation of
//!   over-dense geometry before tool-pathing.
//! * `centroid` — Shapely `MultiPolygon.centroid`, used for label/anchor points.
//! * `contains_point` — Shapely `geom.contains(Point(x, y))`, hit-testing.

use crate::MultiPolygon;
use geo::{Centroid, Contains, ConvexHull, Point, Simplify};

/// Convex hull of a multipolygon (Shapely `MultiPolygon.convex_hull`).
pub fn convex_hull(mp: &MultiPolygon<f64>) -> crate::Polygon<f64> {
    mp.convex_hull()
}

/// Douglas–Peucker simplification of a multipolygon
/// (Shapely `MultiPolygon.simplify(eps)`). Larger `eps` removes more points.
pub fn simplify(mp: &MultiPolygon<f64>, eps: f64) -> MultiPolygon<f64> {
    mp.simplify(&eps)
}

/// Centroid of a multipolygon as `(x, y)` (Shapely `MultiPolygon.centroid`).
/// Returns `None` for an empty/degenerate (zero-area) geometry.
pub fn centroid(mp: &MultiPolygon<f64>) -> Option<(f64, f64)> {
    mp.centroid().map(|p: Point<f64>| (p.x(), p.y()))
}

/// Point-in-polygon test (Shapely `geom.contains(Point(x, y))`).
pub fn contains_point(mp: &MultiPolygon<f64>, x: f64, y: f64) -> bool {
    mp.contains(&Point::new(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{area, centered_rect, union_all, Polygon};

    /// Count the total number of coordinates across every ring of a multipolygon.
    fn point_count(mp: &MultiPolygon<f64>) -> usize {
        mp.0
            .iter()
            .map(|p| {
                p.exterior().0.len()
                    + p.interiors().iter().map(|r| r.0.len()).sum::<usize>()
            })
            .sum()
    }

    /// Build an L-shape as the union of two overlapping rectangles.
    fn l_shape() -> MultiPolygon<f64> {
        // Horizontal bar and vertical bar sharing a corner region.
        let horiz = centered_rect(2.0, 1.0, 4.0, 2.0); // x in [0,4], y in [0,2]
        let vert = centered_rect(1.0, 2.0, 2.0, 4.0); // x in [0,2], y in [0,4]
        union_all(vec![horiz, vert])
    }

    #[test]
    fn convex_hull_area_at_least_shape_area() {
        let shape = l_shape();
        let hull = convex_hull(&shape);
        let hull_mp = MultiPolygon::new(vec![hull]);
        let shape_area = area(&shape);
        let hull_area = area(&hull_mp);
        assert!(
            hull_area >= shape_area - 1e-9,
            "hull area {hull_area} should be >= shape area {shape_area}"
        );
    }

    #[test]
    fn simplify_does_not_increase_point_count() {
        // A circle approximated by many segments; simplifying should drop points.
        let dense = MultiPolygon::new(vec![crate::circle(0.0, 0.0, 1.0, 128)]);
        let before = point_count(&dense);
        let simpler = simplify(&dense, 0.05);
        let after = point_count(&simpler);
        assert!(
            after <= before,
            "simplify increased points: {before} -> {after}"
        );
        assert!(after < before, "expected some decimation: {before} -> {after}");
        // Still a valid, non-empty polygon.
        assert!(!simpler.0.is_empty());
    }

    #[test]
    fn centroid_of_centered_square_is_center() {
        let sq = MultiPolygon::new(vec![centered_rect(3.0, -2.0, 4.0, 4.0)]);
        let (cx, cy) = centroid(&sq).expect("centroid of non-empty square");
        assert!((cx - 3.0).abs() < 1e-9, "cx was {cx}");
        assert!((cy - (-2.0)).abs() < 1e-9, "cy was {cy}");
    }

    #[test]
    fn centroid_of_empty_is_none() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        assert!(centroid(&empty).is_none());
    }

    #[test]
    fn contains_point_inside_and_outside() {
        let sq: MultiPolygon<f64> =
            MultiPolygon::new(vec![centered_rect(0.0, 0.0, 2.0, 2.0)]);
        // Inside.
        assert!(contains_point(&sq, 0.0, 0.0));
        assert!(contains_point(&sq, 0.5, -0.5));
        // Outside.
        assert!(!contains_point(&sq, 5.0, 5.0));
        assert!(!contains_point(&sq, 2.0, 0.0));
    }

    #[test]
    fn convex_hull_of_square_is_the_square() {
        let sq: MultiPolygon<f64> =
            MultiPolygon::new(vec![centered_rect(0.0, 0.0, 2.0, 2.0)]);
        let hull: Polygon<f64> = convex_hull(&sq);
        let hull_mp = MultiPolygon::new(vec![hull]);
        assert!((area(&hull_mp) - 4.0).abs() < 1e-9);
    }
}
