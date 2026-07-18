use std::collections::{HashMap, HashSet};

use crate::errors::BackendError;
use crate::models::chat::{
    ChatExpandedPage, ChatRetrievalDiagnostics, ChatRetrievalHit, ChatRoute, ChatSession,
    ChatSourceRef, ChatSourceSelectionReason,
};
use crate::models::paths::ProjectContext;
use crate::models::wiki::{WikiPageMeta, WikiPageType};
use crate::services::{GraphService, SearchService};

use super::{ChatService, RetrievalContext};

const RETRIEVAL_LIMIT: usize = 6;
const EXCERPT_CHARS: usize = 1200;
const DEFAULT_CONTEXT_CHARS: usize = 24_000;
const AGENT_CONTEXT_CHARS: usize = 48_000;
const MIN_CONTEXT_CHARS: usize = 2_000;
const MAX_CONTEXT_CHARS: usize = 120_000;
const MAX_PURPOSE_CHARS: usize = 4_000;
const MIN_KEYWORD_SOURCE_CHARS: usize = 240;
const MIN_PINNED_SOURCE_CHARS: usize = 2_000;
const MAX_GRAPH_EXPANSIONS: usize = 3;
const MAX_SOURCE_OVERLAP_EXPANSIONS: usize = 3;

impl ChatService {
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
        // Retrieval order is a compatibility and safety invariant:
        // required purpose/index context -> pinned full page -> keyword hits ->
        // bounded graph neighbors -> bounded source-overlap pages -> bounded
        // source/history prompt assembly. Diagnostics are collected at every
        // retrieval stage, including pages later omitted by prompt budgets.
        let (budget_chars, source_budget_chars, history_budget_chars) =
            retrieval_budgets(route, context_window);
        // purpose.md is project-owned input, so bound it before prompt
        // assembly just like page excerpts. Otherwise one large local file
        // can bypass the retrieval budget and overflow the provider window.
        let purpose = self
            .file_store
            .read_markdown(context, "purpose.md")
            .ok()
            .map(|purpose| take_chars(purpose.trim(), MAX_PURPOSE_CHARS).0);
        let mut diagnostic_hits = Vec::new();
        let mut candidates = Vec::new();
        let mut seen_paths = HashSet::new();
        let mut seed_paths = Vec::new();
        let mut expanded_pages = Vec::new();
        // Diagnostics record every primary selection reason. A path may appear
        // more than once when, for example, it is both pinned and a keyword
        // hit. Graph/source expansion keeps its existing path-level dedupe and
        // graph-before-source-overlap precedence.
        let mut expanded_page_paths = HashSet::new();
        if let Ok(index) = search_service.read_page(context, "wiki/index.md", &HashSet::new()) {
            expanded_pages.push(ChatExpandedPage {
                path: index.meta.path.clone(),
                reason: ChatSourceSelectionReason::Index,
            });
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
            expanded_pages.push(ChatExpandedPage {
                path: hit.path.clone(),
                reason: ChatSourceSelectionReason::Pinned,
            });
            diagnostic_hits.push(hit.clone());
            seed_paths.push(hit.path.clone());
            if seen_paths.insert(hit.path.clone()) {
                candidates.push(SourceCandidate::from_hit(hit, true));
            } else if let Some(candidate) = candidates
                .iter_mut()
                .find(|candidate| candidate.path == hit.path)
            {
                // wiki/index.md is added first as required context. If the
                // user also pins it, upgrade that existing candidate instead
                // of dropping the pinned marker and its reserved budget.
                candidate.title = hit.title;
                candidate.excerpt = hit.excerpt.or(hit.snippet).unwrap_or_default();
                candidate.score = hit.score;
                candidate.is_pinned = true;
            }
        }
        let search_hits = search_service.retrieve_with_excerpts(
            context,
            query,
            RETRIEVAL_LIMIT,
            EXCERPT_CHARS,
        )?;
        for hit in search_hits {
            expanded_pages.push(ChatExpandedPage {
                path: hit.path.clone(),
                reason: ChatSourceSelectionReason::KeywordHit,
            });
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
        // Index/purpose context is useful, but a page-scoped Chat must never
        // spend the entire source budget before its pinned page receives any
        // body text. Reserve a bounded slice for every pinned candidate; a
        // very small provider window still gets the maximum available slice.
        let mut reserved_pinned_chars: usize = candidates
            .iter()
            .filter(|candidate| candidate.is_pinned)
            .map(|candidate| char_len(&candidate.excerpt).min(MIN_PINNED_SOURCE_CHARS))
            .sum();
        let mut source_refs = Vec::new();
        let mut omitted_pages = Vec::new();
        for candidate in candidates {
            let excerpt_len = char_len(&candidate.excerpt);
            let pinned_reservation = if candidate.is_pinned {
                excerpt_len.min(MIN_PINNED_SOURCE_CHARS)
            } else {
                0
            };
            if candidate.is_pinned {
                reserved_pinned_chars = reserved_pinned_chars.saturating_sub(pinned_reservation);
            }
            let available_chars = if candidate.is_pinned {
                remaining_source_chars
            } else {
                remaining_source_chars.saturating_sub(reserved_pinned_chars)
            };
            let can_include = candidate.required
                || candidate.is_pinned
                || (available_chars >= MIN_KEYWORD_SOURCE_CHARS && excerpt_len <= available_chars);
            if !can_include {
                omitted_pages.push(candidate.path);
                continue;
            }
            let take = available_chars;
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
struct GraphExpansionCandidate {
    path: String,
    score: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceOverlapCandidate {
    path: String,
    score: i64,
}

fn graph_expand_candidates(
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

fn source_overlap_candidates(
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
    let mut remaining = budget_chars;
    let mut selected = Vec::new();
    // Spend the budget newest-first so recent context is never displaced by
    // old turns, then reverse the selected suffix for chronological rendering.
    for message in session.messages.iter().rev() {
        if remaining == 0 {
            break;
        }
        let label = match message.role {
            crate::models::chat::ChatRole::User => "User",
            crate::models::chat::ChatRole::Assistant => "Assistant",
        };
        let line = format!("{label}: {}\n", message.content.trim());
        let (mut bounded, used) = take_chars(&line, remaining);
        if used < char_len(&line) && !bounded.ends_with('\n') {
            // A truncated older chunk will be rendered before newer selected
            // turns. Replace its final budgeted character with a separator so
            // role labels cannot concatenate, without exceeding the budget.
            bounded.pop();
            if !bounded.is_empty() {
                bounded.push('\n');
            }
        }
        if bounded.trim().is_empty() {
            break;
        }
        selected.push(bounded);
        remaining = remaining.saturating_sub(used);
    }
    if selected.is_empty() {
        return;
    }
    prompt.push_str("\n--- Conversation so far ---\n");
    for line in selected.into_iter().rev() {
        prompt.push_str(&line);
    }
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

#[cfg(test)]
mod tests {
    use super::super::test_support::{seed_vault, tmp_context, write_file};
    use super::ChatService;
    use crate::models::chat::{ChatMessage, ChatRole, ChatRoute, ChatSourceSelectionReason};
    use crate::services::SearchService;
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

        // Required context is first, followed by the full pinned page; keyword
        // candidates follow. Later graph/source-overlap stages are separately
        // bounded and all stages contribute diagnostics before prompt budgets.
        assert_eq!(ctx.source_refs[0].page_path, "wiki/index.md");
        assert_eq!(
            ctx.source_refs[1].page_path,
            "wiki/concepts/agent-memory.md"
        );
        assert!(ctx.source_refs[1].is_pinned);
        let keyword_position = ctx
            .source_refs
            .iter()
            .position(|source| source.page_path == "wiki/concepts/react-pattern.md")
            .unwrap();
        assert!(keyword_position > 1);
        let purpose_position = ctx.prompt.find("--- Wiki purpose ---").unwrap();
        let index_position = ctx.prompt.find("wiki/index.md").unwrap();
        let pinned_position = ctx.prompt.find("wiki/concepts/agent-memory.md").unwrap();
        assert!(purpose_position < index_position && index_position < pinned_position);
        for (path, reason) in [
            ("wiki/index.md", ChatSourceSelectionReason::Index),
            (
                "wiki/concepts/agent-memory.md",
                ChatSourceSelectionReason::Pinned,
            ),
            (
                "wiki/concepts/react-pattern.md",
                ChatSourceSelectionReason::KeywordHit,
            ),
        ] {
            assert!(
                ctx.diagnostics
                    .expanded_pages
                    .iter()
                    .any(|selected| selected.path == path && selected.reason == reason),
                "diagnostics must preserve the {reason:?} selection reason for {path}"
            );
        }
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
        let pinned = ctx
            .source_refs
            .iter()
            .find(|source| source.page_path == "wiki/concepts/huge-pinned.md")
            .expect("pinned page remains represented even under a tight budget");
        assert!(pinned.excerpt.as_deref().is_some_and(|excerpt| !excerpt.is_empty()));
        assert!(ctx.diagnostics.omitted_pages.iter().any(|path| path == "wiki/index.md"));

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
        assert!(ctx
            .diagnostics
            .expanded_pages
            .iter()
            .any(|expanded| expanded.reason == ChatSourceSelectionReason::GraphNeighbor));
        assert!(
            ctx.diagnostics
                .expanded_pages
                .iter()
                .filter(|expanded| expanded.reason == ChatSourceSelectionReason::GraphNeighbor)
                .count()
                <= super::MAX_GRAPH_EXPANSIONS
        );
        assert!(
            ctx.diagnostics
                .expanded_pages
                .iter()
                .filter(|expanded| expanded.reason == ChatSourceSelectionReason::SourceOverlap)
                .count()
                <= super::MAX_SOURCE_OVERLAP_EXPANSIONS
        );
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
        let skill = include_str!("../../../templates/skills/wiki-query/SKILL.md");

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
    fn retrieval_context_assembles_citations_purpose_and_bounded_history() {
        let (context, root) = tmp_context("retrieval");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();

        // Seed history that genuinely exceeds the minimum 500-character
        // history budget. Selection must keep the newest turns, then render
        // the selected suffix in chronological order.
        let mut session = service.create_session(&context, None, None).unwrap();
        for i in 0..15 {
            let content = match i {
                0 => format!("OLDEST_TURN_0: {}", "old history ".repeat(50)),
                13 => format!("PARTIAL_OLDER_13: {}", "partial history ".repeat(50)),
                14 => "RECENT_TURN_14: short newest turn".to_string(),
                _ => format!("MIDDLE_TURN_{i}: {}", "middle history ".repeat(50)),
            };
            session.messages.push(ChatMessage {
                id: format!("old-{i}"),
                role: if i % 2 == 0 {
                    ChatRole::User
                } else {
                    ChatRole::Assistant
                },
                content,
                created_at: "2026-06-01T00:00:00Z".into(),
                citations: Vec::new(),
                route: None,
                provider: None,
                task_id: None,
                convenience_edit: None,
                retrieval_diagnostics: None,
                saved_path: None,
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
                Some(800),
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
        let conversation = ctx
            .prompt
            .split("--- Conversation so far ---\n")
            .nth(1)
            .unwrap()
            .split("\n--- Latest question ---")
            .next()
            .unwrap();
        let older_position = conversation.find("PARTIAL_OLDER_13").unwrap();
        let recent_position = conversation.find("RECENT_TURN_14").unwrap();
        assert!(older_position < recent_position);
        assert!(
            conversation.contains("\nUser: RECENT_TURN_14"),
            "a partial older message must end before the newest role label: {conversation:?}"
        );
        assert!(conversation.chars().count() <= ctx.diagnostics.history_budget_chars);
        assert!(!conversation.contains("OLDEST_TURN_0"));

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
    fn retrieval_context_marks_index_as_pinned_when_it_is_current_page() {
        let (context, root) = tmp_context("retrieval-pinned-index");
        seed_vault(&context);
        let service = ChatService::default();
        let search = SearchService::default();
        let session = service.create_session(&context, None, None).unwrap();

        let ctx = service
            .build_retrieval_context(
                &context,
                &search,
                "wiki overview",
                &session,
                "en",
                ChatRoute::Byok,
                Some(8_000),
                Some("wiki/index.md"),
            )
            .unwrap();

        let index = ctx
            .source_refs
            .iter()
            .find(|source| source.page_path == "wiki/index.md")
            .expect("index should remain in retrieved sources");
        assert!(index.is_pinned);
        assert_eq!(
            ctx.source_refs
                .iter()
                .filter(|source| source.page_path == "wiki/index.md")
                .count(),
            1
        );

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
}
