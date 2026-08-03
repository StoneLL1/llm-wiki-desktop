use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use unicode_normalization::UnicodeNormalization;

use crate::app_state::ProjectRegistry;
use crate::errors::BackendError;
use crate::models::git::GitRepositoryStatus;
use crate::models::graph::GraphData;
use crate::models::layout::{
    canonical_internal_read_path, is_link_or_reparse, project_descendant_path_enters_link,
    resolve_layout_with_budget, LayoutDiscoveryBudget, ProjectLayoutConfidence,
};
use crate::models::paths::ProjectContext;
use crate::models::project::{
    AssessmentId, AssessmentOperationId, AssessmentOperationStatus, ProjectAssessmentOperation,
    ProjectAssessmentWarning, ProjectCapability, ProjectFilesystemAccess, ProjectFormat,
    ProjectHealth, ProjectMarker, ProjectOpenAssessment, ProjectOpenIntent, ProjectTrustKind,
    ProjectTrustState, StartProjectOpenAssessmentResult,
};
use crate::services::{project_identity, GitService, ProjectService};
use crate::utils::path_safety::{
    validate_existing_project_directory, validate_existing_project_file,
    validate_existing_project_root,
};

use super::decision_store::ProjectOpenDecisionStore;

const ASSESSMENT_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_ASSESSMENTS: usize = 64;
const MAX_OPERATIONS: usize = 64;
const MAX_ASSESSMENT_MARKDOWN_ENTRIES: usize = 4_096;
const MAX_ASSESSMENT_MARKDOWN_DEPTH: usize = 16;
const MAX_ASSESSMENT_PATH_NAME_ENTRIES: usize = 4_096;
const MAX_APP_STATE_JSON_BYTES: u64 = 1024 * 1024;
const ASSESSMENT_SCAN_BUDGET: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct ProjectAssessmentService {
    inner: Arc<AssessmentRegistry>,
    config_dir: PathBuf,
    decision_store: Arc<ProjectOpenDecisionStore>,
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
            decision_store: Arc::new(ProjectOpenDecisionStore::new(&config_dir)),
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

    /// Performs the same bounded, read-only inspection used by the background
    /// project-open flow when a backend command needs a current authority
    /// snapshot. This does not register an assessment token and must not be
    /// used for a user-facing long-running scan.
    pub fn inspect_current(&self, path: &str) -> Result<ProjectOpenAssessment, BackendError> {
        let cancelled = AtomicBool::new(false);
        let mut assessment = assess_project_folder(path, &self.config_dir, &cancelled)?;
        self.attach_remembered_intent(&mut assessment);
        Ok(assessment)
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
        self.attach_remembered_intent(&mut current);
        Ok(current)
    }

    pub fn remember_ambiguous_intent(
        &self,
        assessment_id: &AssessmentId,
        intent: ProjectOpenIntent,
    ) -> Result<ProjectOpenAssessment, BackendError> {
        let mut assessment = self.resolve_current(assessment_id)?;
        if assessment.format != ProjectFormat::AmbiguousMarkdown {
            return Err(BackendError::new(
                "PROJECT_OPEN_INTENT_UNAVAILABLE",
                "Only an ambiguous Markdown assessment can receive a remembered open decision.",
                true,
                true,
            ));
        }
        self.decision_store.remember(
            &assessment.canonical_identity_key,
            &assessment.identity_revision,
            intent,
        )?;
        assessment.remembered_open_intent = Some(intent);
        self.update_cached_assessment(assessment_id, &assessment)?;
        Ok(assessment)
    }

    /// Clears an explicit ambiguous-folder decision from global app settings.
    /// The selected Markdown folder remains strictly read-only throughout this
    /// operation, so the next assessment will ask the user again.
    pub fn clear_ambiguous_intent(
        &self,
        assessment_id: &AssessmentId,
    ) -> Result<ProjectOpenAssessment, BackendError> {
        let mut assessment = self.resolve_current(assessment_id)?;
        if assessment.format != ProjectFormat::AmbiguousMarkdown {
            return Err(BackendError::new(
                "PROJECT_OPEN_INTENT_UNAVAILABLE",
                "Only an ambiguous Markdown assessment can clear a remembered open decision.",
                true,
                true,
            ));
        }
        self.decision_store.forget(
            &assessment.canonical_identity_key,
            &assessment.identity_revision,
        )?;
        assessment.remembered_open_intent = None;
        self.update_cached_assessment(assessment_id, &assessment)?;
        Ok(assessment)
    }

    fn update_cached_assessment(
        &self,
        assessment_id: &AssessmentId,
        assessment: &ProjectOpenAssessment,
    ) -> Result<(), BackendError> {
        let mut assessments = self
            .inner
            .assessments
            .lock()
            .map_err(|_| registry_locked())?;
        let stored = assessments
            .get_mut(assessment_id)
            .ok_or_else(|| unknown_assessment(assessment_id))?;
        stored.assessment = assessment.clone();
        Ok(())
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
            Ok(mut assessment) => {
                self.attach_remembered_intent(&mut assessment);
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

    fn attach_remembered_intent(&self, assessment: &mut ProjectOpenAssessment) {
        assessment.remembered_open_intent = if assessment.format == ProjectFormat::AmbiguousMarkdown
        {
            self.decision_store.lookup(
                &assessment.canonical_identity_key,
                &assessment.identity_revision,
            )
        } else {
            None
        };
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
    let assessment_deadline = Instant::now() + ASSESSMENT_SCAN_BUDGET;
    let resolution = resolve_layout_with_budget(
        &canonical_root,
        Some(&LayoutDiscoveryBudget {
            deadline: assessment_deadline,
            cancelled,
        }),
    )?;
    check_cancelled(cancelled)?;

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
    let collision_scan =
        bounded_path_name_collisions(&canonical_root, cancelled, assessment_deadline)?;
    check_cancelled(cancelled)?;
    let app_state_corrupt = app_state_is_corrupt(&canonical_root, cancelled)?;
    let health = health_with_path_collisions(
        derive_health(format, markdown_scan.readable, app_state_corrupt),
        &collision_scan.warnings,
    );
    let repair_available =
        health == ProjectHealth::Recovery && graph_cache_needs_repair(&canonical_root);
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
    warnings.extend(collision_scan.warnings);
    if collision_scan.limited {
        warnings.push(ProjectAssessmentWarning {
            code: "PROJECT_ASSESSMENT_COLLISION_SCAN_LIMIT".into(),
            message: "Portable filename collision inspection reached its bounded scan limit."
                .into(),
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
        remembered_open_intent: None,
        trust,
        filesystem_access,
        health,
        repair_available,
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
        let is_corrupt = bytes.len() as u64 > MAX_APP_STATE_JSON_BYTES
            || if path
                .file_name()
                .is_some_and(|name| name == "graph-cache.json")
            {
                serde_json::from_slice::<GraphData>(&bytes).is_err()
            } else {
                serde_json::from_slice::<serde_json::Value>(&bytes).is_err()
            };
        if is_corrupt {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Recovery is deliberately narrower than detection: a damaged app-state file
/// may make the project recoverable without being safe to rewrite. Today only
/// the fully derived graph cache is eligible for automatic repair.
fn graph_cache_needs_repair(root: &Path) -> bool {
    let target = root.join(".app").join("graph-cache.json");
    let Ok(safe_target) = validate_existing_project_file(root, &target) else {
        return false;
    };
    let Ok(metadata) = fs::metadata(&safe_target) else {
        return false;
    };
    if metadata.len() > MAX_APP_STATE_JSON_BYTES {
        return false;
    }
    fs::read(safe_target).is_ok_and(|bytes| serde_json::from_slice::<GraphData>(&bytes).is_err())
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
    let canonical_root = root.canonicalize().map_err(|error| {
        BackendError::new(
            "PROJECT_ASSESSMENT_READ_FAILED",
            "Project Markdown roots could not be inspected.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "error": error.to_string() }))
    })?;
    let mut queue = VecDeque::new();
    let mut seen_directories = HashSet::new();
    for markdown_root in &layout.markdown_roots {
        // Preserve the configured (logical) path long enough to detect a
        // linked ancestor. Building from `canonical_root` would hide a
        // `raw` junction to `.app` before the sensitive-root guard runs.
        let configured_scan_root = if markdown_root.path == "." {
            root.to_path_buf()
        } else {
            root.join(&markdown_root.path)
        };
        let entered_via_link = project_descendant_path_enters_link(root, &configured_scan_root)?;
        if let Some(scan_root) =
            canonical_internal_read_path(&canonical_root, &configured_scan_root, entered_via_link)?
        {
            queue.push_back((
                scan_root,
                entered_via_link,
                0_usize,
                markdown_root.path != ".",
                markdown_root.exclude.clone().unwrap_or_default(),
            ));
        }
    }

    let mut inspected = 0_usize;
    while let Some((directory, entered_via_link, depth, recursive, excludes)) = queue.pop_front() {
        check_cancelled(cancelled)?;
        if inspected >= MAX_ASSESSMENT_MARKDOWN_ENTRIES || Instant::now() >= deadline {
            return Ok(MarkdownReadability {
                readable: false,
                limited: true,
            });
        }
        if !seen_directories.insert(directory.clone()) {
            continue;
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
            let Ok(link_metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            let entered_via_link = entered_via_link || is_link_or_reparse(&link_metadata);
            let Some(path) =
                canonical_internal_read_path(&canonical_root, &path, entered_via_link)?
            else {
                continue;
            };
            let relative = path
                .strip_prefix(&canonical_root)
                .ok()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if excludes.iter().any(|excluded| {
                relative == *excluded || relative.starts_with(&format!("{excluded}/"))
            }) {
                continue;
            }
            let Ok(metadata) = fs::metadata(&path) else {
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
                queue.push_back((path, entered_via_link, depth + 1, true, excludes.clone()));
            } else if metadata.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
                && fs::File::open(&path).is_ok()
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

struct PathCollisionScan {
    warnings: Vec<ProjectAssessmentWarning>,
    limited: bool,
}

fn bounded_path_name_collisions(
    root: &Path,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PathCollisionScan, BackendError> {
    let mut queue = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut seen = HashMap::<String, String>::new();
    let mut warnings = Vec::new();
    let mut visited = 0usize;

    while let Some((directory, depth)) = queue.pop_front() {
        check_cancelled(cancelled)?;
        if Instant::now() >= deadline || visited >= MAX_ASSESSMENT_PATH_NAME_ENTRIES {
            return Ok(PathCollisionScan {
                warnings,
                limited: true,
            });
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        let parent = directory
            .strip_prefix(root)
            .ok()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| ".".into());
        for entry in entries {
            check_cancelled(cancelled)?;
            if Instant::now() >= deadline || visited >= MAX_ASSESSMENT_PATH_NAME_ENTRIES {
                return Ok(PathCollisionScan {
                    warnings,
                    limited: true,
                });
            }
            let Ok(entry) = entry else {
                continue;
            };
            visited += 1;
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let display_path = if parent == "." {
                name.clone()
            } else {
                format!("{parent}/{name}")
            };
            record_path_name_collision(&mut seen, &mut warnings, &parent, &name, &display_path);

            if metadata.is_dir()
                && depth < MAX_ASSESSMENT_MARKDOWN_DEPTH
                && !ignored_assessment_directory(&name)
                && validate_existing_project_directory(root, &path).is_ok()
            {
                queue.push_back((path, depth + 1));
            }
        }
    }

    Ok(PathCollisionScan {
        warnings,
        limited: false,
    })
}

fn record_path_name_collision(
    seen: &mut HashMap<String, String>,
    warnings: &mut Vec<ProjectAssessmentWarning>,
    parent: &str,
    name: &str,
    display_path: &str,
) {
    let key = format!("{parent}\0{}", portable_path_name_key(name));
    if let Some(previous) = seen.get(&key) {
        if previous != display_path {
            warnings.push(ProjectAssessmentWarning {
                code: "PROJECT_PATH_NAME_COLLISION".into(),
                message: "Two paths collide under portable case or Unicode filename rules. Rename one before enabling writes."
                    .into(),
                path: Some(format!("{previous} | {display_path}")),
            });
        }
        return;
    }
    seen.insert(key, display_path.to_string());
}

fn portable_path_name_key(name: &str) -> String {
    name.nfc()
        .collect::<String>()
        .trim_end_matches(['.', ' '])
        .to_lowercase()
}

fn ignored_assessment_directory(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with('.') || matches!(lower.as_str(), "node_modules" | "target")
}

fn health_with_path_collisions(
    health: ProjectHealth,
    collisions: &[ProjectAssessmentWarning],
) -> ProjectHealth {
    if health == ProjectHealth::Healthy && !collisions.is_empty() {
        // A portable-name collision is readable but unsafe to write: Windows
        // and macOS may fold names that Linux keeps distinct. Keep content
        // available while routing mutations through the repair path.
        ProjectHealth::Repairable
    } else {
        health
    }
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
            ProjectFilesystemAccess::Writable
        } else {
            ProjectFilesystemAccess::ReadOnly
        }
    }
    #[cfg(not(unix))]
    {
        // Windows does not expose POSIX write bits. The read-only attribute is
        // the non-mutating signal available during a quick assessment; actual
        // mutations still revalidate access in the backend service.
        ProjectFilesystemAccess::Writable
    }
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

pub(crate) fn derive_capabilities(
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
    if trust == ProjectTrustState::Trusted && markdown_readable && health == ProjectHealth::Healthy
    {
        capabilities.push(ProjectCapability::ExternalAi);
    }
    if trust == ProjectTrustState::Trusted
        && access == ProjectFilesystemAccess::Writable
        && health == ProjectHealth::Healthy
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
    use crate::models::layout::resolve_layout;

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
    fn detects_graph_cache_json_that_does_not_match_the_cache_schema() {
        let root = temp_root("graph-cache-schema");
        fs::create_dir_all(root.join(".app")).unwrap();
        // This was the old new-project placeholder. It is JSON, but cannot be
        // read by GraphService as GraphData and therefore needs recovery.
        fs::write(
            root.join(".app/graph-cache.json"),
            r#"{"nodes": [], "edges": []}"#,
        )
        .unwrap();

        assert!(app_state_is_corrupt(&root, &AtomicBool::new(false)).unwrap());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn accepts_the_current_empty_graph_cache_schema() {
        let root = temp_root("graph-cache-current");
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(
            root.join(".app/graph-cache.json"),
            serde_json::to_vec(&GraphData::empty(String::new())).unwrap(),
        )
        .unwrap();

        assert!(!app_state_is_corrupt(&root, &AtomicBool::new(false)).unwrap());
        fs::remove_dir_all(root).ok();
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

    #[cfg(windows)]
    #[test]
    fn readability_accepts_markdown_through_a_contained_windows_junction() {
        let root = temp_root("contained-junction-readability");
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::create_dir_all(root.join("shared")).unwrap();
        fs::write(root.join("shared/linked.md"), "# Linked").unwrap();
        let junction = root.join("wiki").join("linked");
        let output = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&junction)
            .arg(root.join("shared"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction setup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let readability = bounded_markdown_readability(
            &root,
            &crate::models::layout::ProjectLayout::native(),
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert!(readability.readable);
        assert!(!readability.limited);

        fs::remove_dir(junction).ok();
        fs::remove_dir_all(root).ok();
    }

    #[cfg(windows)]
    #[test]
    fn readability_rejects_a_sensitive_target_reached_through_a_linked_ancestor() {
        let root = temp_root("sensitive-ancestor-junction-readability");
        for directory in [".app/extracted", "wiki", "exports", "skills"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(root.join("purpose.md"), "purpose").unwrap();
        fs::write(root.join("schema.md"), "schema").unwrap();
        fs::write(
            root.join(".app/extracted/readable.md"),
            "# Must stay hidden",
        )
        .unwrap();
        let junction = root.join("raw");
        let output = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&junction)
            .arg(root.join(".app"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "junction setup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let readability = bounded_markdown_readability(
            &root,
            &crate::models::layout::ProjectLayout::native(),
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(1),
        )
        .unwrap();
        assert!(!readability.readable);
        assert!(!readability.limited);

        fs::remove_dir(junction).ok();
        fs::remove_dir_all(root).ok();
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
        assert!(!trusted_read_only.contains(&ProjectCapability::ExternalAi));
        assert!(!trusted_read_only.contains(&ProjectCapability::GitCheckpoint));
        assert!(!trusted_read_only.contains(&ProjectCapability::ProjectWrite));
        assert_eq!(format, ProjectFormat::MarkdownVault);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn portable_name_collisions_block_writes_without_hiding_readable_content() {
        let mut seen = HashMap::new();
        let mut warnings = Vec::new();
        record_path_name_collision(
            &mut seen,
            &mut warnings,
            "notes",
            "Guide.md",
            "notes/Guide.md",
        );
        record_path_name_collision(
            &mut seen,
            &mut warnings,
            "notes",
            "guide.md",
            "notes/guide.md",
        );

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "PROJECT_PATH_NAME_COLLISION");
        assert_eq!(
            portable_path_name_key("é.md"),
            portable_path_name_key("e\u{301}.md")
        );
        assert_eq!(
            health_with_path_collisions(ProjectHealth::Healthy, &warnings),
            ProjectHealth::Repairable
        );
        assert_eq!(
            health_with_path_collisions(ProjectHealth::Unreadable, &warnings),
            ProjectHealth::Unreadable
        );
    }

    #[cfg(not(unix))]
    #[test]
    fn writable_windows_directory_is_not_misclassified_from_missing_posix_bits() {
        let root = temp_root("windows-metadata-access");
        assert_eq!(
            metadata_filesystem_access(&root),
            ProjectFilesystemAccess::Writable
        );
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
    fn remembers_an_ambiguous_folder_choice_by_identity_without_writing_the_folder() {
        let config = temp_root("remember-intent-config");
        let service = ProjectAssessmentService::new(config.clone());
        let root = temp_root("remember-intent-root");
        fs::write(root.join("note.md"), "# Note").unwrap();
        let assessment = assess_project_folder(
            root.to_string_lossy().as_ref(),
            &config,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(assessment.format, ProjectFormat::AmbiguousMarkdown);
        let id = assessment.assessment_id.clone();
        service.inner.assessments.lock().unwrap().insert(
            id.clone(),
            AssessmentEntry {
                created_at: Instant::now(),
                expires_at: Instant::now() + ASSESSMENT_TTL,
                assessment,
            },
        );

        let remembered = service
            .remember_ambiguous_intent(&id, ProjectOpenIntent::CreateFromMaterials)
            .unwrap();
        assert_eq!(
            remembered.remembered_open_intent,
            Some(ProjectOpenIntent::CreateFromMaterials)
        );
        assert!(!root.join(".app").exists());
        assert!(!root.join(".git").exists());

        let fresh_service = ProjectAssessmentService::new(config.clone());
        let refreshed = fresh_service
            .inspect_current(root.to_string_lossy().as_ref())
            .unwrap();
        assert_eq!(
            refreshed.remembered_open_intent,
            Some(ProjectOpenIntent::CreateFromMaterials)
        );

        let cleared = service.clear_ambiguous_intent(&id).unwrap();
        assert_eq!(cleared.remembered_open_intent, None);
        assert!(!root.join(".app").exists());
        assert!(!root.join(".git").exists());

        let cleared_freshly = ProjectAssessmentService::new(config.clone())
            .inspect_current(root.to_string_lossy().as_ref())
            .unwrap();
        assert_eq!(cleared_freshly.remembered_open_intent, None);
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(config).ok();
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
