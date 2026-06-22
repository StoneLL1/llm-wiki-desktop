# Import, Extraction, and Compile Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make native drag-and-drop start an import preview, make every supported textual source produce real Markdown under `raw/extracted/`, and make compile consume that Markdown or fail clearly before invoking an Agent/LLM.

**Architecture:** Keep the existing React -> Tauri command -> Rust service -> local files flow. Fix the Tauri event envelope at the UI boundary, extend the existing `ExtractionService` adapters in place, change extracted artifacts from `.txt` to `.md`, and add a compile preflight at the service/command boundary while retaining TaskService, Git checkpoints, cancellation, and conflict confirmation.

**Tech Stack:** React 19, TypeScript, Vitest, Tauri v2, Rust, `pdf-extract`, `quick-xml`, `zip`, and the Rust `csv` crate only if required for standards-compliant quoted CSV parsing.

---

## File Map

- Create `src/features/import/dragDrop.ts`: pure translation from Tauri drag payloads to UI state/preview actions.
- Create `src/features/import/dragDrop.test.ts`: Windows, CJK, drop/highlight, and ignored-empty-drop coverage.
- Modify `src/features/import/ImportView.tsx`: consume `event.payload`, delegate to the pure helper, preserve cancellation-safe unlisten.
- Modify `src-tauri/src/services/extraction_service.rs`: Markdown output naming, CSV tables, structured DOCX/PPTX/XLSX extraction, PDF and size-limit regressions.
- Modify `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`: add `csv` only if the focused parser is insufficient.
- Modify `src-tauri/src/services/compile_service.rs`: reusable extracted-Markdown discovery/preflight.
- Modify `src-tauri/src/commands/compile_commands.rs`: fail the TaskService task before checkpoint/model routing when no extracted Markdown exists; prove prompt/workspace wiring.
- Modify `src-tauri/tests/mvp_flow.rs` only as needed to repair existing method-call signature drift that blocks the required full Rust suite.
- Modify `SPEC/progress.txt`, `SPEC/gotchas.txt`, and optionally `SPEC/roadmap/import.md`: reverse-chronological milestone and newly verified pitfalls/out-of-scope items.

Forbidden from edits and staging: `.claude/`, `UI-Frontend-design/`, `src-tauri/gen/`, `wiki/wiki/`, `wiki/.app/*`, and unrelated existing worktree changes.

## Baseline Evidence to Preserve

- `npm run build` currently exposes the drag event envelope error plus unrelated TypeScript drift in `LeftSidebar`, `RightContextPanel`, and `OpenFolderAsProjectDialog`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features --offline extraction_service` passes 23 tests.
- Full Rust tests currently stop in `src-tauri/tests/mvp_flow.rs` because two callers omit newly required language arguments.
- `write_extracted_text` currently writes `.txt`, while compile enumerates Markdown files. This is the confirmed extracted-to-compile path mismatch.

### Task 1: Fix native drag-and-drop preview

**Files:**
- Create: `src/features/import/dragDrop.ts`
- Create: `src/features/import/dragDrop.test.ts`
- Modify: `src/features/import/ImportView.tsx:136-160`

- [ ] **Step 1: Write failing payload behavior tests**

Add tests that pass payloads shaped like Tauri v2 `DragDropEvent` and assert:

```ts
expect(reduceDragDrop({ type: "enter", paths: ["D:\\\\资料\\\\论文.pdf"], position })).toEqual({ active: true, paths: null });
expect(reduceDragDrop({ type: "drop", paths: ["D:\\\\资料\\\\论文.pdf"], position })).toEqual({ active: false, paths: ["D:\\\\资料\\\\论文.pdf"] });
expect(reduceDragDrop({ type: "drop", paths: [], position })).toEqual({ active: false, paths: null });
expect(reduceDragDrop({ type: "leave" })).toEqual({ active: false, paths: null });
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `npm run test -- src/features/import/dragDrop.test.ts`

Expected: FAIL because `reduceDragDrop` does not exist.

- [ ] **Step 3: Implement the minimal pure reducer**

Export a small function accepting `DragDropEvent` and returning `{ active: boolean; paths: string[] | null }`. Do not normalize or trim native absolute paths.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run: `npm run test -- src/features/import/dragDrop.test.ts`

Expected: PASS.

- [ ] **Step 5: Fix the listener envelope and cleanup behavior**

In `ImportView.tsx`, read `event.payload`, apply the reducer, update the highlight, and call `onRequestPreview` only for a non-empty dropped path list. Retain the `cancelled` flag and call the returned unlisten function both after late registration and during cleanup.

- [ ] **Step 6: Verify frontend behavior**

Run:

```powershell
npm run test -- src/features/import/dragDrop.test.ts
npx tsc -b --pretty false
```

Expected: the drag-drop TypeScript errors disappear. Record unrelated pre-existing TypeScript errors separately if they remain.

- [ ] **Step 7: Commit only drag-drop files**

```powershell
git add -- src/features/import/dragDrop.ts src/features/import/dragDrop.test.ts src/features/import/ImportView.tsx
git commit -m "fix(import): enable native drag-drop preview"
```

### Task 2: Make all text imports produce Markdown artifacts

**Files:**
- Modify: `src-tauri/src/services/extraction_service.rs`
- Modify if necessary: `src-tauri/Cargo.toml`
- Modify if necessary: `src-tauri/Cargo.lock`

- [ ] **Step 1: Write failing Markdown artifact tests**

Extend extraction tests to assert that Markdown, text, HTML, CSV, PDF, and OOXML success results have `extracted_text_path` ending in `.md`, use forward slashes, remain below `raw/extracted/`, and safely preserve a CJK stem.

- [ ] **Step 2: Run and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features --offline extracted_artifact -- --nocapture`

Expected: FAIL because current artifacts end in `.txt`.

- [ ] **Step 3: Change the artifact writer to `.md`**

Update `write_extracted_text` to generate `<sanitized-stem>-<hash8>.md`. Keep content-addressing, project-root validation, forward-slash serialization, and `raw/extracted/` enforcement unchanged.

- [ ] **Step 4: Verify GREEN and the compile-compatible extension**

Run the focused artifact tests and the complete extraction-service library tests.

Expected: PASS, with every successful textual extraction returning a `.md` path.

- [ ] **Step 5: Write a failing CSV-to-Markdown table test**

Use a CSV fixture containing a quoted comma and a pipe character. Assert output shaped as a Markdown table, including escaped `\|` and a separator row:

```markdown
| name | note |
| --- | --- |
| Alice | hello, world |
```

- [ ] **Step 6: Run and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features --offline csv_is_converted_to_markdown_table -- --nocapture`

Expected: FAIL because CSV currently passes through unchanged.

- [ ] **Step 7: Implement standards-compliant CSV table conversion**

Route `SourceFileType::Csv` separately from direct text. Parse quoted fields, normalize ragged rows to the widest width, escape Markdown table cells, use the first record as the header, and synthesize `Column N` headers only when the source has no header values. Add `csv = "1"` only if needed.

- [ ] **Step 8: Verify CSV and direct-text regressions**

Run the focused CSV test, then all extraction-service tests.

Expected: PASS without changing Markdown/TXT/URL semantics.

### Task 3: Preserve document structure in DOCX, PPTX, and XLSX

**Files:**
- Modify: `src-tauri/src/services/extraction_service.rs`

- [ ] **Step 1: Write failing DOCX heading/list/table tests**

Build an in-memory DOCX ZIP whose `word/document.xml` contains `Heading1`, a numbered/bulleted paragraph, normal text, and a two-column table. Assert the extracted Markdown contains `# Heading`, `- item`, paragraph breaks, and a valid Markdown table.

- [ ] **Step 2: Run and verify DOCX RED**

Run the named DOCX structured-Markdown test. Expected: FAIL because current code flattens all `<w:t>` runs.

- [ ] **Step 3: Implement paragraph/table-aware DOCX parsing**

Use `quick-xml` to accumulate text runs within paragraphs and cells, inspect paragraph style and numbering properties, flush paragraphs at `w:p`, and flush table rows at `w:tr`. Keep footnotes/endnotes as trailing Markdown sections or paragraphs without inventing unsupported formatting.

- [ ] **Step 4: Verify DOCX GREEN**

Run the structured DOCX test and existing DOCX extraction test. Expected: PASS.

- [ ] **Step 5: Write failing multi-slide PPTX tests**

Create two slide XML entries including a bullet paragraph. Assert exact section headings `## Slide 1` and `## Slide 2`, numeric ordering, slide text, and `- bullet`.

- [ ] **Step 6: Run RED, implement, and verify GREEN**

Update `read_pptx_text` to wrap every slide (including text-empty slides if needed for page numbering) in a slide heading and preserve paragraph/bullet boundaries. Run focused and existing PPTX tests.

- [ ] **Step 7: Write failing XLSX table-layout tests**

Build a worksheet with shared-string A1, numeric C1, a missing B1, and a second row. Assert a `## Sheet 1` heading and a Markdown table where the empty B column is preserved and numeric `1` is never resolved through shared strings.

- [ ] **Step 8: Run RED, implement, and verify GREEN**

Track `<row>` and `<c r="...">` references, resolve `t="s"`, inline strings, and literal numerics into a 2D grid, then serialize through one Markdown-table helper. Run all XLSX regressions, especially the shared-string/numeric test.

- [ ] **Step 9: Add and verify the 64 MiB ZIP-entry guard regression**

Create a test ZIP entry whose declared/uncompressed XML part exceeds the cap without allocating an unbounded buffer. Assert `EXTRACT_ENTRY_TOO_LARGE` and verify no extracted artifact is written.

### Task 4: Verify PDF text and scanned-PDF behavior

**Files:**
- Modify: `src-tauri/src/services/extraction_service.rs`

- [ ] **Step 1: Add a real text-layer PDF fixture builder and failing success test**

Generate a minimal valid PDF in the test using calculated object offsets/xref. Assert `Extracted`, preview contains the embedded text, page count is present, and the artifact ends in `.md`.

- [ ] **Step 2: Run and verify RED before any PDF adapter change**

Run the named PDF test. If it passes immediately, strengthen the fixture/assertion to cover the missing behavior rather than changing production code without a failing regression.

- [ ] **Step 3: Make the smallest PDF correction required by the test**

Retain `pdf-extract`. Do not add MinerU or OCR dependencies. Preserve actionable `Failed` behavior for valid no-text PDFs and corrupt PDFs.

- [ ] **Step 4: Add a valid image-only/scanned PDF test**

Assert the result is `Failed`, is not `Unsupported`, mentions OCR/visual compile handling, and writes no fake Markdown artifact.

- [ ] **Step 5: Run the complete extraction suite**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features --offline extraction_service -- --nocapture`

Expected: all PDF, DOCX, PPTX, XLSX, CSV, CJK, and safety tests pass.

- [ ] **Step 6: Commit extraction changes**

```powershell
git add -- src-tauri/src/services/extraction_service.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(extract): convert supported documents to markdown"
```

Only stage dependency files if they actually changed.

### Task 5: Fail compile clearly without extracted Markdown and prove wiring

**Files:**
- Modify: `src-tauri/src/services/compile_service.rs`
- Modify: `src-tauri/src/commands/compile_commands.rs`

- [ ] **Step 1: Write a failing compile-input preflight test**

Create a temporary project with an empty `raw/extracted/` and assert a dedicated error:

```rust
assert_eq!(error.code, "COMPILE_INPUT_EMPTY");
assert!(error.message.contains("raw/extracted"));
```

- [ ] **Step 2: Run and verify RED**

Run the named compile preflight test. Expected: FAIL because no preflight exists.

- [ ] **Step 3: Implement reusable extracted-Markdown discovery**

Add a `CompileService` helper that recursively lists readable `.md` files under `context.raw_dir.join("extracted")`, rejects an empty set with `COMPILE_INPUT_EMPTY`, and returns sorted paths for deterministic tests. Symlinks remain rejected by workspace copying.

- [ ] **Step 4: Invoke preflight inside the TaskService lifecycle**

Call the helper after transitioning the compile task to `Running` but before creating a Git checkpoint or selecting Agent/BYOK. Let the existing task error/log/status path surface the failure; no silent success and no model call.

- [ ] **Step 5: Verify preflight GREEN**

Run focused compile tests. Expected: PASS and no checkpoint/model work for empty input.

- [ ] **Step 6: Add a prompt/workspace wiring regression**

Create `raw/extracted/资料.md`, build the compile workspace/prompt, and assert the prompt contains both `raw/extracted/资料.md` and its Markdown content. This proves the extension/path repair reaches compile.

- [ ] **Step 7: Verify TaskService/Git/cancellation invariants**

Run existing compile command/service and task tests. Confirm:

- compile tasks are cancellable;
- progress/log records remain persisted;
- checkpoint creation still precedes generated wiki writes;
- baseline conflict detection prevents silent overwrite;
- deletion/overwrite remains behind the existing confirmation registry.

- [ ] **Step 8: Commit compile changes**

```powershell
git add -- src-tauri/src/services/compile_service.rs src-tauri/src/commands/compile_commands.rs
git commit -m "fix(compile): require extracted markdown input"
```

### Task 6: Restore full verification baseline within scope

**Files:**
- Modify only if necessary: `src-tauri/tests/mvp_flow.rs`
- Modify only directly related compile blockers in frontend files if required for the mandated suite.

- [ ] **Step 1: Repair known Rust test signature drift test-first**

Update the two `mvp_flow.rs` callers to pass an explicit language such as `"en"`, matching the current service signatures. Run the affected integration test first, then the full Rust suite.

- [ ] **Step 2: Classify frontend TypeScript blockers**

Run `npm run test` and `npm run lint`. If these pass, do not broaden scope merely because `npm run build` has unrelated type drift. If required checks themselves fail due the known sidebar/provider/dialog drift, apply only minimal type-contract corrections with focused regression coverage and a separate conventional commit.

- [ ] **Step 3: Run all required checks from the beginning**

Run sequentially:

```powershell
npm run test
npm run lint
cargo test --manifest-path src-tauri/Cargo.toml --offline
cargo clippy --manifest-path src-tauri/Cargo.toml --offline -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

Then scan source-controlled code for `console.log`, `dbg!`, and `println!`, excluding reference/generated/vendor folders, and run `npm run build` to verify import resolution and TypeScript paths.

- [ ] **Step 4: If any check fails, fix and restart the entire checklist**

Do not report partial green as completion. If a platform-only WebView2/runtime blocker appears, record the exact command/error and retain the strongest offline/no-default-features evidence.

### Task 7: Dual review, logs, and final commits

**Files:**
- Modify: `SPEC/progress.txt`
- Modify if warranted: `SPEC/gotchas.txt`
- Modify if warranted: `SPEC/roadmap/import.md`

- [ ] **Step 1: Launch two review subagents in parallel**

Reviewer A receives shared task/design context and checks intent, logic, existing architecture, Git safety, TaskService, and spec consistency. Reviewer B receives a fresh review brief and checks blind spots, malformed inputs, Unicode paths, missing tests, and silent failure modes. Both must remain read-only and report file/line findings with severity.

- [ ] **Step 2: Merge findings and fix every valid issue with TDD**

For each accepted bug, add a failing regression, verify RED, implement the smallest fix, and verify GREEN. Reject findings only with concrete code/test evidence.

- [ ] **Step 3: Rerun the full checklist after review fixes**

Repeat every command from Task 6 Step 3 plus the debug-log/import-path scans.

- [ ] **Step 4: Update mandatory project logs**

Prepend one reverse-chronological progress entry:

`[2026-06-22] 导入/提取/编译 — 修复原生拖拽、全格式 Markdown 提取与 extracted→compile 前置校验 — OCR 保留在编译期 Agent/Skill，编译继续由 TaskService 与 Git 检查点保护`

Add a gotcha for the confirmed event-envelope and `.txt`/`.md` mismatch. Add roadmap entries only for verified work that genuinely exceeds this bug-fix scope.

- [ ] **Step 5: Commit documentation without staging user files**

```powershell
git add -- SPEC/progress.txt SPEC/gotchas.txt SPEC/roadmap/import.md
git commit -m "docs(import): record repaired import pipeline"
```

Only stage files that changed. Never stage `wiki/.app/*`, `SPEC/roadmap/loop-prompts.md`, or unrelated user changes.

- [ ] **Step 6: Audit commits and working tree**

Run:

```powershell
git status --short --branch
git log -8 --oneline --decorate
git diff --check HEAD~4..HEAD
```

Confirm expected conventional commits exist, no forbidden path appears in any commit, no `--no-verify` was used, and nothing was pushed.
