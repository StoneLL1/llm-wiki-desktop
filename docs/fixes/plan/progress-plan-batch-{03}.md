# Progress - plan-batch-{03}

Plan: D:\Users\Aletta\Desktop\Works\llm-wiki-desktop\docs\fixes\plan\plan-batch-{03}.md
Status: in_progress
Started: 2026-07-05 00:05

## step Progress

- [x] Task 1 - Backend Bookmark Model/Service
- [x] Task 2 - Wire into Wiki scan/read/toggle
- [x] Task 3 - Frontend Wiki bookmark state/tree star
- [x] Task 4 - Export bookmark backend/store
- [ ] Task 5 - Sidebar Favorites
- [ ] Task 6 - Export Favorites UI
- [ ] Task 7 - Chinese natural question search retrieval fix
- [ ] Task 8 - Pinned Page Chat DTO/backend
- [ ] Task 9 - Chat Store send options
- [ ] Task 10 - Navigation right panel mode
- [ ] Task 11 - Wiki Ask AI button + PageChatPanel
- [ ] Task 12 - Citation UI pinned sources
- [ ] Task 13 - Styling/i18n
- [ ] Task 14 - Integration checks + review

## Activity Log

- [2026-07-05 00:05] 开始阅读计划和项目规范；确认当前分支为 task1-backend-contracts，工作区存在既有改动，将仅提交本批次相关文件。
- [2026-07-05 00:21] 完成 Task 1：新增 app-owned bookmark v2 model/service，支持旧数组/对象迁移、wiki/export toggle、路径边界校验和后端覆盖测试；`cargo check --manifest-path src-tauri/Cargo.toml` 通过，`cargo test --manifest-path src-tauri/Cargo.toml bookmark_service --lib` 受既有 Windows test runner `STATUS_ENTRYPOINT_NOT_FOUND` 阻断。
- [2026-07-05 00:43] 完成 Task 2：`SearchService::scan_wiki/read_page` 改为接收外部 bookmark path set，移除旧 writer/reader；Wiki IPC 继续保留 `toggle_bookmark` 但写入改由 `BookmarkService` 执行；graph/compile/lint/export 调用点完成迁移。
- [2026-07-05 00:52] 完成 Task 3：新增 `updateTreeNodeBookmark`，Wiki bookmark toggle 同步更新 flat pages 与递归 tree node；WikiTree 对 `bookmarked || starred` 渲染固定槽星标。
- [2026-07-05 01:08] 完成 Task 4：ExportRecord 增加派生 `bookmarked` 字段；新增 `toggle_export_bookmark` IPC；export list 从 BookmarkService join record id；exportStore 增加 `toggleBookmark` 并更新本地 records。

## Changed Files

- docs/fixes/plan/progress-plan-batch-{03}.md
- src-tauri/src/models/bookmark.rs
- src-tauri/src/models/mod.rs
- src-tauri/src/app_state.rs
- src-tauri/src/commands/compile_commands.rs
- src-tauri/src/commands/graph_commands.rs
- src-tauri/src/commands/wiki_commands.rs
- src-tauri/src/services/export_service.rs
- src-tauri/src/services/lint_service.rs
- src-tauri/src/services/bookmark_service.rs
- src-tauri/src/services/mod.rs
- src-tauri/src/services/search_service.rs
- src-tauri/tests/mvp_flow.rs
- src/features/wiki/wiki.test.tsx
- src/features/wiki/wikiStore.ts
- src/features/wiki/WikiTree.tsx
- src-tauri/src/models/export.rs
- src-tauri/src/commands/export_commands.rs
- src-tauri/src/lib.rs
- src/stores/exportStore.ts
- src/stores/exportStore.test.ts
- src/types/export.ts
- src/features/exports/exportsView.test.tsx

## Verification

- npm run test: not run
- npm run lint: not run
- npm run build: not run
- cargo check --manifest-path src-tauri/Cargo.toml: pass
- cargo test --manifest-path src-tauri/Cargo.toml bookmark_service --lib: blocked by STATUS_ENTRYPOINT_NOT_FOUND before assertions
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: pass
- cargo test --manifest-path src-tauri/Cargo.toml search_service --lib: blocked by STATUS_ENTRYPOINT_NOT_FOUND before assertions
- npm run test -- src/features/wiki/wiki.test.tsx: pass
- npm run test -- src/stores/exportStore.test.ts src/features/exports/exportsView.test.tsx src/features/wiki/wiki.test.tsx: pass
- cargo test --manifest-path src-tauri/Cargo.toml export_service --lib: blocked by STATUS_ENTRYPOINT_NOT_FOUND before assertions

## Blockers

- None
