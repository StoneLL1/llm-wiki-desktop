//! A run-local Health snapshot. This is deliberately not a shared search cache:
//! exact bytes feed metadata, deterministic rules, hashes and bounded AI excerpts.
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::errors::BackendError;
use crate::models::layout::ProjectMarkdownRootRole;
use crate::models::lint::{
    Fixability, LintIssue, LintIssueType, LintRange, LintReport, LintSeverity, WikiLintSkillRef,
};
use crate::models::paths::ProjectContext;
use crate::models::wiki::WikiPageMeta;
use crate::services::SearchService;
use crate::utils::markdown_utils::extract_wikilinks;
use crate::utils::time_utils::now_rfc3339;

use super::deep::{
    escape_untrusted_markup, prompt_prefix, truncate_chars, DEEP_LINT_EXCERPT_CHARS,
    DEEP_LINT_PROMPT_BUDGET_CHARS,
};
use super::rules::*;
use super::{DeepLintSnapshot, LintService};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthScanPhase {
    Markdown,
    Links,
    Verify,
}

#[derive(Debug, Clone)]
pub struct HealthScanProgress {
    pub phase: HealthScanPhase,
    pub completed: usize,
    pub total: usize,
    pub path: Option<String>,
}

#[derive(Debug)]
pub struct HealthLocalScan {
    pub report: LintReport,
    /// Content hashes plus `resource-exists://` existence evidence for local assets.
    pub input_hashes: BTreeMap<String, Option<String>>,
    pub input_fingerprint: String,
    pub known_paths: HashSet<String>,
    pub scanned_at: String,
    pub source_pages: usize,
    pub wiki_pages: usize,
    pub not_applicable_rules: Vec<String>,
    pub current: bool,
    prompt_blocks: Vec<String>,
    prompt_truncated: bool,
    purpose: Option<String>,
    schema: Option<String>,
}

fn paths_for(
    context: &ProjectContext,
    roles: &[ProjectMarkdownRootRole],
) -> Result<HashSet<String>, BackendError> {
    context
        .list_markdown_files_for_roles(roles)?
        .into_iter()
        .map(|path| context.to_project_relative(&path))
        .collect()
}

fn all_paths(context: &ProjectContext) -> Result<HashSet<String>, BackendError> {
    paths_for(
        context,
        &[
            ProjectMarkdownRootRole::Wiki,
            ProjectMarkdownRootRole::Source,
            ProjectMarkdownRootRole::Mixed,
        ],
    )
}

fn purpose_path(context: &ProjectContext) -> Option<&str> {
    context
        .layout
        .purpose_context
        .as_ref()
        .and_then(|document| document.read_path.as_deref())
}

fn schema_path(context: &ProjectContext) -> Option<&str> {
    context
        .layout
        .schema_context
        .as_ref()
        .and_then(|document| document.read_path.as_deref())
}

fn optional_paths(context: &ProjectContext) -> Vec<String> {
    [
        purpose_path(context),
        schema_path(context),
        context.layout.lint_ignore_path.as_deref(),
        context.layout.wiki_index_path.as_deref(),
        context.layout.wiki_overview_path.as_deref(),
        context.layout.activity_log_path.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::to_string)
    .collect()
}

fn matches_role(context: &ProjectContext, path: &str, role: ProjectMarkdownRootRole) -> bool {
    context.layout.markdown_roots.iter().any(|root| {
        let prefix = format!("{}/", root.path.trim_end_matches('/'));
        let relative = if root.path == "." {
            Some(path)
        } else {
            path.strip_prefix(&prefix)
        };
        root.role == role
            && relative.is_some_and(|_| {
                !root
                    .exclude
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .any(|excluded| path == excluded || path.starts_with(&format!("{excluded}/")))
            })
    })
}

fn progress<F>(
    observer: &mut F,
    phase: HealthScanPhase,
    completed: usize,
    total: usize,
    path: Option<&str>,
) -> Result<(), BackendError>
where
    F: FnMut(HealthScanProgress) -> Result<(), BackendError>,
{
    observer(HealthScanProgress {
        phase,
        completed,
        total,
        path: path.map(str::to_string),
    })
}

impl LintService {
    pub fn run_health_local_scan<F>(
        &self,
        context: &ProjectContext,
        search: &SearchService,
        observer: F,
    ) -> Result<HealthLocalScan, BackendError>
    where
        F: FnMut(HealthScanProgress) -> Result<(), BackendError>,
    {
        self.run_health_scan(context, search, true, observer)
    }

    /// One read/parse pass serves checks, repair verification and AI input.
    /// Local-only callers do not allocate or format AI excerpts.
    pub fn run_health_scan<F>(
        &self,
        context: &ProjectContext,
        search: &SearchService,
        include_deep: bool,
        mut observer: F,
    ) -> Result<HealthLocalScan, BackendError>
    where
        F: FnMut(HealthScanProgress) -> Result<(), BackendError>,
    {
        // Check cancellation even before directory discovery. No route, Git,
        // credential or write-permit dependency belongs in this read-only pass.
        progress(&mut observer, HealthScanPhase::Markdown, 0, 0, None)?;
        let scanned_at = now_rfc3339();
        let known_paths = all_paths(context)?;
        let source_only = known_paths
            .iter()
            .filter(|path| matches_role(context, path, ProjectMarkdownRootRole::Source))
            .cloned()
            .collect::<HashSet<_>>();
        let structural_paths = [
            context.layout.wiki_index_path.as_deref(),
            context.layout.wiki_overview_path.as_deref(),
            context.layout.activity_log_path.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<HashSet<_>>();
        let source_paths = known_paths
            .iter()
            .filter(|path| {
                !structural_paths.contains(path.as_str())
                    && (source_only.contains(*path)
                        || matches_role(context, path, ProjectMarkdownRootRole::Mixed))
            })
            .cloned()
            .collect::<HashSet<_>>();
        let mut paths = known_paths.iter().cloned().collect::<Vec<_>>();
        paths.sort();
        if include_deep {
            // Give both kinds of evidence a place in a bounded AI excerpt.
            // Lexical order alone lets raw/extracted consume the whole budget.
            let (sources, wiki): (Vec<_>, Vec<_>) = paths
                .into_iter()
                .partition(|path| source_paths.contains(path));
            let mut sources = sources.into_iter();
            let mut wiki = wiki.into_iter();
            paths = std::iter::from_fn(|| {
                let pair = [wiki.next(), sources.next()];
                pair.iter().any(Option::is_some).then_some(pair)
            })
            .flatten()
            .flatten()
            .collect();
        }
        let mut input_hashes = BTreeMap::new();
        let mut purpose = None;
        let mut schema = None;
        for path in optional_paths(context) {
            progress(
                &mut observer,
                HealthScanPhase::Markdown,
                0,
                paths.len(),
                Some(&path),
            )?;
            // Guidance is read once and hashed from those very bytes. Retain
            // only prompt-budget text, even for unusually large guidance files.
            if known_paths.contains(&path) {
                continue;
            }
            if Some(path.as_str()) == purpose_path(context)
                || Some(path.as_str()) == schema_path(context)
            {
                if self.file_store.exists(context, &path) {
                    let raw = self.file_store.read_markdown(context, &path)?;
                    input_hashes.insert(
                        path.clone(),
                        Some(self.file_store.content_hash(raw.as_bytes())),
                    );
                    let bounded = raw.chars().take(DEEP_LINT_PROMPT_BUDGET_CHARS).collect();
                    if Some(path.as_str()) == purpose_path(context) {
                        purpose = Some(bounded);
                    } else {
                        schema = Some(bounded);
                    }
                } else {
                    input_hashes.insert(path, None);
                }
            } else if !known_paths.contains(&path) {
                input_hashes.insert(
                    path.clone(),
                    self.file_store.file_hash_if_exists(context, &path)?,
                );
            }
        }
        let mut pages = Vec::new();
        let mut lookup = HashMap::new();
        let mut issues = Vec::new();
        let mut link_lines = HashMap::new();
        let mut prompt_blocks = Vec::new();
        let mut prompt_chars = 0;
        let mut prompt_truncated = false;
        for (index, path) in paths.iter().enumerate() {
            progress(
                &mut observer,
                HealthScanPhase::Markdown,
                index,
                paths.len(),
                Some(path),
            )?;
            let content = search.read_page(context, path, &HashSet::new())?;
            // Source-only roots are valid link destinations even though their
            // faithful documents do not participate in derived-Wiki rules.
            register_page_targets(&mut lookup, &content.meta);
            input_hashes.insert(path.clone(), Some(content.meta.hash.clone()));
            if Some(path.as_str()) == purpose_path(context) {
                purpose = Some(
                    content
                        .raw_markdown
                        .chars()
                        .take(DEEP_LINT_PROMPT_BUDGET_CHARS)
                        .collect(),
                );
            }
            if Some(path.as_str()) == schema_path(context) {
                schema = Some(
                    content
                        .raw_markdown
                        .chars()
                        .take(DEEP_LINT_PROMPT_BUDGET_CHARS)
                        .collect(),
                );
            }
            // Cache presence from the first reference for this run. Both
            // findings and freshness use that same evidence when multiple
            // pages reference an asset edited while the scan is in progress.
            let mut resources_present = HashMap::new();
            let resources = content
                .meta
                .sources
                .iter()
                .cloned()
                .chain(extract_local_resource_refs(&content.body_markdown));
            for resource in resources {
                if is_external(&resource) || is_absolute_resource_ref(&resource.replace('\\', "/"))
                {
                    resources_present.insert(resource, true);
                    continue;
                }
                let mut present = false;
                for candidate in source_path_candidates(path, &resource)
                    .into_iter()
                    .filter_map(|candidate| normalize_resource_path(&candidate))
                {
                    let evidence = input_hashes
                        .entry(format!("resource-exists://{candidate}"))
                        .or_insert_with(|| {
                            context
                                .resolve_project_path(&candidate)
                                .map(|path| path.exists())
                                .unwrap_or(false)
                                .then(|| "present".into())
                        });
                    present |= evidence.is_some();
                }
                resources_present.insert(resource, present);
            }
            if source_only.contains(path)
                && !matches_role(context, path, ProjectMarkdownRootRole::Wiki)
                && !matches_role(context, path, ProjectMarkdownRootRole::Mixed)
            {
                if content.frontmatter_yaml.is_none() {
                    issues.push(local_issue(
                        LintIssueType::MissingFrontmatter,
                        LintSeverity::Warning,
                        path,
                        "Committed Source Markdown has no YAML frontmatter.",
                        None,
                        None,
                    ));
                }
                if content.body_markdown.trim().is_empty() {
                    issues.push(local_issue(
                        LintIssueType::EmptyPage,
                        LintSeverity::Warning,
                        path,
                        "Committed Source Markdown has no readable body.",
                        None,
                        None,
                    ));
                }
                let mut resources = resources_present.keys().cloned().collect::<Vec<_>>();
                resources.sort();
                for resource in resources {
                    if !is_external(&resource)
                        && !resources_present.get(&resource).copied().unwrap_or(true)
                    {
                        let mut issue = local_issue(
                            LintIssueType::MissingResource,
                            LintSeverity::Warning,
                            path,
                            &format!("Source reference `{resource}` does not exist."),
                            None,
                            Some(resource.clone()),
                        );
                        issue.id = format!("missing_resource:{path}:{resource}");
                        issues.push(issue);
                    }
                }
            } else {
                issues.extend(markdown_page_issues_with_resources(
                    context,
                    &content.meta,
                    &content.raw_markdown,
                    |source| resources_present.get(source).copied().unwrap_or(true),
                ));
                let lines = wikilink_lines(&content.body_markdown, || {
                    progress(
                        &mut observer,
                        HealthScanPhase::Markdown,
                        index,
                        paths.len(),
                        Some(path),
                    )
                })?;
                link_lines.insert(path.clone(), lines);
                pages.push(content.meta.clone());
            }
            if include_deep
                && !prompt_truncated
                && Some(path.as_str()) != context.layout.activity_log_path.as_deref()
            {
                let block = escape_untrusted_markup(&format!(
                    "\n### {} ({:?})\npath: {}\ntags: {}\n{}\n",
                    content.meta.title,
                    content.meta.page_type,
                    path,
                    content.meta.tags.join(", "),
                    truncate_chars(&content.body_markdown, DEEP_LINT_EXCERPT_CHARS)
                ));
                let count = block.chars().count();
                if prompt_chars + count <= DEEP_LINT_PROMPT_BUDGET_CHARS && !prompt_truncated {
                    prompt_chars += count;
                    prompt_blocks.push(block);
                } else {
                    prompt_truncated = true;
                }
            }
            progress(
                &mut observer,
                HealthScanPhase::Markdown,
                index + 1,
                paths.len(),
                Some(path),
            )?;
        }
        pages.sort_by(|left, right| left.path.cmp(&right.path));
        let inbound = build_inbound_counts(&pages, &lookup);
        issues.extend(collision_issues(&pages));
        for (index, page) in pages.iter().enumerate() {
            progress(
                &mut observer,
                HealthScanPhase::Links,
                index,
                pages.len(),
                Some(&page.path),
            )?;
            for target in &page.wikilinks {
                if Some(page.path.as_str()) == context.layout.wiki_index_path.as_deref()
                    || is_external(target)
                    || lookup.contains_key(&target.trim().to_ascii_lowercase())
                {
                    continue;
                }
                let mut issue = local_issue(
                    LintIssueType::DeadLink,
                    LintSeverity::Error,
                    &page.path,
                    &format!("Unresolved wikilink `[[{target}]]`."),
                    Some(format!("[[{target}]]")),
                    Some(target.clone()),
                );
                issue.id = format!("dead_link:{}:{target}", page.path);
                issue.range = link_lines
                    .get(&page.path)
                    .and_then(|lines| lines.get(&target.trim().to_ascii_lowercase()))
                    .copied()
                    .map(|line| LintRange { line, column: None });
                issue.fixability = Fixability::HighRisk;
                issue.suggested_action =
                    Some("Remove the link or fix the target to match an existing page.".into());
                issues.push(issue);
            }
            if !structural_paths.contains(page.path.as_str())
                && inbound.get(&page.path).copied().unwrap_or(0) == 0
            {
                let mut issue = local_issue(
                    LintIssueType::OrphanPage,
                    LintSeverity::Info,
                    &page.path,
                    "No other page links to this page.",
                    None,
                    None,
                );
                issue.suggested_action = Some("Link it from a related page or the index.".into());
                issues.push(issue);
            }
            progress(
                &mut observer,
                HealthScanPhase::Links,
                index + 1,
                pages.len(),
                Some(&page.path),
            )?;
        }
        let wiki_pages = pages
            .iter()
            .filter(|page| !source_only.contains(&page.path))
            .count();
        let mut not_applicable_rules = Vec::new();
        if wiki_pages == 0 || context.layout.wiki_index_path.is_none() {
            not_applicable_rules.push("index_drift".into());
        } else {
            issues.extend(snapshot_structural_issues(context, &pages, &lookup));
        }
        for issue in &mut issues {
            if source_paths.contains(&issue.path) {
                issue.fixability = Fixability::None;
            }
            issue.scan_hash = input_hashes.get(&issue.path).cloned().flatten();
        }
        if context.layout.lint_ignore_path.is_some() {
            self.filter_ignored_issues(context, &mut issues)?;
        }
        issues.sort_by(|a, b| {
            severity_rank(a.severity)
                .cmp(&severity_rank(b.severity))
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.id.cmp(&b.id))
        });
        let current = self.verify_health_inputs(context, &input_hashes, &mut observer)?;
        let input_fingerprint = self.file_store.content_hash(
            serde_json::to_vec(&input_hashes)
                .map_err(|err| {
                    BackendError::new("LINT_SNAPSHOT_INVALID", err.to_string(), true, false)
                })?
                .as_slice(),
        );
        Ok(HealthLocalScan {
            report: LintReport {
                issues,
                generated_at: now_rfc3339(),
                scanned_pages: paths.len(),
            },
            input_hashes,
            input_fingerprint,
            known_paths,
            scanned_at,
            source_pages: source_paths.len(),
            wiki_pages,
            not_applicable_rules,
            current,
            prompt_blocks,
            prompt_truncated,
            purpose,
            schema,
        })
    }

    /// Reopening reports needs only their persisted hash evidence. Hashes and
    /// path membership are validated independently of task/repair authorization.
    pub fn verify_health_inputs<F>(
        &self,
        context: &ProjectContext,
        inputs: &BTreeMap<String, Option<String>>,
        mut observer: F,
    ) -> Result<bool, BackendError>
    where
        F: FnMut(HealthScanProgress) -> Result<(), BackendError>,
    {
        progress(
            &mut observer,
            HealthScanPhase::Verify,
            0,
            inputs.len(),
            None,
        )?;
        let mut current_paths = all_paths(context)?;
        current_paths.extend(optional_paths(context));
        current_paths.extend(
            inputs
                .keys()
                .filter(|path| path.starts_with("resource-exists://"))
                .cloned(),
        );
        if current_paths != inputs.keys().cloned().collect() {
            return Ok(false);
        }
        for (index, (path, expected)) in inputs.iter().enumerate() {
            progress(
                &mut observer,
                HealthScanPhase::Verify,
                index,
                inputs.len(),
                Some(path),
            )?;
            let actual = if let Some(candidate) = path.strip_prefix("resource-exists://") {
                context
                    .resolve_project_path(candidate)
                    .map(|path| path.exists())
                    .unwrap_or(false)
                    .then(|| "present".into())
            } else {
                self.file_store.file_hash_if_exists(context, path)?
            };
            if &actual != expected {
                return Ok(false);
            }
            progress(
                &mut observer,
                HealthScanPhase::Verify,
                index + 1,
                inputs.len(),
                Some(path),
            )?;
        }
        Ok(true)
    }

    /// Pure prompt assembly from the local run's exact generation. Full bodies
    /// have already been dropped; retained excerpts have one total char budget.
    pub fn prepare_health_deep_snapshot_from_scan(
        &self,
        scan: &HealthLocalScan,
        language: &str,
    ) -> DeepLintSnapshot {
        let mut prompt = prompt_prefix(
            language,
            &scan.report,
            scan.purpose.as_deref(),
            scan.schema.as_deref(),
        );
        let mut chars = prompt.chars().count();
        let mut covered = 0;
        let mut truncated = scan.prompt_truncated;
        for block in &scan.prompt_blocks {
            if chars + block.chars().count() > DEEP_LINT_PROMPT_BUDGET_CHARS - 128 {
                truncated = true;
                break;
            }
            prompt.push_str(block);
            chars += block.chars().count();
            covered += 1;
        }
        if truncated {
            prompt.push_str("\n[coverage truncated: prompt budget reached; report must not claim full coverage]\n");
        }
        prompt.push_str("</untrusted-wiki-data>\n");
        let skill = WikiLintSkillRef::builtin();
        DeepLintSnapshot {
            input_hashes: scan.input_hashes.clone(),
            prompt,
            skill,
            known_paths: scan.known_paths.clone(),
            deep_covered_pages: covered,
            deep_truncated: truncated,
            deterministic_issue_ids: scan
                .report
                .issues
                .iter()
                .map(|issue| issue.id.clone())
                .collect(),
        }
    }
}

/// Index first occurrences once, rather than rescanning a page for every link.
/// Large pages yield cancellation checks even before their page count advances.
fn wikilink_lines<F>(body: &str, mut checkpoint: F) -> Result<HashMap<String, usize>, BackendError>
where
    F: FnMut() -> Result<(), BackendError>,
{
    let mut lines = HashMap::new();
    for (index, line) in body.lines().enumerate() {
        if index % 256 == 0 {
            checkpoint()?;
        }
        for target in extract_wikilinks(line) {
            lines
                .entry(target.to_ascii_lowercase())
                .or_insert(index + 1);
        }
    }
    Ok(lines)
}

fn snapshot_structural_issues(
    context: &ProjectContext,
    pages: &[WikiPageMeta],
    lookup: &HashMap<String, String>,
) -> Vec<LintIssue> {
    let mut issues = Vec::new();
    if let Some(page) = pages
        .iter()
        .find(|page| Some(page.path.as_str()) == context.layout.wiki_overview_path.as_deref())
    {
        if page.word_count == 0 {
            issues.push(local_issue(
                LintIssueType::SchemaMismatch,
                LintSeverity::Warning,
                &page.path,
                "Structural overview page is empty.",
                None,
                None,
            ));
        }
    }
    let Some(index_path) = &context.layout.wiki_index_path else {
        return issues;
    };
    let Some(index) = pages.iter().find(|page| &page.path == index_path) else {
        let mut issue = local_issue(
            LintIssueType::IndexDrift,
            LintSeverity::Error,
            index_path,
            "The wiki index file is missing.",
            None,
            None,
        );
        issue.id = format!("index_drift:{index_path}:missing");
        issue.suggested_action = Some(format!(
            "Create {index_path} or use the project workflow that maintains its index."
        ));
        issues.push(issue);
        return issues;
    };
    for target in &index.wikilinks {
        if is_external(target) || lookup.contains_key(&target.trim().to_ascii_lowercase()) {
            continue;
        }
        let mut issue = local_issue(
            LintIssueType::IndexDrift,
            LintSeverity::Error,
            index_path,
            &format!("Index links to `{target}`, which does not exist."),
            Some(format!("[[{target}]]")),
            Some(target.clone()),
        );
        issue.id = format!("index_drift:{index_path}:{target}");
        issue.fixability = Fixability::HighRisk;
        issue.suggested_action = Some("Remove the stale link or create the page.".into());
        issues.push(issue);
    }
    let targets = index
        .wikilinks
        .iter()
        .filter_map(|target| lookup.get(&target.to_ascii_lowercase()))
        .collect::<HashSet<_>>();
    for page in pages.iter().filter(|page| is_derived_page(context, page)) {
        if !targets.contains(&page.path) {
            let mut issue = local_issue(
                LintIssueType::IndexDrift,
                LintSeverity::Error,
                index_path,
                &format!("Index does not reference `{}`.", page.path),
                None,
                Some(page.path.clone()),
            );
            issue.id = format!("index_drift:{index_path}:{}", page.path);
            issue.fixability = Fixability::HighRisk;
            issue.suggested_action = Some("Regenerate the index.".into());
            issues.push(issue);
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{seed_clean_vault, tmp_context, write_file};
    use super::*;

    #[test]
    fn source_only_roots_are_valid_wikilink_destinations_without_wiki_rules() {
        let (context, root) = tmp_context("health-source-link-targets");
        write_file(
            &context,
            "raw/extracted/来源.md",
            "---\ntitle: 来源\naliases: [原文]\n---\n\nSource text",
        );
        write_file(
            &context,
            "wiki/concepts/概念.md",
            "# 概念\n\n[[raw/extracted/来源]] [[原文]] [[missing]]",
        );
        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let dead = report
            .issues
            .iter()
            .filter(|issue| issue.issue_type == LintIssueType::DeadLink)
            .collect::<Vec<_>>();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].target.as_deref(), Some("missing"));
        assert!(!report
            .issues
            .iter()
            .any(|issue| issue.path == "raw/extracted/来源.md"));
        let complete = LintService::default()
            .run_health_local_scan(&context, &SearchService::default(), |_| Ok(()))
            .unwrap();
        assert_eq!(
            serde_json::to_value(&report.issues).unwrap(),
            serde_json::to_value(&complete.report.issues).unwrap(),
            "AI sampling order must not change local findings"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wikilink_locations_keep_first_body_line_and_cancel_inside_large_pages() {
        let body = "# Page\n[[中文#小节|别名]] [[OTHER]]\n[[中文]]\n[[other]]";
        let lines = wikilink_lines(body, || Ok(())).unwrap();
        assert_eq!(lines["中文"], 2);
        assert_eq!(lines["other"], 2);
        let large = (0..10_000)
            .map(|index| format!("[[target-{index}]]\n"))
            .collect::<String>();
        let mut checkpoints = 0;
        let error = wikilink_lines(&large, || {
            checkpoints += 1;
            if checkpoints == 2 {
                Err(BackendError::new(
                    "TASK_CANCELLED",
                    "cancelled",
                    false,
                    false,
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.code, "TASK_CANCELLED");
        assert_eq!(checkpoints, 2);
    }

    #[test]
    fn missing_index_entries_with_duplicate_stems_keep_unique_findings() {
        let (context, root) = tmp_context("health-index-unique-paths");
        write_file(&context, "wiki/index.md", "# Index\n");
        for path in ["wiki/concepts/同名.md", "wiki/topics/同名.md"] {
            write_file(&context, path, "---\ntype: concept\n---\n\n# Page\n\nText");
        }
        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let index = report
            .issues
            .iter()
            .filter(|issue| issue.issue_type == LintIssueType::IndexDrift)
            .collect::<Vec<_>>();
        assert_eq!(index.len(), 2);
        assert_ne!(index[0].id, index[1].id);
        assert_eq!(
            index
                .iter()
                .map(|issue| issue.target.as_deref().unwrap())
                .collect::<HashSet<_>>(),
            HashSet::from(["wiki/concepts/同名.md", "wiki/topics/同名.md"])
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bounded_deep_input_represents_both_source_and_wiki_roots() {
        let (context, root) = tmp_context("health-balanced-deep");
        for index in 0..150 {
            for (folder, title) in [("raw/extracted", "Source"), ("wiki/concepts", "Concept")] {
                write_file(
                    &context,
                    &format!("{folder}/{index:03}.md"),
                    &format!(
                        "---\ntitle: {title}-{index}\n---\n\n{}",
                        "readable text ".repeat(100)
                    ),
                );
            }
        }
        let lint = LintService::default();
        let scan = lint
            .run_health_local_scan(&context, &SearchService::default(), |_| Ok(()))
            .unwrap();
        let snapshot = lint.prepare_health_deep_snapshot_from_scan(&scan, "en");
        let page_data = snapshot
            .prompt
            .split("--- Pages (untrusted-wiki-data) ---")
            .nth(1)
            .unwrap();
        assert!(page_data.contains("path: wiki/concepts/000.md"));
        assert!(page_data.contains("path: raw/extracted/000.md"));
        assert!(snapshot.deep_truncated);
        assert!(snapshot.prompt.chars().count() <= DEEP_LINT_PROMPT_BUDGET_CHARS);
        assert_eq!(
            snapshot.deep_covered_pages,
            page_data.matches("\npath: ").count()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn health_scan_without_git_or_app_state_reuses_exact_inputs_and_matches_local_rules() {
        let (context, root) = tmp_context("health-local-中文");
        seed_clean_vault(&context);
        write_file(&context, "wiki/concepts/概念.md", "# 概念\n\n[[missing]]");
        let service = LintService::default();
        let search = SearchService::default();
        let legacy = service.run_local_lint(&context, &search).unwrap();
        let mut events = Vec::new();
        let scan = service
            .run_health_local_scan(&context, &search, |event| {
                events.push(event);
                Ok(())
            })
            .unwrap();
        assert!(scan.current);
        assert_eq!(scan.report.scanned_pages, 6);
        let ids = |report: &LintReport| {
            report
                .issues
                .iter()
                .map(|issue| issue.id.clone())
                .collect::<HashSet<_>>()
        };
        assert_eq!(ids(&legacy), ids(&scan.report));
        assert!(events
            .iter()
            .any(|event| event.phase == HealthScanPhase::Markdown
                && event.completed == 6
                && event.total == 6));
        assert!(events
            .iter()
            .any(|event| event.phase == HealthScanPhase::Links && event.completed == 6));
        let snapshot = service.prepare_health_deep_snapshot_from_scan(&scan, "en");
        service
            .verify_deep_lint_snapshot(&context, &search, &snapshot)
            .unwrap();
        assert!(snapshot.prompt.contains("wiki/concepts/概念.md"));
        assert!(!root.join(".git").exists());
        assert!(!root.join(".app").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn health_scan_cancels_at_each_real_batch_boundary() {
        for phase in [
            HealthScanPhase::Markdown,
            HealthScanPhase::Links,
            HealthScanPhase::Verify,
        ] {
            let (context, root) = tmp_context("health-cancel");
            seed_clean_vault(&context);
            let mut completed = 0;
            let result = LintService::default().run_health_local_scan(
                &context,
                &SearchService::default(),
                |event| {
                    if event.phase == phase {
                        completed = event.completed;
                        if event.completed == 2 {
                            return Err(BackendError::new(
                                "TASK_CANCELLED",
                                "cancelled",
                                false,
                                false,
                            ));
                        }
                    }
                    Ok(())
                },
            );
            assert_eq!(result.unwrap_err().code, "TASK_CANCELLED");
            assert_eq!(completed, 2);
            assert!(!root.join(".app").exists());
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn edits_during_scan_preserve_findings_and_original_prompt_with_stale_evidence() {
        let (context, root) = tmp_context("health-mid-scan-edit");
        write_file(&context, "wiki/concepts/中文.md", "# Before\n\n[[missing]]");
        let mut edited = false;
        let service = LintService::default();
        let scan = service
            .run_health_local_scan(&context, &SearchService::default(), |event| {
                if event.phase == HealthScanPhase::Markdown && event.completed == 1 && !edited {
                    write_file(
                        &context,
                        "wiki/concepts/中文.md",
                        "# After\n\nFixed content",
                    );
                    edited = true;
                }
                Ok(())
            })
            .unwrap();
        assert!(!scan.current);
        assert!(scan
            .report
            .issues
            .iter()
            .any(|issue| issue.issue_type == LintIssueType::DeadLink));
        let snapshot = service.prepare_health_deep_snapshot_from_scan(&scan, "en");
        assert!(snapshot.prompt.contains("# Before"));
        assert!(!snapshot.prompt.contains("# After"));
        assert_eq!(
            service
                .verify_deep_lint_snapshot(&context, &SearchService::default(), &snapshot)
                .unwrap_err()
                .code,
            "LINT_SCAN_CHANGED"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reopened_evidence_detects_add_delete_guidance_and_ignore_changes() {
        for path in [
            "wiki/concepts/新页面.md",
            "raw/extracted/新资料.md",
            "purpose.md",
            "schema.md",
            ".app/lint-ignore.json",
        ] {
            let (context, root) = tmp_context("health-reopen-evidence");
            seed_clean_vault(&context);
            let scan = LintService::default()
                .run_health_local_scan(&context, &SearchService::default(), |_| Ok(()))
                .unwrap();
            let reopened = LintService::default();
            assert!(reopened
                .verify_health_inputs(&context, &scan.input_hashes, |_| Ok(()))
                .unwrap());
            write_file(&context, path, "# External edit");
            assert!(
                !reopened
                    .verify_health_inputs(&context, &scan.input_hashes, |_| Ok(()))
                    .unwrap(),
                "{path}"
            );
            std::fs::remove_file(context.resolve_project_path(path).unwrap()).unwrap();
            assert!(reopened
                .verify_health_inputs(&context, &scan.input_hashes, |_| Ok(()))
                .unwrap());
            std::fs::remove_file(
                context
                    .resolve_project_path("wiki/concepts/agent.md")
                    .unwrap(),
            )
            .unwrap();
            assert!(!reopened
                .verify_health_inputs(&context, &scan.input_hashes, |_| Ok(()))
                .unwrap());
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn source_only_compatible_health_snapshot_verifies_without_native_wiki_paths() {
        let (_, root) = tmp_context("health-source-only");
        // Use the resolver's source-only compatible contract with CJK root.
        std::fs::create_dir_all(root.join("资料")).unwrap();
        std::fs::create_dir_all(root.join(".app/compat")).unwrap();
        std::fs::write(
            root.join(".app/compat/layout.json"),
            r#"{"schemaVersion":1,"sourceWriteRoot":"资料"}"#,
        )
        .unwrap();
        std::fs::write(root.join("资料/来源.md"), "# 来源\n\nSource text").unwrap();
        std::fs::write(root.join(".app/compat/purpose.md"), "# Purpose").unwrap();
        std::fs::write(root.join(".app/compat/schema.md"), "# Schema").unwrap();
        let context = ProjectContext::new("source-only", root.clone())
            .with_resolved_layout()
            .unwrap();
        let service = LintService::default();
        let scan = service
            .run_health_local_scan(&context, &SearchService::default(), |_| Ok(()))
            .unwrap();
        assert_eq!(scan.report.scanned_pages, 1);
        assert_eq!(scan.source_pages, 1);
        assert_eq!(scan.wiki_pages, 0);
        assert!(scan.not_applicable_rules.contains(&"index_drift".into()));
        let deep = service.prepare_health_deep_snapshot_from_scan(&scan, "zh-CN");
        service
            .verify_deep_lint_snapshot(&context, &SearchService::default(), &deep)
            .unwrap();
        std::fs::write(root.join("资料/来源.md"), "# changed").unwrap();
        assert_eq!(
            service
                .verify_deep_lint_snapshot(&context, &SearchService::default(), &deep)
                .unwrap_err()
                .code,
            "LINT_SCAN_CHANGED"
        );
        assert!(!root.join("wiki").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deep_excerpts_use_a_total_memory_budget_and_count_each_page_once() {
        let (context, root) = tmp_context("health-bounded");
        for index in 0..150 {
            write_file(
                &context,
                &format!("raw/extracted/资料-{index:03}.md"),
                &format!("---\ntitle: Source\n---\n{}", "材料 ".repeat(2000)),
            );
        }
        let service = LintService::default();
        let scan = service
            .run_health_local_scan(&context, &SearchService::default(), |_| Ok(()))
            .unwrap();
        assert!(
            scan.prompt_blocks
                .iter()
                .map(|block| block.chars().count())
                .sum::<usize>()
                <= DEEP_LINT_PROMPT_BUDGET_CHARS
        );
        assert_eq!(scan.report.scanned_pages, 150);
        let snapshot = service.prepare_health_deep_snapshot_from_scan(&scan, "en");
        assert!(snapshot.deep_truncated);
        assert!(snapshot.deep_covered_pages < 150);
        assert!(snapshot.deep_covered_pages > 0);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn resource_creation_changes_report_freshness_without_reading_asset_bytes() {
        let (context, root) = tmp_context("health-asset-evidence");
        write_file(
            &context,
            "wiki/concepts/页面.md",
            "# Page\n\n![image](../../raw/图片.png)",
        );
        let service = LintService::default();
        let scan = service
            .run_health_local_scan(&context, &SearchService::default(), |_| Ok(()))
            .unwrap();
        assert!(scan
            .report
            .issues
            .iter()
            .any(|issue| issue.issue_type == LintIssueType::MissingResource));
        assert!(scan
            .input_hashes
            .contains_key("resource-exists://raw/图片.png"));
        std::fs::create_dir_all(root.join("raw")).unwrap();
        std::fs::write(root.join("raw/图片.png"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(!service
            .verify_health_inputs(&context, &scan.input_hashes, |_| Ok(()))
            .unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn same_size_mtime_external_edit_bypasses_search_metadata_cache() {
        let (context, root) = tmp_context("health-exact-read");
        write_file(&context, "wiki/concepts/页面.md", "# Before\n\n[[lost-a]]");
        let path = context
            .resolve_project_path("wiki/concepts/页面.md")
            .unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let search = SearchService::default();
        search.scan_wiki(&context, &HashSet::new()).unwrap();
        std::fs::write(&path, "# After!\n\n[[lost-b]]").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        let service = LintService::default();
        let scan = service
            .run_health_local_scan(&context, &search, |_| Ok(()))
            .unwrap();
        assert!(scan.current);
        let deep = service.prepare_health_deep_snapshot_from_scan(&scan, "en");
        assert!(deep.prompt.contains("After!"));
        assert!(!deep.prompt.contains("Before"));
        assert!(scan
            .report
            .issues
            .iter()
            .any(|issue| issue.target.as_deref() == Some("lost-b")));
        assert!(!scan
            .report
            .issues
            .iter()
            .any(|issue| issue.target.as_deref() == Some("lost-a")));
        // Repair verification and the local IPC entry point must use exactly
        // the same fresh metadata, even with a warmed SearchService cache.
        let local = service.run_local_lint(&context, &search).unwrap();
        assert_eq!(local.issues, scan.report.issues);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_only_scan_has_no_ai_excerpts_and_preserves_rule_results() {
        let (context, root) = tmp_context("health-local-no-prompt");
        seed_clean_vault(&context);
        let lint = LintService::default();
        let search = SearchService::default();
        let local = lint
            .run_health_scan(&context, &search, false, |_| Ok(()))
            .unwrap();
        let complete = lint
            .run_health_scan(&context, &search, true, |_| Ok(()))
            .unwrap();
        assert!(local.prompt_blocks.is_empty());
        assert!(!complete.prompt_blocks.is_empty());
        assert_eq!(local.report.issues, complete.report.issues);
        assert_eq!(local.input_hashes, complete.input_hashes);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn large_guidance_and_local_baseline_leave_room_for_analyzed_pages() {
        let (context, root) = tmp_context("health-prompt-reservation");
        seed_clean_vault(&context);
        write_file(&context, "purpose.md", &"项目说明<数据>".repeat(30_000));
        write_file(&context, "schema.md", &"页面规范".repeat(40_000));
        let lint = LintService::default();
        let mut scan = lint
            .run_health_local_scan(&context, &SearchService::default(), |_| Ok(()))
            .unwrap();
        scan.report.issues = vec![
            local_issue(
                LintIssueType::DeadLink,
                LintSeverity::Error,
                "wiki/index.md",
                &"long deterministic finding ".repeat(100),
                None,
                None,
            );
            2_000
        ];
        let deep = lint.prepare_health_deep_snapshot_from_scan(&scan, "zh-CN");
        assert!(deep.prompt.chars().count() <= DEEP_LINT_PROMPT_BUDGET_CHARS);
        assert_eq!(deep.deep_covered_pages, 4);
        assert!(deep.prompt.contains("Links to [[react]]"));
        assert!(deep.prompt.contains("additional local findings omitted"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_resource_findings_include_frontmatter_and_have_distinct_ids() {
        let (context, root) = tmp_context("health-source-resources");
        write_file(&context, "raw/extracted/来源.md",
            "---\ntitle: 来源\nsources: ['../附件甲.pdf']\n---\n\n# 来源\n![乙](../附件乙.png)\n![丙](../附件丙.png)\n![乙](../附件乙.png)");
        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let resources = report
            .issues
            .iter()
            .filter(|issue| issue.issue_type == LintIssueType::MissingResource)
            .collect::<Vec<_>>();
        assert_eq!(resources.len(), 3);
        assert_eq!(
            resources
                .iter()
                .map(|issue| &issue.id)
                .collect::<HashSet<_>>()
                .len(),
            3
        );
        assert!(resources
            .iter()
            .all(|issue| issue.fixability == Fixability::None && issue.scan_hash.is_some()));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn layout_structural_pages_are_exempt_from_content_and_orphan_rules() {
        let (mut context, root) = tmp_context("health-structural-layout");
        context.layout.wiki_index_path = Some("wiki/入口.md".into());
        context.layout.wiki_overview_path = Some("wiki/概览.md".into());
        context.layout.activity_log_path = Some("wiki/记录.md".into());
        for path in ["wiki/入口.md", "wiki/概览.md", "wiki/记录.md"] {
            write_file(&context, path, "# 结构页\n\n说明内容");
        }
        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn overlapping_native_source_root_never_exposes_an_automatic_fix() {
        let (context, root) = tmp_context("health-protected-source");
        write_file(&context, "wiki/sources/来源.md", "# Source\n\n[[missing]]");
        let scan = LintService::default()
            .run_health_local_scan(&context, &SearchService::default(), |_| Ok(()))
            .unwrap();
        assert_eq!(scan.report.scanned_pages, 1);
        assert_eq!(scan.source_pages, 1);
        assert!(scan.not_applicable_rules.contains(&"index_drift".into()));
        assert!(!scan
            .report
            .issues
            .iter()
            .any(|issue| issue.issue_type == LintIssueType::IndexDrift));
        assert!(scan
            .report
            .issues
            .iter()
            .filter(|issue| issue.path == "wiki/sources/来源.md")
            .all(|issue| issue.fixability == Fixability::None));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn shared_resource_evidence_is_not_overwritten_by_a_later_page() {
        let (context, root) = tmp_context("health-shared-asset");
        for path in ["wiki/a.md", "wiki/b.md"] {
            write_file(&context, path, "# Page\n\n![asset](../raw/shared.png)");
        }
        let mut changed = false;
        let scan = LintService::default()
            .run_health_local_scan(&context, &SearchService::default(), |event| {
                if !changed && event.phase == HealthScanPhase::Markdown && event.completed == 1 {
                    std::fs::create_dir_all(root.join("raw")).unwrap();
                    std::fs::write(root.join("raw/shared.png"), "image").unwrap();
                    changed = true;
                }
                Ok(())
            })
            .unwrap();
        assert!(!scan.current);
        assert_eq!(scan.input_hashes["resource-exists://raw/shared.png"], None);
        assert_eq!(
            scan.report
                .issues
                .iter()
                .filter(|issue| issue.issue_type == LintIssueType::MissingResource)
                .count(),
            2
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
