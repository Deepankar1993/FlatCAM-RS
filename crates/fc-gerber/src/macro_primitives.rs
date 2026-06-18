//! Geometry-producing Gerber aperture-macro primitives that need more than a
//! single shape: the thermal (code 7) and moiré (code 6) primitives.
//!
//! Both are always *additive* (there is no exposure flag in the Gerber spec for
//! these), so they simply return the merged geometry. Angles are in **degrees**
//! and rotation is about the primitive centre `(cx, cy)`.

use fc_geo::{centered_rect, circle, difference, transform, union, MultiPolygon};

/// Thermal relief primitive (macro code 7).
///
/// Parameters: `a[1]=cx`, `a[2]=cy`, `a[3]=outer_dia`, `a[4]=inner_dia`,
/// `a[5]=gap`, `a[6]=rotation`.
///
/// Produces the annulus between `inner_dia` and `outer_dia` with a 4-way cross
/// of width `gap` removed, then rotates the whole thing about `(cx, cy)`.
pub fn thermal(a: &[f64], steps: usize) -> MultiPolygon<f64> {
    let arg = |i: usize| a.get(i).copied().unwrap_or(0.0);
    let cx = arg(1);
    let cy = arg(2);
    let outer_dia = arg(3);
    let inner_dia = arg(4);
    let gap = arg(5);
    let rotation = arg(6);

    // Annular ring between the two diameters.
    let ring = difference(
        &MultiPolygon::new(vec![circle(cx, cy, outer_dia / 2.0, steps)]),
        &MultiPolygon::new(vec![circle(cx, cy, inner_dia / 2.0, steps)]),
    );

    // The 4-way cross of gaps: a horizontal and a vertical bar.
    let gaps = union(
        &MultiPolygon::new(vec![centered_rect(cx, cy, outer_dia, gap)]),
        &MultiPolygon::new(vec![centered_rect(cx, cy, gap, outer_dia)]),
    );

    let result = difference(&ring, &gaps);
    transform::rotate(&result, rotation, (cx, cy))
}

/// Moiré target primitive (macro code 6).
///
/// Parameters: `a[1]=cx`, `a[2]=cy`, `a[3]=outer_dia`, `a[4]=ring_thickness`,
/// `a[5]=gap`, `a[6]=max_rings` (as `usize`), `a[7]=crosshair_thickness`,
/// `a[8]=crosshair_length`, `a[9]=rotation`.
///
/// Builds up to `max_rings` concentric ring annuli plus a crosshair, then
/// rotates about `(cx, cy)`.
pub fn moire(a: &[f64], steps: usize) -> MultiPolygon<f64> {
    let arg = |i: usize| a.get(i).copied().unwrap_or(0.0);
    let cx = arg(1);
    let cy = arg(2);
    let outer_dia = arg(3);
    let ring_thickness = arg(4);
    let gap = arg(5);
    let max_rings = arg(6) as usize;
    let crosshair_thickness = arg(7);
    let crosshair_length = arg(8);
    let rotation = arg(9);

    let mut result = MultiPolygon::new(vec![]);

    let mut outer_r = outer_dia / 2.0;
    for _ in 0..max_rings {
        let inner_r = outer_r - ring_thickness;
        if inner_r <= 0.0 {
            // Last ring is a filled disc.
            let disc = MultiPolygon::new(vec![circle(cx, cy, outer_r, steps)]);
            result = union(&result, &disc);
            break;
        }
        let annulus = difference(
            &MultiPolygon::new(vec![circle(cx, cy, outer_r, steps)]),
            &MultiPolygon::new(vec![circle(cx, cy, inner_r, steps)]),
        );
        result = union(&result, &annulus);
        outer_r = inner_r - gap;
        if outer_r <= 0.0 {
            break;
        }
    }

    // Crosshair: a horizontal and a vertical bar.
    let horizontal: MultiPolygon<f64> =
        MultiPolygon::new(vec![centered_rect(cx, cy, crosshair_length, crosshair_thickness)]);
    let vertical: MultiPolygon<f64> =
        MultiPolygon::new(vec![centered_rect(cx, cy, crosshair_thickness, crosshair_length)]);
    result = union(&result, &horizontal);
    result = union(&result, &vertical);

    transform::rotate(&result, rotation, (cx, cy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::area;
    use std::f64::consts::PI;

    #[test]
    fn thermal_area_is_between_zero_and_full_ring() {
        // outer dia 4 (r=2), inner dia 2 (r=1), gap 0.5, no rotation.
        let a = [7.0, 0.0, 0.0, 4.0, 2.0, 0.5, 0.0];
        let mp = thermal(&a, 128);
        let ar = area(&mp);
        // Full annulus area = pi*(2^2 - 1^2) = 3*pi ≈ 9.42.
        let full_ring = PI * (2.0 * 2.0 - 1.0 * 1.0);
        assert!(ar > 0.0, "thermal area should be positive, was {ar}");
        assert!(
            ar < full_ring,
            "thermal area {ar} should be less than full ring {full_ring}"
        );
    }

    #[test]
    fn moire_area_positive() {
        // outer dia 10, thickness 1, gap 1, 3 rings, crosshair 0.2 x 12.
        let a = [6.0, 0.0, 0.0, 10.0, 1.0, 1.0, 3.0, 0.2, 12.0, 0.0];
        let mp = moire(&a, 128);
        assert!(area(&mp) > 0.0, "moire area should be positive");
    }

    #[test]
    fn thermal_rotation_preserves_area() {
        let base = [7.0, 0.0, 0.0, 4.0, 2.0, 0.5, 0.0];
        let rotated = [7.0, 0.0, 0.0, 4.0, 2.0, 0.5, 90.0];
        let a0 = area(&thermal(&base, 128));
        let a1 = area(&thermal(&rotated, 128));
        assert!(
            (a1 - a0).abs() / a0 < 0.01,
            "rotation should preserve area: {a0} vs {a1}"
        );
    }

    #[test]
    fn moire_rotation_preserves_area() {
        let base = [6.0, 0.0, 0.0, 10.0, 1.0, 1.0, 3.0, 0.2, 12.0, 0.0];
        let rotated = [6.0, 0.0, 0.0, 10.0, 1.0, 1.0, 3.0, 0.2, 12.0, 90.0];
        let a0 = area(&moire(&base, 128));
        let a1 = area(&moire(&rotated, 128));
        assert!(
            (a1 - a0).abs() / a0 < 0.01,
            "rotation should preserve area: {a0} vs {a1}"
        );
    }
}
