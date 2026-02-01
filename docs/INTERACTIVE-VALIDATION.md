# Native validation and reading-room redesign — 2026-10-31

This follow-up supersedes the original code-only verification limits where evidence is available. The latest user instruction pauses Computer Use. The current design/performance phase used source inspection, actual application-generated offscreen PNGs, a bounded diagnostic window and synthetic data; no desktop input automation was used in this phase. No real user document, microphone, global Windows setting, updater installation or remote Git operation was used.

## Native speech and export

An explicit `cargo run --offline --locked --example sapi_smoke -j 2 -- --playback` exercised the production SAPI adapter. Microsoft Zira Desktop (English, United States) was enumerated and five WinMM output devices were visible. Initial normal playback failed with `0x8004503A` (`SPERR_NOT_FOUND`), while synthesis to a file worked. The adapter now retries that playback failure with a directly created Windows default WinMM renderer. It does not change the OS default device. The second native run passed completion of both passages, pause/resume, stop from pause, seek/start and purge.

The initial WAV exporter also failed after rendering all text: resetting its output to the default speaker device returned the same error and the successfully synthesized partial file was removed. Export now releases the voice and closes the stream while retaining COM initialization, without opening speaker output. File synthesis is marked explicitly and cannot enter the speaker fallback.

Actual PCM output passed RIFF/file-length, PCM encoding, mono/22,050 Hz/16-bit, nonzero-signal, progress, cancellation/partial cleanup and byte-preservation checks. Normal-rate output was **5.9934 s**, peak **22,460**, RMS **2,832.97**; rate +3 produced **4.1483 s**. Existing destinations remained unchanged. [The native report](validation/sapi-smoke-2026-10-31.json) distinguishes real backend completion from unobserved physical audibility/subjective voice quality. No cloud TTS or large model was used.

The playback/file-output approach follows Microsoft's [ISpMMSysAudio documentation](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/ms717924%28v%3Dvs.85%29) and [default WAVE_MAPPER behavior](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/ms717928%28v%3Dvs.85%29).

## Design and real captures

Applied the `product-design` skill and `rust-performance` rendering/load gate. The consumer journey is Open/Paste → Play → follow a readable passage → bookmark/return, with Export and Settings deliberately secondary. Reading is a sustained workspace, so it retains a dedicated window instead of an impractical tray-only interface.

Shared design roles are white paper/surfaces, a cool neutral canvas, dark ink, readable secondary text and one teal accent. Segoe UI is read from Windows at startup when available, with bundled egui fonts as fallback. The header has document actions, the left rail has contents/bookmarks, and the bottom player provides the dominant Play/Pause/Resume action. Text wraps on the page; only five bounded passages are laid out at once. Settings retain the required update controls and show actual installed version/local storage information.

The real EXE's `--screenshot` mode runs the same components, fonts and CPU rasterizer as normal operation. It renders four finite settling frames, saves the client bitmap and exits. It creates no native window, workers, user-profile load, network connection or audio operation. External timeout is 20 seconds. Reader, Settings and empty-state PNGs were decoded and visually inspected; a 900 × 620 logical / 150% reading capture was also reviewed. Initial review found a clipped reading surface, unfinished window fade and invisible slider rails; these were corrected and recaptured. The Settings collapse affordance was removed as unnecessary.

- [Reader](images/reader.png), [Settings](images/settings.png), [empty state](images/empty.png).
- [Capture/build manifest](validation/artifact-and-captures.json) records the final executable checksum, dimensions, scale and image hashes.

These are application-generated client-area screenshots, not invented mockups or captures of native title bars/dialogs. Static images do not establish focus traversal, click timing, file-dialog behavior or accessibility interaction.

## Renderer and load gate

The application no longer uses eframe/glow. The actual path is egui 0.33.3 → egui_software_backend 0.0.2 CPU rasterization → retained BGRA bitmap → softbuffer 0.4.8 Windows GDI `BitBlt`. Source inspection and final PE imports agree; OpenGL/DirectX renderer imports are absent. This does not disable Windows desktop composition or claim a GPU-driver diagnosis for the earlier subjective lag.

A custom winit host waits with `Wait` / `WaitUntil`, reuses retained pixels on expose, and avoids layout/raster/presentation while minimized. Background event handling and saving are independent of visibility. The Windows accessibility adapter is initialized before first visibility; the initial bounded host test exposed and fixed an ordering assertion here.

The first CPU-host idle test failed: **366 layouts/rasters/presents in five seconds**, approximately **20.625% of one core**. Minimized and restored samples were already quiet. The host was retaining stale egui repaint callbacks; cumulative-pass filtering and absolute deadlines now discard requests already satisfied by a newer frame. No FPS cap, arbitrary sleep, thread priority or system/driver setting was used as a substitute.

The final optimized executable was tested with an isolated populated fixture, no pointer/input automation, no background document/network/audio work, an internal phase schedule and a 28-second external deadline. Each settled sample lasted approximately five seconds:

| State | Layouts | Rasters | Presents | CPU, one core |
| --- | ---: | ---: | ---: | ---: |
| Visible idle | 0 | 0 | 0 | 0.00% |
| Minimized idle | 0 | 0 | 0 | 0.00% |
| Restored idle | 0 | 0 | 0 | 0.00% |

CPU uses Windows process-time resolution, not an assertion of mathematically zero activity. Actual visibility was checked. Startup/restore still render finite useful frames. See [the failed pre-fix sample](validation/cpu-gate-before-repaint-fix.json) and [the final CPU/GDI sample](validation/cpu-gate-2026-10-31.json) for timings, causes, process identity and supporting loaded-module data. The test process exited; no diagnostic instance was left running.

This passes the isolated idle/minimized/restore scheduling gate. It does not certify subjective cursor/scroll latency, all workloads, real user interaction or GPU/compositor timing. The old OpenGL window was manually closed quickly by the user because of lag; no valid numerical old-renderer baseline is claimed.

## Final checks and remaining scope

Formatting, Clippy with `-D warnings`, all 14 silent/fixture tests, optimized x64 packaging and PE/checksum verification passed. Cargo's incremental hard-link notices fell back to copying; no elevation was requested. The native example is opt-in and does not run during normal tests/CI. CI now also includes bounded offscreen Reader/Settings captures, but was not run remotely.

Remaining checks: actual input/scroll/focus/native file dialogs, physical audibility and subjective responsiveness after this redesign, screen-reader traversal, broader font/language/DPI coverage, clean-machine deployment and a live updater round-trip. Computer Use remains paused as requested. PDF/OCR, DRM and MP3 are explicitly unsupported.

## Bookmark placement follow-up

Bookmark creation now lives directly below the Contents/Bookmarks tabs in the left navigation panel. Saving opens the bookmark list, and an existing bookmark is shown as a readable teal Passage saved status. Optional naming, saved entries and removal share that panel. The detached action above the reading surface and the duplicate save button were removed. Fixed row widths also prevent a bookmark's remove button from widening the navigation rail.

Reader, Bookmarks, compact Reader and both Settings captures were regenerated through the real EXE's four-frame CPU self-capture path and visually inspected. The Bookmarks fixture shows an already saved passage; it does not simulate desktop interaction. The backend, normal repaint scheduling and worker behavior are unchanged. Formatting, Clippy, all 21 existing tests and artifact checks passed. No Computer Use or further native performance launch was performed; the previous native restored-idle limitation remains unresolved.

See [Reader](images/reader.png), [Bookmarks](images/bookmarks.png), [compact Reader at 150%](images/reader-compact.png) and the [capture manifest](validation/artifact-and-captures.json).

## Settings close regression

The reported inability to close Settings was reproduced with pointer movement, press and release events delivered directly to the real egui components. Before the repair, clicking the center of the title-bar X left Settings open. A 24-point window frame margin made the title bar tall enough for egui's double-click title region to overlap the X; that region still receives clicks when collapsing is disabled. Paste text used the same problematic margins.

Both dialogs now retain egui's compact title-bar margins and apply comfortable padding inside the content instead. Settings has a fixed Done footer outside its scrolling preferences; Paste text has Cancel. These actions explicitly close the window rather than its internal collapsing body. Escape closes the active dialog while preserving preferences or the paste draft; an expanded voice list receives Escape first. Reader playback shortcuts are inactive while a dialog is open.

Seven in-process GUI regression tests check the X at its center and edges, Done and reopening, changed preferences, voice-popup Escape priority, paste X/Cancel, a focused paste editor, complete footer text visibility, and return to a settled repaint state. The tested logical sizes/scales are 1180 × 820 at 100% and 900 × 620 at 150%. All 21 tests, Clippy and release packaging passed. Tests create no native window, workers, profile, network or audio; they establish component input behavior, not Windows input routing or subjective responsiveness.

The updated EXE produced fresh four-frame [Settings](images/settings.png) and [compact Settings](images/settings-compact.png) client-area captures, both visually inspected. The first review found the new footer clipped before layout settled; an adequate initial content height now keeps it fully visible before idle. The README image and [artifact manifest](validation/artifact-and-captures.json) are updated. Computer Use remains paused.

One subsequent bounded CPU/GDI native diagnostic exited normally. Its five-second visible and minimized samples produced zero layout/raster/present operations; the restored sample produced 1,658 of each at 56.24% of one core. The run received 1,516 cursor-moved events, so it does not establish a clean restored-idle result. No further launch or input automation followed. The [raw diagnostic and assessment](validation/settings-close-gate-2026-10-31.json) retain the failing result; the earlier clean gate belongs to its earlier build. The component closing fix and static review pass, while full current-build idle/restore and perceived responsiveness remain unverified.

## Native icon follow-up

Added the original teal book mark as a multi-resolution Windows ICO (16, 20, 24, 32, 40, 48, 64, 128 and 256 px), neutral-language EXE resources, and explicit window/taskbar icon attributes. Icon decoding occurs once during native window creation, outside the render loop. Cargo embeds the committed ICO with the existing Windows SDK resource compiler; Pillow is only an asset-regeneration tool. No separate icon file is required by the distributed EXE.

The final PE check verifies `RT_GROUP_ICON` ID 1, all nine referenced `RT_ICON` payloads, required shell sizes and payload hashes against the committed ICO. The asset variants were decoded and visually inspected at small sizes. Formatting, Clippy, 14 tests, release packaging and checksum verification passed. See the [icon validation record](validation/icon-2026-10-31.json). The running user instance was closed by the user before replacement; it was not terminated by a tool.

Bounded native launches successfully exercised icon initialization. Their initial visible intervals received actual pointer motion, so those intervals are not used as new clean-idle evidence. One classified run observed 102 changed cursor positions and no duplicates; an experimental duplicate filter was removed rather than presenting it as a verified fix. Minimized and restored intervals had zero layout/raster/present operations. The earlier clean table above remains evidence for the redesign build, not a claim that the input-contaminated icon runs passed all idle criteria. No further desktop automation was performed. Existing client screenshots remain accurate because native icons do not alter their captured surface; their capture-build hashes are retained in the manifest.
