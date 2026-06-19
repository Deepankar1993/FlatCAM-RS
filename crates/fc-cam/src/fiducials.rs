//! Fiducial / marker geometry.
//!
//! Port of the cores of FlatCAM's `ToolFiducials` / `ToolMarkers`. A fiducial is
//! a small circular copper dot used by pick-and-place machines for board
//! alignment. This module produces the dot geometry as `MultiPolygon`s built
//! from the shared `fc_geo` primitives.

use fc_geo::{centered_rect, union_all, MultiPolygon, Polygon};

/// Shape of a fiducial mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FiducialShape {
    /// A round copper dot (the original behaviour).
    #[default]
    Circular,
    /// A + cross of two crossed bars.
    Cross,
    /// A checkerboard "chess" mark: two diagonally-opposed squares.
    Chess,
}

/// Build the geometry of a single fiducial mark of the given `shape`, sized to
/// `diameter`, centred at `(cx, cy)`.
///
/// * `Circular` — a circle of radius `diameter / 2`.
/// * `Cross` — two crossed bars spanning `diameter`, each of width
///   `diameter / 3` (line/thickness ratio matching FlatCAM's cross fiducial).
/// * `Chess` — two `diameter/2`-sized squares placed in opposite quadrants of a
///   `diameter`-sized cell, forming a 2×2 checkerboard pattern.
pub fn fiducial_shape(
    shape: FiducialShape,
    cx: f64,
    cy: f64,
    diameter: f64,
    steps: usize,
) -> MultiPolygon<f64> {
    match shape {
        FiducialShape::Circular => MultiPolygon::new(vec![fc_geo::circle(cx, cy, diameter / 2.0, steps)]),
        FiducialShape::Cross => {
            let t = diameter / 3.0;
            let h = centered_rect(cx, cy, diameter, t);
            let v = centered_rect(cx, cy, t, diameter);
            union_all(vec![h, v])
        }
        FiducialShape::Chess => {
            let q = diameter / 2.0; // square side
            // Two diagonally-opposed squares: bottom-left and top-right.
            let a = centered_rect(cx - q / 2.0, cy - q / 2.0, q, q);
            let b = centered_rect(cx + q / 2.0, cy + q / 2.0, q, q);
            union_all(vec![a, b])
        }
    }
}

/// Place fiducial marks of the given `shape` at each position, unioning them.
pub fn fiducial_marks(
    shape: FiducialShape,
    positions: &[(f64, f64)],
    diameter: f64,
    steps: usize,
) -> MultiPolygon<f64> {
    let mut polys: Vec<Polygon<f64>> = Vec::new();
    for &(cx, cy) in positions {
        polys.extend(fiducial_shape(shape, cx, cy, diameter, steps).0);
    }
    union_all(polys)
}

/// Build a set of circular fiducial dots, one centred on each position.
///
/// Each dot is a circle of radius `diameter / 2` approximated with `steps`
/// segments. The dots are unioned together into a single `MultiPolygon`;
/// well-separated dots remain distinct polygons, overlapping ones merge.
pub fn fiducial_dots(positions: &[(f64, f64)], diameter: f64, steps: usize) -> MultiPolygon<f64> {
    let r = diameter / 2.0;
    let circles = positions
        .iter()
        .map(|&(cx, cy)| fc_geo::circle(cx, cy, r, steps))
        .collect();
    fc_geo::union_all(circles)
}

/// Place fiducial dots at the four corners of `bounds`, each inset inward by
/// `margin`.
///
/// `bounds` is `(minx, miny, maxx, maxy)`. The corner dots are inset toward the
/// interior so they sit fully on the board rather than on the very edge.
pub fn corner_fiducials(
    bounds: (f64, f64, f64, f64),
    margin: f64,
    diameter: f64,
    steps: usize,
) -> MultiPolygon<f64> {
    let (minx, miny, maxx, maxy) = bounds;
    let positions = [
        (minx + margin, miny + margin),
        (maxx - margin, miny + margin),
        (maxx - margin, maxy - margin),
        (minx + margin, maxy - margin),
    ];
    fiducial_dots(&positions, diameter, steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn three_separated_dots_yield_three_polygons() {
        let positions = [(0.0, 0.0), (100.0, 0.0), (0.0, 100.0)];
        let diameter = 1.0;
        let steps = 64;
        let mp = fiducial_dots(&positions, diameter, steps);

        assert_eq!(mp.0.len(), 3, "expected one polygon per separated dot");

        let r = diameter / 2.0;
        let expected = 3.0 * PI * r * r;
        let actual = fc_geo::area(&mp);
        // Polygonal approximation of a circle slightly under-estimates area.
        assert!(
            (actual - expected).abs() < 0.01,
            "area {actual} not close to {expected}"
        );
    }

    #[test]
    fn corner_fiducials_yield_four_dots() {
        let bounds = (0.0, 0.0, 50.0, 30.0);
        let mp = corner_fiducials(bounds, 2.0, 1.0, 48);
        assert_eq!(mp.0.len(), 4, "expected four corner dots");
    }

    #[test]
    fn cross_shape_is_a_plus() {
        // A + cross of two bars of width d/3 spanning d. Its area is two bars
        // minus the overlapped centre square: 2*(d * d/3) - (d/3)^2.
        let d = 3.0;
        let mp = fiducial_shape(FiducialShape::Cross, 0.0, 0.0, d, 32);
        assert_eq!(mp.0.len(), 1, "the + is a single connected polygon");
        let t = d / 3.0;
        let expected = 2.0 * (d * t) - t * t;
        let got = fc_geo::area(&mp);
        assert!((got - expected).abs() < 1e-6, "cross area {got} != {expected}");
        // Bounds span the full diameter on both axes.
        let (minx, miny, maxx, maxy) = fc_geo::bounds(&mp).unwrap();
        assert!((maxx - minx - d).abs() < 1e-9 && (maxy - miny - d).abs() < 1e-9);
    }

    #[test]
    fn chess_shape_two_opposed_squares() {
        let d = 4.0;
        let mp = fiducial_shape(FiducialShape::Chess, 0.0, 0.0, d, 32);
        // Two diagonally-opposed squares touch only at a corner => two polygons.
        assert_eq!(mp.0.len(), 2, "chess mark is two opposed squares");
        let q = d / 2.0;
        let expected = 2.0 * q * q;
        assert!((fc_geo::area(&mp) - expected).abs() < 1e-6);
    }

    #[test]
    fn marks_at_multiple_positions() {
        let positions = [(0.0, 0.0), (100.0, 0.0)];
        let mp = fiducial_marks(FiducialShape::Cross, &positions, 2.0, 32);
        assert_eq!(mp.0.len(), 2, "one cross per separated position");
    }

    #[test]
    fn corner_dots_are_inset_within_bounds() {
        let bounds = (0.0, 0.0, 50.0, 30.0);
        let margin = 2.0;
        let diameter = 1.0;
        let mp = corner_fiducials(bounds, margin, diameter, 48);
        let (minx, miny, maxx, maxy) = fc_geo::bounds(&mp).expect("non-empty geometry");

        // Every dot centre is inset by `margin`; the dot radius is diameter/2.
        let r = diameter / 2.0;
        assert!(minx >= bounds.0 + margin - r - 1e-9);
        assert!(miny >= bounds.1 + margin - r - 1e-9);
        assert!(maxx <= bounds.2 - margin + r + 1e-9);
        assert!(maxy <= bounds.3 - margin + r + 1e-9);
    }
}
