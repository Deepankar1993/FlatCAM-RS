//! Non-Copper-Clear (port of `ToolNCC`'s core).
//!
//! NCC removes *all* copper that is not part of the routed pattern, leaving
//! only the wanted traces/pads standing proud of a fully cleared board. We
//! model the board as the copper bounding box grown by a margin, then subtract
//! the copper itself to obtain the region that must be milled away. That region
//! is handed to [`crate::paint::paint_region`] for the actual line-fill — NCC
//! is, in effect, "paint everything that is not copper".

use crate::paint::{paint_region, PaintParams};
use fc_gcode::{CncJob, JobKind, JobParams, Polyline, Units};
use fc_geo::{bounds, centered_rect, difference, MultiPolygon};

/// Parameters for a Non-Copper-Clear operation.
#[derive(Clone, Debug)]
pub struct NccParams {
    pub tool_diameter: f64,
    /// Overlap fraction between adjacent passes (0.0..1.0).
    pub overlap: f64,
    /// Margin added around the copper bounding box to define the board area to clear.
    pub boundary_margin: f64,
    pub job: JobParams,
}

impl Default for NccParams {
    fn default() -> Self {
        NccParams {
            tool_diameter: 0.5,
            overlap: 0.4,
            boundary_margin: 1.0,
            job: JobParams::default(),
        }
    }
}

/// Compute the region that must be cleared: the board rectangle (copper bounds
/// grown by `boundary_margin` on every side) minus the copper itself.
///
/// Returns an empty [`MultiPolygon`] if the copper geometry has no extent.
pub fn ncc_region(copper: &MultiPolygon<f64>, p: &NccParams) -> MultiPolygon<f64> {
    let Some((minx, miny, maxx, maxy)) = bounds(copper) else {
        return MultiPolygon::new(vec![]);
    };

    let m = p.boundary_margin;
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let w = (maxx - minx) + 2.0 * m;
    let h = (maxy - miny) + 2.0 * m;

    let board = MultiPolygon::new(vec![centered_rect(cx, cy, w, h)]);
    difference(&board, copper)
}

/// Generate NCC tool-paths (polylines) clearing all non-copper area.
pub fn ncc_paths(copper: &MultiPolygon<f64>, p: &NccParams) -> Vec<Polyline> {
    let region = ncc_region(copper, p);
    let paint_params = PaintParams {
        tool_diameter: p.tool_diameter,
        overlap: p.overlap,
        margin: 0.0,
        add_contour: true,
        job: p.job.clone(),
    };
    paint_region(&region, &paint_params)
}

/// Build an NCC [`CncJob`] for the given copper geometry, in document units.
pub fn ncc_job(copper: &MultiPolygon<f64>, p: &NccParams, units: Units) -> CncJob {
    let paths = ncc_paths(copper, p);
    let mut job = p.job.clone();
    job.units = units;
    job.tool_diameter = p.tool_diameter;
    CncJob {
        params: job,
        kind: JobKind::Mill { paths },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{area, centered_rect};

    fn copper() -> MultiPolygon<f64> {
        MultiPolygon::new(vec![centered_rect(5.0, 5.0, 2.0, 2.0)])
    }

    #[test]
    fn region_is_board_minus_copper() {
        let cu = copper();
        let p = NccParams::default();

        let region = ncc_region(&cu, &p);
        let region_area = area(&region);

        // board: (2 + 2*margin) square = 4x4 = 16; copper = 2x2 = 4.
        let board_area = (2.0 + 2.0 * p.boundary_margin).powi(2);
        let copper_area = area(&cu);
        let expected = board_area - copper_area;

        assert!(region_area > 0.0, "region area must be positive");
        assert!(
            (region_area - expected).abs() < 1e-6,
            "region area {region_area} vs expected {expected}"
        );
    }

    #[test]
    fn empty_copper_yields_empty_region() {
        let empty = MultiPolygon::new(vec![]);
        let region = ncc_region(&empty, &NccParams::default());
        assert!(region.0.is_empty());
    }

    #[test]
    fn paths_are_non_empty() {
        let paths = ncc_paths(&copper(), &NccParams::default());
        assert!(!paths.is_empty(), "expected NCC tool-paths");
    }

    #[test]
    fn job_is_a_mill_job() {
        let job = ncc_job(&copper(), &NccParams::default(), Units::Mm);
        match job.kind {
            JobKind::Mill { paths } => assert!(!paths.is_empty()),
            other => panic!("expected a Mill job, got {other:?}"),
        }
    }
}
