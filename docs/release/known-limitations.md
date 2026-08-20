# Known limitations for 0.1.0

- The canonical updater path is NSIS on Windows, DMG plus the signed app updater archive on macOS, and AppImage on Linux.
- Linux desktop integration and library availability still vary by distribution; the canonical release qualification target is Ubuntu 24.04 x64.
- Compatible Markdown vaults retain their existing layout. App-owned compatibility guidance remains under `.app/compat/`.
- Natural-language answers require Chat or an explicit workflow and a configured Agent/BYOK provider; local navigation search remains keyword/filter based.
- A lost updater private key cannot be repaired with an unsigned downgrade. Affected clients require the signed bridge/hotfix or manual reinstall procedure in the release runbook.
