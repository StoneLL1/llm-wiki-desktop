# LLM Wiki Desktop 0.2.0

This first public release establishes the local-first desktop foundation for importing personal sources, organizing a Markdown wiki, searching and editing locally, using project-scoped workflows, and receiving signed application updates. The release candidate coordinate is `app-v0.2.0-rc.1`; the 0.1.0 coordinate was retired without any stable publication (see `CHANGELOG.md`).

Release highlights:

- project trust, writable-access, Git checkpoint, provider-origin, and process-lifetime guards are enforced in the backend;
- optional OCR, ASR, browser, and media capabilities are distributed as a signed four-platform catalog;
- global update checks, release notes, cancellable downloads, install blockers, and restart-to-update are available from the desktop shell;
- Windows x64, macOS arm64, macOS x64, and Linux x64 artifacts are built from one tag and one commit;
- installers, updater signatures, capability packs, SBOMs, checksums, provenance, and packaged-smoke evidence are published atomically;
- updater and capability signatures are mandatory, while the initial Windows/macOS installers intentionally do not carry Authenticode or Apple Developer ID/notarization identity and may trigger platform warnings documented in `known-limitations.md`.

First-release acceptance policy:

- `0.2.0` has no prior production release from which to exercise the real updater path, so that single upgrade row is waived once (re-approved by the owner on 2026-08-31 for the 0.2.0 line) and replaced by mandatory clean-install, launch/restart, uninstall, project-preservation, signature, and OS-warning acceptance on all four targets;
- real-machine acceptance order, revised by the owner on 2026-09-01: the Windows x64 clean-install row completed before publication with no blocker (see `2026-09-01-app-v0.2.0-rc.1-windows-x64-acceptance.md`); the owner then explicitly approved publishing this stable release before the remaining rows execute, so macOS arm64, macOS x64, and Linux x64 are built and published here with their acceptance rows tracked as Pending in the owner-approved tracking issue [#34](https://github.com/StoneLL1/llm-wiki-desktop/issues/34); the four-platform gate itself is not waived, and the pending rows must be executed and recorded;
- beginning with `0.2.1`, release acceptance again requires a real installed production-signed predecessor to upgrade to the candidate on Windows x64, macOS arm64, macOS x64, and Linux x64.

See `known-limitations.md` in the release assets before installing.
