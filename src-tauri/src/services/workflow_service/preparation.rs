use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};
use std::sync::RwLock;

use chrono::{Duration, Utc};
use serde::Serialize;

use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::compile::SourceVersionRef;
use crate::models::export::ExportType;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::project::ProjectTrustKind;
use crate::models::workflow::{
    HealthCheckMode, UpdateWikiMode, WorkflowArtifactType, WorkflowBaselineSummary,
    WorkflowExecutionOptions, WorkflowFilesystemAccess, WorkflowGitPolicy, WorkflowGitState,
    WorkflowKind, WorkflowOutputSummary, WorkflowPersistenceMode, WorkflowPreparation,
    WorkflowPrerequisite, WorkflowPrerequisiteAction, WorkflowProjectAccessSummary,
    WorkflowProjectTrust, WorkflowRoute, WorkflowRouteSelection, WorkflowScope,
    WorkflowSourceVersionRef, WorkflowStage, WorkflowStageStatus, WORKFLOW_SCHEMA_VERSION,
};
use crate::services::{
    AgentService, CompileService, ExportService, FileStore, SecretService, SettingsService,
};

use super::fingerprint::{canonical_json, hex_sha256};
use super::persistence::project_identity;
use super::preferences::{WorkflowPreference, WorkflowPreferences};

const PREPARATION_TTL_MINUTES: i64 = 15;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowAccessSnapshot {
    pub trust: WorkflowProjectTrust,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_kind: Option<ProjectTrustKind>,
    pub filesystem_access: WorkflowFilesystemAccess,
    pub persistence: WorkflowPersistenceMode,
    pub git_state: WorkflowGitState,
    pub authority_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowPersistenceBinding {
    pub mode: WorkflowPersistenceMode,
    pub task_state_root: Option<std::path::PathBuf>,
}

pub fn resolve_workflow_persistence_binding(
    context: &ProjectContext,
    mode: WorkflowPersistenceMode,
) -> Result<WorkflowPersistenceBinding, BackendError> {
    if mode == WorkflowPersistenceMode::MemoryOnly {
        return Ok(WorkflowPersistenceBinding {
            mode,
            task_state_root: None,
        });
    }
    let Some(relative_root) = context.layout.task_state_root.as_deref() else {
        return Ok(WorkflowPersistenceBinding {
            mode: WorkflowPersistenceMode::MemoryOnly,
            task_state_root: None,
        });
    };
    Ok(WorkflowPersistenceBinding {
        mode,
        task_state_root: Some(context.resolve_project_path(relative_root)?),
    })
}

pub struct WorkflowPreparationEnvironment<'a> {
    pub context: &'a ProjectContext,
    pub access: WorkflowAccessSnapshot,
    pub settings_service: &'a SettingsService,
    pub secret_service: &'a SecretService,
    pub agent_service: &'a AgentService,
}

#[derive(Debug, Clone)]
pub struct PrepareWorkflowInput {
    pub kind: WorkflowKind,
    pub scope: Option<WorkflowScope>,
    pub route_selection: Option<WorkflowRouteSelection>,
}

#[derive(Debug, Clone)]
pub struct ValidatedWorkflowStart {
    pub preparation: WorkflowPreparation,
    pub execution_options: WorkflowExecutionOptions,
    pub stages: Vec<WorkflowStage>,
    pub title: String,
    pub task_state_root: Option<std::path::PathBuf>,
    pub preparation_fingerprint: String,
}

#[derive(Debug, Clone)]
struct PreparedRecord {
    preparation: WorkflowPreparation,
    authority_revision: String,
    route_selection: Option<WorkflowRouteSelection>,
    execution_options: WorkflowExecutionOptions,
    preparation_fingerprint: String,
    expires_at: chrono::DateTime<Utc>,
    started_task_id: Option<String>,
}

#[derive(Debug, Clone)]
struct PreparationSnapshot {
    project_access: WorkflowProjectAccessSummary,
    authority_revision: String,
    scope: WorkflowScope,
    baseline: WorkflowBaselineSummary,
    route: Option<WorkflowRoute>,
    prerequisites: Vec<WorkflowPrerequisite>,
    output: WorkflowOutputSummary,
    git_policy: WorkflowGitPolicy,
    execution_options: WorkflowExecutionOptions,
    preparation_fingerprint: String,
    available_source_versions: Vec<WorkflowSourceVersionRef>,
    available_wiki_pages: Vec<String>,
    available_routes: Vec<WorkflowRouteSelection>,
}

#[derive(Default)]
pub struct WorkflowPreparationService {
    records: RwLock<HashMap<String, PreparedRecord>>,
}

pub fn overview_prerequisites(
    preferences: &WorkflowPreferences,
    environment: &WorkflowPreparationEnvironment<'_>,
) -> Result<Vec<(WorkflowKind, Option<WorkflowPrerequisite>, String)>, BackendError> {
    [
        WorkflowKind::UpdateWiki,
        WorkflowKind::HealthCheck,
        WorkflowKind::GenerateContent,
    ]
    .into_iter()
    .map(|kind| {
        let mut snapshot = build_snapshot(
            environment,
            &PrepareWorkflowInput {
                kind: kind.clone(),
                scope: None,
                route_selection: None,
            },
        )?;
        if let Some(previous) = preferences
            .load(
                environment.context,
                &snapshot.project_access.canonical_identity_key,
                &snapshot.project_access.identity_revision,
                &snapshot.project_access.persistence,
            )?
            .into_iter()
            .find(|entry| entry.kind == kind)
        {
            let remembered = build_snapshot(
                environment,
                &PrepareWorkflowInput {
                    kind: kind.clone(),
                    scope: Some(previous.scope),
                    route_selection: route_selection(&previous.route),
                },
            );
            match remembered {
                Ok(remembered) => snapshot = remembered,
                Err(error) if stale_remembered_scope(&error) => {}
                Err(error) => return Err(error),
            }
        }
        let prerequisite = snapshot
            .prerequisites
            .into_iter()
            .min_by_key(|item| prerequisite_priority(&item.action));
        Ok((kind, prerequisite, snapshot.baseline.fingerprint))
    })
    .collect()
}

impl WorkflowPreparationService {
    pub fn prepare(
        &self,
        preferences: &WorkflowPreferences,
        environment: &WorkflowPreparationEnvironment<'_>,
        input: PrepareWorkflowInput,
    ) -> Result<WorkflowPreparation, BackendError> {
        let mut snapshot = build_snapshot(environment, &input)?;
        let previous = preferences
            .load(
                environment.context,
                &snapshot.project_access.canonical_identity_key,
                &snapshot.project_access.identity_revision,
                &snapshot.project_access.persistence,
            )?
            .into_iter()
            .find(|entry| entry.kind == input.kind);
        let remembered_input = previous.as_ref().and_then(|entry| {
            input.scope.is_none().then(|| PrepareWorkflowInput {
                kind: input.kind.clone(),
                scope: Some(entry.scope.clone()),
                route_selection: input
                    .route_selection
                    .clone()
                    .or_else(|| route_selection(&entry.route)),
            })
        });
        let mut applied_remembered_input = None;
        if let Some(remembered_input) = remembered_input.as_ref() {
            match build_snapshot(environment, remembered_input) {
                Ok(remembered) => {
                    snapshot = remembered;
                    applied_remembered_input = Some(remembered_input.clone());
                }
                Err(error) if stale_remembered_scope(&error) => {}
                Err(error) => return Err(error),
            }
        }
        let quick_rerun_eligible = previous.as_ref().is_some_and(|entry| {
            entry.preparation_fingerprint == snapshot.preparation_fingerprint
                && entry.scope == snapshot.scope
                && entry.route == snapshot.route
                && entry.baseline_fingerprint == snapshot.baseline.fingerprint
                && !snapshot.prerequisites.iter().any(|item| item.blocking)
        });
        let preparation_id = uuid::Uuid::new_v4().to_string();
        let preparation_revision = hex_sha256(
            format!(
                "workflow-preparation-v1\n{}\n{}",
                preparation_id, snapshot.preparation_fingerprint
            )
            .as_bytes(),
        );
        let preparation = WorkflowPreparation {
            schema_version: WORKFLOW_SCHEMA_VERSION,
            preparation_id: preparation_id.clone(),
            preparation_revision: preparation_revision.clone(),
            project_access: snapshot.project_access,
            kind: input.kind,
            scope: snapshot.scope,
            baseline: snapshot.baseline,
            route: snapshot.route,
            prerequisites: snapshot.prerequisites,
            output: snapshot.output,
            git_policy: snapshot.git_policy,
            requires_scope_confirmation: !quick_rerun_eligible,
            quick_rerun_eligible,
            available_source_versions: snapshot.available_source_versions,
            available_wiki_pages: snapshot.available_wiki_pages,
            available_routes: snapshot.available_routes,
        };
        let record = PreparedRecord {
            preparation: preparation.clone(),
            authority_revision: snapshot.authority_revision,
            route_selection: applied_remembered_input
                .and_then(|remembered| remembered.route_selection)
                .or(input.route_selection),
            execution_options: WorkflowExecutionOptions {
                preparation_revision,
                ..snapshot.execution_options
            },
            preparation_fingerprint: snapshot.preparation_fingerprint,
            expires_at: Utc::now() + Duration::minutes(PREPARATION_TTL_MINUTES),
            started_task_id: None,
        };
        self.records
            .write()
            .map_err(|_| preparation_lock_error())?
            .insert(preparation_id, record);
        Ok(preparation)
    }

    pub fn started_task_id(
        &self,
        preparation_id: &str,
        preparation_revision: &str,
    ) -> Result<Option<String>, BackendError> {
        let records = self.records.read().map_err(|_| preparation_lock_error())?;
        let Some(record) = records.get(preparation_id) else {
            return Ok(None);
        };
        if record.preparation.preparation_revision != preparation_revision {
            return Err(stale_preparation_error());
        }
        Ok(record.started_task_id.clone())
    }

    pub fn mark_started(
        &self,
        preparation_id: &str,
        preparation_revision: &str,
        task_id: &str,
    ) -> Result<(), BackendError> {
        let mut records = self.records.write().map_err(|_| preparation_lock_error())?;
        let record = records
            .get_mut(preparation_id)
            .ok_or_else(stale_preparation_error)?;
        if record.preparation.preparation_revision != preparation_revision {
            return Err(stale_preparation_error());
        }
        match record.started_task_id.as_deref() {
            Some(existing) if existing != task_id => Err(stale_preparation_error()),
            Some(_) => Ok(()),
            None => {
                record.started_task_id = Some(task_id.into());
                Ok(())
            }
        }
    }

    pub fn validate_for_start(
        &self,
        environment: &WorkflowPreparationEnvironment<'_>,
        preparation_id: &str,
        preparation_revision: &str,
    ) -> Result<ValidatedWorkflowStart, BackendError> {
        let record = self
            .records
            .read()
            .map_err(|_| preparation_lock_error())?
            .get(preparation_id)
            .cloned()
            .ok_or_else(stale_preparation_error)?;
        if record.expires_at < Utc::now()
            || record.preparation.preparation_revision != preparation_revision
        {
            return Err(stale_preparation_error());
        }
        let refreshed = build_snapshot(
            environment,
            &PrepareWorkflowInput {
                kind: record.preparation.kind.clone(),
                scope: Some(record.preparation.scope.clone()),
                route_selection: record.route_selection.clone(),
            },
        )?;
        if refreshed.project_access != record.preparation.project_access
            || refreshed.authority_revision != record.authority_revision
            || refreshed.scope != record.preparation.scope
            || refreshed.baseline.fingerprint != record.preparation.baseline.fingerprint
            || refreshed.route != record.preparation.route
            || refreshed.prerequisites != record.preparation.prerequisites
            || refreshed.output != record.preparation.output
            || refreshed.git_policy != record.preparation.git_policy
            || refreshed.preparation_fingerprint != record.preparation_fingerprint
        {
            return Err(stale_preparation_error());
        }
        if refreshed.prerequisites.iter().any(|item| item.blocking) {
            return Err(BackendError::new(
                "WORKFLOW_PREREQUISITES_BLOCKING",
                "Workflow prerequisites must be resolved before starting.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "action": WorkflowPrerequisiteAction::PrepareAgain
            })));
        }
        Ok(ValidatedWorkflowStart {
            preparation: record.preparation.clone(),
            execution_options: record.execution_options,
            stages: workflow_stages(&record.preparation.kind),
            title: workflow_title(&record.preparation.kind).into(),
            task_state_root: resolve_workflow_persistence_binding(
                environment.context,
                record.preparation.project_access.persistence.clone(),
            )?
            .task_state_root,
            preparation_fingerprint: record.preparation_fingerprint,
        })
    }

    pub fn remember_started(
        &self,
        preferences: &WorkflowPreferences,
        context: &ProjectContext,
        start: &ValidatedWorkflowStart,
    ) -> Result<(), BackendError> {
        preferences.remember(
            context,
            &start.preparation.project_access.canonical_identity_key,
            &start.preparation.project_access.identity_revision,
            &start.preparation.project_access.persistence,
            WorkflowPreference {
                kind: start.preparation.kind.clone(),
                scope: start.preparation.scope.clone(),
                route: start.preparation.route.clone(),
                baseline_fingerprint: start.preparation.baseline.fingerprint.clone(),
                preparation_fingerprint: start.preparation_fingerprint.clone(),
                saved_at: String::new(),
            },
        )
    }
}

fn build_snapshot(
    environment: &WorkflowPreparationEnvironment<'_>,
    input: &PrepareWorkflowInput,
) -> Result<PreparationSnapshot, BackendError> {
    let identity = project_identity(&environment.context.root)
        .map_err(|message| BackendError::new("WORKFLOW_IDENTITY_FAILED", message, true, false))?;
    let project_access = WorkflowProjectAccessSummary {
        project_id: environment.context.project_id.clone(),
        canonical_identity_key: identity.canonical_identity_key,
        identity_revision: identity.identity_revision,
        trust: environment.access.trust.clone(),
        filesystem_access: environment.access.filesystem_access.clone(),
        persistence: environment.access.persistence.clone(),
        git_state: environment.access.git_state.clone(),
    };
    let source_versions = CompileService::list_source_versions(environment.context)?;
    let resolved_sources = if source_versions.is_empty() {
        Vec::new()
    } else {
        CompileService::resolve_source_versions(environment.context, &source_versions)?
    };
    let wiki_pages = list_wiki_pages(environment.context)?;
    let route_catalog = RouteCatalog::load(environment)?;
    let available_source_versions = source_versions
        .iter()
        .map(|source| WorkflowSourceVersionRef {
            source_id: source.source_id.clone(),
            version_id: source.version_id.clone(),
        })
        .collect();
    let available_wiki_pages = wiki_pages.clone();
    let available_routes = route_catalog.available_selections();
    let default_route =
        resolve_external_route(input.route_selection.as_ref(), &route_catalog, false);
    let scope = normalize_scope(
        environment.context,
        &input.kind,
        input.scope.as_ref(),
        &source_versions,
        &resolved_sources,
        &wiki_pages,
        project_access.trust == WorkflowProjectTrust::Trusted && default_route.route.is_some(),
    )?;
    let route_resolution = resolve_route(&scope, input.route_selection.as_ref(), &route_catalog);
    let route = route_resolution.route;
    let output = output_summary(environment.context, &scope)?;
    let git_policy = git_policy(environment.context, &scope)?;
    let baseline = capture_baseline(environment.context, &scope, &source_versions)?;
    let mut prerequisites = prerequisites(
        &scope,
        &project_access,
        &route,
        &source_versions,
        &wiki_pages,
        &git_policy,
        route_resolution.prerequisite_action,
    );
    let restricted_content_revision = match &scope {
        WorkflowScope::GenerateContent {
            artifact_type,
            page_paths,
            ..
        } => {
            let export_type = match artifact_type {
                WorkflowArtifactType::BeautifulRead => ExportType::BeautifulRead,
                WorkflowArtifactType::KnowledgeCard => ExportType::KnowledgeCard,
                WorkflowArtifactType::ConceptMap => ExportType::ConceptMap,
                WorkflowArtifactType::ProjectReport => ExportType::ProjectReport,
            };
            ExportService::default().restricted_content_revision_for_pages(
                environment.context,
                export_type,
                page_paths,
            )?
        }
        _ => None,
    };
    if restricted_content_revision.is_some() {
        prerequisites.push(WorkflowPrerequisite {
            code: "WORKFLOW_RESTRICTED_CONTENT_ACKNOWLEDGEMENT_REQUIRED".into(),
            message_key: "workflows.prerequisite.acknowledgeRestrictedContent".into(),
            blocking: false,
            action: WorkflowPrerequisiteAction::AcknowledgeRestrictedContent,
        });
    }
    if route_requires_remote_acknowledgement(
        environment.context,
        environment.settings_service,
        route.as_ref(),
    )? {
        prerequisites.push(WorkflowPrerequisite {
            code: "WORKFLOW_REMOTE_PROVIDER_ACKNOWLEDGEMENT_REQUIRED".into(),
            message_key: "workflows.prerequisite.acknowledgeRemoteProvider".into(),
            blocking: false,
            action: WorkflowPrerequisiteAction::AcknowledgeRemoteProvider,
        });
    }
    prerequisites.sort_by(|left, right| left.code.cmp(&right.code));
    prerequisites.dedup_by(|left, right| left.code == right.code);
    let existing_target_hash = match &scope {
        WorkflowScope::GenerateContent { output_path, .. } => output_path
            .as_deref()
            .map(|path| FileStore.file_hash_if_exists(environment.context, path))
            .transpose()?
            .flatten(),
        _ => None,
    };
    let execution_options = WorkflowExecutionOptions {
        preparation_revision: "pending".into(),
        existing_target_hash,
        restricted_content_acknowledgement_revision: None,
        remote_provider_acknowledgement_revision: None,
    };
    let preparation_fingerprint = preparation_fingerprint(
        &project_access,
        &scope,
        &baseline,
        &route,
        &prerequisites,
        &output,
        &git_policy,
        &execution_options,
    )?;
    Ok(PreparationSnapshot {
        project_access,
        authority_revision: environment.access.authority_revision.clone(),
        scope,
        baseline,
        route,
        prerequisites,
        output,
        git_policy,
        execution_options,
        preparation_fingerprint,
        available_source_versions,
        available_wiki_pages,
        available_routes,
    })
}

struct RouteCatalog {
    default_agent: Option<AgentKind>,
    agents: HashMap<AgentKind, AgentRouteCandidate>,
    providers: Vec<ProviderRouteCandidate>,
}

struct AgentRouteCandidate {
    available: bool,
    revision: String,
}

struct ProviderRouteCandidate {
    config: LlmProviderConfig,
    available: bool,
    revision: String,
}

impl RouteCatalog {
    fn available_selections(&self) -> Vec<WorkflowRouteSelection> {
        let mut selections = AgentKind::ALL
            .into_iter()
            .filter(|kind| {
                self.agents
                    .get(kind)
                    .is_some_and(|candidate| candidate.available)
            })
            .map(|agent| WorkflowRouteSelection::Agent { agent })
            .collect::<Vec<_>>();
        selections.extend(
            self.providers
                .iter()
                .filter(|candidate| candidate.available)
                .map(|candidate| WorkflowRouteSelection::Byok {
                    provider: candidate.config.provider,
                }),
        );
        selections
    }

    fn load(environment: &WorkflowPreparationEnvironment<'_>) -> Result<Self, BackendError> {
        let settings = environment
            .settings_service
            .read_settings(environment.context)?;
        let agents = AgentKind::ALL
            .into_iter()
            .map(|kind| {
                let info = environment
                    .agent_service
                    .detect_agent(kind, settings.agent_default == Some(kind));
                let revision = hex_sha256(
                    canonical_json(&(kind, &info.state, &info.version, &info.executable_path))
                        .unwrap_or_default()
                        .as_bytes(),
                );
                (
                    kind,
                    AgentRouteCandidate {
                        available: info.state == AgentDetectionState::Installed,
                        revision,
                    },
                )
            })
            .collect();
        let mut providers = Vec::new();
        for config in settings.llm_providers {
            let configured_secret = !config.provider.requires_secret()
                || environment
                    .settings_service
                    .get_provider_secret_status(environment.secret_service, config.provider)?
                    .is_some();
            let available = config.enabled
                && !config.model.trim().is_empty()
                && valid_provider_url(&config.base_url)
                && configured_secret;
            let revision = hex_sha256(
                canonical_json(&(
                    config.provider,
                    &config.model,
                    &config.base_url,
                    config.context_window,
                    config.enabled,
                    configured_secret,
                ))
                .map_err(serialization_error)?
                .as_bytes(),
            );
            providers.push(ProviderRouteCandidate {
                config,
                available,
                revision,
            });
        }
        providers.sort_by_key(|candidate| provider_order(candidate.config.provider));
        Ok(Self {
            default_agent: settings.agent_default,
            agents,
            providers,
        })
    }
}

fn normalize_scope(
    context: &ProjectContext,
    kind: &WorkflowKind,
    requested: Option<&WorkflowScope>,
    source_versions: &[SourceVersionRef],
    resolved_sources: &[crate::services::ResolvedCompileSource],
    wiki_pages: &[String],
    complete_health_available: bool,
) -> Result<WorkflowScope, BackendError> {
    if requested.is_some_and(|scope| scope_kind(scope) != *kind) {
        return Err(BackendError::new(
            "WORKFLOW_SCOPE_KIND_MISMATCH",
            "Workflow scope does not match the selected workflow.",
            true,
            true,
        ));
    }
    match kind {
        WorkflowKind::UpdateWiki => {
            let (mode, selected) = match requested {
                Some(WorkflowScope::UpdateWiki {
                    mode,
                    source_versions,
                }) => (mode.clone(), source_versions.clone()),
                _ => (UpdateWikiMode::ChangedSources, Vec::new()),
            };
            let consumed = resolved_sources
                .iter()
                .filter(|source| source.already_consumed)
                .map(|source| {
                    (
                        source.reference.source_id.clone(),
                        source.reference.version_id.clone(),
                    )
                })
                .collect::<HashSet<_>>();
            let allowed = source_versions
                .iter()
                .filter(|source| {
                    mode == UpdateWikiMode::FullRecompile
                        || !consumed
                            .contains(&(source.source_id.clone(), source.version_id.clone()))
                })
                .map(|source| WorkflowSourceVersionRef {
                    source_id: source.source_id.clone(),
                    version_id: source.version_id.clone(),
                })
                .collect::<Vec<_>>();
            let mut normalized = if selected.is_empty() {
                allowed
            } else {
                for selection in &selected {
                    if !allowed.contains(selection) {
                        return Err(BackendError::new(
                            "WORKFLOW_SOURCE_SCOPE_STALE",
                            "Selected Source versions are no longer applicable.",
                            true,
                            true,
                        ));
                    }
                }
                selected
            };
            normalized.sort_by(|left, right| {
                (&left.source_id, &left.version_id).cmp(&(&right.source_id, &right.version_id))
            });
            normalized.dedup();
            Ok(WorkflowScope::UpdateWiki {
                mode,
                source_versions: normalized,
            })
        }
        WorkflowKind::HealthCheck => Ok(requested.cloned().unwrap_or(WorkflowScope::HealthCheck {
            mode: if complete_health_available {
                HealthCheckMode::Complete
            } else {
                HealthCheckMode::LocalQuick
            },
        })),
        WorkflowKind::GenerateContent => {
            let (artifact_type, mut page_paths, output_path) = match requested {
                Some(WorkflowScope::GenerateContent {
                    artifact_type,
                    page_paths,
                    output_path,
                }) => (
                    artifact_type.clone(),
                    page_paths.clone(),
                    output_path.clone(),
                ),
                _ => (WorkflowArtifactType::ProjectReport, Vec::new(), None),
            };
            page_paths = page_paths
                .into_iter()
                .map(|path| normalize_project_relative(&path))
                .collect::<Result<Vec<_>, _>>()?;
            page_paths.sort();
            page_paths.dedup();
            if page_paths.iter().any(|path| !wiki_pages.contains(path)) {
                return Err(BackendError::new(
                    "WORKFLOW_WIKI_SCOPE_STALE",
                    "Selected Wiki pages are no longer readable.",
                    true,
                    true,
                ));
            }
            validate_artifact_scope(&artifact_type, &page_paths)?;
            let output_path = match output_path {
                Some(path) => Some(validate_output_path(context, &path)?),
                None => Some(default_output_path(context, &artifact_type, &page_paths)?),
            };
            Ok(WorkflowScope::GenerateContent {
                artifact_type,
                page_paths,
                output_path,
            })
        }
    }
}

struct RouteResolution {
    route: Option<WorkflowRoute>,
    prerequisite_action: Option<WorkflowPrerequisiteAction>,
}

fn resolve_route(
    scope: &WorkflowScope,
    selection: Option<&WorkflowRouteSelection>,
    catalog: &RouteCatalog,
) -> RouteResolution {
    if matches!(
        scope,
        WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick
        }
    ) {
        return RouteResolution {
            route: Some(WorkflowRoute::Local {
                route_revision: "local-v1".into(),
            }),
            prerequisite_action: None,
        };
    }

    resolve_external_route(
        selection,
        catalog,
        !matches!(scope, WorkflowScope::HealthCheck { .. }),
    )
}

fn route_selection(route: &Option<WorkflowRoute>) -> Option<WorkflowRouteSelection> {
    match route {
        Some(WorkflowRoute::Agent { agent, .. }) => {
            Some(WorkflowRouteSelection::Agent { agent: *agent })
        }
        Some(WorkflowRoute::Byok { provider, .. }) => Some(WorkflowRouteSelection::Byok {
            provider: *provider,
        }),
        Some(WorkflowRoute::Local { .. }) | None => None,
    }
}

fn resolve_external_route(
    selection: Option<&WorkflowRouteSelection>,
    catalog: &RouteCatalog,
    allow_agent: bool,
) -> RouteResolution {
    match selection {
        Some(WorkflowRouteSelection::Agent { agent }) => {
            let route = catalog
                .agents
                .get(agent)
                .filter(|candidate| allow_agent && candidate.available)
                .map(|candidate| WorkflowRoute::Agent {
                    agent: *agent,
                    model: None,
                    route_revision: candidate.revision.clone(),
                });
            RouteResolution {
                prerequisite_action: route
                    .is_none()
                    .then_some(WorkflowPrerequisiteAction::ConfigureExecutionRoute),
                route,
            }
        }
        Some(WorkflowRouteSelection::Byok { provider }) => {
            let route = catalog
                .providers
                .iter()
                .find(|candidate| candidate.config.provider == *provider && candidate.available)
                .map(|candidate| WorkflowRoute::Byok {
                    provider: candidate.config.provider,
                    model: candidate.config.model.clone(),
                    route_revision: candidate.revision.clone(),
                });
            RouteResolution {
                prerequisite_action: route
                    .is_none()
                    .then_some(WorkflowPrerequisiteAction::ConfigureExecutionRoute),
                route,
            }
        }
        None => {
            if let Some(default_agent) = catalog.default_agent {
                let route = catalog
                    .agents
                    .get(&default_agent)
                    .filter(|candidate| allow_agent && candidate.available)
                    .map(|candidate| WorkflowRoute::Agent {
                        agent: default_agent,
                        model: None,
                        route_revision: candidate.revision.clone(),
                    });
                return RouteResolution {
                    prerequisite_action: route
                        .is_none()
                        .then_some(WorkflowPrerequisiteAction::ConfigureExecutionRoute),
                    route,
                };
            }
            let usable = catalog
                .providers
                .iter()
                .filter(|candidate| candidate.available)
                .collect::<Vec<_>>();
            let route = (usable.len() == 1).then(|| WorkflowRoute::Byok {
                provider: usable[0].config.provider,
                model: usable[0].config.model.clone(),
                route_revision: usable[0].revision.clone(),
            });
            RouteResolution {
                prerequisite_action: route.is_none().then_some(if usable.len() > 1 {
                    WorkflowPrerequisiteAction::ChooseExecutionRoute
                } else {
                    WorkflowPrerequisiteAction::ConfigureExecutionRoute
                }),
                route,
            }
        }
    }
}

fn prerequisites(
    scope: &WorkflowScope,
    access: &WorkflowProjectAccessSummary,
    route: &Option<WorkflowRoute>,
    sources: &[SourceVersionRef],
    wiki_pages: &[String],
    git_policy: &WorkflowGitPolicy,
    route_prerequisite_action: Option<WorkflowPrerequisiteAction>,
) -> Vec<WorkflowPrerequisite> {
    let mut items = Vec::new();
    let local_quick = matches!(
        scope,
        WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick
        }
    );
    let mut push = |code: &str, action: WorkflowPrerequisiteAction| {
        items.push(WorkflowPrerequisite {
            code: code.into(),
            message_key: prerequisite_message_key(&action).into(),
            blocking: true,
            action,
        });
    };
    if !local_quick && access.trust == WorkflowProjectTrust::Untrusted {
        push(
            "WORKFLOW_PROJECT_UNTRUSTED",
            WorkflowPrerequisiteAction::TrustProject,
        );
    }
    if matches!(
        scope,
        WorkflowScope::UpdateWiki { .. } | WorkflowScope::GenerateContent { .. }
    ) && access.filesystem_access == WorkflowFilesystemAccess::ReadOnly
    {
        push(
            "WORKFLOW_PROJECT_READ_ONLY",
            WorkflowPrerequisiteAction::MakeWritable,
        );
    }
    if matches!(
        git_policy,
        WorkflowGitPolicy::RequiredBeforeWrite | WorkflowGitPolicy::RequiredBeforeOverwrite
    ) {
        match access.git_state {
            WorkflowGitState::Unavailable => push(
                "WORKFLOW_GIT_UNAVAILABLE",
                WorkflowPrerequisiteAction::ConfigureGit,
            ),
            WorkflowGitState::Dirty => push(
                "WORKFLOW_GIT_DIRTY",
                WorkflowPrerequisiteAction::ResolveDirtyGit,
            ),
            WorkflowGitState::Clean => {}
        }
    }
    match scope {
        WorkflowScope::UpdateWiki { .. } => {
            if sources.is_empty() {
                push(
                    "WORKFLOW_SOURCES_REQUIRED",
                    WorkflowPrerequisiteAction::ImportSources,
                );
            }
        }
        WorkflowScope::HealthCheck { .. } if wiki_pages.is_empty() && sources.is_empty() => push(
            "WORKFLOW_MARKDOWN_REQUIRED",
            WorkflowPrerequisiteAction::ImportSources,
        ),
        WorkflowScope::GenerateContent { .. } if wiki_pages.is_empty() => push(
            "WORKFLOW_WIKI_REQUIRED",
            WorkflowPrerequisiteAction::UpdateWiki,
        ),
        _ => {}
    }
    if !local_quick && route.is_none() {
        push(
            "WORKFLOW_ROUTE_REQUIRED",
            route_prerequisite_action
                .unwrap_or(WorkflowPrerequisiteAction::ConfigureExecutionRoute),
        );
    }
    items
}

fn capture_baseline(
    context: &ProjectContext,
    scope: &WorkflowScope,
    current_sources: &[SourceVersionRef],
) -> Result<WorkflowBaselineSummary, BackendError> {
    let mut parts = vec![canonical_json(scope).map_err(serialization_error)?];
    let selected = match scope {
        WorkflowScope::UpdateWiki {
            source_versions, ..
        } => source_versions
            .iter()
            .map(|item| (item.source_id.as_str(), item.version_id.as_str()))
            .collect::<HashSet<_>>(),
        _ => HashSet::new(),
    };
    for source in current_sources {
        if selected.is_empty()
            || selected.contains(&(source.source_id.as_str(), source.version_id.as_str()))
        {
            parts.push(format!(
                "source:{}:{}:{}",
                source.source_id, source.version_id, source.content_hash
            ));
        }
    }
    let files = baseline_files(context, scope)?;
    for relative in &files {
        let hash = FileStore.file_hash(context, relative)?;
        parts.push(format!("file:{relative}:{hash}"));
    }
    if let WorkflowScope::GenerateContent {
        artifact_type,
        page_paths,
        output_path: Some(output_path),
    } = scope
    {
        let export_type = match artifact_type {
            WorkflowArtifactType::BeautifulRead => ExportType::BeautifulRead,
            WorkflowArtifactType::KnowledgeCard => ExportType::KnowledgeCard,
            WorkflowArtifactType::ConceptMap => ExportType::ConceptMap,
            WorkflowArtifactType::ProjectReport => ExportType::ProjectReport,
        };
        let export_service = ExportService::default();
        export_service.validate_workflow_scope(export_type, page_paths)?;
        parts.extend(export_service.workflow_baseline_entries(context, &files, output_path)?);
    }
    parts.sort();
    Ok(WorkflowBaselineSummary {
        fingerprint: hex_sha256(parts.join("\n").as_bytes()),
        captured_at: Utc::now().to_rfc3339(),
        item_count: files.len() as u64 + selected.len() as u64,
    })
}

pub fn workflow_baseline_for_scope(
    context: &ProjectContext,
    scope: &WorkflowScope,
) -> Result<WorkflowBaselineSummary, BackendError> {
    let current_sources = CompileService::list_source_versions(context)?;
    capture_baseline(context, scope, &current_sources)
}

fn baseline_files(
    context: &ProjectContext,
    scope: &WorkflowScope,
) -> Result<Vec<String>, BackendError> {
    let mut files = match scope {
        WorkflowScope::GenerateContent { page_paths, .. } if !page_paths.is_empty() => {
            page_paths.clone()
        }
        _ => list_readable_markdown(context)?,
    };
    for path in ["purpose.md", "schema.md"] {
        if context.resolve_project_path(path)?.is_file() {
            files.push(path.into());
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn output_summary(
    context: &ProjectContext,
    scope: &WorkflowScope,
) -> Result<WorkflowOutputSummary, BackendError> {
    match scope {
        WorkflowScope::UpdateWiki { .. } => Ok(WorkflowOutputSummary {
            label_key: "workflows.output.wiki".into(),
            location: Some("wiki/".into()),
            may_change_wiki: true,
        }),
        WorkflowScope::HealthCheck { .. } => Ok(WorkflowOutputSummary {
            label_key: "workflows.output.healthReport".into(),
            location: None,
            may_change_wiki: false,
        }),
        WorkflowScope::GenerateContent { output_path, .. } => {
            if let Some(path) = output_path {
                let _ = context.resolve_project_path(path)?;
            }
            Ok(WorkflowOutputSummary {
                label_key: "workflows.output.export".into(),
                location: output_path.clone(),
                may_change_wiki: false,
            })
        }
    }
}

fn git_policy(
    context: &ProjectContext,
    scope: &WorkflowScope,
) -> Result<WorkflowGitPolicy, BackendError> {
    match scope {
        WorkflowScope::UpdateWiki { .. } => Ok(WorkflowGitPolicy::RequiredBeforeWrite),
        WorkflowScope::HealthCheck { .. } => Ok(WorkflowGitPolicy::NotRequired),
        WorkflowScope::GenerateContent { output_path, .. } => Ok(match output_path {
            Some(path) if context.resolve_project_path(path)?.exists() => {
                WorkflowGitPolicy::RequiredBeforeOverwrite
            }
            _ => WorkflowGitPolicy::NotRequired,
        }),
    }
}

fn preparation_fingerprint(
    access: &WorkflowProjectAccessSummary,
    scope: &WorkflowScope,
    baseline: &WorkflowBaselineSummary,
    route: &Option<WorkflowRoute>,
    prerequisites: &[WorkflowPrerequisite],
    output: &WorkflowOutputSummary,
    git_policy: &WorkflowGitPolicy,
    execution_options: &WorkflowExecutionOptions,
) -> Result<String, BackendError> {
    let material = (
        &access.canonical_identity_key,
        &access.identity_revision,
        &access.trust,
        &access.filesystem_access,
        &access.persistence,
        &access.git_state,
        scope,
        &baseline.fingerprint,
        route,
        prerequisites,
        output,
        git_policy,
        &execution_options.existing_target_hash,
    );
    Ok(hex_sha256(
        canonical_json(&material)
            .map_err(serialization_error)?
            .as_bytes(),
    ))
}

fn list_wiki_pages(context: &ProjectContext) -> Result<Vec<String>, BackendError> {
    let source_only = context
        .list_markdown_files_for_roles(&[crate::models::layout::ProjectMarkdownRootRole::Source])?
        .into_iter()
        .filter_map(|path| context.to_project_relative(&path).ok())
        .collect::<HashSet<_>>();
    let mut pages = context
        .list_markdown_files_for_roles(&[
            crate::models::layout::ProjectMarkdownRootRole::Wiki,
            crate::models::layout::ProjectMarkdownRootRole::Mixed,
        ])?
        .into_iter()
        .filter_map(|path| {
            let normalized = context.to_project_relative(&path).ok()?;
            (!source_only.contains(&normalized)).then_some(normalized)
        })
        .collect::<Vec<_>>();
    pages.sort();
    pages.dedup();
    Ok(pages)
}

fn list_readable_markdown(context: &ProjectContext) -> Result<Vec<String>, BackendError> {
    let mut files = context
        .list_markdown_files_for_roles(&[
            crate::models::layout::ProjectMarkdownRootRole::Source,
            crate::models::layout::ProjectMarkdownRootRole::Wiki,
            crate::models::layout::ProjectMarkdownRootRole::Mixed,
        ])?
        .into_iter()
        .filter_map(|path| context.to_project_relative(&path).ok())
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    Ok(files)
}

fn validate_artifact_scope(
    artifact: &WorkflowArtifactType,
    pages: &[String],
) -> Result<(), BackendError> {
    let valid = match artifact {
        WorkflowArtifactType::BeautifulRead => pages.len() == 1,
        WorkflowArtifactType::KnowledgeCard | WorkflowArtifactType::ConceptMap => !pages.is_empty(),
        WorkflowArtifactType::ProjectReport => pages.is_empty(),
    };
    if valid {
        Ok(())
    } else {
        Err(BackendError::new(
            "WORKFLOW_ARTIFACT_SCOPE_INVALID",
            "The selected artifact type requires a different Wiki page scope.",
            true,
            true,
        ))
    }
}

fn validate_output_path(context: &ProjectContext, value: &str) -> Result<String, BackendError> {
    let normalized = normalize_project_relative(value)?;
    ExportService::default().validate_workflow_output_path(context, &normalized)
}

fn default_output_path(
    context: &ProjectContext,
    artifact: &WorkflowArtifactType,
    pages: &[String],
) -> Result<String, BackendError> {
    let base = pages
        .first()
        .and_then(|path| Path::new(path).file_stem())
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("project");
    let suffix = match artifact {
        WorkflowArtifactType::BeautifulRead => "beautiful-read",
        WorkflowArtifactType::KnowledgeCard => "knowledge-card",
        WorkflowArtifactType::ConceptMap => "concept-map",
        WorkflowArtifactType::ProjectReport => "project-report",
    };
    let root = ExportService::default().workflow_export_root_relative(context)?;
    Ok(format!("{root}/{base}-{suffix}.html"))
}

fn normalize_project_relative(value: &str) -> Result<String, BackendError> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.contains(':')
        || Path::new(&normalized)
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(BackendError::new(
            "WORKFLOW_PATH_INVALID",
            "Workflow paths must be safe project-relative paths.",
            true,
            true,
        ));
    }
    Ok(normalized)
}

fn valid_provider_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

pub(crate) const REMOTE_PROVIDER_DISCLOSURE_REVISION: &str = "workflow-remote-provider-v1";

pub(crate) fn route_is_remote_provider(
    context: &ProjectContext,
    settings_service: &SettingsService,
    route: Option<&WorkflowRoute>,
) -> Result<bool, BackendError> {
    let Some(WorkflowRoute::Byok { provider, .. }) = route else {
        return Ok(false);
    };
    if *provider == LlmProviderKind::Ollama {
        return Ok(false);
    }
    if *provider != LlmProviderKind::Custom {
        return Ok(true);
    }
    let config = settings_service
        .list_providers(context)?
        .into_iter()
        .find(|config| config.provider == *provider);
    Ok(config
        .as_ref()
        .is_none_or(|config| !is_loopback_provider_url(&config.base_url)))
}

pub(crate) fn route_requires_remote_acknowledgement(
    context: &ProjectContext,
    settings_service: &SettingsService,
    route: Option<&WorkflowRoute>,
) -> Result<bool, BackendError> {
    Ok(route_is_remote_provider(context, settings_service, route)?
        && !settings_service
            .is_remote_provider_disclosure_acknowledged(REMOTE_PROVIDER_DISCLOSURE_REVISION)?)
}

fn is_loopback_provider_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    let Some((_, authority_and_path)) = lower.split_once("://") else {
        return false;
    };
    let authority = authority_and_path.split('/').next().unwrap_or_default();
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    host_port == "localhost"
        || host_port.starts_with("localhost:")
        || host_port == "127.0.0.1"
        || host_port.starts_with("127.0.0.1:")
        || host_port == "[::1]"
        || host_port.starts_with("[::1]:")
}

fn provider_order(provider: LlmProviderKind) -> u8 {
    match provider {
        LlmProviderKind::OpenAi => 0,
        LlmProviderKind::Anthropic => 1,
        LlmProviderKind::Google => 2,
        LlmProviderKind::Ollama => 3,
        LlmProviderKind::Custom => 4,
    }
}

fn scope_kind(scope: &WorkflowScope) -> WorkflowKind {
    match scope {
        WorkflowScope::UpdateWiki { .. } => WorkflowKind::UpdateWiki,
        WorkflowScope::HealthCheck { .. } => WorkflowKind::HealthCheck,
        WorkflowScope::GenerateContent { .. } => WorkflowKind::GenerateContent,
    }
}

fn prerequisite_message_key(action: &WorkflowPrerequisiteAction) -> &'static str {
    match action {
        WorkflowPrerequisiteAction::OpenOrCreateProject => {
            "workflows.prerequisite.openOrCreateProject"
        }
        WorkflowPrerequisiteAction::TrustProject => "workflows.prerequisite.trustProject",
        WorkflowPrerequisiteAction::MakeWritable => "workflows.prerequisite.makeWritable",
        WorkflowPrerequisiteAction::ConfigureGit => "workflows.prerequisite.configureGit",
        WorkflowPrerequisiteAction::ResolveDirtyGit => "workflows.prerequisite.resolveDirtyGit",
        WorkflowPrerequisiteAction::ImportSources => "workflows.prerequisite.importSources",
        WorkflowPrerequisiteAction::UpdateWiki => "workflows.prerequisite.updateWiki",
        WorkflowPrerequisiteAction::ConfigureExecutionRoute => {
            "workflows.prerequisite.configureExecutionRoute"
        }
        WorkflowPrerequisiteAction::ChooseExecutionRoute => {
            "workflows.prerequisite.chooseExecutionRoute"
        }
        WorkflowPrerequisiteAction::PrepareAgain => "workflows.prerequisite.prepareAgain",
        WorkflowPrerequisiteAction::AcknowledgeRemoteProvider => {
            "workflows.prerequisite.acknowledgeRemoteProvider"
        }
        WorkflowPrerequisiteAction::AcknowledgeRestrictedContent => {
            "workflows.prerequisite.acknowledgeRestrictedContent"
        }
    }
}

fn prerequisite_priority(action: &WorkflowPrerequisiteAction) -> u8 {
    match action {
        WorkflowPrerequisiteAction::OpenOrCreateProject => 0,
        WorkflowPrerequisiteAction::ImportSources | WorkflowPrerequisiteAction::UpdateWiki => 1,
        WorkflowPrerequisiteAction::TrustProject => 2,
        WorkflowPrerequisiteAction::MakeWritable => 3,
        WorkflowPrerequisiteAction::ConfigureGit | WorkflowPrerequisiteAction::ResolveDirtyGit => 4,
        WorkflowPrerequisiteAction::ConfigureExecutionRoute
        | WorkflowPrerequisiteAction::ChooseExecutionRoute => 5,
        WorkflowPrerequisiteAction::PrepareAgain
        | WorkflowPrerequisiteAction::AcknowledgeRemoteProvider
        | WorkflowPrerequisiteAction::AcknowledgeRestrictedContent => 6,
    }
}

pub fn workflow_stages(kind: &WorkflowKind) -> Vec<WorkflowStage> {
    let definitions: &[(&str, &str)] = match kind {
        WorkflowKind::UpdateWiki => &[
            (
                "analyze_sources",
                "workflows.stage.updateWiki.analyzeSources",
            ),
            (
                "create_checkpoint",
                "workflows.stage.updateWiki.createCheckpoint",
            ),
            ("plan_updates", "workflows.stage.updateWiki.planUpdates"),
            (
                "generate_candidates",
                "workflows.stage.updateWiki.generateCandidates",
            ),
            (
                "validate_structure",
                "workflows.stage.updateWiki.validateStructure",
            ),
            ("review_risk", "workflows.stage.updateWiki.reviewRisk"),
            ("apply_changes", "workflows.stage.updateWiki.applyChanges"),
            (
                "refresh_indexes",
                "workflows.stage.updateWiki.refreshIndexes",
            ),
            ("record_result", "workflows.stage.updateWiki.recordResult"),
        ],
        WorkflowKind::HealthCheck => &[
            ("read_markdown", "workflows.stage.healthCheck.readMarkdown"),
            (
                "check_markdown",
                "workflows.stage.healthCheck.checkMarkdown",
            ),
            ("check_links", "workflows.stage.healthCheck.checkLinks"),
            ("deep_check", "workflows.stage.healthCheck.deepCheck"),
            (
                "merge_findings",
                "workflows.stage.healthCheck.mergeFindings",
            ),
            (
                "classify_findings",
                "workflows.stage.healthCheck.classifyFindings",
            ),
            ("write_report", "workflows.stage.healthCheck.writeReport"),
            ("complete", "workflows.stage.healthCheck.complete"),
        ],
        WorkflowKind::GenerateContent => &[
            (
                "confirm_scope",
                "workflows.stage.generateContent.confirmScope",
            ),
            ("read_wiki", "workflows.stage.generateContent.readWiki"),
            (
                "load_template",
                "workflows.stage.generateContent.loadTemplate",
            ),
            (
                "generate_content",
                "workflows.stage.generateContent.generateContent",
            ),
            (
                "assemble_artifact",
                "workflows.stage.generateContent.assembleArtifact",
            ),
            (
                "validate_artifact",
                "workflows.stage.generateContent.validateArtifact",
            ),
            (
                "write_export",
                "workflows.stage.generateContent.writeExport",
            ),
            (
                "generate_preview",
                "workflows.stage.generateContent.generatePreview",
            ),
            ("complete", "workflows.stage.generateContent.complete"),
        ],
    };
    definitions
        .iter()
        .enumerate()
        .map(|(index, (id, label_key))| WorkflowStage {
            id: (*id).into(),
            ordinal: (index + 1) as u32,
            status: WorkflowStageStatus::Pending,
            label_key: (*label_key).into(),
            started_at: None,
            completed_at: None,
            current_item: None,
            progress: None,
            decision: None,
        })
        .collect()
}

fn workflow_title(kind: &WorkflowKind) -> &'static str {
    match kind {
        WorkflowKind::UpdateWiki => "Update Wiki",
        WorkflowKind::HealthCheck => "Health Check",
        WorkflowKind::GenerateContent => "Generate Content",
    }
}

fn stale_preparation_error() -> BackendError {
    BackendError::new(
        "WORKFLOW_PREPARATION_STALE",
        "Workflow preparation is stale. Prepare the workflow again.",
        true,
        true,
    )
    .with_details(serde_json::json!({
        "action": WorkflowPrerequisiteAction::PrepareAgain
    }))
}

fn stale_remembered_scope(error: &BackendError) -> bool {
    matches!(
        error.code.as_str(),
        "WORKFLOW_SOURCE_SCOPE_STALE" | "WORKFLOW_WIKI_SCOPE_STALE"
    )
}

fn preparation_lock_error() -> BackendError {
    BackendError::new(
        "WORKFLOW_PREPARATION_LOCKED",
        "Workflow preparation is temporarily unavailable.",
        true,
        false,
    )
}

fn serialization_error(message: String) -> BackendError {
    BackendError::new(
        "WORKFLOW_PREPARATION_SERIALIZE_FAILED",
        message,
        true,
        false,
    )
}
