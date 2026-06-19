use crate::services::{
    AgentService, ExportService, FileStore, GitService, GraphService, ImportService, LlmService,
    ProjectService, SearchService, SettingsService,
};
use crate::tasks::TaskService;

#[derive(Default)]
pub struct AppState {
    pub project_service: ProjectService,
    pub file_store: FileStore,
    pub import_service: ImportService,
    pub git_service: GitService,
    pub agent_service: AgentService,
    pub llm_service: LlmService,
    pub search_service: SearchService,
    pub graph_service: GraphService,
    pub export_service: ExportService,
    pub settings_service: SettingsService,
    pub task_service: TaskService,
}
