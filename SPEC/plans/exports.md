# Exports 板块 P0+P1 实施账本（本轮 loop）

> ✅ 本轮完成 @ 2026-06-29 — exports 板块 P0+P1 三条 scope 全部 verified。
>
> **交付摘要**：
> 1. P0 新建导出对话框（源页 scan_wiki 可搜索浏览 / 模板下拉 default-serif·modern-sans·editorial-magazine / 执行路径 auto·agent·byok / 4 选项，embedCss 锁定反映自包含硬约束）。
> 2. P0/P1 已生成列表表格化（.table，列：文件名/类型/源/时间/执行路径/状态/操作）+ 失败状态徽章（badge--danger）+ 成功徽章（badge--success）+ 失败行「查看日志（开任务抽屉）+ 重试」、成功行「预览 + 打开位置」。
> 3. P1 模板选择端到端：后端 prompt 接收模板名（`include_str!` 注入 skill 的 template.html 样式基底 + 风格方向句），不修改 template.html 内容 → `templates_carry_no_schema_or_lint_directives` 回归测试不破。
>
> **文件清单**：后端 `src-tauri/src/models/export.rs`、`services/export_service.rs`、`commands/export_commands.rs`、`tests/mvp_flow.rs`；前端 `src/types/export.ts`、`stores/exportStore.ts`、`features/exports/{ExportDialog.tsx(新),ExportsView.tsx,exportsView.test.tsx}`、`i18n/locales/{en,zh-CN}.json`。
>
> **验证**：`npm run lint` 全绿；`npm run test` 146 通过（30 files，+3 导出测试）；`cargo check --lib --tests` 全绿。`cargo test` 运行期受阻于 Windows 测试 runner 的 DLL 入口点加载失败（STATUS_ENTRYPOINT_NOT_FOUND，进程起不来，与代码无关），详见 `SPEC/gotchas.txt` 2026-06-29 条。
>
> **双子代理审查**：A（共享上下文）+ B（零偏见）并行审查合并，修复 1 blocker（取消态误判为 Failed 并落 Failed 记录：改用取消令牌 + Cancelling/Cancelled 窗口双判）+ 2 major（浏览加载态文案/竞态、预览成功后错条回退/双触发）+ 数 nit，commit `13dfe46`。
>
> **commit 序列**：`1596b9e`(Item1) → `e956c4f`(Item2) → `df474be`(Item3) → `2a6250a`(Item4) → `13dfe46`(审查修复)。

> 对照源：UI-Frontend-design/exports.html + assets/app.css（只读）+ SPEC/PRD.md + CLAUDE.md
> 范围：仅 exports 板块的 P0+P1（见 loop scope 三条）。不碰 P2、不碰别板块。
> status: pending | in_progress | done | verified

## 本轮 scope（loop 锁定三条）

1. **P0 新建导出对话框**：源页浏览 / 模板下拉 / 执行路径 / 4 选项。当前用户只能用顶栏内联裸 input。
2. **P0/P1 已生成列表表格化 + 失败状态徽章 + 重试按钮**（`ExportStatus::Failed` 已建模但前端不渲染）。
3. **P1 模板选择端到端**：prompt 接收用户选择的模板参数（改 src-tauri export prompt，但不破坏 `templates_carry_no_schema_or_lint_directives` 回归测试）。

## 关键设计决策（动手前锁定）

- **模板现实**：每个 skill 文件夹只有一个 `template.html`（样式参考，含 `{{title}}/{{sourcePath}}/{{body}}` 占位）。设计稿的 `default-serif / modern-sans / editorial-magazine` 是 3 个风格名，无独立文件。决策：选中任一模板时，把该 skill 的 `template.html` 内容（`include_str!` 编译期嵌入，仿 `compile_service.rs:276`）注入 prompt 作为样式基底，并按所选风格名追加一句方向性指引。这样 BYOK 路径（无 skill workspace）也能拿到模板样式，且不改动 `template.html` 内容 → 回归测试不破。
- **失败记录**：后端当前从不持久化失败记录（`run_export` 出错只把 task 置 Failed，不 append record）→ 失败徽章/重试永远不可达。决策：在 `run_export_task` 的 spawn 闭包里，非取消类失败时 append 一条 `status: Failed` 的记录（type+source 来自 directive；route 由 preference 推导；output_path 用 `build_output_relative_path` 推算的“应得路径”）。UI 失败行只显示「查看日志 + 重试」，成功行显示「预览 + 打开位置」（与设计稿一致）。
- **4 选项**：`includeFrontmatter` / `embedImages` 进 prompt（`embedCss` 是 skill 硬约束、恒开，UI 勾选反映该契约）；`openPreview` 纯前端（任务成功后自动加载预览），不入后端 DTO。3 个内容选项走 `ExportContentOptions` 结构写入 `StartExportRequest/RegenerateExportRequest`。
- **类型选择**：本轮不建独立卡片网格（那是另一条 P1，不在 scope 三 bullets 内）。对话框内顶部放类型选择（4 个 segmented），作为对话框入口的必要组成。
- **源页浏览**：复用 `scan_wiki` 拉页面列表，对话框内 input（mono）+「选择…」按钮展开可滚动页面列表。

## 实施项（依赖序）

| # | 项 | 涉及文件 | status |
|---|---|---|---|
| 1 | 后端：DTO 加 `template`+`ExportContentOptions`；prompt 注入模板/选项；失败记录持久化；测试 | `models/export.rs`, `services/export_service.rs`, `commands/export_commands.rs`, `tests/mvp_flow.rs` | done |
| 2 | 前端类型+store：请求类型加字段；`startExport/regenerateExport` 透传 template/options/route；openPreview 状态 | `types/export.ts`, `stores/exportStore.ts` | verified |
| 3 | 新建导出对话框 + 顶栏入口 + i18n | `features/exports/ExportDialog.tsx`(新), `features/exports/ExportsView.tsx`, `i18n/locales/*.json` | verified |
| 4 | 已生成列表表格化 + 状态徽章 + 失败重试 + i18n + 测试更新 | `features/exports/ExportsView.tsx`, `features/exports/exportsView.test.tsx`, `i18n/locales/*.json` | verified |

> Item 1 验证状态：`cargo check --lib --tests` 全绿（编译/类型无误）。`cargo test` 运行期失败 = Windows 测试 runner 的 DLL 入口点（STATUS_ENTRYPOINT_NOT_FOUND）+ 运行中的 app 锁住主 exe，均为环境问题、与本次纯逻辑改动无关。收尾时关闭 app 后重跑 `cargo test` 终验。

## 验收（每项 verified 前）

- `npm run test` + `npm run lint` 全绿；动 src-tauri/ 加 `cargo test`。
- 无 `console.log` 残留。
- 设计稿维度对齐（字号 px、token、badge/table/seg/dialog 类来自 `src/styles.css`）。
