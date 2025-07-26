//! Stable-channel release selection and a portable, user-initiated update path.
use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub const REPOSITORY: &str = "https://github.com/siarheisinelnikau/text-reader";
const API: &str = "https://api.github.com/repos/siarheisinelnikau/text-reader/releases/latest";
const PREFIX: &str = "https://github.com/siarheisinelnikau/text-reader/releases/download/";
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Update {
    pub version: String,
    pub asset: String,
    pub url: String,
    pub checksum_url: String,
    pub size: u64,
}
#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .user_agent(concat!("TextReader/", env!("CARGO_PKG_VERSION")))
        .timeout_global(Some(Duration::from_secs(120)))
        .timeout_per_call(Some(Duration::from_secs(30)))
        .build()
        .into()
}

fn select(release: Release, current: &str) -> Result<Option<Update>> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let version = Version::parse(release.tag_name.trim_start_matches('v'))?;
    if !version.pre.is_empty() || version <= Version::parse(current)? {
        return Ok(None);
    }
    ensure!(
        cfg!(target_arch = "x86_64"),
        "Automatic installation currently supports Windows x64 only."
    );
    let name = format!("TextReader-v{version}.exe");
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .context("The release has no compatible standalone executable")?;
    let checksum = release
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS.txt")
        .context("The release is missing SHA256SUMS.txt; installation is unavailable")?;
    let root = format!("{PREFIX}{}/", release.tag_name);
    ensure!(
        asset.browser_download_url == format!("{root}{name}")
            && checksum.browser_download_url == format!("{root}SHA256SUMS.txt"),
        "Release asset URL is outside the canonical repository."
    );
    ensure!(
        asset.size > 64 && asset.size <= MAX_DOWNLOAD,
        "Release executable size is invalid."
    );
    Ok(Some(Update {
        version: version.to_string(),
        asset: name,
        url: asset.browser_download_url.clone(),
        checksum_url: checksum.browser_download_url.clone(),
        size: asset.size,
    }))
}

pub fn check() -> Result<Option<Update>> {
    let mut response=agent().get(API).call().context("Update check failed. The repository/release may not be published, the network may be unavailable, or GitHub may be rate limiting requests.")?;
    let json = response
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_string()?;
    select(serde_json::from_str(&json)?, env!("CARGO_PKG_VERSION"))
}

fn expected_checksum(text: &str, name: &str) -> Result<String> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(hash), Some(file)) = (parts.next(), parts.next())
            && file.trim_start_matches('*') == name
        {
            ensure!(
                hash.len() == 64 && hash.bytes().all(|c| c.is_ascii_hexdigit()),
                "Invalid SHA-256 checksum."
            );
            return Ok(hash.to_ascii_lowercase());
        }
    }
    bail!("No checksum was provided for the executable.")
}

pub fn download(update: &Update, dir: &Path, mut progress: impl FnMut(f32)) -> Result<PathBuf> {
    // Only values selected from the canonical release are used by the application.
    ensure!(
        update.url.starts_with(PREFIX) && update.checksum_url.starts_with(PREFIX),
        "Invalid update origin."
    );
    let agent = agent();
    let sums = agent
        .get(&update.checksum_url)
        .call()?
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_to_string()?;
    let expected = expected_checksum(&sums, &update.asset)?;
    std::fs::create_dir_all(dir)?;
    let target = dir.join("download.exe.partial");
    let result = (|| -> Result<()> {
        let mut file = std::fs::File::create(&target)?;
        let mut response = agent.get(&update.url).call()?;
        let mut reader = response.body_mut().as_reader();
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            ensure!(
                count <= update.size && count <= MAX_DOWNLOAD,
                "Download exceeds the declared size."
            );
            file.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
            progress(count as f32 / update.size as f32);
        }
        file.sync_all()?;
        ensure!(count == update.size, "Download is incomplete.");
        ensure!(
            format!("{:x}", hash.finalize()) == expected,
            "Checksum mismatch; the existing installation was preserved."
        );
        let mut header = [0u8; 2];
        std::fs::File::open(&target)?.read_exact(&mut header)?;
        ensure!(
            &header == b"MZ",
            "The downloaded file is not a Windows executable."
        );
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&target);
        return Err(e);
    }
    Ok(target)
}

#[cfg(windows)]
pub fn stage_install(staged: &Path, dir: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe()?;
    let helper = dir.join("TextReader-update-helper.exe");
    std::fs::copy(&exe, &helper).context("Cannot prepare the portable update helper")?;
    std::process::Command::new(helper)
        .args(["--apply-update", &std::process::id().to_string()])
        .arg(&exe)
        .arg(staged)
        .arg(file_hash(staged)?)
        .creation_flags(0x08000000)
        .spawn()
        .context("Cannot start update helper. The application is unchanged.")?;
    Ok(())
}

#[cfg(windows)]
pub fn apply_update(args: &[String]) -> Result<()> {
    use windows::Win32::{Foundation::*, System::Threading::*};
    ensure!(args.len() == 6, "Invalid update helper arguments.");
    let pid = args[2].parse::<u32>()?;
    let target = PathBuf::from(&args[3]);
    let staged = PathBuf::from(&args[4]);
    ensure!(
        target.is_absolute() && staged.is_absolute() && target != staged,
        "Invalid update paths."
    );
    // The helper is spawned while the parent still exists, and never terminates it.
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, false, pid)
            .context("Cannot wait for TextReader to exit")?;
        let result = WaitForSingleObject(process, 30_000);
        let _ = CloseHandle(process);
        ensure!(
            result == WAIT_OBJECT_0,
            "TextReader did not exit in time; no replacement was made."
        );
    }
    let backup = replace_portable(&target, &staged, &args[5])?;
    match std::process::Command::new(&target).spawn() {
        Ok(_) => {
            let _ = std::fs::remove_file(backup);
            let _ = std::fs::remove_file(staged);
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&target);
            let _ = std::fs::rename(backup, &target);
            Err(e.into())
        }
    }
}

fn file_hash(path: &Path) -> Result<String> {
    let mut input = std::fs::File::open(path)?;
    ensure!(
        input.metadata()?.len() <= MAX_DOWNLOAD,
        "Staged executable exceeds the size limit."
    );
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn replace_portable(target: &Path, staged: &Path, expected: &str) -> Result<PathBuf> {
    ensure!(
        file_hash(staged)? == expected,
        "The staged executable changed after download; no replacement was made."
    );
    let backup = target.with_extension("exe.previous");
    ensure!(
        !backup.exists(),
        "An earlier executable backup exists at {}. Remove it after reviewing the previous update.",
        backup.display()
    );
    // Move the verified file onto the destination volume before replacing the working executable.
    let next = target.with_extension("exe.next");
    ensure!(
        !next.exists(),
        "An earlier staged executable exists at {}.",
        next.display()
    );
    std::fs::copy(&staged, &next).context(
        "Cannot write to the installation folder. Move the portable app to a writable folder.",
    )?;
    if file_hash(&next)? != expected {
        let _ = std::fs::remove_file(&next);
        bail!(
            "The copied executable failed verification. The existing installation was preserved."
        );
    }
    std::fs::rename(&target, &backup)?;
    if let Err(e) = std::fs::rename(&next, &target) {
        let _ = std::fs::rename(&backup, &target);
        return Err(e.into());
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn release(tag: &str) -> Release {
        let name = format!("TextReader-{}.exe", tag);
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
            assets: vec![
                Asset {
                    name: name.clone(),
                    browser_download_url: format!("{PREFIX}{tag}/{name}"),
                    size: 100,
                },
                Asset {
                    name: "SHA256SUMS.txt".into(),
                    browser_download_url: format!("{PREFIX}{tag}/SHA256SUMS.txt"),
                    size: 90,
                },
            ],
        }
    }
    #[test]
    fn stable_semver_origin_and_missing_assets() {
        assert!(select(release("v0.10.0"), "0.9.0").unwrap().is_some());
        assert!(select(release("v0.1.0"), "0.2.0").unwrap().is_none());
        assert!(select(release("v0.2.0-beta.1"), "0.1.0").unwrap().is_none());
        let mut r = release("v0.2.0");
        r.assets[0].browser_download_url = "https://example.org/app.exe".into();
        assert!(select(r, "0.1.0").is_err());
        let mut r = release("v0.2.0");
        r.assets.pop();
        assert!(select(r, "0.1.0").is_err());
    }
    #[test]
    fn checksum_must_match_exact_name() {
        let hash = "a".repeat(64);
        assert_eq!(
            expected_checksum(
                &format!("{hash} *TextReader-v0.2.0.exe"),
                "TextReader-v0.2.0.exe"
            )
            .unwrap(),
            hash
        );
        assert!(expected_checksum("abcd app.exe", "app.exe").is_err());
        assert!(expected_checksum(&format!("{hash} other.exe"), "app.exe").is_err());
    }

    #[test]
    fn synthetic_portable_replacement_preserves_old_on_integrity_failure() {
        let dir =
            std::env::temp_dir().join(format!("textreader-update-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("fixture.exe");
        let staged = dir.join("fixture.download");
        std::fs::write(&target, b"old synthetic fixture").unwrap();
        std::fs::write(&staged, b"new synthetic fixture").unwrap();
        assert!(replace_portable(&target, &staged, &"0".repeat(64)).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"old synthetic fixture");
        let backup = replace_portable(&target, &staged, &file_hash(&staged).unwrap()).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new synthetic fixture");
        assert_eq!(std::fs::read(&backup).unwrap(), b"old synthetic fixture");
        assert!(replace_portable(&target, &staged, &file_hash(&staged).unwrap()).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
