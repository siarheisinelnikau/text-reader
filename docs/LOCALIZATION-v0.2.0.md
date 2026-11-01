# TextReader 0.2.0 localization and appearance validation

Date: 2026-11-01. Scope: first-party UI localization, session migration, coherent light/dark styling, actual application captures, artifact checks and a bounded isolated lifecycle diagnostic. No desktop input automation, user document access, sound playback or updater execution was used.

## Implemented behavior

Settings exposes **Language** with English, Русский, Français, Deutsch, Español and Italiano, plus **Appearance → Light / Dark**. Both choices apply immediately and save with the normal session. Voice choice and document language remain independent. Previous sessions receive English/light defaults, and unknown future appearance identifiers fall back without discarding a session.

The explicit catalog contains 92 source messages and 552 language values. It covers primary actions, navigation, bookmarks, playback, settings, paste prompts, file-filter labels, progress/status messages and update controls. Status parameters are expanded in a single pass over the original template, preserving titles/paths containing braces. A source audit found no remaining literal first-party textual action labels in `view.rs`; the product name, format names, icons, stable widget IDs and user content are intentionally unchanged. Native dialog controls follow Windows. Operating-system/parser diagnostic details remain in their original language inside a localized error envelope.

The original teal identity is retained. Both palettes define readable text, secondary text, surfaces, canvas, borders, selection/highlight and error roles. Theme changes pin egui’s explicit Light/Dark preference and apply the style only when a preference changes or a session loads. An opposite Windows system-theme event cannot switch to an unconfigured style branch. Fonts load once at construction; changing language/theme introduces no recurring file I/O or redraw timer.

## Automated checks

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --offline --all-targets -- -D warnings`: passed. No Clippy diagnostics were reported.
- `cargo test --locked --offline --all-targets`: 26 passed (12 library, 9 actual egui component tests, 1 opposite-system-theme regression, 4 document-import tests). The silent SAPI example has no tests and was not executed.
- Optimized Windows x64 build with `-C target-feature=+crt-static`: passed. Cargo checks/builds used the shared three-slot limiter, two jobs per invocation and repository-local target output.
- Artifact verifier: x64 PE32+, Windows GUI subsystem, matching SHA-256, required embedded icon resources at 16/20/24/32/40/48/64/128/256 px. Native DrawIconEx resource proof inspected on light/dark backgrounds; book artwork is intact.

The opposite-system-theme regression first reproduced the pre-fix Light-to-Dark override, then verified both saved choices retain context theme, custom palette and typography across opposite/matching/opposite OS-theme events. The UI regressions click the actual Language and Dark controls in an in-process fixture, verify dirty/persisted preferences, keep Settings open across localization, return with translated Done, preserve voice/cursor, and settle to no repaint deadline. All six languages were checked at 900 × 620 logical pixels and 150% scaling. Storage tests preserve legacy voice/positions/startup opt-out and round-trip appearance choices; catalog tests retain document-title placeholder characters literally.

## Real rendered evidence

After pinning the explicit theme preference, the exact rebuilt `TextReader-v0.2.0.exe` generated six isolated render-save-exit captures, four finite CPU frames each. Every process exited successfully. All six image hashes matched the earlier captures, and the new PNGs were reopened and reviewed:

| Screen | Language/theme | Size and scaling | Result |
| --- | --- | --- | --- |
| Reader | English, light | 1180 × 820, 100% | Reviewed; [README image](images/reader.png) |
| Settings | English, light | 1180 × 820, 100% | Reviewed; [README image](images/settings.png) |
| Reader | English, dark | 1180 × 820, 100% | Reviewed; [README image](images/reader-dark.png) |
| Settings | Russian, dark | 1180 × 820, 100% | Reviewed; [README image](images/settings-russian-dark.png) |
| Reader | German, dark | 900 × 620, 150% | Reviewed in isolated test output; primary actions legible |
| Settings | French, light | 900 × 620, 150% | Reviewed in isolated test output; scrolling content and pinned Done legible |

These images establish the client-area layout and palettes. They do not depict OS dialogs, taskbar interactions or speech playback.

## Bounded lifecycle measurements

Source review confirmed `egui_software_backend` CPU rasterization, a retained BGRA bitmap and softbuffer's Windows GDI presentation. The event loop uses Wait/WaitUntil, skips hidden rasterization, and invalidates on useful events. The updated packaged binary ran one isolated populated diagnostic as PID 39892 with no workers/network/audio or injected input. It exited normally within the 28-second external deadline.

| Settled state | Sample | CPU, one core | Layouts | Rasters | Presents |
| --- | --- | --- | --- | --- | --- |
| Visible | 5.000 s | 0.00% | 0 | 0 | 0 |
| Minimized | 5.000 s | 0.31% | 0 | 0 | 0 |
| Restored | 5.000 s | 0.00% | 0 | 0 | 0 |

Visible/restored machine-normalized CPU is 0.00%; the minimized sample recorded 0.31% of one core (about 0.016% machine-normalized across 20 logical processors) without any layout, rasterization or presentation. Four useful total frames were produced across startup and lifecycle transitions. The native window reported 16px/32px small/large icon handles at 100% scale. See the [raw measured report](validation/localization-v0.2.0-gate.json).

The isolated idle/minimized/restore rendering gate passed. GPU telemetry, subjective interaction smoothness, arbitrary real documents/voices, OS-native dialog localization, clean-machine compatibility and a live release download/install round trip remain outside this check. The diagnostic does not measure normal speech/background worker load; those paths were unchanged.

## Prepared artifact

- `dist/TextReader-v0.2.0.exe`, 6,969,344 bytes, Windows x64.
- SHA-256: `82943ee3e43e926d53ad078f2bbeea6ebd78da18926a34e1dc800f32bf7f0556`.
- `dist/SHA256SUMS.txt` and the companion updater MIT notice are prepared. Root publication coordination owns GitHub source/release operations.
