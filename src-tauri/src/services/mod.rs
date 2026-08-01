mod agent_service;
mod bookmark_service;
mod chat_convenience_service;
mod chat_service;
mod compile_instructions;
mod compile_legacy_adapter;
mod compile_service;
mod export_service;
mod file_store;
mod git_service;
mod graph_service;
pub mod import_v2;
mod lint_service;
mod llm_service;
mod project_service;
mod search_service;
mod secret_service;
mod settings_service;
mod wiki_index;
mod workflow_service;

pub use agent_service::{AgentInvocation, AgentService, ProcessRunner, SystemProcessRunner};
pub use bookmark_service::BookmarkService;
pub use chat_convenience_service::{
    audit_changed_paths, classify_chat_intent, ChangedFile, ChatConvenienceService, ChatIntent,
    ConvenienceAuditReport, ConvenienceAuditStatus,
};
pub use chat_service::{ChatService, RetrievalContext};
pub use compile_instructions::{
    render_compile_core_instructions, render_compile_core_instructions_with_policy,
    render_compile_prompt_header, render_compile_prompt_header_with_policy,
    shared_compile_instruction_set, CompileInstructionSet, CompilePromptRoute,
};
pub use compile_legacy_adapter::{
    CompileLegacyAdapter, LegacyCompileDiagnostics, LegacyCompileSource,
};
pub use compile_service::{
    CompileExecutionServices, CompileGenerationObserver, CompileGenerationPolicy, CompileService,
    CompileSourceRegistry, NoopCompileGenerationObserver, ResolvedCompileSource,
};
pub use export_service::ExportService;
pub use file_store::{FileStore, WriteMode};
pub use git_service::GitService;
pub use graph_service::GraphService;
pub use lint_service::LintService;
pub use llm_service::LlmService;
pub use project_service::ProjectService;
pub use search_service::SearchService;
pub use secret_service::SecretService;
pub use settings_service::SettingsService;
pub use wiki_index::{IndexEntry, WikiIndex};
pub(crate) use workflow_service::recover_workflow;
pub use workflow_service::{
    canonical_json, discard_update_wiki_candidate, persist_update_wiki_review, project_identity,
    run_update_wiki, update_wiki_candidate_is_valid, workflow_baseline_for_scope,
    workflow_fingerprint, workflow_stages, EnqueueWorkflow, PrepareWorkflowInput,
    ProjectWorkflowIdentity, UpdateWikiExecutionServices, UpdateWikiRunner, ValidatedWorkflowStart,
    WorkflowAccessSnapshot, WorkflowCoordinator, WorkflowPreference, WorkflowPreferences,
    WorkflowPreparationEnvironment, WorkflowPreparationService, WorkflowRunner, WorkflowService,
    WorkflowStageSink,
};
