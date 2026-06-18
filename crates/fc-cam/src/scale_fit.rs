//! `scale_fit` — scale geometry to a target size and convert between units.
//!
//! Two small, pure-geometry helpers used when fitting an object into a board
//! window or switching a project between millimetres and inches:
//!
//! - [`scale_to_fit`] uniformly scales a [`MultiPolygon`] so that it fits
//!   within a `target_w` x `target_h` box, anchored at the geometry's
//!   bounding-box minimum corner.
//! - [`convert_units`] scales by the fixed mm<->inch factor about the origin.
//!
//! Both use the [`fc_geo::transform`] helpers and never mutate their input.

use fc_geo::transform;
use fc_geo::{bounds, MultiPolygon};

/// Millimetres per inch — the exact conversion constant.
const MM_PER_INCH: f64 = 25.4;

/// Uniformly scale `mp` so its bounding box fits within `target_w` x
/// `target_h`, anchored at the bounding-box minimum corner.
///
/// The scale factor is `min(target_w / width, target_h / height)`, so the
/// result preserves aspect ratio and never exceeds either target dimension.
/// Scaling is performed about the bbox min corner, so that corner stays put.
///
/// Returns a clone of `mp` when it is empty or has zero width/height (nothing
/// meaningful to scale), or when either target dimension is non-positive.
pub fn scale_to_fit(mp: &MultiPolygon<f64>, target_w: f64, target_h: f64) -> MultiPolygon<f64> {
    let Some((minx, miny, maxx, maxy)) = bounds(mp) else {
        return mp.clone();
    };
    let width = maxx - minx;
    let height = maxy - miny;
    if width <= f64::EPSILON || height <= f64::EPSILON || target_w <= 0.0 || target_h <= 0.0 {
        return mp.clone();
    }
    let s = (target_w / width).min(target_h / height);
    transform::scale(mp, s, s, (minx, miny))
}

/// Convert `mp` between millimetres and inches by scaling about the origin.
///
/// - `from_mm_to_in == true`: divide coordinates by 25.4 (mm -> in).
/// - `from_mm_to_in == false`: multiply coordinates by 25.4 (in -> mm).
pub fn convert_units(mp: &MultiPolygon<f64>, from_mm_to_in: bool) -> MultiPolygon<f64> {
    let s = if from_mm_to_in {
        1.0 / MM_PER_INCH
    } else {
        MM_PER_INCH
    };
    transform::scale(mp, s, s, (0.0, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::centered_rect;

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    fn bbox_size(mp: &MultiPolygon<f64>) -> (f64, f64) {
        let (minx, miny, maxx, maxy) = bounds(mp).unwrap();
        (maxx - minx, maxy - miny)
    }

    #[test]
    fn scale_10x10_to_fit_5x5() {
        // 10x10 square centred at origin.
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 10.0, 10.0)]);
        let out = scale_to_fit(&mp, 5.0, 5.0);
        let (w, h) = bbox_size(&out);
        assert!(approx(w, 5.0, 1e-9), "width = {}", w);
        assert!(approx(h, 5.0, 1e-9), "height = {}", h);
    }

    #[test]
    fn scale_preserves_aspect_ratio() {
        // 10x10 into 5x20 box -> limited by width -> scale 0.5 -> 5x5.
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 10.0, 10.0)]);
        let out = scale_to_fit(&mp, 5.0, 20.0);
        let (w, h) = bbox_size(&out);
        assert!(approx(w, 5.0, 1e-9), "width = {}", w);
        assert!(approx(h, 5.0, 1e-9), "height = {}", h);
    }

    #[test]
    fn scale_anchors_min_corner() {
        // Square from (2,2) to (12,12); fit into 5x5 -> scale 0.5.
        // Min corner (2,2) must stay fixed; max corner -> (7,7).
        let mp = MultiPolygon::new(vec![centered_rect(7.0, 7.0, 10.0, 10.0)]);
        let out = scale_to_fit(&mp, 5.0, 5.0);
        let (minx, miny, maxx, maxy) = bounds(&out).unwrap();
        assert!(approx(minx, 2.0, 1e-9), "minx = {}", minx);
        assert!(approx(miny, 2.0, 1e-9), "miny = {}", miny);
        assert!(approx(maxx, 7.0, 1e-9), "maxx = {}", maxx);
        assert!(approx(maxy, 7.0, 1e-9), "maxy = {}", maxy);
    }

    #[test]
    fn empty_is_cloned() {
        let mp: MultiPolygon<f64> = MultiPolygon::new(vec![]);
        let out = scale_to_fit(&mp, 5.0, 5.0);
        assert_eq!(out.0.len(), 0);
    }

    #[test]
    fn nonpositive_target_is_cloned() {
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 10.0, 10.0)]);
        let out = scale_to_fit(&mp, 0.0, 5.0);
        let (w, h) = bbox_size(&out);
        assert!(approx(w, 10.0, 1e-9));
        assert!(approx(h, 10.0, 1e-9));
    }

    #[test]
    fn convert_mm_to_in() {
        // 25.4-wide square -> 1.0 inch wide.
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 25.4, 25.4)]);
        let out = convert_units(&mp, true);
        let (w, h) = bbox_size(&out);
        assert!(approx(w, 1.0, 1e-9), "width = {}", w);
        assert!(approx(h, 1.0, 1e-9), "height = {}", h);
    }

    #[test]
    fn convert_in_to_mm() {
        // 1.0-inch square -> 25.4 mm wide.
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 1.0, 1.0)]);
        let out = convert_units(&mp, false);
        let (w, _h) = bbox_size(&out);
        assert!(approx(w, 25.4, 1e-9), "width = {}", w);
    }

    #[test]
    fn convert_round_trip_is_identity() {
        let mp = MultiPolygon::new(vec![centered_rect(3.0, 4.0, 8.0, 6.0)]);
        let back = convert_units(&convert_units(&mp, true), false);
        let (minx, miny, maxx, maxy) = bounds(&back).unwrap();
        assert!(approx(minx, -1.0, 1e-9), "minx = {}", minx);
        assert!(approx(miny, 1.0, 1e-9), "miny = {}", miny);
        assert!(approx(maxx, 7.0, 1e-9), "maxx = {}", maxx);
        assert!(approx(maxy, 7.0, 1e-9), "maxy = {}", maxy);
    }
}
