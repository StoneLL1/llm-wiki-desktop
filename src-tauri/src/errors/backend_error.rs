use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendErrorKind {
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackendError {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub recoverable: bool,
    pub user_action_required: bool,
}

impl BackendError {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        recoverable: bool,
        user_action_required: bool,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            recoverable,
            user_action_required,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::BackendError;
    use serde_json::json;

    #[test]
    fn serializes_backend_error_with_camel_case_fields() {
        let error = BackendError::new("PATH_TRAVERSAL", "Cannot escape root.", false, true)
            .with_details(json!({ "path": "../secret.md" }));

        let value = serde_json::to_value(error).unwrap();

        assert_eq!(value["code"], json!("PATH_TRAVERSAL"));
        assert_eq!(value["message"], json!("Cannot escape root."));
        assert_eq!(value["details"]["path"], json!("../secret.md"));
        assert_eq!(value["recoverable"], json!(false));
        assert_eq!(value["userActionRequired"], json!(true));
        assert!(value.get("user_action_required").is_none());
    }
}
