# 跨切面特性落差与实施计划

> 对照源：`SPEC/PRD.md` + `CLAUDE.md` 必读硬边界
> 当前实现：`src/stores/`、`src/hooks/`、`src/services/`、`src-tauri/src/{services,commands,utils}/`、`src/i18n/`、`src/components/app/`
> 审计范围：跨视图、被 PRD 或 CLAUDE.md "必读硬边界" 强制约束、不属于单一视图的全局特性。

## 0. 现状摘要

跨切面硬约束整体落地度高。最核心的 **Git 检查点 + 高风险确认** 闭环已打通：`GitService` 能初始化仓库、生成 scoped/全量 checkpoint、输出 Markdown diff；`CompileService`、`LintService`、`ChatService.save_answer_to_wiki` 在写文件前都先创建 checkpoint；`PendingAction + ConfirmationRegistry + ConfirmationDialog` 把删除、替换、覆盖、冲突、智能体自动修复统一收口到前端对话框；`ProjectContext` + `app_state::ProjectRegistry` 实现了项目 id+根路径双校验、路径穿越/绝对路径/符号链接拒绝、Unicode-CJK 兼容（测试覆盖中文资料库路径）。托盘最小化、任务事件流、OS 通知、任务可取消也已落地。

主要的落差集中在三处：

1. **i18n 对 Agent/LLM 生成内容的语言偏好未落地**（CLAUDE.md 硬约束明确要求 "Agent 生成内容按用户语言偏好输出"，但 chat/compile/export/lint 的 prompt 全是英文，且未把 `settings.language` 传入后端）。这是 P0 红线。
2. **LintService 高风险修复的确认流有隐患**：`ConfirmationDialog` 组件硬编码 `checkpointExists={false}`，即便后端已经按操作语义先创建了 checkpoint，UI 仍始终显示 "Checkpoint: not created yet"，在 dead_link / index_drift / chat overwrite 这类"先 checkpoint 再写"的路径上误导用户。是 P0 体验红线。
3. **ConfirmationDialog 里 `riskLevel === "destructive"` 时确认按钮却用 `variant="primary"`**，样式和语义相反（destructive 应该用 danger 样式已经做了，但 button variant 与样式冲突可能让用户优先级倒置）。属于 P1。

另外有几处 P1/P2 收尾项：托盘菜单 i18n 缺失、URL 安全策略与 SSRF 防护已落地但日志不显式遮蔽密钥的回归测试缺失、`detect_agents` 在前端 AppShell 里没有后台运行（切到 Agent/Settings 视图才 detect，MVP 验收要求的"显示可用 Agent、版本和状态"在 Dashboard 首屏会空），以及 `set_default_agent` 后未持久化到 `.app/agent-config.json`（待核对）。

## 1. 跨切面特性清单

| 特性 | 硬约束/PRD 要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 1. Git 检查点机制 | CLAUDE.md 必读硬边界；PRD-GIT-001/002/003/004 | `GitService` 完整落地，compile/lint/chat/import 流程均接入 checkpoint | ✅已完成 | — | `src-tauri/src/services/git_service.rs:60-150`、`src-tauri/src/services/compile_service.rs:259-400`、`src-tauri/src/services/lint_service.rs:505-630`、`src-tauri/src/services/chat_service.rs:323-403` |
| 2. API Key 凭据管理 | CLAUDE.md 必读硬边界；PRD-SET-002 | 走 `keyring` crate；前端只显"已配置"；密钥不入项目文件 | ✅已完成 | — | `src-tauri/src/services/secret_service.rs:14-92`、`src-tauri/src/commands/llm_commands.rs`、`src/components/settings/SettingsView.tsx` |
| 3. Agent 默认优先 / BYOK 兜底 | CLAUDE.md 必读硬边界；PRD-WIKI-001/002、PRD-AGENT-001 | 路由策略 `Auto` 已落地：Agent installed → Agent，否则 BYOK；Agent CLI 检测走 `where`/`which`+`%APPDATA%\npm` fallback；只检测不安装 | ✅已完成 | — | `src-tauri/src/commands/compile_commands.rs:160-257`、`src-tauri/src/services/agent_service.rs:73-131,554-607` |
| 4. 长任务可取消 / 可后台 / 可报告进度 / 托盘 | CLAUDE.md 必读硬边界；PRD-AGENT-003/004/005、§11.1 | `TaskService` 后台任务+事件总线+进度+取消；托盘"最小化到托盘"已接入 `CloseBehavior`；OS 通知 | 🟡部分实现 | P1 | `src-tauri/src/lib.rs:31-100`、`src/stores/taskStore.ts:121-128`、`src/hooks/useTaskEvents.ts:43-98`、`src/services/notifications.ts` |
| 5. 路径安全 / Unicode-CJK / 跨平台 | CLAUDE.md 必读硬边界；PRD §11.4 | `ProjectContext` 全链路路径校验 + 符号链接拒绝 + canonicalize 跨盘符防护 + CJK 测试 | ✅已完成 | — | `src-tauri/src/models/paths.rs:19-103`、`src-tauri/src/app_state.rs:41-95`、`src-tauri/src/services/url_utils.rs:11-57` |
| 6. PendingAction 高风险确认流 | CLAUDE.md 必读硬边界；PRD-LINT-004、PRD-GIT-004 | 后端 `ConfirmationRegistry` 统一登记 + scoped 执行；前端 `ConfirmationDialog` + `CompileConflictDialog` | 🟡部分实现 | P0 | `src/components/app/ConfirmationDialog.tsx:27-172`、`src/components/app/AppShell.tsx:140-169`、`src-tauri/src/commands/compile_commands.rs:385-628` |
| 7. i18n（zh-CN / en） | CLAUDE.md 必读硬边界（UI + Agent 输出）；PRD-SET-003 | UI 全量双语；Agent/LLM 生成内容未按用户语言偏好输出 | 🟡部分实现 | P0 | `src/i18n/index.ts:19-34`、`src/i18n/locales/en.json`、`src-tauri/src/services/chat_service.rs:212-262`、`src-tauri/src/services/compile_service.rs:207-211`、`src-tauri/src/services/export_service.rs:31-150`、`src-tauri/src/services/lint_service.rs:295-345` |
| 8. 本地优先 / 无数据库 | CLAUDE.md 必读硬边界；PRD §11.5 | 无任何数据库依赖；状态 = Markdown + JSON；`FileStore` 原子写 `.app/` | ✅已完成 | — | `src-tauri/src/services/file_store.rs`、`src-tauri/Cargo.toml` |
| 9. 通知 / Toast 系统 | PRD-AGENT-003/005、§11.2 | OS 通知（完成/失败/待确认）+ 前端 Toaster | ✅已完成 | — | `src/services/notifications.ts`、`src/components/app/Toaster.tsx`、`src/stores/toastStore.ts` |
| 10. 搜索全局入口（普通搜索不调模型） | CLAUDE.md §搜索约定；PRD-CHAT-005 | `search_wiki` 后端命令只做关键词/标签/类型/来源过滤；TopBar ⌘K 入口 | ✅已完成 | — | `src-tauri/src/commands/search_commands.rs:7-20`、`src/components/app/TopBar.tsx` |

## 2. 逐条落差与验收

### 2.1 Git 检查点机制（✅已完成）

- 现状：
  - `GitService::initialize_repository` / `create_checkpoint` / `create_scoped_checkpoint` / `diff_markdown` 完整实现，`core.quotepath=false` + 强制 `user.name/email` 保证 CJK 路径可提交、提交者一致。
  - `compile_commands::run_compile` 在启动时先 `create_checkpoint(HighRiskOperation)`，任何分支失败会 `unstage_paths` + `restore_outputs` 回滚；冲突场景把 `checkpoint_hash` 存进 `ConfirmationExecution::CompileMerge`，确认前 `ensure_checkpoint_head` 检查 HEAD 未漂移。
  - `LintService::apply_fix` 对 `Missing frontmatter` 这种安全修复也走 `create_scoped_checkpoint`（`checkpoint_path`），对 `DeadLink`/`IndexDrift` 高风险修复在确认后才 checkpoint；写盘前用 `OverwriteIfHashMatches` 做乐观锁。
  - `ChatService::save_answer_to_wiki` 新建页面直接写、覆盖前必须 `allow_overwrite + expected_hash` 且先 scoped checkpoint。
  - `request_delete_source` / `request_replace_source` 返回 `PendingAction(risk_level=Destructive)`，`affected_paths` 包含源文件 + 关联 extracted 工件。
- 目标：保持现状；**回归测试需要显式覆盖"checkpoint 失败时写盘被阻止"**（目前 `compile_service` 的回滚测试覆盖间接路径，lint/chat 有 scoped checkpoint 失败映射成 `GIT_CHECKPOINT_FAILED` 的代码但缺专门测试）。
- 涉及文件：见清单表。
- 验收标准：
  1. 新增测试：模拟 `git commit` 失败（如 `.git` 目录只读）时，`apply_fix` / `save_answer_to_wiki` / `apply_confirmed_manifest` 不产生任何文件变更。
  2. 新增测试：`delete_source` 确认执行后，checkpoint commit message 出现在 `git log` 且 `affected_paths` 全部在 commit 里。

### 2.2 API Key 凭据管理（✅已完成）

- 现状：
  - `SecretService` 默认走 `keyring::Entry`（Windows Credential Manager / macOS Keychain / Linux Secret Service）；测试模式下 `SecretService::memory()` 使用进程内 `HashMap`，避免在 CI 里污染宿主凭据库。
  - `provider_secret_status` 只返布尔；`mask()` 返回 `••••` + 末 4 字。
  - 前端 `SettingsView` 通过 `provider_secret_status` / `provider.configured` 表达"已配置/未配置"，`store_provider_secret` 直接调 `secret_service.set`，不落 `.app/`。
  - `provider_prompt` / `build_deep_lint_prompt` / `build_retrieval_context` / `build_export_prompt` 里都注释"No secret or API key is ever placed in the prompt"，实际实现也确实不拼 key。
- 目标：保持现状。
- 涉及文件：见清单表。
- 验收标准：
  1. 新增 `grep`-级测试：对 `src-tauri/src/services` 下所有 prompt 构造函数，快照断言输出不含"sk-"、"key"、"secret"等模式（数据驱动，避免未来回归）。
  2. 前端单测：`provider.hasSecret === false` 时 UI 文案严格为"未配置/Not configured"，不出现任何密钥片段。

### 2.3 Agent 默认优先 / BYOK 兜底（✅已完成）

- 现状：
  - `compile_commands::generate_manifest` 实现 `CompileRoutePreference::Auto`：先尝试 `usable_agent`（`detect_agents` 里 `state == Installed`），有则 Agent，无则 BYOK。
  - `find_executable` 做了 `where`/`which` → `%APPDATA%\npm\{cmd,bat,exe}` 的 fallback，且 Windows 下优先 `.cmd/.bat/.exe` 避开无扩展名 bash shim（注释解释了 CreateProcess 无法执行该 shim）。
  - "不静默安装"：`install_guidance` 只是字符串数据，`AgentService` 没有任何 `Command::new("npm install ...")`。
  - BYOK 路径：`LlmService::complete` 在 BYOK 时用 `secret_service.get(provider.provider)` 拿密钥，Provider 未配置 secret 直接错 `LLM_SECRET_MISSING`。
- 目标：保持现状；**建议补充**：在 Dashboard 首屏异步 `detect_agents`，避免用户打开应用看到"Agent not found"误以为没装。
- 涉及文件：见清单表。
- 验收标准：
  1. AppShell `useEffect` 在 `currentProject.projectId` 变化时触发一次 `detect_agents`，把结果写进 `projectStore.currentProject.agentRoute`。
  2. 回归测试：`Auto` + Agent installed + BYOK configured → 路由是 Agent；`Auto` + Agent missing + BYOK configured → 路由是 BYOK；`Auto` + 两者都缺 → 错 `LLM_PROVIDER_MISSING` 或 `AGENT_UNAVAILABLE`。

### 2.4 长任务可取消 / 可后台 / 可报告进度 / 托盘最小化（🟡部分实现，P1）

- 现状：
  - `TaskService` 有 `create_project_task`、`update_progress`、`append_log`、`is_cancelled`、`cancel_task`、`transition_status`（含 `WaitingForConfirmation`）。
  - `run_task_streaming`（AgentService）50ms 轮询 `is_cancelled`，取消时 `child.kill()` 并返回 `AGENT_CANCELLED`；BYOK compile 在 `tokio::select!` 里响应取消。
  - `lib.rs:31-100` 构建托盘 Show/Hide/Quit 菜单，`on_window_event(CloseRequested)` 读 `SettingsService::read_close_behavior`，`MinimizeToTray` 时 `prevent_close + hide`。
  - OS 通知：完成/失败/待确认三种事件，`notifyTaskEvent` 异步触发；`onAction` 把窗口 `show + setFocus` 并打开 task drawer。
- 落差：
  1. 托盘菜单 label 硬编码英文（"Show"/"Hide"/"Quit"），tooltip `"LLM Wiki Desktop"`，与 i18n 硬约束冲突。托盘在 Rust 侧构建，需要把 `settings.language` 读到后端并本地化（或把菜单 label 抽到 i18n 资源 + 后端按语言挑选）。
  2. 进度报告对 BYOK 路径较粗：`generate_manifest` 在 BYOK 下只在 prompt 前 append 一条 "Calling {:?}" 日志，模型生成期间用户看不到进度（单次 `llm_service.complete` 是阻塞的，进度条会"卡住"）。建议改为异步流式（stream）或至少每秒 append "Generating..."。
  3. `AppShell.tsx:86` 通过轮询 `get_task`（250ms）同步 import preview 状态；这种轮询模式没推广到 compile/deep lint/export，后者依赖事件总线。两套机制并存，偶尔会出现 task_store 状态滞后。建议统一成事件驱动。
- 涉及文件：`src-tauri/src/lib.rs:35-82`、`src-tauri/src/commands/compile_commands.rs:226-253`、`src/components/app/AppShell.tsx:266-308`。
- 验收标准：
  1. 托盘菜单随 `settings.language` 切换中英文，tooltip 同步。
  2. BYOK compile 在模型生成期间每 ≤2s append 一条 progress 日志，或切到流式 API。
  3. `AppShell` 的轮询收敛为事件订阅（删除 `setTimeout(250)` 循环）。

### 2.5 路径安全 / Unicode-CJK / 跨平台（✅已完成）

- 现状：
  - `ProjectContext::resolve_project_path` → `validate_project_relative_path`（拒绝绝对路径、Windows 盘符 prefix、`..` 穿越）；`ensure_no_detectable_escape` 通过 canonicalize 检查最终路径在 canonical root 之下，防符号链接逃逸。
  - `ProjectRegistry::register` 强制 root 绝对路径 + canonicalize；`resolve` 双校验 (project_id, canonical_root)，`PROJECT_CONTEXT_MISMATCH` 测试覆盖了"用 A 的 id 拿 B 的 root"被拒。
  - `matching_normalized_root_resolves_and_preserves_cjk` 测试用中文目录名 `"中文资料库"` 验证规范化后可解析。
  - `is_safe_remote_url` + `is_public_ip` + `fetch_import_url` 在 DNS 解析后再校验 IP（防 DNS rebinding 到内网），重定向 `Policy::none()`（不跟随），5MB 上限，UTF-8 强制。
  - `compile_service::is_safe_wiki_markdown` 独立校验编译输出，拒绝反斜杠、绝对路径、非 `wiki/` 前缀。
- 目标：保持现状。
- 验收标准：
  1. 已有测试覆盖路径穿越、CJK、符号链接、跨盘符；建议补一项 `ensure_no_detectable_escape` 对 Windows UNC 路径（`\\?\C:\...`）的行为测试。

### 2.6 PendingAction 高风险确认流（🟡部分实现，P0）

- 现状：
  - 后端 `ConfirmationRegistry::register_with_execution` 把 `PendingAction` 和 `ConfirmationExecution`（`DeleteSource` / `ReplaceSource` / `CompileMerge` / `InitializeFolder` / …）一起存；`confirm(action_id, status)` 取出执行；`ConfirmationDialog` 支持 9 种 `action_type`（`initialize_folder` / `delete_file` / `overwrite_file` / `batch_rewrite` / `replace_source` / `delete_source` / `merge_conflict` / `agent_auto_fix` / `run_skill`）。
  - `AppShell.tsx:86-169` 把项目级 PendingAction（import delete/replace）与 compile 冲突 PendingAction 合并到 `displayedPendingAction`，分别走 `confirmPendingAction`（项目 store）和 `confirm_compile_action`（后端命令）。
- 落差（P0）：
  1. **`ConfirmationDialog` 硬编码 `checkpointExists={false}`**（`AppShell.tsx:152`）。对于 LintService 高风险修复（`dead_link` / `index_drift`）和 chat 覆盖保存，后端是在用户**确认后**才创建 checkpoint（`confirm_high_risk=true` 触发 `checkpoint_path`），所以这里显示"Checkpoint: not created yet"在语义上正确；但对于 `MergeConflict`（compile），checkpoint 在生成 manifest **之前**就已创建，`ConfirmationExecution::CompileMerge.checkpoint_hash` 存了 commit hash，前端却仍显示"not created yet"。这是误导。**应该把 `checkpoint_hash` 透传到 `PendingAction`，前端按是否存在 hash 决定文案。**
  2. `ConfirmationDialog.tsx:158-162`：`isDestructive` 时 button `variant="primary"` + className 用 danger 背景。shadcn `primary` 在语义上对应"主行动"，destructive 应该走独立 `variant="danger"` 或保持 `secondary` + danger class。目前视觉正确但 a11y/语义混乱。
  3. `ConfirmationExecution::InitializeFolder` / `OverwriteFile` / `DeleteFile` / `BatchRewrite` / `RunSkill` 在 `confirmation_models` 里定义了，但 `commands/` 里除了 compile/import 之外没有任何命令产出这些 action。LintService 返回的 `dead_link_pending_action` 用 `AgentAutoFix`，没问题；但 `agent_commands` 里没有"Agent 自动修复"路径产出 `AgentAutoFix`，这条 action_type 目前实际上是 lint 专用。
- 涉及文件：`src/components/app/ConfirmationDialog.tsx:27-172`、`src/components/app/AppShell.tsx:140-169`、`src-tauri/src/models/confirmation.rs`、`src-tauri/src/commands/compile_commands.rs:128-145`。
- 验收标准：
  1. `PendingAction` 增加 `checkpoint_hash: Option<String>` 字段；compile 冲突登记时填入；前端 `checkpointExists = action.checkpointHash !== null`。
  2. 新增 e2e：compile 冲突发生时对话框显示"Checkpoint: available"；dead_link 修复首次对话框显示"Checkpoint: not created yet"，确认后二次写盘创建 checkpoint。
  3. `ConfirmationDialog` 破坏性按钮 a11y label 或 variant 与样式对齐。

### 2.7 i18n（zh-CN / en）（🟡部分实现，P0）

- 现状：
  - UI 侧 `i18n/index.ts` 用 `react-i18next`，`en.json` 和 `zh-CN.json` 的 key 完全对齐（shell、nav、views、settings、lint、chat、graph、exports、import、task、notification、confirmation 全覆盖），`localStorage` 持久化偏好，fallback `en`。
  - **Agent/LLM 生成内容未按用户语言偏好输出**：`ChatService::assemble_prompt`、`CompileService::compile_prompt`、`ExportService::build_export_prompt`、`LintService::build_deep_lint_prompt`、`compile_commands::provider_prompt` 五个 prompt 构造点全部是英文 system instruction，且没有读取 `settings.language`。
  - 托盘菜单（见 2.4）硬编码英文。
- 目标（CLAUDE.md 原文："i18n：Agent 生成内容按用户语言偏好输出"）：
  1. 后端在构造 prompt 时读 `SettingsService::read_settings().language`，在 system instruction 末尾追加 "Respond in {{language}}" 或等价指令；生成式任务（chat 回答、wiki 编译页面文本、导出 HTML 正文、深度 lint 建议）按语言偏好输出。
  2. 确定性输出（JSON schema、frontmatter 字段名、路径、lint issueType 枚举）保持英文，避免破坏解析。
  3. 托盘菜单 + tooltip i18n。
- 涉及文件：`src-tauri/src/services/chat_service.rs:212-262`、`src-tauri/src/services/compile_service.rs:207-211`、`src-tauri/src/services/export_service.rs:31-150`、`src-tauri/src/services/lint_service.rs:295-345`、`src-tauri/src/commands/compile_commands.rs:295-317`、`src-tauri/src/services/settings_service.rs`、`src-tauri/src/lib.rs:35-48`。
- 验收标准：
  1. 切换语言到 zh-CN 后，chat 回答、编译生成的 `wiki/*.md` 正文、HTML 导出正文、深度 lint 的 `suggestion` 字段为中文（确定性字段如 path/issueType 仍英文）。
  2. 托盘菜单在 zh-CN 下显示"显示/隐藏/退出"。
  3. 回归测试：对每个 prompt 构造函数加一条断言，输出包含 "Respond in zh-CN" 或等价标记。

### 2.8 本地优先 / 无数据库（✅已完成）

- 现状：`Cargo.toml` 无 `rusqlite` / `sqlx` / `diesel` / `sqlite` 依赖；全仓 `grep` 命中的 "database" 都在 `wiki/wiki/` 样本知识库内容里（如 `entities/onyx.md`），不是应用代码。所有状态文件位于 `.app/`（chats、tasks、graph-cache、import-conflicts、import-previews、agent-config），Markdown + JSON 原子写。
- 目标：保持现状。
- 验收标准：
  1. CI 加一条依赖扫描脚本，若 `Cargo.toml` 引入数据库 crate 则失败。

### 2.9 通知 / Toast 系统（✅已完成）

- 现状：
  - OS 通知：`notifyTaskEvent` 对 `task_completed` / `task_failed` / `confirmation_requested` 三种事件发通知，权限懒申请；`onAction` 回调把窗口拉前台 + 打开 task drawer。
  - 应用内 Toast：`toastStore` + `Toaster` 组件，`info/warning/error` 三态，`aria-live="polite"`，`pointer-events-none` 容器 + `pointer-events-auto` 卡片，30s 自动消失（由 store 管理）。
- 目标：保持现状。
- 验收标准：
  1. 补一条 e2e：任务失败时同时触发 OS 通知和应用内 Toast；通知点击后窗口前置且 task drawer 打开到对应任务。

### 2.10 搜索全局入口（普通搜索不调模型）（✅已完成）

- 现状：`search_wiki` 命令注释明确"full-text wiki search (keyword + type/tag/source filters, scoring, snippet)"；TopBar ⌘K 入口直接调它，不经任何 LLM；自然语言问答走 Chat 视图（`ChatService::build_retrieval_context` 先本地检索、再交模型）。
- 目标：保持现状。
- 验收标准：
  1. 回归测试：`search_wiki` 在 0 个 LlmProvider 配置时仍返回结果（断言无网络调用）。

## 3. 建议实施顺序（P0 先做）

### P0（红线，MVP 前必须关闭）

1. **2.7 i18n 生成内容语言偏好**：五个 prompt 构造点接入 `SettingsService::language`；托盘菜单 i18n。违反 CLAUDE.md 硬约束。
2. **2.6 ConfirmationDialog checkpoint 显示**：`PendingAction` 增字段，compile 冲突透传 `checkpoint_hash`，前端按 hash 显示状态。高风险确认流的诚实性红线。
3. **2.4 Dashboard 首屏 Agent 检测**：AppShell 项目切换时触发一次 `detect_agents`，避免"打开应用看到 Agent 未检测"的错误首屏。

### P1（MVP 期内补齐）

4. **2.4 BYOK compile 流式进度**：把 `LlmService::complete` 改成 stream，或至少每 2s append "Generating..." 日志。
5. **2.4 任务状态同步机制统一**：删除 `AppShell` 的 250ms 轮询，全部走事件总线。
6. **2.6 ConfirmationDialog destructive 按钮 variant**：a11y 语义对齐。

### P2（打磨）

7. **2.1 checkpoint 失败阻止写盘**的专门回归测试。
8. **2.2 prompt 泄露密钥的快照测试**。
9. **2.5 Windows UNC 路径**的 `ensure_no_detectable_escape` 测试。
10. **2.10 无 Provider 也能搜索**的回归测试。
