use serde::{Deserialize, Serialize};

use crate::errors::{BackendError, IMPORT_V2_ENGINE_OUTPUT_INVALID};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: String,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

impl<T> JsonRpcResponse<T> {
    pub fn validate(&self, request_id: &str) -> Result<(), BackendError> {
        let valid_payload = self.result.is_some() ^ self.error.is_some();
        if self.jsonrpc != "2.0" || self.id != request_id || !valid_payload {
            return Err(BackendError::new(
                IMPORT_V2_ENGINE_OUTPUT_INVALID,
                "The import engine returned an invalid JSON-RPC response.",
                false,
                false,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::IMPORT_V2_ENGINE_OUTPUT_INVALID;

    fn valid_response() -> JsonRpcResponse<String> {
        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: "request-1".into(),
            result: Some("ok".into()),
            error: None,
        }
    }

    #[test]
    fn response_requires_exactly_one_result_or_error() {
        let mut neither = valid_response();
        neither.result = None;
        assert_eq!(
            neither.validate("request-1").unwrap_err().code,
            IMPORT_V2_ENGINE_OUTPUT_INVALID
        );

        let mut both = valid_response();
        both.error = Some(JsonRpcError {
            code: -32000,
            message: "failed".into(),
            data: None,
        });
        assert_eq!(
            both.validate("request-1").unwrap_err().code,
            IMPORT_V2_ENGINE_OUTPUT_INVALID
        );
    }

    #[test]
    fn response_id_must_match_request_id() {
        let error = valid_response().validate("request-2").unwrap_err();
        assert_eq!(error.code, IMPORT_V2_ENGINE_OUTPUT_INVALID);
    }

    #[test]
    fn response_requires_json_rpc_version_two() {
        let mut response = valid_response();
        response.jsonrpc = "1.0".into();

        let error = response.validate("request-1").unwrap_err();

        assert_eq!(error.code, IMPORT_V2_ENGINE_OUTPUT_INVALID);
    }
}
