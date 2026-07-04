use serde::{Deserialize, Serialize};

use super::agent::AgentKind;
use super::compile::CompileRoutePreference;
use super::llm::LlmProviderKind;

/// Who authored a chat message.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
}

/// Which execution backend produced an assistant answer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatRoute {
    Agent,
    Byok,
}

/// A retrieved context page attached to an assistant message. Citations are the
/// pages actually fed to the model (the honest source-of-truth), not parsed out
/// of model output, so they never drift from what the model could see.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatCitation {
    pub page_path: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    pub score: i64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: ChatRole,
    pub content: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<ChatCitation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<ChatRoute>,
    /// Which BYOK provider answered (only set for the BYOK route). Lets the UI
    /// render a model badge ("BYOK · Anthropic") without re-deriving it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<LlmProviderKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

/// Persisted chat session at `.app/chats/{id}.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub project_id: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
}

/// One retrieved wiki page with a bounded body excerpt for the model prompt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatRetrievalHit {
    pub path: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    pub score: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub is_pinned: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChatSessionRequest {
    pub project_id: String,
    pub project_root_path: String,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListChatsRequest {
    pub project_id: String,
    pub project_root_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameChatRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteChatRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadChatRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendChatMessageRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub content: String,
    #[serde(default = "default_route")]
    pub route: CompileRoutePreference,
    #[serde(default)]
    pub agent: Option<AgentKind>,
    #[serde(default)]
    pub provider: Option<LlmProviderKind>,
    #[serde(default)]
    pub pinned_page_path: Option<String>,
}

fn default_route() -> CompileRoutePreference {
    CompileRoutePreference::Auto
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAnswerToWikiRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub message_id: String,
    #[serde(default)]
    pub target_path: Option<String>,
    #[serde(default)]
    pub expected_hash: Option<String>,
    #[serde(default)]
    pub allow_overwrite: bool,
    #[serde(default)]
    pub action_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SaveAnswerResult {
    pub path: String,
    pub created: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_citation_and_message_camel_case() {
        let citation = ChatCitation {
            page_path: "wiki/concepts/a.md".into(),
            title: "A".into(),
            snippet: Some("snip".into()),
            score: 100,
            is_pinned: false,
        };
        let value = serde_json::to_value(&citation).unwrap();
        assert_eq!(value["pagePath"], json!("wiki/concepts/a.md"));
        assert_eq!(value["score"], json!(100));
        assert!(value.get("isPinned").is_none());

        let message = ChatMessage {
            id: "m1".into(),
            role: ChatRole::Assistant,
            content: "answer".into(),
            created_at: "2026-06-20T00:00:00Z".into(),
            citations: vec![citation],
            route: Some(ChatRoute::Byok),
            provider: None,
            task_id: Some("task-1".into()),
        };
        let value = serde_json::to_value(&message).unwrap();
        assert_eq!(value["role"], json!("assistant"));
        assert_eq!(value["route"], json!("byok"));
        assert_eq!(value["taskId"], json!("task-1"));
        assert_eq!(value["createdAt"], json!("2026-06-20T00:00:00Z"));
        // citations round-trip nested camelCase
        assert_eq!(
            value["citations"][0]["pagePath"],
            json!("wiki/concepts/a.md")
        );
    }

    #[test]
    fn omits_empty_citations_and_sparse_fields_on_serialize() {
        let message = ChatMessage {
            id: "m1".into(),
            role: ChatRole::User,
            content: "q".into(),
            created_at: "2026-06-20T00:00:00Z".into(),
            citations: Vec::new(),
            route: None,
            provider: None,
            task_id: None,
        };
        let value = serde_json::to_value(&message).unwrap();
        assert!(value.get("citations").is_none() || value["citations"].is_null());
        assert!(value.get("route").is_none());
        assert!(value.get("taskId").is_none());
    }

    #[test]
    fn send_request_defaults_route_to_auto() {
        let raw = r#"{"projectId":"p","projectRootPath":"/x","sessionId":"s","content":"hi"}"#;
        let request: SendChatMessageRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(request.route, CompileRoutePreference::Auto);
        assert!(request.agent.is_none());
        assert!(request.provider.is_none());
        assert!(request.pinned_page_path.is_none());
    }

    #[test]
    fn send_request_defaults_pinned_page_path_to_none() {
        let raw = r#"{"projectId":"p","projectRootPath":"/x","sessionId":"s","content":"hi","route":"auto"}"#;
        let request: SendChatMessageRequest = serde_json::from_str(raw).unwrap();
        assert!(request.pinned_page_path.is_none());
    }

    #[test]
    fn citation_serializes_is_pinned_when_true() {
        let citation = ChatCitation {
            page_path: "wiki/concepts/a.md".into(),
            title: "A".into(),
            snippet: None,
            score: 10_000,
            is_pinned: true,
        };

        let value = serde_json::to_value(&citation).unwrap();

        assert_eq!(value["isPinned"], json!(true));
    }
}
