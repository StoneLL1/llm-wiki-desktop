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
- the owner-approved execution order for the candidate runs real-machine acceptance on Windows x64 and macOS arm64 first, while macOS x64 and Linux x64 are built, published, and tracked as Pending in a tracking issue; the four-platform gate itself is not waived and closes before `0.2.0` is declared stable beyond the candidate;
- beginning with `0.2.1`, release acceptance again requires a real installed production-signed predecessor to upgrade to the candidate on Windows x64, macOS arm64, macOS x64, and Linux x64.

See `known-limitations.md` in the release assets before installing.
