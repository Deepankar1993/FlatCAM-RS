//! Centralized theme and typography for the FlatCAM-RS desktop GUI.
//!
//! This module owns the app's visual identity: the [`Theme`] enum (Light/Dark),
//! the plot-canvas [`Palette`] (grid, axes, rulers, cursor colours), the tuned
//! [`egui::Visuals`], and a single [`Theme::apply_style`] entry point that builds
//! a full [`egui::Style`] (typography + spacing + rounding + visuals) and applies
//! it to a context. It is a pure styling module: no app state, no I/O.

use eframe::egui;

/// Install a modern UI font as the default, replacing egui's basic bundled face
/// (the single biggest "looks like a toy" tell). Tries native Windows fonts
/// (Segoe UI for text, Consolas for monospace), keeping egui's faces as glyph
/// fallback. A no-op if none are found, so the app still runs anywhere.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut changed = false;

    // Proportional UI text.
    for path in [r"C:\Windows\Fonts\segoeui.ttf", r"C:\Windows\Fonts\SegoeUI.ttf"] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert("ui".to_owned(), egui::FontData::from_owned(bytes));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "ui".to_owned());
            changed = true;
            break;
        }
    }
    // Monospace (G-code, coordinates).
    for path in [r"C:\Windows\Fonts\consola.ttf", r"C:\Windows\Fonts\Consolas.ttf"] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert("mono".to_owned(), egui::FontData::from_owned(bytes));
            fonts
                .families
                .entry(egui::FontFamily::Monospace)
                .or_default()
                .insert(0, "mono".to_owned());
            changed = true;
            break;
        }
    }
    if changed {
        ctx.set_fonts(fonts);
    }
}

/// The application colour scheme.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// Colours for the plot canvas (grid, axes, rulers, cursor) per theme.
pub struct Palette {
    /// Canvas background.
    pub plot_bg: egui::Color32,
    /// Ruler gutter strip background (slightly off from `plot_bg`).
    pub margin_bg: egui::Color32,
    /// Fine grid lines.
    pub grid_minor: egui::Color32,
    /// Major grid lines.
    pub grid_major: egui::Color32,
    /// Red X/Y origin axes.
    pub axis: egui::Color32,
    /// Axis number labels.
    pub ruler_text: egui::Color32,
    /// Cursor crosshair.
    pub cursor: egui::Color32,
}

impl Theme {
    /// The plot-canvas colour palette for this theme.
    pub fn palette(self) -> Palette {
        match self {
            Theme::Light => Palette {
                plot_bg: egui::Color32::from_gray(252),
                margin_bg: egui::Color32::from_gray(244),
                grid_minor: egui::Color32::from_gray(228),
                grid_major: egui::Color32::from_gray(198),
                axis: egui::Color32::from_rgb(224, 122, 122),
                ruler_text: egui::Color32::from_gray(96),
                cursor: egui::Color32::from_rgb(214, 40, 40),
            },
            Theme::Dark => Palette {
                plot_bg: egui::Color32::from_gray(16),
                margin_bg: egui::Color32::from_gray(24),
                grid_minor: egui::Color32::from_gray(38),
                grid_major: egui::Color32::from_gray(64),
                axis: egui::Color32::from_rgb(170, 72, 72),
                ruler_text: egui::Color32::from_gray(150),
                cursor: egui::Color32::from_rgb(255, 80, 80),
            },
        }
    }

    /// The egui visuals for this theme: the stock light/dark base, lightly tuned.
    pub fn visuals(self) -> egui::Visuals {
        match self {
            Theme::Light => egui::Visuals::light(),
            Theme::Dark => egui::Visuals::dark(),
        }
    }

    /// Build a full egui [`Style`](egui::Style) (typography + spacing + rounding +
    /// visuals) for this theme and apply it to `ctx`.
    pub fn apply_style(self, ctx: &egui::Context) {
        use egui::{FontFamily, FontId, TextStyle};

        let mut style = (*ctx.style()).clone();
        style.visuals = self.visuals();

        // Typography: comfortable, professional sizes via the text-style map.
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(13.5, FontFamily::Proportional));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::new(13.0, FontFamily::Proportional));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::new(10.5, FontFamily::Proportional));

        // Spacing: a tidy, professional feel.
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 5.0);
        style.spacing.menu_margin = egui::Margin::same(6.0);
        style.spacing.window_margin = egui::Margin::same(10.0);
        style.spacing.slider_width = 130.0;
        style.spacing.interact_size.y = 24.0;
        style.spacing.indent = 16.0;

        // Rounding: soften widget corners and windows for a modern look.
        let rounding = egui::Rounding::same(5.0);
        style.visuals.widgets.inactive.rounding = rounding;
        style.visuals.widgets.hovered.rounding = rounding;
        style.visuals.widgets.active.rounding = rounding;
        style.visuals.widgets.noninteractive.rounding = rounding;
        style.visuals.window_rounding = rounding;
        style.visuals.menu_rounding = rounding;

        // Modern accent: a single brand colour drives selection, links, and the
        // hover/active tint, so tabs/toggles/sliders read as one coherent system.
        let accent = self.accent();
        style.visuals.selection.bg_fill = accent.gamma_multiply(0.45);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, accent);
        style.visuals.hyperlink_color = accent;
        // Subtle accent wash on hover/active backgrounds.
        let wash = accent.gamma_multiply(if self == Theme::Dark { 0.30 } else { 0.18 });
        style.visuals.widgets.hovered.weak_bg_fill = wash;
        style.visuals.widgets.active.weak_bg_fill = accent.gamma_multiply(0.40);
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent.gamma_multiply(0.6));

        // Flat, modern panel/canvas tones distinct from the widget surfaces.
        match self {
            Theme::Light => {
                style.visuals.panel_fill = egui::Color32::from_gray(246);
                style.visuals.window_fill = egui::Color32::from_gray(250);
            }
            Theme::Dark => {
                style.visuals.panel_fill = egui::Color32::from_gray(22);
                style.visuals.window_fill = egui::Color32::from_gray(26);
            }
        }

        ctx.set_style(style);
    }

    /// The modern brand/accent colour for this theme (selection, links, hover).
    fn accent(self) -> egui::Color32 {
        match self {
            Theme::Light => egui::Color32::from_rgb(56, 132, 232),
            Theme::Dark => egui::Color32::from_rgb(96, 165, 250),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_are_distinct() {
        let light = Theme::Light.palette();
        let dark = Theme::Dark.palette();
        assert_ne!(light.plot_bg, dark.plot_bg);
        assert_ne!(light.margin_bg, dark.margin_bg);
        assert_ne!(light.cursor, dark.cursor);
    }

    #[test]
    fn default_theme_is_light() {
        assert_eq!(Theme::default(), Theme::Light);
    }
}
