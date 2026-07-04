# Progress - plan-batch-{03}

Plan: D:\Users\Aletta\Desktop\Works\llm-wiki-desktop\docs\fixes\plan\plan-batch-{03}.md
Status: in_progress
Started: 2026-07-05 00:05

## step Progress

- [x] Task 1 - Backend Bookmark Model/Service
- [x] Task 2 - Wire into Wiki scan/read/toggle
- [x] Task 3 - Frontend Wiki bookmark state/tree star
- [x] Task 4 - Export bookmark backend/store
- [x] Task 5 - Sidebar Favorites
- [x] Task 6 - Export Favorites UI
- [x] Task 7 - Chinese natural question search retrieval fix
- [x] Task 8 - Pinned Page Chat DTO/backend
- [x] Task 9 - Chat Store send options
- [x] Task 10 - Navigation right panel mode
- [x] Task 11 - Wiki Ask AI button + PageChatPanel
- [x] Task 12 - Citation UI pinned sources
- [ ] Task 13 - Styling/i18n
- [ ] Task 14 - Integration checks + review

## Activity Log

- [2026-07-05 00:05] 开始阅读计划和项目规范；确认当前分支为 task1-backend-contracts，工作区存在既有改动，将仅提交本批次相关文件。
- [2026-07-05 00:21] 完成 Task 1：新增 app-owned bookmark v2 model/service，支持旧数组/对象迁移、wiki/export toggle、路径边界校验和后端覆盖测试；`cargo check --manifest-path src-tauri/Cargo.toml` 通过，`cargo test --manifest-path src-tauri/Cargo.toml bookmark_service --lib` 受既有 Windows test runner `STATUS_ENTRYPOINT_NOT_FOUND` 阻断。
- [2026-07-05 00:43] 完成 Task 2：`SearchService::scan_wiki/read_page` 改为接收外部 bookmark path set，移除旧 writer/reader；Wiki IPC 继续保留 `toggle_bookmark` 但写入改由 `BookmarkService` 执行；graph/compile/lint/export 调用点完成迁移。
- [2026-07-05 00:52] 完成 Task 3：新增 `updateTreeNodeBookmark`，Wiki bookmark toggle 同步更新 flat pages 与递归 tree node；WikiTree 对 `bookmarked || starred` 渲染固定槽星标。
- [2026-07-05 01:08] 完成 Task 4：ExportRecord 增加派生 `bookmarked` 字段；新增 `toggle_export_bookmark` IPC；export list 从 BookmarkService join record id；exportStore 增加 `toggleBookmark` 并更新本地 records。
- [2026-07-05 01:19] 完成 Task 5：新增 FavoriteSidebarItem 类型和 bookmark selector；LeftSidebar 增加 Favorites section，可打开 Wiki favorites 或 Export preview；补充 i18n 与收起侧栏样式。
- [2026-07-05 01:27] 完成 Task 6：ExportsView 成功导出行增加收藏/取消收藏图标按钮，失败行不显示收藏按钮；补充 i18n 和 UI 测试。

- [2026-07-05 00:36] 完成 Task 7：SearchService 改为 Unicode 归一化、中文/ASCII 词项提取和字段加权打分；补充中文自然问句、Unicode lowercase、排序和无命中测试；`cargo check --manifest-path src-tauri/Cargo.toml --lib --tests` 通过，目标 `cargo test` 仍受已知 Windows runner `STATUS_ENTRYPOINT_NOT_FOUND` 阻断。
- [2026-07-05 00:44] 完成 Task 8：Chat DTO 增加 pinned page/citation 字段；retrieval context 支持当前 Wiki 页优先注入、去重和 prompt `Current Wiki page` 分区；chat IPC 透传 pinnedPagePath，TS 类型同步更新。
- [2026-07-05 00:45] 完成 Task 9：chatStore `send` 改为 options 参数，backend payload 固定发送 agent/provider/pinnedPagePath null fallback；补充 pinnedPagePath store 测试，保持全局 Chat 调用不变。
- [2026-07-05 00:54] 完成 Task 10：navigationStore 增加 rightPanelMode/wikiAssistantPagePath 与 open/close/update actions；补充右侧面板模式测试；已有 sidebar layout dirty 改动保持未纳入本步骤提交。
- [2026-07-05 00:59] 完成 Task 11：新增 PageChatPanel 并复用 ChatView 消息组件；Wiki toolbar 增加 Ask AI 图标按钮；RightContextPanel 可切换页面聊天；补充 PageChatPanel 测试和 i18n。
- [2026-07-05 01:00] 完成 Task 12：MessageBubble 与 RightContextPanel citation 列表显示 current page badge；PageChatPanel pinned citation 测试保持覆盖。

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
- src/features/bookmarks/bookmarkSelectors.ts
- src/features/bookmarks/bookmarkSelectors.test.ts
- src/types/bookmark.ts
- src/components/app/LeftSidebar.tsx
- src/i18n/locales/en.json
- src/i18n/locales/zh-CN.json
- src/styles.css
- src/features/exports/ExportsView.tsx
- src-tauri/src/models/chat.rs
- src-tauri/src/services/chat_service.rs
- src-tauri/src/commands/chat_commands.rs
- src/types/chat.ts
- src/stores/chatStore.ts
- src/stores/chatStore.test.ts
- src/stores/navigationStore.ts
- src/stores/navigationStore.test.ts
- src/features/chat/PageChatPanel.tsx
- src/features/chat/PageChatPanel.test.tsx
- src/features/chat/ChatView.tsx
- src/features/chat/ChatComposer.tsx
- src/features/wiki/WikiView.tsx
- src/components/app/RightContextPanel.tsx

## Verification

- npm run test: not run
- npm run lint: not run
- npm run build: not run
- cargo check --manifest-path src-tauri/Cargo.toml: pass
- cargo test --manifest-path src-tauri/Cargo.toml bookmark_service --lib: blocked by STATUS_ENTRYPOINT_NOT_FOUND before assertions
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: pass
- cargo test --manifest-path src-tauri/Cargo.toml search_service --lib: blocked by STATUS_ENTRYPOINT_NOT_FOUND before assertions
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: pass
- cargo test --manifest-path src-tauri/Cargo.toml chat --lib: blocked by STATUS_ENTRYPOINT_NOT_FOUND before assertions
- npm run test -- src/stores/chatStore.test.ts: pass
- npm run test -- src/stores/navigationStore.test.ts: pass
- npm run test -- src/features/chat/PageChatPanel.test.tsx src/features/chat/chatView.test.tsx: pass
- npm run test -- src/features/wiki/wiki.test.tsx: pass
- npm run test -- src/features/chat/PageChatPanel.test.tsx src/features/chat/chatView.test.tsx: pass
- npm run test -- src/features/wiki/wiki.test.tsx: pass
- npm run test -- src/stores/exportStore.test.ts src/features/exports/exportsView.test.tsx src/features/wiki/wiki.test.tsx: pass
- cargo test --manifest-path src-tauri/Cargo.toml export_service --lib: blocked by STATUS_ENTRYPOINT_NOT_FOUND before assertions
- npm run test -- src/features/bookmarks/bookmarkSelectors.test.ts src/features/exports/exportsView.test.tsx src/features/wiki/wiki.test.tsx: pass
- npm run lint -- --quiet: pass
- npm run test -- src/features/exports/exportsView.test.tsx: pass
- cargo check --manifest-path src-tauri/Cargo.toml --lib --tests: pass
- cargo test --manifest-path src-tauri/Cargo.toml search_service --lib: blocked by STATUS_ENTRYPOINT_NOT_FOUND before assertions

## Blockers

- None
