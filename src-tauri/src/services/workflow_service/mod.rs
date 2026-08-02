pub mod coordinator;
pub mod fingerprint;
pub mod overview;
pub mod persistence;
pub mod preferences;
pub mod preparation;
pub mod runners;
pub mod stage_sink;

use std::sync::{Arc, RwLock};

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::models::workflow::{
    WorkflowDisplayStatus, WorkflowKind, WorkflowRun, WorkflowStartOutcome, WorkflowsOverview,
};
use crate::services::{AgentService, SecretService, SettingsService};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

pub use coordinator::{EnqueueWorkflow, WorkflowCoordinator};
pub use fingerprint::{canonical_json, workflow_fingerprint};
pub use overview::WorkflowOverviewService;
pub(crate) use persistence::recover_workflow;
pub use persistence::{project_identity, ProjectWorkflowIdentity};
pub use preferences::{WorkflowPreference, WorkflowPreferences};
pub use preparation::{
    overview_prerequisites, workflow_baseline_for_scope, workflow_stages, PrepareWorkflowInput,
    ValidatedWorkflowStart, WorkflowAccessSnapshot, WorkflowPreparationEnvironment,
    WorkflowPreparationService,
};
pub use runners::generate_content::{
    cancel_generate_content_confirmation, confirm_generate_content_overwrite,
    discard_generate_content_candidate, generate_content_candidate_is_valid_for_workflow,
    restore_generate_content_confirmation, run_generate_content,
    run_generate_content_with_generator, GenerateContentConfirmationFailure,
    GenerateContentExecutionServices, GenerateContentRunner,
};
pub use runners::health_check::{
    run_health_check, run_health_check_with_deep, HealthCheckExecutionServices, HealthCheckRunner,
};
pub use runners::update_wiki::{
    confirm_update_wiki_review, discard_update_wiki_candidate, persist_update_wiki_review,
    restore_update_wiki_confirmation, run_update_wiki, update_wiki_candidate_is_valid,
    update_wiki_decision_review, UpdateWikiConfirmationFailure, UpdateWikiExecutionServices,
    UpdateWikiRunner,
};
pub use stage_sink::WorkflowStageSink;

pub trait WorkflowRunner: Send + Sync {
    fn kind(&self) -> WorkflowKind;

    /// Dispatch one already-created run. Implementations own their worker
    /// lifetime and report every stage through `WorkflowStageSink`; they must
    /// not create another task or resolve a different execution route.
    fn start(&self, run: WorkflowRun);
}

pub struct WorkflowService {
    pub coordinator: WorkflowCoordinator,
    pub preparation: WorkflowPreparationService,
    pub preferences: WorkflowPreferences,
    pub overview: WorkflowOverviewService,
    runners: RwLock<Vec<Arc<dyn WorkflowRunner>>>,
}

impl Default for WorkflowService {
    fn default() -> Self {
        Self {
            coordinator: WorkflowCoordinator::default(),
            preparation: WorkflowPreparationService::default(),
            preferences: WorkflowPreferences::default(),
            overview: WorkflowOverviewService,
            runners: RwLock::new(Vec::new()),
        }
    }
}

impl WorkflowService {
    pub fn register_runner(&self, runner: Arc<dyn WorkflowRunner>) -> Result<(), BackendError> {
        let kind = runner.kind();
        let mut runners = self.runners.write().map_err(|_| runner_lock_error())?;
        runners.retain(|existing| existing.kind() != kind);
        runners.push(runner);
        Ok(())
    }

    pub fn no_project_overview(&self) -> WorkflowsOverview {
        self.overview.no_project()
    }

    pub fn prepare(
        &self,
        environment: &WorkflowPreparationEnvironment<'_>,
        input: PrepareWorkflowInput,
    ) -> Result<crate::models::workflow::WorkflowPreparation, BackendError> {
        self.preparation
            .prepare(&self.preferences, environment, input)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &self,
        context: &ProjectContext,
        access: WorkflowAccessSnapshot,
        settings_service: &SettingsService,
        secret_service: &SecretService,
        agent_service: &AgentService,
        tasks: &TaskService,
        preparation_id: &str,
        preparation_revision: &str,
    ) -> Result<WorkflowStartOutcome, BackendError> {
        self.start_with_acknowledgements(
            context,
            access,
            settings_service,
            secret_service,
            agent_service,
            tasks,
            preparation_id,
            preparation_revision,
            false,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_with_acknowledgements(
        &self,
        context: &ProjectContext,
        access: WorkflowAccessSnapshot,
        settings_service: &SettingsService,
        secret_service: &SecretService,
        agent_service: &AgentService,
        tasks: &TaskService,
        preparation_id: &str,
        preparation_revision: &str,
        acknowledge_restricted_content: bool,
        acknowledge_remote_provider: bool,
    ) -> Result<WorkflowStartOutcome, BackendError> {
        if let Some(task_id) = self
            .preparation
            .started_task_id(preparation_id, preparation_revision)?
        {
            let run = tasks.get_workflow_run(&task_id).ok_or_else(|| {
                BackendError::new(
                    "WORKFLOW_PREPARATION_STALE",
                    "The workflow created from this preparation is no longer available.",
                    true,
                    true,
                )
            })?;
            let identity = project_identity(&context.root).map_err(|message| {
                BackendError::new("WORKFLOW_IDENTITY_FAILED", message, true, false)
            })?;
            if run.canonical_identity_key != identity.canonical_identity_key
                || run.identity_revision != identity.identity_revision
            {
                return Err(BackendError::new(
                    "WORKFLOW_PREPARATION_STALE",
                    "The preparation belongs to a different project identity.",
                    true,
                    true,
                ));
            }
            return Ok(WorkflowStartOutcome::Existing { run });
        }
        let environment = WorkflowPreparationEnvironment {
            context,
            access,
            settings_service,
            secret_service,
            agent_service,
        };
        let mut validated = self.preparation.validate_for_start(
            &environment,
            preparation_id,
            preparation_revision,
        )?;
        if let crate::models::workflow::WorkflowScope::GenerateContent {
            artifact_type,
            page_paths,
            ..
        } = &validated.preparation.scope
        {
            use crate::models::export::ExportType;
            let export_type = match artifact_type {
                crate::models::workflow::WorkflowArtifactType::BeautifulRead => {
                    ExportType::BeautifulRead
                }
                crate::models::workflow::WorkflowArtifactType::KnowledgeCard => {
                    ExportType::KnowledgeCard
                }
                crate::models::workflow::WorkflowArtifactType::ConceptMap => ExportType::ConceptMap,
                crate::models::workflow::WorkflowArtifactType::ProjectReport => {
                    ExportType::ProjectReport
                }
            };
            let required_revision = crate::services::ExportService::default()
                .restricted_content_revision_for_pages(context, export_type, page_paths)?;
            if required_revision.is_some() && !acknowledge_restricted_content {
                return Err(BackendError::new(
                    "WORKFLOW_RESTRICTED_CONTENT_ACKNOWLEDGEMENT_REQUIRED",
                    "This artifact includes restricted content and requires a separate acknowledgement.",
                    true,
                    true,
                ));
            }
            validated
                .execution_options
                .restricted_content_acknowledgement_revision = required_revision;
        }
        let remote_route = preparation::route_is_remote_provider(
            context,
            settings_service,
            validated.preparation.route.as_ref(),
        )?;
        let disclosure_revision = preparation::REMOTE_PROVIDER_DISCLOSURE_REVISION;
        let disclosure_acknowledged =
            settings_service.is_remote_provider_disclosure_acknowledged(disclosure_revision)?;
        if remote_route && !disclosure_acknowledged && !acknowledge_remote_provider {
            return Err(BackendError::new(
                "WORKFLOW_REMOTE_PROVIDER_ACKNOWLEDGEMENT_REQUIRED",
                "This workflow sends selected content to a remote provider and requires a separate acknowledgement.",
                true,
                true,
            ));
        }
        if remote_route && !disclosure_acknowledged {
            settings_service.acknowledge_remote_provider_disclosure(disclosure_revision)?;
        }
        let remote_revision = remote_route.then(|| disclosure_revision.to_string());
        validated
            .execution_options
            .remote_provider_acknowledgement_revision = remote_revision;
        let runner = self
            .runners
            .read()
            .map_err(|_| runner_lock_error())?
            .iter()
            .find(|runner| runner.kind() == validated.preparation.kind)
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    "WORKFLOW_RUNNER_UNAVAILABLE",
                    "This workflow runner is not available in the current migration batch.",
                    true,
                    false,
                )
            })?;
        let outcome = self
            .coordinator
            .enqueue(
                tasks,
                EnqueueWorkflow {
                    project_id: context.project_id.clone(),
                    project_root: context.root.clone(),
                    task_state_root: validated.task_state_root.clone(),
                    title: validated.title.clone(),
                    kind: validated.preparation.kind.clone(),
                    scope: validated.preparation.scope.clone(),
                    route: validated.preparation.route.clone(),
                    baseline_fingerprint: validated.preparation.baseline.fingerprint.clone(),
                    execution_options: validated.execution_options.clone(),
                    stages: validated.stages.clone(),
                    retry: None,
                },
            )
            .map_err(|message| BackendError::new("WORKFLOW_START_FAILED", message, true, false))?;
        let run = match &outcome {
            WorkflowStartOutcome::Created { run } | WorkflowStartOutcome::Existing { run } => run,
        };
        self.preparation
            .mark_started(preparation_id, preparation_revision, &run.task_id)?;
        if self
            .preparation
            .remember_started(&self.preferences, context, &validated)
            .is_err()
        {
            let _ = tasks.append_log(
                &run.task_id,
                LogLevel::Warn,
                "Workflow preferences could not be saved; this run is unaffected.".into(),
            );
        }
        if matches!(&outcome, WorkflowStartOutcome::Created { .. })
            && run.display_status == WorkflowDisplayStatus::Running
        {
            runner.start(run.clone());
        }
        Ok(outcome)
    }

    pub fn dispatch_claimed_run(&self, run: &WorkflowRun) -> Result<bool, BackendError> {
        if run.display_status != WorkflowDisplayStatus::Running {
            return Ok(false);
        }
        let runner = self
            .runners
            .read()
            .map_err(|_| runner_lock_error())?
            .iter()
            .find(|runner| runner.kind() == run.kind)
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    "WORKFLOW_RUNNER_UNAVAILABLE",
                    "The claimed workflow runner is unavailable.",
                    true,
                    false,
                )
            })?;
        runner.start(run.clone());
        Ok(true)
    }

    pub fn project_overview(
        &self,
        context: &ProjectContext,
        access: WorkflowAccessSnapshot,
        settings_service: &SettingsService,
        secret_service: &SecretService,
        agent_service: &AgentService,
        tasks: &TaskService,
    ) -> Result<WorkflowsOverview, BackendError> {
        let identity = project_identity(&context.root).map_err(|message| {
            BackendError::new("WORKFLOW_IDENTITY_FAILED", message, true, false)
        })?;
        let access_summary = crate::models::workflow::WorkflowProjectAccessSummary {
            project_id: context.project_id.clone(),
            canonical_identity_key: identity.canonical_identity_key,
            identity_revision: identity.identity_revision,
            trust: access.trust,
            filesystem_access: access.filesystem_access,
            persistence: access.persistence,
            git_state: access.git_state,
        };
        let prerequisites = overview_prerequisites(
            &self.preferences,
            &WorkflowPreparationEnvironment {
                context,
                access: WorkflowAccessSnapshot {
                    trust: access_summary.trust.clone(),
                    trust_kind: access.trust_kind,
                    filesystem_access: access_summary.filesystem_access.clone(),
                    persistence: access_summary.persistence.clone(),
                    git_state: access_summary.git_state.clone(),
                    authority_revision: access.authority_revision,
                },
                settings_service,
                secret_service,
                agent_service,
            },
        )?;
        self.overview
            .for_project(context, access_summary, &prerequisites, tasks)
    }
}

fn runner_lock_error() -> BackendError {
    BackendError::new(
        "WORKFLOW_RUNNER_REGISTRY_LOCKED",
        "Workflow runners are temporarily unavailable.",
        true,
        false,
    )
}
