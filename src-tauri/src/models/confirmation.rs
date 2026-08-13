use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use crate::errors::BackendError;
use crate::models::project::ProjectSummary;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PendingAction {
    pub id: String,
    pub action_type: PendingActionType,
    pub title: String,
    pub message: String,
    pub risk_level: RiskLevel,
    pub affected_paths: Vec<String>,
    pub preview: Option<ActionPreview>,
    pub expires_at: Option<String>,
    /// Git checkpoint commit hash that already protects this action, if the
    /// backend created one before surfacing the confirmation (e.g. a wiki
    /// compile checkpoint created before the conflict manifest is generated).
    /// `None` for actions that only create their checkpoint *after* the user
    /// confirms (lint high-risk fixes, chat overwrite). The frontend uses this
    /// to render an honest "Checkpoint: available / not created yet" state
    /// (`checkpointExists = action.checkpointHash !== null`), so `None` must
    /// serialize to JSON `null` rather than being omitted.
    #[serde(default)]
    pub checkpoint_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PendingActionType {
    RepairProject,
    EnableCompatibleProject,
    ConfigureCompatibleLayout,
    TrustCompatibleProject,
    InitializeGitRepository,
    CreateGitCheckpoint,
    DeleteFile,
    OverwriteFile,
    BatchRewrite,
    MergeConflict,
    AgentAutoFix,
    InstallAgent,
    RunSkill,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Destructive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActionPreview {
    pub summary: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub diff: Option<String>,
}

#[derive(Default)]
pub struct ConfirmationRegistry {
    actions: Mutex<HashMap<String, StoredPendingAction>>,
    executing: Mutex<HashSet<String>>,
    cancel_requested: Mutex<HashSet<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationStatus {
    Confirmed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedAction {
    pub action: PendingAction,
    pub status: ConfirmationStatus,
    pub checkpoint_exists: bool,
    pub project_summary: Option<ProjectSummary>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredPendingAction {
    pub action: PendingAction,
    pub execution: Option<ConfirmationExecution>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationRegistration {
    Registered,
    Existing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpiringConfirmationRegistration {
    pub registration: ConfirmationRegistration,
    pub stored: StoredPendingAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationClaimDisposition {
    Completed,
    CancelRequested,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ConfirmationExecution {
    RepairProject {
        assessment_id: crate::models::project::AssessmentId,
        project_id: String,
        root_path: String,
        plan: crate::models::project::ProjectRepairPlan,
    },
    EnableCompatibleProject {
        assessment_id: crate::models::project::AssessmentId,
        project_id: String,
        root_path: String,
        template: crate::models::project::ProjectTemplate,
        initialize_git: bool,
    },
    ConfigureCompatibleLayout {
        assessment_id: crate::models::project::AssessmentId,
        project_id: String,
        root_path: String,
        mapping: crate::models::layout::CompatibleLayoutMapping,
        expected_hash: Option<String>,
    },
    TrustCompatibleProject {
        assessment_id: crate::models::project::AssessmentId,
        project_id: String,
        root_path: String,
    },
    InitializeAssessedGit {
        assessment_id: crate::models::project::AssessmentId,
        project_id: String,
        root_path: String,
        expected_head: Option<String>,
        expected_paths: Vec<String>,
    },
    CheckpointAssessedGit {
        assessment_id: crate::models::project::AssessmentId,
        project_id: String,
        root_path: String,
        expected_head: Option<String>,
        expected_paths: Vec<String>,
    },
    CompileMerge {
        project_id: String,
        root_path: String,
        task_id: String,
        route: crate::models::compile::CompileRoute,
        plan: crate::models::compile::CompilePlan,
        manifest: crate::models::compile::CompileManifest,
        source_versions: Vec<crate::models::compile::SourceVersionRef>,
        current_hashes: Vec<(String, String)>,
        checkpoint_hash: Option<String>,
    },
    LintFix {
        project_id: String,
        root_path: String,
        issue: crate::models::lint::LintIssue,
    },
    AgentLintRepairStart {
        project_id: String,
        root_path: String,
        canonical_identity_key: String,
        identity_revision: String,
        preparation_id: String,
        preparation_revision: String,
        report_id: String,
        selection_revision: String,
        selected_finding_ids: Vec<String>,
        route: crate::models::workflow::WorkflowRoute,
        skill: crate::models::lint::WikiLintSkillRef,
        authorized_path_hashes: std::collections::BTreeMap<String, Option<String>>,
        baseline_fingerprint: String,
        expected_git_head: String,
    },
    /// Backend-only authority for applying one dangerous repair candidate.
    /// Every mutable descriptor revision is copied into this binding so a
    /// restored or retried confirmation cannot authorize a different round.
    AgentLintRepairReview(AgentLintRepairReviewBinding),
    ChatOverwrite {
        project_id: String,
        root_path: String,
        session_id: String,
        message_id: String,
        target_path: String,
        current_hash: String,
    },
    GenerateContentOverwrite {
        project_id: String,
        root_path: String,
        canonical_identity_key: String,
        identity_revision: String,
        task_id: String,
        action_id: String,
        candidate: crate::models::workflow::WorkflowCandidateReference,
    },
    UpdateWikiReview {
        project_id: String,
        root_path: String,
        canonical_identity_key: String,
        identity_revision: String,
        task_id: String,
        action_id: String,
        candidate: crate::models::workflow::WorkflowCandidateReference,
    },
    DeleteWikiPage {
        project_id: String,
        root_path: String,
        target_path: String,
        target_hash: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentLintRepairReviewBinding {
    pub project_id: String,
    pub root_path: String,
    pub canonical_identity_key: String,
    pub identity_revision: String,
    pub task_id: String,
    pub round: u8,
    /// Identifier of the task-owned candidate. Project-relative candidates
    /// have no representation in this dangerous-review authority.
    pub candidate_id: String,
    pub candidate_revision: String,
    pub manifest_revision: String,
    pub baseline_fingerprint: String,
    pub checkpoint_hash: String,
    pub action_id: String,
    pub action_revision: String,
}

impl ConfirmationRegistry {
    pub fn register(&self, action: PendingAction) -> Result<(), BackendError> {
        self.register_with_execution(action, None)
    }

    pub fn register_with_execution(
        &self,
        action: PendingAction,
        execution: Option<ConfirmationExecution>,
    ) -> Result<(), BackendError> {
        validate_execution_action_binding(&action, execution.as_ref())?;
        let mut actions = self.actions.lock().map_err(|_| {
            BackendError::new(
                "CONFIRMATION_REGISTRY_LOCKED",
                "Confirmation registry is unavailable.",
                true,
                false,
            )
        })?;
        if actions.contains_key(&action.id) {
            return Err(BackendError::new(
                "CONFIRMATION_ID_CONFLICT",
                "A confirmation with this action id is already pending.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "actionId": action.id })));
        }
        actions.insert(action.id.clone(), StoredPendingAction { action, execution });
        Ok(())
    }

    /// Register a preparation that may be safely retried by the same caller.
    /// Reuse is accepted only when both the user-visible action and every
    /// backend-only execution field are byte-for-byte identical.
    pub fn register_idempotent_with_execution(
        &self,
        action: PendingAction,
        execution: ConfirmationExecution,
    ) -> Result<ConfirmationRegistration, BackendError> {
        validate_execution_action_binding(&action, Some(&execution))?;
        let mut actions = self.actions.lock().map_err(|_| registry_locked())?;
        let stored = StoredPendingAction {
            action: action.clone(),
            execution: Some(execution),
        };
        if let Some(existing) = actions.get(&action.id) {
            return if existing == &stored {
                Ok(ConfirmationRegistration::Existing)
            } else {
                Err(confirmation_id_conflict(&action.id))
            };
        }
        actions.insert(action.id.clone(), stored);
        Ok(ConfirmationRegistration::Registered)
    }

    /// Register a bounded preparation while preserving the first expiration
    /// across exact retries. Once that stored action expires, the same exact
    /// preparation may be registered again with a fresh caller-supplied
    /// expiration.
    pub fn register_idempotent_expiring_with_execution(
        &self,
        mut action: PendingAction,
        execution: ConfirmationExecution,
        expires_at: String,
    ) -> Result<ExpiringConfirmationRegistration, BackendError> {
        action.expires_at = Some(expires_at);
        reject_if_expired(&action)?;
        validate_execution_action_binding(&action, Some(&execution))?;
        let mut stored = StoredPendingAction {
            action: action.clone(),
            execution: Some(execution),
        };
        let mut actions = self.actions.lock().map_err(|_| registry_locked())?;
        if let Some(existing) = actions.get(&action.id).cloned() {
            match reject_if_expired(&existing.action) {
                Ok(()) => {
                    stored.action.expires_at = existing.action.expires_at.clone();
                    return if existing == stored {
                        Ok(ExpiringConfirmationRegistration {
                            registration: ConfirmationRegistration::Existing,
                            stored: existing,
                        })
                    } else {
                        Err(confirmation_id_conflict(&action.id))
                    };
                }
                Err(error) if error.code == "CONFIRMATION_EXPIRED" => {
                    let executing = self.executing.lock().map_err(|_| registry_locked())?;
                    if executing.contains(&action.id) {
                        return Err(confirmation_in_use(&action.id));
                    }
                    actions.remove(&action.id);
                }
                Err(error) => return Err(error),
            }
        }
        actions.insert(action.id.clone(), stored.clone());
        Ok(ExpiringConfirmationRegistration {
            registration: ConfirmationRegistration::Registered,
            stored,
        })
    }

    /// Restore a persisted confirmation, replacing only a matching pending
    /// entry's runtime execution binding. This is required when the same
    /// canonical project root is reopened under a new runtime project id.
    pub fn restore_with_execution(
        &self,
        action: PendingAction,
        execution: ConfirmationExecution,
    ) -> Result<(), BackendError> {
        validate_execution_action_binding(&action, Some(&execution))?;
        let mut actions = self.actions.lock().map_err(|_| registry_locked())?;
        let executing = self.executing.lock().map_err(|_| registry_locked())?;
        if let Some(existing) = actions.get_mut(&action.id) {
            if existing.action.action_type != action.action_type
                || existing.action.risk_level != action.risk_level
                || existing.action.affected_paths != action.affected_paths
                || existing.action.checkpoint_hash != action.checkpoint_hash
                || !restoration_binding_matches(existing.execution.as_ref(), &execution)
            {
                return Err(confirmation_id_conflict(&action.id));
            }
            if executing.contains(&action.id) {
                return if existing.execution.as_ref() == Some(&execution) {
                    Ok(())
                } else {
                    Err(confirmation_in_use(&action.id))
                };
            }

            // The persisted action remains authoritative. Reopening the same
            // canonical root may only refresh the runtime project id carried
            // by an otherwise identical execution binding.
            existing.execution = Some(execution);
            return Ok(());
        }
        if executing.contains(&action.id) {
            return Err(confirmation_in_use(&action.id));
        }
        actions.insert(
            action.id.clone(),
            StoredPendingAction {
                action,
                execution: Some(execution),
            },
        );
        Ok(())
    }

    pub fn confirm(
        &self,
        action_id: &str,
        status: ConfirmationStatus,
    ) -> Result<StoredPendingAction, BackendError> {
        let mut actions = self.actions.lock().map_err(|_| {
            BackendError::new(
                "CONFIRMATION_REGISTRY_LOCKED",
                "Confirmation registry is unavailable.",
                true,
                false,
            )
        })?;
        if actions.get(action_id).is_some_and(|stored| {
            matches!(
                stored.execution.as_ref(),
                Some(ConfirmationExecution::AgentLintRepairReview(_))
            )
        }) {
            return Err(confirmation_exact_execution_required(action_id));
        }
        let executing = self.executing.lock().map_err(|_| registry_locked())?;
        if executing.contains(action_id) {
            if status == ConfirmationStatus::Cancelled {
                self.cancel_requested
                    .lock()
                    .map_err(|_| registry_locked())?
                    .insert(action_id.to_string());
            }
            return Err(confirmation_in_use(action_id));
        }
        let stored = actions.remove(action_id).ok_or_else(|| {
            BackendError::new(
                "CONFIRMATION_NOT_FOUND",
                "The pending action was not found or has already been handled.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "actionId": action_id }))
        })?;

        reject_if_expired(&stored.action)?;

        if status == ConfirmationStatus::Cancelled {
            return Ok(stored);
        }

        Ok(stored)
    }

    pub fn peek(&self, action_id: &str) -> Result<StoredPendingAction, BackendError> {
        let mut actions = self.actions.lock().map_err(|_| {
            BackendError::new(
                "CONFIRMATION_REGISTRY_LOCKED",
                "Confirmation registry is unavailable.",
                true,
                false,
            )
        })?;
        let stored = actions.get(action_id).cloned().ok_or_else(|| {
            BackendError::new(
                "CONFIRMATION_NOT_FOUND",
                "The pending action was not found or has already been handled.",
                true,
                true,
            )
        })?;
        if let Err(error) = reject_if_expired(&stored.action) {
            if error.code.starts_with("CONFIRMATION_EXPIRY") {
                actions.remove(action_id);
            }
            return Err(error);
        }
        Ok(stored)
    }

    /// Claim an action for execution while keeping it in the registry. A
    /// concurrent cancellation then fails closed instead of removing the
    /// approval halfway through a destructive write.
    pub fn claim(&self, action_id: &str) -> Result<StoredPendingAction, BackendError> {
        self.claim_inner(action_id, None)
    }

    /// Atomically claim only the exact backend execution facts asserted by the
    /// caller. Dangerous Agent repair reviews deliberately require this API;
    /// the generic id-only `claim` path rejects them.
    pub fn claim_exact_execution(
        &self,
        action_id: &str,
        expected: &ConfirmationExecution,
    ) -> Result<StoredPendingAction, BackendError> {
        self.claim_inner(action_id, Some(expected))
    }

    fn claim_inner(
        &self,
        action_id: &str,
        expected: Option<&ConfirmationExecution>,
    ) -> Result<StoredPendingAction, BackendError> {
        let mut actions = self.actions.lock().map_err(|_| registry_locked())?;
        let stored = actions.get(action_id).cloned().ok_or_else(|| {
            BackendError::new(
                "CONFIRMATION_NOT_FOUND",
                "The pending action was not found or has already been handled.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "actionId": action_id }))
        })?;
        if let Err(error) = reject_if_expired(&stored.action) {
            if error.code.starts_with("CONFIRMATION_EXPIRY") {
                actions.remove(action_id);
            }
            return Err(error);
        }
        validate_execution_action_binding(&stored.action, stored.execution.as_ref())?;
        if matches!(
            stored.execution.as_ref(),
            Some(ConfirmationExecution::AgentLintRepairReview(_))
        ) && expected.is_none()
        {
            return Err(confirmation_exact_execution_required(action_id));
        }
        if expected.is_some_and(|expected| stored.execution.as_ref() != Some(expected)) {
            return Err(confirmation_execution_mismatch(action_id));
        }
        let mut executing = self.executing.lock().map_err(|_| registry_locked())?;
        if !executing.insert(action_id.to_string()) {
            return Err(confirmation_in_use(action_id));
        }
        Ok(stored)
    }

    /// Cancel one exact execution binding without allowing a colliding action
    /// id to consume or interrupt another review. If execution already holds
    /// the claim, cancellation is remembered and reported by `finish_claim`.
    pub fn cancel_exact_execution(
        &self,
        action_id: &str,
        expected: &ConfirmationExecution,
    ) -> Result<StoredPendingAction, BackendError> {
        let mut actions = self.actions.lock().map_err(|_| registry_locked())?;
        let stored = actions.get(action_id).cloned().ok_or_else(|| {
            BackendError::new(
                "CONFIRMATION_NOT_FOUND",
                "The pending action was not found or has already been handled.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "actionId": action_id }))
        })?;
        if let Err(error) = reject_if_expired(&stored.action) {
            if error.code.starts_with("CONFIRMATION_EXPIRY") {
                actions.remove(action_id);
            }
            return Err(error);
        }
        validate_execution_action_binding(&stored.action, stored.execution.as_ref())?;
        if stored.execution.as_ref() != Some(expected) {
            return Err(confirmation_execution_mismatch(action_id));
        }
        let executing = self.executing.lock().map_err(|_| registry_locked())?;
        if executing.contains(action_id) {
            self.cancel_requested
                .lock()
                .map_err(|_| registry_locked())?
                .insert(action_id.to_string());
            return Err(confirmation_in_use(action_id));
        }
        actions.remove(action_id);
        Ok(stored)
    }

    /// Release a claimed action after execution. Keep it for retryable fix
    /// failures; remove it only after the mutation commits successfully.
    pub fn finish_claim(&self, action_id: &str, consume: bool) -> Result<(), BackendError> {
        self.finish_claim_with_disposition(action_id, consume)
            .map(|_| ())
    }

    /// Release a claimed action and report whether a concurrent cancellation
    /// arrived while the caller held the claim. Existing callers retain the
    /// legacy unit-returning `finish_claim` API; flows that enqueue work before
    /// releasing their claim can use this disposition to keep cancellation
    /// from being silently swallowed.
    pub fn finish_claim_with_disposition(
        &self,
        action_id: &str,
        consume: bool,
    ) -> Result<ConfirmationClaimDisposition, BackendError> {
        let mut actions = self.actions.lock().map_err(|_| registry_locked())?;
        let mut executing = self.executing.lock().map_err(|_| registry_locked())?;
        let mut cancel_requested = self
            .cancel_requested
            .lock()
            .map_err(|_| registry_locked())?;
        executing.remove(action_id);
        let was_cancel_requested = cancel_requested.remove(action_id);
        if consume || was_cancel_requested {
            actions.remove(action_id).ok_or_else(|| {
                BackendError::new(
                    "CONFIRMATION_NOT_FOUND",
                    "The pending action was not found or has already been handled.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "actionId": action_id }))
            })?;
        }
        Ok(if was_cancel_requested {
            ConfirmationClaimDisposition::CancelRequested
        } else {
            ConfirmationClaimDisposition::Completed
        })
    }

    /// Consume an action only after its side effect has completed. Keeping
    /// confirmation entries until then lets callers retry after validation,
    /// Git, or filesystem failures without losing the user's approval.
    pub fn consume(&self, action_id: &str) -> Result<StoredPendingAction, BackendError> {
        let mut actions = self.actions.lock().map_err(|_| {
            BackendError::new(
                "CONFIRMATION_REGISTRY_LOCKED",
                "Confirmation registry is unavailable.",
                true,
                false,
            )
        })?;
        actions.remove(action_id).ok_or_else(|| {
            BackendError::new(
                "CONFIRMATION_NOT_FOUND",
                "The pending action was not found or has already been handled.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "actionId": action_id }))
        })
    }

    /// Remove a workflow confirmation only when the complete persisted/runtime
    /// binding still belongs to the asserted run. A colliding foreign action id
    /// is deliberately left untouched.
    pub fn cancel_workflow_binding(
        &self,
        context: &crate::models::paths::ProjectContext,
        run: &crate::models::workflow::WorkflowRun,
        pending: &crate::models::workflow::WorkflowPendingAction,
    ) -> Result<bool, BackendError> {
        let mut actions = self.actions.lock().map_err(|_| registry_locked())?;
        let executing = self.executing.lock().map_err(|_| registry_locked())?;
        let matches = actions.get(&pending.id).is_some_and(|stored| {
            stored.action.id == pending.id
                && stored.action.action_type == pending.action_type
                && stored.action.risk_level == pending.risk_level
                && stored.action.affected_paths == pending.affected_paths
                && stored.action.checkpoint_hash == pending.checkpoint_hash
                && workflow_execution_matches(
                    &run.kind,
                    stored.execution.as_ref(),
                    context,
                    run,
                    pending,
                )
        });
        if !matches {
            return Ok(false);
        }
        if executing.contains(&pending.id) {
            self.cancel_requested
                .lock()
                .map_err(|_| registry_locked())?
                .insert(pending.id.clone());
            return Err(confirmation_in_use(&pending.id));
        }
        if matches {
            actions.remove(&pending.id);
        }
        Ok(matches)
    }

    /// Removes only the exact workflow action published by a runner. This is
    /// used when publication succeeds but the task cannot enter its waiting
    /// state (for example because cancellation won the race).
    pub fn remove_exact_execution(
        &self,
        action_id: &str,
        execution: &ConfirmationExecution,
    ) -> Result<bool, BackendError> {
        let mut actions = self.actions.lock().map_err(|_| registry_locked())?;
        let executing = self.executing.lock().map_err(|_| registry_locked())?;
        if executing.contains(action_id) {
            return Err(confirmation_in_use(action_id));
        }
        let matches = actions
            .get(action_id)
            .is_some_and(|stored| stored.execution.as_ref() == Some(execution));
        if matches {
            actions.remove(action_id);
        }
        Ok(matches)
    }
}

pub fn workflow_execution_matches(
    kind: &crate::models::workflow::WorkflowKind,
    execution: Option<&ConfirmationExecution>,
    context: &crate::models::paths::ProjectContext,
    run: &crate::models::workflow::WorkflowRun,
    pending: &crate::models::workflow::WorkflowPendingAction,
) -> bool {
    let matches_binding =
        |project_id: &str,
         root_path: &str,
         identity_key: &str,
         identity_revision: &str,
         bound_task_id: &str,
         action_id: &str,
         candidate: &crate::models::workflow::WorkflowCandidateReference| {
            project_id == context.project_id
                && canonical_roots_match(root_path, &context.root.to_string_lossy())
                && identity_key == run.canonical_identity_key
                && identity_revision == run.identity_revision
                && bound_task_id == run.task_id
                && action_id == pending.id
                && pending.candidate.as_ref() == Some(candidate)
        };
    match (kind, execution) {
        (
            crate::models::workflow::WorkflowKind::HealthCheck,
            Some(ConfirmationExecution::AgentLintRepairReview(binding)),
        ) => {
            matches!(
                run.operation,
                crate::models::workflow::WorkflowOperation::AgentLintRepair { .. }
            ) && binding.project_id == context.project_id
                && canonical_roots_match(&binding.root_path, &context.root.to_string_lossy())
                && binding.canonical_identity_key == run.canonical_identity_key
                && binding.identity_revision == run.identity_revision
                && binding.task_id == run.task_id
                && binding.action_id == pending.id
                && pending.checkpoint_hash.as_deref() == Some(binding.checkpoint_hash.as_str())
                && pending.candidate.as_ref().is_some_and(|candidate| {
                    matches!(
                        candidate,
                        crate::models::workflow::WorkflowCandidateReference::TaskOwned {
                            candidate_id
                        } if candidate_id == &binding.candidate_id
                    )
                })
        }
        (
            crate::models::workflow::WorkflowKind::GenerateContent,
            Some(ConfirmationExecution::GenerateContentOverwrite {
                project_id,
                root_path,
                canonical_identity_key,
                identity_revision,
                task_id,
                action_id,
                candidate,
            }),
        ) => matches_binding(
            project_id,
            root_path,
            canonical_identity_key,
            identity_revision,
            task_id,
            action_id,
            candidate,
        ),
        (
            crate::models::workflow::WorkflowKind::UpdateWiki,
            Some(ConfirmationExecution::UpdateWikiReview {
                project_id,
                root_path,
                canonical_identity_key,
                identity_revision,
                task_id,
                action_id,
                candidate,
            }),
        ) => matches_binding(
            project_id,
            root_path,
            canonical_identity_key,
            identity_revision,
            task_id,
            action_id,
            candidate,
        ),
        _ => false,
    }
}

fn registry_locked() -> BackendError {
    BackendError::new(
        "CONFIRMATION_REGISTRY_LOCKED",
        "Confirmation registry is unavailable.",
        true,
        false,
    )
}

fn confirmation_in_use(action_id: &str) -> BackendError {
    BackendError::new(
        "CONFIRMATION_IN_USE",
        "This confirmation is already being executed.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "actionId": action_id }))
}

fn confirmation_exact_execution_required(action_id: &str) -> BackendError {
    BackendError::new(
        "CONFIRMATION_EXACT_EXECUTION_REQUIRED",
        "This confirmation requires its exact backend execution binding.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "actionId": action_id }))
}

fn confirmation_execution_mismatch(action_id: &str) -> BackendError {
    BackendError::new(
        "CONFIRMATION_EXECUTION_MISMATCH",
        "The pending action no longer matches the asserted backend execution binding.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "actionId": action_id }))
}

fn validate_execution_action_binding(
    action: &PendingAction,
    execution: Option<&ConfirmationExecution>,
) -> Result<(), BackendError> {
    if let Some(ConfirmationExecution::AgentLintRepairReview(binding)) = execution {
        let checkpoint_matches =
            action.checkpoint_hash.as_deref() == Some(binding.checkpoint_hash.as_str());
        if binding.action_id != action.id || !checkpoint_matches {
            return Err(confirmation_execution_mismatch(&action.id));
        }
    }
    Ok(())
}

fn confirmation_id_conflict(action_id: &str) -> BackendError {
    BackendError::new(
        "CONFIRMATION_ID_CONFLICT",
        "A different confirmation with this action id is already pending.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "actionId": action_id }))
}

fn restoration_binding_matches(
    existing: Option<&ConfirmationExecution>,
    replacement: &ConfirmationExecution,
) -> bool {
    let same_binding =
        |existing_root: &str,
         existing_identity_key: &str,
         existing_revision: &str,
         existing_task: &str,
         existing_action: &str,
         existing_candidate: &crate::models::workflow::WorkflowCandidateReference,
         root: &str,
         identity_key: &str,
         revision: &str,
         task: &str,
         action: &str,
         candidate: &crate::models::workflow::WorkflowCandidateReference| {
            existing_identity_key == identity_key
                && existing_revision == revision
                && existing_task == task
                && existing_action == action
                && existing_candidate == candidate
                && canonical_roots_match(existing_root, root)
        };
    match (existing, replacement) {
        (
            Some(ConfirmationExecution::AgentLintRepairReview(existing)),
            ConfirmationExecution::AgentLintRepairReview(replacement),
        ) => {
            existing.canonical_identity_key == replacement.canonical_identity_key
                && existing.identity_revision == replacement.identity_revision
                && existing.task_id == replacement.task_id
                && existing.round == replacement.round
                && existing.candidate_id == replacement.candidate_id
                && existing.candidate_revision == replacement.candidate_revision
                && existing.manifest_revision == replacement.manifest_revision
                && existing.baseline_fingerprint == replacement.baseline_fingerprint
                && existing.checkpoint_hash == replacement.checkpoint_hash
                && existing.action_id == replacement.action_id
                && existing.action_revision == replacement.action_revision
                && canonical_roots_match(&existing.root_path, &replacement.root_path)
        }
        (
            Some(ConfirmationExecution::GenerateContentOverwrite {
                root_path: existing_root,
                canonical_identity_key: existing_identity_key,
                identity_revision: existing_revision,
                task_id: existing_task,
                action_id: existing_action,
                candidate: existing_candidate,
                ..
            }),
            ConfirmationExecution::GenerateContentOverwrite {
                root_path,
                canonical_identity_key,
                identity_revision,
                task_id,
                action_id,
                candidate,
                ..
            },
        ) => same_binding(
            existing_root,
            existing_identity_key,
            existing_revision,
            existing_task,
            existing_action,
            existing_candidate,
            root_path,
            canonical_identity_key,
            identity_revision,
            task_id,
            action_id,
            candidate,
        ),
        (
            Some(ConfirmationExecution::UpdateWikiReview {
                root_path: existing_root,
                canonical_identity_key: existing_identity_key,
                identity_revision: existing_revision,
                task_id: existing_task,
                action_id: existing_action,
                candidate: existing_candidate,
                ..
            }),
            ConfirmationExecution::UpdateWikiReview {
                root_path,
                canonical_identity_key,
                identity_revision,
                task_id,
                action_id,
                candidate,
                ..
            },
        ) => same_binding(
            existing_root,
            existing_identity_key,
            existing_revision,
            existing_task,
            existing_action,
            existing_candidate,
            root_path,
            canonical_identity_key,
            identity_revision,
            task_id,
            action_id,
            candidate,
        ),
        _ => false,
    }
}

fn canonical_roots_match(left: &str, right: &str) -> bool {
    let Ok(left) = std::fs::canonicalize(left) else {
        return false;
    };
    let Ok(right) = std::fs::canonicalize(right) else {
        return false;
    };
    left == right
}

fn reject_if_expired(action: &PendingAction) -> Result<(), BackendError> {
    let Some(expires_at) = action.expires_at.as_ref() else {
        return Ok(());
    };
    let expires_at = chrono::DateTime::parse_from_rfc3339(expires_at).map_err(|err| {
        BackendError::new("CONFIRMATION_EXPIRY_INVALID", err.to_string(), true, true)
            .with_details(serde_json::json!({ "actionId": action.id }))
    })?;
    if expires_at < chrono::Utc::now() {
        return Err(BackendError::new(
            "CONFIRMATION_EXPIRED",
            "The pending action has expired. Start the operation again.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "actionId": action.id })));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ActionPreview, AgentLintRepairReviewBinding, ConfirmationClaimDisposition,
        ConfirmationExecution, ConfirmationRegistration, ConfirmationRegistry, ConfirmationStatus,
        PendingAction, PendingActionType, RiskLevel,
    };
    use crate::models::paths::ProjectContext;
    use crate::models::workflow::{
        UpdateWikiMode, WorkflowCandidateReference, WorkflowDisplayStatus, WorkflowKind,
        WorkflowPendingAction, WorkflowPersistenceMode, WorkflowRun, WorkflowScope,
    };
    use serde_json::json;

    #[test]
    fn serializes_confirmation_enums_as_stable_strings() {
        assert_eq!(
            serde_json::to_string(&PendingActionType::AgentAutoFix).unwrap(),
            "\"agent_auto_fix\""
        );
        assert_eq!(
            serde_json::to_string(&RiskLevel::Destructive).unwrap(),
            "\"destructive\""
        );
    }

    #[test]
    fn serializes_pending_action_with_camel_case_fields() {
        let action = PendingAction {
            id: "action-1".to_string(),
            action_type: PendingActionType::MergeConflict,
            title: "Resolve conflict".to_string(),
            message: "A generated page conflicts with external edits.".to_string(),
            risk_level: RiskLevel::High,
            affected_paths: vec!["wiki/concepts/agent.md".to_string()],
            preview: Some(ActionPreview {
                summary: "One conflicting file".to_string(),
                before: Some("current".to_string()),
                after: Some("generated".to_string()),
                diff: Some("-current\n+generated".to_string()),
            }),
            expires_at: Some("2026-06-19T00:00:00Z".to_string()),
            checkpoint_hash: Some("abc123".to_string()),
        };

        let value = serde_json::to_value(action).unwrap();

        assert_eq!(value["actionType"], json!("merge_conflict"));
        assert_eq!(value["riskLevel"], json!("high"));
        assert_eq!(value["affectedPaths"][0], json!("wiki/concepts/agent.md"));
        assert_eq!(value["expiresAt"], json!("2026-06-19T00:00:00Z"));
        assert_eq!(value["preview"]["summary"], json!("One conflicting file"));
        assert_eq!(value["checkpointHash"], json!("abc123"));
        assert!(value.get("action_type").is_none());
    }

    #[test]
    fn pending_action_serializes_absent_checkpoint_hash_as_null() {
        // The frontend renders "Checkpoint: available" when checkpointHash is
        // non-null and "not created yet" when null. A None must therefore
        // serialize to JSON null (not be omitted), or the !== null check would
        // misfire on undefined.
        let action = PendingAction {
            id: "action-2".to_string(),
            action_type: PendingActionType::OverwriteFile,
            title: "Overwrite".to_string(),
            message: "Overwrite later.".to_string(),
            risk_level: RiskLevel::High,
            affected_paths: Vec::new(),
            preview: None,
            expires_at: None,
            checkpoint_hash: None,
        };
        let value = serde_json::to_value(&action).unwrap();
        assert!(
            value.get("checkpointHash").is_some(),
            "field must be present"
        );
        assert!(value["checkpointHash"].is_null());

        // Round-trips back to None.
        let restored: PendingAction = serde_json::from_value(value).unwrap();
        assert!(restored.checkpoint_hash.is_none());
    }

    #[test]
    fn confirmation_registry_resumes_only_backend_stored_actions() {
        let registry = ConfirmationRegistry::default();
        let action = PendingAction {
            id: "action-1".to_string(),
            action_type: PendingActionType::OverwriteFile,
            title: "Overwrite page".to_string(),
            message: "Overwrite wiki/concepts/agent.md".to_string(),
            risk_level: RiskLevel::High,
            affected_paths: vec!["wiki/concepts/agent.md".to_string()],
            preview: None,
            expires_at: None,
            checkpoint_hash: None,
        };

        registry.register(action.clone()).unwrap();

        let confirmed = registry
            .confirm("action-1", ConfirmationStatus::Confirmed)
            .expect("registered action should confirm");
        assert_eq!(confirmed.action, action);

        let err = registry
            .confirm("action-1", ConfirmationStatus::Confirmed)
            .expect_err("actions cannot be replayed after confirmation");
        assert_eq!(err.code, "CONFIRMATION_NOT_FOUND");
    }

    #[test]
    fn confirmation_registry_rejects_expired_actions() {
        let registry = ConfirmationRegistry::default();
        let action = PendingAction {
            id: "expired-action".to_string(),
            action_type: PendingActionType::DeleteFile,
            title: "Delete page".to_string(),
            message: "Delete wiki/page.md".to_string(),
            risk_level: RiskLevel::Destructive,
            affected_paths: vec!["wiki/page.md".to_string()],
            preview: None,
            expires_at: Some("2000-01-01T00:00:00Z".to_string()),
            checkpoint_hash: None,
        };
        registry.register(action).unwrap();

        let err = registry
            .confirm("expired-action", ConfirmationStatus::Confirmed)
            .expect_err("expired actions must fail safely");
        assert_eq!(err.code, "CONFIRMATION_EXPIRED");
    }

    #[test]
    fn cancellation_during_claim_is_replayed_after_execution_releases() {
        let registry = ConfirmationRegistry::default();
        let action = PendingAction {
            id: "claimed-action".to_string(),
            action_type: PendingActionType::AgentAutoFix,
            title: "Run fix".to_string(),
            message: "Apply the approved fix".to_string(),
            risk_level: RiskLevel::High,
            affected_paths: Vec::new(),
            preview: None,
            expires_at: None,
            checkpoint_hash: None,
        };
        registry.register(action).unwrap();
        registry.claim("claimed-action").unwrap();

        let err = registry
            .confirm("claimed-action", ConfirmationStatus::Cancelled)
            .expect_err("cancellation should wait for the active claim");
        assert_eq!(err.code, "CONFIRMATION_IN_USE");

        // The cancellation request is remembered, so a failed execution
        // cannot leave an orphaned confirmation after the project is reset.
        registry.finish_claim("claimed-action", false).unwrap();
        let err = registry
            .confirm("claimed-action", ConfirmationStatus::Confirmed)
            .expect_err("deferred cancellation should consume the action");
        assert_eq!(err.code, "CONFIRMATION_NOT_FOUND");
    }

    #[test]
    fn failed_claimed_execution_keeps_the_approval_retryable() {
        let registry = ConfirmationRegistry::default();
        let action = PendingAction {
            id: "retryable-claim".to_string(),
            action_type: PendingActionType::EnableCompatibleProject,
            title: "Enable compatibility".to_string(),
            message: "Enable compatibility".to_string(),
            risk_level: RiskLevel::High,
            affected_paths: vec![".app/compat/purpose.md".to_string()],
            preview: None,
            expires_at: None,
            checkpoint_hash: None,
        };
        registry.register(action).unwrap();

        registry.claim("retryable-claim").unwrap();
        registry.finish_claim("retryable-claim", false).unwrap();
        registry
            .claim("retryable-claim")
            .expect("a failed side effect must not consume approval");
        registry.finish_claim("retryable-claim", true).unwrap();

        assert_eq!(
            registry.claim("retryable-claim").unwrap_err().code,
            "CONFIRMATION_NOT_FOUND"
        );
    }

    fn agent_lint_action() -> PendingAction {
        PendingAction {
            id: "agent-lint-prepare-1".into(),
            action_type: PendingActionType::AgentAutoFix,
            title: "Repair selected lint findings".into(),
            message: "Start the approved Agent lint repair".into(),
            risk_level: RiskLevel::High,
            affected_paths: vec!["wiki/page.md".into()],
            preview: None,
            expires_at: None,
            checkpoint_hash: None,
        }
    }

    fn agent_lint_binding() -> ConfirmationExecution {
        ConfirmationExecution::AgentLintRepairStart {
            project_id: "project-a".into(),
            root_path: "C:/wiki".into(),
            canonical_identity_key: "identity-a".into(),
            identity_revision: "identity-revision-a".into(),
            preparation_id: "prepare-1".into(),
            preparation_revision: "prepare-revision-1".into(),
            report_id: "health-report-1".into(),
            selection_revision: "selection-revision-1".into(),
            selected_finding_ids: vec!["finding-a".into()],
            route: crate::models::workflow::WorkflowRoute::Agent {
                agent: crate::models::agent::AgentKind::Codex,
                model: None,
                route_revision: "route-revision-1".into(),
            },
            skill: crate::models::lint::WikiLintSkillRef::builtin(),
            authorized_path_hashes: [("wiki/page.md".into(), Some("a".repeat(64)))]
                .into_iter()
                .collect(),
            baseline_fingerprint: "baseline-1".into(),
            expected_git_head: "b".repeat(40),
        }
    }

    #[test]
    fn agent_lint_registration_is_idempotent_only_for_the_exact_action_and_binding() {
        let registry = ConfirmationRegistry::default();
        let action = agent_lint_action();
        let binding = agent_lint_binding();

        assert_eq!(
            registry
                .register_idempotent_with_execution(action.clone(), binding.clone())
                .unwrap(),
            super::ConfirmationRegistration::Registered
        );
        assert_eq!(
            registry
                .register_idempotent_with_execution(action.clone(), binding.clone())
                .unwrap(),
            super::ConfirmationRegistration::Existing
        );

        let mut changed_action = action.clone();
        changed_action.affected_paths.push("wiki/forged.md".into());
        assert_eq!(
            registry
                .register_idempotent_with_execution(changed_action, binding.clone())
                .unwrap_err()
                .code,
            "CONFIRMATION_ID_CONFLICT"
        );

        let mut changed_binding = binding;
        let ConfirmationExecution::AgentLintRepairStart {
            selection_revision, ..
        } = &mut changed_binding
        else {
            unreachable!()
        };
        *selection_revision = "forged-selection".into();
        assert_eq!(
            registry
                .register_idempotent_with_execution(action, changed_binding)
                .unwrap_err()
                .code,
            "CONFIRMATION_ID_CONFLICT"
        );
    }

    #[test]
    fn agent_lint_exact_binding_can_be_claimed_only_once_and_is_consumed_once() {
        let registry = ConfirmationRegistry::default();
        let action = agent_lint_action();
        let binding = agent_lint_binding();
        registry
            .register_idempotent_with_execution(action, binding.clone())
            .unwrap();

        let claimed = registry.claim("agent-lint-prepare-1").unwrap();
        assert_eq!(claimed.execution, Some(binding));
        assert_eq!(
            registry.claim("agent-lint-prepare-1").unwrap_err().code,
            "CONFIRMATION_IN_USE"
        );
        registry.finish_claim("agent-lint-prepare-1", true).unwrap();
        assert_eq!(
            registry.claim("agent-lint-prepare-1").unwrap_err().code,
            "CONFIRMATION_NOT_FOUND"
        );
    }

    #[test]
    fn expiring_idempotent_registration_reuses_the_original_expiry() {
        let registry = ConfirmationRegistry::default();
        let action = agent_lint_action();
        let binding = agent_lint_binding();

        let first = registry
            .register_idempotent_expiring_with_execution(
                action.clone(),
                binding.clone(),
                "2099-01-01T00:15:00Z".into(),
            )
            .unwrap();
        let repeated = registry
            .register_idempotent_expiring_with_execution(
                action,
                binding,
                "2099-01-01T00:30:00Z".into(),
            )
            .unwrap();

        assert_eq!(first.registration, ConfirmationRegistration::Registered);
        assert_eq!(repeated.registration, ConfirmationRegistration::Existing);
        assert_eq!(first.stored, repeated.stored);
        assert_eq!(
            repeated.stored.action.expires_at.as_deref(),
            Some("2099-01-01T00:15:00Z")
        );
    }

    #[test]
    fn expired_idempotent_registration_is_replaced_with_a_fresh_expiry() {
        let registry = ConfirmationRegistry::default();
        let mut expired = agent_lint_action();
        expired.expires_at = Some("2000-01-01T00:00:00Z".into());
        let binding = agent_lint_binding();
        registry
            .register_with_execution(expired, Some(binding.clone()))
            .unwrap();

        let refreshed = registry
            .register_idempotent_expiring_with_execution(
                agent_lint_action(),
                binding,
                "2099-01-01T00:15:00Z".into(),
            )
            .unwrap();

        assert_eq!(refreshed.registration, ConfirmationRegistration::Registered);
        assert_eq!(
            refreshed.stored.action.expires_at.as_deref(),
            Some("2099-01-01T00:15:00Z")
        );
    }

    #[test]
    fn finish_claim_reports_when_concurrent_cancellation_won() {
        let registry = ConfirmationRegistry::default();
        registry
            .register_idempotent_with_execution(agent_lint_action(), agent_lint_binding())
            .unwrap();
        registry.claim("agent-lint-prepare-1").unwrap();
        assert_eq!(
            registry
                .confirm("agent-lint-prepare-1", ConfirmationStatus::Cancelled)
                .unwrap_err()
                .code,
            "CONFIRMATION_IN_USE"
        );

        assert_eq!(
            registry
                .finish_claim_with_disposition("agent-lint-prepare-1", true)
                .unwrap(),
            ConfirmationClaimDisposition::CancelRequested
        );
        assert_eq!(
            registry.peek("agent-lint-prepare-1").unwrap_err().code,
            "CONFIRMATION_NOT_FOUND"
        );
    }

    fn agent_lint_review_action() -> PendingAction {
        PendingAction {
            id: "agent-lint-review-action-1".into(),
            action_type: PendingActionType::MergeConflict,
            title: "Review dangerous Agent lint repair changes".into(),
            message: "Review the exact task-owned repair candidate".into(),
            risk_level: RiskLevel::Destructive,
            affected_paths: vec!["wiki/page.md".into()],
            preview: None,
            expires_at: Some("2099-01-01T00:15:00Z".into()),
            checkpoint_hash: Some("c".repeat(40)),
        }
    }

    fn agent_lint_review_binding(root_path: String) -> AgentLintRepairReviewBinding {
        AgentLintRepairReviewBinding {
            project_id: "runtime-project-a".into(),
            root_path,
            canonical_identity_key: "identity-key-a".into(),
            identity_revision: "identity-revision-a".into(),
            task_id: "repair-task-a".into(),
            round: 2,
            candidate_id: "candidate-a".into(),
            candidate_revision: "candidate-revision-a".into(),
            manifest_revision: "manifest-revision-a".into(),
            baseline_fingerprint: "baseline-fingerprint-a".into(),
            checkpoint_hash: "c".repeat(40),
            action_id: "agent-lint-review-action-1".into(),
            action_revision: "action-revision-a".into(),
        }
    }

    fn agent_lint_review_execution(root_path: String) -> ConfirmationExecution {
        ConfirmationExecution::AgentLintRepairReview(agent_lint_review_binding(root_path))
    }

    fn agent_lint_review_stale_executions(
        execution: &ConfirmationExecution,
    ) -> Vec<ConfirmationExecution> {
        let mutations: [fn(&mut AgentLintRepairReviewBinding); 11] = [
            |binding| binding.canonical_identity_key = "other-identity".into(),
            |binding| binding.identity_revision = "other-identity-revision".into(),
            |binding| binding.task_id = "other-task".into(),
            |binding| binding.round = 3,
            |binding| binding.candidate_id = "other-candidate".into(),
            |binding| binding.candidate_revision = "other-candidate-revision".into(),
            |binding| binding.manifest_revision = "other-manifest-revision".into(),
            |binding| binding.baseline_fingerprint = "other-baseline".into(),
            |binding| binding.checkpoint_hash = "d".repeat(40),
            |binding| binding.action_id = "other-action".into(),
            |binding| binding.action_revision = "other-action-revision".into(),
        ];
        mutations
            .into_iter()
            .map(|mutate| {
                let mut mismatch = execution.clone();
                let ConfirmationExecution::AgentLintRepairReview(binding) = &mut mismatch else {
                    unreachable!()
                };
                mutate(binding);
                mismatch
            })
            .collect()
    }

    #[test]
    fn agent_lint_review_binding_round_trips_strict_camel_case_and_rejects_unknown_fields() {
        let root = tempfile::tempdir().unwrap();
        let execution = agent_lint_review_execution(root.path().to_string_lossy().into_owned());
        let mut value = serde_json::to_value(&execution).unwrap();

        assert_eq!(value["kind"], json!("agent_lint_repair_review"));
        assert_eq!(value["taskId"], json!("repair-task-a"));
        assert_eq!(value["candidateRevision"], json!("candidate-revision-a"));
        assert_eq!(value["manifestRevision"], json!("manifest-revision-a"));
        assert_eq!(
            value["baselineFingerprint"],
            json!("baseline-fingerprint-a")
        );
        assert_eq!(value["actionRevision"], json!("action-revision-a"));
        assert_eq!(
            serde_json::from_value::<ConfirmationExecution>(value.clone()).unwrap(),
            execution
        );

        value
            .as_object_mut()
            .unwrap()
            .insert("futureAuthority".into(), json!(true));
        assert!(serde_json::from_value::<ConfirmationExecution>(value).is_err());
    }

    #[test]
    fn agent_lint_review_registration_is_idempotent_only_for_every_exact_revision() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().to_string_lossy().into_owned();
        let registry = ConfirmationRegistry::default();
        let action = agent_lint_review_action();
        let execution = agent_lint_review_execution(root_path);

        assert_eq!(
            registry
                .register_idempotent_with_execution(action.clone(), execution.clone())
                .unwrap(),
            ConfirmationRegistration::Registered
        );
        assert_eq!(
            registry
                .register_idempotent_with_execution(action.clone(), execution.clone())
                .unwrap(),
            ConfirmationRegistration::Existing
        );

        for mismatch in agent_lint_review_stale_executions(&execution) {
            let ConfirmationExecution::AgentLintRepairReview(binding) = &mismatch else {
                unreachable!()
            };
            let expected_code = if binding.action_id != action.id
                || action.checkpoint_hash.as_deref() != Some(binding.checkpoint_hash.as_str())
            {
                "CONFIRMATION_EXECUTION_MISMATCH"
            } else {
                "CONFIRMATION_ID_CONFLICT"
            };
            assert_eq!(
                registry
                    .register_idempotent_with_execution(action.clone(), mismatch)
                    .unwrap_err()
                    .code,
                expected_code
            );
            assert_eq!(
                registry.peek(&action.id).unwrap().execution,
                Some(execution.clone())
            );
        }
    }

    #[test]
    fn agent_lint_review_registration_rejects_cross_bound_action_and_checkpoint() {
        let root = tempfile::tempdir().unwrap();
        let execution = agent_lint_review_execution(root.path().to_string_lossy().into_owned());

        let mut wrong_action = agent_lint_review_action();
        wrong_action.id = "different-visible-action".into();
        let registry = ConfirmationRegistry::default();
        assert_eq!(
            registry
                .register_idempotent_with_execution(wrong_action.clone(), execution.clone())
                .unwrap_err()
                .code,
            "CONFIRMATION_EXECUTION_MISMATCH"
        );
        assert_eq!(
            registry.peek(&wrong_action.id).unwrap_err().code,
            "CONFIRMATION_NOT_FOUND"
        );

        let mut wrong_checkpoint = agent_lint_review_action();
        wrong_checkpoint.checkpoint_hash = Some("d".repeat(40));
        let registry = ConfirmationRegistry::default();
        assert_eq!(
            registry
                .register_idempotent_with_execution(wrong_checkpoint.clone(), execution)
                .unwrap_err()
                .code,
            "CONFIRMATION_EXECUTION_MISMATCH"
        );
        assert_eq!(
            registry.peek(&wrong_checkpoint.id).unwrap_err().code,
            "CONFIRMATION_NOT_FOUND"
        );
    }

    #[test]
    fn agent_lint_review_restore_rebinds_only_runtime_project_id() {
        let root = tempfile::tempdir().unwrap();
        let other_root = tempfile::tempdir().unwrap();
        let action = agent_lint_review_action();
        let original = agent_lint_review_execution(root.path().to_string_lossy().into_owned());

        let registry = ConfirmationRegistry::default();
        registry
            .register_with_execution(action.clone(), Some(original.clone()))
            .unwrap();
        let mut rebound = original.clone();
        let ConfirmationExecution::AgentLintRepairReview(binding) = &mut rebound else {
            unreachable!()
        };
        binding.project_id = "runtime-project-b".into();
        let mut reconstructed = action.clone();
        reconstructed.title = "Reconstructed title must not replace persisted review facts".into();
        registry
            .restore_with_execution(reconstructed, rebound.clone())
            .unwrap();
        assert_eq!(registry.peek(&action.id).unwrap().action, action);
        assert_eq!(registry.peek(&action.id).unwrap().execution, Some(rebound));

        let mut mismatches = agent_lint_review_stale_executions(&original);
        let mut other_root_execution = original.clone();
        let ConfirmationExecution::AgentLintRepairReview(binding) = &mut other_root_execution
        else {
            unreachable!()
        };
        binding.root_path = other_root.path().to_string_lossy().into_owned();
        mismatches.push(other_root_execution);

        for mismatch in mismatches {
            let registry = ConfirmationRegistry::default();
            registry
                .register_with_execution(action.clone(), Some(original.clone()))
                .unwrap();
            let error = registry
                .restore_with_execution(action.clone(), mismatch)
                .unwrap_err();
            assert!(matches!(
                error.code.as_str(),
                "CONFIRMATION_ID_CONFLICT" | "CONFIRMATION_EXECUTION_MISMATCH"
            ));
            assert_eq!(registry.peek(&action.id).unwrap().action, action);
            assert_eq!(
                registry.peek(&action.id).unwrap().execution,
                Some(original.clone())
            );
        }
    }

    #[test]
    fn agent_lint_review_requires_exact_claim_and_cannot_replay_after_consumption() {
        let root = tempfile::tempdir().unwrap();
        let registry = ConfirmationRegistry::default();
        let action = agent_lint_review_action();
        let execution = agent_lint_review_execution(root.path().to_string_lossy().into_owned());
        registry
            .register_idempotent_with_execution(action.clone(), execution.clone())
            .unwrap();

        assert_eq!(
            registry.claim(&action.id).unwrap_err().code,
            "CONFIRMATION_EXACT_EXECUTION_REQUIRED"
        );
        assert_eq!(
            registry
                .confirm(&action.id, ConfirmationStatus::Confirmed)
                .unwrap_err()
                .code,
            "CONFIRMATION_EXACT_EXECUTION_REQUIRED"
        );
        assert_eq!(
            registry
                .confirm(&action.id, ConfirmationStatus::Cancelled)
                .unwrap_err()
                .code,
            "CONFIRMATION_EXACT_EXECUTION_REQUIRED"
        );

        let mut mismatch = execution.clone();
        let ConfirmationExecution::AgentLintRepairReview(binding) = &mut mismatch else {
            unreachable!()
        };
        binding.manifest_revision = "forged-manifest".into();
        assert_eq!(
            registry
                .claim_exact_execution(&action.id, &mismatch)
                .unwrap_err()
                .code,
            "CONFIRMATION_EXECUTION_MISMATCH"
        );

        let claimed = registry
            .claim_exact_execution(&action.id, &execution)
            .unwrap();
        assert_eq!(claimed.execution, Some(execution.clone()));
        assert_eq!(
            registry
                .claim_exact_execution(&action.id, &execution)
                .unwrap_err()
                .code,
            "CONFIRMATION_IN_USE"
        );
        registry.finish_claim(&action.id, true).unwrap();
        assert_eq!(
            registry
                .claim_exact_execution(&action.id, &execution)
                .unwrap_err()
                .code,
            "CONFIRMATION_NOT_FOUND"
        );
    }

    #[test]
    fn agent_lint_review_exact_cancel_wins_a_claim_and_mismatch_never_consumes() {
        let root = tempfile::tempdir().unwrap();
        let registry = ConfirmationRegistry::default();
        let action = agent_lint_review_action();
        let execution = agent_lint_review_execution(root.path().to_string_lossy().into_owned());
        registry
            .register_idempotent_with_execution(action.clone(), execution.clone())
            .unwrap();

        let mut mismatch = execution.clone();
        let ConfirmationExecution::AgentLintRepairReview(binding) = &mut mismatch else {
            unreachable!()
        };
        binding.checkpoint_hash = "d".repeat(40);
        assert_eq!(
            registry
                .cancel_exact_execution(&action.id, &mismatch)
                .unwrap_err()
                .code,
            "CONFIRMATION_EXECUTION_MISMATCH"
        );
        assert_eq!(
            registry.peek(&action.id).unwrap().execution,
            Some(execution.clone())
        );

        registry
            .claim_exact_execution(&action.id, &execution)
            .unwrap();
        assert_eq!(
            registry
                .cancel_exact_execution(&action.id, &execution)
                .unwrap_err()
                .code,
            "CONFIRMATION_IN_USE"
        );
        assert_eq!(
            registry
                .finish_claim_with_disposition(&action.id, false)
                .unwrap(),
            ConfirmationClaimDisposition::CancelRequested
        );
        assert_eq!(
            registry
                .claim_exact_execution(&action.id, &execution)
                .unwrap_err()
                .code,
            "CONFIRMATION_NOT_FOUND"
        );
    }

    fn workflow_action(id: &str) -> PendingAction {
        PendingAction {
            id: id.into(),
            action_type: PendingActionType::OverwriteFile,
            title: "Original title".into(),
            message: "Original message".into(),
            risk_level: RiskLevel::High,
            affected_paths: vec!["wiki/page.md".into()],
            preview: None,
            expires_at: None,
            checkpoint_hash: Some("checkpoint".into()),
        }
    }

    fn generate_binding(
        project_id: &str,
        root_path: String,
        task_id: &str,
    ) -> ConfirmationExecution {
        ConfirmationExecution::GenerateContentOverwrite {
            project_id: project_id.into(),
            root_path,
            canonical_identity_key: "identity".into(),
            identity_revision: "revision".into(),
            task_id: task_id.into(),
            action_id: "generate".into(),
            candidate: crate::models::workflow::WorkflowCandidateReference::TaskOwned {
                candidate_id: "candidate-generate".into(),
            },
        }
    }

    fn update_binding(project_id: &str, root_path: String, task_id: &str) -> ConfirmationExecution {
        ConfirmationExecution::UpdateWikiReview {
            project_id: project_id.into(),
            root_path,
            canonical_identity_key: "identity".into(),
            identity_revision: "revision".into(),
            task_id: task_id.into(),
            action_id: "update".into(),
            candidate: crate::models::workflow::WorkflowCandidateReference::TaskOwned {
                candidate_id: "candidate-update".into(),
            },
        }
    }

    fn with_identity(mut binding: ConfirmationExecution, identity: &str) -> ConfirmationExecution {
        match &mut binding {
            ConfirmationExecution::GenerateContentOverwrite {
                canonical_identity_key,
                ..
            }
            | ConfirmationExecution::UpdateWikiReview {
                canonical_identity_key,
                ..
            } => *canonical_identity_key = identity.into(),
            _ => unreachable!(),
        }
        binding
    }

    fn with_action(mut binding: ConfirmationExecution, action: &str) -> ConfirmationExecution {
        match &mut binding {
            ConfirmationExecution::GenerateContentOverwrite { action_id, .. }
            | ConfirmationExecution::UpdateWikiReview { action_id, .. } => {
                *action_id = action.into()
            }
            _ => unreachable!(),
        }
        binding
    }

    fn with_revision(mut binding: ConfirmationExecution, revision: &str) -> ConfirmationExecution {
        match &mut binding {
            ConfirmationExecution::GenerateContentOverwrite {
                identity_revision, ..
            }
            | ConfirmationExecution::UpdateWikiReview {
                identity_revision, ..
            } => *identity_revision = revision.into(),
            _ => unreachable!(),
        }
        binding
    }

    fn with_candidate(
        mut binding: ConfirmationExecution,
        candidate_id: &str,
    ) -> ConfirmationExecution {
        match &mut binding {
            ConfirmationExecution::GenerateContentOverwrite { candidate, .. }
            | ConfirmationExecution::UpdateWikiReview { candidate, .. } => {
                *candidate = crate::models::workflow::WorkflowCandidateReference::TaskOwned {
                    candidate_id: candidate_id.into(),
                }
            }
            _ => unreachable!(),
        }
        binding
    }

    #[test]
    fn workflow_confirmation_restore_rebinds_only_project_id_for_same_root_and_task() {
        let root = tempfile::tempdir().unwrap();
        let alias = root.path().join(".");
        for (id, original, replacement) in [
            (
                "generate",
                generate_binding(
                    "runtime-a",
                    root.path().to_string_lossy().into_owned(),
                    "task-generate",
                ),
                generate_binding(
                    "runtime-b",
                    alias.to_string_lossy().into_owned(),
                    "task-generate",
                ),
            ),
            (
                "update",
                update_binding(
                    "runtime-a",
                    root.path().to_string_lossy().into_owned(),
                    "task-update",
                ),
                update_binding(
                    "runtime-b",
                    alias.to_string_lossy().into_owned(),
                    "task-update",
                ),
            ),
        ] {
            let registry = ConfirmationRegistry::default();
            let original_action = workflow_action(id);
            registry
                .register_with_execution(original_action.clone(), Some(original))
                .unwrap();
            let mut reconstructed = original_action.clone();
            reconstructed.title = "Reconstructed title".into();
            reconstructed.message = "Reconstructed message".into();
            registry
                .restore_with_execution(reconstructed, replacement.clone())
                .unwrap();

            let restored = registry.peek(id).unwrap();
            assert_eq!(restored.action, original_action);
            assert_eq!(restored.execution, Some(replacement));
        }
    }

    #[test]
    fn workflow_confirmation_restore_rejects_root_task_and_variant_mismatch_without_mutation() {
        let root = tempfile::tempdir().unwrap();
        let other_root = tempfile::tempdir().unwrap();
        for (id, original, mismatches) in [
            (
                "generate",
                generate_binding(
                    "runtime-a",
                    root.path().to_string_lossy().into_owned(),
                    "task-generate",
                ),
                vec![
                    generate_binding(
                        "runtime-b",
                        other_root.path().to_string_lossy().into_owned(),
                        "task-generate",
                    ),
                    generate_binding(
                        "runtime-b",
                        root.path().to_string_lossy().into_owned(),
                        "other-task",
                    ),
                    with_identity(
                        generate_binding(
                            "runtime-b",
                            root.path().to_string_lossy().into_owned(),
                            "task-generate",
                        ),
                        "other-identity",
                    ),
                    with_revision(
                        generate_binding(
                            "runtime-b",
                            root.path().to_string_lossy().into_owned(),
                            "task-generate",
                        ),
                        "other-revision",
                    ),
                    with_action(
                        generate_binding(
                            "runtime-b",
                            root.path().to_string_lossy().into_owned(),
                            "task-generate",
                        ),
                        "other-action",
                    ),
                    with_candidate(
                        generate_binding(
                            "runtime-b",
                            root.path().to_string_lossy().into_owned(),
                            "task-generate",
                        ),
                        "other-candidate",
                    ),
                    update_binding(
                        "runtime-b",
                        root.path().to_string_lossy().into_owned(),
                        "task-generate",
                    ),
                ],
            ),
            (
                "update",
                update_binding(
                    "runtime-a",
                    root.path().to_string_lossy().into_owned(),
                    "task-update",
                ),
                vec![
                    update_binding(
                        "runtime-b",
                        other_root.path().to_string_lossy().into_owned(),
                        "task-update",
                    ),
                    update_binding(
                        "runtime-b",
                        root.path().to_string_lossy().into_owned(),
                        "other-task",
                    ),
                    with_identity(
                        update_binding(
                            "runtime-b",
                            root.path().to_string_lossy().into_owned(),
                            "task-update",
                        ),
                        "other-identity",
                    ),
                    with_revision(
                        update_binding(
                            "runtime-b",
                            root.path().to_string_lossy().into_owned(),
                            "task-update",
                        ),
                        "other-revision",
                    ),
                    with_action(
                        update_binding(
                            "runtime-b",
                            root.path().to_string_lossy().into_owned(),
                            "task-update",
                        ),
                        "other-action",
                    ),
                    with_candidate(
                        update_binding(
                            "runtime-b",
                            root.path().to_string_lossy().into_owned(),
                            "task-update",
                        ),
                        "other-candidate",
                    ),
                    generate_binding(
                        "runtime-b",
                        root.path().to_string_lossy().into_owned(),
                        "task-update",
                    ),
                ],
            ),
        ] {
            for mismatch in mismatches {
                let registry = ConfirmationRegistry::default();
                let action = workflow_action(id);
                registry
                    .register_with_execution(action.clone(), Some(original.clone()))
                    .unwrap();
                let error = registry
                    .restore_with_execution(action.clone(), mismatch)
                    .unwrap_err();
                assert_eq!(error.code, "CONFIRMATION_ID_CONFLICT");
                let unchanged = registry.peek(id).unwrap();
                assert_eq!(unchanged.action, action);
                assert_eq!(unchanged.execution, Some(original.clone()));
            }
        }
    }

    #[test]
    fn workflow_confirmation_restore_rejects_an_executing_action() {
        let root = tempfile::tempdir().unwrap();
        let registry = ConfirmationRegistry::default();
        let action = workflow_action("executing");
        let original = update_binding(
            "runtime-a",
            root.path().to_string_lossy().into_owned(),
            "task-update",
        );
        registry
            .register_with_execution(action.clone(), Some(original.clone()))
            .unwrap();
        registry.claim(&action.id).unwrap();

        let error = registry
            .restore_with_execution(
                action.clone(),
                update_binding(
                    "runtime-b",
                    root.path().to_string_lossy().into_owned(),
                    "task-update",
                ),
            )
            .unwrap_err();
        assert_eq!(error.code, "CONFIRMATION_IN_USE");
        let claimed = registry.peek(&action.id).unwrap();
        assert_eq!(claimed.action, action);
        assert_eq!(claimed.execution, Some(original));
    }

    #[test]
    fn workflow_confirmation_restore_is_idempotent_while_the_exact_binding_is_claimed() {
        let root = tempfile::tempdir().unwrap();
        let registry = ConfirmationRegistry::default();
        let action = workflow_action("update");
        let binding = update_binding(
            "runtime-a",
            root.path().to_string_lossy().into_owned(),
            "task-update",
        );
        registry
            .register_with_execution(action.clone(), Some(binding.clone()))
            .unwrap();
        registry.claim(&action.id).unwrap();

        registry
            .restore_with_execution(action.clone(), binding.clone())
            .expect("list/get hydration must be idempotent during the same confirmation claim");
        registry.finish_claim(&action.id, false).unwrap();
        assert_eq!(registry.peek(&action.id).unwrap().execution, Some(binding));
    }

    #[test]
    fn foreign_claimed_action_id_collision_does_not_request_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("runtime-a", root.path().to_path_buf());
        let registry = ConfirmationRegistry::default();
        let action = workflow_action("update");
        let binding = update_binding(
            "runtime-a",
            root.path().to_string_lossy().into_owned(),
            "task-foreign",
        );
        registry
            .register_with_execution(action.clone(), Some(binding.clone()))
            .unwrap();
        registry.claim(&action.id).unwrap();

        let pending = WorkflowPendingAction {
            id: action.id.clone(),
            action_type: action.action_type.clone(),
            risk_level: action.risk_level.clone(),
            affected_paths: action.affected_paths.clone(),
            candidate: Some(WorkflowCandidateReference::TaskOwned {
                candidate_id: "candidate-update".into(),
            }),
            expires_at: action.expires_at.clone(),
            checkpoint_hash: action.checkpoint_hash.clone(),
        };
        let run = WorkflowRun {
            schema_version: 1,
            task_id: "task-malicious".into(),
            project_id: context.project_id.clone(),
            canonical_identity_key: "identity".into(),
            identity_revision: "revision".into(),
            kind: WorkflowKind::UpdateWiki,
            operation: crate::models::workflow::WorkflowOperation::BuiltIn,
            display_status: WorkflowDisplayStatus::WaitingForConfirmation,
            scope: WorkflowScope::UpdateWiki {
                mode: UpdateWikiMode::ChangedSources,
                source_versions: Vec::new(),
            },
            route: None,
            fingerprint: "fingerprint".into(),
            baseline_fingerprint: "baseline".into(),
            persistence: WorkflowPersistenceMode::MemoryOnly,
            persistence_transition: None,
            stages: Vec::new(),
            current_stage_id: None,
            queue_position: None,
            continuation_required: false,
            retry: None,
            pending_action: Some(pending.clone()),
            decision_review: None,
            result: None,
            error: None,
            started_at: "2026-08-09T00:00:00Z".into(),
            updated_at: "2026-08-09T00:00:00Z".into(),
            completed_at: None,
            cancellable: true,
            undo_cancel_until: None,
        };

        assert!(!registry
            .cancel_workflow_binding(&context, &run, &pending)
            .unwrap());
        registry.finish_claim(&action.id, false).unwrap();
        assert_eq!(registry.peek(&action.id).unwrap().execution, Some(binding));
    }
}
