use crate::{
    errors::BackendError,
    services::{
        import_v2::url_policy::{SessionWebTarget, UrlPolicy},
        SecretService,
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::{HashMap, HashSet}, path::PathBuf, sync::{Arc, Mutex}};

use super::url_policy::PrivateTargetGrant;

const PREFIX: &str = "import-web-target:";

#[derive(Clone)]
pub struct WebTargetStore {
    secrets: SecretService,
    private_grants: Arc<Mutex<HashMap<String, PrivateTargetGrant>>>,
    asr_grants: Arc<Mutex<HashMap<(String, String, String), BilibiliAsrGrant>>>,
    asr_reservations: Arc<Mutex<HashSet<(String, String, String)>>>,
    authenticated_profiles: Arc<Mutex<HashMap<(String, String, String), PathBuf>>>,
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
            asr_grants: Arc::new(Mutex::new(HashMap::new())),
            asr_reservations: Arc::new(Mutex::new(HashSet::new())),
            authenticated_profiles: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
impl WebTargetStore {
    pub fn new(secrets: SecretService) -> Self {
        Self { secrets, private_grants: Arc::new(Mutex::new(HashMap::new())), asr_grants: Arc::new(Mutex::new(HashMap::new())), asr_reservations: Arc::new(Mutex::new(HashSet::new())), authenticated_profiles: Arc::new(Mutex::new(HashMap::new())) }
    }
    pub fn store(&self, target: &SessionWebTarget) -> Result<String, BackendError> {
        let reference = format!("{PREFIX}{}", uuid::Uuid::new_v4());
        let payload = StoredTarget {
            request_url: target.request_url.to_string(),
            expires_at: (chrono::Utc::now() + chrono::Duration::hours(24)).to_rfc3339(),
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
        Ok(self.private_grants.lock().map_err(|_| store_error())?.remove(item_id))
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
        self.asr_reservations.lock().map_err(|_| store_error())?.remove(&key);
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
        if grants.get(&key).is_some_and(|grant| {
            grant.target_sha256 != asr_target_sha256(expected_request_url)
        })
        {
            return Err(BackendError::new(
                "IMPORT_V2_URL_REFERENCE_MISMATCH",
                "The local ASR authorization does not match this import target.",
                false,
                true,
            ));
        }
        let value = grants.remove(&key);
        self.asr_reservations.lock().map_err(|_| store_error())?.remove(&key);
        Ok(value)
    }
    pub fn has_bilibili_asr(
        &self,
        project_id: &str,
        session_id: &str,
        item_id: &str,
        expected_request_url: &str,
    ) -> Result<bool, BackendError> {
        let key = (project_id.to_string(), session_id.to_string(), item_id.to_string());
        let mut grants = self.asr_grants.lock().map_err(|_| store_error())?;
        grants.retain(|_, grant| grant.expires_at > chrono::Utc::now());
        let Some(grant) = grants.get(&key) else { return Ok(false); };
        if grant.target_sha256 != asr_target_sha256(expected_request_url) {
            return Err(BackendError::new(
                "IMPORT_V2_URL_REFERENCE_MISMATCH",
                "The local ASR authorization does not match this import target.",
                false,
                true,
            ));
        }
        Ok(!self.asr_reservations.lock().map_err(|_| store_error())?.contains(&key))
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
        let key = (project_id.to_string(), session_id.to_string(), item_id.to_string());
        Ok(self.asr_reservations.lock().map_err(|_| store_error())?.insert(key))
    }
    pub fn bind_authenticated_profile(&self, project_id: &str, session_id: &str, item_id: &str, profile: PathBuf) -> Result<(), BackendError> {
        let key = (project_id.to_string(), session_id.to_string(), item_id.to_string());
        self.authenticated_profiles.lock().map_err(|_| store_error())?.insert(key, profile);
        Ok(())
    }
    pub fn take_authenticated_profile(&self, project_id: &str, session_id: &str, item_id: &str) -> Result<Option<PathBuf>, BackendError> {
        let key = (project_id.to_string(), session_id.to_string(), item_id.to_string());
        Ok(self.authenticated_profiles.lock().map_err(|_| store_error())?.remove(&key).filter(|path| path.is_dir()))
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
