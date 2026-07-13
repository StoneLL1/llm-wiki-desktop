use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::{
    agent::AgentKind,
    import_v2::{ImportArtifact, QualityReport},
    llm::LlmProviderKind,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentAssistancePolicy {
    pub auto_local_on_hard_failure: bool,
    pub auto_local_on_quality_warning: bool,
    pub auto_byok: bool,
    pub max_attempts_per_item: u8,
}

impl AgentAssistancePolicy {
    pub fn balanced(auto_local_on_hard_failure: bool) -> Self {
        Self {
            auto_local_on_hard_failure,
            auto_local_on_quality_warning: false,
            auto_byok: false,
            max_attempts_per_item: 1,
        }
    }
}

impl Default for AgentAssistancePolicy {
    fn default() -> Self {
        Self::balanced(false)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAssistanceTrigger {
    DeterministicHardFailure,
    Manual,
    QualityOptimization,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRecoveryAction {
    InvokeLocalAgent,
    RequestByok,
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
#[serde(rename_all = "camelCase")]
pub struct GetImportAgentPolicyRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SetImportAgentPolicyRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub policy: AgentAssistancePolicy,
    pub local_agent_kind: Option<AgentKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendScopeFile {
    pub relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub estimated_tokens: u64,
    #[serde(default)]
    pub redactions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSendScope {
    pub approval_id: String,
    pub item_id: String,
    pub provider: String,
    pub model: String,
    pub destination: String,
    #[serde(default)]
    pub public_metadata: Vec<String>,
    pub files: Vec<SendScopeFile>,
    pub estimated_input_tokens: u64,
    pub estimated_cost_micros: Option<u64>,
    #[serde(default)]
    pub requires_duplicate_charge_acknowledgement: bool,
    pub scope_sha256: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewImportByokScopeRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub trigger: AgentAssistanceTrigger,
    pub provider: LlmProviderKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApproveImportByokAssistanceRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub trigger: AgentAssistanceTrigger,
    pub provider: LlmProviderKind,
    pub model: String,
    pub approval_id: String,
    pub scope_sha256: String,
    #[serde(default)]
    pub acknowledge_possible_duplicate_charge: bool,
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
    pub trigger: AgentAssistanceTrigger,
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
    pub approved_scope_sha256: Option<String>,
    pub granted_tools: Vec<AgentToolGrant>,
    pub input_hashes: Vec<String>,
    pub output_hashes: Vec<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub outcome: String,
    pub warnings: Vec<String>,
}
