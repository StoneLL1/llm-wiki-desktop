use llm_wiki_desktop_lib::models::import_v2_web::{
    NormalizedWebUrl, WebRecoveryAction, WebRouteKind,
};

#[test]
fn web_contracts_are_public_only_and_stable() {
    let value = serde_json::to_value(NormalizedWebUrl {
        public_url: "https://example.com/article?id=7".into(),
        host: "example.com".into(),
        scheme: "https".into(),
    })
    .unwrap();
    assert_eq!(value["publicUrl"], "https://example.com/article?id=7");
    assert_eq!(
        serde_json::to_value(WebRouteKind::Xiaohongshu).unwrap(),
        "xiaohongshu"
    );
    assert_eq!(
        serde_json::to_value(WebRecoveryAction::BeginLogin).unwrap(),
        "begin_login"
    );
    let text = value.to_string().to_ascii_lowercase();
    assert!(!text.contains("token") && !text.contains("fragment"));
}

#[test]
fn authenticated_login_resumption_uses_the_unbounded_operation_launcher() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands/import_v2_web_commands.rs"
    ))
    .unwrap();
    let completion = source
        .split("pub fn complete_import_login_v2")
        .nth(1)
        .expect("login completion command must remain explicit")
        .split("pub async fn authorize_import_private_target_v2")
        .next()
        .unwrap();

    assert!(completion.contains("start_import_batch_for_state"));
    assert!(!completion.contains("start_import_items_for_state"));
}

#[test]
fn batch_workers_finalize_exact_duplicates_before_aggregating_outcomes() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands/import_v2_commands.rs"
    ))
    .unwrap();
    let worker = source
        .split("fn run_import_worker_job")
        .nth(1)
        .expect("import worker must remain a named orchestration boundary")
        .split("fn classify_batch_item_outcome")
        .next()
        .unwrap();
    let finalize = worker
        .find("finalize_exact_duplicate")
        .expect("workers must run exact-duplicate finalization");
    let aggregate = worker
        .find("if let Some(_) = job.batch_operation")
        .expect("batch workers must aggregate one terminal outcome");

    assert!(finalize < aggregate);
}

#[test]
fn generic_task_cancellation_defers_import_operation_cleanup_to_workers() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/commands/task_commands.rs"
    ))
    .unwrap();
    let cancellation = source
        .split("pub fn cancel_task")
        .nth(1)
        .expect("generic cancellation command must remain explicit")
        .split("pub fn get_task_logs")
        .next()
        .unwrap();

    assert!(cancellation.contains("is_import_batch_operation_task"));
    assert!(cancellation.contains("request_cancel"));
}
