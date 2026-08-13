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
pub(crate) mod project_service;
mod search_service;
mod secret_service;
mod settings_service;
mod wiki_index;
mod workflow_service;

pub use agent_service::{
    AgentInvocation, AgentProbeTarget, AgentService, ProcessRunner, SystemProcessRunner,
};
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
pub use export_service::{ExportService, ValidatedExportArtifact};
pub use file_store::{FileStore, WriteMode};
pub use git_service::GitService;
pub use graph_service::GraphService;
pub use lint_service::{
    health_source_paths, AgentLintRepairCandidate, AgentLintRepairWorkspaceDescriptor,
    AgentLintRepairWorkspaceLease, DeepLintSnapshot, LintService, LocalLintPhase,
};
pub use llm_service::LlmService;
pub use project_service::{assess_project_folder, ProjectAssessmentService, ProjectService};
pub use search_service::SearchService;
pub use secret_service::SecretService;
pub use settings_service::SettingsService;
pub use wiki_index::{IndexEntry, WikiIndex};
pub(crate) use workflow_service::recover_workflow;
pub use workflow_service::{
    agent_lint_repair_attestation_digest, agent_lint_repair_interrupted_result,
    agent_lint_repair_result_digest, agent_lint_repair_stages,
    agent_lint_repair_terminal_file_diff_page, cancel_agent_lint_repair_review,
    cancel_generate_content_confirmation, canonical_json,
    confirm_agent_lint_repair_review_authorized,
    confirm_agent_lint_repair_review_with_round_executor, confirm_generate_content_overwrite,
    confirm_update_wiki_review, discard_generate_content_candidate, discard_update_wiki_candidate,
    execute_agent_lint_repair_round_with, generate_content_candidate_is_valid_for_workflow,
    persist_update_wiki_review, project_identity, reconcile_agent_lint_repair_after_recovery,
    record_agent_lint_repair_recovery_failure, resolve_workflow_persistence_binding,
    restore_agent_lint_repair_confirmation, restore_generate_content_confirmation,
    restore_update_wiki_confirmation, rollback_and_discard_agent_lint_repair_candidate,
    run_agent_lint_repair_authorized, run_agent_lint_repair_with_round_executor,
    run_generate_content, run_generate_content_authorized, run_generate_content_with_generator,
    run_health_check, run_health_check_authorized, run_health_check_with_deep, run_update_wiki,
    run_update_wiki_authorized, update_wiki_candidate_is_valid, update_wiki_decision_review,
    workflow_baseline_for_scope, workflow_fingerprint, workflow_stages,
    AgentLintRepairConfirmationFailure, AgentLintRepairExecutionServices,
    AgentLintRepairRoundExecution, AgentLintRepairRunner, EnqueueWorkflow,
    GenerateContentConfirmationFailure, GenerateContentExecutionServices, GenerateContentRunner,
    HealthCheckExecutionServices, HealthCheckRunner, PrepareWorkflowInput, ProjectWorkflowIdentity,
    UpdateWikiConfirmationFailure, UpdateWikiExecutionServices, UpdateWikiRunner,
    ValidatedWorkflowStart, WorkflowAccessSnapshot, WorkflowCoordinator, WorkflowDispatchFailure,
    WorkflowExternalLaunchPermit, WorkflowLaunchCloseBarrier, WorkflowLaunchPublication,
    WorkflowLaunchRegistry, WorkflowPersistenceBinding, WorkflowPreference, WorkflowPreferences,
    WorkflowPreparationEnvironment, WorkflowPreparationService, WorkflowRunner, WorkflowService,
    WorkflowStageSink, WorkflowTrustTransition,
};
#[cfg(feature = "gui")]
pub(crate) use workflow_service::{
    agent_lint_repair_decision_review, agent_lint_repair_file_diff_page,
    update_wiki_decision_review_for_workflow, update_wiki_decision_review_summary_for_workflow,
    update_wiki_file_diff_page_for_workflow, update_wiki_review_can_inline,
};
