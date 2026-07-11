use std::collections::{HashMap, HashSet};

use super::{ChatService, ParsedModelCitations};
use crate::models::chat::{ChatCitation, ChatSourceRef};

impl ChatService {
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
}

fn is_source_marker_id(value: &str) -> bool {
    value
        .strip_prefix('S')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::source_ref;
    use super::ChatService;

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
}
