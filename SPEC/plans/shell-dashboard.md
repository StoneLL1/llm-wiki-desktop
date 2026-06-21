✅ 本轮完成 @ 2026-06-21

全部 P0 (7 项) + P1 (11 项) 已 verified。npm run test 72/72 全绿，npm run lint 全绿。

改动文件：`src/components/app/LeftSidebar.tsx`、`RightContextPanel.tsx`、`TopBar.tsx`、`BottomStatusBar.tsx`、`src/hooks/useProjectStatus.ts`、`src/styles.css`、`src/i18n/locales/{en,zh-CN}.json`、`src/app/App.test.tsx`。

# 进度账本 · shell + dashboard + 启动页 (P0+P1)

> 权威源：SPEC/roadmap/shell-dashboard.md · UI-Frontend-design/{index,launch,dashboard}.html + assets/app.css · SPEC/PRD.md
> status: pending | in_progress | done | verified

## 本轮计划

逐条推进 roadmap 第 1/2 节中的 P0+P1 项。每完成一项 → done → npm run test+lint 全绿 → verified → 追加 progress.txt。
P2 / 别板块问题不动手，只在对应 roadmap 记一行。

## 关键决策（动手前）

- **托盘 close 拦截已接线**：`src-tauri/src/lib.rs:84-100` 已在 `setup()` 里挂 `window.on_window_event`，根据 `SettingsService::read_close_behavior()` 决定 `MinimizeToTray` 时 `api.prevent_close()` + `window.hide()`。shell-dashboard roadmap 第 51 行说"尚未确认"是错的 → 回改 roadmap 文档（不改代码）。
- **Dashboard 数据源**：统计/分布来自 wikiStore.tree.pages（pageType 计数 + wikilinks 求和）；最近活动来自 taskStore.tasks（已有 taskType + createdAt，等价 .app/tasks/ 聚合）。不新增 IPC，避免 cargo 改动风险。
- **启动页 Agent 检测**：`detect_agents` 签名要求 projectId/rootPath。启动页无项目上下文 → 用空串调用，若后端 resolve_project_context 报错则前端 catch 显示"未检测到"。优先不改后端签名（控制本板块 scope）。
- **CSS 策略**：把设计稿 app.css 中 dashboard/launch 用到的组件类（btn/badge/pill/dotstatus/sumcard/summarygrid/projcard/seg/progress/empty/formrow/input-group/checkbox/kbd/skip-link + dashboard 专属 health-row/type-row/activity-row/graph-mini）移植到 src/styles.css，组件用类名而非纯 Tailwind。

## 条目

### P0

- [x] **P0-1 Dashboard 健康行 + CTA** (health-row) — DashboardView.tsx:109-152, styles.css — status: verified
- [x] **P0-2 Dashboard 统计六宫格** (summarygrid) — DashboardView.tsx:155-167, styles.css — status: verified
- [x] **P0-3 Dashboard 最近活动时间线** (activity-row) — DashboardView.tsx:203-235, styles.css — status: verified
- [x] **P0-4 启动页三栏启动器布局** (launch grid) — ProjectStartView.tsx:118-130, styles.css — status: verified
- [x] **P0-5 启动页项目卡网格** (projgrid+projcard) — ProjectStartView.tsx:243-269 — status: verified
- [x] **P0-6 启动页快速操作三卡** (quickaction) — ProjectStartView.tsx:187-217 — status: verified
- [x] **P0-7 关闭主窗口最小化到托盘** — src-tauri/src/lib.rs:84-100 — status: verified (已接线，回改 roadmap)

### P1

- [x] **P1-1 Dashboard 主题分布柱** (type-row) — DashboardView.tsx:178-201 — status: verified
- [x] **P1-2 Dashboard 快速操作四象限** — DashboardView.tsx:237-280 — status: verified
- [x] **P1-3 启动页搜索 + 分类筛选** (launch__filterbar) — ProjectStartView.tsx:146-184 — status: verified
- [x] **P1-4 启动页 Agent CLI 检测侧栏** (agentmini) — ProjectStartView.tsx:97-115, 289-322 — status: verified
- [x] **P1-5 启动页项目模板侧栏** (templateside) — ProjectStartView.tsx:324-345 — status: verified
- [x] **P1-6 启动页新建项目对话框** (dialog--wide) — ProjectStartView.tsx:359-470 — status: verified
- [x] **P1-7 右侧项目信息面板补全** — RightContextPanel.tsx:28-210, useProjectStatus.ts — status: verified
- [x] **P1-8 状态栏补全** — BottomStatusBar.tsx:1-68, styles.css — status: verified
- [x] **P1-9 侧栏 Agent 底部状态脚** — LeftSidebar.tsx:16-22, 108-122 — status: verified
- [x] **P1-10 侧栏 Lint warn 徽标** — LeftSidebar.tsx:13, 57-59 — status: verified
- [x] **P1-11 顶栏项目切换下拉 + 返回总览 + 语言顺序 + kbd ⌘K** — TopBar.tsx:3, 16-58, 93-145, 159-208 — status: verified

## 进度日志

- 2026-06-21 建账本；核实托盘 close 已接线，P0-7 直接 verified。
- 2026-06-21 移植设计组件 CSS 到 src/styles.css（btn/badge/pill/dotstatus/sumcard/summarygrid/projcard/seg/progress/empty/formrow/input-group/checkbox/kbd/skip-link + dashboard health-row/type-row/activity-row/graph-mini + launch 全套）。
- 2026-06-21 重写 DashboardView：health-row + 六宫格 + 主题分布柱 + 活动时间线 + 快速操作四象限（P0-1/2/3 + P1-1/2 verified，72 tests 全绿）。数据源：wikiStore.tree.pages 聚合类型/wikilinks；taskStore.tasks 聚合活动。scan 加 hasTauri 守卫避免 jsdom 误触发。
- 2026-06-21 重写 ProjectStartView 为三栏 launch 布局：顶栏 nav + hero + filterbar(搜索+分类 pill) + projgrid(3 quickaction + 项目卡) + 右侧 agentmini/BYOK/templateside + 底部状态 + 新建项目 dialog--wide（P0-4/5/6 + P1-3/4/5/6 verified）。语言切换直连 i18next.changeLanguage + localStorage。Agent 检测复用 recentProjects[0] 上下文。
- 2026-06-21 创建 shared hook `useProjectStatus.ts`（module-cache dedup git_status + detect_agents + list_llm_providers，供 sidebar/rightpanel/statusbar 三处消费）。
- 2026-06-21 重写 BottomStatusBar：agent dotstatus + 页数 + 任务数 + 索引同步 + git branch·HEAD + git clean + 语言（P1-8 verified）。
- 2026-06-21 重写 RightContextPanel default 面板：paths 段补 Git branch/HEAD、indexState 段补 last compile/pending、route 段改为 per-agent dotstatus rows + BYOK providers、tasks 段加 busy/ok dotstatus + progress %、disk 段 placeholder（P1-7 verified）。
- 2026-06-21 补全 LeftSidebar Agent 底部脚：从 useProjectStatus 取 default agent → "claude · 1.0.23" + ChevronRight 按钮跳 Agent（P1-9 verified）。补 Lint warn 徽标：lintStore localReport summary.totalIssues 橙色 badge（P1-10 verified）。
- 2026-06-21 补全 TopBar：项目切换 dropdown menu（recent projects + back to launch + 当前 badge）、语言顺序 中文在左、kbd ⌘K on macOS、返回总览 corner button（P1-11 verified）。
