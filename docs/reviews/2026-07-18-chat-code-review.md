# Chat 模块代码审查（前端、后端与视觉体验）

- 审查日期：2026-07-18
- 审查对象：`codex/import-v2-final-integration` worktree 当前工作区
- 基线提交：`f2375ae68346e9994bf2da5d1ebdb3be36d0a9ac`
- 范围：Chat 会话、检索与引用、Agent/BYOK 路由、流式任务、保存到 Wiki、Wiki 页面侧 Chat、Chat 便捷写入、右侧引用面板、样式和可访问性
- 依据：项目 `llm-wiki-desktop-context` skill、`AGENTS.md`、`SPEC/PRD.md`、`SPEC/SPEC.md` 第 16 节、`SPEC/APP_flow.md`、`SPEC/BACKEND_STRUCTURE.md`、`SPEC/FRONTEND_GUIDELINES.md`、`UI-Frontend-design/chat.html` 与当前实现/测试

> 说明：目标 worktree 本身包含大量未提交的 Import V2 集成改动。本次只审查当前工作区实际代码，不把这些既有改动还原，也不修改业务实现。

## 结论摘要

Chat 的主闭环已经具备：多会话、Agent/BYOK 路由、流式输出、引用解析、保存到 `wiki/queries/`、页面级 Chat、任务日志和便捷写入审计都有实现基础。但当前还不能视为“安全完整”：便捷写入存在两个可能造成内容丢失或留下未审计改动的 P0 问题；普通 Chat 会话也缺少并发一致性保护，快速切会话/切页面仍有竞态。

建议发布阻断顺序：

1. 先关闭 P0：重做便捷写入的隔离、失败回滚和回滚作用域。
2. 再处理会话并发、异步选择、页面切换与任务事实守卫。
3. 补齐引用可信度、保存结果、固定页面上下文和异常恢复。
4. 最后按设计稿统一 Composer、会话栏、右面板和空状态。

## Findings

### P0-1：便捷写入的回滚会重置整个 worktree，可能删除 Agent 之外的并发编辑

证据：

- `src-tauri/src/commands/chat_commands.rs:564-566`：硬违规直接调用 `rollback_worktree_to_head_preserving_ignored`。
- `src-tauri/src/commands/chat_commands.rs:1088-1105`：手动回滚只检查 Git HEAD 等于 checkpoint，然后同样回滚整个 worktree。
- `src-tauri/src/services/git_service.rs:344-356`：实际执行 `git restore --source=HEAD --staged --worktree -- .`、`git clean -fd -- .`，作用域是项目全部路径。

影响：

- checkpoint 之后，用户或外部编辑器对另一篇 Markdown 的未提交修改不会改变 HEAD。
- 此时点击“回滚上次便捷编辑”仍会通过 HEAD 检查，并把这些后续修改一起恢复/清理。
- `git clean -fd` 还会删除 checkpoint 后新建的未跟踪文件；这违反“保留外部 Markdown 编辑”和“高风险操作只影响已确认路径”的项目硬约束。

建议：

- 不要在用户项目 worktree 中直接运行可写 Agent；优先使用隔离 worktree/临时候选目录，审计通过后再按明确 diff 合并。
- 若短期仍在原 worktree 执行，至少保存 Agent 开始前的逐路径内容哈希、文件状态和允许路径集合，只回滚本次 Agent 实际改变且哈希仍匹配的路径；检测到并发修改时进入冲突确认，绝不执行全项目 reset/clean。
- 增加“checkpoint 后用户编辑其他 tracked 文件”和“新建 untracked 文件”两个回归测试。

### P0-2：便捷写入在 Agent 报错或取消后直接返回，可能留下未审计、未回滚的半成品改动

证据：

- `src-tauri/src/commands/chat_commands.rs:503-520`：Agent 运行出错后直接 `return Err(error)`。
- `src-tauri/src/commands/chat_commands.rs:532-539`：Agent 返回后发现取消也直接返回。
- `src-tauri/src/commands/chat_commands.rs:541-550`：变更枚举或 diff 获取出错时也会通过 `?` 离开；没有统一清理 guard。
- checkpoint 已在 `src-tauri/src/commands/chat_commands.rs:457-462` 创建，说明此后的 Agent 被允许真实写入项目。

影响：

- 可写 Agent 在被杀死、CLI 异常退出、输出解析失败或用户取消前，可能已经写入若干文件。
- 任务最终显示失败/取消，但这些文件不会进入审计，也不会自动回滚，用户很难知道项目已经被改变。
- 这破坏了“失败不能损坏既有 Wiki”和“Agent 自动修改必须有可恢复边界”的核心安全承诺。

建议：

- checkpoint 创建后进入 RAII/事务式 guard：只有“审计完成 + 状态已持久化”才能 commit guard；其他所有退出路径统一安全恢复或生成必须处理的 PendingAction。
- 取消应先停止进程，再审计当前差异；不能把“任务取消”当作“文件没有变化”。
- 即使回滚失败，也必须把受影响路径、diff 和恢复指引写入任务结果，并阻止任务被标记为成功。

### P1-1：回滚失败仍被标记为 Chat 任务成功

证据：

- `src-tauri/src/commands/chat_commands.rs:573-580`：硬违规回滚失败被转换为 `ChatConvenienceEditStatus::RollbackFailed`，但函数继续执行。
- `src-tauri/src/commands/chat_commands.rs:626-641`：最终统一写入 “finished” 结果并把任务状态切为 `Succeeded`。

影响：危险改动仍留在项目中时，任务抽屉和通知却会给出成功事实；用户可能关闭日志，误以为安全边界已生效。

建议：将回滚失败设为 `Failed` 或显式 `NeedsReview`，附带高风险 PendingAction、受影响路径和 diff；前端使用 danger 状态并保持操作入口可见。

### P1-2：会话 JSON 没有版本/锁，并发发送、重命名或删除会产生丢消息、丢标题和“删除后复活”

证据：

- `src-tauri/src/commands/chat_commands.rs:136-153`：发送任务加载整份 session 后把 user message 写回。
- `src-tauri/src/commands/chat_commands.rs:382-407`：模型完成后继续使用先前内存中的整份 session 写回。
- `src-tauri/src/services/chat_service/sessions.rs:151-169`：`append_message` 修改传入对象后原子覆盖整个 JSON，没有 revision/hash/互斥锁。
- `src-tauri/src/services/chat_service/sessions.rs:114-148`：rename/delete 与发送使用同一文件，但没有协调。
- 前端生成期间仍允许工具栏和会话列表执行 rename/delete：`src/features/chat/ChatView.tsx:521-542`、`src/features/chat/ChatSessionList.tsx:128-145`。

可复现场景：

- 两个发送任务都基于同一个旧 session 生成，后写入者覆盖先写入者的消息。
- 生成中重命名会话，assistant 写回旧快照后标题恢复旧值。
- 生成中删除会话，assistant 写回时会重新创建同名 JSON，表现为被删除的会话“复活”。

建议：后端按 `(project, sessionId)` 串行化 mutation；assistant 完成时重新加载最新 session，只追加自己的 answer，并使用 revision/expected hash 防止错误覆盖。删除/重命名要与 in-flight send 明确冲突或排队。补同会话并发发送、生成中 rename/delete 的集成测试。

### P1-3：快速切会话时 `activeSessionId` 与 `activeSession` 会短暂或永久错配

证据：

- `src/stores/chatStore.ts:300-313`：`selectSession` 先提交新 `activeSessionId`，但不清空旧 `activeSession`；await 后也只有项目 scope 检查，没有 selection epoch。
- `loadingSession` 虽存在于 store（`src/stores/chatStore.ts:73-74`），ChatView 没有消费它。
- `src/features/chat/ChatView.tsx:203-259`：UI 按旧 `activeSession` 渲染，但保存等操作使用新 `activeSessionId`。

影响：

- 从 A 快速点 B 时，界面暂时显示 A 内容但 store 已认为 B 活跃；用户点击保存可能拿 B 的 session id 去保存 A 的 message id。
- A、B 两次加载乱序返回时，较慢的 A 可以覆盖 B 的内容，形成 `id=B/content=A` 的持续错配。

建议：增加 session-selection epoch；只原子提交匹配的 `id + session`。选择开始时清空旧内容或显示 skeleton，并在 UI 中禁用依赖 session identity 的操作。

### P1-4：Wiki 页面第一次发送绕过了已有 page epoch，快速切页仍可能把旧页会话写回当前 store

证据：

- `ensurePageSession` 本身有 `pageSessionEpoch`：`src/stores/chatStore.ts:232-297`。
- 但 PageChat 第一次发送走通用 `createSession`：`src/features/chat/PageChatPanel.tsx:110-125`。
- 通用 `createSession` 在多个 await 后调用 `loadSessions`、`selectSession`，只检查项目 scope，不检查页面 epoch：`src/stores/chatStore.ts:208-229`。

影响：页面 A 首次发送时切到 B，如果 B 已经完成会话绑定，A 的通用创建流程仍可能在稍后把 A 选回全局 chat store；B 面板随后因 scope 不匹配显示为空，直到再次刷新。该路径没有被现有 `ensurePageSession` supersession 测试覆盖。

建议：PageChat 首次发送也通过带 page identity/epoch 的专用 intent 创建；每个 await 后验证 project key、page path 和 epoch，最终只为原页面启动任务，不接管当前页面的展示状态。

### P1-5：项目切换发生在发送 IPC 返回前时，合法后台任务不会被写入全局 taskStore

证据：`src/stores/chatStore.ts:359-386` 在 `invoke` 返回后先执行 `isProjectScopeCurrent(scope)`，失效时直接返回；`upsertTask(task)` 位于该 guard 之后。

影响：这与项目约定“backend task facts 是 scope guard 的例外；合法任务必须全局 upsert，只抑制旧项目的 drawer/navigation/toast”直接冲突。极端情况下任务已经在后端运行，但当前应用任务历史缺少它的初始事实，只能依赖后续事件偶然补齐。

建议：收到合法 `BackendTask` 后先无条件 upsert；随后再用 project key 判断是否设置 `sendTaskId`、显示流文本或打开界面。

### P1-6：检索预算可能被 `wiki/index.md` 吃完，使页面级 Chat 的 pinned page 只有引用壳、没有正文

证据：

- `src-tauri/src/services/chat_service/retrieval.rs:108-133`：index 作为 required candidate 先于 pinned page 入队。
- `src-tauri/src/services/chat_service/retrieval.rs:185-216`：required/index 无论剩余预算多少都进入；先按全部 remaining budget 截取，之后 pinned page 即使 required，也可能只生成 `excerpt=None`。

影响：当 index 较长时，“Ask AI 当前页”最重要的页面内容反而没有进入 prompt，违反 SPEC 16.3 的固定页优先约束。UI 仍可能显示 pinned 标记，使用户误以为模型读取了当前页。

建议：为 index、pinned、keyword/history 分配独立最低额度；页面 Chat 先预留 pinned 正文预算，再放 index。diagnostics 应区分“selected metadata”与“included characters”。增加超长 index + pinned page 的边界测试。

### P1-7：后端记录了无效引用与 `[unverified]`，前端完全不展示可信度告警

证据：

- `src-tauri/src/services/chat_service/citations.rs:7-54` 会记录 `invalid_source_ids` 和 `has_unverified`。
- `src-tauri/src/commands/chat_commands.rs:376-392` 将这些值持久化到 retrieval diagnostics。
- `src/types/chat.ts` 已暴露 `invalidCitationIds`、`hasUnverified`，但 ChatView、PageChatPanel 和 RightContextPanel 都只消费 `citations`。

影响：模型输出不存在的 `[S99]` 或明确写出 `[unverified]` 时，用户看不到警告；右面板只展示验证通过的来源，容易形成“回答已全部有据”的错觉。

建议：在回答头部和右面板增加“含未验证陈述 / N 个无效引用”的安静 warning，支持展开 diagnostics；保存到 Wiki 时也保留可见的可信度元数据。

### P1-8：保存到 Wiki 的结果路径被丢弃，重启后也不知道某条回答是否已经保存

证据：

- `src/stores/chatStore.ts:426-440` 收到 `SaveAnswerResult` 后只把状态设为 `saved`，没有保存/展示 `result.path`。
- `src/features/chat/ChatView.tsx:639-652` 保存后只显示“已保存”，不显示或打开目标。
- `ChatMessage` 没有 saved path 元数据；重启或重新加载项目后，所有 saveStatus 又回到 idle。
- `SPEC/FRONTEND_GUIDELINES.md:471-474` 要求明确显示 `wiki/queries/` 目标路径。

影响：用户无法从 Chat 回到生成的 query 页面；同一回答在重启后再次点击保存只会进入覆盖冲突。右面板保存按钮同样没有结果反馈。

建议：至少在 store 中保存 `{messageId, path, checkpoint}` 并提供“打开页面”；更稳妥的是把 `savedAnswerPath` 持久化回 session message，形成可恢复、幂等的状态。

### P1-9：覆盖确认被插在消息流底部，可能完全不在视口中，右面板也不显示冲突状态

证据：

- `src/features/chat/ChatView.tsx:273-296` 把 overwrite UI 放在消息列表末尾。
- `useTranscriptScroll` 的自动滚动依赖 message count/stream/activity，不依赖 `overwriteRequest`：`src/features/chat/ChatView.tsx:592-618`。
- 右面板保存入口在 `src/components/app/RightContextPanel.tsx:185-192`，但右面板不消费 `overwriteRequest` 或 `exists` 状态。

影响：用户从右面板点击保存，碰到同名文件时，当前视口可能仍停在回答处；看不到底部确认，也没有 toast/dialog，按钮还可再次点击并创建更多待确认 action。

建议：使用全局 `ProjectConfirmationController`/统一 dialog 展示 backend-issued PendingAction；若保留内联确认，至少滚动并聚焦、在右面板同步显示、禁止重复提交并处理 action 过期。

### P1-10：Markdown 引用预处理会改写代码块和行内代码的原始内容

证据：`src/features/chat/MessageContent.tsx:15-28` 在 Markdown parse 之前对整段字符串做正则替换，把所有 `[S1]` 改成 Markdown link。

影响：若回答给出包含 `[S1]` 的代码、正则、数组索引或示例文本，代码块中展示的内容会变成 `[S1](citation://S1)`；这不是样式问题，而是内容被篡改。后端 citation parser 同样会把代码里的 `[S1]` 当成真实引用。

建议：用 remark AST plugin 只处理普通 text node，跳过 `code`、`inlineCode`、link、math；后端解析也应采用同等规则或要求模型输出结构化 citation ids。补 fenced code、inline code、footnote、数学公式测试。

### P1-11：删除会话只靠 `window.confirm`，后端直接物理删除，没有 checkpoint、回收站或 backend-issued confirmation

证据：

- 前端确认：`src/features/chat/ChatView.tsx:186-193`、`src/features/chat/ChatView.tsx:530-536`。
- 后端删除：`src-tauri/src/services/chat_service/sessions.rs:136-148` 直接 `remove_file`。

影响：`.app/chats/*.json` 是用户会话事实来源，删除后不可从应用恢复；前端确认也可被其他 IPC 调用绕过。该实现低于项目对 destructive file operation 的统一安全边界。

建议：使用 backend-issued PendingAction；删除前建立可恢复 checkpoint，或先移动到 `.app/trash/chats/` 并提供撤销。生成中的会话应拒绝删除或先取消并等待任务完成。

### P2-1：Agent 路由在 async runtime 任务中执行阻塞进程循环

证据：`send_chat_message` 使用 `tauri::async_runtime::spawn`（`src-tauri/src/commands/chat_commands.rs:106-122`），随后在 Agent 分支同步调用 `run_task_streaming_with_events`（`src-tauri/src/commands/chat_commands.rs:271-277`、`:503-509`）。

影响：长时间 Agent 进程会占用 Tokio worker；并行任务、IPC 和取消响应可能变慢。BYOK 是真正 async，但两条 Chat 路由的调度模型不一致。

建议：把阻塞 Agent runner 放入 `spawn_blocking` 或统一后台 task executor；检索阶段也增加取消检查。补多任务并发和取消延迟测试。

### P2-2：发送请求缺少后端内容校验和长度上限

证据：`SendChatMessageRequest.content` 是裸 `String`（`src-tauri/src/models/chat.rs:237-251`），command 在创建 task 前不校验空值或上限；仅前端 `ChatComposer` 做 trim（`src/features/chat/ChatComposer.tsx:44-50`）。

影响：IPC 可写入空消息或超大消息，造成异常 session 文件、检索/模型成本和 UI 卡顿；前端也没有 `maxLength` 或字符/上下文提示。

建议：后端统一 trim、拒绝空消息、限制合理字符数并返回 typed error；Composer 展示接近上限的计数。

### P2-3：损坏的会话文件被静默跳过，用户没有恢复入口

证据：`src-tauri/src/services/chat_service/sessions.rs:46-83` 在 list 时跳过 parse 失败文件，只写 `eprintln!`。

影响：一个会话损坏后会像“被删除”一样从列表消失；GUI 用户通常看不到 stderr，也不知道文件仍在或如何备份修复。

建议：list DTO 返回 warnings/corrupt summaries，列表显示可恢复错误行并提供“打开所在位置/导出原始 JSON”，不要自动覆盖损坏文件。

### P2-4：当前问题在 prompt 中出现两次，浪费 history budget 并可能放大提问权重

证据：user message 先被追加到 session（`src-tauri/src/commands/chat_commands.rs:136-153`）；`append_prompt_history` 又遍历全部消息（`src-tauri/src/services/chat_service/retrieval.rs:598-635`），之后 `append_prompt_common` 再追加 `Latest question`（`:581-584`）。

建议：history 明确排除本轮 user message，或先构建检索上下文再持久化本轮消息；增加“latest question only once”测试。

### P2-5：全局 Chat 没有使用后端已有的 pinned page 能力，也缺少设计稿中的“引用页面/附加资料”入口

证据：

- store/API 已支持 `pinnedPagePath`：`src/stores/chatStore.ts:30-35`、`:354-370`。
- PageChat 会传当前页：`src/features/chat/PageChatPanel.tsx:123-125`。
- 全局 Chat 的 `handleSend` 没有传 pinned path：`src/features/chat/ChatView.tsx:138-150`。
- 设计稿 `UI-Frontend-design/chat.html:269-280` 有“附加资料”“引用页面”和上下文提示。

影响：用户在全局 Chat 中无法明确指定一篇页面作为强上下文，只能依赖关键词检索；这是已有后端能力未完成的前端闭环。

建议：Composer 增加可移除的页面 context chips；先支持从 Wiki 当前选中页/搜索选择页固定，资料附件若超出 MVP 可先不实现文件上传。

### P2-6：新会话不会根据首问自动命名，页面级标题还硬编码英文

证据：

- 后端默认始终为 `New chat`：`src-tauri/src/services/chat_service/sessions.rs:8-30`；首次发送后不更新标题。
- 页面会话使用硬编码 `Ask:`：`src/stores/chatStore.ts:284-286`、`src/features/chat/PageChatPanel.tsx:114-119`。
- 会话搜索只搜 title：`src/features/chat/ChatSessionList.tsx:51-54`。

影响：普通用户会积累大量同名 “New chat”，搜索和回访价值很低；中文界面仍出现英文系统标题。

建议：第一次 user message 成功落盘后，用本地截断规则生成标题（不额外调用模型），并持久化“用户是否手动改名”以避免覆盖；页面会话标题从 i18n 层传入。

## 前端视觉与交互改进

以下不应阻塞 P0/P1 修复，但能明显提升完成度。

### 1. Composer 重构为一个完整输入容器

当前 `ChatComposer.tsx:58-102` 是“顶部 badge/取消 + 下方 textarea/发送”的两段式布局，信息层级松散。建议对齐 `UI-Frontend-design/chat.html:269-282`：

- 使用一个带边框、圆角和 focus-within 状态的 composer card。
- textarea 自动增高（1-6 行），底部 action rail 放 route、固定页面 chip、上下文预算、停止和发送。
- 发送作为紧凑 icon+文字主按钮；停止只在生成时出现并保持位置稳定。
- compact PageChat 复用同一视觉语法，只隐藏非必要元信息，不另造一套布局。

### 2. 会话栏增加层级、状态和键盘可达性

当前列表只显示平铺 title/time/count。建议：

- 按“今天 / 昨天 / 更早”分组，或至少使用相对日期；当前 `formatTime` 对几天前的会话仍只显示 HH:MM。
- 区分 global chat 与 page-scoped chat，增加小型页面图标/路径 tooltip，并支持过滤。
- 生成中的 session 显示静态状态点，失败显示 warning 点。
- 补设计稿中的底部持久化说明（`.app/chats/`、`wiki/queries/`），强化本地优先心智。
- `ChatSessionList.tsx:128-145` 的 rename/delete 只在 `group-hover` 时 `display:flex`；键盘用户无法把焦点移到隐藏按钮。改为 `group-focus-within:flex`，并用语义 list/option 或 roving focus。

### 3. 减少引用信息重复，强化“最新回答”的右面板

当前每条 assistant message 下方展示完整 citation cards，同时右面板再次展示最新引用。建议：

- transcript 内只保留可点击编号与一行紧凑来源摘要；完整 snippet、路径和来源 lineage 放右面板。
- 右面板编号使用 `citation.sourceId`，不要总用 `index + 1`（`RightContextPanel.tsx:130-152`），避免来源编号与正文 `[S#]` 不一致。
- `RightContextPanel.tsx:172-175` 当前把 `citations.length / wikiCount` 标成 pages，语义混淆；改成“已用引用 / 检索候选 / Wiki 总页数”三项。
- provider 使用友好标签；当前右面板直接显示 `open_ai` 等 enum（`RightContextPanel.tsx:101-107`）。

### 4. 把 diagnostics 变成可读的信任面板

设计稿右侧包含原始资料、执行路径、context window、耗时与 token。当前 backend 已有 selected/omitted/expanded pages 和 task activity，可先实现：

- “本次回答使用 N 个引用，检索 M 页，扩展 K 页，省略 L 页”。
- pinned、keyword、graph neighbor、source overlap 使用低饱和标签。
- invalid citation / unverified 用 warning，不用红色大卡。
- original source lineage 若当前 DTO 不足，先显示 Wiki 页 frontmatter 的 source 摘要，再决定是否扩展 backend DTO。
- token/latency 必须来自真实 provider/task facts；没有数据时不要伪造。

### 5. 改善空态、加载态与错误态

- 当前空态只有一行文字（`ChatView.tsx:227-230`）；可加入 2-3 个与当前 Wiki 有关的低调 prompt suggestions，例如“总结当前 Wiki”“比较两个主题”“查找缺少来源的页面”，避免通用 AI 营销卡片。
- 使用 `loadingSession` skeleton，切会话时不要保留旧 transcript。
- error banner 增加 retry/打开日志动作，并区分 provider missing、Agent unavailable、cancelled、citation warning。
- overwrite、convenience soft violation 和 rollback failed 使用统一 dialog/drawer 语法，替换 `window.confirm` 与容易滚出视口的内联块。

### 6. 响应式与可访问性

- `SessionToolbar` route segment 应增加 `aria-pressed` 或 radiogroup 语义；现在只有 `.is-active` 视觉类（`ChatView.tsx:480-490`）。
- `role="log"` 对每个流式 token 使用 `aria-live="polite"` 可能造成读屏器持续播报；应把稳定消息 addition 与生成状态 live region 分开。
- 小窗口下允许折叠 session rail；当前只从 260px 变到 200px，叠加应用左栏和右面板后中心阅读宽度过窄。
- Copy Markdown 增加成功/失败反馈；`navigator.clipboard.writeText` 当前没有 catch（`RightContextPanel.tsx:96-99`）。

## 测试覆盖缺口

现有测试对 route 决策、引用解析、检索扩展、页面 session epoch、草稿保留和基本 UI 有不错覆盖，但缺少以下高风险场景：

1. 便捷 Agent 写了一部分文件后报错/取消，必须审计并恢复。
2. 回滚期间出现 Agent 之外的并发 tracked/untracked 修改，不能丢失。
3. 回滚失败不能把任务标记为 succeeded。
4. 同一 session 并发 send、send+rename、send+delete。
5. 快速 A→B 会话选择乱序返回，`activeSessionId` 必须始终与内容一致。
6. PageChat 首次创建过程中切页，而非仅 `ensurePageSession` 过程中切页。
7. 发送 IPC 返回前切项目，taskStore 仍收到任务事实。
8. 超长 `wiki/index.md` 不得挤掉 pinned page 正文。
9. Markdown code/inlineCode 中的 `[S1]` 不得被改写或计为 citation。
10. overwrite action 从右面板触发时可见、可聚焦且不能重复注册。
11. session JSON 损坏时 UI 提供可恢复告警。
12. 键盘可选择、重命名、删除任意会话，route selector 可读出当前状态。

## 建议实施批次

### Batch A：安全阻断项

- 隔离便捷写入。
- 统一失败/取消清理 guard。
- 路径级、哈希保护的 rollback。
- rollback failed 任务状态与 PendingAction。

### Batch B：状态一致性

- session mutation 串行化/revision。
- selection epoch 和 loading UI。
- PageChat first-send page guard。
- task fact 先 upsert、presentation 后 scope guard。

### Batch C：可信问答闭环

- pinned budget 预留。
- invalid/unverified diagnostics UI。
- saved answer path 持久化与打开入口。
- 统一 overwrite confirmation。
- AST 级 citation 渲染。

### Batch D：视觉完成度

- Composer、session rail、right panel 对齐设计稿。
- 自动标题、page/global session 分组。
- 空态/加载态/错误态、响应式和键盘可达性。

## 审查边界

- 本次没有调用真实 Agent CLI 或真实 BYOK provider，未验证外部模型的 token usage、SSE 差异和平台进程行为。
- 未修改业务代码；上述问题均基于当前调用链、持久化行为、测试覆盖和设计契约得出。

## 验证结果

- Chat 定向前端测试通过：`npm run test -- src/stores/chatStore.test.ts src/features/chat/chatView.test.tsx src/features/chat/PageChatPanel.test.tsx src/features/chat/ChatSessionList.test.tsx src/features/chat/ChatConveniencePanel.test.tsx`，共 `5 passed` 测试文件、`44 passed` 用例。
- `npm run check` 已从头运行两次；两次均在 frontend test 阶段被同一个非 Chat 用例阻断：`src/app/App.test.tsx > does not emit jsdom canvas getContext noise when the graph view renders`，全套并发运行时在 5–6 秒内找不到 `Graph canvas is unavailable in this environment.`。
- 该失败用例单独重跑通过：`1 passed | 28 skipped`，测试体约 739ms；因此当前证据指向全量并发负载下的既有时序/超时不稳定，而不是本次 Markdown 文档造成的回归。
- 因统一检查在 `npm run test` 即停止，不能声称 lint、build、console scan 和 Rust 阶段在本轮完整链路中通过。

## 修复跟踪（2026-07-18 当前 worktree）

综合报告前文同样是审查快照。当前实现已补齐：路径级 Chat convenience rollback、回滚前文件 hash 检查、全局 Chat send/save/delete/convenience mutation gate、取消清理后的终态事件、会话与页面 selection epoch、保存路径和引用可信度诊断。后端 `cargo check --no-default-features` 与 Rust library tests（624/624）通过；TypeScript 与 ESLint 通过。

保留的安全限制是：HardViolation 自动清理无法识别 Agent 执行期间对同一路径发生的外部编辑；这需要 Agent 写集或人工复审策略，当前不应宣称已完全解决。项目切换后后端 confirmation registry 的孤儿 action、导出知识卡片未传入回答上下文、跨组件卸载后的草稿持久化也仍属后续项。
