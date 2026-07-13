use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};
use url::Url;

use crate::errors::BackendError;
use crate::models::git::{CheckpointPurpose, GitCheckpoint};
use crate::models::import_v2_migration::{
    LegacyInventory, MigrationApplyResult, MigrationConfirmation, MigrationDecision,
    MigrationPlan, MigrationReport, MigrationStatus,
};
use crate::models::paths::ProjectContext;
use crate::services::import_v2::migration::planner::{DefaultMigrationPlanner, MigrationPlanner};
use crate::services::import_v2::source_registry::{
    validate_for_migration, SourceIndex, SourcePointer,
};
use crate::services::import_v2::transaction::{
    is_project_reparse_point, read_project_file_nofollow, FileTransaction,
};
use crate::services::GitService;
use crate::tasks::task_model::CancellationToken;

use super::{DefaultLegacyScanner, LegacyScanner};

const V2_INDEX_PATH: &str = ".app/source-index-v2.json";
const REPORT_PATH: &str = ".app/import-v2-migration/report.json";
const MISSING_FINGERPRINT: &str = "MISSING";

#[derive(Default)]
pub struct MigrationService {
    scanner: DefaultLegacyScanner,
    planner: DefaultMigrationPlanner,
}

impl MigrationService {
    pub fn scan(&self, project_root: &Path) -> Result<LegacyInventory, BackendError> {
        self.scanner.scan(project_root)
    }

    pub fn plan(
        &self,
        project_root: &Path,
        inventory: &LegacyInventory,
    ) -> Result<MigrationPlan, BackendError> {
        inventory.validate()?;
        let (index, raw_fingerprint) = read_v2_index_snapshot(project_root)?;
        let mut plan = self.planner.plan(inventory, &index)?;
        plan.v2_index_fingerprint = raw_fingerprint;
        plan.validate()?;
        Ok(plan)
    }

    pub fn confirmation_token(&self, plan: &MigrationPlan, project_identity: &str) -> String {
        digest(
            format!(
                "import-v2-migration-confirm\n{}\n{}\n{}",
                plan.fingerprint(), plan.v2_index_fingerprint, project_identity
            )
            .as_bytes(),
        )
    }

    pub fn apply_metadata(
        &self,
        core: &crate::services::import_v2::ImportV2Service,
        git: &GitService,
        context: &ProjectContext,
        plan: &MigrationPlan,
        confirmation: MigrationConfirmation,
        cancellation: &CancellationToken,
    ) -> Result<MigrationApplyResult, BackendError> {
        plan.validate()?;
        let _guard = core.acquire_migration_lock()?;
        core.preflight_migration_locked(context)?;

        if cancellation.is_cancelled() {
            return Ok(cancelled_result(plan));
        }
        let inventory = self.scanner.scan(&context.root)?;
        if inventory.project_identity.is_empty()
            || inventory.fingerprint != plan.inventory_fingerprint
        {
            return Err(stale_plan("legacy inventory or project identity changed"));
        }
        if confirmation.plan_fingerprint != plan.fingerprint()
            || confirmation.token != self.confirmation_token(plan, &inventory.project_identity)
        {
            return Err(BackendError::new(
                "IMPORT_V2_MIGRATION_CONFIRMATION_INVALID",
                "Migration confirmation is not bound to the current plan and project.",
                false,
                true,
            ));
        }
        if let Some(result) = self.read_idempotent_result(context, plan)? {
            return Ok(result);
        }

        let (current_index, current_fingerprint) = read_v2_index_snapshot(&context.root)?;
        if current_fingerprint != plan.v2_index_fingerprint {
            return Err(stale_plan("V2 index generation changed after dry run"));
        }
        if cancellation.is_cancelled() {
            return Ok(cancelled_result(plan));
        }

        let affected_paths = vec![V2_INDEX_PATH.to_string(), REPORT_PATH.to_string()];
        let checkpoint = prepare_checkpoint(git, context, &affected_paths, &confirmation)?;
        if cancellation.is_cancelled() {
            return Ok(cancelled_result_with_checkpoint(plan, checkpoint));
        }

        let next_index = build_next_index(current_index, plan)?;
        let index_bytes = serde_json::to_vec_pretty(&next_index).map_err(|error| {
            BackendError::new("JSON_SERIALIZE_FAILED", error.to_string(), false, true)
        })?;
        let mut report = MigrationReport::from_plan(plan, &inventory)?;
        report.status = MigrationStatus::Applied;
        report.required_confirmation = false;
        let report_bytes = serde_json::to_vec_pretty(&report).map_err(|error| {
            BackendError::new("JSON_SERIALIZE_FAILED", error.to_string(), false, true)
        })?;

        let index_path = context.resolve_project_path(V2_INDEX_PATH)?;
        let report_path = context.resolve_project_path(REPORT_PATH)?;
        let mut transaction = FileTransaction::new_for_project(&context.root);
        if current_fingerprint == MISSING_FINGERPRINT {
            transaction.write_new(&index_path, &index_bytes)?;
        } else {
            transaction.write_if_hash_matches(
                &index_path,
                &index_bytes,
                &plan.v2_index_fingerprint,
            )?;
        }
        transaction.write_new(&report_path, &report_bytes)?;
        transaction.commit()?;

        Ok(MigrationApplyResult {
            status: MigrationStatus::Applied,
            plan_fingerprint: plan.fingerprint(),
            applied_candidate_ids: applied_candidate_ids(plan),
            report_relative_path: REPORT_PATH.into(),
            checkpoint,
        })
    }

    pub fn resume(
        &self,
        core: &crate::services::import_v2::ImportV2Service,
        git: &GitService,
        context: &ProjectContext,
        plan: &MigrationPlan,
        confirmation: MigrationConfirmation,
        cancellation: &CancellationToken,
    ) -> Result<MigrationApplyResult, BackendError> {
        FileTransaction::reconcile_project(&context.root)?;
        self.apply_metadata(core, git, context, plan, confirmation, cancellation)
    }

    fn read_idempotent_result(
        &self,
        context: &ProjectContext,
        plan: &MigrationPlan,
    ) -> Result<Option<MigrationApplyResult>, BackendError> {
        let path = context.resolve_project_path(REPORT_PATH)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error("MIGRATION_REPORT_READ_FAILED", error)),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || is_project_reparse_point(&metadata)
        {
            return Err(BackendError::new(
                "MIGRATION_REPORT_INVALID",
                "The migration report is not a safe regular file.",
                false,
                true,
            ));
        }
        let bytes = read_project_file_nofollow(&context.root, &path)?;
        let report: MigrationReport = serde_json::from_slice(&bytes).map_err(|_| {
            BackendError::new(
                "MIGRATION_REPORT_INVALID",
                "The migration report is not valid migration metadata.",
                false,
                true,
            )
        })?;
        if report.status == MigrationStatus::Applied {
            if report.plan_fingerprint == plan.fingerprint() {
                return Ok(Some(MigrationApplyResult {
                    status: MigrationStatus::Applied,
                    plan_fingerprint: plan.fingerprint(),
                    applied_candidate_ids: report
                        .automatic_links
                        .iter()
                        .chain(report.proposed_records.iter())
                        .map(|candidate| candidate.candidate_id.clone())
                        .collect(),
                    report_relative_path: REPORT_PATH.into(),
                    checkpoint: None,
                }));
            }
            return Err(BackendError::new(
                "IMPORT_V2_MIGRATION_ALREADY_APPLIED",
                "A different migration plan has already been applied to this project.",
                false,
                true,
            ));
        }
        Err(BackendError::new(
            "MIGRATION_REPORT_INVALID",
            "An existing migration report is not a resumable applied state.",
            false,
            true,
        ))
    }
}

fn prepare_checkpoint(
    git: &GitService,
    context: &ProjectContext,
    affected_paths: &[String],
    confirmation: &MigrationConfirmation,
) -> Result<Option<GitCheckpoint>, BackendError> {
    if !git.repository_status(context)?.is_repository {
        if !confirmation.acknowledge_no_git_rollback {
            return Err(BackendError::new(
                "IMPORT_V2_MIGRATION_GIT_CHECKPOINT_REQUIRED",
                "No Git repository is available; explicitly acknowledge release-based rollback before applying migration metadata.",
                true,
                true,
            ));
        }
        return Ok(None);
    }
    git.create_scoped_checkpoint(
        context,
        CheckpointPurpose::HighRiskOperation,
        "Before Import V2 migration metadata apply",
        affected_paths,
    )
    .map(Some)
}

fn build_next_index(
    mut index: SourceIndex,
    plan: &MigrationPlan,
) -> Result<SourceIndex, BackendError> {
    for candidate in &plan.candidates {
        let MigrationDecision::CreateV2Record {
            proposed_source_id,
        } = &candidate.decision
        else {
            continue;
        };
        let Some(hash) = candidate
            .record
            .recorded_content_sha256
            .as_deref()
            .or(candidate.record.original_sha256.as_deref())
        else {
            continue;
        };
        let version_id = format!("migrated-version-{}", &candidate.candidate_id[10..]);
        let pointer = SourcePointer {
            source_id: proposed_source_id.clone(),
            version_id,
        };
        if index.by_content_hash.contains_key(hash) {
            return Err(stale_plan("a new V2 content hash appeared after planning"));
        }
        index.by_content_hash.insert(hash.into(), pointer.clone());
        if let Some(url) = candidate
            .record
            .normalized_url
            .as_deref()
            .and_then(normalize_public_url)
        {
            if index.by_locator.contains_key(&url) {
                return Err(stale_plan("a new V2 locator appeared after planning"));
            }
            index.by_locator.insert(url, pointer);
        }
    }
    Ok(index)
}

fn read_v2_index_snapshot(root: &Path) -> Result<(SourceIndex, String), BackendError> {
    let path = root.join(".app").join("source-index-v2.json");
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((SourceIndex::default_v2(), MISSING_FINGERPRINT.into()))
        }
        Err(error) => return Err(io_error("IMPORT_V2_INDEX_READ_FAILED", error)),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || is_project_reparse_point(&metadata)
    {
        return Err(BackendError::new(
            "IMPORT_V2_INDEX_INVALID",
            "The V2 source index is not a safe regular file.",
            false,
            true,
        ));
    }
    let bytes = read_project_file_nofollow(root, &path)?;
    let index: SourceIndex = serde_json::from_slice(&bytes).map_err(|_| {
        BackendError::new(
            "IMPORT_V2_INDEX_INVALID",
            "The V2 source index is not valid JSON.",
            false,
            true,
        )
    })?;
    validate_for_migration(&index)?;
    Ok((index, digest(&bytes)))
}

fn applied_candidate_ids(plan: &MigrationPlan) -> Vec<String> {
    plan.candidates
        .iter()
        .filter(|candidate| {
            matches!(
                candidate.decision,
                MigrationDecision::LinkExisting { .. } | MigrationDecision::CreateV2Record { .. }
            )
        })
        .map(|candidate| candidate.candidate_id.clone())
        .collect()
}

fn cancelled_result(plan: &MigrationPlan) -> MigrationApplyResult {
    cancelled_result_with_checkpoint(plan, None)
}

fn cancelled_result_with_checkpoint(
    plan: &MigrationPlan,
    checkpoint: Option<GitCheckpoint>,
) -> MigrationApplyResult {
    MigrationApplyResult {
        status: MigrationStatus::Cancelled,
        plan_fingerprint: plan.fingerprint(),
        applied_candidate_ids: Vec::new(),
        report_relative_path: REPORT_PATH.into(),
        checkpoint,
    }
}

fn stale_plan(reason: &str) -> BackendError {
    BackendError::new("IMPORT_V2_MIGRATION_PLAN_STALE", reason, true, true)
}

fn io_error(code: &str, error: std::io::Error) -> BackendError {
    BackendError::new(code, error.to_string(), true, true)
}

fn normalize_public_url(value: &str) -> Option<String> {
    let mut url = Url::parse(value.trim()).ok()?;
    url.set_fragment(None);
    Some(url.to_string())
}

fn digest(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}
