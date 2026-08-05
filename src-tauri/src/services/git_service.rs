#[derive(Default)]
pub struct GitService;

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use crate::errors::BackendError;
use crate::models::git::{
    CheckpointPurpose, GitChangedFile, GitChangedFileKind, GitCheckpoint, GitDiff,
    GitRepositoryStatus,
};
use crate::models::paths::ProjectContext;
use crate::utils::path_safety::{
    validate_existing_project_directory, validate_existing_project_file,
    validate_existing_project_root,
};

impl GitService {
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
        Ok(run_git(context, &["ls-files", "--error-unmatch", "--", relative_path]).is_ok())
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
        let output = Command::new("git")
            .current_dir(&context.root)
            .args(["diff", "--no-index", "--no-ext-diff", "--"])
            .arg(&relative[0])
            .arg(&relative[1])
            .output()
            .map_err(|error| {
                BackendError::new("GIT_DIFF_FAILED", error.to_string(), true, false)
            })?;
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
        let version = Command::new("git")
            .arg("--version")
            .output()
            .map_err(|error| {
                BackendError::new("GIT_COMMAND_FAILED", error.to_string(), true, false)
            })?;
        if !version.status.success() {
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
        let tracked_diff = run_git(context, &["diff", "--no-ext-diff", "--"]).unwrap_or_default();
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

        let mut diff = run_git(context, &["diff", "--no-ext-diff", "HEAD", "--"])?;
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

const MAX_ASSESSMENT_GIT_OUTPUT_BYTES: u64 = 64 * 1024;
const ASSESSMENT_READER_GRACE: Duration = Duration::from_millis(100);

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

    let mut child = Command::new("git")
        .arg("--no-optional-locks")
        .args([
            "-c",
            "core.quotepath=false",
            "-c",
            "core.fsmonitor=false",
            "-c",
            assessment_disabled_hooks_config(),
        ])
        .args(args)
        .current_dir(&context.root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| BackendError::new("GIT_COMMAND_FAILED", error.to_string(), true, false))?;
    let stdout = child.stdout.take().expect("piped Git stdout");
    let stderr = child.stderr.take().expect("piped Git stderr");
    let stdout_reader = spawn_bounded_reader(stdout);
    let stderr_reader = spawn_bounded_reader(stderr);

    let status = loop {
        if cancelled.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(BackendError::new(
                "PROJECT_ASSESSMENT_CANCELLED",
                "Project assessment was cancelled.",
                true,
                true,
            ));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(git_assessment_timeout());
        }
        match child.try_wait().map_err(|error| {
            BackendError::new("GIT_COMMAND_FAILED", error.to_string(), true, false)
        })? {
            Some(status) => break status,
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    };

    Ok(BoundedGitOutput {
        success: status.success(),
        stdout: receive_bounded_reader(stdout_reader),
        stderr: receive_bounded_reader(stderr_reader),
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

fn spawn_bounded_reader(reader: impl Read + Send + 'static) -> Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(read_bounded(reader));
    });
    receiver
}

fn receive_bounded_reader(receiver: Receiver<Vec<u8>>) -> Vec<u8> {
    receiver
        .recv_timeout(ASSESSMENT_READER_GRACE)
        .unwrap_or_default()
}

fn read_bounded(reader: impl Read) -> Vec<u8> {
    let mut bytes = Vec::new();
    let _ = reader
        .take(MAX_ASSESSMENT_GIT_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes);
    bytes
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
    let output = Command::new("git")
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .current_dir(&context.root)
        .output()
        .map_err(|err| BackendError::new("GIT_COMMAND_FAILED", err.to_string(), true, false))?;

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
    let root = context
        .root
        .canonicalize()
        .map_err(|err| BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false))?;
    let target = context.root.join(path);
    let target_abs = target
        .canonicalize()
        .map_err(|err| BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false))?;
    if target_abs == root || !target_abs.starts_with(&root) {
        return Err(BackendError::new(
            "GIT_ROLLBACK_FAILED",
            format!("Refusing to remove path outside the project root: {path}"),
            true,
            false,
        ));
    }
    let metadata = fs::symlink_metadata(&target)
        .map_err(|err| BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false))?;
    if metadata.is_dir() {
        fs::remove_dir_all(&target)
    } else {
        fs::remove_file(&target)
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
        let bytes = fs::read(context.root.join(&path)).unwrap_or_default();
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
    let worktree_len = fs::read(context.root.join(&change.path)).map_or(0, |bytes| bytes.len());
    match change.kind {
        GitChangedFileKind::Added => worktree_len.max(head_len),
        GitChangedFileKind::Deleted => head_len,
        GitChangedFileKind::Modified | GitChangedFileKind::Renamed => {
            head_len.saturating_add(worktree_len)
        }
    }
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
    use super::{run_git, GitService};
    use crate::models::git::{CheckpointPurpose, GitChangedFileKind};
    use crate::models::paths::ProjectContext;
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
