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
        outcome.mark_probed(&installed).unwrap();
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
