use std::collections::{HashMap, HashSet};
use std::fs;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{
    BackendError, IMPORT_V2_ITEM_NOT_FOUND, IMPORT_V2_SELECTION_STALE,
    IMPORT_V2_SESSION_CURSOR_INVALID, IMPORT_V2_SESSION_CURSOR_STALE, IMPORT_V2_SESSION_INVALID,
    IMPORT_V2_SESSION_NOT_FOUND, IMPORT_V2_STATE_INVALID,
};
use crate::models::import_v2::{
    ImportCollectionChildRelation, ImportCollectionRelation, ImportInput, ImportItem,
    ImportItemPage, ImportItemPageFilter, ImportItemStatus, ImportMediaAuthorization,
    ImportResourceMode, ImportSelectionSummary, ImportSession, ImportSessionCounts,
    ImportSessionIndexState, ImportSessionOverview, ImportSessionStatus, QualityLevel,
    IMPORT_V2_SCHEMA_VERSION,
};
use crate::models::paths::ProjectContext;
use crate::services::import_v2::transaction::FileTransaction;
use crate::services::FileStore;

#[derive(Default)]
pub struct SessionStore;

#[derive(Debug, Clone)]
pub struct CollectionImportInput {
    pub input: ImportInput,
    pub discovery_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRecord {
    schema_version: u32,
    session_id: String,
    project_id: String,
    status: ImportSessionStatus,
    resource_mode: ImportResourceMode,
    created_at: String,
    updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discovery_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    media_authorizations: Vec<ImportMediaAuthorization>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    collection_relations: Vec<ImportCollectionRelation>,
    item_ids: Vec<String>,
}

const SESSION_CONTROL_SCHEMA_VERSION: u32 = 1;
const ACTIVE_SESSION_SCHEMA_VERSION: u32 = 1;
const ORDER_PAGE_SCHEMA_VERSION: u32 = 1;
const ORDER_PAGE_SIZE: usize = 256;
pub const MAX_SESSION_ITEM_PAGE_SIZE: u16 = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionControlRecord {
    schema_version: u32,
    session_id: String,
    project_id: String,
    status: ImportSessionStatus,
    resource_mode: ImportResourceMode,
    created_at: String,
    updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discovery_task_id: Option<String>,
    semantic_revision: u64,
    selection_revision: u64,
    confirmation_digest: String,
    item_count: u64,
    counts: ImportSessionCounts,
    selection: ImportSelectionSummary,
    status_counts: ImportItemStatusCounts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportItemStatusCounts {
    queued: u64,
    inspecting: u64,
    waiting_capability: u64,
    waiting_login: u64,
    waiting_authorization: u64,
    extracting: u64,
    validating: u64,
    preview_ready: u64,
    needs_merge: u64,
    committing: u64,
    completed: u64,
    paused: u64,
    cancelled: u64,
    skipped: u64,
    failed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActiveSessionPointer {
    schema_version: u32,
    session_id: String,
    status: ImportSessionStatus,
    control_revision: u64,
    summary_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionOrderPage {
    schema_version: u32,
    session_id: String,
    page_index: u64,
    item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SessionItemCursor {
    version: u8,
    session_id: String,
    filter: ImportItemPageFilter,
    snapshot_revision: u64,
    after: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct ItemProjection {
    all: u64,
    active: u64,
    ready: u64,
    needs_action: u64,
    failed: u64,
    completed: u64,
    waiting: u64,
    processed: u64,
    cancelled: u64,
    selected: u64,
    new_sources: u64,
    updates: u64,
    warnings: u64,
    pending: u64,
    restricted: u64,
}

impl From<&ImportSession> for SessionRecord {
    fn from(session: &ImportSession) -> Self {
        Self {
            schema_version: session.schema_version,
            session_id: session.session_id.clone(),
            project_id: session.project_id.clone(),
            status: session.status.clone(),
            resource_mode: session.resource_mode.clone(),
            created_at: session.created_at.clone(),
            updated_at: session.updated_at.clone(),
            discovery_task_id: session.discovery_task_id.clone(),
            media_authorizations: session.media_authorizations.clone(),
            collection_relations: session.collection_relations.clone(),
            item_ids: session
                .items
                .iter()
                .map(|item| item.item_id.clone())
                .collect(),
        }
    }
}

fn invalid_session(message: &str) -> BackendError {
    BackendError::new(IMPORT_V2_SESSION_INVALID, message, false, true)
}

fn validate_id(value: &str) -> Result<(), BackendError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(invalid_session("Import session identifier is invalid."))
    }
}

fn session_root(context: &ProjectContext, session_id: &str) -> Result<String, BackendError> {
    let root = context.layout.import_state_root.as_deref().ok_or_else(|| {
        BackendError::new(
            IMPORT_V2_STATE_INVALID,
            "Import state is unavailable for this project layout.",
            true,
            false,
        )
    })?;
    Ok(format!("{root}/{session_id}"))
}

fn active_session_path(context: &ProjectContext) -> Result<String, BackendError> {
    let root = context.layout.import_state_root.as_deref().ok_or_else(|| {
        BackendError::new(
            IMPORT_V2_STATE_INVALID,
            "Import state is unavailable for this project layout.",
            true,
            false,
        )
    })?;
    Ok(format!("{root}/active-session.json"))
}

fn item_projection(item: &ImportItem) -> ItemProjection {
    let active = matches!(
        item.status,
        ImportItemStatus::Queued
            | ImportItemStatus::Inspecting
            | ImportItemStatus::Extracting
            | ImportItemStatus::Validating
            | ImportItemStatus::Committing
    );
    let resolution = item
        .preview
        .as_ref()
        .and_then(|preview| preview.resolution.as_ref());
    let exact_duplicate = resolution.is_some_and(|value| {
        value.kind == crate::models::import_v2::ImportResolutionKind::ExactDuplicate
    });
    let resolved_merge = matches!(item.status, ImportItemStatus::NeedsMerge)
        && resolution
            .and_then(|value| value.default_resolution.as_ref())
            .is_some();
    let ready = (matches!(item.status, ImportItemStatus::PreviewReady) && !exact_duplicate)
        || (matches!(item.status, ImportItemStatus::PreviewReady)
            && exact_duplicate
            && item.restricted_content)
        || resolved_merge;
    let needs_action = matches!(
        item.status,
        ImportItemStatus::WaitingCapability
            | ImportItemStatus::WaitingLogin
            | ImportItemStatus::WaitingAuthorization
            | ImportItemStatus::Paused
    ) || (matches!(item.status, ImportItemStatus::NeedsMerge)
        && !resolved_merge);
    let failed = item.status == ImportItemStatus::Failed;
    let completed = item.status == ImportItemStatus::Completed;
    let skipped = item.status == ImportItemStatus::Skipped;
    let cancelled = item.status == ImportItemStatus::Cancelled;
    let processed = matches!(
        item.status,
        ImportItemStatus::PreviewReady
            | ImportItemStatus::NeedsMerge
            | ImportItemStatus::Completed
            | ImportItemStatus::Failed
            | ImportItemStatus::Cancelled
            | ImportItemStatus::Skipped
    );
    let waiting = matches!(
        item.status,
        ImportItemStatus::WaitingCapability
            | ImportItemStatus::WaitingLogin
            | ImportItemStatus::WaitingAuthorization
    );
    let committable = item_is_snapshot_committable(item);
    let update = resolution.is_some_and(|value| {
        matches!(
            value.kind,
            crate::models::import_v2::ImportResolutionKind::SameSourceNewVersion
                | crate::models::import_v2::ImportResolutionKind::NeedsThreeWayMerge
        )
    });
    let new_source = resolution.is_some_and(|value| {
        value.kind == crate::models::import_v2::ImportResolutionKind::NewSource
    });
    let warning = item
        .preview
        .as_ref()
        .is_some_and(|preview| preview.quality.level == QualityLevel::Warning);
    ItemProjection {
        all: u64::from(!completed && !skipped),
        active: u64::from(active),
        ready: u64::from(ready && !completed && !skipped),
        needs_action: u64::from(needs_action && !completed && !skipped),
        failed: u64::from(failed),
        completed: u64::from(completed),
        waiting: u64::from(waiting),
        processed: u64::from(processed),
        cancelled: u64::from(cancelled),
        selected: u64::from(committable),
        new_sources: u64::from(committable && new_source),
        updates: u64::from(committable && update),
        warnings: u64::from(committable && warning),
        pending: u64::from(needs_action || failed),
        restricted: u64::from(committable && item.restricted_content),
    }
}

pub(crate) fn item_is_snapshot_committable(item: &ImportItem) -> bool {
    let Some(preview) = item.preview.as_ref() else {
        return false;
    };
    let resolution = preview.resolution.as_ref();
    let exact_duplicate = resolution.is_some_and(|value| {
        value.kind == crate::models::import_v2::ImportResolutionKind::ExactDuplicate
    });
    let resolved_merge = item.status == ImportItemStatus::NeedsMerge
        && resolution
            .and_then(|value| value.default_resolution.as_ref())
            .is_some();
    item.selected
        && preview.quality.level != QualityLevel::Fail
        && ((item.status == ImportItemStatus::PreviewReady
            && (!exact_duplicate || item.restricted_content))
            || resolved_merge)
}

fn counts_and_selection(items: &[ImportItem]) -> (ImportSessionCounts, ImportSelectionSummary) {
    let mut counts = ImportSessionCounts::default();
    let mut selection = ImportSelectionSummary::default();
    for item in items {
        let value = item_projection(item);
        counts.all += value.all;
        counts.active += value.active;
        counts.ready += value.ready;
        counts.needs_action += value.needs_action;
        counts.failed += value.failed;
        counts.completed += value.completed;
        counts.waiting += value.waiting;
        counts.processed += value.processed;
        counts.cancelled += value.cancelled;
        selection.selected += value.selected;
        selection.new_sources += value.new_sources;
        selection.updates += value.updates;
        selection.warnings += value.warnings;
        selection.pending += value.pending;
        selection.restricted += value.restricted;
    }
    (counts, selection)
}

fn status_counts(items: &[ImportItem]) -> ImportItemStatusCounts {
    let mut counts = ImportItemStatusCounts::default();
    for item in items {
        apply_status_count(&mut counts, &item.status, 1, 0);
    }
    counts
}

fn apply_status_count(
    counts: &mut ImportItemStatusCounts,
    status: &ImportItemStatus,
    after: u64,
    before: u64,
) {
    let value = match status {
        ImportItemStatus::Queued => &mut counts.queued,
        ImportItemStatus::Inspecting => &mut counts.inspecting,
        ImportItemStatus::WaitingCapability => &mut counts.waiting_capability,
        ImportItemStatus::WaitingLogin => &mut counts.waiting_login,
        ImportItemStatus::WaitingAuthorization => &mut counts.waiting_authorization,
        ImportItemStatus::Extracting => &mut counts.extracting,
        ImportItemStatus::Validating => &mut counts.validating,
        ImportItemStatus::PreviewReady => &mut counts.preview_ready,
        ImportItemStatus::NeedsMerge => &mut counts.needs_merge,
        ImportItemStatus::Committing => &mut counts.committing,
        ImportItemStatus::Completed => &mut counts.completed,
        ImportItemStatus::Paused => &mut counts.paused,
        ImportItemStatus::Cancelled => &mut counts.cancelled,
        ImportItemStatus::Skipped => &mut counts.skipped,
        ImportItemStatus::Failed => &mut counts.failed,
    };
    *value = value.saturating_sub(before).saturating_add(after);
}

fn confirmation_digest(
    session_id: &str,
    selection_revision: u64,
    selection: &ImportSelectionSummary,
) -> String {
    let value = format!(
        "{session_id}\0{selection_revision}\0{}\0{}\0{}\0{}\0{}\0{}",
        selection.selected,
        selection.new_sources,
        selection.updates,
        selection.warnings,
        selection.pending,
        selection.restricted,
    );
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn control_from_session(session: &ImportSession, revision: u64) -> SessionControlRecord {
    let (counts, selection) = counts_and_selection(&session.items);
    let semantic_revision = revision.max(1);
    let selection_revision = revision.max(1);
    let confirmation_digest =
        confirmation_digest(&session.session_id, selection_revision, &selection);
    SessionControlRecord {
        schema_version: SESSION_CONTROL_SCHEMA_VERSION,
        session_id: session.session_id.clone(),
        project_id: session.project_id.clone(),
        status: super::orchestrator::derive_session_status(&session.items),
        resource_mode: session.resource_mode.clone(),
        created_at: session.created_at.clone(),
        updated_at: session.updated_at.clone(),
        discovery_task_id: session.discovery_task_id.clone(),
        semantic_revision,
        selection_revision,
        confirmation_digest,
        item_count: session.items.len() as u64,
        counts,
        selection,
        status_counts: status_counts(&session.items),
    }
}

fn pointer_from_control(
    control: &SessionControlRecord,
) -> Result<ActiveSessionPointer, BackendError> {
    let bytes = serde_json::to_vec(control)
        .map_err(|_| invalid_session("Import session control record could not be serialized."))?;
    Ok(ActiveSessionPointer {
        schema_version: ACTIVE_SESSION_SCHEMA_VERSION,
        session_id: control.session_id.clone(),
        status: control.status.clone(),
        control_revision: control.semantic_revision,
        summary_hash: format!("{:x}", Sha256::digest(bytes)),
    })
}

fn overview_from_control(control: SessionControlRecord) -> ImportSessionOverview {
    ImportSessionOverview {
        schema_version: IMPORT_V2_SCHEMA_VERSION,
        session_id: control.session_id,
        project_id: control.project_id,
        status: control.status,
        resource_mode: control.resource_mode,
        created_at: control.created_at,
        updated_at: control.updated_at,
        discovery_task_id: control.discovery_task_id,
        item_count: control.item_count,
        semantic_revision: control.semantic_revision,
        selection_revision: control.selection_revision,
        confirmation_digest: control.confirmation_digest,
        counts: control.counts,
        selection: control.selection,
        index_state: ImportSessionIndexState::Ready,
    }
}

fn item_matches_filter(item: &ImportItem, filter: &ImportItemPageFilter) -> bool {
    let value = item_projection(item);
    match filter {
        ImportItemPageFilter::All => value.all == 1,
        ImportItemPageFilter::Active => value.active == 1,
        ImportItemPageFilter::Ready => value.ready == 1,
        ImportItemPageFilter::NeedsAction => value.needs_action == 1,
        ImportItemPageFilter::Failed => value.failed == 1,
        ImportItemPageFilter::Completed => value.completed == 1,
    }
}

fn filter_total(control: &SessionControlRecord, filter: &ImportItemPageFilter) -> u64 {
    match filter {
        ImportItemPageFilter::All => control.counts.all,
        ImportItemPageFilter::Active => control.counts.active,
        ImportItemPageFilter::Ready => control.counts.ready,
        ImportItemPageFilter::NeedsAction => control.counts.needs_action,
        ImportItemPageFilter::Failed => control.counts.failed,
        ImportItemPageFilter::Completed => control.counts.completed,
    }
}

fn status_from_control(control: &SessionControlRecord) -> ImportSessionStatus {
    let statuses = &control.status_counts;
    if statuses.inspecting + statuses.extracting + statuses.validating + statuses.committing > 0 {
        return ImportSessionStatus::Processing;
    }
    if statuses.completed > 0 && statuses.failed + statuses.cancelled > 0 {
        return ImportSessionStatus::PartiallyCommitted;
    }
    if control.item_count > 0
        && statuses.completed + statuses.skipped + statuses.cancelled == control.item_count
        && statuses.completed + statuses.skipped > 0
    {
        return ImportSessionStatus::Completed;
    }
    if control.item_count > 0 && statuses.cancelled == control.item_count {
        return ImportSessionStatus::Cancelled;
    }
    if statuses.preview_ready
        + statuses.needs_merge
        + statuses.waiting_capability
        + statuses.waiting_login
        + statuses.waiting_authorization
        + statuses.failed
        > 0
    {
        return ImportSessionStatus::WaitingForConfirmation;
    }
    ImportSessionStatus::Draft
}

impl SessionStore {
    fn read_control(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
    ) -> Result<SessionControlRecord, BackendError> {
        let root = session_root(context, session_id)?;
        let control: SessionControlRecord =
            file_store.read_json(context, &format!("{root}/state.json"))?;
        if control.schema_version != SESSION_CONTROL_SCHEMA_VERSION
            || control.session_id != session_id
            || control.project_id != context.project_id
        {
            return Err(invalid_session("Import session control record is invalid."));
        }
        Ok(control)
    }

    fn control_writes_for_item_changes(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        changes: &[(&ImportItem, &ImportItem)],
    ) -> Result<Vec<(std::path::PathBuf, Vec<u8>, String)>, BackendError> {
        let root = session_root(context, session_id)?;
        let control_path = format!("{root}/state.json");
        let before_control = self.read_control(context, file_store, session_id)?;
        let mut after_control = before_control.clone();
        after_control.semantic_revision = before_control.semantic_revision.saturating_add(1);
        after_control.updated_at = chrono::Utc::now().to_rfc3339();
        let mut selection_changed = false;
        for (before, after) in changes {
            if before.item_id != after.item_id {
                return Err(invalid_session("Import item identity changed."));
            }
            let before_projection = item_projection(before);
            let after_projection = item_projection(after);
            apply_projection_delta(
                &mut after_control.counts,
                &mut after_control.selection,
                before_projection,
                after_projection,
            );
            apply_status_count(&mut after_control.status_counts, &before.status, 0, 1);
            apply_status_count(&mut after_control.status_counts, &after.status, 1, 0);
            selection_changed |= selection_projection_changed(before_projection, after_projection);
        }
        if selection_changed {
            after_control.selection_revision = before_control.selection_revision.saturating_add(1);
        }
        after_control.status = status_from_control(&after_control);
        after_control.confirmation_digest = confirmation_digest(
            session_id,
            after_control.selection_revision,
            &after_control.selection,
        );

        let pointer_path = active_session_path(context)?;
        let before_pointer: ActiveSessionPointer = file_store.read_json(context, &pointer_path)?;
        let expected_pointer = pointer_from_control(&before_control)?;
        if before_pointer.session_id != session_id
            || before_pointer.control_revision != expected_pointer.control_revision
            || before_pointer.summary_hash != expected_pointer.summary_hash
            || before_pointer.status != expected_pointer.status
        {
            return Err(invalid_session(
                "Active import session pointer does not match the item update.",
            ));
        }
        let after_pointer = pointer_from_control(&after_control)?;
        Ok(vec![
            (
                context.resolve_project_path(&control_path)?,
                pretty_bytes(&after_control, "Import session control record")?,
                format!(
                    "{:x}",
                    Sha256::digest(pretty_bytes(
                        &before_control,
                        "Import session control record"
                    )?)
                ),
            ),
            (
                context.resolve_project_path(&pointer_path)?,
                pretty_bytes(&after_pointer, "Active import session pointer")?,
                format!(
                    "{:x}",
                    Sha256::digest(pretty_bytes(
                        &before_pointer,
                        "Active import session pointer"
                    )?)
                ),
            ),
        ])
    }

    pub(crate) fn stage_item_sidecar_update(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        transaction: &mut FileTransaction,
        session_id: &str,
        before: &ImportItem,
        after: &ImportItem,
    ) -> Result<Vec<(std::path::PathBuf, usize)>, BackendError> {
        let writes = self.control_writes_for_item_changes(
            context,
            file_store,
            session_id,
            &[(before, after)],
        )?;
        // The caller may already have staged canonical source/history/item
        // writes in this transaction. Appending each checked sidecar keeps the
        // existing journal cohort intact; the bulk helper starts a fresh
        // checked-replacement cohort and would discard those prior intents.
        for (path, bytes, expected_hash) in &writes {
            transaction.write_if_hash_matches(path, bytes, expected_hash)?;
        }
        Ok(writes
            .iter()
            .map(|(path, bytes, _)| (path.clone(), bytes.len()))
            .collect())
    }

    pub fn rebuild_sidecars(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session: &ImportSession,
    ) -> Result<(), BackendError> {
        let previous_revision = self
            .read_control(context, file_store, &session.session_id)
            .map(|control| control.semantic_revision.saturating_add(1))
            .unwrap_or(1);
        self.write_sidecars(context, file_store, session, previous_revision)
    }

    pub fn find_unfinished_session(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
    ) -> Result<Option<String>, BackendError> {
        let pointer_path = active_session_path(context)?;
        if file_store.exists(context, &pointer_path) {
            if let Ok(pointer) =
                file_store.read_json::<ActiveSessionPointer>(context, &pointer_path)
            {
                if pointer.schema_version == ACTIVE_SESSION_SCHEMA_VERSION
                    && validate_id(&pointer.session_id).is_ok()
                {
                    if let Ok(control) = self.read_control(context, file_store, &pointer.session_id)
                    {
                        let expected = pointer_from_control(&control)?;
                        if pointer.control_revision == expected.control_revision
                            && pointer.summary_hash == expected.summary_hash
                            && pointer.status == expected.status
                        {
                            return Ok((!matches!(
                                control.status,
                                ImportSessionStatus::Completed | ImportSessionStatus::Cancelled
                            ))
                            .then_some(pointer.session_id));
                        }
                    }
                }
            }
        }

        let import_state_root = context.layout.import_state_root.as_deref().ok_or_else(|| {
            BackendError::new(
                IMPORT_V2_STATE_INVALID,
                "Import state is unavailable for this project layout.",
                true,
                false,
            )
        })?;
        let root = context.resolve_project_path(import_state_root)?;
        let metadata = match fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(BackendError::new(
                    "IMPORT_V2_SESSION_SCAN_FAILED",
                    error.to_string(),
                    true,
                    true,
                ))
            }
        };
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || crate::services::import_v2::transaction::is_project_reparse_point(&metadata)
        {
            return Err(BackendError::new(
                "IMPORT_V2_SESSION_SCAN_FAILED",
                "Import session directory is not safe.",
                false,
                true,
            ));
        }
        let mut candidates = Vec::new();
        for entry in fs::read_dir(root)
            .map_err(|error| {
                BackendError::new(
                    "IMPORT_V2_SESSION_SCAN_FAILED",
                    error.to_string(),
                    true,
                    true,
                )
            })?
            .flatten()
        {
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if !metadata.is_dir()
                || metadata.file_type().is_symlink()
                || crate::services::import_v2::transaction::is_project_reparse_point(&metadata)
            {
                continue;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            if validate_id(&id).is_err() {
                continue;
            }
            let root = session_root(context, &id)?;
            let record = match file_store
                .read_json::<SessionRecord>(context, &format!("{root}/session.json"))
            {
                Ok(record) => record,
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        crate::errors::IMPORT_V2_SESSION_INVALID
                            | crate::errors::IMPORT_V2_SESSION_NOT_FOUND
                            | "JSON_PARSE_FAILED"
                            | "FILE_READ_FAILED"
                    ) =>
                {
                    continue
                }
                Err(error) => return Err(error),
            };
            if record.session_id != id || record.project_id != context.project_id {
                continue;
            }
            if !matches!(
                record.status,
                ImportSessionStatus::Completed | ImportSessionStatus::Cancelled
            ) {
                candidates.push((record.updated_at, id));
            }
        }
        candidates.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        Ok(candidates.pop().map(|(_, id)| id))
    }

    fn write_sidecars(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session: &ImportSession,
        revision: u64,
    ) -> Result<(), BackendError> {
        let root = session_root(context, &session.session_id)?;
        file_store.ensure_dir(context, &format!("{root}/order"))?;
        for (page_index, item_ids) in session
            .items
            .chunks(ORDER_PAGE_SIZE)
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.item_id.clone())
                    .collect::<Vec<_>>()
            })
            .enumerate()
        {
            file_store.write_json_atomic(
                context,
                &format!("{root}/order/{page_index:06}.json"),
                &SessionOrderPage {
                    schema_version: ORDER_PAGE_SCHEMA_VERSION,
                    session_id: session.session_id.clone(),
                    page_index: page_index as u64,
                    item_ids,
                },
            )?;
        }
        let control = control_from_session(session, revision);
        file_store.write_json_atomic(context, &format!("{root}/state.json"), &control)?;
        file_store.write_json_atomic(
            context,
            &active_session_path(context)?,
            &pointer_from_control(&control)?,
        )
    }

    pub fn read_overview(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
    ) -> Result<ImportSessionOverview, BackendError> {
        validate_id(session_id)?;
        let root = session_root(context, session_id)?;
        let control_path = format!("{root}/state.json");
        if file_store.exists(context, &control_path) {
            if let Ok(control) =
                file_store.read_json::<SessionControlRecord>(context, &control_path)
            {
                if control.schema_version == SESSION_CONTROL_SCHEMA_VERSION
                    && control.session_id == session_id
                    && control.project_id == context.project_id
                {
                    return Ok(overview_from_control(control));
                }
            }
        }

        let summary_path = format!("{root}/session.json");
        if !file_store.exists(context, &summary_path) {
            return Err(BackendError::new(
                IMPORT_V2_SESSION_NOT_FOUND,
                "Import session was not found.",
                true,
                false,
            ));
        }
        let record: SessionRecord = file_store.read_json(context, &summary_path)?;
        if record.schema_version != IMPORT_V2_SCHEMA_VERSION
            || record.session_id != session_id
            || record.project_id != context.project_id
        {
            return Err(invalid_session(
                "Import session metadata does not match the current project.",
            ));
        }
        Ok(ImportSessionOverview {
            schema_version: record.schema_version,
            session_id: record.session_id,
            project_id: record.project_id,
            status: record.status,
            resource_mode: record.resource_mode,
            created_at: record.created_at,
            updated_at: record.updated_at,
            discovery_task_id: record.discovery_task_id,
            item_count: record.item_ids.len() as u64,
            semantic_revision: 0,
            selection_revision: 0,
            confirmation_digest: String::new(),
            counts: ImportSessionCounts {
                all: record.item_ids.len() as u64,
                ..ImportSessionCounts::default()
            },
            selection: ImportSelectionSummary::default(),
            index_state: ImportSessionIndexState::RebuildRequired,
        })
    }

    pub fn validate_selection_snapshot(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        expected_revision: u64,
        expected_digest: &str,
    ) -> Result<ImportSessionOverview, BackendError> {
        let overview = self.read_overview(context, file_store, session_id)?;
        if overview.selection_revision != expected_revision
            || overview.confirmation_digest != expected_digest
        {
            return Err(BackendError::new(
                IMPORT_V2_SELECTION_STALE,
                "The import selection changed before confirmation.",
                true,
                false,
            ));
        }
        Ok(overview)
    }

    pub fn list_items(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        filter: ImportItemPageFilter,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<ImportItemPage, BackendError> {
        validate_id(session_id)?;
        if limit == 0 || limit > MAX_SESSION_ITEM_PAGE_SIZE {
            return Err(BackendError::new(
                IMPORT_V2_SESSION_CURSOR_INVALID,
                "Import item page limit must be between 1 and 200.",
                false,
                true,
            ));
        }
        let root = session_root(context, session_id)?;
        let control_path = format!("{root}/state.json");
        let usable_control = file_store
            .exists(context, &control_path)
            .then(|| file_store.read_json::<SessionControlRecord>(context, &control_path))
            .transpose()
            .ok()
            .flatten()
            .filter(|control| {
                control.schema_version == SESSION_CONTROL_SCHEMA_VERSION
                    && control.session_id == session_id
                    && control.project_id == context.project_id
            });
        let (snapshot_revision, item_count, total, indexed) = if let Some(control) = usable_control
        {
            (
                control.semantic_revision,
                control.item_count,
                filter_total(&control, &filter),
                true,
            )
        } else {
            let record: SessionRecord =
                file_store.read_json(context, &format!("{root}/session.json"))?;
            if record.session_id != session_id || record.project_id != context.project_id {
                return Err(invalid_session("Import session metadata is invalid."));
            }
            (
                0,
                record.item_ids.len() as u64,
                record.item_ids.len() as u64,
                false,
            )
        };

        let start = if let Some(value) = cursor {
            let parsed = serde_json::from_str::<SessionItemCursor>(value).map_err(|_| {
                BackendError::new(
                    IMPORT_V2_SESSION_CURSOR_INVALID,
                    "Import item cursor is invalid.",
                    false,
                    true,
                )
            })?;
            if parsed.version != 1 || parsed.session_id != session_id || parsed.filter != filter {
                return Err(BackendError::new(
                    IMPORT_V2_SESSION_CURSOR_INVALID,
                    "Import item cursor does not match this session and filter.",
                    false,
                    true,
                ));
            }
            if parsed.snapshot_revision != snapshot_revision {
                return Err(BackendError::new(
                    IMPORT_V2_SESSION_CURSOR_STALE,
                    "Import session changed while this page was being read.",
                    true,
                    false,
                ));
            }
            parsed.after
        } else {
            0
        };
        if start > item_count {
            return Err(BackendError::new(
                IMPORT_V2_SESSION_CURSOR_INVALID,
                "Import item cursor is outside this session.",
                false,
                true,
            ));
        }

        let scan_end = (start + u64::from(limit)).min(item_count);
        let item_ids = if indexed {
            self.read_order_range(context, file_store, session_id, start, scan_end)?
        } else {
            let record: SessionRecord =
                file_store.read_json(context, &format!("{root}/session.json"))?;
            record.item_ids[start as usize..scan_end as usize].to_vec()
        };
        let mut items = Vec::with_capacity(item_ids.len());
        for item_id in item_ids {
            let item = self.load_item(context, file_store, session_id, &item_id)?;
            if item_matches_filter(&item, &filter) {
                items.push(item);
            }
        }
        let next_cursor = (scan_end < item_count)
            .then(|| {
                serde_json::to_string(&SessionItemCursor {
                    version: 1,
                    session_id: session_id.to_string(),
                    filter,
                    snapshot_revision,
                    after: scan_end,
                })
                .map_err(|_| invalid_session("Import item cursor could not be serialized."))
            })
            .transpose()?;
        Ok(ImportItemPage {
            session_id: session_id.to_string(),
            snapshot_revision,
            items,
            next_cursor,
            total,
        })
    }

    fn read_order_range(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        start: u64,
        end: u64,
    ) -> Result<Vec<String>, BackendError> {
        let root = session_root(context, session_id)?;
        let mut ids = Vec::with_capacity((end - start) as usize);
        let mut position = start as usize;
        while position < end as usize {
            let page_index = position / ORDER_PAGE_SIZE;
            let page: SessionOrderPage =
                file_store.read_json(context, &format!("{root}/order/{page_index:06}.json"))?;
            if page.schema_version != ORDER_PAGE_SCHEMA_VERSION
                || page.session_id != session_id
                || page.page_index != page_index as u64
                || page.item_ids.len() > ORDER_PAGE_SIZE
            {
                return Err(invalid_session("Import session order page is invalid."));
            }
            let offset = position % ORDER_PAGE_SIZE;
            let take = (end as usize - position).min(page.item_ids.len().saturating_sub(offset));
            if take == 0 {
                return Err(invalid_session("Import session order page is incomplete."));
            }
            ids.extend_from_slice(&page.item_ids[offset..offset + take]);
            position += take;
        }
        Ok(ids)
    }

    pub(crate) fn ensure_accepts_new_items(session: &ImportSession) -> Result<(), BackendError> {
        if matches!(
            session.status,
            ImportSessionStatus::Completed | ImportSessionStatus::Cancelled
        ) {
            return Err(BackendError::new(
                IMPORT_V2_STATE_INVALID,
                "This import session has ended. Start a new import session before adding sources.",
                false,
                false,
            ));
        }
        Ok(())
    }

    pub(super) fn serialized_writes(
        &self,
        context: &ProjectContext,
        session: &ImportSession,
    ) -> Result<Vec<(String, Vec<u8>)>, BackendError> {
        validate_id(&session.session_id)?;
        let root = session_root(context, &session.session_id)?;
        let mut writes = Vec::with_capacity(session.items.len() + 1);
        for item in &session.items {
            validate_id(&item.item_id)?;
            writes.push((
                format!("{root}/items/{}.json", item.item_id),
                serde_json::to_vec_pretty(item)
                    .map_err(|_| invalid_session("Import session item could not be serialized."))?,
            ));
        }
        writes.push((
            format!("{root}/session.json"),
            serde_json::to_vec_pretty(&SessionRecord::from(session))
                .map_err(|_| invalid_session("Import session summary could not be serialized."))?,
        ));
        Ok(writes)
    }

    pub fn create(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        mode: ImportResourceMode,
    ) -> Result<ImportSession, BackendError> {
        let id = uuid::Uuid::new_v4().to_string();
        let session = ImportSession::new(&id, &context.project_id, mode);
        self.save(context, file_store, &session)?;
        Ok(session)
    }

    pub fn load(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
    ) -> Result<ImportSession, BackendError> {
        validate_id(session_id)?;
        let root = session_root(context, session_id)?;
        let summary_path = format!("{root}/session.json");
        if !file_store.exists(context, &summary_path) {
            return Err(BackendError::new(
                IMPORT_V2_SESSION_NOT_FOUND,
                "Import session was not found.",
                true,
                false,
            ));
        }
        let record: SessionRecord = file_store.read_json(context, &summary_path)?;
        if record.schema_version != IMPORT_V2_SCHEMA_VERSION
            || record.session_id != session_id
            || record.project_id != context.project_id
        {
            return Err(invalid_session(
                "Import session metadata does not match the current project.",
            ));
        }

        let mut seen = HashSet::new();
        let mut items = Vec::with_capacity(record.item_ids.len());
        for item_id in &record.item_ids {
            validate_id(item_id)?;
            if !seen.insert(item_id.to_ascii_lowercase()) {
                return Err(invalid_session(
                    "Import session contains duplicate item identifiers.",
                ));
            }
            let item_path = format!("{root}/items/{item_id}.json");
            if !file_store.exists(context, &item_path) {
                return Err(invalid_session("Import session item data is missing."));
            }
            let item: ImportItem = file_store.read_json(context, &item_path)?;
            if item.item_id != *item_id {
                return Err(invalid_session("Import session item metadata is invalid."));
            }
            items.push(item);
        }

        Ok(ImportSession {
            schema_version: record.schema_version,
            session_id: record.session_id,
            project_id: record.project_id,
            status: record.status,
            resource_mode: record.resource_mode,
            created_at: record.created_at,
            updated_at: record.updated_at,
            discovery_task_id: record.discovery_task_id,
            media_authorizations: record.media_authorizations,
            collection_relations: record.collection_relations,
            items,
        })
    }

    pub fn save(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session: &ImportSession,
    ) -> Result<(), BackendError> {
        validate_id(&session.session_id)?;
        if session.schema_version != IMPORT_V2_SCHEMA_VERSION {
            return Err(invalid_session(
                "Import session schema version is unsupported.",
            ));
        }
        if session.project_id != context.project_id {
            return Err(invalid_session(
                "Import session belongs to another project.",
            ));
        }
        let root = session_root(context, &session.session_id)?;
        let mut seen = HashSet::new();
        for item in &session.items {
            validate_id(&item.item_id)?;
            if !seen.insert(item.item_id.to_ascii_lowercase()) {
                return Err(invalid_session(
                    "Import session contains duplicate item identifiers.",
                ));
            }
        }

        file_store.ensure_dir(context, &format!("{root}/items"))?;
        for item in &session.items {
            file_store.write_json_atomic(
                context,
                &format!("{root}/items/{}.json", item.item_id),
                item,
            )?;
        }
        file_store.write_json_atomic(
            context,
            &format!("{root}/session.json"),
            &SessionRecord::from(session),
        )?;
        self.rebuild_sidecars(context, file_store, session)
    }

    pub fn add_inputs(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        inputs: Vec<ImportInput>,
    ) -> Result<ImportSession, BackendError> {
        let mut session = self.load(context, file_store, session_id)?;
        Self::ensure_accepts_new_items(&session)?;
        let inputs = inputs
            .into_iter()
            .map(public_import_input)
            .collect::<Result<Vec<_>, _>>()?;
        let new_items = inputs
            .into_iter()
            .map(|input| ImportItem::queued(&uuid::Uuid::new_v4().to_string(), input))
            .collect::<Vec<_>>();
        session.items.extend(new_items.clone());
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.add_items(context, file_store, &session, &new_items)?;
        Ok(session)
    }

    pub fn add_collection_inputs(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        inputs: Vec<CollectionImportInput>,
        source_url: String,
        platform: String,
        title: String,
    ) -> Result<ImportSession, BackendError> {
        let mut session = self.load(context, file_store, session_id)?;
        Self::ensure_accepts_new_items(&session)?;
        let mut known_urls = session
            .items
            .iter()
            .filter_map(|item| item.input.normalized_locator.clone())
            .collect::<HashSet<_>>();
        let known_collection_fingerprints = session
            .collection_relations
            .iter()
            .filter(|relation| relation.source_url == source_url && relation.platform == platform)
            .flat_map(|relation| relation.children.iter())
            .map(|child| {
                (
                    child.canonical_url.clone(),
                    child.discovery_fingerprint.clone(),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut seen_collection_pairs = known_collection_fingerprints
            .iter()
            .map(|(url, fingerprint)| (url.clone(), fingerprint.clone()))
            .collect::<HashSet<_>>();
        let mut child_item_ids = Vec::new();
        let mut children = Vec::new();
        let existing_item_count = session.items.len();
        for collection_input in inputs {
            let input = public_import_input(collection_input.input)?;
            if let Some(url) = input.normalized_locator.as_ref() {
                if !seen_collection_pairs
                    .insert((url.clone(), collection_input.discovery_fingerprint.clone()))
                {
                    continue;
                }
                let changed_collection_child = known_collection_fingerprints.contains_key(url);
                if !known_urls.insert(url.clone()) && !changed_collection_child {
                    continue;
                }
            }
            let item_id = uuid::Uuid::new_v4().to_string();
            child_item_ids.push(item_id.clone());
            if let Some(canonical_url) = input.normalized_locator.clone() {
                children.push(ImportCollectionChildRelation {
                    item_id: item_id.clone(),
                    canonical_url,
                    discovery_fingerprint: collection_input.discovery_fingerprint,
                });
            }
            session.items.push(ImportItem::queued(&item_id, input));
        }
        if child_item_ids.is_empty() {
            return Ok(session);
        }
        let now = chrono::Utc::now().to_rfc3339();
        if let Some(relation) = session
            .collection_relations
            .iter_mut()
            .find(|relation| relation.source_url == source_url && relation.platform == platform)
        {
            relation.child_item_ids.extend(child_item_ids);
            relation.children.extend(children);
            relation.title = title;
            relation.added_at = now.clone();
        } else {
            session.collection_relations.push(ImportCollectionRelation {
                relation_id: uuid::Uuid::new_v4().to_string(),
                source_url,
                platform,
                title,
                child_item_ids,
                children,
                added_at: now.clone(),
            });
        }
        session.updated_at = now;
        let new_items = session.items[existing_item_count..].to_vec();
        self.add_items(context, file_store, &session, &new_items)?;
        Ok(session)
    }

    pub fn completed_collection_fingerprints(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        source_url: &str,
        platform: &str,
    ) -> HashMap<String, String> {
        let mut newest_fingerprints: HashMap<String, (String, String)> = HashMap::new();
        let Some(import_root) = context.layout.import_state_root.as_deref() else {
            return HashMap::new();
        };
        let Ok(sessions_root) = context.resolve_project_path(import_root) else {
            return HashMap::new();
        };
        let Ok(entries) = std::fs::read_dir(sessions_root) else {
            return HashMap::new();
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let session_id = if path.is_dir() {
                path.file_name().and_then(|value| value.to_str())
            } else if path.extension().and_then(|value| value.to_str()) == Some("json") {
                path.file_stem().and_then(|value| value.to_str())
            } else {
                None
            };
            let Some(session_id) = session_id else {
                continue;
            };
            let Ok(session) = self.load(context, file_store, session_id) else {
                continue;
            };
            let completed = session
                .items
                .iter()
                .filter(|item| item.status == ImportItemStatus::Completed)
                .map(|item| item.item_id.as_str())
                .collect::<HashSet<_>>();
            for relation in session.collection_relations.iter().filter(|relation| {
                relation.source_url == source_url && relation.platform == platform
            }) {
                for child in &relation.children {
                    if completed.contains(child.item_id.as_str()) {
                        let replace = newest_fingerprints
                            .get(&child.canonical_url)
                            .is_none_or(|(updated_at, _)| updated_at <= &session.updated_at);
                        if replace {
                            newest_fingerprints.insert(
                                child.canonical_url.clone(),
                                (
                                    session.updated_at.clone(),
                                    child.discovery_fingerprint.clone(),
                                ),
                            );
                        }
                    }
                }
            }
        }
        newest_fingerprints
            .into_iter()
            .map(|(url, (_, fingerprint))| (url, fingerprint))
            .collect()
    }

    pub fn update_item(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        item: ImportItem,
    ) -> Result<ImportSession, BackendError> {
        validate_id(&item.item_id)?;
        let existing = self.load_item(context, file_store, session_id, &item.item_id)?;
        if existing.input != item.input
            || (existing.status != item.status && !existing.status.can_transition_to(&item.status))
        {
            return Err(BackendError::new(
                IMPORT_V2_STATE_INVALID,
                "Import item update violates its identity or state transition.",
                false,
                true,
            ));
        }
        self.write_item(context, file_store, session_id, &item)?;
        self.load(context, file_store, session_id)
    }

    pub fn load_item(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        validate_id(session_id)?;
        validate_id(item_id)?;
        let root = session_root(context, session_id)?;
        let path = format!("{root}/items/{item_id}.json");
        if !file_store.exists(context, &path) {
            return Err(BackendError::new(
                IMPORT_V2_ITEM_NOT_FOUND,
                "Import session item was not found.",
                true,
                false,
            ));
        }
        let item: ImportItem = file_store.read_json(context, &path)?;
        if item.item_id != item_id {
            return Err(invalid_session("Import session item metadata is invalid."));
        }
        Ok(item)
    }

    pub fn write_item(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        item: &ImportItem,
    ) -> Result<(), BackendError> {
        validate_id(session_id)?;
        validate_id(&item.item_id)?;
        let root = session_root(context, session_id)?;
        let control_path = format!("{root}/state.json");
        if !file_store.exists(context, &control_path) {
            let session = self.load(context, file_store, session_id)?;
            self.rebuild_sidecars(context, file_store, &session)?;
        }
        let before = self.load_item(context, file_store, session_id, &item.item_id)?;
        let mut after = item.clone();
        after.item_revision = after
            .item_revision
            .max(before.item_revision.saturating_add(1));
        let before_control = self.read_control(context, file_store, session_id)?;
        let mut after_control = before_control.clone();
        after_control.semantic_revision = before_control.semantic_revision.saturating_add(1);
        after_control.updated_at = chrono::Utc::now().to_rfc3339();
        let before_projection = item_projection(&before);
        let after_projection = item_projection(&after);
        apply_projection_delta(
            &mut after_control.counts,
            &mut after_control.selection,
            before_projection,
            after_projection,
        );
        apply_status_count(&mut after_control.status_counts, &before.status, 0, 1);
        apply_status_count(&mut after_control.status_counts, &after.status, 1, 0);
        if selection_projection_changed(before_projection, after_projection) {
            after_control.selection_revision = before_control.selection_revision.saturating_add(1);
        }
        after_control.status = status_from_control(&after_control);
        after_control.confirmation_digest = confirmation_digest(
            session_id,
            after_control.selection_revision,
            &after_control.selection,
        );
        let pointer_path = active_session_path(context)?;
        let before_pointer: ActiveSessionPointer = file_store.read_json(context, &pointer_path)?;
        if before_pointer.session_id != session_id {
            return Err(invalid_session(
                "Active import session pointer does not match the item update.",
            ));
        }
        let after_pointer = pointer_from_control(&after_control)?;
        let item_path =
            context.resolve_project_path(&format!("{root}/items/{}.json", after.item_id))?;
        let state_path = context.resolve_project_path(&control_path)?;
        let pointer_absolute = context.resolve_project_path(&pointer_path)?;
        let before_item_bytes = pretty_bytes(&before, "Import session item")?;
        let after_item_bytes = pretty_bytes(&after, "Import session item")?;
        let before_control_bytes = pretty_bytes(&before_control, "Import session control record")?;
        let after_control_bytes = pretty_bytes(&after_control, "Import session control record")?;
        let before_pointer_bytes = pretty_bytes(&before_pointer, "Active import session pointer")?;
        let after_pointer_bytes = pretty_bytes(&after_pointer, "Active import session pointer")?;
        let writes = vec![
            (
                item_path,
                after_item_bytes,
                format!("{:x}", Sha256::digest(before_item_bytes)),
            ),
            (
                state_path,
                after_control_bytes,
                format!("{:x}", Sha256::digest(before_control_bytes)),
            ),
            (
                pointer_absolute,
                after_pointer_bytes,
                format!("{:x}", Sha256::digest(before_pointer_bytes)),
            ),
        ];
        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.write_many_if_hash_matches(&writes)?;
        transaction.commit()?;
        for (path, bytes, _) in &writes {
            file_store.observe_atomic_write(path, bytes.len());
        }
        Ok(())
    }

    pub fn write_items(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        items: &[ImportItem],
    ) -> Result<(), BackendError> {
        for item in items {
            let root = session_root(context, session_id)?;
            let path = format!("{root}/items/{}.json", item.item_id);
            if file_store.exists(context, &path) {
                self.write_item(context, file_store, session_id, item)?;
            } else {
                file_store.write_json_atomic(context, &path, item)?;
            }
        }
        Ok(())
    }

    /// Install a cohort as one recoverable compare-and-swap transaction. The
    /// expected hashes are captured from the caller's snapshot, so late
    /// external edits fail closed instead of being overwritten.
    pub(crate) fn write_item_cohort_if_unchanged(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        originals: &[ImportItem],
        replacements: &[ImportItem],
    ) -> Result<(), BackendError> {
        self.write_item_cohort_if_unchanged_with_cancel(
            context,
            file_store,
            session_id,
            originals,
            replacements,
            || false,
        )
    }

    pub(crate) fn write_item_cohort_if_unchanged_with_cancel<F>(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        originals: &[ImportItem],
        replacements: &[ImportItem],
        mut should_cancel: F,
    ) -> Result<(), BackendError>
    where
        F: FnMut() -> bool,
    {
        if originals.len() != replacements.len() {
            return Err(invalid_session(
                "Import item cohort does not match its snapshot.",
            ));
        }
        let root = session_root(context, session_id)?;
        let mut revised_items = Vec::with_capacity(replacements.len());
        let mut writes = Vec::with_capacity(originals.len() + 2);
        for (before, after) in originals.iter().zip(replacements) {
            if should_cancel() {
                return Err(BackendError::new(
                    crate::errors::IMPORT_V2_CANCELLED,
                    "Import was cancelled.",
                    true,
                    false,
                ));
            }
            if before.item_id != after.item_id {
                return Err(invalid_session("Import item cohort identity changed."));
            }
            let expected = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec_pretty(before).map_err(|_| {
                    invalid_session("Import session item could not be serialized.")
                })?)
            );
            let mut revised = after.clone();
            revised.item_revision = revised
                .item_revision
                .max(before.item_revision.saturating_add(1));
            let desired = serde_json::to_vec_pretty(&revised)
                .map_err(|_| invalid_session("Import session item could not be serialized."))?;
            writes.push((
                context.resolve_project_path(&format!("{root}/items/{}.json", after.item_id))?,
                desired,
                expected,
            ));
            revised_items.push(revised);
        }
        let changes = originals.iter().zip(&revised_items).collect::<Vec<_>>();
        writes.extend(
            self.control_writes_for_item_changes(context, file_store, session_id, &changes)?,
        );
        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.write_many_if_hash_matches_with_cancel(&writes, should_cancel)?;
        transaction.commit()?;
        for (path, bytes, _) in &writes {
            file_store.observe_atomic_write(path, bytes.len());
        }
        Ok(())
    }

    /// Publish recovery changes as one compare-and-swap transaction. Item
    /// files and the coarse session record become visible together, while a
    /// concurrent external edit fails closed.
    pub(crate) fn write_recovery_cohort_if_unchanged_with_cancel<F>(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        before_session: &ImportSession,
        after_session: &ImportSession,
        originals: &[ImportItem],
        replacements: &[ImportItem],
        mut should_cancel: F,
    ) -> Result<(), BackendError>
    where
        F: FnMut() -> bool,
    {
        if before_session.session_id != after_session.session_id
            || originals.len() != replacements.len()
        {
            return Err(invalid_session(
                "Import recovery cohort does not match its snapshot.",
            ));
        }
        let root = session_root(context, &after_session.session_id)?;
        let mut revised_items = Vec::with_capacity(replacements.len());
        let mut writes = Vec::with_capacity(replacements.len() + 3);
        for (before, after) in originals.iter().zip(replacements) {
            if should_cancel() {
                return Err(BackendError::new(
                    crate::errors::IMPORT_V2_CANCELLED,
                    "Import recovery was cancelled.",
                    true,
                    false,
                ));
            }
            if before.item_id != after.item_id {
                return Err(invalid_session("Import recovery item identity changed."));
            }
            let expected_bytes = serde_json::to_vec_pretty(before)
                .map_err(|_| invalid_session("Import session item could not be serialized."))?;
            let mut revised = after.clone();
            revised.item_revision = revised
                .item_revision
                .max(before.item_revision.saturating_add(1));
            let desired = serde_json::to_vec_pretty(&revised)
                .map_err(|_| invalid_session("Import session item could not be serialized."))?;
            writes.push((
                context.resolve_project_path(&format!("{root}/items/{}.json", after.item_id))?,
                desired,
                format!("{:x}", Sha256::digest(expected_bytes)),
            ));
            revised_items.push(revised);
        }

        let before_record = serde_json::to_vec_pretty(&SessionRecord::from(before_session))
            .map_err(|_| invalid_session("Import session record could not be serialized."))?;
        let after_record = serde_json::to_vec_pretty(&SessionRecord::from(after_session))
            .map_err(|_| invalid_session("Import session record could not be serialized."))?;
        writes.push((
            context.resolve_project_path(&format!("{root}/session.json"))?,
            after_record,
            format!("{:x}", Sha256::digest(before_record)),
        ));
        let changes = originals.iter().zip(&revised_items).collect::<Vec<_>>();
        writes.extend(self.control_writes_for_item_changes(
            context,
            file_store,
            &after_session.session_id,
            &changes,
        )?);

        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.write_many_if_hash_matches_with_cancel(&writes, should_cancel)?;
        transaction.commit()?;
        for (path, bytes, _) in &writes {
            file_store.observe_atomic_write(path, bytes.len());
        }
        Ok(())
    }

    pub fn write_session_record(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session: &ImportSession,
    ) -> Result<(), BackendError> {
        let root = session_root(context, &session.session_id)?;
        let control_path = format!("{root}/state.json");
        let pointer_path = active_session_path(context)?;
        if !file_store.exists(context, &control_path) || !file_store.exists(context, &pointer_path)
        {
            file_store.write_json_atomic(
                context,
                &format!("{root}/session.json"),
                &SessionRecord::from(session),
            )?;
            return self.rebuild_sidecars(context, file_store, session);
        }

        let before_control = self.read_control(context, file_store, &session.session_id)?;
        let mut after_control =
            control_from_session(session, before_control.semantic_revision.saturating_add(1));
        if after_control.selection == before_control.selection {
            after_control.selection_revision = before_control.selection_revision;
        } else {
            after_control.selection_revision = before_control.selection_revision.saturating_add(1);
        }
        after_control.confirmation_digest = confirmation_digest(
            &session.session_id,
            after_control.selection_revision,
            &after_control.selection,
        );
        let before_pointer: ActiveSessionPointer = file_store.read_json(context, &pointer_path)?;
        let expected_pointer = pointer_from_control(&before_control)?;
        if before_pointer.session_id != session.session_id
            || before_pointer.control_revision != expected_pointer.control_revision
            || before_pointer.summary_hash != expected_pointer.summary_hash
            || before_pointer.status != expected_pointer.status
        {
            return Err(invalid_session(
                "Active import session pointer does not match the session update.",
            ));
        }
        let before_record: SessionRecord =
            file_store.read_json(context, &format!("{root}/session.json"))?;
        file_store.ensure_dir(context, &format!("{root}/order"))?;
        let mut transaction = FileTransaction::new_for_project(&context.root);
        let mut observed = Vec::new();

        let mut stage_existing =
            |relative: &str, before: Vec<u8>, after: Vec<u8>| -> Result<(), BackendError> {
                let path = context.resolve_project_path(relative)?;
                transaction.write_if_hash_matches(
                    &path,
                    &after,
                    &format!("{:x}", Sha256::digest(before)),
                )?;
                observed.push((path, after.len()));
                Ok(())
            };
        stage_existing(
            &format!("{root}/session.json"),
            pretty_bytes(&before_record, "Import session record")?,
            pretty_bytes(&SessionRecord::from(session), "Import session record")?,
        )?;
        stage_existing(
            &control_path,
            pretty_bytes(&before_control, "Import session control record")?,
            pretty_bytes(&after_control, "Import session control record")?,
        )?;
        stage_existing(
            &pointer_path,
            pretty_bytes(&before_pointer, "Active import session pointer")?,
            pretty_bytes(
                &pointer_from_control(&after_control)?,
                "Active import session pointer",
            )?,
        )?;
        drop(stage_existing);

        for (page_index, items) in session.items.chunks(ORDER_PAGE_SIZE).enumerate() {
            let relative = format!("{root}/order/{page_index:06}.json");
            let page = SessionOrderPage {
                schema_version: ORDER_PAGE_SCHEMA_VERSION,
                session_id: session.session_id.clone(),
                page_index: page_index as u64,
                item_ids: items.iter().map(|item| item.item_id.clone()).collect(),
            };
            let bytes = pretty_bytes(&page, "Import session order page")?;
            let path = context.resolve_project_path(&relative)?;
            if file_store.exists(context, &relative) {
                let before: SessionOrderPage = file_store.read_json(context, &relative)?;
                transaction.write_if_hash_matches(
                    &path,
                    &bytes,
                    &format!(
                        "{:x}",
                        Sha256::digest(pretty_bytes(&before, "Import session order page")?)
                    ),
                )?;
            } else {
                transaction.write_new(&path, &bytes)?;
            }
            observed.push((path, bytes.len()));
        }
        transaction.commit()?;
        for (path, bytes) in observed {
            file_store.observe_atomic_write(&path, bytes);
        }
        Ok(())
    }

    pub fn add_items(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session: &ImportSession,
        new_items: &[ImportItem],
    ) -> Result<(), BackendError> {
        let root = session_root(context, &session.session_id)?;
        file_store.ensure_dir(context, &format!("{root}/items"))?;
        self.write_items(context, file_store, &session.session_id, new_items)?;
        self.write_session_record(context, file_store, session)
    }
}

fn pretty_bytes<T: Serialize>(value: &T, label: &str) -> Result<Vec<u8>, BackendError> {
    serde_json::to_vec_pretty(value)
        .map_err(|_| invalid_session(&format!("{label} could not be serialized.")))
}

fn apply_counter(value: &mut u64, before: u64, after: u64) {
    *value = value.saturating_sub(before).saturating_add(after);
}

fn apply_projection_delta(
    counts: &mut ImportSessionCounts,
    selection: &mut ImportSelectionSummary,
    before: ItemProjection,
    after: ItemProjection,
) {
    apply_counter(&mut counts.all, before.all, after.all);
    apply_counter(&mut counts.active, before.active, after.active);
    apply_counter(&mut counts.ready, before.ready, after.ready);
    apply_counter(
        &mut counts.needs_action,
        before.needs_action,
        after.needs_action,
    );
    apply_counter(&mut counts.failed, before.failed, after.failed);
    apply_counter(&mut counts.completed, before.completed, after.completed);
    apply_counter(&mut counts.waiting, before.waiting, after.waiting);
    apply_counter(&mut counts.processed, before.processed, after.processed);
    apply_counter(&mut counts.cancelled, before.cancelled, after.cancelled);
    apply_counter(&mut selection.selected, before.selected, after.selected);
    apply_counter(
        &mut selection.new_sources,
        before.new_sources,
        after.new_sources,
    );
    apply_counter(&mut selection.updates, before.updates, after.updates);
    apply_counter(&mut selection.warnings, before.warnings, after.warnings);
    apply_counter(&mut selection.pending, before.pending, after.pending);
    apply_counter(
        &mut selection.restricted,
        before.restricted,
        after.restricted,
    );
}

fn selection_projection_changed(before: ItemProjection, after: ItemProjection) -> bool {
    before.selected != after.selected
        || before.new_sources != after.new_sources
        || before.updates != after.updates
        || before.warnings != after.warnings
        || before.pending != after.pending
        || before.restricted != after.restricted
}

fn public_import_input(mut input: ImportInput) -> Result<ImportInput, BackendError> {
    if input.kind != crate::models::import_v2::ImportInputKind::Url {
        return Ok(input);
    }
    if let Some(suffix) = input.locator.strip_prefix("import-web-target:") {
        uuid::Uuid::parse_str(suffix)
            .map_err(|_| invalid_session("Secure import URL reference is invalid."))?;
        let reference = input.locator.clone();
        input.locator = input.normalized_locator.clone().ok_or_else(|| {
            invalid_session("Secure import URL reference requires a public locator.")
        })?;
        let mut sanitized = public_import_input(input)?;
        sanitized.locator = reference;
        return Ok(sanitized);
    }
    let mut locator =
        url::Url::parse(&input.locator).map_err(|_| invalid_session("Import URL is invalid."))?;
    if !matches!(locator.scheme(), "http" | "https") || locator.host_str().is_none() {
        return Err(invalid_session(
            "Import URL must be a public HTTP or HTTPS locator.",
        ));
    }
    locator
        .set_username("")
        .map_err(|_| invalid_session("Import URL credentials could not be removed."))?;
    locator
        .set_password(None)
        .map_err(|_| invalid_session("Import URL credentials could not be removed."))?;
    locator.set_fragment(None);
    let public_query: Vec<(String, String)> = locator
        .query_pairs()
        .filter(|(key, _)| !sensitive_query_key(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    locator.set_query(None);
    if !public_query.is_empty() {
        locator
            .query_pairs_mut()
            .extend_pairs(public_query.iter().map(|(key, value)| (key, value)));
    }
    let public = locator.to_string();
    input.locator = public.clone();
    input.normalized_locator = Some(public);
    Ok(input)
}

fn sensitive_query_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace('-', "_");
    key == "sig"
        || key.contains("token")
        || key.contains("signature")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("credential")
        || key.contains("api_key")
        || key.contains("authorization")
}

#[cfg(test)]
mod tests {
    use crate::errors::{IMPORT_V2_SESSION_INVALID, IMPORT_V2_STATE_INVALID};
    use crate::models::import_v2::{
        ImportInput, ImportInputKind, ImportResourceMode, ImportSessionStatus, MediaSaveMode,
    };
    use crate::services::import_v2::test_support::{test_context, test_file_input};
    use crate::services::FileStore;

    use super::{CollectionImportInput, SessionStore};

    fn collection_input(url: &str) -> CollectionImportInput {
        CollectionImportInput {
            input: ImportInput {
                kind: ImportInputKind::Url,
                display_name: "合集子项".into(),
                locator: url.into(),
                normalized_locator: Some(url.into()),
                source_identity: None,
                media_save_mode: MediaSaveMode::ExtractOnly,
            },
            discovery_fingerprint: format!("fingerprint:{url}"),
        }
    }

    #[test]
    fn completed_session_rejects_new_items() {
        let (context, root) = test_context("completed-session-add");
        let files = FileStore::default();
        let store = SessionStore::default();
        let mut session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        session.status = ImportSessionStatus::Completed;
        store.save(&context, &files, &session).unwrap();

        let error = store
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![test_file_input("new.pdf")],
            )
            .unwrap_err();

        assert_eq!(error.code, IMPORT_V2_STATE_INVALID);
        assert!(store
            .load(&context, &files, &session.session_id)
            .unwrap()
            .items
            .is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn url_credentials_sensitive_query_and_fragment_never_reach_session_files() {
        let (context, root) = test_context("session-url-secrets");
        let files = FileStore::default();
        let store = SessionStore::default();
        let session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let secret = "must-not-persist";
        let session = store
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![crate::models::import_v2::ImportInput {
                    source_identity: None,
                    kind: crate::models::import_v2::ImportInputKind::Url,
                    display_name: "公开页面".into(),
                    locator: format!("https://user:{secret}@例子.测试/文档?lang=zh&token={secret}&signature={secret}#access_token={secret}"),
                    normalized_locator: None,
                    media_save_mode: Default::default(),
                }],
            )
            .unwrap();

        assert_eq!(
            session.items[0].input.locator,
            "https://xn--fsqu00a.xn--0zwm56d/%E6%96%87%E6%A1%A3?lang=zh"
        );
        assert_eq!(
            session.items[0].input.normalized_locator.as_deref(),
            Some("https://xn--fsqu00a.xn--0zwm56d/%E6%96%87%E6%A1%A3?lang=zh")
        );
        let session_root = root.join(".app/import-sessions").join(&session.session_id);
        for path in [
            session_root.join("session.json"),
            session_root
                .join("items")
                .join(format!("{}.json", session.items[0].item_id)),
        ] {
            let persisted = std::fs::read_to_string(&path).unwrap();
            assert!(
                !persisted.contains(secret),
                "secret leaked into {}",
                path.display()
            );
            assert!(!persisted.contains("access_token"));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn session_round_trip_restores_items_after_new_store_instance() {
        let (context, root) = test_context("session-round-trip");
        let files = FileStore::default();
        let store = SessionStore::default();
        let session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        store
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![test_file_input("研究报告.pdf")],
            )
            .unwrap();

        let reopened = SessionStore::default()
            .load(&context, &files, &session.session_id)
            .unwrap();
        assert_eq!(reopened.items.len(), 1);
        assert_eq!(reopened.items[0].input.display_name, "研究报告.pdf");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collection_relation_round_trips_and_reimport_only_adds_new_children() {
        let (context, root) = test_context("session-collection-relation");
        let files = FileStore::default();
        let store = SessionStore::default();
        let session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = store
            .add_collection_inputs(
                &context,
                &files,
                &session.session_id,
                vec![
                    collection_input("https://www.bilibili.com/video/BV1first"),
                    collection_input("https://www.bilibili.com/video/BV2second"),
                ],
                "https://www.bilibili.com/medialist/play/42".into(),
                "bilibili".into(),
                "课程合集".into(),
            )
            .unwrap();
        let session = store
            .add_collection_inputs(
                &context,
                &files,
                &session.session_id,
                vec![
                    collection_input("https://www.bilibili.com/video/BV2second"),
                    collection_input("https://www.bilibili.com/video/BV3third"),
                ],
                "https://www.bilibili.com/medialist/play/42".into(),
                "bilibili".into(),
                "课程合集（更新）".into(),
            )
            .unwrap();
        let changed = CollectionImportInput {
            input: collection_input("https://www.bilibili.com/video/BV2second").input,
            discovery_fingerprint: "changed-fingerprint".into(),
        };
        let session = store
            .add_collection_inputs(
                &context,
                &files,
                &session.session_id,
                vec![changed],
                "https://www.bilibili.com/medialist/play/42".into(),
                "bilibili".into(),
                "课程合集（内容变化）".into(),
            )
            .unwrap();

        assert_eq!(session.items.len(), 4);
        assert_eq!(session.collection_relations.len(), 1);
        assert_eq!(
            session.collection_relations[0].child_item_ids.len(),
            session.items.len()
        );
        assert_eq!(
            session.collection_relations[0].title,
            "课程合集（内容变化）"
        );
        let reopened = SessionStore::default()
            .load(&context, &files, &session.session_id)
            .unwrap();
        assert_eq!(reopened.collection_relations, session.collection_relations);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn completed_collection_fingerprints_are_reused_across_sessions() {
        let (context, root) = test_context("collection-cross-session");
        let files = FileStore::default();
        let store = SessionStore::default();
        let source_url = "https://space.bilibili.com/42";
        let session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let mut session = store
            .add_collection_inputs(
                &context,
                &files,
                &session.session_id,
                vec![collection_input(
                    "https://www.bilibili.com/video/BV1completed",
                )],
                source_url.into(),
                "bilibili".into(),
                "作者视频".into(),
            )
            .unwrap();
        session.items[0].status = crate::models::import_v2::ImportItemStatus::Completed;
        store.save(&context, &files, &session).unwrap();

        let fingerprints =
            store.completed_collection_fingerprints(&context, &files, source_url, "bilibili");
        assert_eq!(
            fingerprints
                .get("https://www.bilibili.com/video/BV1completed")
                .map(String::as_str),
            Some("fingerprint:https://www.bilibili.com/video/BV1completed")
        );
        assert_ne!(
            fingerprints
                .get("https://www.bilibili.com/video/BV1completed")
                .map(String::as_str),
            Some("changed-fingerprint")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn session_round_trip_restores_discovery_task_identity() {
        let (context, root) = test_context("session-discovery-task");
        let files = FileStore::default();
        let store = SessionStore::default();
        let mut session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        session.discovery_task_id = Some("scan-task-1".into());
        store.save(&context, &files, &session).unwrap();

        let reopened = SessionStore::default()
            .load(&context, &files, &session.session_id)
            .unwrap();
        assert_eq!(reopened.discovery_task_id.as_deref(), Some("scan-task-1"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn session_id_cannot_escape_import_session_root() {
        let (context, root) = test_context("session-traversal");
        let error = SessionStore::default()
            .load(&context, &FileStore::default(), "../settings")
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_SESSION_INVALID);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_save_validation_preserves_the_persisted_session() {
        let (context, root) = test_context("session-save-validation");
        let files = FileStore::default();
        let store = SessionStore::default();
        let session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let mut session = store
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![test_file_input("original.pdf")],
            )
            .unwrap();
        session.items[0].input.display_name = "mutated.pdf".to_string();
        session
            .items
            .push(crate::models::import_v2::ImportItem::queued(
                "../invalid",
                test_file_input("invalid.pdf"),
            ));

        let error = store.save(&context, &files, &session).unwrap_err();
        assert_eq!(error.code, IMPORT_V2_SESSION_INVALID);
        let reopened = store.load(&context, &files, &session.session_id).unwrap();
        assert_eq!(reopened.items[0].input.display_name, "original.pdf");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unsupported_schema_version_is_rejected_before_persistence() {
        let (context, root) = test_context("session-schema-version");
        let files = FileStore::default();
        let store = SessionStore::default();
        let mut session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        session.schema_version += 1;

        let error = store.save(&context, &files, &session).unwrap_err();
        assert_eq!(error.code, IMPORT_V2_SESSION_INVALID);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn update_item_rejects_illegal_status_transition() {
        let (context, root) = test_context("session-item-transition");
        let files = FileStore::default();
        let store = SessionStore::default();
        let session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = store
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![test_file_input("transition.pdf")],
            )
            .unwrap();
        let mut item = session.items[0].clone();
        item.status = crate::models::import_v2::ImportItemStatus::Completed;

        let error = store
            .update_item(&context, &files, &session.session_id, item)
            .unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_STATE_INVALID);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelled_cohort_claim_rolls_back_every_item() {
        let (context, root) = test_context("session-cohort-cancel");
        let files = FileStore::default();
        let store = SessionStore::default();
        let session = store
            .create(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = store
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![test_file_input("one.pdf"), test_file_input("two.pdf")],
            )
            .unwrap();
        let originals = session.items.clone();
        let replacements = originals
            .iter()
            .cloned()
            .map(|mut item| {
                item.task_id = Some("operation-1".into());
                item
            })
            .collect::<Vec<_>>();
        let mut checks = 0usize;

        let error = store
            .write_item_cohort_if_unchanged_with_cancel(
                &context,
                &files,
                &session.session_id,
                &originals,
                &replacements,
                || {
                    checks += 1;
                    checks >= 4
                },
            )
            .unwrap_err();

        assert_eq!(error.code, crate::errors::IMPORT_V2_CANCELLED);
        let reopened = store.load(&context, &files, &session.session_id).unwrap();
        assert!(reopened.items.iter().all(|item| item.task_id.is_none()));
        std::fs::remove_dir_all(root).unwrap();
    }
}
