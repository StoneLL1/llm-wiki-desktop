use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::app_state::ProjectWritePermit;
use crate::errors::BackendError;
use crate::models::import_v2::{
    ArtifactKind, ImportArtifact, ImportBatchResult, ImportItem, ImportPreviewArtifact,
};
use crate::models::import_v2_agent::{
    AgentAuditRecord, AgentCandidate, AgentCandidateDiff, AgentCandidateManifest,
};
use crate::models::paths::ProjectContext;
use crate::models::task::{TaskResultReference, TaskStatus, TaskType};
use crate::services::import_v2::agent_workspace::AgentTaskBundle;
use crate::services::import_v2::engine::EngineResult;
use crate::services::import_v2::quality_gate::QualityGate;
use crate::services::import_v2::source_registry::{SourceIndex, SourceRegistry};
use crate::services::import_v2::ImportV2Service;
use crate::services::{FileStore, GitService};
use crate::tasks::TaskService;

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_OUTPUT_FILES: usize = 256;
const MAX_OUTPUT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct StoredAgentCandidate {
    candidate: AgentCandidate,
    diff: AgentCandidateDiff,
    deterministic_preview: Option<ImportPreviewArtifact>,
}

pub struct AgentCandidateService<'a> {
    imports: &'a ImportV2Service,
    files: &'a FileStore,
    tasks: &'a TaskService,
}

impl<'a> AgentCandidateService<'a> {
    pub fn new(imports: &'a ImportV2Service, files: &'a FileStore, tasks: &'a TaskService) -> Self {
        Self {
            imports,
            files,
            tasks,
        }
    }

    fn accept_staged_output_unchecked(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<AgentCandidate, BackendError> {
        let task = self
            .tasks
            .get_task(task_id)
            .ok_or_else(|| candidate_error("Agent task was not found."))?;
        if task.status != TaskStatus::Succeeded
            || task.task_type != TaskType::AgentRun
            || task.project_id.as_deref() != Some(context.project_id.as_str())
        {
            return Err(candidate_error(
                "Only a succeeded task bound to this project may be validated.",
            ));
        }
        let result = task
            .result
            .as_ref()
            .ok_or_else(|| candidate_error("Agent task has no result."))?;
        if !matches!(result.reference.as_ref(), Some(TaskResultReference::ImportPreview { session_id: bound_session, item_id: bound_item }) if bound_session == session_id && bound_item == item_id)
        {
            return Err(candidate_error(
                "Agent task result belongs to another import item.",
            ));
        }
        let session = self.imports.load_session(context, self.files, session_id)?;
        let item = session
            .items
            .iter()
            .find(|item| item.item_id == item_id)
            .ok_or_else(|| candidate_error("Import item was not found."))?;
        if item.task_id.as_deref() != Some(task_id) {
            return Err(candidate_error(
                "Agent task is not bound to this import item.",
            ));
        }
        let previous = self.imports.begin_agent_candidate_validation_unchecked(
            context, self.files, session_id, item_id, task_id,
        )?;
        match self.accept_staged_output_validating(context, session_id, item_id, task_id) {
            Ok(candidate) => Ok(candidate),
            Err(validation_error) => {
                self.imports.reject_agent_candidate_validation(
                    context, self.files, session_id, item_id, task_id, previous,
                )?;
                Err(validation_error)
            }
        }
    }

    fn accept_staged_output_validating(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<AgentCandidate, BackendError> {
        let task = self
            .tasks
            .get_task(task_id)
            .ok_or_else(|| candidate_error("Agent task was not found."))?;
        if task.status != TaskStatus::Succeeded
            || task.task_type != TaskType::AgentRun
            || task.project_id.as_deref() != Some(context.project_id.as_str())
        {
            return Err(candidate_error(
                "Only a succeeded task bound to this project may be validated.",
            ));
        }
        if self.tasks.is_cancelled(task_id) {
            return Err(candidate_error(
                "Cancelled Agent output cannot become a candidate.",
            ));
        }
        let result = task
            .result
            .as_ref()
            .ok_or_else(|| candidate_error("Agent task has no result."))?;
        if !matches!(result.reference.as_ref(), Some(TaskResultReference::ImportPreview { session_id: bound_session, item_id: bound_item }) if bound_session == session_id && bound_item == item_id)
        {
            return Err(candidate_error(
                "Agent task result belongs to another import item.",
            ));
        }
        let output_ref = result
            .affected_paths
            .iter()
            .find(|path| path.ends_with("/output"))
            .ok_or_else(|| candidate_error("Agent task has no staged output reference."))?;
        let output_dir = safe_project_directory(context, output_ref)?;
        let workspace = output_dir
            .parent()
            .ok_or_else(|| candidate_error("Agent workspace is invalid."))?;
        validate_workspace_identity(workspace, session_id, item_id)?;

        let bundle: AgentTaskBundle =
            read_json_limited(&workspace.join("task.json"), MAX_MANIFEST_BYTES)?;
        if bundle.session_id != session_id || bundle.item_id != item_id {
            return Err(candidate_error(
                "Agent task bundle belongs to another item.",
            ));
        }
        let session = self.imports.load_session(context, self.files, session_id)?;
        let item = session
            .items
            .iter()
            .find(|item| item.item_id == item_id)
            .ok_or_else(|| candidate_error("Import item was not found."))?;
        if item.task_id.as_deref() != Some(task_id) {
            return Err(candidate_error(
                "Agent task is not bound to this import item.",
            ));
        }

        let source_relative = single_source_relative(workspace)?;
        let source_bytes = read_regular_file(&workspace.join(&source_relative), MAX_OUTPUT_BYTES)?;
        let source_hash = hash_bytes(&source_bytes);
        if !bundle.input_hashes.iter().any(|hash| hash == &source_hash)
            || item
                .preview
                .as_ref()
                .is_some_and(|preview| preview.source_snapshot.sha256 != source_hash)
        {
            return Err(candidate_error(
                "The source snapshot changed before candidate validation.",
            ));
        }
        validate_deterministic_inputs(workspace, item, &bundle)?;
        self.ensure_not_cancelled(task_id)?;
        let manifest: AgentCandidateManifest =
            read_json_limited(&output_dir.join("manifest.json"), MAX_MANIFEST_BYTES)?;
        validate_manifest(&manifest)?;
        validate_declared_output_tree(&output_dir, &manifest)?;
        QualityGate::validate_agent_text_fields(
            std::iter::once(manifest.processing_summary.as_str())
                .chain(manifest.tools_used.iter().map(String::as_str))
                .chain(manifest.uncertainties.iter().map(String::as_str))
                .chain(manifest.warnings.iter().map(String::as_str)),
        )?;
        let markdown_relative = normalize_relative(&manifest.markdown_path)?;
        let markdown_bytes =
            read_regular_file(&output_dir.join(&markdown_relative), MAX_OUTPUT_BYTES)?;
        if hash_bytes(&markdown_bytes) != manifest.markdown_sha256 {
            return Err(candidate_error(
                "Agent Markdown hash does not match its manifest.",
            ));
        }
        let mut asset_bytes = Vec::new();
        for path in &manifest.asset_paths {
            let relative = normalize_relative(path)?;
            let bytes = read_regular_file(&output_dir.join(&relative), MAX_OUTPUT_BYTES)?;
            if manifest.asset_sha256.get(&relative) != Some(&hash_bytes(&bytes)) {
                return Err(candidate_error(
                    "Agent asset hash does not match its manifest.",
                ));
            }
            QualityGate::validate_agent_asset(&relative, &bytes)?;
            asset_bytes.push((relative, bytes));
        }
        self.ensure_not_cancelled(task_id)?;
        let candidate_id =
            hash_bytes(format!("{task_id}:{source_hash}:{}", manifest.markdown_sha256).as_bytes());
        let candidate_root_relative = candidate_root_path(session_id, item_id, &candidate_id)?;
        let candidate_artifact_prefix = candidate_artifact_prefix(&candidate_id)?;
        let candidate_root = context.resolve_project_path(&candidate_root_relative)?;
        let record_relative = candidate_record_path(session_id, item_id, &candidate_id)?;
        if candidate_root.exists() && !self.files.exists(context, &record_relative) {
            reject_links_between(&context.root, &candidate_root)?;
            let metadata = fs::symlink_metadata(&candidate_root).map_err(io_error)?;
            if !metadata.is_dir() {
                return Err(candidate_error(
                    "Incomplete candidate storage is not a directory.",
                ));
            }
            fs::remove_dir_all(&candidate_root).map_err(io_error)?;
        }
        fs::create_dir_all(&candidate_root).map_err(io_error)?;
        reject_links_between(&context.root, &candidate_root)?;
        write_sealed_file(&candidate_root.join("source.bin"), &source_bytes)?;
        write_sealed_file(&candidate_root.join("candidate.md"), &markdown_bytes)?;
        for (path, bytes) in &asset_bytes {
            write_sealed_file(&candidate_root.join(path), bytes)?;
        }
        let engine_result = EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: manifest.asset_paths.clone(),
            metadata_path: None,
            title: format!("AI-assisted: {}", item.input.display_name),
            warnings: manifest
                .warnings
                .iter()
                .cloned()
                .chain(std::iter::once("AGENT_QUALITY_NOT_MEASURED".into()))
                .collect(),
            text_coverage: None,
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: None,
        };
        let mut preview = QualityGate.evaluate_agent_candidate(&candidate_root, &engine_result)?;
        prefix_artifact(&mut preview.markdown, &candidate_artifact_prefix);
        for asset in &mut preview.assets {
            prefix_artifact(asset, &candidate_artifact_prefix);
        }
        let deterministic_path = workspace.join("deterministic/candidate.md");
        let deterministic_markdown = if deterministic_path.is_file() {
            String::from_utf8(read_regular_file(&deterministic_path, MAX_OUTPUT_BYTES)?)
                .map_err(|_| candidate_error("Deterministic baseline is not UTF-8 Markdown."))?
        } else {
            String::new()
        };
        let (registry_baseline, current_markdown) = registry_markdown_views(
            context,
            self.files,
            item.input.normalized_locator.as_deref(),
            &source_hash,
        )?;
        let baseline_markdown = registry_baseline.unwrap_or(deterministic_markdown);
        let baseline_path = candidate_root.join("baseline.md");
        write_sealed_file(&baseline_path, baseline_markdown.as_bytes())?;
        let agent_path = candidate_root.join("candidate.md");
        let agent_markdown = String::from_utf8(markdown_bytes)
            .map_err(|_| candidate_error("Agent Markdown is not UTF-8."))?;
        let needs_three_way_merge = current_markdown
            .as_ref()
            .is_some_and(|current| current != &baseline_markdown);
        let unified_diff = GitService::diff_candidate_files(context, &baseline_path, &agent_path)?;
        let audit_path =
            format!(".app/import-sessions/{session_id}/items/{item_id}/agent-audit/{task_id}.json");
        let mut audit: AgentAuditRecord =
            self.files.read_json(context, &audit_path).map_err(|_| {
                candidate_error("Agent candidate provenance audit is missing or invalid.")
            })?;
        validate_candidate_audit(&audit, &bundle, &manifest, session_id, item_id, task_id)?;
        if audit.outcome == "output_staged" {
            audit.outcome = "succeeded".into();
            self.files.write_json_atomic(context, &audit_path, &audit)?;
        }
        let mut candidate = AgentCandidate {
            candidate_id: candidate_id.clone(),
            task_id: task_id.into(),
            audit_id: audit.audit_id,
            trigger: bundle.trigger,
            agent_kind: audit.agent_kind,
            agent_version: audit.agent_version,
            prompt_template_version: audit.prompt_template_version,
            approved_cost_micros: audit.approved_cost_micros,
            tool_calls: audit.tool_calls,
            markdown: preview.markdown,
            assets: preview.assets,
            quality: preview.quality,
            processing_summary: manifest.processing_summary,
            tools_used: manifest.tools_used,
            uncertainties: manifest.uncertainties,
            warnings: manifest.warnings,
            source_snapshot_sha256: source_hash,
            created_at: Utc::now(),
        };
        let diff = AgentCandidateDiff {
            candidate_id: candidate_id.clone(),
            baseline_markdown,
            current_markdown_sha256: current_markdown
                .as_deref()
                .map(|markdown| hash_bytes(markdown.as_bytes())),
            current_markdown,
            agent_markdown,
            unified_diff,
            needs_three_way_merge,
        };
        self.ensure_not_cancelled(task_id)?;
        let deterministic_preview = if self.files.exists(context, &record_relative) {
            let existing: StoredAgentCandidate = self.files.read_json(context, &record_relative)?;
            let mut expected = candidate.clone();
            expected.created_at = existing.candidate.created_at.to_owned();
            // Candidate records written before the hash field was introduced
            // are still valid when every other sealed fact matches. The new
            // diff is persisted below, upgrading the record for future merge
            // actions without accepting a changed current Wiki silently.
            let mut existing_diff = existing.diff.clone();
            if existing_diff.current_markdown_sha256.is_none() {
                existing_diff.current_markdown_sha256 = diff.current_markdown_sha256.clone();
            }
            if existing.candidate != expected || existing_diff != diff {
                return Err(candidate_error(
                    "An existing candidate record does not match the revalidated output.",
                ));
            }
            candidate = expected;
            existing.deterministic_preview
        } else {
            item.preview.clone()
        };
        self.files.write_json_atomic(
            context,
            &record_relative,
            &StoredAgentCandidate {
                candidate: candidate.clone(),
                diff,
                deterministic_preview,
            },
        )?;
        self.ensure_not_cancelled(task_id)?;
        self.imports.register_agent_candidate(
            context,
            self.files,
            session_id,
            item_id,
            task_id,
            candidate.clone(),
            needs_three_way_merge,
        )?;
        Ok(candidate)
    }

    pub(crate) fn accept_staged_output_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<AgentCandidate, BackendError> {
        self.accept_staged_output_unchecked(permit.context(), session_id, item_id, task_id)
    }

    #[cfg(debug_assertions)]
    pub fn accept_staged_output(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<AgentCandidate, BackendError> {
        self.accept_staged_output_unchecked(context, session_id, item_id, task_id)
    }

    pub fn load_candidate(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        candidate_id: &str,
    ) -> Result<(AgentCandidate, AgentCandidateDiff), BackendError> {
        let relative = candidate_record_path(session_id, item_id, candidate_id)?;
        let stored: StoredAgentCandidate = self.files.read_json(context, &relative)?;
        Ok((stored.candidate, stored.diff))
    }

    /// Compatibility surface for integration tests. Production selection is
    /// performed by the write-permit-bearing combined action below.
    #[cfg(debug_assertions)]
    pub fn select_candidate(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        candidate_id: &str,
        merged_markdown: Option<&str>,
        expected_current_wiki_sha256: Option<&str>,
    ) -> Result<ImportItem, BackendError> {
        self.imports.with_agent_candidate_action_lock(|| {
            self.select_candidate_locked(
                context,
                session_id,
                item_id,
                candidate_id,
                merged_markdown,
                expected_current_wiki_sha256,
            )
        })
    }

    pub(crate) fn select_candidate_and_finalize_exact_duplicate(
        &self,
        permit: &ProjectWritePermit<'_>,
        git_service: &GitService,
        session_id: &str,
        item_id: &str,
        candidate_id: &str,
        merged_markdown: Option<&str>,
        expected_current_wiki_sha256: Option<&str>,
        restricted_content_acknowledged: bool,
    ) -> Result<(ImportItem, Option<ImportBatchResult>), BackendError> {
        let context = permit.context();
        self.imports.with_agent_candidate_action_lock(|| {
            self.select_candidate_locked(
                context,
                session_id,
                item_id,
                candidate_id,
                merged_markdown,
                expected_current_wiki_sha256,
            )?;
            let batch = self.imports.finalize_exact_duplicate_authorized(
                permit,
                self.files,
                git_service,
                session_id,
                item_id,
                restricted_content_acknowledged,
                || Ok(()),
            )?;
            let item = self
                .imports
                .load_session(context, self.files, session_id)?
                .items
                .into_iter()
                .find(|item| item.item_id == item_id)
                .ok_or_else(|| candidate_error("Import item was not found after selection."))?;
            Ok((item, batch))
        })
    }

    fn select_candidate_locked(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        candidate_id: &str,
        merged_markdown: Option<&str>,
        expected_current_wiki_sha256: Option<&str>,
    ) -> Result<ImportItem, BackendError> {
        let stored = self.load_stored(context, session_id, item_id, candidate_id)?;
        let expected_candidate_id = hash_bytes(
            format!(
                "{}:{}:{}",
                stored.candidate.task_id,
                stored.candidate.source_snapshot_sha256,
                stored.candidate.markdown.sha256
            )
            .as_bytes(),
        );
        if stored.candidate.candidate_id != candidate_id || expected_candidate_id != candidate_id {
            return Err(candidate_error(
                "Candidate identity no longer matches its sealed content.",
            ));
        }
        let session = self.imports.load_session(context, self.files, session_id)?;
        let item = session
            .items
            .iter()
            .find(|item| item.item_id == item_id)
            .ok_or_else(|| candidate_error("Import item was not found."))?;
        if item.task_id.as_deref() != Some(stored.candidate.task_id.as_str()) {
            return Err(candidate_error(
                "Candidate is no longer bound to this import item task.",
            ));
        }
        let root_relative = candidate_root_path(session_id, item_id, candidate_id)?;
        let artifact_prefix = candidate_artifact_prefix(candidate_id)?;
        let root = context.resolve_project_path(&root_relative)?;
        reject_links_between(&context.root, &root)?;
        let source_bytes = read_regular_file(&root.join("source.bin"), MAX_OUTPUT_BYTES)?;
        if hash_bytes(&source_bytes) != stored.candidate.source_snapshot_sha256 {
            return Err(candidate_error(
                "Candidate source snapshot changed after validation.",
            ));
        }
        let agent_bytes = read_regular_file(&root.join("candidate.md"), MAX_OUTPUT_BYTES)?;
        if hash_bytes(&agent_bytes) != stored.candidate.markdown.sha256 {
            return Err(candidate_error(
                "Candidate Markdown changed after validation.",
            ));
        }
        for asset in &stored.candidate.assets {
            let relative = strip_candidate_prefix(&asset.relative_path, &artifact_prefix)?;
            let bytes = read_regular_file(&root.join(&relative), MAX_OUTPUT_BYTES)?;
            if bytes.len() as u64 != asset.size_bytes || hash_bytes(&bytes) != asset.sha256 {
                return Err(candidate_error("Candidate asset changed after validation."));
            }
        }
        let mut selected_markdown = stored.candidate.markdown.clone();
        let mut selected_quality = stored.candidate.quality.clone();
        let explicit_merge_current_hash = if stored.diff.needs_three_way_merge {
            let merged = merged_markdown.ok_or_else(|| {
                candidate_error("A three-way merge requires explicit merged Markdown.")
            })?;
            let expected = expected_current_wiki_sha256.ok_or_else(|| {
                candidate_error("A three-way merge requires the expected current Wiki hash.")
            })?;
            let (_, current) = registry_markdown_views(
                context,
                self.files,
                item.input.normalized_locator.as_deref(),
                &stored.candidate.source_snapshot_sha256,
            )?;
            let current = current
                .ok_or_else(|| candidate_error("Current Wiki content is unavailable for merge."))?;
            if hash_bytes(current.as_bytes()) != expected
                || stored.diff.current_markdown.as_deref() != Some(current.as_str())
            {
                return Err(BackendError::new(
                    "IMPORT_AGENT_MERGE_STALE",
                    "Current Wiki changed after the Agent Diff was generated.",
                    false,
                    true,
                ));
            }
            let merged_name = format!("merged-{}.md", hash_bytes(merged.as_bytes()));
            let merged_path = root.join(&merged_name);
            write_sealed_file(&merged_path, merged.as_bytes())?;
            let asset_paths = stored
                .candidate
                .assets
                .iter()
                .map(|artifact| strip_candidate_prefix(&artifact.relative_path, &artifact_prefix))
                .collect::<Result<Vec<_>, _>>()?;
            let result = EngineResult {
                source_snapshot_path: "source.bin".into(),
                markdown_path: merged_name,
                asset_paths,
                metadata_path: None,
                title: format!("AI-assisted merged: {}", item.input.display_name),
                warnings: vec![
                    "AGENT_THREE_WAY_MERGE_REQUIRES_CONFIRMATION".into(),
                    "AGENT_QUALITY_NOT_MEASURED".into(),
                ],
                text_coverage: None,
                table_cell_accuracy: None,
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
                continuation: None,
            };
            let merged_preview = QualityGate.evaluate_agent_candidate(&root, &result)?;
            selected_markdown = merged_preview.markdown;
            prefix_artifact(&mut selected_markdown, &artifact_prefix);
            selected_quality = merged_preview.quality;
            Some(expected.to_string())
        } else if merged_markdown.is_some() || expected_current_wiki_sha256.is_some() {
            return Err(candidate_error(
                "Merge content is accepted only for a three-way candidate.",
            ));
        } else {
            None
        };
        let source = ImportArtifact {
            kind: ArtifactKind::SourceSnapshot,
            relative_path: format!("{artifact_prefix}/source.bin"),
            sha256: stored.candidate.source_snapshot_sha256.clone(),
            size_bytes: source_bytes.len() as u64,
        };
        let preview = ImportPreviewArtifact {
            markdown: selected_markdown,
            assets: stored.candidate.assets.clone(),
            source_snapshot: source,
            quality: selected_quality,
            title: format!("AI-assisted: {}", item.input.display_name),
            resolution: None,
            manual_merge: None,
        };
        let selected = self.imports.select_agent_candidate(
            context,
            self.files,
            session_id,
            item_id,
            &stored.candidate.task_id,
            preview,
            explicit_merge_current_hash.as_deref(),
        )?;
        self.cleanup_task_workspace(context, session_id, item_id, &stored.candidate.task_id)?;
        Ok(selected)
    }

    fn discard_candidate_unchecked(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        candidate_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.imports.with_agent_candidate_action_lock(|| {
            self.discard_candidate_locked(context, session_id, item_id, candidate_id)
        })
    }

    fn discard_candidate_locked(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        candidate_id: &str,
    ) -> Result<ImportItem, BackendError> {
        let stored = self.load_stored(context, session_id, item_id, candidate_id)?;
        let item = self.imports.discard_agent_candidate(
            context,
            self.files,
            session_id,
            item_id,
            &stored.candidate.task_id,
            stored.deterministic_preview,
        )?;
        let root_relative = candidate_root_path(session_id, item_id, candidate_id)?;
        let root = context.resolve_project_path(&root_relative)?;
        reject_links_between(&context.root, &root)?;
        fs::remove_dir_all(&root).map_err(io_error)?;
        self.cleanup_task_workspace(context, session_id, item_id, &stored.candidate.task_id)?;
        Ok(item)
    }

    pub(crate) fn discard_candidate_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        session_id: &str,
        item_id: &str,
        candidate_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.discard_candidate_unchecked(permit.context(), session_id, item_id, candidate_id)
    }

    #[cfg(debug_assertions)]
    pub fn discard_candidate(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        candidate_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.discard_candidate_unchecked(context, session_id, item_id, candidate_id)
    }

    fn recover_completed_outputs_unchecked(
        &self,
        context: &ProjectContext,
        session_id: &str,
    ) -> Result<crate::models::import_v2::ImportSession, BackendError> {
        let session = self.imports.load_session(context, self.files, session_id)?;
        for item in &session.items {
            super::agent_workspace::AgentWorkspaceBuilder::cleanup_abandoned_leases(
                context,
                session_id,
                &item.item_id,
                |task_id| {
                    self.tasks.get_task(task_id).is_some_and(|task| {
                        !matches!(task.status, TaskStatus::Failed | TaskStatus::Cancelled)
                    })
                },
            )?;
        }
        let completed = session
            .items
            .iter()
            .filter_map(|item| {
                let task_id = item.task_id.as_deref()?;
                let task = self.tasks.get_task(task_id)?;
                let exact_reference = matches!(
                    task.result.as_ref().and_then(|result| result.reference.as_ref()),
                    Some(TaskResultReference::ImportPreview { session_id: bound_session, item_id: bound_item })
                        if bound_session == session_id && bound_item == &item.item_id
                );
                let exact_attempt = item.attempts.iter().any(|attempt| {
                    attempt.route == format!("agent_assistance/{task_id}")
                        && attempt.outcome == crate::models::import_v2::AttemptOutcome::Succeeded
                        && !attempt
                            .warnings
                            .iter()
                            .any(|warning| {
                                warning == "AGENT_CANDIDATE_REJECTED"
                                    || warning == "AGENT_CANDIDATE_DISCARDED"
                            })
                });
                (task.status == TaskStatus::Succeeded
                    && task.task_type == TaskType::AgentRun
                    && exact_reference
                    && exact_attempt)
                    .then(|| (item.item_id.clone(), task_id.to_owned()))
            })
            .collect::<Vec<_>>();
        for (item_id, task_id) in completed {
            if let Err(error) =
                self.accept_staged_output_unchecked(context, session_id, &item_id, &task_id)
            {
                let latest = self.imports.load_session(context, self.files, session_id)?;
                let rejection_persisted = latest
                    .items
                    .iter()
                    .find(|item| item.item_id == item_id)
                    .is_some_and(|item| {
                        item.attempts.iter().any(|attempt| {
                            attempt.route == format!("agent_assistance/{task_id}")
                                && attempt
                                    .warnings
                                    .iter()
                                    .any(|warning| warning == "AGENT_CANDIDATE_REJECTED")
                        })
                    });
                if !rejection_persisted {
                    return Err(error);
                }
            }
        }
        let latest = self.imports.load_session(context, self.files, session_id)?;
        for item in &latest.items {
            let has_agent_attempt = item
                .attempts
                .iter()
                .any(|attempt| attempt.route.starts_with("agent_assistance/"));
            let registered_candidate = matches!(
                item.status,
                crate::models::import_v2::ImportItemStatus::PreviewReady
                    | crate::models::import_v2::ImportItemStatus::NeedsMerge
            ) && item.preview.as_ref().is_some_and(|preview| {
                preview
                    .markdown
                    .relative_path
                    .starts_with("agent-candidates/")
            });
            let terminal_without_candidate = has_agent_attempt
                && matches!(
                    item.status,
                    crate::models::import_v2::ImportItemStatus::Failed
                        | crate::models::import_v2::ImportItemStatus::Cancelled
                );
            if registered_candidate || terminal_without_candidate {
                if let Some(task_id) = item.task_id.as_deref() {
                    self.cleanup_task_workspace(context, session_id, &item.item_id, task_id)?;
                }
            }
        }
        Ok(latest)
    }

    pub(crate) fn recover_completed_outputs_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        session_id: &str,
    ) -> Result<crate::models::import_v2::ImportSession, BackendError> {
        self.recover_completed_outputs_unchecked(permit.context(), session_id)
    }

    #[cfg(debug_assertions)]
    pub fn recover_completed_outputs(
        &self,
        context: &ProjectContext,
        session_id: &str,
    ) -> Result<crate::models::import_v2::ImportSession, BackendError> {
        self.recover_completed_outputs_unchecked(context, session_id)
    }

    fn load_stored(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        candidate_id: &str,
    ) -> Result<StoredAgentCandidate, BackendError> {
        let relative = candidate_record_path(session_id, item_id, candidate_id)?;
        self.files.read_json(context, &relative)
    }

    fn ensure_not_cancelled(&self, task_id: &str) -> Result<(), BackendError> {
        if self.tasks.is_cancelled(task_id) {
            Err(candidate_error(
                "Cancelled Agent output cannot become a candidate.",
            ))
        } else {
            Ok(())
        }
    }

    fn cleanup_task_workspace(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<(), BackendError> {
        let audit_path =
            format!(".app/import-sessions/{session_id}/items/{item_id}/agent-audit/{task_id}.json");
        if !self.files.exists(context, &audit_path) {
            return Ok(());
        }
        let audit: AgentAuditRecord = self.files.read_json(context, &audit_path)?;
        if audit.task_id != task_id || audit.session_id != session_id || audit.item_id != item_id {
            return Err(candidate_error(
                "Recorded Agent workspace belongs to another task.",
            ));
        }
        super::agent_workspace::AgentWorkspaceBuilder::cleanup_recorded_workspace(
            context,
            session_id,
            item_id,
            &audit.workspace_relative_path,
        )
    }
}

fn validate_candidate_audit(
    audit: &AgentAuditRecord,
    bundle: &AgentTaskBundle,
    manifest: &AgentCandidateManifest,
    session_id: &str,
    item_id: &str,
    task_id: &str,
) -> Result<(), BackendError> {
    let exact_output = audit.output_hashes == vec![manifest.markdown_sha256.clone()];
    let common = audit.task_id == task_id
        && audit.session_id == session_id
        && audit.item_id == item_id
        && audit.trigger == bundle.trigger
        && matches!(audit.outcome.as_str(), "output_staged" | "succeeded")
        && audit.completed_at.is_some()
        && !audit.agent_version.trim().is_empty()
        && audit.tool_calls.is_empty()
        && exact_output;
    let route_valid = if let Some(command) = audit.route.strip_prefix("local/") {
        audit
            .agent_kind
            .is_some_and(|kind| kind.command() == command)
            && audit.prompt_template_version == "import-recovery/local-v1"
            && audit.approved_cost_micros.is_none()
            && audit.approved_scope_sha256.is_none()
            && audit.input_hashes == bundle.input_hashes
            && audit.granted_tools == bundle.allowed_tools
    } else {
        false
    };
    if common && route_valid {
        Ok(())
    } else {
        Err(candidate_error(
            "Agent candidate provenance does not match the completed task.",
        ))
    }
}

fn strip_candidate_prefix(path: &str, root: &str) -> Result<String, BackendError> {
    path.strip_prefix(&format!("{root}/"))
        .map(str::to_owned)
        .ok_or_else(|| candidate_error("Candidate artifact is outside its immutable set."))
}

fn candidate_record_path(
    session_id: &str,
    item_id: &str,
    candidate_id: &str,
) -> Result<String, BackendError> {
    for value in [session_id, item_id, candidate_id] {
        if value.is_empty()
            || !value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            return Err(candidate_error("Candidate identity is invalid."));
        }
    }
    Ok(format!(
        ".app/import-sessions/{session_id}/items/{item_id}/staging/agent-candidates/{candidate_id}/candidate.json"
    ))
}

fn candidate_root_path(
    session_id: &str,
    item_id: &str,
    candidate_id: &str,
) -> Result<String, BackendError> {
    let record = candidate_record_path(session_id, item_id, candidate_id)?;
    Ok(record.trim_end_matches("/candidate.json").into())
}

fn candidate_artifact_prefix(candidate_id: &str) -> Result<String, BackendError> {
    if candidate_id.len() != 64 || !candidate_id.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(candidate_error("Candidate identity is invalid."));
    }
    Ok(format!("agent-candidates/{candidate_id}"))
}

fn validate_manifest(manifest: &AgentCandidateManifest) -> Result<(), BackendError> {
    normalize_relative(&manifest.markdown_path)?;
    if !manifest.markdown_path.to_ascii_lowercase().ends_with(".md")
        || manifest.processing_summary.trim().is_empty()
        || manifest.tools_used.is_empty()
        || manifest.uncertainties.is_empty()
        || manifest.warnings.is_empty()
        || !valid_sha256(&manifest.markdown_sha256)
        || manifest
            .tools_used
            .iter()
            .chain(&manifest.uncertainties)
            .chain(&manifest.warnings)
            .any(|value| value.trim().is_empty() || value.len() > 2048)
    {
        return Err(candidate_error("Agent manifest is incomplete."));
    }
    let allowed_tools = [
        "tool-free-local-agent",
        "inspect_source",
        "run_deterministic_route",
        "run_ocr",
        "run_asr",
        "parse_sanitized_snapshot",
        "validate_candidate",
    ];
    if manifest
        .tools_used
        .iter()
        .any(|tool| !allowed_tools.contains(&tool.as_str()))
    {
        return Err(candidate_error(
            "Agent manifest claims an unauthorized tool.",
        ));
    }
    let mut declared = HashSet::new();
    declared.insert(normalize_relative(&manifest.markdown_path)?);
    for path in &manifest.asset_paths {
        let path = normalize_relative(path)?;
        let extension = Path::new(&path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "json" | "txt" | "csv"
        ) || !declared.insert(path)
        {
            return Err(candidate_error(
                "Agent manifest declares an unsafe or duplicate asset.",
            ));
        }
    }
    let declared_assets: HashSet<String> = manifest
        .asset_paths
        .iter()
        .map(|path| normalize_relative(path))
        .collect::<Result<_, _>>()?;
    if manifest.asset_sha256.len() != declared_assets.len()
        || manifest.asset_sha256.iter().any(|(path, hash)| {
            normalize_relative(path).ok().as_ref() != Some(path)
                || !declared_assets.contains(path)
                || !valid_sha256(hash)
        })
    {
        return Err(candidate_error(
            "Agent manifest asset hashes are incomplete or invalid.",
        ));
    }
    Ok(())
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn validate_declared_output_tree(
    output: &Path,
    manifest: &AgentCandidateManifest,
) -> Result<(), BackendError> {
    let mut allowed: HashSet<String> = manifest
        .asset_paths
        .iter()
        .map(|path| normalize_relative(path))
        .collect::<Result<_, _>>()?;
    allowed.insert(normalize_relative(&manifest.markdown_path)?);
    allowed.insert("manifest.json".into());
    let mut found = Vec::new();
    let mut total_bytes = 0;
    collect_output_files(output, output, &mut found, &mut total_bytes)?;
    if found.len() > MAX_OUTPUT_FILES
        || total_bytes > MAX_OUTPUT_BYTES
        || found.iter().any(|path| !allowed.contains(path))
        || allowed.iter().any(|path| !found.contains(path))
    {
        return Err(candidate_error(
            "Agent output contains missing, unrelated, or executable files.",
        ));
    }
    Ok(())
}

fn collect_output_files(
    root: &Path,
    current: &Path,
    found: &mut Vec<String>,
    total_bytes: &mut u64,
) -> Result<(), BackendError> {
    for entry in fs::read_dir(current).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
        if metadata.file_type().is_symlink() || is_reparse(&metadata) {
            return Err(candidate_error("Links are forbidden in Agent output."));
        }
        if metadata.is_dir() {
            collect_output_files(root, &entry.path(), found, total_bytes)?;
        } else if metadata.is_file() {
            *total_bytes = total_bytes.saturating_add(metadata.len());
            found.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| candidate_error("Output escaped staging."))?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        } else {
            return Err(candidate_error(
                "Only regular files are accepted as Agent output.",
            ));
        }
    }
    Ok(())
}

fn single_source_relative(workspace: &Path) -> Result<String, BackendError> {
    let source = workspace.join("source");
    let mut files = Vec::new();
    let mut total_bytes = 0;
    collect_output_files(&source, &source, &mut files, &mut total_bytes)?;
    if files.len() != 1 {
        return Err(candidate_error(
            "Agent workspace must contain one source snapshot.",
        ));
    }
    Ok(format!("source/{}", files.remove(0)))
}

fn registry_markdown_views(
    context: &ProjectContext,
    files: &FileStore,
    normalized_locator: Option<&str>,
    hash: &str,
) -> Result<(Option<String>, Option<String>), BackendError> {
    let index: SourceIndex = super::source_registry::SourceRegistry::read_index(context, files)?;
    let pointer = index.by_content_hash.get(hash).or_else(|| {
        normalized_locator
            .and_then(|locator| index.by_locator.get(&locator.trim().replace('\\', "/")))
    });
    let Some(pointer) = pointer else {
        return Ok((None, None));
    };
    let manifest_path = format!(".app/sources/{}.json", pointer.source_id);
    if !files.exists(context, &manifest_path) {
        return Ok((None, None));
    }
    let manifest = SourceRegistry::read_manifest(context, files, &manifest_path)
        .map_err(|_| candidate_error("Source Registry manifest is malformed."))?;
    if manifest.source_id != pointer.source_id {
        return Err(candidate_error(
            "Source Registry pointer and manifest do not match.",
        ));
    }
    if !files.exists(context, &manifest.wiki_path) {
        return Ok((None, None));
    }
    let baseline = manifest
        .versions
        .iter()
        .find(|version| version.version_id == pointer.version_id)
        .filter(|version| files.exists(context, &version.baseline_path))
        .map(|version| read_project_text(context, &version.baseline_path))
        .transpose()?;
    let current = read_project_text(context, &manifest.wiki_path)?;
    Ok((baseline, Some(current)))
}

fn validate_deterministic_inputs(
    workspace: &Path,
    item: &ImportItem,
    bundle: &AgentTaskBundle,
) -> Result<(), BackendError> {
    let root = workspace.join("deterministic");
    let mut files = Vec::new();
    let mut total = 0;
    collect_output_files(&root, &root, &mut files, &mut total)?;
    let Some(preview) = &item.preview else {
        return if files.is_empty() {
            Ok(())
        } else {
            Err(candidate_error(
                "Hard-failure workspace contains an invented deterministic baseline.",
            ))
        };
    };
    let markdown = read_regular_file(&root.join("candidate.md"), MAX_OUTPUT_BYTES)?;
    if hash_bytes(&markdown) != preview.markdown.sha256
        || !bundle.input_hashes.contains(&preview.markdown.sha256)
    {
        return Err(candidate_error(
            "Deterministic baseline changed inside the Agent workspace.",
        ));
    }
    let mut actual_assets = Vec::new();
    for path in files.iter().filter(|path| path.as_str() != "candidate.md") {
        actual_assets.push(hash_bytes(&read_regular_file(
            &root.join(path),
            MAX_OUTPUT_BYTES,
        )?));
    }
    actual_assets.sort();
    let mut expected_assets: Vec<String> = preview
        .assets
        .iter()
        .map(|asset| asset.sha256.clone())
        .collect();
    expected_assets.sort();
    if actual_assets != expected_assets
        || expected_assets
            .iter()
            .any(|hash| !bundle.input_hashes.contains(hash))
    {
        return Err(candidate_error(
            "Deterministic assets changed inside the Agent workspace.",
        ));
    }
    Ok(())
}

fn prefix_artifact(artifact: &mut ImportArtifact, root: &str) {
    artifact.relative_path = format!("{root}/{}", artifact.relative_path);
}

fn write_sealed_file(path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    if path.exists() {
        return if read_regular_file(path, MAX_OUTPUT_BYTES)? == bytes {
            Ok(())
        } else {
            Err(candidate_error(
                "An immutable candidate artifact changed during recovery.",
            ))
        };
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    use std::io::Write;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(io_error)
}

fn read_project_text(context: &ProjectContext, relative: &str) -> Result<String, BackendError> {
    String::from_utf8(read_project_file(context, relative, MAX_OUTPUT_BYTES)?)
        .map_err(|_| candidate_error("Source Registry Markdown is not UTF-8."))
}

fn read_project_file(
    context: &ProjectContext,
    relative: &str,
    limit: u64,
) -> Result<Vec<u8>, BackendError> {
    let path = context.resolve_project_path(relative)?;
    reject_links_between(&context.root, &path)?;
    let canonical_root = context.root.canonicalize().map_err(io_error)?;
    let canonical = path.canonicalize().map_err(io_error)?;
    if !canonical.starts_with(canonical_root) {
        return Err(candidate_error("Source Registry path escaped the project."));
    }
    read_regular_file(&canonical, limit)
}

fn reject_links_between(root: &Path, target: &Path) -> Result<(), BackendError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| candidate_error("Path escaped the project."))?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        cursor.push(component);
        let metadata = fs::symlink_metadata(&cursor).map_err(io_error)?;
        if metadata.file_type().is_symlink() || is_reparse(&metadata) {
            return Err(candidate_error("Links and reparse points are forbidden."));
        }
    }
    Ok(())
}

fn safe_project_directory(
    context: &ProjectContext,
    relative: &str,
) -> Result<PathBuf, BackendError> {
    let path = context.resolve_project_path(relative)?;
    let root = context.root.canonicalize().map_err(io_error)?;
    let canonical = path.canonicalize().map_err(io_error)?;
    if !canonical.starts_with(root) || !canonical.is_dir() {
        return Err(candidate_error("Staged output escaped the project."));
    }
    Ok(canonical)
}

fn validate_workspace_identity(
    workspace: &Path,
    session_id: &str,
    item_id: &str,
) -> Result<(), BackendError> {
    let normalized = workspace.to_string_lossy().replace('\\', "/");
    let marker = format!("/.app/import-sessions/{session_id}/items/{item_id}/staging/agent/");
    if !normalized.contains(&marker) {
        return Err(candidate_error("Workspace belongs to another item."));
    }
    Ok(())
}

fn normalize_relative(value: &str) -> Result<String, BackendError> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.trim().is_empty()
        || normalized.contains(':')
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(candidate_error(
            "Agent output path is not a safe relative path.",
        ));
    }
    Ok(normalized)
}

fn read_json_limited<T: for<'de> Deserialize<'de>>(
    path: &Path,
    limit: u64,
) -> Result<T, BackendError> {
    serde_json::from_slice(&read_regular_file(path, limit)?)
        .map_err(|_| candidate_error("Agent manifest is malformed."))
}

fn read_regular_file(path: &Path, limit: u64) -> Result<Vec<u8>, BackendError> {
    let before = fs::symlink_metadata(path).map_err(io_error)?;
    if !before.is_file()
        || before.file_type().is_symlink()
        || is_reparse(&before)
        || before.len() > limit
    {
        return Err(candidate_error(
            "Candidate artifact is missing, linked, or too large.",
        ));
    }
    let mut file = fs::File::open(path).map_err(io_error)?;
    let opened = file.metadata().map_err(io_error)?;
    if !same_file(&before, &opened) {
        return Err(candidate_error("Candidate artifact changed before open."));
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    use std::io::Read;
    file.by_ref()
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    let after = fs::metadata(path).map_err(io_error)?;
    if bytes.len() as u64 > limit
        || bytes.len() as u64 != before.len()
        || !same_file(&opened, &after)
    {
        return Err(candidate_error(
            "Candidate artifact changed while it was read.",
        ));
    }
    Ok(bytes)
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(unix)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino() && left.len() == right.len()
}
#[cfg(windows)]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    left.creation_time() == right.creation_time()
        && left.last_write_time() == right.last_write_time()
        && left.file_size() == right.file_size()
}
#[cfg(not(any(unix, windows)))]
fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.len() == right.len() && left.modified().ok() == right.modified().ok()
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse(_: &fs::Metadata) -> bool {
    false
}

fn candidate_error(message: impl Into<String>) -> BackendError {
    BackendError::new("IMPORT_AGENT_CANDIDATE_INVALID", message, false, true)
}
fn io_error(error: std::io::Error) -> BackendError {
    BackendError::new(
        "IMPORT_AGENT_CANDIDATE_IO_FAILED",
        error.to_string(),
        true,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent::AgentKind;
    use crate::models::import_v2::{
        ArtifactKind, ImportArtifact, ImportInput, ImportInputKind, ImportItem,
        ImportPreviewArtifact, QualityLevel, QualityReport,
    };
    use crate::models::import_v2_agent::AgentToolGrant;

    fn manifest() -> AgentCandidateManifest {
        AgentCandidateManifest {
            markdown_path: "candidate.md".into(),
            asset_paths: Vec::new(),
            markdown_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
            asset_sha256: Default::default(),
            processing_summary: "AI-assisted extraction".into(),
            tools_used: vec!["tool-free-local-agent".into()],
            uncertainties: vec!["Formatting may differ.".into()],
            warnings: vec!["Review the Diff.".into()],
        }
    }

    fn audit(bundle: &AgentTaskBundle) -> AgentAuditRecord {
        AgentAuditRecord {
            audit_id: "audit-a".into(),
            task_id: "task-a".into(),
            session_id: "session-a".into(),
            item_id: "item-a".into(),
            trigger: bundle.trigger,
            route: "local/claude".into(),
            agent_kind: Some(AgentKind::Claude),
            agent_version: "1.0".into(),
            prompt_template_version: "import-recovery/local-v1".into(),
            approved_cost_micros: None,
            tool_calls: Vec::new(),
            approved_scope_sha256: None,
            workspace_relative_path:
                ".app/import-sessions/session-a/items/item-a/staging/agent/workspace-a".into(),
            granted_tools: bundle.allowed_tools.clone(),
            input_hashes: bundle.input_hashes.clone(),
            output_hashes: vec![manifest().markdown_sha256],
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            outcome: "succeeded".into(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn provenance_fails_closed_for_unknown_routes_and_tampering() {
        let bundle = AgentTaskBundle {
            schema_version: 1,
            session_id: "session-a".into(),
            item_id: "item-a".into(),
            trigger: crate::models::import_v2_agent::AgentAssistanceTrigger::Manual,
            public_source: "Example".into(),
            input_hashes: vec!["b".repeat(64)],
            allowed_tools: vec![AgentToolGrant::ValidateCandidate],
            required_outputs: Vec::new(),
            untrusted_source_material: Vec::new(),
        };
        let manifest = manifest();
        let valid = audit(&bundle);
        assert!(validate_candidate_audit(
            &valid,
            &bundle,
            &manifest,
            "session-a",
            "item-a",
            "task-a"
        )
        .is_ok());
        for tampered in [
            AgentAuditRecord {
                route: "plugin/arbitrary".into(),
                ..valid.clone()
            },
            AgentAuditRecord {
                prompt_template_version: "unknown".into(),
                ..valid.clone()
            },
            AgentAuditRecord {
                completed_at: None,
                ..valid.clone()
            },
            AgentAuditRecord {
                output_hashes: vec![manifest.markdown_sha256.clone(), "c".repeat(64)],
                ..valid.clone()
            },
        ] {
            assert!(validate_candidate_audit(
                &tampered,
                &bundle,
                &manifest,
                "session-a",
                "item-a",
                "task-a"
            )
            .is_err());
        }
    }

    #[test]
    fn rejects_incomplete_provenance_and_executable_assets() {
        let mut value = manifest();
        value.warnings.clear();
        assert!(validate_manifest(&value).is_err());
        let mut value = manifest();
        value.asset_paths.push("assets/payload.exe".into());
        assert!(validate_manifest(&value).is_err());
    }

    #[test]
    fn rejects_missing_and_unrelated_output_files() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("candidate.md"), "# Candidate").unwrap();
        assert!(validate_declared_output_tree(root.path(), &manifest()).is_err());
        fs::write(root.path().join("manifest.json"), "{}").unwrap();
        fs::write(root.path().join("unrelated.txt"), "not declared").unwrap();
        assert!(validate_declared_output_tree(root.path(), &manifest()).is_err());
    }

    #[test]
    fn rejects_traversal_and_duplicate_assets() {
        let mut value = manifest();
        value.markdown_path = "../candidate.md".into();
        assert!(validate_manifest(&value).is_err());
        let mut value = manifest();
        value.asset_paths = vec!["assets/a.png".into(), "assets/a.png".into()];
        assert!(validate_manifest(&value).is_err());
    }

    #[test]
    fn deterministic_baseline_hash_is_reverified_after_agent_run() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("deterministic")).unwrap();
        let baseline = b"# Faithful baseline\n";
        fs::write(root.path().join("deterministic/candidate.md"), baseline).unwrap();
        let baseline_hash = hash_bytes(baseline);
        let mut item = ImportItem::queued(
            "item-a",
            ImportInput {
                kind: ImportInputKind::Url,
                display_name: "Example".into(),
                locator: "https://example.com".into(),
                normalized_locator: Some("https://example.com/".into()),
                source_identity: None,
                media_save_mode: Default::default(),
            },
        );
        let artifact = ImportArtifact {
            kind: ArtifactKind::Markdown,
            relative_path: "staging/candidate.md".into(),
            sha256: baseline_hash.clone(),
            size_bytes: baseline.len() as u64,
        };
        item.preview = Some(ImportPreviewArtifact {
            markdown: artifact,
            assets: Vec::new(),
            source_snapshot: ImportArtifact {
                kind: ArtifactKind::SourceSnapshot,
                relative_path: "staging/source.bin".into(),
                sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                size_bytes: 1,
            },
            quality: QualityReport {
                level: QualityLevel::Pass,
                metrics: Vec::new(),
                warnings: Vec::new(),
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
            },
            title: "Example".into(),
            resolution: None,
            manual_merge: None,
        });
        let bundle = AgentTaskBundle {
            schema_version: 1,
            session_id: "session-a".into(),
            item_id: "item-a".into(),
            trigger: crate::models::import_v2_agent::AgentAssistanceTrigger::Manual,
            public_source: "Example".into(),
            input_hashes: vec![baseline_hash],
            allowed_tools: Vec::new(),
            required_outputs: Vec::new(),
            untrusted_source_material: Vec::new(),
        };
        assert!(validate_deterministic_inputs(root.path(), &item, &bundle).is_ok());
        fs::write(
            root.path().join("deterministic/candidate.md"),
            b"# Replaced baseline\n",
        )
        .unwrap();
        assert!(validate_deterministic_inputs(root.path(), &item, &bundle).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_output() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        symlink(outside.path(), root.path().join("candidate.md")).unwrap();
        fs::write(root.path().join("manifest.json"), "{}").unwrap();
        assert!(validate_declared_output_tree(root.path(), &manifest()).is_err());
    }
}
