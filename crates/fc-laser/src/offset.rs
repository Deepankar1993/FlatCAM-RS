//! Anisotropic (elliptical) Minkowski offset for laser kerf compensation.
//!
//! A round-spot offset uses a single radius, but an elliptical diode spot must
//! be grown by an *ellipse*, not a circle. Doing this directly would require an
//! anisotropic Minkowski sum, which has no convenient implementation. Instead we
//! exploit the fact that the Minkowski sum commutes with linear maps: offsetting
//! a shape by an ellipse is identical to offsetting by a unit circle in a frame
//! where the ellipse has been mapped to that unit circle.
//!
//! Concretely, for an ellipse with semi-axes `a`/`b` rotated by `angle_deg`:
//!
//! 1. rotate the geometry so the ellipse axes align with X/Y,
//! 2. scale by `1/a`, `1/b` so the ellipse becomes the unit circle,
//! 3. perform an ordinary circular offset of radius `1`,
//! 4. scale back by `a`, `b`,
//! 5. rotate back by `angle_deg`.
//!
//! The result is the exact elliptical Minkowski offset (up to the rounding the
//! underlying circular offset applies to convex corners).

use crate::beam::BeamShape;
use fc_geo::MultiPolygon;

/// Grow `geom` outward by the laser spot ellipse scaled by `k` (e.g. `k = 0.5`
/// for a half-kerf offset, `k = 1.0` for a full-spot offset).
///
/// Returns `geom` unchanged when the requested offset is non-positive. Uses a
/// plain circular offset fast path when the beam is (near-)circular.
pub fn anisotropic_offset(geom: &MultiPolygon<f64>, beam: &BeamShape, k: f64) -> MultiPolygon<f64> {
    let a = beam.width_x / 2.0 * k;
    let b = beam.width_y / 2.0 * k;
    if a <= 0.0 || b <= 0.0 {
        return geom.clone();
    }
    if beam.is_circular() {
        // a == b here, so the affine trick degenerates to a plain offset.
        return fc_geo::offset(geom, a);
    }
    // Affine trick: rotate the ellipse to the axes, scale it to the unit circle,
    // offset by 1, then undo the scale and rotation.
    let g = fc_geo::transform::rotate(geom, -beam.angle_deg, (0.0, 0.0));
    let g = fc_geo::transform::scale(&g, 1.0 / a, 1.0 / b, (0.0, 0.0));
    let g = fc_geo::offset(&g, 1.0);
    let g = fc_geo::transform::scale(&g, a, b, (0.0, 0.0));
    fc_geo::transform::rotate(&g, beam.angle_deg, (0.0, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, bounds, centered_rect, MultiPolygon};

    /// Wrap a single polygon in a `MultiPolygon`.
    fn mp(poly: fc_geo::Polygon<f64>) -> MultiPolygon<f64> {
        MultiPolygon::new(vec![poly])
    }

    #[test]
    fn circular_offset_grows_bbox_uniformly() {
        // 10x10 square centred at the origin, grown by a round beam of width 2
        // (a = b = 1) at k = 1: each side should move out by ~1 -> ~12x12.
        let sq = mp(centered_rect(0.0, 0.0, 10.0, 10.0));
        let beam = BeamShape::circular(2.0);
        let out = anisotropic_offset(&sq, &beam, 1.0);

        let (minx, miny, maxx, maxy) = bounds(&out).expect("non-empty");
        // Corner regions are rounded, but the extreme bbox grows by ~a per side.
        assert!((minx - -6.0).abs() < 0.2, "minx={minx}");
        assert!((maxx - 6.0).abs() < 0.2, "maxx={maxx}");
        assert!((miny - -6.0).abs() < 0.2, "miny={miny}");
        assert!((maxy - 6.0).abs() < 0.2, "maxy={maxy}");
        // Area must have grown.
        assert!(area(&out) > area(&sq));
    }

    #[test]
    fn elliptical_offset_grows_bbox_anisotropically() {
        // 10x10 square grown by an elliptical beam width_x=4, width_y=2
        // (a = 2, b = 1) at k = 1: bbox should grow ~2 per side in X and ~1 per
        // side in Y -> roughly 14x12.
        let sq = mp(centered_rect(0.0, 0.0, 10.0, 10.0));
        let beam = BeamShape { width_x: 4.0, width_y: 2.0, angle_deg: 0.0 };
        assert!(!beam.is_circular());
        let out = anisotropic_offset(&sq, &beam, 1.0);

        let (minx, miny, maxx, maxy) = bounds(&out).expect("non-empty");
        assert!((minx - -7.0).abs() < 0.3, "minx={minx}");
        assert!((maxx - 7.0).abs() < 0.3, "maxx={maxx}");
        assert!((miny - -6.0).abs() < 0.3, "miny={miny}");
        assert!((maxy - 6.0).abs() < 0.3, "maxy={maxy}");

        // X must grow more than Y for this horizontally-elongated beam.
        let grow_x = (maxx - minx) - 10.0;
        let grow_y = (maxy - miny) - 10.0;
        assert!(grow_x > grow_y + 1.0, "grow_x={grow_x} grow_y={grow_y}");
    }

    #[test]
    fn non_positive_offset_returns_clone() {
        let sq = mp(centered_rect(0.0, 0.0, 10.0, 10.0));
        let beam = BeamShape::circular(2.0);
        let out = anisotropic_offset(&sq, &beam, 0.0);
        let (minx, miny, maxx, maxy) = bounds(&out).expect("non-empty");
        assert!((minx - -5.0).abs() < 1e-9);
        assert!((maxx - 5.0).abs() < 1e-9);
        assert!((miny - -5.0).abs() < 1e-9);
        assert!((maxy - 5.0).abs() < 1e-9);
    }
}
