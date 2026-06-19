//! Geometry -> DXF exporter for the FlatCAM Rust port.
//!
//! Counterpart to [`crate::parse`]. Emits a minimal but standards-correct ASCII
//! DXF document containing an `ENTITIES` section (plus the small HEADER/TABLES
//! boilerplate the `dxf` crate writes) terminated with `EOF`, so that the output
//! re-parses cleanly via [`crate::parse`].
//!
//! ## Entity mapping
//! - Each polygon **exterior ring** and each **interior ring** (hole) is written
//!   as a closed `LWPOLYLINE` (the closed flag, bit `1`, is set). Holes are
//!   emitted as independent closed polylines — DXF has no native ring/hole
//!   relationship, and this matches what FlatCAM's DXF export does (one closed
//!   polyline per ring).
//! - Each polyline is written as an open `LWPOLYLINE`. A two-point polyline is
//!   written as a `LINE` so it round-trips to exactly two points, matching the
//!   importer's LINE -> 2-point-polyline behaviour.
//!
//! Coordinates are passed through unchanged: DXF, like FlatCAM's internal
//! geometry, is Y-up, so no axis flip is applied (unlike SVG export).

use dxf::enums::AcadVersion;
use dxf::entities::{Entity, EntityType, Line, LwPolyline};
use dxf::{Drawing, LwPolylineVertex, Point};

use fc_geo::{LineString, MultiPolygon};

/// Options controlling DXF output.
#[derive(Debug, Clone)]
pub struct DxfWriteOptions {
    /// When `true`, a two-point polyline is emitted as a `LINE` entity instead
    /// of a two-vertex `LWPOLYLINE`. Both re-parse to a 2-point polyline.
    pub two_point_as_line: bool,
}

impl Default for DxfWriteOptions {
    fn default() -> Self {
        Self {
            two_point_as_line: true,
        }
    }
}

/// Build a DXF [`Drawing`] from the given geometry and serialize it to ASCII
/// DXF text using default options. See [`write_dxf_with`] for option control.
pub fn write_dxf(polygons: &MultiPolygon<f64>, polylines: &[LineString<f64>]) -> String {
    write_dxf_with(polygons, polylines, &DxfWriteOptions::default())
}

/// Like [`write_dxf`], with explicit [`DxfWriteOptions`].
pub fn write_dxf_with(
    polygons: &MultiPolygon<f64>,
    polylines: &[LineString<f64>],
    opts: &DxfWriteOptions,
) -> String {
    let mut drawing = Drawing::new();
    // LWPOLYLINE is only emitted for ACAD R13+; the default version is older,
    // which would silently drop every polyline. Pin to R2000 so closed/open
    // LWPOLYLINE entities are actually written (and thus re-parseable).
    drawing.header.version = AcadVersion::R2000;

    // Polygons: one closed LWPOLYLINE per ring (exterior + each hole).
    for poly in &polygons.0 {
        add_closed_ring(&mut drawing, poly.exterior());
        for hole in poly.interiors() {
            add_closed_ring(&mut drawing, hole);
        }
    }

    // Open polylines.
    for ls in polylines {
        let pts = &ls.0;
        if pts.len() < 2 {
            continue;
        }
        if pts.len() == 2 && opts.two_point_as_line {
            drawing.add_entity(Entity::new(EntityType::Line(Line {
                p1: Point::new(pts[0].x, pts[0].y, 0.0),
                p2: Point::new(pts[1].x, pts[1].y, 0.0),
                ..Default::default()
            })));
        } else {
            add_lwpolyline(&mut drawing, &collect_vertices(pts), false);
        }
    }

    serialize(&drawing)
}

/// Append a closed LWPOLYLINE for a ring. The importer re-closes rings itself,
/// so we drop a duplicated closing vertex (first == last) to avoid a redundant
/// point — the closed flag conveys closure.
fn add_closed_ring(drawing: &mut Drawing, ring: &LineString<f64>) {
    let mut pts: Vec<LwPolylineVertex> = collect_vertices(&ring.0);
    if pts.len() >= 2 {
        let first = (pts[0].x, pts[0].y);
        let last = (pts[pts.len() - 1].x, pts[pts.len() - 1].y);
        if (first.0 - last.0).abs() < 1e-12 && (first.1 - last.1).abs() < 1e-12 {
            pts.pop();
        }
    }
    if pts.len() >= 3 {
        add_lwpolyline(drawing, &pts, true);
    }
}

fn collect_vertices(coords: &[fc_geo::Coord<f64>]) -> Vec<LwPolylineVertex> {
    coords
        .iter()
        .map(|c| LwPolylineVertex {
            x: c.x,
            y: c.y,
            ..Default::default()
        })
        .collect()
}

fn add_lwpolyline(drawing: &mut Drawing, vertices: &[LwPolylineVertex], closed: bool) {
    let mut lw = LwPolyline {
        vertices: vertices.to_vec(),
        ..Default::default()
    };
    if closed {
        lw.flags |= 1;
    }
    drawing.add_entity(Entity::new(EntityType::LwPolyline(lw)));
}

fn serialize(drawing: &Drawing) -> String {
    let mut buf = Vec::new();
    drawing
        .save(&mut buf)
        .expect("writing DXF to an in-memory buffer cannot fail");
    String::from_utf8(buf).expect("dxf crate emits ASCII/UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use fc_geo::{Coord, LineString, Polygon};

    fn square_with_hole() -> Polygon<f64> {
        let exterior = LineString::new(vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 10.0, y: 0.0 },
            Coord { x: 10.0, y: 10.0 },
            Coord { x: 0.0, y: 10.0 },
            Coord { x: 0.0, y: 0.0 },
        ]);
        let hole = LineString::new(vec![
            Coord { x: 3.0, y: 3.0 },
            Coord { x: 7.0, y: 3.0 },
            Coord { x: 7.0, y: 7.0 },
            Coord { x: 3.0, y: 7.0 },
            Coord { x: 3.0, y: 3.0 },
        ]);
        Polygon::new(exterior, vec![hole])
    }

    #[test]
    fn empty_geometry_produces_valid_document() {
        let dxf = write_dxf(&MultiPolygon::new(vec![]), &[]);
        assert!(dxf.contains("EOF"));
        let doc = parse(&dxf).unwrap();
        assert!(doc.polygons.0.is_empty());
        assert!(doc.polylines.is_empty());
    }

    #[test]
    fn square_with_hole_round_trips() {
        // Exterior + hole -> two closed rings -> two closed polygons on re-parse.
        let mp = MultiPolygon::new(vec![square_with_hole()]);
        let dxf = write_dxf(&mp, &[]);
        let doc = parse(&dxf).unwrap();
        assert_eq!(doc.polygons.0.len(), 2, "exterior + hole = 2 closed rings");

        let total: f64 = fc_geo::area(&doc.polygons);
        // 10x10 outer ring (100) + 4x4 inner ring (16), both as filled rings.
        assert!((total - 116.0).abs() < 1e-6, "area was {total}");
    }

    #[test]
    fn line_round_trips_to_two_points() {
        let ls = LineString::new(vec![
            Coord { x: 1.0, y: 2.0 },
            Coord { x: 5.0, y: 8.0 },
        ]);
        let dxf = write_dxf(&MultiPolygon::new(vec![]), &[ls]);
        let doc = parse(&dxf).unwrap();
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(doc.polylines[0].0.len(), 2);
        let p = &doc.polylines[0].0;
        assert!((p[0].x - 1.0).abs() < 1e-9 && (p[0].y - 2.0).abs() < 1e-9);
        assert!((p[1].x - 5.0).abs() < 1e-9 && (p[1].y - 8.0).abs() < 1e-9);
    }

    #[test]
    fn open_polyline_round_trips() {
        let ls = LineString::new(vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 4.0, y: 0.0 },
            Coord { x: 4.0, y: 3.0 },
        ]);
        let dxf = write_dxf(&MultiPolygon::new(vec![]), &[ls]);
        let doc = parse(&dxf).unwrap();
        assert!(doc.polygons.0.is_empty());
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(doc.polylines[0].0.len(), 3);
    }

    #[test]
    fn multiple_polygons_round_trip() {
        let a = Polygon::new(
            LineString::new(vec![
                Coord { x: 0.0, y: 0.0 },
                Coord { x: 2.0, y: 0.0 },
                Coord { x: 2.0, y: 2.0 },
                Coord { x: 0.0, y: 2.0 },
                Coord { x: 0.0, y: 0.0 },
            ]),
            vec![],
        );
        let b = Polygon::new(
            LineString::new(vec![
                Coord { x: 5.0, y: 5.0 },
                Coord { x: 8.0, y: 5.0 },
                Coord { x: 8.0, y: 8.0 },
                Coord { x: 5.0, y: 8.0 },
                Coord { x: 5.0, y: 5.0 },
            ]),
            vec![],
        );
        let mp = MultiPolygon::new(vec![a, b]);
        let dxf = write_dxf(&mp, &[]);
        let doc = parse(&dxf).unwrap();
        assert_eq!(doc.polygons.0.len(), 2);
        let total = fc_geo::area(&doc.polygons);
        assert!((total - (4.0 + 9.0)).abs() < 1e-6, "area was {total}");
    }
}
