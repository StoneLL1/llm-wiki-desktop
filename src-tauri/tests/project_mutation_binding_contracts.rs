fn production_source(source: &'static str) -> String {
    let normalized = source.replace("\r\n", "\n");
    [
        "\n#[cfg(test)]\nmod tests",
        "\n#[cfg(test)]\nmod sealed_read_tests",
    ]
    .into_iter()
    .filter_map(|marker| normalized.find(marker))
    .min()
    .map_or(normalized.clone(), |index| normalized[..index].to_string())
}

#[test]
fn platform_mutation_primitive_is_handle_relative_and_has_no_path_fallback() {
    let source = include_str!("../src/utils/safe_project_dir.rs");

    for required in [
        "NtCreateFile",
        "NtSetInformationFile",
        "RootDirectory",
        "OBJ_DONT_REPARSE",
        "FILE_OPEN_REPARSE_POINT",
        "FILE_RENAME_POSIX_SEMANTICS",
        "openat(",
        "renameat(",
        "SYS_renameat2",
        "RENAME_EXCHANGE",
        "unlinkat(",
        "libc::O_NOFOLLOW",
        "libc::O_NONBLOCK",
    ] {
        assert!(
            source.contains(required),
            "safe project mutation primitive lost required platform binding: {required}"
        );
    }
    for forbidden in ["MoveFileExW", "MoveFileW", "ReplaceFileW"] {
        assert!(
            !source.contains(forbidden),
            "Windows mutation must never fall back to a path-based primitive: {forbidden}"
        );
    }
}

#[test]
fn import_filestore_and_high_risk_cleanup_paths_consume_the_binding() {
    let transaction = production_source(include_str!("../src/services/import_v2/transaction.rs"));
    let file_store = production_source(include_str!("../src/services/file_store.rs"));
    assert!(transaction.contains("BoundProjectMutationRoot as RecoveryParentBinding"));
    assert!(transaction.contains("struct LiveBackup"));
    assert!(transaction.contains("struct LiveRecoveryArtifact"));
    assert!(transaction.contains("set_before_checked_final_mutation_hook"));
    assert!(transaction.contains("set_before_rollback_final_mutation_hook"));
    assert!(file_store.contains("BoundProjectMutationRoot"));
    assert!(file_store.contains("replace_existing_if_identity_and_hash"));
    assert!(file_store.contains("remove_file_if_identity_and_hash"));

    let agent_workspace =
        production_source(include_str!("../src/services/import_v2/agent_workspace.rs"));
    let generic_web_engine = production_source(include_str!(
        "../src/services/import_v2/generic_web_engine.rs"
    ));
    let import_orchestrator =
        production_source(include_str!("../src/services/import_v2/orchestrator.rs"));
    let migrated = [
        ("transaction", transaction.as_str()),
        ("file_store", file_store.as_str()),
        ("agent_workspace", agent_workspace.as_str()),
        ("generic_web_engine", generic_web_engine.as_str()),
        ("import_orchestrator", import_orchestrator.as_str()),
    ];
    for (name, source) in migrated {
        for forbidden in [
            "MoveFileExW",
            "std::fs::rename(",
            "std::fs::remove_file(",
            "std::fs::write(",
            "std::fs::copy(",
            "std::fs::create_dir_all(",
            "std::fs::remove_dir_all(",
            "fs::rename(",
            "fs::remove_file(",
            "fs::write(",
            "fs::copy(",
            "fs::create_dir_all(",
            "fs::remove_dir_all(",
        ] {
            assert!(
                !source.contains(forbidden),
                "{name} regressed to a path-based project mutation: {forbidden}"
            );
        }
    }

    // Generate Content stores review candidates only in a private global temp
    // root; its project-scoped final artifact still has to use FileStore.
    let generate_content =
        include_str!("../src/services/workflow_service/runners/generate_content.rs");
    assert!(generate_content.contains("write_html_checked"));
    assert!(generate_content.contains("remove_if_hash_matches"));
}

#[test]
fn real_platform_race_matrix_is_mandatory() {
    let source = include_str!("../src/utils/safe_project_dir.rs");
    assert!(source.contains("windows_junction_swap_race_never_changes_outside_sentinel"));
    assert!(source.contains("unix_symlink_swap_race_never_changes_outside_sentinel"));
    assert!(source.contains("retained_parent_blocks_or_survives_parent_replacement"));
    assert!(source.contains("windows_locked_target_fails_without_losing_original_bytes"));
    assert!(source.contains("permission_change_keeps_complete_original_or_replacement"));
    assert!(source.contains("unix_special_file_is_rejected_without_blocking"));
    assert!(source.contains("swap_request_tx.send(()).unwrap()"));
    assert!(source.contains("symlink swap timed out"));
    assert!(source.contains("assert_eq!(attacker.join().unwrap(), ROUNDS)"));
    assert!(source.contains("assert_eq!(mutations, ROUNDS)"));
    let transaction = include_str!("../src/services/import_v2/transaction.rs");
    assert!(transaction.contains("live_rollback_uses_retained_parent_while_symlink_swap_is_active"));
    assert!(transaction.contains("live_rollback_pins_parent_against_junction_replacement"));
    assert!(
        !source.contains("#[ignore]"),
        "the release-blocking platform race matrix cannot be ignored"
    );
}
