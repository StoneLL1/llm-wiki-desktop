mod citations;
mod retrieval;
mod saved_answers;
mod sessions;

#[cfg(test)]
mod test_support;

use crate::models::chat::{ChatCitation, ChatRetrievalDiagnostics, ChatSourceRef};
use crate::services::file_store::FileStore;

/// Persists chat sessions as JSON under `.app/chats/{id}.json`, assembles
/// bounded local retrieval context, and saves model answers as Markdown wiki
/// query pages. The focused modules implement those use cases while this type
/// remains the stable command/AppState facade.
#[derive(Default)]
pub struct ChatService {
    pub(super) file_store: FileStore,
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
