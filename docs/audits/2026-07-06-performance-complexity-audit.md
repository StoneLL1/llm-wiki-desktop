# 性能瓶颈与复杂度失控审查报告

日期：2026-07-06  
项目：LLM Wiki Desktop  
范围：启动/构建、前端 bundle、Graph、Search/Chat 上下文、Rust 文件 IO/Git/Task、测试噪声、前后端职责边界  
结论类型：审查与治理计划，不包含重构实现

## 0. 审查边界

本次审查按 `AGENTS.md` 要求先阅读了产品、规格、流程、架构和设计文档：

- `AGENTS.md`
- `SPEC/PRD.md`
- `SPEC/SPEC.md`
- `SPEC/APP_flow.md`
- `SPEC/TECH_STACK.md`
- `SPEC/BACKEND_STRUCTURE.md`
- `SPEC/FRONTEND_GUIDELINES.md`
- `SPEC/DESIGN.md`
- `UI-Frontend-design/dashboard.html`
- `UI-Frontend-design/assets/app.css`

设计原型只作为对照基线读取，没有修改。审查期间未改源码。

### 0.1 已运行检查

| 命令 | 结果 | 说明 |
|---|---:|---|
| `npm run lint` | 通过 | ESLint 退出码 0 |
| `cargo check` | 通过 | `src-tauri` dev profile，用时约 41.30s |
| `npm run test` | 失败 | 48 个 test files 中 1 个失败；313 tests 中 1 个失败；Graph/Sigma 在 jsdom 下反复输出 `HTMLCanvasElement.prototype.getContext` 未实现噪声 |

未运行 fresh `npm run build`，因为它会重写 `dist/` 与 TypeScript build info。审查读取了现有 `dist/`，其中 `dist/assets/index-Dmu_vTi_.js` 约 1.89MB，CSS 约 172KB。

### 0.2 当前 worktree 状态

审查开始时 worktree 已存在多处未提交改动与未跟踪文件，包括 `SPEC/progress.txt`、`SPEC/gotchas.txt`、`src-tauri/Cargo.toml`、`src/components/app/AppShell.tsx`、`src/stores/navigationStore.ts`、`docs/audits/` 等。本报告不判断这些改动归属，也不回滚。

## 1. 总体判断

项目当前不是单点慢，而是三层叠加：

1. 首屏入口链路静态加载过多重功能模块：Graph、Milkdown、Markdown renderer、Readability、多个 feature views。
2. Search、Chat retrieval、Graph cache resolve 都依赖全量 Markdown 扫描和重复文件读取。
3. Graph 渲染层在 reducer/refresh 路径里有重复全图级计算。

复杂度失控也不是单个大文件的问题，而是同一种编排职责同时出现在三层：

1. 前端 `AppShell` 既是 shell，又是 import/provider/agent/task/controller。
2. Tauri command 不够 thin，混入 Git checkpoint、Agent/BYOK 路由、preview 持久化和 rollback 编排。
3. Rust services 与 Zustand stores 都有多个 400-2000+ 行文件，功能边界正在变钝。

建议不要立即做大重构。应先处理 P0 火点，让测试可信、任务等待可靠、首屏拆包、全量扫描可缓存、Graph reducer 不做 O(E*N)。之后再逐步瘦身架构。

## 2. 优先级与难度定义

| 标记 | 含义 |
|---|---|
| P0 | 必须修；直接影响稳定性、用户可感知性能或测试可信度 |
| P1 | 值得修；会持续放大维护成本或中大型数据性能风险 |
| P2 | 可以以后修；偏长期质量、可观测性和工程化 |
| S | 小改，局部可控 |
| M | 中等改动，需要小范围设计和回归 |
| L | 大改，需要分阶段迁移 |

## 3. 性能风险

### PERF-001：测试当前不可信，Graph/Sigma 噪声掩盖真实回归

优先级：P0  
修复难度：S/M  
分类：测试慢 / 测试噪声 / 前端 Graph 环境隔离

现象：

- `npm run test` 失败。
- 失败数量本身不大：313 个测试中 1 个失败。
- 但测试输出反复出现 `Not implemented: HTMLCanvasElement.prototype.getContext`，来自 Sigma 在 jsdom 中构造 WebGL renderer。
- 这种噪声会让真正的回归淹没在无关错误里。

根因：

- `src/test/setup.ts` 只 stub 了 `WebGL2RenderingContext` 和 `WebGLRenderingContext`，没有 stub `HTMLCanvasElement.prototype.getContext`。
- `GraphView` 在测试环境仍然进入 `new Sigma(...)` 路径。
- `AppShell` 静态 import `GraphView`，导致很多 App 级测试间接加载 Graph/Sigma。

证据：

- `src/test/setup.ts:5-14`：只提供 WebGL class stubs。
- `src/features/graph/GraphView.tsx:171-181`：构造 Sigma，catch 后 `console.warn`。
- `src/features/graph/GraphView.tsx:487`：`new Sigma(graph, container, ...)`。
- `src/app/App.test.tsx:454`：失败断言查找 `Collapse sidebar`。
- 测试结果：`Test Files 1 failed | 47 passed`，`Tests 1 failed | 312 passed`。

影响范围：

- CI / 本地测试可信度。
- 所有 import `AppShell` 或 `GraphView` 的测试。
- 后续性能重构时很难判断是真的慢、真的失败，还是环境噪声。

建议：

- P0 先修测试环境：stub canvas `getContext`，或在 `GraphView` 注入 renderer factory，让 jsdom 不构造 Sigma。
- 单独修复 `Collapse sidebar` 测试和当前 UI 行为不一致的问题。
- 把 GraphView lazy import 后，App 级测试不应默认拉入 Sigma。

### PERF-002：任务终态等待可能永久挂住

优先级：P0  
修复难度：S  
分类：长任务可靠性 / 前端任务等待 / Import / Graph

现象：

- Import preview 或 Graph build 可能卡在 loading。
- 如果终态事件丢失、listen 失败或事件先于 listener 注册完成，等待 promise 没有兜底。

根因：

- `waitForTaskTerminal` 只监听 `task://completed`、`task://failed`、`task://cancelled`。
- 它只做一次 `get_task`，且如果不是 terminal 状态就不再轮询。
- Promise 没有 timeout，没有 reject 分支。
- listen 失败只吞掉错误。

证据：

- `src/lib/waitForTaskTerminal.ts:18-57`：Promise 只 resolve，不 reject。
- `src/lib/waitForTaskTerminal.ts:50-56`：只调用一次 `get_task`。
- `src/components/app/AppShell.tsx:345-368`：Import preview 等待该 promise 后才读取 preview。
- `src/stores/graphStore.ts:201-220`：Graph build 等待该 promise 后才 `get_graph`。

影响范围：

- Import preview。
- Graph rebuild。
- 未来所有复用 `waitForTaskTerminal` 的异步任务。

建议：

- 加 fallback polling：例如每 750-1000ms `get_task`，直到 terminal。
- 加合理 timeout，并把超时错误显示到 task drawer/toast。
- listen 注册失败时不要静默永久等待。
- 如果可行，封装为 `waitForTaskTerminal(task, { timeoutMs, pollMs })`。

### PERF-003：首屏 bundle 过大，重功能模块被静态拉入

优先级：P0/P1  
修复难度：M  
分类：启动慢 / bundle 过大 / feature code splitting

现象：

- 现有 `dist/assets/index-Dmu_vTi_.js` 约 1.89MB，主 CSS 约 172KB。
- 首屏项目 shell 会静态拉入 Graph、Wiki editor、Markdown reader、Chat、Import、Exports、Lint、Settings 等模块。
- 即使用户只看 Dashboard，也会承担 Graph/Milkdown/Markdown/Readability 等依赖成本。

根因：

- `AppShell` 顶部静态 import 所有 view。
- `GraphView` 静态 import `sigma`、`graphology`、`forceAtlas2`、`louvain`。
- `WikiView` 静态 import `WikiEditor`，`WikiEditor` 静态 import Milkdown/ProseMirror/theme CSS。
- `Chat` 和 `Wiki` Markdown reader 静态 import `react-markdown`、`remark`、`rehype`、`katex`、highlight。
- `Vite` 没有手动 chunks，也没有 view-level lazy boundary。

证据：

- `src/components/app/AppShell.tsx:5-14`：静态 import 所有 feature views。
- `src/features/graph/GraphView.tsx:1-5`：静态 import Graph/Sigma/ForceAtlas2。
- `src/features/wiki/WikiEditor.tsx:15-30`：静态 import Milkdown 相关包和主题 CSS。
- `src/features/chat/MessageContent.tsx:3-7`：静态 import Markdown renderer plugins。
- `src/features/wiki/MarkdownReader.tsx:3-7`：同类 Markdown renderer plugins。
- `src/lib/readability.ts:1`：静态 import `@mozilla/readability`。
- `vite.config.ts:13-17`：只有 target/minify/sourcemap，没有拆包策略。
- `package.json:18-44`：重依赖集中在 runtime dependencies。

影响范围：

- App 冷启动。
- 首次打开项目后的 Dashboard 可交互时间。
- 测试 import 时间：Vitest 输出中 transform/import/environment 时间都偏高。

建议：

- 第一阶段只做 route/view lazy loading：`GraphView`、`WikiView`、`ChatView`、`ImportView`、`ExportsView`、`LintView`、`SettingsView`。
- 第二阶段拆更细：`WikiEditor` 仅 edit mode 加载；Markdown renderer 可在 reader/chat message boundary lazy；Readability 仅 URL import 路径动态 import。
- 增加 bundle budget 和 chunk 报告，避免回归到单主包。

### PERF-004：Search / Chat retrieval / Graph cache 都被全量 Markdown 扫描拖住

优先级：P0/P1  
修复难度：M  
分类：全量扫描 / 文件 IO / Chat 上下文构造 / Graph 缓存

现象：

- 搜索每次列举并读取所有 Markdown。
- Chat retrieval 复用 search 后，又为 top results 二次读取页面正文。
- Graph `get_graph` 即使缓存命中，也要先 `scan_wiki` 才能计算 live hash。
- 200-500 页目标规模下，单次操作仍可接受，但多功能共享全量扫描会叠加。

根因：

- 没有 wiki metadata/content index。
- `load_page` 读文件后，`build_meta` 又调用 `file_hash`，`file_hash` 再读整文件 bytes。
- `ProjectContext` 每次路径 resolve/to_relative 都可能 canonicalize，安全正确但热路径昂贵。
- Graph cache staleness 依赖 `pages` 的 hash，所以 cache resolve 前必须重新扫全量 pages。

证据：

- `src-tauri/src/services/search_service.rs:37-44`：`scan_wiki` 逐文件 `load_page`。
- `src-tauri/src/services/search_service.rs:487-513`：search 每次列文件、逐文件 load。
- `src-tauri/src/services/search_service.rs:561-576`：正文参与评分和 snippet。
- `src-tauri/src/services/search_service.rs:629-635`：Chat retrieval 调 search 后再 `read_page`。
- `src-tauri/src/services/search_service.rs:655-671`：`load_page` 读文件并 parse frontmatter/body。
- `src-tauri/src/services/search_service.rs:702-723`：metadata、mtime、hash、word count、wikilinks。
- `src-tauri/src/services/file_store.rs:151-157`：`file_hash` resolve path 后 hash。
- `src-tauri/src/services/file_store.rs:238-242`：`hash_file` 再 `fs::read` 整文件。
- `src-tauri/src/commands/graph_commands.rs:17-20`：`get_graph` 先 bookmark + scan，再 graph resolve。
- `src-tauri/src/models/paths.rs:80-103`：每次 escape check canonicalize root/target。

影响范围：

- 顶栏搜索。
- Chat / Agent 上下文构造。
- Graph 打开与 rebuild。
- Wiki tree scan。
- 大 wiki、慢磁盘、Windows Defender 扫描时尤明显。

建议：

- P0/P1 建立 per-project in-memory index：path、mtime、size、hash、frontmatter、title、tags、wikilinks、body excerpt。
- `load_page` 一次读文件时顺手 hash content，避免二次 `fs::read`。
- Graph cache staleness 可基于 index snapshot，而不是每次重新 full scan。
- 对 `.app/index.json` 持久化可以考虑，但必须保持 Markdown/JSON/local files，不引入数据库。

### PERF-005：Graph 渲染 reducer 中存在 O(E*N) 级重复计算

优先级：P0  
修复难度：M  
分类：Graph 渲染 / sigma reducer / 交互卡顿

现象：

- 搜索、hover、selected/focused node、type filter、degree slider、layout refresh 都会触发 renderer refresh。
- edge reducer 每条边都重建 render options，并重新扫描所有 nodes 构造 hidden set。
- 大图下，UI 卡顿来自 reducer 逻辑，而不只是 WebGL。

根因：

- `currentRenderOptions()` 在 reducer 内动态创建 `communityByNodeId` Map。
- `hiddenNodeIds(options)` 每次调用扫描 `graphData.nodes`。
- edge reducer 对每条边都调用 `hiddenNodeIds(options)`。
- `refresh(renderer)` 固定 `skipIndexation: false`。
- layout 运行时每 50ms refresh 一次。

证据：

- `src/features/graph/GraphView.tsx:116-148`：多类 state 变化都 refresh。
- `src/features/graph/GraphView.tsx:468-479`：每次构造 render options 和 community map。
- `src/features/graph/GraphView.tsx:480-485`：hidden set 通过扫描所有 nodes 创建。
- `src/features/graph/GraphView.tsx:512-516`：每条 edge 计算 options + hidden set。
- `src/features/graph/GraphView.tsx:565-566`：refresh 固定 `skipIndexation: false`。
- `src/features/graph/GraphView.tsx:600`：layout 每 50ms refresh。

影响范围：

- Graph 主视图。
- Graph 搜索、筛选、hover。
- Graph layout recompute。
- PNG/SVG export 前的可视状态一致性。

建议：

- 为每次 refresh 预计算 `GraphRenderOptions`、`hiddenNodeIds`、`communityByNodeId`。
- node/edge reducer 只读已有 snapshot，不做全图扫描。
- hover/highlight 类 refresh 优先验证是否可 `skipIndexation: true`。
- 对 search/slider 输入 debounce 或 animation-frame throttle。

### PERF-006：Graph 第二次打开仍可能慢，cache 命中前必须 scan

优先级：P1  
修复难度：M  
分类：Graph cache / 第二次打开性能目标

现象：

- 文档目标提到 Graph 第二次打开应较快。
- 当前 backend cache 存在，但 `get_graph` 为判断 cache 是否新鲜，仍先扫描 wiki pages。

根因：

- `GraphService::resolve` 需要 `pages` 参数计算 live hash。
- command 层负责先 scan，再 resolve。

证据：

- `src-tauri/src/services/graph_service.rs:149-180`：resolve 使用 pages 计算 live hash。
- `src-tauri/src/commands/graph_commands.rs:17-20`：get_graph 每次 scan。

影响范围：

- Graph tab 打开。
- Dashboard 中如果未来展示真实 graph 摘要，也可能受影响。

建议：

- 复用 `PERF-004` 的 wiki index。
- Graph cache 可以存 `wiki_index_generation` 或 content-hash snapshot。
- 如果 index 未变化，直接返回 `.app/graph-cache.json`，避免重新读全部 Markdown。

### PERF-007：Task 日志/进度持久化写放大

优先级：P1  
修复难度：S/M  
分类：Task service / 日志 / JSON 持久化 / UI event volume

现象：

- 长 Agent、Import、Lint 任务会频繁发 log/progress。
- 每次 log/progress 都 clone task/log_lines 并写 task JSON。
- 前端 task store 也每次复制日志数组。

根因：

- TaskService 把 log_lines 放进 persisted task JSON。
- `append_log` 和 `update_progress` 每次都 `persist_current_task`。
- 前端 `appendLog` 使用 `[...existing, line]`，日志越长复制越贵。

证据：

- `src-tauri/src/tasks/task_service.rs:255-281`：update progress 后 emit + persist。
- `src-tauri/src/tasks/task_service.rs:286-306`：append log 后 emit + persist。
- `src-tauri/src/tasks/task_service.rs:453-470`：persist 克隆 task 和完整 log_lines 写 JSON。
- `src/stores/taskStore.ts:88-93`：前端日志数组每次扩展复制。
- `src/hooks/useTaskEvents.ts:24-58`：全局订阅多个 task/event channel。

影响范围：

- Import preview。
- Agent 输出。
- Deep lint。
- Export/compile 长任务。

建议：

- 后端 task JSON 只存 task snapshot；完整 log 追加写 `.log` 或分页 JSONL。
- 前端只保留最近 N 行，Task drawer 打开时按需读取完整日志。
- progress 更新做节流，例如 250-500ms 或百分比变化。

### PERF-008：Chat streaming 用字符串累加，长回答会产生复制放大

优先级：P1  
修复难度：S  
分类：Chat streaming / Zustand render frequency

现象：

- 长回答 token-by-token streaming 时，每个 delta 都复制一遍已有字符串。
- 每个 delta 都触发 store set 和相关订阅渲染。

根因：

- `streamingText` 是单字符串。
- `appendStreamDelta` 使用 `state.streamingText + delta`。

证据：

- `src/stores/chatStore.ts:410-417`。

影响范围：

- Chat BYOK streaming。
- Agent streaming。
- 长回答、代码块、引用较多时更明显。

建议：

- 存 chunks array，selector 或 view 层 join。
- 或用 requestAnimationFrame / 50ms batch 合并 delta。
- 终态消息保存后再清空 ephemeral chunks。

### PERF-009：Graph tag co-occurrence 仍可能制造过多边

优先级：P1  
修复难度：S/M  
分类：Graph topology / 边数控制

现象：

- tag group 已限制 `MAX_TAG_GROUP_FOR_EDGES = 64`，避免单个超大 tag 爆炸。
- 但 64 页以内仍是完全图，单 tag 最多 2016 条 pairwise edges；多个常见 tag 会叠加。

根因：

- tag 共现直接 pairwise emit。
- 没有全局 edge budget、top-k per node、stop words/generic tag 降权。

证据：

- `src-tauri/src/services/graph_service.rs:14-16`：group 上限为 64。
- `src-tauri/src/services/graph_service.rs:63-72`：pairwise edges。

影响范围：

- Graph cache size。
- Graph render。
- Graph export。

建议：

- 给 tag edges 加总预算。
- 对 `llm`、`note`、`misc` 这类泛标签降权或跳过。
- 每个 node 只保留 top-k tag co-occurrence edges。

### PERF-010：长任务同步 IO/CPU 运行在 async runtime task 内

优先级：P1  
修复难度：M  
分类：Tauri backend / async runtime / blocking IO

现象：

- 多个 command 使用 `tauri::async_runtime::spawn` 启动后台任务，但任务内部大量同步文件 IO、Git/process 调用、PDF/DOCX extraction。
- 并发长任务可能挤占 runtime worker。

根因：

- Rust service 大多是同步 API。
- command 层直接在 async task 内调用同步重 IO/CPU 方法。

证据：

- `src-tauri/src/commands/graph_commands.rs:42-44`：spawn 后跑 graph build。
- `src-tauri/src/commands/graph_commands.rs:74-97`：scan wiki + rebuild。
- `src-tauri/src/commands/import_commands.rs:327-367`：import preview loop 同步 extraction。
- `src-tauri/src/commands/chat_commands.rs:102`：chat task spawn。
- `src-tauri/src/commands/compile_commands.rs:69`：compile task spawn。
- `src-tauri/src/commands/export_commands.rs:113`：export task spawn。

影响范围：

- Import。
- Graph。
- Compile。
- Export。
- Chat convenience / Agent。

建议：

- 对 CPU/IO 密集型任务使用 dedicated worker 或 `spawn_blocking`。
- 保持 task progress/cancel channel 不变。
- 避免一口气大迁移，先从 Graph/Import 这种明显同步扫描路径开始。

### PERF-011：source replacement 在确认前执行重 extraction

优先级：P1  
修复难度：M  
分类：Import / destructive confirmation / UX latency

现象：

- 替换 source 是高风险操作，需要用户确认。
- 但当前在返回 PendingAction 之前已经 hash replacement 并执行 extraction。
- 大 PDF/DOCX 会导致用户还没看到确认对话框，前端就等待重处理。

根因：

- `request_replace_source` 同时做验证、hash、extract、构造 pending action。
- command 层承担过多业务编排。

证据：

- `src-tauri/src/commands/import_commands.rs:186-231`：验证、读 index、hash。
- `src-tauri/src/commands/import_commands.rs:232-238`：确认前 `extract_text`。
- `src-tauri/src/commands/import_commands.rs:242-260`：之后才构造 `PendingAction`。

影响范围：

- Replace source UX。
- 大文件导入。
- 高风险操作确认体验。

建议：

- request 阶段只做轻验证与摘要。
- extraction 放到确认后的 cancellable task，或放到 preview task 且明确标识 staged artifacts。
- command 下沉到 import use-case service。

## 4. 复杂度与职责混乱

### CX-001：`AppShell` 已经不是 shell，而是跨功能控制器

优先级：P1  
修复难度：M

现象：

- `AppShell.tsx` 713 行。
- 它同时负责：
  - shell layout / sidebar / right panel；
  - pending action dialog；
  - task drawer；
  - import preview；
  - source delete/replace；
  - provider capabilities；
  - provider secret save/delete/test；
  - agent run dialog；
  - view dispatch。

根因：

- 功能编排逐步堆到 shell。
- feature hooks/controller 缺失。
- 视图静态 import 与 prop drilling 让 shell 越改越胖。

证据：

- `src/components/app/AppShell.tsx:5-14`：静态 import 所有视图。
- `src/components/app/AppShell.tsx:236`：`WorkspaceView` 内开始大量业务编排。
- `src/components/app/AppShell.tsx:285-307`：capability probing + route state。
- `src/components/app/AppShell.tsx:334-368`：import preview task + terminal wait。
- `src/components/app/AppShell.tsx:597-606`：provider secret/test 相关。
- `src/components/app/AppShell.tsx:627-700`：activeView 分发。

影响范围：

- 首屏 bundle。
- 后续功能迭代冲突。
- App 级测试变脆。
- Shell 与 feature 边界模糊。

建议：

- 先不重写 layout。
- 增加 `useImportController`、`useProviderCapabilities`、`useAgentRunController`。
- view map + `React.lazy` 替代长三元分发。
- Shell 只保留布局、导航、全局 task/pending action 容器。

### CX-002：Tauri command 层过胖，违反既定 backend shape

优先级：P1  
修复难度：L，需分阶段

现象：

- 文档要求：thin Tauri commands -> typed DTOs -> services -> local files/Git/Agent/LLM/OS secrets。
- 当前多个 command 文件包含业务编排、Git checkpoint、Agent/BYOK 路由、rollback、pending action、preview 持久化。

根因：

- command 从入口层演化为 use-case 层。
- service API 尚未提供足够粗粒度的 use-case 方法。

证据：

- `src-tauri/src/commands/chat_commands.rs:122`：`run_chat_send`。
- `src-tauri/src/commands/chat_commands.rs:327`：`run_chat_convenience_send`。
- `src-tauri/src/commands/compile_commands.rs:92`：`run_compile`。
- `src-tauri/src/commands/compile_commands.rs:173`：`generate_manifest`。
- `src-tauri/src/commands/import_commands.rs:130`：`request_delete_source`。
- `src-tauri/src/commands/import_commands.rs:186`：`request_replace_source`。
- `src-tauri/src/commands/import_commands.rs:327`：`run_import_preview`。
- `src-tauri/src/commands/export_commands.rs:83`：`run_export_task`。
- `src-tauri/src/commands/export_commands.rs:203`：`run_export`。

影响范围：

- 所有 backend feature。
- 测试粒度混乱。
- command 层难以复用和审计安全边界。

建议：

- 不做大爆炸式拆分。
- 每次触碰某个 command 时，把一个 use-case 抽到 service：
  - `ChatSendUseCase`
  - `ChatConvenienceUseCase`
  - `CompileUseCase`
  - `ImportPreviewUseCase`
  - `ExportUseCase`
- command 只保留 request validation、context resolve、service 调用、DTO 返回。

### CX-003：Rust service 文件过大，职责边界变钝

优先级：P1/P2  
修复难度：M/L

现象：

最大 Rust 文件：

| 文件 | 行数 |
|---|---:|
| `src-tauri/src/services/lint_service.rs` | 2381 |
| `src-tauri/src/services/import_service.rs` | 2275 |
| `src-tauri/src/services/search_service.rs` | 1992 |
| `src-tauri/src/services/extraction_service.rs` | 1983 |
| `src-tauri/src/services/agent_service.rs` | 1373 |
| `src-tauri/src/tasks/task_service.rs` | 1308 |
| `src-tauri/src/services/project_service.rs` | 1286 |
| `src-tauri/src/commands/chat_commands.rs` | 1136 |
| `src-tauri/src/services/export_service.rs` | 1104 |
| `src-tauri/src/services/chat_service.rs` | 1101 |
| `src-tauri/src/services/compile_service.rs` | 1035 |

根因：

- 按领域聚合，但没有继续按 parser/index/planner/executor/persistence 拆分。
- tests 也可能内嵌在同文件，进一步增加认知负担。

影响范围：

- 修改风险高。
- review 成本高。
- 编译错误定位和单元测试定位成本高。

建议：

- 先从纯函数模块拆起，不动外部行为：
  - `search_index`
  - `frontmatter_parser`
  - `wiki_link_rewriter`
  - `task_persistence`
  - `graph_topology`
  - `import_artifact_index`
- 每次拆分都保留旧 public API，减少调用方 churn。

### CX-004：前端 view/store 文件变成小型应用

优先级：P1/P2  
修复难度：M

现象：

最大前端文件：

| 文件 | 行数 |
|---|---:|
| `src/features/wiki/wiki.test.tsx` | 1100 |
| `src/app/App.test.tsx` | 914 |
| `src/components/app/AppShell.tsx` | 713 |
| `src/features/graph/GraphView.tsx` | 644 |
| `src/features/project/ProjectStartView.tsx` | 635 |
| `src/features/wiki/WikiView.tsx` | 571 |
| `src/features/chat/ChatView.tsx` | 535 |
| `src/features/exports/ExportsView.tsx` | 534 |
| `src/features/import/ImportView.tsx` | 500 |
| `src/features/wiki/wikiStore.ts` | 489 |
| `src/stores/lintStore.ts` | 479 |
| `src/stores/chatStore.ts` | 431 |
| `src/stores/settingsStore.ts` | 425 |

根因：

- View 同时负责数据加载、状态选择、事件处理、对话框编排、布局。
- Store 同时负责状态、IPC、副作用、错误处理、scope guard。
- `hasTauri`、`errorMessage`、project scope guard 等模式多处重复。

证据：

- `src/features/wiki/WikiView.tsx:42-90`：大量 store selector 和本地 dialog state。
- `src/stores/chatStore.ts:35-43`：重复 `hasTauri/errorMessage`。
- `src/hooks/useTaskEvents.ts:17-21`：重复 `hasTauri/errorMessage`。
- `src/features/wiki/wikiStore.ts:139` 等多处 `captureProjectScope`。
- `src/stores/lintStore.ts:167` 等多处 `captureProjectScope`。

影响范围：

- UI feature 迭代速度。
- 测试 setup 复杂。
- 状态变更容易产生隐式耦合。

建议：

- 提取共享 frontend platform utils：`hasTauri`、`errorMessage`、`invokeProject`、`withProjectScope`。
- View 拆成 controller hook + presentational components。
- Store 只保留 domain state 和 domain actions；复杂 flows 放 feature service/hook。

### CX-005：GraphView 单文件混合 renderer lifecycle、layout、style reducer、export、导航

优先级：P1  
修复难度：M

现象：

- `GraphView.tsx` 644 行。
- 同时承担：
  - data loading trigger；
  - sigma renderer lifecycle；
  - ForceAtlas2 layout；
  - Louvain communities；
  - reducers；
  - hover/search/filter state sync；
  - export actions registration；
  - wiki navigation。

根因：

- Graph 核心视图没有拆成 renderer hook 和 pure graph utilities。
- reducer/performance-sensitive logic 藏在 React component 文件里。

证据：

- `src/features/graph/GraphView.tsx:100-148`：stateRef 和多处 refresh effects。
- `src/features/graph/GraphView.tsx:159-190`：build graph + renderer lifecycle。
- `src/features/graph/GraphView.tsx:460-525`：createRenderer + reducers。
- `src/features/graph/GraphView.tsx:581-610`：background layout。

影响范围：

- Graph 性能优化难以单测。
- 渲染 bug 和数据 bug 混在一起。
- Graph export 与渲染状态容易不一致。

建议：

- `useSigmaGraphRenderer` 管 lifecycle。
- `graphRenderModel.ts` 放 pure render options、hidden set、visualForNode/Edge。
- `graphLayout.ts` 管 ForceAtlas2/Louvain。
- GraphView 只负责布局和 glue。

### CX-006：路径安全校验正确但在热路径重复 canonicalize

优先级：P2  
修复难度：S/M

现象：

- 路径逃逸保护很重要，不能删除。
- 但全量扫描和 hash 路径上反复 canonicalize root/target，增加系统调用。

根因：

- `ProjectContext` 只存 root/path dirs，没有缓存 canonical root。
- 每次 `ensure_no_detectable_escape` 都 canonicalize。

证据：

- `src-tauri/src/models/paths.rs:80-103`。
- `src-tauri/src/services/search_service.rs:511-513`：search 每文件 to_relative + load。
- `src-tauri/src/services/file_store.rs:156-157`：hash 前 resolve。

影响范围：

- 全量 scan。
- Search。
- Graph build。
- Lint/compile/import 中大量路径 resolve。

建议：

- `ProjectContext` 增加 `canonical_root: Option<PathBuf>` 或构造时缓存。
- 从 `list_markdown_files` 返回的 trusted absolute paths 可走批量相对路径转换。
- 保持 symlink escape tests 不退化。

## 5. 必须修、值得修、可以以后修

### 必须修

1. 修复当前测试失败和 Graph/Sigma jsdom 噪声。
2. `waitForTaskTerminal` 增加轮询兜底、timeout、listen fail handling。
3. 拆首屏重模块：至少 lazy load Graph、Wiki/Milkdown、Import/Readability、Markdown renderer。
4. 建立基础 wiki index，避免 Search/Chat/Graph 重复全量读盘。
5. 优化 Graph reducer 中的 O(E*N) 重复计算。

### 值得修

1. Task log/progress 持久化节流和日志分页。
2. Chat streaming chunk/batch。
3. Graph tag edge budget。
4. Import source replacement 确认前轻量化。
5. Tauri command use-case 下沉。
6. AppShell controller 拆分。

### 可以以后修

1. Rust 大 service 文件按纯函数和子领域逐步拆分。
2. 前端 `hasTauri/errorMessage/projectScope` 工具统一。
3. 路径 canonical root 缓存。
4. bundle budget、性能基准、500 页 fixture。
5. Graph layout 更完整地纳入 backend task progress/cancel 语义。

## 6. 分阶段治理计划

### P0：稳定性与性能火点

目标：让测试可信、任务不会挂、首屏不背重依赖、Graph/Search/Chat 不做明显重复工作。

建议顺序：

1. 测试修复：
   - stub `HTMLCanvasElement.prototype.getContext` 或让 Graph renderer factory 在测试中替换。
   - 修复 `Collapse sidebar` 测试与当前 UI 行为不一致。
   - 重新跑 `npm run test`，确保没有 jsdom WebGL 噪声。

2. 任务等待修复：
   - `waitForTaskTerminal` 加 polling + timeout。
   - Import preview 和 Graph build 覆盖丢事件场景单测。

3. 首屏拆包：
   - `AppShell` 视图层改 lazy。
   - `WikiEditor` 仅编辑模式加载。
   - Readability 改 URL import 路径动态 import。
   - 生成 fresh build，记录 chunk 大小。

4. wiki index MVP：
   - 在 backend 增加 per-project in-memory index。
   - index entry 包含 path、mtime、size、hash、title、frontmatter fields、wikilinks、body excerpt。
   - Search、Chat retrieval、Graph hash 先复用 index。

5. Graph render 热点：
   - reducer 使用预计算 snapshot。
   - hidden set 每次 refresh 只算一次。
   - 验证 `skipIndexation` 使用条件。

### P1：架构瘦身

目标：不改产品行为，把编排职责移到合适层，降低后续迭代阻力。

建议顺序：

1. `AppShell` 瘦身：
   - `useImportController`
   - `useProviderCapabilities`
   - `useAgentRunController`
   - `WorkspaceView` 只做布局和 view boundary。

2. Command 下沉：
   - `ChatSendUseCase`
   - `ChatConvenienceUseCase`
   - `ImportPreviewUseCase`
   - `CompileUseCase`
   - `ExportUseCase`

3. Task/log 管理：
   - task JSON 与 log stream 分离。
   - 前端 Task drawer 支持 lazy log loading。
   - progress 更新节流。

4. Graph 模块化：
   - renderer hook。
   - render model pure utils。
   - layout worker wrapper。
   - export 与 filter 状态共用同一个 visibility model。

### P2：长期质量工程

目标：防止性能和复杂度再次回潮。

建议：

1. 500 页 synthetic wiki fixture：
   - Search latency。
   - Graph open/rebuild latency。
   - Chat retrieval latency。
   - Task log 1000/10000 行压力。

2. Bundle budget：
   - main chunk 上限。
   - Graph/Milkdown/Markdown renderer async chunk 单独记录。
   - CI 中输出 chunk table。

3. Rust service 逐步拆分：
   - search index / frontmatter parser / link rewriter。
   - import artifact index / extraction adapters。
   - task persistence / event bus。

4. 前端平台工具：
   - `hasTauri` 单点。
   - `errorMessage` 单点。
   - `invokeProject` 单点。
   - `withProjectScope` 或 typed scope helper。

5. 性能观测：
   - backend task duration。
   - file scan count。
   - graph node/edge count。
   - chat retrieval hit count and total chars。

## 7. 风险提示

1. 不建议把 wiki content 放进数据库。项目硬规则是 Markdown + JSON + local files；index/cache 也应是 `.app/` JSON 或内存派生物。
2. 不建议先大拆 Rust service。当前更高收益是让热路径停止重复读盘。
3. 不建议先做 UI 视觉重构。当前 UI 已基本贴近 design shell，性能和职责边界更紧急。
4. Graph 不能只靠 WebGL 优化。当前 reducer 里的重复全图扫描先要消掉。
5. 测试噪声必须优先清理，否则后续任何“优化通过”都不可信。

## 8. 附：本次审查的关键证据索引

入口与 bundle：

- `src/main.tsx:4-14`
- `src/app/App.tsx:3-13`
- `src/components/app/AppShell.tsx:5-14`
- `src/features/graph/GraphView.tsx:1-5`
- `src/features/wiki/WikiEditor.tsx:15-30`
- `src/features/chat/MessageContent.tsx:3-7`
- `src/features/wiki/MarkdownReader.tsx:3-7`
- `vite.config.ts:13-17`

Search / Chat / Graph scanning：

- `src-tauri/src/services/search_service.rs:37-44`
- `src-tauri/src/services/search_service.rs:487-513`
- `src-tauri/src/services/search_service.rs:629-635`
- `src-tauri/src/services/search_service.rs:655-671`
- `src-tauri/src/services/search_service.rs:702-723`
- `src-tauri/src/services/file_store.rs:151-157`
- `src-tauri/src/services/file_store.rs:238-242`
- `src-tauri/src/commands/graph_commands.rs:17-20`

Graph render：

- `src/features/graph/GraphView.tsx:116-148`
- `src/features/graph/GraphView.tsx:468-485`
- `src/features/graph/GraphView.tsx:512-516`
- `src/features/graph/GraphView.tsx:565-566`
- `src/features/graph/GraphView.tsx:581-610`

Task / logs：

- `src-tauri/src/tasks/task_service.rs:255-306`
- `src-tauri/src/tasks/task_service.rs:453-470`
- `src/stores/taskStore.ts:88-93`
- `src/hooks/useTaskEvents.ts:24-58`

复杂度热点：

- `src/components/app/AppShell.tsx`
- `src-tauri/src/commands/chat_commands.rs`
- `src-tauri/src/commands/compile_commands.rs`
- `src-tauri/src/commands/import_commands.rs`
- `src-tauri/src/commands/export_commands.rs`
- `src-tauri/src/services/lint_service.rs`
- `src-tauri/src/services/import_service.rs`
- `src-tauri/src/services/search_service.rs`
- `src-tauri/src/services/extraction_service.rs`
- `src/features/graph/GraphView.tsx`
- `src/features/wiki/wikiStore.ts`
- `src/stores/lintStore.ts`
- `src/stores/chatStore.ts`
