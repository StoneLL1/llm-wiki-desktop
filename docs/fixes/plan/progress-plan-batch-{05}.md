# Progress - plan-batch-{05}

Plan: D:\Users\Aletta\Desktop\Works\llm-wiki-desktop\docs\fixes\plan\plan-batch-{05}.md
Status: in_progress
Started: 2026-07-05 02:19

## step Progress

- [x] Task 1 - Task Log Timeline Sorting
- [x] Task 2 - Lint History Backend Persistence
- [ ] Task 3 - Lint History Frontend Restore UX
- [ ] Task 4 - Native Directory Picker and New Project Parent Path
- [ ] Task 5 - Compact Project Start Page and Enriched Recents
- [ ] Task 6 - Regression Coverage, Review, and Quality Gates

## Activity Log

- [2026-07-05 03:16] Task 3 Step 5 完成：补充 Lint 历史中英文文案，lintView 测试通过
- [2026-07-05 03:13] Task 3 Step 4 完成：LintView 集成历史列表、加载历史并自动打开最新报告，lintView 测试通过
- [2026-07-05 03:10] Task 3 Step 3 完成：新增 LintHistoryList 组件与紧凑历史列表 CSS，lintView 测试通过
- [2026-07-05 03:06] Task 3 Step 2 完成：lintStore 增加历史状态、load/open actions，并在 local/deep 报告成功后刷新历史，前端测试通过
- [2026-07-05 03:02] Task 3 Step 1 完成：新增前端 Lint 历史 DTO，lintView 测试通过
- [2026-07-05 02:59] Task 2 Step 4 完成：run_local_lint/deep lint 接入历史持久化，新增 list/read lint history commands 并注册，cargo check 无警告通过
- [2026-07-05 02:53] Task 2 Step 3 完成：新增 Lint 历史本地持久化、50 条上限与单报告损坏回归测试，cargo check 通过
- [2026-07-05 02:48] Task 2 Step 2 完成：LintService 增加历史索引、报告 wrapper 持久化、legacy deep report 兼容读取与 ID 校验，cargo check 通过
- [2026-07-05 02:42] Task 2 Step 1 完成：新增 Lint 历史 DTO、默认值与 serde 省略报告体测试，cargo check 通过
- [2026-07-05 02:31] Task 1 Step 5 完成：新增 TaskService updated_at 倒序回归测试，并通过 cargo check --lib --tests
- [2026-07-05 02:25] Task 1 Step 4 完成：补充 TaskLogDrawer 默认时间排序和切换状态排序保持选中任务的测试
- [2026-07-05 02:24] Task 1 Step 3 完成：TaskLogDrawer 接入排序偏好与 segmented control，补充中英文文案和 CSS
- [2026-07-05 02:23] Task 1 Step 2 完成：实现 taskSort.ts，排序与偏好测试通过
- [2026-07-05 02:22] Task 1 Step 1 完成：新增 taskSort 红灯测试，验证因 taskSort.ts 缺失而失败
- [2026-07-05 02:19] 开始阅读计划和项目规范
- [2026-07-05 02:19] 已读取 AGENTS.md、CLAUDE.md、核心 SPEC 文档、设计参考与 plan-batch-{05}.md
- [2026-07-05 02:19] 确认当前分支为 task1-backend-contracts，工作区已有未提交/未跟踪改动；本计划将只改动计划相关文件并保留既有改动

## Changed Files

- docs/fixes/plan/progress-plan-batch-{05}.md
- src/components/app/TaskLogDrawer.test.tsx
- src/components/app/TaskLogDrawer.tsx
- src/components/app/taskSort.ts
- src/components/app/taskSort.test.ts
- src/i18n/locales/en.json
- src/i18n/locales/zh-CN.json
- src/styles.css
- src-tauri/src/tasks/task_service.rs
- src-tauri/src/models/lint.rs
- src-tauri/src/services/lint_service.rs
- src-tauri/src/commands/lint_commands.rs
- src-tauri/src/lib.rs
- src/types/lint.ts
- src/stores/lintStore.ts
- src/features/lint/LintHistoryList.tsx
- src/styles.css
- src/features/lint/LintView.tsx
- src/i18n/locales/en.json
- src/i18n/locales/zh-CN.json

## Verification

- npm run test -- src/features/lint/lintView.test.tsx: passed (5 tests; Task 3 Step 5)
- npm run test -- src/features/lint/lintView.test.tsx: passed (5 tests; Task 3 Step 4)
- npm run test -- src/features/lint/lintView.test.tsx: passed (5 tests; Task 3 Step 3)
- npm run test -- src/stores/lintStore.test.ts src/features/lint/lintView.test.tsx: passed (15 tests; Task 3 Step 2)
- npm run test -- src/features/lint/lintView.test.tsx: passed (5 tests; Task 3 Step 1)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed (Task 2 Step 4)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed (Task 2 Step 3)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed (Task 2 Step 2)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed (Task 2 Step 1)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed
- npm run test -- src/components/app/TaskLogDrawer.test.tsx src/components/app/taskSort.test.ts: passed (8 tests)
- npm run test -- src/components/app/taskSort.test.ts: passed (5 tests)
- npm run test -- src/components/app/TaskLogDrawer.test.tsx src/components/app/taskSort.test.ts: passed (6 tests)
- npm run test -- src/components/app/taskSort.test.ts: passed (5 tests)
- npm run test -- src/components/app/taskSort.test.ts: failed as expected (missing ./taskSort implementation)
- npm run test: not run
- npm run lint: not run
- npm run build: not run

## Blockers

- None
