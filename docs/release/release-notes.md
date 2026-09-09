# LLM Wiki Desktop v0.2.1

A more focused workflow for collecting sources, maintaining your wiki, and protecting local changes.

## Highlights

- **Clearer imports:** streamlined preparation, queue status, and completion summaries; improved supported Xiaohongshu page extraction and browser/media runner behavior.
- **Simpler Workflows:** Update Wiki, Health Check, and content generation have focused controls, responsive navigation, and a system file picker for output paths.
- **Local version history:** version protection and recovery are available across supported wiki, Source, chat-save, and lint operations, with Git preflight checks and visible recovery settings.
- **Better health checks:** unified scans, clearer issue details, and a dedicated management panel for repairs.
- **Cleaner downloads:** desktop installers and automatic-update files are listed here; optional OCR, transcription, and other engines are downloaded separately from within the app.

## Download and install

| Platform | Installer |
| --- | --- |
| macOS · Apple Silicon | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.1/darwin-aarch64-LLM.Wiki.Desktop_0.2.1_aarch64.dmg) |
| macOS · Intel | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.1/darwin-x86_64-LLM.Wiki.Desktop_0.2.1_x64.dmg) |
| Windows · x64 | [Setup EXE](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.1/windows-x86_64-LLM.Wiki.Desktop_0.2.1_x64-setup.exe) |
| Linux · x64 | [AppImage](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.1/linux-x86_64-LLM.Wiki.Desktop_0.2.1_amd64.AppImage) |

The `.app.tar.gz` files and `latest.json` support automatic updates. `CHECKSUMS.sha256` covers the public desktop downloads. Optional engine archives live in the separate [capability release](https://github.com/StoneLL1/llm-wiki-desktop/releases/tag/capabilities-v0.2.1); most users should let the app install them as needed.

## Updating from v0.2.0

Use **Check for updates**, or quit the old app and install v0.2.1 over it. Keep your knowledge-base folder in place. The application identity and update-signing key are unchanged.

macOS binaries are not Apple-notarized, and Windows installers do not carry an Authenticode identity. First-launch OS warnings may appear. See [installation notes and limitations](https://github.com/StoneLL1/llm-wiki-desktop/blob/app-v0.2.1/docs/release/known-limitations.md).

Source CI, signed-artifact verification, and packaged launch checks are part of the release pipeline. Hosted checks do not constitute a completed interactive upgrade and rollback test on every supported operating system.

[Full changelog](https://github.com/StoneLL1/llm-wiki-desktop/compare/app-v0.2.0...app-v0.2.1) · [Quick start](https://github.com/StoneLL1/llm-wiki-desktop#get-started)
