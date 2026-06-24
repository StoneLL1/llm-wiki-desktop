# Graph 板块 P0+P1 修复 — 进度账本（/loop 本轮）

> 权威范围：本 `/loop` 调度参数。把 roadmap 中部分 P2（度数阈值、导出）**提升为 P1** 本轮必做；不碰 P2 其余、不碰别板块。
> 对照源：`SPEC/roadmap/graph.md` · `SPEC/PRD.md` PRD-GRAPH-001..006 · `UI-Frontend-design/graph.html` + `assets/app.css` §32（只读禁改）· `CLAUDE.md`
> 改动边界：只动 `src/` 与 `src/styles.css`。不改 `UI-Frontend-design/`、不自动改 `src-tauri/`。
> 状态机：`pending` → `in_progress` → `done`（实施完+记行号）→ `verified`（test+lint 全绿、清 console.log）。每项独立 commit。

## 全局决策（动手前定）

1. **控件版式**：对齐设计稿——画布**左上纵向悬浮**放缩放/适配/重置（设计 `.graph-controls`，5 按钮列）。顶部横条保留 seg(着色模式) + 搜索框 + 重新布局/重建 + 导出 SVG + 节点/边计数（设计 `main__toolbar`）。原 GraphControls 顶部横条里的 zoom-in/out/fit/reset 拆出为画布内 `GraphCanvasControls`。改动可控，无需重构为绝对定位的独立层——`GraphView` 的 canvas 容器已 `relative`，悬浮层用绝对定位即可。
2. **导出走前端**：PRD 未规定导出落盘路径，硬边界「持久化必须落到项目文件夹」针对的是项目内容。图谱导出是用户产物（非知识库内容），用浏览器 `Blob`+`a[download]` 下载更稳、不触发 Git 检查点；Tauri `save` dialog 也可但增加 capability/插件耦合。选浏览器下载（无后端改动、守硬边界「不自动改 src-tauri」）。SVG 由 graphology 拓扑+缓存坐标手工构建（不依赖 sigma WebGL，headless 可跑、可单测）；PNG 经 `Image`+`canvas.toDataURL`。文件名 `<projectName>-graph-<timestamp>.svg`。
3. **度数阈值滑块**：roadmap 标 P2，本 loop 提为 P1。隐藏 `degree <= threshold` 的节点（设计稿「隐藏度数 ≤ range」，value 0 即不隐藏）。与类型筛选、搜索叠加。
4. **图例按 colorMode 动态**：type 模式列 6 类 + 计数（仅 WIKI_PAGE_TYPES 六类，忽略 index/overview/log/other 兜底类型）；community 模式列前 8 社区 + 其它；plain 模式仅一句说明不列条目。隐藏类型不计入图例计数（与筛选状态同步）。
5. **信息卡缩放倍率**：sigma camera `ratio`，`1/ratio`≈缩放倍率（设计稿「缩放 · 1.0×」）。订阅 `camera` 的 `updatedState` 事件实时刷新。
6. **搜索语义修正**（roadmap P2，顺手）：现状非匹配 `hidden=true`+`DIM`（DIM 永不可见）→ 改为仅 `hidden=true`，去掉 DIM（设计稿搜索框语义本就是隐藏非匹配）。与类型/度数筛选一致用 `hidden`。
7. **i18n**：所有新文案补 zh-CN + en 双键。
8. **测试**：sigma 依赖 canvas/headless，无法在 jsdom 实例化 renderer（现有测试走「无数据→空态」规避）。新组件逻辑（图例计数、邻居计算、SVG 构建器、筛选 reducer 期望）抽为纯函数并单测；组件渲染测用空态/props 注入。

## 项清单

- [x] **G1 · 画布视觉** `verified` — 网格底纹 + 圆角 + 边框（styles.css `.graph-canvas`）+ 容器重构。commit `4e784c1`。文件:`src/styles.css:1552-1672`、`src/features/graph/GraphView.tsx:219-227`
- [ ] **G2 · 图例 graph-legend** `pending` — 左下悬浮，colorMode 动态。文件:`src/features/graph/GraphLegend.tsx`(新)、`GraphView.tsx`、`graphLegend.ts`(纯函数)
- [ ] **G3 · 信息卡 graph-info** `pending` — 右上悬浮，缩放/选中/度数。文件:`src/features/graph/GraphInfo.tsx`(新)、`GraphView.tsx`
- [x] **G4 · 控件版式** `verified` — GraphCanvasControls(纵向悬浮,左上) + GraphControls 顶部精简(seg+search+rebuild+exportSvg+counts)。文件:`src/features/graph/GraphCanvasControls.tsx`(新)、`src/features/graph/GraphControls.tsx`、`src/features/graph/GraphView.tsx:168-187,205-233`
- [ ] **G5 · 类型筛选 + 度数阈值** `pending` — graphStore(typeFilter,degreeThreshold) + nodeReducer + Inspector 筛选区。文件:`graphStore.ts`、`GraphView.tsx`、`GraphInspector.tsx`、`RightContextPanel.tsx`、`graphFilters.ts`(纯函数)
- [ ] **G6 · 相邻列表 + Inspector 段** `pending` — 邻居列表(label+badge+view-all) + 图谱状态段 + 操作段。文件:`GraphInspector.tsx`、`RightContextPanel.tsx`、`graphNeighbors.ts`(纯函数)
- [x] **G7 · SVG/PNG 导出** `verified`(工具栏入口) — graphExport.ts(SVG 构建+PNG)+工具栏 SVG 按钮+7 单测。Inspector PNG/按钮待 G6。文件:`src/features/graph/graphExport.ts`(新)、`graphExport.test.ts`(新)、`GraphControls.tsx`、`GraphView.tsx:187-190`

## 收敛判据

全部 G1..G7 `verified` + `npm run test` & `npm run lint` 全绿 + 清 console.log + 每项独立 conventional commit（不 --no-verify、不 push）+ progress.txt 里程碑 + 本账本顶部盖「✅ 本轮完成 @ 2026-06-24」+ 文件清单 → 不再调度。
