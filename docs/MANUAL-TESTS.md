# Deferred manual checks

These checks were not run during the unattended session. They require explicit permission to launch the GUI and, for speech, to play audio. Use synthetic documents and a disposable local data profile; do not use private user documents.

- Launch the packaged x64 EXE on Windows 10 and 11, including a clean machine without the Rust toolchain. Confirm Windows DLL imports, OpenGL driver availability, font coverage and display scaling at 100%, 150% and 200%.
- Capture real Reader and Settings screenshots and embed them in README with English captions. Inspect narrow-window layouts, long status messages, bookmarks and large font sizes.
- Enumerate installed classic SAPI voices; test no-voice/unavailable-voice failure. Check two different installed voices and languages where available. Confirm that app startup emits no sound.
- Play synthetic multi-paragraph text. Confirm active-fragment highlighting tracks audible output, pause/resume works within the fragment, Stop and chapter/paragraph/bookmark navigation purge old speech, and Play restarts at the selected fragment. Check rapid repeated navigation for stale speech or cursor changes.
- Inspect long-document scrolling and Follow mode, including distant bookmarks. Read the full selected fragment beneath compact previews. Confirm idle, paused and minimized operation do not busy-poll. Measure actual packaged CPU/frame behavior before claiming responsiveness or low CPU.
- Export a short and a longer document to WAV, inspect RIFF/PCM sample properties and listen to the result. Confirm cancellation cleans partial output, existing destinations are preserved, invalid/unwritable destinations show useful errors, and voices do not play to speakers during export. Test target folders on NTFS and removable filesystems.
- Restart and confirm cached text, position, bookmarks, voice, speed, font size and startup-update preference persist. Corrupt a disposable session and confirm no overwrite; recover from the previous-session backup. Check concurrent shutdown/save behavior and save failure in a read-only profile.
- Check TXT BOM variants and real, non-DRM DOCX/EPUB files. Confirm accepted extraction limitations, malformed ZIP/XML errors, no external-resource fetches and deliberate PDF/DRM rejection.
- In an authorized future release environment, verify manual/startup update checks, 404/unpublished release, offline, rate limit, stable/prerelease selection and persistent opt-out (zero startup requests when disabled). Do not confuse error with up-to-date.
- Using an explicitly prepared disposable portable installation and published test assets, verify download progress, checksum mismatch, truncated payload, installation-folder permissions, wait-for-exit, rollback, relaunch and unchanged local data. Confirm helper error reporting on next manual launch. No live update installation was run during implementation.

The canonical repository/release is planned and was not created or published by this task. No claims of verified sound quality, hardware support, minimum OS, update-service availability or screenshot accuracy should be made before these checks.
