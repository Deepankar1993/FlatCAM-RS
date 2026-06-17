//! Board cutout / outline milling with holding tabs (port of `ToolCutOut`'s core).
//!
//! Given a board outline, generate the tool-paths that mill the board free from
//! its panel, leaving a number of uncut "holding tabs" (gaps) so the piece stays
//! attached until snapped out. The outline can be cut on the line or on the
//! outside (offset outward by the tool radius so the finished board keeps its
//! nominal size). Each ring of the outline geometry becomes a closed loop that
//! is then broken into open cut arcs by removing `tabs` evenly spaced gaps of
//! `tab_gap` length.

use fc_gcode::{CncJob, JobKind, JobParams, Polyline, Units};
use fc_geo::{centered_rect, offset, MultiPolygon};

/// Parameters for a board cutout operation.
#[derive(Clone, Debug)]
pub struct CutoutParams {
    pub tool_diameter: f64,
    /// number of holding tabs (uncut gaps) distributed around each ring
    pub tabs: usize,
    /// length of each tab gap (in document units)
    pub tab_gap: f64,
    /// true => cut on the OUTSIDE of the outline (offset outward by tool radius);
    /// false => cut on the line
    pub outside: bool,
    pub job: JobParams,
}

impl Default for CutoutParams {
    fn default() -> Self {
        CutoutParams {
            tool_diameter: 1.0,
            tabs: 4,
            tab_gap: 2.0,
            outside: true,
            job: JobParams::default(),
        }
    }
}

/// Generate cutout tool-paths (polylines) for an outline.
///
/// When [`CutoutParams::outside`] is set the geometry is first grown outward by
/// the tool radius. Every ring (exterior + interiors) of the resulting geometry
/// is turned into a closed loop and then split into open cut arcs by removing
/// `tabs` evenly spaced gaps of about `tab_gap` length each. With `tabs == 0`
/// the whole closed ring is emitted as a single polyline.
pub fn cutout_geometry(outline: &MultiPolygon<f64>, p: &CutoutParams) -> Vec<Polyline> {
    let cut = if p.outside {
        offset(outline, p.tool_diameter / 2.0)
    } else {
        outline.clone()
    };

    let mut paths: Vec<Polyline> = Vec::new();
    for poly in &cut.0 {
        process_ring(ring_coords(poly.exterior()), p, &mut paths);
        for hole in poly.interiors() {
            process_ring(ring_coords(hole), p, &mut paths);
        }
    }
    paths
}

/// Build a rectangular cutout from a bounding box and emit its tool-paths.
pub fn cutout_rectangular(
    minx: f64,
    miny: f64,
    maxx: f64,
    maxy: f64,
    p: &CutoutParams,
) -> Vec<Polyline> {
    let cx = (minx + maxx) / 2.0;
    let cy = (miny + maxy) / 2.0;
    let w = maxx - minx;
    let h = maxy - miny;
    let rect = MultiPolygon::new(vec![centered_rect(cx, cy, w, h)]);
    cutout_geometry(&rect, p)
}

/// Build a cutout [`CncJob`] from an outline, in the given document units.
pub fn cutout_job(outline: &MultiPolygon<f64>, p: &CutoutParams, units: Units) -> CncJob {
    let paths = cutout_geometry(outline, p);
    let mut job = p.job.clone();
    job.units = units;
    job.tool_diameter = p.tool_diameter;
    CncJob {
        params: job,
        kind: JobKind::Mill { paths },
    }
}

fn ring_coords(ls: &fc_geo::LineString<f64>) -> Polyline {
    ls.coords().map(|c| (c.x, c.y)).collect()
}

/// Distance between two points.
fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    (dx * dx + dy * dy).sqrt()
}

/// Subdivide a closed ring so that no segment exceeds `max_seg` in length.
fn densify(ring: &Polyline, max_seg: f64) -> Polyline {
    let mut out: Polyline = Vec::new();
    for w in ring.windows(2) {
        let (a, b) = (w[0], w[1]);
        out.push(a);
        let d = dist(a, b);
        if d > max_seg {
            let n = (d / max_seg).ceil() as usize;
            for k in 1..n {
                let t = k as f64 / n as f64;
                out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
        }
    }
    if let Some(&last) = ring.last() {
        out.push(last);
    }
    out
}

/// Break one closed ring into cut arcs and append them to `out`.
fn process_ring(mut ring: Polyline, p: &CutoutParams, out: &mut Vec<Polyline>) {
    if ring.len() < 2 {
        return;
    }
    // Ensure the ring is explicitly closed so the full perimeter is walked.
    if ring.first() != ring.last() {
        let first = ring[0];
        ring.push(first);
    }

    if p.tabs == 0 {
        out.push(ring);
        return;
    }

    // Total perimeter and evenly spaced tab centres along it.
    let mut perimeter = 0.0;
    for w in ring.windows(2) {
        perimeter += dist(w[0], w[1]);
    }
    if perimeter <= 0.0 {
        out.push(ring);
        return;
    }

    // Densify: outlines are often just a handful of long edges (a rectangle has
    // four). Without intermediate points the gap test is only sampled at the
    // corners, so tabs placed mid-edge would never register. Insert points so no
    // segment is longer than a fraction of the tab gap.
    let max_seg = (p.tab_gap / 4.0).max(0.25);
    let ring = densify(&ring, max_seg);

    // Place tab centres at edge midpoints (i + 0.5) so they don't coincide with
    // corner vertices of a rectangular outline.
    let tab_centers: Vec<f64> = (0..p.tabs)
        .map(|i| (i as f64 + 0.5) * perimeter / (p.tabs as f64))
        .collect();
    let half_gap = p.tab_gap / 2.0;

    // Walk the ring, accumulating distance; emit cut segments only where the
    // current position is outside every tab gap. A new open polyline starts
    // whenever we cross out of a gap.
    let in_gap = |s: f64| -> bool {
        tab_centers.iter().any(|&c| {
            // distance along the (cyclic) perimeter to the tab centre
            let d = (s - c).abs();
            let d = d.min(perimeter - d);
            d <= half_gap
        })
    };

    let mut segments: Vec<Polyline> = Vec::new();
    let mut current: Polyline = Vec::new();
    let mut s = 0.0;

    // Emit the first vertex if it is not inside a tab gap.
    if !in_gap(s) {
        current.push(ring[0]);
    }
    for w in ring.windows(2) {
        s += dist(w[0], w[1]);
        if in_gap(s) {
            // Reached a tab gap: finish the current arc if it has content.
            if current.len() >= 2 {
                segments.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        } else {
            // Outside any gap: continue (or start) the current arc. Anchor a
            // fresh arc at the previous vertex so it has a leading point even
            // when it begins mid-segment leaving a gap.
            if current.is_empty() {
                current.push(w[0]);
            }
            current.push(w[1]);
        }
    }
    if current.len() >= 2 {
        segments.push(current);
    }

    // De-duplicate consecutive identical points within each arc.
    for mut seg in segments {
        seg.dedup_by(|a, b| dist(*a, *b) < 1e-12);
        if seg.len() >= 2 {
            out.push(seg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{centered_rect, MultiPolygon};

    fn square_20() -> MultiPolygon<f64> {
        MultiPolygon::new(vec![centered_rect(10.0, 10.0, 20.0, 20.0)])
    }

    #[test]
    fn cutout_with_tabs_produces_arcs() {
        let outline = square_20();
        let p = CutoutParams {
            tool_diameter: 1.0,
            tabs: 4,
            tab_gap: 2.0,
            outside: false,
            job: JobParams::default(),
        };
        let paths = cutout_geometry(&outline, &p);
        assert!(
            paths.len() >= 4,
            "4 tab gaps should yield >=4 cut arcs, got {}",
            paths.len()
        );

        // The full closed ring of a 20x20 square has perimeter 80; none of the
        // emitted arcs should be the complete closed ring.
        for arc in &paths {
            let is_closed = arc.len() >= 4 && arc.first() == arc.last();
            assert!(!is_closed, "no arc should be the full closed ring");
        }
    }

    #[test]
    fn cutout_no_tabs_is_one_closed_ring() {
        let outline = square_20();
        let p = CutoutParams {
            tabs: 0,
            outside: false,
            ..Default::default()
        };
        let paths = cutout_geometry(&outline, &p);
        assert_eq!(paths.len(), 1, "one ring => one polyline when tabs==0");
        let ring = &paths[0];
        assert!(
            ring.first() == ring.last() && ring.len() >= 4,
            "tabs==0 should emit the full closed ring"
        );
    }

    #[test]
    fn rectangular_cutout_matches_geometry() {
        let p = CutoutParams {
            tabs: 4,
            outside: false,
            ..Default::default()
        };
        let paths = cutout_rectangular(0.0, 0.0, 20.0, 20.0, &p);
        assert!(paths.len() >= 4);
    }

    #[test]
    fn cutout_job_is_a_mill_job() {
        let outline = square_20();
        let p = CutoutParams::default();
        let job = cutout_job(&outline, &p, Units::Mm);
        assert!((job.params.tool_diameter - p.tool_diameter).abs() < 1e-9);
        match job.kind {
            JobKind::Mill { paths } => assert!(!paths.is_empty()),
            _ => panic!("expected a Mill job"),
        }
    }
}
