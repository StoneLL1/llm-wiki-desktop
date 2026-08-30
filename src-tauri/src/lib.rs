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
use tauri::{Emitter, Manager};

#[cfg(feature = "gui")]
const APP_FOREGROUND_CHANGED_EVENT: &str = "app://foreground-changed";

#[cfg(feature = "gui")]
#[derive(Clone, serde::Serialize)]
struct AppForegroundChangedPayload {
    foreground: bool,
}

#[cfg(all(windows, any(feature = "gui", test)))]
fn foreground_process_matches(foreground_process_id: u32, current_process_id: u32) -> Option<bool> {
    if foreground_process_id == 0 || current_process_id == 0 {
        return None;
    }
    Some(foreground_process_id == current_process_id)
}

#[cfg(all(feature = "gui", windows))]
fn normalized_app_foreground(_focused: bool) -> Option<bool> {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    // SAFETY: both APIs are read-only process/window queries. The HWND is
    // checked before use and the PID out-parameter is valid for the call.
    unsafe {
        let foreground_window = GetForegroundWindow();
        if foreground_window.is_null() {
            return None;
        }
        let mut foreground_process_id = 0;
        if GetWindowThreadProcessId(foreground_window, &mut foreground_process_id) == 0 {
            return None;
        }
        foreground_process_matches(foreground_process_id, GetCurrentProcessId())
    }
}

#[cfg(all(feature = "gui", not(windows)))]
fn normalized_app_foreground(focused: bool) -> Option<bool> {
    Some(focused)
}

#[cfg(feature = "gui")]
fn startup_backend_error(error: errors::BackendError) -> String {
    serde_json::to_string(&error).unwrap_or_else(|_| format!("{}: {}", error.code, error.message))
}

#[cfg(feature = "gui")]
fn reject_workflow_dispatch(
    state: &app_state::AppState,
    task_id: &str,
    code: &str,
    message: String,
) {
    let failure = match code {
        "WORKFLOW_PROJECT_IDENTITY_CHANGED"
        | "WORKFLOW_IDENTITY_FAILED"
        | "PROJECT_IDENTITY_FAILED"
        | "PROJECT_CONTEXT_MISMATCH" => {
            services::WorkflowDispatchFailure::identity(code, "workflows.error.prepareAgain")
        }
        "WORKFLOW_PROJECT_CONTEXT_MISSING" | "WORKFLOW_DISPATCH_INVARIANT" => {
            services::WorkflowDispatchFailure::invariant(code, "workflows.error.prepareAgain")
        }
        _ => services::WorkflowDispatchFailure::stale(code, "workflows.error.prepareAgain"),
    };
    match state
        .workflow_service
        .reject_claimed_dispatch_with_settings(
            &state.task_service,
            &state.settings_service,
            task_id,
            failure,
        ) {
        Ok(_) => {
            if let Err(log_error) =
                state
                    .task_service
                    .append_log(task_id, tasks::task_model::LogLevel::Error, message)
            {
                observe_workflow_adapter_error(task_id, "guard-log", &log_error);
            }
        }
        Err(error) => observe_workflow_adapter_error(task_id, "guard-finalizer", &error.message),
    }
}

#[cfg(feature = "gui")]
fn dispatch_claimed_next(state: &app_state::AppState, next: &models::workflow::WorkflowRun) {
    if let Err(error) = state.workflow_service.dispatch_claimed_run_with_settings(
        &state.task_service,
        &state.settings_service,
        next,
    ) {
        observe_workflow_adapter_error(&next.task_id, "next-dispatch", &error.message);
        if let Err(log_error) = state.task_service.append_log(
            &next.task_id,
            tasks::task_model::LogLevel::Error,
            format!("Next workflow dispatch failed: {}", error.message),
        ) {
            observe_workflow_adapter_error(&next.task_id, "next-dispatch-log", &log_error);
        }
    }
}

#[cfg(feature = "gui")]
fn observe_workflow_adapter_error(task_id: &str, phase: &str, message: &str) {
    eprintln!("workflow_adapter_error task_id={task_id} phase={phase} message={message}");
}

#[cfg(feature = "gui")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(app_state::AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = app.state::<app_state::AppState>();

            match state
                .settings_service
                .reconcile_update_install_receipt(&app.package_info().version.to_string())
            {
                Ok(Some(error)) | Err(error) => state.update_service.restore_startup_error(error),
                Ok(None) => {}
            }

            state
                .task_service
                .set_event_bus(EventBus::new_tauri(handle.clone()));
            let runner_handle = handle.clone();
            state
                .workflow_service
                .register_runner(std::sync::Arc::new(services::UpdateWikiRunner::new(
                    move |run| {
                        let app = runner_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<app_state::AppState>();
                            let Some(root) = state.task_service.project_root_for_task(&run.task_id) else {
                                reject_workflow_dispatch(&state, &run.task_id,
                                    "WORKFLOW_PROJECT_CONTEXT_MISSING",
                                    "Update Wiki task has no project root.".into(),
                                );
                                return;
                            };
                            let asserted_root = root.to_string_lossy().into_owned();
                            let context = match state
                                .resolve_project_context(&run.project_id, &asserted_root)
                            {
                                Ok(context) => context,
                                Err(error) => {
                                    reject_workflow_dispatch(&state, &run.task_id, &error.code, error.message);
                                    return;
                                }
                            };
                            let identity = match services::project_identity(&context.root) {
                                Ok(identity)
                                    if identity.canonical_identity_key
                                        == run.canonical_identity_key
                                        && identity.identity_revision == run.identity_revision =>
                                {
                                    identity
                                }
                                Ok(_) | Err(_) => {
                                    reject_workflow_dispatch(&state, &run.task_id,
                                        "WORKFLOW_PROJECT_IDENTITY_CHANGED",
                                        "Update Wiki project identity changed while queued.".into(),
                                    );
                                    return;
                                }
                            };
                            let _ = identity;
                            let access = match state.resolve_workflow_access(&context) {
                                Ok(access) => access,
                                Err(error) => {
                                    reject_workflow_dispatch(&state, &run.task_id, &error.code, error.message);
                                    return;
                                }
                            };
                            if access.trust
                                != models::workflow::WorkflowProjectTrust::Trusted
                            {
                                reject_workflow_dispatch(&state, &run.task_id,
                                    "WORKFLOW_PROJECT_UNTRUSTED",
                                    "Update Wiki cannot execute until the backend project-open trust policy supplies a current trusted access snapshot.".into(),
                                );
                                return;
                            }
                            if access.filesystem_access
                                != models::workflow::WorkflowFilesystemAccess::Writable
                            {
                                reject_workflow_dispatch(&state, &run.task_id,
                                    "WORKFLOW_PROJECT_READ_ONLY",
                                    "Update Wiki project access is no longer writable.".into(),
                                );
                                return;
                            }
                            let compile = services::CompileExecutionServices {
                                agent_service: &state.agent_service,
                                llm_service: &state.llm_service,
                                secret_service: &state.secret_service,
                                settings_service: &state.settings_service,
                                task_service: &state.task_service,
                            };
                            let update = services::UpdateWikiExecutionServices {
                                compile,
                                git_service: &state.git_service,
                                file_store: &state.file_store,
                                bookmark_service: &state.bookmark_service,
                                search_service: &state.search_service,
                                confirmation_registry: &state.confirmation_registry,
                                coordinator: &state.workflow_service.coordinator,
                            };
                            let authority_run = run.clone();
                            if let Some(next) = services::run_update_wiki_authorized(
                                &context,
                                run,
                                &update,
                                || state.publish_workflow_external_launch(&context, &authority_run),
                            )
                            .await
                            {
                                dispatch_claimed_next(&state, &next);
                            }
                        });
                    },
                )))
                .map_err(startup_backend_error)?;
            let health_runner_handle = handle.clone();
            state
                .workflow_service
                .register_runner(std::sync::Arc::new(services::HealthCheckRunner::new(
                    move |run| {
                        let app = health_runner_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<app_state::AppState>();
                            let Some(root) = state.task_service.project_root_for_task(&run.task_id)
                            else {
                                reject_workflow_dispatch(&state, &run.task_id,
                                    "WORKFLOW_PROJECT_CONTEXT_MISSING",
                                    "Health Check task has no project root.".into(),
                                );
                                return;
                            };
                            let asserted_root = root.to_string_lossy().into_owned();
                            let context = match state
                                .resolve_project_context(&run.project_id, &asserted_root)
                            {
                                Ok(context) => context,
                                Err(error) => {
                                    reject_workflow_dispatch(&state, &run.task_id, &error.code, error.message);
                                    return;
                                }
                            };
                            let identity = match services::project_identity(&context.root) {
                                Ok(identity)
                                    if identity.canonical_identity_key
                                        == run.canonical_identity_key
                                        && identity.identity_revision == run.identity_revision =>
                                {
                                    identity
                                }
                                Ok(_) | Err(_) => {
                                    reject_workflow_dispatch(&state, &run.task_id,
                                        "WORKFLOW_PROJECT_IDENTITY_CHANGED",
                                        "Health Check project identity changed while queued."
                                            .into(),
                                    );
                                    return;
                                }
                            };
                            let _ = identity;
                            let access = match state.resolve_workflow_access(&context) {
                                Ok(access) => access,
                                Err(error) => {
                                    reject_workflow_dispatch(&state, &run.task_id, &error.code, error.message);
                                    return;
                                }
                            };
                            let complete = matches!(
                                &run.scope,
                                models::workflow::WorkflowScope::HealthCheck {
                                    mode: models::workflow::HealthCheckMode::Complete
                                }
                            );
                            if complete
                                && access.trust
                                    != models::workflow::WorkflowProjectTrust::Trusted
                            {
                                reject_workflow_dispatch(&state, &run.task_id,
                                    "WORKFLOW_PROJECT_UNTRUSTED",
                                    "Complete Health Check requires a current trusted project access snapshot."
                                        .into(),
                                );
                                return;
                            }
                            let health = services::HealthCheckExecutionServices {
                                lint_service: &state.lint_service,
                                search_service: &state.search_service,
                                settings_service: &state.settings_service,
                                secret_service: &state.secret_service,
                                agent_service: &state.agent_service,
                                llm_service: &state.llm_service,
                                task_service: &state.task_service,
                                coordinator: &state.workflow_service.coordinator,
                            };
                            let authority_run = run.clone();
                            let report_authority_run = run.clone();
                            if let Some(next) = services::run_health_check_authorized(
                                &context,
                                run,
                                &health,
                                || state.publish_workflow_external_launch(&context, &authority_run),
                                || {
                                    state.publish_workflow_persistent_report(
                                        &context,
                                        &report_authority_run,
                                    )
                                },
                            )
                            .await
                            {
                                dispatch_claimed_next(&state, &next);
                            }
                        });
                    },
                )))
                .map_err(startup_backend_error)?;
            let lint_repair_runner_handle = handle.clone();
            state
                .workflow_service
                .register_runner(std::sync::Arc::new(services::AgentLintRepairRunner::new(
                    move |run| {
                        let app = lint_repair_runner_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<app_state::AppState>();
                            let Some(root) = state.task_service.project_root_for_task(&run.task_id)
                            else {
                                reject_workflow_dispatch(
                                    &state,
                                    &run.task_id,
                                    "WORKFLOW_PROJECT_CONTEXT_MISSING",
                                    "Agent lint repair task has no project root.".into(),
                                );
                                return;
                            };
                            let asserted_root = root.to_string_lossy().into_owned();
                            let context = match state
                                .resolve_project_context(&run.project_id, &asserted_root)
                            {
                                Ok(context) => context,
                                Err(error) => {
                                    reject_workflow_dispatch(
                                        &state,
                                        &run.task_id,
                                        &error.code,
                                        error.message,
                                    );
                                    return;
                                }
                            };
                            match services::project_identity(&context.root) {
                                Ok(identity)
                                    if identity.canonical_identity_key
                                        == run.canonical_identity_key
                                        && identity.identity_revision == run.identity_revision => {}
                                Ok(_) | Err(_) => {
                                    reject_workflow_dispatch(
                                        &state,
                                        &run.task_id,
                                        "WORKFLOW_PROJECT_IDENTITY_CHANGED",
                                        "Agent lint repair project identity changed while queued."
                                            .into(),
                                    );
                                    return;
                                }
                            }
                            let settings = match state.settings_service.read_settings(&context) {
                                Ok(settings) => settings,
                                Err(error) => {
                                    reject_workflow_dispatch(
                                        &state,
                                        &run.task_id,
                                        &error.code,
                                        error.message,
                                    );
                                    return;
                                }
                            };
                            let selected_agent = match &run.route {
                                Some(models::workflow::WorkflowRoute::Agent { agent, .. }) => *agent,
                                _ => {
                                    reject_workflow_dispatch(
                                        &state,
                                        &run.task_id,
                                        "LINT_AGENT_ROUTE_REQUIRED",
                                        "Agent lint repair has no exact Agent route.".into(),
                                    );
                                    return;
                                }
                            };
                            let repair = services::AgentLintRepairExecutionServices {
                                agent_service: &state.agent_service,
                                lint_service: &state.lint_service,
                                git_service: &state.git_service,
                                file_store: &state.file_store,
                                bookmark_service: &state.bookmark_service,
                                search_service: &state.search_service,
                                confirmation_registry: &state.confirmation_registry,
                                settings_service: &state.settings_service,
                                task_service: &state.task_service,
                                coordinator: &state.workflow_service.coordinator,
                            };
                            let authority_run = run.clone();
                            if let Some(next) = services::run_agent_lint_repair_authorized(
                                &context,
                                run,
                                &repair,
                                &settings.language,
                                settings.agent_default == Some(selected_agent),
                                || {
                                    state.publish_workflow_external_launch(
                                        &context,
                                        &authority_run,
                                    )
                                },
                            ) {
                                dispatch_claimed_next(&state, &next);
                            }
                        });
                    },
                )))
                .map_err(startup_backend_error)?;
            let generate_runner_handle = handle.clone();
            state
                .workflow_service
                .register_runner(std::sync::Arc::new(services::GenerateContentRunner::new(
                    move |run| {
                        let app = generate_runner_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let state = app.state::<app_state::AppState>();
                            let Some(root) = state.task_service.project_root_for_task(&run.task_id)
                            else {
                                reject_workflow_dispatch(&state, &run.task_id,
                                    "WORKFLOW_PROJECT_CONTEXT_MISSING",
                                    "Generate Content task has no project root.".into(),
                                );
                                return;
                            };
                            let asserted_root = root.to_string_lossy().into_owned();
                            let context = match state
                                .resolve_project_context(&run.project_id, &asserted_root)
                            {
                                Ok(context) => context,
                                Err(error) => {
                                    reject_workflow_dispatch(&state, &run.task_id, &error.code, error.message);
                                    return;
                                }
                            };
                            match services::project_identity(&context.root) {
                                Ok(identity)
                                    if identity.canonical_identity_key
                                        == run.canonical_identity_key
                                        && identity.identity_revision == run.identity_revision => {}
                                Ok(_) | Err(_) => {
                                    reject_workflow_dispatch(&state, &run.task_id,
                                        "WORKFLOW_PROJECT_IDENTITY_CHANGED",
                                        "Generate Content project identity changed while queued."
                                            .into(),
                                    );
                                    return;
                                }
                            }
                            let access = match state.resolve_workflow_access(&context) {
                                Ok(access) => access,
                                Err(error) => {
                                    reject_workflow_dispatch(&state, &run.task_id, &error.code, error.message);
                                    return;
                                }
                            };
                            if access.trust
                                != models::workflow::WorkflowProjectTrust::Trusted
                            {
                                reject_workflow_dispatch(&state, &run.task_id,
                                    "WORKFLOW_PROJECT_UNTRUSTED",
                                    "Generate Content requires a current trusted project access snapshot."
                                        .into(),
                                );
                                return;
                            }
                            if access.filesystem_access
                                != models::workflow::WorkflowFilesystemAccess::Writable
                            {
                                reject_workflow_dispatch(&state, &run.task_id,
                                    "WORKFLOW_PROJECT_READ_ONLY",
                                    "Generate Content project access is no longer writable.".into(),
                                );
                                return;
                            }
                            let generate = services::GenerateContentExecutionServices {
                                export_service: &state.export_service,
                                search_service: &state.search_service,
                                settings_service: &state.settings_service,
                                secret_service: &state.secret_service,
                                agent_service: &state.agent_service,
                                llm_service: &state.llm_service,
                                git_service: &state.git_service,
                                confirmation_registry: &state.confirmation_registry,
                                task_service: &state.task_service,
                                coordinator: &state.workflow_service.coordinator,
                            };
                            let authority_run = run.clone();
                            if let Some(next) = services::run_generate_content_authorized(
                                &context,
                                run,
                                &generate,
                                || state.publish_workflow_external_launch(&context, &authority_run),
                            )
                            .await
                            {
                                dispatch_claimed_next(&state, &next);
                            }
                        });
                    },
                )))
                .map_err(startup_backend_error)?;
            if let Ok(app_data) = app.path().app_local_data_dir() {
                let install_root = app_data.join("installed-capabilities");
                state
                    .app_capability_coordinator
                    .initialize(&app_data.join("capability-control"), &state.task_service)
                    .map_err(startup_backend_error)?;
                #[cfg(debug_assertions)]
                {
                    let development_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../.dev-capabilities");
                    state
                        .import_capability_runtime
                        .load_startup(
                            &install_root,
                            Some((
                                &development_root.join("installed"),
                                &development_root.join("development-public-key.hex"),
                            )),
                            &state.import_v2_service,
                        )
                        .map_err(startup_backend_error)?;
                }
                #[cfg(not(debug_assertions))]
                state
                    .import_capability_runtime
                    .load_startup(&install_root, None, &state.import_v2_service)
                    .map_err(startup_backend_error)?;
            }
            if let Ok(app_data) = app.path().app_data_dir() {
                let connector_profiles = app_data.join("connector-profiles");
                state
                    .import_v2_service
                    .set_connector_profiles_root(connector_profiles.clone());
                let _ = state
                    .connector_session_service
                    .recover_orphans(&connector_profiles);
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
                    if let tauri::WindowEvent::Focused(focused) = event {
                        if let Some(foreground) = normalized_app_foreground(*focused) {
                            if let Err(error) = window_clone.emit(
                                APP_FOREGROUND_CHANGED_EVENT,
                                AppForegroundChangedPayload { foreground },
                            ) {
                                eprintln!(
                                    "foreground_event_emit_error event={} message={error}",
                                    APP_FOREGROUND_CHANGED_EVENT
                                );
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agent_commands::detect_agents,
            commands::app_capability_commands::list_app_capabilities_v1,
            commands::app_capability_commands::list_app_tasks_v1,
            commands::app_capability_commands::get_app_capability_task_logs_v1,
            commands::app_capability_commands::get_app_capability_task_activities_v1,
            commands::app_capability_commands::install_app_capability_v1,
            commands::app_capability_commands::pause_app_capability_install_v1,
            commands::app_capability_commands::resume_app_capability_install_v1,
            commands::app_capability_commands::cancel_app_capability_install_v1,
            commands::update_commands::get_update_state,
            commands::update_commands::get_global_update_preferences,
            commands::update_commands::save_global_update_preferences,
            commands::update_commands::check_app_update,
            commands::update_commands::download_app_update,
            commands::update_commands::cancel_app_update_download,
            commands::update_commands::install_app_update,
            commands::update_commands::restart_app_after_update,
            commands::update_commands::dismiss_app_update,
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
            commands::compile_commands::list_compile_source_versions,
            commands::compile_commands::confirm_compile_action,
            commands::compile_commands::get_compile_conflict_details,
            commands::compile_commands::resolve_compile_conflict,
            commands::project_commands::get_app_summary,
            commands::project_commands::start_project_open_assessment,
            commands::project_commands::get_project_open_assessment,
            commands::project_commands::cancel_project_open_assessment,
              commands::project_commands::open_assessed_project,
              commands::project_commands::relocate_recent_project,
              commands::project_commands::start_project_inventory,
            commands::project_commands::resolve_ambiguous_assessed_project,
            commands::project_commands::remember_ambiguous_project_intent,
            commands::project_commands::clear_ambiguous_project_intent,
            commands::project_commands::get_project_session_authority,
            commands::project_commands::trust_project,
            commands::project_commands::revoke_project_trust,
            commands::project_commands::enable_compatible_full_features,
            commands::project_commands::configure_compatible_layout,
            commands::project_commands::prepare_assessed_project_repair,
            commands::project_commands::prepare_default_project_parent,
            commands::project_commands::create_project,
            commands::project_commands::open_project,
            commands::project_commands::scan_project,
            commands::project_commands::list_recent_projects,
            commands::project_commands::remember_recent_project,
            commands::project_commands::remove_recent_project,
            commands::file_commands::read_markdown_file,
            commands::file_commands::write_markdown_file,
            commands::file_commands::write_json_file,
            commands::file_commands::get_file_hash,
            commands::file_commands::confirm_pending_action,
            commands::git_commands::git_status,
            commands::git_commands::initialize_git_repository,
            commands::git_commands::request_assessed_git_checkpoint,
            commands::git_commands::create_git_checkpoint,
            commands::git_commands::git_diff_markdown,
            commands::import_v2_async_commands::create_import_session_v2,
            commands::import_v2_async_commands::get_import_session_v2,
            commands::import_v2_async_commands::get_import_session_overview_v2,
            commands::import_v2_async_commands::list_import_session_items_v2,
            commands::import_v2_async_commands::start_import_session_recovery_v2,
            commands::import_v2_async_commands::get_import_restricted_content_status_v2,
            commands::import_v2_async_commands::get_import_history_session_v2,
            commands::import_v2_async_commands::get_import_completion_v2,
            commands::import_v2_async_commands::add_import_items_v2,
            commands::import_v2_async_commands::add_import_text_v2,
            commands::import_v2_async_commands::set_import_item_selection_v2,
            commands::import_v2_async_commands::select_import_subtitle_v2,
            commands::import_v2_async_commands::get_import_merge_context_v2,
            commands::import_v2_async_commands::set_import_item_resolution_v2,
            commands::import_v2_async_commands::stage_import_manual_merge_v2,
            commands::import_v2_async_commands::cancel_import_item_v2,
            commands::import_v2_async_commands::cancel_import_batch_v2,
            commands::import_v2_async_commands::skip_import_item_v2,
            commands::import_v2_async_commands::start_import_items_v2,
            commands::import_v2_async_commands::start_import_batch_v2,
            commands::import_v2_async_commands::confirm_import_session_v2,
            commands::import_v2_async_commands::start_import_agent_assistance_v2,
            commands::import_v2_async_commands::accept_import_agent_candidate_v2,
            commands::import_v2_async_commands::select_import_agent_candidate_v2,
            commands::import_v2_async_commands::discard_import_agent_candidate_v2,
            commands::import_v2_async_commands::start_add_import_paths_v2,
            commands::import_v2_async_commands::get_import_capability_statuses,
            commands::import_v2_async_commands::get_import_scan_result_v2,
            commands::import_v2_async_commands::accept_import_scan_v2,
            commands::import_v2_async_commands::discard_import_scan_v2,
            commands::import_v2_async_commands::add_import_url_v2,
            commands::import_v2_async_commands::discover_import_collection_v2,
            commands::import_v2_async_commands::load_import_collection_page_v2,
            commands::import_v2_async_commands::add_import_collection_items_v2,
            commands::import_v2_async_commands::get_remote_media_retention_plan_v2,
            commands::import_v2_async_commands::confirm_remote_media_retention_v2,
            commands::import_v2_async_commands::begin_import_login_v2,
            commands::import_v2_async_commands::complete_import_login_v2,
            commands::import_v2_async_commands::revoke_import_login_v2,
            commands::import_v2_async_commands::authorize_import_private_target_v2,
            commands::import_v2_async_commands::authorize_local_asr_v2,
            commands::import_v2_async_commands::authorize_local_ocr_v2,
            commands::import_v2_async_commands::authorize_bilibili_asr_v2,
            commands::import_v2_async_commands::scan_import_v2_migration,
            commands::import_v2_async_commands::plan_import_v2_migration,
            commands::import_v2_async_commands::apply_import_v2_migration,
            commands::import_v2_async_commands::get_import_v2_migration_status,
            commands::import_v2_async_commands::resume_import_v2_migration,
            commands::import_v2_async_commands::activate_import_v2,
            commands::import_v2_async_commands::get_import_backend_activation,
            commands::import_v2_async_commands::get_import_preview_content_v2,
            commands::import_v2_async_commands::get_import_frontend_readiness_v2,
            commands::import_v2_async_commands::get_import_workbench_preferences_v2,
            commands::import_v2_async_commands::save_import_workbench_preferences_v2,
            commands::import_v2_async_commands::list_import_history_v2,
            commands::import_v2_async_commands::get_import_history_detail_v2,
            commands::import_v2_async_commands::rebuild_import_history_index_v2,
            commands::import_v2_async_commands::get_import_capability_requirement_v2,
            commands::import_v2_async_commands::get_import_asr_enablement_plan_v2,
            commands::import_v2_async_commands::install_import_capability_v2,
            commands::task_commands::create_task,
            commands::task_commands::list_tasks,
            commands::task_commands::get_task,
            commands::task_commands::cancel_task,
            commands::task_commands::get_task_logs,
            commands::task_commands::get_task_activities,
            commands::task_commands::remove_completed_tasks,
            commands::task_commands::set_active_project,
            commands::task_commands::continue_queued_workflows,
            commands::workflow_commands::get_workflows_overview,
            commands::workflow_commands::prepare_workflow,
            commands::workflow_commands::start_workflow,
            commands::workflow_commands::list_workflow_runs,
            commands::workflow_commands::get_workflow_run,
            commands::workflow_commands::get_workflow_file_diff,
            commands::workflow_commands::cancel_workflow_run,
            commands::workflow_commands::undo_cancel_queued_workflow,
            commands::workflow_commands::reorder_queued_workflow,
            commands::workflow_commands::retry_workflow,
            commands::workflow_commands::confirm_workflow_action,
            commands::workflow_commands::discard_workflow_result,
            commands::wiki_commands::scan_wiki,
            commands::wiki_commands::read_wiki_page,
            commands::wiki_commands::read_wiki_asset,
            commands::wiki_commands::save_wiki_page,
            commands::wiki_commands::create_wiki_page,
            commands::wiki_commands::rename_wiki_page,
            commands::wiki_commands::request_delete_wiki_page,
            commands::source_commands::get_source_detail,
            commands::source_commands::list_source_versions,
            commands::source_commands::preview_source_update,
            commands::source_commands::apply_source_candidate,
            commands::source_commands::discard_source_candidate,
            commands::source_commands::restore_source_version,
            commands::source_commands::start_source_ai_organize,
            commands::source_commands::retry_source_ai_organize,
            commands::source_commands::reprocess_source_ocr,
            commands::source_commands::reprocess_source_asr,
            commands::source_commands::reprocess_source_subtitle,
            commands::source_commands::refresh_source,
            commands::source_commands::preview_move_source,
            commands::source_commands::move_source,
            commands::source_commands::preview_delete_source,
            commands::source_commands::delete_source,
            commands::wiki_commands::toggle_bookmark,
            commands::search_commands::search_wiki,
            commands::settings_commands::get_settings,
            commands::settings_commands::save_settings,
            commands::settings_commands::get_global_ui_preferences,
            commands::settings_commands::save_global_ui_preferences,
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
            commands::lint_commands::prepare_agent_lint_repair,
            commands::lint_commands::confirm_agent_lint_repair_start,
            commands::lint_commands::cancel_agent_lint_repair_preparation,
            commands::lint_commands::rollback_agent_lint_repair,
            commands::lint_commands::get_deep_lint_report,
            commands::lint_commands::list_lint_history,
            commands::lint_commands::read_lint_history_report,
            commands::lint_commands::apply_lint_fix,
            commands::lint_commands::apply_lint_fixes,
            commands::lint_commands::add_lint_ignore,
            commands::lint_commands::remove_lint_ignore,
            commands::lint_commands::list_lint_ignores,
            commands::export_commands::start_export,
            commands::export_commands::get_export_restricted_content_status,
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

#[cfg(all(test, windows))]
mod foreground_tests {
    use super::foreground_process_matches;

    #[test]
    fn foreground_pid_matching_fails_closed_for_unknown_processes() {
        assert_eq!(foreground_process_matches(42, 42), Some(true));
        assert_eq!(foreground_process_matches(42, 7), Some(false));
        assert_eq!(foreground_process_matches(0, 7), None);
        assert_eq!(foreground_process_matches(42, 0), None);
    }
}
