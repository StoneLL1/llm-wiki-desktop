use std::path::Path;

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

impl ImportV2Service {
    pub fn commit_items(
        &self,
        context: &ProjectContext,
        file_store: &FileStore,
        git_service: &GitService,
        request: &CommitImportSessionRequest,
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
        for decision in &request.decisions {
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
                let _ = file_store.write_json_atomic(context, &history_path, &batch);
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
        let existing_manifest: Option<SourceManifest> = pointer
            .map(|pointer| {
                files.read_json(context, &format!(".app/sources/{}.json", pointer.source_id))
            })
            .transpose()?;
        let attempt = item.attempts.last();
        let extension = Path::new(&item.input.display_name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("bin")
            .to_string();
        let plan = SourceRegistry.build_commit_plan(
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
        let mut transaction = FileTransaction::new();
        if !duplicate {
            transaction.write(&context.resolve_project_path(&plan.raw_path)?, &source)?;
            transaction.write(
                &context.resolve_project_path(&plan.baseline_path)?,
                &markdown,
            )?;
            for (relative, bytes) in assets {
                let target = format!("{}/{}", plan.asset_root_path, relative);
                transaction.write(&context.resolve_project_path(&target)?, &bytes)?;
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
                transaction.write(&wiki, &markdown)?;
            }
        }
        transaction.write(
            &context.resolve_project_path(&plan.manifest_path)?,
            &json_bytes(&plan.next_manifest)?,
        )?;
        transaction.write(
            &context.resolve_project_path(".app/source-index-v2.json")?,
            &json_bytes(&plan.next_index)?,
        )?;
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
        self.sessions.save(context, files, &session)?;
        transaction.commit();
        Ok(result)
    }
}

fn verified_artifact(
    root: &Path,
    relative: &str,
    expected_hash: &str,
) -> Result<Vec<u8>, BackendError> {
    let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
    let bytes = std::fs::read(&path)
        .map_err(|_| commit_error(IMPORT_V2_COMMIT_FAILED, "Preview artifact is missing."))?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if hash != expected_hash {
        return Err(commit_error(
            IMPORT_V2_COMMIT_FAILED,
            "Preview artifact changed after validation.",
        ));
    }
    Ok(bytes)
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
    use super::asset_collision_key;

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
}
