pub mod app_state;
#[cfg(feature = "gui")]
pub mod commands;
pub mod errors;
pub mod models;
pub mod services;
pub mod tasks;
pub mod utils;

#[cfg(feature = "gui")]
use tasks::task_events::EventBus;
#[cfg(feature = "gui")]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
#[cfg(feature = "gui")]
use tauri::Manager;

#[cfg(feature = "gui")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(app_state::AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<app_state::AppState>();

            state
                .task_service
                .set_event_bus(EventBus::new_tauri(handle.clone()));
            if let Ok(app_data) = app.path().app_local_data_dir() {
                state.import_capability_runtime.load_installed(
                    &app_data.join("installed-capabilities"),
                    &state.import_v2_service,
                );
            }
            if let Ok(app_data) = app.path().app_data_dir() {
                let _ = state
                    .connector_session_service
                    .recover_orphans(&app_data.join("connector-profiles"));
            }

            // Build tray menu: Show / Hide / Quit. Labels + tooltip are
            // localized to the user's UI language preference (CLAUDE.md: i18n
            // is a hard boundary, including OS-facing surfaces). The menu is
            // built once at startup; changing the language takes effect on the
            // next app launch (Tauri does not rebuild an existing tray menu
            // in-place). The notification plugin is registered above so the
            // frontend can surface task completion/failure/confirmation
            // notifications via OS notification APIs; the backend surfaces
            // state changes through the typed event bus (BackendEventType).
            let language = crate::services::SettingsService::default().read_language();
            let (show_label, hide_label, quit_label, tooltip) =
                crate::utils::i18n::tray_labels(&language);
            let menu = tauri::menu::MenuBuilder::new(app)
                .item(&tauri::menu::MenuItemBuilder::with_id("show", show_label).build(app)?)
                .item(&tauri::menu::MenuItemBuilder::with_id("hide", hide_label).build(app)?)
                .separator()
                .item(&tauri::menu::MenuItemBuilder::with_id("quit", quit_label).build(app)?)
                .build()?;

            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| "default window icon missing; cannot build tray icon".to_string())?;
            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .tooltip(tooltip)
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
                        let close_behavior =
                            crate::services::SettingsService::default().read_close_behavior();
                        if matches!(
                            close_behavior,
                            crate::models::settings::CloseBehavior::MinimizeToTray
                        ) {
                            api.prevent_close();
                            let _ = window_clone.hide();
                        }
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
            commands::llm_commands::check_ollama_reachable,
            commands::llm_commands::test_llm_provider,
            commands::compile_commands::start_wiki_compile,
            commands::compile_commands::confirm_compile_action,
            commands::compile_commands::get_compile_conflict_details,
            commands::compile_commands::resolve_compile_conflict,
            commands::project_commands::get_app_summary,
            commands::project_commands::create_project,
            commands::project_commands::open_project,
            commands::project_commands::preview_open_folder_as_project,
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
            commands::import_commands::get_import_preview,
            commands::import_commands::preview_text_import,
            commands::import_commands::fetch_import_url,
            commands::import_commands::list_imported_sources,
            commands::import_commands::request_delete_source,
            commands::import_commands::request_replace_source,
            commands::import_commands::confirm_import_preview,
            commands::import_commands::extract_text_preview,
            commands::import_commands::validate_import_url,
            commands::import_v2_commands::create_import_session_v2,
            commands::import_v2_commands::get_import_session_v2,
            commands::import_v2_commands::get_import_history_session_v2,
            commands::import_v2_commands::add_import_items_v2,
            commands::import_v2_commands::add_import_text_v2,
            commands::import_v2_commands::set_import_item_selection_v2,
            commands::import_v2_commands::cancel_import_item_v2,
            commands::import_v2_commands::cancel_import_batch_v2,
            commands::import_v2_commands::skip_import_item_v2,
            commands::import_v2_commands::start_import_items_v2,
            commands::import_v2_commands::confirm_import_session_v2,
            commands::import_v2_agent_commands::get_import_agent_policy_v2,
            commands::import_v2_agent_commands::set_import_agent_policy_v2,
            commands::import_v2_agent_commands::start_import_agent_assistance_v2,
            commands::import_v2_agent_commands::preview_import_byok_scope_v2,
            commands::import_v2_agent_commands::approve_import_byok_assistance_v2,
            commands::import_v2_agent_commands::accept_import_agent_candidate_v2,
            commands::import_v2_agent_commands::select_import_agent_candidate_v2,
            commands::import_v2_agent_commands::discard_import_agent_candidate_v2,
            commands::import_v2_file_commands::start_add_import_paths_v2,
            commands::import_v2_file_commands::get_import_capability_statuses,
            commands::import_v2_file_commands::get_import_scan_result_v2,
            commands::import_v2_web_commands::add_import_url_v2,
            commands::import_v2_web_commands::begin_import_login_v2,
            commands::import_v2_web_commands::complete_import_login_v2,
            commands::import_v2_web_commands::revoke_import_login_v2,
            commands::import_v2_web_commands::authorize_import_private_target_v2,
            commands::import_v2_web_commands::authorize_bilibili_asr_v2,
            commands::import_v2_migration::scan_import_v2_migration,
            commands::import_v2_migration::plan_import_v2_migration,
            commands::import_v2_migration::apply_import_v2_migration,
            commands::import_v2_migration::get_import_v2_migration_status,
            commands::import_v2_migration::resume_import_v2_migration,
            commands::import_v2_activation::activate_import_v2,
            commands::import_v2_activation::get_import_backend_activation,
            commands::import_v2_presentation_commands::get_import_preview_content_v2,
            commands::import_v2_presentation_commands::get_import_frontend_readiness_v2,
            commands::import_v2_presentation_commands::list_import_history_v2,
            commands::import_v2_presentation_commands::get_import_capability_requirement_v2,
            commands::import_v2_presentation_commands::install_import_capability_v2,
            commands::task_commands::create_task,
            commands::task_commands::list_tasks,
            commands::task_commands::get_task,
            commands::task_commands::cancel_task,
            commands::task_commands::get_task_logs,
            commands::task_commands::get_task_activities,
            commands::task_commands::remove_completed_tasks,
            commands::task_commands::set_active_project,
            commands::wiki_commands::scan_wiki,
            commands::wiki_commands::read_wiki_page,
            commands::wiki_commands::read_wiki_asset,
            commands::wiki_commands::save_wiki_page,
            commands::wiki_commands::create_wiki_page,
            commands::wiki_commands::rename_wiki_page,
            commands::wiki_commands::request_delete_wiki_page,
            commands::wiki_commands::toggle_bookmark,
            commands::search_commands::search_wiki,
            commands::settings_commands::get_settings,
            commands::settings_commands::save_settings,
            commands::settings_commands::get_provider_secret_status,
            commands::settings_commands::get_chat_convenience_authorization,
            commands::settings_commands::set_chat_convenience_authorization,
            commands::settings_commands::revoke_all_chat_convenience_authorizations,
            commands::graph_commands::get_graph,
            commands::graph_commands::build_graph,
            commands::graph_commands::save_graph_layout,
            commands::chat_commands::create_chat_session,
            commands::chat_commands::list_chat_sessions,
            commands::chat_commands::load_chat_session,
            commands::chat_commands::rename_chat_session,
            commands::chat_commands::delete_chat_session,
            commands::chat_commands::send_chat_message,
            commands::chat_commands::save_answer_to_wiki,
            commands::chat_commands::resolve_chat_convenience_edit,
            commands::chat_commands::rollback_last_chat_convenience_edit,
            commands::lint_commands::run_local_lint,
            commands::lint_commands::start_deep_lint,
            commands::lint_commands::get_deep_lint_report,
            commands::lint_commands::list_lint_history,
            commands::lint_commands::read_lint_history_report,
            commands::lint_commands::apply_lint_fix,
            commands::lint_commands::apply_lint_fixes,
            commands::lint_commands::add_lint_ignore,
            commands::lint_commands::remove_lint_ignore,
            commands::lint_commands::list_lint_ignores,
            commands::export_commands::start_export,
            commands::export_commands::regenerate_export,
            commands::export_commands::list_exports,
            commands::export_commands::toggle_export_bookmark,
            commands::export_commands::read_export_preview,
            commands::export_commands::open_export_in_browser,
            commands::export_commands::open_export_folder,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LLM Wiki Desktop");
}
