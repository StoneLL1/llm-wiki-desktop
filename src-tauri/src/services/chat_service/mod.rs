mod citations;
mod retrieval;
mod saved_answers;
mod sessions;

#[cfg(test)]
mod test_support;

use crate::models::chat::{ChatCitation, ChatRetrievalDiagnostics, ChatSourceRef};
use crate::services::file_store::FileStore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// Persists chat sessions as JSON under `.app/chats/{id}.json`, assembles
/// bounded local retrieval context, and saves model answers as Markdown wiki
/// query pages. The focused modules implement those use cases while this type
/// remains the stable command/AppState facade.
pub struct ChatService {
    pub(super) file_store: FileStore,
    /// Serializes each session's read/modify/write mutation inside this
    /// process. Atomic file replacement protects readers from partial JSON,
    /// but cannot prevent two sends from overwriting each other's messages.
    pub(super) session_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Chat currently exposes one global streaming slot in the UI. Enforce
    /// the same invariant at the command boundary so two windows/projects
    /// cannot start overlapping runs before the first task id is observed.
    pub(super) send_gate: Arc<AsyncMutex<()>>,
}

impl Default for ChatService {
    fn default() -> Self {
        Self {
            file_store: FileStore::default(),
            session_locks: Mutex::new(HashMap::new()),
            send_gate: Arc::new(AsyncMutex::new(())),
        }
    }
}

#[derive(Debug)]
pub struct RetrievalContext {
    pub source_refs: Vec<ChatSourceRef>,
    pub diagnostics: ChatRetrievalDiagnostics,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedModelCitations {
    pub citations: Vec<ChatCitation>,
    pub invalid_source_ids: Vec<String>,
    pub has_unverified: bool,
}

impl ChatService {
    pub fn file_hash_if_exists(
        &self,
        context: &crate::models::paths::ProjectContext,
        relative_path: &str,
    ) -> Result<Option<String>, crate::errors::BackendError> {
        self.file_store.file_hash_if_exists(context, relative_path)
    }

    pub fn try_acquire_send(&self) -> Result<OwnedMutexGuard<()>, crate::errors::BackendError> {
        self.send_gate.clone().try_lock_owned().map_err(|_| {
            crate::errors::BackendError::new(
                "CHAT_BUSY",
                "Another Chat answer is already being generated.",
                true,
                true,
            )
        })
    }
}
