//! Laser isolation routing with beam-shape compensation.
//!
//! Combines the two novel compensations into one operation:
//! * **anisotropic kerf offset** ([`crate::offset`]) so the cut clearance is
//!   correct in every direction for an elliptical spot, and
//! * **per-segment power compensation** ([`crate::compensate_power`]) so the
//!   delivered fluence is uniform regardless of travel direction.
//!
//! The result is a set of power-annotated polylines `(x, y, power_factor)` ready
//! for [`crate::laser_gcode`].

use crate::beam::BeamShape;
use fc_gcode::Polyline;
use fc_geo::MultiPolygon;

fn ring_polylines(mp: &MultiPolygon<f64>) -> Vec<Polyline> {
    let mut out = Vec::new();
    for poly in &mp.0 {
        out.push(poly.exterior().coords().map(|c| (c.x, c.y)).collect());
        for hole in poly.interiors() {
            out.push(hole.coords().map(|c| (c.x, c.y)).collect());
        }
    }
    out
}

/// Generate beam-compensated isolation tool-paths around `geom`.
///
/// `passes` isolation rings are produced; each is offset outward by the beam
/// ellipse (when `compensate_kerf`) or by an equivalent circle, stepping out by
/// `(1 - overlap)` of the kerf per pass. Every ring is then power-compensated.
/// Returns power-annotated polylines `(x, y, power_factor)`.
pub fn laser_isolation(
    geom: &MultiPolygon<f64>,
    beam: &BeamShape,
    passes: usize,
    overlap: f64,
    compensate_kerf: bool,
) -> Vec<Vec<(f64, f64, f32)>> {
    let step_k = 2.0 * (1.0 - overlap.clamp(0.0, 0.999));
    let mut out: Vec<Vec<(f64, f64, f32)>> = Vec::new();
    for i in 0..passes.max(1) {
        let k = 1.0 + (i as f64) * step_k;
        let grown = if compensate_kerf {
            crate::offset::anisotropic_offset(geom, beam, k)
        } else {
            fc_geo::offset(geom, (beam.min_extent() / 2.0) * k)
        };
        for ring in ring_polylines(&grown) {
            if ring.len() >= 2 {
                out.extend(crate::compensate_power(&[ring], beam));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::centered_rect;

    #[test]
    fn laser_isolation_produces_compensated_rings() {
        let geom = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let paths = laser_isolation(&geom, &beam, 2, 0.1, true);
        assert!(paths.len() >= 2, "two passes should yield >=2 rings");
        // Every point carries a power factor in (0, 1].
        for ring in &paths {
            for &(_, _, p) in ring {
                assert!(p > 0.0 && p <= 1.0001, "power factor out of range: {p}");
            }
        }
        // Directionality present: some points are reduced below full power.
        let any_reduced = paths.iter().flatten().any(|&(_, _, p)| p < 0.9);
        assert!(any_reduced, "an elongated beam should reduce power on some moves");
    }

    #[test]
    fn circular_beam_keeps_full_power() {
        let geom = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let beam = BeamShape::circular(0.2);
        let paths = laser_isolation(&geom, &beam, 1, 0.0, true);
        for &(_, _, p) in paths.iter().flatten() {
            assert!((p - 1.0).abs() < 1e-6, "circular beam should keep full power");
        }
    }
}
