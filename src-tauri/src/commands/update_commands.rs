use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::task::TaskStatus;
use crate::models::update::{
    update_error, validate_update_install_guard, AppUpdateState, GlobalUpdatePreferences,
    InstallGuardFacts, SaveGlobalUpdatePreferences, StaticUpdateManifest, UpdateCheckCandidate,
    UpdateInstallRequest, UpdateOfferRequest, UpdateProgressEvent, MAX_UPDATE_MANIFEST_BYTES,
};
use crate::models::workflow::WorkflowDisplayStatus;
use crate::services::verify_signed_update_artifact;

const STABLE_UPDATE_ENDPOINT: &str =
    "https://github.com/StoneLL1/llm-wiki-desktop/releases/latest/download/latest.json";
const UPDATE_SIGNING_PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDBEMjc0RUU4OEFCOTA2NTYKUldSV0JybUs2RTRuRGN2UlZENGdNY1FxUE1aZ2F1Y2NBSnpiZ25qTmRtdURURzYrMitUdms4SUEK";
const CHECK_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const CHECK_TOTAL_TIMEOUT: Duration = Duration::from_secs(35);
const DOWNLOAD_REQUEST_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const DOWNLOAD_POLL_INTERVAL: Duration = Duration::from_millis(100);
const MAX_UPDATE_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[tauri::command]
pub fn get_update_state(state: State<'_, AppState>) -> AppUpdateState {
    state.update_service.state()
}

#[tauri::command]
pub fn get_global_update_preferences(
    state: State<'_, AppState>,
) -> Result<GlobalUpdatePreferences, BackendError> {
    state.settings_service.read_global_update_preferences()
}

#[tauri::command]
pub fn save_global_update_preferences(
    state: State<'_, AppState>,
    preferences: SaveGlobalUpdatePreferences,
) -> Result<GlobalUpdatePreferences, BackendError> {
    state
        .settings_service
        .save_global_update_preferences(preferences)
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AppUpdateState, BackendError> {
    let now = now_unix_seconds();
    let previous_offer_id = state
        .update_service
        .state()
        .offer
        .map(|offer| offer.offer_id);
    let generation = state.update_service.begin_check(now)?;
    if let Some(offer_id) = previous_offer_id {
        state.update_runtime.clear(&offer_id);
    }
    let checked_at = chrono::Utc::now().to_rfc3339();
    let result = tokio::time::timeout(CHECK_TOTAL_TIMEOUT, check_signed_offer(&app)).await;

    let check_result = match result {
        Ok(result) => result,
        Err(_) => Err(update_error(
            "UPDATE_CHECK_TIMEOUT",
            "The update check timed out. Try again when the network is available.",
            true,
        )),
    };

    let final_state = match check_result {
        Ok((candidate, desktop_update)) => {
            let current_version = app.package_info().version.to_string();
            let mut desktop_update = desktop_update;
            state.update_service.complete_check_with_registration(
                generation,
                &current_version,
                Some(candidate),
                now_unix_seconds(),
                |offer, identity| {
                    let update = desktop_update.take().ok_or_else(offer_changed)?;
                    state.update_runtime.register(
                        offer.offer_id.clone(),
                        identity.to_string(),
                        update,
                    )
                },
            )?;
            state.update_service.state()
        }
        Err(error) => {
            state
                .update_service
                .fail_check_with_error(generation, error.clone());
            state.settings_service.record_update_check(checked_at)?;
            return Err(error);
        }
    };

    state.settings_service.record_update_check(checked_at)?;
    Ok(final_state)
}

#[tauri::command]
pub async fn download_app_update(
    state: State<'_, AppState>,
    request: UpdateOfferRequest,
    on_progress: Channel<UpdateProgressEvent>,
) -> Result<AppUpdateState, BackendError> {
    let permit = state
        .update_service
        .begin_download(&request.offer_id, now_unix_seconds())?;
    let runtime = match state.update_runtime.begin_download(&request.offer_id) {
        Ok(runtime) if runtime.bound_identity == permit.candidate_identity() => runtime,
        Ok(_) => {
            let error = offer_changed();
            state.update_service.fail_download(&permit, error.clone())?;
            return Err(error);
        }
        Err(error) => {
            state.update_service.fail_download(&permit, error.clone())?;
            return Err(error);
        }
    };

    let downloaded = Arc::new(Mutex::new(0_u64));
    let callback_error = Arc::new(Mutex::new(None::<BackendError>));
    let callback_downloaded = Arc::clone(&downloaded);
    let callback_failure = Arc::clone(&callback_error);
    let cancellation = Arc::clone(&runtime.cancellation);
    let artifact_too_large = Arc::new(AtomicBool::new(false));
    let callback_too_large = Arc::clone(&artifact_too_large);
    let service = &state.update_service;
    let progress_permit = permit.clone();
    let progress_channel = on_progress.clone();
    let mut download = Box::pin(runtime.update.download(
        move |chunk_size, total_bytes| {
            let current = callback_downloaded
                .lock()
                .map(|mut downloaded| {
                    *downloaded = downloaded.saturating_add(chunk_size as u64);
                    *downloaded
                })
                .unwrap_or(u64::MAX);
            if current > MAX_UPDATE_ARTIFACT_BYTES {
                callback_too_large.store(true, Ordering::SeqCst);
                cancellation.store(true, Ordering::SeqCst);
            }
            match service.record_download_progress(&progress_permit, current, total_bytes) {
                Ok(state) => {
                    let _ = progress_channel.send(UpdateProgressEvent {
                        phase: state.phase,
                        downloaded_bytes: state.downloaded_bytes,
                        total_bytes: state.total_bytes,
                    });
                }
                Err(error) => {
                    cancellation.store(true, Ordering::SeqCst);
                    if let Ok(mut failure) = callback_failure.lock() {
                        *failure = Some(error);
                    }
                }
            }
        },
        || {},
    ));

    let bytes = loop {
        tokio::select! {
            result = &mut download => break result.map_err(map_download_error),
            _ = tokio::time::sleep(DOWNLOAD_POLL_INTERVAL) => {
                if artifact_too_large.load(Ordering::SeqCst) {
                    break Err(update_error(
                        "UPDATE_ARTIFACT_TOO_LARGE",
                        "The update artifact exceeds the allowed size.",
                        true,
                    ));
                }
                if let Some(error) = callback_error
                    .lock()
                    .ok()
                    .and_then(|error| error.clone())
                {
                    break Err(error);
                }
                if runtime.cancellation.load(Ordering::SeqCst)
                    || state.update_service.is_download_cancelled(&permit)
                {
                    break Err(update_error(
                        "UPDATE_DOWNLOAD_CANCELLED",
                        "The update download was cancelled.",
                        true,
                    ));
                }
            }
        }
    };

    let bytes = match bytes {
        Ok(bytes) if (bytes.len() as u64) <= MAX_UPDATE_ARTIFACT_BYTES => bytes,
        Ok(_) => {
            let error = update_error(
                "UPDATE_ARTIFACT_TOO_LARGE",
                "The update artifact exceeds the allowed size.",
                true,
            );
            state.update_service.fail_download(&permit, error.clone())?;
            return Err(error);
        }
        Err(error) => {
            if state.update_service.is_download_cancelled(&permit) {
                return Err(update_error(
                    "UPDATE_DOWNLOAD_CANCELLED",
                    "The update download was cancelled.",
                    true,
                ));
            }
            state.update_service.fail_download(&permit, error.clone())?;
            return Err(error);
        }
    };

    if let Some(error) = callback_error
        .lock()
        .ok()
        .and_then(|mut error| error.take())
    {
        state.update_service.fail_download(&permit, error.clone())?;
        return Err(error);
    }
    state
        .update_runtime
        .store_download(&request.offer_id, permit.candidate_identity(), bytes)?;
    let final_state = state.update_service.finish_download(&permit)?;
    let _ = on_progress.send(UpdateProgressEvent {
        phase: final_state.phase,
        downloaded_bytes: final_state.downloaded_bytes,
        total_bytes: final_state.total_bytes,
    });
    Ok(final_state)
}

#[tauri::command]
pub fn cancel_app_update_download(
    state: State<'_, AppState>,
    request: UpdateOfferRequest,
) -> Result<AppUpdateState, BackendError> {
    state.update_runtime.cancel(&request.offer_id)?;
    state.update_service.cancel_download(&request.offer_id)?;
    Ok(state.update_service.state())
}

#[tauri::command]
pub async fn install_app_update(
    state: State<'_, AppState>,
    request: UpdateInstallRequest,
) -> Result<AppUpdateState, BackendError> {
    let _install_lease = state
        .confirmation_registry
        .update_install_barrier()
        .reserve_install_or_restart(|| validate_install_guard(&state, &request))?;
    let now = now_unix_seconds();
    let identity = state
        .update_service
        .offer_identity(&request.offer_id, now)?;
    state
        .update_service
        .begin_install(&request.offer_id, &identity, now)?;
    let (update, bytes) = match state
        .update_runtime
        .take_for_install(&request.offer_id, &identity)
    {
        Ok(payload) => payload,
        Err(error) => {
            state
                .update_service
                .fail_install(&request.offer_id, error.clone())?;
            return Err(error);
        }
    };
    let offer = state
        .update_service
        .state()
        .offer
        .filter(|offer| offer.offer_id == request.offer_id)
        .ok_or_else(offer_changed)?;
    if let Err(error) = state.settings_service.record_update_install_handoff(&offer) {
        state
            .update_runtime
            .restore_download(&request.offer_id, &identity, bytes);
        state
            .update_service
            .fail_install(&request.offer_id, error.clone())?;
        return Err(error);
    }

    let install_task = tauri::async_runtime::spawn_blocking(move || {
        let result = verify_artifact_again(&update, &bytes)
            .and_then(|_| update.install(&bytes).map_err(map_install_error));
        (result, bytes)
    })
    .await
    .map_err(|_| {
        update_error(
            "UPDATE_INSTALL_FAILED",
            "The verified update could not be started. The current version is unchanged.",
            true,
        )
    });
    let (install_result, bytes) = match install_task {
        Ok(result) => result,
        Err(error) => {
            let error = persist_update_install_failure(&state, &request.offer_id, error);
            state
                .update_service
                .fail_install(&request.offer_id, error.clone())?;
            return Err(error);
        }
    };
    if let Err(error) = install_result {
        let error = persist_update_install_failure(&state, &request.offer_id, error);
        state
            .update_runtime
            .restore_download(&request.offer_id, &identity, bytes);
        state
            .update_service
            .fail_install(&request.offer_id, error.clone())?;
        return Err(error);
    }

    if let Err(error) = state
        .settings_service
        .finish_update_install_receipt(&request.offer_id, Ok(()))
    {
        state
            .update_service
            .fail_install(&request.offer_id, error.clone())?;
        return Err(error);
    }
    state.update_runtime.clear(&request.offer_id);
    state.update_service.finish_install(&request.offer_id)
}

fn persist_update_install_failure(
    state: &AppState,
    offer_id: &str,
    mut primary: BackendError,
) -> BackendError {
    let primary_code = primary.code.clone();
    if let Err(receipt_error) = state
        .settings_service
        .finish_update_install_receipt(offer_id, Err(&primary_code))
    {
        primary.details = Some(serde_json::json!({
            "primaryDetails": primary.details.take(),
            "receiptPersistenceError": receipt_error,
        }));
        primary.message = format!(
            "{} The durable update receipt could not record this failure.",
            primary.message
        );
        primary.recoverable = false;
        primary.user_action_required = true;
    }
    primary
}

#[tauri::command]
pub fn restart_app_after_update(
    app: AppHandle,
    state: State<'_, AppState>,
    request: UpdateInstallRequest,
) -> Result<(), BackendError> {
    let _restart_lease = state
        .confirmation_registry
        .update_install_barrier()
        .reserve_install_or_restart(|| validate_install_guard(&state, &request))?;
    let update_state = state.update_service.state();
    if update_state.phase != crate::models::update::UpdatePhase::Installed
        || update_state
            .offer
            .as_ref()
            .map(|offer| offer.offer_id.as_str())
            != Some(request.offer_id.as_str())
    {
        return Err(update_error(
            "UPDATE_RESTART_NOT_READY",
            "The update is not ready for restart.",
            true,
        ));
    }
    app.restart()
}

fn validate_install_guard(
    state: &AppState,
    request: &UpdateInstallRequest,
) -> Result<(), BackendError> {
    let tasks = state.task_service.list_tasks(None);
    let workflows = state.task_service.list_workflow_runs();
    let confirmation_active = state.confirmation_registry.has_pending_or_executing()?;
    validate_update_install_guard(
        request,
        InstallGuardFacts {
            pending_task_confirmation: confirmation_active
                || tasks
                    .iter()
                    .any(|task| task.status == TaskStatus::WaitingForConfirmation),
            critical_task_active: tasks.iter().any(|task| task.blocks_update_install()),
            workflow_apply_active: workflows.iter().any(|run| {
                run.display_status == WorkflowDisplayStatus::Running
                    && run.current_stage_id.as_deref().is_some_and(|stage| {
                        stage == "apply_changes" || stage.starts_with("apply_changes_")
                    })
            }),
        },
    )
}

#[tauri::command]
pub fn dismiss_app_update(
    state: State<'_, AppState>,
    request: UpdateOfferRequest,
) -> Result<AppUpdateState, BackendError> {
    let offer = state.update_service.dismiss(&request.offer_id)?;
    state.update_runtime.clear(&request.offer_id);
    state
        .settings_service
        .dismiss_update_offer(offer.offer_id, offer.version)?;
    Ok(state.update_service.state())
}

async fn check_signed_offer(
    app: &AppHandle,
) -> Result<(UpdateCheckCandidate, Option<Update>), BackendError> {
    let platform = tauri_plugin_updater::target().ok_or_else(|| {
        update_error(
            "UPDATE_PLATFORM_UNSUPPORTED",
            "This operating system or architecture is not supported for updates.",
            false,
        )
    })?;
    let (target, arch) = platform.split_once('-').ok_or_else(|| {
        update_error(
            "UPDATE_PLATFORM_UNSUPPORTED",
            "This operating system or architecture is not supported for updates.",
            false,
        )
    })?;
    let candidate = fetch_manifest_candidate(target, arch).await?;
    let updater = app
        .updater_builder()
        .timeout(CHECK_REQUEST_TIMEOUT)
        .max_manifest_bytes(MAX_UPDATE_MANIFEST_BYTES)
        .build()
        .map_err(map_check_error)?;
    let update = match updater.check().await.map_err(map_check_error)? {
        Some(mut update) => {
            let raw_manifest_bytes =
                serde_json::to_vec(&update.raw_json).map_err(|_| manifest_invalid())?;
            if raw_manifest_bytes.len() > MAX_UPDATE_MANIFEST_BYTES {
                return Err(manifest_too_large());
            }
            // The bounded preflight is the source of manifest metadata. Do not retain the
            // updater plugin's duplicate raw response for the lifetime of the pending offer.
            update.raw_json = serde_json::Value::Null;
            update.timeout = Some(DOWNLOAD_REQUEST_TIMEOUT);
            Some(update)
        }
        None => None,
    };
    if let Some(update) = update.as_ref() {
        if update.version != candidate.version
            || update.target != candidate.target
            || update.download_url.as_str() != candidate.download_url
            || update.signature != candidate.signature
        {
            return Err(offer_changed());
        }
    }
    Ok((candidate, update))
}

async fn fetch_manifest_candidate(
    target: &str,
    arch: &str,
) -> Result<UpdateCheckCandidate, BackendError> {
    let client = reqwest::Client::builder()
        .connect_timeout(CHECK_REQUEST_TIMEOUT)
        .timeout(CHECK_REQUEST_TIMEOUT)
        .user_agent("llm-wiki-desktop-updater")
        .build()
        .map_err(|_| check_unavailable())?;
    let response = client
        .get(STABLE_UPDATE_ENDPOINT)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                update_error(
                    "UPDATE_CHECK_TIMEOUT",
                    "The update check timed out. Try again when the network is available.",
                    true,
                )
            } else {
                check_unavailable()
            }
        })?;
    if !response.status().is_success() || response.url().scheme() != "https" {
        return Err(check_unavailable());
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_UPDATE_MANIFEST_BYTES as u64)
    {
        return Err(manifest_too_large());
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| check_unavailable())?;
        if bytes.len().saturating_add(chunk.len()) > MAX_UPDATE_MANIFEST_BYTES {
            return Err(manifest_too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    StaticUpdateManifest::parse_bounded(&bytes, MAX_UPDATE_MANIFEST_BYTES)?
        .candidate_for(target, arch)
}

fn verify_artifact_again(update: &Update, bytes: &[u8]) -> Result<(), BackendError> {
    verify_signed_update_artifact(UPDATE_SIGNING_PUBLIC_KEY, &update.signature, bytes)
}

fn map_check_error(error: tauri_plugin_updater::Error) -> BackendError {
    use tauri_plugin_updater::Error;
    match error {
        Error::UnsupportedArch | Error::UnsupportedOs | Error::TargetNotFound(_) => update_error(
            "UPDATE_PLATFORM_UNSUPPORTED",
            "This operating system or architecture is not supported for updates.",
            false,
        ),
        Error::ManifestTooLarge(_) => manifest_too_large(),
        Error::Semver(_) | Error::Serialization(_) | Error::UrlParse(_) => update_error(
            "UPDATE_MANIFEST_INVALID",
            "The update manifest is invalid.",
            true,
        ),
        Error::Reqwest(error) if error.is_timeout() => update_error(
            "UPDATE_CHECK_TIMEOUT",
            "The update check timed out. Try again when the network is available.",
            true,
        ),
        _ => check_unavailable(),
    }
}

fn map_download_error(error: tauri_plugin_updater::Error) -> BackendError {
    use tauri_plugin_updater::Error;
    match error {
        Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) => signature_invalid(),
        Error::Reqwest(error) if error.is_timeout() => update_error(
            "UPDATE_DOWNLOAD_TIMEOUT",
            "The update download timed out. Try again.",
            true,
        ),
        _ => update_error(
            "UPDATE_DOWNLOAD_FAILED",
            "The signed update could not be downloaded. Try again.",
            true,
        ),
    }
}

fn map_install_error(_error: tauri_plugin_updater::Error) -> BackendError {
    update_error(
        "UPDATE_INSTALL_FAILED",
        "The verified update could not be started. The current version is unchanged.",
        true,
    )
}

fn check_unavailable() -> BackendError {
    update_error(
        "UPDATE_CHECK_UNAVAILABLE",
        "The update service is unavailable. Try again when the network is available.",
        true,
    )
}

fn manifest_too_large() -> BackendError {
    update_error(
        "UPDATE_MANIFEST_TOO_LARGE",
        "The update manifest exceeds the allowed size.",
        true,
    )
}

fn manifest_invalid() -> BackendError {
    update_error(
        "UPDATE_MANIFEST_INVALID",
        "The update manifest is invalid.",
        true,
    )
}

fn signature_invalid() -> BackendError {
    update_error(
        "UPDATE_SIGNATURE_INVALID",
        "The update signature is invalid. The update was not installed.",
        true,
    )
}

fn offer_changed() -> BackendError {
    update_error(
        "UPDATE_OFFER_CHANGED",
        "The update offer changed during verification. Check again.",
        true,
    )
}

fn now_unix_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}
