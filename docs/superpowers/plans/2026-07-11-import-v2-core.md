# Import 2.0 Core Implementation Plan

> Historical implementation plan. Do not use it to restore old Source paths, migration UI, automatic Agent fallback, or compile-after-import; follow [`../specs/2026-07-24-import-source-media-flow-design.md`](../specs/2026-07-24-import-source-media-flow-design.md).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立 Import 2.0 的稳定后端核心：版本化 DTO、可恢复会话、引擎协议、来源版本注册、质量门、逐项原子提交、任务事件和 Tauri IPC；使用测试引擎完成端到端验证，但不接入真实文件解析器或网页连接器。

**Architecture:** 新增独立 `ImportV2Service`，暂时与旧 `ImportService` 并存，复用现有 `ProjectContext`、`FileStore`、`TaskService` 与 `GitService`。所有解析能力通过 `ImportEngine` 接口返回 staging 内的结构化产物；Core 验证、预览并在用户确认后提交，真实文件、网页、Agent 和迁移在后续四份计划中接入同一接口。

**Tech Stack:** Rust 2021、Tauri v2、Serde JSON、Markdown + JSON + local files、现有 `chrono`/`uuid`/`sha2`、现有 `TaskService`、`FileStore`、`GitService`；React 19 + TypeScript 只增加 IPC 类型，不实现页面。

## Global Constraints

- 项目内容仍只使用 Markdown、JSON 和本地文件；不得引入数据库。
- `raw/sources/` 默认不可变；Import V2 只能新增来源版本，不能覆盖既有原件。
- 预览阶段只写 `.app/import-sessions/<session-id>/`；正式写入必须由后端根据 session/item ID 解析目标路径。
- React 不接触文件系统、Git、Agent、能力包进程或秘密存储。
- 所有项目内路径必须经过 `ProjectContext`；前端不得提交任意写入路径。
- Cookie、Token、API Key、文档密码和平台签名参数不得进入项目、日志、错误详情或测试 fixture。
- 单项提交必须原子；一个批次允许部分成功，失败项目不能回滚其他已完成项目。
- 高风险覆盖、合并和来源删除必须先创建 Git checkpoint。本 Core 计划只实现“新建”和“带 expected hash 的确认更新”；删除留给 Cutover/Source Management 计划。
- Core 不安装能力包、不启动外部解析进程、不实现网络下载；只固定协议、路径验证、注册接口和进程适配边界。
- Core 不接入真实 DOCX/PDF/OCR/ASR、LibreOffice、FFmpeg、MarkItDown、Docling 或平台连接器。
- Core 不实现 Agent；后续 Agent 计划只能通过 `ImportEngine`/staging/Quality Gate 接入。
- Core 不替换旧 Tauri command；新命令使用 `_v2` 后缀，最终 Cutover 计划一次性移除旧命令并重命名。
- Core 使用临时 `.app/source-index-v2.json`，避免覆盖旧 ImportService 的 `.app/source-index.json`；Cutover 计划验证迁移结果后再原子替换为设计规范中的正式索引路径。
- Core 不实现 Import 页面；只新增与 Rust DTO 同名、同枚举值的 TypeScript 类型。
- 不新增 Rust crate。使用现有 `uuid` v4、`chrono`、`sha2`、`serde` 与 `serde_json`。
- 每项任务使用 TDD：先写失败测试并确认失败，再写最小实现并确认通过。
- 每项任务完成后做独立 commit；不得把 `.superpowers/` 或 `UI-Frontend-design/import-v2-reference.html` 带入任何实现提交。
- Windows 后端测试使用 `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`；GUI command 注册使用 `cargo check --manifest-path src-tauri/Cargo.toml`。

---

## 1. 范围与后续计划边界

本文件是五个实施包中的第 1 份。完成后系统应能用内存测试引擎完成：创建会话 → 追加项目 → 运行 → 质量检查 → 预览 → 确认 → 写入不可变来源、基线、Wiki 和清单 → 重启恢复。

后续计划按以下顺序编写和实施：

1. `Import 2.0 File Ingestion`：扫描、现代/老式 Office、PDF、OCR、Excel、媒体与真实能力包 runner。
2. `Import 2.0 Web Ingestion`：Domain Router、通用网页、微信、知乎、Bilibili 与隔离浏览器。
3. `Import 2.0 Agent Assistance`：平衡自主 Agent、工具授权、自动硬失败兜底与可选 AI 优化。
4. `Import 2.0 Migration and Cutover`：旧数据迁移、完整回归、删除旧实现和一次性命令切换。

## 2. 目标文件结构

```text
src-tauri/src/
├── models/
│   ├── import.rs                       # 旧 DTO，Core 阶段保持不动
│   ├── import_v2.rs                    # V2 公共 DTO、枚举、状态转换
│   └── mod.rs                          # export import_v2
├── errors/
│   └── error_codes.rs                  # 稳定 Import V2 错误码
├── services/
│   ├── import_service/                 # 旧 facade，Core 阶段保持不动
│   ├── import_v2/
│   │   ├── mod.rs                      # ImportV2Service 与公开 re-export
│   │   ├── session_store.rs            # session.json/items/*.json 持久化
│   │   ├── engine.rs                   # ImportEngine、EngineRegistry
│   │   ├── pack_protocol.rs            # JSON-RPC 2.0 结构和 staging 产物校验
│   │   ├── source_registry.rs          # 来源去重、版本计划与清单读取
│   │   ├── quality_gate.rs             # Markdown/资源/覆盖率质量门
│   │   ├── orchestrator.rs             # 状态机、引擎运行、TaskService 协作
│   │   ├── transaction.rs              # 单项文件事务、备份与回滚
│   │   ├── commit.rs                   # 确认、Git checkpoint、逐项提交与历史
│   │   └── test_support.rs             # 单元测试临时项目 helper
│   └── mod.rs                          # export ImportV2Service 等
├── commands/
│   ├── import_v2_commands.rs           # 薄 Tauri commands
│   └── mod.rs
├── app_state.rs                        # 注入 ImportV2Service
└── lib.rs                              # 注册 `_v2` commands

src-tauri/tests/
└── import_v2_core.rs                   # 跨模块端到端与安全回归

src/types/
└── importV2.ts                         # 前端只读 IPC contract
```

## 3. 稳定接口

后续四份计划只能依赖下列接口，不得直接读写会话 JSON 或绕过 Quality Gate：

```rust
pub trait ImportEngine: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;
    fn supports(&self, input: &ImportInput) -> bool;
    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError>;
}

impl ImportV2Service {
    pub fn create_session(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        mode: ImportResourceMode,
    ) -> Result<ImportSession, BackendError>;

    pub fn add_inputs(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        inputs: Vec<ImportInput>,
    ) -> Result<ImportSession, BackendError>;

    pub fn load_session(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
    ) -> Result<ImportSession, BackendError>;

    pub fn run_item(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        task_service: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItem, BackendError>;

    pub fn set_item_selected(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        item_id: &str,
        selected: bool,
    ) -> Result<ImportSession, BackendError>;

    pub fn commit_items(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        request: &CommitImportSessionRequest,
    ) -> Result<ImportBatchResult, BackendError>;

    pub fn register_engine(
        &self,
        engine: Arc<dyn ImportEngine>,
    ) -> Result<(), BackendError>;
}
```

---

### Task 1: 定义 Import V2 DTO、状态机和错误码

**Files:**
- Create: `src-tauri/src/models/import_v2.rs`
- Modify: `src-tauri/src/models/mod.rs:1-16`
- Modify: `src-tauri/src/errors/error_codes.rs:1-5`
- Test: `src-tauri/src/models/import_v2.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `ImportSession`, `ImportItem`, `ImportInput`, `ImportItemStatus`, `ImportSessionStatus`, `ImportResourceMode`, `QualityReport`, `ImportArtifact`, `CommitImportSessionRequest`, `ImportBatchResult`。
- Enum JSON values use `snake_case`; struct fields use `camelCase`。
- `IMPORT_V2_SCHEMA_VERSION` is exactly `2`。

- [ ] **Step 1: 写 DTO 序列化和状态转换失败测试**

在新文件先加入测试模块，测试必须引用尚未定义的类型，从而证明测试确实覆盖新 contract：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn import_v2_contract_is_versioned_and_camel_case() {
        let session = ImportSession::new("session-1", "project-1", ImportResourceMode::Balanced);
        let value = serde_json::to_value(session).unwrap();
        assert_eq!(value["schemaVersion"], json!(2));
        assert_eq!(value["sessionId"], json!("session-1"));
        assert_eq!(value["resourceMode"], json!("balanced"));
        assert!(value.get("session_id").is_none());
    }

    #[test]
    fn item_state_machine_rejects_preview_to_complete_shortcut() {
        assert!(ImportItemStatus::Queued.can_transition_to(&ImportItemStatus::Inspecting));
        assert!(ImportItemStatus::Validating.can_transition_to(&ImportItemStatus::PreviewReady));
        assert!(!ImportItemStatus::PreviewReady.can_transition_to(&ImportItemStatus::Completed));
        assert!(ImportItemStatus::PreviewReady.can_transition_to(&ImportItemStatus::Committing));
    }
}
```

- [ ] **Step 2: 运行测试并确认编译失败**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features models::import_v2::tests
```

Expected: FAIL，错误包含 `cannot find type ImportSession` 或 `use of undeclared type ImportItemStatus`。

- [ ] **Step 3: 实现完整公共 DTO**

在 `models/import_v2.rs` 定义以下类型和字段；不要加入任意写入路径字段：

```rust
use serde::{Deserialize, Serialize};

use crate::models::task::TaskProgress;

pub const IMPORT_V2_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportResourceMode { Balanced, Performance, Saver }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportInputKind { File, Folder, Url }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportSessionStatus {
    Draft,
    Processing,
    WaitingForConfirmation,
    PartiallyCommitted,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportItemStatus {
    Queued,
    Inspecting,
    WaitingCapability,
    WaitingLogin,
    Extracting,
    Validating,
    PreviewReady,
    NeedsMerge,
    Committing,
    Completed,
    Paused,
    Cancelled,
    Skipped,
    Failed,
}

impl ImportItemStatus {
    pub fn can_transition_to(&self, next: &Self) -> bool {
        use ImportItemStatus::*;
        matches!(
            (self, next),
            (Queued, Inspecting | Cancelled | Skipped)
                | (Inspecting, WaitingCapability | WaitingLogin | Extracting | Failed | Cancelled)
                | (WaitingCapability, Extracting | Cancelled | Failed)
                | (WaitingLogin, Extracting | Cancelled | Failed)
                | (Extracting, Validating | Failed | Cancelled)
                | (Validating, PreviewReady | Failed | Cancelled)
                | (PreviewReady, NeedsMerge | Committing | Skipped | Cancelled)
                | (NeedsMerge, PreviewReady | Committing | Skipped | Cancelled)
                | (Committing, Completed | Failed)
                | (Paused, Inspecting | Extracting | Cancelled)
                | (Failed, Inspecting | Skipped | Cancelled)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportStage { Inspect, Route, Extract, Validate, Commit }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QualityLevel { Pass, Warning, Fail }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind { SourceSnapshot, Markdown, Image, Attachment, Subtitle, Metadata }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportInput {
    pub kind: ImportInputKind,
    pub display_name: String,
    pub locator: String,
    pub normalized_locator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QualityMetric {
    pub code: String,
    pub actual: f64,
    pub minimum: f64,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QualityReport {
    pub level: QualityLevel,
    pub metrics: Vec<QualityMetric>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportArtifact {
    pub kind: ArtifactKind,
    pub relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome { Succeeded, Failed, Cancelled }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttemptRecord {
    pub route: String,
    pub engine_id: String,
    pub engine_version: String,
    pub stage: ImportStage,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub outcome: AttemptOutcome,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportIssue {
    pub code: String,
    pub message: String,
    pub stage: ImportStage,
    pub retryable: bool,
    pub user_action_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewArtifact {
    pub markdown: ImportArtifact,
    pub assets: Vec<ImportArtifact>,
    pub source_snapshot: ImportArtifact,
    pub quality: QualityReport,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportItem {
    pub item_id: String,
    pub input: ImportInput,
    pub status: ImportItemStatus,
    pub selected: bool,
    pub task_id: Option<String>,
    pub progress: Option<TaskProgress>,
    pub attempts: Vec<AttemptRecord>,
    pub preview: Option<ImportPreviewArtifact>,
    pub issue: Option<ImportIssue>,
}

impl ImportItem {
    pub fn queued(item_id: &str, input: ImportInput) -> Self {
        Self {
            item_id: item_id.to_string(),
            input,
            status: ImportItemStatus::Queued,
            selected: true,
            task_id: None,
            progress: None,
            attempts: Vec::new(),
            preview: None,
            issue: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSession {
    pub schema_version: u32,
    pub session_id: String,
    pub project_id: String,
    pub status: ImportSessionStatus,
    pub resource_mode: ImportResourceMode,
    pub created_at: String,
    pub updated_at: String,
    pub items: Vec<ImportItem>,
}

impl ImportSession {
    pub fn new(session_id: &str, project_id: &str, resource_mode: ImportResourceMode) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            schema_version: IMPORT_V2_SCHEMA_VERSION,
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            status: ImportSessionStatus::Draft,
            resource_mode,
            created_at: now.clone(),
            updated_at: now,
            items: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitConflictAction { CreateNew, KeepWiki, ApplyMergedCandidate }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitItemDecision {
    pub item_id: String,
    pub conflict_action: Option<CommitConflictAction>,
    pub expected_wiki_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommitImportSessionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub decisions: Vec<CommitItemDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemCommitResult {
    pub item_id: String,
    pub source_id: Option<String>,
    pub version_id: Option<String>,
    pub wiki_path: Option<String>,
    pub committed: bool,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchResult {
    pub batch_id: String,
    pub session_id: String,
    pub committed_count: u32,
    pub failed_count: u32,
    pub items: Vec<ImportItemCommitResult>,
}
```

在 `error_codes.rs` 加入稳定常量：

```rust
pub const IMPORT_V2_SESSION_NOT_FOUND: &str = "IMPORT_V2_SESSION_NOT_FOUND";
pub const IMPORT_V2_SESSION_INVALID: &str = "IMPORT_V2_SESSION_INVALID";
pub const IMPORT_V2_ITEM_NOT_FOUND: &str = "IMPORT_V2_ITEM_NOT_FOUND";
pub const IMPORT_V2_STATE_INVALID: &str = "IMPORT_V2_STATE_INVALID";
pub const IMPORT_V2_ENGINE_UNAVAILABLE: &str = "IMPORT_V2_ENGINE_UNAVAILABLE";
pub const IMPORT_V2_ENGINE_OUTPUT_INVALID: &str = "IMPORT_V2_ENGINE_OUTPUT_INVALID";
pub const IMPORT_V2_SOURCE_INDEX_INVALID: &str = "IMPORT_V2_SOURCE_INDEX_INVALID";
pub const IMPORT_V2_QUALITY_FAILED: &str = "IMPORT_V2_QUALITY_FAILED";
pub const IMPORT_V2_CANCELLED: &str = "IMPORT_V2_CANCELLED";
pub const IMPORT_V2_COMMIT_CONFLICT: &str = "IMPORT_V2_COMMIT_CONFLICT";
pub const IMPORT_V2_COMMIT_FAILED: &str = "IMPORT_V2_COMMIT_FAILED";
```

- [ ] **Step 4: 导出模块并运行定向测试**

在 `models/mod.rs` 加 `pub mod import_v2;`，然后运行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features models::import_v2::tests
```

Expected: 2 tests PASS。

- [ ] **Step 5: 运行 Rust 全套并提交**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
git add src-tauri/src/models/import_v2.rs src-tauri/src/models/mod.rs src-tauri/src/errors/error_codes.rs
git commit -m "feat(import): define import v2 domain contract"
```

Expected: Rust tests PASS；commit 只包含上述三个文件。

---

### Task 2: 实现可恢复的 Import Session Store

**Files:**
- Create: `src-tauri/src/services/import_v2/mod.rs`
- Create: `src-tauri/src/services/import_v2/session_store.rs`
- Create: `src-tauri/src/services/import_v2/test_support.rs`
- Modify: `src-tauri/src/services/mod.rs:1-35`
- Test: `src-tauri/src/services/import_v2/session_store.rs`

**Interfaces:**
- `SessionStore::create(context, file_store, mode) -> ImportSession`
- `SessionStore::load(context, file_store, session_id) -> ImportSession`
- `SessionStore::save(context, file_store, session) -> ()`
- `SessionStore::add_inputs(context, file_store, session_id, inputs) -> ImportSession`
- `SessionStore::update_item(context, file_store, session_id, item) -> ImportSession`
- Persist session summary at `.app/import-sessions/<id>/session.json` and each item at `items/<item-id>.json`。

- [ ] **Step 1: 写恢复、追加和 ID 穿越测试**

```rust
#[test]
fn session_round_trip_restores_items_after_new_store_instance() {
    let (context, root) = test_context("session-round-trip");
    let files = FileStore::default();
    let store = SessionStore::default();
    let session = store.create(&context, &files, ImportResourceMode::Balanced).unwrap();
    store.add_inputs(&context, &files, &session.session_id, vec![test_file_input("研究报告.pdf")]).unwrap();

    let reopened = SessionStore::default().load(&context, &files, &session.session_id).unwrap();
    assert_eq!(reopened.items.len(), 1);
    assert_eq!(reopened.items[0].input.display_name, "研究报告.pdf");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn session_id_cannot_escape_import_session_root() {
    let (context, root) = test_context("session-traversal");
    let error = SessionStore::default()
        .load(&context, &FileStore::default(), "../settings")
        .unwrap_err();
    assert_eq!(error.code, IMPORT_V2_SESSION_INVALID);
    std::fs::remove_dir_all(root).unwrap();
}
```

- [ ] **Step 2: 运行测试并确认失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::session_store::tests
```

Expected: FAIL，模块或 `SessionStore` 尚不存在。

- [ ] **Step 3: 实现路径约束与原子持久化**

核心实现必须使用后端生成的 UUID，并验证任何从 IPC 回传的 ID：

```rust
#[derive(Default)]
pub struct SessionStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRecord {
    schema_version: u32,
    session_id: String,
    project_id: String,
    status: ImportSessionStatus,
    resource_mode: ImportResourceMode,
    created_at: String,
    updated_at: String,
    item_ids: Vec<String>,
}

impl From<&ImportSession> for SessionRecord {
    fn from(session: &ImportSession) -> Self {
        Self {
            schema_version: session.schema_version,
            session_id: session.session_id.clone(),
            project_id: session.project_id.clone(),
            status: session.status.clone(),
            resource_mode: session.resource_mode.clone(),
            created_at: session.created_at.clone(),
            updated_at: session.updated_at.clone(),
            item_ids: session.items.iter().map(|item| item.item_id.clone()).collect(),
        }
    }
}

fn validate_id(value: &str) -> Result<(), BackendError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(BackendError::new(
            IMPORT_V2_SESSION_INVALID,
            "Import session identifier is invalid.",
            false,
            true,
        ))
    }
}

impl SessionStore {
    pub fn create(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        mode: ImportResourceMode,
    ) -> Result<ImportSession, BackendError> {
        let id = uuid::Uuid::new_v4().to_string();
        let session = ImportSession::new(&id, &context.project_id, mode);
        self.save(context, file_store, &session)?;
        Ok(session)
    }

    pub fn save(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session: &ImportSession,
    ) -> Result<(), BackendError> {
        validate_id(&session.session_id)?;
        if session.project_id != context.project_id {
            return Err(BackendError::new(
                IMPORT_V2_SESSION_INVALID,
                "Import session belongs to another project.",
                false,
                true,
            ));
        }
        let root = format!(".app/import-sessions/{}", session.session_id);
        file_store.ensure_dir(context, &format!("{root}/items"))?;
        for item in &session.items {
            validate_id(&item.item_id)?;
            file_store.write_json_atomic(
                context,
                &format!("{root}/items/{}.json", item.item_id),
                item,
            )?;
        }
        let summary = SessionRecord::from(session);
        file_store.write_json_atomic(context, &format!("{root}/session.json"), &summary)
    }
}
```

`SessionRecord` 保存除 `items` 正文外的 session 字段和 `item_ids`；`load` 逐个读取 `items/<id>.json` 并拒绝缺失、重复或项目不匹配。`add_inputs` 为每个输入生成 UUID、默认 `selected = true`、`status = queued`，更新 `updated_at` 后调用 `save`。

`test_support.rs` 定义本计划单元测试共享的两个 helper：

```rust
pub(super) fn test_context(suffix: &str) -> (ProjectContext, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "llm-wiki-import-v2-{}-{}",
        suffix,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    (ProjectContext::new(format!("project-{suffix}"), root.clone()), root)
}

pub(super) fn test_file_input(name: &str) -> ImportInput {
    ImportInput {
        kind: ImportInputKind::File,
        display_name: name.to_string(),
        locator: format!("D:/fixtures/{name}"),
        normalized_locator: Some(format!("file:d:/fixtures/{}", name.to_lowercase())),
    }
}
```

- [ ] **Step 4: 运行会话测试与旧 Import 回归**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::session_store::tests
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_service::
```

Expected: 新测试 PASS；旧 ImportService 测试继续 PASS。

- [ ] **Step 5: 提交**

```powershell
git add src-tauri/src/services/import_v2 src-tauri/src/services/mod.rs
git commit -m "feat(import): persist resumable import v2 sessions"
```

---

### Task 3: 固定引擎接口与 JSON-RPC 2.0 能力包协议

**Files:**
- Create: `src-tauri/src/services/import_v2/engine.rs`
- Create: `src-tauri/src/services/import_v2/pack_protocol.rs`
- Modify: `src-tauri/src/services/import_v2/mod.rs`
- Test: both new files

**Interfaces:**
- Produces the `ImportEngine` trait shown in section 3。
- `EngineRequest` contains protocol/request/session/item/task IDs, input, staging root and operation。
- `EngineResult` contains only staging-relative source snapshot, Markdown and asset paths plus hashes/metrics。
- External process spawning is intentionally absent; File Ingestion plan supplies the stdio runner and process-tree controller。

- [ ] **Step 1: 写 registry 与越界产物测试**

```rust
#[test]
fn registry_selects_only_a_supporting_engine() {
    let registry = EngineRegistry::default();
    registry.register(Arc::new(FixtureEngine::new("fixture", true))).unwrap();
    let input = ImportInput {
        kind: ImportInputKind::File,
        display_name: "a.pdf".into(),
        locator: "D:/a.pdf".into(),
        normalized_locator: None,
    };
    assert_eq!(registry.resolve(&input).unwrap().descriptor().engine_id, "fixture");
}

#[test]
fn engine_result_cannot_escape_item_staging() {
    let result = EngineResult {
        source_snapshot_path: "source.bin".into(),
        markdown_path: "../outside.md".into(),
        asset_paths: Vec::new(),
        title: "Fixture".into(),
        text_coverage: Some(1.0),
        table_cell_accuracy: None,
        warnings: Vec::new(),
    };
    let error = validate_engine_result(".app/import-sessions/s/items/i/staging", &result).unwrap_err();
    assert_eq!(error.code, IMPORT_V2_ENGINE_OUTPUT_INVALID);
}
```

在同一测试模块定义 registry fixture，不导出到生产 API：

```rust
struct FixtureEngine {
    descriptor: EngineDescriptor,
    supported: bool,
}

impl FixtureEngine {
    fn new(engine_id: &str, supported: bool) -> Self {
        Self {
            descriptor: EngineDescriptor {
                engine_id: engine_id.to_string(),
                engine_version: "1.0.0".into(),
                route: "fixture".into(),
            },
            supported,
        }
    }
}

impl ImportEngine for FixtureEngine {
    fn descriptor(&self) -> EngineDescriptor { self.descriptor.clone() }
    fn supports(&self, _input: &ImportInput) -> bool { self.supported }
    fn execute(
        &self,
        _request: &EngineRequest,
        _cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        Ok(EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: Vec::new(),
            title: "Fixture".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            warnings: Vec::new(),
        })
    }
}
```

- [ ] **Step 2: 运行测试并确认失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::engine::tests services::import_v2::pack_protocol::tests
```

Expected: FAIL，`EngineRegistry`、`EngineResult` 和 validator 尚不存在。

- [ ] **Step 3: 实现协议结构**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineRequest {
    pub protocol_version: String,
    pub request_id: String,
    pub session_id: String,
    pub item_id: String,
    pub task_id: String,
    pub operation: EngineOperation,
    pub input: ImportInput,
    pub staging_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineOperation { Inspect, Extract }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineResult {
    pub source_snapshot_path: String,
    pub markdown_path: String,
    pub asset_paths: Vec<String>,
    pub title: String,
    pub text_coverage: Option<f64>,
    pub table_cell_accuracy: Option<f64>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: String,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}
```

`validate_engine_result` 对 source、Markdown 和每个 asset 调用同一个 lexical relative-path validator：拒绝空路径、绝对路径、盘符、UNC、`.`、`..`；将其拼到 `staging_root` 后再次用 `ProjectContext::resolve_project_path` 验证。产物存在性和 SHA-256 在 orchestrator 读取文件时验证。

同时实现 `JsonRpcResponse::validate()`：`jsonrpc` 必须为 `"2.0"`，响应 ID 必须等于请求 ID，`result` 与 `error` 必须恰好存在一个。为“二者皆空”“二者皆有”和 ID 不匹配各写一个失败断言，错误码统一为 `IMPORT_V2_ENGINE_OUTPUT_INVALID`。

- [ ] **Step 4: 实现 thread-safe registry**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EngineDescriptor {
    pub engine_id: String,
    pub engine_version: String,
    pub route: String,
}

pub trait ImportEngine: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;
    fn supports(&self, input: &ImportInput) -> bool;
    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError>;
}

#[derive(Default)]
pub struct EngineRegistry {
    engines: RwLock<Vec<Arc<dyn ImportEngine>>>,
}

impl EngineRegistry {
    pub fn register(&self, engine: Arc<dyn ImportEngine>) -> Result<(), BackendError> {
        let descriptor = engine.descriptor();
        let mut engines = self.engines.write().map_err(|_| registry_error())?;
        if engines.iter().any(|existing| existing.descriptor().engine_id == descriptor.engine_id) {
            return Err(BackendError::new(
                IMPORT_V2_ENGINE_UNAVAILABLE,
                "An import engine with this identifier is already registered.",
                true,
                false,
            ));
        }
        engines.push(engine);
        Ok(())
    }

    pub fn resolve(&self, input: &ImportInput) -> Result<Arc<dyn ImportEngine>, BackendError> {
        self.engines
            .read()
            .map_err(|_| registry_error())?
            .iter()
            .find(|engine| engine.supports(input))
            .cloned()
            .ok_or_else(|| BackendError::new(
                IMPORT_V2_ENGINE_UNAVAILABLE,
                "No installed import engine supports this input.",
                true,
                true,
            ))
    }
}
```

- [ ] **Step 5: 运行测试并提交**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::engine::tests
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::pack_protocol::tests
git add src-tauri/src/services/import_v2
git commit -m "feat(import): define import engine protocol"
```

---

### Task 4: 实现来源去重与不可变版本计划

**Files:**
- Create: `src-tauri/src/services/import_v2/source_registry.rs`
- Modify: `src-tauri/src/services/import_v2/mod.rs`
- Test: `src-tauri/src/services/import_v2/source_registry.rs`

**Interfaces:**
- `SourceRegistry::read_index(context, files) -> SourceIndex`
- `SourceRegistry::resolve(index, normalized_locator, content_hash) -> SourceResolution`
- `SourceRegistry::build_commit_plan(index, existing_manifest, input) -> SourceCommitPlan`
- This module returns updated JSON values and target paths; it does not write them。
- Core 的索引读取与计划写入路径固定为 `.app/source-index-v2.json`；任何代码触碰旧 `.app/source-index.json` 都视为本任务失败。

- [ ] **Step 1: 写四种来源判定测试**

```rust
#[test]
fn source_resolution_distinguishes_new_duplicate_update_and_alias() {
    let pointer = SourcePointer { source_id: "source-1".into(), version_id: "version-1".into() };
    let index = SourceIndex {
        schema_version: 2,
        by_content_hash: BTreeMap::from([("hash-a".into(), pointer)]),
        by_locator: BTreeMap::from([("file:d:/a.docx".into(), SourcePointer { source_id: "source-1".into(), version_id: "version-1".into() })]),
    };
    assert_eq!(SourceRegistry::resolve(&index, "file:d:/new.docx", "hash-new"), SourceResolution::New);
    assert!(matches!(SourceRegistry::resolve(&index, "file:d:/a.docx", "hash-a"), SourceResolution::ExactDuplicate { .. }));
    assert!(matches!(SourceRegistry::resolve(&index, "file:d:/a.docx", "hash-b"), SourceResolution::UpdatedOrigin { .. }));
    assert!(matches!(SourceRegistry::resolve(&index, "file:d:/copy.docx", "hash-a"), SourceResolution::SameContentNewOrigin { .. }));
}

#[test]
fn commit_plan_never_reuses_an_existing_raw_version_path() {
    let existing = SourceManifest {
        schema_version: 2,
        source_id: "source-1".into(),
        origins: vec!["file:d:/a.docx".into()],
        versions: vec![fixture_version("version-1", "hash-a")],
        current_version_id: "version-1".into(),
        wiki_path: "wiki/sources/files/a.md".into(),
    };
    let input = SourceCommitInput::fixture("file:d:/a.docx", "hash-b", "a.docx");
    let plan = SourceRegistry::default()
        .build_commit_plan(&SourceIndex::default_v2(), Some(&existing), &input)
        .unwrap();
    assert!(plan.raw_path.starts_with("raw/sources/source-1/"));
    assert!(!plan.raw_path.contains("version-1/"));
}
```

- [ ] **Step 2: 运行测试并确认失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::source_registry::tests
```

Expected: FAIL，来源类型尚不存在。

- [ ] **Step 3: 实现索引、manifest 和纯计划构建**

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceIndex {
    pub schema_version: u32,
    pub by_content_hash: BTreeMap<String, SourcePointer>,
    pub by_locator: BTreeMap<String, SourcePointer>,
}

impl SourceIndex {
    pub fn default_v2() -> Self {
        Self {
            schema_version: IMPORT_V2_SCHEMA_VERSION,
            by_content_hash: BTreeMap::new(),
            by_locator: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourcePointer {
    pub source_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceManifest {
    pub schema_version: u32,
    pub source_id: String,
    pub origins: Vec<String>,
    pub versions: Vec<SourceVersion>,
    pub current_version_id: String,
    pub wiki_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceVersion {
    pub version_id: String,
    pub content_hash: String,
    pub raw_path: String,
    pub baseline_path: String,
    pub created_at: String,
    pub route: String,
    pub engine_id: String,
    pub engine_version: String,
    pub quality: QualityReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceResolution {
    New,
    ExactDuplicate { source_id: String, version_id: String },
    UpdatedOrigin { source_id: String, previous_version_id: String },
    SameContentNewOrigin { source_id: String, version_id: String },
}

pub struct SourceCommitPlan {
    pub source_id: String,
    pub version_id: String,
    pub raw_path: String,
    pub baseline_path: String,
    pub wiki_path: String,
    pub manifest_path: String,
    pub next_manifest: SourceManifest,
    pub next_index: SourceIndex,
}

pub struct SourceCommitInput {
    pub normalized_locator: String,
    pub content_hash: String,
    pub display_name: String,
    pub input_kind: ImportInputKind,
    pub source_extension: String,
    pub route: String,
    pub engine_id: String,
    pub engine_version: String,
    pub quality: QualityReport,
}

#[derive(Default)]
pub struct SourceRegistry;

impl SourceRegistry {
    pub fn resolve(
        index: &SourceIndex,
        normalized_locator: &str,
        content_hash: &str,
    ) -> SourceResolution {
        match (
            index.by_locator.get(normalized_locator),
            index.by_content_hash.get(content_hash),
        ) {
            (Some(locator_pointer), Some(pointer)) if locator_pointer.source_id == pointer.source_id => {
                SourceResolution::ExactDuplicate {
                    source_id: pointer.source_id.clone(),
                    version_id: pointer.version_id.clone(),
                }
            }
            (Some(locator_pointer), _) => SourceResolution::UpdatedOrigin {
                source_id: locator_pointer.source_id.clone(),
                previous_version_id: locator_pointer.version_id.clone(),
            },
            (None, Some(pointer)) => SourceResolution::SameContentNewOrigin {
                source_id: pointer.source_id.clone(),
                version_id: pointer.version_id.clone(),
            },
            (None, None) => SourceResolution::New,
        }
    }
}
```

测试模块中的 fixture helper 仅存在于 `#[cfg(test)]`：

```rust
fn pass_quality() -> QualityReport {
    QualityReport { level: QualityLevel::Pass, metrics: Vec::new(), warnings: Vec::new() }
}

fn fixture_version(version_id: &str, content_hash: &str) -> SourceVersion {
    SourceVersion {
        version_id: version_id.into(),
        content_hash: content_hash.into(),
        raw_path: format!("raw/sources/source-1/{version_id}/original.docx"),
        baseline_path: format!(".app/source-artifacts/source-1/{version_id}/baseline.md"),
        created_at: "2026-07-11T00:00:00Z".into(),
        route: "fixture".into(),
        engine_id: "fixture".into(),
        engine_version: "1.0.0".into(),
        quality: pass_quality(),
    }
}

impl SourceCommitInput {
    fn fixture(locator: &str, hash: &str, name: &str) -> Self {
        Self {
            normalized_locator: locator.into(),
            content_hash: hash.into(),
            display_name: name.into(),
            input_kind: ImportInputKind::File,
            source_extension: "docx".into(),
            route: "fixture".into(),
            engine_id: "fixture".into(),
            engine_version: "1.0.0".into(),
            quality: pass_quality(),
        }
    }
}
```

`build_commit_plan` 使用新 UUID 生成 `source_id`/`version_id`；新来源路径固定为 `raw/sources/<source>/<version>/original.<safe-ext>`、`.app/source-artifacts/<source>/<version>/baseline.md`、`.app/sources/<source>.json`。Wiki 只使用由 input kind 和安全 slug 生成的 `wiki/sources/files|web|video/<name>.md`；不接受调用者传入 Wiki 目标路径。

`read_index` 在 `.app/source-index-v2.json` 不存在时返回 `SourceIndex::default_v2()`；文件存在但 schema 不是 2、JSON 损坏或包含重复 locator/source 指针时返回 `IMPORT_V2_SOURCE_INDEX_INVALID`，不能把损坏索引当作空索引继续提交。

- [ ] **Step 4: 运行定向和路径安全测试**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::source_registry::tests
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features models::paths::tests
```

Expected: PASS。

- [ ] **Step 5: 提交**

```powershell
git add src-tauri/src/services/import_v2/source_registry.rs src-tauri/src/services/import_v2/mod.rs
git commit -m "feat(import): plan immutable source versions"
```

---

### Task 5: 实现统一 Quality Gate

**Files:**
- Create: `src-tauri/src/services/import_v2/quality_gate.rs`
- Modify: `src-tauri/src/services/import_v2/mod.rs`
- Test: `src-tauri/src/services/import_v2/quality_gate.rs`

**Interfaces:**
- `QualityGate::evaluate(staging_root, EngineResult) -> ImportPreviewArtifact`
- Runtime hard failures: empty Markdown, missing source snapshot, missing/broken asset, path escape, script/unsafe URL, hash mismatch。
- Runtime warnings: text coverage below `0.98`, table accuracy below `0.95`, remote image, engine warning。
- Warning results may be previewed and manually confirmed; `QualityLevel::Fail` cannot commit。

- [ ] **Step 1: 写空正文、危险 HTML、资源缺失和低覆盖率测试**

```rust
#[test]
fn quality_gate_rejects_unsafe_or_empty_markdown() {
    let fixture = quality_fixture("<script>alert(1)</script>");
    let error = QualityGate::default().evaluate(&fixture.root, &fixture.result).unwrap_err();
    assert_eq!(error.code, IMPORT_V2_QUALITY_FAILED);
}

#[test]
fn quality_gate_warns_but_allows_low_coverage_preview() {
    let fixture = quality_fixture_with_metrics("# 标题\n\n正文", 0.91, 0.93);
    let preview = QualityGate::default().evaluate(&fixture.root, &fixture.result).unwrap();
    assert_eq!(preview.quality.level, QualityLevel::Warning);
    assert!(preview.quality.warnings.iter().any(|warning| warning == "LOW_TEXT_COVERAGE"));
    assert!(preview.quality.warnings.iter().any(|warning| warning == "LOW_TABLE_ACCURACY"));
}
```

在同一测试模块定义完整 fixture：

```rust
struct QualityFixture {
    root: PathBuf,
    result: EngineResult,
}

impl Drop for QualityFixture {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); }
}

fn quality_fixture(markdown: &str) -> QualityFixture {
    quality_fixture_with_metrics(markdown, 1.0, 1.0)
}

fn quality_fixture_with_metrics(markdown: &str, text: f64, table: f64) -> QualityFixture {
    let root = std::env::temp_dir().join(format!("quality-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("candidate.md"), markdown).unwrap();
    std::fs::write(root.join("source.bin"), b"source").unwrap();
    QualityFixture {
        root,
        result: EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: Vec::new(),
            title: "Fixture".into(),
            text_coverage: Some(text),
            table_cell_accuracy: Some(table),
            warnings: Vec::new(),
        },
    }
}
```

- [ ] **Step 2: 运行测试并确认失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::quality_gate::tests
```

Expected: FAIL，`QualityGate` 尚不存在。

- [ ] **Step 3: 实现确定性质量规则**

```rust
pub const MIN_TEXT_COVERAGE: f64 = 0.98;
pub const MIN_TABLE_CELL_ACCURACY: f64 = 0.95;

#[derive(Default)]
pub struct QualityGate;

impl QualityGate {
    pub fn evaluate(
        &self,
        staging_root: &Path,
        result: &EngineResult,
    ) -> Result<ImportPreviewArtifact, BackendError> {
        let markdown_path = resolve_staging_artifact(staging_root, &result.markdown_path)?;
        let markdown = std::fs::read_to_string(&markdown_path).map_err(read_error)?;
        if markdown.trim().is_empty()
            || markdown.to_ascii_lowercase().contains("<script")
            || markdown.to_ascii_lowercase().contains("javascript:")
            || markdown.to_ascii_lowercase().contains("data:text/html")
        {
            return Err(BackendError::new(
                IMPORT_V2_QUALITY_FAILED,
                "Generated Markdown is empty or unsafe.",
                true,
                true,
            ));
        }

        let mut warnings = result.warnings.clone();
        let mut metrics = Vec::new();
        push_metric(&mut metrics, &mut warnings, "TEXT_COVERAGE", result.text_coverage, MIN_TEXT_COVERAGE);
        push_metric(&mut metrics, &mut warnings, "TABLE_CELL_ACCURACY", result.table_cell_accuracy, MIN_TABLE_CELL_ACCURACY);
        let level = if warnings.is_empty() { QualityLevel::Pass } else { QualityLevel::Warning };
        build_preview(staging_root, result, level, metrics, warnings)
    }
}
```

`build_preview` 必须重新计算每个产物的 SHA-256 和 byte size；不能信任引擎上报 hash。所有资源引用必须存在于 staging，且 Markdown 中的本地资源引用必须位于 result 的 asset 清单中。

- [ ] **Step 4: 运行测试并提交**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::quality_gate::tests
git add src-tauri/src/services/import_v2/quality_gate.rs src-tauri/src/services/import_v2/mod.rs
git commit -m "feat(import): validate import v2 preview quality"
```

---

### Task 6: 实现会话编排、状态持久化和 TaskService 协作

**Files:**
- Create: `src-tauri/src/services/import_v2/orchestrator.rs`
- Modify: `src-tauri/src/services/import_v2/mod.rs`
- Test: `src-tauri/src/services/import_v2/orchestrator.rs`

**Interfaces:**
- Implements `create_session`, `add_inputs`, `load_session`, `register_engine`, `run_item`, `set_item_selected` from section 3。
- `ImportV2Service` owns `SessionStore`, `EngineRegistry`, `QualityGate`, and a short-held mutation mutex。
- Engine execution happens outside the mutex; only state reload/transition/save uses the mutex。

- [ ] **Step 1: 写成功、无引擎、取消和非法状态测试**

```rust
struct FixtureEngine { project_root: PathBuf }

impl FixtureEngine {
    fn success(project_root: PathBuf) -> Self { Self { project_root } }
}

impl ImportEngine for FixtureEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor { engine_id: "fixture".into(), engine_version: "1.0.0".into(), route: "fixture".into() }
    }
    fn supports(&self, _input: &ImportInput) -> bool { true }
    fn execute(&self, request: &EngineRequest, _cancellation: &CancellationToken) -> Result<EngineResult, BackendError> {
        let root = self.project_root.join(request.staging_root.replace('/', std::path::MAIN_SEPARATOR_STR));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("source.bin"), b"source").unwrap();
        std::fs::write(root.join("candidate.md"), "# Fixture\n\nBody").unwrap();
        Ok(EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: Vec::new(),
            title: "Fixture".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            warnings: Vec::new(),
        })
    }
}

struct OrchestratorFixture {
    root: PathBuf,
    context: ProjectContext,
    files: FileStore,
    tasks: TaskService,
    service: ImportV2Service,
}

impl OrchestratorFixture {
    fn new(suffix: &str) -> Self {
        let (context, root) = test_context(suffix);
        Self { root, context, files: FileStore::default(), tasks: TaskService::default(), service: ImportV2Service::default() }
    }
    fn seed_one_item(&self) -> (ImportSession, ImportItem, BackendTask) {
        let session = self.service.create_session(&self.context, &self.files, ImportResourceMode::Balanced).unwrap();
        let session = self.service.add_inputs(&self.context, &self.files, &session.session_id, vec![test_file_input("a.pdf")]).unwrap();
        let item = session.items[0].clone();
        let task = self.tasks.create_project_task(
            TaskType::Import,
            self.context.project_id.clone(),
            self.root.clone(),
            "Fixture import".into(),
            true,
        ).unwrap();
        (session, item, task)
    }
    fn reopen(&self) -> ImportSession {
        let sessions = std::fs::read_dir(self.context.app_dir.join("import-sessions")).unwrap();
        let session_id = sessions.flatten().next().unwrap().file_name().to_string_lossy().into_owned();
        self.service.load_session(&self.context, &self.files, &session_id).unwrap()
    }
}

impl Drop for OrchestratorFixture {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); }
}

#[test]
fn run_item_persists_preview_ready_after_fixture_engine_succeeds() {
    let fixture = OrchestratorFixture::new("success");
    fixture.service.register_engine(Arc::new(FixtureEngine::success(fixture.root.clone()))).unwrap();
    let (session, item, task) = fixture.seed_one_item();
    let result = fixture.service.run_item(
        &fixture.context,
        &fixture.files,
        &fixture.tasks,
        &session.session_id,
        &item.item_id,
        &task.id,
    ).unwrap();
    assert_eq!(result.status, ImportItemStatus::PreviewReady);
    assert!(result.preview.is_some());
    assert_eq!(fixture.reopen().items[0].status, ImportItemStatus::PreviewReady);
}

#[test]
fn run_item_records_engine_unavailable_without_losing_session() {
    let fixture = OrchestratorFixture::new("no-engine");
    let (session, item, task) = fixture.seed_one_item();
    let error = fixture.service.run_item(
        &fixture.context,
        &fixture.files,
        &fixture.tasks,
        &session.session_id,
        &item.item_id,
        &task.id,
    ).unwrap_err();
    assert_eq!(error.code, IMPORT_V2_ENGINE_UNAVAILABLE);
    assert_eq!(fixture.reopen().items[0].status, ImportItemStatus::WaitingCapability);
}

#[test]
fn run_item_honors_a_pre_cancelled_task() {
    let fixture = OrchestratorFixture::new("cancelled");
    fixture.service.register_engine(Arc::new(FixtureEngine::success(fixture.root.clone()))).unwrap();
    let (session, item, task) = fixture.seed_one_item();
    fixture.tasks.cancel_task(&task.id).unwrap();
    let error = fixture.service.run_item(
        &fixture.context,
        &fixture.files,
        &fixture.tasks,
        &session.session_id,
        &item.item_id,
        &task.id,
    ).unwrap_err();
    assert_eq!(error.code, IMPORT_V2_CANCELLED);
    assert_eq!(fixture.reopen().items[0].status, ImportItemStatus::Cancelled);
}

#[test]
fn transition_helper_rejects_preview_to_completed() {
    let mut item = ImportItem::queued("item-1", test_file_input("a.pdf"));
    item.status = ImportItemStatus::PreviewReady;
    let error = transition_item(&mut item, ImportItemStatus::Completed).unwrap_err();
    assert_eq!(error.code, IMPORT_V2_STATE_INVALID);
}

#[test]
fn engine_error_is_reduced_to_a_secret_free_issue() {
    let error = BackendError::new("ENGINE_FAILED", "Authorization: Bearer secret", true, false)
        .with_details(serde_json::json!({ "path": "C:/Users/Aletta/private.pdf" }));
    let issue = issue_from_engine_error(&error, ImportStage::Extract);
    assert_eq!(issue.message, "Import engine failed.");
    assert!(!serde_json::to_string(&issue).unwrap().contains("secret"));
    assert!(!serde_json::to_string(&issue).unwrap().contains("Aletta"));
}
```

- [ ] **Step 2: 运行测试并确认失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::orchestrator::tests
```

Expected: FAIL，service facade 尚未实现。

- [ ] **Step 3: 实现 facade 和受控状态转换**

```rust
pub struct ImportV2Service {
    sessions: SessionStore,
    engines: EngineRegistry,
    quality: QualityGate,
    mutation_lock: Mutex<()>,
}

impl Default for ImportV2Service {
    fn default() -> Self {
        Self {
            sessions: SessionStore::default(),
            engines: EngineRegistry::default(),
            quality: QualityGate::default(),
            mutation_lock: Mutex::new(()),
        }
    }
}

fn transition_item(item: &mut ImportItem, next: ImportItemStatus) -> Result<(), BackendError> {
    if !item.status.can_transition_to(&next) {
        return Err(BackendError::new(
            IMPORT_V2_STATE_INVALID,
            format!("Invalid import item transition: {:?} -> {:?}", item.status, next),
            false,
            true,
        ));
    }
    item.status = next;
    Ok(())
}

fn issue_from_engine_error(error: &BackendError, stage: ImportStage) -> ImportIssue {
    ImportIssue {
        code: error.code.clone(),
        message: "Import engine failed.".into(),
        stage,
        retryable: error.recoverable,
        user_action_required: error.user_action_required,
    }
}
```

`run_item` 必须按以下精确顺序运行：绑定 `task_id` → 先检查 task cancellation；已取消则 item Cancelled 并返回 `IMPORT_V2_CANCELLED`，不尝试把 terminal Task 改回 Running → 未取消时 Task Running → item Inspecting → resolve engine；无引擎则 item WaitingCapability、Task WaitingForConfirmation；有引擎则 item Extracting → 执行 → item Validating → Quality Gate → item PreviewReady → Task result 指向 session/item IDs → Task WaitingForConfirmation。每一步先写 item/session，再发下一状态事件，确保崩溃恢复看到最后一个可靠状态。

每次 item 变更后用同一纯函数按优先级重算 session status：存在 `Inspecting/Extracting/Validating/Committing` 为 `Processing`；同时存在 Completed 与 Failed/Cancelled 为 `PartiallyCommitted`；全部是 Completed/Skipped 且至少一个 Completed 为 `Completed`；全部 Cancelled 为 `Cancelled`；否则存在 `PreviewReady/NeedsMerge/WaitingCapability/WaitingLogin/Failed` 为 `WaitingForConfirmation`；其余为 `Draft`。为每条优先级规则写一条表驱动测试，不允许 commands 自行推导 session status。

取消检测使用现有 `task_service.is_cancelled(task_id)` 和 engine 收到的 `CancellationToken`；取消后 item 为 `Cancelled`，Task 按现有 `Cancelling → Cancelled` 状态流转。

- [ ] **Step 4: 运行 orchestrator、task 和恢复测试**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::orchestrator::tests
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features tasks::task_service::tests
```

Expected: PASS。

- [ ] **Step 5: 提交**

```powershell
git add src-tauri/src/services/import_v2
git commit -m "feat(import): orchestrate import v2 preview tasks"
```

---

### Task 7: 实现逐项原子提交、来源清单和批次历史

**Files:**
- Create: `src-tauri/src/services/import_v2/transaction.rs`
- Create: `src-tauri/src/services/import_v2/commit.rs`
- Modify: `src-tauri/src/services/import_v2/mod.rs`
- Test: both new files

**Interfaces:**
- Implements `ImportV2Service::commit_items`。
- Writes only backend-computed paths from `SourceCommitPlan`。
- New item: no overwrite and no checkpoint required。
- Existing Wiki update: requires `expected_wiki_hash`, explicit `ApplyMergedCandidate`, and Git checkpoint before write。
- Exact duplicate: records origin alias and returns existing source/version without copying raw bytes。

- [ ] **Step 1: 写单项回滚、批次部分成功和外部编辑保护测试**

```rust
#[test]
fn failed_item_commit_rolls_back_only_that_item() {
    let fixture = CommitFixture::two_ready_items();
    fixture.break_second_asset_after_preview();
    let result = fixture.commit_all();
    assert_eq!(result.committed_count, 1);
    assert_eq!(result.failed_count, 1);
    assert!(fixture.first_wiki_path().is_file());
    assert!(!fixture.second_raw_version_dir().exists());
}

#[test]
fn wiki_hash_drift_blocks_update_before_any_write() {
    let fixture = CommitFixture::updated_source();
    fixture.external_edit_current_wiki();
    let result = fixture.commit_with(CommitConflictAction::ApplyMergedCandidate, Some("stale-hash"));
    assert_eq!(result.items[0].error_code.as_deref(), Some(IMPORT_V2_COMMIT_CONFLICT));
    assert_eq!(fixture.current_wiki_text(), "external edit");
    assert!(!fixture.new_raw_version_dir().exists());
}
```

在 `commit.rs` 的测试模块定义 fixture，不把它导出到生产代码：

```rust
struct CommitFixture {
    root: PathBuf,
    context: ProjectContext,
    files: FileStore,
    git: GitService,
    service: ImportV2Service,
    session_id: String,
    first_item_id: String,
    second_item_id: Option<String>,
}
```

`two_ready_items()` 使用 commit 测试模块内的 engine 为每项写 `source.bin`、`candidate.md` 和 `asset.png`，创建两个 PreviewReady item；`updated_source()` 先完成初版提交，再用相同 normalized locator、不同 source bytes 创建新版 PreviewReady item。`break_second_asset_after_preview()` 删除第二项 preview 清单中的 `asset.png`。`commit_all()` 为两个 item 构造无 conflict action 的 decisions。`commit_with()` 只提交 first item。路径查询方法通过读取 `.app/source-index-v2.json` 和 `.app/sources/<source-id>.json` 得到真实后端路径，禁止在测试中重复实现 slug/目标路径算法。`Drop` 删除 root。

- [ ] **Step 2: 运行测试并确认失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::transaction::tests services::import_v2::commit::tests
```

Expected: FAIL，transaction/commit 尚不存在。

- [ ] **Step 3: 实现 `FileTransaction`**

```rust
pub struct FileTransaction {
    backups: Vec<(PathBuf, Option<Vec<u8>>)>,
    created_dirs: Vec<PathBuf>,
    committed: bool,
}

impl FileTransaction {
    pub fn new() -> Self {
        Self { backups: Vec::new(), created_dirs: Vec::new(), committed: false }
    }

    pub fn write(&mut self, path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
        if let Some(parent) = path.parent() {
            let mut missing: Vec<PathBuf> = parent
                .ancestors()
                .take_while(|candidate| !candidate.exists())
                .map(Path::to_path_buf)
                .collect();
            missing.reverse();
            for directory in missing {
                std::fs::create_dir(&directory).map_err(transaction_io_error)?;
                self.created_dirs.push(directory);
            }
        }
        if !self.backups.iter().any(|(existing, _)| existing == path) {
            let previous = if path.exists() {
                Some(std::fs::read(path).map_err(transaction_io_error)?)
            } else {
                None
            };
            self.backups.push((path.to_path_buf(), previous));
        }
        write_atomic_bytes(path, bytes)
    }

    pub fn commit(mut self) {
        self.committed = true;
    }

    fn rollback(&mut self) {
        for (path, previous) in self.backups.iter().rev() {
            match previous {
                Some(bytes) => { let _ = write_atomic_bytes(path, bytes); }
                None => { let _ = std::fs::remove_file(path); }
            }
        }
        for directory in self.created_dirs.iter().rev() {
            let _ = std::fs::remove_dir(directory);
        }
    }
}

impl Drop for FileTransaction {
    fn drop(&mut self) {
        if !self.committed { self.rollback(); }
    }
}
```

`write_atomic_bytes` 必须在目标同目录创建 UUID 临时文件、`sync_all`、rename；Windows 目标存在时先由 transaction 保留 backup，再使用现有 `FileStore` 的 checked-write 语义，不能用跨卷 rename。

- [ ] **Step 4: 实现 commit preflight 和逐项循环**

每个项目严格执行：重新加载 session/item → 必须 selected 且 PreviewReady/NeedsMerge → 重新计算 staging hash → Quality 不能 Fail → SourceRegistry 生成计划 → 检查所有目标和 Wiki expected hash → 必要时创建 Git checkpoint → transaction 写 raw、assets、baseline、Wiki、manifest、`.app/source-index-v2.json` 和当前 batch history 快照 → transaction commit → item Completed → 写 session。批次开始前先创建空 history；每个单项的 history 更新必须包含在同一 `FileTransaction` 中，避免出现“来源已提交但历史仍报告未提交”。

批次函数捕获单项错误并继续下一个项目：

```rust
pub fn commit_items(
    &self,
    context: &ProjectContext,
    file_store: &FileStore,
    git_service: &GitService,
    request: &CommitImportSessionRequest,
) -> Result<ImportBatchResult, BackendError> {
    let batch_id = uuid::Uuid::new_v4().to_string();
    let mut results = Vec::new();
    for decision in &request.decisions {
        let result = match self.commit_one(context, file_store, git_service, &request.session_id, decision) {
            Ok(value) => value,
            Err(error) => ImportItemCommitResult {
                item_id: decision.item_id.clone(),
                source_id: None,
                version_id: None,
                wiki_path: None,
                committed: false,
                error_code: Some(error.code),
            },
        };
        results.push(result);
    }
    let committed_count = results.iter().filter(|item| item.committed).count() as u32;
    let failed_count = results.len() as u32 - committed_count;
    let batch = ImportBatchResult {
        batch_id,
        session_id: request.session_id.clone(),
        committed_count,
        failed_count,
        items: results,
    };
    file_store.write_json_atomic(
        context,
        &format!(".app/import-history/{}.json", batch.batch_id),
        &batch,
    )?;
    Ok(batch)
}
```

- [ ] **Step 5: 运行 commit、FileStore 和 Git 回归**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_v2::commit::tests
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::file_store::tests
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::git_service::tests
```

Expected: PASS，失败项目无残留目标。

- [ ] **Step 6: 提交**

```powershell
git add src-tauri/src/services/import_v2
git commit -m "feat(import): commit import v2 items atomically"
```

---

### Task 8: 暴露薄 Tauri IPC 和 TypeScript contract

**Files:**
- Create: `src-tauri/src/commands/import_v2_commands.rs`
- Modify: `src-tauri/src/commands/mod.rs:1-15`
- Modify: `src-tauri/src/app_state.rs:8-35`
- Modify: `src-tauri/src/lib.rs:113-180`
- Create: `src/types/importV2.ts`
- Create: `src/types/importV2.test.ts`
- Test: `src-tauri/src/commands/import_v2_commands.rs`

**Interfaces:**
- `create_import_session_v2(request) -> ImportSession`
- `get_import_session_v2(request) -> ImportSession`
- `add_import_items_v2(request) -> ImportSession`
- `set_import_item_selection_v2(request) -> ImportSession`
- `start_import_items_v2(request) -> Vec<BackendTask>`
- `confirm_import_session_v2(request) -> BackendTask`
- Cancellation continues through existing `cancel_task(taskId)`; each item exposes its `taskId`。

- [ ] **Step 1: 写 request 序列化和项目作用域测试**

```rust
#[test]
fn add_items_request_uses_ids_and_inputs_not_target_paths() {
    let request = AddImportItemsV2Request {
        project_id: "p1".into(),
        project_root_path: "D:/wiki".into(),
        session_id: "s1".into(),
        inputs: vec![ImportInput {
            kind: ImportInputKind::File,
            display_name: "a.pdf".into(),
            locator: "D:/in/a.pdf".into(),
            normalized_locator: None,
        }],
    };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["sessionId"], "s1");
    assert!(value.get("targetPath").is_none());
    assert!(value.get("wikiPath").is_none());
}
```

- [ ] **Step 2: 运行测试并确认失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features commands::import_v2_commands::tests
```

Expected: FAIL，command module 尚不存在。

- [ ] **Step 3: 实现 request DTO 与薄 command**

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateImportSessionV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub resource_mode: ImportResourceMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddImportItemsV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub inputs: Vec<ImportInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetImportSessionV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetImportItemSelectionV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub selected: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartImportItemsV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_ids: Vec<String>,
}

#[tauri::command]
pub fn create_import_session_v2(
    state: State<'_, AppState>,
    request: CreateImportSessionV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.create_session(
        &context,
        &state.file_store,
        request.resource_mode,
    )
}

#[tauri::command]
pub fn add_import_items_v2(
    state: State<'_, AppState>,
    request: AddImportItemsV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.import_v2_service.add_inputs(
        &context,
        &state.file_store,
        &request.session_id,
        request.inputs,
    )
}
```

`start_import_items_v2` 仿照当前 `preview_import`：先为每个 item 创建 project-scoped `TaskType::Import`，再用 `tauri::async_runtime::spawn` 重新取得 `app.state::<AppState>()` 并调用 `run_item`。`confirm_import_session_v2` 创建一个可取消 Import task，在后台调用 `commit_items`，将 batch history path 放入 `TaskResult.affected_paths`。

- [ ] **Step 4: 注入 AppState 与注册命令**

在 `AppState` 增加：

```rust
pub import_v2_service: ImportV2Service,
```

在 `commands/mod.rs` 增加 `pub mod import_v2_commands;`，在 `generate_handler!` 注册六个 `_v2` 命令。旧 `import_commands::*` 全部保留。

- [ ] **Step 5: 创建同值 TypeScript 类型**

`src/types/importV2.ts` 必须至少导出：

```ts
import type { TaskProgress } from "./task";

export type ImportResourceMode = "balanced" | "performance" | "saver";
export type ImportInputKind = "file" | "folder" | "url";
export type ImportStage = "inspect" | "route" | "extract" | "validate" | "commit";
export type QualityLevel = "pass" | "warning" | "fail";
export type ArtifactKind = "source_snapshot" | "markdown" | "image" | "attachment" | "subtitle" | "metadata";
export type AttemptOutcome = "succeeded" | "failed" | "cancelled";
export type ImportItemStatus =
  | "queued" | "inspecting" | "waiting_capability" | "waiting_login"
  | "extracting" | "validating" | "preview_ready" | "needs_merge"
  | "committing" | "completed" | "paused" | "cancelled" | "skipped" | "failed";

export interface ImportInput {
  kind: ImportInputKind;
  displayName: string;
  locator: string;
  normalizedLocator: string | null;
}

export interface QualityMetric {
  code: string;
  actual: number;
  minimum: number;
  passed: boolean;
}

export interface QualityReport {
  level: QualityLevel;
  metrics: QualityMetric[];
  warnings: string[];
}

export interface ImportArtifact {
  kind: ArtifactKind;
  relativePath: string;
  sha256: string;
  sizeBytes: number;
}

export interface AttemptRecord {
  route: string;
  engineId: string;
  engineVersion: string;
  stage: ImportStage;
  startedAt: string;
  completedAt: string | null;
  outcome: AttemptOutcome;
  warnings: string[];
}

export interface ImportIssue {
  code: string;
  message: string;
  stage: ImportStage;
  retryable: boolean;
  userActionRequired: boolean;
}

export interface ImportPreviewArtifact {
  markdown: ImportArtifact;
  assets: ImportArtifact[];
  sourceSnapshot: ImportArtifact;
  quality: QualityReport;
  title: string;
}

export interface ImportItem {
  itemId: string;
  input: ImportInput;
  status: ImportItemStatus;
  selected: boolean;
  taskId: string | null;
  progress: TaskProgress | null;
  attempts: AttemptRecord[];
  preview: ImportPreviewArtifact | null;
  issue: ImportIssue | null;
}

export interface ImportSession {
  schemaVersion: 2;
  sessionId: string;
  projectId: string;
  status: "draft" | "processing" | "waiting_for_confirmation" | "partially_committed" | "completed" | "cancelled";
  resourceMode: ImportResourceMode;
  createdAt: string;
  updatedAt: string;
  items: ImportItem[];
}

export type CommitConflictAction = "create_new" | "keep_wiki" | "apply_merged_candidate";

export interface CommitItemDecision {
  itemId: string;
  conflictAction: CommitConflictAction | null;
  expectedWikiHash: string | null;
}

export interface CommitImportSessionRequest {
  projectId: string;
  projectRootPath: string;
  sessionId: string;
  decisions: CommitItemDecision[];
}

export interface ImportItemCommitResult {
  itemId: string;
  sourceId: string | null;
  versionId: string | null;
  wikiPath: string | null;
  committed: boolean;
  errorCode: string | null;
}

export interface ImportBatchResult {
  batchId: string;
  sessionId: string;
  committedCount: number;
  failedCount: number;
  items: ImportItemCommitResult[];
}
```

禁止 `any`、自由 `targetPath` 和自由 `wikiPath`。对每个 Rust serialization test 生成的 JSON fixture，在 Vitest 中断言能赋值给对应 TypeScript contract；fixture 只使用相对项目路径和虚构 locator。

`src/types/importV2.test.ts` 至少包含一个完整最小 session：

```ts
import { describe, expect, it } from "vitest";
import type { ImportSession } from "./importV2";

describe("Import V2 contract", () => {
  it("accepts the Rust camelCase session shape without writable target paths", () => {
    const session = {
      schemaVersion: 2,
      sessionId: "session-1",
      projectId: "project-1",
      status: "draft",
      resourceMode: "balanced",
      createdAt: "2026-07-11T00:00:00Z",
      updatedAt: "2026-07-11T00:00:00Z",
      items: [],
    } satisfies ImportSession;
    expect(session.schemaVersion).toBe(2);
    expect("targetPath" in session).toBe(false);
    expect("wikiPath" in session).toBe(false);
  });
});
```

- [ ] **Step 6: 运行 GUI、Rust 和 TypeScript 检查**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features commands::import_v2_commands::tests
cargo check --manifest-path src-tauri/Cargo.toml
npx vitest run src/types/importV2.test.ts
npm run lint
npm run build
```

Expected: all exit 0；现有 ImportView 仍使用旧 API 且正常编译。

- [ ] **Step 7: 提交**

```powershell
git add src-tauri/src/commands/import_v2_commands.rs src-tauri/src/commands/mod.rs src-tauri/src/app_state.rs src-tauri/src/lib.rs src/types/importV2.ts src/types/importV2.test.ts
git commit -m "feat(import): expose import v2 core ipc"
```

---

### Task 9: 完成跨模块端到端、安全与恢复验收

**Files:**
- Create: `src-tauri/tests/import_v2_core.rs`
- Modify: `SPEC/progress.txt` (prepend one milestone entry after all checks pass)
- Test: `src-tauri/tests/import_v2_core.rs`

**Interfaces:**
- Integration test defines its own `FixtureEngine` implementing the public `ImportEngine` trait；生产代码不暴露测试专用引擎。
- The test runs service APIs, not Tauri window automation。

- [ ] **Step 1: 写端到端失败测试**

测试必须覆盖以下单个场景中的完整链路：两个输入，一个成功、一个引擎失败；关闭并重建 service 后恢复；确认成功项；失败项不生成 raw/wiki；重新加入相同内容得到 exact duplicate；修改 Wiki 后不带正确 hash 的更新被阻止。

```rust
#[test]
fn import_v2_core_is_resumable_partial_atomic_and_deduplicated() {
    let fixture = CoreIntegrationFixture::new();
    fixture.service.register_engine(Arc::new(FixtureEngine::default())).unwrap();
    let session = fixture.create_two_item_session();
    fixture.run_item(&session.items[0].item_id).unwrap();
    fixture.engine_fail_next();
    assert!(fixture.run_item(&session.items[1].item_id).is_err());

    let reopened = fixture.reopen_service().load_session(
        &fixture.context,
        &fixture.files,
        &session.session_id,
    ).unwrap();
    assert_eq!(reopened.items[0].status, ImportItemStatus::PreviewReady);
    assert_eq!(reopened.items[1].status, ImportItemStatus::Failed);

    let batch = fixture.commit_selected(&reopened.items[0].item_id);
    assert_eq!(batch.committed_count, 1);
    assert_eq!(fixture.raw_version_count(), 1);
    assert_eq!(fixture.wiki_page_count(), 1);
    assert!(fixture.reimport_same_bytes_is_exact_duplicate());
    assert!(fixture.external_edit_update_is_blocked());
}
```

- [ ] **Step 2: 运行测试并确认至少一个断言失败**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test import_v2_core -- --nocapture
```

Expected: FAIL，直到前八个任务的 integration glue 完整。

- [ ] **Step 3: 实现 integration fixture 并让测试通过**

在 `src-tauri/tests/import_v2_core.rs` 定义 `FixtureEngine` 与 `CoreIntegrationFixture`。`FixtureEngine` 实现公开 `ImportEngine`，将 `source.bin` 和 `candidate.md` 写入 request 指定的 staging；用 `AtomicBool fail_next` 控制下一次返回 `BackendError::new("FIXTURE_ENGINE_FAILED", "Fixture failure.", true, false)`。`CoreIntegrationFixture` 必须持有以下字段并只调用公开 service API：

```rust
struct CoreIntegrationFixture {
    root: PathBuf,
    context: ProjectContext,
    files: FileStore,
    git: GitService,
    tasks: TaskService,
    service: ImportV2Service,
    engine: Arc<FixtureEngine>,
}
```

它实现测试中出现的十个方法：`new`、`create_two_item_session`、`run_item`、`engine_fail_next`、`reopen_service`、`commit_selected`、`raw_version_count`、`wiki_page_count`、`reimport_same_bytes_is_exact_duplicate`、`external_edit_update_is_blocked`。所有计数只扫描 fixture root 下明确的 `raw/sources/` 和 `wiki/sources/`；`Drop` 删除 fixture root。若测试暴露生产 glue 缺口，允许修改范围仅限 `services/import_v2/`、`commands/import_v2_commands.rs`、`models/import_v2.rs`，并必须保留对应失败断言。

- [ ] **Step 4: 运行定向、Rust 全套和统一检查**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test import_v2_core -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features
npm run check
```

Expected: integration test PASS；Rust 全套 0 failed；`npm run check` exit 0。

- [ ] **Step 5: 执行项目要求的双评审**

评审 A 使用共享上下文，逐条核对设计规范第 3–7、10–13、16 节与本计划；评审 B 使用 fresh context，只给它 diff、设计规范和测试命令，寻找路径越界、状态竞态、非原子写入、secret 泄露和缺失测试。合并问题清单，修复所有有效问题。

- [ ] **Step 6: 修复后从头重跑统一检查**

```powershell
npm run check
```

Expected: exit 0；不得只重跑失败子命令。

- [ ] **Step 7: 更新进度并提交 Core 里程碑**

在 `SPEC/progress.txt` 标题下加入：

```text
[2026-07-11] Import 2.0 Core — Added versioned DTOs, resumable sessions, engine protocol, quality gating, immutable source registry, atomic per-item commits, task integration, and V2 IPC without changing the legacy importer — Key decision: keep real file/web/Agent engines behind the stable ImportEngine boundary and defer command cutover until full regression.
```

然后提交：

```powershell
git add src-tauri/src/models/import_v2.rs src-tauri/src/errors/error_codes.rs src-tauri/src/services/import_v2 src-tauri/src/services/mod.rs src-tauri/src/commands/import_v2_commands.rs src-tauri/src/commands/mod.rs src-tauri/src/app_state.rs src-tauri/src/lib.rs src-tauri/tests/import_v2_core.rs src/types/importV2.ts src/types/importV2.test.ts SPEC/progress.txt
git commit -m "feat(import): complete import v2 core"
```

Expected: 工作区只剩实施前已存在且明确排除的未跟踪参考文件；commit 不包含 `.superpowers/` 或 `UI-Frontend-design/import-v2-reference.html`。

---

## 4. 计划自检清单

### 4.1 设计规范覆盖映射

| 设计规范要求 | 本计划落点 |
| --- | --- |
| 总体架构、薄 command、后端唯一写入边界 | Tasks 1、6、8 |
| 会话、逐项状态、重启恢复、部分成功 | Tasks 2、6、7、9 |
| `.app/import-sessions`、`raw/sources/<source>/<version>`、baseline、Wiki | Tasks 2、4、7 |
| 去重、同源更新、不可变版本、expected hash | Tasks 4、7、9 |
| 能力包 JSON-RPC 与 staging 边界 | Task 3；真实 runner 明确进入 File Ingestion 计划 |
| Quality Gate、安全 Markdown、资源 hash | Task 5 |
| 取消、TaskService、脱敏 issue | Task 6 |
| Git checkpoint、原子提交、history | Task 7 |
| 前后端 typed contract、旧 API 不变 | Task 8 |
| 安全、恢复、CJK 和统一验收 | Task 9 |
| 文件解析、OCR、ASR、LibreOffice、FFmpeg | File Ingestion 计划，不属于 Core |
| 通用网页与平台路由 | Web Ingestion 计划，不属于 Core |
| 平衡自主 Agent | Agent Assistance 计划，不属于 Core |
| 旧数据迁移与一次性替换 | Migration and Cutover 计划，不属于 Core |

自检结论：Core 所需设计要求均有任务；其余要求均明确归属后续独立计划，没有未分配范围。

实施者开始前确认：

- [ ] 每个 Task 都先执行失败测试，再实施。
- [ ] 旧 `ImportService`、旧 DTO 和旧 Tauri command 在 Core 阶段没有行为变化。
- [ ] `ImportV2Service` 的写入只来自后端计算的 session/item/source plan。
- [ ] 引擎只写 staging，不能写 `raw/`、`wiki/`、`.app/sources/` 或 Git。
- [ ] Quality Fail 无法进入 commit；Quality Warning 仍需用户确认。
- [ ] 相同 hash 默认去重，同一来源新 hash 新增不可变 version。
- [ ] Wiki 外部编辑通过 expected hash 保护，高风险更新先 checkpoint。
- [ ] 单项失败没有残留文件，批次其他成功项保持完成。
- [ ] 应用重启可从 session/item JSON 恢复，不依赖内存 registry 中的任务状态。
- [ ] 错误和日志不含 secrets；路径错误不回显未经脱敏的绝对用户名目录。
- [ ] CJK、Unicode、Windows 路径和大小写测试通过。
- [ ] `npm run check` 在评审修复后从头通过。

## 5. Core 完成定义

只有满足以下全部条件才可开始 File Ingestion 计划：

1. 新 V2 contract 已注册，但现有 Import UI 和旧命令仍可工作。
2. fixture engine 端到端流程可恢复、可取消、可部分提交、可去重。
3. 所有正式目标路径由 SourceRegistry 生成，IPC 不接受自由写路径。
4. 原始来源按 source/version 追加保存；外部编辑不会被静默覆盖。
5. 定向测试、Rust 全套、`npm run check` 和两轮评审全部通过。
6. Core commit 范围干净，未包含视觉伴侣、UI 参考或真实解析器依赖。
