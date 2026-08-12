use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::agent::AgentKind;
use llm_wiki_desktop_lib::models::llm::{LlmProviderConfig, LlmProviderKind};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::project::ProjectTrustKind;
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

struct InstalledCodex;
struct InstalledClaude;

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
        unreachable!("a disabled Health Agent route must fail before capture")
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        unreachable!("a disabled Health Agent route must fail before streaming")
    }
}

impl ProcessRunner for InstalledCodex {
    fn find_executable(&self, command: &str) -> Option<PathBuf> {
        (command == "codex").then(|| PathBuf::from("codex"))
    }

    fn run_with_timeout(
        &self,
        _command: &str,
        args: &[&str],
        _timeout: Duration,
    ) -> Result<String, BackendError> {
        if args == ["--version"] {
            return Ok("codex 1.0.0".into());
        }
        Ok("--json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check -C --cd".into())
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        unreachable!("route preparation must not invoke the Agent")
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        unreachable!("route preparation must not invoke the Agent")
    }
}

impl ProcessRunner for InstalledClaude {
    fn find_executable(&self, command: &str) -> Option<PathBuf> {
        (command == "claude").then(|| PathBuf::from("claude"))
    }

    fn run_with_timeout(
        &self,
        _command: &str,
        args: &[&str],
        _timeout: Duration,
    ) -> Result<String, BackendError> {
        if args == ["--version"] {
            return Ok("claude 1.0.0".into());
        }
        Ok("--print --output-format --verbose --permission-mode --settings --bare --safe-mode --disable-slash-commands --no-session-persistence --no-chrome --prompt-suggestions --strict-mcp-config --tools --allowedTools --json-schema".into())
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        unreachable!("route preparation must not invoke the Agent")
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        unreachable!("route preparation must not invoke the Agent")
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
        trust_kind: Some(ProjectTrustKind::Native),
        filesystem_access: WorkflowFilesystemAccess::Writable,
        persistence: WorkflowPersistenceMode::Persistent,
        git_state: WorkflowGitState::Clean,
        authority_revision: "test-authority".into(),
    }
}

fn untrusted_read_only() -> WorkflowAccessSnapshot {
    WorkflowAccessSnapshot {
        trust: WorkflowProjectTrust::Untrusted,
        trust_kind: None,
        filesystem_access: WorkflowFilesystemAccess::ReadOnly,
        persistence: WorkflowPersistenceMode::MemoryOnly,
        git_state: WorkflowGitState::Clean,
        authority_revision: "test-untrusted-authority".into(),
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
fn health_default_stays_local_until_an_external_route_is_explicitly_selected() {
    let (_root, context, _config, settings, secrets, agents) = fixture();
    save_routes(
        &context,
        &settings,
        None,
        vec![provider(LlmProviderKind::Ollama, "qwen")],
    );
    let service = WorkflowService::default();
    let local = prepare(
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
        local.scope,
        WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick
        }
    ));
    assert!(matches!(local.route, Some(WorkflowRoute::Local { .. })));

    let explicit = prepare(
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
            provider: LlmProviderKind::Ollama,
        }),
    )
    .unwrap();
    assert!(matches!(explicit.route, Some(WorkflowRoute::Byok { .. })));

    let untrusted = WorkflowAccessSnapshot {
        trust: WorkflowProjectTrust::Untrusted,
        trust_kind: None,
        filesystem_access: WorkflowFilesystemAccess::ReadOnly,
        persistence: WorkflowPersistenceMode::MemoryOnly,
        git_state: WorkflowGitState::Clean,
        authority_revision: "test-authority".into(),
    };
    let untrusted_local = prepare(
        &service, &context, &settings, &secrets, &agents, untrusted, None, None,
    )
    .unwrap();
    assert!(matches!(
        untrusted_local.scope,
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
        None,
    )
    .unwrap();
    assert!(preparation.route.is_none());
    assert!(preparation
        .prerequisites
        .iter()
        .any(|item| { item.action == WorkflowPrerequisiteAction::ConfigureExecutionRoute }));
    assert!(!preparation
        .available_routes
        .iter()
        .any(|route| matches!(route, WorkflowRouteSelection::Agent { .. })));
    assert!(AgentService::supports_lint_agent(AgentKind::Claude));
    assert!(AgentService::supports_lint_agent(AgentKind::Codex));

    let tasks = TaskService::default();
    let error = service
        .start(
            &context,
            trusted(),
            &settings,
            &secrets,
            &agents,
            &tasks,
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_PREREQUISITES_BLOCKING");
    assert!(tasks.list_workflow_runs().is_empty());
}

#[test]
fn health_complete_advertises_only_an_installed_supported_agent() {
    let (_root, context, _config, settings, secrets, _agents) = fixture();
    save_routes(&context, &settings, Some(AgentKind::Codex), Vec::new());
    let agents = AgentService::with_runner(Arc::new(InstalledCodex));
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
        Some(WorkflowRouteSelection::Agent {
            agent: AgentKind::Codex,
        }),
    )
    .unwrap();

    assert!(matches!(
        preparation.route,
        Some(WorkflowRoute::Agent {
            agent: AgentKind::Codex,
            ..
        })
    ));
    assert_eq!(
        preparation.available_routes,
        vec![WorkflowRouteSelection::Agent {
            agent: AgentKind::Codex,
        }]
    );
    assert!(!preparation
        .available_routes
        .contains(&WorkflowRouteSelection::Agent {
            agent: AgentKind::Openclaw,
        }));
    assert!(!preparation
        .available_routes
        .contains(&WorkflowRouteSelection::Agent {
            agent: AgentKind::Hermes,
        }));
}

#[test]
fn health_complete_advertises_an_installed_claude_profile() {
    let (_root, context, _config, settings, secrets, _agents) = fixture();
    save_routes(&context, &settings, Some(AgentKind::Claude), Vec::new());
    let agents = AgentService::with_runner(Arc::new(InstalledClaude));
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
        Some(WorkflowRouteSelection::Agent {
            agent: AgentKind::Claude,
        }),
    )
    .unwrap();

    assert!(matches!(
        preparation.route,
        Some(WorkflowRoute::Agent {
            agent: AgentKind::Claude,
            ..
        })
    ));
    assert_eq!(
        preparation.available_routes,
        vec![WorkflowRouteSelection::Agent {
            agent: AgentKind::Claude,
        }]
    );
}

#[test]
fn untrusted_project_rejects_installed_agent_health_before_any_invocation() {
    let (_root, context, _config, settings, secrets, _agents) = fixture();
    save_routes(&context, &settings, Some(AgentKind::Codex), Vec::new());
    let agents = AgentService::with_runner(Arc::new(InstalledCodex));
    let service = WorkflowService::default();
    let access = untrusted_read_only();
    let preparation = prepare(
        &service,
        &context,
        &settings,
        &secrets,
        &agents,
        access.clone(),
        Some(WorkflowScope::HealthCheck {
            mode: HealthCheckMode::Complete,
        }),
        Some(WorkflowRouteSelection::Agent {
            agent: AgentKind::Codex,
        }),
    )
    .unwrap();
    assert!(preparation
        .prerequisites
        .iter()
        .any(|item| item.action == WorkflowPrerequisiteAction::TrustProject));

    let tasks = TaskService::default();
    let error = service
        .start(
            &context,
            access,
            &settings,
            &secrets,
            &agents,
            &tasks,
            &preparation.preparation_id,
            &preparation.preparation_revision,
        )
        .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_PREREQUISITES_BLOCKING");
    assert!(tasks.list_workflow_runs().is_empty());
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
        Some(WorkflowRouteSelection::Byok {
            provider: LlmProviderKind::Ollama,
        }),
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
