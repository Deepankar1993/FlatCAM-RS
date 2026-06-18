//! Dogbone corner relief (port of FlatCAM's corner-relief idea).
//!
//! A round end-mill cannot reach into a sharp inside corner of a pocket: it
//! leaves uncut material with a radius equal to the tool radius. "Dogbone"
//! relief widens each corner by adding a small round cut-out so the *mating*
//! part can still seat fully into the pocket.
//!
//! This implementation takes the conservative, geometry-only approach: at every
//! vertex of every polygon's exterior ring it unions a circle of radius
//! `tool_radius` into the pocket. The circles centred on the corners enlarge
//! the pocket exactly where the cutter would otherwise leave a fillet, which is
//! sufficient to relieve the corners for a round tool.

use fc_geo::{circle, union, MultiPolygon};

/// Number of segments used to approximate each relief circle.
const CIRCLE_STEPS: usize = 32;

/// Add corner relief (dogbones) to `pocket` for a round tool of the given
/// radius.
///
/// A circle of radius `tool_radius` is placed at each vertex of each polygon's
/// exterior ring and unioned into the pocket. With `tool_radius == 0.0` the
/// circles are degenerate and the area is left effectively unchanged.
pub fn corner_relief(pocket: &MultiPolygon<f64>, tool_radius: f64) -> MultiPolygon<f64> {
    let mut result = pocket.clone();
    if tool_radius <= 0.0 {
        return result;
    }
    for poly in &pocket.0 {
        // `coords()` includes the closing vertex (== first); the duplicate just
        // unions an identical circle and is harmless.
        for c in poly.exterior().coords() {
            let disc = MultiPolygon::new(vec![circle(c.x, c.y, tool_radius, CIRCLE_STEPS)]);
            result = union(&result, &disc);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect};

    #[test]
    fn relief_increases_area_of_square() {
        let pocket = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let before = area(&pocket);
        let relieved = corner_relief(&pocket, 1.0);
        let after = area(&relieved);
        assert!(
            after > before + 1e-6,
            "relief should grow area: before {before}, after {after}"
        );
    }

    #[test]
    fn zero_radius_leaves_area_unchanged() {
        let pocket = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let before = area(&pocket);
        let relieved = corner_relief(&pocket, 0.0);
        let after = area(&relieved);
        assert!(
            (after - before).abs() < 1e-9,
            "zero radius must not change area: before {before}, after {after}"
        );
    }

    #[test]
    fn relief_adds_material_at_each_corner() {
        // Four corners, each relieved by ~ a quarter to half circle of r=1.
        // Lower bound: at least a quarter circle area per corner.
        let pocket = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let before = area(&pocket);
        let after = area(&corner_relief(&pocket, 1.0));
        let added = after - before;
        let quarter = std::f64::consts::PI * 1.0 * 1.0 / 4.0;
        assert!(
            added >= 4.0 * quarter * 0.9,
            "expected roughly 4 quarter-circles of relief, got {added}"
        );
    }
}
