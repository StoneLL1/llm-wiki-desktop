use std::io::{Read, Write};
use std::path::{Component, Path};

use flate2::{write::GzEncoder, Compression};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::app_state::{ProjectExecutionLease, ProjectWritePermit};
use crate::errors::{
    BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_COMMIT_CONFLICT, IMPORT_V2_COMMIT_FAILED,
    IMPORT_V2_QUALITY_FAILED, IMPORT_V2_STATE_INVALID,
};
use crate::models::git::CheckpointPurpose;
use crate::models::import_v2::{
    ArtifactKind, CommitImportSessionRequest, CommitItemDecision, DuplicateResult, ImportArtifact,
    ImportBatchResult, ImportCommitDisposition, ImportCompletion, ImportInputKind, ImportIssue,
    ImportIssueDiagnostics, ImportItem, ImportItemCommitResult, ImportItemResolution,
    ImportItemStatus, ImportPreviewArtifact, ImportPrimaryAction, ImportRecoveryAction,
    ImportResolutionBinding, ImportResolutionContext, ImportResolutionKind, ImportSession,
    ImportStage, ImportThreeWayMergeContext, ItemFailure, QualityLevel, SourceVersionChange,
    UserIssue,
};
use crate::models::paths::ProjectContext;
use crate::models::source_package::{SourcePackageManifest, SourcePackageMemberRole};
use crate::services::import_v2::orchestrator::derive_session_status;
use crate::services::import_v2::source_finalization::{
    candidate_record, finalize_source, inspect_candidate, validate_final_source_binding,
    CandidateInspection, FinalizationInput,
};
use crate::services::import_v2::source_registry::{
    SourceArtifactRecord, SourceCommitInput, SourceManifest, SourcePointer, SourceRegistry,
    SourceResolution,
};
use crate::services::import_v2::transaction::FileTransaction;
use crate::services::import_v2::ImportV2Service;
use crate::services::{FileStore, GitService};
use crate::utils::markdown_utils::{parse_frontmatter, split_frontmatter};
use crate::utils::safe_project_dir::remove_project_file;

#[derive(Clone, PartialEq)]
pub(crate) struct ExactDuplicateFinalizationFingerprint {
    task_id: Option<String>,
    preview: ImportPreviewArtifact,
}

#[cfg(test)]
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum CommitPersistenceBoundary {
    JournalIntent(CommitPersistenceTarget),
    TargetInstalled(CommitPersistenceTarget),
    CommittedMarkerPersisted,
    JournalDeleted,
}

#[cfg(test)]
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum CommitPersistenceTarget {
    RawSnapshot,
    RawExtracted,
    Baseline,
    Asset(String),
    Wiki,
    Manifest,
    Index,
    History,
    SessionItem,
    SessionSummary,
}

#[cfg(test)]
thread_local! {
    static COMMIT_PERSISTENCE_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&CommitPersistenceBoundary) -> bool>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
thread_local! {
    static COMMIT_DURABLE_TARGETS_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(Vec<(String, String)>)>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
fn set_commit_durable_targets_hook(hook: impl FnOnce(Vec<(String, String)>) + 'static) {
    COMMIT_DURABLE_TARGETS_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn run_commit_durable_targets_hook(targets: Vec<(String, String)>) {
    COMMIT_DURABLE_TARGETS_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(targets);
        }
    });
}

#[cfg(test)]
fn classify_commit_target(relative: &str) -> CommitPersistenceTarget {
    if relative.contains("/assets/") {
        CommitPersistenceTarget::Asset(relative.rsplit('/').next().unwrap_or(relative).into())
    } else if relative.contains("/derived/")
        || relative.contains("/metadata/")
        || relative.contains("/subtitles/")
        || relative.contains("/transcripts/")
    {
        CommitPersistenceTarget::RawExtracted
    } else if relative.starts_with("raw/sources/") || relative.starts_with("raw/web/") {
        CommitPersistenceTarget::RawSnapshot
    } else if relative.starts_with(".app/source-artifacts/") {
        CommitPersistenceTarget::Baseline
    } else if relative.starts_with("wiki/") {
        CommitPersistenceTarget::Wiki
    } else if relative.starts_with(".app/sources/") {
        CommitPersistenceTarget::Manifest
    } else if relative == ".app/source-index-v2.json" {
        CommitPersistenceTarget::Index
    } else if relative.starts_with(".app/import-history/")
        || relative.starts_with(".app/import-history-previews/")
    {
        CommitPersistenceTarget::History
    } else if relative.contains("/items/") && relative.ends_with(".json") {
        CommitPersistenceTarget::SessionItem
    } else if relative.starts_with(".app/import-sessions/") && relative.ends_with("/session.json") {
        CommitPersistenceTarget::SessionSummary
    } else {
        panic!("unclassified Import V2 commit persistence target: {relative}");
    }
}

#[cfg(test)]
pub(super) fn run_commit_persistence_hook(phase: &str, relative: Option<&str>) -> bool {
    COMMIT_PERSISTENCE_HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(hook) = slot.as_mut() else {
            return false;
        };
        let boundary = match phase {
            "intent" => CommitPersistenceBoundary::JournalIntent(classify_commit_target(
                relative.expect("journal intent requires a relative target"),
            )),
            "installed" => CommitPersistenceBoundary::TargetInstalled(classify_commit_target(
                relative.expect("installed target requires a relative path"),
            )),
            "committed" => CommitPersistenceBoundary::CommittedMarkerPersisted,
            "deleted" => CommitPersistenceBoundary::JournalDeleted,
            _ => panic!("unknown persistence phase: {phase}"),
        };
        hook(&boundary)
    })
}

#[cfg(test)]
fn set_commit_persistence_hook(hook: impl FnMut(&CommitPersistenceBoundary) -> bool + 'static) {
    COMMIT_PERSISTENCE_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
thread_local! {
    static BEFORE_FAILED_HISTORY_WRITE_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
    static BEFORE_ARTIFACT_OPEN_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
    static BEFORE_EXACT_DUPLICATE_COMMIT_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
fn run_before_artifact_open_hook(path: &Path) {
    BEFORE_ARTIFACT_OPEN_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(path)
        }
    });
}

#[cfg(not(test))]
fn run_before_artifact_open_hook(_path: &Path) {}

#[cfg(test)]
fn set_before_failed_history_write_hook(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_FAILED_HISTORY_WRITE_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn set_before_artifact_open_hook(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_ARTIFACT_OPEN_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

#[cfg(test)]
fn set_before_exact_duplicate_commit_hook(hook: impl FnOnce() + 'static) {
    BEFORE_EXACT_DUPLICATE_COMMIT_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

fn run_before_exact_duplicate_commit_hook() {
    #[cfg(test)]
    BEFORE_EXACT_DUPLICATE_COMMIT_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

fn run_before_failed_history_write_hook(path: &Path) {
    #[cfg(test)]
    BEFORE_FAILED_HISTORY_WRITE_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(path);
        }
    });
    #[cfg(not(test))]
    let _ = path;
}

impl ImportV2Service {
    pub(crate) fn derive_resolution_context(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item: &ImportItem,
    ) -> Result<ImportResolutionContext, BackendError> {
        derive_resolution_context(context, files, session_id, item)
    }

    pub fn get_three_way_merge_context(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportThreeWayMergeContext, BackendError> {
        let session = self.sessions.load(context, files, session_id)?;
        let item = session
            .items
            .iter()
            .find(|item| item.item_id == item_id)
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_STATE_INVALID,
                    "Import item was not found for three-way merge.",
                )
            })?;
        let resolution = derive_resolution_context(context, files, session_id, item)?;
        if resolution.kind != ImportResolutionKind::NeedsThreeWayMerge {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "Three-way merge context is available only for an edited current Source.",
            ));
        }
        let binding = resolution
            .binding
            .as_ref()
            .ok_or_else(stale_resolution_error)?;
        let manifest = SourceRegistry::read_manifest(
            context,
            files,
            &format!(".app/sources/{}.json", binding.source_id),
        )?;
        let version = manifest
            .versions
            .iter()
            .find(|version| version.version_id == binding.target_version_id)
            .ok_or_else(stale_resolution_error)?;
        let baseline = std::fs::read(context.resolve_project_path(&version.baseline_path)?)
            .map_err(|_| stale_resolution_error())?;
        let current = std::fs::read(context.resolve_project_path(&manifest.wiki_path)?)
            .map_err(|_| stale_resolution_error())?;
        if format!("{:x}", Sha256::digest(&current)) != binding.current_hash {
            return Err(stale_resolution_error());
        }
        let preview = item.preview.as_ref().ok_or_else(stale_resolution_error)?;
        let staging = context.root.join(format!(
            ".app/import-sessions/{session_id}/items/{item_id}/staging"
        ));
        let candidate = verified_artifact(
            &staging,
            &preview.markdown.relative_path,
            &preview.markdown.sha256,
        )?;
        Ok(ImportThreeWayMergeContext {
            resolution,
            baseline_markdown: String::from_utf8(baseline).map_err(|_| stale_resolution_error())?,
            current_markdown: String::from_utf8(current).map_err(|_| stale_resolution_error())?,
            candidate_markdown: String::from_utf8(candidate)
                .map_err(|_| stale_resolution_error())?,
        })
    }

    fn set_item_resolution_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        resolution: ImportItemResolution,
    ) -> Result<ImportItem, BackendError> {
        if !matches!(
            resolution,
            ImportItemResolution::KeepCurrentSource { .. }
                | ImportItemResolution::ApplyImportCandidate { .. }
                | ImportItemResolution::ManualMerge { .. }
        ) {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "This resolution is not valid for an edited current Source.",
            ));
        }
        let _guard = self.mutation_lock.lock().map_err(|_| {
            commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Import commit lock is unavailable.",
            )
        })?;
        FileTransaction::reconcile_project(&context.root)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item_position = session
            .items
            .iter()
            .position(|item| item.item_id == item_id)
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_STATE_INVALID,
                    "Import item was not found for merge resolution.",
                )
            })?;
        let latest_resolution =
            derive_resolution_context(context, files, session_id, &session.items[item_position])?;
        if latest_resolution.kind != ImportResolutionKind::NeedsThreeWayMerge {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "This item no longer requires a three-way merge.",
            ));
        }
        let binding = latest_resolution
            .binding
            .as_ref()
            .ok_or_else(stale_resolution_error)?;
        let item = &mut session.items[item_position];
        let preview = item.preview.as_mut().ok_or_else(stale_resolution_error)?;
        let merged_hash = preview
            .manual_merge
            .as_ref()
            .map(|artifact| artifact.sha256.as_str())
            .unwrap_or("");
        validate_resolution_binding(
            &resolution,
            &binding.source_id,
            &binding.candidate_hash,
            Some(&binding.current_hash),
            Some(&binding.target_version_id),
            merged_hash,
        )?;
        preview.resolution = Some(ImportResolutionContext {
            default_resolution: Some(resolution),
            ..latest_resolution
        });
        item.selected = true;
        let result = item.clone();
        session.updated_at = chrono::Utc::now().to_rfc3339();
        session.status = derive_session_status(&session.items);
        self.sessions.save(context, files, &session)?;
        Ok(result)
    }

    fn stage_manual_merge_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        merged_markdown: &str,
    ) -> Result<ImportItem, BackendError> {
        if merged_markdown.trim().is_empty() || merged_markdown.len() > 16 * 1024 * 1024 {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "Manual merge Markdown is empty or exceeds the supported size.",
            ));
        }
        let _guard = self.mutation_lock.lock().map_err(|_| {
            commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Import commit lock is unavailable.",
            )
        })?;
        FileTransaction::reconcile_project(&context.root)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item_position = session
            .items
            .iter()
            .position(|item| item.item_id == item_id)
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_STATE_INVALID,
                    "Import item was not found for manual merge.",
                )
            })?;
        let latest_resolution =
            derive_resolution_context(context, files, session_id, &session.items[item_position])?;
        let item = &mut session.items[item_position];
        let preview = item.preview.as_mut().ok_or_else(|| {
            commit_error(
                IMPORT_V2_STATE_INVALID,
                "Import preview is missing for manual merge.",
            )
        })?;
        if latest_resolution.kind != ImportResolutionKind::NeedsThreeWayMerge {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "Manual merge is accepted only for a current three-way conflict.",
            ));
        }
        let binding = latest_resolution
            .binding
            .as_ref()
            .ok_or_else(stale_resolution_error)?
            .clone();
        item.status = ImportItemStatus::NeedsMerge;
        let relative_path =
            format!(".app/import-sessions/{session_id}/items/{item_id}/staging/manual-merge.md");
        files.write_markdown(context, &relative_path, merged_markdown)?;
        let manual_merge = ImportArtifact {
            kind: ArtifactKind::Markdown,
            relative_path: "manual-merge.md".into(),
            sha256: files.file_hash(context, &relative_path)?,
            size_bytes: merged_markdown.len() as u64,
        };
        preview.resolution = Some(ImportResolutionContext {
            default_resolution: Some(ImportItemResolution::ManualMerge {
                source_id: binding.source_id,
                candidate_hash: binding.candidate_hash,
                current_hash: binding.current_hash,
                target_version_id: binding.target_version_id,
                merged_hash: manual_merge.sha256.clone(),
            }),
            ..latest_resolution
        });
        preview.manual_merge = Some(manual_merge);
        item.selected = true;
        let result = item.clone();
        session.updated_at = chrono::Utc::now().to_rfc3339();
        session.status = derive_session_status(&session.items);
        self.sessions.save(context, files, &session)?;
        Ok(result)
    }

    pub(crate) fn set_item_resolution_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        resolution: ImportItemResolution,
    ) -> Result<ImportItem, BackendError> {
        self.set_item_resolution_unchecked(permit.context(), files, session_id, item_id, resolution)
    }

    pub(crate) fn stage_manual_merge_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        merged_markdown: &str,
    ) -> Result<ImportItem, BackendError> {
        self.stage_manual_merge_unchecked(
            permit.context(),
            files,
            session_id,
            item_id,
            merged_markdown,
        )
    }

    #[cfg(debug_assertions)]
    pub fn set_item_resolution(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        resolution: ImportItemResolution,
    ) -> Result<ImportItem, BackendError> {
        self.set_item_resolution_unchecked(context, files, session_id, item_id, resolution)
    }

    #[cfg(debug_assertions)]
    pub fn stage_manual_merge(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        merged_markdown: &str,
    ) -> Result<ImportItem, BackendError> {
        self.stage_manual_merge_unchecked(context, files, session_id, item_id, merged_markdown)
    }

    /// Compatibility surface for integration and service tests. Production
    /// commits must enter through a capability-bearing authorized method.
    #[cfg(debug_assertions)]
    pub fn commit_items(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        request: &CommitImportSessionRequest,
    ) -> Result<ImportBatchResult, BackendError> {
        self.commit_items_cancellable(context, file_store, git_service, request, || false)
    }

    #[cfg(debug_assertions)]
    pub fn finalize_exact_duplicate(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        session_id: &str,
        item_id: &str,
        restricted_content_acknowledged: bool,
        before_commit: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<Option<ImportBatchResult>, BackendError> {
        self.finalize_exact_duplicate_cancellable_unchecked(
            context,
            file_store,
            git_service,
            session_id,
            item_id,
            restricted_content_acknowledged,
            || false,
            before_commit,
        )
    }

    #[cfg(debug_assertions)]
    pub fn finalize_exact_duplicate_cancellable<C>(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        session_id: &str,
        item_id: &str,
        restricted_content_acknowledged: bool,
        cancelled: C,
        before_commit: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<Option<ImportBatchResult>, BackendError>
    where
        C: Fn() -> bool,
    {
        self.finalize_exact_duplicate_cancellable_unchecked(
            context,
            file_store,
            git_service,
            session_id,
            item_id,
            restricted_content_acknowledged,
            cancelled,
            before_commit,
        )
    }

    fn finalize_exact_duplicate_cancellable_unchecked<C>(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        session_id: &str,
        item_id: &str,
        restricted_content_acknowledged: bool,
        cancelled: C,
        before_commit: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<Option<ImportBatchResult>, BackendError>
    where
        C: Fn() -> bool,
    {
        let session = self.sessions.load(context, file_store, session_id)?;
        let Some(item) = session.items.iter().find(|item| item.item_id == item_id) else {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "Import item was not found for duplicate finalization.",
            ));
        };
        let Some(preview) = item.preview.as_ref() else {
            return Ok(None);
        };
        let Some(resolution) = preview.resolution.as_ref() else {
            return Ok(None);
        };
        if item.status != ImportItemStatus::PreviewReady
            || resolution.kind != ImportResolutionKind::ExactDuplicate
        {
            return Ok(None);
        }
        if item.restricted_content && !restricted_content_acknowledged {
            return Ok(None);
        }
        let fingerprint = ExactDuplicateFinalizationFingerprint {
            task_id: item.task_id.clone(),
            preview: preview.clone(),
        };
        run_before_exact_duplicate_commit_hook();
        let attempt = (|| {
            let decision = resolution
                .default_resolution
                .clone()
                .filter(|value| matches!(value, ImportItemResolution::ExactDuplicateSkip { .. }))
                .ok_or_else(stale_resolution_error)?;
            let request = CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: context.root.to_string_lossy().into_owned(),
                session_id: session_id.to_string(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                decisions: vec![CommitItemDecision {
                    item_id: item_id.to_string(),
                    resolution: Some(decision),
                }],
            };
            self.commit_items_cancellable_with_progress(
                context,
                file_store,
                git_service,
                &request,
                &cancelled,
                |_| {},
                Some((item_id, &fingerprint)),
                before_commit,
            )
        })();
        let batch = match attempt {
            Ok(batch) => batch,
            Err(error) if error.code == IMPORT_V2_CANCELLED => return Err(error),
            Err(error) => {
                if !self.record_exact_duplicate_commit_failure(
                    context,
                    file_store,
                    session_id,
                    item_id,
                    &error.code,
                    &fingerprint,
                )? {
                    return Ok(None);
                }
                return Err(error);
            }
        };
        let failed = batch
            .items
            .iter()
            .find(|result| result.item_id == item_id && !result.committed);
        if let Some(failed) = failed {
            let code = failed
                .error_code
                .clone()
                .unwrap_or_else(|| IMPORT_V2_COMMIT_FAILED.into());
            if !self.record_exact_duplicate_commit_failure(
                context,
                file_store,
                session_id,
                item_id,
                &code,
                &fingerprint,
            )? {
                return Ok(None);
            }
            return Err(commit_error(
                &code,
                "The duplicate locator could not be recorded.",
            ));
        }
        Ok(Some(batch))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finalize_exact_duplicate_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        file_store: &FileStore,
        git_service: &GitService,
        session_id: &str,
        item_id: &str,
        restricted_content_acknowledged: bool,
        before_commit: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<Option<ImportBatchResult>, BackendError> {
        self.finalize_exact_duplicate_cancellable_unchecked(
            permit.context(),
            file_store,
            git_service,
            session_id,
            item_id,
            restricted_content_acknowledged,
            || false,
            before_commit,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn finalize_exact_duplicate_cancellable_authorized<C>(
        &self,
        execution: &ProjectExecutionLease,
        file_store: &FileStore,
        git_service: &GitService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        restricted_content_acknowledged: bool,
        cancelled: C,
        before_commit: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<Option<ImportBatchResult>, BackendError>
    where
        C: Fn() -> bool,
    {
        self.finalize_exact_duplicate_cancellable_unchecked(
            execution.task_context(task_id)?,
            file_store,
            git_service,
            session_id,
            item_id,
            restricted_content_acknowledged,
            cancelled,
            before_commit,
        )
    }

    fn record_exact_duplicate_commit_failure(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        code: &str,
        fingerprint: &ExactDuplicateFinalizationFingerprint,
    ) -> Result<bool, BackendError> {
        let _guard = self.mutation_lock.lock().map_err(|_| {
            commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Import commit lock is unavailable.",
            )
        })?;
        FileTransaction::reconcile_project(&context.root)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item = session
            .items
            .iter_mut()
            .find(|item| item.item_id == item_id)
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_STATE_INVALID,
                    "Import item was not found after duplicate finalization failed.",
                )
            })?;
        if item.task_id != fingerprint.task_id
            || item.preview.as_ref() != Some(&fingerprint.preview)
            || !matches!(
                item.status,
                ImportItemStatus::PreviewReady
                    | ImportItemStatus::Committing
                    | ImportItemStatus::Failed
            )
        {
            return Ok(false);
        }
        if item.status == ImportItemStatus::PreviewReady {
            item.status = ImportItemStatus::Committing;
        }
        if item.status == ImportItemStatus::Committing {
            item.status = ImportItemStatus::Failed;
        }
        item.progress = None;
        item.issue = Some(ImportIssue {
            code: code.into(),
            message: "The duplicate locator could not be recorded.".into(),
            stage: ImportStage::Commit,
            retryable: true,
            user_action_required: false,
            recovery_actions: vec![ImportRecoveryAction::Retry, ImportRecoveryAction::ViewLog],
            available_actions: Vec::new(),
            subtitle_candidates: Vec::new(),
        });
        session.status = derive_session_status(&session.items);
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.sessions.save(context, files, &session)?;
        Ok(true)
    }

    #[cfg(debug_assertions)]
    pub fn commit_items_cancellable(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        request: &CommitImportSessionRequest,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<ImportBatchResult, BackendError> {
        self.commit_items_cancellable_with_progress(
            context,
            file_store,
            git_service,
            request,
            is_cancelled,
            |_| {},
            None,
            || Ok(()),
        )
    }

    fn commit_items_cancellable_with_progress(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        request: &CommitImportSessionRequest,
        is_cancelled: impl Fn() -> bool,
        mut on_durable_progress: impl FnMut(&ImportBatchResult),
        exact_duplicate_precondition: Option<(&str, &ExactDuplicateFinalizationFingerprint)>,
        before_locked_commit: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<ImportBatchResult, BackendError> {
        let asserted_root = Path::new(&request.project_root_path)
            .canonicalize()
            .map_err(|_| {
                commit_error(
                    IMPORT_V2_STATE_INVALID,
                    "Import commit project root does not exist or cannot be resolved.",
                )
            })?;
        let context_root = context.root.canonicalize().map_err(|_| {
            commit_error(
                IMPORT_V2_STATE_INVALID,
                "The trusted project root does not exist or cannot be resolved.",
            )
        })?;
        if request.project_id != context.project_id || asserted_root != context_root {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "Import commit project context does not match.",
            ));
        }
        let _guard = self.mutation_lock.lock().map_err(|_| {
            commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Import commit lock is unavailable.",
            )
        })?;
        FileTransaction::reconcile_project(&context.root)?;
        SourceRegistry::migrate_project_v3(context, file_store)?;
        let mut session = self
            .sessions
            .load(context, file_store, &request.session_id)?;
        // These hashes protect the same item bytes that supplied the working
        // snapshot below.  Do not recalculate a "current" hash inside
        // `commit_one`: that would bless a newer external edit while writing
        // an older in-memory item.
        let session_snapshot_hashes = self
            .sessions
            .serialized_writes(context, &session)?
            .into_iter()
            .map(|(path, bytes)| (path, format!("{:x}", Sha256::digest(&bytes))))
            .collect::<std::collections::HashMap<_, _>>();
        if let Some((item_id, fingerprint)) = exact_duplicate_precondition {
            let item = session
                .items
                .iter()
                .find(|item| item.item_id == item_id)
                .ok_or_else(stale_resolution_error)?;
            if item.status != ImportItemStatus::PreviewReady
                || item.task_id != fingerprint.task_id
                || item.preview.as_ref() != Some(&fingerprint.preview)
            {
                return Err(stale_resolution_error());
            }
        }
        before_locked_commit()?;
        validate_complete_decision_set(&session, &request.decisions)?;
        let mut history_snapshot = session.clone();
        history_snapshot.items = request
            .decisions
            .iter()
            .filter_map(|decision| {
                session
                    .items
                    .iter()
                    .find(|item| item.item_id == decision.item_id)
                    .cloned()
            })
            .collect();
        history_snapshot.status = derive_session_status(&history_snapshot.items);
        history_snapshot.updated_at = chrono::Utc::now().to_rfc3339();
        let batch_id = uuid::Uuid::new_v4().to_string();
        let history_path = format!(".app/import-history/{batch_id}.json");
        let mut batch = ImportBatchResult {
            completion: Some(ImportCompletion::empty(
                request.session_id.clone(),
                batch_id.clone(),
            )),
            batch_id,
            session_id: request.session_id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            batch_task_id: request.batch_task_id.clone(),
            committed_count: 0,
            failed_count: 0,
            items: Vec::new(),
            history_snapshot: Some(history_snapshot),
        };
        let mut initial_history = FileTransaction::new_for_project(&context.root);
        initial_history.write_new(
            &context.resolve_project_path(&history_path)?,
            &json_bytes(&batch)?,
        )?;
        initial_history.commit()?;
        for (position, decision) in request.decisions.iter().enumerate() {
            let history_hash_before = file_store.file_hash(context, &history_path)?;
            if is_cancelled() {
                for unprocessed in &request.decisions[position..] {
                    let result = ImportItemCommitResult {
                        item_id: unprocessed.item_id.clone(),
                        source_id: None,
                        version_id: None,
                        wiki_path: None,
                        content_hash: None,
                        disposition: None,
                        warnings: Vec::new(),
                        committed: false,
                        error_code: Some(crate::errors::IMPORT_V2_CANCELLED.into()),
                    };
                    batch.items.push(result.clone());
                    update_history_snapshot(&mut batch, &result);
                    refresh_completion(&mut batch);
                }
                batch.failed_count = batch.items.len() as u32 - batch.committed_count;
                persist_history_checked(context, &history_path, &batch, &history_hash_before)?;
                on_durable_progress(&batch);
                return Err(commit_error(
                    crate::errors::IMPORT_V2_CANCELLED,
                    "Import commit was cancelled.",
                ));
            }
            let provisional = match self.commit_one(
                context,
                file_store,
                git_service,
                &mut session,
                decision,
                &history_path,
                &batch,
                &history_hash_before,
                session_snapshot_hashes
                    .get(&format!(
                        "{}/{}/items/{}.json",
                        context.layout.import_state_root.as_deref().ok_or_else(|| {
                            commit_error(
                                IMPORT_V2_STATE_INVALID,
                                "Import state is unavailable for this project layout.",
                            )
                        })?,
                        request.session_id,
                        decision.item_id
                    ))
                    .ok_or_else(|| {
                        commit_error(
                            IMPORT_V2_COMMIT_CONFLICT,
                            "Import item changed before commit.",
                        )
                    })?,
            ) {
                Ok(result) => result,
                Err(error) => ImportItemCommitResult {
                    item_id: decision.item_id.clone(),
                    source_id: None,
                    version_id: None,
                    wiki_path: None,
                    content_hash: None,
                    disposition: None,
                    warnings: Vec::new(),
                    committed: false,
                    error_code: Some(error.code),
                },
            };
            batch.items.push(provisional.clone());
            update_history_snapshot(&mut batch, &provisional);
            refresh_completion(&mut batch);
            batch.committed_count = batch.items.iter().filter(|item| item.committed).count() as u32;
            batch.failed_count = batch.items.len() as u32 - batch.committed_count;
            if !batch.items.last().is_some_and(|item| item.committed) {
                run_before_failed_history_write_hook(&context.resolve_project_path(&history_path)?);
                persist_history_checked(context, &history_path, &batch, &history_hash_before)?;
            }
            on_durable_progress(&batch);
        }
        // Membership and summary are a separate linearization boundary.  The
        // per-item transactions above intentionally never rewrite unrelated
        // item JSON.  If an external edit touched membership/summary, stop
        // rather than clobber it; completed item facts/history remain durable.
        session.status = derive_session_status(&session.items);
        session.updated_at = chrono::Utc::now().to_rfc3339();
        let summary_write = self
            .sessions
            .serialized_writes(context, &session)?
            .into_iter()
            .find(|(path, _)| path.ends_with("/session.json"))
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Import session summary is missing.",
                )
            })?;
        let expected_summary_hash =
            session_snapshot_hashes
                .get(&summary_write.0)
                .ok_or_else(|| {
                    commit_error(
                        IMPORT_V2_COMMIT_CONFLICT,
                        "Import session changed during commit.",
                    )
                })?;
        let mut summary_transaction = FileTransaction::new_for_project(&context.root);
        summary_transaction.write_if_hash_matches(
            &context.resolve_project_path(&summary_write.0)?,
            &summary_write.1,
            expected_summary_hash,
        )?;
        summary_transaction.commit()?;
        Ok(batch)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn commit_items_cancellable_with_progress_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        file_store: &FileStore,
        git_service: &GitService,
        request: &CommitImportSessionRequest,
        is_cancelled: impl Fn() -> bool,
        on_durable_progress: impl FnMut(&ImportBatchResult),
        exact_duplicate_precondition: Option<(&str, &ExactDuplicateFinalizationFingerprint)>,
        before_locked_commit: impl FnOnce() -> Result<(), BackendError>,
    ) -> Result<ImportBatchResult, BackendError> {
        self.commit_items_cancellable_with_progress(
            permit.context(),
            file_store,
            git_service,
            request,
            is_cancelled,
            on_durable_progress,
            exact_duplicate_precondition,
            before_locked_commit,
        )
    }

    fn commit_one(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        git: &GitService,
        session: &mut ImportSession,
        decision: &CommitItemDecision,
        history_path: &str,
        prior_batch: &ImportBatchResult,
        history_expected_hash: &str,
        item_expected_hash: &str,
    ) -> Result<ImportItemCommitResult, BackendError> {
        let item_position = session
            .items
            .iter()
            .position(|item| item.item_id == decision.item_id)
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_STATE_INVALID,
                    "Import item was not found for commit.",
                )
            })?;
        let item = session.items[item_position].clone();
        if !item.selected
            || !matches!(
                item.status,
                ImportItemStatus::PreviewReady | ImportItemStatus::NeedsMerge
            )
        {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "Import item is not ready for commit.",
            ));
        }
        let preview = item
            .preview
            .as_ref()
            .ok_or_else(|| commit_error(IMPORT_V2_STATE_INVALID, "Import preview is missing."))?;
        let preview_is_new_source = preview
            .resolution
            .as_ref()
            .is_some_and(|resolution| resolution.kind == ImportResolutionKind::NewSource);
        let preview_target_wiki_path = if preview_is_new_source {
            Some(
                preview
                    .resolution
                    .as_ref()
                    .and_then(|resolution| resolution.target_wiki_path.as_deref())
                    .ok_or_else(stale_resolution_error)?,
            )
        } else {
            None
        };
        if preview.quality.level == QualityLevel::Fail {
            return Err(commit_error(
                IMPORT_V2_QUALITY_FAILED,
                "Failed quality previews cannot be committed.",
            ));
        }
        let staging = context.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging",
            session.session_id, item.item_id
        ));
        let source = verified_artifact(
            &staging,
            &preview.source_snapshot.relative_path,
            &preview.source_snapshot.sha256,
        )?;
        let markdown = verified_artifact(
            &staging,
            &preview.markdown.relative_path,
            &preview.markdown.sha256,
        )?;
        let committed_markdown = match decision.resolution.as_ref() {
            Some(ImportItemResolution::ManualMerge { merged_hash, .. }) => {
                let artifact = preview.manual_merge.as_ref().ok_or_else(|| {
                    commit_error(
                        IMPORT_V2_COMMIT_CONFLICT,
                        "The staged manual merge candidate is missing.",
                    )
                })?;
                if artifact.kind != ArtifactKind::Markdown || artifact.sha256 != *merged_hash {
                    return Err(stale_resolution_error());
                }
                verified_artifact(&staging, &artifact.relative_path, &artifact.sha256)?
            }
            _ => markdown.clone(),
        };
        let committed_markdown_hash = format!("{:x}", Sha256::digest(&committed_markdown));
        let staged_package = preview
            .assets
            .iter()
            .find(|artifact| artifact.relative_path == "source-package.json")
            .map(|artifact| {
                let bytes = verified_artifact(&staging, &artifact.relative_path, &artifact.sha256)?;
                let package = serde_json::from_slice::<SourcePackageManifest>(&bytes)
                    .map_err(|_| staging_artifact_error())?;
                package
                    .validate_staging()
                    .map_err(|_| staging_artifact_error())?;
                for member in &package.members {
                    if member.role == SourcePackageMemberRole::Index {
                        if member.staging_path != preview.markdown.relative_path
                            || member.content_hash != preview.markdown.sha256
                            || member.human_edit_hash != preview.markdown.sha256
                        {
                            return Err(staging_artifact_error());
                        }
                        continue;
                    }
                    let artifact = preview
                        .assets
                        .iter()
                        .find(|artifact| artifact.relative_path == member.staging_path)
                        .filter(|artifact| artifact.kind == ArtifactKind::Attachment)
                        .ok_or_else(staging_artifact_error)?;
                    if artifact.sha256 != member.content_hash
                        || artifact.sha256 != member.human_edit_hash
                    {
                        return Err(staging_artifact_error());
                    }
                    verified_artifact(&staging, &artifact.relative_path, &artifact.sha256)?;
                }
                Ok(package)
            })
            .transpose()?;
        let package_staging_paths = staged_package
            .as_ref()
            .map(|package| {
                package
                    .members
                    .iter()
                    .filter(|member| member.role != SourcePackageMemberRole::Index)
                    .map(|member| member.staging_path.clone())
                    .chain(std::iter::once("source-package.json".to_string()))
                    .collect::<std::collections::HashSet<_>>()
            })
            .unwrap_or_default();
        let metadata_documents = preview
            .assets
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.kind,
                    crate::models::import_v2::ArtifactKind::Metadata
                )
            })
            .map(|artifact| {
                let bytes = verified_artifact(&staging, &artifact.relative_path, &artifact.sha256)?;
                serde_json::from_slice(&bytes).map_err(|_| staging_artifact_error())
            })
            .collect::<Result<Vec<serde_json::Value>, BackendError>>()?;
        let fallback_locator = item
            .input
            .normalized_locator
            .as_deref()
            .unwrap_or(&item.input.locator);
        let candidate = inspect_candidate(CandidateInspection {
            input_kind: &item.input.kind,
            display_name: &preview.title,
            normalized_locator: fallback_locator,
            markdown: &markdown,
            metadata_documents: &metadata_documents,
        })?;
        let content_hash = content_identity_hash(
            &item.input.kind,
            &item.input.media_save_mode,
            &preview.source_snapshot.sha256,
            &preview.markdown.sha256,
            &preview.assets,
        );
        let index_existed = files.exists(context, ".app/source-index-v2.json");
        let index_hash = index_existed
            .then(|| files.file_hash(context, ".app/source-index-v2.json"))
            .transpose()?;
        let index = SourceRegistry::read_index(context, files)?;
        let locator = canonical_candidate_locator(&item.input.kind, &candidate)
            .or_else(|| canonical_platform_locator(&item.input.kind, &markdown))
            .or_else(|| item.input.normalized_locator.clone())
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_STATE_INVALID,
                    "Normalized source locator is missing.",
                )
            })?;
        let resolution = SourceRegistry::resolve(&index, &locator, &content_hash);
        let duplicate = matches!(
            resolution,
            SourceResolution::ExactDuplicate { .. } | SourceResolution::SameContentNewOrigin { .. }
        );
        let pointer = index
            .by_locator
            .get(&locator)
            .or_else(|| index.by_content_hash.get(&content_hash));
        let existing_manifest_path =
            pointer.map(|pointer| format!(".app/sources/{}.json", pointer.source_id));
        let existing_manifest_hash = existing_manifest_path
            .as_deref()
            .map(|path| files.file_hash(context, path))
            .transpose()?;
        let existing_manifest: Option<SourceManifest> = existing_manifest_path
            .as_deref()
            .map(|path| SourceRegistry::read_manifest(context, files, path))
            .transpose()?;
        let existing_package = existing_manifest
            .as_ref()
            .filter(|manifest| manifest.wiki_path.ends_with("/index.md"))
            .map(|manifest| load_current_source_package(context, files, manifest))
            .transpose()?;
        let attempt = item.attempts.last();
        let extension = Path::new(&item.input.display_name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bin")
            .to_string();
        let prepared_source = prepare_source_snapshot(&item.input.kind, &extension, source)?;
        let imported_at = chrono::Utc::now().to_rfc3339();
        let mut plan = SourceRegistry.build_commit_plan(
            &index,
            existing_manifest.as_ref(),
            &SourceCommitInput {
                normalized_locator: locator.clone(),
                content_hash: content_hash.clone(),
                display_name: item.input.display_name.clone(),
                input_kind: item.input.kind.clone(),
                source_extension: prepared_source.extension.clone(),
                source_kind: candidate.source_kind.clone(),
                canonical_url: candidate.canonical_url.clone(),
                platform: candidate.platform.clone(),
                platform_content_id: candidate.platform_content_id.clone(),
                title: candidate.title.clone(),
                author: candidate.author.clone(),
                published_at: candidate.published_at.clone(),
                imported_at: imported_at.clone(),
                language: candidate.language.clone(),
                candidate_markdown_hash: preview.markdown.sha256.clone(),
                route: attempt
                    .map(|value| value.route.clone())
                    .unwrap_or_else(|| "unknown".into()),
                engine_id: attempt
                    .map(|value| value.engine_id.clone())
                    .unwrap_or_else(|| "unknown".into()),
                engine_version: attempt
                    .map(|value| value.engine_version.clone())
                    .unwrap_or_else(|| "unknown".into()),
                quality: preview.quality.clone(),
            },
        )?;
        plan.next_manifest.restricted_content |= item.restricted_content;
        if item.restricted_content {
            if let Some(summary) = item.restricted_identity_summary.as_ref() {
                plan.next_manifest.restricted_identity_summary = Some(summary.clone());
            }
        }
        if !duplicate {
            match (staged_package.is_some(), existing_manifest.as_ref()) {
                (true, Some(manifest)) if !manifest.wiki_path.ends_with("/index.md") => {
                    return Err(commit_error(
                        IMPORT_V2_COMMIT_CONFLICT,
                        "The existing Source is a single page and cannot be replaced by a Source package without an explicit migration.",
                    ));
                }
                (false, Some(manifest)) if manifest.wiki_path.ends_with("/index.md") => {
                    return Err(commit_error(
                        IMPORT_V2_COMMIT_CONFLICT,
                        "The existing Source package cannot be replaced by a single page without an explicit migration.",
                    ));
                }
                (true, None) => {
                    plan.wiki_path = package_entry_wiki_path(&plan.wiki_path)?;
                    plan.next_manifest.wiki_path = plan.wiki_path.clone();
                }
                _ => {}
            }
        }
        let source_root = plan.evidence_root_path.clone();
        let mut evidence_writes: Vec<(String, Vec<u8>, String)> = Vec::new();
        let mut asset_writes: Vec<(String, Vec<u8>, String)> = Vec::new();
        let mut evidence_targets = std::collections::HashSet::new();
        let mut asset_targets = std::collections::HashSet::new();
        for asset in &preview.assets {
            if package_staging_paths.contains(&asset.relative_path) {
                continue;
            }
            let asset_relative = asset
                .relative_path
                .strip_prefix("assets/")
                .unwrap_or(&asset.relative_path);
            let source_evidence_artifact = matches!(
                &asset.kind,
                crate::models::import_v2::ArtifactKind::SourceEvidence
            );
            let relative = if source_evidence_artifact {
                asset_relative
                    .strip_prefix("source-evidence/")
                    .unwrap_or(asset_relative)
            } else {
                asset_relative
            };
            let bytes = verified_artifact(&staging, &asset.relative_path, &asset.sha256)?;
            let (relative, bytes) = if source_evidence_artifact
                && item.input.kind == crate::models::import_v2::ImportInputKind::Url
            {
                prepare_url_source_evidence(relative, bytes)?
            } else {
                (relative.to_string(), bytes)
            };
            let evidence_kind = match asset.kind {
                crate::models::import_v2::ArtifactKind::SourceEvidence => {
                    Some(("evidence", "source_evidence"))
                }
                crate::models::import_v2::ArtifactKind::Metadata => Some(("metadata", "metadata")),
                crate::models::import_v2::ArtifactKind::Subtitle => Some(("subtitles", "subtitle")),
                crate::models::import_v2::ArtifactKind::Transcript => {
                    Some(("transcripts", "transcript"))
                }
                _ => None,
            };
            let (target, record_kind, targets) = if let Some((directory, kind)) = evidence_kind {
                (
                    format!("{source_root}/{directory}/{relative}"),
                    kind.to_string(),
                    &mut evidence_targets,
                )
            } else {
                (
                    format!("{}/{relative}", plan.asset_root_path),
                    "asset".to_string(),
                    &mut asset_targets,
                )
            };
            if !targets.insert(asset_collision_key(&target)) {
                return Err(commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Preview assets resolve to the same target.",
                ));
            }
            if evidence_kind.is_some() {
                evidence_writes.push((target, bytes, record_kind));
            } else {
                asset_writes.push((target, bytes, record_kind));
            }
        }
        let extracted_markdown_target = format!("{source_root}/derived/extracted.md");
        if !evidence_targets.insert(asset_collision_key(&extracted_markdown_target)) {
            return Err(commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Preview extracted Markdown resolves to an existing target.",
            ));
        }
        evidence_writes.push((
            extracted_markdown_target,
            committed_markdown.clone(),
            "candidate_markdown".into(),
        ));
        if committed_markdown_hash != preview.markdown.sha256 {
            let import_candidate_target = format!("{source_root}/derived/import-candidate.md");
            if !evidence_targets.insert(asset_collision_key(&import_candidate_target)) {
                return Err(commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Original import candidate resolves to an existing target.",
                ));
            }
            evidence_writes.push((
                import_candidate_target,
                markdown.clone(),
                "import_candidate_markdown".into(),
            ));
        }
        let quality_target = format!("{source_root}/derived/quality.json");
        if !evidence_targets.insert(asset_collision_key(&quality_target)) {
            return Err(commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Preview quality report resolves to an existing target.",
            ));
        }
        evidence_writes.push((
            quality_target,
            json_bytes(&preview.quality)?,
            "quality_report".into(),
        ));
        let source_record_path = format!("{source_root}/source.json");
        let source_record = SourceEvidenceRecord {
            schema_version: 1,
            title: preview.title.clone(),
            author: candidate.author.clone(),
            url: matches!(
                item.input.kind,
                crate::models::import_v2::ImportInputKind::Url
            )
            .then(|| {
                crate::services::import_v2::redaction::redact_sensitive_text(
                    item.input
                        .normalized_locator
                        .as_deref()
                        .unwrap_or(&item.input.locator),
                )
            }),
            source_locator: crate::services::import_v2::redaction::redact_sensitive_text(&locator),
            media_save_mode: item.input.media_save_mode.clone(),
            restricted: item.restricted_content,
            restricted_identity_summary: item.restricted_identity_summary.clone(),
            snapshot_path: plan.raw_path.clone(),
            snapshot_sha256: preview.source_snapshot.sha256.clone(),
            content_sha256: content_hash.clone(),
            stored_sha256: format!("{:x}", Sha256::digest(&prepared_source.bytes)),
            content_encoding: prepared_source.content_encoding.clone(),
            original_bytes: preview.source_snapshot.size_bytes,
            stored_bytes: prepared_source.bytes.len() as u64,
            saved_at: imported_at.clone(),
        };
        evidence_writes.push((
            source_record_path.clone(),
            json_bytes(&source_record)?,
            "source_record".into(),
        ));
        let new_target_collides = if staged_package.is_some() {
            Path::new(&plan.wiki_path)
                .parent()
                .and_then(Path::to_str)
                .map(|path| files.exists(context, &path.replace('\\', "/")))
                .unwrap_or(true)
        } else {
            files.exists(context, &plan.wiki_path)
        };
        if matches!(resolution, SourceResolution::New) && new_target_collides {
            plan.wiki_path = if staged_package.is_some() {
                collision_free_package_entry_path(context, &plan.wiki_path)?
            } else {
                collision_free_wiki_path(context, &plan.wiki_path)?
            };
            plan.next_manifest.wiki_path = plan.wiki_path.clone();
        }
        if matches!(resolution, SourceResolution::New) {
            let expected_path = preview_target_wiki_path.ok_or_else(stale_resolution_error)?;
            if expected_path != plan.wiki_path {
                if !expected_path.starts_with("wiki/sources/")
                    || context.resolve_project_path(expected_path).is_err()
                {
                    return Err(stale_resolution_error());
                }
                let expected_collides = if staged_package.is_some() {
                    if !expected_path.ends_with("/index.md") {
                        return Err(stale_resolution_error());
                    }
                    Path::new(expected_path)
                        .parent()
                        .and_then(Path::to_str)
                        .map(|path| files.exists(context, &path.replace('\\', "/")))
                        .unwrap_or(true)
                } else {
                    files.exists(context, expected_path)
                };
                if expected_collides {
                    return Err(stale_resolution_error());
                }
                plan.wiki_path = expected_path.to_string();
                plan.next_manifest.wiki_path = plan.wiki_path.clone();
            }
        }
        let wiki_exists = files.exists(context, &plan.wiki_path);
        let current_hash = wiki_exists
            .then(|| files.file_hash(context, &plan.wiki_path))
            .transpose()?;
        let current_version = existing_manifest.as_ref().and_then(|manifest| {
            manifest
                .versions
                .iter()
                .find(|version| version.version_id == manifest.current_version_id)
        });
        let baseline_hash = current_version
            .and_then(|version| version.human_edit_hash.clone())
            .or_else(|| {
                current_version.and_then(|version| {
                    files
                        .exists(context, &version.baseline_path)
                        .then(|| files.file_hash(context, &version.baseline_path).ok())
                        .flatten()
                })
            });
        let human_edited = current_hash
            .as_deref()
            .zip(baseline_hash.as_deref())
            .is_some_and(|(current, baseline)| current != baseline);
        let resolved = resolve_item_resolution(
            &resolution,
            decision.resolution.as_ref(),
            &plan,
            &content_hash,
            current_hash.as_deref(),
            human_edited,
            &committed_markdown_hash,
        )?;
        if resolved.keep_current {
            let current_version_id = plan.previous_current_version_id.clone().ok_or_else(|| {
                commit_error(IMPORT_V2_COMMIT_CONFLICT, "Current Source is missing.")
            })?;
            plan.next_manifest.current_version_id = current_version_id.clone();
            plan.next_index.by_locator.insert(
                locator.clone(),
                SourcePointer {
                    source_id: plan.source_id.clone(),
                    version_id: current_version_id.clone(),
                },
            );
            if let Some(canonical_url) = candidate.canonical_url.as_ref() {
                plan.next_index.by_locator.insert(
                    canonical_url.clone(),
                    SourcePointer {
                        source_id: plan.source_id.clone(),
                        version_id: current_version_id,
                    },
                );
            }
        }
        if !duplicate {
            let version = plan
                .next_manifest
                .versions
                .iter_mut()
                .find(|version| version.version_id == plan.version_id)
                .ok_or_else(|| {
                    commit_error(IMPORT_V2_COMMIT_FAILED, "Source version is missing.")
                })?;
            version.raw_evidence = std::iter::once(artifact_record(
                &plan.raw_path,
                &prepared_source.bytes,
                "source_snapshot",
            ))
            .chain(
                evidence_writes
                    .iter()
                    .map(|(path, bytes, kind)| artifact_record(path, bytes, kind)),
            )
            .collect();
            version.assets = asset_writes
                .iter()
                .map(|(path, bytes, kind)| artifact_record(path, bytes, kind))
                .collect();
            version.candidate = candidate_record(&candidate, committed_markdown_hash.clone());
        }
        if resolved.apply_wiki {
            plan.next_manifest.source_kind = candidate.source_kind.clone();
            plan.next_manifest.title = candidate.title.clone();
            plan.next_manifest
                .canonical_url
                .clone_from(&candidate.canonical_url);
            plan.next_manifest.platform.clone_from(&candidate.platform);
            plan.next_manifest
                .platform_content_id
                .clone_from(&candidate.platform_content_id);
            plan.next_manifest.author.clone_from(&candidate.author);
            plan.next_manifest
                .published_at
                .clone_from(&candidate.published_at);
            plan.next_manifest.language.clone_from(&candidate.language);
        }
        let version_position = plan
            .next_manifest
            .versions
            .iter()
            .position(|version| version.version_id == plan.version_id)
            .ok_or_else(|| commit_error(IMPORT_V2_COMMIT_FAILED, "Source version is missing."))?;
        let final_source = resolved
            .apply_wiki
            .then(|| {
                finalize_source(FinalizationInput {
                    candidate_markdown: &committed_markdown,
                    candidate: &candidate,
                    source_id: &plan.source_id,
                    version_id: &plan.version_id,
                    content_hash: &content_hash,
                    imported_at: &plan.next_manifest.versions[version_position].created_at,
                    quality: &preview.quality,
                    restricted: plan.next_manifest.restricted_content,
                })
            })
            .transpose()?;
        let mut package_writes: Vec<(String, String, Vec<u8>, Option<String>)> = Vec::new();
        let mut package_removals: Vec<(String, String)> = Vec::new();
        if !duplicate {
            if let Some(mut package) = staged_package.as_ref().cloned() {
                let final_source = final_source.as_ref().ok_or_else(|| {
                    commit_error(
                        IMPORT_V2_COMMIT_FAILED,
                        "A Source package cannot be committed without its index page.",
                    )
                })?;
                let wiki_root = Path::new(&plan.wiki_path)
                    .parent()
                    .and_then(Path::to_str)
                    .map(|path| path.replace('\\', "/"))
                    .ok_or_else(|| {
                        commit_error(
                            IMPORT_V2_COMMIT_FAILED,
                            "The Source package path is invalid.",
                        )
                    })?;
                for member in &mut package.members {
                    let (wiki_path, baseline_path, bytes) = if member.role
                        == SourcePackageMemberRole::Index
                    {
                        (
                            plan.wiki_path.clone(),
                            plan.baseline_path.clone(),
                            final_source.bytes.clone(),
                        )
                    } else {
                        let file_name = Path::new(&member.staging_path)
                            .file_name()
                            .and_then(|value| value.to_str())
                            .ok_or_else(staging_artifact_error)?;
                        let wiki_path = format!("{wiki_root}/{file_name}");
                        let baseline_path = format!(
                            ".app/source-artifacts/{}/{}/package/{file_name}",
                            plan.source_id, plan.version_id
                        );
                        let artifact = preview
                            .assets
                            .iter()
                            .find(|artifact| artifact.relative_path == member.staging_path)
                            .ok_or_else(staging_artifact_error)?;
                        let bytes =
                            verified_artifact(&staging, &artifact.relative_path, &artifact.sha256)?;
                        (wiki_path, baseline_path, bytes)
                    };
                    context.resolve_project_path(&wiki_path)?;
                    context.resolve_project_path(&baseline_path)?;
                    let hash = format!("{:x}", Sha256::digest(&bytes));
                    member.wiki_path = wiki_path.clone();
                    member.baseline_path = baseline_path.clone();
                    member.content_hash = hash.clone();
                    member.human_edit_hash = hash;
                    if member.role != SourcePackageMemberRole::Index {
                        let existing_hash = files
                            .exists(context, &wiki_path)
                            .then(|| files.file_hash(context, &wiki_path))
                            .transpose()?;
                        package_writes.push((wiki_path, baseline_path, bytes, existing_hash));
                    }
                }
                package.schema_version =
                    crate::models::source_package::SOURCE_PACKAGE_SCHEMA_VERSION;
                package.source_id = plan.source_id.clone();
                package.version_id = plan.version_id.clone();
                package.entry_wiki_path = plan.wiki_path.clone();
                package.validate_committed().map_err(|_| {
                    commit_error(
                        IMPORT_V2_COMMIT_FAILED,
                        "The committed Source package contract is invalid.",
                    )
                })?;
                if let Some(previous) = existing_package.as_ref() {
                    validate_source_package_update(context, files, previous, &package)?;
                    package_removals.extend(
                        previous
                            .members
                            .iter()
                            .filter(|member| member.role != SourcePackageMemberRole::Index)
                            .filter(|member| {
                                !package
                                    .members
                                    .iter()
                                    .any(|next| next.wiki_path == member.wiki_path)
                            })
                            .map(|member| {
                                (member.wiki_path.clone(), member.human_edit_hash.clone())
                            }),
                    );
                }
                let target = format!("{source_root}/derived/source-package.json");
                if !evidence_targets.insert(asset_collision_key(&target)) {
                    return Err(commit_error(
                        IMPORT_V2_COMMIT_FAILED,
                        "The Source package manifest target collides with another artifact.",
                    ));
                }
                let bytes = json_bytes(&package)?;
                evidence_writes.push((
                    target.clone(),
                    bytes.clone(),
                    "source_package_manifest".into(),
                ));
                let version = &mut plan.next_manifest.versions[version_position];
                version.raw_evidence.push(artifact_record(
                    &target,
                    &bytes,
                    "source_package_manifest",
                ));
            }
        }
        let package_checkpoint_required = (!package_writes.is_empty()
            || !package_removals.is_empty())
            && existing_manifest.is_some()
            && resolved.apply_wiki;
        let mut checkpoint_hash = None;
        if resolved.checkpoint_required || package_checkpoint_required {
            let mut checkpoint_paths = vec![plan.wiki_path.clone()];
            checkpoint_paths.extend(
                package_writes
                    .iter()
                    .filter(|(_, _, _, existing_hash)| existing_hash.is_some())
                    .map(|(wiki_path, _, _, _)| wiki_path.clone()),
            );
            checkpoint_paths.extend(
                package_removals
                    .iter()
                    .map(|(wiki_path, _)| wiki_path.clone()),
            );
            checkpoint_paths.sort();
            checkpoint_paths.dedup();
            let checkpoint = git.create_scoped_checkpoint(
                context,
                CheckpointPurpose::HighRiskOperation,
                "Before import Source update",
                &checkpoint_paths,
            )?;
            checkpoint_hash = checkpoint.commit_hash;
        }
        if let Some(final_source) = final_source.as_ref() {
            let version = &mut plan.next_manifest.versions[version_position];
            version.human_edit_hash = Some(final_source.human_edit_hash.clone());
            version.checkpoint.clone_from(&checkpoint_hash);
            if let Some(event) = plan.next_manifest.timeline.last_mut() {
                event.checkpoint.clone_from(&checkpoint_hash);
            }
            validate_final_source_binding(
                &final_source.bytes,
                &plan.next_manifest,
                &plan.next_manifest.versions[version_position],
            )?;
        }
        let history_preview = if let Some(final_source) = final_source.as_ref() {
            final_source.bytes.clone()
        } else {
            if !wiki_exists {
                return Err(commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Committed Source Markdown is missing.",
                ));
            }
            std::fs::read(context.resolve_project_path(&plan.wiki_path)?)
                .map_err(|_| commit_error(IMPORT_V2_COMMIT_FAILED, "Source could not be read."))?
        };
        SourceRegistry::validate_manifest_contract(&plan.next_manifest)?;
        for path in [
            &plan.raw_path,
            &source_record_path,
            &plan.asset_root_path,
            &plan.evidence_root_path,
            &plan.baseline_path,
            &plan.wiki_path,
            &plan.manifest_path,
            ".app/source-index-v2.json",
            history_path,
        ] {
            context.resolve_project_path(path)?;
        }
        if !duplicate
            && (files.exists(context, &plan.raw_path)
                || files.exists(context, &plan.evidence_root_path)
                || files.exists(context, &plan.baseline_path))
        {
            return Err(commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "Immutable raw version already exists.",
            ));
        }
        let disposition = match &resolution {
            SourceResolution::New => ImportCommitDisposition::NewSource,
            SourceResolution::ExactDuplicate { .. }
            | SourceResolution::SameContentNewOrigin { .. } => {
                ImportCommitDisposition::DuplicateSkipped
            }
            SourceResolution::UpdatedOrigin { .. } if resolved.keep_current => {
                ImportCommitDisposition::KeptCurrent
            }
            SourceResolution::UpdatedOrigin { .. } => ImportCommitDisposition::UpdatedSource,
        };
        let (result_version_id, result_content_hash) = if resolved.keep_current {
            let current = plan
                .next_manifest
                .versions
                .iter()
                .find(|version| version.version_id == plan.next_manifest.current_version_id)
                .ok_or_else(|| {
                    commit_error(
                        IMPORT_V2_COMMIT_FAILED,
                        "Current Source version is missing.",
                    )
                })?;
            (current.version_id.clone(), current.content_hash.clone())
        } else {
            (plan.version_id.clone(), content_hash.clone())
        };
        let mut result_warnings = preview
            .quality
            .warnings
            .iter()
            .map(|warning| completion_warning("IMPORT_QUALITY_WARNING", warning))
            .collect::<Vec<_>>();
        if resolved.keep_current {
            result_warnings.push(completion_warning(
                "IMPORT_SOURCE_KEPT_CURRENT",
                "The imported version was saved, but the current Source was kept unchanged.",
            ));
        }
        let result = ImportItemCommitResult {
            item_id: item.item_id.clone(),
            source_id: Some(plan.source_id.clone()),
            version_id: Some(result_version_id),
            wiki_path: Some(plan.wiki_path.clone()),
            content_hash: Some(result_content_hash),
            disposition: Some(disposition),
            warnings: result_warnings,
            committed: true,
            error_code: None,
        };
        let mut history = prior_batch.clone();
        history.items.push(result.clone());
        update_history_snapshot(&mut history, &result);
        refresh_completion(&mut history);
        let history_preview_path = format!(
            ".app/import-history-previews/{}/{}.md",
            history.batch_id, item.item_id
        );
        history.committed_count =
            history.items.iter().filter(|entry| entry.committed).count() as u32;
        history.failed_count = history.items.len() as u32 - history.committed_count;
        #[cfg(test)]
        {
            let mut targets = Vec::new();
            if !duplicate {
                targets.extend([
                    ("raw snapshot".into(), plan.raw_path.clone()),
                    ("baseline".into(), plan.baseline_path.clone()),
                ]);
                targets.extend(
                    evidence_writes
                        .iter()
                        .map(|(path, _, kind)| (format!("evidence {kind}"), path.clone())),
                );
                targets.extend(
                    asset_writes
                        .iter()
                        .map(|(path, _, kind)| (format!("asset {kind}"), path.clone())),
                );
            }
            if final_source.is_some() {
                targets.push(("Wiki".into(), plan.wiki_path.clone()));
            }
            targets.extend(
                package_writes
                    .iter()
                    .flat_map(|(wiki_path, baseline_path, _, _)| {
                        [
                            ("package page".into(), wiki_path.clone()),
                            ("package baseline".into(), baseline_path.clone()),
                        ]
                    }),
            );
            targets.extend(
                package_removals
                    .iter()
                    .map(|(wiki_path, _)| ("removed package page".into(), wiki_path.clone())),
            );
            targets.extend([
                ("source manifest".into(), plan.manifest_path.clone()),
                ("source index".into(), ".app/source-index-v2.json".into()),
                ("history preview".into(), history_preview_path.clone()),
                ("batch history".into(), history_path.to_string()),
            ]);
            targets.push((
                format!("session item {}", item.item_id),
                format!(
                    "{}/{}/items/{}.json",
                    context
                        .layout
                        .import_state_root
                        .as_deref()
                        .expect("fixture import state root"),
                    session.session_id,
                    item.item_id
                ),
            ));
            targets.push((
                "session summary".into(),
                format!(
                    "{}/{}/session.json",
                    context
                        .layout
                        .import_state_root
                        .as_deref()
                        .expect("fixture import state root"),
                    session.session_id
                ),
            ));
            run_commit_durable_targets_hook(targets);
        }
        let mut transaction = FileTransaction::new_for_project(&context.root);
        let write_result = (|| -> Result<(), BackendError> {
            if !duplicate {
                transaction.write_new(
                    &context.resolve_project_path(&plan.raw_path)?,
                    &prepared_source.bytes,
                )?;
                let baseline_bytes = final_source
                    .as_ref()
                    .map(|source| source.bytes.as_slice())
                    .unwrap_or(markdown.as_slice());
                transaction.write_new(
                    &context.resolve_project_path(&plan.baseline_path)?,
                    baseline_bytes,
                )?;
                for (target, bytes, _) in &evidence_writes {
                    transaction.write_new(&context.resolve_project_path(target)?, bytes)?;
                }
                for (target, bytes, _) in &asset_writes {
                    transaction.write_new(&context.resolve_project_path(target)?, bytes)?;
                }
            }
            if let Some(final_source) = final_source.as_ref() {
                let wiki = context.resolve_project_path(&plan.wiki_path)?;
                if wiki_exists {
                    transaction.write_if_hash_matches(
                        &wiki,
                        &final_source.bytes,
                        current_hash.as_deref().ok_or_else(|| {
                            commit_error(
                                IMPORT_V2_COMMIT_CONFLICT,
                                "Current Source hash is missing.",
                            )
                        })?,
                    )?;
                } else {
                    transaction.write_new(&wiki, &final_source.bytes)?;
                }
            }
            for (wiki_path, baseline_path, bytes, existing_hash) in &package_writes {
                transaction.write_new(&context.resolve_project_path(baseline_path)?, bytes)?;
                let wiki = context.resolve_project_path(wiki_path)?;
                if let Some(existing_hash) = existing_hash {
                    transaction.write_if_hash_matches(&wiki, bytes, existing_hash)?;
                } else {
                    transaction.write_new(&wiki, bytes)?;
                }
            }
            for (wiki_path, existing_hash) in &package_removals {
                transaction.delete_if_hash_matches(
                    &context.resolve_project_path(wiki_path)?,
                    existing_hash,
                )?;
            }
            let manifest_path = context.resolve_project_path(&plan.manifest_path)?;
            if existing_manifest.is_some() {
                transaction.write_if_hash_matches(
                    &manifest_path,
                    &json_bytes(&plan.next_manifest)?,
                    existing_manifest_hash.as_deref().unwrap(),
                )?;
            } else {
                transaction.write_new(&manifest_path, &json_bytes(&plan.next_manifest)?)?;
            }
            let index_path = context.resolve_project_path(".app/source-index-v2.json")?;
            if index_existed {
                transaction.write_if_hash_matches(
                    &index_path,
                    &json_bytes(&plan.next_index)?,
                    index_hash.as_deref().unwrap(),
                )?;
            } else {
                transaction.write_new(&index_path, &json_bytes(&plan.next_index)?)?;
            }
            transaction.write_new(
                &context.resolve_project_path(&history_preview_path)?,
                &history_preview,
            )?;
            transaction.write_if_hash_matches(
                &context.resolve_project_path(history_path)?,
                &json_bytes(&history)?,
                history_expected_hash,
            )?;
            session.items[item_position].status = ImportItemStatus::Committing;
            session.items[item_position].status = ImportItemStatus::Completed;
            let item_path = format!(
                "{}/{}/items/{}.json",
                context.layout.import_state_root.as_deref().ok_or_else(|| {
                    commit_error(
                        IMPORT_V2_STATE_INVALID,
                        "Import state is unavailable for this project layout.",
                    )
                })?,
                session.session_id,
                item.item_id
            );
            transaction.write_if_hash_matches(
                &context.resolve_project_path(&item_path)?,
                &serde_json::to_vec_pretty(&session.items[item_position]).map_err(|_| {
                    commit_error(
                        IMPORT_V2_COMMIT_FAILED,
                        "Import item could not be serialized.",
                    )
                })?,
                item_expected_hash,
            )?;
            Ok(())
        })();
        if let Err(error) = write_result {
            return Err(transaction.rollback_after(error));
        }
        transaction.commit()?;
        remove_committed_clipboard_input(context, &session.session_id, &item.input);
        Ok(result)
    }
}

fn remove_committed_clipboard_input(
    context: &ProjectContext,
    session_id: &str,
    input: &crate::models::import_v2::ImportInput,
) {
    if input.kind != ImportInputKind::ClipboardText {
        return;
    }
    let expected_prefix = format!(".app/import-sessions/{session_id}/inputs/");
    if !input
        .locator
        .replace('\\', "/")
        .starts_with(&expected_prefix)
    {
        return;
    }
    if let Ok(path) = context.resolve_project_path(&input.locator) {
        let _ = remove_project_file(&context.root, &path);
    }
}

const MAX_URL_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;

struct PreparedSourceSnapshot {
    bytes: Vec<u8>,
    extension: String,
    content_encoding: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceEvidenceRecord {
    schema_version: u32,
    title: String,
    author: Option<String>,
    url: Option<String>,
    source_locator: String,
    media_save_mode: crate::models::import_v2::MediaSaveMode,
    restricted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    restricted_identity_summary: Option<String>,
    snapshot_path: String,
    snapshot_sha256: String,
    content_sha256: String,
    stored_sha256: String,
    content_encoding: Option<String>,
    original_bytes: u64,
    stored_bytes: u64,
    saved_at: String,
}

fn content_identity_hash(
    kind: &crate::models::import_v2::ImportInputKind,
    media_save_mode: &crate::models::import_v2::MediaSaveMode,
    snapshot_sha256: &str,
    markdown_sha256: &str,
    assets: &[crate::models::import_v2::ImportArtifact],
) -> String {
    if kind != &crate::models::import_v2::ImportInputKind::Url {
        return snapshot_sha256.to_string();
    }
    let mut evidence = assets
        .iter()
        .filter_map(|artifact| {
            let tag = match artifact.kind {
                crate::models::import_v2::ArtifactKind::Image => "image",
                crate::models::import_v2::ArtifactKind::Attachment => "attachment",
                crate::models::import_v2::ArtifactKind::Subtitle => "subtitle",
                crate::models::import_v2::ArtifactKind::Transcript => "transcript",
                _ => return None,
            };
            Some(format!("{tag}:{}", artifact.sha256))
        })
        .collect::<Vec<_>>();
    evidence.sort();
    let mut digest = Sha256::new();
    digest.update(b"url-content-v1\0");
    digest.update(match media_save_mode {
        crate::models::import_v2::MediaSaveMode::PreserveOriginal => {
            b"preserve-original".as_slice()
        }
        crate::models::import_v2::MediaSaveMode::ExtractOnly => b"extract-only".as_slice(),
    });
    digest.update(b"\0");
    digest.update(markdown_sha256.as_bytes());
    for artifact in evidence {
        digest.update(b"\0");
        digest.update(artifact.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn prepare_source_snapshot(
    kind: &crate::models::import_v2::ImportInputKind,
    fallback_extension: &str,
    bytes: Vec<u8>,
) -> Result<PreparedSourceSnapshot, BackendError> {
    if *kind != crate::models::import_v2::ImportInputKind::Url {
        return Ok(PreparedSourceSnapshot {
            bytes,
            extension: fallback_extension.into(),
            content_encoding: None,
        });
    }
    if bytes.len() > MAX_URL_SNAPSHOT_BYTES {
        return Err(commit_error(
            IMPORT_V2_COMMIT_FAILED,
            "URL source snapshot exceeds the 16 MiB evidence limit.",
        ));
    }
    let text = std::str::from_utf8(&bytes).ok();
    let trimmed = text
        .map(|value| value.trim_start_matches('\u{feff}').trim_start())
        .unwrap_or_default();
    let fallback_extension = fallback_extension.to_ascii_lowercase();
    let json = serde_json::from_str::<serde_json::Value>(trimmed).is_ok();
    let format = if json
        && (trimmed.starts_with('{') || trimmed.starts_with('[') || fallback_extension == "json")
    {
        Some("json")
    } else if text.is_some()
        && (trimmed.starts_with('<') || fallback_extension == "html" || fallback_extension == "htm")
    {
        Some("html")
    } else {
        None
    };
    let Some(format) = format else {
        return Ok(PreparedSourceSnapshot {
            bytes,
            extension: "bin".into(),
            content_encoding: None,
        });
    };
    let bytes = if format == "json" {
        crate::services::import_v2::redaction::redact_json_snapshot(trimmed).ok_or_else(|| {
            commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "URL JSON snapshot could not be redacted.",
            )
        })?
    } else {
        crate::services::import_v2::redaction::redact_sensitive_text(
            text.expect("recognized HTML snapshots are UTF-8"),
        )
        .into_bytes()
    };
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&bytes).map_err(|_| {
        commit_error(
            IMPORT_V2_COMMIT_FAILED,
            "URL source snapshot could not be compressed.",
        )
    })?;
    let compressed = encoder.finish().map_err(|_| {
        commit_error(
            IMPORT_V2_COMMIT_FAILED,
            "URL source snapshot could not be compressed.",
        )
    })?;
    Ok(PreparedSourceSnapshot {
        bytes: compressed,
        extension: format!("{format}.gz"),
        content_encoding: Some("gzip".into()),
    })
}

fn prepare_url_source_evidence(
    relative: &str,
    bytes: Vec<u8>,
) -> Result<(String, Vec<u8>), BackendError> {
    let fallback_extension = relative
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or("bin");
    let prepared = prepare_source_snapshot(
        &crate::models::import_v2::ImportInputKind::Url,
        fallback_extension,
        bytes,
    )?;
    if prepared.content_encoding.is_none() {
        return Ok((relative.to_string(), prepared.bytes));
    }
    let stem = relative
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(relative);
    Ok((format!("{stem}.{}", prepared.extension), prepared.bytes))
}

fn update_history_snapshot(batch: &mut ImportBatchResult, result: &ImportItemCommitResult) {
    let Some(snapshot) = batch.history_snapshot.as_mut() else {
        return;
    };
    if let Some(item) = snapshot
        .items
        .iter_mut()
        .find(|item| item.item_id == result.item_id)
    {
        item.status = if result.committed {
            ImportItemStatus::Completed
        } else if result.error_code.as_deref() == Some(crate::errors::IMPORT_V2_CANCELLED) {
            ImportItemStatus::Cancelled
        } else {
            ImportItemStatus::Failed
        };
        item.progress = None;
        item.issue = result
            .error_code
            .as_deref()
            .map(ImportIssue::for_commit_code);
    }
    snapshot.status = derive_session_status(&snapshot.items);
    snapshot.updated_at = chrono::Utc::now().to_rfc3339();
}

fn completion_warning(code: &str, title: &str) -> UserIssue {
    UserIssue {
        code: code.into(),
        title: title.into(),
        data_safety: "Saved Source files were not changed by this warning.".into(),
        primary_action: None,
        detail: None,
    }
}

fn refresh_completion(batch: &mut ImportBatchResult) {
    let mut completion = ImportCompletion::empty(&batch.session_id, &batch.batch_id);
    for result in &batch.items {
        completion.warnings.extend(result.warnings.clone());
        if !result.committed {
            let code = result
                .error_code
                .clone()
                .unwrap_or_else(|| IMPORT_V2_COMMIT_FAILED.into());
            let input_label = batch
                .history_snapshot
                .as_ref()
                .and_then(|snapshot| {
                    snapshot
                        .items
                        .iter()
                        .find(|item| item.item_id == result.item_id)
                })
                .map(|item| item.input.display_name.clone())
                .unwrap_or_else(|| "Import item".into());
            completion.failures.push(ItemFailure {
                item_id: result.item_id.clone(),
                input_label,
                issue: UserIssue {
                    title: "This item could not be imported.".into(),
                    data_safety: "Other successfully imported Sources were kept.".into(),
                    primary_action: Some(ImportPrimaryAction::Retry),
                    detail: Some(ImportIssueDiagnostics {
                        technical_code: Some(code.clone()),
                        ..ImportIssueDiagnostics::default()
                    }),
                    code,
                },
            });
            continue;
        }
        let Some(source_id) = result.source_id.clone() else {
            continue;
        };
        let Some(version_id) = result.version_id.clone() else {
            continue;
        };
        let Some(content_hash) = result.content_hash.clone() else {
            continue;
        };
        match result.disposition.as_ref() {
            Some(ImportCommitDisposition::NewSource) => {
                if let Some(wiki_path) = result.wiki_path.clone() {
                    completion.new_sources.push(SourceVersionChange {
                        source_id,
                        version_id,
                        wiki_path,
                        content_hash,
                    });
                }
            }
            Some(ImportCommitDisposition::UpdatedSource) => {
                if let Some(wiki_path) = result.wiki_path.clone() {
                    completion.updated_sources.push(SourceVersionChange {
                        source_id,
                        version_id,
                        wiki_path,
                        content_hash,
                    });
                }
            }
            Some(ImportCommitDisposition::DuplicateSkipped) => {
                completion.duplicate_skips.push(DuplicateResult {
                    item_id: result.item_id.clone(),
                    source_id,
                    version_id,
                    content_hash,
                });
            }
            Some(ImportCommitDisposition::KeptCurrent) | None => {}
        }
    }
    batch.completion = Some(completion);
}

#[cfg_attr(not(any(feature = "gui", test)), allow(dead_code))]
struct PlannedWikiTarget {
    preferred: String,
    locator: String,
    content_hash: String,
    package: bool,
}

#[cfg_attr(not(any(feature = "gui", test)), allow(dead_code))]
fn planned_wiki_target_identity(
    context: &ProjectContext,
    session_id: &str,
    item: &ImportItem,
) -> Result<PlannedWikiTarget, BackendError> {
    let preview = item
        .preview
        .as_ref()
        .ok_or_else(|| commit_error(IMPORT_V2_STATE_INVALID, "Import preview is missing."))?;
    let staging = context.root.join(format!(
        ".app/import-sessions/{session_id}/items/{}/staging",
        item.item_id
    ));
    let markdown = verified_artifact(
        &staging,
        &preview.markdown.relative_path,
        &preview.markdown.sha256,
    )?;
    let metadata_documents = preview
        .assets
        .iter()
        .filter(|artifact| matches!(artifact.kind, ArtifactKind::Metadata))
        .map(|artifact| {
            let bytes = verified_artifact(&staging, &artifact.relative_path, &artifact.sha256)?;
            serde_json::from_slice(&bytes).map_err(|_| staging_artifact_error())
        })
        .collect::<Result<Vec<serde_json::Value>, BackendError>>()?;
    let fallback_locator = item
        .input
        .normalized_locator
        .as_deref()
        .unwrap_or(&item.input.locator);
    let candidate = inspect_candidate(CandidateInspection {
        input_kind: &item.input.kind,
        display_name: &preview.title,
        normalized_locator: fallback_locator,
        markdown: &markdown,
        metadata_documents: &metadata_documents,
    })?;
    let locator = canonical_candidate_locator(&item.input.kind, &candidate)
        .or_else(|| canonical_platform_locator(&item.input.kind, &markdown))
        .or_else(|| item.input.normalized_locator.clone())
        .ok_or_else(|| {
            commit_error(
                IMPORT_V2_STATE_INVALID,
                "Normalized source locator is missing.",
            )
        })?;
    let mut preferred = crate::services::import_v2::source_registry::derive_wiki_path_for_input(
        &item.input.kind,
        &item.input.display_name,
        &locator,
        candidate.canonical_url.as_deref(),
    );
    let package = preview
        .assets
        .iter()
        .any(|artifact| artifact.relative_path == "source-package.json");
    if package {
        preferred = package_entry_wiki_path(&preferred)?;
    }
    Ok(PlannedWikiTarget {
        preferred,
        locator,
        content_hash: content_identity_hash(
            &item.input.kind,
            &item.input.media_save_mode,
            &preview.source_snapshot.sha256,
            &preview.markdown.sha256,
            &preview.assets,
        ),
        package,
    })
}

#[cfg_attr(not(any(feature = "gui", test)), allow(dead_code))]
pub(crate) fn planned_new_source_wiki_path(
    context: &ProjectContext,
    files: &FileStore,
    session: &ImportSession,
    item_id: &str,
) -> Result<Option<String>, BackendError> {
    planned_new_source_wiki_path_internal(context, files, session, item_id, false)
}

fn planned_new_source_wiki_path_internal(
    context: &ProjectContext,
    files: &FileStore,
    session: &ImportSession,
    item_id: &str,
    preserve_bound_predecessors: bool,
) -> Result<Option<String>, BackendError> {
    let index = SourceRegistry::read_index(context, files)?;
    let mut reserved = std::collections::HashSet::new();
    for item in &session.items {
        let target_item = item.item_id == item_id;
        if !target_item && (!item.selected || item.status != ImportItemStatus::PreviewReady) {
            continue;
        }
        let Some(resolution) = item
            .preview
            .as_ref()
            .and_then(|preview| preview.resolution.as_ref())
        else {
            if target_item {
                return Ok(None);
            }
            continue;
        };
        if resolution.kind != ImportResolutionKind::NewSource {
            if target_item {
                return Ok(None);
            }
            continue;
        }
        if !target_item && preserve_bound_predecessors {
            if let Some(bound_target) = resolution.target_wiki_path.as_deref() {
                let package = item.preview.as_ref().is_some_and(|preview| {
                    preview
                        .assets
                        .iter()
                        .any(|artifact| artifact.relative_path == "source-package.json")
                });
                reserved.insert(wiki_target_reservation_key(bound_target, package)?);
                continue;
            }
        }
        let planned = planned_wiki_target_identity(context, &session.session_id, item)?;
        if target_item && item.status == ImportItemStatus::Completed {
            if let Some(pointer) = index
                .by_locator
                .get(&planned.locator)
                .or_else(|| index.by_content_hash.get(&planned.content_hash))
            {
                let manifest = SourceRegistry::read_manifest(
                    context,
                    files,
                    &format!(".app/sources/{}.json", pointer.source_id),
                )?;
                return Ok(Some(manifest.wiki_path));
            }
        }
        let reserved_key = wiki_target_reservation_key(&planned.preferred, planned.package)?;
        let disk_collision = if planned.package {
            Path::new(&planned.preferred)
                .parent()
                .and_then(Path::to_str)
                .map(|path| files.exists(context, &path.replace('\\', "/")))
                .unwrap_or(true)
        } else {
            files.exists(context, &planned.preferred)
        };
        let chosen = if disk_collision || reserved.contains(&reserved_key) {
            if planned.package {
                collision_free_package_entry_path_avoiding(context, &planned.preferred, &reserved)?
            } else {
                collision_free_wiki_path_avoiding(context, &planned.preferred, &reserved)?
            }
        } else {
            planned.preferred
        };
        if target_item {
            return Ok(Some(chosen));
        }
        let chosen_key = wiki_target_reservation_key(&chosen, planned.package)?;
        reserved.insert(chosen_key);
    }
    Ok(None)
}

fn wiki_target_reservation_key(target: &str, package: bool) -> Result<String, BackendError> {
    if package {
        Path::new(target)
            .parent()
            .and_then(Path::to_str)
            .map(|path| asset_collision_key(&path.replace('\\', "/")))
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Source package target directory is invalid.",
                )
            })
    } else {
        Ok(asset_collision_key(target))
    }
}

pub(crate) fn backfill_missing_new_source_wiki_targets(
    context: &ProjectContext,
    files: &FileStore,
    session: &mut ImportSession,
) -> Result<bool, BackendError> {
    let item_ids = session
        .items
        .iter()
        .filter(|item| item.selected && item.status == ImportItemStatus::PreviewReady)
        .filter(|item| {
            item.preview
                .as_ref()
                .and_then(|preview| preview.resolution.as_ref())
                .is_some_and(|resolution| {
                    resolution.kind == ImportResolutionKind::NewSource
                        && resolution.target_wiki_path.is_none()
                })
        })
        .map(|item| item.item_id.clone())
        .collect::<Vec<_>>();
    for item_id in &item_ids {
        let target = planned_new_source_wiki_path_internal(context, files, session, item_id, true)?;
        let item = session
            .items
            .iter_mut()
            .find(|item| item.item_id == *item_id)
            .ok_or_else(|| commit_error(IMPORT_V2_STATE_INVALID, "Import item is missing."))?;
        item.preview
            .as_mut()
            .and_then(|preview| preview.resolution.as_mut())
            .ok_or_else(stale_resolution_error)?
            .target_wiki_path = target;
    }
    Ok(!item_ids.is_empty())
}

pub(crate) fn refresh_new_source_wiki_targets(
    context: &ProjectContext,
    files: &FileStore,
    session: &mut ImportSession,
) -> Result<(), BackendError> {
    let item_ids = session
        .items
        .iter()
        .filter(|item| item.selected && item.status == ImportItemStatus::PreviewReady)
        .filter(|item| {
            item.preview
                .as_ref()
                .and_then(|preview| preview.resolution.as_ref())
                .is_some_and(|resolution| resolution.kind == ImportResolutionKind::NewSource)
        })
        .map(|item| item.item_id.clone())
        .collect::<Vec<_>>();
    for item_id in item_ids {
        let target = planned_new_source_wiki_path(context, files, session, &item_id)?;
        let item = session
            .items
            .iter_mut()
            .find(|item| item.item_id == item_id)
            .ok_or_else(|| commit_error(IMPORT_V2_STATE_INVALID, "Import item is missing."))?;
        item.preview
            .as_mut()
            .and_then(|preview| preview.resolution.as_mut())
            .ok_or_else(stale_resolution_error)?
            .target_wiki_path = target;
    }
    Ok(())
}

pub(crate) fn collision_free_wiki_path(
    context: &ProjectContext,
    preferred: &str,
) -> Result<String, BackendError> {
    collision_free_wiki_path_avoiding(context, preferred, &std::collections::HashSet::new())
}

fn collision_free_wiki_path_avoiding(
    context: &ProjectContext,
    preferred: &str,
    reserved: &std::collections::HashSet<String>,
) -> Result<String, BackendError> {
    let preferred_path = Path::new(preferred);
    let parent = preferred_path.parent().ok_or_else(|| {
        commit_error(IMPORT_V2_COMMIT_FAILED, "Wiki target directory is invalid.")
    })?;
    let stem = preferred_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| commit_error(IMPORT_V2_COMMIT_FAILED, "Wiki target name is invalid."))?;
    let absolute_parent =
        context.resolve_project_path(&parent.to_string_lossy().replace('\\', "/"))?;
    let mut existing = if absolute_parent.is_dir() {
        std::fs::read_dir(&absolute_parent)
            .map_err(|_| {
                commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Wiki target directory could not be inspected.",
                )
            })?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|name| name.nfc().collect::<String>().to_lowercase())
            })
            .collect::<std::collections::HashSet<_>>()
    } else {
        std::collections::HashSet::new()
    };
    for target in reserved {
        let reserved_path = Path::new(target);
        if reserved_path.parent() == Some(parent) {
            if let Some(filename) = reserved_path.file_name().and_then(|value| value.to_str()) {
                existing.insert(filename.nfc().collect::<String>().to_lowercase());
            }
        }
    }
    for suffix in 2u32.. {
        let suffix = format!("-{suffix}");
        let max_stem_bytes = 120usize.saturating_sub(suffix.len());
        let mut bounded = String::new();
        for ch in stem.chars() {
            if bounded.len() + ch.len_utf8() > max_stem_bytes {
                break;
            }
            bounded.push(ch);
        }
        let filename = format!("{bounded}{suffix}.md");
        let portable_key = filename.nfc().collect::<String>().to_lowercase();
        if !existing.contains(&portable_key) {
            return Ok(format!(
                "{}/{}",
                parent.to_string_lossy().replace('\\', "/"),
                filename
            ));
        }
    }
    unreachable!()
}

fn package_entry_wiki_path(preferred: &str) -> Result<String, BackendError> {
    let preferred = Path::new(preferred);
    if preferred.file_name().and_then(|value| value.to_str()) == Some("index.md") {
        return Ok(preferred.to_string_lossy().replace('\\', "/"));
    }
    let parent = preferred.parent().ok_or_else(|| {
        commit_error(
            IMPORT_V2_COMMIT_FAILED,
            "Source package target directory is invalid.",
        )
    })?;
    let stem = preferred
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Source package target name is invalid.",
            )
        })?;
    Ok(parent
        .join(stem)
        .join("index.md")
        .to_string_lossy()
        .replace('\\', "/"))
}

fn load_current_source_package(
    context: &ProjectContext,
    files: &FileStore,
    manifest: &SourceManifest,
) -> Result<SourcePackageManifest, BackendError> {
    let version = manifest
        .versions
        .iter()
        .find(|version| version.version_id == manifest.current_version_id)
        .ok_or_else(|| {
            commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "The current Source package version is missing.",
            )
        })?;
    let record = version
        .raw_evidence
        .iter()
        .find(|record| record.kind == "source_package_manifest")
        .ok_or_else(|| {
            commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "The current Source package descriptor is missing.",
            )
        })?;
    let package: SourcePackageManifest = files.read_json(context, &record.path).map_err(|_| {
        commit_error(
            IMPORT_V2_COMMIT_CONFLICT,
            "The current Source package descriptor could not be verified.",
        )
    })?;
    package.validate_committed().map_err(|_| {
        commit_error(
            IMPORT_V2_COMMIT_CONFLICT,
            "The current Source package descriptor is invalid.",
        )
    })?;
    if package.source_id != manifest.source_id
        || package.version_id != version.version_id
        || package.entry_wiki_path != manifest.wiki_path
    {
        return Err(commit_error(
            IMPORT_V2_COMMIT_CONFLICT,
            "The current Source package descriptor does not match its Source version.",
        ));
    }
    Ok(package)
}

fn validate_source_package_update(
    context: &ProjectContext,
    files: &FileStore,
    previous: &SourcePackageManifest,
    next: &SourcePackageManifest,
) -> Result<(), BackendError> {
    for member in previous
        .members
        .iter()
        .filter(|member| member.role != SourcePackageMemberRole::Index)
    {
        let current_hash = files.file_hash(context, &member.wiki_path).map_err(|_| {
            commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "A current Source package page is missing.",
            )
        })?;
        if current_hash != member.human_edit_hash {
            return Err(commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "A Source package page has human edits and cannot be overwritten.",
            ));
        }
    }
    for member in next
        .members
        .iter()
        .filter(|member| member.role != SourcePackageMemberRole::Index)
        .filter(|member| {
            !previous
                .members
                .iter()
                .any(|previous| previous.wiki_path == member.wiki_path)
        })
    {
        if files.exists(context, &member.wiki_path) {
            return Err(commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "A new Source package page would overwrite an unrelated Wiki page.",
            ));
        }
    }
    Ok(())
}

fn collision_free_package_entry_path(
    context: &ProjectContext,
    preferred: &str,
) -> Result<String, BackendError> {
    collision_free_package_entry_path_avoiding(
        context,
        preferred,
        &std::collections::HashSet::new(),
    )
}

fn collision_free_package_entry_path_avoiding(
    context: &ProjectContext,
    preferred: &str,
    reserved: &std::collections::HashSet<String>,
) -> Result<String, BackendError> {
    let preferred = Path::new(preferred);
    let package_directory = preferred.parent().ok_or_else(|| {
        commit_error(
            IMPORT_V2_COMMIT_FAILED,
            "Source package target directory is invalid.",
        )
    })?;
    let parent = package_directory.parent().ok_or_else(|| {
        commit_error(
            IMPORT_V2_COMMIT_FAILED,
            "Source package parent directory is invalid.",
        )
    })?;
    let stem = package_directory
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Source package target name is invalid.",
            )
        })?;
    for suffix in 2u32.. {
        let package_name = format!("{stem}-{suffix}");
        let candidate = parent.join(&package_name).join("index.md");
        let candidate_relative = candidate.to_string_lossy().replace('\\', "/");
        let candidate_directory = candidate
            .parent()
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Source package target directory is invalid.",
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        if !context.resolve_project_path(&candidate_directory)?.exists()
            && !reserved.contains(&asset_collision_key(&candidate_directory))
        {
            return Ok(candidate_relative);
        }
    }
    unreachable!()
}

const ABSENT_CURRENT_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

fn derive_resolution_context(
    context: &ProjectContext,
    files: &FileStore,
    session_id: &str,
    item: &ImportItem,
) -> Result<ImportResolutionContext, BackendError> {
    let preview = item
        .preview
        .as_ref()
        .ok_or_else(|| commit_error(IMPORT_V2_STATE_INVALID, "Import preview is missing."))?;
    let staging = context.root.join(format!(
        ".app/import-sessions/{}/items/{}/staging",
        session_id, item.item_id
    ));
    derive_resolution_context_from_staging(context, files, item, preview, &staging)
}

fn derive_resolution_context_from_staging(
    context: &ProjectContext,
    files: &FileStore,
    item: &ImportItem,
    preview: &crate::models::import_v2::ImportPreviewArtifact,
    staging: &Path,
) -> Result<ImportResolutionContext, BackendError> {
    let markdown = verified_artifact(
        staging,
        &preview.markdown.relative_path,
        &preview.markdown.sha256,
    )?;
    let metadata_documents = preview
        .assets
        .iter()
        .filter(|artifact| matches!(artifact.kind, ArtifactKind::Metadata))
        .map(|artifact| {
            let bytes = verified_artifact(staging, &artifact.relative_path, &artifact.sha256)?;
            serde_json::from_slice(&bytes).map_err(|_| staging_artifact_error())
        })
        .collect::<Result<Vec<serde_json::Value>, BackendError>>()?;
    let fallback_locator = item
        .input
        .normalized_locator
        .as_deref()
        .unwrap_or(&item.input.locator);
    let candidate = inspect_candidate(CandidateInspection {
        input_kind: &item.input.kind,
        display_name: &preview.title,
        normalized_locator: fallback_locator,
        markdown: &markdown,
        metadata_documents: &metadata_documents,
    })?;
    let candidate_hash = content_identity_hash(
        &item.input.kind,
        &item.input.media_save_mode,
        &preview.source_snapshot.sha256,
        &preview.markdown.sha256,
        &preview.assets,
    );
    let locator = canonical_candidate_locator(&item.input.kind, &candidate)
        .or_else(|| canonical_platform_locator(&item.input.kind, &markdown))
        .or_else(|| item.input.normalized_locator.clone())
        .ok_or_else(|| {
            commit_error(
                IMPORT_V2_STATE_INVALID,
                "Normalized source locator is missing.",
            )
        })?;
    let index = SourceRegistry::read_index(context, files)?;
    let source_resolution = SourceRegistry::resolve(&index, &locator, &candidate_hash);
    if source_resolution == SourceResolution::New {
        return Ok(ImportResolutionContext {
            kind: ImportResolutionKind::NewSource,
            binding: None,
            default_resolution: Some(ImportItemResolution::NewSource),
            target_wiki_path: None,
        });
    }
    let source_id = match &source_resolution {
        SourceResolution::ExactDuplicate { source_id, .. }
        | SourceResolution::SameContentNewOrigin { source_id, .. }
        | SourceResolution::UpdatedOrigin { source_id, .. } => source_id,
        SourceResolution::New => unreachable!(),
    };
    let manifest =
        SourceRegistry::read_manifest(context, files, &format!(".app/sources/{source_id}.json"))?;
    let current_hash = files.file_hash_if_exists(context, &manifest.wiki_path)?;
    let current_version = manifest
        .versions
        .iter()
        .find(|version| version.version_id == manifest.current_version_id)
        .ok_or_else(|| {
            commit_error(
                IMPORT_V2_STATE_INVALID,
                "Current Source version is missing.",
            )
        })?;
    let baseline_hash = current_version.human_edit_hash.clone().or_else(|| {
        files
            .file_hash_if_exists(context, &current_version.baseline_path)
            .ok()
            .flatten()
    });
    let human_edited = current_hash
        .as_deref()
        .zip(baseline_hash.as_deref())
        .is_some_and(|(current, baseline)| current != baseline);
    let binding = ImportResolutionBinding {
        source_id: source_id.clone(),
        candidate_hash,
        current_hash: bound_current_hash(current_hash.as_deref()).to_string(),
        target_version_id: manifest.current_version_id,
    };
    let (kind, default_resolution) = match source_resolution {
        SourceResolution::ExactDuplicate { .. } | SourceResolution::SameContentNewOrigin { .. } => {
            (
                ImportResolutionKind::ExactDuplicate,
                Some(ImportItemResolution::ExactDuplicateSkip {
                    source_id: binding.source_id.clone(),
                    candidate_hash: binding.candidate_hash.clone(),
                    current_hash: binding.current_hash.clone(),
                    target_version_id: binding.target_version_id.clone(),
                }),
            )
        }
        SourceResolution::UpdatedOrigin { .. } if !human_edited => (
            ImportResolutionKind::SameSourceNewVersion,
            Some(ImportItemResolution::SameSourceNewVersion {
                source_id: binding.source_id.clone(),
                candidate_hash: binding.candidate_hash.clone(),
                current_hash: binding.current_hash.clone(),
                target_version_id: binding.target_version_id.clone(),
            }),
        ),
        SourceResolution::UpdatedOrigin { .. } => (ImportResolutionKind::NeedsThreeWayMerge, None),
        SourceResolution::New => unreachable!(),
    };
    Ok(ImportResolutionContext {
        kind,
        binding: Some(binding),
        default_resolution,
        target_wiki_path: Some(manifest.wiki_path),
    })
}

fn bound_current_hash(current_hash: Option<&str>) -> &str {
    current_hash.unwrap_or(ABSENT_CURRENT_HASH)
}

struct ResolvedItemCommit {
    keep_current: bool,
    apply_wiki: bool,
    checkpoint_required: bool,
}

fn resolve_item_resolution(
    source_resolution: &SourceResolution,
    requested: Option<&ImportItemResolution>,
    plan: &crate::services::import_v2::source_registry::SourceCommitPlan,
    candidate_hash: &str,
    current_hash: Option<&str>,
    human_edited: bool,
    merged_candidate_hash: &str,
) -> Result<ResolvedItemCommit, BackendError> {
    let current_version_id = plan.previous_current_version_id.as_deref();
    match source_resolution {
        SourceResolution::New => {
            if requested
                .is_some_and(|resolution| !matches!(resolution, ImportItemResolution::NewSource))
            {
                return Err(stale_resolution_error());
            }
            Ok(ResolvedItemCommit {
                keep_current: false,
                apply_wiki: true,
                checkpoint_required: false,
            })
        }
        SourceResolution::ExactDuplicate { .. } | SourceResolution::SameContentNewOrigin { .. } => {
            let resolution = requested.ok_or_else(stale_resolution_error)?;
            let ImportItemResolution::ExactDuplicateSkip { .. } = resolution else {
                return Err(stale_resolution_error());
            };
            validate_resolution_binding(
                resolution,
                &plan.source_id,
                candidate_hash,
                current_hash,
                current_version_id,
                merged_candidate_hash,
            )?;
            Ok(ResolvedItemCommit {
                keep_current: false,
                apply_wiki: current_hash.is_none(),
                checkpoint_required: false,
            })
        }
        SourceResolution::UpdatedOrigin { .. } if !human_edited => {
            let resolution = requested.ok_or_else(stale_resolution_error)?;
            if !matches!(
                resolution,
                ImportItemResolution::SameSourceNewVersion { .. }
                    | ImportItemResolution::ApplyImportCandidate { .. }
            ) {
                return Err(stale_resolution_error());
            }
            validate_resolution_binding(
                resolution,
                &plan.source_id,
                candidate_hash,
                current_hash,
                current_version_id,
                merged_candidate_hash,
            )?;
            Ok(ResolvedItemCommit {
                keep_current: false,
                apply_wiki: true,
                checkpoint_required: current_hash.is_some(),
            })
        }
        SourceResolution::UpdatedOrigin { .. } => {
            let resolution = requested.ok_or_else(stale_resolution_error)?;
            if !matches!(
                resolution,
                ImportItemResolution::KeepCurrentSource { .. }
                    | ImportItemResolution::ApplyImportCandidate { .. }
                    | ImportItemResolution::ManualMerge { .. }
            ) {
                return Err(stale_resolution_error());
            }
            validate_resolution_binding(
                resolution,
                &plan.source_id,
                candidate_hash,
                current_hash,
                current_version_id,
                merged_candidate_hash,
            )?;
            let keep_current = matches!(resolution, ImportItemResolution::KeepCurrentSource { .. });
            Ok(ResolvedItemCommit {
                keep_current,
                apply_wiki: !keep_current,
                checkpoint_required: !keep_current,
            })
        }
    }
}

fn validate_resolution_binding(
    resolution: &ImportItemResolution,
    expected_source_id: &str,
    expected_candidate_hash: &str,
    expected_current_hash: Option<&str>,
    expected_target_version_id: Option<&str>,
    expected_merged_hash: &str,
) -> Result<(), BackendError> {
    let (source_id, candidate_hash, current_hash, target_version_id, merged_hash) = match resolution
    {
        ImportItemResolution::NewSource => return Err(stale_resolution_error()),
        ImportItemResolution::ExactDuplicateSkip {
            source_id,
            candidate_hash,
            current_hash,
            target_version_id,
        }
        | ImportItemResolution::SameSourceNewVersion {
            source_id,
            candidate_hash,
            current_hash,
            target_version_id,
        }
        | ImportItemResolution::KeepCurrentSource {
            source_id,
            candidate_hash,
            current_hash,
            target_version_id,
        }
        | ImportItemResolution::ApplyImportCandidate {
            source_id,
            candidate_hash,
            current_hash,
            target_version_id,
        } => (
            source_id,
            candidate_hash,
            current_hash,
            target_version_id,
            None,
        ),
        ImportItemResolution::ManualMerge {
            source_id,
            candidate_hash,
            current_hash,
            target_version_id,
            merged_hash,
        } => (
            source_id,
            candidate_hash,
            current_hash,
            target_version_id,
            Some(merged_hash.as_str()),
        ),
    };
    if source_id != expected_source_id
        || candidate_hash != expected_candidate_hash
        || current_hash != bound_current_hash(expected_current_hash)
        || Some(target_version_id.as_str()) != expected_target_version_id
        || merged_hash.is_some_and(|hash| hash != expected_merged_hash)
    {
        return Err(stale_resolution_error());
    }
    Ok(())
}

fn stale_resolution_error() -> BackendError {
    commit_error(
        IMPORT_V2_COMMIT_CONFLICT,
        "The Source resolution is stale or does not match this item.",
    )
}

fn artifact_record(path: &str, bytes: &[u8], kind: &str) -> SourceArtifactRecord {
    SourceArtifactRecord {
        path: path.replace('\\', "/"),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size_bytes: bytes.len() as u64,
        kind: kind.into(),
    }
}

fn canonical_candidate_locator(
    kind: &ImportInputKind,
    candidate: &crate::services::import_v2::source_finalization::CandidateMetadata,
) -> Option<String> {
    if kind != &ImportInputKind::Url {
        return None;
    }
    let platform = candidate.platform.as_deref()?;
    let platform_id = candidate.platform_content_id.as_deref()?;
    if !platform
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        || !platform_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return None;
    }
    let canonical = url::Url::parse(candidate.canonical_url.as_deref()?).ok()?;
    canonical.host_str()?;
    Some(format!("platform:{platform}:{platform_id}"))
}

fn validate_complete_decision_set(
    session: &crate::models::import_v2::ImportSession,
    decisions: &[CommitItemDecision],
) -> Result<(), BackendError> {
    if decisions.is_empty() {
        return Err(commit_error(
            IMPORT_V2_STATE_INVALID,
            "At least one selected import item is required for commit.",
        ));
    }
    let mut item_ids = std::collections::HashSet::with_capacity(decisions.len());
    for decision in decisions {
        if !item_ids.insert(decision.item_id.as_str()) {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "Import commit decisions must be unique.",
            ));
        }
        let item = session
            .items
            .iter()
            .find(|item| item.item_id == decision.item_id)
            .ok_or_else(|| {
                commit_error(
                    IMPORT_V2_STATE_INVALID,
                    "Import item was not found for commit.",
                )
            })?;
        if !item.selected
            || !matches!(
                item.status,
                ImportItemStatus::PreviewReady | ImportItemStatus::NeedsMerge
            )
            || item.preview.is_none()
        {
            return Err(commit_error(
                IMPORT_V2_STATE_INVALID,
                "Import item is not ready for commit.",
            ));
        }
        if item
            .preview
            .as_ref()
            .is_some_and(|preview| preview.quality.level == QualityLevel::Fail)
        {
            return Err(commit_error(
                IMPORT_V2_QUALITY_FAILED,
                "Failed quality previews cannot be committed.",
            ));
        }
        if item.status == ImportItemStatus::NeedsMerge && decision.resolution.is_none() {
            return Err(commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "Merge conflicts require an explicit conflict action.",
            ));
        }
    }
    Ok(())
}

fn verified_artifact(
    root: &Path,
    relative: &str,
    expected_hash: &str,
) -> Result<Vec<u8>, BackendError> {
    if relative.trim().is_empty()
        || relative.contains('\\')
        || relative.contains(':')
        || Path::new(relative)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(staging_artifact_error());
    }
    let canonical_root = root.canonicalize().map_err(|_| staging_artifact_error())?;
    let mut candidate = root.to_path_buf();
    for component in Path::new(relative).components() {
        candidate.push(component.as_os_str());
        let metadata =
            std::fs::symlink_metadata(&candidate).map_err(|_| staging_artifact_error())?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(staging_artifact_error());
        }
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|_| staging_artifact_error())?;
    if !canonical.starts_with(&canonical_root) {
        return Err(staging_artifact_error());
    }
    let mut file = std::fs::File::open(&canonical).map_err(|_| staging_artifact_error())?;
    // Bind validation to this opened object. Later namespace replacement cannot
    // redirect reads through this handle, and its kernel-resolved final path must
    // still be the validated canonical artifact.
    if !opened_file_matches_path(&file, &canonical) {
        return Err(staging_artifact_error());
    }
    run_before_artifact_open_hook(&canonical);
    let before = file.metadata().map_err(|_| staging_artifact_error())?;
    if !before.is_file() || is_reparse_point(&before) {
        return Err(staging_artifact_error());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| staging_artifact_error())?;
    let after = file.metadata().map_err(|_| staging_artifact_error())?;
    if !after.is_file() || before.len() != after.len() || after.len() != bytes.len() as u64 {
        return Err(staging_artifact_error());
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if hash != expected_hash {
        return Err(staging_artifact_error());
    }
    Ok(bytes)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn opened_file_matches_path(file: &std::fs::File, canonical: &Path) -> bool {
    use std::os::unix::io::AsRawFd;
    std::fs::canonicalize(format!("/proc/self/fd/{}", file.as_raw_fd()))
        .is_ok_and(|path| path == canonical)
}

#[cfg(target_os = "macos")]
fn opened_file_matches_path(file: &std::fs::File, canonical: &Path) -> bool {
    use std::os::unix::io::AsRawFd;
    unsafe extern "C" {
        fn fcntl(fd: i32, command: i32, buffer: *mut std::ffi::c_void) -> i32;
    }
    const F_GETPATH: i32 = 50;
    let mut buffer = vec![0u8; 1024];
    // SAFETY: the descriptor is live and the buffer is writable for MAXPATHLEN bytes.
    if unsafe { fcntl(file.as_raw_fd(), F_GETPATH, buffer.as_mut_ptr().cast()) } == -1 {
        return false;
    }
    let length = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    std::str::from_utf8(&buffer[..length]).is_ok_and(|path| Path::new(path) == canonical)
}

#[cfg(windows)]
fn opened_file_matches_path(file: &std::fs::File, canonical: &Path) -> bool {
    use std::os::windows::io::AsRawHandle;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFinalPathNameByHandleW(
            handle: *mut std::ffi::c_void,
            path: *mut u16,
            size: u32,
            flags: u32,
        ) -> u32;
    }
    let mut buffer = vec![0u16; 32768];
    // SAFETY: the file owns a live handle and buffer is writable for its full declared size.
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle().cast(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if length == 0 || length as usize >= buffer.len() {
        return false;
    }
    let resolved = String::from_utf16_lossy(&buffer[..length as usize]);
    let resolved = if let Some(unc) = resolved.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        resolved
            .strip_prefix(r"\\?\")
            .unwrap_or(&resolved)
            .to_string()
    };
    let canonical_text = canonical.to_string_lossy();
    let canonical_text = canonical_text
        .strip_prefix(r"\\?\")
        .unwrap_or(&canonical_text);
    resolved
        .replace('/', "\\")
        .eq_ignore_ascii_case(&canonical_text.replace('/', "\\"))
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    windows
)))]
fn opened_file_matches_path(_file: &std::fs::File, _canonical: &Path) -> bool {
    false
}

fn staging_artifact_error() -> BackendError {
    commit_error(
        IMPORT_V2_COMMIT_FAILED,
        "A staged import artifact is unsafe, missing, or changed.",
    )
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn json_bytes(value: &impl serde::Serialize) -> Result<Vec<u8>, BackendError> {
    serde_json::to_vec_pretty(value).map_err(|_| {
        commit_error(
            IMPORT_V2_COMMIT_FAILED,
            "Import metadata could not be serialized.",
        )
    })
}

fn commit_error(code: &str, message: &str) -> BackendError {
    BackendError::new(code, message, true, true)
}

fn persist_history_checked(
    context: &ProjectContext,
    relative: &str,
    batch: &ImportBatchResult,
    expected_hash: &str,
) -> Result<(), BackendError> {
    let mut transaction = FileTransaction::new_for_project(&context.root);
    transaction.write_if_hash_matches(
        &context.resolve_project_path(relative)?,
        &json_bytes(batch)?,
        expected_hash,
    )?;
    transaction.commit()
}

fn asset_collision_key(path: &str) -> String {
    path.to_lowercase()
}

fn canonical_platform_locator(kind: &ImportInputKind, markdown: &[u8]) -> Option<String> {
    if kind != &ImportInputKind::Url {
        return None;
    }
    let content = std::str::from_utf8(markdown).ok()?;
    let split = split_frontmatter(content);
    let frontmatter = parse_frontmatter(split.frontmatter.as_deref()?);
    if frontmatter.get_scalar("source_platform")?.to_lowercase() != "xiaohongshu" {
        return None;
    }
    let source_id = frontmatter.get_scalar("source_id")?;
    if source_id.is_empty()
        || !source_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        return None;
    }
    let source_url = url::Url::parse(&frontmatter.get_scalar("source_url")?).ok()?;
    let host = source_url.host_str()?.to_ascii_lowercase();
    if host != "xiaohongshu.com" && !host.ends_with(".xiaohongshu.com") {
        return None;
    }
    if !source_url
        .path_segments()?
        .any(|segment| segment == source_id)
    {
        return None;
    }
    Some(format!("platform:xiaohongshu:{source_id}"))
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::errors::{BackendError, IMPORT_V2_COMMIT_CONFLICT, IMPORT_V2_COMMIT_FAILED};
    use crate::models::import_v2::{
        ArtifactKind, CommitImportSessionRequest, CommitItemDecision, ImportArtifact,
        ImportBatchResult, ImportInput, ImportInputKind, ImportItemCommitResult,
        ImportItemResolution, ImportItemStatus, ImportRecoveryAction, ImportResourceMode,
        ImportStage, MediaSaveMode,
    };
    use crate::models::paths::ProjectContext;
    use crate::models::task::TaskType;
    use crate::services::import_v2::engine::{
        EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
    };
    use crate::services::import_v2::source_registry::{
        SourceIndex, SourceManifest, SourceRegistry,
    };
    use crate::services::{FileStore, GitService};
    use crate::tasks::task_model::CancellationToken;
    use crate::tasks::TaskService;

    use super::super::ImportV2Service;
    use super::{
        asset_collision_key, canonical_platform_locator, content_identity_hash,
        planned_new_source_wiki_path, prepare_source_snapshot, refresh_new_source_wiki_targets,
        set_before_artifact_open_hook, set_before_exact_duplicate_commit_hook,
        set_before_failed_history_write_hook, set_commit_durable_targets_hook,
        set_commit_persistence_hook, verified_artifact, CommitPersistenceBoundary,
        CommitPersistenceTarget,
    };
    use super::{validate_source_package_update, SourcePackageManifest, SourcePackageMemberRole};
    use crate::services::import_v2::transaction::set_before_checked_displace_hook;
    use crate::services::import_v2::transaction::{
        set_before_new_install_hook, set_fail_next_candidate_install,
    };

    #[test]
    fn canonical_xiaohongshu_locator_uses_verified_note_identity() {
        let markdown = br#"---
source_platform: "xiaohongshu"
source_id: "note-123"
source_url: "https://www.xiaohongshu.com/explore/note-123?xsec_token=REDACTED"
---
# Note
"#;
        assert_eq!(
            canonical_platform_locator(&ImportInputKind::Url, markdown).as_deref(),
            Some("platform:xiaohongshu:note-123")
        );
        assert_eq!(
            canonical_platform_locator(&ImportInputKind::File, markdown),
            None
        );
    }

    #[test]
    fn canonical_xiaohongshu_locator_rejects_unverified_or_mismatched_urls() {
        let short_link = br#"---
source_platform: xiaohongshu
source_id: note-123
source_url: https://xhslink.com/a1b2
---
"#;
        let mismatched = br#"---
source_platform: xiaohongshu
source_id: note-123
source_url: https://www.xiaohongshu.com/explore/other-note
---
"#;
        assert_eq!(
            canonical_platform_locator(&ImportInputKind::Url, short_link),
            None
        );
        assert_eq!(
            canonical_platform_locator(&ImportInputKind::Url, mismatched),
            None
        );
    }

    #[test]
    fn url_content_identity_ignores_volatile_snapshots_but_tracks_extracted_evidence() {
        let subtitle = ImportArtifact {
            kind: ArtifactKind::Subtitle,
            relative_path: "subtitles/captions.vtt".into(),
            sha256: "subtitle-a".into(),
            size_bytes: 10,
        };
        let first = content_identity_hash(
            &ImportInputKind::Url,
            &MediaSaveMode::ExtractOnly,
            "snapshot-a",
            "markdown-a",
            std::slice::from_ref(&subtitle),
        );
        let same_content = content_identity_hash(
            &ImportInputKind::Url,
            &MediaSaveMode::ExtractOnly,
            "snapshot-b",
            "markdown-a",
            std::slice::from_ref(&subtitle),
        );
        let changed_content = content_identity_hash(
            &ImportInputKind::Url,
            &MediaSaveMode::ExtractOnly,
            "snapshot-b",
            "markdown-b",
            &[subtitle],
        );
        assert_eq!(first, same_content);
        assert_ne!(first, changed_content);
    }

    struct FixtureEngine {
        root: PathBuf,
    }
    impl ImportEngine for FixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "commit-fixture".into(),
                engine_version: "1".into(),
                route: "pdf.text".into(),
            }
        }
        fn supports(&self, _: &ImportInput) -> bool {
            true
        }
        fn execute(
            &self,
            request: &EngineRequest,
            _: &CancellationToken,
        ) -> Result<EngineResult, crate::errors::BackendError> {
            let root = self.root.join(
                request
                    .staging_root
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            std::fs::create_dir_all(root.join("assets")).unwrap();
            std::fs::write(
                root.join("source.bin"),
                request.input.display_name.as_bytes(),
            )
            .unwrap();
            std::fs::write(
                root.join("candidate.md"),
                format!("# {}", request.input.display_name),
            )
            .unwrap();
            std::fs::write(root.join("assets/asset.png"), b"png").unwrap();
            Ok(EngineResult {
                source_snapshot_path: "source.bin".into(),
                markdown_path: "candidate.md".into(),
                asset_paths: vec!["assets/asset.png".into()],
                metadata_path: None,
                title: request.input.display_name.clone(),
                text_coverage: Some(1.0),
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

    struct CommitFixture {
        root: PathBuf,
        context: ProjectContext,
        files: FileStore,
        git: GitService,
        service: ImportV2Service,
        tasks: TaskService,
        session_id: String,
        first_item_id: String,
        second_item_id: Option<String>,
    }

    impl CommitFixture {
        fn two_ready_items() -> Self {
            let root =
                std::env::temp_dir().join(format!("import-v2-commit-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&root).unwrap();
            let context = ProjectContext::new("project", root.clone());
            let files = FileStore;
            let git = GitService;
            let service = ImportV2Service::default();
            service
                .register_engine(Arc::new(FixtureEngine { root: root.clone() }))
                .unwrap();
            let tasks = TaskService::default();
            let session = service
                .create_session(&context, &files, ImportResourceMode::Balanced)
                .unwrap();
            let inputs = ["first.pdf", "second.pdf"]
                .into_iter()
                .map(|name| {
                    let path = root.join(name);
                    std::fs::write(&path, b"%PDF-1.4\n% commit fixture\n%%EOF\n").unwrap();
                    let locator = path.to_string_lossy().replace('\\', "/");
                    ImportInput {
                        source_identity: None,
                        kind: ImportInputKind::File,
                        display_name: name.into(),
                        locator: locator.clone(),
                        normalized_locator: Some(format!("file:d:/{name}")),
                        media_save_mode: Default::default(),
                    }
                })
                .collect::<Vec<_>>();
            let session = service
                .add_inputs(&context, &files, &session.session_id, inputs)
                .unwrap();
            for item in &session.items {
                let task = tasks
                    .create_project_task(
                        TaskType::Import,
                        context.project_id.clone(),
                        root.clone(),
                        "preview".into(),
                        true,
                    )
                    .unwrap();
                service
                    .run_item(
                        &context,
                        &files,
                        &tasks,
                        &session.session_id,
                        &item.item_id,
                        &task.id,
                    )
                    .unwrap();
            }
            Self {
                root,
                context,
                files,
                git,
                service,
                tasks,
                session_id: session.session_id,
                first_item_id: session.items[0].item_id.clone(),
                second_item_id: Some(session.items[1].item_id.clone()),
            }
        }
        fn updated_source() -> Self {
            let mut fixture = Self::two_ready_items();
            fixture
                .git
                .initialize_repository(&fixture.context, "initial")
                .unwrap();
            fixture.commit_with(None);
            let second_item_id = fixture.second_item_id.take().unwrap();
            fixture
                .service
                .skip_item(
                    &fixture.context,
                    &fixture.files,
                    &fixture.tasks,
                    &fixture.session_id,
                    &second_item_id,
                )
                .unwrap();
            let session = fixture
                .service
                .create_session(
                    &fixture.context,
                    &fixture.files,
                    ImportResourceMode::Balanced,
                )
                .unwrap();
            let input = ImportInput {
                source_identity: None,
                kind: ImportInputKind::File,
                display_name: "updated.pdf".into(),
                locator: fixture
                    .root
                    .join("first.pdf")
                    .to_string_lossy()
                    .into_owned(),
                normalized_locator: Some("file:d:/first.pdf".into()),
                media_save_mode: Default::default(),
            };
            let session = fixture
                .service
                .add_inputs(
                    &fixture.context,
                    &fixture.files,
                    &session.session_id,
                    vec![input],
                )
                .unwrap();
            let task = fixture
                .tasks
                .create_project_task(
                    TaskType::Import,
                    fixture.context.project_id.clone(),
                    fixture.root.clone(),
                    "updated".into(),
                    true,
                )
                .unwrap();
            fixture
                .service
                .run_item(
                    &fixture.context,
                    &fixture.files,
                    &fixture.tasks,
                    &session.session_id,
                    &session.items[0].item_id,
                    &task.id,
                )
                .unwrap();
            fixture.session_id = session.session_id;
            fixture.first_item_id = session.items[0].item_id.clone();
            fixture
        }
        fn ready_url_item() -> Self {
            let mut fixture = Self::two_ready_items();
            fixture.second_item_id = None;
            let mut session = fixture
                .service
                .sessions
                .load(&fixture.context, &fixture.files, &fixture.session_id)
                .unwrap();
            let item = session
                .items
                .iter_mut()
                .find(|item| item.item_id == fixture.first_item_id)
                .unwrap();
            item.input.kind = ImportInputKind::Url;
            item.input.display_name = "https://www.douyin.com/video/123".into();
            item.input.locator = "https://www.douyin.com/video/123?token=secret&keep=1".into();
            item.input.normalized_locator = Some(item.input.locator.clone());
            let html = br#"<!doctype html><html><body>platform evidence<script>window.data={"access_token":"snapshot-secret"}</script><a href="https://cdn.example/video?id=42&future-signature=url-secret">media</a></body></html>"#;
            let source_path = fixture.root.join(format!(
                ".app/import-sessions/{}/items/{}/staging/source.bin",
                fixture.session_id, fixture.first_item_id
            ));
            std::fs::write(&source_path, html).unwrap();
            let preview = item.preview.as_mut().unwrap();
            preview.title = "Platform title".into();
            preview.source_snapshot.sha256 = format!("{:x}", Sha256::digest(html));
            preview.source_snapshot.size_bytes = html.len() as u64;
            let api = br#"{"data":{"title":"Platform title","authorization":"Bearer api-secret","expires":123}}"#;
            let evidence_path = source_path
                .parent()
                .unwrap()
                .join("source-evidence/bilibili-api.json");
            std::fs::create_dir_all(evidence_path.parent().unwrap()).unwrap();
            std::fs::write(&evidence_path, api).unwrap();
            preview.assets.push(ImportArtifact {
                kind: ArtifactKind::SourceEvidence,
                relative_path: "source-evidence/bilibili-api.json".into(),
                sha256: format!("{:x}", Sha256::digest(api)),
                size_bytes: api.len() as u64,
            });
            fixture
                .service
                .sessions
                .save(&fixture.context, &fixture.files, &session)
                .unwrap();
            fixture
        }
        fn request(&self, decisions: Vec<CommitItemDecision>) -> CommitImportSessionRequest {
            CommitImportSessionRequest {
                project_id: self.context.project_id.clone(),
                project_root_path: self.root.to_string_lossy().into(),
                session_id: self.session_id.clone(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                decisions,
            }
        }
        fn commit_all(&self) -> crate::models::import_v2::ImportBatchResult {
            let session = self
                .service
                .sessions
                .load(&self.context, &self.files, &self.session_id)
                .unwrap();
            let decisions = [
                Some(self.first_item_id.clone()),
                self.second_item_id.clone(),
            ]
            .into_iter()
            .flatten()
            .map(|item_id| {
                let resolution = session
                    .items
                    .iter()
                    .find(|item| item.item_id == item_id)
                    .and_then(|item| item.preview.as_ref())
                    .and_then(|preview| preview.resolution.as_ref())
                    .and_then(|context| context.default_resolution.clone());
                CommitItemDecision {
                    item_id,
                    resolution,
                }
            })
            .collect();
            self.service
                .commit_items(
                    &self.context,
                    &self.files,
                    &self.git,
                    &self.request(decisions),
                )
                .unwrap()
        }
        fn commit_with(
            &self,
            resolution: Option<ImportItemResolution>,
        ) -> crate::models::import_v2::ImportBatchResult {
            self.service
                .commit_items(
                    &self.context,
                    &self.files,
                    &self.git,
                    &self.request(vec![CommitItemDecision {
                        item_id: self.first_item_id.clone(),
                        resolution,
                    }]),
                )
                .unwrap()
        }
        fn bound_resolution(&self, kind: &str) -> ImportItemResolution {
            let session = self
                .service
                .sessions
                .load(&self.context, &self.files, &self.session_id)
                .unwrap();
            let item = session
                .items
                .iter()
                .find(|item| item.item_id == self.first_item_id)
                .unwrap();
            let preview = item.preview.as_ref().unwrap();
            let candidate_hash = content_identity_hash(
                &item.input.kind,
                &item.input.media_save_mode,
                &preview.source_snapshot.sha256,
                &preview.markdown.sha256,
                &preview.assets,
            );
            let index = SourceRegistry::read_index(&self.context, &self.files).unwrap();
            let locator = item.input.normalized_locator.as_deref().unwrap();
            let pointer = index.by_locator.get(locator).unwrap();
            let manifest = SourceRegistry::read_manifest(
                &self.context,
                &self.files,
                &format!(".app/sources/{}.json", pointer.source_id),
            )
            .unwrap();
            let current_hash = self
                .files
                .file_hash(&self.context, &manifest.wiki_path)
                .unwrap();
            let binding = (
                pointer.source_id.clone(),
                candidate_hash,
                current_hash,
                manifest.current_version_id,
            );
            match kind {
                "keep" => ImportItemResolution::KeepCurrentSource {
                    source_id: binding.0,
                    candidate_hash: binding.1,
                    current_hash: binding.2,
                    target_version_id: binding.3,
                },
                "apply" => ImportItemResolution::ApplyImportCandidate {
                    source_id: binding.0,
                    candidate_hash: binding.1,
                    current_hash: binding.2,
                    target_version_id: binding.3,
                },
                _ => unreachable!(),
            }
        }
        fn break_second_asset_after_preview(&self) {
            let id = self.second_item_id.as_ref().unwrap();
            std::fs::remove_file(self.root.join(format!(
                ".app/import-sessions/{}/items/{id}/staging/assets/asset.png",
                self.session_id
            )))
            .unwrap();
        }
        fn prepare_exact_duplicate(&mut self) -> ImportItemCommitResult {
            let second_item_id = self.second_item_id.take().unwrap();
            let first = self.commit_all().items[0].clone();
            self.service
                .skip_item(
                    &self.context,
                    &self.files,
                    &self.tasks,
                    &self.session_id,
                    &second_item_id,
                )
                .unwrap();
            let session = self
                .service
                .create_session(&self.context, &self.files, ImportResourceMode::Balanced)
                .unwrap();
            let input = ImportInput {
                source_identity: None,
                kind: ImportInputKind::File,
                display_name: "first.pdf".into(),
                locator: self.root.join("first.pdf").to_string_lossy().into_owned(),
                normalized_locator: Some("file:d:/alias/first.pdf".into()),
                media_save_mode: Default::default(),
            };
            let session = self
                .service
                .add_inputs(&self.context, &self.files, &session.session_id, vec![input])
                .unwrap();
            let task = self
                .tasks
                .create_project_task(
                    TaskType::Import,
                    self.context.project_id.clone(),
                    self.root.clone(),
                    "alias".into(),
                    true,
                )
                .unwrap();
            self.service
                .run_item(
                    &self.context,
                    &self.files,
                    &self.tasks,
                    &session.session_id,
                    &session.items[0].item_id,
                    &task.id,
                )
                .unwrap();
            self.session_id = session.session_id;
            self.first_item_id = session.items[0].item_id.clone();
            first
        }
        fn stage_first_as_index_package(&self) {
            let mut session = self
                .service
                .sessions
                .load(&self.context, &self.files, &self.session_id)
                .unwrap();
            let item = session
                .items
                .iter_mut()
                .find(|item| item.item_id == self.first_item_id)
                .unwrap();
            let preview = item.preview.as_mut().unwrap();
            let package = SourcePackageManifest::staging(vec![
                crate::models::source_package::SourcePackageMember {
                    order: 0,
                    role: SourcePackageMemberRole::Index,
                    title: preview.title.clone(),
                    staging_path: preview.markdown.relative_path.clone(),
                    wiki_path: String::new(),
                    baseline_path: String::new(),
                    content_hash: preview.markdown.sha256.clone(),
                    human_edit_hash: preview.markdown.sha256.clone(),
                },
            ]);
            let package_bytes = serde_json::to_vec_pretty(&package).unwrap();
            let staging = self.root.join(format!(
                ".app/import-sessions/{}/items/{}/staging",
                self.session_id, self.first_item_id
            ));
            std::fs::write(staging.join("source-package.json"), &package_bytes).unwrap();
            preview.assets.push(ImportArtifact {
                kind: ArtifactKind::Attachment,
                relative_path: "source-package.json".into(),
                sha256: format!("{:x}", Sha256::digest(&package_bytes)),
                size_bytes: package_bytes.len() as u64,
            });
            self.service
                .sessions
                .save(&self.context, &self.files, &session)
                .unwrap();
        }
        fn manifest(&self) -> SourceManifest {
            let index: SourceIndex = self
                .files
                .read_json(&self.context, ".app/source-index-v2.json")
                .unwrap();
            let pointer = index.by_locator.get("file:d:/first.pdf").unwrap();
            self.files
                .read_json(
                    &self.context,
                    &format!(".app/sources/{}.json", pointer.source_id),
                )
                .unwrap()
        }
    }
    impl Drop for CommitFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn failed_item_commit_rolls_back_only_that_item() {
        let fixture = CommitFixture::two_ready_items();
        fixture.break_second_asset_after_preview();
        let result = fixture.commit_all();
        assert_eq!((result.committed_count, result.failed_count), (1, 1));
        let completion = result.completion.as_ref().expect("completion");
        assert_eq!(completion.new_sources.len(), 1);
        assert_eq!(completion.failures.len(), 1);
        let source_id = result.items[0].source_id.as_deref().unwrap();
        let manifest: SourceManifest = fixture
            .files
            .read_json(&fixture.context, &format!(".app/sources/{source_id}.json"))
            .unwrap();
        assert!(fixture.root.join(manifest.wiki_path).is_file());
        let index: SourceIndex = fixture
            .files
            .read_json(&fixture.context, ".app/source-index-v2.json")
            .unwrap();
        assert!(!index.by_locator.contains_key("file:d:/second.pdf"));
        assert_eq!(
            std::fs::read_dir(fixture.root.join("raw/sources"))
                .unwrap()
                .count(),
            1,
            "failed item must not leave a raw source directory"
        );
        let history_path = std::fs::read_dir(fixture.root.join(".app/import-history"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let history: ImportBatchResult =
            serde_json::from_slice(&std::fs::read(history_path).unwrap()).unwrap();
        let snapshot = history.history_snapshot.expect("history snapshot");
        assert_eq!(snapshot.items.len(), 2);
        assert!(snapshot
            .items
            .iter()
            .any(|item| item.status == ImportItemStatus::Completed));
        assert!(snapshot
            .items
            .iter()
            .any(|item| item.status == ImportItemStatus::Failed));
    }

    #[test]
    fn successful_batch_history_snapshot_is_terminal_after_last_commit() {
        let fixture = CommitFixture::two_ready_items();
        let result = fixture.commit_all();
        assert_eq!((result.committed_count, result.failed_count), (2, 0));
        assert_eq!(
            result
                .completion
                .as_ref()
                .expect("completion")
                .new_sources
                .len(),
            2
        );
        let history_path = std::fs::read_dir(fixture.root.join(".app/import-history"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let history: ImportBatchResult =
            serde_json::from_slice(&std::fs::read(history_path).unwrap()).unwrap();
        assert_eq!(history.completion, result.completion);
        let snapshot = history.history_snapshot.expect("history snapshot");
        assert_eq!(
            snapshot.status,
            crate::models::import_v2::ImportSessionStatus::Completed
        );
        assert!(snapshot
            .items
            .iter()
            .all(|item| item.status == ImportItemStatus::Completed));
        assert!(!history.created_at.is_empty());
        let history_preview_dir = fixture
            .root
            .join(".app/import-history-previews")
            .join(&history.batch_id);
        assert_eq!(
            std::fs::read_dir(history_preview_dir).unwrap().count(),
            2,
            "completed batches keep immutable Markdown previews"
        );
    }

    #[test]
    fn empty_commit_decisions_are_rejected_before_history_creation() {
        let fixture = CommitFixture::two_ready_items();
        let error = fixture
            .service
            .commit_items(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.request(Vec::new()),
            )
            .unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_STATE_INVALID);
        assert!(!fixture.root.join(".app/import-history").exists());
    }

    #[test]
    fn cancellation_before_first_item_returns_typed_error_and_accounts_all_decisions() {
        let fixture = CommitFixture::two_ready_items();
        let request = fixture.request(vec![
            CommitItemDecision {
                item_id: fixture.first_item_id.clone(),
                resolution: None,
            },
            CommitItemDecision {
                item_id: fixture.second_item_id.clone().unwrap(),
                resolution: None,
            },
        ]);
        let error = fixture
            .service
            .commit_items_cancellable(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &request,
                || true,
            )
            .unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_CANCELLED);
        let history = std::fs::read_dir(fixture.root.join(".app/import-history"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let batch: crate::models::import_v2::ImportBatchResult =
            serde_json::from_slice(&std::fs::read(history).unwrap()).unwrap();
        assert_eq!(batch.items.len(), 2);
        assert!(batch
            .items
            .iter()
            .all(|item| item.error_code.as_deref() == Some(crate::errors::IMPORT_V2_CANCELLED)));
        assert_eq!((batch.committed_count, batch.failed_count), (0, 2));
    }

    #[test]
    fn cancellation_between_items_commits_first_and_accounts_remaining_decision() {
        let fixture = CommitFixture::two_ready_items();
        let request = fixture.request(vec![
            CommitItemDecision {
                item_id: fixture.first_item_id.clone(),
                resolution: None,
            },
            CommitItemDecision {
                item_id: fixture.second_item_id.clone().unwrap(),
                resolution: None,
            },
        ]);
        let checks = std::cell::Cell::new(0);
        let durable_progress = std::cell::RefCell::new(Vec::new());
        let error = fixture
            .service
            .commit_items_cancellable_with_progress(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &request,
                || {
                    let next = checks.get() + 1;
                    checks.set(next);
                    next > 1
                },
                |batch| {
                    durable_progress.borrow_mut().push((
                        batch.items.len(),
                        batch.committed_count,
                        batch.failed_count,
                    ));
                },
                None,
                || Ok(()),
            )
            .unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_CANCELLED);
        let history = std::fs::read_dir(fixture.root.join(".app/import-history"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let batch: crate::models::import_v2::ImportBatchResult =
            serde_json::from_slice(&std::fs::read(history).unwrap()).unwrap();
        assert_eq!(batch.items.len(), 2);
        assert!(batch.items[0].committed);
        assert_eq!(
            batch.items[1].error_code.as_deref(),
            Some(crate::errors::IMPORT_V2_CANCELLED)
        );
        assert_eq!((batch.committed_count, batch.failed_count), (1, 1));
        assert_eq!(durable_progress.into_inner(), vec![(1, 1, 0), (2, 1, 1)]);
    }

    #[test]
    fn create_new_derives_collision_free_cjk_portable_wiki_path() {
        let fixture = CommitFixture::two_ready_items();
        let original = fixture.root.join("wiki/sources/local/first.md");
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::write(&original, "unrelated existing Source").unwrap();
        let colliding = original.with_file_name(format!(
            "{}-2.md",
            original.file_stem().unwrap().to_string_lossy()
        ));
        std::fs::write(&colliding, "existing collision").unwrap();
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        refresh_new_source_wiki_targets(&fixture.context, &fixture.files, &mut session).unwrap();
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();
        let result = fixture.commit_with(None);
        assert!(
            result.items[0].committed,
            "collision-free commit failed: {:?}",
            result.items[0]
        );
        let created = result.items[0].wiki_path.as_deref().unwrap();
        assert!(
            created.ends_with("-3.md"),
            "unexpected derived path: {created}"
        );
        assert_eq!(
            std::fs::read_to_string(original).unwrap(),
            "unrelated existing Source"
        );
        assert_eq!(
            std::fs::read_to_string(colliding).unwrap(),
            "existing collision"
        );
        assert!(fixture.root.join(created).is_file());
    }

    #[test]
    fn preview_plan_reserves_same_session_names_in_commit_order() {
        let fixture = CommitFixture::two_ready_items();
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        session.items[1].input.display_name = "first.pdf".into();
        refresh_new_source_wiki_targets(&fixture.context, &fixture.files, &mut session).unwrap();
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();
        let session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        let second_item_id = fixture.second_item_id.as_deref().unwrap();
        let planned = planned_new_source_wiki_path(
            &fixture.context,
            &fixture.files,
            &session,
            second_item_id,
        )
        .unwrap()
        .unwrap();

        let batch = fixture.commit_all();
        let committed = batch
            .items
            .iter()
            .find(|item| item.item_id == second_item_id)
            .and_then(|item| item.wiki_path.as_deref())
            .unwrap();
        assert_eq!(planned, committed);
        assert!(planned.ends_with("first-2.md"), "{planned}");
    }

    #[test]
    fn preview_targets_replan_in_session_order_after_out_of_order_completion() {
        let fixture = CommitFixture::two_ready_items();
        let second_item_id = fixture.second_item_id.as_deref().unwrap();
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        session.items[1].input.display_name = "first.pdf".into();
        session.items[0].status = ImportItemStatus::Extracting;
        refresh_new_source_wiki_targets(&fixture.context, &fixture.files, &mut session).unwrap();
        assert_eq!(
            session.items[1]
                .preview
                .as_ref()
                .unwrap()
                .resolution
                .as_ref()
                .unwrap()
                .target_wiki_path
                .as_deref(),
            Some("wiki/sources/local/first.md")
        );

        session.items[0].status = ImportItemStatus::PreviewReady;
        refresh_new_source_wiki_targets(&fixture.context, &fixture.files, &mut session).unwrap();
        let first_target = session.items[0]
            .preview
            .as_ref()
            .unwrap()
            .resolution
            .as_ref()
            .unwrap()
            .target_wiki_path
            .clone();
        let second_target = session.items[1]
            .preview
            .as_ref()
            .unwrap()
            .resolution
            .as_ref()
            .unwrap()
            .target_wiki_path
            .clone();
        assert_eq!(first_target.as_deref(), Some("wiki/sources/local/first.md"));
        assert_eq!(
            second_target.as_deref(),
            Some("wiki/sources/local/first-2.md")
        );
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();

        let batch = fixture.commit_all();
        assert!(batch.items.iter().all(|item| item.committed));
        assert_eq!(
            batch
                .items
                .iter()
                .find(|item| item.item_id == second_item_id)
                .and_then(|item| item.wiki_path.as_deref()),
            second_target.as_deref()
        );
    }

    #[test]
    fn failed_earlier_collision_does_not_block_the_later_bound_target() {
        let fixture = CommitFixture::two_ready_items();
        let second_item_id = fixture.second_item_id.as_deref().unwrap();
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        session.items[1].input.display_name = "first.pdf".into();
        refresh_new_source_wiki_targets(&fixture.context, &fixture.files, &mut session).unwrap();
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();
        let first_asset = fixture.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging/assets/asset.png",
            fixture.session_id, fixture.first_item_id
        ));
        std::fs::remove_file(first_asset).unwrap();

        let batch = fixture.commit_all();

        assert!(!batch.items[0].committed);
        let second = batch
            .items
            .iter()
            .find(|item| item.item_id == second_item_id)
            .unwrap();
        assert!(second.committed);
        assert_eq!(
            second.wiki_path.as_deref(),
            Some("wiki/sources/local/first-2.md")
        );
    }

    #[test]
    fn deselecting_an_earlier_item_replans_the_remaining_preview_target() {
        let fixture = CommitFixture::two_ready_items();
        let second_item_id = fixture.second_item_id.as_deref().unwrap();
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        session.items[1].input.display_name = "first.pdf".into();
        refresh_new_source_wiki_targets(&fixture.context, &fixture.files, &mut session).unwrap();
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();

        fixture
            .service
            .set_item_selected(
                &fixture.context,
                &fixture.files,
                &fixture.session_id,
                &fixture.first_item_id,
                false,
            )
            .unwrap();
        let session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        assert_eq!(
            session.items[1]
                .preview
                .as_ref()
                .unwrap()
                .resolution
                .as_ref()
                .unwrap()
                .target_wiki_path
                .as_deref(),
            Some("wiki/sources/local/first.md")
        );

        let batch = fixture
            .service
            .commit_items(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.request(vec![CommitItemDecision {
                    item_id: second_item_id.into(),
                    resolution: None,
                }]),
            )
            .unwrap();
        assert!(batch.items[0].committed);
        assert_eq!(
            batch.items[0].wiki_path.as_deref(),
            Some("wiki/sources/local/first.md")
        );
    }

    #[test]
    fn preview_plan_uses_candidate_canonical_url_like_commit() {
        let mut fixture = CommitFixture::ready_url_item();
        fixture.second_item_id = None;
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        let item = session
            .items
            .iter_mut()
            .find(|item| item.item_id == fixture.first_item_id)
            .unwrap();
        item.input.display_name = "Redirect article.html".into();
        let metadata = br#"{"finalPublicUrl":"https://canonical.example/article"}"#;
        let staging = fixture.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging",
            fixture.session_id, fixture.first_item_id
        ));
        std::fs::write(staging.join("canonical.json"), metadata).unwrap();
        item.preview.as_mut().unwrap().assets.push(ImportArtifact {
            kind: ArtifactKind::Metadata,
            relative_path: "canonical.json".into(),
            sha256: format!("{:x}", Sha256::digest(metadata)),
            size_bytes: metadata.len() as u64,
        });
        refresh_new_source_wiki_targets(&fixture.context, &fixture.files, &mut session).unwrap();
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();
        let session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        let planned = planned_new_source_wiki_path(
            &fixture.context,
            &fixture.files,
            &session,
            &fixture.first_item_id,
        )
        .unwrap()
        .unwrap();

        let committed = fixture.commit_with(None).items[0]
            .wiki_path
            .clone()
            .unwrap();
        assert_eq!(planned, committed);
        assert!(planned.starts_with("wiki/sources/web/canonical.example/"));
    }

    #[test]
    fn preview_plan_uses_package_index_target_like_commit() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        fixture.stage_first_as_index_package();
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        refresh_new_source_wiki_targets(&fixture.context, &fixture.files, &mut session).unwrap();
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();
        let planned = planned_new_source_wiki_path(
            &fixture.context,
            &fixture.files,
            &session,
            &fixture.first_item_id,
        )
        .unwrap()
        .unwrap();

        let committed = fixture.commit_with(None).items[0]
            .wiki_path
            .clone()
            .unwrap();
        assert_eq!(planned, committed);
        assert!(planned.ends_with("/index.md"), "{planned}");
    }

    #[test]
    fn commit_rejects_a_new_source_target_that_changed_after_preview() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        let planned = planned_new_source_wiki_path(
            &fixture.context,
            &fixture.files,
            &session,
            &fixture.first_item_id,
        )
        .unwrap()
        .unwrap();
        session.items[0]
            .preview
            .as_mut()
            .unwrap()
            .resolution
            .as_mut()
            .unwrap()
            .target_wiki_path = Some(planned.clone());
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();
        let occupied = fixture.root.join(&planned);
        std::fs::create_dir_all(occupied.parent().unwrap()).unwrap();
        std::fs::write(&occupied, "external content").unwrap();

        let result = fixture.commit_with(None).items.remove(0);

        assert!(!result.committed);
        assert_eq!(
            result.error_code.as_deref(),
            Some(IMPORT_V2_COMMIT_CONFLICT)
        );
        assert_eq!(
            std::fs::read_to_string(occupied).unwrap(),
            "external content"
        );
    }

    #[test]
    fn loading_an_older_preview_binds_its_missing_target_before_commit() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        session.items[0]
            .preview
            .as_mut()
            .unwrap()
            .resolution
            .as_mut()
            .unwrap()
            .target_wiki_path = None;
        session.items[1].selected = false;
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();

        let recovered = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        let bound_target = recovered.items[0]
            .preview
            .as_ref()
            .unwrap()
            .resolution
            .as_ref()
            .unwrap()
            .target_wiki_path
            .clone()
            .expect("legacy preview target should be backfilled");
        let occupied = fixture.root.join(&bound_target);
        std::fs::create_dir_all(occupied.parent().unwrap()).unwrap();
        std::fs::write(&occupied, "external content after recovery").unwrap();

        let result = fixture.commit_with(None).items.remove(0);

        assert!(!result.committed);
        assert_eq!(
            result.error_code.as_deref(),
            Some(IMPORT_V2_COMMIT_CONFLICT)
        );
        assert_eq!(
            std::fs::read_to_string(occupied).unwrap(),
            "external content after recovery"
        );
    }

    #[test]
    fn invalid_later_decision_cannot_commit_an_earlier_item_or_create_history() {
        let fixture = CommitFixture::two_ready_items();
        let history_dir = fixture.root.join(".app/import-history");
        let request = fixture.request(vec![
            CommitItemDecision {
                item_id: fixture.first_item_id.clone(),
                resolution: None,
            },
            CommitItemDecision {
                item_id: "missing-item".into(),
                resolution: None,
            },
        ]);

        let error = fixture
            .service
            .commit_items(&fixture.context, &fixture.files, &fixture.git, &request)
            .unwrap_err();

        assert_eq!(error.code, crate::errors::IMPORT_V2_STATE_INVALID);
        assert!(!history_dir.exists());
        assert!(!fixture.root.join("raw/sources").exists());
        assert!(!fixture.root.join("wiki").exists());
    }

    #[test]
    fn source_package_disk_write_failure_rolls_back_the_whole_source_id() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        let item = session
            .items
            .iter_mut()
            .find(|item| item.item_id == fixture.first_item_id)
            .unwrap();
        let preview = item.preview.as_mut().unwrap();
        let staging = fixture.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging",
            fixture.session_id, fixture.first_item_id
        ));
        let child_relative = "package/pages/disk-fixture.md";
        let child = b"# Disk fixture\n\nThe package must roll back atomically.\n";
        std::fs::create_dir_all(staging.join("package/pages")).unwrap();
        std::fs::write(staging.join(child_relative), child).unwrap();
        let child_hash = format!("{:x}", Sha256::digest(child));
        let package = SourcePackageManifest::staging(vec![
            crate::models::source_package::SourcePackageMember {
                order: 0,
                role: SourcePackageMemberRole::Index,
                title: preview.title.clone(),
                staging_path: preview.markdown.relative_path.clone(),
                wiki_path: String::new(),
                baseline_path: String::new(),
                content_hash: preview.markdown.sha256.clone(),
                human_edit_hash: preview.markdown.sha256.clone(),
            },
            crate::models::source_package::SourcePackageMember {
                order: 1,
                role: SourcePackageMemberRole::Sheet,
                title: "Disk fixture".into(),
                staging_path: child_relative.into(),
                wiki_path: String::new(),
                baseline_path: String::new(),
                content_hash: child_hash.clone(),
                human_edit_hash: child_hash.clone(),
            },
        ]);
        let package_bytes = serde_json::to_vec_pretty(&package).unwrap();
        std::fs::write(staging.join("source-package.json"), &package_bytes).unwrap();
        preview.assets.extend([
            ImportArtifact {
                kind: ArtifactKind::Attachment,
                relative_path: child_relative.into(),
                sha256: child_hash,
                size_bytes: child.len() as u64,
            },
            ImportArtifact {
                kind: ArtifactKind::Attachment,
                relative_path: "source-package.json".into(),
                sha256: format!("{:x}", Sha256::digest(&package_bytes)),
                size_bytes: package_bytes.len() as u64,
            },
        ]);
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();

        set_before_new_install_hook(|path| {
            let is_package_page = path
                .file_name()
                .is_some_and(|name| name == "disk-fixture.md");
            if is_package_page {
                set_fail_next_candidate_install();
            }
            is_package_page
        });
        let batch = fixture.commit_with(None);
        assert_eq!(batch.committed_count, 0);
        assert_eq!(batch.failed_count, 1);
        assert!(!fixture.root.join("raw").exists());
        assert!(!fixture.root.join("wiki").exists());
        assert!(!fixture.root.join(".app/sources").exists());
        assert!(!fixture.root.join(".app/source-index-v2.json").exists());
        assert_no_transaction_orphans(&fixture.root);
        std::fs::remove_dir_all(&fixture.root).unwrap();
    }

    #[test]
    fn source_package_updates_fail_closed_on_child_edits_but_allow_shape_changes() {
        let (context, root) = super::super::test_support::test_context("package-update");
        let files = FileStore;
        let hash = format!("{:x}", Sha256::digest(b"# child\n"));
        let member = |order, role, staging: &str, wiki: &str| {
            crate::models::source_package::SourcePackageMember {
                order,
                role,
                title: format!("member-{order}"),
                staging_path: staging.into(),
                wiki_path: wiki.into(),
                baseline_path: format!(".app/source-artifacts/source/version/package/{order}.md"),
                content_hash: hash.clone(),
                human_edit_hash: hash.clone(),
            }
        };
        let previous = SourcePackageManifest {
            schema_version: crate::models::source_package::SOURCE_PACKAGE_SCHEMA_VERSION,
            source_id: "source".into(),
            version_id: "version".into(),
            entry_wiki_path: "wiki/sources/local/book/index.md".into(),
            members: vec![
                member(
                    0,
                    SourcePackageMemberRole::Index,
                    "document.md",
                    "wiki/sources/local/book/index.md",
                ),
                member(
                    1,
                    SourcePackageMemberRole::Sheet,
                    "package/pages/sheet.md",
                    "wiki/sources/local/book/sheet.md",
                ),
            ],
        };
        let mut next = previous.clone();
        next.version_id = "next".into();
        std::fs::create_dir_all(root.join("wiki/sources/local/book")).unwrap();
        std::fs::write(
            root.join("wiki/sources/local/book/sheet.md"),
            b"# human edit\n",
        )
        .unwrap();
        assert_eq!(
            validate_source_package_update(&context, &files, &previous, &next)
                .unwrap_err()
                .code,
            IMPORT_V2_COMMIT_CONFLICT
        );

        std::fs::write(root.join("wiki/sources/local/book/sheet.md"), b"# child\n").unwrap();
        next.members.pop();
        validate_source_package_update(&context, &files, &previous, &next).unwrap();

        let mut added = previous.clone();
        added.members.push(member(
            2,
            SourcePackageMemberRole::Sheet,
            "package/pages/new.md",
            "wiki/sources/local/book/new.md",
        ));
        validate_source_package_update(&context, &files, &previous, &added).unwrap();
        std::fs::write(
            root.join("wiki/sources/local/book/new.md"),
            b"# unrelated\n",
        )
        .unwrap();
        assert_eq!(
            validate_source_package_update(&context, &files, &previous, &added)
                .unwrap_err()
                .code,
            IMPORT_V2_COMMIT_CONFLICT
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn forged_engine_owned_ids_fail_without_source_package_residue() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        let item = session
            .items
            .iter_mut()
            .find(|item| item.item_id == fixture.first_item_id)
            .unwrap();
        let preview = item.preview.as_mut().unwrap();
        let forged = b"---\nsourceId: \"engine-forged\"\n---\n\n# Candidate\n";
        let candidate_path = fixture.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging/{}",
            fixture.session_id, fixture.first_item_id, preview.markdown.relative_path
        ));
        std::fs::write(candidate_path, forged).unwrap();
        preview.markdown.sha256 = format!("{:x}", Sha256::digest(forged));
        preview.markdown.size_bytes = forged.len() as u64;
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();

        let result = fixture.commit_all();

        assert_eq!(result.committed_count, 0, "{result:?}");
        assert_eq!(result.failed_count, 1, "{result:?}");
        assert_eq!(
            result.items[0].error_code.as_deref(),
            Some(crate::errors::IMPORT_V2_COMMIT_FAILED)
        );
        assert!(!fixture.root.join("raw").exists());
        assert!(!fixture.root.join("wiki").exists());
        assert!(!fixture.root.join(".app/sources").exists());
        assert!(!fixture.root.join(".app/source-index-v2.json").exists());
    }

    #[test]
    fn duplicate_decisions_are_rejected_before_history_or_item_mutation() {
        let fixture = CommitFixture::two_ready_items();
        let decision = CommitItemDecision {
            item_id: fixture.first_item_id.clone(),
            resolution: None,
        };
        let request = fixture.request(vec![decision.clone(), decision]);

        let error = fixture
            .service
            .commit_items(&fixture.context, &fixture.files, &fixture.git, &request)
            .unwrap_err();

        assert_eq!(error.code, crate::errors::IMPORT_V2_STATE_INVALID);
        assert!(!fixture.root.join(".app/import-history").exists());
        assert!(!fixture.root.join("raw/sources").exists());
    }

    #[test]
    fn wiki_hash_drift_blocks_update_before_any_write() {
        let fixture = CommitFixture::updated_source();
        let manifest = fixture.manifest();
        std::fs::write(fixture.root.join(&manifest.wiki_path), "external edit").unwrap();
        let before_versions = manifest.versions.len();
        let resolution = fixture.bound_resolution("apply");
        let resolution = match resolution {
            ImportItemResolution::ApplyImportCandidate {
                source_id,
                candidate_hash,
                target_version_id,
                ..
            } => ImportItemResolution::ApplyImportCandidate {
                source_id,
                candidate_hash,
                current_hash: "stale-hash".into(),
                target_version_id,
            },
            _ => unreachable!(),
        };
        let result = fixture.commit_with(Some(resolution));
        assert_eq!(
            result.items[0].error_code.as_deref(),
            Some(IMPORT_V2_COMMIT_CONFLICT)
        );
        assert_eq!(
            std::fs::read_to_string(fixture.root.join(&manifest.wiki_path)).unwrap(),
            "external edit"
        );
        assert_eq!(fixture.manifest().versions.len(), before_versions);
    }

    #[test]
    fn keep_current_source_records_candidate_version_without_advancing_current() {
        let fixture = CommitFixture::updated_source();
        let before = fixture.manifest();
        let wiki_path = fixture.root.join(&before.wiki_path);
        std::fs::write(&wiki_path, "# User-edited Source\n\nKeep this note.\n").unwrap();
        let resolution = fixture.bound_resolution("keep");
        let stored = fixture
            .service
            .set_item_resolution(
                &fixture.context,
                &fixture.files,
                &fixture.session_id,
                &fixture.first_item_id,
                resolution.clone(),
            )
            .unwrap();
        assert_eq!(
            stored
                .preview
                .as_ref()
                .and_then(|preview| preview.resolution.as_ref())
                .and_then(|resolution| resolution.default_resolution.as_ref()),
            Some(&resolution)
        );

        let result = fixture.commit_with(Some(resolution));

        assert_eq!(result.committed_count, 1, "{result:?}");
        let after = fixture.manifest();
        assert_eq!(after.versions.len(), before.versions.len() + 1);
        assert_eq!(after.current_version_id, before.current_version_id);
        assert_eq!(
            std::fs::read_to_string(wiki_path).unwrap(),
            "# User-edited Source\n\nKeep this note.\n"
        );
        let index = SourceRegistry::read_index(&fixture.context, &fixture.files).unwrap();
        assert_eq!(
            index.by_locator["file:d:/first.pdf"].version_id,
            before.current_version_id
        );
    }

    #[test]
    fn manual_merge_commits_the_staged_three_way_result_with_checkpoint() {
        let fixture = CommitFixture::updated_source();
        let before = fixture.manifest();
        let wiki_path = fixture.root.join(&before.wiki_path);
        std::fs::write(
            &wiki_path,
            include_str!("../../../../tests/fixtures/import-v2/merge/current-edited.md"),
        )
        .unwrap();

        let context = fixture
            .service
            .get_three_way_merge_context(
                &fixture.context,
                &fixture.files,
                &fixture.session_id,
                &fixture.first_item_id,
            )
            .unwrap();
        assert_eq!(
            context.resolution.kind,
            crate::models::import_v2::ImportResolutionKind::NeedsThreeWayMerge
        );
        assert!(context.baseline_markdown.contains("first.pdf"));
        assert_eq!(
            context.current_markdown,
            include_str!("../../../../tests/fixtures/import-v2/merge/current-edited.md")
        );
        assert!(context.candidate_markdown.contains("updated.pdf"));

        let merged = include_str!("../../../../tests/fixtures/import-v2/merge/merged.md");
        let staged = fixture
            .service
            .stage_manual_merge(
                &fixture.context,
                &fixture.files,
                &fixture.session_id,
                &fixture.first_item_id,
                merged,
            )
            .unwrap();
        let merged_hash = staged
            .preview
            .as_ref()
            .and_then(|preview| preview.manual_merge.as_ref())
            .unwrap()
            .sha256
            .clone();
        let binding = context.resolution.binding.unwrap();
        let resolution = ImportItemResolution::ManualMerge {
            source_id: binding.source_id,
            candidate_hash: binding.candidate_hash,
            current_hash: binding.current_hash,
            target_version_id: binding.target_version_id,
            merged_hash,
        };
        assert_eq!(
            staged
                .preview
                .as_ref()
                .and_then(|preview| preview.resolution.as_ref())
                .and_then(|resolution| resolution.default_resolution.as_ref()),
            Some(&resolution)
        );
        let result = fixture.commit_with(Some(resolution));

        assert_eq!(result.committed_count, 1, "{result:?}");
        let after = fixture.manifest();
        assert_ne!(after.current_version_id, before.current_version_id);
        let current = after
            .versions
            .iter()
            .find(|version| version.version_id == after.current_version_id)
            .unwrap();
        assert!(current.checkpoint.is_some());
        let final_source = std::fs::read_to_string(wiki_path).unwrap();
        assert!(final_source.contains("这一段来自当前 Source 的人工编辑"));
        assert!(!final_source.contains("# updated.pdf"));
        assert!(current
            .raw_evidence
            .iter()
            .any(|artifact| artifact.path.ends_with("/derived/import-candidate.md")));
    }

    #[test]
    fn exact_duplicate_records_alias_without_copying_raw_bytes() {
        let mut fixture = CommitFixture::two_ready_items();
        let first = fixture.prepare_exact_duplicate();
        let duplicate_batch = fixture
            .service
            .finalize_exact_duplicate(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.session_id,
                &fixture.first_item_id,
                true,
                || Ok(()),
            )
            .unwrap()
            .unwrap();
        let alias = duplicate_batch.items[0].clone();
        assert_eq!(alias.source_id, first.source_id);
        assert_eq!(alias.version_id, first.version_id);
        assert_eq!(
            alias.disposition,
            Some(crate::models::import_v2::ImportCommitDisposition::DuplicateSkipped)
        );
        let manifest = fixture.manifest();
        assert_eq!(manifest.versions.len(), 1);
        assert!(manifest.origins.contains(&"file:d:/alias/first.pdf".into()));
        let completed = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        assert_eq!(
            completed.status,
            crate::models::import_v2::ImportSessionStatus::Completed
        );
        assert_eq!(completed.items[0].status, ImportItemStatus::Completed);
        let next = fixture
            .service
            .create_session(
                &fixture.context,
                &fixture.files,
                ImportResourceMode::Balanced,
            )
            .unwrap();
        assert_ne!(next.session_id, completed.session_id);
    }

    #[test]
    fn exact_duplicate_commit_failure_is_returned_and_persisted_as_retryable() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.prepare_exact_duplicate();
        let asset = fixture.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging/assets/asset.png",
            fixture.session_id, fixture.first_item_id
        ));
        std::fs::remove_file(asset).unwrap();

        let error = fixture
            .service
            .finalize_exact_duplicate(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.session_id,
                &fixture.first_item_id,
                true,
                || Ok(()),
            )
            .unwrap_err();

        assert_eq!(error.code, IMPORT_V2_COMMIT_FAILED);
        let failed = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        assert_eq!(failed.items[0].status, ImportItemStatus::Failed);
        let issue = failed.items[0].issue.as_ref().expect("commit issue");
        assert_eq!(issue.stage, ImportStage::Commit);
        assert!(issue.retryable);
        assert!(issue
            .recovery_actions
            .contains(&ImportRecoveryAction::Retry));
    }

    #[test]
    fn exact_duplicate_batch_setup_failure_is_persisted_as_retryable() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.prepare_exact_duplicate();
        set_before_new_install_hook(|path| {
            let is_initial_history = path
                .parent()
                .and_then(|parent| parent.file_name())
                .is_some_and(|name| name == "import-history");
            if is_initial_history {
                set_fail_next_candidate_install();
            }
            is_initial_history
        });

        let error = fixture
            .service
            .finalize_exact_duplicate(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.session_id,
                &fixture.first_item_id,
                true,
                || Ok(()),
            )
            .unwrap_err();

        assert_eq!(error.code, "FILE_WRITE_FAILED");
        let failed = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        assert_eq!(failed.items[0].status, ImportItemStatus::Failed);
        assert!(failed.items[0].issue.as_ref().is_some_and(|issue| {
            issue.stage == ImportStage::Commit
                && issue.retryable
                && issue
                    .recovery_actions
                    .contains(&ImportRecoveryAction::Retry)
        }));
    }

    #[test]
    fn exact_duplicate_cancellation_does_not_relabel_the_item_as_failed() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.prepare_exact_duplicate();

        let error = fixture
            .service
            .finalize_exact_duplicate_cancellable(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.session_id,
                &fixture.first_item_id,
                true,
                || true,
                || Ok(()),
            )
            .unwrap_err();

        assert_eq!(error.code, crate::errors::IMPORT_V2_CANCELLED);
        let reopened = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        assert_eq!(reopened.items[0].status, ImportItemStatus::PreviewReady);
        assert!(reopened.items[0].issue.is_none());
    }

    #[test]
    fn exact_duplicate_task_transition_failure_is_persisted_as_retryable() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.prepare_exact_duplicate();

        let error = fixture
            .service
            .finalize_exact_duplicate(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.session_id,
                &fixture.first_item_id,
                true,
                || {
                    Err(BackendError::new(
                        "IMPORT_V2_TASK_FAILED",
                        "The preview task was cancelled before commit.",
                        true,
                        false,
                    ))
                },
            )
            .unwrap_err();

        assert_eq!(error.code, "IMPORT_V2_TASK_FAILED");
        let failed = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        assert_eq!(failed.items[0].status, ImportItemStatus::Failed);
        assert!(failed.items[0].issue.as_ref().is_some_and(|issue| {
            issue.stage == ImportStage::Commit
                && issue.retryable
                && issue
                    .recovery_actions
                    .contains(&ImportRecoveryAction::Retry)
        }));
    }

    #[test]
    fn restricted_exact_duplicate_waits_for_project_acknowledgement() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.prepare_exact_duplicate();
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        session.items[0].restricted_content = true;
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();

        let started = std::cell::Cell::new(false);
        let deferred = fixture
            .service
            .finalize_exact_duplicate(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.session_id,
                &fixture.first_item_id,
                false,
                || {
                    started.set(true);
                    Ok(())
                },
            )
            .unwrap();
        assert!(deferred.is_none());
        assert!(!started.get());
        let waiting = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        assert_eq!(waiting.items[0].status, ImportItemStatus::PreviewReady);

        let committed = fixture
            .service
            .finalize_exact_duplicate(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.session_id,
                &fixture.first_item_id,
                true,
                || Ok(()),
            )
            .unwrap()
            .expect("acknowledged duplicate should finalize");
        assert_eq!(committed.committed_count, 1);
    }

    #[test]
    fn stale_exact_duplicate_finalizer_does_not_overwrite_a_competing_completion() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.prepare_exact_duplicate();
        let history_root = fixture.root.join(".app/import-history");
        let history_count_before = std::fs::read_dir(&history_root)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or_default();
        let context = fixture.context.clone();
        let session_id = fixture.session_id.clone();
        let item_id = fixture.first_item_id.clone();
        set_before_exact_duplicate_commit_hook(move || {
            let service = ImportV2Service::default();
            let files = FileStore;
            let mut session = service
                .sessions
                .load(&context, &files, &session_id)
                .unwrap();
            let item = session
                .items
                .iter_mut()
                .find(|item| item.item_id == item_id)
                .unwrap();
            item.status = ImportItemStatus::Completed;
            item.issue = None;
            session.status = crate::models::import_v2::ImportSessionStatus::Completed;
            service.sessions.save(&context, &files, &session).unwrap();
        });

        let stale = fixture
            .service
            .finalize_exact_duplicate(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.session_id,
                &fixture.first_item_id,
                true,
                || Ok(()),
            )
            .unwrap();

        assert!(stale.is_none());
        let completed = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        assert_eq!(completed.items[0].status, ImportItemStatus::Completed);
        assert!(completed.items[0].issue.is_none());
        let history_count_after = std::fs::read_dir(&history_root)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or_default();
        assert_eq!(history_count_after, history_count_before);
    }

    #[test]
    fn stale_exact_duplicate_finalizer_rejects_a_rebound_item_before_history() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.prepare_exact_duplicate();
        let history_root = fixture.root.join(".app/import-history");
        let history_count_before = std::fs::read_dir(&history_root)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or_default();
        let context = fixture.context.clone();
        let session_id = fixture.session_id.clone();
        let item_id = fixture.first_item_id.clone();
        set_before_exact_duplicate_commit_hook(move || {
            let service = ImportV2Service::default();
            let files = FileStore;
            let mut session = service
                .sessions
                .load(&context, &files, &session_id)
                .unwrap();
            let item = session
                .items
                .iter_mut()
                .find(|item| item.item_id == item_id)
                .unwrap();
            item.task_id = Some("replacement-agent-task".into());
            service.sessions.save(&context, &files, &session).unwrap();
        });

        let task_advanced = std::cell::Cell::new(false);
        let stale = fixture
            .service
            .finalize_exact_duplicate(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.session_id,
                &fixture.first_item_id,
                true,
                || {
                    task_advanced.set(true);
                    Ok(())
                },
            )
            .unwrap();

        assert!(stale.is_none());
        assert!(!task_advanced.get());
        let rebound = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        assert_eq!(
            rebound.items[0].task_id.as_deref(),
            Some("replacement-agent-task")
        );
        assert_eq!(rebound.items[0].status, ImportItemStatus::PreviewReady);
        assert!(rebound.items[0].issue.is_none());
        let history_count_after = std::fs::read_dir(&history_root)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or_default();
        assert_eq!(history_count_after, history_count_before);
    }

    #[test]
    fn asset_collision_key_folds_unicode_case() {
        assert_eq!(
            asset_collision_key("assets/Ä.png"),
            asset_collision_key("assets/ä.png")
        );
    }

    #[test]
    fn failed_item_history_write_failure_is_propagated() {
        let fixture = CommitFixture::two_ready_items();
        fixture.break_second_asset_after_preview();
        set_before_failed_history_write_hook(|path| {
            std::fs::remove_file(path).unwrap();
            std::fs::create_dir(path).unwrap();
        });
        let decisions = [
            fixture.first_item_id.clone(),
            fixture.second_item_id.clone().unwrap(),
        ]
        .into_iter()
        .map(|item_id| CommitItemDecision {
            item_id,
            resolution: None,
        })
        .collect();
        let error = fixture
            .service
            .commit_items(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &fixture.request(decisions),
            )
            .unwrap_err();
        assert_eq!(error.code, "FILE_WRITE_FAILED");
    }

    #[test]
    fn concurrent_new_destinations_are_never_clobbered() {
        use std::cell::RefCell;
        use std::rc::Rc;

        for target_kind in ["raw", "baseline", "asset", "wiki", "index"] {
            let mut fixture = CommitFixture::two_ready_items();
            fixture.second_item_id = None;
            let captured = Rc::new(RefCell::new(None::<PathBuf>));
            let output = captured.clone();
            set_before_new_install_hook(move |path| {
                let normalized = path.to_string_lossy().replace('\\', "/");
                let matches = match target_kind {
                    "raw" => {
                        normalized.contains("/raw/sources/") && normalized.contains("/original.")
                    }
                    "baseline" => normalized.ends_with("/baseline.md"),
                    "asset" => {
                        normalized.contains("/raw/assets/") && normalized.ends_with("/asset.png")
                    }
                    "wiki" => normalized.contains("/wiki/") && normalized.ends_with(".md"),
                    "index" => normalized.ends_with("/.app/source-index-v2.json"),
                    _ => false,
                };
                if matches {
                    std::fs::write(path, b"external concurrent file").unwrap();
                    *output.borrow_mut() = Some(path.to_path_buf());
                }
                matches
            });
            let result = fixture.commit_all();
            assert_eq!(result.failed_count, 1, "{target_kind}");
            let path = captured
                .borrow()
                .clone()
                .expect("race hook must match destination");
            assert_eq!(
                std::fs::read(path).unwrap(),
                b"external concurrent file",
                "{target_kind}"
            );
        }
    }

    #[test]
    fn url_snapshot_preparation_handles_html_fragments_and_bom_json() {
        use flate2::read::GzDecoder;
        use std::io::Read;

        for (fallback, input, expected_extension) in [
            (
                "bin",
                br#"<article data-token="html-secret"><p>keep</p></article>"#.as_slice(),
                "html.gz",
            ),
            (
                "json",
                b"\xEF\xBB\xBF {\"accessToken\":\"json-secret\",\"title\":\"keep\"}".as_slice(),
                "json.gz",
            ),
        ] {
            let prepared = prepare_source_snapshot(
                &crate::models::import_v2::ImportInputKind::Url,
                fallback,
                input.to_vec(),
            )
            .unwrap();
            assert_eq!(prepared.extension, expected_extension);
            assert_eq!(prepared.content_encoding.as_deref(), Some("gzip"));
            let mut decoded = String::new();
            GzDecoder::new(prepared.bytes.as_slice())
                .read_to_string(&mut decoded)
                .unwrap();
            assert!(decoded.contains("keep"));
            assert!(decoded.contains("REDACTED"));
            assert!(!decoded.contains("html-secret"));
            assert!(!decoded.contains("json-secret"));
            if expected_extension == "json.gz" {
                serde_json::from_str::<serde_json::Value>(&decoded).unwrap();
            }
        }
    }

    #[test]
    fn url_commit_writes_compressed_evidence_and_a_readable_final_source() {
        use flate2::read::GzDecoder;
        use std::io::Read;

        let fixture = CommitFixture::ready_url_item();
        let result = fixture.commit_all();
        assert_eq!(result.committed_count, 1);
        let wiki_path = result.items[0]
            .wiki_path
            .as_deref()
            .expect("committed URL must have a Source path");
        let source_id = result.items[0]
            .source_id
            .as_deref()
            .expect("committed URL must have a source id");
        let manifest: SourceManifest = fixture
            .files
            .read_json(&fixture.context, &format!(".app/sources/{source_id}.json"))
            .unwrap();
        assert_eq!(manifest.wiki_path, wiki_path);
        assert!(fixture.root.join(&manifest.wiki_path).is_file());
        let version = manifest
            .versions
            .iter()
            .find(|version| version.version_id == manifest.current_version_id)
            .unwrap();
        let raw_path = &version
            .raw_evidence
            .iter()
            .find(|artifact| artifact.kind == "source_snapshot")
            .unwrap()
            .path;
        assert!(raw_path.ends_with("/snapshot.html.gz"));
        let mut decoded = String::new();
        GzDecoder::new(std::fs::File::open(fixture.root.join(raw_path)).unwrap())
            .read_to_string(&mut decoded)
            .unwrap();
        assert!(decoded.contains("platform evidence"));
        assert!(!decoded.contains("snapshot-secret"));
        assert!(!decoded.contains("url-secret"));
        assert!(decoded.contains("REDACTED"));
        let source_root = std::path::Path::new(raw_path).parent().unwrap();
        let evidence: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture.root.join(source_root).join("source.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(evidence["title"], "Platform title");
        assert_eq!(evidence["contentEncoding"], "gzip");
        assert!(!evidence["url"].as_str().unwrap().contains("secret"));
        assert!(fixture
            .root
            .join(source_root)
            .join("evidence/bilibili-api.json.gz")
            .is_file());
        let mut api_decoded = String::new();
        GzDecoder::new(
            std::fs::File::open(
                fixture
                    .root
                    .join(source_root)
                    .join("evidence/bilibili-api.json.gz"),
            )
            .unwrap(),
        )
        .read_to_string(&mut api_decoded)
        .unwrap();
        let api: serde_json::Value = serde_json::from_str(&api_decoded).unwrap();
        assert_eq!(api["data"]["authorization"], "REDACTED");
        assert_eq!(api["data"]["expires"], "REDACTED");
        assert!(!api_decoded.contains("api-secret"));
        assert!(fixture
            .root
            .join(format!(
                "raw/assets/{}/{}",
                manifest.source_id, manifest.current_version_id
            ))
            .join("asset.png")
            .is_file());
        assert!(fixture
            .root
            .join(source_root)
            .join("derived/extracted.md")
            .is_file());
        assert!(fixture
            .root
            .join(source_root)
            .join("derived/quality.json")
            .is_file());
        let final_source = std::fs::read_to_string(fixture.root.join(wiki_path)).unwrap();
        let (frontmatter, body) =
            crate::services::import_v2::source_finalization::parse_final_source(&final_source)
                .unwrap();
        assert_eq!(frontmatter.source_id, manifest.source_id);
        assert_eq!(frontmatter.version_id, manifest.current_version_id);
        assert_eq!(frontmatter.content_hash, version.content_hash);
        assert!(body.contains("first.pdf"));
        std::fs::remove_dir_all(
            fixture
                .root
                .join(format!(".app/import-sessions/{}", fixture.session_id)),
        )
        .unwrap();
        assert!(std::fs::read_to_string(fixture.root.join(wiki_path)).is_ok());
    }

    #[test]
    fn restricted_url_commit_marks_source_and_keeps_only_safe_identity_summary() {
        let fixture = CommitFixture::ready_url_item();
        let mut session = fixture
            .service
            .sessions
            .load(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        let item = session
            .items
            .iter_mut()
            .find(|item| item.item_id == fixture.first_item_id)
            .unwrap();
        item.restricted_content = true;
        item.restricted_identity_summary = Some("Bilibili account ending in 42".into());
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();

        let result = fixture.commit_all();
        let source_id = result.items[0].source_id.as_deref().unwrap();
        let wiki_path = result.items[0].wiki_path.as_deref().unwrap();
        let manifest: SourceManifest = fixture
            .files
            .read_json(&fixture.context, &format!(".app/sources/{source_id}.json"))
            .unwrap();
        assert!(manifest.restricted_content);
        assert_eq!(
            manifest.restricted_identity_summary.as_deref(),
            Some("Bilibili account ending in 42")
        );
        let final_source = std::fs::read_to_string(fixture.root.join(wiki_path)).unwrap();
        let (frontmatter, _) =
            crate::services::import_v2::source_finalization::parse_final_source(&final_source)
                .unwrap();
        assert!(frontmatter.restricted);
        assert!(!final_source.to_ascii_lowercase().contains("cookie"));
        assert!(!serde_json::to_string(&manifest)
            .unwrap()
            .to_ascii_lowercase()
            .contains("profile_path"));
    }

    #[test]
    fn concurrent_existing_index_update_blocks_without_manifest_inconsistency() {
        let fixture = CommitFixture::updated_source();
        let manifest = fixture.manifest();
        std::fs::write(
            fixture.root.join(&manifest.wiki_path),
            "# User-edited Source\n",
        )
        .unwrap();
        let manifest_path = fixture
            .root
            .join(format!(".app/sources/{}.json", manifest.source_id));
        let before_manifest = std::fs::read(&manifest_path).unwrap();
        let index_path = fixture.root.join(".app/source-index-v2.json");
        set_before_checked_displace_hook(|path| {
            if path.ends_with("source-index-v2.json") {
                std::fs::write(path, b"external index update").unwrap();
                true
            } else {
                false
            }
        });
        let resolution = fixture.bound_resolution("keep");
        let result = fixture.commit_with(Some(resolution));
        assert_eq!(result.failed_count, 1);
        assert_eq!(
            result.items[0].error_code.as_deref(),
            Some(IMPORT_V2_COMMIT_CONFLICT)
        );
        assert_eq!(
            std::fs::read(&index_path).unwrap(),
            b"external index update"
        );
        assert_eq!(std::fs::read(manifest_path).unwrap(), before_manifest);
    }

    #[test]
    fn concurrent_existing_manifest_update_blocks_before_index_commit() {
        let fixture = CommitFixture::updated_source();
        let manifest = fixture.manifest();
        std::fs::write(
            fixture.root.join(&manifest.wiki_path),
            "# User-edited Source\n",
        )
        .unwrap();
        let manifest_path = fixture
            .root
            .join(format!(".app/sources/{}.json", manifest.source_id));
        let before_index = std::fs::read(fixture.root.join(".app/source-index-v2.json")).unwrap();
        set_before_checked_displace_hook(|path| {
            if path.to_string_lossy().contains("/.app/sources/")
                || path.to_string_lossy().contains("\\.app\\sources\\")
            {
                std::fs::write(path, b"external manifest update").unwrap();
                true
            } else {
                false
            }
        });
        let resolution = fixture.bound_resolution("keep");
        let result = fixture.commit_with(Some(resolution));
        assert_eq!(result.failed_count, 1);
        assert_eq!(
            std::fs::read(manifest_path).unwrap(),
            b"external manifest update"
        );
        assert_eq!(
            std::fs::read(fixture.root.join(".app/source-index-v2.json")).unwrap(),
            before_index
        );
    }

    #[test]
    fn commit_rejects_persisted_preview_path_traversal() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        let mut session = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        std::fs::write(fixture.root.join("outside.bin"), b"first.pdf").unwrap();
        session.items[0]
            .preview
            .as_mut()
            .unwrap()
            .source_snapshot
            .relative_path = "../../../../../../outside.bin".into();
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &session)
            .unwrap();
        let result = fixture.commit_all();
        assert_eq!(result.failed_count, 1);
        assert_eq!(
            result.items[0].error_code.as_deref(),
            Some(crate::errors::IMPORT_V2_COMMIT_FAILED)
        );
        assert!(!fixture.root.join("raw/sources").exists());
    }

    #[cfg(unix)]
    #[test]
    fn commit_rejects_symlinked_staging_source() {
        use std::os::unix::fs::symlink;
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        let source = fixture.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging/source.bin",
            fixture.session_id, fixture.first_item_id
        ));
        let outside = fixture.root.join("outside.bin");
        std::fs::write(&outside, b"first.pdf").unwrap();
        std::fs::remove_file(&source).unwrap();
        symlink(&outside, &source).unwrap();
        assert_eq!(fixture.commit_all().failed_count, 1);
        assert!(!fixture.root.join("raw/sources").exists());
    }

    #[cfg(windows)]
    #[test]
    fn commit_rejects_windows_reparse_staging_source_when_supported() {
        use std::os::windows::fs::symlink_file;
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        let source = fixture.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging/source.bin",
            fixture.session_id, fixture.first_item_id
        ));
        let outside = fixture.root.join("outside.bin");
        std::fs::write(&outside, b"first.pdf").unwrap();
        std::fs::remove_file(&source).unwrap();
        if symlink_file(&outside, &source).is_err() {
            return;
        }
        assert_eq!(fixture.commit_all().failed_count, 1);
        assert!(!fixture.root.join("raw/sources").exists());
    }

    #[test]
    fn concurrent_session_edit_is_preserved_and_stops_summary_linearization() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        set_before_checked_displace_hook(|path| {
            let normalized = path.to_string_lossy().replace('\\', "/");
            if normalized.contains("/import-sessions/") && normalized.ends_with("/session.json") {
                std::fs::write(path, b"external session edit").unwrap();
                true
            } else {
                false
            }
        });
        let request = fixture.request(vec![CommitItemDecision {
            item_id: fixture.first_item_id.clone(),
            resolution: None,
        }]);
        let error = fixture
            .service
            .commit_items(&fixture.context, &fixture.files, &fixture.git, &request)
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_COMMIT_CONFLICT);
        let session_path = fixture.root.join(format!(
            ".app/import-sessions/{}/session.json",
            fixture.session_id
        ));
        assert_eq!(
            std::fs::read(session_path).unwrap(),
            b"external session edit"
        );
        // The item transaction may already be complete, but the external
        // session edit is never overwritten; history/item facts permit a
        // later recovery to reconcile the batch safely.
        assert!(fixture.root.join("raw/sources").exists());
    }

    #[test]
    fn concurrent_history_edit_is_preserved() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        set_before_checked_displace_hook(|path| {
            if path
                .to_string_lossy()
                .replace('\\', "/")
                .contains("/.app/import-history/")
            {
                std::fs::write(path, b"external history edit").unwrap();
                true
            } else {
                false
            }
        });
        let request = fixture.request(vec![CommitItemDecision {
            item_id: fixture.first_item_id.clone(),
            resolution: None,
        }]);
        let error = fixture
            .service
            .commit_items(&fixture.context, &fixture.files, &fixture.git, &request)
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_COMMIT_CONFLICT);
        let history = std::fs::read_dir(fixture.root.join(".app/import-history"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(std::fs::read(history).unwrap(), b"external history edit");
    }

    #[test]
    fn staged_artifact_replacement_after_open_cannot_redirect_bound_handle() {
        let root =
            std::env::temp_dir().join(format!("import-v2-artifact-{}", uuid::Uuid::new_v4()));
        let path = root.join("staging/item.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"expected").unwrap();
        set_before_artifact_open_hook(|path| {
            let replacement = path.with_extension("replacement");
            std::fs::write(&replacement, b"attacker").unwrap();
            std::fs::remove_file(path).unwrap();
            std::fs::rename(replacement, path).unwrap();
        });
        assert_eq!(
            verified_artifact(
                &root,
                "staging/item.md",
                &format!("{:x}", Sha256::digest(b"expected"))
            )
            .unwrap(),
            b"expected"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"attacker");
        std::fs::remove_dir_all(root).unwrap();
    }

    fn persistence_target_for(label: &str, path: &str) -> CommitPersistenceTarget {
        if label == "raw snapshot" {
            CommitPersistenceTarget::RawSnapshot
        } else if label == "baseline" {
            CommitPersistenceTarget::Baseline
        } else if label.starts_with("asset ") {
            CommitPersistenceTarget::Asset(
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap()
                    .to_string(),
            )
        } else if label == "extracted Markdown" || label == "quality report" {
            CommitPersistenceTarget::RawExtracted
        } else if let Some(kind) = label.strip_prefix("evidence ") {
            if matches!(kind, "candidate_markdown" | "quality_report") {
                CommitPersistenceTarget::RawExtracted
            } else {
                CommitPersistenceTarget::RawSnapshot
            }
        } else if label == "source evidence" {
            CommitPersistenceTarget::RawSnapshot
        } else if label == "Wiki" {
            CommitPersistenceTarget::Wiki
        } else if label == "source manifest" {
            CommitPersistenceTarget::Manifest
        } else if label == "source index" {
            CommitPersistenceTarget::Index
        } else if label == "batch history" || label == "history preview" {
            CommitPersistenceTarget::History
        } else if label.starts_with("session item ") {
            CommitPersistenceTarget::SessionItem
        } else if label == "session summary" {
            CommitPersistenceTarget::SessionSummary
        } else {
            panic!("unclassified expected durable target {label}: {path}");
        }
    }

    fn expected_item_commit_boundaries(
        targets: &[(String, String)],
    ) -> Vec<CommitPersistenceBoundary> {
        let mut expected = Vec::with_capacity(targets.len() * 2 + 4);
        let summary = targets.iter().find(|(label, _)| label == "session summary");
        for (label, path) in targets
            .iter()
            .filter(|(label, _)| label != "session summary")
        {
            let target = persistence_target_for(label, path);
            expected.push(CommitPersistenceBoundary::JournalIntent(target.clone()));
            expected.push(CommitPersistenceBoundary::TargetInstalled(target));
        }
        expected.push(CommitPersistenceBoundary::CommittedMarkerPersisted);
        expected.push(CommitPersistenceBoundary::JournalDeleted);
        if let Some((label, path)) = summary {
            let target = persistence_target_for(label, path);
            expected.push(CommitPersistenceBoundary::JournalIntent(target.clone()));
            expected.push(CommitPersistenceBoundary::TargetInstalled(target));
            expected.push(CommitPersistenceBoundary::CommittedMarkerPersisted);
            expected.push(CommitPersistenceBoundary::JournalDeleted);
        }
        expected
    }

    fn observed_item_commit_contract(
        fixture: &CommitFixture,
        overwrite: bool,
    ) -> (Vec<(String, String)>, Vec<CommitPersistenceBoundary>) {
        use std::cell::{Cell, RefCell};
        use std::rc::Rc;
        let observed = Rc::new(RefCell::new(Vec::new()));
        let targets = Rc::new(RefCell::new(None));
        let captured_targets = targets.clone();
        set_commit_durable_targets_hook(move |value| *captured_targets.borrow_mut() = Some(value));
        let armed = Rc::new(Cell::new(false));
        let captured = observed.clone();
        let hook_armed = armed.clone();
        set_commit_persistence_hook(move |boundary| {
            if matches!(
                boundary,
                CommitPersistenceBoundary::JournalIntent(CommitPersistenceTarget::RawSnapshot)
            ) {
                hook_armed.set(true);
            }
            if hook_armed.get() {
                captured.borrow_mut().push(boundary.clone());
            }
            false
        });
        if overwrite {
            let resolution = fixture.bound_resolution("apply");
            fixture.commit_with(Some(resolution));
        } else {
            fixture.commit_with(None);
        }
        set_commit_persistence_hook(|_| false);
        let targets = Rc::try_unwrap(targets).unwrap().into_inner().unwrap();
        let observed = Rc::try_unwrap(observed).unwrap().into_inner();
        assert_eq!(
            observed,
            expected_item_commit_boundaries(&targets),
            "every expected persistence hook must fire in durable target order"
        );
        (targets, observed)
    }

    fn read_optional(path: &std::path::Path) -> Option<Vec<u8>> {
        match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!("failed to read {}: {error}", path.display()),
        }
    }

    fn assert_no_transaction_orphans(root: &std::path::Path) {
        fn visit(root: &std::path::Path, path: &std::path::Path) {
            if !path.exists() {
                return;
            }
            for entry in std::fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if entry.file_type().unwrap().is_dir() {
                    visit(root, &path);
                    continue;
                }
                let relative = path.strip_prefix(root).unwrap().to_string_lossy();
                let name = entry.file_name().to_string_lossy().into_owned();
                assert!(
                    !relative.contains(".app\\import-v2-journal")
                        && !relative.contains(".app/import-v2-journal")
                        && !name.contains(".tmp")
                        && !name.contains(".wiki-guard-")
                        && !name.contains(".recovery-"),
                    "transaction orphan remained after recovery: {relative}"
                );
            }
        }
        visit(root, root);
    }

    fn abort_at_item_boundary(
        fixture: &CommitFixture,
        target: CommitPersistenceBoundary,
        occurrence: usize,
        overwrite: bool,
    ) {
        use std::cell::Cell;
        use std::rc::Rc;
        let armed = Rc::new(Cell::new(false));
        let hook_armed = armed.clone();
        let expected = target.clone();
        let mut seen = 0usize;
        use std::cell::RefCell;
        let durable = Rc::new(RefCell::new(None));
        let captured_durable = durable.clone();
        let root = fixture.root.clone();
        set_commit_durable_targets_hook(move |targets| {
            let snapshots: Vec<(String, String, Option<Vec<u8>>)> = targets
                .into_iter()
                .map(|(label, relative)| {
                    let old = read_optional(&root.join(&relative));
                    (label, relative, old)
                })
                .collect();
            *captured_durable.borrow_mut() = Some(snapshots);
        });
        set_commit_persistence_hook(move |boundary| {
            if matches!(
                boundary,
                CommitPersistenceBoundary::JournalIntent(CommitPersistenceTarget::RawSnapshot)
            ) {
                hook_armed.set(true);
            }
            if hook_armed.get() && boundary == &expected {
                seen += 1;
                seen == occurrence
            } else {
                false
            }
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if overwrite {
                let resolution = fixture.bound_resolution("apply");
                fixture.commit_with(Some(resolution));
            } else {
                fixture.commit_with(None);
            }
        }));
        assert!(
            result.is_err(),
            "fault boundary was not reached: {target:?}"
        );
        set_commit_persistence_hook(|_| false);

        let durable = Rc::try_unwrap(durable).unwrap().into_inner().unwrap();
        let crashed: Vec<_> = durable
            .iter()
            .map(|(_, relative, _)| read_optional(&fixture.root.join(relative)))
            .collect();

        let drift = fixture.root.join("wiki/external-drift.md");
        std::fs::create_dir_all(drift.parent().unwrap()).unwrap();
        std::fs::write(&drift, b"external drift before recovery").unwrap();
        let reopened = ImportV2Service::default();
        let session = reopened
            .load_session(&fixture.context, &fixture.files, &fixture.session_id)
            .unwrap();
        let forward = matches!(
            target,
            CommitPersistenceBoundary::CommittedMarkerPersisted
                | CommitPersistenceBoundary::JournalDeleted
        );
        let item_forward = forward
            || matches!(
                target,
                CommitPersistenceBoundary::JournalIntent(CommitPersistenceTarget::SessionSummary)
                    | CommitPersistenceBoundary::TargetInstalled(
                        CommitPersistenceTarget::SessionSummary
                    )
            );
        assert_eq!(
            session.items[0].status == ImportItemStatus::Completed,
            item_forward
        );
        for ((label, relative, old), new) in durable.iter().zip(&crashed) {
            let expected = if label == "session summary" {
                if forward {
                    new
                } else {
                    old
                }
            } else if item_forward {
                new
            } else {
                old
            };
            assert_eq!(
                &read_optional(&fixture.root.join(relative)),
                expected,
                "{label} must recover to exact {} bytes at {target:?}: {relative}",
                if forward { "new" } else { "old-or-absent" }
            );
        }
        assert_eq!(
            std::fs::read(drift).unwrap(),
            b"external drift before recovery"
        );
        assert_no_transaction_orphans(&fixture.root);
    }

    #[test]
    fn real_new_import_commit_recovers_at_every_persistence_boundary() {
        let mut discovery = CommitFixture::two_ready_items();
        discovery.second_item_id = None;
        let (targets, boundaries) = observed_item_commit_contract(&discovery, false);
        assert_eq!(boundaries, expected_item_commit_boundaries(&targets));
        let mut occurrences = std::collections::HashMap::new();
        for boundary in boundaries {
            let occurrence = occurrences
                .entry(boundary.clone())
                .and_modify(|value| *value += 1)
                .or_insert(1);
            let mut fixture = CommitFixture::two_ready_items();
            fixture.second_item_id = None;
            abort_at_item_boundary(&fixture, boundary, *occurrence, false);
        }
    }

    #[test]
    fn real_checked_wiki_overwrite_recovers_at_every_persistence_boundary() {
        let discovery = CommitFixture::updated_source();
        let (targets, boundaries) = observed_item_commit_contract(&discovery, true);
        assert_eq!(boundaries, expected_item_commit_boundaries(&targets));
        let mut occurrences = std::collections::HashMap::new();
        for boundary in boundaries {
            let occurrence = occurrences
                .entry(boundary.clone())
                .and_modify(|value| *value += 1)
                .or_insert(1);
            abort_at_item_boundary(
                &CommitFixture::updated_source(),
                boundary,
                *occurrence,
                true,
            );
        }
    }
}
