use llm_wiki_desktop_lib::models::import_v2::{ImportIssue, ImportRecoveryAction, ImportStage};

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
fn batch_one_scan_acceptance_owns_operation_creation_and_dispatch() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands/import_v2_file_commands.rs"
    ))
    .unwrap();
    let accept = source
        .split("pub fn accept_import_scan_v2")
        .nth(1)
        .expect("saved scan acceptance must remain a named command boundary");
    let accept = accept
        .split("pub fn discard_import_scan_v2")
        .next()
        .unwrap();
    assert!(accept.contains("accept_scan_inputs_with_operation_authorized"));
    assert!(accept.contains("dispatch_claimed_import_batch_for_state"));
    assert!(accept.contains("recover_claimed_scan_operation"));
    assert!(!accept.contains("add_inputs_authorized"));

    let ordinary = source
        .split("pub fn start_add_import_paths_v2")
        .nth(1)
        .expect("ordinary discovery must remain a named command boundary")
        .split("pub fn get_import_scan_result_v2")
        .next()
        .unwrap();
    assert!(ordinary.contains("accept_scan_inputs_with_operation_authorized"));
    assert!(ordinary.contains("TaskResultReference::ImportOperation"));
    assert!(ordinary.contains("recover_claimed_scan_operation"));
}

#[test]
fn batch_one_legacy_item_start_remains_bounded_compatibility_only() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands/import_v2_commands.rs"
    ))
    .unwrap();
    let start = source
        .split("pub(crate) fn start_import_items_for_state")
        .nth(1)
        .expect("legacy compatibility boundary must remain explicit");
    let start = start.split("pub fn start_import_batch_v2").next().unwrap();
    assert!(start.contains("IMPORT_BATCH_COMMAND_REQUIRED"));
    assert!(start.contains("request.item_ids.len() > 200"));

    let cancel = source
        .split("pub(crate) fn cancel_import_operation_for_state")
        .nth(1)
        .expect("operation cancellation must remain a named boundary");
    assert!(cancel.contains("take_queued_import_jobs"));
    assert!(cancel.contains("cancel_batch_item_cohort_for_task_authorized"));
}
