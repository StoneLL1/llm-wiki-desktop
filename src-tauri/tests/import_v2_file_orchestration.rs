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
