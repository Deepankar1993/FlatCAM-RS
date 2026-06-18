//! Non-linear power→depth response and visually-uniform power correction.
//!
//! A diode laser's engraved **depth/darkness** is *not* linear in drive power
//! (or in areal fluence): doubling the power does not double the visible depth.
//! The directional [`crate::beam::BeamShape::power_factor`] equalises *fluence*
//! across travel directions, but because depth is a non-linear function of
//! fluence, equal fluence does **not** yield equal visible depth. The marks on
//! the long-dwell direction still read darker even after fluence equalisation.
//!
//! This module captures the measured response (from the `power` calibration
//! grid: a matrix of marks at varying drive `S` / feed, whose depth/darkness is
//! measured) into a monotone lookup table (LUT) and uses it to convert a
//! fluence-uniform factor into a *visually*-uniform one.
//!
//! Approach: sort the `(power, depth)` samples by power, average duplicate
//! powers, then enforce a monotonically non-decreasing depth with a
//! pool-adjacent-violators (PAVA) isotonic regression. The resulting curve is
//! invertible, so [`PowerCurve::depth_at`] and [`PowerCurve::power_for_depth`]
//! are well-defined linear interpolations. To make the visible result track the
//! linear (fluence) intent, a directional factor `f` is interpreted as a target
//! of `f · depth_at(1.0)` and inverted through the curve — see
//! [`PowerCurve::visual_factor`].

/// A monotone power→depth response curve fitted from measured samples.
///
/// `power` values are the relative drive in `[0, 1]` (fraction of S-max, or
/// normalised fluence); `depth` is the measured engraving depth/darkness (any
/// consistent positive unit). Stored sorted by power, with depth made
/// monotonically non-decreasing (isotonic), so the curve is invertible.
#[derive(Clone, Debug)]
pub struct PowerCurve {
    /// Knot powers, sorted ascending.
    power: Vec<f64>,
    /// Knot depths, monotonically non-decreasing, aligned with `power`.
    depth: Vec<f64>,
}

/// Clamp a value to a finite number, mapping NaN/Inf to a safe finite result.
fn finite(x: f64, fallback: f64) -> f64 {
    if x.is_finite() {
        x
    } else {
        fallback
    }
}

impl PowerCurve {
    /// Build from `(power, depth)` samples. Sorts by power, deduplicates equal
    /// powers (averaging their depths), then enforces monotonic non-decreasing
    /// depth via a pool-adjacent-violators (PAVA) isotonic pass. Empty or
    /// single-point input yields an identity-ish curve that maps power→power.
    pub fn from_samples(samples: &[(f64, f64)]) -> Self {
        // Collect finite samples only.
        let mut pts: Vec<(f64, f64)> = samples
            .iter()
            .filter(|(p, d)| p.is_finite() && d.is_finite())
            .map(|&(p, d)| (p, d))
            .collect();

        // Identity fallback: with <2 usable points the response is unknown, so
        // map power→power (a linear curve over [0,1]).
        if pts.len() < 2 {
            return PowerCurve { power: vec![0.0, 1.0], depth: vec![0.0, 1.0] };
        }

        // Sort by power (NaN already filtered, so total order is safe).
        pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // Deduplicate equal powers, averaging their depths.
        let mut powers: Vec<f64> = Vec::with_capacity(pts.len());
        let mut depths: Vec<f64> = Vec::with_capacity(pts.len());
        let mut i = 0;
        while i < pts.len() {
            let p = pts[i].0;
            let mut sum = 0.0;
            let mut n = 0.0;
            while i < pts.len() && (pts[i].0 - p).abs() < 1e-12 {
                sum += pts[i].1;
                n += 1.0;
                i += 1;
            }
            powers.push(p);
            depths.push(sum / n);
        }

        // Degenerate to identity if dedup collapsed everything to one knot.
        if powers.len() < 2 {
            return PowerCurve { power: vec![0.0, 1.0], depth: vec![0.0, 1.0] };
        }

        // Pool-adjacent-violators (PAVA) isotonic regression with unit weights.
        // Each block tracks (sum, weight); merge while the previous block's
        // mean exceeds the current one.
        let mut block_mean: Vec<f64> = Vec::with_capacity(depths.len());
        let mut block_w: Vec<f64> = Vec::with_capacity(depths.len());
        for &d in &depths {
            block_mean.push(d);
            block_w.push(1.0);
            while block_mean.len() >= 2 {
                let n = block_mean.len();
                if block_mean[n - 2] <= block_mean[n - 1] {
                    break;
                }
                // Merge the last two blocks into a weighted mean.
                let w = block_w[n - 2] + block_w[n - 1];
                let m = (block_mean[n - 2] * block_w[n - 2]
                    + block_mean[n - 1] * block_w[n - 1])
                    / w;
                block_mean.truncate(n - 2);
                block_w.truncate(n - 2);
                block_mean.push(m);
                block_w.push(w);
            }
        }

        // Expand block means back to per-knot monotone depths.
        let mut mono: Vec<f64> = Vec::with_capacity(depths.len());
        for (bi, &m) in block_mean.iter().enumerate() {
            let count = block_w[bi] as usize;
            for _ in 0..count {
                mono.push(m);
            }
        }
        // Guard against any rounding drift in the expansion count.
        while mono.len() < depths.len() {
            mono.push(*mono.last().unwrap());
        }
        mono.truncate(depths.len());

        PowerCurve { power: powers, depth: mono }
    }

    /// Interpolated depth at a given power in `[0, 1]` (clamped to the data
    /// range, linear interpolation between knots).
    pub fn depth_at(&self, power: f64) -> f64 {
        let p = finite(power, 0.0).clamp(0.0, 1.0);
        let lo = self.power[0];
        let hi = *self.power.last().unwrap();
        // Clamp query to the knot range.
        let p = p.clamp(lo, hi);
        if p <= lo {
            return finite(self.depth[0], 0.0);
        }
        if p >= hi {
            return finite(*self.depth.last().unwrap(), 0.0);
        }
        // Find the bracketing interval [power[i], power[i+1]].
        for i in 0..self.power.len() - 1 {
            let (p0, p1) = (self.power[i], self.power[i + 1]);
            if p >= p0 && p <= p1 {
                let span = p1 - p0;
                let frac = if span.abs() < 1e-15 { 0.0 } else { (p - p0) / span };
                let d = self.depth[i] + frac * (self.depth[i + 1] - self.depth[i]);
                return finite(d, self.depth[i]);
            }
        }
        finite(*self.depth.last().unwrap(), 0.0)
    }

    /// Inverse: the power in `[0, 1]` that yields the given depth (clamped,
    /// linear interpolation). Well-defined because depth is monotone.
    pub fn power_for_depth(&self, depth: f64) -> f64 {
        let d = finite(depth, 0.0);
        let d0 = self.depth[0];
        let dn = *self.depth.last().unwrap();
        // Clamp target depth to the achievable range.
        let d = d.clamp(d0, dn);
        if d <= d0 {
            return finite(self.power[0], 0.0).clamp(0.0, 1.0);
        }
        if d >= dn {
            return finite(*self.power.last().unwrap(), 0.0).clamp(0.0, 1.0);
        }
        // Walk segments; depth is non-decreasing so the first bracketing
        // interval gives the inverse. Skip flat segments (no inverse there).
        for i in 0..self.depth.len() - 1 {
            let (d0i, d1i) = (self.depth[i], self.depth[i + 1]);
            if d >= d0i && d <= d1i {
                let span = d1i - d0i;
                if span.abs() < 1e-15 {
                    // Flat block: return the lower power of the block.
                    return finite(self.power[i], 0.0).clamp(0.0, 1.0);
                }
                let frac = (d - d0i) / span;
                let p = self.power[i] + frac * (self.power[i + 1] - self.power[i]);
                return finite(p, self.power[i]).clamp(0.0, 1.0);
            }
        }
        finite(*self.power.last().unwrap(), 0.0).clamp(0.0, 1.0)
    }

    /// Map a *fluence-uniform* power factor in `(0, 1]` to a *visually-uniform*
    /// one.
    ///
    /// At full power `1.0` we hit `depth_full = depth_at(1.0)`. A directional
    /// factor `f` (from [`crate::beam::BeamShape::power_factor`]) would naively
    /// set `power = f`, giving `depth_at(f)` which — being non-linear — is *not*
    /// a proportional reduction. To make the *visible* result track the linear
    /// (fluence) intent, treat the target as a fraction `f` of the full-power
    /// depth: `target_depth = f · depth_full`, and return
    /// `power_for_depth(target_depth)`. Returns a value in `(0, 1]`. For an
    /// identity/linear curve this returns approximately `f` unchanged.
    pub fn visual_factor(&self, fluence_factor: f64) -> f64 {
        let f = finite(fluence_factor, 1.0).clamp(0.0, 1.0);
        let depth_full = self.depth_at(1.0);
        if depth_full <= 0.0 {
            // No measurable response span; fall back to the linear intent.
            return f.max(1e-6);
        }
        let target = f * depth_full;
        let p = self.power_for_depth(target);
        // Never return non-positive (would silence the laser entirely).
        p.clamp(1e-6, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-6;

    #[test]
    fn monotonicity_enforced() {
        // Non-monotone input: the (0.6, 0.4) dip violates monotonicity.
        let c = PowerCurve::from_samples(&[
            (0.0, 0.0),
            (0.3, 0.5),
            (0.6, 0.4),
            (1.0, 1.0),
        ]);
        let mut prev = f64::NEG_INFINITY;
        for k in 0..=100 {
            let p = k as f64 / 100.0;
            let d = c.depth_at(p);
            assert!(d.is_finite());
            assert!(d >= prev - 1e-9, "depth must be non-decreasing at p={p}");
            prev = d;
        }
    }

    #[test]
    fn round_trip_strictly_increasing() {
        // Strictly increasing curve (here convex, power^2-ish but invertible).
        let c = PowerCurve::from_samples(&[
            (0.0, 0.0),
            (0.25, 0.0625),
            (0.5, 0.25),
            (0.75, 0.5625),
            (1.0, 1.0),
        ]);
        for &p in &[0.1, 0.3, 0.5, 0.7, 0.9] {
            let back = c.power_for_depth(c.depth_at(p));
            assert!((back - p).abs() < 1e-6, "round-trip failed: p={p} back={back}");
        }
    }

    #[test]
    fn convex_curve_corrects_nonlinearity() {
        // depth ∝ power^2 sampled densely so interpolation is accurate.
        let mut s = Vec::new();
        for k in 0..=20 {
            let p = k as f64 / 20.0;
            s.push((p, p * p));
        }
        let c = PowerCurve::from_samples(&s);
        let vf = c.visual_factor(0.5);
        // depth_full = 1.0; target = 0.5; power_for_depth(0.5) = sqrt(0.5) ≈ 0.707.
        assert!(vf > 0.0 && vf <= 1.0);
        assert!((vf - 0.5).abs() > 1e-3, "convex curve must shift the factor");
        assert!((vf - 0.5_f64.sqrt()).abs() < 1e-2, "vf={vf}");
    }

    #[test]
    fn near_linear_curve_preserves_factor() {
        let c = PowerCurve::from_samples(&[
            (0.0, 0.0),
            (0.25, 0.25),
            (0.5, 0.5),
            (0.75, 0.75),
            (1.0, 1.0),
        ]);
        assert!((c.visual_factor(0.5) - 0.5).abs() < EPS);
        assert!((c.visual_factor(0.25) - 0.25).abs() < EPS);
        assert!((c.visual_factor(1.0) - 1.0).abs() < EPS);
    }

    #[test]
    fn identity_fallback_empty() {
        let c = PowerCurve::from_samples(&[]);
        for &p in &[0.0, 0.2, 0.5, 0.8, 1.0] {
            assert!((c.depth_at(p) - p).abs() < EPS, "depth_at({p}) should ≈ p");
        }
        for &f in &[0.1, 0.5, 1.0] {
            assert!((c.visual_factor(f) - f).abs() < EPS, "visual_factor({f}) should ≈ f");
        }
    }

    #[test]
    fn single_sample_is_identity() {
        let c = PowerCurve::from_samples(&[(0.5, 7.0)]);
        for &p in &[0.0, 0.3, 0.5, 0.9, 1.0] {
            assert!((c.depth_at(p) - p).abs() < EPS);
        }
        assert!((c.visual_factor(0.5) - 0.5).abs() < EPS);
    }

    #[test]
    fn unsorted_and_duplicate_powers() {
        // Out-of-order input with a duplicate power whose depths get averaged.
        let c = PowerCurve::from_samples(&[
            (1.0, 1.0),
            (0.5, 0.4),
            (0.0, 0.0),
            (0.5, 0.6), // averages with the other 0.5 -> depth 0.5
        ]);
        // At p=0.5 the averaged depth should be ~0.5.
        assert!((c.depth_at(0.5) - 0.5).abs() < 1e-9);
        // Still monotone across the sweep.
        let mut prev = f64::NEG_INFINITY;
        for k in 0..=50 {
            let d = c.depth_at(k as f64 / 50.0);
            assert!(d >= prev - 1e-9);
            prev = d;
        }
    }

    #[test]
    fn queries_clamp_and_stay_finite() {
        let c = PowerCurve::from_samples(&[(0.0, 0.0), (1.0, 2.0)]);
        // Out-of-range power inputs clamp into [0,1] knot range.
        assert!((c.depth_at(-5.0) - 0.0).abs() < EPS);
        assert!((c.depth_at(5.0) - 2.0).abs() < EPS);
        // Out-of-range depth inputs clamp to the power range, never NaN.
        assert!(c.power_for_depth(-10.0).is_finite());
        assert!(c.power_for_depth(100.0).is_finite());
        assert!((c.power_for_depth(100.0) - 1.0).abs() < EPS);
        // NaN/Inf inputs never propagate.
        assert!(c.depth_at(f64::NAN).is_finite());
        assert!(c.visual_factor(f64::INFINITY).is_finite());
        assert!(c.visual_factor(f64::NAN) > 0.0);
    }
}
