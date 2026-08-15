# LLM Wiki Desktop 性能与交互流畅度审查

日期：2026-08-14
来源：从《第一性原理对抗性审查》拆分
范围：冷启动、板块切换、Chat 流式输出、拖拽、Graph、WikiTree、后端扫描、持久化写放大、性能观测

## 1. 结论

当前性能问题不是“某个按钮慢几毫秒”，而是几条核心路径缺少预算和合批：

- 首屏虽有 `React.lazy`，但壳层静态依赖仍把大量 feature、Markdown 和双语言资源带入启动链；
- 同一项目状态在多个 shell surface 并发探测，没有 single-flight；
- 切换板块会卸载视图，返回时重复 IPC、扫描和 Graph renderer 重建；
- Chat 每个 token 都复制累计文本、完整解析 Markdown 并同步滚动；
- pane、Graph filter、WikiTree 等高频交互把主线程工作放在每次鼠标或键盘事件中；
- 后端扫描、索引 clone、task JSON 重写也会放大前端等待。

这些机制足以构成明确性能风险，但本轮没有真实 packaged WebView 的 p50/p95 数据，因此不把它描述为“所有机器已经达到某个具体卡顿数值”。

## 2. 构建基线

本轮 production build 的首屏依赖为 **45 个 JS 文件**：

| 指标 | 当前基线 |
| --- | ---: |
| 首屏 JS raw | 1,596,119 B |
| 首屏 JS gzip | 461,565 B |
| 主 CSS | 244.34 kB raw / 53.02 kB gzip |
| 最大依赖 chunk | 496,405 B |
| 入口 chunk | 433,953 B |
| projectStore chunk | 430,973 B |
| WikiEditor 独立 chunk | 347.90 kB |
| GraphView 独立 chunk | 225.57 kB |

这只是资产基线。Tauri 本地资源没有公网下载耗时，但仍需承担 Windows 文件 I/O、杀毒扫描、WebView parse/compile、模块初始化和主线程执行成本。

## 3. 发布前必须优先处理

### PERF-P1-01 Chat 流式输出的单位工作量随答案长度持续增加

对应总报告：P1-13。

同一 stream event 被 task 与 Chat 两条 listener 消费；taskStore/chatStore 每个 delta 都 concat/clone。随后 Chat 把完整累计文本交给 GFM、math、KaTeX、highlight，并在每次更新后读写滚动布局：

- `src/hooks/useTaskEvents.ts:25-40,81-93`
- `src/hooks/useChatStream.ts:25-54`
- `src/stores/taskStore.ts:102-112`
- `src/stores/chatStore.ts:725-755`
- `src/features/chat/ChatView.tsx:663-727`
- `src/features/chat/MessageContent.tsx:78-91`

**影响**：短回答可能正常，长回答越到后面越容易抢占输入、滚动和板块切换的主线程时间；高频小 delta 会造成大量 React publication 和 GC。

**建议**：统一 event distributor；30–50 ms 或 RAF 合并 delta；内部保存 chunks/rope 并设上限；流式阶段使用轻量文本或节流 Markdown snapshot，终态完整解析；滚动同帧合并。

**验收**：1,000–10,000 个 delta、256 KiB 最终答案；测 publication、React commits、heap/GC、long tasks、input-to-paint、scroll 次数；终态文本逐字节一致。

**修复状态（2026-08-16 Batch 6）：Not Closed。** 实现提交 `02fa0068` 已加入 task stream 合批、终态前 flush 与轻量流式展示；聚焦测试和 1k/10k delta 各 20 个已完成 packaged 回放均保持 262,144 B byte-equal，中途输入、pane 拖动、滚离底部与 Wiki↔Chat 往返可执行。但终态附近仍相关出现多个 >50 ms long task，观测峰值约 1.7 s；当前 duration-only observer 不能把任务归因到 Markdown/math/highlight。路由往返后未发送草稿也丢失，因此不能按“已有实现”代替关闭证据。详见 [Batch 6 脱敏结果](../qa/results/2026-08-16-core-interaction-performance-batch-6.json) 与 [测量协议第 10 节](../qa/core-interaction-performance-benchmark.md#10-batch-6-整体验收2026-08-16)。

### PERF-P1-02 首屏 lazy 边界被壳层依赖泄漏削弱

对应总报告：P1-14。

壳层和 project reset 聚合器静态引入多个 feature store/controller：

- `src/components/app/AppShell.tsx:11-20`
- `src/components/app/RightContextPanel.tsx:3-18`
- `src/components/app/WorkspaceController.tsx:5-16`
- `src/stores/projectStore.ts:21`
- `src/stores/resetProjectScope.ts:1-10`

Import 右栏为一个 helper 反向引入 Markdown preview 及 KaTeX/highlight：

- `src/features/import/ImportRightPanel.tsx:4-14`
- `src/features/import/ImportMarkdownPreviewDialog.tsx:4-8`

两份 locale 也在 `src/i18n/index.ts:5-25` 静态注册，主应用等待 i18n 初始化后 render。

**影响**：用户只打开 Dashboard 也需解析大量非当前功能代码，冷启动和第一次交互的成本被前移。

**建议**：常驻小 facade + feature controller/panel 按需加载；把纯 helper 移到无 UI 依赖的 util；locale 按选中语言动态加载；用 idle/hover 预取下一视图而非首屏全 preload。

**验收**：入口依赖图和 chunk allowlist；当前 1.60 MB raw / 462 kB gzip 先降到建议的 0.9 MB / 250 kB 级别，再按真实启动结果校准。

**修复状态（2026-08-16 Batch 6）：Closed。** `fc876a1e` 建立初始闭包图与预算，`ca94a600` 恢复 lazy route/right-panel/locale 边界并收紧预算；bundle graph、forbidden initial modules、lazy reject/retry 和 locale parity 测试通过。最终初始 JS 为 38 files、616,366 B raw、187,483 B gzip，相对 Batch 0 分别下降 15.6%、61.3%、59.5%，同时低于 0.9 MB / 250 kB 目标；未使用全量 preload 或粗粒度 `manualChunks`。packaged debug fresh-profile/warm-profile p95 约 9.77 s 已作为独立观察记录，不被 bundle 通过结论掩盖；fresh-profile 不等同稳定 profile 的进程冷启动。证据见 [Batch 6 脱敏结果](../qa/results/2026-08-16-core-interaction-performance-batch-6.json)。

### PERF-P1-03 项目打开时重复 Git/Agent/provider 探测

对应总报告：P1-14。

三个 shell surface 同时使用 `useProjectStatus`，其 cache 没有 in-flight single-flight/TTL；controller 的 `useAiCapabilities` 又重复 Agent/provider 探测：

- `src/hooks/useProjectStatus.ts:28-73`
- `src/hooks/useAiCapabilities.ts:52-82`
- `src/components/app/LeftSidebar.tsx:32`
- `src/components/app/RightContextPanel.tsx:117`
- `src/components/app/BottomStatusBar.tsx:10`

默认右栏开启时，首次打开项目最多可出现 9 次 Git/Agent/provider probe，再加 controller 的 2 次 Agent/provider probe。

**影响**：重复 IPC、Git 进程和 CLI 探测互相竞争，既拖慢首屏，也增加功耗；完成态 cache 无 TTL 又会导致状态陈旧。

**建议**：project-keyed external store/query cache，保存 `{snapshot,inFlight,updatedAt}`；同 key single-flight；status 与 capability 共用 Agent/provider 事实源；设置变化、聚焦和 TTL 定向失效。

**验收**：同时 mount 全部 consumer，Git、Agent、provider 各调用一次；失败后能重试；A/B 项目旧响应不能覆盖。

**修复状态（2026-08-16 Batch 6）：Not Closed（packaged 证据 Pending）。** `7dae81a7` 已实现 project-keyed single-flight、TTL/force/retry、定向失效与 A/B 旧响应保护；对应聚焦测试证明全部 shell consumer 首次各 1 次、新鲜命中 0 次新增。打包 WebView 中 `window.__TAURI_INTERNALS__.invoke` 为 non-writable/non-configurable，Batch 6 未加入临时生产诊断入口，因而无法无侵入取得首次打开的真实 IPC 命令计数；按计划“没有真实数据不能 Closed”，本项保持未关闭。证据与 Pending 原因见 [Batch 6 脱敏结果](../qa/results/2026-08-16-core-interaction-performance-batch-6.json)。

### PERF-P1-04 后端 LLM 响应和 Markdown inventory 没有硬预算

对应总报告：P1-07；安全/可用性交叉项。

非流式 LLM 直接 `response.json()`；流式路径同时保留完整答案、frame、UTF-8 pending 和 raw response，未限制 raw bytes、单 frame、visible text 或 delta 数：

- `src-tauri/src/services/llm_service.rs:241-295,334-360,432-501`

Markdown walker 递归遍历、每目录收集并排序全部 entry，无文件数、深度、字节、deadline 或 cancellation budget；Graph async task 在 Tokio worker 上直接同步扫描：

- `src-tauri/src/models/layout.rs:367-409,888-966`
- `src-tauri/src/commands/graph_commands.rs:13-20,42-97`

**影响**：异常 provider 或巨大 vault 可造成内存失控、异步 worker 被占用、取消长时间无效。

**建议**：分别限制 response、frame、text、delta；迭代 walker + `ScanBudget`；分批取消/进度；有界 blocking pool 和每项目并发限制。

**验收**：chunked 无限流、无换行单帧、压缩炸弹、10 万文件、深目录、取消 SLA；峰值内存和其他 async command 延迟保持有界。

## 4. Beta 前应处理

### PERF-P2-01 板块切换会卸载视图并重复加载

对应总报告：P2-01。

`WorkspaceRouter` 只 render active branch；Wiki、Chat、Graph 等在 mount 时重拉数据，Graph 还重建/销毁 graphology、Sigma 和 worker：

- `src/components/app/WorkspaceRouter.tsx:76-100`
- `src/features/wiki/WikiView.tsx:219-225`
- `src/features/chat/ChatView.tsx:126-129`
- `src/features/graph/GraphView.tsx:197-301,716-734`
- `src/stores/graphStore.ts:92-125`

**建议**：per-project freshness-aware、event-invalidated、single-flight 的 stale-while-revalidate cache；对 Graph 等昂贵实例做有资源上限的保活或状态恢复。

**验收**：同项目 20 次往返，记录 IPC、spinner、Graph renderer rebuild、焦点/滚动/选择恢复和 p50/p95。

**修复状态（2026-08-16 Batch 6）：Not Closed（route-specific packaged 证据 Pending）。** `d21c8f42` 加入 freshness-aware route data reuse，`48e25456` 恢复 Wiki/Chat/Graph/Exports/Lint presentation，`cb7a54eb` 量化后按停止条件保留 Graph 正常 mount/unmount，未实现 warm host。packaged 20 次循环得到 click→`aria-current`+2 RAF CDP proxy p95 23.8–94.0 ms，追加合成 Chat session 后为 24.0–99.9 ms；该 proxy 没有等待各目的视图的可交互 marker，也没有逐次记录 transient loading 或 IPC，不能支撑整体关闭。500 页 Graph 热返回 p95 44.1 ms、无 >50 ms long task，且自动化 selection/scroll/camera、topology guard、项目切换清理、fresh hit 和 lazy recovery 仍是有效的局部证据。证据见 [Batch 4C Graph 结果](../qa/results/2026-08-16-core-interaction-performance-batch-4c.json) 与 [Batch 6 结果](../qa/results/2026-08-16-core-interaction-performance-batch-6.json)。

### PERF-P2-02 pane 拖拽每个 pointermove 同步持久化

对应总报告：P2-02。

`useResizablePane` 无 RAF；每次移动都更新完整 paneSizes、同步 `localStorage.setItem`，并让 AppShell 与重型当前视图跟随 render：

- `src/hooks/useResizablePane.ts:122-130,195-217`
- `src/stores/navigationStore.ts:229-250`
- `src/components/app/AppShell.tsx:59-76,141-175`

**建议**：拖拽时 local ref/CSS variable + RAF；pointerup/cancel 只持久化一次；selector 精确到当前 pane。

**验收**：2 秒拖拽约 1 次 storage write；长 Chat/Graph 下无 >50 ms long task。

**修复状态（2026-08-16 Batch 6）：Not Closed。** `f569d8aa` 已把 pointermove 收敛为 ref + RAF preview，并仅在 pointerup/cancel 边界 commit；聚焦测试证明有效 drag 恰好一次 store/storage 写入，move 不写 store。packaged 五类 splitter 各 20 个已完成 drag 的 input→next-RAF CDP proxy p95 为 17.82–18.03 ms；它包含 CDP 往返且 RAF 回调发生在 presentation 前，不能作为 input-to-paint 门槛通过。稳态 route+splitter trace 仍捕获 >50 ms long task，且 repeated CDP trace 对 rightPanel/wikiTree/exportsList 未能在每次样本产生有效 `aria-valuenow` 变化，不能作为五类 storage 合同的完整 packaged 关闭证据。详见 [Batch 6 脱敏结果](../qa/results/2026-08-16-core-interaction-performance-batch-6.json)。

### PERF-P2-03 Graph 搜索/过滤每个输入事件都全图扫描和 indexation

对应总报告：P2-03。

- `src/features/graph/GraphControls.tsx:59-64`
- `src/features/graph/GraphView.tsx:174-192,669-675`
- `src/features/graph/graphRenderModel.ts:84-118`

**建议**：deferred/debounce；RAF/transition 合并；预构建 lowercase/type/degree/community index；hidden set 真变化才 index。

**验收**：500 页产品基线和 10k/50k stress；连续输入 20 字符、slider sweep 的 keystroke-to-paint p95、refresh/indexation 次数。

### PERF-P2-04 WikiTree 无虚拟化，深目录计数最坏 O(N²)

对应总报告：P2-04。

过滤每字符拼接全部 searchable 字段；递归挂载可见 row；每个 folder 又递归统计后代：`src/features/wiki/WikiTree.tsx:66-107,145-235,338-390`。

**建议**：一次 post-order 生成 pruned tree/count；展平 visible rows 后 windowing；预构建规范化搜索串并用 deferred value。

**验收**：10k pages 宽树/深链 fixture；输入 p95、DOM row 数、render 次数、最大调用栈和键盘语义。

### PERF-P2-05 Wiki/Search 缓存仍做全树 stat 和大对象 clone

对应总报告：P2-10。

`scan_wiki` 每次 refresh 完整 inventory；prior snapshot、return entries、full body 多次 clone；tree insert 每层线性找已有 folder：

- `src-tauri/src/services/search_service/catalog.rs:24-48,168-209`
- `src-tauri/src/services/wiki_index.rs:128-206`

**建议**：immutable `Arc<IndexSnapshot/Entry>`；metadata/body 分层；watcher dirty set；Search/Graph 用轻量 projection；child map 后一次性转 Vec。

**验收**：1k/10k/100k 页面记录 stat、body read、clone bytes、峰值 RSS；10k sibling tree 接近 O(N) 或 O(N log N)。

### PERF-P2-06 authority revalidation 的慢 I/O 重复且位于全局锁内

对应总报告：P2-12；安全交叉项。

access snapshot 可执行实际 fsync probe 和多个同步 Git 进程，Chat 又重复 external/write 检查；全局 mutex 可能让项目 A 阻塞 B：

- `src-tauri/src/app_state.rs:788-849`
- `src-tauri/src/services/project_service.rs:137-165,1830-1891`
- `src-tauri/src/services/git_service.rs:368-419`

**建议**：一次命令生成 `ProjectAuthoritySnapshot`；慢 I/O 在锁外 `spawn_blocking`，完成后 CAS epoch；锁缩到 per-project；缓存 `git --version`。

**验收**：单次 Chat 的 Git process/fsync 次数明确下降；项目 A fake Git 卡住时项目 B 仍可及时操作。

### PERF-P2-07 Task log/activity 追加形成 JSON 重写放大

对应总报告：P2-13。

每条 log/activity 立即 persist，snapshot clone 全历史；恢复又整文件读取所有 task JSON：`src-tauri/src/tasks/task_service.rs:2450-2527,3293-3367`。

**建议**：bounded append-only event chunks + 小 metadata snapshot；内存只留 tail；终态归档；恢复先做 count/size/schema budget。

**验收**：10k logs 累计写入字节和耗时接近 O(N)，而非随日志数量二次放大。

### PERF-P3-01 缺少隐私友好的真实性能诊断

对应总报告：P3-02。

当前没有统一的 route/startup PerformanceObserver budget、React commit/long-task/heap gate，也没有可由用户安全导出的本地 support trace。

**影响**：性能退化通常只能等到用户主观报告后手工复现；不同机器和运行环境的比较缺少统一数据，修复也难证明有效。

**建议**：默认不启用云遥测；提供 opt-in 本地 trace，只记录 timing、错误 code、包体和任务阶段，严格移除路径、内容和 secret，由用户显式导出。

**验收**：诊断包通过自动 redaction 测试；在不包含知识库内容和绝对路径的前提下，足以还原启动、切换、流式和任务阶段耗时。

## 5. 性能门禁建议

| 场景 | 建议目标 |
| --- | --- |
| cold process start → shell paint | p95 ≤ 2.0 s |
| warm start → interactive | p95 ≤ 1.0 s |
| 已缓存普通板块切换 | p95 ≤ 150 ms |
| 已缓存 Graph 返回 | 500 页 p95 ≤ 500 ms；10k 作为 stress |
| 输入/拖拽/滚动 | p95 input-to-paint ≤ 100 ms；无 >50 ms long task |
| Chat UI publication | ≤ 20–30 Hz；终态内容一致 |
| 同项目新鲜缓存重入 | 不重复全扫描；同一事实源 IPC 0–1 次 |
| 长任务取消 | 在固定 SLA 内停止并清理进程/请求 |

指标必须在 packaged Tauri、参考 Windows 低配机、杀软开启、远程桌面/GPU fallback 环境采集。浏览器 dev server 结果不能替代桌面结论。

## 6. 推荐顺序

1. Chat stream 合批与后端响应上限；
2. project status/capability single-flight；
3. 拆首屏依赖泄漏并建立 bundle budget；
4. 板块 cache/保活与 pane drag；
5. Graph/WikiTree/后端 WikiIndex 大规模优化；
6. task persistence 与 authority lock 粒度；
7. 固化真实 WebView 性能门禁和本地可导出 trace。
