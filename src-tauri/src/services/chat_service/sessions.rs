use super::ChatService;
use crate::errors::BackendError;
use crate::models::chat::{ChatMessage, ChatSession, ChatSessionSummary};
use crate::models::paths::ProjectContext;
use crate::services::WriteMode;
use crate::utils::time_utils::now_rfc3339;

const CHATS_DIR: &str = ".app/chats";
const DEFAULT_TITLE: &str = "New chat";

impl ChatService {
    pub fn create_session(
        &self,
        context: &ProjectContext,
        title: Option<&str>,
        context_page_path: Option<&str>,
    ) -> Result<ChatSession, BackendError> {
        let now = now_rfc3339();
        let normalized_page_path = context_page_path
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(|path| validate_context_page_path(context, path))
            .transpose()?;
        let session = ChatSession {
            id: uuid::Uuid::new_v4().to_string(),
            title: title
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .unwrap_or(DEFAULT_TITLE)
                .to_string(),
            project_id: context.project_id.clone(),
            created_at: now.clone(),
            updated_at: now,
            messages: Vec::new(),
            context_page_path: normalized_page_path,
        };
        self.file_store.write_json_atomic_checked(
            context,
            &session_path(&session.id),
            &session,
            WriteMode::CreateNew,
        )?;
        Ok(session)
    }

    /// Enumerate `.app/chats/*.json`; corrupt files are skipped so one damaged
    /// session cannot prevent the remaining history from loading.
    pub fn list_sessions(
        &self,
        context: &ProjectContext,
    ) -> Result<Vec<ChatSessionSummary>, BackendError> {
        let dir = context.resolve_project_path(CHATS_DIR)?;
        let mut summaries = Vec::new();
        if !dir.exists() {
            return Ok(summaries);
        }
        let entries = std::fs::read_dir(&dir).map_err(|err| {
            BackendError::new("CHAT_LIST_FAILED", err.to_string(), true, false)
                .with_details(serde_json::json!({ "path": dir.to_string_lossy() }))
        })?;
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            match self.file_store.read_json_file::<ChatSession>(&path) {
                Ok(session) => summaries.push(ChatSessionSummary {
                    id: session.id,
                    title: session.title,
                    created_at: session.created_at,
                    updated_at: session.updated_at,
                    message_count: session.messages.len(),
                    context_page_path: session.context_page_path,
                }),
                Err(err) => {
                    eprintln!(
                        "Skipping corrupt chat session {}: {}",
                        path.display(),
                        err.message
                    );
                }
            }
        }
        summaries.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(summaries)
    }

    pub fn load_session(
        &self,
        context: &ProjectContext,
        session_id: &str,
    ) -> Result<ChatSession, BackendError> {
        let path = session_path(session_id);
        self.file_store.read_json(context, &path).map_err(|err| {
            BackendError::new(
                "CHAT_PARSE_FAILED",
                if err.code == "JSON_PARSE_FAILED" {
                    "Chat session file is corrupt.".to_string()
                } else {
                    err.message
                },
                true,
                false,
            )
            .with_details(serde_json::json!({ "sessionId": session_id, "path": path }))
        })
    }

    pub fn rename_session(
        &self,
        context: &ProjectContext,
        session_id: &str,
        title: &str,
    ) -> Result<ChatSession, BackendError> {
        let mut session = self.load_session(context, session_id)?;
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return Err(BackendError::new(
                "CHAT_TITLE_EMPTY",
                "Chat session title cannot be empty.",
                true,
                true,
            ));
        }
        session.title = trimmed.to_string();
        session.updated_at = now_rfc3339();
        self.save_session(context, &session)?;
        Ok(session)
    }

    pub fn delete_session(
        &self,
        context: &ProjectContext,
        session_id: &str,
    ) -> Result<(), BackendError> {
        let path = context.resolve_project_path(&session_path(session_id))?;
        if path.exists() {
            std::fs::remove_file(&path).map_err(|err| {
                BackendError::new("CHAT_DELETE_FAILED", err.to_string(), true, false)
                    .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
            })?;
        }
        Ok(())
    }

    pub fn append_message(
        &self,
        context: &ProjectContext,
        session: &mut ChatSession,
        message: ChatMessage,
    ) -> Result<(), BackendError> {
        session.messages.push(message);
        session.updated_at = now_rfc3339();
        self.save_session(context, session)
    }

    pub fn save_session(
        &self,
        context: &ProjectContext,
        session: &ChatSession,
    ) -> Result<(), BackendError> {
        self.file_store
            .write_json_atomic(context, &session_path(&session.id), session)
    }
}

fn session_path(id: &str) -> String {
    format!("{CHATS_DIR}/{id}.json")
}

fn validate_context_page_path(
    context: &ProjectContext,
    path: &str,
) -> Result<String, BackendError> {
    let normalized = path.replace('\\', "/");
    let absolute = context.resolve_project_path(&normalized)?;
    if absolute.strip_prefix(&context.wiki_dir).is_err() || !normalized.starts_with("wiki/") {
        return Err(BackendError::new(
            "CHAT_CONTEXT_PAGE_INVALID",
            "Page-scoped chat sessions must reference a page under the wiki/ directory.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }
    if !normalized.ends_with(".md") {
        return Err(BackendError::new(
            "CHAT_CONTEXT_PAGE_INVALID",
            "Page-scoped chat sessions must reference a Markdown (.md) page.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{tmp_context, user_message, write_file};
    use super::ChatService;

    #[test]
    fn chat_session_persists_and_round_trips() {
        let (context, root) = tmp_context("roundtrip");
        let service = ChatService::default();
        let session = service
            .create_session(&context, Some("My Chat"), None)
            .unwrap();
        assert_eq!(session.title, "My Chat");
        assert!(context
            .resolve_project_path(&format!(".app/chats/{}.json", session.id))
            .unwrap()
            .exists());
        assert_eq!(
            service.load_session(&context, &session.id).unwrap(),
            session
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_session_normalizes_and_validates_context_page_path() {
        let (context, root) = tmp_context("context-page");
        let service = ChatService::default();
        let session = service
            .create_session(
                &context,
                Some("Page Chat"),
                Some("wiki\\concepts\\react-pattern.md"),
            )
            .unwrap();
        assert_eq!(
            session.context_page_path.as_deref(),
            Some("wiki/concepts/react-pattern.md")
        );
        for invalid in [
            "../wiki/concepts/a.md",
            "/wiki/concepts/a.md",
            "raw/sources/a.md",
            "wiki/concepts/a.txt",
        ] {
            let err = service
                .create_session(&context, Some("Bad"), Some(invalid))
                .expect_err("invalid page-scoped chat metadata must be rejected");
            assert!(
                err.code == "CHAT_CONTEXT_PAGE_INVALID" || err.code.starts_with("PATH_"),
                "unexpected error code for {invalid}: {}",
                err.code
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_session_defaults_title_and_appends_message() {
        let (context, root) = tmp_context("append");
        let service = ChatService::default();
        let mut session = service.create_session(&context, None, None).unwrap();
        assert_eq!(session.title, "New chat");
        service
            .append_message(&context, &mut session, user_message("hello"))
            .unwrap();
        let reloaded = service.load_session(&context, &session.id).unwrap();
        assert_eq!(reloaded.messages.len(), 1);
        assert_eq!(reloaded.messages[0].content, "hello");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_and_delete_session_update_disk() {
        let (context, root) = tmp_context("rename");
        let service = ChatService::default();
        let session = service.create_session(&context, None, None).unwrap();
        let original_updated = session.updated_at.clone();
        let renamed = service
            .rename_session(&context, &session.id, "Renamed Title")
            .unwrap();
        assert_eq!(renamed.title, "Renamed Title");
        assert_ne!(renamed.updated_at, original_updated);
        assert_eq!(
            service.load_session(&context, &session.id).unwrap().title,
            "Renamed Title"
        );
        service.delete_session(&context, &session.id).unwrap();
        assert!(service.load_session(&context, &session.id).is_err());
        service.delete_session(&context, &session.id).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_rejects_empty_title() {
        let (context, root) = tmp_context("empty-title");
        let service = ChatService::default();
        let session = service.create_session(&context, None, None).unwrap();
        let err = service
            .rename_session(&context, &session.id, "   ")
            .expect_err("empty title must be rejected");
        assert_eq!(err.code, "CHAT_TITLE_EMPTY");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn list_sessions_skips_corrupt_files_without_panicking() {
        let (context, root) = tmp_context("list-corrupt");
        let service = ChatService::default();
        let good = service
            .create_session(&context, Some("Good"), None)
            .unwrap();
        write_file(&context, ".app/chats/corrupt.json", "{ not valid json");
        let summaries = service.list_sessions(&context).unwrap();
        assert!(summaries.iter().any(|summary| summary.id == good.id));
        assert!(!summaries.iter().any(|summary| summary.id == "corrupt"));
        assert_eq!(
            summaries
                .iter()
                .find(|summary| summary.id == good.id)
                .unwrap()
                .message_count,
            0
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn load_corrupt_session_returns_recoverable_error() {
        let (context, root) = tmp_context("load-corrupt");
        let service = ChatService::default();
        write_file(&context, ".app/chats/broken.json", "{ broken");
        let err = service
            .load_session(&context, "broken")
            .expect_err("corrupt session must error, not panic");
        assert_eq!(err.code, "CHAT_PARSE_FAILED");
        assert!(err.recoverable);
        std::fs::remove_dir_all(root).unwrap();
    }
}
