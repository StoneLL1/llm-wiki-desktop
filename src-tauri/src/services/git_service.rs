#[derive(Default)]
pub struct GitService;

#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use crate::errors::BackendError;
use crate::models::git::{
    CheckpointPurpose, GitChangedFile, GitChangedFileKind, GitCheckpoint, GitDiff,
    GitRepositoryStatus,
};
use crate::models::paths::ProjectContext;
use crate::tasks::task_model::CancellationToken;
use crate::utils::path_safety::{
    validate_existing_project_directory, validate_existing_project_file,
    validate_existing_project_root,
};
use crate::utils::process_lifetime::{
    run_bounded_process, BoundedProcessError, CapturedProcessOutput,
};
use crate::utils::safe_project_dir::BoundProjectMutationRoot;

thread_local! {
    static GIT_TASK_CANCELLATION: RefCell<Vec<CancellationToken>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
thread_local! {
    static TEST_GIT_PROCESS_ATTEMPTS: Cell<usize> = const { Cell::new(0) };
}

struct GitTaskCancellationScope;

impl Drop for GitTaskCancellationScope {
    fn drop(&mut self) {
        GIT_TASK_CANCELLATION.with(|tokens| {
            if let Ok(mut tokens) = tokens.try_borrow_mut() {
                tokens.pop();
            }
        });
    }
}

fn git_task_cancelled() -> bool {
    GIT_TASK_CANCELLATION.with(|tokens| {
        tokens.try_borrow().map_or(true, |tokens| {
            tokens.iter().any(CancellationToken::is_cancelled)
        })
    })
}

impl GitService {
    /// Bind nested app-owned Git calls to the current task cancellation token.
    /// The scope is thread-local because the synchronous Git transaction must
    /// stay on the permit-owning thread, and nested scopes compose fail-closed.
    pub(crate) fn with_task_cancellation<T>(
        &self,
        token: CancellationToken,
        operation: impl FnOnce() -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        GIT_TASK_CANCELLATION.with(|tokens| tokens.borrow_mut().push(token));
        let _scope = GitTaskCancellationScope;
        operation()
    }

    pub fn initial_commit_paths(
        &self,
        context: &ProjectContext,
    ) -> Result<Vec<String>, BackendError> {
        let root = validate_existing_project_root(&context.root).map_err(git_path_unsafe)?;
        let mut paths = Vec::new();
        let mut entries_seen = 0;
        collect_initial_commit_paths(&root, &root, 0, &mut entries_seen, &mut paths)?;
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    pub fn verify_initialization_state(
        &self,
        context: &ProjectContext,
        expected_head: Option<&str>,
        expected_paths: &[String],
    ) -> Result<(), BackendError> {
        let current_head = self.repository_status(context)?.head;
        let current_paths = self.initial_commit_paths(context)?;
        if current_head.as_deref() != expected_head || current_paths != expected_paths {
            return Err(BackendError::new(
                "GIT_INITIALIZATION_STATE_CHANGED",
                "Project files no longer match the confirmed Git initialization preview.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "expectedPaths": expected_paths,
                "currentPaths": current_paths,
                "expectedHead": expected_head,
                "currentHead": current_head,
            })));
        }
        Ok(())
    }

    pub fn initialize_repository_from_snapshot(
        &self,
        context: &ProjectContext,
        initial_message: &str,
        expected_paths: &[String],
    ) -> Result<GitRepositoryStatus, BackendError> {
        if !self.repository_status(context)?.is_repository {
            run_git(context, &["init"])?;
        }
        let has_head = run_git(context, &["rev-parse", "--verify", "HEAD"]).is_ok();
        if !has_head {
            let candidates = initial_commit_git_candidates(context)?;
            if candidates
                .iter()
                .any(|candidate| !expected_paths.contains(candidate))
            {
                return Err(BackendError::new(
                    "GIT_INITIALIZATION_STATE_CHANGED",
                    "New project files appeared after the Git initialization preview.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({
                    "expectedPaths": expected_paths,
                    "currentGitCandidates": candidates,
                })));
            }
            if !candidates.is_empty() {
                let literal_candidates = candidates
                    .iter()
                    .map(|path| format!(":(literal){path}"))
                    .collect::<Vec<_>>();
                let mut args = vec!["add", "--"];
                args.extend(literal_candidates.iter().map(String::as_str));
                run_git(context, &args)?;
            }
            let _ = commit_with_message(context, initial_message, true)?;
        }
        self.repository_status(context)
    }

    pub fn repository_status_for_assessment(
        &self,
        context: &ProjectContext,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<GitRepositoryStatus, BackendError> {
        let root = validate_existing_project_root(&context.root).map_err(git_path_unsafe)?;
        let has_git_marker = validate_git_marker(&root)?;
        let version = run_git_bounded(context, &["--version"], deadline, cancelled)?;
        if !version.success {
            return Err(git_command_error(&version.stderr, &["--version"]));
        }

        let top_level = run_git_bounded(
            context,
            &["rev-parse", "--show-toplevel"],
            deadline,
            cancelled,
        )?;
        if !top_level.success {
            if has_git_marker {
                return Err(BackendError::new(
                    "GIT_REPOSITORY_INVALID",
                    "Project-local Git metadata is incomplete or unreadable.",
                    true,
                    true,
                ));
            }
            return Ok(GitRepositoryStatus {
                is_repository: false,
                branch: None,
                head: None,
                has_changes: false,
            });
        }
        let top_level = String::from_utf8_lossy(&top_level.stdout);
        let is_repository = Path::new(top_level.trim())
            .canonicalize()
            .is_ok_and(|candidate| candidate == root);
        if !is_repository {
            return Ok(GitRepositoryStatus {
                is_repository: false,
                branch: None,
                head: None,
                has_changes: false,
            });
        }

        let branch = bounded_optional_git_value(
            context,
            &["branch", "--show-current"],
            deadline,
            cancelled,
        )?;
        let head = bounded_optional_git_value(
            context,
            &["rev-parse", "--short", "HEAD"],
            deadline,
            cancelled,
        )?;
        let status = run_git_bounded(
            context,
            &["status", "--porcelain=v1", "--untracked-files=normal", "--"],
            deadline,
            cancelled,
        )?;
        if !status.success && status.stdout.is_empty() {
            return Err(git_command_error(
                &status.stderr,
                &["status", "--porcelain=v1", "--untracked-files=normal", "--"],
            ));
        }

        Ok(GitRepositoryStatus {
            is_repository: true,
            branch,
            head,
            has_changes: !status.stdout.is_empty(),
        })
    }

    pub fn changed_paths(&self, context: &ProjectContext) -> Result<Vec<String>, BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before enumerating changes.",
                true,
                true,
            ));
        }
        status_paths(context)
    }

    /// Check whether a precise project-relative path is tracked without
    /// staging it. High-risk overwrite flows use this to avoid treating an
    /// ignored app-state file as if a Git checkpoint could recover it.
    pub fn is_path_tracked(
        &self,
        context: &ProjectContext,
        relative_path: &str,
    ) -> Result<bool, BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Ok(false);
        }
        Ok(run_git(
            context,
            &["ls-files", "--error-unmatch", "--", relative_path],
        )
        .is_ok())
    }

    pub fn verify_checkpoint_state(
        &self,
        context: &ProjectContext,
        expected_head: Option<&str>,
        expected_paths: &[String],
    ) -> Result<(), BackendError> {
        let current_head = self.repository_status(context)?.head;
        let current_paths = status_paths(context)?;
        if current_head.as_deref() != expected_head || current_paths != expected_paths {
            return Err(BackendError::new(
                "GIT_CHECKPOINT_STATE_CHANGED",
                "Project changes no longer match the confirmed checkpoint preview.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "expectedPaths": expected_paths,
                "currentPaths": current_paths,
                "expectedHead": expected_head,
                "currentHead": current_head,
            })));
        }
        Ok(())
    }

    pub fn checkpoint_exists(project_root: &Path, checkpoint_hash: &str) -> bool {
        if !(7..=64).contains(&checkpoint_hash.len())
            || !checkpoint_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return false;
        }
        let context = ProjectContext::new("workflow-recovery", project_root.to_path_buf());
        if !GitService
            .repository_status(&context)
            .is_ok_and(|status| status.is_repository)
        {
            return false;
        }
        let commit = format!("{checkpoint_hash}^{{commit}}");
        run_git(&context, &["rev-parse", "--verify", "--quiet", &commit]).is_ok()
    }

    /// Read a UTF-8 project file exactly as it existed at a validated checkpoint.
    /// Missing files are represented as `None`; this never mutates the repository.
    pub fn file_at_checkpoint(
        context: &ProjectContext,
        checkpoint_hash: &str,
        relative_path: &str,
    ) -> Result<Option<String>, BackendError> {
        if !Self::checkpoint_exists(&context.root, checkpoint_hash) {
            return Err(BackendError::new(
                "GIT_CHECKPOINT_INVALID",
                "The requested workflow checkpoint is unavailable.",
                true,
                true,
            ));
        }
        context.resolve_project_path(relative_path)?;
        let spec = format!("{checkpoint_hash}:{relative_path}");
        if run_git(context, &["cat-file", "-e", spec.as_str()]).is_err() {
            return Ok(None);
        }
        let bytes = run_git_bytes(context, &["show", spec.as_str()])?;
        String::from_utf8(bytes).map(Some).map_err(|error| {
            BackendError::new(
                "GIT_CHECKPOINT_FILE_INVALID_UTF8",
                error.to_string(),
                false,
                true,
            )
        })
    }

    pub fn head_subject(context: &ProjectContext) -> Option<String> {
        if !GitService
            .repository_status(context)
            .is_ok_and(|status| status.is_repository)
        {
            return None;
        }
        run_git(context, &["log", "-1", "--pretty=%s"])
            .ok()
            .map(|subject| subject.trim().to_string())
            .filter(|subject| !subject.is_empty())
    }

    /// Render a candidate comparison with Git's existing diff renderer without
    /// staging or mutating either file.
    pub fn diff_candidate_files(
        context: &ProjectContext,
        baseline: &std::path::Path,
        candidate: &std::path::Path,
    ) -> Result<String, BackendError> {
        let root = context.root.canonicalize().map_err(|error| {
            BackendError::new("GIT_DIFF_FAILED", error.to_string(), true, false)
        })?;
        let mut relative = Vec::new();
        for path in [baseline, candidate] {
            let canonical = path.canonicalize().map_err(|error| {
                BackendError::new("GIT_DIFF_FAILED", error.to_string(), true, false)
            })?;
            if !canonical.starts_with(&root) || !canonical.is_file() {
                return Err(BackendError::new(
                    "GIT_DIFF_FAILED",
                    "Candidate diff inputs must be regular files inside the project.",
                    false,
                    true,
                ));
            }
            relative.push(
                canonical
                    .strip_prefix(&root)
                    .expect("candidate containment was checked")
                    .to_path_buf(),
            );
        }
        let lane = git_project_lane(&root)?;
        let started = Instant::now();
        let _lane = lock_git_lane(&lane, DEFAULT_GIT_TIMEOUT, git_task_cancelled)
            .map_err(|error| git_process_error(error, &["diff", "--no-index"]))?;
        let mut command = hardened_git_command(context);
        command
            .args(["diff", "--no-index", "--no-ext-diff", "--no-textconv", "--"])
            .arg(&relative[0])
            .arg(&relative[1]);
        let output = run_bounded_process(
            &mut command,
            None,
            DEFAULT_GIT_TIMEOUT.saturating_sub(started.elapsed()),
            MAX_GIT_OUTPUT_BYTES,
            git_task_cancelled,
        )
        .map_err(|error| git_process_error(error, &["diff", "--no-index"]))?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(BackendError::new(
                "GIT_DIFF_FAILED",
                String::from_utf8_lossy(&output.stderr).into_owned(),
                true,
                false,
            ));
        }
        let body = String::from_utf8_lossy(&output.stdout);
        Ok(format!("```diff\n{}\n```", body.trim_end()))
    }

    pub fn initialize_repository(
        &self,
        context: &ProjectContext,
        initial_message: &str,
    ) -> Result<GitRepositoryStatus, BackendError> {
        if !self.repository_status(context)?.is_repository {
            run_git(context, &["init"])?;
        }
        let has_head = run_git(context, &["rev-parse", "--verify", "HEAD"]).is_ok();
        if !has_head {
            run_git(context, &["add", "--all"])?;
            let _ = commit_with_message(context, initial_message, true)?;
        }
        self.repository_status(context)
    }

    pub fn repository_status(
        &self,
        context: &ProjectContext,
    ) -> Result<GitRepositoryStatus, BackendError> {
        let root = validate_existing_project_root(&context.root).map_err(git_path_unsafe)?;
        let has_git_marker = validate_git_marker(&root)?;
        // A project-local repository must own a real `.git` directory or
        // worktree marker. Avoid spawning Git for ordinary projects: a
        // missing executable (or transient process pressure) must still map
        // to the stable Unavailable state when no repository exists.
        if !has_git_marker {
            return Ok(GitRepositoryStatus {
                is_repository: false,
                branch: None,
                head: None,
                has_changes: false,
            });
        }
        if run_git(context, &["--version"]).is_err() {
            return Err(BackendError::new(
                "GIT_COMMAND_FAILED",
                "Git is unavailable.",
                true,
                false,
            ));
        }
        let is_repository = match run_git(context, &["rev-parse", "--show-toplevel"]) {
            Ok(value) => Path::new(value.trim())
                .canonicalize()
                .is_ok_and(|top_level| top_level == root),
            Err(_) if has_git_marker => {
                return Err(BackendError::new(
                    "GIT_REPOSITORY_INVALID",
                    "Project-local Git metadata is incomplete or unreadable.",
                    true,
                    true,
                ));
            }
            Err(_) => false,
        };
        if !is_repository {
            return Ok(GitRepositoryStatus {
                is_repository: false,
                branch: None,
                head: None,
                has_changes: false,
            });
        }

        let branch = run_git(context, &["branch", "--show-current"])
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let head = run_git(context, &["rev-parse", "--short", "HEAD"])
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let has_changes = !status_paths(context)?.is_empty();

        Ok(GitRepositoryStatus {
            is_repository,
            branch,
            head,
            has_changes,
        })
    }

    pub fn create_checkpoint(
        &self,
        context: &ProjectContext,
        purpose: CheckpointPurpose,
        message: &str,
    ) -> Result<GitCheckpoint, BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before creating a checkpoint.",
                true,
                true,
            ));
        }

        let affected_paths = status_paths(context)?;
        if affected_paths.is_empty() {
            return Ok(GitCheckpoint {
                created: false,
                commit_hash: self.repository_status(context)?.head,
                message: message.to_string(),
                purpose,
                affected_paths,
            });
        }

        run_git(context, &["add", "--all"])?;
        let commit_hash = commit_with_message(context, message, false)?;

        Ok(GitCheckpoint {
            created: true,
            commit_hash: Some(commit_hash),
            message: message.to_string(),
            purpose,
            affected_paths,
        })
    }

    /// Capture the current clean HEAD without staging or committing anything.
    ///
    /// Workflow preparation already requires a clean worktree. This variant is
    /// intentionally non-mutating so an external edit racing the check can
    /// never be absorbed into an application-authored checkpoint commit.
    pub fn clean_head_checkpoint(
        &self,
        context: &ProjectContext,
        purpose: CheckpointPurpose,
        message: &str,
    ) -> Result<GitCheckpoint, BackendError> {
        let status = self.repository_status(context)?;
        if !status.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before creating a checkpoint.",
                true,
                true,
            ));
        }
        let affected_paths = status_paths(context)?;
        if !affected_paths.is_empty() {
            return Err(BackendError::new(
                "GIT_WORKTREE_DIRTY",
                "The project changed after preparation. Resolve or checkpoint those edits, then run again.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "affectedPaths": affected_paths })));
        }
        let commit_hash = status.head.ok_or_else(|| {
            BackendError::new(
                "GIT_HEAD_MISSING",
                "Git HEAD is unavailable for the required checkpoint.",
                true,
                true,
            )
        })?;
        Ok(GitCheckpoint {
            created: false,
            commit_hash: Some(commit_hash),
            message: message.to_string(),
            purpose,
            affected_paths: Vec::new(),
        })
    }

    /// Capture the current HEAD while tolerating an exact, caller-owned set of
    /// workflow state paths. The allowed paths are never staged or committed;
    /// any other dirty path still fails closed before an external Agent starts.
    pub fn clean_head_checkpoint_allowing_paths(
        &self,
        context: &ProjectContext,
        purpose: CheckpointPurpose,
        message: &str,
        allowed_dirty_paths: &[String],
    ) -> Result<GitCheckpoint, BackendError> {
        for path in allowed_dirty_paths {
            validate_relative_git_path(path)?;
        }
        let status = self.repository_status(context)?;
        if !status.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before creating a checkpoint.",
                true,
                true,
            ));
        }
        let affected_paths = status_paths(context)?;
        let unexpected = affected_paths
            .iter()
            .filter(|path| !allowed_dirty_paths.iter().any(|allowed| allowed == *path))
            .cloned()
            .collect::<Vec<_>>();
        if !unexpected.is_empty() {
            return Err(BackendError::new(
                "GIT_WORKTREE_DIRTY",
                "The project changed after preparation. Resolve or checkpoint those edits, then run again.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "affectedPaths": unexpected })));
        }
        let commit_hash = status.head.ok_or_else(|| {
            BackendError::new(
                "GIT_HEAD_MISSING",
                "Git HEAD is unavailable for the required checkpoint.",
                true,
                true,
            )
        })?;
        Ok(GitCheckpoint {
            created: false,
            commit_hash: Some(commit_hash),
            message: message.to_string(),
            purpose,
            affected_paths: Vec::new(),
        })
    }

    pub fn unstage_paths(
        &self,
        context: &ProjectContext,
        paths: &[String],
    ) -> Result<(), BackendError> {
        if paths.is_empty() {
            return Ok(());
        }
        let mut args = vec!["reset", "-q", "HEAD", "--"];
        args.extend(paths.iter().map(String::as_str));
        run_git(context, &args).map(|_| ())
    }

    pub fn create_scoped_checkpoint(
        &self,
        context: &ProjectContext,
        purpose: CheckpointPurpose,
        message: &str,
        paths: &[String],
    ) -> Result<GitCheckpoint, BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before creating a checkpoint.",
                true,
                true,
            ));
        }
        let affected_paths: Vec<String> = status_paths(context)?
            .into_iter()
            .filter(|changed| paths.iter().any(|path| path == changed))
            .collect();
        if affected_paths.is_empty() {
            return Ok(GitCheckpoint {
                created: false,
                commit_hash: self.repository_status(context)?.head,
                message: message.to_string(),
                purpose,
                affected_paths,
            });
        }
        let mut args = vec!["add", "--"];
        args.extend(affected_paths.iter().map(String::as_str));
        run_git(context, &args)?;
        let commit_hash = commit_paths_with_message(context, message, &affected_paths)?;
        Ok(GitCheckpoint {
            created: true,
            commit_hash: Some(commit_hash),
            message: message.to_string(),
            purpose,
            affected_paths,
        })
    }

    pub fn diff_markdown(&self, context: &ProjectContext) -> Result<GitDiff, BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before generating a diff.",
                true,
                true,
            ));
        }

        let affected_paths = status_paths(context)?;
        let tracked_diff =
            run_git(context, &["diff", "--no-ext-diff", "--no-textconv", "--"]).unwrap_or_default();
        let untracked: Vec<String> = affected_paths
            .iter()
            .filter(|path| run_git(context, &["ls-files", "--error-unmatch", path]).is_err())
            .cloned()
            .collect();

        let mut body = String::new();
        if !tracked_diff.trim().is_empty() {
            body.push_str(&tracked_diff);
        }
        if !untracked.is_empty() {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str("Untracked files:\n");
            for path in untracked {
                body.push_str(&format!("+ {path}\n"));
            }
        }
        if body.trim().is_empty() {
            body.push_str("No changes.");
        }

        Ok(GitDiff {
            markdown: format!("```diff\n{}\n```", body.trim_end()),
            affected_paths,
        })
    }

    pub fn changed_files_since_head(
        &self,
        context: &ProjectContext,
    ) -> Result<Vec<GitChangedFile>, BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before enumerating changes.",
                true,
                true,
            ));
        }

        let mut changes = status_changes(context)?;
        append_ignored_changes(context, &mut changes, &[])?;
        for change in &mut changes {
            change.changed_chars = estimate_changed_bytes(context, change);
        }
        changes.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(changes)
    }

    pub fn changed_files_since_head_with_ignored_baseline(
        &self,
        context: &ProjectContext,
        preserved_ignored_paths: &[String],
    ) -> Result<Vec<GitChangedFile>, BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before enumerating changes.",
                true,
                true,
            ));
        }

        let mut changes = status_changes(context)?;
        append_ignored_changes(context, &mut changes, preserved_ignored_paths)?;
        for change in &mut changes {
            change.changed_chars = estimate_changed_bytes(context, change);
        }
        changes.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(changes)
    }

    pub fn ignored_paths(&self, context: &ProjectContext) -> Result<Vec<String>, BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before listing ignored paths.",
                true,
                true,
            ));
        }
        ignored_paths(context)
    }

    pub fn diff_since_head(&self, context: &ProjectContext) -> Result<String, BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before generating a diff.",
                true,
                true,
            ));
        }

        let mut diff = run_git(
            context,
            &["diff", "--no-ext-diff", "--no-textconv", "HEAD", "--"],
        )?;
        let untracked_diff = untracked_file_diff(context)?;
        if !untracked_diff.is_empty() {
            if !diff.is_empty() && !diff.ends_with('\n') {
                diff.push('\n');
            }
            diff.push_str(&untracked_diff);
        }
        Ok(diff)
    }

    pub fn rollback_worktree_to_head(&self, context: &ProjectContext) -> Result<(), BackendError> {
        self.rollback_worktree_to_head_preserving_ignored(context, &[])
    }

    pub fn rollback_worktree_to_head_preserving_ignored(
        &self,
        context: &ProjectContext,
        preserved_ignored_paths: &[String],
    ) -> Result<(), BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before rolling back changes.",
                true,
                true,
            ));
        }

        run_git(
            context,
            &[
                "restore",
                "--source=HEAD",
                "--staged",
                "--worktree",
                "--",
                ".",
            ],
        )?;
        run_git(context, &["clean", "-fd", "--", "."])?;
        remove_new_ignored_paths(context, preserved_ignored_paths)?;
        Ok(())
    }

    /// Restore only the paths known to have been touched by one operation.
    ///
    /// Chat convenience runs can overlap with edits made elsewhere in the
    /// worktree.  A whole-worktree `restore`/`clean` would therefore erase
    /// unrelated user changes.  This scoped variant deliberately accepts an
    /// explicit path list and never traverses or cleans any other path.
    pub fn rollback_paths_to_head_preserving_ignored(
        &self,
        context: &ProjectContext,
        paths: &[String],
        preserved_ignored_paths: &[String],
    ) -> Result<(), BackendError> {
        if !self.repository_status(context)?.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before rolling back changes.",
                true,
                true,
            ));
        }

        for path in paths {
            validate_relative_git_path(path)?;
            if preserved_ignored_paths
                .iter()
                .any(|preserved| preserved == path)
            {
                continue;
            }

            let head_spec = format!("HEAD:{path}");
            let exists_in_head = run_git(context, &["cat-file", "-e", head_spec.as_str()]).is_ok();
            if exists_in_head {
                run_git(
                    context,
                    &[
                        "restore",
                        "--source=HEAD",
                        "--staged",
                        "--worktree",
                        "--",
                        path.as_str(),
                    ],
                )?;
            } else if context.root.join(path).exists() {
                remove_project_path(context, path)?;
            }
        }
        Ok(())
    }

    /// Restore an exact path set from an earlier checkpoint and commit only
    /// those restored paths. The current HEAD binding prevents replay after a
    /// later commit, while the scoped path list preserves unrelated worktree
    /// edits. This is intentionally narrower than a generic Git revert API.
    pub fn rollback_paths_to_checkpoint(
        &self,
        context: &ProjectContext,
        expected_head: &str,
        checkpoint: &str,
        message: &str,
        paths: &[String],
    ) -> Result<GitCheckpoint, BackendError> {
        let status = self.repository_status(context)?;
        if !status.is_repository {
            return Err(BackendError::new(
                "GIT_REPOSITORY_MISSING",
                "Git repository is required before rolling back a repair batch.",
                true,
                true,
            ));
        }
        if status.head.as_deref() != Some(expected_head) {
            return Err(BackendError::new(
                "GIT_ROLLBACK_NOT_CURRENT",
                "The repair can only be rolled back while its final commit is the current Git HEAD.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "expectedHead": expected_head,
                "currentHead": status.head,
            })));
        }
        if paths.is_empty() {
            return Err(BackendError::new(
                "GIT_ROLLBACK_PATHS_MISSING",
                "The repair result has no affected paths to roll back.",
                true,
                true,
            ));
        }
        for path in paths {
            validate_relative_git_path(path)?;
        }
        run_git(
            context,
            &["rev-parse", "--verify", &format!("{checkpoint}^{{commit}}")],
        )?;
        let parent = run_git(
            context,
            &["rev-parse", "--short", &format!("{expected_head}^")],
        )?;
        if parent.trim() != checkpoint {
            return Err(BackendError::new(
                "GIT_ROLLBACK_CHECKPOINT_MISMATCH",
                "The repair checkpoint is not the direct parent of the final repair commit.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "checkpoint": checkpoint,
                "finalCommit": expected_head,
                "parent": parent.trim(),
            })));
        }

        let restore_result = (|| {
            for path in paths {
                let tracked =
                    !run_git(context, &["ls-tree", "--name-only", checkpoint, "--", path])?
                        .trim()
                        .is_empty();
                if tracked {
                    run_git(
                        context,
                        &[
                            "restore",
                            &format!("--source={checkpoint}"),
                            "--staged",
                            "--worktree",
                            "--",
                            path,
                        ],
                    )?;
                } else if context.root.join(path).exists() {
                    remove_project_path(context, path)?;
                }
            }
            self.create_scoped_checkpoint(context, CheckpointPurpose::FinalResult, message, paths)
        })();

        match restore_result {
            Ok(checkpoint) if checkpoint.created => Ok(checkpoint),
            Ok(_) => {
                let _ = self.rollback_paths_to_head_preserving_ignored(context, paths, &[]);
                Err(BackendError::new(
                    "GIT_ROLLBACK_NO_CHANGES",
                    "The approved repair paths no longer differ from the initial checkpoint.",
                    true,
                    true,
                ))
            }
            Err(error) => match self.rollback_paths_to_head_preserving_ignored(context, paths, &[]) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(BackendError::new(
                    "GIT_ROLLBACK_FAILED",
                    format!(
                        "Repair rollback failed and the worktree could not be restored: rollback={}; cleanup={}",
                        error.message, cleanup.message
                    ),
                    true,
                    true,
                )),
            },
        }
    }

    /// Prove that the current HEAD is the exact scoped compensating commit
    /// produced after `final_commit`, and that it restored the repair paths to
    /// the checkpoint tree. This lets recovery consume a still-present WAL if
    /// the process stopped after Git committed but before app settings were
    /// atomically updated.
    pub fn is_exact_compensating_rollback(
        &self,
        context: &ProjectContext,
        final_commit: &str,
        checkpoint: &str,
        allowed_paths: &[String],
    ) -> Result<bool, BackendError> {
        let current = match self.repository_status(context)?.head {
            Some(head) => head,
            None => return Ok(false),
        };
        if current == final_commit || allowed_paths.is_empty() {
            return Ok(false);
        }
        for path in allowed_paths {
            validate_relative_git_path(path)?;
        }
        let final_full = run_git(context, &["rev-parse", final_commit])?;
        let checkpoint_full = run_git(context, &["rev-parse", checkpoint])?;
        let current_parent = run_git(context, &["rev-parse", &format!("{current}^")])?;
        let final_parent = run_git(context, &["rev-parse", &format!("{final_commit}^")])?;
        if current_parent.trim() != final_full.trim()
            || final_parent.trim() != checkpoint_full.trim()
        {
            return Ok(false);
        }
        let repair_paths = commit_changed_paths(context, checkpoint, final_commit)?;
        let compensation_paths = commit_changed_paths(context, final_commit, &current)?;
        let allowed = allowed_paths.iter().cloned().collect::<BTreeSet<_>>();
        if repair_paths.is_empty()
            || repair_paths != compensation_paths
            || !repair_paths.is_subset(&allowed)
        {
            return Ok(false);
        }
        for path in &repair_paths {
            let diff = run_git(
                context,
                &["diff", "--name-only", checkpoint, &current, "--", path],
            )?;
            if !diff.trim().is_empty() {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

const MAX_INITIAL_COMMIT_PREVIEW_ENTRIES: usize = 10_000;
const MAX_INITIAL_COMMIT_PREVIEW_DEPTH: usize = 32;

fn collect_initial_commit_paths(
    root: &Path,
    current: &Path,
    depth: usize,
    entries_seen: &mut usize,
    paths: &mut Vec<String>,
) -> Result<(), BackendError> {
    if depth > MAX_INITIAL_COMMIT_PREVIEW_DEPTH {
        return Err(initial_commit_preview_too_large());
    }
    for entry in fs::read_dir(current).map_err(|error| {
        BackendError::new(
            "GIT_INITIALIZATION_PREVIEW_FAILED",
            error.to_string(),
            true,
            true,
        )
    })? {
        let entry = entry.map_err(|error| {
            BackendError::new(
                "GIT_INITIALIZATION_PREVIEW_FAILED",
                error.to_string(),
                true,
                true,
            )
        })?;
        *entries_seen += 1;
        if *entries_seen > MAX_INITIAL_COMMIT_PREVIEW_ENTRIES {
            return Err(initial_commit_preview_too_large());
        }
        let path = entry.path();
        if depth == 0 && entry.file_name() == ".git" {
            continue;
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            BackendError::new(
                "GIT_INITIALIZATION_PREVIEW_FAILED",
                error.to_string(),
                true,
                true,
            )
        })?;
        if metadata.is_dir() {
            if validate_existing_project_directory(root, &path).is_ok() {
                collect_initial_commit_paths(root, &path, depth + 1, entries_seen, paths)?;
            }
        } else if metadata.is_file() && validate_existing_project_file(root, &path).is_ok() {
            let relative = path.strip_prefix(root).map_err(|_| {
                git_path_unsafe("Initial commit candidate escaped the project root".into())
            })?;
            paths.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn initial_commit_preview_too_large() -> BackendError {
    BackendError::new(
        "GIT_INITIALIZATION_PREVIEW_TOO_LARGE",
        "The initial Git commit preview exceeded the safe file or depth limit.",
        true,
        true,
    )
}

fn validate_git_marker(root: &Path) -> Result<bool, BackendError> {
    let marker = root.join(".git");
    let Ok(metadata) = fs::symlink_metadata(&marker) else {
        return Ok(false);
    };
    let validation = if metadata.is_dir() {
        validate_existing_project_directory(root, &marker).map(|_| ())
    } else if metadata.is_file() {
        validate_existing_project_file(root, &marker).map(|_| ())
    } else {
        Err("Git metadata has an unsupported file type".into())
    };
    validation.map_err(git_path_unsafe)?;
    Ok(true)
}

fn git_path_unsafe(message: String) -> BackendError {
    BackendError::new(
        "GIT_REPOSITORY_PATH_UNSAFE",
        "Git metadata is linked, outside the project, or otherwise unsafe.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "error": message }))
}

const MAX_ASSESSMENT_GIT_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_GIT_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const GIT_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "SystemRoot",
    "WINDIR",
    "COMSPEC",
    "HOME",
    "USERPROFILE",
    "TEMP",
    "TMP",
    "TMPDIR",
    "LANG",
    "LC_ALL",
];

static GIT_PROJECT_LANES: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

struct BoundedGitOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn run_git_bounded(
    context: &ProjectContext,
    args: &[&str],
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<BoundedGitOutput, BackendError> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(BackendError::new(
            "PROJECT_ASSESSMENT_CANCELLED",
            "Project assessment was cancelled.",
            true,
            true,
        ));
    }
    if Instant::now() >= deadline {
        return Err(git_assessment_timeout());
    }

    let remaining = deadline.saturating_duration_since(Instant::now());
    let output = run_git_process(
        context,
        args,
        remaining,
        MAX_ASSESSMENT_GIT_OUTPUT_BYTES,
        || cancelled.load(Ordering::SeqCst),
    )
    .map_err(|error| match error {
        BoundedProcessError::Cancelled => BackendError::new(
            "PROJECT_ASSESSMENT_CANCELLED",
            "Project assessment was cancelled.",
            true,
            true,
        ),
        BoundedProcessError::Timeout => git_assessment_timeout(),
        other => git_process_error(other, args),
    })?;
    Ok(BoundedGitOutput {
        success: output.status.success(),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

#[cfg(windows)]
fn assessment_disabled_hooks_config() -> &'static str {
    "core.hooksPath=NUL"
}

#[cfg(not(windows))]
fn assessment_disabled_hooks_config() -> &'static str {
    "core.hooksPath=/dev/null"
}

fn bounded_optional_git_value(
    context: &ProjectContext,
    args: &[&str],
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Option<String>, BackendError> {
    let output = run_git_bounded(context, args, deadline, cancelled)?;
    if !output.success {
        return Ok(None);
    }
    Ok(
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|value| !value.is_empty()),
    )
}

fn git_assessment_timeout() -> BackendError {
    BackendError::new(
        "GIT_ASSESSMENT_TIMEOUT",
        "Git inspection exceeded the project assessment time limit.",
        true,
        true,
    )
}

fn git_command_error(stderr: &[u8], args: &[&str]) -> BackendError {
    let message = String::from_utf8_lossy(stderr).trim().to_string();
    BackendError::new(
        "GIT_COMMAND_FAILED",
        if message.is_empty() {
            "Git command failed.".to_string()
        } else {
            message
        },
        true,
        false,
    )
    .with_details(serde_json::json!({ "args": args }))
}

fn run_git(context: &ProjectContext, args: &[&str]) -> Result<String, BackendError> {
    run_git_bytes(context, args).map(|stdout| String::from_utf8_lossy(&stdout).to_string())
}

fn run_git_bytes(context: &ProjectContext, args: &[&str]) -> Result<Vec<u8>, BackendError> {
    let output = run_git_process(
        context,
        args,
        DEFAULT_GIT_TIMEOUT,
        MAX_GIT_OUTPUT_BYTES,
        || false,
    )
    .map_err(|error| git_process_error(error, args))?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(BackendError::new(
            "GIT_COMMAND_FAILED",
            if stderr.is_empty() {
                "Git command failed.".to_string()
            } else {
                stderr
            },
            true,
            false,
        )
        .with_details(serde_json::json!({ "args": args })))
    }
}

fn run_git_process(
    context: &ProjectContext,
    args: &[&str],
    timeout: Duration,
    max_stream_bytes: usize,
    cancelled: impl Fn() -> bool,
) -> Result<CapturedProcessOutput, BoundedProcessError> {
    #[cfg(test)]
    TEST_GIT_PROCESS_ATTEMPTS.with(|count| count.set(count.get() + 1));
    let effective_cancelled = || cancelled() || git_task_cancelled();
    if effective_cancelled() {
        return Err(BoundedProcessError::Cancelled);
    }
    let lane = git_project_lane(&context.root)
        .map_err(|error| BoundedProcessError::Wait(std::io::Error::other(error.message)))?;
    let started = Instant::now();
    let _lane = lock_git_lane(&lane, timeout, &effective_cancelled)?;
    reject_local_git_filters(
        context,
        timeout.saturating_sub(started.elapsed()),
        &effective_cancelled,
    )?;
    let mut command = hardened_git_command(context);
    command.args(args);
    run_bounded_process(
        &mut command,
        None,
        timeout.saturating_sub(started.elapsed()),
        max_stream_bytes,
        effective_cancelled,
    )
}

fn reject_local_git_filters(
    context: &ProjectContext,
    timeout: Duration,
    cancelled: &impl Fn() -> bool,
) -> Result<(), BoundedProcessError> {
    if !context.root.join(".git").exists() {
        return Ok(());
    }
    let mut command = hardened_git_command(context);
    // Inspect every scope still visible to the hardened command. In
    // particular, `--local` omits `.git/config.worktree` when a repository
    // enables `extensions.worktreeConfig`, even though later Git commands
    // load filters from that worktree scope.
    command.args(["config", "--no-includes", "--name-only", "--list"]);
    let output = run_bounded_process(&mut command, None, timeout, 64 * 1024, cancelled)?;
    let has_unsafe_execution_config = output.stdout.split(|byte| *byte == b'\n').any(|line| {
        let key = String::from_utf8_lossy(line).trim().to_ascii_lowercase();
        key.starts_with("filter.") || key.starts_with("include.") || key.starts_with("includeif.")
    });
    if output.status.success() && has_unsafe_execution_config {
        return Err(BoundedProcessError::Wait(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "repository-defined Git filters or config includes are not allowed",
        )));
    }
    if output.status.success() {
        Ok(())
    } else {
        Err(BoundedProcessError::Wait(std::io::Error::other(
            "repository Git filter policy could not be verified",
        )))
    }
}

fn lock_git_lane<'lane>(
    lane: &'lane Mutex<()>,
    timeout: Duration,
    cancelled: impl Fn() -> bool,
) -> Result<std::sync::MutexGuard<'lane, ()>, BoundedProcessError> {
    let deadline = Instant::now() + timeout;
    loop {
        match lane.try_lock() {
            Ok(guard) => return Ok(guard),
            Err(std::sync::TryLockError::Poisoned(_)) => {
                return Err(BoundedProcessError::Wait(std::io::Error::other(
                    "Git project lane is unavailable",
                )))
            }
            Err(std::sync::TryLockError::WouldBlock) if cancelled() => {
                return Err(BoundedProcessError::Cancelled)
            }
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() >= deadline => {
                return Err(BoundedProcessError::Timeout)
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

fn hardened_git_command(context: &ProjectContext) -> Command {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .args([
            "-c",
            "core.quotepath=false",
            "-c",
            "core.fsmonitor=false",
            "-c",
            assessment_disabled_hooks_config(),
            "-c",
            disabled_attributes_config(),
            "-c",
            "core.pager=cat",
            "-c",
            "credential.helper=",
            "-c",
            "credential.interactive=never",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "diff.external=",
        ])
        .current_dir(&context.root)
        .env_clear();
    for name in GIT_ENV_ALLOWLIST {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", git_null_device())
        .env("GIT_CONFIG_SYSTEM", git_null_device())
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .env("NO_COLOR", "1");
    command
}

#[cfg(windows)]
fn disabled_attributes_config() -> &'static str {
    "core.attributesFile=NUL"
}

#[cfg(not(windows))]
fn disabled_attributes_config() -> &'static str {
    "core.attributesFile=/dev/null"
}

#[cfg(windows)]
fn git_null_device() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
fn git_null_device() -> &'static str {
    "/dev/null"
}

fn git_project_lane(root: &Path) -> Result<Arc<Mutex<()>>, BackendError> {
    let key = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let lanes = GIT_PROJECT_LANES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut lanes = lanes.lock().map_err(|_| git_lane_error())?;
    lanes.retain(|_, lane| lane.strong_count() > 0);
    if let Some(lane) = lanes.get(&key).and_then(Weak::upgrade) {
        return Ok(lane);
    }
    let lane = Arc::new(Mutex::new(()));
    lanes.insert(key, Arc::downgrade(&lane));
    Ok(lane)
}

fn git_lane_error() -> BackendError {
    BackendError::new(
        "GIT_PROCESS_LANE_UNAVAILABLE",
        "The project Git process lane is unavailable.",
        true,
        true,
    )
}

fn git_process_error(error: BoundedProcessError, args: &[&str]) -> BackendError {
    let (code, message, action) = match error {
        BoundedProcessError::Timeout => (
            "GIT_COMMAND_TIMEOUT",
            "Git command exceeded the execution deadline.".to_string(),
            true,
        ),
        BoundedProcessError::Cancelled => (
            "GIT_COMMAND_CANCELLED",
            "Git command was cancelled.".to_string(),
            false,
        ),
        BoundedProcessError::OutputTooLarge => (
            "GIT_COMMAND_OUTPUT_TOO_LARGE",
            "Git command output exceeded the raw byte limit.".to_string(),
            true,
        ),
        BoundedProcessError::Isolation(error) => {
            ("GIT_PROCESS_ISOLATION_FAILED", error.to_string(), true)
        }
        BoundedProcessError::Spawn(error)
        | BoundedProcessError::Stdin(error)
        | BoundedProcessError::Read(error)
        | BoundedProcessError::Wait(error) => ("GIT_COMMAND_FAILED", error.to_string(), false),
    };
    BackendError::new(code, message, true, action).with_details(serde_json::json!({ "args": args }))
}

fn commit_with_message(
    context: &ProjectContext,
    message: &str,
    allow_empty: bool,
) -> Result<String, BackendError> {
    let mut args = vec![
        "-c",
        "user.name=LLM Wiki Desktop",
        "-c",
        "user.email=llm-wiki-desktop@example.local",
        "commit",
    ];
    if allow_empty {
        args.push("--allow-empty");
    }
    args.push("-m");
    args.push(message);
    run_git(context, &args)?;
    run_git(context, &["rev-parse", "--short", "HEAD"]).map(|value| value.trim().to_string())
}

fn commit_changed_paths(
    context: &ProjectContext,
    from: &str,
    to: &str,
) -> Result<BTreeSet<String>, BackendError> {
    Ok(run_git(context, &["diff", "--name-only", from, to, "--"])?
        .lines()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect())
}

fn commit_paths_with_message(
    context: &ProjectContext,
    message: &str,
    paths: &[String],
) -> Result<String, BackendError> {
    let mut args = vec![
        "-c",
        "user.name=LLM Wiki Desktop",
        "-c",
        "user.email=llm-wiki-desktop@example.local",
        "commit",
        "-m",
        message,
        "--",
    ];
    args.extend(paths.iter().map(String::as_str));
    run_git(context, &args)?;
    run_git(context, &["rev-parse", "--short", "HEAD"]).map(|value| value.trim().to_string())
}

fn status_paths(context: &ProjectContext) -> Result<Vec<String>, BackendError> {
    let mut paths: Vec<String> = status_changes(context)?
        .into_iter()
        .map(|change| change.path)
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn status_changes(context: &ProjectContext) -> Result<Vec<GitChangedFile>, BackendError> {
    let raw = run_git_bytes(context, &["status", "--porcelain=v1", "-z", "-uall"])?;
    Ok(parse_status_changes(&raw))
}

fn initial_commit_git_candidates(context: &ProjectContext) -> Result<Vec<String>, BackendError> {
    let raw = run_git_bytes(
        context,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )?;
    let mut paths = raw
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| normalize_git_path(&String::from_utf8_lossy(record)))
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn append_ignored_changes(
    context: &ProjectContext,
    changes: &mut Vec<GitChangedFile>,
    preserved_ignored_paths: &[String],
) -> Result<(), BackendError> {
    for path in ignored_paths(context)? {
        if preserved_ignored_paths
            .iter()
            .any(|preserved| preserved == &path)
            || changes.iter().any(|change| change.path == path)
        {
            continue;
        }
        changes.push(GitChangedFile {
            path,
            kind: GitChangedFileKind::Added,
            changed_chars: 0,
        });
    }
    Ok(())
}

fn parse_status_changes(raw: &[u8]) -> Vec<GitChangedFile> {
    let records: Vec<&[u8]> = raw
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect();
    let mut changes = Vec::new();
    let mut index = 0;
    while index < records.len() {
        let record = records[index];
        if record.len() < 4 {
            index += 1;
            continue;
        }

        let index_status = record[0] as char;
        let worktree_status = record[1] as char;
        let path = normalize_git_path(&String::from_utf8_lossy(&record[3..]));
        if path.is_empty() {
            index += 1;
            continue;
        }

        let kind = if index_status == 'R' || worktree_status == 'R' {
            index += 1;
            GitChangedFileKind::Renamed
        } else if index_status == 'D' || worktree_status == 'D' {
            GitChangedFileKind::Deleted
        } else if index_status == 'A'
            || worktree_status == 'A'
            || index_status == '?'
            || worktree_status == '?'
        {
            GitChangedFileKind::Added
        } else {
            GitChangedFileKind::Modified
        };

        changes.push(GitChangedFile {
            path,
            kind,
            changed_chars: 0,
        });
        index += 1;
    }
    changes.sort_by(|left, right| left.path.cmp(&right.path));
    changes.dedup_by(|left, right| left.path == right.path);
    changes
}

fn untracked_paths(context: &ProjectContext) -> Result<Vec<String>, BackendError> {
    let raw = run_git_bytes(
        context,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    Ok(raw
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| normalize_git_path(&String::from_utf8_lossy(record)))
        .filter(|path| !path.is_empty())
        .collect())
}

fn ignored_paths(context: &ProjectContext) -> Result<Vec<String>, BackendError> {
    let raw = run_git_bytes(
        context,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
    )?;
    Ok(raw
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| normalize_git_path(&String::from_utf8_lossy(record)))
        .filter(|path| !path.is_empty())
        .collect())
}

fn remove_new_ignored_paths(
    context: &ProjectContext,
    preserved_ignored_paths: &[String],
) -> Result<(), BackendError> {
    for path in ignored_paths(context)? {
        if preserved_ignored_paths
            .iter()
            .any(|preserved| preserved == &path)
        {
            continue;
        }
        remove_project_path(context, &path)?;
    }
    Ok(())
}

fn remove_project_path(context: &ProjectContext, path: &str) -> Result<(), BackendError> {
    validate_relative_git_path(path)?;
    let target = context.root.join(path);
    let binding = BoundProjectMutationRoot::bind(&context.root, &target)
        .map_err(|err| BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false))?;
    let metadata = fs::symlink_metadata(&target)
        .map_err(|err| BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false))?;
    if metadata.is_dir() {
        binding.remove_directory_tree(&target)
    } else {
        binding.remove_file(&target)
    }
    .map_err(|err| BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false))
}

fn validate_relative_git_path(path: &str) -> Result<(), BackendError> {
    let candidate = std::path::Path::new(path);
    if path.trim().is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(BackendError::new(
            "GIT_ROLLBACK_FAILED",
            format!("Refusing to roll back unsafe path: {path}"),
            true,
            false,
        ));
    }
    Ok(())
}

fn untracked_file_diff(context: &ProjectContext) -> Result<String, BackendError> {
    let mut diff = String::new();
    for path in untracked_paths(context)? {
        let bytes = read_worktree_regular(context, &path)?;
        if !diff.is_empty() && !diff.ends_with('\n') {
            diff.push('\n');
        }
        diff.push_str(&render_added_file_diff(&path, &bytes));
    }
    Ok(diff)
}

fn render_added_file_diff(path: &str, bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    let line_count = text.lines().count().max(1);
    let mut diff = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{line_count} @@\n"
    );
    if text.is_empty() {
        diff.push('+');
        diff.push('\n');
        return diff;
    }
    for line in text.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}

fn estimate_changed_bytes(context: &ProjectContext, change: &GitChangedFile) -> usize {
    let head_len = head_file_bytes(context, &change.path).map_or(0, |bytes| bytes.len());
    let worktree_len = read_worktree_regular(context, &change.path).map_or(0, |bytes| bytes.len());
    match change.kind {
        GitChangedFileKind::Added => worktree_len.max(head_len),
        GitChangedFileKind::Deleted => head_len,
        GitChangedFileKind::Modified | GitChangedFileKind::Renamed => {
            head_len.saturating_add(worktree_len)
        }
    }
}

fn read_worktree_regular(context: &ProjectContext, path: &str) -> Result<Vec<u8>, BackendError> {
    validate_relative_git_path(path)?;
    let target = context.root.join(path);
    BoundProjectMutationRoot::bind_read(&context.root, &target)
        .and_then(|binding| binding.read_regular(&target))
        .map_err(|error| {
            BackendError::new(
                "GIT_DIFF_FAILED",
                format!("Cannot read worktree file through its project binding: {error}"),
                true,
                false,
            )
        })
}

fn head_file_bytes(context: &ProjectContext, path: &str) -> Result<Vec<u8>, BackendError> {
    let spec = format!("HEAD:{path}");
    run_git_bytes(context, &["show", spec.as_str()])
}

fn normalize_git_path(path: &str) -> String {
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        git_project_lane, remove_project_path, run_git, untracked_file_diff, GitService,
        TEST_GIT_PROCESS_ATTEMPTS,
    };
    use crate::models::git::{CheckpointPurpose, GitChangedFileKind};
    use crate::models::paths::ProjectContext;
    use crate::tasks::task_model::CancellationToken;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command as ProcessCommand;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    fn unique_temp_dir(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("llm-wiki-git-{label}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run_git_in(root: &Path, args: &[&str]) {
        let output = ProcessCommand::new("git")
            .args(["-c", "core.quotepath=false"])
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn write_marker_script(path: &Path, marker: &Path) {
        let marker = marker.to_string_lossy().replace('\\', "/");
        fs::write(
            path,
            format!(
                "#!/bin/sh\nprintf 'invoked\\n' >> '{marker}'\nif [ -f \"$1\" ]; then cat \"$1\"; fi\n"
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn reads_utf8_file_at_checkpoint_without_using_worktree_content() {
        let root = unique_temp_dir("checkpoint-file");
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/冲突.md"), "baseline 内容\n").unwrap();
        run_git_in(&root, &["init"]);
        run_git_in(&root, &["config", "user.email", "tests@example.com"]);
        run_git_in(&root, &["config", "user.name", "Tests"]);
        run_git_in(&root, &["add", "--", "wiki/冲突.md"]);
        run_git_in(&root, &["commit", "-m", "baseline"]);
        let context = ProjectContext::new("project-1", root.clone());
        let checkpoint = run_git(&context, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        fs::write(root.join("wiki/冲突.md"), "current 用户编辑\n").unwrap();

        assert_eq!(
            GitService::file_at_checkpoint(&context, &checkpoint, "wiki/冲突.md").unwrap(),
            Some("baseline 内容\n".into())
        );
        assert_eq!(
            GitService::file_at_checkpoint(&context, &checkpoint, "wiki/missing.md").unwrap(),
            None
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn assessment_git_probe_honors_cancellation_and_deadline_before_spawning() {
        let root = unique_temp_dir("assessment-bounds");
        let context = ProjectContext::new("project-1", root.clone());
        let cancelled = AtomicBool::new(true);

        let cancelled_error = GitService
            .repository_status_for_assessment(
                &context,
                Instant::now() + Duration::from_secs(1),
                &cancelled,
            )
            .unwrap_err();
        assert_eq!(cancelled_error.code, "PROJECT_ASSESSMENT_CANCELLED");

        let active = AtomicBool::new(false);
        let timeout_error = GitService
            .repository_status_for_assessment(
                &context,
                Instant::now() - Duration::from_millis(1),
                &active,
            )
            .unwrap_err();
        assert_eq!(timeout_error.code, "GIT_ASSESSMENT_TIMEOUT");
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn markerless_repository_status_does_not_spawn_git() {
        let root = unique_temp_dir("markerless-status");
        let context = ProjectContext::new("project-1", root.clone());
        let before = TEST_GIT_PROCESS_ATTEMPTS.with(|count| count.get());

        let status = GitService.repository_status(&context).unwrap();

        assert!(!status.is_repository);
        assert_eq!(status.branch, None);
        assert_eq!(status.head, None);
        assert!(!status.has_changes);
        assert_eq!(
            TEST_GIT_PROCESS_ATTEMPTS.with(|count| count.get()),
            before,
            "markerless repository status must not depend on an external Git process"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn invalid_git_marker_is_not_downgraded_to_unavailable() {
        let root = unique_temp_dir("invalid-marker-status");
        fs::create_dir(root.join(".git")).unwrap();
        let context = ProjectContext::new("project-1", root.clone());
        let before = TEST_GIT_PROCESS_ATTEMPTS.with(|count| count.get());

        let error = GitService.repository_status(&context).unwrap_err();

        assert_eq!(error.code, "GIT_REPOSITORY_INVALID");
        assert!(
            TEST_GIT_PROCESS_ATTEMPTS.with(|count| count.get()) > before,
            "a project-local Git marker must keep repository validation fail-closed"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn app_owned_git_disables_hooks_fsmonitor_and_textconv() {
        let root = unique_temp_dir("hardened-config");
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/page.md"), "baseline\n").unwrap();
        fs::write(root.join(".gitattributes"), "*.md diff=batch2c\n").unwrap();
        run_git_in(&root, &["init"]);
        run_git_in(&root, &["config", "user.email", "tests@example.com"]);
        run_git_in(&root, &["config", "user.name", "Tests"]);
        run_git_in(&root, &["add", "--all"]);
        run_git_in(&root, &["commit", "-m", "baseline"]);

        let marker = root.join("unsafe-process-marker.txt");
        let hooks = root.join("unsafe-hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("post-commit");
        let helper = root.join("unsafe-helper.sh");
        write_marker_script(&hook, &marker);
        write_marker_script(&helper, &marker);
        let hooks_config = hooks.to_string_lossy().replace('\\', "/");
        let helper_config = helper.to_string_lossy().replace('\\', "/");
        run_git_in(&root, &["config", "core.hooksPath", &hooks_config]);
        run_git_in(&root, &["config", "core.fsmonitor", &helper_config]);
        run_git_in(&root, &["config", "diff.external", &helper_config]);
        run_git_in(&root, &["config", "diff.batch2c.textconv", &helper_config]);

        let context = ProjectContext::new("project-1", root.clone());
        fs::write(root.join("wiki/page.md"), "checkpoint\n").unwrap();
        GitService
            .create_checkpoint(
                &context,
                CheckpointPurpose::HighRiskOperation,
                "hardened checkpoint",
            )
            .unwrap();
        assert!(
            !marker.exists(),
            "app-owned status/commit executed a repository fsmonitor or hook"
        );

        fs::write(root.join("wiki/page.md"), "diff\n").unwrap();
        let diff = GitService.diff_since_head(&context).unwrap();
        assert!(diff.contains("checkpoint"));
        assert!(diff.contains("diff"));
        assert!(
            !marker.exists(),
            "app-owned diff executed a repository textconv helper"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn app_owned_git_rejects_repository_clean_and_process_filters() {
        let root = unique_temp_dir("hardened-filter");
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/page.md"), "baseline\n").unwrap();
        run_git_in(&root, &["init"]);
        run_git_in(&root, &["config", "user.email", "tests@example.com"]);
        run_git_in(&root, &["config", "user.name", "Tests"]);
        run_git_in(&root, &["add", "--all"]);
        run_git_in(&root, &["commit", "-m", "baseline"]);

        let marker = root.join("unsafe-filter-marker.txt");
        let helper = root.join("unsafe-filter.sh");
        write_marker_script(&helper, &marker);
        let helper_config = helper.to_string_lossy().replace('\\', "/");
        fs::write(root.join(".gitattributes"), "*.md filter=batch2c\n").unwrap();
        run_git_in(&root, &["config", "filter.batch2c.clean", &helper_config]);
        run_git_in(&root, &["config", "filter.batch2c.process", &helper_config]);
        fs::write(root.join("wiki/page.md"), "candidate\n").unwrap();

        let context = ProjectContext::new("project-1", root.clone());
        let error = GitService
            .create_checkpoint(
                &context,
                CheckpointPurpose::HighRiskOperation,
                "must reject filters",
            )
            .expect_err("repository-defined filters must fail closed");
        assert_eq!(error.code, "GIT_COMMAND_FAILED");
        assert!(
            !marker.exists(),
            "Git filter executed before policy rejection"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn app_owned_git_rejects_filters_loaded_from_local_includes() {
        for conditional in [false, true] {
            let root = unique_temp_dir(if conditional {
                "hardened-filter-include-if"
            } else {
                "hardened-filter-include"
            });
            fs::create_dir_all(root.join("wiki")).unwrap();
            fs::write(root.join("wiki/page.md"), "baseline\n").unwrap();
            run_git_in(&root, &["init"]);
            run_git_in(&root, &["config", "user.email", "tests@example.com"]);
            run_git_in(&root, &["config", "user.name", "Tests"]);
            run_git_in(&root, &["add", "--all"]);
            run_git_in(&root, &["commit", "-m", "baseline"]);

            let marker = root.join("unsafe-included-filter-marker.txt");
            let helper = root.join("unsafe-included-filter.sh");
            write_marker_script(&helper, &marker);
            let helper_config = helper.to_string_lossy().replace('\\', "/");
            let included = root.join("included-filter.config");
            fs::write(
                &included,
                format!("[filter \"batch2c\"]\n\tclean = {helper_config}\n"),
            )
            .unwrap();
            let included_config = included.to_string_lossy().replace('\\', "/");
            if conditional {
                let git_dir = root.to_string_lossy().replace('\\', "/");
                run_git_in(
                    &root,
                    &[
                        "config",
                        &format!("includeIf.gitdir:{git_dir}/.path"),
                        &included_config,
                    ],
                );
            } else {
                run_git_in(&root, &["config", "include.path", &included_config]);
            }
            fs::write(root.join(".gitattributes"), "*.md filter=batch2c\n").unwrap();
            fs::write(root.join("wiki/page.md"), "candidate\n").unwrap();

            let context = ProjectContext::new("project-1", root.clone());
            let error = GitService
                .create_checkpoint(
                    &context,
                    CheckpointPurpose::HighRiskOperation,
                    "must reject included filters",
                )
                .expect_err("included repository filters must fail closed");
            assert_eq!(error.code, "GIT_COMMAND_FAILED");
            assert!(
                !marker.exists(),
                "included Git filter executed before policy rejection"
            );
            fs::remove_dir_all(root).ok();
        }
    }

    #[test]
    fn app_owned_git_rejects_filters_from_worktree_config() {
        let root = unique_temp_dir("hardened-worktree-filter");
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/page.md"), "baseline\n").unwrap();
        run_git_in(&root, &["init"]);
        run_git_in(&root, &["config", "user.email", "tests@example.com"]);
        run_git_in(&root, &["config", "user.name", "Tests"]);
        run_git_in(&root, &["add", "--all"]);
        run_git_in(&root, &["commit", "-m", "baseline"]);

        let marker = root.join("unsafe-worktree-filter-marker.txt");
        let helper = root.join("unsafe-worktree-filter.sh");
        write_marker_script(&helper, &marker);
        let helper_config = helper.to_string_lossy().replace('\\', "/");
        run_git_in(&root, &["config", "extensions.worktreeConfig", "true"]);
        run_git_in(
            &root,
            &[
                "config",
                "--worktree",
                "filter.batch2c.clean",
                &helper_config,
            ],
        );
        fs::write(root.join(".gitattributes"), "*.md filter=batch2c\n").unwrap();
        fs::write(root.join("wiki/page.md"), "candidate\n").unwrap();

        let context = ProjectContext::new("project-1", root.clone());
        let error = GitService
            .create_checkpoint(
                &context,
                CheckpointPurpose::HighRiskOperation,
                "must reject worktree filters",
            )
            .expect_err("worktree-scoped repository filters must fail closed");
        assert_eq!(error.code, "GIT_COMMAND_FAILED");
        assert!(
            !marker.exists(),
            "worktree-scoped Git filter executed before policy rejection"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn blocked_project_git_lane_does_not_block_another_project() {
        let root_a = unique_temp_dir("lane-a");
        let root_b = unique_temp_dir("lane-b");
        for root in [&root_a, &root_b] {
            fs::create_dir_all(root).unwrap();
            run_git_in(root, &["init"]);
        }
        let lane_a = git_project_lane(&root_a).unwrap();
        let _blocked = lane_a.lock().unwrap();
        let context_b = ProjectContext::new("project-b", root_b.clone());
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(run_git(&context_b, &["status", "--porcelain"]));
        });

        let result = receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("project B Git should not wait on project A's lane");
        assert!(result.is_ok());
        fs::remove_dir_all(root_a).ok();
        fs::remove_dir_all(root_b).ok();
    }

    #[test]
    fn task_cancellation_interrupts_a_git_lane_wait() {
        let root = unique_temp_dir("lane-cancel");
        fs::create_dir_all(&root).unwrap();
        run_git_in(&root, &["init"]);
        let lane = git_project_lane(&root).unwrap();
        let _blocked = lane.lock().unwrap();
        let context = ProjectContext::new("project-a", root.clone());
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            entered_tx.send(()).unwrap();
            let result = GitService.with_task_cancellation(worker_cancellation, || {
                run_git(&context, &["status", "--porcelain"])
            });
            result_tx.send(result).unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        cancellation.cancel();

        let error = result_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("task cancellation must interrupt the Git lane wait")
            .expect_err("cancelled Git must fail closed");
        assert_eq!(error.code, "GIT_COMMAND_CANCELLED");
        worker.join().unwrap();
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn initializes_repo_creates_checkpoint_and_markdown_diff() {
        let root = unique_temp_dir("checkpoint");
        let context = ProjectContext::new("project-1", root.clone());
        fs::write(root.join("purpose.md"), "# Purpose\n").unwrap();

        let service = GitService;
        let repo = service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap();
        assert!(repo.is_repository);
        assert!(root.join(".git").exists());

        fs::write(root.join("purpose.md"), "# Purpose\n\nUpdated\n").unwrap();
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki").join("概念.md"), "# 概念\n").unwrap();

        let diff = service.diff_markdown(&context).unwrap();
        assert!(diff.affected_paths.contains(&"purpose.md".to_string()));
        assert!(diff.affected_paths.contains(&"wiki/概念.md".to_string()));
        assert!(diff.markdown.contains("```diff"));

        let checkpoint = service
            .create_checkpoint(
                &context,
                CheckpointPurpose::HighRiskOperation,
                "Before overwrite",
            )
            .unwrap();
        assert!(checkpoint.created);
        assert!(checkpoint.commit_hash.is_some());
        assert!(checkpoint
            .affected_paths
            .contains(&"purpose.md".to_string()));
        assert!(checkpoint
            .affected_paths
            .contains(&"wiki/概念.md".to_string()));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn clean_head_checkpoint_never_absorbs_a_dirty_worktree() {
        let root = unique_temp_dir("clean-only-checkpoint");
        let context = ProjectContext::new("project-1", root.clone());
        fs::write(root.join("purpose.md"), "# Purpose\n").unwrap();
        let service = GitService;
        let initial = service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap()
            .head
            .unwrap();

        fs::write(root.join("purpose.md"), "# External edit\n").unwrap();
        let error = service
            .clean_head_checkpoint(
                &context,
                CheckpointPurpose::HighRiskOperation,
                "Before Update Wiki",
            )
            .unwrap_err();

        assert_eq!(error.code, "GIT_WORKTREE_DIRTY");
        assert_eq!(
            service.repository_status(&context).unwrap().head,
            Some(initial)
        );
        assert_eq!(
            fs::read_to_string(root.join("purpose.md")).unwrap(),
            "# External edit\n"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn workflow_checkpoint_ignores_only_exact_task_state_noise() {
        let root = unique_temp_dir("workflow-clean-checkpoint");
        let context = ProjectContext::new("project", root.clone());
        std::fs::create_dir_all(root.join("wiki")).unwrap();
        std::fs::create_dir_all(root.join(".app/tasks")).unwrap();
        std::fs::write(root.join("wiki/a.md"), "# A").unwrap();
        std::fs::write(root.join(".app/tasks/task-1.json"), "{}").unwrap();
        let service = GitService;
        service.initialize_repository(&context, "Initial").unwrap();
        std::fs::write(root.join(".app/tasks/task-1.json"), "{\"running\":true}").unwrap();

        let checkpoint = service
            .clean_head_checkpoint_allowing_paths(
                &context,
                CheckpointPurpose::HighRiskOperation,
                "Agent lint repair task-1",
                &[".app/tasks/task-1.json".into()],
            )
            .unwrap();
        assert!(!checkpoint.created);
        assert!(checkpoint.commit_hash.is_some());

        std::fs::write(root.join("wiki/a.md"), "# External edit").unwrap();
        assert_eq!(
            service
                .clean_head_checkpoint_allowing_paths(
                    &context,
                    CheckpointPurpose::HighRiskOperation,
                    "Agent lint repair task-1",
                    &[".app/tasks/task-1.json".into()],
                )
                .unwrap_err()
                .code,
            "GIT_WORKTREE_DIRTY"
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn checkpoint_preview_rejects_new_unconfirmed_paths_without_committing() {
        let root = unique_temp_dir("checkpoint-preview-drift");
        let context = ProjectContext::new("project-1", root.clone());
        fs::write(root.join("existing.md"), "initial\n").unwrap();
        let service = GitService;
        let initial_head = service
            .initialize_repository(&context, "Initial project")
            .unwrap()
            .head;
        fs::write(root.join("existing.md"), "changed\n").unwrap();
        let expected = service.changed_paths(&context).unwrap();
        fs::write(root.join("late.md"), "late\n").unwrap();

        let error = service
            .verify_checkpoint_state(&context, initial_head.as_deref(), &expected)
            .unwrap_err();

        assert_eq!(error.code, "GIT_CHECKPOINT_STATE_CHANGED");
        assert_eq!(
            service.repository_status(&context).unwrap().head,
            initial_head
        );
        assert!(service.repository_status(&context).unwrap().has_changes);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn initializes_empty_repo_with_empty_initial_commit() {
        let root = unique_temp_dir("empty-init");
        let context = ProjectContext::new("project-1", root.clone());
        let service = GitService;

        let repo = service
            .initialize_repository(&context, "Initial empty project")
            .unwrap();

        assert!(repo.is_repository);
        assert!(repo.head.is_some());
        assert!(!repo.has_changes);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn completes_an_existing_unborn_repository_with_an_initial_commit() {
        let root = unique_temp_dir("unborn-init");
        let context = ProjectContext::new("project-1", root.clone());
        run_git_in(&root, &["init"]);
        fs::write(root.join("note.md"), "# Note\n").unwrap();
        let service = GitService;
        let before = service.repository_status(&context).unwrap();
        assert!(before.is_repository);
        assert!(before.head.is_none());
        let expected_paths = service.initial_commit_paths(&context).unwrap();
        service
            .verify_initialization_state(&context, None, &expected_paths)
            .unwrap();

        let after = service
            .initialize_repository_from_snapshot(&context, "Initial project", &expected_paths)
            .unwrap();

        assert!(after.head.is_some());
        assert!(!after.has_changes);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn assessed_initialization_rejects_files_added_after_confirmation() {
        let root = unique_temp_dir("initialization-preview-drift");
        let context = ProjectContext::new("project-1", root.clone());
        fs::write(root.join("approved.md"), "approved\n").unwrap();
        let service = GitService;
        let expected_paths = service.initial_commit_paths(&context).unwrap();
        fs::write(root.join("late.md"), "late\n").unwrap();

        let error = service
            .verify_initialization_state(&context, None, &expected_paths)
            .unwrap_err();

        assert_eq!(error.code, "GIT_INITIALIZATION_STATE_CHANGED");
        assert!(!root.join(".git").exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn parent_repository_is_not_treated_as_project_local_history() {
        let parent = unique_temp_dir("parent-repository");
        let parent_context = ProjectContext::new("parent", parent.clone());
        fs::write(parent.join("parent.md"), "parent\n").unwrap();
        let service = GitService;
        service
            .initialize_repository(&parent_context, "Initial parent")
            .unwrap();

        let child = parent.join("knowledge-base");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("note.md"), "# Note\n").unwrap();
        let child_context = ProjectContext::new("child", child.clone());

        assert!(
            !service
                .repository_status(&child_context)
                .unwrap()
                .is_repository
        );
        assert_eq!(
            service
                .create_checkpoint(
                    &child_context,
                    CheckpointPurpose::HighRiskOperation,
                    "Unsafe parent checkpoint",
                )
                .unwrap_err()
                .code,
            "GIT_REPOSITORY_MISSING"
        );

        let nested = service
            .initialize_repository(&child_context, "Initial knowledge base")
            .unwrap();
        assert!(nested.is_repository);
        let top_level = run_git(&child_context, &["rev-parse", "--show-toplevel"]).unwrap();
        assert_eq!(
            Path::new(top_level.trim()).canonicalize().unwrap(),
            child.canonicalize().unwrap()
        );
        fs::remove_dir_all(parent).ok();
    }

    #[cfg(unix)]
    #[test]
    fn linked_git_metadata_is_rejected() {
        use std::os::unix::fs::symlink;

        let root = unique_temp_dir("linked-git-root");
        let external = unique_temp_dir("linked-git-external");
        symlink(&external, root.join(".git")).unwrap();
        let context = ProjectContext::new("linked", root.clone());

        let error = GitService.repository_status(&context).unwrap_err();
        assert_eq!(error.code, "GIT_REPOSITORY_PATH_UNSAFE");
        fs::remove_file(root.join(".git")).ok();
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(external).ok();
    }

    #[test]
    fn changed_files_since_head_reports_status_and_changed_chars() {
        let root = unique_temp_dir("changed-files");
        let context = ProjectContext::new("project-1", root.clone());
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki").join("existing.md"), "alpha\n").unwrap();
        fs::write(root.join("wiki").join("deleted.md"), "remove me\n").unwrap();

        let service = GitService;
        service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap();

        fs::write(
            root.join("wiki").join("existing.md"),
            "alpha\nupdated line\n",
        )
        .unwrap();
        fs::write(root.join("wiki").join("new.md"), "new page\n").unwrap();
        fs::remove_file(root.join("wiki").join("deleted.md")).unwrap();

        let changes = service.changed_files_since_head(&context).unwrap();
        let by_path: HashMap<String, _> = changes
            .into_iter()
            .map(|change| (change.path.clone(), change))
            .collect();

        assert_eq!(
            by_path.get("wiki/existing.md").map(|change| &change.kind),
            Some(&GitChangedFileKind::Modified)
        );
        assert_eq!(
            by_path.get("wiki/new.md").map(|change| &change.kind),
            Some(&GitChangedFileKind::Added)
        );
        assert_eq!(
            by_path.get("wiki/deleted.md").map(|change| &change.kind),
            Some(&GitChangedFileKind::Deleted)
        );
        assert!(by_path["wiki/existing.md"].changed_chars > 0);
        assert!(by_path["wiki/new.md"].changed_chars > 0);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn changed_files_since_head_counts_long_single_line_bytes() {
        let root = unique_temp_dir("long-line");
        let context = ProjectContext::new("project-1", root.clone());
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki").join("page.md"), "short").unwrap();

        let service = GitService;
        service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap();

        fs::write(root.join("wiki").join("page.md"), "x".repeat(5_000)).unwrap();

        let changes = service.changed_files_since_head(&context).unwrap();
        let page = changes
            .iter()
            .find(|change| change.path == "wiki/page.md")
            .unwrap();
        assert_eq!(page.kind, GitChangedFileKind::Modified);
        assert!(page.changed_chars > 2_000);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rollback_removes_new_directory_through_a_bound_tree() {
        let root = unique_temp_dir("bound-rollback-tree");
        let context = ProjectContext::new("project-1", root.clone());
        fs::create_dir_all(root.join("scratch/nested")).unwrap();
        fs::write(root.join("scratch/nested/page.md"), "temporary\n").unwrap();

        remove_project_path(&context, "scratch").unwrap();

        assert!(!root.join("scratch").exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn untracked_diff_rejects_a_symlink_instead_of_reading_its_target() {
        let root = unique_temp_dir("untracked-symlink");
        let outside = unique_temp_dir("untracked-symlink-outside");
        let context = ProjectContext::new("project-1", root.clone());
        fs::write(root.join("tracked.md"), "tracked\n").unwrap();
        GitService
            .initialize_repository(&context, "Initial project")
            .unwrap();
        fs::write(outside.join("secret.md"), "outside-secret\n").unwrap();
        let link = root.join("leak.md");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(outside.join("secret.md"), &link).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(outside.join("secret.md"), &link).is_ok();
        if !linked {
            fs::remove_dir_all(root).ok();
            fs::remove_dir_all(outside).ok();
            return;
        }

        let error =
            untracked_file_diff(&context).expect_err("symlinked worktree file must fail closed");
        assert_eq!(error.code, "GIT_DIFF_FAILED");

        fs::remove_file(link).ok();
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn diff_since_head_includes_untracked_file_content() {
        let root = unique_temp_dir("untracked-diff");
        let context = ProjectContext::new("project-1", root.clone());
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki").join("page.md"), "stable\n").unwrap();

        let service = GitService;
        service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap();

        fs::write(root.join("wiki").join("new page.md"), "# New page\nBody\n").unwrap();

        let diff = service.diff_since_head(&context).unwrap();
        assert!(diff.contains("diff --git a/wiki/new page.md b/wiki/new page.md"));
        assert!(diff.contains("+++ b/wiki/new page.md"));
        assert!(diff.contains("+# New page"));
        assert!(diff.contains("+Body"));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn changed_files_since_head_with_ignored_baseline_reports_new_ignored_only() {
        let root = unique_temp_dir("ignored-audit");
        let context = ProjectContext::new("project-1", root.clone());
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        fs::write(root.join("wiki").join("page.md"), "stable\n").unwrap();
        fs::write(root.join("keep.log"), "preexisting\n").unwrap();

        let service = GitService;
        service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap();

        fs::write(root.join("agent.log"), "agent artifact\n").unwrap();

        let changes = service
            .changed_files_since_head_with_ignored_baseline(&context, &["keep.log".to_string()])
            .unwrap();
        let by_path: HashMap<String, _> = changes
            .into_iter()
            .map(|change| (change.path.clone(), change))
            .collect();

        assert!(!by_path.contains_key("keep.log"));
        assert_eq!(
            by_path.get("agent.log").map(|change| &change.kind),
            Some(&GitChangedFileKind::Added)
        );
        assert!(by_path["agent.log"].changed_chars > 0);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rollback_to_head_restores_worktree_after_agent_changes() {
        let root = unique_temp_dir("rollback");
        let context = ProjectContext::new("project-1", root.clone());
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki").join("page.md"), "stable\n").unwrap();

        let service = GitService;
        service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap();

        fs::write(root.join("wiki").join("page.md"), "agent edit\n").unwrap();
        fs::write(root.join("wiki").join("agent-new.md"), "draft\n").unwrap();

        service.rollback_worktree_to_head(&context).unwrap();

        let restored = fs::read_to_string(root.join("wiki").join("page.md"))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(restored, "stable\n");
        assert!(!root.join("wiki").join("agent-new.md").exists());
        assert!(!service.repository_status(&context).unwrap().has_changes);

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rollback_preserving_ignored_baseline_removes_new_ignored_only() {
        let root = unique_temp_dir("ignored-rollback");
        let context = ProjectContext::new("project-1", root.clone());
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        fs::write(root.join("wiki").join("page.md"), "stable\n").unwrap();
        fs::write(root.join("keep.log"), "preexisting\n").unwrap();

        let service = GitService;
        service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap();

        fs::write(root.join("wiki").join("page.md"), "agent edit\n").unwrap();
        fs::write(root.join("agent.log"), "agent artifact\n").unwrap();

        service
            .rollback_worktree_to_head_preserving_ignored(&context, &["keep.log".to_string()])
            .unwrap();

        let restored = fs::read_to_string(root.join("wiki").join("page.md"))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(restored, "stable\n");
        assert!(root.join("keep.log").exists());
        assert!(!root.join("agent.log").exists());

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn scoped_rollback_preserves_unrelated_worktree_edits() {
        let root = unique_temp_dir("scoped-rollback");
        let context = ProjectContext::new("project-1", root.clone());
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki").join("page.md"), "stable\n").unwrap();
        fs::write(root.join("notes.md"), "stable notes\n").unwrap();

        let service = GitService;
        service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap();

        fs::write(root.join("wiki").join("page.md"), "agent edit\n").unwrap();
        fs::write(root.join("wiki").join("agent-new.md"), "draft\n").unwrap();
        fs::write(root.join("notes.md"), "user edit\n").unwrap();

        service
            .rollback_paths_to_head_preserving_ignored(
                &context,
                &["wiki/page.md".to_string(), "wiki/agent-new.md".to_string()],
                &[],
            )
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("wiki/page.md"))
                .unwrap()
                .replace("\r\n", "\n"),
            "stable\n"
        );
        assert!(!root.join("wiki/agent-new.md").exists());
        assert_eq!(
            fs::read_to_string(root.join("notes.md")).unwrap(),
            "user edit\n"
        );

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn scoped_checkpoint_rollback_requires_exact_head_and_preserves_unrelated_edits() {
        let root = unique_temp_dir("scoped-checkpoint-rollback");
        let context = ProjectContext::new("project-1", root.clone());
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/page.md"), "stable\n").unwrap();
        fs::write(root.join("notes.md"), "stable notes\n").unwrap();

        let service = GitService;
        let initial = service
            .initialize_repository(&context, "Initial wiki project")
            .unwrap()
            .head
            .unwrap();
        fs::write(root.join("wiki/page.md"), "agent edit\n").unwrap();
        fs::write(root.join("wiki/agent-new.md"), "draft\n").unwrap();
        let final_commit = service
            .create_scoped_checkpoint(
                &context,
                CheckpointPurpose::FinalResult,
                "Agent lint repair",
                &["wiki/page.md".into(), "wiki/agent-new.md".into()],
            )
            .unwrap()
            .commit_hash
            .unwrap();
        fs::write(root.join("notes.md"), "user edit\n").unwrap();

        let rollback = service
            .rollback_paths_to_checkpoint(
                &context,
                &final_commit,
                &initial,
                "Rollback Agent lint repair",
                &["wiki/page.md".into(), "wiki/agent-new.md".into()],
            )
            .unwrap();

        assert!(rollback.created);
        assert_ne!(rollback.commit_hash.as_deref(), Some(final_commit.as_str()));
        assert_eq!(
            fs::read_to_string(root.join("wiki/page.md"))
                .unwrap()
                .replace("\r\n", "\n"),
            "stable\n"
        );
        assert!(!root.join("wiki/agent-new.md").exists());
        assert_eq!(
            fs::read_to_string(root.join("notes.md")).unwrap(),
            "user edit\n"
        );
        assert!(service
            .is_exact_compensating_rollback(
                &context,
                &final_commit,
                &initial,
                &["wiki/page.md".into(), "wiki/agent-new.md".into()],
            )
            .unwrap());

        let stale = service
            .rollback_paths_to_checkpoint(
                &context,
                &final_commit,
                &initial,
                "Rollback Agent lint repair",
                &["wiki/page.md".into()],
            )
            .unwrap_err();
        assert_eq!(stale.code, "GIT_ROLLBACK_NOT_CURRENT");

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn rollback_rejects_a_parent_repository_without_touching_any_files() {
        let root = unique_temp_dir("parent-repo");
        let project = root.join("project");
        fs::create_dir_all(project.join("wiki")).unwrap();
        fs::write(project.join("wiki").join("page.md"), "stable\n").unwrap();
        fs::write(root.join("outside.txt"), "outside stable\n").unwrap();

        run_git_in(&root, &["init"]);
        run_git_in(&root, &["config", "user.name", "test"]);
        run_git_in(&root, &["config", "user.email", "test@example.local"]);
        run_git_in(&root, &["add", "--all"]);
        run_git_in(&root, &["commit", "-m", "init"]);

        fs::write(project.join("wiki").join("page.md"), "agent edit\n").unwrap();
        fs::write(project.join("wiki").join("agent-new.md"), "draft\n").unwrap();
        fs::write(root.join("outside.txt"), "outside edit\n").unwrap();

        let service = GitService;
        let context = ProjectContext::new("project-1", project.clone());
        let error = service.rollback_worktree_to_head(&context).unwrap_err();

        assert_eq!(error.code, "GIT_REPOSITORY_MISSING");
        let unchanged = fs::read_to_string(project.join("wiki").join("page.md"))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(unchanged, "agent edit\n");
        assert!(project.join("wiki").join("agent-new.md").exists());
        assert_eq!(
            fs::read_to_string(root.join("outside.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "outside edit\n"
        );

        fs::remove_dir_all(root).ok();
    }
}
