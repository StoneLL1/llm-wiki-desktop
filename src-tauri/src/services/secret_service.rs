use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::errors::BackendError;
use crate::models::llm::LlmProviderKind;

const SERVICE_NAME: &str = "LLM Wiki Desktop";

#[derive(Clone, Default)]
pub struct SecretService {
    memory: Option<Arc<RwLock<HashMap<String, String>>>>,
}

impl SecretService {
    pub fn memory() -> Self {
        Self {
            memory: Some(Arc::new(RwLock::new(HashMap::new()))),
        }
    }

    pub fn set(&self, provider: LlmProviderKind, secret: &str) -> Result<(), BackendError> {
        if secret.trim().is_empty() {
            return Err(BackendError::new(
                "SECRET_EMPTY",
                "Provider secret cannot be empty.",
                true,
                true,
            ));
        }
        if let Some(memory) = &self.memory {
            memory
                .write()
                .expect("secret lock poisoned")
                .insert(provider.credential_account().into(), secret.into());
            return Ok(());
        }
        keyring_entry(provider)?
            .set_password(secret)
            .map_err(secret_error)
    }

    pub fn get(&self, provider: LlmProviderKind) -> Result<Option<String>, BackendError> {
        if let Some(memory) = &self.memory {
            return Ok(memory
                .read()
                .expect("secret lock poisoned")
                .get(provider.credential_account())
                .cloned());
        }
        match keyring_entry(provider)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(secret_error(error)),
        }
    }

    pub fn delete(&self, provider: LlmProviderKind) -> Result<(), BackendError> {
        if let Some(memory) = &self.memory {
            memory
                .write()
                .expect("secret lock poisoned")
                .remove(provider.credential_account());
            return Ok(());
        }
        match keyring_entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(secret_error(error)),
        }
    }

    pub fn mask(&self, provider: LlmProviderKind) -> Result<Option<String>, BackendError> {
        Ok(self.get(provider)?.map(|secret| {
            let suffix: String = secret
                .chars()
                .rev()
                .take(4)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            format!("••••{suffix}")
        }))
    }
}

fn keyring_entry(provider: LlmProviderKind) -> Result<keyring::Entry, BackendError> {
    keyring::Entry::new(SERVICE_NAME, provider.credential_account()).map_err(secret_error)
}

fn secret_error(error: keyring::Error) -> BackendError {
    BackendError::new("SECRET_BACKEND_FAILED", error.to_string(), true, true)
}
