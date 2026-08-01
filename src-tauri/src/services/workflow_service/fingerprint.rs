use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::models::workflow::{
    WorkflowExecutionOptions, WorkflowKind, WorkflowRoute, WorkflowScope, WORKFLOW_SCHEMA_VERSION,
};

pub fn canonical_json<T: Serialize>(value: &T) -> Result<String, String> {
    let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
    serde_json::to_string(&sort_value(value)).map_err(|error| error.to_string())
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut entries = object.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = Map::new();
            for (key, value) in entries {
                sorted.insert(key, sort_value(value));
            }
            Value::Object(sorted)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sort_value).collect()),
        other => other,
    }
}

pub fn workflow_fingerprint(
    canonical_identity_key: &str,
    identity_revision: &str,
    kind: &WorkflowKind,
    scope: &WorkflowScope,
    execution_options: &WorkflowExecutionOptions,
    route: &Option<WorkflowRoute>,
    baseline_fingerprint: &str,
) -> Result<String, String> {
    let parts = [
        WORKFLOW_SCHEMA_VERSION.to_string(),
        canonical_identity_key.to_string(),
        identity_revision.to_string(),
        canonical_json(kind)?,
        canonical_json(scope)?,
        canonical_json(execution_options)?,
        canonical_json(route)?,
        baseline_fingerprint.to_string(),
    ];
    Ok(hex_sha256(parts.join("\n").as_bytes()))
}

pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::canonical_json;
    use serde_json::json;

    #[test]
    fn canonical_json_sorts_nested_object_keys_without_reordering_arrays() {
        assert_eq!(
            canonical_json(&json!({"z": 1, "a": {"y": 2, "x": [3, 1]}})).unwrap(),
            r#"{"a":{"x":[3,1],"y":2},"z":1}"#
        );
    }
}
