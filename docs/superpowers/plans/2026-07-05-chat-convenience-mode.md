# Chat Convenience Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement project-level Chat convenience mode so authorized write-intent Chat turns can directly make small wiki edits with checkpoint, audit, rollback, and UI feedback.

**Architecture:** Add a focused backend `ChatConvenienceService` that owns authorization checks, intent classification, checkpoint orchestration, post-write diff audit, and rollback decisions. Keep React as a typed IPC client that renders authorization state, write outcomes, soft-violation actions, and last-edit rollback controls. Existing read-only Chat flow remains the default path.

**Tech Stack:** Rust/Tauri v2 services and commands, existing GitService/AgentService/TaskService/FileStore abstractions, React 19 + TypeScript + Zustand, existing shadcn/Tailwind token style, Vitest and Rust unit tests.

---

## File Structure

**Create**

- `src-tauri/src/services/chat_convenience_service.rs` - intent classification, prompt suffix, diff audit, soft/hard violation classification, rollback orchestration helpers.
- `src/features/chat/ChatConveniencePanel.tsx` - Chat convenience toggle, first-use confirmation, applied/soft/hard result rendering.
- `src/features/chat/ChatConveniencePanel.test.tsx` - UI behavior tests for authorization and result rendering.

**Modify**

- `src-tauri/src/services/mod.rs` - export `ChatConvenienceService`.
- `src-tauri/src/app_state.rs` - add `chat_convenience_service`.
- `src-tauri/src/models/settings.rs` - add global convenience authorization records.
- `src-tauri/src/services/settings_service.rs` - read/write local authorization without touching project settings.
- `src-tauri/src/commands/settings_commands.rs` - add get/set/revoke convenience authorization commands.
- `src-tauri/src/models/chat.rs` - add convenience request fields, message metadata, and command DTOs.
- `src-tauri/src/services/agent_service.rs` - add convenience Agent invocation profile for all installed Agents.
- `src-tauri/src/services/git_service.rs` - add rollback/diff helper methods needed by audit flow.
- `src-tauri/src/commands/chat_commands.rs` - route write-intent turns through convenience flow and add keep/rollback commands.
- `src/types/settings.ts` - add convenience authorization types.
- `src/types/chat.ts` - add convenience metadata, request flags, and keep/rollback DTOs.
- `src/stores/settingsStore.ts` - load and mutate convenience authorization state.
- `src/stores/chatStore.ts` - send convenience flag, keep/rollback soft violation, rollback last edit.
- `src/features/chat/ChatView.tsx` - mount convenience controls and result UI.
- `src/features/settings/SecuritySettings.tsx` - add authorization revocation controls.
- `src/i18n/locales/en.json` and `src/i18n/locales/zh-CN.json` - add user-facing copy.
- `SPEC/progress.txt` - append completion record after implementation.
- `SPEC/gotchas.txt` - only append if a subtle recurring issue is encountered.

---

## Task 1: Backend Authorization Model And Commands

**Files:**
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/services/settings_service.rs`
- Modify: `src-tauri/src/commands/settings_commands.rs`
- Modify: `src/types/settings.ts`
- Modify: `src/stores/settingsStore.ts`

- [ ] **Step 1: Write failing Rust tests for global-only authorization**

Add tests in `src-tauri/src/services/settings_service.rs`:

```rust
#[test]
fn chat_convenience_authorization_is_global_only() {
    let (context, root, config_dir) = tmp_paths("chat-convenience-auth");
    let service = SettingsService::with_config_dir(config_dir.clone());

    let saved = service
        .set_chat_convenience_authorization(&context, true)
        .unwrap();

    assert!(saved.enabled);
    assert_eq!(saved.project_id, context.project_id);
    assert!(saved.root_path_fingerprint.len() >= 16);

    let global: serde_json::Value = FileStore
        .read_json_file(&config_dir.join("settings.json"))
        .unwrap();
    assert!(global["chatConvenienceAuthorizations"].is_array());
    assert!(!context.resolve_project_path(".app/settings.json").unwrap().exists());

    let loaded = service
        .get_chat_convenience_authorization(&context)
        .unwrap();
    assert!(loaded.enabled);

    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(config_dir).unwrap();
}

#[test]
fn chat_convenience_authorization_can_be_revoked_for_project() {
    let (context, root, config_dir) = tmp_paths("chat-convenience-revoke");
    let service = SettingsService::with_config_dir(config_dir.clone());

    service
        .set_chat_convenience_authorization(&context, true)
        .unwrap();
    let revoked = service
        .set_chat_convenience_authorization(&context, false)
        .unwrap();

    assert!(!revoked.enabled);
    assert!(!service
        .get_chat_convenience_authorization(&context)
        .unwrap()
        .enabled);

    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(config_dir).unwrap();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
cargo test --lib --no-default-features services::settings_service::tests::chat_convenience -- --nocapture
```

Expected: fail because the authorization model and service methods do not exist.

- [ ] **Step 3: Add settings model types**

In `src-tauri/src/models/settings.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatConvenienceAuthorization {
    pub enabled: bool,
    pub confirmed_at: String,
    pub project_id: String,
    pub root_path_fingerprint: String,
}
```

Add this field to `GlobalSettingsFile` only:

```rust
#[serde(default)]
pub chat_convenience_authorizations: Vec<ChatConvenienceAuthorization>,
```

Update `Default for GlobalSettingsFile`, `Settings`, `Default for Settings`, `apply_global`, and `to_global_file` if the app keeps a merged view on `Settings`. Do not add the field to `ProjectSettingsFile` or `to_project_file`.

- [ ] **Step 4: Add SettingsService authorization methods**

In `src-tauri/src/services/settings_service.rs`, add:

```rust
pub fn get_chat_convenience_authorization(
    &self,
    context: &ProjectContext,
) -> Result<ChatConvenienceAuthorization, BackendError> {
    let fingerprint = project_root_fingerprint(&context.root);
    let global = self.read_global_settings()?;
    Ok(global
        .chat_convenience_authorizations
        .into_iter()
        .find(|item| item.project_id == context.project_id && item.root_path_fingerprint == fingerprint)
        .unwrap_or(ChatConvenienceAuthorization {
            enabled: false,
            confirmed_at: String::new(),
            project_id: context.project_id.clone(),
            root_path_fingerprint: fingerprint,
        }))
}

pub fn set_chat_convenience_authorization(
    &self,
    context: &ProjectContext,
    enabled: bool,
) -> Result<ChatConvenienceAuthorization, BackendError> {
    let store = FileStore;
    store.ensure_absolute_dir(&self.config_dir)?;
    let mut global = self.read_global_settings()?;
    let fingerprint = project_root_fingerprint(&context.root);
    global.chat_convenience_authorizations.retain(|item| {
        !(item.project_id == context.project_id && item.root_path_fingerprint == fingerprint)
    });
    let auth = ChatConvenienceAuthorization {
        enabled,
        confirmed_at: if enabled {
            crate::utils::time_utils::now_rfc3339()
        } else {
            String::new()
        },
        project_id: context.project_id.clone(),
        root_path_fingerprint: fingerprint,
    };
    if enabled {
        global.chat_convenience_authorizations.push(auth.clone());
    }
    store.write_json_atomic_absolute(&self.global_settings_path(), &global)?;
    Ok(auth)
}

pub fn revoke_all_chat_convenience_authorizations(&self) -> Result<(), BackendError> {
    let store = FileStore;
    store.ensure_absolute_dir(&self.config_dir)?;
    let mut global = self.read_global_settings()?;
    global.chat_convenience_authorizations.clear();
    store.write_json_atomic_absolute(&self.global_settings_path(), &global)
}
```

Add helper:

```rust
fn project_root_fingerprint(path: &std::path::Path) -> String {
    use std::hash::{Hash, Hasher};
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
```

- [ ] **Step 5: Add Tauri command DTOs and commands**

In `src-tauri/src/models/settings.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetChatConvenienceAuthorizationRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub enabled: bool,
}
```

In `src-tauri/src/commands/settings_commands.rs`, add commands:

```rust
#[tauri::command]
pub fn get_chat_convenience_authorization(
    state: State<'_, AppState>,
    request: SettingsProjectRequest,
) -> Result<ChatConvenienceAuthorization, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .settings_service
        .get_chat_convenience_authorization(&context)
}

#[tauri::command]
pub fn set_chat_convenience_authorization(
    state: State<'_, AppState>,
    request: SetChatConvenienceAuthorizationRequest,
) -> Result<ChatConvenienceAuthorization, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .settings_service
        .set_chat_convenience_authorization(&context, request.enabled)
}

#[tauri::command]
pub fn revoke_all_chat_convenience_authorizations(
    state: State<'_, AppState>,
) -> Result<(), BackendError> {
    state
        .settings_service
        .revoke_all_chat_convenience_authorizations()
}
```

Also register the commands in the existing Tauri command handler if `lib.rs` lists commands explicitly.

- [ ] **Step 6: Mirror TypeScript types and store methods**

In `src/types/settings.ts`, add:

```ts
export interface ChatConvenienceAuthorization {
  enabled: boolean;
  confirmedAt: string;
  projectId: string;
  rootPathFingerprint: string;
}
```

In `src/stores/settingsStore.ts`, add state and methods:

```ts
chatConvenienceAuthorization: ChatConvenienceAuthorization | null;
loadChatConvenienceAuthorization: (projectId: string, projectRootPath: string) => Promise<ChatConvenienceAuthorization | null>;
setChatConvenienceAuthorization: (projectId: string, projectRootPath: string, enabled: boolean) => Promise<ChatConvenienceAuthorization | null>;
revokeAllChatConvenienceAuthorizations: () => Promise<void>;
```

Use `invoke("get_chat_convenience_authorization", { request: { projectId, projectRootPath } })`, `invoke("set_chat_convenience_authorization", { request: { projectId, projectRootPath, enabled } })`, and `invoke("revoke_all_chat_convenience_authorizations")`.

- [ ] **Step 7: Run Task 1 tests**

Run:

```powershell
cargo test --lib --no-default-features services::settings_service::tests::chat_convenience -- --nocapture
npm run test -- settingsStore
```

Expected: Rust tests pass. If no `settingsStore` test file exists yet, add focused tests or note that frontend store coverage lands in Task 5.

---

## Task 2: Convenience Models, Intent Classifier, And Audit Service

**Files:**
- Create: `src-tauri/src/services/chat_convenience_service.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/models/chat.rs`

- [ ] **Step 1: Write failing service tests**

Create `chat_convenience_service.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_read_only_write_and_ambiguous_intents() {
        assert_eq!(classify_chat_intent("分析一下这页的问题"), ChatIntent::ReadOnly);
        assert_eq!(classify_chat_intent("帮我整理这页并补摘要"), ChatIntent::Write);
        assert_eq!(classify_chat_intent("save this answer as a page"), ChatIntent::Write);
        assert_eq!(classify_chat_intent("这页有点乱，帮我看看"), ChatIntent::ReadOnly);
        assert_eq!(classify_chat_intent("帮我处理一下"), ChatIntent::Ambiguous);
    }

    #[test]
    fn audit_accepts_three_small_wiki_markdown_changes() {
        let report = audit_changed_paths(
            vec![
                ChangedFile::modified("wiki/a.md", 100),
                ChangedFile::modified("wiki/index.md", 100),
                ChangedFile::modified("wiki/log.md", 100),
            ],
        );
        assert_eq!(report.status, ConvenienceAuditStatus::Passed);
    }

    #[test]
    fn audit_soft_violates_large_or_many_wiki_changes() {
        let many = audit_changed_paths(vec![
            ChangedFile::modified("wiki/a.md", 10),
            ChangedFile::modified("wiki/b.md", 10),
            ChangedFile::modified("wiki/c.md", 10),
            ChangedFile::modified("wiki/d.md", 10),
        ]);
        assert_eq!(many.status, ConvenienceAuditStatus::SoftViolation);

        let large = audit_changed_paths(vec![ChangedFile::modified("wiki/a.md", 2001)]);
        assert_eq!(large.status, ConvenienceAuditStatus::SoftViolation);
    }

    #[test]
    fn audit_hard_violates_delete_raw_config_and_outside_wiki() {
        for change in [
            ChangedFile::deleted("wiki/a.md"),
            ChangedFile::modified("raw/sources/pdfs/a.pdf", 10),
            ChangedFile::modified(".app/settings.json", 10),
            ChangedFile::modified("purpose.md", 10),
        ] {
            let report = audit_changed_paths(vec![change]);
            assert_eq!(report.status, ConvenienceAuditStatus::HardViolation);
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
cargo test --lib --no-default-features services::chat_convenience_service::tests -- --nocapture
```

Expected: fail because module/types are incomplete.

- [ ] **Step 3: Define model types in `models/chat.rs`**

Add:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatConvenienceEditStatus {
    Applied,
    SoftViolationPending,
    KeptAfterSoftViolation,
    RolledBack,
    RollbackFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatConvenienceEdit {
    pub status: ChatConvenienceEditStatus,
    pub checkpoint_hash: Option<String>,
    pub affected_paths: Vec<String>,
    pub diff_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub violation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_task_id: Option<String>,
}
```

Add to `ChatMessage`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub convenience_edit: Option<ChatConvenienceEdit>,
```

Add to `SendChatMessageRequest`:

```rust
#[serde(default)]
pub convenience_enabled: bool,
```

Add command DTOs:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveChatConvenienceEditRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub message_id: String,
    pub keep: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackLastChatConvenienceEditRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
}
```

Update serialization tests in `models/chat.rs` for `convenienceEdit`.

- [ ] **Step 4: Implement `ChatConvenienceService`**

Create:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatIntent {
    ReadOnly,
    Write,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvenienceAuditStatus {
    Passed,
    SoftViolation,
    HardViolation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangedFileKind {
    Modified,
    Added,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub kind: ChangedFileKind,
    pub changed_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvenienceAuditReport {
    pub status: ConvenienceAuditStatus,
    pub affected_paths: Vec<String>,
    pub diff_summary: String,
    pub violation_reason: Option<String>,
}

#[derive(Default)]
pub struct ChatConvenienceService;
```

Implement:

```rust
pub fn classify_chat_intent(input: &str) -> ChatIntent { /* keyword classifier */ }
pub fn audit_changed_paths(changes: Vec<ChangedFile>) -> ConvenienceAuditReport { /* rules */ }
pub fn convenience_prompt_suffix() -> &'static str { /* policy text */ }
```

Keyword sets should include Chinese and English terms from the spec. Keep classifier deterministic and conservative.

- [ ] **Step 5: Register the service**

In `src-tauri/src/services/mod.rs`:

```rust
mod chat_convenience_service;
pub use chat_convenience_service::ChatConvenienceService;
```

In `src-tauri/src/app_state.rs`, import and add:

```rust
pub chat_convenience_service: ChatConvenienceService,
```

- [ ] **Step 6: Run Task 2 tests**

Run:

```powershell
cargo test --lib --no-default-features services::chat_convenience_service::tests models::chat::tests -- --nocapture
```

Expected: all pass.

---

## Task 3: Git Diff, Audit Inputs, And Rollback Helpers

**Files:**
- Modify: `src-tauri/src/services/git_service.rs`
- Modify: `src-tauri/src/models/git.rs`
- Modify: `src-tauri/src/services/chat_convenience_service.rs`

- [ ] **Step 1: Write failing Git helper tests**

In `git_service.rs` tests, add:

```rust
#[test]
fn changed_files_since_head_reports_status_and_changed_chars() {
    let root = unique_temp_dir("changed-files");
    let context = ProjectContext::new("project-1", root.clone());
    fs::create_dir_all(root.join("wiki")).unwrap();
    fs::write(root.join("wiki/a.md"), "one\n").unwrap();
    let service = GitService;
    service.initialize_repository(&context, "baseline").unwrap();

    fs::write(root.join("wiki/a.md"), "one\ntwo\n").unwrap();
    fs::write(root.join("wiki/b.md"), "new page\n").unwrap();

    let changes = service.changed_files_since_head(&context).unwrap();

    assert!(changes.iter().any(|c| c.path == "wiki/a.md" && c.changed_chars > 0));
    assert!(changes.iter().any(|c| c.path == "wiki/b.md" && c.changed_chars > 0));

    fs::remove_dir_all(root).ok();
}

#[test]
fn rollback_to_head_restores_worktree_after_agent_changes() {
    let root = unique_temp_dir("rollback");
    let context = ProjectContext::new("project-1", root.clone());
    fs::create_dir_all(root.join("wiki")).unwrap();
    fs::write(root.join("wiki/a.md"), "one\n").unwrap();
    let service = GitService;
    service.initialize_repository(&context, "baseline").unwrap();

    fs::write(root.join("wiki/a.md"), "changed\n").unwrap();
    fs::write(root.join("wiki/b.md"), "new\n").unwrap();

    service.rollback_worktree_to_head(&context).unwrap();

    assert_eq!(fs::read_to_string(root.join("wiki/a.md")).unwrap(), "one\n");
    assert!(!root.join("wiki/b.md").exists());

    fs::remove_dir_all(root).ok();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
cargo test --lib --no-default-features services::git_service::tests::changed_files_since_head_reports_status_and_changed_chars services::git_service::tests::rollback_to_head_restores_worktree_after_agent_changes -- --nocapture
```

Expected: fail because helpers do not exist.

- [ ] **Step 3: Add git model type**

In `models/git.rs`, add:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitChangedFileKind {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitChangedFile {
    pub path: String,
    pub kind: GitChangedFileKind,
    pub changed_chars: usize,
}
```

- [ ] **Step 4: Implement GitService helpers**

In `git_service.rs`, add:

```rust
pub fn changed_files_since_head(
    &self,
    context: &ProjectContext,
) -> Result<Vec<GitChangedFile>, BackendError> {
    let raw = run_git(context, &["status", "--porcelain", "-uall"])?;
    let numstat = run_git(context, &["diff", "--numstat", "HEAD", "--"]).unwrap_or_default();
    let mut changes = parse_status_with_numstat(&raw, &numstat);
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

pub fn diff_since_head(&self, context: &ProjectContext) -> Result<GitDiff, BackendError> {
    let affected_paths = self
        .changed_files_since_head(context)?
        .into_iter()
        .map(|change| change.path)
        .collect();
    let body = run_git(context, &["diff", "--no-ext-diff", "HEAD", "--"]).unwrap_or_default();
    Ok(GitDiff {
        markdown: format!("```diff\n{}\n```", body.trim_end()),
        affected_paths,
    })
}

pub fn rollback_worktree_to_head(&self, context: &ProjectContext) -> Result<(), BackendError> {
    run_git(context, &["reset", "--hard", "HEAD"])?;
    run_git(context, &["clean", "-fd", "--", "wiki"])?;
    Ok(())
}
```

Important: `clean -fd -- wiki` removes untracked wiki files only. If a hard violation created untracked files outside wiki, inspect whether `git clean -fd -- .` is needed after confirming the target path stays inside the project root. Do not delete arbitrary computed paths without path verification.

- [ ] **Step 5: Connect Git changes to audit service**

In `chat_convenience_service.rs`, add conversion:

```rust
pub fn audit_git_changes(changes: &[GitChangedFile]) -> ConvenienceAuditReport {
    audit_changed_paths(
        changes
            .iter()
            .map(|change| ChangedFile {
                path: change.path.clone(),
                kind: match change.kind {
                    GitChangedFileKind::Deleted => ChangedFileKind::Deleted,
                    GitChangedFileKind::Added => ChangedFileKind::Added,
                    GitChangedFileKind::Modified | GitChangedFileKind::Renamed => ChangedFileKind::Modified,
                },
                changed_chars: change.changed_chars,
            })
            .collect(),
    )
}
```

- [ ] **Step 6: Run Task 3 tests**

Run:

```powershell
cargo test --lib --no-default-features services::git_service::tests services::chat_convenience_service::tests -- --nocapture
```

Expected: pass.

---

## Task 4: Agent Convenience Invocation And Chat Command Integration

**Files:**
- Modify: `src-tauri/src/services/agent_service.rs`
- Modify: `src-tauri/src/commands/chat_commands.rs`
- Modify: `src-tauri/src/services/chat_service.rs`
- Modify: `src-tauri/src/models/chat.rs`

- [ ] **Step 1: Write failing Agent invocation tests**

In `agent_service.rs` tests, add:

```rust
#[test]
fn convenience_chat_invocation_supports_all_agents_from_project_root() {
    let workspace = std::env::temp_dir().join(format!(
        "llm-wiki-convenience-root-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(workspace.join("wiki")).unwrap();

    for kind in AgentKind::ALL {
        let invocation = AgentService::chat_convenience_invocation(kind, &workspace, "prompt")
            .expect("all installed agent kinds should have a convenience profile");
        assert_eq!(invocation.cwd, workspace);
        assert!(invocation.args.iter().any(|arg| arg.contains("prompt")) || invocation.stdin.as_deref() == Some("prompt"));
    }

    let _ = std::fs::remove_dir_all(&workspace);
}
```

- [ ] **Step 2: Write failing chat command routing tests**

In `chat_commands.rs` tests, add a pure helper test around a new function:

```rust
#[test]
fn should_use_convenience_flow_only_when_enabled_and_write_intent() {
    assert!(!should_use_convenience_flow(false, ChatIntent::Write));
    assert!(!should_use_convenience_flow(true, ChatIntent::ReadOnly));
    assert!(should_use_convenience_flow(true, ChatIntent::Write));
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run:

```powershell
cargo test --lib --no-default-features services::agent_service::tests::convenience_chat_invocation_supports_all_agents_from_project_root commands::chat_commands::tests::should_use_convenience_flow_only_when_enabled_and_write_intent -- --nocapture
```

Expected: fail because helpers do not exist.

- [ ] **Step 4: Implement `chat_convenience_invocation`**

In `agent_service.rs`, add:

```rust
pub fn chat_convenience_invocation(
    kind: AgentKind,
    workspace: &Path,
    prompt: &str,
) -> Result<AgentInvocation, BackendError> {
    validate_chat_workspace(workspace)?;
    let cwd = workspace.to_path_buf();
    let prompt_owned = prompt.to_string();
    let invocation = match kind {
        AgentKind::Claude => AgentInvocation {
            program: "claude".into(),
            args: vec![
                "--bare".into(),
                "--print".into(),
                "--output-format".into(),
                "text".into(),
                "--permission-mode".into(),
                "dontAsk".into(),
                "--allowedTools=Read Grep Glob Edit Write Bash".into(),
            ],
            stdin: Some(prompt_owned),
            cwd,
        },
        AgentKind::Codex => AgentInvocation {
            program: "codex".into(),
            args: vec![
                "exec".into(),
                "--ephemeral".into(),
                "--sandbox".into(),
                "workspace-write".into(),
                "--skip-git-repo-check".into(),
                "-C".into(),
                workspace.to_string_lossy().into_owned(),
                "-".into(),
            ],
            stdin: Some(prompt_owned),
            cwd,
        },
        AgentKind::Openclaw => AgentInvocation {
            program: "openclaw".into(),
            args: vec!["agent".into(), "--message".into(), prompt_owned, "--json".into()],
            stdin: None,
            cwd,
        },
        AgentKind::Hermes => AgentInvocation {
            program: "hermes".into(),
            args: vec!["--prompt".into(), prompt_owned, "--json".into()],
            stdin: None,
            cwd,
        },
    };
    Ok(invocation)
}
```

If OpenClaw/Hermes long prompts risk Windows command-line length, prefer stdin only if the CLI supports it. If support is unknown, keep current CLI-compatible form and document the residual risk in `gotchas.txt` only if reproduced.

- [ ] **Step 5: Implement convenience flow in `chat_commands.rs`**

Add:

```rust
fn should_use_convenience_flow(enabled: bool, intent: ChatIntent) -> bool {
    enabled && matches!(intent, ChatIntent::Write)
}
```

In `run_chat_send`, before normal route execution:

1. Classify intent.
2. If `request.convenience_enabled` and intent is ambiguous, append a user-visible assistant clarification or return a recoverable `CHAT_CONVENIENCE_CLARIFY` error.
3. If write flow, call a new helper `run_chat_convenience_send`.

The helper should:

```rust
async fn run_chat_convenience_send(
    state: &AppState,
    request: SendChatMessageRequest,
    context: &ProjectContext,
    session: &mut ChatSession,
    retrieval_prompt: String,
    citations: Vec<ChatCitation>,
    task_id: &str,
) -> Result<(), BackendError> {
    state.git_service.initialize_repository(context, "Before chat convenience edit")?;
    let checkpoint = state.git_service.create_checkpoint(
        context,
        CheckpointPurpose::HighRiskOperation,
        &format!("Before chat convenience edit: {}", truncate_title(&request.content)),
    )?;
    let prompt = format!(
        "{}\n\n{}",
        retrieval_prompt,
        ChatConvenienceService::convenience_prompt_suffix()
    );
    let kind = resolve_convenience_agent(state, context, request.agent)?;
    let invocation = AgentService::chat_convenience_invocation(kind, &context.root, &prompt)?;
    let answer = state.agent_service.run_task_streaming_with_delta(/* existing pattern */)?;
    let changes = state.git_service.changed_files_since_head(context)?;
    let audit = state.chat_convenience_service.audit_git_changes(&changes);
    let diff = state.git_service.diff_since_head(context)?;
    // persist applied/soft/rolled_back according to audit
    Ok(())
}
```

Use the same streaming callback pattern as current Agent Chat.

- [ ] **Step 6: Persist applied, soft, and hard outcomes**

Rules:

- Passed: append assistant message with `convenience_edit.status = Applied`.
- Soft: append assistant message with `convenience_edit.status = SoftViolationPending`.
- Hard: call `git_service.rollback_worktree_to_head(context)` and append assistant/system-style message content like `Chat convenience edit was rolled back: <reason>`. Since `ChatRole` has no `System`, use assistant role with `route: Some(ChatRoute::Agent)` and `convenience_edit.status = RolledBack` or `RollbackFailed`.

Do not persist the Agent's answer text for hard violations.

- [ ] **Step 7: Add keep/rollback commands for soft violation and last edit**

In `chat_commands.rs`, add:

```rust
#[tauri::command]
pub fn resolve_chat_convenience_edit(
    state: State<'_, AppState>,
    request: ResolveChatConvenienceEditRequest,
) -> Result<ChatSession, BackendError> { /* update status or rollback */ }

#[tauri::command]
pub fn rollback_last_chat_convenience_edit(
    state: State<'_, AppState>,
    request: RollbackLastChatConvenienceEditRequest,
) -> Result<ChatSession, BackendError> { /* find latest applied/kept/pending and rollback */ }
```

First version may implement rollback by resetting worktree to HEAD if the checkpoint is the current HEAD. If not current, return a recoverable `CHAT_ROLLBACK_NOT_CURRENT` error rather than attempting arbitrary historical rollback.

- [ ] **Step 8: Run Task 4 tests**

Run:

```powershell
cargo test --lib --no-default-features services::agent_service::tests::convenience_chat_invocation_supports_all_agents_from_project_root commands::chat_commands::tests -- --nocapture
```

Expected: pass.

---

## Task 5: Frontend Types, Store, Chat UI, And Settings UI

**Files:**
- Modify: `src/types/chat.ts`
- Modify: `src/types/settings.ts`
- Modify: `src/stores/chatStore.ts`
- Modify: `src/stores/settingsStore.ts`
- Create: `src/features/chat/ChatConveniencePanel.tsx`
- Create: `src/features/chat/ChatConveniencePanel.test.tsx`
- Modify: `src/features/chat/ChatView.tsx`
- Modify: `src/features/settings/SecuritySettings.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`

- [ ] **Step 1: Write failing UI tests**

Create `ChatConveniencePanel.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ChatConveniencePanel } from "./ChatConveniencePanel";

describe("ChatConveniencePanel", () => {
  it("asks for confirmation before enabling", () => {
    const onSetEnabled = vi.fn();
    render(
      <ChatConveniencePanel
        enabled={false}
        pending={false}
        onSetEnabled={onSetEnabled}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /convenience/i }));

    expect(screen.getByText(/directly modify/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /enable/i }));
    expect(onSetEnabled).toHaveBeenCalledWith(true);
  });

  it("renders applied edit metadata with collapsed diff", () => {
    render(
      <ChatConveniencePanel
        enabled
        pending={false}
        onSetEnabled={vi.fn()}
        edit={{
          status: "applied",
          checkpointHash: "abc123",
          affectedPaths: ["wiki/a.md"],
          diffSummary: "Updated wiki/a.md",
          diffText: "```diff\n+hi\n```",
        }}
      />,
    );

    expect(screen.getByText(/wiki\/a.md/)).toBeInTheDocument();
    expect(screen.getByText(/abc123/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /diff/i })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```powershell
npm run test -- ChatConveniencePanel
```

Expected: fail because the component and types do not exist.

- [ ] **Step 3: Add TypeScript types**

In `src/types/chat.ts`, add:

```ts
export type ChatConvenienceEditStatus =
  | "applied"
  | "soft_violation_pending"
  | "kept_after_soft_violation"
  | "rolled_back"
  | "rollback_failed";

export interface ChatConvenienceEdit {
  status: ChatConvenienceEditStatus;
  checkpointHash?: string | null;
  affectedPaths: string[];
  diffSummary: string;
  diffText?: string | null;
  violationReason?: string | null;
  rollbackTaskId?: string | null;
}
```

Add `convenienceEdit?: ChatConvenienceEdit | null` to `ChatMessage`, and `convenienceEnabled?: boolean` to `SendChatMessageRequest`.

- [ ] **Step 4: Update stores**

In `chatStore.ts`:

- Extend `SendChatOptions` with `convenienceEnabled?: boolean`.
- Include `convenienceEnabled: options?.convenienceEnabled ?? false` in `send_chat_message`.
- Add methods:

```ts
resolveConvenienceEdit: (projectId: string, rootPath: string, sessionId: string, messageId: string, keep: boolean) => Promise<void>;
rollbackLastConvenienceEdit: (projectId: string, rootPath: string, sessionId: string) => Promise<void>;
```

These call `resolve_chat_convenience_edit` and `rollback_last_chat_convenience_edit`, then reload the active session.

- [ ] **Step 5: Implement `ChatConveniencePanel`**

Use compact controls matching Chat toolbar density. Props:

```tsx
export interface ChatConveniencePanelProps {
  enabled: boolean;
  pending: boolean;
  edit?: ChatConvenienceEdit | null;
  onSetEnabled: (enabled: boolean) => void;
  onKeep?: () => void;
  onRollback?: () => void;
  onRollbackLast?: () => void;
}
```

Render:

- Toggle/badge button.
- First-use confirmation dialog.
- Applied summary block.
- Soft violation warning with keep/rollback buttons.
- Hard violation report.
- Collapsed diff using `<details>`.

- [ ] **Step 6: Wire ChatView**

In `ChatView.tsx`:

- Load convenience authorization when project changes.
- Pass `convenienceEnabled` to `send`.
- Mount the toggle near `SessionToolbar` route segment.
- Render `ChatConveniencePanel` for assistant messages with `convenienceEdit`.
- Disable another convenience write if latest message has `soft_violation_pending`.

- [ ] **Step 7: Wire Security settings**

In `SecuritySettings.tsx`, add a compact section:

- Current project convenience authorization status.
- Revoke current project button.
- Revoke all button.

Use existing Button/control styles and i18n strings. Do not write project settings for these actions.

- [ ] **Step 8: Add i18n copy**

In `en.json`, add keys under `chat.convenience` and `settings.security.convenience`.

In `zh-CN.json`, add Chinese equivalents. Keep labels short enough for compact UI.

- [ ] **Step 9: Run Task 5 tests**

Run:

```powershell
npm run test -- ChatConveniencePanel
npm run test -- chatStore
npm run test -- settingsStore
```

Expected: targeted tests pass. If `chatStore`/`settingsStore` tests do not exist, add focused store tests for request payloads and authorization mutations.

---

## Task 6: End-To-End Verification, Review, And Logs

**Files:**
- Modify: `SPEC/progress.txt`
- Append to: `SPEC/gotchas.txt` only if a subtle/repeated issue appears

- [ ] **Step 1: Run targeted Rust checks**

Run:

```powershell
cargo check --lib --tests --no-default-features
cargo test --lib --no-default-features services::chat_convenience_service::tests -- --nocapture
cargo test --lib --no-default-features services::git_service::tests -- --nocapture
cargo test --lib --no-default-features services::settings_service::tests::chat_convenience -- --nocapture
cargo test --lib --no-default-features services::agent_service::tests::convenience_chat_invocation_supports_all_agents_from_project_root -- --nocapture
```

Expected: all pass.

- [ ] **Step 2: Run frontend checks**

Run:

```powershell
npm run test
npm run lint
npm run build
Get-ChildItem -Path src,src-tauri/src -Recurse -File | Select-String -Pattern 'console\.log'
```

Expected: tests/lint/build pass, and console scan returns no source matches.

- [ ] **Step 3: Run full Rust checks where possible**

Run:

```powershell
cargo check --lib --tests
cargo test --lib --no-default-features
```

Expected: `cargo check` passes. If `cargo test --lib --no-default-features` still has unrelated existing failures, record exact test names and do not fix unrelated dirty work. If default-feature `cargo test --lib` hits the known Windows loader `0xc0000139`, cite `SPEC/gotchas.txt`.

- [ ] **Step 4: Run two reviews**

Launch or manually perform the required two reviews:

- Review A with shared context: design intent, logic, consistency, docs integration.
- Review B with fresh context: blind spots, missing tests, unclear behavior.

Fix valid findings, then rerun all checks from Step 1.

- [ ] **Step 5: Update progress log**

Append newest-on-top in `SPEC/progress.txt`:

```text
[2026-07-05] Chat convenience mode implementation - Implemented project-level Chat convenience mode with global local authorization, direct project Agent edits, per-turn Git checkpoints, post-write audit, soft/hard violation handling, and UI rollback controls - Key decision: convenience mode supports all installed Agents with best-effort constraints while hard violations roll back automatically.
```

- [ ] **Step 6: Final response**

Report:

- changed files,
- root behavior implemented,
- verification results,
- residual risks,
- any checks that could not run and exact reason.

Do not claim success before verification output confirms it.

---

## Plan Self-Review

- Spec coverage: covers authorization, intent, direct real-project Agent writes, checkpoint, audit, hard rollback, soft keep/rollback, all-Agent support, network/shell best-effort, UI feedback, settings revocation, and last-edit rollback.
- Red-flag scan: no unresolved markers remain; where implementation details can vary, exact acceptable behavior is specified.
- Type consistency: Rust and TypeScript names use `ChatConvenienceAuthorization`, `ChatConvenienceEdit`, and `ChatConvenienceEditStatus`; command names are snake_case for Tauri IPC and camelCase for DTO fields.
- Scope: excludes candidate workspace, historical rollback, strong sandbox abstraction, full audit center, and high-risk mode as required by the approved design.
