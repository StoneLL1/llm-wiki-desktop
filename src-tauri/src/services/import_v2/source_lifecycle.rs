use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::BackendError;
use crate::models::git::CheckpointPurpose;
use crate::models::import_v2::{
    ImportInput, ImportInputKind, MediaSaveMode, QualityLevel, SourceFrontmatter, SourceIdentity,
};
use crate::models::paths::ProjectContext;
use crate::models::source::{
    ApplySourceCandidateRequest, DeleteSourcePreview, DeleteSourceRequest, MoveSourcePreview,
    PreviewDeleteSourceRequest, PreviewMoveSourceRequest, ReprocessSourceRequest,
    SourceAiOrganizeCandidateMeta, SourceAiOrganizeRoute, SourceArtifactSummary, SourceBinding,
    SourceCandidateKind, SourceCandidateSummary, SourceDetail, SourceEvidenceRetention,
    SourceMutationResult, SourcePrimaryAction, SourceStatus, SourceTechnicalDetails,
    SourceTimelineItem, SourceUpdateMode, SourceUpdatePreview, SourceVersionSummary,
};
use crate::models::source_package::{SourcePackageManifest, SourcePackageMemberRole};
use crate::models::wiki::{WikiPageType, WikiTree, WikiTreeNode};
use crate::services::import_v2::engine::{execute_engine, EngineOperation, EngineRequest};
use crate::services::import_v2::source_ai_organize::{
    self, SourceAiOrganizeInput, SourceAiTextEvidence, MAX_SOURCE_AI_EVIDENCE_BYTES,
    MAX_SOURCE_AI_MARKDOWN_BYTES, MAX_SOURCE_AI_MEDIA_REFERENCES,
    MAX_SOURCE_AI_MEDIA_REFERENCE_BYTES,
};
use crate::services::import_v2::source_finalization::{
    parse_final_source, render_source_markdown, validate_source_version_binding,
};
use crate::services::import_v2::source_registry::{
    SourceArtifactRecord, SourceCandidateRecord, SourceIndex, SourceManifest, SourcePointer,
    SourceProvenance, SourceRegistry, SourceTimelineEvent, SourceVersion,
};
use crate::services::import_v2::transaction::{read_project_file_nofollow, FileTransaction};
use crate::services::import_v2::url_policy::UrlPolicy;
use crate::services::import_v2::ImportV2Service;
use crate::services::{FileStore, GitService};
use crate::tasks::task_model::CancellationToken;
use crate::utils::markdown_utils::extract_wikilinks;
use crate::utils::markdown_utils::rewrite_wikilinks;
use crate::utils::path_utils::normalize_project_path;

const SOURCE_INDEX_PATH: &str = ".app/source-index-v2.json";
const MAX_SOURCE_PREVIEW_BYTES: usize = 256 * 1024;
const DELETE_CONFIRMATION_TEXT: &str = "永久删除此来源";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredSourceCandidate {
    schema_version: u32,
    candidate_id: String,
    source_id: String,
    base_version_id: String,
    base_markdown_hash: String,
    candidate_markdown_hash: String,
    kind: SourceCandidateKind,
    created_at: String,
    base_markdown: String,
    candidate_markdown: String,
    quality: crate::models::import_v2::QualityReport,
    #[serde(default)]
    processing_evidence: Vec<SourceArtifactRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ai_organize: Option<SourceAiOrganizeCandidateMeta>,
}

struct SourceProcessingOutput {
    markdown: String,
    evidence: Vec<(String, Vec<u8>)>,
}

#[derive(Debug, Clone)]
struct InventoryEntry {
    path: String,
    kind: String,
    size_bytes: u64,
    hash: String,
}

#[derive(Debug, Clone)]
struct LoadedSource {
    manifest_path: String,
    manifest_hash: String,
    manifest: SourceManifest,
    version: SourceVersion,
    current_markdown: Vec<u8>,
    current_hash: String,
    package: Option<SourcePackageManifest>,
}

struct AppliedSourceProvenance {
    route: String,
    engine_id: String,
    engine_version: String,
}

impl ImportV2Service {
    pub fn get_source_detail(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        source_id: &str,
    ) -> Result<SourceDetail, BackendError> {
        let loaded = load_source(context, files, source_id)?;
        let candidate = latest_candidate(context, files, source_id)?;
        let status = source_status(&loaded.version, candidate.is_some());
        let available_actions = available_actions(&loaded.manifest, &loaded.version);
        let primary_action = if candidate.is_some() {
            SourcePrimaryAction::ReviewCandidate
        } else if available_actions.contains(&SourceCandidateKind::Ocr) {
            SourcePrimaryAction::ReprocessOcr
        } else if available_actions.contains(&SourceCandidateKind::Asr) {
            SourcePrimaryAction::ReprocessAsr
        } else if available_actions.contains(&SourceCandidateKind::Refresh) {
            SourcePrimaryAction::RefreshSource
        } else {
            SourcePrimaryAction::None
        };
        let original_path = loaded
            .manifest
            .versions
            .first()
            .map(|version| version.baseline_path.as_str())
            .unwrap_or(loaded.version.baseline_path.as_str());
        let (original_draft, original_draft_truncated) =
            read_bounded_text(context, original_path, MAX_SOURCE_PREVIEW_BYTES)?;
        let evidence = loaded
            .version
            .raw_evidence
            .iter()
            .chain(loaded.version.assets.iter())
            .map(|artifact| SourceArtifactSummary {
                path: artifact.path.clone(),
                kind: artifact.kind.clone(),
                size_bytes: artifact.size_bytes,
            })
            .collect();
        let restorability = version_restorability(context, files, &loaded.manifest);
        let versions = version_summaries(&loaded.manifest, &restorability);
        let timeline = timeline_summaries(&loaded.manifest, &restorability);
        let related_wiki_paths =
            pages_referencing_source(context, &loaded.manifest, loaded.package.as_ref())?;
        Ok(SourceDetail {
            source_id: loaded.manifest.source_id.clone(),
            version_id: loaded.version.version_id.clone(),
            title: loaded.manifest.title.clone(),
            source_kind: loaded.manifest.source_kind.clone(),
            status,
            current_path: loaded.manifest.wiki_path.clone(),
            current_markdown_hash: loaded.current_hash,
            primary_action,
            candidate: candidate.as_ref().map(candidate_summary),
            target_path: loaded.manifest.wiki_path.clone(),
            evidence_retention: SourceEvidenceRetention::ImmutableOriginalsRetained,
            evidence,
            quality: loaded.version.quality.clone(),
            original_draft,
            original_draft_truncated,
            versions,
            timeline,
            related_wiki_paths,
            technical_details: SourceTechnicalDetails {
                route: loaded.version.provenance.route.clone(),
                engine: loaded.version.provenance.engine_id.clone(),
                engine_version: loaded.version.provenance.engine_version.clone(),
                locator: loaded.version.provenance.locator.clone(),
                manifest_path: loaded.manifest_path,
            },
            available_actions,
        })
    }

    pub fn list_source_versions(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        source_id: &str,
    ) -> Result<Vec<SourceVersionSummary>, BackendError> {
        let loaded = load_source(context, files, source_id)?;
        let restorability = version_restorability(context, files, &loaded.manifest);
        Ok(version_summaries(&loaded.manifest, &restorability))
    }

    pub fn reprocess_source(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        request: &ReprocessSourceRequest,
        kind: SourceCandidateKind,
        cancellation: &CancellationToken,
    ) -> Result<SourceCandidateSummary, BackendError> {
        let _guard = self.mutation_lock.lock().map_err(|_| source_busy())?;
        FileTransaction::reconcile_project(&context.root)?;
        let loaded = load_source(context, files, &request.source_id)?;
        if loaded.current_hash != request.expected_markdown_hash {
            return Err(source_changed(&loaded.current_hash));
        }
        if !available_actions(&loaded.manifest, &loaded.version).contains(&kind) {
            return Err(BackendError::new(
                "SOURCE_ACTION_UNAVAILABLE",
                "This processing action is not available for the current Source.",
                false,
                true,
            ));
        }
        let mut selected_subtitle = None;
        if kind == SourceCandidateKind::Subtitle {
            let subtitle = request
                .subtitle_path
                .as_deref()
                .filter(|path| !path.trim().is_empty())
                .ok_or_else(|| {
                    BackendError::new(
                        "SOURCE_SUBTITLE_REQUIRED",
                        "Choose a retained subtitle before creating a candidate.",
                        false,
                        true,
                    )
                })?;
            let normalized = normalize_project_path(subtitle);
            let allowed = loaded
                .version
                .raw_evidence
                .iter()
                .chain(loaded.version.assets.iter())
                .any(|artifact| artifact.path == normalized && artifact.kind.contains("subtitle"));
            if !allowed {
                return Err(BackendError::new(
                    "SOURCE_SUBTITLE_INVALID",
                    "The selected subtitle is not retained by this Source version.",
                    false,
                    true,
                ));
            }
            selected_subtitle = Some(normalized);
        }

        // Reprocessing executes the retained-original route into isolated
        // staging. Only the resulting candidate/evidence is persisted here;
        // the current Wiki file and registry remain untouched until Diff approval.
        let base_markdown = reliable_version_baseline(context, &loaded)?;
        let base_markdown_hash = digest(base_markdown.as_bytes());
        let processed = if kind == SourceCandidateKind::Subtitle {
            None
        } else {
            Some(execute_source_processing(
                self,
                context,
                &loaded,
                &kind,
                cancellation,
            )?)
        };
        let markdown = build_reprocess_candidate(
            context,
            &loaded,
            &base_markdown,
            &kind,
            selected_subtitle.as_deref(),
            processed.as_ref().map(|output| output.markdown.as_str()),
        )?;
        let candidate_id = uuid::Uuid::new_v4().to_string();
        let processing_evidence = processed
            .map(|output| output.evidence)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .map(|(index, (evidence_kind, bytes))| {
                let path =
                    candidate_evidence_path(&loaded.manifest.source_id, &candidate_id, index);
                (
                    SourceArtifactRecord {
                        path: path.clone(),
                        sha256: digest(&bytes),
                        size_bytes: bytes.len() as u64,
                        kind: evidence_kind,
                    },
                    bytes,
                )
            })
            .collect::<Vec<_>>();
        let candidate = StoredSourceCandidate {
            schema_version: 1,
            candidate_id: candidate_id.clone(),
            source_id: loaded.manifest.source_id,
            base_version_id: loaded.version.version_id,
            base_markdown_hash,
            candidate_markdown_hash: digest(markdown.as_bytes()),
            kind,
            created_at: chrono::Utc::now().to_rfc3339(),
            base_markdown,
            candidate_markdown: markdown,
            quality: loaded.version.quality,
            processing_evidence: processing_evidence
                .iter()
                .map(|(record, _)| record.clone())
                .collect(),
            ai_organize: None,
        };
        let path = candidate_path(&candidate.source_id, &candidate.candidate_id);
        let mut transaction = FileTransaction::new_for_project(&context.root);
        for (record, bytes) in processing_evidence {
            transaction.write_new(&context.resolve_project_path(&record.path)?, &bytes)?;
        }
        transaction.write_new(
            &context.resolve_project_path(&path)?,
            &pretty_json(&candidate)?,
        )?;
        transaction.commit()?;
        Ok(candidate_summary(&candidate))
    }

    pub fn prepare_source_ai_organize_input(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        source_id: &str,
        expected_version_id: &str,
        expected_markdown_hash: &str,
        custom_instructions: Option<&str>,
    ) -> Result<SourceAiOrganizeInput, BackendError> {
        let loaded = load_source(context, files, source_id)?;
        if loaded.version.version_id != expected_version_id
            || loaded.current_hash != expected_markdown_hash
        {
            return Err(source_changed(&loaded.current_hash));
        }
        if loaded.current_markdown.len() > MAX_SOURCE_AI_MARKDOWN_BYTES {
            return Err(BackendError::new(
                "SOURCE_AI_INPUT_TOO_LARGE",
                "This Source is too large for bounded AI organization input.",
                true,
                true,
            ));
        }
        let current_markdown =
            String::from_utf8(loaded.current_markdown.clone()).map_err(|_| source_invalid())?;
        let (_, body) = parse_final_source(&current_markdown).map_err(|_| source_invalid())?;
        let meaningful_chars = body
            .chars()
            .filter(|character| !character.is_whitespace() && !character.is_ascii_punctuation())
            .count();
        let processing_actions = available_actions(&loaded.manifest, &loaded.version);
        if meaningful_chars < 80
            && processing_actions
                .iter()
                .any(|kind| matches!(kind, SourceCandidateKind::Ocr | SourceCandidateKind::Asr))
        {
            let suggested_action = if processing_actions.contains(&SourceCandidateKind::Ocr) {
                "ocr"
            } else {
                "asr"
            };
            return Err(BackendError::new(
                "SOURCE_AI_CONTENT_INSUFFICIENT",
                "The Source does not contain enough readable text. Run OCR/ASR first, then organize it.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "suggestedAction": suggested_action })));
        }
        let mut retained_text_evidence = Vec::new();
        let mut evidence_bytes = 0usize;
        for artifact in loaded
            .version
            .raw_evidence
            .iter()
            .chain(loaded.version.assets.iter())
            .filter(|artifact| is_source_ai_text_evidence(&artifact.kind))
        {
            if evidence_bytes >= MAX_SOURCE_AI_EVIDENCE_BYTES {
                break;
            }
            let bytes = read_project_file_nofollow(
                &context.root,
                &context.resolve_project_path(&artifact.path)?,
            )
            .map_err(|_| source_invalid())?;
            if digest(&bytes) != artifact.sha256 {
                return Err(source_invalid());
            }
            let remaining = MAX_SOURCE_AI_EVIDENCE_BYTES - evidence_bytes;
            let bounded = &bytes[..bytes.len().min(remaining)];
            let Ok(text) = std::str::from_utf8(bounded) else {
                continue;
            };
            evidence_bytes += bounded.len();
            retained_text_evidence.push(SourceAiTextEvidence {
                kind: artifact.kind.clone(),
                path: artifact.path.clone(),
                text: text.to_string(),
            });
        }
        let mut media_references = loaded
            .version
            .raw_evidence
            .iter()
            .chain(loaded.version.assets.iter())
            .filter(|artifact| is_source_ai_image_reference(&artifact.kind))
            .map(|artifact| artifact.path.clone())
            .collect::<Vec<_>>();
        media_references.sort();
        media_references.dedup();
        media_references.truncate(MAX_SOURCE_AI_MEDIA_REFERENCES);
        let mut reference_bytes = 0usize;
        media_references.retain(|reference| {
            let next = reference_bytes.saturating_add(reference.len());
            if next > MAX_SOURCE_AI_MEDIA_REFERENCE_BYTES {
                return false;
            }
            reference_bytes = next;
            true
        });
        Ok(SourceAiOrganizeInput {
            source_id: loaded.manifest.source_id,
            version_id: loaded.version.version_id,
            markdown_hash: loaded.current_hash,
            title: loaded.manifest.title,
            source_kind: loaded.manifest.source_kind,
            author: loaded.manifest.author,
            published_at: loaded.manifest.published_at,
            language: loaded.manifest.language,
            current_markdown,
            retained_text_evidence,
            media_references,
            custom_instructions: source_ai_organize::validate_custom_instructions(
                custom_instructions,
            )?,
        })
    }

    pub fn store_source_ai_organize_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        input: &SourceAiOrganizeInput,
        task_id: &str,
        route: SourceAiOrganizeRoute,
        engine: String,
        model: String,
        engine_version: Option<String>,
        candidate_markdown: String,
    ) -> Result<SourceCandidateSummary, BackendError> {
        let _guard = self.mutation_lock.lock().map_err(|_| source_busy())?;
        FileTransaction::reconcile_project(&context.root)?;
        let loaded = load_source(context, files, &input.source_id)?;
        if digest(input.current_markdown.as_bytes()) != input.markdown_hash {
            return Err(source_invalid());
        }
        let base_version = loaded
            .manifest
            .versions
            .iter()
            .find(|version| version.version_id == input.version_id)
            .cloned()
            .ok_or_else(source_invalid)?;
        source_ai_organize::validate_exactly_one_overview(&candidate_markdown)?;
        let candidate_id = uuid::Uuid::new_v4().to_string();
        let candidate = StoredSourceCandidate {
            schema_version: 1,
            candidate_id: candidate_id.clone(),
            source_id: loaded.manifest.source_id,
            base_version_id: input.version_id.clone(),
            base_markdown_hash: input.markdown_hash.clone(),
            candidate_markdown_hash: digest(candidate_markdown.as_bytes()),
            kind: SourceCandidateKind::AiOrganize,
            created_at: chrono::Utc::now().to_rfc3339(),
            base_markdown: input.current_markdown.clone(),
            candidate_markdown,
            quality: base_version.quality,
            processing_evidence: Vec::new(),
            ai_organize: Some(SourceAiOrganizeCandidateMeta {
                task_id: task_id.to_string(),
                route,
                engine,
                model,
                engine_version,
            }),
        };
        let path = candidate_path(&candidate.source_id, &candidate.candidate_id);
        let mut superseded = Vec::new();
        let candidate_root = context
            .resolve_project_path(&format!(".app/source-candidates/{}", candidate.source_id))?;
        if candidate_root.exists() {
            for entry in fs::read_dir(&candidate_root).map_err(|_| source_invalid())? {
                let entry = entry.map_err(|_| source_invalid())?;
                let relative = context.to_project_relative(&entry.path())?;
                let existing: StoredSourceCandidate = files.read_json(context, &relative)?;
                validate_candidate(&existing, &candidate.source_id)?;
                if existing.kind == SourceCandidateKind::AiOrganize {
                    superseded.push(relative);
                }
            }
        }
        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.write_new(
            &context.resolve_project_path(&path)?,
            &pretty_json(&candidate)?,
        )?;
        for previous in superseded {
            let hash = files.file_hash(context, &previous)?;
            transaction.delete_if_hash_matches(&context.resolve_project_path(&previous)?, &hash)?;
        }
        transaction.commit()?;
        Ok(candidate_summary(&candidate))
    }

    pub fn discard_source_ai_organize_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        source_id: &str,
        candidate_id: &str,
        task_id: &str,
    ) -> Result<(), BackendError> {
        let _guard = self.mutation_lock.lock().map_err(|_| source_busy())?;
        let candidate = load_candidate(context, files, source_id, candidate_id)?;
        if candidate.kind != SourceCandidateKind::AiOrganize
            || candidate
                .ai_organize
                .as_ref()
                .map(|metadata| metadata.task_id.as_str())
                != Some(task_id)
        {
            return Err(source_invalid());
        }
        let path = candidate_path(source_id, candidate_id);
        let hash = files.file_hash(context, &path)?;
        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.delete_if_hash_matches(&context.resolve_project_path(&path)?, &hash)?;
        transaction.commit()?;
        remove_empty_tree(
            &context.resolve_project_path(&format!(".app/source-candidates/{source_id}"))?,
        );
        Ok(())
    }

    pub fn discard_source_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        source_id: &str,
        candidate_id: &str,
    ) -> Result<(), BackendError> {
        let _guard = self.mutation_lock.lock().map_err(|_| source_busy())?;
        FileTransaction::reconcile_project(&context.root)?;
        let candidate = load_candidate(context, files, source_id, candidate_id)?;
        let path = candidate_path(&candidate.source_id, &candidate.candidate_id);
        let hash = files.file_hash(context, &path)?;
        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.delete_if_hash_matches(&context.resolve_project_path(&path)?, &hash)?;
        transaction.commit()?;
        remove_empty_tree(
            &context.resolve_project_path(&format!(".app/source-candidates/{source_id}"))?,
        );
        Ok(())
    }

    pub fn preview_source_update(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        source_id: &str,
        candidate_id: &str,
    ) -> Result<SourceUpdatePreview, BackendError> {
        let loaded = load_source(context, files, source_id)?;
        let candidate = load_candidate(context, files, source_id, candidate_id)?;
        let current = String::from_utf8(loaded.current_markdown).map_err(|_| source_invalid())?;
        let mode = if loaded.version.version_id == candidate.base_version_id
            && loaded.current_hash == candidate.base_markdown_hash
        {
            SourceUpdateMode::TwoWay
        } else {
            SourceUpdateMode::ThreeWay
        };
        let guard_token =
            update_guard_token(&candidate, &loaded.manifest_hash, &loaded.current_hash);
        Ok(SourceUpdatePreview {
            source_id: source_id.to_string(),
            candidate_id: candidate_id.to_string(),
            mode,
            base_markdown: candidate.base_markdown.clone(),
            current_markdown: current.clone(),
            candidate_markdown: candidate.candidate_markdown.clone(),
            diff: render_line_diff(&current, &candidate.candidate_markdown),
            current_markdown_hash: loaded.current_hash,
            candidate_markdown_hash: candidate.candidate_markdown_hash.clone(),
            guard_token,
        })
    }

    pub fn apply_source_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        git: &GitService,
        request: &ApplySourceCandidateRequest,
    ) -> Result<SourceMutationResult, BackendError> {
        let _guard = self.mutation_lock.lock().map_err(|_| source_busy())?;
        FileTransaction::reconcile_project(&context.root)?;
        let loaded = load_source(context, files, &request.source_id)?;
        let candidate = load_candidate(context, files, &request.source_id, &request.candidate_id)?;
        let expected_guard =
            update_guard_token(&candidate, &loaded.manifest_hash, &loaded.current_hash);
        if request.guard_token != expected_guard {
            return Err(source_changed(&loaded.current_hash));
        }
        let current_changed = loaded.version.version_id != candidate.base_version_id
            || loaded.current_hash != candidate.base_markdown_hash;
        let selected_markdown = if current_changed {
            request
                .merged_markdown
                .as_deref()
                .filter(|markdown| !markdown.trim().is_empty())
                .ok_or_else(|| {
                    BackendError::new(
                        "SOURCE_THREE_WAY_MERGE_REQUIRED",
                        "The Source changed after this candidate was created. Review a three-way merge before applying it.",
                        true,
                        true,
                    )
                })?
        } else {
            request
                .merged_markdown
                .as_deref()
                .unwrap_or(&candidate.candidate_markdown)
        };
        if candidate.kind == SourceCandidateKind::AiOrganize {
            source_ai_organize::validate_exactly_one_overview(selected_markdown)?;
        }
        let processing_evidence = candidate
            .processing_evidence
            .iter()
            .map(|artifact| {
                read_project_file_nofollow(
                    &context.root,
                    &context.resolve_project_path(&artifact.path)?,
                )
                .map(|bytes| (artifact.kind.clone(), bytes))
                .map_err(|_| source_invalid())
            })
            .collect::<Result<Vec<_>, BackendError>>()?;
        let mut candidate_paths = candidate
            .processing_evidence
            .iter()
            .map(|artifact| artifact.path.clone())
            .collect::<Vec<_>>();
        candidate_paths.push(candidate_path(&request.source_id, &request.candidate_id));
        let provenance = candidate
            .ai_organize
            .as_ref()
            .map(|metadata| AppliedSourceProvenance {
                route: match metadata.route {
                    SourceAiOrganizeRoute::Agent => "source_ai_agent".into(),
                    SourceAiOrganizeRoute::Byok => "source_ai_byok".into(),
                },
                engine_id: metadata.engine.clone(),
                engine_version: metadata
                    .engine_version
                    .clone()
                    .unwrap_or_else(|| metadata.model.clone()),
            });
        apply_markdown_version(
            context,
            files,
            git,
            loaded,
            selected_markdown,
            candidate.kind.timeline_kind(),
            candidate_paths,
            processing_evidence,
            None,
            provenance,
        )
    }

    pub fn restore_source_version(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        git: &GitService,
        source_id: &str,
        version_id: &str,
        expected_markdown_hash: &str,
    ) -> Result<SourceMutationResult, BackendError> {
        let _guard = self.mutation_lock.lock().map_err(|_| source_busy())?;
        FileTransaction::reconcile_project(&context.root)?;
        let loaded = load_source(context, files, source_id)?;
        if loaded.current_hash != expected_markdown_hash {
            return Err(source_changed(&loaded.current_hash));
        }
        let selected = loaded
            .manifest
            .versions
            .iter()
            .find(|version| version.version_id == version_id)
            .cloned()
            .ok_or_else(source_not_found)?;
        if !version_is_restorable(context, files, &loaded.manifest, &selected) {
            return Err(BackendError::new(
                "SOURCE_VERSION_NOT_RESTORABLE",
                "This version does not have a reliable restorable snapshot.",
                false,
                true,
            ));
        }
        let package_snapshot =
            load_package_for_version(context, files, &loaded.manifest, &selected)?;
        let markdown = files.read_markdown(context, &selected.baseline_path)?;
        apply_markdown_version(
            context,
            files,
            git,
            loaded,
            &markdown,
            "version_restored",
            Vec::new(),
            Vec::new(),
            package_snapshot,
            None,
        )
    }

    pub fn preview_move_source(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        request: &PreviewMoveSourceRequest,
    ) -> Result<MoveSourcePreview, BackendError> {
        let loaded = load_source(context, files, &request.source_id)?;
        build_move_preview(context, files, &loaded, &request.new_wiki_path)
    }

    pub fn move_source(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        git: &GitService,
        request: &crate::models::source::MoveSourceRequest,
    ) -> Result<SourceMutationResult, BackendError> {
        let _guard = self.mutation_lock.lock().map_err(|_| source_busy())?;
        FileTransaction::reconcile_project(&context.root)?;
        let loaded = load_source(context, files, &request.source_id)?;
        let preview = build_move_preview(context, files, &loaded, &request.new_wiki_path)?;
        if preview.guard_token != request.guard_token {
            return Err(source_changed(&loaded.current_hash));
        }
        apply_source_move(context, files, git, loaded, &preview)
    }

    pub fn preview_delete_source(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        request: &PreviewDeleteSourceRequest,
    ) -> Result<DeleteSourcePreview, BackendError> {
        let loaded = load_source(context, files, &request.source_id)?;
        build_delete_preview(context, files, &loaded)
    }

    pub fn delete_source(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        git: &GitService,
        request: &DeleteSourceRequest,
    ) -> Result<SourceMutationResult, BackendError> {
        if request.confirmation_text != DELETE_CONFIRMATION_TEXT {
            return Err(BackendError::new(
                "SOURCE_DELETE_CONFIRMATION_INVALID",
                "Type the exact permanent-delete confirmation before continuing.",
                false,
                true,
            ));
        }
        let _guard = self.mutation_lock.lock().map_err(|_| source_busy())?;
        FileTransaction::reconcile_project(&context.root)?;
        let loaded = load_source(context, files, &request.source_id)?;
        let preview = build_delete_preview(context, files, &loaded)?;
        if preview.guard_token != request.guard_token {
            return Err(source_changed(&loaded.current_hash));
        }
        let inventory = source_inventory(context, files, &loaded)?;
        let affected_paths = inventory
            .iter()
            .map(|entry| entry.path.clone())
            .chain(std::iter::once(SOURCE_INDEX_PATH.to_string()))
            .collect::<Vec<_>>();
        let checkpoint = git.create_scoped_checkpoint(
            context,
            CheckpointPurpose::HighRiskOperation,
            "Before permanently deleting Source",
            &affected_paths,
        )?;
        let checkpoint_hash = checkpoint.commit_hash.clone();

        let mut next_index = SourceRegistry::read_index(context, files)?;
        next_index
            .by_content_hash
            .retain(|_, pointer| pointer.source_id != request.source_id);
        next_index
            .by_locator
            .retain(|_, pointer| pointer.source_id != request.source_id);
        let index_hash = files.file_hash(context, SOURCE_INDEX_PATH)?;
        let audit_path = format!(".app/source-audit/deletions/{}.json", uuid::Uuid::new_v4());
        let audit = serde_json::json!({
            "schemaVersion": 1,
            "sourceId": request.source_id,
            "title": loaded.manifest.title,
            "deletedAt": chrono::Utc::now().to_rfc3339(),
            "checkpoint": checkpoint_hash,
            "result": "succeeded"
        });

        let mut transaction = FileTransaction::new_for_project(&context.root);
        for entry in &inventory {
            transaction
                .delete_if_hash_matches(&context.resolve_project_path(&entry.path)?, &entry.hash)?;
        }
        transaction.write_if_hash_matches(
            &context.resolve_project_path(SOURCE_INDEX_PATH)?,
            &pretty_json(&next_index)?,
            &index_hash,
        )?;
        transaction.write_new(
            &context.resolve_project_path(&audit_path)?,
            &pretty_json(&audit)?,
        )?;
        transaction.commit()?;
        remove_empty_source_directories(context, &request.source_id);
        Ok(SourceMutationResult {
            source_id: request.source_id.clone(),
            version_id: loaded.version.version_id,
            wiki_path: loaded.manifest.wiki_path,
            checkpoint: checkpoint.commit_hash,
        })
    }
}

fn build_move_preview(
    context: &ProjectContext,
    files: &FileStore,
    loaded: &LoadedSource,
    requested_path: &str,
) -> Result<MoveSourcePreview, BackendError> {
    let new_wiki_path = normalize_project_path(requested_path.trim());
    validate_source_move_target(context, loaded, &new_wiki_path)?;
    if new_wiki_path == loaded.manifest.wiki_path {
        return Err(BackendError::new(
            "SOURCE_MOVE_UNCHANGED",
            "Choose a different Source path.",
            false,
            true,
        ));
    }
    let moves = source_move_paths(loaded, &new_wiki_path)?;
    for (_, destination) in &moves {
        if files.exists(context, destination) {
            return Err(BackendError::new(
                "FILE_ALREADY_EXISTS",
                "The destination Source path already exists.",
                false,
                true,
            )
            .with_details(serde_json::json!({ "path": destination })));
        }
    }
    let mut material = format!(
        "{}\n{}\n{}\n{}\n",
        loaded.manifest.source_id, loaded.manifest_hash, loaded.current_hash, new_wiki_path
    );
    let mut affected_paths = Vec::new();
    for (source, destination) in &moves {
        let hash = files.file_hash(context, source)?;
        material.push_str(source);
        material.push('\0');
        material.push_str(&hash);
        material.push('\0');
        material.push_str(destination);
        material.push('\n');
        affected_paths.push(source.clone());
        affected_paths.push(destination.clone());
    }
    affected_paths.push(loaded.manifest_path.clone());
    affected_paths.push(SOURCE_INDEX_PATH.into());
    affected_paths.sort();
    affected_paths.dedup();
    Ok(MoveSourcePreview {
        source_id: loaded.manifest.source_id.clone(),
        old_wiki_path: loaded.manifest.wiki_path.clone(),
        new_wiki_path,
        affected_paths,
        guard_token: digest(material.as_bytes()),
    })
}

fn validate_source_move_target(
    context: &ProjectContext,
    loaded: &LoadedSource,
    target: &str,
) -> Result<(), BackendError> {
    let resolved = context.resolve_project_path(target)?;
    if !target.starts_with("wiki/sources/")
        || !target.ends_with(".md")
        || resolved.strip_prefix(&context.wiki_dir).is_err()
        || target.split('/').any(|segment| {
            segment.is_empty()
                || segment == "."
                || segment == ".."
                || segment.contains(['\\', ':'])
                || segment.chars().any(char::is_control)
        })
        || loaded
            .package
            .as_ref()
            .is_some_and(|_| !target.ends_with("/index.md"))
    {
        return Err(BackendError::new(
            "SOURCE_MOVE_PATH_INVALID",
            "Source destinations must stay under wiki/sources and preserve the package layout.",
            false,
            true,
        ));
    }
    Ok(())
}

fn source_move_paths(
    loaded: &LoadedSource,
    new_entry_path: &str,
) -> Result<Vec<(String, String)>, BackendError> {
    let Some(package) = loaded.package.as_ref() else {
        return Ok(vec![(
            loaded.manifest.wiki_path.clone(),
            new_entry_path.to_string(),
        )]);
    };
    let new_root = Path::new(new_entry_path)
        .parent()
        .and_then(Path::to_str)
        .map(|path| path.replace('\\', "/"))
        .ok_or_else(source_invalid)?;
    let mut moves = Vec::new();
    for member in &package.members {
        let file_name = Path::new(&member.wiki_path)
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(source_invalid)?;
        let destination = if member.role == SourcePackageMemberRole::Index {
            new_entry_path.to_string()
        } else {
            format!("{new_root}/{file_name}")
        };
        moves.push((member.wiki_path.clone(), destination));
    }
    Ok(moves)
}

fn apply_source_move(
    context: &ProjectContext,
    files: &FileStore,
    git: &GitService,
    loaded: LoadedSource,
    preview: &MoveSourcePreview,
) -> Result<SourceMutationResult, BackendError> {
    let moves = source_move_paths(&loaded, &preview.new_wiki_path)?;
    let now = chrono::Utc::now().to_rfc3339();
    let version_id = uuid::Uuid::new_v4().to_string();
    let (_, body) = parse_final_source(&String::from_utf8_lossy(&loaded.current_markdown))
        .map_err(|_| source_invalid())?;
    let content_hash = digest(body.as_bytes());
    let frontmatter = SourceFrontmatter {
        page_type: crate::models::import_v2::SourcePageType::Source,
        source_id: loaded.manifest.source_id.clone(),
        version_id: version_id.clone(),
        source_kind: loaded.manifest.source_kind.clone(),
        title: loaded.manifest.title.clone(),
        imported_at: now.clone(),
        content_hash: content_hash.clone(),
        platform: loaded.manifest.platform.clone(),
        canonical_url: loaded.manifest.canonical_url.clone(),
        platform_content_id: loaded.manifest.platform_content_id.clone(),
        author: loaded.manifest.author.clone(),
        published_at: loaded.manifest.published_at.clone(),
        language: loaded.manifest.language.clone(),
        quality: loaded.version.quality.clone(),
        restricted: loaded.manifest.restricted_content,
    };
    let final_entry = render_source_markdown(&frontmatter, &body)?.into_bytes();
    let final_hash = digest(&final_entry);
    let baseline_path = format!(
        ".app/source-artifacts/{}/{}/baseline.md",
        loaded.manifest.source_id, version_id
    );
    let mut new_writes = vec![(baseline_path.clone(), final_entry.clone())];
    let mut raw_evidence = Vec::new();
    let mut assets = Vec::new();
    let mut next_package = loaded.package.clone();

    for artifact in &loaded.version.raw_evidence {
        if artifact.kind == "source_package_manifest" {
            continue;
        }
        let next_path =
            replace_version_segment(&artifact.path, &loaded.version.version_id, &version_id)?;
        let bytes = read_project_file_nofollow(
            &context.root,
            &context.resolve_project_path(&artifact.path)?,
        )
        .map_err(|_| source_invalid())?;
        if digest(&bytes) != artifact.sha256 {
            return Err(source_invalid());
        }
        raw_evidence.push(SourceArtifactRecord {
            path: next_path.clone(),
            sha256: artifact.sha256.clone(),
            size_bytes: bytes.len() as u64,
            kind: artifact.kind.clone(),
        });
        new_writes.push((next_path, bytes));
    }
    for artifact in &loaded.version.assets {
        let next_path =
            replace_version_segment(&artifact.path, &loaded.version.version_id, &version_id)?;
        let bytes = read_project_file_nofollow(
            &context.root,
            &context.resolve_project_path(&artifact.path)?,
        )
        .map_err(|_| source_invalid())?;
        if digest(&bytes) != artifact.sha256 {
            return Err(source_invalid());
        }
        assets.push(SourceArtifactRecord {
            path: next_path.clone(),
            sha256: artifact.sha256.clone(),
            size_bytes: bytes.len() as u64,
            kind: artifact.kind.clone(),
        });
        new_writes.push((next_path, bytes));
    }

    let move_map = moves.iter().cloned().collect::<BTreeMap<_, _>>();
    if let Some(package) = next_package.as_mut() {
        package.version_id = version_id.clone();
        package.entry_wiki_path = preview.new_wiki_path.clone();
        for member in &mut package.members {
            member.wiki_path = move_map
                .get(&member.wiki_path)
                .cloned()
                .ok_or_else(source_invalid)?;
            if member.role == SourcePackageMemberRole::Index {
                member.baseline_path = baseline_path.clone();
                member.content_hash = final_hash.clone();
                member.human_edit_hash = final_hash.clone();
            } else {
                let source_path = moves
                    .iter()
                    .find(|(_, destination)| destination == &member.wiki_path)
                    .map(|(source, _)| source)
                    .ok_or_else(source_invalid)?;
                let bytes = read_project_file_nofollow(
                    &context.root,
                    &context.resolve_project_path(source_path)?,
                )
                .map_err(|_| source_invalid())?;
                let hash = digest(&bytes);
                let file_name = Path::new(&member.wiki_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(source_invalid)?;
                let next_baseline = format!(
                    ".app/source-artifacts/{}/{}/package/{file_name}",
                    loaded.manifest.source_id, version_id
                );
                member.baseline_path = next_baseline.clone();
                member.content_hash = hash.clone();
                member.human_edit_hash = hash;
                new_writes.push((next_baseline, bytes));
            }
        }
        package.validate_committed().map_err(|_| source_invalid())?;
        let package_path = format!(
            "raw/sources/{}/{}/derived/source-package.json",
            loaded.manifest.source_id, version_id
        );
        let bytes = pretty_json(package)?;
        raw_evidence.push(SourceArtifactRecord {
            path: package_path.clone(),
            sha256: digest(&bytes),
            size_bytes: bytes.len() as u64,
            kind: "source_package_manifest".into(),
        });
        new_writes.push((package_path, bytes));
    }
    if raw_evidence.is_empty() {
        let raw_path = format!(
            "raw/sources/{}/{}/derived/source-move.md",
            loaded.manifest.source_id, version_id
        );
        raw_evidence.push(SourceArtifactRecord {
            path: raw_path.clone(),
            sha256: final_hash.clone(),
            size_bytes: final_entry.len() as u64,
            kind: "source_move_snapshot".into(),
        });
        new_writes.push((raw_path, final_entry.clone()));
    }

    let source_paths = moves
        .iter()
        .map(|(source, _)| source.clone())
        .collect::<HashSet<_>>();
    let mut reference_writes = Vec::<(String, String, Vec<u8>)>::new();
    for absolute in files.list_markdown_files(&context.wiki_dir)? {
        let relative = context.to_project_relative(&absolute)?;
        if source_paths.contains(&relative) {
            continue;
        }
        let original = fs::read_to_string(&absolute).map_err(|_| source_invalid())?;
        let mut rewritten = original.clone();
        for (source, destination) in &moves {
            let old_stem = Path::new(source)
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(source_invalid)?;
            let new_stem = Path::new(destination)
                .file_stem()
                .and_then(|value| value.to_str())
                .ok_or_else(source_invalid)?;
            let old_without_extension = source.strip_suffix(".md").unwrap_or(source);
            let new_without_extension = destination.strip_suffix(".md").unwrap_or(destination);
            let mut variants = vec![
                (old_stem, new_stem),
                (old_without_extension, new_without_extension),
                (
                    old_without_extension
                        .strip_prefix("wiki/")
                        .unwrap_or(old_without_extension),
                    new_without_extension
                        .strip_prefix("wiki/")
                        .unwrap_or(new_without_extension),
                ),
            ];
            if old_stem.eq_ignore_ascii_case("index") && new_stem.eq_ignore_ascii_case("index") {
                if let (Some(old_parent), Some(new_parent)) = (
                    Path::new(source)
                        .parent()
                        .and_then(Path::file_name)
                        .and_then(|value| value.to_str()),
                    Path::new(destination)
                        .parent()
                        .and_then(Path::file_name)
                        .and_then(|value| value.to_str()),
                ) {
                    variants.push((old_parent, new_parent));
                }
            }
            for (old_target, new_target) in variants {
                rewritten = rewrite_wikilinks(&rewritten, old_target, new_target).0;
            }
        }
        if rewritten != original {
            reference_writes.push((
                relative,
                digest(original.as_bytes()),
                rewritten.into_bytes(),
            ));
        }
    }

    let mut affected_paths = preview.affected_paths.clone();
    affected_paths.extend(reference_writes.iter().map(|(path, _, _)| path.clone()));
    affected_paths.extend(new_writes.iter().map(|(path, _)| path.clone()));
    affected_paths.sort();
    affected_paths.dedup();
    let checkpoint = git.create_scoped_checkpoint(
        context,
        CheckpointPurpose::HighRiskOperation,
        "Before moving Source",
        &affected_paths,
    )?;
    let mut next_manifest = loaded.manifest.clone();
    next_manifest.wiki_path = preview.new_wiki_path.clone();
    next_manifest.current_version_id = version_id.clone();
    next_manifest.versions.push(SourceVersion {
        version_id: version_id.clone(),
        content_hash: content_hash.clone(),
        raw_evidence,
        assets,
        baseline_path,
        candidate: SourceCandidateRecord {
            markdown_hash: final_hash.clone(),
            title: next_manifest.title.clone(),
            source_kind: next_manifest.source_kind.clone(),
            canonical_url: next_manifest.canonical_url.clone(),
            platform: next_manifest.platform.clone(),
            platform_content_id: next_manifest.platform_content_id.clone(),
            author: next_manifest.author.clone(),
            published_at: next_manifest.published_at.clone(),
            language: next_manifest.language.clone(),
        },
        provenance: SourceProvenance {
            locator: loaded.version.provenance.locator.clone(),
            route: "source_move".into(),
            engine_id: loaded.version.provenance.engine_id.clone(),
            engine_version: loaded.version.provenance.engine_version.clone(),
        },
        quality: loaded.version.quality.clone(),
        created_at: now.clone(),
        human_edit_hash: Some(final_hash),
        checkpoint: checkpoint.commit_hash.clone(),
    });
    SourceRegistry::validate_manifest_contract(&next_manifest)?;
    let mut next_index = SourceRegistry::read_index(context, files)?;
    repoint_source(&mut next_index, &next_manifest.source_id, &version_id);
    next_index.by_content_hash.insert(
        content_hash,
        SourcePointer {
            source_id: next_manifest.source_id.clone(),
            version_id: version_id.clone(),
        },
    );
    let index_hash = files.file_hash(context, SOURCE_INDEX_PATH)?;
    let mut transaction = FileTransaction::new_for_project(&context.root);
    for (path, bytes) in new_writes {
        transaction.write_new(&context.resolve_project_path(&path)?, &bytes)?;
    }
    for (source, destination) in &moves {
        let bytes = if source == &loaded.manifest.wiki_path {
            final_entry.clone()
        } else {
            read_project_file_nofollow(&context.root, &context.resolve_project_path(source)?)
                .map_err(|_| source_invalid())?
        };
        transaction.write_new(&context.resolve_project_path(destination)?, &bytes)?;
    }
    for (path, hash, bytes) in reference_writes {
        transaction.write_if_hash_matches(&context.resolve_project_path(&path)?, &bytes, &hash)?;
    }
    for (source, _) in &moves {
        let hash = files.file_hash(context, source)?;
        transaction.delete_if_hash_matches(&context.resolve_project_path(source)?, &hash)?;
    }
    transaction.write_if_hash_matches(
        &context.resolve_project_path(&loaded.manifest_path)?,
        &pretty_json(&next_manifest)?,
        &loaded.manifest_hash,
    )?;
    transaction.write_if_hash_matches(
        &context.resolve_project_path(SOURCE_INDEX_PATH)?,
        &pretty_json(&next_index)?,
        &index_hash,
    )?;
    transaction.commit()?;
    Ok(SourceMutationResult {
        source_id: next_manifest.source_id,
        version_id,
        wiki_path: next_manifest.wiki_path,
        checkpoint: checkpoint.commit_hash,
    })
}

pub fn apply_validated_source_bindings(
    context: &ProjectContext,
    files: &FileStore,
    tree: &mut WikiTree,
) -> Result<(), BackendError> {
    let bindings = validated_source_bindings(context, files)?;
    for page in &mut tree.pages {
        if page.page_type != WikiPageType::Source {
            continue;
        }
        if let Some(binding) = bindings.get(&normalize_project_path(&page.path)) {
            page.source_id = Some(binding.source_id.clone());
            page.version_id = Some(binding.version_id.clone());
            page.source_status = Some(binding.status);
            page.quality = Some(binding.quality.clone());
            page.source_binding = Some(binding.clone());
        }
    }
    let page_types: BTreeMap<String, WikiPageType> = tree
        .pages
        .iter()
        .map(|page| (page.path.clone(), page.page_type))
        .collect();
    apply_tree_binding_types(&mut tree.root, &page_types);
    Ok(())
}

pub fn apply_validated_page_binding(
    context: &ProjectContext,
    files: &FileStore,
    page: &mut crate::models::wiki::WikiPageContent,
) -> Result<(), BackendError> {
    if page.meta.page_type != WikiPageType::Source {
        return Ok(());
    }
    if let Some(binding) =
        validated_source_binding_for_page(context, files, &page.meta.path, &page.raw_markdown)?
    {
        page.meta.source_id = Some(binding.source_id.clone());
        page.meta.version_id = Some(binding.version_id.clone());
        page.meta.source_status = Some(binding.status);
        page.meta.quality = Some(binding.quality.clone());
        page.meta.source_binding = Some(binding);
    }
    Ok(())
}

pub fn reject_generic_source_path(
    context: &ProjectContext,
    files: &FileStore,
    relative_path: &str,
) -> Result<(), BackendError> {
    let normalized = normalize_project_path(relative_path.trim());
    if normalized.starts_with("wiki/sources/") {
        return Err(source_requires_dedicated_action());
    }
    let index = SourceRegistry::read_index(context, files)?;
    for source_id in source_ids(&index) {
        let manifest = SourceRegistry::read_manifest(context, files, &manifest_path(&source_id))?;
        if manifest.wiki_path == normalized {
            return Err(source_requires_dedicated_action());
        }
        if let Some(package) =
            load_package_for_version(context, files, &manifest, current_version(&manifest)?)?
        {
            if package
                .members
                .iter()
                .any(|member| member.wiki_path == normalized)
            {
                return Err(source_requires_dedicated_action());
            }
        }
    }
    Ok(())
}

pub fn reject_generic_source_create(
    relative_path: &str,
    page_type: Option<&str>,
    contents: Option<&str>,
) -> Result<(), BackendError> {
    let normalized = normalize_project_path(relative_path.trim());
    let declares_source = page_type.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "source" | "sources"
        )
    }) || contents.is_some_and(markdown_declares_source);
    if normalized.starts_with("wiki/sources/") || declares_source {
        return Err(source_requires_dedicated_action());
    }
    Ok(())
}

fn validated_source_bindings(
    context: &ProjectContext,
    files: &FileStore,
) -> Result<BTreeMap<String, SourceBinding>, BackendError> {
    let index = SourceRegistry::read_index(context, files)?;
    let mut bindings = BTreeMap::new();
    for source_id in source_ids(&index) {
        let loaded = match load_source_with_index(context, files, &index, &source_id) {
            Ok(loaded) => loaded,
            // A broken entry does not promote an arbitrary Markdown file to
            // Source mode. Other valid Sources remain readable.
            Err(_) => continue,
        };
        let candidate_ready = latest_candidate(context, files, &source_id)
            .ok()
            .flatten()
            .is_some();
        let status = source_status(&loaded.version, candidate_ready);
        bindings.insert(
            normalize_project_path(&loaded.manifest.wiki_path),
            SourceBinding {
                source_id: loaded.manifest.source_id,
                version_id: loaded.version.version_id,
                status,
                quality: loaded.version.quality,
            },
        );
    }
    Ok(bindings)
}

fn validated_source_binding_for_page(
    context: &ProjectContext,
    files: &FileStore,
    page_path: &str,
    raw_markdown: &str,
) -> Result<Option<SourceBinding>, BackendError> {
    let (frontmatter, _) = match parse_final_source(raw_markdown) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(None),
    };
    if !safe_id(&frontmatter.source_id) || !safe_id(&frontmatter.version_id) {
        return Ok(None);
    }

    let index = SourceRegistry::read_index(context, files)?;
    let Some(pointer) = index.by_content_hash.get(&frontmatter.content_hash) else {
        return Ok(None);
    };
    if pointer.source_id != frontmatter.source_id || pointer.version_id != frontmatter.version_id {
        return Ok(None);
    }

    let manifest =
        match SourceRegistry::read_manifest(context, files, &manifest_path(&frontmatter.source_id))
        {
            Ok(manifest) => manifest,
            Err(_) => return Ok(None),
        };
    if manifest.source_id != frontmatter.source_id
        || manifest.current_version_id != frontmatter.version_id
        || normalize_project_path(&manifest.wiki_path) != normalize_project_path(page_path)
    {
        return Ok(None);
    }
    let version = match current_version(&manifest) {
        Ok(version) => version,
        Err(_) => return Ok(None),
    };
    if validate_source_version_binding(raw_markdown.as_bytes(), &manifest, version).is_err() {
        return Ok(None);
    }
    if load_package_for_version(context, files, &manifest, version).is_err() {
        return Ok(None);
    }

    let candidate_ready = latest_candidate(context, files, &manifest.source_id)
        .ok()
        .flatten()
        .is_some();
    Ok(Some(SourceBinding {
        source_id: manifest.source_id.clone(),
        version_id: version.version_id.clone(),
        status: source_status(version, candidate_ready),
        quality: version.quality.clone(),
    }))
}

fn apply_tree_binding_types(node: &mut WikiTreeNode, page_types: &BTreeMap<String, WikiPageType>) {
    if node.kind == crate::models::wiki::WikiTreeNodeKind::File {
        if let Some(page_type) = page_types.get(&node.path) {
            node.page_type = Some(*page_type);
        }
        return;
    }
    for child in &mut node.children {
        apply_tree_binding_types(child, page_types);
    }
}

fn load_source(
    context: &ProjectContext,
    files: &FileStore,
    source_id: &str,
) -> Result<LoadedSource, BackendError> {
    let index = SourceRegistry::read_index(context, files)?;
    load_source_with_index(context, files, &index, source_id)
}

fn load_source_with_index(
    context: &ProjectContext,
    files: &FileStore,
    index: &SourceIndex,
    source_id: &str,
) -> Result<LoadedSource, BackendError> {
    if !safe_id(source_id) {
        return Err(source_not_found());
    }
    let manifest_path = manifest_path(source_id);
    let manifest = SourceRegistry::read_manifest(context, files, &manifest_path)?;
    if manifest.source_id != source_id {
        return Err(source_not_found());
    }
    let version = current_version(&manifest)?.clone();
    let indexed = index
        .by_content_hash
        .get(&version.content_hash)
        .is_some_and(|pointer| {
            pointer.source_id == manifest.source_id && pointer.version_id == version.version_id
        });
    if !indexed {
        return Err(source_invalid());
    }
    let current_path = context.resolve_project_path(&manifest.wiki_path)?;
    let current_markdown =
        read_project_file_nofollow(&context.root, &current_path).map_err(|_| source_invalid())?;
    validate_source_version_binding(&current_markdown, &manifest, &version)
        .map_err(|_| source_invalid())?;
    let current_hash = digest(&current_markdown);
    let manifest_hash = files.file_hash(context, &manifest_path)?;
    let package = load_package_for_version(context, files, &manifest, &version)?;
    Ok(LoadedSource {
        manifest_path,
        manifest_hash,
        manifest,
        version,
        current_markdown,
        current_hash,
        package,
    })
}

fn current_version(manifest: &SourceManifest) -> Result<&SourceVersion, BackendError> {
    manifest
        .versions
        .iter()
        .find(|version| version.version_id == manifest.current_version_id)
        .ok_or_else(source_invalid)
}

fn reliable_version_baseline(
    context: &ProjectContext,
    loaded: &LoadedSource,
) -> Result<String, BackendError> {
    let bytes = read_project_file_nofollow(
        &context.root,
        &context.resolve_project_path(&loaded.version.baseline_path)?,
    )
    .map_err(|_| source_invalid())?;
    validate_source_version_binding(&bytes, &loaded.manifest, &loaded.version)
        .map_err(|_| source_invalid())?;
    if !loaded
        .version
        .human_edit_hash
        .as_deref()
        .is_some_and(|hash| hash == digest(&bytes))
    {
        return Err(source_invalid());
    }
    String::from_utf8(bytes).map_err(|_| source_invalid())
}

fn load_package_for_version(
    context: &ProjectContext,
    files: &FileStore,
    manifest: &SourceManifest,
    version: &SourceVersion,
) -> Result<Option<SourcePackageManifest>, BackendError> {
    let Some(record) = version
        .raw_evidence
        .iter()
        .find(|record| record.kind == "source_package_manifest")
    else {
        return Ok(None);
    };
    if files.file_hash(context, &record.path)? != record.sha256 {
        return Err(source_invalid());
    }
    let package: SourcePackageManifest = files.read_json(context, &record.path)?;
    package.validate_committed().map_err(|_| source_invalid())?;
    if package.source_id != manifest.source_id
        || package.version_id != version.version_id
        || package.entry_wiki_path
            != if version.version_id == manifest.current_version_id {
                manifest.wiki_path.as_str()
            } else {
                package.entry_wiki_path.as_str()
            }
    {
        return Err(source_invalid());
    }
    Ok(Some(package))
}

fn available_actions(
    manifest: &SourceManifest,
    version: &SourceVersion,
) -> Vec<SourceCandidateKind> {
    let mut actions = Vec::new();
    let kind = manifest.source_kind.to_ascii_lowercase();
    let evidence_kinds = version
        .raw_evidence
        .iter()
        .chain(version.assets.iter())
        .map(|artifact| artifact.kind.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let snapshot_extension = version
        .raw_evidence
        .iter()
        .find(|artifact| artifact.kind == "source_snapshot")
        .and_then(|artifact| Path::new(&artifact.path).extension())
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    let ocr_input_retained = snapshot_extension.as_deref().is_some_and(|extension| {
        matches!(
            extension,
            "pdf" | "avif" | "bmp" | "gif" | "jpeg" | "jpg" | "png" | "tif" | "tiff" | "webp"
        )
    });
    let asr_input_retained = snapshot_extension.as_deref().is_some_and(|extension| {
        matches!(
            extension,
            "aac"
                | "flac"
                | "m4a"
                | "mka"
                | "mp3"
                | "ogg"
                | "opus"
                | "wav"
                | "avi"
                | "m4v"
                | "mkv"
                | "mov"
                | "mp4"
                | "mpeg"
                | "mpg"
                | "webm"
        )
    });
    if ocr_input_retained
        && (kind.contains("image")
            || kind.contains("scan")
            || kind.contains("pdf")
            || evidence_kinds.iter().any(|value| value.contains("ocr")))
    {
        actions.push(SourceCandidateKind::Ocr);
    }
    if asr_input_retained
        && (kind.contains("audio")
            || kind.contains("video")
            || evidence_kinds
                .iter()
                .any(|value| value.contains("transcript") || value.contains("subtitle")))
    {
        actions.push(SourceCandidateKind::Asr);
    }
    if evidence_kinds
        .iter()
        .any(|value| value.contains("subtitle"))
    {
        actions.push(SourceCandidateKind::Subtitle);
    }
    if manifest.canonical_url.is_some() || manifest.platform.is_some() {
        actions.push(SourceCandidateKind::Refresh);
    }
    actions
}

fn is_source_ai_text_evidence(kind: &str) -> bool {
    let kind = kind.to_ascii_lowercase();
    ["ocr", "transcript", "subtitle", "caption"]
        .iter()
        .any(|needle| kind.contains(needle))
}

fn is_source_ai_image_reference(kind: &str) -> bool {
    let kind = kind.to_ascii_lowercase();
    kind == "image"
        || kind == "cover"
        || kind == "figure"
        || kind == "screenshot"
        || kind.contains("image_")
        || kind.ends_with("_image")
}

fn execute_source_processing(
    service: &ImportV2Service,
    context: &ProjectContext,
    loaded: &LoadedSource,
    kind: &SourceCandidateKind,
    cancellation: &CancellationToken,
) -> Result<SourceProcessingOutput, BackendError> {
    if cancellation.is_cancelled() {
        return Err(source_processing_cancelled());
    }
    let job_id = uuid::Uuid::new_v4().to_string();
    let staging_relative = format!(".app/import-staging/source-reprocess-{job_id}");
    let staging = context.resolve_project_path(&staging_relative)?;
    fs::create_dir_all(&staging).map_err(|_| source_invalid())?;
    let staging_guard = SourceProcessingStaging::new(context, staging.clone())?;
    let mut web_reference = None;
    let result = (|| {
        let (route, input) = match kind {
            SourceCandidateKind::Refresh => {
                let canonical_url = loaded
                    .manifest
                    .canonical_url
                    .as_deref()
                    .ok_or_else(source_action_unavailable)?;
                let target = UrlPolicy.normalize_for_session(canonical_url)?;
                let public_url = target.public.public_url.to_string();
                let reference = service.web_targets.store(&target)?;
                web_reference = Some(reference.clone());
                (
                    loaded.version.provenance.route.clone(),
                    ImportInput {
                        kind: ImportInputKind::Url,
                        display_name: loaded.manifest.title.clone(),
                        locator: reference,
                        normalized_locator: Some(public_url),
                        source_identity: None,
                        media_save_mode: MediaSaveMode::ExtractOnly,
                    },
                )
            }
            SourceCandidateKind::Ocr | SourceCandidateKind::Asr => {
                let snapshot = loaded
                    .version
                    .raw_evidence
                    .iter()
                    .find(|artifact| artifact.kind == "source_snapshot")
                    .ok_or_else(source_action_unavailable)?;
                let absolute = context.resolve_project_path(&snapshot.path)?;
                let bytes = read_project_file_nofollow(&context.root, &absolute)
                    .map_err(|_| source_invalid())?;
                if digest(&bytes) != snapshot.sha256 {
                    return Err(source_invalid());
                }
                let identity = source_identity(&absolute, &bytes)?;
                let route = if *kind == SourceCandidateKind::Asr {
                    "media.asr".to_string()
                } else {
                    select_ocr_route(service, loaded, &absolute)?
                };
                let display_name = absolute
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("source")
                    .to_string();
                (
                    route,
                    ImportInput {
                        kind: ImportInputKind::File,
                        display_name,
                        locator: identity.canonical_path.clone(),
                        normalized_locator: None,
                        source_identity: Some(identity),
                        media_save_mode: MediaSaveMode::ExtractOnly,
                    },
                )
            }
            SourceCandidateKind::Subtitle | SourceCandidateKind::AiOrganize => {
                return Err(source_action_unavailable())
            }
        };
        let engine = service.engines.resolve_route(&route, &input)?;
        let request = EngineRequest {
            protocol_version: "import-v2".into(),
            request_id: job_id.clone(),
            project_id: context.project_id.clone(),
            session_id: format!("source-reprocess-{}", loaded.manifest.source_id),
            item_id: loaded.manifest.source_id.clone(),
            task_id: job_id.clone(),
            operation: EngineOperation::Extract,
            input,
            project_root: context.root.to_string_lossy().into_owned(),
            staging_root: staging_relative.clone(),
            chained_input: None,
            local_asr_authorized: *kind == SourceCandidateKind::Asr,
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: loaded.manifest.language.clone(),
            selected_subtitle: None,
            local_ocr_authorized: *kind == SourceCandidateKind::Ocr,
            media_save_mode: MediaSaveMode::ExtractOnly,
        };
        let output = execute_engine(engine.as_ref(), &request, cancellation)?;
        let markdown_path = safe_staging_output(&staging, &output.markdown_path)?;
        let markdown_bytes = read_project_file_nofollow(&context.root, &markdown_path)
            .map_err(|_| source_invalid())?;
        if markdown_bytes.is_empty() || markdown_bytes.len() > 32 * 1024 * 1024 {
            return Err(source_invalid());
        }
        let markdown = String::from_utf8(markdown_bytes).map_err(|_| source_invalid())?;
        let mut evidence = Vec::new();
        let mut evidence_bytes = 0usize;
        for asset in output.asset_paths {
            let path = safe_staging_output(&staging, &asset)?;
            let bytes =
                read_project_file_nofollow(&context.root, &path).map_err(|_| source_invalid())?;
            evidence_bytes = evidence_bytes
                .checked_add(bytes.len())
                .ok_or_else(source_invalid)?;
            if evidence_bytes > 64 * 1024 * 1024 {
                return Err(source_invalid());
            }
            evidence.push((processing_evidence_kind(kind, &asset), bytes));
        }
        Ok(SourceProcessingOutput { markdown, evidence })
    })();
    if let Some(reference) = web_reference {
        let _ = service.web_targets.delete(&reference);
    }
    drop(staging_guard);
    result
}

fn processing_evidence_kind(kind: &SourceCandidateKind, path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.contains("subtitle") {
        "subtitle".into()
    } else if lower.contains("transcript") || *kind == SourceCandidateKind::Asr {
        "transcript".into()
    } else if *kind == SourceCandidateKind::Ocr {
        "ocr_output".into()
    } else if *kind == SourceCandidateKind::AiOrganize {
        "ai_organize".into()
    } else {
        "refresh_evidence".into()
    }
}

fn select_ocr_route(
    service: &ImportV2Service,
    loaded: &LoadedSource,
    source: &Path,
) -> Result<String, BackendError> {
    let routes = service.engines.registered_routes()?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let cjk = loaded.manifest.language.as_deref().is_some_and(|language| {
        let language = language.to_ascii_lowercase();
        language.starts_with("zh") || language.starts_with("ja") || language.starts_with("ko")
    });
    if cjk && extension != "pdf" && routes.iter().any(|route| route == "ocr.cjk-accurate") {
        return Ok("ocr.cjk-accurate".into());
    }
    if routes.iter().any(|route| route == "ocr.basic") {
        return Ok("ocr.basic".into());
    }
    if extension != "pdf" && routes.iter().any(|route| route == "ocr.cjk-accurate") {
        return Ok("ocr.cjk-accurate".into());
    }
    Err(source_action_unavailable())
}

fn source_identity(path: &Path, bytes: &[u8]) -> Result<SourceIdentity, BackendError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| source_invalid())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(source_invalid());
    }
    let canonical = path.canonicalize().map_err(|_| source_invalid())?;
    let modified_nanos = metadata.modified().ok().and_then(|value| {
        value
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_nanos())
    });
    Ok(SourceIdentity {
        canonical_path: canonical.to_string_lossy().into_owned(),
        size_bytes: bytes.len() as u64,
        modified_nanos,
        file_id: None,
        sha256: digest(bytes),
        magic: digest(&bytes[..bytes.len().min(8192)]),
    })
}

fn safe_staging_output(staging: &Path, relative: &str) -> Result<PathBuf, BackendError> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(source_invalid());
    }
    Ok(staging.join(relative))
}

struct SourceProcessingStaging {
    root: PathBuf,
    path: PathBuf,
}

impl SourceProcessingStaging {
    fn new(context: &ProjectContext, path: PathBuf) -> Result<Self, BackendError> {
        let root = context.resolve_project_path(".app/import-staging")?;
        if path.strip_prefix(&root).is_err() || path == root {
            return Err(source_invalid());
        }
        Ok(Self { root, path })
    }
}

impl Drop for SourceProcessingStaging {
    fn drop(&mut self) {
        if self.path != self.root && self.path.strip_prefix(&self.root).is_ok() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn build_reprocess_candidate(
    context: &ProjectContext,
    loaded: &LoadedSource,
    base_markdown: &str,
    kind: &SourceCandidateKind,
    selected_subtitle: Option<&str>,
    processed_markdown: Option<&str>,
) -> Result<String, BackendError> {
    if *kind == SourceCandidateKind::Refresh {
        let refreshed = processed_markdown
            .filter(|markdown| !markdown.trim().is_empty())
            .ok_or_else(source_invalid)?;
        let refreshed = crate::utils::markdown_utils::split_frontmatter(refreshed);
        let body = refreshed.body.trim();
        if body.is_empty() {
            return Err(source_invalid());
        }
        let (mut frontmatter, _) =
            parse_final_source(base_markdown).map_err(|_| source_invalid())?;
        frontmatter.content_hash = digest(body.as_bytes());
        return render_source_markdown(&frontmatter, body);
    }
    let retained_artifact = match kind {
        SourceCandidateKind::Subtitle => selected_subtitle.and_then(|selected| {
            loaded
                .version
                .raw_evidence
                .iter()
                .chain(loaded.version.assets.iter())
                .find(|artifact| artifact.path == selected)
        }),
        SourceCandidateKind::Ocr | SourceCandidateKind::Asr => None,
        SourceCandidateKind::Refresh | SourceCandidateKind::AiOrganize => None,
    };
    let recognized = if let Some(artifact) = retained_artifact {
        let bytes = read_project_file_nofollow(
            &context.root,
            &context.resolve_project_path(&artifact.path)?,
        )
        .map_err(|_| source_invalid())?;
        if digest(&bytes) != artifact.sha256 || bytes.len() > 16 * 1024 * 1024 {
            return Err(source_invalid());
        }
        let extension = Path::new(&artifact.path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "srt" | "vtt" | "ass" | "ssa" | "lrc") {
            crate::services::import_v2::subtitle::render_subtitle_markdown(&bytes, &extension)
                .ok_or_else(source_invalid)?
        } else {
            String::from_utf8(bytes).map_err(|_| source_invalid())?
        }
    } else {
        processed_markdown
            .filter(|markdown| !markdown.trim().is_empty())
            .ok_or_else(source_invalid)?
            .to_string()
    };
    if recognized.trim().is_empty() {
        return Err(source_invalid());
    }
    let (mut frontmatter, _) = parse_final_source(base_markdown).map_err(|_| source_invalid())?;
    let section = match kind {
        SourceCandidateKind::Ocr => "Recognized text",
        SourceCandidateKind::Asr | SourceCandidateKind::Subtitle => "Transcript",
        SourceCandidateKind::Refresh | SourceCandidateKind::AiOrganize => unreachable!(),
    };
    let body = format!(
        "# {}\n\n## {section}\n\n{}\n",
        loaded.manifest.title,
        recognized.trim()
    );
    frontmatter.content_hash = digest(body.as_bytes());
    render_source_markdown(&frontmatter, &body)
}

fn source_status(version: &SourceVersion, candidate_ready: bool) -> SourceStatus {
    if candidate_ready {
        SourceStatus::CandidateReady
    } else if version.quality.level == QualityLevel::Fail || !version.quality.warnings.is_empty() {
        SourceStatus::NeedsAttention
    } else {
        SourceStatus::Current
    }
}

fn version_restorability(
    context: &ProjectContext,
    files: &FileStore,
    manifest: &SourceManifest,
) -> BTreeMap<String, bool> {
    manifest
        .versions
        .iter()
        .map(|version| {
            (
                version.version_id.clone(),
                version_is_restorable(context, files, manifest, version),
            )
        })
        .collect()
}

fn version_summaries(
    manifest: &SourceManifest,
    restorability: &BTreeMap<String, bool>,
) -> Vec<SourceVersionSummary> {
    manifest
        .versions
        .iter()
        .rev()
        .map(|version| SourceVersionSummary {
            version_id: version.version_id.clone(),
            created_at: version.created_at.clone(),
            event_kind: timeline_kind_for_version(manifest, &version.version_id),
            quality: version.quality.clone(),
            current: version.version_id == manifest.current_version_id,
            restorable: restorability
                .get(&version.version_id)
                .copied()
                .unwrap_or(false),
            checkpoint: version.checkpoint.clone(),
        })
        .collect()
}

fn timeline_summaries(
    manifest: &SourceManifest,
    restorability: &BTreeMap<String, bool>,
) -> Vec<SourceTimelineItem> {
    manifest
        .timeline
        .iter()
        .filter_map(|event| {
            let kind = product_timeline_kind(&event.kind)?;
            let restorable = event
                .version_id
                .as_deref()
                .and_then(|version_id| restorability.get(version_id))
                .copied()
                .unwrap_or(false);
            Some(SourceTimelineItem {
                event_id: event.event_id.clone(),
                kind: kind.into(),
                version_id: event.version_id.clone(),
                created_at: event.created_at.clone(),
                checkpoint: event.checkpoint.clone(),
                restorable,
            })
        })
        .rev()
        .collect()
}

fn product_timeline_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "imported" => Some("source_imported"),
        "ocr_reprocessed" | "asr_reprocessed" => Some("ocr_asr_reprocessed"),
        "ai_organized" | "ai_organize_applied" => Some("ai_organize_applied"),
        "manual_checkpoint" => Some("manual_checkpoint"),
        "version_added" | "source_refreshed" => Some("source_refreshed"),
        "version_restored" => Some("version_restored"),
        _ => None,
    }
}

fn timeline_kind_for_version(manifest: &SourceManifest, version_id: &str) -> String {
    manifest
        .timeline
        .iter()
        .rev()
        .find(|event| event.version_id.as_deref() == Some(version_id))
        .and_then(|event| product_timeline_kind(&event.kind))
        .unwrap_or("source_imported")
        .to_string()
}

fn version_is_restorable(
    context: &ProjectContext,
    files: &FileStore,
    manifest: &SourceManifest,
    version: &SourceVersion,
) -> bool {
    let Ok(baseline_path) = context.resolve_project_path(&version.baseline_path) else {
        return false;
    };
    let Ok(bytes) = read_project_file_nofollow(&context.root, &baseline_path) else {
        return false;
    };
    if validate_source_version_binding(&bytes, manifest, version).is_err() {
        return false;
    }
    let package_members_reliable = match load_package_for_version(context, files, manifest, version)
    {
        Ok(Some(package)) => package
            .members
            .iter()
            .filter(|member| member.role != SourcePackageMemberRole::Index)
            .all(|member| {
                files
                    .file_hash(context, &member.baseline_path)
                    .is_ok_and(|hash| hash == member.content_hash)
            }),
        Ok(None) => true,
        Err(_) => false,
    };
    package_members_reliable
        && version
            .human_edit_hash
            .as_deref()
            .is_some_and(|hash| hash == digest(&bytes))
        && version.raw_evidence.iter().all(|artifact| {
            files
                .file_hash(context, &artifact.path)
                .is_ok_and(|hash| hash == artifact.sha256)
        })
        && version.assets.iter().all(|artifact| {
            files
                .file_hash(context, &artifact.path)
                .is_ok_and(|hash| hash == artifact.sha256)
        })
}

fn candidate_summary(candidate: &StoredSourceCandidate) -> SourceCandidateSummary {
    SourceCandidateSummary {
        candidate_id: candidate.candidate_id.clone(),
        kind: candidate.kind.clone(),
        created_at: candidate.created_at.clone(),
        base_version_id: candidate.base_version_id.clone(),
        base_markdown_hash: candidate.base_markdown_hash.clone(),
        candidate_markdown_hash: candidate.candidate_markdown_hash.clone(),
        quality: candidate.quality.clone(),
        ai_organize: candidate.ai_organize.clone(),
    }
}

fn normalize_ai_organize_metadata(metadata: &mut Option<SourceAiOrganizeCandidateMeta>) {
    let Some(metadata) = metadata else {
        return;
    };
    if metadata.route == SourceAiOrganizeRoute::Agent
        && metadata.engine_version.is_none()
        && metadata.model != "cli-default"
    {
        metadata.engine_version = Some(std::mem::replace(
            &mut metadata.model,
            "cli-default".to_string(),
        ));
    }
}

fn latest_candidate(
    context: &ProjectContext,
    files: &FileStore,
    source_id: &str,
) -> Result<Option<StoredSourceCandidate>, BackendError> {
    let root = context.resolve_project_path(&format!(".app/source-candidates/{source_id}"))?;
    let entries = match fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(source_invalid()),
    };
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| source_invalid())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| source_invalid())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(source_invalid());
        }
        let path = context.to_project_relative(&entry.path())?;
        let mut candidate: StoredSourceCandidate = files.read_json(context, &path)?;
        normalize_ai_organize_metadata(&mut candidate.ai_organize);
        validate_candidate(&candidate, source_id)?;
        validate_candidate_evidence(context, files, &candidate)?;
        candidates.push(candidate);
    }
    candidates.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(candidates.pop())
}

fn load_candidate(
    context: &ProjectContext,
    files: &FileStore,
    source_id: &str,
    candidate_id: &str,
) -> Result<StoredSourceCandidate, BackendError> {
    if !safe_id(source_id) || !safe_id(candidate_id) {
        return Err(source_not_found());
    }
    let mut candidate: StoredSourceCandidate =
        files.read_json(context, &candidate_path(source_id, candidate_id))?;
    normalize_ai_organize_metadata(&mut candidate.ai_organize);
    validate_candidate(&candidate, source_id)?;
    validate_candidate_evidence(context, files, &candidate)?;
    if candidate.candidate_id != candidate_id {
        return Err(source_invalid());
    }
    Ok(candidate)
}

fn validate_candidate(
    candidate: &StoredSourceCandidate,
    source_id: &str,
) -> Result<(), BackendError> {
    if candidate.schema_version != 1
        || candidate.source_id != source_id
        || !safe_id(&candidate.source_id)
        || !safe_id(&candidate.candidate_id)
        || !safe_id(&candidate.base_version_id)
        || candidate.base_markdown_hash != digest(candidate.base_markdown.as_bytes())
        || candidate.candidate_markdown_hash != digest(candidate.candidate_markdown.as_bytes())
        || candidate.created_at.trim().is_empty()
        || (candidate.kind == SourceCandidateKind::AiOrganize) != candidate.ai_organize.is_some()
    {
        return Err(source_invalid());
    }
    Ok(())
}

fn validate_candidate_evidence(
    context: &ProjectContext,
    files: &FileStore,
    candidate: &StoredSourceCandidate,
) -> Result<(), BackendError> {
    let prefix = format!(
        ".app/source-candidate-evidence/{}/{}/",
        candidate.source_id, candidate.candidate_id
    );
    for artifact in &candidate.processing_evidence {
        let metadata = fs::symlink_metadata(context.resolve_project_path(&artifact.path)?)
            .map_err(|_| source_invalid())?;
        if !artifact.path.starts_with(&prefix)
            || !metadata.is_file()
            || metadata.file_type().is_symlink()
            || files.file_hash(context, &artifact.path)? != artifact.sha256
            || metadata.len() != artifact.size_bytes
        {
            return Err(source_invalid());
        }
    }
    Ok(())
}

fn apply_markdown_version(
    context: &ProjectContext,
    files: &FileStore,
    git: &GitService,
    loaded: LoadedSource,
    selected_markdown: &str,
    timeline_kind: &str,
    candidate_paths_to_delete: Vec<String>,
    processing_evidence: Vec<(String, Vec<u8>)>,
    package_snapshot: Option<SourcePackageManifest>,
    provenance: Option<AppliedSourceProvenance>,
) -> Result<SourceMutationResult, BackendError> {
    let (_, body) = parse_final_source(selected_markdown).map_err(|_| source_invalid())?;
    if body.trim().is_empty() {
        return Err(source_invalid());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let version_id = uuid::Uuid::new_v4().to_string();
    let content_hash = digest(body.as_bytes());
    let frontmatter = SourceFrontmatter {
        page_type: crate::models::import_v2::SourcePageType::Source,
        source_id: loaded.manifest.source_id.clone(),
        version_id: version_id.clone(),
        source_kind: loaded.manifest.source_kind.clone(),
        title: loaded.manifest.title.clone(),
        imported_at: now.clone(),
        content_hash: content_hash.clone(),
        platform: loaded.manifest.platform.clone(),
        canonical_url: loaded.manifest.canonical_url.clone(),
        platform_content_id: loaded.manifest.platform_content_id.clone(),
        author: loaded.manifest.author.clone(),
        published_at: loaded.manifest.published_at.clone(),
        language: loaded.manifest.language.clone(),
        quality: loaded.version.quality.clone(),
        restricted: loaded.manifest.restricted_content,
    };
    let final_markdown = render_source_markdown(&frontmatter, &body)?;
    let final_hash = digest(final_markdown.as_bytes());
    let raw_path = format!(
        "raw/sources/{}/{}/derived/source-update.md",
        loaded.manifest.source_id, version_id
    );
    let baseline_path = format!(
        ".app/source-artifacts/{}/{}/baseline.md",
        loaded.manifest.source_id, version_id
    );
    let manifest_path = loaded.manifest_path.clone();
    let mut raw_evidence = Vec::new();
    let mut additional_new_writes = Vec::<(String, Vec<u8>)>::new();
    for artifact in loaded.version.raw_evidence.iter().filter(|artifact| {
        !matches!(
            artifact.kind.as_str(),
            "source_package_manifest" | "source_reprocess_candidate"
        )
    }) {
        let next_path =
            replace_version_segment(&artifact.path, &loaded.version.version_id, &version_id)?;
        let bytes = read_project_file_nofollow(
            &context.root,
            &context.resolve_project_path(&artifact.path)?,
        )
        .map_err(|_| source_invalid())?;
        if digest(&bytes) != artifact.sha256 {
            return Err(source_invalid());
        }
        raw_evidence.push(SourceArtifactRecord {
            path: next_path.clone(),
            sha256: artifact.sha256.clone(),
            size_bytes: bytes.len() as u64,
            kind: artifact.kind.clone(),
        });
        additional_new_writes.push((next_path, bytes));
    }
    raw_evidence.push(SourceArtifactRecord {
        path: raw_path.clone(),
        sha256: final_hash.clone(),
        size_bytes: final_markdown.len() as u64,
        kind: "source_reprocess_candidate".into(),
    });
    for (index, (kind, bytes)) in processing_evidence.into_iter().enumerate() {
        let path = format!(
            "raw/sources/{}/{}/derived/processing-evidence-{index}.bin",
            loaded.manifest.source_id, version_id
        );
        raw_evidence.push(SourceArtifactRecord {
            path: path.clone(),
            sha256: digest(&bytes),
            size_bytes: bytes.len() as u64,
            kind,
        });
        additional_new_writes.push((path, bytes));
    }
    let mut additional_wiki_writes = Vec::<(String, String, Vec<u8>)>::new();
    let mut next_assets = Vec::new();
    for asset in &loaded.version.assets {
        let next_path =
            replace_version_segment(&asset.path, &loaded.version.version_id, &version_id)?;
        let bytes =
            read_project_file_nofollow(&context.root, &context.resolve_project_path(&asset.path)?)
                .map_err(|_| source_invalid())?;
        if digest(&bytes) != asset.sha256 {
            return Err(source_invalid());
        }
        next_assets.push(SourceArtifactRecord {
            path: next_path.clone(),
            sha256: asset.sha256.clone(),
            size_bytes: bytes.len() as u64,
            kind: asset.kind.clone(),
        });
        additional_new_writes.push((next_path, bytes));
    }
    if let Some(mut package) = loaded.package.clone() {
        package.version_id = version_id.clone();
        package.entry_wiki_path = loaded.manifest.wiki_path.clone();
        for member in &mut package.members {
            if member.role == SourcePackageMemberRole::Index {
                member.wiki_path = loaded.manifest.wiki_path.clone();
                member.baseline_path = baseline_path.clone();
                member.content_hash = final_hash.clone();
                member.human_edit_hash = final_hash.clone();
                continue;
            }
            let current_member_hash = files.file_hash(context, &member.wiki_path)?;
            let bytes = if let Some(snapshot) = package_snapshot.as_ref() {
                let snapshot_member = snapshot
                    .members
                    .iter()
                    .find(|candidate| {
                        candidate.order == member.order && candidate.role == member.role
                    })
                    .ok_or_else(source_invalid)?;
                let bytes = read_project_file_nofollow(
                    &context.root,
                    &context.resolve_project_path(&snapshot_member.baseline_path)?,
                )
                .map_err(|_| source_invalid())?;
                if digest(&bytes) != snapshot_member.content_hash {
                    return Err(source_invalid());
                }
                additional_wiki_writes.push((
                    member.wiki_path.clone(),
                    current_member_hash,
                    bytes.clone(),
                ));
                bytes
            } else {
                read_project_file_nofollow(
                    &context.root,
                    &context.resolve_project_path(&member.wiki_path)?,
                )
                .map_err(|_| source_invalid())?
            };
            let hash = digest(&bytes);
            let file_name = Path::new(&member.wiki_path)
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(source_invalid)?;
            let next_baseline = format!(
                ".app/source-artifacts/{}/{}/package/{file_name}",
                loaded.manifest.source_id, version_id
            );
            member.baseline_path = next_baseline.clone();
            member.content_hash = hash.clone();
            member.human_edit_hash = hash;
            additional_new_writes.push((next_baseline, bytes));
        }
        package.validate_committed().map_err(|_| source_invalid())?;
        let package_path = format!(
            "raw/sources/{}/{}/derived/source-package.json",
            loaded.manifest.source_id, version_id
        );
        let package_bytes = pretty_json(&package)?;
        raw_evidence.push(SourceArtifactRecord {
            path: package_path.clone(),
            sha256: digest(&package_bytes),
            size_bytes: package_bytes.len() as u64,
            kind: "source_package_manifest".into(),
        });
        additional_new_writes.push((package_path, package_bytes));
    }
    let mut affected_paths = vec![
        loaded.manifest.wiki_path.clone(),
        manifest_path.clone(),
        SOURCE_INDEX_PATH.into(),
        raw_path.clone(),
        baseline_path.clone(),
    ];
    affected_paths.extend(candidate_paths_to_delete.iter().cloned());
    affected_paths.extend(
        additional_wiki_writes
            .iter()
            .map(|(path, _, _)| path.clone()),
    );
    affected_paths.sort();
    affected_paths.dedup();
    let checkpoint_message = if timeline_kind == "version_restored" {
        "Before restoring Source version"
    } else {
        "Before applying Source candidate"
    };
    let checkpoint = git.create_scoped_checkpoint(
        context,
        CheckpointPurpose::HighRiskOperation,
        checkpoint_message,
        &affected_paths,
    )?;
    let mut next_manifest = loaded.manifest.clone();
    next_manifest.current_version_id = version_id.clone();
    next_manifest.versions.push(SourceVersion {
        version_id: version_id.clone(),
        content_hash: content_hash.clone(),
        raw_evidence,
        assets: next_assets,
        baseline_path: baseline_path.clone(),
        candidate: SourceCandidateRecord {
            markdown_hash: final_hash.clone(),
            title: next_manifest.title.clone(),
            source_kind: next_manifest.source_kind.clone(),
            canonical_url: next_manifest.canonical_url.clone(),
            platform: next_manifest.platform.clone(),
            platform_content_id: next_manifest.platform_content_id.clone(),
            author: next_manifest.author.clone(),
            published_at: next_manifest.published_at.clone(),
            language: next_manifest.language.clone(),
        },
        provenance: SourceProvenance {
            locator: loaded.version.provenance.locator.clone(),
            route: provenance
                .as_ref()
                .map(|value| value.route.clone())
                .unwrap_or_else(|| "source_reprocess".into()),
            engine_id: provenance
                .as_ref()
                .map(|value| value.engine_id.clone())
                .unwrap_or_else(|| loaded.version.provenance.engine_id.clone()),
            engine_version: provenance
                .as_ref()
                .map(|value| value.engine_version.clone())
                .unwrap_or_else(|| loaded.version.provenance.engine_version.clone()),
        },
        quality: loaded.version.quality.clone(),
        created_at: now.clone(),
        human_edit_hash: Some(final_hash.clone()),
        checkpoint: checkpoint.commit_hash.clone(),
    });
    next_manifest.timeline.push(SourceTimelineEvent {
        event_id: uuid::Uuid::new_v4().to_string(),
        kind: timeline_kind.into(),
        version_id: Some(version_id.clone()),
        created_at: now,
        checkpoint: checkpoint.commit_hash.clone(),
    });
    SourceRegistry::validate_manifest_contract(&next_manifest)?;
    let mut next_index = SourceRegistry::read_index(context, files)?;
    repoint_source(&mut next_index, &next_manifest.source_id, &version_id);
    next_index.by_content_hash.insert(
        content_hash,
        SourcePointer {
            source_id: next_manifest.source_id.clone(),
            version_id: version_id.clone(),
        },
    );
    let index_hash = files.file_hash(context, SOURCE_INDEX_PATH)?;
    let mut transaction = FileTransaction::new_for_project(&context.root);
    transaction.write_new(
        &context.resolve_project_path(&raw_path)?,
        final_markdown.as_bytes(),
    )?;
    transaction.write_new(
        &context.resolve_project_path(&baseline_path)?,
        final_markdown.as_bytes(),
    )?;
    for (path, bytes) in additional_new_writes {
        transaction.write_new(&context.resolve_project_path(&path)?, &bytes)?;
    }
    for (path, expected_hash, bytes) in additional_wiki_writes {
        transaction.write_if_hash_matches(
            &context.resolve_project_path(&path)?,
            &bytes,
            &expected_hash,
        )?;
    }
    transaction.write_if_hash_matches(
        &context.resolve_project_path(&loaded.manifest.wiki_path)?,
        final_markdown.as_bytes(),
        &loaded.current_hash,
    )?;
    transaction.write_if_hash_matches(
        &context.resolve_project_path(&manifest_path)?,
        &pretty_json(&next_manifest)?,
        &loaded.manifest_hash,
    )?;
    transaction.write_if_hash_matches(
        &context.resolve_project_path(SOURCE_INDEX_PATH)?,
        &pretty_json(&next_index)?,
        &index_hash,
    )?;
    let applied_candidate = !candidate_paths_to_delete.is_empty();
    for path in candidate_paths_to_delete {
        let candidate_hash = files.file_hash(context, &path)?;
        transaction
            .delete_if_hash_matches(&context.resolve_project_path(&path)?, &candidate_hash)?;
    }
    transaction.commit()?;
    if applied_candidate {
        remove_empty_tree(&context.resolve_project_path(&format!(
            ".app/source-candidates/{}",
            next_manifest.source_id
        ))?);
        remove_empty_tree(&context.resolve_project_path(&format!(
            ".app/source-candidate-evidence/{}",
            next_manifest.source_id
        ))?);
    }
    Ok(SourceMutationResult {
        source_id: next_manifest.source_id,
        version_id,
        wiki_path: next_manifest.wiki_path,
        checkpoint: checkpoint.commit_hash,
    })
}

fn replace_version_segment(
    path: &str,
    previous_version_id: &str,
    next_version_id: &str,
) -> Result<String, BackendError> {
    let needle = format!("/{previous_version_id}/");
    if !path.contains(&needle) {
        return Err(source_invalid());
    }
    Ok(path.replacen(&needle, &format!("/{next_version_id}/"), 1))
}

fn build_delete_preview(
    context: &ProjectContext,
    files: &FileStore,
    loaded: &LoadedSource,
) -> Result<DeleteSourcePreview, BackendError> {
    let inventory = source_inventory(context, files, loaded)?;
    let referenced_by =
        pages_referencing_source(context, &loaded.manifest, loaded.package.as_ref())?;
    let guard_token = inventory_guard_token(
        &loaded.manifest.source_id,
        &loaded.manifest_hash,
        &inventory,
    );
    let restorability = version_restorability(context, files, &loaded.manifest);
    Ok(DeleteSourcePreview {
        source_id: loaded.manifest.source_id.clone(),
        title: loaded.manifest.title.clone(),
        paths: inventory
            .iter()
            .map(|entry| SourceArtifactSummary {
                path: entry.path.clone(),
                kind: entry.kind.clone(),
                size_bytes: entry.size_bytes,
            })
            .collect(),
        versions: version_summaries(&loaded.manifest, &restorability),
        reference_count: referenced_by.len(),
        referenced_by,
        expected_freed_bytes: inventory.iter().map(|entry| entry.size_bytes).sum(),
        guard_token,
    })
}

fn source_inventory(
    context: &ProjectContext,
    files: &FileStore,
    loaded: &LoadedSource,
) -> Result<Vec<InventoryEntry>, BackendError> {
    let mut paths = BTreeMap::<String, String>::new();
    paths.insert(loaded.manifest_path.clone(), "source_manifest".into());
    paths.insert(loaded.manifest.wiki_path.clone(), "source_markdown".into());
    for version in &loaded.manifest.versions {
        let current_version = version.version_id == loaded.manifest.current_version_id;
        paths.insert(version.baseline_path.clone(), "baseline".into());
        for artifact in &version.raw_evidence {
            paths.insert(artifact.path.clone(), artifact.kind.clone());
        }
        for artifact in &version.assets {
            paths.insert(artifact.path.clone(), artifact.kind.clone());
        }
        if let Some(package) = load_package_for_version(context, files, &loaded.manifest, version)?
        {
            for member in package.members {
                // Historical public paths may have been released and later
                // claimed by another Source. Only the current package owns
                // live Wiki pages; historical versions own baselines/evidence.
                if current_version {
                    paths.insert(member.wiki_path, "source_package_page".into());
                }
                paths.insert(member.baseline_path, "source_package_baseline".into());
            }
        }
    }
    let candidate_root = context.resolve_project_path(&format!(
        ".app/source-candidates/{}",
        loaded.manifest.source_id
    ))?;
    if candidate_root.exists() {
        collect_project_files(context, &candidate_root, &mut paths, "source_candidate")?;
    }
    let candidate_evidence_root = context.resolve_project_path(&format!(
        ".app/source-candidate-evidence/{}",
        loaded.manifest.source_id
    ))?;
    if candidate_evidence_root.exists() {
        collect_project_files(
            context,
            &candidate_evidence_root,
            &mut paths,
            "source_candidate_evidence",
        )?;
    }
    let mut inventory = Vec::new();
    for (path, kind) in paths {
        validate_source_inventory_path(&path, &loaded.manifest.source_id)?;
        let absolute = context.resolve_project_path(&path)?;
        let metadata = fs::symlink_metadata(&absolute).map_err(|_| source_invalid())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(source_invalid());
        }
        inventory.push(InventoryEntry {
            hash: files.file_hash(context, &path)?,
            path,
            kind,
            size_bytes: metadata.len(),
        });
    }
    inventory.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(inventory)
}

fn validate_source_inventory_path(path: &str, source_id: &str) -> Result<(), BackendError> {
    let normalized = normalize_project_path(path);
    let allowed = normalized == format!(".app/sources/{source_id}.json")
        || normalized.starts_with(&format!(".app/source-artifacts/{source_id}/"))
        || normalized.starts_with(&format!(".app/source-candidates/{source_id}/"))
        || normalized.starts_with(&format!(".app/source-candidate-evidence/{source_id}/"))
        || normalized.starts_with(&format!("raw/sources/{source_id}/"))
        || normalized.starts_with(&format!("raw/web/{source_id}/"))
        || normalized.starts_with(&format!("raw/assets/{source_id}/"))
        || normalized.starts_with("wiki/sources/");
    if !allowed {
        return Err(source_invalid());
    }
    Ok(())
}

fn collect_project_files(
    context: &ProjectContext,
    root: &Path,
    paths: &mut BTreeMap<String, String>,
    kind: &str,
) -> Result<(), BackendError> {
    for entry in fs::read_dir(root).map_err(|_| source_invalid())? {
        let entry = entry.map_err(|_| source_invalid())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|_| source_invalid())?;
        if metadata.file_type().is_symlink() {
            return Err(source_invalid());
        }
        if metadata.is_dir() {
            collect_project_files(context, &entry.path(), paths, kind)?;
        } else if metadata.is_file() {
            paths.insert(context.to_project_relative(&entry.path())?, kind.into());
        } else {
            return Err(source_invalid());
        }
    }
    Ok(())
}

fn pages_referencing_source(
    context: &ProjectContext,
    manifest: &SourceManifest,
    package: Option<&SourcePackageManifest>,
) -> Result<Vec<String>, BackendError> {
    let mut targets = BTreeSet::new();
    targets.insert(wiki_link_key(&manifest.wiki_path));
    targets.insert(manifest.title.trim().to_lowercase());
    for alias in &manifest.aliases {
        targets.insert(alias.value.trim().to_lowercase());
    }
    if let Some(package) = package {
        for member in &package.members {
            targets.insert(wiki_link_key(&member.wiki_path));
            targets.insert(member.title.trim().to_lowercase());
        }
    }
    let source_paths = package
        .map(|package| {
            package
                .members
                .iter()
                .map(|member| normalize_project_path(&member.wiki_path))
                .collect::<HashSet<_>>()
        })
        .unwrap_or_else(|| HashSet::from([normalize_project_path(&manifest.wiki_path)]));
    let mut referenced = Vec::new();
    for path in crate::services::FileStore.list_markdown_files(&context.wiki_dir)? {
        let relative = context.to_project_relative(&path)?;
        if source_paths.contains(&normalize_project_path(&relative)) {
            continue;
        }
        let contents = fs::read_to_string(&path).map_err(|_| source_invalid())?;
        if extract_wikilinks(&contents)
            .iter()
            .map(|value| value.trim().to_lowercase())
            .any(|target| {
                targets.contains(&target)
                    || targets.contains(&wiki_link_key(&format!("{target}.md")))
                    || targets
                        .iter()
                        .any(|known| target.ends_with(&format!("/{known}")))
            })
        {
            referenced.push(relative);
        }
    }
    referenced.sort();
    referenced.dedup();
    Ok(referenced)
}

fn wiki_link_key(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .trim()
        .to_lowercase()
}

fn read_bounded_text(
    context: &ProjectContext,
    relative_path: &str,
    limit: usize,
) -> Result<(String, bool), BackendError> {
    let bytes =
        read_project_file_nofollow(&context.root, &context.resolve_project_path(relative_path)?)
            .map_err(|_| source_invalid())?;
    let truncated = bytes.len() > limit;
    let bounded = if truncated { &bytes[..limit] } else { &bytes };
    Ok((String::from_utf8_lossy(bounded).into_owned(), truncated))
}

fn repoint_source(index: &mut SourceIndex, source_id: &str, version_id: &str) {
    for pointer in index.by_content_hash.values_mut() {
        if pointer.source_id == source_id {
            pointer.version_id = version_id.into();
        }
    }
    for pointer in index.by_locator.values_mut() {
        if pointer.source_id == source_id {
            pointer.version_id = version_id.into();
        }
    }
}

fn update_guard_token(
    candidate: &StoredSourceCandidate,
    manifest_hash: &str,
    current_hash: &str,
) -> String {
    digest(
        format!(
            "{}\n{}\n{}\n{}\n{}",
            candidate.source_id,
            candidate.candidate_id,
            candidate.candidate_markdown_hash,
            manifest_hash,
            current_hash
        )
        .as_bytes(),
    )
}

fn inventory_guard_token(
    source_id: &str,
    manifest_hash: &str,
    inventory: &[InventoryEntry],
) -> String {
    let mut material = format!("{source_id}\n{manifest_hash}\n");
    for entry in inventory {
        material.push_str(&entry.path);
        material.push('\0');
        material.push_str(&entry.hash);
        material.push('\0');
        material.push_str(&entry.size_bytes.to_string());
        material.push('\n');
    }
    digest(material.as_bytes())
}

fn render_line_diff(current: &str, candidate: &str) -> String {
    if current == candidate {
        return "No content changes.".into();
    }
    let current_lines = current.lines().collect::<Vec<_>>();
    let candidate_lines = candidate.lines().collect::<Vec<_>>();
    let mut diff = String::from("--- current\n+++ candidate\n");
    let max = current_lines.len().max(candidate_lines.len());
    for index in 0..max {
        match (current_lines.get(index), candidate_lines.get(index)) {
            (Some(left), Some(right)) if left == right => {}
            (Some(left), Some(right)) => {
                diff.push_str(&format!("-{left}\n+{right}\n"));
            }
            (Some(left), None) => diff.push_str(&format!("-{left}\n")),
            (None, Some(right)) => diff.push_str(&format!("+{right}\n")),
            (None, None) => {}
        }
    }
    diff
}

fn markdown_declares_source(contents: &str) -> bool {
    let split = crate::utils::markdown_utils::split_frontmatter(contents);
    split
        .frontmatter
        .as_deref()
        .map(crate::utils::markdown_utils::parse_frontmatter)
        .and_then(|frontmatter| frontmatter.get_scalar("type"))
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("source"))
}

fn source_ids(index: &SourceIndex) -> BTreeSet<String> {
    index
        .by_content_hash
        .values()
        .chain(index.by_locator.values())
        .map(|pointer| pointer.source_id.clone())
        .collect()
}

fn remove_empty_source_directories(context: &ProjectContext, source_id: &str) {
    for path in [
        context.root.join(".app/source-candidates").join(source_id),
        context
            .root
            .join(".app/source-candidate-evidence")
            .join(source_id),
        context.root.join(".app/source-artifacts").join(source_id),
        context.root.join("raw/sources").join(source_id),
        context.root.join("raw/web").join(source_id),
        context.root.join("raw/assets").join(source_id),
    ] {
        remove_empty_tree(&path);
    }
}

fn remove_empty_tree(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let child_paths = entries
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    for child in child_paths {
        if child.is_dir() {
            remove_empty_tree(&child);
        }
    }
    let _ = fs::remove_dir(path);
}

fn candidate_path(source_id: &str, candidate_id: &str) -> String {
    format!(".app/source-candidates/{source_id}/{candidate_id}.json")
}

fn candidate_evidence_path(source_id: &str, candidate_id: &str, index: usize) -> String {
    format!(".app/source-candidate-evidence/{source_id}/{candidate_id}/{index}.bin")
}

fn manifest_path(source_id: &str) -> String {
    format!(".app/sources/{source_id}.json")
}

fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn pretty_json(value: &impl Serialize) -> Result<Vec<u8>, BackendError> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|_| source_invalid())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn source_not_found() -> BackendError {
    BackendError::new(
        "SOURCE_NOT_FOUND",
        "The requested Source could not be found.",
        false,
        true,
    )
}

fn source_invalid() -> BackendError {
    BackendError::new(
        "SOURCE_BINDING_INVALID",
        "The Source registry binding or immutable evidence is invalid.",
        false,
        true,
    )
}

fn source_changed(current_hash: &str) -> BackendError {
    BackendError::new(
        "SOURCE_CHANGED",
        "The Source changed after this action was prepared. Reload and review it again.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "currentHash": current_hash }))
}

fn source_busy() -> BackendError {
    BackendError::new(
        "SOURCE_BUSY",
        "Another Source mutation is currently running.",
        true,
        false,
    )
}

fn source_action_unavailable() -> BackendError {
    BackendError::new(
        "SOURCE_ACTION_UNAVAILABLE",
        "The required retained original or processing capability is unavailable for this Source.",
        false,
        true,
    )
}

fn source_processing_cancelled() -> BackendError {
    BackendError::new(
        "SOURCE_PROCESSING_CANCELLED",
        "Source processing was cancelled before a candidate was created.",
        false,
        true,
    )
}

fn source_requires_dedicated_action() -> BackendError {
    BackendError::new(
        "SOURCE_DEDICATED_ACTION_REQUIRED",
        "Sources must be managed with their dedicated Source actions.",
        false,
        true,
    )
}

#[cfg(test)]
#[path = "source_lifecycle_tests.rs"]
mod tests;
