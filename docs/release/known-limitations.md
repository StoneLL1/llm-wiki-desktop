# Known limitations for 0.1.0

- The canonical updater path is NSIS on Windows, DMG plus the signed app updater archive on macOS, and AppImage on Linux.
- Windows installers are not Authenticode-signed for 0.1.0. SmartScreen or an unknown-publisher warning is expected; verify the exact-tag checksum and GitHub attestation before deliberately continuing.
- macOS installers are not signed with Apple Developer ID or notarized for 0.1.0. Gatekeeper may block first launch and require a deliberate manual override after the exact-tag checksum and GitHub attestation are verified.
- Linux desktop integration and library availability still vary by distribution; the canonical release qualification target is Ubuntu 24.04 x64.
- `0.1.0` is the bootstrap release, so it cannot prove an upgrade from an earlier production version. Its one-time exception covers only that missing predecessor; four-platform clean installation and all cryptographic, restart, uninstall, preservation, and platform-warning checks remain required. Real old-to-new upgrade qualification becomes mandatory with `0.1.1`.
- Compatible Markdown vaults retain their existing layout. App-owned compatibility guidance remains under `.app/compat/`.
- Natural-language answers require Chat or an explicit workflow and a configured Agent/BYOK provider; local navigation search remains keyword/filter based.
- A lost updater private key cannot be repaired with an unsigned downgrade. Affected clients require the signed bridge/hotfix or manual reinstall procedure in the release runbook.
