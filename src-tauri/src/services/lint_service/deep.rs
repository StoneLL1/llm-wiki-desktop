use std::collections::{HashMap, HashSet};

use crate::errors::BackendError;
use crate::models::lint::{Fixability, LintAgentIssue, LintIssue, LintIssueSource, LintSeverity};
use crate::models::paths::ProjectContext;
use crate::services::SearchService;

use super::rules::lint_issue_type_id;
use super::LintService;

const DEEP_LINT_EXCERPT_CHARS: usize = 1000;

impl LintService {
    /// Assemble the prompt for the `wiki-lint` Skill: purpose, schema, and a
    /// per-page summary with a bounded excerpt. No secret or API key is ever
    /// placed in the prompt.
    pub fn build_deep_lint_prompt(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        language: &str,
    ) -> Result<String, BackendError> {
        let tree = search_service.scan_wiki(context, &HashSet::new())?;
        let purpose = self.file_store.read_markdown(context, "purpose.md").ok();
        let schema = self.file_store.read_markdown(context, "schema.md").ok();
        let local_baseline = self.run_local_lint(context, search_service)?;
        // `language` is read by the command layer from SettingsService so this
        // service stays host-state-free and testable. The suggestion prose
        // follows the user's language; the JSON contract (issueType enum,
        // ```json fence) stays English so parsing is stable.

        let mut prompt = String::new();
        prompt.push_str(
            "You are linting a local Markdown wiki for structural quality. Judge the wiki \
             across these dimensions only: duplicate_topic, weak_cross_reference, \
             missing_source, schema_mismatch, outdated_content, contradiction. Use the \
             page paths exactly as given. Respond with ONLY a fenced JSON block (```json) \
             containing an array of objects with fields: issueType (one of the six above), \
             severity (error|warning|info), path, message, evidence, suggestion. If there \
             are no issues, respond with an empty array. Do not repeat deterministic local \
             findings listed in the baseline section.\n\n\
             Severity rubric: error means deterministic broken navigation, index, or \
             source-traceability failure; warning means likely duplicate, merge, schema, \
             citation, stale, or contradiction issue with concrete evidence; info means a \
             suggestion or low-confidence gap without direct breakage. Evidence is required \
             for error severity.\n",
        );
        prompt.push_str(&crate::utils::i18n::language_instruction(language));
        prompt.push_str(
            " Write the `message` and `suggestion` text in that language; keep issueType, \
             severity, path, and the JSON structure in English.\n",
        );
        if let Some(purpose) = &purpose {
            prompt.push_str("\n--- Purpose ---\n");
            prompt.push_str(purpose.trim());
            prompt.push('\n');
        }
        if let Some(schema) = &schema {
            prompt.push_str("\n--- Schema ---\n");
            prompt.push_str(schema.trim());
            prompt.push('\n');
        }
        prompt.push_str("\n--- Local deterministic findings already detected ---\n");
        if local_baseline.issues.is_empty() {
            prompt.push_str("None.\n");
        } else {
            for issue in &local_baseline.issues {
                prompt.push_str(&format!(
                    "- {} | {:?} | {:?} | {} | {}\n",
                    issue.path, issue.issue_type, issue.severity, issue.id, issue.message
                ));
            }
        }
        prompt.push_str("\n--- Pages ---\n");
        for page in &tree.pages {
            if page.path == "wiki/log.md" {
                continue;
            }
            prompt.push_str(&format!(
                "\n### {} ({:?})\npath: {}\ntags: {}\n",
                page.title,
                page.page_type,
                page.path,
                page.tags.join(", ")
            ));
            if let Ok(content) = search_service.read_page(context, &page.path, &HashSet::new()) {
                let excerpt = truncate_chars(&content.body_markdown, DEEP_LINT_EXCERPT_CHARS);
                if !excerpt.is_empty() {
                    prompt.push_str(excerpt.trim());
                    prompt.push('\n');
                }
            }
        }
        Ok(prompt)
    }

    /// Parse the structured ` ```json ` block emitted by the `wiki-lint` Skill
    /// into typed issues. Surrounding prose is ignored; a missing block yields
    /// an empty list.
    pub fn parse_agent_issues(raw: &str) -> Result<Vec<LintIssue>, BackendError> {
        let json = extract_json_block(raw);
        let Some(json) = json else {
            return Ok(Vec::new());
        };
        let parsed: Vec<LintAgentIssue> = serde_json::from_str(&json).map_err(|err| {
            BackendError::new(
                "LINT_AGENT_OUTPUT_INVALID",
                format!("Could not parse deep-lint JSON: {err}"),
                true,
                false,
            )
        })?;
        Ok(Self::normalize_agent_issues(parsed, None, &HashSet::new()))
    }

    pub fn parse_agent_issues_for_known_paths(
        raw: &str,
        known_paths: &HashSet<String>,
        deterministic_issue_ids: &HashSet<String>,
    ) -> Result<Vec<LintIssue>, BackendError> {
        let json = extract_json_block(raw);
        let Some(json) = json else {
            return Ok(Vec::new());
        };
        let parsed: Vec<LintAgentIssue> = serde_json::from_str(&json).map_err(|err| {
            BackendError::new(
                "LINT_AGENT_OUTPUT_INVALID",
                format!("Could not parse deep-lint JSON: {err}"),
                true,
                false,
            )
        })?;
        Ok(Self::normalize_agent_issues(
            parsed,
            Some(known_paths),
            deterministic_issue_ids,
        ))
    }

    fn normalize_agent_issues(
        parsed: Vec<LintAgentIssue>,
        known_paths: Option<&HashSet<String>>,
        deterministic_issue_ids: &HashSet<String>,
    ) -> Vec<LintIssue> {
        // Disambiguate ids when the same issue type lands on the same page
        // multiple times (otherwise the frontend's fixStatus/selection map
        // collapses them). Append a per-(type,path) counter only when needed.
        let mut seen: HashMap<String, usize> = HashMap::new();
        parsed
            .into_iter()
            .filter_map(|agent| {
                let path = agent.path.trim().replace('\\', "/");
                if path.is_empty()
                    || path.contains("..")
                    || known_paths.is_some_and(|paths| !paths.contains(&path))
                {
                    return None;
                }
                let base = format!("{}:{path}", lint_issue_type_id(agent.issue_type));
                if deterministic_issue_ids.contains(&base) {
                    return None;
                }
                let count = seen.entry(base.clone()).or_insert(0);
                *count += 1;
                let id = if *count > 1 {
                    format!("{base}:{}", count)
                } else {
                    base
                };
                let evidence = agent.evidence.and_then(|value| {
                    let trimmed = value.trim().to_string();
                    (!trimmed.is_empty()).then_some(trimmed)
                });
                let severity = if agent.severity == LintSeverity::Error && evidence.is_none() {
                    LintSeverity::Warning
                } else {
                    agent.severity
                };
                Some(LintIssue {
                    id,
                    source: LintIssueSource::Agent,
                    severity,
                    issue_type: agent.issue_type,
                    path,
                    range: None,
                    message: agent.message,
                    evidence,
                    target: None,
                    // Agent issues are judgment calls; none are auto-fixable.
                    fixability: Fixability::None,
                    suggested_action: agent.suggestion,
                })
            })
            .collect()
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let taken: String = trimmed.chars().take(max_chars).collect();
    format!("{}…", taken.trim_end())
}

fn extract_json_block(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find("```json") {
        let rest = &trimmed[start + 7..];
        let end = rest.find("```")?;
        return Some(rest[..end].trim().to_string());
    }
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            if end > start {
                let candidate = &trimmed[start..=end];
                // Only trust the bare-bracket fallback if it is genuinely valid
                // JSON — otherwise prose like "see [A and B]" would make every
                // deep-lint run fail instead of returning an empty issue list.
                if serde_json::from_str::<Vec<LintAgentIssue>>(candidate).is_ok() {
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::models::lint::{Fixability, LintIssueType, LintSeverity};
    use crate::services::SearchService;

    use super::super::test_support::{seed_clean_vault, tmp_context, write_file};
    use super::super::LintService;

    #[test]
    fn parse_agent_issues_extracts_fenced_json() {
        let raw = "Here is my analysis.\n\n```json\n[\n  {\"issueType\": \"duplicate_topic\", \"severity\": \"warning\", \"path\": \"wiki/a.md\", \"message\": \"Overlaps\", \"evidence\": \"x\", \"suggestion\": \"merge\"}\n]\n```\nThanks.";
        let issues = LintService::parse_agent_issues(raw).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, LintIssueType::DuplicateTopic);
        assert_eq!(issues[0].severity, LintSeverity::Warning);
        assert_eq!(issues[0].suggested_action.as_deref(), Some("merge"));
        assert_eq!(issues[0].fixability, Fixability::None);

        // No block → empty list, not an error.
        assert!(LintService::parse_agent_issues("no json here")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn deep_lint_prompt_includes_purpose_and_pages() {
        let (context, root) = tmp_context("prompt");
        seed_clean_vault(&context);
        write_file(&context, "purpose.md", "# Purpose\n\nExplain agents.");
        write_file(&context, "schema.md", "# Schema\n\nPages need a type.");

        let prompt = LintService::default()
            .build_deep_lint_prompt(&context, &SearchService::default(), "en")
            .unwrap();
        assert!(prompt.contains("Purpose"));
        assert!(prompt.contains("Explain agents."));
        assert!(prompt.contains("Schema"));
        assert!(prompt.contains("wiki/concepts/agent.md"));
        assert!(prompt.contains("Local deterministic findings already detected"));
        assert!(prompt.contains("Severity rubric"));
        assert!(prompt.contains("Do not repeat deterministic local findings"));
        assert!(prompt.contains("```json"));
        assert!(prompt.contains("Respond in English."));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_issue_normalization_rejects_unknown_paths_and_downgrades_evidence_free_errors() {
        let raw = "```json\n[\n  {\"issueType\":\"duplicate_topic\",\"severity\":\"error\",\"path\":\"wiki/concepts/agent.md\",\"message\":\"Overlap\",\"evidence\":\"\",\"suggestion\":\"merge\"},\n  {\"issueType\":\"contradiction\",\"severity\":\"warning\",\"path\":\"wiki/missing.md\",\"message\":\"Invented\",\"evidence\":\"x\",\"suggestion\":\"check\"},\n  {\"issueType\":\"missing_source\",\"severity\":\"warning\",\"path\":\"wiki/concepts/agent.md\",\"message\":\"Duplicate deterministic\",\"evidence\":\"x\",\"suggestion\":\"add source\"}\n]\n```";
        let known_paths = HashSet::from(["wiki/concepts/agent.md".to_string()]);
        let deterministic_ids =
            HashSet::from(["missing_source:wiki/concepts/agent.md".to_string()]);

        let issues =
            LintService::parse_agent_issues_for_known_paths(raw, &known_paths, &deterministic_ids)
                .unwrap();

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].path, "wiki/concepts/agent.md");
        assert_eq!(issues[0].severity, LintSeverity::Warning);
        assert_eq!(issues[0].issue_type, LintIssueType::DuplicateTopic);
    }
}
