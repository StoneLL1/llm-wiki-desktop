# Progress - plan-batch-{01}

Plan: D:\Users\Aletta\Desktop\Works\llm-wiki-desktop\docs\fixes\plan\plan-batch-{01}.md
Status: in_progress
Started: 2026-07-04 18:38

## Task Progress

- [x] Task 1 - Layout State Model
- [x] Task 2 - Accessible Splitter Primitive
- [x] Task 3 - Shell Splitters and Collapsible Sidebar
- [x] Task 4 - Wiki, Exports, and Lint Internal Splitters
- [x] Task 5 - Compact Project Switcher
- [x] Task 6 - Color Theme Presets and Settings Contract
- [x] Task 7 - Appearance UI and Reading Tokens
- [ ] Task 8 - Regression Coverage and Quality Gates

## Activity Log

- [2026-07-04 19:34] Task 8 Step 1 完成：计划指定 focused frontend tests 通过，覆盖 resizable pane、layout store、path display、color presets、Appearance UI、App shell actions 和 CSS contracts。
- [2026-07-04 19:32] Task 7 Step 5 完成：新增 AppearanceSettings UI test，覆盖内置 preset、无任意颜色输入和 preset selection callback；focused Appearance/colorTheme tests 通过。
- [2026-07-04 19:29] Task 7 Step 4 完成：新增 Appearance color theme/Markdown preview 与 6 个 preset 的中英文 i18n 文案，preset metadata 改用计划要求的 `themePreset.*` key。
- [2026-07-04 19:26] Task 7 Step 3 完成：新增 reading token 默认值，Wiki/Chat Markdown、HTML preview 外壳和 Appearance Markdown preview 改用 reading token，并补齐 preset grid 样式。
- [2026-07-04 19:22] Task 7 Step 2 完成：AppearanceSettings 新增内置 color theme preset radiogroup、色块、选中状态和 Markdown reading preview 结构。
- [2026-07-04 19:20] Task 7 Step 1 完成：AppearanceSettings props 扩展为接收 `colorThemePreset` 与 preset change callback，SettingsView 通过现有 settings store savePatch 接线全局颜色主题偏好。
- [2026-07-04 19:16] Task 6 Step 5 完成：Rust settings DTO 新增 global-only `colorThemePreset` 契约、legacy 默认值和 global/project split 测试；`cargo test` 编译后命中已知 Windows loader 0xc0000139，`cargo check --lib --tests` 通过。
- [2026-07-04 19:07] Task 6 Step 4 完成：settings store 新增 color theme preset root/localStorage application helper，并在 load、optimistic save、saved response、rollback 中同步应用；focused settings store test 通过。
- [2026-07-04 19:06] Task 6 Step 1/2/3 完成：新增 color theme preset 测试、TS settings preset 类型/defaultSettings 和 preset metadata/root application helper；focused preset test 通过。
- [2026-07-04 19:03] Task 5 Step 5/6/7/8 完成：TopBar 使用 compact path 和 full-path title，recent menu 显示缺失状态与 openedAt meta，缺失项目不会打开，Arrow/Escape 键盘菜单行为和 compact switcher CSS 已覆盖；Task 5 前端 tests 与 cargo check 通过。
- [2026-07-04 19:00] Task 5 Step 4 完成：TypeScript `RecentProject` DTO 新增 `missing` 字段并更新 project store fixture；focused projectStore test 通过。
- [2026-07-04 18:58] Task 5 Step 3 完成：Rust `RecentProject` 新增 `missing` 默认字段，recent 列表按路径存在性动态标记，所有构造点显式写入 `missing: false`；`cargo test` 命中已知 Windows loader 0xc0000139，`cargo check --lib --tests` 通过。
- [2026-07-04 18:56] Task 5 Step 1/2 完成：新增 `compactPath` 红灯测试与路径压缩 helper，覆盖 Windows drive、UNC、POSIX、短路径和 CJK leaf name；focused helper test 通过。
- [2026-07-04 18:55] Task 4 完成：Wiki tree、Exports list、Lint issue list 接入 `ResizableSplitter`，新增内部 pane 宽度 CSS 变量、响应式隐藏规则和中英文 i18n 键；计划指定 focused tests 通过。
- [2026-07-04 18:51] Task 3 完成：新增 shell 级 sidebar/right panel splitters、顶部栏侧边栏折叠按钮、折叠侧栏可访问导航名称、响应式 splitter 隐藏规则和中英文 i18n 键；focused App/CSS tests 通过。
- [2026-07-04 18:46] Task 2 Step 3 完成：新增 `.resize-handle`、focus-visible/hover/drag 状态和 `body.is-resizing-pane` CSS contract；Task 2 focused tests 通过。
- [2026-07-04 18:45] Task 2 Step 2 完成：新增 `ResizableSplitter` 组件，映射 separator role、aria value 属性、pane id、方向和 reset/change 回调。
- [2026-07-04 18:44] Task 2 Step 1 完成：扩展 `useResizablePane` pointer/keyboard resize hook，覆盖 Arrow/Home/End/Enter、direction=-1 pointer delta、pointer cleanup 和 body resizing class。
- [2026-07-04 18:42] Task 1 Step 3 完成：扩展 `navigationStore` 的 `sidebarCollapsed`、`paneSizes`、collapse/pane setter 和 reset API；layout-changing setter 会写入 sanitized localStorage snapshot，active view/right panel open 不写 layout preferences。
- [2026-07-04 18:41] Task 1 Step 2 完成：新增 `useResizablePane` 布局常量、宽度 clamp、layout preference sanitize/read/write helper；单测已通过。
- [2026-07-04 18:40] Task 1 Step 2 绿灯前发现计划文字与测试数据不一致：文字称 NaN 用 min/max 中点，但指定测试要求已知 pane 默认值；实现按测试契约处理，并把任意 min/max 中点作为兜底。`SPEC/gotchas.txt` 当前已有用户未提交改动且终端编码无法稳定 patch，未强行写入。
- [2026-07-04 18:39] Task 1 Step 1 完成：新增 layout preference 红灯测试；`npm run test -- src/hooks/useResizablePane.test.ts` 按预期失败，原因是 `./useResizablePane` 尚不存在。
- [2026-07-04 18:38] 开始阅读计划和项目规范；已读取 AGENTS.md、SPEC/PRD.md、SPEC/SPEC.md、SPEC/APP_flow.md、SPEC/TECH_STACK.md、SPEC/BACKEND_STRUCTURE.md、SPEC/FRONTEND_GUIDELINES.md、SPEC/DESIGN.md 和 UI-Frontend-design 参考文件。
- [2026-07-04 18:38] 确认当前分支 task1-backend-contracts 存在既有未提交修改；后续仅提交计划相关文件并避免覆盖用户改动。

## Changed Files

- src/styles.css
- src/test/ui-css-contracts.test.ts
- src/app/App.test.tsx
- src/components/app/AppShell.tsx
- src/components/app/LeftSidebar.tsx
- src/components/app/TopBar.tsx
- src/features/exports/ExportsView.tsx
- src/features/exports/exportsView.test.tsx
- src/features/lint/LintView.tsx
- src/features/lint/lintView.test.tsx
- src/features/wiki/WikiView.tsx
- src/features/wiki/wiki.test.tsx
- src/lib/pathDisplay.ts
- src/lib/pathDisplay.test.ts
- src-tauri/src/commands/project_commands.rs
- src-tauri/src/models/project.rs
- src-tauri/src/services/project_service.rs
- src/stores/projectStore.test.ts
- src/types/project.ts
- src/app/App.test.tsx
- src/components/app/TopBar.tsx
- src/features/settings/AppearanceSettings.tsx
- src/features/settings/AppearanceSettings.test.tsx
- src/features/settings/SettingsView.tsx
- src/lib/colorThemePresets.ts
- src/lib/colorThemePresets.test.ts
- src/stores/settingsStore.ts
- src/stores/settingsStore.test.ts
- src/types/settings.ts
- src-tauri/src/models/settings.rs
- src/i18n/locales/en.json
- src/i18n/locales/zh-CN.json
- src/styles.css
- src/test/ui-css-contracts.test.ts
- src/components/app/ResizableSplitter.tsx
- src/i18n/locales/en.json
- src/i18n/locales/zh-CN.json
- src/stores/navigationStore.ts
- src/stores/navigationStore.test.ts
- src/hooks/useResizablePane.ts
- src/hooks/useResizablePane.test.ts
- docs/fixes/plan/progress-plan-batch-{01}.md

## Verification

- npm run test -- src/hooks/useResizablePane.test.ts src/stores/navigationStore.test.ts src/lib/pathDisplay.test.ts src/lib/colorThemePresets.test.ts src/features/settings/AppearanceSettings.test.tsx src/app/App.test.tsx src/components/app/appShellActions.test.tsx src/test/ui-css-contracts.test.ts: passed (62 tests)
- npm run test -- src/features/settings/AppearanceSettings.test.tsx src/lib/colorThemePresets.test.ts: passed (5 tests)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed
- cargo test --manifest-path src-tauri/Cargo.toml settings::tests::color_theme_preset_is_global_and_legacy_safe: failed due known Windows loader 0xc0000139 after successful compile
- npm run test -- src/stores/settingsStore.test.ts: passed (1 test)
- npm run test -- src/stores/settingsStore.test.ts: failed as expected (document root preset not applied)
- npm run test -- src/lib/colorThemePresets.test.ts: passed (3 tests)
- npm run test -- src/lib/colorThemePresets.test.ts: failed as expected (missing ./colorThemePresets)
- npm run test -- src/lib/pathDisplay.test.ts src/app/App.test.tsx src/test/ui-css-contracts.test.ts: passed (43 tests)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed
- npm run test -- src/stores/projectStore.test.ts: passed (3 tests)
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed
- cargo test --manifest-path src-tauri/Cargo.toml project::tests::recent_project_missing_defaults_to_false_for_legacy_json: failed due known Windows loader 0xc0000139 after successful compile
- npm run test -- src/lib/pathDisplay.test.ts: passed (5 tests)
- npm run test -- src/lib/pathDisplay.test.ts: failed as expected (missing ./pathDisplay)
- npm run test -- src/features/wiki/wiki.test.tsx src/features/exports/exportsView.test.tsx src/features/lint/lintView.test.tsx src/test/ui-css-contracts.test.ts: passed (51 tests)
- npm run test -- src/app/App.test.tsx src/test/ui-css-contracts.test.ts: passed (33 tests)
- npm run test -- src/hooks/useResizablePane.test.ts src/test/ui-css-contracts.test.ts: passed (12 tests)
- npm run test -- src/hooks/useResizablePane.test.ts: passed (6 tests)
- npm run test -- src/hooks/useResizablePane.test.ts: passed (6 tests)
- npm run test -- src/stores/navigationStore.test.ts src/hooks/useResizablePane.test.ts: passed (6 tests)
- npm run test -- src/hooks/useResizablePane.test.ts: passed (4 tests)
- npm run test -- src/hooks/useResizablePane.test.ts: failed as expected (missing ./useResizablePane)
- npm run test: not run
- npm run lint: not run
- npm run build: not run
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: not run

## Blockers

- None
