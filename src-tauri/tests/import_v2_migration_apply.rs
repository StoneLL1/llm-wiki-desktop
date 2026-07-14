use std::fs;
use std::path::Path;

use llm_wiki_desktop_lib::models::import_v2_migration::{
    MigrationConfirmation, MigrationStatus,
};
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::services::import_v2::migration::MigrationService;
use llm_wiki_desktop_lib::services::import_v2::ImportV2Service;
use llm_wiki_desktop_lib::services::GitService;
use llm_wiki_desktop_lib::tasks::task_model::CancellationToken;
use tempfile::tempdir;

fn project() -> tempfile::TempDir {
    let directory = tempdir().unwrap();
    let root = directory.path();
    fs::create_dir_all(root.join(".app/import-history")).unwrap();
    fs::create_dir_all(root.join("raw")).unwrap();
    fs::create_dir_all(root.join("wiki")).unwrap();
    fs::write(
        root.join(".app/source-index.json"),
        r#"{"schemaVersion":1,"records":[{"recordId":"legacy-1","rawPath":"raw/a.md","wikiPath":"wiki/a.md","sha256":"hash-1"}]}"#,
    )
    .unwrap();
    fs::write(root.join(".app/import-history/old.json"), r#"{"recordId":"old"}"#).unwrap();
    fs::write(root.join("raw/a.md"), "legacy raw").unwrap();
    fs::write(root.join("wiki/a.md"), "legacy wiki").unwrap();
    directory
}

fn prepare(
    root: &Path,
) -> (
    MigrationService,
    llm_wiki_desktop_lib::models::import_v2_migration::MigrationPlan,
    MigrationConfirmation,
    ProjectContext,
) {
    let service = MigrationService::default();
    let inventory = service.scan(root).unwrap();
    let plan = service.plan(root, &inventory).unwrap();
    let confirmation = MigrationConfirmation {
        plan_fingerprint: plan.fingerprint(),
        token: service.confirmation_token(&plan, &inventory.project_identity),
        acknowledge_no_git_rollback: true,
    };
    let context = ProjectContext::new("migration-test", root.to_path_buf());
    (service, plan, confirmation, context)
}

#[test]
fn apply_requires_confirmation_and_preserves_legacy_bytes_and_timestamps() {
    let directory = project();
    let root = directory.path();
    let legacy_index = fs::read(root.join(".app/source-index.json")).unwrap();
    let raw = fs::read(root.join("raw/a.md")).unwrap();
    let wiki = fs::read(root.join("wiki/a.md")).unwrap();
    let raw_time = fs::symlink_metadata(root.join("raw/a.md")).unwrap().modified().unwrap();
    let wiki_time = fs::symlink_metadata(root.join("wiki/a.md")).unwrap().modified().unwrap();
    let (service, plan, mut confirmation, context) = prepare(root);
    let core = ImportV2Service::default();
    let git = GitService::default();
    confirmation.token = "wrong-token".into();
    let error = service
        .apply_metadata(
            &core,
            &git,
            &context,
            &plan,
            confirmation,
            &CancellationToken::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_MIGRATION_CONFIRMATION_INVALID");
    assert!(!root.join(".app/import-v2-migration/report.json").exists());

    let (service, plan, confirmation, context) = prepare(root);
    let result = service
        .apply_metadata(
            &core,
            &git,
            &context,
            &plan,
            confirmation,
            &CancellationToken::new(),
        )
        .unwrap();
    assert_eq!(result.status, MigrationStatus::Applied);
    assert_eq!(fs::read(root.join(".app/source-index.json")).unwrap(), legacy_index);
    assert_eq!(fs::read(root.join("raw/a.md")).unwrap(), raw);
    assert_eq!(fs::read(root.join("wiki/a.md")).unwrap(), wiki);
    assert_eq!(fs::symlink_metadata(root.join("raw/a.md")).unwrap().modified().unwrap(), raw_time);
    assert_eq!(fs::symlink_metadata(root.join("wiki/a.md")).unwrap().modified().unwrap(), wiki_time);
    assert!(root.join(".app/source-index-v2.json").exists());
    assert!(root.join(".app/import-v2-migration/report.json").exists());
}

#[test]
fn no_git_requires_an_explicit_release_rollback_acknowledgement() {
    let directory = project();
    let root = directory.path();
    let (service, plan, mut confirmation, context) = prepare(root);
    confirmation.acknowledge_no_git_rollback = false;
    let error = service
        .apply_metadata(
            &ImportV2Service::default(),
            &GitService::default(),
            &context,
            &plan,
            confirmation,
            &CancellationToken::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_MIGRATION_GIT_CHECKPOINT_REQUIRED");
    assert!(!root.join(".app/source-index-v2.json").exists());
}

#[test]
fn external_markdown_edit_and_cancellation_fail_closed_without_new_metadata() {
    let directory = project();
    let root = directory.path();
    let (service, plan, confirmation, context) = prepare(root);
    fs::write(root.join("wiki/a.md"), "edited outside migration").unwrap();
    let error = service
        .apply_metadata(
            &ImportV2Service::default(),
            &GitService::default(),
            &context,
            &plan,
            confirmation,
            &CancellationToken::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_V2_MIGRATION_PLAN_STALE");
    assert!(!root.join(".app/source-index-v2.json").exists());

    let directory = project();
    let root = directory.path();
    let (service, plan, confirmation, context) = prepare(root);
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let result = service
        .apply_metadata(
            &ImportV2Service::default(),
            &GitService::default(),
            &context,
            &plan,
            confirmation,
            &cancellation,
        )
        .unwrap();
    assert_eq!(result.status, MigrationStatus::Cancelled);
    assert!(!root.join(".app/source-index-v2.json").exists());
}

#[test]
fn repeat_apply_converges_to_the_existing_applied_report() {
    let directory = project();
    let root = directory.path();
    let (service, plan, confirmation, context) = prepare(root);
    let first = service
        .apply_metadata(
            &ImportV2Service::default(),
            &GitService::default(),
            &context,
            &plan,
            confirmation.clone(),
            &CancellationToken::new(),
        )
        .unwrap();
    let second = service
        .apply_metadata(
            &ImportV2Service::default(),
            &GitService::default(),
            &context,
            &plan,
            confirmation,
            &CancellationToken::new(),
        )
        .unwrap();
    assert_eq!(first, second);
}
