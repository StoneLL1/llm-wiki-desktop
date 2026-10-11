use tauri::AppHandle;

use crate::errors::BackendError;
use crate::services::BlockingWorkClass;

/// Opens one fixed OS preferences destination; accepts no user-supplied URL or command.
#[tauri::command]
pub async fn open_notification_preferences(app: AppHandle) -> Result<(), BackendError> {
    super::runtime::run_blocking(app, BlockingWorkClass::MetadataIo, move |_app| {
        open_notification_preferences_native()
    })
    .await
}

fn open_notification_preferences_native() -> Result<(), BackendError> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.Notifications-Settings.extension")
        .status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("explorer.exe")
        .arg("ms-settings:notifications")
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Err(BackendError::new(
        "NOTIFICATION_SETTINGS_MANUAL",
        "Open your desktop's system settings and choose Notifications to allow LLM Wiki Desktop notifications.",
        true,
        true,
    ));
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    match status {
        Ok(status) if status.success() => Ok(()),
        _ => Err(BackendError::new(
            "NOTIFICATION_SETTINGS_OPEN_FAILED",
            "Could not open system notification settings. Open them from your operating system settings instead.",
            true,
            true,
        )),
    }
}
