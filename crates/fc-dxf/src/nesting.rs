//! Ring-nesting reconstruction.
//!
//! Importers collect every closed subpath/shape as a standalone ring. To make
//! round-trips faithful, a square-with-a-hole must come back as **one** polygon
//! with an interior ring (area 100 − 16 = 84), not two separate filled polygons
//! (100 + 16). This module rebuilds that exterior/hole nesting from a flat list
//! of rings.
//!
//! ## Algorithm
//!
//! 1. Drop degenerate (zero-area) rings.
//! 2. Sort rings by unsigned area **descending**, so any container is processed
//!    before the rings it contains.
//! 3. For each ring, its immediate parent is the *smallest already-placed ring
//!    that contains it* (containment via a representative-point-in-polygon test,
//!    not winding — input winding is not guaranteed). Depth = parent depth + 1.
//! 4. Even depth (0, 2, …) ⇒ a filled exterior (new polygon). Odd depth ⇒ a hole
//!    of its parent exterior. A filled island sitting inside a hole nests as a
//!    fresh exterior again, giving correct multi-level islands.
//! 5. Assemble the `MultiPolygon`.

use fc_geo::{LineString, MultiPolygon, Polygon};
use geo::{Area, Contains};

/// Reconstruct exterior/hole nesting from a flat list of closed rings.
pub(crate) fn nest_rings(rings: Vec<LineString<f64>>) -> MultiPolygon<f64> {
    // Wrap each ring in a hole-less polygon so we can reuse `geo`'s area and
    // point-in-polygon (`Contains`) implementations.
    let mut items: Vec<RingItem> = rings
        .into_iter()
        .filter_map(|ring| {
            let poly = Polygon::new(ring, vec![]);
            let area = poly.unsigned_area();
            if area <= f64::EPSILON {
                return None; // skip degenerate / collinear rings
            }
            let rep = representative_point(&poly)?;
            Some(RingItem { poly, area, rep })
        })
        .collect();

    // Largest first: a parent always precedes its children.
    items.sort_by(|a, b| b.area.partial_cmp(&a.area).unwrap_or(std::cmp::Ordering::Equal));

    // Per-ring depth and the index of the exterior polygon it belongs to.
    let mut depth: Vec<usize> = vec![0; items.len()];
    // `exterior_slot[i]` = index into `result` for ring `i` if it is itself an
    // exterior; holes attach to their parent's slot.
    let mut exterior_slot: Vec<Option<usize>> = vec![None; items.len()];
    let mut result: Vec<Polygon<f64>> = Vec::new();

    for i in 0..items.len() {
        // Immediate parent = smallest already-placed ring (j < i, hence area >=)
        // whose polygon contains ring i's representative point.
        let mut parent: Option<usize> = None;
        for j in 0..i {
            if items[j].poly.contains(&items[i].rep) {
                // Smallest container wins; items are area-descending, so a later
                // j is smaller-or-equal — keep updating to the last match.
                parent = Some(j);
            }
        }

        let d = match parent {
            Some(p) => depth[p] + 1,
            None => 0,
        };
        depth[i] = d;

        if d % 2 == 0 {
            // Filled exterior — start a fresh polygon.
            let ring = items[i].poly.exterior().clone();
            exterior_slot[i] = Some(result.len());
            result.push(Polygon::new(ring, vec![]));
        } else {
            // Hole — attach to the exterior polygon of its parent.
            let parent = parent.expect("odd depth implies a parent");
            let slot = exterior_slot[parent]
                .expect("parent at even depth is an exterior with a slot");
            let hole = items[i].poly.exterior().clone();
            let (ext, holes) = std::mem::replace(
                &mut result[slot],
                Polygon::new(LineString::new(vec![]), vec![]),
            )
            .into_inner();
            let mut holes = holes;
            holes.push(hole);
            result[slot] = Polygon::new(ext, holes);
        }
    }

    MultiPolygon::new(result)
}

struct RingItem {
    poly: Polygon<f64>,
    area: f64,
    rep: geo::Point<f64>,
}

/// A point guaranteed to lie strictly inside the polygon's exterior, suitable
/// for an unambiguous point-in-polygon containment test. We try the centroid
/// first (correct for convex rings and most concave ones); if the centroid
/// happens to fall outside a concave ring, fall back to a triangle-fan
/// midpoint that is interior by construction.
fn representative_point(poly: &Polygon<f64>) -> Option<geo::Point<f64>> {
    use geo::Centroid;
    if let Some(c) = poly.centroid() {
        if poly.contains(&c) {
            return Some(c);
        }
    }
    // Fallback: midpoint of the first vertex and a non-adjacent vertex tends to
    // be interior for simple rings; scan triangle centroids of the fan until one
    // lands inside.
    let pts = &poly.exterior().0;
    if pts.len() < 3 {
        return None;
    }
    let a = pts[0];
    for w in 1..pts.len() - 1 {
        let b = pts[w];
        let c = pts[w + 1];
        let cx = (a.x + b.x + c.x) / 3.0;
        let cy = (a.y + b.y + c.y) / 3.0;
        let p = geo::Point::new(cx, cy);
        if poly.contains(&p) {
            return Some(p);
        }
    }
    // Last resort: any vertex (boundary). Shouldn't normally be reached.
    Some(geo::Point::new(a.x, a.y))
}
