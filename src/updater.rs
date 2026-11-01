//! TextReader's release identity; transport, verification and installation are shared.
use anyhow::Result;
use rust_auto_updator::Config;
pub use rust_auto_updator::{Prepared, Release as Update};

pub const REPOSITORY: &str = "https://github.com/siarheisinelnikau/text-reader";

pub fn config() -> Result<Config> {
    let mut config = Config::new(
        "siarheisinelnikau",
        "text-reader",
        env!("CARGO_PKG_VERSION"),
        "TextReader",
    )
    .map_err(anyhow::Error::msg)?;
    config.max_executable_size = 200 * 1024 * 1024;
    config.install.stage_prefix = ".TextReader-update-".into();
    config.install.helper_name = "TextReader-update-helper.exe".into();
    Ok(config)
}

pub fn check() -> Result<Option<Update>> {
    rust_auto_updator::check_latest(&config()?).map_err(anyhow::Error::msg)
}

pub fn download(update: &Update, mut progress: impl FnMut(f32)) -> Result<Prepared> {
    rust_auto_updator::prepare_download(update, &config()?, |received, total| {
        progress(received as f32 / total.max(1) as f32);
    })
    .map_err(anyhow::Error::msg)
}

pub fn stage_install(staged: &mut Prepared) -> Result<()> {
    rust_auto_updator::handoff(staged).map_err(anyhow::Error::msg)
}

pub fn apply_update() -> Result<()> {
    rust_auto_updator::apply_update(&config()?).map_err(anyhow::Error::msg)
}

pub fn error_data_dir() -> std::path::PathBuf {
    config()
        .ok()
        .and_then(|config| rust_auto_updator::installation_target(&config).ok())
        .and_then(|target| target.parent().map(|parent| parent.join("TextReader.data")))
        .filter(|portable| portable.is_dir())
        .unwrap_or_else(crate::storage::data_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_selection_uses_text_reader_identity_and_required_checksums() {
        let config = config().unwrap();
        let fixture = |checksum: bool| {
            let mut assets = vec![
                serde_json::json!({"name":"TextReader-v0.3.0.exe","size":2048,
                "browser_download_url":format!("{REPOSITORY}/releases/download/v0.3.0/TextReader-v0.3.0.exe")}),
            ];
            if checksum {
                assets.push(serde_json::json!({"name":"SHA256SUMS.txt","size":100,
                "browser_download_url":format!("{REPOSITORY}/releases/download/v0.3.0/SHA256SUMS.txt")}));
            }
            serde_json::json!({"tag_name":"v0.3.0","html_url":format!("{REPOSITORY}/releases/tag/v0.3.0"),
                "draft":false,"prerelease":false,"assets":assets}).to_string()
        };
        assert!(
            rust_auto_updator::parse_release(&fixture(true), &config)
                .unwrap()
                .is_some()
        );
        assert!(rust_auto_updator::parse_release(&fixture(false), &config).is_err());
    }
}
