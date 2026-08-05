use serde::{Deserialize, Serialize};

use crate::models::confirmation::PendingAction;
use crate::models::git::GitRepositoryStatus;
use crate::models::layout::{ProjectLayout, ProjectLayoutConfidence, ProjectLayoutWarning};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSummary {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectTemplate {
    #[default]
    General,
    Research,
    Reading,
    PersonalGrowth,
    Business,
}

impl ProjectTemplate {
    pub fn template_key(&self) -> &'static str {
        match self {
            ProjectTemplate::General => "general",
            ProjectTemplate::Research => "research",
            ProjectTemplate::Reading => "reading",
            ProjectTemplate::PersonalGrowth => "personal-growth",
            ProjectTemplate::Business => "business",
        }
    }

    pub fn from_template_key(key: &str) -> Option<Self> {
        match key {
            "general" => Some(ProjectTemplate::General),
            "research" => Some(ProjectTemplate::Research),
            "reading" => Some(ProjectTemplate::Reading),
            "personal-growth" => Some(ProjectTemplate::PersonalGrowth),
            "business" => Some(ProjectTemplate::Business),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTrustKind {
    Native,
    Compatible,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectFilesystemAccess {
    Writable,
    ReadOnly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectFormat {
    NativeCurrent,
    NativeLegacy,
    NashsuLlmWiki,
    ObsidianVault,
    MarkdownVault,
    AmbiguousMarkdown,
    OrdinaryMaterials,
    Unknown,
}

/// A user-owned route for a low-confidence Markdown folder. It is stored in
/// global app settings against the canonical folder identity, never in the
/// selected folder itself.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOpenIntent {
    OpenAsMarkdownVault,
    CreateFromMaterials,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTrustState {
    Trusted,
    Untrusted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectHealth {
    Healthy,
    Repairable,
    Recovery,
    Unreadable,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProjectCapability {
    ReadMarkdown,
    LocalSearch,
    InMemoryGraph,
    LocalHealthCheck,
    ExternalAi,
    ProjectWrite,
    GitCheckpoint,
    EnableCompatibleFeatures,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMarker {
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAssessmentWarning {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AssessmentOperationId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(transparent)]
pub struct AssessmentId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOpenAssessment {
    pub assessment_id: AssessmentId,
    pub canonical_root_path: String,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub format: ProjectFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remembered_open_intent: Option<ProjectOpenIntent>,
    pub trust: ProjectTrustState,
    pub filesystem_access: ProjectFilesystemAccess,
    pub health: ProjectHealth,
    #[serde(default)]
    pub repair_available: bool,
    pub layout: ProjectLayout,
    pub confidence: ProjectLayoutConfidence,
    pub markers: Vec<ProjectMarker>,
    pub capabilities: Vec<ProjectCapability>,
    pub warnings: Vec<ProjectAssessmentWarning>,
    pub layout_warnings: Vec<ProjectLayoutWarning>,
    pub git: GitRepositoryStatus,
}

/// Backend-derived authorization and readiness facts for an opened project.
///
/// This snapshot deliberately keeps trust, filesystem access, health, layout,
/// and capabilities independent. UI surfaces may display a condensed state,
/// but commands must continue to authorize from the individual facts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSessionAuthority {
    pub project_id: String,
    pub canonical_root_path: String,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub authority_revision: String,
    pub format: ProjectFormat,
    pub trust: ProjectTrustState,
    pub filesystem_access: ProjectFilesystemAccess,
    pub health: ProjectHealth,
    pub layout: ProjectLayout,
    pub confidence: ProjectLayoutConfidence,
    pub capabilities: Vec<ProjectCapability>,
    pub warnings: Vec<ProjectAssessmentWarning>,
    pub layout_warnings: Vec<ProjectLayoutWarning>,
    pub git: GitRepositoryStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
    pub summary: ProjectSummary,
    pub authority: ProjectSessionAuthority,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentOperationStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAssessmentOperation {
    pub assessment_operation_id: AssessmentOperationId,
    pub status: AssessmentOperationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assessment: Option<ProjectOpenAssessment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<crate::errors::BackendError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartProjectOpenAssessmentResult {
    pub assessment_operation_id: AssessmentOperationId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub project_id: String,
    pub name: String,
    pub root_path: String,
    pub template: ProjectTemplate,
    pub wiki_page_count: usize,
    pub source_count: usize,
    pub task_count: usize,
    pub index_state: IndexState,
    pub graph_state: GraphState,
    pub agent_route: AgentRoute,
    #[serde(default)]
    pub inventory_state: ProjectInventoryState,
    pub health: ProjectHealthReport,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectInventoryState {
    Scanning,
    Partial,
    Failed,
    #[default]
    Ready,
}

/// A bounded recovery plan prepared from a current project assessment. The
/// plan contains only app-owned, derived state operations; it never carries
/// Markdown, original sources, or a request to follow external links.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRepairPlan {
    pub repair_plan_id: String,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub expected_git_head: Option<String>,
    pub expected_git_paths: Vec<String>,
    pub operations: Vec<ProjectRepairOperation>,
    pub protected_paths: Vec<String>,
    pub external_links_remain_blocked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRepairOperation {
    pub operation_type: ProjectRepairOperationType,
    pub target_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    /// Content hash observed during the read-only prepare phase. Apply must
    /// refuse the repair if the target changed after preview.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowlist_descriptor: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRepairOperationType {
    RegenerateGraphCache,
    CreateDirectory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IndexState {
    Indexed,
    Stale,
    Missing,
}

impl Default for IndexState {
    fn default() -> Self {
        Self::Missing
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraphState {
    Cached,
    Stale,
    Missing,
}

impl Default for GraphState {
    fn default() -> Self {
        Self::Missing
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRoute {
    Agent,
    Byok,
    Unconfigured,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHealthReport {
    pub is_wiki_project: bool,
    pub has_purpose: bool,
    pub has_schema: bool,
    pub has_app_state: bool,
    pub has_obsidian: bool,
    pub missing_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub project_id: String,
    pub name: String,
    pub root_path: String,
    #[serde(default)]
    pub template: ProjectTemplate,
    pub opened_at: String,
    #[serde(default)]
    pub wiki_page_count: usize,
    #[serde(default)]
    pub source_count: usize,
    #[serde(default)]
    pub task_count: usize,
    #[serde(default)]
    pub index_state: IndexState,
    #[serde(default)]
    pub graph_state: GraphState,
    #[serde(default)]
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub root_path: String,
    pub name: String,
    #[serde(default)]
    pub template: ProjectTemplate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenProjectResponse {
    pub kind: OpenProjectKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ProjectSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<ProjectSessionAuthority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<PendingAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OpenProjectKind {
    Opened,
    NeedsConfirmation,
}

impl OpenProjectResponse {
    pub fn opened(summary: ProjectSummary) -> Self {
        Self {
            kind: OpenProjectKind::Opened,
            summary: Some(summary),
            authority: None,
            pending_action: None,
        }
    }

    pub fn needs_confirmation(pending_action: PendingAction) -> Self {
        Self {
            kind: OpenProjectKind::NeedsConfirmation,
            summary: None,
            authority: None,
            pending_action: Some(pending_action),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RememberRecentProjectRequest {
    pub project_id: String,
    pub name: String,
    pub root_path: String,
    #[serde(default)]
    pub template: ProjectTemplate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveRecentProjectRequest {
    pub project_id: String,
    pub root_path: String,
}

/// Relocates a missing native knowledge-base entry only after the selected
/// folder's app-owned stable ID has been revalidated by the backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelocateRecentProjectRequest {
    pub assessment_id: AssessmentId,
    pub previous_project_id: String,
    pub previous_root_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_project_template_as_kebab_case() {
        assert_eq!(
            serde_json::to_string(&ProjectTemplate::PersonalGrowth).unwrap(),
            "\"personal-growth\""
        );
        assert_eq!(
            serde_json::to_string(&ProjectTemplate::General).unwrap(),
            "\"general\""
        );
    }

    #[test]
    fn round_trips_template_keys() {
        for template in [
            ProjectTemplate::General,
            ProjectTemplate::Research,
            ProjectTemplate::Reading,
            ProjectTemplate::PersonalGrowth,
            ProjectTemplate::Business,
        ] {
            let key = template.template_key();
            assert_eq!(ProjectTemplate::from_template_key(key), Some(template));
        }
    }

    #[test]
    fn serializes_project_summary_with_camel_case_fields() {
        let summary = ProjectSummary {
            project_id: "project-1".to_string(),
            name: "Agent Wiki".to_string(),
            root_path: "D:/wiki".to_string(),
            template: ProjectTemplate::Research,
            wiki_page_count: 345,
            source_count: 12,
            task_count: 2,
            index_state: IndexState::Indexed,
            graph_state: GraphState::Cached,
            agent_route: AgentRoute::Agent,
            inventory_state: ProjectInventoryState::Ready,
            health: ProjectHealthReport {
                is_wiki_project: true,
                has_purpose: true,
                has_schema: true,
                has_app_state: true,
                has_obsidian: false,
                missing_paths: Vec::new(),
            },
        };

        let value = serde_json::to_value(&summary).unwrap();

        assert_eq!(value["projectId"], json!("project-1"));
        assert_eq!(value["rootPath"], json!("D:/wiki"));
        assert_eq!(value["wikiPageCount"], json!(345));
        assert_eq!(value["template"], json!("research"));
        assert_eq!(value["indexState"], json!("indexed"));
        assert_eq!(value["agentRoute"], json!("agent"));
        assert_eq!(value["inventoryState"], json!("ready"));
        assert_eq!(value["health"]["isWikiProject"], json!(true));
        assert!(value.get("wiki_page_count").is_none());
    }

    #[test]
    fn tags_open_outcome_variants() {
        use super::{OpenProjectKind, OpenProjectResponse};
        let opened = OpenProjectResponse::opened(ProjectSummary {
            project_id: "p".to_string(),
            name: "n".to_string(),
            root_path: "/r".to_string(),
            template: ProjectTemplate::General,
            wiki_page_count: 0,
            source_count: 0,
            task_count: 0,
            index_state: IndexState::Missing,
            graph_state: GraphState::Missing,
            agent_route: AgentRoute::Unconfigured,
            inventory_state: ProjectInventoryState::Ready,
            health: ProjectHealthReport {
                is_wiki_project: false,
                has_purpose: false,
                has_schema: false,
                has_app_state: false,
                has_obsidian: false,
                missing_paths: Vec::new(),
            },
        });
        let value = serde_json::to_value(&opened).unwrap();
        assert_eq!(value["kind"], json!("opened"));
        assert_eq!(value["summary"]["projectId"], json!("p"));

        let pending = OpenProjectResponse::needs_confirmation(PendingAction {
            id: "pa-1".to_string(),
            action_type: crate::models::confirmation::PendingActionType::TrustCompatibleProject,
            title: "Trust".to_string(),
            message: "Will trust this compatible knowledge base.".to_string(),
            risk_level: crate::models::confirmation::RiskLevel::High,
            affected_paths: Vec::new(),
            preview: None,
            expires_at: None,
            checkpoint_hash: None,
        });
        let pending_value = serde_json::to_value(&pending).unwrap();
        assert_eq!(pending_value["kind"], json!("needs_confirmation"));
        assert_eq!(
            pending_value["pendingAction"]["actionType"],
            json!("trust_compatible_project")
        );
        let _ = OpenProjectKind::Opened;
    }

    #[test]
    fn recent_project_missing_defaults_to_false_for_legacy_json() {
        let raw = serde_json::json!({
            "projectId": "p",
            "name": "Project",
            "rootPath": "D:/missing",
            "template": "general",
            "openedAt": "2026-07-04T00:00:00Z"
        });
        let project: RecentProject = serde_json::from_value(raw).unwrap();
        assert!(!project.missing);
    }

    #[test]
    fn recent_project_legacy_json_defaults_summary_fields() {
        let raw = r#"{
            "projectId":"p",
            "name":"Project",
            "rootPath":"D:/wiki",
            "template":"general",
            "openedAt":"2026-07-04T00:00:00Z"
        }"#;
        let recent: RecentProject = serde_json::from_str(raw).unwrap();
        assert_eq!(recent.wiki_page_count, 0);
        assert_eq!(recent.index_state, IndexState::Missing);
        assert!(!recent.missing);
    }
}
