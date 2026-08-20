# LLM Wiki Desktop 后端架构说明

> Import V2、来源版本、媒体处理、登录态、OCR / ASR 和独立编译的目标后端边界，以 [`../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md) 为准。本文中的 legacy `ImportService` 模块说明仅描述现状，不得覆盖新规范。
> Batch 9 收口后，旧 `list_imported_sources` / `request_delete_source` / `request_replace_source` 不再注册为生产命令；Source 生命周期只经 typed `source_commands` 与 Import V2 服务。旧 compile index adapter 与 legacy asset fallback 仅为只读兼容边界，并有“不改写 legacy 文件”的独立测试。
> Workflows 的项目隔离队列、状态、结构化阶段、确认、重试与恢复合同，以 [`../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md) 为准。本文区分当前任务 DTO 与待迁移目标，不能把现有 Agent 页面行为当成目标后端合同。
> 无项目工作台、新建知识库、typed 打开评估、受限 / 信任 / 只读、兼容启用、修复和深度扫描的目标合同，以 [`../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md) 为准。该 typed assessment/access/trust/repair 管线已落地；普通资料目录只允许新建知识库后导入，不会原地初始化或隐式初始化 Git。

## 1. 文档目的

本文面向后续开发 Agent / Claude Code，用来确定 LLM Wiki Desktop 的 Tauri / Rust 后端架构、目录结构、模块职责、IPC 边界、任务模型、错误模型和安全规则。

本文同时记录当前已实现后端结构事实与已确认的目标合同；两者冲突时，带日期的已确认设计决定目标行为，“当前实现”段落只用于迁移定位。后续调整后端时，应同时核对：

- `PRD.md`
- `SPEC.md`
- `APP_flow.md`
- `TECH_STACK.md`
- `BACKEND_STRUCTURE.md`

本文重点规定后端架构，不重复 UI 细节。前端视图和用户流程以 `APP_flow.md` 为准；技术栈和跨层边界以 `TECH_STACK.md` 为准。

## 2. 后端设计原则

- Tauri command 层保持薄，业务逻辑放在 service 层。
- 后端所有输入输出使用结构化 DTO，不用临时字符串承载复杂状态。
- 项目内容保持 Markdown + JSON + 本地文件，不引入数据库。
- 可以在内存中建立临时索引、缓存和受限/只读操作结果；需要持久化的项目状态只能落到 `ProjectLayout` 允许的项目内逻辑根，不能旁路写入用户目录。
- 所有项目文件读写必须经过 `ProjectContext` 路径安全校验。
- 长任务统一进入 `TaskService`，支持进度、日志、取消、后台运行和事件推送。
- 高风险操作统一返回 `PendingAction`，由用户确认后继续执行。
- 所有 command 使用统一错误模型 `BackendError`。
- API Key 和访问令牌只进入系统凭据管理，不写入项目文件。
- Agent CLI、BYOK 与本地规则是产品工作流背后的执行路径；默认值由设置决定，单次运行可显式覆盖，所选路径不可用时不得静默回退。Import 解析恢复仍不允许使用 BYOK。
- Git 检查点是 checkpoint-required mutation 的强制安全边界；它不授权在评估或打开外部目录时初始化 Git。无 Git 时必须按能力降级。
- 外部目录先做零写入 quick assessment，不创建 `.app`、Git、缓存或项目任务，不执行项目内容，也不发送外部 AI。
- 格式、健康、权限和 Git 分别建模；受限 / 只读写保护必须在 command 与 service 执行点重新校验。

## 3. 当前目录结构

后端源码位于 Tauri 标准目录。以下 command、model、service 和 task 布局与当前 `src-tauri/src/` 一致：

```text
src-tauri/
├── Cargo.toml
├── tauri.conf.json
└── src/
    ├── main.rs
    ├── lib.rs
    ├── app_state.rs
    ├── commands/
    │   ├── agent_commands.rs
    │   ├── chat_commands.rs
    │   ├── compile_commands.rs
    │   ├── export_commands.rs
    │   ├── file_commands.rs
    │   ├── git_commands.rs
    │   ├── graph_commands.rs
    │   ├── import_commands.rs
    │   ├── lint_commands.rs
    │   ├── llm_commands.rs
    │   ├── mod.rs
    │   ├── project_commands.rs
    │   ├── search_commands.rs
    │   ├── settings_commands.rs
    │   ├── task_commands.rs
    │   └── wiki_commands.rs
    ├── models/
    │   ├── agent.rs
    │   ├── bookmark.rs
    │   ├── chat.rs
    │   ├── compile.rs
    │   ├── confirmation.rs
    │   ├── export.rs
    │   ├── git.rs
    │   ├── graph.rs
    │   ├── import.rs
    │   ├── lint.rs
    │   ├── llm.rs
    │   ├── mod.rs
    │   ├── paths.rs
    │   ├── project.rs
    │   ├── search.rs
    │   ├── settings.rs
    │   ├── task.rs
    │   └── wiki.rs
    ├── services/
    │   ├── agent_service.rs
    │   ├── bookmark_service.rs
    │   ├── chat_convenience_service.rs
    │   ├── chat_service/
    │   │   ├── citations.rs
    │   │   ├── mod.rs
    │   │   ├── retrieval.rs
    │   │   ├── saved_answers.rs
    │   │   ├── sessions.rs
    │   │   └── test_support.rs
    │   ├── compile_instructions.rs
    │   ├── compile_service.rs
    │   ├── export_service.rs
    │   ├── extraction_service.rs
    │   ├── file_store.rs
    │   ├── git_service.rs
    │   ├── graph_service.rs
    │   ├── import_service/
    │   │   ├── artifacts.rs
    │   │   ├── classification.rs
    │   │   ├── confirmation.rs
    │   │   ├── mod.rs
    │   │   ├── preview.rs
    │   │   ├── promotion.rs
    │   │   ├── source_actions.rs
    │   │   ├── source_catalog.rs
    │   │   └── test_support.rs
    │   ├── lint_service/
    │   │   ├── deep.rs
    │   │   ├── fixes.rs
    │   │   ├── ignores.rs
    │   │   ├── mod.rs
    │   │   ├── reports.rs
    │   │   ├── rules.rs
    │   │   └── test_support.rs
    │   ├── llm_service.rs
    │   ├── mod.rs
    │   ├── project_service.rs
    │   ├── search_service/
    │   │   ├── catalog.rs
    │   │   ├── excerpts.rs
    │   │   ├── mod.rs
    │   │   ├── pages.rs
    │   │   ├── query.rs
    │   │   └── test_support.rs
    │   ├── secret_service.rs
    │   ├── settings_service.rs
    │   └── wiki_index.rs
    ├── tasks/
    │   ├── byok_progress.rs
    │   ├── cancellation.rs
    │   ├── mod.rs
    │   ├── task_events.rs
    │   ├── task_model.rs
    │   └── task_service.rs
    ├── errors/
    │   ├── mod.rs
    │   ├── backend_error.rs
    │   └── error_codes.rs
    └── utils/
        ├── i18n.rs
        ├── markdown_utils.rs
        ├── mod.rs
        ├── path_utils.rs
        ├── time_utils.rs
        └── url_utils.rs
```

四个稳定 facade 目录的完整文件集合也可写为：

```text
services/import_service/{artifacts,classification,confirmation,preview,promotion,source_actions,source_catalog,test_support}.rs
services/search_service/{catalog,excerpts,pages,query,test_support}.rs
services/lint_service/{deep,fixes,ignores,reports,rules,test_support}.rs
services/chat_service/{citations,retrieval,saved_answers,sessions,test_support}.rs
```

各目录另有定义 facade 与模块边界的 `mod.rs`。

物理文件可以继续按用例拆分，但 facade、DTO 和持久化兼容性不能因为文件移动而改变。

## 4. 后端分层总览

```text
React shell / feature workflows
  -> typed Tauri invoke
      -> thin command modules
          -> AppState + ProjectRegistry
              -> stable service facades
              -> TaskService
              -> ConfirmationRegistry
                  -> local files / Git / Agent CLI / LLM API / OS credentials
```

各层职责：

- `commands/`：接收前端请求、参数校验、调用 service、返回 DTO。
- `app_state.rs`：持有共享服务实例和当前应用级状态。
- `services/`：稳定 facade 和聚焦用例实现；`services/mod.rs` 是 crate-facing re-export boundary。
- `tasks/`：后台任务生命周期、进度、日志、取消和事件。
- `models/`：领域模型、DTO、持久化 JSON 结构。
- `errors/`：统一错误类型和错误码。
- `utils/`：无状态工具函数。

不要在 command 函数里实现导入、Git、Agent、Lint、导出等核心逻辑。

当前 facade 依赖规则：

- commands 和 `AppState` 依赖 facade 类型，绝不依赖私有 use-case 子模块。
- `services/mod.rs` 是面向 crate 的统一 re-export boundary；跨层调用从这里取得稳定类型。
- `ImportService`、`SearchService`、`LintService`、`ChatService` 的多个聚焦 `impl Service` block 分布在用例子模块中，共同实现同一个稳定 facade。
- 子模块使用满足协作所需的最窄可见性；默认 `mod` 私有，兄弟模块共享 helper 才使用 `pub(super)`，计划级共享常量才使用 `pub(crate)`。
- `src-tauri/tests/service_facade_contracts.rs` 保护 facade 的构造方式和选定公开契约。
- `ChatConvenienceService` 与 `WikiIndex` 保持独立：前者承载 Chat 便捷写入的意图与变更审计，后者承载项目级只读内存索引；二者不并入四个 facade。
- command 注册继续在 `lib.rs` 中通过 `tauri::generate_handler!` 显式维护。
- 物理文件移动不改变 command DTO、公开 facade 或 layout-defined app-state JSON（原生映射为 `.app/*.json`）的持久化兼容性。

## 5. Tauri IPC 规则

### 5.1 Command 层职责

Command 层只做：

- 反序列化请求 DTO。
- 基础参数校验。
- 从 `AppState` 获取 service。
- 调用 service。
- 将结果转换为响应 DTO。
- 将内部错误转换为 `BackendError`。

Command 层不做：

- 复杂文件迁移。
- Git 检查点策略。
- Agent 进程生命周期管理。
- LLM 请求上下文组装。
- 图谱构建。
- Lint 修复。
- HTML 生成。

GUI command 模块由 `commands/mod.rs` 明确导出，并在 `lib.rs` 的 `tauri::generate_handler!` 中逐项注册。仅增加 Rust 函数而遗漏显式注册，不构成可调用 IPC 接口。

### 5.2 结构化输入输出

每个 command 应有明确请求 / 响应类型。项目打开使用评估与执行两步，不使用一个 `open_project(path)` 隐式判断并写盘：

```rust
pub struct StartProjectOpenAssessmentRequest {
    pub path: String,
}

pub struct StartProjectOpenAssessmentResponse {
    pub assessment_operation_id: String,
}

pub struct CancelProjectOpenAssessmentRequest {
    pub assessment_operation_id: String,
}

pub struct ProjectOpenAssessment {
    pub assessment_id: String,
    pub canonical_root: String,
    pub format: ProjectFormat,
    pub trust: ProjectTrustState,
    pub filesystem_access: ProjectFilesystemAccess,
    pub health: ProjectHealth,
    pub layout: ProjectLayout,
    pub git: ProjectGitAssessment,
    pub capabilities: ProjectCapabilities,
    pub warnings: Vec<ProjectWarning>,
    pub confidence: AssessmentConfidence,
    pub recommended_actions: Vec<ProjectOpenAction>,
}

pub struct OpenAssessedProjectRequest {
    pub assessment_id: String,
    pub action: ProjectOpenAction,
}
```

`assessment_operation_id` 是应用级、可取消的运行句柄，不属于任何项目 Task；查询 / 取消只能使用该 opaque ID。取消会丢弃未完成快照并保持 no-project shell。完成后返回的 `assessment_id` 对应后端短期保存的评估快照。打开、信任、兼容启用或修复确认时必须重新校验 canonical identity、trust、filesystem access、Git 和相关文件 hash；前端回传的 assessment 对象不能作为执行依据。修复使用独立 `ProjectRepairPlan` / `repair_plan_id`，不要与 Lint 内容修复混用。

不要使用 `HashMap<String, String>` 或自由 JSON 作为长期接口，除非该字段确实是插件或 provider 的开放配置。

### 5.3 事件推送

后端通过 Tauri event 向前端推送：

- 任务进度。
- 任务日志。
- 任务完成。
- 任务失败。
- 取消状态。
- 确认请求。
- 项目刷新。
- Agent 输出。

事件名建议稳定命名，例如：

- `task://updated`
- `task://log`
- `task://completed`
- `task://failed`
- `confirmation://requested`
- `project://refreshed`
- `agent://output`

事件 payload 必须是结构化 JSON。

## 6. AppState 与依赖管理

`AppState` 持有稳定 service facade、进程级任务 / 确认运行态以及多项目可信根注册表。进程级持有不等于跨项目可见：所有任务与确认操作仍必须按 `project_id` 授权和筛选。当前结构为：

```rust
pub struct AppState {
    pub project_registry: ProjectRegistry,
    pub project_service: ProjectService,
    pub file_store: FileStore,
    pub import_service: ImportService,
    pub extraction_service: ExtractionService,
    pub git_service: GitService,
    pub agent_service: AgentService,
    pub bookmark_service: BookmarkService,
    pub chat_convenience_service: ChatConvenienceService,
    pub chat_service: ChatService,
    pub llm_service: LlmService,
    pub search_service: SearchService,
    pub graph_service: GraphService,
    pub lint_service: LintService,
    pub export_service: ExportService,
    pub settings_service: SettingsService,
    pub secret_service: SecretService,
    pub task_service: TaskService,
    pub confirmation_registry: ConfirmationRegistry,
}
```

当前 `ProjectRegistry` 把已打开项目的 `project_id` 映射到 canonical root；command 使用 `AppState::resolve_project_context` 校验调用方断言的 id / root 组合。目标条目还必须携带后端派生的 format、trust、filesystem access、health、layout 和 capabilities。路径注册只是运行时句柄授权，不等于用户已经信任该目录。

全局 `ProjectTrustStore`（或等价持久层）保存 canonical identity 绑定的用户信任与歧义 Markdown 意图；`ProjectAssessmentRegistry` / repair registry 保存短期、可重校验的评估与计划。三者不能合并成一个“trusted ProjectRegistry”。需要并发共享的实现细节可以使用 `Arc`、`Mutex`、`RwLock` 或内部 channel，但不能让 command 绕过 facade 直接获取私有模块状态。

规则：

- Service 之间依赖要明确。
- 共享状态尽量集中在 `AppState`。
- 当前项目上下文通过 `ProjectContext` 传递。
- 不要让 service 随意从全局变量读取当前项目。
- `AppState` 的 service 字段类型来自 `crate::services` 的稳定 re-export，`TaskService` 来自 `crate::tasks` 的 re-export；`ConfirmationRegistry` 和 `ProjectContext` 从 `crate::models` 导入，`ProjectRegistry` 在 `app_state.rs` 本地定义。上述依赖均不引用四个 facade 目录内的私有模块。

## 7. ProjectContext 路径安全边界

`ProjectContext` 是后端访问项目文件的安全边界。

建议字段：

```rust
pub struct ProjectContext {
    pub project_id: String,
    pub root: PathBuf,
    pub format: ProjectFormat,
    pub health: ProjectHealth,
    pub layout: ProjectLayout,
    pub access: ProjectAccessPolicy,
    pub capabilities: ProjectCapabilities,
}
```

`ProjectAccessPolicy` 至少携带独立的 `trust: ProjectTrustState`、`filesystem_access: ProjectFilesystemAccess` 与后端派生 capabilities；health 和 layout 也参与授权。`trusted + read_only` 与 `untrusted + read_only` 都必须可表达。

`ProjectLayout` 使用逻辑路径而不是固定目录假设，至少覆盖 app state、evidence、Markdown read roots（带 Source/Wiki/mixed role 与 exclusions）、Source/Wiki/query write roots、export root、workflow/task state、graph cache、Lint report 和 purpose/schema 上下文。原生项目映射到 `.app/`、`raw/`、`wiki/`、`exports/` 与 `skills/`；兼容 vault 不假设这些目录都存在。所有项目读取与写入必须通过 `ProjectContext` 解析路径，并在执行点检查所需 capability；缺少写根时返回 typed prerequisite。

硬规则：

- 前端不能让后端直接读写任意绝对路径。
- 项目根本身可以是符号链接 / junction，但注册前必须 canonicalize。后端必须拒绝 `../`、绝对路径注入以及 canonical 目标逃逸；根内链接只在仍被根包含且无循环时读取，根外链接只展示，不跟随、不索引、不写入。
- 内部逻辑使用规范化路径。
- 对 UI 返回路径时可以返回展示路径和项目相对路径。
- 项目相对路径统一使用正斜杠。
- 必须安全处理 Unicode 和 CJK 文件名。

建议提供方法：

```rust
impl ProjectContext {
    pub fn resolve_project_path(&self, relative_path: &str) -> Result<PathBuf, BackendError>;
    pub fn resolve_layout_path(&self, logical_root: ProjectLogicalRoot, relative_path: &str) -> Result<PathBuf, BackendError>;
    pub fn to_project_relative(&self, absolute_path: &Path) -> Result<String, BackendError>;
}
```

任何 service 操作项目文件前，都应先经过这些方法。

## 8. 统一错误模型

所有 command 返回统一错误模型。

建议结构：

```rust
pub struct BackendError {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub recoverable: bool,
    pub user_action_required: bool,
}
```

字段说明：

- `code`：稳定错误码，供前端分支处理。
- `message`：可展示给用户或开发者的简短说明。
- `details`：结构化详情，例如文件路径、任务 id、provider 名称。
- `recoverable`：是否可通过重试、重新选择路径、修改配置恢复。
- `user_action_required`：是否需要用户确认、授权或补充配置。

错误码建议按领域前缀：

- `PROJECT_*`
- `PATH_*`
- `FILE_*`
- `IMPORT_*`
- `EXTRACT_*`
- `GIT_*`
- `AGENT_*`
- `LLM_*`
- `SECRET_*`
- `TASK_*`
- `GRAPH_*`
- `LINT_*`
- `EXPORT_*`
- `SETTINGS_*`

实现建议使用 `thiserror` 定义内部错误，再转换为前端可序列化的 `BackendError`。

## 9. 统一确认模型 PendingAction

高风险操作必须先返回待确认动作，由前端展示确认 UI，用户确认后再继续。

适用场景：

- 对兼容知识库启用完整功能。
- 应用项目打开修复计划。
- 删除文件。
- 覆盖文件。
- 批量替换。
- 删除或替换原始资料。
- Wiki 编译冲突合并。
- Agent 自动修复。
- 执行 Agent 安装命令。
- 执行可能影响大量文件的 Skill。

建议模型：

```rust
pub struct PendingAction {
    pub id: String,
    pub action_type: PendingActionType,
    pub title: String,
    pub message: String,
    pub risk_level: RiskLevel,
    pub affected_paths: Vec<String>,
    pub preview: Option<ActionPreview>,
    pub checkpoint_policy: CheckpointPolicy,
    pub checkpoint_available: bool,
    pub expires_at: Option<String>,
}

pub enum CheckpointPolicy {
    Required,
    Optional,
    None,
}
```

确认流程：

1. 前端调用 command。
2. 后端发现需要确认，创建 `PendingAction`。
3. 后端返回 `PendingActionRequired` 或通过事件推送确认请求。
4. 前端展示确认 UI。
5. 用户确认后，前端调用 `confirm_pending_action(id)`。
6. 后端再次校验当前状态。
7. 后端按保存的 `checkpoint_policy` 处理 Git：`Required` 必须成功创建；`Optional` 只在用户于确认模型中明确选择时创建；`None` 不创建。策略和用户选择都由后端保存并在确认时重验。
8. 后端执行操作。

兼容启用属于显式写入确认，但不等于所有情况都强制 Git：创建全新的 `.app/compat` 文件可使用 `Optional`，默认选择初始化本地 Git；用户拒绝后仍可完成已明确确认的兼容启用，但后续 checkpoint-required 写入保持禁用。删除、覆盖、批量重写、冲突合并等高风险动作使用 `Required`。

不要让前端自己拼接危险操作的继续参数。继续执行必须由后端保存的 `PendingAction` 驱动。

## 10. 统一后台任务模型

长任务必须统一进入 `TaskService`。

后台任务包括：

- 兼容知识库深度扫描（只读、可取消、持续发布 partial snapshot）。
- 导入解析。
- 媒体下载、OCR、ASR 和能力安装。
- Source AI 整理、重新 OCR / ASR 和平台刷新。
- 更新 Wiki。
- Agent 执行。
- BYOK LLM 请求。
- 图谱首次构建。
- 健康检查。
- 自动修复。
- 生成内容与 HTML / 卡片 / 报告导出。

当前公开任务 DTO 定义在 `models/task.rs`，任务运行态、取消令牌和状态迁移定义在 `tasks/task_model.rs`；`TaskService` 通过 `tasks/mod.rs` re-export。公开 DTO 结构为：

```rust
pub struct BackendTask {
    pub id: String,
    pub task_type: TaskType,
    pub project_id: Option<String>,
    pub title: String,
    pub status: TaskStatus,
    pub progress: Option<TaskProgress>,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub cancellable: bool,
    pub log_path: Option<String>,
    pub result: Option<TaskResult>,
    pub error: Option<BackendError>,
}
```

任务状态：

- `queued`
- `running`
- `waiting_for_confirmation`
- `cancelling`
- `cancelled`
- `succeeded`
- `failed`

任务要求：

- 每个任务有稳定 id。
- 项目 app state 可写时，任务状态和日志持久化到布局定义的 task state root（原生映射为 `.app/tasks/`）；restricted/read-only 允许的只读盘点与 Local Quick Check 使用相同 typed envelope，但仅在运行内存中存在并标记为 non-persistent。
- 每个项目同一时间只有一个活动 `ImportSession`，其中可以有多个独立 `ImportItem`。
- Import 后端细状态持久化，前端映射为发现、处理、需要操作、可确认、导入中、已导入、失败 / 取消七类。
- 登录、OCR、ASR 和能力缺失是可恢复的等待状态，不是普通失败。
- 每个任务可以记录日志。
- 可取消任务必须接入取消信号。
- 任务状态变化通过事件推送。
- 关闭主窗口后任务继续运行。
- 任务完成、失败或等待确认时触发系统通知。
- 页面切换和最小化不停止任务；应用重启后耗时下载、OCR、ASR 进入暂停状态，由用户继续。
- 已完成分片可复用；主动取消清理临时数据，之后重试从头开始。
- `ProjectDeepScan` 始终只读、可取消，并持续发布已发现 Markdown 数、能力判断和 warning 的 partial snapshot；取消或失败后，已经可读的内容仍保持可浏览，不自动关闭项目。

### 10.1 Workflows 目标任务合同

上面的 `BackendTask` 是当前公开 DTO。Workflows 实现前必须以 typed DTO 扩展而不是自由 JSON 补齐以下语义：

- 工作流任务的运行时 `project_id` 必填，并带后端派生的 `canonical_identity_key`、`identity_revision`、稳定 `workflow_kind`、输入范围、项目基线和输入指纹。
- 用户可见状态统一为 `queued`、`running`、`waiting_for_confirmation`、`succeeded`、`failed`、`cancelled`、`interrupted`；`cancelling` 可以保留为内部过渡状态。
- 任务进度包含工作流阶段 id、阶段顺序、当前处理项、已完成数、总数和结构化活动记录。原始 stdout/stderr 继续写日志，但不能承担主状态合同。
- 每个项目维护独立串行工作流队列。不同项目可以并行执行，任何列表、确认、取消和历史 command 都必须验证项目归属；前端切换项目后才可操作该项目任务。
- Workflows overview 行必须携带渲染该行动作所需的有界目标事实：活动任务是否要求显式续队，以及最近完成任务的稳定 task id。前端不得通过偶然命中最近五条运行历史来推断“继续队列”或“查看已完成任务”。
- `canonical_identity_key + identity_revision + workflow_kind + scope + execution options + route + baseline` 生成持久稳定输入指纹；`project_id` 只作为当前进程句柄，不参与跨重开 dedupe。相同指纹的非终态或可复用任务应返回既有任务，不重复入队。
- 重试必须创建新任务并通过 `attempt_of` 指向原任务；不得覆盖原任务、日志或错误。
- 当布局提供可写 task state root 时，等待确认和排队任务持久化；排队任务重开后需要显式 continuation。进程异常退出时，原先持久化的运行中工作流映射为 `interrupted`，并记录已完成阶段和可复用产物；不得伪造进程级续跑。restricted/read-only 的 ephemeral 本地任务不承诺跨重启恢复。
- Update Wiki、Health Check 与 Generate Content 的阶段顺序由工作流设计规范 §11 定义；后端发出阶段事件，React 只负责呈现。

### 10.2 Workflows 编排边界

- 工作流准备请求只传结构化类型、范围、选项和显式执行路径覆盖，不接受任意 prompt 或任意 shell 参数。
- 启动时后端重新解析 `ProjectContext` access policy：外部 AI / Agent / Skill 要求 trusted，任何项目写入还要求 writable，checkpoint-required mutation 还要求可用 Git checkpoint。无项目、restricted 或 read-only 状态不得创建一个随后必然失败的写入工作流任务。
- 路径默认值由 `SettingsService` 解析；不可用时返回结构化 prerequisite，不自动换用另一 Agent、Provider 或模型。
- Update Wiki 的低风险、无冲突写入在所需 Git 检查点成功后自动应用；Generate Content 新建制品不要求检查点，覆盖既有制品则必须先建检查点并进入确认。Agent repair 由初次 selected-batch 批批准覆盖安全 selected-path 更新/新建以及批量/广泛重写本身；只有 delete、unexpected existing-path overwrite 和 baseline/user-edit conflict 创建二次持久 `PendingAction`。其他产品操作继续按各自合同对删除、覆盖、广泛重写和冲突创建确认。
- 确认是异步任务状态。用户可以离开页面继续工作；确认 command 必须再次校验项目、基线、计划和检查点。
- 取消后不得把未确认的部分结果提升到正式 Wiki 或 Exports 路径。
- 系统通知只在 `waiting_for_confirmation`、`succeeded` 和 `failed` 时触发。

当前实现状态（2026-08-13）：H3–H5 复用本节的 ProjectContext、Workflow queue、TaskService、confirmation、Git checkpoint、candidate manifest 与 typed result 边界完成 Agent Health/repair bridge；H6 未改变 backend runtime。由于最终 full gate 与完整验证矩阵未全绿，Gate H / Batch 7 继续 fail closed。

## 11. ProjectService

职责：

- 事务式创建原生项目，生成创建时模板的 `purpose.md` 和 `schema.md`。
- 对用户选择目录执行零写入 quick assessment。
- 通过应用级 operation registry 发布 quick assessment 进度 / 结果并处理取消；不得为尚未打开的目录创建项目 Task，取消后不得保留可执行 assessment。
- format 分类当前 / legacy 原生、`nashsu`、Obsidian、Markdown vault、歧义 Markdown、普通资料与 unknown；独立 health 返回 healthy、repairable、recovery 或 unreadable。
- 组合 format、health、trust、filesystem access、layout、Git、capabilities、warnings、confidence 和建议动作。
- 把歧义 Markdown 意图与普通资料“新建并导入”路由交给 typed response；不在原目录初始化或移动。
- 按 canonical identity 查询 / 更新全局 trust，并把 trust、filesystem access、health、layout 与 capabilities 组合成 access policy；`trusted + read_only` 必须是一等组合，`restricted` 只是未信任能力集合的 UI 摘要。
- 规划兼容 `.app/compat` 启用与应用级 repair；通过后端保存的 plan id 重校验并执行确认写入。
- 编排可取消的只读深度扫描和 partial snapshot。
- 管理最近项目。
- 扫描项目摘要。

主要输入：

- 项目路径。
- 项目名称。
- 创建时项目模板与父级保存位置。
- assessment id、用户选择的打开动作或 repair plan id。

主要输出：

- `ProjectContext`
- `ProjectSummary`
- `ProjectHealthReport`
- `ProjectOpenAssessment`
- `ProjectRepairPlan`
- `PendingAction`

硬边界：

- 项目模板只影响创建时原生根目录的 `purpose.md` 和 `schema.md`；创建后不提供切换。
- 新建原生知识库的核心目录结构必须稳定；兼容知识库保留 `ProjectLayout` 返回的既有布局。
- quick assessment 不得写盘、初始化 Git、创建项目任务、执行目录内容或发送外部 AI。
- 普通资料目录不得原地初始化、移动、重命名或写 marker；创建新项目后由 Import 复制 / 归档用户确认的资料。
- 兼容库的应用自有指导文件只写 `.app/compat/`；根目录同名文件始终视为用户内容。
- trust、assessment 与 repair plan 是不同状态；任何写入确认都必须重新校验目录身份和输入 hash。
- 初始化 Git 由 `GitService` 执行，`ProjectService` 只编排调用。

## 12. FileStore

职责：

- 读取 Markdown。
- 保存 Markdown。
- 读取和写入 JSON。
- 枚举文件。
- 创建目录。
- 安全复制、移动、重命名。
- 计算文件哈希。
- 路径规范化。

主要输入：

- `ProjectContext`
- 项目相对路径。
- 文件内容。

主要输出：

- 文件内容。
- 文件元数据。
- 安全路径。
- 冲突信息。

硬边界：

- 不接受未经校验的任意绝对路径执行项目写入。
- 不静默覆盖。
- 不吞掉 CJK 文件名错误。
- 所有 JSON 写入应尽量原子化，避免写一半损坏状态文件。

## 13. ImportService

仓库仍包含 legacy `services/import_service/` facade，同时已经存在 `services/import_v2/` 链路。legacy 用例分布为：

- `artifacts.rs`：产物路径校验、hash 校验和失败清理 helper。
- `classification.rs`：文件类型分类、归档目录和确定性重命名；只有 `classify_file` 经 facade 与 `services/mod.rs` 公开 re-export。
- `preview.rs`：来源收集、hash 和文件 / 文本 / URL 导入预览。
- `confirmation.rs`：确认预览与归档执行。
- `promotion.rs`：将可浏览副本提升到 Wiki source 页面并重映射提取路径。
- `source_actions.rs`：原始来源删除 / 替换的确认请求。
- `source_catalog.rs`：已导入来源目录读取。
- `test_support.rs`：仅在 `cfg(test)` 下编译的共享测试 helper。

`confirmation.rs`、`preview.rs`、`promotion.rs`、`source_actions.rs`、`source_catalog.rs` 分别提供聚焦的 `impl super::ImportService` block；内部 helper 使用 `pub(super)` 或更窄可见性。

这些模块可用于理解当前代码和迁移边界，但目标职责由 Import V2 统一：

- `ImportSessionService`：持久化一个项目级活动会话、任务、尝试、取消与恢复。
- `Discovery / Classification`：处理文件、文件夹、URL、平台、集合和剪贴板。
- `CapabilityResolver`：把媒体、OCR、ASR、语言包和平台能力解析成用户目标级需求。
- `LoginSessionService`：管理隔离平台会话；不向 React 暴露 Cookie。
- `ExtractionService`：确定性提取文档、网页、媒体、字幕、OCR / ASR 结果和资源。
- `QualityGate`：验证正文覆盖、结构、资源和不确定区间。
- `SourceCandidateService`：生成最终 Markdown 预览与更新 Diff。
- `SourceCommitService`：以 `sourceId` 为原子边界提交 layout-defined evidence、app-state 与 Source write roots。
- `SourceVersionService`：管理别名、版本、人工编辑基线、时间线和恢复。
- `CompileService`：消费显式 `sourceId + versionId` change set，不属于导入提交事务。

Import V2 的批量控制面合同：

- `start_import_batch_v2` 为一个 item cohort 创建并返回一个持久化 operation `BackendTask`；operation marker 是 `import-v2-operation:<session_id>`，共享一个 cancellation token，聚合进度 / 日志与 `import://session-patch` 最多每 100ms flush 一次，terminal 强制 flush。
- `ImportItem.task_id` 是 operation claim，不表示每个 item 拥有独立 `TaskService` 任务。逐项 JSON / session summary 是 partial success、waiting、preview、failed、skipped、cancelled 和 retry 的事实来源。
- `start_import_items_v2` 保持原 `Vec<BackendTask>` wire contract，仅服务 `<= 200` 的兼容调用；更大 cohort 返回 `IMPORT_BATCH_COMMAND_REQUIRED`。
- `accept_import_scan_v2` / `discard_import_scan_v2` 只消费 layout-defined import-state root 中的保存扫描，并把当前 layout 解析、trusted + writable 检查、scan 重验和全部 app-state 写入放在同一个 authority transition 临界区。无 Tauri 依赖的 `scan_confirmation` use-case 负责 totals、project/root/session/task identity、token、全部来源 fingerprint、总量 / 单表两阶段授权、幂等 accepted/discarded 状态和“确认前无 session inputs”；command 只负责 typed DTO、`AppState`、持久化与 task glue。
- discovery hard file limit 在首个越界项处返回 typed error，不持久化可接受的部分 scan。XLSX 输出量由受限 OOXML worksheet 条目保守估算；旧 XLS 无可靠内建 Sheet 计数时把总输出估算标为未知并强制确认，而不是按单输出放行。
- React 通过 `taskStore.upsertTasks` 和 `importStore.patchItems` 对每个 flush 各做一次主要 store commit；command/event 响应必须通过 project key、session ID、identity / authority revision 与 epoch guards。

目标存储规则（括号内路径均为新建原生知识库映射，兼容知识库使用 `ProjectLayout`）：

- evidence root（`raw/`）保存不可变原文件、网页 / 平台证据、原图、字幕、OCR / ASR 原始输出和版本证据。
- Source write root（`wiki/sources/`）保存忠实、规范化、可阅读、可编辑的当前 Source。
- app-state root（`.app/`）保存 `sourceId`、`versionId`、别名、hash、质量、基线、任务和编译消费记录。
- 新建原生知识库的 `wiki/sources/` 物理路径按稳定来源渠道组织，例如 `local/`、`web/<host>/`；媒体类型写入元数据，不作为唯一目录分区。
- Excel 等可使用一个逻辑来源、多个可读 Markdown 的来源包。

硬边界：

- 导入到当前项目使用复制，不持续跟踪原始路径。
- 文件夹输入只表示把其中资料批量导入当前项目；打开知识库属于 ProjectService。普通资料目录不原地初始化或移动。
- 每个成功导入项都必须在 layout-defined Source write root 生成 Source；失败项不得生成占位 Markdown。
- URL、视频和图文不得因 `input.kind == Url` 跳过 Source 写入。
- 完全重复项只追加别名，不创建新 Source。
- 更新来源时保护人工编辑，并通过 Diff 或三方合并确认。
- 纯新增不需要高风险 Git 检查点；覆盖、合并、替换和删除必须先创建。
- 批次允许部分成功；单项失败不得回滚其他已成功 `sourceId`。
- 导入提交不得自动调用 CompileService。

## 14. ExtractionService

职责：

- 原生提取文档、表格、网页、平台正文、字幕和媒体元数据。
- 发现并关联本地音视频伴随字幕或稿件。
- 根据正文缺口生成 OCR / ASR 用户动作，不自动取得授权。
- 调用应用管理且校验过的媒体、OCR、ASR 和语言能力包。
- 生成 staging 产物、资源清单、质量报告和 SourceCandidate。
- 保留原始证据，但不直接覆盖当前 Source。

解析器策略：

- PDF 文本层优先，仅对缺失或低质量页面请求 OCR。
- Word / PPT 原生结构优先，只有主体截图进入 OCR 候选。
- Excel / CSV 保留完整单元格结构，按工作表或连续行拆成来源包，不静默截断。
- 普通网页先轻量提取，必要时自动升级隔离浏览器。
- 视频 / 音频优先使用原语言可靠字幕；没有可靠字幕时只提供本地 ASR。
- 视频 ASR 无有效语音时，可在用户启用后对关键帧做画面 OCR。
- 图片视觉理解、自动描述和图表解读不在首版范围。
- BYOK 不参与导入解析；本地 Agent 只能在用户主动触发后操作隔离 staging。

建议接口：

```rust
pub trait Extractor {
    fn supports(&self, input: &SourceFile) -> bool;
    async fn extract(&self, input: ExtractRequest) -> Result<ExtractResult, BackendError>;
}
```

输出应包含：

- 候选 Markdown 与资源引用。
- 原始证据和 staging 产物描述。
- 来源身份、规范 URL、别名和版本输入。
- 页数、字数、时长、语言或可用统计。
- 质量门槛、警告和可定位问题区间。
- 后续所需 `CapabilityRequirement` 或登录动作。
- 用户可读状态、技术错误和重试信息。

### 14.1 Source 重处理与 AI 整理

- 重新 OCR / ASR、换字幕和平台刷新都以当前 `sourceId + versionId + Markdown hash` 为输入，生成新的 `SourceCandidate`。
- Source 在处理期间变化时，必须重新 Diff 或三方合并，不得直接覆盖。
- AI 整理由一份内置 `source-rewrite` Skill 合同驱动，Agent 与 BYOK 文本路线必须复用同一合同；输入只包含当前 Source、元数据、已有 OCR / ASR / 字幕和图片引用。
- `source-rewrite` 可按用户自定义要求重写或纠错事实、数字、人名、URL、引语和时间，但只能依据有界输入，不得引入外部事实或抬高原文确定性。
- 后端只硬校验有界 JSON / 大小、Markdown 与 app-owned frontmatter、唯一 `## 内容概览`、Source/version/hash 绑定；不得使用事实 token、姓名词表或数字集合等启发式语义 guard 拒绝候选。
- 所有重处理和 AI 整理结果在确认前只写 staging；确认后创建有意义的版本时间线事件。
- 普通可恢复失败保留已绑定的 Source 基线和执行设置，等待用户显式重试；同一次运行不得静默 fallback 到另一 Agent / BYOK 路线、第二个模型或双调用。

## 15. GitService

职责：

- 初始化 Git 仓库。
- 创建初始提交。
- 创建检查点。
- 提交最终结果。
- 检测工作区状态。
- 生成 diff。
- 检测外部修改。
- 支持恢复或回滚。

必须创建检查点的操作：

- 删除。
- 覆盖。
- 批量替换。
- Agent 自动修复。
- 重大重新编译。
- 原始资料替换或删除。
- 冲突合并前。

库选择原则：

- 可以使用 Rust Git 库，也可以调用系统 `git`。
- 选择时优先考虑跨平台稳定性、错误可解释性、中文路径兼容性和打包复杂度。
- 不在本文强行指定 `git2` 或系统 `git`。

硬边界：

- Git 检查点失败时，高风险操作不能继续。
- 新建原生知识库自动初始化 Git；quick assessment 和打开外部知识库绝不初始化、`git add`、提交或 stash。
- 兼容知识库只在启用完整功能的确认页提供“初始化本地 Git”，默认勾选但可拒绝；拒绝后禁用所有要求 checkpoint 的写入能力。
- 已有 dirty worktree 不自动处理。用户可以先自行处理，或明确授权把当前全部变更作为检查点；授权范围必须在确认页可见。
- 普通用户不需要理解 Git，但开发实现不能绕开 Git 安全边界。

## 16. AgentService

职责：

- 检测 Agent CLI。
- 获取版本号。
- 维护默认 Agent。
- 启动 Agent 任务。
- 捕获 stdout / stderr。
- 推送实时输出事件。
- 支持取消。
- 记录任务日志。

支持目标：

- `claude`
- `codex`
- `openclaw`
- `hermes`
- 后续其他 CLI

执行规则：

- 默认执行路径由设置决定；工作流、Source AI 整理和 Chat 可以显式覆盖单次路径，但不得静默改变全局默认值。
- 用户可以在 Source 已经形成后的 AI 整理、更新 Wiki、生成内容和 Chat 中手动选择 Agent CLI 或 BYOK API。
- BYOK 不参与导入解析或失败恢复。
- 应用不能静默安装 Agent。
- 安装命令必须用户确认。
- 所选执行路径不可用时返回 prerequisite，不得自动回退到另一 Agent、Provider 或模型。
- Agent 输出修改文件前后必须受 GitService 和 PendingAction 保护。

`import-recovery` 规则：

- 仅本地 Agent，并且必须由用户主动触发。
- 可在当前任务授权范围内使用浏览器、媒体、OCR / ASR 和临时脚本。
- 只写隔离 staging 候选，不得直接修改布局定义的 evidence、Source/Wiki roots 或 Git。
- 不安装软件、不执行未知下载二进制、不接触原始 Cookie / API Key、不绕过访问控制。

`source-rewrite` 规则：

- Agent 路线支持 Claude Code、Codex、OpenClaw 和 Hermes；四者与 BYOK 路线消费同一份内置 `source-rewrite` 合同，并复用所选 CLI 可用于执行的本地登录态。
- 每次 Source Agent 都在临时候选 workspace 中运行，应用不会把项目目录作为可写工作区，也不会让候选绕过 Diff、显式确认和 Git checkpoint 直接落盘。Claude Code 与 Codex 使用无会话、跳过项目规则/扩展的隔离执行配置，其中 Codex 保留认证目录但不加载用户 `config.toml`；OpenClaw 使用 `agent exec` 临时状态的一次性执行；Hermes 使用 `-z` 一次性执行并跳过项目规则。

`wiki-lint` Agent 合同（2026-08-12 产品决定）：

- 复用现有 `AgentService` runtime、structured transport、超时、取消、进程树终止和环境清理；不建设新的 Agent runtime、credential broker、逐工具审批或专门 no-tools/no-network 子系统。
- 只有通过精确 invocation/output contract 的 Claude Code 与 Codex 可进入首期 lint capability；OpenClaw/Hermes 在同等测试落地前保持 unsupported。所选 route 不可用时 fail closed，不切换 BYOK。
- Agent 只把 task-owned candidate workspace 作为可写 cwd；真实项目根不作为可写 workspace。candidate 中的内置 Skill、typed request、purpose/schema context 与 Source snapshot 都受 hash 保护。
- H0–H2 只落地合同和不可达 bridge；在后续 Health route/repair runner 批次完成前，`supports_lint_agent` 仍为 false，外部 command 和 Health `availableRoutes` 不可到达该 transport。
- 环境清理后只恢复所选 Agent 必需的认证/配置路径：OpenClaw 保留活动 state/config/profile、auth secret dir 与 include roots；Hermes 优先使用显式 `HERMES_HOME`，否则解析 sticky `active_profile`（Windows 默认根为 `%LOCALAPPDATA%\hermes`），并保留 OAuth/model 路径覆盖。
- 为复用本机登录与默认模型，Agent 进程仍可使用所选 CLI 的本地配置；该宽松边界不等价于操作系统级只读沙箱。应用只主动提供当前 Source 的有界输入，不承诺所选第三方 CLI 的工具无法读取其他本地路径。
- 普通可恢复失败必须由用户按保存的 Source 基线、路线和设置显式重试。BYOK 重试锁定 provider 与 model；Agent 重试锁定 Agent 种类，使用重试时该 CLI 当前的 Source 执行 profile（其本地 profile、认证或默认模型若已变化，不伪装成原设置）。运行时禁止静默 fallback、自动第二模型或双调用。

## 17. LlmService

职责：

- 管理 BYOK Provider 配置。
- 组装请求。
- 执行 LLM API 调用。
- 支持流式或非流式响应。
- 处理错误。
- 返回引用和生成内容。

Provider：

- OpenAI
- Anthropic
- Google
- Ollama
- Custom

建议基础 crate：

- HTTP 客户端可使用 `reqwest`。
- JSON 序列化使用 `serde` / `serde_json`。
- 异步运行使用 `tokio`。

硬边界：

- API Key 必须从 `SecretService` 获取。
- 不把密钥写入任何项目 app-state JSON（新建原生设置映射为 `.app/settings.json`）。
- 普通搜索不能自动调用 LLM。
- BYOK 只处理已经存在的 Source、Wiki 和 Chat 文本，不得成为 Import extractor 或 recovery route。

## 18. SearchService 与 ChatService

### 18.1 SearchService

`SearchService` 是 `services/search_service/mod.rs` 中的稳定 facade，当前子模块为：

- `catalog.rs`：按 `ProjectContext.layout` 扫描 Source/Wiki Markdown roots，构建目录树和页面元数据目录。
- `pages.rs`：Source/Wiki 页面读取，以及在 access policy 允许时创建、保存、重命名和删除 Wiki 页面。
- `query.rs`：本地关键词、标签、类型和来源过滤查询。
- `excerpts.rs`：受限检索片段和正文摘要。
- `test_support.rs`：仅在 `cfg(test)` 下编译的目录、索引和 CJK fixture helper。

这些文件通过多个 `impl SearchService` block 实现同一个 facade。`SearchService` 组合 `FileStore` 和独立 `WikiIndex`；`WikiIndex` 负责项目级只读内存索引及外部 Markdown 变更失效，不成为 Search facade 的私有子模块。

职责：

- 构建本地搜索索引。
- 标题搜索。
- 全文关键词搜索。
- 标签过滤。
- 类型过滤。
- 来源过滤。
- 为 Chat 召回相关 Source 或 Wiki 页面，并保留内容类型与可导航标识。

存储策略：

- 可以在内存中建立索引。
- trusted writable 项目可以将轻量缓存写入 `.app/`；restricted/read-only 项目只使用有界内存索引，不要引入数据库。
- Markdown 文件仍是事实来源。

边界：

- SearchService 不直接调用 LLM。
- 语义问答由 Chat 流程调用 LlmService 或 AgentService。
- `WikiIndex` 保持独立，不能与 `SearchService` 的 query / page 用例物理合并后向 commands 暴露。

### 18.2 ChatService

`ChatService` 是 `services/chat_service/mod.rs` 中的稳定 facade，当前子模块为：

- `sessions.rs`：会话创建、列表、加载、重命名、删除和消息持久化。
- `retrieval.rs`：本地检索上下文、预算和诊断组装。
- `citations.rs`：模型引用解析和来源校验。
- `saved_answers.rs`：将回答保存为 Markdown Wiki 页面。
- `test_support.rs`：仅在 `cfg(test)` 下编译的会话与检索 fixture helper。

这些文件通过多个 `impl ChatService` block 实现同一个 facade；`RetrievalContext` 等选定类型从 `services/mod.rs` re-export 以维持 command / contract test 调用面。

边界：

- `ChatService` 负责会话、检索、引用和保存答案，不负责 Chat 便捷写入的授权、意图分类或 Git 变更审计。
- `ChatConvenienceService` 保持独立 service，由 `AppState` 单独持有，不并入 `ChatService`。
- Chat 的模型 / Agent 路由通过现有 command、`LlmService`、`AgentService` 和 `TaskService` 编排；物理拆分不得改变 Chat DTO 或 `ProjectLayout.chatStateRoot` records 的持久化兼容性（原生映射为 `.app/chats/*.json`）。
- Chat retrieval 同时消费 Source/Wiki 结果，Source-only 项目不要求先编译。citations 必须带 content kind，后端只接受仍位于当前 layout roots 内的引用。
- 外部 AI/Agent/Skill 调用前重验 canonical identity 与 trust；缺少 Git 不阻止纯问答。保存回答才要求 writable，并按 overwrite/hash/Git 策略执行。
- 配置或信任 prerequisite 返回可恢复目的地；再次进入 Chat 时保留前端草稿，但后端不得自动重放原请求。
- `ProjectLayout.chatStateRoot` 可写时使用该状态根目录（原生映射为 `.app/chats/*.json`）；文件系统只读或该路径缺失时只提供显式标记为 ephemeral 的内存会话，不尝试旁路写入用户目录。

## 19. GraphService

职责：

- 扫描 `ProjectContext.layout` 允许的 Source/Wiki Markdown roots。
- 解析 frontmatter。
- 解析 `[[wikilinks]]`。
- 推断页面类型。
- 构建节点和边。
- 生成图谱数据。
- 仅在 trusted writable 且 `ProjectLayout.graphCachePath` 可用时写入缓存（原生映射为 `.app/graph-cache.json`）；restricted/read-only 返回有界内存结果。
- 深度扫描未完成时返回 partial 标记、覆盖计数和 task id。

前后端分工：

- 后端负责扫描、解析、访问策略、可选缓存和提供图数据。
- 前端使用 sigma.js / graphology 渲染和交互。
- ForceAtlas2 和 Louvain 可以在前端或后端执行，具体取决于性能和库选择。

首版规则：

- 每个可读 Source/Wiki Markdown 文档是页面级节点；不要求先编译 Wiki。
- 边来自 wikilinks 和多信号关联度。
- 边统一表示“相关”。
- 不实现复杂关系类型和证据系统。

## 20. LintService

`LintService` 是 `services/lint_service/mod.rs` 中的稳定 facade，当前子模块为：

- `rules.rs`：确定性本地规则与规则 helper。
- `ignores.rs`：ignore 读取、写入和匹配。
- `reports.rs`：Lint 报告、历史和持久化读取。
- `deep.rs`：深度 Lint 编排与结果解析。
- `fixes.rs`：single / batch fix、确认和 Git checkpoint 编排。
- `test_support.rs`：仅在 `cfg(test)` 下编译的临时项目与 fixture helper。

这些文件通过多个 `impl LintService` block 实现同一个 facade。`LINT_REPORTS_DIR` 只保持计划级 `pub(crate)`，兄弟模块共享 helper 使用 `pub(super)`，其余实现保持私有；commands 和 `AppState` 只依赖 `LintService`。

职责：

- 执行本地快速 Lint。
- 编排 Agent 深度 Lint。
- 生成问题列表。
- 生成修复计划。
- 执行可自动修复项。
- 对高风险修复返回 `PendingAction`。

本地快速 Lint：

- 死链。
- 孤立页面。
- 缺失 frontmatter。
- 布局声明了 Wiki 索引入口时，该入口与实际 Wiki 页面不一致；没有 Wiki 根目录时该规则不适用。
- 空页面。
- 重复文件名。
- 路径大小写问题。
- 缺失资源文件。

Agent 深度 Lint：

- 重复主题。
- 弱交叉引用。
- 来源缺失。
- schema 不一致。
- 内容过期。
- 跨页面矛盾。

内置 Skill 与 Agent repair 合同：

- 应用编译期内置、版本固定的 `wiki-lint` 是唯一 authority；项目同名 Skill 不读取。purpose/schema/layout context 只是不可信输入，不能覆盖 Skill ref、operation、write allowlist、round limit 或 schema。
- Lint 拥有 Finding selection、batch preparation 和结果关联；用户一次批准选中批次。WorkflowService 只在批准后承载隐藏的 repair operation，复用同一项目队列、TaskService、history/cancel/recovery，不新增 WorkflowKind/Overview 行。
- 批准后的 queued dispatch 必须在第一次 Agent repair invocation 前调用 GitService 创建 clean-HEAD checkpoint；失败时 invocation 与 candidate/project mutation 均为 0。Agent 只修改 bounded candidate 中的授权 Wiki Markdown；`raw/**`、忠实 Source、`wiki/sources/**`、layout-defined Source roots、`.app/**`、Skill/request/context 均不可由 Agent 修改。
- CompileService/Update Wiki 已有 candidate manifest、baseline、checked apply、journal 与 two/three-way lazy Diff 是唯一 apply/review 实现。安全 selected-path 更新/新建由初次批准覆盖；delete、unexpected overwrite、baseline conflict 进入二次 PendingAction；越界候选直接失败。
- 每轮 apply 后 LintService 运行 deterministic recheck 并以稳定 Finding identity 关联结果，最多三轮；未解决保留 verified Diff、commit/checkpoint 与 rollback facts，返回 typed partial/manual-review 终态。Agent 输出不能自行宣布最终 resolved，repair 不回退 BYOK。

硬边界：

- 本地只读 Lint 可在 restricted 模式对有限深度 Markdown 运行，不写报告或缓存。
- Agent 深度 Lint 要求项目已信任。
- 任何修复要求 trusted writable + clean Git；一次批批准成功后、queued dispatch 在第一次 Agent repair invocation 前创建 Git 检查点。
- 删除、未授权既有路径覆盖、冲突修复必须二次确认；raw/Source 越界不可确认。
- Agent 深度 Lint 与 repair 只由应用内置固定 `wiki-lint` Skill 驱动。

## 21. ExportService

职责：

- 调用 `skills/html-*`。
- 读取 HTML 模板。
- 生成单篇美化阅读页。
- 生成知识卡片。
- 生成项目级 HTML 报告。
- 输出到 `ProjectContext.layout` 解析的导出根；原生项目默认是 `exports/html/`。
- 提供预览路径。

硬边界：

- HTML 模板只影响输出样式。
- HTML 模板不能改变 Wiki schema。
- HTML 模板不能改变 Lint 规则。
- HTML 生成不要硬编码为单一不可扩展流程。
- 外部 Agent/Skill/Provider 生成需要项目已信任；写入布局定义的导出根需要 writable，覆盖既有制品还需要 checkpoint 与确认。

## 22. SettingsService

职责：

- 读取全局设置。
- 读取项目设置。
- 保存设置。
- 管理固定启动规则所需的最近项目与失效记录。
- 管理最近创建父目录、歧义 Markdown 意图和 canonical 目录信任。
- 管理语言和主题。
- 管理默认执行路径与默认 Agent 绑定。
- 管理 LLM Provider 非密钥配置。
- 管理后台任务关闭行为。
- 管理更新检查配置。

边界：

- 项目级设置写入可写的 `ProjectLayout.settingsPath`（原生映射为 `.app/settings.json`）。
- 最近项目、最近创建父目录、歧义意图和 trust 写入应用配置目录，并可在无项目上下文读取；trust 不写入项目 marker。
- 密钥只由 `SecretService` 管理。

## 23. SecretService

职责：

- 保存 API Key。
- 读取 API Key。
- 删除 API Key。
- 检查 provider 是否已配置密钥。
- 管理平台 Cookie、token 和隔离登录 profile 引用。
- 返回平台、账号摘要、有效状态和最近验证时间。

平台目标：

- Windows Credential Manager。
- macOS Keychain。
- Linux Secret Service 或平台可用凭据管理方案。

库选择原则：

- 不在本文强行指定具体密钥库 crate。
- 选择时优先考虑 Tauri v2 兼容性、跨平台行为、打包复杂度和错误可解释性。

硬边界：

- 密钥不写入项目文件。
- 密钥不写入日志。
- UI 不默认回显完整密钥。
- React、项目文件、日志和导出不得接收原始 Cookie、token 或 profile 内容。

## 24. 数据模型目录

`models/` 分三类：

- Command DTO：前端和后端 IPC 使用。
- Domain model：后端服务内部使用。
- Persistence model：写入 `.app/*.json` 或项目文件的稳定结构。

规则：

- DTO 可以为了前端易用而扁平。
- Domain model 可以更严格、更类型化。
- Persistence model 必须考虑向后兼容。
- 不要把内部临时字段随意写进持久化 JSON。

常见模型：

- `ProjectSummary`
- `ProjectHealthReport`
- `ProjectFormat`
- `ProjectTrustState`
- `ProjectFilesystemAccess`
- `ProjectOpenAssessment`
- `ProjectGitAssessment`
- `ProjectCapabilities`
- `ProjectTrustIdentity`
- `ProjectRepairPlan`
- `WikiPageMeta`
- `ImportSession`
- `ImportItem`
- `ImportAttempt`
- `CapabilityRequirement`
- `LoginSessionRef`
- `SourceCandidate`
- `SourceRecord`
- `SourceVersion`
- `SourceAlias`
- `QualityReport`
- `CompileChangeSet`
- `GitCheckpoint`
- `AgentInfo`
- `AgentTaskRequest`
- `LlmProviderConfig`
- `SearchResult`
- `GraphNode`
- `GraphEdge`
- `LintIssue`
- `ExportJob`
- `PendingAction`
- `BackendTask`

## 25. 事件模型

事件应有统一 envelope：

```rust
pub struct BackendEvent<T> {
    pub event_id: String,
    pub event_type: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub timestamp: String,
    pub payload: T,
}
```

事件类型：

- `task.updated`
- `task.log`
- `task.completed`
- `task.failed`
- `task.cancelled`
- `confirmation.requested`
- `project.refreshed`
- `wiki.changed`
- `graph.updated`
- `agent.output`

前端不能只依赖一次性 command 返回来追踪长任务。长任务状态以事件和 task 查询为准。

## 26. 安全边界

### 26.1 路径安全

- 所有项目路径经过 `ProjectContext`。
- 拒绝越界路径。
- 拒绝未确认的覆盖和删除。
- 项目根符号链接 / junction 先 canonicalize；根内链接做包含性与循环保护，根外链接只展示、不跟随、不索引、不写入。
- 大小写和 Unicode normalization 冲突只报告，不自动改名或改写链接。
- restricted / read-only access policy 在每个写入、Agent、Skill、外部 AI 和任务启动点重新校验。
- quick assessment 不执行项目内脚本、hook、Skill 或其他可执行内容。

### 26.2 密钥安全

- 密钥只进系统凭据管理。
- 密钥不进项目文件。
- 密钥不进日志。
- 错误信息不得包含完整密钥。

### 26.3 Agent 安全

- 不静默安装 Agent。
- 不静默执行安装命令。
- Agent 修改必须受 Git 检查点保护。
- 高风险 Agent 操作必须确认。

### 26.4 Git 安全

- 高风险操作前必须检查点。
- 检查点失败则阻止操作。
- 冲突不能静默覆盖。
- rollback 删除与 untracked diff 读取必须通过 `BoundProjectMutationRoot` 的 retained-directory handle；不得在 canonicalize 后再用 ambient pathname 递归删除或跟随 worktree symlink 读取内容。

### 26.5 Batch 6 authority、凭据与更新边界

- `ProjectRegistry` 只解析当前 canonical identity/access；外部 AI、Agent、Skill 与 Workflow 必须携带后端签发的 authority revision/permit，并在执行点重新验证 trust、writable access、Git policy 与 revoke epoch。
- provider credential binding 绑定 project identity、provider kind、canonical origin、config id 与 revision。origin 轮换先持久化新 binding，再退休旧凭据；持久化失败不得删除仍工作的旧 secret。
- `UpdateService` 作为 project-independent coordinator 绑定 offer generation、TTL 和 candidate identity；`DesktopUpdateRuntime` 只保留同一进程内由 Tauri updater 校验过的 handle/bytes。IPC 不接受 endpoint、artifact URL、signature、channel 或 public key。
- install/restart 的前端确认临界区会重新采集 editor/import presentation facts，并在 handoff 期间锁定编辑器；后端 `UpdateInstallBarrier` 原子复查其拥有的 confirmation、critical task 与 Workflow apply facts。两侧任一 blocker 出现都 fail closed。
- Windows vendored updater 除 manifest transport size limit 外，必须验证 `ShellExecuteW > 32` 才能退出旧进程；launch 失败保留当前版本并返回结构化错误。
- stable 发布只允许 protected final publisher。draft reverse verification、stable publish 与 anonymous asset verification 在同一 guarded step 内执行；未完成验证的正常退出、错误或终止信号回退 draft。runner/control-plane 硬丢失仍属于 release-owner incident response，不能表述为完全原子。
- Public beta 还需要同 tag/commit 的四目标真实签名包、旧签名版本升级、uninstall 项目保留、capability continuation 与匿名 endpoint 证据；source contract 通过不能替代这些证据。

## 27. Crate 选择原则

可以预设的低争议基础 crate：

- `serde`
- `serde_json`
- `thiserror`
- `tokio`
- `reqwest`
- `uuid`
- `chrono` 或同类时间库

暂不强行指定的领域库：

- Git 库。
- PDF 解析库。
- DOCX / PPTX / XLSX 解析库。
- 系统密钥库。
- 系统托盘和通知插件。
- Markdown AST 解析库。

选择领域库时优先考虑：

- Tauri v2 兼容性。
- Windows / macOS / Linux 一致性。
- CJK 文件名支持。
- 打包体积和安装复杂度。
- 错误信息可解释性。
- 是否能在 service 层后续替换。

## 28. 测试策略

### 28.1 Service 单元测试

适用：

- 路径规范化。
- frontmatter 解析。
- wikilink 解析。
- Lint 确定性规则。
- JSON 模型读写。
- 错误转换。

### 28.2 临时目录集成测试

适用：

- 项目创建。
- quick assessment 零写入、零 Git、零项目任务；operation id 可查询 / 取消，取消后没有可执行 assessment snapshot，重复取消幂等。
- format 覆盖当前 / legacy 原生、`nashsu`、Obsidian、Markdown、歧义、普通资料与 unknown；独立 health 覆盖 healthy、repairable、recovery 与 unreadable。
- 普通资料目录只返回新建并导入，原目录字节与结构不变。
- trust × filesystem access × health 组合 access policy，至少覆盖 trusted writable、trusted read-only、untrusted writable、untrusted read-only 与 recovery。
- `.app/compat` 写入且根级同名用户文件不覆盖。
- trust 在目录移动、替换或身份变化后失效。
- repair plan 确认时身份 / hash 重校验。
- dirty Git 与拒绝初始化 Git 的能力降级。
- 深度扫描取消及 partial snapshot。
- 文件导入。
- Git 检查点。
- 文件覆盖冲突。
- `.app/*.json` 写入。
- CJK 文件名。
- Windows 风格路径和大小写问题。
- 根符号链接、内部循环链接、外部链接和 Unicode normalization 冲突。

### 28.3 Command 契约测试

Command 层测试重点：

- 请求 DTO 校验。
- 错误格式。
- PendingAction 返回。
- task id 返回。

Command 层不需要重复 service 的全部业务测试。

### 28.4 Agent / LLM 测试

Agent 和 LLM 应提供可替换 adapter：

- 测试中使用 fake Agent。
- 测试中使用 fake LLM provider。
- 不依赖真实 API Key。
- 不依赖真实 CLI 安装。

### 28.5 必须覆盖的边界用例

- CJK 文件名。
- Unicode 路径。
- 路径越界。
- 同名不同内容文件。
- 完全重复文件。
- Git 检查点失败。
- Agent 任务取消。
- LLM provider 配置缺失。
- 密钥不存在。
- 导入解析部分失败。
- 图谱缓存损坏。
- quick assessment、信任、repair plan 和 deep-scan DTO 的向后兼容与防重放。

## 29. 当前后端演进规则

1. 新增 Tauri 能力时先扩展稳定 model / DTO，再实现 service facade 用例，最后在薄 command 与 `lib.rs` 注册表接线。
2. 单文件 facade 变大时，可以按聚焦用例拆分目录和多个 `impl Service` block；不得改变 command / `AppState` 依赖面。
3. `services/mod.rs` 只 re-export 跨 crate 真正需要的 facade 和选定契约，不为测试方便扩大私有 helper 可见性。
4. facade 拆分或文件移动后，更新 `service_facade_contracts.rs` 或同级契约测试，验证构造方式和选定公开方法仍可用。
5. DTO 序列化、错误码、事件类型和 layout-defined app-state JSON（原生映射为 `.app/*.json`）的持久化结构必须保持兼容；仅移动物理文件不构成协议变更授权。
6. `ChatConvenienceService` 与 `WikiIndex` 继续作为独立边界；除非有单独设计批准，不并入四个聚焦 facade。
7. `ProjectRegistry`、全局 trust store 与 assessment / repair registry 保持三个独立概念；注册 canonical root 不得被解释为用户信任。
8. 项目上下文必须携带后端派生的 layout / access policy；command 与 service 不能仅凭前端 disabled 状态假设能力可用。

## 30. 后续开发 Agent 禁止事项

- 不要在 command 层堆业务逻辑。
- 不要绕过 `ProjectContext` 读写项目文件。
- 不要让前端传任意绝对路径执行写操作。
- 不要引入数据库保存项目内容。
- 不要把 API Key 写入项目文件或日志。
- 不要静默覆盖用户文件。
- 不要跳过 Git 检查点执行高风险操作。
- 不要让每个服务各自发明任务进度格式。
- 不要让每个服务各自发明错误格式。
- 不要让前端自己保存危险操作继续执行参数。
- 不要把普通资料目录原地初始化、移动或写 marker。
- 不要在 quick assessment 或打开外部目录时初始化 Git、创建 `.app`、执行项目内容或发送外部 AI。
- 不要把路径注册当成用户信任，或绕过 restricted / read-only access policy。
- 不要强行指定尚未验证的 PDF / Office 解析库。
- 不要把样本 `wiki/wiki/` 当成应用源码。
