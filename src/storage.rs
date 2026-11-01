use crate::document::Document;
use crate::i18n::Language;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Bookmark {
    pub chunk: usize,
    pub label: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Position {
    pub cursor: usize,
    pub bookmarks: Vec<Bookmark>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    #[default]
    #[serde(other)]
    Light,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub language: Language,
    pub theme: Theme,
    pub startup_updates: bool,
    pub voice: String,
    pub rate: i32,
    pub positions: BTreeMap<String, Position>,
    pub document: Option<std::sync::Arc<Document>>,
    pub font_size: f32,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: Theme::default(),
            startup_updates: true,
            voice: String::new(),
            rate: 0,
            positions: BTreeMap::new(),
            document: None,
            font_size: 20.0,
        }
    }
}

pub fn data_dir() -> PathBuf {
    if let Some(portable) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("TextReader.data")))
        && portable.is_dir()
    {
        return portable;
    }
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("TextReader")
}

pub fn load(dir: &Path) -> Result<Session> {
    let path = dir.join("session.json");
    if !path.exists() {
        return Ok(Session::default());
    }
    ensure!(
        path.metadata()?.len() < 40 * 1024 * 1024,
        "Session is too large. Back up and reset session.json."
    );
    let mut session: Session = serde_json::from_slice(&std::fs::read(path)?)
        .context("Cannot read saved session; the original has been preserved")?;
    session.rate = session.rate.clamp(-10, 10);
    session.font_size = session.font_size.clamp(14.0, 32.0);
    if let Some(doc) = &session.document {
        ensure!(
            !doc.chunks.is_empty() && doc.chunks.len() <= 200_000,
            "Saved document has invalid chunks."
        );
        ensure!(
            doc.chapters.iter().all(|c| c.start < doc.chunks.len())
                && doc
                    .chapters
                    .windows(2)
                    .all(|pair| pair[0].start < pair[1].start),
            "Saved document has invalid chapters."
        );
        ensure!(
            doc.chunks
                .iter()
                .all(|c| !c.text.trim().is_empty() && c.text.chars().count() <= 300)
                && doc.chunks.iter().map(|c| c.text.len()).sum::<usize>()
                    <= crate::document::MAX_TEXT_BYTES,
            "Saved document text is invalid or too large."
        );
    }
    Ok(session)
}

pub fn save(dir: &Path, session: &Session) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("session.json");
    let temp = dir.join("session.next.json");
    let backup = dir.join("session.previous.json");
    let bytes = serde_json::to_vec(session)?;
    use std::io::Write;
    let mut file = std::fs::File::create(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    if path.exists() {
        std::fs::copy(&path, &backup).context("Cannot back up the saved session")?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{
                MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            },
            core::PCWSTR,
        };
        let source: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let dest: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(dest.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )?;
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(temp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_sessions_keep_voice_positions_and_receive_appearance_defaults() {
        let session: Session = serde_json::from_str(r#"{"voice":"SAPI-fixture","rate":3,"startup_updates":false,"positions":{"book":{"cursor":2,"bookmarks":[]}}}"#).unwrap();
        assert_eq!(session.language, Language::English);
        assert_eq!(session.theme, Theme::Light);
        assert_eq!(session.voice, "SAPI-fixture");
        assert_eq!(session.positions["book"].cursor, 2);
        assert!(!session.startup_updates);
    }
    #[test]
    fn language_and_theme_round_trip_independently_of_voice() {
        let session = Session {
            language: Language::Italian,
            theme: Theme::Dark,
            voice: "Russian SAPI fixture".into(),
            ..Session::default()
        };
        let restored: Session =
            serde_json::from_str(&serde_json::to_string(&session).unwrap()).unwrap();
        assert_eq!(restored.language, Language::Italian);
        assert_eq!(restored.theme, Theme::Dark);
        assert_eq!(restored.voice, "Russian SAPI fixture");
        let future: Session = serde_json::from_str(
            r#"{"language":"future","theme":"future","voice":"Keep this voice"}"#,
        )
        .unwrap();
        assert_eq!(future.language, Language::English);
        assert_eq!(future.theme, Theme::Light);
        assert_eq!(future.voice, "Keep this voice");
    }
    #[test]
    fn isolated_persistence_and_corruption() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".test-output")
            .join(format!("storage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Session {
            startup_updates: false,
            ..Session::default()
        };
        s.positions.insert(
            "fixture".into(),
            Position {
                cursor: 2,
                bookmarks: vec![Bookmark {
                    chunk: 1,
                    label: "Here".into(),
                }],
            },
        );
        save(&dir, &s).unwrap();
        let read = load(&dir).unwrap();
        assert!(!read.startup_updates);
        assert_eq!(read.positions["fixture"].bookmarks[0].chunk, 1);
        std::fs::write(dir.join("session.json"), b"invalid").unwrap();
        assert!(load(&dir).is_err());
        assert_eq!(std::fs::read(dir.join("session.json")).unwrap(), b"invalid");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
