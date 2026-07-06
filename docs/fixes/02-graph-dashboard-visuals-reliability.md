# 02 Graph, Dashboard Visuals, and Reliability Specification

本规范整合以下条目：图谱美化、图谱重新打开消失、概览界面丰富板块。

## 条目 A：图谱美化

## 1. 需求概述
- 用户想要什么：知识图谱更清晰、更美观、更适合探索，节点、边、标签、图例、选中态和空/加载态都更专业。
- 为什么：PRD 明确图谱是导入后的第一价值感；当前图谱虽已可用，但仍需要更稳定的视觉层次和探索效率。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/features/graph/GraphView.tsx`、`GraphControls.tsx`、`GraphCanvasControls.tsx`、`GraphInfo.tsx`、`GraphLegend.tsx`、`GraphInspector.tsx`、`legendEntries.ts`、`graphExport.ts`、`graphNeighbors.ts`、`src/stores/graphStore.ts`、`src/types/graph.ts`、`src/styles.css`。
- 当前行为是什么：已有 sigma.js 画布、类型/社区/纯色模式、缩放、fit/reset、SVG/PNG 导出、类型过滤、度数阈值和右侧 Inspector。
- 问题出在哪里：视觉仍偏功能堆叠，节点标签密度、边透明度、选中路径、高亮邻居、布局状态提示和图谱空状态还可以更接近 Codex-like 分析工具。

## 3. 方案设计
- 第一性原理：图谱的本质是“降低页面关系的认知负担”。美化优先服务可读性：先看见结构，再看见类别，最后看见单点详情。
- 推荐方案：在不改变 GraphData 后端 schema 的前提下，优化前端 nodeReducer/edgeReducer、图例和 Inspector；不新增图谱关系类型。
- 技术方案：
  - 修改 `src/features/graph/GraphView.tsx`：
    - 抽出 `buildNodeReducer(options: GraphRenderOptions)` 和 `buildEdgeReducer(options: GraphRenderOptions)` 到新文件。
    - 选中节点时：选中节点半径 +2、描边 teal、邻居 opacity 1、非邻居 opacity 0.16。
    - 搜索命中时：匹配节点保持 label，非匹配隐藏或淡化需与筛选语义分开。
  - 新增 `src/features/graph/graphRenderStyle.ts`：
    - `export interface GraphRenderOptions { colorMode: GraphColorMode; selectedNodeId: string | null; search: string; typeFilter: Set<WikiPageType>; degreeThreshold: number; }`
    - `export function nodeVisualFor(node: GraphNode, options: GraphRenderOptions): NodeVisual`
    - `export function edgeVisualFor(edge: GraphEdge, options: GraphRenderOptions): EdgeVisual`
  - 修改 `src/types/graph.ts` 的颜色常量，保持 `PAGE_TYPE_COLORS` 低饱和，但增加 label/edge token，不改 DTO。
  - 修改 `GraphLegend.tsx`，图例项显示 visible/hidden count，hover 某一类型时可临时高亮该类型。
  - 修改 `GraphInspector.tsx`，加入“Focus neighbors”开关和“Open in Wiki”主动作。
  - 修改 `src/styles.css` 的 `.graph-*`，统一画布背景网格、右上信息卡、左下图例、控件尺寸。
- 需要新增哪些文件：`src/features/graph/graphRenderStyle.ts`、`src/features/graph/graphRenderStyle.test.ts`。
- 需要修改哪些文件：上述 graph 组件、`src/types/graph.ts`、`src/styles.css`、`src/i18n/locales/*.json`。
- 是否需要新增依赖：不需要。

## 4. UI / 交互设计
- 界面变化描述：中央画布保持 full canvas；左上为垂直 zoom/focus/reset 控件，顶部为紧凑 filter toolbar，左下图例，右侧面板为节点详情。
- 交互流程：用户搜索/筛选 -> 画布只突出相关节点 -> 点击节点 -> 邻居和路径高亮，右侧展示页面摘要、类型、度数、邻居列表 -> 点击 Open -> 切到 Wiki 页面。
- 需要参考的设计规范：`Spec/FRONTEND_GUIDELINES.md` 的 Graph 章节；`Spec/PRD.md` PRD-GRAPH-001 到 PRD-GRAPH-006；gotchas 中 sigma v3 的 `type` 保留字段，graphology 节点属性必须继续使用 `pageType` 而不是 `type`。

## 5. 验收标准（Done Definition）
- [ ] 图谱节点颜色、边透明度、标签显示、选中态和 hover 态在 200-500 页规模下仍可读。
- [ ] 类型/社区/纯色三种模式的图例颜色与画布一致。
- [ ] 选中节点后，邻居、非邻居、边、右侧详情状态清晰区分。
- [ ] SVG/PNG 导出反映当前筛选后的可见节点，不包含隐藏节点。
- [ ] 无新增 `console.log`，sigma 初始化失败仍有用户可理解 fallback。

## 6. 风险与注意事项
- 可能影响的现有功能：已有 graph P0/P1 修复处理了导出筛选、legend 颜色、camera ratio、`pageType` 等问题，不能回退。
- 边界情况：孤立节点、单节点图、ForceAtlas2 NaN/Infinity 坐标、WebGL 不可用环境、CJK 文件名节点都必须保留 fallback。

## 7. 实施步骤
- [ ] 写 `graphRenderStyle.test.ts` 覆盖选中/搜索/筛选/度数阈值视觉结果。
- [ ] 抽 GraphView reducer 逻辑到纯函数。
- [ ] 调整 GraphLegend 与 GraphInspector 状态展示。
- [ ] 更新 styles.css 图谱视觉 token。
- [ ] 补 graphView/graphExport/legendEntries 回归测试。

## 条目 B：BUG：图谱有时候重新打开会消失

## 1. 需求概述
- 用户想要什么：重新打开项目或图谱视图时，图谱不能偶发空白或消失；缓存损坏或过期时应自动重建并显示进度。
- 为什么：图谱是核心价值入口，偶发消失会让用户误以为数据丢失。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/stores/graphStore.ts`、`src/features/graph/GraphView.tsx`、`src-tauri/src/commands/graph_commands.rs`、`src-tauri/src/services/graph_service.rs`、`src-tauri/src/services/search_service.rs`。
- 当前行为是什么：`graphStore.load()` 调 `get_graph`，失败时触发 `build_graph`；后端 `GraphService::resolve` 已有 content hash 校验和 cache fallback，`GraphService::read_cache` 对损坏缓存回 None。
- 问题出在哪里：审计报告指出 `get_graph` 曾只读 cache、不校验 live hash；当前服务已有 `resolve`，但仍需确认 command 层使用 `resolve`，前端使用 `layoutStale` 并正确处理空 cache、stale layout、任务完成事件和 renderer 重建。

## 3. 方案设计
- 第一性原理：图谱消失不应被视为 UI 空态，而应被视为“缓存不可用 -> 自动重建 -> 可见恢复”的状态机。
- 推荐方案：后端 `get_graph` 永远走 `GraphService::resolve(context, pages)`，前端 `graphStore.load` 明确区分 `ready-empty`、`rebuilding`、`error`；GraphView 对 `data.nodes.length === 0` 显示空图说明而不是空白。
- 技术方案：
  - 修改 `src-tauri/src/commands/graph_commands.rs::get_graph(request, state)`：扫描 wiki -> 调 `state.graph_service.resolve(&context, &tree.pages)` -> 返回 `GraphBuildResult { data, layoutStale }`。
  - 修改 `src-tauri/src/services/graph_service.rs::resolve` 的测试，增加 cache contentHash mismatch、cache nodes empty but pages non-empty、corrupt JSON 三类回归。
  - 修改 `src/stores/graphStore.ts::load(projectId, rootPath)`：
    - 如果 `result.data.nodes.length === 0 && wiki totalPages > 0`，调用 `rebuild` 并显示 task drawer。
    - 若 `layoutStale` 为 true，仍渲染节点，但提示“layout refreshed”并允许保存新布局。
  - 修改 `src/features/graph/GraphView.tsx`：
    - renderer 初始化失败、data 为空、data 缺 positions 三种状态分开显示。
    - `useEffect` dependency 使用 `data.contentHash` 和 `data.nodes.length`，避免 stale renderer 没有重建。
- 需要新增哪些文件：不需要，补既有测试即可。
- 需要修改哪些文件：`src-tauri/src/commands/graph_commands.rs`、`src-tauri/src/services/graph_service.rs`、`src/stores/graphStore.ts`、`src/features/graph/GraphView.tsx`、`src/features/graph/graphStore.test.ts`、`src/features/graph/graphView.test.tsx`。
- 是否需要新增依赖：不需要。

## 4. UI / 交互设计
- 界面变化描述：空白画布改为明确状态：`Building graph...`、`Graph cache is stale`、`No wiki pages yet`、`Canvas unavailable`。
- 交互流程：用户进入 Graph -> 读取 cache -> hash 匹配则直接渲染 -> hash 不匹配或 cache 损坏则创建 build task -> task 完成后自动 reload -> 仍失败则显示错误与 retry。
- 需要参考的设计规范：`Spec/APP_flow.md` 图谱流程中“布局缓存后秒级打开”“首次构建可进入后台任务并允许取消”。

## 5. 验收标准（Done Definition）
- [ ] `.app/graph-cache.json` 缺失、损坏、hash 过期时，进入图谱会自动重建或给出 retry，不出现无说明空白。
- [ ] 修改/保存 Wiki 页面后图谱再次打开能反映新 content hash。
- [ ] 项目切换后不会把上一个项目的图谱数据写入当前项目 store。
- [ ] 单节点、无边、无 wiki 页面三种状态都有明确 UI。
- [ ] graphStore 和 graphService 回归测试覆盖上述路径。

## 6. 风险与注意事项
- 可能影响的现有功能：图谱构建是后台任务，不能引入新的 250ms 轮询；已有 `waitForTaskTerminal` 可复用。
- 边界情况：Graph build 任务被取消时，UI 应回到 stale cache 或 error，而不是清空已可用旧图。

## 7. 实施步骤
- [ ] 确认并修改 `get_graph` command 使用 `GraphService::resolve`。
- [ ] 写 Rust cache stale/corrupt 回归测试。
- [ ] 修改 graphStore 状态机，复用 `waitForTaskTerminal`。
- [ ] 修改 GraphView 空/错误/重建状态。
- [ ] 跑 graphStore/graphView 相关测试。

