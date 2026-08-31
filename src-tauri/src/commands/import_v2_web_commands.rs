use crate::{
    app_state::AppState,
    commands::import_v2_commands::{
        import_session_context, start_import_batch_for_state, StartImportBatchV2Request,
    },
    errors::BackendError,
    models::{
        import_v2::{
            ImportInput, ImportInputKind, ImportItem, ImportItemStatus,
            ImportMediaAuthorizationKind, ImportSession,
        },
        import_v2_web::{
            AddImportCollectionItemsV2Request, AddImportUrlV2Request,
            AuthorizeBilibiliAsrV2Request, AuthorizeLocalAsrV2Request, AuthorizeLocalOcrV2Request,
            ConfirmRemoteMediaRetentionV2Request, DiscoverImportCollectionV2Request,
            ImportCollectionItemPreview, ImportCollectionPage, ImportCollectionPreview,
            LoadImportCollectionPageV2Request, RemoteMediaRetentionPlan,
            RemoteMediaRetentionRequest,
        },
        task::{BackendTask, TaskResult, TaskResultReference, TaskStatus},
    },
    services::import_v2::{
        connector_session::ConnectorSessionRef,
        platform_network_policy::{
            trusted_platform_page_host_suffixes, upgrade_trusted_platform_page_to_https,
        },
        platform_provider::{extract_platform_collection, looks_like_collection_url, Platform},
        remote_media_retention::build_remote_media_retention_plan,
        session_store::CollectionImportInput,
        url_policy::{PrivateTargetGrant, UrlPolicy},
        web_fetch::{WebFetchContent, WebFetchPolicy, WebFetchService},
        web_target_store::{asr_target_sha256, BilibiliAsrGrant},
    },
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tauri::{AppHandle, Manager, State};

fn collection_task_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_WEB_COLLECTION_TASK_FAILED", message, true, false)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub platform: Option<String>,
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
pub struct CompleteLoginResult {
    pub connector_session: ConnectorSessionRef,
    pub resumed_item_ids: Vec<String>,
    pub tasks: Vec<BackendTask>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizePrivateTargetRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub item_id: String,
    pub url: String,
}
pub fn add_import_url_v2(
    state: State<'_, AppState>,
    request: AddImportUrlV2Request,
) -> Result<ImportSession, BackendError> {
    if crate::commands::import_v2_commands::is_temporary_preview_session(&request.session_id) {
        let context = crate::commands::import_v2_commands::import_session_context(
            &state,
            &request.project_id,
            &request.project_root_path,
            &request.session_id,
        )?;
        let target = UrlPolicy.normalize_for_session(&request.url)?;
        let target = upgrade_trusted_platform_page_to_https(target)?;
        let reference = state.import_v2_service.store_web_target(&target)?;
        let result = state.import_v2_service.add_temporary_preview_inputs(
            &context,
            &state.file_store,
            &request.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: target.public.host.clone(),
                locator: reference.clone(),
                normalized_locator: Some(target.public.public_url),
                source_identity: None,
                media_save_mode: request.media_save_mode.clone(),
            }],
        );
        if result.is_err() {
            let _ = state.import_v2_service.delete_web_target(&reference);
        }
        return result;
    }
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, _context| {
            let target = UrlPolicy.normalize_for_session(&request.url)?;
            let target = upgrade_trusted_platform_page_to_https(target)?;
            let reference = state.import_v2_service.store_web_target(&target)?;
            let result = state.import_v2_service.add_inputs_authorized(
                permit,
                &state.file_store,
                &request.session_id,
                vec![ImportInput {
                    kind: ImportInputKind::Url,
                    display_name: target.public.host.clone(),
                    locator: reference.clone(),
                    normalized_locator: Some(target.public.public_url),
                    source_identity: None,
                    media_save_mode: request.media_save_mode.clone(),
                }],
            );
            if result.is_err() {
                let _ = state.import_v2_service.delete_web_target(&reference);
            }
            result
        },
    )
}

pub async fn discover_import_collection_v2(
    app: AppHandle,
    request: DiscoverImportCollectionV2Request,
) -> Result<Option<ImportCollectionPreview>, BackendError> {
    let coordinator = app.state::<AppState>().blocking_work.clone();
    let preflight_app = app.clone();
    let preflight_request = request.clone();
    let Some((context, authority_context, execution, target, allowed_host_suffixes, task)) =
        coordinator
            .run(crate::services::BlockingWorkClass::HeavyIo, move || {
                let state = preflight_app.state::<AppState>();
                let authority_context = state.resolve_project_context(
                    &preflight_request.project_id,
                    &preflight_request.project_root_path,
                )?;
                let context = import_session_context(
                    &state,
                    &preflight_request.project_id,
                    &preflight_request.project_root_path,
                    &preflight_request.session_id,
                )?;
                state.import_v2_service.load_session(
                    &context,
                    &state.file_store,
                    &preflight_request.session_id,
                )?;
                let target = UrlPolicy.normalize_for_session(&preflight_request.url)?;
                if !looks_like_collection_url(&target.public.public_url) {
                    return Ok(None);
                }
                let task = if context.root != authority_context.root {
                    state
                        .task_service
                        .create_memory_import_collection_task(
                            preflight_request.project_id.clone(),
                            authority_context.root.clone(),
                            "Discover import collection".into(),
                            preflight_request.session_id.clone(),
                        )
                        .map_err(|message| collection_task_error(&message))?
                } else {
                    state.with_current_project_write_access(
                        &preflight_request.project_id,
                        &preflight_request.project_root_path,
                        |_permit, context| {
                            let task_state_root = context
                                .layout
                                .task_state_root
                                .as_deref()
                                .ok_or_else(|| {
                                    collection_task_error(
                                        "Collection task persistence is unavailable.",
                                    )
                                })
                                .and_then(|relative| context.resolve_project_path(relative))?;
                            state
                                .task_service
                                .create_project_import_collection_task(
                                    preflight_request.project_id.clone(),
                                    context.root.clone(),
                                    task_state_root,
                                    "Discover import collection".into(),
                                    preflight_request.session_id.clone(),
                                )
                                .map_err(|message| collection_task_error(&message))
                        },
                    )?
                };
                state
                    .task_service
                    .transition_status(&task.id, TaskStatus::Running)
                    .map_err(|message| collection_task_error(&message))?;
                let execution =
                    match state.begin_project_external_task(&authority_context, &task.id) {
                        Ok(execution) => execution,
                        Err(error) => {
                            let _ = state.task_service.finish_running_operation(
                                &task.id,
                                TaskResult {
                                    summary: "Collection discovery could not start".into(),
                                    affected_paths: Vec::new(),
                                    reference: None,
                                    pending_action: None,
                                },
                                TaskStatus::Failed,
                                Some(error.clone()),
                            );
                            return Err(error);
                        }
                    };
                let allowed_host_suffixes =
                    trusted_platform_page_host_suffixes(&target.public.public_url)
                        .iter()
                        .map(|suffix| (*suffix).into())
                        .collect();
                Ok(Some((
                    context,
                    authority_context,
                    execution,
                    target,
                    allowed_host_suffixes,
                    task,
                )))
            })
            .await?
    else {
        return Ok(None);
    };
    let task_id = task.id.clone();
    let fetch_tasks = app.state::<AppState>().task_service.clone();
    let progress_tasks = fetch_tasks.clone();
    let progress_task_id = task_id.clone();
    let cancel_tasks = fetch_tasks.clone();
    let cancel_task_id = task_id.clone();
    let artifact = match WebFetchService
        .fetch(
            target.clone(),
            &UrlPolicy,
            &WebFetchPolicy {
                max_response_bytes: 8 * 1024 * 1024,
                max_attempts_per_route: 1,
                total_timeout_ms: 30_000,
                content: WebFetchContent::Page,
                require_https: true,
                allowed_host_suffixes,
                ..WebFetchPolicy::default()
            },
            None,
            "collection-discovery",
            move |progress| {
                let _ = progress_tasks.update_progress(
                    &progress_task_id,
                    progress.downloaded_bytes,
                    progress.total_bytes,
                    Some("collection.downloading".into()),
                );
            },
            move || cancel_tasks.is_cancelled(&cancel_task_id),
        )
        .await
    {
        Ok(artifact) => artifact,
        Err(error) => {
            let _ = fetch_tasks.finish_running_operation(
                &task_id,
                TaskResult {
                    summary: "Collection discovery failed".into(),
                    affected_paths: Vec::new(),
                    reference: None,
                    pending_action: None,
                },
                TaskStatus::Failed,
                Some(error.clone()),
            );
            return Err(error);
        }
    };
    let finish_tasks = app.state::<AppState>().task_service.clone();
    let finish_task_id = task_id.clone();
    let result = coordinator
        .run(crate::services::BlockingWorkClass::HeavyIo, move || {
            let state = app.state::<AppState>();
            let platform = Platform::from_url(&artifact.final_public_url).ok_or_else(|| {
                BackendError::new(
                    "IMPORT_WEB_COLLECTION_UNSUPPORTED",
                    "This collection platform is not supported.",
                    false,
                    true,
                )
            })?;
            let html = String::from_utf8_lossy(&artifact.bytes);
            let Some(collection) =
                extract_platform_collection(platform, &html, &artifact.final_public_url)
            else {
                state
                    .task_service
                    .finish_running_operation(
                        &task.id,
                        TaskResult {
                            summary: "No collection entries were found".into(),
                            affected_paths: Vec::new(),
                            reference: None,
                            pending_action: None,
                        },
                        TaskStatus::Succeeded,
                        None,
                    )
                    .map_err(|message| collection_task_error(&message))?;
                return Ok(None);
            };
            let known = state.import_v2_service.completed_collection_fingerprints(
                &context,
                &state.file_store,
                &target.public.public_url,
                &collection.platform,
            );
            let mut pending_items = Vec::with_capacity(collection.items.len());
            let mut total_duration_seconds: Option<u64> = None;
            let mut estimated_login_count = 0;
            let mut estimated_asr_count = 0;
            for item in collection.items {
                let child = UrlPolicy.normalize_for_session(&item.url)?;
                if known
                    .get(&child.public.public_url)
                    .is_some_and(|fingerprint| fingerprint == &item.discovery_fingerprint)
                {
                    continue;
                }
                if let Some(duration) = item.duration_seconds {
                    total_duration_seconds = Some(
                        total_duration_seconds
                            .unwrap_or_default()
                            .saturating_add(duration),
                    );
                }
                estimated_login_count += usize::from(item.estimated_login_required);
                estimated_asr_count += usize::from(item.estimated_asr_required);
                pending_items.push((item.title, child, item.discovery_fingerprint));
            }
            state.require_current_execution_epoch(&authority_context, &execution)?;
            let (collection_ref, page) = if context.root != authority_context.root {
                state.import_v2_service.store_web_collection_durable(
                    &context,
                    &state.file_store,
                    &task.id,
                    &request.project_id,
                    &request.session_id,
                    target.public.public_url.clone(),
                    collection.platform.clone(),
                    collection.title.clone(),
                    pending_items,
                )?
            } else {
                state.with_current_project_write_access(
                    &request.project_id,
                    &request.project_root_path,
                    |_permit, _context| {
                        state.import_v2_service.store_web_collection_durable(
                            _context,
                            &state.file_store,
                            &task.id,
                            &request.project_id,
                            &request.session_id,
                            target.public.public_url.clone(),
                            collection.platform.clone(),
                            collection.title.clone(),
                            pending_items,
                        )
                    },
                )?
            };
            let items = page
                .items
                .iter()
                .map(|item| ImportCollectionItemPreview {
                    item_ref: item.item_ref.clone(),
                    title: item.title.clone(),
                    public_url: item.public_url.clone(),
                })
                .collect::<Vec<_>>();
            let preview = ImportCollectionPreview {
                task_id: task.id.clone(),
                collection_ref: collection_ref.clone(),
                source_url: target.public.public_url.clone(),
                platform: collection.platform.clone(),
                title: collection.title.clone(),
                total_duration_seconds,
                estimated_login_count,
                estimated_asr_count,
                discovered_total: page.discovered_total,
                loaded_count: page.loaded_count,
                has_more: page.has_more,
                next_cursor: page.next_cursor.clone(),
                items,
            };
            let finished = match state.task_service.finish_running_operation(
                &task.id,
                TaskResult {
                    summary: format!("Discovered {} collection entries", page.discovered_total),
                    affected_paths: Vec::new(),
                    reference: Some(TaskResultReference::ImportCollectionPreview {
                        session_id: request.session_id.clone(),
                        collection_ref: collection_ref.clone(),
                        preview: preview.clone(),
                    }),
                    pending_action: None,
                },
                TaskStatus::WaitingForConfirmation,
                None,
            ) {
                Ok(finished) => finished,
                Err(message) => {
                    let _ = state.import_v2_service.delete_web_collection_durable(
                        &context,
                        &state.file_store,
                        &collection_ref,
                    );
                    return Err(collection_task_error(&message));
                }
            };
            if finished.status == TaskStatus::Cancelled {
                let _ = state.import_v2_service.delete_web_collection_durable(
                    &context,
                    &state.file_store,
                    &collection_ref,
                );
                return Err(collection_task_error("Collection discovery was cancelled."));
            }
            Ok(Some(preview))
        })
        .await;
    if let Err(error) = &result {
        let _ = finish_tasks.finish_running_operation(
            &finish_task_id,
            TaskResult {
                summary: "Collection discovery failed".into(),
                affected_paths: Vec::new(),
                reference: None,
                pending_action: None,
            },
            TaskStatus::Failed,
            Some(error.clone()),
        );
    }
    result
}

pub fn load_import_collection_page_v2(
    state: State<'_, AppState>,
    request: LoadImportCollectionPageV2Request,
) -> Result<ImportCollectionPage, BackendError> {
    let authority_context =
        state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let context = import_session_context(
        &state,
        &request.project_id,
        &request.project_root_path,
        &request.session_id,
    )?;
    if context.root != authority_context.root {
        return load_import_collection_page_for_context(&state, &context, &request);
    }
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| load_import_collection_page_for_context(&state, context, &request),
    )
}

fn load_import_collection_page_for_context(
    state: &AppState,
    context: &crate::models::paths::ProjectContext,
    request: &LoadImportCollectionPageV2Request,
) -> Result<ImportCollectionPage, BackendError> {
    state
        .import_v2_service
        .load_session(context, &state.file_store, &request.session_id)?;
    let (page, task_id) = state.import_v2_service.load_web_collection_page_durable(
        context,
        &state.file_store,
        &request.collection_ref,
        &request.project_id,
        &request.session_id,
        &request.cursor,
        request.load_all,
    )?;
    let _ = state.task_service.update_progress(
        &task_id,
        page.loaded_count as u64,
        Some(page.discovered_total as u64),
        Some("collection.loading".into()),
    );
    let public_page = ImportCollectionPage {
        items: page
            .items
            .into_iter()
            .map(|item| ImportCollectionItemPreview {
                item_ref: item.item_ref,
                title: item.title,
                public_url: item.public_url,
            })
            .collect(),
        discovered_total: page.discovered_total,
        loaded_count: page.loaded_count,
        has_more: page.has_more,
        next_cursor: page.next_cursor,
    };
    let mut task_result = state
        .task_service
        .get_task(&task_id)
        .and_then(|task| task.result)
        .ok_or_else(|| collection_task_error("Collection task result is unavailable."))?;
    match task_result.reference.as_mut() {
        Some(TaskResultReference::ImportCollectionPreview {
            collection_ref,
            preview,
            ..
        }) if collection_ref == &request.collection_ref => {
            preview.items = public_page.items.clone();
            preview.discovered_total = public_page.discovered_total;
            preview.loaded_count = public_page.loaded_count;
            preview.has_more = public_page.has_more;
            preview.next_cursor.clone_from(&public_page.next_cursor);
        }
        _ => return Err(collection_task_error("Collection task binding is invalid.")),
    }
    state
        .task_service
        .set_result(&task_id, task_result)
        .map_err(|error| collection_task_error(&error))?;
    Ok(public_page)
}

pub fn add_import_collection_items_v2(
    state: State<'_, AppState>,
    request: AddImportCollectionItemsV2Request,
) -> Result<ImportSession, BackendError> {
    if request.item_refs.is_empty() || request.item_refs.len() > 5_000 {
        return Err(BackendError::new(
            "IMPORT_WEB_COLLECTION_SELECTION_INVALID",
            "Select between 1 and 5000 collection items.",
            false,
            true,
        ));
    }
    let unique = request.item_refs.iter().collect::<HashSet<_>>();
    if unique.len() != request.item_refs.len() {
        return Err(BackendError::new(
            "IMPORT_WEB_COLLECTION_SELECTION_INVALID",
            "Collection item selections must be unique.",
            false,
            true,
        ));
    }
    let authority_context =
        state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let session_context = import_session_context(
        &state,
        &request.project_id,
        &request.project_root_path,
        &request.session_id,
    )?;
    let add = |permit: Option<&crate::app_state::ProjectWritePermit<'_>>,
               context: &crate::models::paths::ProjectContext|
     -> Result<ImportSession, BackendError> {
        state
            .import_v2_service
            .load_session(context, &state.file_store, &request.session_id)?;
        let selection = state
            .import_v2_service
            .resolve_web_collection_selection_durable(
                context,
                &state.file_store,
                &request.collection_ref,
                &request.project_id,
                &request.session_id,
                &request.item_refs,
            )?;
        let collection_task_id = selection.task_id.clone();
        let mut stored_refs = Vec::with_capacity(selection.targets.len());
        let mut inputs = Vec::with_capacity(selection.targets.len());
        for selected in selection.targets {
            let target = selected.target;
            match state.import_v2_service.store_web_target(&target) {
                Ok(item_ref) => {
                    stored_refs.push(item_ref.clone());
                    inputs.push(CollectionImportInput {
                        input: ImportInput {
                            kind: ImportInputKind::Url,
                            display_name: target.public.host,
                            locator: item_ref,
                            normalized_locator: Some(target.public.public_url),
                            source_identity: None,
                            media_save_mode: request.media_save_mode.clone(),
                        },
                        discovery_fingerprint: selected.discovery_fingerprint,
                    });
                }
                Err(error) => {
                    for item_ref in stored_refs {
                        let _ = state.import_v2_service.delete_web_target(&item_ref);
                    }
                    return Err(error);
                }
            }
        }
        let result = match permit {
            Some(permit) => state.import_v2_service.add_collection_inputs_authorized(
                permit,
                &state.file_store,
                &request.session_id,
                inputs,
                selection.source_url,
                selection.platform,
                selection.title,
            ),
            None => state
                .import_v2_service
                .add_temporary_preview_collection_inputs(
                    context,
                    &state.file_store,
                    &request.session_id,
                    inputs,
                    selection.source_url,
                    selection.platform,
                    selection.title,
                ),
        };
        let session = match result {
            Ok(session) => session,
            Err(error) => {
                for item_ref in stored_refs {
                    let _ = state.import_v2_service.delete_web_target(&item_ref);
                }
                return Err(error);
            }
        };
        let used_refs = session
            .items
            .iter()
            .map(|item| item.input.locator.as_str())
            .collect::<HashSet<_>>();
        for item_ref in stored_refs {
            if !used_refs.contains(item_ref.as_str()) {
                let _ = state.import_v2_service.delete_web_target(&item_ref);
            }
        }
        if let Some(task_id) = collection_task_id.as_deref() {
            let task = state.task_service.get_task(task_id).ok_or_else(|| {
                collection_task_error("Collection discovery task is unavailable.")
            })?;
            if task.status == TaskStatus::WaitingForConfirmation {
                state
                    .task_service
                    .transition_status(task_id, TaskStatus::Running)
                    .map_err(|message| collection_task_error(&message))?;
            } else if task.status != TaskStatus::Running {
                return Err(collection_task_error(
                    "Collection discovery task is no longer selectable.",
                ));
            }
            let finished = state
                .task_service
                .finish_running_operation(
                    task_id,
                    TaskResult {
                        summary: format!("Added {} collection entries", request.item_refs.len()),
                        affected_paths: Vec::new(),
                        reference: None,
                        pending_action: None,
                    },
                    TaskStatus::Succeeded,
                    None,
                )
                .map_err(|message| collection_task_error(&message))?;
            if finished.status == TaskStatus::Cancelled {
                let _ = state.import_v2_service.delete_web_collection_durable(
                    context,
                    &state.file_store,
                    &request.collection_ref,
                );
                return Err(collection_task_error("Collection selection was cancelled."));
            }
        }
        state.import_v2_service.delete_web_collection_durable(
            context,
            &state.file_store,
            &request.collection_ref,
        )?;
        Ok(session)
    };
    if session_context.root != authority_context.root {
        add(None, &session_context)
    } else {
        state.with_current_project_write_access(
            &request.project_id,
            &request.project_root_path,
            |permit, context| add(Some(permit), context),
        )
    }
}

pub fn get_remote_media_retention_plan_v2(
    state: State<'_, AppState>,
    request: RemoteMediaRetentionRequest,
) -> Result<RemoteMediaRetentionPlan, BackendError> {
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
            "IMPORT_WEB_MEDIA_RETENTION_UNAVAILABLE",
            "Remote media retention is available only for URL imports.",
            false,
            true,
        ));
    }
    build_remote_media_retention_plan(&context, &request.session_id, item)
}

pub fn confirm_remote_media_retention_v2(
    state: State<'_, AppState>,
    request: ConfirmRemoteMediaRetentionV2Request,
) -> Result<ImportSession, BackendError> {
    if !request.acknowledge_size_and_disk {
        return Err(BackendError::new(
            "IMPORT_WEB_MEDIA_RETENTION_CONFIRMATION_REQUIRED",
            "Remote media retention requires explicit size and disk confirmation.",
            false,
            true,
        ));
    }
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, context| {
            let session = state.import_v2_service.load_session(
                context,
                &state.file_store,
                &request.session_id,
            )?;
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
            let plan = build_remote_media_retention_plan(context, &request.session_id, item)?;
            if plan.enough_disk != Some(true) {
                return Err(BackendError::new(
                    "IMPORT_WEB_MEDIA_RETENTION_DISK_INSUFFICIENT",
                    "Remote media cannot be retained because verified free disk space is insufficient.",
                    true,
                    true,
                ));
            }
            state.import_v2_service.enable_remote_media_retention_authorized(
                permit,
                &state.file_store,
                &request.session_id,
                &request.item_id,
            )?;
            state.import_v2_service.load_session(
                context,
                &state.file_store,
                &request.session_id,
            )
        },
    )
}

pub fn begin_import_login_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: LoginRequest,
) -> Result<ConnectorSessionRef, BackendError> {
    let (context, target, pack, root) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            let session = state.import_v2_service.load_session(
                context,
                &state.file_store,
                &request.session_id,
            )?;
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
            Ok((context.clone(), target, pack, root))
        },
    )?;
    let execution = state.begin_project_external_execution(
        &context,
        &format!("connector-login:{}", uuid::Uuid::new_v4()),
    )?;
    state.require_current_execution_epoch(&context, &execution)?;
    let session = state.connector_session_service.begin_login(
        &request.platform,
        &root,
        &pack,
        target.request_url.as_str(),
        &request.project_id,
        &request.session_id,
        &request.item_id,
        execution,
    )?;
    Ok(session)
}
pub fn revoke_import_login_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: RevokeRequest,
) -> Result<(), BackendError> {
    if let Some(platform) = request.platform.as_deref() {
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
        state
            .connector_session_service
            .revoke_platform(platform, &root)
    } else {
        state.connector_session_service.revoke(&request.session_id)
    }
}
pub fn complete_import_login_v2(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CompleteLoginRequest,
) -> Result<CompleteLoginResult, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, context| {
            let session = state.import_v2_service.load_session(
                context,
                &state.file_store,
                &request.import_session_id,
            )?;
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
            let target = state.import_v2_service.resolve_web_target(
                &item.input.locator,
                item.input.normalized_locator.as_deref(),
            )?;
            let reference = state
                .connector_session_service
                .resume(&request.connector_session_id)?;
            if !platform_matches_host(&reference.platform, &target.public.host) {
                return Err(BackendError::new(
                    "IMPORT_V2_BROWSER_SESSION_FAILED",
                    "The authenticated connector does not match this item.",
                    false,
                    true,
                ));
            }
            let mut resumed_item_ids = Vec::new();
            for waiting_item in &session.items {
                if waiting_item.status != ImportItemStatus::WaitingLogin
                    || waiting_item.input.kind != ImportInputKind::Url
                {
                    continue;
                }
                let waiting_target = state.import_v2_service.resolve_web_target(
                    &waiting_item.input.locator,
                    waiting_item.input.normalized_locator.as_deref(),
                )?;
                if platform_matches_host(&reference.platform, &waiting_target.public.host) {
                    resumed_item_ids.push(waiting_item.item_id.clone());
                }
            }
            if resumed_item_ids.is_empty() {
                return Err(BackendError::new(
                    "IMPORT_V2_BROWSER_SESSION_FAILED",
                    "No waiting import items match the authenticated platform.",
                    false,
                    true,
                ));
            }
            let (reference, profile) = state
                .connector_session_service
                .authenticated_profile_bound(
                    &request.connector_session_id,
                    &request.project_id,
                    &request.import_session_id,
                    &request.item_id,
                    target.request_url.as_str(),
                )?;
            state.import_v2_service.bind_authenticated_profiles(
                &request.project_id,
                &request.import_session_id,
                &resumed_item_ids,
                &profile,
            )?;
            if let Err(error) = state
                .import_v2_service
                .mark_authenticated_login_group_authorized(
                    permit,
                    &state.file_store,
                    &request.import_session_id,
                    &resumed_item_ids,
                    reference.account_summary.as_deref(),
                )
            {
                let _ = state.import_v2_service.unbind_authenticated_profiles(
                    &request.project_id,
                    &request.import_session_id,
                    &resumed_item_ids,
                );
                return Err(error);
            }
            let tasks = match start_import_batch_for_state(
                app,
                &state,
                permit,
                StartImportBatchV2Request {
                    project_id: request.project_id.clone(),
                    project_root_path: request.project_root_path.clone(),
                    session_id: request.import_session_id.clone(),
                    item_ids: resumed_item_ids.clone(),
                    recovery_action: None,
                },
            ) {
                Ok(task) => vec![task],
                Err(error) => {
                    let _ = state.import_v2_service.unbind_authenticated_profiles(
                        &request.project_id,
                        &request.import_session_id,
                        &resumed_item_ids,
                    );
                    let _ = state
                        .import_v2_service
                        .clear_authenticated_login_group_authorized(
                            permit,
                            &state.file_store,
                            &request.import_session_id,
                            &resumed_item_ids,
                        );
                    return Err(error);
                }
            };
            Ok(CompleteLoginResult {
                connector_session: reference,
                resumed_item_ids,
                tasks,
            })
        },
    )
}
pub async fn authorize_import_private_target_v2(
    app: AppHandle,
    request: AuthorizePrivateTargetRequest,
) -> Result<String, BackendError> {
    let coordinator = app.state::<AppState>().blocking_work.clone();
    let preflight_app = app.clone();
    let preflight_request = request.clone();
    let (context, execution, target, port) = coordinator
        .run(crate::services::BlockingWorkClass::HeavyIo, move || {
            let state = preflight_app.state::<AppState>();
            let context = state.resolve_project_context(
                &preflight_request.project_id,
                &preflight_request.project_root_path,
            )?;
            let session = state.import_v2_service.load_session(
                &context,
                &state.file_store,
                &preflight_request.session_id,
            )?;
            let item = session
                .items
                .iter()
                .find(|item| item.item_id == preflight_request.item_id)
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
                    "IMPORT_V2_URL_REJECTED",
                    "Private authorization is available only for URL imports.",
                    false,
                    true,
                ));
            }
            let target = state.import_v2_service.resolve_web_target(
                &item.input.locator,
                item.input.normalized_locator.as_deref(),
            )?;
            let confirmed = UrlPolicy.normalize_for_session(&preflight_request.url)?;
            if confirmed.public != target.public {
                return Err(BackendError::new(
                    "IMPORT_V2_URL_REFERENCE_MISMATCH",
                    "The confirmed private target does not match this import item.",
                    false,
                    true,
                ));
            }
            let port = target.request_url.port_or_known_default().ok_or_else(|| {
                BackendError::new(
                    "IMPORT_V2_URL_REJECTED",
                    "URL port is invalid.",
                    false,
                    true,
                )
            })?;
            let execution = state.begin_project_external_execution(
                &context,
                &format!(
                    "private-target-dns:{}:{}",
                    preflight_request.session_id, preflight_request.item_id
                ),
            )?;
            Ok((context, execution, target, port))
        })
        .await?;
    let ips = tokio::net::lookup_host((target.public.host.as_str(), port))
        .await
        .map_err(|_| {
            BackendError::new("IMPORT_V2_DNS_FAILED", "DNS resolution failed.", true, true)
        })?
        .map(|a| a.ip())
        .collect::<Vec<_>>();
    coordinator
        .run(crate::services::BlockingWorkClass::HeavyIo, move || {
            let state = app.state::<AppState>();
            state.require_current_execution_epoch(&context, &execution)?;
            let grant = PrivateTargetGrant {
                item_id: request.item_id.clone(),
                scheme: target.public.scheme,
                host: target.public.host,
                port,
                resolved_ips: ips,
                expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
            };
            state.with_current_project_write_access(
                &request.project_id,
                &request.project_root_path,
                |_permit, _context| state.import_v2_service.authorize_private_target(grant),
            )
        })
        .await
}

pub fn authorize_local_asr_v2(
    state: State<'_, AppState>,
    request: AuthorizeLocalAsrV2Request,
) -> Result<(), BackendError> {
    authorize_local_asr(&state, request, None)
}

pub fn authorize_local_ocr_v2(
    state: State<'_, AppState>,
    request: AuthorizeLocalOcrV2Request,
) -> Result<(), BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, _context| {
            state
                .import_v2_service
                .authorize_media_for_session_authorized(
                    permit,
                    &state.file_store,
                    &request.session_id,
                    &request.item_id,
                    ImportMediaAuthorizationKind::Ocr,
                    None,
                    None,
                )
        },
    )
}

pub fn authorize_bilibili_asr_v2(
    state: State<'_, AppState>,
    request: AuthorizeBilibiliAsrV2Request,
) -> Result<(), BackendError> {
    authorize_local_asr(&state, request, None)
}

pub(crate) fn authorize_local_asr(
    state: &State<'_, AppState>,
    request: AuthorizeLocalAsrV2Request,
    expected_item: Option<&ImportItem>,
) -> Result<(), BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, context| {
            let session = state.import_v2_service.load_session(
                context,
                &state.file_store,
                &request.session_id,
            )?;
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
            validate_local_asr_item(item)?;
            let asr_grant = if item.input.kind == ImportInputKind::Url {
                let target = state.import_v2_service.resolve_web_target(
                    &item.input.locator,
                    item.input.normalized_locator.as_deref(),
                )?;
                validate_local_asr_host(&target.public.host)?;
                Some(BilibiliAsrGrant {
                    project_id: request.project_id.clone(),
                    session_id: request.session_id.clone(),
                    item_id: request.item_id.clone(),
                    target_sha256: asr_target_sha256(target.request_url.as_str()),
                    expires_at: chrono::Utc::now() + chrono::Duration::minutes(5),
                })
            } else {
                None
            };
            if let Some(expected_item) = expected_item {
                state
                    .import_v2_service
                    .authorize_media_for_session_if_unchanged_authorized(
                        permit,
                        &state.file_store,
                        &request.session_id,
                        &request.item_id,
                        ImportMediaAuthorizationKind::Asr,
                        Some(request.profile.clone()),
                        request.language.clone(),
                        expected_item,
                    )?;
            } else {
                state
                    .import_v2_service
                    .authorize_media_for_session_authorized(
                        permit,
                        &state.file_store,
                        &request.session_id,
                        &request.item_id,
                        ImportMediaAuthorizationKind::Asr,
                        Some(request.profile.clone()),
                        request.language.clone(),
                    )?;
            }
            if let Some(grant) = asr_grant {
                state.import_v2_service.authorize_bilibili_asr(grant)?;
            }
            Ok(())
        },
    )
}

fn validate_local_asr_item(item: &ImportItem) -> Result<(), BackendError> {
    if !matches!(
        item.status,
        ImportItemStatus::WaitingAuthorization
            | ImportItemStatus::WaitingCapability
            | ImportItemStatus::Failed
    ) {
        return Err(BackendError::new(
            "IMPORT_V2_STATE_INVALID",
            "Local ASR can be authorized only for a media item currently waiting for recognition.",
            false,
            true,
        ));
    }
    Ok(())
}

fn validate_local_asr_host(host: &str) -> Result<(), BackendError> {
    if !["bilibili", "xiaohongshu", "douyin"]
        .into_iter()
        .any(|platform| platform_matches_host(platform, host))
    {
        return Err(BackendError::new(
            "IMPORT_V2_URL_REJECTED",
            "Local ASR authorization is limited to an exact supported-media import target.",
            false,
            true,
        ));
    }
    Ok(())
}

fn platform_matches_host(platform: &str, host: &str) -> bool {
    match platform {
        "wechat" => host == "mp.weixin.qq.com",
        "zhihu" => host == "zhihu.com" || host.ends_with(".zhihu.com"),
        "bilibili" => host == "b23.tv" || host == "bilibili.com" || host.ends_with(".bilibili.com"),
        "xiaohongshu" => {
            host == "xiaohongshu.com"
                || host.ends_with(".xiaohongshu.com")
                || host == "xhslink.com"
                || host.ends_with(".xhslink.com")
                || host == "xhslink.cn"
                || host.ends_with(".xhslink.cn")
        }
        "douyin" => {
            host == "douyin.com"
                || host.ends_with(".douyin.com")
                || host == "iesdouyin.com"
                || host.ends_with(".iesdouyin.com")
        }
        "x" => {
            host == "x.com"
                || host.ends_with(".x.com")
                || host == "twitter.com"
                || host.ends_with(".twitter.com")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::import_v2::{ImportIssue, ImportStage};

    fn failed_url_item(code: &str, url: &str) -> ImportItem {
        let mut item = ImportItem::queued(
            "item-a",
            ImportInput {
                kind: ImportInputKind::Url,
                display_name: url.into(),
                locator: "import-web-target:opaque".into(),
                normalized_locator: Some(url.into()),
                source_identity: None,
                media_save_mode: Default::default(),
            },
        );
        item.status = ImportItemStatus::Failed;
        item.issue = Some(ImportIssue::for_web_code(code, ImportStage::Extract));
        item
    }

    #[test]
    fn local_asr_authorization_requires_exact_issue_and_supported_platform_host() {
        for (url, host) in [
            (
                "https://www.xiaohongshu.com/explore/abc",
                "www.xiaohongshu.com",
            ),
            ("https://www.douyin.com/video/123", "www.douyin.com"),
            (
                "https://www.bilibili.com/video/BV1exact",
                "www.bilibili.com",
            ),
        ] {
            let item = failed_url_item("IMPORT_WEB_SUBTITLE_UNAVAILABLE", url);
            assert!(validate_local_asr_item(&item).is_ok(), "{url}");
            assert!(validate_local_asr_host(host).is_ok(), "{host}");
        }
        assert_eq!(
            validate_local_asr_host("example.com").unwrap_err().code,
            "IMPORT_V2_URL_REJECTED"
        );
        let wrong_issue = failed_url_item(
            "IMPORT_WEB_STRUCTURE_CHANGED",
            "https://www.bilibili.com/video/BV1exact",
        );
        assert_eq!(
            validate_local_asr_item(&wrong_issue).unwrap_err().code,
            "IMPORT_V2_STATE_INVALID"
        );
    }
}
