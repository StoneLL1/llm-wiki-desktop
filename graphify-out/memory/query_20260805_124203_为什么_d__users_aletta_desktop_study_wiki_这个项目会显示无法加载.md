---
type: "query"
date: "2026-08-05T12:42:03.108793+00:00"
question: "为什么 D:\\Users\\Aletta\\Desktop\\Study\\wiki 这个项目会显示无法加载项目工作流，并报 A selected Source version is missing or its content hash no longer matches."
contributor: "graphify"
outcome: "useful"
source_nodes: ["WorkflowAccessSnapshot", "WorkflowService", "ProjectContext", "FileStore"]
---

# Q: 为什么 D:\Users\Aletta\Desktop\Study\wiki 这个项目会显示无法加载项目工作流，并报 A selected Source version is missing or its content hash no longer matches.

## Answer

Expanded from original query via graph vocab: [source, version, content, hash, missing, mismatch, workflow, readiness, prepare, project, selection, manifest]. Verified chain: get_workflows_overview calls WorkflowService::project_overview, overview_prerequisites, build_snapshot, CompileService::list_source_versions and resolve_source_versions, then SourceRegistry::resolve_compile_source_version. The project has ten current Source records; five migrated legacy records point to missing current wikiPath files and also lack humanEditHash: 0ae21703, 62653c8d, 6c382266, 7b0a7eae, eb4860e2. Index pointers match, and all five baseline backups still exist. Because source IDs are sorted, 0ae21703 fails first when wiki/sources/web/www.xiaohongshu.md cannot be read; the backend maps that I/O failure to generic COMPILE_SOURCE_VERSION_INVALID. The migration accepts absent wikiPath by persisting humanEditHash=None, while runtime resolution requires both the file and humanEditHash, creating structurally accepted but operationally unusable V3 state. Workflow overview resolves every Source for every workflow kind, so one dangling Source aborts the whole page rather than becoming a per-workflow repair prerequisite.

## Outcome

- Signal: useful

## Source Nodes

- WorkflowAccessSnapshot
- WorkflowService
- ProjectContext
- FileStore