use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

pub const INK: Color32 = Color32::from_rgb(30, 49, 55);
pub const MUTED: Color32 = Color32::from_rgb(101, 118, 124);
pub const ACCENT: Color32 = Color32::from_rgb(16, 116, 105);
pub const TINT: Color32 = Color32::from_rgb(226, 243, 237);
pub const CANVAS: Color32 = Color32::from_rgb(246, 247, 249);
pub const LINE: Color32 = Color32::from_rgb(226, 232, 234);

pub fn apply(ctx: &egui::Context) {
    let mut style = egui::Style {
        visuals: egui::Visuals::light(),
        ..Default::default()
    };
    style.visuals.override_text_color = Some(INK);
    style.visuals.panel_fill = Color32::WHITE;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.faint_bg_color = CANVAS;
    style.visuals.extreme_bg_color = CANVAS;
    style.visuals.selection.bg_fill = TINT;
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(224, 233, 234);
    style.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE);
    style.visuals.widgets.hovered.bg_fill = TINT;
    style.visuals.widgets.hovered.weak_bg_fill = TINT;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
    style.visuals.widgets.active.bg_fill = TINT;
    style.visuals.widgets.active.weak_bg_fill = TINT;
    style.visuals.window_corner_radius = CornerRadius::same(16);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.corner_radius = CornerRadius::same(8);
    }
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(14.0, 9.0);
    style.spacing.interact_size = egui::vec2(36.0, 36.0);
    style.spacing.slider_width = 190.0;
    style.animation_time = 0.10;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(15.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(26.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(12.0, FontFamily::Proportional),
    );
    ctx.set_style(style);
    // Read once before opening a window; no font-file I/O occurs during frames.
    if let Some(root) = std::env::var_os("WINDIR")
        && let Ok(bytes) = std::fs::read(std::path::PathBuf::from(root).join("Fonts/segoeui.ttf"))
    {
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("Segoe UI".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "Segoe UI".into());
        ctx.set_fonts(fonts);
    }
}
