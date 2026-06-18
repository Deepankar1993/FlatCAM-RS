//! Cross-hatch area fill as a complete laser operation.
//!
//! Where [`crate::cam::laser_isolation`] traces *contours*, this module *fills*
//! a region: it lays down a (cross-)hatch of straight passes
//! ([`crate::crosshatch`]), power-compensates each hatch line for the elliptical
//! spot ([`crate::compensate_power`]) so the delivered fluence is even across
//! travel directions, and emits GRBL laser G-code ([`crate::laser_gcode`]).
//!
//! Cross-hatching with two or more pass angles averages out the directional
//! burn bias of a single orientation; the per-segment power compensation
//! corrects the residual directionality of each individual line. The result is
//! a set of power-annotated polylines `(x, y, power_factor)` ready for
//! [`crate::laser_gcode`].

use crate::beam::BeamShape;
use crate::crosshatch::crosshatch_fill;
use crate::emit::{compensate_power, laser_gcode};
use fc_gcode::JobParams;
use fc_geo::MultiPolygon;

/// Smallest hatch spacing we will request; guards against zero/negative spacing
/// collapsing the scanline sweep.
const MIN_SPACING: f64 = 1e-6;

/// Generate power-compensated fill tool-paths for `region` using cross-hatch
/// passes at the given `angles` (degrees) and line `spacing`.
///
/// Each hatch line is power-compensated for the beam (long-axis moves get less
/// power). Lines with fewer than two points are skipped. Returns power-annotated
/// polylines `(x, y, power_factor)` ready for [`laser_gcode`].
pub fn laser_fill_paths(
    region: &MultiPolygon<f64>,
    beam: &BeamShape,
    spacing: f64,
    angles: &[f64],
) -> Vec<Vec<(f64, f64, f32)>> {
    let lines = crosshatch_fill(region, spacing, angles);
    let mut out: Vec<Vec<(f64, f64, f32)>> = Vec::new();
    for line in lines {
        if line.len() < 2 {
            continue;
        }
        // compensate_power takes a slice of paths and returns annotated paths.
        out.extend(compensate_power(&[line], beam));
    }
    out
}

/// Convenience: an orthogonal (0/90 relative to the beam mount angle) cross-hatch
/// fill sized to the beam, then power-compensated.
///
/// Spacing is `beam.min_extent() * (1 - overlap)` (clamped to a tiny positive),
/// and the two pass angles are `beam.angle_deg` and `beam.angle_deg + 90`.
/// `overlap` is expected in `[0, 1)`.
pub fn laser_fill_for_beam(
    region: &MultiPolygon<f64>,
    beam: &BeamShape,
    overlap: f64,
) -> Vec<Vec<(f64, f64, f32)>> {
    let overlap = overlap.clamp(0.0, 0.999);
    let spacing = (beam.min_extent() * (1.0 - overlap)).max(MIN_SPACING);
    let angles = [beam.angle_deg, beam.angle_deg + 90.0];
    laser_fill_paths(region, beam, spacing, &angles)
}

/// Full operation: produce fill paths (via [`laser_fill_paths`]) and emit laser
/// G-code with [`laser_gcode`]. `dynamic` selects GRBL dynamic mode (`M4`) over
/// constant mode (`M3`).
pub fn laser_fill_gcode(
    region: &MultiPolygon<f64>,
    beam: &BeamShape,
    spacing: f64,
    angles: &[f64],
    params: &JobParams,
    dynamic: bool,
) -> String {
    let paths = laser_fill_paths(region, beam, spacing, angles);
    laser_gcode(&paths, params, dynamic)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_gcode::Units;
    use fc_geo::centered_rect;

    /// A 20x20 square centred at the origin.
    fn square() -> MultiPolygon<f64> {
        MultiPolygon::new(vec![centered_rect(0.0, 0.0, 20.0, 20.0)])
    }

    fn empty() -> MultiPolygon<f64> {
        MultiPolygon::new(vec![])
    }

    #[test]
    fn fill_paths_are_compensated_and_directional() {
        let region = square();
        // Elongated spot: long along X, short along Y.
        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let paths = laser_fill_paths(&region, &beam, 2.0, &[0.0, 90.0]);
        assert!(!paths.is_empty(), "cross-hatch fill should yield paths");

        // Every annotated factor is in (0, 1].
        for line in &paths {
            for &(_, _, f) in line {
                assert!(f > 0.0 && f <= 1.0001, "power factor out of (0,1]: {f}");
            }
        }
        // Directionality present: an elongated beam reduces power on some moves.
        let any_reduced = paths.iter().flatten().any(|&(_, _, f)| f < 0.9);
        assert!(any_reduced, "an elongated beam should reduce power on some moves");
    }

    #[test]
    fn fill_gcode_dynamic_mode_has_moves_and_spindle() {
        let mut params = JobParams::default();
        params.units = Units::Mm;
        params.spindle_rpm = 1000.0;

        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let gc = laser_fill_gcode(&square(), &beam, 2.0, &[0.0, 90.0], &params, true);

        assert!(gc.contains("G1"), "expected cutting moves:\n{gc}");
        assert!(gc.contains('S'), "expected at least one S value:\n{gc}");
        assert!(gc.contains("M4"), "expected dynamic laser mode:\n{gc}");
        assert!(!gc.contains("M3"), "should not contain constant mode:\n{gc}");
    }

    #[test]
    fn fill_gcode_constant_mode_uses_m3() {
        let mut params = JobParams::default();
        params.units = Units::Mm;
        params.spindle_rpm = 1000.0;

        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let gc = laser_fill_gcode(&square(), &beam, 2.0, &[0.0, 90.0], &params, false);

        assert!(gc.contains("G1"), "expected cutting moves:\n{gc}");
        assert!(gc.contains('S'), "expected at least one S value:\n{gc}");
        assert!(gc.contains("M3"), "expected constant laser mode:\n{gc}");
    }

    #[test]
    fn empty_region_yields_no_paths_and_no_cuts() {
        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let paths = laser_fill_paths(&empty(), &beam, 2.0, &[0.0, 90.0]);
        assert!(paths.is_empty(), "empty region should yield no paths");

        let params = JobParams::default();
        let gc = laser_fill_gcode(&empty(), &beam, 2.0, &[0.0, 90.0], &params, true);
        assert!(!gc.contains("G1"), "empty region should emit no cuts:\n{gc}");
    }

    #[test]
    fn fill_for_beam_nonempty_on_elongated_beam() {
        let region = square();
        let beam = BeamShape { width_x: 2.0, width_y: 1.0, angle_deg: 0.0 };
        let paths = laser_fill_for_beam(&region, &beam, 0.25);
        assert!(!paths.is_empty(), "beam-sized cross-hatch fill should yield paths");
    }
}
