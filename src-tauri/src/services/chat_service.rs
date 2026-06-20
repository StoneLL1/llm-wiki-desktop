use std::path::Path;

use crate::errors::BackendError;
use crate::models::chat::{
    ChatCitation, ChatMessage, ChatRetrievalHit, ChatSession, ChatSessionSummary, SaveAnswerResult,
};
use crate::models::paths::ProjectContext;
use crate::services::file_store::FileStore;
use crate::services::SearchService;
use crate::services::{GitService, WriteMode};
use crate::utils::markdown_utils::slugify_query;
use crate::utils::time_utils::now_rfc3339;

const CHATS_DIR: &str = ".app/chats";
const DEFAULT_TITLE: &str = "New chat";
const RETRIEVAL_LIMIT: usize = 6;
const EXCERPT_CHARS: usize = 1200;
const HISTORY_TURNS: usize = 8;

/// Persists chat sessions as JSON under `.app/chats/{id}.json` and assembles the
/// retrieval context (local SearchService hits + purpose + bounded history) for
/// the model prompt. Citations are the retrieved pages themselves, never parsed
/// from model output.
#[derive(Default)]
pub struct ChatService {
    file_store: FileStore,
}

impl ChatService {
    pub fn create_session(
        &self,
        context: &ProjectContext,
        title: Option<&str>,
    ) -> Result<ChatSession, BackendError> {
        let now = now_rfc3339();
        let session = ChatSession {
            id: uuid::Uuid::new_v4().to_string(),
            title: title
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .unwrap_or(DEFAULT_TITLE)
                .to_string(),
            project_id: context.project_id.clone(),
            created_at: now.clone(),
            updated_at: now,
            messages: Vec::new(),
        };
        self.file_store.write_json_atomic_checked(
            context,
            &session_path(&session.id),
            &session,
            WriteMode::CreateNew,
        )?;
        Ok(session)
    }

    /// Enumerate `.app/chats/*.json`. Corrupt files are logged and skipped —
    /// they must not crash app startup (matches TaskService::recover_tasks).
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
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            match self.file_store.read_json_file::<ChatSession>(&path) {
                Ok(session) => summaries.push(ChatSessionSummary {
                    id: session.id,
                    title: session.title,
                    created_at: session.created_at,
                    updated_at: session.updated_at,
                    message_count: session.messages.len(),
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

    fn save_session(
        &self,
        context: &ProjectContext,
        session: &ChatSession,
    ) -> Result<(), BackendError> {
        self.file_store
            .write_json_atomic(context, &session_path(&session.id), session)
    }

    /// Local retrieval: keyword search → top pages + bounded excerpts, plus
    /// `purpose.md`. Returns both the typed citations (for the UI) and the
    /// single assembled prompt string (for the Agent/BYOK backend). No model is
    /// called here.
    pub fn build_retrieval_context(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        query: &str,
        session: &ChatSession,
    ) -> Result<RetrievalContext, BackendError> {
        let hits = search_service.retrieve_with_excerpts(
            context,
            query,
            RETRIEVAL_LIMIT,
            EXCERPT_CHARS,
        )?;
        let citations: Vec<ChatCitation> = hits
            .iter()
            .map(|hit| ChatCitation {
                page_path: hit.path.clone(),
                title: hit.title.clone(),
                snippet: hit.snippet.clone(),
                score: hit.score,
            })
            .collect();
        let purpose = self.file_store.read_markdown(context, "purpose.md").ok();
        let prompt = self.assemble_prompt(query, session, &hits, purpose.as_deref());
        Ok(RetrievalContext { citations, prompt })
    }

    fn assemble_prompt(
        &self,
        query: &str,
        session: &ChatSession,
        hits: &[ChatRetrievalHit],
        purpose: Option<&str>,
    ) -> String {
        let mut prompt = String::new();
        prompt.push_str(
            "You are answering a question about a local Markdown wiki. Answer using only the \
             provided context. Cite sources by their page path in parentheses. If the context is \
             insufficient, say so explicitly.\n",
        );
        if let Some(purpose) = purpose {
            prompt.push_str("\n--- Wiki purpose ---\n");
            prompt.push_str(purpose.trim());
            prompt.push('\n');
        }
        prompt.push_str("\n--- Sources ---\n");
        for hit in hits {
            prompt.push_str(&format!("\n### {} ({})\n", hit.title, hit.path));
            if let Some(excerpt) = &hit.excerpt {
                prompt.push_str(excerpt.trim());
                prompt.push('\n');
            }
        }
        let history = session
            .messages
            .iter()
            .rev()
            .take(HISTORY_TURNS)
            .collect::<Vec<&ChatMessage>>()
            .into_iter()
            .rev();
        let mut has_history = false;
        for message in history {
            if !has_history {
                prompt.push_str("\n--- Conversation so far ---\n");
                has_history = true;
            }
            let label = match message.role {
                crate::models::chat::ChatRole::User => "User",
                crate::models::chat::ChatRole::Assistant => "Assistant",
            };
            prompt.push_str(&format!("{label}: {}\n", message.content.trim()));
        }
        prompt.push_str("\n--- Latest question ---\n");
        prompt.push_str(query.trim());
        prompt.push('\n');
        prompt
    }

    /// Render an assistant message as a `wiki/queries/` Markdown page with
    /// frontmatter (`type: query`, title, created, sources) and Question /
    /// Answer / Sources sections. Returns `(slug, markdown)`.
    pub fn build_answer_markdown(
        &self,
        session: &ChatSession,
        question: &ChatMessage,
        answer: &ChatMessage,
    ) -> (String, String) {
        let title = format!(
            "Q: {}",
            first_line(&question.content)
                .chars()
                .take(80)
                .collect::<String>()
        );
        let slug = slugify_query(&question.content, &answer.id);
        let sources: Vec<&str> = answer
            .citations
            .iter()
            .map(|citation| citation.page_path.as_str())
            .collect();
        let mut markdown = String::new();
        markdown.push_str("---\n");
        markdown.push_str("type: query\n");
        markdown.push_str(&format!("title: {}\n", yaml_scalar(&title)));
        markdown.push_str(&format!("created: {}\n", answer.created_at));
        markdown.push_str(&format!("session: {}\n", session.id));
        markdown.push_str("sources:\n");
        if sources.is_empty() {
            markdown.push_str("[]\n");
        } else {
            for source in &sources {
                markdown.push_str(&format!("  - {}\n", yaml_scalar(source)));
            }
        }
        markdown.push_str("---\n\n");
        markdown.push_str(&format!("# {}\n\n", title));
        markdown.push_str("## Question\n\n");
        markdown.push_str(question.content.trim());
        markdown.push_str("\n\n## Answer\n\n");
        markdown.push_str(answer.content.trim());
        markdown.push_str("\n\n## Sources\n\n");
        if sources.is_empty() {
            markdown.push_str("_No citations._\n");
        } else {
            for source in &sources {
                markdown.push_str(&format!("- [[{}]]\n", stem_of(source)));
            }
        }
        (slug, markdown)
    }

    /// Save an assistant answer to `wiki/queries/`. New pages write directly
    /// (graph-cache invalidated, log appended). Overwriting an existing page is
    /// never silent: without `allow_overwrite` it surfaces `FILE_ALREADY_EXISTS`
    /// with the current hash; with `allow_overwrite` + a matching `expected_hash`
    /// it creates a scoped Git checkpoint first, then writes.
    #[allow(clippy::too_many_arguments)]
    pub fn save_answer_to_wiki(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        target_path: Option<&str>,
        expected_hash: Option<&str>,
        allow_overwrite: bool,
        markdown: &str,
        fallback_slug: &str,
    ) -> Result<SaveAnswerResult, BackendError> {
        let resolved = match target_path.map(str::trim).filter(|p| !p.is_empty()) {
            Some(custom) => validate_query_path(custom)?,
            None => format!("wiki/queries/{fallback_slug}.md"),
        };

        let absolute = context.resolve_project_path(&resolved)?;
        let exists = absolute.exists();

        let (mode, checkpoint) = if !exists {
            (WriteMode::CreateNew, None)
        } else if !allow_overwrite {
            let current_hash = self.file_store.file_hash(context, &resolved)?;
            return Err(BackendError::new(
                "FILE_ALREADY_EXISTS",
                "A query page already exists at this path. Confirm to overwrite.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "path": resolved,
                "currentHash": current_hash,
            })));
        } else {
            let expected = expected_hash.ok_or_else(|| {
                BackendError::new(
                    "CHAT_OVERWRITE_HASH_MISSING",
                    "Overwriting an existing query page requires its current hash.",
                    true,
                    true,
                )
            })?;
            // The Git checkpoint is the data-safety boundary for an overwrite
            // (CLAUDE.md hard rule). A checkpoint failure must not be swallowed:
            // surface it so the user can resolve git state rather than losing
            // the prior page to an un-checkpointed write.
            let checkpoint = git_service
                .create_scoped_checkpoint(
                    context,
                    crate::models::git::CheckpointPurpose::HighRiskOperation,
                    "Before overwriting saved chat answer",
                    std::slice::from_ref(&resolved),
                )
                .map_err(|err| {
                    BackendError::new(
                        "GIT_CHECKPOINT_FAILED",
                        format!(
                            "Could not create a Git checkpoint before overwriting: {}",
                            err.message
                        ),
                        true,
                        true,
                    )
                    .with_details(serde_json::json!({ "path": resolved }))
                })?
                .commit_hash;
            (
                WriteMode::OverwriteIfHashMatches(expected.to_string()),
                checkpoint,
            )
        };

        self.file_store
            .write_markdown_checked(context, &resolved, markdown, mode)?;
        invalidate_graph_cache(context);
        append_save_log(context, &resolved);
        Ok(SaveAnswerResult {
            path: resolved,
            created: !exists,
            checkpoint,
        })
    }
}

pub struct RetrievalContext {
    pub citations: Vec<ChatCitation>,
    pub prompt: String,
}

fn session_path(id: &str) -> String {
    format!("{CHATS_DIR}/{id}.json")
}

fn validate_query_path(path: &str) -> Result<String, BackendError> {
    // ProjectContext enforces traversal/absolute safety on resolve; here we
    // additionally constrain user-provided targets to the wiki subtree.
    let normalized = path.replace('\\', "/");
    if !normalized.starts_with("wiki/") {
        return Err(BackendError::new(
            "CHAT_QUERY_PATH_INVALID",
            "Saved answers must live under the wiki/ directory.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }
    if !normalized.ends_with(".md") {
        return Err(BackendError::new(
            "CHAT_QUERY_PATH_INVALID",
            "Saved answers must be Markdown (.md) files.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }
    Ok(normalized)
}

fn invalidate_graph_cache(context: &ProjectContext) {
    let path = context.app_dir.join("graph-cache.json");
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
}

fn append_save_log(context: &ProjectContext, relative_path: &str) {
    let log_path = context.wiki_dir.join("log.md");
    if !log_path.exists() {
        return;
    }
    let stamp = chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string();
    let line = format!("- [{}] saved {} · chat\n", stamp, relative_path);
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&log_path) {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}

fn first_line(content: &str) -> String {
    content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn yaml_scalar(value: &str) -> String {
    // Keep frontmatter simple: quote if it contains characters that would break
    // our hand-rolled scalar parser (colon, brackets, leading quote).
    if value.contains(':') || value.contains('[') || value.contains(']') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

fn stem_of(path: &str) -> &str {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::ChatService;
    use crate::models::chat::{ChatMessage, ChatRole};
    use crate::models::paths::ProjectContext;
    use crate::services::GitService;
    use crate::services::SearchService;
    use std::path::PathBuf;

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-chat-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    fn write_file(context: &ProjectContext, rel: &str, body: &str) {
        let path = context.resolve_project_path(rel).unwrap();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
    }

    fn seed_vault(context: &ProjectContext) {
        write_file(
            context,
            "wiki/concepts/react-pattern.md",
            "---\ntitle: ReAct Pattern\ntype: concept\ntags: [reasoning]\n---\n\n# ReAct Pattern\n\nReason then act loop for agents.",
        );
        write_file(
            context,
            "wiki/concepts/agent-memory.md",
            "---\ntitle: Agent Memory\ntype: concept\ntags: [memory]\n---\n\n# Agent Memory\n\nCovers short context windows and RAG.",
        );
        write_file(context, "wiki/index.md", "# Index\n");
        write_file(
            context,
            "purpose.md",
            "# Purpose\n\nThis wiki explains agents.",
        );
    }

    fn user_message(content: &str) -> ChatMessage {
        ChatMessage {
            id: format!("u-{}", content.len()),
            role: ChatRole::User,
            content: content.into(),
            created_at: "2026-06-20T00:00:00Z".into(),
            citations: Vec::new(),
            route: None,
            task_id: None,
        }
    }

    #[test]
    fn chat_session_persists_and_round_trips() {
        let (context, root) = tmp_context("roundtrip");
        let service = ChatService::default();

        let session = service.create_session(&context, Some("My Chat")).unwrap();
        assert_eq!(session.title, "My Chat");
        assert!(context
            .resolve_project_path(&format!(".app/chats/{}.json", session.id))
            .unwrap()
            .exists());

        let loaded = service.load_session(&context, &session.id).unwrap();
        assert_eq!(loaded, session);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_session_defaults_title_and_appends_message() {
        let (context, root) = tmp_context("append");
        let service = ChatService::default();
        let mut session = service.create_session(&context, None).unwrap();
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
        let session = service.create_session(&context, None).unwrap();
        let original_updated = session.updated_at.clone();

        let renamed = service
            .rename_session(&context, &session.id, "Renamed Title")
            .unwrap();
        assert_eq!(renamed.title, "Renamed Title");
        assert_ne!(renamed.updated_at, original_updated);
        let loaded = service.load_session(&context, &session.id).unwrap();
        assert_eq!(loaded.title, "Renamed Title");

        service.delete_session(&context, &session.id).unwrap();
        assert!(service.load_session(&context, &session.id).is_err());
        // Deleting a missing session is idempotent.
        service.delete_session(&context, &session.id).unwrap();

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_rejects_empty_title() {
        let (context, root) = tmp_context("empty-title");
        let service = ChatService::default();
        let session = service.create_session(&context, None).unwrap();
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
        let good = service.create_session(&context, Some("Good")).unwrap();

        // Seed a corrupt session file alongside the good one.
        write_file(&context, ".app/chats/corrupt.json", "{ not valid json");

        let summaries = service.list_sessions(&context).unwrap();
        let ids: Vec<&str> = summaries.iter().map(|s| s.id.as_str()).collect();
        assert!(ids.contains(&good.id.as_str()));
        assert!(!ids.contains(&"corrupt"));
        assert!(
            summaries
                .iter()
                .find(|s| s.id == good.id)
                .unwrap()
                .message_count
                == 0
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

    #[test]
    fn retrieval_context_assembles_citations_purpose_and_bounded_history() {
        let (context, root) = tmp_context("retrieval");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();

        // Seed a session with > HISTORY_TURNS turns; older turns must be dropped.
        let mut session = service.create_session(&context, None).unwrap();
        for i in 0..15 {
            session.messages.push(ChatMessage {
                id: format!("old-{i}"),
                role: if i % 2 == 0 {
                    ChatRole::User
                } else {
                    ChatRole::Assistant
                },
                content: format!("ancient turn number {i}"),
                created_at: "2026-06-01T00:00:00Z".into(),
                citations: Vec::new(),
                route: None,
                task_id: None,
            });
        }

        let ctx = service
            .build_retrieval_context(&context, &search, "react pattern", &session)
            .unwrap();

        // Top citation is the title-matching page (title scores 100).
        assert!(!ctx.citations.is_empty());
        assert_eq!(ctx.citations[0].page_path, "wiki/concepts/react-pattern.md");
        assert!(ctx.prompt.contains("Wiki purpose"));
        assert!(ctx.prompt.contains("This wiki explains agents."));
        assert!(ctx.prompt.contains("ReAct Pattern"));
        assert!(ctx.prompt.contains("Latest question"));
        assert!(ctx.prompt.contains("react pattern"));
        // Only the last HISTORY_TURNS turns appear in the prompt.
        assert!(ctx.prompt.contains("ancient turn number 14"));
        assert!(!ctx.prompt.contains("ancient turn number 0"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_answer_markdown_includes_frontmatter_and_sources() {
        let (context, root) = tmp_context("markdown");
        let service = ChatService::default();
        let session = service.create_session(&context, None).unwrap();
        let question = user_message("What is the ReAct pattern?");
        let answer = ChatMessage {
            id: "a-1".into(),
            role: ChatRole::Assistant,
            content: "It is a reason-then-act loop.".into(),
            created_at: "2026-06-20T00:00:00Z".into(),
            citations: vec![crate::models::chat::ChatCitation {
                page_path: "wiki/concepts/react-pattern.md".into(),
                title: "ReAct Pattern".into(),
                snippet: None,
                score: 100,
            }],
            route: None,
            task_id: None,
        };

        let (slug, markdown) = service.build_answer_markdown(&session, &question, &answer);
        assert!(slug.contains("react"));
        assert!(markdown.starts_with("---\n"));
        assert!(markdown.contains("type: query"));
        assert!(markdown.contains("## Question"));
        assert!(markdown.contains("What is the ReAct pattern?"));
        assert!(markdown.contains("## Answer"));
        assert!(markdown.contains("reason-then-act loop"));
        assert!(markdown.contains("react-pattern"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_answer_creates_new_query_page_with_frontmatter() {
        let (context, root) = tmp_context("save-new");
        seed_vault(&context);
        std::fs::write(context.wiki_dir.join("log.md"), "# Log\n").unwrap();
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        std::fs::write(context.app_dir.join("graph-cache.json"), "{}").unwrap();

        let service = ChatService::default();
        let git = GitService;
        let markdown = "---\ntype: query\n---\n\n# Q\n\nAnswer.";

        let result = service
            .save_answer_to_wiki(&context, &git, None, None, false, markdown, "my-query")
            .unwrap();
        assert!(result.created);
        assert_eq!(result.path, "wiki/queries/my-query.md");
        assert!(result.checkpoint.is_none());
        assert!(context
            .resolve_project_path("wiki/queries/my-query.md")
            .unwrap()
            .exists());
        // graph cache invalidated, log appended
        assert!(!context.app_dir.join("graph-cache.json").exists());
        let log = std::fs::read_to_string(context.wiki_dir.join("log.md")).unwrap();
        assert!(log.contains("wiki/queries/my-query.md"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_answer_overwrite_requires_allow_flag_then_hash_matches() {
        let (context, root) = tmp_context("save-overwrite");
        seed_vault(&context);
        // Initialize a git repo so scoped checkpoints can run.
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let service = ChatService::default();
        let markdown_v1 = "---\ntype: query\n---\n\n# Q\n\nFirst.";
        let markdown_v2 = "---\ntype: query\n---\n\n# Q\n\nSecond.";

        // Create the page first.
        service
            .save_answer_to_wiki(&context, &git, None, None, false, markdown_v1, "q")
            .unwrap();

        // Without allow_overwrite → FILE_ALREADY_EXISTS with currentHash.
        let err = service
            .save_answer_to_wiki(&context, &git, None, None, false, markdown_v2, "q")
            .expect_err("overwrite must require allow_overwrite");
        assert_eq!(err.code, "FILE_ALREADY_EXISTS");
        let current_hash = err.details.as_ref().unwrap()["currentHash"]
            .as_str()
            .unwrap()
            .to_string();

        // With allow_overwrite but stale hash → FILE_HASH_MISMATCH.
        let err = service
            .save_answer_to_wiki(
                &context,
                &git,
                None,
                Some("stale-hash"),
                true,
                markdown_v2,
                "q",
            )
            .expect_err("stale hash must be rejected");
        assert_eq!(err.code, "FILE_HASH_MISMATCH");

        // With allow_overwrite + matching hash → checkpoint + overwrite.
        let result = service
            .save_answer_to_wiki(
                &context,
                &git,
                None,
                Some(&current_hash),
                true,
                markdown_v2,
                "q",
            )
            .unwrap();
        assert!(!result.created);
        assert!(result.checkpoint.is_some());
        let on_disk =
            std::fs::read_to_string(context.resolve_project_path("wiki/queries/q.md").unwrap())
                .unwrap();
        assert!(on_disk.contains("Second."));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_answer_rejects_path_outside_wiki() {
        let (context, root) = tmp_context("save-path");
        let service = ChatService::default();
        let git = GitService;
        let err = service
            .save_answer_to_wiki(
                &context,
                &git,
                Some("raw/sources/x.md"),
                None,
                false,
                "body",
                "q",
            )
            .expect_err("non-wiki path must be rejected");
        assert_eq!(err.code, "CHAT_QUERY_PATH_INVALID");
        std::fs::remove_dir_all(root).unwrap();
    }
}
