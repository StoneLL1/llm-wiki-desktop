# Desktop release runbook

Stable application releases use `app-vX.Y.Z` tags and display `vX.Y.Z` to users. Desktop downloads and optional engine archives have separate release pages. Official installers embed a complete verified engine catalog so users can install OCR, transcription, browser extraction, and document-conversion engines from within the app. Historical acceptance records remain evidence, not repeated approval steps.

## Prepare a release

1. Update package, Cargo, and Tauri versions together, plus release notes and known limitations. Run `npm run check` and `npm run check:release-config`.
2. Select a completed canonical Desktop release run containing all qualified engine archives and its merged catalog. Set the repository variable `CAPABILITY_SOURCE_RUN_ID`, or pass `capability_source_run_id` when dispatching the workflow; the explicit input takes precedence. The source selected for this v0.2.1 release is `34439099432`.
3. Merge the PR after the three required CI checks pass. Create the stable tag on the intended master commit. A tag push uses the repository source-run variable; a manual dispatch accepts `release_tag` and the optional source-run override. The tag or dispatch is the maintainer's publication decision.
4. Watch all required jobs. Check the desktop and capability release pages and the post-publication manifest probe. Record any platform limitations; hosted checks do not prove a complete interactive upgrade or recovery journey.

`master` requires `Validate (ubuntu-latest)`, `Validate (windows-latest)`, and `Validate (macos-latest)`. Linux runs the full suite; Windows/macOS run native tests except four exhaustive recovery/format/scale sweeps already covered on Linux. PRs and conversation resolution remain required. CI retries a failed Linux job once when a hosted-runner shutdown kills it and the other platforms pass; ordinary test failures remain failures.

## Required release checks

- `preflight` validates the repository, tag, commit, and version progression. It verifies the source run, successful qualification jobs, unexpired artifacts, artifact identities, and unchanged engine inputs for the complete manifest-derived matrix (43 entries for v0.2.1). It validates the source catalog and trust keys, preserves its original provenance, and records the current desktop integration identity.
- `source-check` runs the full `npm run check` on macOS for the exact release commit, including real macOS runtime checks.
- `desktop-build` embeds the non-empty verified catalog, builds the four desktop targets, verifies updater signatures, and performs installation and launch smoke checks on each target.
- `publish-capabilities` re-verifies the signed engine archives and their exact merged catalog, then publishes them in `capabilities-vX.Y.Z` as a prerelease with `latest=false`.
- `publish` waits for source checks, all desktop builds and smoke checks, and capability publication. It assembles `latest.json`, checks the public desktop downloads, verifies the release tag still points at the built commit, and publishes the desktop release.

## Engine reuse limits

Reuse is conditional, not a substitute for rebuilding changed engines. Engine sources, dependencies, packaging inputs, or qualification changes require newly built and qualified archives. The current reuse path requires a source run for the exact application tag with all required artifacts still available. It does not implement unrestricted reuse across versions or restoration from a permanent release channel.

If the selected run is missing required artifacts or they have expired, stop and obtain a suitable new build run. Restoration from published engine assets would need a separately implemented and verified path; changing the source ID does not bypass input or provenance checks. Logs and source/release provenance stay in Actions artifacts with their configured retention periods.

## Signing and publication

- `desktop-release` supplies `TAURI_SIGNING_PRIVATE_KEY` and its password, which may be empty for an unencrypted key. Updater signatures remain mandatory. Windows Authenticode and Apple Developer ID/notarization are not required; platform warnings are disclosed in installation notes.
- The independent `publish-capabilities` job uses the `capability-release` environment. It verifies already signed archives with committed public keys and does not need to read or regenerate the capability signing secret.
- Build and verification jobs have read-only repository access. Only the two publisher jobs receive `contents: write`, each in its corresponding release environment. Release jobs run from `master` or `app-v*` tags; no repeated reviewer approval is required.
- Both publishers create or resume drafts, upload missing or changed draft assets, and compare remote names, sizes, and GitHub SHA-256 digests before publication. Existing tags are verified.
- Public releases are not overwritten or automatically deleted. The post-publication manifest probe retries CDN access and reports a warning if access still fails; a network failure does not delete a valid release.

## Public download layout

The main release has exactly eight assets:

- Four installers: Windows setup EXE, two macOS DMGs, and Linux AppImage.
- Two macOS `.app.tar.gz` archives for automatic updates.
- `latest.json`, containing the updater signatures.
- `CHECKSUMS.sha256` for the seven other desktop assets.

The separate [v0.2.1 capability release](https://github.com/StoneLL1/llm-wiki-desktop/releases/tag/capabilities-v0.2.1) contains optional engine archives, the install catalog, public trust keys, catalog provenance, and checksums. It never takes over the stable `latest` channel. Users normally install engines through the app. Build logs, qualification reports, and standalone updater signature files remain in Actions artifacts.

## Retry a failed run

Use GitHub's **Re-run failed jobs** first. Successful jobs and their artifacts can be retained, and partial draft uploads are resumable. Keep the tag on its original commit.

To start a new run for an unpublished tag, dispatch Desktop release with `release_tag` and a valid engine source run. The required checks run again; matching draft uploads can be retained. If the release is already public, the stable-version progression check rejects the same version even though the publisher itself can recognize identical public assets. Ship a higher version for a published code fix; do not move a published tag.

An unexpected draft attachment is reported by name. Inspect it before removing it. A failed desktop publication may leave the independent capability prerelease available; it does not become the desktop updater's latest release.

## Focused maintenance checks

- `npm run check:release-config:local` checks local origin/default-branch setup.
- `npm run check:acceptance` audits historical product evidence and redline declarations.
- `npm run test:updater-signature` checks valid signatures, tampered bytes, and wrong keys.
- `actionlint` validates GitHub Actions syntax.
- Cargo audit runs weekly and on demand; advisory failures remain visible without blocking every PR.

For key loss or rotation, see [release identity and access](release-identity-and-access.md#updater-signing-key-operations). Keep the client trust anchor unless an explicit migration is implemented.
