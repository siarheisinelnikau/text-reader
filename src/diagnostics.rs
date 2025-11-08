//! Opt-in local diagnostics; no document content or network telemetry.
use std::{io::Write, path::PathBuf, sync::OnceLock};

static LOG: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn initialize() {
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let log = path
        .filter(|p| p.join("TextReader.diagnostics").is_file())
        .map(|p| p.join("TextReader-diagnostics.log"));
    let enabled = log.is_some();
    let _ = LOG.set(log);
    if enabled {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            event(&format!(
                "panic: {info}\n{}",
                std::backtrace::Backtrace::force_capture()
            ));
            original(info);
        }));
        event(&format!(
            "startup pid={} version={} exe={:?}",
            std::process::id(),
            env!("CARGO_PKG_VERSION"),
            std::env::current_exe()
        ));
    }
}

pub fn event(message: &str) {
    if let Some(Some(path)) = LOG.get()
        && let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = writeln!(file, "{:?}: {message}", std::time::SystemTime::now());
    }
}
