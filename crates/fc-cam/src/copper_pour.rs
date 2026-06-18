//! Copper pour / ground fill (port of FlatCAM's copper-pour helper).
//!
//! Fills the board outline with copper, leaving a clearance gap around the
//! existing copper features. The result is the board rectangle minus the
//! existing copper grown by `clearance` (the keep-out region), giving a solid
//! ground/power plane that nowhere comes closer than `clearance` to a trace.

use fc_geo::{centered_rect, difference, offset, MultiPolygon};

/// Generate a copper-pour region for `board` that keeps `clearance` away from
/// the existing `copper` geometry.
///
/// `board` is the bounding rectangle as `(minx, miny, maxx, maxy)`.
pub fn copper_pour(
    board: (f64, f64, f64, f64),
    copper: &MultiPolygon<f64>,
    clearance: f64,
) -> MultiPolygon<f64> {
    let (minx, miny, maxx, maxy) = board;
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let w = maxx - minx;
    let h = maxy - miny;
    let board_mp = MultiPolygon::new(vec![centered_rect(cx, cy, w, h)]);
    let keepout = offset(copper, clearance);
    difference(&board_mp, &keepout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect};

    #[test]
    fn pour_leaves_clearance_gap() {
        // 20x20 board with a centered 4x4 copper square, 1mm clearance.
        let copper = MultiPolygon::new(vec![centered_rect(10.0, 10.0, 4.0, 4.0)]);
        let pour = copper_pour((0.0, 0.0, 20.0, 20.0), &copper, 1.0);

        let board_area = 400.0;
        let grown = offset(&copper, 1.0);
        let grown_area = area(&grown);

        let pour_area = area(&pour);
        let expected = board_area - grown_area;

        assert!(pour_area > 0.0, "pour area must be positive: {pour_area}");
        assert!(pour_area < board_area, "pour must be less than full board");
        assert!(
            (pour_area - expected).abs() < 1e-6,
            "pour area {pour_area} != board - grown copper {expected}"
        );
    }

    #[test]
    fn empty_copper_fills_whole_board() {
        let copper = MultiPolygon::new(vec![]);
        let pour = copper_pour((0.0, 0.0, 10.0, 10.0), &copper, 1.0);
        assert!((area(&pour) - 100.0).abs() < 1e-6, "empty copper => full board");
    }
}
