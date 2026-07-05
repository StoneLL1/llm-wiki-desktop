#[derive(Default)]
pub struct GitService;

use std::fs;
use std::process::Command;

use crate::errors::BackendError;
use crate::models::git::{
    CheckpointPurpose, GitChangedFile, GitChangedFileKind, GitCheckpoint, GitDiff,
    GitRepositoryStatus,
};
use crate::models::paths::ProjectContext;

impl GitService {
    pub fn initialize_repository(
        &self,
        context: &ProjectContext,
        initial_message: &str,
    ) -> Result<GitRepositoryStatus, BackendError> {
        if !context.root.join(".git").exists() {
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
        let is_repository = context.root.join(".git").exists()
            || run_git(context, &["rev-parse", "--is-inside-work-tree"]).is_ok();
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

        run_git(context, &["restore", "--source=HEAD", "--staged", "--worktree", "--", "."])?;
        run_git(context, &["clean", "-fd", "--", "."])?;
        remove_new_ignored_paths(context, preserved_ignored_paths)?;
        Ok(())
    }
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

fn append_ignored_changes(
    context: &ProjectContext,
    changes: &mut Vec<GitChangedFile>,
    preserved_ignored_paths: &[String],
) -> Result<(), BackendError> {
    for path in ignored_paths(context)? {
        if preserved_ignored_paths.iter().any(|preserved| preserved == &path)
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
    let raw = run_git_bytes(context, &["ls-files", "--others", "--exclude-standard", "-z"])?;
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
        &["ls-files", "--others", "--ignored", "--exclude-standard", "-z"],
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
        if preserved_ignored_paths.iter().any(|preserved| preserved == &path) {
            continue;
        }
        remove_project_path(context, &path)?;
    }
    Ok(())
}

fn remove_project_path(context: &ProjectContext, path: &str) -> Result<(), BackendError> {
    let root = context.root.canonicalize().map_err(|err| {
        BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false)
    })?;
    let target = context.root.join(path);
    let target_abs = target.canonicalize().map_err(|err| {
        BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false)
    })?;
    if target_abs == root || !target_abs.starts_with(&root) {
        return Err(BackendError::new(
            "GIT_ROLLBACK_FAILED",
            format!("Refusing to remove path outside the project root: {path}"),
            true,
            false,
        ));
    }
    let metadata = fs::symlink_metadata(&target).map_err(|err| {
        BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false)
    })?;
    if metadata.is_dir() {
        fs::remove_dir_all(&target)
    } else {
        fs::remove_file(&target)
    }
    .map_err(|err| BackendError::new("GIT_ROLLBACK_FAILED", err.to_string(), true, false))
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
    use super::GitService;
    use crate::models::git::{CheckpointPurpose, GitChangedFileKind};
    use crate::models::paths::ProjectContext;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command as ProcessCommand;

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
    fn rollback_from_parent_repo_subdirectory_preserves_outside_files() {
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
        service.rollback_worktree_to_head(&context).unwrap();

        let restored = fs::read_to_string(project.join("wiki").join("page.md"))
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(restored, "stable\n");
        assert!(!project.join("wiki").join("agent-new.md").exists());
        assert_eq!(
            fs::read_to_string(root.join("outside.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "outside edit\n"
        );

        fs::remove_dir_all(root).ok();
    }
}
