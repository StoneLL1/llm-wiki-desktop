use crate::{
    errors::BackendError,
    services::{
        import_v2::url_policy::{SessionWebTarget, UrlPolicy},
        SecretService,
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::{Arc, Mutex}};

use super::url_policy::PrivateTargetGrant;

const PREFIX: &str = "import-web-target:";

#[derive(Clone)]
pub struct WebTargetStore {
    secrets: SecretService,
    private_grants: Arc<Mutex<HashMap<String, PrivateTargetGrant>>>,
    authenticated_profiles: Arc<Mutex<HashMap<String, PathBuf>>>,
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
            authenticated_profiles: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
impl WebTargetStore {
    pub fn new(secrets: SecretService) -> Self {
        Self { secrets, private_grants: Arc::new(Mutex::new(HashMap::new())), authenticated_profiles: Arc::new(Mutex::new(HashMap::new())) }
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
    pub fn bind_authenticated_profile(&self, item_id: &str, profile: PathBuf) -> Result<(), BackendError> {
        self.authenticated_profiles.lock().map_err(|_| store_error())?.insert(item_id.to_string(), profile);
        Ok(())
    }
    pub fn authenticated_profile(&self, item_id: &str) -> Result<Option<PathBuf>, BackendError> {
        Ok(self.authenticated_profiles.lock().map_err(|_| store_error())?.get(item_id).filter(|path| path.is_dir()).cloned())
    }
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
