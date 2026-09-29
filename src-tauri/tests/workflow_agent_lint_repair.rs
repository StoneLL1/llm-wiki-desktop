use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::agent::AgentKind;
use llm_wiki_desktop_lib::models::confirmation::ConfirmationRegistry;
use llm_wiki_desktop_lib::models::graph::GraphData;
use llm_wiki_desktop_lib::models::lint::{
    AgentLintRepairDeclaredChange, AgentLintRepairDeclaredChangeOperation, AgentLintRepairFinding,
    AgentLintRepairFindingResult, AgentLintRepairFindingStatus, AgentLintRepairOperation,
    AgentLintRepairOutcome, AgentLintRepairRoundOutput, DeepLintIssueType, LintSeverity,
    WikiLintSkillRef, WIKI_LINT_SCHEMA_VERSION,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::settings::AgentLintRepairAttestationLifecycle;
use llm_wiki_desktop_lib::models::workflow::{
    HealthCheckMode, WorkflowDisplayStatus, WorkflowExecutionOptions, WorkflowKind,
    WorkflowOperation, WorkflowResult, WorkflowRoute, WorkflowScope, WorkflowStartOutcome,
};
use llm_wiki_desktop_lib::services::{
    agent_lint_repair_attestation_digest, agent_lint_repair_result_digest,
    agent_lint_repair_stages, agent_lint_repair_terminal_file_diff_page,
    confirm_agent_lint_repair_review_with_round_executor, project_identity,
    reconcile_agent_lint_repair_after_recovery, run_agent_lint_repair_with_round_executor,
    AgentLintRepairExecutionServices, AgentService, BookmarkService, EnqueueWorkflow, FileStore,
    GitService, LintService, SearchService, WorkflowCoordinator,
};
use llm_wiki_desktop_lib::tasks::TaskService;

struct Fixture {
    _temp: tempfile::TempDir,
    context: ProjectContext,
    task_service: TaskService,
    coordinator: WorkflowCoordinator,
    agent_service: AgentService,
    lint_service: LintService,
    git_service: GitService,
    file_store: FileStore,
    bookmark_service: BookmarkService,
    search_service: SearchService,
    confirmation_registry: ConfirmationRegistry,
    settings_service: llm_wiki_desktop_lib::services::SettingsService,
}

impl Fixture {
    fn new(with_git: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let config_dir = temp.path().join("app-config");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(root.join("wiki/concepts")).unwrap();
        fs::create_dir_all(root.join("wiki/sources")).unwrap();
        fs::create_dir_all(root.join("raw/sources")).unwrap();
        fs::create_dir_all(root.join(".app/tasks")).unwrap();
        fs::write(
            root.join("wiki/concepts/page.md"),
            "---\ntitle: Page\ntype: concept\n---\n\n# Page\n\nOriginal\n",
        )
        .unwrap();
        fs::write(
            root.join("wiki/sources/source-a.md"),
            "---\ntitle: Source A\ntype: source\n---\n\n# Source A\n",
        )
        .unwrap();
        fs::write(root.join("purpose.md"), "# Purpose\n").unwrap();
        fs::write(root.join("schema.md"), "# Schema\n").unwrap();
        let context = ProjectContext::new("project-a", root.clone());
        let fixture = Self {
            _temp: temp,
            context,
            task_service: TaskService::default(),
            coordinator: WorkflowCoordinator::default(),
            agent_service: AgentService::default(),
            lint_service: LintService::default(),
            git_service: GitService,
            file_store: FileStore,
            bookmark_service: BookmarkService::default(),
            search_service: SearchService::default(),
            confirmation_registry: ConfirmationRegistry::default(),
            settings_service: llm_wiki_desktop_lib::services::SettingsService::with_config_dir(
                config_dir,
            ),
        };
        if with_git {
            fixture
                .git_service
                .initialize_repository(&fixture.context, "Initial project")
                .unwrap();
        }
        fixture
    }

    fn services(&self) -> AgentLintRepairExecutionServices<'_> {
        AgentLintRepairExecutionServices {
            agent_service: &self.agent_service,
            lint_service: &self.lint_service,
            git_service: &self.git_service,
            file_store: &self.file_store,
            bookmark_service: &self.bookmark_service,
            search_service: &self.search_service,
            confirmation_registry: &self.confirmation_registry,
            settings_service: &self.settings_service,
            task_service: &self.task_service,
            coordinator: &self.coordinator,
        }
    }

    fn enqueue(&self) -> llm_wiki_desktop_lib::models::workflow::WorkflowRun {
        self.enqueue_finding(
            "missing_source:wiki/concepts/page.md",
            DeepLintIssueType::MissingSource,
            "Missing source",
        )
    }

    fn enqueue_finding(
        &self,
        finding_id: &str,
        issue_type: DeepLintIssueType,
        message: &str,
    ) -> llm_wiki_desktop_lib::models::workflow::WorkflowRun {
        self.enqueue_findings(vec![AgentLintRepairFinding {
            id: finding_id.into(),
            issue_type,
            severity: LintSeverity::Warning,
            path: "wiki/concepts/page.md".into(),
            message: message.into(),
            evidence: None,
            suggested_action: None,
        }])
    }

    fn enqueue_findings(
        &self,
        mut findings: Vec<AgentLintRepairFinding>,
    ) -> llm_wiki_desktop_lib::models::workflow::WorkflowRun {
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        let identity = project_identity(&self.context.root).unwrap();
        let authorized_path_hashes = findings
            .iter()
            .map(|finding| {
                (
                    finding.path.clone(),
                    self.file_store
                        .file_hash_if_exists(&self.context, &finding.path)
                        .unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let expected_git_head = self
            .git_service
            .repository_status(&self.context)
            .unwrap()
            .head
            .unwrap_or_else(|| "0".repeat(40));
        let operation = WorkflowOperation::AgentLintRepair {
            preparation_id: "repair-preparation".into(),
            preparation_revision: "repair-preparation-revision".into(),
            report_id: "health-report".into(),
            selection_revision: "selection-revision".into(),
            selected_finding_ids: findings.iter().map(|finding| finding.id.clone()).collect(),
            selected_findings: findings,
            skill: WikiLintSkillRef::builtin(),
            authorized_path_hashes,
            expected_git_head,
        };
        let execution_options = WorkflowExecutionOptions {
            preparation_revision: "repair-preparation-revision".into(),
            operation,
            ..WorkflowExecutionOptions::default()
        };
        let outcome = self
            .coordinator
            .enqueue(
                &self.task_service,
                EnqueueWorkflow {
                    project_id: self.context.project_id.clone(),
                    project_root: self.context.root.clone(),
                    task_state_root: Some(self.context.app_dir.join("tasks")),
                    title: "Agent lint repair".into(),
                    kind: WorkflowKind::HealthCheck,
                    scope: WorkflowScope::HealthCheck {
                        mode: HealthCheckMode::Complete,
                    },
                    route: Some(WorkflowRoute::Agent {
                        agent: AgentKind::Codex,
                        model: None,
                        route_revision: "repair-route-revision".into(),
                    }),
                    baseline_fingerprint: "baseline-revision".into(),
                    execution_options: execution_options.clone(),
                    stages: agent_lint_repair_stages(),
                    retry: None,
                },
            )
            .unwrap();
        let run = match outcome {
            WorkflowStartOutcome::Created { run } => run,
            WorkflowStartOutcome::Existing { .. } => panic!("expected a new repair run"),
        };
        assert_eq!(run.canonical_identity_key, identity.canonical_identity_key);
        assert_eq!(run.display_status, WorkflowDisplayStatus::Running);
        let digest = agent_lint_repair_attestation_digest(&run, &execution_options).unwrap();
        self.settings_service
            .record_agent_lint_repair_attestation(
                &run.canonical_identity_key,
                &run.identity_revision,
                &run.task_id,
                &digest,
            )
            .unwrap();
        self.settings_service
            .transition_agent_lint_repair_attestation(
                &run.task_id,
                &digest,
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
                AgentLintRepairAttestationLifecycle::Dispatched,
            )
            .unwrap();
        run
    }
}

fn output(
    request: &llm_wiki_desktop_lib::models::lint::AgentLintRepairRequest,
    status: AgentLintRepairFindingStatus,
) -> String {
    let value = AgentLintRepairRoundOutput {
        schema_version: WIKI_LINT_SCHEMA_VERSION,
        operation: AgentLintRepairOperation::Repair,
        skill: WikiLintSkillRef::builtin(),
        report_id: request.report_id.clone(),
        selection_revision: request.selection_revision.clone(),
        round: request.round,
        finding_results: vec![AgentLintRepairFindingResult {
            finding_id: request.findings[0].id.clone(),
            status,
            message: "bounded candidate".into(),
        }],
        declared_changes: vec![AgentLintRepairDeclaredChange {
            path: "wiki/concepts/page.md".into(),
            operation: AgentLintRepairDeclaredChangeOperation::Update,
        }],
        summary: format!("round {}", request.round),
    };
    format!("```json\n{}\n```", serde_json::to_string(&value).unwrap())
}

fn output_with_changes(
    request: &llm_wiki_desktop_lib::models::lint::AgentLintRepairRequest,
    status: AgentLintRepairFindingStatus,
    declared_changes: Vec<AgentLintRepairDeclaredChange>,
) -> String {
    let value = AgentLintRepairRoundOutput {
        schema_version: WIKI_LINT_SCHEMA_VERSION,
        operation: AgentLintRepairOperation::Repair,
        skill: WikiLintSkillRef::builtin(),
        report_id: request.report_id.clone(),
        selection_revision: request.selection_revision.clone(),
        round: request.round,
        finding_results: request
            .findings
            .iter()
            .map(|finding| AgentLintRepairFindingResult {
                finding_id: finding.id.clone(),
                status,
                message: "candidate evaluated".into(),
            })
            .collect(),
        declared_changes,
        summary: format!("round {}", request.round),
    };
    format!("```json\n{}\n```", serde_json::to_string(&value).unwrap())
}

fn delete_output(request: &llm_wiki_desktop_lib::models::lint::AgentLintRepairRequest) -> String {
    let value = AgentLintRepairRoundOutput {
        schema_version: WIKI_LINT_SCHEMA_VERSION,
        operation: AgentLintRepairOperation::Repair,
        skill: WikiLintSkillRef::builtin(),
        report_id: request.report_id.clone(),
        selection_revision: request.selection_revision.clone(),
        round: request.round,
        finding_results: vec![AgentLintRepairFindingResult {
            finding_id: request.findings[0].id.clone(),
            status: AgentLintRepairFindingStatus::Attempted,
            message: "delete duplicate".into(),
        }],
        declared_changes: vec![AgentLintRepairDeclaredChange {
            path: "wiki/concepts/page.md".into(),
            operation: AgentLintRepairDeclaredChangeOperation::Delete,
        }],
        summary: "delete duplicate".into(),
    };
    format!("```json\n{}\n```", serde_json::to_string(&value).unwrap())
}

fn git_commit_count(root: &Path) -> usize {
    let output = std::process::Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn commit_path(root: &Path, path: &str, message: &str) {
    let add = std::process::Command::new("git")
        .args(["add", "--", path])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let commit = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=LLM Wiki Desktop Tests",
            "-c",
            "user.email=tests@llm-wiki.local",
            "commit",
            "-m",
            message,
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
}

fn rewrite_persisted_run_as_running(fixture: &Fixture, task_id: &str) {
    let persisted_path = fixture
        .context
        .app_dir
        .join("tasks")
        .join(format!("{task_id}.json"));
    let mut persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(&persisted_path).unwrap()).unwrap();
    persisted["task"]["status"] = serde_json::json!("running");
    persisted["task"]["completedAt"] = serde_json::Value::Null;
    persisted["workflow"]["result"] = serde_json::Value::Null;
    persisted["workflow"]["error"] = serde_json::Value::Null;
    persisted["workflow"]["currentStageId"] = serde_json::json!("finalize_repair");
    if let Some(final_stage) = persisted["workflow"]["stages"]
        .as_array_mut()
        .and_then(|stages| stages.last_mut())
    {
        final_stage["status"] = serde_json::json!("running");
        final_stage["completedAt"] = serde_json::Value::Null;
    }
    fs::write(
        &persisted_path,
        serde_json::to_vec_pretty(&persisted).unwrap(),
    )
    .unwrap();
}

#[test]
fn happy_path_uses_one_agent_round_and_private_history() {
    let fixture = Fixture::new(true);
    let before_commits = git_commit_count(&fixture.context.root);
    let run = fixture.enqueue();
    let invocations = AtomicUsize::new(0);

    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            invocations.fetch_add(1, Ordering::SeqCst);
            fs::write(
                workspace.join("wiki/concepts/page.md"),
                format!(
                    "---\ntitle: Page\ntype: concept\nsources:\n  - wiki/sources/source-a.md\n---\n\n# Page\n\nRepaired round {}\n\n> Sources: [[wiki/sources/source-a.md]]\n",
                    request.round
                ),
            )
            .unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );

    assert_eq!(
        invocations.load(Ordering::SeqCst),
        1,
        "run={:#?}",
        fixture.task_service.get_workflow_run(&run.task_id)
    );
    assert_eq!(git_commit_count(&fixture.context.root), before_commits);
    let completed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(
        completed.display_status,
        WorkflowDisplayStatus::Completed,
        "{:?}",
        completed.error
    );
    match completed.result.unwrap() {
        WorkflowResult::AgentLintRepair {
            outcome,
            rounds,
            final_commit,
            rollback_available,
            affected_path_hashes,
            ..
        } => {
            assert_eq!(outcome, AgentLintRepairOutcome::Succeeded);
            assert_eq!(rounds.len(), 1);
            assert!(final_commit.is_some());
            assert!(rollback_available);
            assert!(affected_path_hashes.contains_key("wiki/concepts/page.md"));
            assert!(affected_path_hashes.contains_key(".app/graph-cache.json"));
        }
        other => panic!("unexpected result: {other:?}"),
    }
    let graph: GraphData = serde_json::from_slice(
        &fs::read(fixture.context.app_dir.join("graph-cache.json")).unwrap(),
    )
    .unwrap();
    assert!(
        graph.content_hash.is_empty(),
        "Agent repair must leave a parseable stale cache so project trust survives"
    );
}

#[test]
fn unrelated_staged_and_unstaged_edits_survive_private_repair_history() {
    let fixture = Fixture::new(true);
    fs::write(
        fixture.context.root.join("unrelated-staged.txt"),
        "staged\n",
    )
    .unwrap();
    let staged = std::process::Command::new("git")
        .args(["add", "unrelated-staged.txt"])
        .current_dir(&fixture.context.root)
        .status()
        .unwrap();
    assert!(staged.success());
    fs::write(
        fixture.context.root.join("unrelated-unstaged.txt"),
        "unstaged\n",
    )
    .unwrap();
    let before_head = fixture
        .git_service
        .repository_status(&fixture.context)
        .unwrap()
        .head;
    let before_commits = git_commit_count(&fixture.context.root);
    let run = fixture.enqueue();
    assert!(std::process::Command::new("git")
        .args(["config", "--local", "filter.unrelated.clean", "/bin/false"])
        .current_dir(&fixture.context.root)
        .status()
        .unwrap()
        .success());
    if let WorkflowOperation::AgentLintRepair {
        expected_git_head, ..
    } = &run.operation
    {
        assert!(
            fixture
                .git_service
                .head_matches(&fixture.context, expected_git_head)
                .unwrap(),
            "the pinned HEAD must remain readable with unrelated filters: expected={expected_git_head:?}, actual={:?}",
            std::process::Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .current_dir(&fixture.context.root)
                .output()
                .unwrap()
                .stdout
        );
    }
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            fs::write(
                workspace.join("wiki/concepts/page.md"),
                "---\ntitle: Page\ntype: concept\nsources:\n  - wiki/sources/source-a.md\n---\n\n# Page\n\nRepaired\n\n> Sources: [[wiki/sources/source-a.md]]\n",
            )
            .unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );
    let completed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert!(std::process::Command::new("git")
        .args(["config", "--local", "--unset-all", "filter.unrelated.clean"])
        .current_dir(&fixture.context.root)
        .status()
        .unwrap()
        .success());
    assert_eq!(
        completed.display_status,
        WorkflowDisplayStatus::Completed,
        "{:?}",
        completed.error
    );
    assert_eq!(
        fixture
            .git_service
            .repository_status(&fixture.context)
            .unwrap()
            .head,
        before_head
    );
    assert_eq!(git_commit_count(&fixture.context.root), before_commits);
    assert_eq!(
        fs::read_to_string(fixture.context.root.join("unrelated-staged.txt")).unwrap(),
        "staged\n"
    );
    assert_eq!(
        fs::read_to_string(fixture.context.root.join("unrelated-unstaged.txt")).unwrap(),
        "unstaged\n"
    );
    let staged_names = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&fixture.context.root)
        .output()
        .unwrap();
    assert!(String::from_utf8(staged_names.stdout)
        .unwrap()
        .lines()
        .any(|name| name == "unrelated-staged.txt"));
    assert!(fixture
        .git_service
        .history_snapshot(&fixture.context, &run.task_id, "before")
        .unwrap()
        .is_some());
    assert!(fixture
        .git_service
        .history_snapshot(&fixture.context, &run.task_id, "after")
        .unwrap()
        .is_some());
    let WorkflowResult::AgentLintRepair {
        affected_paths,
        checkpoint_hash: Some(checkpoint),
        final_commit: Some(final_commit),
        ..
    } = completed.result.unwrap()
    else {
        panic!("completed repair must have private history");
    };
    let mut rollback_paths = affected_paths;
    rollback_paths.push(".app/graph-cache.json".into());
    rollback_paths.sort();
    rollback_paths.dedup();
    let before = fixture
        .git_service
        .read_history_files(&fixture.context, &checkpoint, &rollback_paths)
        .unwrap();
    let after = fixture
        .git_service
        .read_history_files(&fixture.context, &final_commit, &rollback_paths)
        .unwrap();
    let page_path = fixture.context.root.join("wiki/concepts/page.md");
    fs::write(&page_path, "External edit after repair\n").unwrap();
    assert_eq!(
        llm_wiki_desktop_lib::services::CompileService::prepare_history_restore(
            &fixture.context,
            &before,
            &after,
        )
        .err()
        .unwrap()
        .code,
        "WORKFLOW_UNDO_CONFLICT"
    );
    fs::write(
        &page_path,
        after["wiki/concepts/page.md"].as_deref().unwrap(),
    )
    .unwrap();
    let undo_started = fixture
        .git_service
        .create_history_snapshot(
            &fixture.context,
            &run.task_id,
            "undo-started",
            "Before Agent lint repair undo",
            Some(&final_commit),
            &after,
        )
        .unwrap();
    fs::write(
        &page_path,
        before["wiki/concepts/page.md"].as_deref().unwrap(),
    )
    .unwrap();
    let current = fixture
        .git_service
        .capture_history_files(&fixture.context, &rollback_paths, None)
        .unwrap();
    let restore = llm_wiki_desktop_lib::services::CompileService::prepare_history_restore(
        &fixture.context,
        &before,
        &current,
    )
    .unwrap();
    llm_wiki_desktop_lib::services::CompileService::restore_prepared_history_outputs(
        &fixture.context,
        &restore,
    )
    .unwrap();
    fixture
        .git_service
        .create_history_snapshot(
            &fixture.context,
            &run.task_id,
            "undo",
            "Agent lint repair undo",
            undo_started.commit_hash.as_deref(),
            &before,
        )
        .unwrap();
    assert!(
        fs::read_to_string(fixture.context.root.join("wiki/concepts/page.md"))
            .unwrap()
            .contains("Original")
    );
    assert_eq!(
        fixture
            .git_service
            .repository_status(&fixture.context)
            .unwrap()
            .head,
        before_head
    );
    assert_eq!(git_commit_count(&fixture.context.root), before_commits);
    assert_eq!(
        fs::read_to_string(fixture.context.root.join("unrelated-unstaged.txt")).unwrap(),
        "unstaged\n"
    );
    let staged_names = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(&fixture.context.root)
        .output()
        .unwrap();
    assert!(String::from_utf8(staged_names.stdout)
        .unwrap()
        .lines()
        .any(|name| name == "unrelated-staged.txt"));
}

#[test]
fn unresolved_without_verified_progress_stops_after_one_round() {
    let fixture = Fixture::new(true);
    let run = fixture.enqueue();
    let invocations = AtomicUsize::new(0);

    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            invocations.fetch_add(1, Ordering::SeqCst);
            fs::write(
                workspace.join("wiki/concepts/page.md"),
                format!(
                    "---\ntitle: Page\ntype: concept\n---\n\n# Page\n\nStill unresolved {}\n",
                    request.round
                ),
            )
            .unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Skipped))
        },
    );

    assert_eq!(invocations.load(Ordering::SeqCst), 1);
    let completed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    let terminal_diff = agent_lint_repair_terminal_file_diff_page(
        &fixture.context,
        &completed,
        &fixture.services(),
        "file-0",
        0,
        64 * 1024,
    )
    .unwrap()
    .unwrap();
    assert_eq!(terminal_diff.path, "wiki/concepts/page.md");
    assert!(terminal_diff.diff.contains("Still unresolved 1"));
    match completed.result.unwrap() {
        WorkflowResult::AgentLintRepair {
            outcome,
            rounds,
            unresolved_finding_ids,
            ..
        } => {
            assert_eq!(outcome, AgentLintRepairOutcome::ManualReviewRequired);
            assert_eq!(rounds.len(), 1);
            assert_eq!(
                unresolved_finding_ids,
                ["missing_source:wiki/concepts/page.md"]
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn agent_attempted_claim_never_resolves_a_semantic_finding_without_backend_proof() {
    let fixture = Fixture::new(true);
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Possible duplicate topic",
    );
    let invocations = AtomicUsize::new(0);

    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            invocations.fetch_add(1, Ordering::SeqCst);
            fs::write(
                workspace.join("wiki/concepts/page.md"),
                format!(
                    "---\ntitle: Page\ntype: concept\n---\n\n# Page\n\nAgent says attempted {}\n",
                    request.round
                ),
            )
            .unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );

    assert_eq!(invocations.load(Ordering::SeqCst), 1);
    let completed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    match completed.result.unwrap() {
        WorkflowResult::AgentLintRepair {
            outcome,
            unresolved_finding_ids,
            ..
        } => {
            assert_eq!(outcome, AgentLintRepairOutcome::ManualReviewRequired);
            assert_eq!(
                unresolved_finding_ids,
                ["duplicate_topic:wiki/concepts/page.md"]
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn checkpoint_failure_never_invokes_the_agent() {
    let fixture = Fixture::new(false);
    let run = fixture.enqueue();
    let invocations = AtomicUsize::new(0);

    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |_, _, _| {
            invocations.fetch_add(1, Ordering::SeqCst);
            unreachable!("Agent must not run without a Git checkpoint")
        },
    );

    assert_eq!(invocations.load(Ordering::SeqCst), 0);
    let failed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(failed.display_status, WorkflowDisplayStatus::Failed);
    assert_eq!(
        failed.error.unwrap().project_mutation_state,
        llm_wiki_desktop_lib::models::workflow::WorkflowProjectMutationState::NotModified
    );

    // Simulate a crash after the app-owned terminal receipt was durable but
    // before the project task reached Failed. Recovery must accept only this
    // empty/no-checkpoint failure shape and restore the exact terminal status.
    rewrite_persisted_run_as_running(&fixture, &run.task_id);
    let recovered_tasks = TaskService::default();
    recovered_tasks
        .recover_tasks(&fixture.context.root)
        .unwrap();
    let interrupted = recovered_tasks.get_workflow_run(&run.task_id).unwrap();
    let recovered_services = AgentLintRepairExecutionServices {
        agent_service: &fixture.agent_service,
        lint_service: &fixture.lint_service,
        git_service: &fixture.git_service,
        file_store: &fixture.file_store,
        bookmark_service: &fixture.bookmark_service,
        search_service: &fixture.search_service,
        confirmation_registry: &fixture.confirmation_registry,
        settings_service: &fixture.settings_service,
        task_service: &recovered_tasks,
        coordinator: &fixture.coordinator,
    };
    let reconciled = reconcile_agent_lint_repair_after_recovery(
        &fixture.context,
        &interrupted,
        &recovered_services,
    )
    .unwrap()
    .unwrap();
    assert_eq!(reconciled.display_status, WorkflowDisplayStatus::Failed);
    assert!(matches!(
        reconciled.result,
        Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::Failed,
            checkpoint_hash: None,
            ..
        })
    ));
}

#[test]
fn deletion_waits_for_exact_second_confirmation_before_project_mutation() {
    let fixture = Fixture::new(true);
    let before_commits = git_commit_count(&fixture.context.root);
    let run = fixture.enqueue();
    let invocations = AtomicUsize::new(0);

    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            invocations.fetch_add(1, Ordering::SeqCst);
            fs::remove_file(workspace.join("wiki/concepts/page.md")).unwrap();
            Ok(delete_output(request))
        },
    );

    assert_eq!(invocations.load(Ordering::SeqCst), 1);
    assert!(fixture.context.root.join("wiki/concepts/page.md").is_file());
    assert_eq!(git_commit_count(&fixture.context.root), before_commits);
    let waiting = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(
        waiting.display_status,
        WorkflowDisplayStatus::WaitingForConfirmation
    );
    assert!(waiting.pending_action.is_some());

    confirm_agent_lint_repair_review_with_round_executor(
        &fixture.context,
        &run.task_id,
        &fixture.services(),
        "en",
        || Ok(()),
        |_, _, _| unreachable!("resolved deletion must not start another Agent round"),
    )
    .unwrap_or_else(|failure| panic!("confirmation failed: {}", failure.error.message));

    assert!(!fixture.context.root.join("wiki/concepts/page.md").exists());
    assert_eq!(git_commit_count(&fixture.context.root), before_commits);
    assert_eq!(
        fixture
            .task_service
            .get_workflow_run(&run.task_id)
            .unwrap()
            .display_status,
        WorkflowDisplayStatus::Completed
    );
}

#[test]
fn external_edit_after_review_is_never_overwritten_by_confirmation() {
    let fixture = Fixture::new(true);
    let run = fixture.enqueue();
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            fs::remove_file(workspace.join("wiki/concepts/page.md")).unwrap();
            Ok(delete_output(request))
        },
    );
    fs::write(
        fixture.context.root.join("wiki/concepts/page.md"),
        "---\ntitle: Page\ntype: concept\n---\n\n# Page\n\nExternal edit\n",
    )
    .unwrap();

    let failure = confirm_agent_lint_repair_review_with_round_executor(
        &fixture.context,
        &run.task_id,
        &fixture.services(),
        "en",
        || Ok(()),
        |_, _, _| unreachable!(),
    )
    .unwrap_err();

    assert!(
        matches!(
            failure.error.code.as_str(),
            "LINT_REPAIR_GIT_STATE_CHANGED"
                | "LINT_REPAIR_CANDIDATE_STALE"
                | "LINT_REPAIR_BATCH_STALE"
                | "LINT_REPAIR_ROLLBACK_FAILED"
        ),
        "{}: {}",
        failure.error.code,
        failure.error.message
    );
    assert_eq!(
        fs::read_to_string(fixture.context.root.join("wiki/concepts/page.md")).unwrap(),
        "---\ntitle: Page\ntype: concept\n---\n\n# Page\n\nExternal edit\n"
    );
}

#[test]
fn failure_after_an_applied_round_uses_the_durable_journal_to_restore_the_batch() {
    let fixture = Fixture::new(true);
    let original = fs::read_to_string(fixture.context.root.join("wiki/concepts/page.md")).unwrap();
    let before_commits = git_commit_count(&fixture.context.root);
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Possible duplicate topic",
    );
    let invocations = AtomicUsize::new(0);
    let authorizations = AtomicUsize::new(0);

    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || {
            if authorizations.fetch_add(1, Ordering::SeqCst) == 2 {
                return Err(BackendError::new(
                    "TEST_FINALIZE_FAILURE",
                    "Injected failure after the applied round.",
                    true,
                    true,
                ));
            }
            Ok(())
        },
        |request, workspace, _| {
            invocations.fetch_add(1, Ordering::SeqCst);
            fs::write(
                workspace.join("wiki/concepts/page.md"),
                "---\ntitle: Page\ntype: concept\n---\n\n# Page\n\nRound one edit\n",
            )
            .unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );

    assert_eq!(invocations.load(Ordering::SeqCst), 1);
    assert_eq!(
        fs::read_to_string(fixture.context.root.join("wiki/concepts/page.md"))
            .unwrap()
            .replace("\r\n", "\n"),
        original.replace("\r\n", "\n")
    );
    assert_eq!(git_commit_count(&fixture.context.root), before_commits);
    let failed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(failed.display_status, WorkflowDisplayStatus::Failed);
    assert!(matches!(
        failed.result,
        Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::RolledBack,
            ..
        })
    ));
    assert_eq!(
        failed.error.unwrap().project_mutation_state,
        llm_wiki_desktop_lib::models::workflow::WorkflowProjectMutationState::RolledBack
    );
    let options = WorkflowExecutionOptions {
        preparation_revision: "repair-preparation-revision".into(),
        operation: run.operation.clone(),
        ..WorkflowExecutionOptions::default()
    };
    let digest = agent_lint_repair_attestation_digest(&run, &options).unwrap();
    let receipt = fixture
        .settings_service
        .get_agent_lint_repair_attestation(
            &run.canonical_identity_key,
            &run.identity_revision,
            &run.task_id,
            &digest,
        )
        .unwrap();
    assert_eq!(
        receipt.lifecycle,
        AgentLintRepairAttestationLifecycle::Completed
    );
    assert!(receipt.mutation_journal.is_none());
}

#[test]
fn completed_receipt_recovers_a_crash_before_task_terminal_persistence() {
    let fixture = Fixture::new(true);
    let run = fixture.enqueue();
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            fs::write(
                workspace.join("wiki/concepts/page.md"),
                "---\ntitle: Page\ntype: concept\nsources:\n  - wiki/sources/source-a.md\n---\n\n# Page\n\nRecovered terminal\n\n> Sources: [[wiki/sources/source-a.md]]\n",
            )
            .unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );
    rewrite_persisted_run_as_running(&fixture, &run.task_id);

    let recovered_tasks = TaskService::default();
    recovered_tasks
        .recover_tasks(&fixture.context.root)
        .unwrap();
    let interrupted = recovered_tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(
        interrupted.display_status,
        WorkflowDisplayStatus::Interrupted
    );
    let recovered_services = AgentLintRepairExecutionServices {
        agent_service: &fixture.agent_service,
        lint_service: &fixture.lint_service,
        git_service: &fixture.git_service,
        file_store: &fixture.file_store,
        bookmark_service: &fixture.bookmark_service,
        search_service: &fixture.search_service,
        confirmation_registry: &fixture.confirmation_registry,
        settings_service: &fixture.settings_service,
        task_service: &recovered_tasks,
        coordinator: &fixture.coordinator,
    };
    let reconciled = reconcile_agent_lint_repair_after_recovery(
        &fixture.context,
        &interrupted,
        &recovered_services,
    )
    .unwrap()
    .unwrap();
    assert_eq!(reconciled.display_status, WorkflowDisplayStatus::Completed);
    assert!(matches!(
        reconciled.result,
        Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::Succeeded,
            ..
        })
    ));
    let options = WorkflowExecutionOptions {
        preparation_revision: "repair-preparation-revision".into(),
        operation: run.operation.clone(),
        ..WorkflowExecutionOptions::default()
    };
    let digest = agent_lint_repair_attestation_digest(&run, &options).unwrap();
    let receipt = fixture
        .settings_service
        .get_agent_lint_repair_attestation(
            &run.canonical_identity_key,
            &run.identity_revision,
            &run.task_id,
            &digest,
        )
        .unwrap();
    assert_eq!(
        receipt.lifecycle,
        AgentLintRepairAttestationLifecycle::Completed
    );
    assert!(receipt.terminal_result_digest.is_some());
    assert!(receipt.terminal_result_json.is_some());
}

#[test]
fn completed_success_receipt_wins_a_late_task_cancellation() {
    let fixture = Fixture::new(true);
    let run = fixture.enqueue();
    let options = WorkflowExecutionOptions {
        preparation_revision: "repair-preparation-revision".into(),
        operation: run.operation.clone(),
        ..WorkflowExecutionOptions::default()
    };
    let operation_digest = agent_lint_repair_attestation_digest(&run, &options).unwrap();
    let head = fixture
        .git_service
        .repository_status(&fixture.context)
        .unwrap()
        .head
        .unwrap();
    let result = WorkflowResult::AgentLintRepair {
        outcome: AgentLintRepairOutcome::Succeeded,
        resolved_finding_ids: vec!["missing_source:wiki/concepts/page.md".into()],
        unresolved_finding_ids: Vec::new(),
        introduced_finding_ids: Vec::new(),
        skipped_finding_ids: Vec::new(),
        rounds: Vec::new(),
        affected_paths: Vec::new(),
        affected_path_hashes: BTreeMap::new(),
        checkpoint_hash: Some(head.clone()),
        final_commit: Some(head),
        diff_available: false,
        rollback_available: false,
        index_refresh_warnings: Vec::new(),
    };
    fixture
        .settings_service
        .complete_agent_lint_repair_success_attestation(
            &run.task_id,
            &operation_digest,
            None,
            &agent_lint_repair_result_digest(&result).unwrap(),
            &serde_json::to_string(&result).unwrap(),
        )
        .unwrap();
    let cancelling = fixture
        .task_service
        .request_workflow_cancel(&run.task_id)
        .unwrap();
    assert_eq!(cancelling.display_status, WorkflowDisplayStatus::Running);

    let reconciled = reconcile_agent_lint_repair_after_recovery(
        &fixture.context,
        &fixture.task_service.get_workflow_run(&run.task_id).unwrap(),
        &fixture.services(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(reconciled.display_status, WorkflowDisplayStatus::Completed);
    assert!(matches!(
        reconciled.result,
        Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::Succeeded,
            ..
        })
    ));
}

#[test]
fn running_restart_before_first_apply_persists_a_typed_interrupted_result() {
    let fixture = Fixture::new(true);
    let run = fixture.enqueue();
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_agent_lint_repair_with_round_executor(
            &fixture.context,
            run.clone(),
            &fixture.services(),
            "en",
            || Ok(()),
            |_, _, _| panic!("simulated process crash before the first apply"),
        );
    }));
    assert!(crashed.is_err());

    let recovered_tasks = TaskService::default();
    recovered_tasks
        .recover_tasks(&fixture.context.root)
        .unwrap();
    let interrupted = recovered_tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(
        interrupted.display_status,
        WorkflowDisplayStatus::Interrupted
    );
    let recovered_services = AgentLintRepairExecutionServices {
        agent_service: &fixture.agent_service,
        lint_service: &fixture.lint_service,
        git_service: &fixture.git_service,
        file_store: &fixture.file_store,
        bookmark_service: &fixture.bookmark_service,
        search_service: &fixture.search_service,
        confirmation_registry: &fixture.confirmation_registry,
        settings_service: &fixture.settings_service,
        task_service: &recovered_tasks,
        coordinator: &fixture.coordinator,
    };
    let reconciled = reconcile_agent_lint_repair_after_recovery(
        &fixture.context,
        &interrupted,
        &recovered_services,
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        reconciled.result,
        Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::Interrupted,
            ..
        })
    ));
    let options = WorkflowExecutionOptions {
        preparation_revision: "repair-preparation-revision".into(),
        operation: run.operation.clone(),
        ..WorkflowExecutionOptions::default()
    };
    let digest = agent_lint_repair_attestation_digest(&run, &options).unwrap();
    let receipt = fixture
        .settings_service
        .get_agent_lint_repair_attestation(
            &run.canonical_identity_key,
            &run.identity_revision,
            &run.task_id,
            &digest,
        )
        .unwrap();
    assert_eq!(
        receipt.lifecycle,
        AgentLintRepairAttestationLifecycle::Completed
    );
}

#[test]
fn completed_noop_receipt_recovers_manual_result_before_task_finish() {
    let fixture = Fixture::new(true);
    fixture
        .file_store
        .write_json_atomic(
            &fixture.context,
            ".app/graph-cache.json",
            &serde_json::json!({ "status": "stale" }),
        )
        .unwrap();
    commit_path(
        &fixture.context.root,
        ".app/graph-cache.json",
        "Seed stale graph cache",
    );
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Possible duplicate topic",
    );
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            let content = if request.round == 3 {
                "---\ntitle: Page\ntype: concept\n---\n\n# Page\n\nOriginal\n".to_string()
            } else {
                format!(
                    "---\ntitle: Page\ntype: concept\n---\n\n# Page\n\nTemporary semantic edit {}\n",
                    request.round
                )
            };
            fs::write(workspace.join("wiki/concepts/page.md"), content).unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );
    let completed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    let completed_result = completed.result.clone();
    assert!(
        matches!(
            completed_result,
            Some(WorkflowResult::AgentLintRepair {
                outcome: AgentLintRepairOutcome::ManualReviewRequired,
                final_commit: Some(_),
                ..
            })
        ),
        "unexpected no-op result: {:?}",
        completed.result
    );
    rewrite_persisted_run_as_running(&fixture, &run.task_id);

    let recovered_tasks = TaskService::default();
    recovered_tasks
        .recover_tasks(&fixture.context.root)
        .unwrap();
    let interrupted = recovered_tasks.get_workflow_run(&run.task_id).unwrap();
    let recovered_services = AgentLintRepairExecutionServices {
        agent_service: &fixture.agent_service,
        lint_service: &fixture.lint_service,
        git_service: &fixture.git_service,
        file_store: &fixture.file_store,
        bookmark_service: &fixture.bookmark_service,
        search_service: &fixture.search_service,
        confirmation_registry: &fixture.confirmation_registry,
        settings_service: &fixture.settings_service,
        task_service: &recovered_tasks,
        coordinator: &fixture.coordinator,
    };
    let reconciled = reconcile_agent_lint_repair_after_recovery(
        &fixture.context,
        &interrupted,
        &recovered_services,
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        reconciled.result,
        Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::ManualReviewRequired,
            final_commit: Some(_),
            ..
        })
    ));
}

#[test]
fn descriptor_cas_mismatch_after_apply_still_consumes_the_durable_wal() {
    let fixture = Fixture::new(true);
    let original = fs::read_to_string(fixture.context.root.join("wiki/concepts/page.md")).unwrap();
    let before_commits = git_commit_count(&fixture.context.root);
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Possible duplicate topic",
    );
    let authorizations = AtomicUsize::new(0);
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || {
            if authorizations.fetch_add(1, Ordering::SeqCst) == 2 {
                let descriptor_path = std::env::temp_dir()
                    .join("llm-wiki-desktop")
                    .join(&run.task_id)
                    .join("agent-lint-repair-candidate.json");
                let mut descriptor: serde_json::Value =
                    serde_json::from_slice(&fs::read(&descriptor_path).unwrap()).unwrap();
                descriptor["indexRefreshWarnings"] =
                    serde_json::json!(["simulated CAS write split"]);
                fs::write(
                    descriptor_path,
                    serde_json::to_vec_pretty(&descriptor).unwrap(),
                )
                .unwrap();
                return Err(BackendError::new(
                    "TEST_AGENT_FAILURE",
                    "Injected failure after descriptor/receipt split.",
                    true,
                    true,
                ));
            }
            Ok(())
        },
        |request, workspace, _| {
            fs::write(
                workspace.join("wiki/concepts/page.md"),
                "---\ntitle: Page\ntype: concept\n---\n\n# Page\n\nRound one edit\n",
            )
            .unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );

    assert_eq!(
        fs::read_to_string(fixture.context.root.join("wiki/concepts/page.md"))
            .unwrap()
            .replace("\r\n", "\n"),
        original.replace("\r\n", "\n")
    );
    assert_eq!(git_commit_count(&fixture.context.root), before_commits);
    let failed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert!(matches!(
        failed.result,
        Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::RolledBack,
            ..
        })
    ));
}

#[test]
fn project_owned_candidate_tamper_never_rebinds_the_exact_review() {
    let fixture = Fixture::new(true);
    let run = fixture.enqueue();
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            fs::remove_file(workspace.join("wiki/concepts/page.md")).unwrap();
            Ok(delete_output(request))
        },
    );
    let before = fs::read_to_string(fixture.context.root.join("wiki/concepts/page.md")).unwrap();
    let descriptor_path = std::env::temp_dir()
        .join("llm-wiki-desktop")
        .join(&run.task_id)
        .join("agent-lint-repair-candidate.json");
    let mut descriptor: serde_json::Value =
        serde_json::from_slice(&fs::read(&descriptor_path).unwrap()).unwrap();
    descriptor["pendingRound"]["output"]["summary"] =
        serde_json::json!("tampered project-owned summary");
    fs::write(
        &descriptor_path,
        serde_json::to_vec_pretty(&descriptor).unwrap(),
    )
    .unwrap();

    let failure = confirm_agent_lint_repair_review_with_round_executor(
        &fixture.context,
        &run.task_id,
        &fixture.services(),
        "en",
        || Ok(()),
        |_, _, _| panic!("tampered review must not invoke another Agent round"),
    )
    .err()
    .expect("tampered descriptor must fail closed");
    assert!(matches!(
        failure.error.code.as_str(),
        "LINT_REPAIR_CANDIDATE_STALE" | "LINT_REPAIR_ATTESTATION_STATE_INVALID"
    ));
    assert_eq!(
        fs::read_to_string(fixture.context.root.join("wiki/concepts/page.md")).unwrap(),
        before
    );
}

#[test]
fn first_round_skipped_or_needs_review_without_diff_finishes_without_empty_wal() {
    for status in [
        AgentLintRepairFindingStatus::Skipped,
        AgentLintRepairFindingStatus::NeedsReview,
    ] {
        let fixture = Fixture::new(true);
        let path = fixture.context.root.join("wiki/concepts/page.md");
        let original = fs::read(&path).unwrap();
        let run = fixture.enqueue_finding(
            "duplicate_topic:wiki/concepts/page.md",
            DeepLintIssueType::DuplicateTopic,
            "Needs judgment",
        );
        let invocations = AtomicUsize::new(0);
        run_agent_lint_repair_with_round_executor(
            &fixture.context,
            run.clone(),
            &fixture.services(),
            "en",
            || Ok(()),
            |request, _, _| {
                invocations.fetch_add(1, Ordering::SeqCst);
                Ok(output_with_changes(request, status, Vec::new()))
            },
        );
        assert_eq!(invocations.load(Ordering::SeqCst), 1);
        assert_eq!(fs::read(&path).unwrap(), original);
        let completed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(
            completed.display_status,
            WorkflowDisplayStatus::Completed,
            "{:?}",
            completed.error
        );
        assert!(
            matches!(completed.result, Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::ManualReviewRequired,
            ref affected_paths,
            ..
        }) if affected_paths.is_empty())
        );
    }
}

#[test]
fn applied_round_then_empty_round_or_net_zero_result_keeps_history() {
    for return_to_original in [false, true] {
        let fixture = Fixture::new(true);
        let page_path = fixture.context.root.join("wiki/concepts/page.md");
        let original = fs::read(&page_path).unwrap();
        let other = "wiki/concepts/other.md";
        fs::write(
            fixture.context.root.join(other),
            "---\ntitle: Other\ntype: concept\n---\n\n# Other\n\nNo source yet\n",
        )
        .unwrap();
        let run = fixture.enqueue_findings(vec![
            AgentLintRepairFinding {
                id: "missing_source:wiki/concepts/page.md".into(),
                issue_type: DeepLintIssueType::MissingSource,
                severity: LintSeverity::Warning,
                path: "wiki/concepts/page.md".into(),
                message: "First missing source".into(),
                evidence: None,
                suggested_action: None,
            },
            AgentLintRepairFinding {
                id: format!("missing_source:{other}"),
                issue_type: DeepLintIssueType::MissingSource,
                severity: LintSeverity::Warning,
                path: other.into(),
                message: "Second missing source".into(),
                evidence: None,
                suggested_action: None,
            },
        ]);
        let calls = AtomicUsize::new(0);
        run_agent_lint_repair_with_round_executor(
            &fixture.context,
            run.clone(),
            &fixture.services(),
            "en",
            || Ok(()),
            |request, workspace, _| {
                calls.fetch_add(1, Ordering::SeqCst);
                if request.round == 1 {
                    fs::write(workspace.join("wiki/concepts/page.md"),
                        "---\ntitle: Page\ntype: concept\nsources:\n  - wiki/sources/source-a.md\n---\n\n# Page\n\nCited first page\n\n> Sources: [[wiki/sources/source-a.md]]\n").unwrap();
                    Ok(output_with_changes(
                        request,
                        AgentLintRepairFindingStatus::Attempted,
                        vec![AgentLintRepairDeclaredChange {
                            path: "wiki/concepts/page.md".into(),
                            operation: AgentLintRepairDeclaredChangeOperation::Update,
                        }],
                    ))
                } else if return_to_original {
                    fs::write(workspace.join("wiki/concepts/page.md"), &original).unwrap();
                    Ok(output_with_changes(
                        request,
                        AgentLintRepairFindingStatus::Skipped,
                        vec![AgentLintRepairDeclaredChange {
                            path: "wiki/concepts/page.md".into(),
                            operation: AgentLintRepairDeclaredChangeOperation::Update,
                        }],
                    ))
                } else {
                    Ok(output_with_changes(
                        request,
                        AgentLintRepairFindingStatus::NeedsReview,
                        vec![],
                    ))
                }
            },
        );
        let completed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(
            completed.display_status,
            WorkflowDisplayStatus::Completed,
            "{:?}",
            completed.error
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(
            matches!(completed.result, Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::PartiallyCompleted | AgentLintRepairOutcome::ManualReviewRequired,
            ref rounds,
            ..
        }) if rounds.len() == 2),
            "{:?}",
            completed.result
        );
        assert!(fixture
            .git_service
            .history_snapshot(&fixture.context, &run.task_id, "after")
            .unwrap()
            .is_some());
        if return_to_original {
            assert_eq!(fs::read(&page_path).unwrap(), original);
        }
    }
}

#[test]
fn additional_existing_page_is_restored_byte_for_byte_after_confirmed_apply_fails() {
    let fixture = Fixture::new(true);
    let extra = "wiki/concepts/额外 页面.md";
    let original = b"---\r\ntitle: Existing\r\ntype: concept\r\n---\r\n\r\n# Existing\r\n\r\nUncommitted original\r\n";
    fs::write(fixture.context.root.join(extra), original).unwrap();
    let before_head = fixture
        .git_service
        .repository_status(&fixture.context)
        .unwrap()
        .head;
    let before_index = fs::read(fixture.context.root.join(".git/index")).unwrap();
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Needs judgment",
    );
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            fs::write(
                workspace.join(extra),
                "---\ntitle: Existing\ntype: concept\n---\n\n# Changed\n",
            )
            .unwrap();
            Ok(output_with_changes(
                request,
                AgentLintRepairFindingStatus::Attempted,
                vec![AgentLintRepairDeclaredChange {
                    path: extra.into(),
                    operation: AgentLintRepairDeclaredChangeOperation::Update,
                }],
            ))
        },
    );
    assert_eq!(
        fixture
            .task_service
            .get_workflow_run(&run.task_id)
            .unwrap()
            .display_status,
        WorkflowDisplayStatus::WaitingForConfirmation
    );
    let authorizations = AtomicUsize::new(0);
    let failure = confirm_agent_lint_repair_review_with_round_executor(
        &fixture.context,
        &run.task_id,
        &fixture.services(),
        "en",
        || {
            if authorizations.fetch_add(1, Ordering::SeqCst) == 1 {
                return Err(BackendError::new(
                    "TEST_FINALIZE_FAILURE",
                    "after confirmed apply",
                    true,
                    true,
                ));
            }
            Ok(())
        },
        |_, _, _| unreachable!("no progress should retry"),
    )
    .unwrap_err();
    assert_eq!(failure.error.code, "TEST_FINALIZE_FAILURE");
    assert_eq!(
        fs::read(fixture.context.root.join(extra)).unwrap(),
        original
    );
    assert_eq!(
        fixture
            .git_service
            .repository_status(&fixture.context)
            .unwrap()
            .head,
        before_head
    );
    assert_eq!(
        fs::read(fixture.context.root.join(".git/index")).unwrap(),
        before_index
    );
    let terminal = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(terminal.display_status, WorkflowDisplayStatus::Failed);
    assert!(matches!(
        terminal.result,
        Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::RolledBack,
            ..
        })
    ));
}

#[test]
fn restart_restores_second_confirmed_existing_page_from_persisted_before_state() {
    let fixture = Fixture::new(true);
    let extra = "wiki/concepts/重启 页面.md";
    let original =
        b"---\r\ntitle: Restart\r\ntype: concept\r\n---\r\n\r\n# Restart\r\n\r\nOriginal bytes\r\n";
    fs::write(fixture.context.root.join(extra), original).unwrap();
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Needs judgment",
    );
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            fs::write(
                workspace.join(extra),
                "---\ntitle: Restart\ntype: concept\n---\n\n# Changed\n",
            )
            .unwrap();
            Ok(output_with_changes(
                request,
                AgentLintRepairFindingStatus::Attempted,
                vec![AgentLintRepairDeclaredChange {
                    path: extra.into(),
                    operation: AgentLintRepairDeclaredChangeOperation::Update,
                }],
            ))
        },
    );
    let authorizations = AtomicUsize::new(0);
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = confirm_agent_lint_repair_review_with_round_executor(
            &fixture.context,
            &run.task_id,
            &fixture.services(),
            "en",
            || {
                if authorizations.fetch_add(1, Ordering::SeqCst) == 1 {
                    panic!("simulated process termination after confirmed write");
                }
                Ok(())
            },
            |_, _, _| unreachable!(),
        );
    }));
    assert!(crashed.is_err());
    assert_ne!(
        fs::read(fixture.context.root.join(extra)).unwrap(),
        original
    );
    let recovered_tasks = TaskService::default();
    recovered_tasks
        .recover_tasks(&fixture.context.root)
        .unwrap();
    let interrupted = recovered_tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(
        interrupted.display_status,
        WorkflowDisplayStatus::Interrupted
    );
    let recovered_services = AgentLintRepairExecutionServices {
        agent_service: &fixture.agent_service,
        lint_service: &fixture.lint_service,
        git_service: &fixture.git_service,
        file_store: &fixture.file_store,
        bookmark_service: &fixture.bookmark_service,
        search_service: &fixture.search_service,
        confirmation_registry: &fixture.confirmation_registry,
        settings_service: &fixture.settings_service,
        task_service: &recovered_tasks,
        coordinator: &fixture.coordinator,
    };
    let settled = reconcile_agent_lint_repair_after_recovery(
        &fixture.context,
        &interrupted,
        &recovered_services,
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        fs::read(fixture.context.root.join(extra)).unwrap(),
        original
    );
    assert!(matches!(
        settled.result,
        Some(WorkflowResult::AgentLintRepair {
            outcome: AgentLintRepairOutcome::RolledBack,
            ..
        })
    ));
}

#[test]
fn completed_second_confirmed_page_has_exact_undo_bytes() {
    let fixture = Fixture::new(true);
    let extra = "wiki/concepts/撤销 页面.md";
    let original =
        b"---\r\ntitle: Undo\r\ntype: concept\r\n---\r\n\r\n# Undo\r\n\r\nLocal draft\r\n";
    fs::write(fixture.context.root.join(extra), original).unwrap();
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Needs judgment",
    );
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            fs::write(
                workspace.join(extra),
                "---\ntitle: Undo\ntype: concept\n---\n\n# Rewritten\n",
            )
            .unwrap();
            Ok(output_with_changes(
                request,
                AgentLintRepairFindingStatus::NeedsReview,
                vec![AgentLintRepairDeclaredChange {
                    path: extra.into(),
                    operation: AgentLintRepairDeclaredChangeOperation::Update,
                }],
            ))
        },
    );
    confirm_agent_lint_repair_review_with_round_executor(
        &fixture.context,
        &run.task_id,
        &fixture.services(),
        "en",
        || Ok(()),
        |_, _, _| unreachable!(),
    )
    .unwrap_or_else(|failure| panic!("confirmation failed: {}", failure.error.message));
    let completed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(completed.display_status, WorkflowDisplayStatus::Completed);
    let WorkflowResult::AgentLintRepair {
        checkpoint_hash: Some(before_hash),
        final_commit: Some(after_hash),
        ..
    } = completed.result.unwrap()
    else {
        panic!("missing history")
    };
    let paths = vec![extra.to_string()];
    let before = fixture
        .git_service
        .read_history_files(&fixture.context, &before_hash, &paths)
        .unwrap();
    let after = fixture
        .git_service
        .read_history_files(&fixture.context, &after_hash, &paths)
        .unwrap();
    assert_eq!(before[extra], Some(original.to_vec()));
    assert_eq!(
        after[extra],
        Some(fs::read(fixture.context.root.join(extra)).unwrap())
    );
    let receipt = fixture
        .settings_service
        .get_agent_lint_repair_attestation(
            &run.canonical_identity_key,
            &run.identity_revision,
            &run.task_id,
            &agent_lint_repair_attestation_digest(
                &run,
                &WorkflowExecutionOptions {
                    preparation_revision: "repair-preparation-revision".into(),
                    operation: run.operation.clone(),
                    ..WorkflowExecutionOptions::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
    assert!(receipt.original_path_hashes[extra].is_some());
    let restore = llm_wiki_desktop_lib::services::CompileService::prepare_history_restore(
        &fixture.context,
        &before,
        &after,
    )
    .unwrap();
    llm_wiki_desktop_lib::services::CompileService::restore_prepared_history_outputs(
        &fixture.context,
        &restore,
    )
    .unwrap();
    assert_eq!(
        fs::read(fixture.context.root.join(extra)).unwrap(),
        original
    );
}

#[test]
fn safe_new_page_is_in_final_history_and_preserved_for_manual_review() {
    let fixture = Fixture::new(true);
    let new_page = "wiki/concepts/新 页面.md";
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Needs judgment",
    );
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || Ok(()),
        |request, workspace, _| {
            fs::write(
                workspace.join(new_page),
                "---\ntitle: New\ntype: concept\n---\n\n# New\n\nSynthetic page\n",
            )
            .unwrap();
            Ok(output_with_changes(
                request,
                AgentLintRepairFindingStatus::NeedsReview,
                vec![AgentLintRepairDeclaredChange {
                    path: new_page.into(),
                    operation: AgentLintRepairDeclaredChangeOperation::Create,
                }],
            ))
        },
    );
    if fixture
        .task_service
        .get_workflow_run(&run.task_id)
        .unwrap()
        .display_status
        == WorkflowDisplayStatus::WaitingForConfirmation
    {
        confirm_agent_lint_repair_review_with_round_executor(
            &fixture.context,
            &run.task_id,
            &fixture.services(),
            "en",
            || Ok(()),
            |_, _, _| unreachable!("no unverifiable retry"),
        )
        .unwrap_or_else(|failure| panic!("confirmation failed: {}", failure.error.message));
    }
    let completed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(
        completed.display_status,
        WorkflowDisplayStatus::Completed,
        "{:?}",
        completed.error
    );
    assert!(
        matches!(completed.result, Some(WorkflowResult::AgentLintRepair {
        outcome: AgentLintRepairOutcome::ManualReviewRequired,
        ref affected_paths,
        ..
    }) if affected_paths.contains(&new_page.to_string()))
    );
    let before_hash = fixture
        .git_service
        .history_snapshot(&fixture.context, &run.task_id, "before")
        .unwrap()
        .unwrap();
    let after_hash = fixture
        .git_service
        .history_snapshot(&fixture.context, &run.task_id, "after")
        .unwrap()
        .unwrap();
    assert_eq!(
        fixture
            .git_service
            .read_history_files(&fixture.context, &before_hash, &[new_page.into()])
            .unwrap()[new_page],
        None
    );
    assert_eq!(
        fixture
            .git_service
            .read_history_files(&fixture.context, &after_hash, &[new_page.into()])
            .unwrap()[new_page],
        Some(fs::read(fixture.context.root.join(new_page)).unwrap())
    );
}

#[test]
fn recovery_conflict_preserves_external_bytes_and_finishes_failed_task() {
    let fixture = Fixture::new(true);
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Needs judgment",
    );
    let authorizations = AtomicUsize::new(0);
    let external = b"---\ntitle: External\ntype: concept\n---\n\n# External edit\n";
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || {
            if authorizations.fetch_add(1, Ordering::SeqCst) == 2 {
                fs::write(fixture.context.root.join("wiki/concepts/page.md"), external).unwrap();
                return Err(BackendError::new(
                    "TEST_FINALIZE_FAILURE",
                    "external edit after apply",
                    true,
                    true,
                ));
            }
            Ok(())
        },
        |request, workspace, _| {
            fs::write(
                workspace.join("wiki/concepts/page.md"),
                "---\ntitle: Page\ntype: concept\n---\n\n# Agent edit\n",
            )
            .unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );
    assert_eq!(
        fs::read(fixture.context.root.join("wiki/concepts/page.md")).unwrap(),
        external
    );
    let failed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(failed.display_status, WorkflowDisplayStatus::Failed);
    assert_eq!(failed.error.unwrap().code, "LINT_REPAIR_ROLLBACK_FAILED");
}

#[test]
fn missing_private_before_ref_preserves_applied_bytes_for_manual_recovery() {
    let fixture = Fixture::new(true);
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Needs judgment",
    );
    let authorizations = AtomicUsize::new(0);
    let changed = b"---\ntitle: Page\ntype: concept\n---\n\n# Applied edit\n";
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || {
            if authorizations.fetch_add(1, Ordering::SeqCst) == 2 {
                let reference = format!("refs/llm-wiki/operations/{}/before", run.task_id);
                assert!(std::process::Command::new("git")
                    .args(["update-ref", "-d", &reference])
                    .current_dir(&fixture.context.root)
                    .status()
                    .unwrap()
                    .success());
                return Err(BackendError::new(
                    "TEST_FINALIZE_FAILURE",
                    "lost private before ref",
                    true,
                    true,
                ));
            }
            Ok(())
        },
        |request, workspace, _| {
            fs::write(workspace.join("wiki/concepts/page.md"), changed).unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );
    assert_eq!(
        fs::read(fixture.context.root.join("wiki/concepts/page.md")).unwrap(),
        changed
    );
    let failed = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(failed.display_status, WorkflowDisplayStatus::Failed);
    assert_eq!(failed.error.unwrap().code, "LINT_REPAIR_ROLLBACK_FAILED");
}

#[test]
fn cancellation_after_apply_restores_original_bytes_and_ends_task() {
    let fixture = Fixture::new(true);
    let path = fixture.context.root.join("wiki/concepts/page.md");
    let original = fs::read(&path).unwrap();
    let run = fixture.enqueue_finding(
        "duplicate_topic:wiki/concepts/page.md",
        DeepLintIssueType::DuplicateTopic,
        "Needs judgment",
    );
    let authorizations = AtomicUsize::new(0);
    run_agent_lint_repair_with_round_executor(
        &fixture.context,
        run.clone(),
        &fixture.services(),
        "en",
        || {
            if authorizations.fetch_add(1, Ordering::SeqCst) == 2 {
                fixture.task_service.request_cancel(&run.task_id).unwrap();
                return Err(BackendError::new(
                    "AGENT_CANCELLED",
                    "cancel at finalization",
                    true,
                    false,
                ));
            }
            Ok(())
        },
        |request, workspace, _| {
            fs::write(
                workspace.join("wiki/concepts/page.md"),
                "---\ntitle: Page\ntype: concept\n---\n\n# Temporary agent edit\n",
            )
            .unwrap();
            Ok(output(request, AgentLintRepairFindingStatus::Attempted))
        },
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    let cancelled = fixture.task_service.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(cancelled.display_status, WorkflowDisplayStatus::Cancelled);
    assert!(
        matches!(
            cancelled.result,
            Some(WorkflowResult::AgentLintRepair {
                outcome: AgentLintRepairOutcome::RolledBack,
                ..
            })
        ),
        "{:?}",
        cancelled.result
    );
}
