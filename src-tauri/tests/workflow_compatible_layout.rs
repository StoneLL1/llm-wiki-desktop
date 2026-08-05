use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::workflow::{
    WorkflowFilesystemAccess, WorkflowGitState, WorkflowKind, WorkflowPersistenceMode,
    WorkflowPrerequisiteAction, WorkflowProjectTrust,
};
use llm_wiki_desktop_lib::services::{
    AgentService, SecretService, SettingsService, WorkflowAccessSnapshot, WorkflowService,
};
use llm_wiki_desktop_lib::tasks::TaskService;

fn restricted_access() -> WorkflowAccessSnapshot {
    WorkflowAccessSnapshot {
        trust_kind: None,
        trust: WorkflowProjectTrust::Untrusted,
        filesystem_access: WorkflowFilesystemAccess::ReadOnly,
        persistence: WorkflowPersistenceMode::MemoryOnly,
        git_state: WorkflowGitState::Clean,
        authority_revision: "batch-a-witness".into(),
    }
}

#[test]
fn compatible_enablement_uses_app_owned_state_roots_without_content_write_roots() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".obsidian")).unwrap();
    std::fs::create_dir_all(root.path().join(".app/compat")).unwrap();
    std::fs::write(root.path().join(".app/compat/purpose.md"), "# Purpose\n").unwrap();
    std::fs::write(root.path().join(".app/compat/schema.md"), "# Schema\n").unwrap();
    std::fs::write(root.path().join("索引.md"), "# 索引\n").unwrap();

    let context = ProjectContext::new("compatible", root.path().to_path_buf())
        .with_resolved_layout()
        .unwrap();
    assert_eq!(context.layout.app_state_root.as_deref(), Some(".app/compat"));
    assert_eq!(
        context.layout.task_state_root.as_deref(),
        Some(".app/compat/tasks")
    );
    assert_eq!(
        context.layout.workflow_state_root.as_deref(),
        Some(".app/compat/workflows")
    );
    assert!(context.layout.source_write_root.is_none());
    assert!(context.layout.wiki_write_root.is_none());
}

#[test]
fn compatible_restricted_overview_keeps_readable_markdown_without_creating_state() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".obsidian")).unwrap();
    std::fs::create_dir_all(root.path().join("笔记/嵌套")).unwrap();
    std::fs::write(root.path().join("笔记/嵌套/主题.md"), "# 主题\n").unwrap();
    let context = ProjectContext::new("compatible", root.path().to_path_buf())
        .with_resolved_layout()
        .unwrap();
    let config = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let overview = WorkflowService::default()
        .project_overview(
            &context,
            restricted_access(),
            &settings,
            &secrets,
            &agents,
            &TaskService::default(),
        )
        .unwrap();
    assert_eq!(overview.rows.len(), 3);
    let health = overview.rows.iter().find(|row| row.kind == WorkflowKind::HealthCheck).unwrap();
    assert_ne!(
        health.prerequisite.as_ref().map(|item| &item.action),
        Some(&WorkflowPrerequisiteAction::ImportSources),
        "Workflows overview must consume compatible readable Markdown rather than fixed native roots"
    );
    assert!(context
        .list_markdown_files_for_roles(&[
            llm_wiki_desktop_lib::models::layout::ProjectMarkdownRootRole::Source,
            llm_wiki_desktop_lib::models::layout::ProjectMarkdownRootRole::Wiki,
            llm_wiki_desktop_lib::models::layout::ProjectMarkdownRootRole::Mixed,
        ])
        .unwrap()
        .iter()
        .any(|path| path.ends_with("主题.md")));
    assert!(!root.path().join(".app").exists());
}


#[test]
fn compatible_fixture_matrix_keeps_nashsu_and_custom_markdown_roots_read_only() {
    let nashsu = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(nashsu.path().join("raw")).unwrap();
    std::fs::create_dir_all(nashsu.path().join("wiki")).unwrap();
    std::fs::write(nashsu.path().join("wiki/index.md"), "# Index\n").unwrap();
    std::fs::write(nashsu.path().join("wiki/overview.md"), "# Overview\n").unwrap();
    let nashsu_context = ProjectContext::new("nashsu", nashsu.path().to_path_buf())
        .with_resolved_layout()
        .unwrap();
    assert!(nashsu_context.layout.app_state_root.is_none());
    assert!(nashsu_context.layout.task_state_root.is_none());
    assert!(nashsu_context.layout.source_write_root.is_none());
    assert!(nashsu_context
        .list_markdown_files_for_roles(&[
            llm_wiki_desktop_lib::models::layout::ProjectMarkdownRootRole::Source,
            llm_wiki_desktop_lib::models::layout::ProjectMarkdownRootRole::Wiki,
            llm_wiki_desktop_lib::models::layout::ProjectMarkdownRootRole::Mixed,
        ])
        .unwrap()
        .iter()
        .any(|path| path.ends_with("index.md")));

    let custom = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(custom.path().join("notes/nested")).unwrap();
    std::fs::write(custom.path().join("notes/nested/page.md"), "# Page\n").unwrap();
    let custom_context = ProjectContext::new("custom", custom.path().to_path_buf())
        .with_resolved_layout()
        .unwrap();
    assert!(custom_context.layout.app_state_root.is_none());
    assert!(custom_context.layout.task_state_root.is_none());
    assert!(custom_context.layout.source_write_root.is_none());
    assert!(custom_context
        .list_markdown_files_for_roles(&[
            llm_wiki_desktop_lib::models::layout::ProjectMarkdownRootRole::Mixed,
        ])
        .unwrap()
        .iter()
        .any(|path| path.ends_with("page.md")));
}
