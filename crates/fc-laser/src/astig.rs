//! Z-dependent **astigmatic** beam model.
//!
//! A diode laser has two independent axes (fast / slow) that each focus to a
//! waist at a *different* Z plane (astigmatism) and broaden away from it. So the
//! spot is not only elliptical, its width — and even which axis is wider —
//! changes with focus height. Each axis follows Gaussian-beam propagation:
//!
//! ```text
//! W(z) = W0 · √( 1 + ((z − z_f) / z_R)² )
//! ```
//!
//! where `W0` is the waist (minimum) width, `z_f` the focal Z of that axis, and
//! `z_R` its Rayleigh range (the defocus over which the width grows by √2).
//! Evaluating both axes at a machine Z yields a flat [`BeamShape`] for that Z,
//! so all of the existing compensation ([`crate::offset`], [`crate::emit`], …)
//! works unchanged at any focus height.

use crate::beam::BeamShape;

/// An astigmatic elliptical beam whose per-axis widths vary with Z.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AstigmaticBeam {
    /// Full spot width along local X at its focus.
    pub waist_x: f64,
    /// Full spot width along local Y at its focus.
    pub waist_y: f64,
    /// Machine Z at which the X axis is in focus.
    pub focus_x: f64,
    /// Machine Z at which the Y axis is in focus.
    pub focus_y: f64,
    /// Rayleigh range of the X axis (> 0).
    pub rayleigh_x: f64,
    /// Rayleigh range of the Y axis (> 0).
    pub rayleigh_y: f64,
    /// Mount rotation of the ellipse (degrees, CCW).
    pub angle_deg: f64,
}

impl Default for AstigmaticBeam {
    fn default() -> Self {
        // Neutral, non-astigmatic round beam.
        AstigmaticBeam {
            waist_x: 0.1,
            waist_y: 0.1,
            focus_x: 0.0,
            focus_y: 0.0,
            rayleigh_x: 1.0,
            rayleigh_y: 1.0,
            angle_deg: 0.0,
        }
    }
}

fn axis_width(w0: f64, z: f64, zf: f64, zr: f64) -> f64 {
    let zr = zr.max(1e-9);
    w0 * (1.0 + ((z - zf) / zr).powi(2)).sqrt()
}

impl AstigmaticBeam {
    pub fn width_x_at(&self, z: f64) -> f64 {
        axis_width(self.waist_x, z, self.focus_x, self.rayleigh_x)
    }
    pub fn width_y_at(&self, z: f64) -> f64 {
        axis_width(self.waist_y, z, self.focus_y, self.rayleigh_y)
    }

    /// The elliptical [`BeamShape`] at machine Z `z`.
    pub fn at(&self, z: f64) -> BeamShape {
        BeamShape {
            width_x: self.width_x_at(z),
            width_y: self.width_y_at(z),
            angle_deg: self.angle_deg,
        }
    }

    /// The Z at which the spot is **round** (`W_x(z) == W_y(z)`), if one exists.
    ///
    /// Solving `W_x² = W_y²` is a quadratic in `z`. When several solutions exist
    /// the one giving the smaller (tighter) spot is returned. Identical axes
    /// return their common focus.
    pub fn round_spot_z(&self) -> Option<f64> {
        let (wx, wy) = (self.waist_x, self.waist_y);
        let (rx, ry) = (self.rayleigh_x.max(1e-9), self.rayleigh_y.max(1e-9));
        let big_a = wx * wx / (rx * rx);
        let big_b = wy * wy / (ry * ry);
        // (A-B)z² + (-2A·fx + 2B·fy)z + (A·fx² + wx² - B·fy² - wy²) = 0
        let qa = big_a - big_b;
        let qb = -2.0 * big_a * self.focus_x + 2.0 * big_b * self.focus_y;
        let qc = big_a * self.focus_x * self.focus_x + wx * wx
            - big_b * self.focus_y * self.focus_y
            - wy * wy;

        if qa.abs() < 1e-12 {
            if qb.abs() < 1e-12 {
                // Degenerate: identical width curves -> round everywhere.
                return if qc.abs() < 1e-9 { Some(self.focus_x) } else { None };
            }
            return Some(-qc / qb);
        }
        let disc = qb * qb - 4.0 * qa * qc;
        if disc < 0.0 {
            return None;
        }
        let s = disc.sqrt();
        let z1 = (-qb + s) / (2.0 * qa);
        let z2 = (-qb - s) / (2.0 * qa);
        // Prefer the root with the smaller resulting spot.
        let w = |z: f64| self.width_x_at(z).max(self.width_y_at(z));
        Some(if w(z1) <= w(z2) { z1 } else { z2 })
    }

    /// The Z minimising the spot area proxy `W_x(z)·W_y(z)` (the practical "best
    /// focus" for a balanced spot). Found by a coarse-then-fine search over the
    /// span around both foci.
    pub fn best_focus(&self) -> f64 {
        let lo = self.focus_x.min(self.focus_y) - 2.0 * self.rayleigh_x.max(self.rayleigh_y);
        let hi = self.focus_x.max(self.focus_y) + 2.0 * self.rayleigh_x.max(self.rayleigh_y);
        let area = |z: f64| self.width_x_at(z) * self.width_y_at(z);
        let mut best_z = lo;
        let mut best = f64::INFINITY;
        let steps = 400;
        for i in 0..=steps {
            let z = lo + (hi - lo) * (i as f64) / (steps as f64);
            let a = area(z);
            if a < best {
                best = a;
                best_z = z;
            }
        }
        best_z
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetric_beam_is_round_everywhere() {
        let b = AstigmaticBeam::default();
        for z in [-1.0, 0.0, 0.5, 2.0] {
            assert!((b.width_x_at(z) - b.width_y_at(z)).abs() < 1e-12);
            assert!(b.at(z).is_circular());
        }
        assert!(b.round_spot_z().is_some());
    }

    #[test]
    fn width_grows_sqrt2_at_one_rayleigh() {
        let b = AstigmaticBeam { waist_x: 0.06, focus_x: 0.0, rayleigh_x: 0.5, ..Default::default() };
        let w_focus = b.width_x_at(0.0);
        let w_zr = b.width_x_at(0.5); // one Rayleigh range away
        assert!((w_focus - 0.06).abs() < 1e-12);
        assert!((w_zr - 0.06 * 2.0_f64.sqrt()).abs() < 1e-9);
    }

    #[test]
    fn astigmatic_round_spot_between_foci() {
        // Equal waists/Rayleigh, foci at 0.0 and 0.4 -> round spot at the midpoint.
        let b = AstigmaticBeam {
            waist_x: 0.06,
            waist_y: 0.06,
            focus_x: 0.0,
            focus_y: 0.4,
            rayleigh_x: 0.5,
            rayleigh_y: 0.5,
            angle_deg: 0.0,
        };
        let z = b.round_spot_z().expect("round spot exists");
        assert!((z - 0.2).abs() < 1e-6, "round spot z was {z}");
        // At the X focus the Y axis is broader (out of focus).
        assert!(b.width_x_at(0.0) < b.width_y_at(0.0));
        // best_focus lands near the midpoint too (areas are symmetric).
        assert!((b.best_focus() - 0.2).abs() < 0.05);
    }

    #[test]
    fn at_returns_beamshape_widths() {
        let b = AstigmaticBeam { waist_x: 0.08, waist_y: 0.05, focus_x: 0.0, focus_y: 0.3, rayleigh_x: 0.6, rayleigh_y: 0.4, angle_deg: 10.0 };
        let s = b.at(0.0);
        assert!((s.width_x - 0.08).abs() < 1e-12);
        assert!(s.width_y > 0.05); // Y out of focus at z=0
        assert!((s.angle_deg - 10.0).abs() < 1e-12);
    }
}
