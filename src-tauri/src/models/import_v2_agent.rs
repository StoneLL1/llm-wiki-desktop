use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{
    agent::AgentKind,
    import_v2::{ImportArtifact, QualityReport},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentAssistancePolicy {
    pub max_attempts_per_item: u8,
}

impl AgentAssistancePolicy {
    pub fn balanced() -> Self {
        Self {
            max_attempts_per_item: 1,
        }
    }
}

impl Default for AgentAssistancePolicy {
    fn default() -> Self {
        Self::balanced()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAssistanceTrigger {
    Manual,
    QualityOptimization,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRecoveryAction {
    InvokeLocalAgent,
    CompareCandidate,
    DiscardCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolGrant {
    InspectSource,
    RunDeterministicRoute,
    RunOcr,
    RunAsr,
    ParseSanitizedSnapshot,
    ValidateCandidate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentInvocationRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub trigger: AgentAssistanceTrigger,
    pub agent_kind: AgentKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentCandidateManifest {
    pub markdown_path: String,
    pub asset_paths: Vec<String>,
    pub markdown_sha256: String,
    #[serde(default)]
    pub asset_sha256: BTreeMap<String, String>,
    pub processing_summary: String,
    pub tools_used: Vec<String>,
    pub uncertainties: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCandidate {
    pub candidate_id: String,
    pub task_id: String,
    pub audit_id: String,
    pub trigger: AgentAssistanceTrigger,
    pub agent_kind: Option<AgentKind>,
    pub agent_version: String,
    pub prompt_template_version: String,
    pub approved_cost_micros: Option<u64>,
    pub tool_calls: Vec<String>,
    pub markdown: ImportArtifact,
    pub assets: Vec<ImportArtifact>,
    pub quality: QualityReport,
    pub processing_summary: String,
    pub tools_used: Vec<String>,
    pub uncertainties: Vec<String>,
    pub warnings: Vec<String>,
    pub source_snapshot_sha256: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCandidateDiff {
    pub candidate_id: String,
    pub baseline_markdown: String,
    pub current_markdown: Option<String>,
    /// Hash of the current Wiki bytes shown in the diff. Three-way merge
    /// selection must bind to this exact version so the UI cannot silently
    /// overwrite an edit made after the diff was opened.
    #[serde(default)]
    pub current_markdown_sha256: Option<String>,
    pub agent_markdown: String,
    pub unified_diff: String,
    pub needs_three_way_merge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AcceptImportAgentCandidateRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SelectImportAgentCandidateRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub candidate_id: String,
    pub merged_markdown: Option<String>,
    pub expected_current_wiki_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscardImportAgentCandidateRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub candidate_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCandidateView {
    pub project_id: String,
    pub session_id: String,
    pub item_id: String,
    pub candidate: AgentCandidate,
    pub diff: AgentCandidateDiff,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCandidateActionResult {
    pub project_id: String,
    pub session_id: String,
    pub item_id: String,
    pub candidate_id: String,
    pub item: super::import_v2::ImportItem,
    pub completion: Option<super::import_v2::ImportCompletion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuditRecord {
    pub audit_id: String,
    pub task_id: String,
    pub session_id: String,
    pub item_id: String,
    pub trigger: AgentAssistanceTrigger,
    pub route: String,
    pub agent_kind: Option<AgentKind>,
    pub agent_version: String,
    pub prompt_template_version: String,
    pub approved_cost_micros: Option<u64>,
    pub tool_calls: Vec<String>,
    pub approved_scope_sha256: Option<String>,
    pub workspace_relative_path: String,
    pub granted_tools: Vec<AgentToolGrant>,
    pub input_hashes: Vec<String>,
    pub output_hashes: Vec<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub outcome: String,
    pub warnings: Vec<String>,
}
