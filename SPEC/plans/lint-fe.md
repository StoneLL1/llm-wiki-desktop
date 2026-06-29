# Lint 前端壳 P0+P1 进度账本（loop: lint-fe）

> 对照源：SPEC/roadmap/lint.md（§1 组件表 P0/P1 行、§2 PRD-LINT-001/002/003、§5 实施顺序）、SPEC/PRD.md、UI-Frontend-design/lint.html + assets/app.css（只读，禁改）、CLAUDE.md
> 范围：仅 `src/` 与 `src/styles.css`。后端缺口标 blocked，不动 src-tauri/、不动 UI-Frontend-design/。
> 前提：lint-be P0+P1 已 verified（severity error、apply_fixes_batch、lint-ignore 持久化，commits b2689be/cdc6f17/ab8b77a/8460329）。本 loop 是前端配套。
> 验证：`npm run test` + `npm run lint` 全绿；无 console.log。

## 后端契约（来自 lint-be，本 loop 消费）
- `run_local_lint` → `LintReport`（DeadLink/IndexDrift 已为 `severity: error`）。
- `apply_lint_fixes`（批量）：`ApplyLintFixesBatchRequest{projectId, projectRootPath, issues, expectedHashes: {path->sha256}}` → `LintBatchOutcome{checkpoint?, applied[], needsConfirmation[{issue, pendingAction}], skipped[{issueId, path, reasonCode, reason}]}`。一次 Git 检查点覆盖全部 safe 写盘；high-risk 收集确认（命令层已注册 pending action）。safe 写盘缺 hash → skip `LINT_FIX_HASH_REQUIRED`。
- `apply_lint_fix(confirm=true, action_id)` 逐条确认 high-risk（既有路径，复用）。
- `add_lint_ignore`/`remove_lint_ignore`/`list_lint_ignores`：`{projectId, projectRootPath, path, rule}` → `LintIgnoreFile{ignored[{path, rule, createdAt}]}`。key=(path, rule)。run_local_lint 自动跳过被忽略项。

## 关键决策
- **mode 过滤**：store 加 `mode: "all"|"local"|"agent"`；LintView 按 source 过滤后传 list；seg 计数 = total / local / agent。摘要卡 error/warning/info 跟随 mode；passed 卡恒取 localReport 规则集。
- **passed 派生**：从 localReport.issues 反推未触发的确定性规则（frontmatter/index/duplicate_filename/missing_resource/path_case）→ 已通过 badge 行。无 localReport 时 passed 区不渲染。
- **批量 hash 收集**：`LintIssue` 无 hash 字段；批量前对 `fixability==="safe"` 的唯一 path 逐个 `read_wiki_page` 取 `meta.hash` 组 `expectedHashes`。safe 通常很少，可接受 N 次读。
- **批量后 high-risk 队列**：`needsConfirmation` 存 `batchConfirmations[]`；banner 列出待确认项，点选某项 → `selectIssue` + 置 `fixConfirm`（复用既有内联确认 UI）。不自动推进，用户驱动。
- **修复方案 radio**：每条 issue 给两选项——`应用修复`（fixability!=="none" 时默认选；high-risk 走既有确认流）/`忽略本次`。选"忽略本次"时底部 CTA 变"写入 lint-ignore"，调 add_lint_ignore 后重跑 local lint。
- **安全检查 checkbox 真值化**：`修复前 Git 检查点` 与 `修复后立即提交` 是硬边界（后端 create_scoped_checkpoint 必做且即提交）→ 渲染 **checked + disabled**（如实反映"不可关闭数据安全"，绝不给用户关掉检查点的错觉）。`完成后重编译` 真接线：勾选且修复成功后 fire-and-forget `start_wiki_compile`（既有命令，CompileRequest 最小参）。偏好持久化用 localStorage（settings_service 的 lint 偏好为后端 blocked 项）。
- **CSS 复用**：`.sumcard*`/`.seg`/`.seg__btn`/`.badge*`/`.checkbox`/`.btn*` 已在 styles.css；新增 `.check-row`、`.badge--info`、`.btn--block`。
- **token bug 修复**：`LintIssueList.tsx` 原用 `var(--error)`（不存在）→ 改 `var(--danger)`；info 用 `var(--info)`。

## status: pending|in_progress|done|verified|blocked

| id | 优先级 | 条目 | 状态 |
|---|---|---|---|
| L1 | P0 | 批量"自动修复 (N)" CTA + Git 检查点确认对话框（apply_lint_fixes） | verified |
| L2 | P1 | 摘要卡四宫格（error/warning/info/passed，消费 error 级） | verified |
| L3 | P1 | 已通过区（passed badge 行） | verified |
| L4 | P1 | 模式分段 all/local/agent + 计数 + 过滤 | verified |
| L5 | P1 | issue tags + 内联修复/查看详情按钮 + 修 --error token bug | verified |
| L6 | P1 | 修复方案 radio（check-row）+ lint-ignore "忽略本次" UI | verified |
| L7 | P1 | 安全检查 checkbox（硬边界 disabled + recompile 接 compile） | verified |

## ✅ 本轮完成 @ 2026-06-29

七项 P0+P1 全部 verified。`npm run test`（152）✅；`npm run lint`（0 warning）✅；`tsc --noEmit` ✅；无 console.log。浏览器预览确认 app 干净启动（lint 视图为 Tauri 门控，需后端 invoke；组件级验证走 jsdom 测试）。双子代理审查后修了 1 个真 BLOCKER（单条 safe 修复缺 hash）+ 数个 SHOULD-FIX。

### 摘要
- **L1 批量自动修复**：顶栏"自动修复 (N)"主 CTA → `LintBatchConfirmDialog`（Git 检查点提示）→ 收集 safe 路径 hash → `apply_lint_fixes`（一次检查点）→ 应用/跳过/待确认分类提示 + high-risk 待确认 banner（点选进入既有内联确认流）。
- **L2 摘要卡四宫格**：`LintSummaryCards`（error 红/warning 黄/info 蓝/passed 绿，跟随 mode 计数；passed 取 localReport 规则集）。
- **L3 已通过区**：`LintPassedSection`（绿底 check badge 行，反推未触发的确定性规则）。
- **L4 模式分段**：`mode` store 字段 + `.seg` 控件（全部/本地/Agent 深度，各自计数 + aria-pressed）+ list/summary 按 mode 过滤。
- **L5 issue 卡片**：三列网格（icon/title+sub+tags/actions）+ severity/issueType/source/可自动·高风险 tags + 内联"修复"（safe）/查看详情（high_risk）按钮；修 `var(--error)`→`var(--danger)` token bug；卡片改 `div[role=button]` 避免嵌套 button。
- **L6 修复方案 radio + 忽略**：`check-row` radio（应用修复 / 忽略本次）+ `add_lint_ignore` 接线 + 重跑 local lint。
- **L7 安全检查 checkbox**：修复前检查点 / 修复后提交 = **checked+disabled**（硬边界，如实反映不可关闭）；完成后重编译 = 真接 `start_wiki_compile`（勾选且修复成功后触发）；偏好 localStorage 持久化。

### 审查修复（双子代理）
- **BLOCKER（单条 safe 修复缺 hash）**：后端 `write_missing_frontmatter_fix` 对 safe 修复**强制** `expected_hash`（缺则 `LINT_FIX_HASH_REQUIRED`）。原 `applyFix` 传 `null` → safe 修复必失败。修：`applyFix` 增加 `expectedHash` 入参；`handleApplyFix` 对 safe issue 先 `read_wiki_page` 取 `meta.hash` 再调用。两审查子代理独立命中同一根因。
- **SHOULD-FIX**：批量 `skipped` 在 notice 中附 reason 文案（不再"看着成功"）；忽略成功后 `selectIssue(null)` 避免详情面板悬空；mode 切换清 notice；seg `role="tablist"`→`role="group"`+`aria-pressed`（无 tabpanel 不假装 tablist）。
- 误报已核驳：批量只对 safe 收集 hash 是 by-design（high-risk 走确认流，不写盘）；`refreshAfterFix` 的 recompile 门控正确（needs_confirmation 不触发）。

### 文件清单（仅 src/ + styles.css + i18n + 测试）
- `src/types/lint.ts`：LintMode / 批量 DTO / ignore DTO / LintSafetyPrefs / LintFixChoice。
- `src/stores/lintStore.ts`：mode/batch/ignore/safetyPrefs 状态与 actions；confirmHighRisk·cancelHighRisk 修剪 batchConfirmations；applyFix 增 expectedHash 入参；safetyPrefs localStorage 持久化（checkpoint 恒 true）。
- `src/features/lint/LintView.tsx`：seg + 自动修复 CTA + 确认对话框 + 摘要卡 + 已通过区 + high-risk banner + ignore/recompile 接线 + safe hash 收集。
- `src/features/lint/LintIssueList.tsx`：三列卡片 + tags + 内联按钮 + token bug 修复。
- `src/features/lint/LintIssueDetails.tsx`：修复方案 radio + 安全检查 checkbox + ignore 接线。
- `src/features/lint/LintSummaryCards.tsx`、`LintPassedSection.tsx`、`LintBatchConfirmDialog.tsx`：新组件。
- `src/styles.css`：`.check-row` / `.badge--info` / `.btn--block` / `.issue-card*` / `.lint-passed*` / `.sumcard--lint`。
- `src/i18n/locales/en.json`、`zh-CN.json`：lint.mode/.summary/.passed/.tag/.card/.batch/.plan/.safety 等 key。
- `src/features/lint/lintView.test.tsx`、`src/stores/lintStore.test.ts`：扩展测试（摘要卡 / 自动修复 CTA / 批量 / ignore / mode / safetyPrefs / safe hash 转发）。

## blocked（后端，非本 loop）
- B1: settings_service 持久化 lint 安全偏好（FE 用 localStorage 兜底；后端未提供 lint settings key）。

## 未做（显式留白，P2 / 非本 loop scope）
- 详情头部 `ico-xl` + `rightpanel__meta` `<dl>` 重构、行级 diff（`diff__line--add/del/ctx`）、Agent 建议独立卡片 → P2。
- 状态栏 lint 专用状态（待确认数/上次检查/Git hash）、列表方向键导航、本地规则开关 → P2。

## blocked（后端，非本 loop）
- B1: settings_service 持久化 lint 安全偏好（FE 用 localStorage 兜底；后端未提供 lint settings key）。
