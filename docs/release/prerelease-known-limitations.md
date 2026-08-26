# Known limitations for 0.1.0-rc.1

- This is a GitHub Prerelease intended for testing, not the first stable release.
- Linux desktop binaries are intentionally deferred.
- Optional OCR, ASR, browser, and media capability runtime packs are not published with this RC; related features may require later stable release assets.
- The Windows NSIS file doubles as the updater payload and carries the Tauri updater signature, but it is not Authenticode-signed. SmartScreen or an unknown-publisher warning is expected; verify `CHECKSUMS.sha256` and GitHub attestation before continuing.
- The macOS `.app.tar.gz` updater archives carry Tauri updater signatures. The DMG installers are checksummed and attested, but are neither updater-signed nor signed with Apple Developer ID/notarized. Gatekeeper may require a deliberate manual override after checksum and attestation verification.
- This RC does not publish the stable updater manifest and cannot be offered through the production stable update endpoint.
- Compatible Markdown vaults retain their existing layout. App-owned compatibility guidance remains under `.app/compat/`.
- Natural-language answers require Chat or an explicit workflow and a configured Agent/BYOK provider; local navigation search remains keyword/filter based.
