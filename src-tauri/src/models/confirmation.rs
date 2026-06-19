use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::{ActionPreview, PendingAction, PendingActionType, RiskLevel};
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
        };

        let value = serde_json::to_value(action).unwrap();

        assert_eq!(value["actionType"], json!("merge_conflict"));
        assert_eq!(value["riskLevel"], json!("high"));
        assert_eq!(value["affectedPaths"][0], json!("wiki/concepts/agent.md"));
        assert_eq!(value["expiresAt"], json!("2026-06-19T00:00:00Z"));
        assert_eq!(value["preview"]["summary"], json!("One conflicting file"));
        assert!(value.get("action_type").is_none());
    }
}
