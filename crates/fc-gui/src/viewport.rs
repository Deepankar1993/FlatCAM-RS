//! GUI-free grid/ruler math for the plot canvas.
//!
//! Pure f64 helpers that decide where adaptive grid lines and numeric axis
//! labels go. No egui (or any) dependency, so the logic is deterministically
//! unit-testable. `main.rs` calls these to position major grid lines and to
//! render the ruler tick labels.

/// A "nice" 1-2-5 grid step at or above `raw` (e.g. 0.7 -> 1.0, 3.1 -> 5.0,
/// 12.0 -> 20.0). Returns 1.0 for non-positive / non-finite input.
pub fn nice_step(raw: f64) -> f64 {
    if !(raw > 0.0) || !raw.is_finite() {
        return 1.0;
    }
    let exp = raw.log10().floor();
    let base = 10f64.powf(exp);
    let f = raw / base;
    let nice = if f <= 1.0 {
        1.0
    } else if f <= 2.0 {
        2.0
    } else if f <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * base
}

/// The major grid step for a target on-screen spacing. `scale` is pixels per
/// world unit (>0); `target_px` is the desired pixels between major lines
/// (e.g. 80). Returns `nice_step(target_px / scale)`. Guards scale<=0 -> 1.0.
pub fn major_step(scale: f64, target_px: f64) -> f64 {
    if !(scale > 0.0) || !scale.is_finite() {
        return 1.0;
    }
    nice_step(target_px / scale)
}

/// All multiples of `step` lying within the inclusive range [min, max], in
/// ascending order. Empty if step<=0 or the range is degenerate, and capped at
/// `max_count` entries (returns empty if it would exceed the cap, so callers can
/// skip drawing rather than hang). Starts at the first multiple >= min, ends at
/// the last <= max.
pub fn ticks(min: f64, max: f64, step: f64, max_count: usize) -> Vec<f64> {
    if !(step > 0.0) || !step.is_finite() || !min.is_finite() || !max.is_finite() || max < min {
        return Vec::new();
    }
    // Small epsilon so values landing exactly on a boundary aren't dropped to
    // floating-point dust.
    let eps = step * 1e-9;
    let first = (min / step - 1e-9).ceil();
    let last = (max / step + 1e-9).floor();
    if last < first {
        return Vec::new();
    }
    let count = (last - first) as i64 + 1;
    if count <= 0 || count as usize > max_count {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(count as usize);
    let mut k = first as i64;
    let end = last as i64;
    while k <= end {
        let v = k as f64 * step;
        if v >= min - eps && v <= max + eps {
            out.push(v);
        }
        k += 1;
    }
    out
}

/// Round to 3 decimals and trim trailing zeros / dot for a compact ruler label.
/// `0.0` (and -0.0) render as "0".
pub fn format_tick(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    if r.abs() < 1e-9 {
        return "0".to_string();
    }
    let s = format!("{r:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[test]
    fn nice_step_one_two_five() {
        assert!((nice_step(0.7) - 1.0).abs() < EPS);
        assert!((nice_step(1.0) - 1.0).abs() < EPS);
        assert!((nice_step(1.6) - 2.0).abs() < EPS);
        assert!((nice_step(3.1) - 5.0).abs() < EPS);
        assert!((nice_step(6.0) - 10.0).abs() < EPS);
        assert!((nice_step(12.0) - 20.0).abs() < EPS);
    }

    #[test]
    fn nice_step_bad_input() {
        assert!((nice_step(0.0) - 1.0).abs() < EPS);
        assert!((nice_step(-3.0) - 1.0).abs() < EPS);
        assert!((nice_step(f64::NAN) - 1.0).abs() < EPS);
        assert!((nice_step(f64::INFINITY) - 1.0).abs() < EPS);
    }

    #[test]
    fn major_step_basic() {
        // 80 px / 10 px-per-unit = 8.0 world units -> nice_step(8.0) = 10.0.
        assert!((major_step(10.0, 80.0) - 10.0).abs() < EPS);
        assert!((nice_step(8.0) - 10.0).abs() < EPS);
        assert!((major_step(0.0, 80.0) - 1.0).abs() < EPS);
        assert!((major_step(-5.0, 80.0) - 1.0).abs() < EPS);
    }

    #[test]
    fn ticks_aligned_range() {
        let got = ticks(-2.0, 2.0, 1.0, 100);
        let want = [-2.0, -1.0, 0.0, 1.0, 2.0];
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want.iter()) {
            assert!((g - w).abs() < EPS, "got {g}, want {w}");
        }
    }

    #[test]
    fn ticks_non_aligned_range() {
        let got = ticks(0.3, 5.2, 1.0, 100);
        let want = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want.iter()) {
            assert!((g - w).abs() < EPS, "got {g}, want {w}");
        }
        // First tick >= min, last tick <= max.
        assert!(*got.first().unwrap() >= 0.3 - EPS);
        assert!(*got.last().unwrap() <= 5.2 + EPS);
    }

    #[test]
    fn ticks_bad_step_empty() {
        assert!(ticks(-2.0, 2.0, 0.0, 100).is_empty());
        assert!(ticks(-2.0, 2.0, -1.0, 100).is_empty());
    }

    #[test]
    fn ticks_over_cap_empty() {
        // 0..=1000 at step 1.0 would be 1001 ticks; cap of 10 -> empty.
        assert!(ticks(0.0, 1000.0, 1.0, 10).is_empty());
    }

    #[test]
    fn format_tick_compact() {
        assert_eq!(format_tick(0.0), "0");
        assert_eq!(format_tick(-0.0), "0");
        assert_eq!(format_tick(2.5), "2.5");
        assert_eq!(format_tick(10.0), "10");
        assert_eq!(format_tick(12.456), "12.456");
        assert_eq!(format_tick(0.1), "0.1");
    }
}
