use crate::models::confirmation::ConfirmationRegistry;
use crate::services::{
    AgentService, ChatService, ExportService, ExtractionService, FileStore, GitService,
    GraphService, ImportService, LintService, LlmService, ProjectService, SearchService,
    SecretService, SettingsService,
};
use crate::tasks::TaskService;

#[derive(Default)]
pub struct AppState {
    pub project_service: ProjectService,
    pub file_store: FileStore,
    pub import_service: ImportService,
    pub extraction_service: ExtractionService,
    pub git_service: GitService,
    pub agent_service: AgentService,
    pub chat_service: ChatService,
    pub llm_service: LlmService,
    pub search_service: SearchService,
    pub graph_service: GraphService,
    pub lint_service: LintService,
    pub export_service: ExportService,
    pub settings_service: SettingsService,
    pub secret_service: SecretService,
    pub task_service: TaskService,
    pub confirmation_registry: ConfirmationRegistry,
}
