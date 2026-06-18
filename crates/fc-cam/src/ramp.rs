//! Ramped plunge entry generation.
//!
//! Instead of plunging straight down to the cut depth (which is hard on small
//! end mills), a ramp entry descends gradually while moving horizontally
//! toward a target point.

/// Produce a ramped plunge entry.
///
/// Returns `steps + 1` points starting at `start` (with `z = 0`) and descending
/// linearly to `z = cut_z` while advancing from `start` toward `toward` over a
/// total horizontal distance of `ramp_len`. The horizontal distance is clamped
/// to the available distance between `start` and `toward`.
///
/// If `start == toward` or `ramp_len <= 0.0`, a single point performing a
/// straight plunge at `cut_z` is returned: `vec![(start.0, start.1, cut_z)]`.
///
/// `cut_z` is expected to be negative (the cutting depth).
pub fn ramp_entry(
    start: (f64, f64),
    toward: (f64, f64),
    cut_z: f64,
    ramp_len: f64,
    steps: usize,
) -> Vec<(f64, f64, f64)> {
    let dx = toward.0 - start.0;
    let dy = toward.1 - start.1;
    let avail = (dx * dx + dy * dy).sqrt();

    // Degenerate cases: nowhere to ramp toward, or no ramp requested.
    if avail <= 0.0 || ramp_len <= 0.0 || steps == 0 {
        return vec![(start.0, start.1, cut_z)];
    }

    // Clamp the ramp length to the available horizontal distance.
    let len = if ramp_len > avail { avail } else { ramp_len };

    // Unit direction from start toward toward.
    let ux = dx / avail;
    let uy = dy / avail;

    let mut out = Vec::with_capacity(steps + 1);
    let n = steps as f64;
    for i in 0..=steps {
        let t = i as f64 / n; // 0.0 ..= 1.0
        let d = len * t;
        let x = start.0 + ux * d;
        let y = start.1 + uy * d;
        let z = cut_z * t; // 0.0 -> cut_z linearly
        out.push((x, y, z));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn basic_ramp() {
        let pts = ramp_entry((0.0, 0.0), (10.0, 0.0), -1.0, 4.0, 4);
        assert_eq!(pts.len(), 5);
        assert!(approx(pts[0].2, 0.0));
        assert!(approx(pts[4].2, -1.0));
        assert!(approx(pts[4].0, 4.0));
        assert!(approx(pts[4].1, 0.0));
    }

    #[test]
    fn z_monotonic_decreasing() {
        let pts = ramp_entry((0.0, 0.0), (10.0, 0.0), -1.0, 4.0, 4);
        for w in pts.windows(2) {
            assert!(w[1].2 < w[0].2, "z must strictly decrease");
        }
    }

    #[test]
    fn degenerate_same_point() {
        let pts = ramp_entry((3.0, 5.0), (3.0, 5.0), -2.0, 4.0, 4);
        assert_eq!(pts.len(), 1);
        assert!(approx(pts[0].0, 3.0));
        assert!(approx(pts[0].1, 5.0));
        assert!(approx(pts[0].2, -2.0));
    }

    #[test]
    fn degenerate_zero_ramp_len() {
        let pts = ramp_entry((1.0, 2.0), (10.0, 0.0), -2.0, 0.0, 4);
        assert_eq!(pts.len(), 1);
        assert!(approx(pts[0].0, 1.0));
        assert!(approx(pts[0].1, 2.0));
        assert!(approx(pts[0].2, -2.0));
    }

    #[test]
    fn ramp_len_clamped_to_available() {
        // Available distance is 3.0, request 100.0 -> last xy lands on toward.
        let pts = ramp_entry((0.0, 0.0), (3.0, 0.0), -1.0, 100.0, 3);
        assert_eq!(pts.len(), 4);
        assert!(approx(pts[3].0, 3.0));
        assert!(approx(pts[3].1, 0.0));
        assert!(approx(pts[3].2, -1.0));
    }

    #[test]
    fn diagonal_ramp() {
        // 3-4-5 triangle: available distance 5.0, ramp 5.0.
        let pts = ramp_entry((0.0, 0.0), (3.0, 4.0), -2.0, 5.0, 5);
        assert_eq!(pts.len(), 6);
        assert!(approx(pts[5].0, 3.0));
        assert!(approx(pts[5].1, 4.0));
        assert!(approx(pts[5].2, -2.0));
        assert!(approx(pts[0].2, 0.0));
    }

    #[test]
    fn zero_steps_is_straight_plunge() {
        let pts = ramp_entry((0.0, 0.0), (10.0, 0.0), -1.0, 4.0, 0);
        assert_eq!(pts.len(), 1);
        assert!(approx(pts[0].2, -1.0));
    }
}
