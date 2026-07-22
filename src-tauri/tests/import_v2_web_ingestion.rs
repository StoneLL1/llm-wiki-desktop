use llm_wiki_desktop_lib::{
    errors::BackendError,
    models::{
        import_v2::{
            ImportInput, ImportInputKind, ImportItemStatus, ImportRecoveryAction,
            ImportResourceMode,
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
        std::fs::write(
            staging.join(".asr-input-fixture/input.m4a"),
            b"authorized audio",
        )
        .unwrap();
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
                temporary_input_path: ".asr-input-fixture/input.m4a".into(),
                media_kind: "audio".into(),
            }),
            warnings: vec![],
        })
    }
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
        input.kind == ImportInputKind::File && input.locator.ends_with(".m4a")
    }
    fn execute(
        &self,
        request: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        assert!(request.local_asr_authorized);
        let staging = std::path::Path::new(&request.project_root).join(&request.staging_root);
        std::fs::create_dir_all(staging.join(".sensevoice-output-fixture")).unwrap();
        std::fs::write(staging.join(".sensevoice-output-fixture/source.json"), b"{}").unwrap();
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

#[test]
fn bilibili_without_subtitles_runs_local_asr_automatically_and_cleans_temp() {
    assert_platform_auto_asr(
        "web.bilibili.metadata",
        "https://www.bilibili.com/video/BV1Ab411c7de?token=ephemeral",
    );
}

#[test]
fn xiaohongshu_and_douyin_without_subtitles_run_local_asr_automatically() {
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
        assert_platform_auto_asr(route, url);
    }
}

fn assert_platform_auto_asr(route: &'static str, exact: &str) {
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
    let completed = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &first.id,
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
fn login_required_pauses_item_and_task_with_typed_recovery_without_secret_logs() {
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
    let released = service
        .release_item_after_login(&context, &files, &session.session_id, &item.item_id)
        .unwrap();
    assert_eq!(released.status, ImportItemStatus::Failed);
    assert!(released.task_id.is_none());
    let retry = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "retry".into(),
            true,
        )
        .unwrap();
    let retried = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &retry.id,
        )
        .unwrap();
    assert_eq!(retried.status, ImportItemStatus::WaitingLogin);
    std::fs::remove_dir_all(root).ok();
}
