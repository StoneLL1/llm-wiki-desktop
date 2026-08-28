use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::agent::AgentKind;
use llm_wiki_desktop_lib::models::llm::{
    LlmProviderConfig, LlmProviderKind, ProviderCredentialBinding,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::project::ProjectTrustKind;
use llm_wiki_desktop_lib::models::settings::Settings;
use llm_wiki_desktop_lib::models::workflow::{
    HealthCheckMode, WorkflowArtifactType, WorkflowFilesystemAccess, WorkflowGitState,
    WorkflowKind, WorkflowPersistenceMode, WorkflowPrerequisiteAction, WorkflowProjectTrust,
    WorkflowRouteSelection, WorkflowRun, WorkflowScope, WorkflowStartOutcome,
};
use llm_wiki_desktop_lib::services::{
    project_identity, resolve_workflow_persistence_binding, AgentInvocation, AgentService,
    PrepareWorkflowInput, ProcessRunner, SecretService, SettingsService, WorkflowAccessSnapshot,
    WorkflowPreference, WorkflowPreferences, WorkflowPreparationEnvironment, WorkflowRunner,
    WorkflowService,
};
use llm_wiki_desktop_lib::tasks::TaskService;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

struct NoAgents;

struct MutableCodex {
    generation: AtomicUsize,
    invocations: AtomicUsize,
}

impl ProcessRunner for NoAgents {
    fn find_executable(&self, _command: &str) -> Option<PathBuf> {
        None
    }

    fn run_with_timeout(
        &self,
        _command: &str,
        _args: &[&str],
        _timeout: Duration,
    ) -> Result<String, BackendError> {
        Ok(String::new())
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        Ok((String::new(), String::new()))
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        Ok(String::new())
    }
}

impl ProcessRunner for MutableCodex {
    fn find_executable(&self, command: &str) -> Option<PathBuf> {
        (command == "codex").then(|| {
            std::env::current_exe()
                .expect("the workflow preparation test executable must be resolvable")
        })
    }

    fn run_with_timeout(
        &self,
        _command: &str,
        args: &[&str],
        _timeout: Duration,
    ) -> Result<String, BackendError> {
        if args == ["--version"] {
            return Ok(format!(
                "codex {}.0.0",
                self.generation.load(Ordering::SeqCst)
            ));
        }
        Ok("--json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check -C --cd".into())
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        Ok((String::new(), String::new()))
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        Ok("[]".into())
    }
}

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

#[test]
fn persistence_binding_consumes_layout_without_creating_state() {
    let root = tempfile::tempdir().unwrap();
    let mut context = ProjectContext::new("项目-一", root.path().to_path_buf());
    context.layout.task_state_root = Some(".app/任务".into());

    let binding =
        resolve_workflow_persistence_binding(&context, WorkflowPersistenceMode::Persistent)
            .unwrap();

    assert_eq!(binding.mode, WorkflowPersistenceMode::Persistent);
    assert_eq!(binding.task_state_root, Some(root.path().join(".app/任务")));
    assert!(!root.path().join(".app").exists());
}

#[test]
fn missing_layout_task_root_fails_closed_to_memory_only() {
    let root = tempfile::tempdir().unwrap();
    let mut context = ProjectContext::new("project", root.path().to_path_buf());
    context.layout.task_state_root = None;

    let binding =
        resolve_workflow_persistence_binding(&context, WorkflowPersistenceMode::Persistent)
            .unwrap();

    assert_eq!(binding.mode, WorkflowPersistenceMode::MemoryOnly);
    assert!(binding.task_state_root.is_none());
    assert!(std::fs::read_dir(root.path()).unwrap().next().is_none());
}

#[test]
fn trusted_read_only_authority_stays_memory_only_without_creating_task_state() {
    let root = tempfile::tempdir().unwrap();
    let mut context = ProjectContext::new("project", root.path().to_path_buf());
    context.layout.task_state_root = Some(".app/tasks".into());
    let access = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::ReadOnly,
        WorkflowPersistenceMode::MemoryOnly,
    );

    let binding = resolve_workflow_persistence_binding(&context, access.persistence).unwrap();

    assert_eq!(binding.mode, WorkflowPersistenceMode::MemoryOnly);
    assert_eq!(binding.task_state_root, None);
    assert!(!root.path().join(".app").exists());
}

#[test]
fn agent_version_change_after_prepare_fails_start_before_dispatch_or_invocation() {
    let (_root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    settings
        .save_settings(
            &context,
            &Settings {
                agent_default: Some(AgentKind::Codex),
                ..Settings::default()
            },
        )
        .unwrap();
    let secrets = SecretService::memory();
    let runner = Arc::new(MutableCodex {
        generation: AtomicUsize::new(1),
        invocations: AtomicUsize::new(0),
    });
    let agents = AgentService::with_runner(runner.clone());
    let service = WorkflowService::default();
    let dispatches = Arc::new(CountingRunner::default());
    service.register_runner(dispatches.clone()).unwrap();
    let tasks = TaskService::default();
    let project_access = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::ReadOnly,
        WorkflowPersistenceMode::MemoryOnly,
    );
    let preparation = service
        .prepare(
            &WorkflowPreparationEnvironment {
                context: &context,
                access: project_access.clone(),
                settings_service: &settings,
                secret_service: &secrets,
                agent_service: &agents,
            },
            PrepareWorkflowInput {
                kind: WorkflowKind::HealthCheck,
                scope: Some(WorkflowScope::HealthCheck {
                    mode: HealthCheckMode::Complete,
                }),
                route_selection: Some(WorkflowRouteSelection::Agent {
                    agent: AgentKind::Codex,
                }),
            },
        )
        .unwrap();
    runner.generation.store(2, Ordering::SeqCst);

    let error = service
        .start(
            &context,
            project_access,
            &settings,
            &secrets,
            &agents,
            &tasks,
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap_err();

    assert_eq!(error.code, "WORKFLOW_PREPARATION_STALE");
    assert_eq!(dispatches.0.load(Ordering::SeqCst), 0);
    assert_eq!(runner.invocations.load(Ordering::SeqCst), 0);
    assert!(tasks.list_workflow_runs().is_empty());
}

#[test]
fn complete_health_preserves_persistent_project_access_for_repair_ownership() {
    let (_root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    settings
        .save_settings(
            &context,
            &Settings {
                agent_default: Some(AgentKind::Codex),
                ..Settings::default()
            },
        )
        .unwrap();
    let secrets = SecretService::memory();
    let runner = Arc::new(MutableCodex {
        generation: AtomicUsize::new(1),
        invocations: AtomicUsize::new(0),
    });
    let agents = AgentService::with_runner(runner);
    let service = WorkflowService::default();
    service
        .register_runner(Arc::new(CountingRunner::default()))
        .unwrap();
    let tasks = TaskService::default();
    let project_access = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::Writable,
        WorkflowPersistenceMode::Persistent,
    );
    let preparation = service
        .prepare(
            &WorkflowPreparationEnvironment {
                context: &context,
                access: project_access.clone(),
                settings_service: &settings,
                secret_service: &secrets,
                agent_service: &agents,
            },
            PrepareWorkflowInput {
                kind: WorkflowKind::HealthCheck,
                scope: Some(WorkflowScope::HealthCheck {
                    mode: HealthCheckMode::Complete,
                }),
                route_selection: Some(WorkflowRouteSelection::Agent {
                    agent: AgentKind::Codex,
                }),
            },
        )
        .unwrap();

    assert_eq!(
        preparation.project_access.persistence,
        WorkflowPersistenceMode::Persistent
    );
    let outcome = service
        .start(
            &context,
            project_access,
            &settings,
            &secrets,
            &agents,
            &tasks,
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap();
    let run = match outcome {
        WorkflowStartOutcome::Created { run } => run,
        WorkflowStartOutcome::Existing { .. } => panic!("first start must create"),
    };
    assert_eq!(run.persistence, WorkflowPersistenceMode::Persistent);
    assert!(context.root.join(".app/tasks").is_dir());
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

#[derive(Default)]
struct GenerateCountingRunner(AtomicUsize);

impl WorkflowRunner for GenerateCountingRunner {
    fn kind(&self) -> WorkflowKind {
        WorkflowKind::GenerateContent
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
fn generate_content_artifact_types_keep_their_page_scope_contracts() {
    let (root, context) = project();
    std::fs::write(root.path().join("wiki/第二页.md"), "# 第二页\n").unwrap();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::with_runner(Arc::new(NoAgents));
    let service = WorkflowService::default();
    let project_access = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::Writable,
        WorkflowPersistenceMode::Persistent,
    );
    let environment = WorkflowPreparationEnvironment {
        context: &context,
        access: project_access,
        settings_service: &settings,
        secret_service: &secrets,
        agent_service: &agents,
    };

    let valid_cases = [
        (
            WorkflowArtifactType::BeautifulRead,
            vec!["wiki/概览.md".into()],
        ),
        (
            WorkflowArtifactType::KnowledgeCard,
            vec!["wiki/概览.md".into(), "wiki/第二页.md".into()],
        ),
        (
            WorkflowArtifactType::ConceptMap,
            vec!["wiki/概览.md".into(), "wiki/第二页.md".into()],
        ),
        (WorkflowArtifactType::ProjectReport, Vec::new()),
    ];
    for (artifact_type, page_paths) in valid_cases {
        let preparation = service
            .prepare(
                &environment,
                PrepareWorkflowInput {
                    kind: WorkflowKind::GenerateContent,
                    scope: Some(WorkflowScope::GenerateContent {
                        artifact_type: artifact_type.clone(),
                        page_paths: page_paths.clone(),
                        output_path: None,
                    }),
                    route_selection: None,
                },
            )
            .unwrap();
        match preparation.scope {
            WorkflowScope::GenerateContent {
                artifact_type: prepared_artifact_type,
                page_paths: prepared_page_paths,
                output_path,
            } => {
                assert_eq!(prepared_artifact_type, artifact_type);
                assert_eq!(prepared_page_paths, page_paths);
                assert!(output_path.is_some());
            }
            _ => unreachable!(),
        }
    }

    let invalid_cases = [
        (WorkflowArtifactType::BeautifulRead, Vec::new()),
        (
            WorkflowArtifactType::BeautifulRead,
            vec!["wiki/概览.md".into(), "wiki/第二页.md".into()],
        ),
        (WorkflowArtifactType::KnowledgeCard, Vec::new()),
        (WorkflowArtifactType::ConceptMap, Vec::new()),
        (
            WorkflowArtifactType::ProjectReport,
            vec!["wiki/概览.md".into()],
        ),
    ];
    for (artifact_type, page_paths) in invalid_cases {
        let error = service
            .prepare(
                &environment,
                PrepareWorkflowInput {
                    kind: WorkflowKind::GenerateContent,
                    scope: Some(WorkflowScope::GenerateContent {
                        artifact_type,
                        page_paths,
                        output_path: None,
                    }),
                    route_selection: None,
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "WORKFLOW_ARTIFACT_SCOPE_INVALID");
    }
}

#[test]
fn empty_project_surfaces_import_and_update_prerequisites_without_inventing_content() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".app")).unwrap();
    let context = ProjectContext::new("empty-project", root.path().to_path_buf());
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let overview = WorkflowService::default()
        .project_overview(
            &context,
            access(
                WorkflowProjectTrust::Untrusted,
                WorkflowFilesystemAccess::ReadOnly,
                WorkflowPersistenceMode::MemoryOnly,
            ),
            &settings,
            &SecretService::memory(),
            &AgentService::default(),
            &TaskService::default(),
        )
        .unwrap();

    assert_eq!(overview.rows.len(), 3);
    assert_eq!(
        overview.rows[0]
            .prerequisite
            .as_ref()
            .map(|item| &item.action),
        Some(&WorkflowPrerequisiteAction::ImportSources)
    );
    assert_eq!(
        overview.rows[1]
            .prerequisite
            .as_ref()
            .map(|item| &item.action),
        Some(&WorkflowPrerequisiteAction::ImportSources)
    );
    assert_eq!(
        overview.rows[2]
            .prerequisite
            .as_ref()
            .map(|item| &item.action),
        Some(&WorkflowPrerequisiteAction::UpdateWiki)
    );
    assert!(!root.path().join("wiki").exists());
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
fn concurrent_same_preparation_start_is_always_idempotent() {
    let (_root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = Arc::new(SettingsService::with_config_dir(
        config.path().to_path_buf(),
    ));
    let secrets = Arc::new(SecretService::memory());
    let agents = Arc::new(AgentService::with_runner(Arc::new(NoAgents)));
    let service = Arc::new(WorkflowService::default());
    let tasks = Arc::new(TaskService::default());
    service
        .register_runner(Arc::new(CountingRunner::default()))
        .unwrap();
    let context = Arc::new(context);
    let trusted = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::Writable,
        WorkflowPersistenceMode::Persistent,
    );
    let preparation = prepare(
        &service,
        &context,
        trusted.clone(),
        &settings,
        &secrets,
        &agents,
    );
    let workers = 24;
    let barrier = Arc::new(Barrier::new(workers));
    let handles = (0..workers)
        .map(|_| {
            let service = Arc::clone(&service);
            let tasks = Arc::clone(&tasks);
            let context = Arc::clone(&context);
            let settings = Arc::clone(&settings);
            let secrets = Arc::clone(&secrets);
            let agents = Arc::clone(&agents);
            let barrier = Arc::clone(&barrier);
            let trusted = trusted.clone();
            let preparation_id = preparation.preparation_id.clone();
            let preparation_revision = preparation.preparation_revision.clone();
            std::thread::spawn(move || {
                barrier.wait();
                service.start(
                    &context,
                    trusted,
                    &settings,
                    &secrets,
                    &agents,
                    &tasks,
                    &preparation_id,
                    &preparation_revision,
                )
            })
        })
        .collect::<Vec<_>>();

    let outcomes = handles
        .into_iter()
        .map(|handle| handle.join().unwrap().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, WorkflowStartOutcome::Created { .. }))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, WorkflowStartOutcome::Existing { .. }))
            .count(),
        workers - 1
    );
    assert_eq!(tasks.list_workflow_runs().len(), 1);
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
fn dirty_git_blocks_overwrite_until_the_host_supplies_remediated_access_and_reprepares() {
    let (root, context) = project();
    let output_path = "exports/html/beautiful-read.html";
    std::fs::create_dir_all(root.path().join("exports/html")).unwrap();
    std::fs::write(root.path().join(output_path), "existing artifact").unwrap();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let provider = LlmProviderConfig {
        provider: LlmProviderKind::Ollama,
        model: "qwen".into(),
        base_url: "http://127.0.0.1:11434".into(),
        context_window: 8192,
        enabled: true,
    };
    let target = llm_wiki_desktop_lib::services::import_v2::url_policy::UrlPolicy
        .normalize_provider_endpoint(&provider.base_url)
        .unwrap();
    let canonical_origin =
        llm_wiki_desktop_lib::services::import_v2::url_policy::UrlPolicy.canonical_origin(&target);
    let config_id = uuid::Uuid::new_v4().to_string();
    let binding = ProviderCredentialBinding {
        credential_account_id: SecretService::provider_binding_account_id(
            &context,
            provider.provider,
            &config_id,
            &canonical_origin,
            1,
        )
        .unwrap(),
        config_id,
        provider_kind: provider.provider,
        canonical_origin,
        approved_at: None,
        revision: 1,
    };
    settings
        .save_provider_with_binding(&context, provider, binding)
        .unwrap();
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let service = WorkflowService::default();
    let runner = Arc::new(GenerateCountingRunner::default());
    service.register_runner(runner.clone()).unwrap();
    let tasks = TaskService::default();
    let mut dirty = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::Writable,
        WorkflowPersistenceMode::Persistent,
    );
    dirty.git_state = WorkflowGitState::Dirty;
    let input = PrepareWorkflowInput {
        kind: WorkflowKind::GenerateContent,
        scope: Some(WorkflowScope::GenerateContent {
            artifact_type: WorkflowArtifactType::BeautifulRead,
            page_paths: vec!["wiki/概览.md".into()],
            output_path: Some(output_path.into()),
        }),
        route_selection: None,
    };
    let blocked = service
        .prepare(
            &WorkflowPreparationEnvironment {
                context: &context,
                access: dirty.clone(),
                settings_service: &settings,
                secret_service: &secrets,
                agent_service: &agents,
            },
            input.clone(),
        )
        .unwrap();
    assert!(blocked.prerequisites.iter().any(|item| {
        item.action == WorkflowPrerequisiteAction::ResolveDirtyGit && item.blocking
    }));
    let error = service
        .start(
            &context,
            dirty,
            &settings,
            &secrets,
            &agents,
            &tasks,
            &blocked.preparation_id,
            &blocked.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_PREREQUISITES_BLOCKING");
    assert_eq!(runner.0.load(Ordering::SeqCst), 0);
    assert_eq!(
        std::fs::read_to_string(root.path().join(output_path)).unwrap(),
        "existing artifact"
    );
    assert!(!root.path().join(".git").exists());

    let clean = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::Writable,
        WorkflowPersistenceMode::Persistent,
    );
    let ready = service
        .prepare(
            &WorkflowPreparationEnvironment {
                context: &context,
                access: clean.clone(),
                settings_service: &settings,
                secret_service: &secrets,
                agent_service: &agents,
            },
            input,
        )
        .unwrap();
    assert!(ready.prerequisites.is_empty());
    let outcome = service
        .start(
            &context,
            clean,
            &settings,
            &secrets,
            &agents,
            &tasks,
            &ready.preparation_id,
            &ready.preparation_revision,
        )
        .unwrap();
    assert!(matches!(outcome, WorkflowStartOutcome::Created { .. }));
    assert_eq!(runner.0.load(Ordering::SeqCst), 1);
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

fn concurrent_preference(kind: WorkflowKind, marker: usize) -> WorkflowPreference {
    let scope = match kind {
        WorkflowKind::UpdateWiki => WorkflowScope::UpdateWiki {
            mode: llm_wiki_desktop_lib::models::workflow::UpdateWikiMode::ChangedSources,
            source_versions: Vec::new(),
        },
        WorkflowKind::HealthCheck => WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick,
        },
        WorkflowKind::GenerateContent => WorkflowScope::GenerateContent {
            artifact_type: WorkflowArtifactType::ProjectReport,
            page_paths: Vec::new(),
            output_path: Some(format!("exports/html/report-{marker}.html")),
        },
    };
    WorkflowPreference {
        kind,
        scope,
        route: None,
        baseline_fingerprint: format!("{marker:064x}"),
        preparation_fingerprint: format!("{:064x}", marker + 1),
        saved_at: String::new(),
    }
}

#[test]
fn concurrent_preference_remember_preserves_all_workflow_kinds() {
    let root = tempfile::tempdir().unwrap();
    let mut context = ProjectContext::new("并发偏好", root.path().to_path_buf());
    context.layout.workflow_state_root = Some(".app/workflows".into());
    let context = Arc::new(context);
    let identity = project_identity(&context.root).unwrap();
    let workers = 48;
    let barrier = Arc::new(Barrier::new(workers));
    let handles = (0..workers)
        .map(|index| {
            let context = Arc::clone(&context);
            let barrier = Arc::clone(&barrier);
            let identity_key = identity.canonical_identity_key.clone();
            let identity_revision = identity.identity_revision.clone();
            std::thread::spawn(move || {
                let kind = match index % 3 {
                    0 => WorkflowKind::UpdateWiki,
                    1 => WorkflowKind::HealthCheck,
                    _ => WorkflowKind::GenerateContent,
                };
                barrier.wait();
                WorkflowPreferences::default()
                    .remember(
                        &context,
                        &identity_key,
                        &identity_revision,
                        &WorkflowPersistenceMode::Persistent,
                        concurrent_preference(kind, index),
                    )
                    .unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }

    let entries = WorkflowPreferences::default()
        .load(
            &context,
            &identity.canonical_identity_key,
            &identity.identity_revision,
            &WorkflowPersistenceMode::Persistent,
        )
        .unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].kind, WorkflowKind::UpdateWiki);
    assert_eq!(entries[1].kind, WorkflowKind::HealthCheck);
    assert_eq!(entries[2].kind, WorkflowKind::GenerateContent);
    assert!(entries
        .iter()
        .all(|entry| chrono::DateTime::parse_from_rfc3339(&entry.saved_at).is_ok()));
}

#[cfg(unix)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn create_directory_link(target: &std::path::Path, link: &std::path::Path) {
    let output = std::process::Command::new("cmd")
        .arg("/c")
        .arg("mklink")
        .arg("/J")
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "junction setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn preference_write_rejects_workflow_root_link_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let link = root.path().join("偏好链接");
    create_directory_link(outside.path(), &link);
    let mut context = ProjectContext::new("link-escape", root.path().to_path_buf());
    context.layout.workflow_state_root = Some("偏好链接".into());
    let identity = project_identity(&context.root).unwrap();
    let error = WorkflowPreferences::default()
        .remember(
            &context,
            &identity.canonical_identity_key,
            &identity.identity_revision,
            &WorkflowPersistenceMode::Persistent,
            concurrent_preference(WorkflowKind::HealthCheck, 1),
        )
        .unwrap_err();

    assert!(matches!(
        error.code.as_str(),
        "PATH_OUTSIDE_PROJECT" | "PATH_UNSAFE_LINK" | "PATH_INVALID"
    ));
    assert!(!outside.path().join("preferences.json").exists());
}

#[test]
fn preparation_capacity_evicts_old_unstarted_tokens_but_keeps_started_replay() {
    let (_root, context) = project();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::with_runner(Arc::new(NoAgents));
    let service = WorkflowService::default();
    let tasks = TaskService::default();
    let runner = Arc::new(CountingRunner::default());
    service.register_runner(runner).unwrap();
    let trusted = access(
        WorkflowProjectTrust::Trusted,
        WorkflowFilesystemAccess::Writable,
        WorkflowPersistenceMode::Persistent,
    );

    let started = prepare(
        &service,
        &context,
        trusted.clone(),
        &settings,
        &secrets,
        &agents,
    );
    let first_outcome = service
        .start(
            &context,
            trusted.clone(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &started.preparation_id,
            &started.preparation_revision,
        )
        .unwrap();
    let first_task_id = match first_outcome {
        WorkflowStartOutcome::Created { run } | WorkflowStartOutcome::Existing { run } => {
            run.task_id
        }
    };
    let oldest_unstarted = prepare(
        &service,
        &context,
        trusted.clone(),
        &settings,
        &secrets,
        &agents,
    );
    for _ in 0..80 {
        let _ = prepare(
            &service,
            &context,
            trusted.clone(),
            &settings,
            &secrets,
            &agents,
        );
    }

    let stale = service.preparation.validate_for_start(
        &WorkflowPreparationEnvironment {
            context: &context,
            access: trusted.clone(),
            settings_service: &settings,
            secret_service: &secrets,
            agent_service: &agents,
        },
        &oldest_unstarted.preparation_id,
        &oldest_unstarted.preparation_revision,
    );
    assert_eq!(stale.unwrap_err().code, "WORKFLOW_PREPARATION_STALE");

    for marker in 0..128 {
        std::fs::write(
            context.root.join("wiki/cap-pressure.md"),
            format!("# cap pressure {marker}\n"),
        )
        .unwrap();
        let preparation = prepare(
            &service,
            &context,
            trusted.clone(),
            &settings,
            &secrets,
            &agents,
        );
        let outcome = service
            .start(
                &context,
                trusted.clone(),
                &settings,
                &secrets,
                &agents,
                &tasks,
                &preparation.preparation_id,
                &preparation.preparation_revision,
            )
            .unwrap();
        assert!(matches!(outcome, WorkflowStartOutcome::Created { .. }));
    }
    let replay = service
        .start(
            &context,
            trusted,
            &settings,
            &secrets,
            &agents,
            &tasks,
            &started.preparation_id,
            &started.preparation_revision,
        )
        .unwrap();
    match replay {
        WorkflowStartOutcome::Existing { run } => assert_eq!(run.task_id, first_task_id),
        WorkflowStartOutcome::Created { .. } => {
            panic!("started preparation replay duplicated a task")
        }
    }

    let other_root = tempfile::tempdir().unwrap();
    let other_context = ProjectContext::new("other-project", other_root.path().to_path_buf());
    let cross_root = service
        .start(
            &other_context,
            access(
                WorkflowProjectTrust::Trusted,
                WorkflowFilesystemAccess::Writable,
                WorkflowPersistenceMode::Persistent,
            ),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &started.preparation_id,
            &started.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(cross_root.code, "WORKFLOW_PREPARATION_STALE");
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
