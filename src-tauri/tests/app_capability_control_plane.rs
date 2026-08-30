use std::fs;

use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::app_capability::{
    AppCapabilityContinuation, AppCapabilityContinuationState, AppCapabilityDisplayState,
    AppCapabilityDistribution, AppCapabilityDistributionState, AppCapabilityInstallation,
    AppCapabilityInstallationState, AppCapabilityOperation, AppCapabilityOperationState,
    AppCapabilityUpdate, AppCapabilityUpdateState, AppCapabilityView,
};
use llm_wiki_desktop_lib::models::task::{BackendTask, TaskStatus};
use llm_wiki_desktop_lib::services::import_v2::capability_installer::CapabilityCatalogEntry;
use llm_wiki_desktop_lib::services::import_v2::capability_runtime::ImportCapabilityRuntime;
use llm_wiki_desktop_lib::services::{
    app_capability_acknowledgement_version, AppCapabilityCoordinator,
};
use llm_wiki_desktop_lib::tasks::TaskService;
use tempfile::tempdir;

fn fixture_catalog_entry() -> CapabilityCatalogEntry {
    CapabilityCatalogEntry {
        capability_id: "browser-runtime".into(),
        version: "1.2.3".into(),
        target_triple: "x86_64-pc-windows-msvc".into(),
        url: "https://example.invalid/releases/browser-runtime.zip".into(),
        archive_sha256: "a".repeat(64),
        manifest_sha256: "b".repeat(64),
        signing_key_id: "llm-wiki-capability-v1".into(),
        compressed_bytes: 123,
        installed_bytes: 456,
        model_bytes: None,
        license: "MIT".into(),
    }
}

fn create_fixture_app_task(
    root: &std::path::Path,
    tasks: &TaskService,
    entry: &CapabilityCatalogEntry,
) -> BackendTask {
    tasks
        .create_app_capability_install_task(
            root.to_path_buf(),
            format!("Install {} {}", entry.capability_id, entry.version),
            entry.capability_id.clone(),
            entry.version.clone(),
            entry.target_triple.clone(),
            "fixture-archive-identity".into(),
        )
        .unwrap()
}

fn continuation() -> AppCapabilityContinuation {
    AppCapabilityContinuation {
        schema_version: 1,
        continuation_id: "continuation-1".into(),
        capability_id: "browser-runtime".into(),
        project_id: "project-a".into(),
        project_root_path: "D:/knowledge/project-a".into(),
        canonical_identity_key: "identity-key".into(),
        identity_revision: "identity-revision".into(),
        authority_revision: "authority-revision".into(),
        session_id: "session-a".into(),
        item_id: "item-a".into(),
        requirement_revision: "requirement-revision".into(),
        requested_route: "web.generic.browser".into(),
        recovery_action: None,
        asr_profile: None,
        recognition_language: None,
        created_at: "2026-08-30T00:00:00Z".into(),
        state: AppCapabilityContinuationState::Registered,
        task_id: None,
        detail_code: None,
    }
}

#[test]
fn app_capability_view_serializes_orthogonal_facts() {
    let view = AppCapabilityView {
        capability_id: "browser-runtime".into(),
        name_key: "importV2.capabilityName.browserRuntime".into(),
        purpose_key: "importV2.capabilityPurpose.browserRuntime".into(),
        category: "web".into(),
        routes: vec!["web.generic.browser".into()],
        formats: vec!["html".into()],
        platform_content_types: vec!["web_page".into()],
        target_triple: "x86_64-pc-windows-msvc".into(),
        publisher_key_id: Some("llm-wiki-capability-v1".into()),
        source_domain: Some("github.com".into()),
        target_version: Some("1.2.3".into()),
        acknowledgement_version: Some("ack-v1".into()),
        install_allowed: true,
        install_blocked_reason_code: None,
        distribution: AppCapabilityDistribution {
            state: AppCapabilityDistributionState::Published,
            error_code: None,
        },
        installation: AppCapabilityInstallation {
            state: AppCapabilityInstallationState::Healthy,
            healthy_version: Some("1.2.2".into()),
        },
        operation: AppCapabilityOperation::default(),
        update: AppCapabilityUpdate {
            state: AppCapabilityUpdateState::Available,
            available_version: Some("1.2.3".into()),
        },
        display_state: AppCapabilityDisplayState::UpdateAvailable,
        compressed_bytes: Some(123),
        installed_bytes: Some(456),
        model_bytes: None,
        license_expression: "MIT".into(),
        third_party_notices: Vec::new(),
        runtime_network: true,
        runtime_subprocess: true,
        runtime_filesystem: vec!["capability_root".into(), "item_staging".into()],
        active_task_id: None,
        current_project_waiting_count: 0,
        error_code: None,
    };

    let value = serde_json::to_value(view).unwrap();
    assert_eq!(value["distribution"]["state"], "published");
    assert_eq!(value["installation"]["state"], "healthy");
    assert_eq!(value["operation"]["state"], serde_json::Value::Null);
    assert_eq!(value["update"]["state"], "available");
    assert_eq!(value["displayState"], "update_available");
    assert_eq!(value["publisherKeyId"], "llm-wiki-capability-v1");
    assert_eq!(value["sourceDomain"], "github.com");
    assert_eq!(value["installAllowed"], true);
    assert_eq!(value["installBlockedReasonCode"], serde_json::Value::Null);
}

#[test]
fn coordinator_creates_one_install_task_for_a_reviewed_official_release() {
    let root = tempdir().unwrap();
    let tasks = TaskService::default();
    let coordinator = AppCapabilityCoordinator::default();
    coordinator.initialize(root.path(), &tasks).unwrap();
    let entry = fixture_catalog_entry();
    let (task, created) = coordinator
        .join_or_create_install(
            &tasks,
            &entry,
            &entry.version,
            &app_capability_acknowledgement_version(&entry),
        )
        .unwrap();

    assert!(created);
    assert_eq!(task.status, TaskStatus::Queued);
    assert_eq!(tasks.list_app_tasks(None), vec![task]);
    assert!(root.path().join("tasks").exists());

    let (joined, created_again) = coordinator
        .join_or_create_install(
            &tasks,
            &entry,
            &entry.version,
            &app_capability_acknowledgement_version(&entry),
        )
        .unwrap();
    assert!(!created_again);
    assert_eq!(joined.id, tasks.list_app_tasks(None)[0].id);
    assert_eq!(tasks.list_app_tasks(None).len(), 1);
}

#[test]
fn continuation_registry_is_versioned_restart_safe_and_secret_free() {
    let root = tempdir().unwrap();
    let tasks = TaskService::default();
    let coordinator = AppCapabilityCoordinator::default();
    coordinator.initialize(root.path(), &tasks).unwrap();
    coordinator.register_continuation(continuation()).unwrap();

    let persisted = fs::read_to_string(root.path().join("continuations-v1.json")).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&persisted).unwrap()["schemaVersion"],
        1,
    );
    assert!(!persisted.to_ascii_lowercase().contains("cookie"));
    assert!(!persisted.to_ascii_lowercase().contains("authorization"));
    assert!(!persisted.contains("example.invalid"));

    let recovered = AppCapabilityCoordinator::default();
    recovered
        .initialize(root.path(), &TaskService::default())
        .unwrap();
    let continuations = recovered.continuations_for("browser-runtime");
    assert_eq!(continuations, vec![continuation()]);
}

#[test]
fn capability_inventory_is_available_without_an_active_project() {
    let root = tempfile::tempdir().unwrap();
    let tasks = TaskService::default();
    let coordinator = AppCapabilityCoordinator::default();
    coordinator.initialize(root.path(), &tasks).unwrap();

    let views = coordinator
        .list_capabilities(&ImportCapabilityRuntime::default(), &tasks)
        .unwrap();

    assert!(!views.is_empty());
    assert!(views
        .iter()
        .all(|view| view.current_project_waiting_count == 0));
    assert!(views.iter().all(|view| view.publisher_key_id.is_none()));
    assert!(views
        .iter()
        .filter(|view| view.target_version.is_some())
        .all(|view| view.install_allowed && view.install_blocked_reason_code.is_none()));
}

#[test]
fn paused_app_install_recovers_as_interrupted_and_can_resume() {
    let root = tempfile::tempdir().unwrap();
    let tasks = TaskService::default();
    let coordinator = AppCapabilityCoordinator::default();
    coordinator.initialize(root.path(), &tasks).unwrap();
    let entry = fixture_catalog_entry();
    let task = create_fixture_app_task(root.path(), &tasks, &entry);

    let paused = tasks
        .request_app_task_pause(&task.id, &task.updated_at)
        .unwrap();
    assert!(tasks
        .request_app_task_pause(&task.id, &task.updated_at)
        .unwrap_err()
        .contains("revision is stale"));
    assert_eq!(
        tasks.get_task(&task.id).unwrap().status,
        TaskStatus::Interrupted
    );

    let restarted = TaskService::default();
    let recovered = restarted.recover_app_tasks(root.path()).unwrap();
    assert_eq!(recovered[0].status, TaskStatus::Interrupted);
    assert_eq!(paused.status, TaskStatus::Interrupted);
    let resumed = restarted
        .resume_app_task(&task.id, &recovered[0].updated_at)
        .unwrap();
    assert_eq!(resumed.status, TaskStatus::Queued);
    assert!(resumed.error.is_none());
    assert!(resumed.completed_at.is_none());
}

#[test]
fn failed_app_task_remains_visible_with_its_stable_error() {
    let root = tempdir().unwrap();
    let tasks = TaskService::default();
    let coordinator = AppCapabilityCoordinator::default();
    coordinator.initialize(root.path(), &tasks).unwrap();
    let entry = fixture_catalog_entry();
    let task = create_fixture_app_task(root.path(), &tasks, &entry);
    tasks
        .transition_status(&task.id, TaskStatus::Running)
        .unwrap();
    tasks
        .set_error(
            &task.id,
            BackendError::new(
                "APP_CAPABILITY_NETWORK_UNAVAILABLE",
                "The capability download is offline.",
                true,
                true,
            ),
        )
        .unwrap();
    tasks
        .transition_status(&task.id, TaskStatus::Failed)
        .unwrap();

    let views = coordinator
        .list_capabilities(&ImportCapabilityRuntime::default(), &tasks)
        .unwrap();
    let view = views
        .iter()
        .find(|view| view.capability_id == entry.capability_id)
        .unwrap();
    assert_eq!(
        view.operation.state,
        Some(AppCapabilityOperationState::Failed)
    );
    assert_eq!(
        view.operation.error_code.as_deref(),
        Some("APP_CAPABILITY_NETWORK_UNAVAILABLE")
    );
}

#[test]
fn registered_continuations_rebind_to_a_management_retry() {
    let root = tempdir().unwrap();
    let tasks = TaskService::default();
    let coordinator = AppCapabilityCoordinator::default();
    coordinator.initialize(root.path(), &tasks).unwrap();
    coordinator.register_continuation(continuation()).unwrap();
    let entry = fixture_catalog_entry();
    let first = create_fixture_app_task(root.path(), &tasks, &entry);
    coordinator
        .bind_registered_continuations(&entry.capability_id, &first.id)
        .unwrap();
    tasks
        .transition_status(&first.id, TaskStatus::Running)
        .unwrap();
    let failed = tasks
        .transition_status(&first.id, TaskStatus::Failed)
        .unwrap();
    coordinator.settle_task(&failed);

    let retry = create_fixture_app_task(root.path(), &tasks, &entry);
    coordinator
        .bind_registered_continuations(&entry.capability_id, &retry.id)
        .unwrap();
    let rebound = coordinator.continuations_for_task(&retry.id);
    assert_eq!(rebound.len(), 1);
    assert_eq!(rebound[0].continuation_id, "continuation-1");
}
