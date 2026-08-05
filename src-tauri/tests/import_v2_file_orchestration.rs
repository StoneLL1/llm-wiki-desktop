use llm_wiki_desktop_lib::models::import_v2::{ImportIssue, ImportRecoveryAction, ImportStage};
use llm_wiki_desktop_lib::models::task::TaskType;
use llm_wiki_desktop_lib::tasks::TaskService;

#[test]
fn stable_file_issues_expose_deterministic_recovery() {
    let cases = [
        (
            "IMPORT_FILE_CAPABILITY_MISSING",
            ImportRecoveryAction::InstallCapability,
        ),
        ("IMPORT_FILE_PASSWORD_REQUIRED", ImportRecoveryAction::Retry),
        ("IMPORT_FILE_CORRUPT", ImportRecoveryAction::InvokeAgent),
        ("IMPORT_FILE_RESOURCE_LIMIT", ImportRecoveryAction::Retry),
        (
            "IMPORT_FILE_CONVERSION_FAILED",
            ImportRecoveryAction::SwitchParser,
        ),
        (
            "IMPORT_FILE_PARSE_FAILED",
            ImportRecoveryAction::SwitchParser,
        ),
        (
            "IMPORT_FILE_QUALITY_FAILED",
            ImportRecoveryAction::EnableOcr,
        ),
        ("IMPORT_FILE_CANCELLED", ImportRecoveryAction::Retry),
    ];
    for (code, expected) in cases {
        let issue = ImportIssue::for_file_code(code, ImportStage::Extract);
        assert_eq!(issue.code, code);
        assert!(issue.recovery_actions.contains(&expected), "{code}");
        assert!(issue
            .recovery_actions
            .contains(&ImportRecoveryAction::ViewLog));
        assert!(issue.recovery_actions.contains(&ImportRecoveryAction::Skip));
    }
}

#[test]
fn issue_contract_is_camel_case_and_actions_are_snake_case() {
    let value = serde_json::to_value(ImportIssue::for_file_code(
        "IMPORT_FILE_PARSE_FAILED",
        ImportStage::Extract,
    ))
    .unwrap();
    assert_eq!(value["recoveryActions"][0], "retry");
    assert!(value.get("userActionRequired").is_some());
}

#[test]
fn batch_a_expected_red_legacy_start_creates_a_task_per_requested_item() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands/import_v2_commands.rs"
    ))
    .unwrap();
    let start = source
        .split("pub(crate) fn start_import_items_for_state")
        .nth(1)
        .expect("legacy start boundary must remain visible until Batch E");
    let start = start.split("#[tauri::command]").next().unwrap();
    assert!(start.contains("prepare_all("));
    assert!(start.contains("create_project_task_with_batch("));
    assert!(start.contains("Result<Vec<BackendTask>, BackendError>"));
}

#[test]
fn batch_a_expected_red_task_service_creates_one_backend_task_per_item_at_scale() {
    for count in [100usize, 1_000, 10_000] {
        let root = tempfile::tempdir().unwrap();
        let service = TaskService::default();
        let mut task_ids = Vec::with_capacity(count);
        for index in 0..count {
            let task = service
                .create_project_task_with_batch(
                    TaskType::Import,
                    "batch-a".into(),
                    root.path().to_path_buf(),
                    format!("Import fixture-{index}"),
                    true,
                    format!("batch-a-{count}"),
                )
                .unwrap();
            task_ids.push(task.id);
        }
        assert_eq!(task_ids.len(), count, "current BackendTask baseline for {count} items");
        assert_eq!(service.list_tasks(None).len(), count);
    }
}
