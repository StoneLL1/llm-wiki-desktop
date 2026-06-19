use tauri::State;

use crate::app_state::AppState;
use crate::models::project::AppSummary;

#[tauri::command]
pub fn get_app_summary(_state: State<'_, AppState>) -> AppSummary {
    AppSummary {
        name: "LLM Wiki Desktop".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}
