//! Minimal, hand-rolled single-page PDF *writer*.
//!
//! This is the inverse of the importer in [`crate`]: given the geometry that
//! [`crate::parse`] would produce (a [`MultiPolygon`] of filled regions plus a
//! list of stroked [`LineString`] polylines), it emits a complete, structurally
//! valid PDF 1.4 document as a byte vector — no heavy PDF dependency.
//!
//! # How the PDF is built
//!
//! A PDF is a sequence of numbered *objects*, an *xref* table that records the
//! byte offset of each object, and a *trailer* that points at the xref. We emit
//! exactly four objects:
//!
//! 1. `/Catalog`  — the document root.
//! 2. `/Pages`    — the page tree (a single kid).
//! 3. `/Page`     — the page, carrying the `/MediaBox`.
//! 4. a content stream — the path-drawing operators.
//!
//! The byte offset of every object is captured *as it is written*, so the
//! `xref` table and `startxref` value are always correct regardless of the
//! geometry. PDF user space is **Y-up**, exactly like the geometry coming out of
//! the importer, so coordinates need no axis flip — only an optional uniform
//! `scale` and a translation that moves the geometry's lower-left corner to the
//! page margin.

use fc_geo::{LineString, MultiPolygon};

/// Options controlling [`write_pdf`].
#[derive(Clone, Copy, Debug)]
pub struct PdfWriteOptions {
    /// Margin, in PDF points, added around the geometry bounds on every side.
    pub margin: f64,
    /// Uniform scale applied to geometry coordinates before they become PDF
    /// points (geometry units are typically mm or inches; `1.0` keeps them 1:1).
    pub scale: f64,
    /// Stroke width for polylines and (when [`Self::fill`] is `false`) polygons.
    pub line_width: f64,
    /// If `true`, closed polygons are *filled*; if `false`, they are *stroked*.
    pub fill: bool,
    /// Optional fixed paper size `(width, height)` in points. When `Some`, the
    /// `/MediaBox` uses it verbatim (geometry is *not* re-centred into it);
    /// when `None` (the default) the MediaBox is derived from the geometry
    /// bounds plus the margin.
    pub paper_size: Option<(f64, f64)>,
}

impl Default for PdfWriteOptions {
    fn default() -> Self {
        PdfWriteOptions {
            margin: 10.0,
            scale: 1.0,
            line_width: 0.5,
            fill: true,
            paper_size: None,
        }
    }
}

/// Bounds `(min_x, min_y, max_x, max_y)` over all input geometry, or `None` if
/// there is no geometry at all.
fn geometry_bounds(
    polygons: &MultiPolygon<f64>,
    polylines: &[LineString<f64>],
) -> Option<(f64, f64, f64, f64)> {
    let mut acc: Option<(f64, f64, f64, f64)> = None;
    let mut visit = |x: f64, y: f64| {
        acc = Some(match acc {
            None => (x, y, x, y),
            Some((nx, ny, xx, xy)) => (nx.min(x), ny.min(y), xx.max(x), xy.max(y)),
        });
    };
    for poly in &polygons.0 {
        for c in poly.exterior().0.iter() {
            visit(c.x, c.y);
        }
        for ring in poly.interiors() {
            for c in ring.0.iter() {
                visit(c.x, c.y);
            }
        }
    }
    for line in polylines {
        for c in line.0.iter() {
            visit(c.x, c.y);
        }
    }
    acc
}

/// Format a coordinate compactly (PDF accepts plain decimals; avoid locale and
/// trailing-zero noise so the stream stays small and deterministic).
fn fmt(v: f64) -> String {
    // Round to a few decimals, then strip trailing zeros / dot.
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// Build the content stream (path operators) for the geometry, mapping geometry
/// coordinates to page coordinates via `tx`/`ty` (translation) and `scale`.
fn build_content_stream(
    polygons: &MultiPolygon<f64>,
    polylines: &[LineString<f64>],
    opts: &PdfWriteOptions,
    tx: f64,
    ty: f64,
) -> String {
    let mut s = String::new();
    let map = |x: f64, y: f64| (x * opts.scale + tx, y * opts.scale + ty);

    // Set a line width for any stroking we do.
    s.push_str(&format!("{} w\n", fmt(opts.line_width)));

    // Filled / stroked polygons.
    for poly in &polygons.0 {
        let ext = &poly.exterior().0;
        if ext.len() < 2 {
            continue;
        }
        let (x0, y0) = map(ext[0].x, ext[0].y);
        s.push_str(&format!("{} {} m\n", fmt(x0), fmt(y0)));
        for c in &ext[1..] {
            let (x, y) = map(c.x, c.y);
            s.push_str(&format!("{} {} l\n", fmt(x), fmt(y)));
        }
        s.push_str("h\n");
        // Interior rings (holes) as sub-paths; even-odd fill handles them.
        for ring in poly.interiors() {
            let r = &ring.0;
            if r.len() < 2 {
                continue;
            }
            let (rx0, ry0) = map(r[0].x, r[0].y);
            s.push_str(&format!("{} {} m\n", fmt(rx0), fmt(ry0)));
            for c in &r[1..] {
                let (x, y) = map(c.x, c.y);
                s.push_str(&format!("{} {} l\n", fmt(x), fmt(y)));
            }
            s.push_str("h\n");
        }
        if opts.fill {
            // Even-odd fill so holes are subtracted.
            s.push_str("f*\n");
        } else {
            s.push_str("S\n");
        }
    }

    // Stroked polylines.
    for line in polylines {
        let pts = &line.0;
        if pts.len() < 2 {
            continue;
        }
        let (x0, y0) = map(pts[0].x, pts[0].y);
        s.push_str(&format!("{} {} m\n", fmt(x0), fmt(y0)));
        for c in &pts[1..] {
            let (x, y) = map(c.x, c.y);
            s.push_str(&format!("{} {} l\n", fmt(x), fmt(y)));
        }
        s.push_str("S\n");
    }

    s
}

/// Write a single-page PDF rendering `polygons` (filled or stroked) and
/// `polylines` (stroked) according to `opts`. The returned bytes are a complete
/// `%PDF-1.4` document ending in `%%EOF`.
///
/// Empty geometry yields a valid, blank one-page PDF (using the paper size if
/// given, else a small default page).
pub fn write_pdf(
    polygons: &MultiPolygon<f64>,
    polylines: &[LineString<f64>],
    opts: &PdfWriteOptions,
) -> Vec<u8> {
    let bounds = geometry_bounds(polygons, polylines);

    // Determine MediaBox and the translation that maps geometry into it.
    let (media_w, media_h, tx, ty) = match opts.paper_size {
        Some((w, h)) => {
            // Fixed paper: place geometry's lower-left at the margin, no centring.
            let (minx, miny) = bounds.map(|(a, b, _, _)| (a, b)).unwrap_or((0.0, 0.0));
            let tx = opts.margin - minx * opts.scale;
            let ty = opts.margin - miny * opts.scale;
            (w, h, tx, ty)
        }
        None => match bounds {
            Some((minx, miny, maxx, maxy)) => {
                let w = (maxx - minx) * opts.scale + 2.0 * opts.margin;
                let h = (maxy - miny) * opts.scale + 2.0 * opts.margin;
                let tx = opts.margin - minx * opts.scale;
                let ty = opts.margin - miny * opts.scale;
                (w.max(1.0), h.max(1.0), tx, ty)
            }
            // No geometry: a tiny blank page.
            None => {
                let s = 2.0 * opts.margin;
                (s.max(72.0), s.max(72.0), opts.margin, opts.margin)
            }
        },
    };

    let content = build_content_stream(polygons, polylines, opts, tx, ty);
    let content_bytes = content.into_bytes();

    // Assemble the body, recording each object's byte offset for the xref.
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();

    out.extend_from_slice(b"%PDF-1.4\n");
    // A binary-marker comment line is conventional for robust transport.
    out.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");

    // Object 1: Catalog.
    offsets.push(out.len());
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    // Object 2: Pages.
    offsets.push(out.len());
    out.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    // Object 3: Page.
    offsets.push(out.len());
    let page = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
/Contents 4 0 R /Resources << >> >>\nendobj\n",
        fmt(media_w),
        fmt(media_h)
    );
    out.extend_from_slice(page.as_bytes());

    // Object 4: Contents stream.
    offsets.push(out.len());
    let stream_hdr = format!("4 0 obj\n<< /Length {} >>\nstream\n", content_bytes.len());
    out.extend_from_slice(stream_hdr.as_bytes());
    out.extend_from_slice(&content_bytes);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    // xref table.
    let xref_offset = out.len();
    let n_objs = offsets.len() + 1; // + the free object 0
    out.extend_from_slice(format!("xref\n0 {}\n", n_objs).as_bytes());
    // Object 0 is the head of the free list.
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }

    // Trailer.
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            n_objs, xref_offset
        )
        .as_bytes(),
    );

    out
}

/// Convenience wrapper using [`PdfWriteOptions::default`].
pub fn write_pdf_default(polygons: &MultiPolygon<f64>, polylines: &[LineString<f64>]) -> Vec<u8> {
    write_pdf(polygons, polylines, &PdfWriteOptions::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::{Coord, LineString, Polygon};

    fn square() -> MultiPolygon<f64> {
        let ring = LineString::new(vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 10.0, y: 0.0 },
            Coord { x: 10.0, y: 10.0 },
            Coord { x: 0.0, y: 10.0 },
            Coord { x: 0.0, y: 0.0 },
        ]);
        MultiPolygon::new(vec![Polygon::new(ring, vec![])])
    }

    fn polyline() -> Vec<LineString<f64>> {
        vec![LineString::new(vec![
            Coord { x: 1.0, y: 1.0 },
            Coord { x: 5.0, y: 8.0 },
            Coord { x: 9.0, y: 2.0 },
        ])]
    }

    /// Locate the `startxref` value and confirm it points at the `xref` keyword.
    fn assert_startxref_points_at_xref(bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        let idx = text.rfind("startxref").expect("startxref present");
        let after = &text[idx + "startxref".len()..];
        let num: usize = after
            .trim_start()
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .unwrap()
            .parse()
            .expect("startxref offset parses");
        assert!(num < bytes.len(), "xref offset in range");
        assert_eq!(
            &bytes[num..num + 4],
            b"xref",
            "startxref offset must land on the xref keyword"
        );
    }

    #[test]
    fn header_and_eof_present() {
        let bytes = write_pdf_default(&square(), &polyline());
        assert!(bytes.starts_with(b"%PDF-"), "must start with %PDF-");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.trim_end().ends_with("%%EOF"), "must end with %%EOF");
    }

    #[test]
    fn contains_mediabox_and_path_operators() {
        let bytes = write_pdf_default(&square(), &polyline());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/MediaBox"), "MediaBox present");
        // moveto / lineto path operators from both the square and the polyline.
        assert!(text.contains(" m\n"), "moveto operator present");
        assert!(text.contains(" l\n"), "lineto operator present");
        // Even-odd fill for the square, stroke for the polyline.
        assert!(text.contains("f*\n"), "fill operator present");
        assert!(text.contains("S\n"), "stroke operator present");
    }

    #[test]
    fn startxref_offset_is_correct() {
        let bytes = write_pdf_default(&square(), &polyline());
        assert_startxref_points_at_xref(&bytes);
    }

    #[test]
    fn mediabox_derived_from_bounds_plus_margin() {
        // 10x10 square, default margin 10 => 30x30 page.
        let opts = PdfWriteOptions::default();
        let bytes = write_pdf(&square(), &[], &opts);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/MediaBox [0 0 30 30]"), "got: {text}");
    }

    #[test]
    fn stroke_mode_uses_stroke_op_for_polygons() {
        let opts = PdfWriteOptions {
            fill: false,
            ..Default::default()
        };
        let bytes = write_pdf(&square(), &[], &opts);
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("f*\n"), "should not fill in stroke mode");
        assert!(text.contains("S\n"), "stroke operator present");
    }

    #[test]
    fn paper_size_override_sets_mediabox() {
        let opts = PdfWriteOptions {
            paper_size: Some((595.0, 842.0)), // A4 points
            ..Default::default()
        };
        let bytes = write_pdf(&square(), &polyline(), &opts);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/MediaBox [0 0 595 842]"), "got: {text}");
        assert_startxref_points_at_xref(&bytes);
    }

    #[test]
    fn empty_geometry_still_valid_blank_page() {
        let empty = MultiPolygon::new(vec![]);
        let bytes = write_pdf_default(&empty, &[]);
        assert!(bytes.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/MediaBox"), "blank page still has a MediaBox");
        assert!(text.contains("/Count 1"), "still one page");
        assert!(text.trim_end().ends_with("%%EOF"));
        assert_startxref_points_at_xref(&bytes);
    }

    #[test]
    fn xref_lists_all_four_objects() {
        let bytes = write_pdf_default(&square(), &polyline());
        let text = String::from_utf8_lossy(&bytes);
        // /Size 5 = 4 objects + free object 0.
        assert!(text.contains("/Size 5"), "trailer Size counts all objects");
        assert!(text.contains("0000000000 65535 f"), "free object 0 entry");
    }
}
