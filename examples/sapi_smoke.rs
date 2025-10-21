//! Opt-in native file-synthesis check. --playback additionally plays synthetic speech.
//! Normal cargo test only compiles the example; it never runs this main function.

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use std::{
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use text_reader::{
        document::Document,
        playback::{Audio, Player, State},
        speech::{self, Sapi},
    };

    fn wave_stats(path: &Path) -> anyhow::Result<serde_json::Value> {
        let bytes = std::fs::read(path)?;
        ensure!(
            bytes.len() >= 44 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE",
            "Invalid RIFF/WAVE header"
        );
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        ensure!(
            u32_at(4) as usize + 8 == bytes.len(),
            "RIFF size does not match file length"
        );
        let mut offset = 12usize;
        let mut format = None;
        let mut pcm = None;
        while offset + 8 <= bytes.len() {
            let size = u32_at(offset + 4) as usize;
            let start = offset + 8;
            let end = start.checked_add(size).context("WAV chunk overflow")?;
            ensure!(end <= bytes.len(), "Truncated WAV chunk");
            match &bytes[offset..offset + 4] {
                b"fmt " => {
                    ensure!(size >= 16, "Truncated format chunk");
                    format = Some((
                        u16_at(start),
                        u16_at(start + 2),
                        u32_at(start + 4),
                        u16_at(start + 14),
                    ));
                }
                b"data" => pcm = Some(&bytes[start..end]),
                _ => {}
            }
            offset = end + (size % 2);
        }
        let (encoding, channels, rate, bits) = format.context("Missing WAV format")?;
        ensure!(
            (encoding, channels, rate, bits) == (1, 1, 22050, 16),
            "Unexpected PCM output format: {format:?}"
        );
        let pcm = pcm.context("Missing WAV data")?;
        ensure!(
            !pcm.is_empty() && pcm.len().is_multiple_of(2),
            "Empty or incomplete PCM payload"
        );
        let mut square_sum = 0f64;
        let mut peak = 0i32;
        let mut nonzero = 0usize;
        for pair in pcm.chunks_exact(2) {
            let sample = i16::from_le_bytes([pair[0], pair[1]]) as i32;
            square_sum += (sample as f64).powi(2);
            peak = peak.max(sample.abs());
            nonzero += usize::from(sample != 0);
        }
        let frames = pcm.len() / 2;
        let seconds = frames as f64 / rate as f64;
        ensure!(
            seconds > 0.3 && nonzero > frames / 20 && peak > 100,
            "WAV has no substantial speech signal"
        );
        Ok(
            serde_json::json!({"file":path.file_name().unwrap().to_string_lossy(),"bytes":bytes.len(),"channels":channels,"sample_rate":rate,"bits_per_sample":bits,"duration_seconds":seconds,"peak":peak,"rms":(square_sum/frames as f64).sqrt(),"nonzero_samples":nonzero}),
        )
    }

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    ensure!(
        args.iter().all(|a| a == "--playback"),
        "Supported option: --playback (plays short synthetic speech)"
    );
    let playback_requested = args.iter().any(|a| a == "--playback");
    let output_device_count = unsafe { windows::Win32::Media::Audio::waveOutGetNumDevs() };
    println!("Visible WinMM playback devices: {output_device_count}");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".test-output");
    std::fs::create_dir_all(&root)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let dir = root.join(format!("native-sapi-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&dir)?;
    println!("Synthetic native integration artifacts: {}", dir.display());
    let doc = Arc::new(Document::from_text(
        "Native SAPI check".into(),
        "This is a TextReader playback check. The second sentence checks speech completion.".into(),
    )?);
    let mut audio = Sapi::new()?;
    let voices = audio.voices()?;
    ensure!(
        !voices.is_empty(),
        "No installed classic SAPI voices were enumerated"
    );
    for voice in &voices {
        audio.configure(&voice.id, 0)?;
        println!("Installed voice: {}", voice.name);
    }
    let voice = voices
        .iter()
        .find(|v| v.name.contains("David") || v.name.contains("Zira"))
        .unwrap_or(&voices[0]);
    let voice_id = voice.id.clone();
    let mut player = Player::new(audio);
    let playback = if playback_requested {
        (|| -> anyhow::Result<Vec<usize>> {
            println!(
                "Playing synthetic speech with {}; checking pause/resume/stop.",
                voice.name
            );
            player
                .start(doc.clone(), 0, voice_id.clone(), 0)
                .context("Initial playback start failed")?;
            std::thread::sleep(Duration::from_millis(120));
            player.pause()?;
            std::thread::sleep(Duration::from_millis(200));
            ensure!(
                player.state == State::Paused && !player.tick()? && player.cursor == 0,
                "Paused playback advanced"
            );
            player.resume()?;
            let deadline = Instant::now() + Duration::from_secs(30);
            let mut visited = vec![player.cursor];
            while player.state != State::Finished {
                ensure!(Instant::now() < deadline, "SAPI playback timed out");
                if player.tick()? && visited.last() != Some(&player.cursor) {
                    visited.push(player.cursor);
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            ensure!(
                visited == vec![0, 1],
                "Unexpected completion/cursor sequence: {visited:?}"
            );
            player.start(doc.clone(), 0, voice_id.clone(), 0)?;
            player.pause()?;
            player.stop()?;
            ensure!(
                player.state == State::Stopped,
                "Stop did not reset paused playback"
            );
            player.start(doc.clone(), 1, voice_id.clone(), 0)?;
            player.stop()?;
            let deadline = Instant::now() + Duration::from_secs(2);
            while !player.audio.done()? {
                ensure!(Instant::now() < deadline, "SAPI purge timed out");
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(visited)
        })()
    } else {
        Ok(vec![])
    };
    if playback_requested {
        let _ = player.stop();
    }
    let playback_error = playback.as_ref().err().map(|e| format!("{e:#}"));
    if let Some(error) = &playback_error {
        eprintln!("Playback check failed; continuing independent WAV checks: {error}");
    }
    let visited = playback.unwrap_or_default();
    drop(player);

    let cancel = AtomicBool::new(false);
    let normal = dir.join("normal.wav");
    let mut progress = Vec::new();
    speech::export(&doc, &normal, &voice_id, 0, &cancel, |p| progress.push(p))?;
    ensure!(
        progress.last() == Some(&1.0) && progress.windows(2).all(|w| w[0] <= w[1]),
        "Invalid export progress"
    );
    let normal_stats = wave_stats(&normal)?;
    let preserved = std::fs::read(&normal)?;
    ensure!(
        speech::export(&doc, &normal, &voice_id, 0, &cancel, |_| {}).is_err(),
        "Export unexpectedly overwrote an existing destination"
    );
    ensure!(
        std::fs::read(&normal)? == preserved,
        "Existing WAV was modified"
    );
    let fast = dir.join("fast.wav");
    speech::export(&doc, &fast, &voice_id, 3, &cancel, |_| {})?;
    let fast_stats = wave_stats(&fast)?;
    ensure!(
        fast_stats["duration_seconds"].as_f64().unwrap()
            < normal_stats["duration_seconds"].as_f64().unwrap(),
        "Increased speech rate did not shorten export"
    );
    let cancelled = dir.join("cancelled.wav");
    ensure!(
        speech::export(&doc, &cancelled, &voice_id, 0, &cancel, |_| cancel
            .store(true, Ordering::Relaxed))
        .is_err(),
        "Export did not observe cancellation"
    );
    ensure!(
        !cancelled.exists()
            && !cancelled
                .with_extension(format!("{}.partial.wav", std::process::id()))
                .exists(),
        "Cancelled export left an output/partial file"
    );
    let report = serde_json::json!({"voices":voices.iter().map(|v|&v.name).collect::<Vec<_>>(),"selected_voice":voice.name,"visible_winmm_playback_devices":output_device_count,"playback_requested":playback_requested,"playback_error":playback_error,"playback_passed":if playback_requested {Some(playback_error.is_none())}else{None},"physical_audibility":"not observed","visited_fragments":visited,"normal":normal_stats,"fast":fast_stats,"preservation":"passed","cancellation":"passed","gui_and_screenshots":"not exercised by this backend-only check"});
    std::fs::write(dir.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    ensure!(
        playback_error.is_none(),
        "Playback check failed; see report.json for the independently completed WAV checks"
    );
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This opt-in integration check requires Windows and installed SAPI voices.");
}
