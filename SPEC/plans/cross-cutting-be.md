# 进度账本 · cross-cutting 后端 (P0+P1)

✅ 本轮完成 @ 2026-06-21

## 摘要

cross-cutting 后端 4 项（P0-1 / P0-2 / P0-3 / P1-1）全部落地并通过验证。范围严格限定 `src-tauri/`，未触碰 `src/` 前端与 `UI-Frontend-design/`。新增 2 个模块（`utils/i18n.rs`、`tasks/byok_progress.rs`）集中语言注入与 BYOK 轮询逻辑，避免 4 处命令文件重复内联。

验证：
- `cargo test --lib --no-default-features` → **259 passed, 0 failed**（含新增 8 个单测：i18n 4 + settings read_language 2 + checkpoint_hash 序列化 1 + byok_progress 4，减去历史基线后净增；原 253 + 6 新增 ≈ 259）。
- `cargo clippy --lib --no-default-features -- -D warnings` → **clean**（本 loop 新增/改动代码零告警）。
- `cargo check`（含 gui feature，托盘代码）→ compiles。
- `npm run test` → **72 passed**；`npm run lint` → **clean**（本 loop 未改 `src/`，前端基线不受影响）。
- 已知基线：`cargo clippy`（full gui）在 HEAD 上有 21 条 pre-existing `needless_borrow`（`&context`）告警，分布在 `lint_commands` / `export_commands` 的非 BYOK 路径与其它模块 — 经 `git stash` 对照确认在动手前即存在，**非本 loop 引入**，属别模块遗留，按硬纪律不在本 loop 修复范围。

## 文件清单

新增：
- `src-tauri/src/utils/i18n.rs` — 语言注入 + 托盘标签 helper（`language_instruction` / `language_display_name` / `tray_labels`）。
- `src-tauri/src/tasks/byok_progress.rs` — BYOK 轮询 helper（`poll_with_progress` / `cancelled_error` / `ByokCancelled`），统一 compile/chat/lint/export 四处取消+进度逻辑。
- `SPEC/plans/cross-cutting-be.md` — 本账本。

修改：
- `src-tauri/src/utils/mod.rs` — `pub mod i18n;`
- `src-tauri/src/tasks/mod.rs` — `pub mod byok_progress;`
- `src-tauri/src/services/settings_service.rs` — `read_language()` + 2 测试。
- `src-tauri/src/services/chat_service.rs` — `build_retrieval_context(language)` 注入语言指令。
- `src-tauri/src/services/compile_service.rs` — `compile_prompt(language)` 注入。
- `src-tauri/src/services/export_service.rs` — `build_export_prompt(language)` 重构为参数注入 + 2 测试。
- `src-tauri/src/services/lint_service.rs` — `build_deep_lint_prompt(language)` 重构为参数注入 + 测试。
- `src-tauri/src/models/confirmation.rs` — `PendingAction.checkpoint_hash: Option<String>`（`#[serde(default)]`，None→null 不省略）+ 序列化测试 + 3 处测试构造补字段。
- `src-tauri/src/models/lint.rs` / `models/project.rs` / `services/project_service.rs` / `commands/import_commands.rs` — PendingAction 构造点补 `checkpoint_hash: None`。
- `src-tauri/src/commands/chat_commands.rs` — 读 language 传入 + BYOK 循环改用 `poll_with_progress("Answering")` + OverwriteFile PendingAction 补 `checkpoint_hash: None`。
- `src-tauri/src/commands/compile_commands.rs` — 读 language 传 compile_prompt/provider_prompt + compile 冲突 PendingAction 填 `checkpoint_hash` + BYOK 循环改用 `poll_with_progress("Generating")`。
- `src-tauri/src/commands/lint_commands.rs` — 读 language 传入 + BYOK 循环改用 `poll_with_progress("Linting")`。
- `src-tauri/src/commands/export_commands.rs` — 读 language 传入 + BYOK 循环改用 `poll_with_progress("Exporting")`。
- `src-tauri/src/lib.rs` — 托盘菜单读 `read_language()` + `tray_labels` 本地化 Show/Hide/Quit/tooltip。

## 遗留（非本 loop 范围，已记 roadmap 不动手）

- 前端 `PendingAction` TS 接口需加 `checkpointHash: string | null`，`ConfirmationDialog` 用其渲染"检查点：已建立/尚未建立"——属前端工作（本 loop 只动 src-tauri/）。后端 serde 已 camelCase 透传。
- 托盘菜单语言切换需重启窗口生效（Tauri 不支持运行时重建已有托盘菜单），未做动态重建。
- `cargo clippy`（full gui）21 条 pre-existing `needless_borrow` 告警，非本 loop 引入。

---

> 权威源：SPEC/roadmap/cross-cutting.md 第 2 节 · SPEC/PRD.md · CLAUDE.md「必读硬边界」
> scope：只动 src-tauri/。status: pending | in_progress | done | verified

## 关键决策（动手前）

- **i18n 语言注入策略**：`Settings.language` 是全局设置（存在 config_dir/settings.json，非项目级）。5 个 prompt 构造点中，4 个有 `&ProjectContext`（chat/compile/export/lint service）→ 走 `SettingsService::default().read_settings(context).language`；`compile_commands::provider_prompt` 只有 `workspace: &Path`（无 context，因为是自由函数）→ 改为接收 `language: &str` 参数，由调用方 `generate_manifest`（有 context）传入。注入方式：在 system instruction 末尾追加 `Respond in {language_name}.` 指令。确定性字段（JSON schema/path/枚举/frontmatter key）保持英文 — 只指令生成式正文（chat 回答、wiki 正文、HTML 正文、lint suggestion）的语言，不动结构化指令。语言名映射：zh-CN→"Simplified Chinese"，en→"English"，其它原样用 language code。
- **PendingAction.checkpoint_hash**：加 `Option<String>` 字段（camelCase serde → `checkpointHash`）。compile 冲突登记时填入 `checkpoint.commit_hash`（已在 CompileMerge execution 里存，现在同步到 PendingAction 顶层透传前端）。其它 PendingAction 构造点（lint pending / chat overwrite / import delete/replace / initialize folder）保持 `None`（这些是"确认后才 checkpoint"语义，前端显示"not created yet"正确）。前端 TS `PendingAction` 接口加 `checkpointHash: string | null` 属前端工作 — 本 loop 只动 src-tauri/，但后端 serde 已 camelCase 透传，前端按需加字段即可（记录为遗留前端接线）。
- **托盘 i18n**：lib.rs 构建托盘菜单时读 `SettingsService::default().read_language()`（新增方法，复用 `read_global_settings`）。新增 `tray_labels(language) -> (show, hide, quit, tooltip)` helper。zh-CN → "显示"/"隐藏"/"退出"/"LLM Wiki 桌面版"；en → "Show"/"Hide"/"Quit"/"LLM Wiki Desktop"。菜单在 setup 时构建一次；语言切换需重启窗口才生效（Tauri 限制，记遗留）— 不做动态重建（超出 P0 scope 且复杂）。
- **BYOK 流式进度**：4 个 BYOK 路径（compile/chat/lint/export）的 `tokio::select!` 循环目前每 100ms 只轮询取消。改为每 2s append 一条 `"Generating…"` Info 日志（首次立即 append 一条，之后每 2s 一条），保留 100ms 取消轮询。用独立计时器累加，不阻塞 completion future。最低风险方案（不改 LlmService 签名、不引真流式 SSE）。**实现**：抽出 `tasks/byok_progress.rs::poll_with_progress(task_service, task_id, verb, completion)`，签名 `Future<Output=Result<T,E>>` → `Result<Result<T,E>, ByokCancelled>`，调用方 `??` 双层解包（外层 ByokCancelled→域错误码，内层 provider 错误原样 `?`）。4 处 verb 分别 "Generating"/"Answering"/"Linting"/"Exporting"。
- **范围边界**：PendingAction 前端接线（ConfirmationDialog 用 checkpointHash）属前端工作，本 loop 不动 src/，只在后端把字段透传出去并记遗留。roadmap 2.6 P1（destructive 按钮 variant）纯前端，不动。

## 条目

### P0

- [x] **P0-1 i18n 5 prompt 点**（chat/compile/export/lint service + compile_commands::provider_prompt）— status: verified · `utils/i18n.rs:7` `services/chat_service.rs` `services/compile_service.rs` `services/export_service.rs` `services/lint_service.rs` `commands/compile_commands.rs::provider_prompt`
- [x] **P0-2 PendingAction.checkpoint_hash** + compile 冲突透传 — status: verified · `models/confirmation.rs:28` · compile 填入 `commands/compile_commands.rs`（CompileMerge 登记点）
- [x] **P0-3 托盘菜单 i18n**（Show/Hide/Quit + tooltip）— status: verified · `lib.rs:40-48` · `utils/i18n.rs::tray_labels`

### P1

- [x] **P1-1 BYOK 流式进度**（compile/chat/lint/export，每 ≤2s append "Generating…"）— status: verified · `tasks/byok_progress.rs:67` · 4 处调用 `commands/{compile,chat,lint,export}_commands.rs`

## 进度日志

- 2026-06-21 建账本；读全 5 prompt 构造点 + confirmation.rs + lib.rs + settings_service.rs，确认 language 在全局设置层、provider_prompt 需改签名接收 language。
- 2026-06-21 P0-1 落地：`utils/i18n.rs` 新增 `language_instruction`/`language_display_name` + 4 测试；5 个 prompt 点注入；chat 历史边界测试修正（15 条取末 8 → 含 index 7 不含 6）。
- 2026-06-21 P0-1 测试修复：export/lint 原读宿主真实全局 settings（host-state-dependent，flaky）→ 重构为 `language: &str` 参数注入，命令层读 SettingsService 传入。host-state-free 可测。
- 2026-06-21 P0-2 落地：`PendingAction.checkpoint_hash` 字段 + `#[serde(default)]`（None→null 不省略，配合前端 `!== null`）+ 序列化测试；所有构造点补字段；compile 冲突填入 commit_hash。
- 2026-06-21 P0-3 落地：`settings_service::read_language()` + 2 测试；lib.rs 托盘读 language + `tray_labels` 本地化。
- 2026-06-21 P1-1 落地：`tasks/byok_progress.rs::poll_with_progress` 统一 4 处 BYOK 轮询（取消 100ms + 进度 2s），`Result<Result<T,E>, ByokCancelled>` 双层解包；4 测试（立即进度/内层错误透传/取消/域错误码）。
- 2026-06-21 收敛：cargo test 259 passed / clippy(lib) clean / npm test 72 passed / npm lint clean。确认 full-gui clippy 21 条 needless_borrow 为 HEAD pre-existing（非本 loop 引入），按硬纪律不在范围内。
- 2026-06-21 双子代理审查（A 共享上下文 / B 全新上下文）通过后修复合并，重跑检查清单全绿。
