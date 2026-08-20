# LLM Wiki Desktop 技术栈与架构边界

> Import V2 的产品与跨层技术不变量见 [`../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md)。本文描述技术边界；任何实现建议不得恢复“导入后自动编译”、URL 不写 Source 或 OCR / ASR 后移到编译阶段的旧行为。
> 全量门禁先运行只读 `check:import-source-media`，验证证据 ID、可执行测试声明、被测试实际消费的真实夹具、禁止项和设置专属迁移入口，再运行前后端测试、构建与静态检查。
> Workflows 的产品行为与跨层契约见 [`../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md)。Agent CLI、BYOK 和本地规则继续作为后端执行能力，但不得再把配置型 Agent 页面作为目标信息架构。
> 无项目工作台、新建知识库、typed 项目打开评估、受限 / 信任 / 只读、兼容启用、修复和深度扫描的跨层合同见 [`../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)。该管线已迁入持续挂载的 `AppShell`；普通资料目录只允许“新建知识库并导入”，不会原地初始化、重排或隐式初始化 Git。

## 1. 文档目的

本文面向后续开发 Agent / Claude Code，用来说明 LLM Wiki Desktop 已确定的技术栈、当前已实现架构分层、模块职责和实现边界。

本文会区分“已确定决策”和“实现建议”。实现建议用于降低后续开发跑偏风险，但如果实际工程约束要求调整，必须保证不违反 `PRD.md`、`SPEC.md` 和本文列出的硬边界。

## 2. 技术原则

- 本地优先：用户项目内容默认只在本地文件夹中。
- 文件透明：知识库内容使用 Markdown、JSON 和普通文件。
- 无数据库：项目内容不引入数据库。
- Git 可恢复：批量修改、Agent 修改和高风险操作前创建检查点。
- 外部目录先评估：打开评估必须只读，不创建 `.app/`、Git、缓存或任务，也不执行目录内代码或发送外部 AI。
- 权限先于能力：项目格式、健康与权限分别建模；受限 / 只读上下文必须在所有写入、Agent、Skill、外部 AI 和任务入口重新校验。
- Agent 增强：Agent CLI 提供高级能力；Source 已形成后，BYOK API 支撑 AI 整理、Wiki 编译和 Chat，不参与 Import 解析恢复。
- 跨平台：Windows、macOS、Linux 都是目标平台。
- CJK 安全：必须正确处理 Unicode 和中文文件名。
- 安全存密钥：API Key 存系统钥匙串或凭据管理器，不进项目文件。

## 3. 已确定技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri v2（Rust 后端） |
| 前端 | React 19 + TypeScript + Vite |
| UI 组件 | shadcn/ui + Tailwind CSS v4 |
| 编辑器 | Milkdown（ProseMirror WYSIWYG） |
| 图谱 | sigma.js + graphology + ForceAtlas2 |
| 社区检测 | graphology Louvain 相关能力 |
| 图标 | Lucide React |
| 状态管理 | Zustand |
| 国际化 | react-i18next（中文 / English） |
| Markdown 渲染 | remark-gfm + remark-math + rehype-katex + rehype-highlight |
| URL 正文提取 | Readability.js |
| 数据存储 | Markdown + JSON + 本地文件 |
| 版本管理 | 自动 Git 检查点和提交 |
| Agent 集成 | CLI spawn：claude / codex / openclaw / hermes / ... |
| LLM API | OpenAI / Anthropic / Google / Ollama / Custom |
| 发布目标 | Windows `.msi`、macOS `.dmg`、Linux `.deb` / `.AppImage` |

## 4. 当前已实现架构分层

```text
React shell/layout
  -> WorkspaceController + feature workflows
  -> typed Tauri invoke
  -> thin command modules
  -> AppState + ProjectRegistry
  -> stable service facades / TaskService / ConfirmationRegistry
  -> local files / Git / Agent / LLM / OS credential store
```

当前前端工作台调用链是 `AppShell -> WorkspaceController -> WorkspaceRouter`。`AppShell` 持有桌面 shell、右侧上下文面板，以及全局 `ProjectConfirmationController`、`TaskLogDrawer`、`Toaster`；`WorkspaceController` 组合 `useAiCapabilities`、`useTaskLauncher`、`useImportWorkflow`、`useProviderWorkflow`、`useAgentWorkflow` 五条领域 workflow；`WorkspaceRouter` 只分发活动视图。这里的 `useAgentWorkflow` 与 Agent view 是 2026-07-30 的实现基线，后续目标是由项目级 Workflows 编排替代主界面职责，而不是删除底层 Agent 能力。

Dashboard 保持首屏同步加载，Wiki、Chat、Graph、Lint、Exports、Import、Agent 等 feature view 使用 `React.lazy` 按需加载，并统一经过 `Suspense` 和 `ViewErrorBoundary`。这是当前实现事实，不是目标导航命名，也不是对 React Router 的推荐；Agent 主视图迁移后应以 Workflows 对用户呈现。

截至 Batch 6，`App.tsx` 始终挂载 `AppShell`，无项目态由 `NoProjectWorkspace` 承接；后端以 typed assessment 将 format、health、filesystem access、trust 与 Git 能力分开建模。打开评估保持零写入，普通资料目录不原地初始化，Git 只在明确的新建/确认写入流程中创建。

跨层仍保持硬边界：React 不直接执行文件系统、Git、Agent 进程或系统凭据操作；typed Tauri invoke 进入显式注册的薄 command，再由 `AppState` 中的稳定 facade、`TaskService` 和 `ConfirmationRegistry` 编排本地能力。

## 5. React UI 层

### 5.1 职责

React UI 负责：

- 主导航和视图切换。
- Dashboard 数据展示。
- 文件树和文章阅读。
- WYSIWYG 编辑器容器。
- Chat 会话界面。
- 图谱画布。
- Workflows 总览、准备页、任务详情和运行历史。
- 导入预览。
- Lint 问题列表。
- Diff 确认。
- HTML 预览。
- 设置表单。

### 5.2 边界

React UI 不应直接实现：

- Git 检查点。
- Agent 进程 spawn。
- API Key 存储。
- 批量文件迁移。
- 跨平台路径规范化核心逻辑。

这些能力应通过 Tauri IPC 调用后端服务。

### 5.3 视图分发与故障隔离

当前实现使用 `navigationStore` 的内部 view state，不把工作台视图绑定到浏览器 URL。`WorkspaceRouter` 负责稳定视图名到 feature view 的映射；lazy chunk 等待状态由 `Suspense` 处理，渲染或加载失败由 `ViewErrorBoundary` 隔离并提供重试。除非产品路由需求改变，不应在 feature view 内另建一套竞争路由状态。

## 6. 前端状态管理

Zustand 用于管理前端应用状态。当前已拆分的主要 store 包括：

- `projectStore`：当前项目、最近项目、no-project / assessing / open 状态、typed assessment、独立的 trust / filesystemAccess / health、能力 readiness 与深度扫描快照。`restricted` 只是后端 capabilities 的 UI 摘要；信任判定和持久化不得由 Zustand 决定。
- `navigationStore`：当前视图、选中文章、右侧面板状态。
- `taskStore`：统一接收后台任务、进度和日志事件；所有列表、抽屉、选择、确认与历史通过当前项目 selector 暴露，不能形成跨项目任务界面。
- `importStore`：导入预览、来源目录和确认状态。
- `settingsStore`：语言、主题等 UI 设置，以及后端返回的最近创建父目录等非敏感全局偏好；目标启动规则固定，不由前端选项分支。
- `chatStore`：当前会话、消息流、引用来源。
- `graphStore`：图谱节点、边、布局状态、筛选模式。
- `lintStore`、`exportStore`：对应领域的结果、历史和交互状态。
- `toastStore`：全局瞬时通知。

当前跨项目异步边界使用 `projectId + rootPath` 并叠加 request epoch；迁移目标必须把 `rootPath` 明确为后端 canonical root，并加入 `identityRevision`。`projectId` 仍是当前进程句柄；持久 workflow 指纹、队列和恢复归属使用后端 opaque `canonicalIdentityKey + identityRevision`。workflow 在提交视图状态、打开任务抽屉、切换视图或发送 toast 前核对 key / epoch，旧项目结果不得写回新项目 UI。项目文件内容仍以按需读取为主，不把大型 Wiki 全量塞入前端 store。

## 7. Tauri IPC Commands

IPC 层负责把前端意图转成后端服务调用。

当前 command 已按领域拆分：

- Project commands：应用摘要、事务式创建、开始 / 查询 / 取消无写入 project-open assessment、按后端保存的 assessment 打开、开始 / 取消深度扫描、信任与兼容启用、读取 / 确认修复计划、最近知识库列表与记忆。quick assessment 使用应用级 operation id，不创建项目 Task。
- Import commands：文件 / 文本 / URL 预览、URL 抓取与校验、来源目录、删除 / 替换请求、确认导入和提取文本预览。
- Wiki commands：扫描、读取、保存、创建、重命名、删除请求和书签切换。
- Search commands：legacy command 名 `search_wiki` 提供 layout-readable Source/Wiki Markdown 的本地关键词 / 过滤搜索，不调用模型。
- Git commands：状态、仓库初始化、检查点和 Markdown diff；当前未注册通用提交或恢复 command。
- Agent commands：检测 CLI、读取 Agent 配置和设置默认 Agent；任务取消与日志归 `Task commands`，当前 Agent command 不直接启动任务。
- LLM commands：Provider 列表 / 保存、密钥保存 / 删除 / 状态、Ollama 可达性和 Provider 测试；当前未注册通用 BYOK 执行 command。
- Chat commands：会话创建 / 列表 / 加载 / 重命名 / 删除、发送消息、保存回答和便捷写入的确认 / 回滚。
- Compile commands：启动编译、确认动作、读取冲突详情和解决冲突。
- File commands：受项目上下文约束的 Markdown / JSON 读写、文件 hash 和 pending action 确认。
- Graph commands：获取 / 构建图谱，以及保存前端计算的布局。
- Lint commands：本地 / 深度检查、报告与历史、single / batch fix 和 ignore 管理。
- Export commands：启动 / 重新生成、列表、书签、预览，以及在浏览器或文件夹中打开导出。
- Settings commands：读取 / 保存项目设置、Provider 密钥状态、Chat 便捷写入授权，以及 project-independent 的全局更新偏好与安装 receipt。
- Update commands：读取全局状态、检查固定 endpoint、下载 / 取消、安装、重启与忽略 offer；输入 DTO 只携带 opaque offer id、用户 consent 和 presentation facts，不能携带 endpoint、artifact URL、signature 或 channel。
- Task commands：创建、列表、详情、取消、日志、清理完成项和活动项目绑定。

IPC 输入输出使用结构化 DTO，不用临时拼接字符串承载复杂状态。所有 GUI command 在 `src-tauri/src/lib.rs` 的 `tauri::generate_handler!` 中显式注册；新增或删除 command 时必须同步该注册表。

## 8. Rust 后端服务层

Rust 后端是本地能力核心，负责文件系统、Git、Agent 进程、密钥存储和跨平台能力。

当前 `AppState` 持有的主要 service / registry：

- `ProjectService`
- `FileStore`
- `ImportV2Service`
- `ImportCapabilityRuntime`
- `ConnectorSessionService`
- `GitService`
- `AgentService`
- `BookmarkService`
- `ChatConvenienceService`
- `ChatService`
- `LlmService`
- `SearchService`
- `GraphService`
- `LintService`
- `ExportService`
- `SettingsService`
- `UpdateService`
- `DesktopUpdateRuntime`
- `SecretService`
- `TaskService`
- `ProjectRegistry`
- `ConfirmationRegistry`

commands 和 `AppState` 只依赖稳定 facade 类型；聚焦用例可以拆到 facade 子模块，但不把私有子模块暴露为跨 crate 依赖。模块间通过清晰数据结构通信，不让一个服务吞掉所有职责。

Import V2 可以在稳定 facade 后组合 `ImportSessionService`、`CapabilityResolver`、`LoginSessionService`、`SourceCandidateService`、`SourceCommitService` 和 `SourceVersionService`。`CompileService` 保持独立，不进入导入提交事务。

`UpdateService` 是 project-independent 的状态机，持有 generation/TTL/identity 绑定的 offer、下载与安装阶段；`DesktopUpdateRuntime` 只保存当前进程中经过 Tauri updater 校验的 runtime handle/bytes。`SettingsService` 持久化 app-global preference 与 phase-aware install receipt。安装确认临界区重新采集 editor/import presentation facts 并锁定编辑器，`ConfirmationRegistry` barrier 原子复查后端拥有的 confirmation、critical task 和 Workflow apply facts；Windows vendor updater 必须检查 `ShellExecuteW > 32` 后才可退出当前进程。

安全 authority 是后端类型边界而不是前端约定：Provider credential 绑定 project/config/canonical origin；mutation 消费 `ProjectWritePermit` 与允许的逻辑 root；Agent/BYOK/Workflow 消费 identity-revision/execution-epoch lease；Git/Agent runner 统一清理环境并禁 hooks/fsmonitor/external diff/credential helper/prompt，bounded timeout/cancel 回收进程树；高风险 filesystem mutation 消费 retained safe-directory binding。

## 9. ProjectService

负责项目生命周期：

- 事务式创建原生项目结构；校验名称、父级位置、最终子目录、CJK / Unicode / 大小写与已存在非空目录。
- 对外部目录执行零写入快速评估，返回 format、health、trust、filesystemAccess、layout、Git、capabilities、warnings、confidence 和建议动作。
- format 分类当前 / legacy 原生、`nashsu`、Obsidian、Markdown vault、歧义 Markdown、普通资料与 unknown；独立 health 返回 healthy、repairable、recovery 或 unreadable。
- 把歧义 Markdown 与普通资料目录路由到用户意图；普通资料目录只返回“新建并导入”，不原地初始化或搬移。
- 以 canonical identity 查询全局信任，并把 trust、filesystemAccess、health、layout 与 capabilities 组合成 `ProjectContext` access policy；必须能表达 trusted + read-only，不能把受限 / 信任 / 只读当作互斥授权事实。
- 规划 / 执行 `.app/compat` 启用和可重校验修复计划。
- 编排可取消深度扫描并发布 partial snapshot。
- 管理最近项目。
- 读取项目元信息。

硬边界：

- 项目模板只能在创建时影响原生根目录 `purpose.md` 和 `schema.md` 初始内容，创建后不提供切换。
- 新建原生知识库的核心目录结构必须稳定；兼容知识库保留评估返回的布局，不强制迁移为原生目录树。
- 打开评估不写盘、不初始化 Git、不执行项目内容。
- 普通资料目录保持原样；兼容库生成的指导文件只写 `.app/compat/`，根级同名文件视为用户内容。
- `ProjectRegistry` 的运行时路径注册不等于用户信任；全局 trust store 与 assessment / repair registry 是独立边界。
- 不能把项目内容写入应用私有数据库。

## 10. FileStore

负责本地文件读写：

- Markdown 页面读取。
- Markdown 页面保存。
- JSON 状态读写。
- 文件枚举。
- 路径规范化。
- 安全重命名。
- Unicode / CJK 文件名处理。

路径规则：

- 内部路径统一使用正斜杠。
- 对外显示保留平台风格。
- 任何来自 UI 的路径都必须校验是否在当前项目范围内。
- 不要允许任意路径写入绕过项目边界。

## 11. Import V2 编排与确定性提取器

### 11.1 ImportV2Service

负责导入会话、来源身份、候选与提交：

- 文件、文件夹、URL、平台内容和剪贴板输入的后端处理。
- 在 layout-defined Import state root 持久化一个项目级活动 `ImportSession` 及其任务、尝试和待办；无可写 state root 时不得启动提交型 Import。
- 统一规范 URL、来源 ID、内容 hash 和别名，完成去重与更新识别。
- 将确定性提取、登录、能力、OCR、ASR、Agent 修复和质量检查编排为 `SourceCandidate`。
- 提交时以 `sourceId` 为原子单元写入 `ProjectLayout` 提供的 evidence、app-state 与 Source write roots；原生映射才是 `raw/`、`.app/` 和 `wiki/sources/`。
- 保护人工编辑，更新时执行 Diff 或三方合并。
- 只在覆盖、合并、替换和删除等高风险操作前创建 Git 检查点。
- 导入完成后生成 `CompileChangeSet` 候选，但不自动启动编译。

### 11.2 Import V2 确定性提取器

负责标准化提取：

- 从文档、图片、音视频、网页和平台内容中提取确定性文本、字幕、资源和元数据。
- 原生文本优先，按内容缺口提出 OCR / ASR 能力需求。
- 通过媒体能力包完成解码、音轨和关键帧处理。
- 生成可验证的中间产物和质量报告，不直接写当前 Source。
- 为候选预览提供最终 Markdown、资源、问题区间和目标路径。

### 11.3 解析器策略

解析器、平台适配器、OCR、ASR 和媒体处理均通过 typed capability / extractor 接口接入。React 只发送用户意图，不拥有进程、路径、登录秘密或能力安装逻辑。

普通网页先走轻量正文提取；正文壳或 JavaScript 页面自动升级到隔离浏览器。平台专用适配器、可安装能力和本地 Agent 修复按已确认优先级接续。

OCR 和 ASR 属于导入层的显式用户授权能力。图片视觉理解不在首版范围。BYOK 不参与导入解析或恢复；本地 Agent 只在隔离 staging 工作区生成候选。

### 11.4 SourceService 与 CompileService 边界

- `SourceService` 管理 `sourceId`、版本、别名、基线、layout-defined Source 页面和整包删除。
- `CompileService` 只在用户点击“用这些来源更新 Wiki”后读取 `sourceId + versionId` change set。
- 编译可以读取现有 Sources 与 Wiki 关系，但不得写入任何 layout-defined Source root。
- 当前 Source Registry / manifest v3 是唯一写入主模型。仅当项目不存在 V2 Source Registry 时，`CompileLegacyAdapter` 才可按原生 legacy 映射只读解析旧 `.app/source-index.json`；兼容读取不得回写旧索引、不得写入任何 layout-defined Source root，也不得把旧记录并入 V2 主模型。

## 12. GitService

负责受策略约束的 Git 管理：

- 初始化 Git 仓库。
- 创建初始提交。
- 高风险操作前创建检查点。
- 成功操作后提交结果。
- 生成 Markdown Diff。
- 检测外部修改。
- 支持恢复或回滚。

必须创建检查点的操作：

- 删除。
- 覆盖。
- 批量替换。
- Agent 自动修复。
- 重大重新编译。
- 原始资料替换或删除。

新建原生知识库自动初始化本地 Git。评估或打开外部知识库绝不自动初始化、`git add`、提交或 stash；只有用户在兼容启用确认页选择后才初始化。用户拒绝 Git 时，阅读、搜索和 Chat 可继续，但所有需要 checkpoint 的写入能力禁用。已有 dirty worktree 不自动处理，只有用户明确授权时才把当前全部变更作为检查点。

## 13. AgentService

负责本地 Agent CLI 集成：

- 检测 `PATH` 中的 Agent CLI。
- 获取版本号。
- 管理默认 Agent。
- spawn Agent 任务。
- 捕获 stdout / stderr。
- 支持取消。
- 支持后台运行。
- 写入任务日志。
- 发出任务状态事件。

执行路径策略：

- 全局默认路径由设置保存；Workflows 和 Chat 可以在单次运行时显式覆盖，但不能静默改变默认值。
- 用户可以在 Source 已形成后的 AI 整理、更新 Wiki、生成内容或 Chat 任务启动时选择 Agent CLI 或 BYOK API。
- 即使未配置 Agent，用户显式选择且已配置的 BYOK API 也必须能跑通这些文本流程，但不能替代 Import recovery。
- 所选路径不可用时返回结构化 prerequisite，前端引导进入设置；不得静默回退到另一执行路径。

安全边界：

- 不静默安装 Agent。
- 不静默执行安装命令。
- Agent 执行继承 CLI 自身权限与沙箱机制。
- 高风险文件修改仍由应用的 Git 和确认流程保护。

导入恢复使用独立 `import-recovery` Skill：

- 只允许本地 Agent，并且必须由用户主动触发。
- 可在当前授权任务工作区使用现有浏览器、媒体、OCR / ASR 和脚本工具。
- 只能写 staging 候选，不得直接写布局定义的 evidence、Source/Wiki roots 或 Git。
- 不得安装软件、执行未知下载二进制、接触原始 Cookie / API Key 或绕过访问控制。

## 14. LlmService

负责 BYOK API：

- Provider 配置。
- 模型配置。
- 请求组装。
- 流式或非流式响应。
- 错误处理。
- 引用上下文传递。

支持 Provider：

- OpenAI
- Anthropic
- Google
- Ollama
- Custom

API Key 不得写入项目文件，必须交给 `SecretService`。

BYOK 只在 Source 已经形成后用于 AI 整理、Wiki 编译、Chat 和引用生成，不参与文件、网页、平台或媒体解析恢复。

## 15. SecretService

负责密钥与平台登录秘密存储：

- Windows Credential Manager。
- macOS Keychain。
- Linux Secret Service 或平台可用凭据管理方案。

要求：

- 项目文件中不能明文保存 API Key。
- 导出项目或复制项目文件夹时不能泄漏密钥。
- UI 只能显示密钥是否已配置，不默认回显完整密钥。
- 平台 Cookie、token 和隔离 profile 引用不得进入 React、项目、日志或导出。
- UI 只接收平台、账号摘要、有效 / 过期状态和最近验证时间。

## 16. SearchService

负责本地搜索：

- 标题搜索。
- 全文关键词搜索。
- 标签过滤。
- 类型过滤。
- 来源过滤。
- 页面打开定位。
- 按 `ProjectContext.layout` 扫描当前访问模式允许的 Source/Wiki Markdown roots；兼容项目不得硬编码只读原生 `wiki/`。

边界：

- 普通搜索不自动调用模型。
- 语义问答只从 Chat 产品入口触发；Agent / BYOK 是 Chat 选择的执行路径。
- Chat retrieval 复用本地结果并保留 Source/Wiki 类型；Source-only 项目无需先编译。

## 17. GraphService

后端图谱路径负责扫描、解析、拓扑构建、缓存和数据提供：

- command 通过 `SearchService` 扫描 `ProjectContext.layout` 允许的 Source/Wiki Markdown，并解析 frontmatter、`[[wikilinks]]` 和页面元数据；不要求先编译。
- `GraphService` 从扫描结果构建节点与边。trusted writable 项目按 content hash 解析 / 重建 `ProjectLayout.graphCachePath`（原生映射为 `.app/graph-cache.json`）；restricted/read-only 项目只返回有界内存结果，不创建缓存。
- `GraphService` 只在 trusted writable 项目保存前端回传且与 content hash 匹配的布局和社区结果；陈旧布局不会附着到新版本内容。
- 深度扫描未完成时 DTO 返回 `partial`、已扫描/待扫描数量和任务引用，UI 不把部分结果显示为空图谱。
- ForceAtlas2 布局和 Louvain 社区检测在前端运行，不在 Rust `GraphService` 中计算。

图谱技术：

- sigma.js 负责前端渲染。
- graphology 负责前端图结构。
- graphology ForceAtlas2 负责前端布局。
- graphology Louvain 负责前端社区检测。
- Rust 后端负责拓扑、访问策略、可选缓存、失效判断和布局持久化。

首版边统一表示“相关”。不要提前实现复杂关系类型和证据系统。

## 18. LintService

负责双层健康检查。

本地快速 Lint：

- 死链。
- 孤立页面。
- 缺失 frontmatter。
- 布局声明了 Wiki 索引/概览入口时，检查对应页面漂移；没有 Wiki 根目录的 Source-only 知识库将该规则标记为“不适用”。
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

自动修复前必须调用 GitService 创建检查点。删除、覆盖和冲突修复必须请求用户确认。

## 19. ExportService 与 Skill Runner

负责 HTML、卡片和报告输出：

- 调用 `skills/html-*`。
- 读取 HTML 模板。
- 生成单篇美化阅读页。
- 生成知识卡片。
- 生成项目级 HTML 报告。
- 输出到 `ProjectContext.layout` 解析的导出根；原生项目默认是 `exports/html/`。
- 为 UI 提供 iframe 预览路径。

边界：

- HTML 模板只影响输出样式。
- HTML 模板不能改变 Wiki schema。
- HTML 模板不能改变 Lint 规则。
- HTML 生成通过 Skill 驱动，不要硬编码为单一不可扩展流程。

## 20. TaskService

负责后台任务：

- 创建任务。
- 更新进度。
- 写入日志。
- 取消任务。
- 恢复任务状态。
- 向前端发送事件。
- 触发系统通知。

后台任务包括：

- 项目兼容深度扫描（只读、可取消、持续发布 partial snapshot）。
- 导入解析。
- 媒体下载、OCR、ASR 与能力安装。
- Source AI 整理和来源重处理。
- 更新 Wiki。
- 图谱构建。
- 健康检查。
- 生成内容。

长任务不能阻塞 UI。关闭主窗口时默认最小化到托盘并继续任务。

应用重启后，耗时下载、OCR 和 ASR 恢复为暂停状态，由用户明确继续；已完成分片可复用。用户主动取消时清理临时数据，后续重试从头开始。

Workflows 迁移必须在 `TaskService` 或其后端编排模块中落实以下契约，不能由 React 本地状态模拟：

- 每个工作流任务都带当前进程使用的 runtime `project_id`、后端 opaque `canonical_identity_key + identity_revision`、稳定 `workflow_kind`、输入范围、基线和输入指纹；持久归属与跨重开去重不得依赖 runtime `project_id`。
- 同一项目的工作流串行执行；不同项目可以独立运行，但前端只能在对应项目内展示和操作。
- 相同 canonical identity/revision、工作流、范围、执行选项、路线和基线的请求按输入指纹去重并返回既有任务。
- 任务事件携带结构化阶段、当前处理项、计数进度和活动记录；stdout/stderr 只作为附属日志。
- 重试创建带 `attempt_of` 关联的新任务，不覆盖原任务。
- 用户可见状态覆盖已排队、运行中、等待确认、已完成、失败、已取消、已中断；异常退出不得把运行中任务恢复成仍在运行。
- 项目 app state 可写时，任务、等待确认与排队状态持久化到 `ProjectLayout.taskStateRoot`（原生映射为 `.app/tasks/`）；排队任务重开后等待用户明确继续，运行中任务映射为已中断并报告可复用阶段。restricted/read-only 允许的只读任务使用 typed 但 non-persistent 的内存/临时状态；其中 trusted read-only Complete Check 也不得尝试创建项目 `.app/`。
- 系统通知只用于等待确认、完成和失败。

## 21. SettingsService

负责设置：

- 语言。
- 主题。
- 固定启动规则所需的最近项目与失效记录。
- 最近创建父目录、歧义 Markdown 意图和 canonical 目录信任记录。
- 默认执行路径与默认 Agent 绑定。
- LLM Provider 配置。
- 上下文窗口。
- 后台任务关闭行为。
- 更新检查。

项目级设置可以写入可写的 `ProjectLayout.settingsPath`（原生映射为 `.app/settings.json`）。最近项目、最近创建父目录、歧义意图和目录信任写入应用配置目录，并可在无项目上下文时读取；目录信任不得写入项目 marker。密钥交给 SecretService。

## 22. 数据存储规范

项目内容结构：

```text
project-root/
├── purpose.md
├── schema.md
├── raw/
├── wiki/
├── exports/
├── skills/
└── .app/
```

上述是新建原生知识库的根结构。兼容知识库保留原目录；受限 / 只读打开不创建 `.app/`。用户确认启用完整功能后，应用自己的指导文件写入 `.app/compat/purpose.md` 与 `.app/compat/schema.md`，根目录同名文件始终是用户内容。

存储规则：

- Wiki 页面是 Markdown。
- 应用状态是 JSON。
- 原始资料保持普通文件。
- 导出产物是普通文件。
- 不引入数据库。
- 不把密钥写入项目文件。
- 用户可以用外部编辑器修改 Markdown。
- 下列路径是新建原生知识库的默认映射；兼容知识库由 `ProjectLayout` 返回等价逻辑位置，缺少写根时对应能力不可用。
- `wiki/sources/` 保存当前可阅读 Source，编译器不得写入。
- `.app/import/` 保存活动会话、任务、尝试和能力需求。
- `.app/sources/` 保存来源身份、版本、别名、编辑基线和时间线。
- `.app/compile/` 保存 Source change set 与已消费版本。
- `.app/compat/` 只保存兼容层应用自有配置；不新增 `.app/project.json` manifest。

## 23. 国际化

使用 react-i18next。

首版语言：

- `zh-CN`
- `en`

语言包建议放在：

```text
src/i18n/locales/
```

Agent 生成内容时，应根据用户语言偏好输出对应语言。

## 24. Markdown 与编辑器

阅读渲染：

- remark-gfm
- remark-math
- rehype-katex
- rehype-highlight

编辑器：

- Milkdown
- ProseMirror WYSIWYG

首版不要求：

- wikilink 自动补全。
- frontmatter 可视化编辑。
- 块引用面板。
- 图谱拖线编辑。

## 25. 依赖管理边界

仓库已经初始化；前端依赖以根目录 `package.json` 和锁文件为事实来源，Rust / Tauri 依赖以 `src-tauri/Cargo.toml` 和锁文件为事实来源。本文只固定技术方向和跨层边界，不再保存一次性脚手架或安装命令。

新增或替换依赖属于实现决策，必须说明它解决的当前问题，并验证 Tauri v2、React 19、Tailwind CSS v4、Windows / macOS / Linux 打包和 CJK 路径兼容性。尚未验证的 PDF / Office 解析库，以及替换现有系统凭据或跨平台进程实现的方案，只能标记为推荐，不能写成当前事实。

应用更新使用 pinned `tauri-plugin-updater` v2.9.0 vendor patch：除 upstream API 外，当前 patch 强制 transport-level manifest size limit，并在 Windows 检查 `ShellExecuteW` launch result。升级 vendor 时必须证明两个边界等价并保留 root integration contracts。

发布技术合同由 `.github/workflows/desktop-release.yml` 和 `release/release-contract.json` 固定：精确 Node/Rust、`npm ci`、Cargo `--locked`、actions SHA pin、同 tag/commit/run 的 4×5 capability catalog 与四 target desktop artifacts、updater 与 OS signing evidence、checksums、SBOM、provenance/attestation、packaged smoke、draft reverse verification，以及 protected final publisher。只有最终 job 可请求 `contents: write`；publish-through-anonymous-verification 使用 guarded `EXIT/INT/TERM` rollback，硬 runner/GitHub control-plane loss仍需 release owner incident response。

本地 fixture、jsdom、`cargo check` 或 unsigned artifact 不代表真实签名安装、升级、公证或匿名 endpoint。当前 Batch 6 Public beta No-Go 与外部 Pending 见 `docs/release/batch-6-acceptance-evidence.md`。

## 26. 当前架构演进规则

1. 前端新增跨视图流程时，优先进入 `WorkspaceController` 组合的领域 workflow；`WorkspaceRouter` 只做视图分发。
2. 新增 feature view 默认评估 lazy load，并使用现有 `Suspense` / `ViewErrorBoundary` 边界。
3. 新增异步 workflow 必须接入 project key / epoch 防护；长任务事件统一进入 `taskStore`，但任何用户可见 selector、抽屉、确认和历史都必须按当前项目隔离。
4. 新增 Tauri 能力先定义 typed DTO 和薄 command，再通过 `AppState` 的稳定 facade 进入聚焦用例。
5. 高风险操作继续复用全局确认控制器和后端 `ConfirmationRegistry` / Git checkpoint，不在 feature view 保存可绕过重校验的继续执行参数。
6. Provider 非密钥配置与密钥流必须分离：配置可写项目设置，密钥只通过 typed invoke 交给 `SecretService` 和 OS credential store；前端只接收 `hasSecret` 与掩码等状态，不回读完整密钥。
7. `ProjectRegistry` 只负责运行时项目句柄授权；用户信任由全局持久层负责。每个 `ProjectContext` 必须携带后端派生的 layout / access policy，所有 service 在执行时重新校验能力。
8. 项目打开 quick scan 必须零写入；兼容启用和 repair 只能通过后端保存、确认时重新评估的 assessment / plan id 继续。

## 27. 后续开发 Agent 注意事项

- 先读 `PRD.md`、`SPEC.md`、`APP_flow.md`、`TECH_STACK.md`。
- 当前仓库包含已初始化的 React 19 + Tauri v2 应用源码；修改架构文档前先核对 `src/`、`src-tauri/src/` 和现有契约测试。
- 不要在没有用户确认时大规模重写产品决策。
- 不要把样本 `wiki/wiki/` 当成应用源码。
- 样本 `wiki/wiki/` 是验证真实规模、Obsidian 兼容性和图谱性能的重要数据。
- 任何涉及删除、覆盖、批量迁移、Agent 自动修复的实现，都必须接入 Git 检查点。
- 任何密钥相关实现都必须走系统凭据管理。
- 任何长任务都必须可取消、可后台运行、可报告进度。
- 工作流长任务必须使用结构化阶段、项目级串行队列和输入指纹去重；不要以原始 Agent 日志代替主状态。
- 任何跨平台路径逻辑都必须测试 Windows、macOS、Linux 风格路径和 CJK 文件名。
