# Progress - plan-batch-{04}

Plan: D:\Users\Aletta\Desktop\Works\llm-wiki-desktop\docs\fixes\plan\plan-batch-{04}.md
Status: in_progress
Started: 2026-07-05 01:25

## step Progress

- [x] Task 1 - Backend DTO and shared export HTML resolver
- [x] Task 2 - Backend browser-open command
- [x] Task 3 - Export store and navigation focus state
- [x] Task 4 - AppShell focus integration
- [ ] Task 5 - Export rows, actions, and preview toolbar
- [ ] Task 6 - CSS and i18n
- [ ] Task 7 - Frontend regression tests
- [ ] Task 8 - Backend verification
- [ ] Task 9 - Required checks and review workflow

## Activity Log

- [2026-07-05 01:25] 开始阅读计划和项目规范
- [2026-07-05 01:25] 完成阅读 AGENTS/CLAUDE、项目 SPEC、设计参考、实施计划和相关源码，确认在既有 batch 01 导出布局基础上实施
- [2026-07-05 01:32] Task 1 红灯测试：新增浏览器打开 DTO 序列化测试和导出 HTML resolver 服务测试，cargo check 按预期失败于缺失 DTO/method
- [2026-07-05 01:34] Task 1 实现：新增 OpenExportInBrowserRequest，并添加共享 resolve_existing_html_export 路径/存在性校验
- [2026-07-05 01:39] Task 2 红灯检查：先注册 open_export_in_browser，cargo check 按预期失败于缺失 command
- [2026-07-05 01:41] Task 2 实现：新增 open_export_in_browser，并让 read_export_preview/open_export_folder 复用共享 HTML resolver
- [2026-07-05 01:33] Task 3 红灯测试：扩展 export/navigation store 测试，确认缺少 previewMode、openInBrowser 和 workspace focus actions
- [2026-07-05 01:34] Task 3 实现：新增导出预览模式、浏览器打开 action，以及 workspace focus 状态恢复逻辑
- [2026-07-05 01:39] Task 4 红灯测试：新增 AppShell workspace focus 用例，确认缺少 focus class、reopen 隐藏和 Escape 退出逻辑
- [2026-07-05 01:40] Task 4 实现：AppShell 使用 showRightPanel 派生右栏显示，并在 Escape 时优先清除 workspace focus

## Changed Files

- docs/fixes/plan/progress-plan-batch-{04}.md
- src-tauri/src/models/export.rs
- src-tauri/src/services/export_service.rs
- src-tauri/src/commands/export_commands.rs
- src-tauri/src/lib.rs
- src/types/export.ts
- src/stores/exportStore.ts
- src/stores/exportStore.test.ts
- src/stores/navigationStore.ts
- src/stores/navigationStore.test.ts
- src/components/app/AppShell.tsx
- src/components/app/appShellActions.test.tsx

## Verification

- npm run test: not run
- npm run test -- src/stores/exportStore.test.ts src/stores/navigationStore.test.ts: passed (Task 3)
- npm run test -- src/components/app/appShellActions.test.tsx: passed (Task 4)
- npm run lint: not run
- npm run build: not run
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: passed (Task 1, Task 2)
- cargo test --manifest-path src-tauri/Cargo.toml export: not run
- console.log scan: not run

## Blockers

- None
