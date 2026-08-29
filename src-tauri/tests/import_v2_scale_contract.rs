use std::cell::Cell;
use std::path::PathBuf;

#[cfg(feature = "performance-observers")]
use llm_wiki_desktop_lib::models::import_v2::ImportBatchResult;
use llm_wiki_desktop_lib::models::import_v2::{
    ImportInput, ImportInputKind, ImportItem, ImportItemPageFilter, ImportResourceMode,
    ImportSession,
};
use llm_wiki_desktop_lib::models::import_v2_file::FileScanPolicy;
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::task::TaskOperation;
use llm_wiki_desktop_lib::services::import_v2::file_discovery::FileDiscoveryService;
#[cfg(feature = "performance-observers")]
use llm_wiki_desktop_lib::services::import_v2::HistoryStore;
use llm_wiki_desktop_lib::services::import_v2::{ImportV2Service, SessionStore};
use llm_wiki_desktop_lib::services::FileStore;
use llm_wiki_desktop_lib::tasks::TaskService;

const SCALE_FIXTURES: [usize; 3] = [100, 1_000, 10_000];

#[test]
fn scale_fixture_sizes_are_frozen_without_external_processing() {
    assert_eq!(SCALE_FIXTURES, [100, 1_000, 10_000]);
    for size in SCALE_FIXTURES {
        let items = (0..size)
            .map(|index| format!("item-{index}"))
            .collect::<Vec<_>>();
        assert_eq!(items.len(), size);
        assert_eq!(items.first(), Some(&"item-0".into()));
        assert_eq!(items.last(), Some(&format!("item-{}", size - 1)));
    }
}

#[test]
fn discovery_coalesces_control_plane_batches() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let inputs = temp.path().join("inputs");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&inputs).unwrap();
    for index in 0..100 {
        std::fs::write(inputs.join(format!("{index:03}.md")), "# fixture\n").unwrap();
    }
    let context = ProjectContext::new("scale", root);
    let callback_invocations = Cell::new(0usize);
    let delivered_items = Cell::new(0usize);
    let scanned = FileDiscoveryService::default()
        .scan(
            &context,
            &[PathBuf::from(&inputs)],
            FileScanPolicy::default(),
            |batch| {
                callback_invocations.set(callback_invocations.get() + 1);
                delivered_items.set(delivered_items.get() + batch.len());
            },
            || false,
        )
        .unwrap();

    assert_eq!(scanned.files.len(), 100);
    assert!(callback_invocations.get() <= 2);
    assert_eq!(delivered_items.get(), 100);
}

fn synthetic_item(index: usize) -> ImportItem {
    ImportItem::queued(
        &format!("item-{index}"),
        ImportInput {
            kind: ImportInputKind::File,
            display_name: format!("{index}.md"),
            locator: format!("fixture/{index}.md"),
            normalized_locator: None,
            source_identity: None,
            media_save_mode: Default::default(),
        },
    )
}

#[test]
fn expected_red_single_item_update_rewrites_every_persisted_item_file() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("scale", root.path().to_path_buf());
    let store = SessionStore::default();
    let files = FileStore::default();
    let mut session = ImportSession::new("scale-session", "scale", ImportResourceMode::Balanced);
    session.items = (0..100).map(synthetic_item).collect();
    store.save(&context, &files, &session).unwrap();
    let item_paths = session
        .items
        .iter()
        .map(|item| {
            let path = root.path().join(format!(
                ".app/import-sessions/scale-session/items/{}.json",
                item.item_id
            ));
            (
                path.clone(),
                std::fs::metadata(path).unwrap().modified().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let mut changed = session.items[0].clone();
    changed.selected = false;
    store
        .update_item(&context, &files, "scale-session", changed)
        .unwrap();
    let rewritten = item_paths
        .iter()
        .filter(|(path, before)| std::fs::metadata(path).unwrap().modified().unwrap() > *before)
        .count();
    assert_eq!(rewritten, 1, "an item state transition must be incremental");
}

#[test]
#[cfg(feature = "performance-observers")]
fn overview_and_first_page_are_bounded_for_ten_thousand_items() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("scale-page", root.path().to_path_buf());
    let store = SessionStore::default();
    let files = FileStore::default();
    let mut session = ImportSession::new(
        "scale-page-session",
        "scale-page",
        ImportResourceMode::Balanced,
    );
    session.items = (0..10_000).map(synthetic_item).collect();
    store.save(&context, &files, &session).unwrap();

    let overview_observer = files.observe_project(&context);
    let overview = store
        .read_overview(&context, &files, &session.session_id)
        .unwrap();
    let overview_io = overview_observer.snapshot();
    assert_eq!(overview.item_count, 10_000);
    assert_eq!(
        overview_io.read_ops, 1,
        "overview must read only state.json"
    );
    assert_eq!(overview_io.write_ops, 0);

    let page_observer = files.observe_project(&context);
    let first = store
        .list_items(
            &context,
            &files,
            &session.session_id,
            ImportItemPageFilter::All,
            None,
            200,
        )
        .unwrap();
    let page_io = page_observer.snapshot();
    assert_eq!(first.items.len(), 200);
    assert!(first.next_cursor.is_some());
    assert_eq!(first.total, 10_000);
    assert!(
        page_io.read_ops <= 202,
        "first page may read state + one order page + at most 200 item files: {page_io:?}"
    );
    assert_eq!(page_io.write_ops, 0);
}

#[test]
fn item_revision_invalidates_an_older_page_cursor() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("scale-stale", root.path().to_path_buf());
    let store = SessionStore::default();
    let files = FileStore::default();
    let mut session = ImportSession::new(
        "scale-stale-session",
        "scale-stale",
        ImportResourceMode::Balanced,
    );
    session.items = (0..3).map(synthetic_item).collect();
    store.save(&context, &files, &session).unwrap();
    let first = store
        .list_items(
            &context,
            &files,
            &session.session_id,
            ImportItemPageFilter::All,
            None,
            1,
        )
        .unwrap();
    let cursor = first.next_cursor.unwrap();
    let mut changed = session.items[0].clone();
    changed.selected = false;
    store
        .update_item(&context, &files, &session.session_id, changed)
        .unwrap();
    let stored = store
        .load_item(&context, &files, &session.session_id, "item-0")
        .unwrap();
    assert_eq!(stored.item_revision, 2);
    let stale = store
        .list_items(
            &context,
            &files,
            &session.session_id,
            ImportItemPageFilter::All,
            Some(&cursor),
            1,
        )
        .unwrap_err();
    assert_eq!(stale.code, "IMPORT_V2_SESSION_CURSOR_STALE");
}

#[test]
fn unrelated_status_progress_does_not_stale_the_selection_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("scale-selection", root.path().to_path_buf());
    let store = SessionStore::default();
    let files = FileStore::default();
    let mut session = ImportSession::new(
        "scale-selection-session",
        "scale-selection",
        ImportResourceMode::Balanced,
    );
    session.items = vec![synthetic_item(0)];
    store.save(&context, &files, &session).unwrap();
    let before = store
        .read_overview(&context, &files, &session.session_id)
        .unwrap();

    let mut inspecting = session.items[0].clone();
    inspecting.status = llm_wiki_desktop_lib::models::import_v2::ImportItemStatus::Inspecting;
    store
        .update_item(&context, &files, &session.session_id, inspecting)
        .unwrap();

    let after = store
        .validate_selection_snapshot(
            &context,
            &files,
            &session.session_id,
            before.selection_revision,
            &before.confirmation_digest,
        )
        .unwrap();
    assert!(after.semantic_revision > before.semantic_revision);
    assert_eq!(after.selection_revision, before.selection_revision);
    assert_eq!(after.confirmation_digest, before.confirmation_digest);
}

#[test]
#[cfg(feature = "performance-observers")]
fn active_pointer_and_legacy_rebuild_keep_foreground_reads_bounded() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("scale-pointer", root.path().to_path_buf());
    let store = SessionStore::default();
    let files = FileStore::default();
    let mut session = ImportSession::new(
        "scale-pointer-session",
        "scale-pointer",
        ImportResourceMode::Balanced,
    );
    session.items = (0..100).map(synthetic_item).collect();
    store.save(&context, &files, &session).unwrap();

    let pointer_observer = files.observe_project(&context);
    assert_eq!(
        store.find_unfinished_session(&context, &files).unwrap(),
        Some(session.session_id.clone())
    );
    let pointer_io = pointer_observer.snapshot();
    assert_eq!(
        pointer_io.read_ops, 2,
        "pointer discovery reads pointer + state only"
    );
    assert_eq!(pointer_io.write_ops, 0);

    let session_root = root
        .path()
        .join(".app/import-sessions")
        .join(&session.session_id);
    std::fs::remove_file(session_root.join("state.json")).unwrap();
    std::fs::remove_dir_all(session_root.join("order")).unwrap();
    std::fs::remove_file(root.path().join(".app/import-sessions/active-session.json")).unwrap();

    let legacy_observer = files.observe_project(&context);
    let legacy = store
        .read_overview(&context, &files, &session.session_id)
        .unwrap();
    let legacy_io = legacy_observer.snapshot();
    assert_eq!(
        legacy.index_state,
        llm_wiki_desktop_lib::models::import_v2::ImportSessionIndexState::RebuildRequired
    );
    assert_eq!(
        legacy_io.write_ops, 0,
        "foreground GET must not rebuild sidecars"
    );

    store.rebuild_sidecars(&context, &files, &session).unwrap();
    assert_eq!(
        store
            .read_overview(&context, &files, &session.session_id)
            .unwrap()
            .index_state,
        llm_wiki_desktop_lib::models::import_v2::ImportSessionIndexState::Ready
    );
}

#[test]
fn older_control_schema_requires_an_explicit_projection_rebuild() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("old-control", root.path().to_path_buf());
    let store = SessionStore::default();
    let files = FileStore::default();
    let mut session = ImportSession::new(
        "old-control-session",
        "old-control",
        ImportResourceMode::Balanced,
    );
    session.items = vec![synthetic_item(0)];
    store.save(&context, &files, &session).unwrap();

    let state_path = root
        .path()
        .join(".app/import-sessions/old-control-session/state.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
    state["schemaVersion"] = serde_json::json!(1);
    std::fs::write(&state_path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();

    let old = store
        .read_overview(&context, &files, &session.session_id)
        .unwrap();
    assert_eq!(
        old.index_state,
        llm_wiki_desktop_lib::models::import_v2::ImportSessionIndexState::RebuildRequired
    );
    store.rebuild_sidecars(&context, &files, &session).unwrap();
    let rebuilt = store
        .read_overview(&context, &files, &session.session_id)
        .unwrap();
    assert_eq!(
        rebuilt.index_state,
        llm_wiki_desktop_lib::models::import_v2::ImportSessionIndexState::Ready
    );
    assert_eq!(rebuilt.status_counts.queued, 1);
}

#[test]
fn session_update_contract_uses_incremental_item_persistence() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/services/import_v2/session_store.rs"
    ))
    .unwrap();
    let update = source
        .split("pub fn update_item")
        .nth(1)
        .expect("update_item must remain a named compatibility boundary");
    let update = update.split("fn public_import_input").next().unwrap();
    assert!(update.contains("self.load_item(context, file_store, session_id, &item.item_id)?"));
    assert!(update.contains("self.write_item(context, file_store, session_id, &item)?"));
    assert!(!update.contains("self.save(context, file_store, &session)?"));
}

fn assert_scan_acceptance_owns_complete_cohort(item_count: usize) {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new(
        format!("scan-acceptance-{item_count}"),
        root.path().to_path_buf(),
    );
    let files = FileStore::default();
    let service = ImportV2Service::default();
    let tasks = TaskService::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let inputs = (0..item_count)
        .map(|index| synthetic_item(index).input)
        .collect::<Vec<_>>();
    let (operation, item_ids) = service
        .accept_scan_inputs_with_operation(&context, &files, &tasks, &session.session_id, inputs)
        .unwrap()
        .expect("non-empty scan must create one operation");

    assert_eq!(tasks.list_tasks(None).len(), 1);
    assert_eq!(item_ids.len(), item_count);
    assert_eq!(operation.batch_id.as_deref(), Some(operation.id.as_str()));
    assert_eq!(
        operation.title,
        if item_count == 1 {
            "Import 0.md".to_string()
        } else {
            format!("Import {item_count} sources")
        }
    );
    assert_eq!(
        operation.operation,
        Some(TaskOperation::ImportBatch {
            session_id: session.session_id.clone(),
            item_count: item_count as u64,
            source_label: (item_count == 1).then(|| "0.md".into()),
        })
    );
    let rebound = service
        .load_session(&context, &files, &session.session_id)
        .unwrap();
    assert!(rebound
        .items
        .iter()
        .all(|item| item.task_id.as_deref() == Some(operation.id.as_str())));
}

#[test]
fn scan_acceptance_uses_one_operation_across_paging_and_confirmation_boundaries() {
    for item_count in [1, 200, 201, 600, 1_000, 1_001] {
        assert_scan_acceptance_owns_complete_cohort(item_count);
    }
}

#[test]
fn scan_acceptance_binds_ten_thousand_items_to_one_operation() {
    assert_scan_acceptance_owns_complete_cohort(10_000);
}

#[test]
fn singleton_url_operation_uses_source_title_and_unique_identity() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("url-operation", root.path().to_path_buf());
    let files = FileStore::default();
    let service = ImportV2Service::default();
    let tasks = TaskService::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let url = "https://example.com/article";
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: url.into(),
                locator: url.into(),
                normalized_locator: Some(url.into()),
                source_identity: None,
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();

    let operation = service
        .create_batch_operation_task(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &[session.items[0].item_id.clone()],
        )
        .unwrap();

    assert_eq!(operation.batch_id.as_deref(), Some(operation.id.as_str()));
    assert_eq!(operation.title, format!("Import {url}"));
    assert_eq!(
        operation.operation,
        Some(TaskOperation::ImportBatch {
            session_id: session.session_id,
            item_count: 1,
            source_label: Some(url.into()),
        })
    );
}

#[test]
fn batch_operation_rejects_empty_before_creating_a_task() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("empty-batch", root.path().to_path_buf());
    let files = FileStore::default();
    let service = ImportV2Service::default();
    let tasks = TaskService::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();

    let error = service
        .begin_batch_operation(&context, &files, &tasks, &session.session_id, &[])
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_BATCH_EMPTY");
    assert!(tasks.list_tasks(None).is_empty());
}

#[test]
fn batch_operation_validates_full_cohort_before_creating_or_claiming() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("invalid-batch", root.path().to_path_buf());
    let files = FileStore::default();
    let service = ImportV2Service::default();
    let tasks = TaskService::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![synthetic_item(0).input, synthetic_item(1).input],
        )
        .unwrap();

    let error = service
        .begin_batch_operation(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &[session.items[0].item_id.clone(), "missing-item".into()],
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_ITEM_NOT_FOUND");
    assert!(tasks.list_tasks(None).is_empty());
    let reopened = service
        .load_session(&context, &files, &session.session_id)
        .unwrap();
    assert!(reopened.items.iter().all(|item| item.task_id.is_none()));
}

#[test]
fn compatible_layout_persists_import_state_only_under_its_app_owned_root() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".app/compat")).unwrap();
    std::fs::write(root.path().join(".app/compat/purpose.md"), "# Import state").unwrap();
    std::fs::write(root.path().join(".app/compat/schema.md"), "# Schema").unwrap();
    std::fs::write(root.path().join("existing-note.md"), "# Existing user note").unwrap();
    let context = ProjectContext::new("compatible-import", root.path().to_path_buf())
        .with_resolved_layout()
        .unwrap();
    assert_eq!(
        context.layout.import_state_root.as_deref(),
        Some(".app/compat/import-sessions")
    );
    let service = ImportV2Service::default();
    let files = FileStore::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    assert!(root
        .path()
        .join(format!(
            ".app/compat/import-sessions/{}",
            session.session_id
        ))
        .is_dir());
    assert!(!root.path().join(".app/import-sessions").exists());
}

#[test]
fn batch_binding_rejects_invalid_cohort_without_partially_claiming_items() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("scale", root.path().to_path_buf());
    let files = FileStore::default();
    let service = ImportV2Service::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![synthetic_item(0).input, synthetic_item(1).input],
        )
        .unwrap();

    let result = service.bind_item_task_ids(
        &context,
        &files,
        &session.session_id,
        &[
            (session.items[0].item_id.clone(), "operation-a".into()),
            ("missing-item".into(), "operation-a".into()),
        ],
    );
    assert!(result.is_err());
    let reopened = service
        .load_session(&context, &files, &session.session_id)
        .unwrap();
    assert!(reopened.items.iter().all(|item| item.task_id.is_none()));
}

#[test]
#[cfg(feature = "performance-observers")]
fn batch_worker_preparation_builds_one_frozen_snapshot_per_item_with_near_linear_io() {
    let mut observed = Vec::new();
    for item_count in SCALE_FIXTURES {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new(
            format!("worker-scale-{item_count}"),
            root.path().to_path_buf(),
        );
        let files = FileStore::default();
        let service = ImportV2Service::default();
        let tasks = TaskService::default();
        let session = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = service
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                (0..item_count)
                    .map(|index| synthetic_item(index).input)
                    .collect(),
            )
            .unwrap();
        let item_ids = session
            .items
            .iter()
            .map(|item| item.item_id.clone())
            .collect::<Vec<_>>();
        let operation = service
            .create_batch_operation_task(&context, &files, &tasks, &session.session_id, &item_ids)
            .unwrap();

        let observer = files.observe_project(&context);
        let preparation = service
            .prepare_batch_operation(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &operation.id,
                &item_ids,
                || false,
            )
            .unwrap();
        let io = observer.snapshot();

        assert!(preparation.replaced_task_ids.is_empty());
        assert_eq!(preparation.snapshots.len(), item_count);
        assert!(preparation.snapshots.iter().all(|snapshot| {
            snapshot.expected_item_revision > 0
                && snapshot.resource_mode == ImportResourceMode::Balanced
        }));
        let control_ops = io.read_ops.saturating_add(io.write_ops);
        assert!(
            control_ops <= (item_count as u64).saturating_mul(4).saturating_add(16),
            "worker preparation must stay within a fixed per-item I/O budget: N={item_count}, {io:?}"
        );
        println!(
            "batch6_worker_control_plane item_count={item_count} read_ops={} write_ops={} control_ops={control_ops}",
            io.read_ops, io.write_ops
        );
        observed.push((item_count, control_ops));
    }

    for window in observed.windows(2) {
        let (smaller_n, smaller_ops) = window[0];
        let (larger_n, larger_ops) = window[1];
        assert_eq!(larger_n, smaller_n * 10);
        assert!(
            larger_ops <= smaller_ops.saturating_mul(15),
            "10x worker cohort must not approach quadratic control-plane growth: {observed:?}"
        );
    }
}

#[test]
fn production_worker_job_carries_a_frozen_snapshot() {
    let commands = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands/import_v2_commands.rs"
    ))
    .unwrap();
    let worker_job = commands
        .split("struct ImportWorkerJob")
        .nth(1)
        .unwrap()
        .split("struct BatchOperationJob")
        .next()
        .unwrap();
    assert!(worker_job.contains("snapshot: ImportWorkItemSnapshot"));
}

#[test]
#[cfg(feature = "performance-observers")]
fn history_page_one_reads_only_the_index_page_at_ten_thousand_batches() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("history-scale", root.path().to_path_buf());
    let files = FileStore::default();
    let history = HistoryStore::default();
    let history_root = root.path().join(".app/import-history");
    std::fs::create_dir_all(&history_root).unwrap();
    for index in 0..10_000 {
        let batch = ImportBatchResult {
            batch_id: format!("batch-{index:05}"),
            session_id: format!("session-{index:05}"),
            created_at: format!("2026-08-27T00:{:02}:{:02}Z", (index / 60) % 60, index % 60),
            batch_task_id: None,
            committed_count: 0,
            failed_count: 0,
            items: Vec::new(),
            history_snapshot: None,
            completion: None,
        };
        std::fs::write(
            history_root.join(format!("batch-{index:05}.json")),
            serde_json::to_vec(&batch).unwrap(),
        )
        .unwrap();
    }
    history.rebuild_index(&context, &files, || false).unwrap();

    let observation = files.observe_project(&context);
    let page = history.list_page(&context, &files, None, 50).unwrap();
    let io = observation.snapshot();
    assert_eq!(page.entries.len(), 50);
    assert!(page.next_cursor.is_some());
    assert!(
        io.read_ops <= 2,
        "page one must read only manifest + one index page: {io:?}"
    );
    assert_eq!(io.write_ops, 0);
    let wire = serde_json::to_value(&page.entries[0]).unwrap();
    assert!(wire.get("itemIds").is_none());
}

#[test]
#[cfg(feature = "performance-observers")]
fn history_detail_reads_only_the_requested_item_page() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("history-detail-scale", root.path().to_path_buf());
    let files = FileStore::default();
    let history = HistoryStore::default();
    let mut session = ImportSession::new(
        "history-session",
        "history-detail-scale",
        ImportResourceMode::Balanced,
    );
    session.items = (0..10_000).map(synthetic_item).collect();
    let batch = ImportBatchResult {
        batch_id: "history-batch".into(),
        session_id: session.session_id.clone(),
        created_at: "2026-08-27T00:00:00Z".into(),
        batch_task_id: None,
        committed_count: 0,
        failed_count: 0,
        items: Vec::new(),
        history_snapshot: Some(session),
        completion: None,
    };
    history.begin_batch(&context, &files, &batch).unwrap();

    let observation = files.observe_project(&context);
    let page = history
        .detail_page(&context, &files, "history-batch", None, 50)
        .unwrap();
    let io = observation.snapshot();
    assert_eq!(page.items.len(), 50);
    assert_eq!(page.total, 10_000);
    assert!(page.next_cursor.is_some());
    assert!(
        io.read_ops <= 52,
        "detail must read manifest + one order page + at most 50 snapshots: {io:?}"
    );
    assert_eq!(io.write_ops, 0);
}
