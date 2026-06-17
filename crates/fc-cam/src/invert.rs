//! Gerber copper inversion (port of `ToolInvertGerber`'s core).
//!
//! "Inverting" a Gerber swaps copper for non-copper within a rectangular board
//! area: the result holds everything inside the board that the original copper
//! did *not* cover. This is the classic trick for turning a positive copper
//! layer into the negative needed for some workflows (e.g. milling away the
//! background instead of isolating the traces).
//!
//! The board rectangle is the bounding box of the input copper grown outward by
//! `margin` on every side, and the inverted geometry is simply
//! `board - copper`.

use fc_geo::{centered_rect, difference, bounds, MultiPolygon};

/// Invert copper within its bounding box grown by `margin` on each side.
///
/// Returns the non-copper area of the board: `board_rect - copper`, where
/// `board_rect` is the bounding box of `copper` expanded by `margin` in every
/// direction. An empty input (no copper, hence no bounds) yields an empty
/// result.
pub fn invert(copper: &MultiPolygon<f64>, margin: f64) -> MultiPolygon<f64> {
    let (minx, miny, maxx, maxy) = match bounds(copper) {
        Some(b) => b,
        None => return MultiPolygon::new(vec![]),
    };

    // Grow the bounding box by `margin` on every side and build it as a
    // centred rectangle, then subtract the copper to obtain the inverse.
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let w = (maxx - minx) + 2.0 * margin;
    let h = (maxy - miny) + 2.0 * margin;
    let board = MultiPolygon::new(vec![centered_rect(cx, cy, w, h)]);

    difference(&board, copper)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect, MultiPolygon};

    fn square(cx: f64, cy: f64, side: f64) -> MultiPolygon<f64> {
        MultiPolygon::new(vec![centered_rect(cx, cy, side, side)])
    }

    #[test]
    fn invert_swaps_copper_for_background() {
        // A 10x10 copper square centred at the origin.
        let copper = square(0.0, 0.0, 10.0);
        let margin = 5.0;
        let inv = invert(&copper, margin);

        // Board is the 10x10 bbox grown by 5 each side => 20x20 = 400.
        let board_area = 20.0 * 20.0;
        let copper_area = 10.0 * 10.0;
        let expected = board_area - copper_area;

        let got = area(&inv);
        assert!(got > 0.0, "inverted area should be positive, got {got}");
        assert!(
            (got - expected).abs() < 1e-6,
            "inverted area {got} should be ~ board - copper ({expected})"
        );
    }

    #[test]
    fn invert_empty_input_is_empty() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let inv = invert(&empty, 2.0);
        assert!(inv.0.is_empty(), "empty copper should invert to empty");
        assert!((area(&inv) - 0.0).abs() < 1e-12);
    }

    #[test]
    fn invert_with_zero_margin_covers_only_bbox() {
        // With no margin the board equals the copper bbox, so the inverse of a
        // single solid square filling its own bbox has (near) zero area.
        let copper = square(3.0, 3.0, 4.0);
        let inv = invert(&copper, 0.0);
        assert!(
            area(&inv) < 1e-6,
            "zero-margin inverse of a full-bbox square should be ~empty"
        );
    }
}
