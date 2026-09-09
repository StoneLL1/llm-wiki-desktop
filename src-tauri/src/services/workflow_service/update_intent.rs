//! Update Wiki separates display queries, durable intent, and execution binding.
use super::{fingerprint::hex_sha256, *};
use crate::app_state::ProjectTaskMutationPermit;
use crate::errors::BackendError;
use crate::models::{
    agent::AgentKind, compile::SourceVersionRef, paths::ProjectContext, workflow::*,
};
use crate::services::import_v2::source_registry::SourceRegistry;
use crate::services::{CompileLegacyAdapter, FileStore, SettingsService};
use crate::tasks::TaskService;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWikiOptions {
    pub routes: Vec<WorkflowRouteSelection>,
    pub default_route: Option<WorkflowRouteSelection>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWikiSource {
    pub source_id: String,
    pub version_id: String,
    pub title: String,
    pub consumed: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWikiSourcePage {
    pub sources: Vec<UpdateWikiSource>,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub unavailable: usize,
}

fn error(code: &str, message: &str) -> BackendError {
    BackendError::new(code, message, true, true)
}

impl WorkflowService {
    /// Configuration facts only: no credentials, executable attestation, or subprocesses.
    pub fn update_wiki_options(
        &self,
        context: &ProjectContext,
        settings: &SettingsService,
    ) -> Result<UpdateWikiOptions, BackendError> {
        let settings = settings.read_settings(context)?;
        let providers = settings
            .llm_providers
            .iter()
            .filter(|p| p.enabled && !p.model.trim().is_empty())
            .map(|p| WorkflowRouteSelection::Byok {
                provider: p.provider,
            })
            .collect::<Vec<_>>();
        let default_route = settings
            .agent_default
            .map(|agent| WorkflowRouteSelection::Agent { agent })
            .or_else(|| (providers.len() == 1).then(|| providers[0].clone()));
        let mut routes = AgentKind::ALL
            .into_iter()
            .map(|agent| WorkflowRouteSelection::Agent { agent })
            .collect::<Vec<_>>();
        routes.extend(providers);
        Ok(UpdateWikiOptions {
            routes,
            default_route,
        })
    }

    /// A paged metadata query. A broken individual manifest cannot poison unrelated rows.
    pub fn list_update_wiki_sources(
        &self,
        context: &ProjectContext,
        query: &str,
        offset: usize,
    ) -> Result<UpdateWikiSourcePage, BackendError> {
        let (rows, unavailable) = source_catalog(context, None)?;
        let query = query.to_lowercase();
        let rows = rows
            .into_iter()
            .filter(|(row, _)| {
                row.title.to_lowercase().contains(&query)
                    || row.source_id.to_lowercase().contains(&query)
            })
            .map(|(row, _)| row)
            .collect::<Vec<_>>();
        let total = rows.len();
        let next_offset = (offset.saturating_add(100) < total).then(|| offset + 100);
        Ok(UpdateWikiSourcePage {
            sources: rows.into_iter().skip(offset).take(100).collect(),
            total,
            next_offset,
            unavailable,
        })
    }

    pub(crate) fn enqueue_update_wiki(
        &self,
        permit: &ProjectTaskMutationPermit<'_>,
        tasks: &TaskService,
        settings: &SettingsService,
        request: UpdateWikiRequest,
    ) -> Result<WorkflowStartOutcome, BackendError> {
        validate_request(&request)?;
        let context = permit.context();
        let access = permit.workflow_access();
        if access.trust != WorkflowProjectTrust::Trusted
            || access.filesystem_access != WorkflowFilesystemAccess::Writable
        {
            return Err(error(
                "WORKFLOW_PROJECT_ACCESS_REQUIRED",
                "Trust a writable knowledge base before updating the Wiki.",
            ));
        }
        let identity =
            project_identity(&context.root).map_err(|e| error("WORKFLOW_IDENTITY_FAILED", &e))?;
        if let Some(run) = tasks.find_workflow_run_by_execution_options(
            &identity.canonical_identity_key,
            &identity.identity_revision,
            |options| {
                options
                    .update_request
                    .as_ref()
                    .is_some_and(|old| old.request_id == request.request_id)
            },
        ) {
            if tasks
                .workflow_execution_options(&run.task_id)
                .and_then(|o| o.update_request)
                .as_ref()
                != Some(&request)
            {
                return Err(error(
                    "WORKFLOW_REQUEST_ID_REUSED",
                    "This request ID already belongs to different update choices.",
                ));
            }
            return Ok(WorkflowStartOutcome::Existing { run });
        }
        let route = configured_route(context, settings, request.route_selection.as_ref())?;
        let config_revision = match &route {
            WorkflowRoute::Agent { route_revision, .. }
            | WorkflowRoute::Byok { route_revision, .. }
            | WorkflowRoute::Local { route_revision } => route_revision.clone(),
        };
        let remote = preparation::route_is_remote_provider(context, settings, Some(&route))?;
        let disclosure = preparation::REMOTE_PROVIDER_DISCLOSURE_REVISION;
        if remote
            && !request.acknowledge_remote_provider
            && !settings.is_remote_provider_disclosure_acknowledged(disclosure)?
        {
            return Err(error(
                "WORKFLOW_REMOTE_PROVIDER_ACKNOWLEDGEMENT_REQUIRED",
                "Confirm sending the selected content to the configured provider.",
            ));
        }
        let source_versions = match &request.selection {
            UpdateWikiSelection::Automatic => Vec::new(),
            UpdateWikiSelection::Selected { source_versions } => source_versions.clone(),
        };
        let retry = request
            .retry_of_task_id
            .as_ref()
            .map(|id| {
                let original = tasks.get_workflow_run(id).ok_or_else(|| {
                    error(
                        "WORKFLOW_SCOPE_REVIEW_RETRY_INVALID",
                        "The previous scope review is unavailable.",
                    )
                })?;
                scope_review_retry_link(&original, &identity, &WorkflowKind::UpdateWiki)
            })
            .transpose()?;
        let request_id = request.request_id.clone();
        let baseline = format!("update-intent:{request_id}");
        self.coordinator
            .enqueue_for_owner(
                tasks,
                EnqueueWorkflow {
                    project_id: context.project_id.clone(),
                    project_root: context.root.clone(),
                    task_state_root: resolve_workflow_persistence_binding(
                        context,
                        access.persistence,
                    )?
                    .task_state_root,
                    title: "Update Wiki".into(),
                    kind: WorkflowKind::UpdateWiki,
                    scope: WorkflowScope::UpdateWiki {
                        mode: request.mode.clone(),
                        source_versions,
                    },
                    route: Some(route),
                    baseline_fingerprint: baseline,
                    execution_options: WorkflowExecutionOptions {
                        preparation_revision: request_id,
                        update_config_revision: Some(config_revision),
                        update_request: Some(request),
                        remote_provider_acknowledgement_revision: remote.then(|| disclosure.into()),
                        ..Default::default()
                    },
                    stages: workflow_stages(&WorkflowKind::UpdateWiki),
                    retry,
                },
                &identity.canonical_identity_key,
                &identity.identity_revision,
            )
            .map_err(|e| error("WORKFLOW_START_FAILED", &e))
    }

    /// Executed by the existing task worker, never by form navigation or admission.
    pub fn bind_update_wiki_inputs(
        &self,
        environment: &WorkflowPreparationEnvironment<'_>,
        tasks: &TaskService,
        run: &WorkflowRun,
    ) -> Result<WorkflowRun, BackendError> {
        let Some(intent) = tasks
            .workflow_execution_options(&run.task_id)
            .and_then(|o| o.update_request)
        else {
            return Ok(run.clone());
        };
        if !run.baseline_fingerprint.starts_with("update-intent:") {
            return Ok(run.clone());
        }
        let sink = WorkflowStageSink::new(tasks, &self.coordinator, &run.task_id);
        sink.start("analyze_sources")
            .map_err(|e| error("WORKFLOW_START_FAILED", &e))?;
        if tasks.is_cancelled(&run.task_id) {
            return Err(error("WORKFLOW_CANCELLED", "Update cancelled."));
        }
        let selected = match &intent.selection {
            UpdateWikiSelection::Automatic => None,
            UpdateWikiSelection::Selected { source_versions } => Some(source_versions.as_slice()),
        };
        let (catalog, unavailable) = source_catalog(environment.context, selected)?;
        if selected.is_none() && unavailable > 0 {
            return Err(error("WORKFLOW_SOURCE_UNAVAILABLE", "Some source metadata is unavailable. Select the sources to update or repair their metadata."));
        }
        let refs = catalog
            .into_iter()
            .filter(|(row, _)| intent.mode == UpdateWikiMode::FullRecompile || !row.consumed)
            .map(|(_, reference)| WorkflowSourceVersionRef {
                source_id: reference.source_id,
                version_id: reference.version_id,
            })
            .collect::<Vec<_>>();
        let chosen = route_selection(run.route.as_ref())
            .ok_or_else(|| error("WORKFLOW_ROUTE_UNAVAILABLE", "Choose an execution route."))?;
        let config = configured_route(
            environment.context,
            environment.settings_service,
            Some(&chosen),
        )?;
        let config_revision = match &config {
            WorkflowRoute::Agent { route_revision, .. }
            | WorkflowRoute::Byok { route_revision, .. }
            | WorkflowRoute::Local { route_revision } => route_revision,
        };
        if tasks
            .workflow_execution_options(&run.task_id)
            .and_then(|o| o.update_config_revision)
            .as_ref()
            != Some(config_revision)
        {
            return Err(error(
                "WORKFLOW_ROUTE_CHANGED",
                "The execution configuration changed while queued. Review it before retrying.",
            ));
        }
        let route = if refs.is_empty() {
            config
        } else {
            preparation::resolve_update_execution_route(environment, &chosen)?
        };
        if tasks.is_cancelled(&run.task_id) {
            return Err(error("WORKFLOW_CANCELLED", "Update cancelled."));
        }
        let scope = WorkflowScope::UpdateWiki {
            mode: intent.mode,
            source_versions: refs,
        };
        let baseline = workflow_baseline_for_scope(environment.context, &scope)?.fingerprint;
        tasks
            .mutate_workflow(&run.task_id, |task, state| {
                if task.status != crate::models::task::TaskStatus::Running
                    || state.fingerprint != run.fingerprint
                {
                    return Err("Update changed or was cancelled before input binding".into());
                }
                state.scope = scope;
                state.route = Some(route);
                state.baseline_fingerprint = baseline;
                state.fingerprint = workflow_fingerprint(
                    &state.canonical_identity_key,
                    &state.identity_revision,
                    &state.kind,
                    &state.scope,
                    &state.execution_options,
                    &state.route,
                    &state.baseline_fingerprint,
                )?;
                Ok(())
            })
            .map_err(|e| error("WORKFLOW_START_FAILED", &e))
    }
}

fn validate_request(request: &UpdateWikiRequest) -> Result<(), BackendError> {
    if uuid::Uuid::parse_str(&request.request_id).is_err() {
        return Err(error(
            "WORKFLOW_REQUEST_INVALID",
            "A valid update request ID is required.",
        ));
    }
    if let UpdateWikiSelection::Selected { source_versions } = &request.selection {
        if source_versions.is_empty() {
            return Err(error(
                "WORKFLOW_SOURCE_SELECTION_EMPTY",
                "Select a source or use automatic selection.",
            ));
        }
        let mut ids = std::collections::HashSet::new();
        for source in source_versions {
            if source.source_id.is_empty()
                || source.version_id.is_empty()
                || !ids.insert(&source.source_id)
            {
                return Err(error(
                    "WORKFLOW_SOURCE_SELECTION_INVALID",
                    "Select exactly one version per source.",
                ));
            }
        }
    }
    Ok(())
}
fn route_selection(route: Option<&WorkflowRoute>) -> Option<WorkflowRouteSelection> {
    match route {
        Some(WorkflowRoute::Agent { agent, .. }) => {
            Some(WorkflowRouteSelection::Agent { agent: *agent })
        }
        Some(WorkflowRoute::Byok { provider, .. }) => Some(WorkflowRouteSelection::Byok {
            provider: *provider,
        }),
        _ => None,
    }
}
fn configured_route(
    context: &ProjectContext,
    settings: &SettingsService,
    selection: Option<&WorkflowRouteSelection>,
) -> Result<WorkflowRoute, BackendError> {
    let settings = settings.read_settings(context)?;
    let configured = settings
        .llm_providers
        .iter()
        .filter(|p| p.enabled && !p.model.trim().is_empty())
        .collect::<Vec<_>>();
    let selection = selection
        .cloned()
        .or_else(|| {
            settings
                .agent_default
                .map(|agent| WorkflowRouteSelection::Agent { agent })
        })
        .or_else(|| {
            (configured.len() == 1).then(|| WorkflowRouteSelection::Byok {
                provider: configured[0].provider,
            })
        })
        .ok_or_else(|| {
            error(
                "WORKFLOW_ROUTE_UNAVAILABLE",
                "Choose an execution route in Update Wiki.",
            )
        })?;
    match selection {
        WorkflowRouteSelection::Agent { agent } => Ok(WorkflowRoute::Agent {
            agent,
            model: None,
            route_revision: "configured-agent-v1".into(),
        }),
        WorkflowRouteSelection::Byok { provider } => {
            let config = configured
                .into_iter()
                .find(|p| p.provider == provider)
                .ok_or_else(|| {
                    error(
                        "WORKFLOW_ROUTE_UNAVAILABLE",
                        "Configure this provider before updating.",
                    )
                })?;
            Ok(WorkflowRoute::Byok {
                provider,
                model: config.model.clone(),
                route_revision: hex_sha256(
                    canonical_json(config)
                        .map_err(|e| error("WORKFLOW_ROUTE_INVALID", &e))?
                        .as_bytes(),
                ),
            })
        }
    }
}

fn source_catalog(
    context: &ProjectContext,
    selected: Option<&[WorkflowSourceVersionRef]>,
) -> Result<(Vec<(UpdateWikiSource, SourceVersionRef)>, usize), BackendError> {
    if !context.app_dir.join("source-index-v2.json").is_file() {
        let legacy = CompileLegacyAdapter::list(context)?;
        let rows = legacy
            .into_iter()
            .filter(|s| {
                selected.is_none_or(|refs| {
                    refs.iter().any(|r| {
                        r.source_id == s.reference.source_id
                            && r.version_id == s.reference.version_id
                    })
                })
            })
            .map(|s| {
                (
                    UpdateWikiSource {
                        source_id: s.reference.source_id.clone(),
                        version_id: s.reference.version_id.clone(),
                        title: s.project_path,
                        consumed: s.already_consumed,
                    },
                    s.reference,
                )
            })
            .collect::<Vec<_>>();
        if selected.is_some_and(|refs| rows.len() != refs.len()) {
            return Err(error(
                "WORKFLOW_SOURCE_SCOPE_STALE",
                "A selected source version is no longer available.",
            ));
        }
        return Ok((rows, 0));
    }
    let index = SourceRegistry::read_index(context, &FileStore)?;
    let mut ids = match selected {
        Some(refs) => refs.iter().map(|r| r.source_id.clone()).collect::<Vec<_>>(),
        None => index
            .by_content_hash
            .values()
            .chain(index.by_locator.values())
            .map(|p| p.source_id.clone())
            .collect(),
    };
    ids.sort();
    ids.dedup();
    let mut rows = Vec::new();
    let mut unavailable = 0;
    for id in ids {
        let read = || -> Result<_, BackendError> {
            let path = context.layout.source_paths()?.manifest(&id)?;
            let manifest = SourceRegistry::read_manifest(context, &FileStore, &path)?;
            if manifest.source_id != id {
                return Err(error(
                    "WORKFLOW_SOURCE_UNAVAILABLE",
                    "Source identity does not match its manifest.",
                ));
            }
            let version = manifest
                .versions
                .iter()
                .find(|v| v.version_id == manifest.current_version_id)
                .ok_or_else(|| {
                    error(
                        "WORKFLOW_SOURCE_UNAVAILABLE",
                        "Source metadata has no current version.",
                    )
                })?;
            if selected.is_some_and(|refs| {
                !refs
                    .iter()
                    .any(|r| r.source_id == id && r.version_id == version.version_id)
            }) {
                return Err(error(
                    "WORKFLOW_SOURCE_SCOPE_STALE",
                    "A selected source version changed. Select its current version.",
                ));
            }
            let consumed = manifest.compiled_consumptions.iter().any(|c| {
                c.version_id == version.version_id && c.content_hash == version.content_hash
            });
            Ok((
                UpdateWikiSource {
                    source_id: id.clone(),
                    version_id: version.version_id.clone(),
                    title: manifest.title.clone(),
                    consumed,
                },
                SourceVersionRef {
                    source_id: id.clone(),
                    version_id: version.version_id.clone(),
                    content_hash: version.content_hash.clone(),
                },
            ))
        };
        match read() {
            Ok(row) => rows.push(row),
            Err(e) if selected.is_some() => return Err(e),
            Err(_) => unavailable += 1,
        }
    }
    Ok((rows, unavailable))
}

#[cfg(test)]
mod tests;
