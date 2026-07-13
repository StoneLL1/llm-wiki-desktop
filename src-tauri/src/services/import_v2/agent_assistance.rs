use std::path::{Path, PathBuf};

use crate::{
    errors::BackendError,
    models::{
        agent::AgentKind,
        import_v2::{AttemptOutcome, ImportItemStatus},
        import_v2_agent::{AgentAssistancePolicy, AgentAssistanceTrigger},
        paths::ProjectContext,
        task::{BackendTask, TaskResult, TaskStatus, TaskType},
    },
    services::{AgentService, FileStore, SettingsService},
    tasks::TaskService,
};

use super::{agent_workspace::AgentWorkspaceBuilder, ImportV2Service};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalAgentStartDecision {
    Start,
    ManualOnly,
    AgentUnavailable,
    AttemptBudgetExhausted,
}

pub struct AgentAssistanceService<'a> {
    imports: &'a ImportV2Service,
    files: &'a FileStore,
    settings: &'a SettingsService,
    agents: &'a AgentService,
    tasks: &'a TaskService,
    skill_path: PathBuf,
}

impl<'a> AgentAssistanceService<'a> {
    pub fn new(
        imports: &'a ImportV2Service,
        files: &'a FileStore,
        settings: &'a SettingsService,
        agents: &'a AgentService,
        tasks: &'a TaskService,
        skill_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            imports,
            files,
            settings,
            agents,
            tasks,
            skill_path: skill_path.into(),
        }
    }

    pub fn bundled_skill_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/skills/wiki-ingest-assist/SKILL.md")
    }

    pub fn local_start_decision(
        policy: &AgentAssistancePolicy,
        trigger: AgentAssistanceTrigger,
        available: bool,
        prior_agent_attempts: usize,
    ) -> LocalAgentStartDecision {
        if prior_agent_attempts >= usize::from(policy.max_attempts_per_item) {
            return LocalAgentStartDecision::AttemptBudgetExhausted;
        }
        if !available {
            return LocalAgentStartDecision::AgentUnavailable;
        }
        match trigger {
            AgentAssistanceTrigger::DeterministicHardFailure
                if policy.auto_local_on_hard_failure =>
            {
                LocalAgentStartDecision::Start
            }
            AgentAssistanceTrigger::Manual => LocalAgentStartDecision::Start,
            AgentAssistanceTrigger::DeterministicHardFailure
            | AgentAssistanceTrigger::QualityOptimization => LocalAgentStartDecision::ManualOnly,
        }
    }

    pub fn start_local(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        trigger: AgentAssistanceTrigger,
        agent_kind: AgentKind,
    ) -> Result<BackendTask, BackendError> {
        let session = self.imports.load_session(context, self.files, session_id)?;
        let item = session
            .items
            .iter()
            .find(|item| item.item_id == item_id)
            .ok_or_else(|| {
                assistance_error("IMPORT_V2_ITEM_NOT_FOUND", "Import item was not found.")
            })?;
        let settings = self.settings.read_settings(context)?;
        if agent_kind != AgentKind::Claude {
            return Err(assistance_error(
                "IMPORT_AGENT_PROFILE_UNSUPPORTED",
                "The selected Agent CLI has no verified tool-free Import profile.",
            ));
        }
        if item.status == ImportItemStatus::Failed
            && !item.issue.as_ref().is_some_and(|issue| {
                issue.available_actions.contains(
                    &crate::models::import_v2_agent::AgentRecoveryAction::InvokeLocalAgent,
                )
            })
        {
            return Err(assistance_error(
                "IMPORT_AGENT_TRIGGER_REJECTED",
                "This failure is not eligible for Agent assistance.",
            ));
        }
        let attempts = item
            .attempts
            .iter()
            .filter(|attempt| attempt.route.starts_with("agent_assistance/"))
            .count();
        let decision = Self::local_start_decision(
            &settings.import_agent_policy,
            trigger,
            self.agents.is_available(agent_kind),
            attempts,
        );
        match decision {
            LocalAgentStartDecision::AgentUnavailable => {
                return Err(assistance_error(
                    "IMPORT_AGENT_UNAVAILABLE",
                    "The selected local Agent is not installed or detected. Installation was not attempted.",
                ));
            }
            LocalAgentStartDecision::AttemptBudgetExhausted => {
                return Err(assistance_error(
                    "IMPORT_AGENT_ATTEMPT_LIMIT",
                    "The Agent assistance attempt budget is exhausted for this item.",
                ));
            }
            LocalAgentStartDecision::ManualOnly
                if trigger == AgentAssistanceTrigger::DeterministicHardFailure =>
            {
                return Err(assistance_error(
                    "IMPORT_AGENT_AUTOMATION_NOT_APPROVED",
                    "Automatic Agent assistance is disabled; a manual action remains available.",
                ));
            }
            _ => {}
        }
        if trigger == AgentAssistanceTrigger::DeterministicHardFailure
            && settings.agent_default != Some(agent_kind)
        {
            return Err(assistance_error(
                "IMPORT_AGENT_AUTOMATION_NOT_APPROVED",
                "Automatic Agent assistance may use only the selected local Agent.",
            ));
        }
        if trigger == AgentAssistanceTrigger::QualityOptimization
            && item.status != ImportItemStatus::PreviewReady
        {
            return Err(assistance_error(
                "IMPORT_V2_STATE_INVALID",
                "Quality optimization requires a deterministic preview.",
            ));
        }

        let task = self
            .tasks
            .create_project_task(
                TaskType::AgentRun,
                context.project_id.clone(),
                context.root.clone(),
                format!("Agent assistance for {}", item.input.display_name),
                true,
            )
            .map_err(|error| assistance_error("IMPORT_AGENT_TASK_FAILED", &error))?;
        if let Err(error) = self.imports.begin_agent_assistance(
            context,
            self.files,
            session_id,
            item_id,
            &task.id,
            trigger,
            agent_kind,
            settings.import_agent_policy.max_attempts_per_item,
        ) {
            let _ = self
                .tasks
                .discard_unstarted_tasks(std::slice::from_ref(&task.id));
            return Err(error);
        }
        Ok(task)
    }

    pub fn run_local(
        &self,
        context: &ProjectContext,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        trigger: AgentAssistanceTrigger,
        agent_kind: AgentKind,
    ) -> Result<(), BackendError> {
        if self.tasks.is_cancelled(task_id) {
            let _ = self.imports.finish_agent_assistance_attempt(
                context,
                self.files,
                session_id,
                item_id,
                task_id,
                AttemptOutcome::Cancelled,
                vec!["Agent assistance was cancelled before process start.".into()],
            );
            return Err(assistance_error(
                "AGENT_CANCELLED",
                "Agent assistance was cancelled before process start.",
            ));
        }
        self.tasks
            .transition_status(task_id, TaskStatus::Running)
            .map_err(|error| assistance_error("IMPORT_AGENT_TASK_FAILED", &error))?;
        let result = (|| {
            let session = self.imports.load_session(context, self.files, session_id)?;
            let item = session
                .items
                .iter()
                .find(|item| item.item_id == item_id)
                .ok_or_else(|| {
                    assistance_error("IMPORT_V2_ITEM_NOT_FOUND", "Import item was not found.")
                })?;
            if item.task_id.as_deref() != Some(task_id) {
                return Err(assistance_error(
                    "IMPORT_AGENT_TASK_MISMATCH",
                    "Agent task is not bound to this Import item.",
                ));
            }
            let workspace = AgentWorkspaceBuilder.build(context, &session, item, trigger)?;
            if self.tasks.is_cancelled(task_id) {
                let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                return Err(assistance_error(
                    "AGENT_CANCELLED",
                    "Agent assistance was cancelled before process start.",
                ));
            }
            let invocation = match AgentService::import_assistance_invocation(
                agent_kind,
                &workspace.root,
                &self.skill_path,
            ) {
                Ok(invocation) => invocation,
                Err(error) => {
                    let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                    return Err(error);
                }
            };
            let output = match self
                .agents
                .run_import_assistance(&invocation, self.tasks, task_id)
            {
                Ok(output) => output,
                Err(error) => {
                    let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                    return Err(error);
                }
            };
            if self.tasks.is_cancelled(task_id) {
                let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                return Err(assistance_error(
                    "AGENT_CANCELLED",
                    "Agent assistance was cancelled before candidate validation.",
                ));
            }
            if output.trim().is_empty() || output.len() > 16 * 1024 * 1024 || output.contains('\0')
            {
                let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                return Err(assistance_error(
                    "IMPORT_AGENT_OUTPUT_INVALID",
                    "Agent output is empty, malformed, or exceeds the candidate limit.",
                ));
            }
            if let Err(error) = AgentWorkspaceBuilder::validate_output_target(&workspace) {
                let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                return Err(error);
            }
            if std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(workspace.output_dir.join("candidate.md"))
                .and_then(|mut file| {
                    use std::io::Write;
                    file.write_all(output.as_bytes())?;
                    file.sync_all()
                })
                .is_err()
            {
                let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                return Err(assistance_error(
                    "IMPORT_AGENT_OUTPUT_INVALID",
                    "Agent output could not be staged for candidate validation.",
                ));
            }
            let relative_workspace = workspace
                .root
                .strip_prefix(&context.root)
                .map_err(|_| {
                    assistance_error(
                        "IMPORT_AGENT_WORKSPACE_INVALID",
                        "Agent workspace escaped the project.",
                    )
                })?
                .to_string_lossy()
                .replace('\\', "/");
            if let Err(error) = self.tasks.complete_running_with_result(
                task_id,
                TaskResult {
                    summary: "Agent output is staged for candidate validation.".into(),
                    affected_paths: vec![format!("{relative_workspace}/output")],
                    reference: None,
                    pending_action: None,
                },
            ) {
                let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                return Err(assistance_error("IMPORT_AGENT_TASK_FAILED", &error));
            }
            // The durable task result is the recovery authority. If session
            // persistence is interrupted here, recover_session reconciles the
            // unfinished attempt from this Succeeded task without rerunning or
            // charging the Agent again.
            let _ = self.imports.finish_agent_assistance_attempt(
                context,
                self.files,
                session_id,
                item_id,
                task_id,
                AttemptOutcome::Succeeded,
                Vec::new(),
            );
            Ok(())
        })();

        match result {
            Ok(()) => Ok(()),
            Err(error) => {
                let cancelled = self.tasks.is_cancelled(task_id) || error.code == "AGENT_CANCELLED";
                let outcome = if cancelled {
                    AttemptOutcome::Cancelled
                } else {
                    AttemptOutcome::Failed
                };
                let _ = self.imports.finish_agent_assistance_attempt(
                    context,
                    self.files,
                    session_id,
                    item_id,
                    task_id,
                    outcome,
                    vec!["Agent output was discarded before candidate validation.".into()],
                );
                if cancelled {
                    if self.tasks.get_task(task_id).map(|task| task.status)
                        == Some(TaskStatus::Cancelling)
                    {
                        let _ = self.tasks.transition_status(task_id, TaskStatus::Cancelled);
                    }
                } else {
                    let safe = assistance_error(
                        &error.code,
                        "Local Agent assistance failed; no Agent output was accepted.",
                    );
                    let _ = self.tasks.set_error(task_id, safe.clone());
                    let _ = self.tasks.transition_status(task_id, TaskStatus::Failed);
                }
                Err(error)
            }
        }
    }
}

fn assistance_error(code: &str, message: &str) -> BackendError {
    BackendError::new(code, message, true, true)
}
