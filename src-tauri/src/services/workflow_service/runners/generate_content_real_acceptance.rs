//! Opt-in real Claude acceptance; only synthetic disposable content leaves the machine.
#![cfg(test)]
use super::*;
use crate::app_state::AppState;
use crate::models::agent::{AgentConfig, AgentKind};
use crate::models::project::ProjectTemplate;
use crate::models::workflow::{WorkflowDisplayStatus, WorkflowStartOutcome};
use crate::services::{PrepareWorkflowInput, ProjectService, WorkflowPreparationEnvironment};
use std::fs;

#[tokio::test]
#[ignore = "requires LLM_WIKI_RUN_REAL_CLAUDE=1 and an authenticated installed Claude CLI"]
async fn real_claude_exports_all_four_html_types_in_disposable_chinese_project() {
    assert_eq!(
        std::env::var("LLM_WIKI_RUN_REAL_CLAUDE").as_deref(),
        Ok("1")
    );
    let evidence = tempfile::Builder::new()
        .prefix("llm-wiki-real-export-")
        .tempdir()
        .unwrap()
        .keep();
    println!("real_export_evidence={}", evidence.display());
    let root = evidence.join("中文 导出验收");
    let state = AppState {
        project_service: ProjectService::with_config_dir(evidence.join("config")),
        ..AppState::default()
    };
    let project = state
        .project_service
        .create_project(
            root.to_str().unwrap(),
            "Export acceptance",
            ProjectTemplate::General,
        )
        .unwrap();
    let context = state
        .project_registry
        .register_trusted_native(&project.project_id, &root)
        .unwrap();
    state
        .task_service
        .set_project_context(
            context.project_id.clone(),
            root.clone(),
            root.join(".app/tasks"),
        )
        .unwrap();
    state
        .file_store
        .write_json_atomic(
            &context,
            ".app/agent-config.json",
            &AgentConfig {
                default_agent: Some(AgentKind::Claude),
            },
        )
        .unwrap();
    let pages = vec![
        "wiki/导出流程.md".to_string(),
        "wiki/文件安全.md".to_string(),
    ];
    fs::write(root.join(&pages[0]), "---\ntitle: 导出流程\ntype: concept\ntags: [知识管理]\n---\n# 导出流程\n选择内容，生成HTML，验证资源后写入新制品。阅读页关注一篇文章，知识卡片可选多篇，概念图展示关系，项目报告汇总全库。参见 [[文件安全]]。\n").unwrap();
    fs::write(root.join(&pages[1]), "---\ntitle: 文件安全\ntype: concept\ntags: [本地优先]\n---\n# 文件安全\n默认创建新制品，显式覆盖前必须创建Git检查点并复核；生成时修改来源或资源需要重新准备。原始资料不能修改。参见 [[导出流程]]。\n").unwrap();
    state
        .workflow_service
        .register_runner(Arc::new(GenerateContentRunner::new(|_| {
            panic!("acceptance drives authorized runner")
        })))
        .unwrap();
    let services = GenerateContentExecutionServices {
        export_service: &state.export_service,
        search_service: &state.search_service,
        settings_service: &state.settings_service,
        secret_service: &state.secret_service,
        agent_service: &state.agent_service,
        llm_service: &state.llm_service,
        git_service: &state.git_service,
        confirmation_registry: &state.confirmation_registry,
        task_service: &state.task_service,
        coordinator: &state.workflow_service.coordinator,
    };
    for artifact_type in [
        WorkflowArtifactType::BeautifulRead,
        WorkflowArtifactType::KnowledgeCard,
        WorkflowArtifactType::ConceptMap,
        WorkflowArtifactType::ProjectReport,
    ] {
        let page_paths = match artifact_type {
            WorkflowArtifactType::BeautifulRead => vec![pages[0].clone()],
            WorkflowArtifactType::ProjectReport => vec![],
            _ => pages.clone(),
        };
        let prepared = state
            .workflow_service
            .prepare(
                &WorkflowPreparationEnvironment {
                    context: &context,
                    access: state.resolve_workflow_access(&context).unwrap(),
                    settings_service: &state.settings_service,
                    secret_service: &state.secret_service,
                    agent_service: &state.agent_service,
                },
                PrepareWorkflowInput {
                    kind: WorkflowKind::GenerateContent,
                    scope: Some(WorkflowScope::GenerateContent {
                        artifact_type: artifact_type.clone(),
                        page_paths,
                        output_path: None,
                    }),
                    route_selection: None,
                },
            )
            .unwrap();
        assert!(
            !prepared.prerequisites.iter().any(|item| item.blocking),
            "{:?}",
            prepared.prerequisites
        );
        let outcome = state
            .with_current_project_task_access(
                &context.project_id,
                root.to_str().unwrap(),
                |permit| {
                    state.workflow_service.enqueue_with_acknowledgements(
                        permit,
                        &state.settings_service,
                        &state.secret_service,
                        &state.agent_service,
                        &state.task_service,
                        &prepared.preparation_id,
                        &prepared.preparation_revision,
                        false,
                        false,
                        None,
                    )
                },
            )
            .unwrap();
        let WorkflowStartOutcome::Created { run } = outcome else {
            panic!("new run")
        };
        let id = run.task_id.clone();
        run_generate_content_with_authority(
            &context,
            run.clone(),
            &services,
            || state.publish_workflow_external_launch(&context, &run),
            || state.publish_workflow_external_launch(&context, &run),
        )
        .await;
        let finished = state.task_service.get_workflow_run(&id).unwrap();
        fs::write(
            evidence.join(format!("{artifact_type:?}.json")),
            serde_json::to_vec_pretty(&finished).unwrap(),
        )
        .unwrap();
        assert_eq!(
            finished.display_status,
            WorkflowDisplayStatus::Completed,
            "{:?}: {:?}; {}",
            artifact_type,
            finished.error,
            evidence.display()
        );
        let records = state.export_service.list_records(&context).unwrap();
        let record = records
            .iter()
            .find(|record| record.task_id.as_deref() == Some(&id))
            .unwrap();
        let html = fs::read_to_string(
            state
                .export_service
                .resolve_existing_html_export(&context, &record.output_path)
                .unwrap(),
        )
        .unwrap();
        assert!(html.contains("Content-Security-Policy"));
        println!(
            "real_export type={artifact_type:?} task={id} bytes={} output={} record={}",
            html.len(),
            record.output_path,
            record.id
        );
    }
    assert_eq!(
        state.export_service.list_records(&context).unwrap().len(),
        4
    );
}
