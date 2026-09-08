use super::*;
use crate::services::import_v2::source_registry::{SourceIndex, SourceManifest, SourcePointer};
use std::collections::BTreeMap;

fn metadata_fixture() -> (tempfile::TempDir, ProjectContext, WorkflowSourceVersionRef) {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("来源目录", root.path().to_path_buf());
    let mut manifest: SourceManifest = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/import-v2/source-manifest-v3.json"
    ))
    .unwrap();
    manifest.title = "中文来源 / Café".into();
    let version = manifest
        .versions
        .iter()
        .find(|version| version.version_id == manifest.current_version_id)
        .unwrap();
    let selected = WorkflowSourceVersionRef {
        source_id: manifest.source_id.clone(),
        version_id: version.version_id.clone(),
    };
    let pointer = SourcePointer {
        source_id: selected.source_id.clone(),
        version_id: selected.version_id.clone(),
    };
    let index = SourceIndex {
        schema_version: manifest.schema_version,
        by_content_hash: BTreeMap::from([(version.content_hash.clone(), pointer.clone())]),
        by_locator: BTreeMap::from([("file:/fixture.md".into(), pointer)]),
    };
    FileStore
        .write_json_atomic(&context, ".app/source-index-v2.json", &index)
        .unwrap();
    let path = context
        .layout
        .source_paths()
        .unwrap()
        .manifest(&selected.source_id)
        .unwrap();
    FileStore
        .write_json_atomic(&context, &path, &manifest)
        .unwrap();
    assert!(!context.root.join(&manifest.wiki_path).exists());
    assert!(!context.root.join(&version.baseline_path).exists());
    (root, context, selected)
}

#[test]
fn directory_lists_metadata_without_source_or_baseline_bodies() {
    let (_root, context, selected) = metadata_fixture();
    let page = WorkflowService::default()
        .list_update_wiki_sources(&context, "中文", 0)
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.unavailable, 0);
    assert_eq!(page.sources[0].source_id, selected.source_id);
    assert_eq!(page.sources[0].version_id, selected.version_id);
    assert_eq!(page.sources[0].title, "中文来源 / Café");
    assert_eq!(page.next_offset, None);
}

#[test]
fn broken_unselected_manifest_does_not_block_manual_source_resolution() {
    let (_root, context, selected) = metadata_fixture();
    let mut index = SourceRegistry::read_index(&context, &FileStore).unwrap();
    index.by_locator.insert(
        "file:/broken.md".into(),
        SourcePointer {
            source_id: "source-broken".into(),
            version_id: "version-broken".into(),
        },
    );
    index.by_content_hash.insert(
        "e".repeat(64),
        SourcePointer {
            source_id: "source-broken".into(),
            version_id: "version-broken".into(),
        },
    );
    FileStore
        .write_json_atomic(&context, ".app/source-index-v2.json", &index)
        .unwrap();
    FileStore
        .write_markdown(&context, ".app/sources/source-broken.json", "{broken")
        .unwrap();
    let page = WorkflowService::default()
        .list_update_wiki_sources(&context, "", 0)
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.unavailable, 1);
    let (rows, unavailable) =
        source_catalog(&context, Some(std::slice::from_ref(&selected))).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(unavailable, 0);
    let references =
        crate::services::CompileService::selected_source_versions(&context, &[selected]).unwrap();
    assert_eq!(references.len(), 1);
}

fn intent() -> UpdateWikiRequest {
    UpdateWikiRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        retry_of_task_id: None,
        mode: UpdateWikiMode::ChangedSources,
        selection: UpdateWikiSelection::Automatic,
        route_selection: Some(WorkflowRouteSelection::Agent {
            agent: AgentKind::Codex,
        }),
        acknowledge_remote_provider: false,
    }
}

#[test]
fn request_validation_distinguishes_automatic_empty_and_ambiguous_selection() {
    let mut request = intent();
    validate_request(&request).unwrap();
    request.request_id = "not-a-request-uuid".into();
    assert_eq!(
        validate_request(&request).unwrap_err().code,
        "WORKFLOW_REQUEST_INVALID"
    );
    request.request_id = uuid::Uuid::new_v4().to_string();
    request.selection = UpdateWikiSelection::Selected {
        source_versions: vec![],
    };
    assert_eq!(
        validate_request(&request).unwrap_err().code,
        "WORKFLOW_SOURCE_SELECTION_EMPTY"
    );
    let first = WorkflowSourceVersionRef {
        source_id: "source-one".into(),
        version_id: "version-one".into(),
    };
    request.selection = UpdateWikiSelection::Selected {
        source_versions: vec![first.clone()],
    };
    validate_request(&request).unwrap();
    request.selection = UpdateWikiSelection::Selected {
        source_versions: vec![
            first,
            WorkflowSourceVersionRef {
                source_id: "source-one".into(),
                version_id: "version-two".into(),
            },
        ],
    };
    assert_eq!(
        validate_request(&request).unwrap_err().code,
        "WORKFLOW_SOURCE_SELECTION_INVALID"
    );
}

fn enqueue_request(root: &std::path::Path, request: UpdateWikiRequest) -> EnqueueWorkflow {
    EnqueueWorkflow {
        project_id: "update-intent-test".into(),
        project_root: root.to_path_buf(),
        task_state_root: Some(root.join(".app/tasks")),
        title: "Update Wiki".into(),
        kind: WorkflowKind::UpdateWiki,
        scope: WorkflowScope::UpdateWiki {
            mode: request.mode.clone(),
            source_versions: vec![],
        },
        route: Some(WorkflowRoute::Agent {
            agent: AgentKind::Codex,
            model: None,
            route_revision: "configured-agent-v1".into(),
        }),
        baseline_fingerprint: format!("update-intent:{}", request.request_id),
        execution_options: WorkflowExecutionOptions {
            preparation_revision: request.request_id.clone(),
            update_config_revision: Some("configured-agent-v1".into()),
            update_request: Some(request),
            ..Default::default()
        },
        stages: workflow_stages(&WorkflowKind::UpdateWiki),
        retry: None,
    }
}

#[test]
fn same_request_id_reuses_active_task_and_rejects_changed_choices() {
    let root = tempfile::tempdir().unwrap();
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let request = intent();
    let WorkflowStartOutcome::Created { run } = coordinator
        .enqueue(&tasks, enqueue_request(root.path(), request.clone()))
        .unwrap()
    else {
        panic!("first request must create a task");
    };
    let WorkflowStartOutcome::Existing { run: repeated } = coordinator
        .enqueue(&tasks, enqueue_request(root.path(), request.clone()))
        .unwrap()
    else {
        panic!("the same request must reuse its task");
    };
    assert_eq!(repeated.task_id, run.task_id);
    let mut changed = request;
    changed.mode = UpdateWikiMode::FullRecompile;
    assert!(coordinator
        .enqueue(&tasks, enqueue_request(root.path(), changed))
        .unwrap_err()
        .contains("different choices"));
}

#[test]
fn request_id_still_reuses_terminal_task_after_recovery() {
    let root = tempfile::tempdir().unwrap();
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let request = intent();
    let WorkflowStartOutcome::Created { run } = coordinator
        .enqueue(&tasks, enqueue_request(root.path(), request.clone()))
        .unwrap()
    else {
        panic!("first request must create a task");
    };
    coordinator
        .reject_claimed_dispatch(
            &tasks,
            &run.task_id,
            WorkflowDispatchFailure::stale_not_modified(
                "WORKFLOW_ROUTE_UNAVAILABLE",
                "workflows.error.configureExecutionRoute",
            ),
        )
        .unwrap();
    let recovered = TaskService::default();
    recovered.recover_tasks(root.path()).unwrap();
    let WorkflowStartOutcome::Existing { run: repeated } = coordinator
        .enqueue(&recovered, enqueue_request(root.path(), request))
        .unwrap()
    else {
        panic!("terminal requests must not execute again after restart");
    };
    assert_eq!(repeated.task_id, run.task_id);
    assert_eq!(repeated.display_status, WorkflowDisplayStatus::Failed);
}

struct ProbeOnlyAgent {
    executable: Option<std::path::PathBuf>,
}

impl crate::services::ProcessRunner for ProbeOnlyAgent {
    fn find_executable(&self, _command: &str) -> Option<std::path::PathBuf> {
        Some(
            self.executable
                .clone()
                .expect("this operation must not probe an Agent"),
        )
    }

    fn run_with_timeout(
        &self,
        _command: &str,
        _args: &[&str],
        _timeout: std::time::Duration,
    ) -> Result<String, BackendError> {
        assert!(
            self.executable.is_some(),
            "this operation must not probe an Agent"
        );
        Ok("codex-cli 0.1.0 --json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check --cd".into())
    }

    fn run_capture(
        &self,
        _invocation: &crate::services::AgentInvocation,
    ) -> Result<(String, String), BackendError> {
        panic!("input binding must not run an Agent task")
    }

    fn run_task_streaming(
        &self,
        _invocation: &crate::services::AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        panic!("input binding must not run an Agent task")
    }
}

struct BindingServices {
    _config: tempfile::TempDir,
    settings: SettingsService,
    secrets: crate::services::SecretService,
    agents: crate::services::AgentService,
}

impl BindingServices {
    fn new(allow_probe: bool) -> Self {
        let config = tempfile::tempdir().unwrap();
        let executable = allow_probe.then(|| {
            let path = config.path().join("probe-fixture");
            std::fs::write(&path, "mock executable identity; never executed").unwrap();
            path
        });
        Self {
            settings: SettingsService::with_config_dir(config.path().to_path_buf()),
            secrets: crate::services::SecretService::memory(),
            agents: crate::services::AgentService::with_runner(std::sync::Arc::new(
                ProbeOnlyAgent { executable },
            )),
            _config: config,
        }
    }

    fn environment<'a>(
        &'a self,
        context: &'a ProjectContext,
    ) -> WorkflowPreparationEnvironment<'a> {
        WorkflowPreparationEnvironment {
            context,
            access: WorkflowAccessSnapshot {
                trust: WorkflowProjectTrust::Trusted,
                trust_kind: None,
                filesystem_access: WorkflowFilesystemAccess::Writable,
                persistence: WorkflowPersistenceMode::Persistent,
                git_state: WorkflowGitState::Unknown,
                authority_revision: "test-authority".into(),
            },
            settings_service: &self.settings,
            secret_service: &self.secrets,
            agent_service: &self.agents,
        }
    }
}

fn created_run(outcome: WorkflowStartOutcome) -> WorkflowRun {
    match outcome {
        WorkflowStartOutcome::Created { run } => run,
        WorkflowStartOutcome::Existing { .. } => panic!("expected a new task"),
    }
}

#[test]
fn automatic_selection_binds_sources_added_after_enqueue() {
    let (root, context, selected) = metadata_fixture();
    let original_index = SourceRegistry::read_index(&context, &FileStore).unwrap();
    FileStore
        .write_json_atomic(
            &context,
            ".app/source-index-v2.json",
            &SourceIndex::default(),
        )
        .unwrap();
    let path = context
        .layout
        .source_paths()
        .unwrap()
        .manifest(&selected.source_id)
        .unwrap();
    let mut manifest = SourceRegistry::read_manifest(&context, &FileStore, &path).unwrap();
    manifest.compiled_consumptions.clear();
    FileStore
        .write_json_atomic(&context, &path, &manifest)
        .unwrap();
    let workflows = WorkflowService::default();
    let tasks = TaskService::default();
    let request = intent();
    let run = created_run(
        workflows
            .coordinator
            .enqueue(&tasks, enqueue_request(root.path(), request.clone()))
            .unwrap(),
    );
    FileStore
        .write_json_atomic(&context, ".app/source-index-v2.json", &original_index)
        .unwrap();
    let services = BindingServices::new(true);
    let bound = workflows
        .bind_update_wiki_inputs(&services.environment(&context), &tasks, &run)
        .unwrap();
    assert_eq!(
        bound.scope,
        WorkflowScope::UpdateWiki {
            mode: UpdateWikiMode::ChangedSources,
            source_versions: vec![selected]
        }
    );
    assert_ne!(bound.fingerprint, run.fingerprint);
    assert!(!bound.baseline_fingerprint.starts_with("update-intent:"));
    assert_eq!(bound.current_stage_id.as_deref(), Some("analyze_sources"));
    assert_eq!(
        tasks
            .workflow_execution_options(&run.task_id)
            .unwrap()
            .update_request,
        Some(request)
    );
    let restarted = TaskService::default();
    restarted.recover_tasks(root.path()).unwrap();
    let recovered = restarted.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(recovered.scope, bound.scope);
    assert_eq!(recovered.fingerprint, bound.fingerprint);
    assert_eq!(recovered.display_status, WorkflowDisplayStatus::Interrupted);
}

fn write_provider_fixture(
    context: &ProjectContext,
    services: &BindingServices,
    config: crate::models::llm::LlmProviderConfig,
) {
    // A configured provider without any approved credential binding or secret.
    let mut settings = services.settings.read_settings(context).unwrap();
    settings.llm_providers = vec![config];
    FileStore
        .write_json_atomic(
            context,
            context.layout.settings_path.as_deref().unwrap(),
            &settings.to_project_file(),
        )
        .unwrap();
}

fn configured_byok_request(
    context: &ProjectContext,
    services: &BindingServices,
) -> EnqueueWorkflow {
    use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
    write_provider_fixture(
        context,
        services,
        LlmProviderConfig {
            provider: LlmProviderKind::OpenAi,
            model: "gpt-test".into(),
            base_url: "https://api.openai.com".into(),
            context_window: 8192,
            enabled: true,
        },
    );
    let mut request = intent();
    request.route_selection = Some(WorkflowRouteSelection::Byok {
        provider: LlmProviderKind::OpenAi,
    });
    request.acknowledge_remote_provider = true;
    let route = configured_route(
        context,
        &services.settings,
        request.route_selection.as_ref(),
    )
    .unwrap();
    let WorkflowRoute::Byok { route_revision, .. } = &route else {
        unreachable!()
    };
    let mut enqueue = enqueue_request(&context.root, request);
    enqueue.execution_options.update_config_revision = Some(route_revision.clone());
    enqueue
        .execution_options
        .remote_provider_acknowledgement_revision =
        Some(preparation::REMOTE_PROVIDER_DISCLOSURE_REVISION.into());
    enqueue.route = Some(route);
    enqueue
}

#[tokio::test]
async fn no_changes_finish_after_binding_without_agent_or_provider_credentials() {
    let (_root, context, _) = metadata_fixture();
    let services = BindingServices::new(false);
    let workflows = WorkflowService::default();
    let tasks = TaskService::default();
    // The fixture's current source is already consumed. No provider key or
    // credential binding exists, so a strict route probe would fail here.
    let request = configured_byok_request(&context, &services);
    let run = created_run(workflows.coordinator.enqueue(&tasks, request).unwrap());
    let bound = workflows
        .bind_update_wiki_inputs(&services.environment(&context), &tasks, &run)
        .unwrap();
    assert!(
        matches!(&bound.scope, WorkflowScope::UpdateWiki { source_versions, .. } if source_versions.is_empty())
    );
    assert_eq!(bound.current_stage_id.as_deref(), Some("analyze_sources"));
    let bookmarks = crate::services::BookmarkService::default();
    let search = crate::services::SearchService::default();
    let confirmations = crate::models::confirmation::ConfirmationRegistry::default();
    let execution = crate::services::UpdateWikiExecutionServices {
        compile: crate::services::CompileExecutionServices {
            agent_service: &services.agents,
            llm_service: &crate::services::LlmService,
            secret_service: &services.secrets,
            settings_service: &services.settings,
            task_service: &tasks,
        },
        git_service: &crate::services::GitService,
        file_store: &FileStore,
        bookmark_service: &bookmarks,
        search_service: &search,
        confirmation_registry: &confirmations,
        coordinator: &workflows.coordinator,
    };
    assert!(
        crate::services::run_update_wiki_authorized(&context, bound, &execution, || {
            panic!("an empty update must not request external launch authority")
        })
        .await
        .is_none()
    );
    let finished = tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(finished.display_status, WorkflowDisplayStatus::Completed);
    assert!(matches!(
        finished.result,
        Some(WorkflowResult::UpdateWiki {
            created: 0,
            updated: 0,
            final_commit: None,
            ..
        })
    ));
    assert!(!context.root.join(".git").exists());
}

#[test]
fn cancelled_intent_is_not_bound_or_probed() {
    let (root, context, _) = metadata_fixture();
    let services = BindingServices::new(false);
    let workflows = WorkflowService::default();
    let tasks = TaskService::default();
    let run = created_run(
        workflows
            .coordinator
            .enqueue(&tasks, enqueue_request(root.path(), intent()))
            .unwrap(),
    );
    workflows.coordinator.cancel(&tasks, &run.task_id).unwrap();
    assert!(workflows
        .bind_update_wiki_inputs(&services.environment(&context), &tasks, &run)
        .is_err());
    let current = tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(current.fingerprint, run.fingerprint);
    assert_eq!(current.baseline_fingerprint, run.baseline_fingerprint);
    assert_eq!(
        tasks.get_task(&run.task_id).unwrap().status,
        crate::models::task::TaskStatus::Cancelling
    );
}

#[test]
fn changed_provider_configuration_rejects_binding_before_credentials_or_processes() {
    let (_root, context, _) = metadata_fixture();
    let services = BindingServices::new(false);
    let workflows = WorkflowService::default();
    let tasks = TaskService::default();
    let request = configured_byok_request(&context, &services);
    let run = created_run(workflows.coordinator.enqueue(&tasks, request).unwrap());
    let mut config = services
        .settings
        .read_settings(&context)
        .unwrap()
        .llm_providers[0]
        .clone();
    config.model = "another-model".into();
    write_provider_fixture(&context, &services, config);
    let error = workflows
        .bind_update_wiki_inputs(&services.environment(&context), &tasks, &run)
        .unwrap_err();
    assert_eq!(error.code, "WORKFLOW_ROUTE_CHANGED");
    let unchanged = tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(unchanged.fingerprint, run.fingerprint);
    assert_eq!(unchanged.route, run.route);
}

#[test]
fn retry_preserves_update_choices_with_a_fresh_request_and_unbound_baseline() {
    let root = tempfile::tempdir().unwrap();
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let original_request = intent();
    let run = created_run(
        coordinator
            .enqueue(
                &tasks,
                enqueue_request(root.path(), original_request.clone()),
            )
            .unwrap(),
    );
    coordinator
        .reject_claimed_dispatch(
            &tasks,
            &run.task_id,
            WorkflowDispatchFailure::stale_not_modified(
                "WORKFLOW_ROUTE_UNAVAILABLE",
                "workflows.error.configureExecutionRoute",
            ),
        )
        .unwrap();
    let retried = created_run(
        coordinator
            .retry(
                &tasks,
                &run.task_id,
                run.project_id.clone(),
                root.path().to_path_buf(),
                Some(root.path().join(".app/tasks")),
            )
            .unwrap(),
    );
    let options = tasks.workflow_execution_options(&retried.task_id).unwrap();
    let retry_request = options.update_request.unwrap();
    assert_ne!(retry_request.request_id, original_request.request_id);
    assert_eq!(retry_request.mode, original_request.mode);
    assert_eq!(retry_request.selection, original_request.selection);
    assert_eq!(
        retry_request.route_selection,
        original_request.route_selection
    );
    assert_eq!(options.preparation_revision, retry_request.request_id);
    assert_eq!(
        retried.baseline_fingerprint,
        format!("update-intent:{}", retry_request.request_id)
    );
    assert_eq!(retried.retry.unwrap().attempt_of, run.task_id);
}
