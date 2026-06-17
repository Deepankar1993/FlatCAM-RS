//! Trace-follow tool paths (port of `ToolFollow`).
//!
//! Unlike isolation routing — which offsets copper features by the tool radius
//! and cuts around them — *follow* simply traces along the **centre lines** of
//! the source geometry with no offset applied. The tool engraves directly over
//! each trace centreline, which is useful for marking, engraving, or scoring
//! along a path rather than isolating it.
//!
//! Each input [`fc_geo::LineString`] becomes one [`Polyline`], and the whole
//! set is wrapped into a milling [`CncJob`] via [`follow_job`].

use fc_gcode::{CncJob, JobKind, JobParams, Polyline, Units};

/// Convert centre-line geometry to follow tool-paths.
///
/// Each [`fc_geo::LineString`] is copied verbatim (no offset) into a
/// [`Polyline`]; point ordering and counts are preserved exactly.
pub fn follow_paths(lines: &[fc_geo::LineString<f64>]) -> Vec<Polyline> {
    lines
        .iter()
        .map(|ls| ls.coords().map(|c| (c.x, c.y)).collect())
        .collect()
}

/// Build a follow [`CncJob`] (a [`JobKind::Mill`]) from centre-line geometry.
///
/// The supplied [`JobParams`] are carried through; only [`JobParams::units`]
/// is overridden with the given `units`.
pub fn follow_job(
    lines: &[fc_geo::LineString<f64>],
    mut job: JobParams,
    units: Units,
) -> CncJob {
    job.units = units;
    let paths = follow_paths(lines);
    CncJob {
        params: job,
        kind: JobKind::Mill { paths },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{Coord, LineString};

    fn sample_lines() -> Vec<LineString<f64>> {
        let a = LineString::new(vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 1.0, y: 0.0 },
            Coord { x: 1.0, y: 1.0 },
        ]);
        let b = LineString::new(vec![
            Coord { x: 2.0, y: 2.0 },
            Coord { x: 3.0, y: 5.0 },
        ]);
        vec![a, b]
    }

    #[test]
    fn follow_paths_preserves_point_counts() {
        let lines = sample_lines();
        let paths = follow_paths(&lines);
        assert_eq!(paths.len(), 2, "one polyline per linestring");
        assert_eq!(paths[0].len(), 3);
        assert_eq!(paths[1].len(), 2);
    }

    #[test]
    fn follow_paths_preserves_coordinates() {
        let lines = sample_lines();
        let paths = follow_paths(&lines);
        // no offset: coordinates copied verbatim
        assert_eq!(paths[0][0], (0.0, 0.0));
        assert_eq!(paths[0][2], (1.0, 1.0));
        assert_eq!(paths[1][1], (3.0, 5.0));
    }

    #[test]
    fn follow_paths_empty_input() {
        let paths = follow_paths(&[]);
        assert!(paths.is_empty());
    }

    #[test]
    fn follow_job_is_mill() {
        let lines = sample_lines();
        let job = follow_job(&lines, JobParams::default(), Units::Inch);
        assert!(matches!(job.params.units, Units::Inch));
        match &job.kind {
            JobKind::Mill { paths } => {
                assert_eq!(paths.len(), 2);
                assert_eq!(paths[0].len(), 3);
            }
            _ => panic!("expected a mill job"),
        }
    }

    #[test]
    fn follow_job_overrides_units() {
        let lines = sample_lines();
        let job = follow_job(&lines, JobParams::default(), Units::Mm);
        assert!(matches!(job.params.units, Units::Mm));
    }
}
