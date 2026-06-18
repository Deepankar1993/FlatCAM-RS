//! Additional design-rule checks (DRC) complementing [`crate::rulescheck`].
//!
//! Port of the extra checks found in FlatCAM's `ToolRulesCheck`: minimum
//! annular ring around plated holes, minimum trace (copper feature) width, and
//! minimum hole-to-board-edge clearance. Each is a small, pure predicate
//! returning whether the geometry passes the supplied rule.

use fc_geo::MultiPolygon;

/// Annular ring check: the copper ring left around a drilled hole must be at
/// least `min_ring` wide on every side.
///
/// The ring width is `(pad_dia - hole_dia) / 2.0` (radius difference).
pub fn annular_ring_ok(pad_dia: f64, hole_dia: f64, min_ring: f64) -> bool {
    (pad_dia - hole_dia) / 2.0 >= min_ring
}

/// Minimum trace width check.
///
/// Shrinks the copper geometry inward by half the minimum width. If any
/// feature is thinner than `min_width` it collapses and vanishes, so the
/// shrunken geometry contains fewer polygons than the original — a failure.
/// Empty copper trivially passes.
pub fn trace_width_ok(copper: &MultiPolygon<f64>, min_width: f64) -> bool {
    if copper.0.is_empty() {
        return true;
    }
    let shrink = fc_geo::offset(copper, -(min_width / 2.0));
    shrink.0.len() >= copper.0.len()
}

/// Minimum hole-to-board-edge clearance check.
///
/// `board` is `(minx, miny, maxx, maxy)`. The distance from the hole's edge
/// (centre minus its radius) to each of the four board edges must be at least
/// `min_clearance`.
pub fn hole_to_edge_ok(
    hole_center: (f64, f64),
    hole_dia: f64,
    board: (f64, f64, f64, f64),
    min_clearance: f64,
) -> bool {
    let (cx, cy) = hole_center;
    let (minx, miny, maxx, maxy) = board;
    let r = hole_dia / 2.0;
    let left = (cx - r) - minx;
    let right = maxx - (cx + r);
    let bottom = (cy - r) - miny;
    let top = maxy - (cy + r);
    left >= min_clearance && right >= min_clearance && bottom >= min_clearance && top >= min_clearance
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::circle;

    #[test]
    fn annular_ring_pass_and_fail() {
        // pad 2, hole 1 -> ring width 0.5
        assert!(annular_ring_ok(2.0, 1.0, 0.3));
        assert!(!annular_ring_ok(2.0, 1.0, 0.6));
    }

    #[test]
    fn annular_ring_exact_boundary() {
        assert!(annular_ring_ok(2.0, 1.0, 0.5));
    }

    #[test]
    fn trace_width_two_fat_circles_ok() {
        // two well-separated fat discs, both far wider than min_width
        let copper = MultiPolygon::new(vec![
            circle(0.0, 0.0, 2.0, 32),
            circle(10.0, 0.0, 2.0, 32),
        ]);
        assert!(trace_width_ok(&copper, 0.5));
    }

    #[test]
    fn trace_width_thin_feature_fails() {
        // a thin disc (diameter 0.2) is narrower than min_width 1.0 and vanishes
        let copper = MultiPolygon::new(vec![circle(0.0, 0.0, 0.1, 24)]);
        assert!(!trace_width_ok(&copper, 1.0));
    }

    #[test]
    fn trace_width_empty_is_ok() {
        let copper = MultiPolygon::new(vec![]);
        assert!(trace_width_ok(&copper, 1.0));
    }

    #[test]
    fn hole_near_edge_fails() {
        // board 0..100, hole dia 1 centred at (0.4, 50): edge is at -0.1 < 0
        let board = (0.0, 0.0, 100.0, 100.0);
        assert!(!hole_to_edge_ok((0.4, 50.0), 1.0, board, 0.25));
    }

    #[test]
    fn hole_with_clearance_ok() {
        let board = (0.0, 0.0, 100.0, 100.0);
        assert!(hole_to_edge_ok((50.0, 50.0), 1.0, board, 5.0));
    }
}
