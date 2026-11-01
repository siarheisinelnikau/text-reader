use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
use text_reader::{
    document::Document,
    i18n::{Language, Message},
    playback::{Audio, Player, State, Voice},
    speech::Sapi,
    storage::{self, Bookmark, Session},
    updater::{self, Prepared, Update},
};

enum SpeechCommand {
    Start(Arc<Document>, usize, String, i32, u64),
    Pause,
    Resume,
    Stop(u64),
    Quit,
}
enum FileCommand {
    Open(Language),
    Paste(String, Language),
    Export(Arc<Document>, String, i32, Language),
}
enum NetCommand {
    Check(bool),
    Install(Update),
    PrepareInstall(Prepared),
}
enum Installation {
    None,
    Downloading,
    Downloaded(Prepared),
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
    FileDone(Result<Message, String>),
    Downloaded(Result<Prepared, String>),
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
    message: Message,
    update_message: Message,
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
    page: usize,
    bookmark_tab: bool,
    pub exit_ready: bool,
    capture: bool,
    speech: SyncSender<SpeechCommand>,
    files: SyncSender<FileCommand>,
    network: SyncSender<NetCommand>,
    saver: SyncSender<Session>,
    events: Receiver<Event>,
    cancel: Arc<AtomicBool>,
}

impl App {
    pub fn new(ctx: &egui::Context, capture: bool) -> Self {
        crate::diagnostics::event("CPU renderer selected; constructing reader");
        crate::theme::apply(ctx, storage::Theme::default());
        crate::theme::load_fonts(ctx);
        let (tx, events) = mpsc::channel();
        let (speech, speech_rx) = mpsc::sync_channel(16);
        let (files, file_rx) = mpsc::sync_channel(2);
        let (network, net_rx) = mpsc::sync_channel(2);
        let (saver, save_rx) = mpsc::sync_channel::<Session>(1);
        let cancel = Arc::new(AtomicBool::new(false));
        if !capture {
            {
                let tx = tx.clone();
                let ctx = ctx.clone();
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
                let ctx = ctx.clone();
                let cancel = cancel.clone();
                std::thread::spawn(move || {
                    while let Ok(command) = file_rx.recv() {
                        match command {
                            FileCommand::Open(language) => {
                                let result = (|| -> anyhow::Result<Option<Document>> {
                                    let Some(path) = rfd::FileDialog::new()
                                        .add_filter(
                                            language.text("Readable documents"),
                                            &["txt", "md", "docx", "epub"],
                                        )
                                        .add_filter(language.text("All files"), &["*"])
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
                            FileCommand::Paste(text, language) => publish(
                                &tx,
                                &ctx,
                                Event::Document(
                                    Document::from_text(
                                        language.text("Pasted document").into(),
                                        text,
                                    )
                                    .map(Some)
                                    .map_err(|e| format!("{e:#}")),
                                ),
                            ),
                            FileCommand::Export(doc, voice, rate, language) => {
                                let result = (|| -> anyhow::Result<Message> {
                                    let Some(path) = rfd::FileDialog::new()
                                        .add_filter(language.text("PCM WAV audio"), &["wav"])
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
                                                publish(
                                                    &tx,
                                                    &ctx,
                                                    Event::Progress("Exporting WAV", p),
                                                );
                                            }
                                        },
                                    )?;
                                    Ok(Message::with(
                                        "WAV saved to {path}",
                                        &[("path", path.display().to_string())],
                                    ))
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
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    while let Ok(command) = net_rx.recv() {
                        match command {
                            NetCommand::Check(startup) => publish(
                                &tx,
                                &ctx,
                                Event::Check(
                                    startup,
                                    updater::check().map_err(|e| format!("{e:#}")),
                                ),
                            ),
                            NetCommand::Install(update) => {
                                let result = (|| -> anyhow::Result<Prepared> {
                                    let mut last = -1i32;
                                    let staged = updater::download(&update, |p| {
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
                            NetCommand::PrepareInstall(mut staged) => publish(
                                &tx,
                                &ctx,
                                Event::Install(
                                    updater::stage_install(&mut staged)
                                        .map_err(|e| format!("{e:#}")),
                                ),
                            ),
                        }
                    }
                });
            }
            {
                let tx = tx.clone();
                let ctx = ctx.clone();
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
            page: 0,
            bookmark_tab: false,
            exit_ready: false,
            capture,
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
                            crate::theme::apply(ctx, self.session.theme);
                            if let Some(d) = &self.session.document {
                                self.cursor = self
                                    .session
                                    .positions
                                    .get(&d.id)
                                    .map_or(0, |p| p.cursor)
                                    .min(d.chunks.len() - 1);
                                self.message = Message::with(
                                    "Restored {title}",
                                    &[("title", d.title.clone())],
                                );
                                self.scroll_to_cursor = true;
                            }
                        }
                        Err(e) => {
                            self.message = Message::error(e);
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
                    Err(e) => self.message = Message::error(e),
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
                            self.message = Message::with(
                                "Loaded {title} — {count} passages",
                                &[
                                    ("title", doc.title.clone()),
                                    ("count", doc.chunks.len().to_string()),
                                ],
                            );
                            self.session.document = Some(Arc::new(doc));
                            self.mark_dirty();
                            self.scroll_to_cursor = true;
                        }
                        Ok(None) => {}
                        Err(e) => self.message = Message::error(e),
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
                    self.message = Message::error(e);
                    self.state = State::Stopped;
                }
                Event::Saved(result) => {
                    self.saving = false;
                    if let Err(e) = result {
                        self.message = Message::error(e);
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
                            self.update_message = Message::with(
                                "Version {version} is available.",
                                &[("version", update.version.to_string())],
                            );
                            self.message = self.update_message.clone();
                            self.available = Some(update);
                        }
                        Ok(None) => self.update_message = "You are up to date.".into(),
                        Err(e) => {
                            self.update_message = Message::error(e.clone());
                            if !startup {
                                self.message = Message::error(e);
                            }
                        }
                    }
                }
                Event::Progress(label, p) => self.progress = Some((label, p)),
                Event::FileDone(result) => {
                    self.file_busy = false;
                    self.progress = None;
                    self.message = result.unwrap_or_else(Message::error);
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
                            self.installation = Installation::None;
                            self.message = Message::error(e);
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
                            self.message = Message::error(e);
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
            match std::mem::replace(&mut self.installation, Installation::Preparing) {
                Installation::Downloaded(staged) => {
                    if let Err(error) = self.network.try_send(NetCommand::PrepareInstall(staged)) {
                        match error {
                            mpsc::TrySendError::Full(NetCommand::PrepareInstall(staged)) => {
                                self.installation = Installation::Downloaded(staged);
                            }
                            _ => {
                                self.installation = Installation::None;
                                self.net_busy = false;
                                self.closing = false;
                                self.message =
                                    "The update worker stopped. Restart TextReader and retry."
                                        .into();
                            }
                        }
                    }
                }
                Installation::Preparing => {}
                state @ (Installation::None | Installation::Ready | Installation::Downloading) => {
                    self.installation = state;
                    self.exit_ready = true;
                }
            }
        }
    }

    pub fn tick(&mut self, ctx: &egui::Context, visible: bool) {
        if !self.capture {
            self.process_events(ctx);
            if !visible {
                self.request_save();
            }
        }
    }

    pub fn request_close(&mut self) {
        self.stop();
        self.cancel.store(true, Ordering::Relaxed);
        self.closing = true;
        if self.load_failed || self.save_failed {
            self.dirty = false;
        }
        self.request_save();
        if !self.saving && !self.dirty {
            self.exit_ready = true;
        }
    }

    pub fn shutdown(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        let _ = self.speech.try_send(SpeechCommand::Quit);
    }

    pub fn capture_preferences(
        &mut self,
        ctx: &egui::Context,
        language: Language,
        theme: storage::Theme,
    ) {
        self.session.language = language;
        self.session.theme = theme;
        crate::theme::apply(ctx, theme);
    }

    pub fn fixture(&mut self, screen: &str, empty: bool) -> anyhow::Result<()> {
        self.loaded = true;
        self.voice_ready = true;
        self.session.startup_updates = false;
        self.settings = screen == "settings";
        self.bookmark_tab = screen == "bookmarks";
        self.message.clear();
        self.update_message = "Not checked this session.".into();
        if !empty {
            let text = include_str!("../tests/fixtures/design.txt");
            let document = Document::from_text("A quieter way to read".into(), text.into())?;
            self.session.document = Some(Arc::new(document));
            self.cursor = if self.bookmark_tab { 4 } else { 1 };
            self.session
                .positions
                .entry(self.session.document.as_ref().unwrap().id.clone())
                .or_default()
                .bookmarks
                .push(Bookmark {
                    chunk: 4,
                    label: "A little room to think".into(),
                });
        }
        Ok(())
    }
}

#[path = "view.rs"]
mod view;
