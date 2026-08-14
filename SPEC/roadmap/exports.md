# Exports 板块落差与实施计划

> 对照源：UI-Frontend-design/exports.html + assets/app.css + SPEC/PRD.md（§9.10 HTML/卡片/报告）
> 当前实现：src/features/exports/、src/stores/exportStore.ts、src-tauri/src/services/export_service.rs、src-tauri/src/commands/export_commands.rs
> Workflows 迁移边界：[`../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md) 规定完整“生成内容”由统一准备与 Workflow 任务模型启动；Exports 的新建/重新生成继续进入该路径。Wiki 当前文章另有专用 `GenerateHtmlDialog` + 普通 Export task 的单篇快速例外，但不恢复 Exports 大型 `ExportDialog`。现有 Exports 结果、列表和预览页继续统一管理两条链路的 ExportRecord；本文件的 Exports UI 打磨项是独立后续工作，首版也不增加用户自定义模板。
> 项目访问权威：[`../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)。无项目时不能创建导出任务；外部 Agent、Skill 或 Provider 执行要求项目已信任；写入要求项目可写；覆盖、删除或批量改写还必须先建立 Git 检查点并显式确认。输出根目录由 `ProjectLayout` 解析，不能把 `exports/html/` 当作所有兼容项目的固定目录。

## 0. 现状摘要

Exports 的**当前原生项目后端链路已完整落地**：4 种导出类型（beautiful_read / knowledge_card / concept_map / project_report）均有 `skills/html-*` Skill 驱动（位于 `src-tauri/templates/skills/html-*/SKILL.md` + `template.html`），`ExportService` 负责 prompt 组装、输出路径推导、HTML 提取（去 ```html 围栏、删尾部 prose）、记录持久化到 `.app/exports.json`，并在现实现中强制所有输出落在 `exports/html/` 下。目标实现必须改为使用评估阶段返回的 `ProjectLayout`，且只能向布局允许的输出根写入。`export_commands.rs` 把 Wiki quick export 跑成可取消的普通 Task，并继续使用直接链路的自动 Agent/BYOK 路由；Workflows Generate Content 则在准备阶段解析设置默认路径或单次显式覆盖且不得静默回退。两条链路都保留取消信号轮询、HTML 合法性校验和跨平台 reveal-in-file-manager。

**前端已能跑通双入口后的结果链路**：Exports 顶栏“新建导出”和记录“重新生成/重试”携带 preset 进入 Workflows Generate Content preparation；记录表格、状态、任务日志、收藏、sandbox iframe 预览、浏览器打开和打开所在位置继续由 Exports 管理。Wiki 单篇 quick export 写入同一 ExportRecord 列表，但在 Wiki 内自动预览。

**主要落差集中在 UI 层与设计稿的偏差**：
1. 设计稿的“卡片式输出类型选择 + 缩略示意图”和 `dlg-export` 信息需求（源页、输出位置、执行路径等）不再由 Exports 结果页内联承接；完整 Generate Content preparation 当前使用类型化控件收集这些信息，未来如增强视觉也应在 Workflows 范围内完成。Exports 大型 `dlg-export` 不再作为待实现的独立对话框；当前恢复的是只服务 Wiki 单篇动作的 `GenerateHtmlDialog`，不得把两者混同。
2. 设计稿"已生成 · exports/html/" 是带状态徽章/失败重试/大小列/源列/执行路径列的 **表格**；当前实现是简化列表，缺失：文件大小、状态徽章、失败重试入口、执行路径列、缩略图标颜色（成功/失败）。
3. 设计稿右侧面板是"模拟浏览器 chrome + 文件信息 dl + 操作列 + 模板说明"四段式；当前实现只有 iframe + 关闭按钮。
4. 状态栏（路径/总数/体积/失败计数）缺失；主标题 "exports/html/ · 12 个已生成" 计数缺失。
5. 操作（在外部浏览器打开 / 复制路径 / 删除 / 重新生成 / 打开所在位置）除预览+重新生成+打开位置外均缺失。
6. 未实现模板选择（default-serif / modern-sans / editorial-magazine）—— `template.html` 存在但 prompt 未让用户选择模板样式。

## 1. 区块 / 组件清单

| 区块/组件 | 设计稿要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 主标题 + 计数 | `Exports · exports/html/ · 12 个已生成` | 无页头标题 | ❌缺失 | P1 | `src/features/exports/ExportsView.tsx:140-144` |
| 工具栏：打开输出目录 | 顶栏按钮 `打开输出目录` | 无（仅在每条记录上有"打开所在位置"） | ❌缺失 | P1 | `ExportsView.tsx:172-192` |
| 工具栏：新建导出 | 顶栏 primary 按钮进入共享 Generate Content preparation | 已携带 `origin: exports` 与默认 artifact preset 进入 Workflows | ✅已完成 | P0（合同） | `ExportsView.tsx`、`WorkflowPreparationView.tsx` |
| 输出类型选择 | 完整 preparation 提供 4 种内建类型；如采用卡片视觉，应在 Workflows 内展示缩略、标题、描述与 Skill | Workflows preparation 已提供 4 种类型化选择；Exports 结果页不再内联选择 | ✅合同完成 / P1 视觉待定 | P1（Workflows 侧） | `WorkflowPreparationView.tsx` |
| 源页选择器 | 共享准备页的类型化适用范围选择器 | Workflows preparation 已按 artifact type 约束单页、多页与项目范围 | ✅已完成 | P0（Workflows 侧） | `WorkflowPreparationView.tsx` |
| 模板下拉 | `default-serif / modern-sans / editorial-magazine` 3 选项 | 完全缺失（prompt 不传模板参数） | ❌缺失 | P1 | `src-tauri/src/services/export_service.rs:31-120`，`src/types/export.ts:24-32` |
| 输出位置 | 共享准备页展示 layout-derived 输出摘要；后端最终校验 | 后端按原生 `exports/html/` 自动生成，UI 不展示 | 🟡部分实现 | P0（合同）/P2（展示） | `export_service.rs:157-188` |
| 项目访问与兼容布局 | 无项目不可启动；restricted/read-only/trusted-writable 分层；输出根由 `ProjectLayout` 决定；执行前后端重验项目身份、访问级别与 Git 条件 | 当前主要按原生 `exports/html/` 与已打开项目假设运行，缺少统一访问合同 | ❌缺失 | P0 | `export_commands.rs`、`export_service.rs`、ProjectService/ProjectContext |
| 执行路径 | 在完整 Generate Content 准备页读取 Settings 默认值，允许单次显式覆盖 | Workflows preparation 展示有效路线并提供本次 route override；Wiki quick export 继续使用直接链路的 `auto` 默认路线 | ✅已完成 | P0（合同） | `WorkflowPreparationView.tsx`、[agent.md](agent.md) |
| 选项 checkbox | frontmatter 元数据 / 嵌入 CSS / 嵌入图片 base64 / 完成后打开预览 | 全部缺失 | ❌缺失 | P2 | 无对应字段 |
| 已生成表格 | 9 列：图标/文件名/类型/源/大小/生成时间/执行路径/状态/操作 | 4 列简化列表（图标/标题+类型+执行路径/输出路径/时间戳）；无大小、无状态徽章、无失败重试 | 🟡部分实现 | P0 | `ExportsView.tsx:204-264` |
| 失败记录重试 | 失败行显示 `重试` 按钮 | 未实现（`status` 字段在前端甚至未展示） | ❌缺失 | P0 | `ExportsView.tsx:204-264`，`src/types/export.ts:8`（已有 `failed` 状态） |
| 状态徽章 | `badge--success` 成功 / `badge--danger` 失败 + 圆点 | 未渲染 | ❌缺失 | P0 | `ExportsView.tsx` |
| 右侧面板：模拟浏览器 chrome | 红黄绿 3 圆点 + 文件名 + 刷新图标 | 无 chrome | ❌缺失 | P2 | `ExportsView.tsx:273-291` |
| 右侧面板：文件信息 dl | 路径/类型/大小/源页面/生成时间/Skill 版本 | 无 | ❌缺失 | P1 | `ExportsView.tsx:273-291` |
| 右侧面板：操作列 | 外部浏览器打开/打开所在位置/复制路径/重新生成/删除 | 仅"关闭预览" | 🟡部分实现 | P1 | `ExportsView.tsx:278-286` |
| 右侧面板：模板说明 | "HTML 模板只影响输出样式…" | 无 | ❌缺失 | P2 | — |
| 预览 iframe | sandbox 静态预览 | `srcDoc` + `sandbox=""`，实现正确且安全 | ✅已完成 | — | `src/features/exports/HtmlPreviewPane.tsx:14-32` |
| 预览空态 | — | 空态文案 + FileText 图标 | ✅已完成 | — | `HtmlPreviewPane.tsx:16-22` |
| 状态栏 | 路径 / Agent 版本 / `exports/html/ · 12 个文件 · 22 MB` / `1 个失败` / 分支 / 语言 | 无 Exports 专属状态栏 | ❌缺失 | P2 | `AppShell.tsx`（状态栏由 shell 管） |
| 完整生成准备 | Workflows Generate Content preparation 承接类型、范围、输出摘要、路线与 Git 策略；Exports 不新增 `dlg-export`，Wiki 专用 `GenerateHtmlDialog` 仅承接单篇 quick export | Exports 新建/重新生成已进入 Workflows；Wiki quick export 已复接专用弹窗 | ✅已实现 | P0（合同） | `src/features/workflows/`、`src/features/wiki/GenerateHtmlDialog.tsx` |
| 导出历史持久化 | `.app/exports.json` 新到旧 | 后端已实现并单测 | ✅已完成 | — | `export_service.rs:237-257` |
| skills/html-* 接入 | 4 个 skill 文件夹 + 模板 | 全部存在；`templates_carry_no_schema_or_lint_directives` 测试锁住"模板只影响样式"硬约束 | ✅已完成 | — | `src-tauri/templates/skills/html-*/SKILL.md`、`template.html`、`export_service.rs:604-628` |
| 路径安全 | 输出只能在 `exports/html/` | 三重校验（service / command / ProjectContext），单测覆盖逃逸 | ✅已完成 | — | `export_service.rs:179-187,223-230`，`export_commands.rs:328-335,347-355` |
| 跨平台打开位置 | explorer / open -R / xdg-open | 三平台分支已实现 | ✅已完成 | — | `export_commands.rs:361-382` |

## 2. 功能落差（PRD 对照）

- [ ] **项目访问与兼容布局 P0**：无项目时只展示可返回“新建/打开”的空态，不创建任务；restricted 项目可查看已存在的安全静态结果但不能调用外部 Agent、Skill 或 Provider；read-only 项目不能生成或覆盖文件；只有 trusted + writable 项目能生成新导出。准备页和 `start` 命令都要按 canonical folder identity 重验项目、访问级别、可写性、解析后的输出根及 Git 状态；覆盖、删除和批量改写必须先给出受影响路径、检查点状态并显式确认。兼容项目只能写入评估返回的布局目录，不能自行补建根级 `.app/` 或强行迁移到 `exports/html/`。
- [x] **PRD-HTML-001/002/003 P0 导出类型 UI 可发现性**：Workflows preparation 已展示 4 种类型并按类型约束范围；Wiki `GenerateHtmlDialog` 只展示三种单篇类型。卡片缩略/描述/Skill 信息属于 Workflows 的可选 P1 视觉增强，不在 Exports 结果页恢复选择器。
- [ ] **PRD-HTML-001/002/003 失败重试**：现状 `ExportStatus::Failed` 已建模但前端列表不渲染状态徽章也不提供重试 → 目标：失败行显示 `badge--danger` 与 `重试` 按钮（调 `regenerate_export`） → `ExportsView.tsx:204-264`、`exportStore.ts:118-141` → 验收：人为制造一次失败（断网/无 Agent/无 BYOK）后能在 UI 看到红色状态并一键重试成功。
- [x] **完整生成准备与 Wiki 快速例外（Workflows/Wiki 边界）**：Exports 与 Workflows 进入同一个 Generate Content 准备页，由其收集内建类型、适用范围、输出位置摘要和高级执行路径；不要新增 Exports 大型 `ExportDialog`。Wiki 文章的单篇快捷动作使用既有 `GenerateHtmlDialog` 直接创建普通 Export task，只创建新文件且不进入 Workflow history。
- [x] **执行路径选择（Workflows 范围）**：完整准备页读取有效默认路径并允许本次显式覆盖，所选路径不可用时由准备/启动合同阻止；Wiki quick export 作为低摩擦例外继续沿用直接导出的 `auto` 路线。
- [ ] **自定义模板选择（已延期）**：首版只使用内建输出类型及其内建 Skill/template，不增加用户模板、任意模板内容或自定义运行指令。
- [ ] **文件信息面板**：现状右侧只有 iframe → 目标：设计稿四段（chrome + dl + 操作 + 模板说明） → `ExportsView.tsx:273-291` → 验收：选中记录后 dl 显示路径/类型/大小/源/时间/Skill；操作列含 5 个按钮。
- [ ] **外部浏览器打开**：现状 iframe 预览是唯一预览入口 → 目标：新增"在外部浏览器打开"（`open` 协议或 shell 命令，走后端） → `export_commands.rs`（新 `open_export_external`） → 验收：默认浏览器打开 HTML。
- [ ] **删除导出**：现状无删除入口，原生 `.app/exports.json` 只增不减 → 目标：操作列“删除”调后端，删除 layout-defined artifact + record（注意 Git 检查点硬约束） → `export_commands.rs`、`export_service.rs` → 验收：删除后文件与记录同步消失；高风险操作走 `ConfirmationDialog`。
- [ ] **复制路径**：现状无 → 目标：操作列"复制路径"把绝对路径写剪贴板 → `ExportsView.tsx` → 验收：剪贴板含项目根 + 相对路径。
- [ ] **标题计数 + 状态栏统计**：现状无 → 目标：主标题带 `· N 个已生成`；状态栏显示 layout-defined export root（原生示例 `exports/html/`）及 `N 个文件 · NB`、`M 个失败` → `ExportsView.tsx`、`AppShell.tsx` → 验收：路径来自后端 layout，数字随列表实时更新。

## 3. 视觉 / 设计 token 落差

- **字号/密度对齐**：Exports 结果页继续遵循 10.5px section label 与紧凑表格密度；生成类型控件已经迁入 Workflows，不应为追随旧 `dlg-export` 设计而在 Exports 恢复 segmented control 或卡片墙。
- **生成类型视觉**：若后续为四种完整类型增加缩略、描述或 Skill 信息，应在 Workflows preparation 中保持紧凑；Wiki `GenerateHtmlDialog` 的三张单篇模板卡继续遵循既有 820px modal 密度。
- **preview-frame 浏览器 chrome**：设计稿用 28px 高 surface 条 + 3 个 8px 红黄绿圆点 + mono 字体文件名 + 刷新图标，当前右侧 pane 头是扁平 44px 条。差距明显。
- **表格样式**：设计稿用 `.panel + .table-wrap + .table` 类（`app.css` 提供），当前是 `divide-y` 列表。如要严格对齐应引入 `Panel`/`Table` 组件，或至少对齐列宽、行高、badge 样式（`badge--success`/`badge--danger`/`badge--accent`）。
- **danger / warning 色**：失败行图标/徽章应用 `var(--danger)` / `var(--warning)`，当前未使用。
- **badge**：设计稿类型列 `badge`（beautiful-read/knowledge-card/concept-map/project-report）、执行路径列 `badge--accent`（claude）、状态列 `badge--success/danger`；当前只有类型小标签且样式自定义，未复用 badge 系统。

## 4. 交互 / 可访问性落差

- **键盘可达性**：Workflows 类型 selector 必须有可访问 label，进入准备页后管理初始焦点与返回焦点；Wiki 模板卡使用 `aria-pressed` 并由 modal 管理焦点；Exports 表格操作按钮继续保留 `aria-label`。
- **加载态**：`loading` 只在列表底部加一行小字（`ExportsView.tsx:265-269`），设计稿无骨架但应有可见 spinner/禁用态。
- **错误反馈**：`error` 显示为顶栏黄色横条（`ExportsView.tsx:193-197`），但 i18n 文案为英文硬编码错误信息（后端返回的 message）。建议映射错误码到 i18n key。
- **取消任务反馈**：取消按钮触发后无 toast/横幅；任务终态依赖 `TaskLogDrawer`。建议接入 toast（设计稿 `window.__toast`）。
- **生成入口 a11y**：完整 Generate Content 使用共享 Workflows preparation；进入后设置语义标题与初始焦点，返回 Exports 时恢复触发点，不新增 Exports `ExportDialog`。Wiki `GenerateHtmlDialog` 需独立满足 modal focus trap、Escape/cancel、触发点恢复和中英文三种单篇类型可读性。
- **空态文案**：`exports.list.empty` 已有 i18n，但设计稿主标题副文案 `· N 个已生成` 在空态应省略。
- **预览 iframe title**：`HtmlPreviewPane.tsx:27` 用硬编码英文 `"export-preview"`，应 i18n 化且描述当前文件。

## 5. 建议实施顺序

1. **P0 项目访问合同与兼容布局**：统一 `prepare/start` 的 ProjectContext 重验、restricted/read-only/trusted-writable 分层、布局解析和 Git 前置条件；先消除固定 `exports/html/` 假设。
2. **P0 已生成列表表格化**：把列表换成 9 列表格（或语义化 row），补状态徽章、失败行重试按钮、大小列、执行路径列、源列。→ 直接解决 PRD-HTML-001/002/003 验收中的"可见失败 + 可重试"。
3. **P0 Generate Content 完整准备页 + Wiki quick exception**：完整路径在 Workflows feature 中实现，不改造现有 Exports 结果页，也不新增 Exports `ExportDialog`；Wiki 专用 `GenerateHtmlDialog` 只保留三种单篇 create-new 快速操作。
4. **P1 Workflows 输出类型视觉增强（可选）**：如需卡片网格，在完整 preparation 中增加紧凑缩略、标题、描述和 Skill 信息；不得在 Exports 结果页恢复第三条生成入口，也不得扩张 Wiki 单篇弹窗的范围。
5. **Deferred 模板选择端到端**：首版不实施用户自定义模板。
6. **P0 执行路径合同**：准备页读取 Settings 默认值并允许单次覆盖；后端不可静默回退。
7. **P1 右侧面板四段式**：补 preview-frame chrome + 文件信息 dl + 5 个操作按钮 + 模板说明。
8. **P1 标题计数 + 状态栏统计**：聚合 `records.length`、失败数、总体积（需后端 `list_exports` 返回 size，或前端 `stat` 补齐）。
9. **P1 外部浏览器打开 + 复制路径**：后端加 `open_export_external`（macOS `open`、Win `start`、Linux `xdg-open`）；复制路径纯前端 `navigator.clipboard`。
10. **P2 删除导出**：后端 `delete_export`（删 HTML + 从 `exports.json` 移除 + Git 检查点 + `ConfirmationDialog` 确认）。
11. **P2 视觉打磨**：引入 `Panel`/`Table`/`Badge` 组件统一 token；preview-frame chrome；i18n iframe title；错误码到文案映射。
