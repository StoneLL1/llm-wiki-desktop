use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BackendErrorKind {
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
    pub user_action_required: bool,
}
