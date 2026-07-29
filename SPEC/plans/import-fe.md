# Import 前端壳 P0+P1 实施账本（/loop 单轮）

> 历史实施账本：旧卡片布局、Git 开关、导入后编译和编译期 OCR 不再是目标。当前 Import / Source 行为只以 [`../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md) 为准。

> ✅ **本轮完成 @ 2026-06-22** — Import 前端壳 P0+P1 全部 verified。
>
> **摘要**：按 `import.html` 重写 ImportView 为「卡片网格 + 文件表 + 右面板 + 底部确认条」四区；Tab+单行输入退场。新增 `importStore`（zustand）跨组件共享预览/选中/确认/对话框/检查点/编译开关；`RightContextPanel` 增 import 分支（选中文件 / 提取预览 / 归档规则 / 冲突记录）。dlg-url 与 dlg-folder 独立对话框（后者调 `preview_open_folder_as_project` → 既有 pendingAction 流）。底部条暴露「Git 检查点」「导入后编译」checkbox 并透传到 `confirm_import_preview.createCheckpoint` + 条件编译。拖拽走 `getCurrentWebview().onDragDropEvent`（无需 dialog 插件）。i18n en/zh-CN 全量补齐。
>
> **验证**：`npm run test` 20 file / 90 test 全绿；`npm run lint` 零警告；无 `console.log`；浏览器预览 6 卡片 + 表格 + 右面板 + URL 对话框均渲染正常。
>
> **文件清单**：
> - `src/stores/importStore.ts`（新）
> - `src/features/import/ImportView.tsx`（重写）
> - `src/features/import/ImportUrlDialog.tsx`（新）
> - `src/features/import/OpenFolderAsProjectDialog.tsx`（新）
> - `src/features/import/ImportRightPanel.tsx`（新）
> - `src/components/app/RightContextPanel.tsx`（增 import 分支）
> - `src/components/app/AppShell.tsx`（接线 + confirm opts）
> - `src/types/import.ts`（`createCheckpoint`）
> - `src/styles.css`（import/table 样式段）
> - `src/i18n/locales/{en,zh-CN}.json`（import.* 键）
> - `src/app/App.test.tsx`（适配新 UI）
> - `SPEC/plans/import-fe.md`（账本）

> 对照源：`SPEC/roadmap/import.md`、`SPEC/PRD.md` PRD-IMP-001/003/004/005/006、`UI-Frontend-design/import.html` + `assets/app.css`、`CLAUDE.md`
> 范围：只动 `src/` 与 `src/styles.css`。后端缺口标 blocked，不动 `src-tauri/`。

## 决策日志

- **状态载体**：新建 `src/stores/importStore.ts`（zustand）持有 `preview / selectedFileId / importedSources / isConfirming / urlDialogOpen / folderDialogOpen / createCheckpoint / compileAfterImport` 等。理由：右侧解析预览面板（`RightContextPanel`）与主区 `ImportView` 需共享选中文件与预览；CLAUDE.md 要求跨组件状态走 Zustand。`WorkspaceView` 的处理函数仍保留在 AppShell，但改为读写 store。
- **右侧面板**：在 `RightContextPanel` 增加 `activeView === "import"` 分支，渲染「选中文件 / 提取预览 / 归档规则 / 冲突记录」四段，匹配设计稿 `rightpanel`。导入历史（P2）本轮不做。
- **原生文件选择对话框**：**blocked**。需要 `tauri-plugin-dialog`（需改 `Cargo.toml` + `package.json` + capabilities），超出「只动 src/」边界。本轮用：①路径多行粘贴（textarea，一行一个）；②Tauri 窗口 `onDragDropEvent` 拖拽落点（无需插件，纯 `@tauri-apps/api/window` 事件）。
- **PDF/Office 实际解析**：**blocked**。后端 `ExtractionService` 对 pdf/docx/pptx/xlsx 返回 `Unsupported`，需改 `src-tauri/`。本轮前端正确消费并展示 `unsupported` 状态、语言、created/modified；当后端补齐适配器后字段自动显示。
- **imageCount / tableCount**：**blocked**。后端 `SourceMetadata`（`src-tauri/src/models/import.rs:69-77`）无此字段，TS 模型不加空字段以免误导。设计稿「图片 / 表格」暂以「—」占位。
- **`create_checkpoint` 参数**：后端 `ConfirmImportRequest` 已支持（`commands/import_commands.rs:28-38`）。TS `ConfirmImportRequest` 增 `createCheckpoint?: boolean`；底部条 checkbox 默认勾选；`onConfirm` 透传。
- **「导入后编译」**：底部条 checkbox 默认勾选；`AppShell.confirmImportPreview` 已在成功后调 `startCompile()`——改为根据 store 中 `compileAfterImport` 条件触发。
- **dlg-folder**：后端 `preview_open_folder_as_project` 已存在（`commands/project_commands.rs:108-114`），返回 `OpenProjectResponse`；`NeedsConfirmation` 走既有 `setPendingAction` + `ConfirmationDialog` 流。对话框 UI 按设计稿画；模板/归档策略 checkbox 展示，但实际由后端 execution plan 决定（前端不伪造参数）。
- **提交策略**：各项高度耦合（重写 ImportView 同时牵动卡片/表格/底部条/右面板），按关注点拆 2 个 commit：①store+types+i18n+styles+dialog 子组件；②ImportView 重写 + RightContextPanel 分支 + AppShell 接线。

## 项目清单

| ID | 优先级 | 内容 | status | 文件:行号 |
|---|---|---|---|---|
| FE-01 | P0 | 新建 importStore（preview/选中/确认态/dialog 开关/编译&检查点开关） | verified | `src/stores/importStore.ts:25` |
| FE-02 | P0 | styles.css 增 `.import-layout/.import-section/.import-grid/.import-source/.file-table-wrap/.import-actions/.table/.col-*/.table-wrap` 等 | verified | `src/styles.css:1076-1171` |
| FE-03 | P0 | ImportView 重写：卡片网格（本地文件/文件夹/URL/剪贴板 + 禁用项）+ 路径 textarea + 拖拽落点 | verified | `src/features/import/ImportView.tsx:96` |
| FE-04 | P0 | 底部确认条：无损保留文案 + Git 检查点 checkbox + 导入后编译 checkbox + 取消/确认 | verified | `src/features/import/ImportView.tsx:430-462` |
| FE-05 | P0 | 文件表：checkbox/类型图标/文件名+摘要/类型徽章/大小/页数字数/目标路径/状态/预览；聚合徽章 | verified | `src/features/import/ImportView.tsx:345-427` |
| FE-06 | P0 | dlg-url：独立对话框（链接输入 + Readability 提示 + 抓取并预览） | verified | `src/features/import/ImportUrlDialog.tsx:11` |
| FE-07 | P0 | dlg-folder：「打开文件夹为项目」对话框（路径输入 + 警告 + 模板 select + 归档策略 checkbox + 初始化按钮），调 `preview_open_folder_as_project` | verified | `src/features/import/OpenFolderAsProjectDialog.tsx:24` |
| FE-08 | P0 | RightContextPanel 增 import 分支：选中文件/提取预览/归档规则/冲突记录 | verified | `src/features/import/ImportRightPanel.tsx:55`、`src/components/app/RightContextPanel.tsx:143` |
| FE-09 | P0 | AppShell 接线：`createCheckpoint` 透传；`compileAfterImport` 条件编译；dlg 开关由 store 驱动 | verified | `src/components/app/AppShell.tsx:377-398` |
| FE-10 | P1 | 消费解析结果：渲染 `language/created/modified`、`extractedAssets` 计数；unsupported 状态清晰展示 + OCR 委托文案 | verified | `src/features/import/ImportRightPanel.tsx:104-176` |
| FE-11 | P1 | i18n 补齐（en + zh-CN）：来源卡片标题/描述、底部条文案、对话框、归档规则、冲突记录空态等 | verified | `src/i18n/locales/en.json`、`src/i18n/locales/zh-CN.json`（`import.*` 段） |
| FE-12 | P0 | 拖拽：`getCurrentWebview().onDragDropEvent` 监听 paths → `onRequestPreview` | verified | `src/features/import/ImportView.tsx:140-163` |

## BLOCKED（后端 / 依赖，记 roadmap 不动手）

- **原生多选文件选择器**：需 `tauri-plugin-dialog`（Cargo.toml + package.json + capabilities）。本轮以路径粘贴 + 拖拽替代。
- **PDF/DOCX/PPTX/XLSX 文本/页数/字数提取**：后端 `ExtractionService` 返回 `Unsupported`，需引入解析 crate/CLI。前端已就绪待消费。
- **imageCount / tableCount**：后端 `SourceMetadata` 模型缺字段。
- **URL 图片落盘 + Markdown 链接改写**：需后端 `fetch_import_url`/Readability 阶段下载图片。本轮不做。
- **导入历史**：P2，本轮不做。

## 收敛判据

全部 FE-01..FE-12 = verified（test+lint 全绿，无 console.log）+ 账本顶部标记 + progress.txt 里程碑 + 不再调度。blocked 项已单列。
