# Exports 板块落差与实施计划

> 对照源：UI-Frontend-design/exports.html + assets/app.css + SPEC/PRD.md（§9.10 HTML/卡片/报告）
> 当前实现：src/features/exports/、src/stores/exportStore.ts、src-tauri/src/services/export_service.rs、src-tauri/src/commands/export_commands.rs

## 0. 现状摘要

Exports 的**后端链路已完整落地**：4 种导出类型（beautiful_read / knowledge_card / concept_map / project_report）均有 `skills/html-*` Skill 驱动（位于 `src-tauri/templates/skills/html-*/SKILL.md` + `template.html`），`ExportService` 负责 prompt 组装、输出路径推导、HTML 提取（去 ```html 围栏、删尾部 prose）、记录持久化到 `.app/exports.json`，并强制所有输出落在 `exports/html/` 下。`export_commands.rs` 把导出跑成可取消的后端 Task，支持 Agent 优先 / BYOK 兜底路由、取消信号轮询、HTML 合法性校验、跨平台 reveal-in-file-manager。

**前端已能跑通主链路**：类型选择（SegmentedControl）、源页路径输入、生成/取消、记录列表（标题/类型/路径/时间戳/执行路径）、预览（sandbox iframe，`srcDoc` + `sandbox=""`）、重新生成、打开所在位置。`ExportsView.tsx` 监听终态任务自动刷新列表。

**主要落差集中在 UI 层与设计稿的偏差**：
1. 设计稿的"卡片式输出类型选择 + 缩略示意图"被压扁成一个 segmented control；设计稿的 `dlg-export` 新建导出对话框（源页选择器、模板下拉、输出位置、执行路径 segmented、4 个选项 checkbox）**完全缺失**。
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
| 工具栏：新建导出 | 顶栏 primary 按钮 `+ 新建导出` 触发对话框 | 用顶栏内联的 Generate 按钮替代 | 🟡部分实现 | P1 | `ExportsView.tsx:182-190` |
| 输出类型选择：卡片网格 | 4 张大卡片，每张含 SVG 缩略图、标题、描述、skill 名链接、选中态 `is-selected`；`html-project-report · 缺模板 →` 标记缺模板 | 压扁为 4 个 segmented 按钮，无缩略图/描述/skill 名/缺模板标记 | 🟡部分实现 | P1 | `ExportsView.tsx:144-162` |
| 源页选择器 | 对话框内 input-group + 选择…按钮 | 顶栏裸 input（无浏览/校验） | 🟡部分实现 | P1 | `ExportsView.tsx:163-171` |
| 模板下拉 | `default-serif / modern-sans / editorial-magazine` 3 选项 | 完全缺失（prompt 不传模板参数） | ❌缺失 | P1 | `src-tauri/src/services/export_service.rs:31-120`，`src/types/export.ts:24-32` |
| 输出位置 | 对话框内 input-group，默认 `exports/html/<slug>.html` | 后端自动生成，UI 不展示 | 🟡部分实现 | P2 | `export_service.rs:157-188` |
| 执行路径 segmented | `claude · 推荐 / BYOK · Anthropic` | 顶栏无此控件；route 写死 `auto` | ❌缺失 | P1 | `exportStore.ts:27`（`ROUTE_PREFERENCE = "auto"`） |
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
| 导出对话框 | `dlg-export` 全屏对话框 | 无 | ❌缺失 | P0 | — |
| 导出历史持久化 | `.app/exports.json` 新到旧 | 后端已实现并单测 | ✅已完成 | — | `export_service.rs:237-257` |
| skills/html-* 接入 | 4 个 skill 文件夹 + 模板 | 全部存在；`templates_carry_no_schema_or_lint_directives` 测试锁住"模板只影响样式"硬约束 | ✅已完成 | — | `src-tauri/templates/skills/html-*/SKILL.md`、`template.html`、`export_service.rs:604-628` |
| 路径安全 | 输出只能在 `exports/html/` | 三重校验（service / command / ProjectContext），单测覆盖逃逸 | ✅已完成 | — | `export_service.rs:179-187,223-230`，`export_commands.rs:328-335,347-355` |
| 跨平台打开位置 | explorer / open -R / xdg-open | 三平台分支已实现 | ✅已完成 | — | `export_commands.rs:361-382` |

## 2. 功能落差（PRD 对照）

- [ ] **PRD-HTML-001/002/003 P0 三种导出类型 UI 可发现性**：现状 segmented 按钮把 4 种类型平铺，但设计稿的卡片含缩略图/描述/skill 名/缺模板提示信息密度丢失 → 目标：按设计稿重做输出类型卡片网格 → `ExportsView.tsx:144-162` → 验收：4 张卡片可见 title/desc/skill folder，project-report 在无模板时标 `缺模板`。
- [ ] **PRD-HTML-001/002/003 失败重试**：现状 `ExportStatus::Failed` 已建模但前端列表不渲染状态徽章也不提供重试 → 目标：失败行显示 `badge--danger` 与 `重试` 按钮（调 `regenerate_export`） → `ExportsView.tsx:204-264`、`exportStore.ts:118-141` → 验收：人为制造一次失败（断网/无 Agent/无 BYOK）后能在 UI 看到红色状态并一键重试成功。
- [ ] **导出对话框**：现状 Generate 直接发任务，设计稿的源页/模板/输出位置/执行路径/4 选项均无入口 → 目标：新建 `ExportDialog`（参照已有 `ConfirmationDialog`/`CompileConflictDialog` 模式） → `src/features/exports/ExportsView.tsx`、`src/components/app/` → 验收：点 "新建导出" 弹对话框；源页支持浏览（复用 Wiki 页面选择器）；4 个选项 checkbox 生效（写入 `StartExportRequest`）。
- [ ] **执行路径选择**：现状 `ROUTE_PREFERENCE = "auto"` 写死 → 目标：对话框内 segmented `claude · 推荐 / BYOK · Anthropic`，值映射到 `ExportRoutePreference` → `exportStore.ts:27`、`ExportsView.tsx` → 验收：用户强制 BYOK 时不走 Agent。
- [ ] **模板选择**（PRD-HTML-005 P1）：现状 `template.html` 存在但 prompt 不让用户选 → 目标：对话框下拉 + 把所选模板内容拼进 prompt（skill 本身不变） → `export_service.rs:31-120`、`src/types/export.ts:24-32`（加 `template?: string`） → 验收：3 个模板能产生视觉差异；模板内容只含样式（已有回归测试 `templates_carry_no_schema_or_lint_directives`）。
- [ ] **文件信息面板**：现状右侧只有 iframe → 目标：设计稿四段（chrome + dl + 操作 + 模板说明） → `ExportsView.tsx:273-291` → 验收：选中记录后 dl 显示路径/类型/大小/源/时间/Skill；操作列含 5 个按钮。
- [ ] **外部浏览器打开**：现状 iframe 预览是唯一预览入口 → 目标：新增"在外部浏览器打开"（`open` 协议或 shell 命令，走后端） → `export_commands.rs`（新 `open_export_external`） → 验收：默认浏览器打开 HTML。
- [ ] **删除导出**：现状无删除入口，`.app/exports.json` 只增不减 → 目标：操作列"删除"调后端，删 HTML 文件 + 记录（注意 Git 检查点硬约束） → `export_commands.rs`、`export_service.rs` → 验收：删除后文件与记录同步消失；高风险操作走 `ConfirmationDialog`。
- [ ] **复制路径**：现状无 → 目标：操作列"复制路径"把绝对路径写剪贴板 → `ExportsView.tsx` → 验收：剪贴板含项目根 + 相对路径。
- [ ] **标题计数 + 状态栏统计**：现状无 → 目标：主标题带 `· N 个已生成`；状态栏带 `exports/html/ · N 个文件 · NB`、`M 个失败` → `ExportsView.tsx`、`AppShell.tsx` → 验收：数字随列表实时更新。

## 3. 视觉 / 设计 token 落差

- **字号/密度对齐**：设计稿 section 标签 `11px / letter-spacing 0.08em / uppercase / muted`；当前 `ExportsView.tsx:275-277` 已用 `10.5px` 接近，但顶栏的 segmented 按钮用 `12px` 偏大。卡片网格完全缺失导致 `--sp-*` 间距体系未铺开（`exports-grid gap:12px`，卡片内 `padding:16px`）。
- **export-card 缩略图**：设计稿每张卡内嵌 100x60 SVG 缩略（文章排版/双面卡片/图谱节点/报告目录），当前完全缺失——是 P1 视觉补足项。
- **preview-frame 浏览器 chrome**：设计稿用 28px 高 surface 条 + 3 个 8px 红黄绿圆点 + mono 字体文件名 + 刷新图标，当前右侧 pane 头是扁平 44px 条。差距明显。
- **表格样式**：设计稿用 `.panel + .table-wrap + .table` 类（`app.css` 提供），当前是 `divide-y` 列表。如要严格对齐应引入 `Panel`/`Table` 组件，或至少对齐列宽、行高、badge 样式（`badge--success`/`badge--danger`/`badge--accent`）。
- **danger / warning 色**：失败行图标/徽章应用 `var(--danger)` / `var(--warning)`，当前未使用。
- **badge**：设计稿类型列 `badge`（beautiful-read/knowledge-card/concept-map/project-report）、执行路径列 `badge--accent`（claude）、状态列 `badge--success/danger`；当前只有类型小标签且样式自定义，未复用 badge 系统。

## 4. 交互 / 可访问性落差

- **键盘可达性**：segmented 按钮和列表项无 `role`/`aria-pressed`；对话框缺失后也缺 focus trap。建议：类型卡片用 `role="radio"` + `aria-checked`，列表项操作按钮已有 `aria-label`（`IconButton`，`ExportsView.tsx:303-316`）继续保持。
- **加载态**：`loading` 只在列表底部加一行小字（`ExportsView.tsx:265-269`），设计稿无骨架但应有可见 spinner/禁用态。
- **错误反馈**：`error` 显示为顶栏黄色横条（`ExportsView.tsx:193-197`），但 i18n 文案为英文硬编码错误信息（后端返回的 message）。建议映射错误码到 i18n key。
- **取消任务反馈**：取消按钮触发后无 toast/横幅；任务终态依赖 `TaskLogDrawer`。建议接入 toast（设计稿 `window.__toast`）。
- **对话框 a11y**：新增 `ExportDialog` 必须支持 Esc 关闭、focus trap、`aria-modal`。
- **空态文案**：`exports.list.empty` 已有 i18n，但设计稿主标题副文案 `· N 个已生成` 在空态应省略。
- **预览 iframe title**：`HtmlPreviewPane.tsx:27` 用硬编码英文 `"export-preview"`，应 i18n 化且描述当前文件。

## 5. 建议实施顺序

1. **P0 已生成列表表格化**：把列表换成 9 列表格（或语义化 row），补状态徽章、失败行重试按钮、大小列、执行路径列、源列。→ 直接解决 PRD-HTML-001/002/003 验收中的"可见失败 + 可重试"。
2. **P0 ExportDialog 对话框**：抽出新建导出弹窗（源页浏览/模板下拉/执行路径 segmented/4 选项 checkbox/输出位置只读预览），替换当前顶栏内联 Generate。复用 `ConfirmationDialog`/`CompileConflictDialog` 的 dialog 模式。
3. **P1 输出类型卡片网格**：按设计稿重做 4 张卡片（SVG 缩略 + 标题 + 描述 + skill folder + 缺模板标记），点击进入对话框。
4. **P1 模板选择端到端**：`StartExportRequest` 加 `template`，`ExportService::build_export_prompt` 拼 `template.html` 内容到 prompt；`html-project-report` 无 template 时卡片标 `缺模板`（设计稿已示意）。
5. **P1 执行路径 segmented**：store 放下 `ROUTE_PREFERENCE`，对话框内可选；后端 `resolve_route` 已支持。
6. **P1 右侧面板四段式**：补 preview-frame chrome + 文件信息 dl + 5 个操作按钮 + 模板说明。
7. **P1 标题计数 + 状态栏统计**：聚合 `records.length`、失败数、总体积（需后端 `list_exports` 返回 size，或前端 `stat` 补齐）。
8. **P1 外部浏览器打开 + 复制路径**：后端加 `open_export_external`（macOS `open`、Win `start`、Linux `xdg-open`）；复制路径纯前端 `navigator.clipboard`。
9. **P2 删除导出**：后端 `delete_export`（删 HTML + 从 `exports.json` 移除 + Git 检查点 + `ConfirmationDialog` 确认）。
10. **P2 视觉打磨**：引入 `Panel`/`Table`/`Badge` 组件统一 token；preview-frame chrome；i18n iframe title；错误码到文案映射。
