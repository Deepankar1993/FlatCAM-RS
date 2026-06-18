//! GUI-free plot **data** for the laser panel's anisotropy polar plot.
//!
//! The GUI shows a small polar plot so the user can *see* how the beam's kerf
//! width and power-factor vary with travel direction. This module produces the
//! sampled curve data only (pure, no egui/GUI dependency); the panel renders it.

use crate::beam::BeamShape;

/// One sampled travel direction and the beam's directional metrics there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolarSample {
    pub angle_deg: f64,    // travel direction, 0..360
    pub kerf: f64,         // kerf_perpendicular(angle)
    pub dwell: f64,        // dwell_extent(angle)
    pub power_factor: f64, // power_factor(angle), in (0,1]
}

/// Sample the beam's directional metrics at `n` evenly-spaced travel angles
/// over a full 0..360° turn (n>=1; angle i = 360*i/n). Useful for a polar plot.
pub fn polar_samples(beam: &BeamShape, n: usize) -> Vec<PolarSample> {
    let n = n.max(1);
    (0..n)
        .map(|i| {
            let angle_deg = 360.0 * (i as f64) / (n as f64);
            PolarSample {
                angle_deg,
                kerf: beam.kerf_perpendicular(angle_deg),
                dwell: beam.dwell_extent(angle_deg),
                power_factor: beam.power_factor(angle_deg),
            }
        })
        .collect()
}

/// Convert samples to (x, y) points for plotting `kerf` as a polar curve, where
/// radius = kerf and theta = angle: `(kerf*cos θ, kerf*sin θ)`. Handy for the
/// GUI to draw a closed loop.
pub fn polar_kerf_points(samples: &[PolarSample]) -> Vec<(f64, f64)> {
    samples
        .iter()
        .map(|s| {
            let t = s.angle_deg.to_radians();
            (s.kerf * t.cos(), s.kerf * t.sin())
        })
        .collect()
}

/// Likewise for the power factor as the polar radius.
pub fn polar_power_points(samples: &[PolarSample]) -> Vec<(f64, f64)> {
    samples
        .iter()
        .map(|s| {
            let t = s.angle_deg.to_radians();
            (s.power_factor * t.cos(), s.power_factor * t.sin())
        })
        .collect()
}

/// Summary stats over a full turn: `(min_kerf, max_kerf, min_power_factor,
/// max_power_factor)`. Useful to label the plot/axes. Returns all-zero for an
/// empty slice.
pub fn polar_extents(samples: &[PolarSample]) -> (f64, f64, f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mut min_k = f64::INFINITY;
    let mut max_k = f64::NEG_INFINITY;
    let mut min_p = f64::INFINITY;
    let mut max_p = f64::NEG_INFINITY;
    for s in samples {
        min_k = min_k.min(s.kerf);
        max_k = max_k.max(s.kerf);
        min_p = min_p.min(s.power_factor);
        max_p = max_p.max(s.power_factor);
    }
    (min_k, max_k, min_p, max_p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circular_beam_is_isotropic() {
        let b = BeamShape::circular(0.2);
        let samples = polar_samples(&b, 16);
        assert_eq!(samples.len(), 16);
        for s in &samples {
            assert!((s.kerf - 0.2).abs() < 1e-9);
            assert!((s.power_factor - 1.0).abs() < 1e-9);
        }
        // radius = kerf = 0.2, so every plotted point sits at distance 0.2.
        for (x, y) in polar_kerf_points(&samples) {
            assert!(((x * x + y * y).sqrt() - 0.2).abs() < 1e-9);
        }
    }

    #[test]
    fn elongated_beam_is_anisotropic() {
        let b = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let samples = polar_samples(&b, 24);
        assert_eq!(samples.len(), 24);
        let (min_k, max_k, min_p, max_p) = polar_extents(&samples);
        // Kerf varies with direction: min strictly below max.
        assert!(min_k < max_k);
        // Power factor peaks at ~1.0 with some directions reduced below 1.0.
        assert!((max_p - 1.0).abs() < 1e-9);
        assert!(min_p < 1.0);
        // Power points stay on/inside the unit circle (factor in (0,1]).
        for (x, y) in polar_power_points(&samples) {
            assert!((x * x + y * y).sqrt() <= 1.0 + 1e-9);
        }
    }

    #[test]
    fn zero_samples_treated_as_one() {
        let b = BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 };
        let samples = polar_samples(&b, 0);
        assert_eq!(samples.len(), 1);
        assert!((samples[0].angle_deg - 0.0).abs() < 1e-12);
    }

    #[test]
    fn empty_extents_are_zero() {
        assert_eq!(polar_extents(&[]), (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn sample_count_matches_n() {
        let b = BeamShape::default();
        for n in [1usize, 3, 7, 90, 360] {
            assert_eq!(polar_samples(&b, n).len(), n);
        }
    }
}
