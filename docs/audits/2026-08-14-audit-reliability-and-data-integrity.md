# LLM Wiki Desktop 可靠性、恢复与数据完整性审查

日期：2026-08-14

来源：从《第一性原理对抗性审查》拆分

范围：跨项目异步隔离、lazy 恢复、缓存新鲜度、并发写入、媒体身份、下载续传、任务持久化、断电语义、进程清理

## 1. 结论

项目在 Import transaction、Workflow/task、Git checkpoint 和候选工作区方面已经有较好的恢复意识。当前可靠性缺口集中在三个层面：

1. **异步结果身份**：搜索结果、cache snapshot、layout save 等结果没有始终绑定产生它的 project/revision/generation；
2. **失败后的真实恢复**：lazy retry、能力包断点续传等界面或流程看似有入口，但无法从真实故障继续；
3. **持久化语义不统一**：部分路径有 journal/hash/checkpoint，另一些仍是无 CAS read-modify-write、全量 JSON 重写或只保证 rename 可见性。

## 2. P1 发布阻断项

### REL-P1-01 顶栏搜索结果可能跨项目串线

对应总报告：P1-11。

搜索使用 request sequence，但项目变化不会递增 epoch 或清空结果。项目 A 的慢响应可在切到 B 后提交，点击时又使用当前项目 B 打开：

- `src/components/app/TopBar.tsx:49-62,267-305`

**影响**：显示旧项目标题/路径；在新项目打开错误页，或打开同名但无关页面；破坏项目隔离的用户心智。

**修复**：请求固定 `{projectId, canonicalRoot, requestId}`；响应、展示和点击同时校验来源 scope；项目切换递增 epoch、清空结果并关闭 popup。

**验收**：A 搜索 → 切 B → A resolve；A 结果不出现，`openPage` 不调用；覆盖 A/B 同名 path 和 Windows 大小写。

### REL-P1-02 lazy chunk 的“重试”无法真正重新加载

对应总报告：P1-12。

`React.lazy` identity 在模块作用域创建；error boundary 只清 error 并改变 Fragment key。被拒绝的 lazy payload 已被 React 缓存，重挂同一对象仍会立即失败：

- `src/components/app/WorkspaceRouter.tsx:12-46`
- `src/components/app/ViewErrorBoundary.tsx:32-44`
- `src/components/app/WorkspaceRouter.lazy-error.test.tsx:98-114`

**影响**：一次瞬态资源读取失败后，按钮承诺“重试”但用户只能手工重启应用。

**修复**：稳妥方案是受控 reload；局部重试必须创建新的 lazy identity/loader generation。

**验收**：第一次 import reject、第二次 resolve；点击后 importer 调用两次且视图出现；packaged Tauri 模拟缺 chunk/瞬态 I/O。

### REL-P1-03 媒体下载 cache 没有绑定原始媒体身份

对应总报告：P1-08。

manifest 只记录 payload/hash/size，没有 source/final URL、media id 或 selector fingerprint；旧 payload 完整时会被包装成当前 URL 的 artifact：

- `src-tauri/src/services/import_v2/generic_web_engine.rs:162-170,744-750`
- `src-tauri/src/services/import_v2/generic_web_engine.rs:1705-1807`

**影响**：signed/CDN URL 或用户选择改变后，重试可能保存和转写旧媒体；provenance 显示的是新地址，真实字节却来自旧内容。

**修复**：manifest v2 保存规范化 request target、稳定 platform media id/selector、必要 final target；cache key 绑定 item + media identity + route options；不匹配时 miss。

**验收**：A cache + B load 必须 miss；等价规范化 A 可 hit；signed URL、selector、redirect final URL 分别测试。

### REL-P1-04 撤销信任与 Chat 外发/写回存在竞态

**Batch 6 状态（2026-08-21）：代码与 deterministic barrier 合同 Closed；真实 packaged 外发/写回证据 Pending。** Commits `8abc93a2`、`11b187d3` 统一 project execution epoch、cancel/drain barrier 与写回重验；Batch 6 聚焦矩阵通过。真实 provider/Agent 进程和网络计数仍需签名包验收，见 [`../release/batch-6-acceptance-evidence.md`](../release/batch-6-acceptance-evidence.md)。

对应总报告：P1-03；安全主报告详述。

从可靠性角度，问题是“操作完成”的含义不可信：revoke 返回成功不能保证所有该项目的外部执行和写回已经停止。

**修复**：统一 project execution epoch、cancel + barrier、写回 compare-and-commit。

**验收**：revoke 返回后，外部调用、子进程和 Chat session 不再变化。

## 3. P2 可靠性与一致性项

### REL-P2-01 No-project 错误会退化成 `[object Object]`

**Batch 6 状态（2026-08-21）：Closed。** Commit `214605d5` 增加共享 structured BackendError normalization、本地化 recovery action、脱敏 technical detail，并迁移 no-project/import/provider/update/task 等优先错误面；Batch 6 前端聚焦组 242/242 与 final-four redline 均通过。

对应总报告：P2-05。

Rust 返回结构化 `BackendError`，projectStore 原样抛出；`NoProjectWorkspace` 对非 `Error` 使用 `String(error)`：

- `src-tauri/src/errors/backend_error.rs:10-18`
- `src/stores/projectStore.ts:326-334,428-432,452-458`
- `src/features/project/NoProjectWorkspace.tsx:11-13,61-109,189,241`

**影响**：首次创建、打开、评估或修复失败时，用户得到不可行动的字符串，最需要恢复指导的路径反而丢失错误语义。

**修复**：共享 `formatBackendError`，结构化 code/message/details；本地化 summary + recovery action + 可展开技术细节。

**验收**：zh/en 注入真实 serialized reject；open/create/import/chat/settings/task 均不出现 `[object Object]`。

### REL-P2-02 能力包下载不能 crash-resume，并可能留下 orphan

**Batch 6 状态（2026-08-21）：源码/集成恢复合同 Closed；四 target 强杀与原 Import 继续 Not Closed。** Commit `b55b7007` 落地 release-scoped partial identity、Range/安全重下、启动 reaper、事务安装/health rollback 与 exact item continuation；capability 工具 Node 66/66、Python 9/9 通过。25%/75% 强杀的真实签名包证据仍是 public-beta blocker。

对应总报告：P2-08。

每次生成随机 `.download-{nonce}.zip`，完整 GET + truncate；只有正常 Drop 清理：

- `src-tauri/src/services/import_v2/capability_installer.rs:84-91,134-196,452-460`

**影响**：大包在 25%/75% 时强杀会从头下载；重复中断占用磁盘；用户无法区分暂停、失败和孤儿文件。

**修复**：确定性 partial metadata；支持时 Range resume；启动 reaper；主动取消才清理；最终完整 hash/signature 校验。

**验收**：强杀后显示 paused 并继续，无 orphan 增长，最终安装字节正确。

### REL-P2-03 WikiIndex 可能静默返回旧正文

对应总报告：P2-09。

Index 只用 `(mtime,size)` 判定未变；snapshot 只按 project id；并发 refresh 没有 generation/single-flight，旧 build 可能最后发布：

- `src-tauri/src/services/wiki_index.rs:54-69,105-191,299-308`

**影响**：Search、Chat retrieval、Graph 看到旧内容；外部编辑已成功但应用继续回答旧正文；项目 relocation 可能误复用旧 snapshot。

**修复**：app write 主动 dirty；watcher generation + file identity/ctime；coarse FS 保守 rehash；key 包含 canonical identity/root/revision；发布时 CAS generation；close/relocate evict。

**验收**：同尺寸保留 mtime 改写；并发 old-read/new-publish/old-publish；同 project id relocation。

### REL-P2-04 Graph read 强制写 cache，layout save 可覆盖新 topology

对应总报告：P2-11。

cache miss 后 `resolve/rebuild` 强制持久化；layout save 是无 revision/CAS 的 read-modify-write：`src-tauri/src/services/graph_service.rs:141-212`。

**影响**：read-only/restricted 项目可能因 cache 写失败而不能查看 Graph；save layout 与 rebuild 竞态时可能把旧 topology 写回。

**修复**：`CachePolicy::{MemoryOnly,Persistent(permit)}`；read-only 返回内存结果；layout 使用 topology hash + revision CAS，或进入单一 per-project lane。

**验收**：真正只读项目首次 Graph 成功且无新增文件；rebuild/layout barrier race 不恢复旧 topology。

### REL-P2-05 Task log/activity 写放大与无界恢复

对应总报告：P2-13；性能交叉项。

每条 log/activity 都重写完整 task JSON；启动恢复读取解析全部 task files，无 count/size budget：`src-tauri/src/tasks/task_service.rs:2450-2527,3293-3367`。

**影响**：长任务持续放大写入；异常巨大/损坏 task 文件会拖慢甚至阻断项目恢复。

**修复**：bounded append-only chunks + metadata snapshot；内存 tail；终态压缩；恢复按大小/数量/schema 分页、quarantine。

**验收**：10k logs 接近 O(N)；1 万 task 或超大 task 在 budget 内返回 partial/quarantine。

### REL-P2-06 FileStore rename 后没有 parent directory durability flush

对应总报告：P2-14。

temp file `write_all + sync_all` 后 rename，但没有 sync parent：`src-tauri/src/services/file_store.rs:364-421`。

**影响**：进程崩溃时通常能保持 old/new 完整可见，但 POSIX 突然断电后 directory entry 仍可能丢失；“atomic”被误解成“已经 durable”。

**修复**：按平台 flush parent；关键状态保留 journal/上一版本；API 区分 atomic visibility 与 durable commit。

**验收**：temp sync 后、rename 后、parent sync 前 fault injection；恢复必须得到 old 或 new 的完整状态。

### REL-P2-07 Graph/Wiki/Chat 路由重入没有统一 freshness 契约

对应总报告：P2-01。

视图 mount 时重新拉取，但数据层没有统一 revision/single-flight/stale-while-revalidate；这既是性能问题，也是错误状态恢复不一致的问题。

**影响**：普通返回板块也可能闪 loading；外部 Markdown 编辑和缓存保活之间缺少可证明的新鲜度语义。

**修复**：per-project revision、event invalidation、stale-while-revalidate；保留旧内容后台刷新，外部 change event 必须使相关 projection dirty。

### REL-P2-08 普通 Agent probe timeout 没有完整回收进程树

对应总报告：P3-01。

`run_spawn_target_with_timeout` 只 `child.kill()`，没有 wait/reap 或 kill descendants：`src-tauri/src/services/agent_service.rs:3525-3568`。正式 runner 已有正确实现：`src-tauri/src/services/agent_service.rs:2777-2797`。

**影响**：异常 CLI 可能留下子孙进程或 zombie，重复探测后积累资源。

**修复**：probe 复用统一 `ProcessLifetimeGuard`/process group/job object；timeout/cancel 后 kill tree + wait。

## 4. 可靠性不变量

建议把以下不变量写入 service 契约与测试：

1. **Scope**：每个异步结果必须携带产生它的 project/root/revision/request epoch；
2. **Freshness**：cache 必须说明由何事件失效，不能只有“完成态永久缓存”；
3. **Atomicity**：用户可见状态只能是 old 或 new，不允许 partial；
4. **Durability**：需要抗断电的状态必须明确 parent flush/journal，而不是只写 atomic；
5. **Recoverability**：按钮写“重试/继续”时必须真实再次执行或从 checkpoint 继续；
6. **Cancellation**：停止不仅改变 UI 状态，还必须停止请求、worker 和子进程；
7. **Identity**：缓存/下载/artifact 必须绑定内容身份，hash 完整不等于来源正确；
8. **Error semantics**：后端 typed error 在前端不得退化成字符串或英文技术消息。

## 5. 恢复测试矩阵

| 场景 | 必须证明 |
| --- | --- |
| A 项目慢响应后切 B | A 结果不进入 B 的视图、drawer、toast、导航 |
| lazy 第一次失败 | 用户操作后真正再次 import 或可靠 reload |
| capability 下载 25%/75% 强杀 | 重启可继续；无 orphan；最终签名/hash 正确 |
| 外部同尺寸保留 mtime 编辑 | Search/Chat/Graph 看到新正文 |
| Graph rebuild 与 layout save 并发 | 新 topology 不被旧 cache 覆盖 |
| 10k logs / 10k task files | 写入、恢复、内存受 budget 约束 |
| FileStore 各持久化阶段中止 | 只得到完整 old/new；无半文件 |
| Agent probe fork child 后超时 | 父子 PID 均消失，无 zombie |
| trust revoke 与 Chat request/commit 竞态 | revoke 返回后无外发、无新写回 |

## 6. 推荐顺序

1. 跨项目搜索与 lazy retry；
2. media cache identity 与 WikiIndex generation；
3. Chat/revoke compare-and-commit；
4. Graph cache policy + CAS；
5. capability resume/reaper；
6. task append/recovery budget；
7. FileStore durable commit 与统一 process lifetime；
8. 建立 fault injection、强杀和并发 barrier 测试层。
