//! Opt-in release fixtures, not WebView frame or IPC latency measurements.
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::task::TaskStatus;
use llm_wiki_desktop_lib::models::workflow::{
    WorkflowExecutionState, WorkflowFilesystemAccess, WorkflowGitState, WorkflowPersistenceMode,
    WorkflowProjectTrust,
};
use llm_wiki_desktop_lib::services::{
    project_identity, AgentService, SecretService, SettingsService, WorkflowAccessSnapshot,
    WorkflowService,
};
use llm_wiki_desktop_lib::tasks::TaskService;
use std::time::Instant;

fn state(owner: &str, revision: &str, index: usize) -> WorkflowExecutionState {
    serde_json::from_value(serde_json::json!({
        "schemaVersion": 2, "canonicalIdentityKey": owner, "identityRevision": revision,
        "kind": "health_check", "scope": { "kind": "health_check", "mode": "local_quick" },
        "executionOptions": { "preparationRevision": "fixture" },
        "route": { "kind": "local", "routeRevision": "local" },
        "fingerprint": format!("fixture-{index}"), "baselineFingerprint": "fixture",
        "persistence": "memory_only", "stages": [], "currentStageId": null,
        "queuePosition": index + 1, "continuationRequired": false, "retry": null,
        "pendingAction": null, "result": null, "error": null,
        "cancelledFromQueue": false, "undoCancelUntil": null
    }))
    .unwrap()
}

fn report(label: &str, samples: &[f64], response_bytes: usize) {
    let mut ordered = samples.to_vec();
    ordered.sort_by(f64::total_cmp);
    eprintln!(
        "WORKFLOW_SCALE {}",
        serde_json::json!({
            "label": label, "profile": "release", "samples": ordered.len(),
            "p50Ms": ordered[(ordered.len() - 1) / 2],
            "p95Ms": ordered[(ordered.len() as f64 * 0.95).ceil() as usize - 1],
            "maxMs": ordered[ordered.len() - 1], "responseBytes": response_bytes,
            "rawMs": samples
        })
    );
}

#[test]
#[ignore = "opt-in release fixture: 10,000 Markdown pages and cross-owner history"]
fn release_large_content_and_history_keep_overview_bounded() {
    assert!(!cfg!(debug_assertions), "run with cargo test --release");
    let root = tempfile::tempdir().unwrap();
    let wiki = root.path().join("中文知识库/wiki/规模");
    std::fs::create_dir_all(&wiki).unwrap();
    let project_root = root.path().join("中文知识库");
    let body = format!("# 中文页面\n{}", "正文 content ".repeat(128));
    let body = format!("{body}{}", "x".repeat(2048 - body.len()));
    assert_eq!(body.len(), 2048);
    for index in 0..10_000 {
        std::fs::write(wiki.join(format!("页面-{index:05}.md")), &body).unwrap();
    }
    let identity = project_identity(&project_root).unwrap();
    let context = ProjectContext::new("performance-fixture", project_root.clone());
    let tasks = TaskService::default();
    for index in 0..10_000 {
        // Exactly 5,000 current-owner terminal summaries, 5,000 foreign summaries.
        let owner = if index % 2 == 0 {
            &identity.canonical_identity_key
        } else {
            "foreign-owner"
        };
        let run = tasks
            .create_workflow_task(
                "performance-fixture".into(),
                project_root.clone(),
                "历史检查".into(),
                state(owner, &identity.identity_revision, index),
                None,
            )
            .unwrap();
        tasks
            .transition_workflow_status(&run.task_id, TaskStatus::Cancelled)
            .unwrap();
    }
    for index in 0..100 {
        let run = tasks
            .create_workflow_task(
                "performance-fixture".into(),
                project_root.clone(),
                "活动检查".into(),
                state(
                    &identity.canonical_identity_key,
                    &identity.identity_revision,
                    index,
                ),
                None,
            )
            .unwrap();
        if index == 0 {
            tasks
                .transition_workflow_status(&run.task_id, TaskStatus::Running)
                .unwrap();
        }
    }
    let service = WorkflowService::default();
    let settings_root = tempfile::tempdir().unwrap();
    let settings = SettingsService::with_config_dir(settings_root.path().to_path_buf());
    let secrets = SecretService::memory();
    let agents = AgentService::default();
    let access = WorkflowAccessSnapshot {
        trust: WorkflowProjectTrust::Untrusted,
        trust_kind: None,
        filesystem_access: WorkflowFilesystemAccess::Unknown,
        persistence: WorkflowPersistenceMode::MemoryOnly,
        git_state: WorkflowGitState::Unknown,
        authority_revision: "fixture".into(),
    };
    let mut samples = Vec::new();
    let mut bytes = 0;
    for _ in 0..50 {
        let started = Instant::now();
        // Public facade includes identity filesystem metadata; pure-memory reference is separate.
        let overview = service
            .project_overview(
                &context,
                access.clone(),
                &settings,
                &secrets,
                &agents,
                &tasks,
            )
            .unwrap();
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
        bytes = serde_json::to_vec(&overview).unwrap().len();
        assert_eq!(overview.rows.len(), 3);
        assert_eq!(overview.recent_runs.len(), 5);
        assert!(overview.active_runs.len() <= 3);
        assert!(overview
            .recent_runs
            .iter()
            .all(|run| run.canonical_identity_key == identity.canonical_identity_key));
        assert_eq!(
            overview.context_summary.as_ref().unwrap().queued_runs.len(),
            5
        );
        assert_eq!(overview.context_summary.as_ref().unwrap().queue_count, 99);
        assert!(
            bytes < 64 * 1024,
            "overview exceeded the 64 KiB target: {bytes}"
        );
    }
    report("overview_facade_including_identity_stat", &samples, bytes);
    let mut page_samples = Vec::new();
    let mut page_bytes = 0;
    for _ in 0..50 {
        let started = Instant::now();
        let (page, more) = tasks.page_workflow_runs(
            &identity.canonical_identity_key,
            &identity.identity_revision,
            None,
            None,
            None,
            50,
        );
        page_samples.push(started.elapsed().as_secs_f64() * 1000.0);
        page_bytes = serde_json::to_vec(&page).unwrap().len();
        assert_eq!(page.len(), 50);
        assert!(more);
        assert!(page
            .iter()
            .all(|run| run.canonical_identity_key == identity.canonical_identity_key));
    }
    report(
        "owner_history_page_including_first_index_build",
        &page_samples,
        page_bytes,
    );
    eprintln!("WORKFLOW_SCALE_FIXTURE markdown_pages=10000 markdown_bytes=20480000 terminal_history=10000 current_owner_terminal=5000 foreign_terminal=5000 current_active=100 running=1 queued=99 overview_details=0 history_page_summaries=50");
}
