//! Solder-paste dispense paths (port of `ToolSolderPaste`'s core).
//!
//! For each pad, the dispensing nozzle should travel along a centre region of
//! the pad rather than its outer edge, so paste lands inside the copper. We
//! model this by insetting the pads by `nozzle_dia/2 + margin` and dispensing
//! around the resulting boundary rings. Pads smaller than the nozzle vanish
//! after the inset and produce no path.

use fc_gcode::{CncJob, JobKind, JobParams, Polyline, Units};
use fc_geo::{offset, MultiPolygon};

/// Parameters for a solder-paste dispense operation.
#[derive(Clone, Debug)]
pub struct PasteParams {
    /// Dispense nozzle diameter.
    pub nozzle_dia: f64,
    /// Extra clearance kept from the pad boundary.
    pub margin: f64,
    pub job: JobParams,
}

impl Default for PasteParams {
    fn default() -> Self {
        PasteParams {
            nozzle_dia: 0.3,
            margin: 0.0,
            job: JobParams::default(),
        }
    }
}

/// Collect every exterior and interior ring of a multipolygon as polylines.
fn ring_polylines(mp: &MultiPolygon<f64>) -> Vec<Vec<(f64, f64)>> {
    let mut o = vec![];
    for p in &mp.0 {
        o.push(p.exterior().coords().map(|c| (c.x, c.y)).collect());
        for h in p.interiors() {
            o.push(h.coords().map(|c| (c.x, c.y)).collect());
        }
    }
    o
}

/// Generate solder-paste dispense paths (polylines) for a set of pads.
///
/// Each pad is inset by `nozzle_dia/2 + margin`; the dispense head then traces
/// the inset boundary. Pads that disappear under the inset yield nothing.
pub fn paste_paths(pads: &MultiPolygon<f64>, p: &PasteParams) -> Vec<Polyline> {
    let inset = p.nozzle_dia / 2.0 + p.margin;
    let inner = offset(pads, -inset);
    if inner.0.is_empty() {
        return vec![];
    }
    ring_polylines(&inner)
}

/// Build a solder-paste [`CncJob`] for a set of pads, in the given units.
pub fn paste_job(pads: &MultiPolygon<f64>, p: &PasteParams, units: Units) -> CncJob {
    let paths = paste_paths(pads, p);
    let mut job = p.job.clone();
    job.units = units;
    job.tool_diameter = p.nozzle_dia;
    CncJob {
        params: job,
        kind: JobKind::Mill { paths },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::centered_rect;

    #[test]
    fn single_pad_yields_path() {
        // 3x3 pad, 0.3 nozzle -> easily insets to a non-empty region.
        let pads = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 3.0, 3.0)]);
        let p = PasteParams::default();
        let paths = paste_paths(&pads, &p);
        assert!(!paths.is_empty(), "expected at least one dispense path");
        // The single ring should be a closed-ish loop with several points.
        assert!(paths[0].len() >= 4, "ring too short: {}", paths[0].len());
    }

    #[test]
    fn tiny_pad_yields_empty() {
        // Pad smaller than the nozzle diameter must vanish after the inset.
        let pads = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 0.1, 0.1)]);
        let p = PasteParams {
            nozzle_dia: 0.3,
            margin: 0.0,
            job: JobParams::default(),
        };
        let paths = paste_paths(&pads, &p);
        assert!(paths.is_empty(), "tiny pad should produce no paths");
    }

    #[test]
    fn job_is_mill_with_nozzle_dia() {
        let pads = MultiPolygon::new(vec![centered_rect(0.0, 0.0, 3.0, 3.0)]);
        let p = PasteParams::default();
        let job = paste_job(&pads, &p, Units::Mm);
        assert!((job.params.tool_diameter - p.nozzle_dia).abs() < 1e-12);
        match job.kind {
            JobKind::Mill { paths } => assert!(!paths.is_empty()),
            _ => panic!("expected a Mill job"),
        }
    }
}
