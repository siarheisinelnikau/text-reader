use eframe::egui::{self, Color32, RichText};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
use text_reader::{
    document::Document,
    playback::{Audio, Player, State, Voice},
    speech::Sapi,
    storage::{self, Bookmark, Session},
    updater::{self, Update},
};

enum SpeechCommand {
    Start(Arc<Document>, usize, String, i32, u64),
    Pause,
    Resume,
    Stop(u64),
    Quit,
}
enum FileCommand {
    Open,
    Paste(String),
    Export(Arc<Document>, String, i32),
}
enum NetCommand {
    Check(bool),
    Install(Update),
    PrepareInstall(PathBuf),
}
enum Installation {
    None,
    Downloaded(PathBuf),
    Preparing,
    Ready,
}
enum Event {
    Loaded(Result<Session, String>),
    Voices(Result<Vec<Voice>, String>),
    Document(Result<Option<Document>, String>),
    Playback(u64, usize, State),
    SpeechError(String),
    Saved(Result<(), String>),
    Check(bool, Result<Option<Update>, String>),
    Progress(&'static str, f32),
    FileDone(Result<String, String>),
    Downloaded(Result<PathBuf, String>),
    Install(Result<(), String>),
}

fn publish(tx: &mpsc::Sender<Event>, ctx: &egui::Context, event: Event) {
    let _ = tx.send(event);
    ctx.request_repaint();
}

pub struct App {
    session: Session,
    loaded: bool,
    load_failed: bool,
    voices: Vec<Voice>,
    voice_ready: bool,
    cursor: usize,
    state: State,
    epoch: u64,
    settings: bool,
    paste_open: bool,
    paste: String,
    bookmark_label: String,
    message: String,
    update_message: String,
    available: Option<Update>,
    net_busy: bool,
    file_busy: bool,
    progress: Option<(&'static str, f32)>,
    dirty: bool,
    last_save: Instant,
    saving: bool,
    closing: bool,
    installation: Installation,
    save_failed: bool,
    scroll_to_cursor: bool,
    follow: bool,
    speech: SyncSender<SpeechCommand>,
    files: SyncSender<FileCommand>,
    network: SyncSender<NetCommand>,
    saver: SyncSender<Session>,
    events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let (tx, events) = mpsc::channel();
        let (speech, speech_rx) = mpsc::sync_channel(16);
        let (files, file_rx) = mpsc::sync_channel(2);
        let (network, net_rx) = mpsc::sync_channel(2);
        let (saver, save_rx) = mpsc::sync_channel::<Session>(1);
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let result = Sapi::new();
                let audio = match result {
                    Ok(a) => a,
                    Err(e) => {
                        publish(&tx, &ctx, Event::Voices(Err(format!("{e:#}"))));
                        return;
                    }
                };
                publish(
                    &tx,
                    &ctx,
                    Event::Voices(audio.voices().map_err(|e| e.to_string())),
                );
                let mut player = Player::new(audio);
                let mut epoch = 0;
                loop {
                    let command = if player.state == State::Playing {
                        match speech_rx.recv_timeout(Duration::from_millis(40)) {
                            Ok(c) => Some(c),
                            Err(mpsc::RecvTimeoutError::Timeout) => None,
                            Err(_) => break,
                        }
                    } else {
                        match speech_rx.recv() {
                            Ok(c) => Some(c),
                            Err(_) => break,
                        }
                    };
                    let result = match command {
                        Some(SpeechCommand::Start(doc, cursor, voice, rate, generation)) => {
                            epoch = generation;
                            player.start(doc, cursor, voice, rate)
                        }
                        Some(SpeechCommand::Pause) => player.pause(),
                        Some(SpeechCommand::Resume) => player.resume(),
                        Some(SpeechCommand::Stop(generation)) => {
                            epoch = generation;
                            player.stop()
                        }
                        Some(SpeechCommand::Quit) => break,
                        None => match player.tick() {
                            Ok(true) => Ok(()),
                            Ok(false) => continue,
                            Err(e) => Err(e),
                        },
                    };
                    if let Err(e) = result {
                        let _ = player.stop();
                        publish(
                            &tx,
                            &ctx,
                            Event::SpeechError(format!("Speech failed: {e:#}")),
                        );
                    }
                    publish(
                        &tx,
                        &ctx,
                        Event::Playback(epoch, player.cursor, player.state),
                    );
                }
                let _ = player.stop();
            });
        }
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                while let Ok(command) = file_rx.recv() {
                    match command {
                        FileCommand::Open => {
                            let result = (|| -> anyhow::Result<Option<Document>> {
                                let Some(path) = rfd::FileDialog::new()
                                    .add_filter(
                                        "Readable documents",
                                        &["txt", "md", "docx", "epub"],
                                    )
                                    .add_filter("All files", &["*"])
                                    .pick_file()
                                else {
                                    return Ok(None);
                                };
                                Ok(Some(text_reader::extract::open(&path)?))
                            })();
                            publish(
                                &tx,
                                &ctx,
                                Event::Document(result.map_err(|e| format!("{e:#}"))),
                            );
                        }
                        FileCommand::Paste(text) => publish(
                            &tx,
                            &ctx,
                            Event::Document(
                                Document::from_text("Pasted document".into(), text)
                                    .map(Some)
                                    .map_err(|e| format!("{e:#}")),
                            ),
                        ),
                        FileCommand::Export(doc, voice, rate) => {
                            let result = (|| -> anyhow::Result<String> {
                                let Some(path) = rfd::FileDialog::new()
                                    .add_filter("PCM WAV audio", &["wav"])
                                    .set_file_name("TextReader-audio.wav")
                                    .save_file()
                                else {
                                    return Ok("Export cancelled.".into());
                                };
                                anyhow::ensure!(
                                    path.extension()
                                        .is_some_and(|e| e.eq_ignore_ascii_case("wav")),
                                    "Choose a .wav filename. MP3 is not supported."
                                );
                                let mut last = -1i32;
                                text_reader::speech::export(
                                    &doc,
                                    &path,
                                    &voice,
                                    rate,
                                    &cancel,
                                    |p| {
                                        let percent = (p * 100.0) as i32;
                                        if percent != last {
                                            last = percent;
                                            publish(&tx, &ctx, Event::Progress("Exporting WAV", p));
                                        }
                                    },
                                )?;
                                Ok(format!("WAV saved to {}", path.display()))
                            })();
                            publish(
                                &tx,
                                &ctx,
                                Event::FileDone(result.map_err(|e| format!("{e:#}"))),
                            );
                        }
                    }
                }
            });
        }
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                while let Ok(command) = net_rx.recv() {
                    match command {
                        NetCommand::Check(startup) => publish(
                            &tx,
                            &ctx,
                            Event::Check(startup, updater::check().map_err(|e| format!("{e:#}"))),
                        ),
                        NetCommand::Install(update) => {
                            let result = (|| -> anyhow::Result<PathBuf> {
                                let dir = storage::data_dir().join("updates");
                                let mut last = -1i32;
                                let staged = updater::download(&update, &dir, |p| {
                                    let percent = (p * 100.0) as i32;
                                    if percent != last {
                                        last = percent;
                                        publish(
                                            &tx,
                                            &ctx,
                                            Event::Progress("Downloading update", p),
                                        );
                                    }
                                })?;
                                Ok(staged)
                            })();
                            publish(
                                &tx,
                                &ctx,
                                Event::Downloaded(result.map_err(|e| format!("{e:#}"))),
                            );
                        }
                        NetCommand::PrepareInstall(staged) => publish(
                            &tx,
                            &ctx,
                            Event::Install(
                                updater::stage_install(
                                    &staged,
                                    &storage::data_dir().join("updates"),
                                )
                                .map_err(|e| format!("{e:#}")),
                            ),
                        ),
                    }
                }
            });
        }
        {
            let tx = tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                publish(
                    &tx,
                    &ctx,
                    Event::Loaded(
                        storage::load(&storage::data_dir()).map_err(|e| format!("{e:#}")),
                    ),
                );
                let error = storage::data_dir().join("update-error.txt");
                if let Ok(message) = std::fs::read_to_string(&error) {
                    publish(&tx, &ctx, Event::FileDone(Err(message)));
                    let _ = std::fs::remove_file(error);
                }
                while let Ok(session) = save_rx.recv() {
                    publish(
                        &tx,
                        &ctx,
                        Event::Saved(
                            storage::save(&storage::data_dir(), &session)
                                .map_err(|e| format!("Cannot save reading data: {e:#}")),
                        ),
                    );
                }
            });
        }
        Self {
            session: Session::default(),
            loaded: false,
            load_failed: false,
            voices: vec![],
            voice_ready: false,
            cursor: 0,
            state: State::Stopped,
            epoch: 0,
            settings: false,
            paste_open: false,
            paste: String::new(),
            bookmark_label: String::new(),
            message: "Open a document or paste text to begin.".into(),
            update_message: "No check has completed yet.".into(),
            available: None,
            net_busy: false,
            file_busy: false,
            progress: None,
            dirty: false,
            last_save: Instant::now(),
            saving: false,
            closing: false,
            installation: Installation::None,
            save_failed: false,
            scroll_to_cursor: false,
            follow: true,
            speech,
            files,
            network,
            saver,
            events,
            cancel,
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }
    fn send(&mut self, c: SpeechCommand) {
        if self.speech.try_send(c).is_err() {
            self.message = "Speech worker is unavailable or busy. Try again.".into();
        }
    }
    fn seek(&mut self, cursor: usize) {
        self.epoch += 1;
        self.send(SpeechCommand::Stop(self.epoch));
        self.state = State::Stopped;
        if let Some(doc) = &self.session.document {
            self.cursor = cursor.min(doc.chunks.len() - 1);
            self.session
                .positions
                .entry(doc.id.clone())
                .or_default()
                .cursor = self.cursor;
            self.mark_dirty();
            self.scroll_to_cursor = true;
        }
    }
    fn stop(&mut self) {
        self.seek(self.cursor);
    }
    fn play(&mut self) {
        if self.state == State::Paused {
            self.send(SpeechCommand::Resume);
        } else if let Some(doc) = &self.session.document {
            self.epoch += 1;
            self.send(SpeechCommand::Start(
                doc.clone(),
                self.cursor,
                self.session.voice.clone(),
                self.session.rate,
                self.epoch,
            ));
        }
    }
    fn request_save(&mut self) {
        if self.dirty
            && !self.saving
            && !self.load_failed
            && !self.save_failed
            && self.saver.try_send(self.session.clone()).is_ok()
        {
            self.saving = true;
            self.dirty = false;
            self.last_save = Instant::now();
        }
    }
    fn check_updates(&mut self, startup: bool) {
        if !self.net_busy && self.network.try_send(NetCommand::Check(startup)).is_ok() {
            self.net_busy = true;
            self.update_message = "Checking the canonical GitHub repository…".into();
        }
    }
    fn process_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Loaded(result) => {
                    self.loaded = true;
                    match result {
                        Ok(s) => {
                            self.session = s;
                            if let Some(d) = &self.session.document {
                                self.cursor = self
                                    .session
                                    .positions
                                    .get(&d.id)
                                    .map_or(0, |p| p.cursor)
                                    .min(d.chunks.len() - 1);
                                self.message = format!("Restored {}", d.title);
                            }
                        }
                        Err(e) => {
                            self.message = e;
                            self.load_failed = true;
                        }
                    }
                    if !self.load_failed && self.session.startup_updates {
                        self.check_updates(true);
                    }
                }
                Event::Voices(result) => match result {
                    Ok(v) => {
                        self.voices = v;
                        self.voice_ready = true;
                    }
                    Err(e) => self.message = e,
                },
                Event::Document(result) => {
                    self.file_busy = false;
                    match result {
                        Ok(Some(doc)) => {
                            self.stop();
                            self.cursor = self
                                .session
                                .positions
                                .get(&doc.id)
                                .map_or(0, |p| p.cursor)
                                .min(doc.chunks.len() - 1);
                            self.message =
                                format!("Loaded {} — {} fragments", doc.title, doc.chunks.len());
                            self.session.document = Some(Arc::new(doc));
                            self.mark_dirty();
                            self.scroll_to_cursor = true;
                        }
                        Ok(None) => {}
                        Err(e) => self.message = e,
                    }
                }
                Event::Playback(epoch, cursor, state) if epoch == self.epoch => {
                    self.state = state;
                    if state != State::Stopped {
                        self.cursor = cursor;
                        self.scroll_to_cursor = self.follow;
                        if let Some(doc) = &self.session.document {
                            self.session
                                .positions
                                .entry(doc.id.clone())
                                .or_default()
                                .cursor = cursor;
                            self.mark_dirty();
                        }
                    }
                }
                Event::Playback(_, _, _) => {}
                Event::SpeechError(e) => {
                    self.message = e;
                    self.state = State::Stopped;
                }
                Event::Saved(result) => {
                    self.saving = false;
                    if let Err(e) = result {
                        self.message = e;
                        self.closing = false;
                        self.installation = Installation::None;
                        self.net_busy = false;
                        self.save_failed = true;
                        self.dirty = true;
                    }
                }
                Event::Check(startup, result) => {
                    self.net_busy = false;
                    match result {
                        Ok(Some(update)) => {
                            self.update_message =
                                format!("Version {} is available.", update.version);
                            self.message = self.update_message.clone();
                            self.available = Some(update);
                        }
                        Ok(None) => self.update_message = "You are up to date.".into(),
                        Err(e) => {
                            self.update_message = e.clone();
                            if !startup {
                                self.message = e;
                            }
                        }
                    }
                }
                Event::Progress(label, p) => self.progress = Some((label, p)),
                Event::FileDone(result) => {
                    self.file_busy = false;
                    self.progress = None;
                    self.message = result.unwrap_or_else(|e| e);
                }
                Event::Downloaded(result) => {
                    self.progress = None;
                    match result {
                        Ok(path) => {
                            self.stop();
                            self.installation = Installation::Downloaded(path);
                            self.closing = true;
                            self.cancel.store(true, Ordering::Relaxed);
                            self.mark_dirty();
                        }
                        Err(e) => {
                            self.net_busy = false;
                            self.message = e;
                        }
                    }
                }
                Event::Install(result) => {
                    self.net_busy = false;
                    match result {
                        Ok(()) => self.installation = Installation::Ready,
                        Err(e) => {
                            self.installation = Installation::None;
                            self.closing = false;
                            self.message = e;
                        }
                    }
                }
            }
        }
        if self.dirty && (self.closing || self.last_save.elapsed() > Duration::from_secs(2)) {
            self.request_save();
        }
        if (self.dirty && !self.save_failed && !self.load_failed) || self.saving {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        if self.closing && !self.saving && !self.dirty {
            match &self.installation {
                Installation::Downloaded(path) => {
                    if self
                        .network
                        .try_send(NetCommand::PrepareInstall(path.clone()))
                        .is_ok()
                    {
                        self.installation = Installation::Preparing;
                    }
                }
                Installation::Preparing => {}
                Installation::None | Installation::Ready => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close)
                }
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("TextReader").color(Color32::from_rgb(25, 92, 100)));
            ui.separator();
            if ui
                .add_enabled(
                    self.loaded && !self.file_busy,
                    egui::Button::new("Open document…"),
                )
                .clicked()
                && self.files.try_send(FileCommand::Open).is_ok()
            {
                self.file_busy = true;
            }
            if ui
                .add_enabled(
                    self.loaded && !self.file_busy,
                    egui::Button::new("Paste text…"),
                )
                .clicked()
            {
                self.paste_open = true;
            }
            if ui.button("Settings").clicked() {
                self.settings = true;
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let ready =
                self.loaded && self.voice_ready && self.session.document.is_some() && !self.closing;
            if ui
                .add_enabled(
                    ready && self.state != State::Playing,
                    egui::Button::new(if self.state == State::Paused {
                        "Resume"
                    } else {
                        "Play"
                    }),
                )
                .clicked()
            {
                self.play();
            }
            if ui
                .add_enabled(self.state == State::Playing, egui::Button::new("Pause"))
                .clicked()
            {
                self.send(SpeechCommand::Pause);
            }
            if ui.add_enabled(ready, egui::Button::new("Stop")).clicked() {
                self.stop();
            }
            if ui
                .add_enabled(ready, egui::Button::new("Previous paragraph"))
                .clicked()
            {
                let n = self
                    .session
                    .document
                    .as_ref()
                    .unwrap()
                    .paragraph_target(self.cursor, false);
                self.seek(n);
            }
            if ui
                .add_enabled(ready, egui::Button::new("Next paragraph"))
                .clicked()
            {
                let n = self
                    .session
                    .document
                    .as_ref()
                    .unwrap()
                    .paragraph_target(self.cursor, true);
                self.seek(n);
            }
            if ui
                .add_enabled(
                    ready && !self.file_busy && !self.net_busy,
                    egui::Button::new("Export WAV…"),
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
        });
        ui.horizontal(|ui| {
            ui.label(format!("{:?}", self.state));
            ui.checkbox(&mut self.follow, "Follow current fragment");
            if let Some(doc) = &self.session.document {
                ui.label(format!(
                    "Fragment {} / {}",
                    self.cursor + 1,
                    doc.chunks.len()
                ));
            }
        });
        if let Some((label, p)) = self.progress {
            ui.add(egui::ProgressBar::new(p).text(format!("{label} — {:.0}%", p * 100.0)));
            if label == "Exporting WAV" && ui.button("Cancel export").clicked() {
                self.cancel.store(true, Ordering::Relaxed);
            }
        }
        ui.separator();
    }

    fn reader(&mut self, ui: &mut egui::Ui) {
        let Some(doc) = self.session.document.clone() else {
            ui.add_space(50.0);
            ui.heading("Make room for reading.");
            ui.label("Open TXT, Markdown, DOCX or EPUB, or paste text. Your installed Windows voices read locally.");
            ui.label("The current sentence or bounded fragment is highlighted. Click any fragment to choose a starting point.");
            ui.label("PDF, OCR, DRM and MP3 are not supported in this version.");
            return;
        };
        egui::Panel::left("navigation")
            .resizable(true)
            .default_size(220.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Chapters");
                    let active_chapter = doc
                        .chapters
                        .partition_point(|c| c.start <= self.cursor)
                        .saturating_sub(1);
                    for (index, chapter) in doc.chapters.iter().enumerate() {
                        if ui
                            .selectable_label(index == active_chapter, &chapter.title)
                            .clicked()
                        {
                            self.seek(chapter.start);
                        }
                    }
                    ui.add_space(18.0);
                    ui.heading("Bookmarks");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.bookmark_label)
                            .hint_text("Optional bookmark name")
                            .char_limit(100),
                    );
                    if ui.button("Bookmark this fragment").clicked() {
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
                    let marks = self
                        .session
                        .positions
                        .get(&doc.id)
                        .map(|p| p.bookmarks.clone())
                        .unwrap_or_default();
                    for (i, b) in marks.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.button(&b.label).clicked() {
                                self.seek(b.chunk);
                            }
                            if ui
                                .small_button("×")
                                .on_hover_text("Remove bookmark")
                                .clicked()
                            {
                                self.session
                                    .positions
                                    .get_mut(&doc.id)
                                    .unwrap()
                                    .bookmarks
                                    .remove(i);
                                self.mark_dirty();
                            }
                        });
                    }
                });
            });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading(&doc.title);
            ui.label("Local speech · fragment highlighting · saved reading position");
            ui.separator();
            let height = self.session.font_size + 22.0;
            let mut scroll = egui::ScrollArea::vertical()
                .id_salt("reader")
                .max_height((ui.available_height() - 130.0).max(100.0));
            if self.scroll_to_cursor {
                scroll = scroll.vertical_scroll_offset(
                    (self.cursor as f32 * (height + ui.spacing().item_spacing.y)
                        - ui.available_height() * 0.3)
                        .max(0.0),
                );
            }
            scroll.show_rows(ui, height, doc.chunks.len(), |ui, range| {
                // Fixed-height rows keep layout bounded even for long documents. Full text is available on hover.
                for i in range {
                    let selected = i == self.cursor;
                    let text = RichText::new(&doc.chunks[i].text)
                        .size(self.session.font_size)
                        .color(if selected {
                            Color32::from_rgb(15, 76, 82)
                        } else {
                            Color32::from_rgb(45, 50, 58)
                        });
                    let response = ui.add_sized(
                        [ui.available_width(), height],
                        egui::Button::new(text)
                            .selected(selected)
                            .truncate()
                            .frame(selected),
                    );
                    if response.clicked() {
                        self.seek(i);
                    }
                    response.clone().on_hover_text(&doc.chunks[i].text);
                    if selected && self.scroll_to_cursor {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                }
            });
            // Virtual rows outside the current viewport need an explicit offset to become visible.
            if self.scroll_to_cursor {
                self.scroll_to_cursor = false;
            }
            ui.separator();
            ui.label(
                RichText::new(&doc.chunks[self.cursor].text)
                    .size(self.session.font_size + 2.0)
                    .color(Color32::from_rgb(15, 76, 82)),
            );
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let mut open = self.settings;
        egui::Window::new("Settings").open(&mut open).resizable(true).default_width(520.0).show(ctx,|ui| {
            egui::ScrollArea::vertical().show(ui,|ui| {
                ui.heading("Reading");
                let name=self.voices.iter().find(|v|v.id==self.session.voice).map(|v|v.name.as_str()).unwrap_or(if self.session.voice.is_empty(){"System default"}else{"Saved voice unavailable"});
                let old=self.session.voice.clone();
                egui::ComboBox::from_label("Installed SAPI voice").selected_text(name).show_ui(ui,|ui| {
                    ui.selectable_value(&mut self.session.voice,String::new(),"System default");
                    for v in &self.voices {ui.selectable_value(&mut self.session.voice,v.id.clone(),&v.name);}
                });
                let rate=ui.add(egui::Slider::new(&mut self.session.rate,-10..=10).text("SAPI speed (0 = normal)"));
                if old!=self.session.voice || rate.changed(){self.stop();self.mark_dirty();}
                ui.label("Voice and speed changes stop playback. Press Play to use the new setting.");
                if ui.add(egui::Slider::new(&mut self.session.font_size,14.0..=32.0).text("Text size")).changed(){self.mark_dirty();}
                ui.separator();ui.heading("Updates");
                ui.label(concat!("Installed version: ",env!("CARGO_PKG_VERSION")," · Windows x64"));
                if ui.checkbox(&mut self.session.startup_updates,"Check for updates on startup").changed(){self.mark_dirty();}
                if ui.add_enabled(!self.net_busy,egui::Button::new("Check for updates")).clicked(){self.check_updates(false);}
                ui.label(&self.update_message);
                if let Some(update)=self.available.clone()
                    && ui.add_enabled(!self.net_busy && !self.file_busy && !self.load_failed && !self.save_failed && !self.closing,egui::Button::new("Download and install")).clicked()
                    && self.network.try_send(NetCommand::Install(update)).is_ok() {self.net_busy=true;self.message="Downloading a verified update. TextReader will save your session and restart after installation.".into();}
                ui.label("Downloads start only when you click Download and install. A published executable and SHA256SUMS.txt are required.");
                ui.hyperlink_to("Canonical repository (planned)",updater::REPOSITORY);
                ui.separator();ui.heading("Local data");
                ui.label("The last imported text, reading positions, bookmarks and preferences are saved in %LOCALAPPDATA%\\TextReader. Source documents are never changed. No document text is sent by the updater.");
                ui.label("Close the app before backing up or deleting that folder. Recent document text is stored unencrypted.");
            });
        });
        self.settings = open;
        let mut open = self.paste_open;
        egui::Window::new("Paste a document").open(&mut open).default_size([650.0,420.0]).show(ctx,|ui| {
            ui.label("Paste up to 4 MiB of text. Blank lines separate paragraphs; # headings mark chapters.");
            egui::ScrollArea::vertical().max_height(300.0).show(ui,|ui|{ui.add(egui::TextEdit::multiline(&mut self.paste).desired_rows(14).desired_width(f32::INFINITY).char_limit(4*1024*1024));});
            if ui.add_enabled(!self.file_busy,egui::Button::new("Read this text")).clicked()
                && self.files.try_send(FileCommand::Paste(std::mem::take(&mut self.paste))).is_ok(){self.file_busy=true;self.paste_open=false;}
        });
        self.paste_open = open && self.paste_open;
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.process_events(&ctx);
        if ctx.input(|i| i.viewport().close_requested())
            && (!self.closing || self.saving || self.dirty)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.load_failed {
                self.stop();
                self.closing = true;
                self.cancel.store(true, Ordering::Relaxed);
                self.request_save();
            } else {
                self.dirty = false;
                self.closing = true;
            }
            ctx.request_repaint();
        }
        self.toolbar(ui);
        ui.label(&self.message);
        if self.file_busy && self.progress.is_none() {
            ui.label("Working…");
        }
        if self.closing {
            ui.label("Saving your session before closing…");
        }
        if self.load_failed {
            ui.colored_label(Color32::DARK_RED,"Session could not be loaded. Automatic saving is disabled to preserve the original. Close TextReader and back up/reset session.json to restore saving.");
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
        ui.separator();
        self.reader(ui);
        self.dialogs(&ctx);
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.speech.try_send(SpeechCommand::Quit);
    }
}
