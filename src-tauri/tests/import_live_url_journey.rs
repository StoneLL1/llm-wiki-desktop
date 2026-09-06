//! Opt-in network acceptance. Each result records the actual waiting/failed state;
//! only committed, reopened articles count as successes.
use llm_wiki_desktop_lib::{
    models::{import_v2::*, paths::ProjectContext, task::TaskType},
    services::{import_v2::ImportV2Service, FileStore, GitService, SecretService},
    tasks::TaskService,
};
use std::{fs, time::Instant};

#[test]
#[ignore = "live public URLs; set IMPORT_LIVE_URLS to a JSON string array"]
fn public_urls_prepare_preview_save_and_reopen() {
    let urls: Vec<String> =
        serde_json::from_str(&std::env::var("IMPORT_LIVE_URLS").unwrap_or_else(|_| {
            r#"["https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Authentication"]"#.into()
        }))
        .unwrap();
    let expected_count = std::env::var("IMPORT_LIVE_EXPECT_COMMITTED")
        .ok()
        .map(|value| value.parse::<usize>().unwrap())
        .unwrap_or(urls.len());
    let root = std::env::temp_dir().join(format!("import-live-urls-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("live-urls", root.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    if let Ok(directory) = std::env::var("IMPORT_ACCEPTANCE_INSTALLED_ROOT") {
        llm_wiki_desktop_lib::services::import_v2::capability_runtime::ImportCapabilityRuntime::default()
            .load_installed(std::path::Path::new(&directory), &service);
    }
    #[cfg(debug_assertions)]
    if let Ok(directory) = std::env::var("IMPORT_ACCEPTANCE_DEV_ROOT") {
        let directory = std::path::Path::new(&directory);
        llm_wiki_desktop_lib::services::import_v2::capability_runtime::ImportCapabilityRuntime::default()
            .load_development(&directory.join("installed"), &directory.join("development-public-key.hex"), &service);
    }
    let tasks = TaskService::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            urls.into_iter()
                .map(|url| {
                    // Use the same secure target handoff as add_import_url_v2;
                    // persisting a raw URL here would discard its signed query.
                    let target = llm_wiki_desktop_lib::services::import_v2::url_policy::UrlPolicy
                        .normalize_for_session(&url)
                        .unwrap();
                    ImportInput {
                        kind: ImportInputKind::Url,
                        display_name: target.public.public_url.clone(),
                        locator: service.store_web_target(&target).unwrap(),
                        normalized_locator: Some(target.public.public_url),
                        source_identity: None,
                        media_save_mode: Default::default(),
                    }
                })
                .collect(),
        )
        .unwrap();
    let mut decisions = Vec::new();
    let mut expected_titles = Vec::new();
    for item in &session.items {
        let task = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                root.clone(),
                "Live URL acceptance".into(),
                true,
            )
            .unwrap();
        let start = Instant::now();
        if std::env::var("IMPORT_ACCEPTANCE_FAKEIP").as_deref() == Ok("1") {
            use std::net::ToSocketAddrs;
            let url = url::Url::parse(&item.input.display_name).unwrap();
            let host = url.host_str().unwrap();
            let addresses: Vec<_> = (host, 443)
                .to_socket_addrs()
                .unwrap()
                .map(|a| a.ip())
                .collect();
            if addresses.iter().all(|ip| matches!(ip, std::net::IpAddr::V4(v4) if v4.octets()[0] == 198 && v4.octets()[1] == 18)) {
                service.authorize_private_target(llm_wiki_desktop_lib::services::import_v2::url_policy::PrivateTargetGrant {
                    item_id: item.item_id.clone(), scheme: "https".into(), host: host.into(), port: 443,
                    resolved_ips: addresses, expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
                }).unwrap();
            }
        }
        let mut prepared = match service.run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &task.id,
        ) {
            Ok(item) => item,
            Err(error) => {
                eprintln!(
                    "LIVE_URL {} elapsed_ms={} error={}",
                    item.input.display_name,
                    start.elapsed().as_millis(),
                    error.code
                );
                continue;
            }
        };
        eprintln!(
            "LIVE_URL {} elapsed_ms={} status={:?} issue={:?} attempts={:?}",
            item.input.display_name,
            start.elapsed().as_millis(),
            prepared.status,
            prepared.issue,
            prepared.attempts
        );
        if std::env::var("IMPORT_ACCEPTANCE_OCR").as_deref() == Ok("1")
            && prepared.issue.as_ref().is_some_and(|issue| {
                issue
                    .recovery_actions
                    .contains(&ImportRecoveryAction::EnableOcr)
                    || issue
                        .recovery_actions
                        .contains(&ImportRecoveryAction::InstallOcrCapability)
            })
        {
            service
                .authorize_media_for_session(
                    &context,
                    &files,
                    &session.session_id,
                    &item.item_id,
                    ImportMediaAuthorizationKind::Ocr,
                    None,
                    None,
                )
                .unwrap();
            let recognition = tasks
                .create_project_task(
                    TaskType::Import,
                    context.project_id.clone(),
                    root.clone(),
                    "Live note image recognition".into(),
                    true,
                )
                .unwrap();
            prepared = service
                .run_item_with_recovery(
                    &context,
                    &files,
                    &tasks,
                    &session.session_id,
                    &item.item_id,
                    &recognition.id,
                    Some(&ImportRecoveryAction::EnableOcr),
                )
                .unwrap();
            eprintln!(
                "LIVE_OCR {} elapsed_ms={} status={:?} issue={:?}",
                item.input.display_name,
                start.elapsed().as_millis(),
                prepared.status,
                prepared.issue
            );
        }
        if std::env::var("IMPORT_ACCEPTANCE_ASR").as_deref() == Ok("1")
            && prepared.status == ImportItemStatus::WaitingAuthorization
            && prepared.issue.as_ref().is_some_and(|issue| {
                issue
                    .recovery_actions
                    .contains(&ImportRecoveryAction::AuthorizeLocalAsr)
            })
        {
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
            let recognition = tasks
                .create_project_task(
                    TaskType::Import,
                    context.project_id.clone(),
                    root.clone(),
                    "Live video recognition".into(),
                    true,
                )
                .unwrap();
            match service.run_item_with_recovery(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &item.item_id,
                &recognition.id,
                Some(&ImportRecoveryAction::AuthorizeLocalAsr),
            ) {
                Ok(item) => prepared = item,
                Err(error) => eprintln!("LIVE_ASR error={}", error.code),
            }
            eprintln!(
                "LIVE_ASR {} elapsed_ms={} status={:?} issue={:?}",
                item.input.display_name,
                start.elapsed().as_millis(),
                prepared.status,
                prepared.issue
            );
        }
        if prepared.status == ImportItemStatus::PreviewReady {
            expected_titles.push(prepared.preview.as_ref().unwrap().title.clone());
            decisions.push(CommitItemDecision {
                item_id: item.item_id.clone(),
                resolution: prepared
                    .preview
                    .unwrap()
                    .resolution
                    .and_then(|r| r.default_resolution),
            });
        }
    }
    eprintln!("LIVE_REPORT_ROOT {}", root.display());
    assert_eq!(
        decisions.len(),
        expected_count,
        "Every expected URL must produce a readable preview; inspect the retained session at {}",
        root.display()
    );
    if !decisions.is_empty() {
        let count = decisions.len();
        let result = service
            .commit_items(
                &context,
                &files,
                &GitService,
                &CommitImportSessionRequest {
                    project_id: context.project_id.clone(),
                    project_root_path: root.to_string_lossy().into(),
                    session_id: session.session_id.clone(),
                    batch_task_id: None,
                    acknowledge_restricted_content: false,
                    expected_selection_revision: None,
                    expected_confirmation_digest: None,
                    decisions,
                },
            )
            .unwrap();
        assert_eq!(result.committed_count as usize, count, "{result:?}");
        drop(service);
        let reopened = ImportV2Service::with_secret_service(SecretService::memory());
        assert_eq!(
            reopened
                .load_session(&context, &files, &session.session_id)
                .unwrap()
                .items
                .iter()
                .filter(|i| i.status == ImportItemStatus::Completed)
                .count(),
            count
        );
        let mut reopened_articles = Vec::new();
        for item in result.items {
            if let Some(path) = item.wiki_path {
                let path = root.join(path);
                let article = fs::read_to_string(&path).unwrap();
                assert!(article.contains("type: source"));
                assert!(
                    !article.contains("xsec_token="),
                    "Signed access parameters must not enter the Source article"
                );
                assert!(
                    !article.contains("<!-- OCR_IMAGE_"),
                    "Every image must have its recognition result or missing-text note"
                );
                eprintln!("LIVE_SOURCE {} bytes={}", path.display(), article.len());
                reopened_articles.push(article);
            }
        }
        for title in expected_titles {
            assert!(
                reopened_articles
                    .iter()
                    .any(|article| article.contains(&title)),
                "The extracted title must survive commit and reopen"
            );
        }
        if let Ok(expected) = std::env::var("IMPORT_LIVE_EXPECT_TEXT") {
            for text in serde_json::from_str::<Vec<String>>(&expected).unwrap() {
                assert!(
                    reopened_articles
                        .iter()
                        .any(|article| article.contains(&text)),
                    "Expected readable text was missing after reopen: {text}"
                );
            }
        }
    }
    eprintln!("LIVE_REPORT_ROOT {}", root.display());
}
