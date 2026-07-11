use std::io::Read;
use std::path::{Component, Path};

use sha2::{Digest, Sha256};

use crate::errors::{
    BackendError, IMPORT_V2_COMMIT_CONFLICT, IMPORT_V2_COMMIT_FAILED, IMPORT_V2_QUALITY_FAILED,
    IMPORT_V2_STATE_INVALID,
};
use crate::models::git::CheckpointPurpose;
use crate::models::import_v2::{
    CommitConflictAction, CommitImportSessionRequest, CommitItemDecision, ImportBatchResult,
    ImportItemCommitResult, ImportItemStatus, QualityLevel,
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
thread_local! {
    static BEFORE_FAILED_HISTORY_WRITE_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
fn set_before_failed_history_write_hook(hook: impl FnOnce(&Path) + 'static) {
    BEFORE_FAILED_HISTORY_WRITE_HOOK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
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
        if request.project_id != context.project_id
            || Path::new(&request.project_root_path) != context.root
        {
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
        let session = self
            .sessions
            .load(context, file_store, &request.session_id)?;
        validate_complete_decision_set(&session, &request.decisions)?;
        let batch_id = uuid::Uuid::new_v4().to_string();
        let history_path = format!(".app/import-history/{batch_id}.json");
        let mut batch = ImportBatchResult {
            batch_id,
            session_id: request.session_id.clone(),
            committed_count: 0,
            failed_count: 0,
            items: Vec::new(),
        };
        file_store.write_json_atomic(context, &history_path, &batch)?;
        for (position, decision) in request.decisions.iter().enumerate() {
            if is_cancelled() {
                for unprocessed in &request.decisions[position..] {
                    batch.items.push(ImportItemCommitResult {
                        item_id: unprocessed.item_id.clone(),
                        source_id: None,
                        version_id: None,
                        wiki_path: None,
                        committed: false,
                        error_code: Some(crate::errors::IMPORT_V2_CANCELLED.into()),
                    });
                }
                batch.failed_count = batch.items.len() as u32 - batch.committed_count;
                file_store.write_json_atomic(context, &history_path, &batch)?;
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
            batch.items.push(provisional);
            batch.committed_count = batch.items.iter().filter(|item| item.committed).count() as u32;
            batch.failed_count = batch.items.len() as u32 - batch.committed_count;
            if !batch.items.last().is_some_and(|item| item.committed) {
                run_before_failed_history_write_hook(&context.resolve_project_path(&history_path)?);
                file_store.write_json_atomic(context, &history_path, &batch)?;
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
        let mut assets = Vec::new();
        let mut asset_targets = std::collections::HashSet::new();
        for asset in &preview.assets {
            let relative = asset
                .relative_path
                .strip_prefix("assets/")
                .unwrap_or(&asset.relative_path);
            let target = format!("{}/{}", plan_asset_placeholder(), relative);
            if !asset_targets.insert(asset_collision_key(&target)) {
                return Err(commit_error(
                    IMPORT_V2_COMMIT_FAILED,
                    "Preview assets resolve to the same target.",
                ));
            }
            assets.push((
                relative.to_string(),
                verified_artifact(&staging, &asset.relative_path, &asset.sha256)?,
            ));
        }
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
        let resolution = SourceRegistry::resolve(&index, locator, &preview.source_snapshot.sha256);
        let pointer = index
            .by_locator
            .get(locator)
            .or_else(|| index.by_content_hash.get(&preview.source_snapshot.sha256));
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
        let mut plan = SourceRegistry.build_commit_plan(
            &index,
            existing_manifest.as_ref(),
            &SourceCommitInput {
                normalized_locator: locator.into(),
                content_hash: preview.source_snapshot.sha256.clone(),
                display_name: item.input.display_name.clone(),
                input_kind: item.input.kind.clone(),
                source_extension: extension,
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
        let duplicate = matches!(
            resolution,
            SourceResolution::ExactDuplicate { .. } | SourceResolution::SameContentNewOrigin { .. }
        );
        let wiki_exists = files.exists(context, &plan.wiki_path);
        if wiki_exists
            && !duplicate
            && decision.conflict_action == Some(CommitConflictAction::CreateNew)
        {
            plan.wiki_path = collision_free_wiki_path(context, &plan.wiki_path)?;
            plan.next_manifest.wiki_path = plan.wiki_path.clone();
        }
        let wiki_exists = files.exists(context, &plan.wiki_path);
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
        for path in [
            &plan.raw_path,
            &plan.asset_root_path,
            &plan.baseline_path,
            &plan.wiki_path,
            &plan.manifest_path,
            ".app/source-index-v2.json",
            history_path,
        ] {
            context.resolve_project_path(path)?;
        }
        if !duplicate
            && (files.exists(context, &plan.raw_path) || files.exists(context, &plan.baseline_path))
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
            wiki_path: Some(plan.wiki_path.clone()),
            committed: true,
            error_code: None,
        };
        let mut history = prior_batch.clone();
        history.items.push(result.clone());
        history.committed_count =
            history.items.iter().filter(|entry| entry.committed).count() as u32;
        history.failed_count = history.items.len() as u32 - history.committed_count;
        let mut transaction = FileTransaction::new_for_project(&context.root);
        let write_result = (|| -> Result<(), BackendError> {
            if !duplicate {
                transaction.write_new(&context.resolve_project_path(&plan.raw_path)?, &source)?;
                transaction.write_new(
                    &context.resolve_project_path(&plan.baseline_path)?,
                    &markdown,
                )?;
                for (relative, bytes) in assets {
                    let target = format!("{}/{}", plan.asset_root_path, relative);
                    transaction.write_new(&context.resolve_project_path(&target)?, &bytes)?;
                }
            }
            if !wiki_exists || overwrite_wiki {
                let wiki = context.resolve_project_path(&plan.wiki_path)?;
                if overwrite_wiki {
                    transaction.write_if_hash_matches(
                        &wiki,
                        &markdown,
                        decision.expected_wiki_hash.as_deref().unwrap(),
                    )?;
                } else {
                    transaction.write_new(&wiki, &markdown)?;
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
            transaction.write(
                &context.resolve_project_path(history_path)?,
                &json_bytes(&history)?,
            )?;
            session.items[item_position].status = ImportItemStatus::Committing;
            session.items[item_position].status = ImportItemStatus::Completed;
            session.status = derive_session_status(&session.items);
            session.updated_at = chrono::Utc::now().to_rfc3339();
            let session_root =
                context.resolve_project_path(&format!(".app/import-sessions/{session_id}"))?;
            transaction.track(&session_root.join("session.json"))?;
            for item in &session.items {
                transaction.track(
                    &session_root
                        .join("items")
                        .join(format!("{}.json", item.item_id)),
                )?;
            }
            self.sessions.save(context, files, &session)
        })();
        if let Err(error) = write_result {
            return Err(transaction.rollback_after(error));
        }
        transaction.commit()?;
        Ok(result)
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

fn plan_asset_placeholder() -> &'static str {
    "asset-root"
}

fn asset_collision_key(path: &str) -> String {
    path.to_lowercase()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::errors::IMPORT_V2_COMMIT_CONFLICT;
    use crate::models::import_v2::{
        CommitConflictAction, CommitImportSessionRequest, CommitItemDecision, ImportInput,
        ImportInputKind, ImportResourceMode,
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
    use super::{asset_collision_key, set_before_failed_history_write_hook};
    use crate::services::import_v2::transaction::set_before_checked_displace_hook;
    use crate::services::import_v2::transaction::set_before_new_install_hook;

    struct FixtureEngine {
        root: PathBuf,
    }
    impl ImportEngine for FixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "commit-fixture".into(),
                engine_version: "1".into(),
                route: "fixture".into(),
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
                title: request.input.display_name.clone(),
                text_coverage: Some(1.0),
                table_cell_accuracy: None,
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
                kind: ImportInputKind::File,
                display_name: name.into(),
                locator: format!("D:/{name}"),
                normalized_locator: Some(format!("file:d:/{name}")),
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
                kind: ImportInputKind::File,
                display_name: "updated.pdf".into(),
                locator: "D:/first.pdf".into(),
                normalized_locator: Some("file:d:/first.pdf".into()),
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
        fn request(&self, decisions: Vec<CommitItemDecision>) -> CommitImportSessionRequest {
            CommitImportSessionRequest {
                project_id: self.context.project_id.clone(),
                project_root_path: self.root.to_string_lossy().into(),
                session_id: self.session_id.clone(),
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
        let manifest = fixture.manifest();
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
            kind: ImportInputKind::File,
            display_name: "first.pdf".into(),
            locator: "D:/alias/first.pdf".into(),
            normalized_locator: Some("file:d:/alias/first.pdf".into()),
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
                    "asset" => normalized.ends_with("/assets/asset.png"),
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
}
