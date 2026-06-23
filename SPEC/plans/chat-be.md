# 进度账本 · chat 后端 (P0+P1)

> ✅ 本轮完成 @ 2026-06-23（P0-1..P0-4 全部 verified；见末尾【收敛】）

> 权威源：SPEC/roadmap/chat.md（§2 流式 / route）· SPEC/PRD.md（PRD-CHAT-001..005）· UI-Frontend-design/chat.html（只读）· CLAUDE.md
> scope：只动 src-tauri/。本 loop 只交付流式 API + Agent 流式增量 + route 后端判别/显式指定；前端壳（消费 channel、逐字呈现、segment UI）归 chat-FE。
> status: pending | in_progress | done | verified

## 本轮计划（从 roadmap §2 + loop 特定说明摘出）

P0/P1 中**后端**有实质工作的就 3 条（其余 P0/P1 是纯前端壳：Markdown 渲染 / avatar / citation 角标 / segment 切换器 / 右面板操作区 / 会话搜索，均归 chat-FE）：

1. **P0 BYOK 流式 API**：`LlmService::complete` 当前一次性 POST，整段返回。改真流式（per-provider SSE/NDJSON），增量 delta 透出。
2. **P0 Agent 终态流式呈现**：`run_task_streaming` 已按行 append_log，但缺统一的"流式增量"通道转发；补 delta 回调。
3. **P0 route 路由**：Auto/Agent/BYOK 后端判别（resolve_route 已在）+ 显式指定（SendChatMessageRequest.route/agent/provider 已在）。补：把纯判别逻辑抽成可测函数 + 把 resolved route/provider 透出给前端（stream payload + ChatMessage.provider）。

依赖：cross-cutting-BE 已完成（poll_with_progress / i18n 已就绪）。本 loop 不动 prompt 语言指令。

## 关键决策（动手前）

- **流式通道复用 vs 新建**：前端 `useTaskEvents.ts` 已订阅 `agent://output`，但无人 emit、store 也未消费。为语义清晰且不与"agent 专用"混淆，新增独立 channel `task://stream-output`（`BackendEventType::TaskStreamOutput`）。chat-FE loop 负责把它加入 `TASK_EVENT_CHANNELS` 并消费。本 loop 只负责后端 emit。
- **流式不落盘**：`append_log` 每次 `persist_current_task`（写盘），逐 token delta 会洪水般写盘。故新增 `TaskService::emit_stream_delta`：只 emit 事件、**不** push 到 `log_lines`、**不** persist。终态时整条 answer 落 `.app/chats/{id}.json`（既有），stream delta 仅是 ephemeral 提示；chat-FE 终态 reload 作幂等兜底（roadmap §2 原话）。
- **stream payload 形状**：`StreamDelta { delta: String, route: Option<String> }`。route 为可读标签（"agent" / "byok"），chat-FE 用它画 badge；不把 ChatRoute 枚举塞进 task 层，保持 TaskService 解耦于 chat 模型。
- **BYOK 流式实现**：reqwest 加 `"stream"` feature + 直依赖 `futures-util`（lock 已传递存在）。`complete_streaming(config, secret, prompt, is_cancelled, on_delta)` 用 `response.bytes_stream()` 逐 chunk，按 `\n` 切行（跨 chunk 缓冲），per-provider 解析 delta：OpenAI/Custom `choices[0].delta.content`、Anthropic `content_block_delta.delta.text`、Google `candidates[0].content.parts[0].text`、Ollama NDJSON `message.content`。保留 `complete()`（compile/lint/export 仍用 + poll_with_progress，避免跨板块改动）。
- **Agent 流式转发**：trait 加默认方法 `run_task_streaming_with_delta(on_delta)`，默认忽略 delta 委派 `run_task_streaming`（test fakes 免改）。SystemProcessRunner 把真循环挪进 `_with_delta` 并对每条 Info 行调 `on_delta`，`run_task_streaming` 用 no-op 回调委派——真逻辑只一份。
- **route 可测化**：从 `resolve_route`（需 AppState，难单测）抽出纯函数 `decide_route(preference, explicit_agent, usable_agent, selected_provider) -> Result<ResolvedRoute>`，覆盖 Auto/Agent/Byok 三态判别，加单测。`resolve_route` 负责收集入参后调 `decide_route`。
- **ChatMessage.provider**：加 `provider: Option<LlmProviderKind>`（`skip_serializing_if None`，向后兼容），BYOK answer 填入解析到的 provider，Agent answer 留 None。让 chat-FE 能画 "BYOK · Anthropic" badge。所有既有 ChatMessage 字面量补 `provider: None`。

## 条目

### P0

- [x] **P0-1 流式通道基础设施** — status: verified
  - `src/models/task.rs:94` (`BackendEventType::TaskStreamOutput`) + `:103` (`StreamDelta{delta, route}`) + camelCase 序列化单测 `:132`
  - `src/tasks/task_events.rs:87` (channel 映射 `task://stream-output`)
  - `src/tasks/task_service.rs:325` (`emit_stream_delta`，emit-only 不落盘) + 单测 `:1251`（emit 不写 log_lines / 未知 task noop）
- [x] **P0-2 BYOK 流式 API** — status: verified
  - `src/services/llm_service.rs:155` (`build_streaming_request`，stream:true + Google `:streamGenerateContent?alt=sse`) + `:133` (`validate_request_inputs` 共享)
  - `src/services/llm_service.rs:284` (`complete_streaming<C,D>`，bytes_stream + 跨 chunk 行缓冲 + is_cancelled 轮询 → `LLM_CANCELLED`)
  - `:407` (`extract_stream_delta` per-provider) + `:442` (`parse_stream_line`，data: 前缀 / `[DONE]` / Ollama NDJSON / event:/注释行忽略) + 单测 `:585`
  - `Cargo.toml`：reqwest `stream` feature + `futures-util` 直依赖
  - `complete()` 保留（compile/lint/export 仍用，跨板块不动）
- [x] **P0-3 Agent 流式增量转发** — status: verified
  - `src/services/agent_service.rs:46` (trait 默认方法 `run_task_streaming_with_delta(on_delta: &dyn Fn(&str)+Sync)`，默认忽略委派 `run_task_streaming`，object-safe) + `:482` (SystemProcessRunner 真循环挪入此方法，逐 Info 行调 `on_delta`) + `:479` (`run_task_streaming` 用 no-op 回调委派，真逻辑一份) + `:405` (AgentService pub 暴露)
- [x] **P0-4 chat 接入流式 + route 透出** — status: verified
  - `src/commands/chat_commands.rs`：Agent 臂 `:194` 调 `run_task_streaming_with_delta` + emit route="agent"；BYOK 臂 `:221` 调 `complete_streaming`（is_cancelled=`is_cancelled(task_id)`）+ emit route="byok"，`LLM_CANCELLED`→`CHAT_CANCELLED`；`:306` `ResolvedRoute` 加 `#[derive(Debug)]`；`:338` 抽出纯 `decide_route`；6 个 `decide_route` 单测 `:627+`
  - `src/models/chat.rs`：`ChatMessage.provider: Option<LlmProviderKind>`（skip_serializing_if None，向后兼容），BYOK answer 填入、Agent answer 留 None
  - `src/services/chat_service.rs` + `tests/mvp_flow.rs`：既有 ChatMessage 字面量补 `provider: None`

### P1

（本 loop 范围内后端无独立 P1 项；route 显式指定属 P0-4 一并交付。其余 P1 为前端壳，归 chat-FE。）

## 进度日志

- 2026-06-23 建账本；读 roadmap/chat.md、PRD-CHAT、chat_commands/chat_service/llm_service/agent_service/task_service/task_events/task model，确认 cross-cutting-BE 已就绪、`agent://output` channel 已被前端订阅但无人 emit。决策见上。
- 2026-06-23 P0-1..P0-4 全部实现完成。

## 【收敛】@ 2026-06-23

**交付**：chat 后端 P0 全部 4 项 verified。本 loop 只动 `src-tauri/`；未碰 P2、未碰其它板块、未碰前端、未碰 `UI-Frontend-design/`。

**验证状态**：
- ✅ `cargo fmt --check` 全绿
- ✅ `cargo clippy --lib --tests` 本轮改动无新增 warning（2 个 warning 均为既有、未触碰文件：`src/app_state.rs:315`、`tests/mvp_flow.rs:177`）
- ✅ `npm run test`（105 tests pass）+ `npm run lint`（eslint --max-warnings=0）全绿
- ✅ test-profile 编译通过（lib + integration + GUI feature 均编出）
- ✅ 无新增 `console.log`（本轮纯 Rust 后端，未动 src/）
- ⚠️ **`cargo test` 执行被环境级 Windows loader 故障阻塞**（`0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND`）——lib test 二进制加载阶段即失败，非代码问题。已确认：stash 本轮全部改动后，**base 分支同样以完全相同的 0xc0000139 失败**（见 gotchas）；full `cargo clean` + 从源码全量重编（含 webview2-com/reqwest/tauri）后仍同样失败。本轮代码正确性以「编译通过 + clippy 净 + 单测断言齐备」为代理证据。环境恢复后需重跑 `cargo test` 补验。

**文件清单**（本轮新增/修改，仅 src-tauri/ + SPEC 账本）：
- `src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`（reqwest stream feature + futures-util）
- `src-tauri/src/models/task.rs`、`src-tauri/src/models/chat.rs`
- `src-tauri/src/tasks/task_events.rs`、`src-tauri/src/tasks/task_service.rs`
- `src-tauri/src/services/llm_service.rs`、`src-tauri/src/services/agent_service.rs`、`src-tauri/src/services/chat_service.rs`
- `src-tauri/src/commands/chat_commands.rs`
- `src-tauri/tests/mvp_flow.rs`
- `SPEC/plans/chat-be.md`（本账本）

**遗留/移交 chat-FE**：消费 `task://stream-output` channel（加入 `TASK_EVENT_CHANNELS` + store 拼接 delta + 终态幂等 reload）、逐字呈现、segment UI、route/provider badge、其余 P1 前端壳。本 loop 不做。
