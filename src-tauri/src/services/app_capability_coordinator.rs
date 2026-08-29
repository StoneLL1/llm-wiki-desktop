use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::BackendError;
use crate::models::app_capability::{
    AppCapabilityContinuation, AppCapabilityContinuationState, AppCapabilityDisplayState,
    AppCapabilityDistribution, AppCapabilityDistributionState, AppCapabilityInstallation,
    AppCapabilityInstallationState, AppCapabilityOperation, AppCapabilityOperationState,
    AppCapabilityUpdate, AppCapabilityUpdateState, AppCapabilityView,
    APP_CAPABILITY_CONTINUATION_SCHEMA_VERSION,
};
use crate::models::task::{BackendTask, TaskOperation, TaskStatus};
use crate::services::import_v2::capability_installer::{
    catalog_availability, catalog_entry, CapabilityCatalogAvailability, CapabilityCatalogEntry,
};
use crate::services::import_v2::capability_runtime::{target_triple, ImportCapabilityRuntime};
use crate::services::import_v2::product_capability::ProductCapabilityManifest;
use crate::services::import_v2::runner_confinement::{
    capability_installation_mutations_enabled, require_capability_installation_confinement,
    APP_CAPABILITY_CONFINEMENT_UNAVAILABLE,
};
use crate::services::FileStore;
use crate::tasks::TaskService;

const CONTINUATIONS_FILE: &str = "continuations-v1.json";

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct InstallKey {
    capability_id: String,
    version: String,
    target_triple: String,
    archive_identity: String,
}

#[derive(Default)]
struct CoordinatorState {
    in_flight: HashMap<InstallKey, String>,
    continuations: Vec<AppCapabilityContinuation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ContinuationStoreV1 {
    schema_version: u32,
    continuations: Vec<AppCapabilityContinuation>,
}

#[derive(Default)]
pub struct AppCapabilityCoordinator {
    root: RwLock<Option<PathBuf>>,
    state: Mutex<CoordinatorState>,
}

impl AppCapabilityCoordinator {
    pub fn initialize(&self, root: &Path, tasks: &TaskService) -> Result<(), BackendError> {
        std::fs::create_dir_all(root).map_err(|error| {
            coordinator_error(
                "APP_CAPABILITY_STATE_UNAVAILABLE",
                "The application capability state directory is unavailable.",
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
        let canonical_root = root.canonicalize().map_err(|error| {
            coordinator_error(
                "APP_CAPABILITY_STATE_UNAVAILABLE",
                "The application capability state directory cannot be resolved.",
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
        let recovered = tasks
            .recover_app_tasks(&canonical_root)
            .map_err(|message| {
                coordinator_error("APP_CAPABILITY_TASK_RECOVERY_FAILED", &message)
            })?;
        let continuations = load_continuations(&canonical_root)?;
        let mut in_flight = HashMap::new();
        for task in recovered {
            if !matches!(
                task.status,
                TaskStatus::Queued
                    | TaskStatus::Running
                    | TaskStatus::Cancelling
                    | TaskStatus::Interrupted
            ) {
                continue;
            }
            if let Some(key) = task_install_key(&task) {
                in_flight.insert(key, task.id);
            }
        }
        *self.root.write().map_err(|_| coordinator_locked())? = Some(canonical_root);
        *self.state.lock().map_err(|_| coordinator_locked())? = CoordinatorState {
            in_flight,
            continuations,
        };
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.root.read().ok().is_some_and(|root| root.is_some())
    }

    pub fn join_or_create_install(
        &self,
        tasks: &TaskService,
        entry: &CapabilityCatalogEntry,
        expected_version: &str,
        acknowledgement_version: &str,
    ) -> Result<(BackendTask, bool), BackendError> {
        require_capability_installation_confinement()?;
        if entry.version != expected_version {
            return Err(coordinator_error(
                "APP_CAPABILITY_VERSION_STALE",
                "The published capability version changed. Review the installation again.",
            ));
        }
        let expected_acknowledgement = app_capability_acknowledgement_version(entry);
        if acknowledgement_version != expected_acknowledgement {
            return Err(coordinator_error(
                "APP_CAPABILITY_ACKNOWLEDGEMENT_STALE",
                "The capability license or archive identity changed. Review the installation again.",
            ));
        }
        let root = self
            .root
            .read()
            .map_err(|_| coordinator_locked())?
            .clone()
            .ok_or_else(|| {
                coordinator_error(
                    "APP_CAPABILITY_STATE_UNAVAILABLE",
                    "Application capability state is not initialized.",
                )
            })?;
        let key = install_key(entry);
        let mut state = self.state.lock().map_err(|_| coordinator_locked())?;
        if let Some(existing_id) = state.in_flight.get(&key).cloned() {
            if let Some(existing) = tasks.get_task(&existing_id).filter(is_joinable_task) {
                return Ok((existing, false));
            }
            state.in_flight.remove(&key);
        }
        let task = tasks
            .create_app_capability_install_task(
                root,
                format!("Install {} {}", entry.capability_id, entry.version),
                entry.capability_id.clone(),
                entry.version.clone(),
                entry.target_triple.clone(),
                key.archive_identity.clone(),
            )
            .map_err(|message| coordinator_error("APP_CAPABILITY_TASK_CREATE_FAILED", &message))?;
        state.in_flight.insert(key, task.id.clone());
        Ok((task, true))
    }

    pub fn register_continuation(
        &self,
        mut continuation: AppCapabilityContinuation,
    ) -> Result<AppCapabilityContinuation, BackendError> {
        validate_continuation(&continuation)?;
        let root = self.require_root()?;
        let mut state = self.state.lock().map_err(|_| coordinator_locked())?;
        if let Some(existing) = state.continuations.iter().find(|candidate| {
            candidate.capability_id == continuation.capability_id
                && candidate.project_id == continuation.project_id
                && candidate.session_id == continuation.session_id
                && candidate.item_id == continuation.item_id
                && candidate.requirement_revision == continuation.requirement_revision
                && candidate.requested_route == continuation.requested_route
                && candidate.recovery_action == continuation.recovery_action
                && candidate.asr_profile == continuation.asr_profile
                && candidate.recognition_language == continuation.recognition_language
                && !matches!(
                    candidate.state,
                    AppCapabilityContinuationState::Succeeded
                        | AppCapabilityContinuationState::Cancelled
                )
        }) {
            return Ok(existing.clone());
        }
        continuation.schema_version = APP_CAPABILITY_CONTINUATION_SCHEMA_VERSION;
        let mut next = state.continuations.clone();
        next.push(continuation.clone());
        persist_continuations(&root, &next)?;
        state.continuations = next;
        Ok(continuation)
    }

    pub fn bind_continuation_task(
        &self,
        continuation_id: &str,
        task_id: &str,
    ) -> Result<AppCapabilityContinuation, BackendError> {
        self.update_continuation(continuation_id, |continuation| {
            continuation.task_id = Some(task_id.to_owned());
            continuation.state = AppCapabilityContinuationState::Registered;
            continuation.detail_code = None;
        })
    }

    pub fn update_continuation_state(
        &self,
        continuation_id: &str,
        next: AppCapabilityContinuationState,
        detail_code: Option<String>,
    ) -> Result<AppCapabilityContinuation, BackendError> {
        self.update_continuation(continuation_id, |continuation| {
            continuation.state = next;
            continuation.detail_code = detail_code;
        })
    }

    pub fn update_continuation_states(
        &self,
        updates: &[(String, AppCapabilityContinuationState, Option<String>)],
    ) -> Result<Vec<AppCapabilityContinuation>, BackendError> {
        let root = self.require_root()?;
        let mut state = self.state.lock().map_err(|_| coordinator_locked())?;
        let mut next = state.continuations.clone();
        let mut results = Vec::with_capacity(updates.len());
        for (continuation_id, next_state, detail_code) in updates {
            let continuation = next
                .iter_mut()
                .find(|continuation| continuation.continuation_id == *continuation_id)
                .ok_or_else(|| {
                    coordinator_error(
                        "APP_CAPABILITY_CONTINUATION_NOT_FOUND",
                        "The capability continuation no longer exists.",
                    )
                })?;
            continuation.state = next_state.clone();
            continuation.detail_code = detail_code.clone();
            results.push(continuation.clone());
        }
        persist_continuations(&root, &next)?;
        state.continuations = next;
        Ok(results)
    }

    pub fn bind_registered_continuations(
        &self,
        capability_id: &str,
        task_id: &str,
    ) -> Result<Vec<AppCapabilityContinuation>, BackendError> {
        let root = self.require_root()?;
        let mut state = self.state.lock().map_err(|_| coordinator_locked())?;
        let mut next = state.continuations.clone();
        let mut results = Vec::new();
        for continuation in next.iter_mut().filter(|continuation| {
            continuation.capability_id == capability_id
                && matches!(
                    continuation.state,
                    AppCapabilityContinuationState::Registered
                        | AppCapabilityContinuationState::Deferred
                        | AppCapabilityContinuationState::Failed
                )
        }) {
            continuation.task_id = Some(task_id.to_owned());
            continuation.state = AppCapabilityContinuationState::Registered;
            continuation.detail_code = None;
            results.push(continuation.clone());
        }
        if results.is_empty() {
            return Ok(results);
        }
        persist_continuations(&root, &next)?;
        state.continuations = next;
        Ok(results)
    }

    pub fn continuations_for(&self, capability_id: &str) -> Vec<AppCapabilityContinuation> {
        self.state
            .lock()
            .map(|state| {
                state
                    .continuations
                    .iter()
                    .filter(|continuation| continuation.capability_id == capability_id)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn continuations_for_task(&self, task_id: &str) -> Vec<AppCapabilityContinuation> {
        self.state
            .lock()
            .map(|state| {
                state
                    .continuations
                    .iter()
                    .filter(|continuation| continuation.task_id.as_deref() == Some(task_id))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn settle_task(&self, task: &BackendTask) {
        let Some(key) = task_install_key(task) else {
            return;
        };
        if let Ok(mut state) = self.state.lock() {
            if state.in_flight.get(&key).is_some_and(|id| id == &task.id) {
                state.in_flight.remove(&key);
            }
        }
    }

    pub fn list_capabilities(
        &self,
        runtime: &ImportCapabilityRuntime,
        tasks: &TaskService,
    ) -> Result<Vec<AppCapabilityView>, BackendError> {
        let manifest = ProductCapabilityManifest::embedded().map_err(|message| {
            coordinator_error("APP_CAPABILITY_PRODUCT_MANIFEST_INVALID", &message)
        })?;
        let target = target_triple();
        let runtime_statuses = runtime.statuses();
        let app_tasks = tasks.list_app_tasks(None);
        let active_root = tasks.current_project_root();
        let state = self.state.lock().map_err(|_| coordinator_locked())?;
        let catalog_state = catalog_availability();
        let mut views = Vec::with_capacity(manifest.definitions.len());
        for definition in manifest.definitions {
            let entry = catalog_entry(&definition.capability_id, &target);
            let (install_allowed, install_blocked_reason_code) =
                capability_install_policy(entry.is_some());
            let distribution_state = if definition.distribution_tier != "published" {
                AppCapabilityDistributionState::Unsupported
            } else if entry.is_some() {
                AppCapabilityDistributionState::Published
            } else if catalog_state == CapabilityCatalogAvailability::CatalogUnavailable {
                AppCapabilityDistributionState::SourceCatalogEmpty
            } else {
                AppCapabilityDistributionState::NotPublishedForTarget
            };
            let relevant_statuses = runtime_statuses
                .iter()
                .filter(|status| {
                    status.capability_id == definition.capability_id
                        && definition.routes.contains(&status.route)
                })
                .collect::<Vec<_>>();
            let all_routes_healthy = !definition.routes.is_empty()
                && definition.routes.iter().all(|route| {
                    relevant_statuses
                        .iter()
                        .any(|status| status.route == *route && status.available)
                });
            let healthy_version = if all_routes_healthy {
                let versions = relevant_statuses
                    .iter()
                    .filter_map(|status| status.healthy_version.clone())
                    .collect::<Vec<_>>();
                versions
                    .first()
                    .filter(|first| versions.iter().all(|version| version == *first))
                    .cloned()
            } else {
                None
            };
            let installed_any = relevant_statuses.iter().any(|status| status.available)
                || runtime
                    .install_root()
                    .is_some_and(|root| root.join(&definition.capability_id).exists());
            let installation_state = if all_routes_healthy {
                AppCapabilityInstallationState::Healthy
            } else if installed_any {
                AppCapabilityInstallationState::Unhealthy
            } else {
                AppCapabilityInstallationState::Absent
            };
            let active_task = app_tasks.iter().find(|task| {
                task_capability_id(task) == Some(definition.capability_id.as_str())
                    && is_joinable_task(task)
            });
            let latest_task = app_tasks
                .iter()
                .filter(|task| task_capability_id(task) == Some(definition.capability_id.as_str()))
                .max_by(|left, right| left.updated_at.cmp(&right.updated_at));
            let operation = operation_from_task(active_task.or(latest_task));
            let update_available =
                healthy_version
                    .as_ref()
                    .zip(entry.as_ref())
                    .is_some_and(|(healthy, entry)| {
                        semver::Version::parse(healthy).ok()
                            < semver::Version::parse(&entry.version).ok()
                    });
            let update_state = if operation.state == Some(AppCapabilityOperationState::Failed)
                && healthy_version.is_some()
            {
                AppCapabilityUpdateState::RollbackRestored
            } else if active_task.is_some() && healthy_version.is_some() {
                AppCapabilityUpdateState::InProgress
            } else if update_available {
                AppCapabilityUpdateState::Available
            } else {
                AppCapabilityUpdateState::None
            };
            let display_state = display_state(
                &distribution_state,
                &installation_state,
                update_available,
                &update_state,
                operation.state.as_ref(),
            );
            let waiting_count = state
                .continuations
                .iter()
                .filter(|continuation| {
                    continuation.capability_id == definition.capability_id
                        && active_root.as_ref().is_some_and(|root| {
                            root.to_string_lossy().replace('\\', "/")
                                == continuation.project_root_path.replace('\\', "/")
                        })
                        && matches!(
                            continuation.state,
                            AppCapabilityContinuationState::Registered
                                | AppCapabilityContinuationState::Deferred
                        )
                })
                .count() as u64;
            let error_code = match distribution_state {
                AppCapabilityDistributionState::SourceCatalogEmpty => {
                    Some("APP_CAPABILITY_CATALOG_UNAVAILABLE".into())
                }
                AppCapabilityDistributionState::NotPublishedForTarget => {
                    Some("APP_CAPABILITY_NOT_PUBLISHED_FOR_TARGET".into())
                }
                AppCapabilityDistributionState::Unsupported => {
                    Some("APP_CAPABILITY_UNSUPPORTED_BY_APP".into())
                }
                AppCapabilityDistributionState::Published => operation.error_code.clone(),
            };
            views.push(AppCapabilityView {
                capability_id: definition.capability_id,
                name_key: definition.name_key,
                purpose_key: definition.purpose_key,
                category: definition.category,
                routes: definition.routes,
                formats: definition.formats.extensions,
                platform_content_types: definition.formats.platform_content_types,
                target_triple: target.clone(),
                target_version: entry.as_ref().map(|entry| entry.version.clone()),
                acknowledgement_version: entry.as_ref().map(app_capability_acknowledgement_version),
                install_allowed,
                install_blocked_reason_code,
                distribution: AppCapabilityDistribution {
                    state: distribution_state,
                    error_code: error_code.clone(),
                },
                installation: AppCapabilityInstallation {
                    state: installation_state,
                    healthy_version,
                },
                operation,
                update: AppCapabilityUpdate {
                    state: update_state,
                    available_version: update_available
                        .then(|| entry.as_ref().map(|entry| entry.version.clone()))
                        .flatten(),
                },
                display_state,
                compressed_bytes: entry.as_ref().map(|entry| entry.compressed_bytes),
                installed_bytes: entry.as_ref().map(|entry| entry.installed_bytes),
                model_bytes: entry.as_ref().and_then(|entry| entry.model_bytes),
                license_expression: definition.license_policy.expression,
                third_party_notices: definition.license_policy.third_party_notices,
                runtime_network: definition.runtime.network,
                runtime_subprocess: definition.runtime.subprocess,
                runtime_filesystem: definition.runtime.filesystem,
                active_task_id: active_task.map(|task| task.id.clone()),
                current_project_waiting_count: waiting_count,
                error_code,
            });
        }
        Ok(views)
    }

    fn update_continuation(
        &self,
        continuation_id: &str,
        update: impl FnOnce(&mut AppCapabilityContinuation),
    ) -> Result<AppCapabilityContinuation, BackendError> {
        let root = self.require_root()?;
        let mut state = self.state.lock().map_err(|_| coordinator_locked())?;
        let mut next = state.continuations.clone();
        let continuation = next
            .iter_mut()
            .find(|continuation| continuation.continuation_id == continuation_id)
            .ok_or_else(|| {
                coordinator_error(
                    "APP_CAPABILITY_CONTINUATION_NOT_FOUND",
                    "The capability continuation no longer exists.",
                )
            })?;
        update(continuation);
        let result = continuation.clone();
        persist_continuations(&root, &next)?;
        state.continuations = next;
        Ok(result)
    }

    fn require_root(&self) -> Result<PathBuf, BackendError> {
        self.root
            .read()
            .map_err(|_| coordinator_locked())?
            .clone()
            .ok_or_else(|| {
                coordinator_error(
                    "APP_CAPABILITY_STATE_UNAVAILABLE",
                    "Application capability state is not initialized.",
                )
            })
    }
}

pub fn app_capability_acknowledgement_version(entry: &CapabilityCatalogEntry) -> String {
    let identity = format!(
        "ack-v1\0{}\0{}\0{}\0{}\0{}",
        entry.capability_id,
        entry.version,
        entry.target_triple,
        entry.archive_sha256.to_ascii_lowercase(),
        entry.license
    );
    format!("ack-v1-{:x}", Sha256::digest(identity.as_bytes()))
}

pub fn app_capability_archive_identity(entry: &CapabilityCatalogEntry) -> String {
    install_key(entry).archive_identity
}

fn install_key(entry: &CapabilityCatalogEntry) -> InstallKey {
    let identity = format!(
        "{}\0{}\0{}\0{}",
        entry.capability_id,
        entry.version,
        entry.target_triple,
        entry.archive_sha256.to_ascii_lowercase()
    );
    InstallKey {
        capability_id: entry.capability_id.clone(),
        version: entry.version.clone(),
        target_triple: entry.target_triple.clone(),
        archive_identity: format!("{:x}", Sha256::digest(identity.as_bytes())),
    }
}

fn task_install_key(task: &BackendTask) -> Option<InstallKey> {
    match task.operation.as_ref()? {
        TaskOperation::AppCapabilityInstall {
            capability_id,
            version,
            target_triple,
            archive_identity,
        } => Some(InstallKey {
            capability_id: capability_id.clone(),
            version: version.clone(),
            target_triple: target_triple.clone(),
            archive_identity: archive_identity.clone(),
        }),
        _ => None,
    }
}

fn task_capability_id(task: &BackendTask) -> Option<&str> {
    match task.operation.as_ref()? {
        TaskOperation::AppCapabilityInstall { capability_id, .. } => Some(capability_id),
        _ => None,
    }
}

fn is_joinable_task(task: &BackendTask) -> bool {
    matches!(
        task.status,
        TaskStatus::Queued | TaskStatus::Running | TaskStatus::Cancelling | TaskStatus::Interrupted
    )
}

fn operation_from_task(task: Option<&BackendTask>) -> AppCapabilityOperation {
    let Some(task) = task else {
        return AppCapabilityOperation::default();
    };
    let state = match task.status {
        TaskStatus::Queued => Some(AppCapabilityOperationState::Queued),
        TaskStatus::Running => Some(
            match task
                .progress
                .as_ref()
                .and_then(|progress| progress.label.as_deref())
            {
                Some("capability.verifying") => AppCapabilityOperationState::Verifying,
                Some("capability.installing") => AppCapabilityOperationState::Installing,
                Some("capability.health_check") => AppCapabilityOperationState::HealthChecking,
                Some("capability.activating") => AppCapabilityOperationState::Activating,
                Some("capability.recovering") => AppCapabilityOperationState::Recovering,
                _ => AppCapabilityOperationState::Downloading,
            },
        ),
        TaskStatus::Interrupted => Some(AppCapabilityOperationState::Paused),
        TaskStatus::Cancelling | TaskStatus::Cancelled => {
            Some(AppCapabilityOperationState::Cancelled)
        }
        TaskStatus::Succeeded => Some(AppCapabilityOperationState::Succeeded),
        TaskStatus::Failed => Some(AppCapabilityOperationState::Failed),
        TaskStatus::WaitingForConfirmation => Some(AppCapabilityOperationState::Queued),
    };
    AppCapabilityOperation {
        state,
        task_id: Some(task.id.clone()),
        progress_current: task.progress.as_ref().map(|progress| progress.current),
        progress_total: task.progress.as_ref().and_then(|progress| progress.total),
        error_code: task.error.as_ref().map(|error| error.code.clone()),
    }
}

fn display_state(
    distribution: &AppCapabilityDistributionState,
    installation: &AppCapabilityInstallationState,
    update_available: bool,
    update: &AppCapabilityUpdateState,
    operation: Option<&AppCapabilityOperationState>,
) -> AppCapabilityDisplayState {
    if *update == AppCapabilityUpdateState::RollbackRestored {
        return AppCapabilityDisplayState::RolledBack;
    }
    if let Some(operation) = operation {
        return match operation {
            AppCapabilityOperationState::Queued => AppCapabilityDisplayState::Queued,
            AppCapabilityOperationState::Downloading => AppCapabilityDisplayState::Downloading,
            AppCapabilityOperationState::Verifying => AppCapabilityDisplayState::Verifying,
            AppCapabilityOperationState::Installing | AppCapabilityOperationState::Activating => {
                AppCapabilityDisplayState::Installing
            }
            AppCapabilityOperationState::HealthChecking => {
                AppCapabilityDisplayState::HealthChecking
            }
            AppCapabilityOperationState::Paused | AppCapabilityOperationState::Recovering => {
                AppCapabilityDisplayState::Paused
            }
            AppCapabilityOperationState::Failed => AppCapabilityDisplayState::FailedRecoverable,
            AppCapabilityOperationState::Cancelled | AppCapabilityOperationState::Succeeded => {
                if *installation == AppCapabilityInstallationState::Healthy {
                    AppCapabilityDisplayState::InstalledHealthy
                } else {
                    AppCapabilityDisplayState::InstallAvailable
                }
            }
        };
    }
    match distribution {
        AppCapabilityDistributionState::SourceCatalogEmpty => {
            AppCapabilityDisplayState::CatalogUnavailable
        }
        AppCapabilityDistributionState::NotPublishedForTarget => {
            AppCapabilityDisplayState::NotPublishedForTarget
        }
        AppCapabilityDistributionState::Unsupported => AppCapabilityDisplayState::UnsupportedByApp,
        AppCapabilityDistributionState::Published if update_available => {
            AppCapabilityDisplayState::UpdateAvailable
        }
        AppCapabilityDistributionState::Published
            if *installation == AppCapabilityInstallationState::Healthy =>
        {
            AppCapabilityDisplayState::InstalledHealthy
        }
        AppCapabilityDistributionState::Published => AppCapabilityDisplayState::InstallAvailable,
    }
}

fn validate_continuation(continuation: &AppCapabilityContinuation) -> Result<(), BackendError> {
    let required = [
        continuation.continuation_id.as_str(),
        continuation.capability_id.as_str(),
        continuation.project_id.as_str(),
        continuation.project_root_path.as_str(),
        continuation.canonical_identity_key.as_str(),
        continuation.identity_revision.as_str(),
        continuation.authority_revision.as_str(),
        continuation.session_id.as_str(),
        continuation.item_id.as_str(),
        continuation.requirement_revision.as_str(),
        continuation.requested_route.as_str(),
        continuation.created_at.as_str(),
    ];
    if continuation.schema_version != APP_CAPABILITY_CONTINUATION_SCHEMA_VERSION
        || required.iter().any(|value| value.trim().is_empty())
        || continuation.requested_route.contains("://")
    {
        return Err(coordinator_error(
            "APP_CAPABILITY_CONTINUATION_INVALID",
            "The capability continuation is invalid.",
        ));
    }
    Ok(())
}

fn load_continuations(root: &Path) -> Result<Vec<AppCapabilityContinuation>, BackendError> {
    let path = root.join(CONTINUATIONS_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(coordinator_error(
                "APP_CAPABILITY_CONTINUATION_READ_FAILED",
                &format!("Capability continuations could not be read: {error}"),
            ))
        }
    };
    let store: ContinuationStoreV1 = serde_json::from_str(&text).map_err(|error| {
        coordinator_error(
            "APP_CAPABILITY_CONTINUATION_INVALID",
            &format!("Capability continuation state is invalid: {error}"),
        )
    })?;
    if store.schema_version != APP_CAPABILITY_CONTINUATION_SCHEMA_VERSION {
        return Err(coordinator_error(
            "APP_CAPABILITY_CONTINUATION_SCHEMA_UNSUPPORTED",
            "Capability continuation state uses an unsupported schema.",
        ));
    }
    for continuation in &store.continuations {
        validate_continuation(continuation)?;
    }
    Ok(store.continuations)
}

fn persist_continuations(
    root: &Path,
    continuations: &[AppCapabilityContinuation],
) -> Result<(), BackendError> {
    FileStore.write_json_atomic_absolute(
        root,
        &root.join(CONTINUATIONS_FILE),
        &ContinuationStoreV1 {
            schema_version: APP_CAPABILITY_CONTINUATION_SCHEMA_VERSION,
            continuations: continuations.to_vec(),
        },
    )
}

fn coordinator_locked() -> BackendError {
    coordinator_error(
        "APP_CAPABILITY_COORDINATOR_LOCKED",
        "The application capability coordinator is unavailable.",
    )
}

fn coordinator_error(code: &str, message: &str) -> BackendError {
    BackendError::new(code, message, true, true)
}

fn capability_install_policy(catalog_entry_present: bool) -> (bool, Option<String>) {
    let install_allowed = catalog_entry_present && capability_installation_mutations_enabled();
    let blocked_reason = (catalog_entry_present && !install_allowed)
        .then(|| APP_CAPABILITY_CONFINEMENT_UNAVAILABLE.to_owned());
    (install_allowed, blocked_reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_catalog_entry_is_non_installable_while_confinement_is_unavailable() {
        let (install_allowed, blocked_reason) = capability_install_policy(true);
        assert!(!install_allowed);
        assert_eq!(
            blocked_reason.as_deref(),
            Some(APP_CAPABILITY_CONFINEMENT_UNAVAILABLE)
        );
    }
}
