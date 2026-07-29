# Import, Extraction, and Compile Repair Design

> Historical design, partially superseded. Keep its legacy bug evidence only; current Import, Source, OCR / ASR and compile boundaries are defined by [`2026-07-24-import-source-media-flow-design.md`](2026-07-24-import-source-media-flow-design.md).

## Goal

Restore the complete local-first import pipeline:

`external source -> raw/sources archive -> raw/extracted Markdown -> wiki compile`

The repair covers native desktop drag-and-drop, structured Markdown extraction for supported source formats, and compile preflight/safety behavior. It must preserve the existing Tauri command/service boundaries, TaskService lifecycle, Git checkpoints, Unicode-safe paths, and immutable-source rules.

## Current Evidence

- `ImportView.tsx` registers `getCurrentWebview().onDragDropEvent`, but reads `event.type` and `event.paths`. The installed Tauri v2 API exposes the drag payload through `event.payload.type` and `event.payload.paths`; TypeScript compilation confirms the mismatch.
- `tauri.conf.json` does not disable drag-and-drop. No capability entry is required for the webview drag event in the current application shape.
- Listener cleanup already handles asynchronous registration and component unmount, but it needs regression coverage so remounting cannot accumulate listeners.
- `ExtractionService` already routes PDF and OOXML formats through adapters and writes extracted text under `raw/extracted/`. Its targeted unit suite passes, but the required structured-Markdown semantics and representative fixtures are incomplete.
- Compile workspaces already copy `raw/extracted/`, and both Agent and BYOK prompt assembly read Markdown from that directory. Compile already runs as a cancellable TaskService task and creates a pre-operation Git checkpoint.
- Compile currently lacks an explicit preflight error when no extracted Markdown is available.

## Chosen Approach

Extend the current services instead of adding a parallel import pipeline or replacing them with a broad document-processing framework.

This is the smallest approach that preserves existing persistence, DTO, task, and confirmation contracts. Heavy Office libraries remain unnecessary unless the existing OOXML adapter cannot meet a concrete acceptance test. OCR is intentionally excluded from import; scanned PDFs receive an actionable status explaining that OCR/vision belongs to the compile Agent/Skill path.

## Drag-and-Drop Design

`ImportView` will consume the Tauri event envelope correctly:

- `enter` and `over` enable the drop highlight.
- `leave` disables it.
- `drop` disables it and forwards the payload paths to the existing preview request.
- Paths remain absolute Unicode strings at the frontend/backend boundary. The frontend must not rewrite drive letters, separators, or CJK names; backend path handling remains authoritative.
- Native browser `dragover`/`drop` handlers will only be added if a regression test or runtime evidence shows DOM default behavior still interferes. Tauri native drag-drop is the primary path.
- The asynchronous `unlisten` lifecycle remains cancellation-safe and gains tests for unmount/remount behavior.

The event-to-action logic will be isolated in a small pure helper so tests can verify payload handling without a real webview.

## Extraction Design

All textual supported formats produce UTF-8 Markdown in `raw/extracted/` during preview. Stored relative paths use forward slashes.

### PDF

- Continue using local `pdf-extract` for text-layer PDFs.
- Join extracted pages into readable Markdown and report page and word counts.
- A valid PDF with no useful text layer returns a clear OCR/vision handoff message rather than pretending extraction succeeded.
- Corrupt PDFs return `Failed`, not `Unsupported`.

### DOCX

- Read `word/document.xml` from the ZIP container.
- Map heading paragraph styles to Markdown headings.
- Preserve paragraphs and list paragraphs; represent list items with Markdown bullets.
- Preserve tables as Markdown tables where the OOXML structure supplies rows and cells.

### PPTX

- Read slide XML in numeric slide order.
- Emit one `## Slide N` section per slide.
- Preserve text paragraphs and map bullet paragraphs to Markdown list items where paragraph properties expose list semantics.
- Report the slide count as page count.

### XLSX and CSV

- Emit Markdown tables rather than flattened text.
- XLSX shared-string cells use `sharedStrings.xml` indexes only when `t="s"`.
- Numeric cells remain numeric, inline strings remain strings, missing cells become empty table cells, and XML cell references preserve column placement.
- Multiple worksheets receive Markdown section headings.
- CSV quoting and escaped delimiters are handled by the smallest necessary dependency or a focused parser if the current dependency set already supports the required cases.

### Images and Limits

- Images remain archived assets without import-time OCR or visual interpretation.
- ZIP entry extraction reuses the existing 64 MiB per-entry guard. Container/file guards must fail clearly before excessive allocation.
- No empty metadata fields are added solely for UI appearance. `SourceMetadata` changes only if a verified adapter needs a meaningful field.

## Compile Design

Compile remains a background TaskService operation with cancellation, persisted task state, logs, and progress.

Before selecting Agent/BYOK or invoking a model, compile will validate that `raw/extracted/` contains at least one readable Markdown file. An empty input set fails the task with a stable, actionable backend error instead of allowing an empty generation attempt.

The existing workspace copy and prompt assembly remain the source of truth for `raw/extracted/ -> wiki/`. Tests will prove extracted Markdown reaches the compile workspace/prompt.

The existing pre-compile Git checkpoint remains mandatory. Existing wiki pages are protected by baseline hashes and conflict confirmation: generated content is not applied when a path changed or already requires explicit resolution. The frontend's checked “compile after import” control is the user's request to start compilation; any destructive overwrite/deletion still enters the existing high-risk confirmation flow.

## Error Handling

- Drag-drop runtime absence remains silent only in browser/unit-test contexts; malformed Tauri payloads do not start preview.
- Extraction failures stay per-file so one bad source does not abort the whole preview batch.
- Unsupported legacy binary Office formats retain explicit conversion guidance.
- Empty compile input uses a dedicated error code and user-facing guidance to import/extract content first.
- Cancellation must leave original sources and wiki pages untouched and remove temporary compile workspaces.

## Testing Strategy

Implementation follows red-green-refactor:

- Frontend unit tests for Tauri payload dispatch, highlight transitions, Unicode/Windows paths, and listener cleanup.
- Rust extraction tests for a real text-layer PDF fixture, scanned/no-text PDF behavior, DOCX headings and lists, multi-slide PPTX, XLSX shared strings plus numeric cells, CSV table conversion, CJK filenames, and ZIP size rejection.
- Rust compile tests for empty extracted input and for copied extracted Markdown appearing in the Agent/BYOK compile context.
- Existing targeted tests run after each change, followed by the full required frontend and Rust check suite.

## Scope and Commit Boundaries

Planned commits:

1. `fix(import): enable native drag-drop preview`
2. `feat(extract): convert supported documents to markdown`
3. `fix(compile): require extracted markdown input`
4. Documentation/logging commit only if progress, gotchas, or roadmap changes do not fit safely with the relevant implementation commit.

The implementation will not modify or stage `UI-Frontend-design/`, `.claude/`, `src-tauri/gen/`, `wiki/wiki/`, runtime `wiki/.app/*`, or unrelated existing worktree changes.

## Acceptance Criteria

- Dropping Windows files, including CJK filenames, highlights the drop target and starts preview immediately.
- Supported textual formats produce Markdown under `raw/extracted/` during preview.
- PDF/DOCX/PPTX/XLSX/CSV behavior matches the structured semantics above and is guarded against oversized ZIP entries.
- Images are archived without OCR.
- Confirmed import can start compile, compile consumes extracted Markdown, and empty extracted input reports a clear failure.
- Compile remains cancellable, background-safe, progress-visible, checkpointed, and conflict-confirmed before destructive changes.
- All required frontend/Rust checks pass, or any pre-existing out-of-scope blocker is identified precisely and kept separate.
- Two independent review passes are merged, valid findings are fixed, and the full check suite is rerun.
