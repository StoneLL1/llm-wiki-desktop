use std::sync::{Arc, Mutex};

use llm_wiki_desktop_lib::{
    errors::BackendError,
    models::import_v2_agent::AgentToolGrant,
    services::import_v2::agent_tools::{
        ImportAgentToolBroker, ImportAgentToolCall, ImportAgentToolExecutor, ImportAgentToolResult,
        ImportAgentToolTaskContext,
    },
};

#[derive(Default)]
struct FakeExecutor {
    calls: Mutex<Vec<ImportAgentToolCall>>,
}

impl ImportAgentToolExecutor for FakeExecutor {
    fn execute(
        &self,
        _context: &ImportAgentToolTaskContext,
        call: &ImportAgentToolCall,
    ) -> Result<ImportAgentToolResult, BackendError> {
        self.calls.lock().unwrap().push(call.clone());
        Ok(ImportAgentToolResult {
            outcome: "succeeded".into(),
            output_hashes: vec!["output-hash".into()],
            warnings: vec![],
            resource_units: 1,
        })
    }
}

fn context(root: &std::path::Path, grants: Vec<AgentToolGrant>) -> ImportAgentToolTaskContext {
    let item_staging_root = root.join(".app/import-sessions/session-a/items/item-a/staging");
    let workspace = item_staging_root.join("agent/workspace-a");
    std::fs::create_dir_all(workspace.join("output")).unwrap();
    ImportAgentToolTaskContext {
        task_id: "task-a".into(),
        project_id: "project-a".into(),
        session_id: "session-a".into(),
        item_id: "item-a".into(),
        item_staging_root,
        workspace_root: workspace,
        grants,
        input_hashes: vec!["input-hash".into()],
        cancelled: false,
    }
}

#[test]
fn broker_allows_named_parser_and_persists_redacted_ledger() {
    let root = tempfile::tempdir().unwrap();
    let executor = Arc::new(FakeExecutor::default());
    let broker = ImportAgentToolBroker::new(executor.clone());
    let context = context(root.path(), vec![AgentToolGrant::RunDeterministicRoute]);

    let result = broker
        .invoke(
            &context,
            ImportAgentToolCall::RunDeterministicRoute {
                route: "file.docling".into(),
            },
        )
        .unwrap();
    assert_eq!(result.outcome, "succeeded");
    assert_eq!(executor.calls.lock().unwrap().len(), 1);

    let ledger =
        std::fs::read_to_string(context.workspace_root.join("logs/tool-ledger.json")).unwrap();
    assert!(ledger.contains("run_deterministic_route"));
    assert!(ledger.contains("file.docling"));
    for forbidden in ["password", "cookie", "authorization", "secret"] {
        assert!(!ledger.to_ascii_lowercase().contains(forbidden));
    }
}

#[test]
fn broker_denies_ungranted_injected_and_cross_item_requests() {
    let root = tempfile::tempdir().unwrap();
    let executor = Arc::new(FakeExecutor::default());
    let broker = ImportAgentToolBroker::new(executor.clone());

    let cases = [
        ImportAgentToolCall::RunDeterministicRoute {
            route: "C:\\tools\\evil.exe --steal".into(),
        },
        ImportAgentToolCall::RunDeterministicRoute {
            route: "https://evil.example/payload".into(),
        },
        ImportAgentToolCall::RunDeterministicRoute {
            route: "git.status".into(),
        },
        ImportAgentToolCall::ValidateCandidate {
            relative_markdown_path: "../item-b/output.md".into(),
        },
        ImportAgentToolCall::ValidateCandidate {
            relative_markdown_path: "output/payload.exe".into(),
        },
    ];
    for call in cases {
        let error = broker
            .invoke(
                &context(
                    root.path(),
                    vec![
                        AgentToolGrant::RunDeterministicRoute,
                        AgentToolGrant::ValidateCandidate,
                    ],
                ),
                call,
            )
            .unwrap_err();
        assert!(error.code.starts_with("IMPORT_AGENT_TOOL_"));
    }

    let mut wrong_item = context(root.path(), vec![AgentToolGrant::InspectSource]);
    wrong_item.item_id = "item-b".into();
    assert_eq!(
        broker
            .invoke(&wrong_item, ImportAgentToolCall::InspectSource)
            .unwrap_err()
            .code,
        "IMPORT_AGENT_TOOL_SCOPE_DENIED"
    );
    let no_grant = context(root.path(), vec![]);
    assert_eq!(
        broker
            .invoke(&no_grant, ImportAgentToolCall::InspectSource)
            .unwrap_err()
            .code,
        "IMPORT_AGENT_TOOL_NOT_GRANTED"
    );
    let mut cancelled = context(root.path(), vec![AgentToolGrant::InspectSource]);
    cancelled.cancelled = true;
    assert_eq!(
        broker
            .invoke(&cancelled, ImportAgentToolCall::InspectSource)
            .unwrap_err()
            .code,
        "IMPORT_AGENT_TOOL_CANCELLED"
    );
    assert!(executor.calls.lock().unwrap().is_empty());
}

#[test]
fn tool_protocol_has_no_arbitrary_authority_variants() {
    let source = include_str!("../src/services/import_v2/agent_tools.rs");
    for forbidden in [
        "RunShell",
        "InstallPlugin",
        "ReadCredential",
        "GitCommand",
        "FetchUrl",
    ] {
        assert!(!source.contains(forbidden));
    }
}
