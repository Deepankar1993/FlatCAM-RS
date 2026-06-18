//! `fc-pdf` — a minimal PDF *vector* importer for FlatCAM Evo.
//!
//! This crate is the Rust analogue of `appParsers/ParsePDF.py`. It walks every
//! page of a PDF, decodes each page's content stream and interprets the
//! path-construction and path-painting operators, turning them into geometry:
//!
//! * closed (filled) subpaths become [`Polygon`]s collected in a
//!   [`MultiPolygon`];
//! * open (stroked) subpaths become standalone [`LineString`] polylines.
//!
//! # Scope / limitations (v1)
//!
//! * **The CTM and the rest of the graphics state are ignored.** PDF coordinate
//!   transforms (`cm`), clipping, text, shadings and images are not handled.
//!   Coordinates are taken verbatim from the operands, in PDF *user space*.
//! * PDF user space is **Y-up**, and we keep coordinates as-is (no flip) — the
//!   consumer is responsible for any axis convention it needs.
//! * Cubic Béziers (`c`/`v`/`y`) are flattened with a fixed segment count
//!   (see [`BEZIER_SEGMENTS`]); there is no adaptive/flatness-based subdivision.
//! * Even-odd vs nonzero winding is not distinguished — every painted, closed
//!   subpath simply becomes its own polygon.

use fc_geo::{Coord, LineString, MultiPolygon, Polygon};

/// Number of straight segments used to flatten a single cubic Bézier curve.
pub const BEZIER_SEGMENTS: usize = 12;

/// Error type for the PDF importer.
#[derive(thiserror::Error, Debug)]
pub enum PdfError {
    /// Any failure originating from `lopdf` (load, page content, decode) or
    /// from the importer itself, stringified for a stable public surface.
    #[error("pdf error: {0}")]
    Pdf(String),
}

/// The geometry recovered from a PDF document.
#[derive(Debug)]
pub struct PdfDoc {
    /// Filled (closed) subpaths.
    pub polygons: MultiPolygon<f64>,
    /// Stroked (open) subpaths.
    pub polylines: Vec<LineString<f64>>,
}

/// Parse a PDF from an in-memory byte slice.
///
/// Every page is interpreted independently and the results are merged into a
/// single [`PdfDoc`]. See the crate docs for the (substantial) list of
/// limitations.
pub fn parse(bytes: &[u8]) -> Result<PdfDoc, PdfError> {
    let doc = lopdf::Document::load_mem(bytes)
        .map_err(|e| PdfError::Pdf(format!("load: {e}")))?;

    let mut all_ops: Vec<lopdf::content::Operation> = Vec::new();

    for (_num, page_id) in doc.get_pages() {
        let data = doc
            .get_page_content(page_id)
            .map_err(|e| PdfError::Pdf(format!("page content: {e}")))?;
        let content = lopdf::content::Content::decode(&data)
            .map_err(|e| PdfError::Pdf(format!("decode: {e}")))?;
        all_ops.extend(content.operations);
    }

    Ok(interpret(&all_ops))
}

/// A subpath accumulated during interpretation: its points plus whether the
/// `h` (close) operator was seen for it.
#[derive(Default)]
struct SubPath {
    points: Vec<Coord<f64>>,
    explicitly_closed: bool,
}

impl SubPath {
    fn last(&self) -> Option<Coord<f64>> {
        self.points.last().copied()
    }
}

/// State machine that turns a flat list of content-stream operations into
/// geometry. This is the heart of the importer and is unit-tested directly
/// (building a real PDF for every case is impractical).
pub(crate) fn interpret(ops: &[lopdf::content::Operation]) -> PdfDoc {
    let mut polys: Vec<Polygon<f64>> = Vec::new();
    let mut polylines: Vec<LineString<f64>> = Vec::new();

    // The set of subpaths built up since the last painting operator.
    let mut subpaths: Vec<SubPath> = Vec::new();

    // Fetch operand `i` as f64, defaulting to 0.0 (matches the verified
    // `as_float().unwrap_or(0.0)` contract for Integer/Real operands).
    fn num(operands: &[lopdf::Object], i: usize) -> f64 {
        operands
            .get(i)
            .map(|o| o.as_float().unwrap_or(0.0) as f64)
            .unwrap_or(0.0)
    }

    // Append a flattened cubic Bézier (excluding the start point, which is
    // already the current point) to the current subpath.
    fn push_bezier(
        sp: &mut SubPath,
        p0: Coord<f64>,
        p1: Coord<f64>,
        p2: Coord<f64>,
        p3: Coord<f64>,
    ) {
        for s in 1..=BEZIER_SEGMENTS {
            let t = s as f64 / BEZIER_SEGMENTS as f64;
            let mt = 1.0 - t;
            let a = mt * mt * mt;
            let b = 3.0 * mt * mt * t;
            let c = 3.0 * mt * t * t;
            let d = t * t * t;
            sp.points.push(Coord {
                x: a * p0.x + b * p1.x + c * p2.x + d * p3.x,
                y: a * p0.y + b * p1.y + c * p2.y + d * p3.y,
            });
        }
    }

    // Flush the accumulated subpaths into outputs. `closed` selects whether
    // they are treated as polygons (fill) or polylines (stroke).
    fn flush(
        subpaths: &mut Vec<SubPath>,
        polys: &mut Vec<Polygon<f64>>,
        polylines: &mut Vec<LineString<f64>>,
        closed: bool,
    ) {
        for sp in subpaths.drain(..) {
            if closed {
                if sp.points.len() >= 3 {
                    let mut ring = sp.points.clone();
                    // Ensure the exterior ring is explicitly closed.
                    if ring.first() != ring.last() {
                        ring.push(ring[0]);
                    }
                    polys.push(Polygon::new(LineString::new(ring), vec![]));
                }
            } else if sp.points.len() >= 2 {
                let mut pts = sp.points.clone();
                if sp.explicitly_closed && pts.first() != pts.last() {
                    pts.push(pts[0]);
                }
                polylines.push(LineString::new(pts));
            }
        }
    }

    for op in ops {
        let operator: &str = &op.operator;
        let operands: &Vec<lopdf::Object> = &op.operands;

        match operator {
            // --- path construction ---
            "m" => {
                // Begin a new subpath at (x, y).
                subpaths.push(SubPath {
                    points: vec![Coord {
                        x: num(operands, 0),
                        y: num(operands, 1),
                    }],
                    explicitly_closed: false,
                });
            }
            "l" => {
                let pt = Coord {
                    x: num(operands, 0),
                    y: num(operands, 1),
                };
                if let Some(sp) = subpaths.last_mut() {
                    sp.points.push(pt);
                } else {
                    // No current point: behave like `m` for robustness.
                    subpaths.push(SubPath {
                        points: vec![pt],
                        explicitly_closed: false,
                    });
                }
            }
            "c" | "v" | "y" => {
                if let Some(sp) = subpaths.last_mut() {
                    let p0 = sp.last().unwrap_or(Coord { x: 0.0, y: 0.0 });
                    let (p1, p2, p3) = match operator {
                        // c: x1 y1 x2 y2 x3 y3
                        "c" => (
                            Coord {
                                x: num(operands, 0),
                                y: num(operands, 1),
                            },
                            Coord {
                                x: num(operands, 2),
                                y: num(operands, 3),
                            },
                            Coord {
                                x: num(operands, 4),
                                y: num(operands, 5),
                            },
                        ),
                        // v: x2 y2 x3 y3  (first control point = current point)
                        "v" => (
                            p0,
                            Coord {
                                x: num(operands, 0),
                                y: num(operands, 1),
                            },
                            Coord {
                                x: num(operands, 2),
                                y: num(operands, 3),
                            },
                        ),
                        // y: x1 y1 x3 y3  (second control point = end point)
                        _ => {
                            let p3 = Coord {
                                x: num(operands, 2),
                                y: num(operands, 3),
                            };
                            (
                                Coord {
                                    x: num(operands, 0),
                                    y: num(operands, 1),
                                },
                                p3,
                                p3,
                            )
                        }
                    };
                    push_bezier(sp, p0, p1, p2, p3);
                }
            }
            "re" => {
                // Closed rectangle subpath: x y w h.
                let x = num(operands, 0);
                let y = num(operands, 1);
                let w = num(operands, 2);
                let h = num(operands, 3);
                subpaths.push(SubPath {
                    points: vec![
                        Coord { x, y },
                        Coord { x: x + w, y },
                        Coord {
                            x: x + w,
                            y: y + h,
                        },
                        Coord { x, y: y + h },
                        Coord { x, y },
                    ],
                    explicitly_closed: true,
                });
            }
            "h" => {
                if let Some(sp) = subpaths.last_mut() {
                    sp.explicitly_closed = true;
                }
            }

            // --- path painting (these END the current path) ---
            // Fill (and fill+stroke) variants: closed polygons.
            "f" | "F" | "f*" | "b" | "b*" | "B" | "B*" => {
                flush(&mut subpaths, &mut polys, &mut polylines, true);
            }
            // Stroke variants: open polylines. `s` also closes its subpaths.
            "S" => {
                flush(&mut subpaths, &mut polys, &mut polylines, false);
            }
            "s" => {
                for sp in subpaths.iter_mut() {
                    sp.explicitly_closed = true;
                }
                flush(&mut subpaths, &mut polys, &mut polylines, false);
            }
            // No-op painting: discard the path.
            "n" => {
                subpaths.clear();
            }

            // Everything else (cm, text, colour, clipping, …) is ignored.
            _ => {}
        }
    }

    PdfDoc {
        polygons: MultiPolygon::new(polys),
        polylines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::Area;
    use lopdf::content::Operation;
    use lopdf::Object;

    fn op(operator: &str, operands: Vec<Object>) -> Operation {
        Operation {
            operator: operator.into(),
            operands,
        }
    }

    #[test]
    fn rectangle_fill_makes_one_polygon() {
        let ops = vec![
            op("re", vec![0.into(), 0.into(), 10.into(), 5.into()]),
            op("f", vec![]),
        ];
        let doc = interpret(&ops);
        assert_eq!(doc.polygons.0.len(), 1);
        assert!(doc.polylines.is_empty());
        assert_eq!(doc.polygons.0[0].unsigned_area(), 50.0);
    }

    #[test]
    fn stroked_open_path_makes_one_polyline() {
        let ops = vec![
            op("m", vec![0.into(), 0.into()]),
            op("l", vec![10.into(), 0.into()]),
            op("l", vec![10.into(), 10.into()]),
            op("S", vec![]),
        ];
        let doc = interpret(&ops);
        assert!(doc.polygons.0.is_empty());
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(doc.polylines[0].0.len(), 3);
    }

    #[test]
    fn stroke_s_closes_the_subpath() {
        let ops = vec![
            op("m", vec![0.into(), 0.into()]),
            op("l", vec![10.into(), 0.into()]),
            op("l", vec![10.into(), 10.into()]),
            op("s", vec![]),
        ];
        let doc = interpret(&ops);
        assert_eq!(doc.polylines.len(), 1);
        // Closing appends the first point back, so 4 vertices.
        assert_eq!(doc.polylines[0].0.len(), 4);
        let pts = &doc.polylines[0].0;
        assert_eq!(pts.first(), pts.last());
    }

    #[test]
    fn no_paint_op_discards_path() {
        let ops = vec![
            op("re", vec![0.into(), 0.into(), 10.into(), 5.into()]),
            op("n", vec![]),
        ];
        let doc = interpret(&ops);
        assert!(doc.polygons.0.is_empty());
        assert!(doc.polylines.is_empty());
    }

    #[test]
    fn manual_close_h_then_fill() {
        let ops = vec![
            op("m", vec![0.into(), 0.into()]),
            op("l", vec![4.into(), 0.into()]),
            op("l", vec![4.into(), 4.into()]),
            op("l", vec![0.into(), 4.into()]),
            op("h", vec![]),
            op("f", vec![]),
        ];
        let doc = interpret(&ops);
        assert_eq!(doc.polygons.0.len(), 1);
        assert_eq!(doc.polygons.0[0].unsigned_area(), 16.0);
    }

    #[test]
    fn cubic_bezier_is_flattened() {
        // A straight-line "curve" from (0,0) to (3,0); flattening should add
        // BEZIER_SEGMENTS points and the final point must be the endpoint.
        let ops = vec![
            op("m", vec![0.into(), 0.into()]),
            op(
                "c",
                vec![
                    Object::Real(1.0),
                    Object::Real(0.0),
                    Object::Real(2.0),
                    Object::Real(0.0),
                    Object::Real(3.0),
                    Object::Real(0.0),
                ],
            ),
            op("S", vec![]),
        ];
        let doc = interpret(&ops);
        assert_eq!(doc.polylines.len(), 1);
        let pts = &doc.polylines[0].0;
        assert_eq!(pts.len(), 1 + BEZIER_SEGMENTS);
        let last = pts.last().unwrap();
        assert!((last.x - 3.0).abs() < 1e-9);
        assert!((last.y - 0.0).abs() < 1e-9);
    }

    #[test]
    fn v_and_y_beziers_reach_their_endpoints() {
        // `v`: control1 = current point. End at (5, 5).
        let ops_v = vec![
            op("m", vec![0.into(), 0.into()]),
            op(
                "v",
                vec![
                    Object::Real(2.0),
                    Object::Real(5.0),
                    Object::Real(5.0),
                    Object::Real(5.0),
                ],
            ),
            op("S", vec![]),
        ];
        let doc_v = interpret(&ops_v);
        let last_v = doc_v.polylines[0].0.last().unwrap();
        assert!((last_v.x - 5.0).abs() < 1e-9);
        assert!((last_v.y - 5.0).abs() < 1e-9);

        // `y`: control2 = end point. End at (7, 1).
        let ops_y = vec![
            op("m", vec![0.into(), 0.into()]),
            op(
                "y",
                vec![
                    Object::Real(1.0),
                    Object::Real(3.0),
                    Object::Real(7.0),
                    Object::Real(1.0),
                ],
            ),
            op("S", vec![]),
        ];
        let doc_y = interpret(&ops_y);
        let last_y = doc_y.polylines[0].0.last().unwrap();
        assert!((last_y.x - 7.0).abs() < 1e-9);
        assert!((last_y.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn parse_empty_slice_is_err() {
        assert!(parse(&[]).is_err());
    }
}
