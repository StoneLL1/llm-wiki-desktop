# Non-Import P0/P1 Fix Code Review

Date: 2026-07-07
Scope: review only; no code implementation. Reviewed current working tree for the non-import Compile, Chat/Retrieval/Citations, and Lint/Schema/Skill fixes.

未发现 blocker. The main P0 trust-chain work is substantially improved, but several P1/P2 follow-ups remain before I would call the whole P0/P1 set fully closed.

## Findings

### P1 - Compile semantic validation does not enforce page type vs target path

File and line: `src-tauri/src/services/compile_service.rs:477`, `src-tauri/src/services/compile_service.rs:600`, `src-tauri/src/services/compile_service.rs:615`

Problem: `validate_plan` checks safe paths, sources, merge targets, and structural-only plans, but it does not verify that `CompilePlanItem.pageType` matches the target directory. `validate_manifest_semantics` only checks that frontmatter `type` is known and equals the accepted plan. A plan can therefore create `wiki/concepts/foo.md` with `pageType: entity`, and a manifest with `type: entity` will pass compile validation.

Why important: This lets compile write schema/path mismatches that local lint may later warn about, but the compile no-write gate should reject them before the wiki is mutated. It weakens P0-1/P1-3's "semantic validation gate" claim.

Suggested fix: Add a shared `expected_page_type_for_path` check in compile validation. Validate both plan items and manifest files against path-derived expectations for `entities`, `concepts`, `synthesis`, `comparisons`, `queries`, and structural files.

Suggested test: Add compile tests where a plan/manifest attempts `wiki/concepts/x.md` with `entity`, and `wiki/entities/x.md` with `concept`; both should fail before apply and leave project files unchanged.

### P1 - Agent read-more/path citations cannot become persisted citations

File and line: `src-tauri/src/services/chat_service.rs:477`, `src-tauri/src/services/chat_service.rs:528`, `src-tauri/templates/skills/wiki-query/SKILL.md:14`

Problem: The Agent prompt allows reading additional wiki pages, while the `wiki-query` Skill says unnumbered read-more pages should be cited by exact project-relative path. But `parse_model_citations` only parses `[S#]` markers from the numbered source list. If the Agent uses a page it read directly, that evidence cannot be persisted as a `ChatCitation`.

Why important: P0-2 is fixed for numbered retrieval sources, but Agent-specific evidence is still either lost as `[unverified]` or left as plain prose. Saved query `sources:` then omits evidence the Agent may actually have used.

Suggested fix: Either require Agent answers to cite only numbered sources and mark all read-more claims unverified, or add a path-style citation parser/allowlist for `wiki/**/*.md` pages read by Agent. Keep invalid paths out of persisted citations.

Suggested test: Fake an Agent answer that cites `wiki/concepts/extra.md` after read-more access; verify it either persists a validated citation or is explicitly flagged as unverified in UI/storage.

### P1 - Invalid/no citation and unverified diagnostics are stored but not surfaced safely in UI

File and line: `src-tauri/src/commands/chat_commands.rs:299`, `src/features/chat/MessageContent.tsx:65`, `src/features/chat/ChatView.tsx:326`, `src/components/app/RightContextPanel.tsx:122`

Problem: Backend records `invalidCitationIds` and `hasUnverified` in retrieval diagnostics, but the Chat UI does not display either. `MessageContent` rewrites `[S9]` to `citation://S9`; when it is not a known citation it falls through as a normal anchor instead of rendering a warning/plain marker. Save remains enabled with no visible warning when an answer has invalid markers, `[unverified]`, or no parsed citations.

Why important: This is a trust UI gap for P0-2. Storage avoids persisting bad citations, but users are not told that model citation markers were invalid or unsupported before saving a query page.

Suggested fix: Render a compact warning state for `hasUnverified`, `invalidCitationIds`, and zero-citation assistant answers. Invalid citation markers should render as non-clickable text or a warning badge, not an external `citation://` link. Saved query pages should carry an explicit "No citations" / "Unverified claims" note when applicable.

Suggested test: Frontend tests for an assistant message with `[S9]`, `[unverified]`, and no citations; assert visible warning text/badges and no clickable `citation://S9` link.

### P2 - Lint index regeneration can still add source/query pages to the main index

File and line: `src-tauri/src/services/lint_service.rs:1174`

Problem: `regenerate_index` lists all Markdown files under `wiki/` and skips only `wiki/index.md` and `wiki/log.md`. It will include `wiki/sources/**`, `wiki/queries/**`, and `wiki/overview.md` as normal index entries.

Why important: This earlier audit risk remains in the lint auto-fix path. Regenerating the main navigation index can pollute it with source originals or saved query records.

Suggested fix: Define and enforce an index inclusion policy. Exclude `wiki/sources/**` and `wiki/queries/**` by default, and decide whether `overview.md` is excluded or grouped separately.

Suggested test: Add a regenerate-index fixture containing `wiki/sources/source.md`, `wiki/queries/q.md`, `wiki/concepts/c.md`, `wiki/overview.md`, and verify only intended pages appear.

### P2 - Deep-lint Agent profile is still less explicit than Chat

File and line: `src-tauri/src/services/agent_service.rs:361`, `src-tauri/src/commands/lint_commands.rs:120`

Problem: Deep lint runs in a temp candidate workspace, which protects project files, but `lint_invocation` does not mirror Chat's explicit read-only profile. Codex lint is invoked as `codex exec -` without `--ephemeral`, `--ignore-rules`, `--sandbox read-only`, `--skip-git-repo-check`, or `-C`.

Why important: Lint is an audit flow and should be deterministic/read-only. The current temp workspace limits damage, but route behavior can still drift with Agent defaults or user config.

Suggested fix: Give lint Agent invocations explicit read-only/non-interactive args. For Codex, mirror Chat's `--ephemeral --ignore-rules --sandbox read-only --skip-git-repo-check -C <workspace> -`. For Claude, use a read-only allowlist if the CLI supports it.

Suggested test: Add invocation-profile tests asserting lint uses explicit read-only/ephemeral flags for supported agents.

## Audit Item Status

| Item | Status | Notes |
|---|---|---|
| P0-1 Compile plan + semantic gate | Partial | Plan exists in BYOK and Agent, manifest validation is no-write, but path/type semantic mismatch can pass. |
| P0-2 Chat citations are model-used evidence | Partial | Numbered citations are parsed from model output; Agent read-more/path citations and UI diagnostics need follow-up. |
| P0-3 Chat retrieval/prompt split | Done | BYOK/Agent prompts are split; budgeted planner includes index, pinned page, keyword hits, graph, source overlap, and diagnostics. |
| P1-1 `wiki-ingest` decision rubric | Done | Skill includes create/update/merge/see-also/conflict/cascade rules. |
| P1-2 Compile instruction drift | Done | Shared instruction builder feeds BYOK/Agent prompts; tests cover Skill/prompt clauses. |
| P1-3 Local schema/source lint | Partial | Missing type/bad type/missing sources/source section/bad source path are mostly local; compile still lacks path/type rejection. |
| P1-4 `wiki-query` / read-only API | Partial | `wiki-query` Skill exists; read-only API remains future work by design. |
| P1-5 Graph/source-overlap retrieval | Done | Expansion is implemented, budgeted, deduped, and filters source/query/structural pages. |

## Verification

- `git status --short`: worktree was already dirty before review; no unrelated files reverted.
- `npm run test`: passed, 50 files / 362 tests.
- `npm run lint`: passed with 0 warnings.
- `cargo test --lib --no-default-features` in `src-tauri`: passed, 456 tests.
- `rg` was unavailable due Windows "Access is denied"; PowerShell fallback console scan found no `console.log` under `src` or `src-tauri/src`.

## Suggested Next Fix Order

1. Compile path/type validation and tests.
2. Agent read-more/path citation contract plus UI warnings for invalid/unverified/no-citation answers.
3. Deep-lint Agent read-only profile hardening.
4. Lint index regeneration inclusion policy for source/query pages.
