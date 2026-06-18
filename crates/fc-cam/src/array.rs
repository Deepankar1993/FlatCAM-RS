//! `array` — geometry array helpers (linear and circular duplication).
//!
//! Port of FlatCAM's array-placement features (the linear/circular array modes
//! in the editors' "Add Array" tools and the `ToolCopper`/`ToolDrilling`
//! pattern fills). Given a source [`MultiPolygon`], these helpers stamp out
//! repeated copies and merge them into a single geometry.
//!
//! - [`linear_array`] tiles copies along a fixed `(dx, dy)` step vector.
//! - [`circular_array`] arranges copies evenly around a centre point.
//!
//! Both functions are GUI-free and operate purely on [`MultiPolygon`]s.

use fc_geo::{transform, union, MultiPolygon};

/// Place `n` copies of `src`, copy `i` translated by `(i*dx, i*dy)`.
///
/// Copy `0` is the original (zero offset). All copies are merged with
/// [`fc_geo::union`] into a single [`MultiPolygon`]. `n == 0` yields an empty
/// result.
pub fn linear_array(src: &MultiPolygon<f64>, dx: f64, dy: f64, n: usize) -> MultiPolygon<f64> {
    let mut acc: MultiPolygon<f64> = MultiPolygon::new(vec![]);
    for i in 0..n {
        let copy = transform::translate(src, i as f64 * dx, i as f64 * dy);
        acc = union(&acc, &copy);
    }
    acc
}

/// Place `count` copies of `src` evenly around `(cx, cy)`.
///
/// Copy `i` is rotated by `i * 360 / count` degrees about `(cx, cy)` (Shapely
/// `affinity.rotate` in the original). Copy `0` is the unrotated original. All
/// copies are merged with [`fc_geo::union`]. `count == 0` yields an empty
/// result.
pub fn circular_array(
    src: &MultiPolygon<f64>,
    cx: f64,
    cy: f64,
    count: usize,
) -> MultiPolygon<f64> {
    let mut acc: MultiPolygon<f64> = MultiPolygon::new(vec![]);
    for i in 0..count {
        let deg = (i as f64) * 360.0 / (count as f64);
        let copy = transform::rotate(src, deg, (cx, cy));
        acc = union(&acc, &copy);
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, bounds, centered_rect};

    /// A unit square (area 1) centred at the origin.
    fn unit_square_at(cx: f64, cy: f64) -> MultiPolygon<f64> {
        MultiPolygon::new(vec![centered_rect(cx, cy, 1.0, 1.0)])
    }

    #[test]
    fn linear_array_non_overlapping_triples_area() {
        let src = unit_square_at(0.0, 0.0);
        let arr = linear_array(&src, 10.0, 0.0, 3);
        // Three well-separated unit squares => area ~3.
        assert!((area(&arr) - 3.0).abs() < 1e-6, "area was {}", area(&arr));
        // Copies span from the first square's left edge to the third's right.
        let (minx, _, maxx, _) = bounds(&arr).unwrap();
        let width = maxx - minx;
        assert!(width > 20.0, "expected wide spread, got {width}");
    }

    #[test]
    fn linear_array_zero_count_is_empty() {
        let src = unit_square_at(0.0, 0.0);
        let arr = linear_array(&src, 5.0, 5.0, 0);
        assert_eq!(arr.0.len(), 0);
    }

    #[test]
    fn circular_array_makes_count_polygons() {
        // Off-centre square so rotated copies land at distinct positions.
        let src = unit_square_at(10.0, 0.0);
        let src_area = area(&src);
        let arr = circular_array(&src, 0.0, 0.0, 4);
        // Four 90-degree-spaced copies => four disjoint polygons.
        assert_eq!(arr.0.len(), 4, "expected 4 separate copies");
        assert!(
            (area(&arr) - 4.0 * src_area).abs() < 1e-6,
            "area was {}, expected {}",
            area(&arr),
            4.0 * src_area
        );
    }

    #[test]
    fn circular_array_single_copy_is_original() {
        let src = unit_square_at(10.0, 0.0);
        let arr = circular_array(&src, 0.0, 0.0, 1);
        assert_eq!(arr.0.len(), 1);
        assert!((area(&arr) - area(&src)).abs() < 1e-9);
    }
}
