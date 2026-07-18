use std::collections::HashMap;

use crate::errors::BackendError;
use crate::models::chat::ChatRetrievalHit;
use crate::models::paths::ProjectContext;
use crate::models::search::SearchRequest;

use super::SearchService;

impl SearchService {
    /// Retrieve the top wiki pages for a natural-language chat question, each
    /// with a bounded body excerpt for the model prompt. Reuses the keyword
    /// `search` index (no model is called). The excerpt is derived from the
    /// cached `WikiIndex` body (no per-result `read_page` re-read), so a chat
    /// retrieval after a search pays zero extra file reads. This is the
    /// chat-retrieval entry point; the global search command stays
    /// keyword-only and never calls this for autocomplete.
    pub fn retrieve_with_excerpts(
        &self,
        context: &ProjectContext,
        query: &str,
        limit: usize,
        excerpt_chars: usize,
    ) -> Result<Vec<ChatRetrievalHit>, BackendError> {
        let request = SearchRequest {
            project_id: context.project_id.clone(),
            project_root_path: context.root.to_string_lossy().to_string(),
            query: Some(query.to_string()),
            page_types: Vec::new(),
            tags: Vec::new(),
            source: None,
            limit: Some(limit),
        };
        let response = self.search(context, &request)?;
        // Search refreshed this snapshot. `entries` clones cached entries and
        // does not read Markdown again, preserving the retrieval hot path.
        let cached = self.index.entries(context)?;
        let by_path: HashMap<&str, &str> = cached
            .iter()
            .map(|entry| (entry.path.as_str(), entry.body_markdown.as_str()))
            .collect();
        let mut hits = Vec::with_capacity(response.results.len());
        for result in response.results {
            let excerpt = by_path
                .get(result.path.as_str())
                .map(|body| truncate_excerpt(body, excerpt_chars));
            hits.push(ChatRetrievalHit {
                path: result.path,
                title: result.title,
                snippet: result.snippet,
                score: result.score,
                excerpt,
                is_pinned: false,
            });
        }
        Ok(hits)
    }
}

pub(super) fn first_body_excerpt(body: &str, max_chars: usize) -> Option<String> {
    let line = body.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(truncate_excerpt(line, max_chars))
}

/// Bound a page excerpt to at most `max_chars` body characters plus an
/// optional ellipsis, keeping chat prompts within a sane token budget.
fn truncate_excerpt(body: &str, max_chars: usize) -> String {
    let trimmed = body.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let taken: String = trimmed.chars().take(max_chars).collect();
    let cut = taken.trim_end();
    let nearest_break = cut.rfind(['\n', '.']).map(|i| &cut[..=i]).unwrap_or(cut);
    format!("{}…", nearest_break.trim_end())
}

#[cfg(test)]
mod tests {
    use super::SearchService;
    use crate::services::search_service::test_support::{
        seed_chinese_question_page, tmp_context, write_file,
    };

    #[test]
    fn retrieve_with_excerpts_handles_chinese_question_suffix() {
        let (context, root) = tmp_context("retrieve-cjk");
        seed_chinese_question_page(&context);
        let service = SearchService::default();
        const EXCERPT_BODY_CHARS: usize = 16;

        let hits = service
            .retrieve_with_excerpts(&context, "约束先行是什么？", 3, EXCERPT_BODY_CHARS)
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "wiki/concepts/constraints-first.md");
        assert!(hits[0].excerpt.as_deref().unwrap().contains("约束先行"));
        assert!(hits[0].excerpt.as_deref().unwrap().ends_with('…'));
        assert!(hits[0].excerpt.as_deref().unwrap().chars().count() <= EXCERPT_BODY_CHARS + 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieve_with_excerpts_does_not_slice_inside_cjk_prefix() {
        let (context, root) = tmp_context("retrieve-cjk-boundary");
        let prefix = format!("{}🙂{}x", "前".repeat(20), "后".repeat(20));
        write_file(
            &context,
            "wiki/concepts/cjk-boundary.md",
            &format!("---\ntitle: CJK boundary\n---\n\n{prefix}命令后缀"),
        );

        let service = SearchService::default();
        let hits = service
            .retrieve_with_excerpts(&context, "命令", 3, 1200)
            .unwrap();

        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.as_deref().unwrap().contains("命令"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod index_integration_tests {
    use std::collections::HashSet;

    use super::SearchService;
    use crate::models::search::SearchRequest;
    use crate::services::search_service::test_support::{
        seed_index as seed, tmp_index_context as tmp_context,
    };

    #[test]
    fn scan_search_and_retrieve_share_one_index_snapshot() {
        let (context, root) = tmp_context("shared");
        seed(&context);
        let service = SearchService::default();
        let bookmarks = HashSet::new();

        let tree = service.scan_wiki(&context, &bookmarks).unwrap();
        assert_eq!(tree.total_pages, 3);

        let request = SearchRequest {
            project_id: context.project_id.clone(),
            project_root_path: context.root.to_string_lossy().to_string(),
            query: Some("agent".to_string()),
            page_types: Vec::new(),
            tags: Vec::new(),
            source: None,
            limit: None,
        };
        let response = service.search(&context, &request).unwrap();
        assert!(response
            .results
            .iter()
            .any(|result| result.path == "wiki/concepts/agent.md"));

        let hits = service
            .retrieve_with_excerpts(&context, "agent", 5, 80)
            .unwrap();
        let agent_hit = hits
            .iter()
            .find(|hit| hit.path == "wiki/concepts/agent.md")
            .unwrap();
        assert!(agent_hit
            .excerpt
            .as_deref()
            .unwrap()
            .contains("short context"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn retrieve_with_excerpts_reuses_cached_body_and_does_not_reread() {
        let (context, root) = tmp_context("retrieve-no-reread");
        seed(&context);
        let service = SearchService::default();

        let hits = service
            .retrieve_with_excerpts(&context, "agent", 5, 80)
            .unwrap();
        let agent = hits
            .iter()
            .find(|hit| hit.path == "wiki/concepts/agent.md")
            .unwrap();
        assert!(agent.excerpt.as_deref().unwrap().contains("short context"));

        let again = service
            .retrieve_with_excerpts(&context, "agent", 5, 80)
            .unwrap();
        let agent_again = again
            .iter()
            .find(|hit| hit.path == "wiki/concepts/agent.md")
            .unwrap();
        assert_eq!(agent.excerpt, agent_again.excerpt);

        std::fs::remove_dir_all(root).unwrap();
    }
}
