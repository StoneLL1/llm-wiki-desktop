# Import V2 实现 Review（2026-07-15）

> 历史实现审查证据，不构成 2026-07-24 后的产品验收门禁。当前 Import / Source 行为以 [`../superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../superpowers/specs/2026-07-24-import-source-media-flow-design.md) 为准。

## 结论

当前实现不建议进入发布或切换窗口。基础工程质量不错，后端 Import V2 的协议、暂存、产物校验、事务恢复和测试覆盖已经形成骨架；但仍有一个 P0 安全阻断，以及多条会让用户无法完成导入、无法恢复或看不见真实结果的 P1 闭环问题。

本次评审只读检查了实际实现工作树 `D:/Users/Aletta/Desktop/Works/llm-wiki-desktop/.worktrees/import-v2-final-integration`，没有修改该工作树中的应用代码。设计基准为 `docs/superpowers/specs/2026-07-11-import-v2-design.md`，并结合项目 `SPEC/SPEC.md` 第 16 节、`SPEC/APP_flow.md`、`SPEC/BACKEND_STRUCTURE.md` 和 `SPEC/FRONTEND_GUIDELINES.md` 进行判断。

## 评审方法：第一性原理

我把“导入完成”拆成几个不可省略的事实：

1. 不可信输入不能越过项目边界，能力包也只能读当前任务授权输入、写当前任务 staging。
2. 用户授权、确认、取消、重试等动作必须真正改变后端状态，并且前端展示必须与后端可执行状态一致。
3. 预览必须是可追溯、可验证、可恢复的中间结果；正式写入必须保留原始来源并能解释每个产物从哪里来。
4. 批量任务必须有边界：并发、磁盘、网络、进程、取消和重启都不能依赖“希望它及时结束”。
5. 每个重要承诺都要有可重复的验证场景，而不是只验证 happy path。

## 已经做得比较好的部分

- `EngineResult` 的路径、大小、哈希和 staging 校验较完整，能阻止许多路径逃逸、产物替换和不完整输出。
- Import session、item、attempt、quality report 和 source registry 使用 typed DTO/JSON 持久化，符合本地文件事实来源和版本化协议要求。
- 文件发现、OOXML 解包、HTML/Markdown 清洗、SSRF、重定向解析、临时媒体清理、Agent candidate provenance 等边界已有大量测试。
- 事务 journal、hash-checked write、rollback、重启恢复和任务取消 token 已有较扎实的基础实现。
- 前端 workflow 已经引入 project key / session epoch，且 React 没有直接持有文件系统、Git、进程或密钥逻辑。
- 错误码、阶段、retryable/action 等信息已有结构化模型；多数错误日志经过安全化处理。

这些优点说明当前主要问题不是“没有架构”，而是若干关键边界还没有贯穿到所有实现路径。

## 发布阻断项

### P0-01：能力包没有得到真正的文件系统沙箱

第一性原理：签名只能证明“这个包来自某个可信发布源”，不能把普通用户权限进程自动变成“只能读任务输入、只能写 staging”的进程。设计文档第 7.2 明确禁止能力包直接写 `raw/`、`wiki/`、Git 或秘密存储。

证据：

- `src-tauri/src/services/import_v2/engine.rs:31-32` 的 `EngineRequest` 暴露完整 `project_root`。
- `src-tauri/src/services/import_v2/orchestrator.rs:973-974` 将项目根目录传入请求。
- `src-tauri/src/services/import_v2/pack_engine.rs:122-172` 直接以当前用户权限启动能力包进程，只设置了工作目录、环境变量、进程组/Job 和进程树清理；没有 Windows restricted token、ACL sandbox、macOS sandbox profile 或 Linux namespace/seccomp 等访问控制。
- 即使 `EngineResult` 产物路径经过校验，能力包仍可能在返回 JSON-RPC 之前直接读写未声明的项目文件；产物校验无法撤销这种副作用。

影响：恶意包、被篡改的包或包内部 bug 都可能修改 `raw/`、`wiki/`、`.app/` 或 Git。这直接违反“能力包只能读授权输入、写 staging”的硬约束，属于发布前必须解决的安全问题。

建议：

- 不再把完整项目根目录作为能力包可用路径；协议只传最小化的任务输入目录和 staging 目录。
- 对每种平台实现 OS 级最小权限：Windows restricted token/ACL/job object，macOS sandbox profile，Linux namespace/seccomp 或等价隔离。
- 加入攻击性回归测试：能力包尝试读写 `wiki/`、`raw/`、`.app/`、Git 和项目外路径时必须失败，且失败后不能留下副作用。
- 在 release gate 中把“能力包沙箱验证”设为必需证据，不能由签名校验替代。

## P1：功能、安全和数据一致性问题

### P1-01：私网 URL 授权无法被 Generic/WeChat 引擎使用

第一性原理：用户点击“允许访问私网目标”后，下一次且仅下一次抓取必须携带同一作用域的 grant；否则 UI 的授权动作没有实际意义。

证据：

- `src-tauri/src/commands/import_v2_web_commands.rs:182-222` 创建并保存 private grant。
- `src-tauri/src/services/import_v2/web_target_store.rs:102-112` 提供 `authorize_private` 和 `take_private`。
- `src-tauri/src/services/import_v2/pack_engine.rs:285-305` 会消费 grant，但 `src-tauri/src/services/import_v2/generic_web_engine.rs:87-94` 与 `src-tauri/src/services/import_v2/wechat_web_engine.rs:85-92` 始终向 `WebFetchService::fetch` 传 `None`。
- `WebFetchService` 在 `web_fetch.rs:90-99` 仍会按普通 SSRF 规则拒绝私网地址。

结果是：用户授权后重试 Generic/WeChat，仍可能被同一个私网策略拒绝；而且 `web_target_store.rs:102-111` 使用单纯 `item_id` 作为 key，grant 没有显式绑定 `project_id + session_id + item_id`。

建议：把 scoped grant 放进 `EngineRequest` 的后端内部上下文，统一下沉到 WebFetch facade；所有 web engine 使用同一套取用/一次性消费逻辑。存储 key 改为 `(project_id, session_id, item_id)`，并增加“授权 → Generic 成功”“授权 → WeChat 成功”“跨项目/跨 session 失败”“重定向到另一私网目标要求重新授权”测试。

### P1-02：三方合并流程在前端无法完成

第一性原理：冲突解决必须形成完整状态机：查看基线 → 编辑/选择结果 → 提交当前 Wiki hash → 后端校验 → 进入可提交状态。

证据：

- `src/features/import/ImportView.tsx:78-85` 始终发送 `expectedCurrentWikiSha256: null`，所谓 merged 内容也只是候选 Agent Markdown，并没有真正的合并编辑器。
- `src/features/import/ImportCandidateDiffDialog.tsx:58` 展示 diff，但没有可编辑的 merged Markdown 输入面。
- `src-tauri/src/services/import_v2/agent_candidate.rs:402-413` 对三方合并明确要求 merged Markdown 和 expected Wiki hash。
- `src-tauri/src/services/import_v2/orchestrator.rs:665-674` 选择需要三方合并的 candidate 后仍保持 `NeedsMerge`。
- `ImportView.tsx:53-56` 的 commit decisions 只筛选 `preview_ready`，不会把 `needs_merge` 结果纳入提交。

因此用户可看到“比较/解决合并”按钮，却无法提交一个后端接受的合并结果；即使补齐 hash，也仍需要把最终 item 状态和 commit decision 接通。

建议：后端返回当前 Wiki 内容及 hash；对话框提供明确的 deterministic/Agent/current/merged 选择与编辑能力；只在后端 hash 校验通过后转换到 `PreviewReady`；为每个 item 生成独立 decision，不能用全局 conflict policy 覆盖三方合并语义。

### P1-03：批量部分失败没有以 typed 结果进入任务/UI

第一性原理：批量导入的真实结果不是一个成功/失败布尔值，而是每个 item 的 `completed/failed/skipped/cancelled` 及可重试原因；用户必须能据此继续工作。

证据：

- `src-tauri/src/services/import_v2/commit.rs:249-274` 只把单项失败写进内存 batch/history。
- `commit.rs:579-581` 只将成功 item 标为 Completed；失败 item 的持久化 issue/status 没有同步完成。
- `src-tauri/src/commands/import_v2_commands.rs:335-349` 丢弃 `batch.items`，TaskResult 只有 summary，且 `reference: None`。
- `src/features/import/useImportWorkflow.ts:224-230` 主要只处理整个 task failed；前端没有逐项结果入口。

影响：用户可能看到“已提交 3 个、失败 2 个”，但看不到具体失败 item、error code、受影响路径和 Retry 入口，违背设计中的 batch partial success 与可观测性要求。

建议：持久化每个失败 item 的稳定 issue；TaskResult 返回 typed `ImportBatchResult` reference；前端按 item 展示结果、日志、影响路径和 retry，并用集成测试覆盖“第 2 项失败、第 1 项已提交、第 3 项仍可重试”。

### P1-04：V2 导入历史不会进入 V2 history UI

`src-tauri/src/commands/import_v2_presentation_commands.rs:130-145` 始终返回 `entries: Vec::new()`，只把旧历史放在 `legacy_read_only`。这与前端 `ImportHistoryPanel` 的“历史、重新打开结果、查看日志”预期不相符：新的 V2 batch 写了 `.app/import-history/{batch_id}.json`，但 UI 无法读取它。

建议：解析 V2 history，返回独立的 `ImportHistoryEntry`，保留 legacy 只读投影；历史 entry 至少应带 session/batch id、状态、时间、可用动作和结果 reference，并添加一次重启后从历史打开结果的测试。

### P1-05：批量 start 全量 spawn，没有资源调度或 resource mode 约束

第一性原理：输入数量可以远大于机器可承受的并发数；“创建了任务”不等于“应该立即运行全部任务”。Saver/Balanced/Performance 必须改变实际资源使用。

证据：

- `src-tauri/src/commands/import_v2_commands.rs:188-231` 为所有 item 创建 task 后立即 `tauri::async_runtime::spawn`，没有全局并发上限、内存预算、磁盘预算或等待队列。
- `ImportResourceMode` 在 session 中被保存，但 start path 没有据此调度；当前可见的 `DomainLimiter` 只覆盖 pack web route，不能限制文件解析、Office/PDF/Agent 等重任务。

影响：大文件夹可同时启动大量解析、浏览器/外部进程或内存占用，造成 UI 卡顿、磁盘耗尽、取消不及时和跨任务互相争抢。设计第 13 节要求 scheduler、重任务低并发和可取消。

建议：引入持久化 scheduler：按 resource mode 设置 global/engine/domain semaphore，队列中只为可运行 item 分配 task；记录 queued/running/backpressure；测试 1,000 个 item、重任务混合、Saver 模式和取消队列头/中/尾。

### P1-06：WaitingCapability/WaitingLogin 的取消动作与后端状态不一致

前端 `src/features/import/importStatusPresentation.ts:48-50` 为 waiting capability/login 展示 Cancel；但后端 `src-tauri/src/services/import_v2/orchestrator.rs:219-239` 的 `cancel_queued_item` 只接受 Queued。任务被取消后，item 可能仍停留在 waiting 状态；重启恢复逻辑也主要处理 Inspecting/Extracting/Validating。

建议：定义“取消等待中的 item”这一明确状态转换，并让 task/item/session 一起原子落盘；验证等待登录、等待能力时点击取消、重启、再 Retry 的完整链路。若产品不允许取消，则 UI 不应展示该动作。

### P1-07：能力缺失没有可完成的恢复闭环

独立审查发现 `src-tauri/src/commands/import_v2_presentation_commands.rs:222` 的能力安装接口仍固定返回 unavailable，前端 `useImportWorkflow.ts:681` 也没有在能力安装成功后重新排队/Retry。这样 `WaitingCapability` 只能被看见，不能被用户完成。

建议：在能力包尚未随应用交付的阶段，明确显示“缺少能力/如何获得/当前不可继续”，不要伪装成可安装；如果支持安装，则安装必须走用户确认、签名/哈希/许可证检查、健康检查，成功后明确让 item Retry，并覆盖安装失败、重启和回滚测试。

### P1-08：内置 Office fallback 不能满足自身的 ModernOffice 质量前置条件

`src-tauri/src/services/import_v2/orchestrator.rs:1748-1781` 对 XLSX 要求单 sheet、非空单元格覆盖率和 formula/value pair，对 PPTX 要求精确页数和 meaningful image coverage；但 `src-tauri/src/services/import_v2/native_file_engine.rs:228-241` 对内置解析器返回多个关键指标 `None`，并发出 `OFFICE_STRUCTURED_CONTENT_NOT_EXTRACTED`。

结果是没有合格能力包时，普通 XLSX/PPTX 会被自己的 precheck 拒绝，用户既不能得到合格预览，也没有清晰的“等待能力/改用低质量预览”选择。

建议：要么让 native engine 真实计算指标，要么给 fallback 使用显式的低质量等级，进入 `Warning/WaitingCapability`，不能把不具备指标的路径放进要求这些指标的 floor；增加真实多 sheet/公式/PPT 图片 fixture。

### P1-09：内置 Web 引擎绕过 DomainLimiter

`src-tauri/src/services/import_v2/domain_limiter.rs:5-10` 的同域并发限制只在 `pack_engine.rs:282-305` 使用； Generic/WeChat 直接调用 `WebFetchService`。批量 WeChat 或同域 URL 因此绕过敏感域单并发限制。

建议：把 limiter 注入统一 WebFetch facade，或让所有 engine 先通过同一个 web route executor；测试同域并发数、敏感平台单并发以及取消等待中的 permit。

### P1-10：已有 source manifest/index 的高风险更新没有 checkpoint

`src-tauri/src/services/import_v2/commit.rs:465-471` 只在 `overwrite_wiki` 时创建 checkpoint；但 `commit.rs:554-572` 仍可能更新已有 manifest 和 `.app/source-index-v2.json`。设计要求来源更新、批量重写等高风险操作在写入前展示影响路径并创建检查点。

建议：统一计算本次 commit 的 affected paths，在第一处写入前创建 scoped checkpoint；如果产品决定“新 source 不需要 checkpoint”，也要把这个边界写入设计并在 UI 中显示。测试同一 source 新版本、manifest/index 更新、checkpoint 失败时零写入。

### P1-11：取消与最终预览写入之间存在竞态

`orchestrator.rs:994` 在 engine 返回时检查取消，但 `orchestrator.rs:1191-1220` 在质量评估后直接写入 PreviewReady、任务结果和 WaitingForConfirmation。若取消发生在这两个阶段之间，可能出现“task 已取消但 item 有可提交预览”的不一致。

建议：在最终 mutate 前再次检查 cancellation，并将 item 状态、task terminal status 和 preview publication 绑定成可验证的原子边界；用 barrier 注入测试覆盖“引擎返回后取消”和“质量评估后取消”。

### P1-12：BYOK 脱敏规则不足以支撑“不发送秘密”的承诺

`src-tauri/src/services/import_v2/agent_assistance.rs:1211-1244` 只覆盖有限 marker（如 authorization/cookie/api_key/access_token/password/secret），且字段脱敏依赖行中存在 `:` 或 `=`。`Bearer ...`、`private_key`、裸 token、代码块中的其他密钥形态可能漏过。当前 `ImportByokApprovalDialog` 也只展示文件名/大小/标签，用户无法审阅实际发送文本。

建议：把“允许发送的文本”做成可审阅的、默认最小化的 preview；采用结构化 secret detector + deny-by-default corpus，任何不确定内容进入手动确认或不发送。用 `refresh_token`、`private_key`、Bearer token、不同 provider key、frontmatter、代码块和 CJK 文本做回归测试。

## P2：代码质量、体验和可维护性问题

### P2-01：目录扫描为每个文件整文件读取并计算 hash

`src-tauri/src/services/import_v2/file_discovery.rs:219-265` 在首次产出 batch 前读取 prefix、识别格式，再由 `source_identity` 在 `:336-350` 使用 `fs::read` 读取整个文件并计算 SHA-256。小 fixture 可以通过首批延迟测试，但对大量接近上限的文件会把扫描体验变成顺序全量 I/O。

建议：使用流式 hash；先发出可展示的 discovered item，再异步补齐 identity；对首批可见时间、取消响应和磁盘吞吐建立真实大文件 benchmark。

### P2-02：Markdown 引用的本地图片缺少总量配额和取消检查

主 Markdown 有单文件限制，但 `native_file_engine.rs:368-434` 逐张复制本地图片，没有明确总字节数、文件数和每张图片的取消检查。一个很小的 Markdown 可以间接拉入大量图片，导致磁盘压力和取消延迟。

建议：为 assets 设置 item 级数量/总字节配额，复制前后检查 cancellation，写入 transaction staging，并在 UI 显示“正文/附件”进度。

### P2-03：WeChat 图片质量指标与实际产物不一致

`wechat_web_engine.rs:122` 收集图片 URL，但 `:153` 返回空 `asset_paths`，同时 `:162` 将 meaningful image coverage 置为 1.0。离线查看或 HTML export 可能丢图，quality report 也会给出错误信号。

建议：要么下载并校验到 staging/assets，要么明确把图片标为 remote reference、coverage 不计为本地覆盖，并在预览中提示离线不可用。增加断网后查看 raw/wiki/export 的测试。

### P2-04：`needs_merge` 的可选/可提交展示与 commit bar 筛选不一致

`src/features/import/importStatusPresentation.ts:52-54` 将 `needs_merge` 标记为 selectable/committable，但 `ImportView.tsx:53-56` 的 selected count 和 decisions 只筛选 `preview_ready`。用户可以勾选或看到合并动作，却不会被确认栏计入。

建议：在状态机完成前把 `needs_merge` 标成不可提交；完成后提供 per-item merge decision 并让 count、按钮、后端校验使用同一 selector。增加“选择/取消选择/确认”三态 UI 测试。

### P2-05：项目切换时单一 refresh Promise 可能吞掉新项目刷新

`src/features/import/useImportWorkflow.ts:151-184` 使用一个全局 `refreshInFlight`。A 项目请求未完成时切到 B，B 的刷新可能复用 A 的 Promise；虽然 then 有 scope guard，但 B 不一定会发起自己的请求。

建议：按 `projectKey/sessionId/epoch` 做 dedupe key，或切换时让旧 promise 失效并强制发起新请求；添加 A→B→A、快速切换和旧请求晚到的测试。

### P2-06：`startNewQueuedItems` 的异常没有统一捕获

`useImportWorkflow.ts:188-207` 直接 await `startItems`，而 `handleTaskEvent` 在 `:216-218` 通过 `void refresh.then(() => startNewQueuedItems(...))` 调用。启动失败会变成未处理 rejection，没有项目级 toast、日志或 Retry 提示。

建议：所有后台链路都使用同一个带 scope guard 的 `runAction` wrapper，记录 task/reference，失败时保留队列状态并给出明确下一步。

### P2-07：空队列的快捷动作没有接线，平台 readiness 也被硬编码为不可用

- `ImportQueue` 支持 `onChooseFiles/onChooseFolder/onFocusUrl`，但 `ImportView.tsx:169-180` 没有传入，空状态只剩说明文字。
- `ImportSourceMethods.tsx:37-48` 将 WeChat、Zhihu、Bilibili、Xiaohongshu、X 固定为 unavailable，尽管后端已有部分 route/connector。用户无法区分“当前平台不支持”“能力缺失”“需要登录”。

建议：空状态提供直接行动；平台状态由 typed readiness DTO 驱动，至少区分 supported/needs-capability/needs-login/unavailable，不要用静态 chip 代替真实能力发现。

### P2-08：存在未完成的兼容 API 和不可达的 paused 状态

`useImportWorkflow.ts:481-489` 的 clipboard/delete/replace callback 只是 toast stub；`ImportItemStatus::Paused` 在模型中有展示和 retry 动作，但没有导入 pause command 或实际生产该状态。它们会让调用方误以为能力存在，增加维护分支。

建议：尚未进入产品范围的 API 直接移除或标为明确 unsupported；如果设计承诺可暂停/恢复，补齐 durable checkpoint、pause/resume command 和重启测试。

### P2-09：前端队列缺少选中状态的语义化暴露

`ImportQueue.tsx:108-121` 使用 `article role="listitem"` 接受键盘选中，但没有 `aria-selected` 或关联的 `aria-controls`；整个队列在 `:72` 使用 `aria-live="polite"`，大量进度更新可能造成屏幕阅读器噪音。

建议：采用 listbox/option 或 grid 语义，暴露 selected/active descendant；只让状态摘要 live，列表本身不要整体 live。补充键盘 Enter/Space、焦点、筛选和取消动作测试。

### P2-10：新 session/terminal session 的追加语义没有明确保护

`src-tauri/src/services/import_v2/session_store.rs:210-229` 的 `add_inputs` 加载 session 后直接追加 queued item，没有拒绝 `Completed`/`Cancelled` 终态，也没有创建新 session。若 UI 继续持有已完成 session，新增 item 可能挂在 Completed session 下，并在重启时因 `find_unfinished_session` 被忽略。

建议：终态 session 追加时创建新 draft 或返回稳定的 `IMPORT_V2_SESSION_TERMINAL`，统一由 workflow 替换 session；添加“完成后继续导入、关闭应用、重新打开”测试。

## 质量门槛和验证实现评估

### 已验证的结果

- `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features`：通过；仅有 `transaction.rs` 中未使用方法 warning。
- Rust 单元测试主体：598 passed。
- Import V2 相关 integration tests 大部分通过，覆盖了路径、SSRF、重定向、能力包签名、取消、恢复、质量 gate、Agent candidate、WeChat/Bilibili/X/知乎等场景。
- `npm run check:console`：通过，没有发现意外 `console.log`。

### 当前未通过或不能作为 release 证据的结果

- `npm run check`：在 Vitest 启动阶段失败，没有进入完整 test/lint/build 链路。环境报错为 `@tailwindcss/oxide-win32-x64-msvc` 原生 binding 无法加载（`stream did not contain valid UTF-8`），同时出现 Vite `spawn EPERM`。
- `npm run check:import-v2-cutover`：失败。缺少/未通过 file/web/agent package release gate、Windows/macOS/Linux platform acceptance、fixture matrix、long-task recovery、schema review 和外部工具 license/provenance 证据；并提示 legacy mutation code 仍在 soak window 前存在。
- `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features`：Import V2 相关测试均通过，但最后的 `mvp_flow::ai_assisted_loop_fake_agent_detected_and_byok_runs` 因测试写入真实用户目录 `C:\Users\Aletta\AppData\Roaming\llm-wiki-desktop\.settings.json.tmp` 被拒绝而失败。测试应隔离到 temp app-data/credential fixture，不能依赖真实用户目录。

### 建议补齐的最小验证矩阵

| 场景 | 必须证明的事实 |
| --- | --- |
| 能力包攻击 | 不能访问/修改项目根下非 staging 的任何文件；跨平台均有证据 |
| 私网授权 | 授权后 Generic/WeChat 可成功一次；跨 project/session、过期、跨重定向均失败 |
| 三方合并 | 选择 deterministic/Agent/current/merged 均有正确 hash 校验、状态转换和提交结果 |
| 批量部分失败 | 每项结果可见、可重试，已成功项不回滚，失败项不伪装成成功 |
| 等待态取消 | WaitingLogin/WaitingCapability 取消后 item、task、session 一致，重启不复活 |
| 调度 | 1,000 项不会全量并发；Saver/Balanced/Performance 的并发和磁盘上限可观测 |
| 产物质量 | XLSX/PPTX 指标与 fallback 语义一致；WeChat 图片离线可验证或明确标记远程 |
| BYOK | secret corpus 不进入 provider 请求、日志、task payload 或 preview；scope 可审阅 |
| 恢复 | 在 scan、engine return、quality、commit 每个边界中断，重启后不重复写、不丢 item |
| 发布 gate | `npm run check`、package evidence、platform acceptance、license/provenance 全部通过 |

## 建议修复顺序

1. 先关闭 P0：能力包最小权限/OS sandbox，并用攻击性测试证明未声明写入无法发生。
2. 修复私网授权、等待态取消、批量调度和取消竞态，先保证安全和状态机不会给出假成功。
3. 接通三方合并、部分失败结果、V2 history 和能力恢复，使用户能够从失败继续完成工作。
4. 统一 Web fetch/limiter/grant，修正 Office fallback 的质量合同和 WeChat assets。
5. 处理 checkpoint 边界、BYOK corpus、session 终态、扫描与 assets 配额。
6. 补齐 UI 空状态、平台 readiness、队列语义和后台异常处理。
7. 隔离真实用户目录测试，修复本地 Tailwind/Vite 运行环境后，从头重跑 `npm run check` 与 cutover gate。

在上述 P0/P1 和 release gate 关闭前，不建议把“已有大量测试通过”解释为导入模块可以发布；目前测试更能证明许多局部约束存在，不能证明完整用户闭环和跨平台安全边界已经成立。

## 第二轮补充：以用户体验和使用流畅度为中心（2026-07-15）

这轮按你的补充重新调整权重：本地工具的安全风险仍然需要避免明显的数据损坏和错误状态，但不把 OS sandbox、攻击面和权限隔离作为主要评分项，重点看用户能否快速开始、持续知道发生了什么、批量任务是否安静而可控、失败后能否就地继续。

### 本轮使用的第一性原理

一次导入体验至少要连续满足五个事实：

1. 用户发起动作后，界面必须立即承认动作已经收到，而不是让用户猜测有没有点成功。
2. 任务耗时超过一个短交互周期后，界面必须展示真实的中间状态，包括阶段、数量和当前可做的动作。
3. 同一批任务应该被用户理解为一个批次，而不是一堆互相打断的子任务。
4. 每个失败结果都必须告诉用户“失败了什么、还能做什么、下一步怎么做”，不能只留下 toast 或技术错误码。
5. 完成、失败、取消和等待必须在数字、按钮、筛选器、历史记录和详情面板中保持同一语义。

本轮没有新增安全类 P0，但发现若干会直接破坏“顺畅完成一次导入”的 P1 问题。

### 本轮 P1：会让用户觉得卡住、重复执行或无法继续

#### UX-P1-01：readiness 检查阻塞了首次可见反馈

证据：`src/features/import/useImportWorkflow.ts:279-307`、`src/features/import/ImportView.tsx:153-155`。

`getReadiness()` 完成前不会创建或加载 session，页面只显示 loading 状态，因此本地 IPC、迁移检查或项目初始化只要慢几秒，用户连文件选择入口都看不到。第一性原理上，启动准备工作和“让用户知道应用还活着”不是同一件事。

建议：先渲染稳定的 Import shell 和明确的准备阶段文案；source methods 可以暂时 disabled，但要显示正在准备什么、已等待多久以及 Retry。readiness 最好后台加载，并设置超时后的非阻塞提示。验证时人为延迟 3–5 秒，确认用户始终能看到进度，而不是一片空白或无意义 spinner。

#### UX-P1-02：文件/文件夹添加没有接入忙碌状态，重复添加很容易发生

证据：`src/features/import/ImportSourceMethods.tsx:8-15,31-48,81-97`、`src/features/import/ImportView.tsx:168-180`、`src/features/import/useImportWorkflow.ts:315-338`。

组件已经定义了 `addingPaths` 和 `onError`，但 `ImportView` 没有传入它们。`addPaths` 发起的是后台发现任务，返回后才在 workflow 中 upsert task；因此选择文件、选择目录、原生拖放都可能在第一次添加尚未完成时再次触发。picker 失败也会因为没有 `onError` 而被静默吞掉。

建议：把添加操作纳入 workflow 的 mutation 状态，立即禁用同一入口并显示“正在扫描”；用批次 id 去重重复路径；失败时在 source methods 旁给出可见错误和 Retry。验证“连续双击、快速重复拖放、picker 取消、picker 异常、2 秒扫描延迟”五个场景，要求每个用户动作最多产生一个批次，并且每个失败都有可见反馈。

#### UX-P1-03：长目录扫描期间，导入队列看不到正在发生的事情

证据：`src-tauri/src/commands/import_v2_file_commands.rs:40-158`、`src/features/import/useImportWorkflow.ts:315-338`、`src/features/import/useImportWorkflow.ts:209-230`。

后端扫描任务会更新 task progress，扫描完成后才调用 `add_inputs`；前端添加目录时主要是 upsert task 并打开全局 task drawer，只有终态成功后才刷新 session 并继续处理。因此用户在主导入页面仍看不到已发现的文件、跳过的文件和当前批次的阶段，只能在另一个抽屉里猜测是否还在运行。这是“后台有进度、主流程无反馈”的典型断裂。

建议至少实现一个内联 discovery placeholder：显示 `正在扫描目录 · 已发现 N 个 · 已跳过 M 个`，并提供 Cancel。更好的方案是利用已有 batch progress 把发现结果分批推送到 queue；扫描失败时显示“保留已发现的 N 个 / 重试 / 查看日志”，而不是只显示全局任务失败。验收要覆盖空目录、100 个文件、含大量不支持格式的目录、扫描中取消和扫描部分失败。

#### UX-P1-04：批量启动被拆成大量子任务，并且每个子任务都尝试打开任务抽屉

证据：`src/features/import/useImportWorkflow.ts:188-207,377-393`。

批量来源进入队列后，workflow 会为新 item 启动任务；返回多个 task 后逐个 `openTaskDrawer(task.id)`。对用户而言，导入 100 个文件应该是一个可观察、可取消、可重试的批次，而不是 100 次任务中心跳转。大量抽屉事件还会造成视觉噪音、焦点跳转和不必要的渲染突刺。

建议：以 batch 为主要交互单位，任务抽屉只打开一次并展示聚合进度；子 item 只在主队列中更新状态。提供“暂停/取消批次”“仅重试失败项”“查看失败明细”，不要为每个 item 自动夺取焦点。验证 100/1000 个 item 的首个反馈时间、抽屉打开次数、同时运行数、取消响应时间和页面滚动稳定性。

#### UX-P1-05：Start/Retry/Cancel/选择等操作缺少统一的 per-item pending 状态

证据：`src/features/import/ImportView.tsx:85-148`、`src/features/import/ImportItemActions.tsx:23-42`、`src/features/import/useImportWorkflow.ts:365-422`。

这些操作大多通过 `void` 触发异步调用，行上没有“这个 item 正在执行某个动作”的本地状态。selection 也要等 IPC 返回后才变化；在慢机器或事件拥堵时，用户会再次点击同一个按钮，或者误以为勾选失败。通用 toast 只能说明命令报错，不能阻止重复命令，也不能说明恢复路径。

建议：建立 `pendingActionByItem`，在请求开始时立刻锁定当前 action，显示 spinner/“正在取消”等文案，成功后以事件刷新为准，失败则恢复按钮并提供 Retry。批量操作需要 batch-level pending，不能只依赖全局 task drawer。验证 500–1000ms IPC 延迟下的双击 Retry/Cancel、快速勾选/取消勾选、切换项目后旧请求返回，要求命令不重复、焦点不丢、旧项目状态不污染新项目。

#### UX-P1-06：进度和状态数字表达不完整，容易产生“假进度”

证据：`src/features/import/importViewModel.ts:72-80`、`src/features/import/ImportQueue.tsx:44-78`、`src/features/import/ImportView.tsx:53-56`。

当前队列头部主要显示 `completed / total` 的百分比，`active` 数量没有形成用户可见摘要；失败或取消也不计入 completed。比如 10 个 item 中 1 个成功、9 个失败，终态仍可能显示 `10% complete`。同时 header、filter、commit bar 对 waiting、needs_merge、preview_ready 的统计口径不同，用户无法从数字判断“还有什么阻塞确认”。

建议把“成功导入”和“已处理”分开：`已处理 10/10 · 成功 1 · 失败 9 · 处理中 0`。commit bar 明确显示“可确认 X 个 / 仍需处理 Y 个”，并与同一 selector 计算；`needs_merge` 要么暂时不可选，要么必须真正进入 per-item decision。验证 queued、active、waiting、preview_ready、needs_merge、failed、cancelled 的组合，以及 0 个 item、全失败、部分成功、重试成功四种终态。

#### UX-P1-07：批量失败后的恢复路径仍然断裂

证据：`src/features/import/ImportHistoryPanel.tsx:14-30`、`src/features/import/ImportView.tsx:139-180`、`src/features/import/ImportQueue.tsx:27-34`、`src/features/import/importStatusPresentation.ts:59`。

History panel 支持 `onOpenEntry`，但 `ImportView` 没有接入，所以历史条目即使有 `open_result` 或 `view_logs` 能力也不会显示动作。失败列表只有逐行 Retry，没有“重试全部失败项”；Load More 没有 pending 锁定或按 id 去重，快速点击可能重复请求和追加记录。大批量失败后，用户只能手工处理几十行，无法把失败批次当作一次可恢复的工作。

建议：先接通历史的“打开结果/查看日志”，再提供“重试所有失败项”和部分重试汇总；Load More 要有 loading/失败重试/重复 cursor 保护。每条失败记录保留简短的用户可读原因、受影响来源、Retry 和 View logs。验证完成、部分失败、全部失败、历史分页重复点击和应用重启后恢复。

### 本轮 P2：不一定阻塞功能，但会显著降低完成感和可理解性

#### UX-P2-01：自动开始的产品语义不够明确

证据：`src/features/import/useImportWorkflow.ts:188-218,340-363`。

文件扫描完成或 URL 添加后会自动启动新 queued item。自动开始可以减少一次点击，但它也意味着用户还没有整理、取消或预览，任务就已经消耗资源并改变状态。建议在 UI 明确写出“添加后自动开始”，或提供“加入队列 / 开始处理”两阶段模式；无论选择哪种，都不要让用户从状态变化反推产品规则。

#### UX-P2-02：空状态没有区分“空 session”“筛选无结果”和“全部完成”

证据：`src/features/import/ImportQueue.tsx:94-103`、`src/features/import/ImportView.tsx:169-180`。

所有空列表都使用同一文案；`ImportQueue` 虽然准备了 `onChooseFiles/onChooseFolder/onFocusUrl`，但当前 view 没有传入，空状态只有说明文字。建议分别提供“添加第一个来源”“当前筛选没有结果（清除筛选）”“全部已完成（查看历史）”，并保留清除筛选的快捷路径。

#### UX-P2-03：预览不是最终结果的可靠代理，也缺少 Retry

证据：`src/features/import/ImportMarkdownPreviewDialog.tsx:52-78,101-130`。

预览标题下直接展示 session/item/candidate 技术 id；图片统一替换成 `[image omitted]`，错误状态只有关闭，没有 Retry，复制失败也没有降级的文本选择或明确的恢复建议。用户打开预览的根本目的是判断“这个结果是否值得写入”，而当前预览无法验证图片完整性，也把过多内部标识暴露在主路径。

建议标题优先显示来源名和目标路径，技术 id 放到可展开的 details；本地资产可访问时显示缩略图，否则显示资源存在/缺失摘要；加载失败提供 Retry；复制失败提供“选择全部并复制”或导出到临时文件的明确路径。预览测试必须包含图片、长文、截断、加载失败和离线场景。

#### UX-P2-04：候选 diff 对话框的动作层级和平滑性不足

证据：`src/features/import/ImportCandidateDiffDialog.tsx`、`src/features/import/ImportView.tsx:118-135`。

选择 deterministic、current、agent、merged 和 discard 等动作在一个 footer 中平铺，触发后由父组件 `void` 执行，dialog 没有统一的 pending/disabled 状态。用户容易误点、重复提交或不知道哪个动作会改变最终写入内容。建议把“查看证据”“选择结果”“放弃候选”分组，显示当前选择，提交期间锁定动作并保留焦点；若是三方合并，则首先呈现真正可编辑的 merged content。

#### UX-P2-05：右侧 inspector 偏向内部诊断，不够像“下一步助手”

证据：`src/features/import/ImportRightPanel.tsx:79-101,105-127`。

右侧面板同时展示 route、engine id、SHA-256、artifact path 和 issue code，但没有把当前用户最需要的“现在需要我做什么”放在顶部。原始 issue code 也直接展示在用户主路径。建议首屏先给一个状态摘要和主 CTA（Preview、Retry、登录、授权、解决合并），把 route/engine/hash/path 放入 Details/Logs；技术诊断仍保留，但不应与行动入口竞争视觉层级。

#### UX-P2-06：筛选器和队列的读屏/键盘语义容易产生噪音

证据：`src/features/import/ImportQueue.tsx:72,108-120`。

整个 queue 被设为 `aria-live="polite"`，大量状态更新可能重复播报整块列表；行本身是 `role="listitem"`，没有 `aria-selected`，内嵌 checkbox 的 Space 行为也需要验证是否会同时触发行选择。建议只把紧凑的进度摘要和单项状态放入 live region，并明确 row selection、checkbox、action button 的焦点与事件边界。切换中英文时还要检查长文案下的 row layout，不要以视觉不溢出代替可操作性验证。

#### UX-P2-07：存在硬编码文案、乱码 fallback 和平台状态不一致

证据：`src/features/import/ImportHistoryPanel.tsx:27` 的 `路` 分隔符、`src/features/import/ImportCandidateDiffDialog.tsx` 的 fallback 文本、`src/features/import/ImportSourceMethods.tsx:37-48` 的静态平台 chip。

这些细节不会让 IPC 失败，但会直接降低产品可信度：英文界面出现中文字符，空 diff 显示异常字符，平台 chip 的“可用/不可用”与后端真实 readiness 不能互相解释。建议所有用户可见文本走 i18n，分隔符使用无语义符号；平台状态由 typed readiness 派生，至少区分 supported、needs capability、needs login、unavailable。

### 以流畅度为核心的验证矩阵

建议把下面的场景加入 Import V2 的 UI 集成测试和人工验收，而不只测最终状态：

| 场景 | 用户应该看到的事实 | 关键断言 |
| --- | --- | --- |
| 点击添加后的前 1 秒 | 动作已收到，入口进入 pending | 不重复发命令，按钮/拖放区状态改变 |
| 扫描持续 2–10 秒 | 正在扫描、已发现 N、已跳过 M | 主页面有内联进度，可取消，首批结果可见 |
| 100/1000 个来源 | 一个批次、聚合进度、稳定滚动 | 不为每个 item 打开抽屉，有整体取消/重试 |
| 一半成功、一半失败 | 成功与失败分开计数 | 失败 item 有原因、Retry、Logs；成功项不被回滚 |
| 全部失败后重试 | 仍在原批次上下文继续 | 可一键重试失败项，结果按 item 合并而不重复 |
| preview 加载失败 | 失败原因和 Retry 在原地出现 | 关闭不是唯一出口，不产生 unhandled rejection |
| 切换项目/重启应用 | 新项目不被旧请求污染，未完成任务可恢复 | scope/epoch 正确，焦点和筛选语义可恢复 |
| 中英文 + 键盘 | 文案完整、焦点明确、状态可读 | 无乱码/硬编码，live region 不重复播报整队列 |

### 建议的 UX 优先修复顺序

1. 先接入 source-level、batch-level、item-level pending 状态，修复重复添加、重复 action 和静默错误。
2. 把目录 discovery 变成主页面可见的连续过程，至少显示阶段、发现数、跳过数、取消和失败恢复。
3. 将任务抽屉从“每个 item 自动打开”改为“批次聚合”，补齐批次取消、失败汇总和一键重试。
4. 统一 progress/counts/needs_merge/commit bar 的状态口径，消除“可选但不计入提交”和“失败后仍是 10% complete”。
5. 接通 history actions、Retry/Logs、preview Retry，并把右侧 inspector 从诊断面板调整为下一步行动面板。
6. 最后处理空状态、图片预览、键盘语义、i18n 和平台 readiness；这些是完成感和可维护性的放大器。

这一轮的结论是：如果产品选择“本地工具、降低安全优先级”，可以暂时推迟部分安全加固，但不能推迟状态真实性和反馈闭环。对导入模块而言，用户感知的流畅度不是动画速度，而是每一步都能立即知道“已经收到、正在做什么、还剩什么、失败后怎么继续”。
## 第三轮补充：以流畅性和状态可信度为中心（2026-07-15）

本轮继续按第一性原理复核：导入动作必须立即可感知；耗时任务必须持续说明阶段、数量和当前可操作动作；同一批任务应被理解为一个整体；失败必须保留可继续工作的路径。鉴于这是本地工具，本轮不把 OS sandbox、攻击面和权限隔离作为主要评分项，但仍保留会导致数据丢失、重复执行或错误状态的边界。

### 本轮已修复

1. **队列交互模型修正**：队列从不完整的 `listbox/option` 改为普通 `list/listitem`，行仅在自身获得焦点时响应 Enter/Space；checkbox、Retry、Cancel 等子控件不再被行级键盘事件拦截。当前选中项通过 `aria-current` 表达。
2. **合并冲突不再出现“可勾选但不会提交”**：`needs_merge` 在解决候选前隐藏提交 checkbox，并从 `confirm()` 的 preview-ready 决策集合中排除；过时的 presentation 单测已同步修正。
3. **异步操作锁**：任务抽屉取消请求去重，并在后端返回 `cancelling` 时持续禁用；导入行级动作增加 per-item pending，避免候选比较、丢弃、重试等重复调用。
4. **历史分页状态**：Load more 增加请求锁、禁用态和按 entry id 去重；首次历史加载区分 loading、error、empty，并提供 Retry，不再把 IPC 失败伪装成“暂无历史”。
5. **输入与并发反馈**：非法 URL 现在有本地化校验、`aria-invalid` 和明确提示；URL 添加与路径扫描在前端 mutation scope 内互斥，避免两个 read-modify-write 请求互相覆盖。
6. **进度与语言**：仅在后端提供明确页数/字节数等 bounded metric 时显示百分比；四阶段 pipeline 继续使用 indeterminate 反馈；项目状态阶段文案可走中英文 i18n。失败项不再在 `retryable=false` 时显示 Retry，已有 task 的失败项可打开任务日志。
7. **项目任务隔离**：任务事件按 active project 过滤，避免其他项目的 Import task 混入当前任务抽屉和导入活动摘要。摘要明确标记为“当前任务列表中的导入活动”，不冒充真实 batch。

### 仍需后端契约或下一批实现

- **V2 history 仍是后端 stub**：`list_import_history_v2` 当前返回空的 V2 entries，只返回 legacy read-only projection；`ImportHistoryPanel` 已具备动作接口，但 `ImportView` 尚不能可靠地把 `open_result/view_logs` 映射到具体 session/item/task。下一步应先定义 entry action 的 typed DTO，再接入预览和任务日志，避免显示无效按钮。
- **失败恢复动作尚未端到端接通**：后端已经定义 `switch_parser`、`enable_ocr`、`skip`、`retry_route`、`switch_route` 等 recovery action，但前端没有对应 IPC/决策协议。当前只接通了可安全复用现有 task 的 `view_log`；不能用“显示一个按钮但不改变状态”的方式伪造恢复能力。
- **批次聚合仍是前端活动摘要，不是真正 batch identity**：主导入页已有批次内联汇总；全局任务抽屉只展示当前任务列表的导入活动，仍缺少后端 batch/session 关联、批次折叠和“仅重试失败项”。大批量导入的长期方案应以 batch 为交互单位，子任务作为明细。
- **检查环境阻塞**：`npm run check` 及针对本轮组件的 Vitest 均在测试启动前失败：`@tailwindcss/oxide-win32-x64-msvc` 原生 binding 报 `stream did not contain valid UTF-8`，Vite 同时报告 `spawn EPERM`。这不是断言失败；修复依赖/运行环境后必须从头重跑 `npm run check`。

### 本轮验证

| 检查 | 结果 |
| --- | --- |
| `npx tsc -b --pretty false` | 通过 |
| `npm run lint` | 通过 |
| locale JSON parse | 通过 |
| `npm run check:console` | 通过 |
| `git diff --check` | 通过（仅 CRLF 转换提示） |
| `npm run check` | 被 Vitest 启动环境阻断，见上文 |

## 第四轮补充：历史连续性与失败可继续（2026-07-15）

两轮独立复核把“历史能看到”进一步收敛为“历史可信、可继续操作”。本轮已完成以下高优先级修复：

1. `list_import_history_v2` 现在读取 `.app/import-history` 中真实的 V2 batch 记录，保留分页、状态、item 关联和读取 warning；可识别的 V2 记录不再被 legacy adapter 重复投影。
2. 历史查看改用只读的 `get_import_history_session_v2`，不再因为打开详情触发 session recovery、候选接收或 staging 改写。
3. 提交 batch 持久化 `batchTaskId`，历史日志优先打开提交任务，而不是误打开某个 item 的抽取任务。
4. 失败、取消和无结果记录始终提供“打开详情”，详情展示 item 状态与 issue；历史按钮增加 opening/disabled 反馈，分页失败会提示并可重试。
5. 提交任务进入终态后自动刷新历史首屏；分页追加时合并并保留已有 warning；历史异步响应回写前再次校验项目作用域。
6. legacy adapter 跳过 V2 import task 持久化结构，避免普通任务或 V2 任务污染“旧版导入历史”。

仍应作为下一批处理的高价值问题：历史详情目前仍借助 session item 作为显示补充，尚未形成完全不可变的 item snapshot；数字 offset cursor 在并发新增记录时可能跳过/重复；parser/OCR/skip/route recovery 仍缺少端到端 IPC 决策协议；大文件 preview 的完整 hash 校验也应补齐。

本轮代码验证：`npx tsc -b --pretty false`、`npm run lint`、`npm run check:console`、`npm run check:rust:gui`、locale JSON parse 与 `git diff --check` 通过；Rust no-default-features 测试已编译并执行 600 个库测试及各 Import V2 集成测试，除 `mvp_flow::ai_assisted_loop_fake_agent_detected_and_byok_runs` 因本机 `AppData\\Roaming` 写入权限失败外，其余已执行测试通过。Vitest/JS build 仍在启动阶段受本机 Tailwind oxide 原生 binding 无效 UTF-8 与 Vite `spawn EPERM` 阻断，尚未进入断言。

## 第五轮补充：恢复动作与任务连续性（2026-07-15）

本轮继续以“本地工具优先流畅完成”为判断标准，补齐了上一轮列出的恢复动作和任务状态连续性问题：

1. **恢复动作已打通**：`retry_route`、`switch_route`、`switch_parser`、`enable_ocr`、`skip` 和本地 ASR 授权已从 issue action 经由前端 workflow、类型化 IPC 连接到后端 orchestrator；按钮只在输入类型适用时展示，避免用户点击后执行同一条无效路线。
2. **Skip 具备原子语义**：后端在导入锁内先取消仍活跃的任务，再持久化 `skipped`，并清理 task/progress/preview/issue；取消竞态晚到时不会把已跳过条目错误改回 cancelled。
3. **取消后可继续**：`cancelled`、`skipped`、`paused` 条目可以绑定新任务重试；旧 task id 不再阻塞新任务领取，取消收尾也会清理旧绑定。
4. **任务状态不倒退**：事件、IPC 创建响应和 `list_tasks`/项目恢复快照都使用时间戳与终态优先合并，避免晚到的 queued/running 或空快照把任务抽屉、活动任务和批次进度倒退/清空；批量取消能够识别单项失败并给出反馈，且批量失败不重复刷单项 toast。
5. **等待态可恢复**：Waiting Login/Waiting Capability 任务从任务抽屉取消后，导入 session 刷新会将条目收敛到 cancelled/failed 可继续状态；本地 ASR 授权成功后自动刷新并重试条目。
6. **契约不再静默缺失**：`skipItem` 和 `authorizeLocalAsr` 成为必需 workflow 能力，避免界面显示恢复按钮但实现缺失时无响应；同时增加了任务快照和取消后重试的回归测试。

### 本轮验证

- `npx tsc -b --pretty false`：通过。
- `npm run lint`：通过。
- `npm run check:console`：通过。
- `npm run check:rust:gui`：通过，仅保留既有 `transaction.rs` 未使用方法 warning。
- `npm run test:rust`：602 个库测试通过，Import V2 集成测试通过；全量集成阶段仅 `mvp_flow::ai_assisted_loop_fake_agent_detected_and_byok_runs` 因本机 `%APPDATA%\\Roaming\\llm-wiki-desktop\\.settings.json.tmp` 写入权限（Windows `os error 5`）失败，与本轮 Import V2 改动无关。
- Import V2 定向集成测试：通过，`import_v2_core`、`import_v2_file_contracts`、`import_v2_file_discovery`、`import_v2_file_ingestion`、`import_v2_file_orchestration`、`import_v2_legacy_history` 共 17 个测试全部通过。
- 完整 `npm run check`/Vitest 仍被环境中的 Tailwind oxide native binding（`stream did not contain valid UTF-8`）和 Vite `spawn EPERM` 阻断，尚未进入 JS 断言。

### 仍值得下一轮处理

历史详情仍应进一步保存不可变 item snapshot；分页 cursor 应从 offset 升级为稳定 cursor；解析器/OCR 可用性最好由后端 readiness 直接驱动，避免只有一条路线时出现“切换后仍走同一路线”的错觉。

### 结论

本轮确认：对本地工具而言，当前最关键的质量不是再增加安全提示，而是让“收到、进行中、已处理、失败、取消、可继续动作”在主页面、队列、任务抽屉和历史区域保持同一语义。已修复的问题覆盖了重复操作、错误选择、伪进度、扫描取消、非法输入和历史加载误导；V2 history、完整恢复动作和真正 batch identity 需要下一轮以后端 DTO/IPC 契约为基础继续实现。

## 第六轮补充：历史可信度、分页稳定性与能力状态（2026-07-15）

本轮继续以“用户应看到真实、可复现、可继续的结果”为第一性原理。安全审查不再作为主要投入，但历史详情不能随当前 session 漂移、分页不能因新记录插入而重复/漏项、连接器不能只靠前端硬编码猜测，这三类问题会直接破坏本地工具的使用流畅度和信任感。

### 本轮已修复

1. **历史详情使用批次内不可变快照**：`ImportBatchResult` 现在在提交开始时保存参与本批次的 `ImportSession` 快照，并在每个 item 完成、失败或取消后更新快照中的最终状态和 issue。历史列表与只读历史 session IPC 优先读取该快照；只有旧格式记录才回退到当前 session，避免后续继续导入或 session 变化改写过去的历史叙述。
2. **历史分页从 offset 升级为稳定游标**：历史记录按文件修改时间、记录类型和 ID 做确定性排序；游标携带版本、高水位时间和上一条排序键。后续新增记录不会改变本次分页窗口，旧数字 offset 也会被明确拒绝，不再出现 Load more 重复或跳过记录。
3. **连接器状态由后端能力投影驱动**：新增 `ImportPlatformReadiness`，HTTP/微信基于已注册路由，知乎/Bilibili 基于能力包状态，二阶段平台明确返回 `phase_two`。前端 chip 保留可用/不可用的简洁视觉，同时为能力缺失、未开放和路由不可用提供中英文原因 tooltip；能力缺失不会锁死本地文件导入入口。
4. **历史详情的作用域继续保持只读**：打开历史详情会携带 `batchId` 读取对应批次，详情状态、数量、失败原因和 task 关联不再从当前正在工作的 session 猜测；历史加载和分页仍有请求锁、失败重试和项目作用域检查。

### 本轮验证

| 检查 | 结果 |
| --- | --- |
| `npx tsc -b --pretty false` | 通过 |
| `npm run lint` | 通过 |
| `npm run check:console` | 通过 |
| `npm run check:rust:gui` | 通过，仅有既有 `transaction.rs` 未使用方法 warning |
| Rust Import V2 定向集成测试 | 通过：`import_v2_core` 4、`import_v2_file_contracts` 1、`import_v2_legacy_history` 2 |
| 历史快照提交回归测试 | 通过；失败 item 不影响同批已提交 item，历史快照保留最终状态 |
| Vitest / `npm run check` | 仍被本机 Tailwind oxide 原生 binding 的 `stream did not contain valid UTF-8` 与 Vite `spawn EPERM` 阻断，未进入 JS 断言 |

### 仍需下一批处理

- 历史预览内容仍依赖 staging 文件是否保留；若产品要求长期离线查看，应把历史结果预览改为读取已提交 Wiki artifact 或持久化受控的 preview snapshot，而不是只保存 session 元数据。
- 当前 `batchTaskId` 已能把历史指向提交任务，但全局任务抽屉尚未拥有真正的后端 batch 聚合模型；大批量导入仍可继续优化为批次折叠、仅重试失败项和批次级日志入口。
- 完整 `npm run check` 需要先修复本地 Node 原生依赖/进程权限环境，然后从头重跑，不能把当前“编译通过”当作完整发布证据。
## 第七轮补充：历史预览不漂移与 readiness 未知态（2026-07-15）

第二轮共享上下文复核没有发现新的 P0，确认正常提交的历史快照已经和提交事务保持原子一致；但从“用户看到的结果必须可复现、未知状态不能伪装成可用、空操作不能留下后台悬挂任务”出发，又补充修复了四个高价值问题：

1. **历史 Markdown 预览改为批次专属副本**：每个成功提交的 item 会在同一提交事务中写入 `.app/import-history-previews/{batchId}/{itemId}.md`；历史预览读取该副本并执行完整 SHA-256 校验，避免 staging 被重新生成、清理或修改后历史页面内容漂移。
2. **空提交在创建历史前拒绝**：后端拒绝空 `decisions`，不再产生永久停留在 `processing` 的空批次；前端仍通过 selected ready count 禁用无效提交。
3. **readiness 失败采用未知态**：能力状态请求失败或尚未完成时，平台 chip 不再默认显示 HTTP/微信可用，而是显示“正在检查可用状态”，并把具体原因放进可访问描述；请求成功后再由后端真实 route/capability 状态覆盖。
4. **旧历史兼容回退显式提示**：没有 `historySnapshot` 的旧记录仍可 best-effort 打开，但详情页明确提示内容由当前 session 重建，可能与原始状态不同；新记录不走该回退。

本轮新增/更新的回归覆盖包括空 decisions 拒绝、成功批次历史预览副本存在、历史快照终态持久化，以及新增预览文件后的全量 commit crash-boundary 顺序。

仍建议后续处理：旧记录的稳定迁移时间、历史 attempts/warnings 的独立保留、rollback failure 的 `recovery_required` 状态，以及批次级日志聚合。它们影响可诊断性和长期维护，但不阻塞当前本地导入主流程。

### 本轮验证

| 检查 | 结果 |
| --- | --- |
| commit 模块定向 Rust 测试 | 22 passed |
| `npx tsc -b --pretty false` | 通过 |
| `npm run lint` | 通过 |
| `npm run check:console` | 通过 |
| `npm run check:rust:gui` | 通过，仅有既有 dead-code warning |
| `npm run check` | 仍在 Vitest 启动阶段被本机 Tailwind oxide native binding 的无效 UTF-8 与 Vite `spawn EPERM` 阻断 |

## 第八轮最终验证：体验优先修复后的质量门禁（2026-07-15）

本轮专项验证已不再停留在启动环境诊断，而是完整进入了测试与生产构建阶段。Import V2 的交互、状态、历史和恢复回归全部通过；统一检查目前只被两个与本轮无关的既有前端测试阻断。

### 结果

- Import V2 专项 Vitest：4 个测试文件、37 个测试全部通过。
- 全量 Vitest：96 个测试文件中 94 个通过，577 个测试中 575 个通过；仅有两个既有失败：`src/app/App.test.tsx` 的 `Tasks: 2 running` 状态文案断言，以及 `src/components/app/TaskLogDrawer.test.tsx` 的取消 IPC 调用断言。两者单独复跑仍失败，未触及 Import V2 代码或本轮导入测试。
- `npm run lint`、`npm run build`、`npm run check:console`、`npm run check:rust:gui`：通过。GUI Rust 仅保留既有 `FileTransaction` dead-code warning。
- `npm run test:rust`：604 个库测试、全部 Import V2 集成测试及其余集成测试通过；此前的 Windows AppData 权限失败在本次完整授权环境中不再复现。

### 本轮最终判断

从本地工具的第一性原理看，当前 Import V2 已具备“用户知道发生了什么、可以安全等待、可以继续失败项、历史结果不会漂移”的主流程体验。剩余建议按产品价值排队处理：稳定迁移旧记录的创建时间、独立保留 attempts/warnings、将 rollback failure 显式升级为 `recovery_required`，以及把全局任务抽屉提升为真正的 batch 聚合视图。它们是可诊断性和规模化体验的后续优化，不影响当前导入主流程。

## 第九轮补充：合并闭环与结果可解释性（2026-07-15）

本轮把上一轮审阅中仍会让用户“看到冲突但无法完成”的断点提升为最高优先级，并以“每个按钮都必须能推动状态前进”为验收标准。

### 已修复

1. **三方合并现在有真实的 merged buffer**：冲突对话框仍同时展示确定性基线、当前 Wiki、Agent 候选和 unified diff；新增可编辑的合并 Markdown 区域，默认填入 Agent 结果，用户可以在同一处手工整理后再应用。空合并内容会禁用应用按钮。
2. **合并动作绑定到用户看到的 Wiki 版本**：`apply_merged` 与三方冲突下的 `choose_agent` 都携带当前 Wiki SHA-256；后端重新读取并校验 hash，Wiki 在查看 diff 后发生变化时明确返回 stale，而不是静默覆盖。
3. **NeedsMerge 到提交完成闭环**：成功选择合并候选后，条目进入 `preview_ready`；前端按 item 保存 `apply_merged_candidate + expectedWikiHash`，提交时使用真正的合并预览内容。选择确定性结果、保留当前 Wiki、创建新文档、丢弃候选也分别落到可执行的回退/冲突策略，不再把空 merged 内容发送给后端。
4. **TaskResult 具备批次追溯**：V2 提交任务的 typed reference 增加可选 `batchId`，旧 JSON 缺失该字段仍可读取；旧导入预览路径同步补齐兼容模式。
5. **历史详情更可解释**：attempts 以折叠时间线展示 route、engine、stage、outcome、duration 和 attempt warning；质量 warning 单独汇总，避免同一 warning 重复出现。没有可重建 session 的旧历史不再显示必然失败的详情入口。

### 本轮验证

| 检查 | 结果 |
| --- | --- |
| 合并编辑器/提交决策前端定向测试 | 4 个文件、18 个测试通过 |
| Import V2 前端专项测试 | 7 个文件、49 个测试通过 |
| Agent candidate Rust 集成测试 | 4 个通过 |
| TaskResult reference Rust 回归 | 1 个通过，覆盖新旧 JSON 形状 |
| `npx tsc -b --pretty false` | 通过 |
| `npm run lint` | 通过 |
| `git diff --check` | 通过（仅有既有 CRLF 转换提示） |

### 仍待后续处理

真正的自动化三方 diff 算法、旧 task JSON 的完整迁移兼容、attempts/warnings 的独立持久化、批次级任务抽屉聚合，以及全量 `npm run check` 中与 Import V2 无关的既有前端断言失败，仍可作为下一批优化；它们不再阻断当前冲突项的“查看—合并—提交”主路径。

## 第十轮补充：批次连续性与注意力友好（2026-07-15）

本轮把重点从“操作是否能完成”进一步推进到“用户能否在长任务中保持方向感”。对本地工具而言，最昂贵的体验成本不是一次额外点击，而是页面跳转、进度口径冲突、批次消失和用户无法判断下一步。因此本轮优先处理批量导入、页面切换、历史入口和长列表。

### 已修复

1. **批次状态跨页面保留**：`useImportWorkflow` 不再因为切换到 Wiki/Chat 等工作区而清空 session、pending 状态和批次 task IDs；返回 Import 页面时复用当前项目 session 和批次上下文，后台任务仍由全局事件监听收敛。
2. **批次边界不再靠“当前是否有活跃任务”猜测**：每次 `startItems` 调用作为一个独立操作批次，单项 retry 不会被错误并入旧批次，避免计数重复和批次取消误伤后续操作。
3. **任务缺失不会制造永久进行中**：task-store 中已被清理的任务计为 unavailable，而不是 active；取消逻辑不会对不存在的 task 发 IPC，批次可以关闭，并向用户解释状态不可用。
4. **批次明细真正按需展开**：主卡片只播报聚合进度；任务列表隐藏在显式 disclosure 内，拥有内部滚动，日志按钮必须由用户主动点击。批量启动和扫描完成不再自动把焦点带到第一个子任务日志。
5. **进度语义分层**：批次卡片明确显示“解析任务”进度，避免把 task 完成误读为 Wiki 已写入；队列继续显示 session/source 的 processed 状态。
6. **历史操作更直接且可理解**：`open_result` 有历史预览证据时直接打开 Markdown 预览；历史行、历史 item 的重复按钮补充带标题的中英文可访问名称；时间和 attempts duration 跟随当前界面语言。
7. **长任务和错误信息可见**：批次明细限制高度并可滚动；扫描失败原因从 hover-only title 提升为可见错误说明；扫描中追加来源不会重置用户已经加载的队列分页。

### 第一性原理判断

- 批量操作的最小可理解单位是“这一次批量解析”，不是某个随机子任务；因此主页面先呈现批次，再把任务日志作为诊断入口。
- 状态缺失不是进行中。把不可观测状态当 active 会让取消/关闭失效，直接破坏用户对系统的控制感。
- `open result` 的承诺是结果，而不是详情目录；如果结果证据存在，应一次点击到达可读内容。
- 多个相同按钮在视觉上可以简洁，在辅助技术和键盘快速导航中却必须携带作用对象，否则用户无法可靠选择。

### 本轮验证

| 检查 | 结果 |
| --- | --- |
| `npx tsc -b --pretty false` | 通过 |
| `npm run lint` | 通过 |
| `npm run check:console` | 通过 |
| JSON locale parse / `git diff --check` | 通过；仅有既有 CRLF 转换提示 |
| Import V2 / full Vitest | 当前工作树被 Tailwind oxide 原生 binding 的 `stream did not contain valid UTF-8` 与 Vite `spawn EPERM` 阻断，未进入 JS 断言 |

### 下一批建议

仍建议将真实 batch identity 下沉到后端 DTO/IPC，以支持多个并行批次的独立取消、失败项重试和批次级日志聚合；同时可以继续优化长文件名的键盘可查看/复制路径入口、任务抽屉焦点转移，以及 session 同步慢于 task 终态时的短暂同步态提示。

### 验证补充

- 提升权限后 `npm run build` 成功，Import V2 定向 Vitest 为 4 个文件、30 个测试全部通过。
- 提升权限后全量 Vitest 为 94/96 文件、584/586 测试通过；剩余两个失败仍是既有 `App.test.tsx` 任务状态文案断言和 `TaskLogDrawer.test.tsx` 取消 IPC 断言。
- `npm run lint`、`npm run check:console`、`npm run check:rust:gui` 通过；Rust `--no-default-features` 库测试 605 个通过，Import V2 集成测试全部通过。
- 完整 Rust 集成阶段仍有一个既有 `mvp_flow::ai_assisted_loop_fake_agent_detected_and_byok_runs` 因 Windows AppData `.settings.json.tmp` 权限（os error 5）失败；没有触及本轮 Import V2 代码。

## 第十一轮补充：真实批次身份、恢复窗口与低打扰反馈（2026-07-15）

本轮专门按“本地工具优先流畅度”复核：用户发起一次批量导入后，系统必须能持续回答四个问题——这批任务是谁、现在做到哪、我能否立即取消、页面重开后还能不能继续理解。安全攻击面不是本轮主要评分项，但会导致批次误取消、状态回退、操作无效或结果消失的竞态仍按高优先级处理。

### 两轮独立盲审发现的关键问题

两位只读审查代理分别从状态连续性、键盘体验、长文件名、测试覆盖和并行操作出发，确认了以下真实风险：

- 仅把一组 task id 保存在 hook 内存中，无法区分并行批次，也无法在重启后恢复；
- task 创建返回时，session item 的 `taskId` 可能尚未由 worker 写回，立即点击取消会返回空集合但任务仍会执行；
- task store 尚未 hydrate 时，恢复批次被当成 `unknown`，用户可能在恢复窗口内把批次关闭，随后失去取消入口；
- 旧的 session refresh 可能晚于新增源/选择变更返回，覆盖用户刚刚看到的队列；
- 所有任务不可取消时仍显示一个点击后无效果的 Cancel batch；全部取消的批次却使用绿色成功图标；
- 任务抽屉缺少 Escape、焦点边界和关闭后的可靠回焦；
- 长 URL/路径虽然视觉截断，但复制按钮文案仍误称为 path，扫描完成也没有展示 added/skipped 的结构化反馈。

### 本轮已修复

1. **批次身份下沉到后端 task DTO**：每次 `start_import_items_v2` IPC 调用生成一个持久化 `batchId`，写入所有子 task；前端按 batch 聚合，因此并行启动的 A/B 批次不会互相覆盖。task store 对缺失 `batchId` 的旧事件保留已有身份，避免兼容快照让批次凭空消失。
2. **取消从“session 映射”改为“持久 task identity”**：`cancel_import_batch_v2` 仍校验项目/session 上下文，但实际按 `projectId + batchId` 查询 task；不再等待 orchestrator 把 `item.taskId` 写回 session，所以任务刚创建就点击取消也能命中。前端只取消目标批次内可取消的任务，并逐批维护 cancelling 状态。
3. **批次可恢复、可重试、可分别关闭**：hook 保存多个 batch record，重启/remount 后从未完成 session item + task.batchId 重建；任务缺失时保留 item 标题和 unavailable 明细；失败项可按批次重新启动，旧失败批次不会吞并新 retry 批次。
4. **恢复窗口不再误导用户**：task registry 增加 hydration 状态，hydrate 完成前不允许 dismiss unknown batch；hydrate 后才把确实缺失的 task 标记为 unavailable。活动任务全部不可取消时不再展示无效取消按钮；取消态使用非成功图标，避免“已取消”与绿色完成产生语义冲突。
5. **session 刷新增加 mutation revision**：refresh response 只有在启动时的 revision 仍是最新时才可 replaceSession。新增 URL/path、选择、重试、skip、取消和授权等用户动作会使旧 refresh 失效；源入口在同步期间禁用，避免用户对过期队列继续操作。
6. **任务抽屉形成完整键盘生命周期**：抽屉使用 `role=dialog` + `aria-modal`，打开将焦点送入关闭按钮，Tab/Shift+Tab 在抽屉内循环，Escape 关闭，关闭后回到触发按钮；这让长任务日志不会劫持用户注意力，也不再把 Escape 传给底层面板。
7. **长 locator 和扫描结果更可解释**：队列保留视觉截断，但复制按钮可复制完整 path/URL，文案统一为 source locator；Clipboard API 不可用时增加 DOM fallback。扫描完成从只显示 discovered 扩展为 discovered/added/skipped，用户能区分“扫描到多少”和“真正加入多少”。

### 第一性原理结论

- **批次边界必须等于用户操作边界**：随机子 task 只是执行细节；取消、重试、进度和恢复都应以一次 IPC 操作为单位。
- **未知不等于已完成，也不等于可关闭**：在事实尚未 hydrate 前，系统应先保护用户的控制权；确认不可观测后再提供关闭。
- **异步刷新不能覆盖更近的用户意图**：时间更晚返回的请求不代表事实更新，必须带 mutation/version 语义。
- **不可执行的按钮就是负反馈**：不可取消任务不展示取消；失败可继续就展示 retry；结果数字需要让用户理解“发现”和“加入”的差异。
- **长任务体验是注意力管理**：主页面显示聚合和下一步，抽屉保留诊断细节；焦点、Escape、复制完整 locator 都是完成任务所需的连续性，而非装饰性可访问性。

### 仍需后续处理

- 目录 discovery 的 task id 仍主要保存在前端内存；重启中断的扫描尚未在 session 中形成持久关联，后续应展示“扫描因重启中断，可重新扫描”的明确恢复入口。
- 全局任务抽屉目前已有 Import 汇总和批次明细，但还没有后端统一的 batch log aggregation；大批量场景可继续增加批次折叠、批次级日志和跨页面恢复摘要。
- `open_result` 仍主要依赖后端 action 可用性，前端可以进一步显式校验“已提交结果”状态，并在历史 artifact 缺失时展示可恢复原因。
- 完整 Vitest/生产构建需要修复本机 `@tailwindcss/oxide-win32-x64-msvc` 原生 binding 和 Vite `spawn EPERM` 环境后重新执行；这属于验证环境阻断，不应被误报为功能测试通过。

### 本轮验证

| 检查 | 结果 |
| --- | --- |
| `npx tsc -b --pretty false` | 通过 |
| `npm run lint` | 通过 |
| `npm run check:console` | 通过 |
| `npm run check:rust:gui` | 通过，仅有既有 `transaction.rs` dead-code warning |
| Rust task/model 定向测试 | 54 passed，包含 batch identity、task cancellation、orchestrator recovery |
| Rust no-default-features 全量 | 606 个库测试与 Import V2 集成测试通过；全量集成仅已有 Windows AppData `.settings.json.tmp` `os error 5` 用例失败 |
| `npm run check` | 在 Vitest 启动阶段被 Tailwind oxide 无效 UTF-8 / Vite `spawn EPERM` 阻断 |
| `npm run build` | `tsc` 阶段通过，Vite 阶段被同一 native binding / `spawn EPERM` 阻断 |
| Import V2 前端专项 Vitest | 当前环境无法进入 Vitest 配置加载，未将启动失败冒充断言结果 |
