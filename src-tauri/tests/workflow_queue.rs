#[path = "support/workflow_baseline.rs"]
mod workflow_baseline;

use llm_wiki_desktop_lib::models::agent::AgentKind;
use llm_wiki_desktop_lib::models::confirmation::{PendingActionType, RiskLevel};
use llm_wiki_desktop_lib::models::task::{BackendEventType, TaskStatus};
use llm_wiki_desktop_lib::models::workflow::{
    HealthCheckMode, UpdateWikiMode, WorkflowArtifactType, WorkflowDisplayStatus,
    WorkflowExecutionOptions, WorkflowKind, WorkflowPendingAction, WorkflowPersistenceMode,
    WorkflowPersistenceTransition, WorkflowProjectMutationState, WorkflowResult, WorkflowRoute,
    WorkflowScope, WorkflowSourceVersionRef, WorkflowStage, WorkflowStageStatus,
    WorkflowStartOutcome,
};
use llm_wiki_desktop_lib::services::{
    project_identity, EnqueueWorkflow, WorkflowCoordinator, WorkflowDispatchFailure,
    WorkflowService,
};
use llm_wiki_desktop_lib::tasks::task_events::EventBus;
use llm_wiki_desktop_lib::tasks::TaskService;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use workflow_baseline::{controlled_race, RacePoint};

fn stage() -> WorkflowStage {
    WorkflowStage {
        id: "prepare".into(),
        ordinal: 1,
        status: WorkflowStageStatus::Pending,
        label_key: "workflows.stage.prepare".into(),
        started_at: None,
        completed_at: None,
        current_item: None,
        progress: None,
        decision: None,
    }
}

#[test]
fn unavailable_runner_finalizes_claim_and_does_not_strand_the_owner_queue() {
    let temp = tempfile::tempdir().unwrap();
    for kind in [
        WorkflowKind::UpdateWiki,
        WorkflowKind::HealthCheck,
        WorkflowKind::GenerateContent,
    ] {
        let root = temp.path().join(format!("{kind:?}"));
        std::fs::create_dir_all(&root).unwrap();
        let tasks = TaskService::default();
        let workflows = WorkflowService::default();
        let request_for = |version: &str, baseline: &str| {
            let mut candidate = request(&root, "p", version, baseline);
            candidate.kind = kind.clone();
            candidate.scope = match kind {
                WorkflowKind::UpdateWiki => candidate.scope,
                WorkflowKind::HealthCheck => WorkflowScope::HealthCheck {
                    mode: HealthCheckMode::Complete,
                },
                WorkflowKind::GenerateContent => WorkflowScope::GenerateContent {
                    artifact_type: WorkflowArtifactType::ProjectReport,
                    page_paths: vec!["wiki/page.md".into()],
                    output_path: Some("exports/report.html".into()),
                },
            };
            candidate
        };
        let active = created(
            workflows
                .coordinator
                .enqueue(&tasks, request_for("v1", "missing-runner"))
                .unwrap(),
        );
        let queued = created(
            workflows
                .coordinator
                .enqueue(&tasks, request_for("v2", "next-runner"))
                .unwrap(),
        );

        assert!(!workflows.dispatch_claimed_run(&tasks, &active).unwrap());
        assert_eq!(
            tasks
                .get_workflow_run(&active.task_id)
                .unwrap()
                .display_status,
            WorkflowDisplayStatus::Failed
        );
        assert_eq!(
            tasks
                .get_workflow_run(&queued.task_id)
                .unwrap()
                .display_status,
            WorkflowDisplayStatus::Failed,
            "the shared finalizer must claim and reject each runner kind exactly once"
        );
    }
}

fn request(root: &Path, project_id: &str, version: &str, baseline: &str) -> EnqueueWorkflow {
    EnqueueWorkflow {
        project_id: project_id.into(),
        project_root: root.to_path_buf(),
        task_state_root: Some(root.join(".app/tasks")),
        title: "Update Wiki".into(),
        kind: WorkflowKind::UpdateWiki,
        scope: WorkflowScope::UpdateWiki {
            mode: UpdateWikiMode::ChangedSources,
            source_versions: vec![WorkflowSourceVersionRef {
                source_id: "来源-一".into(),
                version_id: version.into(),
            }],
        },
        route: Some(WorkflowRoute::Agent {
            agent: AgentKind::Codex,
            model: Some("gpt-5".into()),
            route_revision: "route-1".into(),
        }),
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
        WorkflowStartOutcome::Existing { .. } => panic!("expected a new workflow"),
    }
}

#[test]
fn enqueue_for_owner_rejects_same_path_identity_replacement() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("同路径项目");
    let old_root = parent.path().join("旧项目实体");
    std::fs::create_dir_all(&root).unwrap();
    let expected = project_identity(&root).unwrap();
    std::fs::rename(&root, &old_root).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let replacement = project_identity(&root).unwrap();
    assert_ne!(expected.identity_revision, replacement.identity_revision);

    let tasks = TaskService::default();
    let error = WorkflowCoordinator::default()
        .enqueue_for_owner(
            &tasks,
            request(&root, "runtime-replacement", "v1", "owner-guard"),
            &expected.canonical_identity_key,
            &expected.identity_revision,
        )
        .unwrap_err();

    assert!(error.contains("identity changed"));
    assert!(tasks.list_workflow_runs().is_empty());
}

fn empty_update_wiki_result() -> WorkflowResult {
    WorkflowResult::UpdateWiki {
        created: 0,
        updated: 0,
        skipped: 0,
        deleted: 0,
        conflicted: 0,
        affected_paths: Vec::new(),
        checkpoint_hash: None,
        final_commit: None,
    }
}

#[test]
fn deduplicates_only_matching_active_fingerprints_and_serializes_one_project() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();

    let first = created(
        coordinator
            .enqueue(&service, request(temp.path(), "project-a", "v1", "base-1"))
            .unwrap(),
    );
    assert_eq!(first.display_status, WorkflowDisplayStatus::Running);
    let duplicate = coordinator
        .enqueue(&service, request(temp.path(), "project-a", "v1", "base-1"))
        .unwrap();
    match duplicate {
        WorkflowStartOutcome::Existing { run } => assert_eq!(run.task_id, first.task_id),
        WorkflowStartOutcome::Created { .. } => panic!("duplicate workflow was created"),
    }

    let changed_range = created(
        coordinator
            .enqueue(&service, request(temp.path(), "project-a", "v2", "base-1"))
            .unwrap(),
    );
    let changed_baseline = created(
        coordinator
            .enqueue(&service, request(temp.path(), "project-a", "v1", "base-2"))
            .unwrap(),
    );
    let mut changed_route_request = request(temp.path(), "project-a", "v1", "base-1");
    changed_route_request.route = Some(WorkflowRoute::Agent {
        agent: AgentKind::Claude,
        model: None,
        route_revision: "route-2".into(),
    });
    let changed_route = created(
        coordinator
            .enqueue(&service, changed_route_request)
            .unwrap(),
    );
    let mut changed_options_request = request(temp.path(), "project-a", "v1", "base-1");
    changed_options_request
        .execution_options
        .preparation_revision = "prep-2".into();
    match coordinator
        .enqueue(&service, changed_options_request.clone())
        .unwrap()
    {
        WorkflowStartOutcome::Existing { run } => assert_eq!(run.task_id, first.task_id),
        WorkflowStartOutcome::Created { .. } => {
            panic!("preparation revision must not change execution identity")
        }
    }
    changed_options_request
        .execution_options
        .existing_target_hash = Some("a".repeat(64));
    let changed_execution_options = created(
        coordinator
            .enqueue(&service, changed_options_request)
            .unwrap(),
    );
    let mut beautiful_read_request = request(temp.path(), "project-a", "v1", "base-1");
    beautiful_read_request.kind = WorkflowKind::GenerateContent;
    beautiful_read_request.scope = WorkflowScope::GenerateContent {
        artifact_type: WorkflowArtifactType::BeautifulRead,
        page_paths: vec!["wiki/page.md".into()],
        output_path: Some("exports/page.html".into()),
    };
    let beautiful_read = created(
        coordinator
            .enqueue(&service, beautiful_read_request.clone())
            .unwrap(),
    );
    beautiful_read_request.scope = WorkflowScope::GenerateContent {
        artifact_type: WorkflowArtifactType::KnowledgeCard,
        page_paths: vec!["wiki/page.md".into()],
        output_path: Some("exports/page.html".into()),
    };
    let changed_output_type = created(
        coordinator
            .enqueue(&service, beautiful_read_request)
            .unwrap(),
    );

    for run in [
        &changed_range,
        &changed_baseline,
        &changed_route,
        &changed_execution_options,
        &beautiful_read,
        &changed_output_type,
    ] {
        assert_eq!(run.display_status, WorkflowDisplayStatus::Queued);
        assert_ne!(run.task_id, first.task_id);
    }
    assert_eq!(
        service
            .list_workflow_runs()
            .iter()
            .filter(|run| run.display_status == WorkflowDisplayStatus::Running)
            .count(),
        1
    );
}

#[cfg(windows)]
#[test]
fn case_only_windows_paths_share_project_identity() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("CaseSensitiveIdentity");
    std::fs::create_dir_all(&root).unwrap();
    let case_alias = PathBuf::from(root.to_string_lossy().to_uppercase());
    if !case_alias.exists() {
        return;
    }

    let direct = project_identity(&root).unwrap();
    let differently_cased = project_identity(&case_alias).unwrap();
    assert_eq!(
        direct.canonical_identity_key,
        differently_cased.canonical_identity_key
    );
    assert_eq!(
        direct.identity_revision,
        differently_cased.identity_revision
    );
}

#[test]
fn separate_projects_run_independently_and_task_snapshots_do_not_leak() {
    let parent = tempfile::tempdir().unwrap();
    let root_a = parent.path().join("项目-A");
    let root_b = parent.path().join("项目-B");
    std::fs::create_dir_all(&root_a).unwrap();
    std::fs::create_dir_all(&root_b).unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();

    let a = created(
        coordinator
            .enqueue(&service, request(&root_a, "a", "v1", "a"))
            .unwrap(),
    );
    let b = created(
        coordinator
            .enqueue(&service, request(&root_b, "b", "v1", "b"))
            .unwrap(),
    );
    assert_eq!(a.display_status, WorkflowDisplayStatus::Running);
    assert_eq!(b.display_status, WorkflowDisplayStatus::Running);
    assert_eq!(service.list_tasks_for_root(&root_a, None).len(), 1);
    assert_eq!(service.list_tasks_for_root(&root_b, None).len(), 1);
    assert_eq!(service.list_tasks_for_root(&root_a, None)[0].id, a.task_id);
    assert!(!service.task_belongs_to_root(&a.task_id, &root_b));
}

#[test]
fn canonical_aliases_share_identity_queue_and_fingerprint() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("知识库");
    let alias = parent.path().join("alias");
    std::fs::create_dir_all(&root).unwrap();
    if !make_directory_alias(&root, &alias) {
        return;
    }
    let direct = project_identity(&root).unwrap();
    let linked = project_identity(&alias).unwrap();
    assert_eq!(direct.canonical_identity_key, linked.canonical_identity_key);
    assert_eq!(direct.identity_revision, linked.identity_revision);

    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let first = created(
        coordinator
            .enqueue(&service, request(&root, "p", "v1", "base"))
            .unwrap(),
    );
    match coordinator
        .enqueue(&service, request(&alias, "p", "v1", "base"))
        .unwrap()
    {
        WorkflowStartOutcome::Existing { run } => assert_eq!(run.task_id, first.task_id),
        WorkflowStartOutcome::Created { .. } => panic!("alias created a second queue owner"),
    }
}

#[test]
fn task_state_symlink_outside_project_is_rejected() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("project");
    let external = parent.path().join("external-state");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&external).unwrap();
    if !make_directory_alias(&external, &root.join(".app")) {
        return;
    }
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    assert!(coordinator
        .enqueue(&service, request(&root, "p", "v1", "base"))
        .is_err());
    assert!(!external.join("tasks").exists());
}

#[cfg(unix)]
fn make_directory_alias(root: &Path, alias: &Path) -> bool {
    std::os::unix::fs::symlink(root, alias).is_ok()
}

#[cfg(windows)]
fn make_directory_alias(root: &Path, alias: &Path) -> bool {
    std::os::windows::fs::symlink_dir(root, alias).is_ok()
}

#[test]
fn queued_cancel_and_undo_are_idempotent_and_retry_links_a_new_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let first = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    let queued = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v2", "b1"))
            .unwrap(),
    );

    let cancelled = coordinator.cancel(&service, &queued.task_id).unwrap();
    assert_eq!(cancelled.display_status, WorkflowDisplayStatus::Cancelled);
    assert_eq!(
        coordinator
            .cancel(&service, &queued.task_id)
            .unwrap()
            .task_id,
        queued.task_id
    );
    let (restored, claimed) = coordinator.undo_cancel(&service, &queued.task_id).unwrap();
    assert_eq!(restored.display_status, WorkflowDisplayStatus::Queued);
    assert!(claimed.is_none());
    assert_eq!(
        coordinator
            .undo_cancel(&service, &queued.task_id)
            .unwrap()
            .0
            .task_id,
        queued.task_id
    );

    service
        .start_workflow_stage(&first.task_id, "prepare")
        .unwrap();
    service
        .fail_workflow_stage(
            &first.task_id,
            "prepare",
            llm_wiki_desktop_lib::models::workflow::WorkflowErrorSummary {
                code: "TEST".into(),
                message_key: "test".into(),
                recoverable: true,
                user_action_required: false,
                suggested_action: None,
                project_mutation_state: WorkflowProjectMutationState::Unknown,
            },
        )
        .unwrap();
    let retry = created(
        coordinator
            .retry(
                &service,
                &first.task_id,
                first.project_id.clone(),
                PathBuf::from(temp.path()),
                Some(temp.path().join(".app/tasks")),
            )
            .unwrap(),
    );
    assert_ne!(retry.task_id, first.task_id);
    assert_eq!(retry.retry.as_ref().unwrap().attempt_of, first.task_id);
    assert_eq!(
        service
            .get_workflow_run(&first.task_id)
            .unwrap()
            .display_status,
        WorkflowDisplayStatus::Failed
    );

    let other_root = tempfile::tempdir().unwrap();
    assert!(coordinator
        .retry(
            &service,
            &first.task_id,
            first.project_id.clone(),
            other_root.path().to_path_buf(),
            Some(other_root.path().join(".app/tasks")),
        )
        .is_err());
}

#[test]
fn retry_uses_new_memory_only_authority_without_updating_the_old_task_file() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let original = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    service
        .start_workflow_stage(&original.task_id, "prepare")
        .unwrap();
    service
        .fail_workflow_stage(
            &original.task_id,
            "prepare",
            llm_wiki_desktop_lib::models::workflow::WorkflowErrorSummary {
                code: "TEST".into(),
                message_key: "test".into(),
                recoverable: true,
                user_action_required: false,
                suggested_action: None,
                project_mutation_state: WorkflowProjectMutationState::Unknown,
            },
        )
        .unwrap();
    let old_path = temp
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", original.task_id));
    coordinator
        .apply_persistence_and_continue_queued(
            &service,
            &original.canonical_identity_key,
            &original.identity_revision,
            &[(original.task_id.clone(), None)],
            false,
        )
        .unwrap();
    let old_bytes = std::fs::read(&old_path).unwrap();

    let retry = created(
        coordinator
            .retry(
                &service,
                &original.task_id,
                "new-runtime-project-id".into(),
                temp.path().to_path_buf(),
                None,
            )
            .unwrap(),
    );
    assert_eq!(retry.project_id, "new-runtime-project-id");
    assert_eq!(
        retry.persistence_transition,
        Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly)
    );
    service
        .start_workflow_stage(&retry.task_id, "prepare")
        .unwrap();

    assert_eq!(std::fs::read(old_path).unwrap(), old_bytes);
    assert!(!temp
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", retry.task_id))
        .exists());
    assert!(service
        .get_logs(&retry.task_id)
        .unwrap()
        .iter()
        .any(|line| {
            line.level == llm_wiki_desktop_lib::tasks::task_model::LogLevel::Warn
                && line.message.contains("memory-only")
        }));
}

#[test]
fn retry_uses_new_unicode_persistence_root_without_backfilling_the_old_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let mut request = request(temp.path(), "p", "v1", "b1");
    request.task_state_root = None;
    let original = created(coordinator.enqueue(&service, request).unwrap());
    service
        .start_workflow_stage(&original.task_id, "prepare")
        .unwrap();
    service
        .fail_workflow_stage(
            &original.task_id,
            "prepare",
            llm_wiki_desktop_lib::models::workflow::WorkflowErrorSummary {
                code: "TEST".into(),
                message_key: "test".into(),
                recoverable: true,
                user_action_required: false,
                suggested_action: None,
                project_mutation_state: WorkflowProjectMutationState::Unknown,
            },
        )
        .unwrap();
    assert!(!temp.path().join(".app").exists());
    let new_root = temp.path().join(".app/任务");

    let retry = created(
        coordinator
            .retry(
                &service,
                &original.task_id,
                original.project_id.clone(),
                temp.path().to_path_buf(),
                Some(new_root.clone()),
            )
            .unwrap(),
    );

    assert!(!new_root.join(format!("{}.json", original.task_id)).exists());
    assert!(new_root.join(format!("{}.json", retry.task_id)).exists());
    assert!(service
        .get_logs(&retry.task_id)
        .unwrap()
        .iter()
        .any(|line| {
            line.level == llm_wiki_desktop_lib::tasks::task_model::LogLevel::Info
                && line.message.contains("newly derived")
        }));
}

#[test]
fn continue_rebinds_queued_run_to_memory_only_before_eligibility_allows_claiming() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let active = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    let queued = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v2", "b2"))
            .unwrap(),
    );
    service
        .set_workflow_queue_state(&queued.task_id, queued.queue_position, true)
        .unwrap();
    let old_path = temp
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", queued.task_id));
    let old_bytes = std::fs::read(&old_path).unwrap();

    let (_runs, claimed) = coordinator
        .apply_persistence_and_continue_queued(
            &service,
            &active.canonical_identity_key,
            &active.identity_revision,
            &[(queued.task_id.clone(), None)],
            false,
        )
        .unwrap();

    assert!(claimed.is_none());
    let rebound = service.get_workflow_run(&queued.task_id).unwrap();
    assert_eq!(rebound.persistence, WorkflowPersistenceMode::MemoryOnly);
    assert_eq!(
        rebound.persistence_transition,
        Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly)
    );
    assert!(rebound.continuation_required);
    coordinator.cancel(&service, &queued.task_id).unwrap();
    assert_eq!(std::fs::read(old_path).unwrap(), old_bytes);
}

#[test]
fn continue_upgrade_writes_the_continuation_transition_only_to_the_new_root() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let mut first_request = request(temp.path(), "p", "v1", "b1");
    first_request.task_state_root = None;
    let active = created(coordinator.enqueue(&service, first_request).unwrap());
    let mut second_request = request(temp.path(), "p", "v2", "b2");
    second_request.task_state_root = None;
    let queued = created(coordinator.enqueue(&service, second_request).unwrap());
    service
        .set_workflow_queue_state(&queued.task_id, queued.queue_position, true)
        .unwrap();
    let new_root = temp.path().join(".app/new-tasks");
    std::fs::create_dir_all(&new_root).unwrap();
    let new_root = new_root.canonicalize().unwrap();
    let new_path = new_root.join(format!("{}.json", queued.task_id));

    let (_runs, claimed) = coordinator
        .apply_persistence_and_continue_queued(
            &service,
            &active.canonical_identity_key,
            &active.identity_revision,
            &[(queued.task_id.clone(), Some(new_root))],
            true,
        )
        .unwrap();

    assert!(claimed.is_none());
    assert!(new_path.exists());
    assert!(!temp
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", queued.task_id))
        .exists());
    let rebound = service.get_workflow_run(&queued.task_id).unwrap();
    assert_eq!(rebound.persistence, WorkflowPersistenceMode::Persistent);
    assert_eq!(
        rebound.persistence_transition,
        Some(WorkflowPersistenceTransition::UpgradedToPersistent)
    );
    assert!(!rebound.continuation_required);

    service
        .start_workflow_stage(&active.task_id, "prepare")
        .unwrap();
    service
        .complete_workflow_stage(&active.task_id, "prepare")
        .unwrap();
    let (_completed, claimed) = coordinator
        .complete_and_claim_next(&service, &active.task_id, empty_update_wiki_result())
        .unwrap();
    assert_eq!(claimed.unwrap().task_id, queued.task_id);
    assert!(new_path.exists());
}

#[test]
fn terminal_transitions_claim_next_but_waiting_pauses_the_queue() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let first = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    let second = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v2", "b2"))
            .unwrap(),
    );
    let third = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v3", "b3"))
            .unwrap(),
    );

    service
        .start_workflow_stage(&first.task_id, "prepare")
        .unwrap();
    service
        .complete_workflow_stage(&first.task_id, "prepare")
        .unwrap();
    let (_, claimed_second) = coordinator
        .complete_and_claim_next(
            &service,
            &first.task_id,
            WorkflowResult::UpdateWiki {
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
        .unwrap();
    assert_eq!(claimed_second.unwrap().task_id, second.task_id);

    service
        .start_workflow_stage(&second.task_id, "prepare")
        .unwrap();
    service
        .wait_workflow_stage(
            &second.task_id,
            "prepare",
            WorkflowPendingAction {
                id: "decision".into(),
                action_type: PendingActionType::MergeConflict,
                risk_level: RiskLevel::High,
                affected_paths: vec!["wiki/page.md".into()],
                candidate: None,
                expires_at: None,
                checkpoint_hash: None,
            },
        )
        .unwrap();
    let identity = project_identity(temp.path()).unwrap();
    assert!(coordinator
        .claim_next(
            &service,
            &identity.canonical_identity_key,
            &identity.identity_revision,
        )
        .unwrap()
        .is_none());
    service
        .clear_workflow_pending_action(&second.task_id)
        .unwrap();
    let (_, claimed_third) = coordinator
        .fail_stage_and_claim_next(
            &service,
            &second.task_id,
            "prepare",
            llm_wiki_desktop_lib::models::workflow::WorkflowErrorSummary {
                code: "TEST".into(),
                message_key: "test".into(),
                recoverable: true,
                user_action_required: false,
                suggested_action: None,
                project_mutation_state: WorkflowProjectMutationState::Unknown,
            },
        )
        .unwrap();
    assert_eq!(claimed_third.unwrap().task_id, third.task_id);
}

#[test]
fn invalid_waiting_confirmation_interrupts_and_claims_next_once() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let waiting = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    let queued = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v2", "b2"))
            .unwrap(),
    );
    service
        .start_workflow_stage(&waiting.task_id, "prepare")
        .unwrap();
    service
        .wait_workflow_stage(
            &waiting.task_id,
            "prepare",
            WorkflowPendingAction {
                id: "invalid-action".into(),
                action_type: PendingActionType::MergeConflict,
                risk_level: RiskLevel::High,
                affected_paths: vec!["wiki/page.md".into()],
                candidate: None,
                expires_at: None,
                checkpoint_hash: None,
            },
        )
        .unwrap();

    let (interrupted, next) = coordinator
        .interrupt_invalid_confirmation(
            &service,
            &waiting.task_id,
            llm_wiki_desktop_lib::models::workflow::WorkflowErrorSummary {
                code: "WORKFLOW_CONFIRMATION_RECOVERY_FAILED".into(),
                message_key: "workflows.error.prepareAgain".into(),
                recoverable: false,
                user_action_required: true,
                suggested_action: Some(
                    llm_wiki_desktop_lib::models::workflow::WorkflowPrerequisiteAction::PrepareAgain,
                ),
                project_mutation_state: WorkflowProjectMutationState::Unknown,
            },
        )
        .unwrap();

    assert_eq!(
        interrupted.display_status,
        WorkflowDisplayStatus::Interrupted
    );
    assert!(interrupted.pending_action.is_none());
    assert_eq!(next.unwrap().task_id, queued.task_id);
    let (same, duplicate_next) = coordinator
        .interrupt_invalid_confirmation(
            &service,
            &waiting.task_id,
            llm_wiki_desktop_lib::models::workflow::WorkflowErrorSummary {
                code: "WORKFLOW_CONFIRMATION_RECOVERY_FAILED".into(),
                message_key: "workflows.error.prepareAgain".into(),
                recoverable: false,
                user_action_required: true,
                suggested_action: None,
                project_mutation_state: WorkflowProjectMutationState::Unknown,
            },
        )
        .unwrap();
    assert_eq!(same.display_status, WorkflowDisplayStatus::Interrupted);
    assert!(duplicate_next.is_none());
}

#[test]
fn cancelled_or_terminal_workflows_reject_stale_stage_updates() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let running = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    service
        .start_workflow_stage(&running.task_id, "prepare")
        .unwrap();
    coordinator.cancel(&service, &running.task_id).unwrap();

    assert!(service
        .update_workflow_stage_progress(&running.task_id, "prepare", None, 1, Some(1))
        .is_err());
    assert!(service
        .fail_workflow_stage(
            &running.task_id,
            "prepare",
            llm_wiki_desktop_lib::models::workflow::WorkflowErrorSummary {
                code: "STALE".into(),
                message_key: "stale".into(),
                recoverable: true,
                user_action_required: false,
                suggested_action: None,
                project_mutation_state: WorkflowProjectMutationState::Unknown,
            },
        )
        .is_err());
    assert_eq!(
        service.get_task(&running.task_id).unwrap().status,
        llm_wiki_desktop_lib::models::task::TaskStatus::Cancelling
    );
    coordinator
        .finish_cancelled_and_claim_next(&service, &running.task_id)
        .unwrap();
    assert!(coordinator.undo_cancel(&service, &running.task_id).is_err());
}

#[test]
fn undo_after_the_active_run_finishes_claims_the_restored_queue_item() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let active = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    let queued = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v2", "b2"))
            .unwrap(),
    );
    coordinator.cancel(&service, &queued.task_id).unwrap();
    service
        .start_workflow_stage(&active.task_id, "prepare")
        .unwrap();
    let (_, next) = coordinator
        .fail_stage_and_claim_next(
            &service,
            &active.task_id,
            "prepare",
            llm_wiki_desktop_lib::models::workflow::WorkflowErrorSummary {
                code: "TEST".into(),
                message_key: "test".into(),
                recoverable: true,
                user_action_required: false,
                suggested_action: None,
                project_mutation_state: WorkflowProjectMutationState::Unknown,
            },
        )
        .unwrap();
    assert!(next.is_none());

    let (restored, claimed) = coordinator.undo_cancel(&service, &queued.task_id).unwrap();
    assert_eq!(restored.display_status, WorkflowDisplayStatus::Running);
    assert_eq!(restored.queue_position, None);
    assert_eq!(
        claimed.as_ref().map(|run| run.task_id.as_str()),
        Some(queued.task_id.as_str())
    );

    // An IPC retry after dispatch observes the already-restored run and must
    // not claim or dispatch it a second time.
    let (replayed, replay_claim) = coordinator.undo_cancel(&service, &queued.task_id).unwrap();
    assert_eq!(replayed.display_status, WorkflowDisplayStatus::Running);
    assert_eq!(replayed.queue_position, None);
    assert!(replay_claim.is_none());

    let later = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v3", "b3"))
            .unwrap(),
    );
    assert_eq!(later.display_status, WorkflowDisplayStatus::Queued);
    assert_eq!(later.queue_position, Some(1));
}

#[test]
fn only_one_ordered_current_stage_can_mutate() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let mut two_stage = request(temp.path(), "p", "v1", "b1");
    let mut publish = stage();
    publish.id = "publish".into();
    publish.ordinal = 2;
    two_stage.stages.push(publish);
    let run = created(coordinator.enqueue(&service, two_stage).unwrap());

    service
        .start_workflow_stage(&run.task_id, "prepare")
        .unwrap();
    assert!(service
        .start_workflow_stage(&run.task_id, "publish")
        .is_err());
    assert!(service
        .update_workflow_stage_progress(&run.task_id, "publish", None, 1, Some(1))
        .is_err());
    service
        .complete_workflow_stage(&run.task_id, "prepare")
        .unwrap();
    service
        .start_workflow_stage(&run.task_id, "publish")
        .unwrap();
    assert_eq!(
        service
            .get_workflow_run(&run.task_id)
            .unwrap()
            .current_stage_id
            .as_deref(),
        Some("publish")
    );
}

#[test]
fn workflow_cannot_finish_with_pending_or_running_stages() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let run = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );

    assert!(coordinator
        .complete_and_claim_next(&service, &run.task_id, empty_update_wiki_result())
        .is_err());
    service
        .start_workflow_stage(&run.task_id, "prepare")
        .unwrap();
    assert!(coordinator
        .complete_and_claim_next(&service, &run.task_id, empty_update_wiki_result())
        .is_err());
    assert_eq!(
        service
            .get_workflow_run(&run.task_id)
            .unwrap()
            .display_status,
        WorkflowDisplayStatus::Running
    );
}

#[test]
fn atomic_task_write_ignores_preplaced_predictable_temp_link() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let run = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    let external = temp.path().join("external-target.txt");
    std::fs::write(&external, b"outside-must-not-change").unwrap();
    let predictable = temp
        .path()
        .join(".app/tasks")
        .join(format!(".{}.json.tmp", run.task_id));
    std::fs::hard_link(&external, &predictable).unwrap();

    service
        .start_workflow_stage(&run.task_id, "prepare")
        .unwrap();

    assert_eq!(
        std::fs::read(&external).unwrap(),
        b"outside-must-not-change"
    );
    assert_eq!(
        std::fs::read(&predictable).unwrap(),
        b"outside-must-not-change"
    );
}

#[test]
fn memory_only_workflows_never_create_project_app_state() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let mut memory_only = request(temp.path(), "p", "v1", "b1");
    memory_only.task_state_root = None;
    let run = created(coordinator.enqueue(&service, memory_only).unwrap());
    assert_eq!(run.display_status, WorkflowDisplayStatus::Running);
    assert!(!temp.path().join(".app").exists());
}

#[test]
fn workflow_mutations_persist_one_versioned_snapshot_and_emit_scoped_events() {
    let temp = tempfile::tempdir().unwrap();
    let (event_bus, events) = EventBus::new_test_capture();
    let service = TaskService::with_event_bus(event_bus);
    let coordinator = WorkflowCoordinator::default();
    let run = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    assert!(events.lock().unwrap().iter().any(|event| {
        event.event_type == BackendEventType::WorkflowUpdated
            && event.project_id.as_deref() == Some("p")
            && event.task_id.as_deref() == Some(run.task_id.as_str())
    }));
    service
        .start_workflow_stage(&run.task_id, "prepare")
        .unwrap();
    service
        .update_workflow_stage_progress(
            &run.task_id,
            "prepare",
            Some("wiki/中文.md".into()),
            1,
            Some(2),
        )
        .unwrap();

    let path = temp
        .path()
        .join(".app/tasks")
        .join(format!("{}.json", run.task_id));
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(value["schemaVersion"], 2);
    assert_eq!(value["workflow"]["currentStageId"], "prepare");
    assert_eq!(value["workflow"]["stages"][0]["progress"]["current"], 1);
    assert_eq!(
        value["workflow"]["executionOptions"]["preparationRevision"],
        "prep-1"
    );
    assert!(events.lock().unwrap().iter().any(|event| {
        event.event_type == BackendEventType::WorkflowUpdated
            && event.project_id.as_deref() == Some("p")
            && event.task_id.as_deref() == Some(run.task_id.as_str())
    }));

    assert!(
        serde_json::from_value::<WorkflowExecutionOptions>(serde_json::json!({
            "preparationRevision": "prep-3",
            "credential": "must-not-persist"
        }))
        .is_err()
    );
}

#[test]
fn concurrent_identical_starts_are_atomic() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_path_buf();
    let service = Arc::new(TaskService::default());
    let coordinator = Arc::new(WorkflowCoordinator::default());
    let barrier = Arc::new(Barrier::new(2));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let service = service.clone();
        let coordinator = coordinator.clone();
        let barrier = barrier.clone();
        let request = request(&root, "p", "v1", "same");
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            coordinator.enqueue(&service, request).unwrap()
        }));
    }
    let outcomes = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    let ids = outcomes
        .iter()
        .map(|outcome| match outcome {
            WorkflowStartOutcome::Created { run } | WorkflowStartOutcome::Existing { run } => {
                run.task_id.clone()
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(ids[0], ids[1]);
    assert_eq!(service.list_workflow_runs().len(), 1);
}

#[test]
fn cancel_after_real_queue_claim_is_deterministic_before_first_stage() {
    let temp = tempfile::tempdir().unwrap();
    let service = Arc::new(TaskService::default());
    let coordinator = Arc::new(WorkflowCoordinator::default());
    let active = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "active-window"))
            .unwrap(),
    );
    let queued = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v2", "cancel-window"))
            .unwrap(),
    );
    service
        .start_workflow_stage(&active.task_id, "prepare")
        .unwrap();
    service
        .complete_workflow_stage(&active.task_id, "prepare")
        .unwrap();
    let (controller, worker_window) = controlled_race();
    let worker_service = service.clone();
    let worker_coordinator = coordinator.clone();
    let worker = std::thread::spawn(move || {
        let (_, claimed) = worker_coordinator
            .complete_and_claim_next(&worker_service, &active.task_id, empty_update_wiki_result())
            .unwrap();
        let claimed = claimed.expect("queued workflow must be claimed");
        worker_window.pause_at(RacePoint::Claimed);
        worker_window.pause_at(RacePoint::FirstStage);
        let result = worker_service.start_workflow_stage(&claimed.task_id, "prepare");
        let finalized = result.as_ref().err().map(|_| {
            worker_coordinator
                .reject_claimed_dispatch(
                    &worker_service,
                    &claimed.task_id,
                    WorkflowDispatchFailure::stale(
                        "WORKFLOW_DISPATCH_CANCELLED",
                        "workflows.error.prepareAgain",
                    ),
                )
                .unwrap()
        });
        (claimed.task_id, result, finalized)
    });

    controller.wait_for(RacePoint::Claimed);
    let cancelled = coordinator.cancel(&service, &queued.task_id).unwrap();
    assert_eq!(cancelled.display_status, WorkflowDisplayStatus::Running);
    controller.release();
    controller.wait_for(RacePoint::FirstStage);
    controller.release();

    let (claimed_task_id, first_stage, finalized) = worker.join().unwrap();
    assert_eq!(claimed_task_id, queued.task_id);
    assert!(first_stage.is_err());
    assert_eq!(
        service.get_task(&queued.task_id).unwrap().status,
        TaskStatus::Cancelled,
        "a rejected first-stage start must finalize a concurrent cancellation",
    );
    assert_eq!(
        finalized
            .expect("dispatch rejection must be finalized")
            .0
            .display_status,
        WorkflowDisplayStatus::Cancelled,
    );
}

#[test]
fn queued_runs_can_be_reordered_only_inside_their_project_queue() {
    let temp = tempfile::tempdir().unwrap();
    let service = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let _running = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v1", "b1"))
            .unwrap(),
    );
    let second = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v2", "b2"))
            .unwrap(),
    );
    let third = created(
        coordinator
            .enqueue(&service, request(temp.path(), "p", "v3", "b3"))
            .unwrap(),
    );
    coordinator
        .reorder_queued(&service, &third.task_id, Some(&second.task_id))
        .unwrap();
    assert_eq!(
        service
            .get_workflow_run(&third.task_id)
            .unwrap()
            .queue_position,
        Some(1)
    );
    assert_eq!(
        service
            .get_workflow_run(&second.task_id)
            .unwrap()
            .queue_position,
        Some(2)
    );
}
