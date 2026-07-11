use std::collections::HashSet;

use crate::errors::BackendError;
use crate::models::paths::ProjectContext;
use crate::models::search::{SearchRequest, SearchResponse, SearchResult};
use crate::models::wiki::WikiPageType;
use crate::utils::markdown_utils::snippet_for_query;

use super::{excerpts::first_body_excerpt, SearchService};

impl SearchService {
    /// Local keyword/tag/type/source search.
    ///
    /// Reuses the per-project `WikiIndex` cache so repeated searches do not
    /// re-read unchanged Markdown: the index refreshes once (mtime/size
    /// invalidation), then search scores against the cached bodies/metas.
    /// Bookmarks are intentionally not joined here (the global search command
    /// passes an empty set, matching the pre-index behavior).
    pub fn search(
        &self,
        context: &ProjectContext,
        request: &SearchRequest,
    ) -> Result<SearchResponse, BackendError> {
        let entries = self.index.refresh(context, &self.file_store)?;

        let query_terms = request
            .query
            .as_deref()
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(extract_query_terms)
            .filter(|terms| !terms.is_empty());
        let type_filter: HashSet<WikiPageType> = request.page_types.iter().copied().collect();
        let tag_filter: Vec<String> = request
            .tags
            .iter()
            .map(|tag| normalize_for_search(tag))
            .collect();
        let source_filter = request
            .source
            .as_deref()
            .map(str::trim)
            .filter(|source| !source.is_empty())
            .map(normalize_for_search);

        let mut results: Vec<SearchResult> = Vec::new();

        for entry in &entries {
            let meta = &entry.meta;
            let body = &entry.body_markdown;

            if !type_filter.is_empty() && !type_filter.contains(&meta.page_type) {
                continue;
            }
            if !tag_filter.is_empty()
                && !meta
                    .tags
                    .iter()
                    .any(|tag| tag_filter.contains(&normalize_for_search(tag)))
            {
                continue;
            }
            if let Some(ref source_needle) = source_filter {
                let has_source = meta
                    .sources
                    .iter()
                    .any(|source| normalize_for_search(source).contains(source_needle));
                if !has_source {
                    continue;
                }
            }

            let (matched_fields, snippet, score) = match query_terms.as_deref() {
                Some(terms) => {
                    let mut fields: Vec<&'static str> = Vec::new();
                    let mut score = 0i64;

                    if let Some(field_score) = score_field(&meta.title, terms, 120, 80) {
                        fields.push("title");
                        score += field_score;
                    }
                    let tags = meta.tags.join(" ");
                    if let Some(field_score) = score_field(&tags, terms, 0, 35) {
                        fields.push("tags");
                        score += field_score;
                    }
                    let sources = meta.sources.join(" ");
                    if let Some(field_score) = score_field(&sources, terms, 0, 25) {
                        fields.push("sources");
                        score += field_score;
                    }
                    let aliases = meta.aliases.join(" ");
                    if let Some(field_score) = score_field(&aliases, terms, 70, 45) {
                        fields.push("aliases");
                        score += field_score;
                    }

                    if let Some(field_score) = score_field(body, terms, 18, 8) {
                        fields.push("content");
                        score += field_score;
                    }
                    if let Some(field_score) = score_field(&meta.path, terms, 0, 20) {
                        fields.push("path");
                        score += field_score;
                    }

                    if fields.is_empty() {
                        continue;
                    }

                    let snippet = first_matching_term(body, terms)
                        .and_then(|term| snippet_for_query(body, &term, 48))
                        .or_else(|| first_body_excerpt(body, 96));
                    let fields_owned: Vec<String> = fields.into_iter().map(String::from).collect();
                    (fields_owned, snippet, score)
                }
                None => (Vec::new(), None, 0),
            };

            results.push(SearchResult {
                path: meta.path.clone(),
                title: meta.title.clone(),
                page_type: meta.page_type,
                starred: meta.starred,
                matched_fields,
                snippet,
                score,
            });
        }

        if query_terms.is_some() {
            results.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.path.cmp(&b.path)));
        } else {
            results.sort_by(|a, b| a.path.cmp(&b.path));
        }

        let total = results.len();
        if let Some(limit) = request.limit {
            results.truncate(limit);
        }

        Ok(SearchResponse { results, total })
    }
}

fn normalize_for_search(value: &str) -> String {
    let mut normalized = String::new();
    let mut last_was_space = true;

    for ch in value.to_lowercase().chars() {
        if ch.is_alphanumeric() || is_cjk(ch) {
            normalized.push(ch);
            last_was_space = false;
        } else if !last_was_space {
            normalized.push(' ');
            last_was_space = true;
        }
    }

    let mut trimmed = normalized.trim().to_string();
    for prefix in ["什么是", "请解释", "解释一下"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            trimmed = rest.trim().to_string();
            break;
        }
    }
    for suffix in ["是什么", "？", "?", "吗", "呢"] {
        if let Some(rest) = trimmed.strip_suffix(suffix) {
            trimmed = rest.trim().to_string();
            break;
        }
    }
    trimmed
}

fn extract_query_terms(query: &str) -> Vec<String> {
    let normalized = normalize_for_search(query);
    let mut terms: Vec<String> = Vec::new();
    push_unique_term(&mut terms, normalized.clone());

    let mut cjk_run = String::new();
    let mut ascii_run = String::new();
    for ch in normalized.chars() {
        if is_cjk(ch) {
            flush_ascii_run(&mut terms, &mut ascii_run);
            cjk_run.push(ch);
        } else if ch.is_ascii_alphanumeric() {
            flush_cjk_run(&mut terms, &mut cjk_run);
            ascii_run.push(ch);
        } else {
            flush_cjk_run(&mut terms, &mut cjk_run);
            flush_ascii_run(&mut terms, &mut ascii_run);
        }
    }
    flush_cjk_run(&mut terms, &mut cjk_run);
    flush_ascii_run(&mut terms, &mut ascii_run);

    let base_terms: Vec<String> = terms
        .iter()
        .filter_map(|term| strip_trailing_ascii_digits(term))
        .collect();
    for term in base_terms {
        push_unique_term(&mut terms, term);
    }

    terms
}

fn score_field(
    field: &str,
    terms: &[String],
    exact_phrase_weight: i64,
    term_weight: i64,
) -> Option<i64> {
    let haystack = normalize_for_search(field);
    if haystack.is_empty() {
        return None;
    }

    let mut score = 0i64;
    if let Some(phrase) = terms.first() {
        if !phrase.is_empty() && haystack.contains(phrase) {
            score += exact_phrase_weight;
        }
    }
    for term in terms {
        if !term.is_empty() && haystack.contains(term) {
            score += term_weight;
        }
    }

    (score > 0).then_some(score)
}

fn first_matching_term(field: &str, terms: &[String]) -> Option<String> {
    let haystack = normalize_for_search(field);
    terms
        .iter()
        .find(|term| !term.is_empty() && haystack.contains(term.as_str()))
        .cloned()
}

fn push_unique_term(terms: &mut Vec<String>, term: String) {
    let term = term.trim();
    if term.chars().count() >= 2 && !terms.iter().any(|existing| existing == term) {
        terms.push(term.to_string());
    }
}

fn flush_cjk_run(terms: &mut Vec<String>, run: &mut String) {
    if run.chars().count() >= 2 {
        push_unique_term(terms, run.clone());
    }
    run.clear();
}

fn flush_ascii_run(terms: &mut Vec<String>, run: &mut String) {
    if run.chars().count() >= 2 {
        push_unique_term(terms, run.clone());
    }
    run.clear();
}

fn strip_trailing_ascii_digits(term: &str) -> Option<String> {
    let stripped = term.trim_end_matches(|ch: char| ch.is_ascii_digit());
    if stripped != term && stripped.chars().count() >= 2 {
        Some(stripped.to_string())
    } else {
        None
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2A6DF
            | 0x2A700..=0x2B73F
            | 0x2B740..=0x2B81F
            | 0x2B820..=0x2CEAF
    )
}

#[cfg(test)]
mod tests {
    use super::SearchService;
    use crate::models::search::{SearchRequest, SearchResponse};
    use crate::models::wiki::WikiPageType;
    use crate::services::search_service::test_support::{
        search_request, seed_chinese_question_page, seed_sample_vault, tmp_context, write_file,
    };

    #[test]
    fn search_filters_by_type_tag_source_and_keyword() {
        let (context, root) = tmp_context("search");
        seed_sample_vault(&context);
        let service = SearchService::default();

        let only_entities = service
            .search(
                &context,
                &SearchRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    query: None,
                    page_types: vec![WikiPageType::Entity],
                    tags: Vec::new(),
                    source: None,
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(only_entities.total, 1);
        assert_eq!(only_entities.results[0].path, "wiki/entities/claude.md");

        let only_memory = service
            .search(
                &context,
                &SearchRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    query: None,
                    page_types: Vec::new(),
                    tags: vec!["memory".to_string()],
                    source: None,
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(only_memory.total, 1);
        assert_eq!(only_memory.results[0].path, "wiki/concepts/agent-memory.md");

        let by_source = service
            .search(
                &context,
                &SearchRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    query: None,
                    page_types: Vec::new(),
                    tags: Vec::new(),
                    source: Some("paper.md".to_string()),
                    limit: None,
                },
            )
            .unwrap();
        assert_eq!(by_source.total, 1);

        let keyword = service
            .search(
                &context,
                &SearchRequest {
                    project_id: "p".into(),
                    project_root_path: context.root.to_string_lossy().to_string(),
                    query: Some("react".to_string()),
                    page_types: Vec::new(),
                    tags: Vec::new(),
                    source: None,
                    limit: None,
                },
            )
            .unwrap();
        let paths: Vec<&str> = keyword.results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"wiki/concepts/react-pattern.md"));
        assert!(paths.contains(&"wiki/concepts/agent-memory.md"));
        assert_eq!(keyword.results[0].path, "wiki/concepts/react-pattern.md");
        assert!(keyword.results[0]
            .matched_fields
            .contains(&"title".to_string()));

        let _ = SearchResponse::empty();

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_matches_chinese_question_by_extracted_title_term() {
        let (context, root) = tmp_context("search-cjk-title");
        seed_chinese_question_page(&context);
        let service = SearchService::default();

        let response = service
            .search(&context, &search_request(&context, "什么是约束先行2？"))
            .unwrap();

        assert_eq!(response.total, 1);
        assert_eq!(
            response.results[0].path,
            "wiki/concepts/constraints-first.md"
        );
        assert!(response.results[0]
            .matched_fields
            .contains(&"title".to_string()));
        assert!(response.results[0]
            .matched_fields
            .contains(&"aliases".to_string()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_uses_unicode_lowercase_not_ascii_only() {
        let (context, root) = tmp_context("search-unicode-lower");
        write_file(
            &context,
            "wiki/concepts/eclair.md",
            "---\ntitle: Éclair Guide\n---\n\n# Éclair Guide\n\nDessert notes.",
        );
        let service = SearchService::default();

        let response = service
            .search(&context, &search_request(&context, "éclair"))
            .unwrap();

        assert_eq!(response.total, 1);
        assert_eq!(response.results[0].path, "wiki/concepts/eclair.md");
        assert!(response.results[0]
            .matched_fields
            .contains(&"title".to_string()));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_prefers_exact_title_or_alias_over_body_term() {
        let (context, root) = tmp_context("search-cjk-ranking");
        seed_chinese_question_page(&context);
        write_file(
            &context,
            "wiki/concepts/body-mention.md",
            "---\ntitle: Body Mention\n---\n\n# Body Mention\n\n什么是约束先行2 是正文里的一个问题。",
        );
        let service = SearchService::default();

        let response = service
            .search(&context, &search_request(&context, "什么是约束先行2？"))
            .unwrap();

        assert_eq!(response.total, 2);
        assert_eq!(
            response.results[0].path,
            "wiki/concepts/constraints-first.md"
        );
        assert!(response.results[0].score > response.results[1].score);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_returns_no_hits_for_truly_unmatched_question() {
        let (context, root) = tmp_context("search-cjk-none");
        seed_chinese_question_page(&context);
        let service = SearchService::default();

        let response = service
            .search(
                &context,
                &search_request(&context, "什么是完全不存在的概念？"),
            )
            .unwrap();

        assert_eq!(response.total, 0);
        assert!(response.results.is_empty());

        std::fs::remove_dir_all(root).unwrap();
    }
}
