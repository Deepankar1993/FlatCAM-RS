//! Thermal relief pad geometry.
//!
//! A thermal relief pad is an annular ring of copper (the pad outline minus the
//! drill hole) connected to the surrounding plane by a number of thin spokes.
//! Equivalently it is the ring with a set of evenly-spaced radial gaps cut out
//! of it. This module builds that geometry by subtracting `spokes` thin
//! rectangles (each `gap` wide) radiating from the centre at even angles.

use fc_geo::{
    centered_rect, circle, difference, transform::rotate, union_all, MultiPolygon,
};

/// Build a thermal-relief pad centred at `(cx, cy)`.
///
/// * `pad_dia`  — outer diameter of the copper pad.
/// * `hole_dia` — diameter of the drilled / cleared hole (removed from the pad).
/// * `gap`      — width of each spoke-clearance slot cut through the ring.
/// * `spokes`   — number of evenly-spaced gaps (NOT the number of copper spokes,
///   though for a closed ring they are equal).
/// * `steps`    — number of segments used to approximate the circles.
///
/// The result is the annular ring `circle(pad/2) − circle(hole/2)` with the
/// gaps removed, leaving the copper spokes that remain between them.
pub fn thermal_relief(
    cx: f64,
    cy: f64,
    pad_dia: f64,
    hole_dia: f64,
    gap: f64,
    spokes: usize,
    steps: usize,
) -> MultiPolygon<f64> {
    let pad_r = pad_dia / 2.0;
    let hole_r = hole_dia / 2.0;
    let steps = steps.max(8);

    let outer = MultiPolygon::new(vec![circle(cx, cy, pad_r, steps)]);
    let inner = MultiPolygon::new(vec![circle(cx, cy, hole_r, steps)]);
    let ring = difference(&outer, &inner);

    if spokes == 0 || gap <= 0.0 {
        return ring;
    }

    // Build one slot rectangle: a thin bar spanning the full pad, then rotate a
    // copy for each gap and union them into the cutter.
    // Length must comfortably exceed the pad diameter so the slot fully crosses
    // the ring regardless of rotation.
    let slot_len = pad_dia * 2.0;
    let base_slot = centered_rect(cx, cy, slot_len, gap);

    let mut slot_polys = Vec::with_capacity(spokes);
    let base_mp = MultiPolygon::new(vec![base_slot]);
    for i in 0..spokes {
        let angle = 360.0 * (i as f64) / (spokes as f64);
        let rotated = rotate(&base_mp, angle, (cx, cy));
        slot_polys.extend(rotated.0);
    }
    let cutter = union_all(slot_polys);

    difference(&ring, &cutter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, circle, difference, MultiPolygon};

    fn ring_area(pad_dia: f64, hole_dia: f64, steps: usize) -> f64 {
        let outer = MultiPolygon::new(vec![circle(0.0, 0.0, pad_dia / 2.0, steps)]);
        let inner = MultiPolygon::new(vec![circle(0.0, 0.0, hole_dia / 2.0, steps)]);
        area(&difference(&outer, &inner))
    }

    #[test]
    fn thermal_area_between_zero_and_full_ring() {
        let pad_dia = 4.0;
        let hole_dia = 2.0;
        let steps = 128;
        let thermal = thermal_relief(0.0, 0.0, pad_dia, hole_dia, 0.4, 4, steps);
        let a = area(&thermal);
        let full = ring_area(pad_dia, hole_dia, steps);

        assert!(a > 0.0, "thermal pad must have positive area");
        assert!(
            a < full,
            "gaps must remove copper: {a} should be < full ring {full}"
        );
    }

    #[test]
    fn more_or_wider_gaps_remove_more_copper() {
        let pad_dia = 4.0;
        let hole_dia = 2.0;
        let steps = 128;
        let narrow = area(&thermal_relief(0.0, 0.0, pad_dia, hole_dia, 0.2, 4, steps));
        let wide = area(&thermal_relief(0.0, 0.0, pad_dia, hole_dia, 0.6, 4, steps));
        assert!(
            wide < narrow,
            "wider gaps remove more copper: {wide} should be < {narrow}"
        );
    }

    #[test]
    fn four_spokes_connected_ish_bounds() {
        // For spokes=4 the area must lie strictly between a heavily-gapped lower
        // bound and the full ungapped ring.
        let pad_dia = 4.0;
        let hole_dia = 2.0;
        let steps = 128;
        let full = ring_area(pad_dia, hole_dia, steps);
        let thermal = area(&thermal_relief(0.0, 0.0, pad_dia, hole_dia, 0.4, 4, steps));

        // Crude lower bound: even after the four gaps, more than a fifth of the
        // ring copper should survive (the spokes keep it connected-ish).
        assert!(thermal > full * 0.2, "too much copper removed: {thermal}");
        assert!(thermal < full, "some copper must be removed: {thermal}");
    }

    #[test]
    fn zero_spokes_returns_full_ring() {
        let pad_dia = 4.0;
        let hole_dia = 2.0;
        let steps = 64;
        let a = area(&thermal_relief(0.0, 0.0, pad_dia, hole_dia, 0.4, 0, steps));
        let full = ring_area(pad_dia, hole_dia, steps);
        assert!((a - full).abs() < 1e-9, "no spokes => full ring");
    }
}
