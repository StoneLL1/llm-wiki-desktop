# 应用外壳 + Dashboard + 启动页 板块落差与实施计划

> 对照源：UI-Frontend-design/{index,launch,dashboard}.html + assets/app.css + SPEC/PRD.md
> 当前实现：src/components/app/、src/features/{dashboard,project}/、src/stores/

## 0. 现状摘要

应用外壳的三层栅格骨架（顶栏 48px + 主区 + 状态栏 28px）已经在 `src/components/app/AppShell.tsx` 落地，`LeftSidebar` / `TopBar` / `RightContextPanel` / `BottomStatusBar` / `TaskLogDrawer` / `ConfirmationDialog` / `CompileConflictDialog` / `Toaster` 全部到位。`AppShell` 当前只负责布局、pane 行为、快捷键与全局壳层接线，不再拥有 feature workflow command；`WorkspaceController` 组合项目级 workflows，`WorkspaceRouter` 负责 view-level lazy dispatch，并以 `Suspense` + `ViewErrorBoundary` 隔离加载与渲染失败。token 系统（`src/styles.css`）与设计稿 `app.css` 一一对应，Tauri 托盘菜单和系统通知通道也在 `src-tauri/src/lib.rs:13-80` 实现。整体完成度大约 **55%**：核心栅格和交互行为跑通，但这一板块还停留在"功能能跑"的占位阶段，缺少设计稿定义的关键信息密度与启动体验。

三个最突出的缺口：
1. **Dashboard 退化为纯状态卡片**（`src/features/dashboard/DashboardView.tsx`）——只有 6 个数字指标和三段静态文案，缺设计稿要求的项目健康行、统计摘要六宫格、主题分布柱、最近活动时间线、快速操作四象限、图谱预览（`dashboard.html:142-395`）。
2. **启动页只有一个居中表单**（`src/features/project/ProjectStartView.tsx`）——完全没实现 `launch.html` 定义的三栏启动器：顶栏导航（最近/新建/打开/模板）、搜索 + 分类筛选、项目卡片网格（`projgrid`+`projcard`）、右侧 Agent CLI 检测 / BYOK 状态 / 项目模板侧栏。
3. **顶栏缺失关键元素**：项目切换按钮不带下拉箭头和"当前"徽标、语言切换顺序与设计稿相反（当前 EN 在左，设计稿"中"在左）、顶栏右侧缺"任务与通知"图标按钮的徽标计数（只有红点）、无"返回总览"图标入口。

## 1. 区块 / 组件清单

| 区块/组件 | 设计稿要求（引用设计稿要点） | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 应用栅格 `.app` | `grid-template-rows: 48px 1fr 28px`，三列 `240px / 1fr / 320px`，支持 `.no-right`、`.right-wide`、`.sidebar-collapsed` 三种变体（`app.css:138-163`） | `AppShell` 使用 `.app-shell` 三层栅格和 `is-right-collapsed` / `is-sidebar-collapsed` 状态；左右 pane 可拖拽调整，尺寸与侧栏折叠状态持久化。尚无设计稿独立的 `right-wide` 模式 | 🟡部分实现 | P1 | `src/components/app/AppShell.tsx`、`src/stores/navigationStore.ts`、`src/styles.css` |
| 顶栏 TopBar | 品牌区 + 项目切换按钮（带下拉箭头与路径）+ 搜索框（30px、⌘K 快捷键提示、focus ring）+ 任务/通知铃 + 语言切换 + 设置 + 返回总览（`dashboard.html:62-90`、`app.css:166-307`） | 高 48px；品牌 LW 标、项目名+路径按钮、搜索框含 ⌘K、任务铃（`TaskActivityButton` 红点）、语言切换（EN/中）、设置；**缺：下拉箭头、"当前"徽标、返回总览按钮；语言切换顺序与设计稿相反；搜索是单关键词跳转，非下拉建议** | 🟡部分实现 | P1 | src/components/app/TopBar.tsx:83-171 |
| 项目切换按钮 `.projswitch` | 30px 高、圆角 6、hover 描边、名称 + 路径 + 下拉箭头；点击弹项目切换菜单 | 30px 高、hover 背景一致，但**无下拉箭头、无"当前"徽标、点击直接清空 currentProject 回启动页**（非弹出切换菜单） | 🟡部分实现 | P1 | src/components/app/TopBar.tsx:92-102 |
| 全局搜索 `.searchbar` | 30px 高、focus 时 teal ring、内嵌 `⌘K` kbd 提示 | 高 30px、focus ring 一致、kbd 提示为 `Ctrl K`（设计稿是 `⌘K`）；**回车跳结果列表，无最近搜索、无标签/类型下拉过滤** | 🟡部分实现 | P1 | src/components/app/TopBar.tsx:104-133 |
| 左侧栏 `.sidebar` | 三段：主视图（Dashboard/Wiki/Chat/Graph，Wiki 带计数）/ 工作流（Agent/Import/Lint/Exports，Lint 带 warn 徽标 3）/ 最近页面（26px 行、可滚动）+ 底部 Agent 状态脚（绿点 + claude 版本 + 跳转箭头，`dashboard.html:93-129`） | 主视图 + 工作流 + 最近页面结构到位，计数只在 Wiki 显示，**Lint 的 warn 徽标未实现、Agent 底部状态脚用 i18n routeLabel 替代了设计稿的"claude · 1.0.23"+跳转箭头** | 🟡部分实现 | P1 | src/components/app/LeftSidebar.tsx:28-107 |
| 导航项 `.navitem` | 30px 高、active 态 teal-soft 背景 + teal-hover 文字；26px 行高的紧凑变体用于最近页面 | 30px / 26px 高度、active 样式、`aria-current="page"` 全部对齐 | ✅已完成 | — | src/components/app/LeftSidebar.tsx:33-49, 83-95 |
| Section 标签 `.sidebar__label` | 10.5px、大写、`letter-spacing: 0.08em`、muted | 三段标签都对齐；最近页面缺设计稿的刷新按钮 `.sidebar__label-actions` | 🟡部分实现 | P2 | src/components/app/LeftSidebar.tsx:60-75 |
| 主区头 `.main__header` | 52px、标题 16px + sub 12px mono、右侧 toolbar | `WorkspaceController` 渲染 52px `workspace-header` 与标题；**缺刷新图标按钮，缺 view.toolbar 分段控件（如 Dashboard 的"刷新/导入资料"）** | 🟡部分实现 | P2 | `src/components/app/WorkspaceController.tsx` |
| 主区 body | padded / padded-lg 两种内边距（`app.css:475-476`） | `WorkspaceController` 按 view 选择 `p-4` 或 `overflow-hidden`；**padded-lg（24px）未实现** | 🟡部分实现 | P2 | `src/components/app/WorkspaceController.tsx` |
| 右侧项目信息面板 | 四段：路径（根/Schema/Purpose/Git 分支/HEAD）/ 索引状态（页面/索引/图谱缓存/上次编译/待确认）/ 执行路径（claude/codex/BYOK 三行 dotstatus）/ 背景任务（进度条）/ 磁盘占用（`dashboard.html:400-469`） | 只实现了路径、索引状态、执行路径、背景任务四段；**缺：Git 分支/HEAD、上次编译时间、待确认计数、磁盘占用 5 段；执行路径只渲染单条 routeLabel，不显示每个 agent 的 dotstatus；背景任务无进度条** | 🟡部分实现 | P1 | src/components/app/RightContextPanel.tsx:122-197 |
| 右侧面板分段（chat/wiki/graph） | Chat→引用面板；Wiki→相关文章；Graph→节点 inspector | Chat/Wiki/Graph 三种变体已对齐 PRD-10 | ✅已完成 | — | src/components/app/RightContextPanel.tsx:30-120 |
| 状态栏 `.statusbar` | 28px、mono 11px、分隔符；左：项目路径；右：claude 版本、Agent 路径、237 页、索引同步、main·HEAD、Git 干净、中文 | 28px、mono 11px；**只显示路径 + route + tasks + wikiPages 四项；缺 claude 版本/HEAD/索引同步时间/Git 干净/语言** | 🟡部分实现 | P1 | src/components/app/BottomStatusBar.tsx:1-23 |
| Dashboard 健康行 `.health-row` | 36x36 teal-soft 图标 + 标题 + 描述 + 两个 CTA（查看 Lint / Agent 面板，`dashboard.html:142-153`） | **完全未实现**；以 `ProjectHealth` 徽标列表（purpose/schema/app/wiki/obsidian 五个 pill）替代，信息密度低、无 CTA | ❌缺失 | P0 | src/features/dashboard/DashboardView.tsx:96-129 |
| Dashboard 统计摘要 `.summarygrid` | 六张 sumcard：Wiki 页面（28px mono 数字 + delta）/ Entities / Concepts / Sources / Synthesis / Wikilinks（`dashboard.html:155-190`） | **完全未实现**；用 3 列 6 行 dl/dt 小字表格代替，无 28px 大数字、无 delta、无 hint | ❌缺失 | P0 | src/features/dashboard/DashboardView.tsx:18-35 |
| Dashboard 主题分布 `.type-row` | 六行：色块 + 类型名 + 80px 条形 + 计数，按类型/社区切换（`dashboard.html:194-242`） | **完全未实现** | ❌缺失 | P1 | src/features/dashboard/DashboardView.tsx |
| Dashboard 最近活动 `.activity-row` | 时间线：编译/导入/Lint/编辑/Git 检查点/导出，每行 16px 图标 + 标题 + sub mono + 时间（`dashboard.html:244-300`） | **完全未实现**；仅有最近任务列表（最多 6 条 title + status） | ❌缺失 | P0 | src/features/dashboard/DashboardView.tsx:67-82 |
| Dashboard 快速操作 | 四张可点击 panel 卡：导入资料 / 运行 Lint / 开始问答 / 生成 HTML 报告（`dashboard.html:304-348`） | **完全未实现**；顶部主区头有 primary/secondary 两个按钮但非卡片入口 | ❌缺失 | P1 | src/features/dashboard/DashboardView.tsx |
| Dashboard 图谱预览 `.graph-mini` | 200px 高、网格背景、SVG 节点预览、"进入完整图谱 →"链接（`dashboard.html:352-395`） | **完全未实现** | ❌缺失 | P2 | src/features/dashboard/DashboardView.tsx |
| 启动页整体布局 `.launch` | `56px 顶 + 1fr + 36px 底`、左主区 + 右 360px 侧栏（`launch.html:10-147`） | **完全未实现**；当前是居中 max-w-760 单 card 表单 | ❌缺失 | P0 | src/features/project/ProjectStartView.tsx:54-108 |
| 启动页顶栏 `.launch__top` | 品牌 LW + 名称 + 顶部 nav（最近项目/新建项目/打开文件夹/项目模板）+ 语言切换 + 设置 + 帮助（`launch.html:151-173`） | **完全未实现** | ❌缺失 | P0 | src/features/project/ProjectStartView.tsx:54-63 |
| 启动页项目卡片网格 `.projgrid` + `.projcard` | auto-fill minmax(280px, 1fr)、每张卡片 32x32 mark + 标题 + 路径 + 类型 pill + 页数 + 时间（`launch.html:198-319`） | **完全未实现**；当前是按钮列表 36px 高一行项目 | ❌缺失 | P0 | src/features/project/ProjectStartView.tsx:98-101 |
| 启动页快速操作 `.quickaction` | 三张虚线卡：新建空项目 / 打开文件夹为项目 / 导入资料到已有项目（`launch.html:200-214`） | **完全未实现** | ❌缺失 | P0 | src/features/project/ProjectStartView.tsx |
| 启动页搜索 + 筛选 `.launch__filterbar` | 36px 高搜索框 + 分类 pill（全部/研究/读书/个人成长/商业/通用，`launch.html:182-196`） | **完全未实现** | ❌缺失 | P1 | src/features/project/ProjectStartView.tsx |
| 启动页右侧 Agent 检测 `.agentmini` | claude/codex/openclaw/hermes 四 agent + BYOK Anthropic/OpenAI 状态，每个含 dotstatus + 路径（`launch.html:328-379`） | **完全未实现** | ❌缺失 | P1 | src/features/project/ProjectStartView.tsx |
| 启动页右侧项目模板 `.templateside` | 五张卡：通用/研究/读书/个人成长/商业（`launch.html:381-401`） | **完全未实现**；当前模板是 `<select>` 下拉 | ❌缺失 | P1 | src/features/project/ProjectStartView.tsx:93 |
| 新建项目对话框 `.dialog--wide` | 项目名称 + 保存位置（带选择按钮）+ 模板分段控件 + Git 初始化复选框（`launch.html:421-480`） | **完全未实现**；当前是内联表单 | ❌缺失 | P1 | src/features/project/ProjectStartView.tsx:81-95 |
| 启动页底部状态栏 `.launch__bottom` | 36px、应用就绪 + Agent 可用数 + BYOK 状态（`launch.html:409-417`） | **完全未实现** | ❌缺失 | P2 | src/features/project/ProjectStartView.tsx |
| 确认对话框 `.dialog` | overlay + 480px wide、head/body/foot 三段、ESC 关闭、Tab 焦点陷阱（`app.css:1376-1416`） | overlay + 560px、head/body/foot、ESC + Tab 陷阱已实现 | ✅已完成 | — | src/components/app/ConfirmationDialog.tsx:68-171 |
| 编译冲突对话框 `.dialog--wide` | 640px、Markdown diff 三选一（保留/使用生成/手动合并）（`app.css:1398`、`SPEC.md` §9.9 PRD-GIT-004） | 900px、左右双栏对比 + 手动合并 textarea + 三按钮 | ✅已完成 | — | src/components/app/CompileConflictDialog.tsx:81-110 |
| 任务日志抽屉 `.drawer` | 460px 右抽屉、左 180px 任务列表 + 右日志面板、取消按钮、进度条、终端样式日志（`app.css:1418-1439`） | 420px、180px 任务列表 + 日志、取消按钮、进度条；宽度比设计稿窄 40px | 🟡部分实现 | P2 | src/components/app/TaskLogDrawer.tsx:137-272 |
| Toast `.toast` | 右下、280-380px、深色背景、三态（ok/err/info，`app.css:1441-1467`） | 右下、max-360、白底带描边、info/warning/error 三态；**配色与设计稿相反：设计稿 toast 是深底白字，当前是浅底深字** | 🟡部分实现 | P2 | src/components/app/Toaster.tsx |
| 任务活动按钮 `.topbar__actions` 铃 | 含未读数量 badge（`dashboard.html:82`） | Bell 图标 + 红点（无数字） | 🟡部分实现 | P2 | src/components/app/TaskActivityButton.tsx |
| 托盘 + 系统通知 | 关闭主窗口默认最小化到托盘并继续；完成后系统通知（`SPEC.md` §CLAUDE.md 长任务硬边界、PRD-AGENT-005） | Tauri 托盘菜单 Show/Hide/Quit + 左键点击恢复 + 通知 plugin 注册 + `src/services/notifications.ts` 已发送完成/失败/需确认通知；**close 拦截已接线**：`lib.rs:84-100` 在 `setup()` 挂 `window.on_window_event`，`CloseRequested` 时读 `SettingsService::read_close_behavior()`，`MinimizeToTray` → `api.prevent_close()` + `window.hide()` | ✅已完成 | — | src-tauri/src/lib.rs:84-100, src/services/notifications.ts |
| Skip link `.skip-link` | 键盘可达性：跳至主内容（`app.css:2123-2137`） | **未实现** | ❌缺失 | P2 | src/components/app/AppShell.tsx |
| 响应式折叠 | 1180px 下右面板变 fixed 抽屉、820px 下侧栏折叠为 56px（`app.css:2069-2121`） | 1180px 下右面板以 fixed drawer + backdrop 呈现并默认关闭；pane 尺寸和折叠偏好写入 `localStorage`。820px 下会隐藏侧栏文字，但 CSS 未强制把已持久化宽度收敛为 56px | 🟡部分实现 | P2 | `src/components/app/AppShell.tsx`、`src/styles.css`、`src/stores/navigationStore.ts` |

## 2. 功能落差（PRD 对照）

- [ ] **Dashboard 项目健康行 + CTA**：现状只有静态 pill 列表 → 目标（`dashboard.html:142-153`，PRD §12.1 "看到第一个图谱"前的健康反馈） → 涉及 `src/features/dashboard/DashboardView.tsx` → 验收标准：显示一句"项目状态良好 · 最近一次编译 X 分钟前" + 页面/冲突/Git 提交摘要 + "查看 Lint"和"Agent 面板"两个 CTA 按钮。
- [ ] **Dashboard 统计六宫格 `.summarygrid`**：现状 3×2 dl 小字 → 目标（`dashboard.html:155-190`，PRD §9.5 图谱节点/边数量用户感知） → `src/features/dashboard/DashboardView.tsx` → 验收标准：Wiki 页面/Entities/Concepts/Sources/Synthesis/Wikilinks 六张 sumcard，28px mono 大数字 + delta/hint。
- [ ] **Dashboard 最近活动时间线**：现状只有任务名列表 → 目标（`dashboard.html:244-300`，PRD §9.7 Agent 任务面板的状态可追溯） → 需要新增活动源（`wiki/log.md` + `.app/tasks/`） → `src/features/dashboard/DashboardView.tsx` + 新 store → 验收标准：时间线显示编译/导入/Lint/编辑/Git 检查点/导出 6 类活动，每行图标 + 标题 + mono sub + 相对时间。
- [ ] **Dashboard 快速操作四象限**：现状无 → 目标（`dashboard.html:304-348`，PRD §10 信息架构核心入口） → `src/features/dashboard/DashboardView.tsx` → 验收标准：导入资料 / 运行 Lint / 开始问答 / 生成 HTML 报告 四张可点击 panel 卡，点击进入对应视图。
- [ ] **Dashboard 主题分布柱**：现状无 → 目标（`dashboard.html:194-242`，PRD §9.5 类型着色） → `src/features/dashboard/DashboardView.tsx` + 索引数据 → 验收标准：按页面类型显示色块 + 条形 + 计数，支持"类型/社区"切换。
- [ ] **启动页启动器布局**：现状居中表单 → 目标（`launch.html:150-417`，PRD-PROJ-004 "最近项目列表可快速进入"、§12.2 "新用户不需要理解 Git"） → `src/features/project/ProjectStartView.tsx` 大改 → 验收标准：顶栏 + 主体（搜索/筛选 + 项目卡网格 + 快速操作）+ 右侧 360px 侧栏（Agent 检测 + BYOK 状态 + 模板）+ 底部状态栏。
- [ ] **启动页项目卡网格 `.projcard`**：现状一行按钮 → 目标（`launch.html:217-318`） → `src/features/project/ProjectStartView.tsx` → 验收标准：每张卡 32px mark + 标题 + 路径 + 类型 pill + 页数 + 相对时间；当前项目加"当前"teal 徽标。
- [ ] **启动页快速操作 `.quickaction`**：现状无 → 目标（`launch.html:200-214`，PRD-PROJ-001/002/003 三个入口） → `src/features/project/ProjectStartView.tsx` → 验收标准：新建空项目 / 打开文件夹为项目 / 导入资料到已有项目 三张虚线卡。
- [ ] **启动页 Agent CLI 检测侧栏**：现状无 → 目标（`launch.html:328-379`，PRD-AGENT-001/002） → `src/features/project/ProjectStartView.tsx` + `detect_agents` IPC → 验收标准：在启动页就显示 claude/codex/openclaw/hermes 的 dotstatus + 版本 + 路径，BYOK Anthropic/OpenAI 的 key 状态。
- [ ] **启动页项目模板侧栏 `.templateside`**：现状 `<select>` 下拉 → 目标（`launch.html:381-401`，PRD-PROJ-005） → `src/features/project/ProjectStartView.tsx` → 验收标准：五张可点击模板卡（通用/研究/读书/个人成长/商业），描述用途。
- [ ] **新建项目对话框 `.dialog--wide`**：现状内联表单 → 目标（`launch.html:421-480`，PRD-GIT-001 初始化 Git 复选框） → `src/features/project/ProjectStartView.tsx` → 验收标准：640px 对话框，项目名/保存位置（带"选择…"按钮）/模板分段控件/Git 初始化复选框。
- [ ] **右侧项目信息面板补全**：现状 4 段 → 目标（`dashboard.html:407-468`） → `src/components/app/RightContextPanel.tsx:122-197` → 验收标准：补 Git 分支/HEAD、上次编译时间、待确认计数、磁盘占用 5 段；执行路径从单行 routeLabel 改为每个 agent 一行 dotstatus。
- [ ] **状态栏补全**：现状 4 项 → 目标（`dashboard.html:473-489`） → `src/components/app/BottomStatusBar.tsx` → 验收标准：补 claude 版本、Git 分支 + HEAD、索引同步时间、Git 干净状态、当前语言。
- [x] **关闭主窗口最小化到托盘**：✅ 已实现（`lib.rs:84-100` `on_window_event` 拦截 `CloseRequested`，按 `read_close_behavior()` 决定 hide/quit）。验收通过。
- [ ] **侧栏 Agent 底部状态脚**：现状显示 routeLabel 文案 → 目标（`dashboard.html:122-128`） → `src/components/app/LeftSidebar.tsx:99-106` → 验收标准：显示"claude · 1.0.23"+ 右箭头按钮跳 Agent 视图；未配置时显示灰点 + "未配置"。
- [ ] **侧栏 Lint warn 徽标**：现状无 → 目标（`dashboard.html:108`） → `src/components/app/LeftSidebar.tsx` → 验收标准：Lint 导航项右侧显示橙色数字徽标，数量来自 lint store issueCount。
- [x] **响应式右面板与 pane 持久化** @ 2026-07-10：`src/styles.css` 已实现 1180px fixed 右抽屉/backdrop；`AppShell` 响应右面板状态，`navigationStore` 持久化 pane 尺寸和侧栏折叠偏好。820px 下强制收窄为设计稿 56px 仍是 P2 落差。
- [ ] **Toast 配色对齐设计稿**：现状白底描边 → 目标（`app.css:1441-1467`） → `src/components/app/Toaster.tsx` → 验收标准：深底白字、三态色（默认黑、ok teal、err red）。
- [ ] **Toast + Drawer 宽度对齐**：现状 420px → 目标（`app.css:1429, 1449`：460px、toast 280-380px） → `src/components/app/TaskLogDrawer.tsx`、`Toaster.tsx` → 验收标准：drawer 460px、toast max-width 380px。
- [ ] **Skip link 键盘可达**：现状无 → 目标（`app.css:2123-2137`） → `src/components/app/AppShell.tsx` → 验收标准：Tab 首焦点显示"跳至主内容"链接，回车聚焦主区。
- [ ] **顶栏返回总览按钮**：现状无 → 目标（`dashboard.html:88`） → `src/components/app/TopBar.tsx` → 验收标准：右上角 corner 图标，点击回启动页。

## 3. 视觉 / 设计 token 落差

| 维度 | 设计稿要求 | 当前实现 | 偏差 |
|---|---|---|---|
| 字号 绝对 px | 正文 13px、次要 12px、muted/mono 11px、section 标签 10.5px、sumcard 数字 28px、阅读 14-15px、标题 16/18/22/28px（CLAUDE.md 前端设计对齐原则 #1） | 大量使用 `text-[13px]`、`text-[12px]`、`text-[11px]`、`text-[10.5px]`、`text-[16px]` 绝对 px | ✅ 已对齐 |
| 组件高度 | 顶栏 48、主区头 52、右面板头 52、状态栏 28、导航项 30（小号 26）、panel 头 44 | 顶栏 `h-12`(48)、主区头 `h-[52px]`、右面板头 `h-[52px]`、状态栏 `h-7`(28)、导航 30/26 | ✅ 已对齐 |
| Section 标签 | 10.5px、大写、`letter-spacing: 0.08em`、muted | LeftSidebar 三个 section、RightContextPanel 11px 大写 `tracking-[0.06em]`（**0.06em 比 0.08em 略紧**） | 🟡 偏差 |
| Token 单一来源 | 颜色/圆角/字体/间距只在 `src/styles.css :root` 定义 | `src/styles.css:5-56` 定义；`AppShell` Tailwind 类直接引用 `var(--*)` | ✅ 已对齐 |
| Font | Inter (UI)、JetBrains Mono (code/path)、Source Serif Pro (display) | `--font-ui/display/mono` 已定义；字体打包未在 styles.css 中 @fontsource 导入（**仅在 package.json 依赖、入口是否 import 未在本次审计范围确认**） | 🟡 待确认 |
| 圆角 | sm 4 / md 6 / lg 8 / pill 9999 | token 完全一致 | ✅ 已对齐 |
| 单一 teal 强调 | `--accent: #10a37f`，无渐变无装饰 | token 一致；Toast/Dashboard 自定义 panel 类未走 accent-soft | 🟡 偏差 |
| AppShell 最小宽度 | 设计稿 `app.css:146` 使用 `min-width: 0` | `.app-shell__workbench` 与主区已使用 `min-width: 0`，无 `min-w-[1120px]` 硬限制 | ✅ 已对齐 |
| 间距 token sp-* | 4/8/12/16/20/24/32/40/48/64 | styles.css 已定义；组件中 `gap-2`/`gap-3`/`p-4` 等混用 Tailwind 与 sp-*，与设计稿一致 | ✅ 已对齐 |
| 暗色主题 | 设计稿 `app.css` 仅 light；`SPEC/PRD.md` PRD-SET-004 要求支持暗色 | `src/styles.css:58-102` 已实现 `data-theme="dark"` 和 `auto` 三套 token | ✅ 超出设计稿（合理） |

## 4. 交互 / 可访问性落差

- [ ] **aria-current 导航高亮**：✅ 已实现（`LeftSidebar.tsx:35` `aria-current={active ? "page" : undefined}`）。
- [ ] **搜索快捷键 ⌘K**：🟡 已实现 Ctrl/Cmd+K 聚焦搜索框（`TopBar.tsx:32-41`），但 kbd 提示文案显示 `Ctrl K` 而非设计稿的 `⌘K` 符号；macOS 平台应显示 `⌘K`。
- [ ] **语言切换**：🟡 已实现中/英切换（`TopBar.tsx:138-159`），但顺序与设计稿相反（设计稿"中"在左、当前 EN 在左），且未持久化到 i18n.changeLanguage + settings（调用 `persistPatch` 但未触 i18n 实际切换，需核对 settingsStore 是否联动 i18n）。
- [ ] **键盘焦点陷阱**：✅ 已实现（`ConfirmationDialog.tsx:41-66` Tab/Shift+Tab 循环 + ESC）。
- [ ] **Skip link**：❌ 未实现（`app.css:2123-2137`、WCAG 2.1 SC 2.4.1）。涉及 `AppShell.tsx`。
- [ ] **focus-visible 全局 ring**：🟡 `styles.css:136-141` 实现了 button/input/select，但 icon-button hover 无 focus 态，nav 项 focus 无 ring（对照 `app.css:1950-1976`）。
- [ ] **prefers-reduced-motion**：❌ 未在 `styles.css` 实现降级（`app.css:2031-2043`）；spinner 动画在 reduced-motion 下仍在跑。
- [ ] **aria-live toast**：✅ 已实现（`Toaster.tsx:21-22` `role="status" aria-live="polite"`）。
- [ ] **role/aria-label 面板**：✅ RightContextPanel 四种变体都有 `aria-label`（`RightContextPanel.tsx:35,59,95,127`）。
- [ ] **Tooltip `data-tip`**：❌ 设计稿用 `[data-tip]:hover::after`（`app.css:1484-1503`），当前实现用原生 `title` 属性（`TopBar.tsx:96`、`TaskActivityButton.tsx:12`），样式与延迟不一致。
- [ ] **Dashboard 卡片可达性**：🟡 当前 DashboardView `<section aria-label>` 有标签，但快速操作卡（未实现）应为 `<a>`/`<button>` 可聚焦。
- [ ] **projcard 键盘聚焦**：❌ 启动页项目卡未实现，`app.css:1961-1976` 要求 `.projcard:focus-visible` 有 ring。
- [ ] **顶部 nav 按钮态**：❌ 启动页顶栏 `.launch__nav` 按钮的 `is-active` 态未实现（因为顶栏本身未实现）。

## 5. 建议实施顺序

1. **[P0] Dashboard 四象限重构**（依赖：无） → 重写 `src/features/dashboard/DashboardView.tsx`，落地健康行 + 统计六宫格 + 最近活动时间线 + 快速操作四象限。时间线和分布柱需先约定后端返回结构（`wiki/log.md` 解析 + `.app/tasks/` 聚合）。
2. **[P0] 启动页重写为设计稿三栏布局**（依赖：PRD-PROJ-001/002/003/004 全部） → 重写 `src/features/project/ProjectStartView.tsx`，引入 `projgrid`/`projcard`/`quickaction`/`agentmini`/`templateside`，新建项目走 `.dialog--wide` 弹窗。
3. **[P0] 关闭主窗口最小化到托盘**（依赖：settingsStore 关闭行为选项） → `src-tauri/src/lib.rs` `run()` 中加 `.on_window_event` 拦截 `WindowEvent::CloseRequested`，根据 setting 转 hide；前端 `AppShell` 卸载时不杀任务。
4. **[P1] 右侧项目信息面板 + 状态栏补全**（依赖：GitService 暴露 branch/HEAD/disk usage IPC、IndexService 上次编译时间） → `src/components/app/RightContextPanel.tsx` 补 5 段、`BottomStatusBar.tsx` 补 5 项。
5. **[P1] 顶栏项目切换下拉 + 返回总览**（依赖：recentProjects store） → `src/components/app/TopBar.tsx` 项目按钮改为弹出 menu，新增 corner 按钮。
6. **[P1] 左侧栏 Lint warn 徽标 + Agent 底部脚**（依赖：lintStore issueCount、agent version） → `src/components/app/LeftSidebar.tsx`。
7. **[P1] 启动页搜索/筛选 + 模板侧栏 + Agent 检测**（依赖：步骤 2 已落地骨架） → 启动页第二迭代。
8. **[P2] 视觉打磨**：Toast 配色对齐、Drawer 宽度 460、Tooltip 用 `data-tip`、prefers-reduced-motion、Skip link、820px 下侧栏强制收窄为 56px；响应式右抽屉、pane 持久化与 `min-width: 0` 已完成。
9. **[P2] section 标签 letter-spacing 改 0.08em**（依赖：无） → 全局 find/replace `tracking-[0.06em]` → `tracking-[0.08em]` 在 section 标签上下文。

---

**关键硬边界遵守情况**

- ✅ 所有项目操作走 Tauri IPC（`invoke("open_project")` 等），UI 无直接文件系统访问。
- ✅ 确认对话框覆盖风险操作（删除/覆盖/合并冲突）。
- ✅ 语言切换、Agent 检测、BYOK 状态都有对应 IPC。
- ✅ AppShell 已移除 `min-w-[1120px]` 硬限制，并具备响应式右抽屉与持久化 pane 行为；820px 下侧栏仍需强制收窄为 56px。
- ⚠️ "关闭窗口继续后台任务"未完整闭环（托盘 + 通知已实现，但 close 拦截未实现）。

**遗留风险**

- Dashboard 重构需要后端提供活动流（`wiki/log.md` + `.app/tasks/`），目前无对应 IPC。
- 启动页 Agent 检测需要 `detect_agents` 在无 project 上下文下可调用，当前实现只能复用最近项目的 projectId/rootPath；没有 recent project 时无法检测（`src/features/project/ProjectStartView.tsx:114-135`），启动页场景需调整。
