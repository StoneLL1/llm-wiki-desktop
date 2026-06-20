use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::chat::{
    ChatMessage, ChatRoute, ChatSession, ChatSessionSummary, CreateChatSessionRequest,
    DeleteChatRequest, ListChatsRequest, LoadChatRequest, RenameChatRequest, SaveAnswerResult,
    SaveAnswerToWikiRequest, SendChatMessageRequest,
};
use crate::models::compile::CompileRoutePreference;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskStatus, TaskType};
use crate::services::{AgentService, LlmService};
use crate::tasks::task_model::LogLevel;

fn context_for(project_id: &str, root_path: &str) -> ProjectContext {
    ProjectContext::new(project_id, PathBuf::from(root_path))
}

#[tauri::command]
pub fn create_chat_session(
    state: State<'_, AppState>,
    request: CreateChatSessionRequest,
) -> Result<ChatSession, BackendError> {
    let context = context_for(&request.project_id, &request.project_root_path);
    state
        .chat_service
        .create_session(&context, request.title.as_deref())
}

#[tauri::command]
pub fn list_chat_sessions(
    state: State<'_, AppState>,
    request: ListChatsRequest,
) -> Result<Vec<ChatSessionSummary>, BackendError> {
    let context = context_for(&request.project_id, &request.project_root_path);
    state.chat_service.list_sessions(&context)
}

#[tauri::command]
pub fn load_chat_session(
    state: State<'_, AppState>,
    request: LoadChatRequest,
) -> Result<ChatSession, BackendError> {
    let context = context_for(&request.project_id, &request.project_root_path);
    state
        .chat_service
        .load_session(&context, &request.session_id)
}

#[tauri::command]
pub fn rename_chat_session(
    state: State<'_, AppState>,
    request: RenameChatRequest,
) -> Result<ChatSession, BackendError> {
    let context = context_for(&request.project_id, &request.project_root_path);
    state
        .chat_service
        .rename_session(&context, &request.session_id, &request.title)
}

#[tauri::command]
pub fn delete_chat_session(
    state: State<'_, AppState>,
    request: DeleteChatRequest,
) -> Result<(), BackendError> {
    let context = context_for(&request.project_id, &request.project_root_path);
    state
        .chat_service
        .delete_session(&context, &request.session_id)
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
    let task = state.task_service.create_task(
        TaskType::LlmRequest,
        Some(request.project_id.clone()),
        format!("Chat: {}", truncate_title(&request.content)),
        true,
    );
    let task_id = task.id.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = run_chat_send(&state, request, &task_id).await {
            let _ = state
                .task_service
                .append_log(&task_id, LogLevel::Error, error.message.clone());
            let _ = state.task_service.set_error(&task_id, error);
            if !matches!(
                state.task_service.get_task(&task_id).map(|t| t.status),
                Some(TaskStatus::Cancelled)
            ) {
                let _ = state
                    .task_service
                    .transition_status(&task_id, TaskStatus::Failed);
            }
        }
    });
    Ok(task)
}

async fn run_chat_send(
    state: &AppState,
    request: SendChatMessageRequest,
    task_id: &str,
) -> Result<(), BackendError> {
    state
        .task_service
        .transition_status(task_id, TaskStatus::Running)
        .map_err(task_error)?;
    let context = context_for(&request.project_id, &request.project_root_path);

    let mut session = state
        .chat_service
        .load_session(&context, &request.session_id)?;
    let user_message = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: crate::models::chat::ChatRole::User,
        content: request.content.clone(),
        created_at: crate::utils::time_utils::now_rfc3339(),
        citations: Vec::new(),
        route: None,
        task_id: None,
    };
    state
        .chat_service
        .append_message(&context, &mut session, user_message)?;

    state
        .task_service
        .append_log(task_id, LogLevel::Info, "Retrieving local context".into())
        .map_err(task_error)?;
    let retrieval = state.chat_service.build_retrieval_context(
        &context,
        &state.search_service,
        &request.content,
        &session,
    )?;
    let citations = retrieval.citations.clone();

    let (route, answer) = match resolve_route(
        state,
        &context,
        request.route,
        request.agent,
        request.provider,
    )? {
        ResolvedRoute::Agent(kind) => {
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!("Running {}", kind.command()),
                )
                .map_err(task_error)?;
            let workspace = create_chat_workspace(task_id)?;
            let invocation = AgentService::chat_invocation(kind, &workspace, &retrieval.prompt)?;
            let captured = state.agent_service.run_task_streaming(
                &invocation,
                &state.task_service,
                task_id,
            )?;
            let _ = std::fs::remove_dir_all(&workspace);
            (ChatRoute::Agent, captured.trim().to_string())
        }
        ResolvedRoute::Byok(provider) => {
            state
                .task_service
                .append_log(
                    task_id,
                    LogLevel::Info,
                    format!("Calling {:?}", provider.provider),
                )
                .map_err(task_error)?;
            let secret = state.secret_service.get(provider.provider)?;
            let completion =
                state
                    .llm_service
                    .complete(&provider, secret.as_deref(), &retrieval.prompt);
            tokio::pin!(completion);
            let raw = loop {
                tokio::select! {
                    result = &mut completion => break result?,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                        if state.task_service.is_cancelled(task_id) {
                            return Err(BackendError::new(
                                "CHAT_CANCELLED",
                                "Chat was cancelled.",
                                true,
                                false,
                            ));
                        }
                    }
                }
            };
            (ChatRoute::Byok, raw.trim().to_string())
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

    let assistant_message = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: crate::models::chat::ChatRole::Assistant,
        content: answer,
        created_at: crate::utils::time_utils::now_rfc3339(),
        citations,
        route: Some(route),
        task_id: Some(task_id.to_string()),
    };
    state
        .chat_service
        .append_message(&context, &mut session, assistant_message)?;

    state
        .task_service
        .set_result(
            task_id,
            TaskResult {
                summary: "Chat answer ready.".into(),
                affected_paths: vec![format!(".app/chats/{}.json", session.id)],
                pending_action: None,
            },
        )
        .map_err(task_error)?;
    state
        .task_service
        .transition_status(task_id, TaskStatus::Succeeded)
        .map_err(task_error)?;
    Ok(())
}

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
        state
            .agent_service
            .detect_agents(Some(*kind))
            .iter()
            .any(|info| info.kind == *kind && info.state == AgentDetectionState::Installed)
    });
    let selected_provider = select_provider(explicit_provider, &providers, &state.secret_service)?;
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
        if provider.provider.requires_secret() && secrets.get(provider.provider)?.is_none() {
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
        if !provider.provider.requires_secret() || secrets.get(provider.provider)?.is_some() {
            return Ok(Some(provider.clone()));
        }
    }
    Ok(None)
}

/// Create an empty candidate-scoped workspace directory for the Agent run. The
/// assembled prompt already carries every excerpt inline, so the agent never
/// needs to touch project files; this dir only satisfies the candidate-root
/// guard and gives the CLI a stable cwd.
fn create_chat_workspace(task_id: &str) -> Result<PathBuf, BackendError> {
    let workspace = std::env::temp_dir()
        .join("llm-wiki-desktop")
        .join(format!("chat-{task_id}"));
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace).map_err(|err| {
            BackendError::new("CHAT_WORKSPACE_FAILED", err.to_string(), true, false)
        })?;
    }
    std::fs::create_dir_all(&workspace)
        .map_err(|err| BackendError::new("CHAT_WORKSPACE_FAILED", err.to_string(), true, false))?;
    Ok(workspace)
}

/// Save an assistant answer to `wiki/queries/` as a Markdown page.
#[tauri::command]
pub fn save_answer_to_wiki(
    state: State<'_, AppState>,
    request: SaveAnswerToWikiRequest,
) -> Result<SaveAnswerResult, BackendError> {
    let context = context_for(&request.project_id, &request.project_root_path);
    let session = state
        .chat_service
        .load_session(&context, &request.session_id)?;
    let preceding: Vec<&ChatMessage> = session
        .messages
        .iter()
        .take_while(|m| m.id != request.message_id)
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
        .find(|m| m.id == request.message_id)
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
    state.chat_service.save_answer_to_wiki(
        &context,
        &state.git_service,
        request.target_path.as_deref(),
        request.expected_hash.as_deref(),
        request.allow_overwrite,
        &markdown,
        &slug,
    )
}

fn task_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
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

    #[test]
    fn truncate_title_is_first_line_bounded() {
        assert_eq!(truncate_title("hello\nworld"), "hello");
        let long = "x".repeat(200);
        let t = truncate_title(&long);
        assert!(t.chars().count() <= 60);
    }
}
