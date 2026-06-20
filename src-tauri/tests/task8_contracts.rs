use llm_wiki_desktop_lib::models::agent::{AgentDetectionState, AgentInfo, AgentKind};
use llm_wiki_desktop_lib::models::compile::{
    CompileFile, CompileManifest, CompileRoute, CompileRoutePreference,
};
use llm_wiki_desktop_lib::models::llm::{LlmProviderConfig, LlmProviderKind};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::{
    AgentInvocation, AgentService, CompileService, LlmService, ProcessRunner, SecretService,
};
use llm_wiki_desktop_lib::tasks::TaskService;
use std::sync::Arc;
use std::time::Duration;

struct FakeProcessRunner;

impl ProcessRunner for FakeProcessRunner {
    fn find_executable(&self, command: &str) -> Option<std::path::PathBuf> {
        (command == "claude").then(|| "C:/fake/claude.exe".into())
    }

    fn run_with_timeout(
        &self,
        _command: &str,
        args: &[&str],
        _timeout: Duration,
    ) -> Result<String, llm_wiki_desktop_lib::errors::BackendError> {
        Ok(if args == ["--version"] {
            "1.0.0".into()
        } else {
            "--print --output-format --settings".into()
        })
    }

    fn run_capture(
        &self,
        _invocation: &AgentInvocation,
    ) -> Result<(String, String), llm_wiki_desktop_lib::errors::BackendError> {
        Ok((String::new(), String::new()))
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<(), llm_wiki_desktop_lib::errors::BackendError> {
        Ok(())
    }
}

#[test]
fn agent_contract_serializes_stable_states_and_install_guidance() {
    let info = AgentInfo {
        kind: AgentKind::Claude,
        command: "claude".into(),
        state: AgentDetectionState::Installed,
        version: Some("2.1.133".into()),
        executable_path: Some("C:/tools/claude.exe".into()),
        is_default: true,
        install_guidance: "npm install -g @anthropic-ai/claude-code".into(),
        error: None,
    };

    let json = serde_json::to_value(info).unwrap();
    assert_eq!(json["kind"], "claude");
    assert_eq!(json["state"], "installed");
    assert_eq!(json["isDefault"], true);
    assert!(json["installGuidance"]
        .as_str()
        .unwrap()
        .contains("npm install"));
}

#[test]
fn provider_config_contains_metadata_but_never_secret_material() {
    let config = LlmProviderConfig {
        provider: LlmProviderKind::OpenAi,
        model: "gpt-4.1-mini".into(),
        base_url: "https://api.openai.com".into(),
        context_window: 32_000,
        enabled: true,
    };
    let raw = serde_json::to_string(&config).unwrap();
    assert!(raw.contains("gpt-4.1-mini"));
    assert!(!raw.to_ascii_lowercase().contains("api_key"));
    assert!(!raw.to_ascii_lowercase().contains("authorization"));
}

#[test]
fn compile_manifest_accepts_only_safe_wiki_markdown_and_core_pages() {
    let valid = CompileManifest {
        files: vec![
            CompileFile::new("wiki/index.md", "# Index"),
            CompileFile::new("wiki/overview.md", "# Overview"),
            CompileFile::new("wiki/log.md", "# Log"),
            CompileFile::new("wiki/concepts/中文.md", "# 中文"),
        ],
        deletions: vec![],
        summary: "compiled".into(),
    };
    assert!(CompileService::validate_manifest(&valid).is_ok());

    let escaping = CompileManifest {
        files: vec![CompileFile::new("../outside.md", "bad")],
        deletions: vec![],
        summary: "bad".into(),
    };
    assert_eq!(
        CompileService::validate_manifest(&escaping)
            .unwrap_err()
            .code,
        "COMPILE_PATH_INVALID"
    );
}

#[test]
fn compile_route_contract_distinguishes_preference_from_resolved_route() {
    assert_eq!(
        serde_json::to_string(&CompileRoutePreference::Auto).unwrap(),
        "\"auto\""
    );
    assert_eq!(
        serde_json::to_string(&CompileRoute::Byok).unwrap(),
        "\"byok\""
    );
}

#[test]
fn services_expose_replaceable_test_boundaries() {
    let agent = AgentService::with_runner(Arc::new(FakeProcessRunner));
    let detected = agent.detect_agents(Some(AgentKind::Claude));
    assert_eq!(detected[0].state, AgentDetectionState::Installed);
    assert!(detected[1..]
        .iter()
        .all(|info| info.state == AgentDetectionState::Missing));
    let _llm = LlmService::default();
    let _secret = SecretService::memory();
}

#[test]
fn compile_conflict_does_not_partially_modify_real_wiki() {
    let root = std::env::temp_dir().join(format!("task8-conflict-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("wiki")).unwrap();
    for (name, content) in [
        ("index.md", "index"),
        ("overview.md", "overview"),
        ("log.md", "log"),
    ] {
        std::fs::write(root.join("wiki").join(name), content).unwrap();
    }
    let context = ProjectContext::new("project", root.clone());
    let baseline = CompileService::snapshot_wiki(&context).unwrap();
    std::fs::write(root.join("wiki/index.md"), "external").unwrap();
    let manifest = CompileManifest {
        files: vec![
            CompileFile::new("wiki/index.md", "candidate"),
            CompileFile::new("wiki/overview.md", "candidate overview"),
            CompileFile::new("wiki/log.md", "candidate log"),
        ],
        deletions: vec![],
        summary: "compile".into(),
    };
    let result = CompileService::apply_manifest(&context, &manifest, &baseline).unwrap();
    assert_eq!(result.conflicts, vec!["wiki/index.md"]);
    assert_eq!(
        std::fs::read_to_string(root.join("wiki/overview.md")).unwrap(),
        "overview"
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn project_provider_and_agent_settings_round_trip_without_secret_material() {
    let root = std::env::temp_dir().join(format!("task8-settings-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("project", root.clone());

    AgentService::save_config(
        &context,
        &llm_wiki_desktop_lib::models::agent::AgentConfig {
            default_agent: Some(AgentKind::Codex),
        },
    )
    .unwrap();
    let loaded = AgentService::load_config(&context).unwrap();
    assert_eq!(loaded.default_agent, Some(AgentKind::Codex));

    let config = LlmProviderConfig {
        provider: LlmProviderKind::Anthropic,
        model: "claude-test".into(),
        base_url: "https://api.anthropic.com".into(),
        context_window: 100_000,
        enabled: true,
    };
    LlmService::save_provider(&context, config).unwrap();
    let raw = std::fs::read_to_string(root.join(".app/settings.json")).unwrap();
    assert!(raw.contains("claude-test"));
    assert!(!raw.contains("sk-ant-test"));
    assert_eq!(LlmService::list_providers(&context).unwrap().len(), 1);

    std::fs::remove_dir_all(root).ok();
}
