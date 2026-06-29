# Lint 后端 P0+P1 进度账本（loop: lint-be）

> 对照源：SPEC/roadmap/lint.md（§2 PRD-LINT-001/003、组件表 P0/P1 行）、SPEC/PRD.md、UI-Frontend-design/lint.html（只读）、CLAUDE.md
> 范围：仅 `src-tauri/`。本 loop 只做 P0+P1，不碰 P2、不碰前端、不碰 UI-Frontend-design/。
> 验证：Windows 上 `cargo test` 因 WebView2 加载期 0xc0000139 无法运行（见 gotchas.txt），改用 `cargo check --lib --tests` 作为代码正确性闸门；外加 `npm run test` + `npm run lint`。

## 本轮 scope（来自 loop 说明）
1. **P0 severity 分级**：后端真正发 error 级——死链 / index 漂移 → `Error`；frontmatter 缺失 → `Warning`（保持）。
2. **P0 批量自动修复编排**：新增 `apply_fixes_batch`（一次 Git 检查点 + 批量 manifest 执行 + 回滚 hash），当前只能逐条 apply（每条各做一次 checkpoint）。
3. **P0/P1 lint-ignore 持久化**：`.app/lint-ignore.json`，记录被忽略 issue（key=path+rule），扫描时排除。

## 关键决策
- **ignore key 粒度 = path + rule**（遵循 loop 说明）。DeadLink 同一页多条死链会一并忽略——这是本 loop 显式约定的粒度；如需 per-target，是前端+key 的后续改动（P2/前端，本 loop 不做）。存 `target` 仅作记录，不参与匹配。
- **批量 checkpoint 语义**：对全部 safe 路径在写盘前做**一次** `create_scoped_checkpoint`（HighRiskOperation），作为回滚点；safe 写盘逐条用前端提供的 `expected_hashes` 乐观锁。high-risk（死链/index 漂移）在批量阶段不写盘，只收集 PendingAction，复用既有 `apply_lint_fix(confirm=true, action_id)` 逐条确认（后端在批量命令里把每个 PendingAction 注册进 confirmation_registry）。
- **safe 写盘失败不整体回滚**：逐条收集 `skipped/failures`，checkpoint 已保护 pre-batch 状态可手动回滚（返回 checkpoint hash）。无 git 仓库 → 整批失败（Git 是数据安全硬边界，自动修复前必须有检查点）。
- **severity 仅升级**：DeadLink（lint_service.rs:71）、IndexDrift（check_index_drift 内 :278）Warning→Error。其余保持（MissingFrontmatter=Warning、OrphanPage=Info、其余 Warning）。

## 进度

### [P0] severity 分级 — status: verified
- 动：`lint_service.rs` DeadLink（:71）、IndexDrift（check_index_drift :278）`Warning`→`Error`。
- 文件:行号：`src-tauri/src/services/lint_service.rs`（DeadLink 构造 ~L68-86、IndexDrift 构造 ~L275-289）；新增测试 `severity_grading_marks_dead_link_and_index_drift_as_error`。
- 验证：`cargo check --lib --tests` ✅；`npm run test`（146 passed）✅；`npm run lint` ✅。`cargo test` 因 WebView2 0xc0000139 无法运行（环境问题，见 gotchas）。

### [P0] 批量自动修复 apply_fixes_batch — status: verified
- 动：`models/lint.rs`（DTO：`ApplyLintFixesBatchRequest`/`LintBatchOutcome`/`LintBatchConfirmation`/`LintBatchSkip`）；`lint_service.rs`（抽取共享写盘 helper `write_missing_frontmatter_fix`/`write_dead_link_fix`/`write_index_drift_fix` + `resolve_checkpoint`（共享/单条两种 checkpoint 源）+ 新 `apply_fixes_batch`）；`commands/lint_commands.rs`（`apply_lint_fixes` 命令，注册每个 high-risk PendingAction 到 confirmation_registry）；`lib.rs` 注册命令。
- 文件:行号：`src-tauri/src/models/lint.rs`（DTO ~L160-225）、`src-tauri/src/services/lint_service.rs`（`apply_fixes_batch` + write helpers ~L460-815）、`src-tauri/src/commands/lint_commands.rs`（`apply_lint_fixes` ~L281-310）、`src-tauri/src/lib.rs:186`。
- 关键决策：safe（MissingFrontmatter）先按 `expected_hashes` 分流——无 hash 的提前 skip，有 hash 的共用**一次** `create_scoped_checkpoint` 写盘；high-risk（DeadLink/IndexDrift）收集 `LintBatchConfirmation{issue, pending_action}`，命令层注册后前端逐条走既有 `apply_lint_fix(confirm=true, action_id)` 确认。无 safe 可写 → 不建 checkpoint。路径越界 → 整批 fail-fast。
- 验证：`cargo check --lib --tests` ✅（0 warning）；`npm run test`（146）✅；`npm run lint` ✅。`cargo test` 仍 0xc0000139（环境，见 gotchas），新增 3 测试：`batch_fix_uses_one_shared_checkpoint_for_safe_writes`（断言 2 safe→1 commit）、`batch_fix_collects_high_risk_skips_non_fixable_and_missing_hash`、`batch_fix_rejects_out_of_scope_path`。

### [P0/P1] lint-ignore 持久化 — status: verified
- 动：`models/lint.rs`（`LintIgnoreEntry`/`LintIgnoreFile` + `AddLintIgnoreRequest`/`RemoveLintIgnoreRequest`/`ListLintIgnoresRequest`）；`lint_service.rs`（`load_ignores`/`save_ignores`/`add_ignore`/`remove_ignore`/`list_ignores` + `run_local_lint` 内按 `(path, rule)` 过滤，常量 `LINT_IGNORE_PATH = ".app/lint-ignore.json"`）；`commands/lint_commands.rs`（`add_lint_ignore`/`remove_lint_ignore`/`list_lint_ignores`）；`lib.rs` 注册。
- 文件:行号：`src-tauri/src/models/lint.rs`（`LintIgnoreEntry`/`LintIgnoreFile` ~L227-275）、`src-tauri/src/services/lint_service.rs`（`load_ignores` 等 ~L278-355、过滤 ~L250-263、常量 L20）、`src-tauri/src/commands/lint_commands.rs`（三命令 ~L313-345）、`src-tauri/src/lib.rs:187-189`。
- 关键决策：key = `(path, rule)`（遵循 loop 说明）；损坏/缺失 ignore 文件不崩——`load_ignores` 对 `FILE_READ_FAILED` 静默回空、对其它读错误 eprintln 后回空（mirrors bookmarks reader）。add 去重（同 key 刷新 createdAt）。`write_atomic` 自动建 `.app/`。
- 验证：`cargo check --lib --tests` ✅（0 warning）；`cargo clippy --lib --tests` 我方代码 0 warning（余 3 个 warning 全在 app_state/export_service/import_service，本 loop 范围外，不动）；`npm run test`（146）✅；`npm run lint` ✅。新增 3 测试：`run_local_lint_excludes_ignored_issues`、`add_then_remove_lint_ignore_round_trips`、`run_local_lint_tolerates_corrupt_ignore_file`。
