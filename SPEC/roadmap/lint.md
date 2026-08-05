# Lint 板块落差与实施计划

> 对照源：UI-Frontend-design/lint.html + assets/app.css + SPEC/PRD.md（§8.7、§9.8、Phase 4）
> 当前实现：src/features/lint/、src/stores/lintStore.ts、src-tauri/src/services/lint_service/、src-tauri/src/commands/lint_commands.rs
> Workflows 迁移边界：[`../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md) 规定“健康检查”只读并把结果交给 Lint，现有 Lint 结果与修复页首轮保持不变。本文件的 Lint UI 打磨项是独立后续工作，不得夹带进 Workflows 迁移。
> 项目访问边界：[`../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)。本地只读 Lint 可在 restricted 模式对有限深度的可读 Markdown 运行且不落盘；Agent 深度检查需要信任；任何修复都需要 trusted + writable，危险/批量修复必须有 Git checkpoint 和用户确认。

## 0. 现状摘要

Lint 板块已具备完整的**双层骨架**且大多落在设计稿与 PRD 要求之内：

- **后端确定性规则真实跑通**（非 mock）：`LintService::run_local_lint` 在 `src-tauri/src/services/lint_service/rules.rs` 内扫描 wiki，检出 8 类问题——死链、孤立页、缺 frontmatter、空页面、重复文件名、路径大小写、缺失资源、index.md 漂移；同模块测试覆盖每条规则。
- **Agent 深度 Lint 已接入**：`start_deep_lint`（`lint_commands.rs:39-74`）走 `wiki-lint` Skill；当前使用自动 Agent/BYOK 路由，Workflows 迁移时需遵循设置默认路径或单次显式覆盖且不得静默回退。任务支持取消，并把 ```json``` 结构化输出解析到 `.app/lint-reports/<task_id>.json`。
- **修复闭环打通**：safe 修复（缺 frontmatter）直接落盘；high-risk（死链、index 漂移）返回 `PendingAction` → 前端内联确认 → 二次调用 `apply_lint_fix` 带 `confirm_high_risk + expectedHash` 才写盘。所有写操作前先 `create_scoped_checkpoint`（`src-tauri/src/services/lint_service/fixes.rs`），用 `OverwriteIfHashMatches` 乐观锁防护，并清理 graph-cache、追加 `wiki/log.md`。
- **路径安全现状**：`LintService::apply_fix` 在 `src-tauri/src/services/lint_service/fixes.rs` 拒绝任何不以 `wiki/` 开头或含 `..` 的 fix 路径。`wiki/` 硬编码只覆盖原生布局；目标需改为由 `ProjectContext.layout` 提供允许的 Markdown roots，并继续做 canonical containment。

**核心缺口集中在 UI 层**：当前 `LintView` 仅是功能化的“列表 + 详情 + 修复按钮”，设计稿定义的摘要卡、模式分段（全部 / 本地 / Agent 深度）、批量修复、修复方案 radio、安全检查 checkbox、diff 预览、已通过区域等均未呈现。另外后端从不发出 `severity: error` 级别问题（全部为 warning/info），导致设计稿“错误 2”摘要卡无数据支撑。

## 1. 区块 / 组件清单

| 区块/组件 | 设计稿要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 顶栏标题 + 问题计数副标题 | `Lint · 4 个问题 · 1 待确认` | 无独立标题行，仅工具栏 | 🟡部分实现 | P2 | `src/features/lint/LintView.tsx:108-140` |
| 模式分段控件（全部/本地/Agent 深度） | `seg[data-seg=lintmode]` 三档计数切换 | 无分段，仅文本计数 | ❌缺失 | P1 | `src/features/lint/LintView.tsx:136-139` |
| “重新检查”按钮 | 刷新本地 + 保留 Agent 报告 | “Run local lint” 按钮（等价） | 🟡部分实现 | P2 | `src/features/lint/LintView.tsx:111-118` |
| “自动修复 (N)” 批量按钮 | 一键批量修复所有可自动 issue | 无批量入口，只能逐条 Apply | ❌缺失 | P0 | `src/features/lint/LintView.tsx` |
| 摘要卡（错误/警告/建议/已通过 四宫格） | 顶部 `sumcard` 行，错误红/警告黄/建议蓝/已通过绿 | 无摘要卡 | ❌缺失 | P1 | `src/features/lint/LintView.tsx:146-150` |
| 已通过检查区 | 列表底部绿色 badge 条罗列通过的规则 | 无 | ❌缺失 | P1 | `src/features/lint/LintIssueList.tsx` |
| Issue 卡片（icon + 标题 + 路径 + tags + “修复”/“查看详情”） | 三列网格，悬停高亮、选中左边框 + accent 背景，tags 含 severity/source/可自动 | 简化的 button 列表：icon + 类型 label + source + 路径；**缺 target/evidence 一行、tags、可自动 badge、右侧“修复”按钮** | 🟡部分实现 | P1 | `src/features/lint/LintIssueList.tsx:66-95` |
| Issue 分组小节标签 | 设计稿隐含按 severity 分组 | 按 `severity:source` 分组，含计数 | ✅已完成 | — | `src/features/lint/LintIssueList.tsx:33-47` |
| 详情头部（大 icon + 类型 + 副标题） | `ico-xl` + 14px/600 标题 + mono 副标题 | 简单 13px 标题 + 两个 muted badge，**缺大图标与副标题** | 🟡部分实现 | P2 | `src/features/lint/LintIssueDetails.tsx:78-89` |
| 详情 `rightpanel__meta` 键值表（类型/检测层/相似度/建议/风险） | `<dl>` 风格的元数据列 | 自定义 `Row` 组件覆盖 path/message/target/line/evidence/suggestedAction；**缺“检测层/相似度/风险”字段** | 🟡部分实现 | P2 | `src/features/lint/LintIssueDetails.tsx:92-117` |
| 差异预览（diff）| 逐行 diff，`diff__line--ctx/add/del` 三色 | 仅有 `preview.before/after` 两列纯文本，**无行级 diff 着色** | 🟡部分实现 | P2 | `src/features/lint/LintIssueDetails.tsx:129-148` |
| Agent 建议卡片 | 独立 section，灰底框展示模型自然语言建议 | 仅当 `issue.suggestedAction` 存在时单行渲染 | 🟡部分实现 | P2 | `src/features/lint/LintIssueDetails.tsx:112-114` |
| **修复方案多选 radio**（合并 / 保留加 cross-ref / 忽略）| `check-row` 带 radio + 影响摘要 + 风险 badge | **完全缺失**，只有单一 Apply 入口 | ❌缺失 | P1 | `src/features/lint/LintIssueDetails.tsx:166-178` |
| 安全检查 checkbox（检查点/修复后提交/重编译）| 3 个 checkbox，前两个默认勾选 | **完全缺失**；检查点行为固定在后端，用户不可选 | ❌缺失 | P1 | `src/features/lint/LintIssueDetails.tsx` |
| “忽略本次” / lint-ignore 机制 | radio 项：写入 lint-ignore，后续不再报告 | **前后端均无** ignore 持久化 | ❌缺失 | P1 | `src-tauri/src/services/lint_service/ignores.rs`、`src/features/lint/LintIssueDetails.tsx` |
| 高风险内联确认 + diff 预览 | 大 confirm 面板带 before/after | 已实现 before/after 双列，带 pageHash 乐观锁 | ✅已完成 | — | `src/features/lint/LintIssueDetails.tsx:123-166`、`lintStore.ts:181-217` |
| 取消 / 应用修复 双按钮 | 底部 `btn--block` 两个 | 已有 | ✅已完成 | — | `src/features/lint/LintIssueDetails.tsx:149-165` |
| 状态栏“待确认数 / 上次检查时间 / Git 检查点 hash” | footer 四条状态项 | 无（状态栏在 AppShell，但未显示 lint 专用状态）| ❌缺失 | P2 | `src/components/app/AppShell.tsx` |
| 本地规则开关 | PRD §9.8 隐含可配置 | **无规则启用/禁用 UI**，所有本地规则硬编码常开 | ❌缺失 | P2 | `src/features/lint/LintView.tsx` |
| 项目访问 / 兼容布局 | restricted 可本地只读扫描；深度检查需信任；修复需可写和 checkpoint | 当前扫描/修复主要硬编码 `wiki/`，未区分 trust/access/layout | ❌缺失 | P0 | `lint_commands.rs`、`lint_service/`、项目上下文 DTO |

## 2. 功能落差（PRD 对照）

- [ ] **项目访问与布局（P0）**：本地 Lint 消费后端允许的 Markdown roots，可在 restricted 模式有限深度内存运行且不写 report/cache；Agent 深度 Lint 在外发内容前要求信任；任何修复在后端重验 canonical identity、trusted、writable、expected hash 与 Git policy。兼容/只读项目仍能看到问题，但修复动作替换为权限说明或“信任知识库”。
- [ ] **PRD-LINT-001 / 错误（error）级别**：设计稿摘要卡期望“错误 2（死链·必修）”，但 `LintService::run_local_lint` 在 `src-tauri/src/services/lint_service/rules.rs` 内只发出 `LintSeverity::Warning` 或 `Info`，**从不产生 error**。现状 → 死链目前是 warning。目标 → 将确定性死链以及布局实际提供 `wikiIndexPath` 时的索引漂移等“必修”问题升级为 `error`（或引入规则配置允许用户分级）；没有 Wiki 索引路径的 Source-only 项目将索引规则标记为 N/A，不得报错。涉及 `src-tauri/src/services/lint_service/rules.rs`。验收：摘要卡“错误”计数与当前 layout 适用规则一致。
- [ ] **PRD-LINT-003 / 批量自动修复**：`lint.html` 工具栏有 `自动修复 (3)` 按钮，当前实现仅支持逐条 `apply_lint_fix`，无批量入口、无“一次 Git 检查点 → 逐条应用 → 一次提交”编排。现状 → 每条 fix 各自一次 checkpoint。目标 → 新增 `apply_lint_fixes`（批量）后端命令：一次性 scoped checkpoint，逐条按 fixability 分流，safe 立即写、high-risk 收集 PendingAction 统一确认。涉及 `src-tauri/src/commands/lint_commands.rs`、`src-tauri/src/services/lint_service/fixes.rs`、`src/stores/lintStore.ts`、`LintView.tsx`。验收：一键按钮触发，结束后单次提交、单次回滚可用。
- [ ] **PRD-LINT-002 + Agent 建议 UI**：Agent deep-lint 已能解析 6 类 issue，但 `LintIssueDetails` 没有“Agent 建议卡片”独立区——`suggestedAction` 仅作普通 Row。现状 → 建议混在元数据中。目标 → 单独 section，灰底框，保留模型自然语言原文。涉及 `src/features/lint/LintIssueDetails.tsx`。验收：Agent issue 渲染独立建议块。
- [ ] **修复方案多选 + lint-ignore**：设计稿详情面板允许“合并 / 加 cross-ref / 忽略本次”三选一。现状 → 后端 `apply_fix` 仅针对 `MissingFrontmatter / DeadLink / IndexDrift` 三类有确定性路径，其余一律 `LINT_FIX_NOT_AUTO`；无 ignore 持久化。目标 → (1) 前端把多种修复策略作为 `fixOptions[]` 列在 issue 上（由后端在 `LintIssue` 增加 `available_fixes` 字段返回）；(2) 项目 app state 可写且 `ProjectLayout.lintIgnorePath` 存在时记录被忽略 issue 的稳定 key（原生映射为 `.app/lint-ignore.json`），`run_local_lint` 跳过；只读项目只提供本次会话内忽略。涉及 `src-tauri/src/models/lint.rs`、`src-tauri/src/services/lint_service/{fixes,ignores,rules}.rs`、`src/types/lint.ts`、`LintIssueDetails.tsx`。验收：用户可选策略；可持久化项目中被忽略的 issue 不再出现于下次扫描。
- [ ] **模式分段筛选（全部/本地/Agent 深度）**：设计稿顶栏 seg 控件按 source 过滤并各自计数。现状 → 始终合并展示。目标 → 增加 `mode: "all" | "local" | "agent"` store 字段，`LintView` 顶栏 seg 控件，列表按 mode 过滤。涉及 `src/stores/lintStore.ts`、`LintView.tsx`、`LintIssueList.tsx`。验收：切换可正确过滤且计数准确。
- [ ] **安全策略如实展示**：旧设计稿把“修复前检查点”画成可选 checkbox，但当前硬边界不允许用户关闭危险/批量修复检查点。目标 → 把必需检查点渲染为只读策略状态；“修复后立即提交”和“完成后更新 Wiki”如保留，必须是独立可选行为，且后者进入 Workflows 准备页而非自动运行。验收：没有任何 UI 或 API 能绕过必需 checkpoint，checkpoint 失败时零写入。

## 3. 视觉 / 设计 token 落差

- **摘要卡**：设计稿用 `.sumcard`（`app.css:975-1010`）— 12px padding、22px value、`--danger/--warning/--info/--accent-hover` 配色；当前实现完全没有摘要卡。`LintView` 顶栏仅是 44px 工具条 + 文本计数（`LintView.tsx:136-139`）。
- **Issue 卡片**：设计稿 14px padding、`border-bottom`、hover `--surface-hover`、选中 `--accent-soft` + 左 2px accent 边；当前实现用 `border-l-2` + `--surface-muted`，**路径字号 11px 符合 mono 11.5px 规范，但缺少 tags 行与右侧“修复/查看详情”按钮列**。颜色用 `--error` token，而 `styles.css` 中实际变量名是 `--danger`（参考 `app.css:732`），**SEVERITY_COLOR 可能引用了不存在的 CSS 变量**（`LintIssueList.tsx:21` 用 `var(--error)`，需核实 `src/styles.css` 是否定义）。
- **详情头部**：设计稿 `ico-xl`（24px+）+ 14px/600 + 11px mono 副标题；当前实现无大图标、无副标题，密度偏低（`LintIssueDetails.tsx:78-89`）。
- **diff**：设计稿 `.diff__line--add/del/ctx`（`app.css:1230-1232`）有绿/红/灰三色背景；当前实现仅是两列纯文本 `<code>`，无行级 diff。
- **check-row**：设计稿的 radio 行有 `--accent-soft` 选中底色、`--border` 边框、12px 字号（`lint.html:49-59`）；当前实现无等价组件。
- **rightpanel__meta**：设计稿 `<dl>` 风格（`app.css:524-534`）；当前实现用 flex 列 + uppercase 小标签，视觉密度接近但语义不同。

## 4. 交互 / 可访问性落差

- **键盘导航**：设计稿隐含列表可上下键切换、详情面板可读。当前 issue card 是 `<button>`（可聚焦/Enter 激活），但**缺方向键导航、缺 `aria-current`/`aria-selected`**。
- **模式分段控件**：应使用 `role="tablist"` + `aria-selected`，当前完全缺失。
- **批量修复确认**：设计稿隐含“高风险批量需统一确认对话框”，当前无批量入口故无此交互。
- **diff 可读性**：纯文本 before/after 对照对屏幕阅读器不友好；行级 diff + `role="del"/"ins"` 更合适。
- **进度反馈**：`loadingLocal` 仅把按钮文案换成 `…`，无进度条/spinner；Agent deep lint 依赖全局 TaskLogDrawer，**LintView 本身无进度可视**（设计稿 footer 有“上次检查 2 分钟前”，未实现）。
- **错误恢复**：`lintStore.error` 仅在顶部一行 warning-soft 横幅展示（`LintView.tsx:141-145`），无重试按钮。

## 5. 建议实施顺序

1. **P0 · 项目访问与布局**：restricted 本地只读、trusted 深度检查、trusted writable 修复与兼容 roots。
2. **P0 · 批量自动修复**：后端 `apply_lint_fixes` + 前端“自动修复 (N)”按钮；单次 checkpoint、统一确认、失败零写入。
3. **P0 · error 级别修正**：将死链/index 漂移升级为 `severity: error`。
4. **P1 · 摘要卡 + 已通过区**：顶部四宫格 + 列表底部绿色通过条。
5. **P1 · 模式分段控件**：`mode` store 字段 + seg 控件 + 计数。
6. **P1 · Issue 卡片 tags + 右侧修复按钮**。
7. **P1 · 修复方案 radio + lint-ignore**：扩展 `LintIssue.available_fixes`，新增 `.app/lint-ignore.json`。
8. **P1 · 安全策略状态**：显示必需 checkpoint；可选后续动作不自动触发 Workflows。
9. **P2 · 详情头部重构 + 行级 diff + Agent 建议卡片**。
10. **P2 · 状态栏 lint 状态 + 键盘 a11y**。
11. **P2 · 本地规则开关**。
