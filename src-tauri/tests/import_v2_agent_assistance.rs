use std::sync::{Arc, Mutex};

use llm_wiki_desktop_lib::{
    errors::BackendError,
    models::{
        agent::AgentKind,
        import_v2::{
            AttemptOutcome, AttemptRecord, ImportInput, ImportInputKind, ImportIssue, ImportItem,
            ImportItemStatus, ImportResourceMode, ImportSession, ImportStage,
        },
        import_v2_agent::{
            AgentAssistancePolicy, AgentAssistanceTrigger, AgentRecoveryAction, AgentToolGrant,
        },
        llm::{LlmProviderConfig, LlmProviderKind},
        paths::ProjectContext,
        task::{TaskStatus, TaskType},
    },
    services::{
        import_v2::{
            agent_assistance::{AgentAssistanceService, LocalAgentStartDecision},
            agent_tools::{
                ImportAgentToolBroker, ImportAgentToolCall, ImportAgentToolExecutor,
                ImportAgentToolResult, ImportAgentToolTaskContext,
            },
            quality_gate::QualityGate,
            ImportV2Service, SessionStore,
        },
        AgentService, FileStore, SettingsService,
    },
    tasks::TaskService,
};

#[derive(Default)]
struct RecordingExecutor {
    calls: Mutex<Vec<ImportAgentToolCall>>,
}

impl ImportAgentToolExecutor for RecordingExecutor {
    fn execute(
        &self,
        _context: &ImportAgentToolTaskContext,
        call: &ImportAgentToolCall,
    ) -> Result<ImportAgentToolResult, BackendError> {
        self.calls.lock().unwrap().push(call.clone());
        Ok(ImportAgentToolResult {
            outcome: "succeeded".into(),
            output_hashes: vec![],
            warnings: vec![],
            resource_units: 1,
        })
    }
}

#[test]
fn threat_corpus_denies_injected_authority_secret_echo_and_executables() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root
        .path()
        .join(".app/import-sessions/session-a/items/item-a/staging/agent/workspace-a");
    std::fs::create_dir_all(workspace.join("output")).unwrap();
    let context = ImportAgentToolTaskContext {
        task_id: "task-a".into(),
        project_id: "project-a".into(),
        session_id: "session-a".into(),
        item_id: "item-a".into(),
        workspace_root: workspace,
        grants: vec![
            AgentToolGrant::RunDeterministicRoute,
            AgentToolGrant::ValidateCandidate,
        ],
        input_hashes: vec!["input-hash".into()],
        cancelled: false,
    };
    let executor = Arc::new(RecordingExecutor::default());
    let broker = ImportAgentToolBroker::new(executor.clone());
    let malicious_calls = [
        ImportAgentToolCall::RunDeterministicRoute {
            route: "cmd.exe /c echo %TOKEN%".into(),
        },
        ImportAgentToolCall::RunDeterministicRoute {
            route: "https://evil.example/steal".into(),
        },
        ImportAgentToolCall::RunDeterministicRoute {
            route: "git.status".into(),
        },
        ImportAgentToolCall::RunDeterministicRoute {
            route: "browser.captcha_bypass".into(),
        },
        ImportAgentToolCall::RunDeterministicRoute {
            route: "fetch.paywall".into(),
        },
        ImportAgentToolCall::ValidateCandidate {
            relative_markdown_path: "../item-b/output.md".into(),
        },
        ImportAgentToolCall::ValidateCandidate {
            relative_markdown_path: "output/payload.exe".into(),
        },
    ];
    for call in malicious_calls {
        assert!(broker.invoke(&context, call).is_err());
    }
    assert!(executor.calls.lock().unwrap().is_empty());
    for secret in [
        "Authorization: Bearer secret-value",
        "api_key=do-not-persist",
        "-----BEGIN PRIVATE KEY-----",
        "ghp_abcdefghijklmnopqrstuvwxyz123456",
    ] {
        assert!(QualityGate::validate_agent_text_fields([secret]).is_err());
    }
    for (path, bytes) in [
        ("payload.exe", b"MZ".as_slice()),
        ("renamed.png", b"MZ executable".as_slice()),
        ("notes.txt", b"api_key=do-not-persist".as_slice()),
        ("active.svg", b"<svg><script/></svg>".as_slice()),
    ] {
        assert!(QualityGate::validate_agent_asset(path, bytes).is_err());
    }
}

#[test]
fn product_policy_allows_only_approved_local_hard_failure_automation() {
    let policy = AgentAssistancePolicy {
        auto_local_on_hard_failure: true,
        auto_local_on_quality_warning: false,
        auto_byok: false,
        max_attempts_per_item: 2,
    };
    assert_eq!(
        AgentAssistanceService::local_start_decision(
            &policy,
            AgentAssistanceTrigger::DeterministicHardFailure,
            true,
            0,
        ),
        LocalAgentStartDecision::Start
    );
    assert_eq!(
        AgentAssistanceService::local_start_decision(
            &policy,
            AgentAssistanceTrigger::QualityOptimization,
            true,
            0,
        ),
        LocalAgentStartDecision::ManualOnly
    );
    assert_eq!(
        AgentAssistanceService::local_start_decision(
            &policy,
            AgentAssistanceTrigger::DeterministicHardFailure,
            false,
            0,
        ),
        LocalAgentStartDecision::AgentUnavailable
    );
    assert!(!policy.auto_byok);
}

#[test]
fn restart_twice_closes_inflight_tasks_without_formal_content_mutation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("wiki")).unwrap();
    std::fs::write(root.path().join("wiki/keep.md"), "# keep").unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::AgentRun,
            "project-a".into(),
            root.path().to_path_buf(),
            "Agent assistance".into(),
            true,
        )
        .unwrap();
    tasks
        .transition_status(&task.id, TaskStatus::Running)
        .unwrap();

    let first_restart = TaskService::default();
    let first = first_restart.recover_tasks(root.path()).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].status, TaskStatus::Failed);
    let second_restart = TaskService::default();
    let second = second_restart.recover_tasks(root.path()).unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].status, TaskStatus::Failed);
    assert_eq!(
        std::fs::read_to_string(root.path().join("wiki/keep.md")).unwrap(),
        "# keep"
    );
}

#[test]
fn uncertain_byok_charge_requires_new_explicit_acknowledgement_before_task_creation() {
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("project-a", root.path().to_path_buf());
    let files = FileStore;
    let imports = ImportV2Service::default();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    settings
        .save_provider(
            &context,
            LlmProviderConfig {
                provider: LlmProviderKind::OpenAi,
                model: "gpt-test".into(),
                base_url: "https://api.example.invalid".into(),
                context_window: 8_192,
                enabled: true,
            },
        )
        .unwrap();
    settings
        .set_import_agent_policy(
            &context,
            AgentAssistancePolicy::default(),
            Some(AgentKind::Codex),
        )
        .unwrap();
    let mut item = ImportItem::queued(
        "item-a",
        ImportInput {
            kind: ImportInputKind::File,
            display_name: "notes.txt".into(),
            locator: "notes.txt".into(),
            normalized_locator: None,
            source_identity: None,
        },
    );
    item.status = ImportItemStatus::Failed;
    item.issue = Some(ImportIssue {
        code: "IMPORT_FILE_PARSE_FAILED".into(),
        message: "deterministic parser failed".into(),
        stage: ImportStage::Extract,
        retryable: true,
        user_action_required: true,
        recovery_actions: vec![],
        available_actions: vec![AgentRecoveryAction::RequestByok],
    });
    item.attempts.push(AttemptRecord {
        route: "byok_assistance/prior-task".into(),
        engine_id: "open_ai:gpt-test".into(),
        engine_version: "provider-api".into(),
        stage: ImportStage::Extract,
        started_at: chrono::Utc::now().to_rfc3339(),
        completed_at: Some(chrono::Utc::now().to_rfc3339()),
        outcome: AttemptOutcome::Failed,
        warnings: vec!["BYOK_CHARGE_STATUS_UNKNOWN".into()],
    });
    let mut session = ImportSession::new("session-a", "project-a", ImportResourceMode::Balanced);
    session.items.push(item);
    SessionStore::default()
        .save(&context, &files, &session)
        .unwrap();
    let staging = root
        .path()
        .join(".app/import-sessions/session-a/items/item-a/staging");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(staging.join("source.bin"), "safe source").unwrap();
    let agents = AgentService::default();
    let tasks = TaskService::default();
    let assistance = AgentAssistanceService::new(
        &imports,
        &files,
        &settings,
        &agents,
        &tasks,
        AgentAssistanceService::bundled_skill_path(),
    );
    let scope = assistance
        .preview_byok_scope(
            &context,
            "session-a",
            "item-a",
            AgentAssistanceTrigger::Manual,
            LlmProviderKind::OpenAi,
        )
        .unwrap();
    assert!(scope.requires_duplicate_charge_acknowledgement);
    let error = assistance
        .start_byok(
            &context,
            "session-a",
            "item-a",
            AgentAssistanceTrigger::Manual,
            LlmProviderKind::OpenAi,
            &scope.model,
            &scope.approval_id,
            &scope.scope_sha256,
            false,
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_BYOK_DUPLICATE_CHARGE_ACK_REQUIRED");
    assert!(tasks.list_tasks(None).is_empty());
}

#[test]
fn qa_matrix_names_every_release_gate_without_claiming_external_certification() {
    let evidence = include_str!("../../docs/qa/import-v2-agent-assistance.md");
    for required in [
        "Prompt injection",
        "Tool injection",
        "Path traversal",
        "Command injection",
        "Secret echo",
        "Cross-item/project",
        "Cancellation and restart",
        "Duplicate BYOK charge",
        "Exact audit records",
        "Candidate, Diff, and three-way merge",
        "No direct raw/wiki mutation",
        "Core/File/Web integration",
    ] {
        assert!(evidence.contains(required), "missing QA gate: {required}");
    }
    assert!(evidence.contains("Local automated evidence"));
    assert!(!evidence.contains("independently certified"));
}
