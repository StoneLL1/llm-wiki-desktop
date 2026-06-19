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
            commands::file_commands::read_markdown_file,
            commands::file_commands::write_markdown_file,
            commands::file_commands::write_json_file,
            commands::file_commands::get_file_hash,
            commands::file_commands::confirm_pending_action,
            commands::git_commands::git_status,
            commands::git_commands::initialize_git_repository,
            commands::git_commands::create_git_checkpoint,
            commands::git_commands::git_diff_markdown,
            commands::import_commands::preview_import,
            commands::import_commands::confirm_import_preview,
            commands::import_commands::extract_text_preview,
            commands::import_commands::validate_import_url,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LLM Wiki Desktop");
}
