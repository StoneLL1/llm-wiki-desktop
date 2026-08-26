# LLM Wiki Desktop 0.1.0-rc.1

This is a public test release for installation and feedback before the first stable release.

Available desktop builds:

- Windows x64;
- macOS Apple Silicon (arm64);
- macOS Intel (x64).

The updater payloads are signed with the committed Tauri updater trust anchor. The installers and updater payloads are produced from one exact RC tag, exercised by packaged install-and-launch smoke checks, checksummed, inventoried with CycloneDX SBOMs, and covered by GitHub artifact provenance.

This prerelease does not publish Linux desktop binaries, capability runtime packs, or the stable updater manifest. It therefore does not change the stable/latest update channel. See `known-limitations.md` before installing.
