use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::errors::{
    BackendError, IMPORT_V2_ITEM_NOT_FOUND, IMPORT_V2_SESSION_INVALID, IMPORT_V2_SESSION_NOT_FOUND,
    IMPORT_V2_STATE_INVALID,
};
use crate::models::import_v2::{
    ImportInput, ImportItem, ImportResourceMode, ImportSession, ImportSessionStatus,
    IMPORT_V2_SCHEMA_VERSION,
};
use crate::models::paths::ProjectContext;
use crate::services::FileStore;

#[derive(Default)]
pub struct SessionStore;

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

fn session_root(session_id: &str) -> String {
    format!(".app/import-sessions/{session_id}")
}

impl SessionStore {
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
        let root = session_root(session_id);
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
        let root = session_root(&session.session_id);
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
        session.items.extend(
            inputs
                .into_iter()
                .map(|input| ImportItem::queued(&uuid::Uuid::new_v4().to_string(), input)),
        );
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.save(context, file_store, &session)?;
        Ok(session)
    }

    pub fn update_item(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        session_id: &str,
        item: ImportItem,
    ) -> Result<ImportSession, BackendError> {
        validate_id(&item.item_id)?;
        let mut session = self.load(context, file_store, session_id)?;
        let existing = session
            .items
            .iter_mut()
            .find(|candidate| candidate.item_id == item.item_id)
            .ok_or_else(|| {
                BackendError::new(
                    IMPORT_V2_ITEM_NOT_FOUND,
                    "Import session item was not found.",
                    true,
                    false,
                )
            })?;
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
        *existing = item;
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.save(context, file_store, &session)?;
        Ok(session)
    }
}

#[cfg(test)]
mod tests {
    use crate::errors::IMPORT_V2_SESSION_INVALID;
    use crate::models::import_v2::ImportResourceMode;
    use crate::services::import_v2::test_support::{test_context, test_file_input};
    use crate::services::FileStore;

    use super::SessionStore;

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
}
