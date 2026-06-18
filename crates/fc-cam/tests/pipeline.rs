//! End-to-end pipeline integration tests for `fc-cam`.
//!
//! These exercise the crate as an external dependency would: parse real (inline)
//! Gerber/Excellon fixtures, run the CAM operations, and render the resulting
//! jobs to G-code. They cover the full chain
//! `fc_gerber/fc_excellon -> fc_cam -> fc_gcode`.

use fc_cam::{
    cutout_rectangular, drilling_all, isolation_geo, ncc_job, paint_region, CutoutParams,
    IsolationParams, NccParams, PaintParams,
};
use fc_gcode::{Grbl, JobKind, JobParams, Units};
use fc_geo::{area, bounds};

/// Two pads plus a connecting trace, 1.0mm round aperture, metric.
const GERBER: &str = "%FSLAX24Y24*%\n\
%MOMM*%\n\
%ADD10C,1.0*%\n\
D10*\n\
X0Y0D03*\n\
X50000Y0D03*\n\
M02*\n";

/// Two drilled holes, 0.8mm tool, metric leading-zero suppression.
const EXCELLON: &str = "M48\n\
METRIC,LZ\n\
T1C0.8\n\
%\n\
T1\n\
X10.0Y10.0\n\
X20.0Y10.0\n\
M30\n";

/// Map the gerber unit enum onto the gcode unit enum (kept local to the test so
/// it mirrors what a downstream consumer would have to write).
fn to_gcode_units(u: fc_gerber::Units) -> Units {
    match u {
        fc_gerber::Units::Mm => Units::Mm,
        fc_gerber::Units::Inch => Units::Inch,
    }
}

#[test]
fn gerber_parses_with_positive_copper_area() {
    let g = fc_gerber::parse(GERBER).expect("gerber should parse");
    assert_eq!(g.units, fc_gerber::Units::Mm);
    let a = area(&g.solid_geometry);
    assert!(a > 0.0, "expected positive copper area, got {a}");
    // The two pads must give a non-degenerate bounding box spanning the trace.
    let (minx, _, maxx, _) = bounds(&g.solid_geometry).expect("copper should have bounds");
    assert!(maxx - minx >= 5.0, "pads span ~5mm, got {}", maxx - minx);
}

#[test]
fn isolation_produces_mill_job_with_valid_gcode() {
    let g = fc_gerber::parse(GERBER).unwrap();
    let units = to_gcode_units(g.units);
    let params = IsolationParams {
        tool_diameter: 0.2,
        passes: 1,
        overlap: 0.0,
        job: JobParams::default(),
    };
    let job = isolation_geo(&g.solid_geometry, units, &params);

    let paths = match &job.kind {
        JobKind::Mill { paths } => paths,
        _ => panic!("isolation must yield a Mill job"),
    };
    assert!(!paths.is_empty(), "isolation should produce tool-paths");

    let gcode = job.to_gcode(&Grbl);
    assert!(gcode.contains("G21"), "metric job must emit G21");
    assert!(gcode.contains("M30"), "program must end with M30");
    assert!(
        gcode.matches("G01").count() >= 1,
        "isolation milling must contain at least one linear move"
    );
}

#[test]
fn isolation_multipass_yields_more_paths() {
    let g = fc_gerber::parse(GERBER).unwrap();
    let units = to_gcode_units(g.units);

    let single = isolation_geo(
        &g.solid_geometry,
        units,
        &IsolationParams {
            tool_diameter: 0.2,
            passes: 1,
            overlap: 0.1,
            job: JobParams::default(),
        },
    );
    let multi = isolation_geo(
        &g.solid_geometry,
        units,
        &IsolationParams {
            tool_diameter: 0.2,
            passes: 3,
            overlap: 0.1,
            job: JobParams::default(),
        },
    );

    let n1 = match single.kind {
        JobKind::Mill { paths } => paths.len(),
        _ => panic!("expected mill job"),
    };
    let n3 = match multi.kind {
        JobKind::Mill { paths } => paths.len(),
        _ => panic!("expected mill job"),
    };
    assert!(n3 > n1, "3 passes ({n3}) should yield more paths than 1 ({n1})");
}

#[test]
fn paint_region_fills_the_copper() {
    let g = fc_gerber::parse(GERBER).unwrap();
    let p = PaintParams {
        tool_diameter: 0.3,
        overlap: 0.2,
        margin: 0.0,
        add_contour: true,
        job: JobParams::default(),
    };
    let paths = paint_region(&g.solid_geometry, &p);
    assert!(!paths.is_empty(), "paint should produce non-empty infill paths");
    // every emitted path must be a real polyline (>= 2 points)
    assert!(
        paths.iter().all(|pl| pl.len() >= 2),
        "paint paths must each have at least two points"
    );
}

#[test]
fn ncc_produces_mill_job_clearing_non_copper() {
    let g = fc_gerber::parse(GERBER).unwrap();
    let units = to_gcode_units(g.units);
    let p = NccParams {
        tool_diameter: 0.4,
        overlap: 0.3,
        boundary_margin: 1.0,
        job: JobParams::default(),
    };
    let job = ncc_job(&g.solid_geometry, &p, units);
    match &job.kind {
        JobKind::Mill { paths } => assert!(!paths.is_empty(), "NCC must clear some area"),
        _ => panic!("NCC must yield a Mill job"),
    }
    let gcode = job.to_gcode(&Grbl);
    assert!(gcode.contains("G21"));
    assert!(gcode.contains("M30"));
}

#[test]
fn cutout_rectangular_traces_the_board_outline() {
    let g = fc_gerber::parse(GERBER).unwrap();
    let (minx, miny, maxx, maxy) = bounds(&g.solid_geometry).expect("need bounds for cutout");
    let p = CutoutParams {
        tool_diameter: 1.0,
        tabs: 4,
        tab_gap: 2.0,
        outside: true,
        job: JobParams::default(),
    };
    let paths = cutout_rectangular(minx, miny, maxx, maxy, &p);
    assert!(
        paths.len() >= 4,
        "4 holding tabs should split the outline into >= 4 cut arcs, got {}",
        paths.len()
    );
}

#[test]
fn drilling_all_yields_two_points_and_two_plunges() {
    let e = fc_excellon::parse(EXCELLON).expect("excellon should parse");
    let jobs = drilling_all(&e, JobParams::default());
    assert_eq!(jobs.len(), 1, "fixture has a single tool");

    let (tool, job) = &jobs[0];
    assert_eq!(*tool, 1, "tool number should be T1");

    match &job.kind {
        JobKind::Drill { points } => assert_eq!(points.len(), 2, "two drilled holes"),
        _ => panic!("drilling must yield a Drill job"),
    }
    assert!(
        (job.params.tool_diameter - 0.8).abs() < 1e-9,
        "tool diameter carried from excellon"
    );

    let gcode = job.to_gcode(&Grbl);
    assert!(gcode.contains("G21"), "metric drilling must emit G21");
    assert!(gcode.contains("M30"));
    assert_eq!(
        gcode.matches("G01 Z").count(),
        2,
        "two holes => two plunge moves"
    );
}

#[test]
fn full_pipeline_is_deterministic() {
    // Re-running the whole chain must give byte-identical G-code.
    let run = || {
        let g = fc_gerber::parse(GERBER).unwrap();
        let units = to_gcode_units(g.units);
        let iso = isolation_geo(&g.solid_geometry, units, &IsolationParams::default());
        let e = fc_excellon::parse(EXCELLON).unwrap();
        let (_, drill) = drilling_all(&e, JobParams::default()).remove(0);
        (iso.to_gcode(&Grbl), drill.to_gcode(&Grbl))
    };
    assert_eq!(run(), run(), "pipeline output must be deterministic");
}
