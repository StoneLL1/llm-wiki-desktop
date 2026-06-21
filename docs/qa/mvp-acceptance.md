# Task 14 — MVP Acceptance Evidence

This document records the verification evidence for the End-to-End MVP flow
(Task 14, `IMPLEMENTATION_PLAN.md` §16). It is the QA artifact: command
outputs, fixture paths, sample-wiki counts, and known parser limitations.

## Verification commands and results

Run from the repository root. Rust integration tests use `--no-default-features`
to skip the GUI/Tauri feature (Windows Insider WebView2 DLL blocker — see
`SPEC/gotchas.txt`); the integration-test binary does not import GUI DLLs.

### Frontend

```
npm run test    # 20 files, 72 tests, all passed
npm run lint    # eslint . --max-warnings=0 → 0 problems
npm run build   # tsc -b && vite build → built in ~2s
```

Includes the new fixture-matrix test (`src/test/fixtures/project-fixtures.test.ts`),
5 tests covering every `SourceFileType` × parser-gap status.

### Backend

```
cargo test --manifest-path src-tauri/Cargo.toml --test mvp_flow --no-default-features --offline
# test result: ok. 9 passed; 0 failed

cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features --offline
# test result: ok. 247 passed; 0 failed
```

## MVP loops covered (`src-tauri/tests/mvp_flow.rs`)

| Loop | Test | What it verifies |
| --- | --- | --- |
| 1. Project → wiki | `project_to_wiki_loop_creates_imports_compiles_searches_and_graphs` | create project (index/overview/log skeleton present) → import MD+CSV (preview archives ≥1; CSV extracts as raw text through the shared text branch) → compile a 4-page manifest incl. all core pages via `apply_confirmed_manifest` → concept page lands on disk → keyword search finds it → graph `resolve()` builds then reuses cache (`.app/graph-cache.json` written) |
| 2. Sample wiki | `sample_wiki_loop_scans_searches_and_caches_graph` | copies a ≤50-page slice of `wiki/wiki/` into a temp project (never tests in place, per CLAUDE.md) → `scan_wiki` covers every copied page → graph builds non-empty node set and writes cache. Skips gracefully if the sample is absent. |
| 3. AI-assisted (fakes) | `ai_assisted_loop_fake_agent_detected_and_byok_runs` | fake `ProcessRunner` → Claude detected `Installed` → BYOK provider persists without secret material (`sk-ant` absent) → real local retrieval cites the relevant page → fake assistant answer saved as `wiki/queries/{slug}.md` with `[[wikilinks]]` citations → deep-lint JSON parsed (`duplicate_topic`) → export prompt built + HTML record persisted under `exports/html/` |
| 4. Safety | `safety_loop_compile_conflict_does_not_mutate_without_confirmation` | externally-edited index surfaces as a conflict; original edit survives |
| 4. Safety | `safety_loop_confirm_requires_matching_state_and_creates_checkpoint` | confirmed create works; post-confirm drift → `CONFIRMATION_STATE_MISMATCH` |
| 4. Safety | `safety_loop_chat_overwrite_requires_checkpoint` | unconfirmed overwrite → `FILE_ALREADY_EXISTS` (content survives); confirmed overwrite with git + matching hash → checkpoint commit recorded, new content lands |
| 4. Safety | `safety_loop_lint_fix_rejects_paths_outside_wiki` | `.app/settings.json` target → `LINT_FIX_PATH_OUT_OF_SCOPE` |
| 4. Safety | `safety_loop_git_checkpoint_records_commit` | real changes → checkpoint with commit hash |

## Fixture paths

- **Format matrix (TS):** `src/test/fixtures/project-fixtures.ts` — one entry per
  `SourceFileType` (pdf, document, presentation, spreadsheet, csv, markdown,
  text, html, url, image, unknown) plus clipboard Markdown.
- **Fixture matrix test:** `src/test/fixtures/project-fixtures.test.ts`.
- **Backend loop tests:** `src-tauri/tests/mvp_flow.rs`.

## Sample wiki counts

Loop 2 copies a bounded slice (cap 50 pages) of the real `wiki/wiki/` sample
into a throwaway temp project. The slice cap keeps the test fast while still
exercising scan + graph-cache over heterogeneous real pages. Full-scale
200–500 page performance is exercised by the sample itself; the integration
test asserts the scan count equals the copied count (no drops).

## Known parser limitations (explicit, not hidden)

The MVP ships no binary-document parsers. These formats surface as explicit
`unsupported` partial results through the import pipeline, so users always see
that no text was extracted rather than a silent empty page:

- **PDF** — no built-in text extractor (deferred to compile Agent/Skill).
- **DOCX / PPTX / XLSX** — no Office format parsers in MVP.
- **Image** — OCR / vision deferred to the compile Agent/Skill per CLAUDE.md
  (the import layer "only losslessly preserves"; it does not judge image value).

Formats that DO extract in the MVP: Markdown, plain text, CSV (raw text via the
shared text branch), HTML (Readability.js), URL (fetch + Readability.js metadata).

## Flow descriptions

- **Graph flow:** `GraphService::resolve()` builds from `WikiPageMeta` (one node
  per page, edges from `[[wikilinks]]` + tag co-occurrence), content-hashes the
  page set, and caches to `.app/graph-cache.json`. First call is `cached: false`,
  identical second call is `cached: true`.
- **Chat flow:** `build_retrieval_context()` does local keyword retrieval →
  honest citations (the pages fed to the model) + assembled prompt. The model
  call itself is out-of-service; `save_answer_to_wiki` writes the answer with
  scoped Git checkpoint on overwrite.
- **Export flow:** `build_export_prompt()` assembles the `skills/html-*` prompt
  (no secrets), `write_html()` lands output strictly under `exports/html/`,
  `append_record()` persists to `.app/exports.json`.
- **Lint flow:** local deterministic rules in `run_local_lint()`; Agent
  deep-lint JSON parsed by `parse_agent_issues()`; fixes constrained to
  `wiki/` paths with checkpoint-gated high-risk branches.

## Manual and release-configuration acceptance

- Packaged Windows/macOS/Linux builds must manually verify tray close behavior,
  OS notification click routing, native path selection, and Unicode/CJK paths.
- Real Agent CLI and BYOK provider calls require local credentials and are not
  exercised by the fake-backed automated suite.
- The update UI intentionally reports that no update source is configured.
  Shipping download/install requires a release endpoint and signing public key;
  those are release inputs and are not fabricated in application code.
- Sources imported before `.app/source-index.json` existed are listed, but
  automatic replace/delete is refused because their extracted-artifact ownership
  cannot be inferred safely.
