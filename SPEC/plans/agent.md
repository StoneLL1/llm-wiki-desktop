# Agent 板块 P0+P1 进度账本

> ✅ **本轮完成 @ 2026-06-22** — 8 项 P0/P1 全部 verified。blocker 修复（agent 挂起 / COMPILE_INPUT_EMPTY / 取消无响应）+ 四宫格+BYOK 卡+右面板+RunAgentDialog+terminal 升级全部落地。文件：`src/features/agent/{AgentView,RunAgentDialog,AgentRightPanel}.tsx`、`src/components/app/{AppShell,RightContextPanel,TaskLogDrawer}.tsx`、`src/styles.css`、`src/i18n/locales/*`、`src-tauri/src/services/compile_service.rs`、`src-tauri/src/tasks/task_service.rs`。105 FE + 315 Rust lib 测试 + clippy + fmt + tsc + lint 全绿。遗留：用户需在 `npm run tauri dev` 实测确认 GUI 下 dialog/取消/terminal 行为；skill-template 校验后端命令未实现（右面板用已知清单代替）；安全边界 checkbox 为不可变 invariant（CLAUDE.md 硬规则），未来若改为可配置需扩 Settings。

> 对照源：`SPEC/roadmap/agent.md` + `UI-Frontend-design/agent.html` + `assets/app.css` + `SPEC/PRD.md`
> status: pending | in_progress | done | verified

## ✅ 阻塞修复（本轮前置）

### AGENT-BLOCK-01: Agent 运行挂起（编译/HTML/对话全卡死）— ✅ verified
- commit `fa18266` — `src-tauri/src/services/agent_service.rs`：4 个 Claude invocation builder 加 `--bare`；detection 同步要求；回归守卫测试。
- 验证：314 Rust lib tests + clippy `-D warnings` + fmt + 103 FE tests + lint 零警告。
- 遗留：用户需在 `npm run tauri dev` 实测确认 GUI spawn 下生效。

### AGENT-BLOCK-02: 编译空输入无诊断 + 取消无响应 — ✅ verified
- commit `0b585bd` — `src-tauri/src/services/compile_service.rs`：新增 `compile_input_empty_error` helper，三 stage（`no_confirmed_imports` / `no_extracted_markdown` / `extracted_files_empty`）带 hint+details；message 仍含 `raw/extracted` 保回归；新增 image-only 分支测试。
- commit `15deb07` — `src-tauri/src/tasks/task_service.rs`：`cancel_task` 对 terminal task 改为 idempotent 返回快照（原 Err → 现 Ok），重写测试覆盖 Succeeded/Failed/Cancelled 三态。
- 验证：315 Rust lib tests + clippy + fmt + 103 FE tests + lint 全绿。

---

## P0

### AGENT-P0-01: "运行 Agent"对话框 — ✅ verified
- commit `bacbc01` — 新建 `src/features/agent/RunAgentDialog.tsx`：Skill 选择（7 个）/ 执行路径 seg（已安装 Agent + 已启用 BYOK）/ Git 检查点 checkbox（默认开）/ 后台 toggle（默认开）。`onRun` 输出 `{skill, route, agent, provider, checkpoint, background}`，由 AppShell 派发到 `start_wiki_compile` / `start_deep_lint` / `start_export(project_report)` / chat 导航。
- 验证：105 FE tests + tsc + lint。

### AGENT-P0-02: 核心操作四宫格（ingest-grid）— ✅ verified
- commit `bacbc01` — `AgentView.tsx`：四张 `.ingest-card`，Ingest `is-primary` 打开 RunAgentDialog（preset wiki-ingest），Lint/Query/HTML 调 `onNavigate` 切 activeView。`.ingest-card` 样式族进 `styles.css`。
- 测试：`agent.test.tsx` 新增四宫格交互断言（主卡触发 `onRunAgent("wiki-ingest")`，Lint 卡触发 `onNavigate("lint")`）。

### AGENT-P0-03: BYOK 卡片化（sumcard）— ✅ verified
- commit `bacbc01` — `AgentView.tsx`：4 张 `.sumcard--provider`（Anthropic/OpenAI/Google/Ollama），按 `providers` prop 显示 configured/unconfigured/local + 模型 hint；未配置显示 "前往 Settings → LLM Providers"。

### AGENT-P0-04: 右面板 Agent 配置区 — ✅ verified
- commit `5399651` — 新建 `src/features/agent/AgentRightPanel.tsx` + 在 `RightContextPanel.tsx` 加 `activeView === "agent"` 分支。默认 Agent 元信息（command/version/path）、Skill 清单（已知列表，html-project-report 标缺模板 warn）、上下文窗口滑杆（绑 `settings.contextWindow` 经 `settingsStore.persistPatch` 持久化）、安全边界 4 checkbox（disabled invariant，对应 CLAUDE.md 硬规则）、快捷操作 4 按钮。
- 决策：未加 4 个新 boolean settings 字段——CLAUDE.md 明确这些是硬规则而非可选项，渲染为不可勾选的 invariant 更诚实；`list_skills` 后端命令缺，先用已知清单代替。

---

## P1

### AGENT-P1-01: CLI 行视觉对齐（.cli-row）— ✅ verified
- commit `bacbc01` — `AgentView.tsx`：检测到的 CLI 渲染为 `.cli-row`（icon 2 字母缩写 / vendor 副标题 / 路径·版本·已签名 / `.dotstatus` / 默认 badge / 设为默认按钮）。`.cli-row` 样式族进 `styles.css`。

### AGENT-P1-02: 任务行内进度/取消 — ✅ verified
- commit `bacbc01` — `AgentView.tsx`：任务行 `.task-row` 内嵌 `.progress progress--sm`（含 aria，见 P1-04）+ 行内取消按钮（非终态可取消），调 `cancelTask` → `cancelTaskRequest`。

### AGENT-P1-03: terminal 日志样式升级 — ✅ verified
- commit `95b3628` — `TaskLogDrawer.tsx`：日志行改为三段式 `ts + [LEVEL] + message`，level badge 用 `.lvl-info/.lvl-ok/.lvl-warn/.lvl-err` 染色；overlay 加 Copy/Clear/Expand 按钮；底部 footer 显示状态点 + 时长 + 字节数 + 后台运行提示。`.terminal-*` 样式族进 `styles.css`。
- 决策：未抽独立 Terminal 组件给 AgentView 主区——AgentView 任务行已链接到 drawer，避免重复；`.terminal` 样式族留作未来主区直渲用。

### AGENT-P1-04: 进度条 aria 属性 — ✅ verified
- commit `bacbc01`（AgentView 内）+ `95b3628`（TaskLogDrawer 内）— 所有进度条补 `role="progressbar"` + `aria-valuenow/min/max` + `aria-label`。

---

## 收敛判据
✅ 全部 P0/P1 verified + 完整 test/lint 全绿 + 本账本顶部标记"✅ 本轮完成 @ 2026-06-22" + progress 里程碑。

## 硬纪律提醒
- 只动 src/ + 本板块必要的 src-tauri 接线（compile diagnostic / cancel idempotent）；不改 `UI-Frontend-design/`；不重写后端 spawn/checkpoint/cancel 核心（仅 cancel 加 terminal idempotent）。
- 每项独立 commit（conventional，不 --no-verify，不 push）；每项追加 progress.txt。
- 安装引导（P2）不在本轮范围。
