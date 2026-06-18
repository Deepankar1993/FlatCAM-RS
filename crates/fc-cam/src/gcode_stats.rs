//! Motion statistics estimator for a [`fc_gcode::CncJob`].
//!
//! Walks the abstract job (before any G-code dialect is applied) and tallies
//! cutting distance, rapid-travel distance, plunge count, and a rough machining
//! time estimate. Mirrors the "machining stats" FlatCAM reports after building a
//! CNC job, but works purely on the geometry so it is dialect-independent.

use fc_gcode::{CncJob, JobKind};

/// Estimated motion statistics for a CNC job.
#[derive(Clone, Debug, Default)]
pub struct JobStats {
    /// Total length of cutting moves (XY feed moves along tool-paths).
    pub cut_distance: f64,
    /// Total length of rapid (non-cutting) repositioning moves.
    pub rapid_distance: f64,
    /// Number of plunges into the work (one per path / drill point).
    pub plunge_count: usize,
    /// Rough machining time estimate in seconds.
    pub estimated_seconds: f64,
}

/// Euclidean distance between two 2-D points.
fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    (dx * dx + dy * dy).sqrt()
}

/// Estimate motion statistics from a [`CncJob`].
///
/// For [`JobKind::Mill`] the cut distance is the summed segment length within
/// each path; the gap between one path's end and the next path's start counts as
/// rapid travel, and each path is one plunge. For [`JobKind::Drill`] every point
/// is a plunge and consecutive point-to-point hops are rapid travel.
///
/// The time estimate is `cut_distance / (feed_xy mm/s)` plus the plunge time
/// `plunge_count * |cut_z| / (feed_z mm/s)`. Feeds are taken from `job.params`
/// and are guarded against zero (a zero feed contributes no time).
pub fn stats(job: &CncJob) -> JobStats {
    let mut s = JobStats::default();

    match &job.kind {
        JobKind::Mill { paths } => {
            let mut prev_end: Option<(f64, f64)> = None;
            for path in paths {
                if path.is_empty() {
                    continue;
                }
                s.plunge_count += 1;
                if let Some(end) = prev_end {
                    s.rapid_distance += dist(end, path[0]);
                }
                for w in path.windows(2) {
                    s.cut_distance += dist(w[0], w[1]);
                }
                prev_end = Some(*path.last().unwrap());
            }
        }
        JobKind::Drill { points } => {
            s.plunge_count = points.len();
            for w in points.windows(2) {
                s.rapid_distance += dist(w[0], w[1]);
            }
        }
    }

    let feed_xy = job.params.feed_xy;
    let feed_z = job.params.feed_z;

    if feed_xy > 0.0 {
        s.estimated_seconds += s.cut_distance / (feed_xy / 60.0);
    }
    if feed_z > 0.0 {
        s.estimated_seconds += s.plunge_count as f64 * (job.params.cut_z.abs() / (feed_z / 60.0));
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_gcode::{CncJob, JobKind, JobParams};

    #[test]
    fn single_mill_path() {
        let params = JobParams {
            feed_xy: 60.0,
            feed_z: 0.0,
            cut_z: 0.0,
            ..Default::default()
        };
        let job = CncJob {
            params,
            kind: JobKind::Mill {
                paths: vec![vec![(0.0, 0.0), (10.0, 0.0)]],
            },
        };
        let s = stats(&job);
        assert!((s.cut_distance - 10.0).abs() < 1e-9);
        assert_eq!(s.plunge_count, 1);
        assert!((s.estimated_seconds - 10.0).abs() < 1e-9);
        assert!((s.rapid_distance - 0.0).abs() < 1e-9);
    }

    #[test]
    fn mill_rapid_between_paths() {
        let job = CncJob {
            params: JobParams {
                feed_xy: 0.0,
                feed_z: 0.0,
                ..Default::default()
            },
            kind: JobKind::Mill {
                paths: vec![
                    vec![(0.0, 0.0), (1.0, 0.0)],
                    vec![(4.0, 0.0), (5.0, 0.0)],
                ],
            },
        };
        let s = stats(&job);
        assert!((s.cut_distance - 2.0).abs() < 1e-9);
        // gap from (1,0) to (4,0) == 3.0
        assert!((s.rapid_distance - 3.0).abs() < 1e-9);
        assert_eq!(s.plunge_count, 2);
    }

    #[test]
    fn three_drill_job() {
        let job = CncJob {
            params: JobParams::default(),
            kind: JobKind::Drill {
                points: vec![(0.0, 0.0), (3.0, 0.0), (3.0, 4.0)],
            },
        };
        let s = stats(&job);
        assert_eq!(s.plunge_count, 3);
        // 3 + 4 = 7
        assert!((s.rapid_distance - 7.0).abs() < 1e-9);
        assert!((s.cut_distance - 0.0).abs() < 1e-9);
    }

    #[test]
    fn zero_feed_no_time() {
        let job = CncJob {
            params: JobParams {
                feed_xy: 0.0,
                feed_z: 0.0,
                cut_z: -1.0,
                ..Default::default()
            },
            kind: JobKind::Mill {
                paths: vec![vec![(0.0, 0.0), (10.0, 0.0)]],
            },
        };
        let s = stats(&job);
        assert!((s.estimated_seconds - 0.0).abs() < 1e-9);
    }
}
