# Changelog

All notable changes to LLM Wiki Desktop are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project's first stable version line starts at 0.2.0.

## [Unreleased]

### Planned

- First stable release `0.2.0`, cut from release candidate
  `app-v0.2.0-rc.1` after four-platform clean-install acceptance.

## History of the 0.1.0 coordinate (never a stable release)

For the record, because the tags will not be reused:

- `app-v0.1.0` — a tag was created once but **no stable 0.1.0 was ever
  published**: its release workflow failed during repository preflight and no
  Release or `latest.json` was produced. The coordinate is retired.
- `app-v0.1.0-rc.1` — prerelease workflow run failed closed (macOS updater
  bundle selection was incomplete); cancelled before publication.
- `app-v0.1.0-rc.2` — a GitHub Prerelease page exists but the build is a
  **failed candidate; do not install it.** It is retained only as evidence
  and carries a warning banner.
- `app-v0.1.0-rc.3` — workflow runs hung awaiting environment approval and
  were cancelled; nothing was published.

Development between these attempts continued on `master` (import pipeline
v2, capability packs with signed catalogs, performance remediation, Graph
focus handling, release-contract tooling). Those changes are not enumerated
per-commit here; `0.2.0` will summarize user-visible behavior at its release.

## [0.2.0-rc.1] — 2026-09-01

First release candidate of the 0.2.0 line, sealed and published as a public
GitHub prerelease from `98cdbc5` by workflow run 33481891948 (Windows x64
setup + updater signature, macOS arm64 and macOS x64 DMGs + updater archives,
flat checksums). The global `latest` channel was not touched. Candidate
acceptance: the Windows x64 clean-install row is complete with no blocker
(`docs/release/2026-09-01-app-v0.2.0-rc.1-windows-x64-acceptance.md`);
macOS arm64 runs next, and macOS x64 / Ubuntu 24.04 x64 are tracked as
Pending in issue #34. See the release notes and known limitations attached
to the prerelease for the acceptance policy and platform-warning disclosure.
