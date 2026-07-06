# 04 Export Workflow and Preview Specification

本规范对应原始清单条目：导出页面。该条目包含四个子需求：预览页面可放大、可在浏览器中预览、左侧导出结果条目可单击预览、预览/打开文件夹按钮应靠近文件名右侧。

## 条目 A：导出页面预览与记录列表交互优化

## 1. 需求概述
- 用户想要什么：导出页面里，HTML 预览可以最大化到除最左侧栏外的整个工作区，也可以用系统默认浏览器打开；左侧导出记录可单击预览，预览/打开文件夹等按钮放在文件名右侧，避免横向滚动到最右才能操作。
- 为什么：导出产物需要接近真实阅读尺寸来检查排版；导出记录的主动作是预览，操作入口应贴近文件名并易于发现。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/features/exports/ExportsView.tsx`、`src/features/exports/HtmlPreviewPane.tsx`、`src/stores/exportStore.ts`、`src-tauri/src/commands/export_commands.rs`、`src-tauri/src/services/export_service.rs`、`src-tauri/src/models/export.rs`、`src/styles.css`。
- 当前行为是什么：`ExportsView` 已有记录表格、成功/失败 badge、preview/open folder/retry/log actions；右侧 `HtmlPreviewPane` 使用 sandbox iframe 预览；后端有 `read_export_preview` 和 `open_export_folder`，但没有浏览器打开 command；记录行本身不是预览入口，操作列可能在表格最右侧。
- 问题出在哪里：预览最大化如果只隐藏 export list，仍会被全局 `RightContextPanel` 占用宽度；浏览器打开需要后端路径校验；row click 和 inline actions 需要避免事件冒泡；操作布局与用户扫描路径不一致。

## 3. 方案设计
- 第一性原理：导出页面的本质是“生成产物后的检查与取用工作台”。产物检查应有足够空间，产物取用应保证路径安全，列表主动作应是预览。
- 推荐方案：新增 export preview focus mode。`exportStore` 管理导出视图内部 `previewMode`，同时通过 `navigationStore` 暂时折叠右侧面板或进入 shell-level workspace focus mode，确保最大化预览占满除左侧导航栏外的区域。
- 备选方案 1：只在 `ExportsView` 隐藏列表 pane。实现快，但全局右侧面板仍占宽，不满足用户要求，不采用。
- 备选方案 2：进入系统全屏。空间最大，但隐藏顶栏/状态栏和项目安全上下文，不符合 Codex-like 工作台，不采用。
- 技术方案：
  - 修改 `src/stores/exportStore.ts`：
    - `export type ExportPreviewMode = "split" | "maximized"`
    - `previewMode: ExportPreviewMode`
    - `setPreviewMode(mode: ExportPreviewMode): void`
    - `openInBrowser(request: OpenExportInBrowserRequest): Promise<void>`
  - 修改 `src/stores/navigationStore.ts`：
    - 推荐新增 `workspaceFocus: null | "exportPreview"`
    - `setWorkspaceFocus(focus: WorkspaceFocus): void`
    - 当 `workspaceFocus === "exportPreview"` 时，`AppShell` 折叠 `RightContextPanel`；退出 focus 时恢复进入前的 `rightPanelOpen`。
  - 修改 `src/components/app/AppShell.tsx`：
    - 根据 `workspaceFocus` 添加 `.app-shell.is-workspace-focused`。
    - focus mode 保留左侧栏、顶栏、底部状态栏，仅隐藏右侧上下文面板。
  - 修改 `src/features/exports/HtmlPreviewPane.tsx` props：
    - `previewMode: ExportPreviewMode`
    - `onTogglePreviewMode(): void`
    - `onOpenInBrowser(): void`
    - toolbar 增加 `Maximize2` / `Minimize2` 和 `ExternalLink` icon buttons。
  - 修改 `src/features/exports/ExportsView.tsx`：
    - className 增加 `.exports-view-layout--preview-maximized`。
    - maximized 时列表 pane `display: none`，预览 pane 占据 center workspace。
    - 成功 row `onClick={() => preview(record)}`，并支持 Enter/Space。
    - 文件名 cell 内部结构为 title + inline action group；inline buttons 使用 `event.stopPropagation()`。
    - 当前 `previewId` 行增加 selected state。
  - 修改 `src-tauri/src/models/export.rs`：
    - `pub struct OpenExportInBrowserRequest { pub project_id: String, pub project_root_path: String, pub output_path: String }`
  - 修改 `src-tauri/src/commands/export_commands.rs`：
    - `pub fn open_export_in_browser(request: OpenExportInBrowserRequest, state: State<'_, AppState>) -> Result<(), BackendError>`
    - 与 `read_export_preview` 共用安全解析：必须在当前项目 `exports/html/` 下，扩展名必须是 `.html`。
    - 新增或复用平台 opener helper，优先使用直接 argv 调用：Windows 用 `explorer.exe`/系统 file association 的 direct process strategy，macOS 用 `open`，Linux 用 `xdg-open`。避免拼接 shell 字符串，避免 `cmd /C start`、批处理和 PowerShell 字符串 quoting。
  - 修改 `src-tauri/src/lib.rs` 注册 `open_export_in_browser`。
  - 修改 `src/types/export.ts`，新增 `OpenExportInBrowserRequest`。
  - 修改 `src/styles.css`：
    - `.exports-view-layout--preview-maximized`
    - `.exports-row--selected`
    - `.exports-filecell`
    - `.exports-inline-actions`
- 需要新增哪些文件：不强制新增；可选抽 `src-tauri/src/utils/open_path.rs` 复用浏览器打开与文件夹 reveal。
- 需要修改哪些文件：`src/stores/exportStore.ts`、`src/stores/navigationStore.ts`、`src/components/app/AppShell.tsx`、`src/features/exports/ExportsView.tsx`、`src/features/exports/HtmlPreviewPane.tsx`、`src-tauri/src/models/export.rs`、`src-tauri/src/commands/export_commands.rs`、`src-tauri/src/lib.rs`、`src/types/export.ts`、`src/styles.css`、`src/i18n/locales/en.json`、`src/i18n/locales/zh-CN.json`。
- 是否需要新增依赖：不需要；不引入 `@tauri-apps/plugin-shell` 或新的 Rust crate。

## 4. UI / 交互设计
- 界面变化描述：预览 toolbar 上增加最大化/还原、在浏览器中打开两个 icon-only buttons；导出列表文件名单元格右侧始终放置小眼睛、外链浏览器、文件夹三个 28px icon；当前预览行有 selected 背景。
- 交互流程：用户点击成功导出行 -> 右侧打开 iframe 预览 -> 点击最大化 -> AppShell 进入 `exportPreview` focus mode，右侧上下文面板折叠，列表隐藏，预览占据除左侧栏外的工作区 -> 点击还原或 Esc -> 恢复列表和右侧面板原状态。
- 交互流程：用户点击浏览器按钮 -> 后端校验 `outputPath` -> 系统默认浏览器打开 HTML 文件 -> 失败时 toast 显示错误，并保留“打开文件夹”作为备选。
- 交互流程：用户点击文件名右侧 icon -> 执行对应动作并阻止 row click 冒泡；用户键盘聚焦 row 后按 Enter/Space -> 预览该记录。
- 需要参考的设计规范：`Spec/FRONTEND_GUIDELINES.md` 的 Exports、Tables and Lists、icon-only button tooltip 规范；`Spec/BACKEND_STRUCTURE.md` 的 ProjectContext 路径安全边界；`UI-Frontend-design/exports.html` 的预览结构。

## 5. 验收标准（Done Definition）
- [ ] 成功导出记录单击行即可在预览 pane 打开。
- [ ] 文件名右侧无需横向滚动即可看到 preview/open browser/open folder。
- [ ] maximized 模式保留左侧全局导航、顶栏和状态栏，并折叠全局右侧上下文面板。
- [ ] 按 Esc 或点击还原按钮可恢复 split 模式，并恢复进入最大化前的右侧面板开关状态。
- [ ] 成功导出记录可在系统默认浏览器打开。
- [ ] 非 `exports/html/*.html` 路径、越界路径、被删除文件被后端拒绝并给出错误。
- [ ] 失败记录不会尝试读取 preview，只展示日志和重试。
- [ ] 行点击和 icon 点击不会重复触发。
- [ ] 中英 tooltip、aria-label、键盘操作完整。

## 6. 风险与注意事项
- 可能影响的现有功能：`AppShell` 的全局右侧面板折叠要保存进入前状态，不能永久改变用户偏好；导出 preview focus mode 不应影响 Wiki HTML preview。
- 可能影响的现有功能：表格 row click 与按钮 click 容易重复触发，必须用 `stopPropagation` 并补测试。
- 可能影响的现有功能：平台 opener 要避免 shell 字符串拼接，尤其 Windows 路径含空格、中文、`&`、`%` 时不能走 batch quoting。
- 边界情况：无 previewHtml、导出失败记录、文件被外部删除、HTML 文件名含 CJK、默认浏览器未配置、窄窗口下 iframe 双重滚动。

## 7. 实施步骤
- [ ] 写后端路径校验测试：合法 html、越界路径、非 html、缺失文件。
- [ ] 实现 `open_export_in_browser` 和 direct-argv platform opener helper。
- [ ] 扩展 `exportStore` previewMode/openInBrowser，并写 store 测试。
- [ ] 扩展 `navigationStore` workspaceFocus，并写进入/退出恢复右侧面板状态测试。
- [ ] 修改 `AppShell` 支持 exportPreview focus mode。
- [ ] 修改 `HtmlPreviewPane` toolbar。
- [ ] 修改 `ExportsView` 行点击、inline action group、selected state。
- [ ] 更新 CSS、i18n 和 ExportsView 交互测试。
