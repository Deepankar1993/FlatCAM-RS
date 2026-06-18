//! DXF -> geometry importer for the FlatCAM Rust port.
//!
//! Parses a DXF document (via the `dxf` crate) into FlatCAM geometry types:
//! closed shapes become polygons, open paths become polylines.
//!
//! ## v1 limitations
//! - LwPolyline/Polyline bulge values are ignored: arc segments inside a
//!   polyline are treated as straight segments between vertices.
//! - Only LINE, CIRCLE, ARC, LWPOLYLINE and POLYLINE entities are imported;
//!   all other entity types are silently skipped.
//! - 3D Z coordinates are dropped (geometry is projected onto the XY plane).

use std::f64::consts::PI;

use dxf::entities::*;
use dxf::Drawing;

use fc_geo::{Coord, LineString, MultiPolygon, Polygon};

/// Number of segments used when flattening a full circle.
const CIRCLE_SEGMENTS: usize = 48;

/// Parsed DXF document split into closed and open geometry.
#[derive(Debug)]
pub struct DxfDoc {
    /// Closed shapes (closed polylines, circles).
    pub polygons: MultiPolygon<f64>,
    /// Open paths (lines, arcs, open polylines).
    pub polylines: Vec<LineString<f64>>,
}

/// Errors produced while parsing a DXF document.
#[derive(thiserror::Error, Debug)]
pub enum DxfError {
    #[error("dxf parse error: {0}")]
    Parse(String),
}

/// Parse DXF text into a [`DxfDoc`].
pub fn parse(text: &str) -> Result<DxfDoc, DxfError> {
    let bytes = text.as_bytes();
    let mut cursor = std::io::Cursor::new(bytes);
    let drawing = Drawing::load(&mut cursor).map_err(|e| DxfError::Parse(e.to_string()))?;

    let mut polygons: Vec<Polygon<f64>> = Vec::new();
    let mut polylines: Vec<LineString<f64>> = Vec::new();

    for e in drawing.entities() {
        match &e.specific {
            EntityType::Line(line) => {
                let pts = vec![
                    Coord {
                        x: line.p1.x,
                        y: line.p1.y,
                    },
                    Coord {
                        x: line.p2.x,
                        y: line.p2.y,
                    },
                ];
                polylines.push(LineString::new(pts));
            }
            EntityType::Circle(c) => {
                polygons.push(circle_polygon(c.center.x, c.center.y, c.radius));
            }
            EntityType::Arc(a) => {
                polylines.push(arc_polyline(
                    a.center.x,
                    a.center.y,
                    a.radius,
                    a.start_angle,
                    a.end_angle,
                ));
            }
            EntityType::LwPolyline(p) => {
                let pts: Vec<Coord<f64>> = p
                    .vertices
                    .iter()
                    .map(|v| Coord { x: v.x, y: v.y })
                    .collect();
                push_polyline(&mut polygons, &mut polylines, pts, p.flags & 1 != 0);
            }
            EntityType::Polyline(p) => {
                let pts: Vec<Coord<f64>> = p
                    .vertices()
                    .map(|v| Coord {
                        x: v.location.x,
                        y: v.location.y,
                    })
                    .collect();
                push_polyline(&mut polygons, &mut polylines, pts, p.flags & 1 != 0);
            }
            _ => {}
        }
    }

    Ok(DxfDoc {
        polygons: MultiPolygon::new(polygons),
        polylines,
    })
}

/// Build a closed polygon approximating a circle by sampling
/// [`CIRCLE_SEGMENTS`] points around its circumference.
fn circle_polygon(cx: f64, cy: f64, r: f64) -> Polygon<f64> {
    let mut pts: Vec<Coord<f64>> = Vec::with_capacity(CIRCLE_SEGMENTS + 1);
    for i in 0..CIRCLE_SEGMENTS {
        let theta = 2.0 * PI * (i as f64) / (CIRCLE_SEGMENTS as f64);
        pts.push(Coord {
            x: cx + r * theta.cos(),
            y: cy + r * theta.sin(),
        });
    }
    // Close the ring by repeating the first point.
    pts.push(pts[0]);
    Polygon::new(LineString::new(pts), vec![])
}

/// Flatten a CCW arc (angles in degrees) into an open polyline.
fn arc_polyline(cx: f64, cy: f64, r: f64, start_deg: f64, end_deg: f64) -> LineString<f64> {
    let start = start_deg.to_radians();
    let mut end = end_deg.to_radians();
    if end <= start {
        end += 2.0 * PI;
    }
    let sweep = end - start;
    let steps = ((sweep / (2.0 * PI) * CIRCLE_SEGMENTS as f64).ceil() as i64).max(2) as usize;

    let mut pts: Vec<Coord<f64>> = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let theta = start + sweep * (i as f64) / (steps as f64);
        pts.push(Coord {
            x: cx + r * theta.cos(),
            y: cy + r * theta.sin(),
        });
    }
    LineString::new(pts)
}

/// Route a collected sequence of polyline points into either the closed
/// polygon list or the open polyline list, depending on the closed flag.
fn push_polyline(
    polygons: &mut Vec<Polygon<f64>>,
    polylines: &mut Vec<LineString<f64>>,
    pts: Vec<Coord<f64>>,
    closed: bool,
) {
    if closed && pts.len() >= 3 {
        let mut ring = pts;
        // Ensure the ring is explicitly closed.
        if ring.first() != ring.last() {
            ring.push(ring[0]);
        }
        polygons.push(Polygon::new(LineString::new(ring), vec![]));
    } else if pts.len() >= 2 {
        polylines.push(LineString::new(pts));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dxf::entities::{Arc, Circle, Entity, EntityType, Line};
    use dxf::Point;

    /// Serialize an in-memory drawing to DXF text.
    fn drawing_to_text(drawing: &Drawing) -> String {
        let mut buf = Vec::new();
        drawing.save(&mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn circle_round_trips_to_closed_polygon() {
        let mut drawing = Drawing::new();
        drawing.add_entity(Entity::new(EntityType::Circle(Circle {
            center: Point::new(0.0, 0.0, 0.0),
            radius: 2.0,
            ..Default::default()
        })));
        let text = drawing_to_text(&drawing);

        let doc = parse(&text).unwrap();
        assert_eq!(doc.polygons.0.len(), 1);

        let area = fc_geo::area(&doc.polygons);
        let expected = PI * 4.0;
        assert!(
            (area - expected).abs() < 0.1,
            "circle area {area} not within tolerance of {expected}"
        );
    }

    #[test]
    fn line_yields_open_polyline_with_two_points() {
        let mut drawing = Drawing::new();
        drawing.add_entity(Entity::new(EntityType::Line(Line {
            p1: Point::new(0.0, 0.0, 0.0),
            p2: Point::new(10.0, 0.0, 0.0),
            ..Default::default()
        })));
        let text = drawing_to_text(&drawing);

        let doc = parse(&text).unwrap();
        assert!(doc.polygons.0.is_empty());
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(doc.polylines[0].0.len(), 2);
    }

    #[test]
    fn arc_yields_open_polyline_with_more_than_two_points() {
        let mut drawing = Drawing::new();
        drawing.add_entity(Entity::new(EntityType::Arc(Arc {
            center: Point::new(0.0, 0.0, 0.0),
            radius: 5.0,
            start_angle: 0.0,
            end_angle: 90.0,
            ..Default::default()
        })));
        let text = drawing_to_text(&drawing);

        let doc = parse(&text).unwrap();
        assert!(doc.polygons.0.is_empty());
        assert_eq!(doc.polylines.len(), 1);
        assert!(
            doc.polylines[0].0.len() > 2,
            "arc polyline should be flattened into more than 2 points"
        );
    }
}
