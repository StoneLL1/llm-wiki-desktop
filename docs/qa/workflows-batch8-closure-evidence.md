# Workflows Batch 8 / Closure Batch G Acceptance Evidence

**Date:** 2026-08-03
**Authority:** `docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md` and `docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`
**Scope:** Workflows Batch 8 cutover plus closure-plan Batch G only.

This record binds every Batch G acceptance scenario to a reproducible automated assertion or a bounded implementation-level visual/accessibility review. It does not claim that the deferred full First-run redesign is complete.

## Automated evidence commands

### Workflows and shared frontend entries

```powershell
npm run test -- src/features/workflows/useWorkflowsController.test.tsx src/features/workflows/workflows.test.tsx src/components/app/WorkspaceController.test.tsx src/components/app/WorkspaceRouter.test.tsx src/components/app/WorkspaceRouter.lazy-error.test.tsx src/components/app/LeftSidebar.test.tsx src/stores/navigationStore.test.ts src/hooks/useTaskLauncher.test.tsx src/features/settings/SettingsView.workflow.test.tsx src/test/workflows-architecture.test.ts
```

Result: **10 files / 68 tests passed**.

### Project authority, task state, theming, responsive and CSS contracts

```powershell
npm run test -- src/app/App.test.tsx src/components/app/appShellActions.test.tsx src/components/app/ProjectConfirmationController.test.tsx src/features/project/ProjectAssessmentPanel.test.tsx src/features/project/ProjectAuthorityDialog.test.tsx src/lib/colorThemePresets.test.ts src/services/workflowApi.test.ts src/services/workflowNavigation.test.ts src/stores/projectStore.test.ts src/stores/taskStore.test.ts src/stores/workflowStore.test.ts src/stores/settingsStore.test.ts src/test/ui-css-contracts.test.ts src/types/project.contract.test.ts
```

Result: **14 files / 125 tests passed**.

### Workflow Rust integrations

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test workflow_contracts --test workflow_preparation --test workflow_routes --test workflow_queue --test workflow_recovery --test workflow_update_wiki --test workflow_health_check --test workflow_generate_content
```

Result: **8 integration targets / 79 tests passed**.

### Formatting

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

Result: **passed** after file-scoped `rustfmt` on the existing Batch 8 `file_commands.rs` change and the explicitly authorized, formatting-only `models/layout.rs` and `services/git_service.rs` corrections. The two newly authorized diffs change only line wrapping, indentation, trailing commas and closure layout; identifiers, literals, argument order, control flow and return expressions are unchanged.

## Required scenario matrix

| # | Scenario | Evidence | Result |
|---|---|---|---|
| 1 | No open project | `workflows.test.tsx` — `renders the no-project state without inventing a workflow`; `workflow_preparation.rs` — `overview_is_fixed_order_and_no_project_is_actionable` | Pass |
| 2 | Empty project | `workflow_preparation.rs` — `empty_project_surfaces_import_and_update_prerequisites_without_inventing_content`; fixed three-row frontend overview test | Pass |
| 3 | Restricted compatible project | `ProjectAssessmentPanel.test.tsx`; `workflow_preparation.rs` — `untrusted_local_quick_is_memory_only_and_does_not_write_workflow_state`, `compatible_preparation_reads_mixed_pages_and_excludes_source_only_roots` | Pass |
| 4 | Trusted read-only project | `workflow_preparation.rs` — `trusted_read_only_authority_stays_memory_only_without_creating_task_state` | Pass |
| 5 | Checkpoint-required project without Git | `workflow_update_wiki.rs` — `required_checkpoint_failure_leaves_formal_wiki_unchanged`; `workflow_generate_content.rs` proves create-new remains valid without Git | Pass |
| 6 | Pre-existing dirty Git state | `workflow_preparation.rs` — `dirty_git_blocks_overwrite_until_the_host_supplies_remediated_access_and_reprepares` proves prepare/start block, no runner dispatch, no artifact/Git mutation, then clean authority requires a fresh preparation before start; `git_service` — `clean_head_checkpoint_never_absorbs_a_dirty_worktree` proves the checkpoint path never absorbs dirty files | Pass |
| 7 | Sources but no Wiki | `workflow_health_check.rs` Source-only fixture and not-applicable rule coverage; preparation prerequisite coverage | Pass |
| 8 | Healthy Wiki | `workflow_health_check.rs` — `complete_runs_local_first_merges_duplicate_evidence_and_persists_for_lint`; Update Wiki complete-run integration | Pass |
| 9 | Queued second workflow | `workflow_queue.rs` serialization/deduplication tests and queued reorder tests | Pass |
| 10 | Waiting confirmation | Update Wiki high-risk waiting test; Generate Content overwrite waiting/confirmation tests; frontend complete confirmation evidence | Pass |
| 11 | Failed route | `workflow_routes.rs` exact-route/no-fallback tests; Health Check stale-route failure; Settings return tests prove no automatic launch | Pass |
| 12 | Cancelled | Generate Content cancellation leaves no artifact; queue cancellation/Undo is idempotent; frontend cancel confirmation | Pass |
| 13 | Interrupted after restart | `workflow_recovery.rs` — `restart_interrupts_running_and_holds_queued_until_explicit_continuation` | Pass |
| 14 | Valid and invalid quick rerun | `workflow_preparation.rs` — `validated_start_deduplicates_and_enables_quick_rerun`, `changed_baseline_or_access_invalidates_the_token` | Pass |
| 15 | Remote first-use disclosure | `workflow_generate_content.rs` — `remote_provider_disclosure_is_required_without_conflating_restricted_content` | Pass |
| 16 | CJK paths and long English labels | Workflow queue/recovery/update CJK and Unicode tests; `workflows.test.tsx` — `keeps long English context labels and actions keyboard reachable at 200 percent text size` | Pass (DOM/semantic scaling contract; no pixel screenshot claim) |
| 17 | Narrow right-panel overlay | `ui-css-contracts.test.ts` asserts the `1180px` overlay/backdrop contract; `App.test.tsx` asserts accessible right-panel splitter semantics | Pass |
| 18 | Queue, confirmation, result and cross-surface ownership | WorkspaceController, workflow navigation, result navigation, task-store and confirmation-controller tests | Pass |

## Product-level acceptance

| Requirement | Evidence | Result |
|---|---|---|
| Active task or next action is immediately identifiable | Overview gives attention states precedence and retains a single recommendation; fixed three-row test | Pass |
| Start in no more than three primary actions | Row action -> structured preparation -> Start; no generic launcher or setup detour | Pass |
| Stage/progress/intervention understandable without raw logs | Nine/eight-stage DTO tests, `WorkflowPipeline`, indeterminate-count test and waiting decision node | Pass |
| Agent/Provider/Skill details remain secondary | Workflows rows have no route selector; route appears in preparation/context only; architecture test rejects arbitrary execution input | Pass |
| Every write result exposes files, Git and recovery evidence | Update Wiki/Generate Content result and confirmation assertions expose affected paths and checkpoint/commit data | Pass |
| Prepare/start/confirm revalidate backend authority | Stale preparation token, route revision, canonical identity, authority transition-lock and confirmation reconstruction tests | Pass |
| Settings returns without automatic launch | WorkspaceController and Settings workflow tests restore scope/route then prepare only | Pass |

## Equivalent implementation-level UI and accessibility audit

The Impeccable detector was **not rerun**. The required equivalent review inspected the current Workflows components and incumbent CSS contracts against `SPEC/FRONTEND_GUIDELINES.md` and `UI-Frontend-design/assets/app.css`.

- **Accessibility:** semantic buttons and sections are used; icon-only actions have labels/titles; progress has an accessible label/value; status includes text plus icon/tone; dialogs/drawers retain the global focus-restoration contracts; reduced motion is covered by the CSS contract.
- **Keyboard/focus:** Workflows actions are native buttons/inputs; retry options use `aria-expanded`, `aria-controls` and a labelled group; the focused 200%/long-label DOM test proves the shell control and workflow action remain focusable in document order.
- **Theming:** no raw hex/RGB/HSL colors occur in `src/features/workflows`; presentation consumes existing CSS variables. Light/dark and supported presets share the same semantic token surface and are covered by preset/store tests.
- **Responsive/text scaling:** the CSS contract proves the right panel becomes the existing overlay below `1180px`; the focused DOM test applies `font-size: 200%`, injects a long English accessible title, and verifies the title and keyboard controls remain present. This is contract-level evidence, not a claim of pixel-level GUI inspection.
- **Performance:** history remains paginated, overview remains bounded, one project runs serially, and controller refresh/event buffering is project- and identity-scoped.
- **Implementation integrity:** Workflows production sources contain no direct Tauri `invoke`, filesystem, Git mutation, trust persistence, assessment derivation, arbitrary prompts or secret access. Backend-issued typed DTOs remain the authority.

The first review pass found four release-blocking issues. They were fixed by preferring explicit right-panel selection, atomically resetting presentation state on canonical identity rotation, validating recovered task IDs/file stems and final persistence files, and excluding Workflow-owned Generate Content decisions from the generic confirmation command. Focused regressions cover each fix.

## Detector evidence integrity

| Covered path | Closure-plan anchor | Batch G value | Disposition |
|---|---|---|---|
| `src/features/workflows/` aggregate | `fbb6ae797bff9fb5b7d272cdf3debdd6c31b2e7f2db746c154bca39a7a5ef652` | `db2cdb5947a4a5dc8bc4f816e3d368d80ef6fdd6f15be7bda0089144710ce0e8` | Expected delta from the approved controller/event, right-panel selection fix, tests, and Batch 8 cutover changes; manually reviewed |
| `src/components/app/LeftSidebar.tsx` | `50dffbeb417d1330…` | `50dffbeb417d13305b594bc34a1d8f52b374043288225438be73ebd6af5b3551` | Exact match |
| `src/components/app/RightContextPanel.tsx` | `7314e507b569281e…` | `7314e507b569281ea3f353fed96284394de206064d7f63938f63446123437740` | Exact match |
| `src/styles.css` | `2a45a53ea61c0c5a…` | `2a45a53ea61c0c5af993dc45681c4fc9dc50d451a3ae021f24ae956951a25afe` | Exact match |

Detector execution count for this closure remains **one historical run**. Batch G performed no second detector invocation.

## Final gate and reviews

- Pre-review full `npm run check`: passed from the beginning (116 frontend files / 824 tests; 876 Rust unit/integration/doc tests, build/lint/console/evidence lanes all passed).
- Reviewer A: found explicit-selection precedence and QA overclaim gaps. Both were accepted; selection logic received a regression test and this document now distinguishes executable DOM/CSS evidence from pixel-level inspection.
- Reviewer B: found unsafe recovered task IDs, same-root identity replacement leakage, a generic Generate Content confirmation bypass, and missing dirty-Git workflow coverage. All were accepted and fixed with focused tests.
- Post-review focused reruns: 4 frontend files / 32 tests passed; task recovery/file-command unit filters passed; empty-project and dirty-Git Workflow preparation regressions passed; `cargo fmt --check` passed.
- Post-review full `npm run check`: passed from the beginning in 6m03.1s; 116 frontend files / 828 tests passed, the Rust lane reported 878 library tests plus all integration/doc targets passed, and GUI compilation/build/lint/console/evidence lanes passed.

## Explicitly deferred First-run work

This closure does not implement the full no-project shell/two-card migration, the new-project wizard and Import handoff, ordinary-materials create-from flow, ambiguous-folder intent memory, full repair/recovery pages, two-stage background deep scan, independent right-panel type/trust/access/health rows, or removal of the legacy assessment adapters.
