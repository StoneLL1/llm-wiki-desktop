---
type: "query"
date: "2026-08-05T05:46:00.449070+00:00"
question: "请你调用llm wiki desktop context skill来了解项目上下文，以及用Graphify来溯源。从第一性原理出发。帮我溯源定位几个问题：1、有些旧项目打开，工作流页面没法用。2、一次性导入量过多了之后应用会卡死 3、打开现有知识库兼容性还不够好，导致很多功能无法使用。"
contributor: "graphify"
outcome: "useful"
source_nodes: ["ProjectLayout", "WorkflowAccessSnapshot", "start_import_items_v2", "FileDiscoveryService", "SessionStore"]
---

# Q: 请你调用llm wiki desktop context skill来了解项目上下文，以及用Graphify来溯源。从第一性原理出发。帮我溯源定位几个问题：1、有些旧项目打开，工作流页面没法用。2、一次性导入量过多了之后应用会卡死 3、打开现有知识库兼容性还不够好，导致很多功能无法使用。

## Answer

Expanded from original query via vocab: [workflow legacy project compatibility migration task trust writable manifest; import batch staging extraction queue progress cancellable source manifest; compatible vault assessment repair native trusted writable capability sources knowledge]. Graphify traced ProjectLayout to WorkflowAccessSnapshot through app_state, and FileDiscoveryService to TaskService through import_v2_file_commands/AppState; the Tauri IPC boundary to React ImportQueue required manual source tracing. Diagnosis: (1) old projects fail because native recognition and strict native trust use different marker sets; NativeLegacy is Repairable, loses trust, has no general repair transition, and missing task_state_root forces MemoryOnly, so external/write workflows are blocked. (2) large imports freeze despite bounded workers because discovery emits/persists progress per file, import start creates one persisted BackendTask per item, multiple item lookups and frontend store updates are quadratic, and every item mutation/commit rewrites the whole session item set, producing O(N^2) control-plane and disk work. (3) compatibility enablement only writes .app/compat guidance; it does not map safe write/state roots, while many services still hardcode native raw/wiki/.app/exports paths instead of ProjectLayout. Generic layout discovery is shallow and name-heuristic, so nonstandard or CJK vaults are misclassified. The unifying cause is that recognition, authority, capabilities, and execution do not share one versioned layout/capability contract. Priorities: add versioned migration for legacy native projects; make format adapters expose explicit read/write/state capabilities; remove hardcoded native paths; batch/coalesce import tasks, events, IPC, store mutations, and session writes; add legacy/compatible E2E and 1k/10k import performance tests.

## Outcome

- Signal: useful

## Source Nodes

- ProjectLayout
- WorkflowAccessSnapshot
- start_import_items_v2
- FileDiscoveryService
- SessionStore