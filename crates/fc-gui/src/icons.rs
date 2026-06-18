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
        "gerber" => gerber_chip(painter, r, stroke),
        "excellon" => excellon_sheet(painter, r, color, stroke),
        "editor" => editor_pencil(painter, r, color, stroke),
        "copy" => copy_glyph(painter, r, stroke),
        "delete" => trash_can(painter, r, stroke),
        "distance" => distance_measure(painter, r, stroke),
        "setorigin" => set_origin(painter, r, color, stroke),
        "milling" => milling_cutter(painter, r, stroke),
        "follow" => follow_path(painter, r, color, stroke),
        "panel" => panel_grid(painter, r, stroke),
        "film" => film_strip(painter, r, stroke),
        "twosided" => two_sided(painter, r, stroke),
        "align" => align_shapes(painter, r, stroke),
        "markers" => map_pin(painter, r, color, stroke),
        "calculators" => calculator(painter, r, stroke),
        "mirror" => mirror_glyph(painter, r, stroke),
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

/// Gerber: a board/IC chip — a body rect with short pin stubs on two sides.
fn gerber_chip(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let body = inset(r, 0.62);
    rect_outline(painter, body, stroke);
    let pins = 3;
    let stub = r.width() * 0.12;
    for i in 0..pins {
        let t = (i as f32 + 1.0) / (pins as f32 + 1.0);
        let y = body.top() + body.height() * t;
        // Left pins.
        painter.line_segment(
            [egui::pos2(body.left(), y), egui::pos2(body.left() - stub, y)],
            stroke,
        );
        // Right pins.
        painter.line_segment(
            [egui::pos2(body.right(), y), egui::pos2(body.right() + stub, y)],
            stroke,
        );
    }
}

/// Excellon: a sheet rect with three small filled circles (drill points).
fn excellon_sheet(
    painter: &egui::Painter,
    r: egui::Rect,
    color: egui::Color32,
    stroke: egui::Stroke,
) {
    rect_outline(painter, r, stroke);
    let dot = STROKE_W * 1.4;
    let pts = [
        egui::pos2(r.left() + r.width() * 0.3, r.top() + r.height() * 0.32),
        egui::pos2(r.left() + r.width() * 0.68, r.top() + r.height() * 0.45),
        egui::pos2(r.left() + r.width() * 0.4, r.top() + r.height() * 0.7),
    ];
    for p in pts {
        painter.circle_filled(p, dot, color);
    }
}

/// Editor: a diagonal pencil with a small node dot.
fn editor_pencil(
    painter: &egui::Painter,
    r: egui::Rect,
    color: egui::Color32,
    stroke: egui::Stroke,
) {
    // Pencil body as a thin diagonal quad from bottom-left to top-right.
    let tip = egui::pos2(r.left() + r.width() * 0.18, r.bottom() - r.height() * 0.18);
    let top = egui::pos2(r.right() - r.width() * 0.18, r.top() + r.height() * 0.18);
    let w = r.width() * 0.12;
    let body = vec![
        egui::pos2(tip.x - w, tip.y - w),
        egui::pos2(top.x - w, top.y - w),
        egui::pos2(top.x + w, top.y + w),
        egui::pos2(tip.x + w, tip.y + w),
    ];
    painter.add(egui::Shape::closed_line(body, stroke));
    // Pencil point.
    painter.line_segment([tip, egui::pos2(tip.x - w, tip.y - w)], stroke);
    painter.line_segment([tip, egui::pos2(tip.x + w, tip.y + w)], stroke);
    // Node dot the pencil works on.
    painter.circle_filled(
        egui::pos2(r.left() + r.width() * 0.18, r.bottom() - r.height() * 0.18),
        STROKE_W * 1.3,
        color,
    );
}

/// Copy: two overlapping rectangles (classic copy glyph).
fn copy_glyph(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let off = r.width() * 0.2;
    let sz = r.width() * 0.55;
    let back = egui::Rect::from_min_max(
        egui::pos2(r.left(), r.top()),
        egui::pos2(r.left() + sz, r.top() + sz),
    );
    let front = egui::Rect::from_min_max(
        egui::pos2(r.left() + off, r.top() + off),
        egui::pos2(r.left() + off + sz, r.top() + off + sz),
    );
    rect_outline(painter, back, stroke);
    rect_outline(painter, front, stroke);
}

/// Delete: a trash can (lid line, body, two vertical ribs).
fn trash_can(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let lid_y = r.top() + r.height() * 0.22;
    // Lid line.
    painter.line_segment(
        [egui::pos2(r.left(), lid_y), egui::pos2(r.right(), lid_y)],
        stroke,
    );
    // Handle on top of the lid.
    let hw = r.width() * 0.16;
    let cx = r.center().x;
    painter.line_segment(
        [egui::pos2(cx - hw, lid_y), egui::pos2(cx - hw, r.top())],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(cx + hw, lid_y), egui::pos2(cx + hw, r.top())],
        stroke,
    );
    painter.line_segment(
        [egui::pos2(cx - hw, r.top()), egui::pos2(cx + hw, r.top())],
        stroke,
    );
    // Body (open-topped box).
    let bx0 = r.left() + r.width() * 0.16;
    let bx1 = r.right() - r.width() * 0.16;
    let body = vec![
        egui::pos2(bx0, lid_y),
        egui::pos2(bx0 + r.width() * 0.05, r.bottom()),
        egui::pos2(bx1 - r.width() * 0.05, r.bottom()),
        egui::pos2(bx1, lid_y),
    ];
    painter.add(egui::Shape::line(body, stroke));
    // Two vertical ribs.
    for t in [0.4_f32, 0.6] {
        let x = bx0 + (bx1 - bx0) * t;
        painter.line_segment(
            [
                egui::pos2(x, lid_y + r.height() * 0.12),
                egui::pos2(x, r.bottom() - r.height() * 0.08),
            ],
            stroke,
        );
    }
}

/// Distance: a horizontal measure line with end ticks and an arrowhead each end.
fn distance_measure(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let y = r.center().y;
    let lx = r.left();
    let rx = r.right();
    // Main line.
    painter.line_segment([egui::pos2(lx, y), egui::pos2(rx, y)], stroke);
    // End ticks (vertical).
    let tick = r.height() * 0.22;
    painter.line_segment([egui::pos2(lx, y - tick), egui::pos2(lx, y + tick)], stroke);
    painter.line_segment([egui::pos2(rx, y - tick), egui::pos2(rx, y + tick)], stroke);
    // Arrowheads pointing outward.
    let a = r.width() * 0.14;
    painter.line_segment([egui::pos2(lx, y), egui::pos2(lx + a, y - a * 0.7)], stroke);
    painter.line_segment([egui::pos2(lx, y), egui::pos2(lx + a, y + a * 0.7)], stroke);
    painter.line_segment([egui::pos2(rx, y), egui::pos2(rx - a, y - a * 0.7)], stroke);
    painter.line_segment([egui::pos2(rx, y), egui::pos2(rx - a, y + a * 0.7)], stroke);
}

/// Set-origin: a corner dot with right + up axes (with arrow ticks).
fn set_origin(painter: &egui::Painter, r: egui::Rect, color: egui::Color32, stroke: egui::Stroke) {
    let o = egui::pos2(r.left() + r.width() * 0.18, r.bottom() - r.height() * 0.18);
    let rx = egui::pos2(r.right(), o.y);
    let uy = egui::pos2(o.x, r.top());
    painter.line_segment([o, rx], stroke);
    painter.line_segment([o, uy], stroke);
    // Arrowhead on X axis.
    let a = r.width() * 0.12;
    painter.line_segment([rx, egui::pos2(rx.x - a, rx.y - a * 0.7)], stroke);
    painter.line_segment([rx, egui::pos2(rx.x - a, rx.y + a * 0.7)], stroke);
    // Arrowhead on Y axis.
    painter.line_segment([uy, egui::pos2(uy.x - a * 0.7, uy.y + a)], stroke);
    painter.line_segment([uy, egui::pos2(uy.x + a * 0.7, uy.y + a)], stroke);
    // Origin dot.
    painter.circle_filled(o, STROKE_W * 1.4, color);
}

/// Milling: a vertical end-mill tool body with a small flute/tip.
fn milling_cutter(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let cx = r.center().x;
    let bw = r.width() * 0.22;
    let body_top = r.top();
    let body_bot = r.center().y + r.height() * 0.1;
    // Shank/body rectangle.
    let body = egui::Rect::from_min_max(
        egui::pos2(cx - bw, body_top),
        egui::pos2(cx + bw, body_bot),
    );
    rect_outline(painter, body, stroke);
    // Flute lines inside the body.
    for t in [0.35_f32, 0.65] {
        let x = body.left() + body.width() * t;
        painter.line_segment(
            [egui::pos2(x, body.top()), egui::pos2(x, body.bottom())],
            stroke,
        );
    }
    // Cutting tip (a small triangle pointing down).
    let tip = vec![
        egui::pos2(cx - bw, body_bot),
        egui::pos2(cx, r.bottom()),
        egui::pos2(cx + bw, body_bot),
    ];
    painter.add(egui::Shape::line(tip, stroke));
}

/// Follow: a dashed polyline path following a contour with small dots at vertices.
fn follow_path(painter: &egui::Painter, r: egui::Rect, color: egui::Color32, stroke: egui::Stroke) {
    let pts = [
        egui::pos2(r.left(), r.bottom()),
        egui::pos2(r.left() + r.width() * 0.3, r.top() + r.height() * 0.25),
        egui::pos2(r.left() + r.width() * 0.6, r.top() + r.height() * 0.55),
        egui::pos2(r.right(), r.top()),
    ];
    // Dashed segments: split each segment into two with a gap.
    for w in pts.windows(2) {
        let a = w[0];
        let b = w[1];
        let m1 = egui::pos2(a.x + (b.x - a.x) * 0.35, a.y + (b.y - a.y) * 0.35);
        let m2 = egui::pos2(a.x + (b.x - a.x) * 0.65, a.y + (b.y - a.y) * 0.65);
        painter.line_segment([a, m1], stroke);
        painter.line_segment([m2, b], stroke);
    }
    for p in pts {
        painter.circle_filled(p, STROKE_W, color);
    }
}

/// Panel: a 2x2 grid of small squares (panelize array).
fn panel_grid(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let gap = r.width() * 0.12;
    let sz = (r.width() - gap) * 0.5;
    for col in 0..2 {
        for row in 0..2 {
            let x = r.left() + col as f32 * (sz + gap);
            let y = r.top() + row as f32 * (sz + gap);
            let cell =
                egui::Rect::from_min_max(egui::pos2(x, y), egui::pos2(x + sz, y + sz));
            rect_outline(painter, cell, stroke);
        }
    }
}

/// Film: a film strip — a rect with sprocket-hole squares along top & bottom.
fn film_strip(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    rect_outline(painter, r, stroke);
    let holes = 4;
    let hw = r.width() * 0.12;
    let hh = r.height() * 0.1;
    let pad = r.width() * 0.06;
    let usable = r.width() - 2.0 * pad - hw;
    for i in 0..holes {
        let x = r.left() + pad + usable * (i as f32 / (holes as f32 - 1.0));
        // Top hole.
        let top = egui::Rect::from_min_max(
            egui::pos2(x, r.top() + hh * 0.4),
            egui::pos2(x + hw, r.top() + hh * 1.4),
        );
        // Bottom hole.
        let bot = egui::Rect::from_min_max(
            egui::pos2(x, r.bottom() - hh * 1.4),
            egui::pos2(x + hw, r.bottom() - hh * 0.4),
        );
        rect_outline(painter, top, stroke);
        rect_outline(painter, bot, stroke);
    }
}

/// Two-sided: two stacked rectangles slightly offset (top/bottom layers).
fn two_sided(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let off = r.width() * 0.18;
    let sz = r.width() * 0.6;
    let top = egui::Rect::from_min_max(
        egui::pos2(r.left(), r.top()),
        egui::pos2(r.left() + sz, r.top() + sz * 0.7),
    );
    let bot = egui::Rect::from_min_max(
        egui::pos2(r.left() + off, r.top() + off),
        egui::pos2(r.left() + off + sz, r.top() + off + sz * 0.7),
    );
    rect_outline(painter, bot, stroke);
    rect_outline(painter, top, stroke);
}

/// Align: two small shapes with a centre alignment cross between them.
fn align_shapes(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let sz = r.width() * 0.26;
    let left = egui::Rect::from_min_max(
        egui::pos2(r.left(), r.center().y - sz * 0.5),
        egui::pos2(r.left() + sz, r.center().y + sz * 0.5),
    );
    let right = egui::Rect::from_min_max(
        egui::pos2(r.right() - sz, r.center().y - sz * 0.5),
        egui::pos2(r.right(), r.center().y + sz * 0.5),
    );
    rect_outline(painter, left, stroke);
    rect_outline(painter, right, stroke);
    // Centre alignment cross.
    let c = r.center();
    let cr = r.width() * 0.12;
    painter.line_segment([egui::pos2(c.x - cr, c.y), egui::pos2(c.x + cr, c.y)], stroke);
    painter.line_segment([egui::pos2(c.x, c.y - cr), egui::pos2(c.x, c.y + cr)], stroke);
}

/// Markers: a map-pin / fiducial — a circle with a centre dot and a small stem.
fn map_pin(painter: &egui::Painter, r: egui::Rect, color: egui::Color32, stroke: egui::Stroke) {
    let c = egui::pos2(r.center().x, r.top() + r.height() * 0.38);
    let rad = r.width() * 0.3;
    painter.circle_stroke(c, rad, stroke);
    painter.circle_filled(c, STROKE_W * 1.3, color);
    // Stem down to a point.
    painter.line_segment(
        [egui::pos2(c.x, c.y + rad), egui::pos2(c.x, r.bottom())],
        stroke,
    );
}

/// Calculators: a rect with a small screen line and a 2x2 grid of buttons.
fn calculator(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    rect_outline(painter, r, stroke);
    // Screen line near the top.
    let sy = r.top() + r.height() * 0.28;
    let pad = r.width() * 0.18;
    let screen = egui::Rect::from_min_max(
        egui::pos2(r.left() + pad, r.top() + r.height() * 0.14),
        egui::pos2(r.right() - pad, sy),
    );
    rect_outline(painter, screen, stroke);
    // 2x2 grid of button dots.
    for col in 0..2 {
        for row in 0..2 {
            let x = r.left() + r.width() * (0.35 + col as f32 * 0.3);
            let y = sy + r.height() * (0.22 + row as f32 * 0.28);
            painter.circle_filled(egui::pos2(x, y), STROKE_W, stroke.color);
        }
    }
}

/// Mirror: a shape and its reflection across a vertical dashed line.
fn mirror_glyph(painter: &egui::Painter, r: egui::Rect, stroke: egui::Stroke) {
    let cx = r.center().x;
    // Vertical dashed mirror line.
    let dashes = 4;
    let seg = r.height() / (dashes as f32 * 2.0 - 1.0);
    for i in 0..dashes {
        let y0 = r.top() + i as f32 * 2.0 * seg;
        painter.line_segment([egui::pos2(cx, y0), egui::pos2(cx, y0 + seg)], stroke);
    }
    // Left triangle pointing toward the line.
    let lpts = vec![
        egui::pos2(r.left(), r.top() + r.height() * 0.2),
        egui::pos2(cx - r.width() * 0.08, r.center().y),
        egui::pos2(r.left(), r.bottom() - r.height() * 0.2),
    ];
    painter.add(egui::Shape::line(lpts, stroke));
    // Right (mirrored) triangle.
    let rpts = vec![
        egui::pos2(r.right(), r.top() + r.height() * 0.2),
        egui::pos2(cx + r.width() * 0.08, r.center().y),
        egui::pos2(r.right(), r.bottom() - r.height() * 0.2),
    ];
    painter.add(egui::Shape::line(rpts, stroke));
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
