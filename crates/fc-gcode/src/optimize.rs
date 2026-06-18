//! Path optimization for CNC jobs.
//!
//! The toolpaths produced upstream (Gerber tracing, isolation offsetting, …)
//! often contain long runs of nearly-collinear vertices. Emitting every one of
//! them bloats the G-code and slows the controller without changing the cut.
//! [`simplify_collinear`] drops interior points that lie within `tol` of the
//! straight line spanning their neighbours, and [`optimize_job`] applies that to
//! every milling path while leaving drill points untouched.

use crate::{CncJob, JobKind, Polyline};

/// Perpendicular distance from point `p` to the (infinite) line through `a`–`b`.
///
/// When `a == b` (degenerate segment) this falls back to the point-to-point
/// distance from `p` to `a`.
fn perp_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= f64::EPSILON {
        let ex = p.0 - a.0;
        let ey = p.1 - a.1;
        return (ex * ex + ey * ey).sqrt();
    }
    // |cross product| / |a->b|
    let cross = (p.0 - a.0) * dy - (p.1 - a.1) * dx;
    cross.abs() / len_sq.sqrt()
}

/// Remove interior points that are within `tol` of the line between the
/// last-kept point and the following point.
///
/// This is a single forward pass (not full Douglas–Peucker): for each candidate
/// interior point we measure its perpendicular distance to the line from the
/// most recently kept point to the next point; if it is `<= tol` we drop it,
/// otherwise we keep it and it becomes the new anchor. The first and last points
/// are always retained. Paths with fewer than three points are returned as-is.
pub fn simplify_collinear(path: &[(f64, f64)], tol: f64) -> Vec<(f64, f64)> {
    if path.len() < 3 {
        return path.to_vec();
    }

    let mut out: Vec<(f64, f64)> = Vec::with_capacity(path.len());
    out.push(path[0]);

    let mut prev = path[0];
    for i in 1..path.len() - 1 {
        let cur = path[i];
        let next = path[i + 1];
        if perp_distance(cur, prev, next) <= tol {
            // Near-collinear: drop `cur`, keep `prev` as the anchor.
            continue;
        }
        out.push(cur);
        prev = cur;
    }

    out.push(path[path.len() - 1]);
    out
}

/// Return a copy of `job` with milling paths simplified via
/// [`simplify_collinear`]. [`JobKind::Drill`] points are passed through
/// unchanged. The [`crate::JobParams`] are always preserved.
pub fn optimize_job(job: &CncJob, tol: f64) -> CncJob {
    let kind = match &job.kind {
        JobKind::Mill { paths } => {
            let paths: Vec<Polyline> = paths
                .iter()
                .map(|p| simplify_collinear(p, tol))
                .collect();
            JobKind::Mill { paths }
        }
        JobKind::Drill { points } => JobKind::Drill {
            points: points.clone(),
        },
    };
    CncJob {
        params: job.params.clone(),
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{JobParams, Units};

    #[test]
    fn straight_line_collapses_to_endpoints() {
        let path: Vec<(f64, f64)> = (0..=10).map(|i| (i as f64, 0.0)).collect();
        let s = simplify_collinear(&path, 1e-9);
        assert_eq!(s, vec![(0.0, 0.0), (10.0, 0.0)]);
    }

    #[test]
    fn l_shape_keeps_corner() {
        // Horizontal run, sharp 90-degree turn, vertical run.
        let path = vec![
            (0.0, 0.0),
            (1.0, 0.0),
            (2.0, 0.0),
            (3.0, 0.0), // corner
            (3.0, 1.0),
            (3.0, 2.0),
            (3.0, 3.0),
        ];
        let s = simplify_collinear(&path, 1e-9);
        assert_eq!(s, vec![(0.0, 0.0), (3.0, 0.0), (3.0, 3.0)]);
    }

    #[test]
    fn short_path_unchanged() {
        let path = vec![(0.0, 0.0), (5.0, 5.0)];
        assert_eq!(simplify_collinear(&path, 1.0), path);
        let single = vec![(1.0, 2.0)];
        assert_eq!(simplify_collinear(&single, 1.0), single);
    }

    #[test]
    fn point_within_tolerance_is_dropped() {
        // Middle point sits 0.5 off the line; tol of 1.0 should drop it.
        let path = vec![(0.0, 0.0), (1.0, 0.5), (2.0, 0.0)];
        assert_eq!(simplify_collinear(&path, 1.0), vec![(0.0, 0.0), (2.0, 0.0)]);
        // A tighter tolerance keeps it.
        assert_eq!(simplify_collinear(&path, 0.1), path);
    }

    fn count_points(job: &CncJob) -> usize {
        match &job.kind {
            JobKind::Mill { paths } => paths.iter().map(|p| p.len()).sum(),
            JobKind::Drill { points } => points.len(),
        }
    }

    #[test]
    fn optimize_mill_reduces_points() {
        let params = JobParams {
            units: Units::Mm,
            ..JobParams::default()
        };
        let dense: Vec<(f64, f64)> = (0..=20).map(|i| (i as f64, 0.0)).collect();
        let job = CncJob {
            params: params.clone(),
            kind: JobKind::Mill {
                paths: vec![dense.clone()],
            },
        };
        let before = count_points(&job);
        let opt = optimize_job(&job, 1e-9);
        let after = count_points(&opt);
        assert!(after < before, "expected reduction: {after} < {before}");
        assert_eq!(after, 2);
        // Params preserved.
        assert_eq!(opt.params.units, Units::Mm);
        assert_eq!(opt.params.spindle_rpm, params.spindle_rpm);
    }

    #[test]
    fn optimize_drill_unchanged() {
        let points = vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0)];
        let job = CncJob {
            params: JobParams::default(),
            kind: JobKind::Drill {
                points: points.clone(),
            },
        };
        let opt = optimize_job(&job, 1e-9);
        match opt.kind {
            JobKind::Drill { points: out } => assert_eq!(out, points),
            JobKind::Mill { .. } => panic!("drill job became a mill job"),
        }
    }
}
