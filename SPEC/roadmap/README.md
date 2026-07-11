# LLM Wiki Desktop 实施路线图（Roadmap）

> 本目录是一份**落差审计 + 实施计划**，对照源是 `UI-Frontend-design/` 设计稿与 `SPEC/PRD.md` 功能清单；最初审计时点是 2026-06-21、历史审计分支为 `task1-backend-contracts`，当前状态以各 living roadmap 的后续更新与当前源码为准。
> 目的：让**别的对话**能照着这里的每个板块文件，逐块认领、修复或补齐功能，无需重新调研。

> **2026-07-11 架构迁移说明：**本目录是持续更新的路线图。旧的 `src-tauri/src/services/{import,search,lint,chat}_service.rs` 单文件证据现分别对应 `import_service/`、`search_service/`、`lint_service/`、`chat_service/` 目录模块；四个 service facade、Tauri command、DTO 与持久化契约保持不变，`chat_convenience_service.rs` 与 `wiki_index.rs` 仍是独立边界。前端 `AppShell` 只负责布局、pane 与全局壳层接线，项目级编排由 `WorkspaceController` 组合 `useImportWorkflow` / `useAgentWorkflow` / `useProviderWorkflow` 等 hook，视图 lazy dispatch 由 `WorkspaceRouter` 负责。`docs/audits/` 下的 dated audits 保留为当时的历史证据，不按当前路径回写。

---

## 如何使用本路线图（给后续对话）

1. **认领粒度 = 一个板块文件**。每个 `*.md` 自包含：现状摘要 → 区块/组件清单（带 `文件:行号` 和状态/优先级）→ PRD 功能落差 → 视觉 token 落差 → 交互/可访问性落差 → 建议实施顺序。
2. **挑一个 P0 项**开工。优先级约定：`P0` = 阻塞核心流程或违反 CLAUDE.md 硬边界；`P1` = MVP 期内应补齐；`P2` = 打磨。
3. **状态记号**：`✅已完成` / `🟡部分实现` / `❌缺失`。改动后请同步更新对应板块文件的表格与本 README 的总览表。
4. **硬边界不可越过**。开工前必读 `CLAUDE.md` 的"必读硬边界"：本地优先/无数据库、Git 检查点、API Key 只进系统凭据、路径安全+CJK、Agent 默认优先/BYOK 兜底、长任务可取消/可后台、i18n 含 Agent 生成内容语言偏好。
5. **每个任务完成后**按 `CLAUDE.md` 的"任务完成检查清单"跑 `npm run test` + `npm run lint`，并追加一条 `SPEC/progress.txt` 记录。
6. **后端契约**：历史 `task1-backend-contracts` 审计表明后端 service 骨架（Git/Secret/Agent/Task/Path 安全/PendingAction）整体已落地且测试覆盖较好；当前 Wiki create/rename/request-delete 命令与 PDF/Office 提取均已实现。剩余 P0 主要集中在**前端 UI 与设计稿对齐**，以及少数跨切面缺口（如 Import checkpoint 预操作时序、i18n prompt 注入）。

---

## 总览：各板块完成度

| 板块 | 完成度估计 | P0 数 | 文件 | 最关键缺口 |
|---|---|---|---|---|
| Graph（图谱） | ~65% | 1 | [graph.md](graph.md) | 画布内图例/信息卡悬浮层、6 类型筛选、SVG/PNG 导出 |
| Cross-cutting（跨切面） | 高（多数✅） | 2 | [cross-cutting.md](cross-cutting.md) | **Import checkpoint 仍在确认写入之后**、**i18n 生成内容语言偏好**；confirmation DTO 与 7 种 executable continuation 已厘清 |
| Lint | ~55% | 2 | [lint.md](lint.md) | **批量自动修复编排**、**severity 分级 + lint-ignore**、UI 摘要卡/分段控件 |
| Chat | ~55% | 1 | [chat.md](chat.md) | **流式输出**、消息 Markdown 渲染/avatar/citation 角标、Agent/BYOK 路由切换器 |
| Settings | ~55% | 1 | [settings.md](settings.md) | 多 section 内容错位/缺失、`formrow`/`seg`/`toggle` 样式族、更新检查 mock；Provider 行/掩码/状态与 Ollama 可达性已实现 |
| Shell + Dashboard + 启动页 | ~55% | 2 | [shell-dashboard.md](shell-dashboard.md) | **Dashboard 退化为状态表**（缺健康行/统计/时间线/快速操作）、**启动页未对齐三栏布局**；关闭拦截已实现，仍有托盘 i18n/进度等跨切面收尾 |
| Wiki | ~50% | 4 | [wiki.md](wiki.md) | **frontmatter 卡片化**、**Milkdown 工具条**、**新建/重命名/删除前端 UI 接线**、**冲突 Diff 对话框**；后端生命周期命令已注册，HTML 预览第三态仍缺 |
| Exports | ~45% | 1 | [exports.md](exports.md) | **新建导出对话框**、已生成列表表格化/失败重试、模板选择端到端参数传递 |
| Agent | ~40% | 1 | [agent.md](agent.md) | **运行 Agent 的 checkpoint/background 选项尚未接入实际任务**、核心操作四宫格、BYOK 卡片化、右面板 Agent 配置区、CLI 行/任务行样式族 |
| Import | 后端~75% / 前端~35% | 1 | [import.md](import.md) | **导入 checkpoint 发生在写入之后**、**"打开文件夹为项目"对话框**与预览 UI 落差；PDF/Office 文本提取已实现 |

> 完成度是子代理基于"真功能 vs 空壳 vs 缺失"的粗估，仅供排期参考，不是精确指标。

---

## 全局 P0 红线清单（MVP 前必须关闭）

按"违反硬边界 / 阻塞核心流程"程度排序。括号内为所属板块文件。

1. **i18n：Agent/LLM 生成内容未按用户语言偏好输出** `[cross-cutting]`
   chat/compile/export/lint 五个 prompt 构造点全是英文 system instruction，未读 `settings.language`；托盘菜单硬编码英文。违反 `CLAUDE.md` "i18n：Agent 生成内容按用户语言偏好输出"。
2. **Wiki：新建/重命名/删除页面 + Git 检查点** `[wiki]`
   后端 `create_wiki_page` / `rename_wiki_page` / `request_delete_wiki_page` 已实现并注册；剩余 P0 是把文件树的新建/重命名/删除 UI 接到现有 store/commands 与 `ConfirmationDialog`。
3. **Wiki：frontmatter 卡片化 + Milkdown 工具条** `[wiki]`
   阅读视图 frontmatter 是裸 `<pre>` YAML；编辑器真接入 Milkdown 但无格式工具条（加粗/斜体/标题/链接/代码/引用/撤销重做）。
4. **Wiki：编译冲突 Markdown Diff 对话框** `[wiki]`
   外部修改冲突仅 banner + reload，缺三路 diff（baseline / 外部 / agent）+ 三选项确认。后端 `FILE_HASH_MISMATCH` 需返回 baseline 文本。
5. **Agent：运行选项未接通任务语义** `[agent]`
   `RunAgentDialog` 与 Skill/route dispatch 已存在，但 `useAgentWorkflow` 未消费 `checkpoint` / `background`，两个开关目前不影响实际任务。
6. **Lint：批量自动修复编排 + severity 分级** `[lint]`
   设计稿顶栏"自动修复 (N)"主 CTA 无实现，只能逐条 Apply（每条各做一次 Git 检查点）；后端从不发 `error` 级（死链目前是 warning），severity 分级形同虚设；缺 lint-ignore 持久化。
7. **Import：Git checkpoint 时序违反预操作要求** `[import]`
   动作条与 checkbox 已实现，但 `confirm_import_preview` 先写入导入结果和 conflict JSON，之后才创建 checkpoint；checkpoint 失败不能阻止前置写盘。
8. **Settings：PRD-SET-005 更新检查是 mock** `[settings]`
    `window.confirm` 假弹窗，不真正查更新源。

---

## 建议实施顺序（按依赖与性价比）

### 第 1 波：硬边界红线（并行可做，互相独立）
- `cross-cutting` P0-1 i18n prompt 注入（5 个构造点 + 托盘菜单）
- `agent` P0 将 RunAgentDialog 的 checkpoint/background 选项接入真实任务语义
- `import` P0 把 checkpoint 移到任何 confirm import 写入之前
- `settings` P0 更新检查去 mock（小而独立）

### 第 2 波：Wiki 生命周期（串行，互相依赖）
- `wiki` P0 frontmatter 卡片化 + `.prose` token 迁移（低成本、立竿见影）
- `wiki` P0 Milkdown 工具条
- `wiki` P0 新建/重命名/删除前端 UI 接线（复用已注册后端命令、Git 检查点与 wikilink 同步）
- `wiki` P0 冲突 Diff 对话框（依赖后端 `FILE_HASH_MISMATCH` 返回 baseline）

### 第 3 波：核心功能补全
- `agent` 对话框与 Skill/route dispatch 已存在；剩余 P0 是 checkpoint/background 选项语义
- `lint` P0 批量修复编排 + severity 分级 + lint-ignore
- `import` P0 前端 UI 重构对齐设计稿

### 第 4 波：体验打磨
- `chat` 流式输出 + 消息渲染富化
- `graph` 画布悬浮层（图例/信息卡）+ 6 类型筛选 + SVG/PNG 导出
- `exports` 新建导出对话框 + 失败重试
- `shell-dashboard` Dashboard 信息密度 + 启动页三栏布局
- `settings` 样式族（`apikey-row`/`formrow`/`seg`/`toggle`）+ section 内容归位
- `wiki` P1 HTML 预览第三态（依赖 `skills/html-*`）

---

## 与其他 SPEC 文档的衔接

| 想了解 | 去看 |
|---|---|
| 产品需求条目编号（PRD-XXX）的含义 | `SPEC/PRD.md` |
| 视图/数据流/确认规则 | `SPEC/APP_flow.md` |
| 后端 service 架构与命令清单 | `SPEC/BACKEND_STRUCTURE.md` |
| 前端设计系统（字号/间距/组件高度/section 标签）权威 | `UI-Frontend-design/assets/app.css` + `CLAUDE.md` "前端设计对齐原则" |
| 已踩过的坑 | `SPEC/gotchas.txt` |
| 历史进度记录 | `SPEC/progress.txt` |

> 本路线图只描述"落差与计划"，不复制上述文档内容。实现时仍以原文档为准；若发现路线图与原文档冲突，**以原文档为准并回改路线图**。
