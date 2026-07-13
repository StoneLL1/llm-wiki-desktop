use serde::{Deserialize, Serialize};

use super::import_v2_file::CapabilityRequirement;
use super::import_v2_migration::{LegacyHistoryEntry, LegacyHistoryWarning, MigrationStatus};

pub const IMPORT_V2_PREVIEW_MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetImportPreviewContentV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub candidate_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewContent {
    pub session_id: String,
    pub item_id: String,
    pub candidate_id: Option<String>,
    pub title: String,
    pub markdown: String,
    pub truncated: bool,
    pub total_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetImportFrontendReadinessV2Request {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportFrontendReadiness {
    pub backend_version: String,
    pub active: bool,
    pub migration_status: MigrationStatus,
    pub unfinished_session_id: Option<String>,
    pub legacy_history_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListImportHistoryV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportHistoryEntry {
    pub id: String,
    pub title: String,
    pub status: String,
    pub session_id: Option<String>,
    pub batch_id: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub completed_at: Option<String>,
    pub legacy_read_only: bool,
    pub available_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportHistoryPage {
    pub entries: Vec<ImportHistoryEntry>,
    pub legacy_read_only: Vec<LegacyHistoryEntry>,
    pub next_cursor: Option<String>,
    pub warnings: Vec<LegacyHistoryWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetImportCapabilityRequirementV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportCapabilityRequirement {
    pub requirement: CapabilityRequirement,
    pub route: String,
    pub available: bool,
    pub installable: bool,
    pub compressed_bytes: Option<u64>,
    pub installed_bytes: Option<u64>,
    pub model_bytes: Option<u64>,
    pub license: Option<String>,
    pub fallback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallImportCapabilityV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub capability_id: String,
    pub acknowledge_install: bool,
}
