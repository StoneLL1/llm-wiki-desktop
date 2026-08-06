use std::cell::Cell;
use std::path::PathBuf;

use llm_wiki_desktop_lib::models::import_v2::{
    ImportInput, ImportInputKind, ImportItem, ImportResourceMode, ImportSession,
};
use llm_wiki_desktop_lib::models::import_v2_file::FileScanPolicy;
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::task::TaskOperation;
use llm_wiki_desktop_lib::services::import_v2::file_discovery::FileDiscoveryService;
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

#[test]
fn batch_cohort_uses_one_operation_task_and_binds_ten_thousand_items() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("scale", root.path().to_path_buf());
    let files = FileStore::default();
    let service = ImportV2Service::default();
    let tasks = TaskService::default();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let inputs = (0..10_000)
        .map(|index| synthetic_item(index).input)
        .collect::<Vec<_>>();
    let session = service
        .add_inputs(&context, &files, &session.session_id, inputs)
        .unwrap();
    let ids = session
        .items
        .iter()
        .map(|item| item.item_id.clone())
        .collect::<Vec<_>>();
    let operation = service
        .begin_batch_operation(&context, &files, &tasks, &session.session_id, &ids)
        .unwrap();

    assert_eq!(tasks.list_tasks(None).len(), 1);
    assert_eq!(operation.batch_id.as_deref(), Some(operation.id.as_str()));
    assert_eq!(operation.title, "Import 10000 sources");
    assert_eq!(
        operation.operation,
        Some(TaskOperation::ImportBatch {
            session_id: session.session_id.clone(),
            item_count: 10_000,
            source_label: None,
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
