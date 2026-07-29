use serde::{Deserialize, Serialize};

use crate::models::agent::AgentKind;
use crate::models::compile::CompileRoutePreference;
use crate::models::import_v2::QualityReport;
use crate::models::llm::LlmProviderKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceBinding {
    pub source_id: String,
    pub version_id: String,
    pub status: SourceStatus,
    pub quality: QualityReport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Current,
    CandidateReady,
    NeedsAttention,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourcePrimaryAction {
    ReviewCandidate,
    ReprocessOcr,
    ReprocessAsr,
    RefreshSource,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceCandidateKind {
    Ocr,
    Asr,
    Subtitle,
    Refresh,
    AiOrganize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceEvidenceRetention {
    ImmutableOriginalsRetained,
}

impl SourceCandidateKind {
    pub fn timeline_kind(&self) -> &'static str {
        match self {
            Self::Ocr => "ocr_reprocessed",
            Self::Asr | Self::Subtitle => "asr_reprocessed",
            Self::Refresh => "source_refreshed",
            Self::AiOrganize => "ai_organize_applied",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceAiOrganizeRoute {
    Agent,
    Byok,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceAiOrganizeCandidateMeta {
    pub task_id: String,
    pub route: SourceAiOrganizeRoute,
    pub engine: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceArtifactSummary {
    pub path: String,
    pub kind: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceVersionSummary {
    pub version_id: String,
    pub created_at: String,
    pub event_kind: String,
    pub quality: QualityReport,
    pub current: bool,
    pub restorable: bool,
    pub checkpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceTimelineItem {
    pub event_id: String,
    pub kind: String,
    pub version_id: Option<String>,
    pub created_at: String,
    pub checkpoint: Option<String>,
    pub restorable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceCandidateSummary {
    pub candidate_id: String,
    pub kind: SourceCandidateKind,
    pub created_at: String,
    pub base_version_id: String,
    pub base_markdown_hash: String,
    pub candidate_markdown_hash: String,
    pub quality: QualityReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_organize: Option<SourceAiOrganizeCandidateMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceDetail {
    pub source_id: String,
    pub version_id: String,
    pub title: String,
    pub source_kind: String,
    pub status: SourceStatus,
    pub current_path: String,
    pub current_markdown_hash: String,
    pub primary_action: SourcePrimaryAction,
    pub candidate: Option<SourceCandidateSummary>,
    pub target_path: String,
    pub evidence_retention: SourceEvidenceRetention,
    pub evidence: Vec<SourceArtifactSummary>,
    pub quality: QualityReport,
    pub original_draft: String,
    pub original_draft_truncated: bool,
    pub versions: Vec<SourceVersionSummary>,
    pub timeline: Vec<SourceTimelineItem>,
    pub related_wiki_paths: Vec<String>,
    pub technical_details: SourceTechnicalDetails,
    pub available_actions: Vec<SourceCandidateKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceTechnicalDetails {
    pub route: String,
    pub engine: String,
    pub engine_version: String,
    pub locator: String,
    pub manifest_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSourceDetailRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSourceVersionsRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReprocessSourceRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
    pub expected_markdown_hash: String,
    #[serde(default)]
    pub subtitle_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSourceAiOrganizeRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
    pub expected_version_id: String,
    pub expected_markdown_hash: String,
    pub route: CompileRoutePreference,
    #[serde(default)]
    pub agent: Option<AgentKind>,
    #[serde(default)]
    pub provider: Option<LlmProviderKind>,
    #[serde(default)]
    pub custom_instructions: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrySourceAiOrganizeRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewSourceUpdateRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
    pub candidate_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceUpdateMode {
    TwoWay,
    ThreeWay,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceUpdatePreview {
    pub source_id: String,
    pub candidate_id: String,
    pub mode: SourceUpdateMode,
    pub base_markdown: String,
    pub current_markdown: String,
    pub candidate_markdown: String,
    pub diff: String,
    pub current_markdown_hash: String,
    pub candidate_markdown_hash: String,
    pub guard_token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplySourceCandidateRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
    pub candidate_id: String,
    pub guard_token: String,
    #[serde(default)]
    pub merged_markdown: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardSourceCandidateRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
    pub candidate_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceMutationResult {
    pub source_id: String,
    pub version_id: String,
    pub wiki_path: String,
    pub checkpoint: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreSourceVersionRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
    pub version_id: String,
    pub expected_markdown_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMoveSourceRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
    pub new_wiki_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MoveSourcePreview {
    pub source_id: String,
    pub old_wiki_path: String,
    pub new_wiki_path: String,
    pub affected_paths: Vec<String>,
    pub guard_token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveSourceRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
    pub new_wiki_path: String,
    pub guard_token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDeleteSourceRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSourcePreview {
    pub source_id: String,
    pub title: String,
    pub paths: Vec<SourceArtifactSummary>,
    pub versions: Vec<SourceVersionSummary>,
    pub referenced_by: Vec<String>,
    pub reference_count: usize,
    pub expected_freed_bytes: u64,
    pub guard_token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSourceRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub source_id: String,
    pub guard_token: String,
    pub confirmation_text: String,
}
