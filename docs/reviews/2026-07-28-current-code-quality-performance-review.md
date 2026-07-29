# LLM Wiki Desktop 当前代码质量、性能与功能缺陷审查

审查日期：2026-07-28
审查对象：`master` / `5714059` 加当前未提交工作区
审查方式：只读静态审查，不修改应用代码，不运行测试、构建或 benchmark

## 1. 结论

当前代码不是“完全不可维护”，不少基础边界已经做得比早期版本扎实：项目上下文有可信根路径，用户内容仍保持 Markdown/JSON/local files，Source 和 Import 有较强的路径、hash、事务与确认约束，视图已做 lazy loading，Wiki 内存索引和 Graph render snapshot 也解决了上一轮审查中的一部分明显热点。

但当前仍不能把它视为低风险、性能闭环的实现。本次确认：

| 等级 | 数量 | 结论 |
|---|---:|---|
| P0 | 0 | 未发现正常路径下可立即复现的安全灾难或必然全局数据损坏 |
| P1 | 13 | 涉及 Compile 回滚覆盖外部编辑、确认状态机悬挂、Source/Wiki/Task 竞态、受限内容导出死路、1 GiB 媒体内存峰值、日志写放大、后台 worker 阻塞和 Git checkpoint 并发 |
| P2 | 13 | 涉及跨项目陈旧 UI、伪分页、缓存一致性、恢复不落盘、流式渲染、重复探测、Promise 错误恢复和树渲染复杂度 |
| P3 | 1 | 可访问性语义不完整 |

最高优先级不是继续扩功能，而是先收紧三条主线：

1. 让 Compile / Task / Git 的“状态变更—持久化—发布事件—失败恢复”成为单一、可证明的状态机。
2. 让所有异步 UI 操作同时绑定 `projectKey + entityId + operationEpoch`，而不是只检查“项目仍相同”。
3. 去掉长任务热路径中的全量 JSON 重写、整文件进内存、全局锁内 I/O，以及 Tokio worker 上的同步长阻塞。

## 2. 审查范围与可信度边界

### 2.1 已读取的项目合同

审查先加载了 `skills/llm-wiki-desktop-context`，并以以下文档作为行为合同：

- `AGENTS.md`
- `SPEC/PRD.md`
- `SPEC/SPEC.md`，特别是 §16
- `SPEC/APP_flow.md`
- `SPEC/TECH_STACK.md`
- `SPEC/BACKEND_STRUCTURE.md`
- `SPEC/FRONTEND_GUIDELINES.md`
- `SPEC/DESIGN.md`
- `docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`
- `skills/llm-wiki-desktop-context/references/project-map.md`
- 最新 `SPEC/progress.txt` 和相关 `SPEC/gotchas.txt`

### 2.2 工作区状态

审查开始时工作区有 348 个 status entry；已跟踪 diff 为 267 个文件、约 30,122 行新增和 14,946 行删除，另有未跟踪文件和删除项。因此：

- 本报告针对的是“当前工作区快照”，不是干净 commit。
- 行号会随当前大规模改动继续漂移。
- 现有历史检查结果不能自动证明这一工作区仍通过同一 gate。

### 2.3 方法

- 主审按调用链复核代码和相关测试。
- 前端、后端、跨层三个独立只读审查并行进行；主审对纳入报告的结论重新检查了代码证据。
- 没有仅凭“大文件”判定 bug；只有能给出触发条件和影响的路径才记为功能或性能缺陷。
- 用户要求只读，因此未运行 `npm run check:quick`、`npm run check`、Cargo 测试或运行时 benchmark。本报告中的复杂度和资源结论来自代码路径，不是实测延迟。

## 3. P1：应优先修复

### P1-01 Compile 回滚会覆盖并发外部编辑，且多处吞掉回滚失败

**证据**

- `src-tauri/src/services/compile_service.rs:981-1007` 只保存输出写入前的旧字节。
- `src-tauri/src/services/compile_service.rs:1010-1032` 回滚时直接 `write` 或 `remove_file`，没有验证当前文件仍是本次 Compile 写出的版本，也没有原子事务或 CAS。
- `src-tauri/src/commands/compile_commands.rs:179-185`、`845-860`、`972-983` 的部分错误路径用 `let _ = restore_outputs(...)` 丢弃回滚错误。

**触发**

Compile 已写入部分文件，随后另一文件、Source 消费记录、task 持久化或收尾步骤失败；在写入和回滚之间，外部编辑器或另一后台任务又改了已写路径。

**影响**

- 回滚把用户刚做的外部编辑覆盖成 Compile 之前的旧快照。
- 回滚失败时，调用方可能只报告最初错误，留下半套新输出而不告诉用户残留路径。
- 直接违反“保留外部 Markdown 编辑”和高风险操作必须可恢复的合同。

**建议**

- 首选把 apply、Source 消费记录和必要 cache 更新纳入同一 `FileTransaction`。
- 若仍保留补偿式回滚，备份必须记录“本次写入后的 hash”，只有当前 hash 仍匹配才允许恢复。
- 回滚冲突或失败必须成为一等错误，返回残留路径和可执行恢复动作，不能 `let _`。

### P1-02 Compile 确认流先消费 action、再做可失败工作，可永久留下 Running task

**证据**

- `src-tauri/src/models/confirmation.rs:160-199` 的 `confirm` 在 `:183` 立即从 registry 删除 action。
- 旧确认流 `src-tauri/src/commands/compile_commands.rs:901-953` 先消费 action，再把 task 切到 `Running`。
- 随后的项目解析、Source 重校验和备份在 `:954-972` 都可能用 `?` 直接返回；Source stale 分支不会把 task 收敛到终态。
- 新冲突流也在 `:837-845` 消费 action、切 `Running` 后才备份；备份失败同样没有统一 finalizer。
- `:871-881` 等路径在标记 Failed 前又可能因回滚失败提前返回。

**触发**

确认对话框打开期间 Source 被消费、项目上下文失效、文件被锁定、备份读取失败或恢复失败。

**影响**

task 没有 worker 继续处理，却永久显示 `Running`；action 已不存在，用户无法重新确认，只能等重启 recovery 把它判失败。

**建议**

- 使用 `peek/claim/release/finish` 两阶段确认。
- 所有可失败校验和备份尽量在 action consume 和 `Running` transition 之前完成。
- transition 之后必须由统一 guard/finalizer 保证每条返回路径都进入 `WaitingForConfirmation`、`Failed`、`Cancelled` 或 `Succeeded`，禁止裸 `?` 越过终态。

### P1-03 通用 task 状态迁移先改内存、发事件，最后才落盘

**证据**

- `src-tauri/src/tasks/task_service.rs:241-280` 先修改 task 状态和时间，随后在 `:277-278` 发出 completed/failed/cancelled/updated 事件，最后 `:280` 才持久化。
- 持久化失败时没有恢复内存，也无法撤回已经发给 UI 的终态事件。
- 同文件 `complete_running_with_result` 在 `:490-527` 已经采用“持久化失败回滚、成功后再发事件”，说明当前内部存在两套耐久语义。

**触发**

磁盘满、目录权限变化、临时文件 rename/fsync 失败。

**影响**

UI 可以显示成功或失败，但磁盘仍是旧的 `Running/Queued`；重启后状态反转，调用方还可能把同一个 task 再次标 Failed，形成自相矛盾的事件序列。

**建议**

统一所有 task mutation 的协议：

1. 在锁内生成 previous/next snapshot。
2. 锁外持久化 next snapshot。
3. 失败时 CAS 恢复 previous；成功后才发布事件。
4. 对持久化失败本身提供明确的 fatal task error，而不是让调用方各自补救。

### P1-04 Source 异步操作没有稳定的 source 身份，可能错目标、回跳或永久 busy

这是本轮前端最危险的一类系统性竞态。

**证据**

- `src/features/wiki/WikiView.tsx:322-331` 对 A 调用 `loadSourceDetail(A)` 后，不验证返回身份就继续打开移动对话框或调用删除预览。
- `src/features/wiki/sourceStore.ts:110-131` 的 stale `loadDetail` 会静默 resolve；后续 `previewMove` / `previewDelete` 在 `:327-391` 又从可变的 `get().detail` 读取当前 Source。
- `applyCandidate` 在 `:246-268` 只验证 project scope，之后主动 `loadDetail(result.sourceId)`，可把用户从 B 拉回 A。
- `discardCandidate` 在 `:275-300`、`reprocess` 在 `:133-167` 遇到同项目 Source 切换时直接 return，却不清除 `mutating`；`loadDetail` 的 `:113-121` 也不重置 `mutating`。
- `SourceRightPanel.tsx:123-141` 的旧 Promise 在组件/Source 切换后仍可执行 `onMutation`；`RightContextPanel.tsx:284-296` 会继续 reload/open 旧路径。

**触发**

对 Source A 发起移动、删除预览、OCR/ASR、候选应用、丢弃或版本恢复，在 IPC 返回前快速选择同项目 Source B。

**影响**

- “对 A 点删除”可能实际生成 B 的删除预览。
- 旧 A 操作完成后把右栏和 Wiki 页面重新拉回 A。
- B 的 Source 面板可能永久保持禁用/转圈。

删除仍有后端确认和 guard，但 UI 发起对象已经错位，不能把这一点视为可接受。

**建议**

- 所有 Source API 显式接收并返回 `sourceId`，后续步骤不得重新读取全局 `detail` 决定对象。
- 建立 `{ projectKey, sourceId, operationToken }`；只有 token owner 能提交 presentation state 或清除 busy。
- stale mutation 的“后端事实”可保留，但不得导航、覆盖详情或控制新 Source 的 busy。

### P1-05 Wiki 快速切页是 last-response-wins，不是 last-request-wins

**证据**

- `src/features/wiki/wikiStore.ts:156-176` 的 `openPage` 只捕获 project scope。
- 每次点击会立即写 `selectedPath`，但响应提交时不校验 path 或 request epoch。

**触发**

先打开 A，紧接着打开 B；B 响应先回来，A 响应后回来。

**影响**

树选中 B，但 `page` / `draft` 是 A。用户随后进入编辑会在错误上下文中操作；即使最终保存仍以 page meta 为准，界面所见和被修改对象也已不一致。

**建议**

为 `openPage` 增加 per-request epoch，并在 success、error、loading commit 时同时验证 `projectKey + requestedPath + epoch`。

### P1-06 多个后台任务启动路径会在切项目时丢弃合法 task fact

项目合同已经明确：有效 backend task 必须先无条件进入全局 task store，只有 drawer、toast、navigation 等 presentation 行为受 project scope 限制。

**证据**

- 正确参照：`src/hooks/useTaskLauncher.ts:65-73` 先 `upsertTask`，再按 request key 决定是否打开 drawer。
- Source AI 在 `src/features/wiki/sourceStore.ts:169-223` 仅在 current scope 时 upsert。
- Deep Lint 在 `src/stores/lintStore.ts:257-280` 把完整返回降成 `{ id }`，scope 失效直接丢弃。
- Export 在 `src/stores/exportStore.ts:108-162` 同样只取 id 且不 upsert；后端实际在 `src-tauri/src/commands/export_commands.rs:64-112` 返回完整 `BackendTask`。
- Lint/Exports view 再通过额外 `list_tasks/get_task` 和类型断言补洞，增加 IPC 和时序依赖。
- `src/hooks/useTaskEvents.ts:61-102` 的全局监听 effect 依赖 `projectId`，切项目时会拆除并异步重新注册，存在漏事件窗口。

**触发**

启动 IPC 已在后端创建 A task，但响应尚未回前端时切到 B；同时 task created/updated 事件落在监听重建窗口。

**影响**

后台任务继续运行，但当前进程的任务中心可能没有卡片，用户看不到早期状态、确认入口，也不能方便地选择或取消。

**建议**

- 所有 `start_*` adapter 统一返回 `BackendTask` 并立即无条件 upsert。
- 复用一个 typed TaskLauncher；只 guard feature-local `runningTaskId`、drawer、toast 和导航。
- 全局 task event listener 应在 App 生命周期只注册一次；项目 task recovery 另设独立 scoped effect。

### P1-07 项目 task recovery 没有 request key，可被旧项目响应覆盖

**证据**

- `src/hooks/useTaskEvents.ts:104-111` 在项目变化时发起恢复。
- `src/stores/taskStore.ts:284-298` 调用 `set_active_project` 后，无 project key/epoch 就 `setTasks`；`finally` 也无条件把 `tasksHydrated` 设为 true。
- 后端 `src-tauri/src/commands/task_commands.rs:115-140` 同时修改 TaskService 的 active project root。

**触发**

A→B 快速切换，两个恢复 IPC 的完成顺序与发起顺序不同。

**影响**

B 可以显示 A 的 snapshot；A 的 finally 可在 B 尚未恢复完时错误解除 hydration。后端 active root 也存在被迟到调用回写的风险。

**建议**

恢复请求绑定 project key 和 epoch；snapshot/hydration commit 必须 guard。后端 active-project mutation最好带 generation，拒绝旧 generation。

### P1-08 Wiki 的 Generate/Regenerate HTML 对受限内容是不可恢复死路

**证据**

- 后端在 `src-tauri/src/commands/export_commands.rs:64-76`、`:99-134` 强制校验 restricted-content acknowledgement，这是正确的安全门。
- 独立 Exports 流程已经有预检和 acknowledgement UI。
- Wiki 的 `src/features/wiki/GenerateHtmlDialog.tsx:8-79` 没有预检、警告或 acknowledgement 字段。
- `src/features/wiki/WikiView.tsx:258-277` 关闭对话框并立即切到 preview，调用默认参数。
- `src/stores/exportStore.ts:108-162` 的默认 `acknowledgeRestrictedContent` 是 false。
- Wiki preview 在 `WikiView.tsx:577-596` 不展示 export store error，也没有确认后重试入口。

**触发**

对受限 Source、含受限来源的页面或 project report，从 Wiki 入口生成或重新生成 HTML。

**影响**

后端正确拒绝，但 UI 已进入空 preview；用户看不到原因，也无法在原流程确认并重试，只能偶然发现 Exports 页面这一绕路。

**建议**

抽出 Wiki/Exports 共用的 restricted-content preflight + acknowledgement 对话框；失败时保留原 mode 并显示 typed error。

### P1-09 远程原始媒体虽流式下载，Commit 又把最多 1 GiB 整体读回内存

**证据**

- `src-tauri/src/services/import_v2/generic_web_engine.rs:1820-1869` 使用 `fetch_to_file` 流式下载，这是正确方向。
- 同路径允许 `max_response_bytes = 1024 * 1024 * 1024`，见 `:1832-1848`。
- `src-tauri/src/services/import_v2/commit.rs:1073-1139` 为所有 preview assets 建立 `Vec<(String, Vec<u8>, ...)>`，并同时保留全部 artifact bytes。
- `verified_artifact` 在 `commit.rs:3042-3095` 对每个文件 `read_to_end` 后再 hash。
- 所有 bytes 直到 `commit.rs:1706-1710` 才进入 transaction 写盘。
- remote-media retention 只做磁盘空间规划，没有 commit 内存预算。

**触发**

用户明确选择保留一个接近 1 GiB 的视频，或一次 preview 含大量较大图片/附件。

**影响**

单个 asset 就可能产生约 1 GiB Rust heap 峰值；多个 asset 会累计保留，容易 OOM、进程被杀或桌面完全失去响应。流式下载带来的内存优势在 commit 阶段被完全抵消。

**建议**

- `FileTransaction` 增加 file-backed entry：从已验证 file handle/path 流式 hash + copy，不返回 `Vec<u8>`。
- 一次只处理一个 artifact，设置 per-commit memory budget。
- 保留现有 TOCTOU 防护时，应依赖打开的 handle、metadata 和最终路径验证，而不是把文件读入 Vec。

### P1-10 Task 日志从后端落盘到关闭的前端抽屉都存在累计写/算放大

这是 `docs/audits/2026-07-06-performance-complexity-audit.md` 已记录但仍未解决的热点。

**后端证据**

- `src-tauri/src/tasks/task_service.rs:285-336` 的每次 progress/log 都同步调用 `persist_current_task`；activity 在 `:360-376` 也相同。
- `persist_task` 在 `:641-665` 持有全局 tasks 读锁，克隆全部 logs/activities，再 pretty serialize、原子写和 `sync_all` 整个 JSON。
- logs/activities 没有 retention 上限。
- Agent 每行输出会触发 append，见 `src-tauri/src/services/agent_service.rs:1573-1628`。

**前端证据**

- `src/stores/taskStore.ts:72-88`、`:144-153` 每条事件复制现有数组并永久追加。
- `src/components/app/TaskLogDrawer.tsx:162-250` 在抽屉关闭时仍订阅全部 tasks/logs/activities/output；每个 log 更新会 flatMap、全量 sort 后才 slice 24。
- 真正的 `if (!drawerOpen) return null` 直到 `TaskLogDrawer.tsx:400`。
- 打开运行中 task 后，`:279-294` 还每 2 秒重新拉取全量 logs/activities；旧 snapshot 可覆盖事件刚追加的 tail。

**影响**

L 条日志的累计后端序列化/写盘和前端数组复制接近 O(L²)；batch 聚合还叠加排序。长 Agent/Import/Lint task 会造成磁盘写放大、全局 task lock 争用、内存增长和 UI 卡顿，即使 drawer 从未打开。

**建议**

- task JSON 只存有界 metadata snapshot；完整日志改为 append-only JSONL/log chunks。
- progress 250–500 ms 或按百分比/阶段节流，terminal 状态立即落盘。
- 锁内只 clone 必要 snapshot，所有 I/O 放锁外。
- 前端仅保留有界 tail，drawer 打开后分页；关闭状态不订阅 logs。
- batch recent logs 增量维护，不在每条 log 上重扫全部历史。

### P1-11 同步长任务运行在 Tauri/Tokio async worker 上

**证据**

- Graph 在 `src-tauri/src/commands/graph_commands.rs:42-44` 用 `tauri::async_runtime::spawn`，内部 `:65-118` 同步扫描文件、建图和写盘。
- Source AI 在 `src-tauri/src/commands/source_commands.rs:193-196` spawn async task，Agent 路径在 `:396-410` 同步调用进程 runner。
- `src-tauri/src/services/agent_service.rs:1047-1189` 使用同步 child process 轮询并每 50 ms `thread::sleep`，最长 15 分钟。
- Import commit 在 `src-tauri/src/commands/import_v2_commands.rs:697-737` 的 async task 内直接执行同步的大批量事务。
- Compile、Export、Lint 存在同类 async orchestration + 同步 FS/Git/process 路径。

**影响**

并发几个 Agent/Compile/Graph/Import 后会占住 runtime worker；真正异步的 BYOK 网络 future、timer、进度/取消和普通 IPC 可能延迟或饥饿。“后台运行”不等于“不阻塞调度器”。

**建议**

把 process、Git、同步文件 I/O 和 CPU 图计算放入 `spawn_blocking` 或专用 bounded worker pool；async task 只做真正异步 I/O 和状态协调。

### P1-12 Import V2 由一个全局 mutation lock 串行所有项目，批量 commit 还重复加载和线性查找

**证据**

- 唯一全局锁定义在 `src-tauri/src/services/import_v2/orchestrator.rs:48-57`，同一个 service 上有约 19 个 `self.lock()` 获取点。
- `load_session` 在 `orchestrator.rs:751-767` 也拿 mutation lock，并执行 `preflight_locked`；后者在 `:3385-3387` 调 `FileTransaction::reconcile_project`。
- batch commit 在 `src-tauri/src/services/import_v2/commit.rs:641-795` 从 `:672` 开始全程持锁。
- history snapshot 通过 decisions × session items 的线性 find 构造，见 `:698-709`。
- 每个 decision 调一次 `commit_one`；`commit_one` 在 `:798-820` 又重新 load/parse 整份 session 并线性寻找 item。
- `commit_one` 本身约 1,003 行，期间还执行 artifact 验证、transaction、SourceRegistry、history 和 Git。

**影响**

- 大批量 N 项导入出现明显 O(N²) 查找和高常数全量 JSON/事务写放大。
- 项目 A 的大媒体/大批量 commit 会阻塞项目 B 的 Import/Source session 操作。
- presentation read 也可能因 `load_session + preflight` 被 mutation lock 和恢复扫描拖住。

**建议**

- 锁按 canonical project root，再按 session/source 分片。
- 单次 batch 加载一次 session，并预建 `item_id -> index`。
- 一次规划 batch，按 Source 缩短事务临界区；history summary 增量写入。
- recovery 只在项目打开或明确任务开始时执行，不在普通 presentation read 上重复 reconcile。

### P1-13 Git checkpoint 没有按项目串行化

**证据**

- `src-tauri/src/services/git_service.rs:115-150` 的 checkpoint 是 status → `git add --all` → commit 多步序列。
- scoped checkpoint 在 `:166-204` 同样是 status → add → commit。
- `GitService` 是无状态 unit struct，没有 per-project lock/coordinator。
- Compile、Source、Import、Lint 等后台流程都可能并发调用。

**触发**

同一项目两个高风险后台任务同时创建 checkpoint 或在 checkpoint 序列中修改工作区。

**影响**

- `.git/index.lock` 冲突。
- 一个任务把另一个任务的中间产物暂存/提交。
- checkpoint hash 与它声称保护的范围错位，直接削弱“高风险操作先建 Git checkpoint”的安全保证。

**建议**

建立以 canonical project root 为 key 的 Git coordinator，锁覆盖完整 status/add/commit/unstage 序列；尽量使用精确 path scope，减少 `add --all`。

## 4. P2：应进入近期治理队列

### P2-01 顶栏搜索结果会跨项目泄漏

`src/components/app/TopBar.tsx:158-190` 只使用组件内 request sequence，不绑定 project key；`:192-196` 点击时却使用“当前项目”打开结果。A 搜索未返回或结果弹层仍打开时切到 B，A 结果会显示在 B，并尝试用 B root 打开 A path。项目变化时应 bump epoch、清空结果，并给结果绑定 origin project key。

### P2-02 Graph build 返回后会先写旧项目 UI，再检查 scope

`src/stores/graphStore.ts:248-269` 在 build IPC 返回后无条件 upsert task 是正确的，但 `:258-266` 也无条件写 `buildUi`；直到等待 terminal 后才检查 project scope。A build pending 时切 B，B 的 Graph UI 会被 A taskId/loading 污染，且 stale return 后可能留下 loading。每次 presentation write 前都应检查 project key/epoch。

### P2-03 Graph layout save queue 没有项目/拓扑生命周期

`src/features/graph/GraphView.tsx:150` 创建的 queue 在项目变化时不取消；cleanup `:294-302` 只销毁 renderer。pending callback 在 `:270-273` 执行时读取当前全局 `live.data`，却继续使用旧 graph 和旧 project path。`graphLayoutSaveQueue.ts:1-45` 没有 cancel/invalidate。通常会静默丢掉 A 最新布局；若 A/B 是相同 hash 的克隆项目，还可能把 A layout 响应提交到 B。queue 应绑定 `{projectKey, contentHash}` 并在 cleanup/reset 时失效。

### P2-04 Import History 的 cursor 分页仍然先全量读、全量解析、全量排序

`src-tauri/src/commands/import_v2_presentation_commands.rs:675-717` 在 `take(limit)` 前先 `read_v2_history`、读取 legacy、合并并排序全部记录。`read_v2_history` 在 `:812-978` 枚举和读取每个 JSON，先解析 `Value` 再解析完整 `ImportBatchResult`；缺 history snapshot 时还逐条 `load_session`，并查询 task/metadata。每次“加载更多”仍是 O(历史总量)，cursor 只限制返回量，不限制工作量。

建议写入轻量 immutable history summary/index，先按 metadata/cursor 选 `limit + 1`，只读取选中记录；legacy scan 单独缓存。

### P2-05 重启 recovery 把中断 task 改 Failed，却不回写磁盘

`src-tauri/src/tasks/task_service.rs:684-747` 在内存中把 Running/Queued/Cancelling/WaitingForConfirmation 改为 Failed，但只插入内存，不持久化。下次重启仍读到 Running，再次 recovery 并刷新失败时间。应先原子写 terminal recovery snapshot，再发布/返回。

### P2-06 WikiIndex 只凭 mtime + size 判断内容未变

`src-tauri/src/services/wiki_index.rs:127-168` 在 `:146-153` 仅比较 `(mtime_secs, mtime_nanos, size)`。粗粒度文件系统、保留 mtime 的编辑器/同步工具，或同一时间粒度内的等长编辑会永久复用旧正文/hash，Search/Graph/Chat 都继续看到旧内容。建议引入 watcher generation、ctime/file id，或在不可靠 token 上做 hash fallback。

### P2-07 absolute atomic JSON writer 的固定 tmp 名会被并发 Graph writer 互踩

`src-tauri/src/services/file_store.rs:188-197` 的 `write_json_atomic_absolute` 没走文件内已有的全局 guarded write lock；底层 `write_atomic` 在 `:328-357` 固定使用 `.<filename>.tmp`。Graph `rebuild/write_cache/save_layout` 都可能写 `.app/graph-cache.json`。并发 writer 会 truncate/rename 同一个 tmp，导致失败或旧 cache/layout 覆盖新值。应使用 unique tmp + same-path lock/CAS。

### P2-08 每个 stream delta 都让整个 WorkspaceController 重渲染

`src/components/app/WorkspaceController.tsx:31-33` 永久订阅全部 tasks、activities、taskOutputs，并把它们传给 router；`src/stores/taskStore.ts:90-100` 每个 delta clone map 并拼接最多 512 KiB。App 同时挂 `useTaskEvents` 和 `useChatStream`，二者都监听 `task://stream-output`；chat 在 `src/stores/chatStore.ts:720-750` 继续使用无上限 `streamingText + delta`。

长回答会产生字符串累计复制和非 Agent 页面高频 render。建议建立单一事件分发，按 RAF/30–50 ms batch delta；task output 只由需要它的 route 按 task selector 订阅，chat 使用 chunks 或有界 buffer。

### P2-09 `useProjectStatus` 冷启动不能合并 in-flight，完成后又永久不失效

`src/hooks/useProjectStatus.ts:28-68` 只缓存已完成 snapshot。BottomStatusBar、LeftSidebar、RightContextPanel 同时挂载时，cache 为空会各发一次 git_status/detect_agents/list_llm_providers；进入 Agent/Settings 时 `useAiCapabilities` 又重复探测。最坏冷启动约 11 个 IPC/进程探测。cache 完成后没有 provider/git/task 事件 invalidation，状态又可能永久陈旧到切项目。

建议做 project-keyed single-flight external store，并与 capability store 共享 agents/providers。

### P2-10 Export preview 不是 last-click-wins，Clear 也会被 pending response 反转

`src/stores/exportStore.ts:166-180` 只有 project scope，没有 preview id/epoch。A→B 快点且响应逆序时 A 覆盖 B；读取 pending 时点 Clear，返回后仍会重新打开。应为 load/clear 共用 preview request epoch。

### P2-11 Bootstrap 的迟到错误会清空用户已成功打开的项目

`src/stores/projectStore.ts:205-227` 只在 success 自动打开前检查 `bootstrapEpoch`，catch 无条件把 `currentProject` 设为 default。`ProjectStartView.tsx:269-285` 的 quick actions 在 initializing 时未禁用。bootstrap A pending 时用户成功打开 B，随后 A 失败会把 B 踢回启动页。catch/finally 也必须验证 selection epoch。

### P2-12 Import collection / remote-media dialog 存在 unhandled Promise rejection

`src/features/import/useImportWorkflow.ts:392-425` 的 load page 和 `:503-526` 的 remote confirm 没有 catch；collection confirm 在 `:352-390` toast 后重新 throw。组件在 `ImportCollectionDialog.tsx:69-78`、`:170-191`、`:208` 和 `ImportRemoteMediaDialog.tsx:41-48`、`:108` 使用 `void async`/`finally`，没有 catch。

IPC 失败后 spinner 会停，但会产生 unhandled rejection；部分路径没有就地错误，选择状态和重试说明也不清楚。workflow 和 UI boundary 应明确约定“谁捕获、谁展示”，不能两边都假设对方处理。

### P2-13 WikiTree 的 folder row 重复递归扫描子树

`src/features/wiki/WikiTree.tsx:338-390` 每个 folder render 都调用递归 `countVisibleLeaves`。深链/偏斜目录会在单次 render 接近 O(N²)，每次 filter keypress 重复。应一次 post-order 生成 visible count/pruned tree，再渲染；更大树再考虑扁平化和虚拟化。

## 5. P3：可访问性

### P3-01 部分交互缺少正确可访问名称或语义

- Graph 搜索：`src/features/graph/GraphControls.tsx:59-65`
- Wiki filter：`src/features/wiki/WikiTree.tsx:102-107`
- folder disclosure：`WikiTree.tsx:343-377` 缺 `aria-expanded/aria-controls`
- TopBar 使用 `role=listbox`，但按钮项 `TopBar.tsx:295-302` 没有 `role=option/aria-selected`

建议按真实键盘模型补标准 searchbox/disclosure/listbox 语义；若不实现 listbox 交互，应移除错误 role。

## 6. “屎山”证据：问题不只是文件大

### 6.1 Import 已形成 God Service / God Function

当前大文件规模：

| 文件 | 当前总行数 | 结构信号 |
|---|---:|---|
| `services/import_v2/orchestrator.rs` | 7,566 | 约 4,605 行生产实现；状态机、授权、route、session、task、recovery 混合 |
| `services/import_v2/commit.rs` | 6,025 | 约 3,258 行生产实现；`commit_one` 约 1,003 行 |
| `services/agent_service.rs` | 3,123 | invocation、环境隔离、进程、parser、活动、平台解析混合 |
| `services/import_v2/source_lifecycle.rs` | 2,906 | Source preview/mutation/version/transaction 混合 |
| `services/import_v2/transaction.rs` | 2,803 | 跨平台事务、恢复和多种写模式集中 |
| `commands/import_v2_presentation_commands.rs` | 2,282 | presentation command 内含 history scan、capability 安装、路径/媒体/系统资源逻辑 |

真正的坏味道不是 7,566 行本身，而是：

- 一个全局锁覆盖所有项目和多类 use case。
- `run_item`、`commit_one` 等单函数承载状态机、I/O、Git、history、task 和错误降级。
- read path 会触发 preflight/reconcile，presentation 与 mutation 边界变模糊。
- 为保证正确性加入更多局部 guard/hook 后，分支数继续增长，任何错误路径都容易漏终态。

不建议一次性大重写。应以现有 facade 为兼容层，按 use case 做 strangler refactor：`ImportSessionUseCase`、`ImportCommitPlanner/Executor`、`ImportHistoryIndex`、`SourceMutationCoordinator`。

### 6.2 Tauri command 层仍然过胖

项目合同要求 thin command → typed DTO → service。当前 `chat_commands.rs`、`compile_commands.rs`、`import_v2_presentation_commands.rs`、`export_commands.rs` 仍承担 provider route、Git checkpoint、pending action、rollback、文件解析和 task terminalization。

直接后果已经出现在 P1-01/P1-02：同一状态机的错误收尾散落在多个 command 分支，部分路径 `let _`，部分路径裸 `?`，部分路径手工 transition。

建议每次触碰一个 command 时抽一个完整 application use case，不做“只为减行数”的机械拆文件。

### 6.3 前端的 project-scope guard 数量很多，但 entity-scope 抽象缺失

当前前端约有：

- 63 次 `captureProjectScope(...)`
- 140 次 `isProjectScopeCurrent(...)`
- 分布在 10 个主要文件

这说明团队已经意识到项目切换竞态，但 guard 仍是手工、局部和 presentation-specific 的。它只能回答“还在同一项目吗”，不能回答：

- 还是同一个 Wiki path 吗？
- 还是同一个 Source/candidate/version 吗？
- 还是最近一次 preview/build/recovery 吗？
- 当前 busy flag 属于哪个操作？

P1-04、P1-05、P1-07 和多个 P2 正是这个抽象缺口的结果。建议统一：

```text
OperationScope {
  projectKey,
  entityType,
  entityId,
  epoch
}
```

task fact 始终全局提交；只有 presentation commit 检查 OperationScope。

### 6.4 Task 子系统存在多套耐久顺序

同一个 `TaskService` 中同时存在：

- mutate → emit → persist
- mutate → persist → emit
- mutate → persist failure rollback → emit
- recovery mutate → 不 persist

这不是单个 bug，而是缺少统一 task transaction protocol。后续再加 confirmation、activity、stream、batch，只会继续放大差异。

### 6.5 测试数量很多，但缺的是 fault/timing/performance 合同

最新 progress 记录显示项目已有数百个前后端测试；本次发现仍能穿过现有测试，原因不是“完全没测”，而是测试重心主要在单次成功、类型合同和安全规则。

明显缺口：

- `useTaskEvents.test.tsx` 只测本地 listener bridge，没有测项目切换时原生 listener 生命周期。
- TopBar search 测试只覆盖单项目成功/失败。
- Source AI 导航测试没有断言 stale task 仍被全局 upsert。
- Graph layout queue 测试只覆盖同项目串行/合并，没有 project/topology invalidation。
- WikiIndex 测试通过等待 mtime 变化验证失效，没有 same-size/same-mtime 场景。
- 没有 10k task log、1 GiB sparse media、10k history summary、concurrent Git checkpoint 等性能/故障注入合同。

## 7. 性能热点排序

| 排名 | 热点 | 当前复杂度/资源形态 | 最坏用户表现 |
|---:|---|---|---|
| 1 | Task log 全链路 | 后端累计 O(L²) bytes；前端累计 O(L²) 复制并叠加 sort | 长任务磁盘狂写、抽屉关闭也卡 |
| 2 | 大媒体 commit | O(总 artifact bytes) 同时驻留；单文件允许 1 GiB | OOM / 进程被杀 |
| 3 | Import batch commit | 全局串行；重复 load/find/history，近似 O(N²)+高 I/O | A 项目阻塞 B 项目 |
| 4 | Async worker 同步阻塞 | 单任务可占 worker 数分钟 | IPC、取消、网络 future 延迟 |
| 5 | Import history | 每页 O(H) 读取、解析和排序 | 历史越多，“加载更多”越慢 |
| 6 | Stream delta | 字符串累计复制 + workspace 全树 render | 长 Chat/Agent 回答逐渐卡顿 |
| 7 | WikiTree filter/render | 深链目录接近 O(N²) | 大目录输入过滤卡顿 |
| 8 | Project status | 冷启动最多约 11 个重复探测；无失效 | 启动慢且状态陈旧 |

## 8. 推荐修复顺序

### 第一批：数据和状态机安全

1. Compile apply/rollback 改为事务或写后 hash CAS。
2. Compile confirmation 改 claim/release/finalizer，消灭悬挂 Running。
3. 统一 TaskService 的 durable-before-publish 协议。
4. 增加 per-project Git coordinator。

### 第二批：异步身份

1. 引入统一 OperationScope。
2. 先修 Source、Wiki openPage、task recovery、Graph build、TopBar、Export preview。
3. 所有 task launch 统一为“无条件 upsert fact，条件提交 presentation”。
4. 全局 task/event listener 只注册一次。
5. 复用 Exports 的 restricted-content preflight，补齐 Wiki Generate/Regenerate 的确认和错误恢复。

### 第三批：最值钱的性能修复

1. task snapshot 与 append-only log 分离，前后端加 retention/paging。
2. artifact 改 file-backed streaming transaction，建立 commit memory budget。
3. Import lock 按项目/session/source 分片，batch 一次加载和索引。
4. blocking process/Git/FS/CPU 移到 bounded blocking pool。

### 第四批：规模化和架构治理

1. Import history summary/index。
2. unique tmp + same-path writer coordination。
3. stream batching、project-status single-flight、WikiTree post-order model。
4. 沿现有 facade 逐 use case 下沉胖 command 和 god function，避免大爆炸重写。

## 9. 必补验证矩阵

| 场景 | 断言 |
|---|---|
| Compile 写一半后外部编辑，再触发失败 | 外部编辑不被 rollback 覆盖；残留路径明确 |
| Compile 确认后的每一个 fallible 点注入错误 | task 永不遗留 Running；action 可恢复或明确终止 |
| Task 持久化注入 ENOSPC/rename/fsync 失败 | 不发布虚假 terminal event；重启状态一致 |
| A→B deferred Wiki/Source/Task/Graph/Search/Export/Bootstrap | 后端事实保留，B presentation 不被 A 覆盖 |
| 10,000 行 task log | append amortized O(1)；task metadata 文件有界；drawer 关闭无 log render |
| 1 GiB sparse media commit | RSS 增量有明确上限，不按文件大小线性增长 |
| 10,000 条 Import history，取 50 条 | 读取/解析条目接近 `limit + 1`，不是全量 |
| 同项目两个 Git checkpoint 并发 | 串行、无 index.lock、无跨任务暂存 |
| same-size + preserved-mtime 外部 Markdown 编辑 | Search/Graph/Chat 能看到新内容 |
| save_layout 与 rebuild 并发 | unique tmp；新 cache/layout 不被旧 response 覆盖 |

## 10. 已确认的正向改进

以下旧问题当前已有实质改善，不应重复列为缺陷：

- `WorkspaceRouter` 已对主要 feature view 使用 `React.lazy` / `Suspense`。
- WikiIndex 已消除 Search/Chat/Graph 每次都重读全部 Markdown 的主要 I/O，当前问题是极端失效 token，不是“完全无索引”。
- Graph renderer 已预计算 render snapshot，旧版 reducer 内 O(E×N) 重扫问题已明显修正。
- Chat send 已采用“task fact 先 upsert，presentation 再 scope guard”的正确模式，可作为其他 feature 参照。
- Import task coordinator 的 task fact 与 presentation 分离总体比 Source/Lint/Export 完整。
- Source/Import 的路径、hash、TOCTOU、原始证据和 confirmation 安全检查覆盖较强。
- Restricted content 的后端强制门是正确的；本报告指出的是 Wiki 前端缺少可恢复交互，不是后端绕过。

## 11. 最终判断

当前项目的主要风险不是“业务逻辑完全没有边界”，而是边界在快速扩展后出现了四种不一致：

1. 同一状态机在不同入口有不同持久化和错误收尾顺序。
2. project scope guard 已普及，但 entity/operation scope 缺失。
3. 本应后台化的任务仍把同步长工作放在共享 runtime worker 或全局锁内。
4. 测试很广，但缺少故障注入、乱序完成和规模预算。

如果先修 P1-01～P1-13，再继续扩 Import/Source/Agent 功能，整体可靠性和后续迭代成本会有明显改善。若反过来继续在现有 `orchestrator/commit/command/store` 上叠分支，“屎山”会主要以更多状态遗漏和性能尾延迟的形式继续增长。
