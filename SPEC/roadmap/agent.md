# Agent 板块落差与实施计划

> 对照源：UI-Frontend-design/agent.html + assets/app.css + SPEC/PRD.md
> 当前实现：src/features/agent/、src/stores/taskStore.ts、src/components/app/{TaskLogDrawer,CompileConflictDialog}.tsx、src-tauri/src/{services/agent_service.rs,services/compile_service.rs,commands/compile_commands.rs,commands/lint_commands.rs,commands/agent_commands.rs}

## 0. 现状摘要

后端骨架真实落地，前端为薄壳 MVP：

- **后端（真实可用）**：`AgentService` 通过 `SystemProcessRunner::run_task_streaming` 真正 spawn 子进程（`Command::new` + `--print --output-format stream-json` 等），并把 stdout/stderr 逐行回投到 TaskService 日志，支持 `is_cancelled` → `child.kill()`。编译流 `start_wiki_compile`/`run_compile`/`generate_manifest` 已完整跑通 Agent/BYOK 双路：创建 Git 检查点 → snapshot 哈希 → 创建候选 workspace → 调 Agent/BYOK → 冲突时回灌 `PendingAction(CompileMerge)` → `confirm_compile_action`/`resolve_compile_conflict` 落盘 + 二次检查点 + graph-cache 失效 + search 重扫。deep-lint (`start_deep_lint`) 同样真实 spawn。
- **前端（workflow 已抽离）**：`RunAgentDialog` 已提供 Skill、执行路径、Git 检查点和后台运行选项；`useAgentWorkflow` 按 `wiki-ingest` / `wiki-lint` / `wiki-query` / `html-*` 路由到 compile、deep lint、Chat 或 Exports，`WorkspaceController` 负责对话框接线。当前缺口是 dialog 发出的 `checkpoint` / `background` 值没有进入 `TaskLaunchOptions` 或任何 command，因此这两个开关尚无实际效果。实时日志 terminal 等其余视觉落差见下表。

整体完成度约 **40%**：安全/正确性硬骨架已具备，UI 信息密度与交互形态与设计稿差距大。

## 1. 区块 / 组件清单

| 区块/组件 | 设计稿要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 顶栏 toolbar（重新检测 / 安装引导 / 运行 Agent） | 三按钮 + primary CTA 弹"运行 Agent"对话框 | 仅"重新检测"一个 ghost 按钮 | 🟡部分实现 | P1 | `src/features/agent/AgentView.tsx:30-33` |
| 已检测 Agent CLI 列表（cli-row） | 四列：图标/名称+vendor/路径·版本·签名/状态徽章+在线 dot/设为默认；默认行高亮 `is-default` + accent-soft 背景 | 渲染了基础行（图标+command+version/error/installGuidance+设为默认按钮），但无 vendor 副标题、无"已签名"标识、无在线/离线 dotstatus、无 is-default 高亮、缺 `.cli-row` CSS 类（自定义 div） | 🟡部分实现 | P1 | `src/features/agent/AgentView.tsx:34-54`，缺 CSS `UI-Frontend-design/assets/app.css:1879-1901` |
| BYOK 后备路径（summarygrid 卡片） | 4 张 sumcard：Anthropic/OpenAI/Google/Ollama，显示"已配置/未配置/本地"+ 模型 hint + 掩码 key | 仅一行数字 `providerCount` + 文案"个已配置提供商" | ❌缺失 | P1 | `src/features/agent/AgentView.tsx:65-69` |
| 核心操作四宫格（ingest-grid） | 4 张 ingest-card：Ingest/Lint/Query/HTML，每张带 icon+标题+描述+`claude → skill →` CTA | 仅一个"编译 Wiki"按钮 | ❌缺失 | P1 | `src/features/agent/AgentView.tsx:57-64` |
| 任务列表（task-row） | 行内显示：状态图标+标题+`claude · wiki-lint · started 14:31 · PID 8421`+进度条+查看日志/取消按钮；完成/失败有徽章和 diff 摘要（+4/-1） | AgentView 内只是简单两列按钮跳抽屉，真正的任务展示在 `TaskLogDrawer` 抽屉中（列表+日志+取消+进度条） | 🟡部分实现 | P1 | `src/features/agent/AgentView.tsx:71-81`，抽屉实现 `src/components/app/TaskLogDrawer.tsx:137-272` |
| 实时输出 terminal | 360px 高度的 `terminal` 区，时间戳+level 标签彩色高亮、cursor 闪烁、复制/清空/全屏 overlay、底部状态栏（PID·时长·KB·关闭窗口继续提示） | TaskLogDrawer 内有日志滚动区，但无时间戳 level 染色（只按 level 整行着色）、无 terminal 类名、无复制/清空/全屏、无底部 PID/KB/后台提示 | 🟡部分实现 | P1 | `src/components/app/TaskLogDrawer.tsx:38-54,196-266` |
| 运行 Agent 对话框（dlg-run） | Skill 下拉 + 执行路径分段（claude/codex/BYOK）+ Git 检查点 checkbox + 后台运行 toggle + "运行"按钮 | 对话框与 Skill/route dispatch 已实现；但 `useAgentWorkflow` 构造的 `TaskLaunchOptions` 只消费 route/agent/provider，丢弃 `checkpoint` / `background` | 🟡部分实现 | P0 | `src/features/agent/RunAgentDialog.tsx`、`src/features/agent/useAgentWorkflow.ts:115-154`、`src/components/app/WorkspaceController.tsx` |
| 右面板 "Agent 配置" | 5 个 section：默认 Agent 元信息、Skill 系统 checklist（含缺模板警告）、上下文窗口滑杆、安全边界 4 checkbox、快捷操作 4 按钮 | **完全缺失** | ❌缺失 | P1 | 无（`AgentView` 未渲染右面板） |
| PendingAction 确认流（编译冲突） | 设计稿未画具体形态，由 CLAUDE.md 硬约束派生 | `CompileConflictDialog` 已实现双栏 diff + 手动合并 + 保留/使用生成/取消，调 `resolve_compile_conflict` | ✅已完成 | — | `src/components/app/CompileConflictDialog.tsx:20-111` |
| Git 检查点（PRD 硬约束） | 危险操作前自动检查点 | `run_compile` 创建 `HighRiskOperation` 检查点，`finish_compile` 创建 `FinalResult` 检查点，`ensure_checkpoint_head` 防 HEAD 漂移，失败时 `unstage_paths`+`restore_outputs` 回滚 | ✅已完成 | — | `src-tauri/src/commands/compile_commands.rs:106-110,147-153,484,634-654` |
| 可取消 / 后台运行 | 任务可取消、关闭窗口继续 | `run_task_streaming` 每 50ms 查 `is_cancelled` → kill 子进程；TaskService 持久化使任务跨视图/重启存活；TaskLogDrawer 行内 + 详情页双重取消按钮 | ✅已完成 | — | `src-tauri/src/services/agent_service.rs:485-494`，`src/components/app/TaskLogDrawer.tsx:180-190,246-254` |
| 系统通知（完成后提醒） | 设计稿底部提示"完成后系统通知" | `useTaskEvents` 订阅通道并调 `notifyTaskEvent` 发 OS 通知，`registerNotificationActionListener` 处理通知按钮 | ✅已完成 | — | `src/hooks/useTaskEvents.ts:54-78` |
| 安装引导（不静默安装） | "查看安装引导"按钮（缺失 CLI 行）+ 顶栏"安装引导"按钮 | installGuidance 仅作纯文本展示（`AgentInfo.installGuidance`）；无"安装引导"弹窗或外链跳转；**无静默安装**符合硬约束 | 🟡部分实现 | P2 | `src/features/agent/AgentView.tsx:47`，`src-tauri/src/services/agent_service.rs:345-354` |

## 2. 功能落差（PRD 对照）

- [ ] **"运行 Agent"对话框（Skill 编排入口，P0）**：UI、项目隔离实例与 wiki-ingest/wiki-lint/wiki-query/html-* 的 route dispatch 已完成；剩余验收缺口是把 `checkpoint` / `background` 传入实际 task/command 语义，或在当前后端不支持时禁用并诚实说明，而不是保留无效果开关。
- [ ] **核心操作四宫格**：现状仅 Ingest 按钮 → 目标：Ingest/Lint/Query/HTML 四张卡，Ingest 高亮 `is-primary`，其余跳转对应视图（lint.html/chat.html/exports.html 对应 `activeView` 切换） → 涉及 `src/features/agent/AgentView.tsx:57-64` → 验收：点击 Lint 卡切换到 LintView；HTML 卡切换到 ExportsView；主卡触发运行对话框。
- [ ] **BYOK 卡片化展示**：现状 `providerCount` 数字 → 目标：4 张 sumcard 显示 Anthropic/OpenAI/Google/Ollama 的已配置/未配置/本地状态 + 模型 hint + 掩码 key（如 `sk-ant-···8f3a`）→ 涉及 `src/features/agent/AgentView.tsx:65-69`、需要扩展 props 把 `providers: LlmProviderConfig[]` 传入而非只传 count；密钥掩码需后端补 `list_secret_status` 之类的命令（当前 SecretService 只 `get`） → 验收：卡片区分状态，未配置卡显示"前往 Settings → LLM Providers"。
- [ ] **AgentView 任务行内进度**：现状 AgentView 任务区只显示标题+taskType+status 文字，进度条和取消按钮要去抽屉 → 目标：任务行直接嵌 `ProgressBar` + 行内"取消"按钮 + "查看日志"按钮一键开抽屉 → 涉及 `src/features/agent/AgentView.tsx:71-81` → 验收：running 任务在 AgentView 内可见百分比；行内取消调 `cancelTaskRequest`。
- [ ] **右面板 Agent 配置区**：现状无右面板 → 目标：默认 Agent 元信息（从 `get_agent_config`）、Skill 系统 checklist（需后端列 `.app/skills/` 或内置 Skill 清单 + 模板完整性检测——PRD 要求 html-project-report 缺模板要 warn）、上下文窗口滑杆（200K，BYOK 覆盖）、安全边界 4 checkbox（持久化到 settings.json）、快捷操作 4 按钮 → 涉及新建组件 `src/features/agent/AgentRightPanel.tsx`，可能需后端新增 `list_skills`、`check_skill_templates` 命令 → 验收：安全边界 checkbox 状态读写持久化；Skill 缺模板时黄色 warn 图标。
- [ ] **terminal 日志样式升级**：现状日志按 level 整行变色无 level 标签 → 目标：`ts + [LEVEL] + message` 三段式，INFO/WARN/ERR/OK 各色标签；复制/清空/全屏按钮 overlay；底部状态条显示 PID/时长/KB/后台提示 → 涉及 `src/components/app/TaskLogDrawer.tsx:38-54,196-266`、可能要把 terminal 模式同时暴露给 AgentView（设计稿把 terminal 画在主区而非抽屉） → 验收：日志可一键复制；清空只清前端视图不删持久化。
- [ ] **CLI 行视觉对齐**：现状自定义 div → 目标：使用设计稿 `.cli-row`/`.cli-row__icon`/`.is-default` 类与 token；补 vendor 副标题（Anthropic/OpenAI）、"已签名"字样、在线/离线 `dotstatus` → 涉及 `src/features/agent/AgentView.tsx:34-54`、`src/styles.css` 需新增 `.cli-row` 样式（参照 `UI-Frontend-design/assets/app.css:1879-1901`） → 验收：默认行 accent-soft 高亮；状态 dot 颜色随 state 变化。
- [ ] **安装引导入口（非静默安装）**：现状 installGuidance 纯文本 → 目标：缺失行点击"查看安装引导"弹出说明 + 一键复制命令（**不自动执行**，符合 CLAUDE.md 硬约束）→ 涉及 `src/features/agent/AgentView.tsx:47` → 验收：引导文本可复制；无任何自动 `npm install` 触发。

## 3. 视觉 / 设计 token 落差

- **缺 `.cli-row` 样式族**：设计稿 `app.css:1879-1901` 定义了 cli-row 的 grid 布局、`is-default` 的 accent-border + accent-soft、`__icon` 圆角块、`__path` 的 mono 字体。当前 AgentView 用临时 Tailwind 类拼装，视觉密度与设计稿差距明显。需在 `src/styles.css` 补齐。
- **缺 `.ingest-card` 样式族**：设计稿在 `agent.html:46-62` 内联定义，四宫格卡片的 hover 边框过渡、`is-primary` 的 accent-soft 背景、`__icon`/`__title`/`__desc`/`__cta` 排版当前完全没有。
- **缺 `.dotstatus` + `.dot`**：设计稿 `app.css:760-769` 定义了 ok/busy/err 三色 dot + 2px soft 阴影，CLI 行/顶栏/状态栏都需要。当前完全未使用。
- **缺 `.progress--sm`**：设计稿 `app.css:1262` 定义了 sm 进度条 3px 高，当前 ProgressBar 组件 `h-1`（4px）。
- **缺 `.sumcard`**：BYOK 卡片样式完全未引入。
- **字号对照**：AgentView 现用 `text-[12px]`/`text-[13px]` 基本对齐，但缺 section 标签的 `10.5px uppercase letter-spacing 0.08em`（设计稿 `.agent-section__title`）——当前 panel-header 复用了通用样式，未实现大写小标签形态。

## 4. 交互 / 可访问性落差

- **键盘导航**：运行 Agent 对话框已通过 `useModalDialog` 支持 Esc 与焦点约束，并使用 `role="dialog" aria-modal`。当前 `CompileConflictDialog` 已有 `role="dialog" aria-modal`，但 `AgentView` 的任务行用 `<button>` 包裹整行，屏幕阅读器会把整行当一个按钮，子元素语义丢失。
- **aria 标签缺失**：AgentView 的"重新检测"按钮无 `aria-label`，仅图标+文本；CLI 状态 dot 无 `aria-label`/`title`；进度条无 `role="progressbar"` + `aria-valuenow/min/max`（`TaskLogDrawer.tsx:56-77` 的 ProgressBar 缺这些属性）。
- **颜色对比/状态可读性**：当前 CLI 行 state 文字用 `uppercase tracking 11px muted` 表达，色盲用户无法区分 installed/missing/failed；设计稿用图标颜色 + dot 双重编码，当前只有图标颜色。
- **对话框焦点陷阱**：`CompileConflictDialog` 打开时未实现焦点陷阱（focus trap），Tab 可能逃逸到背景。`TaskLogDrawer` 同理。
- **实时日志可达性**：日志区 `aria-live` 缺失，屏幕阅读器不会播报新日志；设计稿用视觉 cursor 动画对听障用户无影响但对盲用户等于静默。
- **i18n 完整性**：AgentView 大量文案走 i18n（`agent.*`），但任务行的 `task.status`/`task.taskType` 在 AgentView 直接裸显字符串（`{task.status}`），未走 `t()`，多语言下会显示英文枚举。

## 5. 建议实施顺序

1. **P0 — 完成运行 Agent 对话框语义**：Skill 与执行路径路由已打通；继续接通 `checkpoint` / `background`，确保用户设置能影响实际任务，或移除无效控制。
2. **P1 — 核心操作四宫格 + CLI 行视觉对齐**：一起做，因为都依赖补 `.cli-row`/`.ingest-card` CSS 到 `src/styles.css`。四宫格直接接到已实现视图切换。
3. **P1 — BYOK 卡片化**：需先与后端商定 `list_secret_status` 命令（只返掩码 + configured bool，不返完整 key，符合 CLAUDE.md 硬约束）。
4. **P1 — AgentView 任务行内进度/取消**：复用 `TaskLogDrawer` 的 `ProgressBar` 和 `cancelTaskRequest`，改动量小，体验提升大。
5. **P1 — terminal 日志样式升级**：抽 `Terminal` 组件供 AgentView 主区和 TaskLogDrawer 共用，补 level 标签染色和 overlay 按钮。
6. **P1 — 右面板 Agent 配置**：后端补 `list_skills` + `check_skill_templates` 命令；安全边界 checkbox 持久化到 settings.json；上下文窗口滑杆读写 agent-config.json。
7. **P2 — 安装引导弹窗、可访问性补丁**（aria/焦点陷阱/aria-live/任务行 i18n）。

> 备注：后端 Agent/compile/lint 流已真实 spawn 子进程且包含 Git 检查点、冲突确认、回滚、取消、后台持久化等所有硬约束，前端落差集中在信息密度和交互形态，不涉及核心安全逻辑重写。
