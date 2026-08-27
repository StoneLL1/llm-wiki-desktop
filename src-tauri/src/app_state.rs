use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, Weak};

use crate::errors::{BackendError, PATH_INVALID, PROJECT_CONTEXT_MISMATCH};
use crate::models::confirmation::{ConfirmationRegistry, UpdateMutationLease};
use crate::models::layout::{
    inspect_native_layout, resolve_layout, NativeLayoutState, ProjectLayoutConfidence,
};
use crate::models::paths::ProjectContext;
use crate::models::project::{ProjectFilesystemAccess, ProjectTrustKind};
use crate::models::workflow::{
    WorkflowFilesystemAccess, WorkflowGitState, WorkflowKind, WorkflowPersistenceMode,
    WorkflowProjectTrust,
};
use crate::services::import_v2::capability_runtime::ImportCapabilityRuntime;
use crate::services::import_v2::connector_session::ConnectorSessionService;
use crate::services::import_v2::ImportV2Service;
use crate::services::{
    AgentService, BlockingWorkCoordinator, BookmarkService, ChatConvenienceService, ChatService,
    ExportService, FileStore, GitService, GraphService, LintService, LlmService,
    ProjectAssessmentService, ProjectService, SearchService, SecretService, SettingsService,
    WorkflowService,
};
use crate::tasks::TaskService;
use crate::utils::path_safety::validate_existing_project_directory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectWriteRootKind {
    Source,
    Wiki,
    Export,
    Query,
}

/// Backend-issued proof that the current project is trusted, healthy, and
/// writable for the lifetime of one mutation. The private transition-guard
/// reference makes this capability impossible to construct from IPC data or
/// retain after [`AppState::with_current_project_write_access`] returns.
pub struct ProjectWritePermit<'permit> {
    context: ProjectContext,
    workflow_access: crate::services::WorkflowAccessSnapshot,
    _update_mutation_lease: UpdateMutationLease,
    _transition_guard: &'permit MutexGuard<'permit, ()>,
}

/// Narrow capability for explicitly confirmed project-authority mutations
/// such as repair, compatibility enablement, and initial Git setup. These
/// operations may be the action that makes a project trusted/healthy, so they
/// cannot require a normal [`ProjectWritePermit`]; they do still retain the
/// authority transition lock and prove that the registered root is writable.
pub(crate) struct ProjectAuthorityMutationPermit<'permit> {
    context: ProjectContext,
    _update_mutation_lease: UpdateMutationLease,
    _transition_guard: &'permit MutexGuard<'permit, ()>,
}

/// Narrow capability for project-owned task/workflow state. Unlike a content
/// write permit it remains available to restricted or read-only projects, but
/// its access snapshot forces those runs to use memory-only persistence.
pub(crate) struct ProjectTaskMutationPermit<'permit> {
    context: ProjectContext,
    workflow_access: crate::services::WorkflowAccessSnapshot,
    _transition_guard: &'permit MutexGuard<'permit, ()>,
}

impl ProjectTaskMutationPermit<'_> {
    pub(crate) fn context(&self) -> &ProjectContext {
        &self.context
    }

    pub(crate) fn workflow_access(&self) -> crate::services::WorkflowAccessSnapshot {
        self.workflow_access.clone()
    }
}

impl ProjectWritePermit<'_> {
    pub(crate) fn context(&self) -> &ProjectContext {
        &self.context
    }

    pub(crate) fn authority_revision(&self) -> &str {
        &self.workflow_access.authority_revision
    }

    pub(crate) fn workflow_access(&self) -> crate::services::WorkflowAccessSnapshot {
        self.workflow_access.clone()
    }

    pub(crate) fn validates(&self, context: &ProjectContext) -> bool {
        self.context.project_id == context.project_id
            && self.context.root == context.root
            && !self.authority_revision().is_empty()
    }
}

impl ProjectAuthorityMutationPermit<'_> {
    pub(crate) fn context(&self) -> &ProjectContext {
        &self.context
    }
}

/// Epoch-bound guard covering one external publication and every result
/// commit derived from it. Trust revocation closes the epoch first and cannot
/// report success until all guards for the revoked root have been dropped.
pub struct ProjectExecutionLease {
    context: ProjectContext,
    authority_revision: String,
    execution_id: String,
    task_bound: bool,
    _publication: crate::services::WorkflowLaunchPublication,
}

/// Project authority transitions must serialize with writes for the same
/// canonical root, but an unrelated knowledge base must remain independent.
/// Weak entries keep the registry bounded once no operation owns a lane.
#[derive(Default)]
pub(crate) struct ProjectTrustTransitionLanes {
    lanes: Mutex<HashMap<String, Weak<Mutex<()>>>>,
}

impl ProjectTrustTransitionLanes {
    fn lane(&self, root: &Path) -> Result<Arc<Mutex<()>>, BackendError> {
        let identity = crate::services::project_identity(root).map_err(project_identity_error)?;
        let mut lanes = self.lanes.lock().map_err(|_| trust_transition_locked())?;
        lanes.retain(|_, lane| lane.strong_count() > 0);
        if let Some(lane) = lanes
            .get(&identity.canonical_identity_key)
            .and_then(Weak::upgrade)
        {
            return Ok(lane);
        }
        let lane = Arc::new(Mutex::new(()));
        lanes.insert(identity.canonical_identity_key, Arc::downgrade(&lane));
        Ok(lane)
    }
}

impl ProjectExecutionLease {
    pub(crate) fn validates(&self, context: &ProjectContext) -> bool {
        self.context.project_id == context.project_id && self.context.root == context.root
    }

    pub(crate) fn authority_revision(&self) -> &str {
        &self.authority_revision
    }

    pub(crate) fn task_context(&self, task_id: &str) -> Result<&ProjectContext, BackendError> {
        if self.task_bound && self.execution_id == task_id {
            Ok(&self.context)
        } else {
            Err(BackendError::new(
                "PROJECT_EXECUTION_CAPABILITY_MISMATCH",
                "The external execution capability does not own this project task.",
                false,
                true,
            ))
        }
    }
}

impl ProjectWriteRootKind {
    fn detail(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Wiki => "wiki",
            Self::Export => "export",
            Self::Query => "query",
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub blocking_work: BlockingWorkCoordinator,
    pub project_registry: ProjectRegistry,
    pub project_service: ProjectService,
    pub project_assessment_service: ProjectAssessmentService,
    pub file_store: FileStore,
    pub import_v2_service: ImportV2Service,
    pub import_capability_runtime: ImportCapabilityRuntime,
    pub connector_session_service: ConnectorSessionService,
    pub git_service: GitService,
    pub agent_service: AgentService,
    pub bookmark_service: BookmarkService,
    pub chat_convenience_service: ChatConvenienceService,
    pub chat_service: ChatService,
    pub llm_service: LlmService,
    pub search_service: SearchService,
    pub graph_service: GraphService,
    pub lint_service: LintService,
    pub export_service: ExportService,
    pub settings_service: SettingsService,
    pub update_service: crate::services::UpdateService,
    #[cfg(feature = "gui")]
    pub update_runtime: crate::services::DesktopUpdateRuntime,
    pub secret_service: SecretService,
    pub task_service: TaskService,
    pub workflow_service: WorkflowService,
    pub workflow_launch_registry: crate::services::WorkflowLaunchRegistry,
    pub project_execution_registry: crate::services::WorkflowLaunchRegistry,
    pub confirmation_registry: ConfirmationRegistry,
    pub(crate) project_trust_transition: ProjectTrustTransitionLanes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTrustAuthority {
    Untrusted,
    TrustedNative,
    TrustedCompatible,
}

impl ProjectTrustAuthority {
    fn trust_kind(self) -> Option<ProjectTrustKind> {
        match self {
            Self::Untrusted => None,
            Self::TrustedNative => Some(ProjectTrustKind::Native),
            Self::TrustedCompatible => Some(ProjectTrustKind::Compatible),
        }
    }
}

#[derive(Debug, Clone)]
struct RegisteredProject {
    root: PathBuf,
    trust: ProjectTrustAuthority,
    trusted_identity_revision: Option<String>,
    authority_revision: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedProjectAuthority {
    pub context: ProjectContext,
    pub trust: ProjectTrustAuthority,
    pub trust_kind: Option<ProjectTrustKind>,
    pub canonical_identity_key: Option<String>,
    pub identity_revision: Option<String>,
    pub authority_revision: String,
}

#[derive(Default)]
pub struct ProjectRegistry {
    projects: RwLock<HashMap<String, RegisteredProject>>,
}

impl ProjectRegistry {
    pub fn register(
        &self,
        project_id: impl Into<String>,
        root: &Path,
    ) -> Result<ProjectContext, BackendError> {
        self.register_with_authority(project_id, root, ProjectTrustAuthority::Untrusted)
    }

    /// Registers a backend-verified native project as trusted.
    ///
    /// This is intentionally separate from `register`: canonical path
    /// ownership alone is never sufficient to grant external execution or
    /// mutation capabilities.
    pub fn register_trusted_native(
        &self,
        project_id: impl Into<String>,
        root: &Path,
    ) -> Result<ProjectContext, BackendError> {
        self.register_with_authority(project_id, root, ProjectTrustAuthority::TrustedNative)
    }

    /// Rebinds a native project after an explicit recent-project relocation.
    /// The caller must already have compared the app-owned durable project ID
    /// at the new root. We still require the old registered root to match the
    /// exact recent entry and revalidate the new strict-native layout before
    /// changing this process's authority binding.
    pub fn relocate_trusted_native<F>(
        &self,
        project_id: &str,
        previous_root: &Path,
        new_root: &Path,
        update_recent: F,
    ) -> Result<ProjectContext, BackendError>
    where
        F: FnOnce() -> Result<(), BackendError>,
    {
        let canonical_root = new_root.canonicalize().map_err(|error| {
            BackendError::new(
                PATH_INVALID,
                "Relocated project root could not be resolved.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
        if !Self::is_strict_native_layout(&canonical_root) {
            return Err(invalid_trust_authority(
                "Only a backend-verified native project layout can be relocated.",
            ));
        }
        let identity =
            crate::services::project_identity(&canonical_root).map_err(project_identity_error)?;
        let expected_previous_root = root_match_key(previous_root);
        let mut projects = self.projects.write().map_err(|_| registry_locked())?;
        if let Some(registered) = projects.get_mut(project_id) {
            if root_match_key(&registered.root) != expected_previous_root {
                return Err(context_mismatch());
            }
            // Commit the global entry before mutating authority. The closure
            // performs the final durable-ID check and atomic recent write
            // while this registry entry is still bound to its old root.
            update_recent()?;
            registered.root = canonical_root.clone();
            registered.trust = ProjectTrustAuthority::TrustedNative;
            registered.trusted_identity_revision = Some(identity.identity_revision);
            registered.authority_revision = uuid::Uuid::new_v4().to_string();
        } else {
            update_recent()?;
            projects.insert(
                project_id.to_string(),
                RegisteredProject {
                    root: canonical_root.clone(),
                    trust: ProjectTrustAuthority::TrustedNative,
                    trusted_identity_revision: Some(identity.identity_revision),
                    authority_revision: uuid::Uuid::new_v4().to_string(),
                },
            );
        }
        Ok(ProjectContext::new(project_id, canonical_root))
    }

    /// Registers a compatible vault only after a backend-owned caller has
    /// collected the explicit user confirmation required by project-open.
    fn register_trusted_compatible(
        &self,
        project_id: impl Into<String>,
        root: &Path,
    ) -> Result<ProjectContext, BackendError> {
        self.register_with_authority(project_id, root, ProjectTrustAuthority::TrustedCompatible)
    }

    fn register_trusted_compatible_with_identity(
        &self,
        project_id: impl Into<String>,
        root: &Path,
        expected_identity_key: &str,
        expected_identity_revision: &str,
    ) -> Result<ProjectContext, BackendError> {
        self.register_with_expected_authority(
            project_id,
            root,
            ProjectTrustAuthority::TrustedCompatible,
            Some((expected_identity_key, expected_identity_revision)),
        )
    }

    fn register_with_authority(
        &self,
        project_id: impl Into<String>,
        root: &Path,
        requested_trust: ProjectTrustAuthority,
    ) -> Result<ProjectContext, BackendError> {
        self.register_with_expected_authority(project_id, root, requested_trust, None)
    }

    fn register_with_expected_authority(
        &self,
        project_id: impl Into<String>,
        root: &Path,
        requested_trust: ProjectTrustAuthority,
        expected_identity: Option<(&str, &str)>,
    ) -> Result<ProjectContext, BackendError> {
        if !root.is_absolute() {
            return Err(BackendError::new(
                PATH_INVALID,
                "Project root must be an absolute path.",
                false,
                true,
            ));
        }
        let canonical_root = root.canonicalize().map_err(|error| {
            BackendError::new(
                PATH_INVALID,
                "Project root could not be resolved.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "error": error.to_string() }))
        })?;
        match requested_trust {
            ProjectTrustAuthority::TrustedNative
                if !Self::is_strict_native_layout(&canonical_root) =>
            {
                return Err(invalid_trust_authority(
                    "Only a backend-verified native project layout can receive native trust.",
                ));
            }
            ProjectTrustAuthority::TrustedCompatible
                if !Self::is_verified_compatible_layout(&canonical_root) =>
            {
                return Err(invalid_trust_authority(
                    "Only a verified compatible Markdown layout can receive compatible trust.",
                ));
            }
            _ => {}
        }
        let trusted_identity = requested_trust
            .trust_kind()
            .map(|_| {
                crate::services::project_identity(&canonical_root).map_err(project_identity_error)
            })
            .transpose()?;
        if let (Some(expected), Some(identity)) = (expected_identity, trusted_identity.as_ref()) {
            if identity.canonical_identity_key != expected.0
                || identity.identity_revision != expected.1
            {
                return Err(invalid_trust_authority(
                    "The project identity changed while trust was being granted.",
                ));
            }
        }
        let trusted_identity_revision = trusted_identity
            .as_ref()
            .map(|identity| identity.identity_revision.clone());
        let project_id = project_id.into();
        let mut projects = self.projects.write().map_err(|_| registry_locked())?;
        if let Some(registered) = projects.get_mut(&project_id) {
            if registered.root != canonical_root {
                return Err(context_mismatch());
            }
            if requested_trust != ProjectTrustAuthority::Untrusted {
                registered.trust = requested_trust;
                registered.trusted_identity_revision = trusted_identity_revision;
                registered.authority_revision = uuid::Uuid::new_v4().to_string();
            }
        } else {
            projects.insert(
                project_id.clone(),
                RegisteredProject {
                    root: canonical_root.clone(),
                    trust: requested_trust,
                    trusted_identity_revision,
                    authority_revision: uuid::Uuid::new_v4().to_string(),
                },
            );
        }
        ProjectContext::new(project_id, canonical_root).with_resolved_layout()
    }

    pub fn resolve(
        &self,
        project_id: &str,
        asserted_root: &Path,
    ) -> Result<ProjectContext, BackendError> {
        let asserted_root = asserted_root
            .canonicalize()
            .map_err(|_| context_mismatch())?;
        let registered_root = self
            .projects
            .read()
            .map_err(|_| registry_locked())?
            .get(project_id)
            .map(|project| project.root.clone())
            .ok_or_else(context_mismatch)?;
        if asserted_root != registered_root {
            return Err(context_mismatch());
        }
        Ok(ProjectContext::new(project_id, registered_root))
    }

    pub fn resolve_authority(
        &self,
        project_id: &str,
        asserted_root: &Path,
    ) -> Result<ResolvedProjectAuthority, BackendError> {
        let context = self
            .resolve(project_id, asserted_root)?
            .with_resolved_layout()?;
        let mut projects = self.projects.write().map_err(|_| registry_locked())?;
        let registered = projects.get_mut(project_id).ok_or_else(context_mismatch)?;
        if registered.root != context.root {
            return Err(context_mismatch());
        }
        let current_identity = crate::services::project_identity(&context.root).ok();
        let identity_matches =
            registered
                .trusted_identity_revision
                .as_deref()
                .is_some_and(|expected_revision| {
                    current_identity
                        .as_ref()
                        .is_some_and(|identity| identity.identity_revision == expected_revision)
                });
        let trust_is_current = match registered.trust {
            ProjectTrustAuthority::TrustedNative => {
                identity_matches && Self::is_strict_native_layout(&context.root)
            }
            ProjectTrustAuthority::TrustedCompatible => {
                identity_matches && Self::is_verified_compatible_layout(&context.root)
            }
            ProjectTrustAuthority::Untrusted => false,
        };
        if registered.trust != ProjectTrustAuthority::Untrusted && !trust_is_current {
            // A path binding is not a renewable trust grant. Once the backend
            // observes that the verified native layout or directory identity
            // has drifted, revoke this runtime authority permanently. Merely
            // recreating the missing path must not make an old preparation
            // token valid again; a backend-owned reopen/reassessment is needed.
            registered.trust = ProjectTrustAuthority::Untrusted;
            registered.trusted_identity_revision = None;
            registered.authority_revision = uuid::Uuid::new_v4().to_string();
        }
        let trust = if trust_is_current {
            registered.trust
        } else {
            ProjectTrustAuthority::Untrusted
        };
        Ok(ResolvedProjectAuthority {
            context,
            trust,
            trust_kind: trust.trust_kind(),
            canonical_identity_key: current_identity
                .as_ref()
                .map(|identity| identity.canonical_identity_key.clone()),
            identity_revision: current_identity.map(|identity| identity.identity_revision),
            authority_revision: registered.authority_revision.clone(),
        })
    }

    pub fn revoke_trust(&self, project_id: &str, asserted_root: &Path) -> Result<(), BackendError> {
        let context = self.resolve(project_id, asserted_root)?;
        let mut projects = self.projects.write().map_err(|_| registry_locked())?;
        let registered = projects.get_mut(project_id).ok_or_else(context_mismatch)?;
        if registered.root != context.root {
            return Err(context_mismatch());
        }
        if registered.trust != ProjectTrustAuthority::Untrusted {
            registered.trust = ProjectTrustAuthority::Untrusted;
            registered.trusted_identity_revision = None;
            registered.authority_revision = uuid::Uuid::new_v4().to_string();
        }
        Ok(())
    }

    pub fn is_strict_native_layout(root: &Path) -> bool {
        matches!(
            inspect_native_layout(root).state,
            NativeLayoutState::Current
        )
    }

    fn is_verified_compatible_layout(root: &Path) -> bool {
        resolve_layout(root).is_ok_and(|resolution| {
            !resolution.layout.markdown_roots.is_empty()
                && resolution.confidence != ProjectLayoutConfidence::Low
        })
    }
}

fn project_identity_error(message: String) -> BackendError {
    BackendError::new(
        "PROJECT_IDENTITY_FAILED",
        "Project identity could not be established.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "error": message }))
}

fn invalid_trust_authority(message: &str) -> BackendError {
    BackendError::new("PROJECT_TRUST_AUTHORITY_INVALID", message, true, true)
}

fn context_mismatch() -> BackendError {
    BackendError::new(
        PROJECT_CONTEXT_MISMATCH,
        "Project id and root do not match an opened backend project.",
        true,
        true,
    )
}

fn root_match_key(path: &Path) -> String {
    // Roots can arrive through platform aliases (for example an 8.3 path
    // from RUNNER_TEMP on Windows) while the registry stores the canonical
    // spelling. A relocation's leaf is already absent, so canonicalize the
    // nearest surviving ancestor and append the missing tail.
    let canonical = canonicalize_nearest_existing_ancestor(path);
    lexical_root_match_key(&canonical)
}

fn canonicalize_nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut candidate = path;
    let mut missing_tail = Vec::new();
    loop {
        if let Ok(mut canonical) = candidate.canonicalize() {
            for component in missing_tail.iter().rev() {
                canonical.push(component);
            }
            return canonical;
        }
        let (Some(parent), Some(name)) = (candidate.parent(), candidate.file_name()) else {
            return path.to_path_buf();
        };
        missing_tail.push(name.to_os_string());
        candidate = parent;
    }
}

fn lexical_root_match_key(path: &Path) -> String {
    let mut key = path
        .to_string_lossy()
        .trim_end_matches(['/', '\\'])
        .replace('\\', "/");
    if cfg!(windows) {
        key.make_ascii_lowercase();
        if let Some(without_device_prefix) = key.strip_prefix("//?/") {
            key = without_device_prefix.to_string();
            if let Some(unc_path) = key.strip_prefix("unc/") {
                key = format!("//{unc_path}");
            }
        }
    }
    key
}

fn registry_locked() -> BackendError {
    BackendError::new(
        "PROJECT_REGISTRY_LOCKED",
        "Project registry is unavailable.",
        true,
        false,
    )
}

fn trust_transition_locked() -> BackendError {
    BackendError::new(
        "PROJECT_TRUST_TRANSITION_LOCKED",
        "Project trust authority is temporarily unavailable.",
        true,
        false,
    )
}

impl AppState {
    pub fn resolve_project_context(
        &self,
        project_id: &str,
        asserted_root: &str,
    ) -> Result<ProjectContext, BackendError> {
        self.project_registry
            .resolve(project_id, Path::new(asserted_root))
            .and_then(ProjectContext::with_resolved_layout)
    }

    pub fn resolve_workflow_access(
        &self,
        context: &ProjectContext,
    ) -> Result<crate::services::WorkflowAccessSnapshot, BackendError> {
        self.with_workflow_access(context, Ok)
    }

    /// External AI and Agent execution is an explicit privacy boundary. A
    /// registry entry alone is never sufficient: the project must retain a
    /// current trusted authority after layout, identity, and health are
    /// re-evaluated under the same transition lock used by Workflows.
    pub fn require_external_ai_access(&self, context: &ProjectContext) -> Result<(), BackendError> {
        self.with_workflow_access(context, |access| {
            let health = self
                .project_assessment_service
                .inspect_current(context.root.to_string_lossy().as_ref())?
                .health;
            if access.trust == WorkflowProjectTrust::Trusted
                && health == crate::models::project::ProjectHealth::Healthy
            {
                return Ok(());
            }
            Err(BackendError::new(
                "PROJECT_EXTERNAL_AI_REQUIRES_TRUST",
                "Trust this knowledge base before sending its content to an external AI or Agent.",
                true,
                true,
            ))
        })
    }

    /// Issues an epoch-bound permit immediately before a workflow starts an
    /// external Agent/BYOK invocation. The transition lock is released before
    /// the blocking process/network call; the permit's publication guard then
    /// prevents revocation from returning until that cancellable call has
    /// either entered and returned or aborted.
    pub(crate) fn publish_workflow_external_launch(
        &self,
        context: &ProjectContext,
        run: &crate::models::workflow::WorkflowRun,
    ) -> Result<crate::services::WorkflowExternalLaunchPermit, BackendError> {
        self.with_workflow_access(context, |access| {
            let current = self
                .task_service
                .get_workflow_run(&run.task_id)
                .ok_or_else(|| {
                    BackendError::new(
                        "WORKFLOW_DISPATCH_INVARIANT",
                        "The workflow disappeared before external execution.",
                        false,
                        true,
                    )
                })?;
            if self.task_service.is_cancelled(&run.task_id)
                || current.display_status
                    != crate::models::workflow::WorkflowDisplayStatus::Running
                || current.canonical_identity_key != run.canonical_identity_key
                || current.identity_revision != run.identity_revision
                || current.kind != run.kind
                || current.fingerprint != run.fingerprint
            {
                return Err(BackendError::new(
                    "WORKFLOW_EXTERNAL_LAUNCH_REVOKED",
                    "Workflow authority changed before external execution started.",
                    true,
                    true,
                ));
            }
            if access.trust != WorkflowProjectTrust::Trusted {
                return Err(BackendError::new(
                    "PROJECT_EXTERNAL_AI_REQUIRES_TRUST",
                    "Trust this knowledge base before sending its content to an external AI or Agent.",
                    true,
                    true,
                ));
            }
            let agent_lint_repair = matches!(
                run.operation,
                crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
            );
            if (matches!(
                run.kind,
                crate::models::workflow::WorkflowKind::UpdateWiki
                    | crate::models::workflow::WorkflowKind::GenerateContent
            ) || agent_lint_repair)
                && access.filesystem_access != WorkflowFilesystemAccess::Writable
            {
                return Err(BackendError::new(
                    "PROJECT_WRITE_REQUIRES_TRUST",
                    "Current project authority no longer permits this workflow mutation.",
                    true,
                    true,
                ));
            }
            if agent_lint_repair {
                self.validate_project_write_access(context, &access)?;
                if access.persistence
                    != crate::models::workflow::WorkflowPersistenceMode::Persistent
                {
                    return Err(BackendError::new(
                        "LINT_REPAIR_PERSISTENCE_REQUIRED",
                        "Agent lint repair requires persistent project task state.",
                        true,
                        true,
                    ));
                }
                self.require_project_content_write_root(context, ProjectWriteRootKind::Wiki)?;
            }
            self.workflow_launch_registry.issue(
                &run.canonical_identity_key,
                &run.task_id,
                &access.authority_revision,
            )
        })
    }

    /// Publishes the short project-persistence window used by Local Quick
    /// report storage. Trust revocation closes this epoch before rebinding the
    /// workflow to memory-only and waits for any begun write before returning.
    pub(crate) fn publish_workflow_persistent_report(
        &self,
        context: &ProjectContext,
        run: &crate::models::workflow::WorkflowRun,
    ) -> Result<Option<crate::services::WorkflowExternalLaunchPermit>, BackendError> {
        self.with_workflow_access(context, |access| {
            let current = self
                .task_service
                .get_workflow_run(&run.task_id)
                .ok_or_else(|| {
                    BackendError::new(
                        "WORKFLOW_DISPATCH_INVARIANT",
                        "The workflow disappeared before report persistence.",
                        false,
                        true,
                    )
                })?;
            if self.task_service.is_cancelled(&run.task_id)
                || current.display_status != crate::models::workflow::WorkflowDisplayStatus::Running
                || current.canonical_identity_key != run.canonical_identity_key
                || current.identity_revision != run.identity_revision
                || current.kind != run.kind
                || current.fingerprint != run.fingerprint
            {
                return Err(BackendError::new(
                    "WORKFLOW_REPORT_AUTHORITY_CHANGED",
                    "Workflow authority changed before report persistence.",
                    true,
                    true,
                ));
            }
            if self
                .task_service
                .workflow_persistence_dir(&run.task_id)
                .is_none()
            {
                return Ok(None);
            }
            if access.trust != WorkflowProjectTrust::Trusted
                || access.filesystem_access != WorkflowFilesystemAccess::Writable
            {
                return Err(BackendError::new(
                    "WORKFLOW_REPORT_AUTHORITY_CHANGED",
                    "Persistent Health Check reports require current trusted writable authority.",
                    true,
                    true,
                ));
            }
            self.workflow_launch_registry
                .issue(
                    &run.canonical_identity_key,
                    &run.task_id,
                    &access.authority_revision,
                )
                .map(Some)
        })
    }

    /// Project-scoped mutations may only use app state that the current
    /// authority proved both trusted and writable. This prevents restricted,
    /// Recovery, and read-only projects from creating `.app` state through a
    /// command that happened to receive a registered canonical path.
    pub fn require_project_write_access(
        &self,
        context: &ProjectContext,
    ) -> Result<(), BackendError> {
        self.with_workflow_access(context, |access| {
            self.validate_project_write_access(context, &access)
        })
    }

    /// Resolve the current layout and hold the authority transition lock for
    /// the entire mutation. The service call receives an unforgeable permit,
    /// so command handlers cannot validate, release the lock, and later write
    /// with a naked `ProjectContext`.
    pub fn with_current_project_write_access<T>(
        &self,
        project_id: &str,
        asserted_root: &str,
        operation: impl FnOnce(&ProjectWritePermit<'_>, &ProjectContext) -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        let update_mutation_lease = self
            .confirmation_registry
            .update_install_barrier()
            .enter_project_mutation()?;
        let transition_lane = self
            .project_trust_transition
            .lane(Path::new(asserted_root))?;
        let transition = transition_lane
            .lock()
            .map_err(|_| trust_transition_locked())?;
        let context = self
            .project_registry
            .resolve(project_id, Path::new(asserted_root))
            .and_then(ProjectContext::with_resolved_layout)?;
        let access = self.resolve_workflow_access_locked(&context)?;
        self.validate_project_write_access(&context, &access)?;
        let permit = ProjectWritePermit {
            context: context.clone(),
            workflow_access: access,
            _update_mutation_lease: update_mutation_lease,
            _transition_guard: &transition,
        };
        operation(&permit, &context)
    }

    /// Hold the same transition barrier as a normal write permit while an
    /// explicitly confirmed repair/trust/bootstrap action changes project
    /// authority. This intentionally validates writability without requiring
    /// pre-existing trust or healthy state.
    pub(crate) fn with_current_project_authority_mutation<T>(
        &self,
        project_id: &str,
        asserted_root: &str,
        operation: impl FnOnce(
            &ProjectAuthorityMutationPermit<'_>,
            &ProjectContext,
        ) -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        let update_mutation_lease = self
            .confirmation_registry
            .update_install_barrier()
            .enter_project_mutation()?;
        let transition_lane = self
            .project_trust_transition
            .lane(Path::new(asserted_root))?;
        let transition = transition_lane
            .lock()
            .map_err(|_| trust_transition_locked())?;
        let context = self
            .project_registry
            .resolve(project_id, Path::new(asserted_root))
            .and_then(ProjectContext::with_resolved_layout)?;
        if self.project_service.filesystem_access(&context, true)
            != crate::models::project::ProjectFilesystemAccess::Writable
        {
            return Err(BackendError::new(
                "PROJECT_WRITE_READ_ONLY",
                "This project is read-only. Choose a writable folder before changing project authority.",
                true,
                true,
            ));
        }
        let permit = ProjectAuthorityMutationPermit {
            context: context.clone(),
            _update_mutation_lease: update_mutation_lease,
            _transition_guard: &transition,
        };
        operation(&permit, &context)
    }

    pub(crate) fn with_current_project_task_access<T>(
        &self,
        project_id: &str,
        asserted_root: &str,
        operation: impl FnOnce(&ProjectTaskMutationPermit<'_>) -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        let transition_lane = self
            .project_trust_transition
            .lane(Path::new(asserted_root))?;
        let transition = transition_lane
            .lock()
            .map_err(|_| trust_transition_locked())?;
        let context = self
            .project_registry
            .resolve(project_id, Path::new(asserted_root))
            .and_then(ProjectContext::with_resolved_layout)?;
        let workflow_access = self.resolve_workflow_access_locked(&context)?;
        if workflow_access.persistence == WorkflowPersistenceMode::MemoryOnly {
            self.task_service
                .rebind_workflows_for_root(&context.root, None)
                .map_err(|message| {
                    BackendError::new("WORKFLOW_PERSISTENCE_REBIND_FAILED", message, true, true)
                })?;
            self.task_service
                .unbind_non_workflow_persistence_for_root(&context.root)
                .map_err(|message| {
                    BackendError::new(
                        "PROJECT_TASK_PERSISTENCE_REVOKE_FAILED",
                        message,
                        true,
                        true,
                    )
                })?;
        }
        let permit = ProjectTaskMutationPermit {
            context,
            workflow_access,
            _transition_guard: &transition,
        };
        operation(&permit)
    }

    /// Acquire the current project execution epoch immediately before an
    /// Agent process or BYOK request is published. Retrieval may happen before
    /// this point; publication and result handling must retain the returned
    /// lease until they are fully drained.
    pub fn begin_project_external_task(
        &self,
        context: &ProjectContext,
        task_id: &str,
    ) -> Result<ProjectExecutionLease, BackendError> {
        self.begin_project_external_execution_bound(context, task_id, true)
    }

    /// Acquire the same project execution epoch for short external actions
    /// that are not represented as TaskService tasks. Revocation cannot
    /// cancel these directly, so it waits for this lease to drain.
    pub(crate) fn begin_project_external_execution(
        &self,
        context: &ProjectContext,
        execution_id: &str,
    ) -> Result<ProjectExecutionLease, BackendError> {
        self.begin_project_external_execution_bound(context, execution_id, false)
    }

    fn begin_project_external_execution_bound(
        &self,
        context: &ProjectContext,
        execution_id: &str,
        require_task: bool,
    ) -> Result<ProjectExecutionLease, BackendError> {
        let transition_lane = self.project_trust_transition.lane(&context.root)?;
        let transition = transition_lane
            .lock()
            .map_err(|_| trust_transition_locked())?;
        let access = self.resolve_workflow_access_locked(context)?;
        let health = self
            .project_assessment_service
            .inspect_current(context.root.to_string_lossy().as_ref())?
            .health;
        if access.trust != WorkflowProjectTrust::Trusted
            || health != crate::models::project::ProjectHealth::Healthy
        {
            return Err(BackendError::new(
                "PROJECT_EXTERNAL_AI_REQUIRES_TRUST",
                "Trust this knowledge base before sending its content to an external AI or Agent.",
                true,
                true,
            ));
        }
        if require_task
            && (!self
                .task_service
                .task_belongs_to_root(execution_id, &context.root)
                || self.task_service.is_cancelled(execution_id))
        {
            return Err(BackendError::new(
                "PROJECT_EXTERNAL_LAUNCH_REVOKED",
                "Project authority changed before external execution started.",
                true,
                true,
            ));
        }
        let publication = self
            .project_execution_registry
            .issue(
                &project_execution_owner(&context.root),
                execution_id,
                &access.authority_revision,
            )?
            .begin()?;
        drop(transition);
        Ok(ProjectExecutionLease {
            context: context.clone(),
            authority_revision: access.authority_revision,
            execution_id: execution_id.to_string(),
            task_bound: require_task,
            _publication: publication,
        })
    }

    pub(crate) fn require_current_execution_epoch(
        &self,
        context: &ProjectContext,
        lease: &ProjectExecutionLease,
    ) -> Result<(), BackendError> {
        if !lease.validates(context) {
            return Err(BackendError::new(
                "PROJECT_EXTERNAL_LAUNCH_REVOKED",
                "The external result belongs to a different project authority.",
                true,
                true,
            ));
        }
        self.with_workflow_access(context, |access| {
            if access.trust == WorkflowProjectTrust::Trusted
                && access.authority_revision == lease.authority_revision()
            {
                Ok(())
            } else {
                Err(BackendError::new(
                    "PROJECT_EXTERNAL_LAUNCH_REVOKED",
                    "Project authority changed before the external result could be committed.",
                    true,
                    true,
                ))
            }
        })
    }

    pub(crate) fn require_current_execution_permit(
        &self,
        permit: &ProjectWritePermit<'_>,
        lease: &ProjectExecutionLease,
    ) -> Result<(), BackendError> {
        if lease.validates(permit.context())
            && permit.authority_revision() == lease.authority_revision()
        {
            Ok(())
        } else {
            Err(BackendError::new(
                "PROJECT_EXTERNAL_LAUNCH_REVOKED",
                "Project authority changed before the external result could be committed.",
                true,
                true,
            ))
        }
    }

    fn validate_project_write_access(
        &self,
        context: &ProjectContext,
        access: &crate::services::WorkflowAccessSnapshot,
    ) -> Result<(), BackendError> {
        let health = self
            .project_assessment_service
            .inspect_current(context.root.to_string_lossy().as_ref())?
            .health;
        if access.trust != WorkflowProjectTrust::Trusted {
            return Err(BackendError::new(
                "PROJECT_WRITE_REQUIRES_TRUST",
                "Trust this knowledge base before changing project files.",
                true,
                true,
            ));
        }
        if health != crate::models::project::ProjectHealth::Healthy {
            return Err(BackendError::new(
                "PROJECT_WRITE_STATE_UNAVAILABLE",
                "Project health does not permit a safe write.",
                true,
                true,
            ));
        }
        if access.filesystem_access != WorkflowFilesystemAccess::Writable {
            return Err(BackendError::new(
                "PROJECT_WRITE_READ_ONLY",
                "This knowledge base is currently read-only.",
                true,
                true,
            ));
        }
        Ok(())
    }

    /// Content writes require a layout-specific root in addition to the
    /// authority/filesystem/persistence checks above. App-owned state alone
    /// is deliberately insufficient for a compatible vault.
    pub(crate) fn require_project_content_write_root(
        &self,
        context: &ProjectContext,
        root_kind: ProjectWriteRootKind,
    ) -> Result<(), BackendError> {
        let relative = match root_kind {
            ProjectWriteRootKind::Source => context.layout.source_write_root.as_deref(),
            ProjectWriteRootKind::Wiki => context.layout.wiki_write_root.as_deref(),
            ProjectWriteRootKind::Export => context.layout.export_root.as_deref(),
            ProjectWriteRootKind::Query => context.layout.queries_write_root.as_deref(),
        };
        let missing = || {
            BackendError::new(
                "PROJECT_LAYOUT_ROOT_UNAVAILABLE",
                "The project layout does not provide the required content write root.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "rootKind": root_kind.detail() }))
        };
        let relative = relative.ok_or_else(missing)?;
        let root = context
            .resolve_project_path(relative)
            .map_err(|_| missing())?;
        validate_existing_project_directory(&context.root, &root).map_err(|_| missing())?;
        Ok(())
    }

    pub(crate) fn require_workflow_content_write_root(
        &self,
        context: &ProjectContext,
        kind: &WorkflowKind,
    ) -> Result<(), BackendError> {
        match kind {
            WorkflowKind::UpdateWiki => {
                self.require_project_content_write_root(context, ProjectWriteRootKind::Wiki)
            }
            WorkflowKind::GenerateContent => {
                self.require_project_content_write_root(context, ProjectWriteRootKind::Export)
            }
            WorkflowKind::HealthCheck => Ok(()),
        }
    }

    pub(crate) fn with_workflow_access<T>(
        &self,
        context: &ProjectContext,
        operation: impl FnOnce(crate::services::WorkflowAccessSnapshot) -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        let transition_lane = self.project_trust_transition.lane(&context.root)?;
        let _transition = transition_lane
            .lock()
            .map_err(|_| trust_transition_locked())?;
        let access = self.resolve_workflow_access_locked(context)?;
        operation(access)
    }

    fn resolve_workflow_access_locked(
        &self,
        context: &ProjectContext,
    ) -> Result<crate::services::WorkflowAccessSnapshot, BackendError> {
        let mut authority = self
            .project_registry
            .resolve_authority(&context.project_id, &context.root)?;
        if authority.trust == ProjectTrustAuthority::TrustedCompatible {
            let durable_trust_is_current = matches!(
                self.project_service
                    .restore_project_trust(&authority.context.root),
                Ok(Some(ProjectTrustKind::Compatible))
            );
            if !durable_trust_is_current {
                self.project_registry
                    .revoke_trust(&context.project_id, &context.root)?;
                self.task_service
                    .rebind_workflows_for_root(&context.root, None)
                    .map_err(|message| {
                        BackendError::new("WORKFLOW_PERSISTENCE_REBIND_FAILED", message, true, true)
                    })?;
                authority = self
                    .project_registry
                    .resolve_authority(&context.project_id, &context.root)?;
            }
        }
        // Recovery is readable-only until an explicit repair flow completes.
        // Do not let a previously trusted registry record keep workflow, Git,
        // or external-AI mutation access alive after the on-disk app state has
        // become unhealthy.
        let health = self
            .project_assessment_service
            .inspect_current(authority.context.root.to_string_lossy().as_ref())?
            .health;
        let trusted = authority.trust != ProjectTrustAuthority::Untrusted
            && !matches!(
                health,
                crate::models::project::ProjectHealth::Recovery
                    | crate::models::project::ProjectHealth::Repairable
            );
        let filesystem_access = self
            .project_service
            .filesystem_access(&authority.context, trusted);
        let persistent = trusted
            && filesystem_access == ProjectFilesystemAccess::Writable
            && self
                .project_service
                .has_writable_task_state_root(&authority.context);
        let git = self.git_service.repository_status(&authority.context)?;
        Ok(crate::services::WorkflowAccessSnapshot {
            trust: if trusted {
                WorkflowProjectTrust::Trusted
            } else {
                WorkflowProjectTrust::Untrusted
            },
            trust_kind: authority.trust_kind,
            filesystem_access: match filesystem_access {
                ProjectFilesystemAccess::Writable => WorkflowFilesystemAccess::Writable,
                ProjectFilesystemAccess::ReadOnly => WorkflowFilesystemAccess::ReadOnly,
            },
            persistence: if persistent {
                WorkflowPersistenceMode::Persistent
            } else {
                WorkflowPersistenceMode::MemoryOnly
            },
            git_state: if !git.is_repository {
                WorkflowGitState::Unavailable
            } else if git.has_changes {
                WorkflowGitState::Dirty
            } else {
                WorkflowGitState::Clean
            },
            authority_revision: authority.authority_revision,
        })
    }

    pub fn register_opened_project_authority(
        &self,
        project_id: impl Into<String>,
        root: &Path,
    ) -> Result<ProjectContext, BackendError> {
        let transition_lane = self.project_trust_transition.lane(root)?;
        let _transition = transition_lane
            .lock()
            .map_err(|_| trust_transition_locked())?;
        let project_id = project_id.into();
        if ProjectRegistry::is_strict_native_layout(root) {
            return self
                .project_registry
                .register_trusted_native(project_id, root);
        }
        match self.project_service.restore_project_trust(root)? {
            Some(ProjectTrustKind::Compatible) => {
                match self
                    .project_registry
                    .register_trusted_compatible(project_id.clone(), root)
                {
                    Ok(context) => Ok(context),
                    Err(error) if error.code == "PROJECT_TRUST_AUTHORITY_INVALID" => {
                        self.project_service.revoke_project_trust(root)?;
                        self.project_registry.register(project_id, root)
                    }
                    Err(error) => Err(error),
                }
            }
            Some(ProjectTrustKind::Native) => {
                self.project_service.revoke_project_trust(root)?;
                self.project_registry.register(project_id, root)
            }
            None => self.project_registry.register(project_id, root),
        }
    }

    /// Completes the authority transition after a confirmed legacy-native
    /// directory repair.  The caller has already re-assessed the filesystem;
    /// this method only grants native authority after the current layout is
    /// proven and binds workflow persistence only to a safe writable task
    /// directory.  If the bind fails, runtime trust is revoked rather than
    /// leaving a half-refreshed authority behind.
    pub(crate) fn refresh_native_authority_after_repair(
        &self,
        project_id: &str,
        root: &Path,
    ) -> Result<ProjectContext, BackendError> {
        let transition_lane = self.project_trust_transition.lane(root)?;
        let _transition = transition_lane
            .lock()
            .map_err(|_| trust_transition_locked())?;
        if !ProjectRegistry::is_strict_native_layout(root) {
            return Err(BackendError::new(
                "PROJECT_NATIVE_REPAIR_STALE",
                "The repaired project is not a complete current native layout.",
                true,
                true,
            ));
        }
        let context = self
            .project_registry
            .register_trusted_native(project_id.to_string(), root)?;
        let task_root = if self.project_service.has_writable_task_state_root(&context) {
            context
                .layout
                .task_state_root
                .as_ref()
                .map(|relative| context.root.join(relative))
        } else {
            None
        };
        if let Err(message) = self
            .task_service
            .rebind_workflows_for_root(&context.root, task_root)
        {
            let _ = self
                .project_registry
                .revoke_trust(project_id, &context.root);
            return Err(BackendError::new(
                "WORKFLOW_PERSISTENCE_REBIND_FAILED",
                message,
                true,
                true,
            ));
        }
        Ok(context)
    }

    /// Batch E owns the confirming command/UI. This backend method performs
    /// the validated grant and durable write once that confirmation exists.
    pub fn grant_compatible_project_trust(
        &self,
        project_id: &str,
        root: &Path,
    ) -> Result<ProjectContext, BackendError> {
        let transition_lane = self.project_trust_transition.lane(root)?;
        let _transition = transition_lane
            .lock()
            .map_err(|_| trust_transition_locked())?;
        self.project_registry.resolve(project_id, root)?;
        if !ProjectRegistry::is_verified_compatible_layout(root) {
            return Err(invalid_trust_authority(
                "Only a verified compatible Markdown layout can receive compatible trust.",
            ));
        }
        let identity = crate::services::project_identity(root).map_err(project_identity_error)?;
        self.project_service.grant_project_trust(
            &identity.canonical_root,
            ProjectTrustKind::Compatible,
            &identity.canonical_identity_key,
            &identity.identity_revision,
        )?;
        let context = match self
            .project_registry
            .register_trusted_compatible_with_identity(
                project_id.to_string(),
                &identity.canonical_root,
                &identity.canonical_identity_key,
                &identity.identity_revision,
            ) {
            Ok(context) => context,
            Err(error) => {
                let _ = self
                    .project_service
                    .revoke_project_trust(&identity.canonical_root);
                return Err(error);
            }
        };
        let task_root = self
            .project_service
            .has_writable_task_state_root(&context)
            .then(|| {
                context
                    .layout
                    .task_state_root
                    .as_ref()
                    .map(|relative| context.root.join(relative))
            })
            .flatten();
        if let Err(message) = self
            .task_service
            .rebind_workflows_for_root(&context.root, task_root)
        {
            let _ = self
                .project_registry
                .revoke_trust(project_id, &context.root);
            let _ = self.project_service.revoke_project_trust(&context.root);
            return Err(BackendError::new(
                "WORKFLOW_PERSISTENCE_REBIND_FAILED",
                message,
                true,
                true,
            ));
        }
        Ok(context)
    }

    pub fn revoke_project_trust(&self, project_id: &str, root: &Path) -> Result<(), BackendError> {
        let transition_lane = self.project_trust_transition.lane(root)?;
        let transition_guard = transition_lane
            .lock()
            .map_err(|_| trust_transition_locked())?;
        let project_execution_barrier = self
            .project_execution_registry
            .close_owner(&project_execution_owner(root));
        let mut launch_owners = self
            .task_service
            .list_workflow_runs()
            .into_iter()
            .filter(|run| self.task_service.task_belongs_to_root(&run.task_id, root))
            .map(|run| run.canonical_identity_key)
            .collect::<std::collections::HashSet<_>>();
        if let Ok(identity) = crate::services::project_identity(root) {
            launch_owners.insert(identity.canonical_identity_key);
        }
        let launch_barriers = launch_owners
            .into_iter()
            .map(|owner| self.workflow_launch_registry.close_owner(&owner))
            .collect::<Vec<_>>();
        let mut first_error = None;
        if let Err(error) = self
            .connector_session_service
            .stop_project_executions(project_id)
        {
            first_error = Some(error);
        }
        if let Err(message) = self
            .task_service
            .request_cancel_active_project_tasks_for_root(root)
        {
            first_error = Some(BackendError::new(
                "PROJECT_TRUST_REVOCATION_CANCEL_FAILED",
                message,
                true,
                true,
            ));
        }
        if let Err(message) = self
            .task_service
            .unbind_non_workflow_persistence_for_root(root)
        {
            if first_error.is_none() {
                first_error = Some(BackendError::new(
                    "PROJECT_TASK_PERSISTENCE_REVOKE_FAILED",
                    message,
                    true,
                    true,
                ));
            }
        }
        let (stopped_runs, retry_freeze) = match self
            .workflow_service
            .coordinator
            .freeze_owner_for_trust_revocation(&self.task_service, root)
        {
            Ok(transition) => {
                if let Some(message) = transition.errors.first() {
                    first_error = Some(BackendError::new(
                        "WORKFLOW_TRUST_REVOCATION_CANCEL_FAILED",
                        message.clone(),
                        true,
                        true,
                    ));
                }
                let retry = !transition.errors.is_empty();
                (transition.stopped_runs, retry)
            }
            Err(message) => {
                first_error = Some(BackendError::new(
                    "WORKFLOW_TRUST_REVOCATION_CANCEL_FAILED",
                    message,
                    true,
                    true,
                ));
                (Vec::new(), true)
            }
        };
        let context = ProjectContext::new(project_id, root.to_path_buf());
        self.cleanup_stopped_workflow_authority(&context, stopped_runs, &mut first_error);
        if let Err(message) = self.task_service.rebind_workflows_for_root(root, None) {
            if first_error.is_none() {
                first_error = Some(BackendError::new(
                    "WORKFLOW_PERSISTENCE_REBIND_FAILED",
                    message,
                    true,
                    true,
                ));
            }
        }
        if retry_freeze {
            match self
                .workflow_service
                .coordinator
                .freeze_owner_for_trust_revocation(&self.task_service, root)
            {
                Ok(retry) => {
                    self.cleanup_stopped_workflow_authority(
                        &context,
                        retry.stopped_runs,
                        &mut first_error,
                    );
                    if first_error.is_none() {
                        if let Some(message) = retry.errors.first() {
                            first_error = Some(BackendError::new(
                                "WORKFLOW_TRUST_REVOCATION_CANCEL_FAILED",
                                message.clone(),
                                true,
                                true,
                            ));
                        }
                    }
                }
                Err(message) if first_error.is_none() => {
                    first_error = Some(BackendError::new(
                        "WORKFLOW_TRUST_REVOCATION_CANCEL_FAILED",
                        message,
                        true,
                        true,
                    ));
                }
                Err(_) => {}
            }
        }
        if let Err(error) = self.project_registry.revoke_trust(project_id, root) {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        if let Err(error) = self.project_service.revoke_project_trust(root) {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        drop(transition_guard);
        if let Err(error) = project_execution_barrier.wait() {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        for barrier in launch_barriers {
            if let Err(error) = barrier.wait() {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn cleanup_stopped_workflow_authority(
        &self,
        context: &ProjectContext,
        stopped_runs: Vec<crate::models::workflow::WorkflowRun>,
        first_error: &mut Option<BackendError>,
    ) {
        for run in stopped_runs {
            if let Some(pending) = run.pending_action.as_ref() {
                if let Err(error) = self
                    .confirmation_registry
                    .cancel_workflow_binding(context, &run, pending)
                {
                    if first_error.is_none() {
                        *first_error = Some(error);
                    }
                }
            }
            let _ = crate::services::discard_update_wiki_candidate(&run.task_id);
            let _ = crate::services::discard_generate_content_candidate(&run.task_id);
        }
    }
}

fn project_execution_owner(root: &Path) -> String {
    let normalized = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    #[cfg(windows)]
    let normalized = normalized.to_ascii_lowercase();
    format!("project-execution:{normalized}")
}

#[cfg(test)]
mod project_registry_tests {
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Barrier};
    use std::time::Duration;

    #[cfg(windows)]
    use super::lexical_root_match_key;
    use super::{AppState, ProjectRegistry, ProjectTrustAuthority, ProjectWriteRootKind};
    use crate::errors::{BackendError, PROJECT_CONTEXT_MISMATCH};
    use crate::models::confirmation::{
        ConfirmationExecution, PendingAction, PendingActionType, RiskLevel, UpdateInstallBarrier,
    };
    use crate::models::project::{ProjectTemplate, ProjectTrustKind};
    use crate::models::task::{TaskStatus, TaskType};
    use crate::models::workflow::{
        HealthCheckMode, WorkflowCandidateReference, WorkflowExecutionOptions,
        WorkflowFilesystemAccess, WorkflowGitState, WorkflowKind, WorkflowPendingAction,
        WorkflowPersistenceMode, WorkflowPersistenceTransition, WorkflowProjectTrust,
        WorkflowResult, WorkflowRoute, WorkflowRun, WorkflowScope, WorkflowStage,
        WorkflowStageStatus, WorkflowStartOutcome,
    };
    use crate::services::{
        workflow_stages, EnqueueWorkflow, ProjectAssessmentService, ProjectService, WorkflowRunner,
    };

    #[cfg(windows)]
    #[test]
    fn root_match_key_normalizes_windows_unc_device_prefixes() {
        assert_eq!(
            lexical_root_match_key(std::path::Path::new(r"\\?\UNC\Server\Share\moved")),
            lexical_root_match_key(std::path::Path::new(r"\\server\share\moved")),
        );
    }

    #[cfg(windows)]
    #[test]
    fn relocation_matches_an_aliased_parent_after_the_old_leaf_moves() {
        let holder = temp_project("relocate-aliased-parent");
        let actual_parent = holder.join("canonical-parent");
        let alias_parent = holder.join("alias-parent");
        fs::create_dir_all(&actual_parent).unwrap();
        let output = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&alias_parent)
            .arg(&actual_parent)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "mklink /J failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        let seed = strict_native_project("relocate-aliased-seed");
        let old_root = actual_parent.join("old-project");
        let aliased_old_root = alias_parent.join("old-project");
        let new_root = holder.join("new-project");
        fs::rename(&seed, &old_root).unwrap();
        let registry = ProjectRegistry::default();
        registry
            .register_trusted_native("native-project", &aliased_old_root)
            .unwrap();
        fs::rename(&old_root, &new_root).unwrap();

        let context = registry
            .relocate_trusted_native("native-project", &aliased_old_root, &new_root, || Ok(()))
            .expect("a moved project must retain its canonical registered identity");
        assert_eq!(context.root, new_root.canonicalize().unwrap());

        fs::remove_dir(&alias_parent).ok();
        fs::remove_dir_all(holder).ok();
    }

    #[derive(Default)]
    struct CountingWorkflowRunner(AtomicUsize);

    #[test]
    fn update_install_barrier_serializes_project_mutation_admission() {
        let barrier = UpdateInstallBarrier::default();
        let mutation = barrier.enter_project_mutation().unwrap();
        let blocked = barrier.reserve_install_or_restart(|| Ok(())).unwrap_err();
        assert_eq!(blocked.code, "UPDATE_INSTALL_GUARD_BLOCKED");
        drop(mutation);

        let install = barrier.reserve_install_or_restart(|| Ok(())).unwrap();
        let blocked = barrier.enter_project_mutation().unwrap_err();
        assert_eq!(blocked.code, "UPDATE_INSTALL_IN_PROGRESS");
        drop(install);
        assert!(barrier.enter_project_mutation().is_ok());
    }

    impl WorkflowRunner for CountingWorkflowRunner {
        fn kind(&self) -> WorkflowKind {
            WorkflowKind::UpdateWiki
        }

        fn start(&self, _run: WorkflowRun) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn temp_project(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "llm-wiki-project-registry-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn strict_native_project(name: &str) -> std::path::PathBuf {
        let root = temp_project(name);
        fs::write(root.join("purpose.md"), "# Purpose").unwrap();
        fs::write(root.join("schema.md"), "# Schema").unwrap();
        for path in [
            root.join("raw").join("sources"),
            root.join("wiki"),
            root.join(".app").join("tasks"),
            root.join("exports"),
            root.join("skills"),
        ] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(root.join("wiki/index.md"), "# Index").unwrap();
        root
    }

    fn compatible_project(name: &str) -> std::path::PathBuf {
        let root = temp_project(name);
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::write(root.join("index.md"), "# Existing vault").unwrap();
        root
    }

    fn state_with_temp_config(label: &str) -> (AppState, std::path::PathBuf) {
        let config = temp_project(label);
        let state = AppState {
            project_assessment_service: ProjectAssessmentService::new(config.clone()),
            project_service: ProjectService::with_config_dir(config.clone()),
            ..AppState::default()
        };
        (state, config)
    }

    fn cleanup_paths(paths: &[&std::path::PathBuf]) {
        for path in paths {
            fs::remove_dir_all(path).ok();
        }
    }

    #[test]
    fn registered_project_rejects_a_id_with_b_root() {
        let registry = ProjectRegistry::default();
        let project_a = temp_project("项目甲");
        let project_b = temp_project("项目乙");
        registry.register("project-a", &project_a).unwrap();

        let error = registry
            .resolve("project-a", &project_b)
            .expect_err("a registered id must not authorize another root");

        assert_eq!(error.code, PROJECT_CONTEXT_MISMATCH);
        fs::remove_dir_all(project_a).unwrap();
        fs::remove_dir_all(project_b).unwrap();
    }

    #[test]
    fn unknown_project_id_is_rejected() {
        let registry = ProjectRegistry::default();
        let project = temp_project("unknown");

        let error = registry
            .resolve("not-registered", &project)
            .expect_err("unknown ids must not create contexts");

        assert_eq!(error.code, PROJECT_CONTEXT_MISMATCH);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn registering_another_opened_project_preserves_both_path_bindings() {
        let registry = ProjectRegistry::default();
        let project_a = temp_project("active-a");
        let project_b = temp_project("active-b");
        registry.register("project-a", &project_a).unwrap();
        registry.register("project-b", &project_b).unwrap();

        registry
            .resolve("project-a", &project_a)
            .expect("background tasks and confirmations keep their project context");
        registry
            .resolve("project-b", &project_b)
            .expect("the newly opened project has its own project context");
        fs::remove_dir_all(project_a).unwrap();
        fs::remove_dir_all(project_b).unwrap();
    }

    #[test]
    fn matching_normalized_root_resolves_and_preserves_cjk() {
        let registry = ProjectRegistry::default();
        let project = temp_project("中文资料库");
        registry.register("project-cjk", &project).unwrap();

        let context = registry
            .resolve("project-cjk", &project.join("."))
            .expect("the normalized canonical root should match");

        assert_eq!(context.project_id, "project-cjk");
        assert_eq!(context.root, project.canonicalize().unwrap());
        assert!(context.root.to_string_lossy().contains("中文资料库"));
        fs::remove_dir_all(project).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn project_execution_owner_folds_case_on_windows() {
        assert_eq!(
            super::project_execution_owner(std::path::Path::new(r"C:\Knowledge\CaseRoot")),
            "project-execution:c:/knowledge/caseroot"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn project_execution_owner_preserves_case_on_case_sensitive_platforms() {
        assert_ne!(
            super::project_execution_owner(std::path::Path::new("/tmp/Knowledge/CaseRoot")),
            super::project_execution_owner(std::path::Path::new("/tmp/knowledge/caseroot"))
        );
    }

    #[test]
    fn path_registration_alone_never_grants_trust() {
        let registry = ProjectRegistry::default();
        let project = strict_native_project("registered-only");
        registry.register("project-a", &project).unwrap();

        let authority = registry.resolve_authority("project-a", &project).unwrap();

        assert_eq!(authority.trust, ProjectTrustAuthority::Untrusted);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn trusted_native_registration_requires_the_strict_backend_layout() {
        let registry = ProjectRegistry::default();
        let project = temp_project("compatible");
        fs::create_dir_all(project.join(".obsidian")).unwrap();

        let error = registry
            .register_trusted_native("project-a", &project)
            .expect_err("a compatible marker must not grant native trust");

        assert_eq!(error.code, "PROJECT_TRUST_AUTHORITY_INVALID");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn explicit_native_relocation_rebinds_only_the_matching_registered_root() {
        let registry = ProjectRegistry::default();
        let old_root = strict_native_project("relocate-old");
        let new_root = strict_native_project("relocate-new");
        registry
            .register_trusted_native("native-project", &old_root)
            .unwrap();

        let context = registry
            .relocate_trusted_native("native-project", &old_root, &new_root, || Ok(()))
            .expect("the verified native project may be rebound after explicit relocation");

        assert_eq!(context.root, new_root.canonicalize().unwrap());
        assert!(registry.resolve("native-project", &old_root).is_err());
        let authority = registry
            .resolve_authority("native-project", &new_root)
            .unwrap();
        assert_eq!(authority.trust, ProjectTrustAuthority::TrustedNative);

        fs::remove_dir_all(old_root).ok();
        fs::remove_dir_all(new_root).ok();
    }

    #[test]
    fn failed_recent_commit_keeps_the_registered_native_root_unchanged() {
        let registry = ProjectRegistry::default();
        let old_root = strict_native_project("relocate-rollback-old");
        let new_root = strict_native_project("relocate-rollback-new");
        registry
            .register_trusted_native("native-project", &old_root)
            .unwrap();

        let error = registry
            .relocate_trusted_native("native-project", &old_root, &new_root, || {
                Err(crate::errors::BackendError::new(
                    "RECENT_WRITE_FAILED",
                    "Simulated recent write failure.",
                    true,
                    false,
                ))
            })
            .expect_err("a failed recent write must not rebind authority");

        assert_eq!(error.code, "RECENT_WRITE_FAILED");
        assert!(registry.resolve("native-project", &old_root).is_ok());
        assert!(registry.resolve("native-project", &new_root).is_err());

        fs::remove_dir_all(old_root).ok();
        fs::remove_dir_all(new_root).ok();
    }

    #[test]
    fn trusted_compatible_registration_accepts_verified_compatible_layout() {
        let registry = ProjectRegistry::default();
        let project = compatible_project("trusted-compatible");

        registry
            .register_trusted_compatible("project-a", &project)
            .expect("a verified compatible layout should have an independent trust path");

        let authority = registry.resolve_authority("project-a", &project).unwrap();
        assert_eq!(authority.trust, ProjectTrustAuthority::TrustedCompatible);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn low_confidence_compatible_layout_cannot_be_trusted() {
        let registry = ProjectRegistry::default();
        let project = temp_project("low-confidence");
        fs::write(project.join("note.md"), "# Note").unwrap();

        let error = registry
            .register_trusted_compatible("project-a", &project)
            .expect_err("low-confidence Markdown must require a later assessment decision");

        assert_eq!(error.code, "PROJECT_TRUST_AUTHORITY_INVALID");
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn compatible_authority_revocation_rotates_epoch_and_cannot_self_restore() {
        let registry = ProjectRegistry::default();
        let project = compatible_project("compatible-revocation");
        registry
            .register_trusted_compatible("project-a", &project)
            .unwrap();
        let trusted = registry.resolve_authority("project-a", &project).unwrap();

        fs::remove_dir_all(project.join(".obsidian")).unwrap();
        fs::remove_file(project.join("index.md")).unwrap();
        let revoked = registry.resolve_authority("project-a", &project).unwrap();
        assert_eq!(revoked.trust, ProjectTrustAuthority::Untrusted);
        assert_ne!(revoked.authority_revision, trusted.authority_revision);

        fs::create_dir_all(project.join(".obsidian")).unwrap();
        fs::write(project.join("index.md"), "# Restored").unwrap();
        let restored_layout = registry.resolve_authority("project-a", &project).unwrap();
        assert_eq!(restored_layout.trust, ProjectTrustAuthority::Untrusted);
        assert_eq!(
            restored_layout.authority_revision,
            revoked.authority_revision
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn replacing_a_compatible_root_identity_permanently_revokes_runtime_trust() {
        let registry = ProjectRegistry::default();
        let project = compatible_project("compatible-identity-replacement");
        let displaced = project.with_extension("displaced");
        registry
            .register_trusted_compatible("project-a", &project)
            .unwrap();
        let trusted = registry.resolve_authority("project-a", &project).unwrap();

        fs::rename(&project, &displaced).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::create_dir_all(project.join(".obsidian")).unwrap();
        fs::write(project.join("index.md"), "# Replacement").unwrap();
        let revoked = registry.resolve_authority("project-a", &project).unwrap();
        assert_eq!(revoked.trust, ProjectTrustAuthority::Untrusted);
        assert_ne!(revoked.authority_revision, trusted.authority_revision);

        fs::remove_dir_all(&project).unwrap();
        fs::rename(&displaced, &project).unwrap();
        let original_returned = registry.resolve_authority("project-a", &project).unwrap();
        assert_eq!(original_returned.trust, ProjectTrustAuthority::Untrusted);
        assert_eq!(
            original_returned.authority_revision,
            revoked.authority_revision
        );
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn compatible_trust_restores_from_global_settings_for_the_same_identity() {
        let config = temp_project("trust-config");
        let project = compatible_project("持久化兼容库");
        let first = AppState {
            project_service: ProjectService::with_config_dir(config.clone()),
            ..AppState::default()
        };
        first
            .project_registry
            .register("project-a", &project)
            .unwrap();
        first
            .grant_compatible_project_trust("project-a", &project)
            .unwrap();

        let reopened = AppState {
            project_service: ProjectService::with_config_dir(config.clone()),
            ..AppState::default()
        };
        reopened
            .register_opened_project_authority("project-b", &project)
            .unwrap();
        let authority = reopened
            .project_registry
            .resolve_authority("project-b", &project)
            .unwrap();

        assert_eq!(authority.trust, ProjectTrustAuthority::TrustedCompatible);
        assert_eq!(authority.trust_kind, Some(ProjectTrustKind::Compatible));
        assert!(!project.join(".app").exists());
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn failed_durable_grant_never_publishes_runtime_compatible_authority() {
        let (state, config) = state_with_temp_config("failed-grant-config");
        let project = compatible_project("failed-grant");
        state
            .project_registry
            .register("project-a", &project)
            .unwrap();
        fs::write(config.join("project-trust.json"), "{not-json").unwrap();

        let error = state
            .grant_compatible_project_trust("project-a", &project)
            .expect_err("corrupt durable state must fail before runtime trust is published");

        assert_eq!(error.code, "PROJECT_TRUST_STORE_CORRUPT");
        let authority = state
            .project_registry
            .resolve_authority("project-a", &project)
            .unwrap();
        assert_eq!(authority.trust, ProjectTrustAuthority::Untrusted);
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn failed_durable_revoke_still_revokes_runtime_authority() {
        let (state, config) = state_with_temp_config("failed-revoke-config");
        let project = compatible_project("failed-revoke");
        state
            .project_registry
            .register("project-a", &project)
            .unwrap();
        state
            .grant_compatible_project_trust("project-a", &project)
            .unwrap();
        fs::write(config.join("project-trust.json"), "{not-json").unwrap();

        let error = state
            .revoke_project_trust("project-a", &project)
            .expect_err("corrupt durable state should be reported");

        assert_eq!(error.code, "PROJECT_TRUST_STORE_CORRUPT");
        let authority = state
            .project_registry
            .resolve_authority("project-a", &project)
            .unwrap();
        assert_eq!(authority.trust, ProjectTrustAuthority::Untrusted);
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn another_instance_observes_durable_compatible_revocation_before_workflow_access() {
        let config = temp_project("cross-instance-revoke-config");
        let project = compatible_project("cross-instance-revoke");
        let first = AppState {
            project_service: ProjectService::with_config_dir(config.clone()),
            ..AppState::default()
        };
        let second = AppState {
            project_service: ProjectService::with_config_dir(config.clone()),
            ..AppState::default()
        };
        first
            .project_registry
            .register("project-a", &project)
            .unwrap();
        first
            .grant_compatible_project_trust("project-a", &project)
            .unwrap();
        let second_context = second
            .register_opened_project_authority("project-b", &project)
            .unwrap();
        assert_eq!(
            second
                .project_registry
                .resolve_authority("project-b", &project)
                .unwrap()
                .trust,
            ProjectTrustAuthority::TrustedCompatible
        );

        first.revoke_project_trust("project-a", &project).unwrap();
        let access = second.resolve_workflow_access(&second_context).unwrap();

        assert_eq!(access.trust, WorkflowProjectTrust::Untrusted);
        assert_eq!(access.filesystem_access, WorkflowFilesystemAccess::ReadOnly);
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn compatible_access_is_writable_but_memory_only_without_a_state_root() {
        let (state, config) = state_with_temp_config("compatible-memory-config");
        let project = compatible_project("compatible-memory");
        state
            .project_registry
            .register("project-a", &project)
            .unwrap();
        let context = state
            .grant_compatible_project_trust("project-a", &project)
            .unwrap();

        let access = state.resolve_workflow_access(&context).unwrap();

        assert_eq!(access.trust, WorkflowProjectTrust::Trusted);
        assert_eq!(access.trust_kind, Some(ProjectTrustKind::Compatible));
        assert_eq!(access.filesystem_access, WorkflowFilesystemAccess::Writable);
        assert_eq!(access.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert!(!project.join(".app").exists());
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn enabled_compatible_state_persists_workflows_without_content_write_roots() {
        let (state, config) = state_with_temp_config("compatible-state-root-config");
        let project = compatible_project("compatible-state-root");
        state
            .project_registry
            .register("project-a", &project)
            .unwrap();
        let restricted = state
            .project_registry
            .resolve("project-a", &project)
            .unwrap();
        state
            .project_service
            .enable_compatible_guidance(&restricted, ProjectTemplate::General)
            .unwrap();

        let context = state
            .grant_compatible_project_trust("project-a", &project)
            .unwrap();
        let access = state.resolve_workflow_access(&context).unwrap();

        assert_eq!(access.persistence, WorkflowPersistenceMode::Persistent);
        assert!(project.join(".app/compat/tasks").is_dir());
        assert!(project.join(".app/compat/workflows").is_dir());
        assert!(context.layout.wiki_write_root.is_none());
        assert!(context.layout.export_root.is_none());
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn explicit_native_revoke_rebinds_every_existing_workflow_to_memory_only() {
        let (state, config) = state_with_temp_config("native-rebind-config");
        let state = Arc::new(state);
        let project = strict_native_project("native-rebind");
        let context = state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let mut task_ids = Vec::new();
        for revision in ["prep-1", "prep-2"] {
            let outcome = state
                .workflow_service
                .coordinator
                .enqueue(
                    &state.task_service,
                    EnqueueWorkflow {
                        project_id: "project-a".into(),
                        project_root: project.clone(),
                        task_state_root: Some(project.join(".app/tasks")),
                        title: "Health Check".into(),
                        kind: WorkflowKind::HealthCheck,
                        scope: WorkflowScope::HealthCheck {
                            mode: HealthCheckMode::LocalQuick,
                        },
                        route: Some(WorkflowRoute::Local {
                            route_revision: "local-v1".into(),
                        }),
                        baseline_fingerprint: revision.into(),
                        execution_options: WorkflowExecutionOptions {
                            preparation_revision: revision.into(),
                            ..WorkflowExecutionOptions::default()
                        },
                        stages: workflow_stages(&WorkflowKind::HealthCheck),
                        retry: None,
                    },
                )
                .unwrap();
            let run = match outcome {
                WorkflowStartOutcome::Created { run } => run,
                WorkflowStartOutcome::Existing { .. } => panic!("workflow must be unique"),
            };
            task_ids.push(run.task_id);
        }

        let active = state.task_service.get_workflow_run(&task_ids[0]).unwrap();
        let report_publication = state
            .publish_workflow_persistent_report(&context, &active)
            .unwrap()
            .expect("the persistent Local Quick run must publish a disk-write epoch")
            .begin()
            .unwrap();
        let revoke_state = Arc::clone(&state);
        let revoke_project = project.clone();
        let (revoked_tx, revoked_rx) = mpsc::channel();
        let revoke_worker = std::thread::spawn(move || {
            revoke_state
                .revoke_project_trust("project-a", &revoke_project)
                .unwrap();
            revoked_tx.send(()).unwrap();
        });
        assert!(
            revoked_rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "revoke must not return while a persistent report write can still begin"
        );
        report_publication.started();
        revoked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        revoke_worker.join().unwrap();

        for (index, task_id) in task_ids.into_iter().enumerate() {
            let run = state.task_service.get_workflow_run(&task_id).unwrap();
            assert_eq!(run.persistence, WorkflowPersistenceMode::MemoryOnly);
            assert_eq!(
                run.persistence_transition,
                Some(WorkflowPersistenceTransition::DowngradedToMemoryOnly)
            );
            if index == 0 {
                assert_eq!(
                    run.display_status,
                    crate::models::workflow::WorkflowDisplayStatus::Running
                );
                assert!(
                    state
                        .publish_workflow_persistent_report(&context, &run)
                        .unwrap()
                        .is_none(),
                    "a revoked Local Quick run that was rebound successfully must use memory"
                );
            } else {
                assert_eq!(
                    run.display_status,
                    crate::models::workflow::WorkflowDisplayStatus::Queued
                );
                assert!(run.continuation_required);
            }
        }
        assert!(!project.join(".app/lint-reports").exists());
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn revoked_authority_cannot_reopen_a_lingering_local_quick_report_epoch() {
        let (state, config) = state_with_temp_config("report-rebind-failure-config");
        let project = strict_native_project("report-rebind-failure");
        let context = state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let run = match state
            .workflow_service
            .coordinator
            .enqueue(
                &state.task_service,
                EnqueueWorkflow {
                    project_id: "project-a".into(),
                    project_root: project.clone(),
                    task_state_root: Some(project.join(".app/tasks")),
                    title: "Health Check".into(),
                    kind: WorkflowKind::HealthCheck,
                    scope: WorkflowScope::HealthCheck {
                        mode: HealthCheckMode::LocalQuick,
                    },
                    route: Some(WorkflowRoute::Local {
                        route_revision: "local-v1".into(),
                    }),
                    baseline_fingerprint: "report-rebind-failure".into(),
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision: "report-rebind-failure".into(),
                        ..WorkflowExecutionOptions::default()
                    },
                    stages: workflow_stages(&WorkflowKind::HealthCheck),
                    retry: None,
                },
            )
            .unwrap()
        {
            WorkflowStartOutcome::Created { run } => run,
            WorkflowStartOutcome::Existing { .. } => panic!("workflow must be unique"),
        };
        assert!(state
            .task_service
            .workflow_persistence_dir(&run.task_id)
            .is_some());

        // Model the fail-closed state after trust authority was revoked but a
        // persistence rebind failed and left the old directory attached.
        state
            .project_registry
            .revoke_trust("project-a", &project)
            .unwrap();
        state
            .project_service
            .revoke_project_trust(&project)
            .unwrap();
        assert!(state
            .task_service
            .workflow_persistence_dir(&run.task_id)
            .is_some());

        let error = match state.publish_workflow_persistent_report(&context, &run) {
            Err(error) => error,
            Ok(_) => panic!("revoked authority must not reopen persistent report publication"),
        };
        assert_eq!(error.code, "WORKFLOW_REPORT_AUTHORITY_CHANGED");
        assert!(!project.join(".app/lint-reports").exists());
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn claimed_snapshot_is_rejected_after_real_trust_revocation() {
        fn enqueue(state: &AppState, project: &std::path::Path, revision: &str) -> WorkflowRun {
            let outcome = state
                .workflow_service
                .coordinator
                .enqueue(
                    &state.task_service,
                    EnqueueWorkflow {
                        project_id: "project-a".into(),
                        project_root: project.to_path_buf(),
                        task_state_root: Some(project.join(".app/tasks")),
                        title: "Update Wiki".into(),
                        kind: WorkflowKind::UpdateWiki,
                        scope: WorkflowScope::UpdateWiki {
                            mode: crate::models::workflow::UpdateWikiMode::ChangedSources,
                            source_versions: Vec::new(),
                        },
                        route: Some(WorkflowRoute::Agent {
                            agent: crate::models::agent::AgentKind::Codex,
                            model: Some("test-model".into()),
                            route_revision: "test-agent-route".into(),
                        }),
                        baseline_fingerprint: revision.into(),
                        execution_options: WorkflowExecutionOptions {
                            preparation_revision: revision.into(),
                            ..WorkflowExecutionOptions::default()
                        },
                        stages: vec![WorkflowStage {
                            id: "prepare".into(),
                            ordinal: 1,
                            status: WorkflowStageStatus::Pending,
                            label_key: "prepare".into(),
                            started_at: None,
                            completed_at: None,
                            current_item: None,
                            progress: None,
                            decision: None,
                        }],
                        retry: None,
                    },
                )
                .unwrap();
            match outcome {
                WorkflowStartOutcome::Created { run } => run,
                WorkflowStartOutcome::Existing { .. } => panic!("workflow must be unique"),
            }
        }

        let (state, config) = state_with_temp_config("dispatch-after-revoke-config");
        let state = Arc::new(state);
        let project = strict_native_project("dispatch-after-revoke");
        state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let runner = Arc::new(CountingWorkflowRunner::default());
        state
            .workflow_service
            .register_runner(runner.clone())
            .unwrap();
        let active = enqueue(&state, &project, "active-revision");
        let queued = enqueue(&state, &project, "queued-revision");
        state
            .task_service
            .start_workflow_stage(&active.task_id, "prepare")
            .unwrap();
        state
            .task_service
            .complete_workflow_stage(&active.task_id, "prepare")
            .unwrap();
        let (claimed_tx, claimed_rx) = mpsc::channel();
        let (dispatch_tx, dispatch_rx) = mpsc::channel();
        let worker_state = Arc::clone(&state);
        let active_task_id = active.task_id.clone();
        let worker = std::thread::spawn(move || {
            let (_, claimed) = worker_state
                .workflow_service
                .coordinator
                .complete_and_claim_next(
                    &worker_state.task_service,
                    &active_task_id,
                    WorkflowResult::UpdateWiki {
                        created: 0,
                        updated: 0,
                        skipped: 0,
                        deleted: 0,
                        conflicted: 0,
                        affected_paths: Vec::new(),
                        checkpoint_hash: None,
                        final_commit: None,
                    },
                )
                .unwrap();
            let stale_claimed = claimed.expect("the queued workflow must be claimed");
            claimed_tx.send(stale_claimed.clone()).unwrap();
            dispatch_rx.recv().unwrap();
            worker_state
                .workflow_service
                .dispatch_claimed_run_with_settings(
                    &worker_state.task_service,
                    &worker_state.settings_service,
                    &stale_claimed,
                )
                .unwrap()
        });
        let stale_claimed = claimed_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("timed out waiting for a real queue claim");
        assert_eq!(stale_claimed.task_id, queued.task_id);

        state.revoke_project_trust("project-a", &project).unwrap();
        assert_eq!(
            state
                .project_registry
                .resolve_authority("project-a", &project)
                .unwrap()
                .trust,
            ProjectTrustAuthority::Untrusted,
        );
        dispatch_tx.send(()).unwrap();
        assert!(!worker.join().unwrap());

        assert_eq!(runner.0.load(Ordering::SeqCst), 0);
        let current = state
            .task_service
            .get_workflow_run(&queued.task_id)
            .unwrap();
        assert_eq!(current.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(
            state.task_service.get_task(&queued.task_id).unwrap().status,
            TaskStatus::Cancelled,
            "a stale claimed snapshot must be rejected after real trust revocation",
        );
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn trust_revocation_freezes_queued_work_before_stopping_mutating_active_run() {
        let (state, config) = state_with_temp_config("revoke-freezes-queue-config");
        let project = strict_native_project("revoke-freezes-queue");
        state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let mut runs = Vec::new();
        for revision in ["active", "queued-1", "queued-2"] {
            let outcome = state
                .workflow_service
                .coordinator
                .enqueue(
                    &state.task_service,
                    EnqueueWorkflow {
                        project_id: "project-a".into(),
                        project_root: project.clone(),
                        task_state_root: Some(project.join(".app/tasks")),
                        title: "Update Wiki".into(),
                        kind: WorkflowKind::UpdateWiki,
                        scope: WorkflowScope::UpdateWiki {
                            mode: crate::models::workflow::UpdateWikiMode::ChangedSources,
                            source_versions: Vec::new(),
                        },
                        route: Some(WorkflowRoute::Agent {
                            agent: crate::models::agent::AgentKind::Codex,
                            model: None,
                            route_revision: "agent-v1".into(),
                        }),
                        baseline_fingerprint: revision.into(),
                        execution_options: WorkflowExecutionOptions {
                            preparation_revision: revision.into(),
                            ..WorkflowExecutionOptions::default()
                        },
                        stages: workflow_stages(&WorkflowKind::UpdateWiki),
                        retry: None,
                    },
                )
                .unwrap();
            let WorkflowStartOutcome::Created { run } = outcome else {
                panic!("workflow must be unique")
            };
            runs.push(run);
        }

        state.revoke_project_trust("project-a", &project).unwrap();

        assert_eq!(
            state
                .task_service
                .get_task(&runs[0].task_id)
                .unwrap()
                .status,
            TaskStatus::Cancelling
        );
        let (_, claimed) = state
            .workflow_service
            .coordinator
            .reject_claimed_dispatch(
                &state.task_service,
                &runs[0].task_id,
                crate::services::WorkflowDispatchFailure::stale(
                    "WORKFLOW_PROJECT_UNTRUSTED",
                    "workflows.error.prepareAgain",
                ),
            )
            .unwrap();
        assert!(claimed.is_none(), "the suspended queue must not auto-claim");
        assert_eq!(
            state
                .task_service
                .get_task(&runs[0].task_id)
                .unwrap()
                .status,
            TaskStatus::Cancelled
        );
        for queued in &runs[1..] {
            let current = state
                .task_service
                .get_workflow_run(&queued.task_id)
                .unwrap();
            assert_eq!(
                current.display_status,
                crate::models::workflow::WorkflowDisplayStatus::Queued
            );
            assert!(current.continuation_required);
        }

        state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        for queued in &runs[1..] {
            let current = state
                .task_service
                .get_workflow_run(&queued.task_id)
                .unwrap();
            assert_eq!(
                current.display_status,
                crate::models::workflow::WorkflowDisplayStatus::Queued
            );
            assert!(
                current.continuation_required,
                "trust restoration must not auto-run queued work"
            );
        }
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn trust_revocation_removes_waiting_confirmation_authority_before_returning() {
        let (state, config) = state_with_temp_config("revoke-waiting-confirmation-config");
        let project = strict_native_project("revoke-waiting-confirmation");
        let context = state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let outcome = state
            .workflow_service
            .coordinator
            .enqueue(
                &state.task_service,
                EnqueueWorkflow {
                    project_id: context.project_id.clone(),
                    project_root: context.root.clone(),
                    task_state_root: Some(context.root.join(".app/tasks")),
                    title: "Update Wiki".into(),
                    kind: WorkflowKind::UpdateWiki,
                    scope: WorkflowScope::UpdateWiki {
                        mode: crate::models::workflow::UpdateWikiMode::ChangedSources,
                        source_versions: Vec::new(),
                    },
                    route: None,
                    baseline_fingerprint: "waiting".into(),
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision: "waiting".into(),
                        ..WorkflowExecutionOptions::default()
                    },
                    stages: workflow_stages(&WorkflowKind::UpdateWiki),
                    retry: None,
                },
            )
            .unwrap();
        let WorkflowStartOutcome::Created { run } = outcome else {
            panic!("workflow must be unique")
        };
        let stage_id = run.stages[0].id.clone();
        let action_id = "waiting-action".to_string();
        let candidate = WorkflowCandidateReference::TaskOwned {
            candidate_id: run.task_id.clone(),
        };
        let pending = WorkflowPendingAction {
            id: action_id.clone(),
            action_type: PendingActionType::MergeConflict,
            risk_level: RiskLevel::High,
            affected_paths: vec!["wiki/index.md".into()],
            candidate: Some(candidate.clone()),
            expires_at: None,
            checkpoint_hash: Some("checkpoint".into()),
        };
        state
            .task_service
            .start_workflow_stage(&run.task_id, &stage_id)
            .unwrap();
        state
            .task_service
            .wait_workflow_stage(&run.task_id, &stage_id, pending.clone())
            .unwrap();
        state
            .confirmation_registry
            .register_with_execution(
                PendingAction {
                    id: action_id.clone(),
                    action_type: pending.action_type.clone(),
                    title: "Review".into(),
                    message: "Review".into(),
                    risk_level: pending.risk_level.clone(),
                    affected_paths: pending.affected_paths.clone(),
                    preview: None,
                    expires_at: None,
                    checkpoint_hash: pending.checkpoint_hash.clone(),
                },
                Some(ConfirmationExecution::UpdateWikiReview {
                    project_id: context.project_id.clone(),
                    root_path: context.root.to_string_lossy().into_owned(),
                    canonical_identity_key: run.canonical_identity_key.clone(),
                    identity_revision: run.identity_revision.clone(),
                    task_id: run.task_id.clone(),
                    action_id: action_id.clone(),
                    candidate,
                }),
            )
            .unwrap();

        state.revoke_project_trust("project-a", &project).unwrap();

        let current = state.task_service.get_workflow_run(&run.task_id).unwrap();
        assert_eq!(
            current.display_status,
            crate::models::workflow::WorkflowDisplayStatus::Cancelled
        );
        assert!(current.pending_action.is_none());
        assert_eq!(
            state
                .confirmation_registry
                .peek(&action_id)
                .unwrap_err()
                .code,
            "CONFIRMATION_NOT_FOUND"
        );
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn untrusted_access_is_read_only_and_never_probes_or_creates_app_state() {
        let (state, config) = state_with_temp_config("untrusted-no-write-config");
        let project = compatible_project("untrusted-no-write");
        let context = state
            .project_registry
            .register("project-a", &project)
            .unwrap();
        let before = fs::read_dir(&project)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();

        let access = state.resolve_workflow_access(&context).unwrap();

        let after = fs::read_dir(&project)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(access.trust, WorkflowProjectTrust::Untrusted);
        assert_eq!(access.trust_kind, None);
        assert_eq!(access.filesystem_access, WorkflowFilesystemAccess::ReadOnly);
        assert_eq!(access.persistence, WorkflowPersistenceMode::MemoryOnly);
        assert_eq!(after, before);
        assert!(!project.join(".app").exists());
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn project_task_mutation_permit_keeps_restricted_projects_memory_only() {
        let (state, config) = state_with_temp_config("task-permit-restricted-config");
        let project = compatible_project("task-permit-restricted");
        state
            .project_registry
            .register("project-a", &project)
            .unwrap();
        let before = fs::read_dir(&project)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();

        state
            .with_current_project_task_access(
                "project-a",
                project.to_string_lossy().as_ref(),
                |permit| {
                    assert_eq!(permit.context().project_id, "project-a");
                    let access = permit.workflow_access();
                    assert_eq!(access.trust, WorkflowProjectTrust::Untrusted);
                    assert_eq!(access.persistence, WorkflowPersistenceMode::MemoryOnly);
                    Ok(())
                },
            )
            .unwrap();

        let after = fs::read_dir(&project)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(after, before);
        assert!(!project.join(".app").exists());
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn task_permit_cancels_in_memory_after_trusted_project_becomes_read_only() {
        let (state, config) = state_with_temp_config("task-permit-read-only-config");
        let project = strict_native_project("task-permit-read-only");
        state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let task = state
            .with_current_project_write_access(
                "project-a",
                project.to_string_lossy().as_ref(),
                |_permit, context| {
                    state
                        .task_service
                        .create_project_task(
                            TaskType::Import,
                            "project-a".into(),
                            context.root.clone(),
                            "Cancelable import".into(),
                            true,
                        )
                        .map_err(|message| {
                            BackendError::new("TASK_CREATE_FAILED", message, false, false)
                        })
                },
            )
            .unwrap();
        let persisted = project.join(format!(".app/tasks/{}.json", task.id));
        let before = fs::read(&persisted).unwrap();
        state.project_service.force_read_only_for_test(&project);

        state
            .with_current_project_task_access(
                "project-a",
                project.to_string_lossy().as_ref(),
                |permit| {
                    assert_eq!(
                        permit.workflow_access().persistence,
                        WorkflowPersistenceMode::MemoryOnly
                    );
                    state
                        .task_service
                        .request_cancel(&task.id)
                        .map_err(|message| {
                            BackendError::new("TASK_CANCEL_FAILED", message, false, false)
                        })?;
                    Ok(())
                },
            )
            .unwrap();

        assert!(state.task_service.is_cancelled(&task.id));
        assert_eq!(fs::read(persisted).unwrap(), before);
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn external_ai_access_requires_current_project_trust() {
        let (state, config) = state_with_temp_config("external-ai-trust-config");
        let project = compatible_project("external-ai-trust");
        let context = state
            .project_registry
            .register("project-a", &project)
            .unwrap();

        let error = state.require_external_ai_access(&context).unwrap_err();
        assert_eq!(error.code, "PROJECT_EXTERNAL_AI_REQUIRES_TRUST");
        assert!(!project.join(".app").exists());

        let trusted = state
            .grant_compatible_project_trust("project-a", &project)
            .unwrap();
        state.require_external_ai_access(&trusted).unwrap();
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn recovery_state_revokes_external_ai_access_even_for_a_trusted_native_project() {
        let (state, config) = state_with_temp_config("external-ai-recovery-config");
        let project = strict_native_project("external-ai-recovery");
        let context = state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        fs::write(project.join(".app/graph-cache.json"), "{ invalid").unwrap();

        let error = state.require_external_ai_access(&context).unwrap_err();
        assert_eq!(error.code, "PROJECT_EXTERNAL_AI_REQUIRES_TRUST");
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn project_writes_require_trust_writable_health_and_a_specific_content_root() {
        let (state, config) = state_with_temp_config("project-write-access-config");
        let compatible = compatible_project("project-write-compatible");
        let untrusted = state
            .project_registry
            .register("project-a", &compatible)
            .unwrap();
        assert_eq!(
            state
                .require_project_write_access(&untrusted)
                .unwrap_err()
                .code,
            "PROJECT_WRITE_REQUIRES_TRUST"
        );

        let trusted_compatible = state
            .grant_compatible_project_trust("project-a", &compatible)
            .unwrap();
        state
            .require_project_write_access(&trusted_compatible)
            .unwrap();
        let root_error = state
            .require_project_content_write_root(&trusted_compatible, ProjectWriteRootKind::Wiki)
            .unwrap_err();
        assert_eq!(root_error.code, "PROJECT_LAYOUT_ROOT_UNAVAILABLE");
        assert_eq!(
            root_error
                .details
                .as_ref()
                .and_then(|details| details.get("rootKind"))
                .and_then(|value| value.as_str()),
            Some("wiki")
        );

        let native = strict_native_project("project-write-native");
        let trusted_native = state
            .project_registry
            .register_trusted_native("project-b", &native)
            .unwrap();
        state.require_project_write_access(&trusted_native).unwrap();
        cleanup_paths(&[&compatible, &native, &config]);
    }

    #[test]
    fn unreadable_native_project_keeps_general_workflow_state_but_cannot_execute_or_write() {
        let (state, config) = state_with_temp_config("unreadable-native-access-config");
        let project = strict_native_project("unreadable-native-access");
        fs::remove_file(project.join("wiki/index.md")).unwrap();
        let context = state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();

        let access = state.resolve_workflow_access(&context).unwrap();
        assert_eq!(access.trust, WorkflowProjectTrust::Trusted);
        assert_eq!(access.persistence, WorkflowPersistenceMode::Persistent);
        assert_eq!(
            state.require_external_ai_access(&context).unwrap_err().code,
            "PROJECT_EXTERNAL_AI_REQUIRES_TRUST"
        );
        assert_eq!(
            state
                .require_project_write_access(&context)
                .unwrap_err()
                .code,
            "PROJECT_WRITE_STATE_UNAVAILABLE"
        );
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn trusted_native_authority_revocation_rotates_epoch_and_cannot_self_restore() {
        let registry = ProjectRegistry::default();
        let project = strict_native_project("trusted-native");
        registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let trusted = registry.resolve_authority("project-a", &project).unwrap();
        assert_eq!(trusted.trust, ProjectTrustAuthority::TrustedNative);

        fs::remove_dir_all(project.join(".app").join("tasks")).unwrap();
        let revoked = registry.resolve_authority("project-a", &project).unwrap();
        assert_eq!(revoked.trust, ProjectTrustAuthority::Untrusted);
        assert_ne!(revoked.authority_revision, trusted.authority_revision);

        fs::create_dir_all(project.join(".app").join("tasks")).unwrap();
        let restored_layout = registry.resolve_authority("project-a", &project).unwrap();
        assert_eq!(restored_layout.trust, ProjectTrustAuthority::Untrusted);
        assert_eq!(
            restored_layout.authority_revision,
            revoked.authority_revision
        );

        registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let reassessed = registry.resolve_authority("project-a", &project).unwrap();
        assert_eq!(reassessed.trust, ProjectTrustAuthority::TrustedNative);
        assert_ne!(reassessed.authority_revision, revoked.authority_revision);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn project_open_grants_strict_native_authority_without_initializing_git() {
        let (state, config) = state_with_temp_config("native-open-config");
        let project = strict_native_project("native-open");

        state
            .register_opened_project_authority("project-a", &project)
            .unwrap();

        let authority = state
            .project_registry
            .resolve_authority("project-a", &project)
            .unwrap();
        assert_eq!(authority.trust, ProjectTrustAuthority::TrustedNative);
        assert!(!project.join(".git").exists());
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn project_open_keeps_unapproved_compatible_vault_restricted_without_writes() {
        let (state, config) = state_with_temp_config("compatible-open-config");
        let project = compatible_project("compatible-open");

        state
            .register_opened_project_authority("project-a", &project)
            .unwrap();

        let authority = state
            .project_registry
            .resolve_authority("project-a", &project)
            .unwrap();
        assert_eq!(authority.trust, ProjectTrustAuthority::Untrusted);
        assert!(!project.join(".app").exists());
        assert!(!project.join(".git").exists());
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn app_state_resolves_trusted_native_workflow_access_without_inventing_git() {
        let state = AppState::default();
        let project = strict_native_project("workflow-access");
        let context = state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();

        let access = state.resolve_workflow_access(&context).unwrap();

        assert_eq!(access.trust, WorkflowProjectTrust::Trusted);
        assert_eq!(access.persistence, WorkflowPersistenceMode::Persistent);
        assert_eq!(access.git_state, WorkflowGitState::Unavailable);
        fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn workflow_access_critical_section_blocks_concurrent_trust_revocation() {
        let (state, config) = state_with_temp_config("workflow-access-transition-lock-config");
        let state = Arc::new(state);
        let project = strict_native_project("workflow-access-transition-lock");
        let context = state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_state = Arc::clone(&state);
        let worker_context = context.clone();
        let worker_entered = Arc::clone(&entered);
        let worker_release = Arc::clone(&release);
        let access_worker = std::thread::spawn(move || {
            worker_state
                .with_workflow_access(&worker_context, |access| {
                    assert_eq!(access.persistence, WorkflowPersistenceMode::Persistent);
                    worker_entered.wait();
                    worker_release.wait();
                    Ok(())
                })
                .unwrap();
        });
        entered.wait();

        let revoke_state = Arc::clone(&state);
        let revoke_project = project.clone();
        let (revoked_tx, revoked_rx) = mpsc::channel();
        let revoke_worker = std::thread::spawn(move || {
            revoke_state
                .revoke_project_trust("project-a", &revoke_project)
                .unwrap();
            revoked_tx.send(()).unwrap();
        });
        assert!(revoked_rx.recv_timeout(Duration::from_millis(100)).is_err());

        release.wait();
        access_worker.join().unwrap();
        revoked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        revoke_worker.join().unwrap();
        let access = state.resolve_workflow_access(&context).unwrap();
        assert_eq!(access.trust, WorkflowProjectTrust::Untrusted);
        cleanup_paths(&[&project, &config]);
    }

    #[test]
    fn project_write_critical_section_rejects_restricted_authority_and_blocks_revocation() {
        let (restricted_state, restricted_config) =
            state_with_temp_config("project-write-restricted-config");
        let restricted_project = compatible_project("project-write-restricted");
        restricted_state
            .project_registry
            .register("restricted", &restricted_project)
            .unwrap();
        let restricted_error = restricted_state
            .with_current_project_write_access(
                "restricted",
                restricted_project.to_string_lossy().as_ref(),
                |_, _| Ok(()),
            )
            .unwrap_err();
        assert_eq!(restricted_error.code, "PROJECT_WRITE_REQUIRES_TRUST");

        let (state, config) = state_with_temp_config("project-write-transition-config");
        let state = Arc::new(state);
        let project = strict_native_project("project-write-transition");
        state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_state = Arc::clone(&state);
        let worker_project = project.clone();
        let worker_entered = Arc::clone(&entered);
        let worker_release = Arc::clone(&release);
        let write_worker = std::thread::spawn(move || {
            worker_state
                .with_current_project_write_access(
                    "project-a",
                    worker_project.to_string_lossy().as_ref(),
                    |_, _| {
                        worker_entered.wait();
                        worker_release.wait();
                        Ok(())
                    },
                )
                .unwrap();
        });
        entered.wait();

        let revoke_state = Arc::clone(&state);
        let revoke_project = project.clone();
        let (revoked_tx, revoked_rx) = mpsc::channel();
        let revoke_worker = std::thread::spawn(move || {
            revoke_state
                .revoke_project_trust("project-a", &revoke_project)
                .unwrap();
            revoked_tx.send(()).unwrap();
        });
        assert!(revoked_rx.recv_timeout(Duration::from_millis(100)).is_err());

        release.wait();
        write_worker.join().unwrap();
        revoked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        revoke_worker.join().unwrap();
        let revoked_error = state
            .with_current_project_write_access(
                "project-a",
                project.to_string_lossy().as_ref(),
                |_, _| Ok(()),
            )
            .unwrap_err();
        assert_eq!(revoked_error.code, "PROJECT_WRITE_REQUIRES_TRUST");
        cleanup_paths(&[&restricted_project, &restricted_config, &project, &config]);
    }

    #[test]
    fn project_write_critical_sections_do_not_block_unrelated_roots() {
        let (state, config) = state_with_temp_config("project-write-lanes-config");
        let state = Arc::new(state);
        let project_a = strict_native_project("project-write-lane-a");
        let project_b = strict_native_project("project-write-lane-b");
        state
            .project_registry
            .register_trusted_native("project-a", &project_a)
            .unwrap();
        state
            .project_registry
            .register_trusted_native("project-b", &project_b)
            .unwrap();

        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let state_a = Arc::clone(&state);
        let root_a = project_a.clone();
        let worker_a = std::thread::spawn(move || {
            state_a
                .with_current_project_write_access(
                    "project-a",
                    root_a.to_string_lossy().as_ref(),
                    |_, _| {
                        entered_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        Ok(())
                    },
                )
                .unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(15)).unwrap();

        let (completed_tx, completed_rx) = mpsc::channel();
        let state_b = Arc::clone(&state);
        let root_b = project_b.clone();
        let worker_b = std::thread::spawn(move || {
            let result = state_b.with_current_project_write_access(
                "project-b",
                root_b.to_string_lossy().as_ref(),
                |_, _| Ok(()),
            );
            completed_tx.send(result).unwrap();
        });
        let project_b_result = completed_rx.recv_timeout(Duration::from_secs(15));
        release_tx.send(()).unwrap();
        worker_a.join().unwrap();
        worker_b.join().unwrap();

        project_b_result
            .expect("project B must not wait for project A's write lane")
            .unwrap();
        cleanup_paths(&[&project_a, &project_b, &config]);
    }

    #[test]
    fn project_execution_epoch_revocation_cancels_root_waits_and_unbinds_persistence() {
        let (state, config) = state_with_temp_config("project-execution-revoke-config");
        let state = Arc::new(state);
        let project_a = strict_native_project("project-execution-a");
        let project_b = strict_native_project("project-execution-b");
        let context_a = state
            .project_registry
            .register_trusted_native("project-a", &project_a)
            .unwrap();
        let context_b = state
            .project_registry
            .register_trusted_native("project-b", &project_b)
            .unwrap();
        let task_a = state
            .with_current_project_write_access(
                "project-a",
                project_a.to_string_lossy().as_ref(),
                |_permit, context| {
                    state
                        .task_service
                        .create_project_task(
                            TaskType::LlmRequest,
                            "project-a".into(),
                            context.root.clone(),
                            "A Chat".into(),
                            true,
                        )
                        .map_err(|message| BackendError::new("TASK_FAILED", message, true, false))
                },
            )
            .unwrap();
        state
            .task_service
            .transition_status(&task_a.id, TaskStatus::Running)
            .unwrap();
        let task_b = state
            .with_current_project_write_access(
                "project-b",
                project_b.to_string_lossy().as_ref(),
                |_permit, context| {
                    state
                        .task_service
                        .create_project_task(
                            TaskType::LlmRequest,
                            "project-b".into(),
                            context.root.clone(),
                            "B Chat".into(),
                            true,
                        )
                        .map_err(|message| BackendError::new("TASK_FAILED", message, true, false))
                },
            )
            .unwrap();
        state
            .task_service
            .transition_status(&task_b.id, TaskStatus::Running)
            .unwrap();
        let lease_a = state
            .begin_project_external_task(&context_a, &task_a.id)
            .unwrap();
        let snapshot_path = project_a
            .join(".app/tasks")
            .join(format!("{}.json", task_a.id));

        let revoke_state = Arc::clone(&state);
        let revoke_root = project_a.clone();
        let (revoked_tx, revoked_rx) = mpsc::channel();
        let revoke_worker = std::thread::spawn(move || {
            revoke_state
                .revoke_project_trust("project-a", &revoke_root)
                .unwrap();
            revoked_tx.send(()).unwrap();
        });

        assert!(revoked_rx.recv_timeout(Duration::from_millis(100)).is_err());
        assert!(state.task_service.is_cancelled(&task_a.id));
        assert!(!state.task_service.is_cancelled(&task_b.id));
        drop(lease_a);
        revoked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        revoke_worker.join().unwrap();

        let bytes_after_revoke = fs::read(&snapshot_path).unwrap();
        state
            .task_service
            .append_log(
                &task_a.id,
                crate::tasks::task_model::LogLevel::Warn,
                "late audit fact".into(),
            )
            .unwrap();
        assert_eq!(fs::read(&snapshot_path).unwrap(), bytes_after_revoke);
        assert!(state
            .begin_project_external_task(&context_a, &task_a.id)
            .is_err());
        let lease_b = state
            .begin_project_external_task(&context_b, &task_b.id)
            .unwrap();
        drop(lease_b);

        cleanup_paths(&[&project_a, &project_b, &config]);
    }

    #[test]
    fn non_task_external_request_drains_before_project_revocation_returns() {
        let (state, config) = state_with_temp_config("external-request-revoke-config");
        let state = Arc::new(state);
        let project = strict_native_project("external-request-revoke");
        let context = state
            .project_registry
            .register_trusted_native("project-a", &project)
            .unwrap();
        let request_count = Arc::new(AtomicUsize::new(0));
        let execution = state
            .begin_project_external_execution(&context, "provider-probe")
            .unwrap();
        request_count.fetch_add(1, Ordering::SeqCst);

        let revoke_state = Arc::clone(&state);
        let revoke_root = project.clone();
        let (revoked_tx, revoked_rx) = mpsc::channel();
        let revoke_worker = std::thread::spawn(move || {
            revoke_state
                .revoke_project_trust("project-a", &revoke_root)
                .unwrap();
            revoked_tx.send(()).unwrap();
        });

        assert!(revoked_rx.recv_timeout(Duration::from_millis(100)).is_err());
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
        drop(execution);
        revoked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        revoke_worker.join().unwrap();

        assert!(state
            .begin_project_external_execution(&context, "late-provider-probe")
            .is_err());
        assert_eq!(request_count.load(Ordering::SeqCst), 1);
        cleanup_paths(&[&project, &config]);
    }
}
