use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::errors::BackendError;
use crate::utils::path_safety::{
    validate_existing_project_directory, validate_existing_project_file,
};
use crate::utils::path_utils::normalize_project_path;

const MAX_TOP_LEVEL_ENTRIES: usize = 512;
const MAX_SIGNAL_ENTRIES_PER_DIRECTORY: usize = 512;

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

/// A bounded quick assessment must be able to stop compatible-layout
/// discovery before exploring a large ordinary materials directory.
pub struct LayoutDiscoveryBudget<'a> {
    pub deadline: Instant,
    pub cancelled: &'a AtomicBool,
}

impl ProjectLayout {
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
        let legacy_native_scan = self.app_state_root.is_some() && self.evidence_root.is_some();
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
    if native_markers_present(root) {
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
    Ok(ProjectLayoutResolution {
        layout: ProjectLayout {
            app_state_root: compat_enabled.then(|| ".app".into()),
            evidence_root: None,
            markdown_roots,
            source_write_root: None,
            wiki_write_root: None,
            wiki_index_path: has_root_index.then(|| "index.md".into()),
            wiki_overview_path: None,
            activity_log_path: None,
            queries_write_root: None,
            export_root: None,
            skills_root: None,
            import_state_root: None,
            source_state_root: None,
            compile_state_root: None,
            chat_state_root: None,
            task_state_root: None,
            workflow_state_root: None,
            graph_cache_path: None,
            lint_report_root: None,
            lint_ignore_path: None,
            export_record_path: None,
            bookmarks_path: None,
            settings_path: None,
            agent_config_path: None,
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

fn native_markers_present(root: &Path) -> bool {
    safe_file_marker(root, "purpose.md")
        && safe_file_marker(root, "schema.md")
        && safe_directory_marker(root, ".app")
        && safe_directory_marker(root, "raw")
        && safe_directory_marker(root, "wiki")
        && safe_directory_marker(root, "exports")
        && safe_directory_marker(root, "skills")
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

fn safe_file_marker(root: &Path, relative: &str) -> bool {
    validate_existing_project_file(root, &root.join(relative)).is_ok()
}

fn some(value: &str) -> Option<String> {
    Some(value.into())
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
