use crate::{
    errors::BackendError,
    models::paths::ProjectContext,
    services::{
        import_v2::url_policy::{SessionWebTarget, UrlPolicy},
        FileStore, SecretService,
    },
    utils::safe_project_dir::remove_project_file,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use super::url_policy::PrivateTargetGrant;

const PREFIX: &str = "import-web-target:";
const COLLECTION_PREFIX: &str = "import-web-collection:";
const COLLECTION_PAGE_SIZE: usize = 50;
const COLLECTION_MAX_ITEMS: usize = 5_000;
const DURABLE_COLLECTION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
struct PendingCollection {
    project_id: String,
    session_id: String,
    source_url: String,
    platform: String,
    title: String,
    expires_at: chrono::DateTime<chrono::Utc>,
    items: Vec<PendingCollectionItem>,
    loaded_count: usize,
    next_cursor: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingCollectionItem {
    item_ref: String,
    title: String,
    target: SessionWebTarget,
    discovery_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DurablePendingCollection {
    schema_version: u32,
    task_id: String,
    project_id: String,
    session_id: String,
    source_url: String,
    platform: String,
    title: String,
    expires_at: String,
    items: Vec<DurablePendingCollectionItem>,
    loaded_count: usize,
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DurablePendingCollectionItem {
    item_ref: String,
    title: String,
    public_url: String,
    target_locator: String,
    discovery_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionPreviewTarget {
    pub item_ref: String,
    pub title: String,
    pub public_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionPage {
    pub items: Vec<CollectionPreviewTarget>,
    pub discovered_total: usize,
    pub loaded_count: usize,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CollectionSelectionTarget {
    pub target: SessionWebTarget,
    pub discovery_fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct CollectionSelection {
    pub task_id: Option<String>,
    pub source_url: String,
    pub platform: String,
    pub title: String,
    pub targets: Vec<CollectionSelectionTarget>,
}

#[derive(Clone)]
pub struct WebTargetStore {
    secrets: SecretService,
    private_grants: Arc<Mutex<HashMap<String, PrivateTargetGrant>>>,
    active_private_grants: Arc<Mutex<HashMap<(String, String), PrivateTargetGrant>>>,
    asr_grants: Arc<Mutex<HashMap<(String, String, String), BilibiliAsrGrant>>>,
    asr_reservations: Arc<Mutex<HashSet<(String, String, String)>>>,
    authenticated_profiles: Arc<Mutex<HashMap<(String, String, String), PathBuf>>>,
    pending_collections: Arc<Mutex<HashMap<String, PendingCollection>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BilibiliAsrGrant {
    pub project_id: String,
    pub session_id: String,
    pub item_id: String,
    pub target_sha256: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTarget {
    request_url: String,
    expires_at: String,
}

impl Default for WebTargetStore {
    fn default() -> Self {
        Self {
            secrets: SecretService::default(),
            private_grants: Arc::new(Mutex::new(HashMap::new())),
            active_private_grants: Arc::new(Mutex::new(HashMap::new())),
            asr_grants: Arc::new(Mutex::new(HashMap::new())),
            asr_reservations: Arc::new(Mutex::new(HashSet::new())),
            authenticated_profiles: Arc::new(Mutex::new(HashMap::new())),
            pending_collections: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
impl WebTargetStore {
    pub fn new(secrets: SecretService) -> Self {
        Self {
            secrets,
            private_grants: Arc::new(Mutex::new(HashMap::new())),
            active_private_grants: Arc::new(Mutex::new(HashMap::new())),
            asr_grants: Arc::new(Mutex::new(HashMap::new())),
            asr_reservations: Arc::new(Mutex::new(HashSet::new())),
            authenticated_profiles: Arc::new(Mutex::new(HashMap::new())),
            pending_collections: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn store_collection(
        &self,
        project_id: &str,
        session_id: &str,
        source_url: String,
        platform: String,
        title: String,
        items: Vec<(String, SessionWebTarget, String)>,
    ) -> Result<(String, CollectionPage), BackendError> {
        if items.len() > COLLECTION_MAX_ITEMS {
            return Err(collection_error(
                "A collection discovery exceeded the safe item limit.",
            ));
        }
        let collection_ref = format!("{COLLECTION_PREFIX}{}", uuid::Uuid::new_v4());
        let mut stored_items = Vec::with_capacity(items.len());
        for (title, target, discovery_fingerprint) in items {
            let item_ref = format!("import-web-collection-item:{}", uuid::Uuid::new_v4());
            stored_items.push(PendingCollectionItem {
                item_ref,
                title,
                target,
                discovery_fingerprint,
            });
        }
        let loaded_count = stored_items.len().min(COLLECTION_PAGE_SIZE);
        let next_cursor = (loaded_count < stored_items.len()).then(opaque_collection_cursor);
        let pending = PendingCollection {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            source_url,
            platform,
            title,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(20),
            items: stored_items,
            loaded_count,
            next_cursor,
        };
        let page = collection_page(&pending, 0);
        let mut collections = self.pending_collections.lock().map_err(|_| store_error())?;
        collections.retain(|_, value| value.expires_at > chrono::Utc::now());
        collections.insert(collection_ref.clone(), pending);
        Ok((collection_ref, page))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn store_collection_durable(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        task_id: &str,
        project_id: &str,
        session_id: &str,
        source_url: String,
        platform: String,
        title: String,
        items: Vec<(String, SessionWebTarget, String)>,
    ) -> Result<(String, CollectionPage), BackendError> {
        if items.len() > COLLECTION_MAX_ITEMS {
            return Err(collection_error(
                "A collection discovery exceeded the safe item limit.",
            ));
        }
        let id = uuid::Uuid::new_v4();
        let collection_ref = format!("{COLLECTION_PREFIX}{id}");
        let mut stored_locators = Vec::with_capacity(items.len());
        let mut durable_items = Vec::with_capacity(items.len());
        for (title, target, discovery_fingerprint) in items {
            let target_locator = match self.store(&target) {
                Ok(locator) => locator,
                Err(error) => {
                    self.delete_many(&stored_locators);
                    return Err(error);
                }
            };
            stored_locators.push(target_locator.clone());
            durable_items.push(DurablePendingCollectionItem {
                item_ref: format!("import-web-collection-item:{}", uuid::Uuid::new_v4()),
                title,
                public_url: target.public.public_url,
                target_locator,
                discovery_fingerprint,
            });
        }
        let loaded_count = durable_items.len().min(COLLECTION_PAGE_SIZE);
        let next_cursor = durable_collection_cursor(id, loaded_count, durable_items.len());
        let pending = DurablePendingCollection {
            schema_version: DURABLE_COLLECTION_SCHEMA_VERSION,
            task_id: task_id.to_string(),
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            source_url,
            platform,
            title,
            expires_at: (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339(),
            items: durable_items,
            loaded_count,
            next_cursor,
        };
        let path = durable_collection_path(context, id)?;
        if let Err(error) = files.write_json_atomic(context, &path, &pending) {
            self.delete_many(&stored_locators);
            return Err(error);
        }
        Ok((collection_ref, durable_collection_page(&pending, 0)))
    }

    pub fn load_collection_page_durable(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        collection_ref: &str,
        project_id: &str,
        session_id: &str,
        cursor: &str,
        load_all: bool,
    ) -> Result<(CollectionPage, String), BackendError> {
        let id = durable_collection_id(collection_ref)?;
        let path = durable_collection_path(context, id)?;
        let mut collection: DurablePendingCollection = files.read_json(context, &path)?;
        validate_durable_collection(&collection, project_id, session_id)?;
        if cursor.is_empty() {
            return Ok((durable_collection_page(&collection, 0), collection.task_id));
        }
        if collection.next_cursor.as_deref() != Some(cursor) {
            return Err(collection_error(
                "Collection page cursor is invalid or stale.",
            ));
        }
        let previous_loaded = collection.loaded_count;
        collection.loaded_count = if load_all {
            collection.items.len()
        } else {
            collection
                .loaded_count
                .saturating_add(COLLECTION_PAGE_SIZE)
                .min(collection.items.len())
        };
        collection.next_cursor =
            durable_collection_cursor(id, collection.loaded_count, collection.items.len());
        files.write_json_atomic(context, &path, &collection)?;
        Ok((
            durable_collection_page(&collection, previous_loaded),
            collection.task_id,
        ))
    }

    pub fn resolve_collection_selection_durable(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        collection_ref: &str,
        project_id: &str,
        session_id: &str,
        selected_item_refs: &[String],
    ) -> Result<CollectionSelection, BackendError> {
        let id = durable_collection_id(collection_ref)?;
        let collection: DurablePendingCollection =
            files.read_json(context, &durable_collection_path(context, id)?)?;
        validate_durable_collection(&collection, project_id, session_id)?;
        let selected = selected_item_refs.iter().collect::<HashSet<_>>();
        let loaded_refs = collection
            .items
            .iter()
            .take(collection.loaded_count)
            .map(|item| item.item_ref.as_str())
            .collect::<HashSet<_>>();
        if selected_item_refs
            .iter()
            .any(|item_ref| !loaded_refs.contains(item_ref.as_str()))
        {
            return Err(collection_error(
                "Collection selection contains an item that was not loaded.",
            ));
        }
        let mut targets = Vec::with_capacity(selected_item_refs.len());
        for item in collection
            .items
            .iter()
            .filter(|item| selected.contains(&item.item_ref))
        {
            targets.push(CollectionSelectionTarget {
                target: self.resolve(&item.target_locator, Some(&item.public_url))?,
                discovery_fingerprint: item.discovery_fingerprint.clone(),
            });
        }
        if targets.len() != selected_item_refs.len() {
            return Err(collection_error(
                "Collection selection contains an unknown item reference.",
            ));
        }
        Ok(CollectionSelection {
            task_id: Some(collection.task_id),
            source_url: collection.source_url,
            platform: collection.platform,
            title: collection.title,
            targets,
        })
    }

    pub fn delete_collection_durable(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        collection_ref: &str,
    ) -> Result<(), BackendError> {
        let id = durable_collection_id(collection_ref)?;
        let path = durable_collection_path(context, id)?;
        let collection: DurablePendingCollection = files.read_json(context, &path)?;
        self.delete_many(
            &collection
                .items
                .iter()
                .map(|item| item.target_locator.clone())
                .collect::<Vec<_>>(),
        );
        let absolute = context.resolve_project_write_path(&path)?;
        remove_project_file(&context.root, &absolute).map_err(|_| store_error())
    }

    fn delete_many(&self, locators: &[String]) {
        for locator in locators {
            let _ = self.delete(locator);
        }
    }

    pub fn load_collection_page(
        &self,
        collection_ref: &str,
        project_id: &str,
        session_id: &str,
        cursor: &str,
        load_all: bool,
    ) -> Result<CollectionPage, BackendError> {
        let mut collections = self.pending_collections.lock().map_err(|_| store_error())?;
        collections.retain(|_, value| value.expires_at > chrono::Utc::now());
        let collection = collections
            .get_mut(collection_ref)
            .ok_or_else(|| collection_error("Collection preview expired; discover it again."))?;
        if collection.project_id != project_id || collection.session_id != session_id {
            return Err(collection_error(
                "Collection preview does not belong to this project session.",
            ));
        }
        if collection.next_cursor.as_deref() != Some(cursor) {
            return Err(collection_error(
                "Collection page cursor is invalid or stale.",
            ));
        }
        let previous_loaded = collection.loaded_count;
        collection.loaded_count = if load_all {
            collection.items.len()
        } else {
            collection
                .loaded_count
                .saturating_add(COLLECTION_PAGE_SIZE)
                .min(collection.items.len())
        };
        collection.next_cursor =
            (collection.loaded_count < collection.items.len()).then(opaque_collection_cursor);
        Ok(collection_page(collection, previous_loaded))
    }
    pub fn resolve_collection_selection(
        &self,
        collection_ref: &str,
        project_id: &str,
        session_id: &str,
        selected_item_refs: &[String],
    ) -> Result<CollectionSelection, BackendError> {
        if !collection_ref.starts_with(COLLECTION_PREFIX) {
            return Err(collection_error(
                "Collection selection reference is invalid.",
            ));
        }
        let mut collections = self.pending_collections.lock().map_err(|_| store_error())?;
        collections.retain(|_, value| value.expires_at > chrono::Utc::now());
        let collection = collections
            .get(collection_ref)
            .ok_or_else(|| collection_error("Collection preview expired; discover it again."))?;
        if collection.project_id != project_id || collection.session_id != session_id {
            return Err(collection_error(
                "Collection preview does not belong to this project session.",
            ));
        }
        let selected = selected_item_refs.iter().collect::<HashSet<_>>();
        let loaded_refs = collection
            .items
            .iter()
            .take(collection.loaded_count)
            .map(|item| item.item_ref.as_str())
            .collect::<HashSet<_>>();
        if selected_item_refs
            .iter()
            .any(|item_ref| !loaded_refs.contains(item_ref.as_str()))
        {
            return Err(collection_error(
                "Collection selection contains an item that was not loaded.",
            ));
        }
        let targets = collection
            .items
            .iter()
            .filter(|item| selected.contains(&item.item_ref))
            .map(|item| CollectionSelectionTarget {
                target: item.target.clone(),
                discovery_fingerprint: item.discovery_fingerprint.clone(),
            })
            .collect::<Vec<_>>();
        if targets.len() != selected_item_refs.len() {
            return Err(collection_error(
                "Collection selection contains an unknown item reference.",
            ));
        }
        Ok(CollectionSelection {
            task_id: None,
            source_url: collection.source_url.clone(),
            platform: collection.platform.clone(),
            title: collection.title.clone(),
            targets,
        })
    }
    pub fn delete_collection(&self, collection_ref: &str) -> Result<(), BackendError> {
        self.pending_collections
            .lock()
            .map_err(|_| store_error())?
            .remove(collection_ref);
        Ok(())
    }
    pub fn store(&self, target: &SessionWebTarget) -> Result<String, BackendError> {
        let reference = format!("{PREFIX}{}", uuid::Uuid::new_v4());
        let payload = StoredTarget {
            request_url: target.request_url.to_string(),
            expires_at: (chrono::Utc::now() + chrono::Duration::days(7)).to_rfc3339(),
        };
        self.secrets.set_account(
            &reference,
            &serde_json::to_string(&payload).map_err(|_| store_error())?,
        )?;
        Ok(reference)
    }
    pub fn resolve(
        &self,
        locator: &str,
        expected_public: Option<&str>,
    ) -> Result<SessionWebTarget, BackendError> {
        let target = if locator.starts_with(PREFIX) {
            let value = self.secrets.get_account(locator)?.ok_or_else(missing)?;
            let stored: StoredTarget = serde_json::from_str(&value).map_err(|_| missing())?;
            let expires = chrono::DateTime::parse_from_rfc3339(&stored.expires_at)
                .map_err(|_| missing())?
                .with_timezone(&chrono::Utc);
            if expires <= chrono::Utc::now() {
                self.secrets.delete_account(locator)?;
                return Err(missing());
            }
            UrlPolicy.normalize_for_session(&stored.request_url)?
        } else {
            UrlPolicy.normalize_for_session(locator)?
        };
        if expected_public.is_some_and(|expected| expected != target.public.public_url) {
            return Err(BackendError::new(
                "IMPORT_V2_URL_REFERENCE_MISMATCH",
                "Secure URL reference does not match its public locator.",
                false,
                true,
            ));
        }
        Ok(target)
    }
    pub fn renew(&self, reference: &str) -> Result<(), BackendError> {
        if !reference.starts_with(PREFIX) {
            return Ok(());
        }
        let value = self.secrets.get_account(reference)?.ok_or_else(missing)?;
        let mut stored: StoredTarget = serde_json::from_str(&value).map_err(|_| missing())?;
        let expires = chrono::DateTime::parse_from_rfc3339(&stored.expires_at)
            .map_err(|_| missing())?
            .with_timezone(&chrono::Utc);
        if expires <= chrono::Utc::now() {
            self.secrets.delete_account(reference)?;
            return Err(missing());
        }
        let minimum = chrono::Utc::now() + chrono::Duration::days(7);
        if expires < minimum {
            stored.expires_at = minimum.to_rfc3339();
            self.secrets.set_account(
                reference,
                &serde_json::to_string(&stored).map_err(|_| store_error())?,
            )?;
        }
        Ok(())
    }
    pub fn delete(&self, reference: &str) -> Result<(), BackendError> {
        if reference.starts_with(PREFIX) {
            self.secrets.delete_account(reference)?;
        }
        Ok(())
    }
    pub fn authorize_private(&self, grant: PrivateTargetGrant) -> Result<String, BackendError> {
        let id = format!("private-grant:{}", uuid::Uuid::new_v4());
        self.private_grants
            .lock()
            .map_err(|_| store_error())?
            .insert(grant.item_id.clone(), grant);
        Ok(id)
    }
    pub fn take_private(&self, item_id: &str) -> Result<Option<PrivateTargetGrant>, BackendError> {
        Ok(self
            .private_grants
            .lock()
            .map_err(|_| store_error())?
            .remove(item_id))
    }

    pub fn claim_private_for_operation(
        &self,
        item_id: &str,
        operation_id: &str,
    ) -> Result<(), BackendError> {
        let grant = {
            let mut pending = self.private_grants.lock().map_err(|_| store_error())?;
            pending.retain(|_, grant| grant.expires_at > chrono::Utc::now());
            pending.remove(item_id)
        };
        let mut active = self
            .active_private_grants
            .lock()
            .map_err(|_| store_error())?;
        active.retain(|_, grant| grant.expires_at > chrono::Utc::now());
        active.remove(&(item_id.to_owned(), operation_id.to_owned()));
        if let Some(grant) = grant {
            active.insert((item_id.to_owned(), operation_id.to_owned()), grant);
        }
        Ok(())
    }

    pub fn private_for_operation(
        &self,
        item_id: &str,
        operation_id: &str,
    ) -> Result<Option<PrivateTargetGrant>, BackendError> {
        let mut active = self
            .active_private_grants
            .lock()
            .map_err(|_| store_error())?;
        active.retain(|_, grant| grant.expires_at > chrono::Utc::now());
        Ok(active
            .get(&(item_id.to_owned(), operation_id.to_owned()))
            .cloned())
    }

    pub fn release_private_operation(
        &self,
        item_id: &str,
        operation_id: &str,
    ) -> Result<(), BackendError> {
        self.active_private_grants
            .lock()
            .map_err(|_| store_error())?
            .remove(&(item_id.to_owned(), operation_id.to_owned()));
        Ok(())
    }
    pub fn authorize_bilibili_asr(&self, grant: BilibiliAsrGrant) -> Result<(), BackendError> {
        let key = (
            grant.project_id.clone(),
            grant.session_id.clone(),
            grant.item_id.clone(),
        );
        self.asr_grants
            .lock()
            .map_err(|_| store_error())?
            .insert(key.clone(), grant);
        self.asr_reservations
            .lock()
            .map_err(|_| store_error())?
            .remove(&key);
        Ok(())
    }
    pub fn take_bilibili_asr(
        &self,
        project_id: &str,
        session_id: &str,
        item_id: &str,
        expected_request_url: &str,
    ) -> Result<Option<BilibiliAsrGrant>, BackendError> {
        let key = (
            project_id.to_string(),
            session_id.to_string(),
            item_id.to_string(),
        );
        let mut grants = self.asr_grants.lock().map_err(|_| store_error())?;
        grants.retain(|_, grant| grant.expires_at > chrono::Utc::now());
        if grants
            .get(&key)
            .is_some_and(|grant| grant.target_sha256 != asr_target_sha256(expected_request_url))
        {
            return Err(BackendError::new(
                "IMPORT_V2_URL_REFERENCE_MISMATCH",
                "The local ASR authorization does not match this import target.",
                false,
                true,
            ));
        }
        let value = grants.remove(&key);
        self.asr_reservations
            .lock()
            .map_err(|_| store_error())?
            .remove(&key);
        Ok(value)
    }
    pub fn has_bilibili_asr(
        &self,
        project_id: &str,
        session_id: &str,
        item_id: &str,
        expected_request_url: &str,
    ) -> Result<bool, BackendError> {
        let key = (
            project_id.to_string(),
            session_id.to_string(),
            item_id.to_string(),
        );
        let mut grants = self.asr_grants.lock().map_err(|_| store_error())?;
        grants.retain(|_, grant| grant.expires_at > chrono::Utc::now());
        let Some(grant) = grants.get(&key) else {
            return Ok(false);
        };
        if grant.target_sha256 != asr_target_sha256(expected_request_url) {
            return Err(BackendError::new(
                "IMPORT_V2_URL_REFERENCE_MISMATCH",
                "The local ASR authorization does not match this import target.",
                false,
                true,
            ));
        }
        Ok(!self
            .asr_reservations
            .lock()
            .map_err(|_| store_error())?
            .contains(&key))
    }
    pub fn reserve_bilibili_asr(
        &self,
        project_id: &str,
        session_id: &str,
        item_id: &str,
        expected_request_url: &str,
    ) -> Result<bool, BackendError> {
        if !self.has_bilibili_asr(project_id, session_id, item_id, expected_request_url)? {
            return Ok(false);
        }
        let key = (
            project_id.to_string(),
            session_id.to_string(),
            item_id.to_string(),
        );
        Ok(self
            .asr_reservations
            .lock()
            .map_err(|_| store_error())?
            .insert(key))
    }
    pub fn bind_authenticated_profile(
        &self,
        project_id: &str,
        session_id: &str,
        item_id: &str,
        profile: PathBuf,
    ) -> Result<(), BackendError> {
        self.bind_authenticated_profiles(project_id, session_id, &[item_id.to_string()], &profile)
    }
    pub fn bind_authenticated_profiles(
        &self,
        project_id: &str,
        session_id: &str,
        item_ids: &[String],
        profile: &Path,
    ) -> Result<(), BackendError> {
        if item_ids.is_empty() || !profile.is_dir() {
            return Err(store_error());
        }
        let mut profiles = self
            .authenticated_profiles
            .lock()
            .map_err(|_| store_error())?;
        for item_id in item_ids {
            profiles.insert(
                (
                    project_id.to_string(),
                    session_id.to_string(),
                    item_id.clone(),
                ),
                profile.to_path_buf(),
            );
        }
        Ok(())
    }

    pub fn unbind_authenticated_profiles(
        &self,
        project_id: &str,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<(), BackendError> {
        let item_ids = item_ids.iter().collect::<HashSet<_>>();
        self.authenticated_profiles
            .lock()
            .map_err(|_| store_error())?
            .retain(|(project, session, item), _| {
                project != project_id || session != session_id || !item_ids.contains(item)
            });
        Ok(())
    }
    pub fn take_authenticated_profile(
        &self,
        project_id: &str,
        session_id: &str,
        item_id: &str,
    ) -> Result<Option<PathBuf>, BackendError> {
        let key = (
            project_id.to_string(),
            session_id.to_string(),
            item_id.to_string(),
        );
        Ok(self
            .authenticated_profiles
            .lock()
            .map_err(|_| store_error())?
            .remove(&key)
            .filter(|path| path.is_dir()))
    }
}

fn opaque_collection_cursor() -> String {
    format!("import-web-collection-cursor:{}", uuid::Uuid::new_v4())
}

fn durable_collection_id(collection_ref: &str) -> Result<uuid::Uuid, BackendError> {
    collection_ref
        .strip_prefix(COLLECTION_PREFIX)
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .ok_or_else(|| collection_error("Collection selection reference is invalid."))
}

fn durable_collection_path(
    context: &ProjectContext,
    id: uuid::Uuid,
) -> Result<String, BackendError> {
    context
        .layout
        .import_paths()?
        .collection_preview(&id.to_string())
}

fn durable_collection_cursor(id: uuid::Uuid, loaded_count: usize, total: usize) -> Option<String> {
    (loaded_count < total).then(|| format!("import-web-collection-cursor:{id}:{loaded_count}"))
}

fn validate_durable_collection(
    collection: &DurablePendingCollection,
    project_id: &str,
    session_id: &str,
) -> Result<(), BackendError> {
    let expires = chrono::DateTime::parse_from_rfc3339(&collection.expires_at)
        .map_err(|_| collection_error("Collection preview is invalid; discover it again."))?
        .with_timezone(&chrono::Utc);
    if collection.schema_version != DURABLE_COLLECTION_SCHEMA_VERSION
        || collection.project_id != project_id
        || collection.session_id != session_id
        || expires <= chrono::Utc::now()
        || collection.loaded_count > collection.items.len()
    {
        return Err(collection_error(
            "Collection preview expired or does not belong to this project session.",
        ));
    }
    Ok(())
}

fn durable_collection_page(collection: &DurablePendingCollection, start: usize) -> CollectionPage {
    CollectionPage {
        items: collection
            .items
            .iter()
            .skip(start)
            .take(collection.loaded_count.saturating_sub(start))
            .map(|item| CollectionPreviewTarget {
                item_ref: item.item_ref.clone(),
                title: item.title.clone(),
                public_url: item.public_url.clone(),
            })
            .collect(),
        discovered_total: collection.items.len(),
        loaded_count: collection.loaded_count,
        has_more: collection.loaded_count < collection.items.len(),
        next_cursor: collection.next_cursor.clone(),
    }
}

fn collection_page(collection: &PendingCollection, start: usize) -> CollectionPage {
    CollectionPage {
        items: collection
            .items
            .iter()
            .skip(start)
            .take(collection.loaded_count.saturating_sub(start))
            .map(|item| CollectionPreviewTarget {
                item_ref: item.item_ref.clone(),
                title: item.title.clone(),
                public_url: item.target.public.public_url.clone(),
            })
            .collect(),
        discovered_total: collection.items.len(),
        loaded_count: collection.loaded_count,
        has_more: collection.loaded_count < collection.items.len(),
        next_cursor: collection.next_cursor.clone(),
    }
}
pub fn asr_target_sha256(request_url: &str) -> String {
    format!("{:x}", Sha256::digest(request_url.as_bytes()))
}
fn missing() -> BackendError {
    BackendError::new(
        "IMPORT_V2_URL_REFERENCE_EXPIRED",
        "The secure URL reference is missing or expired.",
        true,
        true,
    )
}
fn store_error() -> BackendError {
    BackendError::new(
        "IMPORT_V2_URL_REFERENCE_FAILED",
        "The secure URL reference could not be stored.",
        true,
        true,
    )
}

fn collection_error(message: &str) -> BackendError {
    BackendError::new(
        "IMPORT_WEB_COLLECTION_SELECTION_INVALID",
        message,
        false,
        true,
    )
}

#[cfg(test)]
mod tests {
    use crate::{
        models::paths::ProjectContext,
        services::{import_v2::url_policy::UrlPolicy, FileStore, SecretService},
    };

    use super::{
        durable_collection_id, durable_collection_path, DurablePendingCollection, WebTargetStore,
    };

    #[test]
    fn collection_preview_is_session_bound_and_selection_keeps_source_order() {
        let store = WebTargetStore::default();
        let first = UrlPolicy
            .normalize_for_session("https://www.bilibili.com/video/BV1first")
            .unwrap();
        let second = UrlPolicy
            .normalize_for_session("https://www.bilibili.com/video/BV2second")
            .unwrap();
        let (collection_ref, page) = store
            .store_collection(
                "project",
                "session",
                "https://www.bilibili.com/medialist/play/42".into(),
                "bilibili".into(),
                "课程合集".into(),
                vec![
                    ("第一讲".into(), first, "fingerprint-first".into()),
                    ("第二讲".into(), second, "fingerprint-second".into()),
                ],
            )
            .unwrap();
        let previews = page.items;

        let error = store
            .resolve_collection_selection(
                &collection_ref,
                "project",
                "another-session",
                &[previews[0].item_ref.clone()],
            )
            .unwrap_err();
        assert_eq!(error.code, "IMPORT_WEB_COLLECTION_SELECTION_INVALID");
        let selected = store
            .resolve_collection_selection(
                &collection_ref,
                "project",
                "session",
                &[previews[1].item_ref.clone(), previews[0].item_ref.clone()],
            )
            .unwrap();
        assert_eq!(
            selected
                .targets
                .iter()
                .map(|target| target.target.public.public_url.as_str())
                .collect::<Vec<_>>(),
            [
                "https://www.bilibili.com/video/BV1first",
                "https://www.bilibili.com/video/BV2second"
            ]
        );
    }

    #[test]
    fn collection_pages_continue_past_two_hundred_without_truncation() {
        let store = WebTargetStore::default();
        let items = (0..225)
            .map(|index| {
                (
                    format!("第 {} 讲", index + 1),
                    UrlPolicy
                        .normalize_for_session(&format!(
                            "https://www.bilibili.com/video/BV{index:010}"
                        ))
                        .unwrap(),
                    format!("fingerprint-{index}"),
                )
            })
            .collect();
        let (collection_ref, first) = store
            .store_collection(
                "project",
                "session",
                "https://space.bilibili.com/42".into(),
                "bilibili".into(),
                "作者视频".into(),
                items,
            )
            .unwrap();
        assert_eq!(first.discovered_total, 225);
        assert_eq!(first.items.len(), 50);
        assert!(first.has_more);

        let second = store
            .load_collection_page(
                &collection_ref,
                "project",
                "session",
                first.next_cursor.as_deref().unwrap(),
                false,
            )
            .unwrap();
        assert_eq!(second.items.len(), 50);
        assert_eq!(second.loaded_count, 100);
        let remaining = store
            .load_collection_page(
                &collection_ref,
                "project",
                "session",
                second.next_cursor.as_deref().unwrap(),
                true,
            )
            .unwrap();
        assert_eq!(remaining.items.len(), 125);
        assert_eq!(remaining.loaded_count, 225);
        assert!(!remaining.has_more);
        assert!(remaining.next_cursor.is_none());
    }

    #[test]
    fn durable_collection_survives_store_recreation_and_deletes_child_locators() {
        let root = tempfile::tempdir().unwrap();
        let mut context = ProjectContext::new("project", root.path().to_path_buf());
        context.layout.app_state_root = Some(".app/compat".into());
        context.layout.import_state_root = Some(".app/compat/import-sessions".into());
        let files = FileStore::default();
        let secrets = SecretService::memory();
        let first_store = WebTargetStore::new(secrets.clone());
        let items = (0..55)
            .map(|index| {
                (
                    format!("Entry {index}"),
                    UrlPolicy
                        .normalize_for_session(&format!(
                            "https://www.bilibili.com/video/BV{index:010}?token=secret-{index}"
                        ))
                        .unwrap(),
                    format!("fingerprint-{index}"),
                )
            })
            .collect();
        let (collection_ref, first_page) = first_store
            .store_collection_durable(
                &context,
                &files,
                "task-collection",
                "project",
                "session",
                "https://space.bilibili.com/42".into(),
                "bilibili".into(),
                "Durable collection".into(),
                items,
            )
            .unwrap();
        assert_eq!(first_page.loaded_count, 50);
        let id = durable_collection_id(&collection_ref).unwrap();
        assert!(durable_collection_path(&context, id)
            .unwrap()
            .starts_with(".app/compat/import-collections/"));
        let persisted: DurablePendingCollection = files
            .read_json(&context, &durable_collection_path(&context, id).unwrap())
            .unwrap();
        assert!(!serde_json::to_string(&persisted)
            .unwrap()
            .contains("secret-0"));
        let first_locator = persisted.items[0].target_locator.clone();

        let reopened = WebTargetStore::new(secrets.clone());
        let (last_page, task_id) = reopened
            .load_collection_page_durable(
                &context,
                &files,
                &collection_ref,
                "project",
                "session",
                first_page.next_cursor.as_deref().unwrap(),
                true,
            )
            .unwrap();
        assert_eq!(task_id, "task-collection");
        assert_eq!(last_page.loaded_count, 55);
        let (rehydrated_page, rehydrated_task_id) = reopened
            .load_collection_page_durable(
                &context,
                &files,
                &collection_ref,
                "project",
                "session",
                "",
                false,
            )
            .unwrap();
        assert_eq!(rehydrated_task_id, "task-collection");
        assert_eq!(rehydrated_page.loaded_count, 55);
        assert_eq!(rehydrated_page.items.len(), 55);
        assert!(rehydrated_page.next_cursor.is_none());
        let selected = reopened
            .resolve_collection_selection_durable(
                &context,
                &files,
                &collection_ref,
                "project",
                "session",
                &[last_page.items[0].item_ref.clone()],
            )
            .unwrap();
        assert_eq!(selected.targets.len(), 1);
        reopened
            .delete_collection_durable(&context, &files, &collection_ref)
            .unwrap();
        assert!(secrets.get_account(&first_locator).unwrap().is_none());
        assert!(!context
            .resolve_project_path(&durable_collection_path(&context, id).unwrap())
            .unwrap()
            .exists());
    }

    #[test]
    fn authenticated_profile_is_bound_to_the_whole_login_group() {
        let store = WebTargetStore::default();
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        std::fs::create_dir(&profile).unwrap();
        let item_ids = vec!["item-a".to_string(), "item-b".to_string()];

        store
            .bind_authenticated_profiles("project", "session", &item_ids, &profile)
            .unwrap();

        assert_eq!(
            store
                .take_authenticated_profile("project", "session", "item-a")
                .unwrap(),
            Some(profile.clone())
        );
        assert_eq!(
            store
                .take_authenticated_profile("project", "session", "item-b")
                .unwrap(),
            Some(profile)
        );
    }
}
