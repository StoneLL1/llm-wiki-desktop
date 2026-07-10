use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::{ChatService, ParsedModelCitations};
use crate::models::chat::{ChatCitation, ChatMessage, ChatSession, ChatSourceRef};
use crate::utils::markdown_utils::slugify_query;

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

    /// Render an assistant message as a `wiki/queries/` Markdown page. Sources
    /// are taken only from citations already parsed from actual model markers.
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
}

fn is_source_marker_id(value: &str) -> bool {
    value
        .strip_prefix('S')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
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

#[cfg(test)]
mod tests {
    use super::super::test_support::{source_ref, tmp_context, user_message};
    use super::ChatService;
    use crate::models::chat::{ChatCitation, ChatMessage, ChatRole};

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
        let (_, markdown) = service.build_answer_markdown(&session, &question, &answer);
        assert!(markdown.contains("  - wiki/model-used.md"));
        assert!(!markdown.contains("wiki/retrieved-only.md"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
