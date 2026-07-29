# Design: Sources as Extracted Originals (Karpathy-aligned)

> 历史基础设计：本文关于 `wiki/sources/` 作为可浏览提取原稿、编译保护和引用边界的原则继续有效。去重、版本、人工编辑保护、媒体流程、AI 整理与删除规则已由 [`2026-07-24-import-source-media-flow-design.md`](2026-07-24-import-source-media-flow-design.md) 扩展并取代冲突细节。

- **Date:** 2026-06-24
- **Status:** Historical foundation / partially superseded
- **Owner:** Aletta
- **Depends on:** existing import/extract/compile pipeline
- **Reference projects:**
  - [Astro-Han/karpathy-llm-wiki](https://github.com/Astro-Han/karpathy-llm-wiki) — canonical Karpathy skill (raw = readable originals; wiki = derived concepts; no per-source summary)
  - [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki) — desktop app our layout was modeled on (`src/lib/ingest.ts`)

## 1. Problem

After importing files, the wiki view shows only `index.md`, `log.md`, `overview.md`. The extracted Markdown originals are written to `raw/extracted/`, which is **not browsable** (the wiki tree and reader are scoped to `wiki/`). After compiling, the LLM writes `wiki/sources/*.md` as **per-source summaries** ("structure and summarize that source"), not the verbatim originals. So users can never read the actual imported/extracted articles.

User decision: `wiki/sources/` should hold the **verbatim extracted originals** (import Markdown shown as-is; PDF shown as PDF→Markdown), browsable immediately after import. Compilation returns to the Karpathy model — it builds only entity/concept/synthesis/comparison pages that **cite** the originals, never summarizing or overwriting them.

## 2. Reference-project grounding (what we adopt)

| Pattern | Source | Adoption |
|---|---|---|
| No per-source summary page; raw left alone; derived pages named by concept | Astro-Han SKILL.md | Core principle for our compile |
| Clean file names (kebab-case, no hash, `-2` collision) | Astro-Han | Naming for `wiki/sources/` |
| Cascade-update affected pages after writing | Astro-Han | Add to our wiki-ingest SKILL |
| `sources: ["<filename>"]` frontmatter as machine join key (graph) | nashsu ingest.ts | Put on every page incl. source originals |
| Sources are first-class browsable; filename is the canonical key | nashsu | `wiki/sources/` originals are browsable wiki pages |
| Protect originals from compile overwrite | (new, neither repo needed it) | Our compile-protection layer |

We deliberately **diverge** from nashsu in one place: nashsu puts LLM *summaries* in `wiki/sources/` and originals in `raw/sources/`. We put the **originals themselves** in `wiki/sources/` and generate no summary pages. This matches Astro-Han's "don't summarize sources" discipline while keeping nashsu's browsable-sources UX.

## 3. Design overview

```
raw/sources/<type>/<original-binary>   ← immutable original file (PDF/DOCX/MD…), unchanged
raw/extracted/<stem>-<hash>.md         ← INTERNAL STAGING only (preview); not browsable
wiki/sources/<clean-name>.md           ← verbatim extracted original (promoted on confirm), type: source
wiki/{entities,concepts,synthesis,comparisons}/...  ← LLM-derived pages citing wiki/sources/
```

- **Stage on preview:** extraction writes to `raw/extracted/` (internal staging, as today) so unconfirmed imports never leak into the wiki tree.
- **Promote on confirm:** for each confirmed source, read its staged text, derive a clean name, prepend frontmatter, write verbatim to `wiki/sources/<clean-name>.md`, and record that path in `.app/source-index.json`.
- **Compile-protect `wiki/sources/`:** compile reads originals (to cite them) but can never create/modify/delete under `wiki/sources/`.
- **No new frontend commands:** `scan_wiki` / `read_page` / `WikiTree` are path-agnostic under `wiki/`, and `WikiPageType::infer` already maps `type: source` + `sources/` path → `Source`. Source pages appear and render automatically.

## 4. Data model & paths

### 4.1 Confirm-time promotion (new)

On `confirm_import`, before/within `record_confirmed_sources` (`import_service.rs:573-666`), for each confirmed entry that has an extracted text artifact:

1. Read staged text from `raw/extracted/<stem>-<hash>.md`.
2. Compute clean name from the **original source filename** (stem), sanitized: kebab/snake-safe, CJK-preserving, strip the extraction hash; collision-resolve against existing `wiki/sources/*.md` by appending `-2`, `-3`, …
3. Prepend frontmatter:
   ```yaml
   ---
   type: source
   sources: ["<original-source-filename>"]
   title: <derived title>
   ---
   ```
   followed by the verbatim extracted text.
4. Write to `wiki/sources/<clean-name>.md` (path-safety: under `wiki_dir`, no traversal/symlinks).
5. Record `wiki/sources/<clean-name>.md` (not the staging path) as the entry's extracted-text artifact in `.app/source-index.json`.
6. Delete the staged `raw/extracted/<stem>-<hash>.md` file (staging is transient).

`.app/source-index.json` shape is unchanged: `{ "sources": { "<archived_path>": [<artifact>…] } }`; only the artifact string value changes prefix (`raw/extracted/…` → `wiki/sources/…`).

### 4.2 Source originals are wiki pages

Because they live under `wiki/`, source originals automatically:
- appear in the file tree (`scan_wiki` walks all of `wiki/`),
- are readable in `MarkdownReader` (`read_page`),
- become graph nodes (PRD-GRAPH-001 page-level nodes),
- are linted and indexed like any wiki page.

## 5. Backend changes (Rust)

### 5.1 Extraction staging (keep, relabel) — `extraction_service.rs`
- `write_extracted_text` (line 1046): keep writing to `raw/extracted/<stem>-<hash>.md` during **preview**. No path change needed here — staging stays `raw/extracted/`. Update the validation comment to clarify it is now staging-only.

### 5.2 Confirm promotion + source delete/replace (new logic) — `import_service.rs`
- In `confirm_import` (line 573), after copying sources to `raw/sources/`, add a promotion step that performs §4.1.
- `record_confirmed_sources` (line 207): record the promoted `wiki/sources/…` path as the extracted-text artifact.
- `validate_artifact_paths` (line ~800-814): the `raw/extracted/` prefix check must also accept `wiki/sources/` so the **import** delete/replace flows (`apply_source_delete` line 235, `apply_source_replace` line 284) can manage promoted artifacts.
- **Actor distinction (resolves the apparent contradiction with §5.4):** `wiki/sources/` is protected from **compile/LLM** writes only. **Import** delete/replace is a separate, **user-initiated** actor: when a user deletes a source, its promoted `wiki/sources/` page is cascade-removed (mirroring nashsu); when a user replaces a source, the old promoted page is removed and a new one promoted. This is intended behavior — deleting an imported source should remove its browsable page. These import delete/replace operations are **high-risk, destructive** and must run through the existing Git-checkpoint + user-confirm path (CLAUDE.md); they are NOT the same as compile touching `wiki/sources/`, which remains forbidden.

### 5.3 Compile source enumeration — `compile_service.rs`
- `extracted_markdown_files` (line 50, filter at line 75): change filter from `raw/extracted/`-only to admit **both** `wiki/sources/*.md` and legacy `raw/extracted/*.md` (so old projects still compile and all originals are read).
- `populate_workspace` (lines 237-251): the per-file copy loop into `workspace/raw/extracted/` must **copy ONLY entries whose source-index path starts with `raw/extracted/`** and **skip `wiki/sources/` entries**. Reason: `wiki/sources/` entries are already brought into the workspace by `copy_tree(&context.wiki_dir,…)` (line 252) at `workspace/wiki/sources/`; copying them again into `workspace/raw/extracted/` would **double-inject** the same original into the LLM prompt. Legacy `raw/extracted/` entries are not under `wiki/`, so they still need the explicit copy.

### 5.4 Compile protection for `wiki/sources/` (core safety) — `compile_service.rs`
Three defense-in-depth points:
1. `manifest_from_workspace` (line 264, file/deletion collection loop ~270-290): **skip** any path under `wiki/sources/` when collecting files, and exclude `wiki/sources/` from deletion computation. (Agent-CLI path.)
2. `validate_manifest` (line 318) / `is_safe_wiki_markdown` (line 673): **reject** any file or deletion path under `wiki/sources/` with error `COMPILE_PROTECTED_PATH`. (BYOK path too.) This is the load-bearing guard.
3. Prompt instruction (see §7).

### 5.5 Compile prompts — `compile_service.rs`
- `provider_prompt` (line 24-25): remove "at minimum one source page in `wiki/sources/` for EACH extracted source … structure and summarize that source". Replace with the new citation model (§7).
- `compile_prompt` (line 300-306): remove the per-source-summary mandate and the "every file in `raw/extracted/`" phrasing; restate the new model.

### 5.6 Skeleton — `project_service.rs`
- `ensure_skeleton` (line 94-116): **keep** `wiki/sources/` creation (user requirement). Keep `raw/extracted` as staging. No dir removed.

### 5.7 Tests
Update all tests hardcoding `raw/extracted/` as the confirmed location (`extraction_service.rs` ~17 tests, `import_service.rs` confirm/index tests ~1139/1647/1851, `compile_service.rs` 692-805) to reflect promotion to `wiki/sources/`. Add new tests: promotion writes clean name + frontmatter; collision suffixing; compile rejects a manifest that writes/deletes `wiki/sources/`; compile still reads legacy `raw/extracted/` entries.

## 6. Frontend changes (React/TS) — minimal

- `wikiStore.ts`: after `confirmImport` resolves, re-run `scan()` so the new `wiki/sources/` pages appear immediately.
- `WikiTree.tsx`: no structural change (folders auto-render). **Optional polish:** give `type: source` pages a distinct Lucide icon (e.g. `FileText`) via the existing `page.pageType` hook (line 66).
- `MarkdownReader.tsx`: no change for text. **Known limitation:** no project-relative image rewriting exists today (assets only resolve if Tauri-served/absolute); PDF→Markdown images may not render. Not a regression (same as today). Tracked as a follow-up, out of scope here.

## 7. Agent prompt optimization (wiki-ingest SKILL + compile prompts)

Rewrite `src-tauri/templates/skills/wiki-ingest/SKILL.md` and the `compile_prompt`/`provider_prompt` to encode the new model. Grounded in the reference projects:

- **Sources are originals, not summaries:** "`wiki/sources/` contains the verbatim extracted originals of each imported source (Markdown as-is, or PDF→Markdown). These are import-owned. **Never create, modify, or delete any file under `wiki/sources/`.** Read them as authoritative."
- **Derived pages only:** "Generate `entities/`, `concepts/`, `synthesis/`, `comparisons/` pages that synthesize **across** sources. Do **not** write one page per source."
- **Name by concept:** "Name each derived page after the concept it covers, not after any source filename." (Astro-Han)
- **Cite sources two ways:** (a) frontmatter `sources: ["<original-filename>"]` on every derived page (machine join key for graph, per nashsu); (b) a human-readable `> Sources:` line with markdown links to `../sources/<clean-name>.md` (relative within `wiki/`) or `[[sources/<clean-name>]]`.
- **Cascade update:** "After writing a page, update every other page whose content is materially affected by the new information; refresh `index.md`, `overview.md`, append `log.md`." (Astro-Han)
- **Immutability / safety:** "Work only inside the compile workspace. Never delete pages. `wiki/sources/` is off-limits for writes."

Update `wiki-lint/SKILL.md` citation wording: `missing_source` should say "should cite a source in `wiki/sources/`" (was `raw/`).

Update `schema.md` templates (all 5 project types): keep the `source` row but redefine as "imported verbatim original in `wiki/sources/` (import-owned; derived pages cite, never copy/summarize)".

## 8. Hard-boundary compliance (CLAUDE.md)

- **Path safety:** promotion writes via `FileStore`/`ProjectContext` (project-bound, traversal/symlink-rejected), consistent with existing patterns.
- **Immutability of raw/:** `raw/sources/` untouched; `raw/extracted/` used read-then-delete as transient staging. No new writes to `raw/`.
- **Compile writes only `wiki/`:** unchanged; the new guard further restricts the `wiki/sources/` subtree.
- **No DB, no API keys:** unaffected.
- **Git checkpoints:** promotion itself is additive (creates new files under `wiki/`), running inside `confirm_import`. **Source delete/replace of promoted `wiki/sources/` pages is destructive** (removes wiki content) and must go through the existing high-risk checkpoint + user-confirm path (§5.2). Compile checkpoint logic is unchanged.

## 9. Backward compatibility & migration

- **Legacy `raw/extracted/` entries:** compile still reads them as input (§5.3), but they are **not** browsable in the UI (and we won't make them so). They remain compile-input-only until the user re-imports, at which point promotion moves them to `wiki/sources/`.
- **Old `wiki/sources/` summary pages** (from prior compiles): left as-is; compile won't touch them (protected). Users may delete manually. Not a blocker.
- **Old `schema.md`** in existing projects: still has the old `source` definition; harmless. New projects get the updated template.
- **No automatic migration** of existing `raw/extracted/` files into `wiki/sources/` (data-safety: don't move user data unprompted). Re-importing promotes them.
- **Mixed projects** (some sources promoted to `wiki/sources/`, some legacy entries still in `raw/extracted/`): only `wiki/sources/` originals are browsable in the UI; legacy `raw/extracted/` entries remain compile-input-only until re-imported. Compile reads both (§5.3).

## 10. Testing strategy

- **Unit (Rust):**
  - Promotion: clean-name derivation (ASCII, CJK, with/without hash), collision suffixing, frontmatter injection, verbatim body preserved.
  - `source-index.json` records `wiki/sources/…` after confirm.
  - Compile protection: `validate_manifest` rejects `wiki/sources/` writes and deletions; `manifest_from_workspace` excludes them; compile still succeeds reading originals.
  - Compile reads both `wiki/sources/` and legacy `raw/extracted/`.
- **Existing tests:** update path expectations (§5.7).
- **Integration/manual:** import 1 Markdown + 1 PDF → confirm → verify `wiki/sources/` has both verbatim, browsable, readable; compile → verify `wiki/entities` etc. exist and cite `wiki/sources/`, and **no** new `wiki/sources/` files were created or modified by compile; graph shows source + concept nodes.
- **CLAUDE.md checklist:** `npm run test`, `npm run lint`, no `console.log`, imports valid; then the two-subagent review.

## 11. Out of scope (YAGNI)

- No `raw/articles/` restructure (that was the rejected Option 2).
- No `source-index.json` schema change (only the artifact path prefix).
- No automatic migration of legacy `raw/extracted/`.
- No chat / HTML-export changes (chat already reads wiki pages; will naturally include source pages).
- No project-relative image/asset rewriting in the reader (tracked separately).
- No graph source-overlap edge weighting from `sources[]` (enhancement; current wikilink + multi-signal graph suffices for v1).

## 12. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Compile overwrites imported originals | Triple guard: manifest skip + `validate_manifest` reject + prompt (§5.4) |
| Preview leaks unconfirmed files into wiki tree | Keep `raw/extracted/` as staging; promote only on confirm (§4.1) |
| Filename collisions clobber an existing source | Counter-based `-2/-3` suffixing at promotion time (§4.1) |
| Legacy projects break | Compile still reads `raw/extracted/` (§5.3); no auto-migration (§9) |
| CJK filenames mishandled | Reuse existing `sanitize_filename` + CJK tests (§10) |
| Large source bloats wiki/ graph perf | Source pages are normal wiki pages; graph already targets 200-500 pages (PRD §11.1); monitor |

## 13. Acceptance criteria

1. After importing a Markdown and a PDF and confirming, both appear under `wiki/sources/` with verbatim content and are readable in the app, **without** compiling.
2. Compiling produces `entities/concepts/synthesis` pages that cite `wiki/sources/` originals; compile creates/modifies/deletes **zero** files under `wiki/sources/`.
3. `.app/source-index.json` records `wiki/sources/…` paths.
4. Existing legacy projects with `raw/extracted/` entries still compile.
5. All CLAUDE.md checklist items pass.
