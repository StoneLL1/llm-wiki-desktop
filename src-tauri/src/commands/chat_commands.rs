use tauri::{AppHandle, Manager, State};

use crate::app_state::{AppState, ProjectWritePermit};
use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::chat::{
    ChatAffectedPathHash, ChatConvenienceEdit, ChatConvenienceEditStatus, ChatMessage, ChatRoute,
    ChatSession, ChatSessionSummary, CreateChatSessionRequest, DeleteChatRequest, ListChatsRequest,
    LoadChatRequest, RenameChatRequest, ResolveChatConvenienceEditRequest,
    RollbackLastChatConvenienceEditRequest, SaveAnswerResult, SaveAnswerToWikiRequest,
    SendChatMessageRequest,
};
use crate::models::compile::CompileRoutePreference;
use crate::models::confirmation::{
    ActionPreview, ConfirmationExecution, ConfirmationStatus, PendingAction, PendingActionType,
    RiskLevel,
};
use crate::models::git::CheckpointPurpose;
use crate::models::git::GitChangedFile;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::task::{
    BackendTask, TaskActivity, TaskActivityStatus, TaskResult, TaskStatus, TaskType,
};
use crate::services::{
    AgentService, ChatIntent, ConvenienceAuditStatus, LlmService, RetrievalContext,
};
use crate::tasks::task_model::LogLevel;

const MAX_CHAT_CONTENT_CHARS: usize = 32_000;

#[tauri::command]
pub fn create_chat_session(
    state: State<'_, AppState>,
    request: CreateChatSessionRequest,
) -> Result<ChatSession, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            state.chat_service.create_session(
                context,
                request.title.as_deref(),
                request.context_page_path.as_deref(),
            )
        },
    )
}

#[tauri::command]
pub fn list_chat_sessions(
    state: State<'_, AppState>,
    request: ListChatsRequest,
) -> Result<Vec<ChatSessionSummary>, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.chat_service.list_sessions(&context)
}

#[tauri::command]
pub fn load_chat_session(
    state: State<'_, AppState>,
    request: LoadChatRequest,
) -> Result<ChatSession, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state
        .chat_service
        .load_session(&context, &request.session_id)
}

#[tauri::command]
pub fn rename_chat_session(
    state: State<'_, AppState>,
    request: RenameChatRequest,
) -> Result<ChatSession, BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            state
                .chat_service
                .rename_session(context, &request.session_id, &request.title)
        },
    )
}

#[tauri::command]
pub fn delete_chat_session(
    state: State<'_, AppState>,
    request: DeleteChatRequest,
) -> Result<(), BackendError> {
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            // Sending holds this process-wide guard until its task has finished
            // persisting the assistant turn (or cleaning up after cancellation).
            let _send_guard = state.chat_service.try_acquire_send()?;
            let chat_root = context.layout.chat_state_root.as_deref().ok_or_else(|| {
                BackendError::new(
                    "PROJECT_LAYOUT_STATE_UNAVAILABLE",
                    "Project chat state is unavailable until compatible features are enabled.",
                    true,
                    true,
                )
            })?;
            let session_path = format!("{chat_root}/{}.json", request.session_id);
            state.git_service.create_scoped_checkpoint(
                context,
                CheckpointPurpose::HighRiskOperation,
                "Before deleting Chat session",
                std::slice::from_ref(&session_path),
            )?;
            state
                .chat_service
                .delete_session(context, &request.session_id)
        },
    )
}

/// Send a chat message: append the user turn, retrieve local context, resolve
/// the Agent/BYOK route, run the model as a cancellable background task, and
/// persist the assistant answer + citations back to the session.
#[tauri::command]
pub fn send_chat_message(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SendChatMessageRequest,
) -> Result<BackendTask, BackendError> {
    let content = validate_chat_content(&request.content)?;
    let request = SendChatMessageRequest { content, ..request };
    let send_guard = state.chat_service.try_acquire_send()?;
    let (context, task) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            let task = state
                .task_service
                .create_project_task(
                    TaskType::LlmRequest,
                    request.project_id.clone(),
                    context.root.clone(),
                    format!("Chat: {}", truncate_title(&request.content)),
                    true,
                )
                .map_err(task_error)?;
            Ok((context.clone(), task))
        },
    )?;
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let _send_guard = send_guard;
        let state = app.state::<AppState>();
        if let Err(error) = run_chat_send(&state, request, &context, &task_id).await {
            let _ = state
                .task_service
                .append_log(&task_id, LogLevel::Error, error.message.clone());
            let _ = state.task_service.set_error(&task_id, error);
            match state.task_service.get_task(&task_id).map(|t| t.status) {
                Some(TaskStatus::Cancelling) => {
                    let _ = state
                        .task_service
                        .transition_status(&task_id, TaskStatus::Cancelled);
                }
                Some(TaskStatus::Cancelled) => {}
                _ => {
                    let _ = state
                        .task_service
                        .transition_status(&task_id, TaskStatus::Failed);
                }
            }
        }
    });
    Ok(task)
}

async fn run_chat_send(
    state: &AppState,
    request: SendChatMessageRequest,
    context: &ProjectContext,
    task_id: &str,
) -> Result<(), BackendError> {
    if state.task_service.is_cancelled(task_id) {
        return Err(chat_cancelled_error());
    }
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, _| {
            state
                .task_service
                .transition_status(task_id, TaskStatus::Running)
                .map_err(task_error)
        },
    )?;
    let mut session = state
        .chat_service
        .load_session(context, &request.session_id)?;
    let user_message = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: crate::models::chat::ChatRole::User,
        content: request.content.clone(),
        created_at: crate::utils::time_utils::now_rfc3339(),
        citations: Vec::new(),
        route: None,
        provider: None,
        task_id: None,
        convenience_edit: None,
        retrieval_diagnostics: None,
        saved_path: None,
    };
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, current| {
            state
                .chat_service
                .append_message(current, &mut session, user_message)
        },
    )?;

    state
        .task_service
        .append_log(task_id, LogLevel::Info, "Retrieving local context".into())
        .map_err(task_error)?;
    state.task_service.emit_activity(
        task_id,
        TaskActivity::Phase {
            name: "retrieval".into(),
            status: TaskActivityStatus::Started,
            label: Some("Retrieving local context".into()),
        },
    );
    let language = state
        .settings_service
        .read_settings(context)
        .map(|settings| settings.language)
        .unwrap_or_else(|_| "en".to_string());
    let intent = state
        .chat_convenience_service
        .classify_chat_intent(&request.content);
    if should_use_convenience_flow(request.convenience_enabled, intent) {
        let retrieval = state.chat_service.build_convenience_retrieval_context(
            context,
            &state.search_service,
            &request.content,
            &session,
            &language,
            request.pinned_page_path.as_deref(),
        )?;
        state.task_service.emit_activity(
            task_id,
            TaskActivity::Phase {
                name: "retrieval".into(),
                status: TaskActivityStatus::Completed,
                label: Some("Local context ready".into()),
            },
        );
        return run_chat_convenience_send(
            state,
            request,
            context,
            task_id,
            &mut session,
            retrieval,
        )
        .await;
    }

    let resolved = resolve_route(
        state,
        context,
        request.route,
        request.agent,
        request.provider,
    )?;
    let (planned_route, context_window) = match &resolved {
        ResolvedRoute::Agent(_) => (ChatRoute::Agent, None),
        ResolvedRoute::Byok(provider) => (ChatRoute::Byok, Some(provider.context_window)),
    };
    let retrieval = state.chat_service.build_retrieval_context(
        context,
        &state.search_service,
        &request.content,
        &session,
        &language,
        planned_route,
        context_window,
        request.pinned_page_path.as_deref(),
    )?;
    state.task_service.emit_activity(
        task_id,
        TaskActivity::Phase {
            name: "retrieval".into(),
            status: TaskActivityStatus::Completed,
            label: Some("Local context ready".into()),
        },
    );

    let execution_lease = state.begin_project_external_task(context, task_id)?;
    let (route, answer, provider) = match resolved {
        ResolvedRoute::Agent(kind) => {
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!("Running {}", kind.command()),
                )
                .map_err(task_error)?;
            let workspace = context.root.clone();
            let invocation = AgentService::chat_invocation(kind, &workspace, &retrieval.prompt)?;
            // Stream the agent's stdout lines to the task stream channel so the
            // chat UI can render the answer incrementally (uniform with BYOK).
            let task_service = &state.task_service;
            let task_id_owned = task_id.to_string();
            let on_delta = move |delta: &str| {
                task_service.emit_stream_delta(
                    &task_id_owned,
                    crate::models::task::StreamDelta {
                        delta: delta.to_string(),
                        route: Some("chat-agent".to_string()),
                    },
                );
            };
            let task_service = &state.task_service;
            let task_id_owned = task_id.to_string();
            let on_activity = move |activity: TaskActivity| {
                task_service.emit_activity(&task_id_owned, activity);
            };
            state.task_service.emit_activity(
                task_id,
                TaskActivity::Phase {
                    name: "agent".into(),
                    status: TaskActivityStatus::Started,
                    label: Some(format!("Running {}", kind.command())),
                },
            );
            let captured = match state.agent_service.run_task_streaming_with_events(
                &invocation,
                &state.task_service,
                task_id,
                &on_delta,
                &on_activity,
            ) {
                Ok(captured) => captured,
                Err(error) => {
                    state.task_service.emit_activity(
                        task_id,
                        TaskActivity::Phase {
                            name: "agent".into(),
                            status: TaskActivityStatus::Failed,
                            label: Some("Agent response failed".into()),
                        },
                    );
                    return Err(error);
                }
            };
            state.task_service.emit_activity(
                task_id,
                TaskActivity::Phase {
                    name: "agent".into(),
                    status: TaskActivityStatus::Completed,
                    label: Some("Agent response ready".into()),
                },
            );
            (ChatRoute::Agent, captured.trim().to_string(), None)
        }
        ResolvedRoute::Byok(provider) => {
            let provider_kind = provider.provider;
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!("Calling {:?}", provider_kind),
                )
                .map_err(task_error)?;
            state.task_service.emit_activity(
                task_id,
                TaskActivity::Phase {
                    name: "generation".into(),
                    status: TaskActivityStatus::Started,
                    label: Some("Calling BYOK provider".into()),
                },
            );
            let secret = crate::services::LlmService::bound_secret_for_config(
                context,
                &state.secret_service,
                &provider,
            )?;
            // Real streaming: each text delta is forwarded to the task stream
            // channel for live rendering, and cancellation is polled between
            // chunks. The fully assembled text is returned for persistence.
            let task_service = &state.task_service;
            let task_id_owned = task_id.to_string();
            let raw = match state
                .llm_service
                .complete_streaming(
                    &provider,
                    secret.as_deref(),
                    &retrieval.prompt,
                    move || task_service.is_cancelled(task_id),
                    move |delta| {
                        task_service.emit_stream_delta(
                            &task_id_owned,
                            crate::models::task::StreamDelta {
                                delta: delta.to_string(),
                                route: Some("chat-byok".to_string()),
                            },
                        );
                    },
                )
                .await
            {
                Ok(text) => text,
                Err(error) if error.code == "LLM_CANCELLED" => {
                    return Err(BackendError::new(
                        "CHAT_CANCELLED",
                        "Chat was cancelled.",
                        true,
                        false,
                    ))
                }
                Err(error) => return Err(error),
            };
            state.task_service.emit_activity(
                task_id,
                TaskActivity::Phase {
                    name: "generation".into(),
                    status: TaskActivityStatus::Completed,
                    label: Some("Provider response ready".into()),
                },
            );
            (ChatRoute::Byok, raw.trim().to_string(), Some(provider_kind))
        }
    };

    if state.task_service.is_cancelled(task_id) {
        return Err(BackendError::new(
            "CHAT_CANCELLED",
            "Chat was cancelled.",
            true,
            false,
        ));
    }
    state.require_current_execution_epoch(context, &execution_lease)?;

    let parsed =
        crate::services::ChatService::parse_model_citations(&answer, &retrieval.source_refs);
    let mut diagnostics = retrieval.diagnostics;
    diagnostics.invalid_citation_ids = parsed.invalid_source_ids;
    diagnostics.has_unverified = parsed.has_unverified;

    let assistant_message = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: crate::models::chat::ChatRole::Assistant,
        content: answer,
        created_at: crate::utils::time_utils::now_rfc3339(),
        citations: parsed.citations,
        route: Some(route),
        provider,
        task_id: Some(task_id.to_string()),
        convenience_edit: None,
        retrieval_diagnostics: Some(diagnostics),
        saved_path: None,
    };
    let assistant_message_id = assistant_message.id.clone();
    // Check cancellation again while holding the session mutation lock so a
    // cancel that races with another writer cannot persist an abandoned answer.
    let appended = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, current| {
            state
                .chat_service
                .append_message_if(current, &mut session, assistant_message, || {
                    state.task_service.is_cancelled(task_id)
                })
        },
    )?;
    if !appended {
        return Err(chat_cancelled_error());
    }

    let result = TaskResult {
        summary: "Chat answer ready.".into(),
        affected_paths: vec![format!(".app/chats/{}.json", session.id)],
        reference: None,
        pending_action: None,
    };
    if let Err(error) = state
        .task_service
        .complete_running_with_result(task_id, result)
    {
        let _ = state.chat_service.remove_message_if(
            context,
            &session.id,
            &assistant_message_id,
            task_id,
            || true,
        );
        return if state.task_service.is_cancelled(task_id) {
            Err(chat_cancelled_error())
        } else {
            Err(task_error(error))
        };
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_chat_convenience_send(
    state: &AppState,
    request: SendChatMessageRequest,
    context: &ProjectContext,
    task_id: &str,
    session: &mut ChatSession,
    retrieval: RetrievalContext,
) -> Result<(), BackendError> {
    let authorization = state
        .settings_service
        .get_chat_convenience_authorization(context)?;
    if !authorization.enabled {
        return Err(BackendError::new(
            "CHAT_CONVENIENCE_UNAUTHORIZED",
            "Chat convenience mode is not authorized for this project.",
            true,
            true,
        ));
    }
    // Cover the checkpoint itself as well as the Agent process and result
    // commit. Revocation must not return while Git can still mutate the tree.
    let execution_lease = state.begin_project_external_task(context, task_id)?;

    let (checkpoint, ignored_baseline) = state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, current| {
            state.require_current_execution_permit(permit, &execution_lease)?;
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    "Creating Git checkpoint before Chat convenience edit".into(),
                )
                .map_err(task_error)?;
            let checkpoint = state.git_service.create_checkpoint(
                current,
                CheckpointPurpose::HighRiskOperation,
                "Before Chat convenience edit",
            )?;
            let ignored_baseline = state.git_service.ignored_paths(current)?;
            Ok((checkpoint, ignored_baseline))
        },
    )?;

    let kind = resolve_convenience_agent(state, context, request.agent)?;
    state
        .task_service
        .append_log(
            task_id,
            LogLevel::Info,
            format!("Running {} in Chat convenience mode", kind.command()),
        )
        .map_err(task_error)?;
    let prompt = format!(
        "{}{}",
        retrieval.prompt,
        state.chat_convenience_service.convenience_prompt_suffix()
    );
    let invocation = AgentService::chat_convenience_invocation(kind, &context.root, &prompt)?;
    let task_service = &state.task_service;
    let task_id_owned = task_id.to_string();
    let on_delta = move |delta: &str| {
        task_service.emit_stream_delta(
            &task_id_owned,
            crate::models::task::StreamDelta {
                delta: delta.to_string(),
                route: Some("chat-agent".to_string()),
            },
        );
    };
    let task_service = &state.task_service;
    let task_id_owned = task_id.to_string();
    let on_activity = move |activity: TaskActivity| {
        task_service.emit_activity(&task_id_owned, activity);
    };
    state.task_service.emit_activity(
        task_id,
        TaskActivity::Phase {
            name: "agent".into(),
            status: TaskActivityStatus::Started,
            label: Some(format!(
                "Running {} in Chat convenience mode",
                kind.command()
            )),
        },
    );
    let answer = match state.agent_service.run_task_streaming_with_events(
        &invocation,
        &state.task_service,
        task_id,
        &on_delta,
        &on_activity,
    ) {
        Ok(answer) => answer.trim().to_string(),
        Err(error) => {
            state.task_service.emit_activity(
                task_id,
                TaskActivity::Phase {
                    name: "agent".into(),
                    status: TaskActivityStatus::Failed,
                    label: Some("Agent response failed".into()),
                },
            );
            return Err(cleanup_convenience_failure(
                state,
                context,
                task_id,
                &ignored_baseline,
                error,
            ));
        }
    };
    state.task_service.emit_activity(
        task_id,
        TaskActivity::Phase {
            name: "agent".into(),
            status: TaskActivityStatus::Completed,
            label: Some("Agent response ready".into()),
        },
    );

    if state.task_service.is_cancelled(task_id) {
        return Err(cleanup_convenience_failure(
            state,
            context,
            task_id,
            &ignored_baseline,
            chat_cancelled_error(),
        ));
    }
    let checkpoint_hash = checkpoint.commit_hash;
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |permit, _current| {
            state.require_current_execution_permit(permit, &execution_lease)?;
            commit_chat_convenience_result(
                state,
                permit,
                task_id,
                session,
                retrieval,
                answer,
                checkpoint_hash,
                &ignored_baseline,
            )
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_chat_convenience_result(
    state: &AppState,
    permit: &ProjectWritePermit<'_>,
    task_id: &str,
    session: &mut ChatSession,
    retrieval: RetrievalContext,
    answer: String,
    checkpoint_hash: Option<String>,
    ignored_baseline: &[String],
) -> Result<(), BackendError> {
    let context = permit.context();
    let mut changes = state
        .git_service
        .changed_files_since_head_with_ignored_baseline(context, &ignored_baseline)?;
    changes.retain(|change| !is_current_task_runtime_path(task_id, change));
    let audit = state.chat_convenience_service.audit_git_changes(changes);
    let affected_path_hashes = audit
        .affected_paths
        .iter()
        .map(|path| {
            Ok(ChatAffectedPathHash {
                path: path.clone(),
                hash: state.chat_service.file_hash_if_exists(context, path)?,
            })
        })
        .collect::<Result<Vec<_>, BackendError>>()?;
    let diff_text = state
        .git_service
        .diff_since_head(context)
        .ok()
        .map(|diff| filter_current_task_diff(task_id, &diff));
    let violation_reason = audit.violation_reason.clone();

    let (content, status, rollback_task_id) = match audit.status {
        ConvenienceAuditStatus::Passed => (answer, ChatConvenienceEditStatus::Applied, None),
        ConvenienceAuditStatus::SoftViolation => (
            answer,
            ChatConvenienceEditStatus::SoftViolationPending,
            None,
        ),
        ConvenienceAuditStatus::HardViolation => {
            let reason = violation_reason
                .clone()
                .unwrap_or_else(|| "Convenience edit violated project safety rules.".to_string());
            match state.git_service.rollback_paths_to_head_preserving_ignored(
                context,
                &audit.affected_paths,
                &ignored_baseline,
            ) {
                Ok(()) => (
                    format!("Chat convenience edit was rolled back: {reason}"),
                    ChatConvenienceEditStatus::RolledBack,
                    Some(task_id.to_string()),
                ),
                Err(error) => {
                    return Err(convenience_cleanup_error(
                        task_id,
                        &audit.affected_paths,
                        &reason,
                        error,
                    ));
                }
            }
        }
    };

    let parsed =
        crate::services::ChatService::parse_model_citations(&content, &retrieval.source_refs);
    let mut diagnostics = retrieval.diagnostics;
    diagnostics.invalid_citation_ids = parsed.invalid_source_ids;
    diagnostics.has_unverified = parsed.has_unverified;

    let assistant_message = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: crate::models::chat::ChatRole::Assistant,
        content,
        created_at: crate::utils::time_utils::now_rfc3339(),
        citations: parsed.citations,
        route: Some(ChatRoute::Agent),
        provider: None,
        task_id: Some(task_id.to_string()),
        convenience_edit: Some(ChatConvenienceEdit {
            status,
            checkpoint_hash,
            affected_paths: audit.affected_paths.clone(),
            diff_summary: audit.diff_summary.clone(),
            diff_text: diff_text.filter(|diff| !diff.trim().is_empty()),
            violation_reason,
            rollback_task_id,
            ignored_baseline_paths: ignored_baseline.to_vec(),
            affected_path_hashes,
        }),
        retrieval_diagnostics: Some(diagnostics),
        saved_path: None,
    };
    let assistant_message_id = assistant_message.id.clone();
    if !state
        .chat_service
        .append_message_if(context, session, assistant_message, || {
            state.task_service.is_cancelled(task_id)
        })?
    {
        return Err(cleanup_convenience_failure(
            state,
            context,
            task_id,
            &ignored_baseline,
            chat_cancelled_error(),
        ));
    }

    let mut affected_paths = vec![format!(".app/chats/{}.json", session.id)];
    affected_paths.extend(audit.affected_paths);
    let result = TaskResult {
        summary: "Chat convenience edit finished.".into(),
        affected_paths,
        reference: None,
        pending_action: None,
    };
    if let Err(error) = state
        .task_service
        .complete_running_with_result(task_id, result)
    {
        let cancelled = state.task_service.is_cancelled(task_id);
        let _ = state.chat_service.remove_message_if(
            context,
            &session.id,
            &assistant_message_id,
            task_id,
            || true,
        );
        return if cancelled {
            Err(cleanup_convenience_failure(
                state,
                context,
                task_id,
                &ignored_baseline,
                chat_cancelled_error(),
            ))
        } else {
            Err(task_error(error))
        };
    }
    Ok(())
}

fn should_use_convenience_flow(enabled: bool, intent: ChatIntent) -> bool {
    enabled && matches!(intent, ChatIntent::Write)
}

fn chat_cancelled_error() -> BackendError {
    BackendError::new("CHAT_CANCELLED", "Chat was cancelled.", true, false)
}

fn cleanup_convenience_failure(
    state: &AppState,
    context: &ProjectContext,
    task_id: &str,
    ignored_baseline: &[String],
    original: BackendError,
) -> BackendError {
    let mut changes = match state
        .git_service
        .changed_files_since_head_with_ignored_baseline(context, ignored_baseline)
    {
        Ok(changes) => changes,
        Err(error) => {
            return convenience_cleanup_error(task_id, &[], &original.message, error).with_details(
                serde_json::json!({
                    "original": original,
                    "cleanup": "audit_failed",
                }),
            );
        }
    };
    changes.retain(|change| !is_current_task_runtime_path(task_id, change));
    if changes.is_empty() {
        return original;
    }
    let paths: Vec<String> = changes.into_iter().map(|change| change.path).collect();
    // An Agent error/cancellation does not provide a write-set. Any path that
    // changed while it was running could also have been edited by the user;
    // never guess ownership and roll it back automatically. Surface the
    // exact paths so the user can review them against the checkpoint instead.
    convenience_partial_edit_error(task_id, &paths, original)
}

fn convenience_partial_edit_error(
    task_id: &str,
    paths: &[String],
    original: BackendError,
) -> BackendError {
    BackendError::new(
        "CHAT_CONVENIENCE_REVIEW_REQUIRED",
        format!(
            "Chat convenience stopped, but partial edits remain for review: {}",
            paths.join(", ")
        ),
        true,
        true,
    )
    .with_details(serde_json::json!({
        "taskId": task_id,
        "affectedPaths": paths,
        "original": original,
        "cleanup": "manual_review_required",
    }))
}

fn convenience_cleanup_error(
    task_id: &str,
    paths: &[String],
    reason: &str,
    cleanup: BackendError,
) -> BackendError {
    BackendError::new(
        "CHAT_CONVENIENCE_CLEANUP_FAILED",
        format!(
            "Chat convenience run stopped ({reason}), but its partial edits could not be safely cleaned up: {}",
            cleanup.message
        ),
        true,
        true,
    )
    .with_details(serde_json::json!({
        "taskId": task_id,
        "affectedPaths": paths,
        "cleanupError": cleanup,
    }))
}

fn is_current_task_runtime_path(task_id: &str, change: &GitChangedFile) -> bool {
    let path = change.path.replace('\\', "/");
    path == format!(".app/tasks/{task_id}.json") || path == format!(".app/tasks/{task_id}.log")
}

fn filter_current_task_diff(task_id: &str, diff: &str) -> String {
    let task_json = format!(".app/tasks/{task_id}.json");
    let task_log = format!(".app/tasks/{task_id}.log");
    let mut filtered = String::new();
    let mut current_block = String::new();
    let mut skip_current = false;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            if !skip_current {
                filtered.push_str(&current_block);
            }
            current_block.clear();
            skip_current = line.contains(&task_json) || line.contains(&task_log);
        } else if current_block.is_empty() {
            skip_current = line.contains(&task_json) || line.contains(&task_log);
        }
        current_block.push_str(line);
        current_block.push('\n');
    }
    if !skip_current {
        filtered.push_str(&current_block);
    }
    filtered
}

fn resolve_convenience_agent(
    state: &AppState,
    context: &ProjectContext,
    explicit_agent: Option<AgentKind>,
) -> Result<AgentKind, BackendError> {
    let agent_config = AgentService::load_config(context)?;
    let detected = state.agent_service.detect_agents(explicit_agent);
    let is_installed = |kind: AgentKind| {
        if !AgentService::supports_convenience_project_chat(kind) {
            return false;
        }
        detected
            .iter()
            .any(|info| info.kind == kind && info.state == AgentDetectionState::Installed)
    };
    if let Some(kind) = explicit_agent {
        if is_installed(kind) {
            return Ok(kind);
        }
        return Err(BackendError::new(
            "AGENT_UNAVAILABLE",
            "The selected Agent CLI is not installed or not usable.",
            true,
            true,
        ));
    }
    if let Some(kind) = agent_config
        .default_agent
        .filter(|kind| is_installed(*kind))
    {
        return Ok(kind);
    }
    AgentKind::ALL
        .into_iter()
        .find(|kind| is_installed(*kind))
        .ok_or_else(|| {
            BackendError::new(
                "AGENT_UNAVAILABLE",
                "No installed supported Agent CLI is available for Chat convenience mode. Use Claude or Codex.",
                true,
                true,
            )
        })
}

#[derive(Debug)]
enum ResolvedRoute {
    Agent(AgentKind),
    Byok(LlmProviderConfig),
}

fn resolve_route(
    state: &AppState,
    context: &ProjectContext,
    preference: CompileRoutePreference,
    explicit_agent: Option<AgentKind>,
    explicit_provider: Option<LlmProviderKind>,
) -> Result<ResolvedRoute, BackendError> {
    let agent_config = AgentService::load_config(context)?;
    let providers = LlmService::list_providers(context)?;
    let selected_agent = explicit_agent.or(agent_config.default_agent);
    let usable_agent = selected_agent.filter(|kind| {
        AgentService::supports_read_only_project_chat(*kind)
            && state
                .agent_service
                .detect_agents(Some(*kind))
                .iter()
                .any(|info| info.kind == *kind && info.state == AgentDetectionState::Installed)
    });
    let selected_provider = select_provider(
        context,
        explicit_provider,
        &providers,
        &state.secret_service,
    )?;
    decide_route(preference, usable_agent, selected_provider)
}

/// Pure routing decision (no I/O) extracted from [`resolve_route`] so the
/// Auto/Agent/BYOK discrimination is unit-testable without an `AppState`.
///
/// - `Agent` preference → always Agent (error if none usable).
/// - `Byok` preference → always BYOK (error if no provider).
/// - `Auto` → Agent when a usable Agent is present, otherwise BYOK.
fn decide_route(
    preference: CompileRoutePreference,
    usable_agent: Option<AgentKind>,
    selected_provider: Option<LlmProviderConfig>,
) -> Result<ResolvedRoute, BackendError> {
    let use_agent = match preference {
        CompileRoutePreference::Agent => true,
        CompileRoutePreference::Byok => false,
        CompileRoutePreference::Auto => usable_agent.is_some(),
    };
    if use_agent {
        if let Some(kind) = usable_agent {
            return Ok(ResolvedRoute::Agent(kind));
        }
        return Err(BackendError::new(
            "AGENT_UNAVAILABLE",
            "No usable Agent CLI is configured. Install an Agent or switch to a BYOK provider.",
            true,
            true,
        ));
    }
    match selected_provider {
        Some(provider) => Ok(ResolvedRoute::Byok(provider)),
        None => Err(BackendError::new(
            "LLM_PROVIDER_MISSING",
            "No enabled BYOK provider with a configured secret is available.",
            true,
            true,
        )),
    }
}

fn select_provider(
    context: &ProjectContext,
    explicit: Option<LlmProviderKind>,
    providers: &[LlmProviderConfig],
    secrets: &crate::services::SecretService,
) -> Result<Option<LlmProviderConfig>, BackendError> {
    if let Some(kind) = explicit {
        let provider = providers
            .iter()
            .find(|p| p.enabled && p.provider == kind)
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    "LLM_PROVIDER_MISSING",
                    "The selected BYOK provider is not enabled.",
                    true,
                    true,
                )
            })?;
        if !crate::services::LlmService::bound_secret_available(context, secrets, &provider)? {
            return Err(BackendError::new(
                "LLM_SECRET_MISSING",
                "The selected provider has no configured secret.",
                true,
                true,
            ));
        }
        return Ok(Some(provider));
    }
    for provider in providers.iter().filter(|p| p.enabled) {
        if crate::services::LlmService::bound_secret_available(context, secrets, provider)? {
            return Ok(Some(provider.clone()));
        }
    }
    Ok(None)
}

/// Save an assistant answer to `wiki/queries/` as a Markdown page.
#[tauri::command]
pub fn save_answer_to_wiki(
    state: State<'_, AppState>,
    request: SaveAnswerToWikiRequest,
) -> Result<SaveAnswerResult, BackendError> {
    // Keep wiki write + saved-path session mutation atomic with respect to
    // deletion and background Chat runs. Acquire before consuming an
    // overwrite confirmation so a busy request leaves the confirmation
    // pending and retryable.
    let _send_guard = state.chat_service.try_acquire_send()?;
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, _| Ok(()),
    )?;
    let mut project_id = request.project_id;
    let mut root_path = request.project_root_path;
    let mut session_id = request.session_id;
    let mut message_id = request.message_id;
    let mut target_path = request.target_path;
    let mut expected_hash = request.expected_hash;
    let allow_overwrite = request.allow_overwrite;

    if allow_overwrite {
        let action_id = request.action_id.as_deref().ok_or_else(|| {
            BackendError::new(
                "CONFIRMATION_REQUIRED",
                "A backend-issued overwrite confirmation is required.",
                true,
                true,
            )
        })?;
        let stored = state
            .confirmation_registry
            .confirm(action_id, ConfirmationStatus::Confirmed)?;
        let ConfirmationExecution::ChatOverwrite {
            project_id: stored_project_id,
            root_path: stored_root_path,
            session_id: stored_session_id,
            message_id: stored_message_id,
            target_path: stored_target_path,
            current_hash,
        } = stored.execution.ok_or_else(|| {
            BackendError::new(
                "CONFIRMATION_EXECUTION_MISSING",
                "Chat overwrite confirmation has no execution plan.",
                false,
                true,
            )
        })?
        else {
            return Err(BackendError::new(
                "CONFIRMATION_TYPE_MISMATCH",
                "The pending action is not a chat overwrite.",
                false,
                true,
            ));
        };
        project_id = stored_project_id;
        root_path = stored_root_path;
        session_id = stored_session_id;
        message_id = stored_message_id;
        target_path = Some(stored_target_path);
        expected_hash = Some(current_hash);
    }

    state.with_current_project_write_access(&project_id, &root_path, |_permit, context| {
        let session = state.chat_service.load_session(context, &session_id)?;
        let preceding: Vec<&ChatMessage> = session
            .messages
            .iter()
            .take_while(|m| m.id != message_id)
            .collect();
        let question = preceding
            .iter()
            .rev()
            .find(|m| m.role == crate::models::chat::ChatRole::User)
            .map(|m| (*m).clone())
            .ok_or_else(|| {
                BackendError::new(
                    "CHAT_QUESTION_NOT_FOUND",
                    "Could not find the question preceding this answer.",
                    true,
                    true,
                )
            })?;
        let answer = session
            .messages
            .iter()
            .find(|m| m.id == message_id)
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    "CHAT_MESSAGE_NOT_FOUND",
                    "The selected answer message no longer exists.",
                    true,
                    true,
                )
            })?;
        let (slug, markdown) = state
            .chat_service
            .build_answer_markdown(&session, &question, &answer);
        let result = state.chat_service.save_answer_to_wiki(
            context,
            &state.git_service,
            target_path.as_deref(),
            expected_hash.as_deref(),
            allow_overwrite,
            &markdown,
            &slug,
        );
        if let Err(error) = &result {
            if !allow_overwrite && error.code == "FILE_ALREADY_EXISTS" {
                let path = error
                    .details
                    .as_ref()
                    .and_then(|details| details.get("path"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let current_hash = error
                    .details
                    .as_ref()
                    .and_then(|details| details.get("currentHash"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let action = PendingAction {
                    id: uuid::Uuid::new_v4().to_string(),
                    action_type: PendingActionType::OverwriteFile,
                    title: "Overwrite saved chat answer".into(),
                    message: format!("Overwrite {path} under a Git checkpoint."),
                    risk_level: RiskLevel::High,
                    affected_paths: vec![path.clone()],
                    preview: Some(ActionPreview {
                        summary: "Replace the existing query page with this chat answer.".into(),
                        before: None,
                        after: Some(markdown.clone()),
                        diff: None,
                    }),
                    expires_at: None,
                    // The checkpoint is created only after the user confirms the
                    // overwrite, so there is no hash to surface yet.
                    checkpoint_hash: None,
                };
                state.confirmation_registry.register_with_execution(
                    action.clone(),
                    Some(ConfirmationExecution::ChatOverwrite {
                        project_id: project_id.clone(),
                        root_path: root_path.clone(),
                        session_id: session_id.clone(),
                        message_id: message_id.clone(),
                        target_path: path.clone(),
                        current_hash: current_hash.clone(),
                    }),
                )?;
                return Err(BackendError::new(
                    "FILE_ALREADY_EXISTS",
                    "A query page already exists at this path. Confirm to overwrite.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({
                    "path": path,
                    "currentHash": current_hash,
                    "actionId": action.id,
                    "pendingAction": action,
                })));
            }
        }
        let result = result?;
        state
            .chat_service
            .mark_answer_saved(context, &session_id, &message_id, &result.path)?;
        Ok(result)
    })
}

#[tauri::command]
pub fn resolve_chat_convenience_edit(
    state: State<'_, AppState>,
    request: ResolveChatConvenienceEditRequest,
) -> Result<ChatSession, BackendError> {
    let _send_guard = state.chat_service.try_acquire_send()?;
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            let mut session = state
                .chat_service
                .load_session(context, &request.session_id)?;
            let index = session
                .messages
                .iter()
                .position(|message| message.id == request.message_id)
                .ok_or_else(|| {
                    BackendError::new(
                        "CHAT_MESSAGE_NOT_FOUND",
                        "The selected chat message no longer exists.",
                        true,
                        true,
                    )
                })?;

            if request.keep {
                let edit = session.messages[index]
                    .convenience_edit
                    .as_mut()
                    .ok_or_else(convenience_edit_missing)?;
                if edit.status != ChatConvenienceEditStatus::SoftViolationPending {
                    return Err(BackendError::new(
                        "CHAT_CONVENIENCE_NOT_PENDING",
                        "This convenience edit is not waiting for a keep or rollback decision.",
                        true,
                        true,
                    ));
                }
                edit.status = ChatConvenienceEditStatus::KeptAfterSoftViolation;
                state.chat_service.save_session(context, &session)?;
                return Ok(session);
            }

            rollback_convenience_message(&state, context, &mut session, index)?;
            state.chat_service.save_session(context, &session)?;
            Ok(session)
        },
    )
}

#[tauri::command]
pub fn rollback_last_chat_convenience_edit(
    state: State<'_, AppState>,
    request: RollbackLastChatConvenienceEditRequest,
) -> Result<ChatSession, BackendError> {
    let _send_guard = state.chat_service.try_acquire_send()?;
    state.with_current_project_write_access(
        &request.project_id,
        &request.project_root_path,
        |_permit, context| {
            let mut session = state
                .chat_service
                .load_session(context, &request.session_id)?;
            let index = session
                .messages
                .iter()
                .rposition(|message| {
                    message.convenience_edit.as_ref().is_some_and(|edit| {
                        matches!(
                            edit.status,
                            ChatConvenienceEditStatus::Applied
                                | ChatConvenienceEditStatus::SoftViolationPending
                                | ChatConvenienceEditStatus::KeptAfterSoftViolation
                        )
                    })
                })
                .ok_or_else(|| {
                    BackendError::new(
                        "CHAT_CONVENIENCE_EDIT_MISSING",
                        "No rollbackable Chat convenience edit was found in this session.",
                        true,
                        true,
                    )
                })?;
            rollback_convenience_message(&state, context, &mut session, index)?;
            state.chat_service.save_session(context, &session)?;
            Ok(session)
        },
    )
}

fn rollback_convenience_message(
    state: &AppState,
    context: &ProjectContext,
    session: &mut ChatSession,
    index: usize,
) -> Result<(), BackendError> {
    let (checkpoint, ignored_baseline, affected_paths, affected_path_hashes) = {
        let edit = session.messages[index]
            .convenience_edit
            .as_ref()
            .ok_or_else(convenience_edit_missing)?;
        let checkpoint = edit.checkpoint_hash.clone().ok_or_else(|| {
            BackendError::new(
                "CHAT_ROLLBACK_CHECKPOINT_MISSING",
                "This convenience edit has no Git checkpoint to roll back to.",
                true,
                true,
            )
        })?;
        (
            checkpoint,
            edit.ignored_baseline_paths.clone(),
            edit.affected_paths.clone(),
            edit.affected_path_hashes.clone(),
        )
    };
    let current_head = state.git_service.repository_status(context)?.head;
    if current_head.as_deref() != Some(checkpoint.as_str()) {
        return Err(BackendError::new(
            "CHAT_ROLLBACK_NOT_CURRENT",
            "This convenience edit can only be rolled back while its checkpoint is the current Git HEAD.",
            true,
            true,
        )
        .with_details(serde_json::json!({
            "checkpoint": checkpoint,
            "currentHead": current_head,
        })));
    }
    let rollback_result =
        ensure_convenience_paths_unchanged(state, context, &affected_paths, &affected_path_hashes)
            .and_then(|()| {
                state.git_service.rollback_paths_to_head_preserving_ignored(
                    context,
                    &affected_paths,
                    &ignored_baseline,
                )
            });
    match rollback_result {
        Ok(()) => {
            if let Some(edit) = session.messages[index].convenience_edit.as_mut() {
                edit.status = ChatConvenienceEditStatus::RolledBack;
            }
            Ok(())
        }
        Err(error) => {
            let message = error.message.clone();
            if let Some(edit) = session.messages[index].convenience_edit.as_mut() {
                edit.status = ChatConvenienceEditStatus::RollbackFailed;
                edit.violation_reason = Some(message);
            }
            // Preserve the failure state so the UI does not continue to show
            // a rollbackable edit after the filesystem operation failed.
            state.chat_service.save_session(context, session)?;
            Err(error)
        }
    }
}

fn ensure_convenience_paths_unchanged(
    state: &AppState,
    context: &ProjectContext,
    affected_paths: &[String],
    snapshots: &[ChatAffectedPathHash],
) -> Result<(), BackendError> {
    if affected_paths.is_empty() {
        return Ok(());
    }
    if snapshots.len() != affected_paths.len() {
        return Err(BackendError::new(
            "CHAT_ROLLBACK_NOT_CURRENT",
            "This convenience edit has no complete file snapshot; review the affected files before rolling back.",
            true,
            true,
        )
        .with_details(serde_json::json!({ "affectedPaths": affected_paths })));
    }
    for snapshot in snapshots {
        let current = state
            .chat_service
            .file_hash_if_exists(context, &snapshot.path)?;
        if current != snapshot.hash {
            return Err(BackendError::new(
                "CHAT_ROLLBACK_NOT_CURRENT",
                "A convenience-edit file changed after the Agent finished; rollback was not applied.",
                true,
                true,
            )
            .with_details(serde_json::json!({
                "path": snapshot.path,
                "expectedHash": snapshot.hash,
                "currentHash": current,
            })));
        }
    }
    Ok(())
}

fn convenience_edit_missing() -> BackendError {
    BackendError::new(
        "CHAT_CONVENIENCE_EDIT_MISSING",
        "The selected chat message has no convenience edit metadata.",
        true,
        true,
    )
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

fn validate_chat_content(content: &str) -> Result<String, BackendError> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(BackendError::new(
            "CHAT_CONTENT_EMPTY",
            "Chat message cannot be empty.",
            true,
            true,
        ));
    }
    let length = trimmed.chars().count();
    if length > MAX_CHAT_CONTENT_CHARS {
        return Err(BackendError::new(
            "CHAT_CONTENT_TOO_LONG",
            format!("Chat message exceeds the {MAX_CHAT_CONTENT_CHARS}-character limit."),
            true,
            true,
        )
        .with_details(serde_json::json!({
            "maxChars": MAX_CHAT_CONTENT_CHARS,
            "actualChars": length,
        })));
    }
    Ok(trimmed.to_string())
}

fn truncate_title(content: &str) -> String {
    content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .chars()
        .take(60)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::git::{GitChangedFile, GitChangedFileKind};
    use crate::models::llm::LlmProviderConfig;

    fn byok(provider: LlmProviderKind) -> LlmProviderConfig {
        LlmProviderConfig {
            provider,
            model: "m".into(),
            base_url: "https://example.test".into(),
            context_window: 8_000,
            enabled: true,
        }
    }

    #[test]
    fn truncate_title_is_first_line_bounded() {
        assert_eq!(truncate_title("hello\nworld"), "hello");
        let long = "x".repeat(200);
        let t = truncate_title(&long);
        assert!(t.chars().count() <= 60);
    }

    #[test]
    fn validate_chat_content_trims_and_rejects_empty_or_oversized_input() {
        assert_eq!(validate_chat_content("  hello\n").unwrap(), "hello");
        assert_eq!(
            validate_chat_content("  \n\t").unwrap_err().code,
            "CHAT_CONTENT_EMPTY"
        );
        let err = validate_chat_content(&"x".repeat(MAX_CHAT_CONTENT_CHARS + 1)).unwrap_err();
        assert_eq!(err.code, "CHAT_CONTENT_TOO_LONG");
    }

    #[test]
    fn decide_route_agent_preference_picks_agent_when_usable() {
        // Explicit Agent preference ignores BYOK availability.
        let resolved = decide_route(
            CompileRoutePreference::Agent,
            Some(AgentKind::Claude),
            Some(byok(LlmProviderKind::OpenAi)),
        )
        .unwrap();
        assert!(matches!(resolved, ResolvedRoute::Agent(AgentKind::Claude)));
    }

    #[test]
    fn decide_route_agent_preference_errors_without_usable_agent() {
        let err = decide_route(
            CompileRoutePreference::Agent,
            None,
            Some(byok(LlmProviderKind::OpenAi)),
        )
        .unwrap_err();
        assert_eq!(err.code, "AGENT_UNAVAILABLE");
    }

    #[test]
    fn decide_route_byok_preference_ignores_agent() {
        // Explicit BYOK preference must not route to Agent even if one is usable.
        let resolved = decide_route(
            CompileRoutePreference::Byok,
            Some(AgentKind::Claude),
            Some(byok(LlmProviderKind::Anthropic)),
        )
        .unwrap();
        match resolved {
            ResolvedRoute::Byok(config) => assert_eq!(config.provider, LlmProviderKind::Anthropic),
            other => panic!("expected BYOK, got {other:?}"),
        }
    }

    #[test]
    fn decide_route_byok_preference_errors_without_provider() {
        let err =
            decide_route(CompileRoutePreference::Byok, Some(AgentKind::Claude), None).unwrap_err();
        assert_eq!(err.code, "LLM_PROVIDER_MISSING");
    }

    #[test]
    fn decide_route_auto_prefers_agent_when_usable() {
        let resolved = decide_route(
            CompileRoutePreference::Auto,
            Some(AgentKind::Codex),
            Some(byok(LlmProviderKind::OpenAi)),
        )
        .unwrap();
        assert!(matches!(resolved, ResolvedRoute::Agent(AgentKind::Codex)));
    }

    #[test]
    fn decide_route_auto_falls_back_to_byok_without_agent() {
        let resolved = decide_route(
            CompileRoutePreference::Auto,
            None,
            Some(byok(LlmProviderKind::Ollama)),
        )
        .unwrap();
        match resolved {
            ResolvedRoute::Byok(config) => assert_eq!(config.provider, LlmProviderKind::Ollama),
            other => panic!("expected BYOK fallback, got {other:?}"),
        }
    }

    #[test]
    fn should_use_convenience_flow_only_when_enabled_and_write_intent() {
        assert!(!should_use_convenience_flow(false, ChatIntent::Write));
        assert!(!should_use_convenience_flow(true, ChatIntent::ReadOnly));
        assert!(!should_use_convenience_flow(true, ChatIntent::Ambiguous));
        assert!(should_use_convenience_flow(true, ChatIntent::Write));
    }

    #[test]
    fn current_task_runtime_filter_is_scoped_to_current_task_files() {
        let changed = |path: &str| GitChangedFile {
            path: path.to_string(),
            kind: GitChangedFileKind::Modified,
            changed_chars: 1,
        };
        assert!(is_current_task_runtime_path(
            "task-1",
            &changed(".app/tasks/task-1.json")
        ));
        assert!(is_current_task_runtime_path(
            "task-1",
            &changed(".app/tasks/task-1.log")
        ));
        assert!(!is_current_task_runtime_path(
            "task-1",
            &changed(".app/tasks/task-2.json")
        ));
        assert!(!is_current_task_runtime_path(
            "task-1",
            &changed(".app/settings.json")
        ));
    }

    #[test]
    fn current_task_diff_filter_removes_only_current_task_block() {
        let diff = "diff --git a/.app/tasks/task-1.json b/.app/tasks/task-1.json\n+runtime\n\
diff --git a/wiki/page.md b/wiki/page.md\n+content\n\
diff --git a/.app/tasks/task-2.json b/.app/tasks/task-2.json\n+other\n";

        let filtered = filter_current_task_diff("task-1", diff);

        assert!(!filtered.contains("task-1.json"));
        assert!(filtered.contains("wiki/page.md"));
        assert!(filtered.contains("task-2.json"));
    }
}
