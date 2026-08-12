#[path = "support/workflow_baseline.rs"]
mod workflow_baseline;

use llm_wiki_desktop_lib::models::agent::AgentKind;
use llm_wiki_desktop_lib::models::confirmation::{PendingActionType, RiskLevel};
use llm_wiki_desktop_lib::models::lint::WikiLintSkillRef;
use llm_wiki_desktop_lib::models::task::{BackendEventType, TaskStatus, TaskType};
use llm_wiki_desktop_lib::models::workflow::{
    HealthCheckMode, UpdateWikiMode, WorkflowCandidateReference, WorkflowDisplayStatus,
    WorkflowExecutionOptions, WorkflowKind, WorkflowOperation, WorkflowPendingAction,
    WorkflowRoute, WorkflowScope, WorkflowStage, WorkflowStageStatus, WorkflowStartOutcome,
    WORKFLOW_SCHEMA_VERSION,
};
use llm_wiki_desktop_lib::services::{project_identity, EnqueueWorkflow, WorkflowCoordinator};
use llm_wiki_desktop_lib::tasks::task_events::EventBus;
use llm_wiki_desktop_lib::tasks::TaskService;
use std::sync::Arc;
use workflow_baseline::{controlled_race, RacePoint};

fn stage() -> WorkflowStage {
    WorkflowStage {
        id: "apply".into(),
        ordinal: 1,
        status: WorkflowStageStatus::Pending,
        label_key: "apply".into(),
        started_at: None,
        completed_at: None,
        current_item: None,
        progress: None,
        decision: None,
    }
}

fn request(root: &std::path::Path, baseline: &str) -> EnqueueWorkflow {
    EnqueueWorkflow {
        project_id: "project-中文".into(),
        project_root: root.to_path_buf(),
        task_state_root: Some(root.join(".app/tasks")),
        title: "Update Wiki".into(),
        kind: WorkflowKind::UpdateWiki,
        scope: WorkflowScope::UpdateWiki {
            mode: UpdateWikiMode::ChangedSources,
            source_versions: Vec::new(),
        },
        route: None,
        baseline_fingerprint: baseline.into(),
        execution_options: WorkflowExecutionOptions {
            preparation_revision: "prep-1".into(),
            ..WorkflowExecutionOptions::default()
        },
        stages: vec![stage()],
        retry: None,
    }
}

fn created(outcome: WorkflowStartOutcome) -> llm_wiki_desktop_lib::models::workflow::WorkflowRun {
    match outcome {
        WorkflowStartOutcome::Created { run } => run,
        WorkflowStartOutcome::Existing { .. } => panic!("expected created"),
    }
}

fn agent_repair_request(root: &std::path::Path, baseline: &str) -> EnqueueWorkflow {
    let mut request = request(root, baseline);
    request.kind = WorkflowKind::HealthCheck;
    request.scope = WorkflowScope::HealthCheck {
        mode: HealthCheckMode::Complete,
    };
    request.route = Some(WorkflowRoute::Agent {
        agent: AgentKind::Codex,
        model: Some("gpt-5".into()),
        route_revision: "route-1".into(),
    });
    request.execution_options.operation = WorkflowOperation::AgentLintRepair {
        preparation_id: "prepare-repair".into(),
        preparation_revision: "prepare-revision-repair".into(),
        report_id: "health-report-1".into(),
        selection_revision: "selection-revision-repair".into(),
        selected_finding_ids: vec!["duplicate_topic:wiki/a.md".into()],
        skill: WikiLintSkillRef::builtin(),
        authorized_path_hashes: [("wiki/a.md".into(), Some("a".repeat(64)))]
            .into_iter()
            .collect(),
        expected_git_head: "b".repeat(40),
    };
    request
}

#[test]
fn recovery_migrates_schema_v1_to_built_in_without_trusting_smuggled_operation() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let run = created(
        WorkflowCoordinator::default()
            .enqueue(&service, request(temp.path(), "migration-v1"))
            .unwrap(),
    );
    let path = temp
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", run.task_id));
    let mut persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    persisted["workflow"]["schemaVersion"] = serde_json::json!(1);
    persisted["workflow"]["executionOptions"]["operation"] = serde_json::json!({
        "kind": "agent_lint_repair",
        "preparationId": "smuggled",
        "preparationRevision": "smuggled",
        "reportId": "smuggled",
        "selectionRevision": "smuggled",
        "selectedFindingIds": ["duplicate_topic:wiki/a.md"],
        "skill": {
            "id": "builtin.wiki-lint",
            "version": "2026-08-12.1",
            "sha256": "29e903710745451da287de9d08297ae6863de944bd7d9abd7f4243b5b9f76eb0"
        },
        "authorizedPathHashes": { "wiki/a.md": null },
        "expectedGitHead": "abc"
    });
    let preserved_scope = persisted["workflow"]["scope"].clone();
    let preserved_fingerprint = persisted["workflow"]["fingerprint"].clone();
    std::fs::write(&path, serde_json::to_vec_pretty(&persisted).unwrap()).unwrap();

    let restarted = TaskService::default();
    restarted.recover_tasks(temp.path()).unwrap();
    let recovered = restarted.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(recovered.schema_version, WORKFLOW_SCHEMA_VERSION);
    assert_eq!(recovered.operation, WorkflowOperation::BuiltIn);
    let rewritten: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(rewritten["workflow"]["schemaVersion"], 2);
    assert_eq!(
        rewritten["workflow"]["executionOptions"]["operation"]["kind"],
        "built_in"
    );
    assert_eq!(rewritten["workflow"]["scope"], preserved_scope);
    assert_eq!(rewritten["workflow"]["fingerprint"], preserved_fingerprint);
}

#[test]
fn recovery_rejects_unknown_future_workflow_schema() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let run = created(
        WorkflowCoordinator::default()
            .enqueue(&service, request(temp.path(), "future-schema"))
            .unwrap(),
    );
    let path = temp
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", run.task_id));
    let mut persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    persisted["workflow"]["schemaVersion"] = serde_json::json!(99);
    std::fs::write(&path, serde_json::to_vec_pretty(&persisted).unwrap()).unwrap();

    let restarted = TaskService::default();
    assert!(restarted.recover_tasks(temp.path()).unwrap().is_empty());
    assert!(restarted.get_workflow_run(&run.task_id).is_none());
    let untouched: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(untouched["workflow"]["schemaVersion"], 99);
}

#[test]
fn recovery_rejects_invalid_agent_repair_pairings_and_future_operation_authority() {
    let temp = tempfile::tempdir().unwrap();
    for case in [
        "wrong-kind",
        "wrong-scope",
        "wrong-route",
        "future-authority",
    ] {
        let root = temp.path().join(case);
        std::fs::create_dir_all(&root).unwrap();
        let service = TaskService::default();
        let run = created(
            WorkflowCoordinator::default()
                .enqueue(&service, agent_repair_request(&root, case))
                .unwrap(),
        );
        let path = root
            .join(".app/tasks")
            .join(format!("{}.json", run.task_id));
        let mut persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        match case {
            "wrong-kind" => persisted["workflow"]["kind"] = serde_json::json!("update_wiki"),
            "wrong-scope" => {
                persisted["workflow"]["scope"] = serde_json::json!({
                    "kind": "health_check",
                    "mode": "local_quick"
                });
            }
            "wrong-route" => {
                persisted["workflow"]["route"] = serde_json::json!({
                    "kind": "local",
                    "routeRevision": "route-1"
                });
            }
            "future-authority" => {
                persisted["workflow"]["executionOptions"]["operation"]["futureAuthorization"] =
                    serde_json::json!(true);
            }
            _ => unreachable!(),
        }
        let tampered_bytes = serde_json::to_vec_pretty(&persisted).unwrap();
        std::fs::write(&path, &tampered_bytes).unwrap();

        let restarted = TaskService::default();
        assert!(restarted.recover_tasks(&root).unwrap().is_empty(), "{case}");
        assert!(restarted.get_workflow_run(&run.task_id).is_none(), "{case}");
        assert_eq!(std::fs::read(&path).unwrap(), tampered_bytes, "{case}");
    }
}

#[test]
fn restart_interrupts_running_and_holds_queued_until_explicit_continuation() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("CJK-知识库");
    std::fs::create_dir_all(&root).unwrap();
    let coordinator = WorkflowCoordinator::default();
    let service = TaskService::default();
    let running = created(
        coordinator
            .enqueue(&service, request(&root, "base-1"))
            .unwrap(),
    );
    service
        .start_workflow_stage(&running.task_id, "apply")
        .unwrap();
    let queued = created(
        coordinator
            .enqueue(&service, request(&root, "base-2"))
            .unwrap(),
    );
    drop(service);

    let restarted = TaskService::default();
    let snapshot = restarted.set_project_root(Some(root.clone())).unwrap();
    assert_eq!(snapshot.len(), 2);
    let interrupted = restarted.get_workflow_run(&running.task_id).unwrap();
    assert_eq!(
        interrupted.display_status,
        WorkflowDisplayStatus::Interrupted
    );
    assert_eq!(interrupted.current_stage_id.as_deref(), Some("apply"));
    let recovered_queue = restarted.get_workflow_run(&queued.task_id).unwrap();
    assert_eq!(
        recovered_queue.display_status,
        WorkflowDisplayStatus::Queued
    );
    assert!(recovered_queue.continuation_required);

    let identity = project_identity(&root).unwrap();
    let (continued, claimed) = coordinator
        .continue_queued(
            &restarted,
            &identity.canonical_identity_key,
            &identity.identity_revision,
        )
        .unwrap();
    assert_eq!(
        claimed.as_ref().map(|run| run.task_id.as_str()),
        Some(queued.task_id.as_str())
    );
    assert!(continued.iter().any(|run| {
        run.task_id == queued.task_id && run.display_status == WorkflowDisplayStatus::Running
    }));
}

#[test]
fn worker_finish_after_cancel_uses_a_deterministic_recovery_window() {
    let temp = tempfile::tempdir().unwrap();
    let service = Arc::new(TaskService::default());
    let coordinator = Arc::new(WorkflowCoordinator::default());
    let run = created(
        coordinator
            .enqueue(&service, request(temp.path(), "finish-window"))
            .unwrap(),
    );
    service.start_workflow_stage(&run.task_id, "apply").unwrap();
    let (controller, worker_window) = controlled_race();
    let worker_service = service.clone();
    let worker_coordinator = coordinator.clone();
    let task_id = run.task_id.clone();
    let worker = std::thread::spawn(move || {
        worker_window.pause_at(RacePoint::WorkerFinish);
        worker_coordinator.complete_and_claim_next(
            &worker_service,
            &task_id,
            llm_wiki_desktop_lib::models::workflow::WorkflowResult::UpdateWiki {
                created: 0,
                updated: 0,
                skipped: 0,
                deleted: 0,
                conflicted: 0,
                affected_paths: Vec::new(),
                checkpoint_hash: None,
                final_commit: None,
            },
        )
    });

    controller.wait_for(RacePoint::WorkerFinish);
    coordinator.cancel(&service, &run.task_id).unwrap();
    controller.release();

    let (finalized, next) = worker
        .join()
        .unwrap()
        .expect("worker completion must resolve a concurrent cancellation");
    assert_eq!(finalized.display_status, WorkflowDisplayStatus::Cancelled);
    assert!(next.is_none());
    assert_eq!(
        service.get_task(&run.task_id).unwrap().status,
        TaskStatus::Cancelled,
        "worker completion must not leave a cancellation suspended",
    );
}

#[test]
fn cancelled_initial_approval_cannot_leave_a_recoverable_queued_run() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = WorkflowCoordinator::default();
    let service = TaskService::default();
    let _active = created(
        coordinator
            .enqueue(&service, request(temp.path(), "active-before-repair"))
            .unwrap(),
    );
    let queued = created(
        coordinator
            .enqueue(&service, request(temp.path(), "cancelled-repair"))
            .unwrap(),
    );
    assert_eq!(queued.display_status, WorkflowDisplayStatus::Queued);

    let (cancelled, next) = coordinator
        .cancel_created_without_undo_and_claim_next(&service, &queued.task_id)
        .unwrap();
    assert_eq!(cancelled.display_status, WorkflowDisplayStatus::Cancelled);
    assert!(!cancelled.continuation_required);
    assert!(next.is_none());
    assert!(coordinator.undo_cancel(&service, &queued.task_id).is_err());
    drop(service);

    let restarted = TaskService::default();
    restarted.recover_tasks(temp.path()).unwrap();
    let recovered = restarted.get_workflow_run(&queued.task_id).unwrap();
    assert_eq!(recovered.display_status, WorkflowDisplayStatus::Cancelled);
    assert!(!recovered.continuation_required);
    assert!(coordinator
        .undo_cancel(&restarted, &queued.task_id)
        .is_err());
}

#[test]
fn waiting_confirmation_survives_only_with_valid_reconstruction_data() {
    for (affected_path, candidate_exists, expires_at, expected) in [
        (
            "wiki/安全.md",
            true,
            None,
            WorkflowDisplayStatus::WaitingForConfirmation,
        ),
        (
            "../escape.md",
            true,
            None,
            WorkflowDisplayStatus::Interrupted,
        ),
        (
            "wiki/安全.md",
            false,
            None,
            WorkflowDisplayStatus::Interrupted,
        ),
        (
            "wiki/安全.md",
            true,
            Some("not-a-timestamp"),
            WorkflowDisplayStatus::Interrupted,
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let candidate_path = temp.path().join(".app/candidates/candidate-1.json");
        if candidate_exists {
            std::fs::create_dir_all(candidate_path.parent().unwrap()).unwrap();
            std::fs::write(&candidate_path, "candidate").unwrap();
        }
        let service = TaskService::default();
        let coordinator = WorkflowCoordinator::default();
        let run = created(
            coordinator
                .enqueue(&service, request(temp.path(), affected_path))
                .unwrap(),
        );
        service.start_workflow_stage(&run.task_id, "apply").unwrap();
        service
            .wait_workflow_stage(
                &run.task_id,
                "apply",
                WorkflowPendingAction {
                    id: "decision-1".into(),
                    action_type: PendingActionType::MergeConflict,
                    risk_level: RiskLevel::High,
                    affected_paths: vec![affected_path.into()],
                    candidate: Some(WorkflowCandidateReference::ProjectRelative {
                        path: ".app/candidates/candidate-1.json".into(),
                    }),
                    expires_at: expires_at.map(str::to_string),
                    checkpoint_hash: None,
                },
            )
            .unwrap();
        drop(service);

        let restarted = TaskService::default();
        restarted.recover_tasks(temp.path()).unwrap();
        assert_eq!(
            restarted
                .get_workflow_run(&run.task_id)
                .unwrap()
                .display_status,
            expected
        );
    }
}

#[test]
fn reopening_the_same_root_rebinds_runtime_project_id_before_events() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = WorkflowCoordinator::default();
    let original = TaskService::default();
    let run = created(
        coordinator
            .enqueue(&original, request(temp.path(), "base"))
            .unwrap(),
    );
    drop(original);

    let (event_bus, events) = EventBus::new_test_capture();
    let restarted = TaskService::with_event_bus(event_bus);
    let snapshot = restarted
        .set_project_context(
            "new-runtime-project-id".into(),
            temp.path().to_path_buf(),
            temp.path().join(".app/tasks"),
        )
        .unwrap();
    assert_eq!(
        snapshot[0].project_id.as_deref(),
        Some("new-runtime-project-id")
    );
    assert_eq!(
        restarted.get_workflow_run(&run.task_id).unwrap().project_id,
        "new-runtime-project-id"
    );
    assert!(events.lock().unwrap().iter().any(|event| {
        event.event_type == BackendEventType::WorkflowUpdated
            && event.project_id.as_deref() == Some("new-runtime-project-id")
    }));
}

#[test]
fn same_process_reopen_rebinds_but_cross_root_task_id_collision_fails_closed() {
    let root_a = tempfile::tempdir().unwrap();
    let root_b = tempfile::tempdir().unwrap();
    let coordinator = WorkflowCoordinator::default();
    let service = TaskService::default();
    let run = created(
        coordinator
            .enqueue(&service, request(root_a.path(), "base"))
            .unwrap(),
    );
    let rebound = service
        .set_project_context(
            "new-runtime-id".into(),
            root_a.path().to_path_buf(),
            root_a.path().join(".app/tasks"),
        )
        .unwrap();
    assert_eq!(rebound[0].project_id.as_deref(), Some("new-runtime-id"));
    assert_eq!(
        service.get_workflow_run(&run.task_id).unwrap().project_id,
        "new-runtime-id"
    );

    let source = root_a
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", run.task_id));
    let target_dir = root_b.path().join(".app/tasks");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::copy(source, target_dir.join(format!("{}.json", run.task_id))).unwrap();
    assert!(service
        .set_project_context("project-b".into(), root_b.path().to_path_buf(), target_dir,)
        .is_err());
}

#[test]
fn legacy_non_workflow_recovery_behavior_remains_compatible() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    service
        .set_project_root(Some(temp.path().to_path_buf()))
        .unwrap();
    let legacy = service.create_task(TaskType::Import, Some("p".into()), "Import".into(), true);
    assert_eq!(legacy.status, TaskStatus::Queued);
    drop(service);

    let restarted = TaskService::default();
    restarted.recover_tasks(temp.path()).unwrap();
    let recovered = restarted.get_task(&legacy.id).unwrap();
    assert_eq!(recovered.status, TaskStatus::Failed);
    assert_eq!(recovered.error.unwrap().code, "TASK_RECOVERY");
}

#[test]
fn legacy_v1_wrapper_migrates_and_current_wrapper_is_accepted() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = WorkflowCoordinator::default();
    let service = TaskService::default();
    let run = created(
        coordinator
            .enqueue(&service, request(temp.path(), "base"))
            .unwrap(),
    );
    let path = temp
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", run.task_id));
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(value["schemaVersion"], 2);
    assert_eq!(value["workflow"]["schemaVersion"], 2);
    assert_eq!(
        value["workflow"]["executionOptions"]["operation"]["kind"],
        "built_in"
    );
    value["schemaVersion"] = 1.into();
    std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    drop(service);

    let restarted = TaskService::default();
    restarted.recover_tasks(temp.path()).unwrap();
    assert!(restarted.get_workflow_run(&run.task_id).is_some());
    let migrated: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(migrated["schemaVersion"], 2);
    assert_eq!(migrated["workflow"]["schemaVersion"], 2);
    assert_eq!(
        migrated["workflow"]["executionOptions"]["operation"]["kind"],
        "built_in"
    );
}

#[test]
fn future_wrapper_and_workflow_schemas_are_skipped_without_rewriting_bytes() {
    for nested_workflow in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let coordinator = WorkflowCoordinator::default();
        let service = TaskService::default();
        let run = created(
            coordinator
                .enqueue(&service, request(temp.path(), "base"))
                .unwrap(),
        );
        let path = temp
            .path()
            .join(".app/tasks")
            .join(format!("{}.json", run.task_id));
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        if nested_workflow {
            value["workflow"]["schemaVersion"] = 999.into();
        } else {
            value["schemaVersion"] = 999.into();
        }
        let original_bytes = serde_json::to_vec_pretty(&value).unwrap();
        std::fs::write(&path, &original_bytes).unwrap();
        drop(service);

        let restarted = TaskService::default();
        assert!(restarted.recover_tasks(temp.path()).unwrap().is_empty());
        assert!(restarted.get_workflow_run(&run.task_id).is_none());
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    }
}

#[test]
fn malformed_wrapper_never_falls_back_to_a_valid_raw_task_shape() {
    let temp = tempfile::tempdir().unwrap();
    let coordinator = WorkflowCoordinator::default();
    let service = TaskService::default();
    let run = created(
        coordinator
            .enqueue(&service, request(temp.path(), "base"))
            .unwrap(),
    );
    let path = temp
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", run.task_id));
    let wrapper: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut raw_task = wrapper["task"].as_object().unwrap().clone();
    raw_task.insert("task".into(), serde_json::Value::String("malformed".into()));
    raw_task.insert("schemaVersion".into(), 2.into());
    let original_bytes = serde_json::to_vec_pretty(&raw_task).unwrap();
    std::fs::write(&path, &original_bytes).unwrap();
    drop(service);

    let restarted = TaskService::default();
    assert!(restarted.recover_tasks(temp.path()).unwrap().is_empty());
    assert!(restarted.get_task(&run.task_id).is_none());
    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
}
