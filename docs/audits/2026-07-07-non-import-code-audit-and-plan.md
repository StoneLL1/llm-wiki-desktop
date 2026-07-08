# Non-Import LLM Wiki Code Audit And Execution Plan

Date: 2026-07-07
Scope: code-level audit for non-import surfaces only: Skill/prompt design, wiki compile, BYOK/Agent boundaries, Chat/query/citations, Lint/schema/source traceability, Search/retrieval/graph/index, Agent/API/Skill extensibility, safety boundaries, and execution/test planning.

Out of scope for the main plan: import pipeline redesign. Import-related observations are isolated in the appendix.

## P0 Findings

### P0-1. Compile Has No Auditable Plan Stage And Semantic Validation Gate

Current code evidence:

- `src-tauri/src/services/compile_service.rs:24-45` builds one BYOK prompt that directly asks for a `{files, deletions, summary}` manifest and inlines workspace files.
- `src-tauri/src/services/compile_service.rs:325-333` builds the Agent prompt as a direct "compile this wiki" write task.
- `src-tauri/src/services/compile_service.rs:345-385` validates protected paths, unsafe/duplicate paths, and required `index.md`/`overview.md`/`log.md`, but does not parse each generated page's frontmatter, `sources`, `type`, `> Sources:`, source mirror risk, or schema conformance.

External evidence:

- Karpathy gist: [gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f), short quote: "touch 10-15 wiki pages".
- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "Two-Step Chain-of-Thought Ingest".
- Astro-Han Skill: [SKILL.md](https://raw.githubusercontent.com/Astro-Han/karpathy-llm-wiki/main/SKILL.md), short quote: "Always both steps".

Why this is a gap:

External implementations make the decision layer explicit: analyze first, decide merge/create/update/conflict/cascade, then write. Current code jumps straight to generated files, so the app cannot review or reject bad decisions before content generation.

Risk if not fixed:

Compile can produce source mirrors, weak one-page-per-source outputs, missing source traceability, or schema-invalid pages while passing validation. A user sees a successful compile, but graph/lint/chat later operate on unreliable structure.

Recommended fix:

Introduce `CompilePlan` before `CompileManifest`.

- Add DTOs in `src-tauri/src/models/compile.rs`: `CompilePlan`, `CompilePlanItem`, `CompileAction::{Create,Update,Merge}`, `CompilePageType`, `source_ids`, `affected_existing_pages`, `reason`, `risk_flags`.
- Add `CompileService::validate_plan(context, plan)` to reject protected paths, empty source IDs, source mirrors, and updates to structural files without a reason.
- Strengthen `CompileService::validate_manifest` into semantic validation: parse frontmatter, require non-empty `sources` on derived pages, verify source references exist in `wiki/sources/` or allowed legacy extracted inputs, require `type`, require a human-readable source section, and ensure structural pages mention touched pages.
- Treat failed plan/manifest validation as no-write failure.

Acceptance criteria:

- A generated derived page without `sources` is rejected before apply.
- A generated page under `wiki/sources/` is still rejected.
- A plan item with `action=merge` must name an existing target page.
- A plan item with no source IDs fails validation.
- Compile cannot pass with only `wiki/index.md`, `wiki/overview.md`, and `wiki/log.md`.

Suggested tests:

- Rust unit tests in `compile_service.rs` for semantic manifest failures.
- DTO serialization tests in `models/compile.rs`.
- A fake BYOK/Agent compile test where plan passes but manifest fails, confirming no project file write.

### P0-2. Chat Citations Are Retrieval Hits, Not Model-Used Evidence

Current code evidence:

- `src-tauri/src/services/chat_service.rs:21-24` states citations are retrieved pages and "never parsed from model output".
- `src-tauri/src/services/chat_service.rs:216-228` converts retrieval hits into citations before prompting the model.
- `src-tauri/src/commands/chat_commands.rs:159-167` clones citations before route selection and model execution.
- `src-tauri/src/commands/chat_commands.rs:282-287` persists those cloned citations on the assistant message.
- `src-tauri/src/services/chat_service.rs:333-363` saves `wiki/queries/` sources from `answer.citations`, so saved query pages inherit retrieval hits rather than actual citations.

External evidence:

- Karpathy gist: [gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f), short quote: "answer with citations".
- nashsu Skill: [SKILL.md](https://raw.githubusercontent.com/nashsu/llm_wiki_skill/main/SKILL.md), short quote: "Quote the path".

Why this is a gap:

The UI currently says "these are sources" when the truth is "these were top retrieval hits". If the model ignores one, cites another page by reading filesystem, or answers "insufficient context", the saved citations still show the pre-model top-k.

Risk if not fixed:

The product's trust loop breaks. Users can click a citation and fail to find evidence for the answer, or save a query page whose frontmatter claims sources the answer did not use.

Recommended fix:

Use numbered source IDs and parse model-used citation IDs.

- Add `ChatSourceRef { id: "S1", path, title, excerpt, is_pinned }`.
- Prompt BYOK models to answer with `[S1]` markers and to mark unsupported claims as `[unverified]`.
- Prompt Agent route similarly, but allow Agent to read more pages and require any extra page to be cited as a path.
- Add `ChatService::extract_used_citations(answer_text, available_sources)` and persist only parsed citations.
- Keep retrieval hits separately as `retrievalHits` for debugging/UI, not as `citations`.

Acceptance criteria:

- If model answer cites `[S2]` only, assistant `citations` contains only S2.
- If answer cites no sources, UI and saved query page show no citations or an explicit unverified warning.
- Pinned current page is only a citation if cited.
- `wiki/queries/*.md` frontmatter `sources` matches parsed model-used citations.

Suggested tests:

- Unit tests for citation parser: duplicate markers, invalid IDs, no citations, path-style Agent citations.
- Command-level fake BYOK test proving retrieval hits are not blindly persisted.
- Save-to-wiki test verifying frontmatter source list is model-used only.

### P0-3. Chat Retrieval And Prompting Are Fixed Top-K And Mixed Across BYOK/Agent

Current code evidence:

- `src-tauri/src/services/chat_service.rs:17-19` hardcodes `RETRIEVAL_LIMIT = 6`, `EXCERPT_CHARS = 1200`, and `HISTORY_TURNS = 8`.
- `src-tauri/src/services/chat_service.rs:249-264` uses one prompt for both Agent and BYOK, telling Agent it can read `wiki/` and telling non-filesystem models to fall back to provided context.
- `src-tauri/src/services/search_service.rs:629-657` retrieves top keyword hits with bounded excerpts only.
- `src-tauri/src/commands/chat_commands.rs:191-269` sends the same assembled prompt to Agent or BYOK.

External evidence:

- Karpathy gist: [gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f), short quote: "reads the index first".
- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "Budget Control".
- Astro-Han Skill: [SKILL.md](https://raw.githubusercontent.com/Astro-Han/karpathy-llm-wiki/main/SKILL.md), short quote: "Read `wiki/index.md`".

Why this is a gap:

BYOK cannot read files; Agent can. A shared prompt hides that capability boundary. Fixed top-k excerpts also miss the external pattern: index-first navigation, selective full-page reads, graph expansion, and provider-aware context budgets.

Risk if not fixed:

BYOK can hallucinate from incomplete snippets, while Agent can produce answers using pages not represented in citations. Long chats can crowd out relevant wiki content because history and sources are not budgeted together.

Recommended fix:

- Split `assemble_prompt` into `assemble_byok_prompt` and `assemble_agent_prompt`.
- Add a retrieval planner: index page summary, top keyword hits, optional graph expansion, and full-content inclusion under a token/char budget.
- Use `LlmProviderConfig.context_window` for BYOK budget allocation.
- Store retrieval diagnostics with each message: route, budget, selected pages, expanded pages, omitted pages.

Acceptance criteria:

- BYOK prompt states no filesystem/tool access.
- Agent prompt instructs index-first and read-more behavior.
- Retrieval respects a budget and documents omitted pages.
- Conversation history is trimmed by budget, not only turn count.

Suggested tests:

- Prompt snapshot tests for BYOK vs Agent.
- Retrieval planner tests for pinned page, index inclusion, graph-expanded page, and budget truncation.
- Regression test that global Search remains keyword-only and does not call LLM/Agent.

### P0-4. Deep Lint Is Under-Evidenced And Agent Severity Is Trusted

Current code evidence:

- `src-tauri/src/services/lint_service.rs:21` sets `DEEP_LINT_EXCERPT_CHARS` to 240.
- `src-tauri/src/services/lint_service.rs:547-554` asks for issue types and severity but gives no severity rubric.
- `src-tauri/src/services/lint_service.rs:583-588` sends only truncated page body excerpts.
- `src-tauri/src/services/lint_service.rs:625-632` copies Agent-provided severity directly into `LintIssue`.
- `src-tauri/templates/skills/wiki-lint/SKILL.md:19-25` defines JSON fields but no error/warning/info criteria.

External evidence:

- Karpathy gist: [gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f), short quote: "contradictions between pages".
- Astro-Han Skill: [SKILL.md](https://raw.githubusercontent.com/Astro-Han/karpathy-llm-wiki/main/SKILL.md), short quote: "Deterministic Checks" and "Heuristic Checks".

Why this is a gap:

Contradictions, stale claims, missing source evidence, and schema mismatches are page-level or cross-page judgments. A 240-character excerpt and vague severity labels are not enough evidence for dependable results.

Risk if not fixed:

Deep lint can miss real contradictions and overstate minor issues as errors. Users may trust a green or red Lint state that is mostly a prompt artifact.

Recommended fix:

- Increase deep lint evidence to section-aware excerpts or 800-1200 chars per candidate page.
- Feed local lint baseline into the deep lint prompt and tell Agent not to duplicate deterministic issues.
- Add severity rubric in `wiki-lint/SKILL.md` and `build_deep_lint_prompt`.
- Normalize Agent severity locally: e.g. schema/source contradiction can be at most `warning` unless evidence contains incompatible claims and paths.

Acceptance criteria:

- `wiki-lint` Skill defines concrete error/warning/info criteria.
- Deep lint prompt includes enough content for multi-page comparison.
- Agent issues without evidence are downgraded or rejected.
- Local deterministic issues appear as baseline context, not repeated Agent work.

Suggested tests:

- Prompt snapshot test confirming rubric and baseline presence.
- Parser tests for invalid severity, missing evidence, and unknown paths.
- Fixture wiki with duplicate topics and contradictions to validate deep lint prompt contains both relevant bodies.

### P0-5. Compile Agent Write Profile Allows Bash Without A Controlled Tool Surface

Current code evidence:

- `src-tauri/src/services/agent_service.rs:177-196` pre-approves `Edit Write Read Bash` for Claude compile.
- `src-tauri/src/services/agent_service.rs:187-193` explicitly notes prompt-injected source content could run arbitrary shell and that system-level commands are not contained on Windows.
- `src-tauri/src/services/agent_service.rs:204-214` runs Codex compile with `workspace-write` in a temp workspace.
- `src-tauri/src/services/agent_service.rs:983-999` restricts compile Agent execution to a candidate workspace, which is a good project-level boundary but not a system-command boundary.

External evidence:

- nashsu Skill: [SKILL.md](https://raw.githubusercontent.com/nashsu/llm_wiki_skill/main/SKILL.md), short quote: "localhost-only".
- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "Local HTTP API + MCP".

Why this is a gap:

The app currently relies on temp workspace plus manifest validation after the Agent runs. That protects final wiki writes, but it does not fully constrain shell side effects during the run. External implementations move read/search/graph interaction behind a narrow API surface for Agent use.

Risk if not fixed:

Malicious or prompt-injected source text can ask the Agent to run shell commands. Even if bad output is rejected, the shell command may already have affected the host environment.

Recommended fix:

Short term:

- Keep temp workspace and manifest validation.
- Add explicit warning/telemetry in Agent compile logs when Bash-enabled profile is used.
- Add tests asserting candidate workspace validation and no direct project writes before manifest apply.
- Prefer non-Bash file write tool profiles where supported.

Medium term:

- Build a controlled compile writer: Agent returns plan/manifest only; Rust applies writes.
- Add a read-only API surface for search/read/graph/lint summary so Agent does not need raw filesystem access for query/review flows.

Acceptance criteria:

- Compile never applies files unless plan and manifest pass validation.
- Agent compile logs show the effective write profile and workspace.
- A future "safe compile" profile can run without Bash on supported CLIs.

Suggested tests:

- Invocation profile tests for allowed tools and sandbox mode.
- Candidate workspace rejection tests for paths outside temp workspace.
- Fake Agent test that emits bad path and shell-like content; verify no project write.

## P1 Findings

### P1-1. `wiki-ingest` Skill Lacks Merge/Create/Conflict Decision Rubric

Current code evidence:

- `src-tauri/templates/skills/wiki-ingest/SKILL.md:12-15` protects `wiki/sources/` and rejects one-source-one-page outputs.
- `src-tauri/templates/skills/wiki-ingest/SKILL.md:33-40` mentions cascade and no delete.
- It does not define the external decision rubric: same thesis merge, new concept create, cross-topic see-also, conflict annotation, and cascade scan order.

External evidence:

- Astro-Han Skill: [SKILL.md](https://raw.githubusercontent.com/Astro-Han/karpathy-llm-wiki/main/SKILL.md), short quote: "Same core thesis".
- Karpathy gist: [gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f), short quote: "updates index".

Why this is a gap:

The Skill says what not to do, but it does not tell an Agent how to choose between updating an existing page, merging overlapping pages, creating a new concept page, or annotating a conflict. External Skills make those choices explicit.

Risk if not fixed:

Even after adding `CompilePlan`, weak Skill rules can produce plans that fragment the wiki, duplicate concepts, or hide source conflicts inside prose.

Recommended fix:

Add a "Decision Rules" section to `wiki-ingest/SKILL.md` and reuse it from the compile instruction builder. Acceptance: prompt tests prove these rules appear in Agent and BYOK routes.

Acceptance criteria:

- Skill defines create/update/merge/see-also/conflict/cascade rules.
- Same rules appear in BYOK and Agent compile prompts.
- A plan test exercises same-thesis merge and new-concept create.

Suggested tests:

- Prompt builder snapshot tests.
- CompilePlan validation fixture with one merge and one create decision.

### P1-2. Compile Instructions Exist In Multiple Places And Can Drift

Current code evidence:

- BYOK instruction text lives in `compile_service.rs:24-27`.
- Agent instruction text lives in `compile_service.rs:325-341`.
- Skill instruction text lives in `src-tauri/templates/skills/wiki-ingest/SKILL.md:8-40`.

External evidence:

- alirez Skill: [SKILL.md](https://raw.githubusercontent.com/alirezarezvani/claude-skills/main/engineering/llm-wiki/skills/llm-wiki/SKILL.md), short quote: "same content works".
- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "Local HTTP API + MCP Server + AI Agent Skill".

Why this is a gap:

The project now has three instruction surfaces that must remain semantically identical. As soon as one path changes and another does not, BYOK and Agent outputs diverge in ways users cannot diagnose.

Risk if not fixed:

Future fixes to source traceability, conflict annotation, or cascade behavior may land only in one route. The same project can compile differently depending on route.

Recommended fix:

Create a shared compile instruction builder module or constants used by BYOK prompt, Agent prompt, and Skill template tests. Acceptance: one test asserts all critical clauses are present across routes.

Acceptance criteria:

- Critical clauses are represented once in code or generated from a shared source.
- Tests fail if BYOK/Agent/Skill lose `wiki/sources` protection, source citations, cascade, or merge/create language.

Suggested tests:

- Unit tests that inspect prompt strings.
- Template contract test that reads `src-tauri/templates/skills/wiki-ingest/SKILL.md`.

### P1-3. Schema/Source Traceability Checks Are Too Dependent On Agent Lint

Current code evidence:

- `src-tauri/src/models/lint.rs:25-46` separates local deterministic rules from Agent deep-lint rules; `SchemaMismatch` is Agent-only.
- `src-tauri/src/services/lint_service.rs:45-230` local lint checks dead links, frontmatter presence, source file existence for existing `sources`, orphan pages, duplicate filenames, and path case, but not schema-defined page type/required sections/source-required rules.
- `src-tauri/templates/skills/wiki-lint/SKILL.md:14-17` makes `schema_mismatch` an Agent dimension.

External evidence:

- Astro-Han Skill: [SKILL.md](https://raw.githubusercontent.com/Astro-Han/karpathy-llm-wiki/main/SKILL.md), short quote: "Raw references".
- Karpathy gist: [gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f), short quote: "The schema".

Why this is a gap:

Basic source traceability and page type validity are deterministic file checks. Leaving them to Agent deep lint makes the health signal slower, less repeatable, and dependent on prompt quality.

Risk if not fixed:

Graph joins, saved query sources, and exports can rely on invalid `sources`/`type` metadata while local lint reports no blocking issue.

Recommended fix:

Add local deterministic schema/source checks before Agent lint: validate `type`, required derived-page `sources`, source path existence, structural file index membership, and simple schema section requirements. Acceptance: schema/type mismatch is caught without Agent.

Acceptance criteria:

- A derived page with missing/empty `sources` is reported locally.
- A page with `type: entity` under `wiki/concepts/` is reported or warned locally.
- A missing source path in frontmatter is reported locally.

Suggested tests:

- Local lint fixture tests for wrong type, missing sources, missing source section, and non-existent source reference.

### P1-4. No `wiki-query` / `wiki-chat` Skill Or Read-Only App API Exists Yet

Current code evidence:

- `SPEC/SPEC.md:173-182` defines a Skill system that includes `wiki-query/SKILL.md`.
- `src-tauri/templates/skills/wiki-ingest/SKILL.md:1-4` and `src-tauri/templates/skills/wiki-lint/SKILL.md:1-4` exist as wiki operation Skill templates, but there is no corresponding `src-tauri/templates/skills/wiki-query/SKILL.md` file in the current template tree.
- Agent Chat is internal through `AgentService::chat_invocation` at `src-tauri/src/services/agent_service.rs:257-293`.
- `src-tauri/src/commands/llm_commands.rs:101` only references localhost for Ollama's provider URL; searches for `api/v1`, `wiki-query`, and `wiki-chat` in Rust sources find no wiki read API surface.

External evidence:

- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "`GET /api/v1/health`".
- nashsu Skill: [SKILL.md](https://raw.githubusercontent.com/nashsu/llm_wiki_skill/main/SKILL.md), short quote: "Don't install anything new".

Why this is a gap:

The app has internal Chat, but external Agents do not have a controlled read/search/graph surface. They either read raw files directly or depend on the built-in Chat path.

Risk if not fixed:

Agent extension work will keep accumulating ad hoc filesystem prompts instead of a stable contract. This makes security review and citation semantics harder.

Recommended fix:

First add a `wiki-query` Skill for internal Agent behavior. Later add a token-protected `127.0.0.1` read-only API with health/projects/search/read/graph/lint summary. Do not add write endpoints in the first batch.

Acceptance criteria:

- `wiki-query/SKILL.md` exists and requires index-first, source-numbered citations, read-only behavior, and no source mutation.
- API design doc exists before code if localhost API is pursued.
- No write endpoints are introduced in the first API phase.

Suggested tests:

- Skill template contract test.
- API path traversal/token tests once implemented.

### P1-5. Retrieval Does Not Use Graph Expansion Or Source Overlap

Current code evidence:

- `src-tauri/src/services/search_service.rs:629-657` retrieves keyword top hits and excerpts.
- `src-tauri/src/services/graph_service.rs:37-73` already builds wikilink and shared-tag edges, but Chat retrieval does not consume graph topology.

External evidence:

- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "Graph Expansion".
- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "Source overlap".

Why this is a gap:

The graph already computes useful page relationships, but Chat retrieval ignores them. Keyword top hits alone can miss adjacent synthesis/comparison pages that are highly relevant to a question.

Risk if not fixed:

Users get narrower answers than the wiki can support, especially for conceptual questions whose best evidence is one hop away from the keyword hit.

Recommended fix:

After citation provenance is fixed, allow retrieval planner to expand from seed hits through graph neighbors under budget. Do not introduce vector DB as the first step.

Acceptance criteria:

- Retrieval planner can add a graph neighbor under budget.
- Expanded pages are marked as such in retrieval diagnostics.
- Plain Search remains unaffected and keyword-only.

Suggested tests:

- Fixture graph where a relevant neighbor is included only through graph expansion.
- Test that expansion is skipped when budget is exhausted.

### P1-6. Codex Deep-Lint Agent Profile Is Less Isolated Than Compile/Chat

Current code evidence:

- `src-tauri/src/services/agent_service.rs:361-387` builds `lint_invocation`; Codex receives only `["exec", "-"]` plus `cwd`.
- `src-tauri/src/services/agent_service.rs:204-214` gives compile Codex explicit `--ephemeral`, `--sandbox workspace-write`, `--skip-git-repo-check`, and `-C`.
- `src-tauri/src/services/agent_service.rs:283-293` gives Chat Codex explicit `--ephemeral`, `--ignore-rules`, `--sandbox read-only`, `--skip-git-repo-check`, and `-C`.
- `src-tauri/src/services/agent_service.rs:429-433` shows the same weaker Codex profile pattern for HTML export, which is outside the main scope but shares the risk.

External evidence:

- nashsu Skill: [SKILL.md](https://raw.githubusercontent.com/nashsu/llm_wiki_skill/main/SKILL.md), short quote: "Read-only by default".
- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "AI Agent Skill".

Why this is a gap:

The normal Chat Codex route is deliberately read-only, and compile Codex is deliberately workspace-write. Deep lint should be at least as explicit, because lint is a review operation and should not inherit ambient repo rules or permissions by accident.

Risk if not fixed:

Deep lint can behave differently from Chat/compile depending on Codex defaults, installed rules, or current working directory behavior. That weakens the Agent safety boundary and makes test results less reproducible.

Recommended fix:

Update Codex lint invocation to use explicit `--ephemeral`, `--ignore-rules`, `--sandbox read-only`, `--skip-git-repo-check`, and `-C <workspace>` where supported. Apply the same review to HTML export separately.

Acceptance criteria:

- Codex deep lint uses an explicit read-only sandbox profile.
- Lint invocation tests match compile/chat style assertions.
- Deep lint cannot write candidate workspace files in normal operation.

Suggested tests:

- Agent invocation unit test for Codex lint flags.
- Fake process runner test confirming lint prompt is delivered via stdin while cwd/sandbox flags are present.

## P2 / Later

### P2-1. Graph Edges Lack Evidence, But This Matches MVP Scope

Current code evidence:

- `src-tauri/src/services/graph_service.rs:37-73` builds edge signals from resolved wikilinks and shared tags.
- `src-tauri/src/services/graph_service.rs:81-86` emits `relation: "related"` and `weight`.
- `src-tauri/src/models/graph.rs:28-39` documents v1 edges as single-valued `related`.

External evidence:

- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "4-signal graph".

Why this is a gap:

External graph implementations expose richer association signals. Current UI can show that pages are related, but not why.

Risk if not fixed:

Graph exploration is less explainable. However, this is not a P0 because project SPEC says v1 edges are uniformly "related".

Recommended fix:

Later add optional `signals: [{kind, evidence, weight}]` to `GraphEdge` if UI needs edge explanations.

Acceptance criteria:

- Existing graph payload remains backward-compatible.
- UI can show "wikilink", "shared tag", or "source overlap" when available.

Suggested tests:

- Graph serialization backward-compatibility tests.
- Edge signal aggregation tests.

### P2-2. Vector Search Must Stay Optional Derived Cache

Current code evidence:

- `SPEC/SPEC.md:52-55` states data storage is files and "无数据库".
- `src-tauri/src/services/wiki_index.rs:11-16` explicitly says no database and in-memory index only.
- `src-tauri/src/services/search_service.rs:22-23` says Search never calls LLM or Agent.

External evidence:

- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "Vector Semantic Search".

Why this is a guardrail:

Vector search can improve recall, but adding it as a first-line storage dependency would violate this project's source-of-truth model.

Risk if done wrong:

The project drifts into a database-backed RAG app and weakens file transparency.

Recommended fix:

Do not implement vector search in the first non-import plan. If added later, store embeddings as opt-in, deleteable `.app/` cache, never as the wiki content source.

Acceptance criteria:

- Markdown remains source of truth.
- Deleting `.app/` cache does not lose user content.

Suggested tests:

- Cache rebuild/delete tests if embeddings are introduced later.

### P2-3. MCP-like Surface Should Follow The Read-Only API, Not Precede It

Current code evidence:

- `src-tauri/src/services/agent_service.rs:160-166` intentionally disables host MCP/session hooks for programmatic Claude runs.
- Current Rust source search has no `api/v1` or wiki MCP server implementation.

External evidence:

- nashsu README: [README](https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md), short quote: "MCP Server".
- lucasastorian README: [README](https://raw.githubusercontent.com/lucasastorian/llmwiki/master/README.md), short quote: "small, deliberate set of tools".

Why this is a gap:

External projects expose MCP/tool surfaces, but this project first needs a narrow, tested read-only API contract.

Risk if done too early:

The tool surface may expose writes, leak paths, or bypass the app's Git/PendingAction boundaries.

Recommended fix:

Design and test the read-only local API first; map it to MCP only after endpoint contracts stabilize.

Acceptance criteria:

- MCP proposal references existing read-only endpoints.
- No MCP write tool ships before write safety design.

Suggested tests:

- Same token/path/size-limit tests as read-only API.

### P2-4. Chat Convenience Write Mode Needs A Separate Safety Review Before Expansion

Current code evidence:

- `src-tauri/src/services/agent_service.rs:304-336` builds a Chat convenience invocation with `Read Grep Glob Edit Write Bash` for Claude and `workspace-write` for Codex.
- Normal Chat is read-only at `src-tauri/src/services/agent_service.rs:257-293`.

External evidence:

- nashsu Skill: [SKILL.md](https://raw.githubusercontent.com/nashsu/llm_wiki_skill/main/SKILL.md), short quote: "Stay read-only".

Why this is a gap:

Convenience Chat can become an editing workflow, but it is a different risk class than answer-only Chat.

Risk if not contained:

Users may think they are asking a question while the Agent has write capability.

Recommended fix:

Keep convenience write mode separate, visibly labeled, and checkpointed. Do not merge it into normal Chat behavior.

Acceptance criteria:

- Normal Chat route stays read-only.
- Any convenience write route requires clear UI labeling and audit log.

Suggested tests:

- Invocation tests proving normal Chat does not allow write tools.
- Task log tests for convenience write route.

### P2-5. Local Lint Dead-Link Line Numbers Are Body-Relative

Current code evidence:

- `src-tauri/src/services/lint_service.rs:76` calls `find_wikilink_line(&split.body, target)`.
- `src-tauri/src/services/lint_service.rs:1249-1256` enumerates body lines from 1.

External evidence:

- Astro-Han Skill: [SKILL.md](https://raw.githubusercontent.com/Astro-Han/karpathy-llm-wiki/main/SKILL.md), short quote: "Internal links".

Why this is a gap:

The UI range can point to the wrong physical Markdown line when a page has YAML frontmatter. This is not a semantic LLM Wiki gap, but it hurts code-level auditability and user trust in lint fixes.

Risk if not fixed:

Users click a dead-link issue and land several lines above the actual problem.

Recommended fix:

Make `split_frontmatter` expose the body starting line, or compute the offset before calling `find_wikilink_line`.

Acceptance criteria:

- Dead-link issue range points to the correct file line with and without frontmatter.

Suggested tests:

- Lint fixture with multi-line frontmatter and a dead wikilink in the body.

### P2-6. Lint Index Regeneration May Include Source And Query Pages

Current code evidence:

- `src-tauri/src/services/lint_service.rs:1101-1107` lists all `wiki/**.md` files and skips only `wiki/index.md` and `wiki/log.md`.
- `src-tauri/src/services/lint_service.rs:1121-1125` writes every remaining page into the regenerated index.

External evidence:

- Astro-Han Skill: [SKILL.md](https://raw.githubusercontent.com/Astro-Han/karpathy-llm-wiki/main/SKILL.md), short quote: "Index consistency".

Why this is a gap:

`wiki/sources/` are import-owned originals, and `wiki/queries/` are saved answers. Treating them exactly like derived concept/entity pages in the main index can muddy the navigation layer.

Risk if not fixed:

Auto-fixing index drift may clutter `wiki/index.md` with source mirrors or query records, making the index less useful as an LLM navigation entry.

Recommended fix:

Define index inclusion policy: structural pages excluded, `wiki/sources/**` excluded or grouped separately, `wiki/queries/**` excluded or grouped separately, derived pages included.

Acceptance criteria:

- Regenerated index does not flatten `wiki/sources/**` into normal page entries unless product explicitly chooses a grouped source section.
- Tests cover sources, queries, concepts, entities, and overview/log/index exclusions.

Suggested tests:

- `regenerate_index` fixture with source, query, and derived pages.

## Executive Summary

The project has the right product boundary: Markdown + JSON + local files, no user-content database, local keyword search separated from Chat/BYOK, and React mostly delegates filesystem/Git/Agent work to Tauri services. Several older P0s have improved: `WikiIndex` now provides a shared in-memory index, compile rejects `wiki/sources/` writes, Agent Chat has a read-only profile, and BYOK compile prompt is no longer a one-line stub.

The current shortest path to external-best parity is not a database or vector store. The urgent gaps are:

1. Compile still has no auditable `CompilePlan` stage and `validate_manifest` mostly checks paths/core files, not source/schema/frontmatter semantics.
2. Chat citations are retrieved hits copied before model execution, not evidence the model actually used.
3. Chat retrieval is fixed top-k excerpt assembly with one mixed Agent/BYOK prompt, not index-first/read-more/budgeted retrieval.
4. Deep lint gives the Agent short excerpts and trusts Agent severity without a rubric.
5. The compile Agent write profile pre-approves Bash against user-derived source content; project-level validation helps, but system-level command containment is still explicitly incomplete.
6. Codex deep-lint/export Agent profiles are less explicit than compile/chat sandbox profiles.

Recommended priority: fix compile planning/validation and citation provenance first, then split prompt/DTOs, then improve retrieval and lint evidence, then add a read-only Agent/API surface.

Relationship to earlier audits:

- `docs/audits/2026-07-05-code-level-detail-audit.md` treated the lack of a memory index as a major performance issue. Current code has `WikiIndex`, so that is improved and no longer a P0.
- `docs/audits/2026-07-07-external-projects-deep-dive.md` included import reliability among the top gaps. This audit intentionally excludes import from the main plan per scope; those issues are preserved only in the appendix.
- The older "BYOK prompt is a stub" finding is partially fixed by current `provider_prompt`, but the validation/plan gap remains P0.

## Current Strengths / Already Improved

- `wiki/sources/` compile protection exists in Skill and code: `wiki-ingest/SKILL.md:12-15`, `compile_service.rs:353-360`, and `compile_service.rs:728-731`.
- BYOK compile prompt has improved from the older "thin prompt" issue: `compile_service.rs:24-27` now includes derived pages, cross-source synthesis, protected sources, citation, and core page requirements. It still lacks plan/semantic validation.
- Shared in-memory `WikiIndex` is implemented and explicitly no-database: `wiki_index.rs:1-16`, `wiki_index.rs:100-186`.
- Local Search boundary is explicit: `search_service.rs:22-23` says Search never calls LLM or Agent.
- Agent Chat read-only profile exists: `agent_service.rs:257-293` allows `Read Grep Glob` for Claude and `read-only` sandbox for Codex.
- LLM base URL secret leakage is guarded: `llm_service.rs:59-72` rejects credentials and secret query params; `llm_service.rs:137-143` rejects missing provider secrets.
- BYOK request construction keeps secrets in headers rather than persisted project files or request body content: `llm_service.rs:76-96`.

These improvements mean the old audit items about "no memory index" and "BYOK prompt is only a stub" should not remain P0 in the next plan.

## Things Not To Do

- Do not introduce a database as the source of truth for user wiki content. Keep Markdown/JSON/local files.
- Do not turn global Search into natural-language answer generation. Search remains keyword/filter; Chat/Agent/BYOK owns answers.
- Do not let React own filesystem, Git, Agent process, or secret-storage logic.
- Do not mix import redesign into this execution plan.
- Do not copy nashsu's optional vector/LanceDB path as P0. If done later, make it opt-in and derived-cache only.
- Do not expose write-capable localhost API endpoints in the first Agent API step.
- Do not make Agent CLI mandatory; BYOK must keep core compile/chat flows.

## Import-Specific Appendix

These are real issues, but they belong to an import-redesign audit and are not included in the main execution plan:

- URL fetch currently disables redirects and accepts only UTF-8: `src-tauri/src/commands/import_commands.rs:535`, `src-tauri/src/commands/import_commands.rs:581`.
- Readability extraction is invoked from React: `src/components/app/AppShell.tsx:416-428`, with implementation in `src/lib/readability.ts:31-90`.
- Batch confirm/import behavior and staged cleanup are in `src-tauri/src/services/import_service.rs:682-808`.
- Source replacement and raw/source protection live around `src-tauri/src/services/import_service.rs:104-190` and related tests.

Why not main scope:

- The requested scope explicitly says "导入以外".
- Import redesign changes URL handling, persistent queues, source replacement, partial success, clipping/highlighting, and Readability ownership. Those require a separate risk model and user-confirmation plan.

Future import audit should read:

- `src-tauri/src/commands/import_commands.rs`
- `src-tauri/src/services/import_service.rs`
- `src-tauri/src/services/extraction_service.rs`
- `src/components/app/AppShell.tsx`
- `src/lib/readability.ts`
- import preview/confirm UI components and import tests.

## Execution Plan

### Phase 0: Safety Preparation And Test Baseline

Goal:

Establish current behavior and protect user work before code changes.

Files:

- Read: `AGENTS.md`, `SPEC/*.md`, current audit docs.
- Read/test: `src-tauri/src/services/*`, `src-tauri/src/commands/*`, `src-tauri/src/models/*`.

Steps:

1. Run `git status --short` and record pre-existing dirty files.
2. Run `npm run test` and `npm run lint`.
3. Run targeted Rust tests/checks if available in the branch, especially compile/chat/lint/search/graph service tests.
4. Confirm no work starts on import code.

Tests:

- `npm run test`
- `npm run lint`
- Optional: `cargo test compile_service chat_service lint_service search_service graph_service`

Risks:

- Dirty worktree may contain user changes. Do not reset or format unrelated files.

Rollback:

- No code changes in this phase.

Not included:

- No feature implementation.
- No import changes.

### Phase 1: P0 Small-Step Fixes

Goal:

Land the lowest-churn P0 trust fixes, including a minimal compile semantic gate. Full `CompilePlan` workflow remains Phase 2, but Phase 1 must not leave compile validation unchanged.

Files:

- `src-tauri/src/services/chat_service.rs`
- `src-tauri/src/commands/chat_commands.rs`
- `src-tauri/src/models/chat.rs`
- `src-tauri/src/services/compile_service.rs`
- `src-tauri/src/services/lint_service.rs`
- `src-tauri/templates/skills/wiki-lint/SKILL.md`
- `src-tauri/src/services/agent_service.rs`

Steps:

1. Add numbered source refs to Chat prompt.
2. Parse used citations from model answers and store retrieval hits separately.
3. Split BYOK and Agent prompt headers so BYOK never claims filesystem access.
4. Add a minimal semantic `validate_manifest` gate: reject derived pages without non-empty `sources`, reject missing `type`, and require a human-readable source section for derived pages.
5. Add deep-lint severity rubric and increase evidence budget.
6. Make Codex deep-lint invocation explicit and read-only.
7. Add Agent compile profile logging/warning for Bash-enabled runs, but mark it as residual risk until Phase 2 removes or bypasses Bash in the default safe compile path.

Tests:

- Chat citation parser unit tests.
- Chat command fake route tests.
- Compile semantic manifest rejection tests.
- Deep lint prompt snapshot tests.
- Agent invocation profile tests.
- Codex lint invocation sandbox/profile tests.

Risks:

- Existing UI may expect `citations` to equal retrieval hits.
- Full `CompilePlan` is not complete until Phase 2; Phase 1 only prevents the worst invalid manifests.

Rollback:

- Keep old retrieval hits field for compatibility; feature-flag parsed citations if needed.
- Keep the previous manifest validator behind a test-only helper only if needed for fixture migration; do not use it in production apply.

Not included:

- No full `CompilePlan` UI/DTO flow; only semantic manifest rejection.
- No default safe no-Bash compile path yet.
- No localhost API.
- No graph expansion yet.
- No import code.

### Phase 2: Prompt / DTO / Validation Refactor

Goal:

Make compile decisions reviewable and manifests semantically enforceable.

Files:

- `src-tauri/src/models/compile.rs`
- `src-tauri/src/services/compile_service.rs`
- `src-tauri/src/commands/wiki_commands.rs` or compile command owner
- `src-tauri/templates/skills/wiki-ingest/SKILL.md`
- tests adjacent to compile models/services.

Steps:

1. Add `CompilePlan` DTOs.
2. Add shared compile instruction builder.
3. Update BYOK/Agent prompts to request plan first, then manifest/content.
4. Add `validate_plan`.
5. Extend `validate_manifest` to parse frontmatter, source refs, page type, and source mirror risk.
6. Add a default safe compile path where Agent/BYOK returns plan/manifest content and Rust applies writes after validation; Bash-enabled direct workspace writes become fallback or explicitly acknowledged advanced mode only.
7. Add UI/command response support for rejected plan/manifest details.

Tests:

- DTO serialization tests.
- Plan validation tests.
- Manifest semantic validation tests.
- Safe compile path tests proving Rust applies writes after plan/manifest validation.
- Prompt builder tests for all critical clauses.

Risks:

- Existing compile providers may need one extra request or changed prompt flow.
- Removing Bash from the default path can reduce weak-model reliability until prompts/manifest application are tuned.

Rollback:

- Keep a compatibility path for direct manifest only behind a temporary command flag, but do not use it by default.
- Keep Bash-enabled Agent compile as explicit fallback only, with visible residual-risk labeling.

Not included:

- No import extraction changes.
- No automatic source deletion/replacement.

### Phase 3: Chat Citation / Retrieval Redesign

Goal:

Move from fixed top-k snippets to budgeted, source-numbered, verifiable retrieval.

Files:

- `src-tauri/src/services/chat_service.rs`
- `src-tauri/src/services/search_service.rs`
- `src-tauri/src/services/graph_service.rs`
- `src-tauri/src/models/chat.rs`
- `src-tauri/templates/skills/wiki-query/SKILL.md` (new)

Steps:

1. Add retrieval planner DTOs with selected/expanded/omitted pages.
2. Include `wiki/index.md` or its relevant section first.
3. Add graph-neighbor expansion under budget.
4. Budget page content and history by provider context window.
5. Add `wiki-query` Skill mirroring the same citation rules.

Tests:

- Retrieval planner tests for budget and graph expansion.
- BYOK prompt tests for numbered full/partial pages.
- Agent prompt tests for index-first/read-more behavior.
- Saved query tests for citation provenance.

Risks:

- Token estimation can be approximate; use conservative char-based budget first.

Rollback:

- Keep old `retrieve_with_excerpts` as fallback for one release.

Not included:

- No vector DB.
- No web search.

### Phase 4: Lint / Schema Localisation

Goal:

Move deterministic schema/source checks out of Agent judgment.

Files:

- `src-tauri/src/services/lint_service.rs`
- `src-tauri/src/models/lint.rs`
- `src-tauri/src/utils/markdown_utils.rs`
- `src-tauri/templates/skills/wiki-lint/SKILL.md`

Steps:

1. Parse page frontmatter into typed local checks.
2. Validate `type` against known page types and schema-derived constraints.
3. Require non-source derived pages to have non-empty `sources`.
4. Validate source references and human-readable source section.
5. Feed local lint baseline into deep lint prompt.

Tests:

- Fixture pages for wrong type, missing sources, missing source section, missing referenced source.
- Deep lint baseline prompt tests.
- Existing local lint tests must continue passing.

Risks:

- User-authored schema.md can be loose. Start with stable built-in invariants before schema DSL.

Rollback:

- Keep new rules as warnings first; promote to errors after UX confirms.

Not included:

- No auto-fix for semantic Lint in first pass.

### Phase 5: Agent Skill / API Route

Goal:

Give Agents a controlled read-only surface without making it a write API.

Files:

- `src-tauri/src/services/*` for API facade or local server service.
- `src-tauri/src/models/*` for read-only API DTOs.
- `src-tauri/templates/skills/wiki-query/SKILL.md`
- settings models if token-controlled API is added.

Steps:

1. Finalize `wiki-query` Skill for internal Agent behavior.
2. Design read-only endpoints: `health`, `projects`, `search`, `read`, `graph`, `lint-summary`.
3. Require localhost binding and token storage through existing secret/settings boundary.
4. Add opt-in UI setting later; keep disabled by default until reviewed.

Tests:

- Path traversal rejection.
- Token-required behavior.
- Read endpoint size/type limits.
- Search endpoint proves no LLM call.

Risks:

- API surface can grow too fast. Keep write operations out.

Rollback:

- Disable API setting; no project file migration.

Not included:

- No MCP server until HTTP API is stable.
- No write endpoints.

### Phase 6: Documentation And Regression Tests

Goal:

Keep decisions stable and prevent future drift.

Files:

- `docs/audits/*`
- `SPEC/progress.txt`
- `SPEC/gotchas.txt` only if a recurring/subtle issue is discovered.
- targeted test files.

Steps:

1. Update docs describing CompilePlan, citation semantics, and lint layering.
2. Add regression tests for old P0s.
3. Run full checks from scratch.
4. Launch two review passes per project workflow.
5. Fix valid review issues and rerun checks.

Tests:

- `npm run test`
- `npm run lint`
- targeted Rust tests/checks.
- Search for unexpected `console.log`.
- Lint line-range test with YAML frontmatter.
- Index regeneration test that covers `wiki/sources/**` and `wiki/queries/**`.
- Chat convenience dirty-worktree checkpoint/audit behavior test.

Risks:

- Documentation can drift from code. Keep prompt clauses in tests.

Rollback:

- Revert only docs/tests from this phase if they are wrong; never revert unrelated user changes.

Not included:

- No import-redesign docs beyond appendix.

## Test / Verification Plan

Required after each implementation batch:

1. `npm run test`
2. `npm run lint`
3. `Select-String -Path src/**/*.ts,src/**/*.tsx -Pattern "console.log"` or equivalent.
4. Confirm imports resolve via TypeScript build or existing test/lint coverage.
5. Targeted Rust tests for changed services.

Minimum new regression cases:

- Compile rejects derived page without frontmatter `sources`.
- Compile rejects protected `wiki/sources/` writes.
- Compile plan rejects no-source create/update.
- Chat only persists model-used citations.
- BYOK prompt does not claim filesystem access.
- Agent prompt requires index-first/read-more.
- Deep lint prompt includes severity rubric and enough page evidence.
- Local lint catches schema/type/source traceability issues without Agent.
- Search remains local and model-free.
- Agent compile validates candidate workspace before run.

## Source Links

- Karpathy LLM Wiki gist: https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f
- nashsu/llm_wiki README: https://raw.githubusercontent.com/nashsu/llm_wiki/main/README.md
- nashsu/llm_wiki_skill SKILL.md: https://raw.githubusercontent.com/nashsu/llm_wiki_skill/main/SKILL.md
- Astro-Han/karpathy-llm-wiki SKILL.md: https://raw.githubusercontent.com/Astro-Han/karpathy-llm-wiki/main/SKILL.md
- alirezarezvani/claude-skills llm-wiki SKILL.md: https://raw.githubusercontent.com/alirezarezvani/claude-skills/main/engineering/llm-wiki/skills/llm-wiki/SKILL.md
- lucasastorian/llmwiki README: https://raw.githubusercontent.com/lucasastorian/llmwiki/master/README.md
- Hacker News discussion: https://news.ycombinator.com/item?id=47963913
