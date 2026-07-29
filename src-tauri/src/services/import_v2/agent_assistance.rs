use std::path::Path;

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::{
    errors::BackendError,
    models::{
        agent::AgentKind,
        import_v2::{AttemptOutcome, ImportItemStatus},
        import_v2_agent::{
            AgentAssistancePolicy, AgentAssistanceTrigger, AgentAuditRecord, AgentCandidateManifest,
        },
        paths::ProjectContext,
        task::{BackendTask, TaskResult, TaskResultReference, TaskStatus, TaskType},
    },
    services::{AgentService, FileStore, SettingsService},
    tasks::TaskService,
};

use super::{
    agent_workspace::{AgentTaskBundle, AgentWorkspaceBuilder},
    ImportV2Service,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalAgentStartDecision {
    Start,
    AgentUnavailable,
    AttemptBudgetExhausted,
}

pub struct AgentAssistanceService<'a> {
    imports: &'a ImportV2Service,
    files: &'a FileStore,
    settings: &'a SettingsService,
    agents: &'a AgentService,
    tasks: &'a TaskService,
}

impl<'a> AgentAssistanceService<'a> {
    pub fn new(
        imports: &'a ImportV2Service,
        files: &'a FileStore,
        settings: &'a SettingsService,
        agents: &'a AgentService,
        tasks: &'a TaskService,
    ) -> Self {
        Self {
            imports,
            files,
            settings,
            agents,
            tasks,
        }
    }

    pub fn local_start_decision(
        policy: &AgentAssistancePolicy,
        available: bool,
        prior_agent_attempts: usize,
    ) -> LocalAgentStartDecision {
        if prior_agent_attempts >= usize::from(policy.max_attempts_per_item) {
            return LocalAgentStartDecision::AttemptBudgetExhausted;
        }
        if !available {
            return LocalAgentStartDecision::AgentUnavailable;
        }
        LocalAgentStartDecision::Start
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
            _ => {}
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
        let audit_path =
            format!(".app/import-sessions/{session_id}/items/{item_id}/agent-audit/{task_id}.json");
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
            let workspace =
                AgentWorkspaceBuilder.build_for_task(context, &session, item, trigger, task_id)?;
            let bundle_bytes = super::agent_workspace::read_isolated_regular_file(
                &workspace.root,
                &workspace.task_path,
                64 * 1024,
            )?;
            let bundle: AgentTaskBundle = serde_json::from_slice(&bundle_bytes).map_err(|_| {
                assistance_error(
                    "IMPORT_AGENT_WORKSPACE_INVALID",
                    "Agent task bundle is invalid.",
                )
            })?;
            let agent_version = self
                .agents
                .detect_agents(None)
                .into_iter()
                .find(|status| status.kind == agent_kind)
                .and_then(|status| status.version)
                .unwrap_or_else(|| "unknown".into());
            let mut audit = AgentAuditRecord {
                audit_id: uuid::Uuid::new_v4().to_string(),
                task_id: task_id.into(),
                session_id: session_id.into(),
                item_id: item_id.into(),
                trigger,
                route: format!("local/{}", agent_kind.command()),
                agent_kind: Some(agent_kind),
                agent_version,
                prompt_template_version: "import-recovery/local-v1".into(),
                approved_cost_micros: None,
                tool_calls: Vec::new(),
                approved_scope_sha256: None,
                workspace_relative_path: workspace
                    .root
                    .strip_prefix(&context.root)
                    .map_err(|_| {
                        assistance_error(
                            "IMPORT_AGENT_WORKSPACE_INVALID",
                            "Agent workspace escaped the project.",
                        )
                    })?
                    .to_string_lossy()
                    .replace('\\', "/"),
                granted_tools: bundle.allowed_tools.clone(),
                input_hashes: bundle.input_hashes.clone(),
                output_hashes: Vec::new(),
                started_at: Utc::now(),
                completed_at: None,
                outcome: "running".into(),
                warnings: Vec::new(),
            };
            self.files.write_json_atomic(context, &audit_path, &audit)?;
            if self.tasks.is_cancelled(task_id) {
                let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                return Err(assistance_error(
                    "AGENT_CANCELLED",
                    "Agent assistance was cancelled before process start.",
                ));
            }
            let invocation = match AgentService::import_assistance_invocation_with_skill(
                agent_kind,
                &workspace.root,
                include_str!("../../../templates/skills/import-recovery/SKILL.md"),
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
            if let Err(error) = write_candidate_manifest(
                &workspace.output_dir,
                "sandboxed-local-agent",
                &format!("{:x}", Sha256::digest(output.as_bytes())),
            ) {
                let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                return Err(error);
            }
            audit.output_hashes = vec![format!("{:x}", Sha256::digest(output.as_bytes()))];
            audit.completed_at = Some(Utc::now());
            audit.outcome = "output_staged".into();
            self.files.write_json_atomic(context, &audit_path, &audit)?;
            let relative_workspace = audit.workspace_relative_path.clone();
            if let Err(error) = self.tasks.complete_running_with_result(
                task_id,
                TaskResult {
                    summary: "Agent output is staged for candidate validation.".into(),
                    affected_paths: vec![format!("{relative_workspace}/output")],
                    reference: Some(TaskResultReference::ImportPreview {
                        session_id: session_id.into(),
                        item_id: item_id.into(),
                    }),
                    pending_action: None,
                },
            ) {
                let _ = AgentWorkspaceBuilder::cleanup_terminal(&workspace);
                return Err(assistance_error("IMPORT_AGENT_TASK_FAILED", &error));
            }
            audit.outcome = "succeeded".into();
            let _ = self.files.write_json_atomic(context, &audit_path, &audit);
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
                if self.files.exists(context, &audit_path) {
                    if let Ok(mut audit) = self
                        .files
                        .read_json::<AgentAuditRecord>(context, &audit_path)
                    {
                        audit.completed_at = Some(Utc::now());
                        audit.outcome = if cancelled { "cancelled" } else { "failed" }.into();
                        audit.warnings.push(error.code.clone());
                        let _ = self.files.write_json_atomic(context, &audit_path, &audit);
                    }
                }
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

fn write_candidate_manifest(
    output_dir: &Path,
    route: &str,
    markdown_sha256: &str,
) -> Result<(), BackendError> {
    let manifest = AgentCandidateManifest {
        markdown_path: "candidate.md".into(),
        asset_paths: Vec::new(),
        markdown_sha256: markdown_sha256.into(),
        asset_sha256: std::collections::BTreeMap::new(),
        processing_summary: "AI-assisted Markdown candidate staged for validation.".into(),
        tools_used: vec![route.into()],
        uncertainties: vec![
            "The generated structure may differ from the deterministic extraction.".into(),
        ],
        warnings: vec!["Review the candidate Diff before selection.".into()],
    };
    let bytes = serde_json::to_vec_pretty(&manifest).map_err(|_| {
        assistance_error(
            "IMPORT_AGENT_OUTPUT_INVALID",
            "Agent manifest could not be encoded.",
        )
    })?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_dir.join("manifest.json"))
        .map_err(|_| {
            assistance_error(
                "IMPORT_AGENT_OUTPUT_INVALID",
                "Agent manifest could not be staged.",
            )
        })?;
    use std::io::Write;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| {
            assistance_error(
                "IMPORT_AGENT_OUTPUT_INVALID",
                "Agent manifest could not be staged.",
            )
        })
}
