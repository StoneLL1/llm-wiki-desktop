# Chat 前端代码审查

日期：2026-07-18  
目标 worktree：`.worktrees/import-v2-final-integration`  
分支：`codex/import-v2-final-integration`  
基线 HEAD：`f2375ae68346e9994bf2da5d1ebdb3be36d0a9ac`  
审查对象：当前 worktree 的未提交集成态，而非只审查 HEAD。

## 1. 范围与依据

本次只审查 Chat 前端，不修改业务代码。主要覆盖：

- `src/features/chat/ChatView.tsx`
- `src/features/chat/PageChatPanel.tsx`
- `src/features/chat/ChatComposer.tsx`
- `src/features/chat/ChatSessionList.tsx`
- `src/features/chat/MessageContent.tsx`
- `src/features/chat/ChatConveniencePanel.tsx`
- `src/stores/chatStore.ts`
- `src/hooks/useChatStream.ts`
- `src/components/app/RightContextPanel.tsx`
- `src/styles.css`
- Chat 前端测试与中英文 i18n

产品和视觉契约来自：

- `SPEC/SPEC.md` 16.2、16.3、16.6
- `SPEC/APP_flow.md` 11
- `SPEC/FRONTEND_GUIDELINES.md`
- `SPEC/DESIGN.md`
- `UI-Frontend-design/chat.html`
- `UI-Frontend-design/assets/app.css`

## 2. 总结

Chat 前端已经具备会话列表、全局问答、页面级 Ask AI、Agent/BYOK 路由、流式输出、任务活动、引用跳转、保存回答和便捷写入审计等主体能力，但当前状态管理仍按“同一时刻只有一个 Chat 请求”设计，而界面允许用户切换会话、切换页面后继续发送。这是本轮最主要的结构性风险。

本轮前端专项审查未新增单独的 P0；共记录 **9 个 P1、11 个 P2**。P1 主要集中在会话/任务作用域、异步竞态和可信信息展示，应该先于视觉重构处理。

## 3. P1：高优先级问题

### P1-1：切换会话后可以再次发送，但 store 只有一个全局发送任务槽

证据：

- `src/stores/chatStore.ts:64-81` 只有一组 `sendTaskId`、`sendSessionId`、`streamingText`、`streamingRoute`。
- `src/features/chat/ChatView.tsx:94-98` 仅当 `sendSessionId === activeSessionId` 时才认为当前界面正在生成。
- `src/stores/chatStore.ts:375-384` 新一次发送会直接覆盖上一组发送任务绑定。

复现路径：

1. 在会话 A 发送问题。
2. 任务未完成时切到会话 B。
3. B 的 `generating` 为 false，Composer 重新可用。
4. 在 B 再次发送，store 用 B 的任务覆盖 A 的任务。

影响：

- A 的流式文本、取消入口和终态 reload 失去 UI owner。
- A 后端任务仍可能正常完成，但会话列表和消息内容不会及时刷新。
- 用户回到 A 时无法看到“仍在运行”的准确状态。
- `pendingStreamDeltas` 会开始承担本不该由临时缓冲承担的长期状态。

建议：将发送状态改为 `Record<taskId, ChatRun>`，至少保存 `projectKey/sessionId/taskId/status/streamText/route`；界面按当前 session 查找对应 run。若产品决定只允许一个 Chat 任务，则必须在所有 session/page composer 上明确禁用，并提供“查看正在运行的会话”入口。

### P1-2：发送成功后，刚刚提交的用户问题会从界面暂时消失

证据：

- `src/features/chat/ChatComposer.tsx:44-53` 后端返回 task id 后立即清空草稿。
- `src/stores/chatStore.ts:354-393` `send` 只保存 task 信息，不把用户消息合并到 `activeSession`。
- `src/features/chat/ChatView.tsx:121-136` 直到任务进入终态后才 reload session。

影响：用户点击发送后看到输入框被清空和 assistant streaming bubble，却看不到自己刚才问了什么；长任务期间尤其容易怀疑发送了错误内容，也无法核对问题原文。

建议：后端 `send_chat_message` 返回已持久化的 user message 或更新后的 session revision；前端以服务端事实进行乐观合并。不要只在任务结束时刷新整段会话。

### P1-3：普通会话选择、创建和终态 reload 缺少 selection epoch，旧请求可覆盖新选择

证据：

- `src/stores/chatStore.ts:300-313` `selectSession` 先同步写 `activeSessionId`，await 后无 selection epoch，直接写 `activeSession`。
- `src/stores/chatStore.ts:208-224` `createSession` 依次调用 `loadSessions`、`selectSession`，没有创建请求代次。
- `src/stores/chatStore.ts:409-415` 终态 reload 在进入 `selectSession` 前只检查一次 session id；进入 await 后仍可能被用户的新选择超越。
- `loadingSession` 已定义，但 `ChatView` 没有读取它。

影响：快速点击 A、B 时可能得到 `activeSessionId=B`、`activeSession=A`；双击新建也可能创建多条会话并最终选中较早返回的那条。消息、标题、保存按钮和右侧引用会指向不同会话。

建议：增加独立 `selectionEpoch`；每次 select/create/reload 捕获 epoch，并在每个 await 后验证。加载新会话时不要继续显示旧 transcript，应显示紧凑 skeleton 或保留旧内容但明确禁用操作且标注 loading。

### P1-4：页面级 Chat 的第一次发送绕过了已实现的 page epoch

证据：

- `src/stores/chatStore.ts:232-297` `ensurePageSession` 已实现 `pageSessionEpoch`。
- `src/features/chat/PageChatPanel.tsx:110-125` 首次发送无 session 时却调用普通 `createSession`。

复现路径：在页面 A 第一次发送，创建会话的 IPC 未返回前切到页面 B。A 的 `createSession` 仍可能执行 `loadSessions + selectSession`，把 A 的 session 写回共享 store。

影响：虽然 `activeSessionMatchesPage` 会暂时隐藏错误 session，但共享 store 已被旧页面污染；后续列表、保存、终态 reload 和全局 Chat 都可能继承错误选择。

建议：首发也通过 page-scoped create API，携带捕获的 `pagePath + epoch`；创建成功后只在页面仍匹配时提交展示状态。不要用 `useChatStore.getState().activeSessionId` 作为异步创建后的兜底。

### P1-5：保存冲突的确认状态没有绑定 session，页面侧 Chat 甚至没有确认入口

证据：

- `src/types/chat.ts:135-141` `ChatOverwriteRequest` 只有 `messageId/path/hash/actionId`，没有 `sessionId`。
- `src/stores/chatStore.ts:465-481` 确认覆盖时使用“当前” `activeSessionId`，而不是首次保存发生冲突时的 session。
- `src/features/chat/ChatView.tsx:273-296` 覆盖确认只渲染在全局 Chat transcript 底部。
- `src/features/chat/PageChatPanel.tsx:149-260` 完全没有渲染 `overwriteRequest`。

影响：

- 用户在会话 A 触发冲突后切到 B，再确认时会把 A 的 messageId 与 B 的 sessionId 组合提交。
- 页面 Ask AI 保存遇到已存在文件后，只会得到 `exists` 状态，没有可见的继续/取消入口。
- 内联确认位于长 transcript 底部，用户向上阅读时也很难发现。

建议：`overwriteRequest` 必须携带 project/session/message 身份；统一交给全局 `ProjectConfirmationController` 或 Chat 专用 modal/drawer，并在全局 Chat 与 PageChat 共用同一确认面。切换作用域后仍应显示请求来源，不能自动重绑定到当前 session。

### P1-6：项目切换发生在发送 IPC 返回前时，合法后台 task 不会进入 taskStore

证据：`src/stores/chatStore.ts:359-386` 在 `isProjectScopeCurrent(scope)` 之后才执行 `useTaskStore.getState().upsertTask(task)`。

这违反了项目的“task facts 是 scope guard 例外”契约。任务属于后台历史事实，即使当前项目已切换也必须进入全局 taskStore；只有 drawer、toast、当前视图和导航提交需要被旧项目 guard 抑制。

建议：IPC 返回的有效 `BackendTask` 先无条件 upsert；随后再做 project scope 判断，决定是否绑定到当前 Chat 展示状态。

### P1-7：Composer 草稿没有 session/page 身份，切换页面会把 A 页问题带到 B 页

证据：

- `src/features/chat/ChatComposer.tsx:41` 草稿只存在组件本地的单个 `value`。
- `src/features/chat/PageChatPanel.tsx:105-108` 页面变化复用同一 PageChatPanel/Composer 实例。
- `src/features/chat/ChatView.tsx:310-316` 切换普通 session 同样不会重建或切换草稿。

影响：用户在页面 A 输入未发送问题，切到页面 B 后草稿仍在；此时发送会使用 B 的 `pinnedPagePath`。普通会话间也会共享一份草稿，容易把问题发到错误会话。

建议：草稿按 `projectKey + surface(global/page) + sessionId/pagePath` 持久化或至少内存分桶。页面切换后显示对应草稿；没有草稿时清空，并保留返回原页面后的内容。

### P1-8：生成新回答时，右侧面板仍展示上一条回答的引用和操作

证据：

- `src/components/app/RightContextPanel.tsx:79-107` 始终从 `latestAssistantMessage(chatSession)` 读取引用、route 和保存对象。
- 流式回答在任务完成前没有进入 `activeSession`。

影响：中心区域正在生成回答 N，右侧却仍显示回答 N-1 的来源，并允许复制、保存、生成卡片。对依赖引用可信度的知识库产品而言，这会让用户误以为旧引用支持新答案。

建议：右面板绑定明确的 `focusedMessageId` 或当前 `ChatRun`。生成时显示“正在检索/引用待确认”，禁用或明确标注旧回答操作；完成后再原子切换到新 message facts。

### P1-9：未绑定到当前 sendTask 的流式事件会被长期缓冲，缺少终态清理

证据：

- `src/hooks/useChatStream.ts:30-40` 接收所有 Chat route 的全局流事件。
- `src/stores/chatStore.ts:547-568` 只要 task id 不是当前 `sendTaskId`，就把最多 256 KiB 文本写入 `pendingStreamDeltas[taskId]`。
- 只有某个 task 后来恰好成为当前 send response 时，或当前 send clear 时，才会删除对应键；未知/旧项目/被第二次发送覆盖的任务没有终态清理。

影响：长时间使用、多会话并发或项目切换时可能积累大量永久缓冲；旧任务流也没有可见 owner。

建议：监听 task lifecycle，用已登记 ChatRun 决定是否接收 delta；未知 task 只允许一个很短的 race buffer，并按时间/数量淘汰。终态事件必须清理对应流缓存。

## 4. P2：中优先级问题与功能缺口

### P2-1：后端已有检索诊断，但 UI 完全未消费

`src/types/chat.ts:44-55` 已定义 `selectedPages`、`omittedPages`、`expandedPages`、`invalidCitationIds`、`hasUnverified` 等字段，但源码搜索显示没有任何 Chat 组件读取 `retrievalDiagnostics`。

缺失的关键反馈包括：

- 本次检索了多少页面、为什么选择这些页面。
- pinned page 是否真正进入上下文。
- 哪些模型引用无效。
- 回答是否包含 `[unverified]`。
- 哪些候选因预算被省略。

建议在右面板增加紧凑的“上下文与可信度”区；无效引用和 unverified 使用 warning，而不是隐藏。

### P2-2：引用正则预处理会改写代码内容，流式阶段还会生成不可用的 `citation://` 链接

证据：

- `src/features/chat/MessageContent.tsx:15-28` 在 Markdown parse 前对整段字符串做正则替换，因此 fenced code、inline code 中的 `[S1]` 也会被改写。
- `src/features/chat/ChatView.tsx:576-579` 流式内容以 `citationCount={0}` 渲染；预处理后的 `[S1]` 因未被认定为有效引用，最终退化为普通 `<a href="citation://S1">`。

建议改为 remark AST plugin，只转换 paragraph/list/table 等文本节点，跳过 code、inlineCode、link 和 HTML。流式阶段只显示不可点击的 citation token，拿到持久化 citation facts 后再启用跳转。

### P2-3：`loadingSession` 已存在但未渲染，加载时继续显示旧消息

`src/stores/chatStore.ts:73-74` 和 `300-313` 维护 `loadingSession`，但 `ChatView`/`PageChatPanel` 都没有订阅。用户点击新会话时，选中高亮已经变化，中心区域仍保留旧 transcript，进一步放大 P1-3 的错配感。

建议增加 2-3 行安静 skeleton；加载期间禁用保存、删除、引用跳转等依赖完整 session 的操作。

### P2-4：保存成功结果路径被丢弃，用户看不到写到了哪里

`src/stores/chatStore.ts:427-440` 收到 `SaveAnswerResult.path/checkpoint` 后只把状态设为 `saved`；`src/features/chat/ChatView.tsx:639-652` 只显示“已保存”。这与设计规范“Saved answers should clearly indicate the target `wiki/queries/` path”不一致。

建议持久化或至少保留 per-message `SaveAnswerResult`，展示 basename + 完整路径 tooltip，并提供“打开页面”动作；checkpoint 可在详情中显示。

### P2-5：会话列表的行操作对键盘用户不可达

`src/features/chat/ChatSessionList.tsx:128-145` 的重命名/删除按钮默认 `display:none`，只在 `group-hover` 时变为 flex。隐藏元素无法获得键盘焦点，所以仅用 Tab 无法发现这些操作。

建议至少加入 `group-focus-within:flex`；更好的方案是语义化 listbox/roving focus，并通过上下文菜单或始终可达的 More 按钮承载行操作。

### P2-6：route segmented control 只有视觉选中态，没有选择语义

`src/features/chat/ChatView.tsx:480-490` 未提供 `aria-pressed`、radiogroup/radio 或选中状态文本。屏幕阅读器无法获知当前 Auto/Agent/BYOK 路由。

建议使用 `role="radiogroup" + role="radio" + aria-checked`，或给每个按钮加 `aria-pressed`。

### P2-7：整个流式 transcript 使用 `aria-live="polite"`，可能逐 token 播报

`ChatView.tsx:218-225` 与 `PageChatPanel.tsx:193-200` 将持续变化的 message log 整体设为 live region。流式 token、activity timeline 和新消息都可能触发重复播报。

建议 transcript 保持 `role="log"`，另设一个很小的状态 live region，只播报“正在生成、已完成、失败、已取消”；完整回答在完成后作为一次 addition 暴露。

### P2-8：取消操作没有错误处理，失败会产生未处理 Promise

`ChatView.tsx:154-159` 与 `PageChatPanel.tsx:134-139` 的 `invoke("cancel_task")` 只有 `.then`，没有 `.catch`。取消失败时用户没有反馈，控制台可能出现 unhandled rejection。

建议把取消纳入 typed task action，显示 cancelling 状态，并将错误写入 task/Chat 的分类错误区。

### P2-9：右栏引用编号、Provider 标签和页面计数语义不准确

证据：

- `RightContextPanel.tsx:130-142` 使用 `index + 1`，而正文和消息卡使用 `citation.sourceId`，可能出现正文 `[S4]`、右栏却显示 `1`。
- `RightContextPanel.tsx:101-106` 直接显示 `open_ai` 等 enum。
- `RightContextPanel.tsx:172-175` 把 `citations.length / wikiCount` 放在 “Pages” 下，实际混合了“已引用来源数”和“Wiki 总页面数”。

建议统一显示真实 source id；Provider 通过友好 label/i18n；把统计拆成“引用 N / 检索 M / Wiki K”。

### P2-10：复制与“生成卡片”操作缺少可信反馈或真实上下文传递

- `RightContextPanel.tsx:96-99` clipboard 没有 catch、toast 或成功状态。
- `RightContextPanel.tsx:201-207` “生成知识卡片”只切换到 Exports，没有把当前回答、保存页或 message id 传过去。

这会让按钮看起来已经完成了动作，实际只是导航。建议复制显示成功/失败反馈；生成卡片要么创建带来源身份的 export preset，要么改文案为“前往导出”。

### P2-11：Chat CSS 存在重复 `.seg` 规则，后写样式不一定生效

证据：

- `src/styles.css:970-988` 定义第一套 `.seg`、`.seg button`、`.seg button.is-active`。
- `src/styles.css:2171-2197` 又定义 `.seg`、`.seg__btn`、`.seg__btn.is-active`。

由于 `.seg button.is-active` 的 specificity 高于 `.seg__btn.is-active`，后者设置的 accent active background 会被前者的 background/shadow 压住；容器高度、padding、border 也来自两套规则叠加。这是一个实际的 cascade bug，而不只是重复代码。

建议合并成唯一 `.seg/.seg__btn` 契约，并为选中态、focus-visible、不同主题补 CSS contract test。

## 5. 前端视觉与交互美化建议

以下建议不应先于 P1，但完成后会明显更接近 `UI-Frontend-design/chat.html` 和 Codex desktop 的工作台质感。

### 5.1 Composer：从“两排散件”改为统一输入卡

当前：`ChatComposer.tsx:58-101` 把 route badge/取消放在上排，textarea/发送放在下排；输入框固定高度、不能自动增长。

建议：

- 使用一个 `focus-within` composer card，textarea 无内边框，底部 action rail 放路由、上下文、停止、发送。
- textarea 自动增长到 5-6 行，再内部滚动。
- 全局 Chat 增加“引用页面”“附加资料”入口，或在后端能力未实现前显示 disabled + tooltip，避免设计能力凭空消失。
- 显示真实上下文摘要，如“Wiki 237 页 · 已固定 1 页”；没有数据时不伪造 token window。
- 明确键盘规则。设计稿是 `⌘/Ctrl + Enter` 发送；当前实现是 Enter 发送、Shift+Enter 换行，应统一产品决定并在 placeholder/tooltip 中说明。

### 5.2 会话栏：增加时间层级、作用域和持久化心智

当前所有日期都只显示 `HH:MM`，几天前的会话也如此；global/page-scoped session 外观完全相同。

建议：

- 按“今天 / 昨天 / 更早”分组，或至少使用“昨天、06-17、14:32”等相对日期。
- page-scoped session 增加页面图标、页面标题/路径 tooltip，并提供 global/page 筛选。
- 生成中、失败、待确认分别显示小型状态点。
- 底部补设计稿中的 `.app/chats/`、`wiki/queries/` 持久化说明。
- 新会话首问成功后用本地截断规则自动命名；页面会话标题不要硬编码 `Ask:` 英文（`chatStore.ts:284`、`PageChatPanel.tsx:117`）。

### 5.3 对话流：减少重复卡片，稳定生成前后形态

当前 persisted assistant 使用 `msg/avatar` 布局，streaming assistant 使用另一套 `chat-agent-header/Sparkles` 布局，任务完成时视觉形态跳变；同时消息下方完整 citation cards 与右栏重复。

建议：

- streaming 和 persisted answer 使用同一 skeleton/header 结构，只改变状态点与 cursor。
- transcript 内只保留正文 source marker 和一行紧凑来源摘要；完整 snippet、路径、检索原因放右栏。
- 用户消息、assistant 回答、activity timeline 的垂直间距统一为 16/20px，避免 activity 使每条回答像嵌套卡片。
- 代码块增加 copy action；长表格提供横向滚动提示。

### 5.4 右侧面板：从“静态引用列表”升级为回答 inspector

建议结构：

1. 当前回答身份：时间、Agent/BYOK、Provider、状态。
2. 可信度：有效引用、无效引用、unverified warning。
3. 上下文：pinned/keyword/graph/source-overlap 的 selected/omitted pages。
4. 来源：真实 `[S#]` 编号、snippet、路径、current-page badge。
5. 结果动作：保存路径、复制、打开保存页、导出。

这样右栏才是回答的事实面板，而不是简单重复消息下方的 citation cards。

### 5.5 空态、加载态和错误态

- 空态不只显示“选择或创建会话”，可提供 2-3 个与当前 Wiki 相关的低调 prompt suggestion。
- 会话加载使用紧凑 skeleton，不保留旧 transcript 冒充新会话。
- error banner 增加关闭、重试、打开任务日志；区分 provider 未配置、Agent 不可用、取消、检索警告和保存冲突。
- PageChat 空态应说明“首次发送才创建页面会话”，并显示当前固定页面。

### 5.6 响应式布局

`src/styles.css:2587` 默认保留 220-260px session rail；窄屏规则 `src/styles.css:3062` 只缩到 200px，没有折叠能力。在应用左侧导航仍存在、右面板变成 drawer 的情况下，中心阅读宽度仍容易过窄。

建议在窗口较窄时把 session rail 折叠为 drawer/overlay，并保留一个带当前会话标题的切换按钮；不要继续压缩正文到不可读宽度。

## 6. 测试覆盖缺口

现有 44 个 Chat 定向前端测试覆盖了基本渲染、页面 pinned context、page ensure epoch、引用跳转、保存冲突数据和便捷写入参数，但缺少以下关键场景：

1. 会话 A 生成中切到 B，再从 B 发送，两个 task 都必须可追踪。
2. 发送被接受后，用户消息立即可见且不会重复。
3. A/B 快速 select 的响应乱序，session id 与内容必须一致。
4. 连续点击“新会话”的乱序响应。
5. terminal reload 期间切换会话，旧 reload 不得覆盖新选择。
6. Page A 首次创建期间切换到 Page B。
7. Page A 草稿切到 Page B 后不能变成 B 的问题。
8. PageChat 保存冲突可以确认/取消，且 request 使用原 session。
9. 项目切换后，返回的 task 仍进入 taskStore，但不接管当前视图。
10. 流式新回答期间，右栏不展示旧引用为当前来源。
11. unknown/old task 的 stream buffer 在超时或终态后清理。
12. code fence/inline code 内的 `[S1]` 不被改写。
13. streaming `[S1]` 不产生可导航的 `citation://` 外链。
14. 键盘可重命名、删除 session，route selector 可读出当前状态。
15. clipboard 失败与取消任务失败有可见反馈。
16. CSS 只存在一套 `.seg` owner，并在各主题下保持选中态。

## 7. 建议实施顺序

### Batch A：作用域和任务模型

- 把单个 `sendTaskId` 改为 session/task 维度的 ChatRun。
- 修复 task facts upsert 顺序。
- 为 select/create/reload 增加 selection epoch。
- 让 PageChat 首次创建进入 page epoch。
- 草稿按 session/page 分桶。

### Batch B：可信操作闭环

- 用户消息即时可见。
- overwrite request 绑定 project/session/message，并统一确认 UI。
- 右栏绑定 focused/current answer，而不是无条件 latest persisted assistant。
- 显示 retrieval diagnostics、invalid citations、unverified 和保存路径。

### Batch C：渲染与可访问性

- AST 级 citation plugin。
- loading/error/cancel/clipboard 反馈。
- session row、route selector、live region 的键盘与读屏修复。
- 清理未知 stream buffer。

### Batch D：视觉完成度

- Composer card、自动增高和 context actions。
- 会话分组、作用域 badge、状态点和持久化 footer。
- 对话流/右栏去重与统一形态。
- 折叠 session rail、响应式和 `.seg` CSS 收敛。

## 8. 审查边界

- 本次没有运行真实 Agent CLI 或 BYOK provider，未验证外部模型流事件的真实频率和 provider-specific payload。
- 本次没有修改 Chat 业务代码，也没有改动 `UI-Frontend-design/`。
- 后端回滚安全、会话文件并发写等问题已在综合报告 `docs/reviews/2026-07-18-chat-code-review.md` 中记录，本报告只保留与前端状态和交互直接相关的部分。

## 9. 验证结果

- Chat 定向前端测试通过：`5 passed` 测试文件、`44 passed` 用例。
- 完整 `npm run check` 通过：前端 `97 passed` 测试文件、`612 passed` 用例，ESLint、TypeScript/Vite production build、console-log scan、Tauri GUI `cargo check`、Rust no-default-features 单元/集成/doc tests 均成功。
- Rust 编译仍报告 Import V2 `FileTransaction::write/track/capture_installed` 的既有 dead-code warning；它不属于 Chat 前端审查范围，也没有阻断检查。

## 10. 修复跟踪（2026-07-18）

本报告前文保留的是审查时点快照；以下项目已在当前 worktree 修复并重新核对：

- Chat 发送、保存、便捷编辑决策、删除会话统一使用全局互斥；保存按钮与 Composer 会在其他 Chat mutation 进行时禁用。
- overwrite 确认改为单一中心入口，支持跨会话/无 transcript 时仍可确认或取消；跨项目异步响应不会恢复旧请求。
- Composer 草稿按 project/session/page 分桶，并以 revision 与 scope lineage 清理，避免异步提交覆盖新编辑。
- PageChat session、Windows 路径大小写、selection epoch、terminal reload 错误和便捷编辑响应均加入作用域保护；新会话按钮加入创建中禁用态。
- 便捷编辑回滚改为受影响路径级操作，并保存 Agent 完成后的文件 hash；回滚前路径发生外部变化时拒绝覆盖并要求复核。

仍待后续产品/架构决策的项目：便捷写入在 Agent 执行期间与用户同路径并发编辑的精确写集区分、跨项目后端 pending confirmation 的生命周期、导出知识卡片按钮的真实导出上下文，以及跨页面卸载后的草稿持久化。完整 `npm run check` 在本机被 Tailwind native binding 加载失败和 Vite `spawn EPERM` 阻断，不能据此声称 Vitest 全套通过。
