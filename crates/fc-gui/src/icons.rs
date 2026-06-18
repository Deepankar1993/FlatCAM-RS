//! Monochrome vector toolbar icons drawn with the `egui` painter.
//!
//! Each icon is a handful of strokes (1.4px) drawn inside a caller-supplied
//! rectangle, so the toolbar shows crisp line-art instead of emoji glyphs.
//! All rectangles are drawn as closed 4-corner polylines (not `rect_stroke`)
//! to stay robust against egui 0.29 signature churn.

use eframe::egui;

/// Stroke width shared by every icon.
const STROKE_W: f32 = 1.4;

/// Return a centred sub-rect of `rect` scaled by `frac` (0..1).
fn inset(rect: egui::Rect, frac: f32) -> egui::Rect {
    let s = rect.width().min(rect.height()) * frac;
    egui::Rect::from_center_size(rect.center(), egui::vec2(s, s))
}

/// Draw a rectangle outline as a closed 4-corner polyline.
fn rect_outline(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let pts = vec![
        egui::pos2(r.left(), r.top()),
        egui::pos2(r.right(), r.top()),
        egui::pos2(r.right(), r.bottom()),
        egui::pos2(r.left(), r.bottom()),
    ];
    painter.add(egui::Shape::closed_line(pts, stroke));
}

/// Draw the named tool icon as line-art centred in `rect`, using `color`.
/// Unknown names draw a neutral fallback (a rounded square). The icon
/// occupies ~70% of `rect` (inset), so callers can pass the full button area.
pub fn draw_tool_icon(name: &str, painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let r = inset(rect, 0.7);
    let stroke = egui::Stroke::new(STROKE_W, color);

    match name {
        "open" => open_folder(painter, r, stroke),
        "project" => closed_folder(painter, r, stroke),
        "save" => floppy(painter, r, stroke),
        "isolation" => isolation(painter, r, stroke),
        "paint" => paint(painter, r, stroke),
        "ncc" => ncc(painter, r, stroke),
        "cutout" => scissors(painter, r, stroke),
        "drilling" => drill(painter, r, color, stroke),
        "zoomfit" => magnifier(painter, r, stroke),
        "gcode" => document(painter, r, stroke),
        "savegcode" => save_gcode(painter, r, stroke),
        "settings" => gear(painter, r, stroke),
        _ => fallback(painter, r, stroke),
    }
}

/// Open folder: a folder body with a raised flap.
fn open_folder(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let tab_h = r.height() * 0.22;
    let body_top = r.top() + tab_h;
    // Back panel with a raised flap on the left.
    let back = vec![
        egui::pos2(r.left(), r.bottom()),
        egui::pos2(r.left(), body_top),
        egui::pos2(r.left() + r.width() * 0.4, body_top),
        egui::pos2(r.left() + r.width() * 0.5, r.top()),
        egui::pos2(r.right(), r.top()),
        egui::pos2(r.right(), body_top),
    ];
    painter.add(egui::Shape::line(back, stroke));
    // Open front lip skewed outward.
    let lip = vec![
        egui::pos2(r.left(), r.bottom()),
        egui::pos2(r.right(), r.bottom()),
        egui::pos2(r.right() - r.width() * 0.12, body_top + r.height() * 0.08),
        egui::pos2(r.left() + r.width() * 0.12, body_top + r.height() * 0.08),
        egui::pos2(r.left(), r.bottom()),
    ];
    painter.add(egui::Shape::line(lip, stroke));
}

/// Closed folder: a folder outline with a small top tab.
fn closed_folder(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let tab_h = r.height() * 0.22;
    let body_top = r.top() + tab_h;
    let pts = vec![
        egui::pos2(r.left(), r.bottom()),
        egui::pos2(r.left(), body_top),
        egui::pos2(r.left() + r.width() * 0.4, body_top),
        egui::pos2(r.left() + r.width() * 0.5, r.top()),
        egui::pos2(r.right(), r.top()),
        egui::pos2(r.right(), r.bottom()),
    ];
    painter.add(egui::Shape::closed_line(pts, stroke));
}

/// Floppy disk: square with a notched top-right corner and a label.
fn floppy(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let notch = r.width() * 0.22;
    let body = vec![
        egui::pos2(r.left(), r.top()),
        egui::pos2(r.right() - notch, r.top()),
        egui::pos2(r.right(), r.top() + notch),
        egui::pos2(r.right(), r.bottom()),
        egui::pos2(r.left(), r.bottom()),
    ];
    painter.add(egui::Shape::closed_line(body, stroke));
    // Label rectangle in the lower half.
    let label = egui::Rect::from_min_max(
        egui::pos2(r.left() + r.width() * 0.22, r.center().y + r.height() * 0.08),
        egui::pos2(r.right() - r.width() * 0.22, r.bottom() - r.height() * 0.1),
    );
    rect_outline(painter, label, stroke);
}

/// Isolation: an outer square with a concentric inner square (a ring).
fn isolation(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    rect_outline(painter, r, stroke);
    rect_outline(painter, inset(r, 0.55), stroke);
}

/// Paint: a square filled with diagonal hatch lines.
fn paint(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    rect_outline(painter, r, stroke);
    let n = 4;
    let span = r.width() + r.height();
    for i in 1..n {
        let t = (i as f32 / n as f32) * span;
        // Draw from a point on the top/left edge to a point on the right/bottom edge.
        let a = if t <= r.height() {
            egui::pos2(r.left(), r.top() + t)
        } else {
            egui::pos2(r.left() + (t - r.height()), r.bottom())
        };
        let b = if t <= r.width() {
            egui::pos2(r.left() + t, r.top())
        } else {
            egui::pos2(r.right(), r.top() + (t - r.width()))
        };
        painter.line_segment([a, b], stroke);
    }
}

/// NCC: a square with a stack of short horizontal clearing lines.
fn ncc(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    rect_outline(painter, r, stroke);
    let rows = 4;
    let pad = r.width() * 0.18;
    for i in 1..rows {
        let y = r.top() + (i as f32 / rows as f32) * r.height();
        painter.line_segment(
            [egui::pos2(r.left() + pad, y), egui::pos2(r.right() - pad, y)],
            stroke,
        );
    }
}

/// Cutout: scissors (two finger-loops and two crossing blades).
fn scissors(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let loop_r = r.width() * 0.16;
    let lc = egui::pos2(r.left() + loop_r, r.bottom() - loop_r);
    let uc = egui::pos2(r.left() + loop_r, r.top() + loop_r);
    painter.circle_stroke(lc, loop_r, stroke);
    painter.circle_stroke(uc, loop_r, stroke);
    // Pivot near the right-centre; blades cross from each loop to the far tip.
    let pivot = egui::pos2(r.center().x, r.center().y);
    painter.line_segment([lc, egui::pos2(r.right(), r.top())], stroke);
    painter.line_segment([uc, egui::pos2(r.right(), r.bottom())], stroke);
    painter.circle_filled(pivot, STROKE_W, stroke.color);
}

/// Drilling: a crosshair circle with a centre dot (drill point).
fn drill(painter: &egui::Painter, r: egui::Rect, color: egui::Color32, stroke: egui::Stroke) {
    let c = r.center();
    let rad = r.width() * 0.42;
    painter.circle_stroke(c, rad, stroke);
    painter.line_segment(
        [egui::pos2(c.x - rad - 1.0, c.y), egui::pos2(c.x + rad + 1.0, c.y)],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(c.x, c.y - rad - 1.0), egui::pos2(c.x, c.y + rad + 1.0)],
        stroke,
    );
    painter.circle_filled(c, STROKE_W * 1.3, color);
}

/// Zoom-fit: a magnifier (circle lens + short handle).
fn magnifier(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let lens_c = egui::pos2(r.left() + r.width() * 0.38, r.top() + r.height() * 0.38);
    let lens_r = r.width() * 0.3;
    painter.circle_stroke(lens_c, lens_r, stroke);
    let off = lens_r * 0.71; // ~cos(45)
    painter.line_segment(
        [
            egui::pos2(lens_c.x + off, lens_c.y + off),
            egui::pos2(r.right(), r.bottom()),
        ],
        stroke,
    );
}

/// G-code document: a page with a folded corner and three text lines.
fn document(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let fold = r.width() * 0.28;
    let page = vec![
        egui::pos2(r.left(), r.top()),
        egui::pos2(r.right() - fold, r.top()),
        egui::pos2(r.right(), r.top() + fold),
        egui::pos2(r.right(), r.bottom()),
        egui::pos2(r.left(), r.bottom()),
    ];
    painter.add(egui::Shape::closed_line(page, stroke));
    // Folded corner triangle.
    let corner = vec![
        egui::pos2(r.right() - fold, r.top()),
        egui::pos2(r.right() - fold, r.top() + fold),
        egui::pos2(r.right(), r.top() + fold),
    ];
    painter.add(egui::Shape::line(corner, stroke));
    // Three text lines.
    let pad = r.width() * 0.2;
    for i in 0..3 {
        let y = r.top() + r.height() * (0.55 + i as f32 * 0.13);
        painter.line_segment(
            [egui::pos2(r.left() + pad, y), egui::pos2(r.right() - pad, y)],
            stroke,
        );
    }
}

/// Save G-code: a down-arrow dropping into a tray (baseline).
fn save_gcode(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let cx = r.center().x;
    let top = r.top() + r.height() * 0.1;
    let tip = r.center().y + r.height() * 0.12;
    // Shaft.
    painter.line_segment([egui::pos2(cx, top), egui::pos2(cx, tip)], stroke);
    // Arrow head.
    let head = r.width() * 0.18;
    painter.line_segment([egui::pos2(cx - head, tip - head), egui::pos2(cx, tip)], stroke);
    painter.line_segment([egui::pos2(cx + head, tip - head), egui::pos2(cx, tip)], stroke);
    // Tray baseline (open box bottom).
    let by = r.bottom() - r.height() * 0.12;
    let pad = r.width() * 0.18;
    let tray = vec![
        egui::pos2(r.left() + pad, by - r.height() * 0.14),
        egui::pos2(r.left() + pad, by),
        egui::pos2(r.right() - pad, by),
        egui::pos2(r.right() - pad, by - r.height() * 0.14),
    ];
    painter.add(egui::Shape::line(tray, stroke));
}

/// Settings: a gear (centre ring with 8 short radial teeth).
fn gear(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let c = r.center();
    let inner = r.width() * 0.22;
    let outer = r.width() * 0.4;
    painter.circle_stroke(c, inner, stroke);
    let teeth = 8;
    for i in 0..teeth {
        let a = (i as f32 / teeth as f32) * std::f32::consts::TAU;
        let (s, co) = a.sin_cos();
        let p0 = egui::pos2(c.x + co * inner, c.y + s * inner);
        let p1 = egui::pos2(c.x + co * outer, c.y + s * outer);
        painter.line_segment([p0, p1], stroke);
    }
}

/// Neutral fallback for unknown names: a (square) box outline.
fn fallback(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    rect_outline(painter, r, stroke);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inset_is_centred_and_scaled() {
        let r = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 80.0));
        let i = inset(r, 0.7);
        // Side = min(w, h) * frac = 80 * 0.7 = 56.
        assert!((i.width() - 56.0).abs() < 1e-3);
        assert!((i.height() - 56.0).abs() < 1e-3);
        // Same centre as the source rect.
        assert!((i.center().x - r.center().x).abs() < 1e-3);
        assert!((i.center().y - r.center().y).abs() < 1e-3);
    }

    #[test]
    fn inset_full_frac_matches_min_side() {
        let r = egui::Rect::from_center_size(egui::pos2(10.0, 10.0), egui::vec2(40.0, 40.0));
        let i = inset(r, 1.0);
        assert!((i.width() - 40.0).abs() < 1e-3);
    }
}
