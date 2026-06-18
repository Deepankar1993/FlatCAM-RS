//! Film / negative export geometry (port of `ToolFilm`'s core).
//!
//! Film export turns the artwork of a board layer into the geometry that gets
//! printed onto transparency film for photo-etching. Two polarities exist:
//!
//! * **Positive** — the film carries the artwork exactly as-is; the printed
//!   (opaque) regions are the copper features themselves.
//! * **Negative** — the film carries everything *except* the artwork inside a
//!   rectangular frame, so the opaque region is the board background with the
//!   features knocked out.
//!
//! A negative is therefore `frame - artwork`, where `frame` is the bounding box
//! of the artwork grown outward by `margin` on every side.

use fc_geo::{bounds, centered_rect, difference, MultiPolygon};

/// Build the *positive* film for a layer.
///
/// The positive film is the artwork unchanged, so this simply clones the input.
pub fn positive(art: &MultiPolygon<f64>) -> MultiPolygon<f64> {
    art.clone()
}

/// Build the *negative* film for a layer.
///
/// The negative is `frame - art`, where `frame` is the bounding box of `art`
/// grown by `margin` on every side and rendered as a single rectangle. An empty
/// input (no artwork, hence no bounds) yields an empty result.
pub fn negative(art: &MultiPolygon<f64>, margin: f64) -> MultiPolygon<f64> {
    let (minx, miny, maxx, maxy) = match bounds(art) {
        Some(b) => b,
        None => return MultiPolygon::new(vec![]),
    };

    // Frame rectangle: the artwork bounding box grown by `margin` on each side,
    // expressed as a centred rectangle.
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let w = (maxx - minx) + 2.0 * margin;
    let h = (maxy - miny) + 2.0 * margin;
    let frame = MultiPolygon::new(vec![centered_rect(cx, cy, w, h)]);

    difference(&frame, art)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect, MultiPolygon};

    fn square(cx: f64, cy: f64, side: f64) -> MultiPolygon<f64> {
        MultiPolygon::new(vec![centered_rect(cx, cy, side, side)])
    }

    #[test]
    fn positive_preserves_area() {
        let art = square(0.0, 0.0, 6.0);
        let pos = positive(&art);
        assert!(!pos.0.is_empty(), "positive of non-empty art is non-empty");
        assert!(
            (area(&pos) - area(&art)).abs() < 1e-9,
            "positive should preserve the artwork area"
        );
    }

    #[test]
    fn positive_empty_stays_empty() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let pos = positive(&empty);
        assert!(pos.0.is_empty(), "positive of empty art is empty");
    }

    #[test]
    fn negative_is_frame_minus_art() {
        // A 4x4 artwork square centred at the origin, framed with a 3-unit margin.
        let art = square(0.0, 0.0, 4.0);
        let margin = 3.0;
        let neg = negative(&art, margin);

        // Frame is the 4x4 bbox grown by 3 each side => 10x10 = 100.
        let frame_area = 10.0 * 10.0;
        let art_area = 4.0 * 4.0;
        let expected = frame_area - art_area;

        let got = area(&neg);
        assert!(got > 0.0, "negative area should be positive, got {got}");
        assert!(
            (got - expected).abs() < 1e-6,
            "negative area {got} should be ~ frame - art ({expected})"
        );
    }

    #[test]
    fn negative_empty_input_is_empty() {
        let empty: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let neg = negative(&empty, 2.0);
        assert!(neg.0.is_empty(), "empty art => empty negative");
        assert!((area(&neg) - 0.0).abs() < 1e-12);
    }
}
