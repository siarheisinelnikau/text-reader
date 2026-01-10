# TextReader v0.1.0 — initial release draft

An independent local document listener for Windows x64, using installed desktop SAPI voices. This is a preparation draft; no repository or release has been published by this implementation task.

## Added

- Open UTF-8/UTF-16 TXT and Markdown, extract DOCX main text and read unencrypted EPUB chapters in spine order, or paste text directly.
- Play, pause, resume and stop; select a SAPI voice and speed. Follow the current sentence or bounded fragment and navigate by paragraph, chapter or bookmark.
- Restore the last imported text and saved reading position; retain labeled bookmarks and preferences locally without modifying source documents.
- Export the entire document as PCM WAV with progress and cancellation, using the selected installed voice.
- Settings with installed version, manual update checking, an enabled-by-default persistent startup-check toggle and user-initiated verified portable installation for future stable releases.
- Original multi-resolution book icon embedded in the standalone EXE and assigned to the native window and taskbar.

## Improved

- A quieter reading room replaces truncated preview rows with wrapped passages, page navigation, a separate contents/bookmarks rail and a compact bottom player.
- Use CPU/software rasterization and Windows GDI instead of an OpenGL renderer. Retain the last bitmap and wait when no useful UI work is pending.
- Add isolated render-save-exit screenshots and a bounded visible/minimized/restored performance diagnostic.
- Preserve the selected reading page after restoration, keep pasted text on recoverable errors and allow audio export while a startup update check is pending.

## Fixed during native validation

- Retry playback through the current Windows default WinMM output when SAPI cannot find its default audio token, despite installed playback devices.
- Finalize WAV export without opening the default speaker output. A missing playback token no longer discards an otherwise successful file synthesis.
- Include operation-specific explanations with native speech errors and add an opt-in synthetic SAPI integration check.
- Ignore stale repaint requests that caused repeated scene work. In isolated five-second final samples, visible, minimized and restored idle states produced zero layout/raster/present operations.
- Initialize the Windows accessibility adapter before showing the native window.
- Fix overlapping title-bar hit regions that prevented Settings and Paste text from closing. Add a visible Done action, paste cancellation and Escape handling without discarding preferences or the current paste draft.

## Requirements and known limits

The intended target is Windows 10/11 x64 with a classic SAPI voice; the interface uses CPU rasterization and Windows GDI. The standalone asset is `TextReader-v0.1.0.exe`; the release must also include `SHA256SUMS.txt`. No installed Rust toolchain, external model, cloud account or AI API is needed to use the app.

PDF/OCR, MP3, DRM, exact word highlighting, complex document layout and OneCore-only voices are not supported. Passages wrap on a paginated reading surface. Installation requires a writable portable folder. Multiple simultaneous instances are not coordinated. Native SAPI transport and non-silent PCM WAV synthesis passed with Microsoft Zira Desktop. Physical audibility, GUI behavior, clean-machine portability and a live update round-trip still need manual validation. Real Reader and Settings client-area PNGs were generated and inspected without Computer Use. The final idle/minimized/restore diagnostic passed; subjective interaction smoothness and native dialog traversal remain unverified.
