//! Exercise the real egui components with isolated, in-process input.
//! No native window, desktop input, worker, profile, network or audio is used.
use super::*;
use egui::accesskit::{Node, Role};

struct ReaderUi {
    ctx: egui::Context,
    app: App,
    nodes: Vec<Node>,
    time: f64,
    size: egui::Vec2,
    scale: f32,
    repaint_delay: Duration,
    done_text_visible: bool,
}

impl ReaderUi {
    fn new(size: egui::Vec2, scale: f32) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = App::new(&ctx, true);
        app.fixture("settings", false).unwrap();
        let mut ui = Self {
            ctx,
            app,
            nodes: vec![],
            time: 0.0,
            size,
            scale,
            repaint_delay: Duration::ZERO,
            done_text_visible: false,
        };
        ui.settle();
        ui
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 0.2;
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(self.scale);
        let output = self.ctx.run(input, |ctx| self.app.draw(ctx));
        self.done_text_visible = output.shapes.iter().any(|clipped| {
            if let egui::Shape::Text(text) = &clipped.shape {
                text.galley.text() == "Done"
                    && clipped
                        .clip_rect
                        .contains_rect(text.galley.rect.translate(text.pos.to_vec2()))
            } else {
                false
            }
        });
        // egui emits the complete current accessibility tree on each pass.
        self.nodes = output
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .map(|(_, node)| node)
            .collect();
        self.repaint_delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
    }

    fn settle(&mut self) {
        for _ in 0..4 {
            self.frame(vec![]);
        }
    }

    fn bounds(&self, role: Role, label: &str) -> egui::Rect {
        let bounds = self
            .nodes
            .iter()
            .find(|node| {
                node.role() == role && (node.label() == Some(label) || node.value() == Some(label))
            })
            .unwrap_or_else(|| panic!("Missing {role:?}: {label}"))
            .bounds()
            .unwrap();
        egui::Rect::from_min_max(
            egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
            egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
        )
    }

    fn click_at(&mut self, pos: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            self.frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }

    fn click(&mut self, label: &str) {
        self.click_at(self.bounds(Role::Button, label).center());
    }

    fn key(&mut self, key: egui::Key) {
        for pressed in [true, false] {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }

    fn assert_reader(&mut self) {
        self.settle();
        assert!(!self.app.settings && !self.app.paste_open);
        assert!(
            !self.nodes.iter().any(|n| n.role() == Role::Window
                && matches!(n.label(), Some("Settings" | "Paste text")))
        );
        assert!(!self.app.closing && !self.app.exit_ready);
        assert_eq!(self.app.cursor, 1);
        assert_eq!(
            self.app.session.document.as_ref().unwrap().title,
            "A quieter way to read"
        );
    }
}

#[test]
fn settings_close_button_returns_to_reader_at_supported_sizes_and_scale() {
    for (size, scale) in [(vec2(1180.0, 820.0), 1.0), (vec2(900.0, 620.0), 1.5)] {
        // Include the left edge: title-bar hit regions must not overlap the X.
        for fraction in [0.1, 0.5, 0.9] {
            let mut ui = ReaderUi::new(size, scale);
            let bounds = ui.bounds(Role::Button, "Close window");
            ui.click_at(egui::pos2(
                bounds.left() + bounds.width() * fraction,
                bounds.center().y,
            ));
            assert!(!ui.app.settings, "The title-bar X must dismiss Settings");
            ui.assert_reader();
        }
    }
}

#[test]
fn settings_done_keeps_preferences_and_can_reopen() {
    let mut ui = ReaderUi::new(vec2(1180.0, 820.0), 1.0);
    assert!(
        ui.done_text_visible,
        "Done must be fully legible at the default size"
    );
    ui.click_at(
        ui.bounds(Role::CheckBox, "Check for updates on startup")
            .center(),
    );
    assert!(ui.app.session.startup_updates && ui.app.dirty);
    ui.size = vec2(900.0, 620.0);
    ui.scale = 1.5;
    ui.settle();
    let done = ui.bounds(Role::Button, "Done");
    assert!(
        ui.done_text_visible,
        "Done must remain fully legible below the scrolling settings"
    );
    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, ui.size).contains_rect(done));
    ui.click("Done");
    ui.assert_reader();
    assert!(ui.app.session.startup_updates && ui.app.dirty);
    ui.click("Settings");
    ui.settle();
    assert!(ui.app.settings);
    ui.click("Done");
    ui.assert_reader();
}

#[test]
fn escape_closes_voice_popup_before_settings() {
    let mut ui = ReaderUi::new(vec2(1180.0, 820.0), 1.0);
    ui.click_at(ui.bounds(Role::ComboBox, "System default").center());
    assert!(egui::Popup::is_any_open(&ui.ctx));
    ui.key(egui::Key::Escape);
    assert!(!egui::Popup::is_any_open(&ui.ctx));
    assert!(ui.app.settings);
    ui.key(egui::Key::Escape);
    ui.assert_reader();
}

#[test]
fn paste_close_and_cancel_preserve_draft() {
    for button in ["Close window", "Cancel"] {
        let mut ui = ReaderUi::new(vec2(1180.0, 820.0), 1.0);
        ui.key(egui::Key::Escape);
        ui.click("Paste text");
        ui.app.paste = "An unfinished draft.".into();
        ui.settle();
        assert!(ui.app.paste_open);
        ui.click(button);
        ui.assert_reader();
        assert_eq!(ui.app.paste, "An unfinished draft.");
    }
}

#[test]
fn escape_closes_focused_paste_editor_without_losing_draft() {
    let mut ui = ReaderUi::new(vec2(1180.0, 820.0), 1.0);
    ui.key(egui::Key::Escape);
    ui.click("Paste text");
    ui.app.paste = "A focused draft.".into();
    ui.settle();
    ui.click_at(
        ui.bounds(Role::MultilineTextInput, "A focused draft.")
            .center(),
    );
    assert!(ui.ctx.wants_keyboard_input());
    ui.key(egui::Key::Escape);
    ui.assert_reader();
    assert_eq!(ui.app.paste, "A focused draft.");
}

#[test]
fn closing_settings_stops_requesting_frames() {
    let mut ui = ReaderUi::new(vec2(1180.0, 820.0), 1.0);
    ui.click("Done");
    ui.assert_reader();
    assert_eq!(ui.repaint_delay, Duration::MAX);
}

#[test]
fn settings_layout_settles() {
    for (size, scale) in [(vec2(1180.0, 820.0), 1.0), (vec2(900.0, 620.0), 1.5)] {
        let ui = ReaderUi::new(size, scale);
        assert!(
            ui.done_text_visible,
            "The footer must finish layout before idle"
        );
        assert_eq!(ui.repaint_delay, Duration::MAX);
    }
}

#[test]
fn translated_settings_and_dark_palette_preserve_reading_state_and_settle() {
    for language in Language::ALL {
        let mut ui = ReaderUi::new(vec2(900.0, 620.0), 1.5);
        ui.app.session.voice = "Installed voice fixture".into();
        ui.app
            .capture_preferences(&ui.ctx, language, storage::Theme::Dark);
        ui.settle();
        assert!(
            ui.nodes
                .iter()
                .any(|node| node.label() == Some(language.text("Settings")))
        );
        assert!(
            ui.nodes
                .iter()
                .any(|node| node.label() == Some(language.text("Done")))
        );
        assert!(ui.ctx.style().visuals.dark_mode);
        assert_eq!(ui.app.cursor, 1);
        assert_eq!(ui.app.session.voice, "Installed voice fixture");
        assert_eq!(ui.repaint_delay, Duration::MAX);
    }
}
