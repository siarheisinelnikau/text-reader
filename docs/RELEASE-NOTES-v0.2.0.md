# TextReader 0.2.0

TextReader now lets you choose your interface language and switch between a light reading room and a coordinated dark theme without restarting.

## Added

- English, Russian, French, German, Spanish and Italian interface catalogs, covering the reader, bookmarks, playback actions, settings, paste dialog, status messages, file-filter labels and update controls.
- A saved **Settings → Language** preference with native language names. Changing the interface language leaves document text, saved bookmarks and the installed speech voice unchanged.
- Saved **Settings → Appearance → Light / Dark** controls, applied immediately to reading surfaces, navigation, settings, text fields, selections and error notices.
- Developer-only capture options for selecting a language and theme in the existing isolated render-save-exit mode.

## Improved

- Existing session files receive English/light defaults while preserving voices, positions, bookmarks and the startup-update preference.
- Explicit Light / Dark preferences remain active when Windows reports a different system theme.
- Status messages retain structured parameters so switching language also updates an existing status without rewriting document titles or paths.
- Screenshot and diagnostic scripts obtain the executable version from Cargo.toml.

## Verification and limits

Automated coverage includes all six catalogs and message parameters, session migration and round trips, actual egui language/theme controls, reading-state preservation, and settled UI repaint scheduling. Native Windows file-dialog buttons and operating-system/parser diagnostic details retain their original language.

Windows x64 standalone executable: `TextReader-v0.2.0.exe`, with `SHA256SUMS.txt`. Existing settings and the user-initiated in-app updater contract are preserved. A live release download/install round trip and clean-machine compatibility remain unverified; no updater was executed during development.
