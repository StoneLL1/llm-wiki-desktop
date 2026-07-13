use llm_wiki_desktop_lib::{
    errors::BackendError,
    models::{
        import_v2::{
            ImportInput, ImportInputKind, ImportItemStatus, ImportRecoveryAction,
            ImportResourceMode,
        },
        import_v2_web::WebImportErrorCode,
        paths::ProjectContext,
        task::{TaskStatus, TaskType},
    },
    services::{
        import_v2::{
            domain_router::{ConnectorAvailability, DomainRouter},
            engine::{EngineDescriptor, EngineRequest, EngineResult, ImportEngine},
            url_policy::UrlPolicy,
            ImportV2Service,
        },
        FileStore,
    },
    tasks::{task_model::CancellationToken, TaskService},
};
use std::sync::Arc;
#[test]
fn stage_one_routes_and_stable_errors_are_frozen() {
    let a = ConnectorAvailability {
        browser: true,
        wechat: true,
        zhihu: true,
        bilibili: true,
        phase_two: false,
    };
    for host in ["mp.weixin.qq.com", "www.zhihu.com", "www.bilibili.com"] {
        let t = UrlPolicy
            .normalize_for_session(&format!("https://{host}/one?token=secret#frag"))
            .unwrap();
        let p = DomainRouter::plan(&t.public, &a);
        assert!(p.release_enabled);
        assert!(!serde_json::to_string(&t.public).unwrap().contains("secret"));
    }
    assert_eq!(
        serde_json::to_value(WebImportErrorCode::PrivateTargetBlocked).unwrap(),
        "private_target_blocked"
    );
}
#[test]
fn phase_two_stays_closed_without_gate() {
    let a = ConnectorAvailability {
        browser: true,
        wechat: true,
        zhihu: true,
        bilibili: true,
        phase_two: false,
    };
    for host in ["www.xiaohongshu.com", "x.com"] {
        let t = UrlPolicy
            .normalize_for_session(&format!("https://{host}/one"))
            .unwrap();
        assert!(!DomainRouter::plan(&t.public, &a).release_enabled);
    }
}

struct LoginEngine;
impl ImportEngine for LoginEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "login-fixture".into(),
            engine_version: "1".into(),
            route: "web.wechat.article".into(),
        }
    }
    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::Url
    }
    fn execute(
        &self,
        _: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        Err(BackendError::new(
            "IMPORT_WEB_LOGIN_REQUIRED",
            "secret-cookie-must-not-persist",
            false,
            true,
        ))
    }
}

#[test]
fn login_required_pauses_item_and_task_with_typed_recovery_without_secret_logs() {
    let root = std::env::temp_dir().join(format!("web-login-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("p", root.clone());
    let files = FileStore;
    let service = ImportV2Service::default();
    service.register_engine(Arc::new(LoginEngine)).unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::Url,
                display_name: "wechat".into(),
                locator: "https://mp.weixin.qq.com/s/id".into(),
                normalized_locator: Some("https://mp.weixin.qq.com/s/id".into()),
                source_identity: None,
            }],
        )
        .unwrap();
    let item = &session.items[0];
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            "p".into(),
            root.clone(),
            "web".into(),
            true,
        )
        .unwrap();
    let result = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item.item_id,
            &task.id,
        )
        .unwrap();
    assert_eq!(result.status, ImportItemStatus::WaitingLogin);
    assert!(result
        .issue
        .unwrap()
        .recovery_actions
        .contains(&ImportRecoveryAction::BeginLogin));
    assert_eq!(
        tasks.get_task(&task.id).unwrap().status,
        TaskStatus::WaitingForConfirmation
    );
    let persisted = std::fs::read_to_string(root.join(format!(
        ".app/import-sessions/{}/session.json",
        session.session_id
    )))
    .unwrap();
    let logs = serde_json::to_string(&tasks.get_logs(&task.id).unwrap()).unwrap();
    assert!(!persisted.contains("secret-cookie") && !logs.contains("secret-cookie"));
    let released = service
        .release_item_after_login(&context, &files, &session.session_id, &item.item_id)
        .unwrap();
    assert_eq!(released.status, ImportItemStatus::Failed);
    assert!(released.task_id.is_none());
    let retry = tasks
        .create_project_task(TaskType::Import, "p".into(), root.clone(), "retry".into(), true)
        .unwrap();
    let retried = service
        .run_item(&context, &files, &tasks, &session.session_id, &item.item_id, &retry.id)
        .unwrap();
    assert_eq!(retried.status, ImportItemStatus::WaitingLogin);
    std::fs::remove_dir_all(root).ok();
}
