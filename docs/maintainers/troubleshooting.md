# Maintainer Troubleshooting Guide

This guide distills the project's accumulated working notes into durable,
machine-independent lessons. Each entry follows one shape: a **symptom
class**, the **root-cause pattern** behind it, and the **avoidance rule**
that keeps it from coming back. It is organized by theme so you can skim
the section relevant to whatever just broke, instead of replaying history.

The original working notes (`progress.txt`, `gotchas.txt` at the repo root
and under `SPEC/`) are maintained locally by the maintainers and are no
longer tracked in the repository. This document is the public extract.

## How to use this guide

- Before debugging a failure that only appears on one OS, read
  [Cross-platform filesystem behavior](#cross-platform-filesystem-behavior)
  first — most single-platform failures in this project's history were
  assumptions about paths, encodings, or iteration order.
- Before adding a new test, read
  [Running the test suites](#running-the-test-suites) — several failure
  classes only appear in full-suite runs or on specific runners.
- Rules stated as **must** reflect project policy, not preference.

## Cross-platform filesystem behavior

The application manipulates user files across Windows, macOS, and Linux.
Most of its historical defects came from one platform's filesystem
semantics being assumed universal.

### Line endings and content digests

- **Record digests over canonical bytes, not working-tree bytes.** With
  `core.autocrlf=true` a Windows checkout materializes CRLF files while
  Git stores LF blobs. A digest computed from the working tree passes on
  the author's machine and fails on every Linux CI runner. When a lock or
  manifest pins a digest of a text file, normalize line endings before
  hashing on both the recording side and the verifying side.
- **When one OS disagrees about file content, compare bytes before
  theories.** Fetching the stored blob (`git cat-file blob`) and hashing
  it against the working-tree file immediately distinguishes
  line-ending drift from real content drift.

### Path canonicalization

- **Compare paths through `realpath` on both sides of an assertion.**
  macOS temp directories live under `/var/folders` but canonicalize to
  `/private/var/folders`; Windows drive-letter casing and 8.3 short
  names produce equivalent spellings. Comparing raw path strings fails
  on platforms that spell the same location differently.
- **Canonical Windows paths can carry a `\\?\` device prefix** that
  ordinary stored paths lack. Any comparison or join involving
  canonicalized Windows paths must account for the prefix rather than
  assume all absolute paths share one grammar.
- **A canonical Windows path is not a portable locator.** Storing the
  canonicalized form of a path inside project or capability state makes
  that state non-portable across machines and platforms; persist the
  logical form and canonicalize at use time.
- **Path deduplication must not unconditionally lower-case.** Case
  sensitivity differs per filesystem; collapsing case merges distinct
  files on case-sensitive volumes. Compare case-insensitively only when
  the target volume is known to be case-insensitive.
- **Containment checks must not rely on lexical `starts_with`.** A path
  like `/a/wiki-other/file` is lexically inside `/a/wiki` if you prefix-
  match on strings. Always compare parsed components, and resolve
  symlinks and reparse points first.
- **Path identity checks that stop at ASCII/lowercase or exact-prefix
  comparison are not sufficient** for security-relevant path
  comparison; use full canonical comparison at the point of decision.

### Directory iteration and enumeration

- **Never trust `read_dir().next()` to return a file.** Iteration order
  differs per filesystem (name-ordered on NTFS, hash-ordered on common
  Linux filesystems) and subdirectories may interleave with files.
  Filter for the entry you expect (e.g. `find(|p| p.is_file())`).
- **Re-enumerate, don't re-hash, after rules change.** When a rule pass
  can create, delete, or rename files, hashing the original page list
  misses files added or removed mid-pass; walk the tree again after the
  rules have run.

### Case sensitivity

- **Two files differing only in case can coexist on NTFS** (and on
  Linux), even though macOS default volumes reject one of them. Tests
  and dedup logic that assume case-insensitive uniqueness produce
  different results per platform.

### Filenames and encodings

- **APFS refuses to store invalid UTF-8 filenames outright** (`EILSEQ`),
  while Linux byte-string filesystems allow them. A test fixture that
  writes a non-UTF-8 name must treat a write refusal on macOS as the
  expected precondition (the scenario is unreachable there), and any
  other write failure as a real defect.
- **Markdown image references can traverse symlinks and Windows reparse
  points.** Resolving a wiki-relative path must not silently escape the
  project root through a link.

### Windows-specific filesystem behavior

- **Archive and directory renames can fail with `EPERM`** even when the
  operation looks atomic; retry or rename through a staging location
  instead of assuming rename semantics match POSIX.
- **Rapid same-path directory replacement can preserve identity
  metadata** — a replaced directory can reappear with the same revision
  or window identity. Identity keys must include content, not just
  path-and-time.
- **Same-size (or same-length) replacements pass length-only race
  checks.** Race and tamper detection must compare content hashes, not
  lengths.
- **Same-size artifact swaps can preserve coarse filesystem
  timestamps.** Do not use mtime equality as evidence a file was not
  replaced.
- **`mklink /J` fixture creation rejects mixed-separator link paths.**
  Use consistent separators when creating junction fixtures.
- **Creating file symlinks on Windows requires a privilege** (error
  1314 without Developer Mode). Security tests depending on real file
  symlinks must handle the unprivileged case explicitly rather than
  fail.

## Running the test suites

### Cargo test filters and feature profiles

- **Cargo test filters are substrings, not grouped regular
  expressions.** `a_b` matches `x_a_by`; broad filters silently run
  unrelated binaries, including the GUI library test binary on Windows,
  which fails for environment reasons and looks like a product failure.
  Keep filters narrow.
- **Focused Rust tests must use the project's feature profile.** The
  default-feature binary carries the GUI stack; running it on a host
  without a desktop session fails with entry-point errors
  (`STATUS_ENTRYPOINT_NOT_FOUND` / `0xc0000139`) before any test runs.
- **One PDB can fail to link (`LNK1207`) when parallel test binaries
  share a target directory.** A retry or a distinct target directory
  distinguishes flaky linking from a real failure; a first link failure
  need not be persistent.
- **`cargo fmt -- <paths>` reformats the entire crate** if the path
  arguments are wrong; `cargo` must be invoked against
  `src-tauri/Cargo.toml` because the manifest does not live at the
  repository root.

### Timing-sensitive frontend tests

- **Lazy UI regression tests need a full-suite timing budget.** A test
  that passes alone can time out when suite-wide setup steals its
  window; assert against generous deadlines or mark them accordingly.
- **Real-latency tests become flaky when they time testing-library
  polling** instead of the operation being measured. Measure the
  operation, not the framework's polling interval.
- **A Vitest case can time out while its final `waitFor` still owns the
  same deadline.** Each wait consumes a shared budget; budget waits
  explicitly.
- **UI assertions must not match by substring.** Empty-state titles and
  bodies collide with substring queries; duplicate page titles confuse
  Testing Library queries; select by role and exact name.
- **`matchMedia` stubs must implement listeners** for responsive hooks
  to pass — the failure only appears in broad suites where the stub is
  actually exercised.
- **Node smoke scripts named `*.test.mjs` are collected by Vitest** even
  when they use `node:test`; keep smoke scripts out of test-glob
  patterns. Similarly, explicit `exclude`/`ignores` replace default
  discovery rules — adding an `exclude` without re-listing defaults
  widens collection unexpectedly.

### Test discovery and workspace boundaries

- **Vitest collects `.worktrees/**` copies together with the main
  workspace** and produces doubled or conflicting results; exclude
  auxiliary checkouts from test discovery.
- **Test temporary directories must live outside repository scan
  ranges.** Temp fixtures inside the repo get picked up by lint, glob,
  and file-walking gates and fail them spuriously.
- **Ad-hoc or root-level Cargo target directories break the frontend
  lint gate** by putting generated artifacts inside walked paths; keep
  builds in the standard target location or exclude them.
- **Browser-runtime integration tests fail before exercising the
  runner** when their runtime cache is cold; a failure at setup is an
  environment failure, not a runner regression.

## CI and the gate

- **A green run on one OS proves little.** Historically, batches that
  passed on Windows exposed four distinct defects on first contact with
  macOS and Linux runners. The required checks run all three platforms;
  treat any single-platform pass as provisional.
- **The full gate can pass while a product defect ships** — a green
  gate is necessary, not sufficient. Product-level acceptance evidence
  lives with the release checklist, not in CI.
- **A unified check can pass the frontend build while the Rust
  integration suite fails on a host-owned resource** (a file locked by
  another process). Distinguish environmental failures from
  regressions before re-running.
- **Frontend and browser tests fail before execution when `node_modules`
  is partially installed or a native binding (e.g. Tailwind's Oxide) is
  corrupted.** Reinstall dependencies before debugging test logic.
- **Windows process-launch quirks:** `Start-Process -ArgumentList` can
  silently truncate long argument lists; `Measure-Command` can report
  exit code 0 after an npm child fails; Node orchestrators can hit
  `spawn EINVAL` with certain argument shapes. Verify behavior through
  output files or exit-code plumbing, not assumed success.

## Concurrency and state machines

The application is built around long-running, cancellable tasks. Most
of its hard bugs were ordering bugs.

### Stale and racing responses

- **Cursor equality is not a stale-request guard.** Two different
  requests can carry the same cursor; guard with a freshness identity
  (a revision, a request id), not a value that can repeat.
- **A response can return after a newer user mutation.** Apply
  responses only if their identity still matches the current session,
  page, or project scope; otherwise discard.
- **A task can finish — or request confirmation — before its creation
  response resolves.** Never assume creation precedes lifecycle events.
- **Late task snapshots can arrive after terminal events**, and an
  empty snapshot is not always authoritative. Terminal states win, but
  only when the event ordering is actually established.
- **In-flight request deduplication needs a freshness identity**;
  deduping on parameters alone replays stale answers.
- **Session mutation revisions are reconciliation signals, not success
  filters** — dropping older revisions is correct, treating them as
  failures is not.

### Cancellation and recovery

- **Cancellation checked before acquiring the session lock can still
  race a write.** Check cancellation while holding the lock that
  guards the write, at the last responsible moment.
- **Chat/import cancellation must not publish a terminal state before
  cleanup**, or the UI shows a finished task whose resources are still
  held.
- **Recovery must re-enter the coordinator, not merely repair the
  persisted record.** A repaired task file that the running
  coordinator never observes diverges from reality.
- **Recovery is the authority state that must stop mutation access**;
  unreadable or source-empty states are not the same as recovery and
  must not unlock mutations.
- **Crash recovery cannot infer a task's success from side effects
  alone** (e.g. sources consumed); require explicit completion
  evidence.
- **Confirmation validation followed by a second candidate read is
  still a recovery window** — the candidate can change between the two.
- **An atomic-lease temp file left by a crash can block every later
  recovery**; lease acquisition must tolerate and clear stale temps.

### Idempotency and persistence capabilities

- **Persisted task and record identifiers are untrusted path input**
  even when the surrounding JSON schema is trusted; validate them
  against the persistence layout before use.
- **Recovered task IDs and cached persistence paths are
  capabilities.** Granting them the power to delete or overwrite
  project files turns a cache into an attack surface.
- **Tombstone caps silently break duplicate-start idempotency.** Bound
  tombstones explicitly and document the bound.
- **Read-only workflow persistence still needs a two-sided baseline
  guard** — both the expected prior state and the new state — or a
  concurrent writer's output is clobbered.
- **A mutation journal of intended post-write hashes cannot recover a
  partial write**; record what was actually written, or verify before
  commit.
- **A convenience rollback can overwrite an external edit** on an
  affected path; scope rollbacks to paths the workflow itself changed.
- **Scoped rollbacks must not use whole-worktree Git
  clean/restore.** Limit destructive Git operations to the explicit
  path set.

## Project trust and authorization

- **Registered project paths are not trust grants.** Canonical-path
  registration, trust, and write access are separate states that must
  be serialized together on downgrade or revocation.
- **Runtime trust and persisted trust state can diverge across
  concurrent grants or revocations**; make trust transitions
  transactional.
- **An ancestor Git repository is not project-local history.** Walking
  up to the nearest `.git` can cross the project boundary; stop at the
  project root.
- **Read-only Git assessment can still mutate or execute
  project-controlled programs** (hooks, filters, drivers). Sandboxing
  read-only Git operations is part of the read-only contract.
- **A newly openable folder does not prove a missing recent project
  moved there**; treat relocations as user assertions, not findings.
- **Frontend disabled states are not authorization.** Every mutation
  path must be guarded by the backend policy, not the UI state that
  happens to surround it.

## Import and platform connectors

### Evidence rules

- **Login success is not restricted-content evidence.** A visible
  account avatar is not login proof; a login prompt can render beside
  valid public data. Gate extraction on content evidence, not
  authentication UI.
- **Platform bootstrap payloads usually contain the requested post
  *and* recommendations.** Match the target item, not the first item.
- **Parsed API evidence outranks HTML success.** A page that renders
  does not prove the data API returned the target content; rank
  evidence sources explicitly.
- **Platform route registration is not platform extraction.**
  Advertising a connector in the UI without a working extractor
  misleads users.
- **Signed or expiring query keys must be redacted consistently** in
  both browser and backend snapshots.
- **URL evidence is not always a full HTML document**; content
  sniffing must handle fragments and non-standard payloads.
- **Allowlisting a platform CDN host is not blanket navigation
  approval**; classifier hosts are broader than cookie-navigation
  hosts, and short-link login can authenticate on a different
  first-party origin than the content.
- **On Fake-IP/TUN systems, public HTTPS imports can fail before
  HTTP** is ever attempted; DNS interception changes which transport
  fails first.

### Media and transcripts

- **Subtitle candidate URLs are not transcripts.** Fetch, validate the
  timeline, and reject HTML bodies posing as subtitles.
- **DASH audio tracks are ASR inputs, not preservable original
  media.** Keep the provenance distinction in artifacts and history.
- **"ASR available" is not "ASR authorized"**, and playback CDN trust
  must follow exact runtime evidence per item.
- **Missing media transcript must fail closed** rather than import a
  silently degraded item.
- **URL imports can look successful while the original media is
  unavailable**; verify the artifact, not the task exit code.
- **Restricting a downloader's flags does not restrict its sockets.**
  Containment is about the process boundary, not the argument list.

## Capability packs

### Signing and integrity

- **A capability ZIP's own hash cannot live inside its signed
  manifest** — the digest must be distributed alongside, in a layer
  that covers it.
- **Signed versions are immutable at both the archive and manifest
  layers**; mutating a manifest in place breaks verification history.
- **Extraction must restore every signed helper executable**, not just
  the primary entrypoint; a partially restored tree fails at runtime
  with signature errors that look like packaging bugs.
- **Copying only a verified entrypoint breaks its runtime layout.**
  Capability layouts are directory contracts, not single files.
- **Signed inventories must use bytewise path ordering** so that
  verification is independent of filesystem enumeration order.
- **Signing an entrypoint or archive does not authenticate mutable
  interior files**; every redistributed executable must be covered.

### Versioning and freshness

- **Development capability freshness cannot be keyed only by
  version.** Source-managed runner files and reusable native caches
  age differently; refresh must distinguish them.
- **Capability process exit and stdout delivery are separate
  events.** Wait for output streams to close, not just exit codes.
- **A capability wrapper's timeout must exceed the runner's own staged
  timeouts**, or the wrapper kills a runner that would have recovered.
- **Capability runners must not write through fixed intermediate
  directories inside user-visible staging**; concurrent runs collide.
- **Capability runtime trees must not be traversed recursively** —
  unbounded trees make health checks and scans quadratic.
- **An engine can report failure after a helper succeeded** (e.g. ASR
  engine error after the audio extractor exits 0); trust the engine's
  contract, not the pipeline's exit chain.
- **Third-party license obligations live in the *native-library*
  terms, not just the top-level wheel metadata** — verify at the
  redistributed-binary level, not the package level.
- **A dependency's platform matrix can silently shrink** (an
  unconstrained OpenCV dependency dropping Intel macOS); pin and test
  per target.
- **Accelerator selection can silently fall back while exiting
  successfully**; assert the configured route actually engaged.

## Agent and BYOK execution paths

- **Model identity and CLI version are different audit fields.**
  Recording one and inferring the other produces false equivalence in
  audit trails.
- **Route revisions must hash everything that changes behavior** — not
  just CLI version and logical path — or a changed prompt or contract
  reuses a stale cache.
- **`env_clear` is not enough: an Agent subprocess can re-acquire host
  credential directories** through home-directory resolution.
  Explicitly neutralize credential-directory inheritance (e.g.
  `ANTHROPIC_API_KEY`, OAuth/keychain paths) for isolated runs.
- **Bare-process isolation and local OAuth/keychain reuse are mutually
  exclusive.** Choose one per invocation; don't half-isolate.
- **Structured CLI contracts must be parsed at their real final-output
  field**, and structured stderr is not a safe transcript — provider
  diagnostics can interleave.
- **Agent detection contracts regress silently when an invocation
  requires a new flag**; pin the full invocation shape in tests.
- **Long-running structured generation cannot reuse an interactive
  non-streaming completion**; the request shapes differ.
- **Agent lint stream failures must terminate the process tree before
  candidate cleanup**, or orphaned children keep writing into cleaned
  directories.

## Graph rendering

- **Sigma emits `mousemovebody` before incrementing its drag
  counter.** Tests that assume the counter gates events misread the
  first move.
- **Fast layout saves can finish out of order** and persist stale
  coordinates; version or sequence layout writes.
- **Node dragging makes later layout resets appear clipped or oddly
  scaled.** Reset drag state before re-layout.

## Frontend patterns

- **Scrollable agent output needs per-pane pinned state**; one global
  auto-scroll flag fights the user across panes.
- **Optimistic selection can be overwritten by a stale project or
  session response.** Reconcile by identity, not arrival order.
- **A persisted desktop width can hide sidebar labels** at sizes the
  persistence layer considers valid; clamp persisted dimensions against
  the current viewport on load.
- **Markdown preview links and images must not become implicit network
  or file fetches**; sanitize renderers against arbitrary scheme and
  path resolution.
- **An inline absolute menu stays clipped regardless of z-index**;
  relocate it in the DOM, do not fight stacking contexts.
- **Packaged WebView2 command counting cannot assume framework IPC is
  monkey-patchable**; instrument at the boundary that exists in the
  packaged app.

## Habits that prevent recurrence

1. **Reproduce on the failing platform before theorizing.** Most
   cross-platform bugs are one `git cat-file` or one `realpath` away
   from explanation.
2. **Write assertions against canonical forms** — normalized paths,
   LF-normalized digests, role-and-name UI queries.
3. **Give every async response an identity** and reconcile on it.
4. **Treat persisted identifiers and paths as untrusted input.**
5. **Never let a convenience operation widen its own scope** —
   rollbacks, cleanups, and caches stay within the paths their workflow
   owns.
6. **When a rule has an exception per platform, encode the exception
   at the rule, not at each call site.**
