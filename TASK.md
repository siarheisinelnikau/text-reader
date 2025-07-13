# TextReader

Repository: `text-reader`
Priority: 11 of 15 (user-requested launch order)
Product: TextReader
Purpose: Listen to documents with highlighting and bookmarks
Inspiration: https://www.naturalreaders.com/software.html

Independent open-source implementation; no affiliation, competitor code or copied assets.

## Requested implementation

Build a local-first text/document reader with open/paste, chapter/paragraph navigation, play/pause/stop, speed/voice choice, current-text highlighting, saved positions/bookmarks and an audio export path. Start with a real Windows installed-voice/SAPI adapter and structured text/TXT support; add practical EPUB/DOCX/PDF text extraction where feasible with compatible dependencies and truthful support labels.

Separate document extraction, sentence/chunk timing, playback and export. Test parsing/chunking/state using fixtures and a silent fake audio adapter, not actual sound output. Optional better local voices may be supported via user-requested model downloads later, but do not download large models or contact paid TTS services now. Never claim unavailable voices, scanned-PDF OCR or MP3 output works without an implemented backend. Make errors, unsupported formats and extraction limits understandable.

## Packaging and publication target

Windows standalone asset: `TextReader-vVERSION.exe` (for the initial release, `TextReader-v0.1.0.exe`). Append architecture only when distributing multiple executable architectures in the same release.
Planned canonical repository: https://github.com/siarheisinelnikau/text-reader (not yet created or published; keep availability/error reporting truthful).

## Working agreement

Follow AGENTS.md. Complete as much real implementation and safe noninteractive verification as possible in this unattended code-only session. Manual UI, device and hardware tests will happen later. Record required external components and their verification status. Keep changes committed locally in meaningful increments.

