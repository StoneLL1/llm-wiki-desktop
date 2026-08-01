use serde::{Deserialize, Serialize};

use crate::models::agent::AgentKind;
use crate::models::llm::LlmProviderKind;

/// The four skill-driven export jobs. Each maps 1:1 to a `skills/html-*`
/// folder (see [`ExportType::skill_folder`]). Adding a kind here requires a
/// matching skill folder under `src-tauri/templates/skills/`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportType {
    /// `html-beautiful-read` — long-form readable article for a single page.
    BeautifulRead,
    /// `html-knowledge-card` — compact summary card for a single page.
    KnowledgeCard,
    /// `html-concept-map` — inline-SVG concept map of a page or the wiki.
    ConceptMap,
    /// `html-project-report` — whole-wiki report.
    ProjectReport,
}

impl ExportType {
    pub const ALL: [ExportType; 4] = [
        ExportType::BeautifulRead,
        ExportType::KnowledgeCard,
        ExportType::ConceptMap,
        ExportType::ProjectReport,
    ];

    /// The `skills/html-*` folder name that drives this export. Templates only
    /// affect output styling — they never touch schema/lint/agent behavior.
    pub fn skill_folder(self) -> &'static str {
        match self {
            ExportType::BeautifulRead => "html-beautiful-read",
            ExportType::KnowledgeCard => "html-knowledge-card",
            ExportType::ConceptMap => "html-concept-map",
            ExportType::ProjectReport => "html-project-report",
        }
    }

    /// Whether the job is scoped to a single source page (vs. project-wide).
    pub fn requires_source(self) -> bool {
        matches!(
            self,
            ExportType::BeautifulRead | ExportType::KnowledgeCard | ExportType::ConceptMap
        )
    }
}

/// How the export was generated. Mirrors the resolved branch of `resolve_route`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportRoute {
    Agent,
    Byok,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportPreviewMetadata {
    pub content_type: String,
    pub byte_size: u64,
    pub content_hash: String,
    pub validation_passed: bool,
}

/// A persisted export artifact. `output_path` is always project-relative and
/// always under `exports/html/`. Stored as a list in `.app/exports.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExportRecord {
    pub id: String,
    pub export_type: ExportType,
    pub title: String,
    /// `None` for project-wide exports (`ProjectReport`, wiki-scope concept map).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    pub output_path: String,
    pub created_at: String,
    pub route: ExportRoute,
    pub status: ExportStatus,
    #[serde(default)]
    pub bookmarked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<ExportPreviewMetadata>,
}

/// Route preference, mirroring `CompileRoutePreference` but kept local to exports
/// so the feature stays self-contained (Agent preferred under `Auto`).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportRoutePreference {
    #[default]
    Auto,
    Agent,
    Byok,
}

/// User-controlled content flags for an export. These adjust the prompt only —
/// they never touch schema, lint, or agent behavior. `embed_css` reflects the
/// self-contained HTML contract (always inlined); it is kept as a flag so the
/// dialog can surface it, and the prompt reinforces it when set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportContentOptions {
    pub include_frontmatter: bool,
    pub embed_css: bool,
    pub embed_images: bool,
}

impl Default for ExportContentOptions {
    fn default() -> Self {
        Self {
            include_frontmatter: true,
            embed_css: true,
            embed_images: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartExportRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub export_type: ExportType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default)]
    pub route: ExportRoutePreference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<LlmProviderKind>,
    /// Selected template name (e.g. `default-serif`). When set, the matching
    /// skill's `template.html` styling is injected into the prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default)]
    pub options: ExportContentOptions,
    #[serde(default)]
    pub acknowledge_restricted_content: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetExportRestrictedContentStatusRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub export_type: ExportType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExportRestrictedContentStatus {
    pub contains_restricted_content: bool,
    pub restricted_source_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListExportsRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleExportBookmarkRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub export_record_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleExportBookmarkResponse {
    pub export_record_id: String,
    pub bookmarked: bool,
}

/// Regenerate from an existing record — the task re-runs with the same type
/// and source, producing a fresh timestamped output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegenerateExportRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub export_type: ExportType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default)]
    pub route: ExportRoutePreference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<LlmProviderKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(default)]
    pub options: ExportContentOptions,
    #[serde(default)]
    pub acknowledge_restricted_content: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadExportPreviewRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenExportFolderRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenExportInBrowserRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub output_path: String,
}

#[cfg(test)]
mod tests {
    use super::{
        ExportContentOptions, ExportPreviewMetadata, ExportRecord, ExportRoute, ExportStatus,
        ExportType, OpenExportInBrowserRequest,
    };

    #[test]
    fn skill_folder_maps_each_type() {
        assert_eq!(
            ExportType::BeautifulRead.skill_folder(),
            "html-beautiful-read"
        );
        assert_eq!(
            ExportType::KnowledgeCard.skill_folder(),
            "html-knowledge-card"
        );
        assert_eq!(ExportType::ConceptMap.skill_folder(), "html-concept-map");
        assert_eq!(
            ExportType::ProjectReport.skill_folder(),
            "html-project-report"
        );
    }

    #[test]
    fn requires_source_flags_single_page_jobs() {
        assert!(ExportType::BeautifulRead.requires_source());
        assert!(ExportType::KnowledgeCard.requires_source());
        assert!(!ExportType::ProjectReport.requires_source());
    }

    #[test]
    fn export_record_round_trips_camel_case() {
        let record = ExportRecord {
            id: "export-1".into(),
            export_type: ExportType::BeautifulRead,
            title: "Agent".into(),
            source_path: Some("wiki/concepts/agent.md".into()),
            output_path: "exports/html/agent-20260620-101500.html".into(),
            created_at: "2026-06-20T10:15:00Z".into(),
            route: ExportRoute::Agent,
            status: ExportStatus::Succeeded,
            bookmarked: false,
            task_id: Some("task-1".into()),
            preview: Some(ExportPreviewMetadata {
                content_type: "text/html".into(),
                byte_size: 42,
                content_hash: "a".repeat(64),
                validation_passed: true,
            }),
        };
        let value = serde_json::to_value(&record).unwrap();
        assert_eq!(value["exportType"], serde_json::json!("beautiful_read"));
        assert_eq!(
            value["outputPath"],
            serde_json::json!("exports/html/agent-20260620-101500.html")
        );
        assert_eq!(
            value["sourcePath"],
            serde_json::json!("wiki/concepts/agent.md")
        );
        assert!(value.get("export_type").is_none());
        assert_eq!(
            value["preview"]["validationPassed"],
            serde_json::json!(true)
        );

        let back: ExportRecord = serde_json::from_value(value).unwrap();
        assert_eq!(back, record);
    }

    #[test]
    fn export_type_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ExportType::ProjectReport).unwrap(),
            "\"project_report\""
        );
        assert_eq!(
            serde_json::to_string(&ExportType::ConceptMap).unwrap(),
            "\"concept_map\""
        );
    }

    #[test]
    fn content_options_default_and_round_trip() {
        let defaults = ExportContentOptions::default();
        assert!(defaults.include_frontmatter);
        assert!(defaults.embed_css);
        assert!(!defaults.embed_images);

        // Absent fields deserialize to the same defaults (camelCase keys).
        let from_empty: ExportContentOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(from_empty, defaults);

        let custom = ExportContentOptions {
            include_frontmatter: false,
            embed_css: true,
            embed_images: true,
        };
        let value = serde_json::to_value(&custom).unwrap();
        assert_eq!(value["includeFrontmatter"], serde_json::json!(false));
        assert_eq!(value["embedImages"], serde_json::json!(true));
        let back: ExportContentOptions = serde_json::from_value(value).unwrap();
        assert_eq!(back, custom);
    }

    #[test]
    fn open_export_in_browser_request_uses_camel_case() {
        let request = OpenExportInBrowserRequest {
            project_id: "project-1".into(),
            project_root_path: "D:/Projects/wiki".into(),
            output_path: "exports/html/报告.html".into(),
        };

        let value = serde_json::to_value(&request).unwrap();

        assert_eq!(value["projectId"], serde_json::json!("project-1"));
        assert_eq!(
            value["projectRootPath"],
            serde_json::json!("D:/Projects/wiki")
        );
        assert_eq!(
            value["outputPath"],
            serde_json::json!("exports/html/报告.html")
        );
        assert!(value.get("project_id").is_none());
    }
}
