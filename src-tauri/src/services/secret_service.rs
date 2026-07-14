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
        self.set_account(provider.credential_account(), secret)
    }

    pub fn set_account(&self, account: &str, secret: &str) -> Result<(), BackendError> {
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
                .insert(valid_account(account)?.into(), secret.into());
            return Ok(());
        }
        keyring_account(valid_account(account)?)?
            .set_password(secret)
            .map_err(secret_error)
    }

    pub fn get(&self, provider: LlmProviderKind) -> Result<Option<String>, BackendError> {
        self.get_account(provider.credential_account())
    }

    pub fn get_account(&self, account: &str) -> Result<Option<String>, BackendError> {
        if let Some(memory) = &self.memory {
            return Ok(memory
                .read()
                .expect("secret lock poisoned")
                .get(valid_account(account)?)
                .cloned());
        }
        match keyring_account(valid_account(account)?)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(secret_error(error)),
        }
    }

    pub fn delete(&self, provider: LlmProviderKind) -> Result<(), BackendError> {
        self.delete_account(provider.credential_account())
    }

    pub fn delete_account(&self, account: &str) -> Result<(), BackendError> {
        if let Some(memory) = &self.memory {
            memory
                .write()
                .expect("secret lock poisoned")
                .remove(valid_account(account)?);
            return Ok(());
        }
        match keyring_account(valid_account(account)?)?.delete_credential() {
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
            format!("\u{2022}\u{2022}\u{2022}\u{2022}{suffix}")
        }))
    }
}

fn keyring_account(account: &str) -> Result<keyring::Entry, BackendError> {
    keyring::Entry::new(SERVICE_NAME, account).map_err(secret_error)
}

fn valid_account(account: &str) -> Result<&str, BackendError> {
    if account.is_empty()
        || account.len() > 200
        || !account
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
    {
        return Err(BackendError::new(
            "SECRET_ACCOUNT_INVALID",
            "Secret account identifier is invalid.",
            false,
            true,
        ));
    }
    Ok(account)
}

fn secret_error(error: keyring::Error) -> BackendError {
    BackendError::new("SECRET_BACKEND_FAILED", error.to_string(), true, true)
}
