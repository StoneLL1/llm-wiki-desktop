use serde::{Deserialize, Serialize};

use super::import_v2::{ImportAsrProfile, QualityReport};
use super::import_v2_file::CapabilityRequirement;
use super::import_v2_migration::{LegacyHistoryEntry, LegacyHistoryWarning, MigrationStatus};

pub const IMPORT_V2_PREVIEW_MAX_BYTES: u64 = 2 * 1024 * 1024;
pub const IMPORT_V2_WORKBENCH_PREFERENCES_PATH: &str = ".app/import-workbench.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImportWorkbenchSection {
    #[default]
    Workbench,
    Capabilities,
    History,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ImportQueuePreference {
    #[default]
    All,
    Active,
    Ready,
    NeedsAction,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportWorkbenchPreferences {
    pub schema_version: u32,
    pub active_section: ImportWorkbenchSection,
    pub queue_filter: ImportQueuePreference,
    pub workbench_scroll_top: u32,
    pub capabilities_scroll_top: u32,
    pub history_scroll_top: u32,
    pub source_methods_expanded: bool,
}

impl Default for ImportWorkbenchPreferences {
    fn default() -> Self {
        Self {
            schema_version: 1,
            active_section: ImportWorkbenchSection::Workbench,
            queue_filter: ImportQueuePreference::All,
            workbench_scroll_top: 0,
            capabilities_scroll_top: 0,
            history_scroll_top: 0,
            source_methods_expanded: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportWorkbenchPreferencesRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SaveImportWorkbenchPreferencesRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub preferences: ImportWorkbenchPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetImportPreviewContentV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub candidate_id: Option<String>,
    #[serde(default)]
    pub history_batch_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    pub target: ImportPreviewTarget,
    pub quality: QualityReport,
    pub raw_label: String,
    #[serde(default)]
    pub resources: Vec<ImportPreviewResource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison: Option<ImportPreviewComparison>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewComparison {
    pub current_markdown: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merged_markdown: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewTarget {
    pub disposition: String,
    pub source_id: Option<String>,
    pub version_id: Option<String>,
    pub wiki_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreviewResource {
    pub source: String,
    pub name: String,
    pub kind: String,
    pub size_bytes: u64,
    pub data_url: Option<String>,
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
    #[serde(default)]
    pub files: Vec<ImportFeatureReadiness>,
    #[serde(default)]
    pub platforms: Vec<ImportPlatformReadiness>,
    #[serde(default)]
    pub abilities: Vec<ImportFeatureReadiness>,
    #[serde(default)]
    pub capabilities: Vec<ImportCapabilityReadiness>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlatformReadiness {
    pub id: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportFeatureReadiness {
    pub id: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportCapabilityReadiness {
    pub capability_id: String,
    pub route: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
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
#[serde(rename_all = "snake_case")]
pub enum ImportHistoryAction {
    OpenDetail,
    OpenResult,
    ViewLogs,
    UpdateWiki,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportHistoryEntry {
    pub id: String,
    pub title: String,
    pub status: String,
    pub session_id: Option<String>,
    pub batch_id: Option<String>,
    pub task_id: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub completed_at: Option<String>,
    pub legacy_read_only: bool,
    pub item_count: u64,
    pub committed_count: u64,
    pub failed_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_labels: Vec<String>,
    pub available_actions: Vec<ImportHistoryAction>,
    /// False for pre-snapshot records that can only be shown through a
    /// best-effort live-session compatibility fallback.
    #[serde(default)]
    pub snapshot_available: bool,
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
pub struct GetImportHistoryDetailV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub batch_id: String,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RebuildImportHistoryIndexV2Request {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportHistoryDetailPage {
    pub entry: ImportHistoryEntry,
    pub items: Vec<crate::models::import_v2::ImportItem>,
    pub next_cursor: Option<String>,
    pub total: u64,
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
    pub requirement_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallImportCapabilityV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub capability_id: String,
    pub requirement_revision: String,
    pub acknowledge_install: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asr_profile: Option<ImportAsrProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recognition_language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GetImportAsrEnablementPlanV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportAsrDependencyKind {
    MediaRuntime,
    Engine,
    Model,
    LanguageSupport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportAsrDependency {
    pub kind: ImportAsrDependencyKind,
    pub name: String,
    pub available: bool,
    pub bundled_with_capability: bool,
    pub source: String,
    pub license: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportAsrProfilePlan {
    pub profile: ImportAsrProfile,
    pub capability_id: String,
    pub engine_name: String,
    pub model_name: String,
    pub available: bool,
    pub installable: bool,
    pub download_bytes: Option<u64>,
    pub installed_bytes: Option<u64>,
    pub model_bytes: Option<u64>,
    pub device: String,
    pub estimated_seconds: Option<u64>,
    pub unavailable_reason_code: Option<String>,
    pub dependencies: Vec<ImportAsrDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportAsrEnablementPlan {
    pub requirement_revision: String,
    pub recommended_profile: ImportAsrProfile,
    pub available_memory_bytes: Option<u64>,
    pub available_disk_bytes: Option<u64>,
    pub media_duration_seconds: Option<u64>,
    pub install_location: Option<String>,
    pub local_only: bool,
    pub profiles: Vec<ImportAsrProfilePlan>,
}
