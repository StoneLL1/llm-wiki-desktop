# Import V2 体验优先复审补充与修复记录

日期：2026-07-16  
范围：对照 `2026-07-11-import-v2-design.md`，聚焦本地工具场景下的连续性、反馈真实性和操作流畅度。

## 复审原则

本轮把“用户下一步是否明确、任务是否能继续、界面是否如实表达状态”作为第一性原理标准：

1. 用户离开页面、切换项目或重启应用后，不能因为前端内存丢失而失去继续工作的入口。
2. 一个用户动作应对应一个可识别的任务边界；批量导入不能被拆成一串互相无关的后台任务。
3. 已完成解析但等待确认的任务，不应继续表现成运行中；不可恢复的结果，不应继续显示可打开。
4. 失败或缺失必须提供能真正改变状态的下一步，而不只是关闭提示。

## 本轮修复

### 1. 发现任务在重启后的连续性

- 在 Import session 中持久化 `discoveryTaskId`。
- 启动扫描后先写入任务身份，再启动后台 worker；持久化失败时回收尚未启动的任务。
- 空 session 只要仍有 discovery task，也会被识别为未完成 session，避免“扫描还没产出文件就重启”后丢失恢复入口。
- 页面重新挂载时重新接上持久化的 task identity；普通项目导航不会误清除用户主动关闭的提示。
- task 已不存在时明确显示“任务不可恢复”，并提供真正执行文件/文件夹重新扫描的按钮。

### 2. 任务抽屉的批次可读性

- 用后端持久化的 `batchId` 将同一次 Import 操作聚合成一个批次。
- 批次默认折叠，展开时加载该批次全部子任务的日志，子任务仍可单独查看。
- 批次标题携带首个输入名称，进度与最近日志在同一层展示，减少用户在大量任务中寻找上下文的成本。
- `waiting_for_confirmation` 被视为“后台不再运行、等待用户操作”：从 active/cancel 路径移出；只有对应 item 已是 `preview_ready` 时才显示“可查看”，登录或能力等待显示为“等待操作”。
- task 更新事件进入 `waiting_for_confirmation` 时会释放条目的 pending lock，避免预览已就绪但队列仍被禁用。

### 3. 历史结果的真实性与交互收敛

- `open_result` 只有在历史条目状态是 `completed`/`partially_committed` 且后端动作仍可用时才展示。
- 打开历史结果时再次校验条目状态与 preview；缺失时保留在历史详情中显示明确告警，而不是仅依赖短暂 toast。
- 从历史详情进入 Markdown 预览前先关闭详情 dialog，避免两个 modal 同时存在、焦点困在错误层级。
- 关闭历史详情时清理缺失结果提示状态，避免下一次打开残留旧反馈。

### 4. 任务创建与条目绑定的重启竞态

- `start_import_items_v2` 在 worker spawn 前持久化 item-to-task 绑定。
- 绑定失败会回收所有尚未启动的任务，不留下“任务存在但条目仍像未认领”的中间状态。
- worker 的原有 claim 流程仍负责状态迁移，预绑定只写入身份，不提前伪造 Inspecting/Running 状态。
- 应用重启后，若预绑定的 queued task 已失败/不存在，会释放失效 task identity，保证用户可以重新开始。

### 5. 异步响应的顺序真实性

- session refresh 如果发现期间发生了新的 mutation，不再静默丢弃；旧请求收尾后会补发一次刷新。
- history 分页响应携带当前 history request generation，项目/session 已切换时丢弃旧分页结果，避免跨项目串历史。
- recovery 将失效 task identity 清除，避免失败条目显示无法打开的死日志入口。

## 回归覆盖

- Rust：session discovery identity roundtrip、空 discovery session 仍可恢复、item task pre-bind 持久化。
- React：重开页面重新接回 discovery task、discovery 不可用时的真实重扫回调、批次折叠与全量日志加载、`waiting_for_confirmation` 不显示取消、历史详情缺失结果告警。
- React workflow：`task_updated(waiting_for_confirmation)` 释放 pending item lock。
- Rust：重启后释放 queued item 的失效预绑定 task identity。
- TypeScript 类型检查和 ESLint 已通过。

## 验证结果

已通过：

- `npx tsc -b --pretty false`
- `npm run lint`
- `npm run check:console`
- `cargo test` 定向覆盖 pre-bind 回归（通过；保留既有 transaction dead-code warning）

统一 `npm run check` 仍无法完成：测试进程启动阶段的 Tailwind 原生模块报 `stream did not contain valid UTF-8`，随后 Vite 报 `spawn EPERM`。因此 Vitest 尚未进入用例执行阶段，不能将完整门禁标记为通过。

## 仍值得后续处理

1. 文件夹扫描目前仍在完整扫描结束后才把发现结果写入 session；取消超大目录扫描会丢弃尚未完成的候选，需要以后按批次持久化 scan/input，或明确告知“取消会丢弃本次发现”。
2. 任务抽屉的批次日志目前是“按子任务拉取后前端聚合”；如果需要跨进程/重启后完整展示，后端可提供批次级日志 DTO。
3. 项目快速切换时 task recovery 仍依赖全局 task store；若要彻底杜绝乱序覆盖，应给 recovery 加 project generation。
4. 历史详情仍依赖当前 session 的历史快照兼容策略；更长期可以统一所有历史 item 的 immutable presentation DTO。
5. 环境修复后应从头重跑 `npm run check`，补齐本轮 React 组件回归的真实执行证据。
