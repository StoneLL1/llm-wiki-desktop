use super::ChatService;
use crate::errors::BackendError;
use crate::models::chat::{ChatMessage, ChatSession, ChatSessionSummary};
use crate::models::paths::ProjectContext;
use crate::services::WriteMode;
use crate::utils::safe_project_dir::remove_project_file;
use crate::utils::time_utils::now_rfc3339;
use std::sync::{Arc, Mutex};

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
            &session_path(context, &session.id)?,
            &session,
            WriteMode::CreateNew,
        )?;
        Ok(session)
    }

    /// Enumerate the layout-owned chat state directory; corrupt files are skipped
    /// so one damaged session cannot prevent the remaining history from loading.
    pub fn list_sessions(
        &self,
        context: &ProjectContext,
    ) -> Result<Vec<ChatSessionSummary>, BackendError> {
        let dir = context.resolve_project_path(chat_state_root(context)?)?;
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
        self.load_session_unlocked(context, session_id)
    }

    fn load_session_unlocked(
        &self,
        context: &ProjectContext,
        session_id: &str,
    ) -> Result<ChatSession, BackendError> {
        let path = session_path(context, session_id)?;
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
        let lock = self.session_lock(context, session_id);
        let _guard = lock.lock().map_err(|_| session_lock_error())?;
        let mut session = self.load_session_unlocked(context, session_id)?;
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
        self.save_session_unlocked(context, &session)?;
        Ok(session)
    }

    pub fn delete_session(
        &self,
        context: &ProjectContext,
        session_id: &str,
    ) -> Result<(), BackendError> {
        let lock = self.session_lock(context, session_id);
        let _guard = lock.lock().map_err(|_| session_lock_error())?;
        let path = context.resolve_project_write_path(&session_path(context, session_id)?)?;
        if path.exists() {
            remove_project_file(&context.root, &path).map_err(|err| {
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
        self.append_message_if(context, session, message, || false)
            .map(|_| ())
    }

    /// Append a message only if the caller's predicate is still false while
    /// holding the session mutation lock. This closes the cancellation race
    /// where a worker checks task state, waits for another writer, and then
    /// persists an answer after the user has cancelled it.
    pub fn append_message_if<F>(
        &self,
        context: &ProjectContext,
        session: &mut ChatSession,
        message: ChatMessage,
        should_abort: F,
    ) -> Result<bool, BackendError>
    where
        F: FnOnce() -> bool,
    {
        let lock = self.session_lock(context, &session.id);
        let _guard = lock.lock().map_err(|_| session_lock_error())?;
        // Always merge into the latest on-disk snapshot. The caller may have
        // loaded its session before another send/rename completed; writing
        // that stale snapshot would silently discard the other turn/title.
        let mut latest = self.load_session_unlocked(context, &session.id)?;
        if should_abort() {
            *session = latest;
            return Ok(false);
        }
        latest.messages.push(message);
        latest.updated_at = now_rfc3339();
        self.save_session_unlocked(context, &latest)?;
        *session = latest;
        Ok(true)
    }

    pub fn mark_answer_saved(
        &self,
        context: &ProjectContext,
        session_id: &str,
        message_id: &str,
        path: &str,
    ) -> Result<ChatSession, BackendError> {
        let lock = self.session_lock(context, session_id);
        let _guard = lock.lock().map_err(|_| session_lock_error())?;
        let mut session = self.load_session_unlocked(context, session_id)?;
        let message = session
            .messages
            .iter_mut()
            .find(|message| message.id == message_id)
            .ok_or_else(|| {
                BackendError::new(
                    "CHAT_MESSAGE_NOT_FOUND",
                    "The selected answer message no longer exists.",
                    true,
                    true,
                )
            })?;
        message.saved_path = Some(path.to_string());
        session.updated_at = now_rfc3339();
        self.file_store.write_json_atomic(
            context,
            &session_path(context, &session.id)?,
            &session,
        )?;
        Ok(session)
    }

    /// Remove a just-written message when its task failed before the terminal
    /// task result could be committed. The message id and task id are checked
    /// under the same per-session lock used by appends, so a cancelled answer
    /// cannot remain as a durable assistant turn after its task is cancelled.
    pub fn remove_message_if<F>(
        &self,
        context: &ProjectContext,
        session_id: &str,
        message_id: &str,
        task_id: &str,
        should_remove: F,
    ) -> Result<bool, BackendError>
    where
        F: FnOnce() -> bool,
    {
        let lock = self.session_lock(context, session_id);
        let _guard = lock.lock().map_err(|_| session_lock_error())?;
        let mut session = self.load_session_unlocked(context, session_id)?;
        let Some(index) = session.messages.iter().position(|message| {
            message.id == message_id && message.task_id.as_deref() == Some(task_id)
        }) else {
            return Ok(false);
        };
        if !should_remove() {
            return Ok(false);
        }
        session.messages.remove(index);
        session.updated_at = now_rfc3339();
        self.save_session_unlocked(context, &session)?;
        Ok(true)
    }

    pub fn save_session(
        &self,
        context: &ProjectContext,
        session: &ChatSession,
    ) -> Result<(), BackendError> {
        let lock = self.session_lock(context, &session.id);
        let _guard = lock.lock().map_err(|_| session_lock_error())?;
        self.save_session_unlocked(context, session)
    }

    fn save_session_unlocked(
        &self,
        context: &ProjectContext,
        session: &ChatSession,
    ) -> Result<(), BackendError> {
        let session = match self.load_session_unlocked(context, &session.id) {
            Ok(mut latest) => {
                // Rename/confirmation commands may have loaded their snapshot
                // before a background send appended a new turn. Merge the
                // intentional metadata/message edits instead of replacing the
                // newer session wholesale.
                let message_mutation = session.messages.iter().any(|incoming| {
                    latest
                        .messages
                        .iter()
                        .find(|message| message.id == incoming.id)
                        .map(|existing| incoming.convenience_edit != existing.convenience_edit)
                        .unwrap_or(true)
                });
                if !message_mutation {
                    latest.title = session.title.clone();
                }
                if session.updated_at > latest.updated_at {
                    latest.updated_at = session.updated_at.clone();
                }
                for incoming in &session.messages {
                    if let Some(existing) = latest
                        .messages
                        .iter_mut()
                        .find(|message| message.id == incoming.id)
                    {
                        // The only in-place session mutation callers use here
                        // is convenience-edit resolution. Preserve newer
                        // answer fields (saved path, citations, diagnostics)
                        // written by a concurrent task.
                        if incoming.convenience_edit != existing.convenience_edit {
                            existing.convenience_edit = incoming.convenience_edit.clone();
                        }
                    } else {
                        latest.messages.push(incoming.clone());
                    }
                }
                latest
            }
            Err(error) => return Err(error),
        };
        self.file_store
            .write_json_atomic(context, &session_path(context, &session.id)?, &session)
    }

    fn session_lock(&self, context: &ProjectContext, session_id: &str) -> Arc<Mutex<()>> {
        let key = format!("{}::{session_id}", context.root.to_string_lossy());
        let mut locks = self
            .session_locks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locks
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

fn session_lock_error() -> BackendError {
    BackendError::new(
        "CHAT_SESSION_LOCK_FAILED",
        "Chat session is unavailable because its mutation lock was poisoned.",
        true,
        false,
    )
}

fn chat_state_root(context: &ProjectContext) -> Result<&str, BackendError> {
    context.layout.chat_state_root.as_deref().ok_or_else(|| {
        BackendError::new(
            "PROJECT_LAYOUT_STATE_UNAVAILABLE",
            "Project chat state is unavailable until compatible features are enabled.",
            true,
            true,
        )
    })
}

fn session_path(context: &ProjectContext, id: &str) -> Result<String, BackendError> {
    Ok(format!("{}/{}.json", chat_state_root(context)?, id))
}

fn validate_context_page_path(
    context: &ProjectContext,
    path: &str,
) -> Result<String, BackendError> {
    let normalized = path.replace('\\', "/");
    let absolute = context.resolve_project_path(&normalized)?;
    let is_wiki_page = context
        .layout
        .markdown_roots
        .iter()
        .filter(|root| root.role == crate::models::layout::ProjectMarkdownRootRole::Wiki)
        .any(|root| {
            let Ok(wiki_root) = context.resolve_project_path(&root.path) else {
                return false;
            };
            absolute.strip_prefix(wiki_root).is_ok()
        });
    if !is_wiki_page {
        return Err(BackendError::new(
            "CHAT_CONTEXT_PAGE_INVALID",
            "Page-scoped chat sessions must reference a page under a configured wiki directory.",
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
    use std::sync::Arc;

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
    fn concurrent_appends_merge_into_the_latest_session_snapshot() {
        let (context, root) = tmp_context("concurrent-appends");
        let service = Arc::new(ChatService::default());
        let session = service.create_session(&context, None, None).unwrap();

        std::thread::scope(|scope| {
            for content in ["first concurrent turn", "second concurrent turn"] {
                let service = Arc::clone(&service);
                let context = context.clone();
                let mut stale_session = session.clone();
                scope.spawn(move || {
                    service
                        .append_message(&context, &mut stale_session, user_message(content))
                        .unwrap();
                });
            }
        });

        let loaded = service.load_session(&context, &session.id).unwrap();
        let contents: Vec<_> = loaded
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect();
        assert_eq!(contents.len(), 2);
        assert!(contents.contains(&"first concurrent turn"));
        assert!(contents.contains(&"second concurrent turn"));
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
    fn delete_session_rejects_a_linked_chat_state_root() {
        let (context, root) = tmp_context("delete-linked-state");
        let source_root = root.join("raw/sources");
        std::fs::create_dir_all(&source_root).unwrap();
        let original = source_root.join("original.json");
        std::fs::write(&original, "source bytes").unwrap();
        let chats = root.join(".app/chats");
        std::fs::create_dir_all(chats.parent().unwrap()).unwrap();
        create_directory_link(&source_root, &chats).unwrap();

        let error = ChatService::default()
            .delete_session(&context, "original")
            .expect_err("a linked chat state root must not become a delete target");
        assert_eq!(error.code, "PATH_OUTSIDE_PROJECT");
        assert_eq!(std::fs::read_to_string(&original).unwrap(), "source bytes");

        remove_directory_link(&chats);
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

    #[cfg(unix)]
    fn create_directory_link(
        target: &std::path::Path,
        link: &std::path::Path,
    ) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_directory_link(
        target: &std::path::Path,
        link: &std::path::Path,
    ) -> std::io::Result<()> {
        let quote_path = |path: &std::path::Path| {
            format!(r#"'{}'"#, path.display().to_string().replace('\'', "''"))
        };
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg(format!(
                "New-Item -ItemType Junction -Path {} -Target {} | Out-Null",
                quote_path(link),
                quote_path(target)
            ))
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(
                String::from_utf8_lossy(&output.stderr).into_owned(),
            ))
        }
    }

    fn remove_directory_link(link: &std::path::Path) {
        let _ = std::fs::remove_dir(link);
    }
}
