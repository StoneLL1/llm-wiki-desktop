use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use serde::{Deserialize, Serialize};

use crate::errors::BackendError;
use crate::models::agent::AgentDetectionState;
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, ConfirmationRegistry, PendingAction, PendingActionType,
    RiskLevel,
};
use crate::models::export::{ExportContentOptions, ExportPreviewMetadata, ExportRoute, ExportType};
use crate::models::git::CheckpointPurpose;
use crate::models::paths::ProjectContext;
use crate::models::task::TaskStatus;
use crate::models::workflow::{
    WorkflowArtifactType, WorkflowCandidateReference, WorkflowErrorSummary, WorkflowExecutionState,
    WorkflowKind, WorkflowPendingAction, WorkflowPrerequisiteAction, WorkflowProjectMutationState,
    WorkflowResult, WorkflowRoute, WorkflowRun, WorkflowScope,
};
use crate::services::{
    AgentService, ExportService, FileStore, GitService, LlmService, SearchService, SecretService,
    SettingsService, WriteMode,
};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

use super::super::{
    fingerprint::{canonical_json, hex_sha256},
    persistence::project_identity,
    preparation::workflow_baseline_for_scope,
    WorkflowCoordinator, WorkflowExternalLaunchPermit, WorkflowRunner, WorkflowStageSink,
};

const CONFIRM_SCOPE: &str = "confirm_scope";
const READ_WIKI: &str = "read_wiki";
const LOAD_TEMPLATE: &str = "load_template";
const GENERATE_CONTENT: &str = "generate_content";
const ASSEMBLE_ARTIFACT: &str = "assemble_artifact";
const VALIDATE_ARTIFACT: &str = "validate_artifact";
const WRITE_EXPORT: &str = "write_export";
const GENERATE_PREVIEW: &str = "generate_preview";
const COMPLETE: &str = "complete";

type StartCallback = dyn Fn(WorkflowRun) + Send + Sync;

pub struct GenerateContentRunner {
    start_callback: Arc<StartCallback>,
}

#[cfg(test)]
mod ui3_mutation_state_tests {
    use super::*;

    #[test]
    fn generate_rollback_state_distinguishes_success_and_failure() {
        assert_eq!(
            generate_rollback_mutation_state(false, false, false),
            WorkflowProjectMutationState::NotModified
        );
        assert_eq!(
            generate_rollback_mutation_state(true, true, true),
            WorkflowProjectMutationState::RolledBack
        );
        assert_eq!(
            generate_rollback_mutation_state(true, false, true),
            WorkflowProjectMutationState::Modified
        );
        assert_eq!(
            generate_rollback_mutation_state(true, true, false),
            WorkflowProjectMutationState::Modified
        );
    }
}

impl GenerateContentRunner {
    pub fn new(callback: impl Fn(WorkflowRun) + Send + Sync + 'static) -> Self {
        Self {
            start_callback: Arc::new(callback),
        }
    }
}

impl WorkflowRunner for GenerateContentRunner {
    fn kind(&self) -> WorkflowKind {
        WorkflowKind::GenerateContent
    }

    fn start(&self, run: WorkflowRun) {
        (self.start_callback)(run);
    }
}

pub struct GenerateContentExecutionServices<'a> {
    pub export_service: &'a ExportService,
    pub search_service: &'a SearchService,
    pub settings_service: &'a SettingsService,
    pub secret_service: &'a SecretService,
    pub agent_service: &'a AgentService,
    pub llm_service: &'a LlmService,
    pub git_service: &'a GitService,
    pub confirmation_registry: &'a ConfirmationRegistry,
    pub task_service: &'a TaskService,
    pub coordinator: &'a WorkflowCoordinator,
}

pub async fn run_generate_content(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &GenerateContentExecutionServices<'_>,
) -> Option<WorkflowRun> {
    let permit = WorkflowExternalLaunchPermit::prevalidated(&run);
    run_generate_content_authorized(context, run, services, || Ok(permit)).await
}

pub async fn run_generate_content_authorized<F>(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &GenerateContentExecutionServices<'_>,
    authorize_external_launch: F,
) -> Option<WorkflowRun>
where
    F: FnOnce() -> Result<WorkflowExternalLaunchPermit, BackendError>,
{
    let task_id = run.task_id.clone();
    run_generate_content_with_generator(context, run, services, move |prompt, route| async move {
        let publication = authorize_external_launch()?.begin()?;
        execute_prepared_route(context, services, &task_id, &route, prompt, publication).await
    })
    .await
}

pub async fn run_generate_content_with_generator<F, Fut>(
    context: &ProjectContext,
    run: WorkflowRun,
    services: &GenerateContentExecutionServices<'_>,
    generate: F,
) -> Option<WorkflowRun>
where
    F: FnOnce(String, WorkflowRoute) -> Fut,
    Fut: Future<Output = Result<String, BackendError>>,
{
    match execute_generate_content(context, &run, services, generate).await {
        Ok(next) => next,
        Err(error) => finish_error(&run, services, error),
    }
}

async fn execute_generate_content<F, Fut>(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &GenerateContentExecutionServices<'_>,
    generate: F,
) -> Result<Option<WorkflowRun>, BackendError>
where
    F: FnOnce(String, WorkflowRoute) -> Fut,
    Fut: Future<Output = Result<String, BackendError>>,
{
    let task_id = run.task_id.as_str();
    let sink = WorkflowStageSink::new(services.task_service, services.coordinator, task_id);
    let (artifact_type, export_type, page_paths, output_path) = generation_scope(run)?;

    sink.start(CONFIRM_SCOPE).map_err(task_error)?;
    ensure_not_cancelled(services.task_service, task_id)?;
    services
        .export_service
        .validate_workflow_scope(export_type, &page_paths)?;
    let output_path = services
        .export_service
        .validate_workflow_output_path(context, &output_path)?;
    let baseline = workflow_baseline_for_scope(context, &run.scope)?;
    if baseline.fingerprint != run.baseline_fingerprint {
        return Err(baseline_changed());
    }
    let execution_options = services
        .task_service
        .workflow_execution_options(task_id)
        .ok_or_else(|| task_error("Workflow execution options are unavailable.".into()))?;
    let initial_target_hash = FileStore.file_hash_if_exists(context, &output_path)?;
    if initial_target_hash != execution_options.existing_target_hash {
        return Err(target_changed());
    }
    if let Some(required_revision) = services
        .export_service
        .restricted_content_revision_for_pages(context, export_type, &page_paths)?
    {
        if execution_options
            .restricted_content_acknowledgement_revision
            .as_deref()
            != Some(required_revision.as_str())
        {
            return Err(BackendError::new(
                "WORKFLOW_RESTRICTED_CONTENT_ACKNOWLEDGEMENT_REQUIRED",
                "This artifact includes restricted content and requires a separate acknowledgement.",
                true,
                true,
            ));
        }
    }
    let checkpoint_hash = if initial_target_hash.is_some() {
        Some(
            services
                .git_service
                .clean_head_checkpoint(
                    context,
                    CheckpointPurpose::HighRiskOperation,
                    &format!("Before Generate Content overwrite {task_id}"),
                )?
                .commit_hash
                .ok_or_else(|| {
                    BackendError::new(
                        "GIT_HEAD_MISSING",
                        "The overwrite checkpoint has no commit hash.",
                        true,
                        true,
                    )
                })?,
        )
    } else {
        None
    };
    sink.progress(
        CONFIRM_SCOPE,
        Some(output_path.clone()),
        page_paths.len() as u64,
        Some(page_paths.len() as u64),
    )
    .map_err(task_error)?;
    sink.complete(CONFIRM_SCOPE).map_err(task_error)?;

    sink.start(READ_WIKI).map_err(task_error)?;
    ensure_not_cancelled(services.task_service, task_id)?;
    let input_snapshot = snapshot_generation_inputs(context, services, &page_paths)?;
    sink.progress(
        READ_WIKI,
        page_paths.first().cloned(),
        input_snapshot.len() as u64,
        Some(input_snapshot.len() as u64),
    )
    .map_err(task_error)?;
    sink.complete(READ_WIKI).map_err(task_error)?;

    sink.start(LOAD_TEMPLATE).map_err(task_error)?;
    ensure_not_cancelled(services.task_service, task_id)?;
    let language = services
        .settings_service
        .read_settings(context)
        .map(|settings| settings.language)
        .unwrap_or_else(|_| "en".into());
    let prompt = services.export_service.build_export_prompt_for_pages(
        context,
        export_type,
        &page_paths,
        services.search_service,
        &language,
        None,
        &ExportContentOptions::default(),
    )?;
    sink.progress(
        LOAD_TEMPLATE,
        Some(export_type.skill_folder().into()),
        1,
        Some(1),
    )
    .map_err(task_error)?;
    sink.complete(LOAD_TEMPLATE).map_err(task_error)?;

    sink.start(GENERATE_CONTENT).map_err(task_error)?;
    ensure_not_cancelled(services.task_service, task_id)?;
    let route = run.route.clone().ok_or_else(route_unavailable)?;
    if matches!(route, WorkflowRoute::Byok { .. }) {
        let required = hex_sha256(
            canonical_json(&Some(route.clone()))
                .map_err(|_| route_unavailable())?
                .as_bytes(),
        );
        if execution_options
            .remote_provider_acknowledgement_revision
            .as_deref()
            != Some(required.as_str())
        {
            return Err(BackendError::new(
                "WORKFLOW_REMOTE_PROVIDER_ACKNOWLEDGEMENT_REQUIRED",
                "This workflow sends selected content to a remote provider and requires a separate acknowledgement.",
                true,
                true,
            ));
        }
    }
    validate_prepared_route(context, services, &route)?;
    let raw = generate(prompt, route.clone()).await?;
    ensure_not_cancelled(services.task_service, task_id)?;
    if snapshot_generation_inputs(context, services, &page_paths)? != input_snapshot {
        return Err(baseline_changed());
    }
    sink.progress(
        GENERATE_CONTENT,
        Some(export_type.skill_folder().into()),
        1,
        Some(1),
    )
    .map_err(task_error)?;
    sink.complete(GENERATE_CONTENT).map_err(task_error)?;

    sink.start(ASSEMBLE_ARTIFACT).map_err(task_error)?;
    let assembled = ExportService::extract_html(&raw);
    sink.progress(
        ASSEMBLE_ARTIFACT,
        Some(output_path.clone()),
        assembled.len() as u64,
        Some(assembled.len() as u64),
    )
    .map_err(task_error)?;
    sink.complete(ASSEMBLE_ARTIFACT).map_err(task_error)?;

    sink.start(VALIDATE_ARTIFACT).map_err(task_error)?;
    let candidate = services.export_service.validate_html_artifact(&assembled)?;
    sink.progress(
        VALIDATE_ARTIFACT,
        Some(output_path.clone()),
        candidate.preview.byte_size,
        Some(candidate.preview.byte_size),
    )
    .map_err(task_error)?;
    sink.complete(VALIDATE_ARTIFACT).map_err(task_error)?;

    sink.start(WRITE_EXPORT).map_err(task_error)?;
    ensure_not_cancelled(services.task_service, task_id)?;
    let current_target_hash = FileStore.file_hash_if_exists(context, &output_path)?;
    if let Some(expected_hash) = initial_target_hash {
        let checkpoint_hash = checkpoint_hash.ok_or_else(|| {
            BackendError::new(
                "WORKFLOW_CHECKPOINT_REQUIRED",
                "An overwrite candidate requires a Git checkpoint.",
                true,
                true,
            )
        })?;
        let (pending, execution) = persist_overwrite_candidate(
            context,
            run,
            services,
            PersistedGenerateContentCandidate {
                schema_version: 1,
                task_id: task_id.into(),
                canonical_identity_key: run.canonical_identity_key.clone(),
                identity_revision: run.identity_revision.clone(),
                scope: run.scope.clone(),
                route,
                output_path: output_path.clone(),
                export_root_relative: services
                    .export_service
                    .workflow_export_root_relative(context)?,
                prepared_target_hash: expected_hash.clone(),
                review_target_hash: current_target_hash.clone(),
                review_target_html: current_target_hash
                    .as_ref()
                    .map(|_| {
                        std::fs::read_to_string(context.resolve_project_path(&output_path)?)
                            .map_err(|error| {
                                BackendError::new(
                                    "EXPORT_TARGET_READ_FAILED",
                                    error.to_string(),
                                    true,
                                    false,
                                )
                            })
                    })
                    .transpose()?,
                checkpoint_hash: checkpoint_hash.clone(),
                input_hashes: input_snapshot,
                html: candidate.html,
                preview: candidate.preview,
                title: artifact_title(context, services.search_service, export_type, &page_paths)?,
                source_path: record_source_path(export_type, &page_paths),
            },
            current_target_hash.as_deref() != Some(expected_hash.as_str()),
        )?;
        let action_id = pending.id.clone();
        if let Err(message) = sink.wait(WRITE_EXPORT, pending) {
            let _ = services
                .confirmation_registry
                .remove_exact_execution(&action_id, &execution);
            return Err(task_error(message));
        }
        return Ok(None);
    }
    if current_target_hash.is_some() {
        return Err(target_changed());
    }
    services
        .task_service
        .set_task_cancellable(task_id, false)
        .map_err(task_error)?;
    services.export_service.write_html_checked(
        context,
        &output_path,
        &candidate.html,
        WriteMode::CreateNew,
    )?;
    let title = artifact_title(context, services.search_service, export_type, &page_paths)?;
    let record = ExportService::new_validated_record(
        export_type,
        title,
        record_source_path(export_type, &page_paths),
        output_path.clone(),
        export_record_route(&route),
        Some(task_id.into()),
        candidate.preview.clone(),
    );
    let record_id = record.id.clone();
    if let Err(error) = services.export_service.append_record(context, record) {
        discard_new_artifact_if_unchanged(context, &output_path, &candidate.preview.content_hash);
        return Err(error);
    }
    let completion = (|| {
        sink.progress(WRITE_EXPORT, Some(output_path.clone()), 1, Some(1))
            .map_err(task_error)?;
        sink.complete(WRITE_EXPORT).map_err(task_error)?;

        sink.start(GENERATE_PREVIEW).map_err(task_error)?;
        let preview_path = services
            .export_service
            .resolve_existing_html_export(context, &output_path)?;
        let preview_bytes = std::fs::read(&preview_path).map_err(|error| {
            BackendError::new("EXPORT_PREVIEW_READ_FAILED", error.to_string(), true, false)
        })?;
        if hex_sha256(&preview_bytes) != candidate.preview.content_hash {
            return Err(BackendError::new(
                "EXPORT_PREVIEW_CHANGED",
                "The generated artifact changed before preview validation completed.",
                true,
                true,
            ));
        }
        sink.progress(
            GENERATE_PREVIEW,
            Some(record_id.clone()),
            candidate.preview.byte_size,
            Some(candidate.preview.byte_size),
        )
        .map_err(task_error)?;
        sink.complete(GENERATE_PREVIEW).map_err(task_error)?;

        sink.start(COMPLETE).map_err(task_error)?;
        sink.complete(COMPLETE).map_err(task_error)?;
        let (_, next) = sink
            .finish(WorkflowResult::GenerateContent {
                artifact_type,
                record_id: Some(record_id.clone()),
                output_paths: vec![output_path.clone()],
                artifact_count: Some(1),
                validation_passed: true,
            })
            .map_err(task_error)?;
        Ok(next)
    })();
    if completion.is_err() {
        let _ = services.export_service.remove_record_if_matches(
            context,
            &record_id,
            &output_path,
            &candidate.preview.content_hash,
        );
        discard_new_artifact_if_unchanged(context, &output_path, &candidate.preview.content_hash);
    }
    completion
}

async fn execute_prepared_route(
    context: &ProjectContext,
    services: &GenerateContentExecutionServices<'_>,
    task_id: &str,
    route: &WorkflowRoute,
    prompt: String,
    publication: super::super::WorkflowLaunchPublication,
) -> Result<String, BackendError> {
    match route {
        WorkflowRoute::Local { .. } => Err(route_unavailable()),
        WorkflowRoute::Agent { agent, .. } => {
            let workspace = create_agent_workspace(task_id)?;
            let _guard = WorkspaceGuard(workspace.clone());
            let invocation = AgentService::html_export_invocation(*agent, &workspace, &prompt)?;
            let result = services.agent_service.run_export_streaming(
                *agent,
                &invocation,
                services.task_service,
                task_id,
            );
            publication.started();
            result
        }
        WorkflowRoute::Byok {
            provider, model, ..
        } => {
            let config = services
                .settings_service
                .read_settings(context)?
                .llm_providers
                .into_iter()
                .find(|candidate| {
                    candidate.enabled
                        && candidate.provider == *provider
                        && candidate.model == *model
                })
                .ok_or_else(route_unavailable)?;
            let secret = crate::services::LlmService::bound_secret_for_config(
                context,
                services.secret_service,
                &config,
            )
            .map_err(|_| route_unavailable())?;
            let completion = services
                .llm_service
                .complete(&config, secret.as_deref(), &prompt);
            let result = crate::tasks::byok_progress::poll_with_progress(
                services.task_service,
                task_id,
                "Generating artifact",
                completion,
            )
            .await
            .map_err(|_| {
                crate::tasks::byok_progress::cancelled_error(
                    "WORKFLOW_CANCELLED",
                    "Generate Content was cancelled.",
                )
            });
            publication.started();
            result?
        }
    }
}

fn validate_prepared_route(
    context: &ProjectContext,
    services: &GenerateContentExecutionServices<'_>,
    route: &WorkflowRoute,
) -> Result<(), BackendError> {
    match route {
        WorkflowRoute::Local { .. } => Err(route_unavailable()),
        WorkflowRoute::Agent {
            agent,
            route_revision,
            ..
        } => {
            let settings = services.settings_service.read_settings(context)?;
            let info = services
                .agent_service
                .detect_agent(*agent, settings.agent_default == Some(*agent));
            let revision =
                canonical_json(&(agent, &info.state, &info.version, &info.executable_path))
                    .map(|value| hex_sha256(value.as_bytes()))
                    .map_err(|_| route_unavailable())?;
            (info.state == AgentDetectionState::Installed && revision == *route_revision)
                .then_some(())
                .ok_or_else(route_unavailable)
        }
        WorkflowRoute::Byok {
            provider,
            model,
            route_revision,
        } => {
            let settings = services.settings_service.read_settings(context)?;
            let config = settings
                .llm_providers
                .into_iter()
                .find(|candidate| candidate.provider == *provider && candidate.model == *model)
                .ok_or_else(route_unavailable)?;
            let binding = crate::services::LlmService::credential_binding(context, &config)?;
            let configured_secret = crate::services::LlmService::bound_secret_available(
                context,
                services.secret_service,
                &config,
            )?;
            let available = config.enabled
                && !config.model.trim().is_empty()
                && {
                    let url = config.base_url.trim().to_ascii_lowercase();
                    url.starts_with("https://") || url.starts_with("http://")
                }
                && configured_secret;
            let revision = canonical_json(&(
                config.provider,
                &config.model,
                &config.base_url,
                config.context_window,
                config.enabled,
                configured_secret,
                binding.as_ref().map(|binding| &binding.config_id),
                binding.as_ref().map(|binding| binding.revision),
            ))
            .map(|value| hex_sha256(value.as_bytes()))
            .map_err(|_| route_unavailable())?;
            (available && revision == *route_revision)
                .then_some(())
                .ok_or_else(route_unavailable)
        }
    }
}

fn generation_scope(
    run: &WorkflowRun,
) -> Result<(WorkflowArtifactType, ExportType, Vec<String>, String), BackendError> {
    let WorkflowScope::GenerateContent {
        artifact_type,
        page_paths,
        output_path,
    } = &run.scope
    else {
        return Err(BackendError::new(
            "WORKFLOW_SCOPE_KIND_MISMATCH",
            "Generate Content received a different workflow scope.",
            false,
            true,
        ));
    };
    let export_type = match artifact_type {
        WorkflowArtifactType::BeautifulRead => ExportType::BeautifulRead,
        WorkflowArtifactType::KnowledgeCard => ExportType::KnowledgeCard,
        WorkflowArtifactType::ConceptMap => ExportType::ConceptMap,
        WorkflowArtifactType::ProjectReport => ExportType::ProjectReport,
    };
    let output_path = output_path.clone().ok_or_else(|| {
        BackendError::new(
            "WORKFLOW_OUTPUT_PATH_INVALID",
            "Generate Content requires a prepared output path.",
            true,
            true,
        )
    })?;
    Ok((
        artifact_type.clone(),
        export_type,
        page_paths.clone(),
        output_path,
    ))
}

fn snapshot_generation_inputs(
    context: &ProjectContext,
    services: &GenerateContentExecutionServices<'_>,
    selected_pages: &[String],
) -> Result<BTreeMap<String, String>, BackendError> {
    let mut paths = if selected_pages.is_empty() {
        services
            .search_service
            .scan_wiki(context, &HashSet::new())?
            .pages
            .into_iter()
            .filter(|page| page.path != "wiki/log.md")
            .map(|page| page.path)
            .collect::<Vec<_>>()
    } else {
        selected_pages.to_vec()
    };
    for path in ["purpose.md", "schema.md"] {
        if context.resolve_project_path(path)?.is_file() {
            paths.push(path.into());
        }
    }
    paths.sort();
    paths.dedup();
    let mut snapshot = BTreeMap::new();
    for path in &paths {
        snapshot.insert(path.clone(), FileStore.file_hash(context, path)?);
    }
    for entry in services
        .export_service
        .workflow_resource_entries(context, &paths)?
    {
        let mut parts = entry.splitn(3, ':');
        let _ = parts.next();
        if let (Some(path), Some(hash)) = (parts.next(), parts.next()) {
            snapshot.insert(format!("resource:{path}"), hash.into());
        }
    }
    Ok(snapshot)
}

fn artifact_title(
    context: &ProjectContext,
    search: &SearchService,
    export_type: ExportType,
    pages: &[String],
) -> Result<String, BackendError> {
    if export_type == ExportType::ProjectReport {
        return Ok("Project report".into());
    }
    let Some(first) = pages.first() else {
        return Ok(export_type.skill_folder().into());
    };
    let title = search
        .read_page(context, first, &HashSet::new())?
        .meta
        .title;
    Ok(if pages.len() > 1 {
        format!("{title} + {}", pages.len() - 1)
    } else {
        title
    })
}

fn record_source_path(export_type: ExportType, pages: &[String]) -> Option<String> {
    (export_type != ExportType::ProjectReport && pages.len() == 1).then(|| pages[0].clone())
}

fn export_record_route(route: &WorkflowRoute) -> ExportRoute {
    match route {
        WorkflowRoute::Agent { .. } => ExportRoute::Agent,
        WorkflowRoute::Byok { .. } | WorkflowRoute::Local { .. } => ExportRoute::Byok,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedGenerateContentCandidate {
    schema_version: u32,
    task_id: String,
    canonical_identity_key: String,
    identity_revision: String,
    scope: WorkflowScope,
    route: WorkflowRoute,
    output_path: String,
    export_root_relative: String,
    prepared_target_hash: String,
    review_target_hash: Option<String>,
    review_target_html: Option<String>,
    checkpoint_hash: String,
    input_hashes: BTreeMap<String, String>,
    html: String,
    preview: ExportPreviewMetadata,
    title: String,
    source_path: Option<String>,
}

fn persist_overwrite_candidate(
    context: &ProjectContext,
    run: &WorkflowRun,
    services: &GenerateContentExecutionServices<'_>,
    candidate: PersistedGenerateContentCandidate,
    conflict: bool,
) -> Result<(WorkflowPendingAction, ConfirmationExecution), BackendError> {
    ensure_candidate_root_safe()?;
    let workspace = candidate_workspace(&run.task_id)?;
    if workspace.exists() {
        return Err(BackendError::new(
            "WORKFLOW_CANDIDATE_CONFLICT",
            "A candidate workspace already exists for this task.",
            true,
            false,
        ));
    }
    std::fs::create_dir_all(workspace.parent().unwrap_or(Path::new("."))).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_WRITE_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    std::fs::create_dir(&workspace).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_WRITE_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    secure_candidate_directory(&workspace)?;
    let descriptor = workspace.join("candidate.json");
    let descriptor_bytes = serde_json::to_vec(&candidate).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_WRITE_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    if let Err(error) = write_private_candidate(&descriptor, &descriptor_bytes) {
        let _ = std::fs::remove_dir_all(&workspace);
        return Err(error);
    }
    let candidate_hash = hex_sha256(&descriptor_bytes);
    let action_type = if conflict {
        PendingActionType::MergeConflict
    } else {
        PendingActionType::OverwriteFile
    };
    let action_id = format!("generate-content-overwrite-{}", run.task_id);
    let action = PendingAction {
        id: action_id.clone(),
        action_type: action_type.clone(),
        title: "Review generated artifact overwrite".into(),
        message: format!("Review overwrite of {}", candidate.output_path),
        risk_level: RiskLevel::High,
        affected_paths: vec![candidate.output_path.clone()],
        preview: Some(ActionPreview {
            summary: if conflict {
                "The target changed during generation; review the candidate against the latest file."
                    .into()
            } else {
                "The generated artifact would overwrite an existing export.".into()
            },
            before: candidate
                .review_target_hash
                .as_ref()
                .map(|hash| format!("sha256:{hash}")),
            after: Some(format!("sha256:{}", candidate.preview.content_hash)),
            diff: Some(html_diff_summary(
                candidate.review_target_html.as_deref(),
                &candidate.html,
            )),
        }),
        expires_at: None,
        checkpoint_hash: Some(candidate.checkpoint_hash.clone()),
    };
    let execution = ConfirmationExecution::GenerateContentOverwrite {
        project_id: context.project_id.clone(),
        root_path: context.root.to_string_lossy().into_owned(),
        canonical_identity_key: run.canonical_identity_key.clone(),
        identity_revision: run.identity_revision.clone(),
        task_id: run.task_id.clone(),
        action_id: action_id.clone(),
        candidate: WorkflowCandidateReference::TaskOwned {
            candidate_id: format!("{}:{candidate_hash}", run.task_id),
        },
    };
    if let Err(error) = services
        .confirmation_registry
        .register_with_execution(action, Some(execution.clone()))
    {
        let _ = std::fs::remove_dir_all(&workspace);
        return Err(error);
    }
    Ok((
        WorkflowPendingAction {
            id: action_id,
            action_type,
            risk_level: RiskLevel::High,
            affected_paths: vec![candidate.output_path],
            candidate: Some(WorkflowCandidateReference::TaskOwned {
                candidate_id: format!("{}:{candidate_hash}", run.task_id),
            }),
            expires_at: None,
            checkpoint_hash: Some(candidate.checkpoint_hash),
        },
        execution,
    ))
}

pub fn confirm_generate_content_overwrite(
    context: &ProjectContext,
    task_id: &str,
    services: &GenerateContentExecutionServices<'_>,
) -> Result<(WorkflowRun, Option<WorkflowRun>), GenerateContentConfirmationFailure> {
    let run = services
        .task_service
        .get_workflow_run(task_id)
        .ok_or_else(|| GenerateContentConfirmationFailure {
            error: BackendError::new(
                "TASK_NOT_FOUND",
                "Generate Content task not found.",
                false,
                false,
            ),
            next: None,
        })?;
    let workflow = services
        .task_service
        .workflow_execution_state(task_id)
        .ok_or_else(|| GenerateContentConfirmationFailure {
            error: BackendError::new(
                "TASK_NOT_FOUND",
                "Generate Content task not found.",
                false,
                false,
            ),
            next: None,
        })?;
    let candidate_binding = run.pending_action.as_ref().and_then(workflow_candidate_id);
    if run.display_status != crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
        || !candidate_binding.is_some_and(|candidate_id| {
            generate_content_candidate_is_valid_for_workflow(
                task_id,
                candidate_id,
                &context.root,
                &workflow,
            )
        })
    {
        let error = BackendError::new(
            "WORKFLOW_CANDIDATE_STALE",
            "The generated artifact candidate is no longer safe to apply.",
            true,
            true,
        );
        return Err(fail_confirmed_overwrite(task_id, services, error));
    }
    if run
        .pending_action
        .as_ref()
        .is_some_and(|pending| pending.action_type == PendingActionType::MergeConflict)
    {
        let error = BackendError::new(
            "WORKFLOW_OUTPUT_TARGET_CHANGED",
            "The export changed during generation and cannot be overwritten by a generic confirmation. Review it and prepare again.",
            true,
            true,
        );
        return Err(fail_confirmed_overwrite(task_id, services, error));
    }
    let expected_candidate_hash = run
        .pending_action
        .as_ref()
        .and_then(workflow_candidate_hash)
        .ok_or_else(|| GenerateContentConfirmationFailure {
            error: BackendError::new(
                "WORKFLOW_CANDIDATE_STALE",
                "The generated artifact candidate is not bound to this confirmation.",
                true,
                true,
            ),
            next: None,
        })?;
    let candidate =
        match load_generate_content_candidate(task_id, &context.root, expected_candidate_hash) {
            Some(candidate) => candidate,
            None => {
                let error = BackendError::new(
                    "WORKFLOW_CANDIDATE_STALE",
                    "The generated artifact candidate is unavailable.",
                    true,
                    true,
                );
                return Err(fail_confirmed_overwrite(task_id, services, error));
            }
        };
    let (expected_hash, previous_html) = match (
        candidate.review_target_hash.clone(),
        candidate.review_target_html.clone(),
    ) {
        (Some(hash), Some(html)) => (hash, html),
        _ => {
            return Err(fail_confirmed_overwrite(
                task_id,
                services,
                target_changed(),
            ))
        }
    };
    let validated = match services
        .export_service
        .validate_html_artifact(&candidate.html)
    {
        Ok(validated) => validated,
        Err(error) => return Err(fail_confirmed_overwrite(task_id, services, error)),
    };
    if validated.preview != candidate.preview {
        let error = BackendError::new(
            "WORKFLOW_CANDIDATE_STALE",
            "The generated artifact candidate metadata no longer matches its content.",
            false,
            true,
        );
        return Err(fail_confirmed_overwrite(task_id, services, error));
    }

    services
        .task_service
        .begin_confirmed_workflow_apply(task_id)
        .map_err(|message| GenerateContentConfirmationFailure {
            error: task_error(message),
            next: None,
        })?;
    let mut appended_record_id = None;
    let mut content_written = false;
    let apply = (|| {
        services.export_service.write_html_checked(
            context,
            &candidate.output_path,
            &candidate.html,
            WriteMode::OverwriteIfHashMatches(expected_hash),
        )?;
        content_written = true;
        let record = ExportService::new_validated_record(
            export_type_for_scope(&candidate.scope)?,
            candidate.title.clone(),
            candidate.source_path.clone(),
            candidate.output_path.clone(),
            export_record_route(&candidate.route),
            Some(task_id.into()),
            candidate.preview.clone(),
        );
        let record_id = record.id.clone();
        services.export_service.append_record(context, record)?;
        appended_record_id = Some(record_id.clone());
        let sink = WorkflowStageSink::new(services.task_service, services.coordinator, task_id);
        sink.progress(
            WRITE_EXPORT,
            Some(candidate.output_path.clone()),
            1,
            Some(1),
        )
        .map_err(task_error)?;
        sink.complete(WRITE_EXPORT).map_err(task_error)?;
        sink.start(GENERATE_PREVIEW).map_err(task_error)?;
        let preview = services
            .export_service
            .resolve_existing_html_export(context, &candidate.output_path)?;
        let bytes = std::fs::read(preview).map_err(|error| {
            BackendError::new("EXPORT_PREVIEW_READ_FAILED", error.to_string(), true, false)
        })?;
        if hex_sha256(&bytes) != candidate.preview.content_hash {
            return Err(BackendError::new(
                "EXPORT_PREVIEW_CHANGED",
                "The confirmed artifact changed before preview validation completed.",
                true,
                true,
            ));
        }
        sink.progress(
            GENERATE_PREVIEW,
            Some(record_id.clone()),
            candidate.preview.byte_size,
            Some(candidate.preview.byte_size),
        )
        .map_err(task_error)?;
        sink.complete(GENERATE_PREVIEW).map_err(task_error)?;
        sink.start(COMPLETE).map_err(task_error)?;
        sink.complete(COMPLETE).map_err(task_error)?;
        let artifact_type = artifact_type_for_scope(&candidate.scope)?;
        let (completed, next) = sink
            .finish(WorkflowResult::GenerateContent {
                artifact_type,
                record_id: Some(record_id),
                output_paths: vec![candidate.output_path.clone()],
                artifact_count: Some(1),
                validation_passed: true,
            })
            .map_err(task_error)?;
        Ok((completed, next))
    })();
    match apply {
        Ok((completed, next)) => {
            // Candidate cleanup is housekeeping after the workflow has already
            // completed and the coordinator may have claimed the next run. A
            // filesystem cleanup failure must never discard that continuation.
            let _ = discard_generate_content_candidate(task_id);
            Ok((completed, next))
        }
        Err(error) => {
            let (record_restored, content_restored) = if content_written {
                let record_restored = appended_record_id.as_deref().is_none_or(|record_id| {
                    services.export_service.remove_record_if_matches(
                        context,
                        record_id,
                        &candidate.output_path,
                        &candidate.preview.content_hash,
                    ) == Ok(true)
                });
                let content_restored = services
                    .export_service
                    .write_html_checked(
                        context,
                        &candidate.output_path,
                        &previous_html,
                        WriteMode::OverwriteIfHashMatches(candidate.preview.content_hash.clone()),
                    )
                    .is_ok();
                (record_restored, content_restored)
            } else {
                (true, true)
            };
            Err(fail_confirmed_overwrite_with_state(
                task_id,
                services,
                error,
                generate_rollback_mutation_state(
                    content_written,
                    record_restored,
                    content_restored,
                ),
            ))
        }
    }
}

#[derive(Debug)]
pub struct GenerateContentConfirmationFailure {
    pub error: BackendError,
    pub next: Option<WorkflowRun>,
}

fn fail_confirmed_overwrite(
    task_id: &str,
    services: &GenerateContentExecutionServices<'_>,
    error: BackendError,
) -> GenerateContentConfirmationFailure {
    fail_confirmed_overwrite_with_state(
        task_id,
        services,
        error,
        WorkflowProjectMutationState::NotModified,
    )
}

fn generate_rollback_mutation_state(
    content_written: bool,
    record_restored: bool,
    content_restored: bool,
) -> WorkflowProjectMutationState {
    if !content_written {
        WorkflowProjectMutationState::NotModified
    } else if record_restored && content_restored {
        WorkflowProjectMutationState::RolledBack
    } else {
        WorkflowProjectMutationState::Modified
    }
}

fn fail_confirmed_overwrite_with_state(
    task_id: &str,
    services: &GenerateContentExecutionServices<'_>,
    error: BackendError,
    project_mutation_state: WorkflowProjectMutationState,
) -> GenerateContentConfirmationFailure {
    if services
        .task_service
        .get_workflow_run(task_id)
        .is_some_and(|run| {
            run.display_status
                == crate::models::workflow::WorkflowDisplayStatus::WaitingForConfirmation
        })
    {
        let _ = services.task_service.clear_workflow_pending_action(task_id);
    }
    let _ = services.task_service.set_error(task_id, error.clone());
    let sink = WorkflowStageSink::new(services.task_service, services.coordinator, task_id);
    let next = sink
        .fail(
            WRITE_EXPORT,
            WorkflowErrorSummary {
                code: error.code.clone(),
                message_key: "workflows.error.generateContentFailed".into(),
                recoverable: error.recoverable,
                user_action_required: error.user_action_required,
                suggested_action: Some(WorkflowPrerequisiteAction::PrepareAgain),
                project_mutation_state,
            },
        )
        .ok()
        .and_then(|(_, next)| next);
    let _ = discard_generate_content_candidate(task_id);
    GenerateContentConfirmationFailure { error, next }
}

pub fn cancel_generate_content_confirmation(
    task_id: &str,
    services: &GenerateContentExecutionServices<'_>,
) -> Result<Option<WorkflowRun>, BackendError> {
    let next = services
        .coordinator
        .finish_cancelled_and_claim_next(services.task_service, task_id)
        .map(|(_, next)| next)
        .map_err(task_error)?;
    // The registry action has already been consumed. Keep the task state
    // truthful even if best-effort temporary candidate cleanup fails.
    let _ = discard_generate_content_candidate(task_id);
    Ok(next)
}

pub fn restore_generate_content_confirmation(
    context: &ProjectContext,
    run: &WorkflowRun,
    tasks: &TaskService,
    registry: &ConfirmationRegistry,
) -> Result<(), BackendError> {
    let pending = run.pending_action.as_ref().ok_or_else(|| {
        BackendError::new(
            "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
            "Generate Content has no pending confirmation to restore.",
            true,
            true,
        )
    })?;
    let workflow = tasks
        .workflow_execution_state(&run.task_id)
        .ok_or_else(|| {
            BackendError::new(
                "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
                "Generate Content execution state is unavailable.",
                true,
                true,
            )
        })?;
    let candidate_binding = workflow_candidate_id(pending).ok_or_else(|| {
        BackendError::new(
            "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
            "Generate Content candidate binding is missing.",
            true,
            true,
        )
    })?;
    if !generate_content_candidate_is_valid_for_workflow(
        &run.task_id,
        candidate_binding,
        &context.root,
        &workflow,
    ) {
        return Err(BackendError::new(
            "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
            "Generate Content candidate is no longer valid.",
            true,
            true,
        ));
    }
    let expected_candidate_hash = workflow_candidate_hash(pending).ok_or_else(|| {
        BackendError::new(
            "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
            "Generate Content candidate binding is missing.",
            true,
            true,
        )
    })?;
    let candidate =
        load_generate_content_candidate(&run.task_id, &context.root, expected_candidate_hash)
            .ok_or_else(|| {
                BackendError::new(
                    "WORKFLOW_CONFIRMATION_RECOVERY_FAILED",
                    "Generate Content candidate is unavailable.",
                    true,
                    true,
                )
            })?;
    registry.restore_with_execution(
        PendingAction {
            id: pending.id.clone(),
            action_type: pending.action_type.clone(),
            title: "Review generated artifact overwrite".into(),
            message: format!("Review overwrite of {}", candidate.output_path),
            risk_level: pending.risk_level.clone(),
            affected_paths: pending.affected_paths.clone(),
            preview: Some(ActionPreview {
                summary: if pending.action_type == PendingActionType::MergeConflict {
                    "The target changed during generation; resolve the conflict and prepare again."
                        .into()
                } else {
                    "The generated artifact would overwrite an existing export.".into()
                },
                before: candidate
                    .review_target_hash
                    .as_ref()
                    .map(|hash| format!("sha256:{hash}")),
                after: Some(format!("sha256:{}", candidate.preview.content_hash)),
                diff: Some(html_diff_summary(
                    candidate.review_target_html.as_deref(),
                    &candidate.html,
                )),
            }),
            expires_at: pending.expires_at.clone(),
            checkpoint_hash: pending.checkpoint_hash.clone(),
        },
        ConfirmationExecution::GenerateContentOverwrite {
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

fn artifact_type_for_scope(scope: &WorkflowScope) -> Result<WorkflowArtifactType, BackendError> {
    match scope {
        WorkflowScope::GenerateContent { artifact_type, .. } => Ok(artifact_type.clone()),
        _ => Err(BackendError::new(
            "WORKFLOW_SCOPE_KIND_MISMATCH",
            "Generate Content candidate has a different workflow scope.",
            false,
            true,
        )),
    }
}

fn export_type_for_scope(scope: &WorkflowScope) -> Result<ExportType, BackendError> {
    Ok(match artifact_type_for_scope(scope)? {
        WorkflowArtifactType::BeautifulRead => ExportType::BeautifulRead,
        WorkflowArtifactType::KnowledgeCard => ExportType::KnowledgeCard,
        WorkflowArtifactType::ConceptMap => ExportType::ConceptMap,
        WorkflowArtifactType::ProjectReport => ExportType::ProjectReport,
    })
}

fn html_diff_summary(before: Option<&str>, after: &str) -> String {
    let before = before.unwrap_or_default();
    let before_lines = before.lines().count();
    let after_lines = after.lines().count();
    let shared_lines = before
        .lines()
        .zip(after.lines())
        .filter(|(left, right)| left == right)
        .count();
    format!(
        "HTML summary: {} -> {} bytes; {} -> {} lines; {} aligned lines changed.",
        before.len(),
        after.len(),
        before_lines,
        after_lines,
        before_lines.max(after_lines).saturating_sub(shared_lines)
    )
}

fn workflow_candidate_hash(pending: &WorkflowPendingAction) -> Option<&str> {
    workflow_candidate_id(pending)?
        .split_once(':')
        .map(|(_, hash)| hash)
}

fn workflow_candidate_id(pending: &WorkflowPendingAction) -> Option<&str> {
    match pending.candidate.as_ref()? {
        WorkflowCandidateReference::TaskOwned { candidate_id } => Some(candidate_id),
        WorkflowCandidateReference::ProjectRelative { .. } => None,
    }
}

pub fn generate_content_candidate_is_valid_for_workflow(
    expected_task_id: &str,
    candidate_id: &str,
    project_root: &Path,
    workflow: &WorkflowExecutionState,
) -> bool {
    let Some((task_id, expected_candidate_hash)) = candidate_id.split_once(':') else {
        return false;
    };
    if task_id != expected_task_id {
        return false;
    }
    let Some(candidate) =
        load_generate_content_candidate(task_id, project_root, expected_candidate_hash)
    else {
        return false;
    };
    if workflow.kind != WorkflowKind::GenerateContent
        || workflow.canonical_identity_key != candidate.canonical_identity_key
        || workflow.identity_revision != candidate.identity_revision
        || workflow.scope != candidate.scope
        || workflow.route.as_ref() != Some(&candidate.route)
        || workflow
            .pending_action
            .as_ref()
            .and_then(|pending| pending.checkpoint_hash.as_deref())
            != Some(candidate.checkpoint_hash.as_str())
    {
        return false;
    }
    let Some(context) = candidate_context(project_root, &candidate) else {
        return false;
    };
    if ExportService::default()
        .validate_workflow_output_path(&context, &candidate.output_path)
        .is_err()
        || FileStore
            .file_hash_if_exists(&context, &candidate.output_path)
            .ok()
            .flatten()
            != candidate.review_target_hash
    {
        return false;
    }
    candidate.input_hashes.iter().all(|(path, expected)| {
        let path = path.strip_prefix("resource:").unwrap_or(path);
        let actual = FileStore.file_hash_if_exists(&context, path).ok().flatten();
        if expected == "missing" {
            actual.is_none()
        } else {
            actual.as_deref() == Some(expected.as_str())
        }
    })
}

fn candidate_context(
    project_root: &Path,
    candidate: &PersistedGenerateContentCandidate,
) -> Option<ProjectContext> {
    let export_root = Path::new(&candidate.export_root_relative);
    if export_root.file_name().and_then(|value| value.to_str()) != Some("html") {
        return None;
    }
    let exports_relative = export_root.parent()?;
    let mut context = ProjectContext::new("workflow-recovery", project_root.to_path_buf());
    context.exports_dir = context.root.join(exports_relative);
    Some(context)
}

pub fn discard_generate_content_candidate(task_id: &str) -> Result<(), BackendError> {
    let workspace = candidate_workspace(task_id)?;
    if !workspace.exists() {
        return Ok(());
    }
    let root = candidate_root();
    let canonical_root = root.canonicalize().map_err(candidate_path_error)?;
    let canonical_workspace = workspace.canonicalize().map_err(candidate_path_error)?;
    if canonical_workspace.parent() != Some(canonical_root.as_path()) {
        return Err(candidate_path_error(
            "Candidate workspace escaped its owner root.",
        ));
    }
    std::fs::remove_dir_all(canonical_workspace).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_DISCARD_FAILED",
            error.to_string(),
            true,
            false,
        )
    })
}

fn load_generate_content_candidate(
    task_id: &str,
    project_root: &Path,
    expected_candidate_hash: &str,
) -> Option<PersistedGenerateContentCandidate> {
    let workspace = candidate_workspace(task_id).ok()?;
    let descriptor = workspace.join("candidate.json");
    let root = candidate_root();
    let root_meta = std::fs::symlink_metadata(&root).ok()?;
    let workspace_meta = std::fs::symlink_metadata(&workspace).ok()?;
    let descriptor_meta = std::fs::symlink_metadata(&descriptor).ok()?;
    if !root_meta.is_dir()
        || root_meta.file_type().is_symlink()
        || !workspace_meta.is_dir()
        || workspace_meta.file_type().is_symlink()
        || !descriptor_meta.is_file()
        || descriptor_meta.file_type().is_symlink()
        || !candidate_permissions_are_private(&root_meta)
        || !candidate_permissions_are_private(&workspace_meta)
        || !candidate_permissions_are_private(&descriptor_meta)
    {
        return None;
    }
    let canonical_root = root.canonicalize().ok()?;
    let canonical_workspace = workspace.canonicalize().ok()?;
    let canonical_descriptor = descriptor.canonicalize().ok()?;
    if canonical_workspace.parent() != Some(canonical_root.as_path())
        || canonical_descriptor.parent() != Some(canonical_workspace.as_path())
    {
        return None;
    }
    let descriptor_bytes = std::fs::read(canonical_descriptor).ok()?;
    if hex_sha256(&descriptor_bytes) != expected_candidate_hash {
        return None;
    }
    let candidate: PersistedGenerateContentCandidate =
        serde_json::from_slice(&descriptor_bytes).ok()?;
    let identity = project_identity(project_root).ok()?;
    if candidate.schema_version != 1
        || candidate.task_id != task_id
        || candidate.canonical_identity_key != identity.canonical_identity_key
        || candidate.identity_revision != identity.identity_revision
        || !GitService::checkpoint_exists(project_root, &candidate.checkpoint_hash)
        || candidate.preview.content_hash != hex_sha256(candidate.html.as_bytes())
    {
        return None;
    }
    Some(candidate)
}

fn candidate_root() -> PathBuf {
    std::env::temp_dir().join("llm-wiki-desktop-generate-content")
}

fn ensure_candidate_root_safe() -> Result<(), BackendError> {
    let temp = std::env::temp_dir();
    let root = candidate_root();
    if root.exists() {
        let metadata = std::fs::symlink_metadata(&root).map_err(candidate_path_error)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(candidate_path_error(
                "Candidate root must be a real directory, not a link or file.",
            ));
        }
    } else {
        std::fs::create_dir(&root).map_err(candidate_path_error)?;
    }
    secure_candidate_directory(&root)?;
    let canonical_temp = temp.canonicalize().map_err(candidate_path_error)?;
    let canonical_root = root.canonicalize().map_err(candidate_path_error)?;
    if canonical_root.parent() != Some(canonical_temp.as_path()) {
        return Err(candidate_path_error(
            "Candidate root escaped the operating-system temporary directory.",
        ));
    }
    Ok(())
}

fn write_private_candidate(path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).map_err(|error| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_WRITE_FAILED",
            error.to_string(),
            true,
            false,
        )
    })?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| {
            BackendError::new(
                "WORKFLOW_CANDIDATE_WRITE_FAILED",
                error.to_string(),
                true,
                false,
            )
        })
}

#[cfg(unix)]
fn secure_candidate_directory(path: &Path) -> Result<(), BackendError> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(candidate_path_error)
}

#[cfg(not(unix))]
fn secure_candidate_directory(_path: &Path) -> Result<(), BackendError> {
    Ok(())
}

#[cfg(unix)]
fn candidate_permissions_are_private(metadata: &std::fs::Metadata) -> bool {
    metadata.permissions().mode() & 0o077 == 0
}

#[cfg(not(unix))]
fn candidate_permissions_are_private(_metadata: &std::fs::Metadata) -> bool {
    true
}

fn candidate_workspace(task_id: &str) -> Result<PathBuf, BackendError> {
    let canonical = uuid::Uuid::parse_str(task_id).map_err(|_| {
        BackendError::new(
            "WORKFLOW_CANDIDATE_ID_INVALID",
            "Candidate task id must be a canonical UUID.",
            false,
            true,
        )
    })?;
    if canonical.to_string() != task_id {
        return Err(BackendError::new(
            "WORKFLOW_CANDIDATE_ID_INVALID",
            "Candidate task id must use canonical UUID text.",
            false,
            true,
        ));
    }
    Ok(candidate_root().join(task_id))
}

fn create_agent_workspace(task_id: &str) -> Result<PathBuf, BackendError> {
    let workspace = std::env::temp_dir()
        .join("llm-wiki-desktop")
        .join(format!("export-{task_id}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&workspace).map_err(|error| {
        BackendError::new("EXPORT_WORKSPACE_FAILED", error.to_string(), true, false)
    })?;
    secure_candidate_directory(&workspace)?;
    Ok(workspace)
}

struct WorkspaceGuard(PathBuf);

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn discard_new_artifact_if_unchanged(
    context: &ProjectContext,
    output_path: &str,
    expected_hash: &str,
) {
    let _ = FileStore.remove_if_hash_matches(context, output_path, expected_hash);
}

fn ensure_not_cancelled(tasks: &TaskService, task_id: &str) -> Result<(), BackendError> {
    if tasks.is_cancelled(task_id)
        || tasks.get_task(task_id).is_some_and(|task| {
            matches!(task.status, TaskStatus::Cancelling | TaskStatus::Cancelled)
        })
    {
        Err(BackendError::new(
            "WORKFLOW_CANCELLED",
            "Generate Content was cancelled.",
            true,
            false,
        ))
    } else {
        Ok(())
    }
}

fn baseline_changed() -> BackendError {
    BackendError::new(
        "WORKFLOW_INPUT_BASELINE_CHANGED",
        "Selected Wiki content or resources changed during generation. Prepare and run again.",
        true,
        true,
    )
}

fn target_changed() -> BackendError {
    BackendError::new(
        "WORKFLOW_OUTPUT_TARGET_CHANGED",
        "The output target changed after preparation. Prepare and review the target again.",
        true,
        true,
    )
}

fn route_unavailable() -> BackendError {
    BackendError::new(
        "WORKFLOW_ROUTE_UNAVAILABLE",
        "The prepared Generate Content route is no longer available. Review Settings and retry.",
        true,
        true,
    )
}

fn finish_error(
    run: &WorkflowRun,
    services: &GenerateContentExecutionServices<'_>,
    error: BackendError,
) -> Option<WorkflowRun> {
    let _ = services
        .task_service
        .append_log(&run.task_id, LogLevel::Error, error.message.clone());
    let cancelled = services.task_service.is_cancelled(&run.task_id)
        || services
            .task_service
            .get_task(&run.task_id)
            .is_some_and(|task| {
                matches!(task.status, TaskStatus::Cancelling | TaskStatus::Cancelled)
            });
    let outcome = if cancelled {
        let _ = discard_generate_content_candidate(&run.task_id);
        services
            .coordinator
            .finish_cancelled_and_claim_next(services.task_service, &run.task_id)
    } else {
        let _ = services.task_service.set_error(&run.task_id, error.clone());
        let sink =
            WorkflowStageSink::new(services.task_service, services.coordinator, &run.task_id);
        let refreshed = services.task_service.get_workflow_run(&run.task_id);
        let current = refreshed
            .as_ref()
            .and_then(|run| {
                run.stages
                    .iter()
                    .find(|stage| {
                        stage.status == crate::models::workflow::WorkflowStageStatus::Running
                    })
                    .or_else(|| {
                        run.stages.iter().find(|stage| {
                            stage.status == crate::models::workflow::WorkflowStageStatus::Pending
                        })
                    })
                    .map(|stage| stage.id.clone())
            })
            .unwrap_or_else(|| CONFIRM_SCOPE.into());
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
                message_key: if error.code.contains("BASELINE")
                    || error.code.contains("TARGET_CHANGED")
                {
                    "workflows.error.prepareAgain".into()
                } else if error.code.contains("ROUTE")
                    || error.code.contains("PROVIDER")
                    || error.code.contains("AGENT")
                {
                    "workflows.error.configureExecutionRoute".into()
                } else {
                    "workflows.error.generateContentFailed".into()
                },
                recoverable: error.recoverable,
                user_action_required: error.user_action_required,
                suggested_action: if error.code.contains("BASELINE")
                    || error.code.contains("TARGET_CHANGED")
                {
                    Some(WorkflowPrerequisiteAction::PrepareAgain)
                } else if error.code.contains("ROUTE")
                    || error.code.contains("PROVIDER")
                    || error.code.contains("AGENT")
                {
                    Some(WorkflowPrerequisiteAction::ConfigureExecutionRoute)
                } else {
                    None
                },
                project_mutation_state: WorkflowProjectMutationState::Unknown,
            },
        )
    };
    outcome.ok().and_then(|(_, next)| next)
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

fn candidate_path_error(error: impl std::fmt::Display) -> BackendError {
    BackendError::new(
        "WORKFLOW_CANDIDATE_PATH_INVALID",
        error.to_string(),
        false,
        true,
    )
}
