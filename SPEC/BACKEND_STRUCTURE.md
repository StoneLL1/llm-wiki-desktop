# LLM Wiki Desktop 后端架构说明

## 1. 文档目的

本文面向后续开发 Agent / Claude Code，用来确定 LLM Wiki Desktop 的 Tauri / Rust 后端架构、目录结构、模块职责、IPC 边界、任务模型、错误模型和安全规则。

本文不是对现有源码的解释。当前仓库仍以产品文档和样本 Wiki 为主，尚未初始化完整 Tauri 应用源码。后续实现后端时，应先阅读：

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
- 可以在内存中建立临时索引和缓存，但持久化仍落到项目文件夹。
- 所有项目文件读写必须经过 `ProjectContext` 路径安全校验。
- 长任务统一进入 `TaskService`，支持进度、日志、取消、后台运行和事件推送。
- 高风险操作统一返回 `PendingAction`，由用户确认后继续执行。
- 所有 command 使用统一错误模型 `BackendError`。
- API Key 和访问令牌只进入系统凭据管理，不写入项目文件。
- Agent CLI 默认优先，但 BYOK API 必须支撑核心流程。
- Git 检查点是数据安全边界，不是可选增强。

## 3. 推荐目录结构

后端源码放在 Tauri 标准目录下：

```text
src-tauri/
├── Cargo.toml
├── tauri.conf.json
└── src/
    ├── main.rs
    ├── lib.rs
    ├── app_state.rs
    ├── commands/
    │   ├── mod.rs
    │   ├── project_commands.rs
    │   ├── file_commands.rs
    │   ├── import_commands.rs
    │   ├── wiki_commands.rs
    │   ├── git_commands.rs
    │   ├── agent_commands.rs
    │   ├── llm_commands.rs
    │   ├── search_commands.rs
    │   ├── graph_commands.rs
    │   ├── lint_commands.rs
    │   ├── export_commands.rs
    │   ├── settings_commands.rs
    │   └── task_commands.rs
    ├── services/
    │   ├── mod.rs
    │   ├── project_service.rs
    │   ├── file_store.rs
    │   ├── import_service.rs
    │   ├── extraction_service.rs
    │   ├── git_service.rs
    │   ├── agent_service.rs
    │   ├── llm_service.rs
    │   ├── search_service.rs
    │   ├── graph_service.rs
    │   ├── lint_service.rs
    │   ├── export_service.rs
    │   ├── settings_service.rs
    │   └── secret_service.rs
    ├── tasks/
    │   ├── mod.rs
    │   ├── task_service.rs
    │   ├── task_model.rs
    │   ├── task_events.rs
    │   └── cancellation.rs
    ├── models/
    │   ├── mod.rs
    │   ├── project.rs
    │   ├── paths.rs
    │   ├── import.rs
    │   ├── wiki.rs
    │   ├── git.rs
    │   ├── agent.rs
    │   ├── llm.rs
    │   ├── graph.rs
    │   ├── lint.rs
    │   ├── export.rs
    │   ├── settings.rs
    │   └── confirmation.rs
    ├── errors/
    │   ├── mod.rs
    │   ├── backend_error.rs
    │   └── error_codes.rs
    └── utils/
        ├── mod.rs
        ├── path_utils.rs
        ├── json_utils.rs
        ├── markdown_utils.rs
        ├── process_utils.rs
        └── time_utils.rs
```

实现可以根据 Rust 模块习惯调整文件名，但必须保持同等清晰的职责边界。

## 4. 后端分层总览

```text
React UI
  -> Tauri IPC Commands
      -> AppState
          -> Services
              -> File system / Git / Agent CLI / LLM API / OS secrets
          -> TaskService
          -> Event emitter
```

各层职责：

- `commands/`：接收前端请求、参数校验、调用 service、返回 DTO。
- `app_state.rs`：持有共享服务实例和当前应用级状态。
- `services/`：业务能力实现。
- `tasks/`：后台任务生命周期、进度、日志、取消和事件。
- `models/`：领域模型、DTO、持久化 JSON 结构。
- `errors/`：统一错误类型和错误码。
- `utils/`：无状态工具函数。

不要在 command 函数里实现导入、Git、Agent、Lint、导出等核心逻辑。

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

### 5.2 结构化输入输出

每个 command 应有明确请求 / 响应类型，例如：

```rust
pub struct OpenProjectRequest {
    pub path: String,
}

pub struct OpenProjectResponse {
    pub project_id: String,
    pub root_path: String,
    pub summary: ProjectSummary,
    pub warnings: Vec<ProjectWarning>,
}
```

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

`AppState` 持有后端服务和共享运行态。

建议结构：

```rust
pub struct AppState {
    pub project_service: ProjectService,
    pub file_store: FileStore,
    pub import_service: ImportService,
    pub extraction_service: ExtractionService,
    pub git_service: GitService,
    pub agent_service: AgentService,
    pub llm_service: LlmService,
    pub search_service: SearchService,
    pub graph_service: GraphService,
    pub lint_service: LintService,
    pub export_service: ExportService,
    pub settings_service: SettingsService,
    pub secret_service: SecretService,
    pub task_service: TaskService,
}
```

实现可以用 `Arc`、`Mutex`、`RwLock` 或内部 channel 管理并发，但要避免全局可变状态失控。

规则：

- Service 之间依赖要明确。
- 共享状态尽量集中在 `AppState`。
- 当前项目上下文通过 `ProjectContext` 传递。
- 不要让 service 随意从全局变量读取当前项目。

## 7. ProjectContext 路径安全边界

`ProjectContext` 是后端访问项目文件的安全边界。

建议字段：

```rust
pub struct ProjectContext {
    pub project_id: String,
    pub root: PathBuf,
    pub app_dir: PathBuf,
    pub raw_dir: PathBuf,
    pub wiki_dir: PathBuf,
    pub exports_dir: PathBuf,
    pub skills_dir: PathBuf,
}
```

所有项目文件读写必须通过 `ProjectContext` 解析路径。

硬规则：

- 前端不能让后端直接读写任意绝对路径。
- 后端必须拒绝越界路径，例如 `../` 逃逸、符号链接逃逸、绝对路径注入。
- 内部逻辑使用规范化路径。
- 对 UI 返回路径时可以返回展示路径和项目相对路径。
- 项目相对路径统一使用正斜杠。
- 必须安全处理 Unicode 和 CJK 文件名。

建议提供方法：

```rust
impl ProjectContext {
    pub fn resolve_project_path(&self, relative_path: &str) -> Result<PathBuf, BackendError>;
    pub fn resolve_wiki_path(&self, relative_path: &str) -> Result<PathBuf, BackendError>;
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

- 打开普通文件夹并初始化为项目。
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
    pub expires_at: Option<String>,
}
```

确认流程：

1. 前端调用 command。
2. 后端发现需要确认，创建 `PendingAction`。
3. 后端返回 `PendingActionRequired` 或通过事件推送确认请求。
4. 前端展示确认 UI。
5. 用户确认后，前端调用 `confirm_pending_action(id)`。
6. 后端再次校验当前状态。
7. 后端创建 Git 检查点。
8. 后端执行操作。

不要让前端自己拼接危险操作的继续参数。继续执行必须由后端保存的 `PendingAction` 驱动。

## 10. 统一后台任务模型

长任务必须统一进入 `TaskService`。

后台任务包括：

- 导入解析。
- Wiki 编译。
- Agent 执行。
- BYOK LLM 请求。
- 图谱首次构建。
- Agent 深度 Lint。
- 自动修复。
- HTML / 卡片 / 报告导出。

建议任务模型：

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
- 每个任务可以记录日志。
- 可取消任务必须接入取消信号。
- 任务状态变化通过事件推送。
- 关闭主窗口后任务继续运行。
- 任务完成、失败或等待确认时触发系统通知。

## 11. ProjectService

职责：

- 创建项目。
- 打开已有项目。
- 判断目录是否是项目。
- 判断普通文件夹是否需要初始化。
- 初始化项目目录结构。
- 生成 `purpose.md` 和 `schema.md`。
- 管理最近项目。
- 扫描项目摘要。

主要输入：

- 项目路径。
- 项目名称。
- 项目模板。

主要输出：

- `ProjectContext`
- `ProjectSummary`
- `ProjectHealthReport`
- `PendingAction`

硬边界：

- 项目模板只影响 `purpose.md` 和 `schema.md`。
- 核心目录结构必须稳定。
- 打开普通文件夹为项目必须走用户确认。
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

职责：

- 处理文件导入。
- 处理文件夹导入。
- 处理普通文件夹初始化后的资料归档。
- 将原始文件归档到 `raw/sources/` 或 `raw/assets/`。
- 处理同名和重复文件。
- 写入 `.app/import-conflicts.json`。

归档规则：

- PDF -> `raw/sources/pdfs/`
- DOCX 等文档 -> `raw/sources/docs/`
- PPTX -> `raw/sources/slides/`
- XLSX / CSV -> `raw/sources/sheets/`
- MD / TXT -> `raw/sources/markdown/`
- 图片 -> `raw/assets/`
- 其他 -> `raw/sources/other/`

硬边界：

- 导入到当前项目使用复制，不持续跟踪原始路径。
- 打开普通文件夹为项目可能移动或整理文件，必须确认。
- 冲突、失败、自动重命名必须记录。

## 14. ExtractionService

职责：

- 从原始资料提取文本。
- 提取图片。
- 提取来源元数据。
- 写入 `raw/extracted/`。
- 为导入预览提供摘要。

解析器策略：

- 当前只确定格式能力，不强行指定 PDF / DOCX / PPTX / XLSX 解析库。
- 应先定义 `Extractor` 接口，再选择具体解析器。
- URL 正文提取已确定使用 Readability.js。
- OCR 和视觉理解不在导入层做，交给后续 Agent / Skill。

建议接口：

```rust
pub trait Extractor {
    fn supports(&self, input: &SourceFile) -> bool;
    async fn extract(&self, input: ExtractRequest) -> Result<ExtractResult, BackendError>;
}
```

输出应包含：

- 提取文本。
- 提取图片路径。
- 来源元数据。
- 页数、字数或可用统计。
- 成功 / 失败状态。
- 错误原因。

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

- 配置可用 Agent 时，Agent CLI 是默认优先路径。
- 用户可以手动选择 BYOK API。
- 未配置 Agent 时，BYOK API 仍应跑通核心流程。
- 应用不能静默安装 Agent。
- 安装命令必须用户确认。
- Agent 输出修改文件前后必须受 GitService 和 PendingAction 保护。

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
- 不把密钥写入 `.app/settings.json`。
- 普通搜索不能自动调用 LLM。

## 18. SearchService

职责：

- 构建本地搜索索引。
- 标题搜索。
- 全文关键词搜索。
- 标签过滤。
- 类型过滤。
- 来源过滤。
- 为 Chat 召回相关 Wiki 页面。

存储策略：

- 可以在内存中建立索引。
- 可以将轻量缓存写入 `.app/`，但不要引入数据库。
- Markdown 文件仍是事实来源。

边界：

- SearchService 不直接调用 LLM。
- 语义问答由 Chat 流程调用 LlmService 或 AgentService。

## 19. GraphService

职责：

- 扫描 `wiki/` 页面。
- 解析 frontmatter。
- 解析 `[[wikilinks]]`。
- 推断页面类型。
- 构建节点和边。
- 生成图谱数据。
- 写入 `.app/graph-cache.json`。

前后端分工：

- 后端负责扫描、解析、缓存和提供图数据。
- 前端使用 sigma.js / graphology 渲染和交互。
- ForceAtlas2 和 Louvain 可以在前端或后端执行，具体取决于性能和库选择。

首版规则：

- 每个 Wiki 页面是节点。
- 边来自 wikilinks 和多信号关联度。
- 边统一表示“相关”。
- 不实现复杂关系类型和证据系统。

## 20. LintService

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
- `wiki/index.md` 与实际页面不一致。
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

硬边界：

- 修复前创建 Git 检查点。
- 删除、覆盖、冲突修复必须确认。
- Agent 深度 Lint 通过 `wiki-lint` Skill 驱动。

## 21. ExportService

职责：

- 调用 `skills/html-*`。
- 读取 HTML 模板。
- 生成单篇美化阅读页。
- 生成知识卡片。
- 生成项目级 HTML 报告。
- 输出到 `exports/html/`。
- 提供预览路径。

硬边界：

- HTML 模板只影响输出样式。
- HTML 模板不能改变 Wiki schema。
- HTML 模板不能改变 Lint 规则。
- HTML 生成不要硬编码为单一不可扩展流程。

## 22. SettingsService

职责：

- 读取全局设置。
- 读取项目设置。
- 保存设置。
- 管理启动行为。
- 管理语言和主题。
- 管理 Agent 默认绑定。
- 管理 LLM Provider 非密钥配置。
- 管理后台任务关闭行为。
- 管理更新检查配置。

边界：

- 项目级设置写入 `.app/settings.json`。
- 全局设置写入应用配置目录。
- 密钥只由 `SecretService` 管理。

## 23. SecretService

职责：

- 保存 API Key。
- 读取 API Key。
- 删除 API Key。
- 检查 provider 是否已配置密钥。

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
- `WikiPageMeta`
- `ImportPreview`
- `ImportConflict`
- `ExtractResult`
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
- 处理符号链接时必须避免逃逸项目根目录。

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
- 普通文件夹初始化。
- 文件导入。
- Git 检查点。
- 文件覆盖冲突。
- `.app/*.json` 写入。
- CJK 文件名。
- Windows 风格路径和大小写问题。

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

## 29. MVP 后端实现顺序

建议顺序：

1. 初始化 Tauri v2 / Rust 后端目录。
2. 建立 `BackendError`、DTO 和 command 返回规范。
3. 建立 `AppState` 和 service skeleton。
4. 实现 `ProjectContext` 与路径安全工具。
5. 实现 `FileStore` 基础读写。
6. 实现 `ProjectService` 创建 / 打开 / 扫描。
7. 实现 `GitService` 初始化和检查点。
8. 实现 `TaskService` 和事件推送。
9. 实现 `ImportService` 归档和冲突记录。
10. 实现 `ExtractionService` 接口和基础文本格式解析。
11. 实现 Wiki 页面读取 / 保存 commands。
12. 实现 `AgentService` 检测和任务输出。
13. 实现 `LlmService` provider 测试和基础请求。
14. 实现 `SearchService` 本地搜索。
15. 实现 `GraphService` 扫描和缓存。
16. 实现 `LintService` 本地快速规则。
17. 实现 `ExportService` Skill 调用骨架。
18. 实现 `SecretService` 平台密钥存储。
19. 补齐后台通知、取消和恢复。
20. 增加跨平台和 CJK 路径测试。

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
- 不要强行指定尚未验证的 PDF / Office 解析库。
- 不要把样本 `wiki/wiki/` 当成应用源码。
