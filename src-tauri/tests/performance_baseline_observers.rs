#![cfg(feature = "performance-observers")]

use std::sync::{Arc, Barrier};
use std::time::Duration;

use llm_wiki_desktop_lib::models::import_v2::{
    ImportInput, ImportInputKind, ImportItem, ImportResourceMode, ImportSession,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::import_v2::{ImportV2Service, SessionStore};
use llm_wiki_desktop_lib::services::FileStore;
use llm_wiki_desktop_lib::tasks::TaskService;

const SESSION_FIXTURES: [usize; 3] = [100, 1_000, 10_000];
const HISTORY_FIXTURES: [usize; 3] = [100, 1_000, 10_000];
const DIFF_FIXTURES: [usize; 3] = [100, 1_000, 10_000];

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
fn batch_zero_scale_fixture_cardinalities_are_frozen() {
    assert_eq!(SESSION_FIXTURES, [100, 1_000, 10_000]);
    assert_eq!(HISTORY_FIXTURES, [100, 1_000, 10_000]);
    assert_eq!(DIFF_FIXTURES, [100, 1_000, 10_000]);
}

#[test]
fn absolute_observer_read_rejects_paths_outside_the_project() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("perf-containment", root.path().to_path_buf());
    let outside_path = outside.path().join("outside.json");
    std::fs::write(&outside_path, b"{}").unwrap();

    let error = FileStore::default()
        .read_project_bytes_absolute(&context, &outside_path)
        .expect_err("observer-only absolute reads must preserve canonical containment");
    assert_eq!(error.code, "PATH_OUTSIDE_PROJECT");
}

#[test]
fn session_read_update_and_noop_recovery_publish_exact_file_store_counts() {
    for item_count in SESSION_FIXTURES {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("perf-baseline", root.path().to_path_buf());
        let files = FileStore::default();
        let sessions = SessionStore::default();
        let service = ImportV2Service::default();
        let tasks = TaskService::default();
        let mut session = ImportSession::new(
            "performance-session",
            "perf-baseline",
            ImportResourceMode::Balanced,
        );
        session.items = (0..item_count).map(synthetic_item).collect();
        sessions.save(&context, &files, &session).unwrap();

        let overview_observation = files.observe_project(&context);
        let overview = service
            .read_session_overview(&context, &files, "performance-session")
            .unwrap();
        assert_eq!(overview.item_count, item_count as u64);
        let overview_snapshot = overview_observation.snapshot();
        assert_eq!(overview_snapshot.read_ops, 1);
        assert_eq!(overview_snapshot.write_ops, 0);
        assert_eq!(overview_snapshot.atomic_replaces, 0);

        let read_observation = files.observe_project(&context);
        let loaded = service
            .read_session(&context, &files, "performance-session")
            .unwrap();
        assert_eq!(loaded.items.len(), item_count);
        let read = read_observation.snapshot();
        assert_eq!(read.read_ops, item_count as u64 + 1);
        assert_eq!(read.write_ops, 0);
        assert!(read.bytes_read > 0);
        println!(
            "BATCH0_FILE_STORE session_load N={item_count} {}",
            serde_json::to_string(&read).unwrap()
        );
        drop(read_observation);

        let update_observation = files.observe_project(&context);
        let mut changed = loaded.items[0].clone();
        changed.selected = false;
        let updated = sessions
            .update_item(&context, &files, "performance-session", changed)
            .unwrap();
        assert!(!updated.items[0].selected);
        let update = update_observation.snapshot();
        assert_eq!(update.read_ops, item_count as u64 + 2);
        assert_eq!(update.write_ops, 1);
        assert_eq!(update.atomic_replaces, 1);
        assert!(update.bytes_read > 0);
        assert!(update.bytes_written > 0);
        println!(
            "BATCH0_FILE_STORE session_update N={item_count} {}",
            serde_json::to_string(&update).unwrap()
        );
        drop(update_observation);

        let recovery_observation = files.observe_project(&context);
        let recovered = service
            .recover_session(&context, &files, &tasks, "performance-session")
            .unwrap();
        assert_eq!(recovered.items.len(), item_count);
        let recovery = recovery_observation.snapshot();
        assert_eq!(recovery.read_ops, item_count as u64 + 1);
        assert_eq!(recovery.write_ops, 0);
        assert_eq!(recovery.atomic_replaces, 0);
        assert!(recovery.bytes_read > 0);
        assert_eq!(recovery.bytes_written, 0);
        println!(
            "BATCH0_FILE_STORE session_recovery N={item_count} {}",
            serde_json::to_string(&recovery).unwrap()
        );
    }
}

#[test]
fn recovery_writes_only_stale_items_and_the_session_record() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("perf-dirty-recovery", root.path().to_path_buf());
    let files = FileStore::default();
    let sessions = SessionStore::default();
    let service = ImportV2Service::default();
    let tasks = TaskService::default();
    let mut session = ImportSession::new(
        "dirty-recovery-session",
        "perf-dirty-recovery",
        ImportResourceMode::Balanced,
    );
    session.items = (0..100).map(synthetic_item).collect();
    for (index, item) in session.items.iter_mut().take(3).enumerate() {
        item.task_id = Some(format!("missing-task-{index}"));
    }
    sessions.save(&context, &files, &session).unwrap();

    let observation = files.observe_project(&context);
    let recovered = service
        .recover_session(&context, &files, &tasks, &session.session_id)
        .unwrap();
    assert!(recovered
        .items
        .iter()
        .take(3)
        .all(|item| item.task_id.is_none()));
    let snapshot = observation.snapshot();
    assert_eq!(snapshot.read_ops, 101);
    assert_eq!(snapshot.write_ops, 4);
    assert_eq!(snapshot.atomic_replaces, 4);
}

#[test]
fn cancelled_recovery_does_not_publish_a_partial_session() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("perf-cancelled-recovery", root.path().to_path_buf());
    let files = FileStore::default();
    let sessions = SessionStore::default();
    let service = ImportV2Service::default();
    let tasks = TaskService::default();
    let mut session = ImportSession::new(
        "cancelled-recovery-session",
        "perf-cancelled-recovery",
        ImportResourceMode::Balanced,
    );
    session.items = (0..100).map(synthetic_item).collect();
    for (index, item) in session.items.iter_mut().take(10).enumerate() {
        item.task_id = Some(format!("missing-task-{index}"));
    }
    sessions.save(&context, &files, &session).unwrap();

    let mut checks = 0usize;
    let error = service
        .recover_session_with_cancel(&context, &files, &tasks, &session.session_id, || {
            checks += 1;
            checks >= 5
        })
        .unwrap_err();
    assert_eq!(
        error.code,
        llm_wiki_desktop_lib::errors::IMPORT_V2_CANCELLED
    );

    let reopened = sessions
        .load(&context, &files, &session.session_id)
        .unwrap();
    assert_eq!(reopened.items, session.items);
    assert_eq!(reopened.updated_at, session.updated_at);
}

#[test]
fn import_mutation_lock_observer_records_real_contention_wait() {
    let service = Arc::new(ImportV2Service::default());
    let observation = service.observe_lock_waits();
    let barrier = Arc::new(Barrier::new(2));

    let holder_service = Arc::clone(&service);
    let holder_barrier = Arc::clone(&barrier);
    let holder = std::thread::spawn(move || {
        holder_service
            .hold_mutation_lock_for_observation(Duration::from_millis(100), Some(&holder_barrier));
    });

    barrier.wait();
    let contender_service = Arc::clone(&service);
    let contender = std::thread::spawn(move || {
        contender_service.hold_mutation_lock_for_observation(Duration::ZERO, None);
    });

    holder.join().unwrap();
    contender.join().unwrap();
    let snapshot = observation.snapshot();
    assert_eq!(snapshot.acquisitions, 2);
    assert_eq!(snapshot.waits_over_50_ms, 1);
    assert!(snapshot.total_wait_nanos >= 50_000_000);
    assert!(snapshot.max_wait_nanos >= 50_000_000);
    println!(
        "BATCH0_LOCK_WAIT {}",
        serde_json::to_string(&snapshot).unwrap()
    );
}
