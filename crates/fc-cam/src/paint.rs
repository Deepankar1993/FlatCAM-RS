//! Area painting / pocket infill (port of `ToolPaint`'s line-fill core).
//!
//! Given a region polygon and a tool, generate a set of parallel scanline
//! tool-paths that clear the interior, plus optional boundary contour passes.
//! The region is first inset by `tool_radius + margin` so the cutter stays
//! inside, then filled with horizontal lines spaced by `tool_dia·(1−overlap)`,
//! clipped to the region with the even-odd rule (so holes are respected).
//! Alternate scanlines reverse direction to minimise rapid travel (zig-zag).

use fc_gcode::{CncJob, JobKind, JobParams, Polyline};
use fc_geo::{bounds, offset, MultiPolygon};

/// Parameters for a paint/infill operation.
#[derive(Clone, Debug)]
pub struct PaintParams {
    pub tool_diameter: f64,
    /// Overlap fraction between adjacent passes (0.0..1.0).
    pub overlap: f64,
    /// Extra clearance kept from the region boundary.
    pub margin: f64,
    /// Add a contour pass tracing the inset boundary after the infill.
    pub add_contour: bool,
    pub job: JobParams,
}

impl Default for PaintParams {
    fn default() -> Self {
        PaintParams {
            tool_diameter: 0.5,
            overlap: 0.2,
            margin: 0.0,
            add_contour: true,
            job: JobParams::default(),
        }
    }
}

/// Generate paint tool-paths (polylines) for a region.
pub fn paint_region(region: &MultiPolygon<f64>, p: &PaintParams) -> Vec<Polyline> {
    let inset = p.tool_diameter / 2.0 + p.margin;
    let inner = offset(region, -inset);
    if inner.0.is_empty() {
        return vec![];
    }
    let Some((minx, miny, maxx, maxy)) = bounds(&inner) else {
        return vec![];
    };
    let step = (p.tool_diameter * (1.0 - p.overlap.clamp(0.0, 0.999))).max(1e-6);

    let rings = extract_rings(&inner);

    // Enumerate scanline rows first (cheap), then compute spans per row in
    // parallel. Using the row index `k` keeps the zig-zag direction (flip on
    // odd rows) and output order identical to the sequential version.
    use rayon::prelude::*;
    let mut ys: Vec<(usize, f64)> = Vec::new();
    let mut y = miny + step * 0.5;
    let mut k = 0usize;
    while y < maxy {
        ys.push((k, y));
        k += 1;
        y += step;
    }
    let mut paths: Vec<Polyline> = ys
        .par_iter()
        .flat_map_iter(|&(k, y)| {
            let mut xs = scanline_x(&rings, y);
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let flip = k % 2 == 1;
            let mut row: Vec<Polyline> = Vec::new();
            for span in xs.chunks(2) {
                if span.len() == 2 {
                    let (mut a, mut b) = (span[0], span[1]);
                    if (b - a).abs() < 1e-9 {
                        continue;
                    }
                    if flip {
                        std::mem::swap(&mut a, &mut b);
                    }
                    row.push(vec![(a, y), (b, y)]);
                }
            }
            row
        })
        .collect();

    let _ = (minx, maxx);

    if p.add_contour {
        for ring in &rings {
            let pl: Polyline = ring.iter().map(|&(x, y)| (x, y)).collect();
            if pl.len() >= 2 {
                paths.push(pl);
            }
        }
    }
    paths
}

/// Build a paint [`CncJob`] for a region, in the given document units.
pub fn paint_job(region: &MultiPolygon<f64>, p: &PaintParams, units: fc_gcode::Units) -> CncJob {
    let paths = paint_region(region, p);
    let mut job = p.job.clone();
    job.units = units;
    job.tool_diameter = p.tool_diameter;
    CncJob {
        params: job,
        kind: JobKind::Mill { paths },
    }
}

type Ring = Vec<(f64, f64)>;

fn extract_rings(mp: &MultiPolygon<f64>) -> Vec<Ring> {
    let mut rings = Vec::new();
    for poly in &mp.0 {
        rings.push(poly.exterior().coords().map(|c| (c.x, c.y)).collect());
        for hole in poly.interiors() {
            rings.push(hole.coords().map(|c| (c.x, c.y)).collect());
        }
    }
    rings
}

/// X coordinates where the horizontal line `y` crosses any ring edge.
/// Uses the half-open `[min_y, max_y)` rule so shared vertices aren't double
/// counted, yielding correct even-odd inside spans.
fn scanline_x(rings: &[Ring], y: f64) -> Vec<f64> {
    let mut xs = Vec::new();
    for ring in rings {
        for e in ring.windows(2) {
            let (x1, y1) = e[0];
            let (x2, y2) = e[1];
            let (ylo, yhi) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
            if y >= ylo && y < yhi {
                let t = (y - y1) / (y2 - y1);
                xs.push(x1 + t * (x2 - x1));
            }
        }
    }
    xs
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::centered_rect;

    #[test]
    fn paints_a_square() {
        // 10x10 square, 1mm tool, 0 overlap, no margin.
        let region = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let p = PaintParams {
            tool_diameter: 1.0,
            overlap: 0.0,
            margin: 0.0,
            add_contour: false,
            job: JobParams::default(),
        };
        let paths = paint_region(&region, &p);
        // inner region is ~9x9 (inset 0.5 each side); step 1mm => ~9 scanlines
        assert!(paths.len() >= 8 && paths.len() <= 10, "scanlines: {}", paths.len());
        // each scanline should span ~9mm
        let w = (paths[0][1].0 - paths[0][0].0).abs();
        assert!((w - 9.0).abs() < 0.2, "span width {w}");
    }

    #[test]
    fn zigzag_alternates_direction() {
        let region = MultiPolygon::new(vec![centered_rect(5.0, 5.0, 10.0, 10.0)]);
        let p = PaintParams { add_contour: false, ..PaintParams::default() };
        let paths = paint_region(&region, &p);
        // consecutive scanlines run opposite directions
        let d0 = paths[0][1].0 - paths[0][0].0;
        let d1 = paths[1][1].0 - paths[1][0].0;
        assert!(d0 * d1 < 0.0, "expected alternating direction");
    }

    #[test]
    fn respects_a_hole() {
        // square with a square hole -> scanlines crossing the hole split in two
        let outer = MultiPolygon::new(vec![centered_rect(10.0, 10.0, 20.0, 20.0)]);
        let hole = MultiPolygon::new(vec![centered_rect(10.0, 10.0, 6.0, 6.0)]);
        let region = fc_geo::difference(&outer, &hole);
        let p = PaintParams {
            tool_diameter: 1.0,
            overlap: 0.0,
            margin: 0.0,
            add_contour: false,
            job: JobParams::default(),
        };
        let paths = paint_region(&region, &p);
        // at least some scanlines must be split into two spans by the hole
        let same_y: Vec<_> = paths
            .iter()
            .filter(|p| (p[0].1 - 10.0).abs() < 1.0)
            .collect();
        assert!(same_y.len() >= 2, "hole should split mid scanlines into 2 spans");
    }
}
