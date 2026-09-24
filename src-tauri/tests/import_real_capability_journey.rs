//! Explicit opt-in live acceptance; ordinary CI never downloads models.
use llm_wiki_desktop_lib::{
    models::{import_v2::*, import_v2_file::FileScanPolicy, paths::ProjectContext, task::TaskType},
    services::{
        import_v2::{
            capability_installer::{catalog_entry, install_catalog_entry},
            capability_runtime::{target_triple, ImportCapabilityRuntime},
            file_discovery::{new_import_inputs, FileDiscoveryService},
            ImportV2Service,
        },
        BlockingWorkCoordinator, FileStore, GitService, SecretService,
    },
    tasks::{task_model::CancellationToken, TaskService},
};
use std::{fs, path::PathBuf, time::Instant};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads and executes the signed native ASR/OCR runner; provide IMPORT_ACCEPTANCE_ROOT and IMPORT_ACCEPTANCE_INPUT"]
async fn real_preparation_recognition_preview_save_and_reopen() {
    let root = PathBuf::from(
        std::env::var("IMPORT_ACCEPTANCE_ROOT").expect("disposable acceptance directory"),
    );
    let input = PathBuf::from(
        std::env::var("IMPORT_ACCEPTANCE_INPUT").expect("real speech or PDF/image sample"),
    );
    let capability =
        std::env::var("IMPORT_ACCEPTANCE_CAPABILITY").unwrap_or("asr-sensevoice-small".into());
    let ocr = capability.starts_with("ocr-");
    let project = root.join(format!("knowledge-base-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(project.join(".app")).unwrap();
    let context = ProjectContext::new("real-acceptance", project.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    let tasks = TaskService::default();
    let runtime = ImportCapabilityRuntime::default();
    let installed = root.join("installed-capabilities");
    let entry = catalog_entry(&capability, &target_triple()).expect("prepared development catalog");
    let token = CancellationToken::default();
    let started = Instant::now();
    let mut download_bytes = 0;
    let development = std::env::var("IMPORT_ACCEPTANCE_DEV_ROOT")
        .ok()
        .map(PathBuf::from);
    if let Some(development) = development {
        #[cfg(debug_assertions)]
        assert!(runtime.load_development(
            &development.join("installed"),
            &development.join("development-public-key.hex"),
            &service
        ));
        #[cfg(not(debug_assertions))]
        panic!(
            "development acceptance requires a debug build: {}",
            development.display()
        );
    } else if !installed.join(&capability).join(&entry.version).exists() {
        let mut outcome = install_catalog_entry(
            &BlockingWorkCoordinator::default(),
            &installed,
            &entry,
            "real-acceptance",
            &token,
            |_, current, _| download_bytes = current,
        )
        .await
        .unwrap();
        let probed = runtime
            .probe_version_routes(&installed, &capability, &entry.version, &token)
            .unwrap();
        runtime
            .activate_probed_version_atomically(probed, &capability, &service, || {
                outcome.activate(&installed)
            })
            .unwrap();
    } else {
        // An installed release must remain usable with no download service.
        let mut offline_entry = entry.clone();
        offline_entry.url =
            "https://example.invalid/no-download-for-an-installed-release.zip".into();
        let mut outcome = install_catalog_entry(
            &BlockingWorkCoordinator::default(),
            &installed,
            &offline_entry,
            "real-offline-reuse",
            &token,
            |_, current, _| download_bytes = current,
        )
        .await
        .unwrap();
        assert_eq!(
            download_bytes, 0,
            "an unchanged installed release must not download again"
        );
        let probed = runtime
            .probe_version_routes(&installed, &capability, &entry.version, &token)
            .unwrap();
        runtime
            .activate_probed_version_atomically(probed, &capability, &service, || {
                outcome.activate(&installed)
            })
            .unwrap();
    }
    let preparation_ms = started.elapsed().as_millis();
    assert!(
        runtime
            .statuses()
            .iter()
            .any(|s| s.capability_id == capability && s.available),
        "{:?}",
        runtime.statuses()
    );
    let scan = FileDiscoveryService
        .scan(
            &context,
            &[input],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            new_import_inputs(&session, scan.files),
        )
        .unwrap();
    let id = &session.items[0].item_id;
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            project.clone(),
            "Recognition intent".into(),
            true,
        )
        .unwrap();
    let waiting = service
        .run_item(&context, &files, &tasks, &session.session_id, id, &task.id)
        .unwrap();
    assert!(
        matches!(
            waiting.status,
            ImportItemStatus::WaitingAuthorization
                | ImportItemStatus::WaitingCapability
                | ImportItemStatus::PreviewReady
        ),
        "{waiting:?}"
    );
    service
        .authorize_media_for_session(
            &context,
            &files,
            &session.session_id,
            id,
            if ocr {
                ImportMediaAuthorizationKind::Ocr
            } else {
                ImportMediaAuthorizationKind::Asr
            },
            (!ocr).then_some(ImportAsrProfile::Balanced),
            None,
        )
        .unwrap();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            project.clone(),
            "Real extraction".into(),
            true,
        )
        .unwrap();
    let recognition_start = Instant::now();
    let prepared = service
        .run_item_with_recovery(
            &context,
            &files,
            &tasks,
            &session.session_id,
            id,
            &task.id,
            Some(if ocr {
                &ImportRecoveryAction::EnableOcr
            } else {
                &ImportRecoveryAction::AuthorizeLocalAsr
            }),
        )
        .unwrap();
    assert_eq!(
        prepared.status,
        ImportItemStatus::PreviewReady,
        "{prepared:?}"
    );
    let recognition_ms = recognition_start.elapsed().as_millis();
    let preview = prepared.preview.unwrap();
    let result = service
        .commit_items(
            &context,
            &files,
            &GitService,
            &CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: project.to_string_lossy().into(),
                session_id: session.session_id.clone(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                expected_selection_revision: None,
                expected_confirmation_digest: None,
                decisions: vec![CommitItemDecision {
                    item_id: id.clone(),
                    resolution: preview.resolution.and_then(|r| r.default_resolution),
                }],
            },
        )
        .unwrap();
    assert_eq!(result.committed_count, 1, "{result:?}");
    drop(service);
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    assert_eq!(
        service
            .load_session(&context, &files, &session.session_id)
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::Completed
    );
    let article_path = project.join(result.items[0].wiki_path.as_ref().unwrap());
    let article = fs::read_to_string(&article_path).unwrap();
    if ocr {
        assert!(
            !article.contains("Mean confidence:"),
            "OCR diagnostics leaked into the article"
        );
        assert!(
            !article.contains("# Local OCR evidence"),
            "Raw OCR report replaced the article"
        );
        assert!(
            !article.contains("Coordinates:"),
            "OCR coordinates leaked into the article"
        );
    }
    eprintln!("REAL_ARTICLE path={} preparation_ms={preparation_ms} recognition_ms={recognition_ms} download_bytes={download_bytes}\n{article}", article_path.display());
}

#[test]
#[ignore = "executes local Node and FFmpeg; set LLM_WIKI_TEST_NODE and LLM_WIKI_TEST_FFMPEG"]
fn real_decoder_without_asr_model_commits_subtitles_and_reopens_original_evidence() {
    use llm_wiki_desktop_lib::services::import_v2::capability_pack::{
        CapabilityPackManifest, ResolvedCapabilityPack,
    };
    use sha2::{Digest, Sha256};
    let node = PathBuf::from(std::env::var("LLM_WIKI_TEST_NODE").unwrap());
    let ffmpeg = PathBuf::from(std::env::var("LLM_WIKI_TEST_FFMPEG").unwrap());
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("临时知识库");
    fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("decoder-acceptance", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    let tasks = TaskService::default();
    let pack_root = temporary.path().join("decoder-pack");
    fs::create_dir_all(pack_root.join("runner")).unwrap();
    fs::create_dir_all(pack_root.join("runtime/ffmpeg/bin")).unwrap();
    let source_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../capabilities/media-runtime");
    for file in ["index.mjs", "core.mjs", "video-frames.mjs"] {
        fs::copy(
            source_root.join("runner").join(file),
            pack_root.join("runner").join(file),
        )
        .unwrap();
    }
    let node_name = if cfg!(windows) { "node.exe" } else { "node" };
    let ffmpeg_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    fs::copy(node, pack_root.join(node_name)).unwrap();
    fs::copy(
        &ffmpeg,
        pack_root.join("runtime/ffmpeg/bin").join(ffmpeg_name),
    )
    .unwrap();
    let mut manifest_json: serde_json::Value =
        serde_json::from_slice(&fs::read(source_root.join("manifest.json")).unwrap()).unwrap();
    for field in [
        "payloadStatus",
        "buildProvenance",
        "execution",
        "distributionNote",
    ] {
        manifest_json.as_object_mut().unwrap().remove(field);
    }
    let mut manifest: CapabilityPackManifest = serde_json::from_value(manifest_json).unwrap();
    manifest.entrypoint = node_name.into();
    manifest.entrypoint_args = vec!["runner/index.mjs".into()];
    let entrypoint = pack_root.join(node_name).canonicalize().unwrap();
    service
        .register_capability_pack(
            ResolvedCapabilityPack {
                manifest,
                root: pack_root.canonicalize().unwrap(),
                entrypoint_sha256: format!("{:x}", Sha256::digest(fs::read(&entrypoint).unwrap())),
                entrypoint,
            },
            "media.embedded-subtitle".into(),
            vec!["mp4".into(), "mkv".into()],
            std::time::Duration::from_secs(60),
        )
        .unwrap();
    assert!(!service
        .registered_engine_routes()
        .unwrap()
        .contains(&"media.asr".into()));
    let subtitle = temporary.path().join("字幕.srt");
    fs::write(&subtitle, "1\n00:00:00,000 --> 00:00:01,000\n2026 [中文可靠字幕]\n\n2\n00:00:01,000 --> 00:00:02,000\n42\n").unwrap();
    let input = temporary.path().join("影片.mp4");
    let status = std::process::Command::new(&ffmpeg)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=16x16:rate=1:duration=2",
            "-i",
        ])
        .arg(&subtitle)
        .args([
            "-map", "0", "-map", "1", "-c:v", "mpeg4", "-c:s", "mov_text",
        ])
        .arg(&input)
        .status()
        .unwrap();
    assert!(status.success());
    let original = fs::read(&input).unwrap();
    let scan = FileDiscoveryService
        .scan(
            &context,
            &[input],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            new_import_inputs(&session, scan.files),
        )
        .unwrap();
    let item_id = session.items[0].item_id.clone();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            root.clone(),
            "Decoder only".into(),
            true,
        )
        .unwrap();
    let prepared = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item_id,
            &task.id,
        )
        .unwrap();
    assert_eq!(
        prepared.status,
        ImportItemStatus::PreviewReady,
        "{prepared:?}"
    );
    let preview = prepared.preview.unwrap();
    GitService
        .initialize_repository(&context, "Acceptance fixture")
        .unwrap();
    let committed = service
        .commit_items(
            &context,
            &files,
            &GitService,
            &CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: root.to_string_lossy().into_owned(),
                session_id: session.session_id.clone(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                expected_selection_revision: None,
                expected_confirmation_digest: None,
                decisions: vec![CommitItemDecision {
                    item_id,
                    resolution: preview
                        .resolution
                        .and_then(|resolution| resolution.default_resolution),
                }],
            },
        )
        .unwrap();
    assert_eq!(committed.committed_count, 1, "{committed:?}");
    let article =
        fs::read_to_string(root.join(committed.items[0].wiki_path.as_ref().unwrap())).unwrap();
    assert!(article.contains("2026 [中文可靠字幕]"), "{article}");
    assert!(article.contains("42"), "{article}");
    drop(service);
    let reopened = ImportV2Service::with_secret_service(SecretService::memory());
    assert_eq!(
        reopened
            .load_session(&context, &files, &session.session_id)
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::Completed
    );
    fn all_files(root: &std::path::Path) -> Vec<PathBuf> {
        fs::read_dir(root)
            .unwrap()
            .flat_map(|entry| {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    all_files(&path)
                } else {
                    vec![path]
                }
            })
            .collect()
    }
    let persisted = all_files(&root.join("raw"));
    assert!(
        persisted
            .iter()
            .any(|path| fs::read(path).unwrap() == original),
        "original media bytes missing"
    );
    assert!(
        persisted.iter().any(
            |path| path.file_name().is_some_and(|name| name == "embedded.srt")
                && fs::read_to_string(path)
                    .unwrap()
                    .contains("2026 [中文可靠字幕]")
        ),
        "original subtitle evidence missing"
    );
}

#[test]
#[ignore = "reuses a completed real long-audio run; set LLM_WIKI_TEST_SENSEVOICE_ROOT to its isolated pack/staging directory"]
fn real_long_sensevoice_runner_cache_commits_late_speech_and_reopens() {
    use llm_wiki_desktop_lib::services::import_v2::capability_pack::{
        CapabilityPackManifest, ResolvedCapabilityPack,
    };
    use sha2::{Digest, Sha256};
    let acceptance = PathBuf::from(std::env::var("LLM_WIKI_TEST_SENSEVOICE_ROOT").unwrap());
    let pack_root = acceptance.join("pack").canonicalize().unwrap();
    let manifest: CapabilityPackManifest =
        serde_json::from_slice(&fs::read(pack_root.join("manifest.json")).unwrap()).unwrap();
    let entrypoint = pack_root.join(&manifest.entrypoint).canonicalize().unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("long-audio-knowledge-base");
    fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("long-speech-acceptance", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    let tasks = TaskService::default();
    service
        .register_capability_pack(
            ResolvedCapabilityPack {
                manifest,
                root: pack_root,
                entrypoint_sha256: format!("{:x}", Sha256::digest(fs::read(&entrypoint).unwrap())),
                entrypoint,
            },
            "media.asr".into(),
            vec!["flac".into()],
            std::time::Duration::from_secs(120),
        )
        .unwrap();
    let input = acceptance.join("staging/input.flac");
    let original = fs::read(&input).unwrap();
    let scan = FileDiscoveryService
        .scan(
            &context,
            &[input],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            new_import_inputs(&session, scan.files),
        )
        .unwrap();
    let item_id = session.items[0].item_id.clone();
    let waiting_task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            root.clone(),
            "Inspect long media".into(),
            true,
        )
        .unwrap();
    let waiting = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item_id,
            &waiting_task.id,
        )
        .unwrap();
    assert!(
        matches!(
            waiting.status,
            ImportItemStatus::WaitingAuthorization | ImportItemStatus::WaitingCapability
        ),
        "{waiting:?}"
    );
    service
        .authorize_media_for_session(
            &context,
            &files,
            &session.session_id,
            &item_id,
            ImportMediaAuthorizationKind::Asr,
            Some(ImportAsrProfile::Balanced),
            Some("zh".into()),
        )
        .unwrap();
    let staging = root.join(format!(
        ".app/import-sessions/{}/items/{}/staging",
        session.session_id, item_id
    ));
    fn copy_tree(source: &std::path::Path, destination: &std::path::Path) {
        fs::create_dir_all(destination).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if entry.path().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    // These are actual completed model shards, never synthetic transcripts.
    copy_tree(
        &acceptance.join("staging/asr-shards"),
        &staging.join("asr-shards"),
    );
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            root.clone(),
            "Late speech".into(),
            true,
        )
        .unwrap();
    let prepared = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item_id,
            &task.id,
        )
        .unwrap();
    assert_eq!(
        prepared.status,
        ImportItemStatus::PreviewReady,
        "{prepared:?}"
    );
    let preview = prepared.preview.unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(
        &fs::read(staging.join("transcripts/local-asr.metadata.json")).unwrap(),
    )
    .unwrap();
    let late = metadata["segments"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|segment| {
            segment["startMs"]
                .as_u64()
                .is_some_and(|start| start > 7_200_000)
        })
        .map(|segment| segment["text"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(!late.is_empty(), "late speech absent: {metadata}");
    let expected = late.join(" ");
    assert!(
        (expected.contains("九点") || expected.contains("9点"))
            && (expected.contains("五点") || expected.contains("5点")),
        "known qualification speech missing: {expected}"
    );
    GitService
        .initialize_repository(&context, "Acceptance fixture")
        .unwrap();
    let committed = service
        .commit_items(
            &context,
            &files,
            &GitService,
            &CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: root.to_string_lossy().into_owned(),
                session_id: session.session_id.clone(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                expected_selection_revision: None,
                expected_confirmation_digest: None,
                decisions: vec![CommitItemDecision {
                    item_id,
                    resolution: preview
                        .resolution
                        .and_then(|resolution| resolution.default_resolution),
                }],
            },
        )
        .unwrap();
    assert_eq!(committed.committed_count, 1, "{committed:?}");
    let article =
        fs::read_to_string(root.join(committed.items[0].wiki_path.as_ref().unwrap())).unwrap();
    for text in late {
        assert!(
            article.contains(text),
            "committed late speech missing: {text}"
        );
    }
    assert!(article.contains("02:00:"), "late anchor missing: {article}");
    drop(service);
    let reopened = ImportV2Service::with_secret_service(SecretService::memory());
    assert_eq!(
        reopened
            .load_session(&context, &files, &session.session_id)
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::Completed
    );
    fn contains_bytes(root: &std::path::Path, expected: &[u8]) -> bool {
        fs::read_dir(root).unwrap().any(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                contains_bytes(&path, expected)
            } else {
                fs::read(path).unwrap() == expected
            }
        })
    }
    assert!(
        contains_bytes(&root.join("raw"), &original),
        "original media bytes missing"
    );
}

#[test]
#[ignore = "requires isolated real OCR pack, Node, FFmpeg and two-scene video; set LLM_WIKI_TEST_OCR_ROOT and LLM_WIKI_TEST_VIDEO_INPUT"]
fn real_video_frames_ocr_preview_commit_and_reopen() {
    use llm_wiki_desktop_lib::services::import_v2::capability_pack::{
        CapabilityPackManifest, ResolvedCapabilityPack,
    };
    use sha2::{Digest, Sha256};
    let node = PathBuf::from(std::env::var("LLM_WIKI_TEST_NODE").unwrap());
    let ffmpeg = PathBuf::from(std::env::var("LLM_WIKI_TEST_FFMPEG").unwrap());
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("临时知识库");
    fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("decoder-acceptance", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    let tasks = TaskService::default();
    let pack_root = temporary.path().join("decoder-pack");
    fs::create_dir_all(pack_root.join("runner")).unwrap();
    fs::create_dir_all(pack_root.join("runtime/ffmpeg/bin")).unwrap();
    let source_root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../capabilities/media-runtime");
    for file in ["index.mjs", "core.mjs", "video-frames.mjs"] {
        fs::copy(
            source_root.join("runner").join(file),
            pack_root.join("runner").join(file),
        )
        .unwrap();
    }
    let node_name = if cfg!(windows) { "node.exe" } else { "node" };
    let ffmpeg_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    fs::copy(node, pack_root.join(node_name)).unwrap();
    fs::copy(
        &ffmpeg,
        pack_root.join("runtime/ffmpeg/bin").join(ffmpeg_name),
    )
    .unwrap();
    let mut manifest_json: serde_json::Value =
        serde_json::from_slice(&fs::read(source_root.join("manifest.json")).unwrap()).unwrap();
    for field in [
        "payloadStatus",
        "buildProvenance",
        "execution",
        "distributionNote",
    ] {
        manifest_json.as_object_mut().unwrap().remove(field);
    }
    let mut manifest: CapabilityPackManifest = serde_json::from_value(manifest_json).unwrap();
    manifest.entrypoint = node_name.into();
    manifest.entrypoint_args = vec!["runner/index.mjs".into()];
    let entrypoint = pack_root.join(node_name).canonicalize().unwrap();
    // Prefer the actual verify-install output for resource qualification. The
    // source-runner fixture remains useful when only Node/FFmpeg are available.
    let (manifest, pack_root, entrypoint) =
        if let Ok(root) = std::env::var("LLM_WIKI_TEST_MEDIA_ROOT") {
            let root = PathBuf::from(root).canonicalize().unwrap();
            let manifest: CapabilityPackManifest =
                serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
            let entrypoint = root.join(&manifest.entrypoint).canonicalize().unwrap();
            (manifest, root, entrypoint)
        } else {
            (manifest, pack_root, entrypoint)
        };
    let asr_root = std::env::var("LLM_WIKI_TEST_ASR_ROOT")
        .ok()
        .map(PathBuf::from);
    if let Some(asr_root) = asr_root.as_ref() {
        let asr_root = asr_root.canonicalize().unwrap();
        let asr_manifest: CapabilityPackManifest =
            serde_json::from_slice(&fs::read(asr_root.join("manifest.json")).unwrap()).unwrap();
        let asr_entrypoint = asr_root
            .join(&asr_manifest.entrypoint)
            .canonicalize()
            .unwrap();
        service
            .register_capability_pack(
                ResolvedCapabilityPack {
                    manifest: asr_manifest,
                    root: asr_root,
                    entrypoint: asr_entrypoint.clone(),
                    entrypoint_sha256: format!(
                        "{:x}",
                        Sha256::digest(fs::read(&asr_entrypoint).unwrap())
                    ),
                },
                "media.asr".into(),
                vec!["mp4".into()],
                std::time::Duration::from_secs(180),
            )
            .unwrap();
    } else {
        service
            .register_capability_pack(
                ResolvedCapabilityPack {
                    manifest,
                    root: pack_root.canonicalize().unwrap(),
                    entrypoint_sha256: format!(
                        "{:x}",
                        Sha256::digest(fs::read(&entrypoint).unwrap())
                    ),
                    entrypoint,
                },
                "media.keyframes".into(),
                vec!["mp4".into(), "mkv".into(), "gif".into()],
                std::time::Duration::from_secs(60),
            )
            .unwrap();
        assert!(!service
            .registered_engine_routes()
            .unwrap()
            .contains(&"media.asr".into()));
    }
    let ocr_root = PathBuf::from(std::env::var("LLM_WIKI_TEST_OCR_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let ocr_manifest: CapabilityPackManifest =
        serde_json::from_slice(&fs::read(ocr_root.join("manifest.json")).unwrap()).unwrap();
    let ocr_entrypoint = ocr_root
        .join(&ocr_manifest.entrypoint)
        .canonicalize()
        .unwrap();
    service
        .register_capability_pack(
            ResolvedCapabilityPack {
                manifest: ocr_manifest,
                root: ocr_root,
                entrypoint: ocr_entrypoint.clone(),
                entrypoint_sha256: format!(
                    "{:x}",
                    Sha256::digest(fs::read(&ocr_entrypoint).unwrap())
                ),
            },
            "ocr.cjk-accurate".into(),
            vec!["png".into()],
            std::time::Duration::from_secs(180),
        )
        .unwrap();
    let input = PathBuf::from(std::env::var("LLM_WIKI_TEST_VIDEO_INPUT").unwrap());
    let original = fs::read(&input).unwrap();
    let scan = FileDiscoveryService
        .scan(
            &context,
            &[input],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            new_import_inputs(&session, scan.files),
        )
        .unwrap();
    let item_id = session.items[0].item_id.clone();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            root.clone(),
            "Decoder only".into(),
            true,
        )
        .unwrap();
    let prepared = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item_id,
            &task.id,
        )
        .unwrap();
    assert!(
        matches!(
            prepared.status,
            ImportItemStatus::WaitingAuthorization | ImportItemStatus::WaitingCapability
        ),
        "{prepared:?}"
    );
    if asr_root.is_some() {
        service
            .authorize_media_for_session(
                &context,
                &files,
                &session.session_id,
                &item_id,
                ImportMediaAuthorizationKind::Asr,
                Some(ImportAsrProfile::Balanced),
                None,
            )
            .unwrap();
        let task = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                root.clone(),
                "Real no-audio ASR".into(),
                true,
            )
            .unwrap();
        let waiting = service
            .run_item(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &item_id,
                &task.id,
            )
            .unwrap();
        assert_eq!(
            waiting.status,
            ImportItemStatus::WaitingAuthorization,
            "{waiting:?}"
        );
        assert_eq!(
            waiting.issue.as_ref().unwrap().code,
            "IMPORT_VIDEO_FRAME_OCR_REQUIRED",
            "{waiting:?}"
        );
    }
    service
        .authorize_media_for_session(
            &context,
            &files,
            &session.session_id,
            &item_id,
            ImportMediaAuthorizationKind::Ocr,
            None,
            None,
        )
        .unwrap();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            root.clone(),
            "Real frame OCR".into(),
            true,
        )
        .unwrap();
    let prepared = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item_id,
            &task.id,
        )
        .unwrap();
    assert_eq!(
        prepared.status,
        ImportItemStatus::PreviewReady,
        "{prepared:?}"
    );
    let preview = prepared.preview.unwrap();
    GitService
        .initialize_repository(&context, "Acceptance fixture")
        .unwrap();
    let committed = service
        .commit_items(
            &context,
            &files,
            &GitService,
            &CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: root.to_string_lossy().into_owned(),
                session_id: session.session_id.clone(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                expected_selection_revision: None,
                expected_confirmation_digest: None,
                decisions: vec![CommitItemDecision {
                    item_id,
                    resolution: preview
                        .resolution
                        .and_then(|resolution| resolution.default_resolution),
                }],
            },
        )
        .unwrap();
    assert_eq!(committed.committed_count, 1, "{committed:?}");
    let article =
        fs::read_to_string(root.join(committed.items[0].wiki_path.as_ref().unwrap())).unwrap();
    assert!(article.contains("OPENING"), "{article}");
    assert!(article.contains("LATE"), "{article}");
    assert!(
        article.contains("[00:00:") && article.contains("[00:00:3"),
        "{article}"
    );
    assert!(!article.contains("OCR_IMAGE_"), "{article}");
    let wiki_path = committed.items[0].wiki_path.as_ref().unwrap();
    let destinations: Vec<_> = article
        .lines()
        .filter_map(|line| {
            line.strip_prefix("![Video frame](")
                .and_then(|value| value.strip_suffix(')'))
        })
        .collect();
    assert_eq!(destinations.len(), 2, "{article}");
    for image in destinations {
        let resolved = llm_wiki_desktop_lib::services::import_v2::source_registry::SourceRegistry::resolve_wiki_asset_path(&context, &files, wiki_path, image).unwrap();
        assert!(fs::read(resolved).unwrap().starts_with(b"\x89PNG"));
    }
    drop(service);
    let reopened = ImportV2Service::with_secret_service(SecretService::memory());
    assert_eq!(
        reopened
            .load_session(&context, &files, &session.session_id)
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::Completed
    );
    fn all_files(root: &std::path::Path) -> Vec<PathBuf> {
        fs::read_dir(root)
            .unwrap()
            .flat_map(|entry| {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    all_files(&path)
                } else {
                    vec![path]
                }
            })
            .collect()
    }
    let persisted = all_files(&root.join("raw"));
    assert!(
        persisted
            .iter()
            .any(|path| fs::read(path).unwrap() == original),
        "original media bytes missing"
    );
    assert!(
        persisted
            .iter()
            .any(|path| path.file_name().is_some_and(|name| name == "frames.json")),
        "time evidence missing"
    );
}
