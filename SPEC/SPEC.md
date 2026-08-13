# LLM Wiki Desktop — 项目规格说明

> Import V2、来源库、媒体、OCR / ASR、平台登录态和 Source AI 整理的规范入口为 [`../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md)。本文件只保留总规格摘要；发生冲突时以前述已确认设计为准。
> §19 的 32 条场景、review §12.2 的 26 条合同、14 类真实夹具与 9 条禁止关闭方式统一登记在 [`../docs/qa/import-source-media-flow-batch9-evidence.json`](../docs/qa/import-source-media-flow-batch9-evidence.json)。
> 工作流主导航、内建工作流、准备页、项目隔离队列、可观察流水线、确认与恢复行为的规范入口为 [`../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md)。本文件只保留跨模块总规格；发生冲突时以前述已确认设计为准。
> 无项目工作台、新建知识库、打开原生 / 兼容知识库、目录评估、受限 / 信任 / 只读、兼容启用、修复与 Import 交接的规范入口为 [`../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)。普通资料文件夹不得原地初始化；与本文旧摘要冲突时以前述已确认设计为准。

## 1. 产品定位

**LLM Wiki Desktop** 是一款本地优先的跨平台桌面应用，用于把个人资料、网页、文档和笔记自动整理成结构化、互相链接、可探索的 Markdown 知识库。

产品遵循 Karpathy 的 LLM Wiki 模式：**Raw Sources（原始资料） → Sources（可读来源） → Wiki（结构化页面） → Schema（规则与配置）**。与传统 RAG 不同，用户显式运行“更新 Wiki”后，知识会持续沉淀为可阅读、可维护、可版本化的页面，而不是导入后自动编译，或只在每次提问时临时检索和拼接。

首版优先服务 **个人知识管理者**，兼顾研究者。核心体验目标是：

1. 用户始终处在完整桌面工作台中，通过“新建知识库”或“打开已有知识库”开始。
2. 导入确认后立即读到可浏览 Source；这就是首次价值，不依赖 Wiki 编译、图谱或 Chat。
3. 用户随后可以更新 Wiki、探索知识图谱、基于 Source / Wiki 问答并导出 HTML / 卡片 / 报告。
4. Agent CLI 提供高级编排能力；Source 已形成后，未配置 Agent 时可使用 BYOK API 完成适用的 AI 整理、Wiki 更新和 Chat。

## 2. MVP 范围与验收标准

首版采用“零门槛知识库 + Agent 增强”方案。

### 2.1 必须跑通的闭环

- 新建知识库、打开已有原生 / 兼容知识库；普通资料文件夹通过创建独立知识库并导入进入，不原地初始化。
- 导入多格式资料与媒体，预览最终 Source，确认后写入来源库；编译由用户另行启动。
- 生成 Wiki 页面、索引、概览和页面间链接。
- 查看知识图谱，支持页面类型着色、社区聚类、布局缓存。
- 基于可读 Source 或 Wiki 内容 Chat 问答，并展示引用来源。
- 生成单篇 HTML 辅助阅读页、知识卡片、项目级 HTML 报告。
- Source 已形成后的 AI 整理、Wiki 编译和 Chat 可走 Agent CLI 或 BYOK API；Import 解析恢复只走用户触发的本地 Agent。
- Git 自动检查点、冲突合并、Lint 自动修复、后台任务都可跑通。

### 2.2 测试资料

- 小型多格式资料包：10-20 个项目，覆盖 PDF、DOCX、PPTX、XLSX/CSV、MD/TXT、HTML、图片、音频、视频、URL 和剪贴板文本，并包含 OCR / ASR / 登录态分支。
- 当前仓库中的真实 `wiki/` 样本项目：用于验证真实规模、Obsidian 兼容性和图谱性能。

### 2.3 性能目标

- 面向约 200-500 篇 Wiki 页面。
- 搜索、文章打开、图谱缓存后二次打开应达到秒级可用。
- 图谱首次构建可以进入后台任务，但必须展示进度，并允许取消。

## 3. 技术栈

| 层 | 技术 |
|---|---|
| 桌面框架 | Tauri v2（Rust 后端） |
| 前端 | React 19 + TypeScript + Vite |
| UI 组件 | shadcn/ui + Tailwind CSS v4 |
| 编辑器 | Milkdown（ProseMirror WYSIWYG） |
| 图谱 | sigma.js + graphology + ForceAtlas2 |
| 图标 | Lucide React |
| 状态管理 | Zustand |
| 国际化 | react-i18next（中英双语） |
| Markdown 渲染 | remark-gfm + rehype-katex + remark-math |
| 数据存储 | 纯文件（Markdown + JSON），自动 Git 版本管理，无数据库 |

## 4. 目标平台

- Windows（x64，`.msi` 安装包）
- macOS（Apple Silicon + Intel，`.dmg`）
- Linux（`.deb` / `.AppImage`）

## 5. 项目文件夹模型

应用以项目文件夹为基本单位。每个项目都是一个可被 Obsidian、Git 和外部编辑器直接访问的 Markdown 知识库。

### 5.1 项目结构

以下目录树只定义**新建的原生知识库**。打开兼容知识库时，页面根、Source 根、元数据位置和可用能力由 `ProjectContext.layout` / capabilities 返回；不得用是否存在根 `purpose.md`、`schema.md` 或原生 `wiki/` 作为唯一可用性判断。

```text
project-root/
├── purpose.md              # 知识库目标、关键问题、研究方向
├── schema.md               # Wiki 结构规则、页面类型定义
├── raw/
│   ├── sources/            # 原始本地资料和永久保留的本地媒体，默认不可变
│   ├── web/                # 网页 / 平台证据、原始字幕和远程来源快照
│   ├── assets/             # 图文原图和 Source 所需资源
│   └── extracted/          # 仅兼容读取 legacy 数据；新 staging / 预览不得写入
├── wiki/
│   ├── index.md            # 内容目录（LLM 导航入口）
│   ├── log.md              # 操作历史记录
│   ├── overview.md         # 全局摘要
│   ├── entities/
│   ├── concepts/
│   ├── sources/            # 当前可读、可编辑、编译保护的来源库
│   ├── queries/
│   ├── synthesis/
│   └── comparisons/
├── exports/
│   └── html/               # HTML/卡片/项目报告导出
├── skills/                 # 项目级 Skill，可选
└── .app/                   # 应用状态（JSON 文件）
    ├── import/             # 活动会话、任务、处理尝试和能力需求
    ├── sources/            # sourceId、版本、别名、基线和时间线
    ├── compile/            # change set 与已消费 Source 版本
    ├── bookmarks.json
    ├── chats/
    │   └── {id}.json
    ├── agent-config.json
    ├── settings.json
    ├── graph-cache.json
    ├── import-conflicts.json
    └── tasks/
```

### 5.2 两种文件夹入口

- **打开已有知识库**：从无项目工作台选择一个目录，后端先只读评估。原生或兼容 Markdown 知识库可以直接或受限打开；普通资料目录不因此变成项目。
- **导入**：只在当前知识库内发生，把文件、文件夹、链接或粘贴文本转成布局定义的 evidence 与 Source；新建原生知识库映射为 `raw/` 与 `wiki/sources/`。导入完成后不持续跟踪原始路径。

“新建知识库”是另一项首屏动作，不属于文件夹打开或 Import：它选择父级保存位置并用知识库名称生成新的子目录。

### 5.3 普通资料文件夹

当“打开已有知识库”的只读评估判定所选目录只是普通资料时，应用提供“用这些资料新建知识库”。用户另选项目名称和父级位置后，应用创建独立项目，再进入统一导入会话。原资料目录不写入 `.app/`，不移动或重命名文件，也不创建 `purpose.md` / `schema.md`。

文件通过发现、提取、必要的 OCR / ASR、候选预览和确认后，才以 `sourceId` 为原子边界复制 / 归档到新项目的 raw 证据与 `wiki/sources/`；不支持、失败或未确认文件不创建占位 Source。去重、同名冲突和来源版本规则继续以 Import / Source 权威设计为准。

### 5.4 兼容知识库状态

- 打开评估先执行无写入快速扫描，再在已打开工作台中执行可取消深度扫描。
- 快速评估启动返回 application-scoped `assessmentOperationId`；取消只接受该 opaque ID，不创建项目任务并丢弃未完成快照。完成后返回独立、短期有效的 `assessmentId`，供打开、信任、兼容启用与修复重验。
- 目录格式（当前原生、旧版原生、`nashsu`、Obsidian、普通 Markdown、歧义、普通资料、未知）、信任（受信任、尚未信任）、文件系统访问（可写、只读）与健康（healthy、repairable、recovery、unreadable）分别建模；`repairable` 表示布局仍一致且存在可预览的有界修复计划，`recovery` 表示应用状态损坏但 Markdown 仍可读，`受限` 是能力摘要，Recovery 不是目录格式。
- 健康的旧版 LLM Wiki / `nashsu` 进入兼容 Dashboard；未信任的 Obsidian 或可识别 Markdown vault 默认受限；健康且已信任的兼容知识库直接进入 Dashboard。
- 受限模式允许可读 Markdown、目录树、本地搜索、内存图谱和后台只读盘点；禁用外部 AI、Agent、Skill、项目命令、写入型任务和任何自动修改。
- 信任按 canonical 目录身份存应用全局配置，不通过项目 marker 表示；移动或替换目录后重新确认。
- 兼容启用写入 `.app/` 与 `.app/compat/{purpose.md,schema.md}`，不得把兼容配置写成根目录 `purpose.md` / `schema.md`，也不新增 `.app/project.json` manifest。
- 修复可以自动准备安全的派生状态计划，但任何磁盘写入必须先展示完整确认页；Markdown 可读时允许不修复并保持受限 / 只读。

## 6. 核心架构

### 6.1 来源与知识页面架构

```text
Raw（不可变证据）
  -> Sources（当前可读来源）
     ├-> Wiki pages（用户另行触发编译的派生页面）
     ├-> Graph / Chat（Source-only 即可探索、可问答）
     └-> 与 Wiki pages 一起进入 Graph / Chat / HTML Reports
```

### 6.2 Agent/BYOK 双路径

```text
Tauri 桌面窗口
  ├─ Dashboard / 文章 / Chat / 图谱 / 工作流 / 导入 / Lint / Exports / 设置
  ↓ Tauri IPC
Rust 后端
  ├─ 文件系统与 Git 管理
  ├─ 导入解析与标准化
  ├─ Agent 进程管理
  ├─ BYOK LLM API 调用
  ├─ 搜索与图谱计算
  └─ 导出与后台任务
      ├─ Agent CLI: claude / codex / openclaw / hermes / ...
      └─ LLM APIs: OpenAI / Anthropic / Google / Ollama / Custom
```

工作流是面向用户的产品入口；Agent CLI、BYOK API 与本地规则是可替换的执行路径。默认执行路径由设置决定，单次运行可显式覆盖，缺少所选路径时不得静默回退。

- **Agent CLI**：负责高级 Skill、多文件维护、Lint 自动修复、HTML/报告生成、复杂编译。
- **BYOK API**：Source 已经形成后，支持 AI 整理、Wiki 编译、问答和引用生成；不参与导入解析或恢复。
- **本地程序**：负责项目管理、导入归档、搜索、图谱构建、Git 检查点和 UI 展示。

### 6.3 Agent 集成方式

- 应用启动时检测 `PATH` 中的 Agent CLI，并显示安装状态、版本号和默认绑定。
- 未安装时提供安装指引和安装命令，但应用不替用户执行 Agent 安装命令。
- Agent 执行继承 CLI 自身的权限与沙箱机制，应用不额外实现复杂命令沙箱。
- 应用通过 stdout/stderr 实时读取进度，展示在任务面板中。
- Agent 任务支持取消、后台运行和系统通知。

### 6.4 Skill 系统

```text
skills/
├── import-recovery/
│   └── SKILL.md
├── source-rewrite/
│   └── SKILL.md
├── wiki-ingest/
│   └── SKILL.md
├── wiki-lint/
│   └── SKILL.md
├── wiki-query/
│   └── SKILL.md
├── html-concept-map/
│   ├── SKILL.md
│   └── template.html
├── html-beautiful-read/
│   ├── SKILL.md
│   └── template.html
├── html-knowledge-card/
│   ├── SKILL.md
│   └── template.html
└── html-project-report/
    ├── SKILL.md
    └── template.html
```

SKILL.md 遵循 Claude Code 风格的 skill 约定：YAML frontmatter、触发短语、输入输出约束、工作流程说明。

`import-recovery` 仅供用户主动触发的本地 Agent 使用，只写隔离 staging 候选。`source-rewrite` 用于 Source 的 AI 整理，可走 Agent 或 BYOK 文本处理路线，并始终经过 Diff 与确认。

## 7. 功能规格

### 7.1 Dashboard

- 统计卡片：文章数、实体数、概念数、来源数、链接数、最近任务数。
- 主题分布：按页面类型展示饼图或环形图。
- 知识图谱预览：显示缩略图谱，可点击进入完整图谱。
- 最近活动：读取布局定义的活动日志与任务状态根目录；新建原生知识库映射为 `wiki/log.md` 和 `.app/tasks/`。受限/只读项目允许的本地只读任务只显示本次运行的内存结果。
- 快速操作：导入资料、更新 Wiki、运行健康检查、开始问答、生成内容；工作流类入口进入统一准备与任务模型。

### 7.2 文章浏览、阅读与编辑

文件树：

- 按 `ProjectContext.layout.pageRoots` 展示所有可读 Markdown 页面；新建原生项目对应 `wiki/`，兼容 vault 保留其原目录结构。
- 支持标题、全文关键词、标签、类型、来源过滤。
- 节点显示页面类型、星标状态和文件图标。
- restricted/read-only/recovery 状态继续允许浏览，但隐藏或禁用写操作并说明原因。

阅读：

- 渲染 GFM 表格、代码高亮、KaTeX、`[[wikilinks]]`。
- 展示 YAML frontmatter 元数据。
- 相关文章区域展示与当前页面有关的页面；首版不单独提供反向链接面板。
- 对具有 Source roots 的项目，Sources 继续位于同一文件树；Source 顶部只新增“AI 整理”。纯外部 vault 不伪造 `wiki/sources/`。
- Source 的原始稿、版本时间线、重新 OCR / ASR、更换字幕和刷新来源放在可开关右侧面板。
- AI 整理在固定绑定启动项目、Source 与任务的非模态浮动工作台中运行，生成带唯一 `## 内容概览` 的候选；切页不改绑、切项目时隐藏，完成后默认显示只读最终稿，Diff 与过程按需查看，只有用户明确确认后才更新当前 Source。

编辑：

- 文章页面支持阅读/编辑一键切换。
- 编辑器采用 WYSIWYG，隐藏 Markdown 语法。
- 首版不要求 wikilink 自动补全、frontmatter 可视化编辑、块引用面板或图谱拖线编辑。
- 编辑、新建、重命名与删除要求 trusted writable，后端按 canonical identity、hash 和 Git 策略重验。
- 用户保存 Markdown 后，自动刷新索引和图谱；只有允许持久化的项目才写搜索/图谱缓存。
- 手动链接能力不作为首版重点，页面链接主要由 Agent 编译生成。

### 7.3 收藏与星标

- 收藏：记录感兴趣文章，在项目 app state 可写时存储到 layout-defined bookmarks state（原生映射为 `.app/bookmarks.json`）。
- 星标：标记重要文章，在文件树、Dashboard 和图谱中突出显示。
- 支持自定义收藏夹/分组。

### 7.4 工作流与执行编排

`工作流 / Workflows` 是更新、检查和生成知识库的统一产品入口。首版固定提供三个内建工作流：

| 工作流 | 说明 | 执行方式 | 结果归属 |
|---|---|---|---|
| 更新 Wiki | 读取已确认且发生变化的 Source，生成或更新派生 Wiki 页面；不得写回布局定义的 Source 根目录 | Agent CLI 或 BYOK | 工作流任务详情与 Wiki |
| 健康检查 | 首次运行在项目已信任且有具体 AI 路径时默认完整检查，否则默认本地快速检查；后续记住最近模式 | 本地规则 + Agent/BYOK | 现有 Lint 页面 |
| 生成内容 | 生成单篇辅助阅读页、知识卡片、概念图或项目报告 | Agent Skill 或 BYOK | 现有 Exports 页面 |

工作流界面：

- 使用紧凑行列表和现有右侧上下文面板，不使用配置卡墙。
- 总览优先展示运行中、等待确认或失败的任务；没有需关注任务时展示三个可用工作流。
- 点击工作流进入占据主内容区的结构化准备页，不再打开“运行 Agent”对话框。
- 首次运行要求确认范围；后续相同上下文可以快速重跑。更新 Wiki 默认自动选择变化的 Source，完整重编译只放在高级设置。
- 主界面优先展示阶段、当前处理项、数量进度和安全动作；原始 stdout/stderr 只作为只读次级日志。
- 工作流、任务、确认和历史按项目隔离。单项目内串行执行，重复的项目、工作流、范围和基线组合复用已有任务。
- 任务支持排队、取消、重试和中断恢复说明。重试创建关联的新任务，不覆盖原记录；异常退出后的活动任务标记为“已中断”，不得伪装续跑。
- 无项目时不创建工作流任务；restricted 项目禁止外部 AI/Agent/Skill；任何项目内容写入还要求 trusted writable，并按工作流声明的 Git 策略校验。
- prepare 与 start 都在后端按 canonical project identity 重新校验 trust、access、writability、Git 状态和 baseline。进入设置或信任流程后返回准备页，但不自动运行。

当前实现状态（2026-08-13）：H3–H5 已将 Agent Health route 与 Lint repair 接入上述 Workflows/Task/Confirmation 边界；H6 只负责最终矩阵和状态收口。由于 full gate 与完整性能、负向和 WebView2 证据未全绿，Decision Gate H 与 Batch 7 不解除。

执行路径管理：

- 自动检测 `claude`、`codex`、`openclaw`、`hermes` 等 CLI，并显示安装状态、版本号和默认绑定。
- Agent CLI、BYOK、模型与 Provider 配置保留在设置页；工作流行不暴露执行器选择器。
- 不静默安装 Agent；安装需用户明确确认。
- 缺少必需路径时，工作流仍可点击，但在准备页给出配置入口。
- Query 不属于工作流列表；基于 Source 或 Wiki 的自然语言问答继续由 Chat 承担。

### 7.5 Chat 问答

- 多会话：创建、重命名、删除。
- 基于 Source 或 Wiki 内容问答：先本地搜索相关内容，再由明确的 Agent/BYOK 路径生成回答；不要求先编译。
- 引用溯源：回答中标注 Source/Wiki 类型化引用，可点击跳转。
- 外部问答要求项目已信任；没有 Git 不阻止纯问答。优质回答保存到 layout-defined queries root（原生为 `wiki/queries/`）时才要求可写，并执行覆盖/hash/Git 策略。
- 缺少 AI 配置时提供“去配置”；返回后保留草稿，但不自动发送。
- `ProjectLayout.chatStateRoot` 可写时会话持久化到该根目录（新建原生知识库映射为 `.app/chats/`）；文件系统只读或该路径缺失时只保留当前运行内存会话，并在 UI 明示不持久化。
- 普通全局搜索框不自动调用模型；自然语言问题应进入 Chat。

### 7.6 HTML/卡片/报告生成

首版支持两类输出：

- 单篇文章辅助理解：美化阅读页、知识卡片、思维导图或概念关系图。
- 项目级 HTML 报告：把当前 Wiki 生成可浏览的 HTML 报告。

规则：

- 所有 HTML/卡片/报告生成通过 `skills/html-*` 驱动。
- HTML 模板只影响生成页面样式，不影响 Wiki schema、Lint 规则或 Agent 行为。
- 输出存放在 `ProjectContext.layout` 返回的导出根；新建原生项目默认解析为 `exports/html/`，兼容项目不得被强制改造成该目录结构。
- 应用内 iframe 预览，支持打开导出文件所在位置。

### 7.7 知识图谱

图谱参考 `llm-wiki` 的页面级模型，但输入扩展到当前访问模式允许的可读 Markdown：

- 每个可读 Source/Wiki Markdown 文档是一个页面级节点；不要求先编译 Wiki。
- 页面类型通过 frontmatter 或目录推断：entity、concept、source、synthesis、comparison、query 等。
- 边来自 `[[wikilinks]]` 和多信号关联度模型。
- 首版连线统一表示“相关”，不展示复杂关系类型和关系依据。

交互：

- ForceAtlas2 力导向布局。
- 页面类型着色与社区着色切换。
- Louvain 社区检测。
- 悬停高亮相邻节点。
- 点击节点跳转文章。
- 缩放、拖拽、Fit-to-screen。
- trusted writable 项目把布局缓存到 `ProjectLayout.graphCachePath`（新建原生映射为 `.app/graph-cache.json`）；restricted/read-only 项目只保留内存布局，不写缓存。
- 大型或兼容知识库的深度扫描可在后台继续；扫描未完成时图谱明确标注部分结果和覆盖范围。

### 7.8 导入系统

首版支持多格式、多入口导入，并统一产出可阅读 Source：

| 来源 | 格式 | 说明 |
|---|---|---|
| 本地文档 | PDF, DOC/DOCX, PPT/PPTX, XLS/XLSX, CSV, MD, TXT, HTML | 原生提取优先；扫描页和主体截图按需 OCR |
| 本地图片 | PNG, JPEG, WebP, BMP, TIFF, HEIC/HEIF | 必须识别出有效文字才能生成 Source |
| 本地音频 | MP3, WAV, M4A, AAC, FLAC, OGG, Opus, WMA | 伴随稿优先，否则由用户启用本地 ASR |
| 本地视频 | MP4, MOV, MKV, WebM, AVI, M4V, WMV | 字幕优先；无字幕走 ASR，无有效语音时可走画面 OCR |
| 文件夹 | 普通资料文件夹 | 只导入到当前项目；知识库识别与打开不属于 Import |
| URL/链接 | 普通网页、平台文章、图文、视频、集合 | 轻量抓取、浏览器渲染、平台能力或 Agent 修复分层处理 |
| 剪贴板 | 文本/Markdown | 直接粘贴导入 |
| 后续扩展 | 浏览器扩展、GitHub 仓库、RSS、图片视觉理解 | 非首版硬要求 |

导入流程：

1. 选择资料。
2. 创建项目级可恢复导入会话。
3. 自动执行安全扫描、类型识别、轻量抓取、确定性提取、字幕发现和质量验证。
4. 缺少必要正文时等待用户主动登录、启用 OCR / ASR、安装能力或运行本地 Agent 修复。
5. 展示最终 Source Markdown 预览、资源、质量、目标路径；更新项展示 Diff。
6. 用户点击“导入到来源库”，后端以 `sourceId` 为原子单元写入布局定义的 evidence、来源版本状态和 Source 页面；新建原生知识库映射为 `raw/`、`.app/sources/` 与 `wiki/sources/`。
7. 导入完成后可查看 Sources，或另行点击“用这些来源更新 Wiki”启动独立编译。

文件发现的总文件数、总字节数和预计输出文件数由后端汇总并判断软确认阈值。达到阈值时，扫描结果先持久化到 layout-defined import state root，活动 session 不增加 item；界面展示总量、触发原因、跳过项和每个超大表格的独立估算。总量确认只接受普通文件，超大 Excel / CSV 继续等待独立确认；两阶段都消费同一 scan task/result，不重新扫描目录，并在当前 trusted + writable authority 临界区重验 layout import-state root、项目、根目录、session、task、确认 token、保存的 totals 与全部来源 fingerprint。hard file limit 直接拒绝而不生成部分扫描；取消只标记该扫描已丢弃，不删除或改写来源文件。

一次“处理 N 项”由一个可取消的 operation `BackendTask` 表示，`ImportItem` / session JSON 仍是逐项 partial success、waiting、preview、failed、skipped、cancelled 与 retry 的事实来源。前端通过最多每 100ms 一次的 `import://session-patch` 批量更新 item，并在 terminal cohort 只刷新一次 session summary；旧 `start_import_items_v2` 继续为 `<= 200` 的兼容调用者注册，大批调用使用 `start_import_batch_v2`。

导入层负责无损证据与可读来源；下列路径是新建原生知识库映射：

- `raw/`：不可变原文件、页面证据、原始图片、字幕、OCR / ASR 原始输出和版本证据。
- `wiki/sources/`：忠实、规范化、可阅读、可编辑的当前 Source。
- `.app/`：来源身份、版本、别名、基线、质量、任务和编译消费记录。

OCR 和 ASR 属于导入阶段的按需能力；有可靠正文或字幕时不启用，缺少形成 Source 所需的正文时由用户主动启用。图片视觉理解不在首版范围。

所有成功 URL 导入必须保存：

- 原始页面或平台证据。
- 可追溯资源、字幕或转录。
- 来源元数据和质量信息。
- layout-defined Source root 中的可阅读 Source Markdown（新建原生映射为 `wiki/sources/`）。

完整重复内容不创建第二个 Source；新的 URL 只作为别名。来源更新保存新 raw 版本，并通过 Diff 或三方合并保护人工编辑。

平台登录态使用隔离会话；有效登录态自动复用，没有会话时先匿名尝试，确实需要时再显示“登录并继续”。Cookie、令牌和 API Key 不进入 React、项目文件、日志或导出。

### 7.9 项目管理

- 无项目时仍渲染完整 shell；中心只显示“新建知识库”和“打开已有知识库”两个紧凑卡片，右侧只说明本地目录与打开策略。
- 创建新项目：输入名称、选择父级位置与模板；默认父级为系统 Documents 下的 `LLM Wiki`，之后记住最近父级；目标是父目录下由名称生成的子目录。
- 项目模板不改变初始目录结构，默认“通用”，只在创建时生成不同的 `purpose.md` 和 `schema.md`，创建后不提供切换。
- 创建成功后自动进入当前项目的 Import 工作台，但不自动弹系统文件选择器。
- 打开已有知识库：兼容 Karpathy LLM Wiki、`nashsu/llm_wiki` 风格目录、Obsidian 与普通 Markdown vault，并使用 typed assessment 表达分类、健康、权限、Git 和修复动作。
- 歧义 Markdown 目录让用户选择“以 Markdown 知识库打开”或“用这些资料新建知识库”；选择保存在应用全局配置，不写目录 marker。
- 普通资料目录只走“用这些资料新建知识库”，不在原目录初始化。
- 最近知识库列表支持快速切换；有历史时应用启动自动打开最近知识库并固定落 Dashboard，无历史或路径失效时显示无项目工作台。

项目模板：

- 通用
- 研究
- 阅读
- 个人成长
- 商业

### 7.10 设置

- LLM 配置：OpenAI、Anthropic、Google、Ollama、Custom。
- API Key 存系统钥匙串或凭据管理器。
- Agent 配置：已检测 Agent 列表、安装引导、默认 Agent 绑定。
- 语言：中文 / English。
- 上下文窗口：4K 到 1M tokens 可配置。
- 外观：亮色 / 暗色主题。
- 启动行为：有有效最近知识库时自动打开最近知识库并落 Dashboard；否则进入无项目工作台。设置不再提供跳过该固定规则的目标选项。
- 后台任务：关闭窗口时是否最小化到托盘；默认最小化到托盘并继续任务。
- 更新：支持检查更新；下载和安装必须由用户确认。

## 8. Git 版本、合并与恢复

新建的原生知识库自动初始化并管理 Git，普通用户无需理解 Git。打开外部兼容知识库时，快速评估阶段绝不改动 Git；只有用户确认启用兼容且项目可写时，才按确认页策略初始化或使用仓库。已有 Git 仓库的脏状态不自动清理、暂存、提交、重置或 stash。

### 8.1 自动检查点策略

- 危险操作前创建检查点：删除、覆盖、批量替换、Agent 自动修复、重大重新编译。
- 成功操作后提交最终结果。
- 普通小改动可合并为一次提交，避免历史过碎。

### 8.2 人工编辑保护

Wiki 页面是普通 Markdown，用户可能在应用内、Obsidian 或外部编辑器中修改。

编译合并策略：

1. 编译前记录基线版本。
2. Agent/BYOK 生成候选版本。
3. 检测当前文件是否有外部修改。
4. 无冲突时自动合并。
5. 有冲突时展示 Markdown Diff，由用户确认保留哪一侧或手动合并。

### 8.3 Raw Sources 规则

- `raw/sources/` 默认不可变。
- 来源更新通过新 raw 版本、Diff 或三方合并更新同一 Source。
- 永久删除进入专用二次确认页，展示 Source、raw、资源、字幕 / 转录、基线、全部版本、释放空间和引用页面。
- 删除前自动创建 Git 检查点；派生 Wiki 页面不自动删除，由 Lint 标记缺失引用。

### 8.4 工作流修改确认

- 用户显式启动后，无冲突的普通编译可自动完成；导入不得自动启动编译。
- Update Wiki 的低风险、无冲突修改可在对应检查点成功后自动应用；Health Check 不修改文件或创建检查点；Generate Content 新建制品不要求检查点，覆盖既有制品才要求检查点与确认。Agent repair 则先由用户一次批准整个 selected Finding batch，检查点成功后应用经验证的安全 selected-path 更新/新建。
- 对 Agent repair，只有删除、未授权既有路径覆盖和 baseline/用户编辑冲突必须异步二次确认；批量或广泛重写本身已由初次批批准覆盖。其他产品操作仍按其各自合同对删除、覆盖、广泛重写和冲突进行确认。确认入口展示影响摘要，并允许按需查看 Diff。
- 用户可继续编辑 Markdown；写入前必须复核基线并执行三方合并或转入冲突确认。
- 所有自动修复依靠 Git 检查点提供回滚。

## 9. Lint 健康检查

Lint 采用双层健康检查。

### 9.1 本地快速 Lint

确定性规则由应用本地执行：

- 死链。
- 孤立页面。
- 缺失 frontmatter。
- 布局声明了 Wiki 索引入口时，该入口与实际 Wiki 页面不一致；没有 Wiki 根目录时该规则标记为不适用。
- 空页面。
- 重复文件名。
- 路径大小写问题。
- 缺失资源文件。

### 9.2 Agent 深度 Lint

需要判断的问题交给应用内置、版本固定且以 id/version/content hash 审计的 `wiki-lint` Skill。项目 `skills/wiki-lint/SKILL.md` 不读取、不哈希、不能覆盖内置合同；purpose、schema、layout context 与 Finding evidence 只放在显式不可信数据边界中：

- 重复主题。
- 弱交叉引用。
- 来源缺失。
- 页面结构不符合 `ProjectLayout.schemaContext` 解析出的适用规则；没有 schema 上下文时该规则标记为不适用。
- 内容过期或明显需要更新。
- 跨页面矛盾。

### 9.3 修复策略

- 健康检查工作流本身只读；结果进入现有 Lint 页面，修复由用户从发现项另行启动。
- 确定性问题可由应用一键修复。
- trusted writable 用户选择一批 eligible Finding，并一次批准整个 Agent 修复批次；批准前不创建 task/checkpoint、不运行 Agent，Agent 修复不回退 BYOK。
- 批准后的 queued dispatch 在第一次 Agent repair invocation 前创建 clean-HEAD Git 检查点；失败时 invocation 与候选/真实项目 mutation 均为 0。Agent 只在 task-owned candidate 内修改 backend 授权的 Wiki Markdown，真实项目仅经既有 manifest/hash/checked apply 写入；`raw/**`、忠实 Source、`wiki/sources/**` 和 layout-defined Source roots 永远只读。
- 初次批准覆盖安全的 selected-path 更新与安全新建。删除、未授权既有路径覆盖、baseline 或外部编辑冲突进入持久二次确认；Source/raw 越界直接使候选失败，不提供继续确认。
- 每轮候选应用后复用 deterministic Lint，以稳定 Finding identity 关联 resolved/unresolved/introduced；最多三轮。仍未解决时形成 partial/manual-review typed result，保留 Diff 与 Git rollback 信息，不调用第四轮。
- Agent repair 作为 Lint 发起的隐藏 workflow operation 复用现有项目串行队列、TaskService、历史、取消与恢复，不新增第四个 WorkflowKind 或 Overview 行。

## 10. 后台任务与通知

- 工作流任务支持后台运行。
- 导入下载、OCR、ASR 和能力准备属于可取消后台任务。
- 关闭主窗口时默认最小化到系统托盘，任务继续。
- 页面切换或最小化不停止导入任务；应用重启后耗时下载、OCR、ASR 保持“已暂停，可继续”，不自动恢复。
- 已完成分片和可复用中间结果应保留；用户主动取消才清理临时媒体和分片。
- 用户可在设置中改为关闭时询问或终止任务。
- 系统通知用于任务完成、失败和需要用户确认。
- 点击通知打开结果页、错误日志或 Diff 确认页。

## 11. UI 布局

```text
┌─────────────────────────────────────────────────────────────┐
│ 顶部导航栏（项目名 · 搜索框 · 语言切换 · 设置）            │
├──────┬──────────────────────────────────────┬───────────────┤
│      │                                      │               │
│ 侧边 │              主内容区                │  右侧面板     │
│ 导航 │                                      │  （可折叠）   │
│      │  · Dashboard                         │               │
│ 图标 │  · 文章阅读 / 编辑                   │  · 元数据     │
│      │  · Chat 对话                         │  · 引用来源   │
│      │  · 知识图谱                          │  · 执行上下文 │
│      │  · 工作流总览 / 准备 / 任务详情      │  · 相关文章   │
│      │  · Exports                           │  · Diff 确认  │
│      │                                      │               │
├──────┴──────────────────────────────────────┴───────────────┤
│ 底部状态栏（当前 Agent · 操作状态 · 后台任务 · 文章数）     │
└─────────────────────────────────────────────────────────────┘
```

侧边导航：

- Dashboard
- 文件树 / 文章浏览
- Chat 问答
- 知识图谱
- 工作流
- 导入
- Lint
- Exports
- 设置

侧栏中的原“工作流”分组更名为“知识处理”，其中工作流项使用 Lucide `Workflow` 图标且不显示状态徽标；侧栏底部既有 Agent 状态行保持不变。

右侧面板按主视图切换职责：

- 无项目：只显示本地保存、打开评估、兼容与信任原则；不显示 Agent / BYOK 设置。
- Import：当前来源、一个主操作、候选预览、目标路径、质量、折叠技术详情和日志。
- Source：来源信息、忠实原稿、版本时间线以及重新 OCR / ASR、换字幕、刷新来源。
- 普通知识页面：元数据、引用来源和相关文章。
- Workflows：项目工作流摘要、准备范围与路径、活动任务阶段、确认摘要或完成结果。

## 12. 数据流

### 12.0 新建与打开

```text
应用启动
-> 有有效最近知识库：打开该知识库并落 Dashboard
-> 无有效最近知识库：完整 shell + 双入口工作台

新建知识库
  -> 名称 + 父级位置 + 创建时模板
  -> 校验最终子目录
  -> 创建结构 + 本地 Git 初始提交
  -> 进入 Import（不自动弹选择器）

打开已有知识库
  -> 无写入快速评估
  -> 原生健康：直接 Dashboard
  -> 旧版 LLM Wiki / nashsu：兼容 Dashboard + 可取消深扫
  -> 未信任 Obsidian / Markdown vault：受限兼容 Dashboard + 可取消深扫
  -> 健康且已信任兼容库：Dashboard + 可取消深扫
  -> 歧义：确认作为知识库打开，或新建并导入
  -> 普通资料：新建独立知识库并导入
  -> 损坏：展示修复计划；确认写入，或受限 / 只读打开
```

### 12.1 导入与编译

下图中的 `raw/`、`wiki/sources/` 和 `wiki/*` 路径是新建原生知识库的具体映射；兼容项目使用 `ProjectContext.layout` 提供的等价 roots，并继续受 trust/writable/Git 策略约束。

```text
用户选择资料
  -> 创建 / 恢复 ImportSession
  -> 自动发现、分类、确定性提取与质量验证
  -> 按需等待登录 / OCR / ASR / 能力安装 / Agent 修复
  -> 生成 SourceCandidate
  -> 展示最终 Markdown 预览或更新 Diff
  -> 用户点击“导入到来源库”
  -> 写入 raw 不可变证据、.app 来源版本与 wiki/sources 当前页面
  -> 展示完成摘要
  -> 用户可另行点击“用这些来源更新 Wiki”
  -> 创建 CompileChangeSet(sourceId + versionId)
  -> Update Wiki 应用任何正式写入前创建所需 Git 检查点
  -> Agent 或 BYOK 执行独立 Wiki 编译
  -> 生成或更新 wiki 页面、index.md、overview.md、log.md
  -> 禁止编译器写入 wiki/sources/
  -> 合并检测与冲突处理
  -> 成功后 Git 提交
  -> 刷新搜索、图谱和 UI
```

### 12.2 Chat

```text
用户输入问题
  -> 本地搜索相关 Source / Wiki 页面
  -> 组装 context（页面、类型化引用、聊天历史、layout-resolved purpose）
  -> 已信任项目通过明确的 Agent CLI 或 BYOK API 路径生成回答
  -> 展示回答与 Source/Wiki 引用
  -> writable 项目可保存到 layout-defined queries root
```

### 12.3 Lint

```text
用户运行 Lint
  -> 对可读 Markdown 执行本地快速 Lint（restricted 可内存运行）
  -> 已信任且 concrete route 可执行时可选内置固定 Skill 的 Agent 深度 Lint
  -> 展示问题列表
  -> 用户在 trusted writable + clean Git 项目选择一批 Finding 并一次批准
  -> 批准后创建 Git 检查点；Agent 只写 task-owned candidate 的授权 Wiki
  -> backend checked apply；删除 / 未授权覆盖 / 冲突二次确认，raw/Source 越界失败
  -> 每轮 deterministic Lint 复检，最多三轮
  -> 成功或 partial/manual-review 后提交验证结果、保留 Diff/rollback 并刷新 UI
```

## 13. 国际化

- 中文（zh-CN）
- English（en）
- 语言包存储在 `src/i18n/locales/`。
- 通过 react-i18next 管理。
- Agent 生成内容时根据用户语言偏好输出对应语言。

## 14. 开发约束

- 所有项目内容使用 Markdown、JSON 和本地文件存储，不引入数据库。
- 新建原生知识库自动管理 Git；外部兼容知识库只有在用户确认启用时才初始化/使用 Git，且从不自动清理现有脏状态。
- 目录结构兼容 LLM Wiki、`nashsu/llm_wiki` 和 Obsidian。
- Agent 集成采用 CLI spawn 模式，不绑定特定 Agent。
- Skill 系统遵循 SKILL.md 约定。
- 优先使用本地能力，最小化云服务依赖。
- 跨平台路径统一使用 `normalizePath()`，内部路径使用正斜杠。
- 必须安全处理 Unicode 和 CJK 文件名。
- 普通搜索只做关键词、标签、类型、来源过滤；基于 Source/Wiki 的语义问答交给 Chat 的明确 Agent/BYOK 路径。

## 15. 项目初始化

```bash
# 创建 Tauri v2 + React + TypeScript 项目
npm create tauri-app@latest llm-wiki-desktop -- --template react-ts
cd llm-wiki-desktop

# 安装核心依赖
npm install @tauri-apps/api @tauri-apps/plugin-shell
npm install react-router-dom zustand react-i18next i18next
npm install tailwindcss @tailwindcss/vite
npx shadcn@latest init
npm install lucide-react

# 安装图谱依赖
npm install sigma graphology graphology-layout-forceatlas2 graphology-communities-louvain

# 安装 Markdown 渲染依赖
npm install remark-gfm remark-math rehype-katex rehype-highlight

# 安装编辑器
npm install @milkdown/core @milkdown/react @milkdown/plugin-math
```

## 16. 当前实现对齐记录（2026-07-11）

本节只记录已经落地或已经被测试固定的实现约束，不改变上文的核心产品方向；涉及产品范围、数据模型或安全边界的扩大仍需单独确认。

### 16.1 当前工程与编排结构

- 当前仓库已是 Tauri v2 + React 19 + TypeScript + Vite 应用骨架，而不再只是文档与样本 Wiki。
- 前端代码按 `components/app`、`components/ui`、`features/*`、`stores/*`、`types/*`、`hooks/*`、`services/*` 分层；领域视图已覆盖 Dashboard、Wiki、Chat、Graph、Agent、Import、Lint、Exports、Settings。
- 上一条中的 `Agent` 是 2026-07-30 仍存在的实现基线，不是目标信息架构；后续需按工作流设计把该主视图迁移为 Workflows，同时保留 Agent 检测与执行服务。
- 当前前端工作台的已实现调用链是 `AppShell -> WorkspaceController -> WorkspaceRouter -> lazy feature views`。`AppShell` 持有桌面 shell 和全局覆盖层，`WorkspaceController` 组合项目级 workflow，`WorkspaceRouter` 只负责活动视图分发；除 Dashboard 外的 feature view 按需 lazy load。
- 当前全局控制器是 `ProjectConfirmationController`、`TaskLogDrawer`、`Toaster`，统一挂载在 `AppShell`，不由单个 feature view 重复持有。
- 当前跨视图 workflow 是 `useAiCapabilities`、`useTaskLauncher`、`useImportWorkflow`、`useProviderWorkflow`、`useAgentWorkflow`；它们集中处理 AI 能力发现、任务启动、导入、Provider 配置和 Agent 编排，并以项目 key / epoch 阻止异步结果提交到错误项目。
- 后端代码已按 `commands/`、`services/`、`models/`、`errors/`、`tasks/`、`utils/` 拆分。当前已实现调用链是 `commands -> AppState -> stable service facades -> focused use-case modules`；Tauri command 继续保持薄层，业务逻辑通过 `AppState` 中的稳定 service facade 进入聚焦模块，数据通过 typed DTO 和 JSON/Markdown 文件传递。
- `ImportService`、`SearchService`、`LintService`、`ChatService` 是当前稳定 facade。它们保留 command / `AppState` 调用面，并把具体用例拆入各自子模块。
- `ChatConvenienceService` 与 `WikiIndex` 是独立边界，不并入 `ChatService`、`SearchService` 或其他 facade；前者负责 Chat 便捷写入的意图与变更审计，后者负责项目级只读内存索引。
- LintService 已将确定性 rules、ignore persistence、report/history persistence、deep analysis/parser 与 single/batch fixes 拆入独立模块，同时保持 `LintService::default()` facade、`LINT_REPORTS_DIR` 的计划级 `pub(crate)` 可见性、rules helpers 的最窄模块可见性、SearchService 只读目录依赖、Git checkpoint/PendingAction 安全边界以及既有 Lint command/DTO 契约不变。
- 当前实现和测试仍遵循无数据库约束；项目内容继续以 Markdown、JSON 和本地文件为事实来源。

### 16.2 Shell、布局与视觉约束

- 主界面以 Codex-like 桌面工作台为准：左侧分区导航、中心工作面、右侧上下文面板、底部状态栏、紧凑顶栏。
- `UI-Frontend-design/` 已作为 UI 对齐参照；实现侧通过 `src/styles.css` 的 CSS tokens、绝对 px 字号、固定 pane 高度和 CSS contract tests 固定视觉密度。
- 布局偏好持久化在前端 layout preferences 中；侧栏折叠状态由实际宽度阈值推导，避免折叠标记与 pane 宽度不一致。
- 外观设置已经不是单一亮/暗主题：当前支持 Codex、Paper、Graphite、Mint、Night、High Contrast 等 preset，每个 preset 提供 light/dark CSS 变量。新增主题必须补齐 tokens、测试和中英文可读性。

### 16.3 Chat 与问答实现约束

- `ProjectLayout.chatStateRoot` 可写时，Chat 会话持久化在该根目录（新建原生映射为 `.app/chats/{id}.json`），并允许可选 `contextPagePath`。该字段用于 Wiki 右侧 “Ask AI” 的页面级会话，不引入数据库或外部索引；只读或缺失该路径的布局使用明确标记为非持久化的内存会话。
- Wiki 页面侧栏 Chat 使用页面作用域会话：进入页面时只复用已有 `contextPagePath` 会话，不自动创建；首次发送或点击新建时才创建会话。快速切页必须通过 epoch/作用域守卫避免旧页面会话串到新页面。
- 发送失败或未能创建会话时，输入草稿不得被清空。
- 当前页作为 `pinnedPagePath` 进入检索上下文；固定页优先进入 prompt，且路径必须仍经过项目边界校验。
- 检索命中、图邻居扩展和来源重叠扩展属于 diagnostics；持久化回答引用只来自模型实际输出中的 `[S#]` 标记。保存到 layout-defined queries root 时只写入模型实际引用的来源。

### 16.4 搜索、索引与图谱实现约束

- SearchService 继续是本地关键词/过滤检索，不自动调用模型；问答只能进入 Chat，并由 Chat 选择 Agent/BYOK 执行路径。
- Wiki 索引已采用项目级内存快照缓存，按文件 mtime/size 复用条目，并按项目数量设上限。缓存不得把 bookmark join 状态写死，因为 `.app/bookmarks.json` 变化不一定改变页面文件 mtime。
- 图谱展示已固定为安静、紧凑、可读的分析画布：节点和边尺寸集中在 `graphVisualScale`，默认边颜色/最小线宽必须保持可见，hover/selection/focus 改变 size 或 z-index 时必须触发完整 refresh。
- 图谱移动时不得隐藏边；重建或布局刷新应优先保留可用画面和进度反馈，避免用整页刷新替代局部恢复。

### 16.5 导出与路径展示约束

- Exports 已作为主导航一等视图；导出记录列表需要在表格中保留操作列，避免文件名、路径和操作按钮互相挤压。
- 导出列表的次级路径行显示 basename；完整输出路径保留在 tooltip/title 中。跨平台路径展示统一通过 `pathDisplay` helpers 处理，不在组件内临时拆字符串。
- 成功导出提供收藏、预览、浏览器打开、打开所在文件夹等 icon actions；失败导出保留日志和重试入口。

### 16.6 任务、日志与审计约束

- 后台任务、流式输出、取消、日志抽屉和通知已经成为核心交互的一部分。新增长任务必须接入统一 task 状态，而不是只在单个组件中放本地 loading。
- 工作流任务在前端只按当前项目展示；其他项目的持久任务可继续运行，但必须切换到对应项目后才可查看。Workflows 迁移需在现有 task 事件之上补齐结构化阶段、关联重试、输入指纹去重和“已中断”语义。
- 根目录 `progress.txt` 与 `gotchas.txt` 是项目级协作账本。重要里程碑和易踩坑必须继续记录，并保持历史可追溯。
- 样本 `wiki/wiki/` 仍是验证数据，不是应用源码；但其中 `.app/graph-cache.json` 等应用状态可作为测试真实项目行为的样本数据，提交前必须确认不含密钥或私人内容。

### 16.7 首次使用与项目打开的当前差距

- 截至 2026-07-30，当前 `App.tsx` 仍在无项目时分支到独立 `ProjectStartView`，当前项目服务也仍保留二元“是否项目 / 普通文件夹初始化”路径；这是实现现状，不是目标产品合同。
- 目标实现必须保留完整 shell，并以 typed assessment 替换二元判断，覆盖目录分类、格式、健康、权限、Git、信任、修复和建议动作。
- 在上述迁移真正落地并通过验收前，文档不得把首屏双卡、受限模式、全局信任或自动修复确认描述为已实现能力。

## 17. 参考方向

- Karpathy LLM Wiki：三层知识库模式，Raw Sources -> Wiki -> Schema。
- `nashsu/llm_wiki`：页面级图谱、Wiki 自动编译、桌面应用化思路。
- Open Design：本地 Agent CLI 检测、BYOK API、Skill 驱动生成流程。
