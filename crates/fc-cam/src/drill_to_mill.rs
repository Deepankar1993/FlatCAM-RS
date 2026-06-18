//! Drill-to-mill conversion.
//!
//! For holes larger than the milling tool, generate a circular toolpath that
//! mills the hole instead of plunging straight down. Holes that are smaller
//! than or equal to the tool diameter are kept as single plunge points.

use fc_gcode::Polyline;

use std::f64::consts::PI;

/// Build a milling path for a single hole centered at `(cx, cy)`.
///
/// If `hole_dia <= tool_dia` the hole cannot be milled wider than the tool, so
/// a single plunge point `vec![(cx, cy)]` is returned. Otherwise a closed
/// circular polyline of radius `(hole_dia - tool_dia) / 2` is produced with
/// `steps` segments (the first point is repeated at the end to close it).
pub fn hole_loop(cx: f64, cy: f64, hole_dia: f64, tool_dia: f64, steps: usize) -> Polyline {
    if hole_dia <= tool_dia {
        return vec![(cx, cy)];
    }

    let radius = (hole_dia - tool_dia) / 2.0;
    let n = steps.max(3);

    let mut path: Polyline = Vec::with_capacity(n + 1);
    for i in 0..n {
        let theta = (i as f64) / (n as f64) * 2.0 * PI;
        path.push((cx + radius * theta.cos(), cy + radius * theta.sin()));
    }
    // Close the loop.
    let first = path[0];
    path.push(first);
    path
}

/// Build one [`hole_loop`] per point in `points`.
pub fn mill_holes(
    points: &[(f64, f64)],
    hole_dia: f64,
    tool_dia: f64,
    steps: usize,
) -> Vec<Polyline> {
    points
        .iter()
        .map(|&(cx, cy)| hole_loop(cx, cy, hole_dia, tool_dia, steps))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_hole_makes_loop() {
        let steps = 24;
        let loop_path = hole_loop(0.0, 0.0, 3.0, 1.0, steps);

        // More than steps/2 points (closed loop has steps + 1 points).
        assert!(loop_path.len() > steps / 2);
        assert_eq!(loop_path.len(), steps + 1);

        // First and last coincide (closed).
        let first = loop_path[0];
        let last = *loop_path.last().unwrap();
        assert!((first.0 - last.0).abs() < 1e-9);
        assert!((first.1 - last.1).abs() < 1e-9);

        // Radius is (hole_dia - tool_dia) / 2 = 1.0.
        let r = (first.0 * first.0 + first.1 * first.1).sqrt();
        assert!((r - 1.0).abs() < 1e-9);
    }

    #[test]
    fn small_hole_is_single_point() {
        let loop_path = hole_loop(2.0, 3.0, 0.5, 1.0, 24);
        assert_eq!(loop_path.len(), 1);
        assert_eq!(loop_path[0], (2.0, 3.0));
    }

    #[test]
    fn equal_hole_is_single_point() {
        let loop_path = hole_loop(0.0, 0.0, 1.0, 1.0, 24);
        assert_eq!(loop_path.len(), 1);
    }

    #[test]
    fn mill_holes_one_loop_per_point() {
        let points = [(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)];
        let loops = mill_holes(&points, 3.0, 1.0, 16);
        assert_eq!(loops.len(), 3);
        for l in &loops {
            assert_eq!(l.len(), 17);
        }
    }

    #[test]
    fn mill_holes_small_all_points() {
        let points = [(0.0, 0.0), (1.0, 1.0)];
        let loops = mill_holes(&points, 0.5, 1.0, 16);
        assert_eq!(loops.len(), 2);
        for l in &loops {
            assert_eq!(l.len(), 1);
        }
    }

    #[test]
    fn loop_centered_on_point() {
        let loop_path = hole_loop(10.0, -4.0, 4.0, 2.0, 20);
        // radius = 1.0, centered at (10, -4)
        for &(x, y) in &loop_path {
            let d = ((x - 10.0).powi(2) + (y + 4.0).powi(2)).sqrt();
            assert!((d - 1.0).abs() < 1e-9);
        }
    }
}
