# 跨切面特性落差与实施计划

> 对照源：`SPEC/PRD.md` + `CLAUDE.md` 必读硬边界
> 当前实现：`src/stores/`、`src/hooks/`、`src/services/`、`src-tauri/src/{services,commands,utils}/`、`src/i18n/`、`src/components/app/`
> 审计范围：跨视图、被 PRD 或 CLAUDE.md "必读硬边界" 强制约束、不属于单一视图的全局特性。

## 0. 现状摘要

跨切面硬约束整体落地度高。`GitService` 能初始化仓库、生成 scoped/全量 checkpoint、输出 Markdown diff；`CompileService`、`LintService`、`ChatService.save_answer_to_wiki` 在写文件前创建 checkpoint，但 **Import 仍是 P0 例外**：`confirm_import_preview` 先确认归档并写 `.app/import-conflicts.json`，之后才创建 checkpoint，因此跨切面的“危险写操作前检查点”边界仅部分完成。`PendingAction + ConfirmationRegistry + ConfirmationDialog` 把删除、替换、覆盖、冲突、智能体自动修复统一收口到前端对话框；`ProjectContext` + `app_state::ProjectRegistry` 实现了项目 id+根路径双校验、路径穿越/绝对路径/符号链接拒绝、Unicode-CJK 兼容（测试覆盖中文资料库路径）。托盘最小化、任务事件流、OS 通知、任务可取消也已落地。

主要的 P0 落差集中在两处：

1. **i18n 对 Agent/LLM 生成内容的语言偏好未落地**（CLAUDE.md 硬约束明确要求 "Agent 生成内容按用户语言偏好输出"，但 chat/compile/export/lint 的 prompt 全是英文，且未把 `settings.language` 传入后端）。这是 P0 红线。
2. **Import checkpoint 时序不满足预操作语义**：`confirm_import_preview` 的两次写盘都发生在 `create_import_checkpoint` 之前；checkpoint 失败无法阻止已经发生的导入变更。详细 P0 以 [import.md](import.md) 为准。

另外有几处 P1/P2 收尾项：托盘菜单已按启动时的 Settings language 本地化，但应用运行中切换语言后需重启才能重建菜单（P2）；URL 安全策略与 SSRF 防护已落地但日志不显式遮蔽密钥的回归测试缺失；`set_default_agent` 后未持久化到 `.app/agent-config.json`（待核对）。`WorkspaceController` 挂载的 `useAiCapabilities` 已在每次 project key 变化时无条件检测 Agent/Provider；启动页也会复用最近项目检测，但没有任何 recent project 时仍无法构造后端要求的 project context。

## 1. 跨切面特性清单

| 特性 | 硬约束/PRD 要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 1. Git 检查点机制 | CLAUDE.md 必读硬边界；PRD-GIT-001/002/003/004 | `GitService` 完整落地，compile/lint/chat 在受保护写入前创建 checkpoint；Import 虽接入 checkpoint，但 `confirm_import_preview` 先写导入产物和冲突 JSON 再创建 checkpoint | 🟡部分实现 | P0 | `src-tauri/src/services/git_service.rs:60-150`、`src-tauri/src/commands/import_commands.rs:597-625`、[import.md](import.md) |
| 2. API Key 凭据管理 | CLAUDE.md 必读硬边界；PRD-SET-002 | 走 `keyring` crate；前端只显"已配置"；密钥不入项目文件 | ✅已完成 | — | `src-tauri/src/services/secret_service.rs:14-92`、`src-tauri/src/commands/llm_commands.rs`、`src/components/settings/SettingsView.tsx` |
| 3. Agent 默认优先 / BYOK 兜底 | CLAUDE.md 必读硬边界；PRD-WIKI-001/002、PRD-AGENT-001 | 路由策略 `Auto` 已落地：Agent installed → Agent，否则 BYOK；Agent CLI 检测走 `where`/`which`+`%APPDATA%\npm` fallback；只检测不安装 | ✅已完成 | — | `src-tauri/src/commands/compile_commands.rs:160-257`、`src-tauri/src/services/agent_service.rs:73-131,554-607` |
| 4. 长任务可取消 / 可后台 / 可报告进度 / 托盘 | CLAUDE.md 必读硬边界；PRD-AGENT-003/004/005、§11.1 | `TaskService` 后台任务+事件总线+进度+取消；托盘"最小化到托盘"已接入 `CloseBehavior`；启动时按 Settings language 本地化菜单/tooltip；OS 通知 | 🟡部分实现 | P1 | `src-tauri/src/lib.rs:31-108`、`src-tauri/src/utils/i18n.rs:34-41`、`src/stores/taskStore.ts:121-128`、`src/hooks/useTaskEvents.ts:43-98`、`src/services/notifications.ts` |
| 5. 路径安全 / Unicode-CJK / 跨平台 | CLAUDE.md 必读硬边界；PRD §11.4 | `ProjectContext` 全链路路径校验 + 符号链接拒绝 + canonicalize 跨盘符防护 + CJK 测试 | ✅已完成 | — | `src-tauri/src/models/paths.rs:19-103`、`src-tauri/src/app_state.rs:41-95`、`src-tauri/src/utils/url_utils.rs:11-57` |
| 6. PendingAction 高风险确认流 | CLAUDE.md 必读硬边界；PRD-LINT-004、PRD-GIT-004 | 后端 `ConfirmationRegistry` 统一登记 + scoped 执行；`ProjectConfirmationController` 从 `PendingAction.checkpointHash` 派生 checkpoint 显示，并编排 `ConfirmationDialog` + `CompileConflictDialog` | 🟡部分实现 | P1 | `src/components/app/ProjectConfirmationController.tsx`、`src/components/app/ConfirmationDialog.tsx`、`src-tauri/src/commands/compile_commands.rs:385-628` |
| 7. i18n（zh-CN / en） | CLAUDE.md 必读硬边界（UI + Agent 输出）；PRD-SET-003 | UI 全量双语；Agent/LLM 生成内容未按用户语言偏好输出 | 🟡部分实现 | P0 | `src/i18n/index.ts:19-34`、`src/i18n/locales/en.json`、`src-tauri/src/services/chat_service/retrieval.rs` (`ChatService::build_retrieval_context`)、`src-tauri/src/services/compile_service.rs:207-211`、`src-tauri/src/services/export_service.rs:31-150`、`src-tauri/src/services/lint_service/deep.rs` (`LintService::build_deep_lint_prompt`) |
| 8. 本地优先 / 无数据库 | CLAUDE.md 必读硬边界；PRD §11.5 | 无任何数据库依赖；状态 = Markdown + JSON；`FileStore` 原子写 `.app/` | ✅已完成 | — | `src-tauri/src/services/file_store.rs`、`src-tauri/Cargo.toml` |
| 9. 通知 / Toast 系统 | PRD-AGENT-003/005、§11.2 | OS 通知（完成/失败/待确认）+ 前端 Toaster | ✅已完成 | — | `src/services/notifications.ts`、`src/components/app/Toaster.tsx`、`src/stores/toastStore.ts` |
| 10. 搜索全局入口（普通搜索不调模型） | CLAUDE.md §搜索约定；PRD-CHAT-005 | `search_wiki` 后端命令只做关键词/标签/类型/来源过滤；TopBar ⌘K 入口 | ✅已完成 | — | `src-tauri/src/commands/search_commands.rs:7-20`、`src/components/app/TopBar.tsx` |

## 2. 逐条落差与验收

### 2.1 Git 检查点机制（🟡部分实现，P0）

- 现状：
  - `GitService::initialize_repository` / `create_checkpoint` / `create_scoped_checkpoint` / `diff_markdown` 完整实现，`core.quotepath=false` + 强制 `user.name/email` 保证 CJK 路径可提交、提交者一致。
  - `compile_commands::run_compile` 在启动时先 `create_checkpoint(HighRiskOperation)`，任何分支失败会 `unstage_paths` + `restore_outputs` 回滚；冲突场景把 `checkpoint_hash` 存进 `ConfirmationExecution::CompileMerge`，确认前 `ensure_checkpoint_head` 检查 HEAD 未漂移。
  - `LintService::apply_fix` 对 `Missing frontmatter` 这种安全修复也走 `create_scoped_checkpoint`（`checkpoint_path`），对 `DeadLink`/`IndexDrift` 高风险修复在确认后才 checkpoint；写盘前用 `OverwriteIfHashMatches` 做乐观锁。
  - `ChatService::save_answer_to_wiki` 新建页面直接写、覆盖前必须 `allow_overwrite + expected_hash` 且先 scoped checkpoint。
  - `request_delete_source` / `request_replace_source` 返回 `PendingAction(risk_level=Destructive)`，`affected_paths` 包含源文件 + 关联 extracted 工件。
  - **Import 例外**：`confirm_import_preview` 在 `ImportService::confirm_import` 与 `.app/import-conflicts.json` 写盘后才调用 `create_import_checkpoint`（`src-tauri/src/commands/import_commands.rs:603-618`），不满足预操作检查点硬边界。
- 目标：先修复 Import P0，使 checkpoint 创建发生在任何 confirm-import mutation 之前；同时补显式覆盖“checkpoint 失败时写盘被阻止”的回归测试（详细设计以 [import.md](import.md) 为准）。
- 涉及文件：见清单表。
- 验收标准：
  1. `confirm_import_preview` 必须在 `ImportService::confirm_import`、conflict JSON 或其他确认导入写盘之前成功创建 checkpoint；模拟 checkpoint 失败时项目内容不变。
  2. 新增测试：模拟 `git commit` 失败（如 `.git` 目录只读）时，`apply_fix` / `save_answer_to_wiki` / `apply_confirmed_manifest` 不产生任何文件变更。
  3. 新增测试：`delete_source` 确认执行后，checkpoint commit message 出现在 `git log` 且 `affected_paths` 全部在 commit 里。

### 2.2 API Key 凭据管理（✅已完成）

- 现状：
  - `SecretService` 默认走 `keyring::Entry`（Windows Credential Manager / macOS Keychain / Linux Secret Service）；测试模式下 `SecretService::memory()` 使用进程内 `HashMap`，避免在 CI 里污染宿主凭据库。
  - `provider_secret_status` 只返布尔；`mask()` 返回 `••••` + 末 4 字；`list_llm_providers` 返回 `has_secret` / `secret_mask` 与非敏感配置，不返回原始密钥。
  - 前端 Settings 通过 Provider status 的 `hasSecret` / `secretMask` 表达"已配置/未配置"，`store_provider_secret` 直接调 `secret_service.set`，不落 `.app/`。
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
- 目标：项目工作区保持现状；`useAiCapabilities` 已在 project key 变化时无条件异步 `detect_agents` / `list_llm_providers`。仅启动页在没有任何 recent project 时缺少可用于后端命令的 project context。
- 涉及文件：见清单表。
- 验收标准：
  1. ✅ `WorkspaceController` / `useAiCapabilities` 已在 `projectId + rootPath` 变化时触发 `detect_agents` 与 provider refresh，并把 resolved route 写入共享 project 状态。
  2. 回归测试：`Auto` + Agent installed + BYOK configured → 路由是 Agent；`Auto` + Agent missing + BYOK configured → 路由是 BYOK；`Auto` + 两者都缺 → 错 `LLM_PROVIDER_MISSING` 或 `AGENT_UNAVAILABLE`。

### 2.4 长任务可取消 / 可后台 / 可报告进度 / 托盘最小化（🟡部分实现，P1）

- 现状：
  - `TaskService` 有 `create_project_task`、`update_progress`、`append_log`、`is_cancelled`、`cancel_task`、`transition_status`（含 `WaitingForConfirmation`）。
  - `run_task_streaming`（AgentService）50ms 轮询 `is_cancelled`，取消时 `child.kill()` 并返回 `AGENT_CANCELLED`；BYOK compile 在 `tokio::select!` 里响应取消。
  - `lib.rs:32-48` 读取 `SettingsService::read_language`，调用 `utils::i18n::tray_labels` 构建本地化菜单与 tooltip；`tray_labels` 覆盖 English、Simplified Chinese、Traditional Chinese。`on_window_event(CloseRequested)` 读 `SettingsService::read_close_behavior`，`MinimizeToTray` 时 `prevent_close + hide`。
  - OS 通知：完成/失败/待确认三种事件，`notifyTaskEvent` 异步触发；`onAction` 把窗口 `show + setFocus` 并打开 task drawer。
- 落差：
  1. 进度报告对 BYOK 路径较粗：`generate_manifest` 在 BYOK 下只在 prompt 前 append 一条 "Calling {:?}" 日志，模型生成期间用户看不到进度（单次 `llm_service.complete` 是阻塞的，进度条会"卡住"）。建议改为异步流式（stream）或至少每秒 append "Generating..."。
  2. Import preview 的等待已从 `AppShell` 下沉到 `useImportWorkflow`，并统一调用 event-first 的 `waitForTaskTerminal`；该 helper 仍以 1s `get_task` 轮询作为漏事件/监听失败时的兜底。需要保留兜底语义时，不应把它误记成 AppShell feature workflow。
  3. **P2**：托盘菜单在启动时构建一次；运行中切换 language 不会就地重建菜单，需重启应用后生效。
- 涉及文件：`src-tauri/src/lib.rs:32-108`、`src-tauri/src/utils/i18n.rs:34-41`、`src-tauri/src/commands/compile_commands.rs:226-253`、`src/features/import/useImportWorkflow.ts`、`src/lib/waitForTaskTerminal.ts`。
- 验收标准：
  1. ✅ 启动应用时，托盘菜单与 tooltip 按持久化的 `settings.language` 显示 English / 简体中文 / 繁体中文；运行中切换语言后的菜单热更新属于 P2，当前需重启。
  2. BYOK compile 在模型生成期间每 ≤2s append 一条 progress 日志，或切到流式 API。
  3. 评估 `waitForTaskTerminal` 的 event-first + 1s polling fallback 是否需要进一步统一；不得删除防漏事件兜底而不补等价可靠性保障。

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

### 2.6 PendingAction 高风险确认流（🟡部分实现，P1）

- 现状：
  - 前后端 `PendingActionType` DTO 共有 10 个值；`ConfirmationDialog` 的穷尽映射覆盖全部 10 个，包括 `install_agent`。
  - `ConfirmationExecution` 是 registry continuation 的内部执行计划，恰好只有 7 个变体：`InitializeFolder`、`CompileMerge`、`LintFix`、`ChatOverwrite`、`DeleteSource`、`DeleteWikiPage`、`ReplaceSource`。它与面向 UI/IPC 的 `PendingActionType` 不是一一同名或同数量的概念。
  - 后端 `ConfirmationRegistry::register_with_execution` 把 `PendingAction` 与可选 executable continuation 一起存，`confirm(action_id, status)` 取出执行。
  - `ProjectConfirmationController` 把项目级 PendingAction（import delete/replace）与 compile 冲突 PendingAction 合并到 `displayedPendingAction`，分别走 `confirmPendingAction`（项目 store）和 `confirm_compile_action`（后端命令）；`AppShell` 只挂载 controller。
- 已完成：`PendingAction.checkpointHash` 已透传；`ProjectConfirmationController` 使用 `displayedPendingAction.checkpointHash != null` 派生 `checkpointExists`，compile merge 与尚未创建 checkpoint 的动作能显示不同状态。
- 已完成：破坏性操作确认按钮使用 `Button variant="danger"`；非破坏性确认使用 secondary，不再存在 primary/danger 冲突。
- 剩余落差（P1/P2）：`PendingActionType::BatchRewrite` / `InstallAgent` / `RunSkill` 当前没有生产者。若未来接入，应分别决定是否需要 executable continuation；不能据此虚构同名 `ConfirmationExecution` 变体。
- 涉及文件：`src/components/app/ConfirmationDialog.tsx`、`src/components/app/ProjectConfirmationController.tsx`、`src-tauri/src/models/confirmation.rs`、`src-tauri/src/commands/compile_commands.rs:128-145`。
- 验收标准：
  1. ✅ `PendingAction.checkpoint_hash` 与前端 `checkpointHash` 已接通，controller 按是否存在 hash 派生显示。
  2. 补 e2e：compile 冲突发生时对话框显示"Checkpoint: available"；dead_link 修复首次对话框显示"Checkpoint: not created yet"，确认后二次写盘创建 checkpoint。
  3. 为新生产的 action type 补 controller/registry 端到端测试；只有实际需要延迟执行时才新增或复用 `ConfirmationExecution` continuation。

### 2.7 i18n（zh-CN / en）（🟡部分实现，P0）

- 现状：
  - UI 侧 `i18n/index.ts` 用 `react-i18next`，`en.json` 和 `zh-CN.json` 的 key 完全对齐（shell、nav、views、settings、lint、chat、graph、exports、import、task、notification、confirmation 全覆盖），`localStorage` 持久化偏好，fallback `en`。
  - **Agent/LLM 生成内容未按用户语言偏好输出**：`ChatService::assemble_prompt`、`CompileService::compile_prompt`、`ExportService::build_export_prompt`、`LintService::build_deep_lint_prompt`、`compile_commands::provider_prompt` 五个 prompt 构造点全部是英文 system instruction，且没有读取 `settings.language`。
  - 托盘菜单启动时本地化已完成：`SettingsService::read_language` → `tray_labels`，覆盖 English、Simplified Chinese、Traditional Chinese；运行中切换语言需重启后刷新菜单（P2）。
- 目标（CLAUDE.md 原文："i18n：Agent 生成内容按用户语言偏好输出"）：
  1. 后端在构造 prompt 时读 `SettingsService::read_settings().language`，在 system instruction 末尾追加 "Respond in {{language}}" 或等价指令；生成式任务（chat 回答、wiki 编译页面文本、导出 HTML 正文、深度 lint 建议）按语言偏好输出。
  2. 确定性输出（JSON schema、frontmatter 字段名、路径、lint issueType 枚举）保持英文，避免破坏解析。
- 涉及文件：`src-tauri/src/services/chat_service/retrieval.rs` (`ChatService::build_retrieval_context`)、`src-tauri/src/services/compile_service.rs:207-211`、`src-tauri/src/services/export_service.rs:31-150`、`src-tauri/src/services/lint_service/deep.rs` (`LintService::build_deep_lint_prompt`)、`src-tauri/src/commands/compile_commands.rs:295-317`、`src-tauri/src/services/settings_service.rs`、`src-tauri/src/lib.rs:35-48`。
- 验收标准：
  1. 切换语言到 zh-CN 后，chat 回答、编译生成的 `wiki/*.md` 正文、HTML 导出正文、深度 lint 的 `suggestion` 字段为中文（确定性字段如 path/issueType 仍英文）。
  2. ✅ 以 zh-CN 启动时，托盘菜单显示"显示/隐藏/退出"且 tooltip 本地化；zh-TW 与英文也有对应启动时文案。
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

1. **2.1 Import checkpoint 预操作语义**：把 checkpoint 创建移到任何 confirm-import mutation 之前；checkpoint 失败不得产生项目文件变更。详细实现与验收以 [import.md](import.md) 为准。
2. **2.7 i18n 生成内容语言偏好**：五个 prompt 构造点接入 `SettingsService::language`。托盘启动时本地化已完成，不属于此 P0。

### P1（MVP 期内补齐）

3. **2.4 BYOK compile 流式进度**：把 `LlmService::complete` 改成 stream，或至少每 2s append "Generating..." 日志。
4. **2.4 任务状态同步机制统一**：Import 已改为 `useImportWorkflow` + `waitForTaskTerminal`；继续评估 event-first 与可靠 polling fallback 的统一边界。

### P2（打磨）

5. **2.2 prompt 泄露密钥的快照测试**。
6. **2.5 Windows UNC 路径**的 `ensure_no_detectable_escape` 测试。
7. **2.10 无 Provider 也能搜索**的回归测试。
8. **2.4 托盘菜单语言热更新**：如需免重启切换，重建现有 tray menu；当前启动时本地化已满足，故为 P2。
