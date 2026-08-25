use std::collections::HashMap;
use std::sync::Arc;

use crate::errors::BackendError;
use crate::models::compile::{
    CompileCandidate, CompileConsumptionRecord, CompileFile, CompileManifest, CompileRoute,
    ResolvedCompileRoute, SourceVersionRef,
};
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, ConfirmationRegistry, PendingAction, PendingActionType,
    RiskLevel,
};
use crate::models::git::CheckpointPurpose;
use crate::models::paths::ProjectContext;
use crate::models::task::TaskStatus;
use crate::models::workflow::{
    UpdateWikiMode, WorkflowCandidateReference, WorkflowDecisionCounts, WorkflowDecisionReview,
    WorkflowErrorSummary, WorkflowFileDiff, WorkflowFileDiffKind, WorkflowFileDiffPage,
    WorkflowKind, WorkflowPendingAction, WorkflowPrerequisiteAction, WorkflowProjectMutationState,
    WorkflowResult, WorkflowRoute, WorkflowRun, WorkflowScope,
};
use crate::services::import_v2::source_registry::SourceRegistry;
use crate::services::{
    BookmarkService, CompileExecutionServices, CompileGenerationObserver, CompileGenerationPolicy,
    CompileService, FileStore, GitService, SearchService,
};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;
use crate::utils::private_directory::{create_private_directory, ensure_private_directory};
use serde::{Deserialize, Serialize};

use super::super::fingerprint::{canonical_json, hex_sha256};
use super::super::{
    WorkflowCoordinator, WorkflowExternalLaunchPermit, WorkflowRunner, WorkflowStageSink,
};

const ANALYZE_SOURCES: &str = "analyze_sources";
const CREATE_CHECKPOINT: &str = "create_checkpoint";
const PLAN_UPDATES: &str = "plan_updates";
const GENERATE_CANDIDATES: &str = "generate_candidates";
const VALIDATE_STRUCTURE: &str = "validate_structure";
const REVIEW_RISK: &str = "review_risk";
const APPLY_CHANGES: &str = "apply_changes";
const REFRESH_INDEXES: &str = "refresh_indexes";
const RECORD_RESULT: &str = "record_result";

type StartCallback = dyn Fn(WorkflowRun) + Send + Sync;

pub struct UpdateWikiRunner {
    start_callback: Arc<StartCallback>,
}

impl UpdateWikiRunner {
    pub fn new(callback: impl Fn(WorkflowRun) + Send + Sync + 'static) -> Self {
        Self {
            start_callback: Arc::new(callback),
        }
    }
}

impl WorkflowRunner for UpdateWikiRunner {
    fn kind(&self) -> WorkflowKind {
        WorkflowKind::UpdateWiki
    }

    fn start(&self, run: WorkflowRun) {
        (self.start_callback)(run);
    }
}

pub struct UpdateWikiExecutionServices<'a> {
    pub compile: CompileExecutionServices<'a>,
    pub git_service: &'a GitService,
    pub file_store: &'a FileStore,
    pub bookmark_service: &'a BookmarkService,
    pub search_service: &'a SearchService,
    pub confirmation_registry: &'a ConfirmationRegistry,
    pub coordinator: &'a WorkflowCoordinator,
}

pub async fn run_update_wiki(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &UpdateWikiExecutionServices<'_>,
) -> Option<WorkflowRun> {
    let permit = WorkflowExternalLaunchPermit::prevalidated(&run);
    run_update_wiki_authorized(context, run, services, || Ok(permit)).await
}

pub async fn run_update_wiki_authorized<F>(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &UpdateWikiExecutionServices<'_>,
    authorize_external_launch: F,
) -> Option<WorkflowRun>
where
    F: FnOnce() -> Result<WorkflowExternalLaunchPermit, BackendError>,
{
    match execute_update_wiki(context, &run, services, authorize_external_launch).await {
        Ok(UpdateWikiOutcome::Finished(next)) => next,
        Ok(UpdateWikiOutcome::Waiting | UpdateWikiOutcome::CommittedPendingReconciliation) => None,
        Err(error) => finish_error(&run, services, error),
    }
}

enum UpdateWikiOutcome {
    Finished(Option<WorkflowRun>),
    Waiting,
    CommittedPendingReconciliation,
}

async fn execute_update_wiki<F>(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &UpdateWikiExecutionServices<'_>,
    authorize_external_launch: F,
) -> Result<UpdateWikiOutcome, BackendError>
where
    F: FnOnce() -> Result<WorkflowExternalLaunchPermit, BackendError>,
{
    let task_id = run.task_id.as_str();
    let sink = WorkflowStageSink::new(services.compile.task_service, services.coordinator, task_id);
    sink.start(ANALYZE_SOURCES).map_err(task_error)?;
    let (mode, selected_refs) = update_scope(run)?;
    let current_baseline =
        super::super::preparation::workflow_baseline_for_scope(context, &run.scope)?;
    if current_baseline.fingerprint != run.baseline_fingerprint {
        return Err(BackendError::new(
            "WORKFLOW_INPUT_BASELINE_CHANGED",
            "Update Wiki inputs changed after preparation. Prepare and run again.",
            true,
            true,
        ));
    }
    let source_versions = resolve_selected_versions(context, &selected_refs)?;
    let resolved = CompileService::resolve_source_versions(context, &source_versions)?;
    let selected_sources = match mode {
        UpdateWikiMode::ChangedSources => resolved
            .into_iter()
            .filter(|source| !source.already_consumed)
            .collect::<Vec<_>>(),
        UpdateWikiMode::FullRecompile => resolved,
    };
    sink.progress(
        ANALYZE_SOURCES,
        selected_sources
            .first()
            .map(|source| source.project_path.clone()),
        selected_sources.len() as u64,
        Some(selected_refs.len() as u64),
    )
    .map_err(task_error)?;
    sink.complete(ANALYZE_SOURCES).map_err(task_error)?;
    if selected_sources.is_empty() {
        for stage in [
            CREATE_CHECKPOINT,
            PLAN_UPDATES,
            GENERATE_CANDIDATES,
            VALIDATE_STRUCTURE,
            REVIEW_RISK,
            APPLY_CHANGES,
            REFRESH_INDEXES,
        ] {
            sink.skip(stage).map_err(task_error)?;
        }
        sink.start(RECORD_RESULT).map_err(task_error)?;
        sink.complete(RECORD_RESULT).map_err(task_error)?;
        let (_, next) = sink
            .finish(WorkflowResult::UpdateWiki {
                created: 0,
                updated: 0,
                skipped: selected_refs.len() as u64,
                deleted: 0,
                conflicted: 0,
                affected_paths: Vec::new(),
                checkpoint_hash: None,
                final_commit: None,
            })
            .map_err(task_error)?;
        return Ok(UpdateWikiOutcome::Finished(next));
    }

    let input_baseline = snapshot_compile_inputs(context, &selected_sources)?;
    let wiki_baseline = CompileService::snapshot_wiki(context)?;
    sink.start(CREATE_CHECKPOINT).map_err(task_error)?;
    let checkpoint = with_update_wiki_git_cancellation(services, task_id, || {
        services.git_service.clean_head_checkpoint(
            context,
            CheckpointPurpose::HighRiskOperation,
            "Before Update Wiki workflow",
        )
    })?;
    sink.complete(CREATE_CHECKPOINT).map_err(task_error)?;

    let workspace =
        CompileService::create_workspace_for_sources(context, task_id, &selected_sources)?;
    let protected_sources = CompileService::snapshot_workspace_sources(&workspace)?;
    let result = async {
        sink.start(PLAN_UPDATES).map_err(task_error)?;
        let concrete_route = workflow_compile_route(run)?;
        let mut observer = UpdateWikiObserver {
            sink: &sink,
            current_item: selected_sources
                .first()
                .map(|source| source.project_path.clone()),
            total: selected_sources.len() as u64,
        };
        let publication = authorize_external_launch()?.begin()?;
        let candidate = CompileService::generate_candidate(
            context,
            &workspace,
            task_id,
            &wiki_baseline,
            &selected_sources,
            &protected_sources,
            concrete_route,
            CompileGenerationPolicy::WorkflowReviewableDeletes,
            &services.compile,
            &mut observer,
        )
        .await;
        publication.started();
        let candidate = candidate?;
        sink.progress(
            VALIDATE_STRUCTURE,
            candidate
                .manifest
                .files
                .first()
                .map(|file| file.path.clone()),
            candidate.manifest.files.len() as u64,
            Some(candidate.manifest.files.len() as u64),
        )
        .map_err(task_error)?;
        sink.complete(VALIDATE_STRUCTURE).map_err(task_error)?;
        sink.start(REVIEW_RISK).map_err(task_error)?;
        ensure_checkpoint_head(
            services,
            task_id,
            context,
            checkpoint.commit_hash.as_deref(),
        )?;
        revalidate_non_wiki_inputs(context, &input_baseline)?;
        if mode == UpdateWikiMode::ChangedSources {
            let source_versions = selected_sources
                .iter()
                .map(|source| source.reference.clone())
                .collect::<Vec<_>>();
            if CompileService::resolve_source_versions(context, &source_versions)?
                .iter()
                .any(|source| source.already_consumed)
            {
                return Err(BackendError::new(
                    "COMPILE_SOURCE_VERSION_STALE",
                    "A selected Source version was consumed while Update Wiki was running.",
                    true,
                    true,
                ));
            }
        }
        let summary = CompileService::classify_workflow_changes(
            context,
            &candidate.manifest,
            &candidate.plan,
            &wiki_baseline,
            mode == UpdateWikiMode::FullRecompile,
        )?;
        sink.progress(
            REVIEW_RISK,
            summary.affected_paths().first().cloned(),
            summary.affected_paths().len() as u64,
            Some(summary.affected_paths().len() as u64),
        )
        .map_err(task_error)?;
        if summary.requires_confirmation() {
            persist_update_wiki_review(
                context,
                run,
                &candidate,
                &summary.affected_paths(),
                &wiki_baseline,
                checkpoint.commit_hash.clone(),
                services,
            )?;
            return Ok(UpdateWikiOutcome::Waiting);
        }
        let candidate_sources = selected_sources
            .iter()
            .map(|source| source.reference.clone())
            .collect::<Vec<_>>();
        let current_hashes =
            current_manifest_hashes(context, &candidate.manifest, services.file_store)?;
        let candidate_hash = persist_candidate_state(
            context,
            task_id,
            &candidate,
            &candidate_sources,
            &current_hashes,
            &wiki_baseline,
            checkpoint.commit_hash.clone(),
        )?;
        let descriptor =
            load_valid_update_wiki_candidate(task_id, &context.root, Some(&candidate_hash))
                .ok_or_else(|| {
                    BackendError::new(
                        "WORKFLOW_CANDIDATE_STALE",
                        "The Update Wiki candidate is no longer valid.",
                        true,
                        true,
                    )
                })?;
        apply_persisted_update_wiki_candidate(context, run, descriptor, services)
    }
    .await;
    let preserve_recovery = result.as_ref().is_err_and(|error| {
        matches!(
            error.code.as_str(),
            "WORKFLOW_APPLY_ROLLBACK_FAILED" | "WORKFLOW_ROLLBACK_CONFLICT"
        )
    });
    if !matches!(
        result,
        Ok(UpdateWikiOutcome::Waiting | UpdateWikiOutcome::CommittedPendingReconciliation)
    ) && !preserve_recovery
    {
        let _ = discard_update_wiki_candidate(task_id);
    }
    result
}

struct UpdateWikiObserver<'a, 'b> {
    sink: &'a WorkflowStageSink<'b>,
    current_item: Option<String>,
    total: u64,
}

impl CompileGenerationObserver for UpdateWikiObserver<'_, '_> {
    fn begin_candidate_generation(&mut self) -> Result<(), BackendError> {
        self.sink.complete(PLAN_UPDATES).map_err(task_error)?;
        self.sink.start(GENERATE_CANDIDATES).map_err(task_error)?;
        self.sink
            .progress(
                GENERATE_CANDIDATES,
                self.current_item.clone(),
                0,
                Some(self.total),
            )
            .map_err(task_error)?;
        Ok(())
    }

    fn begin_validation(&mut self) -> Result<(), BackendError> {
        self.sink
            .progress(
                GENERATE_CANDIDATES,
                self.current_item.clone(),
                self.total,
                Some(self.total),
            )
            .map_err(task_error)?;
        self.sink
            .complete(GENERATE_CANDIDATES)
            .map_err(task_error)?;
        self.sink.start(VALIDATE_STRUCTURE).map_err(task_error)?;
        Ok(())
    }
}

fn update_scope(
    run: &WorkflowRun,
) -> Result<
    (
        UpdateWikiMode,
        Vec<crate::models::workflow::WorkflowSourceVersionRef>,
    ),
    BackendError,
> {
    match &run.scope {
        WorkflowScope::UpdateWiki {
            mode,
            source_versions,
        } => Ok((mode.clone(), source_versions.clone())),
        _ => Err(BackendError::new(
            "WORKFLOW_SCOPE_KIND_MISMATCH",
            "Update Wiki runner received another workflow scope.",
            false,
            true,
        )),
    }
}

fn resolve_selected_versions(
    context: &ProjectContext,
    selected: &[crate::models::workflow::WorkflowSourceVersionRef],
) -> Result<Vec<SourceVersionRef>, BackendError> {
    let current = CompileService::list_source_versions(context)?;
    selected
        .iter()
        .map(|selected| {
            current
                .iter()
                .find(|current| {
                    current.source_id == selected.source_id
                        && current.version_id == selected.version_id
                })
                .cloned()
                .ok_or_else(|| {
                    BackendError::new(
                        "WORKFLOW_SOURCE_SCOPE_STALE",
                        "A selected Source version changed before Update Wiki started.",
                        true,
                        true,
                    )
                })
        })
        .collect()
}

fn workflow_compile_route(run: &WorkflowRun) -> Result<ResolvedCompileRoute, BackendError> {
    match run.route.as_ref() {
        Some(WorkflowRoute::Agent { agent, model, .. }) => Ok(ResolvedCompileRoute::Agent {
            agent: *agent,
            model: model.clone(),
        }),
        Some(WorkflowRoute::Byok {
            provider, model, ..
        }) => Ok(ResolvedCompileRoute::Byok {
            provider: *provider,
            model: model.clone(),
        }),
        _ => Err(BackendError::new(
            "WORKFLOW_ROUTE_REQUIRED",
            "Update Wiki requires one prepared Agent or BYOK route.",
            true,
            true,
        )),
    }
}

fn snapshot_compile_inputs(
    context: &ProjectContext,
    sources: &[crate::services::ResolvedCompileSource],
) -> Result<HashMap<String, String>, BackendError> {
    let mut snapshot = CompileService::snapshot_wiki(context)?;
    for path in ["purpose.md", "schema.md"] {
        if context.resolve_project_path(path)?.is_file() {
            snapshot.insert(path.into(), FileStore.file_hash(context, path)?);
        }
    }
    for source in sources {
        snapshot.insert(
            source.project_path.clone(),
            FileStore.file_hash(context, &source.project_path)?,
        );
    }
    Ok(snapshot)
}

fn revalidate_non_wiki_inputs(
    context: &ProjectContext,
    baseline: &HashMap<String, String>,
) -> Result<(), BackendError> {
    for (path, expected) in baseline
        .iter()
        .filter(|(path, _)| !path.starts_with("wiki/") || path.starts_with("wiki/sources/"))
    {
        let absolute = context.resolve_project_path(path)?;
        if !absolute.is_file() || FileStore.file_hash(context, path)? != *expected {
            return Err(BackendError::new(
                "WORKFLOW_INPUT_BASELINE_CHANGED",
                "Update Wiki inputs changed during generation. Prepare and run again.",
                true,
                true,
            ));
        }
    }
    Ok(())
}

fn register_waiting_decision(
    context: &ProjectContext,
    run: &WorkflowRun,
    candidate: &CompileCandidate,
    affected_paths: &[String],
    baseline_hashes: &HashMap<String, String>,
    checkpoint_hash: Option<String>,
    services: &UpdateWikiExecutionServices<'_>,
) -> Result<(WorkflowPendingAction, ConfirmationExecution), BackendError> {
    let action_id = uuid::Uuid::new_v4().to_string();
    let current_hashes =
        current_manifest_hashes(context, &candidate.manifest, services.file_store)?;
    let source_versions = match &run.scope {
        WorkflowScope::UpdateWiki {
            source_versions, ..
        } => resolve_selected_versions(context, source_versions)?,
        _ => Vec::new(),
    };
    let candidate_hash = persist_candidate_state(
        context,
        &run.task_id,
        candidate,
        &source_versions,
        &current_hashes,
        baseline_hashes,
        checkpoint_hash.clone(),
    )?;
    let candidate_id = format!("{}:{candidate_hash}", run.task_id);
    let action = PendingAction {
        id: action_id.clone(),
        action_type: PendingActionType::MergeConflict,
        title: "Review Update Wiki changes".into(),
        message: "Generated Wiki changes include conflicts, deletes, overwrites, or a broad rewrite and require review.".into(),
        risk_level: RiskLevel::High,
        affected_paths: affected_paths.to_vec(),
        preview: Some(ActionPreview {
            summary: format!("{} path(s) require review", affected_paths.len()),
            before: None,
            after: None,
            diff: Some(CompileService::candidate_diff(&candidate.manifest)),
        }),
        expires_at: None,
        checkpoint_hash: checkpoint_hash.clone(),
    };
    let execution = ConfirmationExecution::UpdateWikiReview {
        project_id: context.project_id.clone(),
        root_path: context.root.to_string_lossy().into_owned(),
        canonical_identity_key: run.canonical_identity_key.clone(),
        identity_revision: run.identity_revision.clone(),
        task_id: run.task_id.clone(),
        action_id: action_id.clone(),
        candidate: WorkflowCandidateReference::TaskOwned {
            candidate_id: candidate_id.clone(),
        },
    };
    services
        .confirmation_registry
        .register_with_execution(action, Some(execution.clone()))?;
    Ok((
        WorkflowPendingAction {
            id: action_id,
            action_type: PendingActionType::MergeConflict,
            risk_level: RiskLevel::High,
            affected_paths: affected_paths.to_vec(),
            candidate: Some(WorkflowCandidateReference::TaskOwned { candidate_id }),
            expires_at: None,
            checkpoint_hash,
        },
        execution,
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedUpdateWikiCandidate {
    schema_version: u32,
    task_id: String,
    project_identity_key: String,
    project_identity_revision: String,
    candidate: CompileCandidate,
    source_versions: Vec<SourceVersionRef>,
    current_hashes: HashMap<String, String>,
    baseline_hashes: HashMap<String, String>,
    checkpoint_hash: Option<String>,
    #[serde(default)]
    reconciliation_result: Option<WorkflowResult>,
}

fn persist_candidate_state(
    context: &ProjectContext,
    task_id: &str,
    candidate: &CompileCandidate,
    source_versions: &[SourceVersionRef],
    current_hashes: &HashMap<String, String>,
    baseline_hashes: &HashMap<String, String>,
    checkpoint_hash: Option<String>,
) -> Result<String, BackendError> {
    let workspace = create_candidate_workspace_for_task(task_id)?;
    let metadata = std::fs::symlink_metadata(&workspace).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_PERSIST_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(BackendError::new(
            "WORKFLOW_CANDIDATE_PERSIST_FAILED",
            "The task candidate workspace is not a regular directory.",
            false,
            true,
        ));
    }
    let final_path = workspace.join("workflow-candidate.json");
    let identity =
        super::super::persistence::project_identity(&context.root).map_err(task_error)?;
    let descriptor = PersistedUpdateWikiCandidate {
        schema_version: 1,
        task_id: task_id.to_string(),
        project_identity_key: identity.canonical_identity_key,
        project_identity_revision: identity.identity_revision,
        candidate: candidate.clone(),
        source_versions: source_versions.to_vec(),
        current_hashes: current_hashes.clone(),
        baseline_hashes: baseline_hashes.clone(),
        checkpoint_hash,
        reconciliation_result: None,
    };
    let candidate_hash = update_wiki_candidate_hash(&descriptor).ok_or_else(|| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_PERSIST_FAILED",
            "The Update Wiki candidate authority hash could not be computed.",
            true,
            true,
        )
    })?;
    let bytes = serde_json::to_vec_pretty(&descriptor).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_PERSIST_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    FileStore
        .write_bytes_create_new_absolute(
            workspace.parent().ok_or_else(|| {
                BackendError::new(
                    "WORKFLOW_CANDIDATE_PERSIST_FAILED",
                    "The task candidate workspace root is unavailable.",
                    false,
                    true,
                )
            })?,
            &final_path,
            &bytes,
        )
        .map_err(|error| {
            BackendError::new(
                "WORKFLOW_CANDIDATE_PERSIST_FAILED",
                error.message,
                error.recoverable,
                error.user_action_required,
            )
        })?;
    Ok(candidate_hash)
}

fn update_wiki_candidate_hash(descriptor: &PersistedUpdateWikiCandidate) -> Option<String> {
    let mut authority = descriptor.clone();
    authority.reconciliation_result = None;
    canonical_json(&authority)
        .ok()
        .map(|value| hex_sha256(value.as_bytes()))
}

fn persist_reconciliation_result(
    task_id: &str,
    result: &WorkflowResult,
) -> Result<(), BackendError> {
    let descriptor_path = workspace_for_task(task_id).join("workflow-candidate.json");
    let metadata = std::fs::symlink_metadata(&descriptor_path).map_err(|error| {
        BackendError::new(
            "WORKFLOW_RECONCILIATION_PERSIST_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(BackendError::new(
            "WORKFLOW_RECONCILIATION_PERSIST_FAILED",
            "The Update Wiki candidate descriptor is not a regular file.",
            false,
            true,
        ));
    }
    let mut descriptor =
        FileStore.read_json_file::<PersistedUpdateWikiCandidate>(&descriptor_path)?;
    if descriptor.task_id != task_id {
        return Err(BackendError::new(
            "WORKFLOW_RECONCILIATION_PERSIST_FAILED",
            "The Update Wiki candidate descriptor belongs to another task.",
            false,
            true,
        ));
    }
    descriptor.reconciliation_result = Some(result.clone());
    FileStore.write_json_atomic_absolute(
        descriptor_path
            .parent()
            .and_then(|parent| parent.parent())
            .ok_or_else(|| {
                BackendError::new(
                    "WORKFLOW_RECONCILIATION_PERSIST_FAILED",
                    "The workflow workspace root is unavailable.",
                    false,
                    true,
                )
            })?,
        &descriptor_path,
        &descriptor,
    )
}

pub fn persist_update_wiki_review(
    context: &ProjectContext,
    run: &WorkflowRun,
    candidate: &CompileCandidate,
    affected_paths: &[String],
    baseline_hashes: &HashMap<String, String>,
    checkpoint_hash: Option<String>,
    services: &UpdateWikiExecutionServices<'_>,
) -> Result<WorkflowRun, BackendError> {
    let (pending, execution) = register_waiting_decision(
        context,
        run,
        candidate,
        affected_paths,
        baseline_hashes,
        checkpoint_hash,
        services,
    )?;
    let action_id = pending.id.clone();
    let result = WorkflowStageSink::new(
        services.compile.task_service,
        services.coordinator,
        &run.task_id,
    )
    .wait(REVIEW_RISK, pending)
    .map_err(task_error);
    if result.is_err() {
        let _ = services
            .confirmation_registry
            .remove_exact_execution(&action_id, &execution);
    }
    result
}

#[derive(Debug)]
pub struct UpdateWikiConfirmationFailure {
    pub error: BackendError,
    pub next: Option<WorkflowRun>,
}

pub fn confirm_update_wiki_review(
    context: &ProjectContext,
    task_id: &str,
    services: &UpdateWikiExecutionServices<'_>,
) -> Result<(WorkflowRun, Option<WorkflowRun>), UpdateWikiConfirmationFailure> {
    let run = services
        .compile
        .task_service
        .get_workflow_run(task_id)
        .ok_or_else(|| UpdateWikiConfirmationFailure {
            error: BackendError::new(
                "TASK_NOT_FOUND",
                "Update Wiki task not found.",
                false,
                false,
            ),
            next: None,
        })?;
    let workflow = services
        .compile
        .task_service
        .workflow_execution_state(task_id)
        .ok_or_else(|| UpdateWikiConfirmationFailure {
            error: BackendError::new(
                "TASK_NOT_FOUND",
                "Update Wiki execution state is unavailable.",
                false,
                false,
            ),
            next: None,
        })?;
    let descriptor = load_update_wiki_candidate_for_workflow(task_id, &context.root, &workflow);
    if run.kind != WorkflowKind::UpdateWiki
        || run.display_status
            != crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
        || descriptor.is_none()
    {
        let error = BackendError::new(
            "WORKFLOW_CANDIDATE_STALE",
            "The Update Wiki candidate is no longer safe to apply.",
            true,
            true,
        );
        let next = finish_error(&run, services, error.clone());
        let _ = discard_update_wiki_candidate(task_id);
        return Err(UpdateWikiConfirmationFailure { error, next });
    }
    let descriptor = descriptor.expect("checked above");
    if let Err(message) = services
        .compile
        .task_service
        .begin_confirmed_workflow_apply(task_id)
    {
        let error = task_error(message);
        let next = finish_error(&run, services, error.clone());
        let _ = discard_update_wiki_candidate(task_id);
        return Err(UpdateWikiConfirmationFailure { error, next });
    }

    match apply_persisted_update_wiki_candidate(context, &run, descriptor, services) {
        Ok(UpdateWikiOutcome::Finished(next)) => {
            let completed = services
                .compile
                .task_service
                .get_workflow_run(task_id)
                .ok_or_else(|| UpdateWikiConfirmationFailure {
                    error: BackendError::new(
                        "TASK_NOT_FOUND",
                        "Update Wiki task disappeared after confirmation.",
                        false,
                        false,
                    ),
                    next: next.clone(),
                })?;
            let _ = discard_update_wiki_candidate(task_id);
            Ok((completed, next))
        }
        Ok(UpdateWikiOutcome::CommittedPendingReconciliation) => {
            let current = services
                .compile
                .task_service
                .get_workflow_run(task_id)
                .ok_or_else(|| UpdateWikiConfirmationFailure {
                    error: BackendError::new(
                        "TASK_NOT_FOUND",
                        "Update Wiki task disappeared after the committed result.",
                        false,
                        false,
                    ),
                    next: None,
                })?;
            Ok((current, None))
        }
        Ok(UpdateWikiOutcome::Waiting) => Err(UpdateWikiConfirmationFailure {
            error: BackendError::new(
                "WORKFLOW_CONFIRMATION_STATE_INVALID",
                "Update Wiki remained waiting after confirmation.",
                false,
                true,
            ),
            next: None,
        }),
        Err(error) => {
            let preserve_recovery = matches!(
                error.code.as_str(),
                "WORKFLOW_APPLY_ROLLBACK_FAILED" | "WORKFLOW_ROLLBACK_CONFLICT"
            );
            let next = finish_error(&run, services, error.clone());
            if !preserve_recovery {
                let _ = discard_update_wiki_candidate(task_id);
            }
            Err(UpdateWikiConfirmationFailure { error, next })
        }
    }
}

pub fn restore_update_wiki_confirmation(
    context: &ProjectContext,
    run: &WorkflowRun,
    tasks: &TaskService,
    registry: &ConfirmationRegistry,
) -> Result<(), BackendError> {
    let pending = run.pending_action.as_ref().ok_or_else(|| {
        BackendError::new(
            "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
            "Update Wiki has no pending confirmation to restore.",
            true,
            true,
        )
    })?;
    let workflow = tasks
        .workflow_execution_state(&run.task_id)
        .ok_or_else(|| {
            BackendError::new(
                "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
                "Update Wiki execution state is unavailable.",
                true,
                true,
            )
        })?;
    let Some(descriptor) =
        load_update_wiki_candidate_for_workflow(&run.task_id, &context.root, &workflow)
    else {
        return Err(BackendError::new(
            "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
            "Update Wiki candidate is no longer valid.",
            true,
            true,
        ));
    };
    registry.restore_with_execution(
        PendingAction {
            id: pending.id.clone(),
            action_type: pending.action_type.clone(),
            title: "Review Update Wiki changes".into(),
            message: "Generated Wiki changes require review before they can be applied.".into(),
            risk_level: pending.risk_level.clone(),
            affected_paths: pending.affected_paths.clone(),
            preview: Some(ActionPreview {
                summary: format!("{} path(s) require review", pending.affected_paths.len()),
                before: None,
                after: None,
                diff: Some(CompileService::candidate_diff(
                    &descriptor.candidate.manifest,
                )),
            }),
            expires_at: pending.expires_at.clone(),
            checkpoint_hash: pending.checkpoint_hash.clone(),
        },
        ConfirmationExecution::UpdateWikiReview {
            project_id: context.project_id.clone(),
            root_path: context.root.to_string_lossy().into_owned(),
            canonical_identity_key: run.canonical_identity_key.clone(),
            identity_revision: run.identity_revision.clone(),
            task_id: run.task_id.clone(),
            action_id: pending.id.clone(),
            candidate: pending.candidate.clone().ok_or_else(|| {
                BackendError::new(
                    "WORKFLOW_CONFIRMATION_CANDIDATE_MISSING",
                    "The persisted confirmation has no candidate binding.",
                    true,
                    true,
                )
            })?,
        },
    )
}

fn apply_persisted_update_wiki_candidate(
    context: &ProjectContext,
    run: &WorkflowRun,
    descriptor: PersistedUpdateWikiCandidate,
    services: &UpdateWikiExecutionServices<'_>,
) -> Result<UpdateWikiOutcome, BackendError> {
    let task_id = run.task_id.as_str();
    let sink = WorkflowStageSink::new(services.compile.task_service, services.coordinator, task_id);
    let (mode, _) = update_scope(run)?;
    ensure_checkpoint_head(
        services,
        task_id,
        context,
        descriptor.checkpoint_hash.as_deref(),
    )?;
    ensure_clean_git(services, task_id, context)?;
    if current_manifest_hashes(context, &descriptor.candidate.manifest, services.file_store)?
        != descriptor.current_hashes
    {
        return Err(BackendError::new(
            "WORKFLOW_OUTPUT_BASELINE_CHANGED",
            "Wiki outputs changed after the Update Wiki candidate was prepared.",
            true,
            true,
        ));
    }
    let resolved_sources =
        CompileService::resolve_source_versions(context, &descriptor.source_versions)?;
    if mode == UpdateWikiMode::ChangedSources
        && resolved_sources
            .iter()
            .any(|source| source.already_consumed)
    {
        return Err(BackendError::new(
            "COMPILE_SOURCE_VERSION_STALE",
            "A selected Source version was consumed before Update Wiki confirmation.",
            true,
            true,
        ));
    }
    let known_sources = CompileService::known_source_refs(context)?;
    CompileService::validate_workflow_manifest_semantics(
        context,
        &descriptor.candidate.manifest,
        Some(&descriptor.candidate.plan),
        &known_sources,
    )?;
    let summary = CompileService::classify_workflow_changes(
        context,
        &descriptor.candidate.manifest,
        &descriptor.candidate.plan,
        &descriptor.baseline_hashes,
        mode == UpdateWikiMode::FullRecompile,
    )?;
    // Admission to the non-interruptible apply phase must be atomic with the
    // updater's final guard validation. Holding this lease prevents Windows
    // updater handoff from terminating the process during project writes.
    let _update_mutation_lease = services
        .confirmation_registry
        .update_install_barrier()
        .enter_project_mutation()?;
    sink.complete(REVIEW_RISK).map_err(task_error)?;
    sink.start(APPLY_CHANGES).map_err(task_error)?;

    let mut metadata_paths = vec![format!(".app/compile/{task_id}.json")];
    metadata_paths.extend(
        descriptor
            .source_versions
            .iter()
            .filter(|reference| !reference.source_id.starts_with("legacy-"))
            .map(|reference| format!(".app/sources/{}.json", reference.source_id)),
    );
    let mut backup = CompileService::backup_workflow_outputs(
        context,
        &descriptor.candidate.manifest,
        &metadata_paths,
    )?;
    services
        .compile
        .task_service
        .set_task_cancellable(task_id, false)
        .map_err(task_error)?;
    let preapply_check = (|| {
        ensure_checkpoint_head(
            services,
            task_id,
            context,
            descriptor.checkpoint_hash.as_deref(),
        )?;
        ensure_clean_git(services, task_id, context)?;
        if current_manifest_hashes(context, &descriptor.candidate.manifest, services.file_store)?
            != descriptor.current_hashes
        {
            return Err(BackendError::new(
                "WORKFLOW_OUTPUT_BASELINE_CHANGED",
                "Wiki outputs changed immediately before Update Wiki apply.",
                true,
                true,
            ));
        }
        Ok(())
    })();
    if let Err(error) = preapply_check {
        services
            .compile
            .task_service
            .set_task_cancellable(task_id, true)
            .map_err(task_error)?;
        return Err(error);
    }
    let expected_hashes =
        baseline_manifest_hashes(&descriptor.candidate.manifest, &descriptor.baseline_hashes);
    let affected_paths = match CompileService::apply_confirmed_workflow_manifest(
        context,
        &descriptor.candidate.manifest,
        Some(&descriptor.candidate.plan),
        &expected_hashes,
    ) {
        Ok(paths) => paths,
        Err(error) => {
            let applied_paths = applied_paths_from_error(&error);
            if let Err(rollback_error) = CompileService::restore_workflow_outputs_if_unchanged(
                context,
                &backup,
                &descriptor.candidate.manifest,
                &applied_paths,
            ) {
                return Err(BackendError::new(
                    "WORKFLOW_APPLY_ROLLBACK_FAILED",
                    format!(
                        "Update Wiki apply failed and rollback could not restore a stable state: apply={}; rollback={}",
                        error.message, rollback_error.message
                    ),
                    false,
                    true,
                )
                .with_details(serde_json::json!({
                    "candidateId": task_id,
                    "appliedPaths": applied_paths,
                })));
            }
            let _ = services
                .compile
                .task_service
                .set_task_cancellable(task_id, true);
            return Err(mark_update_wiki_error_after_successful_rollback(
                error,
                &applied_paths,
            ));
        }
    };
    let mut rollback_paths = affected_paths.clone();
    rollback_paths.push(".app/graph-cache.json".into());
    rollback_paths.extend(metadata_paths.iter().cloned());
    let reversible_finalize = (|| {
        sink.progress(
            APPLY_CHANGES,
            affected_paths.first().cloned(),
            affected_paths.len() as u64,
            Some(affected_paths.len() as u64),
        )
        .map_err(task_error)?;
        sink.complete(APPLY_CHANGES).map_err(task_error)?;
        sink.start(REFRESH_INDEXES).map_err(task_error)?;
        refresh_indexes(context, task_id, services)?;
        CompileService::capture_workflow_installed_values(
            context,
            &mut backup,
            &[workflow_graph_cache_relative_path(context)],
        )?;
        sink.complete(REFRESH_INDEXES).map_err(task_error)?;
        sink.start(RECORD_RESULT).map_err(task_error)?;
        Ok::<_, BackendError>(())
    })();
    if let Err(error) = reversible_finalize {
        rollback_after_apply(
            context,
            task_id,
            &backup,
            &descriptor.candidate.manifest,
            &rollback_paths,
            services,
            &error,
        )?;
        return Err(mark_update_wiki_error_rolled_back(error));
    }
    let pending_result = WorkflowResult::UpdateWiki {
        created: summary.created.len() as u64,
        updated: summary.updated.len() as u64,
        skipped: summary.skipped.len() as u64,
        deleted: summary.deleted.len() as u64,
        conflicted: summary.conflicted.len() as u64,
        affected_paths: affected_paths.clone(),
        checkpoint_hash: descriptor.checkpoint_hash.clone(),
        final_commit: None,
    };
    if let Err(error) = persist_reconciliation_result(task_id, &pending_result) {
        rollback_after_apply(
            context,
            task_id,
            &backup,
            &descriptor.candidate.manifest,
            &rollback_paths,
            services,
            &error,
        )?;
        return Err(mark_update_wiki_error_rolled_back(error));
    }
    let final_commit = match record_compile_result(
        context,
        task_id,
        descriptor.candidate.route.legacy_kind(),
        &affected_paths,
        descriptor.checkpoint_hash.clone(),
        &descriptor.source_versions,
        &mut backup,
        &metadata_paths,
        services,
    ) {
        Ok(commit) => commit,
        Err(error) => {
            rollback_after_apply(
                context,
                task_id,
                &backup,
                &descriptor.candidate.manifest,
                &rollback_paths,
                services,
                &error,
            )?;
            return Err(mark_update_wiki_error_rolled_back(error));
        }
    };
    let committed_result = WorkflowResult::UpdateWiki {
        created: summary.created.len() as u64,
        updated: summary.updated.len() as u64,
        skipped: summary.skipped.len() as u64,
        deleted: summary.deleted.len() as u64,
        conflicted: summary.conflicted.len() as u64,
        affected_paths,
        checkpoint_hash: descriptor.checkpoint_hash,
        final_commit,
    };
    let _ = persist_reconciliation_result(task_id, &committed_result);
    let terminal = sink
        .complete(RECORD_RESULT)
        .and_then(|_| sink.finish(committed_result).map(|(_, next)| next));
    let next = match terminal {
        Ok(next) => next,
        Err(error) => {
            let _ = services.compile.task_service.append_log(
                task_id,
                LogLevel::Error,
                format!(
                    "Update Wiki committed successfully but task completion requires recovery: {error}"
                ),
            );
            return Ok(UpdateWikiOutcome::CommittedPendingReconciliation);
        }
    };
    let _ = services
        .compile
        .task_service
        .set_task_cancellable(task_id, true);
    Ok(UpdateWikiOutcome::Finished(next))
}

fn current_manifest_hashes(
    context: &ProjectContext,
    manifest: &CompileManifest,
    file_store: &FileStore,
) -> Result<HashMap<String, String>, BackendError> {
    let mut hashes = HashMap::new();
    for path in manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .chain(manifest.deletions.iter().map(String::as_str))
    {
        if let Some(hash) = file_store.file_hash_if_exists(context, path)? {
            hashes.insert(path.to_string(), hash);
        }
    }
    Ok(hashes)
}

fn baseline_manifest_hashes(
    manifest: &CompileManifest,
    baseline: &HashMap<String, String>,
) -> HashMap<String, String> {
    manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .chain(manifest.deletions.iter().map(String::as_str))
        .filter_map(|path| {
            baseline
                .get(path)
                .cloned()
                .map(|hash| (path.to_string(), hash))
        })
        .collect()
}

fn refresh_indexes(
    context: &ProjectContext,
    task_id: &str,
    services: &UpdateWikiExecutionServices<'_>,
) -> Result<(), BackendError> {
    refresh_workflow_wiki_indexes(
        context,
        task_id,
        services.file_store,
        services.bookmark_service,
        services.search_service,
        services.compile.task_service,
    )
    .map(|_| ())
}

pub(crate) fn refresh_workflow_wiki_indexes(
    context: &ProjectContext,
    task_id: &str,
    file_store: &FileStore,
    bookmark_service: &BookmarkService,
    search_service: &SearchService,
    task_service: &TaskService,
) -> Result<Vec<String>, BackendError> {
    let graph_relative_path = workflow_graph_cache_relative_path(context);
    let graph_cache = workflow_stale_graph_cache_value(context, file_store);
    let mut warnings = Vec::new();
    if let Err(error) = file_store.write_json_atomic(context, &graph_relative_path, &graph_cache) {
        warnings.push(format!("graph_cache: {}", error.message));
        let _ = task_service.append_log(
            task_id,
            LogLevel::Warn,
            format!(
                "Wiki updated, but graph cache refresh failed: {}",
                error.message
            ),
        );
    }
    match bookmark_service.wiki_page_paths(context) {
        Ok(bookmarks) => {
            if let Err(error) = search_service.scan_wiki(context, &bookmarks) {
                warnings.push(format!("search: {}", error.message));
                let _ = task_service.append_log(
                    task_id,
                    LogLevel::Warn,
                    format!("Wiki updated, but search refresh failed: {}", error.message),
                );
            }
        }
        Err(error) => {
            warnings.push(format!("bookmarks: {}", error.message));
            let _ = task_service.append_log(
                task_id,
                LogLevel::Warn,
                format!(
                    "Wiki updated, but bookmarks could not be read: {}",
                    error.message
                ),
            );
        }
    }
    Ok(warnings)
}

pub(crate) fn workflow_stale_graph_cache_value(
    context: &ProjectContext,
    file_store: &FileStore,
) -> serde_json::Value {
    let graph_path = context
        .root
        .join(workflow_graph_cache_relative_path(context));
    let mut graph_cache = if graph_path.exists() {
        file_store
            .read_json_file::<serde_json::Value>(&graph_path)
            .unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    if !graph_cache.is_object() {
        graph_cache = serde_json::json!({});
    }
    graph_cache
        .as_object_mut()
        .expect("graph cache was normalized")
        .insert("status".into(), serde_json::Value::String("stale".into()));
    graph_cache
}

pub(crate) fn workflow_graph_cache_relative_path(context: &ProjectContext) -> String {
    let app_root = context.layout.app_state_root.as_deref().unwrap_or(".app");
    format!("{}/graph-cache.json", app_root.trim_end_matches('/'))
}

fn record_compile_result(
    context: &ProjectContext,
    task_id: &str,
    route: CompileRoute,
    affected_paths: &[String],
    checkpoint_hash: Option<String>,
    source_versions: &[SourceVersionRef],
    backup: &mut crate::services::compile_service::CompileBackup,
    metadata_paths: &[String],
    services: &UpdateWikiExecutionServices<'_>,
) -> Result<Option<String>, BackendError> {
    SourceRegistry::record_compile_consumption(
        context,
        services.file_store,
        &CompileConsumptionRecord {
            schema_version: 1,
            compile_task_id: task_id.into(),
            route,
            consumed_at: chrono::Utc::now().to_rfc3339(),
            source_versions: source_versions.to_vec(),
            affected_paths: affected_paths.to_vec(),
            checkpoint: checkpoint_hash.clone(),
        },
    )?;
    CompileService::capture_workflow_installed_values(context, backup, metadata_paths)?;
    let compile_record_path = format!(".app/compile/{task_id}.json");
    let mut checkpoint_paths = affected_paths.to_vec();
    checkpoint_paths.push(".app/graph-cache.json".into());
    checkpoint_paths.push(compile_record_path);
    checkpoint_paths.extend(
        source_versions
            .iter()
            .filter(|reference| !reference.source_id.starts_with("legacy-"))
            .map(|reference| format!(".app/sources/{}.json", reference.source_id)),
    );
    checkpoint_paths.sort();
    checkpoint_paths.dedup();
    match with_update_wiki_git_cancellation(services, task_id, || {
        services.git_service.create_scoped_checkpoint(
            context,
            CheckpointPurpose::FinalResult,
            &format!("Update Wiki {task_id}"),
            &checkpoint_paths,
        )
    }) {
        Ok(checkpoint) => Ok(checkpoint.commit_hash),
        Err(error) => {
            let _ = services
                .git_service
                .unstage_paths(context, &checkpoint_paths);
            Err(BackendError::new(
                "WORKFLOW_FINAL_COMMIT_FAILED",
                format!("Update Wiki final result commit failed: {}", error.message),
                true,
                true,
            ))
        }
    }
}

fn ensure_checkpoint_head(
    services: &UpdateWikiExecutionServices<'_>,
    task_id: &str,
    context: &ProjectContext,
    expected: Option<&str>,
) -> Result<(), BackendError> {
    let status = with_update_wiki_git_cancellation(services, task_id, || {
        services.git_service.repository_status(context)
    })?;
    if status.head.as_deref() != expected {
        return Err(BackendError::new(
            "COMPILE_CHECKPOINT_CHANGED",
            "The project Git HEAD changed during Update Wiki.",
            true,
            true,
        ));
    }
    Ok(())
}

fn ensure_clean_git(
    services: &UpdateWikiExecutionServices<'_>,
    task_id: &str,
    context: &ProjectContext,
) -> Result<(), BackendError> {
    let status = with_update_wiki_git_cancellation(services, task_id, || {
        services.git_service.repository_status(context)
    })?;
    if !status.is_repository || status.has_changes {
        return Err(BackendError::new(
            "WORKFLOW_GIT_STATE_CHANGED",
            "Update Wiki requires the prepared Git repository to remain clean.",
            true,
            true,
        ));
    }
    Ok(())
}

fn rollback_after_apply(
    context: &ProjectContext,
    task_id: &str,
    backup: &crate::services::compile_service::CompileBackup,
    manifest: &CompileManifest,
    applied_paths: &[String],
    services: &UpdateWikiExecutionServices<'_>,
    original_error: &BackendError,
) -> Result<(), BackendError> {
    if let Err(rollback_error) = CompileService::restore_workflow_outputs_if_unchanged(
        context,
        backup,
        manifest,
        applied_paths,
    ) {
        return Err(BackendError::new(
            "WORKFLOW_APPLY_ROLLBACK_FAILED",
            format!(
                "Update Wiki could not reach a stable state after an error: original={}; rollback={}",
                original_error.message, rollback_error.message
            ),
            false,
            true,
        )
        .with_details(serde_json::json!({
            "candidateId": task_id,
            "appliedPaths": applied_paths,
        })));
    }
    let _ = services
        .compile
        .task_service
        .set_task_cancellable(task_id, true);
    Ok(())
}

fn applied_paths_from_error(error: &BackendError) -> Vec<String> {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("appliedPaths"))
        .and_then(serde_json::Value::as_array)
        .map(|paths| {
            paths
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn mark_update_wiki_error_rolled_back(error: BackendError) -> BackendError {
    mark_update_wiki_error_mutation_state(error, WorkflowProjectMutationState::RolledBack)
}

fn mark_update_wiki_error_after_successful_rollback(
    error: BackendError,
    applied_paths: &[String],
) -> BackendError {
    let state = if matches!(
        error.code.as_str(),
        "WORKFLOW_APPLY_ROLLBACK_FAILED" | "WORKFLOW_ROLLBACK_CONFLICT"
    ) {
        WorkflowProjectMutationState::Modified
    } else if applied_paths.is_empty() {
        WorkflowProjectMutationState::NotModified
    } else {
        WorkflowProjectMutationState::RolledBack
    };
    mark_update_wiki_error_mutation_state(error, state)
}

fn mark_update_wiki_error_mutation_state(
    mut error: BackendError,
    state: WorkflowProjectMutationState,
) -> BackendError {
    let mut details = match error.details.take() {
        Some(serde_json::Value::Object(details)) => details,
        Some(details) => {
            let mut object = serde_json::Map::new();
            object.insert("originalDetails".into(), details);
            object
        }
        None => serde_json::Map::new(),
    };
    details.insert(
        "workflowProjectMutationState".into(),
        serde_json::Value::String(
            match state {
                WorkflowProjectMutationState::NotModified => "not_modified",
                WorkflowProjectMutationState::Modified => "modified",
                WorkflowProjectMutationState::RolledBack => "rolled_back",
                WorkflowProjectMutationState::Unknown => "unknown",
            }
            .into(),
        ),
    );
    error.details = Some(serde_json::Value::Object(details));
    error
}

fn project_mutation_state_for_update_error(error: &BackendError) -> WorkflowProjectMutationState {
    if let Some(state) = error
        .details
        .as_ref()
        .and_then(|details| details.get("workflowProjectMutationState"))
        .and_then(serde_json::Value::as_str)
    {
        match state {
            "not_modified" => WorkflowProjectMutationState::NotModified,
            "modified" => WorkflowProjectMutationState::Modified,
            "rolled_back" => WorkflowProjectMutationState::RolledBack,
            _ => WorkflowProjectMutationState::Unknown,
        }
    } else if error.code.contains("STALE") || error.code.contains("BASELINE") {
        WorkflowProjectMutationState::NotModified
    } else if matches!(
        error.code.as_str(),
        "WORKFLOW_APPLY_ROLLBACK_FAILED" | "WORKFLOW_ROLLBACK_CONFLICT"
    ) {
        WorkflowProjectMutationState::Modified
    } else {
        WorkflowProjectMutationState::Unknown
    }
}

fn finish_error(
    run: &WorkflowRun,
    services: &UpdateWikiExecutionServices<'_>,
    error: BackendError,
) -> Option<WorkflowRun> {
    let tasks: &TaskService = services.compile.task_service;
    let _ = tasks.append_log(&run.task_id, LogLevel::Error, error.message.clone());
    let cancelled = tasks.is_cancelled(&run.task_id)
        || tasks.get_task(&run.task_id).is_some_and(|task| {
            matches!(task.status, TaskStatus::Cancelling | TaskStatus::Cancelled)
        });
    if tasks.get_workflow_run(&run.task_id).is_some_and(|current| {
        current.display_status
            == crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
    }) {
        if cancelled {
            let _ = services.coordinator.cancel(tasks, &run.task_id);
        } else {
            let _ = tasks.clear_workflow_pending_action(&run.task_id);
        }
    }
    let outcome = if cancelled {
        services
            .coordinator
            .finish_cancelled_and_claim_next(tasks, &run.task_id)
    } else {
        let _ = tasks.set_error(&run.task_id, error.clone());
        let sink = WorkflowStageSink::new(tasks, services.coordinator, &run.task_id);
        let refreshed = tasks.get_workflow_run(&run.task_id);
        let running = refreshed.as_ref().and_then(|run| {
            run.stages
                .iter()
                .find(|stage| stage.status == crate::models::workflow::WorkflowStageStatus::Running)
                .map(|stage| stage.id.clone())
        });
        let pending = refreshed.as_ref().and_then(|run| {
            run.stages
                .iter()
                .find(|stage| stage.status == crate::models::workflow::WorkflowStageStatus::Pending)
                .map(|stage| stage.id.clone())
        });
        let current = running
            .or(pending)
            .unwrap_or_else(|| ANALYZE_SOURCES.into());
        if refreshed.as_ref().is_some_and(|run| {
            run.stages.iter().any(|stage| {
                stage.id == current
                    && stage.status == crate::models::workflow::WorkflowStageStatus::Pending
            })
        }) {
            let _ = sink.start(&current);
        }
        sink.fail(
            &current,
            WorkflowErrorSummary {
                code: error.code.clone(),
                message_key: if error.code.contains("STALE") || error.code.contains("BASELINE") {
                    "workflows.error.prepareAgain".into()
                } else {
                    "workflows.error.updateWikiFailed".into()
                },
                recoverable: error.recoverable,
                user_action_required: error.user_action_required,
                suggested_action: if error.code.contains("STALE") || error.code.contains("BASELINE")
                {
                    Some(WorkflowPrerequisiteAction::PrepareAgain)
                } else {
                    None
                },
                project_mutation_state: project_mutation_state_for_update_error(&error),
            },
        )
    };
    outcome.ok().and_then(|(_, next)| next)
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

fn with_update_wiki_git_cancellation<T>(
    services: &UpdateWikiExecutionServices<'_>,
    task_id: &str,
    operation: impl FnOnce() -> Result<T, BackendError>,
) -> Result<T, BackendError> {
    let token = services
        .compile
        .task_service
        .get_cancellation_token(task_id)
        .ok_or_else(|| task_error(format!("Task cancellation token is unavailable: {task_id}")))?;
    services
        .git_service
        .with_task_cancellation(token, operation)
}

fn workspace_for_task(task_id: &str) -> std::path::PathBuf {
    std::env::temp_dir().join("llm-wiki-desktop").join(task_id)
}

fn create_candidate_workspace_for_task(task_id: &str) -> Result<std::path::PathBuf, BackendError> {
    let canonical = uuid::Uuid::parse_str(task_id).map_err(|_| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_ID_INVALID",
            "The workflow candidate id must be a canonical UUID.",
            false,
            true,
        )
    })?;
    if canonical.to_string() != task_id {
        return Err(BackendError::new(
            "WORKFLOW_CANDIDATE_ID_INVALID",
            "The workflow candidate id must be a canonical UUID.",
            false,
            true,
        ));
    }

    let workspace_root = std::env::temp_dir().join("llm-wiki-desktop");
    ensure_private_directory(&workspace_root).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_PERSIST_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    let workspace = workspace_root.join(task_id);
    create_private_directory(&workspace).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_PERSIST_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    Ok(workspace)
}

pub fn discard_update_wiki_candidate(task_id: &str) -> Result<(), BackendError> {
    if uuid::Uuid::parse_str(task_id).is_err() {
        return Err(BackendError::new(
            "WORKFLOW_CANDIDATE_ID_INVALID",
            "The workflow candidate id is invalid.",
            false,
            true,
        ));
    }
    let workspace_root = std::env::temp_dir().join("llm-wiki-desktop");
    let workspace = workspace_root.join(task_id);
    if !workspace.exists() {
        return Ok(());
    }
    let metadata = std::fs::symlink_metadata(&workspace).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_DISCARD_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(BackendError::new(
            "WORKFLOW_CANDIDATE_DISCARD_FAILED",
            "The workflow candidate is not a regular task directory.",
            false,
            true,
        ));
    }
    let canonical_root = workspace_root.canonicalize().map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_DISCARD_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    let canonical_workspace = workspace.canonicalize().map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_DISCARD_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    if canonical_workspace.parent() != Some(canonical_root.as_path()) {
        return Err(BackendError::new(
            "WORKFLOW_CANDIDATE_DISCARD_FAILED",
            "The workflow candidate escaped its task-owned workspace root.",
            false,
            true,
        ));
    }
    std::fs::remove_dir_all(&canonical_workspace).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_DISCARD_FAILED",
            error.to_string(),
            true,
            false,
        )
    })
}

pub fn update_wiki_candidate_is_valid(task_id: &str, project_root: &std::path::Path) -> bool {
    load_valid_update_wiki_candidate(task_id, project_root, None).is_some()
}

pub(crate) fn committed_update_wiki_result(
    task_id: &str,
    project_root: &std::path::Path,
) -> Option<WorkflowResult> {
    let descriptor = load_valid_update_wiki_candidate(task_id, project_root, None)?;
    let mut result = descriptor.reconciliation_result?;
    let WorkflowResult::UpdateWiki {
        final_commit,
        checkpoint_hash,
        ..
    } = &mut result
    else {
        return None;
    };
    if let Some(commit) = final_commit.as_deref() {
        if !GitService::checkpoint_exists(project_root, commit) {
            return None;
        }
    } else {
        let context = ProjectContext::new("workflow-recovery", project_root.to_path_buf())
            .with_resolved_layout()
            .ok()?;
        let head = GitService.repository_status(&context).ok()?.head?;
        if checkpoint_hash.as_deref() == Some(head.as_str())
            || GitService::head_subject(&context).as_deref()
                != Some(format!("Update Wiki {task_id}").as_str())
        {
            return None;
        }
        *final_commit = Some(head);
    }
    let context = ProjectContext::new("workflow-recovery", project_root.to_path_buf())
        .with_resolved_layout()
        .ok()?;
    let compile_root = context.layout.compile_state_root.as_deref()?;
    context
        .resolve_project_path(&format!("{compile_root}/{task_id}.json"))
        .ok()?
        .is_file()
        .then_some(result)
}

pub fn update_wiki_decision_review(
    task_id: &str,
    project_root: &std::path::Path,
) -> Option<WorkflowDecisionReview> {
    let descriptor = load_valid_update_wiki_candidate(task_id, project_root, None)?;
    update_wiki_decision_review_from_descriptor(project_root, descriptor, true)
}

/// Borrowed, owner-neutral view over a task-owned candidate. Update Wiki and
/// Agent lint repair keep separate persistence/authority loaders, then share
/// this single two-way/three-way/lazy Diff implementation after exact owner
/// validation.
pub(crate) struct TaskOwnedCandidateReviewSource<'a> {
    pub manifest: &'a CompileManifest,
    pub baseline_hashes: &'a HashMap<String, String>,
    pub current_hashes: &'a HashMap<String, String>,
    pub checkpoint_hash: Option<&'a str>,
}

pub(crate) fn update_wiki_decision_review_for_workflow(
    task_id: &str,
    project_root: &std::path::Path,
    workflow: &crate::models::workflow::WorkflowExecutionState,
) -> Option<WorkflowDecisionReview> {
    let descriptor = load_update_wiki_candidate_for_workflow(task_id, project_root, workflow)?;
    update_wiki_decision_review_from_descriptor(project_root, descriptor, true)
}

pub(crate) fn update_wiki_decision_review_summary_for_workflow(
    task_id: &str,
    project_root: &std::path::Path,
    workflow: &crate::models::workflow::WorkflowExecutionState,
) -> Option<WorkflowDecisionReview> {
    let descriptor = load_update_wiki_candidate_for_workflow(task_id, project_root, workflow)?;
    update_wiki_decision_review_from_descriptor(project_root, descriptor, false)
}

pub(crate) fn update_wiki_review_can_inline(
    summary: &WorkflowDecisionReview,
    max_file_bytes: usize,
    max_review_bytes: usize,
) -> bool {
    if summary
        .file_diffs
        .iter()
        .any(|file| file.kind == WorkflowFileDiffKind::ThreeWay || file.diff_bytes > max_file_bytes)
    {
        return false;
    }
    let metadata_bytes =
        serde_json::to_vec(summary).map_or(max_review_bytes + 1, |value| value.len());
    let diff_bytes = summary
        .file_diffs
        .iter()
        .try_fold(0usize, |total, file| total.checked_add(file.diff_bytes));
    diff_bytes
        .is_some_and(|diff_bytes| metadata_bytes.saturating_add(diff_bytes) <= max_review_bytes)
}

pub(crate) fn update_wiki_file_diff_page_for_workflow(
    task_id: &str,
    project_root: &std::path::Path,
    workflow: &crate::models::workflow::WorkflowExecutionState,
    file_id: &str,
    start: usize,
    limit: usize,
) -> Result<Option<WorkflowFileDiffPage>, BackendError> {
    let Some(index) = file_id
        .strip_prefix("file-")
        .and_then(|value| usize::from_str_radix(value, 16).ok())
    else {
        return Ok(None);
    };
    let Some(descriptor) = load_update_wiki_candidate_for_workflow(task_id, project_root, workflow)
    else {
        return Ok(None);
    };
    let context = ProjectContext::new("workflow-review", project_root.to_path_buf())
        .with_resolved_layout()
        .map_err(|error| BackendError::new("WORKFLOW_DIFF_NOT_FOUND", error.message, true, true))?;
    let source = TaskOwnedCandidateReviewSource {
        manifest: &descriptor.candidate.manifest,
        baseline_hashes: &descriptor.baseline_hashes,
        current_hashes: &descriptor.current_hashes,
        checkpoint_hash: descriptor.checkpoint_hash.as_deref(),
    };
    task_owned_candidate_file_diff_page(&context, &source, file_id, index, start, limit)
}

pub(crate) fn task_owned_candidate_file_diff_page(
    context: &ProjectContext,
    source: &TaskOwnedCandidateReviewSource<'_>,
    file_id: &str,
    index: usize,
    start: usize,
    limit: usize,
) -> Result<Option<WorkflowFileDiffPage>, BackendError> {
    let files = &source.manifest.files;
    let (path, candidate) = if let Some(file) = files.get(index) {
        (file.path.as_str(), Some(file.content.as_str()))
    } else if let Some(deletion) = source
        .manifest
        .deletions
        .get(index.checked_sub(files.len()).unwrap_or(usize::MAX))
    {
        (deletion.as_str(), None)
    } else {
        return Ok(None);
    };
    let kind = task_owned_file_diff_kind(source, path);
    let page =
        paginate_task_owned_file_diff_source(context, source, path, candidate, kind, start, limit)?;
    Ok(Some(WorkflowFileDiffPage {
        file_id: file_id.to_string(),
        path: path.to_string(),
        kind,
        ..page
    }))
}

fn paginate_task_owned_file_diff_source(
    context: &ProjectContext,
    source: &TaskOwnedCandidateReviewSource<'_>,
    path: &str,
    candidate: Option<&str>,
    kind: WorkflowFileDiffKind,
    start: usize,
    limit: usize,
) -> Result<WorkflowFileDiffPage, BackendError> {
    let mut page = VirtualDiffPageBuilder::new(start, limit);
    if kind == WorkflowFileDiffKind::ThreeWay {
        let checkpoint = source.checkpoint_hash.ok_or_else(|| {
            BackendError::new(
                "WORKFLOW_DIFF_NOT_FOUND",
                "The three-way workflow baseline is unavailable.",
                true,
                true,
            )
        })?;
        let baseline = GitService::file_at_checkpoint(context, checkpoint, path)?;
        let absolute = context.resolve_project_path(path)?;
        let current = if absolute.try_exists().map_err(|error| {
            BackendError::new("WORKFLOW_DIFF_READ_FAILED", error.to_string(), true, true)
        })? {
            Some(FileStore.read_markdown(context, path)?)
        } else {
            None
        };
        let current_hash = current
            .as_deref()
            .map(|content| hex_sha256(content.as_bytes()));
        if current_hash.as_ref() != source.current_hashes.get(path) {
            return Err(BackendError::new(
                "WORKFLOW_OUTPUT_BASELINE_CHANGED",
                "The reviewed workflow file changed after this candidate was prepared.",
                true,
                true,
            ));
        }
        page.push("```diff\n");
        push_three_way_section(&mut page, "baseline", path, baseline.as_deref());
        push_three_way_section(&mut page, "current", path, current.as_deref());
        push_three_way_section(&mut page, "candidate", path, candidate);
        page.push("\n```");
    } else {
        let current_hash = FileStore.file_hash_if_exists(context, path)?;
        if current_hash.as_ref() != source.current_hashes.get(path) {
            return Err(BackendError::new(
                "WORKFLOW_OUTPUT_BASELINE_CHANGED",
                "The reviewed workflow file changed after this candidate was prepared.",
                true,
                true,
            ));
        }
        page.push("```diff\n");
        if let Some(content) = candidate {
            page.push(&format!("--- {path} (current)\n+++ {path} (candidate)\n"));
            for line in content.lines() {
                if page.is_full() {
                    page.mark_remaining();
                    break;
                }
                page.push("+");
                page.push(line);
                page.push("\n");
            }
        } else {
            page.push(&format!("--- {path}\n+++ /dev/null\n"));
        }
        page.push("```");
    }
    page.finish(kind)
}

fn push_three_way_section(
    page: &mut VirtualDiffPageBuilder,
    label: &str,
    path: &str,
    content: Option<&str>,
) {
    page.push(&format!("--- {label}/{path}\n"));
    let content = content.unwrap_or("<file absent>");
    page.push(content);
    if !content.ends_with('\n') {
        page.push("\n");
    }
}

struct VirtualDiffPageBuilder {
    start: usize,
    limit: usize,
    position: usize,
    start_valid: bool,
    has_remaining: bool,
    diff: String,
}

impl VirtualDiffPageBuilder {
    fn new(start: usize, limit: usize) -> Self {
        Self {
            start,
            limit: limit.max(1),
            position: 0,
            start_valid: start == 0,
            has_remaining: false,
            diff: String::with_capacity(limit.min(256 * 1024)),
        }
    }

    fn push(&mut self, segment: &str) {
        let segment_start = self.position;
        let segment_end = segment_start.saturating_add(segment.len());
        if self.start == segment_start || self.start == segment_end {
            self.start_valid = true;
        } else if self.start > segment_start && self.start < segment_end {
            self.start_valid = segment.is_char_boundary(self.start - segment_start);
        }
        if self.start < segment_end && self.diff.len() < self.limit {
            let local_start = self.start.saturating_sub(segment_start).min(segment.len());
            if segment.is_char_boundary(local_start) {
                let remaining = self.limit - self.diff.len();
                let mut local_end = local_start.saturating_add(remaining).min(segment.len());
                while local_end > local_start && !segment.is_char_boundary(local_end) {
                    local_end -= 1;
                }
                if local_end == local_start && local_start < segment.len() {
                    local_end =
                        local_start + segment[local_start..].chars().next().unwrap().len_utf8();
                }
                self.diff.push_str(&segment[local_start..local_end]);
            }
        }
        self.position = segment_end;
    }

    fn is_full(&self) -> bool {
        self.diff.len() >= self.limit
    }

    fn mark_remaining(&mut self) {
        self.has_remaining = true;
    }

    fn finish(mut self, kind: WorkflowFileDiffKind) -> Result<WorkflowFileDiffPage, BackendError> {
        if self.start == self.position {
            self.start_valid = true;
        }
        if self.start > self.position || !self.start_valid {
            return Err(BackendError::new(
                "WORKFLOW_DIFF_CURSOR_INVALID",
                "The workflow diff cursor is invalid.",
                true,
                false,
            ));
        }
        let next = self.start.saturating_add(self.diff.len());
        let truncated = self.has_remaining || next < self.position;
        Ok(WorkflowFileDiffPage {
            file_id: String::new(),
            path: String::new(),
            kind,
            diff: self.diff,
            next_cursor: truncated.then_some(next),
            truncated,
        })
    }
}

fn task_owned_file_diff_kind(
    source: &TaskOwnedCandidateReviewSource<'_>,
    path: &str,
) -> WorkflowFileDiffKind {
    if source.current_hashes.get(path) != source.baseline_hashes.get(path)
        && source.checkpoint_hash.is_some()
    {
        WorkflowFileDiffKind::ThreeWay
    } else {
        WorkflowFileDiffKind::TwoWay
    }
}

fn materialize_task_owned_file_diff(
    context: &ProjectContext,
    source: &TaskOwnedCandidateReviewSource<'_>,
    path: &str,
    candidate: Option<&str>,
    kind: WorkflowFileDiffKind,
) -> Option<String> {
    if kind == WorkflowFileDiffKind::ThreeWay {
        let checkpoint = source.checkpoint_hash?;
        let baseline = GitService::file_at_checkpoint(context, checkpoint, path).ok()?;
        let absolute = context.resolve_project_path(path).ok()?;
        let current = if absolute.try_exists().ok()? {
            Some(FileStore.read_markdown(context, path).ok()?)
        } else {
            None
        };
        return Some(render_three_way_comparison(
            path,
            baseline.as_deref(),
            current.as_deref(),
            candidate,
        ));
    }
    Some(CompileService::candidate_diff(&CompileManifest {
        files: candidate
            .map(|content| CompileFile {
                path: path.to_string(),
                content: content.to_string(),
            })
            .into_iter()
            .collect(),
        deletions: candidate
            .is_none()
            .then(|| path.to_string())
            .into_iter()
            .collect(),
        summary: source.manifest.summary.clone(),
    }))
}

fn render_three_way_comparison(
    path: &str,
    baseline: Option<&str>,
    current: Option<&str>,
    candidate: Option<&str>,
) -> String {
    fn section(label: &str, path: &str, content: Option<&str>) -> String {
        let content = content.unwrap_or("<file absent>");
        let separator = if content.ends_with('\n') { "" } else { "\n" };
        format!("--- {label}/{path}\n{content}{separator}")
    }
    format!(
        "```diff\n{}{}{}\n```",
        section("baseline", path, baseline),
        section("current", path, current),
        section("candidate", path, candidate),
    )
}

fn update_wiki_decision_review_from_descriptor(
    project_root: &std::path::Path,
    descriptor: PersistedUpdateWikiCandidate,
    include_diffs: bool,
) -> Option<WorkflowDecisionReview> {
    let context = ProjectContext::new("workflow-review", project_root.to_path_buf())
        .with_resolved_layout()
        .ok()?;
    let summary = CompileService::classify_workflow_changes(
        &context,
        &descriptor.candidate.manifest,
        &descriptor.candidate.plan,
        &descriptor.baseline_hashes,
        false,
    )
    .ok()?;
    let source = TaskOwnedCandidateReviewSource {
        manifest: &descriptor.candidate.manifest,
        baseline_hashes: &descriptor.baseline_hashes,
        current_hashes: &descriptor.current_hashes,
        checkpoint_hash: descriptor.checkpoint_hash.as_deref(),
    };
    task_owned_candidate_decision_review(
        &context,
        &source,
        &summary,
        &descriptor.candidate.plan.summary,
        include_diffs,
    )
}

pub(crate) fn task_owned_candidate_decision_review(
    context: &ProjectContext,
    source: &TaskOwnedCandidateReviewSource<'_>,
    summary: &crate::models::compile::CompileChangeSummary,
    reason: &str,
    include_diffs: bool,
) -> Option<WorkflowDecisionReview> {
    let counts = WorkflowDecisionCounts {
        created: summary.created.len() as u32,
        modified: summary.updated.len() as u32,
        overwritten: summary.conflicted.len() as u32,
        deleted: summary.deleted.len() as u32,
    };
    let user_edits_detected = workflow_user_edits_detected(
        source.manifest,
        source.baseline_hashes,
        source.current_hashes,
    );
    let mut file_diffs = source
        .manifest
        .files
        .iter()
        .map(|file| {
            let kind = task_owned_file_diff_kind(source, &file.path);
            let diff = include_diffs
                .then(|| {
                    materialize_task_owned_file_diff(
                        context,
                        source,
                        &file.path,
                        Some(&file.content),
                        kind,
                    )
                })
                .flatten();
            WorkflowFileDiff {
                file_id: String::new(),
                path: file.path.clone(),
                diff_bytes: diff.as_ref().map_or_else(
                    || candidate_file_diff_len(&file.path, Some(&file.content)),
                    String::len,
                ),
                diff,
                kind,
            }
        })
        .collect::<Vec<_>>();
    file_diffs.extend(source.manifest.deletions.iter().map(|path| {
        let kind = task_owned_file_diff_kind(source, path);
        let diff = include_diffs
            .then(|| materialize_task_owned_file_diff(context, source, path, None, kind))
            .flatten();
        WorkflowFileDiff {
            file_id: String::new(),
            path: path.clone(),
            diff_bytes: diff
                .as_ref()
                .map_or_else(|| candidate_file_diff_len(path, None), String::len),
            diff,
            kind,
        }
    }));
    Some(WorkflowDecisionReview {
        reason: reason.to_string(),
        counts,
        user_edits_detected,
        file_diffs,
    })
}

fn workflow_user_edits_detected(
    manifest: &CompileManifest,
    baseline_hashes: &HashMap<String, String>,
    current_hashes: &HashMap<String, String>,
) -> bool {
    manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .chain(manifest.deletions.iter().map(String::as_str))
        .any(|path| baseline_hashes.get(path) != current_hashes.get(path))
}

fn candidate_file_diff_len(path: &str, content: Option<&str>) -> usize {
    let mut bytes = "```diff\n".len() + "```".len();
    if let Some(content) = content {
        bytes += "--- ".len()
            + path.len()
            + " (current)\n+++ ".len()
            + path.len()
            + " (candidate)\n".len();
        bytes += content
            .lines()
            .map(|line| 1 + line.len() + 1)
            .sum::<usize>();
    } else {
        bytes += "--- ".len() + path.len() + "\n+++ /dev/null\n".len();
    }
    bytes
}

#[cfg(test)]
mod batch6_diff_summary_tests {
    use super::*;

    #[test]
    fn update_wiki_candidate_workspace_is_private_and_create_new() {
        let task_id = uuid::Uuid::new_v4().to_string();
        let workspace = create_candidate_workspace_for_task(&task_id).unwrap();
        assert_eq!(
            workspace.file_name().and_then(|name| name.to_str()),
            Some(task_id.as_str())
        );
        assert!(create_candidate_workspace_for_task(&task_id).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            assert_eq!(
                std::fs::metadata(&workspace).unwrap().permissions().mode() & 0o777,
                0o700
            );
        }

        discard_update_wiki_candidate(&task_id).unwrap();
    }

    #[test]
    fn task_owned_review_helpers_accept_a_lint_repair_candidate_view() {
        let root =
            std::env::temp_dir().join(format!("llm-wiki-review-owner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("wiki")).unwrap();
        std::fs::write(root.join("wiki/page.md"), "# Current").unwrap();
        let context = ProjectContext::new("lint-review", root.clone());
        let current_hash = hex_sha256(b"# Current");
        let baseline = HashMap::from([("wiki/page.md".into(), current_hash.clone())]);
        let current = HashMap::from([("wiki/page.md".into(), current_hash)]);
        let manifest = CompileManifest {
            files: vec![CompileFile {
                path: "wiki/page.md".into(),
                content: "# Repaired".into(),
            }],
            deletions: Vec::new(),
            summary: "Agent wiki-lint repair".into(),
        };
        let summary = crate::models::compile::CompileChangeSummary {
            updated: vec!["wiki/page.md".into()],
            ..Default::default()
        };
        let source = TaskOwnedCandidateReviewSource {
            manifest: &manifest,
            baseline_hashes: &baseline,
            current_hashes: &current,
            checkpoint_hash: None,
        };
        let review = task_owned_candidate_decision_review(
            &context,
            &source,
            &summary,
            "Review Agent repair",
            false,
        )
        .unwrap();
        assert_eq!(review.reason, "Review Agent repair");
        assert_eq!(review.counts.modified, 1);
        assert_eq!(review.file_diffs[0].kind, WorkflowFileDiffKind::TwoWay);
        let page = task_owned_candidate_file_diff_page(&context, &source, "file-0", 0, 0, 256)
            .unwrap()
            .unwrap();
        assert!(page.diff.contains("# Repaired"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn deleted_worktree_file_is_reported_as_a_user_edit() {
        let manifest = CompileManifest {
            files: Vec::new(),
            deletions: vec!["wiki/deleted.md".into()],
            summary: String::new(),
        };
        let baseline = HashMap::from([
            ("wiki/deleted.md".to_string(), "baseline".to_string()),
            ("wiki/unaffected.md".to_string(), "unaffected".to_string()),
        ]);
        let current = HashMap::new();

        assert!(workflow_user_edits_detected(&manifest, &baseline, &current));
    }

    #[test]
    fn unaffected_baseline_files_do_not_report_user_edits() {
        let manifest = CompileManifest {
            files: vec![crate::models::compile::CompileFile::new(
                "wiki/affected.md",
                "candidate",
            )],
            deletions: Vec::new(),
            summary: String::new(),
        };
        let baseline = HashMap::from([
            ("wiki/affected.md".to_string(), "same".to_string()),
            ("wiki/unaffected.md".to_string(), "other".to_string()),
        ]);
        let current = HashMap::from([("wiki/affected.md".to_string(), "same".to_string())]);

        assert!(!workflow_user_edits_detected(
            &manifest, &baseline, &current
        ));
    }

    #[test]
    fn successful_update_rollback_is_reported_explicitly() {
        let error = BackendError::new("WRITE_FAILED", "write failed", true, true);
        let marked = mark_update_wiki_error_rolled_back(error);

        assert_eq!(
            project_mutation_state_for_update_error(&marked),
            WorkflowProjectMutationState::RolledBack
        );

        let untouched = mark_update_wiki_error_after_successful_rollback(
            BackendError::new("WRITE_FAILED", "write failed", true, true),
            &[],
        );
        assert_eq!(
            project_mutation_state_for_update_error(&untouched),
            WorkflowProjectMutationState::NotModified
        );

        let restored = mark_update_wiki_error_after_successful_rollback(
            BackendError::new("WRITE_FAILED", "write failed", true, true),
            &["wiki/page.md".into()],
        );
        assert_eq!(
            project_mutation_state_for_update_error(&restored),
            WorkflowProjectMutationState::RolledBack
        );

        for applied_paths in [Vec::new(), vec!["wiki/page.md".into()]] {
            let recovery_failed = mark_update_wiki_error_after_successful_rollback(
                BackendError::new(
                    "WORKFLOW_APPLY_ROLLBACK_FAILED",
                    "recovery failed",
                    false,
                    true,
                ),
                &applied_paths,
            );
            assert_eq!(
                project_mutation_state_for_update_error(&recovery_failed),
                WorkflowProjectMutationState::Modified
            );
        }
    }

    #[test]
    fn virtual_diff_pages_copy_only_the_requested_chunk() {
        let content = "甲".repeat(200_000);
        let mut builder = VirtualDiffPageBuilder::new(0, 4096);
        builder.push("```diff\n");
        builder.push(&content);
        builder.push("\n```");
        let first = builder.finish(WorkflowFileDiffKind::ThreeWay).unwrap();

        assert!(first.truncated);
        assert!(first.diff.len() <= 4098);
        assert_eq!(first.next_cursor, Some(first.diff.len()));
    }

    #[test]
    fn three_way_reviews_are_always_lazy() {
        let review = WorkflowDecisionReview {
            reason: "review".into(),
            counts: WorkflowDecisionCounts::default(),
            user_edits_detected: true,
            file_diffs: vec![WorkflowFileDiff {
                file_id: "file-00000000".into(),
                path: "wiki/page.md".into(),
                diff_bytes: 1,
                diff: None,
                kind: WorkflowFileDiffKind::ThreeWay,
            }],
        };

        assert!(!update_wiki_review_can_inline(
            &review,
            256 * 1024,
            1024 * 1024
        ));
    }

    #[test]
    fn summary_diff_lengths_match_materialized_diffs_without_retaining_diff_text() {
        let file = crate::models::compile::CompileFile::new(
            "wiki/规模/页面.md",
            "first line\nsecond line\n",
        );
        let materialized = CompileService::candidate_diff(&CompileManifest {
            files: vec![file.clone()],
            deletions: Vec::new(),
            summary: String::new(),
        });
        assert_eq!(
            candidate_file_diff_len(&file.path, Some(&file.content)),
            materialized.len(),
        );

        let deletion = "wiki/删除/页面.md";
        let materialized = CompileService::candidate_diff(&CompileManifest {
            files: Vec::new(),
            deletions: vec![deletion.into()],
            summary: String::new(),
        });
        assert_eq!(candidate_file_diff_len(deletion, None), materialized.len());
    }

    #[test]
    fn three_way_comparison_keeps_baseline_current_and_candidate_distinct() {
        let rendered = render_three_way_comparison(
            "wiki/冲突.md",
            Some("baseline text\n"),
            Some("user text\n"),
            Some("candidate text\n"),
        );
        assert!(rendered.contains("--- baseline/wiki/冲突.md\nbaseline text"));
        assert!(rendered.contains("--- current/wiki/冲突.md\nuser text"));
        assert!(rendered.contains("--- candidate/wiki/冲突.md\ncandidate text"));

        let deletion = render_three_way_comparison(
            "wiki/删除.md",
            Some("baseline"),
            Some("user changed"),
            None,
        );
        assert!(deletion.contains("--- candidate/wiki/删除.md\n<file absent>"));
    }
}

pub fn update_wiki_candidate_is_valid_for_workflow(
    task_id: &str,
    project_root: &std::path::Path,
    workflow: &crate::models::workflow::WorkflowExecutionState,
) -> bool {
    load_update_wiki_candidate_for_workflow(task_id, project_root, workflow).is_some()
}

fn load_update_wiki_candidate_for_workflow(
    task_id: &str,
    project_root: &std::path::Path,
    workflow: &crate::models::workflow::WorkflowExecutionState,
) -> Option<PersistedUpdateWikiCandidate> {
    let Some(candidate_id) =
        workflow
            .pending_action
            .as_ref()
            .and_then(|pending| match pending.candidate.as_ref()? {
                WorkflowCandidateReference::TaskOwned { candidate_id } => {
                    Some(candidate_id.as_str())
                }
                WorkflowCandidateReference::ProjectRelative { .. } => None,
            })
    else {
        return None;
    };
    let Some((candidate_task_id, expected_hash)) = candidate_id.split_once(':') else {
        return None;
    };
    if candidate_task_id != task_id || expected_hash.len() != 64 {
        return None;
    }
    let descriptor = load_valid_update_wiki_candidate(task_id, project_root, Some(expected_hash))?;
    if workflow.kind != WorkflowKind::UpdateWiki
        || workflow.canonical_identity_key != descriptor.project_identity_key
        || workflow.identity_revision != descriptor.project_identity_revision
        || workflow
            .pending_action
            .as_ref()
            .and_then(|pending| pending.checkpoint_hash.as_ref())
            != descriptor.checkpoint_hash.as_ref()
        || !workflow_route_matches_candidate(workflow.route.as_ref(), &descriptor.candidate.route)
    {
        return None;
    }
    let WorkflowScope::UpdateWiki {
        source_versions, ..
    } = &workflow.scope
    else {
        return None;
    };
    let expected_sources = source_versions
        .iter()
        .map(|source| (&source.source_id, &source.version_id))
        .collect::<std::collections::HashSet<_>>();
    let persisted_sources = descriptor
        .source_versions
        .iter()
        .map(|source| (&source.source_id, &source.version_id))
        .collect::<std::collections::HashSet<_>>();
    if expected_sources != persisted_sources {
        return None;
    }
    let Ok(context) =
        ProjectContext::new("workflow-recovery", project_root.to_path_buf()).with_resolved_layout()
    else {
        return None;
    };
    if CompileService::resolve_source_versions(&context, &descriptor.source_versions).is_err() {
        return None;
    }
    let Ok(known_sources) = CompileService::known_source_refs(&context) else {
        return None;
    };
    CompileService::validate_workflow_manifest_semantics(
        &context,
        &descriptor.candidate.manifest,
        Some(&descriptor.candidate.plan),
        &known_sources,
    )
    .ok()?;
    Some(descriptor)
}

fn workflow_route_matches_candidate(
    workflow: Option<&WorkflowRoute>,
    candidate: &ResolvedCompileRoute,
) -> bool {
    match (workflow, candidate) {
        (
            Some(WorkflowRoute::Agent { agent, model, .. }),
            ResolvedCompileRoute::Agent {
                agent: candidate_agent,
                model: candidate_model,
            },
        ) => agent == candidate_agent && model == candidate_model,
        (
            Some(WorkflowRoute::Byok {
                provider, model, ..
            }),
            ResolvedCompileRoute::Byok {
                provider: candidate_provider,
                model: candidate_model,
            },
        ) => provider == candidate_provider && model == candidate_model,
        _ => false,
    }
}

fn load_valid_update_wiki_candidate(
    task_id: &str,
    project_root: &std::path::Path,
    expected_hash: Option<&str>,
) -> Option<PersistedUpdateWikiCandidate> {
    if uuid::Uuid::parse_str(task_id).is_err() {
        return None;
    }
    let workspace_root = std::env::temp_dir().join("llm-wiki-desktop");
    let workspace = workspace_root.join(task_id);
    let descriptor_path = workspace.join("workflow-candidate.json");
    let Ok(workspace_metadata) = std::fs::symlink_metadata(&workspace) else {
        return None;
    };
    let Ok(descriptor_metadata) = std::fs::symlink_metadata(&descriptor_path) else {
        return None;
    };
    if !workspace_metadata.is_dir()
        || workspace_metadata.file_type().is_symlink()
        || !descriptor_metadata.is_file()
        || descriptor_metadata.file_type().is_symlink()
    {
        return None;
    }
    let (Ok(canonical_root), Ok(canonical_workspace), Ok(canonical_descriptor)) = (
        workspace_root.canonicalize(),
        workspace.canonicalize(),
        descriptor_path.canonicalize(),
    ) else {
        return None;
    };
    if canonical_workspace.parent() != Some(canonical_root.as_path())
        || canonical_descriptor.parent() != Some(canonical_workspace.as_path())
    {
        return None;
    }
    let Ok(bytes) = std::fs::read(canonical_descriptor) else {
        return None;
    };
    let Ok(descriptor) = serde_json::from_slice::<PersistedUpdateWikiCandidate>(&bytes) else {
        return None;
    };
    if expected_hash.is_some_and(|expected| {
        update_wiki_candidate_hash(&descriptor).as_deref() != Some(expected)
    }) {
        return None;
    }
    let Ok(identity) = super::super::persistence::project_identity(project_root) else {
        return None;
    };
    if descriptor.schema_version != 1
        || descriptor.task_id != task_id
        || descriptor.project_identity_key != identity.canonical_identity_key
        || descriptor.project_identity_revision != identity.identity_revision
        || CompileService::validate_workflow_manifest(&descriptor.candidate.manifest).is_err()
    {
        return None;
    }
    let affected = descriptor
        .candidate
        .manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .chain(
            descriptor
                .candidate
                .manifest
                .deletions
                .iter()
                .map(String::as_str),
        )
        .collect::<std::collections::HashSet<_>>();
    let valid = descriptor
        .current_hashes
        .keys()
        .all(|path| affected.contains(path.as_str()))
        && descriptor
            .baseline_hashes
            .keys()
            .all(|path| crate::services::compile_service::is_safe_wiki_markdown(path))
        && descriptor.checkpoint_hash.as_deref().map_or(true, |hash| {
            GitService::checkpoint_exists(project_root, hash)
        });
    valid.then_some(descriptor)
}
