# Import V2 Open-Source Research Record

> Date: 2026-07-12
> Scope: File Ingestion, Web Ingestion, Agent Assistance, Migration & Cutover
> Decision basis: official repositories and official project documentation checked on 2026-07-12

## Decision Rules

- The Rust/Tauri core remains permissively licensed and does not embed GPL, AGPL, non-commercial, SSPL, Elastic-2.0, or source-available-only implementations.
- MPL/LGPL components are allowed only as isolated, replaceable capability packs with their notices, source offer/build recipe, SBOM, and upgrade-time license recheck.
- A permissive license is necessary but not sufficient. New projects must pass the repository Golden Corpus, security corpus, three-platform smoke suite, and package-size budget before becoming a primary route.
- Capability packs are immutable, signed, SHA-256 verified, installed without `pip install`/`npm install` scripts, and communicate through the existing Import Core JSON-RPC 2.0 stdio contract.
- The figures below are planning classes, not release promises. Every pinned release must record actual compressed size, installed size, model size, peak RSS, supported triples, and transitive licenses in its generated manifest.

## File and Media Candidates

| Candidate | License | Maturity / maintenance evidence | Platform and size profile | Decision | Replacement / fallback |
| --- | --- | --- | --- | --- | --- |
| Microsoft MarkItDown | MIT; Microsoft-maintained | Official v0.1.0 release and plugin architecture; active official repository. Its own README says output targets LLM consumption rather than high-fidelity human conversion. | Python; Windows/macOS/Linux. Small-to-medium runtime when format extras are selected; do not ship `[all]`. | Use as `document-standard` fallback and quality baseline, not the primary high-fidelity route. Build a frozen wheelhouse into a signed pack. | Existing Rust extractors for cheap fallback; format-specific primary engines; Agent only after deterministic hard failure. |
| Docling | MIT | IBM-origin project, active repository and broad document-layout focus. | Python plus layout/OCR models; Windows/macOS/Linux CPU support. Large pack/model footprint and high peak RSS. | Use in `document-layout` for PDF layout, reading order, tables, and page-level OCR routing. Download on demand. | Text-layer PDF route first; MarkItDown/existing Rust extractor as low-cost fallback; Agent after quality failure. |
| LibreOffice | MPL-2.0 / LGPL-3.0+ dual-license project | Long-lived office suite with documented headless start and conversion filters. | Windows/macOS/Linux; very large installation. | Isolated `office-legacy` capability pack only. Run with a disposable profile, macros/plugins/network disabled. | Office Oxide direct parse if benchmark-qualified; otherwise preserve source and offer Agent assistance. |
| Office Oxide 0.1.x | MIT OR Apache-2.0 | Official repo advertises DOC/DOCX/XLS/XLSX/PPT/PPTX and cross-platform bindings, but the project is young (0.1.x, small history/community). | Native Rust and binaries; potentially small and fast on all three platforms. | Experimental secondary route only. It must pass independent Golden Corpus and fuzz/security gates before becoming primary or replacing LibreOffice. Never rely on upstream self-reported pass rates alone. | LibreOffice conversion + modern parser remains authoritative; MarkItDown remains general fallback. |
| Tesseract 5.x | Apache-2.0; Leptonica BSD-2-Clause | Long-running OCR engine; current stable major is 5 with maintained releases. | Native Windows/macOS/Linux. Binary moderate; each language model adds tens of MB. | `ocr-basic` for English, simplified Chinese, and optional traditional Chinese. | PaddleOCR for CJK/layout accuracy; Docling page routing; Agent only after deterministic OCR quality failure. |
| PaddleOCR 3.x | Apache-2.0 | Actively maintained Paddle ecosystem with current 3.x line. | Python/native inference; Windows/macOS/Linux CPU varies by supported wheel. Models are large and must be downloaded separately. | `ocr-cjk-accurate` optional pack; only supported triples with reproducible CPU builds are published. | Tesseract basic pack; warning when the accurate pack is unavailable. |
| whisper.cpp | MIT | Active, high-adoption project with frequent releases and CPU/GPU backends. | Native Windows/macOS/Linux. Binary small-to-medium; model dominates size (base/small/medium downloaded separately). | `asr-model-*` packs; recommend multilingual small, allow base/medium. Produce VTT/SRT and timestamped Markdown only. | Platform subtitles, embedded subtitles, then local ASR; no automatic cloud ASR. |
| FFmpeg | LGPL-2.1+ by default; optional components can make a build GPL/nonfree | Mature cross-platform media toolchain with explicit official legal guidance. | Native Windows/macOS/Linux; medium binary footprint. | Ship only a pinned strict-LGPL build with `--disable-gpl --disable-nonfree`, exact configure flags, source/build recipe, notices, and SBOM. | Platform/embedded subtitle parsing when FFmpeg pack is absent; video metadata may still import without media conversion. |

## Web Candidates

| Candidate | License | Maturity / maintenance evidence | Platform and size profile | Decision | Replacement / fallback |
| --- | --- | --- | --- | --- | --- |
| Mozilla Readability | Apache-2.0 | Firefox Reader View extraction library, maintained official repository. README documents Node/jsdom usage and requires separate sanitization. | JavaScript; tiny compared with a browser. | Primary generic article extraction inside `browser-runtime-lite`; scripts/resources disabled. | Domain connector first; Playwright browser fallback when static extraction fails. |
| DOMPurify | Apache-2.0 OR MPL-2.0 | Mature sanitizer with current releases and security focus. | JavaScript; small. | Sanitize Readability output before Turndown; keep locked configuration and security fixtures. | Reject unsafe output rather than bypass sanitization. |
| Turndown | MIT | Mature HTML-to-Markdown converter; current releases and stable plugin model. | JavaScript; small. | Convert sanitized article DOM to GFM with first-party rules for tables, figures, code, and citations. | Preserve sanitized HTML snapshot when conversion quality fails; Agent may produce a candidate from that snapshot. |
| Playwright | Apache-2.0 | Microsoft-maintained with frequent releases and pinned browser revisions. | Windows/macOS/Linux; browser downloads are large (hundreds of MB). | `browser-runtime` on-demand pack. Dedicated profile per connector, no access to daily browser profiles, strict request policy, process-tree cancellation. | Static HTTP + domain connector + Readability before launching a browser. |
| yt-dlp | Unlicense for core; release binaries include separately listed permissive third-party code | Extremely active and site-adaptive, but release notes show recurring security fixes and site churn. | Standalone binaries range from a few MB to tens of MB depending on platform/bundling. | Optional isolated Bilibili media metadata/subtitle fallback only. Pin a reviewed release, verify signatures/checksums, disable `--exec`, external downloaders, netrc commands, arbitrary output templates, cookies-from-browser, plugins, and post-processors. Never use it as the URL security boundary. | First-party Bilibili page/API connector; Playwright session connector; whisper.cpp for user-authorized temporary media. |

## Agent and Migration Reuse

| Existing component / standard | Why reuse it | Decision |
| --- | --- | --- |
| Existing `AgentService`, `ProcessRunner`, `TaskService` | Already owns agent detection, typed invocation, stdout/stderr streaming, cancellation, and task persistence. | Extend through an Import-specific adapter; do not create another process runner or agent registry. |
| Existing `SecretService` / OS keyring | Already enforces the repository secret-storage boundary. | Connector sessions and BYOK approvals store only opaque keyring references in project JSON. |
| Existing `GitService` and `ImportV2Service::commit_items_cancellable` plus Core's private commit module | Already own checkpoints, diffs, expected-hash checks, journaled atomic commit, and external-edit protection. | Agent and migration code produce candidates/metadata plans; only these existing Core boundaries perform formal content writes. |
| JSON-RPC 2.0 over stdio | Already implemented by Import Core and avoids local ports. | All capability packs use the exact protocol and staging containment checks. |
| `serde` / `serde_json`, schema versioning, SHA-256 | Already used across Core persistence. | Migration reports and manifests remain JSON files; no database or new migration framework. |

## Rejected or Restricted Approaches

- Do not copy or clean up code from GPL/AGPL downloaders, office converters, OCR tools, or crawlers. Implement first-party connectors from public web behavior, documented formats, and independently authored frozen fixtures.
- Do not embed Pandoc as a converter: its GPL license conflicts with the core distribution rule. It may not be used as a code reference. Golden outputs must come from project-owned fixtures and structural assertions.
- Do not make Office Oxide primary merely because it has a permissive license and attractive benchmarks. Its 0.1.x maturity requires an independent probation gate.
- Do not install Python/Node dependencies on the user's machine at runtime. Build signed, reproducible capability archives in release infrastructure.
- Do not use the user's normal browser profile or `cookies-from-browser`. Authentication is performed in a dedicated connector profile whose secrets are encrypted through OS credential storage.

## Official Sources Consulted

- https://github.com/microsoft/markitdown
- https://github.com/docling-project/docling
- https://www.libreoffice.org/about-us/licenses/
- https://help.libreoffice.org/latest/en-US/text/shared/guide/start_parameters.html
- https://github.com/yfedoseev/office_oxide
- https://github.com/tesseract-ocr/tesseract
- https://github.com/PaddlePaddle/PaddleOCR
- https://github.com/ggml-org/whisper.cpp
- https://ffmpeg.org/legal.html
- https://github.com/mozilla/readability
- https://github.com/cure53/DOMPurify
- https://github.com/mixmark-io/turndown
- https://github.com/microsoft/playwright
- https://github.com/yt-dlp/yt-dlp
