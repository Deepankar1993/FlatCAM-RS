//! Basic design-rule check (port of `ToolRulesCheck`'s core).
//!
//! PCB fabricators impose a minimum spacing between distinct copper features.
//! This module provides a lightweight "minimum clearance" rule: it flags a
//! board when two copper features sit closer together than the allowed
//! clearance.
//!
//! The trick is geometric rather than pairwise-analytical. If every copper
//! feature is grown outward by `min_clearance / 2`, then any two features whose
//! edges were closer than `min_clearance` will overlap once grown and merge
//! into a single polygon. So a drop in the polygon count after growing is proof
//! that at least one pair of features violated the clearance rule.

use fc_geo::{offset, MultiPolygon};

/// A single design-rule violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// Short machine-readable category, e.g. `"clearance"`.
    pub kind: String,
    /// Human-readable description of what was found.
    pub detail: String,
}

/// Return `true` if no two copper features are closer than `min_clearance`.
///
/// Each feature is grown outward by `min_clearance / 2` via [`fc_geo::offset`].
/// If two features were closer than `min_clearance`, their grown versions
/// overlap and merge, so the grown geometry ends up with *fewer* polygons than
/// the original. Equal-or-greater polygon counts therefore mean the rule holds.
///
/// An empty input (no copper) trivially satisfies the rule.
pub fn min_clearance_ok(copper: &MultiPolygon<f64>, min_clearance: f64) -> bool {
    if copper.0.is_empty() {
        return true;
    }
    let grown = offset(copper, min_clearance / 2.0);
    // Features merging (fewer polygons after growing) means at least one pair
    // sat closer than the minimum clearance.
    grown.0.len() >= copper.0.len()
}

/// Check the minimum-clearance rule and report any violation.
///
/// Returns an empty vector when [`min_clearance_ok`] holds, otherwise a single
/// [`Violation`] of kind `"clearance"` describing the failure.
pub fn check_clearance(copper: &MultiPolygon<f64>, min_clearance: f64) -> Vec<Violation> {
    if min_clearance_ok(copper, min_clearance) {
        return vec![];
    }
    vec![Violation {
        kind: "clearance".to_string(),
        detail: format!(
            "copper features closer than the minimum clearance of {min_clearance}"
        ),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{circle, MultiPolygon};

    fn two_circles(d: f64, r: f64) -> MultiPolygon<f64> {
        // Two circles of radius `r` whose centres are `d` apart along x.
        MultiPolygon::new(vec![
            circle(0.0, 0.0, r, 64),
            circle(d, 0.0, r, 64),
        ])
    }

    #[test]
    fn far_apart_features_pass() {
        // Centres 5 apart, radius 1 => edge-to-edge gap is 3, well over 1.0.
        let copper = two_circles(5.0, 1.0);
        assert!(min_clearance_ok(&copper, 1.0));
        assert!(check_clearance(&copper, 1.0).is_empty());
    }

    #[test]
    fn close_features_fail() {
        // Centres 2.2 apart, radius 1 => edge-to-edge gap is 0.2. Growing each
        // by min_clearance/2 = 0.5 overlaps them (0.5 + 0.5 > 0.2), merging the
        // two polygons into one.
        let copper = two_circles(2.2, 1.0);
        assert!(!min_clearance_ok(&copper, 1.0));

        let violations = check_clearance(&copper, 1.0);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].kind, "clearance");
    }

    #[test]
    fn empty_copper_passes() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        assert!(min_clearance_ok(&empty, 1.0));
        assert!(check_clearance(&empty, 1.0).is_empty());
    }
}
