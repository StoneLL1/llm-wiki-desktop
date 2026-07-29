use llm_wiki_desktop_lib::{
    errors::BackendError,
    models::{
        import_v2::{
            ImportAsrProfile, ImportInput, ImportInputKind, ImportItemStatus,
            ImportMediaAuthorizationKind, ImportRecoveryAction, ImportResourceMode,
        },
        import_v2_web::WebImportErrorCode,
        paths::ProjectContext,
        task::{TaskStatus, TaskType},
    },
    services::{
        import_v2::{
            domain_router::{ConnectorAvailability, DomainRouter},
            engine::{
                EngineContinuation, EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
            },
            url_policy::UrlPolicy,
            web_target_store::{asr_target_sha256, BilibiliAsrGrant},
            ImportV2Service,
        },
        FileStore, SecretService,
    },
    tasks::{task_model::CancellationToken, TaskService},
};
use std::sync::Arc;

const SILENCE_WAV: &[u8] =
    include_bytes!("../../tests/fixtures/import-v2/local/batch3/silence.wav");

#[test]
fn stage_one_routes_and_stable_errors_are_frozen() {
    let a = ConnectorAvailability {
        browser: true,
        wechat: true,
        zhihu: true,
        bilibili: true,
        xiaohongshu: true,
        douyin: true,
        phase_two: false,
    };
    for host in ["mp.weixin.qq.com", "www.zhihu.com", "www.bilibili.com"] {
        let t = UrlPolicy
            .normalize_for_session(&format!("https://{host}/one?token=secret#frag"))
            .unwrap();
        let p = DomainRouter::plan(&t.public, &a);
        assert!(p.release_enabled);
        assert!(!serde_json::to_string(&t.public).unwrap().contains("secret"));
    }
    assert_eq!(
        serde_json::to_value(WebImportErrorCode::PrivateTargetBlocked).unwrap(),
        "private_target_blocked"
    );
    assert_eq!(
        serde_json::to_value(WebImportErrorCode::ContentRemoved).unwrap(),
        "content_removed"
    );
}

struct AuthorizedMediaPlatformEngine {
    route: &'static str,
}
impl ImportEngine for AuthorizedMediaPlatformEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "bili-fixture".into(),
            engine_version: "1".into(),
            route: self.route.into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::Url
    }
    fn execute(
        &self,
        request: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        if !request.local_asr_authorized {
            return Err(BackendError::new(
                "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
                "subtitle missing",
                true,
                true,
            ));
        }
        let staging = std::path::Path::new(&request.project_root).join(&request.staging_root);
        std::fs::create_dir_all(staging.join(".asr-input-fixture")).unwrap();
        std::fs::write(staging.join(".asr-input-fixture/input.wav"), SILENCE_WAV).unwrap();
        std::fs::write(staging.join("source.json"), b"{}").unwrap();
        std::fs::write(staging.join("candidate.md"), b"# Video\n\nMetadata\n").unwrap();
        Ok(EngineResult {
            source_snapshot_path: "source.json".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: vec![],
            metadata_path: None,
            title: "Video".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: Some(EngineContinuation::LocalAsr {
                temporary_input_path: ".asr-input-fixture/input.wav".into(),
                media_kind: "audio".into(),
            }),
            warnings: vec![],
        })
    }
}

#[test]
fn bilibili_without_subtitles_cannot_generate_a_committable_preview() {
    let root = std::env::temp_dir().join(format!("web-no-transcript-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("p", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    service
        .register_engine(Arc::new(AuthorizedMediaPlatformEngine {
            route: "web.bilibili.video",
        }))
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let exact = "https://www.bilibili.com/video/BV1NoTranscript";
    let target = UrlPolicy.normalize_for_session(exact).unwrap();
    let reference = service.store_web_target(&target).unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "bili".into(),
                locator: reference,
                normalized_locator: Some(target.public.public_url),
                source_identity: None,
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "preview".into(),
            true,
        )
        .unwrap();
    let waiting = service
        .run_item_with_recovery(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &session.items[0].item_id,
            &task.id,
            None,
        )
        .unwrap();
    assert_eq!(waiting.status, ImportItemStatus::WaitingCapability);
    assert!(waiting.preview.is_none());
    let recovered = service
        .load_session(&context, &files, &session.session_id)
        .unwrap();
    let item = &recovered.items[0];
    assert_eq!(item.status, ImportItemStatus::WaitingCapability);
    assert!(item.preview.is_none());
    assert!(item
        .issue
        .as_ref()
        .is_some_and(|issue| issue.code == "IMPORT_WEB_SUBTITLE_UNAVAILABLE"));
    assert!(!root.join("raw").exists());
    assert!(!root.join("wiki/sources").exists());
    std::fs::remove_dir_all(root).ok();
}

struct LocalAsrEngine;
impl ImportEngine for LocalAsrEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "asr-fixture".into(),
            engine_version: "1.8.3".into(),
            route: "media.asr".into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File && input.locator.ends_with(".wav")
    }
    fn execute(
        &self,
        request: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        assert!(request.local_asr_authorized);
        assert_eq!(
            request.chained_input.as_deref(),
            Some(".asr-input-fixture/input.wav")
        );
        assert!(!std::path::Path::new(request.chained_input.as_deref().unwrap()).is_absolute());
        let staging = std::path::Path::new(&request.project_root).join(&request.staging_root);
        std::fs::create_dir_all(staging.join(".sensevoice-output-fixture")).unwrap();
        std::fs::write(
            staging.join(".sensevoice-output-fixture/source.json"),
            b"{}",
        )
        .unwrap();
        std::fs::write(
            staging.join(".sensevoice-output-fixture/candidate.md"),
            b"- [00:00:00.000] authorized transcript\n",
        )
        .unwrap();
        Ok(EngineResult {
            source_snapshot_path: ".sensevoice-output-fixture/source.json".into(),
            markdown_path: ".sensevoice-output-fixture/candidate.md".into(),
            asset_paths: vec![],
            metadata_path: None,
            title: "Transcript".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: None,
            warnings: vec![],
        })
    }
}

struct NoSpeechAsrEngine;
impl ImportEngine for NoSpeechAsrEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "sensevoice-no-speech-fixture".into(),
            engine_version: "1".into(),
            route: "media.asr".into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File && input.locator.ends_with(".wav")
    }
    fn execute(
        &self,
        request: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        assert!(request.local_asr_authorized);
        Err(BackendError::new(
            "IMPORT_ASR_NO_SPEECH",
            "No speech was detected.",
            false,
            true,
        ))
    }
}

struct SubtitleFallbackEngine;
impl ImportEngine for SubtitleFallbackEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "subtitle-fallback-fixture".into(),
            engine_version: "1".into(),
            route: "web.bilibili.metadata".into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::Url
    }
    fn execute(
        &self,
        request: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        let staging = std::path::Path::new(&request.project_root).join(&request.staging_root);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("source.json"), b"{}").unwrap();
        std::fs::write(
            staging.join("candidate.md"),
            b"# Video\n\n- [00:00:01.000] platform subtitle\n",
        )
        .unwrap();
        Ok(EngineResult {
            source_snapshot_path: "source.json".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: vec![],
            metadata_path: None,
            title: "Video".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: None,
            warnings: vec![],
        })
    }
}

struct InvalidBilibiliFallbackEngine;
impl ImportEngine for InvalidBilibiliFallbackEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "invalid-bilibili-fallback-fixture".into(),
            engine_version: "1".into(),
            route: "web.bilibili.metadata".into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::Url
    }

    fn execute(
        &self,
        _: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        Err(BackendError::new(
            "IMPORT_V2_ENGINE_OUTPUT_INVALID",
            "Later fallback could not produce a candidate.",
            true,
            true,
        ))
    }
}

#[test]
fn subtitle_unavailable_route_falls_through_to_the_next_bilibili_provider() {
    let root = std::env::temp_dir().join(format!("web-bili-fallback-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("p", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    service
        .register_engine(Arc::new(AuthorizedMediaPlatformEngine {
            route: "web.bilibili.video",
        }))
        .unwrap();
    service
        .register_engine(Arc::new(SubtitleFallbackEngine))
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let target = UrlPolicy
        .normalize_for_session("https://www.bilibili.com/video/BV1Fallback")
        .unwrap();
    let reference = service.store_web_target(&target).unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "bili".into(),
                locator: reference,
                normalized_locator: Some(target.public.public_url),
                source_identity: None,
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "fallback".into(),
            true,
        )
        .unwrap();
    let item = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &session.items[0].item_id,
            &task.id,
        )
        .unwrap();
    assert_eq!(item.status, ImportItemStatus::PreviewReady);
    assert!(item
        .attempts
        .iter()
        .any(|attempt| attempt.route == "web.bilibili.metadata"));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn subtitle_recovery_survives_a_later_bilibili_fallback_failure() {
    let root = std::env::temp_dir().join(format!(
        "web-bili-recovery-priority-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("p", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    service
        .register_engine(Arc::new(AuthorizedMediaPlatformEngine {
            route: "web.bilibili.video",
        }))
        .unwrap();
    service
        .register_engine(Arc::new(InvalidBilibiliFallbackEngine))
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let target = UrlPolicy
        .normalize_for_session("https://www.bilibili.com/video/BV1Recovery")
        .unwrap();
    let reference = service.store_web_target(&target).unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "bili".into(),
                locator: reference,
                normalized_locator: Some(target.public.public_url),
                source_identity: None,
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "recovery priority".into(),
            true,
        )
        .unwrap();
    let item = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &session.items[0].item_id,
            &task.id,
        )
        .unwrap();

    assert_eq!(item.status, ImportItemStatus::WaitingCapability);
    assert_eq!(
        item.issue.as_ref().map(|issue| issue.code.as_str()),
        Some("IMPORT_WEB_SUBTITLE_UNAVAILABLE")
    );
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn bilibili_without_subtitles_waits_for_explicit_asr_authorization_and_cleans_temp() {
    assert_platform_authorized_asr(
        "web.bilibili.metadata",
        "https://www.bilibili.com/video/BV1Ab411c7de?token=ephemeral",
    );
}

#[test]
fn authorized_audio_with_no_speech_creates_no_source_or_raw() {
    assert!(!SILENCE_WAV.is_empty());
    let root = std::env::temp_dir().join(format!("web-asr-no-speech-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("p", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    service
        .register_engine(Arc::new(AuthorizedMediaPlatformEngine {
            route: "web.bilibili.metadata",
        }))
        .unwrap();
    service
        .register_engine(Arc::new(NoSpeechAsrEngine))
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let target = UrlPolicy
        .normalize_for_session("https://www.bilibili.com/video/BV1NoSpeech")
        .unwrap();
    let reference = service.store_web_target(&target).unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "no speech".into(),
                locator: reference,
                normalized_locator: Some(target.public.public_url.clone()),
                source_identity: None,
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let item_id = session.items[0].item_id.clone();
    let tasks = TaskService::default();
    let first = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "wait for ASR".into(),
            true,
        )
        .unwrap();
    assert_eq!(
        service
            .run_item(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &item_id,
                &first.id,
            )
            .unwrap()
            .status,
        ImportItemStatus::WaitingAuthorization
    );
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
    let second = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "authorized ASR".into(),
            true,
        )
        .unwrap();
    let error = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item_id,
            &second.id,
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_ASR_NO_SPEECH");
    assert_eq!(
        service
            .load_session(&context, &files, &session.session_id)
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::Failed
    );
    assert!(!root.join("raw").exists());
    assert!(!root.join("wiki").exists());
    assert!(!root.join(".app/sources").exists());
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn installing_asr_resumes_waiting_capability_into_explicit_authorization() {
    let root = std::env::temp_dir().join(format!("web-install-asr-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("p", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    service
        .register_engine(Arc::new(AuthorizedMediaPlatformEngine {
            route: "web.bilibili.video",
        }))
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let exact = "https://www.bilibili.com/video/BV1InstallAsr";
    let target = UrlPolicy.normalize_for_session(exact).unwrap();
    let reference = service.store_web_target(&target).unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "bili".into(),
                locator: reference,
                normalized_locator: Some(target.public.public_url),
                source_identity: None,
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let item_id = session.items[0].item_id.clone();
    let tasks = TaskService::default();
    let first = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "missing".into(),
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
            &first.id,
        )
        .unwrap();
    assert_eq!(waiting.status, ImportItemStatus::WaitingCapability);

    service.register_engine(Arc::new(LocalAsrEngine)).unwrap();
    let resumed = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "resumed".into(),
            true,
        )
        .unwrap();
    service
        .bind_item_task_ids(
            &context,
            &files,
            &session.session_id,
            &[(item_id.clone(), resumed.id.clone())],
        )
        .unwrap();
    let waiting = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item_id,
            &resumed.id,
        )
        .unwrap();
    assert_eq!(waiting.status, ImportItemStatus::WaitingAuthorization);
    assert!(waiting
        .issue
        .unwrap()
        .recovery_actions
        .contains(&ImportRecoveryAction::AuthorizeLocalAsr));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn xiaohongshu_and_douyin_without_subtitles_wait_for_explicit_asr_authorization() {
    for (route, url) in [
        (
            "web.xiaohongshu.note",
            "https://www.xiaohongshu.com/explore/note-1?xsec_token=ephemeral",
        ),
        (
            "web.douyin.video",
            "https://www.douyin.com/video/1234567890?token=ephemeral",
        ),
    ] {
        assert_platform_authorized_asr(route, url);
    }
}

fn assert_platform_authorized_asr(route: &'static str, exact: &str) {
    let root = std::env::temp_dir().join(format!("web-asr-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("p", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    service
        .register_engine(Arc::new(AuthorizedMediaPlatformEngine { route }))
        .unwrap();
    service.register_engine(Arc::new(LocalAsrEngine)).unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let target = UrlPolicy.normalize_for_session(exact).unwrap();
    let reference = service.store_web_target(&target).unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "bili".into(),
                locator: reference,
                normalized_locator: Some(target.public.public_url.clone()),
                source_identity: None,
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let item = &session.items[0];
    let tasks = TaskService::default();
    let first = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "first".into(),
            true,
        )
        .unwrap();
    let waiting = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &first.id,
        )
        .unwrap();
    assert_eq!(waiting.status, ImportItemStatus::WaitingAuthorization);
    let recovery_actions = &waiting.issue.as_ref().unwrap().recovery_actions;
    assert!(recovery_actions.contains(&ImportRecoveryAction::AuthorizeLocalAsr));
    assert!(!recovery_actions.contains(&ImportRecoveryAction::InstallMediaCapability));
    assert!(!waiting
        .attempts
        .iter()
        .any(|attempt| attempt.route == "media.asr"));
    service
        .authorize_media_for_session(
            &context,
            &files,
            &session.session_id,
            &item.item_id,
            ImportMediaAuthorizationKind::Asr,
            Some(ImportAsrProfile::Balanced),
            None,
        )
        .unwrap();
    let authorized = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "authorized".into(),
            true,
        )
        .unwrap();
    let completed = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &authorized.id,
        )
        .unwrap();
    assert_eq!(completed.status, ImportItemStatus::PreviewReady);
    assert!(completed
        .attempts
        .iter()
        .any(|attempt| attempt.route == "media.asr"));
    let staging = root.join(format!(
        ".app/import-sessions/{}/items/{}/staging",
        session.session_id, item.item_id
    ));
    assert!(std::fs::read_to_string(staging.join("candidate.md"))
        .unwrap()
        .contains("authorized transcript"));
    assert!(!staging.join(".asr-input-fixture").exists());
    assert!(!staging.join(".sensevoice-output-fixture").exists());
    std::fs::remove_dir_all(root).ok();
}

struct TraversalContinuationEngine;
impl ImportEngine for TraversalContinuationEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "bili-traversal".into(),
            engine_version: "1".into(),
            route: "web.bilibili.metadata".into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::Url
    }
    fn execute(
        &self,
        request: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        let staging = std::path::Path::new(&request.project_root).join(&request.staging_root);
        std::fs::create_dir_all(staging.join(".asr-input-traversal")).unwrap();
        std::fs::write(staging.join(".asr-input-traversal/input.m4a"), b"audio").unwrap();
        std::fs::write(staging.join("source.json"), b"{}").unwrap();
        Ok(EngineResult {
            source_snapshot_path: "source.json".into(),
            markdown_path: "../../sentinel.md".into(),
            asset_paths: vec![],
            metadata_path: None,
            title: "Traversal".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: Some(EngineContinuation::LocalAsr {
                temporary_input_path: ".asr-input-traversal/input.m4a".into(),
                media_kind: "audio".into(),
            }),
            warnings: vec![],
        })
    }
}

#[test]
fn traversal_result_is_rejected_before_local_asr_can_read_or_overwrite_it() {
    let root = std::env::temp_dir().join(format!("web-asr-traversal-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("p", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    service
        .register_engine(Arc::new(TraversalContinuationEngine))
        .unwrap();
    service.register_engine(Arc::new(LocalAsrEngine)).unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let exact = "https://www.bilibili.com/video/BV1Traversal";
    let target = UrlPolicy.normalize_for_session(exact).unwrap();
    let reference = service.store_web_target(&target).unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "bili".into(),
                locator: reference,
                normalized_locator: Some(target.public.public_url.clone()),
                source_identity: None,
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let item = &session.items[0];
    service
        .authorize_bilibili_asr(BilibiliAsrGrant {
            project_id: "p".into(),
            session_id: session.session_id.clone(),
            item_id: item.item_id.clone(),
            target_sha256: asr_target_sha256(exact),
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
        })
        .unwrap();
    let staging = root.join(format!(
        ".app/import-sessions/{}/items/{}/staging",
        session.session_id, item.item_id
    ));
    std::fs::create_dir_all(&staging).unwrap();
    let sentinel = staging.join("../../sentinel.md");
    std::fs::write(&sentinel, "do not overwrite").unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "traversal".into(),
            true,
        )
        .unwrap();
    assert!(service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &task.id
        )
        .is_err());
    assert_eq!(
        std::fs::read_to_string(&sentinel).unwrap(),
        "do not overwrite"
    );
    let recovered = service
        .load_session(&context, &files, &session.session_id)
        .unwrap();
    assert!(!recovered.items[0]
        .attempts
        .iter()
        .any(|attempt| attempt.route == "media.asr"));
    assert!(service
        .take_bilibili_asr("p", &session.session_id, &item.item_id, exact)
        .unwrap()
        .is_some());
    std::fs::remove_dir_all(root).ok();
}
#[test]
fn media_platforms_are_released_while_unapproved_platforms_stay_closed() {
    let a = ConnectorAvailability {
        browser: true,
        wechat: true,
        zhihu: true,
        bilibili: true,
        xiaohongshu: false,
        douyin: false,
        phase_two: false,
    };
    let xhs = UrlPolicy
        .normalize_for_session("https://www.xiaohongshu.com/one")
        .unwrap();
    assert!(DomainRouter::plan(&xhs.public, &a).release_enabled);
    let x = UrlPolicy
        .normalize_for_session("https://x.com/one")
        .unwrap();
    assert!(!DomainRouter::plan(&x.public, &a).release_enabled);
}

struct LoginEngine;
impl ImportEngine for LoginEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "login-fixture".into(),
            engine_version: "1".into(),
            route: "web.wechat.article".into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::Url
    }
    fn execute(
        &self,
        _: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        Err(BackendError::new(
            "IMPORT_WEB_LOGIN_REQUIRED",
            "secret-cookie-must-not-persist",
            false,
            true,
        ))
    }
}

#[test]
fn anonymous_attempt_precedes_login_recovery_and_pauses_without_secret_logs() {
    let root = std::env::temp_dir().join(format!("web-login-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("p", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    service.register_engine(Arc::new(LoginEngine)).unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "wechat".into(),
                locator: "https://mp.weixin.qq.com/s/id".into(),
                normalized_locator: Some("https://mp.weixin.qq.com/s/id".into()),
                source_identity: None,
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let item = &session.items[0];
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "web".into(),
            true,
        )
        .unwrap();
    let result = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &task.id,
        )
        .unwrap();
    assert_eq!(result.status, ImportItemStatus::WaitingLogin);
    assert_eq!(result.attempts.len(), 1);
    assert_eq!(result.attempts[0].route, "web.wechat.article");
    assert_eq!(result.attempts[0].engine_id, "login-fixture");
    assert!(result
        .issue
        .unwrap()
        .recovery_actions
        .contains(&ImportRecoveryAction::BeginLogin));
    assert_eq!(
        tasks.get_task(&task.id).unwrap().status,
        TaskStatus::WaitingForConfirmation
    );
    let persisted = std::fs::read_to_string(root.join(format!(
        ".app/import-sessions/{}/session.json",
        session.session_id
    )))
    .unwrap();
    let logs = serde_json::to_string(&tasks.get_logs(&task.id).unwrap()).unwrap();
    assert!(!persisted.contains("secret-cookie") && !logs.contains("secret-cookie"));
    // Closing the login window is a frontend-only dismissal. It must not
    // release the item through a synthetic Failed state or consume its task.
    let still_waiting = service
        .load_session(&context, &files, &session.session_id)
        .unwrap()
        .items
        .into_iter()
        .find(|candidate| candidate.item_id == item.item_id)
        .unwrap();
    assert_eq!(still_waiting.status, ImportItemStatus::WaitingLogin);
    assert_eq!(still_waiting.task_id.as_deref(), Some(task.id.as_str()));
    assert_eq!(
        tasks.get_task(&task.id).unwrap().status,
        TaskStatus::WaitingForConfirmation
    );
    std::fs::remove_dir_all(root).ok();
}
