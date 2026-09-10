# Desktop release runbook

Stable application releases use `app-vX.Y.Z` tags and display `vX.Y.Z` to users.
Optional engine packs are not distributed with releases; the app treats an empty
capability catalog as a normal state and core knowledge-base features do not
require the packs. Historical acceptance records remain evidence, not repeated
approval steps for every release.

## Normal release

1. Update the package, Cargo and Tauri versions together, plus release notes and known limitations. Run `npm run check` for the change and `npm run check:release-config` for the release configuration.
2. Merge the PR with the three required CI checks green. Create an `app-vX.Y.Z` tag on the intended master commit and push it. The tag push is the maintainer's publication decision; release environments do not ask the same maintainer to approve each stage again.
3. Watch the Desktop release workflow. `preflight` validates the tag, repository, exact commit, and version progression against `releases/latest`; `desktop-build` builds and updater-signs the four targets and verifies each signature; `publish` assembles `latest.json`, checks the public bundle, and publishes in one final job. It does not repeat the source test suite already owned by CI.
4. Check the release page and the updater manifest probe. Test clean installation and upgrades on the platforms affected by the change; record limitations and failures. Hosted checks do not prove a complete real-machine upgrade or recovery journey.

`master` still requires `Validate (ubuntu-latest)`, `Validate (windows-latest)`, and `Validate (macos-latest)`. Linux runs the full suite; Windows/macOS run native tests except four exhaustive recovery/format/scale sweeps already covered on Linux. It does not require a branch to be updated after every unrelated master commit. PRs and conversation resolution remain required; force-push and branch deletion remain disabled. CI retries the failed Linux job once automatically when a GitHub-hosted runner shutdown signal kills it and every other platform passed; real test failures are never retried automatically.

## Signing and publication

- `desktop-release` holds `TAURI_SIGNING_PRIVATE_KEY` and its password (which may be empty for an unencrypted key). It is the only release environment in use; the retired `capability-release` environment is kept only as history.
- The environment allows only `master` and `app-v*` tags. It has no required reviewer or wait timer. Private signing keys remain environment secrets.
- Updater signatures remain mandatory. Windows Authenticode and Apple Developer ID/notarization are not required; platform warnings are disclosed in the release notes.
- Build jobs have read-only repository permissions. Only `publish` receives `contents: write`, and it is protected by the `desktop-release` environment.
- Each build job verifies its own updater artifact with the standalone Cargo verifier before uploading its fragment. The final job regenerates and validates `latest.json` from the four descriptors, verifies the tag still points at the built commit, then publishes. The signature verifier is a small standalone Cargo package, not a rebuild of the app.
- The publisher creates or resumes a draft, uploads missing/changed draft assets, compares all remote asset names, sizes and GitHub SHA-256 digests, then publishes. It uses existing tags (`--verify-tag`).
- A public release is never overwritten or automatically deleted. The post-publication manifest probe retries CDN access and reports a warning if it still fails; it does not roll back a valid release because of a network failure.

## Public download layout

The verified per-platform candidates remain in Actions artifacts. The public
release lists exactly the desktop downloads:

- Four installers (Windows setup EXE, two macOS DMGs, Linux AppImage), the two
  macOS `.app.tar.gz` updater archives, one companion `.sig` per platform, and
  `latest.json` — eleven files. `CHECKSUMS.sha256` is added after verification.
- Updater signatures are embedded in `latest.json`; no other artifacts are
  published.

## Retry a failed run

Use GitHub's **Re-run failed jobs** first. Successful matrix jobs and their artifacts can be retained, and a partial GitHub draft upload is resumable. Keep the tag on its original commit.

To rebuild or complete a publication for an existing tag, dispatch Desktop release with `release_tag` set to that stable tag pointing at the current release commit. The run repeats the full pipeline against the same commit; the publisher resumes the existing draft or verifies the already-published assets instead of duplicating them.

A failure leaves the draft and workflow artifacts for diagnosis. An unexpected old draft attachment is reported by name; inspect it before removing it. If the release is already public, ship a higher version for a code fix. Do not move a published tag.

## Optional checks

- `npm run check:release-config:local`: checks local origin/default-branch setup when debugging release coordinates.
- `npm run check:acceptance`: audits historical product evidence and redline declarations when working on those acceptance records.
- `npm run test:updater-signature`: runs the real verifier's valid-signature, tampered-bytes and wrong-key tests.
- `actionlint`: validates GitHub Actions syntax without requiring exact job names or shell text.
- Cargo audit runs weekly and on demand. Advisory failures remain visible without blocking every PR.

For key loss or rotation, use [release-identity-and-access](release-identity-and-access.md#updater-signing-key-operations). Keep the existing client trust anchor unless an explicit migration is implemented.
