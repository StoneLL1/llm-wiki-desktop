use std::cell::Cell;
use std::path::PathBuf;

use llm_wiki_desktop_lib::models::import_v2_file::FileScanPolicy;
use llm_wiki_desktop_lib::models::import_v2::{
    ImportInput, ImportInputKind, ImportItem, ImportResourceMode, ImportSession,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::import_v2::file_discovery::FileDiscoveryService;
use llm_wiki_desktop_lib::services::import_v2::SessionStore;
use llm_wiki_desktop_lib::services::FileStore;

const SCALE_FIXTURES: [usize; 3] = [100, 1_000, 10_000];

#[test]
fn scale_fixture_sizes_are_frozen_without_external_processing() {
    assert_eq!(SCALE_FIXTURES, [100, 1_000, 10_000]);
    for size in SCALE_FIXTURES {
        let items = (0..size).map(|index| format!("item-{index}")).collect::<Vec<_>>();
        assert_eq!(items.len(), size);
        assert_eq!(items.first(), Some(&"item-0".into()));
        assert_eq!(items.last(), Some(&format!("item-{}", size - 1)));
    }
}

#[test]
fn expected_red_discovery_emits_one_control_plane_batch_per_file() {
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
    assert_eq!(callback_invocations.get(), 100, "expected-red witness for Batch E throttling");
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
    let item_paths = session.items.iter().map(|item| {
        let path = root.path().join(format!(".app/import-sessions/scale-session/items/{}.json", item.item_id));
        (path.clone(), std::fs::metadata(path).unwrap().modified().unwrap())
    }).collect::<Vec<_>>();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let mut changed = session.items[0].clone();
    changed.selected = false;
    store.update_item(&context, &files, "scale-session", changed).unwrap();
    let rewritten = item_paths.iter().filter(|(path, before)| {
        std::fs::metadata(path).unwrap().modified().unwrap() > *before
    }).count();
    assert_eq!(rewritten, 100, "expected-red witness for Batch E incremental item persistence");
}

#[test]
fn expected_red_session_update_contract_exposes_full_session_reload_and_rewrite() {
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
    assert!(update.contains("self.load(context, file_store, session_id)?"));
    assert!(update.contains("self.save(context, file_store, &session)?"));
    assert!(update.contains(".iter_mut()"));
}
