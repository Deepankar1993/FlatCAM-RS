//! Laser beam-shape model and direction-dependent compensation.
//!
//! Low-cost diode laser modules do **not** produce a circular spot: the focused
//! spot is an ellipse (often markedly elongated, e.g. 0.06 × 0.10 mm) at some
//! mount angle. For a *moving* spot this produces two distinct
//! direction-dependent effects, both of which this model quantifies:
//!
//! * **Kerf width** — the cut swath is the beam extent *perpendicular* to the
//!   travel direction. A horizontally-elongated beam cuts a **narrow** kerf on
//!   horizontal moves and a **wide** kerf on vertical moves.
//! * **Burn intensity (fluence)** — energy per unit area is proportional to the
//!   dwell time, i.e. the beam extent *parallel* to travel divided by feedrate.
//!   A horizontally-elongated beam dwells longer on **horizontal** moves and so
//!   **burns more** there. This is the effect users see as uneven darkening.
//!
//! The geometry is exact for an elliptical spot: the radius of an axis-aligned
//! ellipse (semi-axes `a`, `b`) in local direction `t` is
//! `r(t) = a·b / √((b·cos t)² + (a·sin t)²)`. A mount angle rotates the frame.

/// An elliptical laser spot. `width_x`/`width_y` are the **full** spot widths
/// along machine X/Y before rotation; `angle_deg` rotates the ellipse CCW.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeamShape {
    pub width_x: f64,
    pub width_y: f64,
    pub angle_deg: f64,
}

impl Default for BeamShape {
    fn default() -> Self {
        // A neutral 0.1 mm round spot (no anisotropy).
        BeamShape { width_x: 0.1, width_y: 0.1, angle_deg: 0.0 }
    }
}

impl BeamShape {
    /// A circular spot of the given diameter.
    pub fn circular(diameter: f64) -> Self {
        BeamShape { width_x: diameter, width_y: diameter, angle_deg: 0.0 }
    }

    /// Semi-axes `(a, b)` (half-widths).
    fn semi(&self) -> (f64, f64) {
        (self.width_x / 2.0, self.width_y / 2.0)
    }

    /// True when the spot is (near-)circular and needs no compensation.
    pub fn is_circular(&self) -> bool {
        (self.width_x - self.width_y).abs() < 1e-9
    }

    /// Radius of the spot boundary from its centre in machine direction
    /// `dir_deg` (accounts for the mount `angle_deg`).
    pub fn radius_in_dir(&self, dir_deg: f64) -> f64 {
        let (a, b) = self.semi();
        if a <= 0.0 || b <= 0.0 {
            return 0.0;
        }
        let t = (dir_deg - self.angle_deg).to_radians();
        let (c, s) = (t.cos(), t.sin());
        a * b / ((b * c).powi(2) + (a * s).powi(2)).sqrt()
    }

    /// Full cut-kerf width produced by motion at `motion_deg` (the spot extent
    /// perpendicular to travel).
    pub fn kerf_perpendicular(&self, motion_deg: f64) -> f64 {
        2.0 * self.radius_in_dir(motion_deg + 90.0)
    }

    /// Spot extent *along* travel at `motion_deg` (drives dwell/fluence).
    pub fn dwell_extent(&self, motion_deg: f64) -> f64 {
        2.0 * self.radius_in_dir(motion_deg)
    }

    /// The shortest possible dwell extent (along the spot's short axis).
    pub fn min_extent(&self) -> f64 {
        self.width_x.min(self.width_y)
    }

    /// The longest dwell extent (along the spot's long axis).
    pub fn max_extent(&self) -> f64 {
        self.width_x.max(self.width_y)
    }

    /// Power-scaling factor in `(0, 1]` to **equalise areal fluence** across
    /// travel directions.
    ///
    /// Areal energy density delivered to the cut is `H ∝ P / (v · L⊥)`, where
    /// `L⊥` is the kerf (perpendicular) extent — the larger spot *area* on the
    /// long axis cancels the longer dwell. To hold `H` constant at fixed feed,
    /// power must scale with `L⊥(θ)`; normalising so the widest-kerf direction
    /// keeps full power gives `factor(θ) = kerf_perpendicular(θ) / max_extent`.
    /// This reduces power on the directions that would otherwise over-burn (a
    /// horizontally-elongated spot gets less power on horizontal moves). The
    /// burn-vs-fluence response is non-linear, so calibrate the absolute power;
    /// this factor corrects the *relative* directionality.
    pub fn power_factor(&self, motion_deg: f64) -> f64 {
        let max_ext = self.max_extent();
        if max_ext <= 0.0 {
            return 1.0;
        }
        (self.kerf_perpendicular(motion_deg) / max_ext).clamp(0.0, 1.0)
    }
}

/// Direction of a segment `a -> b` in degrees, or `None` for a zero-length step.
pub fn segment_angle(a: (f64, f64), b: (f64, f64)) -> Option<f64> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    if dx.abs() < 1e-12 && dy.abs() < 1e-12 {
        None
    } else {
        Some(dy.atan2(dx).to_degrees())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circular_has_no_directionality() {
        let b = BeamShape::circular(0.1);
        assert!(b.is_circular());
        for d in [0.0, 30.0, 90.0, 137.0] {
            assert!((b.power_factor(d) - 1.0).abs() < 1e-9);
            assert!((b.kerf_perpendicular(d) - 0.1).abs() < 1e-9);
            assert!((b.dwell_extent(d) - 0.1).abs() < 1e-9);
        }
    }

    #[test]
    fn horizontal_elongated_beam_directionality() {
        // Long axis along X: width_x=0.2 (a=0.1), width_y=0.1 (b=0.05).
        let b = BeamShape { width_x: 0.2, width_y: 0.1, angle_deg: 0.0 };
        assert!(!b.is_circular());
        // Dwell: long along X (0.2 horizontal) vs short along Y (0.1 vertical).
        assert!((b.dwell_extent(0.0) - 0.2).abs() < 1e-9);
        assert!((b.dwell_extent(90.0) - 0.1).abs() < 1e-9);
        // Horizontal moves over-burn -> half power; vertical -> full power.
        assert!((b.power_factor(0.0) - 0.5).abs() < 1e-9);
        assert!((b.power_factor(90.0) - 1.0).abs() < 1e-9);
        // Kerf: horizontal motion cuts a narrow (0.1) swath; vertical cuts wide (0.2).
        assert!((b.kerf_perpendicular(0.0) - 0.1).abs() < 1e-9);
        assert!((b.kerf_perpendicular(90.0) - 0.2).abs() < 1e-9);
    }

    #[test]
    fn power_factor_off_axis_uses_perpendicular_kerf() {
        // Corrected (research-validated) model: factor = kerf_perp(θ)/max_extent,
        // NOT min_extent/dwell. At 45° the two formulas differ; lock the correct one.
        let b = BeamShape { width_x: 2.0, width_y: 1.0, angle_deg: 0.0 };
        let expected = b.kerf_perpendicular(45.0) / b.max_extent();
        assert!((b.power_factor(45.0) - expected).abs() < 1e-12);
        // Sanity: between the short-axis (full) and long-axis (half) values.
        assert!(b.power_factor(45.0) > 0.5 && b.power_factor(45.0) < 1.0);
    }

    #[test]
    fn mount_angle_rotates_directionality() {
        // Same ellipse rotated 90°: long axis now along Y.
        let b = BeamShape { width_x: 0.2, width_y: 0.1, angle_deg: 90.0 };
        // Now vertical moves dwell long (0.2), horizontal short (0.1).
        assert!((b.dwell_extent(90.0) - 0.2).abs() < 1e-6);
        assert!((b.dwell_extent(0.0) - 0.1).abs() < 1e-6);
        assert!((b.power_factor(90.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn segment_angle_basics() {
        assert!((segment_angle((0.0, 0.0), (1.0, 0.0)).unwrap() - 0.0).abs() < 1e-9);
        assert!((segment_angle((0.0, 0.0), (0.0, 1.0)).unwrap() - 90.0).abs() < 1e-9);
        assert!(segment_angle((1.0, 1.0), (1.0, 1.0)).is_none());
    }
}
