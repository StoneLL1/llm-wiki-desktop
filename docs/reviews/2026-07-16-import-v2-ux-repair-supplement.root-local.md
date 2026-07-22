# Import V2：连续性与流畅性补充 Review / 修复记录

日期：2026-07-16

本轮以“本地工具优先保证用户始终知道发生了什么、能否继续、下一步是什么”为第一性原理，补充复核并修复上一轮 review 中遗留的三个高价值问题。

## 一、判断标准

导入不是一次同步函数，而是一个跨页面、跨进程、可能跨重启的用户操作。因而需要保持以下不变量：

1. 用户发起的一次扫描/解析应该有稳定身份，页面重载后仍能找到它。
2. “任务不存在”不能被解释成“任务仍在运行”；无法恢复时必须给出可行动的恢复入口。
3. 全局任务抽屉需要按用户操作批次理解任务，而不是把并行子任务混成一个数字。
4. “打开结果”只能在结果确实已提交、且预览证据仍存在时成立；数据不一致时要解释并转入详情。

## 二、本轮已修复

### 1. Discovery task 从前端内存下沉到 session 持久化

涉及：

- `src-tauri/src/models/import_v2.rs`
- `src-tauri/src/services/import_v2/session_store.rs`
- `src-tauri/src/services/import_v2/orchestrator.rs`
- `src-tauri/src/commands/import_v2_file_commands.rs`
- `src/types/importV2.ts`
- `src/features/import/useImportWorkflow.ts`

修复内容：

- `ImportSession` 新增可选 `discoveryTaskId`，写入 `.app/import-sessions/<sessionId>/session.json`。
- 旧版本 session JSON 缺少该字段时默认读取为 `null`，不需要迁移文件。
- `start_add_import_paths_v2` 创建任务后、启动 worker 前持久化任务 ID；持久化失败会回收尚未启动的任务，避免留下孤儿任务。
- Import workspace 首次进入、页面切换后重新挂载时，都会把持久化 ID 与 task store 中的恢复任务重新绑定。
- 如果 task hydration 完成后仍找不到该任务，界面显示“上一次扫描未能恢复，请重新扫描来源”，而不是显示一个无限进行中的状态。

用户价值：重启或页面重挂载不再让扫描状态凭空消失；即使任务记录损坏，也有明确且低成本的继续路径。

### 2. 全局任务抽屉增加真实批次视图

涉及：

- `src/components/app/TaskLogDrawer.tsx`
- `src/i18n/locales/en.json`
- `src/i18n/locales/zh-CN.json`

修复内容：

- 使用后端持久化的 `batchId` 聚合 Import 子任务。
- 批次默认折叠，只显示 `完成数/总数` 和批次状态，避免长任务把抽屉变成日志墙。
- 展开后显示 active/failed/cancelled 摘要、最近 24 条批次日志，以及每个子任务的跳转按钮。
- 子任务按钮仍然定位到既有的单任务日志详情，保留细粒度诊断入口。

用户价值：用户先看到“这一批导入整体进度”，只有需要诊断时才展开日志，降低并行任务带来的认知负担。

### 3. 历史 `open_result` 增加前端状态真实性校验

涉及：

- `src/types/importV2Presentation.ts`
- `src/features/import/ImportHistoryPanel.tsx`
- `src/features/import/ImportView.tsx`

修复内容：

- 只有 `completed` / `partially_committed` 且后端 action 仍包含 `open_result` 时，历史列表才显示“打开结果”。
- 实际打开前再次要求对应 item 为 `completed` 且存在 preview。
- 如果历史 action、session 快照和预览证据不一致，不再静默无响应：转入历史详情并提示结果暂不可用。

用户价值：按钮语义和实际可用性一致；历史 artifact 缺失时，用户仍能看到剩余状态和 issue，而不是被带到空白预览。

## 三、回归覆盖

新增/更新了以下回归：

- session round-trip 恢复 discovery task identity。
- Import workflow 在重新打开 workspace 时重新挂载持久化 discovery task。
- Task drawer 批次折叠、批次最近日志和子任务跳转。
- 历史处理中状态即使携带陈旧 `open_result` action，也回退到详情入口。
- ImportSession 的 `discoveryTaskId` 保持对旧 session shape 的兼容。

## 四、验证结果

通过：

- `npx tsc -b --pretty false`
- `npm run lint`
- Rust session store 定向测试：7 passed
- Rust task 定向测试：55 passed
- Rust Import V2 service 定向测试：140 passed

当前环境无法进入 Vitest/Vite 配置加载阶段，原因仍是：

- `@tailwindcss/oxide-win32-x64-msvc` native binding 报 `stream did not contain valid UTF-8`
- Vite externalize-deps 报 `spawn EPERM`

因此本轮前端断言未被错误地标记为通过；修复本机 Node 原生依赖/进程权限后，应从头重跑 `npm run check`。

## 五、仍建议后续处理

- 将批次日志聚合进一步下沉为后端统一 DTO，避免前端只聚合当前已加载的子任务日志。
- 为 discovery 持久化 source paths，使“重新扫描”可以一键复用原来源，而不是要求用户重新选择。
- 将历史 item attempts/warnings 做成真正不可变快照，减少长期历史对当前 session 的依赖。
- 修复本机 Tailwind oxide/Vite 启动环境后，补跑完整 Vitest、`npm run build` 和统一 `npm run check`。
