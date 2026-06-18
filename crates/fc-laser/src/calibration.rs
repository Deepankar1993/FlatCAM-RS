//! Calibration-grid generators for measuring the beam model empirically.
//!
//! You cannot know a diode's spot shape, orientation, focus heights, or
//! power response from the datasheet alone — they must be measured. These
//! generators emit small laser G-code test patterns to engrave on scrap; the
//! resulting marks are measured to fit the [`crate::BeamShape`] /
//! [`crate::astig::AstigmaticBeam`] parameters and the power curve:
//!
//! * [`direction_fan`] — lines radiating at many angles at constant power. The
//!   widest/deepest direction reveals the long axis and mount angle; the
//!   width ratio gives the aspect (`width_x:width_y`).
//! * [`power_feed_grid`] — a matrix of marks at varying power (`S`) and feed.
//!   Reveals the (non-linear) power/depth curve and the lasing threshold.
//! * [`focus_ramp`] — crosses (a horizontal + a vertical mark) at stepped Z.
//!   The Z where each mark is thinnest is that axis's focus; the Z where the
//!   horizontal and vertical kerf match is the round-spot height. Together they
//!   fit the astigmatic model (`waist`, `focus`, `rayleigh` per axis).

use std::fmt::Write as _;

/// Common parameters for the calibration patterns (all lengths in mm).
#[derive(Clone, Copy, Debug)]
pub struct CalParams {
    pub feed: f64,
    pub power_max: f64,
    pub mark_len: f64,
    pub spacing: f64,
    pub travel_z: f64,
    pub dynamic: bool,
}

impl Default for CalParams {
    fn default() -> Self {
        CalParams { feed: 600.0, power_max: 1000.0, mark_len: 5.0, spacing: 3.0, travel_z: 5.0, dynamic: true }
    }
}

fn header(g: &mut String, p: &CalParams) {
    let _ = writeln!(g, "(FlatCAM-RS laser calibration)");
    let _ = writeln!(g, "G90");
    let _ = writeln!(g, "G21");
    let _ = writeln!(g, "M5");
    let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
}

fn footer(g: &mut String) {
    let _ = writeln!(g, "M5");
    let _ = writeln!(g, "M2");
}

fn mode(p: &CalParams) -> &'static str {
    if p.dynamic { "M4" } else { "M3" }
}

/// Emit one straight mark from (x0,y0) to (x1,y1) at S=`s`.
fn mark(g: &mut String, x0: f64, y0: f64, x1: f64, y1: f64, s: f64, p: &CalParams) {
    let _ = writeln!(g, "G00 X{x0:.4} Y{y0:.4}");
    let _ = writeln!(g, "{}", mode(p));
    let _ = writeln!(g, "G01 X{x1:.4} Y{y1:.4} S{:.0} F{:.0}", s, p.feed);
    let _ = writeln!(g, "M5");
}

/// Lines radiating at `n_angles` evenly-spaced angles in [0,180), each centred
/// in a row, at full power. Measure which is widest/deepest to find the long
/// axis and angle; the min/max width ratio is the spot aspect.
pub fn direction_fan(origin: (f64, f64), n_angles: usize, p: &CalParams) -> String {
    let n = n_angles.max(1);
    let mut g = String::new();
    header(&mut g, p);
    let half = p.mark_len / 2.0;
    for i in 0..n {
        let deg = 180.0 * (i as f64) / (n as f64);
        let (cx, cy) = (origin.0 + (i as f64) * p.spacing, origin.1);
        let (dx, dy) = (deg.to_radians().cos() * half, deg.to_radians().sin() * half);
        let _ = writeln!(g, "(angle {deg:.1} deg)");
        mark(&mut g, cx - dx, cy - dy, cx + dx, cy + dy, p.power_max, &p);
    }
    footer(&mut g);
    g
}

/// A matrix of horizontal marks: columns vary power (fractions of `power_max`),
/// rows vary feedrate. Reveals the power/depth curve and threshold.
pub fn power_feed_grid(origin: (f64, f64), powers: &[f64], feeds: &[f64], p: &CalParams) -> String {
    let mut g = String::new();
    header(&mut g, p);
    for (r, &feed) in feeds.iter().enumerate() {
        for (c, &pf) in powers.iter().enumerate() {
            let x0 = origin.0 + (c as f64) * (p.mark_len + p.spacing);
            let y0 = origin.1 + (r as f64) * p.spacing;
            let s = (pf.clamp(0.0, 1.0)) * p.power_max;
            let _ = writeln!(g, "(power {:.0}% feed {feed:.0})", pf * 100.0);
            let pp = CalParams { feed, ..*p };
            mark(&mut g, x0, y0, x0 + p.mark_len, y0, s, &pp);
        }
    }
    footer(&mut g);
    g
}

/// Crosses (horizontal + vertical mark) at each Z in `z_values`, full power.
/// For each Z measure the horizontal kerf (= vertical-axis spot extent) and the
/// vertical kerf (= horizontal-axis spot extent): the Z minimising each is that
/// axis's focus; where they match is the round-spot height.
pub fn focus_ramp(origin: (f64, f64), z_values: &[f64], p: &CalParams) -> String {
    let mut g = String::new();
    header(&mut g, p);
    let half = p.mark_len / 2.0;
    for (i, &z) in z_values.iter().enumerate() {
        let (cx, cy) = (origin.0 + (i as f64) * (p.mark_len + p.spacing), origin.1);
        let _ = writeln!(g, "(Z {z:.4})");
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        // move to the mark Z for cutting
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", cx - half, cy);
        let _ = writeln!(g, "G00 Z{z:.4}");
        let _ = writeln!(g, "{}", mode(p));
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} S{:.0} F{:.0}", cx + half, cy, p.power_max, p.feed);
        let _ = writeln!(g, "M5");
        // vertical mark at the same Z
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", cx, cy - half);
        let _ = writeln!(g, "{}", mode(p));
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} S{:.0} F{:.0}", cx, cy + half, p.power_max, p.feed);
        let _ = writeln!(g, "M5");
    }
    let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    footer(&mut g);
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_fan_has_marks_and_angles() {
        let g = direction_fan((0.0, 0.0), 12, &CalParams::default());
        assert!(g.contains("G21"));
        assert!(g.contains("M4")); // dynamic default
        assert!(g.contains("(angle 0.0 deg)"));
        assert!(g.contains("(angle 90.0 deg)"));
        assert_eq!(g.matches("G01").count(), 12); // one cut per angle
        assert!(g.contains("M2"));
    }

    #[test]
    fn power_grid_dimensions() {
        let g = power_feed_grid((0.0, 0.0), &[0.25, 0.5, 0.75, 1.0], &[400.0, 800.0], &CalParams::default());
        assert_eq!(g.matches("G01").count(), 8); // 4 powers x 2 feeds
        assert!(g.contains("S250")); // 25% of 1000
        assert!(g.contains("S1000"));
        assert!(g.contains("F400") && g.contains("F800"));
    }

    #[test]
    fn focus_ramp_steps_z_with_crosses() {
        let g = focus_ramp((0.0, 0.0), &[-0.2, 0.0, 0.2], &CalParams::default());
        assert!(g.contains("(Z -0.2000)") && g.contains("(Z 0.2000)"));
        assert_eq!(g.matches("G01").count(), 6); // 3 Z x (H + V)
        assert!(g.contains("G00 Z-0.2000") && g.contains("G00 Z0.2000"));
    }

    #[test]
    fn constant_mode_uses_m3() {
        let p = CalParams { dynamic: false, ..Default::default() };
        let g = direction_fan((0.0, 0.0), 3, &p);
        assert!(g.contains("M3") && !g.contains("M4"));
    }
}
