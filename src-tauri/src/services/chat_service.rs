use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::errors::BackendError;
use crate::models::chat::{
    ChatCitation, ChatExpandedPage, ChatMessage, ChatRetrievalDiagnostics, ChatRetrievalHit,
    ChatRoute, ChatSession, ChatSessionSummary, ChatSourceRef, ChatSourceSelectionReason,
    SaveAnswerResult,
};
use crate::models::paths::ProjectContext;
use crate::models::wiki::{WikiPageMeta, WikiPageType};
use crate::services::file_store::FileStore;
use crate::services::SearchService;
use crate::services::{GitService, GraphService, WriteMode};
use crate::utils::markdown_utils::slugify_query;
use crate::utils::time_utils::now_rfc3339;

const CHATS_DIR: &str = ".app/chats";
const DEFAULT_TITLE: &str = "New chat";
const RETRIEVAL_LIMIT: usize = 6;
const EXCERPT_CHARS: usize = 1200;
const DEFAULT_CONTEXT_CHARS: usize = 24_000;
const AGENT_CONTEXT_CHARS: usize = 48_000;
const MIN_CONTEXT_CHARS: usize = 2_000;
const MAX_CONTEXT_CHARS: usize = 120_000;
const MIN_KEYWORD_SOURCE_CHARS: usize = 240;
const MAX_GRAPH_EXPANSIONS: usize = 3;
const MAX_SOURCE_OVERLAP_EXPANSIONS: usize = 3;

/// Persists chat sessions as JSON under `.app/chats/{id}.json` and assembles the
/// retrieval context (local SearchService hits + purpose + bounded history) for
/// the model prompt. Persisted citations are parsed from model output after the
/// answer is generated; retrieval hits remain diagnostics.
#[derive(Default)]
pub struct ChatService {
    file_store: FileStore,
}

impl ChatService {
    pub fn create_session(
        &self,
        context: &ProjectContext,
        title: Option<&str>,
        context_page_path: Option<&str>,
    ) -> Result<ChatSession, BackendError> {
        let now = now_rfc3339();
        // Normalize empty/whitespace page paths to None so a stray "" doesn't
        // masquerade as page-scoped metadata. Backslashes are normalized to
        // forward slashes for cross-platform consistency (CLAUDE.md path rule).
        let normalized_page_path = context_page_path
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| validate_context_page_path(context, p))
            .transpose()?;
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

    /// Local retrieval: keyword search → top pages + bounded excerpts, plus
    /// `purpose.md`. Returns both the typed citations (for the UI) and the
    /// single assembled prompt string (for the Agent/BYOK backend). No model is
    /// called here. The prompt carries a natural-language instruction telling
    /// the model to answer in `language` (CLAUDE.md: "Agent 生成内容按用户
    /// 语言偏好输出").
    pub fn build_retrieval_context(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        query: &str,
        session: &ChatSession,
        language: &str,
        route: ChatRoute,
        context_window: Option<u64>,
        pinned_page_path: Option<&str>,
    ) -> Result<RetrievalContext, BackendError> {
        self.build_retrieval_context_with_mode(
            context,
            search_service,
            query,
            session,
            language,
            route,
            context_window,
            pinned_page_path,
            AgentPromptMode::ReadOnly,
        )
    }

    pub fn build_convenience_retrieval_context(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        query: &str,
        session: &ChatSession,
        language: &str,
        pinned_page_path: Option<&str>,
    ) -> Result<RetrievalContext, BackendError> {
        self.build_retrieval_context_with_mode(
            context,
            search_service,
            query,
            session,
            language,
            ChatRoute::Agent,
            None,
            pinned_page_path,
            AgentPromptMode::ConvenienceWrite,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build_retrieval_context_with_mode(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        query: &str,
        session: &ChatSession,
        language: &str,
        route: ChatRoute,
        context_window: Option<u64>,
        pinned_page_path: Option<&str>,
        agent_prompt_mode: AgentPromptMode,
    ) -> Result<RetrievalContext, BackendError> {
        let (budget_chars, source_budget_chars, history_budget_chars) =
            retrieval_budgets(route, context_window);
        let purpose = self.file_store.read_markdown(context, "purpose.md").ok();
        let mut diagnostic_hits = Vec::new();
        let mut candidates = Vec::new();
        let mut seen_paths = HashSet::new();
        let mut seed_paths = Vec::new();
        let mut expanded_pages = Vec::new();
        let mut expanded_page_paths = HashSet::new();
        if let Ok(index) = search_service.read_page(context, "wiki/index.md", &HashSet::new()) {
            seen_paths.insert(index.meta.path.clone());
            candidates.push(SourceCandidate {
                path: index.meta.path,
                title: index.meta.title,
                excerpt: index.body_markdown.trim().to_string(),
                score: 20_000,
                is_pinned: false,
                required: true,
            });
        }
        if let Some(path) = pinned_page_path {
            let hit = self.pinned_retrieval_hit(context, search_service, path)?;
            diagnostic_hits.push(hit.clone());
            seed_paths.push(hit.path.clone());
            if seen_paths.insert(hit.path.clone()) {
                candidates.push(SourceCandidate::from_hit(hit, true));
            }
        }
        let search_hits = search_service.retrieve_with_excerpts(
            context,
            query,
            RETRIEVAL_LIMIT,
            EXCERPT_CHARS,
        )?;
        for hit in search_hits {
            diagnostic_hits.push(hit.clone());
            seed_paths.push(hit.path.clone());
            if seen_paths.insert(hit.path.clone()) {
                candidates.push(SourceCandidate::from_hit(hit, false));
            }
        }
        let pages = search_service.scan_wiki(context, &HashSet::new())?.pages;
        for expansion in graph_expand_candidates(&pages, &seed_paths, MAX_GRAPH_EXPANSIONS) {
            if expanded_page_paths.insert(expansion.path.clone()) {
                expanded_pages.push(ChatExpandedPage {
                    path: expansion.path.clone(),
                    reason: ChatSourceSelectionReason::GraphNeighbor,
                });
            }
            if seen_paths.insert(expansion.path.clone()) {
                if let Some(candidate) =
                    candidate_from_page(context, search_service, &expansion.path, expansion.score)
                {
                    candidates.push(candidate);
                }
            }
        }
        for expansion in
            source_overlap_candidates(&pages, &seed_paths, MAX_SOURCE_OVERLAP_EXPANSIONS)
        {
            if expanded_page_paths.insert(expansion.path.clone()) {
                expanded_pages.push(ChatExpandedPage {
                    path: expansion.path.clone(),
                    reason: ChatSourceSelectionReason::SourceOverlap,
                });
            }
            if seen_paths.insert(expansion.path.clone()) {
                if let Some(candidate) =
                    candidate_from_page(context, search_service, &expansion.path, expansion.score)
                {
                    candidates.push(candidate);
                }
            }
        }
        let mut remaining_source_chars = source_budget_chars;
        let mut source_refs = Vec::new();
        let mut omitted_pages = Vec::new();
        for candidate in candidates {
            let excerpt_len = char_len(&candidate.excerpt);
            let can_include = candidate.required
                || candidate.is_pinned
                || (remaining_source_chars >= MIN_KEYWORD_SOURCE_CHARS
                    && excerpt_len <= remaining_source_chars);
            if !can_include {
                omitted_pages.push(candidate.path);
                continue;
            }
            let take = remaining_source_chars;
            let (excerpt, used_chars) = take_chars(&candidate.excerpt, take);
            remaining_source_chars = remaining_source_chars.saturating_sub(used_chars);
            let page_path = candidate.path.clone();
            source_refs.push(ChatSourceRef {
                id: format!("S{}", source_refs.len() + 1),
                page_path,
                title: candidate.title,
                excerpt: if excerpt.is_empty() {
                    None
                } else {
                    Some(excerpt)
                },
                score: candidate.score,
                is_pinned: candidate.is_pinned,
            });
            if used_chars < excerpt_len {
                omitted_pages.push(candidate.path);
            }
        }
        let diagnostics = ChatRetrievalDiagnostics {
            route,
            retrieval_hits: diagnostic_hits,
            expanded_pages,
            selected_pages: source_refs
                .iter()
                .map(|source| source.page_path.clone())
                .collect(),
            omitted_pages,
            budget_chars,
            source_budget_chars,
            history_budget_chars,
            invalid_citation_ids: Vec::new(),
            has_unverified: false,
        };
        let prompt = match route {
            ChatRoute::Byok => self.assemble_byok_prompt(
                query,
                session,
                &source_refs,
                purpose.as_deref(),
                language,
                history_budget_chars,
            ),
            ChatRoute::Agent => match agent_prompt_mode {
                AgentPromptMode::ReadOnly => self.assemble_agent_prompt(
                    query,
                    session,
                    &source_refs,
                    purpose.as_deref(),
                    language,
                    history_budget_chars,
                ),
                AgentPromptMode::ConvenienceWrite => self.assemble_agent_convenience_prompt(
                    query,
                    session,
                    &source_refs,
                    purpose.as_deref(),
                    language,
                    history_budget_chars,
                ),
            },
        };
        Ok(RetrievalContext {
            source_refs,
            diagnostics,
            prompt,
        })
    }

    fn pinned_retrieval_hit(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        path: &str,
    ) -> Result<ChatRetrievalHit, BackendError> {
        let normalized = validate_pinned_page_path(context, path)?;
        let page = search_service.read_page(context, &normalized, &HashSet::new())?;
        Ok(ChatRetrievalHit {
            path: page.meta.path,
            title: page.meta.title,
            snippet: Some(first_prompt_line(&page.body_markdown)),
            score: 10_000,
            excerpt: Some(page.body_markdown.trim().to_string()),
            is_pinned: true,
        })
    }

    fn assemble_byok_prompt(
        &self,
        query: &str,
        session: &ChatSession,
        sources: &[ChatSourceRef],
        purpose: Option<&str>,
        language: &str,
        history_budget_chars: usize,
    ) -> String {
        let mut prompt = String::new();
        prompt.push_str(
            "You are answering a question about a local Markdown wiki. You do not have filesystem \
             access. You do not have tool access. Use only the numbered sources in this prompt. \
             Respond with citation markers like [S1] or [S1, S2] for claims grounded in sources. \
             If a claim is not supported by the numbered sources, mark it [unverified].\n",
        );
        append_prompt_common(
            &mut prompt,
            query,
            session,
            sources,
            purpose,
            language,
            history_budget_chars,
        );
        prompt
    }

    fn assemble_agent_prompt(
        &self,
        query: &str,
        session: &ChatSession,
        sources: &[ChatSourceRef],
        purpose: Option<&str>,
        language: &str,
        history_budget_chars: usize,
    ) -> String {
        let mut prompt = String::new();
        prompt.push_str(
            "You are answering a question about a local Markdown wiki in read-only mode. Start from \
             wiki/index.md and the numbered sources below before reading more. You may read \
             additional Markdown files under wiki/ if needed, but Do not modify files. Cite provided \
             sources with markers like [S1] or [S1, S2]. If a claim depends only on additional files \
             that are not among the numbered sources, mark it [unverified] unless a numbered source \
             also supports it.\n",
        );
        append_prompt_common(
            &mut prompt,
            query,
            session,
            sources,
            purpose,
            language,
            history_budget_chars,
        );
        prompt
    }

    fn assemble_agent_convenience_prompt(
        &self,
        query: &str,
        session: &ChatSession,
        sources: &[ChatSourceRef],
        purpose: Option<&str>,
        language: &str,
        history_budget_chars: usize,
    ) -> String {
        let mut prompt = String::new();
        prompt.push_str(
            "You are preparing context for a local Markdown wiki convenience edit. Start from \
             wiki/index.md and the numbered sources below before reading more. You may read \
             additional Markdown files under wiki/ if needed. The appended convenience instructions \
             define the narrow write scope; make only those small Markdown edits and leave original \
             sources untouched. Cite provided sources in your final chat answer with markers like \
             [S1] or [S1, S2]. If a claim depends only on additional files that are not among the \
             numbered sources, mark it [unverified] unless a numbered source also supports it.\n",
        );
        append_prompt_common(
            &mut prompt,
            query,
            session,
            sources,
            purpose,
            language,
            history_budget_chars,
        );
        prompt
    }

    pub fn parse_model_citations(answer: &str, sources: &[ChatSourceRef]) -> ParsedModelCitations {
        let sources_by_id: HashMap<String, &ChatSourceRef> = sources
            .iter()
            .map(|source| (source.id.clone(), source))
            .collect();
        let mut seen = HashSet::new();
        let mut invalid_seen = HashSet::new();
        let mut citations = Vec::new();
        let mut invalid_source_ids = Vec::new();
        let mut has_unverified = false;
        let mut rest = answer;
        while let Some(open) = rest.find('[') {
            let after_open = &rest[open + 1..];
            let Some(close) = after_open.find(']') else {
                break;
            };
            let marker = after_open[..close].trim();
            if marker.eq_ignore_ascii_case("unverified") {
                has_unverified = true;
                rest = &after_open[close + 1..];
                continue;
            }
            for token in marker.replace(',', " ").split_whitespace() {
                let id = token.trim().to_ascii_uppercase();
                if !is_source_marker_id(&id) {
                    continue;
                }
                match sources_by_id.get(&id) {
                    Some(source) if seen.insert(id.clone()) => citations.push(ChatCitation {
                        source_id: Some(id),
                        page_path: source.page_path.clone(),
                        title: source.title.clone(),
                        snippet: source.excerpt.clone(),
                        score: source.score,
                        is_pinned: source.is_pinned,
                    }),
                    Some(_) => {}
                    None if invalid_seen.insert(id.clone()) => invalid_source_ids.push(id),
                    None => {}
                }
            }
            rest = &after_open[close + 1..];
        }
        ParsedModelCitations {
            citations,
            invalid_source_ids,
            has_unverified,
        }
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

#[derive(Debug, Clone, Copy)]
enum AgentPromptMode {
    ReadOnly,
    ConvenienceWrite,
}

struct SourceCandidate {
    path: String,
    title: String,
    excerpt: String,
    score: i64,
    is_pinned: bool,
    required: bool,
}

impl SourceCandidate {
    fn from_hit(hit: ChatRetrievalHit, required: bool) -> Self {
        let snippet = hit.snippet.clone();
        Self {
            path: hit.path,
            title: hit.title,
            excerpt: hit.excerpt.or(snippet).unwrap_or_default(),
            score: hit.score,
            is_pinned: hit.is_pinned,
            required,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphExpansionCandidate {
    pub path: String,
    pub score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOverlapCandidate {
    pub path: String,
    pub score: i64,
}

pub fn graph_expand_candidates(
    pages: &[WikiPageMeta],
    seed_paths: &[String],
    max_neighbors: usize,
) -> Vec<GraphExpansionCandidate> {
    if seed_paths.is_empty() || max_neighbors == 0 {
        return Vec::new();
    }
    let seed_set: HashSet<&str> = seed_paths.iter().map(String::as_str).collect();
    let graph = GraphService::default().build_from_pages(pages);
    let mut scores: HashMap<String, i64> = HashMap::new();
    for edge in graph.edges {
        let source_seed = seed_set.contains(edge.source.as_str());
        let target_seed = seed_set.contains(edge.target.as_str());
        let candidate = match (source_seed, target_seed) {
            (true, false) => Some(edge.target),
            (false, true) => Some(edge.source),
            _ => None,
        };
        if let Some(path) = candidate {
            if is_expandable_page(pages, &path) {
                *scores.entry(path).or_insert(0) += (edge.weight as i64) * 1_000;
            }
        }
    }
    sorted_expansion_candidates(scores)
        .into_iter()
        .take(max_neighbors)
        .map(|(path, score)| GraphExpansionCandidate { path, score })
        .collect()
}

pub fn source_overlap_candidates(
    pages: &[WikiPageMeta],
    seed_paths: &[String],
    max_candidates: usize,
) -> Vec<SourceOverlapCandidate> {
    if seed_paths.is_empty() || max_candidates == 0 {
        return Vec::new();
    }
    let seed_set: HashSet<&str> = seed_paths.iter().map(String::as_str).collect();
    let mut seed_sources = HashSet::new();
    for page in pages
        .iter()
        .filter(|page| seed_set.contains(page.path.as_str()))
    {
        for source in &page.sources {
            if let Some(key) = canonical_source_key(source) {
                seed_sources.insert(key);
            }
        }
    }
    if seed_sources.is_empty() {
        return Vec::new();
    }
    let mut scores = HashMap::new();
    for page in pages {
        if seed_set.contains(page.path.as_str()) || !is_expandable_page(pages, &page.path) {
            continue;
        }
        let overlap = page
            .sources
            .iter()
            .filter_map(|source| canonical_source_key(source))
            .filter(|source| seed_sources.contains(source))
            .count();
        if overlap > 0 {
            scores.insert(page.path.clone(), (overlap as i64) * 900);
        }
    }
    sorted_expansion_candidates(scores)
        .into_iter()
        .take(max_candidates)
        .map(|(path, score)| SourceOverlapCandidate { path, score })
        .collect()
}

fn canonical_source_key(source: &str) -> Option<String> {
    let normalized = source.trim().replace('\\', "/");
    if normalized.is_empty() {
        return None;
    }
    if normalized.starts_with("wiki/sources/") {
        Some(normalized.to_ascii_lowercase())
    } else if normalized.starts_with("sources/") {
        Some(format!("wiki/{normalized}").to_ascii_lowercase())
    } else if !normalized.contains('/') {
        Some(format!("wiki/sources/{normalized}").to_ascii_lowercase())
    } else {
        Some(normalized.to_ascii_lowercase())
    }
}

fn sorted_expansion_candidates(mut scores: HashMap<String, i64>) -> Vec<(String, i64)> {
    let mut items: Vec<(String, i64)> = scores.drain().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items
}

fn is_expandable_page(pages: &[WikiPageMeta], path: &str) -> bool {
    pages.iter().any(|page| {
        page.path == path
            && !matches!(page.page_type, WikiPageType::Source | WikiPageType::Query)
            && !STRUCTURAL_SOURCE_PATHS.contains(&page.path.as_str())
    })
}

const STRUCTURAL_SOURCE_PATHS: &[&str] = &["wiki/index.md", "wiki/overview.md", "wiki/log.md"];

fn candidate_from_page(
    context: &ProjectContext,
    search_service: &SearchService,
    path: &str,
    score: i64,
) -> Option<SourceCandidate> {
    let page = search_service
        .read_page(context, path, &HashSet::new())
        .ok()?;
    Some(SourceCandidate {
        path: page.meta.path,
        title: page.meta.title,
        excerpt: page.body_markdown.trim().to_string(),
        score,
        is_pinned: false,
        required: false,
    })
}

fn retrieval_budgets(route: ChatRoute, context_window: Option<u64>) -> (usize, usize, usize) {
    let base = match (route, context_window) {
        (ChatRoute::Byok, Some(window)) if window > 0 => (window as usize).saturating_mul(2),
        (ChatRoute::Byok, _) => DEFAULT_CONTEXT_CHARS,
        (ChatRoute::Agent, _) => AGENT_CONTEXT_CHARS,
    }
    .clamp(MIN_CONTEXT_CHARS, MAX_CONTEXT_CHARS);
    let source_budget = base.saturating_mul(60) / 100;
    let history_budget = base.saturating_mul(25) / 100;
    (base, source_budget, history_budget)
}

fn append_prompt_common(
    prompt: &mut String,
    query: &str,
    session: &ChatSession,
    sources: &[ChatSourceRef],
    purpose: Option<&str>,
    language: &str,
    history_budget_chars: usize,
) {
    prompt.push_str(&crate::utils::i18n::language_instruction(language));
    prompt.push('\n');
    if let Some(purpose) = purpose {
        prompt.push_str("\n--- Wiki purpose ---\n");
        prompt.push_str(purpose.trim());
        prompt.push('\n');
    }
    if sources.is_empty() {
        prompt.push_str("\n--- Numbered sources ---\nNo numbered sources were retrieved.\n");
    } else {
        prompt.push_str("\n--- Numbered sources ---\n");
        for source in sources {
            append_prompt_source(prompt, source);
        }
    }
    append_prompt_history(prompt, session, history_budget_chars);
    prompt.push_str("\n--- Latest question ---\n");
    prompt.push_str(query.trim());
    prompt.push('\n');
}

fn append_prompt_source(prompt: &mut String, source: &ChatSourceRef) {
    prompt.push_str(&format!(
        "\n### [{}] {} ({})\n",
        source.id, source.title, source.page_path
    ));
    if let Some(excerpt) = &source.excerpt {
        prompt.push_str(excerpt.trim());
        prompt.push('\n');
    }
}

fn append_prompt_history(prompt: &mut String, session: &ChatSession, budget_chars: usize) {
    let history = session
        .messages
        .iter()
        .rev()
        .collect::<Vec<&ChatMessage>>()
        .into_iter()
        .rev();
    let mut remaining = budget_chars;
    let mut has_history = false;
    for message in history {
        if remaining == 0 {
            break;
        }
        let label = match message.role {
            crate::models::chat::ChatRole::User => "User",
            crate::models::chat::ChatRole::Assistant => "Assistant",
        };
        let line = format!("{label}: {}\n", message.content.trim());
        let (bounded, used) = take_chars(&line, remaining);
        if bounded.trim().is_empty() {
            break;
        }
        if !has_history {
            prompt.push_str("\n--- Conversation so far ---\n");
            has_history = true;
        }
        prompt.push_str(&bounded);
        remaining = remaining.saturating_sub(used);
    }
}

fn is_source_marker_id(value: &str) -> bool {
    value
        .strip_prefix('S')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
}

fn take_chars(value: &str, limit: usize) -> (String, usize) {
    let mut out = String::new();
    let mut used = 0;
    for ch in value.chars() {
        if used >= limit {
            break;
        }
        out.push(ch);
        used += 1;
    }
    (out, used)
}

fn char_len(value: &str) -> usize {
    value.chars().count()
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

fn validate_pinned_page_path(context: &ProjectContext, path: &str) -> Result<String, BackendError> {
    let normalized = path.replace('\\', "/");
    let absolute = context.resolve_project_path(&normalized)?;
    if absolute.strip_prefix(&context.wiki_dir).is_err() || !normalized.starts_with("wiki/") {
        return Err(BackendError::new(
            "CHAT_PINNED_PAGE_INVALID",
            "Pinned chat context must be a page under the wiki/ directory.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }
    if !normalized.ends_with(".md") {
        return Err(BackendError::new(
            "CHAT_PINNED_PAGE_INVALID",
            "Pinned chat context must be a Markdown (.md) page.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }
    if !absolute.is_file() {
        return Err(BackendError::new(
            "FILE_NOT_FOUND",
            "The pinned Wiki page no longer exists.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "path": normalized })));
    }
    Ok(normalized)
}

fn first_prompt_line(body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .chars()
        .take(160)
        .collect()
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
    use crate::models::chat::{
        ChatMessage, ChatRole, ChatRoute, ChatSourceRef, ChatSourceSelectionReason,
    };
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
            "---\ntitle: ReAct Pattern\ntype: concept\ntags: [reasoning]\nsources:\n  - wiki/sources/shared.md\n---\n\n# ReAct Pattern\n\nReason then act loop for agents. See [[agent-memory]].",
        );
        write_file(
            context,
            "wiki/concepts/agent-memory.md",
            "---\ntitle: Agent Memory\ntype: concept\ntags: [memory]\nsources:\n  - wiki/sources/shared.md\n---\n\n# Agent Memory\n\nCovers short context windows and RAG.",
        );
        write_file(
            context,
            "wiki/sources/shared.md",
            "---\ntitle: Shared Source\ntype: source\n---\n\n# Shared Source\n\nOriginal source.",
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
            provider: None,
            task_id: None,
            convenience_edit: None,
            retrieval_diagnostics: None,
        }
    }

    fn source_ref(id: &str, path: &str, title: &str) -> ChatSourceRef {
        ChatSourceRef {
            id: id.into(),
            page_path: path.into(),
            title: title.into(),
            excerpt: Some(format!("{title} excerpt")),
            score: 100,
            is_pinned: false,
        }
    }

    #[test]
    fn citation_parser_accepts_single_marker() {
        let refs = vec![source_ref("S1", "wiki/a.md", "A")];

        let parsed = ChatService::parse_model_citations("Answer grounded in A [S1].", &refs);

        assert_eq!(parsed.citations.len(), 1);
        assert_eq!(parsed.citations[0].source_id.as_deref(), Some("S1"));
        assert_eq!(parsed.citations[0].page_path, "wiki/a.md");
        assert!(parsed.invalid_source_ids.is_empty());
        assert!(!parsed.has_unverified);
    }

    #[test]
    fn citation_parser_dedupes_duplicate_markers() {
        let refs = vec![source_ref("S1", "wiki/a.md", "A")];

        let parsed = ChatService::parse_model_citations("A [S1] and again [S1].", &refs);

        assert_eq!(parsed.citations.len(), 1);
        assert_eq!(parsed.citations[0].source_id.as_deref(), Some("S1"));
    }

    #[test]
    fn citation_parser_accepts_multiple_ids_in_one_marker() {
        let refs = vec![
            source_ref("S1", "wiki/a.md", "A"),
            source_ref("S2", "wiki/b.md", "B"),
        ];

        let parsed = ChatService::parse_model_citations("Compare both [S1, S2].", &refs);

        let paths: Vec<&str> = parsed
            .citations
            .iter()
            .map(|citation| citation.page_path.as_str())
            .collect();
        assert_eq!(paths, vec!["wiki/a.md", "wiki/b.md"]);
    }

    #[test]
    fn citation_parser_reports_invalid_ids_but_does_not_persist_them() {
        let refs = vec![source_ref("S1", "wiki/a.md", "A")];

        let parsed =
            ChatService::parse_model_citations("Unsupported [S9] but supported [S1].", &refs);

        assert_eq!(parsed.citations.len(), 1);
        assert_eq!(parsed.invalid_source_ids, vec!["S9"]);
    }

    #[test]
    fn citation_parser_returns_no_citations_for_no_marker_or_unverified() {
        let refs = vec![source_ref("S1", "wiki/a.md", "A")];

        let no_marker = ChatService::parse_model_citations("No source marker here.", &refs);
        let unverified =
            ChatService::parse_model_citations("This is uncertain [unverified].", &refs);

        assert!(no_marker.citations.is_empty());
        assert!(unverified.citations.is_empty());
        assert!(unverified.has_unverified);
    }

    #[test]
    fn byok_prompt_has_no_filesystem_or_tool_access_and_uses_numbered_sources() {
        let (context, root) = tmp_context("byok-prompt");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "react pattern",
                &session,
                "en",
                ChatRoute::Byok,
                Some(8_000),
                None,
            )
            .unwrap();

        assert!(ctx.prompt.contains("You do not have filesystem access"));
        assert!(ctx.prompt.contains("You do not have tool access"));
        assert!(ctx.prompt.contains("[S1]"));
        assert!(ctx.prompt.contains("Respond with citation markers"));
        assert!(!ctx.prompt.contains("read Markdown files under wiki/"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_prompt_is_read_only_index_first_and_can_read_more() {
        let (context, root) = tmp_context("agent-prompt");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "react pattern",
                &session,
                "en",
                ChatRoute::Agent,
                None,
                None,
            )
            .unwrap();

        assert!(ctx.prompt.contains("read-only"));
        assert!(ctx.prompt.contains("Start from wiki/index.md"));
        assert!(ctx
            .prompt
            .contains("read additional Markdown files under wiki/"));
        assert!(ctx.prompt.contains("Do not modify files"));
        assert!(ctx.prompt.contains("mark it [unverified]"));
        assert!(ctx
            .source_refs
            .iter()
            .any(|source| source.page_path == "wiki/index.md"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn convenience_agent_prompt_allows_scoped_writes_without_read_only_conflict() {
        let (context, root) = tmp_context("agent-convenience-prompt");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_convenience_retrieval_context(
                &context,
                &search,
                "update the ReAct page",
                &session,
                "en",
                Some("wiki/concepts/react-pattern.md"),
            )
            .unwrap();

        assert!(ctx.prompt.contains("Start from wiki/index.md"));
        assert!(ctx
            .prompt
            .contains("read additional Markdown files under wiki/"));
        assert!(ctx.prompt.contains("make only those small Markdown edits"));
        assert!(!ctx.prompt.contains("read-only mode"));
        assert!(!ctx.prompt.contains("Do not modify files"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_planner_includes_index_pinned_keyword_hits_and_omits_over_budget() {
        let (context, root) = tmp_context("planner-budget");
        seed_vault(&context);
        write_file(
            &context,
            "wiki/concepts/long-keyword.md",
            &format!(
                "---\ntitle: Long Keyword\ntype: concept\n---\n\n# Long Keyword\n\n{}",
                "react pattern details. ".repeat(400)
            ),
        );
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "react pattern long keyword",
                &session,
                "en",
                ChatRoute::Byok,
                Some(800),
                Some("wiki/concepts/agent-memory.md"),
            )
            .unwrap();

        assert_eq!(ctx.source_refs[0].page_path, "wiki/index.md");
        assert!(ctx
            .source_refs
            .iter()
            .any(|source| source.page_path == "wiki/concepts/agent-memory.md" && source.is_pinned));
        assert!(ctx
            .source_refs
            .iter()
            .any(|source| source.page_path == "wiki/concepts/react-pattern.md"));
        assert!(!ctx.diagnostics.retrieval_hits.is_empty());
        assert!(!ctx.diagnostics.omitted_pages.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_planner_keeps_required_and_pinned_excerpts_within_source_budget() {
        let (context, root) = tmp_context("planner-required-budget");
        seed_vault(&context);
        write_file(
            &context,
            "wiki/index.md",
            &format!("# Index\n\n{}", "index context. ".repeat(500)),
        );
        write_file(
            &context,
            "wiki/concepts/huge-pinned.md",
            &format!(
                "---\ntitle: Huge Pinned\ntype: concept\n---\n\n# Huge Pinned\n\n{}",
                "pinned context. ".repeat(500)
            ),
        );
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "huge pinned",
                &session,
                "en",
                ChatRoute::Byok,
                Some(800),
                Some("wiki/concepts/huge-pinned.md"),
            )
            .unwrap();

        let excerpt_chars: usize = ctx
            .source_refs
            .iter()
            .filter_map(|source| source.excerpt.as_deref())
            .map(super::char_len)
            .sum();
        assert!(excerpt_chars <= ctx.diagnostics.source_budget_chars);
        assert!(ctx
            .diagnostics
            .omitted_pages
            .contains(&"wiki/concepts/huge-pinned.md".to_string()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_planner_adds_one_hop_graph_neighbors_with_diagnostics() {
        let (context, root) = tmp_context("planner-graph");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "react pattern",
                &session,
                "en",
                ChatRoute::Byok,
                Some(8_000),
                None,
            )
            .unwrap();

        assert!(ctx
            .source_refs
            .iter()
            .any(|source| source.page_path == "wiki/concepts/agent-memory.md"));
        assert!(ctx.diagnostics.expanded_pages.iter().any(|expanded| {
            expanded.path == "wiki/concepts/agent-memory.md"
                && expanded.reason == ChatSourceSelectionReason::GraphNeighbor
        }));
        assert_eq!(
            ctx.diagnostics
                .expanded_pages
                .iter()
                .filter(|expanded| expanded.path == "wiki/concepts/agent-memory.md")
                .count(),
            1,
            "diagnostics should list each expanded page once even if multiple expansion strategies find it"
        );
        assert_eq!(
            ctx.source_refs
                .iter()
                .filter(|source| source.page_path == "wiki/concepts/agent-memory.md")
                .count(),
            1,
            "graph-expanded pages must be deduped with keyword/index/pinned pages"
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_planner_adds_source_overlap_candidates_with_diagnostics() {
        let (context, root) = tmp_context("planner-source-overlap");
        seed_vault(&context);
        write_file(
            &context,
            "wiki/synthesis/shared-synthesis.md",
            "---\ntitle: Shared Synthesis\ntype: synthesis\nsources:\n  - shared.md\n---\n\n# Shared Synthesis\n\nThis page shares source evidence without naming the seed terms.",
        );
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "react pattern",
                &session,
                "en",
                ChatRoute::Byok,
                Some(8_000),
                None,
            )
            .unwrap();

        assert!(ctx
            .source_refs
            .iter()
            .any(|source| source.page_path == "wiki/synthesis/shared-synthesis.md"));
        assert!(ctx.diagnostics.expanded_pages.iter().any(|expanded| {
            expanded.path == "wiki/synthesis/shared-synthesis.md"
                && expanded.reason == ChatSourceSelectionReason::SourceOverlap
        }));
        assert!(!ctx
            .source_refs
            .iter()
            .any(|source| source.page_path.starts_with("wiki/sources/")));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_planner_omits_expanded_pages_when_budget_is_exhausted() {
        let (context, root) = tmp_context("planner-expanded-budget");
        seed_vault(&context);
        write_file(
            &context,
            "wiki/concepts/react-pattern.md",
            &format!(
                "---\ntitle: ReAct Pattern\ntype: concept\ntags: [reasoning]\nsources:\n  - wiki/sources/shared.md\n---\n\n# ReAct Pattern\n\n{}",
                "react pattern details. ".repeat(300)
            ),
        );
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "react pattern details",
                &session,
                "en",
                ChatRoute::Byok,
                Some(800),
                None,
            )
            .unwrap();

        assert!(ctx.diagnostics.expanded_pages.iter().any(|expanded| {
            expanded.reason == ChatSourceSelectionReason::GraphNeighbor
                || expanded.reason == ChatSourceSelectionReason::SourceOverlap
        }));
        assert!(ctx
            .diagnostics
            .omitted_pages
            .contains(&"wiki/concepts/agent-memory.md".to_string()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wiki_query_skill_contract_is_read_only_index_first_and_citation_required() {
        let skill = include_str!("../../templates/skills/wiki-query/SKILL.md");

        assert!(skill.contains("name: wiki-query"));
        assert!(skill.contains("read-only"));
        assert!(skill.contains("Read `wiki/index.md` first"));
        assert!(skill.contains("numbered citations like `[S1]"));
        assert!(skill.contains("Do not edit, create, delete, move, or rewrite"));
        assert!(skill.contains("`wiki/`, `raw/`, `.app/`, `exports/`, or `skills/`"));
        assert!(skill.contains("normal Search as local keyword/filter search only"));
        assert!(skill.contains("future phase"));
        assert!(skill.contains("no write endpoints"));
    }

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

        let loaded = service.load_session(&context, &session.id).unwrap();
        assert_eq!(loaded, session);

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

        // Seed a session with many short turns; budget, not a fixed turn cap,
        // decides how much history is included.
        let mut session = service.create_session(&context, None, None).unwrap();
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
                provider: None,
                task_id: None,
                convenience_edit: None,
                retrieval_diagnostics: None,
            });
        }

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "react pattern",
                &session,
                "en",
                ChatRoute::Byok,
                Some(8_000),
                None,
            )
            .unwrap();

        assert!(!ctx.source_refs.is_empty());
        assert!(ctx
            .source_refs
            .iter()
            .any(|source| source.page_path == "wiki/concepts/react-pattern.md"));
        assert!(!ctx.diagnostics.retrieval_hits.is_empty());
        assert!(ctx.prompt.contains("Wiki purpose"));
        assert!(ctx.prompt.contains("This wiki explains agents."));
        assert!(ctx.prompt.contains("ReAct Pattern"));
        assert!(ctx.prompt.contains("Latest question"));
        assert!(ctx.prompt.contains("react pattern"));
        // Language preference is injected into the prompt.
        assert!(ctx.prompt.contains("Respond in English."));
        // These short turns fit the conservative history budget, including
        // turns older than the previous fixed 8-turn cap.
        assert!(ctx.prompt.contains("ancient turn number 14"));
        assert!(ctx.prompt.contains("ancient turn number 0"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_context_includes_pinned_page_first() {
        let (context, root) = tmp_context("retrieval-pinned");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "agent memory",
                &session,
                "en",
                ChatRoute::Byok,
                Some(8_000),
                Some("wiki/concepts/react-pattern.md"),
            )
            .unwrap();

        let pinned = ctx
            .source_refs
            .iter()
            .find(|source| source.page_path == "wiki/concepts/react-pattern.md")
            .unwrap();
        assert!(pinned.is_pinned);
        assert_eq!(pinned.score, 10_000);
        assert!(ctx.prompt.contains("Numbered sources"));
        assert!(ctx.prompt.contains("ReAct Pattern"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_context_includes_full_pinned_page_body() {
        let (context, root) = tmp_context("retrieval-pinned-full");
        seed_vault(&context);
        let long_body = format!(
            "# Long Page\n\n{}\n\nTAIL_MARKER: all later sections are visible",
            "middle paragraph with page details.\n".repeat(80)
        );
        write_file(
            &context,
            "wiki/concepts/long-page.md",
            &format!("---\ntitle: Long Page\ntype: concept\n---\n\n{long_body}"),
        );
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "summarize this page",
                &session,
                "en",
                ChatRoute::Agent,
                None,
                Some("wiki/concepts/long-page.md"),
            )
            .unwrap();

        assert!(ctx
            .prompt
            .contains("TAIL_MARKER: all later sections are visible"));
        assert!(!ctx
            .prompt
            .contains("middle paragraph with page details...."));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_prompt_tells_agent_to_read_wiki_when_sources_are_sparse() {
        let (context, root) = tmp_context("retrieval-agent-read");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "vibecoding",
                &session,
                "en",
                ChatRoute::Agent,
                None,
                None,
            )
            .unwrap();

        assert!(ctx.prompt.contains("read-only"));
        assert!(ctx
            .prompt
            .contains("read additional Markdown files under wiki/"));
        assert!(ctx.prompt.contains("Start from wiki/index.md"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_context_dedupes_pinned_page_from_search_hits() {
        let (context, root) = tmp_context("retrieval-pinned-dedupe");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "react pattern",
                &session,
                "en",
                ChatRoute::Byok,
                Some(8_000),
                Some("wiki/concepts/react-pattern.md"),
            )
            .unwrap();

        assert_eq!(
            ctx.source_refs
                .iter()
                .filter(|source| source.page_path == "wiki/concepts/react-pattern.md")
                .count(),
            1
        );
        assert!(ctx.source_refs.len() <= super::RETRIEVAL_LIMIT + 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieval_context_errors_when_pinned_page_missing() {
        let (context, root) = tmp_context("retrieval-pinned-missing");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let err = service
            .build_retrieval_context(
                &context,
                &search,
                "react pattern",
                &session,
                "en",
                ChatRoute::Byok,
                Some(8_000),
                Some("wiki/concepts/missing.md"),
            )
            .expect_err("missing pinned page must fail retrieval");

        assert_eq!(err.code, "FILE_NOT_FOUND");

        std::fs::remove_dir_all(root).unwrap();
    }

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
            citations: vec![crate::models::chat::ChatCitation {
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
        };

        let (_slug, markdown) = service.build_answer_markdown(&session, &question, &answer);

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
