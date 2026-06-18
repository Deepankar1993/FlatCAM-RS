//! Per-segment laser power compensation and GRBL laser G-code emission.
//!
//! An elliptical diode spot over-burns on its long-axis travel directions
//! (longer dwell). [`compensate_power`] annotates each toolpath point with a
//! power-scaling factor in `(0, 1]` derived from [`crate::beam::BeamShape`] so
//! that fluence is equalised across directions. [`laser_gcode`] then turns the
//! annotated paths into GRBL laser-mode G-code, scaling the spindle `S` word by
//! that per-point factor.

use crate::beam::{segment_angle, BeamShape};
use fc_gcode::{JobParams, Units};
use std::fmt::Write as _;

/// Annotate each path point with a power-scaling factor in `(0, 1]`.
///
/// For point `i` (with `i >= 1`) the factor uses the direction of the incoming
/// segment `(i-1 -> i)`. Point `0` uses the direction of the first segment
/// `(0 -> 1)` if the path has at least two points, otherwise `1.0`. A
/// degenerate (zero-length) segment keeps the previous point's factor.
pub fn compensate_power(
    paths: &[Vec<(f64, f64)>],
    beam: &BeamShape,
) -> Vec<Vec<(f64, f64, f32)>> {
    let mut out: Vec<Vec<(f64, f64, f32)>> = Vec::with_capacity(paths.len());

    for path in paths {
        let mut annotated: Vec<(f64, f64, f32)> = Vec::with_capacity(path.len());

        // Factor for point 0: direction of segment 0->1 if present.
        let first_factor = if path.len() >= 2 {
            match segment_angle(path[0], path[1]) {
                Some(dir) => clamp_factor(beam.power_factor(dir)),
                None => 1.0,
            }
        } else {
            1.0
        };

        let mut prev_factor = first_factor;

        for (i, &(x, y)) in path.iter().enumerate() {
            let factor = if i == 0 {
                first_factor
            } else {
                match segment_angle(path[i - 1], path[i]) {
                    Some(dir) => clamp_factor(beam.power_factor(dir)),
                    // Zero-length segment: keep previous factor.
                    None => prev_factor,
                }
            };
            prev_factor = factor;
            annotated.push((x, y, factor));
        }

        out.push(annotated);
    }

    out
}

/// Clamp a power factor into `(0, 1]`.
fn clamp_factor(f: f64) -> f32 {
    let mut v = f;
    if !v.is_finite() || v <= 0.0 {
        v = f64::MIN_POSITIVE;
    }
    if v > 1.0 {
        v = 1.0;
    }
    v as f32
}

/// Emit GRBL laser-mode G-code for the power-compensated `paths`.
///
/// `dynamic` selects GRBL's dynamic laser mode (`M4`) over constant mode
/// (`M3`). The per-point `factor` scales the maximum spindle value
/// (`params.spindle_rpm`, used here as GRBL's `$30` / `S`-max) for each `G1`.
pub fn laser_gcode(
    paths: &[Vec<(f64, f64, f32)>],
    params: &JobParams,
    dynamic: bool,
) -> String {
    let mut g = String::new();
    let smax = params.spindle_rpm;
    let feed = params.feed_xy;
    let laser_on = if dynamic { "M4" } else { "M3" };

    // Header.
    let _ = writeln!(g, "G90");
    let _ = writeln!(g, "{}", if params.units == Units::Mm { "G21" } else { "G20" });
    let _ = writeln!(g, "M5");

    for path in paths {
        if path.len() < 2 {
            continue;
        }
        let (fx, fy, _) = path[0];
        // Rapid to start with the laser off.
        let _ = writeln!(g, "G0 X{:.4} Y{:.4}", fx, fy);
        // Enter laser-on mode once for this path.
        let _ = writeln!(g, "{}", laser_on);
        for &(x, y, factor) in path.iter().skip(1) {
            let s = ((factor as f64) * smax).round();
            let _ = writeln!(g, "G1 X{:.4} Y{:.4} S{:.0} F{:.0}", x, y, s, feed);
        }
        // Laser off after the path.
        let _ = writeln!(g, "M5");
    }

    // Footer.
    let _ = writeln!(g, "M5");
    let _ = writeln!(g, "M2");

    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_segment_gets_less_power_than_vertical() {
        // Elongated along X: long-axis (horizontal) travel over-burns -> lower factor.
        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        // Path: start -> horizontal move -> vertical move.
        let paths = vec![vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]];
        let out = compensate_power(&paths, &beam);
        let p = &out[0];
        assert_eq!(p.len(), 3);

        // Point 1: incoming segment is horizontal.
        let horiz_factor = p[1].2;
        // Point 2: incoming segment is vertical.
        let vert_factor = p[2].2;

        assert!(
            horiz_factor < vert_factor,
            "horizontal {} should be < vertical {}",
            horiz_factor,
            vert_factor
        );
        // All factors strictly positive and <= 1.
        for &(_, _, f) in p {
            assert!(f > 0.0 && f <= 1.0, "factor out of (0,1]: {}", f);
        }
    }

    #[test]
    fn degenerate_segment_keeps_previous_factor() {
        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        // Repeat a point: the zero-length step must inherit the prior factor.
        let paths = vec![vec![(0.0, 0.0), (1.0, 0.0), (1.0, 0.0)]];
        let out = compensate_power(&paths, &beam);
        let p = &out[0];
        assert_eq!(p.len(), 3);
        assert_eq!(p[1].2, p[2].2);
    }

    #[test]
    fn single_point_path_is_full_power() {
        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let paths = vec![vec![(2.0, 3.0)]];
        let out = compensate_power(&paths, &beam);
        assert_eq!(out[0].len(), 1);
        assert_eq!(out[0][0].2, 1.0);
    }

    #[test]
    fn gcode_dynamic_mode_contains_m4_and_units() {
        let mut params = JobParams::default();
        params.units = Units::Mm;
        params.spindle_rpm = 1000.0;

        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let paths = compensate_power(&vec![vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]], &beam);
        let gc = laser_gcode(&paths, &params, true);

        assert!(gc.contains("G21"), "expected mm units:\n{}", gc);
        assert!(gc.contains("M4"), "expected dynamic laser mode:\n{}", gc);
        assert!(!gc.contains("M3"), "should not contain constant mode:\n{}", gc);
        assert!(gc.contains('S'), "expected at least one S value:\n{}", gc);
        assert!(gc.contains("M5"), "expected laser-off:\n{}", gc);
        assert!(gc.contains("M2"), "expected program end:\n{}", gc);
    }

    #[test]
    fn gcode_constant_mode_uses_m3() {
        let mut params = JobParams::default();
        params.units = Units::Mm;
        params.spindle_rpm = 1000.0;

        let beam = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let paths = compensate_power(&vec![vec![(0.0, 0.0), (1.0, 0.0)]], &beam);
        let gc = laser_gcode(&paths, &params, false);

        assert!(gc.contains("M3"), "expected constant laser mode:\n{}", gc);
        assert!(gc.contains("G21"), "expected mm units:\n{}", gc);
        assert!(gc.contains('S'), "expected at least one S value:\n{}", gc);
        assert!(gc.contains("M5"), "expected laser-off:\n{}", gc);
    }

    #[test]
    fn gcode_skips_short_paths_for_motion() {
        let mut params = JobParams::default();
        params.spindle_rpm = 1000.0;
        // A single-point path should produce no G1 moves.
        let paths = vec![vec![(0.0, 0.0, 1.0_f32)]];
        let gc = laser_gcode(&paths, &params, true);
        assert!(!gc.contains("G1"), "single-point path should emit no cuts:\n{}", gc);
    }
}
