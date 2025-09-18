# TextReader

A local-first Windows document listener with installed SAPI voices, current-fragment highlighting, saved positions, bookmarks and WAV export. Version 0.1.0 is an independent MIT-licensed implementation. It is not affiliated with NaturalReader and includes no competitor code or assets.

## Read and listen

1. Open **TextReader-v0.1.0.exe**. Use **Open document…** for TXT, Markdown, DOCX or an unencrypted EPUB, or **Paste text…** for copied text.
2. Choose an installed SAPI voice and speed in **Settings**. No cloud voice, AI service or model download is needed. Only classic desktop SAPI voices are enumerated; Windows Narrator/OneCore voices may not be exposed by SAPI.
3. Click a fragment, chapter or bookmark to choose a position, then press **Play**. **Pause** / **Resume** retain the active speech. **Stop** retains the selected fragment; playing after Stop starts that fragment again.
4. Navigate with **Previous paragraph** / **Next paragraph**. Plain-text `#` / `##` headings and EPUB spine items provide chapters. DOCX is imported as a linear reading section with paragraph navigation.
5. Add an optional label and click **Bookmark this fragment**. A maximum of 200 bookmarks per document is supported. The last document, positions and bookmarks restore on the next launch, without automatically playing sound.
6. **Export WAV…** exports the entire document using the selected voice and speed to a new `.wav` file. The existing destination is never overwritten. Progress and cancellation are available. MP3 is not included.

Highlighting follows the sentence or bounded fragment submitted to SAPI and advances only after the speech engine reports completion. It is not word-level alignment. Long sentences are divided into fragments of at most 300 Unicode scalar values. The scrollable list uses compact single-line previews; the selected fragment is shown in full underneath and any preview has a full-text tooltip. Turn off **Follow current fragment** to browse while listening. Chapter/paragraph/bookmark navigation stops playback; press Play to resume from that position. Speed and voice changes also stop playback.

## Format support and limits

| Format | Implemented behavior |
| --- | --- |
| TXT, MD | UTF-8, UTF-8 BOM, UTF-16 LE/BE with BOM; blank-line paragraphs; Markdown-style chapter headings. Legacy code pages are rejected with an explanation. |
| DOCX | Main `word/document.xml` text and paragraphs. Formatting, images, headers, footnotes and embedded files are omitted. Old `.doc` files are not supported. |
| EPUB | Unencrypted ZIP-based EPUB, package manifest and spine reading order, XHTML body text, chapter navigation. Scripts/styles are ignored. Images, layout and navigation-tree metadata are omitted. Percent-encoded or fragment-bearing spine paths and encryption manifests are rejected. |
| PDF | Not included in 0.1. Export/copy text to TXT or paste it. No OCR or scanned-document support. |
| Audio export | PCM WAV, mono, 22.05 kHz, 16 bit through SAPI. Actual engine output needs manual verification. No MP3 encoder. |

Imports are limited to 32 MiB per input file, 8 MiB per XML archive part and 4 MiB extracted text. EPUB limits are 5,000 archive entries, 1,000 spine chapters and 32 MiB expanded chapter markup. Archive parts are read in memory, never unpacked to the filesystem. Extraction does not fetch external resources or resolve external entities. Malformed archives/XML and unsupported formats produce errors.

## Local data and network

Session data is stored, unencrypted, in `%LOCALAPPDATA%\TextReader\session.json`. It includes the last document's extracted text, bookmarks, reading positions, font size, SAPI voice ID, speed and startup-update preference. Original document files are never changed. Reading positions use normalized text content identities, so reopening an unchanged document under another filename keeps its position. A new document replaces the cached text; positions and bookmarks for previously opened content remain.

Saves run on a separate worker, with an atomic Windows replacement and `session.previous.json` backup. Closing waits for pending session saving. A failed save offers **Retry saving** and **Close without saving**. A malformed session is preserved and saving stays disabled for that launch; close TextReader and back up/reset `session.json` to recover. To remove cached text, close the application and remove its local data directory. Avoid running multiple instances against the same profile; multi-instance synchronization is not implemented.

The only built-in network operation is release checking/downloading from the planned canonical repository [siarheisinelnikau/text-reader](https://github.com/siarheisinelnikau/text-reader). **Settings → Updates** shows the installed version, **Check for updates**, and a persistent **Check for updates on startup** option, enabled by default. Turning it off prevents startup requests; manual checks still work. Startup checks run in the background and do not show no-update or transient-error dialogs. Errors remain visible in Settings. Documents are not uploaded.

**Download and install** appears when a newer stable release has the compatible executable and `SHA256SUMS.txt`. The download is user initiated, bounded, checked for complete length and SHA-256, then installed by a temporary copy of the current executable after the app saves and exits. The helper rechecks integrity, keeps a rollback copy during replacement and restarts the app. Preferences and documents stay in the data directory. A writable portable-app folder is required; there is no elevation. Installation failure is recorded in `update-error.txt` and shown on the next manual launch. The updater infrastructure has no published release to exercise in this task; it must not be treated as a tested live update service. Executable updates use the existing installation filename, even if it contains the old version.

## Build and verification

The desktop target is Windows x64, with an installed classic SAPI voice and an OpenGL-capable driver for the egui/eframe glow renderer. No WebView, PowerShell speech script, external voice package, service or paid API is required. A Rust/MSVC toolchain and Windows SDK are needed to build, not to read documents in the packaged app. The release script statically links the MSVC CRT for the application; Windows system DLLs and graphics drivers remain runtime requirements. Hardware support and minimum Windows version remain to be confirmed; Windows 10/11 x64 are the intended targets.

```powershell
cargo fmt --all -- --check
cargo clippy --locked --all-targets -j 2 -- -D warnings
cargo test --locked --all-targets -j 2
.\scripts\package.ps1
.\scripts\verify-artifact.ps1
```

Cargo jobs are capped at two with project-local `target/`. The tests generate isolated synthetic DOCX/EPUB and session/update fixtures and use a silent fake audio adapter. Tests do not initialize SAPI, play/record audio, contact the network, open windows, or run an updater helper. The package script builds `dist/TextReader-v0.1.0.exe` and `dist/SHA256SUMS.txt` without launching either. CI performs the same noninteractive checks on Windows and produces an artifact; it does not publish a GitHub release.

See [architecture](docs/ARCHITECTURE.md), [manual verification](docs/MANUAL-TESTS.md), [verification record](docs/VERIFICATION.md) and [initial release notes](docs/RELEASE-NOTES-v0.1.0.md).

## Screenshots

Real application and Settings screenshots are pending. Desktop interaction, application launch and screen capture were explicitly prohibited during this implementation session. No mockups or fabricated screenshots are substituted. Capture and add real images after the authorized manual validation pass.

## License and provenance

MIT; see [LICENSE](LICENSE). Dependencies retain their licenses. `cargo tree --locked` identifies the exact locked dependency set. The task reference [NaturalReader software](https://www.naturalreaders.com/software.html) informed the document-listening use case only; the source, compact reader interface and assets are original.
