//! Ear-cutting triangulation of polygons.
//!
//! GPU/preview rendering and area-based reporting need polygons broken into
//! triangles. This wraps `geo`'s earcut implementation (the Rust analogue of the
//! tessellation FlatCAM gets implicitly through VisPy's mesh building) and
//! flattens the result into plain coordinate triples.

use crate::MultiPolygon;
use geo::TriangulateEarcut;

/// Triangulate every polygon in `mp` and concatenate the results.
///
/// Each polygon's exterior and holes are tessellated with ear-cutting; holes are
/// handled by the earcut routine, so the returned triangles cover only the solid
/// (filled) area of the multipolygon. Each triangle is returned as its three
/// `(x, y)` vertices.
pub fn triangulate(mp: &MultiPolygon<f64>) -> Vec<[(f64, f64); 3]> {
    let mut out: Vec<[(f64, f64); 3]> = Vec::new();
    for poly in &mp.0 {
        for tri in poly.earcut_triangles() {
            let (a, b, c) = (tri.v1(), tri.v2(), tri.v3());
            out.push([(a.x, a.y), (b.x, b.y), (c.x, c.y)]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Coord, LineString, Polygon};

    /// Sum the unsigned areas of a list of triangle vertex triples.
    fn sum_triangle_areas(tris: &[[(f64, f64); 3]]) -> f64 {
        tris.iter()
            .map(|t| {
                let (ax, ay) = t[0];
                let (bx, by) = t[1];
                let (cx, cy) = t[2];
                ((bx - ax) * (cy - ay) - (cx - ax) * (by - ay)).abs() / 2.0
            })
            .sum()
    }

    fn square(min: f64, max: f64) -> Polygon<f64> {
        let ring = vec![
            Coord { x: min, y: min },
            Coord { x: max, y: min },
            Coord { x: max, y: max },
            Coord { x: min, y: max },
            Coord { x: min, y: min },
        ];
        Polygon::new(LineString::new(ring), vec![])
    }

    #[test]
    fn square_triangulates_into_two_triangles_of_total_area_four() {
        let mp = MultiPolygon::new(vec![square(0.0, 2.0)]);
        let tris = triangulate(&mp);
        assert_eq!(tris.len(), 2, "a quad should yield two triangles");
        assert!(
            (sum_triangle_areas(&tris) - 4.0).abs() < 1e-9,
            "total area was {}",
            sum_triangle_areas(&tris)
        );
    }

    #[test]
    fn empty_multipolygon_yields_no_triangles() {
        let mp = MultiPolygon::new(vec![]);
        assert!(triangulate(&mp).is_empty());
    }

    #[test]
    fn polygon_with_hole_has_area_outer_minus_hole() {
        let outer = vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 4.0, y: 0.0 },
            Coord { x: 4.0, y: 4.0 },
            Coord { x: 0.0, y: 4.0 },
            Coord { x: 0.0, y: 0.0 },
        ];
        let hole = vec![
            Coord { x: 1.0, y: 1.0 },
            Coord { x: 3.0, y: 1.0 },
            Coord { x: 3.0, y: 3.0 },
            Coord { x: 1.0, y: 3.0 },
            Coord { x: 1.0, y: 1.0 },
        ];
        let poly = Polygon::new(LineString::new(outer), vec![LineString::new(hole)]);
        let mp = MultiPolygon::new(vec![poly]);
        let tris = triangulate(&mp);
        // 4x4 outer (16) minus 2x2 hole (4) == 12.
        assert!(
            (sum_triangle_areas(&tris) - 12.0).abs() < 1e-6,
            "total area was {}",
            sum_triangle_areas(&tris)
        );
    }
}
