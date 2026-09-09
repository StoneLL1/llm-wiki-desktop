# Installation notes and limitations — v0.2.1

## First launch

- **macOS:** open the DMG and drag LLM Wiki Desktop into Applications. The app is not signed with Apple Developer ID or notarized. If macOS blocks opening it, verify you downloaded the official release and its checksum, then use the per-app **Open Anyway** option in **System Settings → Privacy & Security** if offered. Do not disable Gatekeeper globally.
- **Windows:** run the x64 setup executable. The installer has no Authenticode identity, so SmartScreen or an unknown-publisher warning may appear. Confirm the official download and checksum before choosing the system's option to continue.
- **Linux:** add executable permission to the AppImage, then launch it. Ubuntu 24.04 x64 is the CI target. FUSE, WebKitGTK, keyring, and desktop integration vary by distribution; AppImage extraction mode can be used when FUSE is unavailable.

The updater and capability packages are signature-verified independently of OS vendor identity signing. A lost update-signing key requires a deliberate migration; unsigned updates are not accepted.

## Data and AI

- Knowledge-base content remains Markdown and local files. Compatible vaults keep their layout; app guidance stays under `.app/compat/`.
- AI features need a configured provider or supported Agent CLI and send task content through that route. Local keyword/filter search does not call a model.
- Protected writes need a usable Git installation and successful version-history preflight. Keep a separate backup of important knowledge bases.
- Capability engines are trusted application components, not an OS sandbox for hostile code. Large OCR, layout, and transcription packs include runtimes and models; actual sizes are listed in the signed catalog. Accurate document-layout is unavailable on Intel macOS, where document-standard provides the supported fallback.
- Web extraction depends on accessible source pages. Login walls, protected content, removed posts, and site changes can prevent import; the app does not bypass access controls.

## Verification scope

The release pipeline checks source tests, four target builds, updater signatures,
catalog integrity, and packaged launch behavior. Hosted smoke tests do not prove
all interactive installation, old-version upgrade, restart, rollback, and uninstall
journeys on every real machine. The installed-v0.2.0-to-v0.2.1 interactive upgrade
matrix remains follow-up validation; no unexecuted journey is claimed as passed.
