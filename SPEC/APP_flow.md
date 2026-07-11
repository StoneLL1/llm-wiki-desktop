# LLM Wiki Desktop 应用流程说明

## 1. 文档目的

本文面向后续开发 Agent / Claude Code，用来说明 LLM Wiki Desktop 的应用流程、状态变化、文件读写边界和用户确认规则。

本文不是 UI 视觉稿，也不规定具体前端路由命名。实现时可以使用 React Router、内部视图状态或其他桌面应用导航方式，但必须保持本文描述的视图职责、数据流和安全边界。

主要依据：

- `PRD.md`
- `SPEC.md`
- 当前仓库中的真实样本库：`wiki/wiki/`

## 2. 核心心智模型

LLM Wiki Desktop 是一个本地优先的跨平台桌面知识库应用。它的基本单位不是云端账号，也不是数据库，而是一个本地项目文件夹。

项目文件夹遵循以下长期模型：

```text
Raw Sources
  -> Extracted Markdown
  -> Wiki
  -> Graph / Chat / HTML Reports
```

含义如下：

- `raw/sources/` 保存原始资料，默认不可变。
- `raw/extracted/` 保存从原始资料中提取出来的 Markdown、文本和元数据。
- `wiki/` 保存 LLM / Agent 编译后的结构化 Markdown 页面。
- `.app/` 保存应用状态、任务、缓存、聊天记录、设置等 JSON 数据。
- `exports/html/` 保存 HTML、知识卡片和项目报告输出。
- Git 用于检查点、恢复和冲突保护，普通用户不需要理解 Git。

实现时不要把知识库内容迁入数据库。项目内容必须保持为 Markdown + JSON + 本地文件。

## 3. 全局应用结构

应用主界面包含以下稳定视图：

- Dashboard
- 文件树 / 文章浏览
- Chat 问答
- 知识图谱
- Agent 面板
- 导入
- Lint
- 设置

顶部区域显示当前项目、全局搜索、语言切换和设置入口。底部状态栏显示当前 Agent、操作状态、后台任务和文章数量。右侧面板根据主视图切换，用于展示元数据、引用来源、Agent 输出、相关文章、Diff 确认或 HTML 预览。

实现不要提前假设 URL 路由名称。文档中的视图名称是产品职责，不是强制路径。

### 3.1 当前已实现的跨视图编排

本节记录已经落地的 orchestration 事实，不替代下文的产品流程、安全确认、持久化或审计要求：

- 导入确认链路：`Import preview -> confirm_import_preview -> wikiStore.scan -> optional start_wiki_compile`。确认成功后先刷新当前项目 Wiki；只有用户选择导入后编译时，才继续启动 Wiki 编译任务。
- 任务启动链路：`Task launch -> backend task -> taskStore upsert -> current-project drawer open`。后端返回的任务始终写入全局任务状态；仅当请求所属项目仍是当前项目时，才自动打开并选中任务抽屉。
- 高风险确认链路：`PendingAction -> ProjectConfirmationController -> backend revalidation/checkpoint -> result task/update`。`ProjectConfirmationController` 统一承接项目 PendingAction 和编译冲突；后端在真正执行前重新验证项目、目标状态和既有执行计划，并按操作要求创建或确认 Git checkpoint，随后返回任务或项目/文件状态更新。
- 项目切换链路：`Project switch -> project key/epoch invalidation -> stale UI commits and toasts suppressed`。异步 workflow 在提交视图状态、打开抽屉、切换视图或发送 toast 前校验项目 key / epoch，旧项目结果不得覆盖新项目 UI。

项目切换只抑制过期的 UI commit、抽屉自动打开和 toast，不丢弃后台任务。其他项目的任务继续持久化在各自项目的 `.app/tasks/`，并保留在全局任务列表和任务抽屉中可见、可查看、可取消。

## 4. 启动与项目选择流程

### 4.1 正常启动

1. 应用启动。
2. 读取全局应用设置和最近项目列表。
3. 检查上次项目是否仍存在。
4. 根据用户设置决定进入：
   - 上次项目；
   - 项目选择页；
   - 新建项目流程。
5. 后台检测 Agent CLI 可用性。
6. 恢复未完成或可展示的后台任务状态。

### 4.2 关键读写

全局设置的位置可以由应用自身决定，但不要写入项目内容目录中的用户知识文件。项目级状态写入当前项目的 `.app/`。

项目级常见状态文件：

- `.app/settings.json`
- `.app/agent-config.json`
- `.app/tasks/`
- `.app/graph-cache.json`
- `.app/bookmarks.json`
- `.app/chats/{id}.json`

### 4.3 失败处理

- 最近项目路径不存在时，从最近项目列表中标记失效，并引导用户重新选择。
- 项目结构不完整时，不要静默修复所有内容；先展示检测结果，并允许用户确认初始化缺失结构。
- Agent CLI 检测失败不阻塞应用启动。

## 5. 新建项目流程

### 5.1 用户路径

1. 用户选择“新建项目”。
2. 输入项目名称、保存位置和项目模板。
3. 应用创建统一项目结构。
4. 根据模板生成 `purpose.md` 和 `schema.md`。
5. 初始化 Git 仓库。
6. 创建初始提交。
7. 进入 Dashboard。

### 5.2 必须创建的结构

```text
project-root/
├── purpose.md
├── schema.md
├── raw/
│   ├── sources/
│   ├── extracted/
│   └── assets/
├── wiki/
│   ├── index.md
│   ├── log.md
│   └── overview.md
├── exports/
│   └── html/
├── skills/
└── .app/
```

项目模板只影响 `purpose.md` 和 `schema.md` 的初始内容，不改变核心目录结构。

### 5.3 实现注意点

- 新建项目必须可以被 Obsidian、Git 和外部 Markdown 编辑器直接访问。
- 初始化失败时要回滚未完成的目录创建，或清晰标记失败状态。
- 不要在项目文件中明文保存 API Key。

## 6. 打开普通文件夹为项目

### 6.1 用户路径

1. 用户选择“打开为项目”。
2. 应用判断该目录是否已经是 LLM Wiki 项目。
3. 如果是已有项目，直接打开并扫描。
4. 如果是普通文件夹，展示“初始化为项目”的确认说明。
5. 用户确认后，应用创建项目结构。
6. 按文件类型迁入原始资料。
7. 记录冲突、重命名和失败。
8. 初始化 Git 检查点。
9. 进入导入解析预览。

### 6.2 文件归档规则

- PDF 进入 `raw/sources/pdfs/`
- DOCX 等文档进入 `raw/sources/docs/`
- PPTX 进入 `raw/sources/slides/`
- XLSX / CSV 进入 `raw/sources/sheets/`
- MD / TXT 进入 `raw/sources/markdown/`
- 图片进入 `raw/assets/`
- 其他文件进入 `raw/sources/other/`

### 6.3 冲突规则

- 完全相同文件可跳过或保留一份。
- 不同内容但同名时必须全部保留并自动重命名。
- 冲突、失败和重命名写入 `.app/import-conflicts.json`。

### 6.4 用户确认规则

打开普通文件夹为项目会移动或整理文件，因此必须让用户确认。不要把普通文件夹静默改造成项目。

## 7. 导入到当前项目流程

### 7.1 用户路径

1. 用户点击“导入”或拖拽文件 / 文件夹。
2. 应用让用户选择导入到当前项目。
3. 应用复制资料到当前项目的 `raw/sources/` 或 `raw/assets/`。
4. 应用提取文本、图片和元数据到 `raw/extracted/`。
5. 展示解析预览。
6. 用户确认后，才触发 Wiki 编译。

### 7.2 解析预览必须包含

- 文件名
- 文件类型
- 文件大小
- 解析状态
- 错误原因
- 提取文本预览
- 页数、字数或其他可用元数据

### 7.3 导入层边界

导入层只负责无损保留和标准化提取：

- 保存原文件。
- 提取文本。
- 提取图片。
- 提取来源元数据。

OCR 和视觉理解交给后续编译 Agent / Skill，不在导入层阻塞判断图片价值。

## 8. Wiki 编译流程

### 8.1 默认策略

Agent CLI 是默认优先路径。用户已经配置可用 Agent 时，编译默认走 Agent。

BYOK API 是后备路径，也允许用户在设置或任务启动时手动选择。没有 Agent 时，BYOK API 必须能跑通基础编译、摘要、问答和引用生成。

### 8.2 用户路径

1. 用户在导入预览页确认编译，或手动触发重新编译。
2. 应用创建 Git 检查点。
3. 应用选择执行路径：Agent CLI 或 BYOK API。
4. 编译器读取：
   - `purpose.md`
   - `schema.md`
   - `raw/extracted/`
   - 现有 `wiki/`
5. 生成或更新：
   - Wiki 页面
   - `wiki/index.md`
   - `wiki/overview.md`
   - `wiki/log.md`
6. 应用检测人工编辑冲突。
7. 无冲突时自动合并。
8. 有冲突时展示 Markdown Diff。
9. 成功后提交 Git 结果并刷新 UI、搜索和图谱缓存。

### 8.3 冲突处理

编译前必须记录基线版本。编译后如果发现目标文件已有外部修改，不能直接覆盖。

冲突时用户至少需要能选择：

- 保留当前版本。
- 使用 Agent / BYOK 生成版本。
- 手动合并。

### 8.4 实现注意点

- 编译失败不能破坏已有 Wiki。
- 批量覆盖、删除、重写必须有检查点。
- `raw/sources/` 默认不可变。替换或删除原始资料必须明确确认。

## 9. 文章阅读与编辑流程

### 9.1 阅读

1. 用户进入文件树 / 文章浏览视图。
2. 应用扫描 `wiki/` 目录。
3. 文件树按目录展示页面。
4. 用户选择页面。
5. 应用渲染 Markdown。
6. 右侧面板展示元数据、引用来源和相关文章。

Markdown 渲染必须支持：

- GFM 表格
- 代码高亮
- 数学公式
- `[[wikilinks]]`
- YAML frontmatter 展示

### 9.2 编辑

1. 用户点击编辑。
2. 应用进入 WYSIWYG 编辑模式。
3. 用户保存。
4. 应用写回 Markdown 文件。
5. 刷新索引、搜索缓存和图谱缓存。
6. 必要时记录到 `wiki/log.md`。

### 9.3 首版不要求

- wikilink 自动补全。
- frontmatter 可视化编辑。
- 块引用面板。
- 图谱拖线编辑。
- 独立反向链接面板。

## 10. 知识图谱流程

### 10.1 构建

1. 应用读取 `wiki/` 页面。
2. 解析 frontmatter、目录类型和 `[[wikilinks]]`。
3. 构建页面级节点。
4. 通过 wikilinks 和多信号关联度生成边。
5. 执行布局和社区检测。
6. 写入 `.app/graph-cache.json`。

### 10.2 展示与交互

知识图谱必须支持：

- 页面级节点。
- 页面类型着色。
- 社区着色。
- Louvain 社区检测。
- ForceAtlas2 力导向布局。
- 悬停高亮相邻节点。
- 点击节点进入文章。
- 缩放、拖拽、Fit-to-screen。
- 布局缓存后秒级打开。

首版边统一表示“相关”，不展示复杂关系类型和关系证据。

### 10.3 性能目标

当前样本库 `wiki/wiki/` 有数百个 Markdown 文件，首版目标应覆盖约 200-500 篇 Wiki 页面。图谱首次构建可以进入后台任务，但必须展示进度并允许取消。

## 11. Chat 问答流程

### 11.1 用户路径

1. 用户进入 Chat 视图。
2. 创建或选择一个会话。
3. 输入自然语言问题。
4. 应用先搜索相关 Wiki 页面。
5. 应用组装上下文：
   - 相关页面
   - 引用信息
   - 聊天历史
   - `purpose.md`
6. Agent CLI 或 BYOK API 生成回答。
7. UI 展示回答和引用来源。
8. 用户可将优质回答保存到 `wiki/queries/`。

### 11.2 存储

- 聊天会话存储在 `.app/chats/{id}.json`。
- 保存到 Wiki 的回答存储为 `wiki/queries/` 下的 Markdown 页面。

### 11.3 边界

普通全局搜索只做关键词、标签、类型和来源过滤，不自动调用模型。自然语言问答必须从 Chat / Agent 问答入口触发。

## 12. Lint 与修复流程

### 12.1 本地快速 Lint

应用本地执行确定性检查：

- 死链。
- 孤立页面。
- 缺失 frontmatter。
- `wiki/index.md` 与实际页面不一致。
- 空页面。
- 重复文件名。
- 路径大小写问题。
- 缺失资源文件。

### 12.2 Agent 深度 Lint

需要判断的问题交给 `wiki-lint` Skill：

- 重复主题。
- 弱交叉引用。
- 来源缺失。
- 页面结构不符合 `schema.md`。
- 内容过期。
- 跨页面矛盾。

### 12.3 修复路径

1. 用户运行 Lint。
2. 应用执行本地快速 Lint。
3. 用户可选择 Agent 深度 Lint。
4. 展示问题列表与修复计划。
5. 修复前创建 Git 检查点。
6. 自动修复可处理问题。
7. 高风险修改请求确认。
8. 修复成功后提交结果并刷新 UI。

### 12.4 高风险操作

以下操作必须确认：

- 删除页面。
- 覆盖页面。
- 删除原始资料。
- 替换原始资料。
- 批量重写。
- 冲突合并。

## 13. HTML / 卡片 / 报告导出流程

### 13.1 输出类型

首版支持：

- 单篇美化阅读页。
- 知识卡片。
- 思维导图或概念关系图。
- 项目级 HTML 报告。

### 13.2 用户路径

1. 用户在文章页或项目级入口选择导出。
2. 应用选择对应 `skills/html-*`。
3. Agent Skill 生成 HTML / 卡片 / 报告。
4. 输出保存到 `exports/html/`。
5. 应用内 iframe 预览。
6. 用户可打开导出文件所在位置。

### 13.3 边界

HTML 模板只影响输出样式，不影响 Wiki schema、Lint 规则或 Agent 行为。

## 14. Agent 面板流程

Agent 面板负责展示和管理本地 Agent CLI。

必须支持：

- 检测 `claude`、`codex`、`openclaw`、`hermes` 等 CLI。
- 显示安装状态和版本号。
- 设置默认 Agent。
- 显示安装引导。
- 实时展示 stdout / stderr。
- 展示任务状态。
- 支持取消任务。
- 支持后台运行。
- 支持系统通知。

应用不能静默安装 Agent 或静默执行安装命令。安装必须由用户明确确认。

## 15. 设置流程

设置视图至少包含：

- LLM Provider：OpenAI、Anthropic、Google、Ollama、Custom。
- API Key 管理。
- Agent 配置。
- 语言：中文 / English。
- 上下文窗口：4K 到 1M tokens。
- 外观：亮色 / 暗色。
- 启动行为。
- 后台任务关闭窗口行为。
- 更新检查。

API Key 必须存系统钥匙串或凭据管理器，不能明文写入项目文件。

## 16. 后台任务与通知

长任务必须进入后台任务系统，不能阻塞 UI。

后台任务包括：

- 导入解析。
- Wiki 编译。
- 图谱首次构建。
- Agent 深度 Lint。
- HTML / 报告生成。

关闭主窗口时默认最小化到系统托盘，任务继续运行。用户可以在设置中改为关闭时询问或终止任务。

系统通知用于：

- 任务完成。
- 任务失败。
- 需要用户确认。
- 冲突等待处理。

点击通知应打开结果页、错误日志或 Diff 确认页。

## 17. 关键状态文件与写入时机

| 文件 | 写入时机 | 说明 |
|---|---|---|
| `purpose.md` | 新建项目、模板初始化 | 知识库目标、关键问题、研究方向 |
| `schema.md` | 新建项目、模板初始化 | Wiki 结构规则、页面类型定义 |
| `wiki/index.md` | Wiki 编译、索引刷新 | 内容目录和 LLM 导航入口 |
| `wiki/overview.md` | Wiki 编译 | 全局摘要 |
| `wiki/log.md` | 编译、修复、重要操作 | 操作历史记录 |
| `.app/settings.json` | 设置变化 | 项目级应用设置 |
| `.app/agent-config.json` | Agent 配置变化 | Agent 检测与默认绑定 |
| `.app/graph-cache.json` | 图谱构建后 | 布局缓存 |
| `.app/import-conflicts.json` | 导入冲突、重命名、失败 | 导入问题记录 |
| `.app/bookmarks.json` | 收藏、星标变化 | 用户收藏状态 |
| `.app/chats/{id}.json` | Chat 会话变化 | 会话历史 |
| `.app/tasks/` | 后台任务变化 | 任务状态和日志 |
| `exports/html/` | 导出生成后 | HTML、卡片、报告 |

## 18. MVP 实现优先级

建议顺序：

1. 项目创建、打开、最近项目。
2. 普通文件夹初始化与基础目录结构。
3. 多格式导入与解析预览。
4. Git 初始化和检查点。
5. Wiki 编译的 Agent / BYOK 双路径骨架。
6. Wiki 文件树、Markdown 阅读和基础编辑。
7. 搜索、索引刷新和基础图谱。
8. Chat 问答与引用来源。
9. Lint 本地规则和 Agent 深度 Lint。
10. HTML / 卡片 / 项目报告导出。
11. 后台任务、托盘、通知。
12. 中英双语、主题、更新检查和多平台打包。

## 19. 禁止误解点

- 不要引入数据库保存项目内容。
- 不要把 API Key 写入项目文件。
- 不要让普通搜索自动调用模型。
- 不要把 Agent 作为唯一可用路径；BYOK API 需要支撑核心流程。
- 不要让 BYOK API 替代所有高级 Agent Skill 能力。
- 不要静默安装 Agent。
- 不要静默覆盖用户手动编辑。
- 不要在导入层做复杂 OCR / 视觉判断。
- 不要把 HTML 模板和 Wiki schema 混在一起。
- 不要把路由命名当成本文规定的接口。
