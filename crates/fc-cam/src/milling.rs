//! Profile & pocket milling of a geometry (port of `ToolMilling`'s core).
//!
//! Two operations are supported:
//!  - **Profile**: trace the boundary of a geometry, offset inward or outward by
//!    the tool radius, so the finished part keeps its nominal size (cut outside)
//!    or the slot/aperture keeps its size (cut inside).
//!  - **Pocket**: clear the whole interior of a geometry by delegating to the
//!    paint/infill core ([`crate::paint::paint_region`]) with a boundary contour.
//!
//! Both yield abstract [`Polyline`]s that can be wrapped into a milling
//! [`CncJob`] via [`milling_job`].

use fc_gcode::{CncJob, JobKind, JobParams, Polyline, Units};
use fc_geo::{offset, MultiPolygon};

/// Parameters for a milling operation.
#[derive(Clone, Debug)]
pub struct MillingParams {
    pub tool_diameter: f64,
    /// Fractional overlap between adjacent pocket passes (0.0..1.0).
    pub overlap: f64,
    /// Extra clearance kept from the region boundary when pocketing.
    pub margin: f64,
    /// Milling parameters carried into the generated job.
    pub job: JobParams,
}

impl Default for MillingParams {
    fn default() -> Self {
        MillingParams {
            tool_diameter: 0.8,
            overlap: 0.4,
            margin: 0.0,
            job: JobParams::default(),
        }
    }
}

/// Extract every ring (exterior + interiors) of a multipolygon as a polyline.
fn ring_polylines(mp: &MultiPolygon<f64>) -> Vec<Polyline> {
    let mut o = Vec::new();
    for p in &mp.0 {
        o.push(p.exterior().coords().map(|c| (c.x, c.y)).collect());
        for h in p.interiors() {
            o.push(h.coords().map(|c| (c.x, c.y)).collect());
        }
    }
    o
}

/// Generate profile (boundary) milling paths for a geometry.
///
/// When `outside` is true the boundary is offset outward by the tool radius so
/// the finished part keeps its nominal size; otherwise it is offset inward.
pub fn milling_profile(geo: &MultiPolygon<f64>, tool_diameter: f64, outside: bool) -> Vec<Polyline> {
    let d = if outside {
        tool_diameter / 2.0
    } else {
        -tool_diameter / 2.0
    };
    let off = offset(geo, d);
    ring_polylines(&off)
}

/// Generate pocket (interior clearing) milling paths for a geometry.
///
/// Delegates to the paint/infill core, always adding a boundary contour pass.
pub fn milling_pocket(geo: &MultiPolygon<f64>, p: &MillingParams) -> Vec<Polyline> {
    let pp = crate::paint::PaintParams {
        tool_diameter: p.tool_diameter,
        overlap: p.overlap,
        margin: p.margin,
        add_contour: true,
        job: p.job.clone(),
    };
    crate::paint::paint_region(geo, &pp)
}

/// Build a milling [`CncJob`] from pre-computed tool-paths, in the given units.
pub fn milling_job(
    _geo: &MultiPolygon<f64>,
    paths: Vec<Polyline>,
    p: &MillingParams,
    units: Units,
) -> CncJob {
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
    use fc_geo::{bounds, centered_rect};

    #[test]
    fn profile_outside_grows_bbox() {
        // 10x10 square centred at (5,5); profile on the outside with a 1mm tool.
        let geo = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let paths = milling_profile(&geo, 1.0, true);
        assert!(!paths.is_empty(), "profile should yield >=1 ring");

        let src = bounds(&geo).unwrap();
        // rebuild a multipolygon from the offset to measure its bbox
        let off = offset(&geo, 0.5);
        let ob = bounds(&off).unwrap();
        assert!(ob.0 < src.0, "minx should shrink (grow outward)");
        assert!(ob.1 < src.1, "miny should shrink (grow outward)");
        assert!(ob.2 > src.2, "maxx should grow");
        assert!(ob.3 > src.3, "maxy should grow");
    }

    #[test]
    fn pocket_yields_paths() {
        let geo = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let p = MillingParams {
            tool_diameter: 1.0,
            overlap: 0.0,
            margin: 0.0,
            ..MillingParams::default()
        };
        let paths = milling_pocket(&geo, &p);
        assert!(!paths.is_empty(), "pocket should clear the interior");
    }

    #[test]
    fn job_is_a_mill_job() {
        let geo = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let p = MillingParams::default();
        let paths = milling_profile(&geo, p.tool_diameter, true);
        let job = milling_job(&geo, paths, &p, Units::Mm);
        match &job.kind {
            JobKind::Mill { paths } => assert!(!paths.is_empty()),
            _ => panic!("expected a mill job"),
        }
        assert!((job.params.tool_diameter - p.tool_diameter).abs() < 1e-9);
    }
}
