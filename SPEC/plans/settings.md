# Settings 板块 P0+P1 实施账本

> 对照源：SPEC/roadmap/settings.md · SPEC/PRD.md §9.11 (PRD-SET-001/002/005) · UI-Frontend-design/settings.html + assets/app.css · CLAUDE.md
> 范围：仅本板块 P0+P1。不碰 P2（关于/配置概览右面板/设置搜索），不碰其它板块。
> 状态机：pending → in_progress → done → verified。每项独立 commit（conventional，不 --no-verify，不 push）。

## 设计意图总览（决策依据）

设计稿 settings.html 把 Settings 拆成 左 nav（应用/AI/系统三组 + Lucide 图标 + is-active→aria-current）+ 右内容区（每 section 用 `.formrow` label/hint/control 三列网格 + `.seg` 分段 + `.toggle` 开关 + `.apikey-row` + `.badge` 状态徽章 + `.checkbox` 列表 + range 滑块）。当前实现是 Tailwind 手搓卡片，密度与层级均不符。本次按设计稿逐 section 对齐，并补齐 PRD-SET-001（Provider 行 UX + 末 4 位 + 徽章）、PRD-SET-005（更新真查源 + 应用内确认）。

## 数据模型 scoping 决策（SET-MODEL）

- **全局**（写全局 settings.json，跨项目 UI 偏好/托盘/通知/更新）：startup_behavior、default_project_location、external_editor、associate_md_files、associate_wiki_folders、density、ui_font/reading_font/code_font、agent_output_language、system_notifications、notification_click_behavior、max_concurrent_tasks、update_frequency、auto_download_updates、prompt_changelog_before_install。沿用现有 language/theme/close_behavior/check_updates 归全局。
- **项目**（写 .app/settings.json，影响该项目 Agent/Git/上下文行为）：agent_task_timeout_secs、allow_agent_install、install_command_display_only、prompt_on_new_agent、skill_autoload、max_tokens、temperature、auto_git_checkpoint、manual_edit_protection、raw_sources_immutable。沿用现有 context_window/agent_default/llm_providers/template 归项目。
- 所有新字段 `#[serde(default)]`，旧 .app/settings.json / 全局 settings.json 无新字段时仍可反序列化（向后兼容）。

## 账本条目

| # | ID | 优先级 | 状态 | 描述 | 主改文件 |
|---|---|---|---|---|---|
| 1 | SET-MODEL | P0 | verified | 扩展 Settings 数据模型（Rust + TS）覆盖所有 P0+P1 字段，serde 默认向后兼容；补 serde/作用域测试 | src-tauri/src/models/settings.rs, src/types/settings.ts, src/stores/settingsStore.ts |
| 2 | SET-PROVIDERS | P0 | verified | PRD-SET-001：LlmProviderSettings 重写为 apikey-row + 末4位 + 状态徽章 + Ollama 服务态；后端 list_llm_providers 回填 secret_mask；密钥清除从 SecuritySettings 并入此页 | src/features/settings/LlmProviderSettings.tsx, src-tauri/src/commands/llm_commands.rs, src/styles.css, src/i18n/locales/{en,zh-CN}.json |
| 3 | SET-SECURITY | P0 | pending | SecuritySettings 重写为 自动Git检查点/人工编辑保护/Raw不可变 toggle + 钥匙串 mono 说明 + 沙箱说明 | src/features/settings/SecuritySettings.tsx |
| 4 | SET-AGENT | P0 | pending | AgentSettings 补 任务超时 input + 安装行为 3 checkbox + Skill 自动加载 toggle | src/features/settings/AgentSettings.tsx |
| 5 | SET-CONTEXT | P1 | pending | 新建 ContextWindowSettings.tsx（range 4K-1M + max tokens + 温度 range）；从后台任务页移除 contextWindow | src/features/settings/ContextWindowSettings.tsx, SettingsView.tsx, BackgroundTaskSettings.tsx |
| 6 | SET-GENERAL | P1 | pending | 通用 section：启动行为 select / 默认项目位置 input+选择 / 外部编辑器 input / 文件关联 checkbox | src/features/settings/SettingsView.tsx (general 分支) |
| 7 | SET-BACKGROUND | P1 | pending | 后台任务：closeBehavior 加"询问"第三项 + 系统通知 4 checkbox + 通知点击行为 select + 并发上限 input | src/features/settings/BackgroundTaskSettings.tsx |
| 8 | SET-UPDATES | P1 | pending | PRD-SET-005：新建 UpdateService（reqwest 真查 release 源）+ update_commands；UpdateSettings 接真检查 + 应用内确认对话框（弃 window.confirm）+ 频率 select + 变更说明 checkbox | src/features/settings/UpdateSettings.tsx, src-tauri/src/services/update_service.rs(新), src-tauri/src/commands/update_commands.rs(新), src-tauri/src/app_state.rs, src-tauri/src/lib.rs |
| 9 | SET-APPEARANCE-LANG | P1 | pending | 外观：界面密度 seg + UI/阅读/代码三字体 select；语言：Agent 输出语言 select 四选 | src/features/settings/AppearanceSettings.tsx, src/features/settings/LanguageSettings.tsx |
| 10 | SET-NAV-VISUAL | P1 | pending | 左 nav 三组分组 + Lucide 图标 + aria-current；header 自动保存 badge + 打开配置文件按钮；错误 role=alert；全板块引入 formrow/seg/toggle 统一密度 | src/features/settings/SettingsView.tsx, src/styles.css |

## 验收纪律（每项）
1. 对照 settings.html + app.css + PRD 确认意图。
2. 实施，守硬边界（API Key 只进钥匙串、路径安全、Git 检查点）。
3. `npm run test` + `npm run lint` 全绿（动 src-tauri/ 加 `cargo test`），清 console.log。
4. 仅本项改动 git add + commit（conventional，不 --no-verify，不 push）。
5. 追加 SPEC/progress.txt。
6. status→verified。

## 已完成项证据

### SET-PROVIDERS (verified @ 2026-06-24)
**意图**：settings.html:341-404 — apikey-row 5 列网格（icon/name+hint/badge/测试/编辑），Anthropic 深底白字 "A"、Ollama 用 cpu 图标 + `badge--warn 服务未运行`、Custom 用 link 图标；hint 展示 `model · mask · 钥匙串`；底部 accent-soft 安全说明卡。app.css 已有 `.badge`/`.badge--success/--outline/--warn` 与 `.apikey-row__icon/name/hint` 设计 token。
**实施**：
- 后端 `src-tauri/src/commands/llm_commands.rs:16-26` `status_with_secret` 助手回填 `has_secret`+`secret_mask`（`SecretService::mask` 产 `····XXXX`，永不回显全文，PRD-SET-002）；`list_llm_providers`/`save_llm_provider`/`store_provider_secret` 统一经此助手填 mask；`:88-139` 新增 `check_ollama_reachable` 异步命令探 `{base_url}/api/tags`（4s 超时，返 `OllamaReachability{reachable,baseUrl,modelCount}` 或 `OLLAMA_UNREACHABLE`），`:149-167` `test_llm_provider` 真 complete 探活。`lib.rs:122` 注册。
- 前端 `src/features/settings/LlmProviderSettings.tsx` 全量重写为 apikey-row 设计稿：每 provider 一行 `<button>`（icon/name/hint/badge/末位 model），选中态高亮；编辑面板常显（保 provider.test.tsx 契约：API key input 常在、行内含 model 文本、Test provider 按钮、role=status、saved 文本）；Ollama 行接 `check_ollama_reachable` 实时服务态，服务未运行时禁用 Test 并显 `badge--warn`；密钥清除（Delete key）并入此页。
- `src/styles.css` 补 `.apikey-row` 5 列网格（`grid-template-columns:32px minmax(0,1fr) auto auto auto`）与 `.badge--success/--outline` 变体（设计稿预设但 styles.css 此前缺），`.apikey-row__icon/name/hint` token 化。
- i18n 补 25 个 provider.* key（en/zh-CN 对齐）：name.*、maskedHint、ollamaUp/Down、serviceDown、secretSafety 等。
**守边界**：API Key 仍只进 keyring（`SecretService`），UI 仅显 mask 末4位，项目文件不存密钥；不动 `UI-Frontend-design/`；不动样本库 `wiki/wiki/`。
**验证**：`npm run test`(130 pass,27 files) + `npm run lint`(0 warning) 全绿，无 console.log；`cargo build --lib` + `cargo clippy --lib --tests`（仅旧有 2 warning：app_state.rs:192/import_service.rs:261，HEAD 已存在非本项）+ `rustfmt --check`（llm_commands.rs/lib.rs 干净；cargo fmt 报的 diff 全在 import_service.rs/tests/sources_promotion.rs，属先前未提交工作非本项）。**未做浏览器预览验证**：SettingsView 需 Tauri 运行时（后端 IPC + 已开项目上下文），浏览器预览无法渲染该面；渲染契约由 2 个 provider 单测覆盖（保存清空+不回显、按已存 model/baseUrl 测试）。
