pub mod coordinator;
pub mod fingerprint;
pub mod launch_registry;
pub mod overview;
pub mod persistence;
pub mod preferences;
pub mod preparation;
pub mod runners;
pub mod stage_sink;

use std::sync::{Arc, RwLock};

#[cfg(test)]
use std::sync::Mutex;

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::models::workflow::{
    WorkflowDisplayStatus, WorkflowKind, WorkflowRun, WorkflowStartOutcome, WorkflowsOverview,
};
use crate::services::{AgentService, SecretService, SettingsService};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

pub use coordinator::{
    EnqueueWorkflow, WorkflowCoordinator, WorkflowDispatchFailure, WorkflowTrustTransition,
};
pub use fingerprint::{canonical_json, workflow_fingerprint};
pub use launch_registry::{
    WorkflowExternalLaunchPermit, WorkflowLaunchCloseBarrier, WorkflowLaunchPublication,
    WorkflowLaunchRegistry,
};
pub use overview::WorkflowOverviewService;
pub(crate) use persistence::recover_workflow;
pub use persistence::{project_identity, ProjectWorkflowIdentity};
pub use preferences::{WorkflowPreference, WorkflowPreferences};
pub use preparation::{
    resolve_workflow_persistence_binding, workflow_baseline_for_scope, workflow_stages,
    PrepareWorkflowInput, ValidatedWorkflowStart, WorkflowAccessSnapshot,
    WorkflowPersistenceBinding, WorkflowPreparationEnvironment, WorkflowPreparationService,
};
pub use runners::generate_content::{
    cancel_generate_content_confirmation, confirm_generate_content_overwrite,
    discard_generate_content_candidate, generate_content_candidate_is_valid_for_workflow,
    restore_generate_content_confirmation, run_generate_content, run_generate_content_authorized,
    run_generate_content_with_generator, GenerateContentConfirmationFailure,
    GenerateContentExecutionServices, GenerateContentRunner,
};
pub use runners::health_check::{
    run_health_check, run_health_check_authorized, run_health_check_with_deep,
    HealthCheckExecutionServices, HealthCheckRunner,
};
#[cfg(feature = "gui")]
pub(crate) use runners::update_wiki::update_wiki_decision_review_for_workflow;
pub use runners::update_wiki::{
    confirm_update_wiki_review, discard_update_wiki_candidate, persist_update_wiki_review,
    restore_update_wiki_confirmation, run_update_wiki, run_update_wiki_authorized,
    update_wiki_candidate_is_valid, update_wiki_decision_review, UpdateWikiConfirmationFailure,
    UpdateWikiExecutionServices, UpdateWikiRunner,
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
    #[cfg(test)]
    start_after_prepared_lookup: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Default for WorkflowService {
    fn default() -> Self {
        Self {
            coordinator: WorkflowCoordinator::default(),
            preparation: WorkflowPreparationService::default(),
            preferences: WorkflowPreferences::default(),
            overview: WorkflowOverviewService,
            runners: RwLock::new(Vec::new()),
            #[cfg(test)]
            start_after_prepared_lookup: Mutex::new(None),
        }
    }
}

impl WorkflowService {
    #[cfg(test)]
    fn set_start_after_prepared_lookup_hook(&self, hook: Box<dyn FnOnce() + Send>) {
        *self
            .start_after_prepared_lookup
            .lock()
            .expect("lock poisoned") = Some(hook);
    }

    #[cfg(test)]
    fn run_start_after_prepared_lookup_hook(&self) {
        let hook = self
            .start_after_prepared_lookup
            .lock()
            .expect("lock poisoned")
            .take();
        if let Some(hook) = hook {
            hook();
        }
    }

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
        self.start_with_acknowledgements_impl(
            context,
            access,
            settings_service,
            secret_service,
            agent_service,
            tasks,
            preparation_id,
            preparation_revision,
            acknowledge_restricted_content,
            acknowledge_remote_provider,
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn enqueue_with_acknowledgements(
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
        self.start_with_acknowledgements_impl(
            context,
            access,
            settings_service,
            secret_service,
            agent_service,
            tasks,
            preparation_id,
            preparation_revision,
            acknowledge_restricted_content,
            acknowledge_remote_provider,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn start_with_acknowledgements_impl(
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
        dispatch_immediately: bool,
    ) -> Result<WorkflowStartOutcome, BackendError> {
        let identity = project_identity(&context.root).map_err(|message| {
            BackendError::new("WORKFLOW_IDENTITY_FAILED", message, true, false)
        })?;
        let preparation_lookup = self.preparation.lookup_for_start(
            preparation_id,
            preparation_revision,
            &identity.canonical_identity_key,
            &identity.identity_revision,
        )?;
        match preparation_lookup {
            preparation::PreparationStartLookup::Started(task_id) => {
                return existing_preparation_run(tasks, context, &task_id);
            }
            preparation::PreparationStartLookup::Missing => {
                if let Some(outcome) = recover_preparation_run(
                    tasks,
                    context,
                    &identity,
                    preparation_id,
                    preparation_revision,
                )? {
                    return Ok(outcome);
                }
            }
            preparation::PreparationStartLookup::Prepared => {
                #[cfg(test)]
                self.run_start_after_prepared_lookup_hook();
            }
        }
        let environment = WorkflowPreparationEnvironment {
            context,
            access,
            settings_service,
            secret_service,
            agent_service,
        };
        let mut validated = match self.preparation.validate_for_start(
            &environment,
            preparation_id,
            preparation_revision,
        ) {
            Ok(validated) => validated,
            Err(error) if error.code == "WORKFLOW_PREPARATION_STALE" => {
                match self.preparation.lookup_for_start(
                    preparation_id,
                    preparation_revision,
                    &identity.canonical_identity_key,
                    &identity.identity_revision,
                )? {
                    preparation::PreparationStartLookup::Started(task_id) => {
                        return existing_preparation_run(tasks, context, &task_id);
                    }
                    preparation::PreparationStartLookup::Missing => {
                        if let Some(outcome) = recover_preparation_run(
                            tasks,
                            context,
                            &identity,
                            preparation_id,
                            preparation_revision,
                        )? {
                            return Ok(outcome);
                        }
                    }
                    preparation::PreparationStartLookup::Prepared => {}
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
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
        let _runner = self
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
            .enqueue_for_owner(
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
                &validated.preparation.project_access.canonical_identity_key,
                &validated.preparation.project_access.identity_revision,
            )
            .map_err(|message| BackendError::new("WORKFLOW_START_FAILED", message, true, false))?;
        let run = match &outcome {
            WorkflowStartOutcome::Created { run } | WorkflowStartOutcome::Existing { run } => run,
        };
        self.preparation.mark_started(
            preparation_id,
            preparation_revision,
            &run.task_id,
            &run.canonical_identity_key,
            &run.identity_revision,
        )?;
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
        if dispatch_immediately
            && matches!(&outcome, WorkflowStartOutcome::Created { .. })
            && run.display_status == WorkflowDisplayStatus::Running
        {
            self.dispatch_claimed_run(tasks, run)?;
        }
        Ok(outcome)
    }

    pub fn dispatch_claimed_run(
        &self,
        tasks: &TaskService,
        run: &WorkflowRun,
    ) -> Result<bool, BackendError> {
        let current = tasks.get_workflow_run(&run.task_id).ok_or_else(|| {
            BackendError::new(
                "WORKFLOW_DISPATCH_INVARIANT",
                "The claimed workflow no longer exists.",
                false,
                true,
            )
        })?;
        if tasks.is_cancelled(&run.task_id) {
            let (_, next) = self
                .coordinator
                .reject_claimed_dispatch(
                    tasks,
                    &run.task_id,
                    WorkflowDispatchFailure::stale(
                        "WORKFLOW_DISPATCH_CANCELLED",
                        "workflows.error.prepareAgain",
                    ),
                )
                .map_err(|message| runner_dispatch_error(&message))?;
            if let Some(next) = next {
                self.dispatch_claimed_run(tasks, &next)?;
            }
            return Ok(false);
        }
        if current.display_status != WorkflowDisplayStatus::Running
            || current.canonical_identity_key != run.canonical_identity_key
            || current.identity_revision != run.identity_revision
            || current.kind != run.kind
            || current.fingerprint != run.fingerprint
        {
            if current.display_status == WorkflowDisplayStatus::Running {
                let (_, next) = self
                    .coordinator
                    .reject_claimed_dispatch(
                        tasks,
                        &run.task_id,
                        WorkflowDispatchFailure::invariant(
                            "WORKFLOW_DISPATCH_INVARIANT",
                            "workflows.error.prepareAgain",
                        ),
                    )
                    .map_err(|message| runner_dispatch_error(&message))?;
                if let Some(next) = next {
                    self.dispatch_claimed_run(tasks, &next)?;
                }
            }
            return Ok(false);
        }
        let runner = self
            .runners
            .read()
            .map_err(|_| runner_lock_error())?
            .iter()
            .find(|runner| runner.kind() == run.kind)
            .cloned();
        let Some(runner) = runner else {
            let (_, next) = self
                .coordinator
                .reject_claimed_dispatch(
                    tasks,
                    &run.task_id,
                    WorkflowDispatchFailure::stale(
                        "WORKFLOW_RUNNER_UNAVAILABLE",
                        "workflows.error.configureExecutionRoute",
                    ),
                )
                .map_err(|message| runner_dispatch_error(&message))?;
            if let Some(next) = next {
                self.dispatch_claimed_run(tasks, &next)?;
            }
            return Ok(false);
        };
        runner.start(current);
        Ok(true)
    }

    pub fn reject_claimed_dispatch(
        &self,
        tasks: &TaskService,
        task_id: &str,
        failure: WorkflowDispatchFailure,
    ) -> Result<WorkflowRun, BackendError> {
        let (run, next) = self
            .coordinator
            .reject_claimed_dispatch(tasks, task_id, failure)
            .map_err(|message| runner_dispatch_error(&message))?;
        if let Some(next) = next {
            self.dispatch_claimed_run(tasks, &next)?;
        }
        Ok(run)
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
        let evaluation = preparation::overview_evaluation_snapshot(
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
            .for_project(access_summary, &evaluation, tasks)
    }
}

fn existing_preparation_run(
    tasks: &TaskService,
    context: &ProjectContext,
    task_id: &str,
) -> Result<WorkflowStartOutcome, BackendError> {
    let run = tasks.get_workflow_run(task_id).ok_or_else(|| {
        BackendError::new(
            "WORKFLOW_PREPARATION_STALE",
            "The workflow created from this preparation is no longer available.",
            true,
            true,
        )
    })?;
    current_preparation_run(context, run)
}

fn recover_preparation_run(
    tasks: &TaskService,
    context: &ProjectContext,
    identity: &ProjectWorkflowIdentity,
    preparation_id: &str,
    preparation_revision: &str,
) -> Result<Option<WorkflowStartOutcome>, BackendError> {
    let run = tasks.find_workflow_run_by_execution_options(
        &identity.canonical_identity_key,
        &identity.identity_revision,
        |options| {
            options
                .preparation_fingerprint
                .as_ref()
                .is_some_and(|fingerprint| {
                    preparation::preparation_revision_for(preparation_id, fingerprint)
                        == preparation_revision
                })
        },
    );
    run.map(|run| current_preparation_run(context, run))
        .transpose()
}

fn current_preparation_run(
    context: &ProjectContext,
    run: WorkflowRun,
) -> Result<WorkflowStartOutcome, BackendError> {
    let identity = project_identity(&context.root)
        .map_err(|message| BackendError::new("WORKFLOW_IDENTITY_FAILED", message, true, false))?;
    if run.canonical_identity_key != identity.canonical_identity_key
        || run.identity_revision != identity.identity_revision
    {
        return Err(BackendError::new(
            "WORKFLOW_PREPARATION_STALE",
            "The preparation belongs to a replaced project identity.",
            true,
            true,
        ));
    }
    Ok(WorkflowStartOutcome::Existing { run })
}

fn runner_lock_error() -> BackendError {
    BackendError::new(
        "WORKFLOW_RUNNER_REGISTRY_LOCKED",
        "Workflow runners are temporarily unavailable.",
        true,
        false,
    )
}

fn runner_dispatch_error(message: &str) -> BackendError {
    BackendError::new(
        "WORKFLOW_DISPATCH_FINALIZATION_FAILED",
        message,
        true,
        false,
    )
}

#[cfg(test)]
mod batch_one_start_race_tests {
    use super::*;
    use crate::models::project::ProjectTrustKind;
    use crate::models::workflow::{
        HealthCheckMode, WorkflowFilesystemAccess, WorkflowGitState, WorkflowPersistenceMode,
        WorkflowProjectTrust, WorkflowScope,
    };
    use std::sync::mpsc;

    struct NoopHealthRunner;

    impl WorkflowRunner for NoopHealthRunner {
        fn kind(&self) -> WorkflowKind {
            WorkflowKind::HealthCheck
        }

        fn start(&self, _run: WorkflowRun) {}
    }

    fn trusted_access() -> WorkflowAccessSnapshot {
        WorkflowAccessSnapshot {
            trust: WorkflowProjectTrust::Trusted,
            trust_kind: Some(ProjectTrustKind::Native),
            filesystem_access: WorkflowFilesystemAccess::Writable,
            persistence: WorkflowPersistenceMode::Persistent,
            git_state: WorkflowGitState::Clean,
            authority_revision: "batch-one-race-authority".into(),
        }
    }

    #[test]
    fn prepared_to_started_race_recovers_existing_deterministically() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project.path().join("wiki")).unwrap();
        std::fs::write(project.path().join("wiki/page.md"), "# race\n").unwrap();
        let context = Arc::new(ProjectContext::new(
            "batch-one-race",
            project.path().to_path_buf(),
        ));
        let config = tempfile::tempdir().unwrap();
        let settings = Arc::new(SettingsService::with_config_dir(
            config.path().to_path_buf(),
        ));
        let secrets = Arc::new(SecretService::memory());
        let agents = Arc::new(AgentService::default());
        let tasks = Arc::new(TaskService::default());
        let service = Arc::new(WorkflowService::default());
        service.register_runner(Arc::new(NoopHealthRunner)).unwrap();
        let preparation = service
            .prepare(
                &WorkflowPreparationEnvironment {
                    context: &context,
                    access: trusted_access(),
                    settings_service: &settings,
                    secret_service: &secrets,
                    agent_service: &agents,
                },
                PrepareWorkflowInput {
                    kind: WorkflowKind::HealthCheck,
                    scope: Some(WorkflowScope::HealthCheck {
                        mode: HealthCheckMode::LocalQuick,
                    }),
                    route_selection: None,
                },
            )
            .unwrap();

        let (lookup_reached_tx, lookup_reached_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        service.set_start_after_prepared_lookup_hook(Box::new(move || {
            lookup_reached_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        }));
        let racing_service = Arc::clone(&service);
        let racing_context = Arc::clone(&context);
        let racing_settings = Arc::clone(&settings);
        let racing_secrets = Arc::clone(&secrets);
        let racing_agents = Arc::clone(&agents);
        let racing_tasks = Arc::clone(&tasks);
        let racing_preparation = preparation.clone();
        let racing = std::thread::spawn(move || {
            racing_service.start(
                &racing_context,
                trusted_access(),
                &racing_settings,
                &racing_secrets,
                &racing_agents,
                &racing_tasks,
                &racing_preparation.preparation_id,
                &racing_preparation.preparation_revision,
            )
        });

        lookup_reached_rx.recv().unwrap();
        let created = service
            .start(
                &context,
                trusted_access(),
                &settings,
                &secrets,
                &agents,
                &tasks,
                &preparation.preparation_id,
                &preparation.preparation_revision,
            )
            .unwrap();
        let created_task_id = match created {
            WorkflowStartOutcome::Created { run } => run.task_id,
            WorkflowStartOutcome::Existing { .. } => panic!("controlled winner must create"),
        };
        release_tx.send(()).unwrap();
        let recovered = racing.join().unwrap().unwrap();
        match recovered {
            WorkflowStartOutcome::Existing { run } => assert_eq!(run.task_id, created_task_id),
            WorkflowStartOutcome::Created { .. } => panic!("racing start must recover Existing"),
        }
        assert_eq!(tasks.list_workflow_runs().len(), 1);
    }
}
