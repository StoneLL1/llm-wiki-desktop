# LLM Wiki Desktop 技术栈与架构边界

## 1. 文档目的

本文面向后续开发 Agent / Claude Code，用来说明 LLM Wiki Desktop 已确定的技术栈、当前已实现架构分层、模块职责和实现边界。

本文会区分“已确定决策”和“实现建议”。实现建议用于降低后续开发跑偏风险，但如果实际工程约束要求调整，必须保证不违反 `PRD.md`、`SPEC.md` 和本文列出的硬边界。

## 2. 技术原则

- 本地优先：用户项目内容默认只在本地文件夹中。
- 文件透明：知识库内容使用 Markdown、JSON 和普通文件。
- 无数据库：项目内容不引入数据库。
- Git 可恢复：批量修改、Agent 修改和高风险操作前创建检查点。
- Agent 增强：Agent CLI 提供高级能力，但 BYOK API 必须支撑核心流程。
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

当前前端工作台调用链是 `AppShell -> WorkspaceController -> WorkspaceRouter`。`AppShell` 持有桌面 shell、右侧上下文面板，以及全局 `ProjectConfirmationController`、`TaskLogDrawer`、`Toaster`；`WorkspaceController` 组合 `useAiCapabilities`、`useTaskLauncher`、`useImportWorkflow`、`useProviderWorkflow`、`useAgentWorkflow` 五条领域 workflow；`WorkspaceRouter` 只分发活动视图。

Dashboard 保持首屏同步加载，Wiki、Chat、Graph、Lint、Exports、Import、Agent 等 feature view 使用 `React.lazy` 按需加载，并统一经过 `Suspense` 和 `ViewErrorBoundary`。这是当前实现事实，不是对 React Router 的推荐。

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
- Agent 面板。
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

- `projectStore`：当前项目、最近项目、项目扫描状态。
- `navigationStore`：当前视图、选中文章、右侧面板状态。
- `taskStore`：全局后台任务、进度、日志、抽屉和选中任务；后端任务事件统一 upsert 到这里。
- `importStore`：导入预览、来源目录和确认状态。
- `settingsStore`：语言、主题、启动行为等 UI 设置。
- `chatStore`：当前会话、消息流、引用来源。
- `graphStore`：图谱节点、边、布局状态、筛选模式。
- `lintStore`、`exportStore`：对应领域的结果、历史和交互状态。
- `toastStore`：全局瞬时通知。

当前跨项目异步边界使用由 `projectId + rootPath` 组成的 project key，并在需要时叠加 request epoch。workflow 在提交视图状态、打开任务抽屉、切换视图或发送 toast 前核对 key / epoch，旧项目结果不得写回新项目 UI。项目文件内容仍以按需读取为主，不把大型 Wiki 全量塞入前端 store。

## 7. Tauri IPC Commands

IPC 层负责把前端意图转成后端服务调用。

当前 command 已按领域拆分：

- Project commands：应用摘要、创建 / 打开 / 预览普通文件夹、扫描、最近项目列表与记忆。
- Import commands：文件 / 文本 / URL 预览、URL 抓取与校验、来源目录、删除 / 替换请求、确认导入和提取文本预览。
- Wiki commands：扫描、读取、保存、创建、重命名、删除请求和书签切换。
- Search commands：`search_wiki` 提供本地 Wiki 关键词 / 过滤搜索，不调用模型。
- Git commands：状态、仓库初始化、检查点和 Markdown diff；当前未注册通用提交或恢复 command。
- Agent commands：检测 CLI、读取 Agent 配置和设置默认 Agent；任务取消与日志归 `Task commands`，当前 Agent command 不直接启动任务。
- LLM commands：Provider 列表 / 保存、密钥保存 / 删除 / 状态、Ollama 可达性和 Provider 测试；当前未注册通用 BYOK 执行 command。
- Chat commands：会话创建 / 列表 / 加载 / 重命名 / 删除、发送消息、保存回答和便捷写入的确认 / 回滚。
- Compile commands：启动编译、确认动作、读取冲突详情和解决冲突。
- File commands：受项目上下文约束的 Markdown / JSON 读写、文件 hash 和 pending action 确认。
- Graph commands：获取 / 构建图谱，以及保存前端计算的布局。
- Lint commands：本地 / 深度检查、报告与历史、single / batch fix 和 ignore 管理。
- Export commands：启动 / 重新生成、列表、书签、预览，以及在浏览器或文件夹中打开导出。
- Settings commands：读取 / 保存设置、Provider 密钥状态和 Chat 便捷写入授权；当前没有更新检查 command。
- Task commands：创建、列表、详情、取消、日志、清理完成项和活动项目绑定。

IPC 输入输出使用结构化 DTO，不用临时拼接字符串承载复杂状态。所有 GUI command 在 `src-tauri/src/lib.rs` 的 `tauri::generate_handler!` 中显式注册；新增或删除 command 时必须同步该注册表。

## 8. Rust 后端服务层

Rust 后端是本地能力核心，负责文件系统、Git、Agent 进程、密钥存储和跨平台能力。

当前 `AppState` 持有的主要 service / registry：

- `ProjectService`
- `FileStore`
- `ImportService`
- `ExtractionService`
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
- `SecretService`
- `TaskService`
- `ProjectRegistry`
- `ConfirmationRegistry`

commands 和 `AppState` 只依赖稳定 facade 类型；聚焦用例可以拆到 facade 子模块，但不把私有子模块暴露为跨 crate 依赖。模块间通过清晰数据结构通信，不让一个服务吞掉所有职责。

## 9. ProjectService

负责项目生命周期：

- 创建项目结构。
- 打开已有项目。
- 判断普通文件夹是否需要初始化。
- 扫描项目健康状态。
- 管理最近项目。
- 读取项目元信息。

硬边界：

- 项目模板只能影响 `purpose.md` 和 `schema.md` 初始内容。
- 核心目录结构必须稳定。
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

## 11. ImportService 与 ExtractionService

### 11.1 ImportService

负责导入和归档：

- 文件选择和拖拽输入的后端处理。
- 文件夹导入。
- 普通文件夹初始化为项目。
- 原文件复制或迁移到 `raw/sources/` / `raw/assets/`。
- 同名和重复文件处理。
- 写入 `.app/import-conflicts.json`。

### 11.2 ExtractionService

负责标准化提取：

- 从支持格式中提取文本。
- 提取图片。
- 提取元数据。
- 生成 `raw/extracted/` 内容。
- 为导入预览提供状态和文本摘要。

### 11.3 解析器策略

当前规格只确定支持格式，没有确定 PDF、DOCX、PPTX、XLSX 等具体解析库。实现时应先定义解析器接口，再选择具体库。

已确认：URL / 网页正文提取使用 Readability.js。

不要在导入层做复杂 OCR 或视觉理解判断。OCR 和视觉理解交给后续 Agent / Skill。

## 12. GitService

负责自动 Git 管理：

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

Git 是用户数据安全边界，不是可选增强。

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

默认策略：

- 配置可用 Agent 时，Agent CLI 是默认优先路径。
- 用户可以在设置或任务启动时手动选择 BYOK API。
- 未配置 Agent 时，BYOK API 必须能跑通核心流程。

安全边界：

- 不静默安装 Agent。
- 不静默执行安装命令。
- Agent 执行继承 CLI 自身权限与沙箱机制。
- 高风险文件修改仍由应用的 Git 和确认流程保护。

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

## 15. SecretService

负责密钥存储：

- Windows Credential Manager。
- macOS Keychain。
- Linux Secret Service 或平台可用凭据管理方案。

要求：

- 项目文件中不能明文保存 API Key。
- 导出项目或复制项目文件夹时不能泄漏密钥。
- UI 只能显示密钥是否已配置，不默认回显完整密钥。

## 16. SearchService

负责本地搜索：

- 标题搜索。
- 全文关键词搜索。
- 标签过滤。
- 类型过滤。
- 来源过滤。
- 页面打开定位。

边界：

- 普通搜索不自动调用模型。
- 语义问答交给 Chat / Agent / BYOK 流程。

## 17. GraphService

后端图谱路径负责扫描、解析、拓扑构建、缓存和数据提供：

- command 通过 `SearchService` 扫描 `wiki/` 页面并解析 frontmatter、`[[wikilinks]]` 和页面元数据。
- `GraphService` 从扫描结果构建节点与边，按 content hash 解析 / 重建 `.app/graph-cache.json`，并向前端提供图谱数据。
- `GraphService` 保存前端回传且与 content hash 匹配的布局和社区结果；陈旧布局不会附着到新版本 Wiki。
- ForceAtlas2 布局和 Louvain 社区检测在前端运行，不在 Rust `GraphService` 中计算。

图谱技术：

- sigma.js 负责前端渲染。
- graphology 负责前端图结构。
- graphology ForceAtlas2 负责前端布局。
- graphology Louvain 负责前端社区检测。
- Rust 后端负责拓扑、缓存、失效判断和布局持久化。

首版边统一表示“相关”。不要提前实现复杂关系类型和证据系统。

## 18. LintService

负责双层健康检查。

本地快速 Lint：

- 死链。
- 孤立页面。
- 缺失 frontmatter。
- `wiki/index.md` 漂移。
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
- 输出到 `exports/html/`。
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

- 导入解析。
- Wiki 编译。
- 图谱构建。
- Agent 深度 Lint。
- HTML / 报告生成。

长任务不能阻塞 UI。关闭主窗口时默认最小化到托盘并继续任务。

## 21. SettingsService

负责设置：

- 语言。
- 主题。
- 启动行为。
- Agent 默认绑定。
- LLM Provider 配置。
- 上下文窗口。
- 后台任务关闭行为。
- 更新检查。

项目级设置可以写入 `.app/settings.json`。全局设置写入应用配置目录。密钥交给 SecretService。

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

存储规则：

- Wiki 页面是 Markdown。
- 应用状态是 JSON。
- 原始资料保持普通文件。
- 导出产物是普通文件。
- 不引入数据库。
- 不把密钥写入项目文件。
- 用户可以用外部编辑器修改 Markdown。

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

## 26. 当前架构演进规则

1. 前端新增跨视图流程时，优先进入 `WorkspaceController` 组合的领域 workflow；`WorkspaceRouter` 只做视图分发。
2. 新增 feature view 默认评估 lazy load，并使用现有 `Suspense` / `ViewErrorBoundary` 边界。
3. 新增异步 workflow 必须接入 project key / epoch 防护；长任务结果必须进入全局 `taskStore`。
4. 新增 Tauri 能力先定义 typed DTO 和薄 command，再通过 `AppState` 的稳定 facade 进入聚焦用例。
5. 高风险操作继续复用全局确认控制器和后端 `ConfirmationRegistry` / Git checkpoint，不在 feature view 保存可绕过重校验的继续执行参数。
6. Provider 非密钥配置与密钥流必须分离：配置可写项目设置，密钥只通过 typed invoke 交给 `SecretService` 和 OS credential store；前端只接收 `hasSecret` 与掩码等状态，不回读完整密钥。

## 27. 后续开发 Agent 注意事项

- 先读 `PRD.md`、`SPEC.md`、`APP_flow.md`、`TECH_STACK.md`。
- 当前仓库包含已初始化的 React 19 + Tauri v2 应用源码；修改架构文档前先核对 `src/`、`src-tauri/src/` 和现有契约测试。
- 不要在没有用户确认时大规模重写产品决策。
- 不要把样本 `wiki/wiki/` 当成应用源码。
- 样本 `wiki/wiki/` 是验证真实规模、Obsidian 兼容性和图谱性能的重要数据。
- 任何涉及删除、覆盖、批量迁移、Agent 自动修复的实现，都必须接入 Git 检查点。
- 任何密钥相关实现都必须走系统凭据管理。
- 任何长任务都必须可取消、可后台运行、可报告进度。
- 任何跨平台路径逻辑都必须测试 Windows、macOS、Linux 风格路径和 CJK 文件名。
