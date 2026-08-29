use serde::{Deserialize, Serialize};

use crate::models::import_v2::{ImportAsrProfile, ImportRecoveryAction};

pub const APP_CAPABILITY_CONTINUATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppCapabilityDistributionState {
    Published,
    SourceCatalogEmpty,
    NotPublishedForTarget,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCapabilityDistribution {
    pub state: AppCapabilityDistributionState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppCapabilityInstallationState {
    Absent,
    Healthy,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCapabilityInstallation {
    pub state: AppCapabilityInstallationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub healthy_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppCapabilityOperationState {
    Queued,
    Downloading,
    Paused,
    Verifying,
    Installing,
    HealthChecking,
    Activating,
    Recovering,
    Failed,
    Cancelled,
    Succeeded,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCapabilityOperation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<AppCapabilityOperationState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_current: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_total: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppCapabilityUpdateState {
    None,
    Available,
    InProgress,
    RollbackRestored,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCapabilityUpdate {
    pub state: AppCapabilityUpdateState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppCapabilityDisplayState {
    InstalledHealthy,
    InstallAvailable,
    UpdateAvailable,
    Queued,
    Downloading,
    Verifying,
    Installing,
    HealthChecking,
    Paused,
    FailedRecoverable,
    RolledBack,
    NotPublishedForTarget,
    CatalogUnavailable,
    UnsupportedByApp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCapabilityView {
    pub capability_id: String,
    pub name_key: String,
    pub purpose_key: String,
    pub category: String,
    pub routes: Vec<String>,
    pub formats: Vec<String>,
    pub platform_content_types: Vec<String>,
    pub target_triple: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledgement_version: Option<String>,
    #[serde(default)]
    pub install_allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_blocked_reason_code: Option<String>,
    pub distribution: AppCapabilityDistribution,
    pub installation: AppCapabilityInstallation,
    pub operation: AppCapabilityOperation,
    pub update: AppCapabilityUpdate,
    pub display_state: AppCapabilityDisplayState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressed_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_bytes: Option<u64>,
    pub license_expression: String,
    pub third_party_notices: Vec<String>,
    pub runtime_network: bool,
    pub runtime_subprocess: bool,
    pub runtime_filesystem: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task_id: Option<String>,
    pub current_project_waiting_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppCapabilityContinuationState {
    Registered,
    Running,
    Succeeded,
    Deferred,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppCapabilityContinuation {
    pub schema_version: u32,
    pub continuation_id: String,
    pub capability_id: String,
    pub project_id: String,
    pub project_root_path: String,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub authority_revision: String,
    pub session_id: String,
    pub item_id: String,
    pub requirement_revision: String,
    pub requested_route: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_action: Option<ImportRecoveryAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asr_profile: Option<ImportAsrProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recognition_language: Option<String>,
    pub created_at: String,
    pub state: AppCapabilityContinuationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallAppCapabilityV1Request {
    pub capability_id: String,
    pub expected_version: String,
    pub acknowledgement_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppTaskControlScope {
    AppGlobal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppCapabilityTaskControlRequest {
    pub task_id: String,
    pub task_revision: String,
    pub scope: AppTaskControlScope,
}
