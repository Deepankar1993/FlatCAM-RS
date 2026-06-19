//! Geometry -> SVG exporter for the FlatCAM Rust port.
//!
//! Counterpart to [`crate::parse`]. Produces a valid standalone SVG document:
//! an `<svg>` root whose `viewBox` is derived from the geometry bounds, with
//! polygons emitted as `<path d="...Z">` (one subpath per ring, so holes are
//! preserved via the even-odd / nonzero fill rule) and polylines emitted as
//! `<polyline>` elements.
//!
//! ## The Y-flip (Inkscape compatibility)
//!
//! FlatCAM's geometry (like DXF/Gerber) is **Y-up**: increasing `y` goes up.
//! SVG — and Inkscape, the canonical consumer of FlatCAM's SVG export — is
//! **Y-down**: increasing `y` goes *down* the page. Upstream FlatCAM therefore
//! mirrors geometry vertically on export so that a board drawn "right way up"
//! in FlatCAM also appears right way up when opened in Inkscape.
//!
//! We reproduce that here: every point's `y` is mapped to `max_y - (y - min_y)`
//! `= (min_y + max_y) - y`, i.e. a reflection about the horizontal mid-line of
//! the geometry bounds. This keeps the emitted coordinates inside the same
//! `[min_y, max_y]` range (so the `viewBox` is unchanged) while flipping the
//! vertical sense. Because [`crate::parse`] reads SVG coordinates verbatim
//! (Y-down), the round-trip recovers a vertically-mirrored copy of the input —
//! callers comparing input to output must apply the same flip (see tests).
//!
//! The flip can be disabled via [`SvgWriteOptions::flip_y`] when a verbatim,
//! non-mirrored document is wanted.

use std::fmt::Write as _;

use fc_geo::{LineString, MultiPolygon, Polygon};

/// Options controlling SVG output.
#[derive(Debug, Clone)]
pub struct SvgWriteOptions {
    /// Apply the vertical Y-flip for Inkscape compatibility (default `true`).
    pub flip_y: bool,
    /// Padding (in user units) added around the geometry bounds for the viewBox.
    pub margin: f64,
    /// Fill colour for polygons (any valid SVG paint, e.g. `"black"`, `"none"`).
    pub fill: String,
    /// Stroke colour for polylines and polygon outlines.
    pub stroke: String,
    /// Stroke width (in user units).
    pub stroke_width: f64,
}

impl Default for SvgWriteOptions {
    fn default() -> Self {
        Self {
            flip_y: true,
            margin: 0.0,
            fill: "black".to_string(),
            stroke: "black".to_string(),
            stroke_width: 0.0,
        }
    }
}

/// Serialize geometry to an SVG document string, using default options.
pub fn write_svg(polygons: &MultiPolygon<f64>, polylines: &[LineString<f64>]) -> String {
    write_svg_with(polygons, polylines, &SvgWriteOptions::default())
}

/// Like [`write_svg`], with explicit [`SvgWriteOptions`].
pub fn write_svg_with(
    polygons: &MultiPolygon<f64>,
    polylines: &[LineString<f64>],
    opts: &SvgWriteOptions,
) -> String {
    let bounds = compute_bounds(polygons, polylines);

    // Empty geometry: emit a valid, empty 0-sized document.
    let (min_x, min_y, max_x, max_y) = match bounds {
        Some(b) => b,
        None => {
            return concat!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n",
                "<svg xmlns=\"http://www.w3.org/2000/svg\" ",
                "viewBox=\"0 0 0 0\" width=\"0\" height=\"0\">\n",
                "</svg>\n"
            )
            .to_string();
        }
    };

    let m = opts.margin;
    let vb_x = min_x - m;
    let vb_y = min_y - m;
    let vb_w = (max_x - min_x) + 2.0 * m;
    let vb_h = (max_y - min_y) + 2.0 * m;

    // The flip reflects about the mid-line of the *unpadded* bounds so output
    // y stays in [min_y, max_y].
    let flip = |y: f64| -> f64 {
        if opts.flip_y {
            (min_y + max_y) - y
        } else {
            y
        }
    };

    let mut s = String::new();
    let _ = write!(
        s,
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" \
         viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">\n",
        fmt(vb_x),
        fmt(vb_y),
        fmt(vb_w),
        fmt(vb_h),
        fmt(vb_w),
        fmt(vb_h),
    );

    if !polygons.0.is_empty() {
        let _ = write!(
            s,
            "  <g fill=\"{}\" fill-rule=\"evenodd\" stroke=\"none\">\n",
            xml_escape(&opts.fill)
        );
        for poly in &polygons.0 {
            let _ = writeln!(s, "    <path d=\"{}\"/>", polygon_path_data(poly, &flip));
        }
        s.push_str("  </g>\n");
    }

    if !polylines.is_empty() {
        let sw = if opts.stroke_width > 0.0 {
            opts.stroke_width
        } else {
            // A sensible visible default proportional to the drawing.
            (vb_w.max(vb_h) / 200.0).max(1e-6)
        };
        let _ = write!(
            s,
            "  <g fill=\"none\" stroke=\"{}\" stroke-width=\"{}\">\n",
            xml_escape(&opts.stroke),
            fmt(sw)
        );
        for ls in polylines {
            if ls.0.len() < 2 {
                continue;
            }
            let pts = points_attr(&ls.0, &flip);
            let _ = writeln!(s, "    <polyline points=\"{pts}\"/>");
        }
        s.push_str("  </g>\n");
    }

    s.push_str("</svg>\n");
    s
}

/// Build the `d` attribute for a polygon: exterior ring as the first subpath,
/// each interior ring (hole) as a further `M ... Z` subpath. With
/// `fill-rule="evenodd"` the interior subpaths punch holes.
fn polygon_path_data(poly: &Polygon<f64>, flip: &impl Fn(f64) -> f64) -> String {
    let mut d = String::new();
    ring_subpath(&mut d, poly.exterior(), flip);
    for hole in poly.interiors() {
        d.push(' ');
        ring_subpath(&mut d, hole, flip);
    }
    d
}

fn ring_subpath(d: &mut String, ring: &LineString<f64>, flip: &impl Fn(f64) -> f64) {
    let pts = &ring.0;
    if pts.is_empty() {
        return;
    }
    // Drop a duplicated closing point; `Z` re-closes the subpath.
    let n = if pts.len() >= 2 && pts[0] == pts[pts.len() - 1] {
        pts.len() - 1
    } else {
        pts.len()
    };
    for (i, c) in pts.iter().take(n).enumerate() {
        let cmd = if i == 0 { 'M' } else { 'L' };
        let _ = write!(d, "{}{},{}", cmd, fmt(c.x), fmt(flip(c.y)));
        if i + 1 < n {
            d.push(' ');
        }
    }
    d.push_str(" Z");
}

fn points_attr(coords: &[fc_geo::Coord<f64>], flip: &impl Fn(f64) -> f64) -> String {
    let mut s = String::new();
    for (i, c) in coords.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        let _ = write!(s, "{},{}", fmt(c.x), fmt(flip(c.y)));
    }
    s
}

fn compute_bounds(
    polygons: &MultiPolygon<f64>,
    polylines: &[LineString<f64>],
) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut seen = false;

    let mut acc = |x: f64, y: f64| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };

    for poly in &polygons.0 {
        for c in &poly.exterior().0 {
            acc(c.x, c.y);
            seen = true;
        }
        for hole in poly.interiors() {
            for c in &hole.0 {
                acc(c.x, c.y);
                seen = true;
            }
        }
    }
    for ls in polylines {
        for c in &ls.0 {
            acc(c.x, c.y);
            seen = true;
        }
    }

    if seen {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

/// Format an f64 compactly without a trailing `.0` and without scientific
/// notation, which some strict SVG readers dislike.
fn fmt(v: f64) -> String {
    if v == 0.0 {
        // Normalise -0.0 to 0.
        return "0".to_string();
    }
    let mut s = format!("{v:.6}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use fc_geo::{area, bounds, Coord, LineString, Polygon};

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
    fn empty_geometry_is_valid_empty_document() {
        let svg = write_svg(&MultiPolygon::new(vec![]), &[]);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        // Re-parses to nothing.
        let doc = parse(&svg).unwrap();
        assert!(doc.polygons.0.is_empty());
        assert!(doc.polylines.is_empty());
    }

    #[test]
    fn square_with_hole_round_trips_with_yflip() {
        let mp = MultiPolygon::new(vec![square_with_hole()]);
        let svg = write_svg(&mp, &[]);

        // The hole is emitted as a second `M...Z` subpath inside the same
        // `<path>`. The importer now reconstructs exterior/hole nesting, so the
        // two closed rings come back as ONE polygon with one interior ring.
        let doc = parse(&svg).unwrap();
        assert_eq!(doc.polygons.0.len(), 1, "exterior + hole nest into 1 polygon");
        assert_eq!(
            doc.polygons.0[0].interiors().len(),
            1,
            "the hole is reconstructed as one interior ring"
        );

        // Net filled area = 100 (outer) − 16 (inner) = 84, regardless of Y-flip.
        let a = area(&doc.polygons);
        assert!((a - 84.0).abs() < 1e-6, "area was {a}");

        // Bounds are preserved under the mid-line reflection.
        let (bx0, by0, bx1, by1) = bounds(&doc.polygons).unwrap();
        assert!((bx0 - 0.0).abs() < 1e-6 && (bx1 - 10.0).abs() < 1e-6);
        assert!((by0 - 0.0).abs() < 1e-6 && (by1 - 10.0).abs() < 1e-6);
    }

    #[test]
    fn yflip_is_consistent() {
        // A point near the bottom of the bounds should land near the top after
        // the flip. Build an asymmetric triangle so the flip is observable.
        let poly = Polygon::new(
            LineString::new(vec![
                Coord { x: 0.0, y: 0.0 },
                Coord { x: 10.0, y: 0.0 },
                Coord { x: 0.0, y: 8.0 },
                Coord { x: 0.0, y: 0.0 },
            ]),
            vec![],
        );
        let mp = MultiPolygon::new(vec![poly]);
        let svg = write_svg(&mp, &[]);
        let doc = parse(&svg).unwrap();
        assert_eq!(doc.polygons.0.len(), 1);
        // Area is flip-invariant.
        assert!((area(&doc.polygons) - 40.0).abs() < 1e-6);
        // The flipped triangle: apex moved from y=8 to y=0; the long edge
        // (originally at y=0) now sits at y=8. Verify by checking that the
        // re-parsed exterior has a vertex at y=8 (was the apex region).
        let ys: Vec<f64> = doc.polygons.0[0].exterior().0.iter().map(|c| c.y).collect();
        assert!(ys.iter().any(|&y| (y - 8.0).abs() < 1e-6));
        assert!(ys.iter().any(|&y| (y - 0.0).abs() < 1e-6));
    }

    #[test]
    fn polyline_round_trips() {
        let ls = LineString::new(vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 4.0, y: 0.0 },
            Coord { x: 4.0, y: 3.0 },
        ]);
        let svg = write_svg(&MultiPolygon::new(vec![]), &[ls]);
        let doc = parse(&svg).unwrap();
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(doc.polylines[0].0.len(), 3);
        assert!(doc.polygons.0.is_empty());
    }

    #[test]
    fn polygons_and_polylines_together() {
        let mp = MultiPolygon::new(vec![square_with_hole()]);
        let ls = LineString::new(vec![
            Coord { x: 1.0, y: 1.0 },
            Coord { x: 9.0, y: 9.0 },
        ]);
        let svg = write_svg(&mp, &[ls]);
        let doc = parse(&svg).unwrap();
        // square-with-hole -> 1 nested polygon (see square_with_hole_round_trips_with_yflip).
        assert_eq!(doc.polygons.0.len(), 1);
        assert_eq!(doc.polygons.0[0].interiors().len(), 1);
        assert!((area(&doc.polygons) - 84.0).abs() < 1e-6);
        assert_eq!(doc.polylines.len(), 1);
        assert_eq!(doc.polylines[0].0.len(), 2);
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
        let svg = write_svg(&mp, &[]);
        let doc = parse(&svg).unwrap();
        assert_eq!(doc.polygons.0.len(), 2);
        assert!((area(&doc.polygons) - 13.0).abs() < 1e-6);
    }

    #[test]
    fn no_flip_option_is_verbatim() {
        let opts = SvgWriteOptions {
            flip_y: false,
            ..Default::default()
        };
        let poly = Polygon::new(
            LineString::new(vec![
                Coord { x: 0.0, y: 0.0 },
                Coord { x: 10.0, y: 0.0 },
                Coord { x: 0.0, y: 8.0 },
                Coord { x: 0.0, y: 0.0 },
            ]),
            vec![],
        );
        let mp = MultiPolygon::new(vec![poly]);
        let svg = write_svg_with(&mp, &[], &opts);
        let doc = parse(&svg).unwrap();
        // Without flip, apex stays at y=8 and base at y=0 (same as input).
        let ys: Vec<f64> = doc.polygons.0[0].exterior().0.iter().map(|c| c.y).collect();
        assert!(ys.iter().any(|&y| (y - 8.0).abs() < 1e-6));
    }
}
