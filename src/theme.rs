//! Shared colour palette and egui theme for all shotext GUI windows.
//!
//! Derived from the ilseon / myeon low-sensory palette.

use eframe::egui;

// ── Colour palette ───────────────────────────────────────────────────
pub const MUTED_TEAL: egui::Color32 = egui::Color32::from_rgb(0x5A, 0x9B, 0x80); // #5A9B80
pub const QUIET_AMBER: egui::Color32 = egui::Color32::from_rgb(0xC0, 0x8A, 0x3E); // #C08A3E
pub const MUTED_RED: egui::Color32 = egui::Color32::from_rgb(0xB3, 0x5F, 0x5F); // #B35F5F
pub const BG_LIGHT: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xF2, 0xF2); // #F2F2F2
pub const SURFACE: egui::Color32 = egui::Color32::from_rgb(0xE8, 0xE8, 0xE8); // side panels
pub const SURFACE_BRIGHT: egui::Color32 = egui::Color32::from_rgb(0xFF, 0xFF, 0xFF); // buttons / inputs
pub const TEXT_PRIMARY: egui::Color32 = egui::Color32::from_rgb(0x1A, 0x1A, 0x1A); // #1A1A1A
pub const TEXT_SECONDARY: egui::Color32 = egui::Color32::from_rgb(0x66, 0x66, 0x66); // #666666
pub const BORDER: egui::Color32 = egui::Color32::from_rgb(0xD0, 0xD0, 0xD0); // #D0D0D0

/// Apply the shotext colour theme to the egui context.
pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();

    // Main backgrounds
    visuals.panel_fill = BG_LIGHT;
    visuals.window_fill = BG_LIGHT;
    visuals.extreme_bg_color = SURFACE_BRIGHT; // text-edit backgrounds
    visuals.faint_bg_color = SURFACE;

    // Selection colours (active row, text selection)
    visuals.selection.bg_fill = MUTED_TEAL.gamma_multiply(0.30);
    visuals.selection.stroke = egui::Stroke::new(1.0, MUTED_TEAL);

    // Hyperlink colour
    visuals.hyperlink_color = QUIET_AMBER;

    // Widget styling ─────────────────────────────────────────────
    // Inactive widgets
    visuals.widgets.inactive.bg_fill = SURFACE_BRIGHT;
    visuals.widgets.inactive.weak_bg_fill = SURFACE_BRIGHT;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(0.5, BORDER);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);

    // Hovered widgets
    visuals.widgets.hovered.bg_fill = MUTED_TEAL.gamma_multiply(0.15);
    visuals.widgets.hovered.weak_bg_fill = MUTED_TEAL.gamma_multiply(0.10);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, MUTED_TEAL);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);

    // Active (pressed) widgets
    visuals.widgets.active.bg_fill = MUTED_TEAL.gamma_multiply(0.25);
    visuals.widgets.active.weak_bg_fill = MUTED_TEAL.gamma_multiply(0.20);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, MUTED_TEAL);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);

    // Non-interactive (labels, panels)
    visuals.widgets.noninteractive.bg_fill = BG_LIGHT;
    visuals.widgets.noninteractive.weak_bg_fill = BG_LIGHT;
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(0.5, BORDER);

    // Open (combo-box etc.)
    visuals.widgets.open.bg_fill = SURFACE_BRIGHT;
    visuals.widgets.open.weak_bg_fill = SURFACE_BRIGHT;
    visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, QUIET_AMBER);
    visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, QUIET_AMBER);

    // Separators / window stroke
    visuals.window_stroke = egui::Stroke::new(1.0, BORDER);

    ctx.set_visuals(visuals);
}
