# 01 Workbench Shell, Layout, and Theme Specification

本规范整合以下条目：可拖动分割线、左侧主视图收起、顶栏项目展示与选择、配色与 Markdown 渲染主题自定义。

## 条目 A：添加可以自由拖动分割线的功能

## 1. 需求概述
- 用户想要什么：用户可以拖动工作台里的面板分割线，自由调整左侧栏、中间工作区、右侧上下文面板，以及 Wiki/Export/Lint 等视图内部列表与详情区的宽度。
- 为什么：不同项目的文件名、路径、预览内容和图谱详情长度差异很大，固定宽度会导致信息被截断，降低桌面工作台效率。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/components/app/AppShell.tsx`、`src/styles.css`、`src/stores/navigationStore.ts`、`src/components/app/LeftSidebar.tsx`、`src/components/app/RightContextPanel.tsx`。
- 当前行为是什么：`src/styles.css` 中 `.app-shell__workbench` 使用固定 `grid-template-columns: var(--sidebar-w) minmax(0, 1fr) var(--rightpanel-w)`；右侧面板只有 `rightPanelOpen` 开关，左侧栏宽度和内部视图宽度不可调。
- 问题出在哪里：布局尺寸是 CSS token 固定值，缺少统一 pane 状态模型、拖动手柄组件、键盘可达逻辑和持久化。

## 3. 方案设计
- 第一性原理：分割线的本质不是视觉装饰，而是“用户对工作空间注意力分配的可保存偏好”。实现应以 pane 尺寸状态为源，而不是让 DOM 临时改 style。
- 推荐方案：新增通用 `ResizableSplitter` + `useResizablePane`，将全局 shell 宽度和局部视图宽度都接入 `navigationStore`，用 CSS 变量驱动 grid。持久化使用前端 UI 偏好层 `localStorage`，key 为 `llm-wiki-desktop.layout.v1`；不写入项目 `.app/settings.json`，避免把个人窗口偏好混入可复制的知识库项目。
- 备选方案 1：直接在 `AppShell.tsx` 中监听 mousemove 并写 inline style。优点是快；缺点是不可复用、键盘/a11y 难补。不要采用。
- 备选方案 2：引入第三方 resizable panes。优点是省代码；缺点是新增依赖且需适配 Tauri/React 19。默认不引入新依赖，不推荐。
- 技术方案：
  - 修改 `src/stores/navigationStore.ts`，扩展状态：
    - `sidebarWidth: number`
    - `rightPanelWidth: number`
    - `wikiTreeWidth: number`
    - `exportsListWidth: number`
    - `lintListWidth: number`
    - `setPaneSize(pane: ResizablePaneId, width: number): void`
    - `resetPaneSize(pane: ResizablePaneId): void`
  - 新增 `src/components/app/ResizableSplitter.tsx`：
    - `export type ResizeAxis = "x"`
    - `export interface ResizableSplitterProps { paneId: ResizablePaneId; label: string; min: number; max: number; value: number; onChange(value: number): void; onReset(): void; }`
  - 新增 `src/hooks/useResizablePane.ts`：
    - `export function clampPaneWidth(value: number, min: number, max: number): number`
    - `export function useResizablePane(options: UseResizablePaneOptions): UseResizablePaneResult`
  - 修改 `src/components/app/AppShell.tsx`，在 left/main 与 main/right 之间渲染 `ResizableSplitter`，并把宽度写入 style CSS variables：`--sidebar-w-current`、`--rightpanel-w-current`。
  - 修改 `src/features/wiki/WikiView.tsx`、`src/features/exports/ExportsView.tsx`、`src/features/lint/LintView.tsx`，局部列表/详情分栏接入同一 split handle。
  - 修改 `src/styles.css`，新增 `.resize-handle`、`.resize-handle[aria-orientation="vertical"]`、hover/focus-visible 状态，并把 shell grid 改为使用 current variables。
- 需要新增哪些文件：`src/components/app/ResizableSplitter.tsx`、`src/hooks/useResizablePane.ts`、`src/hooks/useResizablePane.test.ts`。
- 需要修改哪些文件：`src/stores/navigationStore.ts`、`src/components/app/AppShell.tsx`、`src/features/wiki/WikiView.tsx`、`src/features/exports/ExportsView.tsx`、`src/features/lint/LintView.tsx`、`src/styles.css`、`src/i18n/locales/en.json`、`src/i18n/locales/zh-CN.json`。
- 是否需要新增依赖：不需要。

## 4. UI / 交互设计
- 界面变化描述：分割线默认是 1px 边线，hover/focus 时显示 4px 可拖动热区和浅 teal 高亮；拖动时禁用文本选择，状态栏可显示当前宽度。
- 交互流程：用户按住分割线拖动 -> 对应 pane 宽度实时变化 -> 松开后尺寸保存在前端持久化状态；用户双击分割线 -> 恢复默认宽度；键盘聚焦分割线后按左右方向键 -> 每次调整 12px。
- 需要参考的设计规范：`Spec/FRONTEND_GUIDELINES.md` 的 “Layout Dimensions”“Accessibility”；`UI-Frontend-design/dashboard.html` 和 `UI-Frontend-design/assets/app.css` 的 shell 密度与分栏。

## 5. 验收标准（Done Definition）
- [ ] 左侧栏、中间区、右侧栏可通过鼠标拖动调整宽度，刷新后保持上次宽度。
- [ ] Wiki 文件树、Export 记录列表、Lint issue 列表可独立调整宽度。
- [ ] 所有分割线支持键盘聚焦、左右方向键调整和双击重置。
- [ ] 宽度被 clamp 到合理范围：左侧栏 180-360px，右侧栏 280-520px，局部列表 220-480px。
- [ ] 中文长路径、英文长路径、任务状态变化不会导致布局跳动或文字溢出。

## 6. 风险与注意事项
- 可能影响的现有功能：`AppShell.tsx` 当前聚合了很多业务流程，布局 class 改动可能影响导入、导出、Agent 弹窗和右侧面板。
- 边界情况：小窗口下右侧面板已折叠时不应保留不可见拖动手柄；拖动过程中切换视图应清理 pointer listeners；持久化宽度不得写入项目文件，只能作为 `localStorage` UI 偏好。若未来需要跨设备同步，再单独设计全局 app config，而不是复用项目状态。

## 7. 实施步骤
- [ ] 为 `clampPaneWidth` 写单元测试，覆盖最小值、最大值、NaN、负数。
- [ ] 在 `navigationStore.ts` 加 pane size 状态和 setter，并补 store 测试。
- [ ] 实现 `ResizableSplitter.tsx`，先只接入 shell 左/右分割线。
- [ ] 改 `styles.css` 使用 `--sidebar-w-current` 和 `--rightpanel-w-current`。
- [ ] 接入 Wiki/Exports/Lint 内部分割线。
- [ ] 补 AppShell/Wiki/Export/Lint 渲染测试和 CSS contract 测试。

## 条目 B：左侧的主视图也可收起

## 1. 需求概述
- 用户想要什么：左侧主导航栏可以在桌面端手动收起为图标栏，并可随时展开。
- 为什么：图谱、Markdown 阅读、HTML 预览和 Chat 对话都需要更大的横向空间，当前只有右侧面板可收起，左侧栏占用固定宽度。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/components/app/LeftSidebar.tsx`、`src/components/app/AppShell.tsx`、`src/stores/navigationStore.ts`、`src/components/app/TopBar.tsx`、`src/styles.css`。
- 当前行为是什么：`LeftSidebar` 总是渲染完整文本导航、最近页面和 Agent foot；响应式下可能依靠 CSS 适配，但没有桌面手动 collapse 状态。
- 问题出在哪里：`navigationStore.ts` 只有 `activeView` 与 `rightPanelOpen`，缺少 `sidebarCollapsed`；`LeftSidebar.tsx` 中的分组标题和最近页面没有 collapsed 渲染规则。

## 3. 方案设计
- 第一性原理：收起左侧栏的本质是“保留导航可达性，同时释放主工作区空间”。收起态不能隐藏主导航入口，只能隐藏非必要文本和最近/精选列表。
- 推荐方案：在 `navigationStore` 增加 `sidebarCollapsed`，由 `TopBar` 或 `LeftSidebar` 顶部 icon button 控制；expanded 时显示完整 3 分组，collapsed 时只显示主导航图标和底部 Agent 状态点。收起状态与 pane 宽度一起持久化到 `localStorage` key `llm-wiki-desktop.layout.v1`，不写入项目文件。
- 技术方案：
  - 修改 `src/stores/navigationStore.ts`：
    - `sidebarCollapsed: boolean`
    - `setSidebarCollapsed(collapsed: boolean): void`
    - `toggleSidebarCollapsed(): void`
  - 修改 `src/components/app/TopBar.tsx`，在项目选择器左侧加入 `PanelLeftClose` / `PanelLeftOpen` icon button，`aria-label` 使用 i18n。
  - 修改 `src/components/app/LeftSidebar.tsx`，根据 `sidebarCollapsed`：
    - nav row 只保留 16px Lucide icon；
    - `aria-current` 保持；
    - 最近页面、精选页面和分组 label 收起为不可见；
    - 每个 icon-only nav 具备 tooltip/`title`。
  - 修改 `src/styles.css`：`.app-shell.is-sidebar-collapsed .app-shell__workbench` 使用 `var(--sidebar-collapsed-w)`。
- 需要新增哪些文件：不需要。
- 需要修改哪些文件：`src/stores/navigationStore.ts`、`src/components/app/TopBar.tsx`、`src/components/app/LeftSidebar.tsx`、`src/components/app/AppShell.tsx`、`src/styles.css`、`src/i18n/locales/*.json`。
- 是否需要新增依赖：不需要，图标使用 `lucide-react` 的 `PanelLeftClose`、`PanelLeftOpen`。

## 4. UI / 交互设计
- 界面变化描述：展开态维持设计稿；收起态宽度 56px，显示一列 icon，active state 仍使用 `--accent-soft`。
- 交互流程：用户点击顶栏左侧按钮 -> 左侧栏收起/展开 -> 主工作区即时扩展 -> 状态保留到下次启动；窗口宽度低于 760px 时默认收起，但用户仍可展开。
- 需要参考的设计规范：`Spec/FRONTEND_GUIDELINES.md` 的 “Navigation”“Responsive Behavior”；AGENTS 中指定的 `UI-Frontend-design/dashboard.html` 左侧三分组结构。

## 5. 验收标准（Done Definition）
- [ ] 左侧栏可收起为 56px 图标栏，主导航仍可点击和键盘访问。
- [ ] 展开后恢复主视图、工作流、精选页面、最近页面和 Agent foot 的完整展示。
- [ ] 收起态所有 icon-only 控件有可访问名称和 tooltip。
- [ ] 切换语言、切换项目、刷新页面后收起状态保持。
- [ ] 收起态不出现文本裁切、重叠或横向滚动条。

## 6. 风险与注意事项
- 可能影响的现有功能：`LeftSidebar.tsx` 里的最近页面跳转依赖 `openPage` 和 `setActiveView("wiki")`，收起态不能破坏导航。
- 边界情况：当新增“精选页面”后，expanded/collapsed 两种状态都要有明确展示规则；tooltip 文案需中英双语。

## 7. 实施步骤
- [ ] 扩展 `navigationStore` 并写状态切换测试。
- [ ] 修改 `AppShell` className，接入 `is-sidebar-collapsed`。
- [ ] 实现 TopBar toggle。
- [ ] 修改 LeftSidebar collapsed 渲染和 a11y label。
- [ ] 补中英 i18n 和 CSS contract。

## 条目 C：顶栏当前项目、最近项目展示和选择更简洁

## 1. 需求概述
- 用户想要什么：顶栏当前项目和最近项目选择更紧凑，项目名称完整展示，路径可以中段省略。
- 为什么：完整路径在顶栏占位过大，项目名反而不易识别；最近项目切换需要更快、更像桌面应用项目切换器。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/components/app/TopBar.tsx`、`src/stores/projectStore.ts`、`src/types/project.ts`、`src-tauri/src/services/project_service.rs`、`src/styles.css`。
- 当前行为是什么：TopBar 展示 current project name/rootPath/recent projects，路径可能完整显示，占用搜索与右侧工具空间。
- 问题出在哪里：项目切换器没有按“主信息=项目名、次信息=省略路径、状态=健康/失效”组织；RecentProject DTO 属性较少，无法展示丰富但紧凑的信息。

## 3. 方案设计
- 第一性原理：顶栏项目区的本质是“当前工作上下文标识 + 快速切换入口”，不是路径阅读器。完整路径应进入 tooltip 或菜单详情。
- 推荐方案：TopBar 项目按钮主文本仅显示项目名，次文本显示 compact path；点击打开 recent menu，条目显示项目名、compact path、最近打开时间和失效状态。
- 技术方案：
  - 修改 `src/components/app/TopBar.tsx`：
    - 新增 `compactPath(path: string, maxSegments = 3): string`
    - 项目按钮布局为 name + path subtitle，path 使用 `title` 提供完整路径。
    - recent menu 每行使用 project name、compact path、lastOpenedAt。
  - 修改 `src/types/project.ts` 的 `RecentProject` 如后端已有字段则直接使用；若缺少 `name` 或 `lastOpenedAt`，扩展 Rust `RecentProject` DTO。
  - 修改 `src-tauri/src/services/project_service.rs::remember_recent_project` 和 `list_recent_projects`，确保 recent projects 中保留 `name`、`rootPath`、`lastOpenedAt`、`missing?: bool`。
  - 失效项目不自动删除，先标记 missing，并在用户点击时给出重新选择或移除。
- 需要新增哪些文件：可选 `src/lib/pathDisplay.ts` 和 `src/lib/pathDisplay.test.ts`，推荐新增以复用 BottomStatusBar/ProjectStartView。
- 需要修改哪些文件：`src/components/app/TopBar.tsx`、`src/stores/projectStore.ts`、`src/types/project.ts`、`src-tauri/src/models/project.rs`、`src-tauri/src/services/project_service.rs`、`src/styles.css`、`src/i18n/locales/*.json`。
- 是否需要新增依赖：不需要。

## 4. UI / 交互设计
- 界面变化描述：顶栏项目按钮宽度自适应，项目名单行完整优先；路径使用 monospace 11px，中间省略，如 `D:/.../llm-wiki-desktop`。
- 交互流程：点击项目按钮 -> recent popover 打开 -> 点击项目 -> `openProject(rootPath)`；项目不存在 -> 显示失效状态与移除按钮。
- 需要参考的设计规范：`UI-Frontend-design/dashboard.html` 顶栏与 search shortcut；`Spec/FRONTEND_GUIDELINES.md` 的 Top Bar 和 compact list rules。

## 5. 验收标准（Done Definition）
- [ ] 顶栏在 1120px 宽度下不挤压搜索、任务、语言、设置按钮。
- [ ] 项目名优先完整显示；路径中段省略并通过 tooltip 展示完整路径。
- [ ] 最近项目列表可用键盘选择，失效项目有明确状态。
- [ ] 中英界面下项目按钮文本不溢出。

## 6. 风险与注意事项
- 可能影响的现有功能：`projectStore.bootstrap()` 默认打开最近项目，recent DTO 变化要保持旧 JSON 兼容。
- 边界情况：Windows 盘符路径、UNC 路径、CJK 项目名、同名不同路径项目必须显示可区分。

## 7. 实施步骤
- [ ] 新增 `pathDisplay` 纯函数和测试。
- [ ] 扩展 RecentProject DTO，保持 serde default 兼容旧 recent JSON。
- [ ] 改 TopBar 项目按钮和 recent menu。
- [ ] 补 ProjectStore/TopBar 测试。

## 条目 D：添加配色自定义，可以修改主题（包括 Markdown 的渲染）

## 1. 需求概述
- 用户想要什么：用户可以在设置中从若干预设配色主题中选择，并同步影响界面与 Markdown 阅读渲染风格。
- 为什么：不同阅读偏好、屏幕环境和导出审美不同；当前只有 light/dark/auto 与字体/密度，缺少一键切换的成套配色方案，用户不希望自己逐项挑选颜色。
- 配色参考：https://github.com/VoltAgent/awesome-design-md/tree/main/design-md

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/features/settings/AppearanceSettings.tsx`、`src/stores/settingsStore.ts`、`src/types/settings.ts`、`src-tauri/src/services/settings_service.rs`、`src-tauri/src/models/settings.rs`、`src/styles.css`、`src/features/wiki/MarkdownReader.tsx`、`src/features/chat/MessageContent.tsx`。
- 当前行为是什么：`settingsStore.ts` 已有 `theme/density/uiFont/readingFont/codeFont`，`styles.css` 有 light/dark CSS variables 和 `.wiki-prose`、`.chat-prose`、`.html-preview` 样式。
- 问题出在哪里：主题只有明暗模式，不能切换成套的语义色板；Markdown 阅读 token 没有独立主题映射；如果开放任意颜色输入，会破坏 Codex-like 低饱和规范，也增加可读性和无障碍风险。

## 3. 方案设计
- 第一性原理：主题选择的本质是“让用户在可信赖的视觉方案之间快速切换”，而不是让用户承担设计系统配色工作。每个主题必须是一组完整、可读、可维护的语义 token。
- 推荐方案：只提供内置预设主题，不提供用户手动挑色、hex 输入或高级色板覆盖。第一阶段内置 4-6 套主题，例如 `codex`、`paper`、`graphite`、`mint`、`night`、`highContrast`，每套同时定义 app shell token 与 Markdown reading token。
- 备选方案 1：完全开放 CSS 变量编辑。灵活但风险高，不符合产品静稳设计，不推荐。
- 备选方案 2：预设主题 + 用户自定义色板覆盖。比纯预设更灵活，但会重新引入对比度、导出一致性和审美漂移问题，本阶段不推荐。
- 技术方案：
  - 修改 `src/types/settings.ts`：
    - `export type ColorThemePresetId = "codex" | "paper" | "graphite" | "mint" | "night" | "highContrast";`
    - `Settings` 增加 `colorThemePreset: ColorThemePresetId`，默认值为 `"codex"`。
  - 修改 Rust `src-tauri/src/models/settings.rs`，添加同名 serde default 字段，保持旧 `.app/settings.json` 兼容。
  - 修改 `src/stores/settingsStore.ts`：
    - `applyThemePreference(theme)` 保持；
    - 新增 `setColorThemePreset(preset: ColorThemePresetId): Promise<void>`
    - 新增 `applyColorThemePreset(preset: ColorThemePresetId): void`
  - 新增 `src/lib/colorThemePresets.ts`：
    - `export interface ColorThemePreset { id: ColorThemePresetId; labelKey: string; preview: { background: string; surface: string; accent: string; text: string; readingBackground: string; readingText: string; link: string; }; cssVars: Record<string, string>; }`
    - `export const COLOR_THEME_PRESETS: ColorThemePreset[]`
    - `export function getColorThemePreset(id: ColorThemePresetId): ColorThemePreset`
    - `export function applyColorThemePresetToRoot(preset: ColorThemePreset, root?: HTMLElement): void`
  - 修改 `src/features/settings/AppearanceSettings.tsx`，添加主题预设列表、色块预览、当前主题标记、Markdown preview 和恢复默认按钮。
  - 修改 `src/styles.css`，新增 semantic variables：`--reading-background`、`--reading-text`、`--reading-link`、`--reading-code-bg`，并让 `.wiki-prose`、`.chat-prose`、`.html-preview__iframe` wrapper 使用。
- 需要新增哪些文件：`src/lib/colorThemePresets.ts`、`src/lib/colorThemePresets.test.ts`。
- 需要修改哪些文件：`src/types/settings.ts`、`src/stores/settingsStore.ts`、`src/features/settings/AppearanceSettings.tsx`、`src-tauri/src/models/settings.rs`、`src-tauri/src/services/settings_service.rs`、`src/styles.css`、`src/i18n/locales/*.json`。
- 是否需要新增依赖：不需要。预设主题使用静态 token map，不引入颜色选择器或主题生成库。

## 4. UI / 交互设计
- 界面变化描述：Appearance 页面增加“Color theme”区域，以紧凑列表或 segmented grid 展示预设主题；每个主题展示名称、简短说明和 4-6 个色块预览。Markdown preview 显示标题、段落、wikilink、代码块、表格，用于预览阅读渲染。
- 交互流程：用户点击某个预设主题 -> CSS variables 即时应用到当前窗口和 Markdown preview -> 保存到 settings；点击恢复默认 -> 回到 `codex` 主题。用户不输入颜色值。
- 需要参考的设计规范：`Spec/FRONTEND_GUIDELINES.md` 色彩系统、AGENTS 中 `src/styles.css :root` 必须镜像 `UI-Frontend-design/assets/app.css :root` 的要求。自定义覆盖只能作为用户偏好层，不应修改设计源 token。

## 5. 验收标准（Done Definition）
- [ ] 用户可在 Appearance 设置中看到至少 4 套预设配色主题，并通过色块预览区分它们。
- [ ] 选择预设后，app shell、MarkdownReader、Chat MessageContent 和 HTML preview 周边 UI 即时使用对应 token。
- [ ] 主题选择保存到 settings，刷新和重开项目后仍生效。
- [ ] 不存在用户手动输入 hex、RGB、HSL 或任意 CSS 变量的入口。
- [ ] 默认 `codex` 主题仍与 Codex-like near-monochrome 规范一致。

## 6. 风险与注意事项
- 可能影响的现有功能：深色模式、密度、字体设置和 HTML preview iframe 可能与新 token 交叉。
- 边界情况：主题预设不能写入导出 HTML 的内容文件，除非导出流程明确选择模板；主题设置不得影响 `UI-Frontend-design/`；新增主题必须同时覆盖 app shell 和 reading token，不能只改 accent。

## 7. 实施步骤
- [ ] 写 `colorThemePresets` 纯函数测试：preset id fallback、CSS variable map、每个 preset 必含 reading token。
- [ ] 扩展 TS/Rust Settings DTO 并保证旧 settings 反序列化默认值。
- [ ] 实现 settingsStore preset apply 函数。
- [ ] 更新 AppearanceSettings 预设主题 UI。
- [ ] 更新 `.wiki-prose`、`.chat-prose`、Markdown preview 样式。
- [ ] 补中英文案、设置页测试、CSS contract 测试。
