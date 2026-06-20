use serde::{Deserialize, Serialize};

use super::agent::AgentKind;
use super::compile::CompileRoutePreference;
use super::llm::LlmProviderKind;

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

/// The specific rule that produced an issue. Local rules are deterministic
/// (no model); Agent rules come from the `wiki-lint` Skill.
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
    // Agent deep-lint rules.
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
/// (`{issue_type}:{path}[:{target}]`) so the frontend can pass the issue back
/// to `apply_lint_fix` statelessly without a server-side issue registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintIssue {
    pub id: String,
    pub source: LintIssueSource,
    pub severity: LintSeverity,
    pub issue_type: LintIssueType,
    /// Project-relative path of the affected page.
    pub path: String,
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
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintAgentIssue {
    pub issue_type: LintIssueType,
    pub severity: LintSeverity,
    pub path: String,
    pub message: String,
    #[serde(default)]
    pub evidence: Option<String>,
    #[serde(default)]
    pub suggestion: Option<String>,
}

/// Persisted deep-lint report at `.app/lint-reports/<task_id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeepLintReport {
    pub issues: Vec<LintIssue>,
    pub raw_output: String,
    pub generated_at: String,
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

/// The issue is passed back from the frontend so the fix path stays stateless
/// (no server-side issue registry to keep in sync with the live wiki).
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
            pending_action: None,
        };
        let value = serde_json::to_value(&applied).unwrap();
        assert_eq!(value["kind"], json!("applied"));
        assert_eq!(value["checkpoint"], json!("abc123"));

        let needs = LintFixOutcome {
            kind: LintFixOutcomeKind::NeedsConfirmation,
            affected_paths: Vec::new(),
            checkpoint: None,
            pending_action: Some(PendingAction {
                id: "pa-1".into(),
                action_type: PendingActionType::AgentAutoFix,
                title: "Remove dead link".into(),
                message: "Removes an unresolved wikilink".into(),
                risk_level: RiskLevel::High,
                affected_paths: vec!["wiki/a.md".into()],
                preview: None,
                expires_at: None,
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
        assert_eq!(issue.issue_type, LintIssueType::DuplicateTopic);
        assert_eq!(issue.severity, LintSeverity::Warning);
        assert_eq!(issue.suggestion.as_deref(), Some("Merge the two pages"));
    }

    #[test]
    fn start_request_defaults_route_to_auto() {
        let raw = r#"{"projectId":"p","projectRootPath":"/x"}"#;
        let request: StartDeepLintRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(request.route, CompileRoutePreference::Auto);
    }
}
