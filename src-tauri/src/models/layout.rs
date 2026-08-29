use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::errors::BackendError;
use crate::utils::path_safety::{
    validate_existing_project_directory, validate_existing_project_file,
};
use crate::utils::path_utils::normalize_project_path;

const MAX_TOP_LEVEL_ENTRIES: usize = 512;
const MAX_SIGNAL_ENTRIES_PER_DIRECTORY: usize = 512;

/// The native layout is deliberately described in one place.  Opening,
/// assessment, runtime authority, and repair planning all consume this
/// descriptor so a directory cannot be considered native by one layer and
/// non-native by another.
pub const CURRENT_NATIVE_LAYOUT_VERSION: u32 = 1;

const NATIVE_SEMANTIC_FILES: &[&str] = &["purpose.md", "schema.md"];

const NATIVE_CURRENT_DIRECTORY_MARKERS: &[&str] = &[
    "raw/sources",
    "wiki",
    ".app",
    ".app/tasks",
    "exports",
    "skills",
];

/// Directories created for a newly created native project.  These are also
/// the *only* directories a legacy-layout repair may create.  The list is
/// intentionally paths, not a recursive `create_dir_all` policy: each target
/// is revalidated individually by the repair executor.
const NATIVE_REPAIR_DIRECTORY_ALLOWLIST: &[&str] = &[
    "raw",
    "raw/sources",
    "raw/sources/pdfs",
    "raw/sources/docs",
    "raw/sources/slides",
    "raw/sources/sheets",
    "raw/sources/markdown",
    "raw/sources/links",
    "raw/sources/other",
    "raw/extracted",
    "raw/assets",
    "wiki",
    "wiki/entities",
    "wiki/concepts",
    "wiki/sources",
    "wiki/queries",
    "wiki/synthesis",
    "wiki/comparisons",
    "exports",
    "exports/html",
    "skills",
    ".app",
    ".app/chats",
    ".app/tasks",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeLayoutRequirement {
    Directory(&'static str),
}

impl NativeLayoutRequirement {
    pub fn relative_path(&self) -> &'static str {
        match self {
            Self::Directory(path) => path,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeLayoutGap {
    MissingSemanticFile(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeLayoutState {
    Current,
    RepairableLegacy {
        missing: Vec<NativeLayoutRequirement>,
    },
    IncompleteLegacy {
        reasons: Vec<NativeLayoutGap>,
    },
    NotNative,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLayoutInspection {
    pub version: u32,
    pub state: NativeLayoutState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProjectMarkdownRootRole {
    Source,
    Wiki,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMarkdownRoot {
    pub path: String,
    pub role: ProjectMarkdownRootRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectContextDocument {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inferred: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLayout {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_state_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_root: Option<String>,
    pub markdown_roots: Vec<ProjectMarkdownRoot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_write_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki_write_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki_index_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki_overview_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_log_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queries_write_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_state_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_state_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile_state_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_state_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_state_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_state_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_cache_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lint_report_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lint_ignore_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_record_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bookmarks_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose_context: Option<ProjectContextDocument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_context: Option<ProjectContextDocument>,
}

/// Layout-derived paths owned by Import. Callers must use this facade instead
/// of assuming the native `.app` layout so compatible vault state remains
/// isolated under its configured app-state root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportLayoutPaths {
    app_state_root: String,
    import_state_root: String,
}

/// Layout-derived paths owned by the source registry and source artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLayoutPaths {
    app_state_root: String,
    source_state_root: String,
    evidence_root: String,
    source_write_root: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLayoutConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectLayoutWarningCode {
    LowConfidence,
    DiscoveryLimitReached,
    UnsafeEntrySkipped,
    InvalidCompatibleMapping,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLayoutWarning {
    pub code: ProjectLayoutWarningCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectLayoutResolution {
    pub layout: ProjectLayout,
    pub confidence: ProjectLayoutConfidence,
    pub warnings: Vec<ProjectLayoutWarning>,
}

/// The only app-owned record that can associate a compatible vault with
/// existing functional directories.  It is deliberately small: it does not
/// describe a native layout and it cannot create authority by itself.  The
/// command layer requires an explicit confirmation before this document is
/// written, and every mapped directory is revalidated on each open.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompatibleLayoutMapping {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki_write_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_write_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_root: Option<String>,
}

pub const COMPATIBLE_LAYOUT_MAPPING_PATH: &str = ".app/compat/layout.json";
pub const COMPATIBLE_LAYOUT_MAPPING_VERSION: u32 = 1;

impl CompatibleLayoutMapping {
    pub fn validated(
        root: &Path,
        wiki_write_root: Option<String>,
        source_root: Option<String>,
        export_root: Option<String>,
    ) -> Result<Self, BackendError> {
        // Validate lexical input before normalization.  Otherwise an input such
        // as `notes/../elsewhere` could be normalized into a harmless-looking
        // value before the mapping's traversal policy sees it.
        for (path, field) in [
            (wiki_write_root.as_deref(), "wikiWriteRoot"),
            (source_root.as_deref(), "sourceWriteRoot"),
            (export_root.as_deref(), "exportRoot"),
        ] {
            if let Some(path) = path {
                validate_compatible_relative_input(path, field)?;
            }
        }
        let mapping = Self {
            schema_version: COMPATIBLE_LAYOUT_MAPPING_VERSION,
            wiki_write_root: wiki_write_root.map(|path| normalize_project_path(&path)),
            source_write_root: source_root.map(|path| normalize_project_path(&path)),
            export_root: export_root.map(|path| normalize_project_path(&path)),
        };
        mapping.validate_existing_roots(root)?;
        Ok(mapping)
    }

    pub fn validate_existing_roots(&self, root: &Path) -> Result<(), BackendError> {
        if self.schema_version != COMPATIBLE_LAYOUT_MAPPING_VERSION {
            return Err(invalid_compatible_mapping(
                "The compatible layout mapping version is unsupported.",
            ));
        }
        let mut roots = Vec::new();
        if let Some(wiki_root) = self.wiki_write_root.as_deref() {
            validate_compatible_root(root, wiki_root, "wikiWriteRoot")?;
            roots.push(compatible_root_identity(root, wiki_root)?);
        }
        if let Some(source_root) = self.source_write_root.as_deref() {
            validate_compatible_root(root, source_root, "sourceWriteRoot")?;
            roots.push(compatible_root_identity(root, source_root)?);
        }
        if let Some(export_root) = self.export_root.as_deref() {
            validate_compatible_root(root, export_root, "exportRoot")?;
            roots.push(compatible_root_identity(root, export_root)?);
        }
        for (index, (candidate, portable_key)) in roots.iter().enumerate() {
            if roots[..index].iter().any(|(existing, existing_key)| {
                existing_key == portable_key
                    || candidate.starts_with(existing)
                    || existing.starts_with(candidate)
            }) {
                return Err(invalid_compatible_mapping(
                    "Functional directory roles must use distinct, non-overlapping existing directories.",
                ));
            }
        }
        Ok(())
    }
}

/// A bounded quick assessment must be able to stop compatible-layout
/// discovery before exploring a large ordinary materials directory.
pub struct LayoutDiscoveryBudget<'a> {
    pub deadline: Instant,
    pub cancelled: &'a AtomicBool,
}

impl ImportLayoutPaths {
    pub fn session_root(&self, session_id: &str) -> Result<String, BackendError> {
        join_layout_identity(&self.import_state_root, &[session_id])
    }

    pub fn session_manifest(&self, session_id: &str) -> Result<String, BackendError> {
        Ok(format!("{}/session.json", self.session_root(session_id)?))
    }

    pub fn active_session(&self) -> String {
        format!("{}/active-session.json", self.import_state_root)
    }

    pub fn item_root(&self, session_id: &str, item_id: &str) -> Result<String, BackendError> {
        join_layout_identity(&self.import_state_root, &[session_id, "items", item_id])
    }

    pub fn item_record(&self, session_id: &str, item_id: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/items/{item_id}.json",
            self.session_root(session_id)?
        ))
    }

    pub fn item_staging(&self, session_id: &str, item_id: &str) -> Result<String, BackendError> {
        Ok(format!("{}/staging", self.item_root(session_id, item_id)?))
    }

    pub fn item_child(
        &self,
        session_id: &str,
        item_id: &str,
        segments: &[&str],
    ) -> Result<String, BackendError> {
        let mut path = self.item_root(session_id, item_id)?;
        for segment in segments {
            path.push('/');
            path.push_str(validated_layout_identity(segment)?);
        }
        Ok(path)
    }

    pub fn item_staging_child(
        &self,
        session_id: &str,
        item_id: &str,
        segments: &[&str],
    ) -> Result<String, BackendError> {
        let mut path = self.item_staging(session_id, item_id)?;
        for segment in segments {
            path.push('/');
            path.push_str(validated_layout_identity(segment)?);
        }
        Ok(path)
    }

    pub fn item_attempts(&self, session_id: &str, item_id: &str) -> Result<String, BackendError> {
        Ok(format!("{}/attempts", self.item_root(session_id, item_id)?))
    }

    pub fn manual_merge(&self, session_id: &str, item_id: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/manual-merge.md",
            self.item_staging(session_id, item_id)?
        ))
    }

    pub fn clipboard_input_root(&self, session_id: &str) -> Result<String, BackendError> {
        Ok(format!("{}/inputs", self.session_root(session_id)?))
    }

    pub fn history_root(&self) -> String {
        format!("{}/import-history", self.app_state_root)
    }

    pub fn history_entry(&self, history_id: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/{}.json",
            self.history_root(),
            validated_layout_identity(history_id)?
        ))
    }

    pub fn history_working_root(&self, history_id: &str) -> Result<String, BackendError> {
        join_layout_identity(&format!("{}/working", self.history_root()), &[history_id])
    }

    pub fn history_working_manifest(&self, history_id: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/manifest.json",
            self.history_working_root(history_id)?
        ))
    }

    pub fn history_working_snapshot(
        &self,
        history_id: &str,
        item_id: &str,
    ) -> Result<String, BackendError> {
        Ok(format!(
            "{}/snapshots/{}.json",
            self.history_working_root(history_id)?,
            validated_layout_identity(item_id)?
        ))
    }

    pub fn history_working_result(
        &self,
        history_id: &str,
        sequence: u64,
        item_id: &str,
    ) -> Result<String, BackendError> {
        Ok(format!(
            "{}/results/{sequence:08}-{}.json",
            self.history_working_root(history_id)?,
            validated_layout_identity(item_id)?
        ))
    }

    pub fn history_preview(&self, history_id: &str, item_id: &str) -> Result<String, BackendError> {
        let root = format!("{}/import-history-previews", self.app_state_root);
        Ok(format!(
            "{}/{}.md",
            join_layout_identity(&root, &[history_id])?,
            validated_layout_identity(item_id)?
        ))
    }

    pub fn recovery_journal_root(&self) -> String {
        format!("{}/import-v2-journal", self.app_state_root)
    }

    pub fn cleanup_root(&self) -> String {
        format!("{}/import-cleanup", self.app_state_root)
    }

    pub fn staging_root(&self) -> String {
        format!("{}/import-staging", self.app_state_root)
    }
}

impl SourceLayoutPaths {
    pub fn manifest(&self, source_id: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/{}.json",
            self.source_state_root,
            validated_layout_identity(source_id)?
        ))
    }

    pub fn index(&self) -> String {
        format!("{}/source-index-v2.json", self.app_state_root)
    }

    pub fn local_evidence_root(
        &self,
        source_id: &str,
        version_id: &str,
    ) -> Result<String, BackendError> {
        join_layout_identity(
            &format!("{}/sources", self.evidence_root),
            &[source_id, version_id],
        )
    }

    pub fn web_evidence_root(
        &self,
        source_id: &str,
        version_id: &str,
    ) -> Result<String, BackendError> {
        join_layout_identity(
            &format!("{}/web", self.evidence_root),
            &[source_id, version_id],
        )
    }

    pub fn asset_root(&self, source_id: &str, version_id: &str) -> Result<String, BackendError> {
        join_layout_identity(
            &format!("{}/assets", self.evidence_root),
            &[source_id, version_id],
        )
    }

    pub fn artifact_version_root(
        &self,
        source_id: &str,
        version_id: &str,
    ) -> Result<String, BackendError> {
        join_layout_identity(
            &format!("{}/source-artifacts", self.app_state_root),
            &[source_id, version_id],
        )
    }

    pub fn artifact_source_root(&self, source_id: &str) -> Result<String, BackendError> {
        join_layout_identity(
            &format!("{}/source-artifacts", self.app_state_root),
            &[source_id],
        )
    }

    pub fn baseline(&self, source_id: &str, version_id: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/baseline.md",
            self.artifact_version_root(source_id, version_id)?
        ))
    }

    pub fn artifact_package_file(
        &self,
        source_id: &str,
        version_id: &str,
        file_name: &str,
    ) -> Result<String, BackendError> {
        Ok(format!(
            "{}/package/{}",
            self.artifact_version_root(source_id, version_id)?,
            validated_layout_identity(file_name)?
        ))
    }

    pub fn candidate_root(&self, source_id: &str) -> Result<String, BackendError> {
        join_layout_identity(
            &format!("{}/source-candidates", self.app_state_root),
            &[source_id],
        )
    }

    pub fn candidate(&self, source_id: &str, candidate_id: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/{}.json",
            self.candidate_root(source_id)?,
            validated_layout_identity(candidate_id)?
        ))
    }

    pub fn candidate_evidence_root(
        &self,
        source_id: &str,
        candidate_id: &str,
    ) -> Result<String, BackendError> {
        join_layout_identity(
            &format!("{}/source-candidate-evidence", self.app_state_root),
            &[source_id, candidate_id],
        )
    }

    pub fn candidate_evidence(
        &self,
        source_id: &str,
        candidate_id: &str,
        index: usize,
    ) -> Result<String, BackendError> {
        Ok(format!(
            "{}/{index}.bin",
            self.candidate_evidence_root(source_id, candidate_id)?
        ))
    }

    pub fn source_candidate_evidence_root(&self, source_id: &str) -> Result<String, BackendError> {
        join_layout_identity(
            &format!("{}/source-candidate-evidence", self.app_state_root),
            &[source_id],
        )
    }

    pub fn deletion_audit(&self, audit_id: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/source-audit/deletions/{}.json",
            self.app_state_root,
            validated_layout_identity(audit_id)?
        ))
    }

    pub fn local_source_root(&self, source_id: &str) -> Result<String, BackendError> {
        join_layout_identity(&format!("{}/sources", self.evidence_root), &[source_id])
    }

    pub fn web_source_root(&self, source_id: &str) -> Result<String, BackendError> {
        join_layout_identity(&format!("{}/web", self.evidence_root), &[source_id])
    }

    pub fn asset_source_root(&self, source_id: &str) -> Result<String, BackendError> {
        join_layout_identity(&format!("{}/assets", self.evidence_root), &[source_id])
    }

    pub fn local_derived_file(
        &self,
        source_id: &str,
        version_id: &str,
        file_name: &str,
    ) -> Result<String, BackendError> {
        Ok(format!(
            "{}/derived/{}",
            self.local_evidence_root(source_id, version_id)?,
            validated_layout_identity(file_name)?
        ))
    }

    pub fn contains_source_markdown(&self, path: &str) -> bool {
        let normalized = normalize_project_path(path);
        normalized == self.source_write_root
            || normalized.starts_with(&format!("{}/", self.source_write_root))
    }

    pub fn source_write_root(&self) -> &str {
        &self.source_write_root
    }

    pub fn local_markdown(&self, file_name: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/local/{}",
            self.source_write_root,
            validated_layout_identity(file_name)?
        ))
    }

    pub fn web_markdown(&self, file_name: &str) -> Result<String, BackendError> {
        Ok(format!(
            "{}/web/{}",
            self.source_write_root,
            validated_layout_identity(file_name)?
        ))
    }

    pub fn web_markdown_in_host(
        &self,
        host: &str,
        file_name: &str,
    ) -> Result<String, BackendError> {
        Ok(format!(
            "{}/web/{}/{}",
            self.source_write_root,
            validated_layout_identity(host)?,
            validated_layout_identity(file_name)?
        ))
    }
}

impl ProjectLayout {
    pub fn import_paths(&self) -> Result<ImportLayoutPaths, BackendError> {
        Ok(ImportLayoutPaths {
            app_state_root: required_layout_root(&self.app_state_root, "appStateRoot")?,
            import_state_root: required_layout_root(&self.import_state_root, "importStateRoot")?,
        })
    }

    pub fn source_paths(&self) -> Result<SourceLayoutPaths, BackendError> {
        Ok(SourceLayoutPaths {
            app_state_root: required_layout_root(&self.app_state_root, "appStateRoot")?,
            source_state_root: required_layout_root(&self.source_state_root, "sourceStateRoot")?,
            evidence_root: required_layout_root(&self.evidence_root, "evidenceRoot")?,
            source_write_root: required_layout_root(&self.source_write_root, "sourceWriteRoot")?,
        })
    }

    pub fn native() -> Self {
        Self {
            app_state_root: some(".app"),
            evidence_root: some("raw"),
            markdown_roots: vec![
                ProjectMarkdownRoot {
                    path: "wiki".into(),
                    role: ProjectMarkdownRootRole::Wiki,
                    exclude: None,
                },
                ProjectMarkdownRoot {
                    path: "wiki/sources".into(),
                    role: ProjectMarkdownRootRole::Source,
                    exclude: None,
                },
                ProjectMarkdownRoot {
                    path: "raw/extracted".into(),
                    role: ProjectMarkdownRootRole::Source,
                    exclude: None,
                },
            ],
            source_write_root: some("wiki/sources"),
            wiki_write_root: some("wiki"),
            wiki_index_path: some("wiki/index.md"),
            wiki_overview_path: some("wiki/overview.md"),
            activity_log_path: some("wiki/log.md"),
            queries_write_root: some("wiki/queries"),
            export_root: some("exports/html"),
            skills_root: some("skills"),
            import_state_root: some(".app/import-sessions"),
            source_state_root: some(".app/sources"),
            compile_state_root: some(".app/compile"),
            chat_state_root: some(".app/chats"),
            task_state_root: some(".app/tasks"),
            workflow_state_root: some(".app/workflows"),
            graph_cache_path: some(".app/graph-cache.json"),
            lint_report_root: some(".app/lint-reports"),
            lint_ignore_path: some(".app/lint-ignore.json"),
            export_record_path: some(".app/exports.json"),
            bookmarks_path: some(".app/bookmarks.json"),
            settings_path: some(".app/settings.json"),
            agent_config_path: some(".app/agent-config.json"),
            purpose_context: Some(ProjectContextDocument {
                read_path: some("purpose.md"),
                write_path: some("purpose.md"),
                inferred: Some(false),
            }),
            schema_context: Some(ProjectContextDocument {
                read_path: some("schema.md"),
                write_path: some("schema.md"),
                inferred: Some(false),
            }),
        }
    }

    pub fn list_markdown_files(
        &self,
        project_root: &Path,
        roles: &[ProjectMarkdownRootRole],
    ) -> Result<Vec<PathBuf>, BackendError> {
        let canonical_root = project_root
            .canonicalize()
            .map_err(|error| layout_io_error(error, project_root))?;
        let wanted = roles.iter().copied().collect::<HashSet<_>>();
        let legacy_native_scan = self.app_state_root.as_deref() == Some(".app")
            && self.evidence_root.as_deref() == Some("raw");
        let mut seen_files = HashSet::new();
        let mut seen_directories = HashSet::new();
        let mut files = Vec::new();
        for markdown_root in &self.markdown_roots {
            if !wanted.contains(&markdown_root.role) {
                continue;
            }
            let scan_root = resolve_layout_path(project_root, &markdown_root.path)?;
            if !scan_root.exists() {
                continue;
            }
            let entered_via_link = project_descendant_path_enters_link(project_root, &scan_root)?;
            let excludes = markdown_root
                .exclude
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|path| normalize_project_path(path))
                .collect::<Vec<_>>();
            walk_markdown_root(
                &canonical_root,
                &scan_root,
                entered_via_link,
                &excludes,
                markdown_root.path != ".",
                legacy_native_scan,
                &mut seen_directories,
                &mut seen_files,
                &mut files,
            )?;
        }
        files.sort();
        Ok(files)
    }
}

pub fn resolve_layout(root: &Path) -> Result<ProjectLayoutResolution, BackendError> {
    resolve_layout_with_budget(root, None)
}

pub fn resolve_layout_with_budget(
    root: &Path,
    budget: Option<&LayoutDiscoveryBudget<'_>>,
) -> Result<ProjectLayoutResolution, BackendError> {
    check_discovery_budget(budget)?;
    if matches!(
        inspect_native_layout(root).state,
        NativeLayoutState::Current | NativeLayoutState::RepairableLegacy { .. }
    ) {
        return Ok(ProjectLayoutResolution {
            layout: ProjectLayout::native(),
            confidence: ProjectLayoutConfidence::High,
            warnings: Vec::new(),
        });
    }
    discover_compatible_layout(root, budget)
}

fn discover_compatible_layout(
    root: &Path,
    budget: Option<&LayoutDiscoveryBudget<'_>>,
) -> Result<ProjectLayoutResolution, BackendError> {
    let mut warnings = Vec::new();
    let entries = fs::read_dir(root).map_err(|error| layout_io_error(error, root))?;
    let mut root_excludes = Vec::new();
    let mut directory_roots = Vec::new();
    let mut has_root_markdown = false;
    let mut has_root_index = false;
    let mut truncated = false;

    for (index, entry) in entries.enumerate() {
        check_discovery_budget(budget)?;
        if index >= MAX_TOP_LEVEL_ENTRIES {
            truncated = true;
            break;
        }
        let entry = entry.map_err(|error| layout_io_error(error, root))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(layout_io_error(error, &path)),
        };
        if is_link_or_reparse(&metadata) {
            warnings.push(ProjectLayoutWarning {
                code: ProjectLayoutWarningCode::UnsafeEntrySkipped,
                message: "A linked or reparse-point entry was excluded from layout discovery."
                    .into(),
                path: Some(normalize_project_path(&name)),
            });
            continue;
        }
        if metadata.is_file() && is_markdown_path(&path, true) {
            has_root_markdown = true;
            has_root_index |= name.eq_ignore_ascii_case("index.md");
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        root_excludes.push(normalize_project_path(&name));
        if ignored_compatible_directory(&name) {
            continue;
        }
        if bounded_markdown_signal(&path, budget)? {
            directory_roots.push(ProjectMarkdownRoot {
                path: normalize_project_path(&name),
                role: compatible_role(&name),
                exclude: None,
            });
        }
    }

    if truncated {
        warnings.push(ProjectLayoutWarning {
            code: ProjectLayoutWarningCode::DiscoveryLimitReached,
            message: "Layout discovery reached its bounded top-level entry limit.".into(),
            path: None,
        });
    }

    let has_obsidian = safe_directory_marker(root, ".obsidian");
    let confidence = if has_obsidian {
        ProjectLayoutConfidence::High
    } else if has_root_index || !directory_roots.is_empty() {
        ProjectLayoutConfidence::Medium
    } else {
        ProjectLayoutConfidence::Low
    };
    if confidence == ProjectLayoutConfidence::Low {
        warnings.push(ProjectLayoutWarning {
            code: ProjectLayoutWarningCode::LowConfidence,
            message: "Only conservative root-level Markdown could be identified.".into(),
            path: None,
        });
    }

    let mut markdown_roots = Vec::new();
    if has_root_markdown || has_obsidian {
        root_excludes.sort();
        markdown_roots.push(ProjectMarkdownRoot {
            path: ".".into(),
            role: ProjectMarkdownRootRole::Mixed,
            exclude: (!root_excludes.is_empty()).then_some(root_excludes),
        });
    }
    directory_roots.sort_by(|a, b| a.path.cmp(&b.path));
    markdown_roots.extend(directory_roots);

    let root_purpose = safe_file_marker(root, "purpose.md");
    let root_schema = safe_file_marker(root, "schema.md");
    let compat_purpose = safe_file_marker(root, ".app/compat/purpose.md");
    let compat_schema = safe_file_marker(root, ".app/compat/schema.md");
    let compat_enabled = compat_purpose && compat_schema;
    let compatible_mapping = if compat_enabled {
        match read_compatible_layout_mapping(root) {
            Ok(mapping) => mapping,
            Err(error) => {
                warnings.push(ProjectLayoutWarning {
                    code: ProjectLayoutWarningCode::InvalidCompatibleMapping,
                    message: "The compatible layout mapping is unavailable or unsafe; content write capabilities remain disabled.".into(),
                    path: Some(COMPATIBLE_LAYOUT_MAPPING_PATH.into()),
                });
                let _ = error;
                None
            }
        }
    } else {
        None
    };
    if let Some(mapping) = compatible_mapping.as_ref() {
        if let Some(wiki_root) = mapping.wiki_write_root.as_deref() {
            add_mapped_markdown_root(
                &mut markdown_roots,
                wiki_root,
                ProjectMarkdownRootRole::Wiki,
            );
        }
        if let Some(source_root) = mapping.source_write_root.as_deref() {
            add_mapped_markdown_root(
                &mut markdown_roots,
                source_root,
                ProjectMarkdownRootRole::Source,
            );
        }
    }
    Ok(ProjectLayoutResolution {
        layout: ProjectLayout {
            // Compatible vault state is intentionally isolated from the
            // user's Markdown layout.  Having this root never grants a
            // content-write capability: compatible adapters still expose no
            // source/wiki/export write roots until a later explicit mapping.
            app_state_root: compat_enabled.then(|| ".app/compat".into()),
            evidence_root: compat_enabled.then(|| ".app/compat/evidence".into()),
            markdown_roots,
            source_write_root: compatible_mapping
                .as_ref()
                .and_then(|mapping| mapping.source_write_root.clone()),
            wiki_write_root: compatible_mapping
                .as_ref()
                .and_then(|mapping| mapping.wiki_write_root.clone()),
            wiki_index_path: compatible_mapping
                .as_ref()
                .and_then(|mapping| mapping.wiki_write_root.as_deref())
                .and_then(|wiki_root| compatible_index_path(root, wiki_root))
                .or_else(|| has_root_index.then(|| "index.md".into())),
            wiki_overview_path: None,
            activity_log_path: None,
            queries_write_root: None,
            export_root: compatible_mapping
                .as_ref()
                .and_then(|mapping| mapping.export_root.clone()),
            skills_root: None,
            import_state_root: compat_enabled.then(|| ".app/compat/import-sessions".into()),
            source_state_root: compat_enabled.then(|| ".app/compat/sources".into()),
            compile_state_root: compat_enabled.then(|| ".app/compat/compile".into()),
            chat_state_root: compat_enabled.then(|| ".app/compat/chats".into()),
            task_state_root: compat_enabled.then(|| ".app/compat/tasks".into()),
            workflow_state_root: compat_enabled.then(|| ".app/compat/workflows".into()),
            graph_cache_path: compat_enabled.then(|| ".app/compat/graph-cache.json".into()),
            lint_report_root: compat_enabled.then(|| ".app/compat/lint-reports".into()),
            lint_ignore_path: compat_enabled.then(|| ".app/compat/lint-ignore.json".into()),
            export_record_path: compat_enabled.then(|| ".app/compat/exports.json".into()),
            bookmarks_path: compat_enabled.then(|| ".app/compat/bookmarks.json".into()),
            settings_path: compat_enabled.then(|| ".app/compat/settings.json".into()),
            agent_config_path: compat_enabled.then(|| ".app/compat/agent-config.json".into()),
            purpose_context: if compat_purpose {
                Some(ProjectContextDocument {
                    read_path: some(".app/compat/purpose.md"),
                    write_path: some(".app/compat/purpose.md"),
                    inferred: Some(false),
                })
            } else {
                root_purpose.then(|| ProjectContextDocument {
                    read_path: some("purpose.md"),
                    write_path: None,
                    inferred: Some(true),
                })
            },
            schema_context: if compat_schema {
                Some(ProjectContextDocument {
                    read_path: some(".app/compat/schema.md"),
                    write_path: some(".app/compat/schema.md"),
                    inferred: Some(false),
                })
            } else {
                root_schema.then(|| ProjectContextDocument {
                    read_path: some("schema.md"),
                    write_path: None,
                    inferred: Some(true),
                })
            },
        },
        confidence,
        warnings,
    })
}

/// Reads only the app-owned mapping.  A missing mapping is the normal
/// restricted-compatible state; malformed, linked, or stale mappings fail
/// closed so persistence never becomes a content-write grant.
pub fn read_compatible_layout_mapping(
    root: &Path,
) -> Result<Option<CompatibleLayoutMapping>, BackendError> {
    let path = root.join(COMPATIBLE_LAYOUT_MAPPING_PATH);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(layout_io_error(error, &path)),
        Ok(_) => {
            let safe_path = validate_existing_project_file(root, &path).map_err(|_| {
                invalid_compatible_mapping(
                    "The compatible layout mapping is not a safe regular file.",
                )
            })?;
            let bytes = fs::read(&safe_path).map_err(|error| layout_io_error(error, &safe_path))?;
            let mapping =
                serde_json::from_slice::<CompatibleLayoutMapping>(&bytes).map_err(|_| {
                    invalid_compatible_mapping("The compatible layout mapping is not valid JSON.")
                })?;
            mapping.validate_existing_roots(root)?;
            Ok(Some(mapping))
        }
    }
}

fn validate_compatible_root(root: &Path, relative: &str, field: &str) -> Result<(), BackendError> {
    validate_compatible_relative_input(relative, field)?;
    let normalized = normalize_project_path(relative);
    if normalized.starts_with(".app/") || normalized == ".app" {
        return Err(invalid_compatible_mapping(&format!(
            "{field} cannot point into app-owned compatibility state."
        )));
    }
    let path = resolve_layout_path(root, &normalized)?;
    validate_existing_project_directory(root, &path).map_err(|_| {
        invalid_compatible_mapping(&format!(
            "{field} must be an existing safe directory inside the project."
        ))
    })?;
    Ok(())
}

fn validate_compatible_relative_input(relative: &str, field: &str) -> Result<(), BackendError> {
    if relative.is_empty()
        || Path::new(relative).is_absolute()
        || relative
            .split(['/', '\\'])
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(invalid_compatible_mapping(&format!(
            "{field} must be a non-empty project-relative directory."
        )));
    }
    Ok(())
}

fn compatible_root_identity(
    root: &Path,
    relative: &str,
) -> Result<(PathBuf, String), BackendError> {
    let path = resolve_layout_path(root, relative)?;
    let canonical = path.canonicalize().map_err(|_| {
        invalid_compatible_mapping(
            "Functional directories must remain available while configuring compatibility.",
        )
    })?;
    let portable_key = canonical
        .to_string_lossy()
        .nfc()
        .collect::<String>()
        .trim_end_matches(['.', ' '])
        .to_lowercase();
    Ok((canonical, portable_key))
}

fn invalid_compatible_mapping(message: &str) -> BackendError {
    BackendError::new("PROJECT_COMPAT_LAYOUT_INVALID", message, true, true)
}

fn add_mapped_markdown_root(
    roots: &mut Vec<ProjectMarkdownRoot>,
    path: &str,
    role: ProjectMarkdownRootRole,
) {
    if let Some(existing) = roots.iter_mut().find(|root| root.path == path) {
        existing.role = role;
        return;
    }
    roots.push(ProjectMarkdownRoot {
        path: path.into(),
        role,
        exclude: None,
    });
    roots.sort_by(|left, right| left.path.cmp(&right.path));
}

fn compatible_index_path(root: &Path, wiki_root: &str) -> Option<String> {
    let candidate = resolve_layout_path(root, &format!("{wiki_root}/index.md")).ok()?;
    validate_existing_project_file(root, &candidate).ok()?;
    Some(format!("{wiki_root}/index.md"))
}

/// Inspects the current native layout without writing.  A legacy candidate
/// needs both semantic files and an existing native structural anchor; this
/// prevents an ordinary materials folder from becoming native merely because
/// a repair could create empty directories in it.
pub fn inspect_native_layout(root: &Path) -> NativeLayoutInspection {
    let missing_semantic = NATIVE_SEMANTIC_FILES
        .iter()
        .copied()
        .filter(|relative| !safe_file_marker(root, relative))
        .map(NativeLayoutGap::MissingSemanticFile)
        .collect::<Vec<_>>();
    let has_native_anchor = ["raw", "wiki", "exports", "skills"]
        .iter()
        .any(|relative| safe_directory_marker(root, relative));
    let has_current_native_directories = NATIVE_CURRENT_DIRECTORY_MARKERS
        .iter()
        .all(|relative| safe_directory_marker(root, relative));
    let has_unsafe_native_directory = NATIVE_CURRENT_DIRECTORY_MARKERS
        .iter()
        .any(|relative| native_directory_path_is_unsafe(root, relative));
    // A native project may be opened in an editor that creates an Obsidian
    // settings directory, so a complete current native layout remains native.
    // A partial layout, however, must never use generic user files as a
    // pretext to turn a compatible vault into a native repair candidate.
    let recognized_compatible_layout = safe_directory_marker(root, ".obsidian")
        || (safe_file_marker(root, ".app/compat/purpose.md")
            && safe_file_marker(root, ".app/compat/schema.md"));

    let state = if has_unsafe_native_directory {
        NativeLayoutState::NotNative
    } else if missing_semantic.is_empty() && has_current_native_directories {
        NativeLayoutState::Current
    } else if recognized_compatible_layout {
        NativeLayoutState::NotNative
    } else if !missing_semantic.is_empty() {
        if has_native_anchor {
            NativeLayoutState::IncompleteLegacy {
                reasons: missing_semantic,
            }
        } else {
            NativeLayoutState::NotNative
        }
    } else if !has_native_anchor {
        NativeLayoutState::NotNative
    } else {
        let missing = NATIVE_REPAIR_DIRECTORY_ALLOWLIST
            .iter()
            .copied()
            .filter(|relative| !safe_directory_marker(root, relative))
            .map(NativeLayoutRequirement::Directory)
            .collect::<Vec<_>>();
        NativeLayoutState::RepairableLegacy { missing }
    };

    NativeLayoutInspection {
        version: CURRENT_NATIVE_LAYOUT_VERSION,
        state,
    }
}

pub fn native_repair_directory_allowed(relative: &str) -> bool {
    NATIVE_REPAIR_DIRECTORY_ALLOWLIST.contains(&relative)
}

fn compatible_role(name: &str) -> ProjectMarkdownRootRole {
    match name.to_ascii_lowercase().as_str() {
        "source" | "sources" | "raw" | "materials" | "evidence" | "references" => {
            ProjectMarkdownRootRole::Source
        }
        "wiki" | "pages" | "knowledge" => ProjectMarkdownRootRole::Wiki,
        _ => ProjectMarkdownRootRole::Mixed,
    }
}

fn ignored_compatible_directory(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with('.')
        || matches!(
            lower.as_str(),
            "node_modules" | "target" | "dist" | "build" | "exports" | "skills"
        )
}

fn bounded_markdown_signal(
    directory: &Path,
    budget: Option<&LayoutDiscoveryBudget<'_>>,
) -> Result<bool, BackendError> {
    bounded_markdown_signal_at_depth(directory, 0, budget)
}

fn bounded_markdown_signal_at_depth(
    directory: &Path,
    depth: usize,
    budget: Option<&LayoutDiscoveryBudget<'_>>,
) -> Result<bool, BackendError> {
    let entries = fs::read_dir(directory).map_err(|error| layout_io_error(error, directory))?;
    for (index, entry) in entries.enumerate() {
        check_discovery_budget(budget)?;
        if index >= MAX_SIGNAL_ENTRIES_PER_DIRECTORY {
            break;
        }
        let entry = entry.map_err(|error| layout_io_error(error, directory))?;
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(layout_io_error(error, &path)),
        };
        if is_link_or_reparse(&metadata) {
            continue;
        }
        if metadata.is_file() && is_markdown_path(&path, true) {
            return Ok(true);
        }
        if depth == 0
            && metadata.is_dir()
            && !ignored_scan_directory(&entry.file_name().to_string_lossy(), false)
            && bounded_markdown_signal_at_depth(&path, depth + 1, budget)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn check_discovery_budget(budget: Option<&LayoutDiscoveryBudget<'_>>) -> Result<(), BackendError> {
    let Some(budget) = budget else {
        return Ok(());
    };
    if budget.cancelled.load(Ordering::SeqCst) {
        return Err(BackendError::new(
            "PROJECT_ASSESSMENT_CANCELLED",
            "Project assessment was cancelled.",
            true,
            false,
        ));
    }
    if Instant::now() >= budget.deadline {
        return Err(BackendError::new(
            "PROJECT_ASSESSMENT_TIMEOUT",
            "Project assessment exceeded its bounded discovery budget.",
            true,
            false,
        ));
    }
    Ok(())
}

fn walk_markdown_root(
    canonical_root: &Path,
    current: &Path,
    entered_via_link: bool,
    excludes: &[String],
    recursive: bool,
    legacy_native_scan: bool,
    seen_directories: &mut HashSet<PathBuf>,
    seen_files: &mut HashSet<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> Result<(), BackendError> {
    let Some(canonical_current) =
        canonical_internal_read_path(canonical_root, current, entered_via_link)?
    else {
        return Ok(());
    };
    let metadata = fs::metadata(&canonical_current)
        .map_err(|error| layout_io_error(error, &canonical_current))?;
    if !metadata.is_dir() || !seen_directories.insert(canonical_current.clone()) {
        return Ok(());
    }
    let mut entries = fs::read_dir(&canonical_current)
        .map_err(|error| layout_io_error(error, &canonical_current))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| layout_io_error(error, &canonical_current))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(layout_io_error(error, &path)),
        };
        let entry_is_link = is_link_or_reparse(&metadata);
        let entered_via_link = entered_via_link || entry_is_link;
        let Some(canonical_path) =
            canonical_internal_read_path(canonical_root, &path, entered_via_link)?
        else {
            continue;
        };
        let relative = canonical_path
            .strip_prefix(canonical_root)
            .expect("contained canonical path");
        let normalized = normalize_project_path(&relative.to_string_lossy());
        if excluded(&normalized, excludes) {
            continue;
        }
        let metadata = match fs::metadata(&canonical_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(layout_io_error(error, &canonical_path)),
        };
        if metadata.is_dir() {
            if !recursive {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if ignored_scan_directory(&name, legacy_native_scan) {
                continue;
            }
            walk_markdown_root(
                canonical_root,
                &canonical_path,
                entered_via_link,
                excludes,
                true,
                legacy_native_scan,
                seen_directories,
                seen_files,
                files,
            )?;
        } else if metadata.is_file()
            && is_markdown_path(&canonical_path, !legacy_native_scan)
            && seen_files.insert(canonical_path.clone())
        {
            files.push(canonical_path);
        }
    }
    Ok(())
}

/// Resolve a read candidate through a descendant link only after proving that
/// its final physical location remains below the canonical project root. The
/// returned path is canonical, so a link loop or multiple aliases collapse to
/// the same visited directory/file. This is deliberately read-only: write
/// paths continue to use the stricter no-link helpers in `path_safety`.
pub(crate) fn canonical_internal_read_path(
    canonical_root: &Path,
    candidate: &Path,
    entered_via_link: bool,
) -> Result<Option<PathBuf>, BackendError> {
    let canonical = match candidate.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(layout_io_error(error, candidate)),
    };
    if !canonical.starts_with(canonical_root)
        || (entered_via_link
            && (canonical == canonical_root
                || canonical_read_target_is_sensitive(canonical_root, &canonical)))
    {
        return Ok(None);
    }
    Ok(Some(canonical))
}

/// Whether reaching a project-descendant path requires crossing a link or a
/// Windows reparse point. A selected project root may itself be a link; the
/// caller has already canonicalized that root during project admission, so the
/// root component is intentionally not inspected here.
pub(crate) fn project_descendant_path_enters_link(
    project_root: &Path,
    candidate: &Path,
) -> Result<bool, BackendError> {
    let relative = candidate.strip_prefix(project_root).map_err(|_| {
        BackendError::new(
            "PROJECT_LAYOUT_PATH_INVALID",
            "Project layout paths must stay below the selected project root.",
            false,
            true,
        )
    })?;
    let mut current = project_root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(BackendError::new(
                "PROJECT_LAYOUT_PATH_INVALID",
                "Project layout paths must be project-relative.",
                false,
                true,
            ));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_link_or_reparse(&metadata) => return Ok(true),
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(layout_io_error(error, &current)),
        }
    }
    Ok(false)
}

/// App/runtime state and native output roots are not Markdown discovery input,
/// even when a user-created link points back to them from a readable root.
/// That prevents an internal link from bypassing native layout boundaries.
fn canonical_read_target_is_sensitive(canonical_root: &Path, candidate: &Path) -> bool {
    candidate
        .strip_prefix(canonical_root)
        .ok()
        .and_then(|relative| relative.components().next())
        .and_then(|component| match component {
            Component::Normal(name) => Some(name.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .is_some_and(|name| {
            matches!(
                name.as_str(),
                ".app"
                    | ".git"
                    | ".obsidian"
                    | "raw"
                    | "exports"
                    | "skills"
                    | "node_modules"
                    | "target"
            )
        })
}

fn resolve_layout_path(project_root: &Path, relative: &str) -> Result<PathBuf, BackendError> {
    let normalized = normalize_project_path(relative);
    if normalized == "." {
        return Ok(project_root.to_path_buf());
    }
    let mut result = project_root.to_path_buf();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(segment) => result.push(segment),
            _ => {
                return Err(BackendError::new(
                    "PROJECT_LAYOUT_PATH_INVALID",
                    "Project layout paths must be project-relative.",
                    false,
                    true,
                ))
            }
        }
    }
    Ok(result)
}

fn excluded(path: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|exclude| {
        path == exclude
            || path
                .strip_prefix(exclude)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

fn ignored_scan_directory(name: &str, legacy_native_scan: bool) -> bool {
    if legacy_native_scan {
        matches!(name, ".obsidian" | ".git" | ".app")
    } else {
        name.starts_with('.')
            || matches!(
                name.to_ascii_lowercase().as_str(),
                "node_modules" | "target"
            )
    }
}

fn is_markdown_path(path: &Path, case_insensitive: bool) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            if case_insensitive {
                extension.eq_ignore_ascii_case("md")
            } else {
                extension == "md"
            }
        })
}

fn safe_directory_marker(root: &Path, relative: &str) -> bool {
    validate_existing_project_directory(root, &root.join(relative)).is_ok()
}

/// A missing native directory is a repair candidate, but a link, reparse
/// point, or non-directory in its path is never a safe substitute. Keep those
/// states outside the repair machine so a confirmation cannot turn an unsafe
/// marker into a native write root.
fn native_directory_path_is_unsafe(root: &Path, relative: &str) -> bool {
    let mut current = root.to_path_buf();
    for component in Path::new(relative).components() {
        let std::path::Component::Normal(segment) = component else {
            return true;
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if is_link_or_reparse(&metadata) || !metadata.is_dir() => return true,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
            Err(_) => return true,
        }
    }
    false
}

fn safe_file_marker(root: &Path, relative: &str) -> bool {
    validate_existing_project_file(root, &root.join(relative)).is_ok()
}

fn some(value: &str) -> Option<String> {
    Some(value.into())
}

fn required_layout_root(value: &Option<String>, field: &str) -> Result<String, BackendError> {
    value.clone().ok_or_else(|| {
        BackendError::new(
            "PROJECT_LAYOUT_ROOT_UNAVAILABLE",
            format!("The project layout does not expose {field}."),
            false,
            false,
        )
        .with_details(serde_json::json!({ "field": field }))
    })
}

fn validated_layout_identity(value: &str) -> Result<&str, BackendError> {
    if value.is_empty()
        || matches!(value, "." | "..")
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
    {
        return Err(BackendError::new(
            "PROJECT_LAYOUT_PATH_INVALID",
            "A layout identity contains an unsafe path component.",
            false,
            false,
        ));
    }
    Ok(value)
}

fn join_layout_identity(root: &str, segments: &[&str]) -> Result<String, BackendError> {
    let mut path = normalize_project_path(root)
        .trim_end_matches('/')
        .to_string();
    for segment in segments {
        path.push('/');
        path.push_str(validated_layout_identity(segment)?);
    }
    Ok(path)
}

fn layout_io_error(error: std::io::Error, path: &Path) -> BackendError {
    BackendError::new("PROJECT_LAYOUT_READ_FAILED", error.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

pub(crate) fn is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(suffix: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-layout-{stamp}-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn write(root: &Path, relative: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, "# Page").unwrap();
    }

    #[test]
    fn native_layout_matches_the_shared_contract() {
        let expected: ProjectLayout = serde_json::from_str(include_str!(
            "../../../test-fixtures/project-layout-contract.json"
        ))
        .unwrap();
        assert_eq!(ProjectLayout::native(), expected);
    }

    #[test]
    fn compatible_mapping_enables_only_confirmed_existing_cjk_roots() {
        let root = temp_root("compatible-mapping-cjk");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::create_dir_all(root.join("知识库")).unwrap();
        fs::create_dir_all(root.join("资料")).unwrap();
        fs::create_dir_all(root.join("导出")).unwrap();
        fs::create_dir_all(root.join(".app/compat")).unwrap();
        write(&root, ".app/compat/purpose.md");
        write(&root, ".app/compat/schema.md");
        fs::write(
            root.join(COMPATIBLE_LAYOUT_MAPPING_PATH),
            serde_json::to_vec_pretty(&CompatibleLayoutMapping {
                schema_version: COMPATIBLE_LAYOUT_MAPPING_VERSION,
                wiki_write_root: Some("知识库".into()),
                source_write_root: Some("资料".into()),
                export_root: Some("导出".into()),
            })
            .unwrap(),
        )
        .unwrap();

        let resolution = resolve_layout(&root).unwrap();
        assert_eq!(resolution.layout.wiki_write_root.as_deref(), Some("知识库"));
        assert_eq!(resolution.layout.source_write_root.as_deref(), Some("资料"));
        assert_eq!(resolution.layout.export_root.as_deref(), Some("导出"));
        assert!(resolution.layout.markdown_roots.iter().any(|item| {
            item.path == "知识库" && item.role == ProjectMarkdownRootRole::Wiki
        }));
        assert!(resolution
            .layout
            .markdown_roots
            .iter()
            .any(|item| { item.path == "资料" && item.role == ProjectMarkdownRootRole::Source }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_compatible_mapping_fails_closed_without_content_write_roots() {
        let root = temp_root("compatible-mapping-invalid");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::create_dir_all(root.join(".app/compat")).unwrap();
        write(&root, ".app/compat/purpose.md");
        write(&root, ".app/compat/schema.md");
        fs::write(
            root.join(COMPATIBLE_LAYOUT_MAPPING_PATH),
            r#"{"version":1,"wikiRoot":"../outside"}"#,
        )
        .unwrap();

        let resolution = resolve_layout(&root).unwrap();
        assert!(resolution.layout.wiki_write_root.is_none());
        assert!(resolution.layout.source_write_root.is_none());
        assert!(resolution.layout.export_root.is_none());
        assert!(resolution
            .warnings
            .iter()
            .any(|warning| { warning.code == ProjectLayoutWarningCode::InvalidCompatibleMapping }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compatible_mapping_rejects_app_owned_and_missing_roots() {
        let root = temp_root("compatible-mapping-root-validation");
        fs::create_dir_all(root.join("pages")).unwrap();
        let app_owned =
            CompatibleLayoutMapping::validated(&root, Some(".app/compat".into()), None, None)
                .expect_err("app-owned state cannot become a wiki root");
        assert_eq!(app_owned.code, "PROJECT_COMPAT_LAYOUT_INVALID");
        let missing = CompatibleLayoutMapping::validated(
            &root,
            Some("pages".into()),
            Some("missing".into()),
            None,
        )
        .expect_err("missing functional roots cannot be mapped");
        assert_eq!(missing.code, "PROJECT_COMPAT_LAYOUT_INVALID");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compatible_mapping_rejects_duplicate_role_roots() {
        let root = temp_root("compatible-mapping-duplicate-roots");
        fs::create_dir_all(root.join("知识库")).unwrap();
        let error = CompatibleLayoutMapping::validated(
            &root,
            Some("知识库".into()),
            Some("知识库".into()),
            None,
        )
        .expect_err("one functional directory cannot carry two write roles");
        assert_eq!(error.code, "PROJECT_COMPAT_LAYOUT_INVALID");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compatible_mapping_rejects_overlapping_role_roots() {
        let root = temp_root("compatible-mapping-overlapping-roots");
        fs::create_dir_all(root.join("vault/concepts")).unwrap();
        let error = CompatibleLayoutMapping::validated(
            &root,
            Some("vault".into()),
            Some("vault/concepts".into()),
            None,
        )
        .expect_err("source and wiki roots must not overlap");
        assert_eq!(error.code, "PROJECT_COMPAT_LAYOUT_INVALID");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_mapping_collision_key_normalizes_case_and_unicode() {
        let composed = "知识库/é".nfc().collect::<String>().to_lowercase();
        let decomposed = "知识库/e\u{301}".nfc().collect::<String>().to_lowercase();
        assert_eq!(composed, decomposed);
        assert_eq!("Wiki".to_lowercase(), "wiki");
    }

    #[test]
    fn bounded_layout_discovery_stops_before_reading_entries_when_cancelled() {
        let root = temp_root("cancelled-discovery");
        fs::create_dir_all(root.join("large-materials")).unwrap();
        let cancelled = AtomicBool::new(true);

        let error = resolve_layout_with_budget(
            &root,
            Some(&LayoutDiscoveryBudget {
                deadline: Instant::now() + std::time::Duration::from_secs(1),
                cancelled: &cancelled,
            }),
        )
        .expect_err("cancelled assessment must stop layout discovery");

        assert_eq!(error.code, "PROJECT_ASSESSMENT_CANCELLED");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compatible_obsidian_discovery_is_read_only_and_role_aware() {
        let root = temp_root("obsidian");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        write(&root, "index.md");
        write(&root, "sources/资料.md");
        write(&root, "笔记/概念.md");

        let resolution = resolve_layout(&root).unwrap();

        assert_eq!(resolution.confidence, ProjectLayoutConfidence::High);
        assert!(resolution.layout.app_state_root.is_none());
        assert!(resolution.layout.source_write_root.is_none());
        assert!(resolution.layout.wiki_write_root.is_none());
        assert!(resolution.layout.task_state_root.is_none());
        assert!(resolution.layout.markdown_roots.iter().any(|item| {
            item.path == "sources" && item.role == ProjectMarkdownRootRole::Source
        }));
        assert!(resolution
            .layout
            .markdown_roots
            .iter()
            .any(|item| { item.path == "笔记" && item.role == ProjectMarkdownRootRole::Mixed }));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compatible_scan_uses_forward_slashes_and_applies_root_excludes() {
        let root = temp_root("paths");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        write(&root, "index.md");
        write(&root, "笔记/子目录/页面.md");
        let layout = resolve_layout(&root).unwrap().layout;

        let files = layout
            .list_markdown_files(
                &root,
                &[
                    ProjectMarkdownRootRole::Wiki,
                    ProjectMarkdownRootRole::Mixed,
                ],
            )
            .unwrap();
        let canonical_root = root.canonicalize().unwrap();
        let relative = files
            .iter()
            .map(|path| {
                normalize_project_path(
                    &path
                        .strip_prefix(&canonical_root)
                        .unwrap()
                        .to_string_lossy(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(relative, vec!["index.md", "笔记/子目录/页面.md"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn app_owned_compatible_guidance_does_not_switch_to_native_scan_rules() {
        let root = temp_root("compatible-guidance-scan");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::create_dir_all(root.join(".app/compat")).unwrap();
        fs::write(root.join(".app/compat/purpose.md"), "# Purpose").unwrap();
        fs::write(root.join(".app/compat/schema.md"), "# Schema").unwrap();
        fs::write(root.join("UPPER.MD"), "# Upper").unwrap();

        let layout = resolve_layout(&root).unwrap().layout;
        assert_eq!(layout.app_state_root.as_deref(), Some(".app/compat"));
        assert_eq!(layout.task_state_root.as_deref(), Some(".app/compat/tasks"));
        assert_eq!(
            layout.workflow_state_root.as_deref(),
            Some(".app/compat/workflows")
        );
        let files = layout
            .list_markdown_files(
                &root,
                &[
                    ProjectMarkdownRootRole::Source,
                    ProjectMarkdownRootRole::Wiki,
                    ProjectMarkdownRootRole::Mixed,
                ],
            )
            .unwrap();

        assert!(files.iter().any(|path| path.ends_with("UPPER.MD")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_like_user_files_do_not_expose_native_write_or_state_paths() {
        let root = temp_root("native-like-compatible");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::create_dir_all(root.join("wiki")).unwrap();
        write(&root, "purpose.md");
        write(&root, "schema.md");
        write(&root, "wiki/page.md");

        let resolution = resolve_layout(&root).unwrap();

        assert!(resolution.layout.app_state_root.is_none());
        assert!(resolution.layout.wiki_write_root.is_none());
        assert!(resolution.layout.workflow_state_root.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn truncated_discovery_never_turns_the_root_descriptor_into_a_recursive_scan() {
        let root = temp_root("bounded-root");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        for index in 0..=MAX_TOP_LEVEL_ENTRIES {
            fs::write(root.join(format!("entry-{index:04}.txt")), "bounded").unwrap();
        }
        write(&root, "index.md");
        write(&root, "exports/private.md");

        let resolution = resolve_layout(&root).unwrap();
        let files = resolution
            .layout
            .list_markdown_files(&root, &[ProjectMarkdownRootRole::Mixed])
            .unwrap();
        let canonical_root = root.canonicalize().unwrap();
        let relative = files
            .iter()
            .map(|path| {
                normalize_project_path(
                    &path
                        .strip_prefix(&canonical_root)
                        .unwrap()
                        .to_string_lossy(),
                )
            })
            .collect::<Vec<_>>();

        assert!(resolution
            .warnings
            .iter()
            .any(|warning| warning.code == ProjectLayoutWarningCode::DiscoveryLimitReached));
        assert!(relative.iter().all(|path| path != "exports/private.md"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn linked_obsidian_marker_does_not_raise_layout_confidence() {
        let root = temp_root("linked-marker");
        let outside = temp_root("linked-marker-outside");
        write(&root, "index.md");
        if create_directory_link(&outside, &root.join(".obsidian")).is_err() {
            fs::remove_dir_all(root).ok();
            fs::remove_dir_all(outside).ok();
            return;
        }

        let resolution = resolve_layout(&root).unwrap();

        assert_eq!(resolution.confidence, ProjectLayoutConfidence::Medium);
        assert!(resolution
            .warnings
            .iter()
            .any(|warning| warning.code == ProjectLayoutWarningCode::UnsafeEntrySkipped));
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn linked_app_marker_cannot_enable_native_or_compatible_write_paths() {
        let root = temp_root("linked-app-marker");
        let outside = temp_root("linked-app-marker-outside");
        for directory in ["raw", "wiki", "exports", "skills"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        write(&root, "purpose.md");
        write(&root, "schema.md");
        fs::create_dir_all(outside.join("compat")).unwrap();
        fs::write(outside.join("compat/purpose.md"), "external").unwrap();
        fs::write(outside.join("compat/schema.md"), "external").unwrap();
        if create_directory_link(&outside, &root.join(".app")).is_err() {
            fs::remove_dir_all(root).ok();
            fs::remove_dir_all(outside).ok();
            return;
        }

        let resolution = resolve_layout(&root).unwrap();

        assert!(resolution.layout.app_state_root.is_none());
        assert!(resolution.layout.wiki_write_root.is_none());
        assert!(resolution.layout.workflow_state_root.is_none());
        assert_eq!(
            resolution
                .layout
                .purpose_context
                .and_then(|value| value.read_path),
            Some("purpose.md".into())
        );
        fs::remove_dir(root.join(".app")).ok();
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn internal_markdown_links_are_read_once_while_external_and_sensitive_targets_stay_hidden() {
        let root = temp_root("internal-markdown-links");
        let outside = temp_root("internal-markdown-links-outside");
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::create_dir_all(root.join("shared")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::create_dir_all(root.join("raw").join("extracted")).unwrap();
        write(&root, "wiki/visible.md");
        write(&root, "shared/internal.md");
        write(&root, ".app/hidden.md");
        write(&root, ".obsidian/plugin.md");
        write(&root, "raw/extracted/source.md");
        write(&outside, "external.md");
        create_directory_link(&root.join("shared"), &root.join("wiki").join("internal")).unwrap();
        create_directory_link(&outside, &root.join("wiki").join("external")).unwrap();
        create_directory_link(&root.join(".app"), &root.join("wiki").join("app-state")).unwrap();
        create_directory_link(&root.join(".obsidian"), &root.join("wiki").join("obsidian"))
            .unwrap();
        create_directory_link(
            &root.join("raw").join("extracted"),
            &root.join("wiki").join("raw"),
        )
        .unwrap();
        create_directory_link(&root, &root.join("wiki").join("root")).unwrap();
        create_directory_link(&root.join("wiki"), &root.join("wiki").join("loop")).unwrap();

        let canonical_root = root.canonicalize().unwrap();
        let files = ProjectLayout::native()
            .list_markdown_files(&root, &[ProjectMarkdownRootRole::Wiki])
            .unwrap();
        let relative = files
            .iter()
            .map(|path| {
                normalize_project_path(
                    &path
                        .strip_prefix(&canonical_root)
                        .unwrap()
                        .to_string_lossy(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(relative, vec!["shared/internal.md", "wiki/visible.md"]);

        let source_files = ProjectLayout::native()
            .list_markdown_files(&root, &[ProjectMarkdownRootRole::Source])
            .unwrap();
        let source_relative = source_files
            .iter()
            .map(|path| {
                normalize_project_path(
                    &path
                        .strip_prefix(&canonical_root)
                        .unwrap()
                        .to_string_lossy(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(source_relative, vec!["raw/extracted/source.md"]);

        fs::remove_dir(root.join("wiki").join("internal")).ok();
        fs::remove_dir(root.join("wiki").join("external")).ok();
        fs::remove_dir(root.join("wiki").join("app-state")).ok();
        fs::remove_dir(root.join("wiki").join("obsidian")).ok();
        fs::remove_dir(root.join("wiki").join("raw")).ok();
        fs::remove_dir(root.join("wiki").join("root")).ok();
        fs::remove_dir(root.join("wiki").join("loop")).ok();
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn linked_markdown_root_cannot_bypass_sensitive_root_filtering() {
        let root = temp_root("linked-markdown-root");
        fs::create_dir_all(root.join(".app")).unwrap();
        write(&root, ".app/hidden.md");
        create_directory_link(&root.join(".app"), &root.join("wiki")).unwrap();

        let files = ProjectLayout::native()
            .list_markdown_files(&root, &[ProjectMarkdownRootRole::Wiki])
            .unwrap();
        assert!(
            files.is_empty(),
            "linked wiki root must not expose .app Markdown"
        );

        fs::remove_dir(root.join("wiki")).ok();
        fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    fn create_directory_link(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_directory_link(target: &Path, link: &Path) -> std::io::Result<()> {
        let output = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "mklink /J failed for `{}` -> `{}`: {} {}",
                link.display(),
                target.display(),
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim(),
            )))
        }
    }
}
