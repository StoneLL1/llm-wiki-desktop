use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{errors::BackendError, models::import_v2_agent::AgentToolGrant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ImportAgentToolCall {
    InspectSource,
    RunDeterministicRoute {
        route: String,
    },
    RunOcr {
        page_indices: Vec<u32>,
        profile: String,
    },
    RunAsr {
        model_id: String,
    },
    ParseSanitizedSnapshot,
    ValidateCandidate {
        relative_markdown_path: String,
    },
}

impl ImportAgentToolCall {
    pub(crate) fn safe_name(&self) -> &'static str {
        self.kind()
    }
    fn grant(&self) -> AgentToolGrant {
        match self {
            Self::InspectSource => AgentToolGrant::InspectSource,
            Self::RunDeterministicRoute { .. } => AgentToolGrant::RunDeterministicRoute,
            Self::RunOcr { .. } => AgentToolGrant::RunOcr,
            Self::RunAsr { .. } => AgentToolGrant::RunAsr,
            Self::ParseSanitizedSnapshot => AgentToolGrant::ParseSanitizedSnapshot,
            Self::ValidateCandidate { .. } => AgentToolGrant::ValidateCandidate,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::InspectSource => "inspect_source",
            Self::RunDeterministicRoute { .. } => "run_deterministic_route",
            Self::RunOcr { .. } => "run_ocr",
            Self::RunAsr { .. } => "run_asr",
            Self::ParseSanitizedSnapshot => "parse_sanitized_snapshot",
            Self::ValidateCandidate { .. } => "validate_candidate",
        }
    }

    fn safe_detail(&self) -> Option<String> {
        match self {
            Self::RunDeterministicRoute { route } => Some(route.clone()),
            Self::RunOcr { profile, .. } => Some(profile.clone()),
            Self::RunAsr { model_id } => Some(model_id.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportAgentToolTaskContext {
    pub task_id: String,
    pub project_id: String,
    pub session_id: String,
    pub item_id: String,
    pub item_staging_root: PathBuf,
    pub workspace_root: PathBuf,
    pub grants: Vec<AgentToolGrant>,
    pub input_hashes: Vec<String>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportAgentToolResult {
    pub outcome: String,
    pub output_hashes: Vec<String>,
    pub warnings: Vec<String>,
    pub resource_units: u64,
}

pub trait ImportAgentToolExecutor: Send + Sync {
    fn execute(
        &self,
        context: &ImportAgentToolTaskContext,
        call: &ImportAgentToolCall,
    ) -> Result<ImportAgentToolResult, BackendError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolLedgerRecord {
    task_id: String,
    session_id: String,
    item_id: String,
    tool_kind: String,
    safe_detail: Option<String>,
    started_at: String,
    completed_at: String,
    outcome: String,
    warnings: Vec<String>,
    input_hashes: Vec<String>,
    output_hashes: Vec<String>,
    resource_units: u64,
}

pub struct ImportAgentToolBroker {
    executor: Arc<dyn ImportAgentToolExecutor>,
    ledger_lock: Mutex<()>,
}

/// Production executor for the bounded workspace tools. It never resolves a
/// user supplied path outside the task-owned item workspace.
#[derive(Default)]
pub struct LocalImportAgentToolExecutor;

impl ImportAgentToolExecutor for LocalImportAgentToolExecutor {
    fn execute(
        &self,
        context: &ImportAgentToolTaskContext,
        call: &ImportAgentToolCall,
    ) -> Result<ImportAgentToolResult, BackendError> {
        let workspace = &context.workspace_root;
        let (hashes, warnings) = match call {
            ImportAgentToolCall::InspectSource => {
                let mut hashes = Vec::new();
                for entry in fs::read_dir(workspace.join("source")).map_err(tool_io_error)? {
                    let path = entry.map_err(tool_io_error)?.path();
                    let bytes = read_tool_regular(workspace, &path, 64 * 1024 * 1024)?;
                    hashes.push(format!("{:x}", Sha256::digest(bytes)));
                }
                let count = hashes.len();
                (hashes, vec![format!("{count} source evidence file(s) inspected")])
            }
            ImportAgentToolCall::RunDeterministicRoute { route } if route == "file.native" => {
                use crate::services::import_v2::engine::{EngineOperation, EngineRequest, ImportEngine};
                use crate::services::import_v2::native_file_engine::NativeFileEngine;
                use crate::models::import_v2::{ImportInput, ImportInputKind, SourceIdentity};
                let source = fs::read_dir(workspace.join("source")).map_err(tool_io_error)?
                    .filter_map(Result::ok).map(|entry| entry.path())
                    .find(|path| path.extension().and_then(|value| value.to_str())
                        .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown" | "txt" | "html" | "htm")))
                    .ok_or_else(|| tool_error("IMPORT_AGENT_TOOL_CAPABILITY_UNAVAILABLE", "No supported text evidence is available for native parsing."))?;
                let bytes = read_tool_regular(workspace, &source, 8 * 1024 * 1024)?;
                let metadata = fs::metadata(&source).map_err(tool_io_error)?;
                let canonical = source.canonicalize().map_err(tool_io_error)?;
                let name = source.file_name().and_then(|value| value.to_str()).ok_or_else(|| tool_error("IMPORT_AGENT_TOOL_ARGUMENT_REJECTED", "Source filename is invalid."))?;
                let identity = SourceIdentity {
                    canonical_path: canonical.to_string_lossy().into_owned(),
                    size_bytes: bytes.len() as u64,
                    modified_nanos: metadata.modified().ok().and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok().map(|duration| duration.as_nanos())),
                    file_id: None,
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                    magic: format!("{:x}", Sha256::digest(&bytes[..bytes.len().min(8192)])),
                };
                let request = EngineRequest {
                    protocol_version: "1".into(), request_id: uuid::Uuid::new_v4().to_string(),
                    project_id: context.project_id.clone(), session_id: context.session_id.clone(),
                    item_id: context.item_id.clone(), task_id: context.task_id.clone(),
                    operation: EngineOperation::Extract,
                    input: ImportInput { kind: ImportInputKind::File, display_name: name.into(),
                        locator: format!("source/{name}"), normalized_locator: None,
                        source_identity: Some(identity), media_save_mode: Default::default() },
                    project_root: workspace.to_string_lossy().into_owned(),
                    staging_root: "logs/native-route".into(), chained_input: None,
                    local_asr_authorized: false, asr_probe_only: false, asr_profile: None,
                    recognition_language: None, selected_subtitle: None,
                    local_ocr_authorized: false, media_save_mode: Default::default(),
                };
                let output = NativeFileEngine::default().execute(&request, &crate::tasks::task_model::CancellationToken::new())?;
                let markdown = read_tool_regular(workspace, &workspace.join("logs/native-route").join(&output.markdown_path), 8 * 1024 * 1024)?;
                (vec![format!("{:x}", Sha256::digest(markdown))], output.warnings)
            }
            ImportAgentToolCall::RunDeterministicRoute { .. } => return Err(tool_error("IMPORT_AGENT_TOOL_CAPABILITY_UNAVAILABLE", "This deterministic route is unavailable in the local Agent bridge.")),
            ImportAgentToolCall::ValidateCandidate { relative_markdown_path } => {
                let bytes = read_tool_regular(workspace, &workspace.join(relative_markdown_path), 16 * 1024 * 1024)?;
                let text = std::str::from_utf8(&bytes).map_err(|_| tool_error("IMPORT_AGENT_TOOL_ARGUMENT_REJECTED", "Candidate Markdown must be UTF-8."))?;
                if text.trim().is_empty() { return Err(tool_error("IMPORT_AGENT_TOOL_ARGUMENT_REJECTED", "Candidate Markdown is empty.")); }
                (vec![format!("{:x}", Sha256::digest(bytes))], Vec::new())
            }
            ImportAgentToolCall::ParseSanitizedSnapshot => {
                let bytes = read_tool_regular(workspace, &workspace.join("deterministic/candidate.md"), 8 * 1024 * 1024)?;
                (vec![format!("{:x}", Sha256::digest(bytes))], Vec::new())
            }
            ImportAgentToolCall::RunOcr { .. } | ImportAgentToolCall::RunAsr { .. } =>
                return Err(tool_error("IMPORT_AGENT_TOOL_CAPABILITY_UNAVAILABLE", "Media recognition requires an installed capability and explicit item authorization.")),
        };
        Ok(ImportAgentToolResult {
            outcome: "succeeded".into(),
            output_hashes: hashes,
            warnings,
            resource_units: 1,
        })
    }
}

fn read_tool_regular(workspace: &Path, path: &Path, limit: usize) -> Result<Vec<u8>, BackendError> {
    if !path.starts_with(workspace)
        || path.strip_prefix(workspace).is_ok_and(|relative| {
            relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        })
    {
        return Err(tool_error(
            "IMPORT_AGENT_TOOL_SCOPE_DENIED",
            "Tool path escaped its workspace.",
        ));
    }
    let mut ancestor = path.parent();
    while let Some(dir) = ancestor {
        if !dir.starts_with(workspace) {
            break;
        }
        let meta = fs::symlink_metadata(dir).map_err(tool_io_error)?;
        if !meta.is_dir() || meta.file_type().is_symlink() || is_windows_reparse(&meta) {
            return Err(tool_error(
                "IMPORT_AGENT_TOOL_SCOPE_DENIED",
                "Tool directory is linked or reparse-backed.",
            ));
        }
        if dir == workspace {
            break;
        }
        ancestor = dir.parent();
    }
    let metadata = fs::symlink_metadata(path).map_err(tool_io_error)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || is_windows_reparse(&metadata)
        || metadata.len() > limit as u64
    {
        return Err(tool_error(
            "IMPORT_AGENT_TOOL_SCOPE_DENIED",
            "Tool input is linked, non-regular, or too large.",
        ));
    }
    super::agent_workspace::read_isolated_regular_file(workspace, path, limit).map_err(|_| {
        tool_error(
            "IMPORT_AGENT_TOOL_SCOPE_DENIED",
            "Tool input changed or escaped its workspace while being read.",
        )
    })
}

impl ImportAgentToolBroker {
    pub fn new(executor: Arc<dyn ImportAgentToolExecutor>) -> Self {
        Self {
            executor,
            ledger_lock: Mutex::new(()),
        }
    }

    pub fn invoke(
        &self,
        context: &ImportAgentToolTaskContext,
        call: ImportAgentToolCall,
    ) -> Result<ImportAgentToolResult, BackendError> {
        validate_context(context)?;
        if context.cancelled {
            return Err(tool_error(
                "IMPORT_AGENT_TOOL_CANCELLED",
                "Agent tool call was cancelled.",
            ));
        }
        if !context.grants.contains(&call.grant()) {
            return Err(tool_error(
                "IMPORT_AGENT_TOOL_NOT_GRANTED",
                "Agent tool was not granted for this item.",
            ));
        }
        validate_call(context, &call)?;
        let started_at = chrono::Utc::now().to_rfc3339();
        let result = self.executor.execute(context, &call);
        let completed_at = chrono::Utc::now().to_rfc3339();
        let (outcome, warnings, output_hashes, resource_units) = match &result {
            Ok(value) => (
                value.outcome.clone(),
                value.warnings.clone(),
                value.output_hashes.clone(),
                value.resource_units,
            ),
            Err(error) => (error.code.clone(), vec![], vec![], 0),
        };
        self.append_ledger(
            context,
            ToolLedgerRecord {
                task_id: context.task_id.clone(),
                session_id: context.session_id.clone(),
                item_id: context.item_id.clone(),
                tool_kind: call.kind().into(),
                safe_detail: call.safe_detail(),
                started_at,
                completed_at,
                outcome,
                warnings,
                input_hashes: context.input_hashes.clone(),
                output_hashes,
                resource_units,
            },
        )?;
        result
    }

    fn append_ledger(
        &self,
        context: &ImportAgentToolTaskContext,
        record: ToolLedgerRecord,
    ) -> Result<(), BackendError> {
        let _guard = self.ledger_lock.lock().map_err(|_| {
            tool_error(
                "IMPORT_AGENT_TOOL_LEDGER_FAILED",
                "Agent tool ledger is unavailable.",
            )
        })?;
        let logs = context.workspace_root.join("logs");
        fs::create_dir_all(&logs).map_err(tool_io_error)?;
        let path = logs.join("tool-ledger.json");
        let mut records: Vec<ToolLedgerRecord> = if path.exists() {
            serde_json::from_slice(&fs::read(&path).map_err(tool_io_error)?).map_err(|_| {
                tool_error(
                    "IMPORT_AGENT_TOOL_LEDGER_FAILED",
                    "Agent tool ledger is invalid.",
                )
            })?
        } else {
            Vec::new()
        };
        records.push(record);
        let bytes = serde_json::to_vec_pretty(&records).map_err(|_| {
            tool_error(
                "IMPORT_AGENT_TOOL_LEDGER_FAILED",
                "Agent tool ledger could not be encoded.",
            )
        })?;
        fs::write(path, bytes).map_err(tool_io_error)
    }
}

fn validate_context(context: &ImportAgentToolTaskContext) -> Result<(), BackendError> {
    if !is_safe_component(&context.task_id)
        || !is_safe_component(&context.project_id)
        || !is_safe_component(&context.session_id)
        || !is_safe_component(&context.item_id)
    {
        return Err(tool_error(
            "IMPORT_AGENT_TOOL_SCOPE_DENIED",
            "Agent tool identity is invalid.",
        ));
    }
    let agent_root = context.item_staging_root.join("agent");
    let staging_matches_identity = context
        .item_staging_root
        .file_name()
        .is_some_and(|value| value == "staging")
        && context
            .item_staging_root
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|value| value == context.item_id.as_str())
        && context
            .item_staging_root
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|value| value == "items")
        && context
            .item_staging_root
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|value| value == context.session_id.as_str());
    if !staging_matches_identity
        || !context.workspace_root.starts_with(&agent_root)
        || context.workspace_root == agent_root
        || !context.workspace_root.is_dir()
    {
        return Err(tool_error(
            "IMPORT_AGENT_TOOL_SCOPE_DENIED",
            "Agent tool workspace is not bound to this item.",
        ));
    }
    reject_existing_links(&context.workspace_root)?;
    Ok(())
}

fn validate_call(
    context: &ImportAgentToolTaskContext,
    call: &ImportAgentToolCall,
) -> Result<(), BackendError> {
    match call {
        ImportAgentToolCall::RunDeterministicRoute { route } => {
            const ROUTES: &[&str] = &[
                "file.native",
                "file.docling",
                "file.oxide",
                "file.pdf_text",
                "file.pdf_layout",
                "web.generic",
                "ocr.basic",
                "ocr.accurate",
                "media.asr",
            ];
            if !ROUTES.contains(&route.as_str()) {
                return Err(tool_error(
                    "IMPORT_AGENT_TOOL_ARGUMENT_REJECTED",
                    "Deterministic route is not allowlisted.",
                ));
            }
        }
        ImportAgentToolCall::RunOcr {
            page_indices,
            profile,
        } => {
            if page_indices.is_empty()
                || page_indices.len() > 64
                || !matches!(profile.as_str(), "basic" | "accurate")
            {
                return Err(tool_error(
                    "IMPORT_AGENT_TOOL_ARGUMENT_REJECTED",
                    "OCR request exceeds its grant.",
                ));
            }
        }
        ImportAgentToolCall::RunAsr { model_id } => {
            if !safe_identifier(model_id) {
                return Err(tool_error(
                    "IMPORT_AGENT_TOOL_ARGUMENT_REJECTED",
                    "ASR model identifier is invalid.",
                ));
            }
        }
        ImportAgentToolCall::ValidateCandidate {
            relative_markdown_path,
        } => {
            let path = Path::new(relative_markdown_path);
            if path.is_absolute()
                || path.extension().and_then(|value| value.to_str()) != Some("md")
                || !relative_markdown_path
                    .replace('\\', "/")
                    .starts_with("output/")
                || path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(tool_error(
                    "IMPORT_AGENT_TOOL_ARGUMENT_REJECTED",
                    "Candidate path is outside output Markdown.",
                ));
            }
            let output = context.workspace_root.join(path);
            if !output.starts_with(context.workspace_root.join("output")) {
                return Err(tool_error(
                    "IMPORT_AGENT_TOOL_SCOPE_DENIED",
                    "Candidate path escapes output.",
                ));
            }
        }
        ImportAgentToolCall::InspectSource | ImportAgentToolCall::ParseSanitizedSnapshot => {}
    }
    Ok(())
}

fn reject_existing_links(path: &Path) -> Result<(), BackendError> {
    for ancestor in path.ancestors() {
        if !ancestor.exists() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor).map_err(tool_io_error)?;
        if metadata.file_type().is_symlink() || is_windows_reparse(&metadata) {
            return Err(tool_error(
                "IMPORT_AGENT_TOOL_SCOPE_DENIED",
                "Agent tool workspace contains a link or reparse point.",
            ));
        }
        if ancestor.file_name().and_then(|value| value.to_str()) == Some(".app") {
            break;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_windows_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_windows_reparse(_metadata: &fs::Metadata) -> bool {
    false
}

fn is_safe_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

fn safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn tool_error(code: &'static str, message: &'static str) -> BackendError {
    BackendError::new(code, message, false, true)
}

fn tool_io_error(error: std::io::Error) -> BackendError {
    BackendError::new(
        "IMPORT_AGENT_TOOL_LEDGER_FAILED",
        error.to_string(),
        true,
        false,
    )
}
