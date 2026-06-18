//! `optimal` — minimum spacing between distinct copper features.
//!
//! Port of FlatCAM's `ToolOptimal`, which reports the smallest gap between any
//! two distinct copper polygons of a Gerber. This is useful for picking an
//! isolation tool diameter that fits between adjacent traces/pads.

use fc_geo::MultiPolygon;
use geo::{Distance, Euclidean};

/// Find the minimum spacing between distinct copper features.
///
/// Returns the smallest Euclidean distance over every distinct pair of
/// polygons in `copper`. The distance between two polygons is the gap between
/// their nearest boundaries (zero if they touch or overlap).
///
/// Returns `None` when there are fewer than two polygons, since spacing is
/// only defined between distinct features.
pub fn minimum_spacing(copper: &MultiPolygon<f64>) -> Option<f64> {
    let polys = &copper.0;
    if polys.len() < 2 {
        return None;
    }
    let mut best: Option<f64> = None;
    for i in 0..polys.len() {
        for j in (i + 1)..polys.len() {
            let d = Euclidean::distance(&polys[i], &polys[j]);
            best = Some(match best {
                Some(b) if b <= d => b,
                _ => d,
            });
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::circle;

    #[test]
    fn single_polygon_has_no_spacing() {
        let mp = MultiPolygon::new(vec![circle(0.0, 0.0, 1.0, 32)]);
        assert!(minimum_spacing(&mp).is_none());
    }

    #[test]
    fn empty_has_no_spacing() {
        let mp: MultiPolygon<f64> = MultiPolygon::new(Vec::new());
        assert!(minimum_spacing(&mp).is_none());
    }

    #[test]
    fn two_circles_spacing_is_gap() {
        // Two unit-radius circles, centres 5 apart => boundary gap ~= 3.
        let a = circle(0.0, 0.0, 1.0, 256);
        let b = circle(5.0, 0.0, 1.0, 256);
        let mp = MultiPolygon::new(vec![a, b]);
        let s = minimum_spacing(&mp).expect("two polygons => some spacing");
        // Polygonal approximation of the circle slightly under-estimates the
        // radius, so the gap is a touch over 3; tolerate the discretisation.
        assert!((s - 3.0).abs() < 0.05, "expected ~3.0, got {s}");
    }

    #[test]
    fn picks_the_smallest_of_several_pairs() {
        let a = circle(0.0, 0.0, 1.0, 256);
        let b = circle(5.0, 0.0, 1.0, 256); // gap ~3 from a
        let c = circle(0.0, 3.0, 1.0, 256); // gap ~1 from a (closest pair)
        let mp = MultiPolygon::new(vec![a, b, c]);
        let s = minimum_spacing(&mp).expect("spacing");
        assert!((s - 1.0).abs() < 0.05, "expected ~1.0, got {s}");
    }
}
