# LLM Wiki Desktop 技术栈与架构边界

## 1. 文档目的

本文面向后续开发 Agent / Claude Code，用来说明 LLM Wiki Desktop 已确定的技术栈、推荐架构分层、模块职责和实现边界。

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

## 4. 推荐架构分层

```text
React UI
  -> Frontend State / View Models
  -> Tauri IPC Commands
  -> Rust Backend Services
      -> Project / File Store
      -> Importer
      -> Git Manager
      -> Agent Runner
      -> BYOK LLM Client
      -> Search / Graph Builder
      -> Export / Skill Runner
      -> Settings / Security
  -> Local Project Folder
```

实现时要保持 UI、IPC、服务层和文件写入边界清晰。不要让 React 组件直接承担大量文件系统、Git、Agent 进程管理逻辑。

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

### 5.3 视图命名

文档只规定稳定视图职责，不规定具体 URL 路由。实现可以采用 React Router，也可以采用内部 view state。

## 6. 前端状态管理

Zustand 用于管理前端应用状态。

建议拆分 store：

- `projectStore`：当前项目、最近项目、项目扫描状态。
- `navigationStore`：当前视图、选中文章、右侧面板状态。
- `taskStore`：后台任务、进度、日志摘要。
- `agentStore`：Agent 检测状态、默认 Agent、执行路径。
- `settingsStore`：语言、主题、启动行为等 UI 设置。
- `chatStore`：当前会话、消息流、引用来源。
- `graphStore`：图谱节点、边、布局状态、筛选模式。

实现建议：store 保存 UI 状态和轻量元数据；项目文件内容以按需读取为主，避免一次性把大型 Wiki 全量塞入前端内存。

## 7. Tauri IPC Commands

IPC 层负责把前端意图转成后端服务调用。

建议按领域拆分命令：

- Project commands：创建、打开、扫描、最近项目。
- Import commands：选择文件、复制资料、解析预览。
- Wiki commands：读取页面、保存页面、刷新索引。
- Git commands：初始化、检查点、提交、diff、恢复。
- Agent commands：检测 CLI、启动任务、取消任务、读取日志。
- LLM commands：测试 provider、执行 BYOK 请求。
- Graph commands：构建、读取缓存、刷新布局。
- Lint commands：运行本地检查、触发深度检查、应用修复。
- Export commands：生成 HTML、读取预览、打开导出目录。
- Settings commands：读取、保存、密钥管理、更新检查。

IPC 输入输出必须使用结构化数据，不要用临时拼接字符串承载复杂状态。

## 8. Rust 后端服务层

Rust 后端是本地能力核心，负责文件系统、Git、Agent 进程、密钥存储和跨平台能力。

推荐服务模块：

- `ProjectService`
- `FileStore`
- `ImportService`
- `ExtractionService`
- `GitService`
- `AgentService`
- `LlmService`
- `SearchService`
- `GraphService`
- `LintService`
- `ExportService`
- `SettingsService`
- `SecretService`
- `TaskService`

模块间通过清晰数据结构通信，不要让一个服务吞掉所有职责。

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

负责知识图谱：

- 扫描 `wiki/` 页面。
- 解析 frontmatter。
- 解析 `[[wikilinks]]`。
- 推断页面类型。
- 构建节点和边。
- 运行 ForceAtlas2 布局。
- 运行 Louvain 社区检测。
- 写入 `.app/graph-cache.json`。

图谱技术：

- sigma.js 负责前端渲染。
- graphology 负责图结构。
- ForceAtlas2 负责布局。
- Louvain 用于社区检测。

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

## 25. 依赖安装基线

根据 `SPEC.md`，初始化项目时的依赖基线如下：

```bash
npm create tauri-app@latest llm-wiki-desktop -- --template react-ts
cd llm-wiki-desktop

npm install @tauri-apps/api @tauri-apps/plugin-shell
npm install react-router-dom zustand react-i18next i18next
npm install tailwindcss @tailwindcss/vite
npx shadcn@latest init
npm install lucide-react

npm install sigma graphology graphology-layout-forceatlas2 graphology-communities-louvain

npm install remark-gfm remark-math rehype-katex rehype-highlight

npm install @milkdown/core @milkdown/react @milkdown/plugin-math
```

如果后续实现发现 Tauri v2、Tailwind v4 或 React 19 的安装命令已有变化，应以官方当前命令为准，但不能改变本文确定的技术方向，除非用户明确批准。

## 26. 推荐实现顺序

1. Tauri v2 + React + TypeScript 项目骨架。
2. shadcn/ui、Tailwind、Lucide、基础布局。
3. 项目创建、打开、扫描。
4. FileStore 和路径规范化。
5. GitService 初始化和检查点。
6. ImportService 基础归档。
7. ExtractionService 解析预览接口。
8. Wiki 文件树和 Markdown 阅读。
9. Milkdown 编辑和保存。
10. AgentService 检测和任务日志。
11. BYOK LlmService 基础请求。
12. Wiki 编译任务骨架。
13. SearchService。
14. GraphService 和 sigma.js 展示。
15. Chat 问答和引用来源。
16. LintService 本地规则。
17. Agent 深度 Lint。
18. ExportService 和 `skills/html-*`。
19. TaskService、托盘和通知。
20. i18n、主题、更新检查和多平台打包。

## 27. 后续开发 Agent 注意事项

- 先读 `PRD.md`、`SPEC.md`、`APP_flow.md`、`TECH_STACK.md`。
- 当前仓库还不是完整应用源码仓库，先确认是否已经初始化 Tauri 项目。
- 不要在没有用户确认时大规模重写产品决策。
- 不要把样本 `wiki/wiki/` 当成应用源码。
- 样本 `wiki/wiki/` 是验证真实规模、Obsidian 兼容性和图谱性能的重要数据。
- 任何涉及删除、覆盖、批量迁移、Agent 自动修复的实现，都必须接入 Git 检查点。
- 任何密钥相关实现都必须走系统凭据管理。
- 任何长任务都必须可取消、可后台运行、可报告进度。
- 任何跨平台路径逻辑都必须测试 Windows、macOS、Linux 风格路径和 CJK 文件名。
