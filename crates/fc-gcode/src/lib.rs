//! `fc-gcode` — CNC job model and G-code emission.
//!
//! This is the Rust analogue of FlatCAM's `CNCjob` (in `camlib.py`) plus the
//! `preprocessors/` post-processor framework. A [`CncJob`] holds the abstract
//! tool motions (milling polylines and/or drill points) in document units; a
//! [`Preprocessor`] turns those motions into a concrete G-code dialect.
//!
//! Two preprocessors ship here: [`Grbl`] (a generic GRBL/3018-class router) and
//! [`Marlin`]. Adding a dialect means implementing the [`Preprocessor`] trait,
//! mirroring how `appPreProcessor.py` subclasses register G-code variants.

use std::fmt::Write as _;

pub mod dialects;
pub mod dialects_extra;
pub mod dialects_more;
pub mod reader;
pub use reader::parse_gcode;

/// Units a job is expressed in (drives `G20`/`G21`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Inch,
    Mm,
}

/// Machining parameters shared by all motions of a job.
#[derive(Clone, Debug)]
pub struct JobParams {
    pub units: Units,
    pub tool_diameter: f64,
    pub cut_z: f64,        // negative: depth of cut
    pub travel_z: f64,     // clearance height for rapids
    pub depth_per_pass: f64, // >0: multi-pass plunging
    pub feed_xy: f64,
    pub feed_z: f64,
    pub spindle_rpm: f64,
}

impl Default for JobParams {
    fn default() -> Self {
        JobParams {
            units: Units::Mm,
            tool_diameter: 0.1,
            cut_z: -0.05,
            travel_z: 2.0,
            depth_per_pass: 0.0,
            feed_xy: 120.0,
            feed_z: 60.0,
            spindle_rpm: 1000.0,
        }
    }
}

/// A polyline to be milled at `cut_z` (XY coordinates in document units).
pub type Polyline = Vec<(f64, f64)>;

/// What kind of work the job performs.
#[derive(Clone, Debug)]
pub enum JobKind {
    /// Milling/isolation: follow each polyline at depth.
    Mill { paths: Vec<Polyline> },
    /// Drilling: plunge at each point.
    Drill { points: Vec<(f64, f64)> },
}

/// An abstract CNC job, independent of any G-code dialect.
#[derive(Clone, Debug)]
pub struct CncJob {
    pub params: JobParams,
    pub kind: JobKind,
}

impl CncJob {
    /// Render the job to G-code using the given preprocessor.
    pub fn to_gcode(&self, pp: &dyn Preprocessor) -> String {
        let mut g = String::new();
        pp.header(&mut g, &self.params);
        match &self.kind {
            JobKind::Mill { paths } => self.emit_mill(pp, paths, &mut g),
            JobKind::Drill { points } => self.emit_drill(pp, points, &mut g),
        }
        pp.footer(&mut g, &self.params);
        g
    }

    fn emit_mill(&self, pp: &dyn Preprocessor, paths: &[Polyline], g: &mut String) {
        let p = &self.params;
        let depths = pass_depths(p.cut_z, p.depth_per_pass);
        for path in paths {
            if path.len() < 2 {
                continue;
            }
            for &z in &depths {
                pp.rapid_z(g, p.travel_z);
                pp.rapid_xy(g, path[0].0, path[0].1);
                pp.plunge(g, z, p);
                for &(x, y) in &path[1..] {
                    pp.linear(g, x, y, p);
                }
            }
            pp.rapid_z(g, p.travel_z);
        }
    }

    fn emit_drill(&self, pp: &dyn Preprocessor, points: &[(f64, f64)], g: &mut String) {
        let p = &self.params;
        for &(x, y) in points {
            pp.rapid_z(g, p.travel_z);
            pp.rapid_xy(g, x, y);
            pp.plunge(g, p.cut_z, p);
            pp.rapid_z(g, p.travel_z);
        }
    }
}

/// Compute the sequence of intermediate Z depths for multi-pass plunging.
/// Returns just `[cut_z]` when `depth_per_pass <= 0`.
pub fn pass_depths(cut_z: f64, depth_per_pass: f64) -> Vec<f64> {
    if depth_per_pass <= 0.0 || cut_z >= 0.0 {
        return vec![cut_z];
    }
    let mut depths = Vec::new();
    let mut z = -depth_per_pass;
    while z > cut_z {
        depths.push(z);
        z -= depth_per_pass;
    }
    depths.push(cut_z);
    depths
}

/// A G-code dialect. Implementors write directly into the output buffer.
pub trait Preprocessor {
    fn name(&self) -> &str;
    fn header(&self, g: &mut String, p: &JobParams);
    fn footer(&self, g: &mut String, p: &JobParams);
    fn rapid_z(&self, g: &mut String, z: f64);
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64);
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams);
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams);
}

/// Generic GRBL / GRBL-1.1 router preprocessor.
pub struct Grbl;

impl Preprocessor for Grbl {
    fn name(&self) -> &str {
        "GRBL"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "(FlatCAM-RS GRBL preprocessor)");
        let _ = writeln!(g, "(tool dia: {:.4})", p.tool_diameter);
        let _ = writeln!(g, "G90");
        let _ = writeln!(
            g,
            "{}",
            if p.units == Units::Mm { "G21" } else { "G20" }
        );
        let _ = writeln!(g, "M03 S{:.0}", p.spindle_rpm);
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "G00 Z{:.4}", p.travel_z);
        let _ = writeln!(g, "M05");
        let _ = writeln!(g, "G00 X0.0000 Y0.0000");
        let _ = writeln!(g, "M30");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        let _ = writeln!(g, "G00 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G00 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 Z{:.4} F{:.0}", z, p.feed_z);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G01 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

/// Marlin-flavoured preprocessor (laser/3D-printer-derived CNC firmware).
pub struct Marlin;

impl Preprocessor for Marlin {
    fn name(&self) -> &str {
        "Marlin"
    }
    fn header(&self, g: &mut String, p: &JobParams) {
        let _ = writeln!(g, "; FlatCAM-RS Marlin preprocessor");
        let _ = writeln!(g, "G90");
        let _ = writeln!(g, "{}", if p.units == Units::Mm { "G21" } else { "G20" });
        let _ = writeln!(g, "M3 S{:.0}", p.spindle_rpm);
        let _ = writeln!(g, "G0 Z{:.4}", p.travel_z);
    }
    fn footer(&self, g: &mut String, _p: &JobParams) {
        let _ = writeln!(g, "M5");
        let _ = writeln!(g, "M2");
    }
    fn rapid_z(&self, g: &mut String, z: f64) {
        let _ = writeln!(g, "G0 Z{:.4}", z);
    }
    fn rapid_xy(&self, g: &mut String, x: f64, y: f64) {
        let _ = writeln!(g, "G0 X{:.4} Y{:.4}", x, y);
    }
    fn plunge(&self, g: &mut String, z: f64, p: &JobParams) {
        let _ = writeln!(g, "G1 Z{:.4} F{:.0}", z, p.feed_z);
    }
    fn linear(&self, g: &mut String, x: f64, y: f64, p: &JobParams) {
        let _ = writeln!(g, "G1 X{:.4} Y{:.4} F{:.0}", x, y, p.feed_xy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_depths_single() {
        assert_eq!(pass_depths(-0.05, 0.0), vec![-0.05]);
    }

    #[test]
    fn pass_depths_multi() {
        let d = pass_depths(-0.3, 0.1);
        assert_eq!(d.len(), 3);
        assert!((d[0] - -0.1).abs() < 1e-9);
        assert!((d[2] - -0.3).abs() < 1e-9);
    }

    #[test]
    fn grbl_mill_gcode_has_structure() {
        let job = CncJob {
            params: JobParams::default(),
            kind: JobKind::Mill {
                paths: vec![vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]],
            },
        };
        let g = job.to_gcode(&Grbl);
        assert!(g.contains("G21"));
        assert!(g.contains("M03 S1000"));
        assert!(g.contains("G01 X10.0000 Y0.0000"));
        assert!(g.contains("M30"));
    }

    #[test]
    fn grbl_drill_gcode() {
        let mut p = JobParams::default();
        p.cut_z = -1.5;
        let job = CncJob {
            params: p,
            kind: JobKind::Drill {
                points: vec![(1.0, 1.0), (2.0, 2.0)],
            },
        };
        let g = job.to_gcode(&Grbl);
        let plunges = g.matches("G01 Z-1.5000").count();
        assert_eq!(plunges, 2, "one plunge per drill");
    }
}
