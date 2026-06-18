//! Calibration fitting: recover an [`AstigmaticBeam`] from a measured per-Z
//! H/V kerf table by least squares.
//!
//! Each axis follows Gaussian-beam propagation `W(z) = W0·√(1 + ((z−z_f)/z_R)²)`.
//! Fitting that directly is non-linear, but **squaring** it gives a parabola:
//!
//! ```text
//! W(z)² = W0² + (W0/z_R)²·(z − z_f)²
//!       = c0 + c1·z + c2·z²
//! ```
//!
//! with `c2 = (W0/z_R)² = B`, `c1 = −2·B·z_f`, `c0 = W0² + B·z_f²`. So we fit a
//! plain quadratic to the points `(z_i, W_i²)` by ordinary least squares (a 3×3
//! normal-equation solve) and recover the physical parameters in closed form:
//!
//! ```text
//! z_f = −c1 / (2·c2)
//! B   = c2                        (must be > 0)
//! W0  = √(c0 − c1²/(4·c2))
//! z_R = W0 / √B
//! ```
//!
//! Degenerate inputs (fewer than 3 distinct Z, or no measurable defocus) fall
//! back to the minimum observed width at its Z with a large default Rayleigh
//! range. Outputs are always finite and positive (clamped to `1e-6`).

use crate::astig::AstigmaticBeam;

/// One calibration sample: spot extent along each axis at a focus height `z`.
///
/// Per the focus-ramp calibration, the **vertical** mark's width measures the
/// X-axis extent (`width_x`) and the **horizontal** mark's width measures the
/// Y-axis extent (`width_y`).
#[derive(Clone, Copy, Debug)]
pub struct KerfMeasurement {
    /// Focus height of this sample (machine Z).
    pub z: f64,
    /// Measured X-axis spot extent (vertical mark width).
    pub width_x: f64,
    /// Measured Y-axis spot extent (horizontal mark width).
    pub width_y: f64,
}

/// Solve a 3×3 linear system `A·x = b` by Cramer's rule. Returns `None` when
/// the matrix is (near) singular.
fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| -> f64 {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(a);
    if d.abs() < 1e-18 {
        return None;
    }
    let mut out = [0.0; 3];
    for col in 0..3 {
        let mut m = a;
        for row in 0..3 {
            m[row][col] = b[row];
        }
        out[col] = det(m) / d;
    }
    Some(out)
}

/// Least-squares quadratic fit of one axis. Given matched `(z, width)` samples,
/// returns `(waist, focus, rayleigh)` for that axis. Always finite and positive.
pub fn fit_axis_params(z: &[f64], w: &[f64]) -> (f64, f64, f64) {
    fit_axis(z, w)
}

/// Internal worker: fit `W² = c0 + c1·z + c2·z²` then recover physical params.
fn fit_axis(z: &[f64], w: &[f64]) -> (f64, f64, f64) {
    let n = z.len().min(w.len());

    // Degenerate fallback: minimum observed width at its Z, broad default Rayleigh.
    let fallback = || -> (f64, f64, f64) {
        if n == 0 {
            return (0.1_f64.max(1e-6), 0.0, 1.0_f64.max(1e-6));
        }
        let mut min_w = f64::INFINITY;
        let mut min_z = z[0];
        for i in 0..n {
            if w[i] < min_w {
                min_w = w[i];
                min_z = z[i];
            }
        }
        (min_w.max(1e-6), min_z, 1.0_f64.max(1e-6))
    };

    // Need at least 3 distinct Z to pin down a parabola.
    let mut distinct = Vec::new();
    for i in 0..n {
        if !distinct.iter().any(|&zz: &f64| (zz - z[i]).abs() < 1e-12) {
            distinct.push(z[i]);
        }
    }
    if distinct.len() < 3 {
        return fallback();
    }

    // Accumulate the moments for the 3×3 normal equations of a quadratic fit
    // against y = W².  Basis [1, z, z²]; solve for [c0, c1, c2].
    let (mut s0, mut s1, mut s2, mut s3, mut s4) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut sy, mut syz, mut syz2) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let zi = z[i];
        let y = w[i] * w[i];
        let z2 = zi * zi;
        s0 += 1.0;
        s1 += zi;
        s2 += z2;
        s3 += z2 * zi;
        s4 += z2 * z2;
        sy += y;
        syz += y * zi;
        syz2 += y * z2;
    }
    let a = [[s0, s1, s2], [s1, s2, s3], [s2, s3, s4]];
    let b = [sy, syz, syz2];

    let c = match solve3(a, b) {
        Some(c) => c,
        None => return fallback(),
    };
    let (c0, c1, c2) = (c[0], c[1], c[2]);

    // c2 = (W0/zR)² must be positive for a real upward parabola.
    if !(c2 > 1e-12) || !c2.is_finite() {
        return fallback();
    }

    let zf = -c1 / (2.0 * c2);
    let big_b = c2;
    let w0_sq = c0 - c1 * c1 / (4.0 * c2);
    if !(w0_sq > 0.0) || !w0_sq.is_finite() {
        return fallback();
    }
    let w0 = w0_sq.sqrt().max(1e-6);
    let zr = (w0 / big_b.sqrt()).max(1e-6);

    if !zf.is_finite() {
        return fallback();
    }
    (w0, zf, zr)
}

/// Least-squares fit each axis from `measurements`; `angle_deg` is the known
/// (fixed) mount rotation of the elliptical spot. Empty / degenerate input
/// yields a sane default beam rather than panicking.
pub fn fit_astig(measurements: &[KerfMeasurement], angle_deg: f64) -> AstigmaticBeam {
    let z: Vec<f64> = measurements.iter().map(|m| m.z).collect();
    let wx: Vec<f64> = measurements.iter().map(|m| m.width_x).collect();
    let wy: Vec<f64> = measurements.iter().map(|m| m.width_y).collect();

    let (waist_x, focus_x, rayleigh_x) = fit_axis(&z, &wx);
    let (waist_y, focus_y, rayleigh_y) = fit_axis(&z, &wy);

    AstigmaticBeam {
        waist_x,
        waist_y,
        focus_x,
        focus_y,
        rayleigh_x,
        rayleigh_y,
        angle_deg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sample a known beam, fit it back, and assert round-trip recovery.
    #[test]
    fn round_trip_recovers_known_beam() {
        let truth = AstigmaticBeam {
            waist_x: 0.06,
            waist_y: 0.10,
            focus_x: 0.0,
            focus_y: 0.3,
            rayleigh_x: 0.5,
            rayleigh_y: 0.4,
            angle_deg: 12.0,
        };
        // Sample widths across a focus ramp spanning both foci.
        let zs = [-0.6, -0.3, -0.1, 0.0, 0.15, 0.3, 0.45, 0.7, 1.0];
        let meas: Vec<KerfMeasurement> = zs
            .iter()
            .map(|&z| KerfMeasurement {
                z,
                width_x: truth.width_x_at(z),
                width_y: truth.width_y_at(z),
            })
            .collect();

        let fit = fit_astig(&meas, 12.0);

        assert!((fit.waist_x - truth.waist_x).abs() < 1e-3, "waist_x = {}", fit.waist_x);
        assert!((fit.waist_y - truth.waist_y).abs() < 1e-3, "waist_y = {}", fit.waist_y);
        assert!((fit.focus_x - truth.focus_x).abs() < 1e-3, "focus_x = {}", fit.focus_x);
        assert!((fit.focus_y - truth.focus_y).abs() < 1e-3, "focus_y = {}", fit.focus_y);
        assert!((fit.rayleigh_x - truth.rayleigh_x).abs() < 1e-3, "rayleigh_x = {}", fit.rayleigh_x);
        assert!((fit.rayleigh_y - truth.rayleigh_y).abs() < 1e-3, "rayleigh_y = {}", fit.rayleigh_y);
        assert!((fit.angle_deg - 12.0).abs() < 1e-12);
    }

    #[test]
    fn degenerate_flat_widths_are_finite_positive() {
        // All-same width at every Z: no measurable defocus -> fallback path.
        let meas: Vec<KerfMeasurement> = [-0.5, 0.0, 0.5, 1.0]
            .iter()
            .map(|&z| KerfMeasurement { z, width_x: 0.08, width_y: 0.08 })
            .collect();
        let fit = fit_astig(&meas, 0.0);

        for v in [
            fit.waist_x,
            fit.waist_y,
            fit.rayleigh_x,
            fit.rayleigh_y,
            fit.focus_x,
            fit.focus_y,
        ] {
            assert!(v.is_finite(), "non-finite parameter {v}");
        }
        assert!(fit.waist_x > 0.0 && fit.waist_y > 0.0);
        assert!(fit.rayleigh_x > 0.0 && fit.rayleigh_y > 0.0);
        // Recovered waist should be the (constant) observed width.
        assert!((fit.waist_x - 0.08).abs() < 1e-9);
    }

    #[test]
    fn empty_input_returns_sane_default() {
        let fit = fit_astig(&[], 5.0);
        assert!(fit.waist_x > 0.0 && fit.waist_x.is_finite());
        assert!(fit.waist_y > 0.0 && fit.waist_y.is_finite());
        assert!(fit.rayleigh_x > 0.0 && fit.rayleigh_x.is_finite());
        assert!(fit.rayleigh_y > 0.0 && fit.rayleigh_y.is_finite());
        assert!((fit.angle_deg - 5.0).abs() < 1e-12);
    }

    #[test]
    fn too_few_distinct_z_falls_back() {
        // Only two distinct Z -> cannot fit a parabola, fallback to min width.
        let meas = vec![
            KerfMeasurement { z: 0.0, width_x: 0.07, width_y: 0.09 },
            KerfMeasurement { z: 0.0, width_x: 0.07, width_y: 0.09 },
            KerfMeasurement { z: 0.5, width_x: 0.05, width_y: 0.11 },
        ];
        let fit = fit_astig(&meas, 0.0);
        assert!(fit.waist_x.is_finite() && fit.waist_x > 0.0);
        assert!((fit.waist_x - 0.05).abs() < 1e-9); // min observed X width
        assert!(fit.rayleigh_x >= 1e-6);
    }
}
