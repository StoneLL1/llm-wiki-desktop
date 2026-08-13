use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::agent::AgentKind;
use llm_wiki_desktop_lib::models::llm::{LlmProviderConfig, LlmProviderKind};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::project::ProjectTrustKind;
use llm_wiki_desktop_lib::models::settings::Settings;
use llm_wiki_desktop_lib::models::workflow::{
    HealthCheckMode, WorkflowExecutionOptions, WorkflowFilesystemAccess, WorkflowGitState,
    WorkflowKind, WorkflowPersistenceMode, WorkflowProjectTrust, WorkflowResult, WorkflowRoute,
    WorkflowScope, WorkflowStageStatus, WorkflowStartOutcome,
};
use llm_wiki_desktop_lib::services::{
    project_identity, run_health_check, run_health_check_with_deep, workflow_baseline_for_scope,
    workflow_stages, AgentInvocation, AgentProbeTarget, AgentService, EnqueueWorkflow, GitService,
    HealthCheckExecutionServices, LintService, LlmService, PrepareWorkflowInput, ProcessRunner,
    SearchService, SecretService, SettingsService, WorkflowAccessSnapshot,
    WorkflowPreparationEnvironment, WorkflowService,
};
use llm_wiki_desktop_lib::tasks::TaskService;

struct NoAgents;

#[derive(Default)]
struct SuccessfulCodex {
    invocations: AtomicUsize,
}

#[derive(Default)]
struct SuccessfulClaude {
    invocations: AtomicUsize,
}

#[derive(Default)]
struct DriftingCodex {
    version_probes: AtomicUsize,
    invocations: AtomicUsize,
}

struct RetargetedCodex {
    shim: PathBuf,
    scripts: [PathBuf; 2],
    active: AtomicUsize,
    invocations: AtomicUsize,
}

#[derive(Default)]
struct CancelDuringLaunchProbe {
    version_probes: AtomicUsize,
    invocations: AtomicUsize,
    cancel: Mutex<Option<(Arc<TaskService>, String)>>,
}

struct MutateDuringLaunchProbe {
    version_probes: AtomicUsize,
    invocations: AtomicUsize,
    target: PathBuf,
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
        unreachable!("Health Check must not call an unavailable Agent")
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        unreachable!("Health Check must not call an unavailable Agent")
    }
}

impl ProcessRunner for SuccessfulCodex {
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
        unreachable!("Health Agent execution must use streaming")
    }

    fn run_task_streaming(
        &self,
        invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        assert_eq!(invocation.program, "codex");
        assert!(invocation
            .args
            .windows(2)
            .any(|pair| pair == ["--sandbox", "read-only"]));
        Ok(r#"[{"issueType":"schema_mismatch","severity":"warning","path":"wiki/concepts/主题.md","message":"Agent finding","evidence":"agent evidence","suggestion":"Review schema"}]"#.into())
    }
}

impl ProcessRunner for SuccessfulClaude {
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
        unreachable!("Health Agent execution must use streaming")
    }

    fn run_task_streaming(
        &self,
        invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        self.invocations.fetch_add(1, Ordering::SeqCst);
        assert_eq!(invocation.program, "claude");
        assert!(invocation.args.contains(&"--safe-mode".to_string()));
        assert!(invocation.args.contains(&"--tools=".to_string()));
        assert!(!invocation.args.iter().any(|argument| {
            argument.contains("Read")
                || argument.contains("Write")
                || argument.contains("Edit")
                || argument.contains("Bash")
        }));
        Ok("[]".into())
    }
}

impl ProcessRunner for DriftingCodex {
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
            let probe = self.version_probes.fetch_add(1, Ordering::SeqCst);
            return Ok(if probe < 2 {
                "codex 1.0.0".into()
            } else {
                "codex 2.0.0".into()
            });
        }
        Ok("--json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check -C --cd".into())
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        unreachable!("Health Agent execution must use streaming")
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

impl ProcessRunner for RetargetedCodex {
    fn find_executable(&self, command: &str) -> Option<PathBuf> {
        (command == "codex").then(|| self.shim.clone())
    }

    fn resolve_probe_target(&self, command: &str) -> AgentProbeTarget {
        AgentProbeTarget {
            logical_command: command.into(),
            executable_path: self.find_executable(command),
            program: "node.exe".into(),
            leading_args: vec![self.scripts[self.active.load(Ordering::SeqCst)]
                .to_string_lossy()
                .into_owned()],
        }
    }

    fn run_probe_with_timeout(
        &self,
        _target: &AgentProbeTarget,
        args: &[&str],
        _timeout: Duration,
    ) -> Result<String, BackendError> {
        if args == ["--version"] {
            return Ok("codex 1.0.0".into());
        }
        Ok("--json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check -C --cd".into())
    }

    fn run_with_timeout(
        &self,
        _command: &str,
        _args: &[&str],
        _timeout: Duration,
    ) -> Result<String, BackendError> {
        unreachable!("retarget test must probe the exact target")
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        unreachable!()
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

impl ProcessRunner for CancelDuringLaunchProbe {
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
            let probe = self.version_probes.fetch_add(1, Ordering::SeqCst);
            if probe == 2 {
                if let Some((tasks, task_id)) = self.cancel.lock().unwrap().take() {
                    tasks.cancel_task(&task_id).unwrap();
                }
            }
            return Ok("codex 1.0.0".into());
        }
        Ok("--json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check -C --cd".into())
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        unreachable!("Health Agent execution must use streaming")
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

impl ProcessRunner for MutateDuringLaunchProbe {
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
            let probe = self.version_probes.fetch_add(1, Ordering::SeqCst);
            if probe == 2 {
                fs::write(&self.target, "# 主题\n\nChanged before launch.\n").unwrap();
            }
            return Ok("codex 1.0.0".into());
        }
        Ok("--json --ephemeral --sandbox --ignore-user-config --ignore-rules --output-schema --output-last-message --skip-git-repo-check -C --cd".into())
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        unreachable!("Health Agent execution must use streaming")
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

struct Fixture {
    _root: tempfile::TempDir,
    _config: tempfile::TempDir,
    context: ProjectContext,
    settings: SettingsService,
    secrets: SecretService,
    agents: AgentService,
    lint: LintService,
    search: SearchService,
    llm: LlmService,
    tasks: Arc<TaskService>,
    workflows: WorkflowService,
}

impl Fixture {
    fn native(label: &str) -> Self {
        Self::native_with_runner(label, Arc::new(NoAgents))
    }

    fn native_with_runner(label: &str, runner: Arc<dyn ProcessRunner>) -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".app/tasks")).unwrap();
        fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
        fs::create_dir_all(root.path().join("raw/extracted")).unwrap();
        fs::write(root.path().join("wiki/index.md"), "# Index\n").unwrap();
        fs::write(root.path().join("wiki/overview.md"), "# Overview\n").unwrap();
        fs::write(root.path().join("wiki/log.md"), "# Log\n").unwrap();
        fs::write(
            root.path().join("wiki/concepts/主题.md"),
            "# 主题\n\nStable content.\n",
        )
        .unwrap();
        fs::write(
            root.path().join("raw/extracted/来源.md"),
            "---\ntype: source\n---\n# 来源\n\nEvidence.\n",
        )
        .unwrap();
        let context = ProjectContext::new(format!("health-{label}"), root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        Self {
            context,
            settings: SettingsService::with_config_dir(config.path().to_path_buf()),
            secrets: SecretService::memory(),
            agents: AgentService::with_runner(runner),
            lint: LintService::default(),
            search: SearchService::default(),
            llm: LlmService,
            tasks: Arc::new(TaskService::default()),
            workflows: WorkflowService::default(),
            _root: root,
            _config: config,
        }
    }

    fn configure_agent(&self, access: WorkflowAccessSnapshot, agent: AgentKind) -> WorkflowRoute {
        let expected_filesystem_access = access.filesystem_access.clone();
        let expected_persistence = access.persistence.clone();
        self.settings
            .save_settings(
                &self.context,
                &Settings {
                    agent_default: Some(agent),
                    ..Settings::default()
                },
            )
            .unwrap();
        let preparation = self
            .workflows
            .prepare(
                &WorkflowPreparationEnvironment {
                    context: &self.context,
                    access,
                    settings_service: &self.settings,
                    secret_service: &self.secrets,
                    agent_service: &self.agents,
                },
                PrepareWorkflowInput {
                    kind: WorkflowKind::HealthCheck,
                    scope: Some(WorkflowScope::HealthCheck {
                        mode: HealthCheckMode::Complete,
                    }),
                    route_selection: Some(
                        llm_wiki_desktop_lib::models::workflow::WorkflowRouteSelection::Agent {
                            agent,
                        },
                    ),
                },
            )
            .unwrap();
        assert_eq!(
            preparation.project_access.filesystem_access,
            expected_filesystem_access
        );
        assert_eq!(preparation.project_access.persistence, expected_persistence);
        preparation.route.expect("installed Codex route")
    }

    fn configure_codex(&self, access: WorkflowAccessSnapshot) -> WorkflowRoute {
        self.configure_agent(access, AgentKind::Codex)
    }

    fn source_only(label: &str) -> Self {
        let mut fixture = Self::native(label);
        fs::remove_dir_all(&fixture.context.app_dir).unwrap();
        fs::remove_dir_all(fixture.context.root.join("wiki")).unwrap();
        fs::remove_dir_all(fixture.context.root.join("raw")).unwrap();
        fs::create_dir_all(fixture.context.root.join(".obsidian")).unwrap();
        fs::create_dir_all(fixture.context.root.join("raw/extracted")).unwrap();
        fs::write(
            fixture.context.root.join("raw/extracted/资料.md"),
            "---\ntype: source\n---\n# 资料\n",
        )
        .unwrap();
        fs::write(
            fixture.context.root.join("raw/extracted/invalid.md"),
            "# Invalid source\n",
        )
        .unwrap();
        fixture.context =
            ProjectContext::new(format!("health-{label}"), fixture.context.root.clone())
                .with_resolved_layout()
                .unwrap();
        fixture
    }

    fn mixed_compatible(label: &str) -> Self {
        let mut fixture = Self::native(label);
        fs::remove_dir_all(&fixture.context.app_dir).unwrap();
        fs::remove_dir_all(fixture.context.root.join("wiki")).unwrap();
        fs::remove_dir_all(fixture.context.root.join("raw")).unwrap();
        fs::create_dir_all(fixture.context.root.join(".obsidian")).unwrap();
        fs::create_dir_all(fixture.context.root.join("notes")).unwrap();
        fs::write(fixture.context.root.join("index.md"), "# Index\n").unwrap();
        fs::write(
            fixture.context.root.join("notes/shared.md"),
            "---\ntype: note\n---\n# Shared\n",
        )
        .unwrap();
        fixture.context =
            ProjectContext::new(format!("health-{label}"), fixture.context.root.clone())
                .with_resolved_layout()
                .unwrap();
        fixture
    }

    fn services(&self) -> HealthCheckExecutionServices<'_> {
        HealthCheckExecutionServices {
            lint_service: &self.lint,
            search_service: &self.search,
            settings_service: &self.settings,
            secret_service: &self.secrets,
            agent_service: &self.agents,
            llm_service: &self.llm,
            task_service: &self.tasks,
            coordinator: &self.workflows.coordinator,
        }
    }

    fn configure_ollama(&self) -> WorkflowRoute {
        self.settings
            .save_settings(
                &self.context,
                &Settings {
                    llm_providers: vec![LlmProviderConfig {
                        provider: LlmProviderKind::Ollama,
                        model: "qwen-health".into(),
                        base_url: "http://127.0.0.1:11434".into(),
                        context_window: 8192,
                        enabled: true,
                    }],
                    ..Settings::default()
                },
            )
            .unwrap();
        self.workflows
            .prepare(
                &WorkflowPreparationEnvironment {
                    context: &self.context,
                    access: trusted_persistent(),
                    settings_service: &self.settings,
                    secret_service: &self.secrets,
                    agent_service: &self.agents,
                },
                PrepareWorkflowInput {
                    kind: WorkflowKind::HealthCheck,
                    scope: Some(WorkflowScope::HealthCheck {
                        mode: HealthCheckMode::Complete,
                    }),
                    route_selection: Some(
                        llm_wiki_desktop_lib::models::workflow::WorkflowRouteSelection::Byok {
                            provider: LlmProviderKind::Ollama,
                        },
                    ),
                },
            )
            .unwrap()
            .route
            .expect("configured Ollama route")
    }

    fn enqueue(
        &self,
        mode: HealthCheckMode,
        route: WorkflowRoute,
        persistent: bool,
    ) -> llm_wiki_desktop_lib::models::workflow::WorkflowRun {
        let persistent = persistent
            && matches!(mode, HealthCheckMode::Complete)
            && matches!(&route, WorkflowRoute::Agent { .. });
        let scope = WorkflowScope::HealthCheck { mode };
        let baseline = workflow_baseline_for_scope(&self.context, &scope).unwrap();
        let outcome = self
            .workflows
            .coordinator
            .enqueue(
                &self.tasks,
                EnqueueWorkflow {
                    project_id: self.context.project_id.clone(),
                    project_root: self.context.root.clone(),
                    task_state_root: persistent.then(|| self.context.app_dir.join("tasks")),
                    title: "Health Check".into(),
                    kind: WorkflowKind::HealthCheck,
                    scope,
                    route: Some(route),
                    baseline_fingerprint: baseline.fingerprint,
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision: "health-test".into(),
                        ..WorkflowExecutionOptions::default()
                    },
                    stages: workflow_stages(&WorkflowKind::HealthCheck),
                    retry: None,
                },
            )
            .unwrap();
        match outcome {
            WorkflowStartOutcome::Created { run } => run,
            _ => panic!("first Health Check enqueue must create a run"),
        }
    }
}

fn git_status(root: &std::path::Path) -> String {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "-uall"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

fn trusted_persistent() -> WorkflowAccessSnapshot {
    WorkflowAccessSnapshot {
        trust: WorkflowProjectTrust::Trusted,
        trust_kind: Some(ProjectTrustKind::Native),
        filesystem_access: WorkflowFilesystemAccess::Writable,
        persistence: WorkflowPersistenceMode::Persistent,
        git_state: WorkflowGitState::Clean,
        authority_revision: "test-authority".into(),
    }
}

fn trusted_read_only() -> WorkflowAccessSnapshot {
    WorkflowAccessSnapshot {
        trust: WorkflowProjectTrust::Trusted,
        trust_kind: Some(ProjectTrustKind::Native),
        filesystem_access: WorkflowFilesystemAccess::ReadOnly,
        persistence: WorkflowPersistenceMode::MemoryOnly,
        git_state: WorkflowGitState::Clean,
        authority_revision: "test-read-only-authority".into(),
    }
}

#[tokio::test]
async fn local_quick_is_memory_only_skips_ai_and_keeps_eight_ordered_stages() {
    let fixture = Fixture::source_only("local");
    let run = fixture.enqueue(
        HealthCheckMode::LocalQuick,
        WorkflowRoute::Local {
            route_revision: "local-v1".into(),
        },
        false,
    );
    let task_id = run.task_id.clone();
    run_health_check_with_deep(&fixture.context, run, &fixture.services(), |_, _| async {
        panic!("Local Quick must not invoke an AI route")
    })
    .await;

    let finished = fixture.tasks.get_workflow_run(&task_id).unwrap();
    let ids = finished
        .stages
        .iter()
        .map(|stage| stage.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "read_markdown",
            "check_markdown",
            "check_links",
            "deep_check",
            "merge_findings",
            "classify_findings",
            "write_report",
            "complete",
        ]
    );
    assert_eq!(finished.stages[3].status, WorkflowStageStatus::Skipped);
    assert!(finished
        .stages
        .iter()
        .enumerate()
        .all(|(index, stage)| index == 3 || stage.status == WorkflowStageStatus::Completed));
    assert!(!fixture.context.root.join(".git").exists());
    assert!(!fixture.context.app_dir.exists());

    let stored = fixture
        .lint
        .read_lint_history_report(&fixture.context, &task_id)
        .unwrap();
    let report = stored.health_check_report.unwrap();
    assert!(!report.persistent);
    assert_eq!(report.coverage.source_pages, 2);
    assert_eq!(report.coverage.wiki_pages, 0);
    assert_eq!(report.coverage.deep_covered_pages, None);
    assert!(!report.coverage.deep_truncated);
    assert_eq!(report.coverage.not_applicable_rules, vec!["index_drift"]);
    assert!(report
        .issues
        .iter()
        .any(|issue| format!("{:?}", issue.issue_type) == "MissingFrontmatter"));
    assert!(!report
        .issues
        .iter()
        .any(|issue| format!("{:?}", issue.issue_type) == "IndexDrift"));
    assert!(matches!(
        finished.result,
        Some(WorkflowResult::HealthCheck {
            persistent: false,
            ..
        })
    ));
    assert!(finished.pending_action.is_none());
}

#[tokio::test]
async fn mixed_compatible_root_counts_as_source_and_wiki_so_index_drift_applies() {
    let fixture = Fixture::mixed_compatible("mixed");
    let run = fixture.enqueue(
        HealthCheckMode::LocalQuick,
        WorkflowRoute::Local {
            route_revision: "local-v1".into(),
        },
        false,
    );
    let task_id = run.task_id.clone();
    run_health_check_with_deep(&fixture.context, run, &fixture.services(), |_, _| async {
        panic!("Local Quick must not invoke an AI route")
    })
    .await;

    let stored = fixture
        .lint
        .read_lint_history_report(&fixture.context, &task_id)
        .unwrap();
    let report = stored.health_check_report.unwrap();
    assert_eq!(report.coverage.source_pages, 1);
    assert_eq!(report.coverage.wiki_pages, 2);
    assert!(!report
        .coverage
        .not_applicable_rules
        .contains(&"index_drift".to_string()));
    assert!(!report.issues.iter().any(|issue| {
        format!("{:?}", issue.issue_type) == "IndexDrift" && issue.path == "wiki/index.md"
    }));
}

#[tokio::test]
async fn complete_byok_runs_local_first_merges_duplicate_evidence_without_persistent_repair_owner()
{
    let fixture = Fixture::native("complete");
    let route = fixture.configure_ollama();
    let run = fixture.enqueue(HealthCheckMode::Complete, route, true);
    let task_id = run.task_id.clone();
    let observed_task = task_id.clone();
    let tasks = &fixture.tasks;
    run_health_check_with_deep(
        &fixture.context,
        run,
        &fixture.services(),
        move |snapshot, _| async move {
            let current = tasks.get_workflow_run(&observed_task).unwrap();
            assert_eq!(current.stages[1].status, WorkflowStageStatus::Completed);
            assert_eq!(current.stages[2].status, WorkflowStageStatus::Completed);
            assert!(snapshot.prompt.contains("schema"));
            assert!(snapshot.prompt.contains("raw/extracted/来源.md"));
            Ok(r#"[{"issueType":"schema_mismatch","severity":"error","path":"wiki/concepts/主题.md","message":"Deep schema finding","evidence":"deep evidence","suggestion":"Review schema"},{"issueType":"schema_mismatch","severity":"warning","path":"wiki/concepts/主题.md","message":"A distinct schema concern","evidence":"different evidence","suggestion":"Review another field"}]"#.into())
        },
    )
    .await;

    let finished = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert!(finished
        .stages
        .iter()
        .all(|stage| stage.status == WorkflowStageStatus::Completed));
    assert!(!fixture
        .context
        .app_dir
        .join(format!("lint-reports/{task_id}.json"))
        .is_file());
    assert!(!fixture.context.root.join(".git").exists());

    let stored = fixture
        .lint
        .read_lint_history_report(&fixture.context, &task_id)
        .unwrap();
    let report = stored.health_check_report.unwrap();
    assert!(!report.persistent);
    assert_eq!(report.coverage.source_pages, 1);
    assert_eq!(report.coverage.deep_covered_pages, Some(4));
    assert!(!report.coverage.deep_truncated);
    let schema_findings = report
        .issues
        .iter()
        .filter(|issue| {
            issue.path == "wiki/concepts/主题.md"
                && format!("{:?}", issue.issue_type) == "SchemaMismatch"
        })
        .collect::<Vec<_>>();
    assert_eq!(schema_findings.len(), 2);
    let merged = schema_findings
        .iter()
        .find(|issue| {
            issue
                .evidence
                .as_deref()
                .unwrap_or_default()
                .contains("deep evidence")
        })
        .unwrap();
    assert_eq!(format!("{:?}", merged.severity), "Error");
    assert!(merged
        .evidence
        .as_deref()
        .unwrap_or_default()
        .contains("deep evidence"));
    assert_eq!(report.finding_origins[&merged.id].len(), 2);
    assert!(schema_findings
        .iter()
        .any(|issue| report.finding_origins[&issue.id].len() == 1));
    assert_eq!(stored.entry.task_id.as_deref(), Some(task_id.as_str()));
}

#[tokio::test]
async fn markdown_change_during_deep_check_fails_recoverably_without_report() {
    let fixture = Fixture::native("baseline");
    let route = fixture.configure_ollama();
    let run = fixture.enqueue(HealthCheckMode::Complete, route, true);
    let task_id = run.task_id.clone();
    let changed = fixture.context.root.join("wiki/concepts/主题.md");
    run_health_check_with_deep(
        &fixture.context,
        run,
        &fixture.services(),
        move |_, _| async move {
            fs::write(changed, "# 主题\n\nChanged while checking.\n").unwrap();
            Ok("[]".into())
        },
    )
    .await;

    let failed = fixture.tasks.get_workflow_run(&task_id).unwrap();
    let error = failed.error.expect("recoverable workflow error");
    assert_eq!(error.code, "WORKFLOW_INPUT_BASELINE_CHANGED");
    assert!(error.recoverable);
    assert!(!fixture
        .context
        .app_dir
        .join(format!("lint-reports/{task_id}.json"))
        .exists());
}

#[tokio::test]
async fn stale_complete_route_fails_in_deep_stage_without_downgrading() {
    let fixture = Fixture::native("route-stale");
    let route = fixture.configure_ollama();
    let run = fixture.enqueue(HealthCheckMode::Complete, route, true);
    let task_id = run.task_id.clone();
    fixture
        .settings
        .save_settings(&fixture.context, &Settings::default())
        .unwrap();
    run_health_check_with_deep(&fixture.context, run, &fixture.services(), |_, _| async {
        panic!("a stale prepared route must fail before deep execution")
    })
    .await;

    let failed = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert_eq!(
        failed.error.as_ref().map(|error| error.code.as_str()),
        Some("WORKFLOW_ROUTE_UNAVAILABLE")
    );
    assert_eq!(failed.stages[3].status, WorkflowStageStatus::Failed);
    assert!(!failed
        .stages
        .iter()
        .skip(4)
        .any(|stage| stage.status == WorkflowStageStatus::Completed));
}

#[tokio::test]
async fn forged_agent_complete_route_fails_before_agent_invocation() {
    let fixture = Fixture::native("forged-agent-route");
    let run = fixture.enqueue(
        HealthCheckMode::Complete,
        WorkflowRoute::Agent {
            agent: AgentKind::Codex,
            model: None,
            route_revision: "forged-agent-route".into(),
        },
        true,
    );
    let task_id = run.task_id.clone();

    run_health_check_with_deep(&fixture.context, run, &fixture.services(), |_, _| async {
        panic!("a disabled Agent route must fail before deep execution")
    })
    .await;

    let failed = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert_eq!(
        failed.error.as_ref().map(|error| error.code.as_str()),
        Some("WORKFLOW_ROUTE_UNAVAILABLE")
    );
}

#[tokio::test]
async fn trusted_read_only_codex_health_is_memory_only_and_does_not_mutate_the_project() {
    let runner = Arc::new(SuccessfulCodex::default());
    let fixture = Fixture::native_with_runner("agent-readonly", runner.clone());
    let route = fixture.configure_codex(trusted_read_only());
    let run = fixture.enqueue(HealthCheckMode::Complete, route, false);
    let task_id = run.task_id.clone();
    let wiki_before = fs::read(fixture.context.root.join("wiki/concepts/主题.md")).unwrap();
    let source_before = fs::read(fixture.context.root.join("raw/extracted/来源.md")).unwrap();

    run_health_check(&fixture.context, run, &fixture.services()).await;

    let finished = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert!(finished.error.is_none());
    assert_eq!(runner.invocations.load(Ordering::SeqCst), 1);
    assert_eq!(
        fs::read(fixture.context.root.join("wiki/concepts/主题.md")).unwrap(),
        wiki_before
    );
    assert_eq!(
        fs::read(fixture.context.root.join("raw/extracted/来源.md")).unwrap(),
        source_before
    );
    assert!(!fixture.context.root.join(".git").exists());
    assert!(!fixture
        .context
        .app_dir
        .join(format!("lint-reports/{task_id}.json"))
        .is_file());
}

#[tokio::test]
async fn trusted_read_only_claude_health_uses_the_pinned_read_only_profile() {
    let runner = Arc::new(SuccessfulClaude::default());
    let fixture = Fixture::native_with_runner("agent-readonly-claude", runner.clone());
    let route = fixture.configure_agent(trusted_read_only(), AgentKind::Claude);
    let run = fixture.enqueue(HealthCheckMode::Complete, route, false);
    let task_id = run.task_id.clone();
    let wiki_before = fs::read(fixture.context.root.join("wiki/concepts/主题.md")).unwrap();
    let source_before = fs::read(fixture.context.root.join("raw/extracted/来源.md")).unwrap();

    run_health_check(&fixture.context, run, &fixture.services()).await;

    let finished = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert!(finished.error.is_none());
    assert_eq!(runner.invocations.load(Ordering::SeqCst), 1);
    assert_eq!(
        fs::read(fixture.context.root.join("wiki/concepts/主题.md")).unwrap(),
        wiki_before
    );
    assert_eq!(
        fs::read(fixture.context.root.join("raw/extracted/来源.md")).unwrap(),
        source_before
    );
    assert!(!fixture
        .context
        .app_dir
        .join(format!("lint-reports/{task_id}.json"))
        .is_file());
}

#[tokio::test]
async fn trusted_writable_agent_health_persists_report_for_repair_preparation_after_restart() {
    let runner = Arc::new(SuccessfulCodex::default());
    let fixture = Fixture::native_with_runner("agent-persistent-git", runner.clone());
    let route = fixture.configure_codex(trusted_persistent());
    let git = GitService;
    git.initialize_repository(&fixture.context, "Initial Health fixture")
        .unwrap();
    let before = git_status(&fixture.context.root);
    assert!(before.is_empty());
    let wiki_before = fs::read(fixture.context.root.join("wiki/concepts/主题.md")).unwrap();
    let run = fixture.enqueue(HealthCheckMode::Complete, route.clone(), true);
    let task_id = run.task_id.clone();

    run_health_check(&fixture.context, run, &fixture.services()).await;

    let finished = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert!(finished.error.is_none());
    assert_eq!(runner.invocations.load(Ordering::SeqCst), 1);
    assert!(matches!(
        finished.result.as_ref(),
        Some(WorkflowResult::HealthCheck {
            report_id: Some(report_id),
            persistent: true,
            ..
        }) if report_id == &task_id
    ));
    assert!(fixture
        .context
        .app_dir
        .join(format!("lint-reports/{task_id}.json"))
        .is_file());
    let before_restart = fixture
        .lint
        .read_current_health_report(&fixture.context, &task_id)
        .unwrap();
    assert!(before_restart.persistent);

    // Repair preparation may run after the original runner/service instance
    // is gone. The persistent report is the same authorized body, while the
    // completed Workflow owner and current project identity remain the source
    // of authority for repair.
    let restarted_lint = LintService::default();
    let after_restart = restarted_lint
        .read_current_health_report(&fixture.context, &task_id)
        .unwrap();
    let identity = project_identity(&fixture.context.root).unwrap();
    LintService::validate_current_health_report_owner(
        &after_restart,
        &finished,
        &fixture.context.project_id,
        &identity.canonical_identity_key,
        &identity.identity_revision,
        &route,
        &finished.baseline_fingerprint,
    )
    .unwrap();
    assert!(after_restart
        .issues
        .iter()
        .any(|issue| issue.id == "schema_mismatch:wiki/concepts/主题.md"));
    let after = git_status(&fixture.context.root);
    assert!(
        after.lines().all(|line| line.contains(".app/")),
        "persistent Health may create app-owned report/task metadata only: {after}"
    );
    assert_eq!(
        fs::read(fixture.context.root.join("wiki/concepts/主题.md")).unwrap(),
        wiki_before
    );
    assert!(
        before.is_empty(),
        "fixture must start from a clean Git tree"
    );
}

#[tokio::test]
async fn agent_version_drift_during_snapshot_fails_at_launch_with_zero_invocations() {
    let runner = Arc::new(DriftingCodex::default());
    let fixture = Fixture::native_with_runner("agent-route-drift", runner.clone());
    let route = fixture.configure_codex(trusted_read_only());
    let run = fixture.enqueue(HealthCheckMode::Complete, route, false);
    let task_id = run.task_id.clone();

    run_health_check(&fixture.context, run, &fixture.services()).await;

    let failed = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert_eq!(
        failed.error.as_ref().map(|error| error.code.as_str()),
        Some("WORKFLOW_ROUTE_UNAVAILABLE")
    );
    assert_eq!(runner.invocations.load(Ordering::SeqCst), 0);
    assert!(runner.version_probes.load(Ordering::SeqCst) >= 3);
}

#[tokio::test]
async fn same_version_shim_retarget_fails_before_agent_invocation() {
    let target = tempfile::tempdir().unwrap();
    let shim = target.path().join("codex.cmd");
    let script_a = target.path().join("codex-a.js");
    let script_b = target.path().join("codex-b.js");
    fs::write(&shim, "stable shim").unwrap();
    fs::write(&script_a, "script a").unwrap();
    fs::write(&script_b, "script b").unwrap();
    let runner = Arc::new(RetargetedCodex {
        shim,
        scripts: [script_a, script_b],
        active: AtomicUsize::new(0),
        invocations: AtomicUsize::new(0),
    });
    let fixture = Fixture::native_with_runner("agent-target-drift", runner.clone());
    let route = fixture.configure_codex(trusted_read_only());
    let run = fixture.enqueue(HealthCheckMode::Complete, route, false);
    let task_id = run.task_id.clone();
    runner.active.store(1, Ordering::SeqCst);

    run_health_check(&fixture.context, run, &fixture.services()).await;

    let failed = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert_eq!(
        failed.error.as_ref().map(|error| error.code.as_str()),
        Some("WORKFLOW_ROUTE_UNAVAILABLE")
    );
    assert_eq!(runner.invocations.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancelled_agent_health_stops_before_agent_invocation_or_report_write() {
    let runner = Arc::new(SuccessfulCodex::default());
    let fixture = Fixture::native_with_runner("agent-cancel", runner.clone());
    let route = fixture.configure_codex(trusted_read_only());
    let run = fixture.enqueue(HealthCheckMode::Complete, route, false);
    let task_id = run.task_id.clone();
    fixture
        .workflows
        .coordinator
        .cancel(&fixture.tasks, &task_id)
        .unwrap();

    run_health_check(&fixture.context, run, &fixture.services()).await;

    let cancelled = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert_eq!(
        cancelled.display_status,
        llm_wiki_desktop_lib::models::workflow::WorkflowDisplayStatus::Cancelled
    );
    assert_eq!(runner.invocations.load(Ordering::SeqCst), 0);
    assert!(!fixture
        .context
        .app_dir
        .join(format!("lint-reports/{task_id}.json"))
        .exists());
}

#[tokio::test]
async fn cancellation_during_launch_probe_prevents_agent_spawn() {
    let runner = Arc::new(CancelDuringLaunchProbe::default());
    let fixture = Fixture::native_with_runner("agent-cancel-probe", runner.clone());
    let route = fixture.configure_codex(trusted_read_only());
    let run = fixture.enqueue(HealthCheckMode::Complete, route, false);
    let task_id = run.task_id.clone();
    *runner.cancel.lock().unwrap() = Some((fixture.tasks.clone(), task_id.clone()));

    run_health_check(&fixture.context, run, &fixture.services()).await;

    assert!(fixture.tasks.is_cancelled(&task_id));
    assert!(runner.version_probes.load(Ordering::SeqCst) >= 3);
    assert_eq!(runner.invocations.load(Ordering::SeqCst), 0);
    assert!(!fixture
        .context
        .app_dir
        .join(format!("lint-reports/{task_id}.json"))
        .exists());
}

#[tokio::test]
async fn markdown_change_during_final_launch_probe_prevents_agent_spawn() {
    let mut fixture = Fixture::native("agent-launch-stale");
    let target = fs::read_dir(fixture.context.root.join("wiki/concepts"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let runner = Arc::new(MutateDuringLaunchProbe {
        version_probes: AtomicUsize::new(0),
        invocations: AtomicUsize::new(0),
        target,
    });
    fixture.agents = AgentService::with_runner(runner.clone());
    let route = fixture.configure_codex(trusted_read_only());
    let run = fixture.enqueue(HealthCheckMode::Complete, route, false);
    let task_id = run.task_id.clone();

    run_health_check(&fixture.context, run, &fixture.services()).await;

    let failed = fixture.tasks.get_workflow_run(&task_id).unwrap();
    assert!(failed.error.is_some());
    assert_eq!(runner.invocations.load(Ordering::SeqCst), 0);
    assert!(!fixture
        .context
        .app_dir
        .join(format!("lint-reports/{task_id}.json"))
        .exists());
}
