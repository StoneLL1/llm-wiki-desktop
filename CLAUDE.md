# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 仓库现状

- SPEC文件夹内文档：`PRD.md`（产品需求）、`SPEC.md`（规格）、`TECH_STACK.md`（技术栈与架构边界）、`BACKEND_STRUCTURE.md`（Rust 后端架构）、`APP_flow.md`（视图/数据流/确认规则）、`FRONTEND_GUIDELINES.md`（前端设计系统）、`DESIGN.md`（视觉主题）。
- 样本数据：`wiki/wiki/` 是一个**真实的 Obsidian 兼容知识库**（约数百页面），用于验证真实规模、Obsidian 兼容性和图谱性能。**不要把它当作应用源码**，也不要在它里面实验代码。

动手写代码前，先按顺序读：`PRD.md` → `SPEC.md` → `APP_flow.md` → `TECH_STACK.md` → `BACKEND_STRUCTURE.md` → `FRONTEND_GUIDELINES.md`。

## 必读硬边界（违反即跑偏）

这些是文档明确锁定的约束，不要用"更优雅"的实现推翻，除非用户明确批准：

- **本地优先 / 无数据库**：项目内容只用 Markdown + JSON + 本地文件。不引入数据库。可在内存建临时索引和缓存，持久化必须落到项目文件夹。
- **文件透明**：Wiki 页面是普通 Markdown，用户可能在应用内、Obsidian 或外部编辑器修改。兼容 Karpathy LLM Wiki / `nashsu/llm_wiki` / Obsidian 目录约定。
- **Git 是数据安全边界，不是可选增强**：删除、覆盖、批量替换、Agent 自动修复、重大重新编译、原始资料替换/删除——**操作前必须创建 Git 检查点**，成功后提交。高风险（删除/覆盖/冲突）操作必须经用户确认。
- **API Key 只进系统凭据管理**（Windows Credential Manager / macOS Keychain / Linux Secret Service）。**绝不**写入项目文件、配置 JSON 或日志。UI 只显示"已配置"，不回显完整密钥。
- **路径安全**：内部路径统一用正斜杠；任何来自 UI 的路径都要校验是否在当前项目范围内，禁止绕过项目边界写文件。必须正确处理 Unicode / CJK 文件名，跨平台测试 Windows/macOS/Linux 风格路径。
- **Agent 默认优先，BYOK 兜底**：Source 已经形成后，配置可用 Agent CLI 时优先走 CLI；未配置 Agent 时 BYOK API（OpenAI/Anthropic/Google/Ollama/Custom）必须能跑通 AI 整理、Wiki 编译和 Chat。BYOK 不参与 Import 解析或恢复。**不静默安装 Agent**，安装需用户明确确认。
- **长任务必须可取消、可后台运行、可报告进度**，关闭主窗口默认最小化到托盘并继续。

## 架构全景

三层知识库模型（来自 Karpathy LLM Wiki 模式）：

```
Raw Sources (不可变原始资料)  ->  Extracted Markdown  ->  Wiki (结构化页面)  ->  Graph / Chat / HTML Reports
```

项目文件夹结构（`SPEC.md` §5）：

```
project-root/
├── purpose.md / schema.md      # 知识库目标、Wiki 结构规则
├── raw/{sources,extracted,assets}/
├── wiki/{index.md,log.md,overview.md,entities,concepts,sources,queries,synthesis,comparisons}/
├── exports/html/
├── skills/                     # 项目级 SKILL.md
└── .app/                       # 应用状态 JSON（bookmarks, chats, settings, graph-cache, import-conflicts, tasks/）
```

代码分层（`TECH_STACK.md` §4，`BACKEND_STRUCTURE.md` §2-3）：

```
React UI  ->  Frontend State (Zustand stores)  ->  Tauri IPC Commands (薄层，结构化 DTO)  ->  Rust Backend Services  ->  本地项目文件夹
```

- **React UI 层**只做导航、展示、表单、IPC 调用。**不直接**做 Git 检查点、Agent 进程 spawn、API Key 存储、批量文件迁移、跨平台路径规范化核心逻辑——这些都走 Tauri IPC 调后端 service。
- **IPC 命令层保持薄**，业务逻辑在 Rust service 层。所有输入输出用结构化 DTO，不用临时字符串拼复杂状态。统一错误模型 `BackendError`；高风险操作统一返回 `PendingAction` 由用户确认。
- **Rust 后端**是本地能力核心：`ProjectService`、`FileStore`、`ImportService`、`ExtractionService`、`GitService`、`AgentService`、`LlmService`、`SecretService`、`SearchService`、`GraphService`、`LintService`、`ExportService`、`TaskService`、`SettingsService`。所有项目文件读写都经 `ProjectContext` 路径安全校验。

### 技术栈

桌面 Tauri v2（Rust）｜前端 React 19 + TS + Vite｜UI shadcn/ui + Tailwind v4｜编辑器 Milkdown（ProseMirror WYSIWYG）｜图谱 sigma.js + graphology + ForceAtlas2 + Louvain｜图标 Lucide｜状态 Zustand｜i18n react-i18next（zh-CN / en）｜Markdown 渲染 remark-gfm + remark-math + rehype-katex + rehype-highlight｜URL 正文提取 Readability.js。初始化命令见 `SPEC.md` §15。

### 关键行为约定

- **搜索**：普通全局搜索只做关键词/标签/类型/来源过滤，**不自动调用模型**；自然语言问答走 Chat/Agent 入口。
- **导入层必须形成可读 Source**：成功项同时保留 `raw/` 不可变证据并写入 `wiki/sources/`；OCR / ASR 在导入阶段按内容缺口、经用户主动授权运行。图片视觉理解不在首版范围，导入不得自动触发编译。完整规则见 `docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`。
- **Lint 双层**：本地快速 Lint（死链、孤立页、缺 frontmatter、index.md 漂移等确定性规则）+ Agent 深度 Lint（`wiki-lint` Skill）。自动修复前创建 Git 检查点。
- **HTML/卡片/报告**全部通过 `skills/html-*` 驱动，模板只影响输出样式，不改 Wiki schema / Lint 规则 / Agent 行为。
- **图谱首版**：每页一节点，边统一表示"相关"，不实现复杂关系类型和证据系统。布局缓存到 `.app/graph-cache.json`。
- **i18n**：Agent 生成内容按用户语言偏好输出。

## 前端设计对齐原则

**权威设计源是整个 `UI-Frontend-design/` 文件夹**（不要把它当应用源码，不修改、不纳入提交）。做任何 UI 工作前，必须先参考其中的设计文件，对齐应覆盖以下维度：

Import / Source 例外：产品流程、信息架构、状态、文案、登录、OCR / ASR、媒体操作和 AI 整理只以 `docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md` 为准。旧 `UI-Frontend-design/import*.html` 只保留不冲突的视觉密度与结构参考，不得恢复“导入后编译”、Git 开关或编译期 OCR。

1. **页面布局与组件结构** — `UI-Frontend-design/dashboard.html` 定义了完整页面结构：左侧边栏三区段（主视图/工作流/最近页面 + Agent 状态脚）、右侧面板（项目信息：路径/索引状态/执行路径/背景任务）、顶栏、状态栏。页面级组件拆分、DOM 层级、aria 角色以设计 HTML 为准。
2. **CSS token 与视觉密度** — `UI-Frontend-design/assets/app.css` 是样式权威（token、字号、间距、组件尺寸、颜色）。当前应用用 Tailwind v4 + `src/styles.css` 实现：
   - **字号用绝对 px**：UI 正文 13px，次要 12px，muted/mono 11px，小标签 10.5px，阅读区 14–15px，标题 16/18/22/28px。写 `text-[13px]` 而非 `text-sm`。
   - **组件高度**：顶栏 48px、主区头 52px、右面板头 52px、状态栏 28px、导航项 30px（小号 26px）、面板头 44px。
   - **section 标签**：10.5px、大写、`letter-spacing: 0.08em`、muted 色。
   - **token 单一来源**：颜色/圆角/字体/间距 token 只在 `src/styles.css` `:root` 定义（含 `--sp-*` 间距、`--text-inverse`）；组件只引用 token。
3. **字体** — UI: Inter，代码/路径: JetBrains Mono，阅读: Source Serif Pro（`--font-ui/display/mono`）。通过 @fontsource 打包，不走 CDN。
4. **交互与 JS 行为** — 设计 HTML 中的交互（侧栏导航高亮 `aria-current`、语言切换、搜索快捷键提示）应反映到 React 组件实现。
5. **图标** — 使用 Lucide React，尺寸与设计一致（导航图标 16px，文件图标 14px 等）。

未推进到的 feature 视图可暂不做，但已实现的 shell/面板必须逐项对齐上述维度。

## 任务完成检查清单（按改动风险执行）

应用脚手架建好后，按改动规模选择检查，不要把每次编辑都当作发布门禁：

1. **纯文档工作**：只修改 Markdown / 文档、研究记录、计划、审查报告、`progress.txt` 或 `gotchas.txt` 时，不需要运行 npm 检查；如果同时修改了可执行配置或代码，则按下面规则处理。
2. **一般开发过程**：小范围、局部代码改动运行 `npm run check:quick`。它覆盖 lint、前端生产构建与 import 路径解析、`console.log` 扫描和 Rust core compile。
3. **较大或高风险改动的最终收尾**：功能开发、跨层改动、架构或依赖/构建变更、广泛重构、面向发布的工作，以及涉及文件写入、Git 安全、密钥、IPC、并发、后台任务等关键路径的代码，最终运行完整 `npm run check`。用户明确要求完整门禁时也必须运行。
4. 必需检查因本次改动失败时，修复后重跑同一级别门禁；需要完整门禁时，修复后从头运行 `npm run check`。
5. 如果项目尚未初始化或所需脚本不存在，明确报告缺失的文件或脚本，不要假装通过。



## 主 Agent 收尾：按风险审查

只有涉及可执行代码改动时才需要代码审查，审查力度与风险相称：

- **纯文档工作**：Markdown / 文档、研究、计划、审查报告、进度和 gotchas 记录不启动审查子代理。
- **小范围局部代码改动**：对改动代码做聚焦审查；改动直接、低风险时，审查子代理可选。
- **功能、重要修复、跨层或高风险代码改动**：启动两个审查子代理：
  - **子代理 A（共享上下文）**：理解本次设计意图 → 逻辑审查 → 与设计意图的一致性检查。
  - **子代理 B（全新上下文，零偏见）**：以新鲜视角发现盲点、隐性 bug、被忽略的边界。

合并适用的审查结果，修复有效问题，再按上一节的改动分级运行对应检查后交付。需要双审查但环境没有子代理时，手工完成等价审查并在最终报告说明。

## 持续记录机制（强制）

- **`progress.txt`**：每次完成重要进度节点（一个功能落地、一次架构决策、一个里程碑、一次重要修复）后，必须**立即**追加一条记录，时间倒序（最新在上）。格式：`[YYYY-MM-DD] 模块/任务 — 完成内容摘要 — 关键决策/遗留问题`。不要覆盖历史，只追加。
- **`gotchas.txt`**：当某个错误反复出现、或具有隐蔽性（踩一次坑一次），必须单独记录一条。格式：`现象 — 根因 — 规避做法`。下次遇到同类问题时先查这里。
- 两个文档在SPEC文件夹内

这两条规则同样写入 `AGENTS.md`，对子代理同样生效。

## 工作纪律

- 动手前先读文档，不要在没有用户确认时大规模重写产品决策。
- 样本库 `wiki/wiki/` 是验证数据，不是测试场；需要测试导入/编译时，复制一份另开目录。
- 实现 `raw/sources/` 替换/删除、批量迁移、Agent 自动修复时，必须接入 Git 检查点。
- 实现任何密钥相关代码，必须走系统凭据管理。
- 实现任何跨平台路径逻辑，必须测 Windows/macOS/Linux 风格路径和 CJK 文件名。
