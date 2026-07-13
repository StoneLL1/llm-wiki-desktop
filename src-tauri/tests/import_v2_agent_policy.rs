use llm_wiki_desktop_lib::{
    models::{
        agent::AgentKind, import_v2_agent::AgentAssistancePolicy, llm::LlmProviderKind,
        paths::ProjectContext,
    },
    services::{SecretService, SettingsService},
};

#[test]
fn policy_defaults_updates_and_never_persists_provider_secrets() {
    let project = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("project-policy", project.path().to_path_buf());
    let service = SettingsService::with_config_dir(config.path().to_path_buf());
    let secrets = SecretService::memory();
    let marker = "sk-import-agent-policy-secret-marker";
    secrets.set(LlmProviderKind::OpenAi, marker).unwrap();

    assert_eq!(
        service.get_import_agent_policy(&context).unwrap(),
        AgentAssistancePolicy::default()
    );

    let policy = AgentAssistancePolicy {
        auto_local_on_hard_failure: true,
        auto_local_on_quality_warning: false,
        auto_byok: false,
        max_attempts_per_item: 2,
    };
    let saved = service
        .set_import_agent_policy(&context, policy.clone(), Some(AgentKind::Codex))
        .unwrap();
    assert_eq!(saved, policy);

    let loaded = service.read_settings(&context).unwrap();
    assert_eq!(loaded.import_agent_policy, policy);
    assert_eq!(loaded.agent_default, Some(AgentKind::Codex));

    for path in [
        project.path().join(".app/settings.json"),
        project.path().join(".app/agent-config.json"),
        config.path().join("settings.json"),
    ] {
        let text = std::fs::read_to_string(path).unwrap().to_ascii_lowercase();
        assert!(!text.contains(marker));
        assert!(!text.contains("api_key"));
        assert!(!text.contains("apikey"));
        assert!(!text.contains("password"));
        assert!(!text.contains("secret"));
    }
    assert_eq!(
        secrets.get(LlmProviderKind::OpenAi).unwrap().as_deref(),
        Some(marker)
    );
}

#[test]
fn legacy_project_settings_default_to_manual_cloud_and_quality_paths() {
    let value: llm_wiki_desktop_lib::models::settings::ProjectSettingsFile =
        serde_json::from_value(serde_json::json!({
            "agentDefault": "claude",
            "llmProviders": []
        }))
        .unwrap();

    assert_eq!(value.import_agent_policy, AgentAssistancePolicy::default());
    assert!(!value.import_agent_policy.auto_byok);
    assert!(!value.import_agent_policy.auto_local_on_quality_warning);
}

#[test]
fn policy_rejects_automatic_cloud_and_unbounded_attempts() {
    let project = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("project-invalid-policy", project.path().to_path_buf());
    let service = SettingsService::with_config_dir(config.path().to_path_buf());

    for policy in [
        AgentAssistancePolicy {
            auto_byok: true,
            ..AgentAssistancePolicy::default()
        },
        AgentAssistancePolicy {
            max_attempts_per_item: 0,
            ..AgentAssistancePolicy::default()
        },
        AgentAssistancePolicy {
            max_attempts_per_item: 4,
            ..AgentAssistancePolicy::default()
        },
        AgentAssistancePolicy {
            auto_local_on_quality_warning: true,
            ..AgentAssistancePolicy::default()
        },
    ] {
        let error = service
            .set_import_agent_policy(&context, policy, None)
            .unwrap_err();
        assert_eq!(error.code, "IMPORT_AGENT_POLICY_INVALID");
    }
}

#[test]
fn policy_commands_are_thin_facade_calls_and_registered() {
    let commands = include_str!("../src/commands/import_v2_agent_commands.rs");
    assert!(commands.contains("resolve_project_context"));
    assert!(commands.contains("settings_service.get_import_agent_policy"));
    assert!(commands.contains("settings_service.set_import_agent_policy"));
    for forbidden in ["std::fs", "ProcessRunner", "AgentService", "SecretService"] {
        assert!(!commands.contains(forbidden));
    }

    let lib = include_str!("../src/lib.rs");
    assert!(lib.contains("get_import_agent_policy_v2"));
    assert!(lib.contains("set_import_agent_policy_v2"));
}
