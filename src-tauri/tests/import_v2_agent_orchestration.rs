use llm_wiki_desktop_lib::models::import_v2_agent::{
    AgentAssistancePolicy, AgentAssistanceTrigger, DiscardImportAgentCandidateRequest,
    SelectImportAgentCandidateRequest,
};
use llm_wiki_desktop_lib::{
    models::{
        agent::AgentKind,
        import_v2::{
            ArtifactKind, AttemptOutcome, ImportArtifact, ImportInput, ImportInputKind, ImportItem,
            ImportItemStatus, ImportPreviewArtifact, ImportResourceMode, ImportSession,
            QualityLevel, QualityReport,
        },
        paths::ProjectContext,
        task::{TaskStatus, TaskType},
    },
    services::{
        import_v2::{
            agent_assistance::{AgentAssistanceService, LocalAgentStartDecision},
            ImportV2Service, SessionStore,
        },
        FileStore,
    },
    tasks::TaskService,
};

#[test]
fn candidate_actions_are_explicit_and_merge_is_hash_bound() {
    let select = SelectImportAgentCandidateRequest {
        project_id: "project-a".into(),
        project_root_path: "C:/wiki".into(),
        session_id: "session-a".into(),
        item_id: "item-a".into(),
        candidate_id: "candidate-a".into(),
        merged_markdown: Some("# Resolved merge".into()),
        expected_current_wiki_sha256: Some("a".repeat(64)),
    };
    let value = serde_json::to_value(select).unwrap();
    assert_eq!(value["candidateId"], "candidate-a");
    assert_eq!(value["expectedCurrentWikiSha256"], "a".repeat(64));

    let discard = DiscardImportAgentCandidateRequest {
        project_id: "project-a".into(),
        project_root_path: "C:/wiki".into(),
        session_id: "session-a".into(),
        item_id: "item-a".into(),
        candidate_id: "candidate-a".into(),
    };
    assert_eq!(
        serde_json::to_value(discard).unwrap()["candidateId"],
        "candidate-a"
    );
}

#[test]
fn commands_expose_accept_select_and_discard_without_direct_wiki_writes() {
    let source = include_str!("../src/commands/import_v2_agent_commands.rs");
    for command in [
        "accept_import_agent_candidate_v2",
        "select_import_agent_candidate_v2",
        "discard_import_agent_candidate_v2",
    ] {
        assert!(source.contains(command));
    }
    assert!(!source.contains("raw/sources"));
    assert!(!source.contains("std::fs::write"));
    let core_commands = include_str!("../src/commands/import_v2_commands.rs");
    assert!(!core_commands.contains("AgentAssistanceTrigger::QualityOptimization"));
    assert!(core_commands.contains("AgentAssistanceTrigger::DeterministicHardFailure"));
}

#[test]
fn policy_matrix_keeps_quality_manual_and_hard_failure_explicitly_approved() {
    let policy = AgentAssistancePolicy {
        auto_local_on_hard_failure: false,
        auto_local_on_quality_warning: true,
        auto_byok: false,
        max_attempts_per_item: 1,
    };
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
            true,
            0,
        ),
        LocalAgentStartDecision::ManualOnly
    );
    let approved = AgentAssistancePolicy {
        auto_local_on_hard_failure: true,
        ..policy
    };
    assert_eq!(
        AgentAssistanceService::local_start_decision(
            &approved,
            AgentAssistanceTrigger::DeterministicHardFailure,
            true,
            0,
        ),
        LocalAgentStartDecision::Start
    );
    assert_eq!(
        AgentAssistanceService::local_start_decision(
            &approved,
            AgentAssistanceTrigger::Manual,
            false,
            0,
        ),
        LocalAgentStartDecision::AgentUnavailable
    );
}

#[test]
fn session_poll_does_not_close_a_live_agent_attempt_but_restart_failure_does() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("project-a", root.path().to_path_buf());
    let files = FileStore;
    let imports = ImportV2Service::default();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::AgentRun,
            "project-a".into(),
            root.path().to_path_buf(),
            "Agent".into(),
            true,
        )
        .unwrap();
    tasks
        .transition_status(&task.id, TaskStatus::Running)
        .unwrap();
    let mut item = ImportItem::queued(
        "item-a",
        ImportInput {
            kind: ImportInputKind::File,
            display_name: "broken.txt".into(),
            locator: "broken.txt".into(),
            normalized_locator: Some("broken.txt".into()),
            source_identity: None,
            media_save_mode: Default::default(),
        },
    );
    item.status = ImportItemStatus::Failed;
    let mut session = ImportSession::new("session-a", "project-a", ImportResourceMode::Balanced);
    session.items.push(item);
    SessionStore::default()
        .save(&context, &files, &session)
        .unwrap();
    imports
        .begin_agent_assistance(
            &context,
            &files,
            "session-a",
            "item-a",
            &task.id,
            AgentAssistanceTrigger::Manual,
            AgentKind::Claude,
            2,
        )
        .unwrap();
    let live = imports
        .recover_session(&context, &files, &tasks, "session-a")
        .unwrap();
    assert!(live.items[0].attempts[0].completed_at.is_none());
    tasks
        .transition_status(&task.id, TaskStatus::Failed)
        .unwrap();
    let recovered = imports
        .recover_session(&context, &files, &tasks, "session-a")
        .unwrap();
    assert!(recovered.items[0].attempts[0].completed_at.is_some());
    assert_eq!(
        recovered.items[0].attempts[0].outcome,
        AttemptOutcome::Failed
    );
}

#[test]
fn validation_is_explicit_and_failed_optimization_restores_deterministic_preview() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("project-a", root.path().to_path_buf());
    let files = FileStore;
    let imports = ImportV2Service::default();
    let mut item = ImportItem::queued(
        "item-a",
        ImportInput {
            kind: ImportInputKind::File,
            display_name: "source.txt".into(),
            locator: "source.txt".into(),
            normalized_locator: Some("source.txt".into()),
            source_identity: None,
            media_save_mode: Default::default(),
        },
    );
    item.status = ImportItemStatus::PreviewReady;
    item.preview = Some(preview("deterministic/candidate.md"));
    let mut failed = ImportItem::queued(
        "item-b",
        ImportInput {
            kind: ImportInputKind::File,
            display_name: "broken.txt".into(),
            locator: "broken.txt".into(),
            normalized_locator: Some("broken.txt".into()),
            source_identity: None,
            media_save_mode: Default::default(),
        },
    );
    failed.status = ImportItemStatus::Failed;
    let mut session = ImportSession::new("session-a", "project-a", ImportResourceMode::Balanced);
    session.items.extend([item, failed]);
    SessionStore::default()
        .save(&context, &files, &session)
        .unwrap();

    let started = imports
        .begin_agent_assistance(
            &context,
            &files,
            "session-a",
            "item-a",
            "agent-task-1",
            AgentAssistanceTrigger::QualityOptimization,
            AgentKind::Claude,
            2,
        )
        .unwrap();
    assert_eq!(started.status, ImportItemStatus::PreviewReady);
    assert_eq!(started.task_id.as_deref(), Some("agent-task-1"));
    imports
        .finish_agent_assistance_attempt(
            &context,
            &files,
            "session-a",
            "item-a",
            "agent-task-1",
            AttemptOutcome::Failed,
            vec!["safe retry".into()],
        )
        .unwrap();
    imports
        .begin_agent_assistance(
            &context,
            &files,
            "session-a",
            "item-a",
            "agent-task-2",
            AgentAssistanceTrigger::Manual,
            AgentKind::Claude,
            2,
        )
        .unwrap();
    let previous = imports
        .begin_agent_candidate_validation(&context, &files, "session-a", "item-a", "agent-task-2")
        .unwrap();
    assert_eq!(previous, ImportItemStatus::PreviewReady);
    assert_eq!(
        imports
            .load_session(&context, &files, "session-a")
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::Validating
    );
    let restored = imports
        .fail_agent_candidate_validation(
            &context,
            &files,
            "session-a",
            "item-a",
            "agent-task-2",
            previous,
        )
        .unwrap();
    assert_eq!(restored.status, ImportItemStatus::PreviewReady);
    assert_eq!(
        restored.preview.unwrap().markdown.relative_path,
        "deterministic/candidate.md"
    );
    let session = imports.load_session(&context, &files, "session-a").unwrap();
    assert_eq!(session.items[1].status, ImportItemStatus::Failed);
}

fn preview(markdown_path: &str) -> ImportPreviewArtifact {
    let artifact = |kind, relative_path: &str| ImportArtifact {
        kind,
        relative_path: relative_path.into(),
        sha256: "a".repeat(64),
        size_bytes: 1,
    };
    ImportPreviewArtifact {
        markdown: artifact(ArtifactKind::Markdown, markdown_path),
        assets: Vec::new(),
        source_snapshot: artifact(ArtifactKind::SourceSnapshot, "source/source.bin"),
        quality: QualityReport {
            level: QualityLevel::Warning,
            metrics: Vec::new(),
            warnings: vec!["LOW_QUALITY".into()],
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
        },
        title: "Deterministic".into(),
    }
}
