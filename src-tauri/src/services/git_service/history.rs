//! Application history uses private refs and a temporary index. It never
//! stages user files, moves HEAD, or reads bytes through Git attributes.
use super::*;
use crate::services::FileStore;
use crate::utils::private_directory::create_private_directory;
use std::collections::{BTreeMap, HashSet};

const HISTORY_TIMEOUT: Duration = Duration::from_secs(60);

struct SnapshotDirectory(PathBuf);
impl Drop for SnapshotDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn history_ref(task_id: &str, phase: &str) -> Result<String, BackendError> {
    uuid::Uuid::parse_str(task_id).map_err(|_| history_error("Invalid history operation id."))?;
    if !matches!(
        phase,
        "baseline" | "before" | "planned" | "after" | "undo-started" | "undo"
    ) {
        return Err(history_error("Invalid history phase."));
    }
    Ok(format!("refs/llm-wiki/operations/{task_id}/{phase}"))
}

fn history_error(message: impl Into<String>) -> BackendError {
    BackendError::new("GIT_HISTORY_FAILED", message, true, true)
}

fn validate_history_path(path: &str) -> Result<(), BackendError> {
    if path.contains(['\0', '\\'])
        || path.split('/').any(|part| {
            part.is_empty() || part == "." || part == ".." || part.eq_ignore_ascii_case(".git")
        })
    {
        return Err(history_error("Invalid history file path."));
    }
    validate_relative_git_path(path)
}

impl GitService {
    /// Read the selected blobs in one batch, preserving binary bytes and
    /// absence. Path names never become batch protocol commands.
    pub fn read_history_files(
        &self,
        context: &ProjectContext,
        commit: &str,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<Vec<u8>>>, BackendError> {
        self.read_history_files_bounded(context, commit, paths, usize::MAX)
    }

    pub fn read_history_files_bounded(
        &self,
        context: &ProjectContext,
        commit: &str,
        paths: &[String],
        byte_limit: usize,
    ) -> Result<BTreeMap<String, Option<Vec<u8>>>, BackendError> {
        if !Self::checkpoint_exists(&context.root, commit) {
            return Err(history_error("The recovery snapshot is unavailable."));
        }
        let tree = run_git_bytes(context, &["ls-tree", "-r", "-l", "-z", commit])?;
        let mut objects = HashMap::new();
        for entry in tree
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
        {
            let Some(tab) = entry.iter().position(|byte| *byte == b'\t') else {
                return Err(history_error("Invalid history tree."));
            };
            let header = String::from_utf8_lossy(&entry[..tab]);
            let mut fields = header.split_whitespace();
            let _mode = fields.next();
            if fields.next() != Some("blob") {
                return Err(history_error("History must contain regular files."));
            }
            let hash = fields
                .next()
                .ok_or_else(|| history_error("Invalid history object."))?;
            let path = String::from_utf8(entry[tab + 1..].to_vec())
                .map_err(|_| history_error("History paths must be UTF-8."))?;
            let size = fields
                .next()
                .and_then(|size| size.parse::<usize>().ok())
                .ok_or_else(|| history_error("Invalid history object size."))?;
            objects.insert(path, (hash.to_string(), size));
        }
        let mut files = BTreeMap::new();
        let mut present = Vec::new();
        let mut query = Vec::new();
        let mut output_limit = 0usize;
        for path in paths {
            validate_history_path(path)?;
            if let Some((hash, size)) = objects.get(path) {
                output_limit = output_limit
                    .checked_add(*size)
                    .and_then(|total| total.checked_add(128))
                    .ok_or_else(|| history_error("History objects are too large to read."))?;
                if output_limit > byte_limit.saturating_add(paths.len().saturating_mul(128)) {
                    return Err(BackendError::new("VERSION_SIZE_LIMIT", "Selected recovery files exceed the supported read budget.", true, true));
                }
                present.push(path.clone());
                query.extend_from_slice(hash.as_bytes());
                query.push(b'\n');
            } else {
                files.insert(path.clone(), None);
            }
        }
        if !present.is_empty() {
            let lane = git_project_lane(&context.root)?;
            let _guard = lock_git_lane(&lane, HISTORY_TIMEOUT, git_task_cancelled)
                .map_err(|error| git_process_error(error, &["history", "read"]))?;
            reject_local_git_filters(context, HISTORY_TIMEOUT, &git_task_cancelled)
                .map_err(|error| git_process_error(error, &["history", "read"]))?;
            let mut command = hardened_git_command(context);
            command.args(["cat-file", "--batch"]);
            let output = run_bounded_process(
                &mut command,
                Some(query),
                HISTORY_TIMEOUT,
                output_limit,
                git_task_cancelled,
            )
            .map_err(|error| git_process_error(error, &["history", "read"]))?;
            if !output.status.success() {
                return Err(git_command_error(&output.stderr, &["history", "read"]));
            }
            let mut rest = output.stdout.as_slice();
            for path in present {
                let newline = rest
                    .iter()
                    .position(|byte| *byte == b'\n')
                    .ok_or_else(|| history_error("Incomplete history object."))?;
                let header = String::from_utf8_lossy(&rest[..newline]);
                let fields = header.split_whitespace().collect::<Vec<_>>();
                let size = fields
                    .get(2)
                    .and_then(|size| size.parse::<usize>().ok())
                    .filter(|_| fields.get(1) == Some(&"blob"))
                    .ok_or_else(|| history_error("Invalid history blob."))?;
                rest = &rest[newline + 1..];
                if size >= rest.len() || rest[size] != b'\n' {
                    return Err(history_error("Truncated history blob."));
                }
                files.insert(path, Some(rest[..size].to_vec()));
                rest = &rest[size + 1..];
            }
        }
        Ok(files)
    }

    /// Capture exact project bytes; a supplied baseline also binds absence.
    pub fn capture_history_files(
        &self,
        context: &ProjectContext,
        paths: &[String],
        expected: Option<&HashMap<String, String>>,
    ) -> Result<BTreeMap<String, Option<Vec<u8>>>, BackendError> {
        let mut files = BTreeMap::new();
        for path in paths {
            validate_history_path(path)?;
            let absolute = context.resolve_project_path(path)?;
            let bytes = match fs::symlink_metadata(&absolute) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    Some(FileStore.read_bytes(context, path)?)
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Ok(_) => {
                    return Err(history_error(
                        "History inputs must be regular project files.",
                    ))
                }
                Err(error) => return Err(history_error(error.to_string())),
            };
            if let Some(expected) = expected {
                let actual = bytes.as_ref().map(|bytes| FileStore.content_hash(bytes));
                if actual.as_ref() != expected.get(path) {
                    return Err(BackendError::new(
                        "WORKFLOW_OUTPUT_BASELINE_CHANGED",
                        "A file changed before its recovery snapshot was captured.",
                        true,
                        true,
                    ));
                }
            }
            files.insert(path.clone(), bytes);
        }
        Ok(files)
    }

    /// Called only after explicit workflow write admission. A project without
    /// Git gets local object storage, without an initial whole-project commit.
    pub fn create_history_snapshot(
        &self,
        context: &ProjectContext,
        task_id: &str,
        phase: &str,
        message: &str,
        parent: Option<&str>,
        files: &BTreeMap<String, Option<Vec<u8>>>,
    ) -> Result<GitCheckpoint, BackendError> {
        let reference = history_ref(task_id, phase)?;
        for path in files.keys() {
            validate_history_path(path)?;
        }
        if parent.is_some_and(|hash| {
            !(7..=64).contains(&hash.len()) || !hash.bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err(history_error("Invalid parent history object."));
        }
        let root = validate_existing_project_root(&context.root).map_err(git_path_unsafe)?;
        let has_git = validate_git_marker(&root)?;
        let lane = git_project_lane(&root)?;
        let started = Instant::now();
        let _guard = lock_git_lane(&lane, HISTORY_TIMEOUT, git_task_cancelled)
            .map_err(|error| git_process_error(error, &["history"]))?;
        reject_local_git_filters(
            context,
            HISTORY_TIMEOUT.saturating_sub(started.elapsed()),
            &git_task_cancelled,
        )
        .map_err(|error| git_process_error(error, &["history"]))?;
        let directory =
            std::env::temp_dir().join(format!("llm-wiki-history-{}", uuid::Uuid::new_v4()));
        create_private_directory(&directory).map_err(|error| history_error(error.to_string()))?;
        let directory = SnapshotDirectory(directory);
        let index = directory.0.join("index");
        let run = |args: &[&str], input: Option<Vec<u8>>| -> Result<String, BackendError> {
            let mut command = hardened_git_command(context);
            command
                .args(args)
                .env("GIT_INDEX_FILE", &index)
                .env("GIT_AUTHOR_NAME", "LLM Wiki Desktop")
                .env("GIT_COMMITTER_NAME", "LLM Wiki Desktop")
                .env("GIT_AUTHOR_EMAIL", "llm-wiki-desktop@example.local")
                .env("GIT_COMMITTER_EMAIL", "llm-wiki-desktop@example.local");
            let output = run_bounded_process(
                &mut command,
                input,
                HISTORY_TIMEOUT.saturating_sub(started.elapsed()),
                MAX_GIT_OUTPUT_BYTES,
                git_task_cancelled,
            )
            .map_err(|error| git_process_error(error, args))?;
            if !output.status.success() {
                return Err(git_command_error(&output.stderr, args));
            }
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        };
        if !has_git {
            // A nested unborn repository changes how the parent Git sees this
            // folder. Refuse before writing metadata rather than capture it.
            if run(&["rev-parse", "--show-toplevel"], None).is_ok() {
                return Err(history_error("This project is inside another Git repository. Open that repository as the project, or create a separate project folder for application history."));
            }
            run(&["init", "--quiet"], None)?;
        }
        let top = run(&["rev-parse", "--show-toplevel"], None)?;
        if Path::new(&top).canonicalize().ok().as_ref() != Some(&root) {
            return Err(history_error(
                "History repository must belong to this project.",
            ));
        }
        run(&["read-tree", "--empty"], None)?;
        let mut input_paths = String::new();
        let mut present = Vec::new();
        for (path, bytes) in files {
            if git_task_cancelled() {
                return Err(git_process_error(
                    BoundedProcessError::Cancelled,
                    &["history"],
                ));
            }
            if let Some(bytes) = bytes {
                let temporary = directory.0.join(format!("blob-{}", present.len()));
                fs::write(&temporary, bytes).map_err(|error| history_error(error.to_string()))?;
                input_paths.push_str(
                    &serde_json::to_string(&temporary.to_string_lossy())
                        .map_err(|error| history_error(error.to_string()))?,
                );
                input_paths.push('\n');
                present.push(path);
            }
        }
        if !present.is_empty() {
            let hashes = run(
                &["hash-object", "-w", "--no-filters", "--stdin-paths"],
                Some(input_paths.into_bytes()),
            )?;
            let hashes = hashes.lines().collect::<Vec<_>>();
            if hashes.len() != present.len() {
                return Err(history_error("Incomplete history object capture."));
            }
            let mut entries = Vec::new();
            for (path, hash) in present.iter().zip(hashes) {
                entries.extend_from_slice(format!("100644 {hash}\t{path}\0").as_bytes());
            }
            run(&["update-index", "-z", "--index-info"], Some(entries))?;
        }
        let tree = run(&["write-tree"], None)?;
        let existing = run(&["rev-parse", "--verify", "--quiet", &reference], None).ok();
        if let Some(existing) = existing {
            if run(&["rev-parse", &format!("{existing}^{{tree}}")], None)? != tree {
                return Err(history_error(
                    "This operation already has a different history snapshot.",
                ));
            }
            let ancestry = run(&["rev-list", "--parents", "-n", "1", &existing], None)?;
            let parents = ancestry.split_whitespace().skip(1).collect::<Vec<_>>();
            if parents != parent.into_iter().collect::<Vec<_>>() {
                return Err(history_error(
                    "This operation already has a different history parent.",
                ));
            }
            return Ok(GitCheckpoint {
                created: false,
                commit_hash: Some(existing),
                message: message.into(),
                purpose: CheckpointPurpose::HighRiskOperation,
                affected_paths: files.keys().cloned().collect(),
            });
        }
        let mut args = vec!["commit-tree", tree.as_str()];
        if let Some(parent) = parent {
            args.extend(["-p", parent]);
        }
        let commit = run(&args, Some(message.as_bytes().to_vec()))?;
        // The empty old value is a create-only compare-and-swap on our ref.
        run(&["update-ref", &reference, &commit, ""], None)?;
        Ok(GitCheckpoint {
            created: true,
            commit_hash: Some(commit),
            message: message.into(),
            purpose: if phase == "after" {
                CheckpointPurpose::FinalResult
            } else {
                CheckpointPurpose::HighRiskOperation
            },
            affected_paths: files.keys().cloned().collect(),
        })
    }

    /// Completion publishes the already durable plan; it does not re-read
    /// project files or hash the same output a second time.
    pub fn publish_planned_history(
        &self,
        context: &ProjectContext,
        task_id: &str,
    ) -> Result<GitCheckpoint, BackendError> {
        let planned = self
            .history_snapshot(context, task_id, "planned")?
            .ok_or_else(|| history_error("The planned recovery snapshot is unavailable."))?;
        let existing = self.history_snapshot(context, task_id, "after")?;
        if existing.as_ref().is_some_and(|commit| commit != &planned) {
            return Err(history_error(
                "The operation already has a different published result.",
            ));
        }
        if existing.is_none() {
            run_git(
                context,
                &["update-ref", &history_ref(task_id, "after")?, &planned, ""],
            )?;
        }
        Ok(GitCheckpoint {
            created: existing.is_none(),
            commit_hash: Some(planned),
            message: format!("Update Wiki {task_id}"),
            purpose: CheckpointPurpose::FinalResult,
            affected_paths: Vec::new(),
        })
    }

    /// The publisher calls this under the same project write permit as undo.
    /// UI preparation does not enumerate history or block on this check.
    pub fn ensure_no_incomplete_history_undo(
        &self,
        context: &ProjectContext,
    ) -> Result<(), BackendError> {
        if !validate_git_marker(&context.root)? {
            return Ok(());
        }
        let refs = run_git(
            context,
            &[
                "for-each-ref",
                "--format=%(refname)",
                "refs/llm-wiki/operations/",
            ],
        )?;
        let refs = refs.lines().collect::<HashSet<_>>();
        if refs.iter().any(|reference| {
            reference
                .strip_suffix("/undo-started")
                .is_some_and(|base| !refs.contains(format!("{base}/undo").as_str()))
        }) {
            return Err(BackendError::new("WORKFLOW_RECOVERY_REQUIRED", "Finish restoring the interrupted Wiki update in its task history before starting another update.", true, true));
        }
        Ok(())
    }

    pub fn history_snapshot(
        &self,
        context: &ProjectContext,
        task_id: &str,
        phase: &str,
    ) -> Result<Option<String>, BackendError> {
        let reference = history_ref(task_id, phase)?;
        if !validate_git_marker(&context.root)? {
            return Ok(None);
        }
        let output = run_git_process(
            context,
            &["rev-parse", "--verify", "--quiet", &reference],
            Duration::from_secs(5),
            4096,
            git_task_cancelled,
        )
        .map_err(|error| git_process_error(error, &["history", "read"]))?;
        if output.status.success() {
            Ok(Some(
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
            ))
        } else if output.status.code() == Some(1) {
            Ok(None)
        } else {
            Err(git_command_error(&output.stderr, &["history", "read"]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_keeps_head_index_and_exact_dirty_bytes_untouched() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("history", root.path().to_path_buf());
        fs::create_dir_all(root.path().join("wiki/中文")).unwrap();
        fs::write(root.path().join("wiki/中文/page.md"), b"original\n").unwrap();
        fs::write(root.path().join("unrelated.md"), b"original\n").unwrap();
        GitService
            .initialize_repository(&context, "initial")
            .unwrap();
        let head = run_git(&context, &["rev-parse", "HEAD"]).unwrap();
        fs::write(root.path().join("unrelated.md"), b"staged\n").unwrap();
        run_git(&context, &["add", "unrelated.md"]).unwrap();
        let index = fs::read(root.path().join(".git/index")).unwrap();
        fs::write(root.path().join("wiki/中文/page.md"), b"user edit\r\n").unwrap();
        let paths = vec!["wiki/中文/page.md".into(), "wiki/new.md".into()];
        let bytes = GitService
            .capture_history_files(&context, &paths, None)
            .unwrap();
        let task = uuid::Uuid::new_v4().to_string();
        let before = GitService
            .create_history_snapshot(&context, &task, "before", "before", None, &bytes)
            .unwrap();
        let hash = before.commit_hash.unwrap();
        assert_eq!(
            GitService::file_at_checkpoint(&context, &hash, &paths[0])
                .unwrap()
                .as_deref(),
            Some("user edit\r\n")
        );
        assert!(GitService::file_at_checkpoint(&context, &hash, &paths[1])
            .unwrap()
            .is_none());
        assert!(
            GitService::file_at_checkpoint(&context, &hash, "unrelated.md")
                .unwrap()
                .is_none()
        );
        assert_eq!(run_git(&context, &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(fs::read(root.path().join(".git/index")).unwrap(), index);
        assert_eq!(
            fs::read(root.path().join(&paths[0])).unwrap(),
            b"user edit\r\n"
        );
        assert_eq!(
            GitService
                .history_snapshot(&context, &task, "before")
                .unwrap(),
            Some(hash)
        );
        assert!(
            !GitService
                .create_history_snapshot(&context, &task, "before", "retry", None, &bytes)
                .unwrap()
                .created
        );
    }

    #[test]
    fn history_initializes_only_local_objects_and_preserves_absence() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("history", root.path().to_path_buf());
        fs::write(root.path().join("untouched.md"), "private unselected").unwrap();
        let files = BTreeMap::from([("wiki/new.md".into(), None)]);
        let task = uuid::Uuid::new_v4().to_string();
        let checkpoint = GitService
            .create_history_snapshot(&context, &task, "before", "before", None, &files)
            .unwrap();
        assert!(checkpoint.created);
        assert!(run_git(&context, &["rev-parse", "--verify", "HEAD"]).is_err());
        assert!(!root.path().join(".git/index").exists());
        assert_eq!(
            fs::read_to_string(root.path().join("untouched.md")).unwrap(),
            "private unselected"
        );
    }
    #[test]
    fn history_round_trips_more_than_the_generic_git_output_limit() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("history", root.path().to_path_buf());
        let bytes = vec![b'x'; MAX_GIT_OUTPUT_BYTES + 1024];
        let path = "wiki/large.md".to_string();
        let files = BTreeMap::from([(path.clone(), Some(bytes))]);
        let task = uuid::Uuid::new_v4().to_string();
        let hash = GitService
            .create_history_snapshot(&context, &task, "planned", "large", None, &files)
            .unwrap()
            .commit_hash
            .unwrap();
        assert_eq!(
            GitService
                .read_history_files(&context, &hash, &[path])
                .unwrap(),
            files
        );
    }

    #[test]
    fn history_does_not_initialize_an_unborn_repository_inside_external_git() {
        let parent = tempfile::tempdir().unwrap();
        let outer = ProjectContext::new("outer", parent.path().to_path_buf());
        run_git(&outer, &["init", "--quiet"]).unwrap();
        let child = parent.path().join("资料库");
        fs::create_dir(&child).unwrap();
        let context = ProjectContext::new("nested", child.clone());
        let task = uuid::Uuid::new_v4().to_string();
        assert!(GitService
            .create_history_snapshot(&context, &task, "before", "before", None, &BTreeMap::new())
            .is_err());
        assert!(!child.join(".git").exists());
    }
}
