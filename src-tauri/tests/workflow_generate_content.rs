use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::confirmation::{ConfirmationExecution, ConfirmationRegistry};
use llm_wiki_desktop_lib::models::llm::{LlmProviderConfig, LlmProviderKind};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::settings::Settings;
use llm_wiki_desktop_lib::models::workflow::{
    WorkflowArtifactType, WorkflowDisplayStatus, WorkflowExecutionOptions, WorkflowKind,
    WorkflowRoute, WorkflowScope, WorkflowStageStatus, WorkflowStartOutcome,
};
use llm_wiki_desktop_lib::services::{
    confirm_generate_content_overwrite, discard_generate_content_candidate,
    restore_generate_content_confirmation, run_generate_content_with_generator,
    workflow_baseline_for_scope, workflow_stages, AgentInvocation, AgentService, EnqueueWorkflow,
    ExportService, GenerateContentExecutionServices, GitService, LlmService, ProcessRunner,
    SearchService, SecretService, SettingsService, WorkflowCoordinator,
};
use llm_wiki_desktop_lib::tasks::TaskService;
use sha2::{Digest, Sha256};

const HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\"></head><body><main>Generated</main></body></html>";

struct NoAgents;

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
        unreachable!("injected generation must not invoke a process")
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        unreachable!("injected generation must not invoke a process")
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    _config: tempfile::TempDir,
    context: ProjectContext,
    export: ExportService,
    search: SearchService,
    settings: SettingsService,
    secrets: SecretService,
    agents: AgentService,
    llm: LlmService,
    git: GitService,
    confirmations: ConfirmationRegistry,
    tasks: TaskService,
    coordinator: WorkflowCoordinator,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".app/tasks")).unwrap();
        fs::create_dir_all(root.path().join("wiki/assets")).unwrap();
        fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
        fs::write(root.path().join("wiki/index.md"), "# Index\n").unwrap();
        fs::write(root.path().join("wiki/overview.md"), "# Overview\n").unwrap();
        fs::write(root.path().join("wiki/log.md"), "# Log\n").unwrap();
        fs::write(
            root.path().join("wiki/concepts/主题.md"),
            "# 主题\n\n![图](../assets/图.png)\n",
        )
        .unwrap();
        fs::write(root.path().join("wiki/assets/图.png"), "image-v1").unwrap();
        fs::write(root.path().join("purpose.md"), "# Purpose\n").unwrap();
        fs::write(root.path().join("schema.md"), "# Schema\n").unwrap();
        fs::write(root.path().join(".gitignore"), ".app/tasks/\n").unwrap();
        let config = tempfile::tempdir().unwrap();
        let settings = SettingsService::with_config_dir(config.path().to_path_buf());
        let context = ProjectContext::new(format!("generate-{label}"), root.path().to_path_buf());
        settings
            .save_settings(
                &context,
                &Settings {
                    llm_providers: vec![LlmProviderConfig {
                        provider: LlmProviderKind::Ollama,
                        model: "qwen-generate".into(),
                        base_url: "http://127.0.0.1:11434".into(),
                        context_window: 8192,
                        enabled: true,
                    }],
                    ..Settings::default()
                },
            )
            .unwrap();
        Self {
            context,
            export: ExportService::default(),
            search: SearchService::default(),
            settings,
            secrets: SecretService::memory(),
            agents: AgentService::with_runner(Arc::new(NoAgents)),
            llm: LlmService,
            git: GitService,
            confirmations: ConfirmationRegistry::default(),
            tasks: TaskService::default(),
            coordinator: WorkflowCoordinator::default(),
            _root: root,
            _config: config,
        }
    }

    fn services(&self) -> GenerateContentExecutionServices<'_> {
        GenerateContentExecutionServices {
            export_service: &self.export,
            search_service: &self.search,
            settings_service: &self.settings,
            secret_service: &self.secrets,
            agent_service: &self.agents,
            llm_service: &self.llm,
            git_service: &self.git,
            confirmation_registry: &self.confirmations,
            task_service: &self.tasks,
            coordinator: &self.coordinator,
        }
    }

    fn route(&self) -> WorkflowRoute {
        let config = self.settings.read_settings(&self.context).unwrap();
        let provider = &config.llm_providers[0];
        let configured_secret = true;
        let revision = llm_wiki_desktop_lib::services::canonical_json(&(
            provider.provider,
            &provider.model,
            &provider.base_url,
            provider.context_window,
            provider.enabled,
            configured_secret,
        ))
        .map(|value| format!("{:x}", Sha256::digest(value.as_bytes())))
        .unwrap();
        WorkflowRoute::Byok {
            provider: provider.provider,
            model: provider.model.clone(),
            route_revision: revision,
        }
    }

    fn enqueue(
        &self,
        artifact_type: WorkflowArtifactType,
        page_paths: Vec<String>,
        output_path: &str,
        existing_target_hash: Option<String>,
        restricted_ack: Option<String>,
    ) -> llm_wiki_desktop_lib::models::workflow::WorkflowRun {
        self.enqueue_with_remote_ack(
            artifact_type,
            page_paths,
            output_path,
            existing_target_hash,
            restricted_ack,
            true,
        )
    }

    fn enqueue_with_remote_ack(
        &self,
        artifact_type: WorkflowArtifactType,
        page_paths: Vec<String>,
        output_path: &str,
        existing_target_hash: Option<String>,
        restricted_ack: Option<String>,
        acknowledge_remote: bool,
    ) -> llm_wiki_desktop_lib::models::workflow::WorkflowRun {
        let scope = WorkflowScope::GenerateContent {
            artifact_type,
            page_paths,
            output_path: Some(output_path.into()),
        };
        let baseline = workflow_baseline_for_scope(&self.context, &scope).unwrap();
        let route = self.route();
        let remote_ack = llm_wiki_desktop_lib::services::canonical_json(&Some(route.clone()))
            .map(|value| format!("{:x}", Sha256::digest(value.as_bytes())))
            .unwrap();
        match self
            .coordinator
            .enqueue(
                &self.tasks,
                EnqueueWorkflow {
                    project_id: self.context.project_id.clone(),
                    project_root: self.context.root.clone(),
                    task_state_root: Some(self.context.app_dir.join("tasks")),
                    title: "Generate Content".into(),
                    kind: WorkflowKind::GenerateContent,
                    scope,
                    route: Some(route),
                    baseline_fingerprint: baseline.fingerprint,
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision: "generate-test-v1".into(),
                        existing_target_hash,
                        restricted_content_acknowledgement_revision: restricted_ack,
                        remote_provider_acknowledgement_revision: acknowledge_remote
                            .then_some(remote_ack),
                    },
                    stages: workflow_stages(&WorkflowKind::GenerateContent),
                    retry: None,
                },
            )
            .unwrap()
        {
            WorkflowStartOutcome::Created { run } => run,
            WorkflowStartOutcome::Existing { .. } => panic!("new preparation must create a run"),
        }
    }
}

fn write(root: &Path, relative: &str, body: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn new_artifact_completes_exactly_nine_stages_without_git_and_is_exports_readable() {
    let fixture = Fixture::new("new");
    let output = "exports/html/主题 阅读.html";
    let run = fixture.enqueue(
        WorkflowArtifactType::BeautifulRead,
        vec!["wiki/concepts/主题.md".into()],
        output,
        None,
        None,
    );
    let task_id = run.task_id.clone();
    run_generate_content_with_generator(
        &fixture.context,
        run,
        &fixture.services(),
        |prompt, _| async move {
            assert!(prompt.contains("# 主题"));
            Ok(HTML.into())
        },
    )
    .await;

    let finished = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert_eq!(
        finished
            .stages
            .iter()
            .map(|stage| stage.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "confirm_scope",
            "read_wiki",
            "load_template",
            "generate_content",
            "assemble_artifact",
            "validate_artifact",
            "write_export",
            "generate_preview",
            "complete",
        ]
    );
    assert!(finished
        .stages
        .iter()
        .all(|stage| stage.status == WorkflowStageStatus::Completed));
    assert!(!fixture.context.root.join(".git").exists());
    let written = fs::read_to_string(fixture.context.root.join(output)).unwrap();
    assert!(written.contains("Generated"));
    assert!(written.contains("Content-Security-Policy"));
    let records = fixture.export.list_records(&fixture.context).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].output_path, output);
    assert!(records[0].preview.as_ref().unwrap().validation_passed);
    assert!(fixture
        .export
        .resolve_existing_html_export(&fixture.context, output)
        .is_ok());
    let retry = fixture
        .coordinator
        .retry(&fixture.tasks, &task_id, fixture.context.root.clone())
        .unwrap();
    let retry = match retry {
        WorkflowStartOutcome::Created { run } => run,
        WorkflowStartOutcome::Existing { .. } => panic!("linked retry must create a new attempt"),
    };
    assert_eq!(retry.retry.as_ref().unwrap().attempt_of, task_id);
    assert_eq!(retry.retry.as_ref().unwrap().attempt_number, 2);
}

#[tokio::test]
async fn page_or_resource_change_during_generation_fails_without_artifact() {
    let fixture = Fixture::new("baseline");
    let output = "exports/html/baseline.html";
    let run = fixture.enqueue(
        WorkflowArtifactType::KnowledgeCard,
        vec!["wiki/concepts/主题.md".into()],
        output,
        None,
        None,
    );
    let task_id = run.task_id.clone();
    let resource = fixture.context.root.join("wiki/assets/图.png");
    run_generate_content_with_generator(
        &fixture.context,
        run,
        &fixture.services(),
        move |_, _| async move {
            fs::write(resource, "image-v2").unwrap();
            Ok(HTML.into())
        },
    )
    .await;

    let failed = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert_eq!(
        failed.error.as_ref().map(|error| error.code.as_str()),
        Some("WORKFLOW_INPUT_BASELINE_CHANGED")
    );
    assert!(!fixture.context.root.join(output).exists());
}

#[tokio::test]
async fn existing_target_gets_checkpoint_then_waits_and_a_racing_edit_becomes_conflict() {
    let fixture = Fixture::new("overwrite");
    let output = "exports/html/existing.html";
    write(&fixture.context.root, output, "old export");
    git(&fixture.context.root, &["init"]);
    git(
        &fixture.context.root,
        &["config", "user.email", "test@example.com"],
    );
    git(
        &fixture.context.root,
        &["config", "user.name", "Workflow Test"],
    );
    git(&fixture.context.root, &["add", "."]);
    git(&fixture.context.root, &["commit", "-m", "baseline"]);
    let existing_hash = llm_wiki_desktop_lib::services::FileStore
        .file_hash(&fixture.context, output)
        .unwrap();
    let run = fixture.enqueue(
        WorkflowArtifactType::ConceptMap,
        vec!["wiki/concepts/主题.md".into()],
        output,
        Some(existing_hash),
        None,
    );
    let task_id = run.task_id.clone();
    let target = fixture.context.root.join(output);
    run_generate_content_with_generator(
        &fixture.context,
        run,
        &fixture.services(),
        move |_, _| async move {
            fs::write(target, "concurrent edit").unwrap();
            Ok(HTML.into())
        },
    )
    .await;

    let waiting = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert_eq!(
        waiting.display_status,
        WorkflowDisplayStatus::WaitingForConfirmation,
        "unexpected workflow error: {:?}",
        waiting.error
    );
    let pending = waiting.pending_action.as_ref().unwrap();
    assert_eq!(pending.affected_paths, vec![output]);
    assert!(pending.checkpoint_hash.is_some());
    assert_eq!(
        fs::read_to_string(fixture.context.root.join(output)).unwrap(),
        "concurrent edit"
    );

    let restarted = TaskService::default();
    restarted.recover_tasks(&fixture.context.root).unwrap();
    let recovered = restarted.get_workflow_run(&task_id).unwrap();
    assert_eq!(
        recovered.display_status,
        WorkflowDisplayStatus::WaitingForConfirmation
    );
    let restarted_registry = ConfirmationRegistry::default();
    restore_generate_content_confirmation(
        &fixture.context,
        &recovered,
        &restarted,
        &restarted_registry,
    )
    .unwrap();
    assert!(matches!(
        restarted_registry.peek(&pending.id).unwrap().execution,
        Some(ConfirmationExecution::GenerateContentOverwrite { .. })
    ));
    let descriptor = std::env::temp_dir()
        .join("llm-wiki-desktop-generate-content")
        .join(&task_id)
        .join("candidate.json");
    fs::OpenOptions::new()
        .append(true)
        .open(descriptor)
        .unwrap()
        .write_all(b" ")
        .unwrap();
    let tampered_recovery = TaskService::default();
    tampered_recovery
        .recover_tasks(&fixture.context.root)
        .unwrap();
    assert_eq!(
        tampered_recovery
            .get_workflow_run(&task_id)
            .unwrap()
            .display_status,
        WorkflowDisplayStatus::Interrupted
    );
    discard_generate_content_candidate(&task_id).unwrap();
}

#[tokio::test]
async fn unchanged_existing_target_applies_only_after_confirmation() {
    let fixture = Fixture::new("overwrite-confirm");
    let output = "exports/html/confirm.html";
    write(&fixture.context.root, output, "old export");
    git(&fixture.context.root, &["init"]);
    git(
        &fixture.context.root,
        &["config", "user.email", "test@example.com"],
    );
    git(
        &fixture.context.root,
        &["config", "user.name", "Workflow Test"],
    );
    git(&fixture.context.root, &["add", "."]);
    git(&fixture.context.root, &["commit", "-m", "baseline"]);
    let existing_hash = llm_wiki_desktop_lib::services::FileStore
        .file_hash(&fixture.context, output)
        .unwrap();
    let run = fixture.enqueue(
        WorkflowArtifactType::BeautifulRead,
        vec!["wiki/concepts/主题.md".into()],
        output,
        Some(existing_hash),
        None,
    );
    let task_id = run.task_id.clone();
    run_generate_content_with_generator(&fixture.context, run, &fixture.services(), |_, _| async {
        Ok(HTML.into())
    })
    .await;
    assert_eq!(
        fs::read_to_string(fixture.context.root.join(output)).unwrap(),
        "old export"
    );
    let completed =
        confirm_generate_content_overwrite(&fixture.context, &task_id, &fixture.services())
            .unwrap()
            .0;
    assert_eq!(completed.display_status, WorkflowDisplayStatus::Completed);
    assert!(fs::read_to_string(fixture.context.root.join(output))
        .unwrap()
        .contains("Generated"));
    assert_eq!(
        fixture.export.list_records(&fixture.context).unwrap().len(),
        1
    );
}

#[tokio::test]
async fn confirmed_overwrite_rechecks_the_review_hash_before_apply() {
    let fixture = Fixture::new("overwrite-recheck");
    let output = "exports/html/recheck.html";
    write(&fixture.context.root, output, "old export");
    git(&fixture.context.root, &["init"]);
    git(
        &fixture.context.root,
        &["config", "user.email", "test@example.com"],
    );
    git(
        &fixture.context.root,
        &["config", "user.name", "Workflow Test"],
    );
    git(&fixture.context.root, &["add", "."]);
    git(&fixture.context.root, &["commit", "-m", "baseline"]);
    let existing_hash = llm_wiki_desktop_lib::services::FileStore
        .file_hash(&fixture.context, output)
        .unwrap();
    let run = fixture.enqueue(
        WorkflowArtifactType::BeautifulRead,
        vec!["wiki/concepts/主题.md".into()],
        output,
        Some(existing_hash),
        None,
    );
    let task_id = run.task_id.clone();
    run_generate_content_with_generator(&fixture.context, run, &fixture.services(), |_, _| async {
        Ok(HTML.into())
    })
    .await;
    assert_eq!(
        fixture
            .tasks
            .get_workflow_run(&task_id)
            .unwrap()
            .display_status,
        WorkflowDisplayStatus::WaitingForConfirmation
    );

    fs::write(fixture.context.root.join(output), "edited after review").unwrap();
    let error = confirm_generate_content_overwrite(&fixture.context, &task_id, &fixture.services())
        .expect_err("a post-review edit must invalidate the candidate");
    assert_eq!(error.error.code, "WORKFLOW_CANDIDATE_STALE");
    assert_eq!(
        fs::read_to_string(fixture.context.root.join(output)).unwrap(),
        "edited after review"
    );
}

#[tokio::test]
async fn restricted_acknowledgement_is_required_independently_of_the_remote_route() {
    let fixture = Fixture::new("restricted");
    write(
        &fixture.context.root,
        ".app/sources/restricted.json",
        r#"{
            "schemaVersion":3,"sourceId":"restricted","sourceKind":"web_page",
            "currentVersionId":"v1","wikiPath":"wiki/concepts/主题.md","aliases":[],
            "origins":["https://example.com"],"title":"Restricted",
            "importedAt":"2026-07-30T00:00:00Z","versions":[],"compiledConsumptions":[],
            "restrictedContent":true,"restrictedIdentitySummary":"private","timeline":[]
        }"#,
    );
    let output = "exports/html/restricted.html";
    let run = fixture.enqueue(
        WorkflowArtifactType::BeautifulRead,
        vec!["wiki/concepts/主题.md".into()],
        output,
        None,
        None,
    );
    let task_id = run.task_id.clone();
    run_generate_content_with_generator(
        &fixture.context,
        run,
        &fixture.services(),
        |_, route| async move {
            assert!(matches!(route, WorkflowRoute::Byok { .. }));
            Ok(HTML.into())
        },
    )
    .await;
    assert_eq!(
        fixture
            .tasks
            .get_workflow_run(&task_id)
            .unwrap()
            .error
            .unwrap()
            .code,
        "WORKFLOW_RESTRICTED_CONTENT_ACKNOWLEDGEMENT_REQUIRED"
    );
    assert!(!fixture.context.root.join(output).exists());
}

#[tokio::test]
async fn remote_provider_disclosure_is_required_without_conflating_restricted_content() {
    let fixture = Fixture::new("remote-disclosure");
    let output = "exports/html/remote.html";
    let run = fixture.enqueue_with_remote_ack(
        WorkflowArtifactType::BeautifulRead,
        vec!["wiki/concepts/主题.md".into()],
        output,
        None,
        None,
        false,
    );
    let task_id = run.task_id.clone();
    run_generate_content_with_generator(&fixture.context, run, &fixture.services(), |_, _| async {
        panic!("remote generation must not begin before disclosure acknowledgement")
    })
    .await;
    assert_eq!(
        fixture
            .tasks
            .get_workflow_run(&task_id)
            .unwrap()
            .error
            .unwrap()
            .code,
        "WORKFLOW_REMOTE_PROVIDER_ACKNOWLEDGEMENT_REQUIRED"
    );
    assert!(!fixture.context.root.join(output).exists());
}

#[tokio::test]
async fn cancellation_after_generation_leaves_no_artifact_or_record() {
    let fixture = Fixture::new("cancel");
    let output = "exports/html/cancelled.html";
    let run = fixture.enqueue(
        WorkflowArtifactType::ProjectReport,
        Vec::new(),
        output,
        None,
        None,
    );
    let task_id = run.task_id.clone();
    let cancel_id = task_id.clone();
    let tasks = &fixture.tasks;
    run_generate_content_with_generator(
        &fixture.context,
        run,
        &fixture.services(),
        move |_, _| async move {
            tasks.cancel_task(&cancel_id).unwrap();
            Ok(HTML.into())
        },
    )
    .await;

    let cancelled = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert_eq!(cancelled.display_status, WorkflowDisplayStatus::Cancelled);
    assert!(!fixture.context.root.join(output).exists());
    assert!(fixture
        .export
        .list_records(&fixture.context)
        .unwrap()
        .is_empty());
}
