use std::collections::{HashMap, HashSet};

use crate::errors::BackendError;
use crate::models::lint::{
    Fixability, LintIssue, LintIssueSource, LintIssueType, LintReport, LintSeverity,
};
use crate::models::paths::ProjectContext;
use crate::models::wiki::{WikiPageMeta, WikiPageType};
use crate::services::SearchService;
use crate::utils::markdown_utils::{parse_frontmatter, split_frontmatter, Frontmatter};

use super::LintService;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalLintPhase {
    MarkdownComplete,
}

impl LintService {
    /// Run every local deterministic rule. No LLM or Agent is invoked.
    pub fn run_local_lint(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
    ) -> Result<LintReport, BackendError> {
        self.run_local_lint_with_phase(context, search_service, |_| Ok(()))
    }

    pub fn run_local_lint_with_phase<F>(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        mut on_phase: F,
    ) -> Result<LintReport, BackendError>
    where
        F: FnMut(LocalLintPhase) -> Result<(), BackendError>,
    {
        self.run_health_local_lint_with_phase(context, search_service, &mut on_phase)
    }

    /// Health Check extends the established Wiki lint pass with committed
    /// Source Markdown under `raw/extracted`. Source files remain immutable:
    /// findings are descriptive and never expose an automatic fix.
    pub fn run_health_local_lint_with_phase<F>(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
        mut on_phase: F,
    ) -> Result<LintReport, BackendError>
    where
        F: FnMut(LocalLintPhase) -> Result<(), BackendError>,
    {
        let mut markdown_complete = false;
        let scan = self.run_health_scan(context, search_service, false, |progress| {
            if !markdown_complete && progress.phase != super::HealthScanPhase::Markdown {
                markdown_complete = true;
                on_phase(LocalLintPhase::MarkdownComplete)?;
            }
            Ok(())
        })?;
        if !scan.current {
            return Err(BackendError::new(
                "LINT_SCAN_CHANGED",
                "Markdown changed during the Health Check scan.",
                true,
                true,
            ));
        }
        Ok(scan.report)
    }
}

pub(super) fn markdown_page_issues_with_resources<F>(
    context: &ProjectContext,
    page: &WikiPageMeta,
    raw: &str,
    mut exists: F,
) -> Vec<LintIssue>
where
    F: FnMut(&str) -> bool,
{
    let mut issues = Vec::new();
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

    // Missing frontmatter (structural files are exempt).
    if !frontmatter_present && !is_structural_path(context, &page.path) {
        let wiki_prefix = context
            .layout
            .wiki_write_root
            .as_deref()
            .filter(|root| *root != ".")
            .map(|root| format!("{}/", root.trim_end_matches('/')));
        let wiki_relative = wiki_prefix
            .as_deref()
            .and_then(|prefix| page.path.strip_prefix(prefix))
            .unwrap_or(&page.path);
        let inferred_type = WikiPageType::infer(None, wiki_relative);
        let fixability = if inferred_type == WikiPageType::Other {
            Fixability::None
        } else {
            Fixability::Safe
        };
        issues.push(LintIssue {
            id: format!("missing_frontmatter:{}", page.path),
            source: LintIssueSource::Local,
            severity: LintSeverity::Warning,
            issue_type: LintIssueType::MissingFrontmatter,
            path: page.path.clone(),
            scan_hash: None,
            range: None,
            message: "Page has no YAML frontmatter.".into(),
            evidence: None,
            target: None,
            fixability,
            suggested_action: Some(if fixability == Fixability::Safe {
                "Add a minimal frontmatter block inferred from the page folder.".into()
            } else {
                "Choose a recognized page folder/type, then add frontmatter manually.".into()
            }),
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
            scan_hash: None,
            range: None,
            message: "Page body has no readable words.".into(),
            evidence: None,
            target: None,
            fixability: Fixability::None,
            suggested_action: Some("Add content or remove the page.".into()),
        });
    }

    // Missing resources referenced by `sources:` and by local
    // Markdown links/images. Frontmatter alone misses the common
    // `![scan](../raw/scan.png)` path, while treating remote URLs as
    // local files creates noisy false positives.
    let mut resource_refs = page.sources.clone();
    resource_refs.extend(extract_local_resource_refs(&split.body));
    resource_refs.sort();
    resource_refs.dedup();
    for source in &resource_refs {
        if is_external(source) {
            continue;
        }
        if !exists(source) {
            issues.push(LintIssue {
                id: format!("missing_resource:{}:{source}", page.path),
                source: LintIssueSource::Local,
                severity: LintSeverity::Warning,
                issue_type: LintIssueType::MissingResource,
                path: page.path.clone(),
                scan_hash: None,
                range: None,
                message: format!("Source reference `{source}` does not exist."),
                evidence: None,
                target: Some(source.clone()),
                fixability: Fixability::None,
                suggested_action: Some("Add the source file or correct the path.".into()),
            });
        }
    }

    issues
}

pub(super) fn collision_issues(pages: &[WikiPageMeta]) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    // Duplicate filenames (same stem, different folders).
    let mut by_stem: HashMap<String, Vec<&WikiPageMeta>> = HashMap::new();
    for page in pages {
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
                scan_hash: None,
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
    for page in pages {
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
                scan_hash: None,
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

    issues
}

pub fn health_source_paths(context: &ProjectContext) -> Result<Vec<String>, BackendError> {
    let structural_paths = [
        context.layout.wiki_index_path.as_deref(),
        context.layout.wiki_overview_path.as_deref(),
        context.layout.activity_log_path.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<HashSet<_>>();
    let mut paths = context
        .list_markdown_files_for_roles(&[
            crate::models::layout::ProjectMarkdownRootRole::Source,
            crate::models::layout::ProjectMarkdownRootRole::Mixed,
        ])?
        .into_iter()
        .filter_map(|path| context.to_project_relative(&path).ok())
        .filter(|path| !structural_paths.contains(path.as_str()))
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Case-insensitive lookup from note-name/title/alias -> page path, mirroring
/// `graph_service::build_target_lookup`. Replicated here to avoid coupling
/// lint to graph internals.
pub(super) fn register_page_targets(lookup: &mut HashMap<String, String>, page: &WikiPageMeta) {
    for key in resolution_keys(page) {
        lookup
            .entry(key)
            .and_modify(|existing| {
                // Keep collisions deterministic regardless of scan/sampling order.
                if page.path < *existing {
                    existing.clone_from(&page.path);
                }
            })
            .or_insert_with(|| page.path.clone());
    }
}

fn resolution_keys(page: &WikiPageMeta) -> Vec<String> {
    let mut keys = Vec::new();
    let normalized_path = page.path.replace('\\', "/").to_ascii_lowercase();
    // Wikilinks may use a project-relative path (`concepts/x`) or the
    // canonical `wiki/concepts/x.md` path. Register both forms, with and
    // without the Markdown suffix, in addition to title/alias lookup.
    keys.push(normalized_path.clone());
    if let Some(without_root) = normalized_path.strip_prefix("wiki/") {
        keys.push(without_root.to_string());
    }
    if let Some(without_ext) = normalized_path.strip_suffix(".md") {
        keys.push(without_ext.to_string());
        if let Some(without_root) = without_ext.strip_prefix("wiki/") {
            keys.push(without_root.to_string());
        }
    }
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
pub(super) fn build_inbound_counts(
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

pub(super) fn is_external(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.contains("://") || trimmed.starts_with("mailto:")
}

pub(super) fn resource_exists(context: &ProjectContext, page_path: &str, source: &str) -> bool {
    let normalized = source.replace('\\', "/");
    // Absolute paths and URLs are out of project scope; treat as present to
    // avoid false positives on references we cannot verify.
    if normalized.contains("://") || is_absolute_resource_ref(&normalized) {
        return true;
    }
    source_path_candidates(page_path, &normalized)
        .into_iter()
        .filter_map(|candidate| normalize_resource_path(&candidate))
        .any(|candidate| {
            context
                .resolve_project_path(&candidate)
                .map(|p| p.exists())
                .unwrap_or(false)
        })
}

pub(super) fn source_path_candidates(page_path: &str, source: &str) -> Vec<String> {
    let normalized = source
        .trim()
        .trim_matches('<')
        .trim_matches('>')
        .split(['#', '?'])
        .next()
        .unwrap_or_default()
        .replace('\\', "/");
    let mut candidates = Vec::new();
    if let Some(folder) = page_path.rsplit_once('/').map(|(folder, _)| folder) {
        candidates.push(format!("{folder}/{normalized}"));
    }
    candidates.push(normalized.clone());
    if normalized.starts_with("sources/") {
        candidates.push(format!("wiki/{normalized}"));
    } else if !normalized.contains('/') {
        candidates.push(format!("wiki/sources/{normalized}"));
    }
    candidates
}

pub(super) fn is_absolute_resource_ref(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('/')
        || trimmed.starts_with("//")
        || (trimmed.len() >= 3
            && trimmed.as_bytes()[0].is_ascii_alphabetic()
            && trimmed.as_bytes()[1] == b':'
            && (trimmed.as_bytes()[2] == b'/' || trimmed.as_bytes()[2] == b'\\'))
}

pub(super) fn normalize_resource_path(path: &str) -> Option<String> {
    let normalized_path = path.replace('\\', "/");
    let mut segments: Vec<&str> = Vec::new();
    for segment in normalized_path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            value => segments.push(value),
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

pub(super) fn extract_local_resource_refs(body: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative_start) = body[cursor..].find("](") {
        let start = cursor + relative_start + 2;
        let Some(relative_end) = body[start..].find(')') else {
            break;
        };
        let end = start + relative_end;
        let mut destination = body[start..end].trim();
        if let Some(rest) = destination.strip_prefix('<') {
            if let Some(close) = rest.find('>') {
                destination = &rest[..close];
            }
        } else if let Some(path) = destination.split_whitespace().next() {
            destination = path;
        }
        let cleaned = destination
            .trim()
            .split(['#', '?'])
            .next()
            .unwrap_or_default()
            .trim();
        if !cleaned.is_empty()
            && !cleaned.starts_with('#')
            && !cleaned.starts_with('/')
            && !is_external(cleaned)
            && !cleaned.starts_with("data:")
        {
            refs.push(cleaned.to_string());
        }
        cursor = end + 1;
    }
    refs
}

fn schema_source_issues(
    context: &ProjectContext,
    page: &WikiPageMeta,
    body: &str,
    frontmatter: &Frontmatter,
) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    let path = page.path.as_str();
    let type_field = frontmatter.get_scalar("type").unwrap_or_default();
    let normalized_type = type_field.trim().to_ascii_lowercase();

    if !is_structural_path(context, path) && !is_source_or_query_path(path) {
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

    if is_derived_page(context, page) {
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

pub(super) fn local_issue(
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
        scan_hash: None,
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

pub(super) fn is_derived_page(context: &ProjectContext, page: &WikiPageMeta) -> bool {
    !is_structural_path(context, &page.path)
        && !matches!(
            page.page_type,
            WikiPageType::Source | WikiPageType::Query | WikiPageType::Other
        )
        && !page.path.starts_with("wiki/sources/")
        && !page.path.starts_with("wiki/queries/")
}

fn is_structural_path(context: &ProjectContext, path: &str) -> bool {
    [
        context.layout.wiki_index_path.as_deref(),
        context.layout.wiki_overview_path.as_deref(),
        context.layout.activity_log_path.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|structural| structural == path)
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
        "other" => Some(WikiPageType::Other),
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

fn has_human_readable_sources_section(body: &str) -> bool {
    body.lines().any(|line| {
        let trimmed = line.trim().to_ascii_lowercase();
        trimmed.starts_with("> sources:")
            || trimmed == "## sources"
            || trimmed == "### sources"
            || trimmed.starts_with("sources:")
    })
}

pub(super) fn severity_rank(severity: LintSeverity) -> u8 {
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
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nSee [[ghost]], [[concepts/react]], and [[wiki/concepts/react.md]].",
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
        assert!(report.issues.iter().all(|issue| {
            issue.issue_type != LintIssueType::DeadLink || issue.target.as_deref() == Some("ghost")
        }));
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
        assert!(!report
            .issues
            .iter()
            .any(|i| { i.issue_type == LintIssueType::DeadLink && i.path == "wiki/index.md" }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unknown_page_folder_is_not_marked_safe_for_frontmatter_autofix() {
        let (context, root) = tmp_context("frontmatter-unknown-folder");
        write_file(&context, "wiki/notes/bare.md", "# Bare\n\nContent.");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let issue = report
            .issues
            .iter()
            .find(|issue| {
                issue.issue_type == LintIssueType::MissingFrontmatter
                    && issue.path == "wiki/notes/bare.md"
            })
            .expect("missing frontmatter expected");
        assert_eq!(issue.fixability, Fixability::None);
        assert!(!report.issues.iter().any(|issue| {
            matches!(
                issue.issue_type,
                LintIssueType::MissingSource | LintIssueType::MissingSourceSection
            ) && issue.path == "wiki/notes/bare.md"
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn index_membership_resolves_page_titles_and_aliases() {
        let (context, root) = tmp_context("index-alias");
        write_file(
            &context,
            "wiki/concepts/agent-loop.md",
            "---\ntitle: Agent Loop\ntype: concept\naliases: [Looping Agents]\n---\n\n# Agent Loop\n\nContent.",
        );
        write_file(
            &context,
            "wiki/index.md",
            "# Index\n\n- [[Looping Agents]]\n",
        );
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        assert!(!report.issues.iter().any(|issue| {
            issue.issue_type == LintIssueType::IndexDrift
                && issue.target.as_deref() == Some("agent-loop")
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dead_link_anchor_keeps_a_precise_body_line() {
        let (context, root) = tmp_context("deadlink-anchor");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nSee [[ghost#intro|the missing section]].",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let issue = report
            .issues
            .iter()
            .find(|issue| issue.issue_type == LintIssueType::DeadLink)
            .expect("anchor dead link expected");
        assert_eq!(issue.target.as_deref(), Some("ghost"));
        assert_eq!(issue.range.as_ref().map(|range| range.line), Some(3));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reports_missing_index_as_an_error_instead_of_silently_passing() {
        let (context, root) = tmp_context("missing-index");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nContent.",
        );
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let index_issue = report
            .issues
            .iter()
            .find(|issue| issue.id == "index_drift:wiki/index.md:missing")
            .expect("missing index should be reported");
        assert_eq!(index_issue.severity, LintSeverity::Error);
        assert_eq!(index_issue.fixability, Fixability::None);
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
        // PathCase detection requires a case-sensitive filesystem. Default
        // Windows and macOS volumes collapse these two names into one entry.
        let distinct_case_entries = std::fs::read_dir(root.join("wiki/concepts"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| matches!(entry.file_name().to_str(), Some("React.md" | "react.md")))
            .count()
            == 2;
        if distinct_case_entries {
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
    fn detects_missing_inline_image_and_accepts_relative_resource() {
        let (context, root) = tmp_context("inline-resource");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\n![missing](../../raw/missing.png)\n![present](../../raw/present.png)\n![windows](C:/outside.png)\n![unc](//server/share/file.png)",
        );
        write_file(&context, "raw/present.png", "bytes");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");

        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        assert!(report.issues.iter().any(|issue| {
            issue.issue_type == LintIssueType::MissingResource
                && issue.target.as_deref() == Some("../../raw/missing.png")
        }));
        assert!(!report.issues.iter().any(|issue| {
            issue.issue_type == LintIssueType::MissingResource
                && issue.target.as_deref() == Some("../../raw/present.png")
        }));
        assert!(!report.issues.iter().any(|issue| {
            issue.issue_type == LintIssueType::MissingResource
                && matches!(
                    issue.target.as_deref(),
                    Some("C:/outside.png") | Some("//server/share/file.png")
                )
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
