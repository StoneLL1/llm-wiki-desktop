# Import V2 Agent Assistance Implementation Plan

> Historical implementation plan. Import recovery is now local-Agent-only, user-triggered, staging-only, and has no BYOK route; see [`../specs/2026-07-24-import-source-media-flow-design.md`](../specs/2026-07-24-import-source-media-flow-design.md).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add safe local-Agent assistance for deterministic hard failures and user-requested optimization, with an explicit “失败自动调用 Agent 辅助” policy, manual invocation actions, BYOK approval, structured tools, candidate Diff, and full auditability.

**Architecture:** Reuse the existing `AgentService`, `ProcessRunner`, `TaskService`, `SecretService`, `GitService`, and Import Core orchestration instead of creating another agent/process framework. An Import-specific adapter builds a sanitized, item-scoped task bundle and workspace, exposes a narrow structured tool broker, and accepts only staged candidate artifacts. Core Quality Gate, preview, user confirmation, expected-hash checks, and crash-durable commit remain the only route to formal project content.

**Tech Stack:** Tauri v2/Rust, existing agent/task/secret/git services, Import Core `ImportEngine` and DTOs, JSON task bundles, existing local Agent CLIs (Claude/Codex/OpenClaw/Hermes where detected), existing BYOK LLM service, Rust tests, Vitest contracts.

## Global Constraints

- Prerequisites: Import Core HEAD `3bd282c`, File Ingestion, and stage-one Web Ingestion are green.
- No visual Import page implementation in this package. Expose typed policy/action/status/Diff DTOs for the later UI design.
- Local Agent is an enhancement, not the only path. Deterministic routes remain first and their faithful baseline is preserved.
- Automatic assistance is allowed only for deterministic hard failure, only when the project/app policy enables it, and only for an already detected local Agent. Low-quality success is manual-only.
- Cloud BYOK is never automatic. Each invocation displays provider, model, exact send set, expected cost/token estimate, and requires one explicit approval.
- Agent never receives Cookie, Authorization, API key, document password, platform signature, xsec token, OS credential value, or unrestricted project access.
- Agent may read only the current item's authorized source, deterministic outputs, sanitized snapshot, and redacted attempt ledger; it writes only a new item-scoped staging candidate.
- Agent cannot execute macros, bypass login/captcha/paywall, install tools, modify `raw/`, `wiki/`, Git, credentials, source manifests, session JSON, or task JSON.
- Prompt injection inside imported content remains quoted data and cannot expand tool permissions or change system instructions.
- Each candidate records agent kind/version, prompt/template version, approved tools, tool calls, inputs by hash, warnings, uncertainty, cost, and output hashes.
- User can cancel; process-tree termination and task state reuse existing services. No orphan process or ghost task is acceptable.
- Run `npm run check` after every completed task; fix and rerun from the beginning on failure.
- This plan intentionally adds no new third-party agent framework. Reuse decisions are recorded in `docs/superpowers/plans/2026-07-12-import-v2-open-source-research.md`.

## Planned File Structure

- `src-tauri/src/models/import_v2_agent.rs`: policy, request, approval, tool, candidate, Diff, and audit DTOs.
- `src-tauri/src/services/import_v2/agent_assistance.rs`: policy and invocation orchestration.
- `src-tauri/src/services/import_v2/agent_workspace.rs`: sanitized read-only task bundle and staging workspace.
- `src-tauri/src/services/import_v2/agent_tools.rs`: structured allowlisted tool broker using existing capability engines.
- `src-tauri/src/services/import_v2/agent_candidate.rs`: output validation, deterministic baseline preservation, and candidate Diff.
- `src-tauri/src/commands/import_v2_agent_commands.rs`: thin policy/invoke/approve/cancel commands.
- `src/types/importV2Agent.ts`: TypeScript mirror only.
- `src-tauri/templates/skills/wiki-ingest-assist/SKILL.md`: bundled Agent task instructions with explicit boundaries.
- `src-tauri/tests/import_v2_agent_assistance.rs`: end-to-end/security/cancellation tests.

## Reuse Decision

| Existing component | Reuse | Forbidden duplication |
| --- | --- | --- |
| `AgentService` / `ProcessRunner` | detection, typed invocation, stdout/stderr streaming, process-tree cancellation | no second CLI detector or shell runner |
| `TaskService` | task creation, progress, logs, cancellation, typed result reference | no private Agent task store |
| `SecretService` and keyring | BYOK references and provider secrets | no secret value in task bundle/project JSON |
| File/Web capability packs | OCR, browser, media, conversion tools behind typed calls | no Agent-installed package or arbitrary executable |
| `GitService` + Import Core commit | Diff/checkpoint/formal write | Agent never runs Git or writes formal paths |

---

### Task 1: Freeze Agent Assistance Contracts and Policy Semantics

**Files:**
- Create: `src-tauri/src/models/import_v2_agent.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Modify: `src-tauri/src/models/import_v2.rs`
- Create: `src/types/importV2Agent.ts`
- Modify: `src/types/importV2.ts`
- Create: `src/types/importV2Agent.test.ts`
- Create: `src-tauri/tests/import_v2_agent_contracts.rs`

**Interfaces:**
- Produces `AgentAssistancePolicy`, `AgentAssistanceTrigger`, `AgentRecoveryAction`, `AgentInvocationRequest`, `AgentSendScope`, `AgentToolGrant`, `AgentCandidate`, `AgentCandidateDiff`, `AgentAuditRecord`.
- Extends Core `ImportIssue` with `available_actions` and preserves all existing fields/wire names.

- [ ] **Step 1: Write failing Rust/TypeScript contract tests**

```rust
#[test]
fn balanced_policy_never_auto_invokes_cloud_or_low_quality_success() {
    let p = AgentAssistancePolicy::balanced(true);
    assert!(p.auto_local_on_hard_failure);
    assert!(!p.auto_local_on_quality_warning);
    assert!(!p.auto_byok);
}
```

```ts
expect(AGENT_RECOVERY_ACTIONS).toEqual(["invoke_local_agent", "request_byok", "compare_candidate", "discard_candidate"]);
```

- [ ] **Step 2: Run focused tests and verify RED**

Expected: models and action fields are missing.

- [ ] **Step 3: Implement exact DTOs**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentAssistancePolicy {
    pub auto_local_on_hard_failure: bool,
    pub auto_local_on_quality_warning: bool,
    pub auto_byok: bool,
    pub max_attempts_per_item: u8,
}
```

Default is balanced: auto local hard-failure off until user enables the “失败自动调用 Agent 辅助” setting, warning/optimization manual, BYOK manual.

- [ ] **Step 4: Verify backward compatibility and full check**

Deserialize Core session fixtures that do not contain new fields using explicit defaults; assert TypeScript mirrors Rust.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/models/import_v2_agent.rs src-tauri/src/models/mod.rs src-tauri/src/models/import_v2.rs src/types/importV2Agent.ts src/types/importV2.ts src/types/importV2Agent.test.ts src-tauri/tests/import_v2_agent_contracts.rs
git commit -m "test(import): freeze agent assistance contracts"
```

### Task 2: Persist Policy Without Persisting Secrets

**Files:**
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/services/settings_service.rs`
- Create: `src-tauri/src/commands/import_v2_agent_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/tests/import_v2_agent_policy.rs`

**Interfaces:**
- Commands `get_import_agent_policy_v2` and `set_import_agent_policy_v2` resolve project/app scope through `AppState`.
- Policy stores booleans, selected local agent kind, and attempt budget only; BYOK key remains an opaque `SecretService` reference.

- [ ] **Step 1: Write RED tests for defaults, update, and secret isolation**

Seed a real fake provider key through `SecretService::set`, save policy, read it back, then scan settings/project files for the key and generic secret markers.

- [ ] **Step 2: Implement policy persistence through existing settings facade**

Do not introduce `.app/import-agent-settings.json`; extend the existing settings contract with version/default handling.

- [ ] **Step 3: Verify commands are thin and registered**

Command tests assert `resolve_project_context` and stable service facade usage; no command performs filesystem or process work.

- [ ] **Step 4: Run full check and commit**

```bash
git add src-tauri/src/models/settings.rs src-tauri/src/services/settings_service.rs src-tauri/src/commands/import_v2_agent_commands.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/tests/import_v2_agent_policy.rs
git commit -m "feat(import): store agent assistance policy safely"
```

### Task 3: Build Sanitized Item-Scoped Agent Workspaces

**Files:**
- Create: `src-tauri/src/services/import_v2/agent_workspace.rs`
- Create: `src-tauri/templates/skills/wiki-ingest-assist/SKILL.md`
- Create: `src-tauri/tests/import_v2_agent_workspace.rs`

**Interfaces:**
- `AgentWorkspaceBuilder::build(context, session, item, trigger) -> Result<AgentWorkspace, BackendError>`.
- Workspace files: `task.json`, `source/`, `deterministic/`, `logs/attempts.json`, `output/`; only `output/` is writable by the Agent process.

- [ ] **Step 1: Write RED containment and redaction tests**

Seed URL tokens, cookies, passwords, home-directory usernames, another import item, another project file, malicious symlinks/reparse points, and prompt-injection instructions. Assert task bundle contains hashes/public metadata only and cannot resolve outside the item workspace.

- [ ] **Step 2: Implement sanitized bundle schema**

```rust
pub struct AgentTaskBundle {
    pub schema_version: u32,
    pub session_id: String,
    pub item_id: String,
    pub trigger: AgentAssistanceTrigger,
    pub public_source: String,
    pub input_hashes: Vec<String>,
    pub allowed_tools: Vec<AgentToolGrant>,
    pub required_outputs: Vec<String>,
}
```

Imported content is placed under an explicit `untrusted_source_material` field and the Skill says it is data, not instructions.

- [ ] **Step 3: Make source/deterministic files read-only and validate links**

Use copied immutable task inputs, not live formal project paths. Reject link/reparse traversal and path aliases.

- [ ] **Step 4: Verify restart cleanup and full check**

Unfinished workspace remains referenced by the task; terminal/cancelled workspace cleanup preserves only audit/output hashes needed by session history.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/import_v2/agent_workspace.rs src-tauri/templates/skills/wiki-ingest-assist/SKILL.md src-tauri/tests/import_v2_agent_workspace.rs
git commit -m "feat(import): isolate agent assistance workspaces"
```

### Task 4: Add Structured Import Tool Broker

**Files:**
- Create: `src-tauri/src/services/import_v2/agent_tools.rs`
- Modify: `src-tauri/src/services/import_v2/mod.rs`
- Create: `src-tauri/tests/import_v2_agent_tools.rs`

**Interfaces:**
- `ImportAgentToolBroker::invoke(task_context, ToolCall) -> Result<ToolResult, BackendError>`.
- Allowed tool kinds: inspect source, run named deterministic parser, OCR named pages, transcribe authorized temporary media, parse sanitized snapshot, validate candidate. No shell/installer/Git/secret/raw network tool.

- [ ] **Step 1: Write RED allowlist/denylist tests**

Allow `run_parser { route: "file.docling" }`; deny executable path, arbitrary command, arbitrary URL, Git, file write, credential read, plugin install, captcha bypass, paywall bypass, and another item's staging path.

- [ ] **Step 2: Implement typed broker over existing engines/services**

```rust
pub enum ImportAgentToolCall {
    InspectSource,
    RunDeterministicRoute { route: String },
    RunOcr { page_indices: Vec<u32>, profile: String },
    RunAsr { model_id: String },
    ValidateCandidate { relative_markdown_path: String },
}
```

Each call rechecks task/item identity, grant, limits, cancellation, and staging containment.

- [ ] **Step 3: Persist redacted tool ledger**

Store tool kind, route/model version, started/completed time, outcome, warnings, input/output hashes, and resource use; never store secret-bearing arguments.

- [ ] **Step 4: Run full check and commit**

```bash
git add src-tauri/src/services/import_v2/agent_tools.rs src-tauri/src/services/import_v2/mod.rs src-tauri/tests/import_v2_agent_tools.rs
git commit -m "feat(import): broker structured agent tools"
```

### Task 5: Implement Local Agent Invocation and Cancellation

**Files:**
- Create: `src-tauri/src/services/import_v2/agent_assistance.rs`
- Modify: `src-tauri/src/services/agent_service.rs`
- Modify: `src-tauri/src/services/import_v2/orchestrator.rs`
- Modify: `src-tauri/src/commands/import_v2_agent_commands.rs`
- Create: `src-tauri/tests/import_v2_local_agent.rs`

**Interfaces:**
- `AgentAssistanceService::start_local(context, session_id, item_id, trigger, agent_kind) -> Result<BackendTask, BackendError>`.
- Thin command `start_import_agent_assistance_v2` returns a task immediately; existing task cancellation stops the process tree.

- [ ] **Step 1: Write RED tests with a fake `ProcessRunner`**

Cover detected/undetected agent, no silent install, automatic hard-failure policy, low-quality manual-only, max attempt budget, stdout/stderr redaction, cancellation before start/during tool/during final output, crash, malformed stream, and ghost-task cleanup.

- [ ] **Step 2: Add Import-specific invocation builder to existing `AgentService`**

```rust
pub fn import_assistance_invocation(
    kind: AgentKind,
    workspace: &Path,
    skill_path: &Path,
) -> Result<AgentInvocation, BackendError>;
```

Use argument arrays, never a shell string. The invocation receives no environment secrets beyond the selected CLI's normal environment and no project root outside the isolated workspace.

- [ ] **Step 3: Integrate trigger policy and task references**

On deterministic hard failure, enqueue only when enabled and a local agent is available; otherwise expose the manual action. The original issue/attempt ledger remains intact.

- [ ] **Step 4: Verify process-tree cleanup and full check**

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/import_v2/agent_assistance.rs src-tauri/src/services/agent_service.rs src-tauri/src/services/import_v2/orchestrator.rs src-tauri/src/commands/import_v2_agent_commands.rs src-tauri/tests/import_v2_local_agent.rs
git commit -m "feat(import): run local agent assistance safely"
```

### Task 6: Add Explicit BYOK Approval and Send-Scope Audit

**Files:**
- Modify: `src-tauri/src/services/import_v2/agent_assistance.rs`
- Modify: `src-tauri/src/commands/import_v2_agent_commands.rs`
- Create: `src-tauri/tests/import_v2_byok_assistance.rs`

**Interfaces:**
- `preview_import_byok_scope_v2` returns provider/model, file list with byte/token estimates, public metadata, redactions, and estimated cost.
- `approve_import_byok_assistance_v2` requires approval ID bound to item, scope hash, provider/model, and expiry.

- [ ] **Step 1: Write RED approval tests**

Reject missing/stale/replayed/wrong-item/wrong-scope approval; verify no automatic BYOK path; verify provider key is loaded only inside the call and never added to workspace/log/task/session.

- [ ] **Step 2: Implement scope hash and one-shot approval**

```rust
pub struct AgentSendScope {
    pub approval_id: String,
    pub item_id: String,
    pub provider: String,
    pub model: String,
    pub files: Vec<SendScopeFile>,
    pub estimated_input_tokens: u64,
    pub estimated_cost_micros: Option<u64>,
    pub scope_sha256: String,
    pub expires_at: DateTime<Utc>,
}
```

- [ ] **Step 3: Route through existing LLM/provider services**

Do not add a direct HTTP client or provider SDK inside Import. Preserve current provider timeout, cancellation, and keyring behavior.

- [ ] **Step 4: Run secret scans/full check and commit**

```bash
git add src-tauri/src/services/import_v2/agent_assistance.rs src-tauri/src/commands/import_v2_agent_commands.rs src-tauri/tests/import_v2_byok_assistance.rs
git commit -m "feat(import): require BYOK send-scope approval"
```

### Task 7: Validate Agent Candidates and Generate Diff

**Files:**
- Create: `src-tauri/src/services/import_v2/agent_candidate.rs`
- Modify: `src-tauri/src/services/import_v2/quality_gate.rs`
- Modify: `src-tauri/src/services/import_v2/orchestrator.rs`
- Create: `src-tauri/tests/import_v2_agent_candidate.rs`

**Interfaces:**
- `AgentCandidateService::accept_staged_output(context, session_id, item_id, task_id) -> Result<AgentCandidate, BackendError>`.
- Produces a new candidate artifact set and `AgentCandidateDiff`; deterministic baseline is never overwritten.

- [ ] **Step 1: Write RED output/quality/Diff tests**

Reject missing manifest, outside path, symlink/reparse, changed source snapshot, extra executable, unsafe HTML/URI, secret text, malformed Markdown, unrelated file, and output after cancellation. Require stated tools/warnings/uncertainty and exact hashes.

- [ ] **Step 2: Implement staged manifest validation**

```rust
pub struct AgentCandidateManifest {
    pub markdown_path: String,
    pub asset_paths: Vec<String>,
    pub processing_summary: String,
    pub tools_used: Vec<String>,
    pub uncertainties: Vec<String>,
    pub warnings: Vec<String>,
}
```

- [ ] **Step 3: Generate baseline/current/agent views using existing diff primitives**

Reuse `GitService`/existing Markdown diff rendering and Source Registry baseline. Do not add a second file-diff engine. For user-edited Wiki, produce a three-way candidate and set Core status `NeedsMerge`.

- [ ] **Step 4: Run Quality Gate and preserve both candidates**

Agent result cannot reduce hard safety checks. The candidate shows a clear AI-assisted marker; deterministic faithful baseline remains selectable.

- [ ] **Step 5: Run full check and commit**

```bash
git add src-tauri/src/services/import_v2/agent_candidate.rs src-tauri/src/services/import_v2/quality_gate.rs src-tauri/src/services/import_v2/orchestrator.rs src-tauri/tests/import_v2_agent_candidate.rs
git commit -m "feat(import): validate and diff agent candidates"
```

### Task 8: Integrate Manual Actions, Automatic Failure Fallback, and Recovery

**Files:**
- Modify: `src-tauri/src/services/import_v2/orchestrator.rs`
- Modify: `src-tauri/src/commands/import_v2_agent_commands.rs`
- Modify: `src-tauri/src/models/import_v2.rs`
- Modify: `src/types/importV2.ts`
- Create: `src-tauri/tests/import_v2_agent_orchestration.rs`

**Interfaces:**
- Commands: start local assistance, preview/approve BYOK, select candidate, discard candidate.
- Core issues expose “失败自动调用 Agent 辅助” setting state and manual action availability without embedding UI text in backend codes.

- [ ] **Step 1: Write end-to-end RED state tests**

Cover hard deterministic failure with auto enabled/disabled, low-quality success, user manual click, no agent installed, retry after Agent failure, Agent candidate warning, cancellation, restart while Agent active, stale project switch, and partial success in a multi-item session.

- [ ] **Step 2: Implement explicit state transitions**

```text
failed -> agent_assistance task -> validating -> preview_ready | needs_merge | failed
preview_ready deterministic -> manual optimize -> validating -> preview_ready(agent candidate)
```

Do not add a new persisted Core status unless contract tests prove backward compatibility; represent active assistance through the item's typed task reference and attempt record.

- [ ] **Step 3: Reconcile interrupted tasks safely**

On restart, local Agent/BYOK remains waiting or failed; never auto-resubmit cloud calls. Completed staged output is revalidated before display.

- [ ] **Step 4: Verify project-switch guards for later frontend consumers**

Task facts may upsert globally; stale-project results must not imply drawer/navigation/toast takeover. This package supplies project ID, session ID, item ID typed references for the existing frontend workflow.

- [ ] **Step 5: Run full check and commit**

```bash
git add src-tauri/src/services/import_v2/orchestrator.rs src-tauri/src/commands/import_v2_agent_commands.rs src-tauri/src/models/import_v2.rs src/types/importV2.ts src-tauri/tests/import_v2_agent_orchestration.rs
git commit -m "feat(import): orchestrate agent fallback actions"
```

### Task 9: Agent Assistance Release Gates

**Files:**
- Create: `src-tauri/tests/import_v2_agent_assistance.rs`
- Create: `docs/qa/import-v2-agent-assistance.md`
- Modify: `SPEC/progress.txt`

**Interfaces:**
- Produces release evidence only.

- [ ] **Step 1: Run threat and secret corpus**

Cover prompt injection requesting secrets/network/Git/writes, malicious tool calls, link traversal, command injection, environment leakage, output secret echo, another item/project access, captcha/paywall bypass requests, and Agent-created executable artifacts.

- [ ] **Step 2: Run lifecycle/recovery corpus**

Kill before process start, during each tool, during candidate write, after output before validation, during validation, and during cancellation. Restart twice and assert no orphan process/workspace/task, no duplicate BYOK charge, and no formal content mutation.

- [ ] **Step 3: Run product behavior corpus**

Verify automatic local hard-failure policy, manual low-quality optimization, manual failure action, deterministic baseline retention, Diff, three-way merge candidate, candidate discard, partial success, and exact audit records.

- [ ] **Step 4: Run final check and dual review**

Run `npm run check`. Review A checks product/policy/Core integration. Review B starts fresh and attacks privilege expansion, secrets, task identity, process cleanup, output validation, and empty security assertions. Fix findings and rerun from the beginning.

- [ ] **Step 5: Record progress and commit**

```bash
git add src-tauri/tests/import_v2_agent_assistance.rs docs/qa/import-v2-agent-assistance.md SPEC/progress.txt
git commit -m "test(import): certify agent assistance boundaries"
```

## Self-Review Result

- Spec coverage: balanced autonomy, automatic local hard-failure option, manual failure/optimization actions, BYOK approval, tool access, secret denial, staging-only writes, deterministic baseline, Diff, three-way merge, audit, cancellation/recovery, and no silent installs are assigned to Tasks 1–9.
- Placeholder scan: every task provides files, exact interfaces, RED cases, implementation boundary, checks, and commit scope.
- Type/API consistency: Agent work is represented by existing Core item/task/attempt contracts; candidates return through `QualityGate` and `ImportV2Service::commit_items_cancellable`; settings/commands use existing facades.
- Dependency order: contracts/policy/workspace/tools precede local/BYOK invocation; candidate validation precedes orchestration; release certification is last.
