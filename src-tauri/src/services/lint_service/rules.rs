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
        let initial_tree = search_service.scan_wiki(context, &HashSet::new())?;
        let initial_pages = initial_tree.pages;
        // Establish the optimistic-lock baseline before reading page bodies.
        // A second snapshot is taken after all rules run; if any scanned path
        // changed during the pass, the report is rejected instead of attaching
        // a post-edit hash to findings produced from older content.
        let baseline_hashes = self.capture_scan_snapshot(context, &initial_pages)?;
        // Refresh the tree after taking the baseline so page metadata (links,
        // word counts, frontmatter) is from the same generation as the rules.
        // The final path-set comparison below rejects additions/deletions that
        // occur while the pass is running.
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
                .map_err(|error| {
                    BackendError::new("LINT_PAGE_READ_FAILED", error.message, true, false)
                        .with_details(serde_json::json!({ "path": page.path }))
                })?;
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

            // Dead links. The index has its own rule below so a stale index
            // entry is reported once as `index_drift`, not twice as both a
            // generic dead link and an index issue.
            for target in &page.wikilinks {
                if page.path == "wiki/index.md" {
                    continue;
                }
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
                    scan_hash: None,
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
                let wiki_relative = page.path.strip_prefix("wiki/").unwrap_or(&page.path);
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
                        "Choose a recognized page folder/type, then add frontmatter manually."
                            .into()
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
                if !resource_exists(context, &page.path, source) {
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
                    scan_hash: None,
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

        // Index drift (only when wiki/index.md exists).
        issues.extend(self.check_index_drift(context, &lookup)?);
        issues.extend(check_structural_page_basics(context, &pages, &lookup));

        // Re-enumerate immediately before validating the result so files
        // created/deleted after the rules' tree was read are included in the
        // freshness decision rather than silently omitted.
        let final_tree = search_service.scan_wiki(context, &HashSet::new())?;
        let final_pages = final_tree.pages;
        let final_hashes = self.capture_scan_snapshot(context, &final_pages)?;
        if baseline_hashes != final_hashes {
            let all_paths = baseline_hashes
                .keys()
                .chain(final_hashes.keys())
                .cloned()
                .collect::<HashSet<_>>();
            let changed_paths = all_paths
                .into_iter()
                .filter(|path| baseline_hashes.get(path) != final_hashes.get(path))
                .collect::<Vec<_>>();
            return Err(BackendError::new(
                "LINT_SCAN_CHANGED",
                "Wiki content changed while the local Lint scan was running. Please rescan before applying fixes.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "paths": changed_paths })));
        }

        // Freeze the content version used by the report. The frontend must
        // pass this baseline back when applying a fix; it must never promote a
        // hash read after the user has opened the fix UI into a scan baseline.
        for issue in &mut issues {
            issue.scan_hash = baseline_hashes.get(&issue.path).cloned().flatten();
        }

        // Drop issues the user has dismissed via `.app/lint-ignore.json`. The
        // match key is (path, rule): ignoring a rule on a page suppresses every
        // occurrence of that rule on that page.
        let ignored_keys: HashSet<(String, LintIssueType)> = self
            .load_ignores(context)?
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

    fn capture_scan_snapshot(
        &self,
        context: &ProjectContext,
        pages: &[WikiPageMeta],
    ) -> Result<HashMap<String, Option<String>>, BackendError> {
        let mut paths = pages
            .iter()
            .map(|page| page.path.clone())
            .collect::<HashSet<_>>();
        paths.insert("wiki/index.md".into());
        paths
            .into_iter()
            .map(|path| {
                let hash = self.file_store.file_hash_if_exists(context, &path)?;
                Ok((path, hash))
            })
            .collect()
    }

    fn check_index_drift(
        &self,
        context: &ProjectContext,
        lookup: &std::collections::HashMap<String, String>,
    ) -> Result<Vec<LintIssue>, BackendError> {
        let mut issues = Vec::new();
        if self
            .file_store
            .file_hash_if_exists(context, "wiki/index.md")?
            .is_none()
        {
            issues.push(LintIssue {
                id: "index_drift:wiki/index.md:missing".into(),
                source: LintIssueSource::Local,
                severity: LintSeverity::Error,
                issue_type: LintIssueType::IndexDrift,
                path: "wiki/index.md".into(),
                scan_hash: None,
                range: None,
                message: "The wiki index file is missing.".into(),
                evidence: None,
                target: None,
                fixability: Fixability::None,
                suggested_action: Some(
                    "Create wiki/index.md or run a Wiki compile to regenerate it.".into(),
                ),
            });
            return Ok(issues);
        }
        let raw = self.file_store.read_markdown(context, "wiki/index.md")?;
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
                scan_hash: None,
                range: None,
                message: format!("Index links to `{target}`, which does not exist."),
                evidence: Some(format!("[[{target}]]")),
                target: Some(target.clone()),
                fixability: Fixability::HighRisk,
                suggested_action: Some("Remove the stale link or create the page.".into()),
            });
        }
        Ok(issues)
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

fn resource_exists(context: &ProjectContext, page_path: &str, source: &str) -> bool {
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

fn source_path_candidates(page_path: &str, source: &str) -> Vec<String> {
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

fn is_absolute_resource_ref(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('/')
        || trimmed.starts_with("//")
        || (trimmed.len() >= 3
            && trimmed.as_bytes()[0].is_ascii_alphabetic()
            && trimmed.as_bytes()[1] == b':'
            && (trimmed.as_bytes()[2] == b'/' || trimmed.as_bytes()[2] == b'\\'))
}

fn normalize_resource_path(path: &str) -> Option<String> {
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

fn extract_local_resource_refs(body: &str) -> Vec<String> {
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

fn is_derived_page(page: &WikiPageMeta) -> bool {
    !is_structural_path(&page.path)
        && !matches!(
            page.page_type,
            WikiPageType::Source | WikiPageType::Query | WikiPageType::Other
        )
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

fn check_structural_page_basics(
    context: &ProjectContext,
    pages: &[WikiPageMeta],
    lookup: &HashMap<String, String>,
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
        let index_targets: HashSet<String> = extract_wikilinks(&split_frontmatter(&index).body)
            .into_iter()
            .filter_map(|target| lookup.get(&target.to_ascii_lowercase()).cloned())
            .collect();
        for page in pages.iter().filter(|page| is_derived_page(page)) {
            if !index_targets.contains(&page.path) {
                if let Some(stem) = file_stem(&page.path) {
                    issues.push(LintIssue {
                        id: format!("index_drift:wiki/index.md:{stem}"),
                        source: LintIssueSource::Local,
                        severity: LintSeverity::Error,
                        issue_type: LintIssueType::IndexDrift,
                        path: "wiki/index.md".into(),
                        scan_hash: None,
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
    let wanted = target.trim().replace('\\', "/").to_ascii_lowercase();
    for (line_number, line) in body.lines().enumerate() {
        let mut cursor = 0usize;
        while let Some(relative_start) = line[cursor..].find("[[") {
            let start = cursor + relative_start + 2;
            let Some(relative_end) = line[start..].find("]]") else {
                break;
            };
            let inner = &line[start..start + relative_end];
            let destination = inner.split_once('|').map_or(inner, |(value, _)| value);
            let destination = destination
                .split_once('#')
                .map_or(destination, |(value, _)| value)
                .trim()
                .replace('\\', "/")
                .to_ascii_lowercase();
            if destination == wanted {
                return Some(line_number + 1);
            }
            cursor = start + relative_end + 2;
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
