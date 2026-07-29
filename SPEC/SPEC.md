# LLM Wiki Desktop — 项目规格说明

> Import V2、来源库、媒体、OCR / ASR、平台登录态和 Source AI 整理的规范入口为 [`../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md)。本文件只保留总规格摘要；发生冲突时以前述已确认设计为准。
> §19 的 32 条场景、review §12.2 的 26 条合同、14 类真实夹具与 9 条禁止关闭方式统一登记在 [`../docs/qa/import-source-media-flow-batch9-evidence.json`](../docs/qa/import-source-media-flow-batch9-evidence.json)。

## 1. 产品定位

**LLM Wiki Desktop** 是一款本地优先的跨平台桌面应用，用于把个人资料、网页、文档和笔记自动整理成结构化、互相链接、可探索的 Markdown 知识库。

产品遵循 Karpathy 的 LLM Wiki 模式：**Raw Sources（原始资料） → Wiki（结构化页面） → Schema（规则与配置）**。与传统 RAG 不同，知识会被持续编译成可阅读、可维护、可版本化的 Wiki，而不是每次提问时临时检索和拼接。

首版优先服务 **个人知识管理者**，兼顾研究者。核心体验目标是：

1. 导入资料后，应用生成可浏览的 Wiki 页面。
2. 用户立即看到美观、可探索的知识图谱。
3. 用户可以基于 Wiki 问答、导出 HTML/卡片/报告。
4. Agent CLI 提供高级编排能力；Source 已形成后，未配置 Agent 时可使用 BYOK API 完成 AI 整理、Wiki 编译和 Chat。

## 2. MVP 范围与验收标准

首版采用“零门槛知识库 + Agent 增强”方案。

### 2.1 必须跑通的闭环

- 新建项目、打开已有项目、把普通资料文件夹初始化为项目。
- 导入多格式资料与媒体，预览最终 Source，确认后写入来源库；编译由用户另行启动。
- 生成 Wiki 页面、索引、概览和页面间链接。
- 查看知识图谱，支持页面类型着色、社区聚类、布局缓存。
- 基于 Wiki 内容 Chat 问答，并展示引用来源。
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

- **打开为项目**：该文件夹本身成为 LLM Wiki 项目。应用持续跟踪项目内的 `raw/`、`wiki/`、配置、Git 状态和后台任务。
- **导入到当前项目**：把能够形成可读来源的内容提交到当前项目的 `raw/` 与 `wiki/sources/`，导入完成后不再跟踪原始路径。

### 5.3 普通文件夹初始化为项目

当用户把普通资料文件夹“打开为项目”时，应用先确认项目初始化，再创建项目结构并进入统一导入会话。文件通过发现、提取、必要的 OCR / ASR、候选预览和确认后，才以 `sourceId` 为原子边界写入 raw 证据与 `wiki/sources/`；不支持或失败文件不创建占位 Source。

同名或疑似重复文件处理规则：

- 完全相同文件可跳过或保留一份。
- 不同内容但同名时全部保留并自动重命名。
- 冲突、失败和重命名写入 `.app/import-conflicts.json`，稍后由用户处理。

## 6. 核心架构

### 6.1 来源与知识页面架构

```text
Raw（不可变证据）
  -> Sources（当前可读来源）
  -> Wiki pages（用户另行触发编译的派生页面）
  -> Graph / Chat / HTML Reports（可探索、可问答、可导出）
```

### 6.2 Agent/BYOK 双路径

```text
Tauri 桌面窗口
  ├─ Dashboard / 文章 / Chat / 图谱 / 导入 / Agent 面板 / 设置
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

Agent 优先；未配置 Agent 时允许临时使用 BYOK API。

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
- 最近活动：读取 `wiki/log.md` 和 `.app/tasks/`。
- 快速操作：导入资料、运行 Lint、开始问答、生成 HTML 报告。

### 7.2 文章浏览、阅读与编辑

文件树：

- 按 `wiki/` 目录结构展示所有页面。
- 支持标题、全文关键词、标签、类型、来源过滤。
- 节点显示页面类型、星标状态和文件图标。

阅读：

- 渲染 GFM 表格、代码高亮、KaTeX、`[[wikilinks]]`。
- 展示 YAML frontmatter 元数据。
- 相关文章区域展示与当前页面有关的页面；首版不单独提供反向链接面板。
- Sources 继续位于现有 Wiki 树；Source 顶部只新增“AI 整理”。
- Source 的原始稿、版本时间线、重新 OCR / ASR、更换字幕和刷新来源放在可开关右侧面板。
- AI 整理在固定绑定启动项目、Source 与任务的非模态浮动工作台中运行，生成带唯一 `## 内容概览` 的候选；切页不改绑、切项目时隐藏，完成后默认显示只读最终稿，Diff 与过程按需查看，只有用户明确确认后才更新当前 Source。

编辑：

- 文章页面支持阅读/编辑一键切换。
- 编辑器采用 WYSIWYG，隐藏 Markdown 语法。
- 首版不要求 wikilink 自动补全、frontmatter 可视化编辑、块引用面板或图谱拖线编辑。
- 用户保存 Markdown 后，自动刷新索引、搜索缓存和图谱缓存。
- 手动链接能力不作为首版重点，页面链接主要由 Agent 编译生成。

### 7.3 收藏与星标

- 收藏：记录感兴趣文章，存储在 `.app/bookmarks.json`。
- 星标：标记重要文章，在文件树、Dashboard 和图谱中突出显示。
- 支持自定义收藏夹/分组。

### 7.4 Agent 编排

Agent 管理：

- 自动检测 `claude`、`codex`、`openclaw`、`hermes` 等 CLI。
- 显示安装状态、版本号、默认绑定。
- 不静默安装 Agent；安装需用户明确确认。

核心操作：

| 操作 | 说明 | 执行方式 |
|---|---|---|
| Compile / Ingest | 读取已确认的 `wiki/sources/` 版本，生成或更新其他 Wiki 页面；不写回 Sources | Agent CLI 或 BYOK |
| Lint | 检查 Wiki 健康并修复 | 本地规则 + Agent Skill |
| Query | 基于 Wiki 问答 | Agent CLI 或 BYOK |
| HTML 生成 | 单篇辅助阅读、知识卡片、项目报告 | Agent Skill |

执行面板：

- 实时 stdout/stderr。
- 进度、状态、取消按钮。
- 任务历史。
- 后台运行与系统通知。

### 7.5 Chat 问答

- 多会话：创建、重命名、删除。
- 基于 Wiki 内容问答：先搜索相关页面，再由 Agent/BYOK 生成回答。
- 引用溯源：回答中标注引用页面，可点击跳转。
- 优质回答可保存到 `wiki/queries/`。
- 普通全局搜索框不自动调用模型；自然语言问题应进入 Chat/Agent 问答入口。

### 7.6 HTML/卡片/报告生成

首版支持两类输出：

- 单篇文章辅助理解：美化阅读页、知识卡片、思维导图或概念关系图。
- 项目级 HTML 报告：把当前 Wiki 生成可浏览的 HTML 报告。

规则：

- 所有 HTML/卡片/报告生成通过 `skills/html-*` 驱动。
- HTML 模板只影响生成页面样式，不影响 Wiki schema、Lint 规则或 Agent 行为。
- 输出默认存放在 `exports/html/`，纳入项目文件夹，便于版本管理。
- 应用内 iframe 预览，支持打开导出文件所在位置。

### 7.7 知识图谱

图谱参考 `llm-wiki` 的页面级模型：

- 每个 Wiki 页面是一个节点。
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
- 布局缓存到 `.app/graph-cache.json`，避免每次重新布局。

### 7.8 导入系统

首版支持多格式、多入口导入，并统一产出可阅读 Source：

| 来源 | 格式 | 说明 |
|---|---|---|
| 本地文档 | PDF, DOC/DOCX, PPT/PPTX, XLS/XLSX, CSV, MD, TXT, HTML | 原生提取优先；扫描页和主体截图按需 OCR |
| 本地图片 | PNG, JPEG, WebP, BMP, TIFF, HEIC/HEIF | 必须识别出有效文字才能生成 Source |
| 本地音频 | MP3, WAV, M4A, AAC, FLAC, OGG, Opus, WMA | 伴随稿优先，否则由用户启用本地 ASR |
| 本地视频 | MP4, MOV, MKV, WebM, AVI, M4V, WMV | 字幕优先；无字幕走 ASR，无有效语音时可走画面 OCR |
| 文件夹 | 任意文件夹 | 可打开为项目，或导入到当前项目 |
| URL/链接 | 普通网页、平台文章、图文、视频、集合 | 轻量抓取、浏览器渲染、平台能力或 Agent 修复分层处理 |
| 剪贴板 | 文本/Markdown | 直接粘贴导入 |
| 后续扩展 | 浏览器扩展、GitHub 仓库、RSS、图片视觉理解 | 非首版硬要求 |

导入流程：

1. 选择资料。
2. 创建项目级可恢复导入会话。
3. 自动执行安全扫描、类型识别、轻量抓取、确定性提取、字幕发现和质量验证。
4. 缺少必要正文时等待用户主动登录、启用 OCR / ASR、安装能力或运行本地 Agent 修复。
5. 展示最终 Source Markdown 预览、资源、质量、目标路径；更新项展示 Diff。
6. 用户点击“导入到来源库”，后端以 `sourceId` 为原子单元写入 raw 证据、版本信息和 `wiki/sources/`。
7. 导入完成后可查看 Sources，或另行点击“用这些来源更新 Wiki”启动独立编译。

导入层负责无损证据与可读来源：

- `raw/`：不可变原文件、页面证据、原始图片、字幕、OCR / ASR 原始输出和版本证据。
- `wiki/sources/`：忠实、规范化、可阅读、可编辑的当前 Source。
- `.app/`：来源身份、版本、别名、基线、质量、任务和编译消费记录。

OCR 和 ASR 属于导入阶段的按需能力；有可靠正文或字幕时不启用，缺少形成 Source 所需的正文时由用户主动启用。图片视觉理解不在首版范围。

所有成功 URL 导入必须保存：

- 原始页面或平台证据。
- 可追溯资源、字幕或转录。
- 来源元数据和质量信息。
- `wiki/sources/` 中的可阅读 Source Markdown。

完整重复内容不创建第二个 Source；新的 URL 只作为别名。来源更新保存新 raw 版本，并通过 Diff 或三方合并保护人工编辑。

平台登录态使用隔离会话；有效登录态自动复用，没有会话时先匿名尝试，确实需要时再显示“登录并继续”。Cookie、令牌和 API Key 不进入 React、项目文件、日志或导出。

### 7.9 项目管理

- 创建新项目：选择项目模板，生成不同的 `purpose.md` 和 `schema.md`。
- 项目模板不改变初始目录结构。
- 打开已有项目：兼容 Karpathy LLM Wiki、`nashsu/llm_wiki` 风格目录和 Obsidian Markdown 库。
- 最近项目列表：支持快速切换。
- 启动行为：可在设置中选择，默认打开上次项目。

项目模板：

- 通用
- 研究
- 读书
- 个人成长
- 商业

### 7.10 设置

- LLM 配置：OpenAI、Anthropic、Google、Ollama、Custom。
- API Key 存系统钥匙串或凭据管理器。
- Agent 配置：已检测 Agent 列表、安装引导、默认 Agent 绑定。
- 语言：中文 / English。
- 上下文窗口：4K 到 1M tokens 可配置。
- 外观：亮色 / 暗色主题。
- 启动行为：上次项目、项目选择页、按条件自动打开。
- 后台任务：关闭窗口时是否最小化到托盘；默认最小化到托盘并继续任务。
- 更新：支持检查更新；下载和安装必须由用户确认。

## 8. Git 版本、合并与恢复

应用自动初始化和管理 Git，普通用户无需理解 Git。

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

### 8.4 Agent 修改确认

- 用户显式启动后，无冲突的普通编译可自动完成；导入不得自动启动编译。
- 删除、覆盖、冲突操作必须确认。
- 所有自动修复依靠 Git 检查点提供回滚。

## 9. Lint 健康检查

Lint 采用双层健康检查。

### 9.1 本地快速 Lint

确定性规则由应用本地执行：

- 死链。
- 孤立页面。
- 缺失 frontmatter。
- `wiki/index.md` 与实际页面不一致。
- 空页面。
- 重复文件名。
- 路径大小写问题。
- 缺失资源文件。

### 9.2 Agent 深度 Lint

需要判断的问题交给 `wiki-lint` Skill：

- 重复主题。
- 弱交叉引用。
- 来源缺失。
- 页面结构不符合 `schema.md`。
- 内容过期或明显需要更新。
- 跨页面矛盾。

### 9.3 修复策略

- 确定性问题可由应用一键修复。
- Agent 可自动修复所有可处理问题。
- 修复前创建 Git 检查点，修复后提交结果。
- 高风险删除或冲突修改仍需用户确认。

## 10. 后台任务与通知

- Agent 任务支持后台运行。
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
│      │  · 知识图谱                          │  · Agent 输出 │
│      │  · Agent 执行面板                    │  · 相关文章   │
│      │  · HTML 预览                         │  · Diff 确认  │
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
- Agent 面板
- 导入
- Lint
- 设置

右侧面板按主视图切换职责：

- Import：当前来源、一个主操作、候选预览、目标路径、质量、折叠技术详情和日志。
- Source：来源信息、忠实原稿、版本时间线以及重新 OCR / ASR、换字幕、刷新来源。
- 普通知识页面：元数据、引用来源和相关文章。

## 12. 数据流

### 12.1 导入与编译

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
  -> 高风险编译写入前创建 Git 检查点
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
  -> 本地搜索相关 Wiki 页面
  -> 组装 context（页面、引用、聊天历史、purpose.md）
  -> Agent CLI 或 BYOK API 生成回答
  -> 展示回答与引用
  -> 用户可保存到 wiki/queries/
```

### 12.3 Lint

```text
用户运行 Lint
  -> 本地快速 Lint
  -> 可选 Agent 深度 Lint
  -> 展示问题列表
  -> 创建 Git 检查点
  -> 自动修复可处理问题
  -> 高风险修改请求确认
  -> 成功后提交并刷新 UI
```

## 13. 国际化

- 中文（zh-CN）
- English（en）
- 语言包存储在 `src/i18n/locales/`。
- 通过 react-i18next 管理。
- Agent 生成内容时根据用户语言偏好输出对应语言。

## 14. 开发约束

- 所有项目内容使用 Markdown、JSON 和本地文件存储，不引入数据库。
- 应用自动管理 Git，但用户仍可用外部 Git 工具查看历史。
- 目录结构兼容 LLM Wiki、`nashsu/llm_wiki` 和 Obsidian。
- Agent 集成采用 CLI spawn 模式，不绑定特定 Agent。
- Skill 系统遵循 SKILL.md 约定。
- 优先使用本地能力，最小化云服务依赖。
- 跨平台路径统一使用 `normalizePath()`，内部路径使用正斜杠。
- 必须安全处理 Unicode 和 CJK 文件名。
- 普通搜索只做关键词、标签、类型、来源过滤；语义问答交给 Chat/Agent。

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

- Chat 会话持久化在 `.app/chats/{id}.json`，并允许可选 `contextPagePath`。该字段用于 Wiki 右侧 “Ask AI” 的页面级会话，不引入数据库或外部索引。
- Wiki 页面侧栏 Chat 使用页面作用域会话：进入页面时只复用已有 `contextPagePath` 会话，不自动创建；首次发送或点击新建时才创建会话。快速切页必须通过 epoch/作用域守卫避免旧页面会话串到新页面。
- 发送失败或未能创建会话时，输入草稿不得被清空。
- 当前页作为 `pinnedPagePath` 进入检索上下文；固定页优先进入 prompt，且路径必须仍经过项目边界校验。
- 检索命中、图邻居扩展和来源重叠扩展属于 diagnostics；持久化回答引用只来自模型实际输出中的 `[S#]` 标记。保存到 `wiki/queries/` 时只写入模型实际引用的来源。

### 16.4 搜索、索引与图谱实现约束

- SearchService 继续是本地关键词/过滤检索，不自动调用模型；问答只能进入 Chat/Agent/BYOK 流程。
- Wiki 索引已采用项目级内存快照缓存，按文件 mtime/size 复用条目，并按项目数量设上限。缓存不得把 bookmark join 状态写死，因为 `.app/bookmarks.json` 变化不一定改变页面文件 mtime。
- 图谱展示已固定为安静、紧凑、可读的分析画布：节点和边尺寸集中在 `graphVisualScale`，默认边颜色/最小线宽必须保持可见，hover/selection/focus 改变 size 或 z-index 时必须触发完整 refresh。
- 图谱移动时不得隐藏边；重建或布局刷新应优先保留可用画面和进度反馈，避免用整页刷新替代局部恢复。

### 16.5 导出与路径展示约束

- Exports 已作为主导航一等视图；导出记录列表需要在表格中保留操作列，避免文件名、路径和操作按钮互相挤压。
- 导出列表的次级路径行显示 basename；完整输出路径保留在 tooltip/title 中。跨平台路径展示统一通过 `pathDisplay` helpers 处理，不在组件内临时拆字符串。
- 成功导出提供收藏、预览、浏览器打开、打开所在文件夹等 icon actions；失败导出保留日志和重试入口。

### 16.6 任务、日志与审计约束

- 后台任务、流式输出、取消、日志抽屉和通知已经成为核心交互的一部分。新增长任务必须接入统一 task 状态，而不是只在单个组件中放本地 loading。
- `SPEC/progress.txt` 与 `SPEC/gotchas.txt` 是项目级协作账本。重要里程碑和易踩坑必须继续记录，并保持历史可追溯。
- 样本 `wiki/wiki/` 仍是验证数据，不是应用源码；但其中 `.app/graph-cache.json` 等应用状态可作为测试真实项目行为的样本数据，提交前必须确认不含密钥或私人内容。

## 17. 参考方向

- Karpathy LLM Wiki：三层知识库模式，Raw Sources -> Wiki -> Schema。
- `nashsu/llm_wiki`：页面级图谱、Wiki 自动编译、桌面应用化思路。
- Open Design：本地 Agent CLI 检测、BYOK API、Skill 驱动生成流程。
