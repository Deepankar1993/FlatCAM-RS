//! Bed levelling probe grid (port of `ToolLevelling`'s core).
//!
//! A non-flat PCB blank, an imperfectly trammed bed, or a warped sacrificial
//! layer all cause the effective Z height to vary across the board. To
//! compensate, the machine first *probes* a grid of points spanning the work
//! area, recording the measured Z at each. The resulting height map is later
//! used to bend the tool paths to follow the surface.
//!
//! This module only generates the probe *positions* (and a probe job that
//! lowers the tool to touch each one); the measured heights and the warping of
//! tool paths are out of scope here.

use fc_gcode::{CncJob, JobKind, JobParams, Units};

/// Generate an evenly-spaced grid of probe points over `bounds`.
///
/// `bounds` is `(minx, miny, maxx, maxy)`. The grid has `cols` columns and
/// `rows` rows, with points placed on the edges (so the four corners of
/// `bounds` are always included). `cols`/`rows` below 2 are clamped to 2 so a
/// usable map (corners included) is always produced.
///
/// Points are ordered row-major: all points of the bottom row (increasing x),
/// then the next row up, and so on.
pub fn probe_grid(bounds: (f64, f64, f64, f64), cols: usize, rows: usize) -> Vec<(f64, f64)> {
    let (minx, miny, maxx, maxy) = bounds;
    let cols = cols.max(2);
    let rows = rows.max(2);

    let dx = (maxx - minx) / (cols as f64 - 1.0);
    let dy = (maxy - miny) / (rows as f64 - 1.0);

    let mut points = Vec::with_capacity(cols * rows);
    for r in 0..rows {
        let y = miny + dy * r as f64;
        for c in 0..cols {
            let x = minx + dx * c as f64;
            points.push((x, y));
        }
    }
    points
}

/// Build a probe [`CncJob`] that touches each point in `points`.
///
/// A probe is modelled as a [`JobKind::Drill`]: at every grid location the
/// tool is brought down to make contact (the "drill" move), which is exactly
/// the touch-probe motion a height map needs. Units are set on the returned
/// job's params.
pub fn probe_job(points: &[(f64, f64)], mut job: JobParams, units: Units) -> CncJob {
    job.units = units;
    CncJob {
        params: job,
        kind: JobKind::Drill {
            points: points.to_vec(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_3x3_has_nine_points_with_corners() {
        let pts = probe_grid((0.0, 0.0, 10.0, 10.0), 3, 3);
        assert_eq!(pts.len(), 9);
        assert!(pts.contains(&(0.0, 0.0)), "bottom-left corner included");
        assert!(pts.contains(&(10.0, 10.0)), "top-right corner included");
        assert!(pts.contains(&(10.0, 0.0)), "bottom-right corner included");
        assert!(pts.contains(&(0.0, 10.0)), "top-left corner included");
        assert!(pts.contains(&(5.0, 5.0)), "centre point included");
    }

    #[test]
    fn grid_is_row_major() {
        let pts = probe_grid((0.0, 0.0, 2.0, 2.0), 3, 3);
        // First three points are the bottom row, increasing x.
        assert_eq!(pts[0], (0.0, 0.0));
        assert_eq!(pts[1], (1.0, 0.0));
        assert_eq!(pts[2], (2.0, 0.0));
        // Next row starts at y = 1.0.
        assert_eq!(pts[3], (0.0, 1.0));
    }

    #[test]
    fn small_dims_are_clamped_to_two() {
        // 0 cols / 1 row -> clamped to 2 x 2 = 4 points (just the corners).
        let pts = probe_grid((0.0, 0.0, 4.0, 6.0), 0, 1);
        assert_eq!(pts.len(), 4);
        assert!(pts.contains(&(0.0, 0.0)));
        assert!(pts.contains(&(4.0, 6.0)));
        // No NaN/Inf from a divide-by-zero.
        for (x, y) in &pts {
            assert!(x.is_finite() && y.is_finite());
        }
    }

    #[test]
    fn non_origin_bounds_span_correctly() {
        let pts = probe_grid((-1.0, 2.0, 1.0, 4.0), 2, 2);
        assert_eq!(pts.len(), 4);
        assert!(pts.contains(&(-1.0, 2.0)));
        assert!(pts.contains(&(1.0, 4.0)));
    }

    #[test]
    fn probe_job_is_a_drill_job() {
        let pts = probe_grid((0.0, 0.0, 10.0, 10.0), 3, 3);
        let job = probe_job(&pts, JobParams::default(), Units::Mm);
        match &job.kind {
            JobKind::Drill { points } => assert_eq!(points.len(), 9),
            _ => panic!("expected a drill (probe) job"),
        }
        assert!(matches!(job.params.units, Units::Mm));
    }

    #[test]
    fn probe_job_honours_units() {
        let job = probe_job(&[(0.0, 0.0)], JobParams::default(), Units::Inch);
        assert!(matches!(job.params.units, Units::Inch));
    }
}
