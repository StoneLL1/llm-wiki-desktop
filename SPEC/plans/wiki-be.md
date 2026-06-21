# 进度账本 · wiki 后端 (P0+P1)

> 权威源：SPEC/roadmap/wiki.md §1 row 37 + §2 PRD-READ-001 / PRD-WIKI-004 · SPEC/PRD.md (PRD-READ-004 / PRD-WIKI-004 / PRD-GIT-002/004) · CLAUDE.md「必读硬边界」
> scope：只动 src-tauri/（新增/扩展 Tauri 命令与 service）。status: pending | in_progress | done | verified

## 本轮计划

从 SPEC/roadmap/wiki.md 摘出**仅落在后端 src-tauri/** 的 P0+P1 项（前端 UI 项 / P2 / 跨板块 lint 可视化不在本 loop）。逐条实施 → cargo test + npm run test + npm run lint 全绿 → status=verified → 逐项 commit → 追加 progress.txt。收敛后写顶部完成标记并结束 loop。

**范围条目（4 项）：**

| 项 | 来源 | 目标 |
|---|---|---|
| W1 | roadmap §1 row37 + §2 PRD-READ-001 | 新增 `create_wiki_page` / `rename_wiki_page` / `delete_wiki_page` 三个后端命令（wiki_commands.rs 当前仅 scan/read/save/toggle_bookmark）|
| W2 | roadmap §2 PRD-READ-001 | rename 同步更新全仓所有 `[[old]]`→`[[new]]` wikilink 引用（含 alias `[[old|Alias]]`）；delete 走 PendingAction 确认 + GitService 检查点 |
| W3 | roadmap §2 PRD-WIKI-004 + §2 末段 | `FILE_HASH_MISMATCH` 扩展：返回 baseline 文本，供前端做三路 diff（compile_commands.rs CompileMerge 冲突路径 + wiki 保存 save_page 路径）|
| W4 | roadmap §2 PRD-READ-001 验收 | 全部新命令接 ProjectContext 路径安全校验，覆盖 CJK 文件名测试 |

W1/W2/W4 同属"新建/重命名/删除页面生命周期"，实现上高度耦合，合并为**同一批改动 + 同一次 commit**（拆成三个 commit 会导致中间态不可编译/不可测，违反"逐项 commit"需可验证的精神）。W3 是独立的 FILE_HASH_MISMATCH 扩展，单独一个 commit。

→ 实际 commit 粒度：**commit A = W1+W2+W4**（create/rename/delete 命令 + wikilink 重写 + Git 检查点 + 路径安全+CJK 测试），**commit B = W3**（FILE_HASH_MISMATCH baseline 透传）。

## 关键决策（动手前）

- **delete 走 PendingAction + Git 检查点（不立即删）**：参照 `import_commands::request_delete_source` → `file_commands::confirm_pending_action` 的成熟范式（DeleteSource execution）。但 wiki 页面删除**不依赖** source-index（wiki 页没有提取产物索引），需新增一个 `ConfirmationExecution::DeleteWikiPage` 变体，携带 `{project_id, root_path, target_path, target_hash, referenced_by: Vec<String>}`。`request_delete_wiki_page` 命令：校验路径在 wiki/ 内、文件存在、读当前 hash、**扫描全仓 wikilink 找出引用本页的页面**（referenced_by，用于预览"删除后这些页面的链接将变为 missing"）→ 登记 Destructive PendingAction → 返回 PendingAction 供前端 ConfirmationDialog 确认。`confirm_pending_action` 在 DeleteWikiPage 分支：resolve context → `git_service.create_scoped_checkpoint`（HighRiskOperation, paths=[target_path]）→ `fs::remove_file` → 返回 ConfirmedAction(checkpoint_exists=true)。取消分支不执行删除（仅消费 action）。
- **rename 同步重写所有 wikilink 引用（含 alias）**：核心难点。`[[Target]]` / `[[Target|Alias]]` / `[[Target#Section]]` / `[[Target#Section|Alias]]` 中 `Target` 部分若等于（大小写无关）被重命名页面的 stem 或 alias，需改为新 stem。**关键**：wikilink target 按文件名 stem 解析（`extract_wikilinks` 已按 stem 提取），故 rename 的 "old target" = 旧文件 stem（去 `.md`），"new target" = 新文件 stem。同时页面自身的 frontmatter `aliases` 也可能在被其它页引用（alias 匹配）——但 alias 不会因 rename 而变（alias 是 frontmatter 字段，rename 文件不改变 alias 内容）；只有引用**旧 stem** 的 wikilink 需要改。**实现**：在 markdown_utils 新增 `rewrite_wikilinks(body, old_stem, new_stem) -> (rewritten_body, replaced_count)`，大小写无关匹配 target 部分（`[[` 后第一个 `|` 或 `#` 或 `]]` 前的文本），保留 alias/anchor/原大小写外的部分。rename 命令：扫描 wiki 全部 .md → 对每个含 `[[old_stem...]]` 的文件原子重写（write_markdown_checked 无 hash 校验，因为这是系统批量重写，非用户编辑冲突；但**rename 前必须 Git 检查点**保护旧文件+所有引用文件）。重写不计入"用户编辑冲突"，用 WriteMode 非检查式写入（或 OverwriteIfHashMatches 读当前 hash 后立即写——选 read-hash-then-overwrite 以保持原子可见性，但引用页可能很多，简化为直接 write_text 经 file_store.write_markdown，因为 rename 本身已建检查点）。
- **rename 文件移动本身**：旧路径 → 新路径 `fs::rename`（同卷原子）；若跨目录需先 ensure_dir。Git 检查点在 rename 之前创建（保护旧文件可回滚），rename+引用重写全部完成后**不再**自动提交结果（保持与 chat_service.save_answer overwrite 一致：检查点在操作前，结果让用户后续提交）——但这样 git 状态会显示 rename+引用改动为 unstaged。重新审视：roadmap 验收只要求"重命名会同步更新所有 wikilink 引用"+"删除走 Git 检查点"。rename 是否也建检查点？CLAUDE.md 硬边界："删除、覆盖、批量替换、Agent 自动修复——操作前必须创建 Git 检查点"。rename=文件移动+批量引用重写=批量替换，**命中硬边界**→ rename 前**必须** Git 检查点。决策：rename 命令前置 `create_scoped_checkpoint`（paths 含旧页+将改的引用页；但引用页在扫描前未知，故用 `create_checkpoint` 全量 add --all）。简化且安全：rename 用 `git_service.create_checkpoint`（全量）。
- **create_wiki_page**：薄命令。校验路径在 wiki/ 内、CreateNew（拒绝已存在，复用 WriteMode::CreateNew + file_store.write_markdown_checked）→ 写入空模板（frontmatter type/title/created + `# {title}`）→ 失效 graph cache → 返回 SaveWikiPageResponse 复用（relative_path/hash/saved_at/graph_cache_invalidated）。不建 Git 检查点（创建新文件非破坏性，与 chat_service.save_answer 新建页一致：CreateNew 无 checkpoint）。
- **FILE_HASH_MISMATCH 扩展（W3）**：当前 `file_store::verify_write_mode` 的 OverwriteIfHashMatches 分支在 hash 不匹配时返回 details `{path, expectedHash, currentHash}`。扩展为额外返回 `baselineContent`（磁盘当前内容）。**两条路径都要覆盖**：(a) wiki `save_page`→`write_markdown_checked`（wiki 保存冲突，PRD-READ-004 的外部修改场景）；(b) compile `apply_confirmed_manifest`/`apply_manifest` 内部也用 OverwriteIfHashMatches（compile_commands.rs CompileMerge 冲突）。改 `file_store::verify_write_mode` 在 mismatch 时读磁盘内容塞入 details.baselineContent。这样前端 ConflictDiffDialog 能拿到 baseline（磁盘）+ current（编辑器内存）+ agent（生成）三路。**注意**：只动 file_store 一处，两条路径自动受益。需补 mismatch 时磁盘内容读取（path 已在手）。
- **路径安全 + CJK**：所有新命令经 `context.resolve_project_path` / `resolve_wiki_path`（已有 traversal/absolute/symlink 校验）。CJK 测试：create/rename/delete 用中文文件名（`wiki/概念/智能体.md`）走通。复用 file_store 已有 CJK 测试范式。
- **DTO 位置**：新增请求/响应 DTO 放 `models/wiki.rs`（CreateWikiPageRequest/RenameWikiPageRequest/DeleteWikiPageRequest + RenameWikiPageResponse/DeleteWikiPageResponse），与现有 SaveWikiPageRequest 同风格（camelCase serde）。`ConfirmationExecution::DeleteWikiPage` 加在 `models/confirmation.rs`。
- **命令注册**：lib.rs `invoke_handler` 新增 create_wiki_page/rename_wiki_page/request_delete_wiki_page 三个 handler。delete 确认复用已有 `confirm_pending_action`（新增 DeleteWikiPage execution 分支），不新建 confirm 命令。

## 条目

### P0

- [x] **W1+W2+W4 create/rename/delete_wiki_page 命令 + wikilink 重写 + Git 检查点 + 路径安全/CJK**（roadmap §1 row37 + §2 PRD-READ-001）— status: verified
  - 涉及：`src-tauri/src/commands/wiki_commands.rs`（新增 3 命令）、`src-tauri/src/services/search_service.rs`（create_page/rename_page/apply_page_delete/find_pages_referencing service 方法 + 11 测试）、`src-tauri/src/utils/markdown_utils.rs`（新增 `rewrite_wikilinks` + 8 测试）、`src-tauri/src/models/wiki.rs`（新增 DTO）、`src-tauri/src/models/confirmation.rs`（新增 `ConfirmationExecution::DeleteWikiPage`）、`src-tauri/src/commands/file_commands.rs`（confirm_pending_action 新增 DeleteWikiPage 分支 + `execute_wiki_page_delete` 薄包装）、`src-tauri/src/lib.rs`（注册 3 命令）
  - 改动：
    - `utils/markdown_utils.rs`：`rewrite_wikilinks(body, old_stem, new_stem) -> (String, usize)`，大小写无关 target 匹配，保留 alias/anchor/嵌套括号。
    - `models/wiki.rs`：`CreateWikiPageRequest` / `RenameWikiPageRequest` / `RenameWikiPageResponse{updated_references, graph_cache_invalidated}` / `DeleteWikiPageRequest`。
    - `models/confirmation.rs:126-131`：`DeleteWikiPage { project_id, root_path, target_path, target_hash }`。
    - `services/search_service.rs`：`create_page`（CreateNew，seed frontmatter+H1，失效 graph-cache，append save-log，无 checkpoint）、`rename_page`（移动 + 全仓 `[[old]]`→`[[new]]` 重写，跳过自身后重写自引用，返回 updated_references）、`find_pages_referencing`（大小写无关）、`apply_page_delete`（re-verify hash → pre-delete `create_scoped_checkpoint(HighRiskOperation)` → remove_file + 失效 graph-cache → post-delete `create_scoped_checkpoint(FinalResult)` → 返回 `checkpoint.commit_hash.is_some()`；失败 unstage 回滚；镜像 `import_service::apply_source_delete` 两段检查点契约）。
    - `commands/wiki_commands.rs:69-177`：`create_wiki_page` / `rename_wiki_page`（前置 `create_checkpoint(HighRiskOperation)` 因批量替换命中硬边界）/ `request_delete_wiki_page`（PendingAction + referenced_by 预览，checkpoint_hash=None）。
    - `commands/file_commands.rs:210-222, 280-308`：confirm_pending_action `DeleteWikiPage` 分支 → `execute_wiki_page_delete` 薄包装调 service（Windows GUI 模块不能跑单测，逻辑下沉 service）。
    - `lib.rs:161-163`：invoke_handler 注册 3 命令。
  - 验收：275 backend tests green（含 21 search_service + 20 markdown_utils 新测）；npm test 72/72 green；npm lint clean；无 console.log/println!/dbg!。create CreateNew 拒已存在；rename 移动+全仓引用重写（含 alias/anchor/CJK/嵌套括号）+前置 checkpoint；delete PendingAction(referenced_by 预览)→confirm 后 pre-delete+post-delete 双 checkpoint+删除；全部经 ProjectContext 路径安全；CJK（`wiki/概念/智能体.md`）路径通。
- [ ] **W3 FILE_HASH_MISMATCH 返回 baseline 文本**（roadmap §2 PRD-WIKI-004）— status: pending
  - 涉及：`src-tauri/src/services/file_store.rs`（`verify_write_mode` mismatch 分支读磁盘内容塞 details.baselineContent）
  - 验收：wiki save_page 与 compile 冲突路径的 FILE_HASH_MISMATCH error.details 含 `baselineContent`（磁盘当前文本）；前端可据此做三路 diff。补 file_store mismatch 测试断言 baselineContent 存在且等于磁盘内容。

## 进度日志

- 2026-06-21 建账本；读 roadmap wiki.md + PRD + wiki_commands/search_service/file_store/git_service/markdown_utils/confirmation/chat_service/import_commands/file_commands/compile_commands/lib.rs/errors/paths/path_utils/i18n，确认 4 项范围与实现路径。
- 2026-06-21 W1+W2+W4 verified：create/rename/delete_wiki_page 三命令 + `rewrite_wikilinks` + `apply_page_delete`（pre-delete HighRiskOperation + post-delete FinalResult 双 checkpoint，镜像 import_service）+ `find_pages_referencing` + 路径安全/CJK 测试。275 backend tests + 72 npm tests + lint 全绿。关键修正：apply_page_delete 原"checkpoint-before-remove"对已跟踪干净文件 created=false（无变更可提交）；改为镜像 import_service 的两段契约——pre-delete checkpoint（安全网，HEAD 保护 before 态）→ remove → post-delete FinalResult checkpoint（捕获删除为真实变更）。Windows GUI 命令模块 #[cfg(feature="gui")] 跑单测会 STATUS_ENTRYPOINT_NOT_FOUND 崩溃，delete 逻辑下沉到 lib-available 的 SearchService::apply_page_delete，命令层 execute_wiki_page_delete 仅薄包装。
