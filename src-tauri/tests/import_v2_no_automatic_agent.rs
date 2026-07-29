use std::sync::Arc;

use llm_wiki_desktop_lib::{
    errors::BackendError,
    models::{
        agent::AgentKind,
        import_v2::{ImportInput, ImportInputKind, ImportResourceMode, SourceIdentity},
        import_v2_agent::AgentAssistancePolicy,
        paths::ProjectContext,
        task::TaskType,
    },
    services::{
        import_v2::{
            engine::{EngineDescriptor, EngineRequest, EngineResult, ImportEngine},
            ImportV2Service,
        },
        FileStore, SettingsService,
    },
    tasks::{task_model::CancellationToken, TaskService},
};
use sha2::{Digest, Sha256};

struct FailingParser;

impl ImportEngine for FailingParser {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "fixture.fail".into(),
            engine_version: "1".into(),
            route: "file.native".into(),
        }
    }

    fn supports(&self, _: &ImportInput) -> bool {
        true
    }

    fn execute(
        &self,
        _: &EngineRequest,
        _: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        Err(BackendError::new(
            "IMPORT_FILE_PARSE_FAILED",
            "fixture parser failure",
            true,
            false,
        ))
    }
}

#[test]
fn configured_default_agent_does_not_turn_a_normal_parse_failure_into_an_agent_task() {
    let project = tempfile::tempdir().unwrap();
    let config = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("project-a", project.path().to_path_buf());
    let files = FileStore;
    let settings = SettingsService::with_config_dir(config.path().to_path_buf());
    settings
        .set_import_agent_policy(
            &context,
            AgentAssistancePolicy::default(),
            Some(AgentKind::Claude),
        )
        .unwrap();
    assert_eq!(
        settings.read_settings(&context).unwrap().agent_default,
        Some(AgentKind::Claude)
    );

    let source_path = project.path().join("fixture.txt");
    let source_bytes = b"fixture";
    std::fs::write(&source_path, source_bytes).unwrap();
    let canonical_path = source_path.canonicalize().unwrap();
    let canonical_locator = canonical_path.to_string_lossy().into_owned();
    let service = ImportV2Service::default();
    service.register_engine(Arc::new(FailingParser)).unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::File,
                display_name: "fixture.txt".into(),
                locator: canonical_locator.clone(),
                normalized_locator: None,
                source_identity: Some(SourceIdentity {
                    canonical_path: canonical_locator,
                    size_bytes: source_bytes.len() as u64,
                    modified_nanos: None,
                    file_id: None,
                    sha256: format!("{:x}", Sha256::digest(source_bytes)),
                    magic: format!("{:x}", Sha256::digest(source_bytes)),
                }),
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            context.root.clone(),
            "Import fixture".into(),
            true,
        )
        .unwrap();

    let error = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &session.items[0].item_id,
            &task.id,
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_FILE_PARSE_FAILED");
    assert!(tasks
        .list_tasks(None)
        .iter()
        .all(|task| task.task_type != TaskType::AgentRun));

    let command_source = include_str!("../src/commands/import_v2_commands.rs");
    for forbidden in [
        "AgentAssistanceService",
        "AgentAssistanceTrigger",
        "run_local_agent_candidate",
        "start_import_agent_assistance_v2",
    ] {
        assert!(
            !command_source.contains(forbidden),
            "ordinary Import command scheduling must not contain {forbidden}"
        );
    }
}
