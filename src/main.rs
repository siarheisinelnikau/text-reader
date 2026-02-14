#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod desktop;
#[cfg(windows)]
mod diagnostics;
#[cfg(windows)]
mod theme;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|s| s == "--apply-update") {
        anyhow::ensure!(args.len() == 2, "Invalid update helper arguments");
        if let Err(error) = text_reader::updater::apply_update() {
            let dir = text_reader::updater::error_data_dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(
                dir.join("update-error.txt"),
                format!("Update installation failed: {error:#}"),
            );
        }
        return Ok(());
    }
    diagnostics::initialize();
    let cleanup = if args.get(1).is_some_and(|s| s == "--cleanup-update") {
        anyhow::ensure!(args.len() == 3, "Usage: --cleanup-update DIRECTORY");
        let directory = Some(std::path::PathBuf::from(&args[2]));
        args.truncate(1);
        directory
    } else {
        None
    };
    let result = if args.get(1).is_some_and(|s| s == "--screenshot") {
        anyhow::ensure!(
            args.len() >= 3,
            "Usage: --screenshot ABSOLUTE.png [--screen reader|bookmarks|settings|empty] [--width 1180] [--height 820] [--scale 1]"
        );
        let mut screen = "reader";
        let mut width = 1180;
        let mut height = 820;
        let mut scale = 1.0;
        let mut index = 3;
        while index < args.len() {
            anyhow::ensure!(index + 1 < args.len(), "Missing option value");
            match args[index].as_str() {
                "--screen" => screen = &args[index + 1],
                "--width" => width = args[index + 1].parse()?,
                "--height" => height = args[index + 1].parse()?,
                "--scale" => scale = args[index + 1].parse()?,
                value => anyhow::bail!("Unknown capture option: {value}"),
            }
            index += 2;
        }
        desktop::capture(std::path::Path::new(&args[2]), screen, width, height, scale)
    } else if args.get(1).is_some_and(|s| s == "--diagnose") {
        anyhow::ensure!(args.len() == 3, "Usage: --diagnose ABSOLUTE.json");
        desktop::run(Some(args[2].clone().into()), None)
    } else {
        anyhow::ensure!(args.len() == 1, "Unknown command-line options");
        desktop::run(None, cleanup)
    };
    diagnostics::event(&format!("application returned: {result:?}"));
    result
}

#[cfg(not(windows))]
fn main() {
    eprintln!("The TextReader desktop application requires Windows.");
}
