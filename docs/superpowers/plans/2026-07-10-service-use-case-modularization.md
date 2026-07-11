# Service Use-Case Modularization Implementation Plan

> Status: completed on 2026-07-10 and integrated into master.
> Current architecture documentation: [backend structure](../../../SPEC/BACKEND_STRUCTURE.md), [tech stack](../../../SPEC/TECH_STACK.md), and [roadmap migration note](../../../SPEC/roadmap/README.md).
> The task body below remains the historical implementation plan.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不改变 Tauri IPC、DTO、持久化格式和安全边界的前提下，把 `ImportService`、`LintService`、`ChatService`、`SearchService` 的单文件实现拆成按 use-case 组织、可独立审查和测试的 Rust 子模块。

**Architecture:** 保留四个现有 service 类型作为稳定 facade，继续由 `AppState` 持有、由 commands 通过原方法名调用；每个 facade 改为目录模块，并在子模块中用多个 `impl Service` 块承载具体用例。迁移按 Import → Search → Lint → Chat 的顺序进行，每完成一个纵向切片就运行定向测试和 Rust 全套测试，避免四个大服务同时失稳。

**Tech Stack:** Rust 2021、Tauri v2、typed DTO、Markdown/JSON/local files、`ProjectContext`、`FileStore`、`GitService`、`WikiIndex`、Cargo tests。

## Global Constraints

- 项目内容仍只使用 Markdown、JSON 和本地文件；不得引入数据库。
- `raw/sources/` 默认不可变；删除、替换和覆盖必须保留现有确认与 Git checkpoint 语义。
- 所有项目路径继续经过 `ProjectContext`；不得把绝对写路径或路径拼接下放到 command 层。
- API key、token 和 provider secret 不进入项目文件、日志或错误详情。
- Search 只做本地关键词/过滤/摘录；不得在拆分过程中引入模型调用。
- Chat 会话路径、引用筛选规则、页面作用域约束和 `wiki/queries/` 保存格式保持兼容。
- 不改变 `AppState` 的 `import_service`、`lint_service`、`chat_service`、`search_service` 字段类型与默认构造方式。
- 不改变现有 Tauri command 名、请求/响应 DTO、错误码和 JSON camelCase 契约。
- 不新增 crate；本任务是结构重构，不是产品能力扩展。
- Windows 后端单测使用 `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`；GUI 命令层另用 `cargo check --manifest-path src-tauri/Cargo.toml` 验证。
- CJK/Unicode、Windows 大小写、外部编辑、hash 漂移、bookmark live join 等现有回归测试必须继续通过。

---

## 1. 架构审阅结论

### 1.1 建议是否靠谱

结论：**方向正确，优先级合理，但不能按“新增更多 service 对象”的方式直接拆。**

审阅报告指出的事实成立：

| 文件 | 当前行数 | 已混合的职责 |
| --- | ---: | --- |
| `import_service.rs` | 2275 | 分类、source catalog、preview、confirm、extraction promotion、delete/replace、回滚 |
| `lint_service.rs` | 2832 | 本地规则、deep prompt/parser、ignore、report/history、单项/批量修复 |
| `chat_service.rs` | 2195 | session、retrieval、prompt、citations、保存回答 |
| `search_service.rs` | 2313 | index scan/read、页面 CRUD、引用重写、query scoring、excerpt retrieval |

这些文件已经超过“单次审查能稳定装入上下文”的规模，且测试与实现交错，任何局部修改都容易触发大范围冲突。拆分能提高所有权清晰度、测试定位和后续演进速度。

### 1.2 不采纳的两种拆法

1. **不做四套新的 AppState service。** 把 `ImportPreviewService`、`ImportConfirmService` 等全部注入 `AppState` 会扩大依赖图，并迫使 commands 同步重写；它解决了文件行数，却制造了运行时装配复杂度。
2. **不做一次性大爆炸迁移。** 四个文件合计约 9600 行，同时移动会让 `git diff` 难以审阅，也会掩盖行为变化。

### 1.3 采用的方案

采用 **稳定 facade + 目录内 use-case modules + 逐服务迁移**：

```text
commands / AppState
        │ existing public methods
        ▼
ImportService / LintService / ChatService / SearchService
        │ multiple impl blocks, pub(super) helpers
        ▼
use-case modules
        │
        ├── FileStore / ProjectContext / GitService
        ├── WikiIndex / SearchService
        └── typed models
```

这样可同时满足三点：外部契约不变、内部边界清楚、每一步可独立回滚。

### 1.4 范围修正

- `chat_convenience_service.rs` 已经是独立安全边界，负责 Agent convenience write 的意图分类、审计与回滚；本计划**不把它重新塞进 `chat_service/`**。`chat_service` 中的 `save_answer_to_wiki` 拆到 `saved_answers.rs`，而真正的 convenience edit audit 继续保持独立。
- `wiki_index.rs` 已经是缓存实现；`search_service/` 的 `catalog.rs` 只负责调用它并组装 `WikiTree`/page metadata，不复制第二套 index。
- 页面 CRUD 当前属于 `SearchService` 的公开契约。虽然命名不理想，本轮不重命名为 `WikiService`，只拆到 `pages.rs`，避免扩大 command/DTO 迁移范围。

## 2. 目标文件结构

```text
src-tauri/src/services/
├── import_service/
│   ├── mod.rs                 # ImportService facade、公开 re-export、共享类型
│   ├── classification.rs      # classify_file、archive routing、deterministic rename
│   ├── source_catalog.rs      # validate/list/hash/index/record/staging
│   ├── promotion.rs           # raw/extracted → wiki/sources promotion 与路径 remap
│   ├── preview.rs             # collect/scan/deduplicate/preview summary
│   ├── confirmation.rs        # confirm、preflight、copy、rollback
│   ├── source_actions.rs      # delete/replace/cleanup、backup/restore
│   └── test_support.rs        # cfg(test) 临时项目与 fixture helpers
├── lint_service/
│   ├── mod.rs                 # LintService facade、共享常量
│   ├── rules.rs               # 本地确定性规则与解析 lookup
│   ├── deep.rs                # deep prompt、Agent JSON parser/normalizer
│   ├── ignores.rs             # lint-ignore round trip
│   ├── reports.rs             # persist/read/list/history/limit
│   ├── fixes.rs               # 单项/批量修复、pending action、checkpoint
│   └── test_support.rs
├── chat_service/
│   ├── mod.rs                 # ChatService、RetrievalContext 等稳定 re-export
│   ├── sessions.rs            # create/list/load/rename/delete/append/save
│   ├── retrieval.rs           # search/pinned/graph/source-overlap/prompt budgets
│   ├── citations.rs           # [S#] parser 与 answer Markdown source filtering
│   ├── saved_answers.rs       # wiki/queries create/overwrite/hash/checkpoint/log
│   └── test_support.rs
├── search_service/
│   ├── mod.rs                 # SearchService，持有 FileStore + WikiIndex
│   ├── catalog.rs             # scan/read/meta/tree/live bookmark overlay
│   ├── pages.rs               # save/create/rename/references/delete/cache invalidation
│   ├── query.rs               # query tokenize/filter/score/sort
│   ├── excerpts.rs            # retrieve_with_excerpts、bounded snippets
│   └── test_support.rs
├── chat_convenience_service.rs # 保持独立
├── wiki_index.rs               # 保持独立、由 search catalog/query/excerpts 共享
└── mod.rs                      # 外部 re-export 路径保持不变
```

## 3. 模块依赖规则

- Rust 的 `services/import_service.rs` 与 `services/import_service/mod.rs` 不能同时存在；Search/Lint/Chat 同理。每个 service 的“文件 → 目录模块”替换必须在同一个工作单元中完成：先准备完整 patch，再同时删除旧文件、添加目录文件，之后才运行编译。不要在中间态执行 Cargo。
- `mod.rs` 只定义 facade、字段、共享常量和 re-export；不得重新积累业务逻辑。
- 子模块使用 `impl super::ImportService` 等方式实现原公开方法，commands 不需要知道子模块存在。
- 跨子模块 helper 默认 `pub(super)`；只有当前已被其他 service 使用的 `classify_file`、`RetrievalContext` 等继续公开。
- `classification`、`citations`、query tokenization 等纯函数不得依赖 Git、Tauri 或全局状态。
- `confirmation`、`source_actions`、`fixes`、`saved_answers` 是写入边界，必须显式依赖 `ProjectContext`、`FileStore` 和 `GitService`，并保留 preflight → checkpoint → write → rollback/commit 顺序。
- `SearchService` 是 `LintService` 与 `ChatService` 的只读依赖；禁止形成 Search → Chat/Lint 的反向依赖。
- `chat_convenience_service` 可以由 command orchestration 使用，但不能成为 `ChatService` session/retrieval 的隐式依赖。

## 4. 稳定接口清单

实施期间以下调用形态必须继续编译：

```rust
let import = ImportService;
let search = SearchService::default();
let lint = LintService::default();
let chat = ChatService::default();

let preview = import.preview_import(&context, &file_store, &request, &extract_results)?;
import.confirm_import(&context, &file_store, &preview)?;
let tree = search.scan_wiki(&context, &bookmark_paths)?;
let results = search.search(&context, &request)?;
let hits = search.retrieve_with_excerpts(&context, query, limit, excerpt_chars)?;
let report = lint.run_local_lint(&context, &search)?;
let retrieval = chat.build_retrieval_context(
    &context,
    &search,
    question,
    &session,
    language,
    route,
    context_window,
    pinned_page_path,
)?;
```

`services/mod.rs` 的公开路径保持：

```rust
pub use chat_service::{ChatService, RetrievalContext};
pub use import_service::{classify_file, ImportService};
pub use lint_service::LintService;
pub use search_service::SearchService;
```

---

### Task 1: 固定 facade 与跨服务契约

**Files:**
- Create: `src-tauri/tests/service_facade_contracts.rs`
- Read: `src-tauri/tests/mvp_flow.rs`
- Read: `src-tauri/tests/sources_promotion.rs`
- Read: `src-tauri/src/app_state.rs`

**Interfaces:**
- Consumes: 当前 `services/mod.rs` re-export 和四个 service 的 `Default`/unit construction。
- Produces: 一个只依赖公开 API 的编译与最小行为保护层，后续每次移动都运行。

- [ ] **Step 1: 写 facade construction 与纯函数契约测试**

```rust
use std::path::Path;

use llm_wiki_desktop_lib::models::import::SourceFileType;
use llm_wiki_desktop_lib::services::{
    classify_file, ChatService, ImportService, LintService, SearchService,
};

#[test]
fn service_facades_keep_their_public_construction_contract() {
    let _import = ImportService;
    let _lint = LintService::default();
    let _chat = ChatService::default();
    let _search = SearchService::default();
}

#[test]
fn import_classification_remains_reexported() {
    assert_eq!(classify_file(Path::new("研究报告.PDF")), SourceFileType::Pdf);
}
```

- [ ] **Step 2: 增加 citation 静态 API 契约**

```rust
use llm_wiki_desktop_lib::models::chat::ChatSourceRef;

#[test]
fn chat_citation_parser_remains_on_the_facade() {
    let sources = vec![ChatSourceRef {
        id: "S1".into(),
        page_path: "wiki/a.md".into(),
        title: "A".into(),
        excerpt: Some("alpha".into()),
        score: 10,
        is_pinned: false,
    }];
    let parsed = ChatService::parse_model_citations("Answer [S1]", &sources);
    assert_eq!(parsed.citations.len(), 1);
}
```

- [ ] **Step 3: 运行新契约测试，确认基线通过**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test service_facade_contracts`

Expected: PASS；若 `ChatSourceRef` 当前字段与示例不同，按 `models/chat.rs` 的实际必填字段补齐，但不得为测试修改 DTO。

- [ ] **Step 4: 运行现有跨服务基线**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test mvp_flow --test sources_promotion`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tests/service_facade_contracts.rs
git commit -m "test: lock service facade contracts"
```

### Task 2: 拆 Import 分类、catalog 与 promotion

**Files:**
- Create: `src-tauri/src/services/import_service/mod.rs`
- Create: `src-tauri/src/services/import_service/classification.rs`
- Create: `src-tauri/src/services/import_service/source_catalog.rs`
- Create: `src-tauri/src/services/import_service/promotion.rs`
- Create: `src-tauri/src/services/import_service/test_support.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Delete after migration: `src-tauri/src/services/import_service.rs`

**Interfaces:**
- Consumes: `BackendError`, import models, `ProjectContext`, `FileStore`, `GitService`。
- Produces: 原 `ImportService` 类型、`classify_file` re-export，以及 `validate_imported_source_path`、`list_imported_sources`、`hash_external_file`、`read_source_index`、`record_confirmed_sources`、`collect_source_paths`、`stage_text_source` 原方法。

- [ ] **Step 1: 创建最小 facade 和模块声明**

```rust
mod classification;
mod confirmation;
mod preview;
mod promotion;
mod source_actions;
mod source_catalog;

#[cfg(test)]
mod test_support;

pub use classification::classify_file;

#[derive(Default)]
pub struct ImportService;

pub(super) type FileBackup = Vec<(std::path::PathBuf, Option<Vec<u8>>)>;
```

在本 task 中先声明后续模块时，必须同时创建可编译的空文件；不要让一次 commit 留下 module-not-found。

- [ ] **Step 2: 移动纯分类函数并保持可见性**

`classification.rs` 包含：`classify_file`、`target_archive_dir`、`deterministic_rename`。其中只有 `classify_file` 保持 `pub`，其余按当前外部使用情况设为 `pub(super)`。

迁移后保留以下精确签名，并把旧函数体逐行移动到对应函数；本步骤不修改 extension mapping、archive 目录或 hash 后缀算法：

```rust
pub fn classify_file(path: &Path) -> SourceFileType;
pub(super) fn target_archive_dir(file_type: &SourceFileType) -> &'static str;
pub(super) fn deterministic_rename(original_name: &str, hash: &str) -> String;
```

- [ ] **Step 3: 移动 source catalog 用例**

在 `source_catalog.rs` 中增加 `impl ImportService`，迁移以下方法且不改签名：

`source_catalog.rs` 中用 `impl super::ImportService` 保留以下精确签名；函数体逐行迁移，不增加新 DTO 或 facade forwarding：

```rust
pub fn validate_imported_source_path(
    &self,
    context: &ProjectContext,
    relative_path: &str,
) -> Result<PathBuf, BackendError>;

pub fn list_imported_sources(
    &self,
    context: &ProjectContext,
) -> Result<Vec<ImportedSource>, BackendError>;

pub fn hash_external_file(&self, path: &Path) -> Result<String, BackendError>;

pub fn read_source_index(
    &self,
    context: &ProjectContext,
    file_store: &FileStore,
) -> Result<SourceArtifactIndex, BackendError>;

pub fn record_confirmed_sources(
    &self,
    context: &ProjectContext,
    file_store: &FileStore,
    preview: &ImportPreview,
) -> Result<(), BackendError>;

pub fn collect_source_paths(
    &self,
    source_paths: &[String],
) -> Result<Vec<PathBuf>, BackendError>;

pub fn stage_text_source(
    &self,
    context: &ProjectContext,
    file_store: &FileStore,
    source_name: &str,
    extension: &str,
    content: &str,
) -> Result<PathBuf, BackendError>;
```

- [ ] **Step 4: 移动 extraction promotion 与路径 remap**

`promotion.rs` 承载 `promote_extracted_to_sources`、`build_source_page`、title/filename/sanitize/collision/YAML/frontmatter/remap helpers。入口保持 `pub(super)`，只允许 confirmation/source actions 调用。

- [ ] **Step 5: 将相关单测移动到就近模块**

分类/CJK/route 测试进入 `classification.rs`；source index/staging/promotion 测试进入对应文件。通用临时目录 helper 放入 `test_support.rs`：

`test_support.rs` 暴露且只在测试构建中编译以下 helper；函数体直接迁移原 `tmp_context` 的 temp path 创建、目录初始化和 `ProjectContext` construction：

```rust
#[cfg(test)]
pub(super) fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf);
```

- [ ] **Step 6: 运行 Import 定向测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_service`

Expected: 原 Import 单测全部 PASS，测试数量与迁移前一致。

- [ ] **Step 7: 运行 extraction 与 promotion 集成测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test sources_promotion`

Expected: PASS；`ExtractionService` 继续通过 `super::import_service::classify_file` 编译。

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/services/import_service src-tauri/src/services/import_service.rs src-tauri/src/services/mod.rs
git commit -m "refactor: split import catalog and promotion modules"
```

### Task 3: 拆 Import preview、confirmation 与 source actions

**Files:**
- Modify: `src-tauri/src/services/import_service/preview.rs`
- Modify: `src-tauri/src/services/import_service/confirmation.rs`
- Modify: `src-tauri/src/services/import_service/source_actions.rs`
- Modify: `src-tauri/src/services/import_service/mod.rs`
- Test: `src-tauri/src/services/import_service/preview.rs`
- Test: `src-tauri/src/services/import_service/confirmation.rs`
- Test: `src-tauri/src/services/import_service/source_actions.rs`

**Interfaces:**
- Consumes: Task 2 的 classification/catalog/promotion helpers。
- Produces: `preview_import`、`confirm_import`、`cleanup_replacement_artifacts`、`apply_source_delete`、`apply_source_replace` 完整行为。

- [ ] **Step 1: 迁移 preview 的只读 preflight**

将 `collect_source_files`、`preview_import`、`scan_existing`、`collect_hashes`、fast hash helper 放到 `preview.rs`。preview 只能读外部源和项目索引，不能写 archive。

- [ ] **Step 2: 迁移 confirmation 的 preflight + rollback 写入流程**

`confirmation.rs` 保持以下顺序：

```text
validate every entry and source hash
→ compute all targets
→ copy/promote/write index
→ rollback all touched targets on any error
→ remove staging files only after success
→ return Result<(), BackendError>
```

将 `validate_confirm_entry`、`verify_project_hash`、artifact validation、rollback helpers 与 `confirm_import` 放在同一模块，避免写入顺序跨文件跳转。`confirm_import_preview` command 当前在 service 成功后写 `.app/import-conflicts.json`，并按 `request.create_checkpoint` 创建导入结果 checkpoint；本轮保持这个 command 顺序与 DTO 不变，不把 checkpoint 参数虚构进 `ImportService::confirm_import`。

- [ ] **Step 3: 迁移 source delete/replace**

`source_actions.rs` 承载 `cleanup_replacement_artifacts`、`apply_source_delete`、`apply_source_replace`、backup/restore/remove helpers。保留“确认后再次校验 hash → checkpoint → mutation → commit/restore”的现有顺序。

- [ ] **Step 4: 写模块边界回归测试**

从原测试中确保至少保留并定位以下 case：

```text
preview rejects missing source
preview detects exact duplicate and name collision
confirm rejects tampered archive path
confirm rejects source changes after preview
confirm does not partially archive when a later source is stale
delete removes indexed artifacts under checkpoint
replace updates archive, artifacts, index and commit
replacement cleanup preserves shared artifacts
```

- [ ] **Step 5: 运行 Import 全模块测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::import_service`

Expected: PASS。

- [ ] **Step 6: 运行 Import command GUI compile**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: `import_commands.rs` 与 `file_commands.rs` 无需改调用方法即可编译。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/import_service
git commit -m "refactor: split import preview confirm and source actions"
```

### Task 4: 拆 Search catalog 与 page mutations

**Files:**
- Create: `src-tauri/src/services/search_service/mod.rs`
- Create: `src-tauri/src/services/search_service/catalog.rs`
- Create: `src-tauri/src/services/search_service/pages.rs`
- Create: `src-tauri/src/services/search_service/query.rs`
- Create: `src-tauri/src/services/search_service/excerpts.rs`
- Create: `src-tauri/src/services/search_service/test_support.rs`
- Delete after migration: `src-tauri/src/services/search_service.rs`
- Modify: `src-tauri/src/services/mod.rs`

**Interfaces:**
- Consumes: `FileStore`、`WikiIndex`、wiki/search models、markdown utils。
- Produces: 原 `SearchService::default()`、scan/read/page mutation API，且同一实例仍共享同一个 `WikiIndex`。

- [ ] **Step 1: 创建持有共享 index 的 facade**

```rust
mod catalog;
mod excerpts;
mod pages;
mod query;

#[cfg(test)]
mod test_support;

use crate::services::file_store::FileStore;
use crate::services::wiki_index::WikiIndex;

#[derive(Default)]
pub struct SearchService {
    pub(super) file_store: FileStore,
    pub(super) index: WikiIndex,
}
```

- [ ] **Step 2: 迁移 catalog**

移动 `scan_wiki`、`read_page`、`build_meta`、`build_tree`、`insert_node`、`compute_file_counts`、mtime/error helpers。`scan_wiki` 仍必须在每次返回前 overlay live bookmark paths，不能把 bookmark 写入 index entry。

- [ ] **Step 3: 迁移 page mutations**

移动 `save_page`、`create_page`、`rename_page`、`find_pages_referencing`、`apply_page_delete`、graph cache invalidation 和 save log。所有写入继续保留 optimistic hash、checkpoint 与 reference rewrite 回滚。

- [ ] **Step 4: 迁移 catalog/page tests**

必须保留：外部 mtime/size edit、外部 delete、CJK filename、bookmark toggle、create/rename/delete 越界、rename rollback、case-insensitive reference 等测试。

- [ ] **Step 5: 运行 Search catalog/page 测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::search_service`

Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/search_service src-tauri/src/services/search_service.rs src-tauri/src/services/mod.rs
git commit -m "refactor: split search catalog and page mutations"
```

### Task 5: 拆 Search query 与 excerpts

**Files:**
- Modify: `src-tauri/src/services/search_service/query.rs`
- Modify: `src-tauri/src/services/search_service/excerpts.rs`
- Test: `src-tauri/src/services/search_service/query.rs`
- Test: `src-tauri/src/services/search_service/excerpts.rs`

**Interfaces:**
- Consumes: Task 4 的共享 `SearchService.index` 与 bookmark-neutral entries。
- Produces: `search` 与 `retrieve_with_excerpts` 原签名、排序和缓存复用行为。

- [ ] **Step 1: 迁移 query tokenizer/scorer**

`query.rs` 包含 `search`、Unicode normalize、CJK/ASCII term extraction、field scoring、first match、sort/tie-break。不得把 query helper 放入通用 utils；这些规则属于 Search 领域。

- [ ] **Step 2: 迁移 excerpts**

`excerpts.rs` 包含 `retrieve_with_excerpts`、first body excerpt、truncate helper。它直接使用 index snapshot 中的 cached body，不能重新调用 `FileStore::read_markdown`。

- [ ] **Step 3: 保留中文问句与排序回归测试**

测试必须覆盖：中文问号/后缀拆词、Unicode lowercase、title/alias 优先于 body、unmatched 返回空、excerpt 长度上限。

- [ ] **Step 4: 保留单 snapshot/cache read 断言**

确保 `scan_search_and_retrieve_share_one_index_snapshot` 与 `retrieve_with_excerpts_reuses_cached_body_and_does_not_reread` 继续通过。

- [ ] **Step 5: 运行 Search 全模块与依赖方测试**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::search_service
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::chat_service
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::lint_service
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::graph_service
```

Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/search_service
git commit -m "refactor: split search query and excerpts"
```

### Task 6: 拆 Lint rules、ignores 与 reports

**Files:**
- Create: `src-tauri/src/services/lint_service/mod.rs`
- Create: `src-tauri/src/services/lint_service/rules.rs`
- Create: `src-tauri/src/services/lint_service/deep.rs`
- Create: `src-tauri/src/services/lint_service/ignores.rs`
- Create: `src-tauri/src/services/lint_service/reports.rs`
- Create: `src-tauri/src/services/lint_service/fixes.rs`
- Create: `src-tauri/src/services/lint_service/test_support.rs`
- Delete after migration: `src-tauri/src/services/lint_service.rs`
- Modify: `src-tauri/src/services/mod.rs`

**Interfaces:**
- Consumes: `SearchService` catalog、`FileStore`、lint models。
- Produces: `run_local_lint`、ignore CRUD、report/history API；`LINT_REPORTS_DIR` 保持 `pub(crate)`。

- [ ] **Step 1: 创建 facade 与共享常量**

```rust
mod deep;
mod fixes;
mod ignores;
mod reports;
mod rules;

#[cfg(test)]
mod test_support;

use crate::services::file_store::FileStore;

pub(crate) const LINT_REPORTS_DIR: &str = ".app/lint-reports";

#[derive(Default)]
pub struct LintService {
    pub(super) file_store: FileStore,
}
```

- [ ] **Step 2: 迁移 deterministic rules**

`rules.rs` 包含 `run_local_lint`、dead link/orphan/frontmatter/index drift/resource/schema/source 检查、target lookup、resolution keys、inbound counts 与 rule-specific helpers。

- [ ] **Step 3: 迁移 ignore persistence**

`ignores.rs` 包含 load/save/add/remove/list。继续容忍损坏的 ignore JSON 并拒绝 traversal path。

- [ ] **Step 4: 迁移 report/history**

`reports.rs` 包含 local/deep persist、history index、report read、50 条限制、report id validation、severity counts。单个损坏 report 必须返回 report error，不能让 history list 崩溃。

- [ ] **Step 5: 迁移并运行对应测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::lint_service`

Expected: PASS。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/lint_service src-tauri/src/services/lint_service.rs src-tauri/src/services/mod.rs
git commit -m "refactor: split lint rules ignores and reports"
```

### Task 7: 拆 Lint deep parsing 与 fixes

**Files:**
- Modify: `src-tauri/src/services/lint_service/deep.rs`
- Modify: `src-tauri/src/services/lint_service/fixes.rs`
- Test: `src-tauri/src/services/lint_service/deep.rs`
- Test: `src-tauri/src/services/lint_service/fixes.rs`

**Interfaces:**
- Consumes: rules 的 lookup/path helpers、reports persistence、`GitService`、`SearchService`。
- Produces: deep prompt/parser、`apply_fix`、`apply_fixes_batch` 原 API。

- [ ] **Step 1: 迁移 deep prompt/parser/normalizer**

`deep.rs` 包含 `build_deep_lint_prompt`、`parse_agent_issues`、`parse_agent_issues_for_known_paths`、normalize、JSON block extraction。保持未知路径拒绝、无证据 error 降级等规则。

- [ ] **Step 2: 迁移单项 fix**

`fixes.rs` 包含 missing frontmatter、dead link、index drift 三类修复，保留：hash precondition、PendingAction、checkpoint、graph invalidation、fix log。

- [ ] **Step 3: 迁移 batch fix**

批量修复继续只创建一个共享 checkpoint；high-risk 返回 confirmation，non-fixable/missing hash 进入 skip，不允许部分越界写。

- [ ] **Step 4: 运行 Lint 全模块测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::lint_service`

Expected: PASS，且原测试数量不减少。

- [ ] **Step 5: 编译 lint commands**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: `lint_commands.rs` 无需改公开方法调用即可编译。

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/services/lint_service
git commit -m "refactor: split lint deep analysis and fixes"
```

### Task 8: 拆 Chat sessions 与 citations

**Files:**
- Create: `src-tauri/src/services/chat_service/mod.rs`
- Create: `src-tauri/src/services/chat_service/sessions.rs`
- Create: `src-tauri/src/services/chat_service/retrieval.rs`
- Create: `src-tauri/src/services/chat_service/citations.rs`
- Create: `src-tauri/src/services/chat_service/saved_answers.rs`
- Create: `src-tauri/src/services/chat_service/test_support.rs`
- Delete after migration: `src-tauri/src/services/chat_service.rs`
- Modify: `src-tauri/src/services/mod.rs`

**Interfaces:**
- Consumes: chat models、`FileStore`、time/path validation。
- Produces: session CRUD、`parse_model_citations`、`build_answer_markdown`，外部 re-export 保持不变。

- [ ] **Step 1: 创建 Chat facade 与共享 DTO**

```rust
mod citations;
mod retrieval;
mod saved_answers;
mod sessions;

#[cfg(test)]
mod test_support;

use crate::services::file_store::FileStore;

#[derive(Default)]
pub struct ChatService {
    pub(super) file_store: FileStore,
}

pub struct RetrievalContext {
    pub prompt: String,
    pub source_refs: Vec<ChatSourceRef>,
    pub diagnostics: ChatRetrievalDiagnostics,
}
```

字段按旧定义逐字迁移；`RetrievalContext` 继续由 `services/mod.rs` re-export。

- [ ] **Step 2: 迁移 session persistence**

`sessions.rs` 承载 create/list/load/rename/delete/append/save、session path、context page validation。保持损坏文件 list 时跳过、load 时可恢复错误、空 title 拒绝、page path 跨平台 normalization。

- [ ] **Step 3: 迁移 citation parser**

`citations.rs` 承载 `[S#]` 单个/多个/去重/非法 marker 解析和 `build_answer_markdown`。持久化 citations 只能来自模型实际 marker，不能退回 retrieval top-N。

- [ ] **Step 4: 运行 session/citation 测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::chat_service`

Expected: PASS。

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/chat_service src-tauri/src/services/chat_service.rs src-tauri/src/services/mod.rs
git commit -m "refactor: split chat sessions and citations"
```

### Task 9: 拆 Chat retrieval 与 saved answers

**Files:**
- Modify: `src-tauri/src/services/chat_service/retrieval.rs`
- Modify: `src-tauri/src/services/chat_service/saved_answers.rs`
- Test: `src-tauri/src/services/chat_service/retrieval.rs`
- Test: `src-tauri/src/services/chat_service/saved_answers.rs`
- Read-only boundary: `src-tauri/src/services/chat_convenience_service.rs`

**Interfaces:**
- Consumes: `SearchService::retrieve_with_excerpts`、`GraphService`、session/citation helpers、`GitService`。
- Produces: standard/convenience retrieval context、prompt assembly、graph/source overlap diagnostics、save answer API。

- [ ] **Step 1: 迁移 retrieval planner**

`retrieval.rs` 包含 standard/convenience mode、pinned hit、budgets、source candidates、graph expansion、source overlap、BYOK/Agent prompts、history/source append helpers。

- [ ] **Step 2: 固定检索不变量**

以下顺序和限制必须在代码注释与测试中明确：

```text
required purpose/context
→ pinned page first and full body when required
→ keyword hits
→ bounded graph neighbors
→ bounded source-overlap pages
→ prompt/history budgets
→ diagnostics include all retrieval reasons
```

- [ ] **Step 3: 迁移 saved answers**

`saved_answers.rs` 承载 `save_answer_to_wiki`、query path validation、slug/title/YAML、hash overwrite、graph invalidation 与 log。它不是 `chat_convenience_service` 的 Agent write audit；不要合并两者。

- [ ] **Step 4: 运行 Chat 全模块测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features services::chat_service`

Expected: PASS。

- [ ] **Step 5: 运行 Chat/Lint/Search 跨服务测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test mvp_flow`

Expected: PASS。

- [ ] **Step 6: 编译 chat commands**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: `chat_commands.rs` 中的 `ChatService::parse_model_citations`、session/retrieval/save 调用不改签名即可编译。

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/chat_service
git commit -m "refactor: split chat retrieval and saved answers"
```

### Task 10: 跨模块清理、文档与完整验证

**Files:**
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `SPEC/progress.txt`
- Modify only if a subtle issue was discovered: `SPEC/gotchas.txt`
- Test: `src-tauri/tests/service_facade_contracts.rs`

**Interfaces:**
- Consumes: Tasks 1-9 的稳定 facade。
- Produces: 无孤儿旧文件、无公开 API 漂移、统一验证通过的最终状态。

- [ ] **Step 1: 检查旧单文件已删除且 re-export 唯一**

Run:

```powershell
Get-ChildItem src-tauri/src/services -File |
  Where-Object Name -in 'import_service.rs','lint_service.rs','chat_service.rs','search_service.rs'
Get-Content src-tauri/src/services/mod.rs |
  Select-String 'ImportService|LintService|ChatService|SearchService'
```

Expected: 第一条无输出；第二条每个 facade 只有预期 module/re-export。

- [ ] **Step 2: 检查 commands 与 AppState 未被迫扩张**

Run:

```powershell
git diff -- src-tauri/src/app_state.rs src-tauri/src/commands
```

Expected: 无变更；若仅 rustfmt 产生无语义变化，也应回退该噪声。

- [ ] **Step 3: 运行 Rust 格式检查**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Expected: PASS。

- [ ] **Step 4: 运行后端全套测试**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`

Expected: PASS，测试数量不得少于重构前基线。

- [ ] **Step 5: 运行 GUI command compile**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS，无 warning。

- [ ] **Step 6: 运行仓库统一检查**

Run: `npm run check`

Expected: test、lint、build、console scan、Tauri GUI Rust compile、Rust no-default-features tests 全部 PASS。

- [ ] **Step 7: 进行两轮代码审查**

审查 A（共享上下文）检查：use-case 边界、安全顺序、与 SPEC/APP_flow 一致性。审查 B（fresh context）检查：遗漏 public API、循环依赖、测试数减少、错误码/持久化格式漂移。修复所有有效问题后从 `npm run check` 起完整重跑。

- [ ] **Step 8: 记录进度**

在 `SPEC/progress.txt` 顶部插入：

```text
[2026-07-10] Service use-case modularization — Split Import, Search, Lint, and Chat internals into facade-preserving Rust modules with unchanged IPC/DTO/persistence contracts — Key decision: keep AppState and command call sites stable; chat convenience audit and WikiIndex remain separate security/cache boundaries.
```

只有出现新的、可复现且容易再次踩到的问题时才向 `SPEC/gotchas.txt` 新增一条，不为普通编译错误制造 gotcha。

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/services src-tauri/tests/service_facade_contracts.rs SPEC/progress.txt SPEC/gotchas.txt
git commit -m "refactor: modularize backend service use cases"
```

## 5. 验收标准

- 四个 facade 的构造、公开方法、re-export、AppState 字段和 Tauri command 调用保持兼容。
- 四个原 2000+ 行 service 单文件被目录模块替代；任一 use-case 可在不阅读整个领域实现的情况下理解。
- Import preview 无写入；confirm/source action 保持 preflight、checkpoint、原子写/回滚语义。
- Search 继续共享同一 `WikiIndex` snapshot；bookmark 为 live overlay；excerpt 不重复读盘；不调用 LLM。
- Lint 本地规则、deep parser、report/history、ignore 与 fixes 可分别测试；batch fix 仍只有一个 checkpoint。
- Chat session、retrieval、citations、saved answer 各自成模块；模型未引用的 source 不进入持久化 citations。
- `chat_convenience_service.rs` 与 `wiki_index.rs` 仍作为独立边界存在。
- CJK、Unicode、Windows case、path traversal、external edit/hash、corrupt JSON、checkpoint failure 的现有测试全部保留。
- `npm run check` 最终通过；若任何阶段失败，修复后从完整 `npm run check` 开头重跑。

## 6. 回滚策略

- 每个 task 单独 commit；任何服务拆分失败时只回滚该服务的 commit，不影响已完成服务。
- 不使用 `git reset --hard` 或覆盖用户未提交改动；回滚通过普通反向 commit 或精确恢复本任务文件完成。
- 如果 facade contract 测试暴露真实 API 漂移，优先恢复原签名，而不是同步修改所有 commands 来“让新结构通过”。
- 如果模块拆分导致行为测试减少，停止继续下一个服务，先恢复缺失测试与原行为基线。
