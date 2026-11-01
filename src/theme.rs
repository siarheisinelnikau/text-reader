use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

const INK: Color32 = Color32::from_rgb(30, 49, 55);
const MUTED: Color32 = Color32::from_rgb(101, 118, 124);
pub const ACCENT: Color32 = Color32::from_rgb(16, 116, 105);
const TINT: Color32 = Color32::from_rgb(226, 243, 237);
const CANVAS: Color32 = Color32::from_rgb(246, 247, 249);
const LINE: Color32 = Color32::from_rgb(226, 232, 234);

pub struct Palette {
    pub ink: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub tint: Color32,
    pub canvas: Color32,
    pub line: Color32,
    pub surface: Color32,
    pub error: Color32,
}
pub fn palette(theme: text_reader::storage::Theme) -> Palette {
    match theme {
        text_reader::storage::Theme::Light => Palette {
            ink: INK,
            muted: MUTED,
            accent: ACCENT,
            tint: TINT,
            canvas: CANVAS,
            line: LINE,
            surface: Color32::WHITE,
            error: Color32::from_rgb(163, 35, 43),
        },
        text_reader::storage::Theme::Dark => Palette {
            ink: Color32::from_rgb(230, 239, 239),
            muted: Color32::from_rgb(167, 186, 190),
            accent: Color32::from_rgb(104, 218, 193),
            tint: Color32::from_rgb(31, 66, 61),
            canvas: Color32::from_rgb(18, 27, 31),
            line: Color32::from_rgb(53, 70, 76),
            surface: Color32::from_rgb(26, 38, 43),
            error: Color32::from_rgb(255, 151, 151),
        },
    }
}
pub fn apply(ctx: &egui::Context, theme: text_reader::storage::Theme) {
    // Pin the saved preference before styling its active light/dark branch.
    // Native ThemeChanged events must not replace an explicit app choice.
    ctx.set_theme(match theme {
        text_reader::storage::Theme::Light => egui::Theme::Light,
        text_reader::storage::Theme::Dark => egui::Theme::Dark,
    });
    let palette = palette(theme);
    let mut style = egui::Style {
        visuals: match theme {
            text_reader::storage::Theme::Light => egui::Visuals::light(),
            text_reader::storage::Theme::Dark => egui::Visuals::dark(),
        },
        ..Default::default()
    };
    style.visuals.override_text_color = Some(palette.ink);
    style.visuals.panel_fill = palette.surface;
    style.visuals.window_fill = palette.surface;
    style.visuals.faint_bg_color = palette.canvas;
    style.visuals.extreme_bg_color = palette.canvas;
    style.visuals.selection.bg_fill = palette.tint;
    style.visuals.selection.stroke = Stroke::new(1.0_f32, palette.accent);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, palette.line);
    style.visuals.widgets.inactive.bg_fill = palette.line;
    style.visuals.widgets.inactive.weak_bg_fill = palette.surface;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, palette.line);
    style.visuals.widgets.hovered.bg_fill = palette.tint;
    style.visuals.widgets.hovered.weak_bg_fill = palette.tint;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, palette.accent);
    style.visuals.widgets.active.bg_fill = palette.tint;
    style.visuals.widgets.active.weak_bg_fill = palette.tint;
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
}

pub fn load_fonts(ctx: &egui::Context) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use text_reader::storage::Theme;

    #[test]
    fn saved_theme_wins_over_opposite_system_theme() {
        for saved in [Theme::Light, Theme::Dark] {
            let ctx = egui::Context::default();
            apply(&ctx, saved);
            let requested = match saved {
                Theme::Light => egui::Theme::Light,
                Theme::Dark => egui::Theme::Dark,
            };
            let opposite = match requested {
                egui::Theme::Light => egui::Theme::Dark,
                egui::Theme::Dark => egui::Theme::Light,
            };
            for system in [opposite, requested, opposite] {
                let _ = ctx.run(
                    egui::RawInput {
                        system_theme: Some(system),
                        ..Default::default()
                    },
                    |_| {},
                );
                assert_eq!(ctx.theme(), requested);
                assert_eq!(ctx.style().visuals.dark_mode, saved == Theme::Dark);
                assert_eq!(ctx.style().visuals.panel_fill, palette(saved).surface);
                assert_eq!(
                    ctx.style().visuals.override_text_color,
                    Some(palette(saved).ink)
                );
                assert_eq!(ctx.style().text_styles[&TextStyle::Body].size, 15.0);
            }
        }
    }
}
