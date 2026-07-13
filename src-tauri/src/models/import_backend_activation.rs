use serde::{Deserialize, Serialize};

pub const IMPORT_BACKEND_ACTIVATION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportBackend {
    V2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportBackendActivation {
    pub schema_version: u32,
    pub active_backend: ImportBackend,
    pub core_contract_version: String,
    pub migration_report_fingerprint: String,
    pub activated_at: String,
    pub release_version: String,
    pub legacy_mutations_disabled: bool,
    pub rollback_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivationConfirmation {
    pub report_fingerprint: String,
    pub token: String,
    #[serde(default)]
    pub acknowledge_no_git_rollback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivationResult {
    pub record: ImportBackendActivation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<crate::models::git::GitCheckpoint>,
}
