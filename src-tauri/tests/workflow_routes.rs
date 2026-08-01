use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::agent::AgentKind;
use llm_wiki_desktop_lib::models::llm::{LlmProviderConfig, LlmProviderKind};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::settings::Settings;
use llm_wiki_desktop_lib::models::workflow::{
    HealthCheckMode, WorkflowFilesystemAccess, WorkflowGitState, WorkflowKind,
    WorkflowPersistenceMode, WorkflowPrerequisiteAction, WorkflowProjectTrust, WorkflowRoute,
    WorkflowRouteSelection, WorkflowRun, WorkflowScope,
};
use llm_wiki_desktop_lib::services::{
    workflow_stages, AgentInvocation, AgentService, PrepareWorkflowInput, ProcessRunner,
    SecretService, SettingsService, WorkflowAccessSnapshot, WorkflowPreparationEnvironment,
    WorkflowRunner, WorkflowService,
};
use llm_wiki_desktop_lib::tasks::TaskService;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

struct NoAgents;

struct HealthRunner;

impl WorkflowRunner for HealthRunner {
    fn kind(&self) -> WorkflowKind {
        WorkflowKind::HealthCheck
    }

    fn start(&self, _run: WorkflowRun) {}
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

fn fixture() -> (
    tempfile::TempDir,
    ProjectContext,
    tempfile::TempDir,
    SettingsService,
    SecretService,
    AgentService,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".app")).unwrap();
    std::fs::create_dir_all(root.path().join("wiki")).unwrap();
    std::fs::create_dir_all(root.path().join("raw/extracted")).unwrap();
    std::fs::write(root.path().join("wiki/页面.md"), "# 页面\n").unwrap();
    let context = ProjectContext::new("route-project", root.path().to_path_buf());
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    (
        root,
        context,
        config,
        settings,
        SecretService::memory(),
        AgentService::with_runner(Arc::new(NoAgents)),
    )
}

fn trusted() -> WorkflowAccessSnapshot {
    WorkflowAccessSnapshot {
        trust: WorkflowProjectTrust::Trusted,
        filesystem_access: WorkflowFilesystemAccess::Writable,
        persistence: WorkflowPersistenceMode::Persistent,
        git_state: WorkflowGitState::Clean,
    }
}

fn provider(kind: LlmProviderKind, model: &str) -> LlmProviderConfig {
    LlmProviderConfig {
        provider: kind,
        model: model.into(),
        base_url: "http://127.0.0.1:11434".into(),
        context_window: 8192,
        enabled: true,
    }
}

fn save_routes(
    context: &ProjectContext,
    settings_service: &SettingsService,
    default_agent: Option<AgentKind>,
    providers: Vec<LlmProviderConfig>,
) {
    let settings = Settings {
        agent_default: default_agent,
        llm_providers: providers,
        ..Settings::default()
    };
    settings_service.save_settings(context, &settings).unwrap();
}

fn prepare(
    service: &WorkflowService,
    context: &ProjectContext,
    settings: &SettingsService,
    secrets: &SecretService,
    agents: &AgentService,
    access: WorkflowAccessSnapshot,
    scope: Option<WorkflowScope>,
    route_selection: Option<WorkflowRouteSelection>,
) -> Result<llm_wiki_desktop_lib::models::workflow::WorkflowPreparation, BackendError> {
    service.prepare(
        &WorkflowPreparationEnvironment {
            context,
            access,
            settings_service: settings,
            secret_service: secrets,
            agent_service: agents,
        },
        PrepareWorkflowInput {
            kind: WorkflowKind::HealthCheck,
            scope,
            route_selection,
        },
    )
}

#[test]
fn health_default_is_complete_only_when_trusted_route_is_available() {
    let (_root, context, _config, settings, secrets, agents) = fixture();
    save_routes(
        &context,
        &settings,
        None,
        vec![provider(LlmProviderKind::Ollama, "qwen")],
    );
    let service = WorkflowService::default();
    let complete = prepare(
        &service,
        &context,
        &settings,
        &secrets,
        &agents,
        trusted(),
        None,
        None,
    )
    .unwrap();
    assert!(matches!(
        complete.scope,
        WorkflowScope::HealthCheck {
            mode: HealthCheckMode::Complete
        }
    ));
    assert!(matches!(complete.route, Some(WorkflowRoute::Byok { .. })));

    let untrusted = WorkflowAccessSnapshot {
        trust: WorkflowProjectTrust::Untrusted,
        filesystem_access: WorkflowFilesystemAccess::ReadOnly,
        persistence: WorkflowPersistenceMode::MemoryOnly,
        git_state: WorkflowGitState::Clean,
    };
    let local = prepare(
        &service, &context, &settings, &secrets, &agents, untrusted, None, None,
    )
    .unwrap();
    assert!(matches!(
        local.scope,
        WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick
        }
    ));
}

#[test]
fn configured_default_agent_never_falls_back_to_byok() {
    let (_root, context, _config, settings, secrets, agents) = fixture();
    save_routes(
        &context,
        &settings,
        Some(AgentKind::Codex),
        vec![provider(LlmProviderKind::Ollama, "qwen")],
    );
    let preparation = prepare(
        &WorkflowService::default(),
        &context,
        &settings,
        &secrets,
        &agents,
        trusted(),
        Some(WorkflowScope::HealthCheck {
            mode: HealthCheckMode::Complete,
        }),
        None,
    )
    .unwrap();
    assert!(preparation.route.is_none());
    assert!(preparation
        .prerequisites
        .iter()
        .any(|item| { item.action == WorkflowPrerequisiteAction::ConfigureExecutionRoute }));
}

#[test]
fn ambiguous_providers_require_choice_and_explicit_selection_is_exact() {
    let (_root, context, _config, settings, secrets, agents) = fixture();
    secrets
        .set(LlmProviderKind::OpenAi, "sk-route-secret")
        .unwrap();
    save_routes(
        &context,
        &settings,
        None,
        vec![
            provider(LlmProviderKind::OpenAi, "gpt-route"),
            provider(LlmProviderKind::Ollama, "qwen-route"),
        ],
    );
    let service = WorkflowService::default();
    let scope = Some(WorkflowScope::HealthCheck {
        mode: HealthCheckMode::Complete,
    });
    let ambiguous = prepare(
        &service,
        &context,
        &settings,
        &secrets,
        &agents,
        trusted(),
        scope.clone(),
        None,
    )
    .unwrap();
    assert!(ambiguous.route.is_none());
    assert!(ambiguous
        .prerequisites
        .iter()
        .any(|item| item.action == WorkflowPrerequisiteAction::ChooseExecutionRoute));

    let explicit = prepare(
        &service,
        &context,
        &settings,
        &secrets,
        &agents,
        trusted(),
        scope,
        Some(WorkflowRouteSelection::Byok {
            provider: LlmProviderKind::OpenAi,
        }),
    )
    .unwrap();
    assert!(matches!(
        explicit.route,
        Some(WorkflowRoute::Byok {
            provider: LlmProviderKind::OpenAi,
            ..
        })
    ));
}

#[test]
fn route_revision_change_invalidates_start_and_serialization_is_secret_free() {
    let (root, context, _config, settings, secrets, agents) = fixture();
    let marker = "sk-never-serialize-this";
    secrets.set(LlmProviderKind::OpenAi, marker).unwrap();
    save_routes(
        &context,
        &settings,
        None,
        vec![provider(LlmProviderKind::OpenAi, "gpt-before")],
    );
    let service = WorkflowService::default();
    let preparation = prepare(
        &service,
        &context,
        &settings,
        &secrets,
        &agents,
        trusted(),
        Some(WorkflowScope::HealthCheck {
            mode: HealthCheckMode::Complete,
        }),
        Some(WorkflowRouteSelection::Byok {
            provider: LlmProviderKind::OpenAi,
        }),
    )
    .unwrap();
    let json = serde_json::to_string(&preparation).unwrap();
    assert!(!json.contains(marker));
    assert!(!json.contains(&root.path().to_string_lossy().to_string()));

    save_routes(
        &context,
        &settings,
        None,
        vec![provider(LlmProviderKind::OpenAi, "gpt-after")],
    );
    let error = service
        .start(
            &context,
            trusted(),
            &settings,
            &secrets,
            &agents,
            &TaskService::default(),
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_PREPARATION_STALE");
}

#[test]
fn stage_skeletons_have_stable_order_and_counts() {
    for (kind, count) in [
        (WorkflowKind::UpdateWiki, 9),
        (WorkflowKind::HealthCheck, 8),
        (WorkflowKind::GenerateContent, 9),
    ] {
        let stages = workflow_stages(&kind);
        assert_eq!(stages.len(), count);
        assert!(stages
            .iter()
            .enumerate()
            .all(|(index, stage)| stage.ordinal == index as u32 + 1));
    }
}

#[test]
fn later_health_preparation_remembers_the_last_confirmed_mode() {
    let (_root, context, _config, settings, secrets, agents) = fixture();
    save_routes(
        &context,
        &settings,
        None,
        vec![provider(LlmProviderKind::Ollama, "qwen")],
    );
    let service = WorkflowService::default();
    service.register_runner(Arc::new(HealthRunner)).unwrap();
    let first = prepare(
        &service,
        &context,
        &settings,
        &secrets,
        &agents,
        trusted(),
        Some(WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick,
        }),
        None,
    )
    .unwrap();
    service
        .start(
            &context,
            trusted(),
            &settings,
            &secrets,
            &agents,
            &TaskService::default(),
            &first.preparation_id,
            &first.preparation_revision,
        )
        .unwrap();
    let remembered = prepare(
        &service,
        &context,
        &settings,
        &secrets,
        &agents,
        trusted(),
        None,
        None,
    )
    .unwrap();
    assert!(matches!(
        remembered.scope,
        WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick
        }
    ));
    assert!(remembered.quick_rerun_eligible);
}

#[test]
fn overview_uses_remembered_health_mode_when_the_route_disappears() {
    let (_root, context, _config, settings, secrets, agents) = fixture();
    save_routes(
        &context,
        &settings,
        None,
        vec![provider(LlmProviderKind::Ollama, "qwen")],
    );
    let service = WorkflowService::default();
    service.register_runner(Arc::new(HealthRunner)).unwrap();
    let tasks = TaskService::default();
    let first = prepare(
        &service,
        &context,
        &settings,
        &secrets,
        &agents,
        trusted(),
        Some(WorkflowScope::HealthCheck {
            mode: HealthCheckMode::Complete,
        }),
        None,
    )
    .unwrap();
    service
        .start(
            &context,
            trusted(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &first.preparation_id,
            &first.preparation_revision,
        )
        .unwrap();
    save_routes(&context, &settings, None, Vec::new());
    let overview = service
        .project_overview(&context, trusted(), &settings, &secrets, &agents, &tasks)
        .unwrap();
    assert_eq!(
        overview.rows[1]
            .prerequisite
            .as_ref()
            .map(|item| &item.action),
        Some(&WorkflowPrerequisiteAction::ConfigureExecutionRoute)
    );
}
