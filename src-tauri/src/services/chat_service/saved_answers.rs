use std::path::Path;

use super::ChatService;
use crate::errors::BackendError;
use crate::models::chat::{ChatMessage, ChatSession, SaveAnswerResult};
use crate::models::paths::ProjectContext;
use crate::services::{GitService, WriteMode};
use crate::utils::markdown_utils::slugify_query;
use crate::utils::safe_project_dir::remove_project_file;

impl ChatService {
    /// Render an assistant message as a `wiki/queries/` Markdown page. Sources
    /// are taken only from citations already parsed from actual model markers,
    /// never from retrieval diagnostics alone.
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
    ///
    /// This is ordinary saved-answer persistence. It is intentionally separate
    /// from the Agent convenience-edit audit and does not depend on
    /// `ChatConvenienceService`.
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

        let absolute = context.resolve_wiki_write_path(&resolved)?;
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
            // Reject a stale editor token before Git checkpoint preflight can
            // obscure the actionable conflict. The checked write below repeats
            // this comparison after the checkpoint and retains the final
            // identity-and-hash CAS against external edits.
            self.file_store
                .preflight_markdown_overwrite_hash(context, &resolved, expected)?;
            // The Git checkpoint is the data-safety boundary for an overwrite.
            // A checkpoint failure must stop the write rather than silently
            // replacing a user-visible query page without recovery history.
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

        // The checkpoint can take time; revalidate the semantic write root
        // immediately before the mutation so a linked wiki descendant cannot
        // become a write target between the initial preflight and this write.
        context.resolve_wiki_write_path(&resolved)?;
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

fn first_line(content: &str) -> String {
    content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn yaml_scalar(value: &str) -> String {
    if value.contains(':') || value.contains('[') || value.contains(']') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

fn stem_of(path: &str) -> &str {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(path)
}

fn validate_query_path(path: &str) -> Result<String, BackendError> {
    // ProjectContext enforces traversal/absolute safety on resolve; this
    // additional use-case rule preserves the existing wiki-Markdown boundary.
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
    let Ok(path) = context.resolve_project_write_path(".app/graph-cache.json") else {
        return;
    };
    if path.exists() {
        let _ = remove_project_file(&context.root, &path);
    }
}

fn append_save_log(context: &ProjectContext, relative_path: &str) {
    let Ok(log_path) = context.resolve_wiki_write_path("wiki/log.md") else {
        return;
    };
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

#[cfg(test)]
mod tests {
    use super::super::test_support::{seed_vault, source_ref, tmp_context, user_message};
    use super::ChatService;
    use crate::models::chat::{ChatCitation, ChatMessage, ChatRole};
    use crate::services::GitService;

    #[test]
    fn build_answer_markdown_includes_frontmatter_and_sources() {
        let (context, root) = tmp_context("markdown");
        let service = ChatService::default();
        let session = service.create_session(&context, None, None).unwrap();
        let question = user_message("What is the ReAct pattern?");
        let answer = ChatMessage {
            id: "a-1".into(),
            role: ChatRole::Assistant,
            content: "It is a reason-then-act loop.".into(),
            created_at: "2026-06-20T00:00:00Z".into(),
            citations: vec![ChatCitation {
                source_id: Some("S2".into()),
                page_path: "wiki/concepts/react-pattern.md".into(),
                title: "ReAct Pattern".into(),
                snippet: None,
                score: 100,
                is_pinned: false,
            }],
            route: None,
            provider: None,
            task_id: None,
            convenience_edit: None,
            retrieval_diagnostics: None,
            saved_path: None,
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
    fn build_answer_markdown_sources_follow_parsed_model_citations_only() {
        let (context, root) = tmp_context("markdown-parsed-only");
        let service = ChatService::default();
        let session = service.create_session(&context, None, None).unwrap();
        let question = user_message("Which page is cited?");
        let refs = vec![
            source_ref("S1", "wiki/retrieved-only.md", "Retrieved Only"),
            source_ref("S2", "wiki/model-used.md", "Model Used"),
        ];
        let parsed =
            ChatService::parse_model_citations("Only the second source is used [S2].", &refs);
        let answer = ChatMessage {
            id: "a-2".into(),
            role: ChatRole::Assistant,
            content: "Only the second source is used [S2].".into(),
            created_at: "2026-06-20T00:00:00Z".into(),
            citations: parsed.citations,
            route: None,
            provider: None,
            task_id: None,
            convenience_edit: None,
            retrieval_diagnostics: None,
            saved_path: None,
        };
        let (_, markdown) = service.build_answer_markdown(&session, &question, &answer);
        assert!(markdown.contains("  - wiki/model-used.md"));
        assert!(!markdown.contains("wiki/retrieved-only.md"));
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
        assert!(!context.app_dir.join("graph-cache.json").exists());
        let log = std::fs::read_to_string(context.wiki_dir.join("log.md")).unwrap();
        assert!(log.contains("wiki/queries/my-query.md"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_answer_overwrite_requires_allow_flag_then_hash_matches() {
        let (context, root) = tmp_context("save-overwrite");
        seed_vault(&context);
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let service = ChatService::default();
        let markdown_v1 = "---\ntype: query\n---\n\n# Q\n\nFirst.";
        let markdown_v2 = "---\ntype: query\n---\n\n# Q\n\nSecond.";

        service
            .save_answer_to_wiki(&context, &git, None, None, false, markdown_v1, "q")
            .unwrap();

        let err = service
            .save_answer_to_wiki(&context, &git, None, None, false, markdown_v2, "q")
            .expect_err("overwrite must require allow_overwrite");
        assert_eq!(err.code, "FILE_ALREADY_EXISTS");
        let current_hash = err.details.as_ref().unwrap()["currentHash"]
            .as_str()
            .unwrap()
            .to_string();

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
    fn stale_answer_hash_is_rejected_before_git_checkpoint_preflight() {
        let (context, root) = tmp_context("save-stale-before-checkpoint");
        seed_vault(&context);
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let service = ChatService::default();
        let markdown_v1 = "---\ntype: query\n---\n\n# Q\n\nFirst.";
        let markdown_v2 = "---\ntype: query\n---\n\n# Q\n\nSecond.";
        service
            .save_answer_to_wiki(&context, &git, None, None, false, markdown_v1, "q")
            .unwrap();

        // A repository-owned filter makes the checkpoint preflight fail
        // closed. A stale compare-and-swap token must still be rejected before
        // Git is consulted, while a matching token must continue to require a
        // valid checkpoint.
        let configured = std::process::Command::new("git")
            .args(["config", "filter.saved-answer.clean", "false"])
            .current_dir(&context.root)
            .status()
            .unwrap();
        assert!(configured.success());

        let error = service
            .save_answer_to_wiki(
                &context,
                &git,
                None,
                Some("stale-hash"),
                true,
                markdown_v2,
                "q",
            )
            .expect_err("a stale hash must win before checkpoint preflight");
        assert_eq!(error.code, "FILE_HASH_MISMATCH");
        assert_eq!(
            std::fs::read_to_string(context.resolve_project_path("wiki/queries/q.md").unwrap())
                .unwrap(),
            markdown_v1
        );

        let current_hash = service
            .file_store
            .file_hash(&context, "wiki/queries/q.md")
            .unwrap();
        let error = service
            .save_answer_to_wiki(
                &context,
                &git,
                None,
                Some(&current_hash),
                true,
                markdown_v2,
                "q",
            )
            .expect_err("a matching hash must still require a valid checkpoint");
        assert_eq!(error.code, "GIT_CHECKPOINT_FAILED");
        assert_eq!(
            std::fs::read_to_string(context.resolve_project_path("wiki/queries/q.md").unwrap())
                .unwrap(),
            markdown_v1
        );

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
