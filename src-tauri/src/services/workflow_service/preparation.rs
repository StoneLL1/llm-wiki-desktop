use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};
use std::sync::RwLock;

#[cfg(test)]
use std::cell::Cell;

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
use crate::utils::path_utils::normalize_project_path;

use super::fingerprint::{canonical_json, hex_sha256};
use super::persistence::project_identity;
use super::preferences::{WorkflowPreference, WorkflowPreferences};

const PREPARATION_TTL_MINUTES: i64 = 15;
const STARTED_PREPARATION_TTL_HOURS: i64 = 24;
const MAX_PREPARATIONS_PER_IDENTITY: usize = 64;
const MAX_PREPARATIONS_GLOBAL: usize = 512;
const MAX_STARTED_PREPARATIONS_PER_IDENTITY: usize = 128;
const MAX_STARTED_PREPARATIONS_GLOBAL: usize = 1_024;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparationCostSnapshot {
    source_inventories: usize,
    markdown_root_inventories: usize,
    route_catalog_loads: usize,
    agent_probes: usize,
    baseline_hashes: usize,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
struct PreparationTimingSnapshot {
    inventory_nanos: u64,
    route_nanos: u64,
    agent_nanos: u64,
    slowest_agent_probe_nanos: u64,
    markdown_nanos: u64,
}

#[cfg(test)]
thread_local! {
    static SOURCE_INVENTORIES: Cell<usize> = const { Cell::new(0) };
    static MARKDOWN_ROOT_INVENTORIES: Cell<usize> = const { Cell::new(0) };
    static ROUTE_CATALOG_LOADS: Cell<usize> = const { Cell::new(0) };
    static AGENT_PROBES: Cell<usize> = const { Cell::new(0) };
    static BASELINE_HASHES: Cell<usize> = const { Cell::new(0) };
    static INVENTORY_NANOS: Cell<u64> = const { Cell::new(0) };
    static ROUTE_NANOS: Cell<u64> = const { Cell::new(0) };
    static AGENT_NANOS: Cell<u64> = const { Cell::new(0) };
    static SLOWEST_AGENT_PROBE_NANOS: Cell<u64> = const { Cell::new(0) };
    static MARKDOWN_NANOS: Cell<u64> = const { Cell::new(0) };
}

#[cfg(test)]
fn reset_preparation_costs() {
    SOURCE_INVENTORIES.set(0);
    MARKDOWN_ROOT_INVENTORIES.set(0);
    ROUTE_CATALOG_LOADS.set(0);
    AGENT_PROBES.set(0);
    BASELINE_HASHES.set(0);
    INVENTORY_NANOS.set(0);
    ROUTE_NANOS.set(0);
    AGENT_NANOS.set(0);
    SLOWEST_AGENT_PROBE_NANOS.set(0);
    MARKDOWN_NANOS.set(0);
}

#[cfg(test)]
fn preparation_timings() -> PreparationTimingSnapshot {
    PreparationTimingSnapshot {
        inventory_nanos: INVENTORY_NANOS.get(),
        route_nanos: ROUTE_NANOS.get(),
        agent_nanos: AGENT_NANOS.get(),
        slowest_agent_probe_nanos: SLOWEST_AGENT_PROBE_NANOS.get(),
        markdown_nanos: MARKDOWN_NANOS.get(),
    }
}

#[cfg(test)]
fn add_elapsed(counter: &'static std::thread::LocalKey<Cell<u64>>, started: std::time::Instant) {
    let elapsed = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    counter.with(|value| value.set(value.get().saturating_add(elapsed)));
}

#[cfg(test)]
fn preparation_costs() -> PreparationCostSnapshot {
    PreparationCostSnapshot {
        source_inventories: SOURCE_INVENTORIES.get(),
        markdown_root_inventories: MARKDOWN_ROOT_INVENTORIES.get(),
        route_catalog_loads: ROUTE_CATALOG_LOADS.get(),
        agent_probes: AGENT_PROBES.get(),
        baseline_hashes: BASELINE_HASHES.get(),
    }
}

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
    created_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct StartedPreparationRecord {
    preparation_revision: String,
    task_id: String,
    owner: String,
    started_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
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

struct CapturedBaseline {
    summary: WorkflowBaselineSummary,
    has_readable_markdown: bool,
}

struct CachedMarkdownFile {
    hash: String,
    resource_paths: Vec<String>,
}

struct RequestEvaluationSnapshot {
    project_access: WorkflowProjectAccessSummary,
    authority_revision: String,
    source_versions: Vec<SourceVersionRef>,
    resolved_sources: Vec<crate::services::ResolvedCompileSource>,
    wiki_pages: Vec<String>,
    readable_markdown: Vec<String>,
    route_catalog: RouteCatalog,
    collect_resource_paths: bool,
    markdown_files: RefCell<HashMap<String, CachedMarkdownFile>>,
}

pub(super) struct WorkflowOverviewEvaluationSnapshot {
    pub(super) prerequisites: Vec<(WorkflowKind, Option<WorkflowPrerequisite>, String)>,
    pub(super) has_sources: bool,
    pub(super) changed_source_count: usize,
    pub(super) has_readable_markdown: bool,
}

#[derive(Default)]
struct PreparationRecords {
    prepared: HashMap<String, PreparedRecord>,
    started: HashMap<String, StartedPreparationRecord>,
}

#[derive(Default)]
pub struct WorkflowPreparationService {
    records: RwLock<PreparationRecords>,
}

pub(crate) enum PreparationStartLookup {
    Started(String),
    Prepared,
    Missing,
}

impl RequestEvaluationSnapshot {
    fn capture(
        environment: &WorkflowPreparationEnvironment<'_>,
        collect_resource_paths: bool,
    ) -> Result<Self, BackendError> {
        let identity = project_identity(&environment.context.root).map_err(|message| {
            BackendError::new("WORKFLOW_IDENTITY_FAILED", message, true, false)
        })?;
        let project_access = WorkflowProjectAccessSummary {
            project_id: environment.context.project_id.clone(),
            canonical_identity_key: identity.canonical_identity_key,
            identity_revision: identity.identity_revision,
            trust: environment.access.trust.clone(),
            filesystem_access: environment.access.filesystem_access.clone(),
            persistence: environment.access.persistence.clone(),
            git_state: environment.access.git_state.clone(),
        };
        #[cfg(test)]
        SOURCE_INVENTORIES.with(|count| count.set(count.get() + 1));
        #[cfg(test)]
        let inventory_started = std::time::Instant::now();
        let source_versions = CompileService::list_source_versions(environment.context)?;
        let resolved_sources = if source_versions.is_empty() {
            Vec::new()
        } else {
            CompileService::resolve_source_versions(environment.context, &source_versions)?
        };
        let readable_markdown = list_markdown_inventory(environment.context)?;
        let wiki_pages = wiki_pages_from_inventory(environment.context, &readable_markdown);
        #[cfg(test)]
        add_elapsed(&INVENTORY_NANOS, inventory_started);
        #[cfg(test)]
        let route_started = std::time::Instant::now();
        #[cfg(test)]
        let agent_before = AGENT_NANOS.get();
        let route_catalog = RouteCatalog::load(environment, &project_access)?;
        #[cfg(test)]
        {
            let route_total = u64::try_from(route_started.elapsed().as_nanos()).unwrap_or(u64::MAX);
            let agent_elapsed = AGENT_NANOS.get().saturating_sub(agent_before);
            ROUTE_NANOS.with(|nanos| {
                nanos.set(
                    nanos
                        .get()
                        .saturating_add(route_total.saturating_sub(agent_elapsed)),
                )
            });
        }
        Ok(Self {
            project_access,
            authority_revision: environment.access.authority_revision.clone(),
            source_versions,
            resolved_sources,
            wiki_pages,
            readable_markdown,
            route_catalog,
            collect_resource_paths,
            markdown_files: RefCell::new(HashMap::new()),
        })
    }

    fn markdown_file<'a>(
        &'a self,
        context: &ProjectContext,
        relative: &str,
    ) -> Result<std::cell::Ref<'a, CachedMarkdownFile>, BackendError> {
        if !self.markdown_files.borrow().contains_key(relative) {
            #[cfg(test)]
            BASELINE_HASHES.with(|count| count.set(count.get() + 1));
            #[cfg(test)]
            let markdown_started = std::time::Instant::now();
            let bytes = FileStore.read_bytes(context, relative)?;
            let hash = FileStore.content_hash(&bytes);
            let resource_paths = if self.collect_resource_paths {
                String::from_utf8(bytes)
                    .ok()
                    .map(|markdown| {
                        ExportService::workflow_resource_paths_from_markdown(relative, &markdown)
                    })
                    .transpose()?
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            self.markdown_files.borrow_mut().insert(
                relative.to_string(),
                CachedMarkdownFile {
                    hash,
                    resource_paths,
                },
            );
            #[cfg(test)]
            add_elapsed(&MARKDOWN_NANOS, markdown_started);
        }
        Ok(std::cell::Ref::map(self.markdown_files.borrow(), |files| {
            files
                .get(relative)
                .expect("request-scoped Markdown cache entry must exist")
        }))
    }

    fn readable_markdown(&self, context: &ProjectContext) -> Result<Vec<String>, BackendError> {
        let _ = context;
        Ok(self.readable_markdown.clone())
    }
}

pub(super) fn overview_evaluation_snapshot(
    preferences: &WorkflowPreferences,
    environment: &WorkflowPreparationEnvironment<'_>,
) -> Result<WorkflowOverviewEvaluationSnapshot, BackendError> {
    let evaluation = RequestEvaluationSnapshot::capture(environment, true)?;
    let remembered = preferences.load(
        environment.context,
        &evaluation.project_access.canonical_identity_key,
        &evaluation.project_access.identity_revision,
        &evaluation.project_access.persistence,
    )?;
    let remembered_health =
        if evaluation.project_access.persistence == WorkflowPersistenceMode::MemoryOnly {
            remembered
                .iter()
                .find(|entry| entry.kind == WorkflowKind::HealthCheck)
                .cloned()
        } else {
            preferences
                .load(
                    environment.context,
                    &evaluation.project_access.canonical_identity_key,
                    &evaluation.project_access.identity_revision,
                    &WorkflowPersistenceMode::MemoryOnly,
                )?
                .into_iter()
                .find(|entry| entry.kind == WorkflowKind::HealthCheck)
        };
    let prerequisites = [
        WorkflowKind::UpdateWiki,
        WorkflowKind::HealthCheck,
        WorkflowKind::GenerateContent,
    ]
    .into_iter()
    .map(|kind| {
        let mut snapshot = build_snapshot_from_evaluation(
            environment,
            &PrepareWorkflowInput {
                kind: kind.clone(),
                scope: None,
                route_selection: None,
            },
            &evaluation,
        )?;
        let previous = if kind == WorkflowKind::HealthCheck {
            remembered_health.as_ref()
        } else {
            remembered.iter().find(|entry| entry.kind == kind)
        };
        if let Some(previous) = previous {
            let remembered = build_snapshot_from_evaluation(
                environment,
                &PrepareWorkflowInput {
                    kind: kind.clone(),
                    scope: Some(previous.scope.clone()),
                    route_selection: route_selection(&previous.route),
                },
                &evaluation,
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
    .collect::<Result<Vec<_>, BackendError>>()?;
    let has_readable_markdown = !evaluation.source_versions.is_empty()
        || !evaluation
            .readable_markdown(environment.context)?
            .is_empty();
    Ok(WorkflowOverviewEvaluationSnapshot {
        prerequisites,
        has_sources: !evaluation.source_versions.is_empty(),
        changed_source_count: evaluation
            .resolved_sources
            .iter()
            .filter(|source| !source.already_consumed)
            .count(),
        has_readable_markdown,
    })
}

impl WorkflowPreparationService {
    pub fn prepare(
        &self,
        preferences: &WorkflowPreferences,
        environment: &WorkflowPreparationEnvironment<'_>,
        input: PrepareWorkflowInput,
    ) -> Result<WorkflowPreparation, BackendError> {
        let evaluation = RequestEvaluationSnapshot::capture(
            environment,
            input.kind == WorkflowKind::GenerateContent,
        )?;
        let mut snapshot = build_snapshot_from_evaluation(environment, &input, &evaluation)?;
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
            match build_snapshot_from_evaluation(environment, remembered_input, &evaluation) {
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
        let preparation_revision =
            preparation_revision_for(&preparation_id, &snapshot.preparation_fingerprint);
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
        let mut stored_preparation = preparation.clone();
        stored_preparation.available_source_versions.clear();
        stored_preparation.available_wiki_pages.clear();
        stored_preparation.available_routes.clear();
        let now = Utc::now();
        let record = PreparedRecord {
            preparation: stored_preparation,
            authority_revision: snapshot.authority_revision,
            route_selection: applied_remembered_input
                .and_then(|remembered| remembered.route_selection)
                .or(input.route_selection),
            execution_options: WorkflowExecutionOptions {
                preparation_revision,
                preparation_fingerprint: Some(snapshot.preparation_fingerprint.clone()),
                ..snapshot.execution_options
            },
            preparation_fingerprint: snapshot.preparation_fingerprint,
            created_at: now,
            expires_at: now + Duration::minutes(PREPARATION_TTL_MINUTES),
        };
        let owner = preparation_owner(&record.preparation);
        let mut records = self.records.write().map_err(|_| preparation_lock_error())?;
        prune_preparation_records(&mut records, now);
        records.prepared.insert(preparation_id, record);
        enforce_prepared_caps(&mut records, &owner);
        Ok(preparation)
    }

    pub(crate) fn lookup_for_start(
        &self,
        preparation_id: &str,
        preparation_revision: &str,
        identity_key: &str,
        identity_revision: &str,
    ) -> Result<PreparationStartLookup, BackendError> {
        let now = Utc::now();
        let mut records = self.records.write().map_err(|_| preparation_lock_error())?;
        prune_preparation_records(&mut records, now);
        if let Some(record) = records.started.get(preparation_id) {
            if record.preparation_revision != preparation_revision
                || record.owner != owner_key(identity_key, identity_revision)
            {
                return Err(stale_preparation_error());
            }
            return Ok(PreparationStartLookup::Started(record.task_id.clone()));
        }
        if let Some(record) = records.prepared.get(preparation_id) {
            if record.preparation.preparation_revision != preparation_revision {
                return Err(stale_preparation_error());
            }
            return Ok(PreparationStartLookup::Prepared);
        }
        Ok(PreparationStartLookup::Missing)
    }

    pub fn mark_started(
        &self,
        preparation_id: &str,
        preparation_revision: &str,
        task_id: &str,
        identity_key: &str,
        identity_revision: &str,
    ) -> Result<(), BackendError> {
        let now = Utc::now();
        let mut records = self.records.write().map_err(|_| preparation_lock_error())?;
        prune_preparation_records(&mut records, now);
        if let Some(existing) = records.started.get(preparation_id) {
            return if existing.preparation_revision == preparation_revision
                && existing.task_id == task_id
            {
                Ok(())
            } else {
                Err(stale_preparation_error())
            };
        }
        if let Some(record) = records.prepared.remove(preparation_id) {
            if record.preparation.preparation_revision != preparation_revision {
                records.prepared.insert(preparation_id.to_string(), record);
                return Err(stale_preparation_error());
            }
        }
        let owner = owner_key(identity_key, identity_revision);
        records.started.insert(
            preparation_id.to_string(),
            StartedPreparationRecord {
                preparation_revision: preparation_revision.to_string(),
                task_id: task_id.to_string(),
                owner: owner.clone(),
                started_at: now,
                expires_at: now + Duration::hours(STARTED_PREPARATION_TTL_HOURS),
            },
        );
        enforce_started_caps(&mut records, &owner);
        Ok(())
    }

    pub fn validate_for_start(
        &self,
        environment: &WorkflowPreparationEnvironment<'_>,
        preparation_id: &str,
        preparation_revision: &str,
    ) -> Result<ValidatedWorkflowStart, BackendError> {
        let now = Utc::now();
        let record = {
            let mut records = self.records.write().map_err(|_| preparation_lock_error())?;
            prune_preparation_records(&mut records, now);
            records.prepared.get(preparation_id).cloned()
        }
        .ok_or_else(stale_preparation_error)?;
        if record.preparation.preparation_revision != preparation_revision {
            return Err(stale_preparation_error());
        }
        if matches!(record.preparation.route, Some(WorkflowRoute::Agent { .. })) {
            // Route presentation may reuse the short-lived probe cache, but a
            // start token must bind the Agent executable/version/profile that
            // exists now. Force the refreshed snapshot below to probe again.
            environment.agent_service.invalidate_workflow_route_cache();
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

fn preparation_owner(preparation: &WorkflowPreparation) -> String {
    owner_key(
        &preparation.project_access.canonical_identity_key,
        &preparation.project_access.identity_revision,
    )
}

fn owner_key(identity_key: &str, identity_revision: &str) -> String {
    format!("{identity_key}:{identity_revision}")
}

pub(crate) fn preparation_revision_for(
    preparation_id: &str,
    preparation_fingerprint: &str,
) -> String {
    hex_sha256(
        format!("workflow-preparation-v1\n{preparation_id}\n{preparation_fingerprint}").as_bytes(),
    )
}

fn prune_preparation_records(records: &mut PreparationRecords, now: chrono::DateTime<Utc>) {
    records
        .prepared
        .retain(|_, record| record.expires_at >= now);
    records.started.retain(|_, record| record.expires_at >= now);
}

fn enforce_prepared_caps(records: &mut PreparationRecords, owner: &str) {
    while records
        .prepared
        .values()
        .filter(|record| preparation_owner(&record.preparation) == owner)
        .count()
        > MAX_PREPARATIONS_PER_IDENTITY
    {
        let oldest = records
            .prepared
            .iter()
            .filter(|(_, record)| preparation_owner(&record.preparation) == owner)
            .min_by_key(|(_, record)| record.created_at)
            .map(|(id, _)| id.clone());
        let Some(oldest) = oldest else {
            break;
        };
        records.prepared.remove(&oldest);
    }
    while records.prepared.len() > MAX_PREPARATIONS_GLOBAL {
        let oldest = records
            .prepared
            .iter()
            .min_by_key(|(_, record)| record.created_at)
            .map(|(id, _)| id.clone());
        let Some(oldest) = oldest else {
            break;
        };
        records.prepared.remove(&oldest);
    }
}

fn enforce_started_caps(records: &mut PreparationRecords, owner: &str) {
    while records
        .started
        .values()
        .filter(|record| record.owner == owner)
        .count()
        > MAX_STARTED_PREPARATIONS_PER_IDENTITY
    {
        let oldest = records
            .started
            .iter()
            .filter(|(_, record)| record.owner == owner)
            .min_by_key(|(_, record)| record.started_at)
            .map(|(id, _)| id.clone());
        let Some(oldest) = oldest else {
            break;
        };
        records.started.remove(&oldest);
    }
    while records.started.len() > MAX_STARTED_PREPARATIONS_GLOBAL {
        let oldest = records
            .started
            .iter()
            .min_by_key(|(_, record)| record.started_at)
            .map(|(id, _)| id.clone());
        let Some(oldest) = oldest else {
            break;
        };
        records.started.remove(&oldest);
    }
}

fn build_snapshot(
    environment: &WorkflowPreparationEnvironment<'_>,
    input: &PrepareWorkflowInput,
) -> Result<PreparationSnapshot, BackendError> {
    let evaluation = RequestEvaluationSnapshot::capture(
        environment,
        input.kind == WorkflowKind::GenerateContent,
    )?;
    build_snapshot_from_evaluation(environment, input, &evaluation)
}

fn build_snapshot_from_evaluation(
    environment: &WorkflowPreparationEnvironment<'_>,
    input: &PrepareWorkflowInput,
    evaluation: &RequestEvaluationSnapshot,
) -> Result<PreparationSnapshot, BackendError> {
    // Health is read-only with respect to project content and Git. Only a
    // Complete Health Agent route produces the report consumed by H3 repair;
    // keep Local Quick and BYOK Health metadata process-local. Trusted,
    // writable Complete Agent Health still receives a durable owner.
    let mut project_access = evaluation.project_access.clone();
    let available_source_versions = evaluation
        .source_versions
        .iter()
        .map(|source| WorkflowSourceVersionRef {
            source_id: source.source_id.clone(),
            version_id: source.version_id.clone(),
        })
        .collect();
    let available_wiki_pages = evaluation.wiki_pages.clone();
    let agent_policy = match input.kind {
        WorkflowKind::HealthCheck => AgentRoutePolicy::LintOnly,
        WorkflowKind::UpdateWiki | WorkflowKind::GenerateContent => AgentRoutePolicy::Any,
    };
    let available_routes = evaluation
        .route_catalog
        .available_selections_for(agent_policy);
    let default_route = resolve_external_route(
        input.route_selection.as_ref(),
        &evaluation.route_catalog,
        if input.kind == WorkflowKind::HealthCheck {
            AgentRoutePolicy::LintOnly
        } else {
            AgentRoutePolicy::Disabled
        },
        input.kind == WorkflowKind::HealthCheck,
    );
    let scope = normalize_scope(
        environment.context,
        &input.kind,
        input.scope.as_ref(),
        &evaluation.source_versions,
        &evaluation.resolved_sources,
        &evaluation.wiki_pages,
        project_access.trust == WorkflowProjectTrust::Trusted && default_route.route.is_some(),
    )?;
    let route_resolution = resolve_route(
        &scope,
        input.route_selection.as_ref(),
        &evaluation.route_catalog,
    );
    let route = route_resolution.route;
    let durable_health_owner = matches!(
        (&scope, &route),
        (
            WorkflowScope::HealthCheck {
                mode: HealthCheckMode::Complete
            },
            Some(WorkflowRoute::Agent { .. })
        )
    );
    if input.kind == WorkflowKind::HealthCheck && !durable_health_owner {
        project_access.persistence = WorkflowPersistenceMode::MemoryOnly;
    }
    let output = output_summary(environment.context, &scope)?;
    let git_policy = git_policy(environment.context, &scope)?;
    let captured_baseline = capture_baseline(
        environment.context,
        &scope,
        &evaluation.source_versions,
        Some(evaluation),
    )?;
    let mut prerequisites = prerequisites(
        environment.context,
        &scope,
        &project_access,
        &route,
        &evaluation.source_versions,
        &evaluation.wiki_pages,
        &git_policy,
        route_resolution.prerequisite_action,
        captured_baseline.has_readable_markdown,
    );
    let baseline = captured_baseline.summary;
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
        operation: crate::models::workflow::WorkflowOperation::BuiltIn,
        preparation_fingerprint: None,
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
        authority_revision: evaluation.authority_revision.clone(),
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
    lint_available: bool,
    lint_revision: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentRoutePolicy {
    Disabled,
    Any,
    LintOnly,
}

struct ProviderRouteCandidate {
    config: LlmProviderConfig,
    available: bool,
    revision: String,
}

impl RouteCatalog {
    #[cfg(test)]
    fn available_selections(&self) -> Vec<WorkflowRouteSelection> {
        self.available_selections_for(AgentRoutePolicy::Any)
    }

    fn available_selections_for(
        &self,
        agent_policy: AgentRoutePolicy,
    ) -> Vec<WorkflowRouteSelection> {
        let mut selections = AgentKind::ALL
            .into_iter()
            .filter(|kind| {
                self.agents
                    .get(kind)
                    .is_some_and(|candidate| candidate.revision_for(agent_policy).is_some())
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

    fn load(
        environment: &WorkflowPreparationEnvironment<'_>,
        project_access: &WorkflowProjectAccessSummary,
    ) -> Result<Self, BackendError> {
        #[cfg(test)]
        ROUTE_CATALOG_LOADS.with(|count| count.set(count.get() + 1));
        let settings = environment
            .settings_service
            .read_settings(environment.context)?;
        let default_agent = settings.agent_default;
        let settings_revision = hex_sha256(
            canonical_json(&default_agent)
                .map_err(serialization_error)?
                .as_bytes(),
        );
        #[cfg(test)]
        let agent_started = std::time::Instant::now();
        let detected_agents = std::thread::scope(|scope| {
            let handles = AgentKind::ALL
                .into_iter()
                .map(|kind| {
                    let settings_revision = &settings_revision;
                    let canonical_identity_key = &project_access.canonical_identity_key;
                    let identity_revision = &project_access.identity_revision;
                    (
                        kind,
                        scope.spawn(move || {
                            let started = std::time::Instant::now();
                            let result = environment
                                .agent_service
                                .detect_agent_for_workflow_lint_route(
                                    kind,
                                    default_agent == Some(kind),
                                    settings_revision,
                                    canonical_identity_key,
                                    identity_revision,
                                );
                            let elapsed =
                                u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
                            (result, elapsed)
                        }),
                    )
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|(kind, handle)| {
                    let info = match handle.join() {
                        Ok(info) => info,
                        Err(payload) => std::panic::resume_unwind(payload),
                    };
                    (kind, info)
                })
                .collect::<Vec<_>>()
        });
        #[cfg(test)]
        add_elapsed(&AGENT_NANOS, agent_started);
        #[cfg(test)]
        AGENT_PROBES.with(|count| {
            count.set(
                count.get()
                    + detected_agents
                        .iter()
                        .filter(|(_, ((_, probed, _), _))| *probed)
                        .count(),
            )
        });
        #[cfg(test)]
        SLOWEST_AGENT_PROBE_NANOS.with(|slowest| {
            slowest.set(
                detected_agents
                    .iter()
                    .filter(|(_, ((_, probed, _), _))| *probed)
                    .map(|(_, (_, elapsed))| *elapsed)
                    .max()
                    .unwrap_or(0),
            )
        });
        let agents = detected_agents
            .into_iter()
            .map(|(kind, ((info, _probed, target_revision), _elapsed))| {
                let revision = hex_sha256(
                    canonical_json(&(kind, &info.state, &info.version, &info.executable_path))
                        .unwrap_or_default()
                        .as_bytes(),
                );
                let lint_profile = AgentService::lint_route_profile_revision(kind);
                let lint_revision = lint_profile.map(|profile| {
                    hex_sha256(
                        canonical_json(&(
                            kind,
                            &info.state,
                            &info.version,
                            &info.executable_path,
                            profile,
                            &target_revision,
                        ))
                        .unwrap_or_default()
                        .as_bytes(),
                    )
                });
                (
                    kind,
                    AgentRouteCandidate {
                        available: info.state == AgentDetectionState::Installed,
                        revision,
                        lint_available: info.state == AgentDetectionState::Installed
                            && lint_profile.is_some(),
                        lint_revision,
                    },
                )
            })
            .collect();
        let mut providers = Vec::new();
        for config in settings.llm_providers {
            let binding =
                crate::services::LlmService::credential_binding(environment.context, &config)?;
            let configured_secret = if project_access.trust == WorkflowProjectTrust::Trusted {
                crate::services::LlmService::bound_secret_available(
                    environment.context,
                    environment.secret_service,
                    &config,
                )?
            } else {
                false
            };
            let available = config.enabled
                && !config.model.trim().is_empty()
                && crate::services::LlmService::validate_config(&config).is_ok()
                && configured_secret;
            let revision = hex_sha256(
                canonical_json(&(
                    config.provider,
                    &config.model,
                    &config.base_url,
                    config.context_window,
                    config.enabled,
                    configured_secret,
                    binding.as_ref().map(|binding| &binding.config_id),
                    binding.as_ref().map(|binding| binding.revision),
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
            default_agent,
            agents,
            providers,
        })
    }
}

impl AgentRouteCandidate {
    fn revision_for(&self, policy: AgentRoutePolicy) -> Option<&str> {
        match policy {
            AgentRoutePolicy::Disabled => None,
            AgentRoutePolicy::Any => self.available.then_some(self.revision.as_str()),
            AgentRoutePolicy::LintOnly => self
                .lint_available
                .then(|| self.lint_revision.as_deref())
                .flatten(),
        }
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
                // A compatible project without a configured export root must still be
                // inspectable. Keep the target unset so preparation can report the
                // explicit export-root prerequisite; execution remains fail-closed.
                None if context.layout.export_root.is_none() => None,
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
        match scope {
            WorkflowScope::HealthCheck { .. } => AgentRoutePolicy::LintOnly,
            _ => AgentRoutePolicy::Any,
        },
        matches!(scope, WorkflowScope::HealthCheck { .. }),
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
    agent_policy: AgentRoutePolicy,
    require_explicit_provider: bool,
) -> RouteResolution {
    match selection {
        Some(WorkflowRouteSelection::Agent { agent }) => {
            let route = catalog
                .agents
                .get(agent)
                .and_then(|candidate| candidate.revision_for(agent_policy))
                .map(|revision| WorkflowRoute::Agent {
                    agent: *agent,
                    model: None,
                    route_revision: revision.to_string(),
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
                    .and_then(|candidate| candidate.revision_for(agent_policy))
                    .map(|revision| WorkflowRoute::Agent {
                        agent: default_agent,
                        model: None,
                        route_revision: revision.to_string(),
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
            let route =
                (!require_explicit_provider && usable.len() == 1).then(|| WorkflowRoute::Byok {
                    provider: usable[0].config.provider,
                    model: usable[0].config.model.clone(),
                    route_revision: usable[0].revision.clone(),
                });
            RouteResolution {
                prerequisite_action: route.is_none().then_some(if usable.is_empty() {
                    WorkflowPrerequisiteAction::ConfigureExecutionRoute
                } else {
                    WorkflowPrerequisiteAction::ChooseExecutionRoute
                }),
                route,
            }
        }
    }
}

fn prerequisites(
    context: &ProjectContext,
    scope: &WorkflowScope,
    access: &WorkflowProjectAccessSummary,
    route: &Option<WorkflowRoute>,
    sources: &[SourceVersionRef],
    wiki_pages: &[String],
    git_policy: &WorkflowGitPolicy,
    route_prerequisite_action: Option<WorkflowPrerequisiteAction>,
    has_readable_markdown: bool,
) -> Vec<WorkflowPrerequisite> {
    let mut items = Vec::new();
    let local_quick = matches!(
        scope,
        WorkflowScope::HealthCheck {
            mode: HealthCheckMode::LocalQuick
        }
    );
    let prerequisite = |code: &str, message_key: String, action: WorkflowPrerequisiteAction| {
        WorkflowPrerequisite {
            code: code.into(),
            message_key,
            blocking: true,
            action,
        }
    };
    if !local_quick && access.trust == WorkflowProjectTrust::Untrusted {
        items.push(prerequisite(
            "WORKFLOW_PROJECT_UNTRUSTED",
            prerequisite_message_key(&WorkflowPrerequisiteAction::TrustProject).into(),
            WorkflowPrerequisiteAction::TrustProject,
        ));
    }
    if matches!(
        scope,
        WorkflowScope::UpdateWiki { .. } | WorkflowScope::GenerateContent { .. }
    ) && access.filesystem_access == WorkflowFilesystemAccess::ReadOnly
    {
        items.push(prerequisite(
            "WORKFLOW_PROJECT_READ_ONLY",
            prerequisite_message_key(&WorkflowPrerequisiteAction::MakeWritable).into(),
            WorkflowPrerequisiteAction::MakeWritable,
        ));
    }
    match scope {
        WorkflowScope::UpdateWiki { .. } if context.layout.wiki_write_root.is_none() => {
            items.push(prerequisite(
                "WORKFLOW_WIKI_WRITE_ROOT_REQUIRED",
                "workflows.prerequisite.wikiWriteRootRequired".into(),
                WorkflowPrerequisiteAction::MakeWritable,
            ));
        }
        WorkflowScope::GenerateContent { .. } if context.layout.export_root.is_none() => {
            items.push(prerequisite(
                "WORKFLOW_EXPORT_ROOT_REQUIRED",
                "workflows.prerequisite.exportRootRequired".into(),
                WorkflowPrerequisiteAction::MakeWritable,
            ));
        }
        _ => {}
    }
    if matches!(
        git_policy,
        WorkflowGitPolicy::RequiredBeforeWrite | WorkflowGitPolicy::RequiredBeforeOverwrite
    ) {
        match access.git_state {
            WorkflowGitState::Unavailable => items.push(prerequisite(
                "WORKFLOW_GIT_UNAVAILABLE",
                prerequisite_message_key(&WorkflowPrerequisiteAction::ConfigureGit).into(),
                WorkflowPrerequisiteAction::ConfigureGit,
            )),
            WorkflowGitState::Dirty => items.push(prerequisite(
                "WORKFLOW_GIT_DIRTY",
                prerequisite_message_key(&WorkflowPrerequisiteAction::ResolveDirtyGit).into(),
                WorkflowPrerequisiteAction::ResolveDirtyGit,
            )),
            WorkflowGitState::Clean => {}
        }
    }
    match scope {
        WorkflowScope::UpdateWiki { .. } => {
            if sources.is_empty() {
                items.push(prerequisite(
                    "WORKFLOW_SOURCES_REQUIRED",
                    prerequisite_message_key(&WorkflowPrerequisiteAction::ImportSources).into(),
                    WorkflowPrerequisiteAction::ImportSources,
                ));
            }
        }
        WorkflowScope::HealthCheck { .. } if !has_readable_markdown => items.push(prerequisite(
            "WORKFLOW_MARKDOWN_REQUIRED",
            prerequisite_message_key(&WorkflowPrerequisiteAction::ImportSources).into(),
            WorkflowPrerequisiteAction::ImportSources,
        )),
        WorkflowScope::GenerateContent { .. } if wiki_pages.is_empty() => items.push(prerequisite(
            "WORKFLOW_WIKI_REQUIRED",
            prerequisite_message_key(&WorkflowPrerequisiteAction::UpdateWiki).into(),
            WorkflowPrerequisiteAction::UpdateWiki,
        )),
        _ => {}
    }
    if !local_quick && route.is_none() {
        items.push(prerequisite(
            "WORKFLOW_ROUTE_REQUIRED",
            prerequisite_message_key(
                &route_prerequisite_action
                    .clone()
                    .unwrap_or(WorkflowPrerequisiteAction::ConfigureExecutionRoute),
            )
            .into(),
            route_prerequisite_action
                .unwrap_or(WorkflowPrerequisiteAction::ConfigureExecutionRoute),
        ));
    }
    items
}

fn capture_baseline(
    context: &ProjectContext,
    scope: &WorkflowScope,
    current_sources: &[SourceVersionRef],
    evaluation: Option<&RequestEvaluationSnapshot>,
) -> Result<CapturedBaseline, BackendError> {
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
    let (files, has_readable_markdown) = baseline_files(context, scope, evaluation)?;
    for relative in &files {
        let hash = if let Some(evaluation) = evaluation {
            evaluation.markdown_file(context, relative)?.hash.clone()
        } else {
            #[cfg(test)]
            BASELINE_HASHES.with(|count| count.set(count.get() + 1));
            FileStore.file_hash(context, relative)?
        };
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
        if let Some(evaluation) = evaluation {
            let mut resources = files.iter().try_fold(
                Vec::new(),
                |mut resources, path| -> Result<_, BackendError> {
                    resources.extend(
                        evaluation
                            .markdown_file(context, path)?
                            .resource_paths
                            .iter()
                            .cloned(),
                    );
                    Ok(resources)
                },
            )?;
            resources.sort();
            resources.dedup();
            parts.extend(export_service.workflow_baseline_entries_from_resources(
                context,
                &resources,
                output_path,
            )?);
        } else {
            parts.extend(export_service.workflow_baseline_entries(context, &files, output_path)?);
        }
    }
    parts.sort();
    Ok(CapturedBaseline {
        summary: WorkflowBaselineSummary {
            fingerprint: hex_sha256(parts.join("\n").as_bytes()),
            captured_at: Utc::now().to_rfc3339(),
            item_count: files.len() as u64 + selected.len() as u64,
        },
        has_readable_markdown,
    })
}

pub fn workflow_baseline_for_scope(
    context: &ProjectContext,
    scope: &WorkflowScope,
) -> Result<WorkflowBaselineSummary, BackendError> {
    let current_sources = CompileService::list_source_versions(context)?;
    Ok(capture_baseline(context, scope, &current_sources, None)?.summary)
}

fn baseline_files(
    context: &ProjectContext,
    scope: &WorkflowScope,
    evaluation: Option<&RequestEvaluationSnapshot>,
) -> Result<(Vec<String>, bool), BackendError> {
    let (mut files, has_readable_markdown) = match scope {
        WorkflowScope::GenerateContent { page_paths, .. } if !page_paths.is_empty() => {
            (page_paths.clone(), true)
        }
        _ => {
            let files = match evaluation {
                Some(snapshot) => snapshot.readable_markdown(context)?,
                None => list_markdown_inventory(context)?,
            };
            let has_readable_markdown = !files.is_empty();
            (files, has_readable_markdown)
        }
    };
    for document in [
        &context.layout.purpose_context,
        &context.layout.schema_context,
    ] {
        if let Some(path) = document.as_ref().and_then(|item| item.read_path.as_deref()) {
            if context.resolve_project_path(path)?.is_file() {
                files.push(path.into());
            }
        }
    }
    files.sort();
    files.dedup();
    Ok((files, has_readable_markdown))
}

fn output_summary(
    context: &ProjectContext,
    scope: &WorkflowScope,
) -> Result<WorkflowOutputSummary, BackendError> {
    match scope {
        WorkflowScope::UpdateWiki { .. } => Ok(WorkflowOutputSummary {
            label_key: "workflows.output.wiki".into(),
            location: context.layout.wiki_write_root.clone(),
            may_change_wiki: true,
        }),
        WorkflowScope::HealthCheck { .. } => Ok(WorkflowOutputSummary {
            label_key: "workflows.output.healthReport".into(),
            location: None,
            may_change_wiki: false,
        }),
        WorkflowScope::GenerateContent { output_path, .. } => {
            let location = match (context.layout.export_root.as_deref(), output_path) {
                (Some(_root), Some(path)) => {
                    let _ = context.resolve_project_path(path)?;
                    Some(path.clone())
                }
                (Some(root), None) => Some(root.into()),
                (None, _) => None,
            };
            Ok(WorkflowOutputSummary {
                label_key: "workflows.output.export".into(),
                location,
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

fn wiki_pages_from_inventory(context: &ProjectContext, inventory: &[String]) -> Vec<String> {
    use crate::models::layout::ProjectMarkdownRootRole;

    let mut pages = inventory
        .iter()
        .filter(|relative| {
            markdown_inventory_has_role(
                context,
                relative,
                &[
                    ProjectMarkdownRootRole::Wiki,
                    ProjectMarkdownRootRole::Mixed,
                ],
            ) && !markdown_inventory_has_role(context, relative, &[ProjectMarkdownRootRole::Source])
        })
        .cloned()
        .collect::<Vec<_>>();
    pages.sort();
    pages.dedup();
    pages
}

fn markdown_inventory_has_role(
    context: &ProjectContext,
    relative: &str,
    roles: &[crate::models::layout::ProjectMarkdownRootRole],
) -> bool {
    let relative = normalize_project_path(relative);
    context.layout.markdown_roots.iter().any(|root| {
        if !roles.contains(&root.role) {
            return false;
        }
        let root_path = normalize_project_path(&root.path);
        let below_root = if root_path == "." {
            !relative.contains('/')
        } else {
            relative
                .strip_prefix(&root_path)
                .is_some_and(|suffix| suffix.starts_with('/'))
        };
        below_root
            && !root
                .exclude
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|excluded| normalize_project_path(excluded))
                .any(|excluded| {
                    relative == excluded
                        || relative
                            .strip_prefix(&excluded)
                            .is_some_and(|suffix| suffix.starts_with('/'))
                })
    })
}

fn list_markdown_inventory(context: &ProjectContext) -> Result<Vec<String>, BackendError> {
    #[cfg(test)]
    MARKDOWN_ROOT_INVENTORIES.with(|count| count.set(count.get() + 1));
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

/// Persisted stage contract for the Health-owned Agent lint repair subtype.
/// It is intentionally separate from built-in Health so recovery cannot
/// replay a repair attempt through the read-only Health runner.
pub fn agent_lint_repair_stages() -> Vec<WorkflowStage> {
    let mut definitions = vec![(
        "create_checkpoint".to_string(),
        "workflows.stage.agentLintRepair.createCheckpoint".to_string(),
    )];
    for round in 1..=3 {
        for (id, label) in [
            (
                "prepare_round",
                "workflows.stage.agentLintRepair.prepareRound",
            ),
            ("run_agent", "workflows.stage.agentLintRepair.runAgent"),
            (
                "validate_candidate",
                "workflows.stage.agentLintRepair.validateCandidate",
            ),
            ("review_risk", "workflows.stage.agentLintRepair.reviewRisk"),
            (
                "apply_changes",
                "workflows.stage.agentLintRepair.applyChanges",
            ),
            (
                "recheck_lint",
                "workflows.stage.agentLintRepair.recheckLint",
            ),
        ] {
            definitions.push((format!("{id}_{round}"), label.to_string()));
        }
    }
    definitions.push((
        "finalize_repair".to_string(),
        "workflows.stage.agentLintRepair.finalizeRepair".to_string(),
    ));
    definitions
        .into_iter()
        .enumerate()
        .map(|(index, (id, label_key))| WorkflowStage {
            id,
            ordinal: (index + 1) as u32,
            status: WorkflowStageStatus::Pending,
            label_key,
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

#[cfg(test)]
mod batch_zero_cost_tests {
    use super::*;
    use crate::models::llm::{LlmProviderConfig, LlmProviderKind, ProviderCredentialBinding};
    use crate::services::WorkflowService;
    use crate::tasks::TaskService;
    use std::collections::{HashSet, VecDeque};
    use std::sync::{mpsc, Arc, Mutex};

    struct DeterministicMissingProcessRunner;

    impl crate::services::ProcessRunner for DeterministicMissingProcessRunner {
        fn find_executable(&self, _command: &str) -> Option<std::path::PathBuf> {
            None
        }

        fn run_with_timeout(
            &self,
            _command: &str,
            _args: &[&str],
            _timeout: std::time::Duration,
        ) -> Result<String, BackendError> {
            panic!("missing-agent fixture must not spawn a process")
        }

        fn run_capture(
            &self,
            _invocation: &crate::services::AgentInvocation,
        ) -> Result<(String, String), BackendError> {
            panic!("overview fixture must not capture a process")
        }

        fn run_task_streaming(
            &self,
            _invocation: &crate::services::AgentInvocation,
            _tasks: &crate::tasks::TaskService,
            _task_id: &str,
        ) -> Result<String, BackendError> {
            panic!("overview fixture must not stream a process")
        }
    }

    struct ConcurrentProbeRunner {
        entered: mpsc::Sender<()>,
        releases: Mutex<VecDeque<mpsc::Receiver<()>>>,
        resolved_commands: Mutex<HashSet<String>>,
    }

    impl crate::services::ProcessRunner for ConcurrentProbeRunner {
        fn find_executable(&self, command: &str) -> Option<std::path::PathBuf> {
            if !self
                .resolved_commands
                .lock()
                .expect("resolved command lock poisoned")
                .insert(command.to_string())
            {
                return None;
            }
            let release = self
                .releases
                .lock()
                .expect("probe release lock poisoned")
                .pop_front()
                .expect("one release channel per Agent probe");
            self.entered.send(()).unwrap();
            release.recv().unwrap();
            None
        }

        fn run_with_timeout(
            &self,
            _command: &str,
            _args: &[&str],
            _timeout: std::time::Duration,
        ) -> Result<String, BackendError> {
            panic!("missing-agent fixture must not spawn a process")
        }

        fn run_capture(
            &self,
            _invocation: &crate::services::AgentInvocation,
        ) -> Result<(String, String), BackendError> {
            panic!("overview fixture must not capture a process")
        }

        fn run_task_streaming(
            &self,
            _invocation: &crate::services::AgentInvocation,
            _tasks: &crate::tasks::TaskService,
            _task_id: &str,
        ) -> Result<String, BackendError> {
            panic!("overview fixture must not stream a process")
        }
    }

    #[test]
    fn overview_reuses_route_probe_markdown_and_hash_work_for_scale_fixture() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".app")).unwrap();
        std::fs::create_dir_all(root.path().join("wiki/scale")).unwrap();
        for index in 0..1_000 {
            std::fs::write(
                root.path().join(format!("wiki/scale/page-{index:04}.md")),
                format!("# Page {index}\n"),
            )
            .unwrap();
        }
        let context = ProjectContext::new("baseline-project", root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        let settings = SettingsService::with_config_dir(config.path().to_path_buf());
        let secrets = SecretService::memory();
        let agents =
            AgentService::with_runner(std::sync::Arc::new(DeterministicMissingProcessRunner));
        let environment = WorkflowPreparationEnvironment {
            context: &context,
            access: WorkflowAccessSnapshot {
                trust: WorkflowProjectTrust::Untrusted,
                trust_kind: None,
                filesystem_access: WorkflowFilesystemAccess::ReadOnly,
                persistence: WorkflowPersistenceMode::MemoryOnly,
                git_state: WorkflowGitState::Unavailable,
                authority_revision: "baseline-authority".into(),
            },
            settings_service: &settings,
            secret_service: &secrets,
            agent_service: &agents,
        };

        reset_preparation_costs();
        let service = WorkflowService::default();
        let tasks = TaskService::default();
        let result = service
            .project_overview(
                &context,
                environment.access.clone(),
                &settings,
                &secrets,
                &agents,
                &tasks,
            )
            .unwrap();

        assert_eq!(result.rows.len(), 3);
        assert_eq!(AgentKind::ALL.len(), 4, "Batch 0 freezes four Agent kinds");
        assert_eq!(
            preparation_costs(),
            PreparationCostSnapshot {
                source_inventories: 1,
                markdown_root_inventories: 1,
                route_catalog_loads: 1,
                agent_probes: 4,
                baseline_hashes: 1_000,
            }
        );

        service
            .project_overview(
                &context,
                environment.access.clone(),
                &settings,
                &secrets,
                &agents,
                &tasks,
            )
            .unwrap();
        assert_eq!(
            preparation_costs(),
            PreparationCostSnapshot {
                source_inventories: 2,
                markdown_root_inventories: 2,
                route_catalog_loads: 2,
                agent_probes: 4,
                baseline_hashes: 2_000,
            },
            "the TTL route cache may reuse Agent probes, but content and authority facts must remain request-fresh"
        );
    }

    #[test]
    fn provider_secret_availability_remains_request_fresh_while_agents_are_warm() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".app")).unwrap();
        std::fs::create_dir_all(root.path().join("wiki")).unwrap();
        let context = ProjectContext::new("provider-freshness", root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        let settings = SettingsService::with_config_dir(config.path().to_path_buf());
        let provider = LlmProviderConfig {
            provider: LlmProviderKind::OpenAi,
            model: "gpt-test".into(),
            base_url: "https://api.openai.com".into(),
            context_window: 8_192,
            enabled: true,
        };
        let config_id = uuid::Uuid::new_v4().to_string();
        let mut binding = ProviderCredentialBinding {
            credential_account_id: SecretService::provider_binding_account_id(
                &context,
                LlmProviderKind::OpenAi,
                &config_id,
                "https://api.openai.com",
                1,
            )
            .unwrap(),
            config_id,
            provider_kind: LlmProviderKind::OpenAi,
            canonical_origin: "https://api.openai.com".into(),
            approved_at: None,
            revision: 1,
        };
        settings
            .save_provider_with_binding(&context, provider.clone(), binding.clone())
            .unwrap();
        let secrets = SecretService::memory();
        let agents = AgentService::with_runner(Arc::new(DeterministicMissingProcessRunner));
        let environment = WorkflowPreparationEnvironment {
            context: &context,
            access: WorkflowAccessSnapshot {
                trust: WorkflowProjectTrust::Trusted,
                trust_kind: None,
                filesystem_access: WorkflowFilesystemAccess::ReadOnly,
                persistence: WorkflowPersistenceMode::MemoryOnly,
                git_state: WorkflowGitState::Unavailable,
                authority_revision: "provider-authority".into(),
            },
            settings_service: &settings,
            secret_service: &secrets,
            agent_service: &agents,
        };

        reset_preparation_costs();
        let without_secret = RequestEvaluationSnapshot::capture(&environment, false).unwrap();
        assert!(!without_secret
            .route_catalog
            .available_selections()
            .contains(&WorkflowRouteSelection::Byok {
                provider: LlmProviderKind::OpenAi,
            }));

        secrets
            .set(LlmProviderKind::OpenAi, "legacy-kind-only-secret")
            .unwrap();
        let with_legacy_secret = RequestEvaluationSnapshot::capture(&environment, false).unwrap();
        assert!(!with_legacy_secret
            .route_catalog
            .available_selections()
            .contains(&WorkflowRouteSelection::Byok {
                provider: LlmProviderKind::OpenAi,
            }));

        binding.approved_at = Some("2026-08-18T00:00:00Z".into());
        secrets
            .set_bound(&context, &binding, "origin-bound-secret")
            .unwrap();
        settings
            .save_provider_with_binding(&context, provider, binding)
            .unwrap();
        let with_secret = RequestEvaluationSnapshot::capture(&environment, false).unwrap();
        assert!(with_secret.route_catalog.available_selections().contains(
            &WorkflowRouteSelection::Byok {
                provider: LlmProviderKind::OpenAi,
            }
        ));
        assert_eq!(
            preparation_costs().agent_probes,
            AgentKind::ALL.len(),
            "provider secret changes must stay live without invalidating warm Agent probes"
        );
    }

    #[test]
    fn untrusted_or_recovery_access_never_exposes_bound_provider_availability() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".app")).unwrap();
        std::fs::create_dir_all(root.path().join("wiki")).unwrap();
        let context = ProjectContext::new("provider-restricted", root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        let settings = SettingsService::with_config_dir(config.path().to_path_buf());
        let secrets = SecretService::memory();
        let provider = LlmProviderConfig {
            provider: LlmProviderKind::OpenAi,
            model: "gpt-test".into(),
            base_url: "https://api.openai.com".into(),
            context_window: 8_192,
            enabled: true,
        };
        let config_id = uuid::Uuid::new_v4().to_string();
        let binding = ProviderCredentialBinding {
            credential_account_id: SecretService::provider_binding_account_id(
                &context,
                LlmProviderKind::OpenAi,
                &config_id,
                "https://api.openai.com",
                1,
            )
            .unwrap(),
            config_id,
            provider_kind: LlmProviderKind::OpenAi,
            canonical_origin: "https://api.openai.com".into(),
            approved_at: Some("2026-08-18T00:00:00Z".into()),
            revision: 1,
        };
        secrets
            .set_bound(&context, &binding, "restricted-project-secret")
            .unwrap();
        settings
            .save_provider_with_binding(&context, provider, binding)
            .unwrap();
        let agents = AgentService::with_runner(Arc::new(DeterministicMissingProcessRunner));
        let environment = WorkflowPreparationEnvironment {
            context: &context,
            access: WorkflowAccessSnapshot {
                trust: WorkflowProjectTrust::Untrusted,
                trust_kind: None,
                filesystem_access: WorkflowFilesystemAccess::ReadOnly,
                persistence: WorkflowPersistenceMode::MemoryOnly,
                git_state: WorkflowGitState::Unavailable,
                authority_revision: "restricted-or-recovery-authority".into(),
            },
            settings_service: &settings,
            secret_service: &secrets,
            agent_service: &agents,
        };

        let snapshot = RequestEvaluationSnapshot::capture(&environment, false).unwrap();
        assert!(!snapshot.route_catalog.available_selections().contains(
            &WorkflowRouteSelection::Byok {
                provider: LlmProviderKind::OpenAi,
            }
        ));
    }

    #[test]
    fn request_scoped_evaluation_preserves_independent_snapshot_contracts() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".app")).unwrap();
        std::fs::create_dir_all(root.path().join("wiki")).unwrap();
        std::fs::write(root.path().join("wiki/page.md"), "# Page\n").unwrap();
        let context = ProjectContext::new("contract-project", root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        let settings = SettingsService::with_config_dir(config.path().to_path_buf());
        let secrets = SecretService::memory();
        let agents = AgentService::with_runner(Arc::new(DeterministicMissingProcessRunner));
        let environment = WorkflowPreparationEnvironment {
            context: &context,
            access: WorkflowAccessSnapshot {
                trust: WorkflowProjectTrust::Untrusted,
                trust_kind: None,
                filesystem_access: WorkflowFilesystemAccess::ReadOnly,
                persistence: WorkflowPersistenceMode::MemoryOnly,
                git_state: WorkflowGitState::Unavailable,
                authority_revision: "contract-authority".into(),
            },
            settings_service: &settings,
            secret_service: &secrets,
            agent_service: &agents,
        };
        let evaluation = RequestEvaluationSnapshot::capture(&environment, true).unwrap();

        for kind in [
            WorkflowKind::UpdateWiki,
            WorkflowKind::HealthCheck,
            WorkflowKind::GenerateContent,
        ] {
            let input = PrepareWorkflowInput {
                kind,
                scope: None,
                route_selection: None,
            };
            let shared = build_snapshot_from_evaluation(&environment, &input, &evaluation).unwrap();
            let independent = build_snapshot(&environment, &input).unwrap();
            assert_eq!(shared.project_access, independent.project_access);
            assert_eq!(shared.scope, independent.scope);
            assert_eq!(
                shared.baseline.fingerprint,
                independent.baseline.fingerprint
            );
            assert_eq!(shared.baseline.item_count, independent.baseline.item_count);
            assert_eq!(shared.route, independent.route);
            assert_eq!(shared.prerequisites, independent.prerequisites);
            assert_eq!(shared.output, independent.output);
            assert_eq!(shared.git_policy, independent.git_policy);
            assert_eq!(shared.execution_options, independent.execution_options);
            assert_eq!(
                shared.preparation_fingerprint,
                independent.preparation_fingerprint
            );
            assert_eq!(
                shared.available_source_versions,
                independent.available_source_versions
            );
            assert_eq!(
                shared.available_wiki_pages,
                independent.available_wiki_pages
            );
            assert_eq!(shared.available_routes, independent.available_routes);
        }
    }

    #[test]
    fn overview_starts_all_cold_agent_probes_in_parallel() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".app")).unwrap();
        std::fs::create_dir_all(root.path().join("wiki")).unwrap();
        std::fs::write(root.path().join("wiki/page.md"), "# Page\n").unwrap();
        let context = ProjectContext::new("parallel-probes", root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        let settings = SettingsService::with_config_dir(config.path().to_path_buf());
        let secrets = SecretService::memory();
        let (entered_tx, entered_rx) = mpsc::channel();
        let mut release_senders = Vec::new();
        let mut release_receivers = VecDeque::new();
        for _ in AgentKind::ALL {
            let (release_tx, release_rx) = mpsc::channel();
            release_senders.push(release_tx);
            release_receivers.push_back(release_rx);
        }
        let agents = AgentService::with_runner(Arc::new(ConcurrentProbeRunner {
            entered: entered_tx,
            releases: Mutex::new(release_receivers),
            resolved_commands: Mutex::new(HashSet::new()),
        }));
        let worker = std::thread::spawn(move || {
            WorkflowService::default().project_overview(
                &context,
                WorkflowAccessSnapshot {
                    trust: WorkflowProjectTrust::Untrusted,
                    trust_kind: None,
                    filesystem_access: WorkflowFilesystemAccess::ReadOnly,
                    persistence: WorkflowPersistenceMode::MemoryOnly,
                    git_state: WorkflowGitState::Unavailable,
                    authority_revision: "parallel-authority".into(),
                },
                &settings,
                &secrets,
                &agents,
                &TaskService::default(),
            )
        });

        let all_entered_before_release = (0..AgentKind::ALL.len()).all(|_| {
            entered_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .is_ok()
        });
        for release in release_senders {
            release.send(()).unwrap();
        }
        worker.join().unwrap().unwrap();
        assert!(
            all_entered_before_release,
            "all cold Agent probes must start before any one probe completes"
        );
    }

    #[test]
    #[ignore = "local release performance reference for the Batch 5B stop/go gate"]
    fn overview_release_reference_reports_request_phases() {
        assert!(
            !cfg!(debug_assertions),
            "Batch 5B reference must run with cargo test --release"
        );
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".app")).unwrap();
        std::fs::create_dir_all(root.path().join("wiki/scale")).unwrap();
        for index in 0..1_000 {
            std::fs::write(
                root.path().join(format!("wiki/scale/page-{index:04}.md")),
                format!("# Page {index}\n\n![asset](../assets/shared.png)\n"),
            )
            .unwrap();
        }
        let context = ProjectContext::new("release-reference", root.path().to_path_buf());
        let config = tempfile::tempdir().unwrap();
        let settings = SettingsService::with_config_dir(config.path().to_path_buf());
        let secrets = SecretService::memory();
        let agents = AgentService::default();
        let service = WorkflowService::default();
        let tasks = TaskService::default();
        let access = WorkflowAccessSnapshot {
            trust: WorkflowProjectTrust::Untrusted,
            trust_kind: None,
            filesystem_access: WorkflowFilesystemAccess::ReadOnly,
            persistence: WorkflowPersistenceMode::MemoryOnly,
            git_state: WorkflowGitState::Unavailable,
            authority_revision: "release-reference-authority".into(),
        };
        let invoke = || {
            service
                .project_overview(
                    &context,
                    access.clone(),
                    &settings,
                    &secrets,
                    &agents,
                    &tasks,
                )
                .unwrap()
        };
        for _ in 0..5 {
            agents.invalidate_workflow_route_cache();
            reset_preparation_costs();
            invoke();
            reset_preparation_costs();
            invoke();
        }
        let mut warm_total_ms = Vec::with_capacity(50);
        let mut warm_route_ms = Vec::with_capacity(50);
        let mut warm_agent_ms = Vec::with_capacity(50);
        let mut warm_inventory_ms = Vec::with_capacity(50);
        let mut warm_markdown_ms = Vec::with_capacity(50);
        let mut cold_agent_ms = Vec::with_capacity(50);
        let mut cold_slowest_probe_ms = Vec::with_capacity(50);
        for _ in 0..50 {
            agents.invalidate_workflow_route_cache();
            reset_preparation_costs();
            invoke();
            let cold = preparation_timings();
            let cold_agent = cold.agent_nanos as f64 / 1_000_000.0;
            let cold_slowest = cold.slowest_agent_probe_nanos as f64 / 1_000_000.0;
            assert_eq!(preparation_costs().agent_probes, AgentKind::ALL.len());
            assert!(
                cold_agent <= cold_slowest + 500.0,
                "cold Agent phase must stay within the slowest probe plus 500ms: phase={cold_agent:.3}ms slowest={cold_slowest:.3}ms"
            );
            cold_agent_ms.push(cold_agent);
            cold_slowest_probe_ms.push(cold_slowest);

            reset_preparation_costs();
            let started = std::time::Instant::now();
            invoke();
            warm_total_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
            let warm = preparation_timings();
            warm_route_ms.push(warm.route_nanos as f64 / 1_000_000.0);
            warm_agent_ms.push(warm.agent_nanos as f64 / 1_000_000.0);
            warm_inventory_ms.push(warm.inventory_nanos as f64 / 1_000_000.0);
            warm_markdown_ms.push(warm.markdown_nanos as f64 / 1_000_000.0);
            assert_eq!(
                preparation_costs().agent_probes,
                0,
                "TTL-warm overview must spawn no Agent probe subprocesses"
            );
        }
        let warm_total = sample_stats(&warm_total_ms);
        let warm_route = sample_stats(&warm_route_ms);
        let warm_agent = sample_stats(&warm_agent_ms);
        let warm_inventory = sample_stats(&warm_inventory_ms);
        let warm_markdown = sample_stats(&warm_markdown_ms);
        let cold_agent = sample_stats(&cold_agent_ms);
        let cold_slowest = sample_stats(&cold_slowest_probe_ms);
        let agent_kinds = AgentKind::ALL
            .iter()
            .map(|kind| format!("{kind:?}").to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join(",");
        eprintln!(
            "BATCH5B_OVERVIEW_REFERENCE profile=release cache_mode=explicit_cold_then_ttl_warm ttl_secs=30 agent_kinds={} os={} arch={} parallelism={} samples=50 warm_total_mean_ms={:.3} warm_total_p95_ms={:.3} warm_total_cv={:.4} warm_route_non_agent_mean_ms={:.3} warm_route_non_agent_p95_ms={:.3} warm_agent_mean_ms={:.3} warm_agent_p95_ms={:.3} warm_inventory_mean_ms={:.3} warm_inventory_p95_ms={:.3} warm_markdown_mean_ms={:.3} warm_markdown_p95_ms={:.3} cold_agent_mean_ms={:.3} cold_agent_p95_ms={:.3} cold_slowest_probe_mean_ms={:.3} cold_slowest_probe_p95_ms={:.3}",
            agent_kinds,
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::thread::available_parallelism().map_or(0, |value| value.get()),
            warm_total.mean,
            warm_total.p95,
            warm_total.cv,
            warm_route.mean,
            warm_route.p95,
            warm_agent.mean,
            warm_agent.p95,
            warm_inventory.mean,
            warm_inventory.p95,
            warm_markdown.mean,
            warm_markdown.p95,
            cold_agent.mean,
            cold_agent.p95,
            cold_slowest.mean,
            cold_slowest.p95,
        );
        assert!(warm_total.cv < 0.15, "warm overview CV must stay below 15%");
        assert!(
            warm_total.p95 <= 1_000.0,
            "TTL-warm 1,000-Markdown overview p95 must stay within 1 second"
        );
        assert_eq!(preparation_costs().source_inventories, 1);
        assert_eq!(preparation_costs().markdown_root_inventories, 1);
        assert_eq!(preparation_costs().route_catalog_loads, 1);
        assert_eq!(preparation_costs().agent_probes, 0);
        assert_eq!(preparation_costs().baseline_hashes, 1_000);
    }

    struct SampleStats {
        mean: f64,
        p95: f64,
        cv: f64,
    }

    fn sample_stats(samples: &[f64]) -> SampleStats {
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let variance = samples
            .iter()
            .map(|sample| (sample - mean).powi(2))
            .sum::<f64>()
            / samples.len() as f64;
        let mut ordered = samples.to_vec();
        ordered.sort_by(f64::total_cmp);
        let p95_index = ((ordered.len() as f64 * 0.95).ceil() as usize)
            .saturating_sub(1)
            .min(ordered.len() - 1);
        SampleStats {
            mean,
            p95: ordered[p95_index],
            cv: variance.sqrt() / mean,
        }
    }
}

#[cfg(test)]
mod batch_one_record_tests {
    use super::*;

    fn prepared_record(
        identity_key: &str,
        ordinal: i64,
        expires_at: chrono::DateTime<Utc>,
    ) -> PreparedRecord {
        let created_at = expires_at - Duration::minutes(10) + Duration::milliseconds(ordinal);
        let preparation_revision = format!("{ordinal:064x}");
        PreparedRecord {
            preparation: WorkflowPreparation {
                schema_version: WORKFLOW_SCHEMA_VERSION,
                preparation_id: format!("preparation-{ordinal}"),
                preparation_revision: preparation_revision.clone(),
                project_access: WorkflowProjectAccessSummary {
                    project_id: format!("runtime-{identity_key}"),
                    canonical_identity_key: identity_key.to_string(),
                    identity_revision: "revision".into(),
                    trust: WorkflowProjectTrust::Trusted,
                    filesystem_access: WorkflowFilesystemAccess::Writable,
                    persistence: WorkflowPersistenceMode::MemoryOnly,
                    git_state: WorkflowGitState::Unavailable,
                },
                kind: WorkflowKind::HealthCheck,
                scope: WorkflowScope::HealthCheck {
                    mode: HealthCheckMode::LocalQuick,
                },
                baseline: WorkflowBaselineSummary {
                    fingerprint: "a".repeat(64),
                    captured_at: created_at.to_rfc3339(),
                    item_count: 0,
                },
                route: Some(WorkflowRoute::Local {
                    route_revision: "local-v1".into(),
                }),
                prerequisites: Vec::new(),
                output: WorkflowOutputSummary {
                    label_key: "workflows.output.healthReport".into(),
                    location: None,
                    may_change_wiki: false,
                },
                git_policy: WorkflowGitPolicy::NotRequired,
                requires_scope_confirmation: true,
                quick_rerun_eligible: false,
                available_source_versions: Vec::new(),
                available_wiki_pages: Vec::new(),
                available_routes: Vec::new(),
            },
            authority_revision: "authority".into(),
            route_selection: None,
            execution_options: WorkflowExecutionOptions {
                preparation_revision,
                ..WorkflowExecutionOptions::default()
            },
            preparation_fingerprint: "b".repeat(64),
            created_at,
            expires_at,
        }
    }

    #[test]
    fn global_and_started_caps_are_hard_and_oldest_first() {
        let mut records = PreparationRecords::default();
        let expiry = Utc::now() + Duration::minutes(10);
        for ordinal in 0..(MAX_PREPARATIONS_GLOBAL as i64 + 8) {
            let identity_key = format!("owner-{}", ordinal % 10);
            records.prepared.insert(
                format!("prepared-{ordinal}"),
                prepared_record(&identity_key, ordinal, expiry),
            );
        }
        enforce_prepared_caps(&mut records, "owner-0:revision");
        assert_eq!(records.prepared.len(), MAX_PREPARATIONS_GLOBAL);
        assert!(!records.prepared.contains_key("prepared-0"));
        assert!(records
            .prepared
            .contains_key(&format!("prepared-{}", MAX_PREPARATIONS_GLOBAL + 7)));

        for ordinal in 0..=MAX_STARTED_PREPARATIONS_PER_IDENTITY {
            records.started.insert(
                format!("started-{ordinal}"),
                StartedPreparationRecord {
                    preparation_revision: format!("{ordinal:064x}"),
                    task_id: format!("task-{ordinal}"),
                    owner: "started-owner".into(),
                    started_at: Utc::now() + Duration::milliseconds(ordinal as i64),
                    expires_at: expiry,
                },
            );
        }
        enforce_started_caps(&mut records, "started-owner");
        assert_eq!(
            records
                .started
                .values()
                .filter(|record| record.owner == "started-owner")
                .count(),
            MAX_STARTED_PREPARATIONS_PER_IDENTITY
        );
        assert!(!records.started.contains_key("started-0"));

        records.started.clear();
        let started_base = Utc::now();
        for ordinal in 0..(MAX_STARTED_PREPARATIONS_GLOBAL + 8) {
            let owner = format!("global-owner-{}", ordinal % 32);
            records.started.insert(
                format!("global-started-{ordinal}"),
                StartedPreparationRecord {
                    preparation_revision: format!("{ordinal:064x}"),
                    task_id: format!("global-task-{ordinal}"),
                    owner,
                    started_at: started_base + Duration::milliseconds(ordinal as i64),
                    expires_at: expiry,
                },
            );
        }
        enforce_started_caps(&mut records, "global-owner-7");
        assert_eq!(records.started.len(), MAX_STARTED_PREPARATIONS_GLOBAL);
        assert!(!records.started.contains_key("global-started-0"));
        assert!(records.started.contains_key(&format!(
            "global-started-{}",
            MAX_STARTED_PREPARATIONS_GLOBAL + 7
        )));
    }

    #[test]
    fn prune_removes_expired_prepared_and_started_records() {
        let now = Utc::now();
        let mut records = PreparationRecords::default();
        records.prepared.insert(
            "expired-prepared".into(),
            prepared_record("owner", 1, now - Duration::seconds(1)),
        );
        records.started.insert(
            "expired-started".into(),
            StartedPreparationRecord {
                preparation_revision: "revision".into(),
                task_id: "task".into(),
                owner: "owner:revision".into(),
                started_at: now - Duration::hours(25),
                expires_at: now - Duration::seconds(1),
            },
        );

        prune_preparation_records(&mut records, now);

        assert!(records.prepared.is_empty());
        assert!(records.started.is_empty());
    }
}
