# Chat 板块落差与实施计划

> 对照源：UI-Frontend-design/chat.html + assets/app.css + SPEC/PRD.md（§8.6 / §9.6 / Phase 3）
> 当前实现：src/features/chat/、src/stores/chatStore.ts、src/types/{chat,llm}.ts、src-tauri/src/services/chat_service.rs、src-tauri/src/commands/chat_commands.rs

## 0. 现状摘要

Chat 板块已具备端到端可用的最小闭环，不是空 UI 壳：

- **会话管理**：CRUD 完整，持久化到 `.app/chats/{id}.json`，支持空标题校验、损坏文件隔离、按 updatedAt 倒序。
- **真接入模型**（非 mock）：`send_chat_message` 创建可取消后台 Task → `ChatService::build_retrieval_context` 用 SearchService 取本地 Top-6 页面+excerpts+purpose.md+最近 8 轮历史组装 prompt → `resolve_route` 按 auto/agent/byok 路由到 AgentService 或 LlmService.complete（OpenAI/Anthropic/Google/Ollama/Custom 全打通） → 落盘 assistant 消息+citations。
- **引用**：citations 由本地检索结果直接生成（不从模型输出解析），CitationPanel 接入 RightContextPanel，可跳转 Wiki。
- **保存到 wiki/queries/**：含 Git checkpoint + hash 匹配 + 二次确认对话框，路径强制 wiki/ 子树。
- **取消**：chat_commands.rs:200-210 100ms 轮询 `is_cancelled`；前端 ChatView 监听终态自动 reload。

但与设计稿相比明显缺位：**流式输出 / 多模型 segment 切换 / 模型 badge / 附加资料与引用页面按钮 / Skill 选择器（`/`） / 消息内嵌引用编号 / 右侧面板原始资料+执行路径+操作区 / Markdown 渲染（现仅 whitespace-pre-wrap 纯文本）/ 保存按钮反馈 "已保存到 wiki/queries/" 的路径回显**。

PRD P0/P1 的 5 条中：CHAT-001~004 已达标，CHAT-005 由"全局搜索不调模型"的边界规则保证，无显式测试。

## 1. 区块 / 组件清单

| 区块/组件 | 设计稿要求 | 当前实现 | 状态 | 优先级 | 涉及文件 |
|---|---|---|---|---|---|
| 顶栏 `seg` 模型切换（claude / BYOK · Anthropic） | 双 tab 切换显示活跃路由 | `ChatView.tsx:13` 硬编码 `ROUTE_PREFERENCE="auto"`，无 UI 切换器 | ❌缺失 | P1 | src/features/chat/ChatView.tsx:13、ChatComposer.tsx:42 |
| 顶栏 "历史" 按钮 | 打开历史抽屉 | 无 | ❌缺失 | P2 | src/features/chat/ChatView.tsx |
| 顶栏 "新会话" 按钮 | 主操作入口 | 仅左侧列表头有 "+新建"，主区无 | 🟡部分实现 | P2 | src/features/chat/ChatView.tsx:79 |
| 左侧会话列表（搜索框 + meta "时间·N 条消息"） | 搜索 + 每行 meta | 无会话内搜索；meta 仅靠 title，无"时间·条数"副行 | 🟡部分实现 | P1 | src/features/chat/ChatSessionList.tsx:43-119 |
| 列表底部提示 "会话保存在 .app/chats/ · 可导出" | 信息条 | 无 | ❌缺失 | P2 | src/features/chat/ChatSessionList.tsx |
| 会话工具栏（badge 模型 + 标题 + 消息数·时间 + 保存/重命名/删除） | 完整 toolbar | 无独立会话头 toolbar，重命名/删除藏在列表行 hover | 🟡部分实现 | P1 | src/features/chat/ChatView.tsx:99-183 |
| 消息气泡（avatar YOU/AI + 头部 name/time + ingest badge + Markdown 正文 + 内嵌 `<sup>` 引用编号） | 富文本 + avatar + badge | 无 avatar、无时间戳、无 ingest badge、纯文本 `whitespace-pre-wrap`、无内嵌引用角标 | 🟡部分实现 | P0 | src/features/chat/ChatView.tsx:106-144 |
| 消息内嵌 citation 角标 `<sup>1</sup>` | 点击跳转 | citations 仅在右面板展示，正文内无角标 | ❌缺失 | P1 | src/features/chat/ChatView.tsx:118 |
| 消息底部 citations 卡片（编号 + 标题 + 路径） | 每条 AI 消息下 | 无（仅右面板） | ❌缺失 | P1 | src/features/chat/ChatView.tsx:120-142 |
| 生成中 badge（"生成中…" + 跳动点） | 气泡头部 + 状态栏 | 仅 ChatView.tsx:146 一行纯文字 "Generating answer…" | 🟡部分实现 | P1 | src/features/chat/ChatView.tsx:145-157 |
| 流式输出（逐字呈现） | 设计稿留有 terminal cursor | 非流式，BYOK 走 `LlmService::complete` 一次性 POST；Task 终态后整条 reload | ❌缺失 | P1 | src-tauri/src/services/llm_service.rs:141-195、chat_commands.rs:182-214 |
| Composer（textarea + 附加资料 + 引用页面 + hint + 停止 + 发送） | 完整底栏 | 仅 textarea+发送；取消按钮在外层而非 composer 内；无附加资料/引用页面/hint/⌘↵ | 🟡部分实现 | P1 | src/features/chat/ChatComposer.tsx:44-86 |
| Composer `⌘↵ 发送 · / 选择 Skill` 提示 | placeholder | placeholder 只写 "Ask about this wiki…" | 🟡部分实现 | P2 | src/features/chat/ChatComposer.tsx:72 |
| `/` Skill 选择器 | 设计稿显式标注 | 无 Skill 入口 | ❌缺失 | P2 | src/features/chat/ChatComposer.tsx |
| 右面板 "引用与来源"（编号 + title + path + 箭头） | 列表 | CitationPanel 有等价实现，缺编号 idx、truncate、箭头 | 🟡部分实现 | P2 | src/features/chat/CitationPanel.tsx:20-37 |
| 右面板 "原始资料"（关联 raw/sources 文件） | 列出 PDF/docx/link | 完全缺失，citations 不含 raw 关联 | ❌缺失 | P2 | src/features/chat/CitationPanel.tsx、src-tauri/src/models/chat.rs(ChatCitation) |
| 右面板 "执行路径"（路径/版本/窗口/检索/耗时/Token） | 元信息 dl | 完全缺失 | ❌缺失 | P2 | src-tauri/src/models/chat.rs |
| 右面板 "操作"（保存/复制 MD/生成卡片/标记问题） | 4 按钮 | 仅 "保存到 wiki/queries"（且按钮在 ChatView 而非右面板） | ❌缺失 | P1 | src/features/chat/ChatView.tsx:132-140 |
| 状态栏 Chat 行（.app/chats/xx.json · N 条 · tokens） | 多 segment | 无 Chat 状态栏行 | ❌缺失 | P2 | src/components/app/StatusStrip.tsx（待查） |
| Agent vs BYOK 路由 | auto/agent/byok 三态 | 后端 resolve_route 完整；前端无选择器，固定 auto | 🟡部分实现 | P1 | src-tauri/src/commands/chat_commands.rs:272-315、ChatView.tsx:13 |
| 会话标题自动生成 | 新会话应自动命名 | 后端 create_session 默认 "New chat"，前端无"按首条问题改名" | 🟡部分实现 | P2 | src-tauri/src/services/chat_service.rs:38-42 |

## 2. 功能落差（PRD 对照）

- [ ] **PRD-CHAT-002 基于 Wiki 生成回答 / Markdown 渲染**：现状回答以纯文本 `whitespace-pre-wrap` 显示，设计稿示例含 `<strong>` `<em>` 有序列表与 `<sup>` 角标 → 目标：接入 remark-gfm+remark-math+rehype-katex+rehype-highlight（已用于 Wiki 视图） → 涉及 `src/features/chat/ChatView.tsx:118`、新建 `MessageContent.tsx` → 验收：回答正确渲染加粗/列表/代码/数学公式。
- [ ] **PRD-CHAT-003 引用角标可点击跳转**：现状 citations 只在右面板列表 → 目标：AI 消息正文末尾追加 `<sup>N</sup>` 序号、消息下方追加 citation 卡片（编号+标题+路径）、角标点击切到 Wiki 视图 → 涉及 `src/features/chat/ChatView.tsx:106-144` → 验收：点击角标/卡片打开对应页面。
- [ ] **流式输出**：现状 BYOK 整段返回，Agent 走 stdout stream 但前端只看终态 → 目标：BYOK 改 SSE/Anthropic stream；Agent 沿用 task log stream；前端订阅 task 日志增量追加到临时 assistant 气泡 → 涉及 `src-tauri/src/services/llm_service.rs:141`、`src-tauri/src/commands/chat_commands.rs:182-214`、`src/stores/chatStore.ts:209-233`、`src/features/chat/ChatView.tsx:145-157` → 验收：生成中可见逐字、可中途停止、最终内容与落盘一致。
- [ ] **模型/路由切换 UI**：现状前端固定 `auto` → 目标：主区头 `seg` 三态切换（Auto/Agent/BYOK-Anthropic）显示当前活跃模型 badge → 涉及 `src/features/chat/ChatView.tsx:13`、新增 segment 组件 → 验收：切换后下次 send 携带显式 route，badge 反映 lastResolvedRoute。
- [ ] **右面板操作区**：现状右面板仅 citations 列表 → 目标：补 "复制回答 Markdown / 生成知识卡片 / 标记问题回答" 按钮及 "原始资料 + 执行路径（tokens/耗时/检索数）" 元信息 → 涉及 `src/features/chat/CitationPanel.tsx`、扩展 `ChatMessage`/`ChatCitation` 携带 rawRefs、timing、tokenUsage → 验收：每项可点击产生预期副作用（剪贴板/跳 Exports/标记 persisted flag）。
- [ ] **会话搜索**：左侧无搜索框 → 目标：列表头加 28px 搜索框，按 title 模糊过滤 → 涉及 `src/features/chat/ChatSessionList.tsx:43` → 验收：输入即时过滤。
- [ ] **PRD-CHAT-005 边界保护测试**：现状无显式用例证明全局搜索不调模型 → 目标：加一条 e2e/单元测试断言 SearchService 不触发 LlmService/AgentService → 涉及 `src-tauri/tests/mvp_flow.rs` → 验收：测试常绿。
- [ ] **Skill 选择器 `/`**：Composer 无 `/` 入口 → 目标：输入 `/` 弹出 skills/* 下 SKILL.md 列表注入到 send 的 agent 字段或 prompt 前缀 → 涉及 `src/features/chat/ChatComposer.tsx`、后端 send 扩展 skillId → 验收：选定后 send 携带 skill。

## 3. 视觉 / 设计 token 落差

- 消息气泡未用 `.msg__avatar`（YOU/AI 28px 圆形）；无头像导致左右区分仅靠对齐（设计是左右均带头像）。
- 未使用设计稿 `.msg__head`（name + time + badge 行），当前无时间戳、无模型 badge、无 "ingest · N 来源" badge。
- 会话行设计含 `.chat-session__meta`（14:32 · 8 条消息），当前只有 title。
- Composer 缺 `.composer__foot` 结构（badge + 附加资料 + 引用页面 + hint + 停止 + 发送），当前只一个 textarea+send。
- 左栏宽度 220px（`ChatView.tsx:80`）与设计 260px 不一致。
- 右面板 citation 缺编号 `.citation__idx` 与箭头图标，整体样式与设计的 `.citation` class 不对齐。
- 生成中状态缺 `.dotstatus--busy` 动效点；当前用 Tailwind `animate-pulse`。
- ChatView 未承载 `.toolbar` 会话头（border-bottom + 标题 + 右侧操作组）。

## 4. 交互 / 可访问性落差

- 消息列表无 `role="log"` / `aria-live="polite"`，生成中更新不被读屏播报。
- Composer textarea 无 `aria-label`，仅靠 placeholder。
- 会话列表行 hover 才出现 ✎/× 按钮，键盘用户无法重命名/删除（无菜单、无上下文键）。
- 角标跳转缺失（见 §2）。
- 取消按钮在 Composer 外（`ChatComposer.tsx:50-58`），与设计的 composer 底栏内 "停止" 不符；快捷键无 Esc 取消。
- `⌘↵ / Ctrl+Enter` 发送未在 placeholder 标注，且代码只处理 Enter（`ChatComposer.tsx:67`），Mac 用户的 ⌘↵ 没有专门 UI 提示。
- CitationPanel 列表用 `<ul>` 自定义 button，键盘 Tab 可达，但无 "打开" 文字 label，仅 title 提示。
- 右面板操作区按钮缺失，导致用户无法仅用键盘完成"保存到 wiki/queries"以外的动作。
- 无错误重试入口：send 失败仅顶部黄条提示，无"重试"按钮。

## 5. 建议实施顺序

1. **P0 消息渲染升级**：抽 `MessageContent` 复用 Wiki 渲染管线，让 AI 回答支持 Markdown/列表/代码/数学；同步补 avatar + time + model badge（解决最显眼的落差，不涉及后端）。
2. **P1 流式输出**：BYOK 改 streaming，Agent 沿用 task log，前端 chatStore 增量 append；保留终态 reload 作幂等兜底。
3. **P1 引用角标 + 消息内 citation 卡片**：在每条 assistant 消息上渲染 `<sup>N</sup>` 和底部卡片，点击切到 Wiki。
4. **P1 模型/路由 segment**：主区头加 Auto/Agent/BYOK 三态，反映并控制 resolve_route；同步会话头 toolbar（标题/保存/重命名/删除）。
5. **P1 右面板操作区 + 执行路径**：补按钮组，扩展 DTO 带回 tokens/耗时/检索数；补"原始资料"需先在 citations 模型加 rawRefs 字段。
6. **P1 会话搜索 + meta 行**：列表头加搜索框、行加 "时间·N 条" 副行。
7. **P2 Skill `/` 选择器 + 顶栏历史 + 状态栏 Chat 行 + 路径回显**：增量打磨，依赖前序组件落地。
8. **P2 a11y 扫尾**：aria-live、键盘菜单、错误重试。
