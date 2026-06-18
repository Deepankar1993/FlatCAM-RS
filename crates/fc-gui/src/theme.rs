//! Centralized theme and typography for the FlatCAM-RS desktop GUI.
//!
//! This module owns the app's visual identity: the [`Theme`] enum (Light/Dark),
//! the plot-canvas [`Palette`] (grid, axes, rulers, cursor colours), the tuned
//! [`egui::Visuals`], and a single [`Theme::apply_style`] entry point that builds
//! a full [`egui::Style`] (typography + spacing + rounding + visuals) and applies
//! it to a context. It is a pure styling module: no app state, no I/O.

use eframe::egui;

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
        style.spacing.item_spacing = egui::vec2(7.0, 5.0);
        style.spacing.button_padding = egui::vec2(7.0, 4.0);
        style.spacing.menu_margin = egui::Margin::same(6.0);
        style.spacing.window_margin = egui::Margin::same(8.0);

        // Rounding: soften widget corners and windows.
        let rounding = egui::Rounding::same(4.0);
        style.visuals.widgets.inactive.rounding = rounding;
        style.visuals.widgets.hovered.rounding = rounding;
        style.visuals.widgets.active.rounding = rounding;
        style.visuals.widgets.noninteractive.rounding = rounding;
        style.visuals.window_rounding = rounding;

        ctx.set_style(style);
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
