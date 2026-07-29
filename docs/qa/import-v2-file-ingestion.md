# Import V2 File Ingestion Release-Gate Evidence

> Historical QA evidence; it does not cover the current Source, image, audio, video, OCR / ASR and independent compile contract. Current gate: [`../superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../superpowers/specs/2026-07-24-import-source-media-flow-design.md).

Date: 2026-07-12
Baseline: `3bd282c6a86a5baa2d16660d1387b617e88a35a7`
Evidence candidate: `codex/import-v2-file-ingestion`
Overall status: **UNVERIFIED — RELEASE BLOCKER**

This report distinguishes executable source-level contract evidence from evidence that requires
signed capability payloads, a frozen Golden Corpus, reference hardware, or platform CI. It does
not certify a one-time cutover. No external package or binary was downloaded or fabricated.

## Declared format matrix

| Format | Implemented deterministic route | Executable evidence | Golden payload evidence | Status |
| --- | --- | --- | --- | --- |
| Markdown | native engine | normalization, immutable byte snapshot, images, unsafe HTML/path tests | repository-owned text fixtures only | contract passed; full corpus unverified |
| DOCX | modern Office pack, MarkItDown fallback | routing and quality floor tests | no real parser payload or golden DOCX | blocked |
| DOC | isolated LibreOffice conversion, qualified Office Oxide fallback | route, sandbox profile, macro/ActiveX rejection contracts | no LibreOffice payload or golden DOC | blocked |
| PDF | text/layout/selective OCR route | page routing, encryption and PDF actions rejection tests | no Docling/OCR payload, scanned/mixed PDF corpus absent | blocked |
| XLSX | modern Office pack, MarkItDown fallback | workbook modes, formulas plus values, hidden sheets, exact count metrics | no real parser payload or golden XLSX | blocked |
| XLS | isolated LibreOffice conversion, qualified Office Oxide fallback | route and source/cache identity tests | no LibreOffice payload or golden XLS | blocked |
| PPTX | modern Office pack, MarkItDown fallback | ordered slides, notes, meaningful images, exact count metrics | no real parser payload or golden PPTX | blocked |
| PPT | isolated LibreOffice conversion, qualified Office Oxide fallback | route, source immutability, active-content policy tests | no LibreOffice payload or golden PPT | blocked |

Critical thresholds encoded in tests are 98% normalized text coverage, 95% table/non-empty-cell
coverage where applicable, exact page/sheet/slide counts, and explicit warnings instead of silent
loss. OCR character accuracy and human readability cannot be measured without real signed engines
and frozen binary fixtures.

## Capability manifest inventory

All checked-in manifests use JSON-RPC 2.0 stdio and staging-only execution. Their zero hashes,
placeholder signatures, zero byte sizes, or empty target lists mean they are source/planning
manifests and are intentionally not installable release artifacts.

| Pack / engine | Manifest version | License | Declared platform state | Planning size | Actual evidence |
| --- | --- | --- | --- | --- | --- |
| document-standard / MarkItDown | 0.1.0 | MIT | four triples declared; payload absent | small-to-medium | actual package size: unavailable; installed size/RSS/runtime unavailable |
| document-layout / Docling | 2.48.0 | MIT | four triples declared; payload absent | large | actual package size: unavailable; model size/RSS/runtime unavailable |
| office-legacy / LibreOffice | 24.2.7 | MPL-2.0 OR LGPL-3.0-or-later | four triples declared; payload absent | very large | actual package size: unavailable; transitive audit unavailable |
| office-oxide | 0.1.0-probation | MIT OR Apache-2.0 | four triples declared; disabled without qualification | small planning class | actual package size: unavailable; independent qualification absent |
| ocr-basic / Tesseract | 5.4.1 | Apache-2.0 AND BSD-2-Clause | four triples declared; release CI required | moderate + tens of MB/language | actual package size: unavailable; accuracy/RSS unavailable |
| ocr-cjk-accurate / PaddleOCR | 3.0.0 | Apache-2.0 | no qualified triples | large | actual package size: unavailable; blocked on reproducible CPU builds |
| media-runtime / FFmpeg | 8.0.1 | LGPL-2.1-or-later | no published triples | medium | actual package size: unavailable; strict-LGPL binary/SBOM absent |
| asr-whisper / whisper.cpp | 1.8.3 | MIT | no published triples; models separate | binary small-to-medium; model dominates | actual package size: unavailable; model bytes/RSS unavailable |

Fallbacks remain: native Rust extraction for lightweight files; text-layer PDF before layout/OCR;
Tesseract before PaddleOCR; subtitles before local ASR; LibreOffice conversion before probationary
Office Oxide; Agent assistance only after deterministic hard failure and only as a staged candidate.
No GPL, AGPL, non-commercial, Pandoc, runtime `pip install`, or runtime `npm install` route is used.

## Security, recovery, and lifecycle matrix

| Gate | Evidence | Status |
| --- | --- | --- |
| path traversal and authorized staging | OOXML entry validation, native image/output containment, Core artifact validation | passed contract |
| archive bomb | OOXML entry count and 64 MiB expanded-size bounds | passed contract; large corpus unverified |
| symlink/reparse | discovery rejects links/reparse points before traversal | platform contract; macOS/Linux reparse smoke unverified |
| malicious HTML | scripts, styles, event handlers, unsafe URI schemes removed | passed fixture |
| PDF actions | passive inspection rejects active actions without executing them | passed generated fixture |
| macro/ActiveX | legacy runner policy disables macro/plugin/network execution | contract only; real binary sandbox unverified |
| Prompt Injection | imported text remains data and engines cannot expand authority | architecture contract; adversarial Agent suite belongs to Agent package |
| password secrecy | password absent from serialized inspection/protocol objects and logs | passed contract; encrypted real corpus unverified |
| secret redaction | Core structured-log redaction tests | passed contract |
| timeout and child process cleanup | pack runner timeout/process-tree design tests | contract only; real child process exit under 5s unverified |
| cancellation | pre-cancel and scan cancellation leave no staging output | passed local contract |
| crash recovery | Core journal/restart tests recover staged transactions | passed Core suite; real OS-kill smoke unverified |
| repeated import | normalized locator dedup and immutable source versions | passed contract |
| partial success | per-item Core commit transaction and batch tests | passed Core suite |
| disk-full | atomic transaction error paths | simulated Core contract only; filesystem quota test unverified |
| package corruption/download interruption | signature/hash/resumable manifest and runtime rejection contracts | passed contract; real release archive absent |
| external Wiki edit | expected-hash/three-way conflict Core tests | passed Core suite |

## Performance and platform gates

The Rust integration gate creates 10,000 Markdown entries before timing discovery. On the local
Windows host it asserts first callback in less than 1 second and pre-cancellation response in less
than 1 second. This is a contract benchmark, not a stable reference-device result; elapsed values,
hardware identity, peak RSS, and scheduler free-memory telemetry are not captured by the current
test harness. No real pack process exists, so the under-5-second process-tree exit gate cannot be
claimed.

| Platform | 10k first batch <1s | cancelling <1s | pack tree exit <5s | 20%/1 GiB memory reserve |
| --- | --- | --- | --- | --- |
| Windows | locally asserted, reference device undocumented | locally asserted | unverified | scheduler contract only |
| macOS | unverified | unverified | unverified | scheduler contract only |
| Linux | unverified | unverified | unverified | scheduler contract only |

macOS, Linux, documented-reference-device measurements, peak RSS, real capability-pack runtime,
and process-tree cleanup are **UNVERIFIED — RELEASE BLOCKER**. A release pipeline must publish the
raw timing/RSS logs and signed manifest for each supported target triple.

## Required evidence before cutover

1. Build signed immutable pack archives in release CI; replace placeholder hashes/signatures/sizes,
   generate SBOM/notices/build provenance, and audit all transitive licenses.
2. Commit or securely provision a licensed, immutable Golden Corpus for all eight formats, including
   Chinese/English, Unicode/long names, formulas, notes, images, hidden sheets, scans, encryption,
   corruption, malicious samples, and oversized cases.
3. Run accuracy and structural assertions with actual payloads on documented Windows, macOS, and
   Linux reference devices; record runtime, peak RSS, warnings, and exact package/model sizes.
4. Exercise timeout, app/pack crash, restart recovery, disk exhaustion, interrupted downloads, and
   complete process-tree cleanup with OS-level tests.
5. Run the repository unified check and both independent reviews after all above evidence exists.

Until those items are complete, the stable interfaces available to later Web Ingestion are the Core
`ImportEngine`, `EngineRequest`, `EngineResult`, `CancellationToken`, versioned JSON-RPC stdio
protocol, item-contained staging, structured progress/log/warning messages, and the Core-only formal
commit boundary. Web engines must not weaken or bypass them.
