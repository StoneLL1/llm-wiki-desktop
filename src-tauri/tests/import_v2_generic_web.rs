use std::{
    fs,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, TcpListener},
    sync::Arc,
};

use llm_wiki_desktop_lib::{
    models::import_v2::{ImportInput, ImportInputKind, MediaSaveMode},
    services::{
        import_v2::{
            engine::{EngineOperation, EngineRequest, ImportEngine},
            generic_web_engine::GenericWebEngine,
            url_policy::{PrivateTargetGrant, UrlPolicy},
            web_target_store::WebTargetStore,
        },
        SecretService,
    },
    tasks::task_model::CancellationToken,
};
#[test]
fn lite_pack_is_offline_locked_and_fail_closed() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let runner =
        fs::read_to_string(root.join("capabilities/browser-runtime-lite/runner/index.mjs"))
            .unwrap();
    assert!(
        runner.contains("Readability")
            && runner.contains("DOMPurify")
            && runner.contains("Turndown")
    );
    assert!(!runner.contains("fetch(") && !runner.contains("http.request"));
    let manifest =
        fs::read_to_string(root.join("capabilities/browser-runtime-lite/manifest.json")).unwrap();
    assert!(manifest.contains("\"targetTriples\":[]"));
}

#[test]
fn generic_engine_consumes_an_item_bound_private_target_grant() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 2048];
        let _ = stream.read(&mut request);
        let body = "<html><title>Private fixture</title><article>Private fixture body with enough readable text.</article></html>";
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let targets = Arc::new(WebTargetStore::new(SecretService::memory()));
    let url = format!("http://127.0.0.1:{port}/article");
    let target = UrlPolicy.normalize_for_session(&url).unwrap();
    let locator = targets.store(&target).unwrap();
    targets
        .authorize_private(PrivateTargetGrant {
            item_id: "private-item".into(),
            scheme: "http".into(),
            host: "127.0.0.1".into(),
            port,
            resolved_ips: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(1),
        })
        .unwrap();
    targets
        .claim_private_for_operation("private-item", "task")
        .unwrap();
    let engine = GenericWebEngine::new(targets, "builtin.web-http", "web.generic.readability");
    let result = engine
        .execute(
            &EngineRequest {
                protocol_version: "2".into(),
                request_id: "private-request".into(),
                project_id: "project".into(),
                session_id: "session".into(),
                item_id: "private-item".into(),
                task_id: "task".into(),
                operation: EngineOperation::Extract,
                input: ImportInput {
                    kind: ImportInputKind::Url,
                    display_name: target.public.host,
                    locator,
                    normalized_locator: Some(target.public.public_url),
                    source_identity: None,
                    media_save_mode: MediaSaveMode::ExtractOnly,
                },
                project_root: root.path().to_string_lossy().into_owned(),
                staging_root: "staging/private-item".into(),
                chained_input: None,
                local_asr_authorized: false,
                asr_probe_only: false,
                asr_profile: None,
                recognition_language: None,
                selected_subtitle: None,
                local_ocr_authorized: false,
                media_save_mode: MediaSaveMode::ExtractOnly,
            },
            &CancellationToken::new(),
        )
        .unwrap();

    server.join().unwrap();
    assert_eq!(result.title, "Private fixture");
    assert!(result.text_coverage.is_some_and(|coverage| coverage > 0.0));
}
