use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use super::agent::AgentKind;
use super::compile::CompileRoutePreference;
use super::llm::LlmProviderKind;
use super::workflow::{HealthCheckMode, WorkflowRoute};

pub const WIKI_LINT_SCHEMA_VERSION: u32 = 1;
pub const WIKI_LINT_SKILL_ID: &str = "builtin.wiki-lint";
pub const WIKI_LINT_SKILL_VERSION: &str = "2026-08-12.1";
pub const WIKI_LINT_SKILL_SHA256: &str =
    "29e903710745451da287de9d08297ae6863de944bd7d9abd7f4243b5b9f76eb0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WikiLintSkillRef {
    pub id: String,
    pub version: String,
    pub sha256: String,
}

impl WikiLintSkillRef {
    pub fn builtin() -> Self {
        Self {
            id: WIKI_LINT_SKILL_ID.into(),
            version: WIKI_LINT_SKILL_VERSION.into(),
            sha256: WIKI_LINT_SKILL_SHA256.into(),
        }
    }

    pub fn is_builtin(&self) -> bool {
        self.id == WIKI_LINT_SKILL_ID
            && self.version == WIKI_LINT_SKILL_VERSION
            && self.sha256 == WIKI_LINT_SKILL_SHA256
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentLintRepairOperation {
    Analyze,
    Repair,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLintRepairPreparation {
    pub preparation_id: String,
    pub preparation_revision: String,
    pub report_id: String,
    pub selected_finding_ids: Vec<String>,
    pub route: WorkflowRoute,
    pub skill: WikiLintSkillRef,
    pub authorized_paths: Vec<String>,
    pub baseline_fingerprint: String,
    pub pending_action: crate::models::confirmation::PendingAction,
}

/// Coarse severity for surfacing and grouping lint issues.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
}

/// Whether an issue came from the deterministic local pass or an Agent run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LintIssueSource {
    Local,
    Agent,
}

/// The specific rule that produced an issue. `LintIssueSource` says whether a
/// finding came from deterministic local checks or Agent deep lint; issue types
/// such as `MissingSource` and `SchemaMismatch` may be emitted by either layer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LintIssueType {
    // Local deterministic rules.
    DeadLink,
    OrphanPage,
    MissingFrontmatter,
    IndexDrift,
    EmptyPage,
    DuplicateFilename,
    PathCase,
    MissingResource,
    MissingSourceSection,
    InvalidPageType,
    // Agent heuristic deep-lint rules.
    DuplicateTopic,
    WeakCrossReference,
    MissingSource,
    SchemaMismatch,
    OutdatedContent,
    Contradiction,
}

/// Whether the backend can apply a fix without further judgment, and at what
/// risk level. `None` means the issue is informational only.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Fixability {
    None,
    Safe,
    HighRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LintRange {
    /// 1-based line number within the page body.
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

/// A single lint finding. `id` is deterministic
/// (`{issue_type}:{path}[:{target}]`); the backend also carries a scan-time
/// hash and validates the deterministic fix shape before accepting it back
/// from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintIssue {
    pub id: String,
    pub source: LintIssueSource,
    pub severity: LintSeverity,
    pub issue_type: LintIssueType,
    /// Project-relative path of the affected page.
    pub path: String,
    /// Content hash captured when this finding was produced. Fix execution
    /// must use this baseline instead of obtaining a fresh hash from the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<LintRange>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    /// For link issues, the unresolved target or referenced path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub fixability: Fixability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintReport {
    pub issues: Vec<LintIssue>,
    pub generated_at: String,
    pub scanned_pages: usize,
}

/// The shape the `wiki-lint` Skill emits inside its fenced JSON block. The
/// backend maps each of these onto a [`LintIssue`] with `source = Agent`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeepLintIssueType {
    DuplicateTopic,
    WeakCrossReference,
    MissingSource,
    SchemaMismatch,
    OutdatedContent,
    Contradiction,
}

impl From<DeepLintIssueType> for LintIssueType {
    fn from(value: DeepLintIssueType) -> Self {
        match value {
            DeepLintIssueType::DuplicateTopic => Self::DuplicateTopic,
            DeepLintIssueType::WeakCrossReference => Self::WeakCrossReference,
            DeepLintIssueType::MissingSource => Self::MissingSource,
            DeepLintIssueType::SchemaMismatch => Self::SchemaMismatch,
            DeepLintIssueType::OutdatedContent => Self::OutdatedContent,
            DeepLintIssueType::Contradiction => Self::Contradiction,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintAgentIssue {
    pub issue_type: DeepLintIssueType,
    pub severity: LintSeverity,
    pub path: String,
    pub message: String,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WikiLintAnalysisIssue {
    pub issue_type: DeepLintIssueType,
    pub severity: LintSeverity,
    pub path: String,
    pub message: String,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub suggestion: Option<String>,
}

impl From<WikiLintAnalysisIssue> for LintAgentIssue {
    fn from(value: WikiLintAnalysisIssue) -> Self {
        Self {
            issue_type: value.issue_type,
            severity: value.severity,
            path: value.path,
            message: value.message,
            evidence: value.evidence,
            suggestion: value.suggestion,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WikiLintAnalysisOutput {
    pub schema_version: u32,
    pub operation: AgentLintRepairOperation,
    pub skill: WikiLintSkillRef,
    pub issues: Vec<WikiLintAnalysisIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLintRepairFinding {
    pub id: String,
    pub issue_type: DeepLintIssueType,
    pub severity: LintSeverity,
    pub path: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLintRepairRoundSummary {
    pub round: u8,
    #[serde(default)]
    pub affected_paths: Vec<String>,
    #[serde(default)]
    pub unresolved_finding_ids: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLintRepairRequest {
    pub schema_version: u32,
    pub operation: AgentLintRepairOperation,
    pub skill: WikiLintSkillRef,
    pub report_id: String,
    pub selection_revision: String,
    pub round: u8,
    pub max_rounds: u8,
    pub findings: Vec<AgentLintRepairFinding>,
    #[serde(default)]
    pub prior_rounds: Vec<AgentLintRepairRoundSummary>,
    pub writable_paths: Vec<String>,
    pub creatable_roots: Vec<String>,
    pub read_only_roots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub language: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentLintRepairFindingStatus {
    Attempted,
    Skipped,
    NeedsReview,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLintRepairFindingResult {
    pub finding_id: String,
    pub status: AgentLintRepairFindingStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentLintRepairDeclaredChangeOperation {
    Create,
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLintRepairDeclaredChange {
    pub path: String,
    pub operation: AgentLintRepairDeclaredChangeOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLintRepairRoundOutput {
    pub schema_version: u32,
    pub operation: AgentLintRepairOperation,
    pub skill: WikiLintSkillRef,
    pub report_id: String,
    pub selection_revision: String,
    pub round: u8,
    pub finding_results: Vec<AgentLintRepairFindingResult>,
    pub declared_changes: Vec<AgentLintRepairDeclaredChange>,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentLintRepairOutcome {
    Succeeded,
    PartiallyCompleted,
    ManualReviewRequired,
    Cancelled,
    Failed,
    Interrupted,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLintRepairCorrelation {
    pub resolved_finding_ids: Vec<String>,
    pub unresolved_finding_ids: Vec<String>,
    pub introduced_finding_ids: Vec<String>,
    pub skipped_finding_ids: Vec<String>,
}

/// Persisted deep-lint report at `.app/lint-reports/<task_id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeepLintReport {
    pub issues: Vec<LintIssue>,
    pub raw_output: String,
    pub generated_at: String,
}

/// Coverage and provenance for one composed Health Check run. Finding ids are
/// stable Lint ids; the origin map lets the existing Lint surface filter a
/// merged finding by local/deep source without rendering it twice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckCoverage {
    pub scanned_pages: usize,
    pub source_pages: usize,
    pub wiki_pages: usize,
    #[serde(default)]
    pub deep_covered_pages: Option<usize>,
    #[serde(default)]
    pub deep_truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_applicable_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheckReport {
    pub report_id: String,
    pub task_id: String,
    pub mode: HealthCheckMode,
    pub route: WorkflowRoute,
    pub persistent: bool,
    pub issues: Vec<LintIssue>,
    #[serde(default)]
    pub finding_origins: BTreeMap<String, Vec<LintIssueSource>>,
    pub coverage: HealthCheckCoverage,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    #[serde(default)]
    pub findings_by_type: BTreeMap<String, usize>,
    pub duration_ms: u64,
    pub generated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LintReportKind {
    Local,
    Deep,
    HealthCheck,
}

impl Default for LintReportKind {
    fn default() -> Self {
        Self::Local
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintHistoryEntry {
    pub id: String,
    pub kind: LintReportKind,
    pub created_at: String,
    pub issue_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    #[serde(default)]
    pub scanned_pages: Option<usize>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub route: Option<CompileRoutePreference>,
    #[serde(default)]
    pub workflow_route: Option<WorkflowRoute>,
    #[serde(default)]
    pub health_check_mode: Option<HealthCheckMode>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default = "default_persistent")]
    pub persistent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintHistoryFile {
    #[serde(default = "lint_history_version")]
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<LintHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedLintReport {
    pub entry: LintHistoryEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_report: Option<LintReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deep_report: Option<DeepLintReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_check_report: Option<HealthCheckReport>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadLintHistoryReportRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListLintHistoryRequest {
    pub project_id: String,
    pub project_root_path: String,
}

fn lint_history_version() -> u32 {
    1
}

fn default_persistent() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunLocalLintRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDeepLintRequest {
    pub project_id: String,
    pub project_root_path: String,
    #[serde(default = "default_route")]
    pub route: CompileRoutePreference,
    #[serde(default)]
    pub agent: Option<AgentKind>,
    #[serde(default)]
    pub provider: Option<LlmProviderKind>,
}

fn default_route() -> CompileRoutePreference {
    CompileRoutePreference::Auto
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDeepLintReportRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub task_id: String,
}

/// The report issue is passed back from the frontend together with its
/// server-generated scan hash; deterministic fix handlers validate both before
/// writing.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyLintFixRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub issue: LintIssue,
    /// For high-risk fixes: `false` (default) returns a [`LintFixOutcome`] of
    /// kind `needs_confirmation`; `true` executes the fix (requires
    /// `expected_hash`).
    #[serde(default)]
    pub confirm_high_risk: bool,
    #[serde(default)]
    pub expected_hash: Option<String>,
    #[serde(default)]
    pub action_id: Option<String>,
}

/// Batch auto-fix request (PRD-LINT-003). One Git checkpoint protects every
/// safe write; high-risk fixes are returned as confirmations for unified
/// user review instead of being written immediately.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyLintFixesBatchRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub issues: Vec<LintIssue>,
    /// `{path -> sha256}` captured at scan time, used as the optimistic-lock
    /// baseline for each safe fix. Paths missing from the map are skipped with
    /// `LINT_FIX_HASH_REQUIRED` rather than aborting the whole batch.
    #[serde(default)]
    pub expected_hashes: HashMap<String, String>,
}

/// Result of a batch auto-fix run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LintBatchOutcome {
    /// Single Git checkpoint hash covering every applied safe fix (the rollback
    /// point). `None` when no safe fixes ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
    /// Commit created after all applied safe changes were verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_commit: Option<String>,
    /// Safe fixes that were written under the shared checkpoint.
    pub applied: Vec<LintFixOutcome>,
    /// High-risk fixes awaiting user confirmation. Each carries its source
    /// issue so the command layer can register it for the existing confirm path.
    pub needs_confirmation: Vec<LintBatchConfirmation>,
    /// Issues the batch could not handle (non-fixable, stale, missing hash).
    pub skipped: Vec<LintBatchSkip>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LintBatchConfirmation {
    pub issue: LintIssue,
    pub pending_action: crate::models::confirmation::PendingAction,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LintBatchSkip {
    pub issue_id: String,
    pub path: String,
    pub reason_code: String,
    pub reason: String,
}

/// One persisted ignore entry. The match key is `(path, rule)` — ignoring a
/// rule on a page suppresses every occurrence of that rule on that page (e.g.
/// all dead links on one page). Stored at `.app/lint-ignore.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LintIgnoreEntry {
    pub path: String,
    pub rule: LintIssueType,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LintIgnoreFile {
    #[serde(default)]
    pub ignored: Vec<LintIgnoreEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLintIgnoreRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub path: String,
    pub rule: LintIssueType,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveLintIgnoreRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub path: String,
    pub rule: LintIssueType,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListLintIgnoresRequest {
    pub project_id: String,
    pub project_root_path: String,
}

/// Result of an apply attempt. Kept as a struct with a discriminator field
/// (rather than a `#[serde(tag)]` enum) so nested `pending_action` fields
/// serialize camelCase consistently — see gotchas.txt on tagged-enum leakage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintFixOutcome {
    /// `"applied"` or `"needs_confirmation"`.
    pub kind: LintFixOutcomeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
    /// Commit created after the applied change was verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<crate::models::confirmation::PendingAction>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LintFixOutcomeKind {
    Applied,
    NeedsConfirmation,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::confirmation::{PendingAction, PendingActionType, RiskLevel};
    use serde_json::json;

    #[test]
    fn serializes_issue_with_camel_case_and_snake_enums() {
        let issue = LintIssue {
            id: "dead_link:wiki/a.md:ghost".into(),
            source: LintIssueSource::Local,
            severity: LintSeverity::Warning,
            issue_type: LintIssueType::DeadLink,
            path: "wiki/a.md".into(),
            scan_hash: Some("hash-a".into()),
            range: Some(LintRange {
                line: 3,
                column: None,
            }),
            message: "Unresolved wikilink".into(),
            evidence: Some("[[ghost]]".into()),
            target: Some("ghost".into()),
            fixability: Fixability::HighRisk,
            suggested_action: Some("Remove or fix the link".into()),
        };
        let value = serde_json::to_value(&issue).unwrap();
        assert_eq!(value["source"], json!("local"));
        assert_eq!(value["severity"], json!("warning"));
        assert_eq!(value["issueType"], json!("dead_link"));
        assert_eq!(value["fixability"], json!("high_risk"));
        assert_eq!(value["range"]["line"], json!(3));
        assert!(value.get("range.column").is_none()); // column omitted
        assert_eq!(value["suggestedAction"], json!("Remove or fix the link"));
    }

    #[test]
    fn outcome_round_trips_both_kinds() {
        let applied = LintFixOutcome {
            kind: LintFixOutcomeKind::Applied,
            affected_paths: vec!["wiki/a.md".into()],
            checkpoint: Some("abc123".into()),
            final_commit: Some("def456".into()),
            pending_action: None,
        };
        let value = serde_json::to_value(&applied).unwrap();
        assert_eq!(value["kind"], json!("applied"));
        assert_eq!(value["checkpoint"], json!("abc123"));

        let needs = LintFixOutcome {
            kind: LintFixOutcomeKind::NeedsConfirmation,
            affected_paths: Vec::new(),
            checkpoint: None,
            final_commit: None,
            pending_action: Some(PendingAction {
                id: "pa-1".into(),
                action_type: PendingActionType::AgentAutoFix,
                title: "Remove dead link".into(),
                message: "Removes an unresolved wikilink".into(),
                risk_level: RiskLevel::High,
                affected_paths: vec!["wiki/a.md".into()],
                preview: None,
                expires_at: None,
                checkpoint_hash: None,
            }),
        };
        let value = serde_json::to_value(&needs).unwrap();
        assert_eq!(value["kind"], json!("needs_confirmation"));
        assert_eq!(
            value["pendingAction"]["actionType"],
            json!("agent_auto_fix")
        );
        assert_eq!(value["pendingAction"]["riskLevel"], json!("high"));
    }

    #[test]
    fn agent_issue_deserializes_skill_payload() {
        let raw = r#"{
            "issueType": "duplicate_topic",
            "severity": "warning",
            "path": "wiki/concepts/a.md",
            "message": "Overlaps with b.md",
            "evidence": "Both define RAG",
            "suggestion": "Merge the two pages"
        }"#;
        let issue: LintAgentIssue = serde_json::from_str(raw).unwrap();
        assert_eq!(issue.issue_type, DeepLintIssueType::DuplicateTopic);
        assert_eq!(issue.severity, LintSeverity::Warning);
        assert_eq!(issue.suggestion.as_deref(), Some("Merge the two pages"));
    }

    #[test]
    fn start_request_defaults_route_to_auto() {
        let raw = r#"{"projectId":"p","projectRootPath":"/x"}"#;
        let request: StartDeepLintRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(request.route, CompileRoutePreference::Auto);
    }

    #[test]
    fn lint_history_file_defaults_version_and_entries() {
        let file: LintHistoryFile = serde_json::from_str("{}").unwrap();
        assert_eq!(file.version, 1);
        assert!(file.entries.is_empty());
    }

    #[test]
    fn persisted_lint_report_omits_missing_report_bodies() {
        let entry = LintHistoryEntry {
            id: "local-1".into(),
            kind: LintReportKind::Local,
            created_at: "2026-07-04T00:00:00Z".into(),
            issue_count: 0,
            error_count: 0,
            warning_count: 0,
            info_count: 0,
            scanned_pages: Some(10),
            task_id: None,
            route: None,
            workflow_route: None,
            health_check_mode: None,
            duration_ms: None,
            persistent: true,
        };
        let value = serde_json::to_value(PersistedLintReport {
            entry,
            local_report: None,
            deep_report: None,
            health_check_report: None,
        })
        .unwrap();

        assert_eq!(value["entry"]["kind"], json!("local"));
        assert!(value.get("localReport").is_none());
        assert!(value.get("deepReport").is_none());
        assert!(value.get("healthCheckReport").is_none());
    }
}
