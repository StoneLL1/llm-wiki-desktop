use std::fs;
use std::path::Path;

use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::import_v2::migration::{
    LegacyHistoryAdapter, LegacyHistoryLimits,
};
use tempfile::tempdir;

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn visit(root: &Path, path: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).unwrap();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((
                relative,
                if metadata.is_file() {
                    fs::read(&path).unwrap()
                } else {
                    Vec::new()
                },
            ));
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                visit(root, &path, out);
            }
        }
    }
    let mut out = Vec::new();
    visit(root, root, &mut out);
    out.sort_by(|left, right| left.0.cmp(&right.0));
    out
}

#[test]
fn legacy_history_is_a_read_only_projection_without_destructive_actions() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join(".app/tasks")).unwrap();
    fs::create_dir_all(root.join(".app/import-history")).unwrap();
    fs::write(
        root.join(".app/tasks/task-1.json"),
        r#"{"id":"task-1","title":"Legacy import","status":"succeeded","startedAt":"2026-07-01T00:00:00Z","updatedAt":"2026-07-01T00:01:00Z","logPath":".app/tasks/task-1.log","secret":"do-not-project"}"#,
    )
    .unwrap();
    fs::write(
        root.join(".app/tasks/task-1.log"),
        "secret browser cookie output",
    )
    .unwrap();
    fs::write(
        root.join(".app/import-history/batch-1.json"),
        r#"{"batchId":"batch-1","status":"completed","createdAt":"2026-07-02T00:00:00Z"}"#,
    )
    .unwrap();
    fs::write(root.join(".app/import-history/broken.json"), b"{broken").unwrap();

    let before = snapshot(root);
    let context = ProjectContext::new("legacy", root.to_path_buf());
    let view = LegacyHistoryAdapter::default().list(&context).unwrap();
    assert!(view.entries.iter().all(|entry| entry.legacy_read_only));
    assert!(view
        .entries
        .iter()
        .all(|entry| entry.available_actions.is_empty()));
    assert!(view
        .entries
        .iter()
        .all(|entry| !entry.can_retry && !entry.can_delete && !entry.can_replace_source));
    assert!(view.entries.iter().any(|entry| entry.id == "task-1"));
    assert!(view.entries.iter().any(|entry| entry.id == "batch-1"));
    assert!(view
        .warnings
        .iter()
        .any(|warning| warning.code == "LEGACY_HISTORY_CORRUPT"));
    assert!(view
        .entries
        .iter()
        .all(|entry| !entry.title.contains("secret")));
    assert_eq!(before, snapshot(root));
}

#[test]
fn legacy_history_reads_are_bounded_and_corrupt_entries_do_not_hide_valid_entries() {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join(".app/tasks")).unwrap();
    fs::write(
        root.join(".app/tasks/valid.json"),
        r#"{"id":"valid","title":"Valid"}"#,
    )
    .unwrap();
    fs::write(root.join(".app/tasks/large.json"), "x".repeat(100)).unwrap();
    let context = ProjectContext::new("legacy-bounded", root.to_path_buf());
    let view = LegacyHistoryAdapter::new(LegacyHistoryLimits {
        max_files: 1,
        max_bytes: 20,
    })
    .list(&context)
    .unwrap();
    assert!(view
        .warnings
        .iter()
        .any(|warning| warning.code == "LEGACY_HISTORY_LIMIT"));
}
