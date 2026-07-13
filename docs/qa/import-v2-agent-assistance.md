# Import V2 Agent Assistance QA Evidence

Status: Local automated evidence for the implementation boundary. This document records reproducible repository tests; it is not a claim of third-party penetration testing, provider billing certification, or production deployment certification.

## Policy and product behavior

| Gate | Expected boundary | Automated evidence |
| --- | --- | --- |
| Automatic deterministic failure assistance | A local Agent may start only after a deterministic hard failure, only when the saved policy enables it, a configured Agent is available, the per-item attempt cap is open, and a reviewed text input is available. Binary-only inputs fail before process invocation. | `import_v2_agent_assistance::product_policy_allows_only_approved_local_hard_failure_automation`; `import_v2_agent_orchestration::policy_matrix_keeps_quality_manual_and_hard_failure_explicitly_approved`; `import_v2_local_agent` |
| Manual low-quality optimization | A successful deterministic Warning remains usable and Agent optimization requires an explicit user action. | `import_v2_agent_assistance::product_policy_allows_only_approved_local_hard_failure_automation`; `import_v2_agent_contracts::balanced_policy_never_auto_invokes_cloud_or_low_quality_success` |
| Agent unavailable | Return an unavailable/manual fallback state and installation guidance; never install or execute an install command. | `import_v2_agent_orchestration::policy_matrix_keeps_quality_manual_and_hard_failure_explicitly_approved`; `import_v2_local_agent` |
| BYOK disclosure and approval | Preview exact provider, model, destination, redacted files, estimated tokens/cost and scope hash before a one-shot approval. Cloud invocation is never automatic. | `import_v2_byok_assistance::byok_scope_is_exact_expiring_and_one_shot`; `commands_expose_preview_and_approval_without_automatic_byok` |
| Duplicate BYOK charge | A prior `BYOK_CHARGE_STATUS_UNKNOWN` requires a new preview and explicit possible-duplicate-charge acknowledgement before another task can be created. | `import_v2_agent_assistance::uncertain_byok_charge_requires_new_explicit_acknowledgement_before_task_creation` |
| Exact audit records | Local and BYOK attempts durably bind task/session/item, route, Agent kind/model version, prompt template version, approved tools/scope/cost, input/output hashes, timestamps and outcome. A Candidate carries the audit identity and immutable provenance fields. | `import_v2_local_agent::start_returns_bound_task_and_run_redacts_output_without_replacing_failure`; `import_v2_byok_assistance::byok_scope_is_exact_expiring_and_one_shot`; `import_v2_agent_candidate::accepts_staged_candidate_with_exact_hashes_and_preserves_baseline` |

## Threat and secret corpus

| Gate | Attack corpus and expected result | Automated evidence |
| --- | --- | --- |
| Prompt injection | Source instructions requesting credentials, network, Git or project reads remain untrusted data; the workspace contains only copied current-item inputs and the broker grants no such authority. | `import_v2_agent_workspace::workspace_contains_only_sanitized_current_item_copies`; `import_v2_agent_assistance::threat_corpus_denies_injected_authority_secret_echo_and_executables` |
| Tool injection | Unknown, ungranted, cancelled and forged tool calls fail before executor invocation. The protocol has no shell, install, credential, Git or fetch variant. | `import_v2_agent_tools::broker_denies_ungranted_injected_and_cross_item_requests`; `tool_protocol_has_no_arbitrary_authority_variants` |
| Path traversal | Parent paths, cross-item paths, links/reparse points and paths outside item staging are rejected. | `import_v2_agent_assistance::threat_corpus_denies_injected_authority_secret_echo_and_executables`; `import_v2_agent_workspace::workspace_rejects_symlinked_source_snapshot`; `import_v2_agent_candidate` |
| Command injection | Executable route strings, shell syntax and command-bearing parser identifiers do not reach the tool executor or process runner. | `import_v2_agent_assistance::threat_corpus_denies_injected_authority_secret_echo_and_executables`; `import_v2_agent_tools::broker_denies_ungranted_injected_and_cross_item_requests`; `import_v2_local_agent` |
| Secret echo | Candidate Markdown, provenance text and text assets reject private-key, bearer, API-key, GitHub-token and common model-key markers; logs/audits are redacted and provider keys remain in OS credential storage. | `import_v2_agent_assistance::threat_corpus_denies_injected_authority_secret_echo_and_executables`; `quality_gate::agent_candidate_rejects_secret_corpus_and_unclosed_fence`; `import_v2_byok_assistance::byok_scope_is_exact_expiring_and_one_shot`; `import_v2_agent_policy` |
| Cross-item/project | Task, workspace, tool, BYOK approval, candidate and action identities are bound to exact project/session/item/task identifiers. | `import_v2_agent_tools::broker_denies_ungranted_injected_and_cross_item_requests`; `import_v2_agent_workspace`; `import_v2_agent_candidate`; `import_v2_agent_orchestration` |
| Captcha/paywall bypass | No browser bypass or fetch authority exists in Agent tools; injected route names are rejected. Existing Web Ingestion typed user-action boundaries remain authoritative. | `import_v2_agent_assistance::threat_corpus_denies_injected_authority_secret_echo_and_executables`; `import_v2_web_ingestion`; `import_v2_browser_sessions` |
| Executable artifacts | Executables, renamed executable bytes, active SVG and undeclared files fail candidate validation. | `import_v2_agent_assistance::threat_corpus_denies_injected_authority_secret_echo_and_executables`; `quality_gate::agent_candidate_assets_reject_secrets_active_svg_and_renamed_executables`; `import_v2_agent_candidate` |

## Candidate, Diff, and three-way merge

Agent output is copied into item staging as a hash-bound candidate only after task identity, source/deterministic hashes, strict manifest, declared tree, secret checks and `QualityGate` pass. The original deterministic preview is retained. Source Registry baseline, current Wiki and Agent candidate form the Diff; Wiki drift requires an explicit merged Markdown value bound to the exact current Wiki hash. Discard restores the deterministic preview. Selection still does not write formal content; only `ImportV2Service::commit_items` crosses the existing Git-checkpointed commit boundary.

Evidence: `import_v2_agent_candidate`, including select → formal commit; `import_v2_agent_orchestration::candidate_actions_are_explicit_and_merge_is_hash_bound`; candidate unit tests in `agent_candidate` and `quality_gate`.

## Lifecycle and recovery

| Gate | Expected result | Automated evidence |
| --- | --- | --- |
| Cancellation and restart | Cancellation before provider connection or process start is terminal; a real child/grandchild cancellation test verifies tree termination. Windows starts the Agent suspended, assigns a kill-on-close Job, then resumes it; Unix combines a process group, parent-death signal and watchdog pipe that kills the group when the app handle closes. Live polling preserves active tasks; restart closes abandoned tasks and containment-checked recovery removes only the exact workspace recorded by that task. | `import_v2_byok_assistance::cancelled_provider_call_does_not_open_a_connection`; `import_v2_local_agent::system_runner_redacts_stdout_stderr_and_stops_a_cancelled_process`; `import_v2_agent_orchestration::session_poll_does_not_close_a_live_agent_attempt_but_restart_failure_does`; `import_v2_agent_assistance::restart_twice_closes_inflight_tasks_without_formal_content_mutation`; `import_v2_agent_candidate` |
| Candidate write crash | Recovery revalidates complete candidates and rebuilds containment-checked incomplete candidate directories after truncated source, Markdown or asset writes. Repeated recovery is idempotent and does not add a false rejected marker. | `import_v2_agent_candidate::accepts_staged_candidate_with_exact_hashes_and_preserves_baseline` |
| Validation failure | Item state returns atomically to the prior deterministic state with one `AGENT_CANDIDATE_REJECTED` marker; repeated polls do not retry poisoned output. | `import_v2_agent_candidate`; `import_v2_agent_orchestration::validation_is_explicit_and_failed_optimization_restores_deterministic_preview` |
| BYOK ambiguous send | Restart never resubmits a cloud request. Ambiguous send state is surfaced as charge unknown and requires a new explicit acknowledgement. | `import_v2_byok_assistance`; `import_v2_agent_assistance::uncertain_byok_charge_requires_new_explicit_acknowledgement_before_task_creation` |
| No orphan/formal mutation assertion | Task recovery is stable across two service restarts, select/discard and terminal recovery remove only the completed task workspace while preserving a sibling workspace, a grandchild PID test verifies explicit cancellation cleanup, and release recovery preserves a sentinel Wiki file. Parent-crash cleanup is implemented with OS lifetime primitives, but a packaged-app parent-crash descendant test remains operational release evidence rather than a repository test. | `import_v2_agent_assistance::restart_twice_closes_inflight_tasks_without_formal_content_mutation`; `import_v2_local_agent::system_runner_redacts_stdout_stderr_and_stops_a_cancelled_process`; `import_v2_agent_candidate`; `import_v2_agent_workspace` |

## No direct raw/wiki mutation

Agent local/BYOK runners write only their item workspace. Candidate validation and selection write only the item's staging/session records. Agent commands expose start, preview/approve, accept, select and discard actions but no direct formal write. Formal raw/wiki mutation remains behind `ImportV2Service::commit_items` and `GitService` checkpoint/conflict rules.

Evidence: `import_v2_agent_orchestration::commands_expose_accept_select_and_discard_without_direct_wiki_writes`; `import_v2_agent_candidate`; `import_v2_core`; commit and transaction unit tests.

## Core/File/Web integration

- Core: typed item/task/attempt state, `QualityGate`, Source Registry and `ImportV2Service` commit boundaries are reused without a second task or commit system.
- File: deterministic file routes surface stable hard failures and preserve their staged source/baseline for optional Agent assistance.
- Web: Web routes retain login/challenge/private-target/ASR authorization boundaries; Agent assistance receives only the current item's sanitized staged copy and cannot gain browser/network authority.

Evidence: `import_v2_core`, `import_v2_file_*`, `import_v2_web_*`, `import_v2_agent_workspace`, `import_v2_agent_orchestration`.

## Reproduction

Run from the repository root:

```text
npm run check
```

Focused Rust release evidence:

```text
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test import_v2_agent_assistance --test import_v2_agent_candidate --test import_v2_agent_orchestration --test import_v2_agent_tools --test import_v2_agent_workspace --test import_v2_local_agent --test import_v2_byok_assistance
```

Record the exact commit and check output with the release report. Provider-side billing correctness, real hostile-model behavior and OS-specific process semantics outside the tested platforms remain residual operational risks.
