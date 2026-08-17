use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::errors::BackendError;
use crate::models::llm::{LlmProviderKind, ProviderCredentialBinding};
use crate::models::paths::ProjectContext;

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

    pub fn provider_binding_account_id(
        context: &ProjectContext,
        provider: LlmProviderKind,
        config_id: &str,
        canonical_origin: &str,
        revision: u64,
    ) -> Result<String, BackendError> {
        uuid::Uuid::parse_str(config_id).map_err(|_| binding_invalid())?;
        if revision == 0 || canonical_origin.is_empty() || canonical_origin.len() > 512 {
            return Err(binding_invalid());
        }
        let project_scope = binding_hash(&project_scope_key(context));
        let origin_scope = binding_hash(canonical_origin);
        let account = format!(
            "provider.binding.v1.{project_scope}.{}.{config_id}.{origin_scope}.{revision}",
            provider.binding_slug()
        );
        valid_account(&account)?;
        Ok(account)
    }

    pub fn set_bound(
        &self,
        context: &ProjectContext,
        binding: &ProviderCredentialBinding,
        secret: &str,
    ) -> Result<(), BackendError> {
        let account = validate_binding_account(context, binding)?;
        if binding.approved_at.is_none() {
            return Err(binding_invalid());
        }
        self.set_account(&account, secret)
    }

    pub fn get_bound(
        &self,
        context: &ProjectContext,
        binding: &ProviderCredentialBinding,
    ) -> Result<Option<String>, BackendError> {
        let account = validate_binding_account(context, binding)?;
        if binding.approved_at.is_none() {
            return Ok(None);
        }
        self.get_account(&account)
    }

    pub fn delete_bound(
        &self,
        context: &ProjectContext,
        binding: &ProviderCredentialBinding,
    ) -> Result<(), BackendError> {
        let account = validate_binding_account(context, binding)?;
        self.delete_account(&account)
    }

    pub fn mask_bound(
        &self,
        context: &ProjectContext,
        binding: &ProviderCredentialBinding,
    ) -> Result<Option<String>, BackendError> {
        Ok(self.get_bound(context, binding)?.map(mask_secret))
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
        Ok(self.get(provider)?.map(mask_secret))
    }
}

fn validate_binding_account(
    context: &ProjectContext,
    binding: &ProviderCredentialBinding,
) -> Result<String, BackendError> {
    let expected = SecretService::provider_binding_account_id(
        context,
        binding.provider_kind,
        &binding.config_id,
        &binding.canonical_origin,
        binding.revision,
    )?;
    if binding.credential_account_id != expected {
        return Err(binding_invalid());
    }
    Ok(expected)
}

fn project_scope_key(context: &ProjectContext) -> String {
    let mut root = context.root.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        root = root.to_ascii_lowercase();
    }
    format!("{}\0{root}", context.project_id)
}

fn binding_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(value.as_bytes());
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn mask_secret(secret: String) -> String {
    let suffix: String = secret
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("\u{2022}\u{2022}\u{2022}\u{2022}{suffix}")
}

fn binding_invalid() -> BackendError {
    BackendError::new(
        "PROVIDER_CREDENTIAL_BINDING_INVALID",
        "The saved provider credential binding is invalid or changed.",
        true,
        true,
    )
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
