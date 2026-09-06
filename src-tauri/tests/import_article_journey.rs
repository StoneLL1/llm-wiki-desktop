//! Production parsers and Source transaction; no mock extractor or real vault.
use llm_wiki_desktop_lib::{
    models::{
        import_v2::{
            CommitImportSessionRequest, CommitItemDecision, ImportItemStatus, ImportResourceMode,
        },
        import_v2_file::FileScanPolicy,
        paths::ProjectContext,
        task::TaskType,
    },
    services::{
        import_v2::{
            file_discovery::{new_import_inputs, FileDiscoveryService},
            ImportV2Service,
        },
        FileStore, GitService, SecretService,
    },
    tasks::TaskService,
};
use std::{fs, io::Write, time::Instant};

#[test]
fn text_and_large_captioned_media_preview_commit_and_reopen() {
    let media_mib: u64 = std::env::var("IMPORT_BENCH_MEDIA_MIB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(65);
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("中文 资料");
    let project = root.path().join("知识库");
    fs::create_dir_all(&input).unwrap();
    fs::create_dir_all(project.join(".app")).unwrap();
    fs::write(
        input.join("短文.md"),
        "# 可读文章\n\n原始内容 includes challenge, login required and 安全验证.\n",
    )
    .unwrap();
    let media = input.join("访谈 65 MiB.mp3");
    let mut original = fs::File::create(&media).unwrap();
    original.write_all(b"ID3\x04\0\0\0\0\0\0").unwrap();
    original.set_len(media_mib * 1024 * 1024).unwrap();
    original.sync_all().unwrap();
    fs::write(media.with_extension("srt"), "1\n00:00:00,000 --> 00:00:02,000\n这是一份可靠的原始字幕。\n\n2\n00:00:02,000 --> 00:00:04,000\nImport preserves the complete interview.\n").unwrap();
    let context = ProjectContext::new("journey", project.clone());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    let tasks = TaskService::default();
    let started = Instant::now();
    let scan = FileDiscoveryService
        .scan(
            &context,
            &[input.join("短文.md"), media.clone()],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let inputs = new_import_inputs(&session, scan.files);
    let session = service
        .add_inputs(&context, &files, &session.session_id, inputs)
        .unwrap();
    let mut decisions = Vec::new();
    for item in &session.items {
        let task = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                project.clone(),
                "Article journey".into(),
                true,
            )
            .unwrap();
        let prepared = service
            .run_item(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        assert_eq!(
            prepared.status,
            ImportItemStatus::PreviewReady,
            "{:?}",
            prepared.issue
        );
        let preview = prepared.preview.unwrap();
        if item.input.display_name.ends_with(".mp3") {
            assert_eq!(preview.source_snapshot.size_bytes, media_mib * 1024 * 1024);
        }
        decisions.push(CommitItemDecision {
            item_id: item.item_id.clone(),
            resolution: preview
                .resolution
                .and_then(|resolution| resolution.default_resolution),
        });
    }
    let candidate_ms = started.elapsed().as_millis();
    let commit_start = Instant::now();
    let batch = service
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
                decisions,
            },
        )
        .unwrap();
    assert_eq!(batch.committed_count, 2, "{batch:?}");
    assert_eq!(batch.failed_count, 0);
    let commit_ms = commit_start.elapsed().as_millis();
    drop(service);
    let restarted = ImportV2Service::with_secret_service(SecretService::memory());
    let reopened = restarted
        .load_session(&context, &files, &session.session_id)
        .unwrap();
    assert!(reopened
        .items
        .iter()
        .all(|item| item.status == ImportItemStatus::Completed));
    let articles = batch
        .items
        .iter()
        .map(|item| fs::read_to_string(project.join(item.wiki_path.as_ref().unwrap())).unwrap())
        .collect::<Vec<_>>();
    assert!(articles
        .iter()
        .any(|text| text.contains("原始内容 includes challenge")));
    assert!(articles
        .iter()
        .any(|text| text.contains("这是一份可靠的原始字幕")));
    assert_eq!(fs::metadata(media).unwrap().len(), media_mib * 1024 * 1024);
    eprintln!(
        "article_journey candidate_ms={candidate_ms} commit_ms={commit_ms} items=2 media_bytes={}",
        media_mib * 1024 * 1024
    );
}
