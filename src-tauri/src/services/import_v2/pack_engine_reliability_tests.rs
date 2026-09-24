//! Real lite RPC -> production adapter -> Source transaction in a disposable project.
use super::*;
use crate::services::import_v2::capability_pack::CapabilityPackManifest;
use crate::services::import_v2::{source_registry::SourceRegistry, ImportV2Service};
use crate::{
    models::{import_v2::*, paths::ProjectContext, task::TaskType},
    services::{FileStore, GitService, SecretService},
    tasks::TaskService,
};
use std::path::PathBuf;
use std::{
    io::Cursor,
    net::{IpAddr, Ipv4Addr, TcpListener},
};

struct RealLite {
    image_url: String,
}
impl ImportEngine for RealLite {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "test.real-lite-rpc".into(),
            engine_version: "source".into(),
            route: "web.generic.readability".into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::Url
    }
    fn execute(
        &self,
        request: &EngineRequest,
        token: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        let root = Path::new(&request.project_root).join(&request.staging_root);
        std::fs::create_dir_all(&root).unwrap();
        // Only acquisition is supplied by the test. Extraction and RPC bytes
        // come from the actual shipped JavaScript source, not a fake runner.
        std::fs::write(root.join("fetched.html"), format!("<html><head><title>Article fidelity</title></head><body><article><h1>Article fidelity</h1><p>Explain javascript: as ordinary prose. This short article preserves its meaningful illustration and all the words around it.</p><p>Before <img src='{}' alt='diagram'> after image.</p></article></body></html>", self.image_url)).unwrap();
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../capabilities/browser-runtime-lite/runner/index.mjs");
        let mut child = Command::new("node")
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let rpc = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: request.request_id.clone(),
            method: "import.extract".into(),
            params: request,
        };
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&rpc).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response = read_response(Cursor::new(output.stdout)).unwrap();
        response.rpc.validate(&request.request_id).unwrap();
        assert_eq!(response.remote_assets.len(), 1);
        let mut result = response.rpc.result.expect("actual lite article result");
        assert_eq!(
            result.text_coverage, None,
            "unknown coverage remains unknown"
        );
        let url = url::Url::parse(&self.image_url).unwrap();
        let grant = super::super::url_policy::PrivateTargetGrant {
            item_id: request.item_id.clone(),
            scheme: "http".into(),
            host: "127.0.0.1".into(),
            port: url.port().unwrap(),
            resolved_ips: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(2),
        };
        localize_remote_assets(
            request,
            &mut result,
            response.remote_assets,
            token,
            Arc::new(DomainLimiter::default()),
            Some(&grant),
            &|_| Ok(()),
        )?;
        assert_eq!(result.text_coverage, None);
        Ok(result)
    }
}
#[test]
fn real_lite_rpc_default_mode_retains_article_images_after_commit_and_restart() {
    use std::sync::atomic::{AtomicBool, Ordering};
    for available in [true, false] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let png =
            include_bytes!("../../../../tests/fixtures/import-v2/local/batch3/matrix/image.png")
                .to_vec();
        let expected = png.clone();
        let server = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut buffer = [0; 8192];
                        let _ = stream.read(&mut buffer);
                        let status = if available { "200 OK" } else { "404 Not Found" };
                        let body = if available {
                            png.as_slice()
                        } else {
                            b"missing"
                        };
                        write!(stream, "HTTP/1.1 {status}\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                        stream.write_all(body).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("lite-fidelity", dir.path().to_path_buf());
        std::fs::create_dir_all(context.root.join(".app")).unwrap();
        let service = ImportV2Service::with_secret_service(SecretService::memory());
        service
            .register_engine(Arc::new(RealLite {
                image_url: format!("http://127.0.0.1:{port}/diagram.png"),
            }))
            .unwrap();
        let session = service
            .create_session(&context, &FileStore, ImportResourceMode::Balanced)
            .unwrap();
        let target = UrlPolicy
            .normalize_for_session("https://example.com/article")
            .unwrap();
        let session = service
            .add_inputs(
                &context,
                &FileStore,
                &session.session_id,
                vec![ImportInput {
                    kind: ImportInputKind::Url,
                    display_name: "Article".into(),
                    locator: service.store_web_target(&target).unwrap(),
                    normalized_locator: Some(target.public.public_url),
                    source_identity: None,
                    media_save_mode: MediaSaveMode::ExtractOnly,
                }],
            )
            .unwrap();
        let tasks = TaskService::default();
        let task = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                context.root.clone(),
                "real lite".into(),
                true,
            )
            .unwrap();
        let prepared = service
            .run_item(
                &context,
                &FileStore,
                &tasks,
                &session.session_id,
                &session.items[0].item_id,
                &task.id,
            )
            .unwrap();
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
        assert_eq!(
            prepared.status,
            ImportItemStatus::PreviewReady,
            "{:?}",
            prepared.issue
        );
        let preview = prepared.preview.unwrap();
        if !available {
            assert!(!preview.quality.warnings.is_empty());
        }
        let batch = service
            .commit_items(
                &context,
                &FileStore,
                &GitService,
                &CommitImportSessionRequest {
                    project_id: context.project_id.clone(),
                    project_root_path: context.root.to_string_lossy().into(),
                    session_id: session.session_id.clone(),
                    batch_task_id: None,
                    acknowledge_restricted_content: false,
                    expected_selection_revision: None,
                    expected_confirmation_digest: None,
                    decisions: vec![CommitItemDecision {
                        item_id: prepared.item_id,
                        resolution: preview.resolution.and_then(|r| r.default_resolution),
                    }],
                },
            )
            .unwrap();
        assert_eq!(batch.committed_count, 1, "{batch:?}");
        drop(service);
        let restarted = ImportV2Service::with_secret_service(SecretService::memory());
        assert_eq!(
            restarted
                .load_session(&context, &FileStore, &session.session_id)
                .unwrap()
                .items[0]
                .status,
            ImportItemStatus::Completed
        );
        let wiki_path = batch.items[0].wiki_path.as_ref().unwrap();
        let body = std::fs::read_to_string(context.root.join(wiki_path)).unwrap();
        assert!(body.contains("Explain javascript:"));
        assert!(
            body.contains("Before") && body.contains("after image."),
            "{body}"
        );
        assert!(!body.contains("asset://"));
        let images = super::super::quality_gate::image_destinations(&body);
        if available {
            assert_eq!(images.len(), 1, "{body}");
            let path = SourceRegistry::resolve_wiki_asset_path(
                &context, &FileStore, wiki_path, &images[0],
            )
            .unwrap();
            assert_eq!(std::fs::read(path).unwrap(), expected);
        } else {
            assert!(images.is_empty(), "{body}");
        }
    }
}

#[test]
fn markitdown_snapshot_guard_rejects_old_runner_overwrite_and_preserves_utf8_original() {
    let project = tempfile::tempdir().unwrap();
    let staging = project.path().join("staging");
    std::fs::create_dir_all(&staging).unwrap();
    let original = b"Original document: access_token=keep-original-bytes";
    let (hash, size) = super::super::artifact::hash_reader(&mut original.as_slice()).unwrap();
    let request: EngineRequest = serde_json::from_value(serde_json::json!({
        "protocolVersion": "2", "requestId": "snapshot", "sessionId": "session", "itemId": "item", "taskId": "task",
        "operation": "extract", "projectRoot": project.path(), "stagingRoot": "staging", "chainedInput": "converted.docx",
        "input": {"kind": "file", "displayName": "原件.doc", "locator": "原件.doc", "normalizedLocator": null,
            "sourceIdentity": {"canonicalPath": "原件.doc", "sizeBytes": size, "sha256": hash, "magic": "ole"}}
    })).unwrap();
    let result: EngineResult = serde_json::from_value(serde_json::json!({
        "sourceSnapshotPath": "source.bin", "markdownPath": "document.md", "assetPaths": [], "title": "Converted",
        "textCoverage": null, "tableCellAccuracy": null, "warnings": []
    })).unwrap();
    std::fs::write(staging.join("document.md"), "# Converted\n\nReadable body.").unwrap();
    std::fs::write(staging.join("source.bin"), b"converted OOXML bytes").unwrap();
    assert_eq!(
        validate_document_snapshot(&request, &result)
            .unwrap_err()
            .code,
        "IMPORT_FILE_SOURCE_CHANGED"
    );
    std::fs::write(staging.join("source.bin"), original).unwrap();
    validate_document_snapshot(&request, &result).unwrap();
    sanitize_capability_text_artifacts(&request, &result).unwrap();
    assert_eq!(std::fs::read(staging.join("source.bin")).unwrap(), original);
    validate_document_snapshot(&request, &result).unwrap();
}

struct LegacySnapshotJourney {
    route: &'static str,
    original_hash: String,
    seen: Arc<std::sync::Mutex<Vec<String>>>,
}
impl ImportEngine for LegacySnapshotJourney {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: format!("test.snapshot.{}", self.route),
            engine_version: "fixture".into(),
            route: self.route.into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File
    }
    fn execute(
        &self,
        request: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        let identity = request
            .input
            .source_identity
            .as_ref()
            .expect("materialization must preserve the original identity");
        assert_eq!(identity.sha256, self.original_hash);
        let root = Path::new(&request.project_root).join(&request.staging_root);
        let authorized = Path::new(&request.project_root).join(&request.input.locator);
        assert!(authorized.starts_with(&root));
        assert_eq!(
            identity.canonical_path,
            authorized.canonicalize().unwrap().to_string_lossy()
        );
        self.seen.lock().unwrap().push(self.route.into());
        if self.route == "office.modern.docx" {
            assert!(request.chained_input.is_some());
            return Err(BackendError::new(
                "IMPORT_FILE_PARSE_FAILED",
                "Controlled modern reader failure",
                true,
                false,
            ));
        }
        if self.route == "pack.office-legacy" {
            std::fs::create_dir_all(root.join("converted")).unwrap();
            std::fs::copy(&authorized, root.join("source.bin")).unwrap();
            std::fs::write(
                root.join("converted/source.docx"),
                include_bytes!(
                    "../../../../tests/fixtures/import-v2/local/batch3/matrix/document.docx"
                ),
            )
            .unwrap();
        } else {
            assert_eq!(
                request.chained_input.as_deref(),
                Some("converted/source.docx")
            );
            assert!(root.join("converted/source.docx").is_file());
        }
        std::fs::write(root.join("document.md"), "# Converted legacy document\n\nThe original bytes survive conversion, failed native parsing, and fallback extraction.\n").unwrap();
        let result: EngineResult = serde_json::from_value(serde_json::json!({
            "sourceSnapshotPath": "source.bin", "markdownPath": "document.md",
            "assetPaths": if self.route == "pack.office-legacy" { vec!["converted/source.docx"] } else { vec![] },
            "title": "Converted legacy document", "textCoverage": 1.0, "tableCellAccuracy": null, "warnings": []
        })).unwrap();
        if self.route == "pack.markitdown" {
            validate_document_snapshot(request, &result)?;
        }
        Ok(result)
    }
}

#[test]
fn legacy_conversion_native_failure_fallback_preserves_original_identity_through_commit() {
    let project = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("legacy-snapshot", project.path().to_path_buf());
    std::fs::create_dir_all(context.root.join(".app")).unwrap();
    let original =
        include_bytes!("../../../../tests/fixtures/import-v2/local/batch3/matrix/legacy.doc");
    let (original_hash, _) = super::super::artifact::hash_reader(&mut original.as_slice()).unwrap();
    let input_directory = tempfile::tempdir().unwrap();
    let source = input_directory.path().join("旧原件.doc");
    std::fs::write(&source, original).unwrap();
    let scan = super::super::file_discovery::FileDiscoveryService
        .scan(
            &context,
            std::slice::from_ref(&source),
            crate::models::import_v2_file::FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    let identity = scan
        .files
        .first()
        .expect("legacy sample must be discoverable")
        .source_identity
        .clone();
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    for route in [
        "pack.office-legacy",
        "office.modern.docx",
        "pack.markitdown",
    ] {
        service
            .register_engine(Arc::new(LegacySnapshotJourney {
                route,
                original_hash: original_hash.clone(),
                seen: seen.clone(),
            }))
            .unwrap();
    }
    let session = service
        .create_session(&context, &FileStore, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &FileStore,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::File,
                display_name: "旧原件.doc".into(),
                locator: source.to_string_lossy().into(),
                normalized_locator: Some(format!("file:{}", identity.canonical_path)),
                source_identity: Some(identity),
                media_save_mode: MediaSaveMode::ExtractOnly,
            }],
        )
        .unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            context.root.clone(),
            "legacy fallback".into(),
            true,
        )
        .unwrap();
    let item = service
        .run_item(
            &context,
            &FileStore,
            &tasks,
            &session.session_id,
            &session.items[0].item_id,
            &task.id,
        )
        .unwrap();
    assert_eq!(
        item.status,
        ImportItemStatus::PreviewReady,
        "{:?}",
        item.issue
    );
    assert_eq!(
        *seen.lock().unwrap(),
        [
            "pack.office-legacy",
            "office.modern.docx",
            "pack.markitdown"
        ]
    );
    assert_eq!(
        item.preview.as_ref().unwrap().source_snapshot.sha256,
        original_hash
    );
    let batch = service
        .commit_items(
            &context,
            &FileStore,
            &GitService,
            &CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: context.root.to_string_lossy().into(),
                session_id: session.session_id.clone(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                expected_selection_revision: None,
                expected_confirmation_digest: None,
                decisions: vec![CommitItemDecision {
                    item_id: item.item_id,
                    resolution: item
                        .preview
                        .unwrap()
                        .resolution
                        .and_then(|r| r.default_resolution),
                }],
            },
        )
        .unwrap();
    assert_eq!(batch.committed_count, 1, "{batch:?}");
    let paths = context.layout.source_paths().unwrap();
    let manifest = SourceRegistry::read_manifest(
        &context,
        &FileStore,
        &paths
            .manifest(batch.items[0].source_id.as_ref().unwrap())
            .unwrap(),
    )
    .unwrap();
    let snapshot = manifest.versions[0]
        .raw_evidence
        .iter()
        .find(|r| r.kind == "source_snapshot")
        .unwrap();
    assert_eq!(snapshot.sha256, original_hash);
    assert_eq!(
        std::fs::read(context.root.join(&snapshot.path)).unwrap(),
        original
    );
    assert_eq!(std::fs::read(source).unwrap(), original);
    assert!(
        std::fs::read_to_string(context.root.join(manifest.wiki_path))
            .unwrap()
            .contains("original bytes survive")
    );
}

/// Source runner + available real Chromium + production PackProcessEngine,
/// with an isolated cookie profile and an HTTP server that rejects anonymous
/// article requests. No mock engine or anonymous prefetch is involved.
#[test]
#[ignore = "requires LLM_WIKI_TEST_BROWSER_ROOT built by prepare-local-acceptance.mjs"]
fn real_browser_authenticated_profile_reaches_preview_commit_and_restart_without_anonymous_fetch() {
    real_browser_authenticated_journey(false);
    real_browser_authenticated_journey(true);
}

fn real_browser_authenticated_journey(replay_session_cookie: bool) {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let pack_root = PathBuf::from(std::env::var("LLM_WIKI_TEST_BROWSER_ROOT").unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let authenticated = Arc::new(AtomicUsize::new(0));
    let anonymous = Arc::new(AtomicUsize::new(0));
    let flags = (stop.clone(), authenticated.clone(), anonymous.clone());
    let server = std::thread::spawn(move || {
        while !flags.0.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut buffer = [0; 8192];
                    let size = stream.read(&mut buffer).unwrap_or(0);
                    if size == 0 {
                        continue;
                    }
                    let request = String::from_utf8_lossy(&buffer[..size]);
                    let has_cookie = request.contains("acceptance_session=authenticated");
                    if has_cookie {
                        flags.1.fetch_add(1, Ordering::SeqCst);
                    } else {
                        flags.2.fetch_add(1, Ordering::SeqCst);
                    }
                    let body = if has_cookie {
                        "<html><head><title>Authenticated article</title></head><body><article><h1>Authenticated article</h1><p>登录后的实际正文。This article discusses captcha and login required as ordinary prose, with faithful readable body.</p></article></body></html>"
                    } else {
                        "<form id='challenge-form'>login required</form>"
                    };
                    let status = if has_cookie {
                        "200 OK"
                    } else {
                        "401 Unauthorized"
                    };
                    let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                Err(error) => panic!("{error}"),
            }
        }
    });
    let temp = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("browser-auth-fidelity", temp.path().join("知识库"));
    std::fs::create_dir_all(context.root.join(".app")).unwrap();
    let profile = temp.path().join("isolated-profile");
    let url = if replay_session_cookie {
        "https://mp.weixin.qq.com/s/acceptance".into()
    } else {
        format!("http://127.0.0.1:{port}/article")
    };
    let seeded = Command::new(pack_root.join("node"))
        .arg(pack_root.join("runner/acceptance-seed.mjs"))
        .arg(&profile)
        .arg(&url)
        .output()
        .unwrap();
    assert!(
        seeded.status.success(),
        "{}",
        String::from_utf8_lossy(&seeded.stderr)
    );
    let secrets = SecretService::memory();
    if replay_session_cookie {
        secrets.set_account("connector-cookie:wechat", &serde_json::json!([{
            "name": "wxuin", "value": "authenticated", "domain": "mp.weixin.qq.com", "path": "/", "expires": -1, "httpOnly": true, "secure": true
        }]).to_string()).unwrap();
    }
    let service = ImportV2Service::with_secret_service(secrets);
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(pack_root.join("manifest.json")).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("browserRevision");
    let mut manifest: CapabilityPackManifest = serde_json::from_value(json).unwrap();
    if replay_session_cookie {
        let fixture_path = pack_root.join("acceptance/host-cookie.json");
        std::fs::create_dir_all(fixture_path.parent().unwrap()).unwrap();
        std::fs::write(&fixture_path, serde_json::json!({
            "requireCookie": "wxuin",
            "html": "<meta charset='utf-8'><h1 id='activity-name'>Session cookie article</h1><div id='js_content'><p>登录后的实际正文。Discussion of captcha and login required.</p></div>"
        }).to_string()).unwrap();
        manifest.entrypoint_args = vec![
            "runner/acceptance-fixture-entry.mjs".into(),
            "acceptance/host-cookie.json".into(),
        ];
    }
    let entrypoint = pack_root.join("node").canonicalize().unwrap();
    service
        .register_capability_pack(
            ResolvedCapabilityPack {
                manifest,
                root: pack_root.canonicalize().unwrap(),
                entrypoint_sha256: format!(
                    "{:x}",
                    Sha256::digest(std::fs::read(&entrypoint).unwrap())
                ),
                entrypoint,
            },
            "web.generic.browser".into(),
            vec![],
            Duration::from_secs(60),
        )
        .unwrap();
    let session = service
        .create_session(&context, &FileStore, ImportResourceMode::Balanced)
        .unwrap();
    let target = UrlPolicy.normalize_for_session(&url).unwrap();
    let session = service
        .add_inputs(
            &context,
            &FileStore,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "Authenticated article".into(),
                locator: service.store_web_target(&target).unwrap(),
                normalized_locator: Some(target.public.public_url),
                source_identity: None,
                media_save_mode: MediaSaveMode::ExtractOnly,
            }],
        )
        .unwrap();
    let item_id = session.items[0].item_id.clone();
    let mut waiting = session.items[0].clone();
    waiting.status = ImportItemStatus::WaitingLogin;
    service
        .sessions
        .write_item(&context, &FileStore, &session.session_id, &waiting)
        .unwrap();
    service
        .bind_authenticated_profiles(
            &context.project_id,
            &session.session_id,
            std::slice::from_ref(&item_id),
            &profile,
        )
        .unwrap();
    service
        .mark_authenticated_login_group(
            &context,
            &FileStore,
            &session.session_id,
            std::slice::from_ref(&item_id),
            Some("Fixture account"),
        )
        .unwrap();
    service
        .authorize_private_target(super::super::url_policy::PrivateTargetGrant {
            item_id: item_id.clone(),
            scheme: "http".into(),
            host: "127.0.0.1".into(),
            port,
            resolved_ips: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        })
        .unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            context.root.clone(),
            "real browser".into(),
            true,
        )
        .unwrap();
    let prepared = service
        .run_item(
            &context,
            &FileStore,
            &tasks,
            &session.session_id,
            &item_id,
            &task.id,
        )
        .unwrap();
    stop.store(true, Ordering::SeqCst);
    server.join().unwrap();
    assert_eq!(
        prepared.status,
        ImportItemStatus::PreviewReady,
        "{:?}",
        prepared.issue
    );
    if !replay_session_cookie {
        assert!(authenticated.load(Ordering::SeqCst) >= 1);
    }
    assert_eq!(
        anonymous.load(Ordering::SeqCst),
        0,
        "authenticated recovery must not prefetch anonymously"
    );
    let preview = prepared.preview.unwrap();
    let batch = service
        .commit_items(
            &context,
            &FileStore,
            &GitService,
            &CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: context.root.to_string_lossy().into(),
                session_id: session.session_id.clone(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                expected_selection_revision: None,
                expected_confirmation_digest: None,
                decisions: vec![CommitItemDecision {
                    item_id,
                    resolution: preview.resolution.and_then(|r| r.default_resolution),
                }],
            },
        )
        .unwrap();
    assert_eq!(batch.committed_count, 1, "{batch:?}");
    drop(service);
    let restarted = ImportV2Service::with_secret_service(SecretService::memory());
    assert_eq!(
        restarted
            .load_session(&context, &FileStore, &session.session_id)
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::Completed
    );
    let body = std::fs::read_to_string(
        context
            .root
            .join(batch.items[0].wiki_path.as_ref().unwrap()),
    )
    .unwrap();
    assert!(body.contains("登录后的实际正文。"));
    assert!(body.contains("captcha and login required"));
    assert!(!body.contains("acceptance_session"));
    assert!(!body.contains("wxuin"));
    let persisted = serde_json::to_string(
        &restarted
            .load_session(&context, &FileStore, &session.session_id)
            .unwrap(),
    )
    .unwrap();
    assert!(!persisted.contains("cookieBackup"));
    assert!(!persisted.contains("wxuin"));
}

#[test]
fn browser_rpc_subtitle_kind_never_promotes_translation_or_unknown_to_source() {
    for (kind, reliable) in [
        (Some("author_original"), true),
        (Some("platform_auto_original"), true),
        (Some("author_other"), true),
        (Some("machine_translation"), false),
        (Some("unknown"), false),
        (None, false),
    ] {
        let mut notification = serde_json::json!({
            "jsonrpc": "2.0", "method": "import.remoteAsset", "params": {
                "placeholder": "platform-subtitle-0", "url": "https://sns-subtitle-s2.xhscdn.com/source.srt",
                "kind": "subtitle", "automatic": true, "language": "zh-CN", "label": "source"
            }
        });
        if let Some(kind) = kind {
            notification["params"]["subtitleKind"] = kind.into();
        }
        let response = serde_json::json!({ "jsonrpc": "2.0", "id": "r1", "error": null,
            "result": {"sourceSnapshotPath": "source.html", "markdownPath": "candidate.md", "assetPaths": [], "title": "Video", "warnings": []} });
        let rpc = format!("{notification}\n{response}\n");
        let parsed = read_response(Cursor::new(rpc)).unwrap();
        assert_eq!(parsed.remote_assets.len(), 1);
        let asset = &parsed.remote_assets[0];
        assert_eq!(asset.subtitle_kind.as_deref(), kind);
        assert_eq!(asset.language.as_deref(), Some("zh-CN"));
        assert_eq!(
            subtitle_kind(asset).is_some_and(|kind| kind.is_reliable_source()),
            reliable
        );
    }
}

#[test]
#[ignore = "requires real Chromium qualification RPC under LLM_WIKI_TEST_BROWSER_ROOT"]
fn real_browser_subtitle_notifications_keep_reliability_through_host_adapter() {
    let root = PathBuf::from(std::env::var("LLM_WIKI_TEST_BROWSER_ROOT").unwrap());
    for (name, kind, reliable) in [
        ("xhs-original", "platform_auto_original", true),
        ("xhs-translation", "machine_translation", false),
    ] {
        let bytes = std::fs::read(root.join("acceptance").join(name).join("rpc.jsonl")).unwrap();
        let parsed = read_response(Cursor::new(bytes)).unwrap();
        parsed.rpc.validate("r1").unwrap();
        let asset = parsed
            .remote_assets
            .iter()
            .find(|asset| asset.kind == "subtitle")
            .unwrap();
        assert_eq!(asset.subtitle_kind.as_deref(), Some(kind));
        assert_eq!(
            subtitle_kind(asset).is_some_and(|kind| kind.is_reliable_source()),
            reliable
        );
    }
}
