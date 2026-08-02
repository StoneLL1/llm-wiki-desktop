use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::app_state::ProjectRegistry;
use crate::errors::BackendError;
use crate::models::git::GitRepositoryStatus;
use crate::models::layout::{resolve_layout, ProjectLayoutConfidence};
use crate::models::paths::ProjectContext;
use crate::models::project::{
    AssessmentId, AssessmentOperationId, AssessmentOperationStatus, ProjectAssessmentOperation,
    ProjectAssessmentWarning, ProjectCapability, ProjectFilesystemAccess, ProjectFormat,
    ProjectHealth, ProjectMarker, ProjectOpenAssessment, ProjectTrustKind, ProjectTrustState,
    StartProjectOpenAssessmentResult,
};
use crate::services::{project_identity, GitService, ProjectService};
use crate::utils::path_safety::{
    validate_existing_project_directory, validate_existing_project_file,
    validate_existing_project_root,
};

const ASSESSMENT_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_ASSESSMENTS: usize = 64;
const MAX_OPERATIONS: usize = 64;
const MAX_ASSESSMENT_MARKDOWN_ENTRIES: usize = 4_096;
const MAX_ASSESSMENT_MARKDOWN_DEPTH: usize = 16;
const MAX_APP_STATE_JSON_BYTES: u64 = 1024 * 1024;
const ASSESSMENT_SCAN_BUDGET: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct ProjectAssessmentService {
    inner: Arc<AssessmentRegistry>,
    config_dir: PathBuf,
}

struct AssessmentRegistry {
    operations: Mutex<HashMap<AssessmentOperationId, OperationEntry>>,
    assessments: Mutex<HashMap<AssessmentId, AssessmentEntry>>,
}

struct OperationEntry {
    created_at: Instant,
    cancelled: Arc<AtomicBool>,
    state: OperationState,
}

enum OperationState {
    Running,
    Completed(ProjectOpenAssessment),
    Failed(BackendError),
}

#[derive(Clone)]
struct AssessmentEntry {
    created_at: Instant,
    expires_at: Instant,
    assessment: ProjectOpenAssessment,
}

impl Default for ProjectAssessmentService {
    fn default() -> Self {
        Self::new(super::default_config_dir())
    }
}

impl ProjectAssessmentService {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(AssessmentRegistry {
                operations: Mutex::new(HashMap::new()),
                assessments: Mutex::new(HashMap::new()),
            }),
            config_dir,
        }
    }

    pub fn start(&self, path: String) -> Result<StartProjectOpenAssessmentResult, BackendError> {
        let operation_id = AssessmentOperationId(uuid::Uuid::new_v4().to_string());
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut operations = self
                .inner
                .operations
                .lock()
                .map_err(|_| registry_locked())?;
            prune_operations(&mut operations);
            operations.insert(
                operation_id.clone(),
                OperationEntry {
                    created_at: Instant::now(),
                    cancelled: Arc::clone(&cancelled),
                    state: OperationState::Running,
                },
            );
        }

        let service = self.clone();
        let worker_operation_id = operation_id.clone();
        std::thread::spawn(move || {
            let outcome = assess_project_folder(&path, &service.config_dir, &cancelled);
            service.finish_operation(&worker_operation_id, outcome);
        });

        Ok(StartProjectOpenAssessmentResult {
            assessment_operation_id: operation_id,
        })
    }

    pub fn get_operation(
        &self,
        operation_id: &AssessmentOperationId,
    ) -> Result<ProjectAssessmentOperation, BackendError> {
        let operations = self
            .inner
            .operations
            .lock()
            .map_err(|_| registry_locked())?;
        let entry = operations
            .get(operation_id)
            .ok_or_else(|| unknown_operation(operation_id))?;
        let (status, assessment, error) = match &entry.state {
            OperationState::Running => (AssessmentOperationStatus::Running, None, None),
            OperationState::Completed(assessment) => (
                AssessmentOperationStatus::Completed,
                Some(assessment.clone()),
                None,
            ),
            OperationState::Failed(error) => {
                (AssessmentOperationStatus::Failed, None, Some(error.clone()))
            }
        };
        Ok(ProjectAssessmentOperation {
            assessment_operation_id: operation_id.clone(),
            status,
            assessment,
            error,
        })
    }

    pub fn cancel(&self, operation_id: &AssessmentOperationId) -> Result<(), BackendError> {
        let mut operations = self
            .inner
            .operations
            .lock()
            .map_err(|_| registry_locked())?;
        let entry = operations
            .remove(operation_id)
            .ok_or_else(|| unknown_operation(operation_id))?;
        entry.cancelled.store(true, Ordering::SeqCst);
        if let OperationState::Completed(assessment) = entry.state {
            let mut assessments = self
                .inner
                .assessments
                .lock()
                .map_err(|_| registry_locked())?;
            assessments.remove(&assessment.assessment_id);
        }
        Ok(())
    }

    pub fn resolve(
        &self,
        assessment_id: &AssessmentId,
    ) -> Result<ProjectOpenAssessment, BackendError> {
        let mut assessments = self
            .inner
            .assessments
            .lock()
            .map_err(|_| registry_locked())?;
        prune_assessments(&mut assessments);
        assessments
            .get(assessment_id)
            .map(|entry| entry.assessment.clone())
            .ok_or_else(|| unknown_assessment(assessment_id))
    }

    pub fn resolve_current(
        &self,
        assessment_id: &AssessmentId,
    ) -> Result<ProjectOpenAssessment, BackendError> {
        let previous = self.resolve(assessment_id)?;
        let cancelled = AtomicBool::new(false);
        let mut current =
            assess_project_folder(&previous.canonical_root_path, &self.config_dir, &cancelled)?;
        if current.canonical_identity_key != previous.canonical_identity_key
            || current.identity_revision != previous.identity_revision
        {
            return Err(BackendError::new(
                "PROJECT_ASSESSMENT_IDENTITY_CHANGED",
                "The folder changed after assessment. Assess it again before continuing.",
                true,
                true,
            ));
        }
        current.assessment_id = previous.assessment_id;
        Ok(current)
    }

    pub fn invalidate(&self, assessment_id: &AssessmentId) -> Result<(), BackendError> {
        self.inner
            .assessments
            .lock()
            .map_err(|_| registry_locked())?
            .remove(assessment_id);
        Ok(())
    }

    fn finish_operation(
        &self,
        operation_id: &AssessmentOperationId,
        outcome: Result<ProjectOpenAssessment, BackendError>,
    ) {
        let Ok(mut operations) = self.inner.operations.lock() else {
            return;
        };
        let Some(entry) = operations.get_mut(operation_id) else {
            return;
        };
        if entry.cancelled.load(Ordering::SeqCst) {
            operations.remove(operation_id);
            return;
        }
        match outcome {
            Ok(assessment) => {
                if let Ok(mut assessments) = self.inner.assessments.lock() {
                    prune_assessments(&mut assessments);
                    assessments.insert(
                        assessment.assessment_id.clone(),
                        AssessmentEntry {
                            created_at: Instant::now(),
                            expires_at: Instant::now() + ASSESSMENT_TTL,
                            assessment: assessment.clone(),
                        },
                    );
                }
                entry.state = OperationState::Completed(assessment);
            }
            Err(error) => entry.state = OperationState::Failed(error),
        }
    }
}

pub fn assess_project_folder(
    path: &str,
    config_dir: &Path,
    cancelled: &AtomicBool,
) -> Result<ProjectOpenAssessment, BackendError> {
    check_cancelled(cancelled)?;
    let raw_root = PathBuf::from(path);
    let canonical_root = validate_existing_project_root(&raw_root).map_err(path_safety_error)?;
    check_cancelled(cancelled)?;
    let identity = project_identity(&canonical_root).map_err(|message| {
        BackendError::new(
            "PROJECT_ASSESSMENT_IDENTITY_FAILED",
            "The selected folder identity could not be established.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "error": message }))
    })?;
    let resolution = resolve_layout(&canonical_root)?;
    check_cancelled(cancelled)?;
    let assessment_deadline = Instant::now() + ASSESSMENT_SCAN_BUDGET;

    let format = classify_format(&canonical_root, resolution.confidence, &resolution.layout);
    let project_service = ProjectService::with_config_dir(config_dir.to_path_buf());
    let restored_trust = project_service.restore_project_trust(&canonical_root)?;
    let trust = if format == ProjectFormat::NativeCurrent
        || restored_trust.is_some_and(|kind| kind == ProjectTrustKind::Compatible)
    {
        ProjectTrustState::Trusted
    } else {
        ProjectTrustState::Untrusted
    };
    let context =
        ProjectContext::new("assessment", canonical_root.clone()).with_resolved_layout()?;
    let filesystem_access = match trust {
        ProjectTrustState::Trusted => project_service.filesystem_access(&context, true),
        ProjectTrustState::Untrusted => metadata_filesystem_access(&canonical_root),
    };
    check_cancelled(cancelled)?;
    let (git, git_warning) = match GitService.repository_status_for_assessment(
        &context,
        assessment_deadline,
        cancelled,
    ) {
        Ok(status) => (status, None),
        Err(_) => (
            GitRepositoryStatus {
                is_repository: false,
                branch: None,
                head: None,
                has_changes: false,
            },
            Some(ProjectAssessmentWarning {
                code: "PROJECT_GIT_UNAVAILABLE".into(),
                message:
                    "Git state could not be inspected safely; Markdown access remains available."
                        .into(),
                path: Some(".git".into()),
            }),
        ),
    };
    check_cancelled(cancelled)?;
    let markdown_scan = bounded_markdown_readability(
        &canonical_root,
        &resolution.layout,
        cancelled,
        assessment_deadline,
    )?;
    check_cancelled(cancelled)?;
    let app_state_corrupt = app_state_is_corrupt(&canonical_root, cancelled)?;
    let health = derive_health(format, markdown_scan.readable, app_state_corrupt);
    let markers = collect_markers(&canonical_root);
    let mut warnings = assessment_warnings(format, health, resolution.confidence);
    if let Some(warning) = git_warning {
        warnings.push(warning);
    }
    if markdown_scan.limited {
        warnings.push(ProjectAssessmentWarning {
            code: "PROJECT_ASSESSMENT_SCAN_LIMIT".into(),
            message: "Markdown readability inspection reached its bounded scan limit.".into(),
            path: None,
        });
    }
    let capabilities = derive_capabilities(
        format,
        trust,
        filesystem_access,
        health,
        markdown_scan.readable,
        git.head.is_some(),
        &resolution.layout,
    );

    Ok(ProjectOpenAssessment {
        assessment_id: AssessmentId(uuid::Uuid::new_v4().to_string()),
        canonical_root_path: identity.canonical_root.to_string_lossy().replace('\\', "/"),
        canonical_identity_key: identity.canonical_identity_key,
        identity_revision: identity.identity_revision,
        format,
        trust,
        filesystem_access,
        health,
        layout: resolution.layout,
        confidence: resolution.confidence,
        markers,
        capabilities,
        warnings,
        layout_warnings: resolution.warnings,
        git,
    })
}

fn classify_format(
    root: &Path,
    confidence: ProjectLayoutConfidence,
    layout: &crate::models::layout::ProjectLayout,
) -> ProjectFormat {
    if ProjectRegistry::is_strict_native_layout(root) {
        return ProjectFormat::NativeCurrent;
    }
    if safe_directory(root, "raw")
        && safe_directory(root, "wiki")
        && (safe_file(root, "wiki/index.md") || safe_file(root, "wiki/overview.md"))
    {
        return ProjectFormat::NashsuLlmWiki;
    }
    if safe_file(root, ".app/compat/purpose.md") && safe_file(root, ".app/compat/schema.md") {
        if safe_directory(root, ".obsidian") {
            return ProjectFormat::ObsidianVault;
        }
        return if confidence == ProjectLayoutConfidence::Low {
            ProjectFormat::AmbiguousMarkdown
        } else {
            ProjectFormat::MarkdownVault
        };
    }
    if safe_directory(root, ".obsidian") {
        return ProjectFormat::ObsidianVault;
    }
    if (safe_file(root, "purpose.md") || safe_file(root, "schema.md"))
        && (safe_directory(root, "wiki")
            || safe_directory(root, "raw")
            || safe_directory(root, ".app"))
    {
        return ProjectFormat::NativeLegacy;
    }
    if !layout.markdown_roots.is_empty() {
        return if confidence == ProjectLayoutConfidence::Low {
            ProjectFormat::AmbiguousMarkdown
        } else {
            ProjectFormat::MarkdownVault
        };
    }
    if fs::read_dir(root).is_ok_and(|mut entries| entries.next().is_some()) {
        ProjectFormat::OrdinaryMaterials
    } else {
        ProjectFormat::Unknown
    }
}

fn derive_health(
    format: ProjectFormat,
    markdown_readable: bool,
    app_state_corrupt: bool,
) -> ProjectHealth {
    if matches!(
        format,
        ProjectFormat::NativeCurrent
            | ProjectFormat::NativeLegacy
            | ProjectFormat::NashsuLlmWiki
            | ProjectFormat::ObsidianVault
            | ProjectFormat::MarkdownVault
            | ProjectFormat::AmbiguousMarkdown
    ) && !markdown_readable
    {
        return ProjectHealth::Unreadable;
    }
    if app_state_corrupt && markdown_readable {
        return ProjectHealth::Recovery;
    }
    if format == ProjectFormat::NativeLegacy {
        return ProjectHealth::Repairable;
    }
    ProjectHealth::Healthy
}

fn app_state_is_corrupt(root: &Path, cancelled: &AtomicBool) -> Result<bool, BackendError> {
    let Ok(app) = validate_existing_project_directory(root, &root.join(".app")) else {
        return Ok(false);
    };
    let Ok(entries) = fs::read_dir(app) else {
        return Ok(false);
    };
    for entry in entries.take(256) {
        check_cancelled(cancelled)?;
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            continue;
        }
        let Ok(safe_path) = validate_existing_project_file(root, &path) else {
            continue;
        };
        let Ok(mut file) = fs::File::open(&safe_path) else {
            continue;
        };
        let mut bytes = Vec::new();
        if file
            .by_ref()
            .take(MAX_APP_STATE_JSON_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
        {
            continue;
        }
        if bytes.len() as u64 > MAX_APP_STATE_JSON_BYTES
            || serde_json::from_slice::<serde_json::Value>(&bytes).is_err()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

struct MarkdownReadability {
    readable: bool,
    limited: bool,
}

fn bounded_markdown_readability(
    root: &Path,
    layout: &crate::models::layout::ProjectLayout,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<MarkdownReadability, BackendError> {
    let mut queue = VecDeque::new();
    for markdown_root in &layout.markdown_roots {
        let scan_root = if markdown_root.path == "." {
            root.to_path_buf()
        } else {
            root.join(&markdown_root.path)
        };
        if scan_root == root || validate_existing_project_directory(root, &scan_root).is_ok() {
            queue.push_back((
                scan_root,
                0_usize,
                markdown_root.path != ".",
                markdown_root.exclude.clone().unwrap_or_default(),
            ));
        }
    }

    let mut inspected = 0_usize;
    while let Some((directory, depth, recursive, excludes)) = queue.pop_front() {
        check_cancelled(cancelled)?;
        if inspected >= MAX_ASSESSMENT_MARKDOWN_ENTRIES || Instant::now() >= deadline {
            return Ok(MarkdownReadability {
                readable: false,
                limited: true,
            });
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries {
            check_cancelled(cancelled)?;
            inspected += 1;
            if inspected > MAX_ASSESSMENT_MARKDOWN_ENTRIES || Instant::now() >= deadline {
                return Ok(MarkdownReadability {
                    readable: false,
                    limited: true,
                });
            }
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .ok()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if excludes.iter().any(|excluded| {
                relative == *excluded || relative.starts_with(&format!("{excluded}/"))
            }) {
                continue;
            }
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_dir() {
                if !recursive || depth >= MAX_ASSESSMENT_MARKDOWN_DEPTH {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target") {
                    continue;
                }
                if validate_existing_project_directory(root, &path).is_ok() {
                    queue.push_back((path, depth + 1, true, excludes.clone()));
                }
            } else if metadata.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
                && validate_existing_project_file(root, &path)
                    .is_ok_and(|safe_path| fs::File::open(safe_path).is_ok())
            {
                return Ok(MarkdownReadability {
                    readable: true,
                    limited: false,
                });
            }
        }
    }
    Ok(MarkdownReadability {
        readable: false,
        limited: false,
    })
}

fn metadata_filesystem_access(root: &Path) -> ProjectFilesystemAccess {
    let Ok(metadata) = fs::metadata(root) else {
        return ProjectFilesystemAccess::ReadOnly;
    };
    if metadata.permissions().readonly() {
        return ProjectFilesystemAccess::ReadOnly;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o222 != 0 {
            return ProjectFilesystemAccess::Writable;
        }
    }
    ProjectFilesystemAccess::ReadOnly
}

fn collect_markers(root: &Path) -> Vec<ProjectMarker> {
    let candidates = [
        ("purpose", "purpose.md", true),
        ("schema", "schema.md", true),
        ("app_state", ".app", false),
        ("obsidian", ".obsidian", false),
        ("raw", "raw", false),
        ("wiki", "wiki", false),
        ("index", "index.md", true),
    ];
    candidates
        .into_iter()
        .filter(|(_, path, is_file)| {
            if *is_file {
                safe_file(root, path)
            } else {
                safe_directory(root, path)
            }
        })
        .map(|(kind, path, _)| ProjectMarker {
            kind: kind.into(),
            path: path.into(),
        })
        .collect()
}

fn safe_file(root: &Path, relative: &str) -> bool {
    validate_existing_project_file(root, &root.join(relative)).is_ok()
}

fn safe_directory(root: &Path, relative: &str) -> bool {
    validate_existing_project_directory(root, &root.join(relative)).is_ok()
}

fn assessment_warnings(
    format: ProjectFormat,
    health: ProjectHealth,
    confidence: ProjectLayoutConfidence,
) -> Vec<ProjectAssessmentWarning> {
    let mut warnings = Vec::new();
    if confidence == ProjectLayoutConfidence::Low {
        warnings.push(ProjectAssessmentWarning {
            code: "PROJECT_LAYOUT_LOW_CONFIDENCE".into(),
            message: "The folder intent is ambiguous and requires an explicit user choice.".into(),
            path: None,
        });
    }
    if health == ProjectHealth::Recovery {
        warnings.push(ProjectAssessmentWarning {
            code: "PROJECT_APP_STATE_RECOVERY".into(),
            message: "Application state is damaged, but readable Markdown remains available."
                .into(),
            path: Some(".app".into()),
        });
    }
    if format == ProjectFormat::OrdinaryMaterials {
        warnings.push(ProjectAssessmentWarning {
            code: "PROJECT_ORDINARY_MATERIALS".into(),
            message: "This folder looks like source material rather than a knowledge base.".into(),
            path: None,
        });
    }
    warnings
}

fn derive_capabilities(
    format: ProjectFormat,
    trust: ProjectTrustState,
    access: ProjectFilesystemAccess,
    health: ProjectHealth,
    markdown_readable: bool,
    has_git_history: bool,
    layout: &crate::models::layout::ProjectLayout,
) -> Vec<ProjectCapability> {
    let mut capabilities = Vec::new();
    if markdown_readable && health != ProjectHealth::Unreadable {
        capabilities.extend([
            ProjectCapability::ReadMarkdown,
            ProjectCapability::LocalSearch,
            ProjectCapability::InMemoryGraph,
            ProjectCapability::LocalHealthCheck,
        ]);
    }
    if trust == ProjectTrustState::Trusted && markdown_readable {
        capabilities.push(ProjectCapability::ExternalAi);
    }
    if trust == ProjectTrustState::Trusted
        && access == ProjectFilesystemAccess::Writable
        && (layout.source_write_root.is_some()
            || layout.wiki_write_root.is_some()
            || layout.export_root.is_some())
    {
        capabilities.push(ProjectCapability::ProjectWrite);
    }
    if has_git_history
        && trust == ProjectTrustState::Trusted
        && access == ProjectFilesystemAccess::Writable
        && health != ProjectHealth::Unreadable
    {
        capabilities.push(ProjectCapability::GitCheckpoint);
    }
    if matches!(
        format,
        ProjectFormat::ObsidianVault
            | ProjectFormat::MarkdownVault
            | ProjectFormat::NashsuLlmWiki
            | ProjectFormat::NativeLegacy
    ) {
        capabilities.push(ProjectCapability::EnableCompatibleFeatures);
    }
    capabilities
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), BackendError> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(BackendError::new(
            "PROJECT_ASSESSMENT_CANCELLED",
            "Project assessment was cancelled.",
            true,
            false,
        ));
    }
    Ok(())
}

fn prune_assessments(entries: &mut HashMap<AssessmentId, AssessmentEntry>) {
    let now = Instant::now();
    entries.retain(|_, entry| entry.expires_at > now);
    while entries.len() >= MAX_ASSESSMENTS {
        let Some(oldest) = entries
            .iter()
            .min_by_key(|(_, entry)| entry.created_at)
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        entries.remove(&oldest);
    }
}

fn prune_operations(entries: &mut HashMap<AssessmentOperationId, OperationEntry>) {
    while entries.len() >= MAX_OPERATIONS {
        let Some(oldest) = entries
            .iter()
            .min_by_key(|(_, entry)| entry.created_at)
            .map(|(id, _)| id.clone())
        else {
            break;
        };
        if let Some(entry) = entries.remove(&oldest) {
            entry.cancelled.store(true, Ordering::SeqCst);
        }
    }
}

fn registry_locked() -> BackendError {
    BackendError::new(
        "PROJECT_ASSESSMENT_REGISTRY_LOCKED",
        "Project assessment state is temporarily unavailable.",
        true,
        false,
    )
}

fn path_safety_error(message: String) -> BackendError {
    BackendError::new(
        "PROJECT_ASSESSMENT_PATH_UNSAFE",
        "The selected folder could not be assessed safely.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "error": message }))
}

fn unknown_operation(id: &AssessmentOperationId) -> BackendError {
    BackendError::new(
        "PROJECT_ASSESSMENT_OPERATION_UNKNOWN",
        "The project assessment operation is unknown or no longer available.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "assessmentOperationId": id.0 }))
}

fn unknown_assessment(id: &AssessmentId) -> BackendError {
    BackendError::new(
        "PROJECT_ASSESSMENT_UNKNOWN",
        "The project assessment expired or is unknown. Assess the folder again.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "assessmentId": id.0 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("assessment-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn classifies_all_supported_folder_families_conservatively() {
        let current = temp_root("current");
        for path in ["raw/sources", "wiki", ".app/tasks", "exports", "skills"] {
            fs::create_dir_all(current.join(path)).unwrap();
        }
        fs::write(current.join("purpose.md"), "# Purpose").unwrap();
        fs::write(current.join("schema.md"), "# Schema").unwrap();
        let resolution = resolve_layout(&current).unwrap();
        assert_eq!(
            classify_format(&current, resolution.confidence, &resolution.layout),
            ProjectFormat::NativeCurrent
        );

        let obsidian = temp_root("obsidian");
        fs::create_dir_all(obsidian.join(".obsidian")).unwrap();
        fs::write(obsidian.join("主页.md"), "# 主页").unwrap();
        let resolution = resolve_layout(&obsidian).unwrap();
        assert_eq!(
            classify_format(&obsidian, resolution.confidence, &resolution.layout),
            ProjectFormat::ObsidianVault
        );

        let ambiguous = temp_root("ambiguous");
        fs::write(ambiguous.join("note.md"), "# Note").unwrap();
        let resolution = resolve_layout(&ambiguous).unwrap();
        assert_eq!(
            classify_format(&ambiguous, resolution.confidence, &resolution.layout),
            ProjectFormat::AmbiguousMarkdown
        );

        let materials = temp_root("materials");
        fs::write(materials.join("report.pdf"), b"pdf").unwrap();
        let resolution = resolve_layout(&materials).unwrap();
        assert_eq!(
            classify_format(&materials, resolution.confidence, &resolution.layout),
            ProjectFormat::OrdinaryMaterials
        );

        let empty = temp_root("empty");
        let resolution = resolve_layout(&empty).unwrap();
        assert_eq!(
            classify_format(&empty, resolution.confidence, &resolution.layout),
            ProjectFormat::Unknown
        );
    }

    #[test]
    fn corrupt_app_state_keeps_markdown_in_recovery() {
        let root = temp_root("recovery");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(root.join("文档.md"), "# 可读").unwrap();
        fs::write(root.join(".app/settings.json"), "{").unwrap();
        assert_eq!(
            derive_health(ProjectFormat::ObsidianVault, true, true),
            ProjectHealth::Recovery
        );
    }

    #[test]
    fn empty_obsidian_vault_is_not_reported_as_readable_markdown() {
        let root = temp_root("empty-obsidian");
        let config = temp_root("empty-obsidian-config");
        fs::create_dir(root.join(".obsidian")).unwrap();
        let assessment = assess_project_folder(
            root.to_string_lossy().as_ref(),
            &config,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(assessment.format, ProjectFormat::ObsidianVault);
        assert_eq!(assessment.health, ProjectHealth::Unreadable);
        assert!(!assessment
            .capabilities
            .contains(&ProjectCapability::ReadMarkdown));
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(config).ok();
    }

    #[test]
    fn assessment_dimensions_do_not_collapse_into_format() {
        let root = temp_root("dimensions");
        fs::write(root.join("note.md"), "# Note").unwrap();
        let resolution = resolve_layout(&root).unwrap();
        let format = ProjectFormat::MarkdownVault;
        let untrusted = derive_capabilities(
            format,
            ProjectTrustState::Untrusted,
            ProjectFilesystemAccess::Writable,
            ProjectHealth::Healthy,
            true,
            false,
            &resolution.layout,
        );
        let trusted_read_only = derive_capabilities(
            format,
            ProjectTrustState::Trusted,
            ProjectFilesystemAccess::ReadOnly,
            ProjectHealth::Recovery,
            true,
            true,
            &resolution.layout,
        );

        assert!(!untrusted.contains(&ProjectCapability::ExternalAi));
        assert!(trusted_read_only.contains(&ProjectCapability::ExternalAi));
        assert!(!trusted_read_only.contains(&ProjectCapability::GitCheckpoint));
        assert!(!trusted_read_only.contains(&ProjectCapability::ProjectWrite));
        assert_eq!(format, ProjectFormat::MarkdownVault);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn cancelling_discards_operation_and_completed_snapshot() {
        let config = temp_root("config");
        let service = ProjectAssessmentService::new(config);
        let root = temp_root("cancel");
        fs::write(root.join("note.md"), "# Note").unwrap();
        let started = service.start(root.to_string_lossy().into_owned()).unwrap();
        for _ in 0..100 {
            if service
                .get_operation(&started.assessment_operation_id)
                .is_ok_and(|value| value.status != AssessmentOperationStatus::Running)
            {
                break;
            }
            std::thread::yield_now();
        }
        service.cancel(&started.assessment_operation_id).unwrap();
        assert_eq!(
            service
                .get_operation(&started.assessment_operation_id)
                .unwrap_err()
                .code,
            "PROJECT_ASSESSMENT_OPERATION_UNKNOWN"
        );
        assert!(!root.join(".app").exists());
    }

    #[test]
    fn expired_assessment_is_rejected_without_revival() {
        let config = temp_root("expired-config");
        let service = ProjectAssessmentService::new(config);
        let root = temp_root("expired-root");
        fs::write(root.join("note.md"), "# Note").unwrap();
        let assessment = assess_project_folder(
            root.to_string_lossy().as_ref(),
            &service.config_dir,
            &AtomicBool::new(false),
        )
        .unwrap();
        let id = assessment.assessment_id.clone();
        service.inner.assessments.lock().unwrap().insert(
            id.clone(),
            AssessmentEntry {
                created_at: Instant::now() - ASSESSMENT_TTL,
                expires_at: Instant::now() - Duration::from_millis(1),
                assessment,
            },
        );

        assert_eq!(
            service.resolve_current(&id).unwrap_err().code,
            "PROJECT_ASSESSMENT_UNKNOWN"
        );
        assert!(!root.join(".app").exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compatible_guidance_preserves_obsidian_format_with_root_user_context_files() {
        let root = temp_root("compat-format");
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::create_dir_all(root.join(".app/compat")).unwrap();
        for path in [
            "purpose.md",
            "schema.md",
            ".app/compat/purpose.md",
            ".app/compat/schema.md",
            "note.md",
        ] {
            fs::write(root.join(path), "# User content").unwrap();
        }
        let resolution = resolve_layout(&root).unwrap();

        assert_eq!(
            classify_format(&root, resolution.confidence, &resolution.layout),
            ProjectFormat::ObsidianVault
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn resolve_current_rejects_a_replaced_folder_identity() {
        let config = temp_root("identity-config");
        let service = ProjectAssessmentService::new(config);
        let root = temp_root("identity-root");
        fs::write(root.join("note.md"), "# Note").unwrap();
        let assessment = assess_project_folder(
            root.to_string_lossy().as_ref(),
            &service.config_dir,
            &AtomicBool::new(false),
        )
        .unwrap();
        let id = assessment.assessment_id.clone();
        service.inner.assessments.lock().unwrap().insert(
            id.clone(),
            AssessmentEntry {
                created_at: Instant::now(),
                expires_at: Instant::now() + ASSESSMENT_TTL,
                assessment,
            },
        );

        let replaced = root.with_extension("replaced");
        fs::rename(&root, &replaced).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(root.join("note.md"), "# Replacement").unwrap();

        let error = service.resolve_current(&id).unwrap_err();
        assert!(matches!(
            error.code.as_str(),
            "PROJECT_ASSESSMENT_IDENTITY_CHANGED" | "PROJECT_ASSESSMENT_PATH_UNSAFE"
        ));
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(replaced).ok();
    }

    #[test]
    fn resolve_current_refreshes_folder_classification_without_reusing_stale_content() {
        let config = temp_root("refresh-config");
        let service = ProjectAssessmentService::new(config);
        let root = temp_root("refresh-root");
        fs::create_dir(root.join(".obsidian")).unwrap();
        fs::write(root.join("note.md"), "# Note").unwrap();
        let assessment = assess_project_folder(
            root.to_string_lossy().as_ref(),
            &service.config_dir,
            &AtomicBool::new(false),
        )
        .unwrap();
        let id = assessment.assessment_id.clone();
        service.inner.assessments.lock().unwrap().insert(
            id.clone(),
            AssessmentEntry {
                created_at: Instant::now(),
                expires_at: Instant::now() + ASSESSMENT_TTL,
                assessment,
            },
        );
        fs::remove_file(root.join("note.md")).unwrap();
        fs::remove_dir(root.join(".obsidian")).unwrap();
        fs::write(root.join("report.pdf"), "pdf").unwrap();

        let refreshed = service.resolve_current(&id).unwrap();
        assert_eq!(refreshed.assessment_id, id);
        assert_eq!(refreshed.format, ProjectFormat::OrdinaryMaterials);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn incomplete_git_metadata_does_not_block_readable_markdown() {
        let root = temp_root("incomplete-git");
        let config = temp_root("incomplete-git-config");
        fs::create_dir(root.join(".obsidian")).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join("note.md"), "# Note").unwrap();

        let assessment = assess_project_folder(
            root.to_string_lossy().as_ref(),
            &config,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(assessment.format, ProjectFormat::ObsidianVault);
        assert_eq!(assessment.health, ProjectHealth::Healthy);
        assert!(!assessment.git.is_repository);
        assert!(assessment
            .capabilities
            .contains(&ProjectCapability::ReadMarkdown));
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(config).ok();
    }

    #[cfg(unix)]
    #[test]
    fn linked_app_state_and_git_metadata_are_never_followed() {
        use std::os::unix::fs::symlink;

        let root = temp_root("linked-markers-root");
        let external_app = temp_root("linked-markers-app");
        let external_git = temp_root("linked-markers-git");
        let config = temp_root("linked-markers-config");
        fs::write(root.join("note.md"), "# Note").unwrap();
        fs::write(external_app.join("settings.json"), "{").unwrap();
        symlink(&external_app, root.join(".app")).unwrap();
        symlink(&external_git, root.join(".git")).unwrap();

        let assessment = assess_project_folder(
            root.to_string_lossy().as_ref(),
            &config,
            &AtomicBool::new(false),
        )
        .unwrap();

        assert_eq!(assessment.health, ProjectHealth::Healthy);
        assert!(!assessment
            .markers
            .iter()
            .any(|marker| marker.kind == "app_state"));
        assert!(assessment
            .warnings
            .iter()
            .any(|warning| warning.code == "PROJECT_GIT_UNAVAILABLE"));
        fs::remove_file(root.join(".app")).ok();
        fs::remove_file(root.join(".git")).ok();
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(external_app).ok();
        fs::remove_dir_all(external_git).ok();
        fs::remove_dir_all(config).ok();
    }
}
