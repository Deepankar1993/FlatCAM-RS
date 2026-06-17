//! `fc-cam` — CAM operations: isolation routing and drilling.
//!
//! Port of the geometry-generating heart of FlatCAM's `ToolIsolation` and
//! `ToolDrilling` plugins. Given parsed [`fc_gerber::Gerber`] /
//! [`fc_excellon::Excellon`] objects, these functions produce abstract
//! [`fc_gcode::CncJob`]s ready to be rendered to any G-code dialect.

use fc_gcode::{CncJob, JobKind, JobParams, Polyline, Units};
use fc_geo::{offset, MultiPolygon};
use fc_gerber::Gerber;
use fc_excellon::Excellon;

/// Parameters for isolation routing.
#[derive(Clone, Debug)]
pub struct IsolationParams {
    pub tool_diameter: f64,
    /// Number of isolation passes around each copper feature.
    pub passes: usize,
    /// Fractional overlap between adjacent passes (0.0..1.0).
    pub overlap: f64,
    /// Milling parameters carried into the generated job.
    pub job: JobParams,
}

impl Default for IsolationParams {
    fn default() -> Self {
        IsolationParams {
            tool_diameter: 0.1,
            passes: 1,
            overlap: 0.0,
            job: JobParams::default(),
        }
    }
}

/// Convert document units between the gerber and gcode crate enums.
fn map_units(u: fc_gerber::Units) -> Units {
    match u {
        fc_gerber::Units::Mm => Units::Mm,
        fc_gerber::Units::Inch => Units::Inch,
    }
}

fn map_units_exc(u: fc_excellon::Units) -> Units {
    match u {
        fc_excellon::Units::Mm => Units::Mm,
        fc_excellon::Units::Inch => Units::Inch,
    }
}

/// Extract every ring (exterior + interiors) of a multipolygon as a closed
/// polyline. These are the tool-path centre lines for isolation.
pub fn rings_to_polylines(mp: &MultiPolygon<f64>) -> Vec<Polyline> {
    let mut out = Vec::new();
    for poly in &mp.0 {
        out.push(ring_coords(poly.exterior()));
        for hole in poly.interiors() {
            out.push(ring_coords(hole));
        }
    }
    out
}

fn ring_coords(ls: &geo::LineString<f64>) -> Polyline {
    ls.coords().map(|c| (c.x, c.y)).collect()
}

/// Build an isolation [`CncJob`] from a parsed Gerber.
///
/// Each pass `i` (0-based) offsets the copper geometry outward by
/// `tool_radius + i * tool_diameter * (1 - overlap)`, then takes the boundary
/// of the result as the cut path — matching FlatCAM's offset-based isolation.
pub fn isolation(gerber: &Gerber, params: &IsolationParams) -> CncJob {
    let r = params.tool_diameter / 2.0;
    let step = params.tool_diameter * (1.0 - params.overlap.clamp(0.0, 0.999));
    let mut paths: Vec<Polyline> = Vec::new();
    for i in 0..params.passes.max(1) {
        let dist = r + (i as f64) * step;
        let grown = offset(&gerber.solid_geometry, dist);
        paths.extend(rings_to_polylines(&grown));
    }
    let mut job = params.job.clone();
    job.units = map_units(gerber.units);
    job.tool_diameter = params.tool_diameter;
    CncJob {
        params: job,
        kind: JobKind::Mill { paths },
    }
}

/// Build a drilling [`CncJob`] for a single tool of a parsed Excellon file.
pub fn drilling(exc: &Excellon, tool: i32, mut job: JobParams) -> CncJob {
    let points = exc
        .tools
        .get(&tool)
        .map(|t| t.drills.clone())
        .unwrap_or_default();
    job.units = map_units_exc(exc.units);
    if let Some(t) = exc.tools.get(&tool) {
        job.tool_diameter = t.diameter;
    }
    CncJob {
        params: job,
        kind: JobKind::Drill { points },
    }
}

/// Build a drilling job covering all tools (drills only), in tool order.
pub fn drilling_all(exc: &Excellon, job: JobParams) -> Vec<(i32, CncJob)> {
    exc.tools
        .keys()
        .map(|&tool| (tool, drilling(exc, tool, job.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAD_GERBER: &str = "\
%FSLAX24Y24*%
%MOMM*%
%ADD10C,1.0*%
D10*
X0Y0D03*
M02*
";

    #[test]
    fn isolation_makes_a_ring() {
        let g = fc_gerber::parse(PAD_GERBER).unwrap();
        let params = IsolationParams {
            tool_diameter: 0.2,
            passes: 1,
            overlap: 0.0,
            ..Default::default()
        };
        let job = isolation(&g, &params);
        match &job.kind {
            JobKind::Mill { paths } => {
                assert_eq!(paths.len(), 1, "single pad => one isolation ring");
                assert!(paths[0].len() > 8, "ring should be a polygon");
            }
            _ => panic!("expected mill job"),
        }
    }

    #[test]
    fn isolation_multipass_makes_more_rings() {
        let g = fc_gerber::parse(PAD_GERBER).unwrap();
        let params = IsolationParams {
            tool_diameter: 0.2,
            passes: 3,
            overlap: 0.1,
            ..Default::default()
        };
        let job = isolation(&g, &params);
        if let JobKind::Mill { paths } = &job.kind {
            assert_eq!(paths.len(), 3, "three passes => three rings");
        }
    }

    #[test]
    fn isolation_gcode_renders() {
        let g = fc_gerber::parse(PAD_GERBER).unwrap();
        let job = isolation(&g, &IsolationParams::default());
        let gcode = job.to_gcode(&fc_gcode::Grbl);
        assert!(gcode.contains("G21")); // mm
        assert!(gcode.contains("M30"));
    }

    #[test]
    fn drilling_job_from_excellon() {
        let src = "\
M48
METRIC,LZ
T1C0.8
%
T1
X10.0Y10.0
X20.0Y10.0
M30
";
        let e = fc_excellon::parse(src).unwrap();
        let job = drilling(&e, 1, JobParams::default());
        if let JobKind::Drill { points } = &job.kind {
            assert_eq!(points.len(), 2);
        }
        assert!((job.params.tool_diameter - 0.8).abs() < 1e-9);
        let gcode = job.to_gcode(&fc_gcode::Grbl);
        assert_eq!(gcode.matches("G01 Z").count(), 2);
    }
}
