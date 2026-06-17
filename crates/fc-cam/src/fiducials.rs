//! Fiducial / marker geometry.
//!
//! Port of the cores of FlatCAM's `ToolFiducials` / `ToolMarkers`. A fiducial is
//! a small circular copper dot used by pick-and-place machines for board
//! alignment. This module produces the dot geometry as `MultiPolygon`s built
//! from the shared `fc_geo` primitives.

use fc_geo::MultiPolygon;

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
