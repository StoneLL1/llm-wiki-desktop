# Import V2 Web Ingestion release evidence

> Historical QA evidence. Future release validation must additionally prove every successful URL creates a Source and never auto-compiles, as required by [`../superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../superpowers/specs/2026-07-24-import-source-media-flow-design.md).

## Local automated gate (2026-07-12, Windows x86_64)

- Frozen fixtures: generic contract, WeChat challenge/article, Zhihu article/auth/drift, Bilibili metadata/subtitle/no-subtitle, Xiaohongshu signed note, X same-author thread.
- Security: public-only URL serialization; DNS-set/connected-IP equality; localhost/private/link-local/reserved/multicast blocking; redirect reparsing; response byte/MIME limits; signed image/subtitle query redaction; dedicated profile denial; fixed yt-dlp surface.
- Recovery: cancellation checks in streaming fetch and reused capability process-tree termination/media temp cleanup contracts.
- Unified check: required after final review; record final command result in the delivery report.

## Release gates

| Phase | Code/fixture status | Distribution/manual evidence | Release status |
| --- | --- | --- | --- |
| Stage 1: generic, WeChat, Zhihu, Bilibili | Contracts, thin fixture parsers, and security primitives implemented; production fetch-to-extract integration is incomplete | Signed sandboxed browser-lite/browser/media payloads, measured sizes/RSS, macOS/Linux and authenticated online smoke are not present in this repository | Blocked / unavailable |
| Stage 2: Xiaohongshu, X | Thin fixture parsers exist; production routes and generic fallback are forcibly disabled | Requires completed Stage 1 plus account/manual drift and access-control smoke | Disabled |

No credentials, Cookie values, account names, proxy secrets, or signed target URLs may be added to this file. Online smoke must record only date, platform, application version, connector version, environment class, and pass/fail.
