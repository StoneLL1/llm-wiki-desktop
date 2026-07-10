# LLM Wiki Desktop — 项目规格说明

## 1. 产品定位

**LLM Wiki Desktop** 是一款本地优先的跨平台桌面应用，用于把个人资料、网页、文档和笔记自动整理成结构化、互相链接、可探索的 Markdown 知识库。

产品遵循 Karpathy 的 LLM Wiki 模式：**Raw Sources（原始资料） → Wiki（结构化页面） → Schema（规则与配置）**。与传统 RAG 不同，知识会被持续编译成可阅读、可维护、可版本化的 Wiki，而不是每次提问时临时检索和拼接。

首版优先服务 **个人知识管理者**，兼顾研究者。核心体验目标是：

1. 导入资料后，应用生成可浏览的 Wiki 页面。
2. 用户立即看到美观、可探索的知识图谱。
3. 用户可以基于 Wiki 问答、导出 HTML/卡片/报告。
4. Agent CLI 提供高级编排能力；未配置 Agent 时，可临时使用 BYOK API 完成核心流程。

## 2. MVP 范围与验收标准

首版采用“零门槛知识库 + Agent 增强”方案。

### 2.1 必须跑通的闭环

- 新建项目、打开已有项目、把普通资料文件夹初始化为项目。
- 导入多格式资料，预览解析结果，确认后进入编译。
- 生成 Wiki 页面、索引、概览和页面间链接。
- 查看知识图谱，支持页面类型着色、社区聚类、布局缓存。
- 基于 Wiki 内容 Chat 问答，并展示引用来源。
- 生成单篇 HTML 辅助阅读页、知识卡片、项目级 HTML 报告。
- Agent CLI 与 BYOK API 两条路径都可跑通。
- Git 自动检查点、冲突合并、Lint 自动修复、后台任务都可跑通。

### 2.2 测试资料

- 小型多格式资料包：10-20 个文件，覆盖 PDF、DOCX、PPTX、XLSX/CSV、MD/TXT、HTML、URL、剪贴板文本。
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
│   ├── sources/            # 原始资料，默认不可变
│   │   ├── pdfs/
│   │   ├── docs/
│   │   ├── slides/
│   │   ├── sheets/
│   │   ├── markdown/
│   │   ├── links/
│   │   └── other/
│   ├── extracted/          # 标准化后的 Markdown、文本、解析元数据
│   └── assets/             # 从 PDF/DOCX/PPTX/网页等提取的图片资源
├── wiki/
│   ├── index.md            # 内容目录（LLM 导航入口）
│   ├── log.md              # 操作历史记录
│   ├── overview.md         # 全局摘要
│   ├── entities/
│   ├── concepts/
│   ├── sources/
│   ├── queries/
│   ├── synthesis/
│   └── comparisons/
├── exports/
│   └── html/               # HTML/卡片/项目报告导出
├── skills/                 # 项目级 Skill，可选
└── .app/                   # 应用状态（JSON 文件）
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
- **导入到当前项目**：只把文件夹内容归档到当前项目的 `raw/sources/`，导入完成后不再跟踪原始路径。

### 5.3 普通文件夹初始化为项目

当用户把普通资料文件夹“打开为项目”时，应用自动创建项目结构，并按文件类型迁入：

- 文档进入 `raw/sources/docs/`
- PDF 进入 `raw/sources/pdfs/`
- PPTX 进入 `raw/sources/slides/`
- XLSX/CSV 进入 `raw/sources/sheets/`
- MD/TXT 进入 `raw/sources/markdown/`
- 图片进入 `raw/assets/`
- 其他文件进入 `raw/sources/other/`

同名或疑似重复文件处理规则：

- 完全相同文件可跳过或保留一份。
- 不同内容但同名时全部保留并自动重命名。
- 冲突、失败和重命名写入 `.app/import-conflicts.json`，稍后由用户处理。

## 6. 核心架构

### 6.1 三层架构

```text
Raw Sources（不可变原始资料）
  -> Extracted Markdown（标准化中间层）
  -> Wiki（LLM 生成和维护的结构化页面）
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
- **BYOK API**：支持导入后的 Wiki 编译、摘要、问答和引用生成。
- **本地程序**：负责项目管理、导入归档、搜索、图谱构建、Git 检查点和 UI 展示。

### 6.3 Agent 集成方式

- 应用启动时检测 `PATH` 中的 Agent CLI，并显示安装状态、版本号和默认绑定。
- 未安装时提供安装指引和安装命令；高级用户可授权应用执行安装命令。
- Agent 执行继承 CLI 自身的权限与沙箱机制，应用不额外实现复杂命令沙箱。
- 应用通过 stdout/stderr 实时读取进度，展示在任务面板中。
- Agent 任务支持取消、后台运行和系统通知。

### 6.4 Skill 系统

```text
skills/
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
| Ingest | 读取 raw/extracted，生成或更新 wiki 页面 | Agent CLI 或 BYOK |
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

首版支持多格式、多入口导入：

| 来源 | 格式 | 说明 |
|---|---|---|
| 本地文件 | PDF, DOCX, PPTX, XLSX, CSV, MD, TXT, HTML | 文件选择器或拖拽导入 |
| 文件夹 | 任意文件夹 | 可打开为项目，或导入到当前项目 |
| URL/链接 | 网页 | Readability.js 提取正文到 Markdown |
| 剪贴板 | 文本/Markdown | 直接粘贴导入 |
| 后续扩展 | 浏览器扩展、GitHub 仓库、RSS、音视频转写 | 非首版硬要求 |

导入流程：

1. 选择资料。
2. 存入 `raw/sources/` 或 `raw/assets/`。
3. 提取文本、图片、元数据，写入 `raw/extracted/`。
4. 展示解析预览：文件列表、格式、大小、解析成功/失败、提取文本预览、页数或字数。
5. 用户确认后再触发 Wiki 编译。

导入层只负责无损保留：

- 原文件。
- 提取文本。
- 提取图片。
- 来源元数据。

OCR 和视觉理解交给后续编译 Agent/Skill，不在导入层判断图片价值。

URL 导入保存：

- 正文 Markdown。
- 网页图片。
- 来源元数据。

首版不强制保存完整 HTML 快照。

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
- 用户可以明确选择替换或删除原始资料。
- 删除或替换原始资料后，应用生成变更预览；用户确认后批量更新相关 Wiki 内容。

### 8.4 Agent 修改确认

- 普通编译可自动执行。
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
- 关闭主窗口时默认最小化到系统托盘，任务继续。
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

## 12. 数据流

### 12.1 导入与编译

```text
用户选择资料
  -> 保存原文件到 raw/sources/ 或 raw/assets/
  -> 提取文本、图片、元数据到 raw/extracted/
  -> 展示解析预览
  -> 用户确认
  -> 创建 Git 检查点
  -> Agent 或 BYOK 执行 wiki-ingest
  -> 生成或更新 wiki 页面、index.md、overview.md、log.md
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

## 16. 当前实现对齐记录（2026-07-10）

本节只记录已经落地或已经被测试固定的实现约束，不改变上文的核心产品方向；涉及产品范围、数据模型或安全边界的扩大仍需单独确认。

### 16.1 已初始化工程结构

- 当前仓库已是 Tauri v2 + React 19 + TypeScript + Vite 应用骨架，而不再只是文档与样本 Wiki。
- 前端代码按 `components/app`、`components/ui`、`features/*`、`stores/*`、`types/*`、`hooks/*`、`services/*` 分层；领域视图已覆盖 Dashboard、Wiki、Chat、Graph、Agent、Import、Lint、Exports、Settings。
- 后端代码已按 `commands/`、`services/`、`models/`、`errors/`、`tasks/`、`utils/` 拆分。Tauri command 继续保持薄层，业务逻辑集中在 service 层，数据通过 typed DTO 和 JSON/Markdown 文件传递。
- LintService 已将确定性 rules、ignore persistence、report/history persistence、deep analysis/parser 与 single/batch fixes 拆入独立模块，同时保持 `LintService::default()` facade、SearchService 只读目录依赖、Git checkpoint/PendingAction 安全边界以及既有 Lint command/DTO 契约不变。
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
