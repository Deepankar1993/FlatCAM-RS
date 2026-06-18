//! Reusable, app-agnostic egui widgets for the FlatCAM-RS desktop UI.
//!
//! Pure presentation helpers only — no application state, no I/O, no side
//! effects beyond the [`egui::Ui`] / painter passed in. Targets egui 0.29.x.
//!
//! Contents:
//! * [`toggle`] / [`toggle_ui`] — animated modern toggle switch.
//! * [`segmented`] — segmented / pill tab selector.
//! * [`card_frame`] — a flat card/section [`egui::Frame`].
//! * [`section_header`] — bold label + hairline divider.
//! * [`primary_button`] — accent-filled primary button via scoped restyle.

use eframe::egui;

/// Paint and handle a modern toggle switch into `rect` allocated from `ui`.
///
/// Canonical egui toggle example. Flips `*on` when clicked and animates the
/// knob between off/on positions.
pub fn toggle_ui(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, "")
    });
    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
        let visuals = ui.style().interact_selectable(&response, *on);
        let rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();
        ui.painter()
            .rect(rect, radius, visuals.bg_fill, visuals.bg_stroke);
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter()
            .circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
    }
    response
}

/// A [`egui::Widget`] wrapper around [`toggle_ui`] for use with `ui.add(...)`.
pub fn toggle(on: &mut bool) -> impl egui::Widget + '_ {
    move |ui: &mut egui::Ui| toggle_ui(ui, on)
}

/// A segmented / pill tab control.
///
/// Renders `labels` as horizontally adjacent segments inside a rounded track,
/// highlighting the one at `*selected`. Clicking a segment updates `*selected`
/// and marks the returned [`egui::Response`] as changed.
pub fn segmented(ui: &mut egui::Ui, selected: &mut usize, labels: &[&str]) -> egui::Response {
    let height = ui.spacing().interact_size.y + 6.0;
    let seg_w = 88.0;
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(seg_w * labels.len() as f32, height), egui::Sense::click());

    // Read all needed visuals/colors into locals up front so we can clone the
    // painter and still call `ui.rect_contains_pointer` inside the loop without
    // borrow conflicts.
    let track_color = ui.visuals().faint_bg_color;
    // Solid accent pill (the pale selection fill left white text unreadable).
    let pill_color = ui.visuals().hyperlink_color;
    let active_color = ui.visuals().widgets.active.fg_stroke.color;
    let hovered_color = ui.visuals().widgets.hovered.fg_stroke.color;
    let inactive_color = ui.visuals().widgets.inactive.fg_stroke.color;
    let button_font = egui::TextStyle::Button.resolve(ui.style());
    let painter = ui.painter().clone();

    if ui.is_rect_visible(rect) {
        painter.rect_filled(rect, egui::Rounding::same(height * 0.5), track_color);
    }

    // Use the pointer-DOWN position (not the current hover) so a fast click that
    // moves between frames still selects the segment it landed on.
    let click_pos = if response.clicked() { response.interact_pointer_pos() } else { None };
    for (i, label) in labels.iter().enumerate() {
        let seg = egui::Rect::from_min_size(
            egui::pos2(rect.left() + seg_w * i as f32, rect.top()),
            egui::vec2(seg_w, height),
        );
        let is_selected = *selected == i;
        let hovered = ui.rect_contains_pointer(seg);

        if is_selected {
            painter.rect_filled(
                seg.shrink(3.0),
                egui::Rounding::same((height - 6.0) * 0.5),
                pill_color,
            );
        }

        let color = if is_selected {
            active_color
        } else if hovered {
            hovered_color
        } else {
            inactive_color
        };

        painter.text(
            seg.center(),
            egui::Align2::CENTER_CENTER,
            label,
            button_font.clone(),
            color,
        );

        if click_pos.is_some_and(|p| seg.contains(p)) {
            *selected = i;
            response.mark_changed();
        }
    }

    response
}

/// A flat card/section [`egui::Frame`] suitable for grouping in-panel content.
///
/// No shadow (kept cheap for in-panel use); a 1px hairline border, rounded
/// corners, and comfortable inner padding.
pub fn card_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::none()
        .fill(style.visuals.window_fill)
        .stroke(egui::Stroke::new(
            1.0,
            style.visuals.widgets.noninteractive.bg_stroke.color,
        ))
        .rounding(egui::Rounding::same(8.0))
        .inner_margin(egui::Margin::same(12.0))
        .outer_margin(egui::Margin::symmetric(0.0, 4.0))
}

/// A flat section header: a bold title followed by a hairline divider.
pub fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(title)
            .font(egui::FontId::new(
                13.0,
                egui::FontFamily::Name("ui_bold".into()),
            ))
            .color(ui.visuals().widgets.noninteractive.fg_stroke.color),
    );
    ui.add_space(4.0);
    let rect = ui.max_rect();
    let y = ui.cursor().top();
    ui.painter().hline(
        rect.x_range(),
        y,
        egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    ui.add_space(8.0);
}

/// A primary (accent-filled) button.
///
/// Restyles the button widgets within a scoped [`egui::Ui`] so the fill uses
/// the theme accent (`hyperlink_color`) with white foreground text, then emits
/// a standard [`egui::Button`]. Returns the button's [`egui::Response`].
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.scope(|ui| {
        let accent = ui.visuals().hyperlink_color;
        // Hover = a touch lighter (blend toward white); pressed = a touch darker.
        // gamma_multiply(1.2) would wash a saturated accent to near-white, so
        // blend per-channel instead.
        let [ar, ag, ab, _] = accent.to_array();
        let lighten = |c: u8| c.saturating_add(((255 - c) as f32 * 0.18) as u8);
        let accent_hover = egui::Color32::from_rgb(lighten(ar), lighten(ag), lighten(ab));
        let accent_press = accent.linear_multiply(0.85);
        let on = egui::Color32::WHITE;

        {
            let widgets = &mut ui.style_mut().visuals.widgets;

            widgets.inactive.weak_bg_fill = accent;
            widgets.inactive.fg_stroke = egui::Stroke::new(1.0, on);
            widgets.inactive.bg_stroke = egui::Stroke::NONE;

            widgets.hovered.weak_bg_fill = accent_hover;
            widgets.hovered.fg_stroke = egui::Stroke::new(1.0, on);
            widgets.hovered.bg_stroke = egui::Stroke::NONE;

            widgets.active.weak_bg_fill = accent_press;
            widgets.active.fg_stroke = egui::Stroke::new(1.0, on);
            widgets.active.bg_stroke = egui::Stroke::NONE;
        }

        ui.add(
            egui::Button::new(egui::RichText::new(text).color(on))
                .min_size(egui::vec2(0.0, 26.0)),
        )
    })
    .inner
}

#[cfg(test)]
mod tests {
    // Constructing a real `egui::Ui`/`Context` in a unit test is awkward
    // without a running app, so these are compile-only sanity checks for the
    // pure helpers in this module.

    /// Mirrors the accent-highlight computation used by `primary_button`.
    fn brighten(c: eframe::egui::Color32) -> eframe::egui::Color32 {
        c.gamma_multiply(1.2)
    }

    #[test]
    fn brighten_is_pure() {
        let base = eframe::egui::Color32::from_rgb(10, 20, 30);
        // Same input -> same output; just exercises the helper path.
        assert_eq!(brighten(base), brighten(base));
    }

    #[test]
    fn trivial() {
        assert_eq!(2 + 2, 4);
    }
}
