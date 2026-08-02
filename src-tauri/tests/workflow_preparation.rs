use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::project::ProjectTrustKind;
use llm_wiki_desktop_lib::models::workflow::{
    HealthCheckMode, WorkflowArtifactType, WorkflowFilesystemAccess, WorkflowGitState,
    WorkflowKind, WorkflowPersistenceMode, WorkflowProjectTrust, WorkflowRun, WorkflowScope,
    WorkflowStartOutcome,
};
use llm_wiki_desktop_lib::services::{
    project_identity, AgentService, PrepareWorkflowInput, SecretService, SettingsService,
    WorkflowAccessSnapshot, WorkflowPreference, WorkflowPreferences,
    WorkflowPreparationEnvironment, WorkflowRunner, WorkflowService,
};
use llm_wiki_desktop_lib::tasks::TaskService;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn project() -> (tempfile::TempDir, ProjectContext) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".app")).unwrap();
    std::fs::create_dir_all(root.path().join("wiki")).unwrap();
    std::fs::create_dir_all(root.path().join("raw/extracted")).unwrap();
    std::fs::write(root.path().join("wiki/概览.md"), "# 概览\n").unwrap();
    let context = ProjectContext::new("项目-一", root.path().to_path_buf());
    (root, context)
}

fn access(
    trust: WorkflowProjectTrust,
    filesystem_access: WorkflowFilesystemAccess,
    persistence: WorkflowPersistenceMode,
) -> WorkflowAccessSnapshot {
    WorkflowAccessSnapshot {
        trust_kind: (trust == WorkflowProjectTrust::Trusted).then_some(ProjectTrustKind::Native),
        trust,
        filesystem_access,
        persistence,
        git_state: WorkflowGitState::Clean,
        authority_revision: "test-authority".into(),
    }
}

fn local_health() -> PrepareWorkflowInput {
    PrepareWorkflowInput {
        kind: WorkflowKind::HealthCheck,
        scope: Some(WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick,
        }),
        route_selection: None,
    }
}

fn prepare(
    service: &WorkflowService,
    context: &ProjectContext,
    access: WorkflowAccessSnapshot,
    settings: &SettingsService,
    secrets: &SecretService,
    agents: &AgentService,
) -> llm_wiki_desktop_lib::models::workflow::WorkflowPreparation {
    service
        .prepare(
            &WorkflowPreparationEnvironment {
                context,
                access,
                settings_service: settings,
                secret_service: secrets,
                agent_service: agents,
            },
            local_health(),
        )
        .unwrap()
}

#[derive(Default)]
struct CountingRunner(AtomicUsize);

impl WorkflowRunner for CountingRunner {
    fn kind(&self) -> WorkflowKind {
        WorkflowKind::HealthCheck
    }

    fn start(&self, _run: WorkflowRun) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn overview_is_fixed_order_and_no_project_is_actionable() {
    let service = WorkflowService::default();
    let overview = service.no_project_overview();
    assert!(overview.project_access.is_none());
    assert_eq!(overview.rows.len(), 3);
    assert_eq!(overview.rows[0].kind, WorkflowKind::UpdateWiki);
    assert_eq!(overview.rows[1].kind, WorkflowKind::HealthCheck);
    assert_eq!(overview.rows[2].kind, WorkflowKind::GenerateContent);
    assert!(overview.rows.iter().all(|row| row.prerequisite.is_some()));

    let (_root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let tasks = TaskService::default();
    let project_overview = service
        .project_overview(
            &context,
            access(
                WorkflowProjectTrust::Untrusted,
                WorkflowFilesystemAccess::ReadOnly,
                WorkflowPersistenceMode::MemoryOnly,
            ),
            &settings,
            &secrets,
            &agents,
            &tasks,
        )
        .unwrap();
    assert_eq!(project_overview.rows.len(), 3);
    assert_eq!(
        project_overview
            .rows
            .iter()
            .filter(|row| row.recommended)
            .count(),
        1
    );
    assert_eq!(
        project_overview.rows[0]
            .prerequisite
            .as_ref()
            .map(|item| &item.action),
        Some(&llm_wiki_desktop_lib::models::workflow::WorkflowPrerequisiteAction::ImportSources)
    );
}

#[test]
fn untrusted_local_quick_is_memory_only_and_does_not_write_workflow_state() {
    let (root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let service = WorkflowService::default();
    let preparation = prepare(
        &service,
        &context,
        access(
            WorkflowProjectTrust::Untrusted,
            WorkflowFilesystemAccess::ReadOnly,
            WorkflowPersistenceMode::MemoryOnly,
        ),
        &settings,
        &secrets,
        &agents,
    );
    assert!(preparation.prerequisites.is_empty());
    assert!(matches!(
        preparation.route,
        Some(llm_wiki_desktop_lib::models::workflow::WorkflowRoute::Local { .. })
    ));
    assert_eq!(preparation.available_wiki_pages, vec!["wiki/概览.md"]);
    assert!(!root.path().join(".app/workflows").exists());
}

#[test]
fn compatible_preparation_reads_mixed_pages_and_excludes_source_only_roots() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".obsidian")).unwrap();
    std::fs::create_dir_all(root.path().join("notes")).unwrap();
    std::fs::create_dir_all(root.path().join("sources")).unwrap();
    std::fs::write(root.path().join("index.md"), "# Index\n").unwrap();
    std::fs::write(root.path().join("notes/shared.md"), "# Shared\n").unwrap();
    std::fs::write(root.path().join("sources/material.md"), "# Material\n").unwrap();
    let context = ProjectContext::new("compatible-preparation", root.path().to_path_buf())
        .with_resolved_layout()
        .unwrap();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let service = WorkflowService::default();

    let preparation = prepare(
        &service,
        &context,
        access(
            WorkflowProjectTrust::Untrusted,
            WorkflowFilesystemAccess::ReadOnly,
            WorkflowPersistenceMode::MemoryOnly,
        ),
        &settings,
        &secrets,
        &agents,
    );

    assert_eq!(
        preparation.available_wiki_pages,
        vec!["index.md", "notes/shared.md"]
    );
    assert!(!root.path().join(".app").exists());
}

#[test]
fn unavailable_runner_fails_before_task_creation() {
    let (_root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let service = WorkflowService::default();
    let tasks = TaskService::default();
    let snapshot = access(
        WorkflowProjectTrust::Untrusted,
        WorkflowFilesystemAccess::ReadOnly,
        WorkflowPersistenceMode::MemoryOnly,
    );
    let preparation = prepare(
        &service,
        &context,
        snapshot.clone(),
        &settings,
        &secrets,
        &agents,
    );
    let error = service
        .start(
            &context,
            snapshot,
            &settings,
            &secrets,
            &agents,
            &tasks,
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_RUNNER_UNAVAILABLE");
    assert!(tasks.list_workflow_runs().is_empty());
}

#[test]
fn validated_start_deduplicates_and_enables_quick_rerun() {
    let (_root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let service = WorkflowService::default();
    let tasks = TaskService::default();
    let runner = Arc::new(CountingRunner::default());
    service.register_runner(runner.clone()).unwrap();
    let snapshot = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::Writable,
        WorkflowPersistenceMode::Persistent,
    );
    let preparation = prepare(
        &service,
        &context,
        snapshot.clone(),
        &settings,
        &secrets,
        &agents,
    );
    let first = service
        .start(
            &context,
            snapshot.clone(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap();
    let first_task_id = match first {
        WorkflowStartOutcome::Created { run } => run.task_id,
        WorkflowStartOutcome::Existing { .. } => panic!("first start must create"),
    };
    let independently_prepared = prepare(
        &service,
        &context,
        snapshot.clone(),
        &settings,
        &secrets,
        &agents,
    );
    let independently_started = service
        .start(
            &context,
            snapshot.clone(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &independently_prepared.preparation_id,
            &independently_prepared.preparation_revision,
        )
        .unwrap();
    assert!(matches!(
        independently_started,
        WorkflowStartOutcome::Existing { .. }
    ));
    let second = service
        .start(
            &context,
            snapshot.clone(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap();
    assert!(matches!(second, WorkflowStartOutcome::Existing { .. }));
    assert_eq!(runner.0.load(Ordering::SeqCst), 1);
    let (_other_root, other_context) = project();
    let cross_project = service
        .start(
            &other_context,
            snapshot.clone(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(cross_project.code, "WORKFLOW_PREPARATION_STALE");
    service.coordinator.cancel(&tasks, &first_task_id).unwrap();
    service
        .coordinator
        .finish_cancelled_and_claim_next(&tasks, &first_task_id)
        .unwrap();
    let replay = service
        .start(
            &context,
            snapshot.clone(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap();
    match replay {
        WorkflowStartOutcome::Existing { run } => assert_eq!(run.task_id, first_task_id),
        WorkflowStartOutcome::Created { .. } => panic!("preparation token was replayed"),
    }
    assert!(
        prepare(
            &service,
            &ProjectContext::new("new-runtime-handle", context.root.clone()),
            snapshot,
            &settings,
            &secrets,
            &agents
        )
        .quick_rerun_eligible
    );
}

#[test]
fn queued_runs_are_not_dispatched_before_the_coordinator_claims_them() {
    let (root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let service = WorkflowService::default();
    let tasks = TaskService::default();
    let runner = Arc::new(CountingRunner::default());
    service.register_runner(runner.clone()).unwrap();
    let snapshot = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::Writable,
        WorkflowPersistenceMode::Persistent,
    );
    let first = prepare(
        &service,
        &context,
        snapshot.clone(),
        &settings,
        &secrets,
        &agents,
    );
    service
        .start(
            &context,
            snapshot.clone(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &first.preparation_id,
            &first.preparation_revision,
        )
        .unwrap();
    std::fs::write(root.path().join("wiki/概览.md"), "# changed baseline\n").unwrap();
    let second = prepare(&service, &context, snapshot, &settings, &secrets, &agents);
    let outcome = service
        .start(
            &context,
            access(
                WorkflowProjectTrust::Trusted,
                WorkflowFilesystemAccess::Writable,
                WorkflowPersistenceMode::Persistent,
            ),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &second.preparation_id,
            &second.preparation_revision,
        )
        .unwrap();
    match outcome {
        WorkflowStartOutcome::Created { run } => assert_eq!(
            run.display_status,
            llm_wiki_desktop_lib::models::workflow::WorkflowDisplayStatus::Queued
        ),
        WorkflowStartOutcome::Existing { .. } => panic!("changed baseline must queue"),
    }
    assert_eq!(runner.0.load(Ordering::SeqCst), 1);
}

#[test]
fn changed_baseline_or_access_invalidates_the_token() {
    let (root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let service = WorkflowService::default();
    service
        .register_runner(Arc::new(CountingRunner::default()))
        .unwrap();
    let tasks = TaskService::default();
    let trusted = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::Writable,
        WorkflowPersistenceMode::Persistent,
    );
    let baseline = prepare(
        &service,
        &context,
        trusted.clone(),
        &settings,
        &secrets,
        &agents,
    );
    std::fs::write(root.path().join("wiki/概览.md"), "# 已修改\n").unwrap();
    let error = service
        .start(
            &context,
            trusted.clone(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &baseline.preparation_id,
            &baseline.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_PREPARATION_STALE");

    let access_token = prepare(
        &service,
        &context,
        trusted.clone(),
        &settings,
        &secrets,
        &agents,
    );
    let error = service
        .start(
            &context,
            access(
                WorkflowProjectTrust::Untrusted,
                WorkflowFilesystemAccess::ReadOnly,
                WorkflowPersistenceMode::MemoryOnly,
            ),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &access_token.preparation_id,
            &access_token.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_PREPARATION_STALE");

    let authority_token = prepare(
        &service,
        &context,
        trusted.clone(),
        &settings,
        &secrets,
        &agents,
    );
    let mut changed_authority = trusted;
    changed_authority.authority_revision = "replacement-authority".into();
    let error = service
        .start(
            &context,
            changed_authority,
            &settings,
            &secrets,
            &agents,
            &tasks,
            &authority_token.preparation_id,
            &authority_token.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_PREPARATION_STALE");
}

#[test]
fn preferences_round_trip_unicode_and_reject_unknown_workflow_kinds() {
    let (root, context) = project();
    let identity = project_identity(&context.root).unwrap();
    let preferences = WorkflowPreferences::default();
    preferences
        .remember(
            &context,
            &identity.canonical_identity_key,
            &identity.identity_revision,
            &WorkflowPersistenceMode::Persistent,
            WorkflowPreference {
                kind: WorkflowKind::GenerateContent,
                scope: WorkflowScope::GenerateContent {
                    artifact_type: WorkflowArtifactType::BeautifulRead,
                    page_paths: vec!["wiki/概览.md".into()],
                    output_path: Some("exports/html/概览.html".into()),
                },
                route: Some(
                    llm_wiki_desktop_lib::models::workflow::WorkflowRoute::Local {
                        route_revision: "local-v1".into(),
                    },
                ),
                baseline_fingerprint: "a".repeat(64),
                preparation_fingerprint: "b".repeat(64),
                saved_at: "ignored".into(),
            },
        )
        .unwrap();
    let disk =
        std::fs::read_to_string(root.path().join(".app/workflows/preferences.json")).unwrap();
    assert!(disk.contains("wiki/概览.md"));
    assert_eq!(
        preferences
            .load(
                &context,
                &identity.canonical_identity_key,
                &identity.identity_revision,
                &WorkflowPersistenceMode::Persistent,
            )
            .unwrap()
            .len(),
        1
    );

    std::fs::write(
        root.path().join(".app/workflows/preferences.json"),
        r#"{"schemaVersion":1,"entries":[{"kind":"future_kind","scope":{"kind":"health_check","mode":"local_quick"},"route":null,"baselineFingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","preparationFingerprint":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","savedAt":"2026-08-01T00:00:00Z"}]}"#,
    )
    .unwrap();
    assert!(preferences
        .load(
            &context,
            &identity.canonical_identity_key,
            &identity.identity_revision,
            &WorkflowPersistenceMode::Persistent,
        )
        .is_err());
}

#[test]
fn stale_remembered_page_falls_back_without_breaking_overview() {
    let (root, context) = project();
    let identity = project_identity(&context.root).unwrap();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let service = WorkflowService::default();
    service
        .preferences
        .remember(
            &context,
            &identity.canonical_identity_key,
            &identity.identity_revision,
            &WorkflowPersistenceMode::Persistent,
            WorkflowPreference {
                kind: WorkflowKind::GenerateContent,
                scope: WorkflowScope::GenerateContent {
                    artifact_type: WorkflowArtifactType::BeautifulRead,
                    page_paths: vec!["wiki/概览.md".into()],
                    output_path: Some("exports/html/概览.html".into()),
                },
                route: None,
                baseline_fingerprint: "a".repeat(64),
                preparation_fingerprint: "b".repeat(64),
                saved_at: String::new(),
            },
        )
        .unwrap();
    std::fs::remove_file(root.path().join("wiki/概览.md")).unwrap();
    let overview = service
        .project_overview(
            &context,
            access(
                WorkflowProjectTrust::Trusted,
                WorkflowFilesystemAccess::Writable,
                WorkflowPersistenceMode::Persistent,
            ),
            &settings,
            &secrets,
            &agents,
            &TaskService::default(),
        )
        .unwrap();
    assert_eq!(overview.rows.len(), 3);
    let prepared = service
        .prepare(
            &WorkflowPreparationEnvironment {
                context: &context,
                access: access(
                    WorkflowProjectTrust::Trusted,
                    WorkflowFilesystemAccess::Writable,
                    WorkflowPersistenceMode::Persistent,
                ),
                settings_service: &settings,
                secret_service: &secrets,
                agent_service: &agents,
            },
            PrepareWorkflowInput {
                kind: WorkflowKind::GenerateContent,
                scope: None,
                route_selection: None,
            },
        )
        .unwrap();
    assert!(!prepared.quick_rerun_eligible);
    assert!(matches!(
        prepared.scope,
        WorkflowScope::GenerateContent {
            artifact_type: WorkflowArtifactType::ProjectReport,
            ..
        }
    ));
}
