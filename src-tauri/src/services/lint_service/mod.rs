mod deep;
mod fixes;
mod ignores;
mod reports;
mod rules;

#[cfg(test)]
mod test_support;

use std::collections::{HashMap, HashSet};

use crate::errors::BackendError;
use crate::models::confirmation::{ActionPreview, PendingAction, PendingActionType, RiskLevel};
use crate::models::lint::{
    Fixability, LintAgentIssue, LintBatchConfirmation, LintBatchOutcome, LintBatchSkip,
    LintFixOutcome, LintFixOutcomeKind, LintIssue, LintIssueSource, LintIssueType, LintSeverity,
};
use crate::models::paths::ProjectContext;
use crate::models::wiki::WikiPageType;
use crate::services::file_store::FileStore;
use crate::services::{GitService, SearchService, WriteMode};
use crate::utils::markdown_utils::{
    extract_title, parse_frontmatter, split_frontmatter, Frontmatter,
};

use self::rules::{file_stem, lint_issue_type_id};

const DEEP_LINT_EXCERPT_CHARS: usize = 1000;
pub(crate) const LINT_REPORTS_DIR: &str = ".app/lint-reports";

/// Local deterministic lint + Agent deep-lint orchestration.
///
/// Local lint never calls a model: it walks the page metadata produced by
/// [SearchService::scan_wiki] and emits deterministic issues. Deep lint is
/// driven by the wiki-lint Skill through AgentService; this service only
/// assembles the prompt and parses the structured output.
#[derive(Default)]
pub struct LintService {
    pub(super) file_store: FileStore,
}

pub fn has_human_readable_sources_section(body: &str) -> bool {
    rules::has_human_readable_sources_section(body)
}

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
        let (affected_paths, checkpoint) =
            self.write_missing_frontmatter_fix(context, git_service, issue, expected_hash, None)?;
        Ok(LintFixOutcome {
            kind: LintFixOutcomeKind::Applied,
            affected_paths,
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
        let (affected_paths, checkpoint) =
            self.write_dead_link_fix(context, git_service, issue, expected_hash, None)?;
        Ok(LintFixOutcome {
            kind: LintFixOutcomeKind::Applied,
            affected_paths,
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
            let target = issue.target.clone().unwrap_or_default();
            return Ok(LintFixOutcome {
                kind: LintFixOutcomeKind::NeedsConfirmation,
                affected_paths: Vec::new(),
                checkpoint: None,
                pending_action: Some(index_drift_pending_action(path, &target, &issue.message)),
            });
        }
        let (affected_paths, checkpoint) =
            self.write_index_drift_fix(context, git_service, issue, None)?;
        Ok(LintFixOutcome {
            kind: LintFixOutcomeKind::Applied,
            affected_paths,
            checkpoint,
            pending_action: None,
        })
    }

    /// Read-transform-write for the missing-frontmatter fix without wrapping
    /// the outcome. `shared_checkpoint` lets the batch flow pass a single
    /// pre-created checkpoint hash instead of creating one per path.
    fn write_missing_frontmatter_fix(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        issue: &LintIssue,
        expected_hash: Option<&str>,
        shared_checkpoint: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), BackendError> {
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

        let checkpoint = self.resolve_checkpoint(
            context,
            git_service,
            path,
            shared_checkpoint,
            "Before applying wiki lint fix",
        )?;
        self.file_store.write_markdown_checked(
            context,
            path,
            &new_contents,
            WriteMode::OverwriteIfHashMatches(expected.to_string()),
        )?;
        invalidate_graph_cache(context);
        append_fix_log(context, path, "added frontmatter");

        Ok((vec![path.clone()], checkpoint))
    }

    /// Read-transform-write for the dead-link fix (confirmed path only). The
    /// unconfirmed branch lives in [`Self::apply_dead_link_fix`] / batch.
    fn write_dead_link_fix(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        issue: &LintIssue,
        expected_hash: Option<&str>,
        shared_checkpoint: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), BackendError> {
        let path = &issue.path;
        let target = issue.target.clone().unwrap_or_default();
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

        let checkpoint = self.resolve_checkpoint(
            context,
            git_service,
            path,
            shared_checkpoint,
            "Before applying wiki lint fix",
        )?;
        self.file_store.write_markdown_checked(
            context,
            path,
            &new_contents,
            WriteMode::OverwriteIfHashMatches(expected.to_string()),
        )?;
        invalidate_graph_cache(context);
        append_fix_log(context, path, &format!("removed dead link [[{target}]]"));

        Ok((vec![path.clone()], checkpoint))
    }

    /// Read-transform-write for the index-drift fix (confirmed path only). The
    /// index hash is recomputed server-side because regeneration is destructive.
    fn write_index_drift_fix(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        _issue: &LintIssue,
        shared_checkpoint: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), BackendError> {
        let path = "wiki/index.md";
        let expected = self.file_store.file_hash(context, path)?;
        let new_contents = regenerate_index(context, self)?;
        let checkpoint = self.resolve_checkpoint(
            context,
            git_service,
            path,
            shared_checkpoint,
            "Before applying wiki lint fix",
        )?;
        self.file_store.write_markdown_checked(
            context,
            path,
            &new_contents,
            WriteMode::OverwriteIfHashMatches(expected),
        )?;
        invalidate_graph_cache(context);
        append_fix_log(context, path, "regenerated index");

        Ok((vec![path.into()], checkpoint))
    }

    /// Resolve the checkpoint for a write: reuse a caller-provided shared
    /// checkpoint (batch flow) or create a per-path scoped checkpoint
    /// (single-fix flow). A missing repo surfaces as an error so the user can
    /// init Git rather than lose the prior content to an un-checkpointed write.
    fn resolve_checkpoint(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        path: &str,
        shared_checkpoint: Option<&str>,
        message: &str,
    ) -> Result<Option<String>, BackendError> {
        if let Some(hash) = shared_checkpoint {
            return Ok(Some(hash.to_string()));
        }
        let checkpoint = git_service
            .create_scoped_checkpoint(
                context,
                crate::models::git::CheckpointPurpose::HighRiskOperation,
                message,
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

    /// Apply (or plan) fixes for many issues in one shot (PRD-LINT-003). A
    /// single Git checkpoint protects every safe write so the whole batch can
    /// be rolled back at once; high-risk fixes are returned as confirmations
    /// for unified review instead of being written. Per-item failures are
    /// collected into `skipped` rather than aborting the batch — the checkpoint
    /// already preserves the pre-batch state.
    pub fn apply_fixes_batch(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        issues: &[LintIssue],
        expected_hashes: &HashMap<String, String>,
    ) -> Result<LintBatchOutcome, BackendError> {
        // Defense in depth: validate every path is in-scope before touching
        // anything, so a single out-of-scope payload can't slip through once
        // other writes have started.
        for issue in issues {
            if !issue.path.starts_with("wiki/") || issue.path.contains("..") {
                return Err(BackendError::new(
                    "LINT_FIX_PATH_OUT_OF_SCOPE",
                    "Lint fixes may only target pages inside the wiki/ folder.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": issue.path })));
            }
        }

        let mut applied: Vec<LintFixOutcome> = Vec::new();
        let mut needs_confirmation: Vec<LintBatchConfirmation> = Vec::new();
        let mut skipped: Vec<LintBatchSkip> = Vec::new();

        let safe: Vec<&LintIssue> = issues
            .iter()
            .filter(|issue| issue.issue_type == LintIssueType::MissingFrontmatter)
            .collect();
        // Only safe fixes that carry an optimistic-lock hash can proceed; the
        // rest are skipped up front so we don't create a checkpoint for writes
        // that will never happen.
        let (safe_ready, safe_no_hash): (Vec<&LintIssue>, Vec<&LintIssue>) = safe
            .iter()
            .partition(|issue| expected_hashes.contains_key(&issue.path));
        for issue in safe_no_hash {
            skipped.push(LintBatchSkip {
                issue_id: issue.id.clone(),
                path: issue.path.clone(),
                reason_code: "LINT_FIX_HASH_REQUIRED".into(),
                reason: "Applying a fix requires the page's current hash.".into(),
            });
        }

        // One checkpoint over every ready safe path, created before any write.
        // Git is the data-safety boundary, so a checkpoint failure aborts the
        // batch wholesale rather than writing without a rollback point.
        let shared_checkpoint: Option<String> = if safe_ready.is_empty() {
            None
        } else {
            let safe_paths: Vec<String> = safe_ready.iter().map(|i| i.path.clone()).collect();
            let checkpoint = git_service
                .create_scoped_checkpoint(
                    context,
                    crate::models::git::CheckpointPurpose::HighRiskOperation,
                    "Before applying batch wiki lint fixes",
                    &safe_paths,
                )
                .map_err(|err| {
                    BackendError::new(
                        "GIT_CHECKPOINT_FAILED",
                        format!(
                            "Could not create a Git checkpoint before batch fixing: {}",
                            err.message
                        ),
                        true,
                        true,
                    )
                })?;
            checkpoint.commit_hash
        };

        for issue in &safe_ready {
            let expected = expected_hashes.get(&issue.path).map(String::as_str);
            match self.write_missing_frontmatter_fix(
                context,
                git_service,
                issue,
                expected,
                shared_checkpoint.as_deref(),
            ) {
                Ok((affected_paths, _)) => applied.push(LintFixOutcome {
                    kind: LintFixOutcomeKind::Applied,
                    affected_paths,
                    checkpoint: shared_checkpoint.clone(),
                    pending_action: None,
                }),
                Err(err) => skipped.push(LintBatchSkip {
                    issue_id: issue.id.clone(),
                    path: issue.path.clone(),
                    reason_code: err.code,
                    reason: err.message,
                }),
            }
        }

        for issue in issues {
            match issue.issue_type {
                LintIssueType::DeadLink => {
                    let target = issue.target.clone().unwrap_or_default();
                    needs_confirmation.push(LintBatchConfirmation {
                        issue: issue.clone(),
                        pending_action: dead_link_pending_action(&issue.path, &target),
                    });
                }
                LintIssueType::IndexDrift => {
                    let target = issue.target.clone().unwrap_or_default();
                    needs_confirmation.push(LintBatchConfirmation {
                        issue: issue.clone(),
                        pending_action: index_drift_pending_action(
                            "wiki/index.md",
                            &target,
                            &issue.message,
                        ),
                    });
                }
                LintIssueType::MissingFrontmatter => {} // handled above
                _ => skipped.push(LintBatchSkip {
                    issue_id: issue.id.clone(),
                    path: issue.path.clone(),
                    reason_code: "LINT_FIX_NOT_AUTO".into(),
                    reason: "This issue type has no deterministic auto-fix.".into(),
                }),
            }
        }

        Ok(LintBatchOutcome {
            checkpoint: shared_checkpoint,
            applied,
            needs_confirmation,
            skipped,
        })
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
        // Lint high-risk fixes create their scoped checkpoint only after the
        // user confirms; no hash exists to surface at confirmation time.
        checkpoint_hash: None,
    }
}

fn index_drift_pending_action(path: &str, target: &str, message: &str) -> PendingAction {
    PendingAction {
        // Include the target so multiple index-drift issues (one per stale link
        // in wiki/index.md) get distinct confirmation ids instead of colliding
        // in the registry and silently dropping all but the last.
        id: format!("lint-index-drift-{path}-{target}"),
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
        checkpoint_hash: None,
    }
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
    use super::test_support::{seed_clean_vault, tmp_context, write_file};
    use super::{strip_wikilink, LintService};
    use crate::models::lint::{Fixability, LintFixOutcomeKind, LintIssueType, LintSeverity};
    use crate::models::paths::ProjectContext;
    use crate::services::{GitService, SearchService};
    use std::collections::{HashMap, HashSet};

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

    /// Count commits on HEAD; used to prove the batch creates a single shared
    /// checkpoint rather than one per fix.
    fn commit_count(context: &ProjectContext) -> usize {
        let output = std::process::Command::new("git")
            .args(["rev-list", "--count", "HEAD"])
            .current_dir(&context.root)
            .output()
            .expect("git rev-list must succeed in test");
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<usize>()
            .unwrap_or(0)
    }

    #[test]
    fn batch_fix_uses_one_shared_checkpoint_for_safe_writes() {
        let (context, root) = tmp_context("batch-cp");
        write_file(&context, "wiki/concepts/bare.md", "# Bare\n\n[[react]].");
        write_file(&context, "wiki/concepts/bare2.md", "# Bare2\n\n[[react]].");
        write_file(
            &context,
            "wiki/concepts/react.md",
            "---\ntitle: ReAct\ntype: concept\n---\n\n# ReAct\n\n[[bare]].",
        );
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();
        // Dirty the safe pages so the scoped checkpoint produces a real commit
        // we can count (clean files return created:false with the existing HEAD).
        write_file(
            &context,
            "wiki/concepts/bare.md",
            "# Bare\n\nuncommitted edit [[react]].",
        );
        write_file(
            &context,
            "wiki/concepts/bare2.md",
            "# Bare2\n\nuncommitted edit [[react]].",
        );

        let service = LintService::default();
        let report = service
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let mut expected_hashes: HashMap<String, String> = HashMap::new();
        for issue in &report.issues {
            if issue.issue_type == LintIssueType::MissingFrontmatter {
                expected_hashes.insert(
                    issue.path.clone(),
                    service.file_store.file_hash(&context, &issue.path).unwrap(),
                );
            }
        }
        let before = commit_count(&context);
        let outcome = service
            .apply_fixes_batch(&context, &git, &report.issues, &expected_hashes)
            .unwrap();
        let after = commit_count(&context);

        assert_eq!(outcome.applied.len(), 2, "both safe fixes should apply");
        assert_eq!(
            after - before,
            1,
            "two safe fixes must share a single checkpoint commit"
        );
        let cp = outcome.checkpoint.clone().expect("shared checkpoint hash");
        assert!(!cp.is_empty());
        for applied in &outcome.applied {
            assert_eq!(applied.checkpoint.as_deref(), Some(cp.as_str()));
            assert!(applied.pending_action.is_none());
        }
        for path in ["wiki/concepts/bare.md", "wiki/concepts/bare2.md"] {
            let on_disk =
                std::fs::read_to_string(context.resolve_project_path(path).unwrap()).unwrap();
            assert!(
                on_disk.starts_with("---\n"),
                "{path} should have frontmatter"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_fix_collects_high_risk_skips_non_fixable_and_missing_hash() {
        let (context, root) = tmp_context("batch-partition");
        // Dead link (high-risk) + missing-frontmatter (safe) side by side.
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nSee [[ghost]].",
        );
        write_file(&context, "wiki/concepts/bare.md", "# Bare\n\n[[agent]].");
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
        let report = service
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        // Deliberately pass NO hashes → the safe fix is skipped up front, no
        // checkpoint is created, and the high-risk fix is still surfaced.
        let outcome = service
            .apply_fixes_batch(&context, &git, &report.issues, &HashMap::new())
            .unwrap();

        // Dead link → confirmation, never written.
        assert!(outcome
            .needs_confirmation
            .iter()
            .any(|c| c.issue.issue_type == LintIssueType::DeadLink));
        let agent_disk = std::fs::read_to_string(
            context
                .resolve_project_path("wiki/concepts/agent.md")
                .unwrap(),
        )
        .unwrap();
        assert!(agent_disk.contains("[[ghost]]"));

        // Safe fix skipped for lack of a hash.
        assert!(outcome.skipped.iter().any(|s| {
            s.path == "wiki/concepts/bare.md" && s.reason_code == "LINT_FIX_HASH_REQUIRED"
        }));

        // Nothing applied → no checkpoint.
        assert!(outcome.applied.is_empty());
        assert!(outcome.checkpoint.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_fix_rejects_out_of_scope_path() {
        let (context, root) = tmp_context("batch-scope");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();
        let bad = crate::models::lint::LintIssue {
            id: "missing_frontmatter:../etc/evil".into(),
            source: crate::models::lint::LintIssueSource::Local,
            severity: LintSeverity::Warning,
            issue_type: LintIssueType::MissingFrontmatter,
            path: "../etc/evil.md".into(),
            range: None,
            message: "x".into(),
            evidence: None,
            target: None,
            fixability: Fixability::Safe,
            suggested_action: None,
        };
        let err = LintService::default()
            .apply_fixes_batch(&context, &git, std::slice::from_ref(&bad), &HashMap::new())
            .expect_err("out-of-scope path must abort the batch");
        assert_eq!(err.code, "LINT_FIX_PATH_OUT_OF_SCOPE");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn batch_fix_gives_index_drift_confirmations_distinct_ids() {
        // Two stale links in wiki/index.md must produce two confirmations with
        // distinct ids; otherwise the registry keeps only the last.
        let (context, root) = tmp_context("batch-drift-ids");
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
            "# Index\n\n- [[agent]]\n- [[ghost1]]\n- [[ghost2]]\n",
        );
        write_file(&context, "wiki/log.md", "# Log\n");
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();

        let service = LintService::default();
        let report = service
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let drift_count = report
            .issues
            .iter()
            .filter(|i| {
                i.issue_type == LintIssueType::IndexDrift
                    && matches!(i.target.as_deref(), Some("ghost1" | "ghost2"))
            })
            .count();
        assert_eq!(drift_count, 2);

        let outcome = service
            .apply_fixes_batch(&context, &git, &report.issues, &HashMap::new())
            .unwrap();
        let ids: Vec<&str> = outcome
            .needs_confirmation
            .iter()
            .map(|c| c.pending_action.id.as_str())
            .collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(
            ids.len(),
            unique.len(),
            "index-drift confirmation ids must be distinct, got {ids:?}"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
