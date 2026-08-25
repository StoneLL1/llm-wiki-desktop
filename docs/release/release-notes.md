# LLM Wiki Desktop 0.1.0

This first public release establishes the local-first desktop foundation for importing personal sources, organizing a Markdown wiki, searching and editing locally, using project-scoped workflows, and receiving signed application updates.

Release highlights:

- project trust, writable-access, Git checkpoint, provider-origin, and process-lifetime guards are enforced in the backend;
- optional OCR, ASR, browser, and media capabilities are distributed as a signed four-platform catalog;
- global update checks, release notes, cancellable downloads, install blockers, and restart-to-update are available from the desktop shell;
- Windows x64, macOS arm64, macOS x64, and Linux x64 artifacts are built from one tag and one commit;
- installers, updater signatures, capability packs, SBOMs, checksums, provenance, and packaged-smoke evidence are published atomically;
- updater and capability signatures are mandatory, while the initial Windows/macOS installers intentionally do not carry Authenticode or Apple Developer ID/notarization identity and may trigger platform warnings documented in `known-limitations.md`.

See `known-limitations.md` in the release assets before installing.
