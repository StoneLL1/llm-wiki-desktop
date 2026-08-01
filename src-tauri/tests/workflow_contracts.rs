use llm_wiki_desktop_lib::models::agent::AgentKind;
use llm_wiki_desktop_lib::models::confirmation::{PendingActionType, RiskLevel};
use llm_wiki_desktop_lib::models::llm::LlmProviderKind;
use llm_wiki_desktop_lib::models::task::{BackendTask, TaskStatus};
use llm_wiki_desktop_lib::models::workflow::{
    HealthCheckMode, UpdateWikiMode, WorkflowArtifactType, WorkflowBaselineSummary,
    WorkflowCandidateReference, WorkflowCountProgress, WorkflowDisplayStatus, WorkflowErrorSummary,
    WorkflowFilesystemAccess, WorkflowGitPolicy, WorkflowGitState, WorkflowKind,
    WorkflowOutputSummary, WorkflowPendingAction, WorkflowPersistenceMode, WorkflowPreparation,
    WorkflowPrerequisite, WorkflowPrerequisiteAction, WorkflowProjectAccessSummary,
    WorkflowProjectTrust, WorkflowRetryLink, WorkflowRoute, WorkflowRouteSelection, WorkflowRun,
    WorkflowScope, WorkflowSourceVersionRef, WorkflowStage, WorkflowStageStatus,
    WorkflowStartOutcome, WORKFLOW_SCHEMA_VERSION,
};
use llm_wiki_desktop_lib::tasks::TaskService;
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

fn project_access() -> WorkflowProjectAccessSummary {
    WorkflowProjectAccessSummary {
        project_id: "project-中文".into(),
        canonical_identity_key: "identity-1".into(),
        identity_revision: "revision-1".into(),
        trust: WorkflowProjectTrust::Trusted,
        filesystem_access: WorkflowFilesystemAccess::Writable,
        persistence: WorkflowPersistenceMode::Persistent,
        git_state: WorkflowGitState::Clean,
    }
}

fn update_scope() -> WorkflowScope {
    WorkflowScope::UpdateWiki {
        mode: UpdateWikiMode::ChangedSources,
        source_versions: vec![WorkflowSourceVersionRef {
            source_id: "source-中文".into(),
            version_id: "version-1".into(),
        }],
    }
}

fn waiting_action() -> WorkflowPendingAction {
    WorkflowPendingAction {
        id: "action-1".into(),
        action_type: PendingActionType::MergeConflict,
        risk_level: RiskLevel::High,
        affected_paths: vec!["wiki/概念.md".into()],
        candidate: Some(WorkflowCandidateReference::TaskOwned {
            candidate_id: "candidate-1".into(),
        }),
        expires_at: None,
        checkpoint_hash: Some("checkpoint-1".into()),
    }
}

fn sample_run() -> WorkflowRun {
    WorkflowRun {
        schema_version: WORKFLOW_SCHEMA_VERSION,
        task_id: "task-1".into(),
        project_id: "project-中文".into(),
        canonical_identity_key: "identity-1".into(),
        identity_revision: "revision-1".into(),
        kind: WorkflowKind::UpdateWiki,
        display_status: WorkflowDisplayStatus::WaitingForConfirmation,
        scope: update_scope(),
        route: Some(WorkflowRoute::Byok {
            provider: LlmProviderKind::OpenAi,
            model: "configured-model".into(),
            route_revision: "route-1".into(),
        }),
        fingerprint: "fingerprint-1".into(),
        baseline_fingerprint: "baseline-1".into(),
        stages: vec![WorkflowStage {
            id: "review".into(),
            ordinal: 1,
            status: WorkflowStageStatus::Waiting,
            label_key: "workflows.stage.review".into(),
            started_at: Some("2026-07-30T00:00:00Z".into()),
            completed_at: None,
            current_item: Some("wiki/概念.md".into()),
            progress: Some(WorkflowCountProgress {
                current: 1,
                total: Some(2),
            }),
            decision: Some(waiting_action()),
        }],
        current_stage_id: Some("review".into()),
        queue_position: None,
        continuation_required: false,
        retry: Some(WorkflowRetryLink {
            attempt_of: "task-0".into(),
            attempt_number: 2,
        }),
        pending_action: None,
        result: None,
        error: None,
        started_at: "2026-07-30T00:00:00Z".into(),
        updated_at: "2026-07-30T00:01:00Z".into(),
        completed_at: None,
        cancellable: true,
        undo_cancel_until: None,
    }
}

#[test]
fn workflow_contract_uses_schema_v1_and_stable_wire_casing() {
    assert_eq!(WORKFLOW_SCHEMA_VERSION, 1);
    assert_eq!(
        serde_json::to_string(&WorkflowKind::GenerateContent).unwrap(),
        "\"generate_content\""
    );
    assert_eq!(
        serde_json::to_string(&WorkflowStageStatus::Waiting).unwrap(),
        "\"waiting\""
    );

    let value = serde_json::to_value(sample_run()).unwrap();
    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["displayStatus"], "waiting_for_confirmation");
    assert_eq!(value["cancellable"], true);
    assert!(value["undoCancelUntil"].is_null());
    assert_eq!(value["scope"]["kind"], "update_wiki");
    assert_eq!(value["scope"]["mode"], "changed_sources");
    assert_eq!(
        value["scope"]["sourceVersions"][0]["sourceId"],
        "source-中文"
    );
    assert_eq!(value["route"]["kind"], "byok");
    assert_eq!(value["route"]["provider"], "open_ai");
    assert_eq!(value["stages"][0]["currentItem"], "wiki/概念.md");
    assert_eq!(
        value["stages"][0]["decision"]["candidate"]["kind"],
        "task_owned"
    );
    assert!(value.get("display_status").is_none());
    assert_no_secret_bearing_keys(&value);
}

#[test]
fn every_scope_and_route_is_a_tagged_union() {
    let cases = [
        serde_json::to_value(WorkflowRoute::Local {
            route_revision: "local-1".into(),
        })
        .unwrap(),
        serde_json::to_value(WorkflowRoute::Agent {
            agent: AgentKind::Claude,
            model: None,
            route_revision: "agent-1".into(),
        })
        .unwrap(),
        serde_json::to_value(WorkflowRoute::Byok {
            provider: LlmProviderKind::Anthropic,
            model: "configured-model".into(),
            route_revision: "byok-1".into(),
        })
        .unwrap(),
    ];
    assert_eq!(cases[0]["kind"], "local");
    assert_eq!(cases[1]["kind"], "agent");
    assert_eq!(cases[2]["kind"], "byok");

    let scopes = [
        update_scope(),
        WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick,
        },
        WorkflowScope::GenerateContent {
            artifact_type: WorkflowArtifactType::KnowledgeCard,
            page_paths: vec!["wiki/概念.md".into()],
            output_path: None,
        },
    ];
    let kinds = scopes
        .into_iter()
        .map(|scope| serde_json::to_value(scope).unwrap()["kind"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            json!("update_wiki"),
            json!("health_check"),
            json!("generate_content")
        ]
    );
}

#[test]
fn task_status_maps_to_user_facing_workflow_status_without_renaming_succeeded() {
    let cases = [
        (TaskStatus::Queued, WorkflowDisplayStatus::Queued),
        (TaskStatus::Running, WorkflowDisplayStatus::Running),
        (TaskStatus::Cancelling, WorkflowDisplayStatus::Running),
        (
            TaskStatus::WaitingForConfirmation,
            WorkflowDisplayStatus::WaitingForConfirmation,
        ),
        (TaskStatus::Succeeded, WorkflowDisplayStatus::Completed),
        (TaskStatus::Failed, WorkflowDisplayStatus::Failed),
        (TaskStatus::Cancelled, WorkflowDisplayStatus::Cancelled),
        (TaskStatus::Interrupted, WorkflowDisplayStatus::Interrupted),
    ];
    for (task, display) in cases {
        assert_eq!(WorkflowDisplayStatus::from(&task), display);
    }
    assert_eq!(
        serde_json::to_string(&TaskStatus::Succeeded).unwrap(),
        "\"succeeded\""
    );
    assert_eq!(
        serde_json::to_string(&TaskStatus::Interrupted).unwrap(),
        "\"interrupted\""
    );
}

#[test]
fn optional_workflow_migration_fields_default_safely() {
    let mut value = serde_json::to_value(sample_run()).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("schemaVersion");
    object.remove("continuationRequired");
    object.remove("retry");
    object.remove("pendingAction");
    object.remove("result");
    object.remove("error");

    let restored: WorkflowRun = serde_json::from_value(value).unwrap();
    assert_eq!(restored.schema_version, WORKFLOW_SCHEMA_VERSION);
    assert!(!restored.continuation_required);
    assert!(restored.retry.is_none());
    assert!(restored.pending_action.is_none());
    assert!(restored.result.is_none());
    assert!(restored.error.is_none());
}

#[test]
fn preparation_and_start_outcome_are_structured_and_non_secret() {
    let preparation = WorkflowPreparation {
        schema_version: WORKFLOW_SCHEMA_VERSION,
        preparation_id: "preparation-1".into(),
        preparation_revision: "1".into(),
        project_access: project_access(),
        kind: WorkflowKind::UpdateWiki,
        scope: update_scope(),
        baseline: WorkflowBaselineSummary {
            fingerprint: "baseline-1".into(),
            captured_at: "2026-07-30T00:00:00Z".into(),
            item_count: 1,
        },
        route: Some(WorkflowRoute::Agent {
            agent: AgentKind::Codex,
            model: None,
            route_revision: "agent-route-1".into(),
        }),
        prerequisites: vec![WorkflowPrerequisite {
            code: "PROJECT_DIRTY".into(),
            message_key: "workflows.prerequisite.resolveDirtyGit".into(),
            blocking: true,
            action: WorkflowPrerequisiteAction::ResolveDirtyGit,
        }],
        output: WorkflowOutputSummary {
            label_key: "workflows.output.wiki".into(),
            location: Some("wiki/".into()),
            may_change_wiki: true,
        },
        git_policy: WorkflowGitPolicy::RequiredBeforeWrite,
        requires_scope_confirmation: true,
        quick_rerun_eligible: false,
        available_source_versions: vec![WorkflowSourceVersionRef {
            source_id: "source-1".into(),
            version_id: "version-1".into(),
        }],
        available_wiki_pages: vec!["wiki/概念.md".into()],
        available_routes: vec![WorkflowRouteSelection::Agent {
            agent: AgentKind::Codex,
        }],
    };

    let value = serde_json::to_value(&preparation).unwrap();
    assert_eq!(value["projectAccess"]["filesystemAccess"], "writable");
    assert_eq!(value["gitPolicy"], "required_before_write");
    assert_eq!(value["prerequisites"][0]["action"], "resolve_dirty_git");
    assert_eq!(value["availableWikiPages"][0], "wiki/概念.md");
    assert_eq!(value["availableRoutes"][0]["kind"], "agent");
    assert_no_secret_bearing_keys(&value);

    let outcome = WorkflowStartOutcome::Created { run: sample_run() };
    let outcome_value = serde_json::to_value(outcome).unwrap();
    assert_eq!(outcome_value["kind"], "created");
    assert_eq!(outcome_value["run"]["taskId"], "task-1");
}

#[test]
fn error_summary_uses_localization_key_and_typed_recovery_action() {
    let error = WorkflowErrorSummary {
        code: "WORKFLOW_ROUTE_UNAVAILABLE".into(),
        message_key: "workflows.error.routeUnavailable".into(),
        recoverable: true,
        user_action_required: true,
        suggested_action: Some(WorkflowPrerequisiteAction::ConfigureExecutionRoute),
    };
    let value = serde_json::to_value(error).unwrap();
    assert_eq!(value["messageKey"], "workflows.error.routeUnavailable");
    assert_eq!(value["suggestedAction"], "configure_execution_route");
    assert!(value.get("message").is_none());
}

#[test]
fn legacy_raw_and_wrapped_task_files_still_recover() {
    let root = std::env::temp_dir().join(format!("workflow-contracts-{}", Uuid::new_v4()));
    let tasks_dir = root.join(".app").join("tasks");
    std::fs::create_dir_all(&tasks_dir).unwrap();

    let raw_id = Uuid::new_v4().to_string();
    let wrapped_id = Uuid::new_v4().to_string();
    write_json(
        &tasks_dir.join(format!("{raw_id}.json")),
        &legacy_task_json(&raw_id, "succeeded"),
    );
    write_json(
        &tasks_dir.join(format!("{wrapped_id}.json")),
        &json!({
            "task": legacy_task_json(&wrapped_id, "cancelled"),
            "logLines": [{
                "timestamp": "2026-07-30T00:00:00Z",
                "level": "info",
                "message": "legacy log"
            }]
        }),
    );

    let service = TaskService::default();
    let recovered = service.recover_tasks(&root).unwrap();
    assert_eq!(recovered.len(), 2);
    assert_eq!(
        service.get_task(&raw_id).unwrap().status,
        TaskStatus::Succeeded
    );
    assert_eq!(
        service.get_task(&wrapped_id).unwrap().status,
        TaskStatus::Cancelled
    );
    assert_eq!(
        service.get_logs(&wrapped_id).unwrap()[0].message,
        "legacy log"
    );

    std::fs::remove_dir_all(root).ok();
}

fn legacy_task_json(id: &str, status: &str) -> Value {
    json!({
        "id": id,
        "taskType": "wiki_compile",
        "projectId": "legacy-project",
        "title": "Legacy compile",
        "status": status,
        "progress": null,
        "startedAt": "2026-07-01T00:00:00Z",
        "updatedAt": "2026-07-01T00:01:00Z",
        "completedAt": "2026-07-01T00:01:00Z",
        "cancellable": false,
        "logPath": null,
        "result": null,
        "error": null
    })
}

fn write_json(path: &Path, value: &Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn assert_no_secret_bearing_keys(value: &Value) {
    const FORBIDDEN: &[&str] = &[
        "apiKey",
        "apiToken",
        "secret",
        "secretMask",
        "cookie",
        "prompt",
        "instructions",
        "command",
        "sourceExcerpt",
        "modelOutput",
    ];
    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                assert!(!FORBIDDEN.contains(&key.as_str()), "forbidden key: {key}");
                assert_no_secret_bearing_keys(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                assert_no_secret_bearing_keys(item);
            }
        }
        _ => {}
    }
}

#[test]
fn backend_task_still_deserializes_without_batch_identity() {
    let task: BackendTask = serde_json::from_value(legacy_task_json("task-legacy", "succeeded"))
        .expect("legacy task should remain readable");
    assert!(task.batch_id.is_none());
}

#[test]
fn workflow_frontend_commands_are_registered() {
    let lib = include_str!("../src/lib.rs");
    for command in [
        "commands::workflow_commands::get_workflows_overview",
        "commands::workflow_commands::prepare_workflow",
        "commands::workflow_commands::start_workflow",
        "commands::workflow_commands::list_workflow_runs",
        "commands::workflow_commands::get_workflow_run",
        "commands::workflow_commands::cancel_workflow_run",
        "commands::workflow_commands::undo_cancel_queued_workflow",
        "commands::workflow_commands::reorder_queued_workflow",
        "commands::task_commands::continue_queued_workflows",
        "commands::workflow_commands::retry_workflow",
        "commands::workflow_commands::confirm_workflow_action",
        "commands::workflow_commands::discard_workflow_result",
    ] {
        assert!(lib.contains(command), "missing Tauri command: {command}");
    }
}
