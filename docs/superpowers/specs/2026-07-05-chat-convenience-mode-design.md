# Chat Convenience Mode Design

## Goal

Add a project-scoped Chat convenience mode that lets an authorized Chat Agent make small, low-risk wiki edits directly in the real project, while preserving rollback, auditability, and hard stops for destructive or out-of-scope changes.

This design intentionally prioritizes user convenience over strict pre-execution sandboxing. Safety is provided by project-level opt-in, Git checkpointing before each write-intent Chat turn, post-write diff audit, and automatic rollback for hard violations.

## User Intent Decisions

The approved product choices are:

- Convenience mode is enabled per project, with one explicit confirmation on this machine.
- The authorization is stored in global app settings, not in project `.app/settings.json`.
- Normal Chat questions remain read-only.
- Chat enters write mode only when the user expresses a write intent, such as "整理", "优化", "改一下", "补一下", "保存", "生成页面", "apply", "save", "edit", "organize", or similar wording.
- Ambiguous phrasing is interpreted by verb strength: "看看", "分析", "建议", "explain", "review" stay read-only; "整理", "优化", "改", "补", "保存", "生成" enter write mode. If still ambiguous, Chat asks one clarifying question.
- Convenience mode may lightly rewrite existing Markdown pages under `wiki/`.
- `wiki/index.md`, `wiki/overview.md`, and `wiki/log.md` may be modified, and count toward the file limit.
- Each write-intent Chat turn creates one Git checkpoint before Agent execution.
- The Agent writes directly in the real project root, not in a candidate workspace.
- All installed Agents are eligible in convenience mode: Claude, Codex, OpenClaw, and Hermes.
- Permission boundaries are best-effort for Agents that do not expose strong sandbox controls.
- Network reading is allowed, but downloading, installing, and executing remote scripts are forbidden.
- Shell usage is allowed only for read-only operations such as listing files, searching, counting, and reading context.
- Successful writes show a summary, affected paths, checkpoint, rollback entry, and collapsed diff.
- The first version exposes only "rollback last Chat automatic edit".

## Allowed Changes

A convenience edit passes audit when all of the following are true:

- Only Markdown files under `wiki/` changed.
- At most 3 wiki files changed.
- Each changed file has at most 2000 changed characters.
- No file was deleted.
- No path under `raw/sources/` changed.
- No project configuration file changed, including `.app/settings.json` and `.app/agent-config.json`.
- No external-edit conflict is detected between the pre-Agent baseline and post-Agent audit.

The changed-character threshold is calculated per file from the post-write diff. The implementation may use line-diff additions/deletions or a normalized text delta, but it must be deterministic and covered by tests.

## Hard Violations

Hard violations trigger automatic rollback to the pre-turn checkpoint. The Agent's answer text is discarded, and Chat persists only a system rollback report.

Hard violations are:

- Any file deletion.
- Any change under `raw/sources/`.
- Any change to project configuration files, including `.app/settings.json` and `.app/agent-config.json`.
- Any write outside `wiki/`.
- Any external-edit conflict detected during audit.
- Any audit failure where affected paths cannot be trusted.

The rollback report must include the reason, affected paths when known, the checkpoint hash, and whether rollback succeeded.

## Soft Violations

Soft violations keep the Agent's modifications in place, but require user action before another convenience write may run in the same Chat session.

Soft violations are:

- More than 3 wiki Markdown files changed.
- A wiki Markdown file changed by more than 2000 characters.

The UI shows a confirmation bar with "Keep changes" and "Rollback" actions. Read-only Chat remains allowed while this state is pending. A second convenience write is blocked until the user chooses.

## Execution Flow

1. The frontend sends a Chat message with the current route preference and the current convenience-mode state.
2. The backend verifies that convenience mode is enabled for this project on this machine.
3. The backend classifies the message intent:
   - read-only intent: use the existing Chat flow.
   - write intent: use the convenience edit flow.
   - ambiguous intent: return a recoverable clarification response.
4. For a write intent, the backend records a pre-Agent baseline:
   - current Git HEAD.
   - current working-tree diff or file status.
   - relevant file hashes needed to detect external edits.
5. The backend creates one Git checkpoint for this Chat turn.
6. The Agent runs in the real project root with a prompt that explicitly states:
   - it may lightly edit Markdown under `wiki/`.
   - it must not delete files.
   - it must not modify `raw/sources/`.
   - it must not modify project configuration files.
   - shell commands must be read-only.
   - network use is read-only and must not download, install, or run remote scripts.
   - it must summarize changed files and intent.
7. After Agent completion, the backend audits the diff since the checkpoint.
8. The backend classifies the audit as pass, soft violation, or hard violation.
9. The backend persists the Chat result:
   - pass: assistant answer plus convenience metadata.
   - soft violation: assistant answer plus pending metadata.
   - hard violation: system rollback report only.

## Agent Invocation Policy

Convenience mode intentionally supports every installed Agent. Strong controls should be used where available, but lack of strong controls does not block convenience mode.

Expected first-version behavior:

- Claude: use non-interactive mode and allow read/write tools needed for wiki Markdown edits. Prompt clearly forbids out-of-scope writes and destructive operations.
- Codex: use a workspace-write profile in the project root. Avoid project-rule interference where possible if it would conflict with Chat behavior, but preserve enough context for local wiki edits.
- OpenClaw and Hermes: use their existing non-interactive invocation style, with the convenience prompt providing the main boundary.

Post-write audit and checkpoint rollback are the real enforcement layer for unsupported or weakly constrained Agents.

## Data Model

Global app settings gain local-machine convenience authorization records:

```ts
type ChatConvenienceAuthorization = {
  enabled: boolean;
  confirmedAt: string;
  projectId: string;
  rootPathFingerprint: string;
};
```

The key should combine `projectId` and a stable fingerprint of the project root path. This avoids accidentally sharing authorization across copied or renamed projects. The raw project path should not be used as the only durable key if a hash/fingerprint is already available in settings patterns.

Chat messages may carry convenience metadata:

```ts
type ChatConvenienceEditStatus =
  | "applied"
  | "soft_violation_pending"
  | "kept_after_soft_violation"
  | "rolled_back"
  | "rollback_failed";

type ChatConvenienceEdit = {
  status: ChatConvenienceEditStatus;
  checkpointHash: string;
  affectedPaths: string[];
  diffSummary: string;
  diffText?: string;
  violationReason?: string;
  rollbackTaskId?: string;
};
```

This metadata belongs to the Chat message/session JSON because it describes the answer and user-visible outcome. Authorization stays global because it is a local-machine preference.

## UI Design

Chat view:

- Add a compact convenience-mode toggle or badge near the route selector.
- When disabled, clicking it opens a confirmation dialog.
- When enabled, show a small "Convenience mode on" badge and a close/disable affordance.
- If a write-intent message is sent while disabled, offer to enable convenience mode instead of silently falling back to write behavior.

Confirmation dialog copy must state:

- Agent may directly modify `wiki/` Markdown files.
- A Git checkpoint is created before every write-intent Chat turn.
- Deletes, raw source changes, config changes, and external conflicts are rolled back.
- Larger edits are kept pending until the user chooses keep or rollback.
- This authorization is local to this machine and can be revoked in Settings.

Assistant message result:

- Show changed-file summary.
- Show affected paths.
- Show checkpoint hash.
- Provide a collapsed diff region.
- Provide "Rollback last Chat automatic edit".

Soft violation UI:

- Show a warning bar under the affected assistant message.
- Actions: "Keep changes" and "Rollback".
- Block further convenience writes in that session until the warning is resolved.

Hard violation UI:

- Do not show the Agent's original answer.
- Show a system rollback report with reason, affected paths, checkpoint hash, and rollback status.

Settings UI:

- Add a local-machine "Chat convenience mode authorizations" section.
- Show the current project's authorization state.
- Allow revoking the current project authorization.
- Allow revoking all convenience authorizations.

## Backend Boundaries

The React UI must not perform filesystem, Git, Agent, or diff logic. It only sends typed requests and renders typed results.

Backend responsibilities:

- Resolve project context.
- Read/write global authorization settings.
- Classify Chat intent.
- Create Git checkpoint.
- Build Agent prompt and invocation.
- Run Agent task.
- Audit diff.
- Roll back hard violations.
- Persist Chat message metadata.
- Expose typed commands for keep/rollback soft violations and rollback last automatic edit.

The existing high-risk rules remain intact for non-convenience workflows. Convenience mode is a Chat-specific workflow with its own checkpoint-before-write and audit-after-write contract.

## Failure Modes

- Checkpoint creation fails: do not run the Agent; return a recoverable error.
- Agent spawn fails: no assistant edit result is saved.
- Agent exits with failure after changing files: audit the diff; hard violations roll back, soft violations require user choice, allowed changes may be reported as partial success only if the diff passes.
- Audit cannot determine changed paths: treat as hard violation and attempt rollback.
- Rollback fails: persist a system message with `rollback_failed`, checkpoint hash, affected paths when available, and instructions to recover through Git.
- User cancels during Agent execution: stop the task; if file changes exist, audit and handle them before marking the Chat turn complete.
- User has unresolved soft violation: allow read-only Chat, block new convenience writes until resolved.

## Testing Strategy

Backend tests:

- Intent classifier treats normal questions as read-only.
- Intent classifier treats edit/save/organize/generate wording as write intent.
- Ambiguous wording returns clarification.
- Authorization persists to global settings only.
- Project `.app/settings.json` is not modified when enabling convenience mode.
- Checkpoint is created before Agent execution.
- Checkpoint failure prevents Agent execution.
- Passing audit accepts up to 3 wiki Markdown files with each file under 2000 changed characters.
- Soft violation is produced for more than 3 wiki files.
- Soft violation is produced for one wiki file over 2000 changed characters.
- Hard violation rolls back after file deletion.
- Hard violation rolls back after `raw/sources/` modification.
- Hard violation rolls back after `.app/settings.json` or `.app/agent-config.json` modification.
- Hard violation rolls back after writes outside `wiki/`.
- External-edit conflict triggers hard violation.
- Rollback-last command targets only the most recent Chat automatic edit.
- CJK and Unicode wiki paths survive audit and rollback.
- Claude, Codex, OpenClaw, and Hermes convenience invocations are constructible.

Frontend tests:

- Convenience toggle opens first-use confirmation.
- Enabled state displays in Chat.
- Revoking authorization disables the Chat badge.
- Applied edit renders summary, paths, checkpoint, collapsed diff, and rollback action.
- Soft violation renders keep/rollback actions and blocks another convenience write.
- Hard violation renders rollback report and not the Agent answer text.

Required checks after implementation:

- `npm run test`
- `npm run lint`
- `npm run build` or equivalent import-path verification
- `cargo check --lib --tests`
- targeted Rust tests for Chat convenience audit and rollback
- source scan confirming no unintended `console.log`

## Out Of Scope

The first version will not include:

- Arbitrary historical checkpoint rollback.
- Candidate-workspace mode.
- Strong sandbox abstraction across all Agent CLIs.
- Multi-edit dependency graph.
- Full audit center.
- Automatic high-risk mode.
- Writing or changing raw sources.
- Silent deletion.
- Silent project configuration changes.

## Open Implementation Notes

- The implementation should prefer adding a focused Chat convenience service or module rather than expanding `chat_commands.rs` into a large orchestration file.
- Diff audit should be deterministic and unit-tested without real Agent CLIs.
- Real Agent execution should remain behind the existing `ProcessRunner` abstraction for testability.
- Existing Chat read-only behavior should remain the fallback path when convenience mode is off or intent is read-only.
