use std::io::{Read, Write};
use std::path::{Component, Path};

use flate2::{write::GzEncoder, Compression};
use sha2::{Digest, Sha256};

use crate::errors::{
    BackendError, IMPORT_V2_COMMIT_CONFLICT, IMPORT_V2_COMMIT_FAILED, IMPORT_V2_QUALITY_FAILED,
    IMPORT_V2_STATE_INVALID,
};
use crate::models::git::CheckpointPurpose;
use crate::models::import_v2::{
    CommitConflictAction, CommitImportSessionRequest, CommitItemDecision, ImportBatchResult,
    ImportIssue, ImportItemCommitResult, ImportItemStatus, QualityLevel,
};
use crate::models::paths::ProjectContext;
use crate::services::import_v2::orchestrator::derive_session_status;
use crate::services::import_v2::source_registry::{
    SourceCommitInput, SourceManifest, SourceRegistry, SourceResolution,
};
use crate::services::import_v2::transaction::FileTransaction;
use crate::services::import_v2::ImportV2Service;
use crate::services::{FileStore, GitService};

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
    } else if relative.starts_with("raw/sources/") {
        CommitPersistenceTarget::RawSnapshot
    } else if relative.starts_with("raw/extracted/") {
        CommitPersistenceTarget::RawExtracted
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
    pub fn commit_items(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        request: &CommitImportSessionRequest,
    ) -> Result<ImportBatchResult, BackendError> {
        self.commit_items_cancellable(context, file_store, git_service, request, || false)
    }

    pub fn commit_items_cancellable(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        request: &CommitImportSessionRequest,
        is_cancelled: impl Fn() -> bool,
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
        let session = self
            .sessions
            .load(context, file_store, &request.session_id)?;
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
                        committed: false,
                        error_code: Some(crate::errors::IMPORT_V2_CANCELLED.into()),
                    };
                    batch.items.push(result.clone());
                    update_history_snapshot(&mut batch, &result);
                }
                batch.failed_count = batch.items.len() as u32 - batch.committed_count;
                persist_history_checked(context, &history_path, &batch, &history_hash_before)?;
                return Err(commit_error(
                    crate::errors::IMPORT_V2_CANCELLED,
                    "Import commit was cancelled.",
                ));
            }
            let provisional = match self.commit_one(
                context,
                file_store,
                git_service,
                &request.session_id,
                decision,
                &history_path,
                &batch,
                &history_hash_before,
            ) {
                Ok(result) => result,
                Err(error) => ImportItemCommitResult {
                    item_id: decision.item_id.clone(),
                    source_id: None,
                    version_id: None,
                    wiki_path: None,
                    committed: false,
                    error_code: Some(error.code),
                },
            };
            batch.items.push(provisional.clone());
            update_history_snapshot(&mut batch, &provisional);
            batch.committed_count = batch.items.iter().filter(|item| item.committed).count() as u32;
            batch.failed_count = batch.items.len() as u32 - batch.committed_count;
            if !batch.items.last().is_some_and(|item| item.committed) {
                run_before_failed_history_write_hook(&context.resolve_project_path(&history_path)?);
                persist_history_checked(context, &history_path, &batch, &history_hash_before)?;
            }
        }
        Ok(batch)
    }

    fn commit_one(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        git: &GitService,
        session_id: &str,
        decision: &CommitItemDecision,
        history_path: &str,
        prior_batch: &ImportBatchResult,
        history_expected_hash: &str,
    ) -> Result<ImportItemCommitResult, BackendError> {
        let mut session = self.sessions.load(context, files, session_id)?;
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
        if preview.quality.level == QualityLevel::Fail {
            return Err(commit_error(
                IMPORT_V2_QUALITY_FAILED,
                "Failed quality previews cannot be committed.",
            ));
        }
        let staging = context.root.join(format!(
            ".app/import-sessions/{session_id}/items/{}/staging",
            item.item_id
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
        let locator = item.input.normalized_locator.as_deref().ok_or_else(|| {
            commit_error(
                IMPORT_V2_STATE_INVALID,
                "Normalized source locator is missing.",
            )
        })?;
        let resolution = SourceRegistry::resolve(&index, locator, &content_hash);
        let pointer = index
            .by_locator
            .get(locator)
            .or_else(|| index.by_content_hash.get(&content_hash));
        let existing_manifest_path =
            pointer.map(|pointer| format!(".app/sources/{}.json", pointer.source_id));
        let existing_manifest_hash = existing_manifest_path
            .as_deref()
            .map(|path| files.file_hash(context, path))
            .transpose()?;
        let existing_manifest: Option<SourceManifest> = existing_manifest_path
            .as_deref()
            .map(|path| files.read_json(context, path))
            .transpose()?;
        let attempt = item.attempts.last();
        let extension = Path::new(&item.input.display_name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bin")
            .to_string();
        let prepared_source = prepare_source_snapshot(&item.input.kind, &extension, source)?;
        let mut plan = SourceRegistry.build_commit_plan(
            &index,
            existing_manifest.as_ref(),
            &SourceCommitInput {
                normalized_locator: locator.into(),
                content_hash: content_hash.clone(),
                display_name: item.input.display_name.clone(),
                input_kind: item.input.kind.clone(),
                source_extension: prepared_source.extension.clone(),
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
        let source_root = plan
            .raw_path
            .rsplit_once('/')
            .map(|(root, _)| root)
            .ok_or_else(|| commit_error(IMPORT_V2_COMMIT_FAILED, "Raw source path is invalid."))?;
        let mut source_evidence = Vec::new();
        let mut assets = Vec::new();
        let mut extracted = Vec::new();
        let mut source_evidence_targets = std::collections::HashSet::new();
        let mut asset_targets = std::collections::HashSet::new();
        let mut extracted_targets = std::collections::HashSet::new();
        for asset in &preview.assets {
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
            let bytes =
                verified_artifact(&staging, &asset.relative_path, &asset.sha256)?;
            let (relative, bytes) = if source_evidence_artifact
                && item.input.kind == crate::models::import_v2::ImportInputKind::Url
            {
                prepare_url_source_evidence(relative, bytes)?
            } else {
                (relative.to_string(), bytes)
            };
            let extracted_artifact = matches!(
                &asset.kind,
                crate::models::import_v2::ArtifactKind::Metadata
                    | crate::models::import_v2::ArtifactKind::Subtitle
                    | crate::models::import_v2::ArtifactKind::Transcript
            );
            let target_root: &str = if source_evidence_artifact {
                source_root
            } else if extracted_artifact {
                plan.extracted_root_path.as_str()
            } else {
                plan.asset_root_path.as_str()
            };
            let target = if source_evidence_artifact {
                format!("{target_root}/evidence/{relative}")
            } else {
                format!("{target_root}/{relative}")
            };
            let targets = if source_evidence_artifact {
                &mut source_evidence_targets
            } else if extracted_artifact {
                &mut extracted_targets
            } else {
                &mut asset_targets
            };
            if !targets.insert(asset_collision_key(&target)) {
                return Err(commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Preview assets resolve to the same target.",
                ));
            }
            let artifact = (relative, bytes);
            if source_evidence_artifact {
                source_evidence.push(artifact);
            } else if extracted_artifact {
                extracted.push(artifact);
            } else {
                assets.push(artifact);
            }
        }
        // Keep the derived Markdown/transcript evidence alongside metadata
        // under raw/extracted. The baseline remains in .app for conflict and
        // preview bookkeeping, while this copy is the durable source-derived
        // artifact promised by the import contract.
        let extracted_markdown_target = format!("{}/extracted.md", plan.extracted_root_path);
        if !extracted_targets.insert(asset_collision_key(&extracted_markdown_target)) {
            return Err(commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Preview extracted Markdown resolves to an existing target.",
            ));
        }
        extracted.push((
            "extracted.md".into(),
            verified_artifact(
                &staging,
                &preview.markdown.relative_path,
                &preview.markdown.sha256,
            )?,
        ));
        let quality_target = format!("{}/quality.json", plan.extracted_root_path);
        if !extracted_targets.insert(asset_collision_key(&quality_target)) {
            return Err(commit_error(
                IMPORT_V2_COMMIT_FAILED,
                "Preview quality report resolves to an existing target.",
            ));
        }
        extracted.push(("quality.json".into(), json_bytes(&preview.quality)?));
        let source_record_path = format!("{source_root}/source.json");
        let source_record = SourceEvidenceRecord {
            schema_version: 1,
            title: preview.title.clone(),
            author: metadata_author(&extracted),
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
            source_locator: crate::services::import_v2::redaction::redact_sensitive_text(locator),
            media_save_mode: item.input.media_save_mode.clone(),
            snapshot_path: plan.raw_path.clone(),
            snapshot_sha256: preview.source_snapshot.sha256.clone(),
            content_sha256: content_hash,
            stored_sha256: format!("{:x}", Sha256::digest(&prepared_source.bytes)),
            content_encoding: prepared_source.content_encoding.clone(),
            original_bytes: preview.source_snapshot.size_bytes,
            stored_bytes: prepared_source.bytes.len() as u64,
            saved_at: chrono::Utc::now().to_rfc3339(),
        };
        let duplicate = matches!(
            resolution,
            SourceResolution::ExactDuplicate { .. } | SourceResolution::SameContentNewOrigin { .. }
        );
        let writes_wiki = item.input.kind != crate::models::import_v2::ImportInputKind::Url;
        let wiki_exists = writes_wiki && files.exists(context, &plan.wiki_path);
        if writes_wiki
            && wiki_exists
            && !duplicate
            && decision.conflict_action == Some(CommitConflictAction::CreateNew)
        {
            plan.wiki_path = collision_free_wiki_path(context, &plan.wiki_path)?;
            plan.next_manifest.wiki_path = plan.wiki_path.clone();
        }
        let wiki_exists = writes_wiki && files.exists(context, &plan.wiki_path);
        let overwrite_wiki = wiki_exists
            && decision.conflict_action == Some(CommitConflictAction::ApplyMergedCandidate);
        if wiki_exists
            && !duplicate
            && !overwrite_wiki
            && decision.conflict_action != Some(CommitConflictAction::KeepWiki)
        {
            return Err(commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "Existing Wiki updates require an explicit conflict action.",
            ));
        }
        if overwrite_wiki {
            let expected = decision.expected_wiki_hash.as_deref().ok_or_else(|| {
                commit_error(IMPORT_V2_COMMIT_CONFLICT, "Expected Wiki hash is required.")
            })?;
            if files.file_hash(context, &plan.wiki_path)? != expected {
                return Err(commit_error(
                    IMPORT_V2_COMMIT_CONFLICT,
                    "Wiki changed after preview.",
                ));
            }
        }
        let wiki_markdown = writes_wiki
            .then(|| rewrite_wiki_asset_links(&markdown, &plan.wiki_path, &plan.asset_root_path));
        for path in [
            &plan.raw_path,
            &source_record_path,
            &plan.asset_root_path,
            &plan.extracted_root_path,
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
                || files.exists(context, &plan.extracted_root_path)
                || files.exists(context, &plan.baseline_path))
        {
            return Err(commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "Immutable raw version already exists.",
            ));
        }
        if overwrite_wiki {
            git.create_scoped_checkpoint(
                context,
                CheckpointPurpose::HighRiskOperation,
                "Before import Wiki update",
                std::slice::from_ref(&plan.wiki_path),
            )?;
        }
        let result = ImportItemCommitResult {
            item_id: item.item_id.clone(),
            source_id: Some(plan.source_id.clone()),
            version_id: Some(plan.version_id.clone()),
            wiki_path: writes_wiki.then(|| plan.wiki_path.clone()),
            committed: true,
            error_code: None,
        };
        let mut history = prior_batch.clone();
        history.items.push(result.clone());
        update_history_snapshot(&mut history, &result);
        let history_preview_path = format!(
            ".app/import-history-previews/{}/{}.md",
            history.batch_id, item.item_id
        );
        history.committed_count =
            history.items.iter().filter(|entry| entry.committed).count() as u32;
        history.failed_count = history.items.len() as u32 - history.committed_count;
        let session_expected_hashes: std::collections::HashMap<_, _> = self
            .sessions
            .serialized_writes(&session)?
            .into_iter()
            .map(|(path, bytes)| (path, format!("{:x}", Sha256::digest(&bytes))))
            .collect();
        #[cfg(test)]
        {
            let mut targets = vec![
                ("raw snapshot".into(), plan.raw_path.clone()),
                ("source evidence".into(), source_record_path.clone()),
                ("baseline".into(), plan.baseline_path.clone()),
            ];
            targets.extend(source_evidence.iter().map(|(relative, _)| {
                (
                    format!("source evidence {relative}"),
                    format!("{source_root}/evidence/{relative}"),
                )
            }));
            targets.extend(preview.assets.iter().filter_map(|asset| {
                if matches!(
                    &asset.kind,
                    crate::models::import_v2::ArtifactKind::SourceEvidence
                ) {
                    return None;
                }
                let relative = asset
                    .relative_path
                    .strip_prefix("assets/")
                    .unwrap_or(&asset.relative_path);
                let root: &str = if matches!(
                    &asset.kind,
                    crate::models::import_v2::ArtifactKind::Metadata
                        | crate::models::import_v2::ArtifactKind::Subtitle
                        | crate::models::import_v2::ArtifactKind::Transcript
                ) {
                    plan.extracted_root_path.as_str()
                } else {
                    plan.asset_root_path.as_str()
                };
                let target = format!("{root}/{relative}");
                Some((format!("asset {relative}"), target))
            }));
            targets.push((
                "extracted Markdown".into(),
                format!("{}/extracted.md", plan.extracted_root_path),
            ));
            targets.push((
                "quality report".into(),
                format!("{}/quality.json", plan.extracted_root_path),
            ));
            if writes_wiki {
                targets.push(("Wiki".into(), plan.wiki_path.clone()));
            }
            targets.extend([
                ("source manifest".into(), plan.manifest_path.clone()),
                ("source index".into(), ".app/source-index-v2.json".into()),
                ("history preview".into(), history_preview_path.clone()),
                ("batch history".into(), history_path.to_string()),
            ]);
            targets.extend(session.items.iter().map(|session_item| {
                (
                    format!("session item {}", session_item.item_id),
                    format!(
                        ".app/import-sessions/{session_id}/items/{}.json",
                        session_item.item_id
                    ),
                )
            }));
            targets.push((
                "session summary".into(),
                format!(".app/import-sessions/{session_id}/session.json"),
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
                transaction.write_new(
                    &context.resolve_project_path(&source_record_path)?,
                    &json_bytes(&source_record)?,
                )?;
                transaction.write_new(
                    &context.resolve_project_path(&plan.baseline_path)?,
                    &markdown,
                )?;
                for (relative, bytes) in source_evidence {
                    let target = format!("{source_root}/evidence/{relative}");
                    transaction.write_new(&context.resolve_project_path(&target)?, &bytes)?;
                }
                for (relative, bytes) in assets {
                    let target = format!("{}/{}", plan.asset_root_path, relative);
                    transaction.write_new(&context.resolve_project_path(&target)?, &bytes)?;
                }
                for (relative, bytes) in extracted {
                    let target = format!("{}/{}", plan.extracted_root_path, relative);
                    transaction.write_new(&context.resolve_project_path(&target)?, &bytes)?;
                }
            }
            if writes_wiki && (!wiki_exists || overwrite_wiki) {
                let wiki = context.resolve_project_path(&plan.wiki_path)?;
                if overwrite_wiki {
                    transaction.write_if_hash_matches(
                        &wiki,
                        wiki_markdown.as_deref().unwrap(),
                        decision.expected_wiki_hash.as_deref().unwrap(),
                    )?;
                } else {
                    transaction.write_new(&wiki, wiki_markdown.as_deref().unwrap())?;
                }
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
                &markdown,
            )?;
            transaction.write_if_hash_matches(
                &context.resolve_project_path(history_path)?,
                &json_bytes(&history)?,
                history_expected_hash,
            )?;
            session.items[item_position].status = ImportItemStatus::Committing;
            session.items[item_position].status = ImportItemStatus::Completed;
            session.status = derive_session_status(&session.items);
            session.updated_at = chrono::Utc::now().to_rfc3339();
            for (relative, bytes) in self.sessions.serialized_writes(&session)? {
                transaction.write_if_hash_matches(
                    &context.resolve_project_path(&relative)?,
                    &bytes,
                    session_expected_hashes.get(&relative).ok_or_else(|| {
                        commit_error(
                            IMPORT_V2_COMMIT_FAILED,
                            "Import session changed during commit.",
                        )
                    })?,
                )?;
            }
            Ok(())
        })();
        if let Err(error) = write_result {
            return Err(transaction.rollback_after(error));
        }
        transaction.commit()?;
        Ok(result)
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
        && (trimmed.starts_with('{')
            || trimmed.starts_with('[')
            || fallback_extension == "json")
    {
        Some("json")
    } else if text.is_some()
        && (trimmed.starts_with('<')
            || fallback_extension == "html"
            || fallback_extension == "htm")
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
        crate::services::import_v2::redaction::redact_json_snapshot(
            trimmed,
        )
        .ok_or_else(|| {
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
    Ok((
        format!("{stem}.{}", prepared.extension),
        prepared.bytes,
    ))
}

fn metadata_author(extracted: &[(String, Vec<u8>)]) -> Option<String> {
    extracted.iter().find_map(|(path, bytes)| {
        if !path.ends_with(".json") || path == "quality.json" {
            return None;
        }
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        author_from_value(&value)
    })
}

fn author_from_value(value: &serde_json::Value) -> Option<String> {
    let object = value.as_object()?;
    for key in ["author", "creator", "uploader", "nickname", "ownerName"] {
        if let Some(value) = object.get(key) {
            if let Some(author) = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                return Some(author.chars().take(512).collect());
            }
            if let Some(author) = value.as_object().and_then(|author| {
                ["name", "nickname", "displayName", "uname"]
                    .into_iter()
                    .find_map(|key| author.get(key)?.as_str())
            }) {
                let author = author.trim();
                if !author.is_empty() {
                    return Some(author.chars().take(512).collect());
                }
            }
        }
    }
    object.values().find_map(author_from_value)
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

fn rewrite_wiki_asset_links(markdown: &[u8], wiki_path: &str, asset_root: &str) -> Vec<u8> {
    let from = Path::new(wiki_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let target = Path::new(asset_root)
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let common = from
        .iter()
        .zip(&target)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec!["..".to_string(); from.len().saturating_sub(common)];
    parts.extend(target.into_iter().skip(common));
    let prefix = parts.join("/");
    let Ok(markdown) = std::str::from_utf8(markdown) else {
        return markdown.to_vec();
    };
    if prefix.is_empty() {
        markdown.as_bytes().to_vec()
    } else {
        markdown
            .replace("assets/", &format!("{prefix}/"))
            .into_bytes()
    }
}

fn collision_free_wiki_path(
    context: &ProjectContext,
    preferred: &str,
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
    let existing = if absolute_parent.is_dir() {
        std::fs::read_dir(&absolute_parent)
            .map_err(|_| {
                commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Wiki target directory could not be inspected.",
                )
            })?
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str().map(|name| name.to_lowercase()))
            .collect::<std::collections::HashSet<_>>()
    } else {
        std::collections::HashSet::new()
    };
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
        if !existing.contains(&filename.to_lowercase()) {
            return Ok(format!(
                "{}/{}",
                parent.to_string_lossy().replace('\\', "/"),
                filename
            ));
        }
    }
    unreachable!()
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
        if item.status == ImportItemStatus::NeedsMerge && decision.conflict_action.is_none() {
            return Err(commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "Merge conflicts require an explicit conflict action.",
            ));
        }
        if decision.conflict_action == Some(CommitConflictAction::ApplyMergedCandidate)
            && decision
                .expected_wiki_hash
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(commit_error(
                IMPORT_V2_COMMIT_CONFLICT,
                "Expected Wiki hash is required.",
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

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::errors::IMPORT_V2_COMMIT_CONFLICT;
    use crate::models::import_v2::{
        ArtifactKind, CommitConflictAction, CommitImportSessionRequest, CommitItemDecision,
        ImportArtifact, ImportBatchResult, ImportInput, ImportInputKind, ImportItemStatus,
        ImportResourceMode, MediaSaveMode,
    };
    use crate::models::paths::ProjectContext;
    use crate::models::task::TaskType;
    use crate::services::import_v2::engine::{
        EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
    };
    use crate::services::import_v2::source_registry::{SourceIndex, SourceManifest};
    use crate::services::{FileStore, GitService};
    use crate::tasks::task_model::CancellationToken;
    use crate::tasks::TaskService;

    use super::super::ImportV2Service;
    use super::{
        asset_collision_key, content_identity_hash, set_before_artifact_open_hook,
        set_before_failed_history_write_hook, set_commit_durable_targets_hook,
        set_commit_persistence_hook, verified_artifact, prepare_source_snapshot,
        CommitPersistenceBoundary, CommitPersistenceTarget,
    };
    use crate::services::import_v2::transaction::set_before_checked_displace_hook;
    use crate::services::import_v2::transaction::set_before_new_install_hook;

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
            let inputs = ["first.pdf", "second.pdf"].map(|name| ImportInput {
                source_identity: None,
                kind: ImportInputKind::File,
                display_name: name.into(),
                locator: format!("D:/{name}"),
                normalized_locator: Some(format!("file:d:/{name}")),
                media_save_mode: Default::default(),
            });
            let session = service
                .add_inputs(&context, &files, &session.session_id, inputs.into())
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
            fixture.second_item_id = None;
            fixture
                .git
                .initialize_repository(&fixture.context, "initial")
                .unwrap();
            fixture.commit_with(None, None);
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
                locator: "D:/first.pdf".into(),
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
                decisions,
            }
        }
        fn commit_all(&self) -> crate::models::import_v2::ImportBatchResult {
            let decisions = [
                Some(self.first_item_id.clone()),
                self.second_item_id.clone(),
            ]
            .into_iter()
            .flatten()
            .map(|item_id| CommitItemDecision {
                item_id,
                conflict_action: None,
                expected_wiki_hash: None,
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
            action: Option<CommitConflictAction>,
            hash: Option<&str>,
        ) -> crate::models::import_v2::ImportBatchResult {
            self.service
                .commit_items(
                    &self.context,
                    &self.files,
                    &self.git,
                    &self.request(vec![CommitItemDecision {
                        item_id: self.first_item_id.clone(),
                        conflict_action: action,
                        expected_wiki_hash: hash.map(str::to_string),
                    }]),
                )
                .unwrap()
        }
        fn break_second_asset_after_preview(&self) {
            let id = self.second_item_id.as_ref().unwrap();
            std::fs::remove_file(self.root.join(format!(
                ".app/import-sessions/{}/items/{id}/staging/assets/asset.png",
                self.session_id
            )))
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
        let history_path = std::fs::read_dir(fixture.root.join(".app/import-history"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let history: ImportBatchResult =
            serde_json::from_slice(&std::fs::read(history_path).unwrap()).unwrap();
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
                conflict_action: None,
                expected_wiki_hash: None,
            },
            CommitItemDecision {
                item_id: fixture.second_item_id.clone().unwrap(),
                conflict_action: None,
                expected_wiki_hash: None,
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
                conflict_action: None,
                expected_wiki_hash: None,
            },
            CommitItemDecision {
                item_id: fixture.second_item_id.clone().unwrap(),
                conflict_action: None,
                expected_wiki_hash: None,
            },
        ]);
        let checks = std::cell::Cell::new(0);
        let error = fixture
            .service
            .commit_items_cancellable(
                &fixture.context,
                &fixture.files,
                &fixture.git,
                &request,
                || {
                    let next = checks.get() + 1;
                    checks.set(next);
                    next > 1
                },
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
    }

    #[test]
    fn create_new_derives_collision_free_cjk_portable_wiki_path() {
        let fixture = CommitFixture::updated_source();
        let manifest = fixture.manifest();
        let original = fixture.root.join(&manifest.wiki_path);
        std::fs::write(&original, "external edit").unwrap();
        let colliding = original.with_file_name(format!(
            "{}-2.md",
            original.file_stem().unwrap().to_string_lossy()
        ));
        std::fs::write(&colliding, "existing collision").unwrap();
        let result = fixture.commit_with(Some(CommitConflictAction::CreateNew), None);
        assert!(result.items[0].committed);
        let created = result.items[0].wiki_path.as_deref().unwrap();
        assert!(
            created.ends_with("-3.md"),
            "unexpected derived path: {created}"
        );
        assert_eq!(std::fs::read_to_string(original).unwrap(), "external edit");
        assert_eq!(
            std::fs::read_to_string(colliding).unwrap(),
            "existing collision"
        );
        assert!(fixture.root.join(created).is_file());
    }

    #[test]
    fn invalid_later_decision_cannot_commit_an_earlier_item_or_create_history() {
        let fixture = CommitFixture::two_ready_items();
        let history_dir = fixture.root.join(".app/import-history");
        let request = fixture.request(vec![
            CommitItemDecision {
                item_id: fixture.first_item_id.clone(),
                conflict_action: None,
                expected_wiki_hash: None,
            },
            CommitItemDecision {
                item_id: "missing-item".into(),
                conflict_action: None,
                expected_wiki_hash: None,
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
    fn duplicate_decisions_are_rejected_before_history_or_item_mutation() {
        let fixture = CommitFixture::two_ready_items();
        let decision = CommitItemDecision {
            item_id: fixture.first_item_id.clone(),
            conflict_action: None,
            expected_wiki_hash: None,
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
        let result = fixture.commit_with(
            Some(CommitConflictAction::ApplyMergedCandidate),
            Some("stale-hash"),
        );
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
    fn exact_duplicate_records_alias_without_copying_raw_bytes() {
        let mut fixture = CommitFixture::two_ready_items();
        fixture.second_item_id = None;
        let first = fixture.commit_all().items[0].clone();
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
            display_name: "first.pdf".into(),
            locator: "D:/alias/first.pdf".into(),
            normalized_locator: Some("file:d:/alias/first.pdf".into()),
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
                "alias".into(),
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
        let alias = fixture.commit_all().items[0].clone();
        assert_eq!(alias.source_id, first.source_id);
        assert_eq!(alias.version_id, first.version_id);
        let manifest = fixture.manifest();
        assert_eq!(manifest.versions.len(), 1);
        assert!(manifest.origins.contains(&"file:d:/alias/first.pdf".into()));
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
            conflict_action: None,
            expected_wiki_hash: None,
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
    fn url_commit_writes_compressed_evidence_without_creating_a_wiki_page() {
        use flate2::read::GzDecoder;
        use std::io::Read;

        let fixture = CommitFixture::ready_url_item();
        let result = fixture.commit_all();
        assert_eq!(result.committed_count, 1);
        assert!(result.items[0].wiki_path.is_none());
        let source_id = result.items[0]
            .source_id
            .as_deref()
            .expect("committed URL must have a source id");
        let manifest: SourceManifest = fixture
            .files
            .read_json(&fixture.context, &format!(".app/sources/{source_id}.json"))
            .unwrap();
        assert!(!fixture.root.join(&manifest.wiki_path).exists());
        let version = manifest
            .versions
            .iter()
            .find(|version| version.version_id == manifest.current_version_id)
            .unwrap();
        assert!(version.raw_path.ends_with("/original.html.gz"));
        let mut decoded = String::new();
        GzDecoder::new(std::fs::File::open(fixture.root.join(&version.raw_path)).unwrap())
            .read_to_string(&mut decoded)
            .unwrap();
        assert!(decoded.contains("platform evidence"));
        assert!(!decoded.contains("snapshot-secret"));
        assert!(!decoded.contains("url-secret"));
        assert!(decoded.contains("REDACTED"));
        let source_root = std::path::Path::new(&version.raw_path).parent().unwrap();
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
            .join(&version.extracted_path)
            .join("extracted.md")
            .is_file());
        assert!(fixture
            .root
            .join(&version.extracted_path)
            .join("quality.json")
            .is_file());
    }

    #[test]
    fn concurrent_existing_index_update_blocks_without_manifest_inconsistency() {
        let fixture = CommitFixture::updated_source();
        let manifest = fixture.manifest();
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
        let result = fixture.commit_with(Some(CommitConflictAction::KeepWiki), None);
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
        let result = fixture.commit_with(Some(CommitConflictAction::KeepWiki), None);
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
    fn concurrent_session_edit_is_preserved_and_commit_rolls_back() {
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
        let result = fixture.commit_all();
        assert_eq!(result.failed_count, 1);
        let session_path = fixture.root.join(format!(
            ".app/import-sessions/{}/session.json",
            fixture.session_id
        ));
        assert_eq!(
            std::fs::read(session_path).unwrap(),
            b"external session edit"
        );
        assert!(!fixture.root.join("raw/sources").exists());
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
            conflict_action: None,
            expected_wiki_hash: None,
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
        } else if let Some(name) = label.strip_prefix("asset ") {
            CommitPersistenceTarget::Asset(name.to_string())
        } else if label == "extracted Markdown" || label == "quality report" {
            CommitPersistenceTarget::RawExtracted
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
        let mut expected = Vec::with_capacity(targets.len() * 2 + 2);
        for (label, path) in targets {
            let target = persistence_target_for(label, path);
            expected.push(CommitPersistenceBoundary::JournalIntent(target.clone()));
            expected.push(CommitPersistenceBoundary::TargetInstalled(target));
        }
        expected.push(CommitPersistenceBoundary::CommittedMarkerPersisted);
        expected.push(CommitPersistenceBoundary::JournalDeleted);
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
            let manifest = fixture.manifest();
            let hash = fixture
                .files
                .file_hash(&fixture.context, &manifest.wiki_path)
                .unwrap();
            fixture.commit_with(
                Some(CommitConflictAction::ApplyMergedCandidate),
                Some(&hash),
            );
        } else {
            fixture.commit_with(None, None);
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
        let checked_wiki = overwrite.then(|| {
            let manifest = fixture.manifest();
            let path = fixture.root.join(&manifest.wiki_path);
            let original = std::fs::read(&path).unwrap();
            (path, original, b"# updated.pdf".to_vec())
        });
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
                let manifest = fixture.manifest();
                let hash = fixture
                    .files
                    .file_hash(&fixture.context, &manifest.wiki_path)
                    .unwrap();
                fixture.commit_with(
                    Some(CommitConflictAction::ApplyMergedCandidate),
                    Some(&hash),
                );
            } else {
                fixture.commit_with(None, None);
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
        assert_eq!(
            session.items[0].status == ImportItemStatus::Completed,
            forward
        );
        for ((label, relative, old), new) in durable.iter().zip(&crashed) {
            let expected = if forward { new } else { old };
            assert_eq!(
                &read_optional(&fixture.root.join(relative)),
                expected,
                "{label} must recover to exact {} bytes at {target:?}: {relative}",
                if forward { "new" } else { "old-or-absent" }
            );
        }
        if let Some((wiki, original, candidate)) = checked_wiki {
            assert_eq!(
                std::fs::read(wiki).unwrap(),
                if forward { candidate } else { original },
                "checked Wiki overwrite must recover to the exact old or new bytes"
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
