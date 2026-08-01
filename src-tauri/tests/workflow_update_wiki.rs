use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::agent::AgentKind;
use llm_wiki_desktop_lib::models::compile::{
    CompileAction, CompileCandidate, CompileConsumptionRecord, CompileFile, CompileManifest,
    CompilePageType, CompilePlan, CompilePlanItem, CompileRoute, ResolvedCompileRoute,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::workflow::{
    UpdateWikiMode, WorkflowDisplayStatus, WorkflowExecutionOptions, WorkflowKind, WorkflowRoute,
    WorkflowScope, WorkflowSourceVersionRef, WorkflowStageStatus,
};
use llm_wiki_desktop_lib::services::{
    persist_update_wiki_review, run_update_wiki, update_wiki_candidate_is_valid,
    workflow_baseline_for_scope, workflow_stages, AgentInvocation, AgentService, BookmarkService,
    CompileExecutionServices, CompileService, EnqueueWorkflow, FileStore, GitService, LlmService,
    ProcessRunner, SearchService, SecretService, SettingsService, UpdateWikiExecutionServices,
    WorkflowCoordinator, WorkflowStageSink,
};
use llm_wiki_desktop_lib::tasks::TaskService;

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
        Ok((String::new(), String::new()))
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        unreachable!("missing agents must fail before invocation")
    }
}

struct SuccessfulAgent {
    delete_path: Option<String>,
}

impl ProcessRunner for SuccessfulAgent {
    fn find_executable(&self, _command: &str) -> Option<PathBuf> {
        Some(PathBuf::from("fake-agent"))
    }

    fn run_with_timeout(
        &self,
        _command: &str,
        _args: &[&str],
        _timeout: Duration,
    ) -> Result<String, BackendError> {
        Ok("--print --output-format --verbose --permission-mode --settings --bare --safe-mode --disable-slash-commands --no-session-persistence --no-chrome --prompt-suggestions --strict-mcp-config --tools --allowedTools --json-schema".into())
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        Ok((String::new(), String::new()))
    }

    fn run_task_streaming(
        &self,
        invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        let target = "wiki/concepts/工作流成功.md";
        fs::write(
            invocation.cwd.join("compile-plan.json"),
            serde_json::to_vec_pretty(&CompilePlan {
                summary: "update wiki".into(),
                items: vec![CompilePlanItem {
                    action: CompileAction::Create,
                    target_path: target.into(),
                    page_type: CompilePageType::Concept,
                    source_ids: vec!["资料.md".into()],
                    affected_existing_pages: self.delete_path.clone().into_iter().collect(),
                    reason: "new evidence".into(),
                    risk_flags: self
                        .delete_path
                        .as_ref()
                        .map(|_| vec!["rename".into()])
                        .unwrap_or_default(),
                }],
                global_risk_flags: Vec::new(),
            })
            .unwrap(),
        )
        .unwrap();
        fs::write(
            invocation.cwd.join("wiki/index.md"),
            "# Index\n- [[concepts/工作流成功]]\n",
        )
        .unwrap();
        fs::write(
            invocation.cwd.join("wiki/overview.md"),
            "# Overview\nUpdated\n",
        )
        .unwrap();
        fs::write(invocation.cwd.join("wiki/log.md"), "# Log\n- Updated\n").unwrap();
        let target_path = invocation.cwd.join(target);
        fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        fs::write(
            target_path,
            "---\ntype: concept\nsources:\n  - 资料.md\n---\n# 工作流成功\n\n> Sources: [资料](../sources/资料.md)\n",
        )
        .unwrap();
        if let Some(path) = &self.delete_path {
            fs::remove_file(invocation.cwd.join(path)).unwrap();
        }
        Ok("completed".into())
    }
}

fn project(label: &str) -> (ProjectContext, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "workflow-update-wiki-{label}-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(root.join(".app/tasks")).unwrap();
    fs::create_dir_all(root.join(".app/compile")).unwrap();
    fs::create_dir_all(root.join("raw/extracted")).unwrap();
    fs::create_dir_all(root.join("wiki/concepts")).unwrap();
    fs::write(root.join("purpose.md"), "# Purpose\n").unwrap();
    fs::write(root.join("schema.md"), "# Schema\n").unwrap();
    fs::write(root.join("wiki/index.md"), "# Index\n").unwrap();
    fs::write(root.join("wiki/overview.md"), "# Overview\n").unwrap();
    fs::write(root.join("wiki/log.md"), "# Log\n").unwrap();
    fs::write(root.join("raw/extracted/资料.md"), "# 资料\n").unwrap();
    fs::write(
        root.join(".app/source-index.json"),
        r#"{"sources":{"raw/sources/资料.txt":["raw/extracted/资料.md"]}}"#,
    )
    .unwrap();
    let context = ProjectContext::new(format!("project-{label}"), root.clone());
    (context, root)
}

fn enqueue_update(
    context: &ProjectContext,
    tasks: &TaskService,
    coordinator: &WorkflowCoordinator,
    mode: UpdateWikiMode,
    source: WorkflowSourceVersionRef,
) -> llm_wiki_desktop_lib::models::workflow::WorkflowRun {
    let scope = WorkflowScope::UpdateWiki {
        mode,
        source_versions: vec![source],
    };
    let baseline = workflow_baseline_for_scope(context, &scope).unwrap();
    let outcome = coordinator
        .enqueue(
            tasks,
            EnqueueWorkflow {
                project_id: context.project_id.clone(),
                project_root: context.root.clone(),
                task_state_root: None,
                title: "Update Wiki".into(),
                kind: WorkflowKind::UpdateWiki,
                scope,
                route: Some(WorkflowRoute::Agent {
                    agent: AgentKind::Claude,
                    model: None,
                    route_revision: "route-test".into(),
                }),
                baseline_fingerprint: baseline.fingerprint,
                execution_options: WorkflowExecutionOptions {
                    preparation_revision: "preparation-test".into(),
                    ..WorkflowExecutionOptions::default()
                },
                stages: workflow_stages(&WorkflowKind::UpdateWiki),
                retry: None,
            },
        )
        .unwrap();
    match outcome {
        llm_wiki_desktop_lib::models::workflow::WorkflowStartOutcome::Created { run } => run,
        _ => panic!("first enqueue must create a run"),
    }
}

fn source_scope(
    context: &ProjectContext,
) -> (
    WorkflowSourceVersionRef,
    llm_wiki_desktop_lib::models::compile::SourceVersionRef,
) {
    let source = CompileService::list_source_versions(context)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    (
        WorkflowSourceVersionRef {
            source_id: source.source_id.clone(),
            version_id: source.version_id.clone(),
        },
        source,
    )
}

#[tokio::test]
async fn changed_sources_passes_only_unconsumed_pairs_and_keeps_nine_explicit_stages() {
    let (context, root) = project("consumed");
    GitService
        .initialize_repository(&context, "initial")
        .unwrap();
    let (scope, source) = source_scope(&context);
    fs::write(
        context.app_dir.join("compile/previous.json"),
        serde_json::to_vec_pretty(&CompileConsumptionRecord {
            schema_version: 1,
            compile_task_id: "previous".into(),
            route: CompileRoute::Byok,
            consumed_at: "2026-08-01T00:00:00Z".into(),
            source_versions: vec![source],
            affected_paths: Vec::new(),
            checkpoint: None,
        })
        .unwrap(),
    )
    .unwrap();
    GitService
        .create_checkpoint(
            &context,
            llm_wiki_desktop_lib::models::git::CheckpointPurpose::FinalResult,
            "record previous compile",
        )
        .unwrap();
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let run = enqueue_update(
        &context,
        &tasks,
        &coordinator,
        UpdateWikiMode::ChangedSources,
        scope,
    );
    let agents = AgentService::with_runner(Arc::new(NoAgents));
    let llm = LlmService;
    let secrets = SecretService::default();
    let settings = SettingsService::default();
    let file_store = FileStore;
    let bookmarks = BookmarkService::default();
    let search = SearchService::default();
    let confirmations = Default::default();
    let services = UpdateWikiExecutionServices {
        compile: CompileExecutionServices {
            agent_service: &agents,
            llm_service: &llm,
            secret_service: &secrets,
            settings_service: &settings,
            task_service: &tasks,
        },
        git_service: &GitService,
        file_store: &file_store,
        bookmark_service: &bookmarks,
        search_service: &search,
        confirmation_registry: &confirmations,
        coordinator: &coordinator,
    };

    assert!(run_update_wiki(&context, run.clone(), &services)
        .await
        .is_none());
    let completed = tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(completed.display_status, WorkflowDisplayStatus::Completed);
    assert_eq!(completed.stages.len(), 9);
    assert_eq!(completed.stages[0].status, WorkflowStageStatus::Completed);
    assert!(completed.stages[1..8]
        .iter()
        .all(|stage| stage.status == WorkflowStageStatus::Skipped));
    assert_eq!(completed.stages[8].status, WorkflowStageStatus::Completed);
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn full_recompile_is_explicit_and_reaches_checkpoint_and_generation_for_consumed_source() {
    let (context, root) = project("full");
    GitService
        .initialize_repository(&context, "initial")
        .unwrap();
    let (scope, source) = source_scope(&context);
    fs::write(
        context.app_dir.join("compile/previous.json"),
        serde_json::to_vec_pretty(&CompileConsumptionRecord {
            schema_version: 1,
            compile_task_id: "previous".into(),
            route: CompileRoute::Byok,
            consumed_at: "2026-08-01T00:00:00Z".into(),
            source_versions: vec![source],
            affected_paths: Vec::new(),
            checkpoint: None,
        })
        .unwrap(),
    )
    .unwrap();
    GitService
        .create_checkpoint(
            &context,
            llm_wiki_desktop_lib::models::git::CheckpointPurpose::FinalResult,
            "record previous compile",
        )
        .unwrap();
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let run = enqueue_update(
        &context,
        &tasks,
        &coordinator,
        UpdateWikiMode::FullRecompile,
        scope,
    );
    let agents = AgentService::with_runner(Arc::new(NoAgents));
    let llm = LlmService;
    let secrets = SecretService::default();
    let settings = SettingsService::default();
    let file_store = FileStore;
    let bookmarks = BookmarkService::default();
    let search = SearchService::default();
    let confirmations = Default::default();
    let services = UpdateWikiExecutionServices {
        compile: CompileExecutionServices {
            agent_service: &agents,
            llm_service: &llm,
            secret_service: &secrets,
            settings_service: &settings,
            task_service: &tasks,
        },
        git_service: &GitService,
        file_store: &file_store,
        bookmark_service: &bookmarks,
        search_service: &search,
        confirmation_registry: &confirmations,
        coordinator: &coordinator,
    };

    run_update_wiki(&context, run.clone(), &services).await;
    let failed = tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(failed.display_status, WorkflowDisplayStatus::Failed);
    assert_eq!(failed.stages[0].status, WorkflowStageStatus::Completed);
    assert_eq!(failed.stages[1].status, WorkflowStageStatus::Completed);
    assert_eq!(failed.stages[2].status, WorkflowStageStatus::Failed);
    assert!(GitService
        .repository_status(&context)
        .unwrap()
        .head
        .is_some());
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn required_checkpoint_failure_leaves_formal_wiki_unchanged() {
    let (context, root) = project("checkpoint-failure");
    let before = CompileService::snapshot_wiki(&context).unwrap();
    let (scope, _) = source_scope(&context);
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let run = enqueue_update(
        &context,
        &tasks,
        &coordinator,
        UpdateWikiMode::ChangedSources,
        scope,
    );
    let agents = AgentService::with_runner(Arc::new(NoAgents));
    let llm = LlmService;
    let secrets = SecretService::default();
    let settings = SettingsService::default();
    let file_store = FileStore;
    let bookmarks = BookmarkService::default();
    let search = SearchService::default();
    let confirmations = Default::default();
    let services = UpdateWikiExecutionServices {
        compile: CompileExecutionServices {
            agent_service: &agents,
            llm_service: &llm,
            secret_service: &secrets,
            settings_service: &settings,
            task_service: &tasks,
        },
        git_service: &GitService,
        file_store: &file_store,
        bookmark_service: &bookmarks,
        search_service: &search,
        confirmation_registry: &confirmations,
        coordinator: &coordinator,
    };

    run_update_wiki(&context, run.clone(), &services).await;
    let failed = tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(failed.display_status, WorkflowDisplayStatus::Failed);
    assert_eq!(failed.stages[0].status, WorkflowStageStatus::Completed);
    assert_eq!(failed.stages[1].status, WorkflowStageStatus::Failed);
    assert_eq!(CompileService::snapshot_wiki(&context).unwrap(), before);
    assert!(!std::env::temp_dir()
        .join("llm-wiki-desktop")
        .join(&run.task_id)
        .exists());
    fs::remove_dir_all(root).ok();
}

#[test]
fn risk_classification_covers_safe_update_delete_broad_rewrite_external_edit_and_case_collision() {
    let (context, root) = project("risk-中文");
    fs::write(context.wiki_dir.join("concepts/Agent.md"), "# Original\n").unwrap();
    let baseline = CompileService::snapshot_wiki(&context).unwrap();
    let plan = CompilePlan {
        summary: "update".into(),
        items: vec![CompilePlanItem {
            action: CompileAction::Update,
            target_path: "wiki/concepts/Agent.md".into(),
            page_type: CompilePageType::Concept,
            source_ids: vec!["资料.md".into()],
            affected_existing_pages: vec!["wiki/concepts/Agent.md".into()],
            reason: "new evidence".into(),
            risk_flags: Vec::new(),
        }],
        global_risk_flags: Vec::new(),
    };
    let manifest = CompileManifest {
        files: vec![
            CompileFile::new("wiki/index.md", "# Index\n"),
            CompileFile::new("wiki/overview.md", "# Overview\n"),
            CompileFile::new("wiki/log.md", "# Log\n"),
            CompileFile::new("wiki/concepts/Agent.md", "# Updated\n"),
        ],
        deletions: Vec::new(),
        summary: "update".into(),
    };
    let safe =
        CompileService::classify_workflow_changes(&context, &manifest, &plan, &baseline, false)
            .unwrap();
    assert!(safe.updated.contains(&"wiki/concepts/Agent.md".into()));
    assert!(!safe.requires_confirmation());

    let mut overwrite_plan = plan.clone();
    overwrite_plan.items[0].action = CompileAction::Create;
    let overwrite = CompileService::classify_workflow_changes(
        &context,
        &manifest,
        &overwrite_plan,
        &baseline,
        false,
    )
    .unwrap();
    assert!(overwrite.requires_confirmation());

    let broad =
        CompileService::classify_workflow_changes(&context, &manifest, &plan, &baseline, true)
            .unwrap();
    assert!(broad.requires_confirmation());

    let mut deletion = manifest.clone();
    deletion.deletions.push("wiki/concepts/Agent.md".into());
    deletion
        .files
        .retain(|file| file.path != "wiki/concepts/Agent.md");
    let deleted =
        CompileService::classify_workflow_changes(&context, &deletion, &plan, &baseline, false)
            .unwrap();
    assert!(deleted.deleted.contains(&"wiki/concepts/Agent.md".into()));
    assert!(deleted.requires_confirmation());

    fs::write(
        context.wiki_dir.join("concepts/Agent.md"),
        "# External edit\n",
    )
    .unwrap();
    let conflict =
        CompileService::classify_workflow_changes(&context, &manifest, &plan, &baseline, false)
            .unwrap();
    assert!(conflict
        .conflicted
        .contains(&"wiki/concepts/Agent.md".into()));

    let case_manifest = CompileManifest {
        files: vec![
            CompileFile::new("wiki/index.md", "# Index\n"),
            CompileFile::new("wiki/overview.md", "# Overview\n"),
            CompileFile::new("wiki/log.md", "# Log\n"),
            CompileFile::new("wiki/concepts/agent.md", "# collision\n"),
        ],
        deletions: Vec::new(),
        summary: "collision".into(),
    };
    let collision = CompileService::classify_workflow_changes(
        &context,
        &case_manifest,
        &plan,
        &baseline,
        false,
    )
    .unwrap();
    assert!(collision
        .conflicted
        .contains(&"wiki/concepts/agent.md".into()));
    fs::remove_dir_all(root).ok();
}

#[test]
fn delete_overwrite_broad_rewrite_and_conflict_review_is_persisted_as_waiting() {
    let (context, root) = project("waiting");
    let (scope, _) = source_scope(&context);
    let checkpoint_hash = GitService
        .initialize_repository(&context, "initial")
        .unwrap()
        .head
        .unwrap();
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let run = enqueue_update(
        &context,
        &tasks,
        &coordinator,
        UpdateWikiMode::FullRecompile,
        scope,
    );
    let sink = WorkflowStageSink::new(&tasks, &coordinator, &run.task_id);
    for stage in [
        "analyze_sources",
        "create_checkpoint",
        "plan_updates",
        "generate_candidates",
        "validate_structure",
    ] {
        sink.start(stage).unwrap();
        sink.complete(stage).unwrap();
    }
    sink.start("review_risk").unwrap();
    let candidate = CompileCandidate {
        route: ResolvedCompileRoute::Agent {
            agent: AgentKind::Claude,
            model: None,
        },
        plan: CompilePlan {
            summary: "rename".into(),
            items: vec![CompilePlanItem {
                action: CompileAction::Create,
                target_path: "wiki/concepts/新名称.md".into(),
                page_type: CompilePageType::Concept,
                source_ids: vec!["资料.md".into()],
                affected_existing_pages: vec!["wiki/concepts/旧名称.md".into()],
                reason: "rename".into(),
                risk_flags: vec!["rename".into()],
            }],
            global_risk_flags: vec!["broad_rewrite".into()],
        },
        manifest: CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "# Index\n"),
                CompileFile::new("wiki/overview.md", "# Overview\n"),
                CompileFile::new("wiki/log.md", "# Log\n"),
                CompileFile::new(
                    "wiki/concepts/新名称.md",
                    "---\ntype: concept\nsources:\n  - 资料.md\n---\n# 新名称\n\n> Sources: [资料](../sources/资料.md)\n",
                ),
            ],
            deletions: vec!["wiki/concepts/旧名称.md".into()],
            summary: "rename".into(),
        },
    };
    let agents = AgentService::with_runner(Arc::new(NoAgents));
    let llm = LlmService;
    let secrets = SecretService::default();
    let settings = SettingsService::default();
    let file_store = FileStore;
    let bookmarks = BookmarkService::default();
    let search = SearchService::default();
    let confirmations = Default::default();
    let services = UpdateWikiExecutionServices {
        compile: CompileExecutionServices {
            agent_service: &agents,
            llm_service: &llm,
            secret_service: &secrets,
            settings_service: &settings,
            task_service: &tasks,
        },
        git_service: &GitService,
        file_store: &file_store,
        bookmark_service: &bookmarks,
        search_service: &search,
        confirmation_registry: &confirmations,
        coordinator: &coordinator,
    };
    let candidate_workspace = std::env::temp_dir()
        .join("llm-wiki-desktop")
        .join(&run.task_id);
    fs::create_dir_all(&candidate_workspace).unwrap();
    let waiting = persist_update_wiki_review(
        &context,
        &run,
        &candidate,
        &[
            "wiki/concepts/旧名称.md".into(),
            "wiki/concepts/新名称.md".into(),
        ],
        &CompileService::snapshot_wiki(&context).unwrap(),
        Some(checkpoint_hash.clone()),
        &services,
    )
    .unwrap();
    assert_eq!(
        waiting.display_status,
        WorkflowDisplayStatus::WaitingForConfirmation
    );
    let pending = waiting.pending_action.clone().unwrap();
    assert_eq!(
        pending.checkpoint_hash.as_deref(),
        Some(checkpoint_hash.as_str())
    );
    assert!(matches!(
        pending.candidate,
        Some(llm_wiki_desktop_lib::models::workflow::WorkflowCandidateReference::TaskOwned {
            candidate_id
        }) if candidate_id == run.task_id
    ));
    let stored = confirmations.peek(&pending.id).unwrap();
    assert!(stored
        .action
        .preview
        .and_then(|preview| preview.diff)
        .is_some_and(|diff| diff.contains("旧名称.md")));
    assert!(candidate_workspace
        .join("workflow-candidate.json")
        .is_file());
    assert!(update_wiki_candidate_is_valid(&run.task_id, &context.root));
    fs::remove_dir_all(candidate_workspace).ok();
    fs::remove_dir_all(root).ok();
}

#[test]
fn duplicate_fingerprint_returns_existing_and_atomic_boundary_rejects_cancel() {
    let (context, root) = project("dedupe");
    let (scope, _) = source_scope(&context);
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let run = enqueue_update(
        &context,
        &tasks,
        &coordinator,
        UpdateWikiMode::ChangedSources,
        scope.clone(),
    );
    let duplicate = coordinator
        .enqueue(
            &tasks,
            EnqueueWorkflow {
                project_id: context.project_id.clone(),
                project_root: context.root.clone(),
                task_state_root: Some(context.app_dir.join("tasks")),
                title: "Update Wiki".into(),
                kind: WorkflowKind::UpdateWiki,
                scope: WorkflowScope::UpdateWiki {
                    mode: UpdateWikiMode::ChangedSources,
                    source_versions: vec![scope],
                },
                route: run.route.clone(),
                baseline_fingerprint: run.baseline_fingerprint.clone(),
                execution_options: WorkflowExecutionOptions {
                    preparation_revision: "preparation-test".into(),
                    ..WorkflowExecutionOptions::default()
                },
                stages: workflow_stages(&WorkflowKind::UpdateWiki),
                retry: None,
            },
        )
        .unwrap();
    match duplicate {
        llm_wiki_desktop_lib::models::workflow::WorkflowStartOutcome::Existing {
            run: existing,
        } => assert_eq!(existing.task_id, run.task_id),
        _ => panic!("duplicate must return existing run"),
    }
    tasks.set_task_cancellable(&run.task_id, false).unwrap();
    assert!(coordinator.cancel(&tasks, &run.task_id).is_err());
    tasks.set_task_cancellable(&run.task_id, true).unwrap();
    assert!(coordinator.cancel(&tasks, &run.task_id).is_ok());
    assert!(tasks.set_task_cancellable(&run.task_id, false).is_err());
    fs::remove_dir_all(root).ok();
}

#[test]
fn low_risk_update_uses_hash_checked_apply_and_never_overwrites_late_edit() {
    let (context, root) = project("late-edit");
    let baseline = CompileService::snapshot_wiki(&context).unwrap();
    let manifest = CompileManifest {
        files: vec![
            CompileFile::new("wiki/index.md", "# Candidate\n"),
            CompileFile::new("wiki/overview.md", "# Overview\n"),
            CompileFile::new("wiki/log.md", "# Log\n"),
        ],
        deletions: Vec::new(),
        summary: "candidate".into(),
    };
    fs::write(context.wiki_dir.join("index.md"), "# User edit\n").unwrap();
    let expected = baseline;
    let error =
        CompileService::apply_confirmed_workflow_manifest(&context, &manifest, None, &expected)
            .unwrap_err();
    assert_eq!(error.code, "CONFIRMATION_STATE_MISMATCH");
    assert_eq!(
        fs::read_to_string(context.wiki_dir.join("index.md")).unwrap(),
        "# User edit\n"
    );
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn real_low_risk_runner_completes_all_stages_consumes_source_and_commits() {
    let (context, root) = project("complete-runner");
    let (scope, _) = source_scope(&context);
    GitService
        .initialize_repository(&context, "initial")
        .unwrap();
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let run = enqueue_update(
        &context,
        &tasks,
        &coordinator,
        UpdateWikiMode::ChangedSources,
        scope,
    );
    let agents = AgentService::with_runner(Arc::new(SuccessfulAgent { delete_path: None }));
    let llm = LlmService;
    let secrets = SecretService::default();
    let settings = SettingsService::default();
    let file_store = FileStore;
    let bookmarks = BookmarkService::default();
    let search = SearchService::default();
    let confirmations = Default::default();
    let services = UpdateWikiExecutionServices {
        compile: CompileExecutionServices {
            agent_service: &agents,
            llm_service: &llm,
            secret_service: &secrets,
            settings_service: &settings,
            task_service: &tasks,
        },
        git_service: &GitService,
        file_store: &file_store,
        bookmark_service: &bookmarks,
        search_service: &search,
        confirmation_registry: &confirmations,
        coordinator: &coordinator,
    };

    run_update_wiki(&context, run.clone(), &services).await;
    let completed = tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(completed.display_status, WorkflowDisplayStatus::Completed);
    assert_eq!(completed.stages.len(), 9);
    assert!(completed
        .stages
        .iter()
        .all(|stage| stage.status == WorkflowStageStatus::Completed));
    assert!(context.wiki_dir.join("concepts/工作流成功.md").is_file());
    let result = completed.result.unwrap();
    let llm_wiki_desktop_lib::models::workflow::WorkflowResult::UpdateWiki {
        created,
        final_commit,
        ..
    } = result
    else {
        panic!("expected Update Wiki result");
    };
    assert_eq!(created, 1);
    assert!(final_commit.is_some());
    assert!(context
        .app_dir
        .join("compile")
        .join(format!("{}.json", run.task_id))
        .is_file());
    assert!(!GitService.repository_status(&context).unwrap().has_changes);
    fs::remove_dir_all(root).ok();
}

#[tokio::test]
async fn real_generated_deletion_enters_persisted_waiting_without_mutating_wiki() {
    let (context, root) = project("delete-runner");
    let old_path = "wiki/concepts/旧名称.md";
    fs::write(
        context.resolve_project_path(old_path).unwrap(),
        "---\ntype: concept\nsources:\n  - 资料.md\n---\n# 旧名称\n\n> Sources: [资料](../sources/资料.md)\n",
    )
    .unwrap();
    let (scope, _) = source_scope(&context);
    GitService
        .initialize_repository(&context, "initial")
        .unwrap();
    let tasks = TaskService::default();
    let coordinator = WorkflowCoordinator::default();
    let run = enqueue_update(
        &context,
        &tasks,
        &coordinator,
        UpdateWikiMode::ChangedSources,
        scope,
    );
    let agents = AgentService::with_runner(Arc::new(SuccessfulAgent {
        delete_path: Some(old_path.into()),
    }));
    let llm = LlmService;
    let secrets = SecretService::default();
    let settings = SettingsService::default();
    let file_store = FileStore;
    let bookmarks = BookmarkService::default();
    let search = SearchService::default();
    let confirmations = Default::default();
    let services = UpdateWikiExecutionServices {
        compile: CompileExecutionServices {
            agent_service: &agents,
            llm_service: &llm,
            secret_service: &secrets,
            settings_service: &settings,
            task_service: &tasks,
        },
        git_service: &GitService,
        file_store: &file_store,
        bookmark_service: &bookmarks,
        search_service: &search,
        confirmation_registry: &confirmations,
        coordinator: &coordinator,
    };

    run_update_wiki(&context, run.clone(), &services).await;
    let waiting = tasks.get_workflow_run(&run.task_id).unwrap();
    assert_eq!(
        waiting.display_status,
        WorkflowDisplayStatus::WaitingForConfirmation
    );
    assert!(context.resolve_project_path(old_path).unwrap().is_file());
    assert!(!context.wiki_dir.join("concepts/工作流成功.md").exists());
    assert!(update_wiki_candidate_is_valid(&run.task_id, &context.root));
    llm_wiki_desktop_lib::services::discard_update_wiki_candidate(&run.task_id).unwrap();
    fs::remove_dir_all(root).ok();
}

#[test]
fn cjk_wikilinks_and_resource_links_survive_workflow_semantic_validation() {
    let (context, root) = project("wikilink-resource");
    let target = "wiki/concepts/代理记忆.md";
    let plan = CompilePlan {
        summary: "create cjk page".into(),
        items: vec![CompilePlanItem {
            action: CompileAction::Create,
            target_path: target.into(),
            page_type: CompilePageType::Concept,
            source_ids: vec!["资料.md".into()],
            affected_existing_pages: Vec::new(),
            reason: "new concept".into(),
            risk_flags: Vec::new(),
        }],
        global_risk_flags: Vec::new(),
    };
    let body = "---\ntype: concept\nsources:\n  - 资料.md\n---\n# 代理记忆\n\n参见 [[相关概念]]。\n\n![结构图](../assets/结构图.png)\n\n> Sources: [[sources/资料]]\n";
    let manifest = CompileManifest {
        files: vec![
            CompileFile::new("wiki/index.md", "# Index\n"),
            CompileFile::new("wiki/overview.md", "# Overview\n"),
            CompileFile::new("wiki/log.md", "# Log\n"),
            CompileFile::new(target, body),
        ],
        deletions: Vec::new(),
        summary: "cjk".into(),
    };
    let known = HashSet::from(["资料.md".to_string()]);
    CompileService::validate_workflow_manifest_semantics(&context, &manifest, Some(&plan), &known)
        .unwrap();
    assert!(manifest.files[3].content.contains("[[相关概念]]"));
    assert!(manifest.files[3].content.contains("结构图.png"));
    fs::remove_dir_all(root).ok();
}
