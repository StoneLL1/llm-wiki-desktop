# LLM Wiki Desktop — Graphify code map

This directory is a local, code-first knowledge graph produced with Graphify on 2026-08-05.
The extraction used `graphify extract . --code-only --max-workers 4`, so parsing was local and
deterministic: no source files or project documentation were sent to an LLM.

## Start here

- Open `GRAPH_TREE.html` for a lightweight, searchable file-and-symbol tree.
- Query `graph.json` from the repository root:

  ```powershell
  graphify god-nodes --top 20 --graph graphify-out/graph.json
  graphify explain "ProjectContext" --graph graphify-out/graph.json
  graphify affected "TaskService" --graph graphify-out/graph.json
  graphify query "where is project trust enforced?" --graph graphify-out/graph.json
  ```

## Code landmarks

| Area | Main paths | Role |
| --- | --- | --- |
| Desktop composition | `src/app/`, `src/components/app/` | React app entry, desktop shell, routing and shared panels. |
| Product surfaces | `src/features/` | Feature-owned UI for project opening, import, wiki, graph, chat, workflows, lint, exports, dashboard and settings. |
| Frontend state/contracts | `src/stores/`, `src/hooks/`, `src/services/`, `src/types/` | Zustand state, Tauri event handling/API wrappers and IPC DTOs. |
| Tauri boundary | `src-tauri/src/lib.rs`, `src-tauri/src/commands/` | Command registration and thin IPC adapters. |
| Backend composition | `src-tauri/src/app_state.rs`, `src-tauri/src/services/` | Shared service construction and domain-service implementations. |
| Long-running work | `src-tauri/src/tasks/`, `src-tauri/src/services/workflow_service/`, `src-tauri/src/services/import_v2/` | Task lifecycle, workflow orchestration, cancellable import and progress handling. |
| Domain/persistence safety | `src-tauri/src/models/`, `src-tauri/src/utils/` | DTOs/domain models, project paths, confirmation and path-safety utilities. |

## High-connectivity anchors

Graphify identified these as the strongest architectural entry points: `ProjectContext` (654
edges), `AppState` (286), `TaskService` (185), `FileStore` (168), `CancellationToken` (99),
`ImportV2Service` (88), `ImportV2Api` (75), `GitService` (64), `useProjectStore` (65), and
`useNavigationStore` (49). Start code exploration from the relevant anchor, rather than reading
the entire repository.

`ProjectContext` is the key backend boundary for project paths and is referenced by import,
compile, Git, export, lint, workflow and chat services. `AppState` wires services to the command
layer. On the frontend, `useProjectStore` connects the shell and most feature views, while
`useNavigationStore` controls cross-surface routing.

## Scope and refresh policy

The current graph contains 11,147 nodes and 32,745 edges from 686 code files. It intentionally
does **not** semantically index the repository's Markdown, PDFs, images or video; Graphify
skipped 619 such files in this run. Configuration formats without supported parsers and 41 files
that produced no AST nodes are also absent. Treat this as a code relationship map, not a substitute
for the authoritative specifications in `SPEC/` and `docs/superpowers/specs/`.

To refresh after code changes, run `graphify update .` and then regenerate the tree. Those commands
overwrite the generated graph, so follow this repository's Git-checkpoint policy first and never
overwrite a graph you need to preserve. Do not install Graphify hooks or its Codex integration
unless the team explicitly wants persistent automation; both modify repository configuration.
