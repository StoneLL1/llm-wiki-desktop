use std::fs;
use std::path::PathBuf;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::BackendError;
use crate::models::llm::LlmProviderConfig;
use crate::models::paths::ProjectContext;
use crate::models::workflow::WorkflowKind;
use crate::services::compile_service::ResolvedCompileSource;
use crate::tasks::TaskService;
use crate::utils::private_directory::{ensure_private_directory, validate_private_directory};

/// Only a user-created Workflow retry reads an earlier run's verified batches.
/// Prompt, provider configuration and project identity all participate in the
/// key, so a changed input or route creates a fresh model request.
pub(super) struct ByokBatchCache {
    path: PathBuf,
    project: String,
    config: Vec<u8>,
    reuse: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedBatch<T> {
    input_hash: String,
    value_hash: String,
    value: T,
}

impl ByokBatchCache {
    pub(super) fn for_workflow(
        tasks: &TaskService,
        task_id: &str,
        context: &ProjectContext,
        config: &LlmProviderConfig,
        sources: &[ResolvedCompileSource],
    ) -> Result<Option<Self>, BackendError> {
        let Some(run) = tasks.get_workflow_run(task_id) else {
            return Ok(None);
        };
        if run.kind != WorkflowKind::UpdateWiki {
            return Ok(None);
        }
        let root_id = run
            .retry
            .as_ref()
            .map_or(task_id, |retry| retry.attempt_of.as_str());
        let root_id = uuid::Uuid::parse_str(root_id).map_err(|error| cache_error(error))?;
        let parent = std::env::temp_dir().join("llm-wiki-desktop");
        ensure_private_directory(&parent).map_err(cache_error)?;
        let path = parent.join(format!("{root_id}-byok-batches"));
        ensure_private_directory(&path).map_err(cache_error)?;
        let selected_versions = sources
            .iter()
            .map(|source| &source.reference)
            .collect::<Vec<_>>();
        let config = serde_json::to_vec(&(config, selected_versions)).map_err(cache_error)?;
        Ok(Some(Self {
            path,
            project: context.root.to_string_lossy().into_owned(),
            config,
            reuse: run.retry.is_some(),
        }))
    }

    fn key(&self, kind: &str, prompt: &str) -> String {
        let mut hash = Sha256::new();
        hash.update(b"byok-batch-v1\0");
        hash.update(kind.as_bytes());
        hash.update(b"\0");
        hash.update(self.project.as_bytes());
        hash.update(b"\0");
        hash.update(&self.config);
        hash.update(b"\0");
        hash.update(prompt.as_bytes());
        format!("{:x}", hash.finalize())
    }

    pub(super) fn load<T: DeserializeOwned + Serialize>(
        &self,
        kind: &str,
        prompt: &str,
    ) -> Result<Option<T>, BackendError> {
        if !self.reuse {
            return Ok(None);
        }
        validate_private_directory(&self.path).map_err(cache_error)?;
        let key = self.key(kind, prompt);
        let path = self.path.join(format!("{key}.json"));
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(cache_error(error)),
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(cache_error("BYOK batch cache entry is not a regular file"));
        }
        let bytes = fs::read(&path).map_err(cache_error)?;
        let cached: CachedBatch<T> = serde_json::from_slice(&bytes).map_err(cache_error)?;
        let value = serde_json::to_vec(&cached.value).map_err(cache_error)?;
        let actual = format!("{:x}", Sha256::digest(value));
        if cached.input_hash != key || cached.value_hash != actual {
            return Err(cache_error("BYOK batch cache integrity check failed"));
        }
        Ok(Some(cached.value))
    }

    pub(super) fn save<T: Serialize>(
        &self,
        kind: &str,
        prompt: &str,
        value: &T,
    ) -> Result<(), BackendError> {
        validate_private_directory(&self.path).map_err(cache_error)?;
        let key = self.key(kind, prompt);
        let value_bytes = serde_json::to_vec(value).map_err(cache_error)?;
        let cached = CachedBatch {
            input_hash: key.clone(),
            value_hash: format!("{:x}", Sha256::digest(&value_bytes)),
            value,
        };
        let bytes = serde_json::to_vec(&cached).map_err(cache_error)?;
        let temporary_path = self.path.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut temporary = options.open(&temporary_path).map_err(cache_error)?;
        use std::io::Write;
        temporary.write_all(&bytes).map_err(cache_error)?;
        temporary.sync_all().map_err(cache_error)?;
        drop(temporary);
        let target = self.path.join(format!("{key}.json"));
        if target.exists() {
            let metadata = fs::symlink_metadata(&target).map_err(cache_error)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(cache_error("BYOK batch cache entry is not a regular file"));
            }
            fs::remove_file(&target).map_err(cache_error)?;
        }
        fs::rename(&temporary_path, target).map_err(cache_error)?;
        Ok(())
    }

    pub(super) fn cleanup(&self) -> Result<(), BackendError> {
        validate_private_directory(&self.path).map_err(cache_error)?;
        for entry in fs::read_dir(&self.path).map_err(cache_error)? {
            let entry = entry.map_err(cache_error)?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(cache_error)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(cache_error("BYOK batch cache contains an unexpected entry"));
            }
            fs::remove_file(entry.path()).map_err(cache_error)?;
        }
        fs::remove_dir(&self.path).map_err(cache_error)?;
        Ok(())
    }
}

fn cache_error(error: impl std::fmt::Display) -> BackendError {
    BackendError::new("COMPILE_BATCH_CACHE_FAILED", error.to_string(), true, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_retry_reuses_only_matching_verified_batch() {
        let root = tempfile::tempdir().unwrap();
        let first = ByokBatchCache {
            path: root.path().to_path_buf(),
            project: "/project/知识".into(),
            config: b"provider-a".to_vec(),
            reuse: false,
        };
        first.save("page", "old input", &"verified result").unwrap();
        assert!(first.load::<String>("page", "old input").unwrap().is_none());
        let retry = ByokBatchCache {
            reuse: true,
            ..first
        };
        assert_eq!(
            retry
                .load::<String>("page", "old input")
                .unwrap()
                .as_deref(),
            Some("verified result")
        );
        assert!(retry
            .load::<String>("page", "changed input")
            .unwrap()
            .is_none());
        assert!(retry.load::<String>("plan", "old input").unwrap().is_none());
        let changed_route = ByokBatchCache {
            path: retry.path.clone(),
            project: retry.project.clone(),
            config: b"provider-b".to_vec(),
            reuse: true,
        };
        assert!(changed_route
            .load::<String>("page", "old input")
            .unwrap()
            .is_none());
        retry.cleanup().unwrap();
    }
}
