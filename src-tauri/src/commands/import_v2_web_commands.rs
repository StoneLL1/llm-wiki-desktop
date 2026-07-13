use crate::{
    app_state::AppState,
    errors::BackendError,
    models::{
        import_v2::{ImportInput, ImportInputKind, ImportSession},
        import_v2_web::AddImportUrlV2Request,
    },
    services::import_v2::{
        connector_session::ConnectorSessionRef,
        url_policy::{PrivateTargetGrant, UrlPolicy},
    },
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub platform: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeRequest {
    pub session_id: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteLoginRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub import_session_id: String,
    pub item_id: String,
    pub connector_session_id: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizePrivateTargetRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub url: String,
}
#[tauri::command]
pub fn add_import_url_v2(
    state: State<'_, AppState>,
    request: AddImportUrlV2Request,
) -> Result<ImportSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let target = UrlPolicy.normalize_for_session(&request.url)?;
    let reference = state.import_v2_service.store_web_target(&target)?;
    let result = state.import_v2_service.add_inputs(
        &context,
        &state.file_store,
        &request.session_id,
        vec![ImportInput {
            kind: ImportInputKind::Url,
            display_name: target.public.host.clone(),
            locator: reference.clone(),
            normalized_locator: Some(target.public.public_url),
            source_identity: None,
        }],
    );
    if result.is_err() {
        let _ = state.import_v2_service.delete_web_target(&reference);
    }
    result
}
#[tauri::command]
pub fn begin_import_login_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: LoginRequest,
) -> Result<ConnectorSessionRef, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session =
        state
            .import_v2_service
            .load_session(&context, &state.file_store, &request.session_id)?;
    let item = session
        .items
        .iter()
        .find(|item| item.item_id == request.item_id)
        .ok_or_else(|| {
            BackendError::new(
                "IMPORT_V2_ITEM_NOT_FOUND",
                "Import item was not found.",
                false,
                true,
            )
        })?;
    if item.input.kind != ImportInputKind::Url {
        return Err(BackendError::new(
            "IMPORT_V2_BROWSER_SESSION_FAILED",
            "Login is available only for URL imports.",
            false,
            true,
        ));
    }
    let target = state.import_v2_service.resolve_web_target(
        &item.input.locator,
        item.input.normalized_locator.as_deref(),
    )?;
    if !platform_matches_host(&request.platform, &target.public.host) {
        return Err(BackendError::new(
            "IMPORT_V2_BROWSER_SESSION_FAILED",
            "The connector platform does not match the import target.",
            false,
            true,
        ));
    }
    let pack = state
        .import_capability_runtime
        .browser_pack()
        .ok_or_else(|| {
            BackendError::new(
                "IMPORT_V2_CAPABILITY_UNAVAILABLE",
                "The signed browser capability is unavailable.",
                true,
                true,
            )
        })?;
    let root = app
        .path()
        .app_data_dir()
        .map_err(|_| {
            BackendError::new(
                "IMPORT_V2_BROWSER_SESSION_FAILED",
                "App data path is unavailable.",
                true,
                true,
            )
        })?
        .join("connector-profiles");
    state.connector_session_service.begin_login(
        &request.platform,
        &root,
        &pack,
        target.request_url.as_str(),
    )
}
#[tauri::command]
pub fn revoke_import_login_v2(
    state: State<'_, AppState>,
    request: RevokeRequest,
) -> Result<(), BackendError> {
    state.connector_session_service.revoke(&request.session_id)
}
#[tauri::command]
pub fn complete_import_login_v2(
    state: State<'_, AppState>,
    request: CompleteLoginRequest,
) -> Result<ConnectorSessionRef, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session = state.import_v2_service.load_session(&context, &state.file_store, &request.import_session_id)?;
    let item = session.items.iter().find(|item| item.item_id == request.item_id).ok_or_else(|| {
        BackendError::new("IMPORT_V2_ITEM_NOT_FOUND", "Import item was not found.", false, true)
    })?;
    let target = state.import_v2_service.resolve_web_target(&item.input.locator, item.input.normalized_locator.as_deref())?;
    let reference = state.connector_session_service.resume(&request.connector_session_id)?;
    if !platform_matches_host(&reference.platform, &target.public.host) {
        return Err(BackendError::new("IMPORT_V2_BROWSER_SESSION_FAILED", "The authenticated connector does not match this item.", false, true));
    }
    let profile = state.connector_session_service.authenticated_profile(&request.connector_session_id)?;
    state.import_v2_service.bind_authenticated_profile(&request.item_id, profile)?;
    Ok(reference)
}
#[tauri::command]
pub async fn authorize_import_private_target_v2(
    state: State<'_, AppState>,
    request: AuthorizePrivateTargetRequest,
) -> Result<String, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session = state.import_v2_service.load_session(&context, &state.file_store, &request.session_id)?;
    let item = session.items.iter().find(|item| item.item_id == request.item_id).ok_or_else(|| {
        BackendError::new("IMPORT_V2_ITEM_NOT_FOUND", "Import item was not found.", false, true)
    })?;
    if item.input.kind != ImportInputKind::Url {
        return Err(BackendError::new("IMPORT_V2_URL_REJECTED", "Private authorization is available only for URL imports.", false, true));
    }
    let target = state.import_v2_service.resolve_web_target(&item.input.locator, item.input.normalized_locator.as_deref())?;
    let confirmed = UrlPolicy.normalize_for_session(&request.url)?;
    if confirmed.public != target.public {
        return Err(BackendError::new("IMPORT_V2_URL_REFERENCE_MISMATCH", "The confirmed private target does not match this import item.", false, true));
    }
    let port = target.request_url.port_or_known_default().ok_or_else(|| {
        BackendError::new(
            "IMPORT_V2_URL_REJECTED",
            "URL port is invalid.",
            false,
            true,
        )
    })?;
    let ips = tokio::net::lookup_host((target.public.host.as_str(), port))
        .await
        .map_err(|_| {
            BackendError::new("IMPORT_V2_DNS_FAILED", "DNS resolution failed.", true, true)
        })?
        .map(|a| a.ip())
        .collect();
    let grant = PrivateTargetGrant {
        item_id: request.item_id,
        scheme: target.public.scheme,
        host: target.public.host,
        port,
        resolved_ips: ips,
        expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
    };
    state.import_v2_service.authorize_private_target(grant)
}

fn platform_matches_host(platform: &str, host: &str) -> bool {
    match platform {
        "wechat" => host == "mp.weixin.qq.com",
        "zhihu" => host == "zhihu.com" || host.ends_with(".zhihu.com"),
        "bilibili" => host == "b23.tv" || host == "bilibili.com" || host.ends_with(".bilibili.com"),
        "xiaohongshu" => host == "xiaohongshu.com" || host.ends_with(".xiaohongshu.com"),
        "x" => host == "x.com" || host.ends_with(".x.com") || host == "twitter.com" || host.ends_with(".twitter.com"),
        _ => false,
    }
}
