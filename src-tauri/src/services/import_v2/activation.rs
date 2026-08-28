use std::fs;

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::errors::BackendError;
use crate::models::git::CheckpointPurpose;
use crate::models::import_backend_activation::{
    ActivationConfirmation, ActivationResult, ImportBackend, ImportBackendActivation,
    IMPORT_BACKEND_ACTIVATION_SCHEMA_VERSION,
};
use crate::models::import_v2_migration::{MigrationReport, MigrationStatus};
use crate::models::paths::ProjectContext;
use crate::services::import_v2::migration::{
    CutoverVerifier, MigrationReadinessEvidence, REQUIRED_IMPORT_V2_CONTRACT,
};
use crate::services::import_v2::transaction::{
    is_project_reparse_point, read_project_file_nofollow, FileTransaction,
};
use crate::services::GitService;

const ACTIVATION_PATH: &str = ".app/import-v2-migration/activation.json";

#[derive(Debug, Clone, Copy, Default)]
pub struct ImportV2ActivationService;

impl ImportV2ActivationService {
    pub fn confirmation_token(&self, report: &MigrationReport, release_version: &str) -> String {
        digest(
            format!(
                "import-v2-activation\n{}\n{}",
                report.plan_fingerprint, release_version
            )
            .as_bytes(),
        )
    }

    pub fn activate(
        &self,
        core: &crate::services::import_v2::ImportV2Service,
        git: &GitService,
        context: &ProjectContext,
        report: &MigrationReport,
        readiness: &MigrationReadinessEvidence,
        release_version: &str,
        confirmation: ActivationConfirmation,
    ) -> Result<ActivationResult, BackendError> {
        let project_locks = core.project_locks(context)?;
        let _guard = core.lock_project(&project_locks);
        core.preflight_migration_locked(context)?;
        if release_version.trim().is_empty() {
            return Err(BackendError::new(
                "IMPORT_V2_ACTIVATION_RELEASE_INVALID",
                "A release version is required for activation.",
                false,
                true,
            ));
        }
        if report.status != MigrationStatus::Applied {
            return Err(BackendError::new(
                "IMPORT_V2_ACTIVATION_REPORT_NOT_APPLIED",
                "Activation requires an applied migration report.",
                false,
                true,
            ));
        }
        let readiness_result = CutoverVerifier::default().verify(readiness);
        if !readiness_result.passed {
            return Err(BackendError::new(
                "IMPORT_V2_ACTIVATION_NOT_READY",
                "Cutover readiness gates have not passed.",
                false,
                true,
            )
            .with_details(serde_json::json!({ "blockers": readiness_result.blockers })));
        }
        if confirmation.report_fingerprint != report.plan_fingerprint
            || confirmation.token != self.confirmation_token(report, release_version)
        {
            return Err(BackendError::new(
                "IMPORT_V2_ACTIVATION_CONFIRMATION_INVALID",
                "Activation confirmation is not bound to the migration report and release.",
                false,
                true,
            ));
        }
        if self.read(context)?.is_some() {
            return Err(BackendError::new(
                "IMPORT_V2_ACTIVATION_ALREADY_EXISTS",
                "This project already has an activation record; rollback is release-based.",
                false,
                true,
            ));
        }

        let checkpoint = if git.repository_status(context)?.is_repository {
            git.create_scoped_checkpoint(
                context,
                CheckpointPurpose::HighRiskOperation,
                "Before activating Import V2 as the sole import writer",
                &[ACTIVATION_PATH.into()],
            )?
            .into()
        } else {
            if !confirmation.acknowledge_no_git_rollback {
                return Err(BackendError::new(
                    "IMPORT_V2_ACTIVATION_GIT_CHECKPOINT_REQUIRED",
                    "No Git repository is available; acknowledge release-based rollback before activation.",
                    true,
                    true,
                ));
            }
            None
        };

        let record = ImportBackendActivation {
            schema_version: IMPORT_BACKEND_ACTIVATION_SCHEMA_VERSION,
            active_backend: ImportBackend::V2,
            core_contract_version: REQUIRED_IMPORT_V2_CONTRACT.into(),
            migration_report_fingerprint: report.plan_fingerprint.clone(),
            activated_at: Utc::now().to_rfc3339(),
            release_version: release_version.into(),
            legacy_mutations_disabled: true,
            rollback_mode: "release_based".into(),
        };
        let bytes = serde_json::to_vec_pretty(&record).map_err(|error| {
            BackendError::new("JSON_SERIALIZE_FAILED", error.to_string(), false, true)
        })?;
        let path = context.resolve_project_path(ACTIVATION_PATH)?;
        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.write_new(&path, &bytes)?;
        transaction.commit()?;
        Ok(ActivationResult { record, checkpoint })
    }

    pub fn read(
        &self,
        context: &ProjectContext,
    ) -> Result<Option<ImportBackendActivation>, BackendError> {
        let path = context.resolve_project_path(ACTIVATION_PATH)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
        };
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || is_project_reparse_point(&metadata)
        {
            return Err(BackendError::new(
                "IMPORT_V2_ACTIVATION_INVALID",
                "Activation metadata is not a safe regular file.",
                false,
                true,
            ));
        }
        let bytes = read_project_file_nofollow(&context.root, &path)?;
        let record: ImportBackendActivation = serde_json::from_slice(&bytes).map_err(|_| {
            BackendError::new(
                "IMPORT_V2_ACTIVATION_INVALID",
                "Activation metadata is not valid JSON.",
                false,
                true,
            )
        })?;
        if record.schema_version != IMPORT_BACKEND_ACTIVATION_SCHEMA_VERSION
            || record.active_backend != ImportBackend::V2
            || !record.legacy_mutations_disabled
            || record.rollback_mode != "release_based"
        {
            return Err(BackendError::new(
                "IMPORT_V2_ACTIVATION_INVALID",
                "Activation metadata does not describe the supported V2 release cutover.",
                false,
                true,
            ));
        }
        Ok(Some(record))
    }

    pub fn legacy_mutation_guard(context: &ProjectContext) -> Result<(), BackendError> {
        if Self::default().read(context)?.is_some() {
            return Err(BackendError::new(
                "IMPORT_V2_LEGACY_MUTATION_DISABLED",
                "Legacy import mutation is disabled after Import V2 activation; rollback is release-based.",
                false,
                true,
            ));
        }
        Ok(())
    }

    pub fn is_active(context: &ProjectContext) -> Result<bool, BackendError> {
        Ok(Self::default().read(context)?.is_some())
    }
}

fn io_error(error: std::io::Error) -> BackendError {
    BackendError::new(
        "IMPORT_V2_ACTIVATION_READ_FAILED",
        error.to_string(),
        true,
        true,
    )
}

fn digest(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
