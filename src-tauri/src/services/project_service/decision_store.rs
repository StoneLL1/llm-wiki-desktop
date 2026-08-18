use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::errors::BackendError;
use crate::models::project::ProjectOpenIntent;
use crate::services::FileStore;

const PROJECT_OPEN_DECISION_FILE: &str = "project-open-decisions.json";
const PROJECT_OPEN_DECISION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredProjectOpenDecision {
    canonical_identity_key: String,
    identity_revision: String,
    intent: ProjectOpenIntent,
    decided_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectOpenDecisionFile {
    schema_version: u32,
    entries: Vec<StoredProjectOpenDecision>,
}

impl Default for ProjectOpenDecisionFile {
    fn default() -> Self {
        Self {
            schema_version: PROJECT_OPEN_DECISION_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

pub(crate) struct ProjectOpenDecisionStore {
    root: PathBuf,
    path: PathBuf,
    lock: RwLock<()>,
}

impl ProjectOpenDecisionStore {
    pub(crate) fn new(config_dir: &Path) -> Self {
        Self {
            root: config_dir.parent().unwrap_or(config_dir).to_path_buf(),
            path: config_dir.join(PROJECT_OPEN_DECISION_FILE),
            lock: RwLock::new(()),
        }
    }

    /// A corrupt or unavailable memory must never block a safe read-only
    /// assessment. Treat it as absent; an explicit new choice can replace it.
    pub(crate) fn lookup(
        &self,
        canonical_identity_key: &str,
        identity_revision: &str,
    ) -> Option<ProjectOpenIntent> {
        let _guard = self.lock.read().ok()?;
        self.read_file()
            .ok()?
            .entries
            .into_iter()
            .find_map(|entry| {
                (entry.canonical_identity_key == canonical_identity_key
                    && entry.identity_revision == identity_revision)
                    .then_some(entry.intent)
            })
    }

    pub(crate) fn remember(
        &self,
        canonical_identity_key: &str,
        identity_revision: &str,
        intent: ProjectOpenIntent,
    ) -> Result<(), BackendError> {
        let _guard = self.lock.write().map_err(|_| decision_store_locked())?;
        let mut file = self.read_file()?;
        file.entries
            .retain(|entry| entry.canonical_identity_key != canonical_identity_key);
        file.entries.push(StoredProjectOpenDecision {
            canonical_identity_key: canonical_identity_key.to_string(),
            identity_revision: identity_revision.to_string(),
            intent,
            decided_at: Utc::now().to_rfc3339(),
        });
        FileStore.write_json_atomic_absolute(&self.root, &self.path, &file)
    }

    /// Removes the decision bound to this exact folder identity and revision.
    /// This only changes global application preferences; it never touches the
    /// assessed folder or its contents.
    pub(crate) fn forget(
        &self,
        canonical_identity_key: &str,
        identity_revision: &str,
    ) -> Result<(), BackendError> {
        let _guard = self.lock.write().map_err(|_| decision_store_locked())?;
        let mut file = self.read_file()?;
        let entry_count = file.entries.len();
        file.entries.retain(|entry| {
            entry.canonical_identity_key != canonical_identity_key
                || entry.identity_revision != identity_revision
        });
        if file.entries.len() == entry_count {
            return Ok(());
        }
        FileStore.write_json_atomic_absolute(&self.root, &self.path, &file)
    }

    fn read_file(&self) -> Result<ProjectOpenDecisionFile, BackendError> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Ok(ProjectOpenDecisionFile::default())
            }
            Err(error) => {
                return Err(file_error(
                    "PROJECT_OPEN_DECISION_READ_FAILED",
                    "Project-open decisions could not be read.",
                    &self.path,
                    error.to_string(),
                ))
            }
        };
        let file = serde_json::from_str::<ProjectOpenDecisionFile>(&contents).map_err(|error| {
            file_error(
                "PROJECT_OPEN_DECISION_CORRUPT",
                "Project-open decisions are corrupt.",
                &self.path,
                error.to_string(),
            )
        })?;
        if file.schema_version != PROJECT_OPEN_DECISION_SCHEMA_VERSION {
            return Err(file_error(
                "PROJECT_OPEN_DECISION_SCHEMA_UNSUPPORTED",
                "Project-open decisions use an unsupported schema version.",
                &self.path,
                file.schema_version.to_string(),
            ));
        }
        Ok(file)
    }
}

fn decision_store_locked() -> BackendError {
    BackendError::new(
        "PROJECT_OPEN_DECISION_LOCKED",
        "Project-open decisions are temporarily unavailable.",
        true,
        true,
    )
}

fn file_error(code: &str, message: &str, path: &Path, detail: String) -> BackendError {
    BackendError::new(code, message, true, true).with_details(serde_json::json!({
        "path": path.to_string_lossy(),
        "error": detail,
    }))
}
