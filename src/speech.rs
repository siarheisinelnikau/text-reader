//! COM objects never leave their creating worker thread. Construction does not play sound.
#[cfg(windows)]
mod platform {
    use crate::{
        document::Document,
        playback::{Audio, Voice},
    };
    use anyhow::{Context, Result, bail, ensure};
    use std::{
        path::Path,
        sync::atomic::{AtomicBool, Ordering},
        time::{Duration, Instant},
    };
    use windows::{
        Win32::{
            Media::{Audio::WAVEFORMATEX, Speech::*},
            System::Com::*,
        },
        core::{GUID, Interface, PCWSTR, PWSTR},
    };

    struct Apartment;
    impl Apartment {
        fn new() -> Result<Self> {
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            }
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    pub struct Sapi {
        voice: ISpVoice,
        tokens: Vec<(Voice, ISpObjectToken)>,
        default: ISpObjectToken,
        paused: bool,
        _apartment: Apartment,
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    unsafe fn take_string(p: PWSTR) -> Result<String> {
        let value = unsafe { p.to_string() };
        unsafe {
            CoTaskMemFree(Some(p.0.cast()));
        }
        Ok(value?)
    }

    impl Sapi {
        pub fn new() -> Result<Self> {
            let apartment = Apartment::new()?;
            // SAFETY: COM is initialized on this thread; interfaces stay on it until drop.
            unsafe {
                let voice: ISpVoice = CoCreateInstance(&SpVoice, None, CLSCTX_INPROC_SERVER)
                    .context("Windows SAPI is unavailable")?;
                let default = voice
                    .GetVoice()
                    .context("No default SAPI voice is installed")?;
                let category: ISpObjectTokenCategory =
                    CoCreateInstance(&SpObjectTokenCategory, None, CLSCTX_INPROC_SERVER)?;
                category.SetId(SPCAT_VOICES, false)?;
                let items = category.EnumTokens(PCWSTR::null(), PCWSTR::null())?;
                let mut count = 0;
                items.GetCount(&mut count)?;
                let mut tokens = Vec::new();
                for i in 0..count {
                    let token = items.Item(i)?;
                    let id = take_string(token.GetId()?)?;
                    let name = take_string(token.GetStringValue(PCWSTR::null())?)
                        .unwrap_or_else(|_| id.clone());
                    tokens.push((Voice { id, name }, token));
                }
                Ok(Self {
                    voice,
                    tokens,
                    default,
                    paused: false,
                    _apartment: apartment,
                })
            }
        }
    }

    impl Audio for Sapi {
        fn voices(&self) -> Result<Vec<Voice>> {
            Ok(self.tokens.iter().map(|t| t.0.clone()).collect())
        }
        fn configure(&mut self, id: &str, rate: i32) -> Result<()> {
            let token = if id.is_empty() {
                &self.default
            } else {
                &self
                    .tokens
                    .iter()
                    .find(|t| t.0.id == id)
                    .context("The saved voice is no longer installed. Select another voice.")?
                    .1
            };
            unsafe {
                self.voice.SetVoice(token)?;
                self.voice.SetRate(rate.clamp(-10, 10))?;
            }
            Ok(())
        }
        fn speak(&mut self, text: &str) -> Result<()> {
            let text = wide(text);
            unsafe {
                self.voice.Speak(
                    PCWSTR(text.as_ptr()),
                    (SPF_ASYNC.0 | SPF_IS_NOT_XML.0) as u32,
                    None,
                )?;
            }
            Ok(())
        }
        fn done(&self) -> Result<bool> {
            // The generated Result wrapper discards S_FALSE; use the raw HRESULT to distinguish pending.
            let status = unsafe { (self.voice.vtable().WaitUntilDone)(self.voice.as_raw(), 0) };
            match status.0 {
                0 => Ok(true),
                1 => Ok(false),
                _ => {
                    status.ok()?;
                    bail!("Unexpected SAPI completion status")
                }
            }
        }
        fn pause(&mut self) -> Result<()> {
            unsafe {
                self.voice.Pause()?;
            }
            self.paused = true;
            Ok(())
        }
        fn resume(&mut self) -> Result<()> {
            unsafe {
                self.voice.Resume()?;
            }
            self.paused = false;
            Ok(())
        }
        fn stop(&mut self) -> Result<()> {
            unsafe {
                self.voice.Speak(
                    PCWSTR::null(),
                    (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32,
                    None,
                )?;
                if self.paused {
                    self.voice.Resume()?;
                    self.paused = false;
                }
            }
            Ok(())
        }
    }

    pub fn export(
        doc: &Document,
        path: &Path,
        voice: &str,
        rate: i32,
        cancel: &AtomicBool,
        mut progress: impl FnMut(f32),
    ) -> Result<()> {
        ensure!(
            !path.exists(),
            "Export destination already exists. Choose a new filename to preserve that file."
        );
        let mut sapi = Sapi::new()?;
        sapi.configure(voice, rate)?;
        let part = path.with_extension(format!("{}.partial.wav", std::process::id()));
        ensure!(
            !part.exists(),
            "A partial export already exists at {}. Choose another destination.",
            part.display()
        );
        let result = (|| -> Result<()> {
            unsafe {
                let stream: ISpStream = CoCreateInstance(&SpStream, None, CLSCTX_INPROC_SERVER)?;
                let filename = wide(&part.to_string_lossy());
                let format = WAVEFORMATEX {
                    wFormatTag: 1,
                    nChannels: 1,
                    nSamplesPerSec: 22050,
                    nAvgBytesPerSec: 44100,
                    nBlockAlign: 2,
                    wBitsPerSample: 16,
                    cbSize: 0,
                };
                let wave = GUID::from_u128(0xc31adbae_527f_4ff5_a230_f62bb61ff70c);
                stream.BindToFile(
                    PCWSTR(filename.as_ptr()),
                    SPFM_CREATE_ALWAYS,
                    Some(&wave),
                    Some(&format),
                    0,
                )?;
                sapi.voice.SetOutput(&stream, false)?;
                for (i, chunk) in doc.chunks.iter().enumerate() {
                    if cancel.load(Ordering::Relaxed) {
                        bail!("Audio export cancelled.");
                    }
                    sapi.speak(&chunk.text)?;
                    let deadline = Instant::now() + Duration::from_secs(120);
                    while !sapi.done()? {
                        if cancel.load(Ordering::Relaxed) {
                            sapi.stop()?;
                            bail!("Audio export cancelled.");
                        }
                        ensure!(
                            Instant::now() < deadline,
                            "The speech engine timed out during export."
                        );
                        std::thread::sleep(Duration::from_millis(30));
                    }
                    progress((i + 1) as f32 / doc.chunks.len() as f32);
                }
                sapi.voice
                    .SetOutput(None::<&windows::core::IUnknown>, true)?;
                stream.Close()?;
            }
            ensure!(
                !path.exists(),
                "The destination was created during export; it has been preserved."
            );
            // Same-folder Windows move is atomic and refuses to overwrite an existing destination.
            use std::os::windows::ffi::OsStrExt;
            let source: Vec<u16> = part.as_os_str().encode_wide().chain(Some(0)).collect();
            let dest: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                windows::Win32::Storage::FileSystem::MoveFileW(
                    PCWSTR(source.as_ptr()),
                    PCWSTR(dest.as_ptr()),
                )
                .context("Cannot finalize WAV in the selected folder")?;
            }
            Ok(())
        })();
        drop(sapi);
        let _ = std::fs::remove_file(&part);
        result
    }
}

#[cfg(windows)]
pub use platform::*;
