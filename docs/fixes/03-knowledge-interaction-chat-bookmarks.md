# 03 Knowledge Interaction, Chat, and Bookmarks Specification

本规范整合以下条目：页面收藏星标、Chat “上下文不足” bug、Wiki 页面侧边栏追问。

## 条目 A：页面收藏星标

## 1. 需求概述
- 用户想要什么：在 Wiki 页面中星标后，中间文件树对应页面右侧显示星星；左侧栏在工作流下方、最近页面上方新增“精选页面”；除 Markdown 页面外，生成的 HTML 导出页面也可收藏/星标。
- 为什么：用户需要把重要知识页面和导出成品固定在高频入口，而不是每次通过搜索或文件树寻找。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/features/wiki/WikiView.tsx`、`src/features/wiki/WikiTree.tsx`、`src/features/wiki/wikiStore.ts`、`src-tauri/src/services/search_service.rs`、`src-tauri/src/commands/wiki_commands.rs`、`src-tauri/src/models/wiki.rs`、`src/types/wiki.ts`、`src/components/app/LeftSidebar.tsx`、`src/stores/exportStore.ts`、`src/features/exports/ExportsView.tsx`、`src-tauri/src/services/export_service.rs`、`src-tauri/src/models/export.rs`。
- 当前行为是什么：Wiki toolbar 已有 `toggleBookmark`；`WikiPageMeta` 与 `WikiTreeNode` 已同时有 `starred` 与 `bookmarked` 字段；`WikiTree` 渲染星标时历史上更多依赖 `starred`；HTML export record 没有收藏字段。
- 问题出在哪里：产品语义混杂。`frontmatter starred` 和 `.app/bookmarks.json bookmarked` 同时存在，用户操作的是 bookmark，但部分 UI 显示 starred，导致星标后树节点可能不更新；HTML 导出没有 resource type，不能复用 wiki page path。

## 3. 方案设计
- 第一性原理：收藏是用户私有应用状态，应存 `.app/`；页面 frontmatter 的 `starred` 是内容元数据，可能来自 Agent/schema。用户点击星星应统一操作 app-level favorite，不改 Markdown 正文。
- 推荐方案：将 UI 里的“星标/精选”统一映射到 `.app/bookmarks.json`，并让 frontmatter `starred` 只作为导入/兼容信号。新增通用 favorite resource schema，支持 `wiki_page` 和 `export_html`。
- 备选方案 1：直接写 frontmatter `starred: true`。优点是 Obsidian 可见；缺点是用户 UI 偏好会修改知识内容并触发图谱/搜索变更，不推荐。
- 技术方案：
  - 新增 Rust persistence model `src-tauri/src/models/bookmark.rs`：
    - `pub enum BookmarkResourceKind { WikiPage, ExportHtml }`
    - `pub struct BookmarkEntry { pub id: String, pub kind: BookmarkResourceKind, pub path: String, pub title: String, pub export_record_id: Option<String>, pub created_at: String }`
    - `pub struct BookmarkFile { pub version: u32, pub entries: Vec<BookmarkEntry> }`
  - 新增 `src-tauri/src/services/bookmark_service.rs`：
    - `pub fn list_bookmarks(&self, context: &ProjectContext) -> Result<BookmarkFile, BackendError>`
    - `pub fn toggle_bookmark(&self, context: &ProjectContext, kind: BookmarkResourceKind, path: &str, title: &str, export_record_id: Option<&str>) -> Result<BookmarkToggleResult, BackendError>`
  - 短期兼容：现有 `.app/bookmarks.json` 如是旧 `Vec<String>` 或旧 page map，读取时迁移到 `BookmarkFile`，写回新 schema。
  - 修改 `search_service.rs::scan_wiki`，对每个 `WikiPageMeta.bookmarked` 从 BookmarkService 的 `WikiPage` entry 推断；`WikiTreeNode.bookmarked` 同步写入 root tree 节点。
  - 修改 `wikiStore.toggleBookmark(projectId, rootPath)`，调用现有 `toggle_bookmark` 或新 `toggle_bookmark_resource` 后，同时更新 `page.meta`、`tree.pages` 和 `tree.root` 中对应节点。
  - 修改 `WikiTree.tsx`，文件行右侧显示 `bookmarked || starred` 星星；用户操作后的 app favorite 优先。
  - 修改 `LeftSidebar.tsx`，新增“精选页面”section，位置在 workflowViews 下方、recentPages 上方，列表来自 `selectFavoriteWikiPages(tree)` 与 favorite HTML entries 的合并。
  - 修改 Export：
    - `src-tauri/src/models/export.rs::ExportRecord` 增加 `bookmarked: bool` 或通过 BookmarkService join；HTML favorite 以 `exportRecordId` 作为主身份，`outputPath` 作为文件定位 fallback，避免同一路径/重复生成时无法回到正确记录。
    - 新增 command `toggle_export_bookmark(request: ToggleExportBookmarkRequest)`.
    - `src/features/exports/ExportsView.tsx` 每条成功导出右侧显示 Star icon。
- 需要新增哪些文件：`src-tauri/src/models/bookmark.rs`、`src-tauri/src/services/bookmark_service.rs`、`src/types/bookmark.ts`、可选 `src/features/wiki/bookmarkSelectors.ts`。
- 需要修改哪些文件：`src-tauri/src/models/mod.rs`、`src-tauri/src/services/mod.rs`、`app_state.rs`、`wiki_commands.rs`、`export_commands.rs`、`search_service.rs`、`export_service.rs`、`wikiStore.ts`、`WikiTree.tsx`、`LeftSidebar.tsx`、`ExportsView.tsx`、`exportStore.ts`、i18n、styles。
- 是否需要新增依赖：不需要。

## 4. UI / 交互设计
- 界面变化描述：文件树每个 Markdown file row 尾部固定 20px action slot，收藏时显示实心星，未收藏 hover 显示空心星；左侧“精选页面”使用小星图标 + title，HTML 导出项用 `FileOutput` 或 `Star` 区分。
- 交互流程：用户在 Wiki toolbar 点星星 -> 后端写 `.app/bookmarks.json` -> WikiTree 行出现星星 -> LeftSidebar 精选页面即时出现；用户在 Export list 点星星 -> HTML entry 加入精选 -> 点击精选 HTML -> 进入 Exports 并预览该 record。
- 需要参考的设计规范：`Spec/SPEC.md` “收藏与星标”；`Spec/FRONTEND_GUIDELINES.md` list row anatomy；AGENTS 对左侧三分组和设计 HTML 结构的要求。

## 5. 验收标准（Done Definition）
- [ ] Wiki 页面星标后，WikiTree 对应文件行右侧立即显示星星，刷新后仍存在。
- [ ] LeftSidebar 在工作流下方、最近页面上方显示“精选页面”，含 wiki favorites 和 HTML favorites。
- [ ] 对同一页面取消星标后，WikiTree 与 LeftSidebar 同步移除。
- [ ] HTML export 成功记录可星标，刷新后仍存在，并可从精选页面打开预览。
- [ ] 旧 `.app/bookmarks.json` 可被兼容读取，不造成收藏丢失。

## 6. 风险与注意事项
- 可能影响的现有功能：`starred` frontmatter 仍用于搜索/图谱突出时，不能被删除；新的 `bookmarked` 不能触发 Markdown 文件修改。
- 边界情况：导出文件被删除时，保留 favorite 但显示 missing；Wiki 页面被重命名时可按旧 path 显示失效；同名 HTML 多次导出时优先用 `exportRecordId` 打开原记录，`outputPath` 仅作 fallback；项目切换时 favorites 不得串项目。

## 7. 实施步骤
- [ ] 设计并测试 BookmarkFile 兼容 reader。
- [ ] 把 BookmarkService 接入 AppState。
- [ ] 修改 wiki scan/toggle，确保 tree.root 与 tree.pages 同步。
- [ ] 实现 LeftSidebar 精选页面。
- [ ] 扩展 export bookmark command/store/UI。
- [ ] 补 WikiTree/LeftSidebar/ExportsView 测试。

## 条目 B：BUG：Chat 提问显示“上下文不足”

## 1. 这是一个重要的bug

- [ ] 问题回归：
  提问的时候显示

  根据提供的上下文，我无法回答"什么是约束先行2"这个问题。

  提供的资料中只包含了该知识库的目的说明（Purpose页面），其中描述了知识库的目标、范围和方向等元信息，但并没有收录任何关于"约束先行"概念的实际内容。Sources部分为空，也没有其他可引用的页面。

1. 这是一个重要的bug，请你进行对抗式审查来进行调研和修复

## 条目 C：Wiki 每个页面中调出侧边栏继续对文章提问

## 1. 需求概述
- 用户想要什么：每个 Wiki 页面上有一个按钮，可以调出侧边栏，围绕当前文章继续向 AI 提问和追问。
- 为什么：用户阅读文章时的问题通常与当前页面强相关，跳到全局 Chat 会丢失阅读上下文和思路。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/features/wiki/WikiView.tsx`、`MarkdownReader.tsx`、`src/components/app/RightContextPanel.tsx`、`src/features/chat/ChatView.tsx`、`ChatComposer.tsx`、`src/stores/chatStore.ts`、`src-tauri/src/services/chat_service.rs`、`src-tauri/src/commands/chat_commands.rs`、`src-tauri/src/models/chat.rs`、`src/types/chat.ts`、`src/stores/navigationStore.ts`。
- 当前行为是什么：Wiki 右侧面板显示元数据、相关文章和导出；Chat 视图有完整会话与 citation panel；后端 retrieval 只按 query 检索相关页面，未支持“固定当前页作为强上下文”。
- 问题出在哪里：UI 入口和后端 DTO 都缺少 page-scoped chat；如果只把页面标题拼进用户问题，不稳定且无法保证引用当前页。

## 3. 方案设计
- 第一性原理：页面追问的本质是“以当前页面为强制上下文的 Chat 子模式”。它应复用 ChatService/ChatStore，而不是另造一套 AI 调用。
- 推荐方案：扩展 Chat DTO 支持 `pinnedPagePath?: string`，Wiki toolbar 的 `Ask AI` 按钮打开右侧面板模式 `wikiAssistant`，该模式复用 `ChatComposer` 的轻量变体。
- 技术方案：
  - 修改 `src/types/chat.ts` / `src-tauri/src/models/chat.rs::SendChatMessageRequest`：
    - `pinnedPagePath?: string | null`
  - 修改 `src-tauri/src/services/chat_service.rs::build_retrieval_context(context, session, query, search_service, pinned_page_path)`：
    - pinned page 永远作为第一 source；
    - search hits 去重后补充其他页面；
    - citations 中 pinned page 标记 `isPinned` 可选。
  - 修改 `src-tauri/src/commands/chat_commands.rs::send_chat_message`，透传 pinned page。
  - 修改 `src/stores/navigationStore.ts`：
    - `rightPanelMode: "default" | "wikiAssistant"`
    - `wikiAssistantPagePath: string | null`
    - `openWikiAssistant(path: string): void`
  - 修改 `src/features/wiki/WikiView.tsx` toolbar，新增 `MessageSquareText` icon button，label `Ask about this page`。
  - 新增 `src/features/chat/PageChatPanel.tsx`：
    - 显示当前页 title/path；
    - 消息列表使用当前 active session 或自动创建 page session；
    - composer 调 `chatStore.sendMessage(..., { pinnedPagePath })`。
  - 修改 `RightContextPanel.tsx`：当 activeView 为 wiki 且 mode 为 wikiAssistant 时渲染 `PageChatPanel`，否则渲染现有 RelatedPagesPanel。
- 需要新增哪些文件：`src/features/chat/PageChatPanel.tsx`。
- 需要修改哪些文件：`chat.ts`、`models/chat.rs`、`chat_commands.rs`、`chat_service.rs`、`chatStore.ts`、`navigationStore.ts`、`WikiView.tsx`、`RightContextPanel.tsx`、i18n、styles。
- 是否需要新增依赖：不需要。

## 4. UI / 交互设计
- 界面变化描述：Wiki toolbar 新增一个 icon-only AI 问答按钮；右侧面板切换为文章问答模式，顶部显示当前文章名和返回元数据按钮。
- 交互流程：用户打开 Wiki 页面 -> 点击 Ask AI -> 右侧面板打开 PageChatPanel -> 输入追问 -> 后端把当前页作为第一上下文 -> 流式回答与引用显示在侧栏 -> 用户可继续追问或保存答案到 `wiki/queries/`。
- 需要参考的设计规范：`Spec/FRONTEND_GUIDELINES.md` Wiki Browser、Chat 章节；右侧面板应用作 context area，不弹出大 modal。

## 5. 验收标准（Done Definition）
- [ ] 每个 Wiki 页面工具栏都有 Ask AI 按钮，点击后右侧面板切到当前页问答。
- [ ] 提问时后端请求包含 `pinnedPagePath`，回答 citations 至少包含当前页。
- [ ] 用户切换到另一篇页面后，PageChatPanel 更新 pinned page，不把旧页面当当前页。
- [ ] 侧栏问答不破坏全局 Chat 视图的会话、流式状态和保存回答功能。
- [ ] 未配置 Agent/BYOK 时显示与 Chat 一致的配置提示。

## 6. 风险与注意事项
- 可能影响的现有功能：ChatStore 当前可能假设只有 ChatView 调用发送；需要避免两个 composer 同时发送导致 streaming state 串扰。
- 边界情况：当前页很长时 excerpt 要截断；页面被外部修改后 pinned context 应读取最新文件；当前页不存在或被删除时侧栏显示错误。

## 7. 实施步骤
- [ ] 扩展 SendChatMessageRequest DTO 和 serde 默认测试。
- [ ] 修改 ChatService pinned retrieval，并写后端测试。
- [ ] 扩展 chatStore sendMessage 参数。
- [ ] 新增 PageChatPanel。
- [ ] WikiView 接入按钮，RightContextPanel 接入模式切换。
- [ ] 补前端集成测试：点击 Ask AI、发送时带 pinnedPagePath、citation 展示。
