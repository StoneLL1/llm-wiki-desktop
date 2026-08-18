const MAX_CONVENIENCE_FILES: usize = 3;
const MAX_CONVENIENCE_CHANGED_CHARS: usize = 2000;
const MAX_CANDIDATE_INPUT_FILES: usize = 16;
const MAX_CANDIDATE_INPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_CANDIDATE_OUTPUT_ENTRIES: usize = 256;
const MAX_CANDIDATE_OUTPUT_DEPTH: usize = 16;
const MAX_CANDIDATE_OUTPUT_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_CANDIDATE_OUTPUT_TOTAL_BYTES: u64 = 4 * 1024 * 1024;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::app_state::ProjectWritePermit;
use crate::errors::BackendError;
use crate::models::git::{GitChangedFile, GitChangedFileKind};
use crate::models::paths::ProjectContext;
use crate::services::{FileStore, GitService, WriteMode};
use crate::utils::path_safety::validate_existing_project_file;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatIntent {
    ReadOnly,
    Write,
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvenienceAuditStatus {
    Passed,
    SoftViolation,
    HardViolation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangedFileKind {
    Modified,
    Added,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub kind: ChangedFileKind,
    pub changed_chars: usize,
}

impl ChangedFile {
    pub fn modified(path: impl Into<String>, changed_chars: usize) -> Self {
        Self {
            path: path.into(),
            kind: ChangedFileKind::Modified,
            changed_chars,
        }
    }

    pub fn added(path: impl Into<String>, changed_chars: usize) -> Self {
        Self {
            path: path.into(),
            kind: ChangedFileKind::Added,
            changed_chars,
        }
    }

    pub fn deleted(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            kind: ChangedFileKind::Deleted,
            changed_chars: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvenienceAuditReport {
    pub status: ConvenienceAuditStatus,
    pub affected_paths: Vec<String>,
    pub diff_summary: String,
    pub violation_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CandidateChange {
    pub audit: ChangedFile,
    pub content: Option<String>,
    pub expected_hash: Option<String>,
    pub original_content: Option<String>,
}

/// Task-owned workspace used for both no-tool Chat and convenience edits.
/// Dropping it discards all Agent-created files without touching the project.
pub struct ChatTaskWorkspace {
    root: PathBuf,
    baselines: BTreeMap<String, (String, String)>,
}

impl ChatTaskWorkspace {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn collect_changes(&self) -> Result<Vec<CandidateChange>, BackendError> {
        collect_candidate_changes(&self.root, &self.baselines)
    }
}

impl Drop for ChatTaskWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[derive(Default)]
pub struct ChatConvenienceService;

impl ChatConvenienceService {
    pub fn classify_chat_intent(&self, input: &str) -> ChatIntent {
        classify_chat_intent(input)
    }

    pub fn audit_changed_paths(&self, changes: Vec<ChangedFile>) -> ConvenienceAuditReport {
        audit_changed_paths(changes)
    }

    pub fn audit_git_changes(&self, changes: Vec<GitChangedFile>) -> ConvenienceAuditReport {
        audit_git_changes(changes)
    }

    pub fn convenience_prompt_suffix(&self) -> &'static str {
        convenience_prompt_suffix()
    }

    pub fn prepare_read_only_workspace(&self) -> Result<ChatTaskWorkspace, BackendError> {
        create_task_workspace("chat-read")
    }

    pub fn prepare_candidate_workspace(
        &self,
        context: &ProjectContext,
        source_paths: impl IntoIterator<Item = String>,
    ) -> Result<ChatTaskWorkspace, BackendError> {
        let mut workspace = create_task_workspace("chat-edit")?;
        let mut paths = BTreeSet::from(["wiki/index.md".to_string()]);
        paths.extend(source_paths);
        if paths.len() > MAX_CANDIDATE_INPUT_FILES {
            return Err(candidate_error(
                "CHAT_CANDIDATE_CONTEXT_TOO_LARGE",
                "Chat candidate context contains too many Markdown files.",
            ));
        }

        let mut total_bytes = 0usize;
        for path in paths {
            let path = validated_wiki_markdown_path(&path)?;
            let source = context.root.join(Path::new(&path));
            if !source.exists() {
                continue;
            }
            let source =
                validate_existing_project_file(&context.root, &source).map_err(|error| {
                    candidate_error("CHAT_CANDIDATE_SOURCE_UNSAFE", &error)
                        .with_details(serde_json::json!({ "path": path }))
                })?;
            let remaining = MAX_CANDIDATE_INPUT_BYTES.saturating_sub(total_bytes);
            let mut bytes = Vec::new();
            fs::File::open(&source)
                .map_err(|error| {
                    candidate_error("CHAT_CANDIDATE_SOURCE_READ_FAILED", &error.to_string())
                })?
                .take(remaining.saturating_add(1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|error| {
                    candidate_error("CHAT_CANDIDATE_SOURCE_READ_FAILED", &error.to_string())
                })?;
            if bytes.len() > remaining {
                return Err(candidate_error(
                    "CHAT_CANDIDATE_CONTEXT_TOO_LARGE",
                    "Chat candidate context exceeds the raw byte limit.",
                ));
            }
            total_bytes = total_bytes.saturating_add(bytes.len());
            let content = String::from_utf8(bytes).map_err(|_| {
                candidate_error(
                    "CHAT_CANDIDATE_SOURCE_INVALID_UTF8",
                    "Chat candidate inputs must be UTF-8 Markdown.",
                )
            })?;
            let target = workspace.root.join(Path::new(&path));
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    candidate_error("CHAT_CANDIDATE_CREATE_FAILED", &error.to_string())
                })?;
            }
            fs::write(&target, content.as_bytes()).map_err(|error| {
                candidate_error("CHAT_CANDIDATE_CREATE_FAILED", &error.to_string())
            })?;
            workspace
                .baselines
                .insert(path, (FileStore.content_hash(content.as_bytes()), content));
        }
        Ok(workspace)
    }

    pub fn apply_candidate_changes(
        &self,
        permit: &ProjectWritePermit<'_>,
        git_service: &GitService,
        changes: &[CandidateChange],
    ) -> Result<(), BackendError> {
        let context = permit.context();
        for change in changes {
            match (&change.content, &change.expected_hash) {
                (Some(_), Some(expected)) => {
                    let actual = FileStore.file_hash(context, &change.audit.path)?;
                    if &actual != expected {
                        return Err(candidate_changed_error(&change.audit.path));
                    }
                }
                (Some(_), None) if FileStore.exists(context, &change.audit.path) => {
                    return Err(candidate_changed_error(&change.audit.path));
                }
                (None, _) => {
                    return Err(candidate_error(
                        "CHAT_CANDIDATE_DELETE_REJECTED",
                        "Chat convenience candidates cannot delete project files.",
                    ));
                }
                _ => {}
            }
        }
        for (index, change) in changes.iter().enumerate() {
            let content = change.content.as_deref().ok_or_else(|| {
                candidate_error(
                    "CHAT_CANDIDATE_DELETE_REJECTED",
                    "Chat convenience candidates cannot delete project files.",
                )
            })?;
            let mode = change
                .expected_hash
                .clone()
                .map(WriteMode::OverwriteIfHashMatches)
                .unwrap_or(WriteMode::CreateNew);
            if let Err(error) =
                FileStore.write_markdown_checked(context, &change.audit.path, content, mode)
            {
                return match self.compensate_applied_candidate_changes(
                    permit,
                    git_service,
                    &changes[..index],
                ) {
                    Ok(()) => Err(error),
                    Err(cleanup) => Err(candidate_error(
                        "CHAT_CANDIDATE_APPLY_CLEANUP_FAILED",
                        &format!(
                            "Chat candidate apply failed and exact compensation also failed: apply={}; cleanup={}",
                            error.message, cleanup.message
                        ),
                    )),
                };
            }
        }
        Ok(())
    }

    pub fn compensate_applied_candidate_changes(
        &self,
        permit: &ProjectWritePermit<'_>,
        git_service: &GitService,
        changes: &[CandidateChange],
    ) -> Result<(), BackendError> {
        let context = permit.context();
        for change in changes {
            let content = change.content.as_deref().ok_or_else(|| {
                candidate_error(
                    "CHAT_CANDIDATE_COMPENSATION_INVALID",
                    "A non-applied candidate deletion cannot be compensated.",
                )
            })?;
            let applied_hash = FileStore.content_hash(content.as_bytes());
            let actual_hash = FileStore.file_hash(context, &change.audit.path)?;
            if actual_hash != applied_hash {
                return Err(candidate_error(
                    "CHAT_CANDIDATE_COMPENSATION_STALE",
                    "A Chat candidate path changed after apply and requires manual review.",
                )
                .with_details(serde_json::json!({ "path": change.audit.path })));
            }
        }

        for change in changes
            .iter()
            .filter(|change| change.original_content.is_some())
        {
            let content = change.content.as_deref().ok_or_else(|| {
                candidate_error(
                    "CHAT_CANDIDATE_COMPENSATION_INVALID",
                    "A non-applied candidate deletion cannot be compensated.",
                )
            })?;
            let original_content = change.original_content.as_deref().ok_or_else(|| {
                candidate_error(
                    "CHAT_CANDIDATE_COMPENSATION_INVALID",
                    "A modified candidate is missing its compensation baseline.",
                )
            })?;
            FileStore.write_markdown_checked(
                context,
                &change.audit.path,
                original_content,
                WriteMode::OverwriteIfHashMatches(FileStore.content_hash(content.as_bytes())),
            )?;
        }
        let added_paths = changes
            .iter()
            .filter(|change| change.original_content.is_none())
            .map(|change| change.audit.path.clone())
            .collect::<Vec<_>>();
        if !added_paths.is_empty() {
            git_service.rollback_paths_to_head_preserving_ignored(context, &added_paths, &[])?;
        }
        Ok(())
    }
}

pub fn classify_chat_intent(input: &str) -> ChatIntent {
    let normalized = input.trim().to_lowercase();
    if normalized.is_empty() {
        return ChatIntent::Ambiguous;
    }

    if starts_with_write_command(&normalized) {
        return ChatIntent::Write;
    }

    if starts_with_read_only_question(&normalized) {
        return ChatIntent::ReadOnly;
    }

    let has_write = contains_any(
        &normalized,
        &[
            "保存", "存成", "写入", "写成", "新建", "创建", "新增", "修改", "更新", "编辑", "改写",
            "重写", "整理", "补", "添加", "加入", "删除", "移除", "save", "write", "edit",
            "update", "create", "add", "append", "delete", "remove", "rewrite",
        ],
    );
    if has_write && has_write_request_cue(&normalized) {
        return ChatIntent::Write;
    }

    if contains_read_only_cue(&normalized) {
        return ChatIntent::ReadOnly;
    }

    ChatIntent::Ambiguous
}

pub fn audit_git_changes(changes: Vec<GitChangedFile>) -> ConvenienceAuditReport {
    audit_changed_paths(
        changes
            .into_iter()
            .map(|change| ChangedFile {
                path: change.path,
                kind: match change.kind {
                    GitChangedFileKind::Added => ChangedFileKind::Added,
                    GitChangedFileKind::Modified => ChangedFileKind::Modified,
                    GitChangedFileKind::Deleted | GitChangedFileKind::Renamed => {
                        ChangedFileKind::Deleted
                    }
                },
                changed_chars: change.changed_chars,
            })
            .collect(),
    )
}

pub fn audit_changed_paths(changes: Vec<ChangedFile>) -> ConvenienceAuditReport {
    let affected_paths: Vec<String> = changes
        .iter()
        .map(|change| normalize_project_path(&change.path))
        .collect();

    if changes.is_empty() {
        return ConvenienceAuditReport {
            status: ConvenienceAuditStatus::SoftViolation,
            diff_summary: summarize_changes(0, &affected_paths),
            affected_paths,
            violation_reason: Some("Convenience edit produced no file changes.".to_string()),
        };
    }

    if let Some(reason) = hard_violation_reason(&changes) {
        return ConvenienceAuditReport {
            status: ConvenienceAuditStatus::HardViolation,
            diff_summary: summarize_changes(changes.len(), &affected_paths),
            affected_paths,
            violation_reason: Some(reason),
        };
    }

    if changes.len() > MAX_CONVENIENCE_FILES {
        return ConvenienceAuditReport {
            status: ConvenienceAuditStatus::SoftViolation,
            diff_summary: summarize_changes(changes.len(), &affected_paths),
            affected_paths,
            violation_reason: Some(format!(
                "Convenience edits are limited to {MAX_CONVENIENCE_FILES} wiki Markdown files."
            )),
        };
    }

    if changes
        .iter()
        .any(|change| change.changed_chars > MAX_CONVENIENCE_CHANGED_CHARS)
    {
        return ConvenienceAuditReport {
            status: ConvenienceAuditStatus::SoftViolation,
            diff_summary: summarize_changes(changes.len(), &affected_paths),
            affected_paths,
            violation_reason: Some(format!(
                "Convenience edits are limited to {MAX_CONVENIENCE_CHANGED_CHARS} changed characters per file."
            )),
        };
    }

    ConvenienceAuditReport {
        status: ConvenienceAuditStatus::Passed,
        diff_summary: summarize_changes(changes.len(), &affected_paths),
        affected_paths,
        violation_reason: None,
    }
}

pub fn convenience_prompt_suffix() -> &'static str {
    "\n\nConvenience edit policy:\n\
     - Write only small Markdown edits under wiki/.\n\
     - Never delete files.\n\
     - Never modify raw/sources/ or .app/settings.json or .app/agent-config.json.\n\
     - Change at most 3 wiki Markdown files and at most 2000 characters per file.\n\
     - The current directory is a task-owned candidate snapshot; never read or write outside it.\n\
     - Do not access HOME, SSH files, cloud credentials, or any path outside the candidate.\n\
     - Do not install packages, download binaries, or run remote scripts."
}

fn create_task_workspace(prefix: &str) -> Result<ChatTaskWorkspace, BackendError> {
    let candidate_root = std::env::temp_dir().join("llm-wiki-desktop");
    ensure_private_candidate_parent(&candidate_root)?;
    let root = candidate_root.join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root)
        .map_err(|error| candidate_error("CHAT_CANDIDATE_CREATE_FAILED", &error.to_string()))?;
    validate_private_candidate_directory(&root)?;
    fs::create_dir(root.join("wiki"))
        .map_err(|error| candidate_error("CHAT_CANDIDATE_CREATE_FAILED", &error.to_string()))?;
    Ok(ChatTaskWorkspace {
        root,
        baselines: BTreeMap::new(),
    })
}

fn ensure_private_candidate_parent(path: &Path) -> Result<(), BackendError> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(candidate_error(
                "CHAT_CANDIDATE_CREATE_FAILED",
                &error.to_string(),
            ))
        }
    }
    validate_private_candidate_directory(path)
}

fn validate_private_candidate_directory(path: &Path) -> Result<(), BackendError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| candidate_error("CHAT_CANDIDATE_CREATE_FAILED", &error.to_string()))?;
    if !metadata.is_dir() || candidate_metadata_is_link(&metadata) {
        return Err(candidate_error(
            "CHAT_CANDIDATE_ROOT_UNSAFE",
            "Chat candidate storage must be a real private directory.",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| candidate_error("CHAT_CANDIDATE_CREATE_FAILED", &error.to_string()))?;
    }
    Ok(())
}

fn validated_wiki_markdown_path(path: &str) -> Result<String, BackendError> {
    let normalized = normalize_project_path(path);
    if normalized != path.trim().replace('\\', "/")
        || !normalized.starts_with("wiki/")
        || !normalized.ends_with(".md")
    {
        return Err(candidate_error(
            "CHAT_CANDIDATE_SOURCE_UNSAFE",
            "Chat candidate inputs must be normalized Markdown paths under wiki/.",
        )
        .with_details(serde_json::json!({ "path": path })));
    }
    Ok(normalized)
}

fn collect_candidate_changes(
    root: &Path,
    baselines: &BTreeMap<String, (String, String)>,
) -> Result<Vec<CandidateChange>, BackendError> {
    let mut current = BTreeMap::new();
    collect_candidate_files(root, root, &mut current)?;
    let mut paths = baselines.keys().cloned().collect::<BTreeSet<_>>();
    paths.extend(current.keys().cloned());
    let mut changes = Vec::new();
    for path in paths {
        let baseline = baselines.get(&path);
        let candidate = current.get(&path);
        match (baseline, candidate) {
            (Some((expected_hash, original)), Some(content)) if original != content => {
                changes.push(CandidateChange {
                    audit: ChangedFile::modified(path, changed_character_count(original, content)),
                    content: Some(content.clone()),
                    expected_hash: Some(expected_hash.clone()),
                    original_content: Some(original.clone()),
                });
            }
            (Some((expected_hash, original)), None) => changes.push(CandidateChange {
                audit: ChangedFile::deleted(path),
                content: None,
                expected_hash: Some(expected_hash.clone()),
                original_content: Some(original.clone()),
            }),
            (None, Some(content)) => changes.push(CandidateChange {
                audit: ChangedFile::added(path, content.chars().count()),
                content: Some(content.clone()),
                expected_hash: None,
                original_content: None,
            }),
            _ => {}
        }
    }
    Ok(changes)
}

fn collect_candidate_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), BackendError> {
    let mut pending = vec![(directory.to_path_buf(), 0usize)];
    let mut entry_count = 0usize;
    let mut total_bytes = 0u64;
    while let Some((directory, depth)) = pending.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| candidate_error("CHAT_CANDIDATE_AUDIT_FAILED", &error.to_string()))?
        {
            let entry = entry.map_err(|error| {
                candidate_error("CHAT_CANDIDATE_AUDIT_FAILED", &error.to_string())
            })?;
            entry_count = entry_count.saturating_add(1);
            if entry_count > MAX_CANDIDATE_OUTPUT_ENTRIES {
                return Err(candidate_output_limit_error(
                    "Chat candidate output contains too many files or directories.",
                ));
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                candidate_error("CHAT_CANDIDATE_AUDIT_FAILED", &error.to_string())
            })?;
            if candidate_metadata_is_link(&metadata) {
                return Err(candidate_error(
                    "CHAT_CANDIDATE_LINK_REJECTED",
                    "Chat candidate output cannot contain links or reparse points.",
                ));
            }
            if metadata.is_dir() {
                let relative = project_relative_path(root, &path)?;
                if matches!(relative.as_str(), "runtime-home" | "runtime-temp") {
                    continue;
                }
                if depth >= MAX_CANDIDATE_OUTPUT_DEPTH {
                    return Err(candidate_output_limit_error(
                        "Chat candidate output directory depth exceeds the audit limit.",
                    ));
                }
                pending.push((path, depth + 1));
            } else if metadata.is_file() {
                if metadata.len() > MAX_CANDIDATE_OUTPUT_FILE_BYTES {
                    return Err(candidate_output_limit_error(
                        "A Chat candidate output file exceeds the raw byte limit.",
                    ));
                }
                let relative = project_relative_path(root, &path)?;
                let mut bytes = Vec::new();
                fs::File::open(&path)
                    .map_err(|error| {
                        candidate_error("CHAT_CANDIDATE_AUDIT_FAILED", &error.to_string())
                    })?
                    .take(MAX_CANDIDATE_OUTPUT_FILE_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|error| {
                        candidate_error("CHAT_CANDIDATE_AUDIT_FAILED", &error.to_string())
                    })?;
                if bytes.len() as u64 > MAX_CANDIDATE_OUTPUT_FILE_BYTES {
                    return Err(candidate_output_limit_error(
                        "A Chat candidate output file exceeds the raw byte limit.",
                    ));
                }
                total_bytes = total_bytes.saturating_add(bytes.len() as u64);
                if total_bytes > MAX_CANDIDATE_OUTPUT_TOTAL_BYTES {
                    return Err(candidate_output_limit_error(
                        "Chat candidate output exceeds the total raw byte limit.",
                    ));
                }
                let content = String::from_utf8(bytes).map_err(|_| {
                    candidate_error(
                        "CHAT_CANDIDATE_OUTPUT_INVALID",
                        "Chat candidate output must contain only UTF-8 text files.",
                    )
                })?;
                files.insert(relative, content);
            } else {
                return Err(candidate_error(
                    "CHAT_CANDIDATE_OUTPUT_INVALID",
                    "Chat candidate output contains an unsupported file type.",
                ));
            }
        }
    }
    Ok(())
}

fn candidate_output_limit_error(message: &str) -> BackendError {
    candidate_error("CHAT_CANDIDATE_OUTPUT_TOO_LARGE", message)
}

fn project_relative_path(root: &Path, path: &Path) -> Result<String, BackendError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        candidate_error(
            "CHAT_CANDIDATE_OUTPUT_OUTSIDE_WORKSPACE",
            "Chat candidate output escaped its task workspace.",
        )
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn changed_character_count(before: &str, after: &str) -> usize {
    let shared = before
        .chars()
        .zip(after.chars())
        .filter(|(left, right)| left != right)
        .count();
    shared + before.chars().count().abs_diff(after.chars().count())
}

fn candidate_changed_error(path: &str) -> BackendError {
    candidate_error(
        "CHAT_CANDIDATE_TARGET_CHANGED",
        "A Chat convenience target changed after the candidate snapshot was created.",
    )
    .with_details(serde_json::json!({ "path": path }))
}

fn candidate_error(code: &'static str, message: &str) -> BackendError {
    BackendError::new(code, message, true, true)
}

#[cfg(windows)]
fn candidate_metadata_is_link(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn candidate_metadata_is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn contains_any(input: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| input.contains(needle))
}

fn starts_with_write_command(input: &str) -> bool {
    [
        "please save",
        "please write",
        "please edit",
        "please update",
        "please create",
        "please add",
        "please append",
        "please delete",
        "please remove",
        "please rewrite",
        "save ",
        "write ",
        "edit ",
        "update ",
        "create ",
        "add ",
        "append ",
        "delete ",
        "remove ",
        "rewrite ",
        "保存",
        "存成",
        "写入",
        "写成",
        "新建",
        "创建",
        "新增",
        "修改",
        "更新",
        "编辑",
        "改写",
        "重写",
        "整理",
        "补",
        "添加",
        "加入",
        "删除",
        "移除",
    ]
    .iter()
    .any(|prefix| input.starts_with(prefix))
}

fn starts_with_read_only_question(input: &str) -> bool {
    [
        "can i ",
        "could i ",
        "should i ",
        "do i ",
        "is it ",
        "how ",
        "why ",
        "what ",
        "explain ",
        "analyze ",
        "review ",
        "inspect ",
        "summarize ",
        "can you explain",
        "could you explain",
        "please explain",
        "分析",
        "解释",
        "说明",
        "看看",
        "看一下",
        "检查",
        "评价",
        "评估",
        "为什么",
        "是什么",
        "怎么样",
        "如何",
        "怎样",
        "怎么",
        "该如何",
        "我该如何",
    ]
    .iter()
    .any(|prefix| input.starts_with(prefix))
}

fn has_write_request_cue(input: &str) -> bool {
    [
        "please ",
        "help me ",
        "can you ",
        "could you ",
        "would you ",
        "请",
        "帮我",
        "替我",
        "麻烦",
        "把",
    ]
    .iter()
    .any(|cue| input.contains(cue))
}

fn contains_read_only_cue(input: &str) -> bool {
    contains_any(input, &["看看", "看一下", "解释一下", "分析一下"])
}

fn hard_violation_reason(changes: &[ChangedFile]) -> Option<String> {
    for change in changes {
        let path = normalize_project_path(&change.path);

        if matches!(change.kind, ChangedFileKind::Deleted) {
            return Some(format!("Convenience edits cannot delete files: {path}"));
        }
        if path.starts_with("raw/sources/") {
            return Some(format!(
                "Convenience edits cannot modify raw sources: {path}"
            ));
        }
        if matches!(
            path.as_str(),
            ".app/settings.json" | ".app/agent-config.json"
        ) {
            return Some(format!(
                "Convenience edits cannot modify protected app config: {path}"
            ));
        }
        if !path.starts_with("wiki/") {
            return Some(format!(
                "Convenience edits must stay under wiki Markdown files: {path}"
            ));
        }
        if !path.ends_with(".md") {
            return Some(format!(
                "Convenience edits can only modify Markdown files under wiki/: {path}"
            ));
        }
    }
    None
}

fn normalize_project_path(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/");
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    parts.push("..");
                }
            }
            value => parts.push(value),
        }
    }
    parts.join("/")
}

fn summarize_changes(count: usize, affected_paths: &[String]) -> String {
    if count == 0 {
        return "No file changes.".to_string();
    }
    let preview = affected_paths
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<String>>()
        .join(", ");
    if count <= 3 {
        format!("{count} file change(s): {preview}")
    } else {
        format!("{count} file change(s): {preview}, ...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::git::{GitChangedFile, GitChangedFileKind};

    fn candidate_project(label: &str) -> (ProjectContext, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "llm-wiki-chat-candidate-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join("wiki/private")).unwrap();
        fs::write(root.join("wiki/index.md"), "# Index").unwrap();
        fs::write(root.join("wiki/selected.md"), "# Selected").unwrap();
        fs::write(root.join("wiki/private/sentinel.md"), "PRIVATE SENTINEL").unwrap();
        (ProjectContext::new("project-1", root.clone()), root)
    }

    #[test]
    fn classifies_read_only_write_and_ambiguous_intents() {
        assert_eq!(
            classify_chat_intent("分析一下这个页面的问题"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("帮我整理这一页并补摘要"),
            ChatIntent::Write
        );
        assert_eq!(
            classify_chat_intent("save this answer as a page"),
            ChatIntent::Write
        );
        assert_eq!(
            classify_chat_intent("这页有点乱，帮我看看"),
            ChatIntent::ReadOnly
        );
        assert_eq!(classify_chat_intent("帮我处理一下"), ChatIntent::Ambiguous);
        assert_eq!(
            classify_chat_intent("how do I add a page?"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("explain how to delete stale notes"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("what changed in this update?"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("how do I update this page?"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("why did you delete the note?"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("can I add a page?"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("should I delete this note?"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("如何更新这个页面？"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("怎样修改这一页？"),
            ChatIntent::ReadOnly
        );
        assert_eq!(
            classify_chat_intent("can you update this page?"),
            ChatIntent::Write
        );
        assert_eq!(
            classify_chat_intent("please update this page"),
            ChatIntent::Write
        );
        assert_eq!(
            classify_chat_intent("please update this page and explain what changed"),
            ChatIntent::Write
        );
        assert_eq!(
            classify_chat_intent("update this page, then summarize why"),
            ChatIntent::Write
        );
    }

    #[test]
    fn audit_accepts_three_small_wiki_markdown_changes() {
        let report = audit_changed_paths(vec![
            ChangedFile::modified("wiki/a.md", 100),
            ChangedFile::modified("wiki/index.md", 100),
            ChangedFile::modified("wiki/log.md", 100),
        ]);
        assert_eq!(report.status, ConvenienceAuditStatus::Passed);
    }

    #[test]
    fn audit_soft_violates_large_or_many_wiki_changes() {
        let many = audit_changed_paths(vec![
            ChangedFile::modified("wiki/a.md", 10),
            ChangedFile::modified("wiki/b.md", 10),
            ChangedFile::modified("wiki/c.md", 10),
            ChangedFile::modified("wiki/d.md", 10),
        ]);
        assert_eq!(many.status, ConvenienceAuditStatus::SoftViolation);

        let large = audit_changed_paths(vec![ChangedFile::modified("wiki/a.md", 2001)]);
        assert_eq!(large.status, ConvenienceAuditStatus::SoftViolation);
    }

    #[test]
    fn audit_hard_violates_delete_raw_config_and_outside_wiki() {
        for change in [
            ChangedFile::deleted("wiki/a.md"),
            ChangedFile::modified("raw/sources/pdfs/a.pdf", 10),
            ChangedFile::modified(".app/settings.json", 10),
            ChangedFile::modified("purpose.md", 10),
        ] {
            let report = audit_changed_paths(vec![change]);
            assert_eq!(report.status, ConvenienceAuditStatus::HardViolation);
        }
    }

    #[test]
    fn audit_hard_violates_dot_segments_case_variants_and_non_markdown() {
        for change in [
            ChangedFile::modified("wiki/../raw/sources/a.md", 10),
            ChangedFile::modified("WIKI/a.md", 10),
            ChangedFile::modified("wiki/assets/a.png", 10),
            ChangedFile::modified("wiki\\..\\raw\\sources\\a.md", 10),
        ] {
            let report = audit_changed_paths(vec![change]);
            assert_eq!(report.status, ConvenienceAuditStatus::HardViolation);
        }
    }

    #[test]
    fn audit_soft_violates_empty_change_sets() {
        let report = audit_changed_paths(Vec::new());
        assert_eq!(report.status, ConvenienceAuditStatus::SoftViolation);
        assert_eq!(report.affected_paths.len(), 0);
    }

    #[test]
    fn audit_git_changes_reuses_convenience_rules() {
        let raw_report = audit_git_changes(vec![GitChangedFile {
            path: "raw/sources/source.md".to_string(),
            kind: GitChangedFileKind::Modified,
            changed_chars: 12,
        }]);
        assert_eq!(raw_report.status, ConvenienceAuditStatus::HardViolation);

        let delete_report = audit_git_changes(vec![GitChangedFile {
            path: "wiki/page.md".to_string(),
            kind: GitChangedFileKind::Deleted,
            changed_chars: 1,
        }]);
        assert_eq!(delete_report.status, ConvenienceAuditStatus::HardViolation);

        let wiki_report = audit_git_changes(vec![GitChangedFile {
            path: "wiki/page.md".to_string(),
            kind: GitChangedFileKind::Modified,
            changed_chars: 40,
        }]);
        assert_eq!(wiki_report.status, ConvenienceAuditStatus::Passed);
    }

    #[test]
    fn candidate_snapshot_copies_only_bounded_selected_markdown() {
        let (context, root) = candidate_project("bounded");
        let workspace = ChatConvenienceService
            .prepare_candidate_workspace(&context, ["wiki/selected.md".to_string()])
            .unwrap();

        assert_eq!(
            fs::read_to_string(workspace.root().join("wiki/selected.md")).unwrap(),
            "# Selected"
        );
        assert!(workspace.root().join("wiki/index.md").is_file());
        assert!(!workspace.root().join("wiki/private/sentinel.md").exists());
        assert_eq!(
            fs::read_to_string(root.join("wiki/private/sentinel.md")).unwrap(),
            "PRIVATE SENTINEL"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_snapshot_rejects_oversized_input_with_a_bounded_read() {
        let (context, root) = candidate_project("oversized-input");
        let oversized = root.join("wiki/oversized.md");
        let file = std::fs::File::create(&oversized).unwrap();
        file.set_len(MAX_CANDIDATE_INPUT_BYTES as u64 + 1).unwrap();

        let error = ChatConvenienceService
            .prepare_candidate_workspace(&context, ["wiki/oversized.md".to_string()])
            .err()
            .expect("oversized candidate input must be rejected");
        assert_eq!(error.code, "CHAT_CANDIDATE_CONTEXT_TOO_LARGE");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_outside_wiki_marker_is_hard_violation_and_never_reaches_project() {
        let (context, root) = candidate_project("outside-marker");
        let workspace = ChatConvenienceService
            .prepare_candidate_workspace(&context, ["wiki/selected.md".to_string()])
            .unwrap();
        fs::write(workspace.root().join("outside-marker.txt"), "attempted").unwrap();

        let changes = workspace.collect_changes().unwrap();
        let report =
            audit_changed_paths(changes.iter().map(|change| change.audit.clone()).collect());
        assert_eq!(report.status, ConvenienceAuditStatus::HardViolation);
        assert_eq!(report.affected_paths, vec!["outside-marker.txt"]);
        assert!(!root.join("outside-marker.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_audit_rejects_oversized_output_before_reading_it() {
        let (context, root) = candidate_project("oversized-output");
        let workspace = ChatConvenienceService
            .prepare_candidate_workspace(&context, ["wiki/selected.md".to_string()])
            .unwrap();
        let oversized = workspace.root().join("wiki/oversized.md");
        let file = std::fs::File::create(&oversized).unwrap();
        file.set_len(MAX_CANDIDATE_OUTPUT_FILE_BYTES + 1).unwrap();

        let error = workspace.collect_changes().unwrap_err();
        assert_eq!(error.code, "CHAT_CANDIDATE_OUTPUT_TOO_LARGE");
        assert!(!root.join("wiki/oversized.md").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_audit_rejects_excessive_directory_depth_iteratively() {
        let (context, root) = candidate_project("deep-output");
        let workspace = ChatConvenienceService
            .prepare_candidate_workspace(&context, ["wiki/selected.md".to_string()])
            .unwrap();
        let mut directory = workspace.root().join("wiki");
        for index in 0..=MAX_CANDIDATE_OUTPUT_DEPTH {
            directory = directory.join(format!("level-{index}"));
            fs::create_dir(&directory).unwrap();
        }

        let error = workspace.collect_changes().unwrap_err();
        assert_eq!(error.code, "CHAT_CANDIDATE_OUTPUT_TOO_LARGE");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn candidate_change_preserves_project_hash_for_checked_apply() {
        let (context, root) = candidate_project("checked-apply");
        let workspace = ChatConvenienceService
            .prepare_candidate_workspace(&context, ["wiki/selected.md".to_string()])
            .unwrap();
        fs::write(
            workspace.root().join("wiki/selected.md"),
            "# Selected\n\nUpdated",
        )
        .unwrap();

        let changes = workspace.collect_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].audit.path, "wiki/selected.md");
        assert_eq!(
            changes[0].expected_hash.as_deref(),
            Some(
                FileStore
                    .file_hash(&context, "wiki/selected.md")
                    .unwrap()
                    .as_str()
            )
        );
        assert_eq!(
            fs::read_to_string(root.join("wiki/selected.md")).unwrap(),
            "# Selected"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
