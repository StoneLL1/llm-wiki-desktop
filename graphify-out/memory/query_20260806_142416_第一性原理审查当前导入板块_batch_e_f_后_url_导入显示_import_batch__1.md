---
type: "audit"
date: "2026-08-06T14:24:16.576057+00:00"
question: "第一性原理审查当前导入板块：Batch E/F 后 URL 导入显示 Import batch (1)，并核对大批量导入的正确性、可观察性与测试覆盖"
contributor: "graphify"
outcome: "useful"
source_nodes: ["useImportWorkflow", "useImportTaskCoordinator", "importV2Api", "TaskLogDrawer", "ImportBatchStatus", "orchestrator.rs", "import_v2_commands.rs", "import_v2_web_commands.rs", "SessionStore", "TaskService"]
---

# Q: 第一性原理审查当前导入板块：Batch E/F 后 URL 导入显示 Import batch (1)，并核对大批量导入的正确性、可观察性与测试覆盖

## Answer

扩展词元: import,batch,url,source,task,item,session,orchestrator,coordinator,presentation,frontend,backend。结论: URL 入口仍创建 ImportInputKind::Url；直接症状来自 begin_batch_operation 把单项也命名为 Import batch (1)，并用 session 级 import-v2-operation:<session> 填充 BackendTask.batchId。TaskLogDrawer 仍按旧 task-per-item 模型用 batchId 分组和按任务数计数，导致同会话多次操作合并、单项 URL 丢失来源身份、失败项也可能显示任务成功。生产 batch 分支在 worker 后提前返回，跳过 finalize_exact_duplicate；登录恢复又调用受 200 项限制的 legacy start_import_items_for_state；startNewQueuedItems 吞掉 startBatch 错误，外层会把 URL 当成功而清空。测试盲区: URL 测试用无 operation marker 的旧任务形状；无 start_import_batch_v2 到 URL engine 的端到端测试。10k 规模测试连续两次超过 120 秒，命令在返回/可取消前同步装载并写入 item cohort。

## Outcome

- Signal: useful

## Source Nodes

- useImportWorkflow
- useImportTaskCoordinator
- importV2Api
- TaskLogDrawer
- ImportBatchStatus
- orchestrator.rs
- import_v2_commands.rs
- import_v2_web_commands.rs
- SessionStore
- TaskService