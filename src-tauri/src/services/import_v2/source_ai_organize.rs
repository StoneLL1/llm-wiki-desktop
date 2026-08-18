use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::errors::BackendError;
use crate::services::import_v2::source_finalization::{parse_final_source, render_source_markdown};
use crate::utils::private_directory::{create_private_directory, ensure_private_directory};

pub const MAX_SOURCE_AI_MARKDOWN_BYTES: usize = 384 * 1024;
pub const MAX_SOURCE_AI_EVIDENCE_BYTES: usize = 128 * 1024;
pub const MAX_SOURCE_AI_MEDIA_REFERENCES: usize = 128;
pub const MAX_SOURCE_AI_MEDIA_REFERENCE_BYTES: usize = 32 * 1024;
pub const MAX_SOURCE_AI_CUSTOM_INSTRUCTION_CHARS: usize = 4_000;
const MAX_SOURCE_AI_OUTPUT_BYTES: usize = 512 * 1024;
const MAX_OVERVIEW_CHARS: usize = 4_000;
const STALE_AGENT_WORKSPACE_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const OVERVIEW_END_MARKER: &str = "<!-- llm-wiki:content-overview-end -->";
const SOURCE_REWRITE_CONTRACT: &str =
    include_str!("../../../templates/skills/source-rewrite/SKILL.md");
pub const SOURCE_AI_OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "overview": { "type": "string" },
    "bodyMarkdown": { "type": "string" }
  },
  "required": ["overview", "bodyMarkdown"]
}"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceAiTextEvidence {
    pub kind: String,
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SourceAiOrganizeInput {
    pub source_id: String,
    pub version_id: String,
    pub markdown_hash: String,
    pub title: String,
    pub source_kind: String,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub language: Option<String>,
    pub current_markdown: String,
    pub retained_text_evidence: Vec<SourceAiTextEvidence>,
    pub media_references: Vec<String>,
    pub custom_instructions: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedSourceOrganization {
    overview: String,
    body_markdown: String,
}

pub fn validate_custom_instructions(
    instructions: Option<&str>,
) -> Result<Option<String>, BackendError> {
    let Some(instructions) = instructions
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    if instructions.chars().count() > MAX_SOURCE_AI_CUSTOM_INSTRUCTION_CHARS {
        return Err(ai_error(
            "SOURCE_AI_INSTRUCTIONS_TOO_LONG",
            "Custom organization instructions are too long.",
            true,
        ));
    }
    Ok(Some(instructions.to_string()))
}

pub fn create_agent_workspace(
    _task_id: &str,
    input: &SourceAiOrganizeInput,
) -> Result<PathBuf, BackendError> {
    let candidate_root = std::env::temp_dir().join("llm-wiki-desktop");
    ensure_private_directory(&candidate_root)
        .map_err(|error| workspace_error("protect", &candidate_root, error))?;
    cleanup_stale_agent_workspaces(
        &candidate_root,
        STALE_AGENT_WORKSPACE_AGE,
        SystemTime::now(),
    );
    let workspace = candidate_root.join(format!("source-ai-{}", uuid::Uuid::new_v4()));
    create_private_directory(&workspace)
        .map_err(|error| workspace_error("create", &workspace, error))?;
    let bytes = serde_json::to_vec_pretty(input).map_err(|error| {
        ai_error(
            "SOURCE_AI_INPUT_INVALID",
            &format!("Source AI input could not be serialized: {error}"),
            false,
        )
    })?;
    let input_path = workspace.join("input.json");
    fs::write(&input_path, bytes).map_err(|error| workspace_error("write", &workspace, error))?;
    restrict_permissions(&input_path, false)
        .map_err(|error| workspace_error("protect", &workspace, error))?;
    let schema_path = workspace.join("output-schema.json");
    fs::write(&schema_path, SOURCE_AI_OUTPUT_SCHEMA.as_bytes())
        .map_err(|error| workspace_error("write", &workspace, error))?;
    restrict_permissions(&schema_path, false)
        .map_err(|error| workspace_error("protect", &workspace, error))?;
    if let Err(error) = register_agent_workspace(&workspace) {
        let _ = fs::remove_dir_all(&workspace);
        return Err(error);
    }
    Ok(workspace)
}

fn cleanup_stale_agent_workspaces(root: &Path, max_age: Duration, now: SystemTime) -> usize {
    let active_workspaces = match active_agent_workspaces().lock() {
        Ok(workspaces) => workspaces.clone(),
        Err(_) => return 0,
    };
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("source-ai-") || name.len() == "source-ai-".len() {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        if active_workspaces.contains(&entry.path()) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age < max_age {
            continue;
        }
        if fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

fn active_agent_workspaces() -> &'static Mutex<HashSet<PathBuf>> {
    static ACTIVE: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn register_agent_workspace(workspace: &Path) -> Result<(), BackendError> {
    active_agent_workspaces()
        .lock()
        .map_err(|_| {
            ai_error(
                "SOURCE_AI_WORKSPACE_FAILED",
                "Could not register the active Source AI workspace.",
                true,
            )
        })?
        .insert(workspace.to_path_buf());
    Ok(())
}

fn unregister_agent_workspace(workspace: &Path) {
    if let Ok(mut workspaces) = active_agent_workspaces().lock() {
        workspaces.remove(workspace);
    }
}

pub fn cleanup_agent_workspace(workspace: &Path) -> Result<(), BackendError> {
    let candidate_root = std::env::temp_dir().join("llm-wiki-desktop");
    if workspace != candidate_root && workspace.starts_with(&candidate_root) && workspace.exists() {
        let result = fs::remove_dir_all(workspace)
            .map_err(|error| workspace_error("remove", workspace, error));
        unregister_agent_workspace(workspace);
        result?;
    } else {
        unregister_agent_workspace(workspace);
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_permissions(path: &Path, directory: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if directory { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path, _directory: bool) -> std::io::Result<()> {
    Ok(())
}

pub fn agent_prompt(input: &SourceAiOrganizeInput, language: &str) -> Result<String, BackendError> {
    let prompt = provider_prompt(input, language)?;
    Ok(format!(
        "All bounded Source input is embedded below. Filesystem and shell tools are unavailable; never attempt to read any local file.\n\n{prompt}"
    ))
}

pub fn provider_prompt(
    input: &SourceAiOrganizeInput,
    language: &str,
) -> Result<String, BackendError> {
    let input_json = serde_json::to_string(input).map_err(|error| {
        ai_error(
            "SOURCE_AI_INPUT_INVALID",
            &format!("Source AI input could not be serialized: {error}"),
            false,
        )
    })?;
    Ok(format!(
        r#"Follow the shared source-rewrite contract below.

<source-rewrite-contract>
{SOURCE_REWRITE_CONTRACT}
</source-rewrite-contract>

<bounded-source-input>
{input_json}
</bounded-source-input>

Return only the required UTF-8 JSON. UI language: {language}."#
    ))
}

pub fn read_agent_result(workspace: &Path, captured: &str) -> Result<String, BackendError> {
    let output = workspace.join("candidate.json");
    match open_candidate_output(&output) {
        Ok(file) => {
            let metadata = file
                .metadata()
                .map_err(|error| workspace_error("inspect", workspace, error))?;
            if !metadata.is_file() || output_metadata_is_link_or_reparse(&metadata) {
                return Err(ai_error(
                    "SOURCE_AI_OUTPUT_INVALID",
                    "Agent candidate output is not a safe bounded regular file.",
                    true,
                ));
            }
            let mut bytes = Vec::with_capacity(
                (metadata.len() as usize).min(MAX_SOURCE_AI_OUTPUT_BYTES.saturating_add(1)),
            );
            file.take((MAX_SOURCE_AI_OUTPUT_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|error| workspace_error("read", workspace, error))?;
            if bytes.len() > MAX_SOURCE_AI_OUTPUT_BYTES {
                return Err(ai_error(
                    "SOURCE_AI_OUTPUT_INVALID",
                    "Agent candidate output is not a safe bounded regular file.",
                    true,
                ));
            }
            return String::from_utf8(bytes).map_err(|_| {
                ai_error(
                    "SOURCE_AI_OUTPUT_INVALID",
                    "Agent candidate output must be valid UTF-8 JSON.",
                    true,
                )
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(ai_error(
                "SOURCE_AI_OUTPUT_INVALID",
                &format!("Agent candidate output could not be opened safely: {error}"),
                true,
            ));
        }
    }
    if captured.len() > MAX_SOURCE_AI_OUTPUT_BYTES {
        return Err(ai_error(
            "SOURCE_AI_OUTPUT_INVALID",
            "Agent candidate output is too large.",
            true,
        ));
    }
    Ok(captured.to_string())
}

#[cfg(unix)]
fn open_candidate_output(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;

    fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

#[cfg(windows)]
fn open_candidate_output(path: &Path) -> std::io::Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(windows)]
fn output_metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn output_metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

pub fn build_candidate_markdown(
    base_markdown: &str,
    title: &str,
    raw_output: &str,
) -> Result<String, BackendError> {
    if raw_output.len() > MAX_SOURCE_AI_OUTPUT_BYTES {
        return Err(ai_error(
            "SOURCE_AI_OUTPUT_INVALID",
            "AI organization output is too large.",
            true,
        ));
    }
    let generated: GeneratedSourceOrganization =
        serde_json::from_str(extract_json_object(raw_output)?).map_err(|_| {
            ai_error(
                "SOURCE_AI_OUTPUT_INVALID",
                "AI organization must return the required candidate JSON.",
                true,
            )
        })?;
    let overview = normalize_overview(&generated.overview)?;
    let organized_body = normalize_body(&generated.body_markdown, title, &overview)?;
    let (mut frontmatter, _) = parse_final_source(base_markdown).map_err(|_| source_invalid())?;
    frontmatter.content_hash = sha256(organized_body.as_bytes());
    let candidate = render_source_markdown(&frontmatter, &organized_body)?;
    validate_exactly_one_overview(&candidate)?;
    Ok(candidate)
}

pub fn validate_exactly_one_overview(markdown: &str) -> Result<(), BackendError> {
    let count = markdown
        .lines()
        .filter(|line| line.trim_end_matches('\r') == "## 内容概览")
        .count();
    if count != 1 {
        return Err(ai_error(
            "SOURCE_AI_OVERVIEW_INVALID",
            "The candidate must contain exactly one `## 内容概览` section.",
            true,
        ));
    }
    let (_, body) = parse_final_source(markdown).map_err(|_| source_invalid())?;
    let mut meaningful = body.lines().filter(|line| !line.trim().is_empty());
    let Some(title) = meaningful.next() else {
        return Err(source_invalid());
    };
    let Some(overview) = meaningful.next() else {
        return Err(source_invalid());
    };
    if !title.starts_with("# ") || overview != "## 内容概览" {
        return Err(ai_error(
            "SOURCE_AI_OVERVIEW_INVALID",
            "The content overview must appear immediately after the Source title.",
            true,
        ));
    }
    Ok(())
}

fn normalize_overview(raw: &str) -> Result<String, BackendError> {
    let overview = raw.trim().replace("\r\n", "\n");
    if overview.is_empty()
        || overview.chars().count() > MAX_OVERVIEW_CHARS
        || overview
            .lines()
            .any(|line| line.trim_start().starts_with('#'))
        || overview.contains("## 内容概览")
    {
        return Err(ai_error(
            "SOURCE_AI_OVERVIEW_INVALID",
            "The content overview must be 1-3 plain-text paragraphs without headings.",
            true,
        ));
    }
    let paragraphs = overview
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>();
    if !(1..=3).contains(&paragraphs.len()) {
        return Err(ai_error(
            "SOURCE_AI_OVERVIEW_INVALID",
            "The content overview must contain 1-3 paragraphs.",
            true,
        ));
    }
    Ok(paragraphs.join("\n\n"))
}

fn normalize_body(raw: &str, title: &str, overview: &str) -> Result<String, BackendError> {
    let body = raw.trim().replace("\r\n", "\n");
    if body.is_empty() || body.starts_with("---\n") {
        return Err(ai_error(
            "SOURCE_AI_OUTPUT_INVALID",
            "AI organization must return body Markdown without frontmatter.",
            true,
        ));
    }
    let without_overview = remove_existing_overviews(&body)?;
    let mut lines = without_overview.lines();
    let first = lines.next().unwrap_or_default().trim();
    let (heading, rest) = if first.starts_with("# ") && !first.starts_with("## ") {
        (
            first.to_string(),
            lines.collect::<Vec<_>>().join("\n").trim().to_string(),
        )
    } else {
        (
            format!("# {}", title.trim()),
            without_overview.trim().to_string(),
        )
    };
    if heading.trim() == "#" {
        return Err(ai_error(
            "SOURCE_AI_OUTPUT_INVALID",
            "AI organization produced an empty Source title.",
            true,
        ));
    }
    let mut result = format!("{heading}\n\n## 内容概览\n\n{overview}\n\n{OVERVIEW_END_MARKER}");
    if !rest.is_empty() {
        result.push_str("\n\n");
        result.push_str(&rest);
    }
    result.push('\n');
    Ok(result)
}

fn remove_existing_overviews(body: &str) -> Result<String, BackendError> {
    let lines = body.lines().collect::<Vec<_>>();
    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if line.trim() != "## 内容概览" {
            output.push(line);
            index += 1;
            continue;
        }

        let tail = &lines[index + 1..];
        if let Some(marker_offset) = tail
            .iter()
            .position(|candidate| candidate.trim() == OVERVIEW_END_MARKER)
        {
            index += marker_offset + 2;
            continue;
        }

        if let Some(heading_offset) = tail.iter().position(|candidate| {
            let candidate = candidate.trim();
            candidate.starts_with("# ") || candidate.starts_with("## ")
        }) {
            index += heading_offset + 1;
            continue;
        }

        return Err(ai_error(
            "SOURCE_AI_OVERVIEW_AMBIGUOUS",
            "An existing content overview has no safe boundary. Add a following section heading or rerun after saving a marker-delimited overview.",
            true,
        ));
    }
    Ok(output.join("\n"))
}

fn extract_json_object(raw: &str) -> Result<&str, BackendError> {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find("```json") {
        let rest = &trimmed[start + 7..];
        let end = rest.find("```").ok_or_else(|| {
            ai_error(
                "SOURCE_AI_OUTPUT_INVALID",
                "AI organization returned an unclosed JSON fence.",
                true,
            )
        })?;
        return Ok(rest[..end].trim());
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        return Ok(&trimmed[start..=end]);
    }
    Err(ai_error(
        "SOURCE_AI_OUTPUT_INVALID",
        "AI organization did not return candidate JSON.",
        true,
    ))
}

fn workspace_error(action: &str, workspace: &Path, error: std::io::Error) -> BackendError {
    ai_error(
        "SOURCE_AI_WORKSPACE_FAILED",
        &format!(
            "Could not {action} the isolated Source AI workspace {}: {error}",
            workspace.display()
        ),
        true,
    )
}

fn source_invalid() -> BackendError {
    ai_error(
        "SOURCE_INVALID",
        "The Source record is invalid or incomplete.",
        false,
    )
}

fn ai_error(code: &str, message: &str, recoverable: bool) -> BackendError {
    BackendError::new(code, message, recoverable, false)
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::import_v2::{QualityReport, SourceFrontmatter, SourcePageType};

    fn base_markdown(body: &str) -> String {
        let frontmatter = SourceFrontmatter {
            page_type: SourcePageType::Source,
            source_id: "source-1".into(),
            version_id: "version-1".into(),
            source_kind: "document".into(),
            title: "测试来源".into(),
            imported_at: "2026-07-28T00:00:00Z".into(),
            content_hash: sha256(body.as_bytes()),
            platform: None,
            canonical_url: None,
            platform_content_id: None,
            author: Some("张三".into()),
            published_at: Some("2026-07-24".into()),
            language: Some("zh-CN".into()),
            quality: serde_json::from_value::<QualityReport>(serde_json::json!({
                "level": "pass",
                "metrics": [],
                "warnings": []
            }))
            .unwrap(),
            restricted: false,
        };
        render_source_markdown(&frontmatter, body).unwrap()
    }

    #[test]
    fn candidate_has_exactly_one_overview_and_rerun_replaces_it() {
        let base = base_markdown(
            "# 测试来源\n\n## 内容概览\n\n旧概览。\n\n## 正文\n\n张三在 2026-07-24 说“保留原话”。\n",
        );
        let raw = serde_json::json!({
            "overview": "新概览。",
            "bodyMarkdown": "# 测试来源\n\n## 内容概览\n\n模型重复的旧概览。\n\n## 正文\n\n张三在 2026-07-24 说“保留原话”。"
        })
        .to_string();
        let candidate = build_candidate_markdown(&base, "测试来源", &raw).unwrap();
        assert_eq!(candidate.matches("## 内容概览").count(), 1);
        assert!(candidate.contains("新概览。"));
        assert!(!candidate.contains("模型重复的旧概览"));
        validate_exactly_one_overview(&candidate).unwrap();
        let (_, candidate_body) = parse_final_source(&candidate).unwrap();
        let rerun_raw = serde_json::json!({
            "overview": "第二次概览。",
            "bodyMarkdown": candidate_body,
        })
        .to_string();
        let rerun = build_candidate_markdown(&candidate, "测试来源", &rerun_raw).unwrap();
        assert_eq!(rerun.matches("## 内容概览").count(), 1);
        assert!(rerun.contains("第二次概览。"));
        assert!(rerun.contains("张三在 2026-07-24 说“保留原话”"));
    }

    #[test]
    fn markerless_overview_without_following_heading_fails_closed() {
        let base = base_markdown(
            "# 测试来源\n\n## 内容概览\n\n旧概览。\n\n张三保留的正文没有下一个标题。\n",
        );
        let raw = serde_json::json!({
            "overview": "新概览。",
            "bodyMarkdown": "# 测试来源\n\n## 内容概览\n\n旧概览。\n\n张三保留的正文没有下一个标题。"
        })
        .to_string();
        assert_eq!(
            build_candidate_markdown(&base, "测试来源", &raw)
                .unwrap_err()
                .code,
            "SOURCE_AI_OVERVIEW_AMBIGUOUS"
        );
    }

    #[test]
    fn factual_text_changes_reach_candidate_diff_review_instead_of_being_hard_rejected() {
        let base = base_markdown(
            "# Source\n\nAlice reported 42 items at 09:30 via https://old.example and said “exact quote”.\n\n张三在北京启动项目。\n",
        );
        let raw = serde_json::json!({
            "overview": "Bob corrected the record to 43 items at 10:30.",
            "bodyMarkdown": "# Source revised\n\nBob reported 43 items at 10:30 via https://new.example and said “changed quote”.\n\n李四在上海取消项目。"
        })
        .to_string();
        let candidate = build_candidate_markdown(&base, "Source", &raw).unwrap();
        let (frontmatter, body) = parse_final_source(&candidate).unwrap();

        assert_eq!(frontmatter.source_id, "source-1");
        assert_eq!(frontmatter.version_id, "version-1");
        assert_eq!(frontmatter.author.as_deref(), Some("张三"));
        assert!(body.contains("Bob corrected the record to 43 items at 10:30."));
        assert!(body.contains("https://new.example"));
        assert!(body.contains("“changed quote”"));
        assert!(body.contains("李四在上海取消项目"));
        assert!(!body.contains("https://old.example"));
        validate_exactly_one_overview(&candidate).unwrap();
    }

    #[test]
    fn malformed_model_output_is_recoverable_for_saved_route_retry() {
        let base = base_markdown("# Source\n\nBody\n");
        let output_error = build_candidate_markdown(&base, "Source", "not candidate json")
            .expect_err("malformed output must fail");
        assert_eq!(output_error.code, "SOURCE_AI_OUTPUT_INVALID");
        assert!(output_error.recoverable);

        let invalid_overview = serde_json::json!({
            "overview": "## Model-added heading",
            "bodyMarkdown": "# Source\n\nBody"
        })
        .to_string();
        let overview_error = build_candidate_markdown(&base, "Source", &invalid_overview)
            .expect_err("invalid overview must fail");
        assert_eq!(overview_error.code, "SOURCE_AI_OVERVIEW_INVALID");
        assert!(overview_error.recoverable);
    }

    #[test]
    fn prompt_and_workspace_are_bounded_to_serialized_source_input() {
        let input = SourceAiOrganizeInput {
            source_id: "source-中文".into(),
            version_id: "version-1".into(),
            markdown_hash: "hash".into(),
            title: "标题".into(),
            source_kind: "image".into(),
            author: None,
            published_at: None,
            language: Some("zh-CN".into()),
            current_markdown: "# 标题\n\nOCR 文本".into(),
            retained_text_evidence: vec![SourceAiTextEvidence {
                kind: "ocr_text".into(),
                path: "raw/sources/source-中文/version-1/derived/ocr.txt".into(),
                text: "只允许的 OCR".into(),
            }],
            media_references: vec!["raw/sources/source-中文/version-1/original/image.png".into()],
            custom_instructions: Some("Correct 42 to 43 using the retained evidence.".into()),
        };
        let provider = provider_prompt(&input, "zh-CN").unwrap();
        let agent = agent_prompt(&input, "zh-CN").unwrap();
        for prompt in [&provider, &agent] {
            assert!(prompt.contains(SOURCE_REWRITE_CONTRACT));
            assert!(prompt.contains("complete candidate as a Diff"));
            assert!(!prompt.contains("Preserve all factual tokens"));
            assert!(!prompt.contains("Do not alter facts"));
        }
        assert!(provider.contains("只允许的 OCR"));
        assert!(provider.contains("image.png"));
        assert!(provider.contains("Correct 42 to 43"));
        assert!(!provider.contains("api_key"));
        let task_id = uuid::Uuid::new_v4().to_string();
        let workspace = create_agent_workspace(&task_id, &input).unwrap();
        let mut entries = fs::read_dir(&workspace)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        entries.sort();
        assert_eq!(entries, vec!["input.json", "output-schema.json"]);
        assert_eq!(
            fs::read_to_string(workspace.join("output-schema.json")).unwrap(),
            SOURCE_AI_OUTPUT_SCHEMA
        );
        cleanup_agent_workspace(&workspace).unwrap();
    }

    #[test]
    fn agent_result_reads_actual_bytes_from_one_bounded_handle() {
        let workspace = tempfile::tempdir().unwrap();
        fs::write(
            workspace.path().join("candidate.json"),
            vec![b'x'; MAX_SOURCE_AI_OUTPUT_BYTES + 1],
        )
        .unwrap();

        let error = read_agent_result(workspace.path(), "fallback").unwrap_err();
        assert_eq!(error.code, "SOURCE_AI_OUTPUT_INVALID");
    }

    #[cfg(unix)]
    #[test]
    fn agent_result_rejects_symlink_without_following_it() {
        use std::os::unix::fs::symlink;

        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), "outside sentinel").unwrap();
        symlink(outside.path(), workspace.path().join("candidate.json")).unwrap();

        let error = read_agent_result(workspace.path(), "fallback").unwrap_err();
        assert_eq!(error.code, "SOURCE_AI_OUTPUT_INVALID");
    }

    #[test]
    fn stale_source_ai_workspaces_are_scavenged_without_touching_other_temp_content() {
        let root = std::env::temp_dir()
            .join("llm-wiki-desktop-scavenger-tests")
            .join(uuid::Uuid::new_v4().to_string());
        let stale = root.join("source-ai-stale");
        let unrelated = root.join("other-work");
        fs::create_dir_all(&stale).unwrap();
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(stale.join("input.json"), b"sensitive source").unwrap();

        let removed = cleanup_stale_agent_workspaces(
            &root,
            Duration::ZERO,
            SystemTime::now() + Duration::from_secs(1),
        );

        assert_eq!(removed, 1);
        assert!(!stale.exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_scavenger_never_removes_a_workspace_leased_by_this_process() {
        let root = std::env::temp_dir()
            .join("llm-wiki-desktop-scavenger-tests")
            .join(uuid::Uuid::new_v4().to_string());
        let active = root.join("source-ai-active");
        fs::create_dir_all(&active).unwrap();
        register_agent_workspace(&active).unwrap();

        assert_eq!(
            cleanup_stale_agent_workspaces(
                &root,
                Duration::ZERO,
                SystemTime::now() + Duration::from_secs(1),
            ),
            0
        );
        assert!(active.exists());

        unregister_agent_workspace(&active);
        assert_eq!(
            cleanup_stale_agent_workspaces(
                &root,
                Duration::ZERO,
                SystemTime::now() + Duration::from_secs(1),
            ),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }
}
