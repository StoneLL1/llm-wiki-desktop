use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::import_v2::ImportSession;
use crate::models::import_v2_file::FileScanPolicy;
use crate::services::import_v2::file_discovery::{new_import_inputs, FileDiscoveryService};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddImportPathsV2Request {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub source_paths: Vec<String>,
}

#[tauri::command]
pub fn add_import_paths_v2(
    state: State<'_, AppState>,
    request: AddImportPathsV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session = state.import_v2_service.load_session(&context, &state.file_store, &request.session_id)?;
    let roots = request.source_paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    let scan = FileDiscoveryService::default().scan(&context, &roots, FileScanPolicy::default(), |_| {}, || false)?;
    let inputs = new_import_inputs(&session, scan.files);
    if inputs.is_empty() { return Ok(session); }
    state.import_v2_service.add_inputs(&context, &state.file_store, &request.session_id, inputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_has_sources_but_no_target_paths_or_policy_override() {
        let value = serde_json::to_value(AddImportPathsV2Request { project_id: "p".into(), project_root_path: "root".into(), session_id: "s".into(), source_paths: vec!["a.md".into()] }).unwrap();
        assert_eq!(value["sourcePaths"][0], "a.md");
        assert!(value.get("targetPath").is_none());
        assert!(value.get("policy").is_none());
    }
}
