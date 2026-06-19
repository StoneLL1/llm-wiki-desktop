pub mod app_state;
pub mod commands;
pub mod errors;
pub mod models;
pub mod services;
pub mod tasks;
pub mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(app_state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::project_commands::get_app_summary,
            commands::project_commands::create_project,
            commands::project_commands::open_project,
            commands::project_commands::scan_project,
            commands::project_commands::list_recent_projects,
            commands::project_commands::remember_recent_project,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LLM Wiki Desktop");
}
