# 历史 Import 路线图（禁止直接执行）

> 本文保留 2026-06 至 2026-07 早期实现审计，只用于追溯。当前 Import / Source / Media 唯一权威是 [`../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md)；项目创建与打开唯一权威是 [`../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)。
>
> 本文所有“打开文件夹为项目”“原地初始化普通文件夹”“移动原资料”“导入后自动/可选编译”“编译期 OCR”及对应 P0/实施步骤均已废止，不得据此实现。Import 只能把文件、文件夹、URL 或剪贴板内容预览并复制到**当前已打开的知识库**；若用户选择的是普通资料文件夹，必须另建知识库后再导入，原目录保持不变。

## 原始路线图快照：Import 板块落差与实施计划

> 对照源：UI-Frontend-design/import.html + assets/app.css + SPEC/PRD.md
> 当前实现：src/features/import/、src/lib/readability.ts、src/types/import.ts
> 后端实现：src-tauri/src/services/import_service/、extraction_service.rs、commands/import_commands.rs

## 2026-06-22 导入闭环修复更新（覆盖下方旧现状）

- 已完成：Tauri v2 原生拖拽（`event.payload`）、Windows/CJK 路径透传与 listener 生命周期测试。
- 已完成：PDF 文本层、DOCX、PPTX、XLSX、CSV 转结构化 Markdown；成功产物统一为 `raw/extracted/*.md`。扫描 PDF 不在导入层做 OCR，明确交给编译期 Agent/Skill。
- 已完成：compile 只消费 `.app/source-index.json` 中已确认且非空的 extracted Markdown；没有有效输入时报 `COMPILE_INPUT_EMPTY`；修改已有 Wiki 页面前创建 checkpoint 并进入 PendingAction 确认流。
- 已完成：64 MiB 源文件/OOXML 累计展开限制、4096 ZIP entry 限制、XLSX XFD 列上限、shared-string 越界失败、extracted 外部编辑不覆盖。
- 已完成（2026-07-10）：`useImportWorkflow` 独立拥有 preview、URL/clipboard、source action 与确认编排；确认顺序固定为 `confirm_import_preview` → Wiki `scan` → 按用户选项启动 compile，`AppShell` 不再直接发 import/compile commands。
- 仍属后续范围：把 preview extracted 产物完全移入临时 staging 并在确认时原子迁移/取消时清理。目前孤儿 preview 文件不会进入 compile、也不会覆盖外部编辑，但可能占用磁盘。
- 仍属后续范围：可选 MinerU 集成、PDF/Office 内嵌图片提取与 URL 图片本地化。它们需要独立的隐私、网络、配额和资产生命周期设计，不在本次修 bug 中静默引入。

## 0. 现状摘要

Import 板块已打通「路径 / URL / 剪贴板」三种来源的预览-确认-归档主链路：

- **后端核心成熟**：`ImportService` 完成文件分类、按类型路由到 `raw/sources|assets/`、去重（SHA256）、同名冲突确定性重命名、确认归档、staging text、`source-index.json` 资产索引，并有完整的单元测试覆盖（含 CJK 文件名、嵌套目录、原子回滚、source 变更校验）。见 `src-tauri/src/services/import_service/{classification,preview,confirmation,source_catalog}.rs`。
- **提取层有意收窄**：`ExtractionService` 只支持 md/txt/csv/html/url 的纯文本提取；PDF/DOCX/PPTX/XLSX 一律返回 `ExtractionStatus::Unsupported`（占位错误信息「Parser adapter not yet available」）。图片不做 OCR（符合硬约束）。见 `src-tauri/src/services/extraction_service.rs:105-134`。
- **URL 抓取走 Rust reqwest + SSRF 防护**（禁止私网 IP、限 5MB、禁重定向），HTML 回前端后由 `@mozilla/readability` + 自写 `articleToMarkdown` 转 Markdown（含 frontmatter / 图片绝对化 / 链接绝对化）。见 `src-tauri/src/commands/import_commands.rs:481-585`、`src/lib/readability.ts`。
- **Source 生命周期闭环**：删除 / 替换 raw/sources 走 `PendingAction` 确认 + Git scoped checkpoint + 失败回滚 + source-index 同步。见 `src-tauri/src/services/import_service/source_actions.rs`、`commands/import_commands.rs:123-275`。
- **前端导入动作闭环已实现**：`ImportView` 已提供来源入口、预览列表和 `import-actions` 底部确认条；动作条包含无损保留说明、Git 检查点与导入后编译 checkbox、取消和确认按钮，`useImportWorkflow` 负责确认、scan 与可选 compile。仍未补齐的独立信息区包括归档规则、完整冲突记录与导入历史。

整体完成度：**后端约 75%，前端约 35%**。落差集中在 UI 重构、批量文件选择、解析预览面板、导入历史、PDF/Office 解析适配器、文件夹打开为项目流程。

## 1. 区块 / 组件清单

| 区块/组件 | 设计稿要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 顶栏：标题 + 文件计数 + 清空/打开文件夹按钮 | 标题副标「解析预览 · N 文件 · M URL · K 冲突」，右侧「清空」「打开文件夹为项目」按钮 | 无；只有视图内摘要条 | ❌缺失 | P1 | `src/features/import/ImportView.tsx:70-178` |
| 来源卡片网格（6 张卡片） | 本地文件 / 文件夹 / URL / 剪贴板 / 浏览器扩展（禁用）/ GitHub·RSS·音视频（禁用）；卡片含大图标、标题、描述、hover 高亮 | 用 Tab 单选切换 path/url/clipboard 三种模式，无卡片、无图标、无「文件夹」与「浏览器扩展」入口 | ❌缺失 | P0 | `src/features/import/ImportView.tsx:74-109` |
| 本地文件选择（多选 + 拖拽） | 点击卡片弹原生选择器；支持拖拽；显示已选文件 | 只能粘贴一个绝对路径字符串；无多选；无拖拽；无文件选择对话框 | ❌缺失 | P0 | `src/features/import/ImportView.tsx:98-101` |
| 文件夹导入 | 卡片「文件夹」：导入到当前项目 或 打开为项目 | 只能把文件夹路径粘到 path 输入；无「打开为项目」入口；无 PRD-IMP-003 的二选一弹窗 | 🟡部分实现 | P0 | `src/features/import/ImportView.tsx:98-101`、`src-tauri/src/commands/import_commands.rs:277-430` |
| URL 导入对话框（dlg-url） | 独立对话框：链接输入 + Readability 提示 + 抓取并预览 | 内联到 Tab，输入框 + 提交；无独立 Dialog；交互可跑通 | 🟡部分实现 | P1 | `src/features/import/ImportView.tsx:101-108` |
| 文件表（待确认列表） | 表格：checkbox / 图标 / 文件名+摘要 / 类型徽章 / 大小 / 页数或字数 / 目标路径 / 状态徽章 / 预览按钮；顶部聚合「N 解析成功 / M 部分失败 / K 失败 / 总大小」 | 左侧 button 列表：状态图标 / 文件名+重命名标记 / 类型+大小+字数+页数 / 状态徽章；无 checkbox、无目标路径列、无摘要徽章组、无预览按钮（整行点击切换） | 🟡部分实现 | P0 | `src/features/import/ImportView.tsx:182-234` |
| 右侧解析预览面板 | 选中文件详情：图标+文件名+大小页数 / 元数据 dl（类型、页数、字数、图片数、表格数、语言）/ 提取预览（首页）/ 归档规则清单 / 冲突记录（`.app/import-conflicts.json` 状态）/ 导入历史 | 右半边：标题+路径 / 状态 / 冲突详情 / 文本预览 / 元数据 dl（仅 title/author/words/pages）/ hash；**无归档规则、无冲突记录板块、无导入历史** | 🟡部分实现 | P1 | `src/features/import/ImportView.tsx:238-348` |
| 提取文本预览（首页正文） | 等宽字体框、max-height 240、显示前 N 行 + 「后续 N 页」提示 | `<pre>` 500 字截断 + 省略号；无分页、无「后续 N 页」 | 🟡部分实现 | P2 | `src/features/import/ImportView.tsx:292-301` |
| 元数据：图片数 / 表格数 / 语言 | 类型 / 页数 / 字数 / 图片 / 表格 / 语言 6 项 | 只有 title / author / words / pages 4 项；缺 imageCount、tableCount、language | ❌缺失 | P2 | `src/types/import.ts:32-40`、`src/features/import/ImportView.tsx:304-336` |
| 底部确认条（import-actions） | 左侧「导入层只负责无损保留」提示；右侧：Git 检查点 checkbox、导入后编译 checkbox、取消、确认导入并编译 | UI 控件与文案已完整实现，选项交给 `useImportWorkflow.confirm`；但后端 `confirm_import_preview` 当前先执行 import 写入，再按 checkbox 创建 checkpoint，不满足“危险写操作前检查点”语义 | 🟡部分实现 | P0 | `src/features/import/ImportView.tsx:439-471`、`src/features/import/useImportWorkflow.ts:286-324`、`src-tauri/src/commands/import_commands.rs:597-625` |
| 冲突记录板块 | 独立 section：`.app/import-conflicts.json` 为空 / 有冲突详情 | 只有底部一条 warning banner；无独立板块 | 🟡部分实现 | P1 | `src/features/import/ImportView.tsx:352-358` |
| 导入历史 | 独立 section：时间 + 摘要 + 冲突数，多条 | 完全缺失 | ❌缺失 | P2 | `src/features/import/ImportView.tsx` |
| 归档规则展示 | 独立 section：PDF→raw/sources/pdfs 等 7 条规则 + 「原文件不可变」提示 | 完全缺失（后端逻辑存在于 `target_archive_dir`） | ❌缺失 | P2 | `src-tauri/src/services/import_service/classification.rs` (`target_archive_dir`) |
| 批量 checkbox / 清空 | 表头全选 checkbox + 单行 checkbox + 顶部「清空」 | 无 checkbox、无批量操作、无清空 | ❌缺失 | P1 | `src/features/import/ImportView.tsx` |
| 导入后触发 Wiki 编译 | 设计稿 checkbox「导入后触发 Wiki 编译」+ 主按钮「确认导入并编译」 | `ImportView` 传入 `compileAfterImport`；`useImportWorkflow.confirm` 按 `confirm_import_preview` → Wiki `scan` → 可选 `startCompile` 顺序执行 | ✅已完成 | — | `src/features/import/ImportView.tsx`、`src/features/import/useImportWorkflow.ts` |
| Source 替换/删除 UI | 设计稿未画，PRD-IMP-005 / PRD-WIKI-005 要求 | `<details>` 折叠面板：source 列表 + 替换路径输入 + 替换/删除按钮；功能可用但视觉与设计稿无关 | 🟡部分实现 | P1 | `src/features/import/ImportView.tsx:111-127` |
| PDF/Office 解析 | PRD-IMP-001 要求 PDF/DOCX/PPTX/XLSX 能进入预览 | 文件能归档但 extraction 返回 `Unsupported`，无文本/页数/字数/图片/表格 | ❌缺失 | P0 | `src-tauri/src/services/extraction_service.rs:105-134` |
| 图片资产提取 | PRD-IMP-005 要求导入层保留图片 | 图片被归档到 `raw/assets/`；但 PDF/Office 中的图片无适配器提取；URL 中的 `<img>` 在 Readability 阶段转为 Markdown 引用，不下载落盘 | 🟡部分实现 | P1 | `src-tauri/src/services/extraction_service.rs:135-156`、`src/lib/readability.ts:57-61` |
| 来源元数据保留 | PRD-IMP-005：原文件 + 提取文本 + 图片 + 来源元数据 | md/txt/csv/html 提取字数与标题；PDF/Office 无适配器无元数据；URL 有 title/byline/source_url frontmatter；缺 created/modified/language/imageCount/tableCount | 🟡部分实现 | P1 | `src-tauri/src/services/extraction_service.rs`、`src/lib/readability.ts:90-105` |
| 「打开文件夹为项目」对话框（dlg-folder） | 警告条 + 文件夹路径 + 项目模板 select + 归档策略 checkboxes（按类型归档、同名重命名、初始化 Git） | 完全缺失；无对应入口或命令 | ❌缺失 | P0 | `UI-Frontend-design/import.html:439-479`、`src-tauri/src/commands/project_commands.rs` |
| 任务进度反馈 | PRD：长任务可报告进度 | `preview_import` 是后端任务，有 progress / cancel；但 ImportView 内不展示当前 preview 任务的进度条或日志入口 | 🟡部分实现 | P1 | `src/features/import/ImportView.tsx`、`src/hooks/useTaskEvents.ts` |
| 无损保留说明 | 底部条强调「原文件 / 提取文本 / 图片 / 来源元数据 / OCR 交给编译 Agent」 | `import-actions__note` 已通过 `import.actions.note` / `note.detail` 双语文案说明无损保留与 OCR/视觉理解边界 | ✅已完成 | — | `src/features/import/ImportView.tsx:439-442`、`src/i18n/locales/{en,zh-CN}.json` |

## 2. 功能落差（PRD 对照）

- [ ] **PRD-IMP-001 多格式解析（P0）**：现状 = PDF/DOCX/PPTX/XLSX 在 `ExtractionService` 直接返回 `Unsupported`，只有 md/txt/csv/html/url 能出文本 → 目标 = 接入 PDF/Office 解析适配器（如 `pdf-extract` / `docx-rs` / `calamine` 或外部 CLI），至少产出文本、页数、字数 → 涉及 `src-tauri/src/services/extraction_service.rs:105-134`、`src-tauri/Cargo.toml` → 验收 = 小型多格式测试资料包中所有 PRD-IMP-001 列出格式均能进入预览且 `extractionStatus != unsupported`。
- [ ] **PRD-IMP-003 文件夹导入 / 打开为项目（P0）**：现状 = 只能粘贴文件夹路径，预览归档到当前项目 → 目标 = 卡片「文件夹」弹出对话框，二选一「导入到当前项目 / 打开为项目」；后者调用 project 初始化命令（移动文件、生成 purpose/schema、初始化 Git） → 涉及 `src/features/import/ImportView.tsx`、`src-tauri/src/commands/project_commands.rs`、`src-tauri/src/commands/import_commands.rs` → 验收 = 用户可在「导入到当前项目」与「打开为项目」之间明确选择；选择后行为符合 PRD §8.2/8.3。
- [ ] **PRD-IMP-004 预览完整性（P0）**：现状 = 缺目标路径列、图片/表格计数、语言、归档规则展示、批量 checkbox → 目标 = 表格按设计稿补齐列；元数据补 imageCount/tableCount/language；右侧面板补归档规则与冲突记录板块 → 涉及 `src/features/import/ImportView.tsx:182-348`、`src/types/import.ts:32-40`、`src-tauri/src/models/import.rs` → 验收 = 预览展示文件列表、格式、大小、状态、文本预览、页数或字数、目标路径，并区分成功/失败。
- [ ] **PRD-IMP-005 图片与元数据无损保留（P0）**：现状 = 原文件归档 ok；PDF/Office 内嵌图片不提取；URL 图片只转 Markdown 引用不落盘 → 目标 = PDF/Office 解析时把图片落到 `raw/assets/`；URL 抓取时可选下载图片到 `raw/assets/` 并改写 Markdown 链接 → 涉及 `src-tauri/src/services/extraction_service.rs`、`src-tauri/src/commands/import_commands.rs:481-585`、`src/lib/readability.ts` → 验收 = 项目目录中可找到原始资料与提取资产（含图片）。
- [ ] **PRD-IMP-006 OCR 不在导入层（P1）**：现状 = 符合约束（图片直接归档，不 OCR） → 目标 = 保持现状，但 UI 应在底部条/右面板明确说明「OCR 与视觉理解交给编译 Agent」避免用户期待 → 涉及 `src/features/import/ImportView.tsx`、`src/i18n/locales/*.json` → 验收 = UI 文案明确说明 OCR 不在导入层。
- [x] **导入后编译链路（P0）** @ 2026-07-10：底部条选项由 `ImportView` 传入；`useImportWorkflow.confirm` 在归档确认和 Wiki scan 成功后按需启动 compile，用户一次确认即可完成导入 + 编译。
- [ ] **Git 检查点预操作语义（P0）**：底部条「创建 Git 检查点」checkbox 已实现并默认开启，参数也已传到 `confirm_import_preview`；真实缺口是该 command 在 `ImportService::confirm_import` 和 conflict JSON 写盘**之后**才调用 `create_import_checkpoint`。目标 = 任何 import 写入前创建 checkpoint，checkpoint 失败时不得产生文件变更 → 涉及 `src-tauri/src/commands/import_commands.rs:597-625`、`src-tauri/src/services/import_service/confirmation.rs` → 验收 = 确认导入前已有可回滚提交，模拟 checkpoint 失败时项目内容不变。
- [ ] **拖拽与多选文件（P0）**：现状 = 只能粘贴单路径 → 目标 = 卡片支持点击打开文件选择器（多选）+ 拖拽到卡片或表格 → 涉及 `src/features/import/ImportView.tsx`、Tauri `dialog`/`fs` 插件 → 验收 = 用户可拖拽或选择多个文件进入预览。
- [ ] **导入历史（P2）**：现状 = 无 → 目标 = 右面板显示最近 N 次导入（时间、项数、冲突数），从 `.app/import-conflicts.json` 或新建 `.app/import-history.json` 读 → 涉及 `src/features/import/ImportView.tsx`、`src-tauri/src/services/import_service/`、`src-tauri/src/models/import.rs` → 验收 = 历史可追溯。

## 3. 视觉 / 设计 token 落差

- **整体布局错位**：设计稿是 `import-layout` 三段（来源区 / 表格 / 底部条）+ 右侧独立 rightpanel；当前实现是「顶部 tab+输入 + 左右双栏 + 顶部摘要按钮」自定义结构，未使用设计稿布局。涉及 `src/features/import/ImportView.tsx:70-360`。
- **来源卡片视觉缺失**：`.import-source` 卡片（dashed 边框、`--radius-lg`、icon 32×32 bg `--surface-muted`、title 13/600、desc 11.5 muted）完全未实现。涉及 `UI-Frontend-design/import.html:22-47`、`src/features/import/ImportView.tsx:74-87`。
- **表格样式未对齐**：设计稿使用 `.table` 全宽表头表行 + 类型徽章 + `col-path` mono；当前用 button 列表模拟，无 table 语义。
- **底部确认条已落地**：`.import-actions` 已包含无损保留文案、两个 checkbox、取消与确认操作；剩余问题是 Git checkpoint 的后端执行顺序，而非 UI 缺失。
- **右侧面板未独立**：当前详情塞在右半栏，未使用 `rightpanel` 容器与 `rightpanel__section` 分段。
- **顶部摘要徽章组缺失**：设计稿 `badge--success/warn/danger` 三色 dot + 总大小 mono；当前只有文本计数 span。
- **图标尺寸**：设计稿要求来源卡片用 `ico-xl`、文件表用 `ico-sm` 14px、状态栏 12px；当前 `Upload` 用 16px、`StatusIcon` 用 14px，但缺少 PDF/DOC/PPTX/Sheet/MD/URL 等类型图标。

## 4. 交互 / 可访问性落差

- **无原生文件选择 Dialog**：用户必须手动粘贴绝对路径，对普通用户不友好；应通过 Tauri `dialog.open({multiple: true})` 提供。
- **无拖拽支持**：设计稿暗示卡片可点击 + 文件可拖入；实现无 `onDrop` / `onDragOver`。
- **无批量选择**：表头/行 checkbox 缺失，无法批量确认或排除。
- **URL 交互降级**：设计稿是独立 Dialog（`dlg-url`），实现是 Tab 内嵌；两者交互节奏不同，Dialog 更符合「抓取并预览」心智。
- **键盘可达性**：当前 button 列表可键盘选择，但来源 Tab、输入框、提交按钮之间缺乏 `tabindex` 序列与 `aria-labelledby` 关联；右面板 section 缺 `aria-label`。
- **进度反馈不足**：preview 是后台任务，但 ImportView 没有绑定当前 preview 任务的 progress / cancel UI；用户只能通过当前项目的任务抽屉查看。
- **i18n 后续项**：动作条的无损保留、OCR/视觉理解边界与确认操作已加入 `src/i18n/locales/{en,zh-CN}.json`；仍需随未来归档规则、完整冲突记录和导入历史板块补齐对应文案。
- **冲突信息展示薄弱**：底部 warning banner 是英文硬编码「N conflict(s) found…」（`ImportView.tsx:354-357`），未用 i18n；冲突详情只在右面板按文件显示，无全局冲突列表。

## 5. 建议实施顺序

1. **P0 - UI 骨架重构**：按设计稿重写 ImportView 为「卡片网格 + 文件表 + 右面板 + 底部条」四区，复用 `rightpanel`、`table`、`badge` 等已有样式；Tab 模式退化为卡片选择。涉及 `src/features/import/ImportView.tsx`、新增 `src/features/import/SourceCardGrid.tsx`、`ImportFileTable.tsx`、`ImportRightPanel.tsx`、`ImportActionsBar.tsx`。
2. **P0 - 文件选择与拖拽**：卡片接入 Tauri `dialog.open({multiple:true})`、`dialog.open({directory:true})`；卡片与表格支持 `onDrop`。新增 `src/features/import/useFileDrop.ts`。
3. **P0 - 「打开文件夹为项目」对话框**：新增 `src/components/app/OpenFolderAsProjectDialog.tsx` + 后端 `open_folder_as_project` 命令（移动文件、建结构、初始化 Git），满足 PRD-IMP-003。
4. **P0 - PDF/Office 解析适配器**：在 `ExtractionService` 引入 `pdf-extract`/`docx-rs`/`calamine` 等 crate 或外部 CLI；先做到文本 + 页数 + 字数，图片提取放到下一轮。
5. **🟡 部分完成 - 底部确认条与编译链路**：UI 与 `confirm_import_preview` → Wiki scan → 可选 compile 编排已完成；P0 剩余项是把 `createCheckpoint` 对应的 checkpoint 移到任何 import 写入之前。
6. **P1 - 右面板完善**：补「归档规则」「冲突记录」「导入历史」三段；元数据补 imageCount/tableCount/language（同步 `SourceMetadata` TS/Rust 模型）。
7. **P1 - URL Dialog 还原**：拆出独立 `ImportUrlDialog`，匹配设计稿交互；保留 Readability + Markdown 转换现有逻辑。
8. **P1 - 图片资产落盘**：URL 抓取后下载同源图片到 `raw/assets/`，改写 Markdown 链接；PDF/Office 解析阶段提取内嵌图片。
9. **P2 - i18n / 无障碍 / 视觉打磨**：补齐 i18n key；补 aria 属性；按 app.css 校准字号/间距/颜色 token；用 Lucide 类型图标替换通用 `File`。
10. **P2 - 性能与稳定性**：大文件夹（>200 文件）预览增量渲染；preview 任务进度回流到 ImportView；source-index 升级兼容老项目。
