# 00 Codebase Audit

> **历史代码审计快照：** 本文描述 2026-07-04 的源码和规格状态，其中 `ProjectStartView`、项目选择页、二元目录识别等内容是当时事实，不是当前目标合同。首次使用与打开已有知识库以 [2026-07-30 权威规范](../superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md) 为准，living 路线图以 [`../../SPEC/roadmap/README.md`](../../SPEC/roadmap/README.md) 为准。

审计日期：2026-07-04  
审计范围：`D:\Users\Aletta\Desktop\Works\llm-wiki-desktop`  
审计方式：只读阅读 SPEC、设计稿、前端源码、Tauri/Rust 后端、测试与配置。`rg.exe` 在本机返回 access denied，本轮改用 PowerShell 目录枚举与文件读取。`node_modules/`、`dist/`、`.git/` 排除；`src-tauri/target/` 为 Rust 构建产物，报告中只摘要。样例 `wiki/wiki/raw/**` 是验证数据，文件量大，报告按目录与代表性文件摘要。

## 项目概况

LLM Wiki Desktop 是一个 local-first Tauri v2 桌面应用，用 React 19 + TypeScript + Vite 构建渲染层，用 Rust/Tauri 承担文件、Git、Agent/LLM、任务、导入、导出、图谱、Lint、Chat 等本地能力。项目内容以 Markdown + JSON + 本地文件为源，不使用数据库；项目目录约定为 `raw/`、`wiki/`、`.app/`、`exports/`、`skills/`。

设计目标非常明确：界面应贴近 Codex desktop，紧凑三栏工作台，左侧导航，中间主工作区，右侧上下文面板，底部状态栏；`UI-Frontend-design/` 是 UI 权威设计参考，不是应用源码。

当前实现已经具备主要骨架：项目启动页、应用 shell、Wiki 浏览/编辑/HTML 预览、图谱、Chat、Lint、导入、导出、Agent、设置、任务日志与通知。后续 15 个修复大多属于 UI shell 能力补齐、搜索/检索质量、图谱缓存恢复、历史持久化和项目管理交互优化。

## 目录结构树

```text
.
├─ .agents/
├─ .claude/
│  ├─ launch.json
│  └─ settings.local.json
├─ .codex/
├─ docs/
│  ├─ architecture/
│  │  └─ parser-adapters.md
│  ├─ fixes/
│  │  └─ 00-codebase-audit.md
│  ├─ qa/
│  │  └─ mvp-acceptance.md
│  └─ superpowers/
│     ├─ plans/
│     │  ├─ 2026-06-21-spec-audit-remediation.md
│     │  ├─ 2026-06-22-import-extract-compile-repair.md
│     │  ├─ 2026-06-23-import-chat-reliability-fix.md
│     │  └─ 2026-06-24-ui-polish.md
│     └─ specs/
│        ├─ 2026-06-22-import-extract-compile-repair-design.md
│        ├─ 2026-06-23-import-chat-reliability-design.md
│        ├─ 2026-06-24-sources-as-extracted-originals-design.md
│        └─ 2026-06-24-ui-polish-design.md
├─ SPEC/
│  ├─ PRD.md
│  ├─ SPEC.md
│  ├─ APP_flow.md
│  ├─ TECH_STACK.md
│  ├─ BACKEND_STRUCTURE.md
│  ├─ FRONTEND_GUIDELINES.md
│  ├─ DESIGN.md
│  ├─ progress.txt
│  ├─ gotchas.txt
│  ├─ plans/
│  │  ├─ agent.md
│  │  ├─ chat-be.md
│  │  ├─ chat-fe.md
│  │  ├─ cross-cutting-be.md
│  │  ├─ cross-cutting-fe.md
│  │  ├─ exports.md
│  │  ├─ graph.md
│  │  ├─ import-be.md
│  │  ├─ import-fe.md
│  │  ├─ lint-be.md
│  │  ├─ lint-fe.md
│  │  ├─ settings.md
│  │  ├─ shell-dashboard.md
│  │  ├─ wiki-be.md
│  │  └─ wiki-fe.md
│  └─ roadmap/
│     ├─ README.md
│     ├─ agent.md
│     ├─ chat.md
│     ├─ cross-cutting.md
│     ├─ exports.md
│     ├─ graph.md
│     ├─ import.md
│     ├─ lint.md
│     ├─ loop-prompts.md
│     ├─ settings.md
│     ├─ shell-dashboard.md
│     └─ wiki.md
├─ src/
│  ├─ main.tsx
│  ├─ styles.css
│  ├─ app/
│  │  ├─ App.tsx
│  │  └─ App.test.tsx
│  ├─ components/
│  │  ├─ app/
│  │  │  ├─ AppShell.tsx
│  │  │  ├─ BottomStatusBar.tsx
│  │  │  ├─ CompileConflictDialog.tsx
│  │  │  ├─ ConfirmationDialog.tsx
│  │  │  ├─ LeftSidebar.tsx
│  │  │  ├─ RightContextPanel.tsx
│  │  │  ├─ RightPanelHeader.tsx
│  │  │  ├─ TaskActivityButton.tsx
│  │  │  ├─ TaskLogDrawer.tsx
│  │  │  ├─ Toaster.tsx
│  │  │  ├─ TopBar.tsx
│  │  │  └─ shellNavigation.ts
│  │  └─ ui/
│  │     └─ button.tsx
│  ├─ features/
│  │  ├─ agent/
│  │  ├─ chat/
│  │  ├─ dashboard/
│  │  ├─ exports/
│  │  ├─ graph/
│  │  ├─ import/
│  │  ├─ lint/
│  │  ├─ project/
│  │  ├─ settings/
│  │  └─ wiki/
│  ├─ hooks/
│  ├─ i18n/
│  │  └─ locales/
│  │     ├─ en.json
│  │     └─ zh-CN.json
│  ├─ lib/
│  ├─ services/
│  ├─ stores/
│  ├─ test/
│  └─ types/
├─ src-tauri/
│  ├─ Cargo.toml
│  ├─ Cargo.lock
│  ├─ tauri.conf.json
│  ├─ build.rs
│  ├─ capabilities/
│  │  └─ main.json
│  ├─ src/
│  │  ├─ main.rs
│  │  ├─ lib.rs
│  │  ├─ app_state.rs
│  │  ├─ commands/
│  │  ├─ errors/
│  │  ├─ models/
│  │  ├─ services/
│  │  ├─ tasks/
│  │  └─ utils/
│  ├─ templates/
│  │  ├─ projects/
│  │  └─ skills/
│  └─ tests/
├─ UI-Frontend-design/
│  ├─ dashboard.html
│  ├─ wiki.html
│  ├─ chat.html
│  ├─ graph.html
│  ├─ lint.html
│  ├─ exports.html
│  ├─ import.html
│  ├─ agent.html
│  ├─ settings.html
│  ├─ launch.html
│  └─ assets/
│     ├─ app.css
│     └─ app.js
├─ wiki/
│  ├─ .app/
│  │  ├─ agent-config.json
│  │  ├─ graph-cache.json
│  │  ├─ settings.json
│  │  └─ tasks/
│  ├─ wiki/
│  │  ├─ .obsidian/
│  │  ├─ comparisons/
│  │  ├─ concepts/
│  │  ├─ entities/
│  │  ├─ queries/
│  │  ├─ raw/
│  │  ├─ scripts/
│  │  ├─ _archive/
│  │  ├─ index.md
│  │  ├─ log.md
│  │  └─ SCHEMA.md
│  └─ __MACOSX/
├─ AGENTS.md
├─ CLAUDE.md
├─ IMPLEMENTATION_PLAN.md
├─ index.html
├─ package.json
├─ package-lock.json
├─ eslint.config.js
├─ tsconfig.json
├─ tsconfig.app.json
├─ tsconfig.node.json
└─ vite.config.ts
```

## 主要目录职责

- `SPEC/`：产品、流程、技术栈、后端结构、前端规范、设计约束与路线图，是实现决策依据。
- `UI-Frontend-design/`：HTML/CSS/JS 高保真设计规格，尤其 `dashboard.html`、`assets/app.css`、`assets/app.js`，不可当作 app 源码修改。
- `src/`：React 渲染层。`components/app/` 是桌面 shell，`features/` 是业务视图，`stores/` 是 Zustand 状态，`types/` 是前端 DTO。
- `src-tauri/src/commands/`：Tauri IPC 命令层，薄封装，负责项目上下文解析与调度服务。
- `src-tauri/src/services/`：后端业务服务，处理本地文件、Git、检索、图谱、Chat、Lint、导出、项目初始化、设置和 OS secrets。
- `src-tauri/src/models/`：Rust DTO/序列化契约。
- `src-tauri/src/tasks/`：后台任务、取消、事件、日志与任务持久化。
- `src-tauri/templates/`：新项目模板与 Agent/HTML/Lint skill 模板。
- `wiki/`：样例/验证 wiki 数据，不是应用源码。`wiki/.app/` 是样例项目 app 状态；`wiki/wiki/raw/**` 是原始资料样本。

## 技术栈清单

### 前端运行依赖（package-lock 锁定版本）

- `react` 19.2.7, `react-dom` 19.2.7
- `vite` 8.0.16, `@vitejs/plugin-react` 6.0.2, `typescript` 5.9.3
- `@tauri-apps/api` 2.9.1, `@tauri-apps/plugin-dialog` 2.6.0, `@tauri-apps/plugin-notification` 2.3.3
- `tailwindcss` 4.3.1, `@tailwindcss/vite` 4.3.1, `tailwind-merge` 2.6.1
- `zustand` 5.0.14
- `react-router-dom` 7.18.0（当前主应用未使用 URL 路由，使用 Zustand view state）
- `lucide-react` 0.468.0
- `i18next` 25.10.10, `react-i18next` 15.7.4
- `@fontsource/inter` 5.2.8, `@fontsource/jetbrains-mono` 5.2.8, `@fontsource/source-serif-4` 5.2.9
- `@milkdown/kit` 7.21.2, `@milkdown/react` 7.21.2, `@milkdown/preset-gfm` 7.21.2, `@milkdown/plugin-listener` 7.21.2, `@milkdown/theme-nord` 7.21.2
- `react-markdown` 10.1.0, `remark-gfm` 4.0.1, `remark-math` 6.0.0, `rehype-katex` 7.0.1, `rehype-highlight` 7.0.2
- `sigma` 3.0.3, `graphology` 0.26.0, `graphology-layout-forceatlas2` 0.10.1, `graphology-communities-louvain` 1.5.3
- `@mozilla/readability` 0.6.0
- `@radix-ui/react-slot` 1.3.0, `class-variance-authority` 0.7.1, `clsx` 2.1.1

### 前端测试/质量工具

- `vitest` 4.1.9, `jsdom` 25.0.1
- `@testing-library/react` 16.3.2, `@testing-library/jest-dom` 6.9.1
- `eslint` 9.39.4, `@eslint/js` 9.39.4, `typescript-eslint` 8.61.1, `globals` 15.15.0
- `@types/react` 19.2.17, `@types/react-dom` 19.2.3, `@types/node` 22.19.21
- `esbuild` 0.28.1

### Rust/Tauri 后端依赖（Cargo.toml 约束）

- Tauri v2：`tauri` 2（tray-icon）、`tauri-build` 2、`tauri-plugin-dialog` 2、`tauri-plugin-notification` 2
- Async/HTTP：`tokio` 1、`reqwest` 0.12（json/rustls-tls/stream）、`futures-util` 0.3
- 序列化与数据：`serde` 1、`serde_json` 1、`csv` 1、`quick-xml` 0.36、`zip` 2
- 文件/解析/安全：`pdf-extract` 0.10、`sha2` 0.10、`url` 2、`uuid` 1、`chrono` 0.4、`thiserror` 2
- Secrets：`keyring` 3（apple-native/windows-native/sync-secret-service）

### 构建配置

- `package.json` 脚本：`dev` = `vite`；`build` = `tsc -b && vite build`；`test` = `vitest run`；`lint` = `eslint . --max-warnings=0`；`tauri` = `tauri`
- Tauri dev server：`http://localhost:1420`
- Tauri 主窗口：1280x820，最小 1120x720，`dragDropEnabled: true`
- ESLint 忽略：`dist`、`node_modules`、`src-tauri/target`、`UI-Frontend-design`

## 架构分析

### 进程/层次划分

本项目不是 Electron。对应关系是：

```text
Tauri WebView / React renderer
  -> @tauri-apps/api invoke / event listen
  -> Rust command layer (src-tauri/src/commands)
  -> typed DTOs (src-tauri/src/models)
  -> services (src-tauri/src/services)
  -> local Markdown/JSON/files + Git + Agent CLI + BYOK LLM + OS keyring
```

`src-tauri/src/lib.rs` 注册 Tauri 插件、托盘、窗口关闭行为和所有 IPC 命令。`src-tauri/src/app_state.rs` 持有 `ProjectRegistry` 与服务单例，`ProjectRegistry` 把前端传入的 `projectId + rootPath` 解析为受信任的 `ProjectContext`，避免 UI 任意访问文件系统。

### 前端组件层次

```text
main.tsx
└─ App.tsx
   ├─ ProjectStartView                # 未打开项目时的启动/项目选择页
   └─ AppShell                        # 打开项目后的桌面 shell
      ├─ TopBar
      ├─ LeftSidebar
      ├─ 主工作区 activeView
      │  ├─ DashboardView
      │  ├─ WikiView
      │  ├─ ChatView
      │  ├─ GraphView
      │  ├─ LintView
      │  ├─ ExportsView
      │  ├─ ImportView
      │  ├─ AgentView
      │  └─ SettingsView
      ├─ RightContextPanel
      ├─ BottomStatusBar
      ├─ TaskLogDrawer
      └─ Confirmation / conflict / run-agent dialogs
```

`AppShell.tsx` 是当前耦合最高的前端枢纽：它连接 project/navigation/task/settings/wiki/import/export/agent/chat/lint stores，并承接大量 Tauri invoke 流程。未来做分割线、收起、面板状态持久化时应优先从这里和 `navigationStore.ts` 切入。

### 状态与持久化

- 前端状态：Zustand stores。
  - `projectStore.ts`：当前项目、最近项目、打开/创建项目、pending action。
  - `navigationStore.ts`：`activeView`、`rightPanelOpen`。当前没有左栏收起和分割线尺寸状态。
  - `wikiStore.ts`：wiki tree、当前页、编辑草稿、冲突、最近页面、bookmark。
  - `chatStore.ts`：会话、当前会话、发送任务、流式消息。
  - `graphStore.ts`：图谱数据、筛选、颜色模式、选中节点、布局操作回调。
  - `lintStore.ts`：本地报告、深度报告、fix 状态、忽略项、安全偏好。本地报告未持久化。
  - `exportStore.ts`：导出记录、预览 HTML、运行中的导出任务。
  - `taskStore.ts`：任务列表、日志、抽屉状态。
  - `settingsStore.ts`：语言、主题、字体、密度、LLM/Agent 设置，并把主题/字体写入 DOM CSS 变量或 dataset。
- 后端持久化：
  - `.app/chats/*.json`：Chat 会话。
  - `.app/tasks/*.json`：任务与日志。
  - `.app/graph-cache.json`：图谱缓存与布局。
  - `.app/bookmarks.json`：wiki 页面 bookmark。
  - `.app/exports.json`：导出历史。
  - `.app/lint-ignore.json`：Lint 忽略规则。
  - `.app/lint-reports/<task_id>.json`：深度 Lint 报告。
  - 项目外全局 config：recent projects。
  - OS keyring：LLM provider secrets。

### 路由结构

虽然安装了 `react-router-dom`，当前应用没有基于 URL 的页面路由。主视图由 `navigationStore.activeView` 驱动，定义在 `src/components/app/shellNavigation.ts`：

- 主视图：`dashboard`、`wiki`、`chat`、`graph`
- 工作流：`agent`、`import`、`lint`、`exports`
- 设置视图由 topbar/settings action 进入

### 后端服务图

```text
commands
  agent_commands       -> AgentService
  chat_commands        -> ChatService + SearchService + AgentService/LlmService + TaskService
  compile_commands     -> CompileService + Import/Git/File helpers
  export_commands      -> ExportService + AgentService/LlmService + TaskService
  graph_commands       -> GraphService + SearchService + TaskService
  import_commands      -> ImportService + ExtractionService + Git/File/Task
  lint_commands        -> LintService + AgentService/LlmService + Git + TaskService
  project_commands     -> ProjectService + GitService + ConfirmationRegistry
  settings_commands    -> SettingsService + SecretService
  task_commands        -> TaskService
  wiki/search/file/git -> corresponding services
```

## 15 个修复条目关键文件地图

### 1. 添加可以自由拖动分割线的功能

- `src/components/app/AppShell.tsx`：主 shell DOM，当前左/中/右布局由 CSS grid/flex 固定。
- `src/styles.css`：`.app-shell`、`.app-sidebar`、`.workspace`、`.right-panel`、`.wiki-view-layout`、`.exports-view-layout`、`.lint-view-layout` 等尺寸定义。
- `src/stores/navigationStore.ts`：目前只有右栏开关，适合加入 sidebar/main/right 宽度与持久化状态。
- `src/components/app/LeftSidebar.tsx`、`src/components/app/RightContextPanel.tsx`：分割线两侧目标面板。
- `UI-Frontend-design/dashboard.html`、`UI-Frontend-design/assets/app.css`：权威布局密度与面板尺寸参考。

### 2. 左侧的主视图也可收起

- `src/components/app/LeftSidebar.tsx`：左栏渲染入口，目前仅 CSS 响应式隐藏，没有桌面手动 collapse。
- `src/components/app/AppShell.tsx`：需要把 collapse 状态传给 shell class/layout。
- `src/stores/navigationStore.ts`：新增 `leftPanelOpen` 或 `sidebarCollapsed`。
- `src/components/app/TopBar.tsx`：可放置 sidebar toggle。
- `src/styles.css`：新增 collapsed 宽度、icon-only 状态、移动端兼容。

### 3. 图谱美化

- `src/features/graph/GraphView.tsx`：sigma 渲染、节点/边样式、布局、选中、高亮、WebGL fallback。
- `src/features/graph/GraphControls.tsx`、`GraphCanvasControls.tsx`、`GraphInfo.tsx`、`GraphLegend.tsx`、`GraphInspector.tsx`：控制、图例、右侧检查器。
- `src/features/graph/legendEntries.ts`、`graphExport.ts`、`graphNeighbors.ts`：图例、导出和邻居逻辑。
- `src/types/graph.ts`：节点/边 DTO 与颜色常量。
- `src/stores/graphStore.ts`：颜色模式、筛选、选中状态。
- `src/styles.css`：`.graph-*` 样式。
- `src-tauri/src/services/graph_service.rs`、`src-tauri/src/commands/graph_commands.rs`：数据生成、缓存、布局保存。

### 4. 概览界面丰富板块

- `src/features/dashboard/DashboardView.tsx`：当前有健康、统计、类型分布、最近活动、快捷入口。
- `src/stores/projectStore.ts`、`src/hooks/useProjectStatus.ts`、`src/features/wiki/wikiStore.ts`、`src/stores/taskStore.ts`：当前 Dashboard 数据来源。
- `src-tauri/src/services/project_service.rs`、`src-tauri/src/commands/project_commands.rs`、`src-tauri/src/models/project.rs`：项目摘要、健康状态、页面/source/task 数。
- `src/components/app/RightContextPanel.tsx`：默认项目信息与任务摘要，可复用或扩展。
- `src/i18n/locales/en.json`、`src/i18n/locales/zh-CN.json`：新增面板文案。

### 5. 添加配色自定义，可以修改主题（包括 Markdown 渲染）

- `src/features/settings/AppearanceSettings.tsx`：当前仅 light/dark/auto 三个预览按钮。
- `src/stores/settingsStore.ts`、`src/types/settings.ts`：settings schema，当前包含 theme/density/uiFont/readingFont/codeFont。
- `src-tauri/src/services/settings_service.rs`、`src-tauri/src/commands/settings_commands.rs`、`src-tauri/src/models/settings.rs`：设置读写和后端 DTO。
- `src/styles.css`：`:root`、`:root[data-theme="dark"]`、字体变量、`.wiki-prose`、`.chat-prose`、`.html-preview`。
- `src/features/wiki/MarkdownReader.tsx`、`src/features/chat/MessageContent.tsx`：Markdown 渲染样式落点。
- `UI-Frontend-design/assets/app.css`：token 权威来源。

### 6. BUG：图谱有时候重新打开会消失

- `src/stores/graphStore.ts`：`loadGraph`、`build_graph` fallback、`layoutStale`、错误状态。
- `src/features/graph/GraphView.tsx`：依赖 `data?.contentHash` 初始化 renderer；有 layout 时直接使用缓存，否则随机+FA2；`layoutStale` 当前未参与渲染决策。
- `src-tauri/src/commands/graph_commands.rs`：`get_graph` 当前只读 cache，不调用 `GraphService::resolve` 校验 live hash。
- `src-tauri/src/services/graph_service.rs`：缓存 `.app/graph-cache.json`、content hash、layout hash mismatch no-op、corrupt cache fallback。
- `src-tauri/src/services/search_service.rs`：wiki 扫描和图谱缓存失效触发点。
- `src/features/graph/graphStore.test.ts`、`graphView.test.tsx`：回归测试入口。

### 7. BUG：Chat 提问显示“上下文不足”

- `src-tauri/src/services/chat_service.rs`：`build_retrieval_context` 只从 search hits 取最多 6 个页面；hits 为空时仍只提供 purpose 和空 Sources。
- `src-tauri/src/services/search_service.rs`：`search` 使用整句 substring 匹配，`to_ascii_lowercase`，对中文自然语言问题和“什么是 X”类 query 召回很弱。
- `src-tauri/src/utils/markdown_utils.rs`：标题、frontmatter、wikilinks、snippet 等工具。
- `src-tauri/src/commands/chat_commands.rs`：`send_chat_message` 构建 retrieval context 并调用 Agent/BYOK。
- `src/stores/chatStore.ts`、`src/features/chat/ChatView.tsx`、`ChatComposer.tsx`、`CitationPanel.tsx`、`MessageContent.tsx`：前端提问、会话与引用展示。
- 关键判断：这不是纯 UI bug，更像检索召回 bug。`什么是约束先行2` 若页面只含 `约束先行`，整句匹配不会命中。

### 8. BUG：Lint 没有历史记录保留，重新打开就没有记录

- `src/stores/lintStore.ts`：`localReport` 存在内存，deep report 需 task id 读取；没有本地 lint 历史列表。
- `src/features/lint/LintView.tsx`：进入视图只加载 ignores/deep task result，不恢复 local report 历史。
- `src-tauri/src/commands/lint_commands.rs`：local lint 同步返回，不写 `.app/lint-reports`；deep lint 写 `.app/lint-reports/<task_id>.json`。
- `src-tauri/src/services/lint_service.rs`：local deterministic lint 与 ignore 文件 `.app/lint-ignore.json`。
- `src-tauri/src/models/lint.rs`、`src/types/lint.ts`：报告 DTO。
- 候选新增持久化：`.app/lint-history.json` 或 `.app/lint-reports/local-*.json`，需明确历史保留策略。

### 9. 页面收藏星标：文件夹展示、左栏精选页面、HTML 页面收藏

- `src/features/wiki/WikiView.tsx`：当前正文 toolbar 已有 `toggleBookmark` 星标按钮，使用 `page.meta.bookmarked`。
- `src/features/wiki/WikiTree.tsx`：文件树已有 `node.starred` 图标渲染，但显示的是 `starred` 字段，未显示 `bookmarked`。
- `src/features/wiki/wikiStore.ts`：`toggleBookmark` 更新 page meta 和 `tree.pages`，未更新 `tree.root` 里的节点字段。
- `src-tauri/src/services/search_service.rs`：从 frontmatter 读取 `starred`，从 `.app/bookmarks.json` 读取 `bookmarked`；`toggle_bookmark` 仅支持 wiki pages。
- `src-tauri/src/commands/wiki_commands.rs`、`src-tauri/src/models/wiki.rs`、`src/types/wiki.ts`：bookmark DTO 与 tree node 字段。
- `src/components/app/LeftSidebar.tsx`：当前有最近页面，适合在工作流下方/最近页面上方新增“精选页面”。
- `src/stores/exportStore.ts`、`src/features/exports/ExportsView.tsx`、`src-tauri/src/services/export_service.rs`、`src-tauri/src/models/export.rs`：HTML 导出记录与未来 HTML 收藏状态。
- 风险：`starred` 与 `bookmarked` 语义重复，需要先统一产品语义。

### 10. 导出页面：预览放大/浏览器预览/列表点击预览/按钮位置

- `src/features/exports/ExportsView.tsx`：左侧 table、末列 eye/folder 按钮、右侧 preview pane。当前 `<tr>` 没有单击预览。
- `src/features/exports/HtmlPreviewPane.tsx`：exports 预览是 sandbox iframe，无工具栏。
- `src/features/wiki/HtmlPreviewPane.tsx`：Wiki 页面内 HTML 预览已有返回、重新生成、打开文件夹、复制路径，可借鉴。
- `src/stores/exportStore.ts`：`records`、`previewHtml`、`previewId`、`openFolder`，无 browser open/maximized state。
- `src-tauri/src/commands/export_commands.rs`：`read_export_preview`、`open_export_folder`；缺少 `open_export_in_browser` 或类似命令。
- `src-tauri/src/services/export_service.rs`：导出文件与 `.app/exports.json`。
- `src/styles.css`：`.exports-view-layout`、`.html-preview`，适合实现最大化/宽屏预览状态。

### 11. Chat：每个 wiki 页面按钮调出侧栏，对当前文章追问

- `src/features/wiki/WikiView.tsx`、`MarkdownReader.tsx`：页面阅读工具栏/正文上方适合放“Ask AI”按钮。
- `src/components/app/RightContextPanel.tsx`：当前 Chat 右栏显示引用，Wiki 右栏显示相关页/导出；可新增 wiki-page chat panel 或切换模式。
- `src/features/chat/ChatView.tsx`、`ChatComposer.tsx`、`src/stores/chatStore.ts`：发送消息和会话上下文。
- `src-tauri/src/services/chat_service.rs`：上下文构建当前只按 query 检索，未支持“固定当前页作为强上下文”。
- `src-tauri/src/commands/chat_commands.rs`、`src-tauri/src/models/chat.rs`、`src/types/chat.ts`：若要支持 page-scoped chat，需扩展请求 DTO。
- `src/stores/navigationStore.ts`：可保存右栏模式和来源页。

### 12. 任务日志（任务和通知页）按执行时间排序

- `src/components/app/TaskLogDrawer.tsx`：当前 `sorted` 先按 `TASK_STATUS_ORDER` 排序，不按执行时间；这正是可见问题入口。
- `src/stores/taskStore.ts`：`upsertTask` 更新/追加，未在 store 层排序。
- `src-tauri/src/tasks/task_service.rs`：后端 `list_tasks` 已按 `updated_at desc` 排序，但前端抽屉又覆盖成状态排序。
- `src-tauri/src/commands/task_commands.rs`、`src-tauri/src/models/task.rs`、`src/types/task.ts`：任务字段 `startedAt`、`updatedAt`、`completedAt`。
- `src/components/app/TaskActivityButton.tsx`：入口按钮只有运行状态提示。

### 13. 顶栏：当前项目、最近项目展示和选择更简洁

- `src/components/app/TopBar.tsx`：当前项目菜单展示 name/rootPath/recent projects，路径完整显示，容易占位。
- `src/stores/projectStore.ts`、`src/types/project.ts`：recent/current project 数据。
- `src-tauri/src/services/project_service.rs`：recent projects 存储与排序。
- `src/styles.css`：topbar 尺寸和文本截断样式。
- `src/i18n/locales/*.json`：文案。

### 14. 优化项目总览页面（初始页）

- `src/features/project/ProjectStartView.tsx`：当前有 hero、filter、quick actions、新建、打开、导入 note、手输 open path form、recent cards、右侧 agent/byok/template。
- `src/stores/projectStore.ts`：create/open/confirm pending action。
- `src-tauri/src/commands/project_commands.rs`：`create_project`、`open_project`、`preview_open_folder_as_project`、`list_recent_projects`。
- `src-tauri/src/services/project_service.rs`：项目 summary 和 health，可为 recent card 提供属性。
- `src/types/project.ts`：ProjectSummary/RecentProject，目前 recent 属性较少。
- `src/styles.css`：`.launch*`、`.projgrid`、`.projcard`、`.quickaction`。

### 15. 新建项目选择保存位置应使用文件资源管理器

- `src/features/project/ProjectStartView.tsx`：`NewProjectDialog` 现在手输 `rootPath`。
- `@tauri-apps/plugin-dialog`：已安装。
- `src/features/import/nativeFilePicker.ts`：已有文件选择 helper，但只支持导入文件；可扩展 folder picker。
- `src/features/import/OpenFolderAsProjectDialog.tsx`：也手输 path，可复用同一目录选择器。
- `src-tauri/capabilities/main.json`：需要确认 dialog 权限。
- `src-tauri/src/services/project_service.rs`：创建项目仍要求目标路径不存在或为空；前端应明确选择“父目录 + 项目名”还是“目标目录”。

## 潜在风险点

1. `AppShell.tsx` 过重，聚合了大量业务流程和多个 store。布局/面板状态改动容易牵动不相关功能。
2. UI 宽度/高度大多写在 `src/styles.css` 和 Tailwind class 中，缺少统一 resizable pane 状态模型。
3. `navigationStore.ts` 只管理 active view 与右栏开关，不足以支持左栏收起、分割线拖动、预览最大化等多面板状态。
4. Chat 检索是当前最大功能风险：`SearchService::search` 使用整句 substring；中文自然语言 query、别名、模糊词、数字后缀都可能召回失败。
5. 图谱 `get_graph` 只读 cache，没有校验当前 wiki content hash；`layoutStale` 传到前端后未被 `GraphView` 使用。
6. 图谱 WebGL 初始化失败只有静态 fallback；`GraphView.tsx` 存在 `console.warn`，虽然不是 `console.log`，但最终检查应确认控制台输出政策。
7. Lint 本地报告和深度报告持久化模型不一致：deep 有 `.app/lint-reports/<task_id>.json`，local 没历史。
8. 收藏语义混乱：frontmatter `starred` 与 `.app/bookmarks.json` `bookmarked` 同时存在；UI 星标按钮操作 bookmark，但树上星标展示 starred。
9. HTML 导出记录没有收藏字段；若 HTML 收藏也写入 `.app/bookmarks.json`，需要支持不同资源类型，避免混淆 wiki page path 与 export path。
10. Export 预览能力分裂：`features/exports/HtmlPreviewPane.tsx` 很轻，`features/wiki/HtmlPreviewPane.tsx` 更完整；后续应抽通用预览控件或明确分工。
11. `TaskLogDrawer` 当前前端状态排序覆盖后端时间排序，是排序需求的直接冲突点。
12. 项目启动页仍带类 landing/hero 结构，与 AGENTS 对“不要 landing hero，直接可用工具”的最新约束有偏差。
13. RecentProject DTO 属性较少，项目卡片要展示更丰富属性时可能需要后端批量 scan recent folders，注意性能和失效路径。
14. 样例 `wiki/wiki/` 是 validation data，不应被当作应用源码改动；当前仓库中已有样例 `.app/settings.json`、task json 等未跟踪/修改状态。
15. Git 工作区已有与本审计无关的改动：`SPEC/roadmap/loop-prompts.md`、`src-tauri/Cargo.toml`、样例 task json、`wiki/.app/settings.json`，后续任务不能误还原。

## 审计结论

所有 15 个条目都能定位到明确代码区域；没有发现“需要人工指引”的条目。最高优先级建议从三类基础能力入手：

1. Shell 面板状态模型：分割线、左栏收起、导出预览最大化都依赖它。
2. 检索与上下文模型：Chat bug 与页面级追问都依赖它。
3. 持久化模型统一：Lint 历史、收藏/精选页面、导出 HTML 收藏都需要清晰的 `.app/*.json` schema。

## 本轮未执行的检查

本轮目标是代码库审计并写入报告，没有修改应用源码；没有运行 `npm run test` 或 `npm run lint`。按项目规则，后续每个实际修复任务完成后应运行 `npm run test`、`npm run lint`，并确认无意外 `console.log`、导入路径可解析。
