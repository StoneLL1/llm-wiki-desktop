use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitRepositoryStatus {
    pub is_repository: bool,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub has_changes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointPurpose {
    InitialProject,
    HighRiskOperation,
    FinalResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCheckpoint {
    pub created: bool,
    pub commit_hash: Option<String>,
    pub message: String,
    pub purpose: CheckpointPurpose,
    pub affected_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitDiff {
    pub markdown: String,
    pub affected_paths: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{CheckpointPurpose, GitCheckpoint};
    use serde_json::json;

    #[test]
    fn serializes_git_checkpoint_with_camel_case_fields() {
        let checkpoint = GitCheckpoint {
            created: true,
            commit_hash: Some("abc123".to_string()),
            message: "Before overwrite".to_string(),
            purpose: CheckpointPurpose::HighRiskOperation,
            affected_paths: vec!["wiki/page.md".to_string()],
        };

        let value = serde_json::to_value(checkpoint).unwrap();

        assert_eq!(value["commitHash"], json!("abc123"));
        assert_eq!(value["purpose"], json!("high_risk_operation"));
        assert_eq!(value["affectedPaths"][0], json!("wiki/page.md"));
    }
}
