#[path = "support/workflow_baseline.rs"]
mod workflow_baseline;

use std::collections::HashSet;

use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::workflow::{
    HealthCheckMode, WorkflowKind, WorkflowPersistenceMode, WorkflowScope,
};
use llm_wiki_desktop_lib::services::{WorkflowPreference, WorkflowPreferences};

use workflow_baseline::{
    fixed_diffs, fixture_signature, history_attempts, markdown_paths, scope_options,
    MutableTestAuthority, DIFF_BYTES, DIFF_FILE_COUNT, HISTORY_ATTEMPT_COUNT, MARKDOWN_FILE_COUNT,
    SCOPE_OPTION_COUNT,
};

#[test]
fn deterministic_scale_fixtures_repeat_identically_ten_times() {
    let signatures = (0..10).map(|_| fixture_signature()).collect::<HashSet<_>>();
    assert_eq!(signatures.len(), 1);
    assert_eq!(markdown_paths().len(), MARKDOWN_FILE_COUNT);
    assert_eq!(scope_options().len(), SCOPE_OPTION_COUNT);
    assert_eq!(history_attempts().len(), HISTORY_ATTEMPT_COUNT);
    let diffs = fixed_diffs();
    assert_eq!(diffs.len(), DIFF_FILE_COUNT);
    assert!(diffs.iter().all(|(_, diff)| diff.len() == DIFF_BYTES));
}

#[test]
fn mutable_test_authority_switches_all_identity_and_access_dimensions_atomically() {
    let authority = MutableTestAuthority::new();
    assert!(authority.snapshot().trusted);
    authority.replace(false, false, "runtime-b", "identity-b", "revision-b");
    let replacement = authority.snapshot();
    assert!(!replacement.trusted);
    assert!(!replacement.writable);
    assert_eq!(replacement.runtime_project_id, "runtime-b");
    assert_eq!(replacement.canonical_identity_key, "identity-b");
    assert_eq!(replacement.identity_revision, "revision-b");
}

#[test]
fn preferences_follow_layout_root_and_fall_back_to_identity_isolated_memory() {
    fn preference() -> WorkflowPreference {
        WorkflowPreference {
            kind: WorkflowKind::HealthCheck,
            scope: WorkflowScope::HealthCheck {
                mode: HealthCheckMode::LocalQuick,
            },
            route: None,
            baseline_fingerprint: "a".repeat(64),
            preparation_fingerprint: "b".repeat(64),
            saved_at: String::new(),
        }
    }

    let native = tempfile::tempdir().unwrap();
    let native_context = ProjectContext::new("native", native.path().to_path_buf());
    let native_preferences = WorkflowPreferences::default();
    native_preferences
        .remember(
            &native_context,
            "native-identity",
            "native-revision",
            &WorkflowPersistenceMode::Persistent,
            preference(),
        )
        .unwrap();
    assert!(native
        .path()
        .join(".app/workflows/preferences.json")
        .is_file());
    assert_eq!(
        native_preferences
            .load(
                &native_context,
                "native-identity",
                "native-revision",
                &WorkflowPersistenceMode::Persistent,
            )
            .unwrap()
            .len(),
        1,
    );

    let compatible = tempfile::tempdir().unwrap();
    let mut compatible_context = ProjectContext::new("compatible", compatible.path().to_path_buf());
    compatible_context.layout.workflow_state_root = Some(".app/compat/workflows".into());
    let compatible_preferences = WorkflowPreferences::default();
    compatible_preferences
        .remember(
            &compatible_context,
            "compatible-identity",
            "compatible-revision",
            &WorkflowPersistenceMode::Persistent,
            preference(),
        )
        .unwrap();
    assert!(
        compatible
            .path()
            .join(".app/compat/workflows/preferences.json")
            .is_file(),
        "compatible preferences must follow ProjectLayout.workflow_state_root",
    );
    assert!(!compatible
        .path()
        .join(".app/workflows/preferences.json")
        .exists());
    assert_eq!(
        compatible_preferences
            .load(
                &compatible_context,
                "compatible-identity",
                "compatible-revision",
                &WorkflowPersistenceMode::Persistent,
            )
            .unwrap()
            .len(),
        1,
    );

    let memory = tempfile::tempdir().unwrap();
    let mut memory_context = ProjectContext::new("memory", memory.path().to_path_buf());
    memory_context.layout.workflow_state_root = None;
    let memory_preferences = WorkflowPreferences::default();
    memory_preferences
        .remember(
            &memory_context,
            "memory-identity",
            "memory-revision",
            &WorkflowPersistenceMode::Persistent,
            preference(),
        )
        .unwrap();
    assert!(!memory.path().join(".app").exists());
    assert_eq!(
        memory_preferences
            .load(
                &memory_context,
                "memory-identity",
                "memory-revision",
                &WorkflowPersistenceMode::Persistent,
            )
            .unwrap()
            .len(),
        1,
    );
    assert!(memory_preferences
        .load(
            &memory_context,
            "memory-identity",
            "replacement-revision",
            &WorkflowPersistenceMode::Persistent,
        )
        .unwrap()
        .is_empty());
}

#[test]
fn compatible_preferences_ignore_and_preserve_historical_native_path() {
    let root = tempfile::tempdir().unwrap();
    let legacy_context = ProjectContext::new("legacy-wrong-root", root.path().to_path_buf());
    let legacy_preferences = WorkflowPreferences::default();
    legacy_preferences
        .remember(
            &legacy_context,
            "legacy-identity",
            "legacy-revision",
            &WorkflowPersistenceMode::Persistent,
            WorkflowPreference {
                kind: WorkflowKind::HealthCheck,
                scope: WorkflowScope::HealthCheck {
                    mode: HealthCheckMode::LocalQuick,
                },
                route: None,
                baseline_fingerprint: "c".repeat(64),
                preparation_fingerprint: "d".repeat(64),
                saved_at: String::new(),
            },
        )
        .unwrap();
    let historical_path = root.path().join(".app/workflows/preferences.json");
    let historical_bytes = std::fs::read(&historical_path).unwrap();

    let mut compatible_context = ProjectContext::new("compatible", root.path().to_path_buf());
    compatible_context.layout.workflow_state_root = Some(".app/compat/workflows".into());
    let preferences = WorkflowPreferences::default();
    assert!(preferences
        .load(
            &compatible_context,
            "compatible-identity",
            "compatible-revision",
            &WorkflowPersistenceMode::Persistent,
        )
        .unwrap()
        .is_empty());
    preferences
        .remember(
            &compatible_context,
            "compatible-identity",
            "compatible-revision",
            &WorkflowPersistenceMode::Persistent,
            WorkflowPreference {
                kind: WorkflowKind::HealthCheck,
                scope: WorkflowScope::HealthCheck {
                    mode: HealthCheckMode::LocalQuick,
                },
                route: None,
                baseline_fingerprint: "e".repeat(64),
                preparation_fingerprint: "f".repeat(64),
                saved_at: String::new(),
            },
        )
        .unwrap();

    assert_eq!(std::fs::read(historical_path).unwrap(), historical_bytes);
    assert!(root
        .path()
        .join(".app/compat/workflows/preferences.json")
        .is_file());
}
