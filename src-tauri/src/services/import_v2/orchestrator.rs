use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use crate::errors::{
    BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ITEM_NOT_FOUND, IMPORT_V2_STATE_INVALID,
};
use crate::models::import_v2::{
    AttemptOutcome, AttemptRecord, ImportInput, ImportInputKind, ImportIssue, ImportItem,
    ImportItemStatus, ImportResourceMode, ImportSession, ImportSessionStatus, ImportStage,
    SourceIdentity,
};
use crate::models::import_v2_agent::AgentAssistanceTrigger;
use crate::models::import_v2_agent::AgentCandidate;
use crate::models::import_v2_file::FileFormat;
use crate::models::paths::ProjectContext;
use crate::models::task::{TaskResult, TaskResultReference, TaskStatus, TaskType};
use crate::services::import_v2::capability_pack::ResolvedCapabilityPack;
use crate::services::import_v2::engine::{
    validate_engine_result, EngineContinuation, EngineOperation, EngineRegistry, EngineRequest,
    EngineResult, ImportEngine,
};
use crate::services::import_v2::file_router::{
    AttemptOutcome as RouteOutcome, CapabilitySnapshot, FileRoutePlanner, QualityFloor,
};
use crate::services::import_v2::native_file_engine::NativeFileEngine;
use crate::services::import_v2::pack_engine::PackProcessEngine;
use crate::services::import_v2::quality_gate::QualityGate;
use crate::services::import_v2::transaction::FileTransaction;
use crate::services::import_v2::web_target_store::WebTargetStore;
use crate::services::import_v2::SessionStore;
use crate::services::FileStore;
use crate::services::SecretService;
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;
use unicode_normalization::UnicodeNormalization;

pub struct ImportV2Service {
    pub(super) sessions: SessionStore,
    engines: EngineRegistry,
    quality: QualityGate,
    pub(super) mutation_lock: Mutex<()>,
    web_targets: Arc<WebTargetStore>,
}

impl Default for ImportV2Service {
    fn default() -> Self {
        Self::with_secret_service(SecretService::default())
    }
}

impl ImportV2Service {
    pub fn find_unfinished_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
    ) -> Result<Option<String>, BackendError> {
        let root = context.app_dir.join("import-sessions");
        let metadata = match fs::symlink_metadata(&root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(BackendError::new("IMPORT_V2_SESSION_SCAN_FAILED", error.to_string(), true, true)),
        };
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || crate::services::import_v2::transaction::is_project_reparse_point(&metadata)
        {
            return Err(BackendError::new("IMPORT_V2_SESSION_SCAN_FAILED", "Import session directory is not safe.", false, true));
        }
        let mut ids = fs::read_dir(root)
            .map_err(|error| BackendError::new("IMPORT_V2_SESSION_SCAN_FAILED", error.to_string(), true, true))?
            .flatten()
            .filter_map(|entry| {
                let metadata = fs::symlink_metadata(entry.path()).ok()?;
                if metadata.is_dir()
                    && !metadata.file_type().is_symlink()
                    && !crate::services::import_v2::transaction::is_project_reparse_point(&metadata)
                {
                    let id = entry.file_name().to_string_lossy().into_owned();
                    (id.len() <= 64 && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')).then_some(id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        ids.sort();
        for id in ids {
            let session = match self.load_session(context, files, &id) {
                Ok(session) => session,
                // A stale or partially written session is evidence to leave
                // untouched, not a reason to make every new V2 import
                // unavailable. Unsafe session roots still fail above.
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        crate::errors::IMPORT_V2_SESSION_INVALID
                            | crate::errors::IMPORT_V2_SESSION_NOT_FOUND
                            | "JSON_PARSE_FAILED"
                            | "FILE_READ_FAILED"
                    ) => continue,
                Err(error) => return Err(error),
            };
            if !matches!(session.status, ImportSessionStatus::Completed | ImportSessionStatus::Cancelled)
                && !session.items.is_empty()
            {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    pub fn with_secret_service(secrets: SecretService) -> Self {
        let engines = EngineRegistry::default();
        engines
            .register(Arc::new(NativeFileEngine::default()))
            .expect("the built-in native file engine identifier is unique");
        Self {
            sessions: SessionStore::default(),
            engines,
            quality: QualityGate::default(),
            mutation_lock: Mutex::new(()),
            web_targets: Arc::new(WebTargetStore::new(secrets)),
        }
    }
    pub fn store_web_target(
        &self,
        target: &crate::services::import_v2::url_policy::SessionWebTarget,
    ) -> Result<String, BackendError> {
        self.web_targets.store(target)
    }
    pub fn delete_web_target(&self, reference: &str) -> Result<(), BackendError> {
        self.web_targets.delete(reference)
    }
    pub fn resolve_web_target(
        &self,
        locator: &str,
        public: Option<&str>,
    ) -> Result<crate::services::import_v2::url_policy::SessionWebTarget, BackendError> {
        self.web_targets.resolve(locator, public)
    }
    pub fn authorize_private_target(
        &self,
        grant: crate::services::import_v2::url_policy::PrivateTargetGrant,
    ) -> Result<String, BackendError> {
        self.web_targets.authorize_private(grant)
    }
    pub fn authorize_bilibili_asr(
        &self,
        grant: crate::services::import_v2::web_target_store::BilibiliAsrGrant,
    ) -> Result<(), BackendError> {
        self.web_targets.authorize_bilibili_asr(grant)
    }
    pub fn take_bilibili_asr(
        &self,
        project_id: &str,
        session_id: &str,
        item_id: &str,
        expected_request_url: &str,
    ) -> Result<Option<crate::services::import_v2::web_target_store::BilibiliAsrGrant>, BackendError> {
        self.web_targets.take_bilibili_asr(
            project_id,
            session_id,
            item_id,
            expected_request_url,
        )
    }
    pub fn bind_authenticated_profile(&self, project_id: &str, session_id: &str, item_id: &str, profile: std::path::PathBuf) -> Result<(), BackendError> {
        self.web_targets.bind_authenticated_profile(project_id, session_id, item_id, profile)
    }
    pub fn release_item_after_login(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.status != ImportItemStatus::WaitingLogin {
                return Err(BackendError::new(crate::errors::IMPORT_V2_STATE_INVALID, "Only a waiting-login item can be released.", false, true));
            }
            transition_item(item, ImportItemStatus::Failed)?;
            item.task_id = None;
            item.issue = None;
            Ok(())
        })
    }
    pub fn create_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        mode: ImportResourceMode,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        self.sessions.create(context, files, mode)
    }
    pub fn add_inputs(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        inputs: Vec<ImportInput>,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        self.sessions.add_inputs(context, files, session_id, inputs)
    }

    /// Stage user-provided text inside the V2 session workspace and register
    /// it as a normal immutable file input. This keeps clipboard imports out
    /// of `raw/` and gives the native engine the same identity/CAS checks as a
    /// discovered file.
    pub fn add_text_input(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        display_name: &str,
        content: &str,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let name = if display_name.trim().is_empty() {
            "text-import.md"
        } else {
            display_name.trim()
        };
        let extension = Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("md");
        let relative = format!(
            ".app/import-sessions/{session_id}/inputs/{}.{}",
            uuid::Uuid::new_v4(),
            extension
        );
        let path = context.resolve_project_path(&relative)?;
        let bytes = content.as_bytes();
        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.write_new(&path, bytes)?;
        transaction.commit()?;
        let canonical_path = path.canonicalize().map_err(|error| {
            BackendError::new("IMPORT_V2_TEXT_STAGE_FAILED", error.to_string(), true, false)
        })?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            BackendError::new("IMPORT_V2_TEXT_STAGE_FAILED", error.to_string(), true, false)
        })?;
        let modified_nanos = metadata.modified().ok().and_then(|value| {
            value
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_nanos())
        });
        let digest = Sha256::digest(bytes);
        let magic = Sha256::digest(&bytes[..bytes.len().min(8192)]);
        self.sessions.add_inputs(
            context,
            files,
            session_id,
            vec![ImportInput {
                kind: ImportInputKind::File,
                display_name: name.to_string(),
                locator: relative.clone(),
                normalized_locator: Some(
                    canonical_path
                        .to_string_lossy()
                        .replace('\\', "/")
                        .nfc()
                        .collect::<String>()
                        .to_lowercase(),
                ),
                source_identity: Some(SourceIdentity {
                    canonical_path: canonical_path.to_string_lossy().into_owned(),
                    size_bytes: metadata.len(),
                    modified_nanos,
                    file_id: None,
                    sha256: format!("{digest:x}"),
                    magic: format!("{magic:x}"),
                }),
            }],
        )
    }
    pub fn load_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        self.sessions.load(context, files, session_id)
    }

    pub fn begin_agent_assistance(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        trigger: AgentAssistanceTrigger,
        agent_kind: crate::models::agent::AgentKind,
        max_attempts: u8,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            let agent_attempts = item
                .attempts
                .iter()
                .filter(|attempt| {
                    attempt.route.starts_with("agent_assistance/")
                        || attempt.route.starts_with("byok_assistance/")
                })
                .collect::<Vec<_>>();
            if agent_attempts.len() >= usize::from(max_attempts) {
                return Err(task_error(
                    "The Agent assistance attempt budget is exhausted for this item.",
                ));
            }
            if agent_attempts
                .iter()
                .any(|attempt| attempt.completed_at.is_none())
            {
                return Err(task_error(
                    "Another Agent assistance task is already active for this item.",
                ));
            }
            match trigger {
                AgentAssistanceTrigger::DeterministicHardFailure
                    if item.status != ImportItemStatus::Failed =>
                {
                    return Err(task_error(
                        "Automatic Agent assistance requires a deterministic hard failure.",
                    ));
                }
                AgentAssistanceTrigger::QualityOptimization
                    if item.status != ImportItemStatus::PreviewReady =>
                {
                    return Err(task_error(
                        "Quality optimization requires a deterministic preview.",
                    ));
                }
                AgentAssistanceTrigger::Manual
                    if !matches!(
                        item.status,
                        ImportItemStatus::Failed | ImportItemStatus::PreviewReady
                    ) =>
                {
                    return Err(task_error(
                        "Manual Agent assistance requires a failed item or preview.",
                    ));
                }
                _ => {}
            }
            item.task_id = Some(task_id.to_string());
            item.attempts.push(AttemptRecord {
                route: format!("agent_assistance/{task_id}"),
                engine_id: format!("{:?}", agent_kind).to_ascii_lowercase(),
                engine_version: "detected-local-cli".into(),
                stage: ImportStage::Extract,
                started_at: chrono::Utc::now().to_rfc3339(),
                completed_at: None,
                outcome: AttemptOutcome::Failed,
                warnings: Vec::new(),
            });
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_byok_assistance(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        trigger: AgentAssistanceTrigger,
        provider: crate::models::llm::LlmProviderKind,
        max_attempts: u8,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            let attempts = item
                .attempts
                .iter()
                .filter(|attempt| {
                    attempt.route.starts_with("agent_assistance/")
                        || attempt.route.starts_with("byok_assistance/")
                })
                .collect::<Vec<_>>();
            if attempts.len() >= usize::from(max_attempts)
                || attempts.iter().any(|attempt| attempt.completed_at.is_none())
            {
                return Err(task_error("The Agent assistance attempt budget is exhausted or active."));
            }
            if !matches!(item.status, ImportItemStatus::Failed | ImportItemStatus::PreviewReady) {
                return Err(task_error("BYOK assistance requires a failed item or preview."));
            }
            item.task_id = Some(task_id.to_string());
            item.attempts.push(AttemptRecord {
                route: format!("byok_assistance/{task_id}"),
                engine_id: serde_json::to_value(provider)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "byok".into()),
                engine_version: "configured-provider".into(),
                stage: ImportStage::Extract,
                started_at: chrono::Utc::now().to_rfc3339(),
                completed_at: None,
                outcome: AttemptOutcome::Failed,
                warnings: vec![format!("trigger={trigger:?}")],
            });
            Ok(())
        })
    }

    pub fn finish_agent_assistance_attempt(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        outcome: AttemptOutcome,
        warnings: Vec<String>,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            let local_route = format!("agent_assistance/{task_id}");
            let byok_route = format!("byok_assistance/{task_id}");
            let attempt = item
                .attempts
                .iter_mut()
                .rev()
                .find(|attempt| attempt.route == local_route || attempt.route == byok_route)
                .ok_or_else(|| task_error("Agent assistance attempt was not found."))?;
            attempt.completed_at = Some(chrono::Utc::now().to_rfc3339());
            attempt.outcome = outcome;
            attempt.warnings = warnings;
            Ok(())
        })
    }

    pub fn register_agent_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        candidate: AgentCandidate,
        needs_three_way_merge: bool,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) || candidate.task_id != task_id {
                return Err(task_error("Agent candidate is not bound to this import item task."));
            }
            item.status = if needs_three_way_merge {
                ImportItemStatus::NeedsMerge
            } else {
                ImportItemStatus::PreviewReady
            };
            Ok(())
        })
    }

    pub fn begin_agent_candidate_validation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItemStatus, BackendError> {
        let mut previous = ImportItemStatus::Failed;
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error("Agent candidate validation is not bound to this item task."));
            }
            if !matches!(
                item.status,
                ImportItemStatus::Failed
                    | ImportItemStatus::PreviewReady
                    | ImportItemStatus::NeedsMerge
                    | ImportItemStatus::Validating
            ) {
                return Err(task_error("This import item cannot enter Agent candidate validation."));
            }
            previous = item.status.clone();
            item.status = ImportItemStatus::Validating;
            Ok(())
        })?;
        Ok(previous)
    }

    pub fn fail_agent_candidate_validation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        previous: ImportItemStatus,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error("Agent candidate validation failure is not bound to this item task."));
            }
            item.status = match previous {
                ImportItemStatus::PreviewReady | ImportItemStatus::NeedsMerge => previous,
                _ if item.preview.is_some() => ImportItemStatus::PreviewReady,
                _ => ImportItemStatus::Failed,
            };
            Ok(())
        })
    }

    pub fn mark_agent_candidate_rejected(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error("Rejected Agent candidate is not bound to this item task."));
            }
            let local_route = format!("agent_assistance/{task_id}");
            let byok_route = format!("byok_assistance/{task_id}");
            let attempt = item
                .attempts
                .iter_mut()
                .rev()
                .find(|attempt| attempt.route == local_route || attempt.route == byok_route)
                .ok_or_else(|| task_error("Agent assistance attempt was not found."))?;
            if !attempt
                .warnings
                .iter()
                .any(|warning| warning == "AGENT_CANDIDATE_REJECTED")
            {
                attempt.warnings.push("AGENT_CANDIDATE_REJECTED".into());
            }
            if attempt.route == byok_route
                && !attempt
                    .warnings
                    .iter()
                    .any(|warning| warning == "BYOK_CHARGE_STATUS_UNKNOWN")
            {
                attempt.warnings.push("BYOK_CHARGE_STATUS_UNKNOWN".into());
            }
            Ok(())
        })
    }

    pub fn reject_agent_candidate_validation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        previous: ImportItemStatus,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error("Rejected Agent candidate is not bound to this item task."));
            }
            item.status = match previous {
                ImportItemStatus::PreviewReady | ImportItemStatus::NeedsMerge => previous,
                _ if item.preview.is_some() => ImportItemStatus::PreviewReady,
                _ => ImportItemStatus::Failed,
            };
            let local_route = format!("agent_assistance/{task_id}");
            let byok_route = format!("byok_assistance/{task_id}");
            let attempt = item
                .attempts
                .iter_mut()
                .rev()
                .find(|attempt| attempt.route == local_route || attempt.route == byok_route)
                .ok_or_else(|| task_error("Agent assistance attempt was not found."))?;
            if !attempt
                .warnings
                .iter()
                .any(|warning| warning == "AGENT_CANDIDATE_REJECTED")
            {
                attempt.warnings.push("AGENT_CANDIDATE_REJECTED".into());
            }
            if attempt.route == byok_route
                && !attempt
                    .warnings
                    .iter()
                    .any(|warning| warning == "BYOK_CHARGE_STATUS_UNKNOWN")
            {
                attempt.warnings.push("BYOK_CHARGE_STATUS_UNKNOWN".into());
            }
            Ok(())
        })
    }

    pub fn select_agent_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        preview: crate::models::import_v2::ImportPreviewArtifact,
        needs_three_way_merge: bool,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error("Agent candidate selection is not bound to this item task."));
            }
            item.preview = Some(preview);
            item.status = if needs_three_way_merge {
                ImportItemStatus::NeedsMerge
            } else {
                ImportItemStatus::PreviewReady
            };
            Ok(())
        })
    }

    pub fn discard_agent_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        deterministic_preview: Option<crate::models::import_v2::ImportPreviewArtifact>,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error("Agent candidate discard is not bound to this item task."));
            }
            item.preview = deterministic_preview;
            item.status = if item.preview.is_some() {
                ImportItemStatus::PreviewReady
            } else {
                ImportItemStatus::Failed
            };
            let local_route = format!("agent_assistance/{task_id}");
            let byok_route = format!("byok_assistance/{task_id}");
            if let Some(attempt) = item
                .attempts
                .iter_mut()
                .rev()
                .find(|attempt| attempt.route == local_route || attempt.route == byok_route)
            {
                if !attempt
                    .warnings
                    .iter()
                    .any(|warning| warning == "AGENT_CANDIDATE_DISCARDED")
                {
                    attempt.warnings.push("AGENT_CANDIDATE_DISCARDED".into());
                }
            }
            Ok(())
        })
    }
    pub fn recover_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        for item in &mut session.items {
            for attempt in &mut item.attempts {
                let is_byok = attempt.route.starts_with("byok_assistance/");
                let Some(task_id) = attempt
                    .route
                    .strip_prefix("agent_assistance/")
                    .or_else(|| attempt.route.strip_prefix("byok_assistance/"))
                    .filter(|_| attempt.completed_at.is_none())
                    .map(str::to_owned)
                else {
                    continue;
                };
                let task_status = tasks.get_task(&task_id).map(|task| task.status);
                let charge_unknown = if is_byok {
                    let audit_path = format!(
                        ".app/import-sessions/{session_id}/items/{}/agent-audit/{task_id}.json",
                        item.item_id
                    );
                    files
                        .read_json::<crate::models::import_v2_agent::AgentAuditRecord>(
                            context,
                            &audit_path,
                        )
                        .ok()
                        .is_some_and(|audit| {
                            matches!(audit.outcome.as_str(), "send_started" | "outcome_unknown")
                        })
                } else {
                    false
                };
                if matches!(
                    task_status,
                    Some(
                        TaskStatus::Queued
                            | TaskStatus::Running
                            | TaskStatus::WaitingForConfirmation
                            | TaskStatus::Cancelling
                    )
                ) {
                    continue;
                }
                attempt.completed_at = Some(chrono::Utc::now().to_rfc3339());
                match task_status {
                    Some(TaskStatus::Succeeded) => {
                        attempt.outcome = AttemptOutcome::Succeeded;
                        attempt.warnings.clear();
                    }
                    Some(TaskStatus::Cancelled) => {
                        attempt.outcome = AttemptOutcome::Cancelled;
                        attempt.warnings = vec![
                            "Agent assistance was cancelled before recovery completed.".into(),
                        ];
                    }
                    _ => {
                        attempt.outcome = AttemptOutcome::Failed;
                        attempt.warnings = if charge_unknown {
                            vec!["BYOK_CHARGE_STATUS_UNKNOWN".into()]
                        } else {
                            vec![
                                "Interrupted Agent assistance was closed during recovery.".into(),
                            ]
                        };
                    }
                }
            }
            if matches!(
                item.status,
                ImportItemStatus::Inspecting
                    | ImportItemStatus::Extracting
                    | ImportItemStatus::Validating
            ) {
                let recovered_status = item
                    .task_id
                    .as_deref()
                    .and_then(|id| tasks.get_task(id))
                    .map(|task| task.status);
                if recovered_status == Some(TaskStatus::Cancelled) {
                    let runtime_temp = context.root.join(format!(".app/import-sessions/{session_id}/items/{}/staging/runtime-temp", item.item_id));
                    crate::services::import_v2::media_router::recover_media_temp_root(&runtime_temp)?;
                    transition_item(item, ImportItemStatus::Cancelled)?;
                    continue;
                }
                let interrupted =
                    recovered_status.is_none_or(|status| status == TaskStatus::Failed);
                if interrupted {
                    let runtime_temp = context.root.join(format!(".app/import-sessions/{session_id}/items/{}/staging/runtime-temp", item.item_id));
                    crate::services::import_v2::media_router::recover_media_temp_root(&runtime_temp)?;
                    transition_item(item, ImportItemStatus::Failed)?;
                    item.issue = Some(ImportIssue {
                        code: "TASK_RECOVERY".into(),
                        message: "Import was interrupted and can be retried.".into(),
                        stage: ImportStage::Extract,
                        retryable: true,
                        user_action_required: false,
                        recovery_actions: vec![
                            crate::models::import_v2::ImportRecoveryAction::Retry,
                            crate::models::import_v2::ImportRecoveryAction::ViewLog,
                        ],
                        available_actions: Vec::new(),
                    });
                }
            }
        }
        session.status = derive_session_status(&session.items);
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.sessions.save(context, files, &session)?;
        Ok(session)
    }
    pub fn register_engine(&self, engine: Arc<dyn ImportEngine>) -> Result<(), BackendError> {
        self.engines.register(engine)
    }

    pub fn registered_engine_routes(&self) -> Result<Vec<String>, BackendError> {
        self.engines.registered_routes()
    }

    pub fn register_capability_pack(
        &self,
        pack: ResolvedCapabilityPack,
        route: String,
        supported_extensions: Vec<String>,
        timeout: std::time::Duration,
    ) -> Result<(), BackendError> {
        self.register_engine(Arc::new(PackProcessEngine::new(
            pack,
            route,
            supported_extensions,
            timeout,
            self.web_targets.clone(),
        )))
    }
    pub fn set_item_selected(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        selected: bool,
    ) -> Result<ImportItem, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item = find_item_mut(&mut session, item_id)?;
        item.selected = selected;
        let item = item.clone();
        persist_derived(&self.sessions, context, files, session)?;
        Ok(item)
    }

    pub fn run_item(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItem, BackendError> {
        let task = tasks
            .get_task(task_id)
            .ok_or_else(|| task_error("Import task was not found."))?;
        if task.task_type != TaskType::Import
            || task.project_id.as_deref() != Some(context.project_id.as_str())
            || !matches!(task.status, TaskStatus::Queued | TaskStatus::Cancelled)
        {
            return Err(task_error("Task is not compatible with this import item."));
        }
        let pre_cancelled = tasks.is_cancelled(task_id);
        self.claim_item_for_run(context, files, session_id, item_id, task_id, pre_cancelled)?;
        if pre_cancelled {
            return Err(cancelled_error());
        }
        self.start_claimed_task(context, files, tasks, session_id, item_id, task_id)?;
        task_call(tasks.update_progress(task_id, 0, Some(4), Some("Inspecting input".into())))?;
        let input = self
            .load_session(context, files, session_id)?
            .items
            .into_iter()
            .find(|item| item.item_id == item_id)
            .ok_or_else(item_not_found)?
            .input;
        let planned_routes = self.planned_routes(&input)?;
        let engines = planned_routes
            .iter()
            .filter_map(|attempt| {
                let route_input = route_resolution_input(attempt.0, &input);
                self.engines
                    .resolve_route(attempt.0, &route_input)
                    .ok()
                    .map(|engine| (attempt, engine))
            })
            .collect::<Vec<_>>();
        if engines.is_empty() {
            let error = BackendError::new(
                crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE,
                "No planned import route is installed.",
                true,
                true,
            );
            self.mutate_item(context, files, session_id, item_id, |item| {
                transition_item(item, ImportItemStatus::WaitingCapability)?;
                item.issue = Some(issue_from_engine_error(&error, ImportStage::Route));
                Ok(())
            })?;
            task_call(tasks.append_log(
                task_id,
                LogLevel::Warn,
                "No available import engine supports this input.".into(),
            ))?;
            task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
            return Err(error);
        }
        self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::Extracting)
        })?;
        task_call(tasks.update_progress(task_id, 1, Some(4), Some("Extracting source".into())))?;
        let staging_root = format!(".app/import-sessions/{session_id}/items/{item_id}/staging");
        let local_asr_authorized = if input
            .normalized_locator
            .as_deref()
            .is_some_and(is_bilibili_url)
        {
            let exact = self.web_targets.resolve(
                &input.locator,
                input.normalized_locator.as_deref(),
            )?;
            let route_available = self.engines.registered_routes()?.iter().any(|route| route == "media.asr");
            route_available && self.web_targets
                .has_bilibili_asr(
                    &context.project_id,
                    session_id,
                    item_id,
                    exact.request_url.as_str(),
                )?
        } else {
            false
        };
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            project_id: context.project_id.clone(),
            session_id: session_id.into(),
            item_id: item_id.into(),
            task_id: task_id.into(),
            operation: EngineOperation::Extract,
            input,
            project_root: context.root.to_string_lossy().into_owned(),
            staging_root: staging_root.clone(),
            chained_input: None,
            local_asr_authorized,
        };
        let token = tasks
            .get_cancellation_token(task_id)
            .ok_or_else(|| task_error("Task cancellation state is unavailable."))?;
        let mut selected = None;
        let mut last_error = None;
        let mut request = request;
        for ((_, quality_floor), engine) in engines {
            let descriptor = engine.descriptor();
            if is_capability_route(&descriptor.route) && request.input.source_identity.is_some() {
                request.input =
                    materialize_capability_input(context, &staging_root, &request.input)?;
            }
            let started_at = chrono::Utc::now().to_rfc3339();
            let mut candidate = match engine.execute(&request, &token) {
                Ok(result) if !token.is_cancelled() => result,
                Ok(_) => {
                    return self
                        .finish_cancelled(context, files, tasks, session_id, item_id, task_id)
                }
                Err(_) if token.is_cancelled() => {
                    return self
                        .finish_cancelled(context, files, tasks, session_id, item_id, task_id)
                }
                Err(error) => {
                    self.record_attempt(
                        context,
                        files,
                        session_id,
                        item_id,
                        &descriptor,
                        started_at,
                        crate::models::import_v2::AttemptOutcome::Failed,
                        Vec::new(),
                    )?;
                    if is_web_user_wait(&error) {
                        return self.finish_waiting_login(
                            context,
                            files,
                            tasks,
                            session_id,
                            item_id,
                            task_id,
                            error,
                            ImportStage::Extract,
                        );
                    }
                    if is_non_fallback_error(&error) {
                        return self.finish_failed(
                            context,
                            files,
                            tasks,
                            session_id,
                            item_id,
                            task_id,
                            error,
                            ImportStage::Extract,
                        );
                    }
                    let route_record = FileRoutePlanner::record(
                        descriptor.route.clone(),
                        RouteOutcome::Failed(classify_route_failure(&error)),
                    );
                    last_error = Some(error);
                    if route_record.allows_fallback() {
                        continue;
                    }
                    break;
                }
            };
            if let Err(error) = validate_engine_result(&staging_root, &candidate) {
                self.record_attempt(
                    context,
                    files,
                    session_id,
                    item_id,
                    &descriptor,
                    started_at.clone(),
                    crate::models::import_v2::AttemptOutcome::Failed,
                    candidate.warnings.clone(),
                )?;
                last_error = Some(error);
                continue;
            }
            if candidate.continuation.is_some() {
                candidate = match self.execute_local_asr_continuation(
                    context,
                    files,
                    session_id,
                    item_id,
                    &staging_root,
                    &request,
                    candidate,
                    &token,
                ) {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        if token.is_cancelled() || error.code == crate::errors::IMPORT_V2_CANCELLED {
                            return self.finish_cancelled(context, files, tasks, session_id, item_id, task_id);
                        }
                        return self.finish_failed(
                            context,
                            files,
                            tasks,
                            session_id,
                            item_id,
                            task_id,
                            error,
                            ImportStage::Extract,
                        );
                    }
                };
            }
            if let Err(error) = validate_engine_result(&staging_root, &candidate) {
                self.record_attempt(
                    context,
                    files,
                    session_id,
                    item_id,
                    &descriptor,
                    started_at.clone(),
                    crate::models::import_v2::AttemptOutcome::Failed,
                    candidate.warnings.clone(),
                )?;
                last_error = Some(error);
                continue;
            }
            // Attempt-level precheck selects a candidate; the formal QualityGate still runs once.
            let required_coverage = quality_floor.requirements().minimum_text_coverage as f64;
            if !candidate_meets_floor(&request.input, &candidate, *quality_floor) {
                self.record_attempt(
                    context,
                    files,
                    session_id,
                    item_id,
                    &descriptor,
                    started_at.clone(),
                    crate::models::import_v2::AttemptOutcome::Failed,
                    candidate.warnings.clone(),
                )?;
                last_error = Some(BackendError::new(
                    crate::errors::IMPORT_V2_QUALITY_FAILED,
                    "Candidate failed the route precheck.",
                    true,
                    true,
                ));
                let record = FileRoutePlanner::record(
                    descriptor.route.clone(),
                    RouteOutcome::QualityRejected {
                        actual: candidate.text_coverage.unwrap_or_default() as f32,
                        required: required_coverage as f32,
                    },
                );
                if record.allows_fallback() {
                    continue;
                }
            }
            if descriptor.route == "pack.office-legacy" {
                if let Some(converted) = candidate
                    .asset_paths
                    .iter()
                    .find(|path| path.starts_with("converted/"))
                {
                    request.chained_input = Some(converted.clone());
                    continue;
                }
            }
            selected = Some((descriptor, started_at, candidate));
            break;
        }
        let Some((descriptor, started_at, result)) = selected else {
            return self.finish_failed(
                context,
                files,
                tasks,
                session_id,
                item_id,
                task_id,
                last_error.unwrap_or_else(|| {
                    BackendError::new(
                        crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE,
                        "No planned route produced a candidate.",
                        true,
                        true,
                    )
                }),
                ImportStage::Extract,
            );
        };
        self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::Validating)
        })?;
        task_call(tasks.update_progress(task_id, 3, Some(4), Some("Validating preview".into())))?;
        let preview = match self
            .quality
            .evaluate(&context.root.join(Path::new(&staging_root)), &result)
        {
            Ok(preview) => preview,
            Err(error) => {
                return self.finish_failed(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    error,
                    ImportStage::Validate,
                )
            }
        };
        let item = self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::PreviewReady)?;
            item.preview = Some(preview);
            item.issue = None;
            item.attempts.push(crate::models::import_v2::AttemptRecord {
                route: descriptor.route.clone(),
                engine_id: descriptor.engine_id.clone(),
                engine_version: descriptor.engine_version.clone(),
                stage: ImportStage::Validate,
                started_at,
                completed_at: Some(chrono::Utc::now().to_rfc3339()),
                outcome: crate::models::import_v2::AttemptOutcome::Succeeded,
                warnings: result.warnings.clone(),
            });
            Ok(())
        })?;
        task_call(tasks.update_progress(task_id, 4, Some(4), Some("Preview ready".into())))?;
        task_call(tasks.set_result(
            task_id,
            TaskResult {
                summary: "Import preview ready.".into(),
                affected_paths: Vec::new(),
                reference: Some(TaskResultReference::ImportPreview {
                    session_id: session_id.into(),
                    item_id: item_id.into(),
                }),
                pending_action: None,
            },
        ))?;
        task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
        Ok(item)
    }

    fn execute_local_asr_continuation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        staging_root: &str,
        request: &EngineRequest,
        mut web_result: EngineResult,
        token: &crate::tasks::task_model::CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        let Some(EngineContinuation::LocalAsr { temporary_input_path, .. }) = web_result.continuation.take() else {
            return Ok(web_result);
        };
        let staging = context.root.join(staging_root);
        let media_path = staging.join(&temporary_input_path);
        let canonical_staging = staging.canonicalize().map_err(|_| asr_unavailable())?;
        let canonical_runtime_temp = staging.join("runtime-temp").canonicalize().map_err(|_| asr_unavailable())?;
        let canonical_media = media_path.canonicalize().map_err(|_| asr_unavailable())?;
        let media_workspace = canonical_media.parent().ok_or_else(asr_unavailable)?;
        if !canonical_media.starts_with(&canonical_staging)
            || !canonical_media.starts_with(&canonical_runtime_temp)
            || media_workspace == canonical_runtime_temp
            || !canonical_media.is_file()
        {
            return Err(asr_unavailable());
        }
        let _cleanup = crate::services::import_v2::media_router::TemporaryMediaWorkspace::create(media_workspace)?;
        let asr_input = ImportInput {
            kind: crate::models::import_v2::ImportInputKind::File,
            display_name: canonical_media.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            locator: canonical_media.to_string_lossy().into_owned(),
            normalized_locator: None,
            source_identity: None,
        };
        let engine = self.engines.resolve_route("media.asr", &asr_input)?;
        let exact = self.web_targets.resolve(&request.input.locator, request.input.normalized_locator.as_deref())?;
        if self.web_targets.take_bilibili_asr(&request.project_id, &request.session_id, &request.item_id, exact.request_url.as_str())?.is_none() {
            return Err(asr_unavailable());
        }
        let descriptor = engine.descriptor();
        let started_at = chrono::Utc::now().to_rfc3339();
        let mut asr_request = request.clone();
        asr_request.request_id = uuid::Uuid::new_v4().to_string();
        asr_request.input = asr_input;
        asr_request.chained_input = None;
        let outcome = (|| -> Result<(EngineResult, Vec<String>), BackendError> {
            let asr_result = engine.execute(&asr_request, token)?;
            validate_engine_result(staging_root, &asr_result)?;
            let output_path = staging.join(&asr_result.markdown_path).canonicalize().map_err(|_| asr_unavailable())?;
            let output_workspace = output_path.parent().ok_or_else(asr_unavailable)?;
            if !output_path.starts_with(&canonical_runtime_temp) || output_workspace == canonical_runtime_temp {
                return Err(asr_unavailable());
            }
            let _output_cleanup = crate::services::import_v2::media_router::TemporaryMediaWorkspace::create(output_workspace)?;
            if asr_result.continuation.is_some()
                || asr_result.markdown_path == web_result.markdown_path
                || asr_result.source_snapshot_path == web_result.source_snapshot_path
            {
                return Err(asr_unavailable());
            }
            let base_path = staging.join(&web_result.markdown_path);
            let transcript_path = staging.join(&asr_result.markdown_path);
            let mut base = std::fs::read_to_string(&base_path).map_err(|_| asr_unavailable())?;
            let transcript = std::fs::read_to_string(&transcript_path).map_err(|_| asr_unavailable())?;
            base.push_str("\n\n## Local ASR Transcript\n\n");
            base.push_str(&transcript);
            std::fs::write(&base_path, base).map_err(|_| asr_unavailable())?;
            for relative in std::iter::once(&asr_result.markdown_path)
                .chain(std::iter::once(&asr_result.source_snapshot_path))
                .chain(asr_result.metadata_path.iter())
                .chain(asr_result.asset_paths.iter())
            {
                let _ = std::fs::remove_file(staging.join(relative));
            }
            web_result.warnings.push("local_asr:whisper.cpp-1.8.3:ggml-small".into());
            Ok((web_result, asr_result.warnings))
        })();
        let (outcome_kind, warnings) = match &outcome {
            Ok((_, warnings)) => (crate::models::import_v2::AttemptOutcome::Succeeded, warnings.clone()),
            Err(_) => (crate::models::import_v2::AttemptOutcome::Failed, Vec::new()),
        };
        self.record_attempt(
            context,
            files,
            session_id,
            item_id,
            &descriptor,
            started_at,
            outcome_kind,
            warnings,
        )?;
        outcome.map(|(result, _)| result)
    }

    fn planned_routes(
        &self,
        input: &ImportInput,
    ) -> Result<Vec<(&'static str, QualityFloor)>, BackendError> {
        let extension = Path::new(&input.locator)
            .extension()
            .and_then(|v| v.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let format = match extension.as_str() {
            "doc" => Some(FileFormat::Doc),
            "docx" => Some(FileFormat::Docx),
            "xls" => Some(FileFormat::Xls),
            "xlsx" => Some(FileFormat::Xlsx),
            "ppt" => Some(FileFormat::Ppt),
            "pptx" => Some(FileFormat::Pptx),
            _ => None,
        };
        let Some(format) = format else {
            return Ok(explicit_routes(input)
                .into_iter()
                .map(|route| {
                    let floor = match route {
                        "pdf.text" | "pdf.layout" => QualityFloor::DeterministicDocument,
                        _ => QualityFloor::ComparisonFallback,
                    };
                    (route, floor)
                })
                .collect());
        };
        let routes = self.engines.registered_routes()?;
        let has = |route: &str| routes.iter().any(|registered| registered == route);
        let capabilities = CapabilitySnapshot {
            document_standard: has("pack.markitdown"),
            office_legacy: has("pack.office-legacy"),
            office_oxide_installed: has("pack.office-oxide"),
            office_oxide_qualified: has("pack.office-oxide"),
            agent_available: has("agent.office"),
        };
        Ok(FileRoutePlanner::plan(format, capabilities)
            .into_iter()
            .map(|attempt| (attempt.route, attempt.quality_floor))
            .collect())
    }

    fn record_attempt(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        descriptor: &crate::services::import_v2::engine::EngineDescriptor,
        started_at: String,
        outcome: crate::models::import_v2::AttemptOutcome,
        warnings: Vec<String>,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            item.attempts.push(crate::models::import_v2::AttemptRecord {
                route: descriptor.route.clone(),
                engine_id: descriptor.engine_id.clone(),
                engine_version: descriptor.engine_version.clone(),
                stage: ImportStage::Extract,
                started_at,
                completed_at: Some(chrono::Utc::now().to_rfc3339()),
                outcome,
                warnings,
            });
            Ok(())
        })
    }

    fn claim_item_for_run(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        pre_cancelled: bool,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if !matches!(
                item.status,
                ImportItemStatus::Queued | ImportItemStatus::Failed
            ) || item
                .task_id
                .as_deref()
                .is_some_and(|bound| bound != task_id && item.status != ImportItemStatus::Failed)
            {
                return Err(task_error(
                    "Import item is already claimed by another task.",
                ));
            }
            item.task_id = Some(task_id.to_string());
            item.issue = None;
            transition_item(
                item,
                if pre_cancelled {
                    ImportItemStatus::Cancelled
                } else {
                    ImportItemStatus::Inspecting
                },
            )
        })
    }

    fn start_claimed_task(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<(), BackendError> {
        if tasks.is_cancelled(task_id) {
            return self
                .finish_cancelled(context, files, tasks, session_id, item_id, task_id)
                .map(|_| ());
        }
        if let Err(error) = tasks.transition_status(task_id, TaskStatus::Running) {
            if tasks.is_cancelled(task_id)
                || tasks
                    .get_task(task_id)
                    .is_some_and(|task| task.status == TaskStatus::Cancelled)
            {
                return self
                    .finish_cancelled(context, files, tasks, session_id, item_id, task_id)
                    .map(|_| ());
            }
            return Err(task_call::<()>(Err(error)).unwrap_err());
        }
        Ok(())
    }

    fn finish_cancelled(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::Cancelled)
        })?;
        if tasks
            .get_task(task_id)
            .is_some_and(|task| task.status != TaskStatus::Cancelled)
        {
            task_call(tasks.cancel_task(task_id))?;
        }
        Err(cancelled_error())
    }
    fn finish_failed(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        error: BackendError,
        stage: ImportStage,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::Failed)?;
            let mut issue = issue_from_engine_error(&error, stage);
            if is_agent_eligible_failure(&error.code, &issue) {
                issue.available_actions = vec![
                    crate::models::import_v2_agent::AgentRecoveryAction::InvokeLocalAgent,
                    crate::models::import_v2_agent::AgentRecoveryAction::RequestByok,
                ];
            }
            item.issue = Some(issue);
            Ok(())
        })?;
        task_call(tasks.append_log(task_id, LogLevel::Error, "Import engine failed.".into()))?;
        task_call(tasks.set_error(task_id, issue_safe_error(&error)))?;
        task_call(tasks.transition_status(task_id, TaskStatus::Failed))?;
        Err(issue_safe_error(&error))
    }
    fn finish_waiting_login(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        error: BackendError,
        stage: ImportStage,
    ) -> Result<ImportItem, BackendError> {
        let item = self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::WaitingLogin)?;
            item.issue = Some(ImportIssue::for_web_code(&error.code, stage));
            Ok(())
        })?;
        task_call(tasks.append_log(
            task_id,
            LogLevel::Warn,
            "Web import is waiting for user authentication.".into(),
        ))?;
        task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
        Ok(item)
    }
    fn mutate_item<F>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        mutation: F,
    ) -> Result<ImportItem, BackendError>
    where
        F: FnOnce(&mut ImportItem) -> Result<(), BackendError>,
    {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item = find_item_mut(&mut session, item_id)?;
        mutation(item)?;
        let item = item.clone();
        persist_derived(&self.sessions, context, files, session)?;
        Ok(item)
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, BackendError> {
        self.mutation_lock
            .lock()
            .map_err(|_| task_error("Import session mutation lock is unavailable."))
    }

    fn preflight_locked(&self, context: &ProjectContext) -> Result<(), BackendError> {
        FileTransaction::reconcile_project(&context.root)
    }

    pub(crate) fn acquire_migration_lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, ()>, BackendError> {
        self.lock()
    }

    pub(crate) fn preflight_migration_locked(
        &self,
        context: &ProjectContext,
    ) -> Result<(), BackendError> {
        self.preflight_locked(context)
    }
}

fn is_agent_eligible_failure(original_code: &str, issue: &ImportIssue) -> bool {
    let original_is_eligible = matches!(
        original_code,
        crate::errors::IMPORT_V2_ENGINE_OUTPUT_INVALID
            | crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE
            | crate::errors::IMPORT_V2_CAPABILITY_UNAVAILABLE
            | crate::errors::IMPORT_V2_QUALITY_FAILED
            | "IMPORT_WEB_STRUCTURE_CHANGED"
            | "IMPORT_WEB_SUBTITLE_UNAVAILABLE"
            | "IMPORT_ASR_TIMEOUT"
            | "IMPORT_ASR_ENGINE_FAILED"
            | "IMPORT_ASR_OUTPUT_INVALID"
    ) || original_code.contains("PARSE")
        || original_code.contains("CORRUPT")
        || original_code.contains("CONVERSION");
    original_is_eligible
        && matches!(
            issue.code.as_str(),
            "IMPORT_FILE_PARSE_FAILED"
                | "IMPORT_FILE_CORRUPT"
                | "IMPORT_FILE_CONVERSION_FAILED"
                | "IMPORT_FILE_QUALITY_FAILED"
                | "IMPORT_FILE_CAPABILITY_MISSING"
                | "IMPORT_WEB_STRUCTURE_CHANGED"
                | "IMPORT_WEB_SUBTITLE_UNAVAILABLE"
                | "IMPORT_ASR_TIMEOUT"
                | "IMPORT_ASR_ENGINE_FAILED"
                | "IMPORT_ASR_OUTPUT_INVALID"
        )
}

fn explicit_routes(input: &ImportInput) -> Vec<&'static str> {
    if input.kind == crate::models::import_v2::ImportInputKind::Url {
        let host = url::Url::parse(
            input
                .normalized_locator
                .as_deref()
                .unwrap_or(&input.locator),
        )
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
        if host == "xiaohongshu.com"
            || host.ends_with(".xiaohongshu.com")
            || host == "x.com"
            || host.ends_with(".x.com")
            || host == "twitter.com"
            || host.ends_with(".twitter.com")
        {
            return Vec::new();
        }
        if host == "bilibili.com" || host.ends_with(".bilibili.com") || host == "b23.tv" {
            return vec![
                "web.bilibili.metadata",
                "web.bilibili.video",
                "web.generic.browser",
            ];
        }
        let platform = if host == "mp.weixin.qq.com" {
            Some("web.wechat.article")
        } else if host == "zhihu.com" || host.ends_with(".zhihu.com") {
            Some("web.zhihu.content")
        } else {
            None
        };
        let mut routes = platform.into_iter().collect::<Vec<_>>();
        routes.extend(["web.generic.readability", "web.generic.browser"]);
        return routes;
    }
    let extension = Path::new(&input.locator)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown" | "txt" | "csv" | "html" | "htm" => vec!["file.native"],
        "docx" => vec![
            "office.modern.docx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        "xlsx" => vec![
            "office.modern.xlsx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        "pptx" => vec![
            "office.modern.pptx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        "doc" | "xls" | "ppt" => vec!["pack.office-legacy", "pack.office-oxide", "agent.office"],
        "pdf" => vec![
            "pdf.text",
            "pdf.layout",
            "ocr.cjk-accurate",
            "ocr.basic",
            "agent.pdf",
        ],
        "srt" | "vtt" | "lrc" | "ass" | "ssa" => vec!["media.subtitle"],
        "mp3" | "wav" | "m4a" | "mp4" | "mov" | "mkv" => vec!["media.asr"],
        _ => Vec::new(),
    }
}

fn is_bilibili_url(value: &str) -> bool {
    url::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| {
            host == "b23.tv" || host == "bilibili.com" || host.ends_with(".bilibili.com")
        })
}

fn asr_unavailable() -> BackendError {
    BackendError::new(
        "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
        "A verified subtitle is unavailable and local ASR could not complete.",
        true,
        true,
    )
}

fn is_capability_route(route: &str) -> bool {
    route.starts_with("pack.")
        || route.starts_with("pdf.")
        || route.starts_with("ocr.")
        || route.starts_with("media.")
}

fn is_non_fallback_error(error: &BackendError) -> bool {
    error.code == crate::errors::IMPORT_V2_CANCELLED
        || error.code.contains("PASSWORD")
        || error.code.contains("LOGIN")
}
fn is_web_user_wait(error: &BackendError) -> bool {
    matches!(
        error.code.as_str(),
        "IMPORT_WEB_LOGIN_REQUIRED"
            | "IMPORT_WEB_CHALLENGE_DETECTED"
            | "IMPORT_WEB_CAPTCHA_REQUIRED"
    )
}

fn classify_route_failure(
    error: &BackendError,
) -> crate::services::import_v2::file_router::RouteFailure {
    use crate::services::import_v2::file_router::RouteFailure;
    if error.code.contains("CAPABILITY") || error.code.contains("UNAVAILABLE") {
        RouteFailure::CapabilityUnavailable
    } else if error.code.contains("CORRUPT") || error.code.contains("PARSE") {
        RouteFailure::CorruptInput
    } else if error.code.contains("RESOURCE") || error.code.contains("LIMIT") {
        RouteFailure::ResourceLimit
    } else if error.code.contains("UNSUPPORTED") {
        RouteFailure::UnsupportedFeature {
            feature: error.code.clone(),
        }
    } else {
        RouteFailure::EngineFailure {
            code: error.code.clone(),
        }
    }
}

fn candidate_meets_floor(
    input: &ImportInput,
    result: &crate::services::import_v2::engine::EngineResult,
    floor: QualityFloor,
) -> bool {
    let requirements = floor.requirements();
    if result
        .text_coverage
        .is_some_and(|coverage| coverage < requirements.minimum_text_coverage as f64)
    {
        return false;
    }
    if floor != QualityFloor::ModernOffice {
        return true;
    }
    let extension = Path::new(&input.locator)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "xls" | "xlsx" => {
            result.sheet_count_exact == Some(1.0)
                && result
                    .non_empty_cell_coverage
                    .is_some_and(|value| value >= 0.95)
                && result.formula_value_pairs == Some(1.0)
        }
        "ppt" | "pptx" => {
            result.slide_count_exact == Some(1.0)
                && result
                    .meaningful_image_coverage
                    .is_some_and(|value| value >= 0.95)
        }
        _ => true,
    }
}

fn materialize_capability_input(
    context: &ProjectContext,
    staging_root: &str,
    input: &ImportInput,
) -> Result<ImportInput, BackendError> {
    let identity = input.source_identity.as_ref().ok_or_else(|| {
        BackendError::new(
            "IMPORT_FILE_SOURCE_CHANGED",
            "The selected source must be scanned again before capability execution.",
            true,
            true,
        )
    })?;
    let source = Path::new(&input.locator);
    let bytes = crate::services::import_v2::native_file_engine::safe_read_source(source, identity)?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bin");
    let authorized_relative = format!("{staging_root}/authorized");
    let authorized_root = context.resolve_project_path(&authorized_relative)?;
    reset_authorized_directory(&context.root, &authorized_root)?;
    let relative = format!("{authorized_relative}/source.{extension}");
    let destination = context.resolve_project_path(&relative)?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| {
            BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
        })?;
    std::io::Write::write_all(&mut output, &bytes).map_err(|error| {
        BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
    })?;
    output.sync_all().map_err(|error| {
        BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
    })?;
    let mut authorized = input.clone();
    authorized.locator = destination.to_string_lossy().into_owned();
    authorized.normalized_locator = Some(relative.replace('\\', "/"));
    authorized.source_identity = None;
    Ok(authorized)
}

fn reset_authorized_directory(
    project_root: &Path,
    authorized_root: &Path,
) -> Result<(), BackendError> {
    let parent = authorized_root.parent().ok_or_else(|| {
        BackendError::new(
            "IMPORT_FILE_STAGE_FAILED",
            "The staging path is invalid.",
            false,
            false,
        )
    })?;
    let relative_parent = parent.strip_prefix(project_root).map_err(|_| {
        BackendError::new(
            "IMPORT_FILE_STAGE_FAILED",
            "The staging path escaped the project.",
            false,
            false,
        )
    })?;
    let mut current = project_root.to_path_buf();
    for component in relative_parent.components() {
        current.push(component.as_os_str());
        if let Ok(metadata) = std::fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() || staging_reparse(&metadata) {
                return Err(BackendError::new(
                    "IMPORT_FILE_STAGE_FAILED",
                    "The staging path contains a link or reparse point.",
                    false,
                    true,
                ));
            }
        }
    }
    if let Ok(metadata) = std::fs::symlink_metadata(authorized_root) {
        if metadata.file_type().is_symlink() || staging_reparse(&metadata) || !metadata.is_dir() {
            return Err(BackendError::new(
                "IMPORT_FILE_STAGE_FAILED",
                "The authorized staging directory is not a regular directory.",
                false,
                true,
            ));
        }
        std::fs::remove_dir_all(authorized_root).map_err(|error| {
            BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
        })?;
    }
    std::fs::create_dir_all(authorized_root).map_err(|error| {
        BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
    })
}

#[cfg(windows)]
fn staging_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn staging_reparse(_: &std::fs::Metadata) -> bool {
    false
}

fn route_resolution_input(route: &str, input: &ImportInput) -> ImportInput {
    let original_extension = Path::new(&input.locator)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = match route {
        "office.modern.docx" => Some("docx"),
        "office.modern.xlsx" => Some("xlsx"),
        "office.modern.pptx" => Some("pptx"),
        "pack.markitdown" => match original_extension.as_str() {
            "doc" => Some("docx"),
            "xls" => Some("xlsx"),
            "ppt" => Some("pptx"),
            _ => None,
        },
        _ => None,
    };
    let Some(extension) = extension else {
        return input.clone();
    };
    let mut routed = input.clone();
    routed.locator = format!("converted.{extension}");
    routed.display_name = routed.locator.clone();
    routed
}

fn persist_derived(
    store: &SessionStore,
    context: &ProjectContext,
    files: &FileStore,
    mut session: ImportSession,
) -> Result<(), BackendError> {
    session.status = derive_session_status(&session.items);
    session.updated_at = chrono::Utc::now().to_rfc3339();
    store.save(context, files, &session)
}
fn find_item_mut<'a>(
    session: &'a mut ImportSession,
    item_id: &str,
) -> Result<&'a mut ImportItem, BackendError> {
    session
        .items
        .iter_mut()
        .find(|item| item.item_id == item_id)
        .ok_or_else(item_not_found)
}
fn item_not_found() -> BackendError {
    BackendError::new(
        IMPORT_V2_ITEM_NOT_FOUND,
        "Import session item was not found.",
        true,
        false,
    )
}
fn task_error(message: &str) -> BackendError {
    BackendError::new(IMPORT_V2_STATE_INVALID, message, true, false)
}
fn task_call<T>(result: Result<T, String>) -> Result<T, BackendError> {
    result.map_err(|_| task_error("Import task state could not be updated."))
}
fn cancelled_error() -> BackendError {
    BackendError::new(IMPORT_V2_CANCELLED, "Import was cancelled.", true, false)
}
fn issue_safe_error(error: &BackendError) -> BackendError {
    BackendError::new(
        error.code.clone(),
        "Import engine failed.",
        error.recoverable,
        error.user_action_required,
    )
}

fn transition_item(item: &mut ImportItem, next: ImportItemStatus) -> Result<(), BackendError> {
    if !item.status.can_transition_to(&next) {
        return Err(BackendError::new(
            IMPORT_V2_STATE_INVALID,
            format!(
                "Invalid import item transition: {:?} -> {:?}",
                item.status, next
            ),
            false,
            true,
        ));
    }
    item.status = next;
    Ok(())
}
fn issue_from_engine_error(error: &BackendError, stage: ImportStage) -> ImportIssue {
    if error.code.starts_with("IMPORT_WEB_")
        || error.code.starts_with("IMPORT_ASR_")
        || error.code.starts_with("IMPORT_V2_URL_")
        || error.code.starts_with("IMPORT_V2_REDIRECT_")
        || error.code.starts_with("IMPORT_V2_RESPONSE_")
        || error.code == "IMPORT_V2_CONNECTOR_RATE_LIMITED"
    {
        return ImportIssue::for_web_code(&error.code, stage);
    }
    let code = stable_file_error_code(&error.code);
    ImportIssue::for_file_code(code, stage)
}

fn stable_file_error_code(code: &str) -> &'static str {
    match code {
        crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE
        | crate::errors::IMPORT_V2_CAPABILITY_UNAVAILABLE => "IMPORT_FILE_CAPABILITY_MISSING",
        crate::errors::IMPORT_V2_CANCELLED => "IMPORT_FILE_CANCELLED",
        crate::errors::IMPORT_V2_QUALITY_FAILED => "IMPORT_FILE_QUALITY_FAILED",
        crate::errors::IMPORT_V2_ENGINE_OUTPUT_INVALID => "IMPORT_FILE_PARSE_FAILED",
        _ if code.contains("PASSWORD") => "IMPORT_FILE_PASSWORD_REQUIRED",
        _ if code.contains("CORRUPT") => "IMPORT_FILE_CORRUPT",
        _ if code.contains("RESOURCE") || code.contains("LIMIT") => "IMPORT_FILE_RESOURCE_LIMIT",
        _ if code.contains("CONVERSION") => "IMPORT_FILE_CONVERSION_FAILED",
        _ => "IMPORT_FILE_PARSE_FAILED",
    }
}
pub(super) fn derive_session_status(items: &[ImportItem]) -> ImportSessionStatus {
    use ImportItemStatus::*;
    let has =
        |statuses: &[ImportItemStatus]| items.iter().any(|item| statuses.contains(&item.status));
    if has(&[Inspecting, Extracting, Validating, Committing]) {
        ImportSessionStatus::Processing
    } else if has(&[Completed]) && has(&[Failed, Cancelled]) {
        ImportSessionStatus::PartiallyCommitted
    } else if has(&[Completed])
        && items
            .iter()
            .all(|item| matches!(item.status, Completed | Skipped))
    {
        ImportSessionStatus::Completed
    } else if !items.is_empty() && items.iter().all(|item| item.status == Cancelled) {
        ImportSessionStatus::Cancelled
    } else if has(&[
        PreviewReady,
        NeedsMerge,
        WaitingCapability,
        WaitingLogin,
        Failed,
    ]) {
        ImportSessionStatus::WaitingForConfirmation
    } else {
        ImportSessionStatus::Draft
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Condvar, Mutex};

    use crate::errors::{
        BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_UNAVAILABLE, IMPORT_V2_STATE_INVALID,
    };
    use crate::models::import_v2::{
        ImportInput, ImportItem, ImportItemStatus, ImportResourceMode, ImportSession,
        ImportSessionStatus, ImportStage,
    };
    use crate::models::paths::ProjectContext;
    use crate::models::task::{BackendTask, TaskStatus, TaskType};
    use crate::services::import_v2::engine::{
        EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
    };
    use crate::services::import_v2::test_support::{test_context, test_file_input};
    use crate::services::FileStore;
    use crate::tasks::task_model::CancellationToken;
    use crate::tasks::TaskService;

    use super::*;

    #[test]
    fn agent_eligibility_uses_stable_issue_codes_and_excludes_access_failures() {
        let invalid = BackendError::new(
            crate::errors::IMPORT_V2_ENGINE_OUTPUT_INVALID,
            "invalid",
            true,
            false,
        );
        let stable = issue_from_engine_error(&invalid, ImportStage::Extract);
        assert_eq!(stable.code, "IMPORT_FILE_PARSE_FAILED");
        assert!(is_agent_eligible_failure(&invalid.code, &stable));
        assert!(!is_agent_eligible_failure(
            "IMPORT_WEB_LOGIN_REQUIRED",
            &ImportIssue::for_web_code("IMPORT_WEB_LOGIN_REQUIRED", ImportStage::Extract),
        ));
        assert!(!is_agent_eligible_failure(
            "IMPORT_FILE_PASSWORD_REQUIRED",
            &ImportIssue::for_file_code("IMPORT_FILE_PASSWORD_REQUIRED", ImportStage::Extract),
        ));
        assert!(!is_agent_eligible_failure(
            "PATH_OUTSIDE_PROJECT",
            &ImportIssue::for_file_code("IMPORT_FILE_PARSE_FAILED", ImportStage::Extract),
        ));
    }

    #[test]
    fn authorized_staging_rejects_a_residual_non_directory() {
        let root = std::env::temp_dir().join(format!("authorized-stage-{}", uuid::Uuid::new_v4()));
        let authorized = root.join(".app/import-staging/session/item/authorized");
        std::fs::create_dir_all(authorized.parent().unwrap()).unwrap();
        std::fs::write(&authorized, b"must not be followed or overwritten").unwrap();
        let error = reset_authorized_directory(&root, &authorized).unwrap_err();
        assert_eq!(error.code, "IMPORT_FILE_STAGE_FAILED");
        assert_eq!(
            std::fs::read(&authorized).unwrap(),
            b"must not be followed or overwritten"
        );
        std::fs::remove_dir_all(root).ok();
    }

    struct FixtureEngine {
        project_root: PathBuf,
        markdown: &'static [u8],
    }
    impl FixtureEngine {
        fn success(project_root: PathBuf) -> Self {
            Self {
                project_root,
                markdown: b"# Fixture\n\nBody",
            }
        }
    }
    impl ImportEngine for FixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "fixture".into(),
                engine_version: "1.0.0".into(),
                route: "pdf.text".into(),
            }
        }
        fn supports(&self, _input: &ImportInput) -> bool {
            true
        }
        fn execute(
            &self,
            request: &EngineRequest,
            _cancellation: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            let root = self.project_root.join(
                request
                    .staging_root
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("source.bin"), b"source").unwrap();
            std::fs::write(root.join("candidate.md"), self.markdown).unwrap();
            Ok(EngineResult {
                source_snapshot_path: "source.bin".into(),
                markdown_path: "candidate.md".into(),
                asset_paths: Vec::new(),
                metadata_path: None,
                title: "Fixture".into(),
                text_coverage: Some(1.0),
                table_cell_accuracy: None,
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
                continuation: None,
                warnings: Vec::new(),
            })
        }
    }

    struct BlockingEngine {
        inner: FixtureEngine,
        entered: Arc<(Mutex<bool>, Condvar)>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }
    impl ImportEngine for BlockingEngine {
        fn descriptor(&self) -> EngineDescriptor {
            self.inner.descriptor()
        }
        fn supports(&self, input: &ImportInput) -> bool {
            self.inner.supports(input)
        }
        fn execute(
            &self,
            request: &EngineRequest,
            cancellation: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            let (lock, signal) = &*self.entered;
            *lock.lock().unwrap() = true;
            signal.notify_all();
            let (lock, signal) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = signal.wait(released).unwrap();
            }
            self.inner.execute(request, cancellation)
        }
    }

    struct FailingEngine {
        root: PathBuf,
        sabotage_task_store: bool,
    }

    struct RouteFixtureEngine {
        root: PathBuf,
        id: &'static str,
        route: &'static str,
        coverage: f64,
    }
    impl ImportEngine for RouteFixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: self.id.into(),
                engine_version: "1".into(),
                route: self.route.into(),
            }
        }
        fn supports(&self, _: &ImportInput) -> bool {
            true
        }
        fn execute(
            &self,
            request: &EngineRequest,
            _: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            let staging = self.root.join(
                request
                    .staging_root
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            std::fs::create_dir_all(&staging).unwrap();
            std::fs::write(staging.join("source.bin"), b"source").unwrap();
            std::fs::write(staging.join("candidate.md"), b"# Candidate\n\nBody").unwrap();
            Ok(EngineResult {
                source_snapshot_path: "source.bin".into(),
                markdown_path: "candidate.md".into(),
                asset_paths: vec![],
                metadata_path: None,
                title: self.id.into(),
                text_coverage: Some(self.coverage),
                table_cell_accuracy: None,
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
                continuation: None,
                warnings: vec![],
            })
        }
    }

    #[test]
    fn quality_rejected_route_falls_back_and_persists_attempt_order() {
        let fixture = OrchestratorFixture::new("route-fallback");
        fixture
            .service
            .register_engine(Arc::new(RouteFixtureEngine {
                root: fixture.root.clone(),
                id: "low",
                route: "pdf.text",
                coverage: 0.1,
            }))
            .unwrap();
        fixture
            .service
            .register_engine(Arc::new(RouteFixtureEngine {
                root: fixture.root.clone(),
                id: "good",
                route: "pdf.layout",
                coverage: 1.0,
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let result = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        assert_eq!(result.attempts.len(), 2, "attempts: {:?}", result.attempts);
        assert_eq!(result.attempts[0].engine_id, "low");
        assert_eq!(
            result.attempts[0].outcome,
            crate::models::import_v2::AttemptOutcome::Failed
        );
        assert_eq!(result.attempts[1].engine_id, "good");
        assert_eq!(
            result.attempts[1].outcome,
            crate::models::import_v2::AttemptOutcome::Succeeded
        );
        assert!(result.preview.is_some());
    }

    #[test]
    fn modern_workbook_floor_requires_exact_structure_metrics() {
        let input = ImportInput {
            kind: crate::models::import_v2::ImportInputKind::File,
            display_name: "book.xlsx".into(),
            locator: "book.xlsx".into(),
            normalized_locator: None,
            source_identity: None,
        };
        let mut result = EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: vec![],
            metadata_path: None,
            title: "book".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: Some(1.0),
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: None,
            warnings: vec![],
        };
        assert!(!candidate_meets_floor(
            &input,
            &result,
            QualityFloor::ModernOffice
        ));
        result.sheet_count_exact = Some(1.0);
        result.non_empty_cell_coverage = Some(0.95);
        result.formula_value_pairs = Some(1.0);
        assert!(candidate_meets_floor(
            &input,
            &result,
            QualityFloor::ModernOffice
        ));
    }
    impl ImportEngine for FailingEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "failing".into(),
                engine_version: "1".into(),
                route: "pdf.text".into(),
            }
        }
        fn supports(&self, _: &ImportInput) -> bool {
            true
        }
        fn execute(
            &self,
            _: &EngineRequest,
            _: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            if self.sabotage_task_store {
                let tasks = self.root.join(".app/tasks");
                std::fs::remove_dir_all(&tasks).unwrap();
                std::fs::write(&tasks, b"blocked").unwrap();
            }
            Err(BackendError::new(
                "ENGINE_SECRET",
                "Bearer private-token C:/Users/Aletta/a.pdf",
                true,
                false,
            ))
        }
    }

    struct OrchestratorFixture {
        root: PathBuf,
        context: ProjectContext,
        files: FileStore,
        tasks: TaskService,
        service: ImportV2Service,
    }
    impl OrchestratorFixture {
        fn new(suffix: &str) -> Self {
            let (context, root) = test_context(suffix);
            Self {
                root,
                context,
                files: FileStore::default(),
                tasks: TaskService::default(),
                service: ImportV2Service::default(),
            }
        }
        fn seed_one_item(&self) -> (ImportSession, ImportItem, BackendTask) {
            let session = self
                .service
                .create_session(&self.context, &self.files, ImportResourceMode::Balanced)
                .unwrap();
            let session = self
                .service
                .add_inputs(
                    &self.context,
                    &self.files,
                    &session.session_id,
                    vec![test_file_input("a.pdf")],
                )
                .unwrap();
            let item = session.items[0].clone();
            let task = self
                .tasks
                .create_project_task(
                    TaskType::Import,
                    self.context.project_id.clone(),
                    self.root.clone(),
                    "Fixture import".into(),
                    true,
                )
                .unwrap();
            (session, item, task)
        }
        fn reopen(&self) -> ImportSession {
            let sessions = std::fs::read_dir(self.context.app_dir.join("import-sessions")).unwrap();
            let session_id = sessions
                .flatten()
                .next()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .into_owned();
            self.service
                .load_session(&self.context, &self.files, &session_id)
                .unwrap()
        }
    }
    impl Drop for OrchestratorFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn run_item_persists_preview_ready_after_fixture_engine_succeeds() {
        let fixture = OrchestratorFixture::new("success");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let result = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        assert_eq!(result.status, ImportItemStatus::PreviewReady);
        assert!(result.preview.is_some());
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::PreviewReady
        );
        assert_eq!(
            fixture.tasks.get_task(&task.id).unwrap().status,
            TaskStatus::WaitingForConfirmation
        );
    }

    #[test]
    fn run_item_records_engine_unavailable_without_losing_session() {
        let fixture = OrchestratorFixture::new("no-engine");
        let (session, item, task) = fixture.seed_one_item();
        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_ENGINE_UNAVAILABLE);
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::WaitingCapability
        );
    }

    #[test]
    fn run_item_honors_a_pre_cancelled_task() {
        let fixture = OrchestratorFixture::new("cancelled");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        fixture.tasks.cancel_task(&task.id).unwrap();
        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_CANCELLED);
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::Cancelled
        );
        assert_eq!(
            fixture.tasks.get_task(&task.id).unwrap().status,
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn transition_helper_rejects_preview_to_completed() {
        let mut item = ImportItem::queued("item-1", test_file_input("a.pdf"));
        item.status = ImportItemStatus::PreviewReady;
        let error = transition_item(&mut item, ImportItemStatus::Completed).unwrap_err();
        assert_eq!(error.code, IMPORT_V2_STATE_INVALID);
    }

    #[test]
    fn engine_error_is_reduced_to_a_secret_free_issue() {
        let error = BackendError::new("ENGINE_FAILED", "Authorization: Bearer secret", true, false)
            .with_details(serde_json::json!({ "path": "C:/Users/Aletta/private.pdf" }));
        let issue = issue_from_engine_error(&error, ImportStage::Extract);
        assert_eq!(issue.message, "File import could not be completed.");
        let value = serde_json::to_string(&issue).unwrap();
        assert!(!value.contains("secret"));
        assert!(!value.contains("Aletta"));
    }

    #[test]
    fn session_status_priority_is_derived_from_items() {
        use ImportItemStatus::*;
        let cases = [
            (vec![Completed, Extracting], ImportSessionStatus::Processing),
            (
                vec![Completed, Failed],
                ImportSessionStatus::PartiallyCommitted,
            ),
            (vec![Completed, Skipped], ImportSessionStatus::Completed),
            (vec![Cancelled, Cancelled], ImportSessionStatus::Cancelled),
            (
                vec![PreviewReady, Queued],
                ImportSessionStatus::WaitingForConfirmation,
            ),
            (vec![Queued, Skipped], ImportSessionStatus::Draft),
        ];
        for (statuses, expected) in cases {
            let items: Vec<_> = statuses
                .into_iter()
                .enumerate()
                .map(|(i, status)| {
                    let mut item =
                        ImportItem::queued(&format!("item-{i}"), test_file_input("a.pdf"));
                    item.status = status;
                    item
                })
                .collect();
            assert_eq!(derive_session_status(&items), expected);
        }
    }

    #[test]
    fn set_item_selected_persists_the_choice() {
        let fixture = OrchestratorFixture::new("selection");
        let (session, item, _) = fixture.seed_one_item();
        let changed = fixture
            .service
            .set_item_selected(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                false,
            )
            .unwrap();
        assert!(!changed.selected);
        assert!(!fixture.reopen().items[0].selected);
    }

    #[test]
    fn quality_failure_never_becomes_preview_ready() {
        let fixture = OrchestratorFixture::new("quality-fail");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine {
                project_root: fixture.root.clone(),
                markdown: b"",
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_QUALITY_FAILED);
        let reopened = fixture.reopen();
        assert_eq!(reopened.items[0].status, ImportItemStatus::Failed);
        assert!(reopened.items[0].preview.is_none());
        assert_eq!(
            fixture.tasks.get_task(&task.id).unwrap().status,
            TaskStatus::Failed
        );
    }

    #[test]
    fn restart_reconciles_in_flight_items_and_allows_retry() {
        for status in [
            ImportItemStatus::Inspecting,
            ImportItemStatus::Extracting,
            ImportItemStatus::Validating,
        ] {
            let fixture = OrchestratorFixture::new(&format!("recovery-{status:?}"));
            let (session, item, task) = fixture.seed_one_item();
            fixture
                .tasks
                .transition_status(&task.id, TaskStatus::Running)
                .unwrap();
            let mut persisted = fixture
                .service
                .load_session(&fixture.context, &fixture.files, &session.session_id)
                .unwrap();
            persisted.items[0].status = status;
            persisted.items[0].task_id = Some(task.id.clone());
            fixture
                .service
                .sessions
                .save(&fixture.context, &fixture.files, &persisted)
                .unwrap();
            let recovered_tasks = TaskService::default();
            recovered_tasks.recover_tasks(&fixture.root).unwrap();
            assert_eq!(
                recovered_tasks.get_task(&task.id).unwrap().status,
                TaskStatus::Failed
            );
            let restarted = ImportV2Service::default();
            let reconciled = restarted
                .recover_session(
                    &fixture.context,
                    &fixture.files,
                    &recovered_tasks,
                    &session.session_id,
                )
                .unwrap();
            assert_eq!(reconciled.items[0].status, ImportItemStatus::Failed);
            assert_eq!(
                reconciled.items[0].issue.as_ref().unwrap().code,
                "TASK_RECOVERY"
            );
            restarted
                .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
                .unwrap();
            let retry = recovered_tasks
                .create_project_task(
                    TaskType::Import,
                    fixture.context.project_id.clone(),
                    fixture.root.clone(),
                    "Retry".into(),
                    true,
                )
                .unwrap();
            assert_eq!(
                restarted
                    .run_item(
                        &fixture.context,
                        &fixture.files,
                        &recovered_tasks,
                        &session.session_id,
                        &item.item_id,
                        &retry.id
                    )
                    .unwrap()
                    .status,
                ImportItemStatus::PreviewReady
            );
        }
    }

    #[test]
    fn concurrent_run_cannot_overwrite_claim_or_start_second_task() {
        let (context, root) = test_context("concurrent-claim");
        let files = Arc::new(FileStore::default());
        let tasks = Arc::new(TaskService::default());
        let service = Arc::new(ImportV2Service::default());
        let session = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = service
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![test_file_input("a.pdf")],
            )
            .unwrap();
        let item_id = session.items[0].item_id.clone();
        let first = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                root.clone(),
                "First".into(),
                true,
            )
            .unwrap();
        let second = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                root.clone(),
                "Second".into(),
                true,
            )
            .unwrap();
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        service
            .register_engine(Arc::new(BlockingEngine {
                inner: FixtureEngine::success(root.clone()),
                entered: entered.clone(),
                release: release.clone(),
            }))
            .unwrap();
        let worker = {
            let service = service.clone();
            let tasks = tasks.clone();
            let files = files.clone();
            let context = context.clone();
            let session_id = session.session_id.clone();
            let item_id = item_id.clone();
            let task_id = first.id.clone();
            std::thread::spawn(move || {
                service.run_item(&context, &files, &tasks, &session_id, &item_id, &task_id)
            })
        };
        let (lock, signal) = &*entered;
        let mut seen = lock.lock().unwrap();
        while !*seen {
            seen = signal.wait(seen).unwrap();
        }
        drop(seen);
        let error = service
            .run_item(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &item_id,
                &second.id,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_STATE_INVALID);
        assert_eq!(
            tasks.get_task(&second.id).unwrap().status,
            TaskStatus::Queued
        );
        assert_eq!(
            service
                .load_session(&context, &files, &session.session_id)
                .unwrap()
                .items[0]
                .task_id
                .as_deref(),
            Some(first.id.as_str())
        );
        let (lock, signal) = &*release;
        *lock.lock().unwrap() = true;
        signal.notify_all();
        worker.join().unwrap().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_incompatible_tasks_before_binding() {
        let fixture = OrchestratorFixture::new("task-validation");
        let (session, item, _) = fixture.seed_one_item();
        let wrong_type = fixture
            .tasks
            .create_project_task(
                TaskType::Export,
                fixture.context.project_id.clone(),
                fixture.root.clone(),
                "Wrong".into(),
                true,
            )
            .unwrap();
        let wrong_project = fixture
            .tasks
            .create_project_task(
                TaskType::Import,
                "other-project".into(),
                fixture.root.clone(),
                "Wrong project".into(),
                true,
            )
            .unwrap();
        let running = fixture
            .tasks
            .create_project_task(
                TaskType::Import,
                fixture.context.project_id.clone(),
                fixture.root.clone(),
                "Running".into(),
                true,
            )
            .unwrap();
        fixture
            .tasks
            .transition_status(&running.id, TaskStatus::Running)
            .unwrap();
        for task_id in [&wrong_type.id, &wrong_project.id, &running.id] {
            assert_eq!(
                fixture
                    .service
                    .run_item(
                        &fixture.context,
                        &fixture.files,
                        &fixture.tasks,
                        &session.session_id,
                        &item.item_id,
                        task_id
                    )
                    .unwrap_err()
                    .code,
                IMPORT_V2_STATE_INVALID
            );
            assert!(fixture.reopen().items[0].task_id.is_none());
        }
    }

    #[test]
    fn inspecting_is_persisted_before_task_running() {
        let fixture = OrchestratorFixture::new("ordering");
        let (session, item, task) = fixture.seed_one_item();
        let task_store = fixture.root.join(".app/tasks");
        std::fs::remove_dir_all(&task_store).unwrap();
        std::fs::write(&task_store, b"blocked").unwrap();
        assert_eq!(
            fixture
                .service
                .run_item(
                    &fixture.context,
                    &fixture.files,
                    &fixture.tasks,
                    &session.session_id,
                    &item.item_id,
                    &task.id
                )
                .unwrap_err()
                .code,
            IMPORT_V2_STATE_INVALID
        );
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::Inspecting
        );
    }

    #[test]
    fn finish_failed_propagates_task_persistence_errors() {
        let fixture = OrchestratorFixture::new("failure-persistence");
        fixture
            .service
            .register_engine(Arc::new(FailingEngine {
                root: fixture.root.clone(),
                sabotage_task_store: true,
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_STATE_INVALID);
    }

    #[test]
    fn finish_failed_persists_redacted_error_log_and_terminal_task() {
        let fixture = OrchestratorFixture::new("failure-durable");
        fixture
            .service
            .register_engine(Arc::new(FailingEngine {
                root: fixture.root.clone(),
                sabotage_task_store: false,
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        assert_eq!(
            fixture
                .service
                .run_item(
                    &fixture.context,
                    &fixture.files,
                    &fixture.tasks,
                    &session.session_id,
                    &item.item_id,
                    &task.id
                )
                .unwrap_err()
                .code,
            "ENGINE_SECRET"
        );
        let restarted = TaskService::default();
        restarted.recover_tasks(&fixture.root).unwrap();
        let recovered = restarted.get_task(&task.id).unwrap();
        assert_eq!(recovered.status, TaskStatus::Failed);
        assert_eq!(recovered.error.unwrap().message, "Import engine failed.");
        assert_eq!(
            restarted.get_logs(&task.id).unwrap()[0].message,
            "Import engine failed."
        );
    }

    #[test]
    fn restart_reconciles_in_flight_item_with_cancelled_task() {
        let fixture = OrchestratorFixture::new("recover-cancelled");
        let (session, _, task) = fixture.seed_one_item();
        fixture
            .tasks
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        let mut persisted = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &session.session_id)
            .unwrap();
        persisted.items[0].status = ImportItemStatus::Extracting;
        persisted.items[0].task_id = Some(task.id.clone());
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &persisted)
            .unwrap();
        fixture.tasks.cancel_task(&task.id).unwrap();
        let restarted_tasks = TaskService::default();
        restarted_tasks.recover_tasks(&fixture.root).unwrap();
        let recovered = ImportV2Service::default()
            .recover_session(
                &fixture.context,
                &fixture.files,
                &restarted_tasks,
                &session.session_id,
            )
            .unwrap();
        assert_eq!(recovered.items[0].status, ImportItemStatus::Cancelled);
        assert_eq!(recovered.status, ImportSessionStatus::Cancelled);
    }

    #[test]
    fn cancellation_after_claim_cannot_leave_item_processing() {
        let fixture = OrchestratorFixture::new("cancel-race");
        let (session, item, task) = fixture.seed_one_item();
        fixture
            .service
            .claim_item_for_run(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &task.id,
                false,
            )
            .unwrap();
        fixture.tasks.cancel_task(&task.id).unwrap();
        let error = fixture
            .service
            .start_claimed_task(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_CANCELLED);
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::Cancelled
        );
        assert_eq!(fixture.reopen().status, ImportSessionStatus::Cancelled);
    }

    #[test]
    fn preview_task_result_serializes_typed_reference() {
        let fixture = OrchestratorFixture::new("typed-result");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        let value = serde_json::to_value(fixture.tasks.get_task(&task.id).unwrap().result.unwrap())
            .unwrap();
        assert_eq!(value["reference"]["type"], "import_preview");
        assert_eq!(value["reference"]["sessionId"], session.session_id);
        assert_eq!(value["reference"]["itemId"], item.item_id);
        assert_eq!(value["affectedPaths"], serde_json::json!([]));
    }
}
