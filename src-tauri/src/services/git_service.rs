#[derive(Default)]
pub struct GitService;

use std::process::Command;

use crate::errors::BackendError;
use crate::models::git::{CheckpointPurpose, GitCheckpoint, GitDiff, GitRepositoryStatus};
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
}

fn run_git(context: &ProjectContext, args: &[&str]) -> Result<String, BackendError> {
    let output = Command::new("git")
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .current_dir(&context.root)
        .output()
        .map_err(|err| BackendError::new("GIT_COMMAND_FAILED", err.to_string(), true, false))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
    let raw = run_git(context, &["status", "--porcelain", "-uall"])?;
    let mut paths = Vec::new();
    for line in raw.lines() {
        if line.len() < 4 {
            continue;
        }
        let path_part = &line[3..];
        let path = path_part
            .split(" -> ")
            .last()
            .unwrap_or(path_part)
            .trim_matches('"')
            .replace('\\', "/");
        if !path.is_empty() {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::GitService;
    use crate::models::git::CheckpointPurpose;
    use crate::models::paths::ProjectContext;
    use std::fs;
    use std::path::PathBuf;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("llm-wiki-git-{label}-{stamp}"));
        fs::create_dir_all(&dir).unwrap();
        dir
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
}
