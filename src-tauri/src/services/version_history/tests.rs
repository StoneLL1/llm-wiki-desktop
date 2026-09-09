use super::*;
use std::process::Command;

fn fixture() -> (tempfile::TempDir, ProjectContext) {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("versions", root.path().to_path_buf());
    fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
    fs::create_dir_all(root.path().join(".app")).unwrap();
    fs::write(root.path().join("wiki/concepts/[ab].md"), b"before\r\n").unwrap();
    fs::write(root.path().join("other.md"), b"original\n").unwrap();
    GitService
        .initialize_repository(&context, "initial")
        .unwrap();
    (root, context)
}

fn changed(context: &ProjectContext) -> VersionOperation {
    let path = "wiki/concepts/[ab].md";
    let mut record = VersionHistoryService
        .begin(context, VersionOperationKind::LintFix, &[path.into()], None)
        .unwrap();
    VersionHistoryService
        .plan_write(context, &mut record, path, Some(b"after\n"))
        .unwrap();
    fs::write(context.root.join(path), b"after\n").unwrap();
    VersionHistoryService.finish(context, &mut record).unwrap();
    record
}

#[test]
fn operation_survives_reload_and_preserves_head_index_and_unrelated_edits() {
    let (_root, context) = fixture();
    fs::write(context.root.join("other.md"), b"staged\n").unwrap();
    assert!(Command::new("git")
        .current_dir(&context.root)
        .args(["add", "other.md"])
        .status()
        .unwrap()
        .success());
    let index = fs::read(context.root.join(".git/index")).unwrap();
    let head = GitService.repository_status(&context).unwrap().head;
    fs::write(context.root.join("other.md"), b"later edit\n").unwrap();
    let record = changed(&context);
    let page = VersionHistoryService.list(&context, None, 50).unwrap();
    assert_eq!(page.operations.len(), 1);
    assert_eq!(page.operations[0].operation_id, record.summary.operation_id);
    let restored = VersionHistoryService
        .restore(&context, &record.summary.operation_id)
        .unwrap();
    assert_eq!(restored.summary.state, VersionOperationState::Restored);
    VersionHistoryService
        .restore(&context, &record.summary.operation_id)
        .unwrap();
    assert_eq!(
        fs::read(context.root.join("wiki/concepts/[ab].md")).unwrap(),
        b"before\r\n"
    );
    assert_eq!(
        fs::read(context.root.join("other.md")).unwrap(),
        b"later edit\n"
    );
    assert_eq!(fs::read(context.root.join(".git/index")).unwrap(), index);
    assert_eq!(GitService.repository_status(&context).unwrap().head, head);
}

#[test]
fn later_edits_conflict_even_if_they_equal_original_bytes() {
    let (_root, context) = fixture();
    let record = changed(&context);
    fs::write(context.root.join("wiki/concepts/[ab].md"), b"before\r\n").unwrap();
    assert_eq!(
        VersionHistoryService
            .restore(&context, &record.summary.operation_id)
            .unwrap_err()
            .code,
        "VERSION_RESTORE_CONFLICT"
    );
    assert_eq!(
        VersionHistoryService
            .load(&context, &record.summary.operation_id)
            .unwrap()
            .summary
            .state,
        VersionOperationState::Applied
    );
}

#[test]
fn interrupted_apply_keeps_durable_intent_and_can_restore_only_owned_files() {
    let (_root, context) = fixture();
    let paths = vec![
        "wiki/concepts/[ab].md".into(),
        "wiki/concepts/new.md".into(),
    ];
    let mut record = VersionHistoryService
        .begin(&context, VersionOperationKind::LintFix, &paths, None)
        .unwrap();
    VersionHistoryService
        .plan_write(&context, &mut record, &paths[0], Some(b"partial\n"))
        .unwrap();
    fs::write(context.root.join(&paths[0]), b"partial\n").unwrap();
    VersionHistoryService
        .plan_write(
            &context,
            &mut record,
            &paths[1],
            Some(b"planned but never written"),
        )
        .unwrap();
    let restored = VersionHistoryService
        .restore(&context, &record.summary.operation_id)
        .unwrap();
    assert_eq!(restored.summary.state, VersionOperationState::Restored);
    assert!(!context.root.join(&paths[1]).exists());
    assert_eq!(
        fs::read(context.root.join(&paths[0])).unwrap(),
        b"before\r\n"
    );
}

#[test]
fn private_history_works_without_head_and_captures_ignored_files() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("private", root.path().to_path_buf());
    fs::create_dir_all(root.path().join("wiki/concepts")).unwrap();
    fs::write(root.path().join(".gitignore"), "wiki/\n.app/\n").unwrap();
    fs::write(root.path().join("wiki/concepts/[ab].md"), b"before\r\n").unwrap();
    GitService.enable_local_history(&context).unwrap();
    assert!(GitService.local_history_status(&context).unwrap().enabled);
    assert!(GitService
        .repository_status(&context)
        .unwrap()
        .head
        .is_none());
    let record = changed(&context);
    VersionHistoryService
        .restore(&context, &record.summary.operation_id)
        .unwrap();
    assert!(GitService
        .repository_status(&context)
        .unwrap()
        .head
        .is_none());
    assert!(!context.root.join(".git/index").exists());
}

#[test]
fn record_paths_and_size_limit_fail_before_snapshots_or_writes() {
    let (_root, context) = fixture();
    assert!(VersionHistoryService
        .load(&context, "../../outside")
        .is_err());
    let path = context.root.join("wiki/concepts/large.md");
    fs::File::create(&path)
        .unwrap()
        .set_len(MAX_CAPTURE_BYTES + 1)
        .unwrap();
    assert_eq!(
        VersionHistoryService
            .begin(
                &context,
                VersionOperationKind::LintFix,
                &["wiki/concepts/large.md".into()],
                None
            )
            .unwrap_err()
            .code,
        "VERSION_SIZE_LIMIT"
    );
    assert!(VersionHistoryService
        .list(&context, None, 50)
        .unwrap()
        .operations
        .is_empty());
}

#[test]
fn summary_failure_does_not_change_completed_facts_or_hide_recovery() {
    let (_root, context) = fixture();
    fs::create_dir_all(context.root.join(".app/version-history")).unwrap();
    fs::write(
        context.root.join(".app/version-history/summaries"),
        b"blocked directory",
    )
    .unwrap();
    let record = changed(&context);
    assert_eq!(
        VersionHistoryService
            .load(&context, &record.summary.operation_id)
            .unwrap()
            .summary
            .state,
        VersionOperationState::Applied
    );
    let page = VersionHistoryService.list(&context, None, 50).unwrap();
    assert_eq!(page.operations[0].state, VersionOperationState::Applied);
    VersionHistoryService
        .restore(&context, &record.summary.operation_id)
        .unwrap();
    assert_eq!(
        fs::read(context.root.join("wiki/concepts/[ab].md")).unwrap(),
        b"before\r\n"
    );
}

#[test]
fn manual_version_binds_preview_and_preserves_files_outside_its_scope() {
    let (_root, context) = fixture();
    let path = "wiki/concepts/[ab].md";
    let mut saved = VersionHistoryService
        .begin(
            &context,
            VersionOperationKind::ManualSnapshot,
            &[path.into()],
            None,
        )
        .unwrap();
    VersionHistoryService.finish(&context, &mut saved).unwrap();
    fs::write(context.root.join(path), b"current edit").unwrap();
    fs::write(
        context.root.join("wiki/concepts/later.md"),
        b"keep new page",
    )
    .unwrap();
    let baseline = hashes(
        &VersionHistoryService
            .capture(&context, &[path.into()])
            .unwrap(),
    );
    let detail = VersionHistoryService
        .file_diff(&context, &saved.summary.operation_id, path)
        .unwrap();
    assert_eq!(detail.before_text.as_deref(), Some("current edit"));
    assert_eq!(detail.after_text.as_deref(), Some("before\r\n"));
    fs::write(context.root.join(path), b"edited after preview").unwrap();
    assert_eq!(
        VersionHistoryService
            .restore_confirmed(&context, &saved.summary.operation_id, Some(&baseline))
            .unwrap_err()
            .code,
        "VERSION_RESTORE_CONFLICT"
    );
    fs::write(context.root.join(path), b"current edit").unwrap();
    let restored = VersionHistoryService
        .restore_confirmed(&context, &saved.summary.operation_id, Some(&baseline))
        .unwrap();
    let recovery = VersionHistoryService
        .load(&context, restored.restoration_id.as_deref().unwrap())
        .unwrap();
    assert_eq!(
        recovery.restored_from.as_deref(),
        Some(saved.summary.operation_id.as_str())
    );
    assert_eq!(recovery.summary.state, VersionOperationState::Applied);
    assert_eq!(
        fs::read(context.root.join("wiki/concepts/later.md")).unwrap(),
        b"keep new page"
    );
    VersionHistoryService
        .restore(&context, &recovery.summary.operation_id)
        .unwrap();
    assert_eq!(fs::read(context.root.join(path)).unwrap(), b"current edit");
}

#[test]
fn history_cursor_cannot_be_reused_for_another_project() {
    let (_root, context) = fixture();
    changed(&context);
    changed(&context);
    let cursor = VersionHistoryService
        .list(&context, None, 1)
        .unwrap()
        .next_cursor
        .unwrap();
    let (_other_root, other) = fixture();
    assert_eq!(
        VersionHistoryService
            .list(&other, Some(&cursor), 50)
            .unwrap_err()
            .code,
        "VERSION_CURSOR_INVALID"
    );
}

#[test]
fn interrupted_manual_restore_rejects_new_edits_instead_of_reusing_old_backup() {
    let (_root, context) = fixture();
    let path = "wiki/concepts/[ab].md";
    let mut saved = VersionHistoryService
        .begin(
            &context,
            VersionOperationKind::ManualSnapshot,
            &[path.into()],
            None,
        )
        .unwrap();
    VersionHistoryService.finish(&context, &mut saved).unwrap();
    fs::write(context.root.join(path), b"original current").unwrap();
    let mut recovery = VersionHistoryService
        .begin(
            &context,
            VersionOperationKind::Restore,
            &[path.into()],
            None,
        )
        .unwrap();
    recovery.restored_from = Some(saved.summary.operation_id.clone());
    recovery.expected_hashes = saved.before_hashes.clone();
    VersionHistoryService.persist(&context, &recovery).unwrap();
    saved.restoration_id = Some(recovery.summary.operation_id);
    saved.summary.state = VersionOperationState::Restoring;
    VersionHistoryService.persist(&context, &saved).unwrap();
    fs::write(context.root.join(path), b"newer external edit").unwrap();
    let baseline = hashes(
        &VersionHistoryService
            .capture(&context, &[path.into()])
            .unwrap(),
    );
    assert_eq!(
        VersionHistoryService
            .restore_confirmed(&context, &saved.summary.operation_id, Some(&baseline))
            .unwrap_err()
            .code,
        "VERSION_RESTORE_CONFLICT"
    );
    assert_eq!(
        fs::read(context.root.join(path)).unwrap(),
        b"newer external edit"
    );
}

#[test]
fn plain_operation_rejects_activity_log_before_writing() {
    let (_root, context) = fixture();
    fs::write(context.root.join("wiki/log.md"), b"audit record").unwrap();
    let expected = BTreeMap::from([(
        "wiki/log.md".into(),
        Some(FileStore.content_hash(b"audit record")),
    )]);
    let outputs = BTreeMap::from([("wiki/log.md".into(), Some(b"answer".to_vec()))]);
    assert_eq!(
        VersionHistoryService
            .apply_wiki_files(
                &context,
                VersionOperationKind::ChatEdit,
                &expected,
                &outputs
            )
            .unwrap_err()
            .code,
        "VERSION_PATH_UNSAFE"
    );
    assert_eq!(
        fs::read(context.root.join("wiki/log.md")).unwrap(),
        b"audit record"
    );
}

#[test]
fn growing_candidates_are_rejected_before_their_first_write() {
    let (_root, context) = fixture();
    let path = "wiki/concepts/[ab].md";
    let outputs = BTreeMap::from([(
        path.into(),
        Some(vec![b'x'; MAX_CAPTURE_BYTES as usize + 1]),
    )]);
    let expected = BTreeMap::from([(
        path.into(),
        FileStore.file_hash_if_exists(&context, path).unwrap(),
    )]);
    assert_eq!(
        VersionHistoryService
            .apply_wiki_files(
                &context,
                VersionOperationKind::ChatEdit,
                &expected,
                &outputs
            )
            .unwrap_err()
            .code,
        "VERSION_SIZE_LIMIT"
    );
    assert!(VersionHistoryService
        .list(&context, None, 50)
        .unwrap()
        .operations
        .is_empty());
    let other = "wiki/concepts/large.md";
    fs::write(context.root.join(other), b"small").unwrap();
    let mut record = VersionHistoryService
        .begin(
            &context,
            VersionOperationKind::LintFix,
            &[path.into(), other.into()],
            None,
        )
        .unwrap();
    fs::File::create(context.root.join(other))
        .unwrap()
        .set_len(MAX_CAPTURE_BYTES)
        .unwrap();
    assert_eq!(
        VersionHistoryService
            .plan_write(&context, &mut record, path, Some(b"candidate"))
            .unwrap_err()
            .code,
        "VERSION_SIZE_LIMIT"
    );
    assert_eq!(fs::read(context.root.join(path)).unwrap(), b"before\r\n");
    assert_eq!(
        VersionHistoryService
            .load(&context, &record.summary.operation_id)
            .unwrap()
            .expected_hashes,
        record.before_hashes
    );
}

#[test]
fn list_is_bounded_and_diff_reads_only_selected_file() {
    let (_root, context) = fixture();
    let first = changed(&context);
    let second = changed(&context);
    let page = VersionHistoryService.list(&context, None, 1).unwrap();
    assert_eq!(page.operations.len(), 1);
    assert_eq!(page.operations[0].operation_id, second.summary.operation_id);
    let next = VersionHistoryService
        .list(&context, page.next_cursor.as_deref(), 1)
        .unwrap();
    assert_eq!(next.operations[0].operation_id, first.summary.operation_id);
    assert!(next.next_cursor.is_none());
    let diff = VersionHistoryService
        .file_diff(
            &context,
            &first.summary.operation_id,
            "wiki/concepts/[ab].md",
        )
        .unwrap();
    assert_eq!(diff.before_text.as_deref(), Some("before\r\n"));
    assert_eq!(diff.after_text.as_deref(), Some("after\n"));
    assert!(VersionHistoryService
        .file_diff(&context, &first.summary.operation_id, "other.md")
        .is_err());
}

#[test]
#[ignore = "on-demand history pagination performance probe"]
fn ten_thousand_history_rows_keep_page_queries_git_free() {
    let (_root, context) = fixture();
    let mut record = changed(&context);
    let directory = context.root.join(".app/version-history");
    for index in 0..10_000u64 {
        record.summary.operation_id = format!(
            "{:016}-{}",
            1_700_000_000_000_000u64 + index,
            uuid::Uuid::new_v4()
        );
        fs::write(
            directory
                .join("operations")
                .join(format!("{}.json", record.summary.operation_id)),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        fs::write(
            directory
                .join("summaries")
                .join(format!("{}.json", record.summary.operation_id)),
            serde_json::to_vec(&record.summary).unwrap(),
        )
        .unwrap();
    }
    GitService::reset_process_attempts_for_test();
    let started = std::time::Instant::now();
    let first = VersionHistoryService.list(&context, None, 50).unwrap();
    let first_ms = started.elapsed().as_millis();
    let started = std::time::Instant::now();
    let second = VersionHistoryService
        .list(&context, first.next_cursor.as_deref(), 50)
        .unwrap();
    let second_ms = started.elapsed().as_millis();
    assert_eq!(first.operations.len(), 50);
    assert_eq!(second.operations.len(), 50);
    assert!(first.operations.iter().all(|first| second
        .operations
        .iter()
        .all(|second| first.operation_id != second.operation_id)));
    assert_eq!(GitService::process_attempts_for_test(), 0);
    eprintln!("history probe: 10001 rows, first page {first_ms} ms, second page {second_ms} ms, 0 Git processes");
}
