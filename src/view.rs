use super::*;
use crate::theme::{ACCENT, CANVAS, INK, LINE, MUTED, TINT};
use egui::{Color32, RichText, vec2};

const PAGE_SIZE: usize = 5;

fn primary(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
        .fill(ACCENT)
        .min_size(vec2(106.0, 40.0))
}

fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).strong().color(MUTED));
}

impl App {
    fn open_document(&mut self) {
        if self.loaded && !self.file_busy && self.files.try_send(FileCommand::Open).is_ok() {
            self.file_busy = true;
        }
    }
    fn toggle_playback(&mut self) {
        if self.state == State::Playing {
            self.send(SpeechCommand::Pause);
        } else {
            self.play();
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context) {
        if (self.settings || self.paste_open)
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            if self.paste_open {
                self.paste_open = false;
            } else {
                self.settings = false;
            }
        }
        if !self.settings
            && !self.paste_open
            && !ctx.wants_keyboard_input()
            && self.session.document.is_some()
            && self.voice_ready
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space))
        {
            self.toggle_playback();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
            self.open_document();
        }
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(24, 15)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 10, ACCENT);
                    let c = rect.center();
                    ui.painter().line_segment(
                        [c + vec2(0.0, -10.0), c + vec2(0.0, 10.0)],
                        egui::Stroke::new(1.5, Color32::WHITE),
                    );
                    for side in [-1.0, 1.0] {
                        let points = vec![
                            c + vec2(0.0, -7.0),
                            c + vec2(side * 10.0, -10.0),
                            c + vec2(side * 10.0, 7.0),
                            c + vec2(0.0, 10.0),
                        ];
                        ui.painter().add(egui::Shape::line(
                            points,
                            egui::Stroke::new(1.5, Color32::WHITE),
                        ));
                    }
                    ui.add_space(2.0);
                    ui.label(RichText::new("TextReader").size(22.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Settings").clicked() {
                            self.settings = true;
                        }
                        if self.session.document.is_some()
                            && ui
                                .add_enabled(
                                    !self.file_busy
                                        && !self.closing
                                        && !matches!(
                                            self.installation,
                                            Installation::Downloading | Installation::Preparing
                                        ),
                                    egui::Button::new("Export audio"),
                                )
                                .clicked()
                        {
                            self.stop();
                            self.cancel.store(false, Ordering::Relaxed);
                            if self
                                .files
                                .try_send(FileCommand::Export(
                                    self.session.document.as_ref().unwrap().clone(),
                                    self.session.voice.clone(),
                                    self.session.rate,
                                ))
                                .is_ok()
                            {
                                self.file_busy = true;
                            }
                        }
                        if ui
                            .add_enabled(
                                self.loaded && !self.file_busy,
                                egui::Button::new("Paste text"),
                            )
                            .clicked()
                        {
                            self.paste_open = true;
                        }
                        if ui
                            .add_enabled(
                                self.loaded && !self.file_busy,
                                egui::Button::new("Open document"),
                            )
                            .clicked()
                        {
                            self.open_document();
                        }
                    });
                });
            });
        if self.session.document.is_some() {
            self.player_bar(ctx);
        }
        if self.session.document.is_some() {
            self.navigation(ctx);
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(CANVAS).inner_margin(24))
            .show(ctx, |ui| {
                if self.session.document.is_some() {
                    self.reading_page(ui);
                } else {
                    self.empty_page(ui);
                }
            });
        self.dialogs(ctx);
    }

    fn empty_page(&mut self, ui: &mut egui::Ui) {
        ui.add_space((ui.available_height() * 0.16).max(24.0));
        ui.vertical_centered(|ui| {
            caption(ui,"YOUR WORDS, AT YOUR PACE");ui.add_space(16.0);
            ui.label(RichText::new("Make room for reading.").size(38.0).strong());
            ui.add_space(12.0);
            ui.label(RichText::new("Listen to a document, follow the words,\nand pick up right where you left off.").size(18.0).color(MUTED));
            ui.add_space(28.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width()-310.0).max(0.0)/2.0);
                if ui.add_enabled(self.loaded&&!self.file_busy,primary("Open document")).clicked(){self.open_document();}
                if ui.add_enabled(self.loaded&&!self.file_busy,egui::Button::new("Paste some text").min_size(vec2(142.0,40.0))).clicked(){self.paste_open=true;}
            });
            ui.add_space(24.0);ui.label(RichText::new("TXT   ·   Markdown   ·   DOCX   ·   EPUB").color(MUTED));
            ui.add_space(8.0);ui.label(RichText::new("Read locally with the voices on your PC.").size(13.0).color(MUTED));
        });
        self.notice(ui);
    }

    fn navigation(&mut self, ctx: &egui::Context) {
        let doc = self.session.document.clone().unwrap();
        egui::SidePanel::left("navigation")
            .resizable(false)
            .exact_width(242.0)
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(20))
            .show(ctx, |ui| {
                ui.add_space(5.0);
                caption(ui, "IN THIS DOCUMENT");
                ui.add_space(10.0);
                ui.label(RichText::new(&doc.title).size(17.0).strong());
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.bookmark_tab, false, "Contents");
                    ui.selectable_value(&mut self.bookmark_tab, true, "Bookmarks");
                });
                ui.add_space(12.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if !self.bookmark_tab {
                        let active = doc
                            .chapters
                            .partition_point(|c| c.start <= self.cursor)
                            .saturating_sub(1);
                        for (n, chapter) in doc.chapters.iter().enumerate() {
                            let text = format!("{:02}  {}", n + 1, chapter.title);
                            let button = egui::Button::new(
                                RichText::new(text).size(14.0).color(if n == active {
                                    ACCENT
                                } else {
                                    MUTED
                                }),
                            )
                            .truncate()
                            .frame(false)
                            .selected(n == active)
                            .min_size(vec2(ui.available_width(), 48.0));
                            if ui.add(button).on_hover_text(&chapter.title).clicked() {
                                self.seek(chapter.start);
                            }
                        }
                    } else {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.bookmark_label)
                                .hint_text("Bookmark name (optional)")
                                .char_limit(100),
                        );
                        if ui.button("Save this passage").clicked() {
                            self.add_bookmark();
                        }
                        ui.add_space(12.0);
                        let marks = self
                            .session
                            .positions
                            .get(&doc.id)
                            .map(|p| p.bookmarks.clone())
                            .unwrap_or_default();
                        if marks.is_empty() {
                            ui.label(
                                RichText::new("Keep a place you want to return to.").color(MUTED),
                            );
                        }
                        let mut remove = None;
                        for (index, mark) in marks.iter().enumerate() {
                            ui.horizontal(|ui| {
                                if ui
                                    .add_sized(
                                        [ui.available_width() - 32.0, 40.0],
                                        egui::Button::new(&mark.label).wrap().frame(false),
                                    )
                                    .clicked()
                                {
                                    self.seek(mark.chunk);
                                }
                                if ui
                                    .small_button("×")
                                    .on_hover_text("Remove bookmark")
                                    .clicked()
                                {
                                    remove = Some(index);
                                }
                            });
                        }
                        if let Some(index) = remove {
                            self.session
                                .positions
                                .get_mut(&doc.id)
                                .unwrap()
                                .bookmarks
                                .remove(index);
                            self.mark_dirty();
                        }
                    }
                });
            });
    }

    fn add_bookmark(&mut self) {
        if let Some(doc) = &self.session.document {
            let label = if self.bookmark_label.trim().is_empty() {
                doc.chunks[self.cursor].text.chars().take(44).collect()
            } else {
                self.bookmark_label.trim().to_owned()
            };
            let position = self.session.positions.entry(doc.id.clone()).or_default();
            if position.bookmarks.len() < 200
                && !position.bookmarks.iter().any(|b| b.chunk == self.cursor)
            {
                position.bookmarks.push(Bookmark {
                    chunk: self.cursor,
                    label,
                });
                self.mark_dirty();
            }
            self.bookmark_label.clear();
        }
    }

    fn reading_page(&mut self, ui: &mut egui::Ui) {
        let doc = self.session.document.clone().unwrap();
        if self.scroll_to_cursor {
            self.page = self.cursor / PAGE_SIZE;
        }
        let pages = doc.chunks.len().div_ceil(PAGE_SIZE);
        self.page = self.page.min(pages - 1);
        let available = ui.available_rect_before_wrap();
        let width = available.width().min(760.0);
        let bounds = egui::Rect::from_min_max(
            egui::pos2(available.center().x - width / 2.0, available.top()),
            egui::pos2(available.center().x + width / 2.0, available.bottom()),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(bounds)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_max_width(width);
                ui.horizontal(|ui| {
                    caption(ui, "READING ROOM");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Bookmark")
                            .on_hover_text("Save the current passage")
                            .clicked()
                        {
                            self.add_bookmark();
                            self.bookmark_tab = true;
                        }
                    });
                });
                ui.add_space(8.0);
                ui.label(RichText::new(&doc.title).size(30.0).strong());
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Take your time. Your place is saved.")
                        .size(14.0)
                        .color(MUTED),
                );
                ui.add_space(20.0);
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .corner_radius(16)
                    .stroke(egui::Stroke::new(1.0, LINE))
                    .inner_margin(24)
                    .show(ui, |ui| {
                        ui.set_min_width((ui.available_width()).max(100.0));
                        egui::ScrollArea::vertical()
                            .id_salt(("reading_page", self.page))
                            .max_height((ui.available_height() - 105.0).max(140.0))
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                            )
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 4.0;
                                let start = self.page * PAGE_SIZE;
                                let end = (start + PAGE_SIZE).min(doc.chunks.len());
                                for i in start..end {
                                    if i > start
                                        && doc.chunks[i].paragraph != doc.chunks[i - 1].paragraph
                                    {
                                        ui.add_space(8.0);
                                    }
                                    let selected = i == self.cursor;
                                    let frame = egui::Frame::new()
                                        .fill(if selected { TINT } else { Color32::TRANSPARENT })
                                        .corner_radius(8)
                                        .inner_margin(egui::Margin::symmetric(12, 6));
                                    frame.show(ui, |ui| {
                                        let response = ui.add(
                                            egui::Label::new(
                                                RichText::new(
                                                    doc.chunks[i]
                                                        .text
                                                        .trim_start_matches('#')
                                                        .trim_start(),
                                                )
                                                .size(self.session.font_size + 1.0)
                                                .color(if selected {
                                                    INK
                                                } else {
                                                    Color32::from_rgb(67, 83, 90)
                                                }),
                                            )
                                            .wrap()
                                            .sense(egui::Sense::click()),
                                        );
                                        if response.clicked() {
                                            self.seek(i);
                                        }
                                        if selected && self.scroll_to_cursor {
                                            response.scroll_to_me(Some(egui::Align::Center));
                                        }
                                    });
                                }
                            });
                    });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.page > 0,
                            egui::Button::new("← Previous page").frame(false),
                        )
                        .clicked()
                    {
                        self.seek((self.page - 1) * PAGE_SIZE);
                    }
                    ui.label(
                        RichText::new(format!("Page {} of {pages}", self.page + 1)).color(MUTED),
                    );
                    if ui
                        .add_enabled(
                            self.page + 1 < pages,
                            egui::Button::new("Next page →").frame(false),
                        )
                        .clicked()
                    {
                        self.seek((self.page + 1) * PAGE_SIZE);
                    }
                });
                self.scroll_to_cursor = false;
                self.notice(ui);
            },
        );
    }

    fn player_bar(&mut self, ctx: &egui::Context) {
        let ready = self.loaded && self.voice_ready && !self.closing;
        egui::TopBottomPanel::bottom("player")
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(24, 15)),
            )
            .show(ctx, |ui| {
                let doc = self.session.document.as_ref().unwrap();
                let count = doc.chunks.len();
                let title = doc.title.clone();
                ui.add(
                    egui::ProgressBar::new((self.cursor + 1) as f32 / count as f32)
                        .desired_height(3.0)
                        .fill(ACCENT),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).strong().size(13.0));
                        ui.label(
                            RichText::new(format!("Passage {} of {count}", self.cursor + 1))
                                .color(MUTED)
                                .size(12.0),
                        );
                    });
                    ui.add_space(20.0);
                    if ui
                        .add_enabled(ready, egui::Button::new("Previous").frame(false))
                        .on_hover_text("Previous paragraph")
                        .clicked()
                    {
                        let target = self
                            .session
                            .document
                            .as_ref()
                            .unwrap()
                            .paragraph_target(self.cursor, false);
                        self.seek(target);
                    }
                    let label = match self.state {
                        State::Playing => "Pause",
                        State::Paused => "Resume",
                        _ => "Play",
                    };
                    if ui
                        .add_enabled(ready, primary(label))
                        .on_hover_text("Space to play or pause")
                        .clicked()
                    {
                        self.toggle_playback();
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Next").frame(false))
                        .on_hover_text("Next paragraph")
                        .clicked()
                    {
                        let target = self
                            .session
                            .document
                            .as_ref()
                            .unwrap()
                            .paragraph_target(self.cursor, true);
                        self.seek(target);
                    }
                    if ui
                        .add_enabled(
                            ready && matches!(self.state, State::Playing | State::Paused),
                            egui::Button::new("Stop").frame(false),
                        )
                        .clicked()
                    {
                        self.stop();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Aa")
                            .on_hover_text("Text and voice settings")
                            .clicked()
                        {
                            self.settings = true;
                        }
                        ui.checkbox(&mut self.follow, "Follow")
                            .on_hover_text("Follow the current passage");
                    });
                });
            });
    }

    fn notice(&mut self, ui: &mut egui::Ui) {
        if self.file_busy {
            ui.label(RichText::new("Working on your document…").color(MUTED));
        }
        if let Some((label, progress)) = self.progress {
            ui.add(
                egui::ProgressBar::new(progress)
                    .text(format!("{label} · {:.0}%", progress * 100.0)),
            );
            if label == "Exporting WAV" && ui.button("Cancel export").clicked() {
                self.cancel.store(true, Ordering::Relaxed);
            }
        }
        if !self.message.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(&self.message).size(12.0).color(MUTED));
        }
        if self.load_failed {
            ui.colored_label(Color32::DARK_RED,"Saved data could not be read. The original file has been preserved; saving is disabled for this launch.");
        }
        if self.save_failed {
            ui.horizontal(|ui| {
                ui.colored_label(
                    Color32::DARK_RED,
                    "Your latest changes have not been saved.",
                );
                if ui.button("Retry saving").clicked() {
                    self.save_failed = false;
                    self.request_save();
                }
                if ui.button("Close without saving").clicked() {
                    self.dirty = false;
                    self.closing = true;
                    self.installation = Installation::None;
                }
            });
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let mut open = self.settings;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(526.0)
            .default_height((ctx.content_rect().height() - 100.0).max(300.0))
            .default_pos(egui::pos2(
                ((ctx.content_rect().width() - 550.0) / 2.0).max(16.0),
                40.0,
            ))
            // Keep egui's compact title-bar margins: large vertical margins move
            // its close button underneath the title's double-click hit region.
            .frame(egui::Frame::window(&ctx.style()))
            .show(ctx, |ui| {
                egui::Frame::new().inner_margin(18).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .max_height((ctx.content_rect().height() - 240.0).clamp(180.0, 590.0))
                        .show(ui, |ui| self.settings_contents(ui));
                    ui.add_space(14.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(primary("Done"))
                                .on_hover_text("Close settings (Esc)")
                                .clicked()
                            {
                                ui.close_kind(egui::UiKind::Window);
                            }
                        });
                    });
                });
            });
        self.settings = open;
        let mut open = self.paste_open;
        egui::Window::new("Paste text")
            .open(&mut open)
            .default_width(646.0)
            .frame(egui::Frame::window(&ctx.style()))
            .show(ctx, |ui| {
                egui::Frame::new().inner_margin(18).show(ui, |ui| {
                    ui.heading("What would you like to hear?");
                    ui.label(
                        RichText::new("Paste your text below. Blank lines separate paragraphs.")
                            .color(MUTED),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.paste)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(12)
                                    .hint_text("Your next good read starts here…")
                                    .char_limit(4 * 1024 * 1024),
                            );
                        });
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!self.file_busy, primary("Read this text"))
                            .clicked()
                            && self
                                .files
                                .try_send(FileCommand::Paste(self.paste.clone()))
                                .is_ok()
                        {
                            self.file_busy = true;
                            self.paste_open = false;
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_text("Close without clearing your text (Esc)")
                            .clicked()
                        {
                            ui.close_kind(egui::UiKind::Window);
                        }
                    });
                });
            });
        self.paste_open = open && self.paste_open;
    }

    fn settings_contents(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Choose a voice and a comfortable text size.").color(MUTED));
        ui.add_space(12.0);
        caption(ui, "LISTENING");
        ui.add_space(4.0);
        ui.label("Voice");
        let old = self.session.voice.clone();
        let name = self
            .voices
            .iter()
            .find(|v| v.id == self.session.voice)
            .map_or(
                if self.session.voice.is_empty() {
                    "System default"
                } else {
                    "Saved voice unavailable"
                },
                |v| v.name.as_str(),
            );
        egui::ComboBox::from_id_salt("voice")
            .width(400.0)
            .selected_text(name)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.session.voice, String::new(), "System default");
                for voice in &self.voices {
                    ui.selectable_value(&mut self.session.voice, voice.id.clone(), &voice.name);
                }
            });
        ui.label(
            RichText::new("Uses voices installed on this PC.")
                .size(12.0)
                .color(MUTED),
        );
        let rate =
            ui.add(egui::Slider::new(&mut self.session.rate, -10..=10).text("Reading speed"));
        if old != self.session.voice || rate.changed() {
            self.stop();
            self.mark_dirty();
        }
        if ui
            .add(egui::Slider::new(&mut self.session.font_size, 14.0..=32.0).text("Text size"))
            .changed()
        {
            self.mark_dirty();
        }
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(14.0);
        caption(ui, "UPDATES");
        ui.horizontal(|ui| {
            ui.label(concat!("TextReader ", env!("CARGO_PKG_VERSION")));
            if ui
                .add_enabled(!self.net_busy, egui::Button::new("Check for updates"))
                .clicked()
            {
                self.check_updates(false);
            }
        });
        if ui
            .checkbox(
                &mut self.session.startup_updates,
                "Check for updates on startup",
            )
            .changed()
        {
            self.mark_dirty();
        }
        ui.label(RichText::new(&self.update_message).size(12.0).color(MUTED));
        if let Some(update) = self.available.clone()
            && ui
                .add_enabled(
                    !self.net_busy
                        && !self.file_busy
                        && !self.save_failed
                        && !self.load_failed
                        && !self.closing,
                    primary("Download and install"),
                )
                .clicked()
            && self.network.try_send(NetCommand::Install(update)).is_ok()
        {
            self.net_busy = true;
            self.installation = Installation::Downloading;
            self.message =
                "Downloading your update. TextReader will save your place and restart.".into();
        }
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(14.0);
        caption(ui, "YOUR DATA");
        ui.label(RichText::new("Your last document, bookmarks and preferences stay on this PC. Original documents are never changed.").color(MUTED).size(13.0));
        ui.add(
            egui::Label::new(
                RichText::new(storage::data_dir().display().to_string())
                    .monospace()
                    .size(11.0)
                    .color(MUTED),
            )
            .wrap(),
        );
        ui.hyperlink_to("Project and updates", updater::REPOSITORY);
    }
}

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
