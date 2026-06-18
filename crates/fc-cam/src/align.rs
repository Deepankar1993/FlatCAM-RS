//! `align` — align objects by two reference point pairs.
//!
//! Port of FlatCAM's `ToolAlignObjects`. The user picks two reference points
//! on a *source* object (`src_a`, `src_b`) and the corresponding two points on
//! a *destination* object (`dst_a`, `dst_b`). From those two pairs we recover a
//! similarity transform (uniform scale + rotation + translation) that maps the
//! source onto the destination.
//!
//! This module is pure math: [`compute_align`] returns an [`AlignTransform`]
//! and [`apply_align`] applies it to a [`MultiPolygon`] using the
//! [`fc_geo::transform`] helpers.

use fc_geo::transform;
use fc_geo::MultiPolygon;

/// A similarity transform recovered from two reference point pairs.
///
/// Apply order (see [`apply_align`]): uniform `scale` about a pivot, then
/// `rotation_deg` about the same pivot, then a `(dx, dy)` translation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlignTransform {
    /// Translation in X applied after scale + rotation.
    pub dx: f64,
    /// Translation in Y applied after scale + rotation.
    pub dy: f64,
    /// Rotation in degrees (CCW positive).
    pub rotation_deg: f64,
    /// Uniform scale factor.
    pub scale: f64,
}

/// Compute the alignment transform mapping the source pair onto the
/// destination pair.
///
/// - `rotation_deg` = angle(`dst_b` − `dst_a`) − angle(`src_b` − `src_a`),
///   in degrees.
/// - `scale` = |`dst_b` − `dst_a`| / |`src_b` − `src_a`|.
/// - Translation: `apply_align` scales and rotates the geometry *about
///   `src_a`* (the pivot), which leaves `src_a` fixed. We then need to shift
///   `src_a` onto `dst_a`. Since the pivot is fixed by the scale/rotate step,
///   the exact translation is simply `dst_a − src_a`. This is set as
///   `(dx, dy)` and is exact when `apply_align` is called with `pivot ==
///   src_a` (the intended usage); for other pivots it is a first-order
///   approximation.
///
/// If `src_a == src_b` the source segment is degenerate; scale falls back to
/// `1.0` and rotation to `0.0`.
pub fn compute_align(
    src_a: (f64, f64),
    src_b: (f64, f64),
    dst_a: (f64, f64),
    dst_b: (f64, f64),
) -> AlignTransform {
    let src_vx = src_b.0 - src_a.0;
    let src_vy = src_b.1 - src_a.1;
    let dst_vx = dst_b.0 - dst_a.0;
    let dst_vy = dst_b.1 - dst_a.1;

    let src_len = (src_vx * src_vx + src_vy * src_vy).sqrt();
    let dst_len = (dst_vx * dst_vx + dst_vy * dst_vy).sqrt();

    let scale = if src_len > f64::EPSILON {
        dst_len / src_len
    } else {
        1.0
    };

    let rotation_deg = if src_len > f64::EPSILON && dst_len > f64::EPSILON {
        let src_ang = src_vy.atan2(src_vx);
        let dst_ang = dst_vy.atan2(dst_vx);
        (dst_ang - src_ang).to_degrees()
    } else {
        0.0
    };

    AlignTransform {
        dx: dst_a.0 - src_a.0,
        dy: dst_a.1 - src_a.1,
        rotation_deg,
        scale,
    }
}

/// Apply an [`AlignTransform`] to a multipolygon.
///
/// Order: `scale` about `pivot`, then `rotate` about `pivot`, then `translate`
/// by `(t.dx, t.dy)`. Pass `pivot == src_a` (the source reference point used
/// in [`compute_align`]) for the intended exact mapping.
pub fn apply_align(
    mp: &MultiPolygon<f64>,
    t: &AlignTransform,
    pivot: (f64, f64),
) -> MultiPolygon<f64> {
    let scaled = transform::scale(mp, t.scale, t.scale, pivot);
    let rotated = transform::rotate(&scaled, t.rotation_deg, pivot);
    transform::translate(&rotated, t.dx, t.dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::centered_rect;

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn identical_pairs_is_identity() {
        let t = compute_align((0.0, 0.0), (1.0, 0.0), (0.0, 0.0), (1.0, 0.0));
        assert!(approx(t.rotation_deg, 0.0, 1e-9), "rotation = {}", t.rotation_deg);
        assert!(approx(t.scale, 1.0, 1e-9), "scale = {}", t.scale);
        assert!(approx(t.dx, 0.0, 1e-9));
        assert!(approx(t.dy, 0.0, 1e-9));
    }

    #[test]
    fn ninety_degree_rotation_detected() {
        // src along +X, dst along +Y -> +90 degrees CCW.
        let t = compute_align((0.0, 0.0), (1.0, 0.0), (0.0, 0.0), (0.0, 1.0));
        assert!(
            approx(t.rotation_deg, 90.0, 1e-9),
            "rotation = {}",
            t.rotation_deg
        );
        assert!(approx(t.scale, 1.0, 1e-9));
    }

    #[test]
    fn scale_doubling_detected() {
        // src length 1, dst length 2 -> scale 2.
        let t = compute_align((0.0, 0.0), (1.0, 0.0), (0.0, 0.0), (2.0, 0.0));
        assert!(approx(t.scale, 2.0, 1e-9), "scale = {}", t.scale);
        assert!(approx(t.rotation_deg, 0.0, 1e-9));
    }

    #[test]
    fn translation_maps_src_a_onto_dst_a() {
        let t = compute_align((1.0, 1.0), (2.0, 1.0), (5.0, 7.0), (6.0, 7.0));
        assert!(approx(t.dx, 4.0, 1e-9), "dx = {}", t.dx);
        assert!(approx(t.dy, 6.0, 1e-9), "dy = {}", t.dy);
        assert!(approx(t.scale, 1.0, 1e-9));
        assert!(approx(t.rotation_deg, 0.0, 1e-9));
    }

    #[test]
    fn degenerate_source_is_safe() {
        let t = compute_align((3.0, 3.0), (3.0, 3.0), (0.0, 0.0), (1.0, 1.0));
        assert!(approx(t.scale, 1.0, 1e-9));
        assert!(approx(t.rotation_deg, 0.0, 1e-9));
    }

    #[test]
    fn apply_align_moves_geometry() {
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 2.0, 2.0)]);
        // Pure translation by (10, 5).
        let t = AlignTransform {
            dx: 10.0,
            dy: 5.0,
            rotation_deg: 0.0,
            scale: 1.0,
        };
        let out = apply_align(&mp, &t, (0.0, 0.0));
        let c = out.0[0].exterior().coords().next().unwrap();
        let orig = mp.0[0].exterior().coords().next().unwrap();
        assert!(approx(c.x, orig.x + 10.0, 1e-9), "x moved: {} vs {}", c.x, orig.x);
        assert!(approx(c.y, orig.y + 5.0, 1e-9), "y moved: {} vs {}", c.y, orig.y);
    }

    #[test]
    fn apply_align_scale_about_pivot() {
        let mp = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 2.0, 2.0)]);
        let t = AlignTransform {
            dx: 0.0,
            dy: 0.0,
            rotation_deg: 0.0,
            scale: 2.0,
        };
        let out = apply_align(&mp, &t, (0.0, 0.0));
        // A corner at (1,1) scaled x2 about origin lands at (2,2).
        let mut found = false;
        for c in out.0[0].exterior().coords() {
            if approx(c.x.abs(), 2.0, 1e-9) && approx(c.y.abs(), 2.0, 1e-9) {
                found = true;
            }
        }
        assert!(found, "scaled corner not found at +/-2");
    }
}
