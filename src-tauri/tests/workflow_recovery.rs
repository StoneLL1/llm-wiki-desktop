use llm_wiki_desktop_lib::models::confirmation::{PendingActionType, RiskLevel};
use llm_wiki_desktop_lib::models::task::{BackendEventType, TaskStatus, TaskType};
use llm_wiki_desktop_lib::models::workflow::{
    UpdateWikiMode, WorkflowCandidateReference, WorkflowDisplayStatus, WorkflowExecutionOptions,
    WorkflowKind, WorkflowPendingAction, WorkflowScope, WorkflowStage, WorkflowStageStatus,
    WorkflowStartOutcome,
};
use llm_wiki_desktop_lib::services::{project_identity, EnqueueWorkflow, WorkflowCoordinator};
use llm_wiki_desktop_lib::tasks::task_events::EventBus;
use llm_wiki_desktop_lib::tasks::TaskService;

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
    let continued = coordinator
        .continue_queued(
            &restarted,
            &identity.canonical_identity_key,
            &identity.identity_revision,
        )
        .unwrap();
    assert!(continued.iter().any(|run| {
        run.task_id == queued.task_id && run.display_status == WorkflowDisplayStatus::Running
    }));
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
