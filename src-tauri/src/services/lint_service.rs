use std::collections::HashMap;

use crate::errors::BackendError;
use crate::models::confirmation::{ActionPreview, PendingAction, PendingActionType, RiskLevel};
use crate::models::lint::{
    Fixability, LintAgentIssue, LintFixOutcome, LintFixOutcomeKind, LintIssue, LintIssueSource,
    LintIssueType, LintRange, LintReport, LintSeverity,
};
use crate::models::paths::ProjectContext;
use crate::models::wiki::{WikiPageMeta, WikiPageType};
use crate::services::file_store::FileStore;
use crate::services::{GitService, SearchService, WriteMode};
use crate::utils::markdown_utils::{
    extract_title, extract_wikilinks, parse_frontmatter, split_frontmatter, Frontmatter,
};
use crate::utils::time_utils::now_rfc3339;

const DEEP_LINT_EXCERPT_CHARS: usize = 240;
/// Pages linked from `index.md` aren't "orphans" even though nothing links
/// back to them, and the structural pages themselves are never orphans.
const STRUCTURAL_FILES: &[&str] = &["wiki/index.md", "wiki/overview.md", "wiki/log.md"];

/// Local deterministic lint + Agent deep-lint orchestration.
///
/// Local lint never calls a model: it walks the page metadata produced by
/// [`SearchService::scan_wiki`] and emits deterministic issues. Deep lint is
/// driven by the `wiki-lint` Skill through `AgentService`; this service only
/// assembles the prompt and parses the structured output.
#[derive(Default)]
pub struct LintService {
    file_store: FileStore,
}

impl LintService {
    /// Run every local deterministic rule. No LLM or Agent is invoked.
    pub fn run_local_lint(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
    ) -> Result<LintReport, BackendError> {
        let tree = search_service.scan_wiki(context)?;
        let pages = tree.pages;
        let scanned = pages.len();

        let lookup = build_target_lookup(&pages);
        let inbound = build_inbound_counts(&pages, &lookup);
        let existing: std::collections::HashSet<&str> =
            pages.iter().map(|p| p.path.as_str()).collect();

        let mut issues: Vec<LintIssue> = Vec::new();

        for page in &pages {
            let raw = self
                .file_store
                .read_markdown(context, &page.path)
                .unwrap_or_default();
            let split = split_frontmatter(&raw);
            let frontmatter_present = split.frontmatter.is_some();

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
                    severity: LintSeverity::Warning,
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
        issues.extend(self.check_index_drift(context, &existing));

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
        existing: &std::collections::HashSet<&str>,
    ) -> Vec<LintIssue> {
        let mut issues = Vec::new();
        let Ok(raw) = self.file_store.read_markdown(context, "wiki/index.md") else {
            return issues;
        };
        let split = split_frontmatter(&raw);
        let linked: Vec<String> = extract_wikilinks(&split.body);

        // Ghost links: targets that resolve to no page.
        for target in &linked {
            if is_external(target) || existing.iter().any(|p| *p == target) {
                continue;
            }
            issues.push(LintIssue {
                id: format!("index_drift:wiki/index.md:{target}"),
                source: LintIssueSource::Local,
                severity: LintSeverity::Warning,
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

    /// Assemble the prompt for the `wiki-lint` Skill: purpose, schema, and a
    /// per-page summary with a bounded excerpt. No secret or API key is ever
    /// placed in the prompt.
    pub fn build_deep_lint_prompt(
        &self,
        context: &ProjectContext,
        search_service: &SearchService,
    ) -> Result<String, BackendError> {
        let tree = search_service.scan_wiki(context)?;
        let purpose = self.file_store.read_markdown(context, "purpose.md").ok();
        let schema = self.file_store.read_markdown(context, "schema.md").ok();

        let mut prompt = String::new();
        prompt.push_str(
            "You are linting a local Markdown wiki for structural quality. Judge the wiki \
             across these dimensions only: duplicate_topic, weak_cross_reference, \
             missing_source, schema_mismatch, outdated_content, contradiction. Use the \
             page paths exactly as given. Respond with ONLY a fenced JSON block (```json) \
             containing an array of objects with fields: issueType (one of the six above), \
             severity (error|warning|info), path, message, evidence, suggestion. If there \
             are no issues, respond with an empty array.\n",
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
            if let Ok(content) = search_service.read_page(context, &page.path) {
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
        // Disambiguate ids when the same issue type lands on the same page
        // multiple times (otherwise the frontend's fixStatus/selection map
        // collapses them). Append a per-(type,path) counter only when needed.
        let mut seen: HashMap<String, usize> = HashMap::new();
        Ok(parsed
            .into_iter()
            .map(|agent| {
                let base = format!("{:?}:{}", agent.issue_type, agent.path).to_ascii_lowercase();
                let count = seen.entry(base.clone()).or_insert(0);
                *count += 1;
                let id = if *count > 1 {
                    format!("{base}:{}", count)
                } else {
                    base
                };
                LintIssue {
                    id,
                    source: LintIssueSource::Agent,
                    severity: agent.severity,
                    issue_type: agent.issue_type,
                    path: agent.path,
                    range: None,
                    message: agent.message,
                    evidence: agent.evidence,
                    target: None,
                    // Agent issues are judgment calls; none are auto-fixable.
                    fixability: Fixability::None,
                    suggested_action: agent.suggestion,
                }
            })
            .collect())
    }

    /// Apply (or plan) a fix for a single issue. Deterministic safe fixes
    /// create a Git checkpoint before writing; high-risk fixes return a
    /// `PendingAction` until the caller confirms with `confirm_high_risk`.
    pub fn apply_fix(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        issue: &LintIssue,
        confirm_high_risk: bool,
        expected_hash: Option<&str>,
    ) -> Result<LintFixOutcome, BackendError> {
        // Defense in depth: fixes only ever touch `wiki/` pages. Agent-supplied
        // issue payloads (and a crafted frontend request) could otherwise point
        // at e.g. `.app/settings.json`; reject before any read/write.
        if !issue.path.starts_with("wiki/") || issue.path.contains("..") {
            return Err(BackendError::new(
                "LINT_FIX_PATH_OUT_OF_SCOPE",
                "Lint fixes may only target pages inside the wiki/ folder.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": issue.path })));
        }
        match issue.issue_type {
            LintIssueType::MissingFrontmatter => {
                self.apply_missing_frontmatter(context, git_service, issue, expected_hash)
            }
            LintIssueType::DeadLink => self.apply_dead_link_fix(
                context,
                git_service,
                issue,
                confirm_high_risk,
                expected_hash,
            ),
            LintIssueType::IndexDrift => self.apply_index_drift_fix(
                context,
                git_service,
                issue,
                confirm_high_risk,
                expected_hash,
            ),
            _ => Err(BackendError::new(
                "LINT_FIX_NOT_AUTO",
                "This issue type has no deterministic auto-fix.",
                true,
                true,
            )),
        }
    }

    fn apply_missing_frontmatter(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        issue: &LintIssue,
        expected_hash: Option<&str>,
    ) -> Result<LintFixOutcome, BackendError> {
        let path = &issue.path;
        let expected = expected_hash.ok_or_else(|| {
            BackendError::new(
                "LINT_FIX_HASH_REQUIRED",
                "Applying a fix requires the page's current hash.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": path }))
        })?;
        let raw = self.file_store.read_markdown(context, path)?;
        let split = split_frontmatter(&raw);
        // Don't double-add if a frontmatter block appeared between scan and fix.
        if split.frontmatter.is_some() {
            return Err(BackendError::new(
                "LINT_FIX_STALE",
                "The page already has frontmatter; reload the lint report.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": path })));
        }

        let wiki_relative = path.strip_prefix("wiki/").unwrap_or(path);
        let page_type = WikiPageType::infer(None, wiki_relative);
        let stem = file_stem(path).unwrap_or_else(|| "page".to_string());
        let title = extract_title(&split.body, &Frontmatter::empty(), &stem);
        let header = format!(
            "---\ntype: {:?}\ntitle: {}\n---\n\n",
            page_type,
            yaml_scalar(&title)
        );
        let new_contents = format!("{header}{}", raw);

        let checkpoint = self.checkpoint_path(context, git_service, path)?;
        self.file_store.write_markdown_checked(
            context,
            path,
            &new_contents,
            WriteMode::OverwriteIfHashMatches(expected.to_string()),
        )?;
        invalidate_graph_cache(context);
        append_fix_log(context, path, "added frontmatter");

        Ok(LintFixOutcome {
            kind: LintFixOutcomeKind::Applied,
            affected_paths: vec![path.clone()],
            checkpoint,
            pending_action: None,
        })
    }

    fn apply_dead_link_fix(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        issue: &LintIssue,
        confirm_high_risk: bool,
        expected_hash: Option<&str>,
    ) -> Result<LintFixOutcome, BackendError> {
        let path = &issue.path;
        let target = issue.target.clone().unwrap_or_default();
        if !confirm_high_risk {
            return Ok(LintFixOutcome {
                kind: LintFixOutcomeKind::NeedsConfirmation,
                affected_paths: Vec::new(),
                checkpoint: None,
                pending_action: Some(dead_link_pending_action(path, &target)),
            });
        }
        let expected = expected_hash.ok_or_else(|| {
            BackendError::new(
                "LINT_FIX_HASH_REQUIRED",
                "Applying a high-risk fix requires the page's current hash.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": path }))
        })?;

        let raw = self.file_store.read_markdown(context, path)?;
        let new_contents = strip_wikilink(&raw, &target);
        if new_contents == raw {
            return Err(BackendError::new(
                "LINT_FIX_STALE",
                "The wikilink is no longer present; reload the lint report.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": path, "target": target })));
        }

        let checkpoint = self.checkpoint_path(context, git_service, path)?;
        self.file_store.write_markdown_checked(
            context,
            path,
            &new_contents,
            WriteMode::OverwriteIfHashMatches(expected.to_string()),
        )?;
        invalidate_graph_cache(context);
        append_fix_log(context, path, &format!("removed dead link [[{target}]]"));

        Ok(LintFixOutcome {
            kind: LintFixOutcomeKind::Applied,
            affected_paths: vec![path.clone()],
            checkpoint,
            pending_action: None,
        })
    }

    fn apply_index_drift_fix(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        issue: &LintIssue,
        confirm_high_risk: bool,
        _expected_hash: Option<&str>,
    ) -> Result<LintFixOutcome, BackendError> {
        let path = "wiki/index.md";
        if !confirm_high_risk {
            return Ok(LintFixOutcome {
                kind: LintFixOutcomeKind::NeedsConfirmation,
                affected_paths: Vec::new(),
                checkpoint: None,
                pending_action: Some(index_drift_pending_action(path, &issue.message)),
            });
        }
        let expected = self.file_store.file_hash(context, path)?;
        let new_contents = regenerate_index(context, self)?;
        let checkpoint = self.checkpoint_path(context, git_service, path)?;
        self.file_store.write_markdown_checked(
            context,
            path,
            &new_contents,
            WriteMode::OverwriteIfHashMatches(expected),
        )?;
        invalidate_graph_cache(context);
        append_fix_log(context, path, "regenerated index");

        Ok(LintFixOutcome {
            kind: LintFixOutcomeKind::Applied,
            affected_paths: vec![path.into()],
            checkpoint,
            pending_action: None,
        })
    }

    /// Create a scoped checkpoint for a single path. A missing repo surfaces
    /// as an error so the user can init Git rather than lose the prior content
    /// to an un-checkpointed write (mirrors `ChatService::save_answer_to_wiki`).
    fn checkpoint_path(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        path: &str,
    ) -> Result<Option<String>, BackendError> {
        let checkpoint = git_service
            .create_scoped_checkpoint(
                context,
                crate::models::git::CheckpointPurpose::HighRiskOperation,
                "Before applying wiki lint fix",
                std::slice::from_ref(&path.to_string()),
            )
            .map_err(|err| {
                BackendError::new(
                    "GIT_CHECKPOINT_FAILED",
                    format!(
                        "Could not create a Git checkpoint before fixing: {}",
                        err.message
                    ),
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": path }))
            })?;
        Ok(checkpoint.commit_hash)
    }
}

fn regenerate_index(
    context: &ProjectContext,
    service: &LintService,
) -> Result<String, BackendError> {
    // Re-scan to get the current page set. `FileStore` is private to the
    // module; reach it through a fresh SearchService-free read of the tree by
    // listing markdown files directly.
    let store = FileStore;
    let files = store.list_markdown_files(&context.wiki_dir)?;
    let mut pages: Vec<(String, String)> = Vec::new();
    for absolute in &files {
        let rel = context.to_project_relative(absolute)?;
        if rel == "wiki/index.md" || rel == "wiki/log.md" {
            continue;
        }
        let raw = std::fs::read_to_string(absolute).unwrap_or_default();
        let split = split_frontmatter(&raw);
        let fm = split
            .frontmatter
            .as_deref()
            .map(parse_frontmatter)
            .unwrap_or_default();
        let stem = file_stem(&rel).unwrap_or_else(|| rel.clone());
        let title = extract_title(&split.body, &fm, &stem);
        pages.push((rel, title));
    }
    pages.sort();
    let _ = service; // service param kept for future reuse / symmetry.
    let mut body = String::from("# Index\n\nAutomatically generated by the lint fix flow.\n\n");
    for (rel, title) in &pages {
        let stem = file_stem(rel).unwrap_or_else(|| rel.clone());
        body.push_str(&format!("- [[{stem}]] — {}\n", yaml_scalar(title)));
    }
    Ok(body)
}

fn dead_link_pending_action(path: &str, target: &str) -> PendingAction {
    PendingAction {
        id: format!("lint-dead-link-{path}-{target}"),
        action_type: PendingActionType::AgentAutoFix,
        title: "Remove dead wikilink".into(),
        message: format!("Remove the unresolved `[[{target}]]` from {path}."),
        risk_level: RiskLevel::High,
        affected_paths: vec![path.into()],
        preview: Some(ActionPreview {
            summary: format!(
                "Replace `[[{target}]]` with plain text `{target}` and create a Git checkpoint."
            ),
            before: Some(format!("…[[{target}]]…")),
            after: Some(format!("…{target}…")),
            diff: None,
        }),
        expires_at: None,
    }
}

fn index_drift_pending_action(path: &str, message: &str) -> PendingAction {
    PendingAction {
        id: format!("lint-index-drift-{path}"),
        action_type: PendingActionType::AgentAutoFix,
        title: "Regenerate wiki index".into(),
        message: format!("{message} Regenerate {path} from the current page set."),
        risk_level: RiskLevel::High,
        affected_paths: vec![path.into()],
        preview: Some(ActionPreview {
            summary:
                "Overwrite wiki/index.md with an auto-generated page list under a Git checkpoint."
                    .into(),
            before: None,
            after: None,
            diff: None,
        }),
        expires_at: None,
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

fn file_stem(path: &str) -> Option<String> {
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
    context
        .resolve_project_path(&normalized)
        .map(|p| p.exists())
        .unwrap_or(false)
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

/// Replace `[[target]]` and `[[target|alias]]` with the visible label, leaving
/// the rest of the content untouched. Operates on raw markdown so the
/// frontmatter block is preserved.
fn strip_wikilink(raw: &str, target: &str) -> String {
    let plain = format!("[[{target}]]");
    let label = target.to_string();
    let after_plain = raw.replace(&plain, &label);
    // Replace `[[target|alias]]` with `alias`.
    let mut out = after_plain;
    while let Some(start) = out.find(&format!("[[{target}|")) {
        let after_bracket_start = start + target.len() + 3; // "[[|" -> +3
        if let Some(end) = out[after_bracket_start..].find("]]") {
            let alias_start = after_bracket_start;
            let alias_end = after_bracket_start + end;
            let alias = out[alias_start..alias_end].to_string();
            out.replace_range(start..alias_end + 2, &alias);
        } else {
            break;
        }
    }
    out
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let taken: String = trimmed.chars().take(max_chars).collect();
    format!("{}…", taken.trim_end())
}

fn severity_rank(severity: LintSeverity) -> u8 {
    match severity {
        LintSeverity::Error => 0,
        LintSeverity::Warning => 1,
        LintSeverity::Info => 2,
    }
}

fn yaml_scalar(value: &str) -> String {
    if value.contains(':') || value.contains('[') || value.contains(']') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
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

fn invalidate_graph_cache(context: &ProjectContext) {
    let path = context.app_dir.join("graph-cache.json");
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
}

fn append_fix_log(context: &ProjectContext, relative_path: &str, action: &str) {
    let log_path = context.wiki_dir.join("log.md");
    if !log_path.exists() {
        return;
    }
    let stamp = chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string();
    let line = format!("- [{}] {} · lint ({})\n", stamp, relative_path, action);
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&log_path) {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::{strip_wikilink, LintService};
    use crate::models::lint::{Fixability, LintFixOutcomeKind, LintIssueType, LintSeverity};
    use crate::models::paths::ProjectContext;
    use crate::services::{GitService, SearchService};
    use std::path::PathBuf;

    fn tmp_context(suffix: &str) -> (ProjectContext, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-lint-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    fn write_file(context: &ProjectContext, rel: &str, body: &str) {
        let path = context.resolve_project_path(rel).unwrap();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, body).unwrap();
    }

    /// A vault where every rule is satisfied: frontmatter on content pages,
    /// bidirectional links, index lists the page, no collisions.
    fn seed_clean_vault(context: &ProjectContext) {
        write_file(
            context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\ntags: [ai]\n---\n\n# Agent\n\nLinks to [[react]].",
        );
        write_file(
            context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\ntags: [ai]\n---\n\n# ReAct\n\nLinks back to [[agent]].",
        );
        write_file(
            context,
            "wiki/index.md",
            "# Index\n\n- [[agent]]\n- [[react]]\n",
        );
        write_file(context, "wiki/log.md", "# Log\n");
    }

    #[test]
    fn clean_vault_has_no_local_issues() {
        let (context, root) = tmp_context("clean");
        seed_clean_vault(&context);
        let report = LintService::default()
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        assert_eq!(report.scanned_pages, 4);
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
        write_file(
            &context,
            "wiki/index.md",
            "# Index\n\n- [[hub]]\n- [[spoke]]\n",
        );
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
        assert!(report
            .issues
            .iter()
            .any(|i| i.issue_type == LintIssueType::PathCase));
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
            .build_deep_lint_prompt(&context, &SearchService::default())
            .unwrap();
        assert!(prompt.contains("Purpose"));
        assert!(prompt.contains("Explain agents."));
        assert!(prompt.contains("Schema"));
        assert!(prompt.contains("wiki/concepts/agent.md"));
        assert!(prompt.contains("```json"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn safe_fix_adds_frontmatter_under_checkpoint_and_invalidates_cache() {
        let (context, root) = tmp_context("fix-safe");
        write_file(
            &context,
            "wiki/concepts/bare.md",
            "# Bare\n\nSee [[react]].",
        );
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\n[[bare]].",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        std::fs::create_dir_all(context.app_dir.clone()).unwrap();
        std::fs::write(context.app_dir.join("graph-cache.json"), "{}").unwrap();
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let service = LintService::default();
        let search = SearchService::default();
        let report = service.run_local_lint(&context, &search).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.issue_type == LintIssueType::MissingFrontmatter)
            .unwrap();
        let hash = service.file_store.file_hash(&context, &issue.path).unwrap();

        let outcome = service
            .apply_fix(&context, &git, issue, false, Some(&hash))
            .unwrap();
        assert_eq!(outcome.kind, LintFixOutcomeKind::Applied);
        assert!(outcome.checkpoint.is_some());

        let on_disk =
            std::fs::read_to_string(context.resolve_project_path(&issue.path).unwrap()).unwrap();
        assert!(on_disk.starts_with("---\n"));
        assert!(on_disk.contains("type:"));
        assert!(on_disk.contains("# Bare"));
        assert!(!context.app_dir.join("graph-cache.json").exists());
        let log = std::fs::read_to_string(context.wiki_dir.join("log.md")).unwrap();
        assert!(log.contains("added frontmatter"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn safe_fix_requires_hash() {
        let (context, root) = tmp_context("fix-nohash");
        write_file(&context, "wiki/concepts/bare.md", "# Bare\n\n[[react]].");
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\n[[bare]].",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let service = LintService::default();
        let report = service
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.issue_type == LintIssueType::MissingFrontmatter)
            .unwrap();
        let err = service
            .apply_fix(&context, &git, issue, false, None)
            .expect_err("hash required");
        assert_eq!(err.code, "LINT_FIX_HASH_REQUIRED");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn high_risk_dead_link_fix_returns_pending_then_applies() {
        let (context, root) = tmp_context("fix-deadlink");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nSee [[ghost]].",
        );
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\n[[agent]].",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let service = LintService::default();
        let search = SearchService::default();
        let report = service.run_local_lint(&context, &search).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.issue_type == LintIssueType::DeadLink)
            .unwrap()
            .clone();

        // First without confirmation → needs confirmation, no write.
        let needs = service
            .apply_fix(&context, &git, &issue, false, None)
            .unwrap();
        assert_eq!(needs.kind, LintFixOutcomeKind::NeedsConfirmation);
        assert!(needs.pending_action.is_some());
        let before =
            std::fs::read_to_string(context.resolve_project_path(&issue.path).unwrap()).unwrap();
        assert!(before.contains("[[ghost]]"));

        // Then confirmed with hash → applies.
        let hash = service.file_store.file_hash(&context, &issue.path).unwrap();
        let applied = service
            .apply_fix(&context, &git, &issue, true, Some(&hash))
            .unwrap();
        assert_eq!(applied.kind, LintFixOutcomeKind::Applied);
        let after =
            std::fs::read_to_string(context.resolve_project_path(&issue.path).unwrap()).unwrap();
        assert!(!after.contains("[[ghost]]"));
        assert!(after.contains("ghost")); // plain-text label remains
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn non_fixable_issue_type_is_rejected() {
        let (context, root) = tmp_context("fix-reject");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        let issue = crate::models::lint::LintIssue {
            id: "orphan_page:wiki/x.md".into(),
            source: crate::models::lint::LintIssueSource::Local,
            severity: LintSeverity::Info,
            issue_type: LintIssueType::OrphanPage,
            path: "wiki/x.md".into(),
            range: None,
            message: "orphan".into(),
            evidence: None,
            target: None,
            fixability: Fixability::None,
            suggested_action: None,
        };
        let err = LintService::default()
            .apply_fix(&context, &GitService, &issue, false, None)
            .expect_err("not auto-fixable");
        assert_eq!(err.code, "LINT_FIX_NOT_AUTO");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn index_drift_fix_regenerates_index() {
        let (context, root) = tmp_context("fix-index");
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
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let service = LintService::default();
        let search = SearchService::default();
        let report = service.run_local_lint(&context, &search).unwrap();
        let issue = report
            .issues
            .iter()
            .find(|i| i.issue_type == LintIssueType::IndexDrift)
            .unwrap()
            .clone();

        let needs = service
            .apply_fix(&context, &git, &issue, false, None)
            .unwrap();
        assert_eq!(needs.kind, LintFixOutcomeKind::NeedsConfirmation);

        let applied = service
            .apply_fix(&context, &git, &issue, true, None)
            .unwrap();
        assert_eq!(applied.kind, LintFixOutcomeKind::Applied);
        let index = std::fs::read_to_string(context.resolve_project_path("wiki/index.md").unwrap())
            .unwrap();
        assert!(!index.contains("[[ghost]]"));
        assert!(index.contains("[[agent]]"));
        assert!(index.contains("[[react]]"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn strip_wikilink_handles_alias_form() {
        let raw = "body [[react|the ReAct pattern]] more";
        assert_eq!(strip_wikilink(raw, "react"), "body the ReAct pattern more");
        assert_eq!(strip_wikilink("see [[ghost]].", "ghost"), "see ghost.");
    }
}
