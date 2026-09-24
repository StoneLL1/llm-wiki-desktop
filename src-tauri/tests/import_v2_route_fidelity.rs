//! Production route selection and native parsers, with only network acquisition replaced.
use llm_wiki_desktop_lib::{
    errors::BackendError,
    models::{import_v2::*, import_v2_file::FileScanPolicy, paths::ProjectContext, task::TaskType},
    services::{
        import_v2::{
            engine::*,
            file_discovery::{new_import_inputs, FileDiscoveryService},
            generic_web_engine::{GenericWebEngine, WebArtifactSource},
            url_policy::{PrivateTargetGrant, SessionWebTarget, UrlPolicy},
            web_fetch::{WebFetchArtifact, WebFetchPolicy},
            web_target_store::WebTargetStore,
            ImportV2Service,
        },
        FileStore, GitService, SecretService,
    },
    tasks::{task_model::CancellationToken, TaskService},
};
use std::{
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct Page(&'static str);
impl WebArtifactSource for Page {
    fn fetch(
        &self,
        target: SessionWebTarget,
        _: WebFetchPolicy,
        _: Option<&PrivateTargetGrant>,
        _: &str,
        _: &CancellationToken,
    ) -> Result<WebFetchArtifact, BackendError> {
        Ok(WebFetchArtifact {
            bytes: self.0.as_bytes().to_vec(),
            byte_len: self.0.len() as u64,
            final_public_url: target.public.public_url.clone(),
            final_session_target: target,
            content_type: "text/html".into(),
            sanitized_headers: Default::default(),
            redirects: vec![],
            elapsed_ms: 0,
        })
    }
}
struct CountEngine {
    id: &'static str,
    route: &'static str,
    calls: Arc<AtomicUsize>,
}
impl ImportEngine for CountEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: self.id.into(),
            engine_version: "old-resource".into(),
            route: self.route.into(),
        }
    }
    fn supports(&self, _: &ImportInput) -> bool {
        true
    }
    fn execute(
        &self,
        _: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(BackendError::new(
            "IMPORT_WEB_STRUCTURE_CHANGED",
            "counted fallback",
            true,
            true,
        ))
    }
}
fn run_and_commit(
    service: &ImportV2Service,
    context: &ProjectContext,
    input: ImportInput,
) -> String {
    let session = service
        .create_session(context, &FileStore, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(context, &FileStore, &session.session_id, vec![input])
        .unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            context.root.clone(),
            "route fidelity".into(),
            true,
        )
        .unwrap();
    let item = service
        .run_item(
            context,
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
    let preview = item.preview.unwrap();
    let result = service
        .commit_items(
            context,
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
                    resolution: preview.resolution.and_then(|r| r.default_resolution),
                }],
            },
        )
        .unwrap();
    assert_eq!(result.committed_count, 1, "{result:?}");
    let restarted = ImportV2Service::with_secret_service(SecretService::memory());
    let reopened = restarted
        .load_session(context, &FileStore, &session.session_id)
        .unwrap();
    assert_eq!(reopened.items[0].status, ImportItemStatus::Completed);
    fs::read_to_string(
        context
            .root
            .join(result.items[0].wiki_path.as_ref().unwrap()),
    )
    .unwrap()
}
#[test]
fn standalone_numeric_and_bracketed_subtitles_survive_old_pack_installation() {
    for installed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("subtitles", dir.path().join("知识库"));
        fs::create_dir_all(context.root.join(".app")).unwrap();
        let subtitle = dir.path().join("字幕.srt");
        fs::write(&subtitle, "1\n00:00:01,000 --> 00:00:02,000\n2026\n\n2\n00:01:03,000 --> 00:01:04,000\n42\n[掌声] (原文)\n").unwrap();
        let service = ImportV2Service::with_secret_service(SecretService::memory());
        let calls = Arc::new(AtomicUsize::new(0));
        if installed {
            service
                .register_engine(Arc::new(CountEngine {
                    id: "pack.media-runtime.media.subtitle",
                    route: "media.subtitle",
                    calls: calls.clone(),
                }))
                .unwrap();
        }
        let scan = FileDiscoveryService
            .scan(
                &context,
                &[subtitle],
                FileScanPolicy::default(),
                |_| {},
                || false,
            )
            .unwrap();
        let seed = service
            .create_session(&context, &FileStore, ImportResourceMode::Balanced)
            .unwrap();
        let input = new_import_inputs(&seed, scan.files).remove(0);
        let body = run_and_commit(&service, &context, input);
        for text in [
            "2026",
            "42",
            "[掌声] (原文)",
            "00:00:01.000",
            "00:01:03.000",
        ] {
            assert!(body.contains(text), "missing {text}: {body}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
#[test]
fn short_static_body_commits_but_title_navigation_shell_reaches_browser() {
    for (html, shell) in [("<main><nav>Home About</nav><div id='root'></div></main>", true), ("<title>Title</title><nav>Menu</nav><div id='root'></div><script>load()</script>", true),
        ("<header><h1>Site</h1></header><title>Title</title><nav>Menu</nav><main><h1>Guide</h1><p>Explain javascript: and captcha.</p></main>", false)] {
        let dir = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("web", dir.path().to_owned());
        fs::create_dir_all(context.root.join(".app")).unwrap();
        let secrets = SecretService::memory();
        let service = ImportV2Service::with_secret_service(secrets.clone());
        service.register_engine(Arc::new(GenericWebEngine::new_with_artifact_source(Arc::new(WebTargetStore::new(secrets)),
            "fixture.http", "web.generic.readability", Arc::new(Page(html))))).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        service.register_engine(Arc::new(CountEngine { id: "pack.browser-runtime.web.generic.browser", route: "web.generic.browser", calls: calls.clone() })).unwrap();
        let target = UrlPolicy.normalize_for_session("https://example.com/article").unwrap();
        let input = ImportInput { kind: ImportInputKind::Url, locator: service.store_web_target(&target).unwrap(), normalized_locator: Some(target.public.public_url), display_name: "article".into(), source_identity: None, media_save_mode: Default::default() };
        if !shell {
            let body = run_and_commit(&service, &context, input);
            assert!(body.contains("Explain javascript: and captcha."));
            assert!(!body.contains("Menu"));
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        } else {
            let session = service.create_session(&context, &FileStore, ImportResourceMode::Balanced).unwrap();
            let session = service.add_inputs(&context, &FileStore, &session.session_id, vec![input]).unwrap();
            let tasks = TaskService::default();
            let task = tasks.create_project_task(TaskType::Import, context.project_id.clone(), context.root.clone(), "shell".into(), true).unwrap();
            let error = service.run_item(&context, &FileStore, &tasks, &session.session_id, &session.items[0].item_id, &task.id).unwrap_err();
            assert_eq!(error.code, "IMPORT_WEB_STRUCTURE_CHANGED");
            let session = service.load_session(&context, &FileStore, &session.session_id).unwrap();
            assert_ne!(session.items[0].status, ImportItemStatus::PreviewReady);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }
    }
}
