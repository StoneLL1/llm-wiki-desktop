# Desktop release runbook

Stable application releases use `app-vX.Y.Z` tags and display `vX.Y.Z` to users.
Optional engines use a matching `capabilities-vX.Y.Z` supporting release. The
supporting release is marked prerelease and never replaces the desktop latest
channel. Historical acceptance records remain evidence, not repeated approval
steps for every release.

## Normal release

1. Update the package, Cargo and Tauri versions together, plus release notes and known limitations. Run `npm run check` for the change and `npm run check:release-config` for the release configuration.
2. Merge the PR with the three required CI checks green. Create an `app-vX.Y.Z` tag on the intended master commit and push it. The tag push is the maintainer's publication decision; release environments do not ask the same maintainer to approve each stage again.
3. Watch the Desktop release workflow. It validates the tag/version, builds signed capabilities, builds the four desktop targets, assembles the candidate, runs four installation/launch smoke jobs, and verifies/publishes in one final job. It does not repeat the source test suite already owned by CI.
4. Check the release page and the updater manifest probe. Test clean installation and upgrades on the platforms affected by the change; record limitations and failures. Hosted launch smoke does not prove a complete real-machine upgrade or recovery journey.

`master` still requires `Validate (ubuntu-latest)`, `Validate (windows-latest)`, and `Validate (macos-latest)`. Linux runs the full suite; Windows/macOS run native tests except four exhaustive recovery/format/scale sweeps already covered on Linux. It does not require a branch to be updated after every unrelated master commit. PRs and conversation resolution remain required; force-push and branch deletion remain disabled.

## Signing and publication

- `desktop-release` holds `TAURI_SIGNING_PRIVATE_KEY` and its password (which may be empty for an unencrypted key).
- `capability-release` holds `LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX`; repository variable `CAPABILITY_SIGNING_KEY_ID` names its committed public key.
- Both environments allow only `master` and `app-v*` tags. They have no required reviewer or wait timer. Private signing keys remain environment secrets.
- Updater and capability signatures remain mandatory. Windows Authenticode and Apple Developer ID/notarization are not required; platform warnings are disclosed in the release notes.
- Build jobs have read-only repository permissions. Only `publish-stable` can write the two Releases; it also creates the provenance attestation.
- Before publication, the final job verifies the complete catalog/artifact set, updater signatures, manifest, smoke summary and checksums. The signature verifier is a small standalone Cargo package, not a rebuild of the app.
- The publisher creates or resumes a draft, uploads missing/changed draft assets, compares all remote asset names, sizes and GitHub SHA-256 digests, then publishes. It uses existing tags (`--verify-tag`). The final job creates the supporting capability tag at the same commit, checks any existing tag matches, publishes those packs first, then publishes the desktop release.
- A public release is never overwritten or automatically deleted. The post-publication manifest probe retries CDN access and reports a warning if it still fails. It does not download every capability archive again or roll back a valid release because of a network failure.

## Public download layout

The complete verified candidate, SBOMs, qualification reports, signatures, and
provenance remain in Actions artifacts; GitHub also retains build attestations.
`stage-public-release.mjs` selects public assets explicitly:

- Desktop: four installers, two macOS updater archives, `latest.json`, and
  `CHECKSUMS.sha256` — eight files. Updater signatures are embedded in `latest.json`.
- Capabilities: the 43 manifest-listed archives, install catalog, trusted public
  keys, catalog provenance, and checksums. The app retrieves these on demand.

The capability catalog is generated with matching `capabilities-vX.Y.Z` URLs
before signing and embedding. Legacy same-version `app-vX.Y.Z` URLs remain valid.
Do not move or delete existing capability assets used by older installations.

## Retry a failed run

Use GitHub's **Re-run failed jobs** first. Successful matrix jobs and their artifacts can be retained, and a partial GitHub draft upload is resumable. Keep the tag on its original commit.

If scripts need a repair after a late failure, dispatch Desktop release with `release_tag` and `source_run_id`. The source must be a desktop-release run for the exact tag commit with the candidate, updater fixture and four desktop artifacts still available. This skips capability/desktop rebuilding and reruns the four smoke jobs and final publisher; provenance continues to identify the original build run. This recovery path does not reuse artifacts across different app commits. For an ordinary manual build without `source_run_id`, select the release tag as the workflow ref.

A failure leaves the draft and workflow artifacts for diagnosis. An unexpected old draft attachment is reported by name; inspect it before removing it. If the release is already public, ship a higher version for a code fix. Do not move a published tag.

## Optional checks

- `npm run check:release-config:local`: checks local origin/default-branch setup when debugging release coordinates.
- `npm run check:acceptance`: audits historical product evidence and redline declarations when working on those acceptance records.
- `npm run test:updater-signature`: runs the real verifier's valid-signature, tampered-bytes and wrong-key tests.
- `actionlint`: validates GitHub Actions syntax without requiring exact job names or shell text.
- Cargo audit runs weekly and on demand. Advisory failures remain visible without blocking every PR.

For key loss or rotation, use [release-identity-and-access](release-identity-and-access.md#updater-signing-key-operations). Keep the existing client trust anchor unless an explicit migration is implemented.
