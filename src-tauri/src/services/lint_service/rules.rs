use std::collections::{HashMap, HashSet};

use crate::errors::BackendError;
use crate::models::lint::{
    Fixability, LintIssue, LintIssueSource, LintIssueType, LintRange, LintReport, LintSeverity,
};
use crate::models::paths::ProjectContext;
use crate::models::wiki::{WikiPageMeta, WikiPageType};
use crate::services::SearchService;
use crate::utils::markdown_utils::{
    extract_wikilinks, parse_frontmatter, split_frontmatter, Frontmatter,
};
use crate::utils::time_utils::now_rfc3339;

use super::LintService;

/// Pages linked from index.md aren't "orphans" even though nothing links
/// back to them, and the structural pages themselves are never orphans.
const STRUCTURAL_FILES: &[&str] = &["wiki/index.md", "wiki/overview.md", "wiki/log.md"];

impl LintService {
    /// Run every local deterministic rule. No LLM or Agent is invoked.
    pub fn run_local_lint(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
    ) -> Result<LintReport, BackendError> {
        let tree = search_service.scan_wiki(context, &HashSet::new())?;
        let pages = tree.pages;
        let scanned = pages.len();

        let lookup = build_target_lookup(&pages);
        let inbound = build_inbound_counts(&pages, &lookup);

        let mut issues: Vec<LintIssue> = Vec::new();

        for page in &pages {
            let raw = self
                .file_store
                .read_markdown(context, &page.path)
                .unwrap_or_default();
            let split = split_frontmatter(&raw);
            let frontmatter_present = split.frontmatter.is_some();
            let frontmatter = split
                .frontmatter
                .as_deref()
                .map(parse_frontmatter)
                .unwrap_or_default();

            issues.extend(schema_source_issues(
                context,
                page,
                &split.body,
                &frontmatter,
            ));

            // Dead links.
            for target in &page.wikilinks {
                if is_external(target) {
                    continue;
                }
                let key = target.trim().to_ascii_lowercase();
                if lookup.contains_key(&key) {
                    continue;
                }
                let line = find_wikilink_line(&split.body, target);
                issues.push(LintIssue {
                    id: format!("dead_link:{}:{target}", page.path),
                    source: LintIssueSource::Local,
                    // Dead links break navigation — they are "must-fix", so the
                    // summary card's error count carries real data (PRD-LINT-001).
                    severity: LintSeverity::Error,
                    issue_type: LintIssueType::DeadLink,
                    path: page.path.clone(),
                    range: line.map(|l| LintRange {
                        line: l,
                        column: None,
                    }),
                    message: format!("Unresolved wikilink `[[{target}]]`."),
                    evidence: Some(format!("[[{target}]]")),
                    target: Some(target.clone()),
                    fixability: Fixability::HighRisk,
                    suggested_action: Some(
                        "Remove the link or fix the target to match an existing page.".into(),
                    ),
                });
            }

            // Missing frontmatter (structural files are exempt).
            if !frontmatter_present && !STRUCTURAL_FILES.contains(&page.path.as_str()) {
                issues.push(LintIssue {
                    id: format!("missing_frontmatter:{}", page.path),
                    source: LintIssueSource::Local,
                    severity: LintSeverity::Warning,
                    issue_type: LintIssueType::MissingFrontmatter,
                    path: page.path.clone(),
                    range: None,
                    message: "Page has no YAML frontmatter.".into(),
                    evidence: None,
                    target: None,
                    fixability: Fixability::Safe,
                    suggested_action: Some("Add a minimal frontmatter block.".into()),
                });
            }

            // Empty page.
            if page.word_count == 0 {
                issues.push(LintIssue {
                    id: format!("empty_page:{}", page.path),
                    source: LintIssueSource::Local,
                    severity: LintSeverity::Warning,
                    issue_type: LintIssueType::EmptyPage,
                    path: page.path.clone(),
                    range: None,
                    message: "Page body has no readable words.".into(),
                    evidence: None,
                    target: None,
                    fixability: Fixability::None,
                    suggested_action: Some("Add content or remove the page.".into()),
                });
            }

            // Missing resources referenced by `sources:`.
            for source in &page.sources {
                if is_external(source) {
                    continue;
                }
                if !resource_exists(context, source) {
                    issues.push(LintIssue {
                        id: format!("missing_resource:{}:{source}", page.path),
                        source: LintIssueSource::Local,
                        severity: LintSeverity::Warning,
                        issue_type: LintIssueType::MissingResource,
                        path: page.path.clone(),
                        range: None,
                        message: format!("Source reference `{source}` does not exist."),
                        evidence: None,
                        target: Some(source.clone()),
                        fixability: Fixability::None,
                        suggested_action: Some("Add the source file or correct the path.".into()),
                    });
                }
            }
        }

        // Orphan pages (no inbound links, not structural).
        for page in &pages {
            if STRUCTURAL_FILES.contains(&page.path.as_str()) {
                continue;
            }
            if inbound.get(page.path.as_str()).copied().unwrap_or(0) == 0 {
                issues.push(LintIssue {
                    id: format!("orphan_page:{}", page.path),
                    source: LintIssueSource::Local,
                    severity: LintSeverity::Info,
                    issue_type: LintIssueType::OrphanPage,
                    path: page.path.clone(),
                    range: None,
                    message: "No other page links to this page.".into(),
                    evidence: None,
                    target: None,
                    fixability: Fixability::None,
                    suggested_action: Some("Link it from a related page or the index.".into()),
                });
            }
        }

        // Duplicate filenames (same stem, different folders).
        let mut by_stem: HashMap<String, Vec<&WikiPageMeta>> = HashMap::new();
        for page in &pages {
            if let Some(stem) = file_stem(&page.path) {
                by_stem
                    .entry(stem.to_ascii_lowercase())
                    .or_default()
                    .push(page);
            }
        }
        for group in by_stem.values() {
            if group.len() < 2 {
                continue;
            }
            let colliding: Vec<String> = group.iter().map(|p| p.path.clone()).collect();
            for page in group {
                issues.push(LintIssue {
                    id: format!("duplicate_filename:{}", page.path),
                    source: LintIssueSource::Local,
                    severity: LintSeverity::Warning,
                    issue_type: LintIssueType::DuplicateFilename,
                    path: page.path.clone(),
                    range: None,
                    message: format!(
                        "Filename stem collides with {} other page(s).",
                        group.len() - 1
                    ),
                    evidence: Some(colliding.join(", ")),
                    target: None,
                    fixability: Fixability::None,
                    suggested_action: Some("Rename one of the pages to disambiguate.".into()),
                });
            }
        }

        // Path-case collisions (paths equal modulo ASCII case).
        let mut by_casefold: HashMap<String, Vec<&WikiPageMeta>> = HashMap::new();
        for page in &pages {
            by_casefold
                .entry(page.path.to_ascii_lowercase())
                .or_default()
                .push(page);
        }
        for group in by_casefold.values() {
            if group.len() < 2 {
                continue;
            }
            for page in group {
                issues.push(LintIssue {
                    id: format!("path_case:{}", page.path),
                    source: LintIssueSource::Local,
                    severity: LintSeverity::Warning,
                    issue_type: LintIssueType::PathCase,
                    path: page.path.clone(),
                    range: None,
                    message: "Path differs from another page only by letter case.".into(),
                    evidence: Some(
                        group
                            .iter()
                            .filter(|p| p.path != page.path)
                            .map(|p| p.path.clone())
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                    target: None,
                    fixability: Fixability::None,
                    suggested_action: Some(
                        "Rename so paths are unambiguous on case-insensitive filesystems.".into(),
                    ),
                });
            }
        }

        // Index drift (only when wiki/index.md exists).
        issues.extend(self.check_index_drift(context, &lookup));
        issues.extend(check_structural_page_basics(context, &pages));

        // Drop issues the user has dismissed via `.app/lint-ignore.json`. The
        // match key is (path, rule): ignoring a rule on a page suppresses every
        // occurrence of that rule on that page.
        let ignored_keys: HashSet<(String, LintIssueType)> = self
            .load_ignores(context)
            .ignored
            .into_iter()
            .map(|entry| (entry.path, entry.rule))
            .collect();
        if !ignored_keys.is_empty() {
            issues.retain(|issue| !ignored_keys.contains(&(issue.path.clone(), issue.issue_type)));
        }

        issues.sort_by(|a, b| {
            severity_rank(a.severity)
                .cmp(&severity_rank(b.severity))
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| format!("{:?}", a.issue_type).cmp(&format!("{:?}", b.issue_type)))
        });

        Ok(LintReport {
            issues,
            generated_at: now_rfc3339(),
            scanned_pages: scanned,
        })
    }

    fn check_index_drift(
        &self,
        context: &ProjectContext,
        lookup: &std::collections::HashMap<String, String>,
    ) -> Vec<LintIssue> {
        let mut issues = Vec::new();
        let Ok(raw) = self.file_store.read_markdown(context, "wiki/index.md") else {
            return issues;
        };
        let split = split_frontmatter(&raw);
        let linked: Vec<String> = extract_wikilinks(&split.body);

        // Ghost links: targets that resolve to no page (same resolution as
        // DeadLink, using build_target_lookup keys).
        for target in &linked {
            if is_external(target) || lookup.contains_key(&target.trim().to_ascii_lowercase()) {
                continue;
            }
            issues.push(LintIssue {
                id: format!("index_drift:wiki/index.md:{target}"),
                source: LintIssueSource::Local,
                // Index drift means the entry point references missing pages —
                // must-fix, surfaces in the error summary (PRD-LINT-001).
                severity: LintSeverity::Error,
                issue_type: LintIssueType::IndexDrift,
                path: "wiki/index.md".into(),
                range: None,
                message: format!("Index links to `{target}`, which does not exist."),
                evidence: Some(format!("[[{target}]]")),
                target: Some(target.clone()),
                fixability: Fixability::HighRisk,
                suggested_action: Some("Remove the stale link or create the page.".into()),
            });
        }
        issues
    }
}

/// Case-insensitive lookup from note-name/title/alias -> page path, mirroring
/// `graph_service::build_target_lookup`. Replicated here to avoid coupling
/// lint to graph internals.
fn build_target_lookup(pages: &[WikiPageMeta]) -> HashMap<String, String> {
    let mut lookup: HashMap<String, String> = HashMap::new();
    for page in pages {
        for key in resolution_keys(page) {
            lookup.entry(key).or_insert_with(|| page.path.clone());
        }
    }
    lookup
}

fn resolution_keys(page: &WikiPageMeta) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(stem) = file_stem(&page.path) {
        keys.push(stem.to_ascii_lowercase());
    }
    keys.push(page.title.trim().to_ascii_lowercase());
    for alias in &page.aliases {
        keys.push(alias.trim().to_ascii_lowercase());
    }
    keys
}

/// Count resolved inbound wikilinks per page (for orphan detection).
fn build_inbound_counts(
    pages: &[WikiPageMeta],
    lookup: &HashMap<String, String>,
) -> HashMap<String, usize> {
    let mut inbound: HashMap<String, usize> = HashMap::new();
    for page in pages {
        for target in &page.wikilinks {
            if is_external(target) {
                continue;
            }
            if let Some(resolved) = lookup.get(&target.trim().to_ascii_lowercase()) {
                if resolved != &page.path {
                    *inbound.entry(resolved.clone()).or_insert(0) += 1;
                }
            }
        }
    }
    inbound
}

pub(super) fn file_stem(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let file_name = normalized.rsplit('/').next()?;
    file_name
        .strip_suffix(".md")
        .map(|stem| stem.to_string())
        .or_else(|| Some(file_name.to_string()))
}

fn is_external(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.contains("://") || trimmed.starts_with("mailto:")
}

fn resource_exists(context: &ProjectContext, source: &str) -> bool {
    let normalized = source.replace('\\', "/");
    // Absolute paths and URLs are out of project scope; treat as present to
    // avoid false positives on references we cannot verify.
    if normalized.contains("://") || normalized.starts_with('/') {
        return true;
    }
    source_path_candidates(&normalized)
        .into_iter()
        .any(|candidate| {
            context
                .resolve_project_path(&candidate)
                .map(|p| p.exists())
                .unwrap_or(false)
        })
}

fn source_path_candidates(source: &str) -> Vec<String> {
    let normalized = source.trim().replace('\\', "/");
    let mut candidates = vec![normalized.clone()];
    if normalized.starts_with("sources/") {
        candidates.push(format!("wiki/{normalized}"));
    } else if !normalized.contains('/') {
        candidates.push(format!("wiki/sources/{normalized}"));
    }
    candidates
}

fn schema_source_issues(
    _context: &ProjectContext,
    page: &WikiPageMeta,
    body: &str,
    frontmatter: &Frontmatter,
) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    let path = page.path.as_str();
    let type_field = frontmatter.get_scalar("type").unwrap_or_default();
    let normalized_type = type_field.trim().to_ascii_lowercase();

    if !is_structural_path(path) && !is_source_or_query_path(path) {
        if normalized_type.is_empty() {
            issues.push(local_issue(
                LintIssueType::SchemaMismatch,
                LintSeverity::Warning,
                path,
                "Derived page is missing frontmatter `type`.",
                None,
                None,
            ));
        } else if recognized_page_type(&normalized_type).is_none() {
            issues.push(local_issue(
                LintIssueType::InvalidPageType,
                LintSeverity::Warning,
                path,
                &format!("Unknown page type `{}`.", type_field.trim()),
                Some(type_field.trim().to_string()),
                None,
            ));
        }
    }

    if let Some(expected) = expected_page_type_for_path(path) {
        if let Some(actual) = recognized_page_type(&normalized_type) {
            if actual != expected {
                issues.push(local_issue(
                    LintIssueType::InvalidPageType,
                    LintSeverity::Warning,
                    path,
                    &format!(
                        "Page type `{}` does not match the path expectation `{:?}`.",
                        type_field.trim(),
                        expected
                    ),
                    Some(type_field.trim().to_string()),
                    None,
                ));
            }
        }
    }

    if is_derived_page(page) {
        let sources: Vec<String> = frontmatter
            .get_list("sources")
            .into_iter()
            .map(|source| source.trim().to_string())
            .filter(|source| !source.is_empty())
            .collect();
        if sources.is_empty() {
            issues.push(local_issue(
                LintIssueType::MissingSource,
                LintSeverity::Error,
                path,
                "Derived page is missing non-empty frontmatter `sources`.",
                None,
                None,
            ));
        }
        if !has_human_readable_sources_section(body) {
            issues.push(local_issue(
                LintIssueType::MissingSourceSection,
                LintSeverity::Warning,
                path,
                "Derived page is missing a human-readable `> Sources:` section.",
                None,
                None,
            ));
        }
    }

    issues
}

fn local_issue(
    issue_type: LintIssueType,
    severity: LintSeverity,
    path: &str,
    message: &str,
    evidence: Option<String>,
    target: Option<String>,
) -> LintIssue {
    LintIssue {
        id: format!("{}:{path}", lint_issue_type_id(issue_type)),
        source: LintIssueSource::Local,
        severity,
        issue_type,
        path: path.to_string(),
        range: None,
        message: message.to_string(),
        evidence,
        target,
        fixability: Fixability::None,
        suggested_action: None,
    }
}

pub(super) fn lint_issue_type_id(issue_type: LintIssueType) -> &'static str {
    match issue_type {
        LintIssueType::DeadLink => "dead_link",
        LintIssueType::OrphanPage => "orphan_page",
        LintIssueType::MissingFrontmatter => "missing_frontmatter",
        LintIssueType::IndexDrift => "index_drift",
        LintIssueType::EmptyPage => "empty_page",
        LintIssueType::DuplicateFilename => "duplicate_filename",
        LintIssueType::PathCase => "path_case",
        LintIssueType::MissingResource => "missing_resource",
        LintIssueType::MissingSourceSection => "missing_source_section",
        LintIssueType::InvalidPageType => "invalid_page_type",
        LintIssueType::DuplicateTopic => "duplicate_topic",
        LintIssueType::WeakCrossReference => "weak_cross_reference",
        LintIssueType::MissingSource => "missing_source",
        LintIssueType::SchemaMismatch => "schema_mismatch",
        LintIssueType::OutdatedContent => "outdated_content",
        LintIssueType::Contradiction => "contradiction",
    }
}

fn is_derived_page(page: &WikiPageMeta) -> bool {
    !is_structural_path(&page.path)
        && !matches!(page.page_type, WikiPageType::Source | WikiPageType::Query)
        && !page.path.starts_with("wiki/sources/")
        && !page.path.starts_with("wiki/queries/")
}

fn is_structural_path(path: &str) -> bool {
    STRUCTURAL_FILES.contains(&path)
}

fn is_source_or_query_path(path: &str) -> bool {
    path.starts_with("wiki/sources/") || path.starts_with("wiki/queries/")
}

fn recognized_page_type(normalized: &str) -> Option<WikiPageType> {
    match normalized {
        "entity" | "entities" => Some(WikiPageType::Entity),
        "concept" | "concepts" => Some(WikiPageType::Concept),
        "source" | "sources" => Some(WikiPageType::Source),
        "synthesis" | "syntheses" => Some(WikiPageType::Synthesis),
        "comparison" | "comparisons" => Some(WikiPageType::Comparison),
        "query" | "queries" => Some(WikiPageType::Query),
        "index" => Some(WikiPageType::Index),
        "overview" => Some(WikiPageType::Overview),
        "log" | "changelog" => Some(WikiPageType::Log),
        _ => None,
    }
}

fn expected_page_type_for_path(path: &str) -> Option<WikiPageType> {
    let wiki_relative = path.strip_prefix("wiki/").unwrap_or(path);
    let first = wiki_relative.split('/').next().unwrap_or("");
    match first {
        "entities" => Some(WikiPageType::Entity),
        "concepts" => Some(WikiPageType::Concept),
        "sources" => Some(WikiPageType::Source),
        "synthesis" => Some(WikiPageType::Synthesis),
        "comparisons" => Some(WikiPageType::Comparison),
        "queries" => Some(WikiPageType::Query),
        _ => match wiki_relative {
            "index.md" => Some(WikiPageType::Index),
            "overview.md" => Some(WikiPageType::Overview),
            "log.md" => Some(WikiPageType::Log),
            _ => None,
        },
    }
}

pub(super) fn has_human_readable_sources_section(body: &str) -> bool {
    body.lines().any(|line| {
        let trimmed = line.trim().to_ascii_lowercase();
        trimmed.starts_with("> sources:")
            || trimmed == "## sources"
            || trimmed == "### sources"
            || trimmed.starts_with("sources:")
    })
}

fn check_structural_page_basics(
    context: &ProjectContext,
    pages: &[WikiPageMeta],
) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    let overview_path = context.resolve_project_path("wiki/overview.md").ok();
    if let Some(path) = overview_path.filter(|path| path.exists()) {
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        if split_frontmatter(&raw).body.trim().is_empty() {
            issues.push(local_issue(
                LintIssueType::SchemaMismatch,
                LintSeverity::Warning,
                "wiki/overview.md",
                "Structural overview page is empty.",
                None,
                None,
            ));
        }
    }

    let index_raw = context
        .resolve_project_path("wiki/index.md")
        .ok()
        .filter(|path| path.exists())
        .and_then(|path| std::fs::read_to_string(path).ok());
    if let Some(index) = index_raw {
        for page in pages.iter().filter(|page| is_derived_page(page)) {
            if let Some(stem) = file_stem(&page.path) {
                if !index.contains(&format!("[[{stem}]]")) && !index.contains(&page.path) {
                    issues.push(LintIssue {
                        id: format!("index_drift:wiki/index.md:{stem}"),
                        source: LintIssueSource::Local,
                        severity: LintSeverity::Error,
                        issue_type: LintIssueType::IndexDrift,
                        path: "wiki/index.md".into(),
                        range: None,
                        message: format!("Index does not reference `{}`.", page.path),
                        evidence: None,
                        target: Some(stem),
                        fixability: Fixability::HighRisk,
                        suggested_action: Some("Regenerate the index.".into()),
                    });
                }
            }
        }
    }

    issues
}

/// Find the 1-based body line of the first `[[target]]` occurrence.
fn find_wikilink_line(body: &str, target: &str) -> Option<usize> {
    let needle_plain = format!("[[{target}]]");
    let needle_alias = format!("[[{target}|");
    for (i, line) in body.lines().enumerate() {
        if line.contains(&needle_plain) || line.contains(&needle_alias) {
            return Some(i + 1);
        }
    }
    None
}

fn severity_rank(severity: LintSeverity) -> u8 {
    match severity {
        LintSeverity::Error => 0,
        LintSeverity::Warning => 1,
        LintSeverity::Info => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{seed_clean_vault, tmp_context, write_file};
    use super::super::LintService;
    use crate::models::lint::{Fixability, LintIssueSource, LintIssueType, LintSeverity};
    use crate::services::SearchService;

    #[test]
    fn clean_vault_has_no_local_issues() {
        let (context, root) = tmp_context("clean");
        seed_clean_vault(&context);
        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        assert_eq!(report.scanned_pages, 5);
        assert!(
            report.issues.is_empty(),
            "expected no issues, got: {:?}",
            report
                .issues
                .iter()
                .map(|i| (i.issue_type, i.path.as_str()))
                .collect::<Vec<_>>()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_dead_link_with_range() {
        let (context, root) = tmp_context("deadlink");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nSee [[ghost]] and [[react]].",
        );
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\nBack.",
        );
        write_file(&context, "wiki/index.md", "# Index\n\n- [[agent]]\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let dead = report
            .issues
            .iter()
            .find(|i| i.issue_type == LintIssueType::DeadLink)
            .expect("dead link expected");
        assert_eq!(dead.target.as_deref(), Some("ghost"));
        assert_eq!(dead.range.as_ref().unwrap().line, 3);
        assert_eq!(dead.fixability, Fixability::HighRisk);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_orphan_page() {
        let (context, root) = tmp_context("orphan");
        write_file(
            &context,
            "wiki/concepts/hub.md",
            "---\ntitle: Hub\ntype: concept\n---\n\n# Hub\n\nLinks [[spoke]].",
        );
        write_file(
            &context,
            "wiki/concepts/spoke.md",
            "---\ntitle: Spoke\ntype: concept\n---\n\n# Spoke\n\nNothing links back.",
        );
        write_file(&context, "wiki/index.md", "# Index\n\n- [[spoke]]\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        // spoke has inbound from hub; hub is an orphan (nothing links to it).
        assert!(report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::OrphanPage && i.path == "wiki/concepts/hub.md"
        }));
        assert!(!report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::OrphanPage && i.path == "wiki/concepts/spoke.md"
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_missing_frontmatter_and_empty_page() {
        let (context, root) = tmp_context("frontmatter");
        write_file(
            &context,
            "wiki/concepts/bare.md",
            "# Bare\n\nSee [[react]].",
        );
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\nBack [[bare]].",
        );
        write_file(&context, "wiki/concepts/empty.md", "");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        assert!(report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::MissingFrontmatter
                && i.path == "wiki/concepts/bare.md"
                && i.fixability == Fixability::Safe
        }));
        // index.md has no frontmatter but is structural → exempt.
        assert!(!report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::MissingFrontmatter && i.path == "wiki/index.md"
        }));
        assert!(report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::EmptyPage && i.path == "wiki/concepts/empty.md"
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_index_drift_ghost_link() {
        let (context, root) = tmp_context("drift");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\n[[react]]",
        );
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\n[[agent]]",
        );
        write_file(
            &context,
            "wiki/index.md",
            "# Index\n\n- [[agent]]\n- [[ghost]]\n",
        );
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        assert!(report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::IndexDrift && i.target.as_deref() == Some("ghost")
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn severity_grading_marks_dead_link_and_index_drift_as_error() {
        // PRD-LINT-001: dead links and index drift are "must-fix" → Error so the
        // summary card's error count is meaningful; frontmatter stays Warning.
        let (context, root) = tmp_context("severity");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nSee [[ghost]].",
        );
        write_file(
            &context,
            "wiki/concepts/bare.md",
            "# Bare\n\nLinks [[agent]].",
        );
        write_file(
            &context,
            "wiki/index.md",
            "# Index\n\n- [[agent]]\n- [[missing]]\n",
        );
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let dead = report
            .issues
            .iter()
            .find(|i| i.issue_type == LintIssueType::DeadLink)
            .expect("dead link expected");
        assert_eq!(
            dead.severity,
            LintSeverity::Error,
            "dead links must be error-grade"
        );
        let drift = report
            .issues
            .iter()
            .find(|i| i.issue_type == LintIssueType::IndexDrift)
            .expect("index drift expected");
        assert_eq!(
            drift.severity,
            LintSeverity::Error,
            "index drift must be error-grade"
        );
        let fm = report
            .issues
            .iter()
            .find(|i| i.issue_type == LintIssueType::MissingFrontmatter)
            .expect("missing frontmatter expected");
        assert_eq!(
            fm.severity,
            LintSeverity::Warning,
            "missing frontmatter stays warning-grade"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_duplicate_filename_and_path_case() {
        let (context, root) = tmp_context("dupes");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent A\ntype: concept\n---\n\n# A\n\n[[react]]",
        );
        write_file(
            &context,
            "wiki/entities/agent.md",
            "---\ntitle: Agent B\ntype: entity\n---\n\n# B\n\n[[concepts/agent]] [[react]]",
        );
        write_file(
            &context,
            "wiki/concepts/React.md",
            "---\ntitle: React Dup\ntype: concept\n---\n\n# ReactDup\n\n[[react]]",
        );
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\n[[agent]]",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        assert!(report
            .issues
            .iter()
            .any(|i| i.issue_type == LintIssueType::DuplicateFilename));
        // PathCase detection requires a case-sensitive filesystem — on
        // Windows (NTFS) the two names collide and overwrite each other.
        if cfg!(not(target_os = "windows")) {
            assert!(report
                .issues
                .iter()
                .any(|i| i.issue_type == LintIssueType::PathCase));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_missing_resource() {
        let (context, root) = tmp_context("resource");
        write_file(
            &context,
            "wiki/sources/paper.md",
            "---\ntitle: Paper\ntype: source\nsources:\n  - raw/sources/missing.md\n---\n\n# Paper\n\n[[react]]",
        );
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\n[[paper]]",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        assert!(report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::MissingResource
                && i.target.as_deref() == Some("raw/sources/missing.md")
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_lint_catches_missing_and_empty_sources() {
        let (context, root) = tmp_context("source-required");
        write_file(
            &context,
            "wiki/concepts/missing.md",
            "---\ntitle: Missing\ntype: concept\n---\n\n# Missing\n\n> Sources: later",
        );
        write_file(
            &context,
            "wiki/concepts/empty.md",
            "---\ntitle: Empty\ntype: concept\nsources: []\n---\n\n# Empty\n\n> Sources: later",
        );
        write_file(
            &context,
            "wiki/index.md",
            "# Index\n\n- [[missing]]\n- [[empty]]",
        );
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();

        assert!(report.issues.iter().any(|i| {
            i.source == LintIssueSource::Local
                && i.issue_type == LintIssueType::MissingSource
                && i.path == "wiki/concepts/missing.md"
        }));
        assert!(report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::MissingSource && i.path == "wiki/concepts/empty.md"
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_lint_catches_bad_type_missing_source_section_and_bad_source_path() {
        let (context, root) = tmp_context("schema-source");
        write_file(
            &context,
            "wiki/concepts/bad-type.md",
            "---\ntitle: Bad Type\ntype: entity\nsources:\n  - wiki/sources/missing.md\n---\n\n# Bad Type\n\nNo source section.",
        );
        write_file(
            &context,
            "wiki/sources/source-a.md",
            "---\ntitle: Source A\ntype: source\n---\n\n# Source A\n\nOriginal.",
        );
        write_file(
            &context,
            "wiki/concepts/shorthand-source.md",
            "---\ntitle: Shorthand Source\ntype: concept\nsources: [source-a.md]\n---\n\n# Shorthand Source\n\nUses a compile-style source basename.\n\n> Sources: [[sources/source-a]]",
        );
        write_file(&context, "wiki/index.md", "# Index\n\n- [[bad-type]]");
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();

        assert!(report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::InvalidPageType && i.path == "wiki/concepts/bad-type.md"
        }));
        assert!(report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::MissingSourceSection
                && i.path == "wiki/concepts/bad-type.md"
        }));
        assert!(report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::MissingResource
                && i.target.as_deref() == Some("wiki/sources/missing.md")
        }));
        assert_eq!(
            report
                .issues
                .iter()
                .filter(|i| {
                    i.id == "missing_resource:wiki/concepts/bad-type.md:wiki/sources/missing.md"
                })
                .count(),
            1,
            "local deterministic source-path checks should emit one stable issue id"
        );
        assert!(!report.issues.iter().any(|i| {
            i.issue_type == LintIssueType::MissingResource
                && i.path == "wiki/concepts/shorthand-source.md"
                && i.target.as_deref() == Some("source-a.md")
        }));
        std::fs::remove_dir_all(root).unwrap();
    }
}
