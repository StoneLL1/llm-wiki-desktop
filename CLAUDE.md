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
- **Agent 默认优先，BYOK 兜底**：配置可用 Agent CLI 时优先走 CLI；未配置 Agent 时 BYOK API（OpenAI/Anthropic/Google/Ollama/Custom）必须能跑通核心流程。**不静默安装 Agent**，安装需用户明确确认。
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
- **导入层只无损保留**原文件、提取文本、图片、来源元数据。OCR / 视觉理解交给编译 Agent/Skill，不在导入层判断图片价值。
- **Lint 双层**：本地快速 Lint（死链、孤立页、缺 frontmatter、index.md 漂移等确定性规则）+ Agent 深度 Lint（`wiki-lint` Skill）。自动修复前创建 Git 检查点。
- **HTML/卡片/报告**全部通过 `skills/html-*` 驱动，模板只影响输出样式，不改 Wiki schema / Lint 规则 / Agent 行为。
- **图谱首版**：每页一节点，边统一表示"相关"，不实现复杂关系类型和证据系统。布局缓存到 `.app/graph-cache.json`。
- **i18n**：Agent 生成内容按用户语言偏好输出。

## 任务完成检查清单（每个功能完成后强制执行）

应用脚手架建好（有 `package.json`、lint、test 脚本）后，**每个任务完成后自动运行以下检查**：

1. `npm run test` — 确保全部通过
2. `npm run lint` — 检查代码风格
3. 确认无 `console.log` 残留（前端调试日志）
4. 验证所有 `import` 路径存在
5. 任一检查失败 → 修复后重新运行**所有**检查（不要只补跑失败的那项）



## 主 Agent 收尾：双子代理审查

主 Agent 完成工作后，启动**两个审查子代理**并行运行：

- **子代理 A（共享上下文）**：理解本次设计意图 → 逻辑审查 → 与设计意图的一致性检查。
- **子代理 B（全新上下文，零偏见）**：以新鲜视角发现盲点、隐性 bug、被忽略的边界。

合并两个审查结果 → 修复所有问题 → 重新跑上面的检查清单 → 交付。

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
