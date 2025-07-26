#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;

#[cfg(windows)]
fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--apply-update") {
        if let Err(error) = text_reader::updater::apply_update(&args) {
            // Keep failures for the next launch; the parent may still be running on timeout.
            let dir = text_reader::storage::data_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(
                dir.join("update-error.txt"),
                format!("Update installation failed: {error:#}"),
            );
        }
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 760.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "TextReader",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

#[cfg(not(windows))]
fn main() {
    eprintln!("TextReader's speech adapter and desktop application currently require Windows.");
}
