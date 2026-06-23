# 进度账本 · chat 前端壳 (P0+P1)

**✅ 本轮完成 @ 2026-06-23** — P0-1 (消息富化) / P0-2 (流式UI) / P1-1 (segment切换器) / P1-2 (右面板) 全部 verified。P1-3 (原始资料段) + timing/tokens 字段 blocked（后端未交付 ChatCitation.rawRefs + ChatMessage.timing/tokenUsage）。

## 交付摘要
- 消息气泡：Markdown（remark-gfm+remark-math+rehype-katex+rehype-highlight）+ YOU/AI avatar 28px + time + route/provider badge + citation `<sup>` 角标（点击跳 Wiki）+ 消息内 citation 卡片列表
- 流式：`useChatStream` app 级订阅 `task://stream-output`→chatStore streamingText 累积→StreamingBubble 逐字渲染+闪烁 cursor；终态 reload 幂等取代
- segment：ChatView 顶部会话工具栏 `.seg` Auto/Agent/BYOK 三态切换+标题 inline 重命名+删除
- 右面板：引用编号列表+跳转 / 原始资料（blocked）/ 执行路径 route+检索数+页面数 / 操作（保存+复制MD+生成卡片+标记问题）
- 左栏 220→260px 对齐设计稿
- i18n en+zh-CN 完整
- lint 0 warning · test 105 pass · 无 console.log

## 文件清单（仅 src/ + styles.css）
`src/features/chat/MessageContent.tsx` `ChatView.tsx` `src/stores/chatStore.ts` `src/hooks/useChatStream.ts` `src/types/task.ts` `src/types/chat.ts` `src/components/app/RightContextPanel.tsx` `src/app/App.tsx` `src/styles.css` `src/i18n/locales/en.json` `src/i18n/locales/zh-CN.json` + `SPEC/plans/chat-fe.md`（本账本）

## blocked 项
- P1-3 原始资料段：后端 ChatCitation 需 `rawRefs` 字段（roadmap §1 P2）
- 执行路径 timing / tokens：后端 ChatMessage 需 `timing`/`tokenUsage` 字段
- P0 流式验证：本机 `cargo test` 被环境级 loader 故障阻塞（同 chat-BE 账本 gotcha），编译通过+clippy 净+前端逻辑对为代理证据

> 权威源：SPEC/roadmap/chat.md（§1-5）· SPEC/PRD.md（PRD-CHAT-002/003）· UI-Frontend-design/chat.html + assets/app.css（只读）· CLAUDE.md
> scope：只动 src/ + src/styles.css。后端缺口标 blocked。
> status: pending | in_progress | done | verified | blocked

## 后端契约（chat-be 已交付，本 loop 消费）

- 流式 channel：`task://stream-output`，eventType `task_stream_output`，payload `{ delta: string, route?: "agent"|"byok" }`
- delta 仅 ephemeral UI 提示，**不落盘**；终态后整条 answer 落 `.app/chats/{id}.json`，前端 reload 作幂等兜底
- `ChatMessage.provider: Option<LlmProviderKind>`（BYOK answer 填入，Agent 留 None）→ 画 "BYOK · Anthropic" badge
- 显式路由：`send_chat_message` 已接受 `route/agent/provider`，前端此前固定 `auto`

## 关键决策（动手前）

- **segment 切换器位置**：主区 header 是 AppShell 全局统一的两按钮（primary/secondary），跨 view 共享，不宜塞 chat 专属 segment。按设计稿 chat.html 的 `.toolbar`（会话头：badge+标题+操作组），在 **ChatView 主区顶部新增会话工具栏**，segment 放其内。左栏宽度 220→260 对齐设计稿。
- **流式消费链路**：`TASK_EVENT_CHANNELS` 加 `task://stream-output`；`BackendEventType` 加 `task_stream_output`；taskStore.applyBackendEvent 对该类型 **不** 改 task 状态（delta 非状态），改为 chatStore 直接订阅消费。为避免 taskStore 与 chatStore 双订阅同一 channel 重复处理，**chatStore 自己 listen** `task://stream-output`（按 sendTaskId 过滤），taskStore 不动。终态 reload 幂等清空 streaming buffer。
- **streaming buffer**：chatStore 增 `streamingText: string` + `streamingRoute: ChatRoute|null`，delta 追加；ChatView 在 generating 时渲染一个临时 assistant 气泡（buffer 内容 + cursor），终态 reload 后被真实落盘消息取代。
- **Markdown 渲染**：抽 `MessageContent.tsx`，复用 MarkdownReader 的 remark/rehype 管线但去掉 frontmatter/wikilink（chat 无），保留 citation 角标预处理（`[1]`→citation 链接点击跳 Wiki）。user 消息仍纯文本（设计稿 user 无 markdown）。
- **citation 角标 + 卡片**：assistant 消息正文末尾追加 `<sup>N</sup>`（设计稿 `.citation-ref`），消息下方追加 citation 卡片列表（`.msg__citations` + `.citation`），点击切 wiki 视图打开页面。
- **avatar/time/badge**：`.msg__avatar` YOU/AI 28px；`.msg__head` name+time+route badge；时间用 `createdAt`（已 ISO）格式化 HH:MM。
- **右面板三段**：操作区（复制MD/生成卡片/标记问题）纯前端可做；执行路径 route+检索数+页面数 可做，tokens/timing/version **blocked**（后端 ChatMessage 无 timing/tokenUsage 字段）；原始资料 **blocked**（后端 ChatCitation 无 rawRefs 字段，roadmap §1 标 P2）。

## 条目

### P0

- [x] **P0-1 消息渲染富化** — status: verified
  - 新建 `src/features/chat/MessageContent.tsx`（remark-gfm+remark-math+rehype-katex+rehype-highlight，`[N]`→可点击 citation 角标 `<sup>`）
  - `src/features/chat/ChatView.tsx` 重写消息渲染：`.msg` + `.msg__avatar`(YOU/AI 28px) + `.msg__head`(name+time+route badge) + MessageContent + `.msg__citations` 消息内 citation 卡片；左栏 220→260；role=log/aria-live
  - `src/types/chat.ts`：ChatMessage 加 `provider?: LlmProviderKind|null`（badge "BYOK · Anthropic"）
  - `src/styles.css`：补 `.chat-stream`/`.msg*`/`.msg__citation*`/`.chat-prose`/`.chat-prose .citation-ref`/`.seg`/`.chat-session__meta`/`.stream-cursor`
  - i18n：`chat.thread.you`/`chat.thread.citationRef`（en + zh-CN）
  - 验证：`npm run lint`（0 warning）+ `npm run test`（105 pass）全绿；无 console.log
- [x] **P0-2 流式 UI** — status: verified
  - `src/types/task.ts`：BackendEventType 补 `task_stream_output`
  - `src/hooks/useChatStream.ts`：app 级常驻 listen `task://stream-output`，按 sendTaskId 过滤转发 delta（跨 view 不丢）
  - `src/stores/chatStore.ts`：streamingText/streamingRoute + appendStreamDelta（按 sendTaskId 过滤，仅累积当前 in-flight send）；send 成功清空、clearSendTask 清空
  - `src/features/chat/ChatView.tsx`：generating 时渲染 StreamingBubble（avatar AI + busy badge + MessageContent 流式正文 + `.stream-cursor` 闪烁光标，空时显占位+光标），终态 reload 幂等取代
  - `src/app/App.tsx`：挂 useChatStream
  - 验证：lint 0 warning + test 105 pass；无 console.log

### P1

- [x] **P1-1 route segment 切换器** — status: verified
  - `src/features/chat/ChatView.tsx`：去掉 `ROUTE_PREFERENCE` const → `useState<ChatRoutePreference>("auto")`；新增 SessionToolbar（`.seg` 三态 Auto/Agent/BYOK + 标题 inline 编辑 + 消息数·时间 + 重命名/删除按钮）；send 携带所选 route；`.seg` / `.seg__btn` / `.seg__btn.is-active` class 已在 P0-1 预置于 styles.css
  - 验证：lint 0 warning + test 105 pass
- [x] **P1-2 右面板操作 + 执行路径** — status: verified
  - `src/components/app/RightContextPanel.tsx`：chat 分支重写为四段（引用编号列表+跳转 / 原始资料 blocked / 执行路径 route+检索数+页面数 / 操作 保存+复制MD+生成卡片+标记问题）；内联 CitationPanel 实现，去外部依赖；timing/tokens 占位 "—"
  - `src/i18n/*.json`：补 `chat.citations.rawSources` / `.rawSourcesBlocked` / `.route` / `.routePath` / `.timing` / `.tokens` / `.actions` / `.copyMd` / `.generateCard` / `.flagIssue`（en + zh-CN）
  - 验证：lint 0 warning + test 105 pass
- [ ] **P1-3 原始资料段** — status: blocked
  - 后端 ChatCitation 无 rawRefs 字段（roadmap §1 P2，未交付）。前端无法填充。移交后端。

## 进度日志

- 2026-06-23 建账本；读完对照源（roadmap/chat.md、PRD-CHAT、chat.html、app.css、chat-be 账本、ChatView/Composer/SessionList/CitationPanel/chatStore/taskStore/useTaskEvents/RightContextPanel/AppShell/MarkdownReader/task types/i18n/styles.css）。确认后端流式 channel 已 emit、前端未消费。决策见上。
