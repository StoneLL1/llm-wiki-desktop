# 进度账本 · cross-cutting 前端 (P0+P1)

✅ 本轮完成 @ 2026-06-29

## 摘要

cross-cutting 前端 P0+P1 四项全部落地、通过双子代理审查并 verified。范围严格限定 `src/`，未触碰 `src-tauri/` 与 `UI-Frontend-design/`。后端 cross-cutting（P0-1 i18n 5 prompt 点 / P0-2 PendingAction.checkpoint_hash / P0-3 托盘 i18n / P1-1 BYOK 流式进度）已于 2026-06-21 收敛，本轮只接其后端明确遗留的前端接线。

- **FE-P0-1 ConfirmationDialog 检查点如实显示**：`AppShell.tsx:162` 把硬编码 `checkpointExists={false}` 改为 `displayedPendingAction.checkpointHash != null`。后端 compile 冲突已把 `checkpoint_hash` camelCase 透传到 `PendingAction.checkpointHash`（TS 接口 `types/backend.ts:31` 早有该字段），现在 compile 冲突（先 checkpoint）显示"available"，lint/chat/import（确认后才 checkpoint）显示"not created yet"。诚实性红线关闭。
- **FE-P0-2 Dashboard 首屏 Agent 检测**：`AppShell.tsx:272-275` 新增 effect——`currentProject.projectId` 变化时跑一次 `refreshCapabilities`（守 `hasTauri && projectId`），把 `agentRoute` 写进 projectStore。`refreshCapabilities` 加陈旧写入守卫（`AppShell.tsx:253-257`）：探测在途期间项目已切，则丢弃结果，防 A 项目的慢探测覆盖 B 的 agentRoute。
- **FE-P1-1 import 预览去 250ms 轮询改事件驱动**：新增 `src/lib/waitForTaskTerminal.ts`——订阅 `task://completed|failed|cancelled` + race-safe 初始 `get_task`（防 listener attach 前已 terminal），已 terminal 立即短路，settled 后清所有 listener。`AppShell.tsx:309` 用它替换 `while + setTimeout(250) + get_task` 轮询，统一事件驱动机制。
- **FE-P1-2 destructive 按钮 variant 对齐**：`button.tsx:14` cva 加 `danger` variant（视觉与原 className 覆盖 1:1），`ConfirmationDialog.tsx:128` 改 `variant={isDestructive ? "danger" : "secondary"}` 去掉 className 覆盖，语义/样式对齐。

验证：
- `npm run test` → **163 passed / 31 files**（净增 +11 测试：waitForTaskTerminal 5、appShellActions +4、ConfirmationDialog +2）。
- `npm run lint` → **0 warning（exit 0）**。
- 无 `console.log` 残留（全仓唯一 `console.warn` 在 graph 板块 GraphView，非本次文件）。
- 未改 `src-tauri/`、未改 `UI-Frontend-design/`。

双子代理审查（A 共享上下文 / B 零偏见）：**无 BLOCKER**，B 独立确认 P0-1/P0-2/P1-1 测试均非假阳性。合并修 3 项（commit `f1d0bff`）：①refreshCapabilities 陈旧写入竞态守卫；②waitForTaskTerminal `get_task` 返回 null/缺 `.status` 时 `next &&` 守卫防崩→挂起；③FE-P0-1 undefined 字段回归测试。

## 文件清单

新增：
- `src/lib/waitForTaskTerminal.ts` — 事件驱动等任务终态 helper。
- `src/lib/waitForTaskTerminal.test.ts` — 5 个用例（已 terminal 短路 / 事件触发+清监听 / get_task 竞态 / 忽略它任务 / running 不 resolve）。
- `SPEC/plans/cross-cutting-fe.md` — 本账本。

修改：
- `src/components/app/AppShell.tsx` — checkpointExists 接线（P0-1）、首屏 detect effect + 陈旧写入守卫（P0-2）、import 预览改 waitForTaskTerminal + 移除局部 isTerminalTask（P1-1）。
- `src/components/app/ConfirmationDialog.tsx` — destructive 用 danger variant、去 className 覆盖（P1-2）。
- `src/components/ui/button.tsx` — cva 加 danger variant（P1-2）。
- `src/components/app/appShellActions.test.tsx` — checkpoint 接线测试（available / null / undefined）+ 首屏 agentRoute 刷新测试。
- `src/components/app/ConfirmationDialog.test.tsx` — destructive→danger、非 destructive→secondary 样式断言。

## commits

`310f4b6`（P0-1）→ `4575f68`（P0-2）→ `32e4a9a`（P1-1）→ `e9a240e`（P1-2）→ `f1d0bff`（双子代理审查修复）。

---

## blocked（后端/平台，不在 src/ scope）

- **托盘菜单运行时语言切换**：托盘在 `src-tauri/src/lib.rs` 构建，动态重建受 Tauri 限制（需重启窗口）。后端已按 settings.language 本地化 Show/Hide/Quit/tooltip，但切换语言后要重启才生效。属 src-tauri/ + 平台，本 loop 不动手。

## 审查延后项（记 roadmap，不在本 loop 修，附理由）

- **danger 按钮 hover 无变化**（B-5）：`hover:bg-[var(--danger)]` 与 rest 同色，鼠标无 hover 反馈。系原 className 覆盖既有 bug，被 1:1 迁入。本 loop P1-2 scoped 为"variant↔样式对齐、视觉不变"，加 hover darken 需 `color-mix` 脆弱 Tailwind 任意值或新增 `--danger-hover` token，超范围。记 roadmap。
- **graphStore 仍是 250ms 轮询**（A-5/B-3）：`src/stores/graphStore.ts:156` 的 runGraphBuild 用同款轮询，与本轮事件驱动方向不一致。属 graph 板块（已单独收敛），本 loop 不越界。可后续复用 `waitForTaskTerminal` 收敛。
- **useProjectStatus 与 refreshCapabilities 双路探测**（B-3）：3 个 shell 面板的 useProjectStatus 各自发 detect_agents/list_llm_providers 写 local state，refreshCapabilities 另写 projectStore.agentRoute，并发可能短暂不一致。深层架构收敛（单一数据源），超本 loop scope。
- **waitForTaskTerminal 无超时**（A-NIT-4/B-1）：backend 静默丢事件 + get_task 不返回 terminal 时会挂（与原轮询等价）。no-Tauri 路径下 `preview_import` 先 reject，helper 不可达；真 Tauri 下需 backend bug 才触发。加任意超时会掩盖真实慢任务，未加。

---

> 权威源：SPEC/roadmap/cross-cutting.md 第 2 节 · SPEC/PRD.md · CLAUDE.md「必读硬边界」· UI-Frontend-design/（只读）
> scope：只动 src/。后端 cross-cutting 已收敛（见 cross-cutting-be.md，2026-06-21），本账本只接前端遗留。
> status: pending | in_progress | done | verified | blocked

## 范围界定（动手前）

cross-cutting 后端 P0+P1 已全部 verified。roadmap §3 里前端可做（只动 src/）的 P0/P1 收敛为 4 项；其余要么后端已落地、要么是后端 src-tauri/ 工作（标 blocked，不动手）：

- **2.7 i18n Agent 生成内容语言**（P0）：后端已注入 5 prompt 点。前端 UI i18n 已全量双语。**无前端可做项** → 不列条目。
- **2.4 BYOK compile 流式进度**（P1）：后端 `tasks/byok_progress.rs` 已统一 4 处。前端无消费动作 → 不列条目。
- **托盘菜单 i18n**：托盘在 Rust lib.rs 构建，属 src-tauri/。动态重建需重启窗口 → **blocked（后端/平台）**。

## 条目

### P0

- [x] **FE-P0-1 ConfirmationDialog 检查点如实显示** — status: verified · `src/components/app/AppShell.tsx:162`（`checkpointExists={displayedPendingAction.checkpointHash != null}`）
- [x] **FE-P0-2 Dashboard 首屏 Agent 检测** — status: verified · `src/components/app/AppShell.tsx:272-275`（projectId effect）+ `:253-257`（陈旧写入守卫）

### P1

- [x] **FE-P1-1 import 预览去 250ms 轮询改事件驱动** — status: verified · `src/lib/waitForTaskTerminal.ts` + `src/components/app/AppShell.tsx:309`
- [x] **FE-P1-2 ConfirmationDialog destructive 按钮 variant 对齐** — status: verified · `src/components/ui/button.tsx:14` + `src/components/app/ConfirmationDialog.tsx:128`

## 关键决策（动手前）

- **FE-P0-1**：`checkpointHash` 后端 serde 已 camelCase 透传且 None→null（`#[serde(default)]` 不 skip），TS 接口已是 `checkpointHash?: string | null`。前端只改 AppShell 一处：`checkpointHash != null`（null 与 undefined 都判 false）。无需碰 ConfirmationDialog 组件（它已正确按 `checkpointExists` 渲染）。
- **FE-P0-2**：在 `WorkspaceView` 加独立 effect，依赖 `[currentProject.projectId, hasTauri, refreshCapabilities]`，调既有 `refreshCapabilities`。与 agent/settings effect 并存（那两个保留，切到对应视图时刷新合理）。jsdom 无 `__TAURI_INTERNALS__` → effect 不触发 invoke，不破坏现有测试。
- **FE-P1-1**：抽 `waitForTaskTerminal(task: BackendTask)`——接收 task 对象，已 terminal 立即 `Promise.resolve`（保原轮询"started 已 terminal 则不进循环"语义、省一次 get_task、让 App 集成测试不需 mock get_task/listen）；否则订阅 3 个 terminal 通道按 `event.taskId === task.id` 过滤 + race-safe `get_task` 初始检查。`settled` 守防重复 resolve，listener resolve 后若已 settled 立即 unlisten。
- **FE-P1-2**：Button cva 加 `danger` variant（与现有视觉 1:1，`--danger` token 已存在），ConfirmationDialog 改 `variant={isDestructive ? "danger" : "secondary"}` 去 className 覆盖。accessible name 仍由 confirm label 文本提供，满足 roadmap"label 或 variant 对齐"。

## 进度日志

- 2026-06-29 建账本；读 roadmap + cross-cutting-be.md + AppShell/ConfirmationDialog/types/button/useTaskEvents，确认后端已收敛、前端遗留 4 项、TS checkpointHash 已就绪。
- 2026-06-29 FE-P0-1 落地 + appShellActions checkpoint 接线测试（available/null/undefined）。commit `310f4b6`。
- 2026-06-29 FE-P0-2 落地 + 首屏 agentRoute 刷新测试（hasTauri + mock detect_agents → agentRoute="agent"，非假阳性）。commit `4575f68`。
- 2026-06-29 FE-P1-1 落地：waitForTaskTerminal helper（先 taskId 后改 task 入参，保已 terminal 短路语义，修 App 集成测试 get_task 未 mock 挂起）+ 5 单测 + AppShell 接线 + 移除局部 isTerminalTask。commit `32e4a9a`。
- 2026-06-29 FE-P1-2 落地：Button danger variant + ConfirmationDialog 用之 + destructive/非 destructive 样式断言。commit `e9a240e`。
- 2026-06-29 双子代理审查（A 共享上下文 / B 零偏见）→ 无 BLOCKER、测试非假阳性；合并修 3 项（陈旧写入守卫 / get_task null 守卫 / undefined 字段测试）。commit `f1d0bff`。
- 2026-06-29 收敛：npm test 163/31 全绿、npm lint 0 warning、无 console.log、未碰 src-tauri/ 与 UI-Frontend-design/。延后项记 roadmap。
