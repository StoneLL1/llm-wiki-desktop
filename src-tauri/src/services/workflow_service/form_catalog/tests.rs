use super::*;
use crate::models::agent::AgentKind;
use crate::models::layout::ProjectMarkdownRoot;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::workflow::{HealthCheckMode, WorkflowArtifactType, WorkflowRoute};
use crate::services::{FileStore, WorkflowPreference};

fn fixture() -> (tempfile::TempDir, ProjectContext, SettingsService) {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("表单-Café", root.path().join("project"));
    std::fs::create_dir_all(&context.root).unwrap();
    let settings = SettingsService::with_config_dir(root.path().join("config"));
    (root, context, settings)
}

#[test]
fn generate_catalog_lists_unicode_names_without_reading_bodies_or_source_registry() {
    let (_root, context, settings) = fixture();
    std::fs::create_dir_all(context.root.join("wiki/中文")).unwrap();
    std::fs::create_dir_all(context.root.join("wiki/sources")).unwrap();
    std::fs::create_dir_all(context.root.join(".app/workflows")).unwrap();
    std::fs::write(context.root.join("wiki/中文/Café.md"), [0xff, 0xfe]).unwrap();
    std::fs::write(context.root.join("wiki/sources/source.md"), [0xff]).unwrap();
    std::fs::write(context.root.join(".app/source-index-v2.json"), "invalid").unwrap();
    std::fs::write(
        context.root.join(".app/workflows/preferences.json"),
        "invalid",
    )
    .unwrap();
    assert!(FileStore
        .read_markdown(&context, "wiki/中文/Café.md")
        .is_err());

    let catalog = WorkflowService::default()
        .form_catalog(&context, &settings, WorkflowKind::GenerateContent)
        .unwrap();
    assert_eq!(catalog.wiki_pages, ["wiki/中文/Café.md"]);
    assert!(catalog.remembered_draft.is_none());
    assert_eq!(catalog.routes.len(), AgentKind::ALL.len());
}

#[test]
fn health_catalog_does_not_even_resolve_markdown_roots() {
    let (_root, mut context, settings) = fixture();
    context.layout.markdown_roots = vec![ProjectMarkdownRoot {
        path: "../outside-project".into(),
        role: ProjectMarkdownRootRole::Wiki,
        exclude: None,
    }];
    // Control: an attempted inventory would fail before any traversal.
    assert!(wiki_page_catalog(&context).is_err());
    let catalog = WorkflowService::default()
        .form_catalog(&context, &settings, WorkflowKind::HealthCheck)
        .unwrap();
    assert!(catalog.wiki_pages.is_empty());
    assert!(catalog.routes.iter().all(|route| match route {
        WorkflowRouteSelection::Agent { agent } =>
            AgentService::lint_route_profile_revision(*agent).is_some(),
        WorkflowRouteSelection::Byok { .. } => true,
    }));
}

#[test]
fn configured_provider_is_listed_without_credentials_but_not_health_default() {
    let (_root, context, settings) = fixture();
    let mut config = settings.read_settings(&context).unwrap();
    config.agent_default = None;
    config.llm_providers = vec![LlmProviderConfig {
        provider: LlmProviderKind::OpenAi,
        model: "configured-model".into(),
        base_url: "https://api.openai.com/v1".into(),
        context_window: 32000,
        enabled: true,
    }];
    settings.save_settings(&context, &config).unwrap();
    FileStore
        .write_json_atomic(
            &context,
            context.layout.settings_path.as_deref().unwrap(),
            &config.to_project_file(),
        )
        .unwrap();
    let service = WorkflowService::default();
    let provider = WorkflowRouteSelection::Byok {
        provider: LlmProviderKind::OpenAi,
    };
    let health = service
        .form_catalog(&context, &settings, WorkflowKind::HealthCheck)
        .unwrap();
    assert!(health.routes.contains(&provider));
    assert!(health.default_route.is_none());
    let generate = service
        .form_catalog(&context, &settings, WorkflowKind::GenerateContent)
        .unwrap();
    assert_eq!(generate.default_route, Some(provider));
    assert!(service
        .form_catalog(&context, &settings, WorkflowKind::UpdateWiki)
        .is_err());
}

#[test]
fn catalog_restores_only_editable_scope_and_route_from_preferences() {
    let (_root, context, settings) = fixture();
    let service = WorkflowService::default();
    let identity = project_identity(&context.root).unwrap();
    let scope = WorkflowScope::GenerateContent {
        artifact_type: WorkflowArtifactType::BeautifulRead,
        page_paths: vec!["wiki/外部已删除.md".into()],
        output_path: None,
    };
    for (kind, scope) in [
        (WorkflowKind::GenerateContent, scope.clone()),
        (
            WorkflowKind::HealthCheck,
            WorkflowScope::HealthCheck {
                mode: HealthCheckMode::LocalQuick,
            },
        ),
    ] {
        service
            .preferences
            .remember(
                &context,
                &identity.canonical_identity_key,
                &identity.identity_revision,
                &WorkflowPersistenceMode::Persistent,
                WorkflowPreference {
                    kind,
                    scope,
                    route: Some(WorkflowRoute::Agent {
                        agent: AgentKind::Codex,
                        model: None,
                        route_revision: "old-route".into(),
                    }),
                    baseline_fingerprint: "a".repeat(64),
                    preparation_fingerprint: "b".repeat(64),
                    saved_at: String::new(),
                },
            )
            .unwrap();
    }
    let catalog = service
        .form_catalog(&context, &settings, WorkflowKind::GenerateContent)
        .unwrap();
    let draft = catalog.remembered_draft.unwrap();
    assert_eq!(draft.scope, scope);
    assert_eq!(
        draft.route_selection,
        Some(WorkflowRouteSelection::Agent {
            agent: AgentKind::Codex
        })
    );
}

#[test]
fn remembered_generate_destination_never_becomes_an_implicit_overwrite() {
    let (_root, context, settings) = fixture();
    let service = WorkflowService::default();
    let identity = project_identity(&context.root).unwrap();
    let previous_scope = WorkflowScope::GenerateContent {
        artifact_type: WorkflowArtifactType::KnowledgeCard,
        page_paths: vec!["wiki/中文.md".into()],
        output_path: Some("exports/html/previous-card.html".into()),
    };
    std::fs::create_dir_all(context.root.join("exports/html")).unwrap();
    std::fs::write(
        context.root.join("exports/html/previous-card.html"),
        "previous artifact",
    )
    .unwrap();
    service
        .preferences
        .remember(
            &context,
            &identity.canonical_identity_key,
            &identity.identity_revision,
            &WorkflowPersistenceMode::Persistent,
            WorkflowPreference {
                kind: WorkflowKind::GenerateContent,
                scope: previous_scope.clone(),
                route: None,
                baseline_fingerprint: "a".repeat(64),
                preparation_fingerprint: "b".repeat(64),
                saved_at: String::new(),
            },
        )
        .unwrap();
    let draft = service
        .form_catalog(&context, &settings, WorkflowKind::GenerateContent)
        .unwrap()
        .remembered_draft
        .unwrap();
    assert_eq!(
        draft.scope,
        WorkflowScope::GenerateContent {
            artifact_type: WorkflowArtifactType::KnowledgeCard,
            page_paths: vec!["wiki/中文.md".into()],
            output_path: None,
        }
    );
    // The catalog is a projection; it must not rewrite saved execution facts.
    let persisted = service
        .preferences
        .load(
            &context,
            &identity.canonical_identity_key,
            &identity.identity_revision,
            &WorkflowPersistenceMode::Persistent,
        )
        .unwrap();
    assert_eq!(persisted[0].scope, previous_scope);
    assert_eq!(
        std::fs::read_to_string(context.root.join("exports/html/previous-card.html")).unwrap(),
        "previous artifact"
    );
}

#[test]
fn source_exceptions_preserve_nested_wiki_pages_in_compatible_layouts() {
    let (_root, mut context, settings) = fixture();
    std::fs::create_dir_all(context.root.join("notes/wiki")).unwrap();
    std::fs::write(context.root.join("notes/wiki/页面.md"), "page").unwrap();
    std::fs::write(context.root.join("notes/source.md"), "source").unwrap();
    context.layout.markdown_roots = vec![
        ProjectMarkdownRoot {
            path: "notes".into(),
            role: ProjectMarkdownRootRole::Mixed,
            exclude: None,
        },
        ProjectMarkdownRoot {
            path: "notes".into(),
            role: ProjectMarkdownRootRole::Source,
            exclude: Some(vec!["notes/wiki".into()]),
        },
    ];
    assert_eq!(
        WorkflowService::default()
            .form_catalog(&context, &settings, WorkflowKind::GenerateContent)
            .unwrap()
            .wiki_pages,
        ["notes/wiki/页面.md"]
    );
}
