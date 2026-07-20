use sha2::{Digest, Sha256};
use std::collections::HashMap;

use crate::errors::BackendError;
use crate::models::confirmation::{ActionPreview, PendingAction, PendingActionType, RiskLevel};
use crate::models::lint::{
    LintBatchConfirmation, LintBatchOutcome, LintBatchSkip, LintFixOutcome, LintFixOutcomeKind,
    LintIssue, LintIssueType,
};
use crate::models::paths::ProjectContext;
use crate::models::wiki::WikiPageType;
use crate::services::file_store::FileStore;
use crate::services::{GitService, WriteMode};
use crate::utils::markdown_utils::{
    extract_title, parse_frontmatter, split_frontmatter, Frontmatter,
};

use super::rules::file_stem;
use super::LintService;

impl LintService {
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
        let _fix_guard = self.fix_write_lock.lock().map_err(|_| {
            BackendError::new(
                "LINT_FIX_LOCK_FAILED",
                "Another Lint mutation is currently running.",
                true,
                true,
            )
        })?;
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
        if matches!(
            issue.issue_type,
            LintIssueType::MissingFrontmatter | LintIssueType::DeadLink | LintIssueType::IndexDrift
        ) {
            validate_issue_shape(issue)?;
        }
        let mut outcome = match issue.issue_type {
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
        }?;
        if let Some(action) = outcome.pending_action.as_mut() {
            action.affected_paths = fix_affected_paths(context, &issue.path);
        }
        if outcome.kind == LintFixOutcomeKind::Applied {
            // Capture the exact post-write state before verification/final
            // commit. If an external editor changes a path while verification
            // or Git is running, rollback must preserve that newer content
            // instead of restoring HEAD over it.
            let post_write_hashes = self.capture_path_hashes(context, &outcome.affected_paths)
                .map_err(|error| {
                    BackendError::new(
                        "LINT_FIX_POST_HASH_FAILED",
                        format!(
                            "The fix was written, but its post-write state could not be verified: {}",
                            error.message
                        ),
                        true,
                        true,
                    )
                    .with_details(serde_json::json!({
                        "affectedPaths": &outcome.affected_paths,
                        "cause": error,
                    }))
                })?;
            if let Err(error) = self.verify_local_fix(context, issue) {
                return Err(Self::rollback_after_failure_guarded(
                    context,
                    git_service,
                    &outcome.affected_paths,
                    &post_write_hashes,
                    error,
                ));
            }
            outcome.final_commit = match self.finalize_result(
                context,
                git_service,
                &outcome.affected_paths,
                "After applying wiki lint fix",
            ) {
                Ok(commit) => commit,
                Err(error) => {
                    return Err(Self::rollback_after_failure_guarded(
                        context,
                        git_service,
                        &outcome.affected_paths,
                        &post_write_hashes,
                        error,
                    ))
                }
            };
        }
        Ok(outcome)
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
            final_commit: None,
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
            let preview = self.build_dead_link_preview(context, issue)?;
            return Ok(LintFixOutcome {
                kind: LintFixOutcomeKind::NeedsConfirmation,
                affected_paths: Vec::new(),
                checkpoint: None,
                final_commit: None,
                pending_action: Some(dead_link_pending_action_with_preview(
                    path,
                    &target,
                    Some(preview),
                )),
            });
        }
        let (affected_paths, checkpoint) =
            self.write_dead_link_fix(context, git_service, issue, expected_hash, None)?;
        Ok(LintFixOutcome {
            kind: LintFixOutcomeKind::Applied,
            affected_paths,
            checkpoint,
            final_commit: None,
            pending_action: None,
        })
    }

    fn apply_index_drift_fix(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        issue: &LintIssue,
        confirm_high_risk: bool,
        expected_hash: Option<&str>,
    ) -> Result<LintFixOutcome, BackendError> {
        let path = "wiki/index.md";
        if !confirm_high_risk {
            let target = issue.target.clone().unwrap_or_default();
            let preview = self.build_index_preview(context, issue)?;
            return Ok(LintFixOutcome {
                kind: LintFixOutcomeKind::NeedsConfirmation,
                affected_paths: Vec::new(),
                checkpoint: None,
                final_commit: None,
                pending_action: Some(index_drift_pending_action(
                    path,
                    &target,
                    &issue.message,
                    Some(preview),
                )),
            });
        }
        let (affected_paths, checkpoint) =
            self.write_index_drift_fix(context, git_service, issue, expected_hash, None)?;
        Ok(LintFixOutcome {
            kind: LintFixOutcomeKind::Applied,
            affected_paths,
            checkpoint,
            final_commit: None,
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
        validate_scan_hash(issue, expected)?;
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
        if page_type == WikiPageType::Other {
            return Err(BackendError::new(
                "LINT_FIX_TYPE_REQUIRED",
                "This page is outside a recognized wiki type folder; choose a page type before adding frontmatter.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": path })));
        }
        let stem = file_stem(path).unwrap_or_else(|| "page".to_string());
        let title = extract_title(&split.body, &Frontmatter::empty(), &stem);
        let header = format!(
            "---\ntype: {}\ntitle: {}\n---\n\n",
            page_type_name(page_type),
            yaml_scalar(&title)
        );
        let new_contents = format!("{header}{}", raw);

        let affected_paths = fix_affected_paths(context, path);
        let mut expected_after = self.capture_path_hashes(context, &affected_paths)?;
        let checkpoint = self.resolve_checkpoint(
            context,
            git_service,
            &affected_paths,
            shared_checkpoint,
            "Before applying wiki lint fix",
        )?;
        self.file_store.write_markdown_checked(
            context,
            path,
            &new_contents,
            WriteMode::OverwriteIfHashMatches(expected.to_string()),
        )?;
        expected_after.insert(path.to_string(), Some(hash_text(&new_contents)));
        if let Err(error) = invalidate_graph_cache(context) {
            return Err(if shared_checkpoint.is_none() {
                Self::rollback_after_failure_guarded(
                    context,
                    git_service,
                    &affected_paths,
                    &expected_after,
                    error,
                )
            } else {
                attach_post_write_hashes(error, &expected_after)
            });
        }
        if context.app_dir.join("graph-cache.json").exists() {
            expected_after.insert(".app/graph-cache.json".into(), None);
        }
        if let Err(error) = append_fix_log(context, path, "added frontmatter") {
            return Err(if shared_checkpoint.is_none() {
                Self::rollback_after_failure_guarded(
                    context,
                    git_service,
                    &affected_paths,
                    &expected_after,
                    error,
                )
            } else {
                attach_post_write_hashes(error, &expected_after)
            });
        }
        expected_after.insert(
            "wiki/log.md".into(),
            hash_relative_path(context, "wiki/log.md"),
        );

        Ok((affected_paths, checkpoint))
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
        validate_scan_hash(issue, expected)?;

        let raw = self.file_store.read_markdown(context, path)?;
        if target_exists(context, &target)? {
            return Err(BackendError::new(
                "LINT_FIX_STALE",
                "The link target now exists; reload the lint report before removing it.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": path, "target": target })));
        }
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

        let affected_paths = fix_affected_paths(context, path);
        let mut expected_after = self.capture_path_hashes(context, &affected_paths)?;
        let checkpoint = self.resolve_checkpoint(
            context,
            git_service,
            &affected_paths,
            shared_checkpoint,
            "Before applying wiki lint fix",
        )?;
        // Recheck after checkpoint creation, immediately before the guarded
        // write. A target page/alias created during confirmation must not be
        // silently converted into plain text.
        if target_exists(context, &target)? {
            return Err(BackendError::new(
                "LINT_FIX_STALE",
                "The link target now exists; reload the lint report before removing it.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "path": path, "target": target })));
        }
        self.file_store.write_markdown_checked(
            context,
            path,
            &new_contents,
            WriteMode::OverwriteIfHashMatches(expected.to_string()),
        )?;
        expected_after.insert(path.to_string(), Some(hash_text(&new_contents)));
        if let Err(error) = invalidate_graph_cache(context) {
            return Err(if shared_checkpoint.is_none() {
                Self::rollback_after_failure_guarded(
                    context,
                    git_service,
                    &affected_paths,
                    &expected_after,
                    error,
                )
            } else {
                attach_post_write_hashes(error, &expected_after)
            });
        }
        expected_after.insert(".app/graph-cache.json".into(), None);
        if let Err(error) =
            append_fix_log(context, path, &format!("removed dead link [[{target}]]"))
        {
            return Err(if shared_checkpoint.is_none() {
                Self::rollback_after_failure_guarded(
                    context,
                    git_service,
                    &affected_paths,
                    &expected_after,
                    error,
                )
            } else {
                attach_post_write_hashes(error, &expected_after)
            });
        }
        expected_after.insert(
            "wiki/log.md".into(),
            hash_relative_path(context, "wiki/log.md"),
        );

        Ok((affected_paths, checkpoint))
    }

    /// Read-transform-write for the index-drift fix (confirmed path only). The
    /// index hash is recomputed server-side because regeneration is destructive.
    fn write_index_drift_fix(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        issue: &LintIssue,
        expected_hash: Option<&str>,
        shared_checkpoint: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), BackendError> {
        let path = "wiki/index.md";
        let expected = expected_hash.ok_or_else(|| {
            BackendError::new(
                "LINT_FIX_HASH_REQUIRED",
                "Applying an index fix requires the index hash from the lint report.",
                true,
                true,
            )
        })?;
        validate_scan_hash(issue, expected)?;
        let new_contents = regenerate_index(context, self)?;
        let affected_paths = fix_affected_paths(context, path);
        let mut expected_after = self.capture_path_hashes(context, &affected_paths)?;
        let checkpoint = self.resolve_checkpoint(
            context,
            git_service,
            &affected_paths,
            shared_checkpoint,
            "Before applying wiki lint fix",
        )?;
        self.file_store.write_markdown_checked(
            context,
            path,
            &new_contents,
            WriteMode::OverwriteIfHashMatches(expected.to_string()),
        )?;
        expected_after.insert(path.to_string(), Some(hash_text(&new_contents)));
        if let Err(error) = invalidate_graph_cache(context) {
            return Err(if shared_checkpoint.is_none() {
                Self::rollback_after_failure_guarded(
                    context,
                    git_service,
                    &affected_paths,
                    &expected_after,
                    error,
                )
            } else {
                attach_post_write_hashes(error, &expected_after)
            });
        }
        expected_after.insert(".app/graph-cache.json".into(), None);
        if let Err(error) = append_fix_log(context, path, "regenerated index") {
            return Err(if shared_checkpoint.is_none() {
                Self::rollback_after_failure_guarded(
                    context,
                    git_service,
                    &affected_paths,
                    &expected_after,
                    error,
                )
            } else {
                attach_post_write_hashes(error, &expected_after)
            });
        }
        expected_after.insert(
            "wiki/log.md".into(),
            hash_relative_path(context, "wiki/log.md"),
        );

        Ok((affected_paths, checkpoint))
    }

    fn build_dead_link_preview(
        &self,
        context: &ProjectContext,
        issue: &LintIssue,
    ) -> Result<(String, String, String), BackendError> {
        let path = &issue.path;
        let target = issue.target.as_deref().unwrap_or_default();
        let expected = issue.scan_hash.as_deref().ok_or_else(|| {
            BackendError::new(
                "LINT_FIX_SCAN_BASELINE_REQUIRED",
                "Previewing a dead-link fix requires the page hash from the lint report.",
                true,
                true,
            )
        })?;
        let current_hash = self.file_store.file_hash(context, path)?;
        if current_hash != expected {
            return Err(BackendError::new(
                "LINT_FIX_STALE",
                "The page changed after this finding was produced; run lint again.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "path": path,
                "expectedHash": expected,
                "currentHash": current_hash,
            })));
        }
        if target_exists(context, target)? {
            return Err(BackendError::new(
                "LINT_FIX_STALE",
                "The link target now exists; reload the lint report before removing it.",
                true,
                true,
            ));
        }
        let before = self.file_store.read_markdown(context, path)?;
        let after = strip_wikilink(&before, target);
        if before == after {
            return Err(BackendError::new(
                "LINT_FIX_STALE",
                "The reported wikilink is no longer present; reload the lint report.",
                true,
                true,
            ));
        }
        Ok((
            before.clone(),
            after.clone(),
            render_text_diff(path, &before, &after),
        ))
    }

    fn build_index_preview(
        &self,
        context: &ProjectContext,
        issue: &LintIssue,
    ) -> Result<(String, String, String), BackendError> {
        let path = "wiki/index.md";
        let expected = issue.scan_hash.as_deref().ok_or_else(|| {
            BackendError::new(
                "LINT_FIX_SCAN_BASELINE_REQUIRED",
                "Previewing an index fix requires the index hash from the lint report.",
                true,
                true,
            )
        })?;
        let current_hash = self.file_store.file_hash(context, path)?;
        if current_hash != expected {
            return Err(BackendError::new(
                "LINT_FIX_STALE",
                "The index changed after this finding was produced; run lint again.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "path": path,
                "expectedHash": expected,
                "currentHash": current_hash,
            })));
        }
        let before = self.file_store.read_markdown(context, path)?;
        let after = regenerate_index(context, self)?;
        let diff = render_text_diff(path, &before, &after);
        Ok((before, after, diff))
    }

    /// Resolve the checkpoint for a write: reuse a caller-provided shared
    /// checkpoint (batch flow) or create a per-path scoped checkpoint
    /// (single-fix flow). A missing repo surfaces as an error so the user can
    /// init Git rather than lose the prior content to an un-checkpointed write.
    fn resolve_checkpoint(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        paths: &[String],
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
                paths,
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
                .with_details(serde_json::json!({ "paths": paths }))
            })?;
        Ok(checkpoint.commit_hash)
    }

    fn verify_local_fix(
        &self,
        context: &ProjectContext,
        issue: &LintIssue,
    ) -> Result<(), BackendError> {
        let report = self.run_local_lint(context, &crate::services::SearchService::default())?;
        if report
            .issues
            .iter()
            .any(|candidate| candidate.id == issue.id)
        {
            return Err(BackendError::new(
                "LINT_FIX_VERIFY_FAILED",
                "The fix was written, but the lint issue remains after verification.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "issueId": issue.id,
                "path": issue.path,
            })));
        }
        Ok(())
    }

    fn capture_path_hashes(
        &self,
        context: &ProjectContext,
        paths: &[String],
    ) -> Result<HashMap<String, Option<String>>, BackendError> {
        paths
            .iter()
            .map(|path| {
                Ok((
                    path.clone(),
                    self.file_store.file_hash_if_exists(context, path)?,
                ))
            })
            .collect()
    }

    fn rollback_after_failure_guarded(
        context: &ProjectContext,
        git_service: &GitService,
        paths: &[String],
        expected_after: &HashMap<String, Option<String>>,
        error: BackendError,
    ) -> BackendError {
        let mut rollback_paths = Vec::new();
        let mut preserved_paths = Vec::new();
        for path in paths {
            let current = hash_relative_path(context, path);
            if expected_after.get(path) == Some(&current) {
                rollback_paths.push(path.clone());
            } else {
                preserved_paths.push(path.clone());
            }
        }
        let original_code = error.code.clone();
        let original_message = error.message.clone();
        match git_service.rollback_paths_to_head_preserving_ignored(context, &rollback_paths, &[]) {
            Ok(()) => BackendError::new(
                "LINT_FIX_ROLLED_BACK",
                format!("Lint fix failed and was rolled back: {original_message}"),
                true,
                true,
            )
            .with_details(serde_json::json!({
                "originalCode": original_code,
                "affectedPaths": paths,
                "rollbackPaths": rollback_paths,
                "preservedExternalPaths": preserved_paths,
                "rollback": "succeeded",
            })),
            Err(rollback_error) => BackendError::new(
                "LINT_FIX_ROLLBACK_FAILED",
                format!(
                    "Lint fix failed ({original_code}) and rollback also failed: {}",
                    rollback_error.message
                ),
                true,
                true,
            )
            .with_details(serde_json::json!({
                "originalCode": original_code,
                "affectedPaths": paths,
                "rollbackPaths": rollback_paths,
                "preservedExternalPaths": preserved_paths,
                "rollback": "failed",
                "rollbackError": rollback_error.message,
            })),
        }
    }

    fn finalize_result(
        &self,
        context: &ProjectContext,
        git_service: &GitService,
        paths: &[String],
        message: &str,
    ) -> Result<Option<String>, BackendError> {
        if paths.is_empty() {
            return Ok(None);
        }
        let checkpoint = git_service
            .create_scoped_checkpoint(
                context,
                crate::models::git::CheckpointPurpose::FinalResult,
                message,
                paths,
            )
            .map_err(|err| {
                BackendError::new(
                    "GIT_FINAL_COMMIT_FAILED",
                    format!("Could not commit the verified lint result: {}", err.message),
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "affectedPaths": paths }))
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
        let _fix_guard = self.fix_write_lock.lock().map_err(|_| {
            BackendError::new(
                "LINT_FIX_LOCK_FAILED",
                "Another Lint mutation is currently running.",
                true,
                true,
            )
        })?;
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
            match issue.issue_type {
                LintIssueType::MissingFrontmatter => {
                    if issue.fixability != crate::models::lint::Fixability::Safe {
                        return Err(BackendError::new(
                            "LINT_FIX_PLAN_INVALID",
                            "A missing-frontmatter batch item must be marked safe.",
                            true,
                            true,
                        ));
                    }
                    validate_issue_shape(issue)?;
                    if let Some(expected) = expected_hashes.get(&issue.path) {
                        validate_scan_hash(issue, expected)?;
                    }
                }
                LintIssueType::DeadLink | LintIssueType::IndexDrift => {
                    if issue.fixability != crate::models::lint::Fixability::HighRisk {
                        return Err(BackendError::new(
                            "LINT_FIX_PLAN_INVALID",
                            "A high-risk batch item must be marked high-risk.",
                            true,
                            true,
                        ));
                    }
                    validate_issue_shape(issue)?;
                    if issue.scan_hash.is_none() {
                        return Err(BackendError::new(
                            "LINT_FIX_SCAN_BASELINE_REQUIRED",
                            "High-risk batch fixes require a scan baseline.",
                            true,
                            true,
                        ));
                    }
                }
                _ => {}
            }
        }

        let mut applied: Vec<LintFixOutcome> = Vec::new();
        let mut needs_confirmation: Vec<LintBatchConfirmation> = Vec::new();
        let mut skipped: Vec<LintBatchSkip> = Vec::new();
        let mut batch_post_write_hashes: HashMap<String, Option<String>> = HashMap::new();

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
        let safe_ready_paths: std::collections::HashSet<String> =
            safe_ready.iter().map(|issue| issue.path.clone()).collect();
        let safe_checkpoint_paths: Vec<String> = safe_ready
            .iter()
            .flat_map(|issue| fix_affected_paths(context, &issue.path))
            .fold(Vec::new(), |mut paths, path| {
                if !paths.contains(&path) {
                    paths.push(path);
                }
                paths
            });

        // One checkpoint over every ready safe path, created before any write.
        // Git is the data-safety boundary, so a checkpoint failure aborts the
        // batch wholesale rather than writing without a rollback point.
        let shared_checkpoint: Option<String> = if safe_ready.is_empty() {
            None
        } else {
            let checkpoint = git_service
                .create_scoped_checkpoint(
                    context,
                    crate::models::git::CheckpointPurpose::HighRiskOperation,
                    "Before applying batch wiki lint fixes",
                    &safe_checkpoint_paths,
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
                Ok((affected_paths, _)) => {
                    let hashes = self.capture_path_hashes(context, &affected_paths).map_err(|error| {
                        Self::rollback_after_failure_guarded(
                            context,
                            git_service,
                            &safe_checkpoint_paths,
                            &batch_post_write_hashes,
                            BackendError::new(
                                "LINT_FIX_POST_HASH_FAILED",
                                format!(
                                    "A batch fix was written, but its post-write state could not be verified: {}",
                                    error.message
                                ),
                                true,
                                true,
                            )
                            .with_details(serde_json::json!({
                                "path": issue.path,
                                "cause": error,
                            })),
                        )
                    })?;
                    batch_post_write_hashes.extend(hashes);
                    if let Err(err) = self.verify_local_fix(context, issue) {
                        return Err(Self::rollback_after_failure_guarded(
                            context,
                            git_service,
                            &safe_checkpoint_paths,
                            &batch_post_write_hashes,
                            err,
                        ));
                    } else {
                        applied.push(LintFixOutcome {
                            kind: LintFixOutcomeKind::Applied,
                            affected_paths,
                            checkpoint: shared_checkpoint.clone(),
                            final_commit: None,
                            pending_action: None,
                        });
                    }
                }
                Err(err) => {
                    if let Some(post_write_hashes) = err
                        .details
                        .as_ref()
                        .and_then(|details| details.get("postWriteHashes"))
                    {
                        if let Ok(hashes) = serde_json::from_value::<HashMap<String, Option<String>>>(
                            post_write_hashes.clone(),
                        ) {
                            batch_post_write_hashes.extend(hashes);
                        }
                    }
                    if err.code == "FILE_CHANGED_DURING_WRITE" {
                        // Preserve the raced file, but undo earlier batch
                        // writes under the shared checkpoint so a race cannot
                        // turn an atomic batch into an unexplained partial
                        // application.
                        let raced_path = err
                            .details
                            .as_ref()
                            .and_then(|details| details.get("path"))
                            .and_then(serde_json::Value::as_str);
                        let prior_paths = safe_checkpoint_paths
                            .iter()
                            .filter(|path| Some(path.as_str()) != raced_path)
                            .cloned()
                            .collect::<Vec<_>>();
                        if !prior_paths.is_empty() {
                            let rollback_error = Self::rollback_after_failure_guarded(
                                context,
                                git_service,
                                &prior_paths,
                                &batch_post_write_hashes,
                                err.clone(),
                            );
                            if rollback_error.code == "LINT_FIX_ROLLBACK_FAILED" {
                                return Err(BackendError::new(
                                    "LINT_FIX_ROLLBACK_FAILED",
                                    format!(
                                        "The batch race was detected, but earlier writes could not be rolled back: {}",
                                        rollback_error.message
                                    ),
                                    true,
                                    true,
                                ));
                            }
                        }
                        return Err(err);
                    }
                    return Err(Self::rollback_after_failure_guarded(
                        context,
                        git_service,
                        &safe_checkpoint_paths,
                        &batch_post_write_hashes,
                        err,
                    ));
                }
            }
        }

        let mut confirmation_paths = std::collections::HashSet::new();
        for issue in issues {
            match issue.issue_type {
                LintIssueType::DeadLink => {
                    if safe_ready_paths.contains(&issue.path)
                        || !confirmation_paths.insert(issue.path.clone())
                    {
                        skipped.push(LintBatchSkip {
                            issue_id: issue.id.clone(),
                            path: issue.path.clone(),
                            reason_code: "LINT_FIX_BATCH_COALESCED".into(),
                            reason: "Another fix for this page is already in this batch; rerun lint after it completes.".into(),
                        });
                        continue;
                    }
                    let target = issue.target.clone().unwrap_or_default();
                    match self.build_dead_link_preview(context, issue) {
                        Ok(preview) => needs_confirmation.push(LintBatchConfirmation {
                            issue: issue.clone(),
                            pending_action: {
                                let mut action = dead_link_pending_action_with_preview(
                                    &issue.path,
                                    &target,
                                    Some(preview),
                                );
                                action.affected_paths = fix_affected_paths(context, &issue.path);
                                action
                            },
                        }),
                        Err(error) => skipped.push(LintBatchSkip {
                            issue_id: issue.id.clone(),
                            path: issue.path.clone(),
                            reason_code: error.code,
                            reason: error.message,
                        }),
                    }
                }
                LintIssueType::IndexDrift => {
                    if !confirmation_paths.insert(issue.path.clone()) {
                        skipped.push(LintBatchSkip {
                            issue_id: issue.id.clone(),
                            path: issue.path.clone(),
                            reason_code: "LINT_FIX_BATCH_COALESCED".into(),
                            reason: "Index changes are coalesced into one confirmation; rerun lint after it completes.".into(),
                        });
                        continue;
                    }
                    let target = issue.target.clone().unwrap_or_default();
                    match self.build_index_preview(context, issue) {
                        Ok(preview) => needs_confirmation.push(LintBatchConfirmation {
                            issue: issue.clone(),
                            pending_action: {
                                let mut action = index_drift_pending_action(
                                    "wiki/index.md",
                                    &target,
                                    &issue.message,
                                    Some(preview),
                                );
                                action.affected_paths =
                                    fix_affected_paths(context, "wiki/index.md");
                                action
                            },
                        }),
                        Err(error) => skipped.push(LintBatchSkip {
                            issue_id: issue.id.clone(),
                            path: issue.path.clone(),
                            reason_code: error.code,
                            reason: error.message,
                        }),
                    }
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

        let final_paths: Vec<String> = applied
            .iter()
            .flat_map(|outcome| outcome.affected_paths.iter().cloned())
            .collect();
        let final_commit = match self.finalize_result(
            context,
            git_service,
            &final_paths,
            "After applying batch wiki lint fixes",
        ) {
            Ok(commit) => commit,
            Err(error) => {
                return Err(Self::rollback_after_failure_guarded(
                    context,
                    git_service,
                    &safe_checkpoint_paths,
                    &batch_post_write_hashes,
                    error,
                ))
            }
        };
        for outcome in &mut applied {
            outcome.final_commit = final_commit.clone();
        }

        Ok(LintBatchOutcome {
            checkpoint: shared_checkpoint,
            final_commit,
            applied,
            needs_confirmation,
            skipped,
        })
    }
}

/// Preserve the post-write CAS baselines when a shared-checkpoint item fails
/// after its markdown write. The batch caller can then roll back this item
/// together with earlier safe writes instead of treating it as an untouched
/// path and leaving a partial mutation on disk.
fn attach_post_write_hashes(
    mut error: BackendError,
    expected_after: &HashMap<String, Option<String>>,
) -> BackendError {
    let mut details = match error.details.take() {
        Some(serde_json::Value::Object(details)) => details,
        _ => serde_json::Map::new(),
    };
    details.insert(
        "postWriteHashes".into(),
        serde_json::to_value(expected_after).unwrap_or_else(|_| serde_json::json!({})),
    );
    error.details = Some(serde_json::Value::Object(details));
    error
}

fn validate_scan_hash(issue: &LintIssue, expected: &str) -> Result<(), BackendError> {
    let Some(scan_hash) = issue.scan_hash.as_deref() else {
        return Err(BackendError::new(
            "LINT_FIX_SCAN_BASELINE_REQUIRED",
            "This fix was not produced from a versioned lint report. Run lint again.",
            true,
            true,
        ));
    };
    if scan_hash != expected {
        return Err(BackendError::new(
            "LINT_FIX_SCAN_BASELINE_MISMATCH",
            "The fix plan is based on a different file version. Run lint again.",
            true,
            true,
        )
        .with_details(serde_json::json!({
            "path": issue.path,
            "scanHash": scan_hash,
            "expectedHash": expected,
        })));
    }
    Ok(())
}

fn validate_issue_shape(issue: &LintIssue) -> Result<(), BackendError> {
    let target = issue.target.as_deref().unwrap_or_default();
    let valid = issue.source == crate::models::lint::LintIssueSource::Local
        && match issue.issue_type {
            LintIssueType::MissingFrontmatter => {
                issue.path != "wiki/index.md"
                    && issue.path != "wiki/log.md"
                    && issue.id == format!("missing_frontmatter:{}", issue.path)
            }
            LintIssueType::DeadLink => {
                !target.is_empty() && issue.id == format!("dead_link:{}:{target}", issue.path)
            }
            LintIssueType::IndexDrift => {
                issue.path == "wiki/index.md"
                    && !target.is_empty()
                    && issue.id == format!("index_drift:wiki/index.md:{target}")
            }
            _ => false,
        };
    if valid {
        return Ok(());
    }
    Err(BackendError::new(
        "LINT_FIX_PLAN_INVALID",
        "The supplied lint issue is not a server-issued deterministic fix plan.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "issueId": issue.id, "path": issue.path })))
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
        let raw = std::fs::read_to_string(absolute).map_err(|error| {
            BackendError::new(
                "LINT_INDEX_SOURCE_READ_FAILED",
                error.to_string(),
                true,
                false,
            )
            .with_details(serde_json::json!({ "path": rel }))
        })?;
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
        body.push_str(&format!("- [[{stem}]] — {}\n", markdown_label(title)));
    }
    Ok(body)
}

fn dead_link_pending_action(path: &str, target: &str) -> PendingAction {
    PendingAction {
        id: format!("lint-dead-link-{}", uuid::Uuid::new_v4()),
        action_type: PendingActionType::AgentAutoFix,
        title: "Remove dead wikilink".into(),
        message: format!(
            "Remove the unresolved `[[{target}]]` occurrence reported in {path}; the target is rechecked before writing."
        ),
        risk_level: RiskLevel::High,
        affected_paths: vec![path.into()],
        preview: Some(ActionPreview {
            summary: format!(
                "Replace the reported `[[{target}]]` occurrence with plain text `{target}` and create a Git checkpoint."
            ),
            before: Some(format!("…[[{target}]]…")),
            after: Some(format!("…{target}…")),
            diff: None,
        }),
        expires_at: Some(lint_confirmation_expiry()),
        // Lint high-risk fixes create their scoped checkpoint only after the
        // user confirms; no hash exists to surface at confirmation time.
        checkpoint_hash: None,
    }
}

fn dead_link_pending_action_with_preview(
    path: &str,
    target: &str,
    preview: Option<(String, String, String)>,
) -> PendingAction {
    let mut action = dead_link_pending_action(path, target);
    if let Some((before, after, diff)) = preview {
        action.preview = Some(ActionPreview {
            summary: format!(
                "Replace the reported [[{target}]] occurrence with plain text {target} and create a Git checkpoint."
            ),
            before: Some(before),
            after: Some(after),
            diff: Some(diff),
        });
    }
    action
}

fn index_drift_pending_action(
    path: &str,
    _target: &str,
    message: &str,
    preview: Option<(String, String, String)>,
) -> PendingAction {
    PendingAction {
        // Use a per-action nonce: the registry is app-global, so path/target
        // keys can collide across projects or repeated scans.
        id: format!("lint-index-drift-{}", uuid::Uuid::new_v4()),
        action_type: PendingActionType::AgentAutoFix,
        title: "Regenerate wiki index".into(),
        message: format!("{message} Regenerate {path} from the current page set."),
        risk_level: RiskLevel::High,
        affected_paths: vec![path.into()],
        preview: Some({
            let (before, after, diff) = preview.unwrap_or_else(|| {
                (
                    "<current index>".into(),
                    "<generated index>".into(),
                    "".into(),
                )
            });
            ActionPreview {
                summary:
                    "Overwrite wiki/index.md with an auto-generated page list under a Git checkpoint."
                        .into(),
                before: Some(before),
                after: Some(after),
                diff: Some(diff),
            }
        }),
        expires_at: Some(lint_confirmation_expiry()),
        checkpoint_hash: None,
    }
}

fn lint_confirmation_expiry() -> String {
    (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339()
}

/// Replace the first matching `[[target]]`/`[[target|alias]]` occurrence with
/// its visible label, leaving every other occurrence untouched. A finding is
/// one navigation edge, so replacing the whole page would exceed the reported
/// mutation and make the confirmation preview misleading. Operates on raw
/// markdown so the frontmatter block is preserved.
fn strip_wikilink(raw: &str, target: &str) -> String {
    let wanted = target
        .trim()
        .replace(char::from(92), "/")
        .to_ascii_lowercase();
    let mut out = String::with_capacity(raw.len());
    let mut cursor = 0;
    let mut replaced = false;
    while let Some(relative_start) = raw[cursor..].find("[[") {
        let start = cursor + relative_start;
        out.push_str(&raw[cursor..start]);
        let Some(relative_end) = raw[start + 2..].find("]]") else {
            out.push_str(&raw[start..]);
            return out;
        };
        let end = start + 2 + relative_end;
        let inner = &raw[start + 2..end];
        let (destination, alias) = inner.split_once("|").unwrap_or((inner, ""));
        let base = destination
            .split_once("#")
            .map_or(destination, |(base, _)| base);
        let normalized_base = base
            .trim()
            .replace(char::from(92), "/")
            .to_ascii_lowercase();
        if !replaced && normalized_base == wanted {
            out.push_str(if alias.is_empty() { target } else { alias });
            replaced = true;
        } else {
            out.push_str(&raw[start..end + 2]);
        }
        cursor = end + 2;
    }
    out.push_str(&raw[cursor..]);
    out
}

fn yaml_scalar(value: &str) -> String {
    // JSON string syntax is a strict, portable YAML double-quoted scalar and
    // safely handles dates, booleans, indicators, control characters, CJK,
    // quotes, and backslashes without reimplementing YAML's grammar.
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn markdown_label(value: &str) -> String {
    value
        .replace('\r', " ")
        .replace('\n', " ")
        .trim()
        .to_string()
}

fn hash_relative_path(context: &ProjectContext, path: &str) -> Option<String> {
    FileStore.file_hash_if_exists(context, path).ok().flatten()
}

fn hash_text(contents: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Render a bounded, deterministic text diff for confirmation previews. The
/// complete before/after values remain available beside it; this compact view
/// makes the exact replacement visible without writing a temporary candidate
/// file into the project.
fn render_text_diff(path: &str, before: &str, after: &str) -> String {
    const MAX_CHARS: usize = 12_000;
    let mut diff = format!("```diff\n--- a/{path}\n+++ b/{path}\n");
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let prefix = before_lines
        .iter()
        .zip(after_lines.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = before_lines[prefix..]
        .iter()
        .rev()
        .zip(after_lines[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    for line in &before_lines[prefix..before_lines.len().saturating_sub(suffix)] {
        diff.push('-');
        diff.push_str(line);
        diff.push('\n');
    }
    for line in &after_lines[prefix..after_lines.len().saturating_sub(suffix)] {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff.push_str("```");
    if diff.chars().count() <= MAX_CHARS {
        return diff;
    }
    let trimmed: String = diff.chars().take(MAX_CHARS.saturating_sub(40)).collect();
    format!("{trimmed}\n... diff truncated ...\n```")
}

fn page_type_name(page_type: WikiPageType) -> &'static str {
    match page_type {
        WikiPageType::Entity => "entity",
        WikiPageType::Concept => "concept",
        WikiPageType::Source => "source",
        WikiPageType::Synthesis => "synthesis",
        WikiPageType::Comparison => "comparison",
        WikiPageType::Query => "query",
        WikiPageType::Index => "index",
        WikiPageType::Overview => "overview",
        WikiPageType::Log => "log",
        WikiPageType::Other => "other",
    }
}

fn fix_affected_paths(context: &ProjectContext, path: &str) -> Vec<String> {
    let mut paths = vec![path.to_string()];
    if context.wiki_dir.join("log.md").exists() {
        paths.push("wiki/log.md".to_string());
    }
    if context.app_dir.join("graph-cache.json").exists() {
        paths.push(".app/graph-cache.json".to_string());
    }
    paths
}

fn target_exists(context: &ProjectContext, target: &str) -> Result<bool, BackendError> {
    let normalized_target = target
        .trim()
        .replace('\\', "/")
        .trim_start_matches("wiki/")
        .trim_end_matches(".md")
        .to_ascii_lowercase();
    if normalized_target.is_empty() {
        return Ok(false);
    }
    for absolute in FileStore.list_markdown_files(&context.wiki_dir)? {
        let relative = context.to_project_relative(&absolute)?;
        let candidate = relative
            .trim_start_matches("wiki/")
            .trim_end_matches(".md")
            .to_ascii_lowercase();
        if candidate == normalized_target {
            return Ok(true);
        }
        let raw = std::fs::read_to_string(&absolute).map_err(|error| {
            BackendError::new("LINT_TARGET_READ_FAILED", error.to_string(), true, false)
                .with_details(serde_json::json!({ "path": relative }))
        })?;
        let split = split_frontmatter(&raw);
        let frontmatter = split
            .frontmatter
            .as_deref()
            .map(parse_frontmatter)
            .unwrap_or_default();
        let stem = file_stem(&relative).unwrap_or_else(|| relative.clone());
        let title = extract_title(&split.body, &frontmatter, &stem);
        let matches_label = |value: &str| {
            value.trim().trim_end_matches(".md").to_ascii_lowercase() == normalized_target
        };
        if matches_label(&title)
            || frontmatter
                .get_list("aliases")
                .iter()
                .any(|alias| matches_label(alias))
            || frontmatter
                .get_scalar("title")
                .is_some_and(|title| matches_label(&title))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn invalidate_graph_cache(context: &ProjectContext) -> Result<(), BackendError> {
    let path = context.app_dir.join("graph-cache.json");
    if path.exists() {
        std::fs::remove_file(&path).map_err(|err| {
            BackendError::new(
                "LINT_GRAPH_CACHE_INVALIDATE_FAILED",
                format!("Could not invalidate graph cache: {err}"),
                true,
                true,
            )
        })?;
    }
    Ok(())
}

fn append_fix_log(
    context: &ProjectContext,
    relative_path: &str,
    action: &str,
) -> Result<(), BackendError> {
    let log_path = context.wiki_dir.join("log.md");
    if !log_path.exists() {
        return Ok(());
    }
    let stamp = chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string();
    let line = format!("- [{}] {} · lint ({})\n", stamp, relative_path, action);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&log_path)
        .map_err(|err| {
            BackendError::new("LINT_FIX_LOG_OPEN_FAILED", err.to_string(), true, true)
        })?;
    use std::io::Write;
    file.write_all(line.as_bytes())
        .map_err(|err| BackendError::new("LINT_FIX_LOG_WRITE_FAILED", err.to_string(), true, true))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::models::lint::{Fixability, LintFixOutcomeKind, LintIssueType, LintSeverity};
    use crate::models::paths::ProjectContext;
    use crate::services::{GitService, SearchService};

    use super::super::test_support::{tmp_context, write_file};
    use super::super::LintService;
    use super::strip_wikilink;

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
        assert!(on_disk.contains("type: concept\n"));
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
            scan_hash: None,
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

        let hash = issue.scan_hash.clone().expect("lint report hash");
        let applied = service
            .apply_fix(&context, &git, &issue, true, Some(&hash))
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
        assert_eq!(
            strip_wikilink("see [[Ghost#intro]] and [[ghost#x|the ghost]].", "ghost"),
            "see ghost and [[ghost#x|the ghost]]."
        );
    }

    /// Count commits on HEAD; used to prove the batch creates one shared
    /// pre-fix checkpoint and one final-result commit rather than one pair per
    /// fix.
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
            2,
            "batch should create one pre-fix checkpoint and one final-result commit"
        );
        let cp = outcome.checkpoint.clone().expect("shared checkpoint hash");
        assert!(!cp.is_empty());
        assert!(outcome.final_commit.is_some());
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
    fn batch_skips_stale_high_risk_preview_without_discarding_safe_result() {
        let (context, root) = tmp_context("batch-stale-preview");
        write_file(
            &context,
            "wiki/concepts/agent.md",
            "---\ntitle: Agent\ntype: concept\n---\n\n# Agent\n\nSee [[ghost]].",
        );
        write_file(&context, "wiki/concepts/bare.md", "# Bare\n\nContent.");
        write_file(&context, "wiki/index.md", "# Index\n");
        write_file(&context, "wiki/log.md", "# Log\n");
        let git = GitService;
        git.initialize_repository(&context, "init").unwrap();
        write_file(
            &context,
            "wiki/concepts/bare.md",
            "# Bare\n\nEdited content.",
        );

        let service = LintService::default();
        let report = service
            .run_local_lint(&context, &SearchService::default())
            .unwrap();
        let mut issues = report.issues.clone();
        let safe = issues
            .iter()
            .find(|issue| issue.issue_type == LintIssueType::MissingFrontmatter)
            .expect("safe issue expected")
            .path
            .clone();
        let stale_id = {
            let stale = issues
                .iter_mut()
                .find(|issue| issue.issue_type == LintIssueType::DeadLink)
                .expect("dead-link issue expected");
            stale.scan_hash = Some("stale-after-scan".into());
            stale.id.clone()
        };
        let expected_hashes = HashMap::from([(
            safe.clone(),
            service.file_store.file_hash(&context, &safe).unwrap(),
        )]);

        let outcome = service
            .apply_fixes_batch(&context, &git, &issues, &expected_hashes)
            .unwrap();
        assert!(outcome
            .applied
            .iter()
            .any(|item| item.affected_paths.contains(&safe)));
        assert!(outcome
            .skipped
            .iter()
            .any(|item| { item.issue_id == stale_id && item.reason_code == "LINT_FIX_STALE" }));
        assert!(
            !std::fs::read_to_string(context.resolve_project_path(&safe).unwrap())
                .unwrap()
                .is_empty()
        );
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
            scan_hash: None,
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
