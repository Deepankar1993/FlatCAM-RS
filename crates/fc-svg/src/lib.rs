//! `fc-svg` — a pragmatic SVG → geometry importer for the FlatCAM Rust port.
//!
//! Port of the geometry-extraction core of FlatCAM's `appParsers/ParseSVG.py`.
//! It walks an SVG document and converts the drawing primitives into `geo`
//! geometry: `<path>` data (M/L/H/V/C/S/Q/T/Z with cubic/quadratic Bézier
//! flattening), plus `<rect>`, `<circle>`, `<ellipse>`, `<line>`, `<polyline>`,
//! and `<polygon>`. Closed subpaths/shapes become polygons; open ones become
//! polylines (suitable for engraving/follow paths).
//!
//! Scope notes (v1): element-level `transform` attributes and elliptical-arc
//! (`A`) commands are approximated (arcs become straight chords); these are
//! tracked in the roadmap. The SVG Y-axis (down) is preserved as-is — callers
//! that need Y-up can mirror via `fc_geo::transform::mirror_x`.

use fc_geo::{circle, Coord, LineString, MultiPolygon, Polygon};

#[derive(thiserror::Error, Debug)]
pub enum SvgError {
    #[error("XML parse error: {0}")]
    Xml(#[from] roxmltree::Error),
}

/// Result of importing an SVG: filled shapes as polygons, open paths as lines.
#[derive(Debug)]
pub struct SvgDoc {
    pub polygons: MultiPolygon<f64>,
    pub polylines: Vec<LineString<f64>>,
}

const BEZIER_STEPS: usize = 16;
const ELLIPSE_STEPS: usize = 48;

/// Parse SVG source text into geometry.
pub fn parse(text: &str) -> Result<SvgDoc, SvgError> {
    let doc = roxmltree::Document::parse(text)?;
    let mut polys: Vec<Polygon<f64>> = Vec::new();
    let mut lines: Vec<LineString<f64>> = Vec::new();

    for node in doc.descendants() {
        if !node.is_element() {
            continue;
        }
        let attr = |n: &str| node.attribute(n).and_then(|v| v.trim().parse::<f64>().ok());
        match node.tag_name().name() {
            "path" => {
                if let Some(d) = node.attribute("d") {
                    for sub in parse_path(d) {
                        if sub.closed && sub.points.len() >= 3 {
                            polys.push(close_polygon(sub.points));
                        } else if sub.points.len() >= 2 {
                            lines.push(LineString::new(sub.points));
                        }
                    }
                }
            }
            "rect" => {
                let (x, y) = (attr("x").unwrap_or(0.0), attr("y").unwrap_or(0.0));
                let (w, h) = (attr("width").unwrap_or(0.0), attr("height").unwrap_or(0.0));
                if w > 0.0 && h > 0.0 {
                    let ring = vec![
                        Coord { x, y },
                        Coord { x: x + w, y },
                        Coord { x: x + w, y: y + h },
                        Coord { x, y: y + h },
                        Coord { x, y },
                    ];
                    polys.push(Polygon::new(LineString::new(ring), vec![]));
                }
            }
            "circle" => {
                let (cx, cy) = (attr("cx").unwrap_or(0.0), attr("cy").unwrap_or(0.0));
                if let Some(r) = attr("r") {
                    if r > 0.0 {
                        polys.push(circle(cx, cy, r, ELLIPSE_STEPS));
                    }
                }
            }
            "ellipse" => {
                let (cx, cy) = (attr("cx").unwrap_or(0.0), attr("cy").unwrap_or(0.0));
                let (rx, ry) = (attr("rx").unwrap_or(0.0), attr("ry").unwrap_or(0.0));
                if rx > 0.0 && ry > 0.0 {
                    polys.push(ellipse(cx, cy, rx, ry, ELLIPSE_STEPS));
                }
            }
            "line" => {
                let (x1, y1) = (attr("x1").unwrap_or(0.0), attr("y1").unwrap_or(0.0));
                let (x2, y2) = (attr("x2").unwrap_or(0.0), attr("y2").unwrap_or(0.0));
                lines.push(LineString::new(vec![
                    Coord { x: x1, y: y1 },
                    Coord { x: x2, y: y2 },
                ]));
            }
            "polyline" => {
                if let Some(pts) = node.attribute("points") {
                    let c = parse_points(pts);
                    if c.len() >= 2 {
                        lines.push(LineString::new(c));
                    }
                }
            }
            "polygon" => {
                if let Some(pts) = node.attribute("points") {
                    let c = parse_points(pts);
                    if c.len() >= 3 {
                        polys.push(close_polygon(c));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(SvgDoc {
        polygons: MultiPolygon::new(polys),
        polylines: lines,
    })
}

fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64, steps: usize) -> Polygon<f64> {
    let mut ring = Vec::with_capacity(steps + 1);
    for i in 0..steps {
        let a = std::f64::consts::TAU * (i as f64) / (steps as f64);
        ring.push(Coord { x: cx + rx * a.cos(), y: cy + ry * a.sin() });
    }
    ring.push(ring[0]);
    Polygon::new(LineString::new(ring), vec![])
}

fn close_polygon(mut pts: Vec<Coord<f64>>) -> Polygon<f64> {
    if pts.first() != pts.last() {
        if let Some(f) = pts.first().copied() {
            pts.push(f);
        }
    }
    Polygon::new(LineString::new(pts), vec![])
}

fn parse_points(s: &str) -> Vec<Coord<f64>> {
    let nums: Vec<f64> = s
        .split([',', ' ', '\t', '\n', '\r'])
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<f64>().ok())
        .collect();
    nums.chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| Coord { x: c[0], y: c[1] })
        .collect()
}

struct SubPath {
    points: Vec<Coord<f64>>,
    closed: bool,
}

/// Parse an SVG path `d` string into subpaths.
fn parse_path(d: &str) -> Vec<SubPath> {
    let toks = tokenize_path(d);
    let mut out: Vec<SubPath> = Vec::new();
    let mut cur: Vec<Coord<f64>> = Vec::new();
    let (mut x, mut y) = (0.0f64, 0.0f64);
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    let mut i = 0;
    let mut cmd = ' ';
    let mut last_c2: Option<(f64, f64)> = None; // last cubic control (for S)
    let mut last_q: Option<(f64, f64)> = None; // last quad control (for T)

    let num = |i: &mut usize| -> Option<f64> {
        while *i < toks.len() {
            if let Tok::Num(n) = toks[*i] {
                *i += 1;
                return Some(n);
            } else {
                return None;
            }
        }
        None
    };

    while i < toks.len() {
        match toks[i] {
            Tok::Cmd(c) => {
                cmd = c;
                i += 1;
            }
            Tok::Num(_) => { /* repeat previous command */ }
        }
        let rel = cmd.is_ascii_lowercase();
        match cmd.to_ascii_uppercase() {
            'M' => {
                let nx = match num(&mut i) { Some(v) => v, None => break };
                let ny = match num(&mut i) { Some(v) => v, None => break };
                if cur.len() >= 2 {
                    out.push(SubPath { points: std::mem::take(&mut cur), closed: false });
                } else {
                    cur.clear();
                }
                x = if rel { x + nx } else { nx };
                y = if rel { y + ny } else { ny };
                sx = x;
                sy = y;
                cur.push(Coord { x, y });
                cmd = if rel { 'l' } else { 'L' }; // subsequent pairs are lineto
                last_c2 = None;
                last_q = None;
            }
            'L' => {
                let nx = match num(&mut i) { Some(v) => v, None => break };
                let ny = match num(&mut i) { Some(v) => v, None => break };
                x = if rel { x + nx } else { nx };
                y = if rel { y + ny } else { ny };
                cur.push(Coord { x, y });
                last_c2 = None;
                last_q = None;
            }
            'H' => {
                let nx = match num(&mut i) { Some(v) => v, None => break };
                x = if rel { x + nx } else { nx };
                cur.push(Coord { x, y });
                last_c2 = None;
                last_q = None;
            }
            'V' => {
                let ny = match num(&mut i) { Some(v) => v, None => break };
                y = if rel { y + ny } else { ny };
                cur.push(Coord { x, y });
                last_c2 = None;
                last_q = None;
            }
            'C' => {
                let (mut a, mut b, mut c, mut dd, mut e, mut f) = (0.0,0.0,0.0,0.0,0.0,0.0);
                for (k, slot) in [&mut a,&mut b,&mut c,&mut dd,&mut e,&mut f].into_iter().enumerate() {
                    match num(&mut i) { Some(v) => *slot = v, None => { let _ = k; return out_finish(out, cur); } }
                }
                let (x1,y1) = abs(rel,x,y,a,b);
                let (x2,y2) = abs(rel,x,y,c,dd);
                let (ex,ey) = abs(rel,x,y,e,f);
                flatten_cubic(&mut cur,(x,y),(x1,y1),(x2,y2),(ex,ey));
                last_c2 = Some((x2,y2));
                last_q = None;
                x = ex; y = ey;
            }
            'S' => {
                let (mut c, mut dd, mut e, mut f) = (0.0,0.0,0.0,0.0);
                for slot in [&mut c,&mut dd,&mut e,&mut f] {
                    match num(&mut i) { Some(v) => *slot = v, None => return out_finish(out, cur) }
                }
                let (x2,y2) = abs(rel,x,y,c,dd);
                let (ex,ey) = abs(rel,x,y,e,f);
                let (x1,y1) = match last_c2 { Some((px,py)) => (2.0*x-px, 2.0*y-py), None => (x,y) };
                flatten_cubic(&mut cur,(x,y),(x1,y1),(x2,y2),(ex,ey));
                last_c2 = Some((x2,y2));
                last_q = None;
                x = ex; y = ey;
            }
            'Q' => {
                let (mut a, mut b, mut e, mut f) = (0.0,0.0,0.0,0.0);
                for slot in [&mut a,&mut b,&mut e,&mut f] {
                    match num(&mut i) { Some(v) => *slot = v, None => return out_finish(out, cur) }
                }
                let (cxp,cyp) = abs(rel,x,y,a,b);
                let (ex,ey) = abs(rel,x,y,e,f);
                flatten_quad(&mut cur,(x,y),(cxp,cyp),(ex,ey));
                last_q = Some((cxp,cyp));
                last_c2 = None;
                x = ex; y = ey;
            }
            'T' => {
                let (mut e, mut f) = (0.0,0.0);
                for slot in [&mut e,&mut f] {
                    match num(&mut i) { Some(v) => *slot = v, None => return out_finish(out, cur) }
                }
                let (ex,ey) = abs(rel,x,y,e,f);
                let (cxp,cyp) = match last_q { Some((px,py)) => (2.0*x-px, 2.0*y-py), None => (x,y) };
                flatten_quad(&mut cur,(x,y),(cxp,cyp),(ex,ey));
                last_q = Some((cxp,cyp));
                last_c2 = None;
                x = ex; y = ey;
            }
            'A' => {
                // Elliptical arc: approximate as a straight chord to the endpoint.
                // Consume rx ry x-axis-rotation large-arc-flag sweep-flag x y.
                let mut params = [0.0f64; 7];
                let mut ok = true;
                for slot in params.iter_mut() {
                    match num(&mut i) {
                        Some(v) => *slot = v,
                        None => { ok = false; break; }
                    }
                }
                if !ok {
                    return out_finish(out, cur);
                }
                let (ex, ey) = abs(rel, x, y, params[5], params[6]);
                cur.push(Coord { x: ex, y: ey });
                x = ex;
                y = ey;
                last_c2 = None;
                last_q = None;
            }
            'Z' => {
                if !cur.is_empty() {
                    cur.push(Coord { x: sx, y: sy });
                    out.push(SubPath { points: std::mem::take(&mut cur), closed: true });
                }
                x = sx; y = sy;
                last_c2 = None;
                last_q = None;
            }
            _ => { i += 1; }
        }
    }
    if cur.len() >= 2 {
        out.push(SubPath { points: cur, closed: false });
    }
    out
}

fn out_finish(mut out: Vec<SubPath>, cur: Vec<Coord<f64>>) -> Vec<SubPath> {
    if cur.len() >= 2 {
        out.push(SubPath { points: cur, closed: false });
    }
    out
}

fn abs(rel: bool, x: f64, y: f64, dx: f64, dy: f64) -> (f64, f64) {
    if rel { (x + dx, y + dy) } else { (dx, dy) }
}

fn flatten_cubic(out: &mut Vec<Coord<f64>>, p0:(f64,f64), p1:(f64,f64), p2:(f64,f64), p3:(f64,f64)) {
    for k in 1..=BEZIER_STEPS {
        let t = k as f64 / BEZIER_STEPS as f64;
        let u = 1.0 - t;
        let x = u*u*u*p0.0 + 3.0*u*u*t*p1.0 + 3.0*u*t*t*p2.0 + t*t*t*p3.0;
        let y = u*u*u*p0.1 + 3.0*u*u*t*p1.1 + 3.0*u*t*t*p2.1 + t*t*t*p3.1;
        out.push(Coord { x, y });
    }
}

fn flatten_quad(out: &mut Vec<Coord<f64>>, p0:(f64,f64), p1:(f64,f64), p2:(f64,f64)) {
    for k in 1..=BEZIER_STEPS {
        let t = k as f64 / BEZIER_STEPS as f64;
        let u = 1.0 - t;
        let x = u*u*p0.0 + 2.0*u*t*p1.0 + t*t*p2.0;
        let y = u*u*p0.1 + 2.0*u*t*p1.1 + t*t*p2.1;
        out.push(Coord { x, y });
    }
}

#[derive(Clone, Copy)]
enum Tok {
    Cmd(char),
    Num(f64),
}

fn tokenize_path(d: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let b: Vec<char> = d.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_alphabetic() {
            toks.push(Tok::Cmd(c));
            i += 1;
        } else if c == ',' || c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '-' || c == '+' || c == '.' {
            // scan a number
            let start = i;
            if b[i] == '-' || b[i] == '+' {
                i += 1;
            }
            let mut seen_dot = false;
            while i < b.len() {
                let ch = b[i];
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !seen_dot {
                    seen_dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E') && i + 1 < b.len() {
                    i += 1;
                    if b[i] == '-' || b[i] == '+' {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            let s: String = b[start..i].iter().collect();
            if let Ok(n) = s.parse::<f64>() {
                toks.push(Tok::Num(n));
            }
        } else {
            i += 1;
        }
    }
    toks
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_geo::area;

    #[test]
    fn parses_rect() {
        let svg = r#"<svg><rect x="0" y="0" width="10" height="5"/></svg>"#;
        let d = parse(svg).unwrap();
        assert_eq!(d.polygons.0.len(), 1);
        assert!((area(&d.polygons) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn parses_circle() {
        let svg = r#"<svg><circle cx="0" cy="0" r="2"/></svg>"#;
        let d = parse(svg).unwrap();
        assert!((area(&d.polygons) - std::f64::consts::PI * 4.0).abs() < 0.1);
    }

    #[test]
    fn parses_closed_path_triangle() {
        let svg = r#"<svg><path d="M0,0 L10,0 L10,10 Z"/></svg>"#;
        let d = parse(svg).unwrap();
        assert_eq!(d.polygons.0.len(), 1);
        assert!((area(&d.polygons) - 50.0).abs() < 1e-6);
    }

    #[test]
    fn parses_open_polyline_path() {
        let svg = r#"<svg><path d="M0,0 L10,0 L10,10"/></svg>"#;
        let d = parse(svg).unwrap();
        assert_eq!(d.polylines.len(), 1);
        assert_eq!(d.polylines[0].0.len(), 3);
        assert_eq!(d.polygons.0.len(), 0);
    }

    #[test]
    fn cubic_bezier_is_flattened() {
        let svg = r#"<svg><path d="M0,0 C0,10 10,10 10,0 Z"/></svg>"#;
        let d = parse(svg).unwrap();
        assert_eq!(d.polygons.0.len(), 1);
        assert!(area(&d.polygons) > 0.0);
        // flattened curve adds many points
        assert!(d.polygons.0[0].exterior().0.len() > 10);
    }

    #[test]
    fn polygon_points() {
        let svg = r#"<svg><polygon points="0,0 4,0 4,4 0,4"/></svg>"#;
        let d = parse(svg).unwrap();
        assert!((area(&d.polygons) - 16.0).abs() < 1e-6);
    }
}
