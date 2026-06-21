use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    InitializeFolder,
    DeleteFile,
    OverwriteFile,
    BatchRewrite,
    ReplaceSource,
    DeleteSource,
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

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmationExecution {
    InitializeFolder {
        root_path: String,
        file_hashes: Vec<(String, String)>,
    },
    CompileMerge {
        project_id: String,
        root_path: String,
        task_id: String,
        route: crate::models::compile::CompileRoute,
        manifest: crate::models::compile::CompileManifest,
        current_hashes: Vec<(String, String)>,
        checkpoint_hash: Option<String>,
    },
    LintFix {
        project_id: String,
        root_path: String,
        issue: crate::models::lint::LintIssue,
    },
    ChatOverwrite {
        project_id: String,
        root_path: String,
        session_id: String,
        message_id: String,
        target_path: String,
        current_hash: String,
    },
    DeleteSource {
        project_id: String,
        root_path: String,
        target_path: String,
        target_hash: String,
        artifacts: Vec<String>,
    },
    DeleteWikiPage {
        project_id: String,
        root_path: String,
        target_path: String,
        target_hash: String,
    },
    ReplaceSource {
        project_id: String,
        root_path: String,
        target_path: String,
        target_hash: String,
        replacement_path: String,
        replacement_hash: String,
        old_artifacts: Vec<String>,
        new_artifacts: Vec<String>,
    },
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
        let mut actions = self.actions.lock().map_err(|_| {
            BackendError::new(
                "CONFIRMATION_REGISTRY_LOCKED",
                "Confirmation registry is unavailable.",
                true,
                false,
            )
        })?;
        actions.insert(action.id.clone(), StoredPendingAction { action, execution });
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
        let actions = self.actions.lock().map_err(|_| {
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
        reject_if_expired(&stored.action)?;
        Ok(stored)
    }
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
        ActionPreview, ConfirmationRegistry, ConfirmationStatus, PendingAction, PendingActionType,
        RiskLevel,
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
        assert!(value.get("checkpointHash").is_some(), "field must be present");
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
}
