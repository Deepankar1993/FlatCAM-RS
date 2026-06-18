//! Curve-corrected per-segment laser power compensation.
//!
//! [`crate::emit::compensate_power`] equalises **areal fluence** across travel
//! directions: it cancels the longer dwell of an elliptical spot's long axis so
//! every direction receives the same energy *per unit area*. But the engraved
//! **visible depth/darkness** is a non-linear function of fluence (see
//! [`crate::powercurve`]), so equal fluence does *not* read as equal depth — the
//! long-dwell direction still looks darker after fluence equalisation.
//!
//! This module composes the directional factor with the calibrated
//! [`crate::powercurve::PowerCurve`]: it first computes the fluence-uniform
//! factor (reusing the directional logic verbatim) and then re-maps it through
//! `curve.visual_factor` so the *visible* depth — not just the fluence — is
//! equalised across directions. Coordinates are never touched; only the
//! per-point power factor changes. Every output factor is clamped into `(0, 1]`.

use crate::beam::BeamShape;
use crate::powercurve::PowerCurve;

/// Like [`crate::emit::compensate_power`], but the per-point factor is further
/// passed through [`PowerCurve::visual_factor`] so the *visible* depth — not
/// just the areal fluence — is equalised across travel directions. Equivalent
/// to `compensate_power` then [`recompensate_with_curve`].
pub fn compensate_power_curve(
    paths: &[Vec<(f64, f64)>],
    beam: &BeamShape,
    curve: &PowerCurve,
) -> Vec<Vec<(f64, f64, f32)>> {
    // Reuse the directional logic verbatim, then re-map through the curve.
    recompensate_with_curve(&crate::emit::compensate_power(paths, beam), curve)
}

/// Re-map an already power-annotated set of paths (e.g. the output of
/// [`crate::cam::laser_isolation`] or [`crate::emit::compensate_power`]) through
/// the power curve in place of the linear factor. Each `(x, y, f)` becomes
/// `(x, y, curve.visual_factor(f))` clamped into `(0, 1]`. Coordinates unchanged.
pub fn recompensate_with_curve(
    paths: &[Vec<(f64, f64, f32)>],
    curve: &PowerCurve,
) -> Vec<Vec<(f64, f64, f32)>> {
    paths
        .iter()
        .map(|path| {
            path.iter()
                .map(|&(x, y, f)| (x, y, clamp_factor(curve.visual_factor(f as f64))))
                .collect()
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::compensate_power;

    const EPS: f32 = 1e-6;

    fn elongated() -> BeamShape {
        BeamShape { width_x: 0.4, width_y: 0.2, angle_deg: 0.0 }
    }

    fn sample_path() -> Vec<Vec<(f64, f64)>> {
        vec![vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]]
    }

    #[test]
    fn identity_curve_leaves_factors_unchanged() {
        // Empty samples -> identity curve (maps power->power), so visual_factor(f) ≈ f.
        let curve = PowerCurve::from_samples(&[]);
        let beam = elongated();
        let paths = sample_path();

        let base = compensate_power(&paths, &beam);
        let corrected = compensate_power_curve(&paths, &beam, &curve);

        assert_eq!(base.len(), corrected.len());
        for (bp, cp) in base.iter().zip(&corrected) {
            assert_eq!(bp.len(), cp.len());
            for (&(bx, by, bf), &(cx, cy, cf)) in bp.iter().zip(cp) {
                // Coordinates preserved exactly.
                assert_eq!(bx, cx);
                assert_eq!(by, cy);
                // Factor essentially unchanged under the identity curve.
                assert!((bf - cf).abs() < EPS, "identity changed factor: {bf} vs {cf}");
            }
        }
    }

    #[test]
    fn convex_curve_shifts_reduced_factor() {
        // depth ∝ power^2, sampled densely so interpolation is accurate.
        let mut s = Vec::new();
        for k in 0..=20 {
            let p = k as f64 / 20.0;
            s.push((p, p * p));
        }
        let curve = PowerCurve::from_samples(&s);
        let beam = elongated();
        let paths = sample_path();

        let base = compensate_power(&paths, &beam);
        let corrected = compensate_power_curve(&paths, &beam, &curve);

        // Point 1's incoming segment is horizontal -> over-burn -> factor < 1.
        let base_h = base[0][1].2;
        let corr_h = corrected[0][1].2;
        assert!(base_h < 1.0, "expected reduced fluence factor, got {base_h}");

        // The convex curve must move the reduced factor (depth∝power^2 raises it
        // towards sqrt(f)) while keeping it in (0,1].
        assert!(
            (corr_h - base_h).abs() > 1e-3,
            "convex curve must shift factor: base={base_h} corr={corr_h}"
        );
        assert!(corr_h > 0.0 && corr_h <= 1.0, "factor out of (0,1]: {corr_h}");

        // For depth∝power^2: target = base_h, power_for_depth ≈ sqrt(base_h).
        let expected = (base_h as f64).sqrt() as f32;
        assert!(
            (corr_h - expected).abs() < 1e-2,
            "corr={corr_h} expected≈{expected}"
        );
    }

    #[test]
    fn all_factors_in_range_and_geometry_preserved() {
        let mut s = Vec::new();
        for k in 0..=20 {
            let p = k as f64 / 20.0;
            s.push((p, p * p));
        }
        let curve = PowerCurve::from_samples(&s);
        let beam = elongated();
        let paths = sample_path();

        let base = compensate_power(&paths, &beam);
        let corrected = compensate_power_curve(&paths, &beam, &curve);

        assert_eq!(corrected.len(), paths.len());
        for (orig, cp) in paths.iter().zip(&corrected) {
            // Point counts preserved.
            assert_eq!(orig.len(), cp.len());
            for (&(ox, oy), &(cx, cy, cf)) in orig.iter().zip(cp) {
                // Coordinates preserved.
                assert_eq!(ox, cx);
                assert_eq!(oy, cy);
                // Strictly in (0,1].
                assert!(cf > 0.0 && cf <= 1.0, "factor out of (0,1]: {cf}");
            }
        }
        // Sanity: base point count matches too.
        assert_eq!(base[0].len(), 3);
    }

    #[test]
    fn recompensate_matches_visual_factor_per_point() {
        // Hand-built annotated path with assorted factors.
        let mut s = Vec::new();
        for k in 0..=20 {
            let p = k as f64 / 20.0;
            s.push((p, p * p));
        }
        let curve = PowerCurve::from_samples(&s);

        let annotated: Vec<Vec<(f64, f64, f32)>> =
            vec![vec![(0.0, 0.0, 1.0_f32), (1.0, 2.0, 0.5_f32), (3.0, 4.0, 0.25_f32)]];

        let out = recompensate_with_curve(&annotated, &curve);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].len(), 3);

        for (&(ix, iy, ifac), &(ox, oy, ofac)) in annotated[0].iter().zip(&out[0]) {
            // Coordinates unchanged.
            assert_eq!(ix, ox);
            assert_eq!(iy, oy);
            // Output equals clamp_factor(visual_factor(input)).
            let expected = clamp_factor(curve.visual_factor(ifac as f64));
            assert!((ofac - expected).abs() < EPS, "ofac={ofac} expected={expected}");
            assert!(ofac > 0.0 && ofac <= 1.0, "factor out of (0,1]: {ofac}");
        }
    }
}
