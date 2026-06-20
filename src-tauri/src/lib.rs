pub mod app_state;
pub mod commands;
pub mod errors;
pub mod models;
pub mod services;
pub mod tasks;
pub mod utils;

use tasks::task_events::EventBus;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(app_state::AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<app_state::AppState>();

            state
                .task_service
                .set_event_bus(EventBus::new_tauri(handle.clone()));

            // Build tray menu: Show / Hide / Quit. The notification plugin is registered
            // above so the frontend can surface task completion/failure/confirmation
            // notifications via OS notification APIs; the backend surfaces state changes
            // through the typed event bus (BackendEventType).
            let menu = tauri::menu::MenuBuilder::new(app)
                .item(&tauri::menu::MenuItemBuilder::with_id("show", "Show").build(app)?)
                .item(&tauri::menu::MenuItemBuilder::with_id("hide", "Hide").build(app)?)
                .separator()
                .item(&tauri::menu::MenuItemBuilder::with_id("quit", "Quit").build(app)?)
                .build()?;

            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| "default window icon missing; cannot build tray icon".to_string())?;
            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .tooltip("LLM Wiki Desktop")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app_handle, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "hide" => {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                    "quit" => {
                        app_handle.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray_handle, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app_handle = tray_handle.app_handle();
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Intercept window close to minimize to tray (default behavior).
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_clone.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agent_commands::detect_agents,
            commands::agent_commands::get_agent_config,
            commands::agent_commands::set_default_agent,
            commands::llm_commands::list_llm_providers,
            commands::llm_commands::save_llm_provider,
            commands::llm_commands::store_provider_secret,
            commands::llm_commands::delete_provider_secret,
            commands::llm_commands::provider_secret_status,
            commands::llm_commands::test_llm_provider,
            commands::compile_commands::start_wiki_compile,
            commands::compile_commands::confirm_compile_action,
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
            commands::task_commands::create_task,
            commands::task_commands::list_tasks,
            commands::task_commands::get_task,
            commands::task_commands::cancel_task,
            commands::task_commands::get_task_logs,
            commands::task_commands::remove_completed_tasks,
            commands::task_commands::set_active_project,
            commands::wiki_commands::scan_wiki,
            commands::wiki_commands::read_wiki_page,
            commands::wiki_commands::save_wiki_page,
            commands::search_commands::search_wiki,
            commands::graph_commands::get_graph,
            commands::graph_commands::build_graph,
            commands::graph_commands::save_graph_layout,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LLM Wiki Desktop");
}
