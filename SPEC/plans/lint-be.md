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

### [P0] 批量自动修复 apply_fixes_batch — status: pending
- 动：`models/lint.rs`（DTO：`ApplyLintFixesBatchRequest`/`LintBatchOutcome`/`LintBatchSkip`）；`lint_service.rs`（抽取 `write_missing_frontmatter_fix`/`write_dead_link_fix`/`write_index_drift_fix` 共享写盘 helper + 新 `apply_fixes_batch`）；`commands/lint_commands.rs`（`apply_lint_fixes` 命令，注册 high-risk PendingAction）；`lib.rs` 注册命令。
- 测试：`batch_fix_creates_one_checkpoint_applies_safe_collects_high_risk`；`batch_fix_rejects_when_no_git_checkpoint`。

### [P0/P1] lint-ignore 持久化 — status: pending
- 动：`models/lint.rs`（`LintIgnoreEntry`/`LintIgnoreFile`）；`lint_service.rs`（`load_ignores`/`save_ignore`/`remove_ignore` + `run_local_lint` 过滤）；`commands/lint_commands.rs`（`add_lint_ignore`/`remove_lint_ignore`/`list_lint_ignores`）；`lib.rs` 注册。
- 测试：`run_local_lint_excludes_ignored_issues`；`add_then_remove_lint_ignore_round_trips`。
