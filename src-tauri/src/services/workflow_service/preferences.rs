use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock, Weak};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::models::workflow::{
    WorkflowKind, WorkflowPersistenceMode, WorkflowRoute, WorkflowScope,
};
use crate::services::FileStore;

const PREFERENCES_SCHEMA_VERSION: u32 = 1;
const PREFERENCES_FILE_NAME: &str = "preferences.json";
static PREFERENCE_OPERATION_LOCKS: OnceLock<Mutex<HashMap<String, Weak<Mutex<()>>>>> =
    OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowPreference {
    pub kind: WorkflowKind,
    pub scope: WorkflowScope,
    pub route: Option<WorkflowRoute>,
    pub baseline_fingerprint: String,
    pub preparation_fingerprint: String,
    pub saved_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowPreferencesFile {
    schema_version: u32,
    entries: Vec<WorkflowPreference>,
}

impl Default for WorkflowPreferencesFile {
    fn default() -> Self {
        Self {
            schema_version: PREFERENCES_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct WorkflowPreferences {
    memory: RwLock<HashMap<String, Vec<WorkflowPreference>>>,
}

impl WorkflowPreferences {
    pub fn load(
        &self,
        context: &ProjectContext,
        identity_key: &str,
        identity_revision: &str,
        persistence: &WorkflowPersistenceMode,
    ) -> Result<Vec<WorkflowPreference>, BackendError> {
        let owner = owner_key(identity_key, identity_revision);
        let operation_lock = self.operation_lock(&owner)?;
        let _operation = operation_lock
            .lock()
            .map_err(|_| preferences_lock_error())?;
        self.load_unlocked(context, &owner, persistence)
    }

    fn load_unlocked(
        &self,
        context: &ProjectContext,
        owner: &str,
        persistence: &WorkflowPersistenceMode,
    ) -> Result<Vec<WorkflowPreference>, BackendError> {
        let Some(path) = preferences_path(context, persistence) else {
            return Ok(self
                .memory
                .read()
                .map_err(|_| preferences_lock_error())?
                .get(owner)
                .cloned()
                .unwrap_or_default());
        };
        if !context.resolve_project_path(&path)?.is_file() {
            return Ok(Vec::new());
        }
        let value: WorkflowPreferencesFile = FileStore.read_json(context, &path)?;
        if value.schema_version != PREFERENCES_SCHEMA_VERSION {
            return Err(BackendError::new(
                "WORKFLOW_PREFERENCES_VERSION_UNSUPPORTED",
                "Workflow preferences use an unsupported schema version.",
                true,
                true,
            ));
        }
        validate_entries(&value.entries)?;
        Ok(value.entries)
    }

    pub fn remember(
        &self,
        context: &ProjectContext,
        identity_key: &str,
        identity_revision: &str,
        persistence: &WorkflowPersistenceMode,
        mut preference: WorkflowPreference,
    ) -> Result<(), BackendError> {
        validate_preference(&preference)?;
        let owner = owner_key(identity_key, identity_revision);
        let operation_lock = self.operation_lock(&owner)?;
        let _operation = operation_lock
            .lock()
            .map_err(|_| preferences_lock_error())?;
        preference.saved_at = Utc::now().to_rfc3339();
        let mut entries = self.load_unlocked(context, &owner, persistence)?;
        entries.retain(|entry| entry.kind != preference.kind);
        entries.push(preference);
        entries.sort_by_key(|entry| workflow_order(&entry.kind));

        let Some(path) = preferences_path(context, persistence) else {
            self.memory
                .write()
                .map_err(|_| preferences_lock_error())?
                .insert(owner, entries);
            return Ok(());
        };
        FileStore.write_json_atomic(
            context,
            &path,
            &WorkflowPreferencesFile {
                schema_version: PREFERENCES_SCHEMA_VERSION,
                entries,
            },
        )
    }

    fn operation_lock(&self, owner: &str) -> Result<Arc<Mutex<()>>, BackendError> {
        let mut locks = PREFERENCE_OPERATION_LOCKS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .map_err(|_| preferences_lock_error())?;
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(owner).and_then(Weak::upgrade) {
            return Ok(lock);
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(owner.to_string(), Arc::downgrade(&lock));
        Ok(lock)
    }
}

fn preferences_path(
    context: &ProjectContext,
    persistence: &WorkflowPersistenceMode,
) -> Option<String> {
    if matches!(persistence, WorkflowPersistenceMode::MemoryOnly) {
        return None;
    }
    context
        .layout
        .workflow_state_root
        .as_deref()
        .map(|root| format!("{}/{PREFERENCES_FILE_NAME}", root.trim_end_matches('/')))
}

fn validate_entries(entries: &[WorkflowPreference]) -> Result<(), BackendError> {
    for entry in entries {
        validate_preference(entry)?;
    }
    Ok(())
}

fn validate_preference(entry: &WorkflowPreference) -> Result<(), BackendError> {
    if entry.baseline_fingerprint.len() != 64
        || entry.preparation_fingerprint.len() != 64
        || !entry
            .baseline_fingerprint
            .bytes()
            .chain(entry.preparation_fingerprint.bytes())
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(BackendError::new(
            "WORKFLOW_PREFERENCES_INVALID",
            "Workflow preferences contain an invalid backend fingerprint.",
            true,
            true,
        ));
    }
    Ok(())
}

fn owner_key(identity_key: &str, identity_revision: &str) -> String {
    format!("{identity_key}:{identity_revision}")
}

fn workflow_order(kind: &WorkflowKind) -> u8 {
    match kind {
        WorkflowKind::UpdateWiki => 0,
        WorkflowKind::HealthCheck => 1,
        WorkflowKind::GenerateContent => 2,
    }
}

fn preferences_lock_error() -> BackendError {
    BackendError::new(
        "WORKFLOW_PREFERENCES_LOCKED",
        "Workflow preferences are temporarily unavailable.",
        true,
        false,
    )
}
