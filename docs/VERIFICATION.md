# Verification record — 2026-10-31

All work and commands were scoped to `E:\work\github\text-reader`. The application, speech adapter and updater helper were not launched. No GUI, audio output, microphone, screen capture, real user document, service, driver, credential or remote Git operation was used. Tests operated on synthetic fixtures only. The task reference and public API documentation were read without browser UI automation.

## Environment

- Windows/MSVC host; Rust `1.96.1 (31fca3adb 2026-06-26)`, Cargo `1.96.1 (356927216 2026-06-26)`.
- Dependencies resolved from the existing local Cargo cache, with the resulting lockfile committed. Offline builds succeeded. Cargo used two jobs and a repository-local `target/`.
- Native interface: eframe/egui 0.37.0, glow; Windows adapter: windows 0.62.2 SAPI bindings.

## Completed checks

| Check | Actual result |
| --- | --- |
| `cargo check --offline -j 2` | Passed for the Windows native application. |
| `cargo fmt --all -- --check` | Passed on final source. |
| `cargo clippy --offline --locked --all-targets -j 2 -- -D warnings` | Passed. Cargo emitted filesystem hard-link fallback notices, not source lint errors. |
| `cargo test --offline --locked --all-targets -j 2` | Passed: 10 library tests, 4 import integration tests, 0 failures. The binary test harness ran zero tests and did not execute application main. |
| `scripts/package.ps1 -Offline` | Passed: optimized Windows x64 standalone executable and checksum file generated. Final rebuild includes the fixed PCM output-format flag. No executable was launched. |
| `scripts/verify-artifact.ps1` | Passed: PE signature, x64 machine, PE32+, Windows GUI subsystem, DLL imports and matching package SHA-256. No dynamically imported VCRUNTIME/MSVCP/API CRT dependency. |
| PowerShell script syntax | Both scripts parsed and executed successfully. |
| `git diff --check` | Passed before final commit. |

The 14 tests cover Unicode and bounded chunking (including separator accounting), headings and paragraph navigation, content identity, BOM decoding, XML entities/hidden XHTML text, safe archive paths, pause/resume/stop/seek/completion with a silent fake, persistent settings/bookmarks and corrupt-session preservation, semver/stable release selection, origin and missing-checksum rejection, checksum name matching, synthetic replacement integrity/preservation, deflate-compressed DOCX paragraphs, EPUB spine order, DRM/missing-part/PDF errors, and the repository reading fixture.

The initial session/replacement tests encountered access denied while moving synthetic files in the system temporary directory under the sandbox. Fixtures were moved to the repository's ignored `.test-output/` directory, where the Windows persistence and replacement checks passed. Test-generated files were removed by the passing tests. Cargo's incremental cache could not hard-link some files in this environment and fell back to copying; builds and checks still completed. No elevation or environment changes were requested.

## Prepared artifact

- File: `dist/TextReader-v0.1.0.exe`
- Size: **10,208,256 bytes**
- SHA-256: `12b6642d7f63e3f1a3869ce26c5a6a22faabbf23f7ee89f5da3fd8d44b3d52db`
- Companion: `dist/SHA256SUMS.txt`
- Imports: Windows system DLLs including SAPI's COM infrastructure, UI Automation, OpenGL/GDI, shell, networking and cryptography. No external document extractor, WebView, speech script or model is packaged.

Build artifacts are intentionally ignored by Git and remain locally available in `dist/`. The CI workflow is prepared but has not run remotely. The canonical GitHub repository and release were not created or published.

## Not verified

No visible GUI/layout/DPI checks, installed-voice enumeration, audible SAPI playback, real PCM WAV export, clean-machine deployment, graphics compatibility, measured idle CPU, screen-reader usability or live updater round-trip was performed. Source compilation and silent state/fixture tests do not establish those results. Application and Settings screenshots are absent by explicit user instruction. See [MANUAL-TESTS.md](MANUAL-TESTS.md) for the required authorized follow-up.

PDF/OCR, DRM and MP3 remain deliberately unsupported rather than unverified working capabilities. No cloud TTS or AI API backend is included.
