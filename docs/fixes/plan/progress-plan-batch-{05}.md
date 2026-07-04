# Progress - plan-batch-{05}

Plan: D:\Users\Aletta\Desktop\Works\llm-wiki-desktop\docs\fixes\plan\plan-batch-{05}.md
Status: in_progress
Started: 2026-07-05 02:19

## step Progress

- [x] Task 1 - Task Log Timeline Sorting
- [x] Task 2 - Lint History Backend Persistence
- [x] Task 3 - Lint History Frontend Restore UX
- [x] Task 4 - Native Directory Picker and New Project Parent Path
- [x] Task 5 - Compact Project Start Page and Enriched Recents
- [ ] Task 6 - Regression Coverage, Review, and Quality Gates

## Activity Log

- [2026-07-05 04:22] Task 6 Step 5 完成：cargo check --manifest-path src-tauri/Cargo.toml --lib --tests 通过
- [2026-07-05 04:20] Task 6 Step 4 完成：扫描 src 下 console.log，无命中
- [2026-07-05 04:18] Task 6 Step 3 完成：npm run build 初次发现 RecentProject 测试 fixture 缺少 enriched metadata；补齐后重跑 npm run test、npm run lint、npm run build 均通过
- [2026-07-05 04:14] Task 6 Step 2 完成：npm run test 通过（298 tests），npm run lint 初次发现 projectPath 控制字符正则后已改为 codePoint 过滤并重跑通过
- [2026-07-05 04:10] Task 6 Step 1 完成：focused frontend tests 通过（85 tests；保留既有 jsdom canvas getContext 噪声）
- [2026-07-05 04:07] Task 5 Step 6 完成：新增 launch CSS contract，并补 projcard metadata 样式，CSS contract 测试通过
- [2026-07-05 04:03] Task 5 Step 5 完成：新增启动页三入口、native picker 打开已有项目、新建项目父目录生成路径回归测试，三组前端测试通过
- [2026-07-05 03:59] Task 5 Step 4 完成：recent project 卡片展示页面/来源/索引/图谱状态，缺失项目禁用并标记，App 测试通过
- [2026-07-05 03:56] Task 5 Step 3 完成：启动页 quick actions 改为新建、打开文件夹为项目、打开已有项目，移除手填路径表单与导入提示，App 测试通过
- [2026-07-05 03:51] Task 5 Step 2 完成：ProjectService list recents 只读 enrichment、缺失路径标记与 command summary 写入，cargo check 通过
- [2026-07-05 03:45] Task 5 Step 1 完成：扩展 RecentProject 前后端 DTO、Index/Graph 默认值与 legacy serde 测试，cargo check 通过
- [2026-07-05 03:38] Task 4 Step 7 完成：新增项目路径预览 CSS，projectPath/native picker/App 测试通过（保留既有 jsdom canvas getContext 噪声）
- [2026-07-05 03:35] Task 4 Step 6 完成：补充新建项目父目录选择中英文文案，App 测试通过（保留既有 jsdom canvas getContext 噪声）
- [2026-07-05 03:32] Task 4 Step 5 完成：NewProjectDialog 改为项目名 + 父目录选择 + 生成路径预览，App/projectPath 测试通过（保留既有 jsdom canvas getContext 噪声）
- [2026-07-05 03:29] Task 4 Step 4 完成：实现项目文件夹名清洗与父目录拼接 helper，project/import picker 测试通过
- [2026-07-05 03:27] Task 4 Step 3 完成：新增 projectPath 红灯测试，验证因 projectPath.ts 缺失而失败
- [2026-07-05 03:25] Task 4 Step 2 完成：实现 pickDirectory 单目录选择 helper，native picker 测试通过
- [2026-07-05 03:23] Task 4 Step 1 完成：新增 pickDirectory 红灯测试，验证因 pickDirectory 未实现而失败
- [2026-07-05 03:20] Task 3 Step 6 完成：新增 LintView 历史恢复/损坏报告测试与 lintStore history payload 测试，前端测试通过
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
- src/features/lint/lintView.test.tsx
- src/stores/lintStore.test.ts
- src/features/import/nativeFilePicker.test.ts
- src/features/import/nativeFilePicker.ts
- src/features/project/projectPath.test.ts
- src/features/project/projectPath.ts
- src/features/project/ProjectStartView.tsx
- src/i18n/locales/en.json
- src/i18n/locales/zh-CN.json
- src/styles.css
- src/types/project.ts
- src-tauri/src/models/project.rs
- src-tauri/src/commands/project_commands.rs
- src-tauri/src/services/project_service.rs
- src/app/App.test.tsx
- src/test/ui-css-contracts.test.ts
- src/features/project/projectPath.ts
- src/stores/projectStore.test.ts

## Verification

- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed (Task 6 Step 5)
- console.log scan: passed (Get-ChildItem src -Recurse -File | Select-String 'console\.log' returned no matches; Task 6 Step 4)
- npm run build: passed (Task 6 Step 3 rerun after RecentProject fixture update; Vite large chunk warning only)
- npm run lint: passed (Task 6 Step 3 rerun)
- npm run test: passed (47 files, 298 tests; jsdom canvas getContext warnings; Task 6 Step 3 rerun)
- npm run build: failed initially (RecentProject test fixtures missing enriched metadata fields)
- npm run test: passed (47 files, 298 tests; jsdom canvas getContext warnings; Task 6 Step 2 rerun)
- npm run lint: passed (Task 6 Step 2 rerun after projectPath no-control-regex fix)
- npm run lint: failed initially (no-control-regex in projectPath INVALID_FOLDER_CHARS)
- npm run test -- src/components/app/taskSort.test.ts src/components/app/TaskLogDrawer.test.tsx src/features/import/nativeFilePicker.test.ts src/features/project/projectPath.test.ts src/features/lint/lintView.test.tsx src/stores/lintStore.test.ts src/app/App.test.tsx src/test/ui-css-contracts.test.ts: passed (85 tests; Task 6 Step 1)
- npm run test -- src/test/ui-css-contracts.test.ts: passed (13 tests; Task 5 Step 6)
- npm run test -- src/app/App.test.tsx src/features/project/projectPath.test.ts src/features/import/nativeFilePicker.test.ts: passed (45 tests; jsdom canvas getContext warnings; Task 5 Step 5)
- npm run test -- src/app/App.test.tsx: passed (31 tests; jsdom canvas getContext warnings; Task 5 Step 4)
- npm run test -- src/app/App.test.tsx: passed (31 tests; jsdom canvas getContext warnings; Task 5 Step 3)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed (Task 5 Step 2)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed (Task 5 Step 1)
- npm run test -- src/features/project/projectPath.test.ts src/features/import/nativeFilePicker.test.ts src/app/App.test.tsx: passed (43 tests; jsdom canvas getContext warnings; Task 4 Step 7)
- npm run test -- src/app/App.test.tsx: passed (31 tests; jsdom canvas getContext warnings; Task 4 Step 6)
- npm run test -- src/app/App.test.tsx src/features/project/projectPath.test.ts: passed (37 tests; jsdom canvas getContext warnings; Task 4 Step 5)
- npm run test -- src/features/project/projectPath.test.ts src/features/import/nativeFilePicker.test.ts: passed (12 tests; Task 4 Step 4)
- npm run test -- src/features/project/projectPath.test.ts: failed as expected (missing ./projectPath; Task 4 Step 3)
- npm run test -- src/features/import/nativeFilePicker.test.ts: passed (6 tests; Task 4 Step 2)
- npm run test -- src/features/import/nativeFilePicker.test.ts: failed as expected (pickDirectory is not a function; Task 4 Step 1)
- npm run test -- src/features/lint/lintView.test.tsx src/stores/lintStore.test.ts: passed (19 tests; Task 3 Step 6)
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
