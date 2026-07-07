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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatConvenienceEditStatus {
    Applied,
    SoftViolationPending,
    KeptAfterSoftViolation,
    RolledBack,
    RollbackFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatConvenienceEdit {
    pub status: ChatConvenienceEditStatus,
    pub checkpoint_hash: Option<String>,
    pub affected_paths: Vec<String>,
    pub diff_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub violation_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_baseline_paths: Vec<String>,
}

/// A numbered source supplied to the model prompt. These are retrieval/planner
/// inputs, not persisted citations unless the model actually cites them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSourceRef {
    pub id: String,
    pub page_path: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    pub score: i64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_pinned: bool,
}

/// A citation the model actually used, parsed from `[S#]` markers in the final
/// answer. Retrieval hits that were merely available live in diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatCitation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub page_path: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    pub score: i64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_pinned: bool,
}

/// Retrieval diagnostics are for transparency/debugging only. They are not the
/// answer's evidence list; persisted citations come solely from parsed markers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatRetrievalDiagnostics {
    pub route: ChatRoute,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retrieval_hits: Vec<ChatRetrievalHit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expanded_pages: Vec<ChatExpandedPage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_pages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted_pages: Vec<String>,
    pub budget_chars: usize,
    pub source_budget_chars: usize,
    pub history_budget_chars: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invalid_citation_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_unverified: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChatSourceSelectionReason {
    Index,
    Pinned,
    KeywordHit,
    GraphNeighbor,
    SourceOverlap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChatExpandedPage {
    pub path: String,
    pub reason: ChatSourceSelectionReason,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convenience_edit: Option<ChatConvenienceEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_diagnostics: Option<ChatRetrievalDiagnostics>,
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
    /// Wiki page this session is scoped to (Wiki "Ask AI" sidebar). Absent for
    /// global Chat-view sessions. Persisted as typed metadata on the session
    /// JSON — no database. Defaults to None for older session files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_page_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatSessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    /// Mirrors [`ChatSession::context_page_path`] so the session list can group
    /// page-scoped chats without loading each full session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_page_path: Option<String>,
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
    /// Optional page path to scope this session to a wiki page (Wiki AI sidebar).
    #[serde(default)]
    pub context_page_path: Option<String>,
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
    #[serde(default)]
    pub convenience_enabled: bool,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveChatConvenienceEditRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
    pub message_id: String,
    pub keep: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RollbackLastChatConvenienceEditRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub session_id: String,
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
            source_id: Some("S1".into()),
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
            convenience_edit: None,
            retrieval_diagnostics: None,
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
            convenience_edit: None,
            retrieval_diagnostics: None,
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
        assert!(!request.convenience_enabled);
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
            source_id: Some("S1".into()),
            page_path: "wiki/concepts/a.md".into(),
            title: "A".into(),
            snippet: None,
            score: 10_000,
            is_pinned: true,
        };

        let value = serde_json::to_value(&citation).unwrap();

        assert_eq!(value["isPinned"], json!(true));
        assert_eq!(value["sourceId"], json!("S1"));
    }

    #[test]
    fn old_chat_message_json_defaults_new_citation_and_diagnostic_fields() {
        let raw = r#"{
            "id":"m1",
            "role":"assistant",
            "content":"old answer",
            "createdAt":"2026-07-07T00:00:00Z",
            "citations":[{"pagePath":"wiki/a.md","title":"A","score":1}]
        }"#;

        let message: ChatMessage = serde_json::from_str(raw).unwrap();

        assert_eq!(message.citations[0].source_id, None);
        assert!(message.retrieval_diagnostics.is_none());
    }

    #[test]
    fn retrieval_diagnostics_serializes_retrieval_hits_separately_from_citations() {
        let message = ChatMessage {
            id: "m1".into(),
            role: ChatRole::Assistant,
            content: "answer [S1]".into(),
            created_at: "2026-07-07T00:00:00Z".into(),
            citations: vec![ChatCitation {
                source_id: Some("S1".into()),
                page_path: "wiki/cited.md".into(),
                title: "Cited".into(),
                snippet: Some("used".into()),
                score: 10,
                is_pinned: false,
            }],
            route: Some(ChatRoute::Byok),
            provider: None,
            task_id: None,
            convenience_edit: None,
            retrieval_diagnostics: Some(ChatRetrievalDiagnostics {
                route: ChatRoute::Byok,
                retrieval_hits: vec![ChatRetrievalHit {
                    path: "wiki/not-cited.md".into(),
                    title: "Not Cited".into(),
                    snippet: Some("retrieved".into()),
                    score: 5,
                    excerpt: Some("retrieved excerpt".into()),
                    is_pinned: false,
                }],
                expanded_pages: vec![ChatExpandedPage {
                    path: "wiki/expanded.md".into(),
                    reason: ChatSourceSelectionReason::GraphNeighbor,
                }],
                selected_pages: vec!["wiki/cited.md".into()],
                omitted_pages: vec!["wiki/omitted.md".into()],
                budget_chars: 24_000,
                source_budget_chars: 14_400,
                history_budget_chars: 6_000,
                invalid_citation_ids: vec!["S9".into()],
                has_unverified: true,
            }),
        };

        let value = serde_json::to_value(&message).unwrap();

        assert_eq!(
            value["retrievalDiagnostics"]["retrievalHits"][0]["path"],
            json!("wiki/not-cited.md")
        );
        assert_eq!(
            value["retrievalDiagnostics"]["expandedPages"][0]["reason"],
            json!("graph_neighbor")
        );
        assert_eq!(value["citations"][0]["pagePath"], json!("wiki/cited.md"));
    }

    #[test]
    fn message_serializes_convenience_edit_camel_case() {
        let message = ChatMessage {
            id: "m1".into(),
            role: ChatRole::Assistant,
            content: "updated".into(),
            created_at: "2026-06-20T00:00:00Z".into(),
            citations: Vec::new(),
            route: None,
            provider: None,
            task_id: None,
            convenience_edit: Some(ChatConvenienceEdit {
                status: ChatConvenienceEditStatus::SoftViolationPending,
                checkpoint_hash: Some("abc123".into()),
                affected_paths: vec!["wiki/a.md".into()],
                diff_summary: "1 wiki page changed".into(),
                diff_text: Some("+summary".into()),
                violation_reason: Some("too many files".into()),
                rollback_task_id: None,
                ignored_baseline_paths: vec!["keep.log".into()],
            }),
            retrieval_diagnostics: None,
        };

        let value = serde_json::to_value(&message).unwrap();

        assert_eq!(
            value["convenienceEdit"]["status"],
            json!("soft_violation_pending")
        );
        assert_eq!(value["convenienceEdit"]["checkpointHash"], json!("abc123"));
        assert_eq!(
            value["convenienceEdit"]["affectedPaths"][0],
            json!("wiki/a.md")
        );
        assert_eq!(value["convenienceEdit"]["diffText"], json!("+summary"));
        assert_eq!(
            value["convenienceEdit"]["ignoredBaselinePaths"][0],
            json!("keep.log")
        );
        assert!(value["convenienceEdit"].get("rollbackTaskId").is_none());
    }

    #[test]
    fn send_request_defaults_convenience_enabled_to_false() {
        let raw = r#"{"projectId":"p","projectRootPath":"/x","sessionId":"s","content":"hi"}"#;
        let request: SendChatMessageRequest = serde_json::from_str(raw).unwrap();
        assert!(!request.convenience_enabled);

        let raw = r#"{"projectId":"p","projectRootPath":"/x","sessionId":"s","content":"hi","convenienceEnabled":true}"#;
        let request: SendChatMessageRequest = serde_json::from_str(raw).unwrap();
        assert!(request.convenience_enabled);
    }

    #[test]
    fn session_defaults_context_page_path_for_existing_json() {
        // Older session files written before page-scoped chat existed must
        // still deserialize: context_page_path defaults to None.
        let raw = r#"{
            "id":"s1",
            "title":"Old",
            "projectId":"p",
            "createdAt":"2026-07-07T00:00:00Z",
            "updatedAt":"2026-07-07T00:00:00Z",
            "messages":[]
        }"#;
        let session: ChatSession = serde_json::from_str(raw).unwrap();
        assert!(session.context_page_path.is_none());
    }

    #[test]
    fn session_round_trips_context_page_path_camel_case() {
        let raw = r#"{
            "id":"s1",
            "title":"Page Chat",
            "projectId":"p",
            "createdAt":"2026-07-07T00:00:00Z",
            "updatedAt":"2026-07-07T00:00:00Z",
            "messages":[],
            "contextPagePath":"wiki/concepts/react-pattern.md"
        }"#;
        let session: ChatSession = serde_json::from_str(raw).unwrap();
        assert_eq!(
            session.context_page_path.as_deref(),
            Some("wiki/concepts/react-pattern.md")
        );
        // Serializes back to camelCase and omits the field when absent.
        let value = serde_json::to_value(&ChatSession {
            id: "s2".into(),
            title: "No page".into(),
            project_id: "p".into(),
            created_at: "2026-07-07T00:00:00Z".into(),
            updated_at: "2026-07-07T00:00:00Z".into(),
            messages: Vec::new(),
            context_page_path: None,
        })
        .unwrap();
        assert!(value.get("contextPagePath").is_none());
    }

    #[test]
    fn create_session_request_defaults_context_page_path_to_none() {
        let raw = r#"{"projectId":"p","projectRootPath":"/x"}"#;
        let request: CreateChatSessionRequest = serde_json::from_str(raw).unwrap();
        assert!(request.context_page_path.is_none());

        let raw = r#"{"projectId":"p","projectRootPath":"/x","contextPagePath":"wiki/a.md"}"#;
        let request: CreateChatSessionRequest = serde_json::from_str(raw).unwrap();
        assert_eq!(request.context_page_path.as_deref(), Some("wiki/a.md"));
    }
}
