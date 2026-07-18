use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
};

use llm_wiki_desktop_lib::{
    models::{
        import_v2::{
            ImportInput, ImportInputKind, ImportItem, ImportItemStatus, ImportResourceMode,
            ImportSession,
        },
        import_v2_agent::{AgentAssistanceTrigger, AgentAuditRecord, AgentRecoveryAction},
        llm::{LlmProviderConfig, LlmProviderKind},
        paths::ProjectContext,
    },
    services::{
        import_v2::{
            agent_assistance::AgentAssistanceService, agent_candidate::AgentCandidateService,
            ImportV2Service, SessionStore,
        },
        AgentService, FileStore, LlmService, SecretService, SettingsService,
    },
    tasks::TaskService,
};

#[test]
fn byok_scope_is_exact_expiring_and_one_shot() {
    let (port, captured, server) = byok_server();
    let root = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("project", root.path().to_path_buf());
    let files = FileStore;
    let imports = ImportV2Service::default();
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    settings
        .save_provider(
            &context,
            LlmProviderConfig {
                provider: LlmProviderKind::OpenAi,
                model: "gpt-test".into(),
                base_url: format!("http://127.0.0.1:{port}"),
                context_window: 8_192,
                enabled: true,
            },
        )
        .unwrap();
    let agents = AgentService::default();
    let tasks = TaskService::default();
    let mut session = ImportSession::new("session-a", "project", ImportResourceMode::Balanced);
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
    item.issue = Some(llm_wiki_desktop_lib::models::import_v2::ImportIssue {
        code: "IMPORT_FILE_PARSE_FAILED".into(),
        message: "failed".into(),
        stage: llm_wiki_desktop_lib::models::import_v2::ImportStage::Extract,
        retryable: true,
        user_action_required: true,
        recovery_actions: Vec::new(),
        available_actions: vec![AgentRecoveryAction::RequestByok],
    });
    session.items.push(item);
    SessionStore::default()
        .save(&context, &files, &session)
        .unwrap();
    let staging = root
        .path()
        .join(".app/import-sessions/session-a/items/item-a/staging");
    std::fs::create_dir_all(&staging).unwrap();
    std::fs::write(
        staging.join("source.bin"),
        b"send exactly this\napi_key=source-secret-value",
    )
    .unwrap();
    let service = AgentAssistanceService::new(
        &imports,
        &files,
        &settings,
        &agents,
        &tasks,
        AgentAssistanceService::bundled_skill_path(),
    );

    let scope = service
        .preview_byok_scope(
            &context,
            "session-a",
            "item-a",
            AgentAssistanceTrigger::Manual,
            LlmProviderKind::OpenAi,
        )
        .unwrap();
    assert_eq!(scope.model, "gpt-test");
    assert_eq!(scope.files.len(), 1);
    assert_eq!(scope.destination, format!("http://127.0.0.1:{port}"));
    assert!(scope.estimated_cost_micros.is_some());
    assert!(!scope.files[0].redactions.is_empty());
    assert!(scope.expires_at > chrono::Utc::now());
    assert!(service
        .start_byok(
            &context,
            "session-a",
            "item-a",
            AgentAssistanceTrigger::Manual,
            LlmProviderKind::OpenAi,
            &scope.model,
            &scope.approval_id,
            "wrong-scope",
            false,
        )
        .is_err());
    settings
        .save_provider(
            &context,
            LlmProviderConfig {
                provider: LlmProviderKind::OpenAi,
                model: "gpt-test".into(),
                base_url: "http://127.0.0.1:9".into(),
                context_window: 8_192,
                enabled: true,
            },
        )
        .unwrap();
    let changed = service
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
    assert_eq!(changed.code, "IMPORT_BYOK_DESTINATION_CHANGED");
    settings
        .save_provider(
            &context,
            LlmProviderConfig {
                provider: LlmProviderKind::OpenAi,
                model: "gpt-test".into(),
                base_url: format!("http://127.0.0.1:{port}"),
                context_window: 8_192,
                enabled: true,
            },
        )
        .unwrap();
    assert!(service
        .start_byok(
            &context,
            "session-a",
            "wrong-item",
            AgentAssistanceTrigger::Manual,
            LlmProviderKind::OpenAi,
            &scope.model,
            &scope.approval_id,
            &scope.scope_sha256,
            false,
        )
        .is_err());
    let task = service
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
        .unwrap();
    assert!(service
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
        .is_err());
    assert_eq!(task.project_id.as_deref(), Some("project"));

    let secrets = SecretService::memory();
    secrets
        .set(LlmProviderKind::OpenAi, "sk-must-never-persist")
        .unwrap();
    let serialized = serde_json::to_string(&scope).unwrap();
    assert!(!serialized.contains("sk-must-never-persist"));
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(service.run_byok(
            &context,
            "session-a",
            "item-a",
            &task.id,
            AgentAssistanceTrigger::Manual,
            LlmProviderKind::OpenAi,
            &LlmService,
            &secrets,
        ))
        .unwrap();
    server.join().unwrap();
    let request = captured.lock().unwrap().clone();
    assert!(request.contains("Bearer sk-must-never-persist"));
    assert!(request.contains("send exactly this"));
    assert!(!request.contains("source-secret-value"));
    assert!(!tree_contains(root.path(), "sk-must-never-persist"));
    let audit: AgentAuditRecord = files
        .read_json(
            &context,
            &format!(
                ".app/import-sessions/session-a/items/item-a/agent-audit/{}.json",
                task.id
            ),
        )
        .unwrap();
    assert_eq!(audit.task_id, task.id);
    assert_eq!(audit.agent_kind, None);
    assert_eq!(audit.agent_version, "gpt-test");
    assert_eq!(audit.prompt_template_version, "wiki-ingest-assist/byok-v1");
    assert_eq!(audit.approved_cost_micros, scope.estimated_cost_micros);
    assert_eq!(audit.byok_provider.as_deref(), Some("open_ai"));
    assert_eq!(
        audit.byok_destination.as_deref(),
        Some(scope.destination.as_str())
    );
    assert_eq!(audit.outcome, "succeeded");
    assert_eq!(audit.output_hashes.len(), 1);

    let candidate = AgentCandidateService::new(&imports, &files, &tasks)
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .unwrap();
    assert_eq!(candidate.audit_id, audit.audit_id);

    let affected = tasks
        .get_task(&task.id)
        .unwrap()
        .result
        .unwrap()
        .affected_paths[0]
        .clone();
    std::fs::write(
        root.path().join(affected).join("candidate.md"),
        "tampered after response",
    )
    .unwrap();
    assert!(AgentCandidateService::new(&imports, &files, &tasks)
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .is_err());
    let retry = service
        .preview_byok_scope(
            &context,
            "session-a",
            "item-a",
            AgentAssistanceTrigger::Manual,
            LlmProviderKind::OpenAi,
        )
        .unwrap();
    assert!(retry.requires_duplicate_charge_acknowledgement);
    assert_eq!(
        service
            .start_byok(
                &context,
                "session-a",
                "item-a",
                AgentAssistanceTrigger::Manual,
                LlmProviderKind::OpenAi,
                &retry.model,
                &retry.approval_id,
                &retry.scope_sha256,
                false,
            )
            .unwrap_err()
            .code,
        "IMPORT_BYOK_DUPLICATE_CHARGE_ACK_REQUIRED"
    );
}

#[test]
fn commands_expose_preview_and_approval_without_automatic_byok() {
    let source = include_str!("../src/commands/import_v2_agent_commands.rs");
    assert!(source.contains("preview_import_byok_scope_v2"));
    assert!(source.contains("approve_import_byok_assistance_v2"));
    assert!(!source.contains("auto_byok &&"));
}

#[test]
fn cancelled_provider_call_does_not_open_a_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let config = LlmProviderConfig {
        provider: LlmProviderKind::OpenAi,
        model: "gpt-test".into(),
        base_url: format!("http://{}", listener.local_addr().unwrap()),
        context_window: 8_192,
        enabled: true,
    };
    let error = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(LlmService.complete_streaming(
            &config,
            Some("sk-never-sent"),
            "prompt",
            || true,
            |_| {},
        ))
        .unwrap_err();
    assert_eq!(error.code, "LLM_CANCELLED");
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(matches!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    ));
}

fn byok_server() -> (u16, Arc<Mutex<String>>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let captured = Arc::new(Mutex::new(String::new()));
    let output = Arc::clone(&captured);
    let join = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..header_end + 4]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(str::trim)
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= header_end + 4 + length {
                    break;
                }
            }
        }
        *output.lock().unwrap() = String::from_utf8_lossy(&bytes).into_owned();
        let body = "data: {\"choices\":[{\"delta\":{\"content\":\"# Approved candidate\"}}]}\n\ndata: [DONE]\n\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(), body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    (port, captured, join)
}

fn tree_contains(root: &std::path::Path, needle: &str) -> bool {
    std::fs::read_dir(root).unwrap().flatten().any(|entry| {
        let path = entry.path();
        if path.is_dir() {
            tree_contains(&path, needle)
        } else {
            std::fs::read(&path)
                .ok()
                .is_some_and(|bytes| String::from_utf8_lossy(&bytes).contains(needle))
        }
    })
}
