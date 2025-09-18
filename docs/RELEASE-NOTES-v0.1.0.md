# TextReader v0.1.0 — initial release draft

An independent local document listener for Windows x64, using installed desktop SAPI voices. This is a preparation draft; no repository or release has been published by this implementation task.

## Added

- Open UTF-8/UTF-16 TXT and Markdown, extract DOCX main text and read unencrypted EPUB chapters in spine order, or paste text directly.
- Play, pause, resume and stop; select a SAPI voice and speed. Follow the current sentence or bounded fragment and navigate by paragraph, chapter or bookmark.
- Restore the last imported text and saved reading position; retain labeled bookmarks and preferences locally without modifying source documents.
- Export the entire document as PCM WAV with progress and cancellation, using the selected installed voice.
- Settings with installed version, manual update checking, an enabled-by-default persistent startup-check toggle and user-initiated verified portable installation for future stable releases.

## Requirements and known limits

The intended target is Windows 10/11 x64 with a classic SAPI voice and an OpenGL-capable driver. The standalone asset is `TextReader-v0.1.0.exe`; the release must also include `SHA256SUMS.txt`. No installed Rust toolchain, external model, cloud account or AI API is needed to use the app.

PDF/OCR, MP3, DRM, exact word highlighting, complex document layout and OneCore-only voices are not supported. List previews are compact; the active fragment appears in full. Installation requires a writable portable folder. Multiple simultaneous instances are not coordinated. GUI, audible speech, real WAV output, clean-machine portability and a live update round-trip still require manual validation. Screenshots are pending because desktop operation was prohibited during implementation.
