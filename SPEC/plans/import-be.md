# Import 后端 P0+P1 进度账本

> /loop 本轮范围：只动 `src-tauri/`。对照源 SPEC/PRD.md、UI-Frontend-design/import.html、CLAUDE.md。
> 硬约束：导入层只无损保留（原文件/提取文本/图片/来源元数据）；OCR/视觉理解交给编译 Agent/Skill。

✅ 本轮进行中 @ 2026-06-21

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
- status: pending
- 意图：PRD §8.3 step6「用户确认后触发编译」；roadmap「导入后编译链路 P0：confirm_import_preview 接受 createCheckpoint 参数」。设计稿底部条有「Git 检查点 checkbox」「导入后触发编译 checkbox」。
- 现状：`confirm_import_preview`（import_commands.rs:591-613）只做 confirm + 写 import-conflicts.json，不创建 checkpoint，不触发编译。
- 决策：
  - (a) `ConfirmImportRequest` 增 `create_checkpoint: bool`。为 true 时在归档成功后用 `git_service.create_scoped_checkpoint`（paths = 归档文件 + source-index.json + import-conflicts.json）创建检查点，返回值带 `checkpoint_hash`。
  - (b) 「导入后触发编译」的链路：后端不强行串接编译（编译是独立长任务，前端 checkbox 勾选时前端依次调 confirm → start_wiki_compile）。本 loop 确保命令链路通 = `confirm_import_preview` 成功后前端能拿到 confirmed 结果再调 `start_wiki_compile`。后端无需新增串接命令（避免把两个长任务耦合）。但需在 `ConfirmedImport` 暴露 checkpoint_hash 供前端展示。
  - (c) `ConfirmedImport` 增 `checkpoint_hash: Option<String>`。
- 涉及文件：`src-tauri/src/commands/import_commands.rs`、`src-tauri/src/models/import.rs`、`src-tauri/tests/`（若需）
- 验收：create_checkpoint=true 时项目有可回滚提交；ConfirmedImport 带 checkpoint_hash；cargo test 覆盖。

---

## 不在本轮（前端条目，记 roadmap）
- 卡片网格、文件表、右面板、底部条、拖拽多选、URL Dialog、导入历史、i18n、视觉 token——均属 src/ 前端，不在本 loop。
