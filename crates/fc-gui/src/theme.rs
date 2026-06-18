//! Centralized theme and typography for the FlatCAM-RS desktop GUI.
//!
//! This module owns the app's visual identity: the [`Theme`] enum (Light/Dark),
//! the plot-canvas [`Palette`] (grid, axes, rulers, cursor colours), the tuned
//! [`egui::Visuals`], and a single [`Theme::apply_style`] entry point that builds
//! a full [`egui::Style`] (typography + spacing + rounding + visuals) and applies
//! it to a context. It is a pure styling module: no app state, no I/O.
//!
//! The design system is built on the Radix UI colour ramps: the Slate (neutral)
//! scale provides surfaces/borders/text, and the Blue scale provides the accent.
//! Visuals are configured per widget state (inactive/hovered/active/open) so that
//! buttons, combos, sliders, and toggles all read as one coherent, modern system.

use eframe::egui;

// ---------------------------------------------------------------------------
// Colour helpers
// ---------------------------------------------------------------------------

/// Build a `Color32` from a packed `0xRRGGBB` literal.
const fn rgb(hex: u32) -> egui::Color32 {
    egui::Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

// ---------------------------------------------------------------------------
// Design tokens (Radix Slate + Blue)
// ---------------------------------------------------------------------------

/// A resolved set of semantic colour tokens for one theme. Built from the Radix
/// Slate (neutral) and Blue (accent) ramps and consumed by [`Theme::visuals`].
struct Tokens {
    // Neutral surfaces (Slate scale, light → dark within a theme).
    panel: egui::Color32,    // outer panel background
    window: egui::Color32,   // cards / windows / popups
    faint: egui::Color32,    // faint striped/alt rows (step2)
    extreme: egui::Color32,  // recessed inputs / code background
    step3: egui::Color32,    // button at rest
    step4: egui::Color32,    // button hovered
    divider: egui::Color32,  // borders / dividers (step6)
    text_muted: egui::Color32, // secondary text (step11)
    text: egui::Color32,     // primary text (step12)

    // Accent (Blue scale).
    accent: egui::Color32,        // base accent (blue9)
    accent_hover: egui::Color32,  // accent hover (blue10)
    accent_subtle: egui::Color32, // subtle accent wash (blue3) — selection fill
    on_accent: egui::Color32,     // text drawn on top of an accent fill

    // Semantics.
    warning: egui::Color32,
    error: egui::Color32,

    dark: bool,
}

impl Tokens {
    fn light() -> Self {
        Tokens {
            panel: rgb(0xfcfcfd),   // slate step1
            window: rgb(0xffffff),  // white
            faint: rgb(0xf9f9fb),   // slate step2
            extreme: rgb(0xffffff), // white (recessed inputs)
            step3: rgb(0xf0f0f3),   // slate step3
            step4: rgb(0xe8e8ec),   // slate step4
            divider: rgb(0xd9d9e0), // slate step6
            text_muted: rgb(0x60646c), // slate step11
            text: rgb(0x1c2024),    // slate step12
            accent: rgb(0x0090ff),
            accent_hover: rgb(0x0588f0),
            accent_subtle: rgb(0xe6f4fe), // blue subtle-fill
            on_accent: rgb(0xffffff),
            warning: rgb(0xffc53d),
            error: rgb(0xe5484d),
            dark: false,
        }
    }

    fn dark() -> Self {
        Tokens {
            panel: rgb(0x18191b),   // slate step2
            window: rgb(0x212225),  // slate step3
            faint: rgb(0x18191b),   // slate step2
            extreme: rgb(0x1a1b1d), // recessed inputs
            step3: rgb(0x212225),   // slate step3
            step4: rgb(0x272a2d),   // slate step4
            divider: rgb(0x363a3f), // slate step6
            text_muted: rgb(0xb0b4ba), // slate step11
            text: rgb(0xedeef0),    // slate step12
            accent: rgb(0x0090ff),
            accent_hover: rgb(0x3b9eff),
            accent_subtle: rgb(0x0d2847), // dark blue subtle-fill
            on_accent: rgb(0xffffff),
            warning: rgb(0xffc53d),
            error: rgb(0xe5484d),
            dark: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

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
    /// The resolved design tokens for this theme.
    fn tokens(self) -> Tokens {
        match self {
            Theme::Light => Tokens::light(),
            Theme::Dark => Tokens::dark(),
        }
    }

    /// The plot-canvas colour palette for this theme.
    pub fn palette(self) -> Palette {
        match self {
            Theme::Light => Palette {
                plot_bg: rgb(0xffffff),
                margin_bg: rgb(0xf4f4f6),
                grid_minor: rgb(0xe8e8ec),
                grid_major: rgb(0xd9d9e0),
                axis: egui::Color32::from_rgb(224, 122, 122),
                ruler_text: rgb(0x60646c),
                cursor: egui::Color32::from_rgb(214, 40, 40),
            },
            Theme::Dark => Palette {
                plot_bg: rgb(0x0e0e10),
                margin_bg: rgb(0x18191b),
                grid_minor: rgb(0x272a2d),
                grid_major: rgb(0x363a3f),
                axis: egui::Color32::from_rgb(170, 72, 72),
                ruler_text: rgb(0xb0b4ba),
                cursor: egui::Color32::from_rgb(255, 80, 80),
            },
        }
    }

    /// Build a single [`WidgetVisuals`](egui::style::WidgetVisuals) from its parts.
    fn make_widget(
        bg: egui::Color32,
        weak: egui::Color32,
        stroke: egui::Stroke,
        fg: egui::Stroke,
        rounding: egui::Rounding,
        expansion: f32,
    ) -> egui::style::WidgetVisuals {
        egui::style::WidgetVisuals {
            bg_fill: bg,
            weak_bg_fill: weak,
            bg_stroke: stroke,
            rounding,
            fg_stroke: fg,
            expansion,
        }
    }

    /// The fully-built egui visuals for this theme: the stock light/dark base,
    /// then comprehensively overridden per widget state from the Radix tokens.
    pub fn visuals(self) -> egui::Visuals {
        let t = self.tokens();
        let mut v = if t.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        let r6 = egui::Rounding::same(6.0);

        // Per-state widget visuals. `weak_bg_fill` is what buttons paint;
        // `bg_fill` is what checkboxes/combos paint — both set per state.
        v.widgets.noninteractive = Self::make_widget(
            t.panel,
            t.panel,
            egui::Stroke::new(1.0, t.divider),
            egui::Stroke::new(1.0, t.text_muted),
            r6,
            0.0,
        );
        v.widgets.inactive = Self::make_widget(
            t.step3,
            t.step3,
            egui::Stroke::new(1.0, t.divider),
            egui::Stroke::new(1.0, t.text),
            r6,
            0.0,
        );
        v.widgets.hovered = Self::make_widget(
            t.step4,
            t.step4,
            egui::Stroke::new(1.0, t.accent),
            egui::Stroke::new(1.0, t.text),
            r6,
            1.0,
        );
        v.widgets.active = Self::make_widget(
            t.accent,
            t.accent,
            egui::Stroke::new(1.0, t.accent_hover),
            egui::Stroke::new(1.0, t.on_accent),
            r6,
            1.0,
        );
        v.widgets.open = Self::make_widget(
            t.step4,
            t.step4,
            egui::Stroke::new(1.0, t.divider),
            egui::Stroke::new(1.0, t.text),
            r6,
            0.0,
        );

        // Surfaces.
        v.panel_fill = t.panel;
        v.window_fill = t.window;
        v.faint_bg_color = t.faint;
        v.extreme_bg_color = t.extreme;
        v.code_bg_color = t.extreme;

        // Selection / focus.
        v.selection = egui::style::Selection {
            bg_fill: t.accent_subtle,
            stroke: egui::Stroke::new(2.0, t.accent),
        };

        // Semantic foreground colours.
        v.hyperlink_color = t.accent;
        v.warn_fg_color = t.warning;
        v.error_fg_color = t.error;

        // Windows / menus.
        v.window_stroke = egui::Stroke::new(1.0, t.divider);
        v.window_rounding = egui::Rounding::same(8.0);
        v.menu_rounding = egui::Rounding::same(6.0);
        v.window_shadow = egui::epaint::Shadow {
            offset: egui::vec2(0.0, 12.0),
            blur: 24.0,
            spread: 0.0,
            color: egui::Color32::from_black_alpha(96),
        };
        v.popup_shadow = egui::epaint::Shadow {
            offset: egui::vec2(0.0, 6.0),
            blur: 16.0,
            spread: 0.0,
            color: egui::Color32::from_black_alpha(80),
        };

        // Controls behaviour / flair.
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;
        v.striped = true;
        v.dark_mode = t.dark;

        // Let per-state `fg_stroke` drive text colour (so hover/active text
        // changes). Do NOT pin override_text_color.
        v.override_text_color = None;

        v
    }

    /// Build a full egui [`Style`](egui::Style) (typography + spacing + rounding +
    /// visuals) for this theme and apply it to `ctx`.
    pub fn apply_style(self, ctx: &egui::Context) {
        use egui::{FontFamily, FontId, TextStyle};

        let mut style = (*ctx.style()).clone();
        style.visuals = self.visuals();

        // Typography: comfortable, professional sizes via the text-style map.
        // Headings use the dedicated bold family registered by `install_fonts`.
        let bold = FontFamily::Name("ui_bold".into());
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::new(16.0, bold));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(13.5, FontFamily::Proportional));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::new(13.0, FontFamily::Proportional));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::new(11.0, FontFamily::Proportional));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace));

        // Spacing: a tidy, professional feel.
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.window_margin = egui::Margin::same(10.0);
        style.spacing.menu_margin = egui::Margin::same(6.0);
        style.spacing.indent = 14.0;
        style.spacing.interact_size.y = 26.0;
        style.spacing.slider_width = 150.0;
        style.spacing.scroll.bar_width = 9.0;
        style.spacing.scroll.floating = true;

        ctx.set_style(style);
    }
}

// ---------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------

/// Install modern UI fonts, replacing egui's basic bundled face (the single
/// biggest "looks like a toy" tell). Loads native Windows faces: Segoe UI for
/// proportional text (regular + a separate bold file, since egui does not
/// synthesize bold), and Consolas for monospace. Each load is guarded, so a
/// missing file simply leaves egui's fallback in place. If the bold file is
/// missing but the regular one loaded, the `ui_bold` family falls back to the
/// regular face so headings still resolve. A no-op if nothing is found.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut changed = false;
    let mut have_ui = false;
    let mut have_ui_bold = false;

    // Proportional UI text (regular).
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts
            .font_data
            .insert("ui".to_owned(), egui::FontData::from_owned(bytes));
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "ui".to_owned());
        changed = true;
        have_ui = true;
    }

    // Proportional UI text (bold) — a separate file; egui does not synthesize bold.
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeuib.ttf") {
        fonts
            .font_data
            .insert("ui_bold".to_owned(), egui::FontData::from_owned(bytes));
        changed = true;
        have_ui_bold = true;
    }

    // Register the named bold family. Prefer the real bold face; otherwise fall
    // back to the regular "ui" face so `FontFamily::Name("ui_bold")` resolves.
    if have_ui_bold {
        fonts.families.insert(
            egui::FontFamily::Name("ui_bold".into()),
            vec!["ui_bold".to_owned()],
        );
    } else if have_ui {
        fonts.families.insert(
            egui::FontFamily::Name("ui_bold".into()),
            vec!["ui".to_owned()],
        );
    }

    // Monospace (G-code, coordinates).
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\consola.ttf") {
        fonts
            .font_data
            .insert("mono".to_owned(), egui::FontData::from_owned(bytes));
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, "mono".to_owned());
        changed = true;
    }

    if changed {
        ctx.set_fonts(fonts);
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
