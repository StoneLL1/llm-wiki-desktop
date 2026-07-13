use llm_wiki_desktop_lib::models::{
    import_v2::{ImportIssue, ImportStage},
    import_v2_agent::{AgentAssistancePolicy, AgentRecoveryAction},
};

#[test]
fn balanced_policy_never_auto_invokes_cloud_or_low_quality_success() {
    let policy = AgentAssistancePolicy::balanced(true);
    assert!(policy.auto_local_on_hard_failure);
    assert!(!policy.auto_local_on_quality_warning);
    assert!(!policy.auto_byok);
    assert_eq!(policy.max_attempts_per_item, 1);
}

#[test]
fn legacy_import_issue_defaults_available_actions_without_changing_existing_fields() {
    let issue: ImportIssue = serde_json::from_value(serde_json::json!({
        "code": "IMPORT_V2_ENGINE_FAILED",
        "message": "failed",
        "stage": "extract",
        "retryable": true,
        "userActionRequired": false,
        "recoveryActions": ["retry", "invoke_agent"]
    }))
    .unwrap();

    assert_eq!(issue.stage, ImportStage::Extract);
    assert!(issue.available_actions.is_empty());
    assert_eq!(issue.recovery_actions.len(), 2);

    let value = serde_json::to_value(issue).unwrap();
    assert_eq!(value["availableActions"], serde_json::json!([]));
    assert_eq!(
        value["recoveryActions"],
        serde_json::json!(["retry", "invoke_agent"])
    );
}

#[test]
fn agent_recovery_action_wire_names_are_stable() {
    assert_eq!(
        serde_json::to_value(AgentRecoveryAction::InvokeLocalAgent).unwrap(),
        "invoke_local_agent"
    );
    assert_eq!(
        serde_json::to_value(AgentRecoveryAction::RequestByok).unwrap(),
        "request_byok"
    );
}
