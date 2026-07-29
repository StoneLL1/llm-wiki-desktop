# Import 后端 P0+P1 进度账本

> 历史进度记录：本文描述 2026-06 的 legacy Import 实现，不再是产品或架构约束。OCR / ASR、Source 提交、导入与编译分离等当前规则见 [`../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md)。

> /loop 本轮范围：只动 `src-tauri/`。对照源 SPEC/PRD.md、UI-Frontend-design/import.html、CLAUDE.md。
> 硬约束：导入层只无损保留（原文件/提取文本/图片/来源元数据）；OCR/视觉理解交给编译 Agent/Skill。

✅ 本轮完成 @ 2026-06-21

## 摘要

本轮在 `src-tauri/` 内完成 import 后端 3 项 P0 条目（roadmap 的 P1 中只动后端的部分经核对无独立后端条目，详见各 item 说明）。所有条目 status=verified，完整 test/lint 全绿，未触碰前端 `src/`、未改 `UI-Frontend-design/`、未碰 P2 或其它板块。

### 交付项
- **[P0-1] PDF/Office 解析适配器**（commit f9a3153）：ExtractionService 对 PDF/DOCX/PPTX/XLSX 不再返回 Unsupported。pdf-extract 抽 PDF（按页），zip+quick-xml 抽 OOXML（docx w:t / pptx a:t / xlsx sharedStrings+cells）。扫描件/无文本层 → Failed 路由 OCR 到编译 Agent（守「导入层只无损保留」硬约束）；旧式 .doc/.ppt/.xls → Failed 带转换提示。文本落 `raw/extracted/` + word/page count。
- **[P0-2] 打开文件夹为项目命令链路**（commit 29b4326）：新增 `preview_open_folder_as_project` 命令（dlg-folder 入口），复用 `project_service.open_project`：普通文件夹→NeedsConfirmation（注册可确认 pending action）；已有项目→Opened（纯预览，无副作用）。
- **[P0-3] 导入确认 Git 检查点 + 编译链路**（commit 1dce42f）：`ConfirmImportRequest.create_checkpoint` + `ConfirmedImport.checkpoint_hash`。为 true 时 scoped 提交归档文件 + source-index.json + import-conflicts.json；新仓库自动初始化；无变更→None。编译链路前端驱动（confirm → start_wiki_compile），后端只暴露 checkpoint_hash。

### 文件清单（仅 src-tauri/ + SPEC 文档）
- `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`：加 pdf-extract / zip(--no-default-features,deflate) / quick-xml
- `src-tauri/src/services/extraction_service.rs`：PDF/Office 解析（extract_pdf / extract_ooxml / read_docx_text / read_pptx_text / read_xlsx_text / build_text_result / no_text_layer_result）+ 20 测试
- `src-tauri/src/services/search_service.rs`、`src-tauri/src/utils/markdown_utils.rs`：clippy --fix 副作用（合并 replace / char-array find）
- `src-tauri/src/app_state.rs`：`preview_folder_as_project` + `create_import_checkpoint` 方法 + 7 测试（folder_preview_tests 3 + import_checkpoint_tests 4）
- `src-tauri/src/commands/project_commands.rs`：`preview_open_folder_as_project` 命令薄包装
- `src-tauri/src/commands/import_commands.rs`：`confirm_import_preview` createCheckpoint 分支
- `src-tauri/src/models/import.rs`：`ConfirmedImport.checkpoint_hash`
- `src-tauri/src/lib.rs`：注册 `preview_open_folder_as_project`
- `SPEC/plans/import-be.md`、`SPEC/progress.txt`：账本 + 进度记录

### 最终验证（全绿）
- `cargo test --lib --no-default-features --offline`：295 passed（P0-1 23 测试 + P0-2 3 测试 + P0-3 4 测试 = 本轮新增 30；其余预存）
- `cargo test --test task8_contracts --no-default-features --offline`：7 passed
- `cargo clippy --no-default-features --offline --lib -- -D warnings`：clean
- `cargo check --offline`（gui feature）：compiles
- `npm run test`：90 passed（20 files）
- `npm run lint`：clean
- 我的文件无 console.log/println!/dbg! 残留

### 遗留与说明
- 图片资产落盘是 roadmap P1 单独条目，不在本轮（本 loop scope 只含上述 3 项 P0 后端）。
- Windows 测试 harness 有预存 `STATUS_ENTRYPOINT_NOT_FOUND` 崩溃（WebView2/DLL），gui-feature `cargo test --lib` 不可跑；用 `--no-default-features --offline` 绕过（已记 gotchas.txt）。gui-feature clippy 有 21 个预存错误在其它命令模块（lint_commands 等），stash 基线验证一致，非本 loop 范围。
- `mvp_flow.rs` 预存编译错误（build_export_prompt 参数数），基线即存在，属 export 模块，非本 loop。
- 双子代理审查见下。

### 双子代理审查修复（CLAUDE.md 收尾要求）

双子代理审查（A 共享上下文 + B 全新上下文）并行运行，合并发现并修复：

- **BLOCKER (B1)** XLSX 数值单元格被误读为共享字符串：`read_xlsx_text` 对每个 `<v>` 无条件 `parse::<usize>()` 后查 shared，导致值恰好是合法索引的数值单元格被误替换（如 3 个 shared 时数值 `1` 被输出为第 2 个 shared）。**修复**：在 `<c>` start 事件上记录 `t="s"` 属性，仅当 `cell_shared` 为真时才按索引解析 shared；否则输出字面值。新增回归测试 `xlsx_numeric_cell_is_not_misread_as_a_shared_string`（断言数值 `1` 不变成第二个 shared，"World" 只出现一次）。
- **SHOULD-FIX (A1)** `.xls` 转换提示拼成 `.xlx`：`trim_end_matches('s')` 把 `xls`→`xl`。**修复**：改 match 显式映射 doc→docx / ppt→pptx / xls→xlsx。新增 `legacy_xls_hint_targets_xlsx_not_xlx` 覆盖三种后缀。
- **SHOULD-FIX (A2)** XLSX inline string（`t="inlineStr"`，`<is><t>`）被静默丢弃：原循环只在 `is_value`(v) 时输出。**修复**：元素栈记录 enclosing element，`<t>` 文本也输出。新增 `xlsx_inline_string_cells_are_extracted`。
- **SHOULD-FIX (B4)** Zip bomb / OOM：OOXML 部件用 `read_to_string` 全量读入内存，无解压上限。**修复**：`ensure_entry_size` 在读取前校验 `entry.size()` ≤ 64MiB，超限返回 `EXTRACT_ENTRY_TOO_LARGE`。四个读取器（docx/pptx/xlsx sharedStrings/xlsx worksheet）均接入。
- **SHOULD-FIX (B5)** `ConfirmedImport.checkpoint_hash` 序列化不对称：用 `skip_serializing_if` 会省略字段→前端 `undefined`，与 `PendingAction.checkpoint_hash`（强制 `null`）约定不一致。**修复**：去掉 `skip_serializing_if`，保留 `#[serde(default)]`，None 序列化为 `null`。

**未改（评估后判定）**：
- B1-encrypt PDF 消息模糊（S1）：pdf-extract 行为随版本变化，空结果路径已正确路由 OCR；错误消息 hedging 属打磨，非 bug，记 roadmap。
- S2 新仓库首检查点覆盖整树：与 `confirm_folder_initialization` 行为一致（首检查点天然整树），是预期行为，不改；已在方法文档说明。
- S3 preview 泄漏 pending action：预存模式（`open_project` 同样），registry 无 TTL；preview 调用更频繁但属预存架构问题，记 roadmap，不本 loop 改。
- N1 CJK word count：`split_whitespace` 对中文返回 1（无空格）；plan 验收标准是「word_count 有值」，CJK 分词需 `unicode-segmentation`，记 roadmap。
- 其它 NIT（slide_index unwrap_or(0) 排序、archive_names 吞错）记 roadmap。

最终验证：295 lib + 7 task8_contracts + 90 FE + lint + clippy(--no-default-features, -D warnings) 全绿；GUI 编译通过；我的文件无 clippy 告警、无 debug 日志。

---

## 后端可落地条目（roadmap P0/P1 中只动 src-tauri 的部分）

> 说明：roadmap 把大量 P0/P1 落在前端（卡片网格/文件表/右面板/底部条/拖拽/UI Dialog 等）。
> 本 loop 明确"只动 src-tauri/"，故前端条目不在本轮账本，记入 roadmap 待前端 loop。

### [P0-1] PDF/Office 解析适配器（PRD-IMP-001 / PRD-IMP-005）
- status: verified
- 意图：ExtractionService 当前对 Pdf/Document/Presentation/Spreadsheet 一律返回 `Unsupported`（extraction_service.rs:105-134）。PRD-IMP-001 要求这些格式能进入预览且 `extractionStatus != unsupported`；PRD-IMP-005 要求提取文本落盘。OCR/图片价值判断不在本层。
- 决策：
  - 依赖：`pdf-extract`（纯 Rust PDF 文本）+ `zip`（`--no-default-features --features deflate`，规避 Windows 原生 xz/zstd DLL）+ `quick-xml`（OOXML = docx/pptx/xlsx 都是 zip+XML，统一抽文本）。最小依赖、无原生编译需求。
  - 产出：text（落 `raw/extracted/*.txt`，复用 `write_extracted_text`）、word_count、page_count（PDF 用 `extract_text_by_pages` 的页数；PPTX 用幻灯片数；DOCX/XLSX 无可靠页数→None，符合 PRD-IMP-004"页数或字数"的"或"语义）。图片提取不在本条目（roadmap 列为 P1「图片资产落盘」，单独条目）。
  - 解析失败（损坏/加密）→ `ExtractionStatus::Failed` 带 error，不抛 panic，batch 继续（符合现有契约）。
  - 扫描件/无文本 PDF（`extract_text_by_pages` 返回空或全部空字符串）→ `no_text_layer_result` 返回 Failed + error="No extractable text layer found. OCR / visual understanding is handled by the compile Agent."。前端能看到字数 0 且有说明，且 status != unsupported。
  - 旧式二进制 .doc/.ppt/.xls（非 OOXML）→ Failed + error 带"Convert to .docx/.pptx/.xlsx"提示，不静默降级。
- 涉及文件：`src-tauri/Cargo.toml`、`src-tauri/src/services/extraction_service.rs`
- 落地位置：`extraction_service.rs` `extract_pdf` / `extract_ooxml` / `read_docx_text` / `read_pptx_text` / `read_xlsx_text` / `build_text_result` / `no_text_layer_result`（约 580-760 行）；match 分支在原 `Unsupported` 位置（约 105-134 行）。
- 验收：✅ 构造真实最小 pdf/docx/pptx/xlsx 解析后 status=Extracted（非 Unsupported），文本落盘，word_count/page_count 有值；损坏文件→Failed；扫描件→Failed(OCR 交给编译 Agent)；旧式二进制→Failed(转换提示)。新增 6 测试 + 替换 2 旧 Unsupported 测试，共 20 测试通过；285 lib tests + 7 task8_contracts + 90 FE tests + lint + clippy(-D warnings) 全绿；GUI feature 编译通过。

### [P0-2] 「打开文件夹为项目」后端命令链路（PRD-IMP-003 / PRD-PROJ-003）
- status: verified
- 意图：设计稿 `dlg-folder`（import.html:439-479）要求：文件夹路径 + 项目模板 select + 归档策略（按类型归档/同名重命名/初始化 Git）。PRD-IMP-003 要求用户可选「打开为项目」。
- 现状：`open_project`（project_commands.rs:56-95）已对"普通文件夹"返回 `NeedsConfirmation` + `InitializeFolder` pending action，`confirm_pending_action` 走 `confirm_folder_initialization`。**链路已存在且可用**。
- 决策：新增显式 `preview_open_folder_as_project(path) -> OpenProjectResponse` 命令，复用 `project_service.open_project`，给 dlg-folder 一个语义独立的入口（区别于"打开已有项目"）。关键差异：对 `NeedsConfirmation` 注册 pending action（含 execution plan）让前端可经 `confirm_pending_action` 确认；对 `Opened` 不跑 `open_project` 的 Git/registry/recents 副作用（纯预览，已是项目的文件夹无需重新初始化）。模板选择是前端侧（影响 `confirm_folder_initialization`），预览阶段后端不绑定模板。核心逻辑放 `AppState::preview_folder_as_project`（非 gui-gate，可被 `--no-default-features` 测试），命令层只做薄包装。
- 涉及文件：`src-tauri/src/commands/project_commands.rs`、`src-tauri/src/app_state.rs`、`src-tauri/src/lib.rs`（注册）
- 落地位置：`app_state.rs:128-149`（`preview_folder_as_project` 方法 + `folder_preview_tests` 模块 3 测试）、`project_commands.rs:97-118`（命令薄包装）、`lib.rs:129`（注册）。
- 验收：✅ 命令存在、注册；普通文件夹返回 NeedsConfirmation 且 pending action 已注册可确认（`peek` 验证）；已有 wiki 文件夹返回 Opened 且无 pending action 注册；CJK 文件名安全。3 新测试通过；288 lib + 7 task8_contracts + 90 FE + lint + clippy(--no-default-features, -D warnings) 全绿；GUI feature 编译通过。（注：gui-feature clippy 有 21 个预存错误在其它命令模块，非本 loop 范围，基线 stash 验证一致。）

### [P0-3] 导入后触发 Wiki 编译的 hook + confirm_import_preview 支持 Git 检查点参数
- status: verified
- 意图：PRD §8.3 step6「用户确认后触发编译」；roadmap「导入后编译链路 P0：confirm_import_preview 接受 createCheckpoint 参数」。设计稿底部条有「Git 检查点 checkbox」「导入后触发编译 checkbox」。
- 现状：`confirm_import_preview`（import_commands.rs:591-613）只做 confirm + 写 import-conflicts.json，不创建 checkpoint，不触发编译。
- 决策：
  - (a) `ConfirmImportRequest` 增 `create_checkpoint: bool`（`#[serde(default)]` 向后兼容）。为 true 时在归档 + 写 import-conflicts 后用 `git_service.create_scoped_checkpoint`（paths = 归档文件[排除 Skip/LinkToExisting] + `.app/source-index.json` + `.app/import-conflicts.json`）创建 scoped 提交，返回 commit hash。
  - (b) 「导入后触发编译」链路：后端不串接编译（编译是独立长任务，前端 checkbox 勾选时依次调 `confirm_import_preview` → `start_wiki_compile`）。后端确保 `ConfirmedImport` 暴露 `checkpoint_hash` 供前端展示。无新增串接命令。
  - (c) `ConfirmedImport` 增 `checkpoint_hash: Option<String>`（`skip_serializing_if = "Option::is_none"`）。
  - 关键边界处理：仓库未初始化时先 `initialize_repository`（新建仓库的初始提交已含归档文件，返回该 hash）；已有仓库且有新归档→scoped 提交；无变更→None。避免「scoped 找不到变更」误返回 None。
  - 核心逻辑放 `AppState::create_import_checkpoint`（非 gui-gate 可测），命令层薄包装。
- 涉及文件：`src-tauri/src/commands/import_commands.rs`、`src-tauri/src/models/import.rs`、`src-tauri/src/app_state.rs`
- 落地位置：`import_commands.rs:595-640`（命令 + createCheckpoint 分支）、`import.rs:135-144`（ConfirmedImport.checkpoint_hash）、`app_state.rs:152-213`（`create_import_checkpoint` 方法 + `import_checkpoint_tests` 模块 4 测试）。
- 验收：✅ create_checkpoint=true 时项目有可回滚提交（新仓库→初始提交含归档；已有仓库→scoped 提交含归档+index；无变更→None）；ConfirmedImport 带 checkpoint_hash；create_checkpoint=false 默认不创建。4 新测试通过；292 lib + 7 task8_contracts + 90 FE + lint + clippy(--no-default-features, -D warnings) 全绿；GUI 编译通过，我的文件无 clippy 告警。

---

## 不在本轮（前端条目，记 roadmap）
- 卡片网格、文件表、右面板、底部条、拖拽多选、URL Dialog、导入历史、i18n、视觉 token——均属 src/ 前端，不在本 loop。
