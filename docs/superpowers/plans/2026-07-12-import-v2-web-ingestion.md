# Import V2 Web Ingestion Implementation Plan

> Historical implementation plan. Connector and security research remains useful, but current login, media, Source commit and independent compile behavior is defined only by [`../specs/2026-07-24-import-source-media-flow-design.md`](../specs/2026-07-24-import-source-media-flow-design.md).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import one exact user-supplied URL into traceable Markdown through SSRF-safe domain routing, deterministic article/video connectors, dedicated login sessions, and a browser fallback, including WeChat, Zhihu, Bilibili, then phase-two Xiaohongshu and X.

**Architecture:** Build on Import Core HEAD `3bd282c` and the capability-pack runtime delivered by File Ingestion. Rust owns URL normalization, redirect authorization, DNS/IP policy, domain routing, credentials, rate limits, retries, and artifact validation; JavaScript browser packs use Mozilla Readability, DOMPurify, Turndown, and Playwright but can only write staging. Platform connectors are independently authored from public behavior and frozen sanitized fixtures, never copied from GPL/AGPL crawlers.

**Tech Stack:** Tauri v2/Rust, reqwest/rustls, `url`, OS keyring, Import Core `ImportEngine`, JSON-RPC stdio packs, Mozilla Readability, DOMPurify, Turndown, Playwright Chromium, optional restricted yt-dlp Bilibili fallback, Vitest, Rust tests.

## Global Constraints

- Prerequisites: Import Core final HEAD `3bd282c` and File Ingestion Tasks 1–2 (typed capability packs) are integrated and green.
- This package implements backend contracts and capability runners only; no Import page visual work.
- Exact URL only: no recursive crawl, site mirror, comment import, mirror search, or automatic third-party search.
- Every redirect repeats scheme/host/DNS/IP validation. `file://`, non-HTTP(S), executable downloads, invalid TLS, localhost, link-local, private, multicast, and reserved addresses are blocked by default.
- A single private-network exception is scoped to the displayed final target and current item; a redirect to another private target requires another explicit authorization.
- Domain-specific connector is selected before generic extraction. Generic order is HTTP -> Readability -> dedicated Playwright browser.
- Cookies, Authorization headers, passwords, xsec tokens, URL fragments, platform signatures, and tracking parameters never enter project JSON, Markdown, history, logs, task summaries, or exports.
- Authentication uses a dedicated connector profile and OS credential storage; never read a user's normal browser profile or `cookies-from-browser`.
- Connector/pack processes read only the current task bundle and write only staging. All candidates pass Core Quality Gate, preview, explicit confirmation, and crash-durable commit.
- Same route retries at most twice only when idempotent and safe. Login, captcha, cloud/BYOK, paid access, and commit are never automatically retried.
- Per-domain default concurrency is 2; sensitive connectors (WeChat, Xiaohongshu, X authentication) use 1.
- First-stage cutover requires generic web, WeChat, Zhihu, and Bilibili. Xiaohongshu and X tasks are phase-two gates and must not weaken first-stage safety.
- Run `npm run check` after every completed task; fix and rerun from the beginning on failure.
- Open-source evaluation details are recorded in `docs/superpowers/plans/2026-07-12-import-v2-open-source-research.md`.

## Planned File Structure

- `src-tauri/src/models/import_v2_web.rs`: normalized URL, redirect, auth, connector, web metadata, and recovery DTOs.
- `src-tauri/src/services/import_v2/url_policy.rs`: canonicalization, DNS/IP validation, redirect authorization, public-only persistence.
- `src-tauri/src/services/import_v2/domain_router.rs`: exact host matching and route plans.
- `src-tauri/src/services/import_v2/web_fetch.rs`: bounded HTTP fetch, content checks, retry/rate policy.
- `src-tauri/src/services/import_v2/connector_session.rs`: keyring-backed dedicated auth session references.
- `src-tauri/src/services/import_v2/connectors/*.rs`: WeChat, Zhihu, Bilibili, Xiaohongshu, X first-party orchestration.
- `src-tauri/src/commands/import_v2_web_commands.rs`: thin add URL, authorize target, and login-session commands.
- `src/types/importV2Web.ts`: TypeScript contract mirror; no UI components.
- `capabilities/browser-runtime-lite/`: Readability + sanitization + Turndown runner.
- `capabilities/browser-runtime/`: Playwright browser runner.
- `capabilities/media-metadata/`: optional reviewed yt-dlp fallback with a restricted argument surface.
- `tests/fixtures/import-v2/web/`: sanitized frozen responses and explicit expected-output manifests.

## Open-Source Route Decision

| Component | License / maturity | Role and size decision | Fallback |
| --- | --- | --- | --- |
| Mozilla Readability | Apache-2.0, Firefox Reader View library | small primary generic article extractor; scripts/resources disabled | platform connector or Playwright |
| DOMPurify + Turndown | Apache-2.0/MPL-2.0 + MIT, mature | small sanitizer and GFM conversion layer | preserve sanitized snapshot and fail quality; never bypass sanitizer |
| Playwright | Apache-2.0, Microsoft-maintained | large on-demand browser pack with pinned Chromium | static HTTP route first |
| yt-dlp | Unlicense core with permissive bundled notices, highly active but frequent security churn | optional isolated Bilibili metadata/subtitle fallback only; exact binary sizes recorded per release | first-party Bilibili connector and Playwright |

---

### Task 1: Freeze Web DTOs and Connector Fixtures

**Files:**
- Create: `src-tauri/src/models/import_v2_web.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src/types/importV2Web.ts`
- Create: `src/types/importV2Web.test.ts`
- Create: `tests/fixtures/import-v2/web/manifest.json`
- Create: `src-tauri/tests/import_v2_web_contracts.rs`

**Interfaces:**
- Produces: `NormalizedWebUrl`, `WebRouteKind`, `RedirectDecision`, `WebContentKind`, `WebMetadata`, `WebAuthState`, `WebRecoveryAction`, and `AddImportUrlV2Request`.
- Extends Core `ImportIssue` only through stable action codes; keeps current fields and camelCase serialization.

- [ ] **Step 1: Write failing wire-contract tests**

```rust
#[test]
fn normalized_url_persists_only_public_components() {
    let value = serde_json::to_value(NormalizedWebUrl {
        public_url: "https://example.com/article?id=7".into(),
        host: "example.com".into(),
        scheme: "https".into(),
    }).unwrap();
    let text = value.to_string().to_ascii_lowercase();
    assert!(!text.contains("token"));
    assert!(!text.contains("fragment"));
}
```

TypeScript asserts exact route/action unions for generic, WeChat, Zhihu, Bilibili, Xiaohongshu, and X.

- [ ] **Step 2: Run focused tests and verify RED**

Expected: web model modules do not exist.

- [ ] **Step 3: Implement DTOs and fixture manifest**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebRouteKind { GenericHttp, GenericBrowser, Wechat, Zhihu, Bilibili, Xiaohongshu, X }
```

Fixture manifest records public URL, route, status/body fixture, expected title/author/date/body sentinels/image count/content kind/warning or error, and whether a browser/login transition is expected.

- [ ] **Step 4: Verify contracts and run `npm run check`**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models/import_v2_web.rs src-tauri/src/models/mod.rs src/types/importV2Web.ts src/types/importV2Web.test.ts tests/fixtures/import-v2/web/manifest.json src-tauri/tests/import_v2_web_contracts.rs
git commit -m "test(import): freeze web ingestion contracts"
```

### Task 2: Implement URL Canonicalization and SSRF Policy

**Files:**
- Create: `src-tauri/src/services/import_v2/url_policy.rs`
- Modify: `src-tauri/src/services/import_v2/mod.rs`
- Create: `src-tauri/tests/import_v2_url_policy.rs`

**Interfaces:**
- Produces: `UrlPolicy::normalize_for_session(&str) -> Result<SessionWebTarget, BackendError>`, `validate_resolved_target`, `validate_redirect`, and `public_persistence_url`.
- `SessionWebTarget` may hold secrets in memory; only `NormalizedWebUrl` can be serialized by session/history/source code.

- [ ] **Step 1: Write RED tests for normalization and address policy**

Cover mixed-case hosts, IDNA, default ports, fragments, UTM parameters, signed parameters, userinfo, IPv4 decimal/octal/hex forms, IPv6, DNS rebinding, CNAME chains, redirects, localhost/private/link-local/reserved/multicast, `file://`, redirect loops, and invalid TLS.

- [ ] **Step 2: Run focused tests and confirm missing policy**

- [ ] **Step 3: Implement strict parsing and public-only serialization**

```rust
pub struct PrivateTargetGrant {
    pub item_id: String,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub resolved_ips: Vec<IpAddr>,
    pub expires_at: DateTime<Utc>,
}
```

Resolve immediately before connection and connect only to validated resolved addresses. Host headers/SNI use the validated host. A grant is single-item, single-origin, short-lived, and never serialized to project files.

- [ ] **Step 4: Assert secret redaction through Core session/history/task paths**

Seed URLs with credentials, access tokens, xsec tokens, signatures, and fragments, then scan every persisted file and captured log for those values.

- [ ] **Step 5: Run full check and commit**

```bash
git add src-tauri/src/services/import_v2/url_policy.rs src-tauri/src/services/import_v2/mod.rs src-tauri/tests/import_v2_url_policy.rs
git commit -m "feat(import): enforce URL and SSRF policy"
```

### Task 3: Add Bounded HTTP Fetch, Rate Limits, and Redirect Ledger

**Files:**
- Create: `src-tauri/src/services/import_v2/web_fetch.rs`
- Create: `src-tauri/src/services/import_v2/domain_limiter.rs`
- Modify: `src-tauri/src/services/import_v2/orchestrator.rs`
- Create: `src-tauri/tests/import_v2_web_fetch.rs`

**Interfaces:**
- `WebFetchService::fetch(target, policy, on_progress, is_cancelled) -> Result<WebFetchArtifact, BackendError>`.
- `WebFetchArtifact` stores sanitized headers, bounded response bytes in staging, final public URL, content type, timing, and redirect ledger; never request headers/cookies.

- [ ] **Step 1: Write RED tests using a local controlled server**

Cover bounded streaming, missing/lying content length, decompression ratio, slow body, cancellation, invalid MIME, executable response, cross-origin redirect, public-to-private redirect, two retryable 503 responses, challenge-page 200, and per-domain concurrency.

- [ ] **Step 2: Implement streaming reqwest client with rustls**

```rust
pub struct WebFetchPolicy {
    pub max_response_bytes: u64,
    pub max_redirects: u8,
    pub max_attempts_per_route: u8,
    pub connect_timeout_ms: u64,
    pub total_timeout_ms: u64,
}
```

Disable automatic redirects and validate each Location manually. Never ignore TLS errors.

- [ ] **Step 3: Emit structured attempt records and progress**

Map route retries to Core `AttemptRecord`; do not write raw URLs containing sensitive query components.

- [ ] **Step 4: Verify cancellation/process cleanup and full check**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/import_v2/web_fetch.rs src-tauri/src/services/import_v2/domain_limiter.rs src-tauri/src/services/import_v2/orchestrator.rs src-tauri/tests/import_v2_web_fetch.rs
git commit -m "feat(import): fetch web sources with bounded policy"
```

### Task 4: Build Generic Readability/Sanitization/Markdown Pack

**Files:**
- Create: `capabilities/browser-runtime-lite/manifest.json`
- Create: `capabilities/browser-runtime-lite/package-lock.json`
- Create: `capabilities/browser-runtime-lite/runner/`
- Create: `src-tauri/src/services/import_v2/generic_web_engine.rs`
- Create: `src-tauri/tests/import_v2_generic_web.rs`

**Interfaces:**
- Implements `ImportEngine` route `web.generic.readability`.
- Input is the already-fetched staged HTML plus final public URL; the pack has no network permission.
- Output is source snapshot, sanitized article Markdown, localized images requested through Rust, and metadata.

- [ ] **Step 1: Write frozen-HTML RED tests**

Cover article/news/blog/docs, JSON-LD metadata, relative URLs, GFM tables, figures/captions, code blocks, lazy images, script/event attributes, tracking pixels, `javascript:` links, prompt-injection text, and empty/navigation-only pages.

- [ ] **Step 2: Build a locked Node pack**

Pin `@mozilla/readability`, `jsdom`, `dompurify`, and `turndown`; disable jsdom scripts/resources. Generate SBOM/licenses and actual package sizes in CI. No runtime npm command is allowed.

- [ ] **Step 3: Implement extraction pipeline**

```text
frozen HTML -> jsdom(no scripts/resources) -> Readability -> DOMPurify -> Turndown(GFM rules) -> artifact manifest
```

The runner returns remote image requests as structured URLs; Rust revalidates and downloads each image through `UrlPolicy`/`WebFetchService`.

- [ ] **Step 4: Verify Quality Gate, containment, and full check**

- [ ] **Step 5: Commit**

```bash
git add capabilities/browser-runtime-lite src-tauri/src/services/import_v2/generic_web_engine.rs src-tauri/tests/import_v2_generic_web.rs
git commit -m "feat(import): extract generic articles with Readability"
```

### Task 5: Add Domain Router and Typed Route Switching

**Files:**
- Create: `src-tauri/src/services/import_v2/domain_router.rs`
- Modify: `src-tauri/src/services/import_v2/orchestrator.rs`
- Create: `src-tauri/tests/import_v2_domain_router.rs`

**Interfaces:**
- `DomainRouter::plan(&NormalizedWebUrl, &ConnectorAvailability) -> WebRoutePlan`.
- Exact/suffix host rules are canonicalized and boundary-safe: `evilweixin.qq.com.example` must not match WeChat.

- [ ] **Step 1: Write route precedence RED tests**

Assert platform connector -> generic static -> generic browser ordering, two safe retries per route, sensitive-domain single concurrency, and no mirror/search fallback.

- [ ] **Step 2: Implement explicit host matcher and route plan**

```rust
pub struct WebRoutePlan { pub primary: WebRouteKind, pub fallbacks: Vec<WebRouteKind>, pub concurrency_key: String }
```

- [ ] **Step 3: Record each switch in `AttemptRecord`**

Challenge page, empty body, structural drift, authentication required, and captcha must have separate stable codes.

- [ ] **Step 4: Run focused and full checks**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/import_v2/domain_router.rs src-tauri/src/services/import_v2/orchestrator.rs src-tauri/tests/import_v2_domain_router.rs
git commit -m "feat(import): route exact URLs by domain"
```

### Task 6: Add Dedicated Playwright Browser and Login Sessions

**Files:**
- Create: `capabilities/browser-runtime/manifest.json`
- Create: `capabilities/browser-runtime/runner/`
- Create: `src-tauri/src/services/import_v2/connector_session.rs`
- Create: `src-tauri/src/commands/import_v2_web_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/tests/import_v2_browser_sessions.rs`

**Interfaces:**
- `ConnectorSessionService::create(platform) -> ConnectorSessionRef`, `resume`, `revoke`.
- Commands: `add_import_url_v2`, `authorize_import_private_target_v2`, `begin_import_login_v2`, `revoke_import_login_v2`.
- Only opaque keyring/profile references are persisted.

- [ ] **Step 1: Write RED tests for browser isolation**

Assert dedicated profile creation, denial of daily browser profile paths, request allowlist, download denial, popup/new-origin policy, captcha -> `waiting_login`, cancellation/timeout process-tree kill, logout/revoke, and no cookie/token persistence in project files.

- [ ] **Step 2: Build pinned Playwright Chromium pack**

Record browser revision, archive/install sizes, supported triples, notices, and hashes. Disable extensions, background networking, password storage, remote debugging exposure, and arbitrary downloads.

- [ ] **Step 3: Implement interactive handoff and resume**

The connector owns authenticated fetches and returns sanitized snapshots. Import Core remains paused until an explicit resume signal; it never receives cookie values.

- [ ] **Step 4: Verify command thinness, registration, and full check**

- [ ] **Step 5: Commit**

```bash
git add capabilities/browser-runtime src-tauri/src/services/import_v2/connector_session.rs src-tauri/src/commands/import_v2_web_commands.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/import_v2_browser_sessions.rs
git commit -m "feat(import): add isolated browser connector sessions"
```

### Task 7: Implement WeChat Connector

**Files:**
- Create: `src-tauri/src/services/import_v2/connectors/mod.rs`
- Create: `src-tauri/src/services/import_v2/connectors/wechat.rs`
- Create: `tests/fixtures/import-v2/web/wechat/`
- Create: `src-tauri/tests/import_v2_wechat.rs`

**Interfaces:**
- Route `web.wechat.article`; public metadata includes title, author, publish time, public URL, fetch time, images.
- Challenge/verification/empty-body detection occurs before success.

- [ ] **Step 1: Write frozen-response RED tests**

Cover valid article, HTTP 200 verification page, challenge page, empty body, removed article, malformed metadata, lazy images, signed image URLs, redirect, and login-required flow.

- [ ] **Step 2: Implement HTTP parser without copying crawler code**

Use independently authored selectors/structured-data rules derived from project-owned fixtures. Normalize the public URL before persistence and keep signed image parameters only in the in-memory fetch request.

- [ ] **Step 3: Add one configured proxy retry and browser fallback**

Proxy comes from application/connector settings and secrets; no hardcoded proxy. Retry only once, then browser/login wait. Never search a third-party mirror.

- [ ] **Step 4: Run fixtures, online manual smoke checklist, and full check**

Automated release uses frozen fixtures; `docs/qa/import-v2-web-ingestion.md` records date/account/environment for manual smoke without credentials.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/import_v2/connectors tests/fixtures/import-v2/web/wechat src-tauri/tests/import_v2_wechat.rs
git commit -m "feat(import): add WeChat article connector"
```

### Task 8: Implement Zhihu Connector

**Files:**
- Create: `src-tauri/src/services/import_v2/connectors/zhihu.rs`
- Create: `tests/fixtures/import-v2/web/zhihu/`
- Create: `src-tauri/tests/import_v2_zhihu.rs`

**Interfaces:**
- Routes article/question-answer/publication pages into one deterministic article contract; comments are excluded.

- [ ] **Step 1: Write RED fixtures for supported page kinds and auth/drift**

Assert title, author, date, accepted/body content order, code, formulas, images, collapsed content, login wall, challenge, deleted content, and structural drift.

- [ ] **Step 2: Implement public structured extraction and sanitization**

Reuse common sanitizer/Markdown pack; connector supplies only the exact content root and platform metadata.

- [ ] **Step 3: Add browser fallback and rate policy**

No comments, recommendations, related questions, or recursive links are added to the import session.

- [ ] **Step 4: Run fixture/manual smoke/full checks and commit**

```bash
git add src-tauri/src/services/import_v2/connectors/zhihu.rs tests/fixtures/import-v2/web/zhihu src-tauri/tests/import_v2_zhihu.rs
git commit -m "feat(import): add Zhihu content connector"
```

### Task 9: Implement Bilibili Video Connector

**Files:**
- Create: `src-tauri/src/services/import_v2/connectors/bilibili.rs`
- Create: `capabilities/media-metadata/manifest.json`
- Create: `capabilities/media-metadata/runner/`
- Create: `tests/fixtures/import-v2/web/bilibili/`
- Create: `src-tauri/tests/import_v2_bilibili.rs`

**Interfaces:**
- Produces cover, title, author, publish date, description, chapters, subtitle/transcript artifacts, and public source URL. Original video/audio is temporary only.
- Subtitle priority matches File Ingestion media contract.

- [ ] **Step 1: Write RED tests for metadata/subtitle variants**

Cover human subtitle, automatic subtitle, no subtitle, multi-part video, unavailable/private/login-required, rate limit, deleted video, and temporary media cleanup.

- [ ] **Step 2: Implement first-party page/structured-data route**

Parse public metadata and subtitle descriptors through Rust fetch policy; every subtitle URL is independently revalidated.

- [ ] **Step 3: Add restricted yt-dlp fallback**

Expose only a fixed typed invocation equivalent to metadata/subtitle dump. Reject `--exec`, external downloader, netrc, browser cookies, plugins, arbitrary templates, post-processors, and caller-provided flags. Pin a release, verify official checksums/signatures, and inventory bundled licenses/CVEs.

- [ ] **Step 4: Integrate optional whisper.cpp route**

Only after user-authorized media processing and when subtitles are absent; media is deleted after task success/failure/restart.

- [ ] **Step 5: Run fixture/manual smoke/full checks and commit**

```bash
git add src-tauri/src/services/import_v2/connectors/bilibili.rs capabilities/media-metadata tests/fixtures/import-v2/web/bilibili src-tauri/tests/import_v2_bilibili.rs
git commit -m "feat(import): add Bilibili video connector"
```

### Task 10: Implement Phase-Two Xiaohongshu Connector

**Files:**
- Create: `src-tauri/src/services/import_v2/connectors/xiaohongshu.rs`
- Create: `tests/fixtures/import-v2/web/xiaohongshu/`
- Create: `src-tauri/tests/import_v2_xiaohongshu.rs`

**Interfaces:**
- Route is disabled until first-stage Web release gates pass.
- `xsec_token` and equivalent signatures exist only inside `SessionWebTarget`/connector memory and encrypted session storage; public persistence strips them.

- [ ] **Step 1: Write RED tests for signed URL secrecy and note/video content**

Cover text/image note, video note, expired token, login, captcha, challenge, deleted/private note, image order, no comments, and full persisted-tree secret scan.

- [ ] **Step 2: Implement dedicated browser-session connector**

No generic HTTP attempt is made with stripped signatures if that would change the target. Authentication/captcha waits for the user; no bypass behavior.

- [ ] **Step 3: Verify rate limit, revocation, staging, and full check**

- [ ] **Step 4: Run manual smoke gate and commit**

```bash
git add src-tauri/src/services/import_v2/connectors/xiaohongshu.rs tests/fixtures/import-v2/web/xiaohongshu src-tauri/tests/import_v2_xiaohongshu.rs
git commit -m "feat(import): add gated Xiaohongshu connector"
```

### Task 11: Implement Phase-Two X Connector

**Files:**
- Create: `src-tauri/src/services/import_v2/connectors/x.rs`
- Create: `tests/fixtures/import-v2/web/x/`
- Create: `src-tauri/tests/import_v2_x.rs`

**Interfaces:**
- Imports one exact post/thread target; quoted post and media needed to understand that target may be included, but replies/comments/search results are excluded.

- [ ] **Step 1: Write RED fixtures for post/thread/media/auth/drift**

Cover public post, author/date/text, image/video metadata, quoted post, short thread by same author, login wall, rate limit, deleted/private post, and unrelated replies exclusion.

- [ ] **Step 2: Implement dedicated Playwright connector**

Use the isolated profile and domain limiter. Persist only public canonical URLs and sanitized content; never persist bearer/cookie/session data.

- [ ] **Step 3: Verify revocation, cancellation, no comment crawl, and full check**

- [ ] **Step 4: Manual smoke gate and commit**

```bash
git add src-tauri/src/services/import_v2/connectors/x.rs tests/fixtures/import-v2/web/x src-tauri/tests/import_v2_x.rs
git commit -m "feat(import): add gated X connector"
```

### Task 12: Integrate Errors, Recovery Actions, and Web Release Gates

**Files:**
- Modify: `src-tauri/src/services/import_v2/orchestrator.rs`
- Modify: `src-tauri/src/models/import_v2.rs`
- Modify: `src/types/importV2.ts`
- Create: `src-tauri/tests/import_v2_web_ingestion.rs`
- Create: `docs/qa/import-v2-web-ingestion.md`
- Modify: `SPEC/progress.txt`

**Interfaces:**
- Stable actions: retry route, switch route, begin login, authorize one private target, install browser/media capability, invoke Agent, skip, view log.
- Reuses Core partial success, typed task references, Quality Gate, preview, confirmation, and atomic commit.

- [ ] **Step 1: Write end-to-end RED tests**

Run a session containing generic, WeChat, Zhihu, and Bilibili items with one challenge, one cancellation, one quality failure, one duplicate URL with new content, and one private redirect. Assert independent terminal states and no placeholder Markdown.

- [ ] **Step 2: Add stable web issue/action mapping**

```rust
pub enum WebImportErrorCode {
    UrlRejected, PrivateTargetBlocked, RedirectRejected, TlsFailed, ResponseTooLarge,
    ChallengeDetected, LoginRequired, CaptchaRequired, StructureChanged,
    SubtitleUnavailable, ConnectorRateLimited,
}
```

- [ ] **Step 3: Run security and secret scans**

Seed cookies, Authorization, passwords, signed params, fragments, proxy credentials, and local usernames. Scan staging metadata allowed for current task, session/history/source manifests, task JSON, logs, Markdown, and exports; only the protected runtime/session storage may contain secret material.

- [ ] **Step 4: Run frozen corpus, manual smoke, three-platform, and performance gates**

Record connector version, fixture version, browser revision, route timing, pack sizes/RSS, retries, and smoke date. Verify same-domain limits, cancellation within 1 second, child exit within 5 seconds, no orphan profiles/processes, and offline deterministic fixture tests.

- [ ] **Step 5: Run final check, dual review, record progress, and commit**

Review A checks design, source/version semantics, and Core integration. Review B starts fresh and attacks SSRF/rebinding/redirects, secret persistence, browser containment, process cleanup, and connector drift detection. Fix findings, rerun `npm run check`, then add a newest-first progress entry.

```bash
git add src-tauri/src/services/import_v2/orchestrator.rs src-tauri/src/models/import_v2.rs src/types/importV2.ts src-tauri/tests/import_v2_web_ingestion.rs docs/qa/import-v2-web-ingestion.md SPEC/progress.txt
git commit -m "test(import): certify web ingestion release gates"
```

## Self-Review Result

- Spec coverage: exact URL, domain-first routing, generic HTTP/Readability/browser, WeChat/Zhihu/Bilibili, phase-two Xiaohongshu/X, login/captcha waits, SSRF redirects, secrets, images, video subtitles/ASR, retries/rate limits, no crawl/comments/mirrors, partial success, recovery, and release gates are assigned.
- Placeholder scan: every task names concrete files, interfaces, RED cases, commands, expected outcomes, and commits.
- Type/API consistency: web inputs still become Core `ImportInputKind::Url`; all engines return Core `EngineResult`; only public normalized locators are serializable; formal writes remain in Core commit.
- Dependency order: Tasks 1–6 establish safe common infrastructure; Tasks 7–9 deliver stage-one connectors; Tasks 10–11 are explicit stage-two gates; Task 12 certifies both without making stage two a stage-one cutover blocker.
