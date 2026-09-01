# Known limitations for 0.2.0

- The canonical updater path is NSIS on Windows, DMG plus the signed app updater archive on macOS, and AppImage on Linux.
- Windows installers are not Authenticode-signed for 0.2.0. SmartScreen or an unknown-publisher warning is expected; verify the exact-tag checksum and GitHub attestation before deliberately continuing.
- macOS installers are not signed with Apple Developer ID or notarized for 0.2.0. Gatekeeper may block first launch and require a deliberate manual override after the exact-tag checksum and GitHub attestation are verified.
- Linux desktop integration and library availability still vary by distribution; the canonical release qualification target is Ubuntu 24.04 x64.
- Capability runners are maintainer-signed trusted application components, not hostile-code OS sandboxes. Archive, signature, exact file inventory, target, protocol, route-set, staging containment, cancellation, and atomic rollback checks remain mandatory.
- X import supports public status pages through their official server-rendered page metadata. Login walls, protected/restricted posts, removed posts, and structure changes stop with typed recovery; the app does not call an undocumented X API or bypass access controls.
- Accurate PDF layout and OCR/ASR packs are large because they include offline models and target runtimes. Catalog `compressedBytes`, `installedBytes`, and `modelBytes` are the only release size authority; source manifests and documentation do not promise a fixed download size.
- Source and release-contract evidence is not packaged-host evidence. The public beta remains No-Go until the same-candidate real-machine installation/import acceptance completes: Windows x64 and macOS arm64 first, with macOS x64 and Ubuntu 24.04 x64 tracked as Pending in the owner-approved tracking issue.
- `0.2.0` is the bootstrap release, so it cannot prove an upgrade from an earlier production version. Its one-time exception covers only that missing predecessor; four-platform clean installation and all cryptographic, restart, uninstall, preservation, and platform-warning checks remain required. Real old-to-new upgrade qualification becomes mandatory with `0.2.1`.
- Compatible Markdown vaults retain their existing layout. App-owned compatibility guidance remains under `.app/compat/`.
- Natural-language answers require Chat or an explicit workflow and a configured Agent/BYOK provider; local navigation search remains keyword/filter based.
- A lost updater private key cannot be repaired with an unsigned downgrade. Affected clients require the signed bridge/hotfix or manual reinstall procedure in the release runbook.
