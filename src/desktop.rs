//! Reactive CPU rasterization -> retained bitmap -> Windows GDI presentation.
use crate::app::App;
use anyhow::{Context, Result, ensure};
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use serde::Serialize;
use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy, OwnedDisplayHandle},
    window::{Window, WindowId},
};

fn apply_application_icons(window: &Window) -> Result<()> {
    use winit::{
        dpi::PhysicalSize,
        platform::windows::{IconExtWindows, WindowExtWindows},
        window::Icon,
    };
    let size = |logical: f64| (logical * window.scale_factor()).round().max(1.0) as u32;
    let small = size(16.0);
    let large = size(32.0);
    window.set_window_icon(Some(Icon::from_resource(
        1,
        Some(PhysicalSize::new(small, small)),
    )?));
    window.set_taskbar_icon(Some(Icon::from_resource(
        1,
        Some(PhysicalSize::new(large, large)),
    )?));
    Ok(())
}

fn native_icons(window: &Window) -> serde_json::Value {
    use windows_sys::Win32::{
        Graphics::Gdi::{BITMAP, DeleteObject, GetObjectW},
        UI::WindowsAndMessaging::{
            GetIconInfo, ICON_BIG, ICON_SMALL, ICONINFO, SendMessageW, WM_GETICON,
        },
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else {
        return serde_json::Value::Null;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return serde_json::Value::Null;
    };
    let dimensions = |kind: u32| -> Option<[i32; 2]> {
        // Reads this diagnostic window only, never another application's handles.
        unsafe {
            let icon = SendMessageW(handle.hwnd.get() as _, WM_GETICON, kind as usize, 0) as _;
            let mut info = ICONINFO::default();
            if GetIconInfo(icon, &mut info) == 0 {
                return None;
            }
            let mut bitmap = BITMAP::default();
            let read = GetObjectW(
                info.hbmColor,
                std::mem::size_of::<BITMAP>() as i32,
                (&mut bitmap as *mut BITMAP).cast(),
            );
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
            (read != 0).then_some([bitmap.bmWidth, bitmap.bmHeight])
        }
    };
    serde_json::json!({"small":dimensions(ICON_SMALL),"large":dimensions(ICON_BIG),"scale":window.scale_factor()})
}

#[derive(Debug)]
enum Wake {
    Repaint { when: Instant, pass: u64 },
    Access(accesskit_winit::Event),
}
impl From<accesskit_winit::Event> for Wake {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Access(event)
    }
}

#[derive(Default, Clone, Serialize)]
struct Counters {
    layouts: u64,
    rasters: u64,
    presents: u64,
    retained_blits: u64,
    layout_ms: f64,
    raster_ms: f64,
    present_ms: f64,
}

struct Raster {
    renderer: EguiSoftwareRender,
    pixels: Vec<[u8; 4]>,
    width: u32,
    height: u32,
}
impl Raster {
    fn new() -> Self {
        Self {
            renderer: EguiSoftwareRender::new(ColorFieldOrder::Bgra),
            pixels: vec![],
            width: 0,
            height: 0,
        }
    }
    fn paint(&mut self, ctx: &egui::Context, output: &egui::FullOutput, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.pixels
            .resize(width as usize * height as usize, [249, 247, 246, 255]);
        let primitives = ctx.tessellate(output.shapes.clone(), output.pixels_per_point);
        self.renderer.render(
            &mut BufferMutRef::new(&mut self.pixels, width as usize, height as usize),
            &primitives,
            &output.textures_delta,
            output.pixels_per_point,
        );
    }
    fn save(&self, path: &Path) -> Result<()> {
        ensure!(
            !path.exists(),
            "Screenshot destination already exists: {}",
            path.display()
        );
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let rgba = self
            .pixels
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], 255])
            .collect::<Vec<_>>();
        let temporary = path.with_extension("partial.png");
        image::save_buffer_with_format(
            &temporary,
            &rgba,
            self.width,
            self.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    }
}

pub fn capture(
    path: &Path,
    screen: &str,
    width: u32,
    height: u32,
    scale: f32,
    language: text_reader::i18n::Language,
    theme: text_reader::storage::Theme,
) -> Result<()> {
    ensure!(
        matches!(screen, "reader" | "bookmarks" | "settings" | "empty"),
        "Unknown capture screen: {screen}"
    );
    ensure!(path.is_absolute(), "Screenshot path must be absolute");
    ensure!(
        (820..=1800).contains(&width)
            && (560..=1200).contains(&height)
            && (1.0..=2.0).contains(&scale),
        "Capture dimensions or scale are outside the supported range"
    );
    let context = egui::Context::default();
    let mut app = App::new(&context, true);
    app.fixture(screen, screen == "empty")?;
    app.capture_preferences(&context, language, theme);
    let mut raster = Raster::new();
    let deadline = Instant::now() + Duration::from_secs(12);
    for frame in 0..4 {
        ensure!(Instant::now() < deadline, "Capture timed out");
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width as f32, height as f32),
            )),
            time: Some(frame as f64 * 0.2),
            ..Default::default()
        };
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .native_pixels_per_point = Some(scale);
        let output = context.run(input, |ctx| app.draw(ctx));
        raster.paint(
            &context,
            &output,
            (width as f32 * scale) as u32,
            (height as f32 * scale) as u32,
        );
    }
    raster.save(path)?;
    app.shutdown();
    println!(
        "Saved real application client-area capture: {} (CPU software, {width}x{height} logical, scale {scale}, 4 finite frames, no window/workers/network/audio)",
        path.display()
    );
    Ok(())
}

struct Diagnostic {
    path: PathBuf,
    phase: usize,
    deadline: Instant,
    start: Counters,
    cpu_start: f64,
    samples: Vec<serde_json::Value>,
}
struct Desktop {
    ctx: egui::Context,
    app: Option<App>,
    window: Option<Arc<Window>>,
    state: Option<egui_winit::State>,
    surface: Option<softbuffer::Surface<OwnedDisplayHandle, Arc<Window>>>,
    display: Option<softbuffer::Context<OwnedDisplayHandle>>,
    proxy: EventLoopProxy<Wake>,
    raster: Raster,
    counts: Counters,
    dirty: bool,
    occluded: bool,
    next: Option<Instant>,
    diagnostic: Option<Diagnostic>,
    error: Option<anyhow::Error>,
    causes: std::collections::BTreeMap<String, u64>,
    window_events: std::collections::BTreeMap<String, u64>,
    cleanup: Option<PathBuf>,
}
impl Desktop {
    fn visible(&self) -> bool {
        self.window.as_ref().is_some_and(|w| {
            !self.occluded && !w.is_minimized().unwrap_or(false) && w.is_visible().unwrap_or(true)
        })
    }
    fn queue(&mut self, delay: Duration) {
        if let Some(at) = Instant::now().checked_add(delay) {
            self.next = Some(self.next.map_or(at, |old| old.min(at)));
        }
    }
    fn finish(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(app) = &mut self.app {
            app.shutdown();
        }
        self.ctx.set_request_repaint_callback(|_| {});
        crate::diagnostics::event(&format!(
            "CPU/GDI counters: {}",
            serde_json::to_string(&self.counts).unwrap_or_default()
        ));
        event_loop.exit();
    }
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.error = Some(error);
        self.finish(event_loop);
    }

    fn redraw(&mut self) -> Result<()> {
        if !self.visible() {
            return Ok(());
        }
        let window = self.window.as_ref().unwrap().clone();
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        if self.dirty || self.raster.width != size.width || self.raster.height != size.height {
            self.next = None;
            self.dirty = false;
            let state = self.state.as_mut().unwrap();
            let mut input = state.take_egui_input(&window);
            egui_winit::update_viewport_info(
                input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap(),
                &self.ctx,
                &window,
                false,
            );
            let app = self.app.as_mut().unwrap();
            let start = Instant::now();
            let output = self.ctx.run(input, |ctx| {
                app.tick(ctx, true);
                app.draw(ctx);
                app.tick(ctx, true);
            });
            if self.diagnostic.is_some() {
                for cause in self.ctx.repaint_causes() {
                    *self.causes.entry(cause.to_string()).or_default() += 1;
                }
            }
            self.counts.layouts += 1;
            self.counts.layout_ms += start.elapsed().as_secs_f64() * 1000.0;
            state.handle_platform_output(&window, output.platform_output.clone());
            let mut commands = vec![];
            if let Some(viewport) = output.viewport_output.get(&egui::ViewportId::ROOT) {
                if viewport.repaint_delay != Duration::MAX {
                    commands.push(viewport.repaint_delay);
                }
                for command in &viewport.commands {
                    if matches!(command, egui::ViewportCommand::Close) {
                        app.request_close();
                    }
                }
                let mut info = egui::ViewportInfo::default();
                let mut actions = vec![];
                egui_winit::process_viewport_commands(
                    &self.ctx,
                    &mut info,
                    viewport.commands.clone().into_iter().filter(|c| {
                        !matches!(
                            c,
                            egui::ViewportCommand::Close | egui::ViewportCommand::CancelClose
                        )
                    }),
                    &window,
                    &mut actions,
                );
            }
            let start = Instant::now();
            self.raster
                .paint(&self.ctx, &output, size.width, size.height);
            self.counts.rasters += 1;
            self.counts.raster_ms += start.elapsed().as_secs_f64() * 1000.0;
            for delay in commands {
                self.queue(delay);
            }
        } else {
            self.counts.retained_blits += 1;
        }
        let start = Instant::now();
        let surface = self.surface.as_mut().unwrap();
        surface
            .resize(
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            )
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let mut buffer = surface
            .buffer_mut()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        buffer.copy_from_slice(bytemuck::cast_slice(&self.raster.pixels));
        buffer
            .present()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        self.counts.presents += 1;
        self.counts.present_ms += start.elapsed().as_secs_f64() * 1000.0;
        Ok(())
    }

    fn diagnostic_step(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let observed_visible = self.visible();
        let Some(run) = &mut self.diagnostic else {
            return Ok(());
        };
        if Instant::now() < run.deadline {
            return Ok(());
        }
        if matches!(run.phase, 1 | 2 | 4) {
            let start = &run.start;
            let current = &self.counts;
            let label = match run.phase {
                1 => "visible_idle",
                2 => "minimized_idle",
                _ => "restored_idle",
            };
            let cpu = cpu_seconds()? - run.cpu_start;
            let seconds = Instant::now()
                .saturating_duration_since(run.deadline - Duration::from_secs(5))
                .as_secs_f64();
            let layouts = current.layouts - start.layouts;
            let rasters = current.rasters - start.rasters;
            run.samples.push(serde_json::json!({"state":label,"observed_visible":observed_visible,"state_valid":observed_visible==(run.phase!=2),"seconds":seconds,"cpu_seconds":cpu,"percent_of_one_core":cpu/seconds*100.0,"layouts":layouts,"rasters":rasters,"presents":current.presents-start.presents,"retained_blits":current.retained_blits-start.retained_blits,"layout_ms":current.layout_ms-start.layout_ms,"raster_ms":current.raster_ms-start.raster_ms,"present_ms":current.present_ms-start.present_ms,"settled":layouts==0&&rasters==0}));
        }
        run.phase += 1;
        match run.phase {
            1 => {
                run.start = self.counts.clone();
                run.cpu_start = cpu_seconds()?;
                run.deadline = Instant::now() + Duration::from_secs(5);
            }
            2 => {
                self.window.as_ref().unwrap().set_minimized(true);
                self.occluded = true;
                self.next = None;
                run.start = self.counts.clone();
                run.cpu_start = cpu_seconds()?;
                run.deadline = Instant::now() + Duration::from_secs(5);
            }
            3 => {
                self.window.as_ref().unwrap().set_minimized(false);
                self.occluded = false;
                self.dirty = true;
                self.window.as_ref().unwrap().request_redraw();
                run.deadline = Instant::now() + Duration::from_secs(2);
            }
            4 => {
                run.start = self.counts.clone();
                run.cpu_start = cpu_seconds()?;
                run.deadline = Instant::now() + Duration::from_secs(5);
            }
            _ => {
                let report = serde_json::json!({"backend":"egui CPU rasterizer -> retained BGRA bitmap -> softbuffer Windows GDI","process_id":std::process::id(),"profile":"optimized release required","scenario":"isolated populated fixture; no worker/network/audio tasks; no input automation","samples":run.samples,"totals":self.counts,"repaint_causes":self.causes,"window_events":self.window_events,"native_icons":self.window.as_ref().map(|window|native_icons(window)),"scope":"idle/minimized/restore scheduling and CPU plus native icon assignment; not perceived interaction smoothness or GPU telemetry"});
                if let Some(parent) = run.path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&run.path, serde_json::to_vec_pretty(&report)?)?;
                self.finish(event_loop);
            }
        }
        Ok(())
    }
}

fn cpu_seconds() -> Result<f64> {
    use windows::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetCurrentProcess, GetProcessTimes},
    };
    unsafe {
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(
            GetCurrentProcess(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
        .context("Cannot measure this process CPU time")?;
        let ticks = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
        Ok((ticks(kernel) + ticks(user)) as f64 / 10_000_000.0)
    }
}

impl ApplicationHandler<Wake> for Desktop {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<()> {
            let window = Arc::new(
                event_loop.create_window(
                    Window::default_attributes()
                        .with_title("TextReader")
                        .with_visible(false)
                        .with_inner_size(winit::dpi::LogicalSize::new(1180.0, 820.0))
                        .with_min_inner_size(winit::dpi::LogicalSize::new(900.0, 620.0)),
                )?,
            );
            apply_application_icons(&window)?;
            let display = softbuffer::Context::new(event_loop.owned_display_handle())
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let surface = softbuffer::Surface::new(&display, window.clone())
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let mut state = egui_winit::State::new(
                self.ctx.clone(),
                egui::ViewportId::ROOT,
                &window,
                Some(window.scale_factor() as f32),
                None,
                None,
            );
            state.init_accesskit(event_loop, &window, self.proxy.clone());
            let mut app = App::new(&self.ctx, self.diagnostic.is_some());
            if self.diagnostic.is_some() {
                app.fixture("reader", false)?;
            }
            self.window = Some(window.clone());
            self.state = Some(state);
            self.app = Some(app);
            self.surface = Some(surface);
            self.display = Some(display);
            window.set_visible(true);
            if let Some(directory) = self.cleanup.take() {
                rust_auto_updator::cleanup_after_update(
                    directory,
                    text_reader::updater::config()?.install,
                );
            }
            crate::diagnostics::event(
                "backend=CPU software rasterizer; presentation=softbuffer Win32 GDI; scheduling=Wait/WaitUntil",
            );
            window.request_redraw();
            Ok(())
        })();
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone().filter(|w| w.id() == id) else {
            return;
        };
        if self.diagnostic.is_some() {
            let name = match &event {
                WindowEvent::CursorMoved { .. } => "cursor_moved",
                WindowEvent::RedrawRequested => "redraw",
                WindowEvent::Resized(_) => "resize",
                WindowEvent::Focused(_) => "focus",
                WindowEvent::Occluded(_) => "occlusion",
                _ => "other",
            };
            *self.window_events.entry(name.into()).or_default() += 1;
        }
        let response = self
            .state
            .as_mut()
            .unwrap()
            .on_window_event(&window, &event);
        match event {
            WindowEvent::CloseRequested => {
                self.app.as_mut().unwrap().request_close();
                if self.app.as_ref().unwrap().exit_ready {
                    self.finish(event_loop);
                }
            }
            WindowEvent::Occluded(value) => {
                self.occluded = value;
                if !value {
                    self.dirty = true;
                    window.request_redraw();
                } else {
                    self.next = None;
                }
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if matches!(event, WindowEvent::ScaleFactorChanged { .. })
                    && let Err(error) = apply_application_icons(&window)
                {
                    self.fail(event_loop, error);
                    return;
                }
                self.dirty = true;
                if self.visible() {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw() {
                    self.fail(event_loop, error);
                }
            }
            _ => {
                if response.repaint {
                    self.dirty = true;
                    if self.visible() {
                        window.request_redraw();
                    }
                }
            }
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        let visible = self.visible();
        if let Some(app) = &mut self.app {
            app.tick(&self.ctx, visible);
            if app.exit_ready {
                self.finish(event_loop);
                return;
            }
        }
        match event {
            Wake::Repaint { when, pass } => {
                let current = self.ctx.cumulative_pass_nr();
                if current == pass || current == pass + 1 {
                    self.dirty = true;
                    if visible {
                        self.next = Some(self.next.map_or(when, |old| old.min(when)));
                    }
                }
            }
            Wake::Access(event) => {
                match event.window_event {
                    accesskit_winit::WindowEvent::InitialTreeRequested => {
                        self.ctx.enable_accesskit()
                    }
                    accesskit_winit::WindowEvent::ActionRequested(request) => self
                        .state
                        .as_mut()
                        .unwrap()
                        .on_accesskit_action_request(request),
                    accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                        self.ctx.disable_accesskit()
                    }
                }
                self.dirty = true;
                if visible {
                    self.window.as_ref().unwrap().request_redraw();
                }
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.diagnostic_step(event_loop) {
            self.fail(event_loop, error);
            return;
        }
        let now = Instant::now();
        if self.visible() && self.next.is_some_and(|deadline| deadline <= now) {
            self.next = None;
            self.dirty = true;
            self.window.as_ref().unwrap().request_redraw();
        }
        if !self.visible() {
            self.next = None;
        }
        let deadline = self
            .next
            .into_iter()
            .chain(self.diagnostic.as_ref().map(|r| r.deadline))
            .min();
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        if let Some(app) = &mut self.app {
            app.shutdown();
        }
    }
}

pub fn run(diagnostic: Option<PathBuf>, cleanup: Option<PathBuf>) -> Result<()> {
    if let Some(path) = &diagnostic {
        ensure!(
            path.is_absolute() && !path.exists(),
            "Diagnostic output must be a new absolute file"
        );
    }
    let event_loop = EventLoop::<Wake>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let ctx = egui::Context::default();
    let repaint_proxy = proxy.clone();
    ctx.set_request_repaint_callback(move |info| {
        if let Some(when) = Instant::now().checked_add(info.delay) {
            let _ = repaint_proxy.send_event(Wake::Repaint {
                when,
                pass: info.current_cumulative_pass_nr,
            });
        }
    });
    let mut desktop = Desktop {
        ctx,
        app: None,
        window: None,
        state: None,
        surface: None,
        display: None,
        proxy,
        raster: Raster::new(),
        counts: Counters::default(),
        dirty: true,
        occluded: false,
        next: None,
        diagnostic: diagnostic.map(|path| Diagnostic {
            path,
            phase: 0,
            deadline: Instant::now() + Duration::from_secs(2),
            start: Counters::default(),
            cpu_start: 0.0,
            samples: vec![],
        }),
        error: None,
        causes: Default::default(),
        window_events: Default::default(),
        cleanup,
    };
    event_loop
        .run_app(&mut desktop)
        .context("Desktop event loop failed")?;
    if let Some(error) = desktop.error {
        return Err(error);
    }
    Ok(())
}
