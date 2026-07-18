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
        let masked_answer = mask_code_spans(answer);
        let mut rest = masked_answer.as_str();
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

/// Keep citation markers inside Markdown code spans/fences from being treated
/// as claims made by the model. Newlines are preserved so the surrounding
/// scanner remains stable for multi-line answers.
fn mask_code_spans(answer: &str) -> String {
    let chars: Vec<char> = answer.chars().collect();
    let mut output = String::with_capacity(answer.len());
    let mut index = 0;
    let mut fence: Option<(char, usize)> = None;
    let mut inline_ticks: Option<usize> = None;
    while index < chars.len() {
        let delimiter = chars[index];
        let run_length = if delimiter == '`' || delimiter == '~' {
            let mut length = 0;
            while index + length < chars.len() && chars[index + length] == delimiter {
                length += 1;
            }
            length
        } else {
            0
        };

        if let Some((fence_delimiter, fence_length)) = fence {
            if delimiter == fence_delimiter && run_length >= fence_length {
                for _ in 0..run_length {
                    output.push(' ');
                }
                index += run_length;
                fence = None;
                continue;
            }
            output.push(if delimiter == '\n' { '\n' } else { ' ' });
            index += 1;
            continue;
        }
        if let Some(inline_length) = inline_ticks {
            if delimiter == '`' && run_length >= inline_length {
                for _ in 0..inline_length {
                    output.push(' ');
                }
                index += inline_length;
                inline_ticks = None;
                continue;
            }
            output.push(if delimiter == '\n' { '\n' } else { ' ' });
            index += 1;
            continue;
        }
        if run_length >= 3 {
            for _ in 0..run_length {
                output.push(' ');
            }
            fence = Some((delimiter, run_length));
            index += run_length;
            continue;
        }
        if delimiter == '`' && run_length > 0 {
            for _ in 0..run_length {
                output.push(' ');
            }
            inline_ticks = Some(run_length);
            index += run_length;
            continue;
        }
        if delimiter == '\n' {
            output.push(if chars[index] == '\n' { '\n' } else { ' ' });
        } else {
            output.push(chars[index]);
        }
        index += 1;
    }
    output
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

    #[test]
    fn citation_parser_ignores_markers_inside_code() {
        let refs = vec![source_ref("S1", "wiki/a.md", "A")];
        let parsed = ChatService::parse_model_citations(
            "Code `[S9]` and:\n```md\n[S8]\n```\nReal [S1].",
            &refs,
        );
        assert_eq!(parsed.citations.len(), 1);
        assert_eq!(parsed.citations[0].source_id.as_deref(), Some("S1"));
        assert!(parsed.invalid_source_ids.is_empty());
    }

    #[test]
    fn citation_parser_ignores_tilde_and_long_backtick_fences() {
        let refs = vec![source_ref("S1", "wiki/a.md", "A")];
        let parsed = ChatService::parse_model_citations(
            "~~~~md\n[S9]\n~~~~\n````md\n[S8]\n````\nReal [S1].",
            &refs,
        );
        assert_eq!(parsed.citations.len(), 1);
        assert_eq!(parsed.citations[0].source_id.as_deref(), Some("S1"));
        assert!(parsed.invalid_source_ids.is_empty());
    }
}
