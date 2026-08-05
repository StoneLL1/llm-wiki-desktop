# Graph 板块落差与实施计划

> 对照源：UI-Frontend-design/graph.html + assets/app.css §32 + SPEC/PRD.md §9.5
> 当前实现：src/features/graph/、src/stores/graphStore.ts、src-tauri/src/commands/graph_commands.rs
> 首次价值与项目访问边界：[`../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)。Graph 从当前可读 Markdown 建图，不要求先编译 Wiki；restricted/read-only 模式只构建内存图且不写 `ProjectLayout.graphCachePath`；深度扫描未完成时必须标注部分结果。

## 0. 现状摘要

核心管线已真实落地（非 stub）：

- sigma.js + graphology + graphology-layout-forceatlas2（含 Worker supervisor）+ graphology-communities-louvain 均已在 `GraphView.tsx` 顶部 import 并真正调用（`src/features/graph/GraphView.tsx:1-5, 263, 379, 383`）。
- 数据当前来自后端 `.app/graph-cache.json`：`get_graph` 读取缓存 → 缓存缺失时前端收到 `GRAPH_BUILD_REQUIRED` → `build_graph` 启动可取消后台任务 → 重建后回读（`src-tauri/src/commands/graph_commands.rs:12-127`、`src/stores/graphStore.ts:51-147`）。这是 trusted writable 项目的现状；restricted/read-only 的内存构建与不落盘路径仍待实现。
- 前端布局计算后通过 `save_graph_layout` 持久化位置与社区（`src/features/graph/GraphView.tsx:400-421`、`src-tauri/src/commands/graph_commands.rs:137-143`），NaN/Infinity 已做 sanitize，满足"布局缓存"硬约束。
- 三种着色模式（类型/社区/单色）、缩放/适配/重置布局/重建、节点点击选中、双击跳转 Wiki、悬停高亮邻居——均已实现（`GraphView.tsx:129-146, 299-330`、`GraphControls.tsx`）。
- 选中节点右侧检查器已通过 `RightContextPanel` 接入（`src/components/app/RightContextPanel.tsx:78-104`）。

PRD-GRAPH-001/002/003/004/005 的核心算法基本达成，PRD-GRAPH-006（边统一表示“相关”）也已在 `GraphEdge.relation` 单一字段体现。每页一节点、单一“相关”边保持不变；布局缓存只适用于允许持久化的项目，不能把 `.app/graph-cache.json` 当成 restricted 模式的前置条件。

主要落差集中在**设计稿的画布内悬浮 UI 层（图例/信息卡/筛选/导出）和视觉密度**，而非核心算法。

## 1. 区块 / 组件清单

| 区块/组件 | 设计稿要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| sigma.js 画布 | `graph.html:133-242` SVG 节点+边；app.css §32 圆角网格背景画布 | 真实 sigma WebGL renderer，容器 `.graph-canvas`，但无网格/辐射底纹 | 🟡部分实现 | P1 | `src/features/graph/GraphView.tsx:220`；`UI-Frontend-design/assets/app.css:1513-1526` |
| ForceAtlas2 布局 | 动态布局 + "重新布局"按钮 | Worker supervisor 真实运行，1s 后停并落盘；重置/重新布局按钮已接 | ✅已完成 | — | `GraphView.tsx:369-398, 169-177` |
| Louvain 社区发现 | 社区着色模式、右面板"社区 #3 · 24 节点" | `louvain.assign` 真实运行，community 模式着色已实现 | ✅已完成 | — | `GraphView.tsx:264, 335-351` |
| 类型着色 | 6 类页面色板 swatch（Entity/Concept/Source/Synthesis/Comparison/Query） | `PAGE_TYPE_COLORS` 与设计稿 hex 完全一致 | ✅已完成 | — | `src/types/graph.ts:57-68` |
| 顶部工具条 | seg(类型/社区/单色) + 重新布局 + 导出 SVG 按钮（`graph.html:92-100`） | 有 seg + 重建，**无导出 SVG 按钮** | 🟡部分实现 | P1 | `GraphControls.tsx:36-89` |
| 画布内悬浮控件 | 左上纵向 5 按钮（放大/缩小/适配/重置/筛选，`graph.html:106-112`、app.css:1528-1550） | 控件做成**顶部横条**而非左上纵向悬浮；无"筛选"按钮 | 🟡部分实现 | P1 | `GraphControls.tsx:36-89`；`app.css:1528-1550` |
| 信息卡 graph-info | 右上 mono 信息（缩放/选中/度数，`graph.html:115-119`、app.css:1587-1599） | **完全缺失** | ❌缺失 | P1 | `GraphView.tsx:204-227` |
| 图例 graph-legend | 左下类型图例 + 每类计数（`graph.html:122-130`、app.css:1552-1585） | **完全缺失** | ❌缺失 | P1 | — |
| 右面板"选中节点"详情 | 图标+标题+路径+meta(类型/度数/社区/中心度/更新)+相邻节点列表+操作+图谱状态+筛选+操作（`graph.html:248-318`） | 仅有类型/度数/邻居数/标签/打开页面；**缺社区、中心度、相邻节点列表、图谱状态、筛选、导出** | 🟡部分实现 | P1 | `src/features/graph/GraphInspector.tsx`；`src/components/app/RightContextPanel.tsx:78-104` |
| 筛选：按类型勾选 | 6 个 checkbox + 各类型计数（`graph.html:298-305`） | **缺失**（仅顶部搜索框做模糊匹配隐藏） | ❌缺失 | P1 | — |
| 筛选：度数阈值滑块 | "隐藏度数 ≤ range"（`graph.html:306-308`） | **缺失** | ❌缺失 | P2 | — |
| 导出 SVG/PNG | 工具栏主按钮 + 右面板操作按钮 | **完全缺失** | ❌缺失 | P2 | `GraphControls.tsx:80-86` |
| 标题副信息 | "知识图谱 · 237 节点 · 1,284 边 · ForceAtlas2"（`graph.html:91`） | 节点/边数移到了右上角 mono 标签；**缺主标题与"ForceAtlas2"算法标识** | 🟡部分实现 | P2 | `GraphControls.tsx:65-67` |
| 状态栏图谱状态 | "类型着色 · ForceAtlas2"、"图谱已缓存"、节点/边/社区数（`graph.html:322-328`） | 由全局状态栏承载，未核实是否回显图谱着色模式/算法 | 🟡部分实现 | P2 | — |
| 加载/错误/空态 | 设计稿无显式态 | 三态均实现，文案 i18n | ✅已完成 | — | `GraphView.tsx:182-202` |
| 画布不可用兜底 | 设计稿无 | headless/无 canvas 时显示 `graph.canvasUnavailable` 占位 | ✅已完成 | — | `GraphView.tsx:221-225` |
| 悬停高亮 | 设计稿 `.is-active` 边/节点强调 | nodeReducer/edgeReducer 实现邻居高亮 + 选中节点着色 | ✅已完成 | — | `GraphView.tsx:299-330` |
| 搜索高亮 | 设计稿搜索框"搜索节点…" | 搜索非匹配节点 `hidden=true`（而非 dim），匹配保留 | 🟡部分实现 | P2 | `GraphView.tsx:310-313` |
| 项目访问 / 部分结果 | 可读 Markdown 即可建图；restricted 内存只读；深度扫描可后台继续 | 当前要求缓存可用/可构建，未区分 trust、read-only、兼容布局或部分扫描 | ❌缺失 | P0 | `graph_commands.rs`、`graphStore.ts`、项目访问 DTO |

## 2. 功能落差（PRD 对照）

- [ ] **可读 Markdown 与访问模式（P0）**：输入同时覆盖已提交 Source Markdown 与 Wiki Markdown；不得把“先编译 Wiki”作为 Graph 前提。restricted 模式在有限深度内存建图且不写缓存；trusted read-only 可读取并内存建图但不写布局；trusted writable 才持久化 cache/layout。深度扫描运行中显示 `partial=true`、已扫描/待扫描数量和“继续扫描”任务入口。→ 验收：Source-only、restricted、read-only、trusted writable 与大型仓库部分扫描均有真实结果和准确状态。
- [ ] **PRD-GRAPH-003 增强 · 类型筛选 checkbox**：现状仅有模糊搜索框 → 目标：6 类型 checkbox（实体/概念/来源/综合/对比/查询）各自显隐并显示计数 → 涉及 `src/features/graph/GraphControls.tsx`、`src/features/graph/GraphView.tsx`（`nodeReducer` 增加类型白名单）、`src/stores/graphStore.ts`（新增 `typeFilter: Set<WikiPageType>`）→ 验收：勾掉"概念"，图中所有 concept 节点消失，图例计数与勾选状态同步。
- [ ] **PRD-GRAPH-005 · 导出 SVG/PNG**：现状无导出 → 目标：工具栏 + 检查器均提供导出，SVG 经 sigma `renderer.toSVG()`（或 graphology 导出），PNG 经 `canvas.toDataURL` → 涉及 `GraphControls.tsx`、`GraphInspector.tsx`、新建 `src/features/graph/graphExport.ts` → 验收：点导出 SVG 得到包含全部可见节点/边的 `.svg` 文件，文件名含项目名+时间戳。
- [ ] **信息卡（缩放/选中/度数）**：现状缺失 → 目标：画布右上悬浮 mono 卡片，实时显示当前相机缩放倍率、选中节点 label、度数 → 涉及 `GraphView.tsx`（订阅 `renderer.getCamera().getState().ratio` 与 `selectedNodeId`）→ 验收：滚轮缩放时缩放倍率随之刷新；选中节点时 label 更新。
- [ ] **图例（类型色板 + 计数）**：现状缺失 → 目标：左下悬浮，按当前 colorMode 动态切换"类型"/"社区"图例条目，每行 swatch + 文案 + 计数；社区模式显示前 N 个社区 + "其它" → 涉及新建 `src/features/graph/GraphLegend.tsx`、`GraphView.tsx` 内挂载 → 验收：切换 colorMode 图例内容跟随变化；隐藏类型不在图例中重复计数。
- [ ] **度数阈值滑块**：现状缺失 → 目标：右面板筛选区 range 0–20，默认 0 且隐藏 `degree < threshold` 的节点；因此 0 保留合法的孤立/零边节点，用户主动调到 5 才隐藏度数 0–4 → 涉及 `graphStore.ts`（`degreeThreshold: number`）、`GraphView.tsx` `nodeReducer` → 验收：Source-only 单节点零边图默认可见；滑到 5 时孤立小节点淡出/隐藏，邻居高亮与搜索组合生效。
- [ ] **选中节点相邻列表**：现状检查器只显示邻居数 → 目标：列前 6–12 个相邻节点 label + 类型 badge + "查看全部 N 个 →"跳转 → 涉及 `GraphInspector.tsx`（从 `graphData.edges` 计算 neighbor 节点数组传入）、`RightContextPanel.tsx` → 验收：点击列表项跳转 Wiki 并选中该节点。
- [ ] **节点搜索语义修正**：现状非匹配节点同时 `color=DIM` 且 `hidden=true`（DIM 永远看不到）→ 目标：默认 dim 不隐藏，或仅隐藏（二选一并与设计一致）；建议保留 dim、移除 hidden，避免布局塌陷 → 涉及 `GraphView.tsx:310-313` → 验收：搜索时非匹配节点变灰但保留位置，清空搜索立即还原。

## 3. 视觉 / 设计 token 落差

- **画布底纹缺失**：设计稿 `graph-canvas` 有 32px 网格 + 中心辐射绿色光晕（`app.css:1513-1526`），当前仅纯背景色。建议在 `graph-canvas` 容器加 tailwind arbitrary background 或复用设计稿 CSS。优先级 P1。
- **画布圆角与边框**：设计稿 `border-radius: var(--radius-lg)` + `border: 1px solid var(--border-subtle)`，当前 `GraphView.tsx:220` 容器无圆角无边框。P1。
- **控件布局形态错位**：设计稿为画布**内左上纵向悬浮**（5 按钮 column，`app.css:1528-1540`），当前为画布**顶部横条工具栏**。属于版式偏离，需产品确认是沿用当前横条还是回到悬浮；若回到设计稿，需重构 `GraphControls` 为绝对定位。P1。
- **节点 label 字号**：设计稿 10px、`text-anchor: middle`、`fill: var(--text-secondary)`、选中 `var(--accent-hover)` 600 字重（`graph.html:22-30`）；sigma 由 `labelColor: { color: "#6b7280" }` 接近，但字号/字重靠 sigma 内部规则，需对齐。P2。
- **edge 样式**：设计稿 default `stroke: var(--border)` 0.5px、active `var(--accent)` 1.2px；当前 `EDGE_COLOR=#d4d4d4`、`SELECTED_COLOR=#0d9488`，与 token 近似但不完全一致（设计 accent 为 `#10a37f` 系，选中用的是 teal-600）。建议核对 `--accent` token 后统一。P2。
- **顶栏标题副信息**：设计稿 `main__title` + 副标题"· 237 节点 · 1,284 边 · ForceAtlas2"，当前 `GraphView` 无主标题（由 AppShell 统一渲染？需核实是否已有页面 H1）。P2。

## 4. 交互 / 可访问性落差

- **键盘可访问性**：sigma 画布节点无法用键盘聚焦/切换。建议补 `tabIndex` 容器 + 方向键遍历选中节点（最低限度：Tab 进入画布后方向键移动选中）。P2。
- **`aria-label`**：`GraphControls` 的 icon 按钮已有 `aria-label`/`title`（✅）；但图例、信息卡缺失时一并需补 `role="status"` / `aria-live`（信息卡选中节点变化应对屏幕阅读器可见）。P2。
- **画布缩放快捷键**：设计稿顶栏 `⌘K` 是全局搜索；图谱画布缺 `+`/`-`/`0` 快捷键（放大/缩小/适配）。P2。
- **右面板折叠**：设计稿有 `data-right-toggle` 折叠按钮（`graph.html:251`），需核实 `RightContextPanel` 是否已支持折叠。P2。
- **搜索框去抖**：`onSearchChange` 每键触发 `refresh`，大图（数百节点）可能卡顿，建议 150ms 去抖。P2。
- **状态栏图谱行**：设计稿状态栏"类型着色 · ForceAtlas2 · 237 节点 · 1,284 边 · 18 社区 · 图谱已缓存"需核实 `StatusBar` 是否回显这些字段；若未回显，补 graph 状态切片。P2。

## 5. 建议实施顺序

1. **P0 · 输入与访问策略**：统一 Source/Wiki Markdown 扫描、restricted/read-only 内存模式、trusted writable 缓存和 partial 状态。
2. **P1 · 画布视觉对齐**（底纹 + 圆角 + 边框 + edge/label token 校准）。
3. **P1 · 信息卡 + 图例**——新建两个轻量悬浮组件，复用现有 `data`/`colorMode`。
4. **P1 · 类型筛选 checkbox + 右面板筛选区**——`graphStore` 加 `typeFilter`，`nodeReducer` 增白名单逻辑；图例计数复用。
5. **P1 · 控件版式回设计稿（产品确认）**——若产品确认悬浮形态，重构 `GraphControls` 为绝对定位纵列；否则在当前横条上加“筛选”按钮打开 popover。
6. **P1 · 选中节点相邻列表**——扩展 `GraphInspector` props，`RightContextPanel` 计算 neighbor 节点数组传入。
7. **P2 · 导出 SVG/PNG**——封装 `graphExport.ts`，双入口（工具栏 + 检查器）。
8. **P2 · 度数阈值滑块 + 搜索去抖 + 键盘可达 + 状态栏图谱行**——打磨批次。

> 注：本板块所有建议严格遵守 CLAUDE.md 硬约束——每个可读 Source/Wiki Markdown 文档对应一个页面级节点、边统一表示“相关”；restricted/read-only 只保留有界内存结果，只有 trusted writable 且布局提供 `graphCachePath` 时才持久化布局（原生映射为 `.app/graph-cache.json`）；不引入复杂关系类型与证据系统。
