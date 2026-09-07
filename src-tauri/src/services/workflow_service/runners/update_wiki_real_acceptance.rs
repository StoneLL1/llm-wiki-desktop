//! Opt-in local acceptance with the installed, authenticated Claude CLI.
//! This is ignored by normal gates; it sends only synthetic disposable content.
#![cfg(test)]

use super::*;
use crate::app_state::AppState;
use crate::models::agent::{AgentConfig, AgentKind};
use crate::models::project::ProjectTemplate;
use crate::models::workflow::{WorkflowDisplayStatus, WorkflowStartOutcome};
use crate::services::{PrepareWorkflowInput, ProjectService, WorkflowPreparationEnvironment};
use std::collections::HashSet;
use std::fs;

#[tokio::test]
#[ignore = "requires explicit LLM_WIKI_RUN_REAL_CLAUDE=1 and an authenticated local Claude CLI"]
async fn real_claude_default_route_generates_and_applies_disposable_wiki() {
    assert_eq!(
        std::env::var("LLM_WIKI_RUN_REAL_CLAUDE").as_deref(),
        Ok("1")
    );
    let evidence_root = tempfile::Builder::new()
        .prefix("llm-wiki-real-claude-")
        .tempdir()
        .unwrap()
        .keep();
    println!("real_workflow_evidence={}", evidence_root.display());
    let root = evidence_root.join("中文 Workflow 验收");
    let state = AppState {
        project_service: ProjectService::with_config_dir(evidence_root.join("config")),
        ..AppState::default()
    };
    let project = state
        .project_service
        .create_project(
            root.to_str().unwrap(),
            "Workflow acceptance",
            ProjectTemplate::General,
        )
        .unwrap();
    let context = state
        .project_registry
        .register_trusted_native(&project.project_id, &root)
        .unwrap();
    state
        .task_service
        .set_project_context(
            context.project_id.clone(),
            root.clone(),
            root.join(".app/tasks"),
        )
        .unwrap();
    // Only this disposable project's route is configured; global settings and
    // actual CLI authentication remain read-only, with no provider fallback.
    state
        .file_store
        .write_json_atomic(
            &context,
            ".app/agent-config.json",
            &AgentConfig {
                default_agent: Some(AgentKind::Claude),
            },
        )
        .unwrap();
    let source_path = "raw/sources/markdown/验收来源.md";
    let extracted_path = "raw/extracted/验收来源.md";
    let source = "# 灯塔知识维护\n\n灯塔项目有三个步骤：保存原始资料、生成候选 Wiki、检查后应用。原始资料必须保留，用户的 Markdown 编辑必须通过检查点和冲突复核保护。复核通过后更新索引，结果页面可以打开。\n";
    for path in [source_path, extracted_path] {
        fs::write(root.join(path), source).unwrap();
    }
    state
        .file_store
        .write_json_atomic(
            &context,
            ".app/source-index.json",
            &serde_json::json!({"sources": {source_path: [extracted_path]}}),
        )
        .unwrap();
    state
        .git_service
        .create_checkpoint(
            &context,
            CheckpointPurpose::HighRiskOperation,
            "Synthetic source and explicit Claude route for acceptance",
        )
        .unwrap();
    state
        .workflow_service
        .register_runner(Arc::new(UpdateWikiRunner::new(|_| {
            panic!("acceptance drives the authorized runner directly")
        })))
        .unwrap();
    let prepare = |scope| {
        state
            .workflow_service
            .prepare(
                &WorkflowPreparationEnvironment {
                    context: &context,
                    access: state.resolve_workflow_access(&context).unwrap(),
                    settings_service: &state.settings_service,
                    secret_service: &state.secret_service,
                    agent_service: &state.agent_service,
                },
                PrepareWorkflowInput {
                    kind: WorkflowKind::UpdateWiki,
                    scope,
                    route_selection: None,
                },
            )
            .unwrap()
    };
    let initial = prepare(None);
    assert_eq!(initial.available_source_versions.len(), 1);
    let prepared = prepare(Some(WorkflowScope::UpdateWiki {
        mode: UpdateWikiMode::ChangedSources,
        source_versions: initial.available_source_versions,
    }));
    fs::write(
        evidence_root.join("preparation.json"),
        serde_json::to_vec_pretty(&prepared).unwrap(),
    )
    .unwrap();
    let detected = state.agent_service.detect_agent(AgentKind::Claude, true);
    fs::write(
        evidence_root.join("agent-detection.json"),
        serde_json::to_vec_pretty(&detected).unwrap(),
    )
    .unwrap();
    println!(
        "real_workflow_agent_state={:?} version={:?} route_ready={} blocking_codes={:?}",
        detected.state,
        detected.version,
        prepared.route.is_some(),
        prepared
            .prerequisites
            .iter()
            .filter(|item| item.blocking)
            .map(|item| &item.code)
            .collect::<Vec<_>>()
    );
    assert!(
        !prepared.prerequisites.iter().any(|item| item.blocking),
        "preparation blocked; no external invocation was made; evidence at {}",
        evidence_root.display(),
    );
    assert!(matches!(
        prepared.route,
        Some(WorkflowRoute::Agent {
            agent: AgentKind::Claude,
            ..
        })
    ));
    let outcome = state
        .with_current_project_task_access(&context.project_id, root.to_str().unwrap(), |permit| {
            state.workflow_service.enqueue_with_acknowledgements(
                permit,
                &state.settings_service,
                &state.secret_service,
                &state.agent_service,
                &state.task_service,
                &prepared.preparation_id,
                &prepared.preparation_revision,
                false,
                false,
                None,
            )
        })
        .unwrap();
    let WorkflowStartOutcome::Created { run } = outcome else {
        panic!("expected new run");
    };
    let wiki_before = CompileService::snapshot_wiki(&context).unwrap();
    let head_before = state.git_service.repository_status(&context).unwrap().head;
    let cancel_after_tool =
        std::env::var("LLM_WIKI_REAL_CLAUDE_CANCEL_AFTER_TOOL").as_deref() == Ok("1");
    let (stop_cancel_watch, stop_cancel_receiver) = std::sync::mpsc::channel::<()>();
    let cancel_watch = cancel_after_tool.then(|| {
        let tasks = state.task_service.clone();
        let task_id = run.task_id.clone();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            while started.elapsed() < std::time::Duration::from_secs(180) {
                if tasks
                    .get_activities(&task_id)
                    .unwrap()
                    .iter()
                    .any(|activity| {
                        matches!(activity, crate::models::task::TaskActivity::ToolCall { .. })
                    })
                {
                    tasks.request_cancel(&task_id).unwrap();
                    return true;
                }
                if stop_cancel_receiver
                    .recv_timeout(std::time::Duration::from_millis(50))
                    .is_ok()
                {
                    return false;
                }
            }
            tasks.request_cancel(&task_id).unwrap();
            false
        })
    });
    let services = UpdateWikiExecutionServices {
        compile: CompileExecutionServices {
            agent_service: &state.agent_service,
            llm_service: &state.llm_service,
            secret_service: &state.secret_service,
            settings_service: &state.settings_service,
            task_service: &state.task_service,
        },
        git_service: &state.git_service,
        file_store: &state.file_store,
        bookmark_service: &state.bookmark_service,
        search_service: &state.search_service,
        confirmation_registry: &state.confirmation_registry,
        coordinator: &state.workflow_service.coordinator,
    };
    run_update_wiki_authorized(&context, run.clone(), &services, || {
        state.publish_workflow_external_launch(&context, &run)
    })
    .await;
    let _ = stop_cancel_watch.send(());
    let cancelled_after_tool = cancel_watch.map(|watch| watch.join().unwrap());
    let generated = state.task_service.get_workflow_run(&run.task_id).unwrap();
    fs::write(
        evidence_root.join("generated.json"),
        serde_json::to_vec_pretty(&generated).unwrap(),
    )
    .unwrap();
    if cancel_after_tool {
        assert_eq!(
            cancelled_after_tool,
            Some(true),
            "must observe a real tool call before cancellation"
        );
        assert_eq!(generated.display_status, WorkflowDisplayStatus::Cancelled);
        assert_eq!(
            CompileService::snapshot_wiki(&context).unwrap(),
            wiki_before
        );
        assert_eq!(
            state.git_service.repository_status(&context).unwrap().head,
            head_before
        );
        for path in [source_path, extracted_path] {
            assert_eq!(fs::read_to_string(root.join(path)).unwrap(), source);
        }
        assert!(generated.result.is_none());
        println!("real_workflow_cancelled_after_tool=true task={} source_unchanged=true wiki_unchanged=true checkpoint_unchanged=true", run.task_id);
        return;
    }
    if generated.display_status == WorkflowDisplayStatus::WaitingForConfirmation {
        let review = update_wiki_decision_review(&run.task_id, &root).unwrap();
        fs::write(
            evidence_root.join("review.json"),
            serde_json::to_vec_pretty(&review).unwrap(),
        )
        .unwrap();
        // The caller explicitly authorizes the complete synthetic acceptance
        // cycle; ordinary users still review and confirm through typed IPC.
        state
            .with_current_project_write_access(
                &context.project_id,
                root.to_str().unwrap(),
                |_, _| {
                    confirm_update_wiki_review(&context, &run.task_id, &services)
                        .map(|_| ())
                        .map_err(|failure| failure.error)
                },
            )
            .unwrap();
    }
    let completed = state.task_service.get_workflow_run(&run.task_id).unwrap();
    fs::write(
        evidence_root.join("completed.json"),
        serde_json::to_vec_pretty(&completed).unwrap(),
    )
    .unwrap();
    assert_eq!(
        completed.display_status,
        WorkflowDisplayStatus::Completed,
        "task failed with code {:?}; evidence at {}",
        completed.error.as_ref().map(|error| &error.code),
        evidence_root.display()
    );
    assert!(completed.error.is_none(), "unexpected post-apply error");
    let WorkflowResult::UpdateWiki {
        affected_paths,
        checkpoint_hash: Some(checkpoint),
        final_commit: Some(commit),
        ..
    } = completed.result.unwrap()
    else {
        panic!("expected checked and committed Wiki result");
    };
    assert!(!affected_paths.is_empty());
    for path in [source_path, extracted_path] {
        assert_eq!(fs::read_to_string(root.join(path)).unwrap(), source);
        assert_eq!(
            GitService::file_at_checkpoint(&context, &checkpoint, path)
                .unwrap()
                .as_deref(),
            Some(source)
        );
    }
    let index = state
        .search_service
        .scan_wiki(&context, &HashSet::new())
        .unwrap();
    let opened = affected_paths
        .iter()
        .filter(|path| root.join(path).is_file())
        .collect::<Vec<_>>();
    assert!(!opened.is_empty());
    for path in &opened {
        assert!(!state
            .file_store
            .read_markdown(&context, path)
            .unwrap()
            .is_empty());
        assert!(index.pages.iter().any(|page| &page.path == *path));
    }
    println!("real_workflow_route=claude task={} applied={} opened={} indexed={} checkpoint={} final_commit={}", run.task_id, affected_paths.len(), opened.len(), index.pages.len(), checkpoint, commit);
}
