# Workflows

Project-scoped presentation for the three fixed product workflows: Update Wiki, Health Check, and Generate Content.

## Presentation

The three reference-style function cards remain above the selected preparation/task panel and bounded recent history. Card clicks use the existing guarded preparation or task-opening actions; rendering the selectors never preloads all forms or starts a workflow. Forms use grouped options, a scope/output summary and one start action. Four semantic phases render horizontally, while detailed stages/logs remain on demand. Responsive layouts use the actual content width; full history retains its fixed virtual row geometry.

## Ownership

- `useWorkflowsController.ts` owns typed IPC calls, stale-request guards, preparation, and query reconciliation.
- Backend `TaskService` owns task facts; frontend `taskStore.workflowById` accepts canonical summaries from IPC and the global event bridge, including for inactive projects.
- `workflowStore.ts` owns project-scoped queries, drafts, selection, and at most 16 on-demand task details; project identity or backend session changes invalidate stale presentation.
- Preparation/start actions, history queries, result opening, and Workflow notification details load on demand. The controller captures guards and operation locks before loading; stale intents stop before IPC and loading failures release their locks.
- Views render structured workflow DTOs and project-scoped run history. Backend services remain the authority for project identity, access, trust, route selection, queueing, Git policy, confirmation, and mutation.
- Settings owns Agent and Provider configuration. Lint owns repairs. Exports owns generated-artifact records and previews. The generic task drawer owns raw logs.

## Refresh model

- Ordinary running updates are owner/session-filtered, merged by task and revision in the global dispatcher, and committed in a 100ms window. They never pull overview or history.
- Waiting, terminal, queued, continuation, and other semantic boundaries commit immediately and schedule a project-scoped overview reconciliation.
- Overview reconciliation uses one in-flight request and a dirty flag. The backend reads its owner summary index and observed access snapshot without preparation, Markdown scans, Git, Agent probes, or history reads. An old-project response cannot commit or schedule work for the new project.
- History loads independently when its surface is opened, on explicit pagination/refresh, or on a terminal boundary while visible. History errors do not hide the overview. Selected details hydrate on demand; only a newer known revision schedules a follow-up read.
- The global task event bridge always preserves backend task facts. Workflows and right-panel selectors are route-local; inactive Import ownership retains a frozen context for exact resumption without subscribing to background task updates.

## Update Wiki

- Opening the form is a local navigation change, with no `prepare_workflow` request. `UpdateWikiForm` owns independent route-summary and optional paged Source-directory queries; neither disables mode/selection editing. Automatic selection remains intent, not a cached list from a previous run. Manual empty selection is distinct and cannot start.
- `start_update_wiki` persists a typed request ID, mode, selection and configured route under task authority. Duplicate delivery of the same request reuses its task, including after recovery. The task worker resolves current automatic inputs or verifies explicit versions, checks only the selected route, then atomically persists scope, route, baseline and fingerprint. Queued replay uses this intent path; old preparation-based tasks remain compatible.
- V2 directory queries read manifest metadata without source-body hashes. Legacy source indexes do not contain version metadata, so manual legacy listing still computes content versions on demand. Directory work is never a prerequisite for opening the form or choosing Automatic.
- A no-change run completes without Agent discovery, credentials, external invocation or Git history. Actual generation still uses CompileService and an isolated candidate workspace. Source originals remain outside the Agent's writable scope.
- Selected-source validation continues through candidate recovery and checked apply; an unrelated broken Source does not poison a manually selected update. Changed fixed inputs retain scope review. Provider configuration is checked against the exact immutable config sent to LlmService.
- Candidate confirmation, app-private Git history, external-edit conflict checks, trust/path authority and cancellation remain owned by existing backend services. Navigating away preserves accepted task facts without reopening its detail panel.

## Health Check

- Local Quick starts from the readable knowledge base at execution time, independently of Agent discovery, Provider settings/credentials, and Git. Enqueueing after Update Wiki therefore checks the new pages.
- LintService owns the run-local read snapshot and shared rules. It reports actual page batches and observes cancellation at page boundaries; bounded AI excerpts reuse the same reads.
- Trusted projects with writable app state persist reports and tasks for every route, including after queued-task recovery and explicit continuation. Restricted/read-only projects retain process-local reports. Reports do not authorize repairs or require content checkpoints.
- Complete saves the local portion before invoking the selected AI route. AI failure or cancellation retains a result link and accurate unfinished coverage. Changed disclosure inputs enter the existing scope-review flow before external launch.
- Opening a report checks its recorded inputs for freshness in a blocking worker. Stored input evidence and task facts remain unchanged; repair still validates the current report owner, selected findings, hashes, route and authorization.

## Generate Content

- Each built-in workflow presents four semantic phases; detailed backend stages mount only when expanded.
- The four existing HTML artifact types are `beautiful_read`, `knowledge_card`, `concept_map`, and `project_report`. Preparation exposes artifact type, applicable Wiki pages, output path, and execution route; the concept map centers on the first selected page. Separate free-text topic, theme, report subtype, and optional report-exclusion controls are absent; the latter remains a target-design gap, not a completed capability.
- A normal run prepares a new artifact path. Explicitly targeting an existing artifact preserves its checked overwrite, scoped Git checkpoint, and candidate confirmation. Changed queued inputs enter `review_scope`; re-preparation preserves the selected artifact/page intent and links the new task to the cancelled review task.
- Generation reuses ExportService templates and HTML/resource validation. Workflow and Wiki quick export share validated `ExportRecord` construction and create-new publication; a pending record receipt bridges a crash between publishing the exact HTML and saving history. Receipt recovery is read-only, verifies the artifact bytes, and deduplicates by record ID. Failure cleanup removes only bytes still matching this operation.
- The final publication section atomically closes cancellation through TaskService's canonical `cancellable` fact. A cancellation already accepted prevents publication; the UI stops offering cancellation after the seal. No runner-local cancellation state competes with the task summary.
- Opening a result resolves its exact `recordId`, checks `taskId` when present, and waits for the corresponding preview before navigating to Exports. Missing, replaced, or superseded results fail explicitly rather than opening the newest unrelated artifact.
- Wiki single-article quick export remains in the article and uses a direct Export task, outside Workflow history. It supports the existing reading-page, knowledge-card, and concept-map choices without overwrite.

## Recovery and verification boundaries

- Schema v2 and command names remain compatible; old missing/numeric revisions are normalized, and process sessions prevent delayed pre-restart events from replacing recovered facts. Persisted queued work needs explicit continuation; incomplete running work becomes interrupted, while valid pending confirmation remains reviewable and invalid candidates become interrupted.
- A durable Health report or exact task-associated Export record may be attached to an interrupted task without changing it to success or relaunching AI. A committed Update receipt is handled by the existing committed-apply recovery contract. Current trust, identity, paths, candidate hashes, and write authority are still checked when continuing or confirming.
- Structural and release fixture checks cover bounded overview/history summaries, 50 hot activations, 10,000 progress events, and large content/history inputs. They do not establish Tauri frame timing, real AI route coverage, or Decision Gate H6 completion; acceptance evidence must distinguish these layers.

## Non-goals

- No arbitrary prompts, shell commands, filesystem writes, Git operations, or secret access from React.
- No global or cross-project task launcher, silent execution-route fallback, or fourth built-in workflow.
- No replacement for technical Agent services, Agent types, capability detection, or the unchanged sidebar Agent status foot.

The superseded Agent page, right panel, and generic Run Agent dialog were retired in Workflows Batch 8. Compatibility-only Agent concepts remain under their existing technical names.

## Decision Gate H status

As of 2026-08-13, H3–H5 provide backend-derived Agent Health availability and the guarded Lint repair task surface while preserving the existing queue, confirmation, checkpoint, result, and Diff contracts. H6 final validation remains no-go: the recorded full gate is not green in the current Windows environment, and the complete performance/negative/WebView2 evidence matrix is not closed. Do not mark Decision Gate H or Batch 7 unblocked.

## Verification commands

- Full source gate: `npm run check` (required for cross-layer or file-safety changes).
- Workflow contracts: `npm test -- src/features/workflows src/stores/workflowStore.test.ts src/services/workflowNavigation.test.ts src/services/taskEventDispatcher.test.ts`.
- Export/Health/recovery file journeys: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test workflow_generate_content --test workflow_health_check --test workflow_recovery`.
- Large-history release fixture: `cargo test --manifest-path src-tauri/Cargo.toml --release --test workflow_performance -- --ignored --nocapture`. This measures public Rust queries, not WebView frames or IPC transport.
- Opt-in real Claude export acceptance: `LLM_WIKI_RUN_REAL_CLAUDE=1 cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib real_claude_exports_all_four_html_types_in_disposable_chinese_project -- --ignored --nocapture`. Uses synthetic disposable content and the already configured CLI; retains the evidence directory printed by the test.
