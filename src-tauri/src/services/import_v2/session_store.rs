use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{
    BackendError, IMPORT_V2_ITEM_NOT_FOUND, IMPORT_V2_SESSION_INVALID, IMPORT_V2_SESSION_NOT_FOUND,
    IMPORT_V2_STATE_INVALID,
};
use crate::models::import_v2::{
    ImportCollectionChildRelation, ImportCollectionRelation, ImportInput, ImportItem,
    ImportItemStatus, ImportMediaAuthorization, ImportResourceMode, ImportSession,
    ImportSessionOverview, ImportSessionStatus, IMPORT_V2_SCHEMA_VERSION,
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

impl SessionStore {
    pub fn read_overview(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
    ) -> Result<ImportSessionOverview, BackendError> {
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
        })
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
        )
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
        file_store.write_json_atomic(
            context,
            &format!("{root}/items/{}.json", item.item_id),
            item,
        )
    }

    pub fn write_items(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        items: &[ImportItem],
    ) -> Result<(), BackendError> {
        for item in items {
            self.write_item(context, file_store, session_id, item)?;
        }
        Ok(())
    }

    /// Install a cohort as one recoverable compare-and-swap transaction. The
    /// expected hashes are captured from the caller's snapshot, so late
    /// external edits fail closed instead of being overwritten.
    pub(crate) fn write_item_cohort_if_unchanged(
        &self,
        context: &ProjectContext,
        _file_store: &FileStore,
        session_id: &str,
        originals: &[ImportItem],
        replacements: &[ImportItem],
    ) -> Result<(), BackendError> {
        self.write_item_cohort_if_unchanged_with_cancel(
            context,
            _file_store,
            session_id,
            originals,
            replacements,
            || false,
        )
    }

    pub(crate) fn write_item_cohort_if_unchanged_with_cancel<F>(
        &self,
        context: &ProjectContext,
        _file_store: &FileStore,
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
        let mut writes = Vec::with_capacity(originals.len());
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
            let desired = serde_json::to_vec_pretty(after)
                .map_err(|_| invalid_session("Import session item could not be serialized."))?;
            writes.push((
                context.resolve_project_path(&format!("{root}/items/{}.json", after.item_id))?,
                desired,
                expected,
            ));
        }
        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.write_many_if_hash_matches_with_cancel(&writes, should_cancel)?;
        transaction.commit()
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
        let mut writes = Vec::with_capacity(replacements.len() + 1);
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
            let desired = serde_json::to_vec_pretty(after)
                .map_err(|_| invalid_session("Import session item could not be serialized."))?;
            writes.push((
                context.resolve_project_path(&format!("{root}/items/{}.json", after.item_id))?,
                desired,
                format!("{:x}", Sha256::digest(expected_bytes)),
            ));
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
        file_store.write_json_atomic(
            context,
            &format!("{root}/session.json"),
            &SessionRecord::from(session),
        )
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
