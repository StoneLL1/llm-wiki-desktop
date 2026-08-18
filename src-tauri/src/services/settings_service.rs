use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::errors::BackendError;
use crate::models::agent::{AgentConfig, AgentKind};
use crate::models::import_v2_agent::AgentAssistancePolicy;
use crate::models::llm::{LlmProviderConfig, LlmProviderKind, ProviderCredentialBinding};
use crate::models::paths::ProjectContext;
use crate::models::settings::{
    AgentLintRepairAttestation, AgentLintRepairAttestationLifecycle,
    AgentLintRepairMutationJournal, AgentLintRepairMutationPhase, ChatConvenienceAuthorization,
    CloseBehavior, GlobalSettingsFile, GlobalUiPreferences, ProjectSettingsFile, Settings,
};
use crate::services::FileStore;

pub struct SettingsService {
    config_dir: PathBuf,
}

const MAX_AGENT_LINT_REPAIR_ATTESTATIONS: usize = 256;
const MAX_AGENT_LINT_REPAIR_JOURNAL_PATHS: usize = 4_096;
const AGENT_LINT_REPAIR_CANCEL_TOMBSTONE_MINUTES: i64 = 15;

impl Default for SettingsService {
    fn default() -> Self {
        Self {
            config_dir: default_config_dir(),
        }
    }
}

impl SettingsService {
    pub fn with_config_dir(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    pub fn read_settings(&self, context: &ProjectContext) -> Result<Settings, BackendError> {
        let mut settings = Settings::default();
        settings.apply_global(self.read_global_settings()?);

        if let Some(settings_path) = context.layout.settings_path.as_deref() {
            let project_path = context.resolve_project_path(settings_path)?;
            if project_path.exists() {
                let project: ProjectSettingsFile = FileStore.read_json(context, settings_path)?;
                settings.apply_project(project);
            }
        }

        if let Some(agent_config_path) = context.layout.agent_config_path.as_deref() {
            let agent_config_file = context.resolve_project_path(agent_config_path)?;
            if agent_config_file.exists() {
                let agent_config: AgentConfig = FileStore.read_json(context, agent_config_path)?;
                settings.agent_default = agent_config.default_agent;
            }
        }

        Ok(settings)
    }

    pub fn save_settings(
        &self,
        context: &ProjectContext,
        settings: &Settings,
    ) -> Result<Settings, BackendError> {
        self.save_settings_internal(context, settings, false)
    }

    fn save_settings_internal(
        &self,
        context: &ProjectContext,
        settings: &Settings,
        allow_provider_binding_update: bool,
    ) -> Result<Settings, BackendError> {
        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        let _guard = self.lock_global_settings()?;
        let mut global = settings.to_global_file();
        let existing_global = self.read_global_settings()?;
        global.chat_convenience_authorizations = existing_global.chat_convenience_authorizations;
        global.agent_lint_repair_attestations = existing_global.agent_lint_repair_attestations;
        global.remote_provider_disclosure_revision =
            existing_global.remote_provider_disclosure_revision;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &global,
        )?;
        let settings_path =
            project_state_path(context, context.layout.settings_path.as_deref(), "settings")?;
        let agent_config_path = project_state_path(
            context,
            context.layout.agent_config_path.as_deref(),
            "agent configuration",
        )?;
        let mut project = settings.to_project_file();
        if !allow_provider_binding_update {
            let absolute_settings_path = context.resolve_project_path(settings_path)?;
            if absolute_settings_path.exists() {
                let existing: ProjectSettingsFile = store.read_json(context, settings_path)?;
                project.llm_providers = existing.llm_providers;
                project.provider_credential_bindings = existing.provider_credential_bindings;
            } else {
                project.llm_providers.clear();
                project.provider_credential_bindings.clear();
            }
        }
        store.write_json_atomic(context, settings_path, &project)?;
        store.write_json_atomic(
            context,
            agent_config_path,
            &AgentConfig {
                default_agent: settings.agent_default,
            },
        )?;
        self.read_settings(context)
    }

    pub fn save_agent_default(
        &self,
        context: &ProjectContext,
        agent: Option<AgentKind>,
    ) -> Result<AgentConfig, BackendError> {
        let mut settings = self.read_settings(context)?;
        settings.agent_default = agent;
        let config = AgentConfig {
            default_agent: agent,
        };
        let store = FileStore;
        let settings_path =
            project_state_path(context, context.layout.settings_path.as_deref(), "settings")?;
        let agent_config_path = project_state_path(
            context,
            context.layout.agent_config_path.as_deref(),
            "agent configuration",
        )?;
        store.write_json_atomic(context, agent_config_path, &config)?;
        store.write_json_atomic(context, settings_path, &settings.to_project_file())?;
        Ok(config)
    }

    pub fn get_import_agent_policy(
        &self,
        context: &ProjectContext,
    ) -> Result<AgentAssistancePolicy, BackendError> {
        Ok(self.read_settings(context)?.import_agent_policy)
    }

    pub fn set_import_agent_policy(
        &self,
        context: &ProjectContext,
        policy: AgentAssistancePolicy,
        local_agent_kind: Option<AgentKind>,
    ) -> Result<AgentAssistancePolicy, BackendError> {
        if policy.max_attempts_per_item == 0 || policy.max_attempts_per_item > 3 {
            return Err(BackendError::new(
                "IMPORT_AGENT_POLICY_INVALID",
                "Agent assistance attempt budget must be between one and three.",
                false,
                true,
            ));
        }
        let mut settings = self.read_settings(context)?;
        settings.import_agent_policy = policy.clone();
        settings.agent_default = local_agent_kind;
        self.save_settings(context, &settings)?;
        Ok(policy)
    }

    pub fn list_providers(
        &self,
        context: &ProjectContext,
    ) -> Result<Vec<LlmProviderConfig>, BackendError> {
        Ok(self.read_settings(context)?.llm_providers)
    }

    pub fn merge_providers(
        &self,
        context: &ProjectContext,
        providers: Vec<LlmProviderConfig>,
    ) -> Result<Settings, BackendError> {
        let mut settings = self.read_settings(context)?;
        settings.llm_providers = providers;
        self.save_settings(context, &settings)
    }

    pub fn save_provider(
        &self,
        context: &ProjectContext,
        config: LlmProviderConfig,
    ) -> Result<Settings, BackendError> {
        let mut settings = self.read_settings(context)?;
        settings
            .llm_providers
            .retain(|item| item.provider != config.provider);
        settings.llm_providers.push(config);
        settings
            .llm_providers
            .sort_by_key(|item| item.provider.credential_account().to_string());
        self.save_settings(context, &settings)
    }

    pub fn save_provider_with_binding(
        &self,
        context: &ProjectContext,
        config: LlmProviderConfig,
        binding: ProviderCredentialBinding,
    ) -> Result<Settings, BackendError> {
        let mut settings = self.read_settings(context)?;
        settings
            .llm_providers
            .retain(|item| item.provider != config.provider);
        settings.llm_providers.push(config);
        settings
            .llm_providers
            .sort_by_key(|item| item.provider.credential_account().to_string());
        settings
            .provider_credential_bindings
            .retain(|item| item.provider_kind != binding.provider_kind);
        settings.provider_credential_bindings.push(binding);
        settings
            .provider_credential_bindings
            .sort_by_key(|item| item.provider_kind.binding_slug().to_string());
        self.save_settings_internal(context, &settings, true)
    }

    pub fn provider_credential_binding(
        &self,
        context: &ProjectContext,
        provider: LlmProviderKind,
    ) -> Result<Option<ProviderCredentialBinding>, BackendError> {
        Ok(self
            .read_settings(context)?
            .provider_credential_bindings
            .into_iter()
            .find(|binding| binding.provider_kind == provider))
    }

    pub fn read_global_settings(&self) -> Result<GlobalSettingsFile, BackendError> {
        let global_path = self.global_settings_path();
        if !global_path.exists() {
            return Ok(GlobalSettingsFile::default());
        }
        FileStore.read_json_file(&global_path)
    }

    pub fn read_close_behavior(&self) -> CloseBehavior {
        self.read_global_settings()
            .map(|settings| settings.close_behavior)
            .unwrap_or_default()
    }

    pub fn read_global_ui_preferences(&self) -> Result<GlobalUiPreferences, BackendError> {
        let settings = self.read_global_settings()?;
        Ok(GlobalUiPreferences {
            language: settings.language,
            theme: settings.theme,
        })
    }

    pub fn save_global_ui_preferences(
        &self,
        preferences: GlobalUiPreferences,
    ) -> Result<GlobalUiPreferences, BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        settings.language = preferences.language;
        settings.theme = preferences.theme;
        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &settings,
        )?;
        Ok(GlobalUiPreferences {
            language: settings.language,
            theme: settings.theme,
        })
    }

    pub fn is_remote_provider_disclosure_acknowledged(
        &self,
        revision: &str,
    ) -> Result<bool, BackendError> {
        Ok(self
            .read_global_settings()?
            .remote_provider_disclosure_revision
            .as_deref()
            == Some(revision))
    }

    pub fn acknowledge_remote_provider_disclosure(
        &self,
        revision: &str,
    ) -> Result<(), BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        settings.remote_provider_disclosure_revision = Some(revision.to_string());
        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &settings,
        )
    }

    /// Read the user's UI/content language preference from global settings.
    /// Used by the tray menu (no project context available at tray-build
    /// time) and anywhere else that needs the language without a project.
    pub fn read_language(&self) -> String {
        self.read_global_settings()
            .map(|settings| settings.language)
            .unwrap_or_else(|_| "en".to_string())
    }

    pub fn get_chat_convenience_authorization(
        &self,
        context: &ProjectContext,
    ) -> Result<ChatConvenienceAuthorization, BackendError> {
        let root_path_fingerprint = project_root_fingerprint(&context.root);
        let settings = self.read_global_settings()?;

        Ok(settings
            .chat_convenience_authorizations
            .into_iter()
            .rev()
            .find(|authorization| {
                authorization.project_id == context.project_id
                    && authorization.root_path_fingerprint == root_path_fingerprint
            })
            .unwrap_or(ChatConvenienceAuthorization {
                enabled: false,
                confirmed_at: String::new(),
                project_id: context.project_id.clone(),
                root_path_fingerprint,
            }))
    }

    pub fn set_chat_convenience_authorization(
        &self,
        context: &ProjectContext,
        enabled: bool,
    ) -> Result<ChatConvenienceAuthorization, BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        let root_path_fingerprint = project_root_fingerprint(&context.root);
        settings
            .chat_convenience_authorizations
            .retain(|authorization| {
                authorization.project_id != context.project_id
                    || authorization.root_path_fingerprint != root_path_fingerprint
            });

        let authorization = if enabled {
            ChatConvenienceAuthorization {
                enabled,
                confirmed_at: chrono::Utc::now().to_rfc3339(),
                project_id: context.project_id.clone(),
                root_path_fingerprint,
            }
        } else {
            ChatConvenienceAuthorization {
                enabled: false,
                confirmed_at: String::new(),
                project_id: context.project_id.clone(),
                root_path_fingerprint,
            }
        };
        if enabled {
            settings
                .chat_convenience_authorizations
                .push(authorization.clone());
        }

        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &settings,
        )?;

        Ok(authorization)
    }

    pub fn revoke_all_chat_convenience_authorizations(&self) -> Result<(), BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        settings.chat_convenience_authorizations.clear();
        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &settings,
        )
    }

    pub fn record_agent_lint_repair_attestation(
        &self,
        canonical_identity_key: &str,
        identity_revision: &str,
        task_id: &str,
        operation_digest: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        if let Some(existing) = settings
            .agent_lint_repair_attestations
            .iter()
            .find(|item| item.task_id == task_id)
        {
            if existing.canonical_identity_key == canonical_identity_key
                && existing.identity_revision == identity_revision
                && existing.operation_digest == operation_digest
                && existing.lifecycle == AgentLintRepairAttestationLifecycle::QueuedAuthorized
            {
                return Ok(existing.clone());
            }
            return Err(BackendError::new(
                "LINT_REPAIR_ATTESTATION_STATE_INVALID",
                "The Agent lint repair approval was cancelled or changed before it could be authorized.",
                true,
                true,
            ));
        }
        let attestation = AgentLintRepairAttestation {
            canonical_identity_key: canonical_identity_key.to_string(),
            identity_revision: identity_revision.to_string(),
            task_id: task_id.to_string(),
            operation_digest: operation_digest.to_string(),
            confirmed_at: chrono::Utc::now().to_rfc3339(),
            lifecycle: AgentLintRepairAttestationLifecycle::QueuedAuthorized,
            descriptor_digest: None,
            mutation_journal: None,
            terminal_result_digest: None,
            terminal_result_json: None,
            terminal_task_status: None,
        };
        settings
            .agent_lint_repair_attestations
            .push(attestation.clone());
        prune_agent_lint_repair_attestations(
            &mut settings.agent_lint_repair_attestations,
            task_id,
        )?;
        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &settings,
        )?;
        Ok(attestation)
    }

    /// Persist a fail-closed cancellation receipt for an exact repair run.
    ///
    /// This also creates a tombstone when initial confirmation has created the
    /// held task but has not written its authorization receipt yet. A later
    /// authorization attempt therefore cannot resurrect a cancellation that
    /// won the race.
    pub fn cancel_agent_lint_repair_attestation(
        &self,
        canonical_identity_key: &str,
        identity_revision: &str,
        task_id: &str,
        operation_digest: &str,
    ) -> Result<(), BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        if let Some(existing) = settings
            .agent_lint_repair_attestations
            .iter_mut()
            .find(|item| item.task_id == task_id)
        {
            if existing.canonical_identity_key != canonical_identity_key
                || existing.identity_revision != identity_revision
                || existing.operation_digest != operation_digest
            {
                return Err(BackendError::new(
                    "LINT_REPAIR_ATTESTATION_REQUIRED",
                    "The Agent lint repair cancellation does not match its app-owned approval attestation.",
                    true,
                    true,
                ));
            }
            if existing.lifecycle == AgentLintRepairAttestationLifecycle::Completed {
                return Err(BackendError::new(
                    "LINT_REPAIR_ATTESTATION_STATE_INVALID",
                    "A completed Agent lint repair cannot be cancelled.",
                    true,
                    true,
                ));
            }
            existing.lifecycle = AgentLintRepairAttestationLifecycle::Cancelled;
            existing.confirmed_at = chrono::Utc::now().to_rfc3339();
        } else {
            settings
                .agent_lint_repair_attestations
                .push(AgentLintRepairAttestation {
                    canonical_identity_key: canonical_identity_key.to_string(),
                    identity_revision: identity_revision.to_string(),
                    task_id: task_id.to_string(),
                    operation_digest: operation_digest.to_string(),
                    confirmed_at: chrono::Utc::now().to_rfc3339(),
                    lifecycle: AgentLintRepairAttestationLifecycle::Cancelled,
                    descriptor_digest: None,
                    mutation_journal: None,
                    terminal_result_digest: None,
                    terminal_result_json: None,
                    terminal_task_status: None,
                });
            prune_agent_lint_repair_attestations(
                &mut settings.agent_lint_repair_attestations,
                task_id,
            )?;
        }
        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &settings,
        )
    }

    pub fn has_agent_lint_repair_attestation(
        &self,
        canonical_identity_key: &str,
        identity_revision: &str,
        task_id: &str,
        operation_digest: &str,
        allowed_lifecycles: &[AgentLintRepairAttestationLifecycle],
    ) -> Result<bool, BackendError> {
        Ok(self
            .read_global_settings()?
            .agent_lint_repair_attestations
            .iter()
            .any(|item| {
                item.canonical_identity_key == canonical_identity_key
                    && item.identity_revision == identity_revision
                    && item.task_id == task_id
                    && item.operation_digest == operation_digest
                    && allowed_lifecycles.contains(&item.lifecycle)
            }))
    }

    pub fn transition_agent_lint_repair_attestation(
        &self,
        task_id: &str,
        operation_digest: &str,
        from: &[AgentLintRepairAttestationLifecycle],
        to: AgentLintRepairAttestationLifecycle,
    ) -> Result<(), BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        let item = settings
            .agent_lint_repair_attestations
            .iter_mut()
            .find(|item| item.task_id == task_id && item.operation_digest == operation_digest)
            .ok_or_else(|| {
                BackendError::new(
                    "LINT_REPAIR_ATTESTATION_REQUIRED",
                    "The Agent lint repair has no exact app-owned approval attestation.",
                    true,
                    true,
                )
            })?;
        if item.lifecycle == AgentLintRepairAttestationLifecycle::Completed
            || to == AgentLintRepairAttestationLifecycle::Completed
            || !from.contains(&item.lifecycle)
            || (item.lifecycle == AgentLintRepairAttestationLifecycle::Cancelled
                && to == AgentLintRepairAttestationLifecycle::QueuedAuthorized
                && (item.descriptor_digest.is_some()
                    || item.mutation_journal.is_some()
                    || item.terminal_result_digest.is_some()))
        {
            return Err(BackendError::new(
                "LINT_REPAIR_ATTESTATION_STATE_INVALID",
                "The Agent lint repair approval is no longer dispatchable.",
                true,
                true,
            ));
        }
        item.lifecycle = to;
        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &settings,
        )
    }

    pub fn get_agent_lint_repair_attestation(
        &self,
        canonical_identity_key: &str,
        identity_revision: &str,
        task_id: &str,
        operation_digest: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        let _guard = self.lock_global_settings()?;
        self.read_global_settings()?
            .agent_lint_repair_attestations
            .into_iter()
            .find(|item| {
                item.canonical_identity_key == canonical_identity_key
                    && item.identity_revision == identity_revision
                    && item.task_id == task_id
                    && item.operation_digest == operation_digest
            })
            .ok_or_else(agent_lint_repair_attestation_required)
    }

    /// Bind a project-owned descriptor by exact compare-and-swap. This is the
    /// only supported way to advance the trusted descriptor lineage.
    pub fn bind_agent_lint_repair_descriptor_digest(
        &self,
        task_id: &str,
        operation_digest: &str,
        expected_descriptor_digest: Option<&str>,
        descriptor_digest: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        require_non_empty_repair_receipt_value(descriptor_digest)?;
        self.update_agent_lint_repair_attestation(task_id, operation_digest, |item| {
            require_dispatched_repair_attestation(item)?;
            if item.descriptor_digest.as_deref() != expected_descriptor_digest
                || item
                    .mutation_journal
                    .as_ref()
                    .and_then(|journal| journal.final_commit.as_ref())
                    .is_some()
            {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            item.descriptor_digest = Some(descriptor_digest.to_string());
            Ok(())
        })
    }

    /// Write or advance a durable pre-mutation journal bound to the exact
    /// descriptor and the run's initial checkpoint. Applying rounds retain one
    /// cumulative journal until terminal completion or verified rollback.
    pub fn begin_agent_lint_repair_mutation_journal(
        &self,
        task_id: &str,
        operation_digest: &str,
        descriptor_digest: &str,
        checkpoint_hash: &str,
        affected_path_hashes: BTreeMap<String, Option<String>>,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        self.begin_agent_lint_repair_mutation_journal_with_pre_hashes(
            task_id,
            operation_digest,
            descriptor_digest,
            checkpoint_hash,
            affected_path_hashes.clone(),
            affected_path_hashes,
        )
    }

    pub fn begin_agent_lint_repair_mutation_journal_with_pre_hashes(
        &self,
        task_id: &str,
        operation_digest: &str,
        descriptor_digest: &str,
        checkpoint_hash: &str,
        pre_mutation_path_hashes: BTreeMap<String, Option<String>>,
        affected_path_hashes: BTreeMap<String, Option<String>>,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        require_valid_repair_mutation_journal_inputs(
            descriptor_digest,
            checkpoint_hash,
            &affected_path_hashes,
        )?;
        if pre_mutation_path_hashes
            .keys()
            .ne(affected_path_hashes.keys())
        {
            return Err(agent_lint_repair_attestation_state_invalid());
        }
        let next = AgentLintRepairMutationJournal {
            phase: AgentLintRepairMutationPhase::Applying,
            checkpoint_hash: checkpoint_hash.to_string(),
            pre_mutation_path_hashes,
            affected_path_hashes,
            final_commit: None,
        };
        self.update_agent_lint_repair_attestation(task_id, operation_digest, |item| {
            require_dispatched_repair_attestation(item)?;
            if item.descriptor_digest.as_deref() != Some(descriptor_digest) {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            match item.mutation_journal.as_ref() {
                None => item.mutation_journal = Some(next),
                Some(existing)
                    if existing.phase == AgentLintRepairMutationPhase::Applying
                        && existing.checkpoint_hash == checkpoint_hash
                        && existing.final_commit.is_none()
                        && existing
                            .affected_path_hashes
                            .keys()
                            .all(|path| next.affected_path_hashes.contains_key(path)) =>
                {
                    item.mutation_journal = Some(next);
                }
                Some(_) => return Err(agent_lint_repair_attestation_state_invalid()),
            }
            Ok(())
        })
    }

    pub fn mark_agent_lint_repair_mutation_finalizing(
        &self,
        task_id: &str,
        operation_digest: &str,
        descriptor_digest: &str,
        checkpoint_hash: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        require_non_empty_repair_receipt_value(descriptor_digest)?;
        require_non_empty_repair_receipt_value(checkpoint_hash)?;
        self.update_agent_lint_repair_attestation(task_id, operation_digest, |item| {
            require_dispatched_repair_attestation(item)?;
            if item.descriptor_digest.as_deref() != Some(descriptor_digest) {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            let journal = item
                .mutation_journal
                .as_mut()
                .ok_or_else(agent_lint_repair_attestation_state_invalid)?;
            if journal.checkpoint_hash != checkpoint_hash {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            journal.phase = AgentLintRepairMutationPhase::Finalizing;
            Ok(())
        })
    }

    /// Bind the exact final commit while retaining the journal until the
    /// terminal result receipt is durable.
    pub fn mark_agent_lint_repair_final_commit(
        &self,
        task_id: &str,
        operation_digest: &str,
        descriptor_digest: &str,
        checkpoint_hash: &str,
        final_commit: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        require_non_empty_repair_receipt_value(final_commit)?;
        self.update_agent_lint_repair_attestation(task_id, operation_digest, |item| {
            require_dispatched_repair_attestation(item)?;
            if item.descriptor_digest.as_deref() != Some(descriptor_digest) {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            let journal = item
                .mutation_journal
                .as_mut()
                .ok_or_else(agent_lint_repair_attestation_state_invalid)?;
            if journal.phase != AgentLintRepairMutationPhase::Finalizing
                || journal.checkpoint_hash != checkpoint_hash
                || journal
                    .final_commit
                    .as_deref()
                    .is_some_and(|existing| existing != final_commit)
            {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            journal.final_commit = Some(final_commit.to_string());
            Ok(())
        })
    }

    pub fn clear_agent_lint_repair_mutation_journal(
        &self,
        task_id: &str,
        operation_digest: &str,
        descriptor_digest: &str,
        checkpoint_hash: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        self.update_agent_lint_repair_attestation(task_id, operation_digest, |item| {
            let lifecycle = item.lifecycle;
            if !matches!(
                item.lifecycle,
                AgentLintRepairAttestationLifecycle::Dispatched
                    | AgentLintRepairAttestationLifecycle::Cancelled
            ) || item.terminal_result_digest.is_some()
                || item.descriptor_digest.as_deref() != Some(descriptor_digest)
            {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            let journal = item
                .mutation_journal
                .as_ref()
                .ok_or_else(agent_lint_repair_attestation_state_invalid)?;
            if journal.checkpoint_hash != checkpoint_hash
                || (lifecycle == AgentLintRepairAttestationLifecycle::Dispatched
                    && journal.final_commit.is_some())
            {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            item.mutation_journal = None;
            Ok(())
        })
    }

    /// Consume a finalizing WAL only after the Git owner has created an exact
    /// compensating rollback from the recorded final commit to its checkpoint.
    pub fn clear_agent_lint_repair_journal_after_compensating_rollback(
        &self,
        task_id: &str,
        operation_digest: &str,
        descriptor_digest: &str,
        checkpoint_hash: &str,
        final_commit: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        self.update_agent_lint_repair_attestation(task_id, operation_digest, |item| {
            if !matches!(
                item.lifecycle,
                AgentLintRepairAttestationLifecycle::Dispatched
                    | AgentLintRepairAttestationLifecycle::Cancelled
            ) || item.terminal_result_digest.is_some()
                || item.descriptor_digest.as_deref() != Some(descriptor_digest)
            {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            let journal = item
                .mutation_journal
                .as_ref()
                .ok_or_else(agent_lint_repair_attestation_state_invalid)?;
            if journal.phase != AgentLintRepairMutationPhase::Finalizing
                || journal.checkpoint_hash != checkpoint_hash
                || journal.final_commit.as_deref() != Some(final_commit)
            {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            item.mutation_journal = None;
            Ok(())
        })
    }

    /// Make a terminal result authoritative. Exact retries are idempotent;
    /// neither a descriptor nor a terminal digest can be rewritten afterward.
    pub fn complete_agent_lint_repair_attestation(
        &self,
        task_id: &str,
        operation_digest: &str,
        expected_descriptor_digest: Option<&str>,
        terminal_result_digest: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        self.complete_agent_lint_repair_attestation_inner(
            task_id,
            operation_digest,
            expected_descriptor_digest,
            terminal_result_digest,
            None,
            None,
            true,
        )
    }

    pub fn complete_agent_lint_repair_success_attestation(
        &self,
        task_id: &str,
        operation_digest: &str,
        expected_descriptor_digest: Option<&str>,
        terminal_result_digest: &str,
        terminal_result_json: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        self.complete_agent_lint_repair_attestation_inner(
            task_id,
            operation_digest,
            expected_descriptor_digest,
            terminal_result_digest,
            Some(terminal_result_json),
            Some("succeeded"),
            false,
        )
    }

    pub fn complete_agent_lint_repair_terminal_attestation(
        &self,
        task_id: &str,
        operation_digest: &str,
        expected_descriptor_digest: Option<&str>,
        terminal_result_digest: &str,
        terminal_result_json: &str,
        terminal_task_status: &str,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        self.complete_agent_lint_repair_attestation_inner(
            task_id,
            operation_digest,
            expected_descriptor_digest,
            terminal_result_digest,
            Some(terminal_result_json),
            Some(terminal_task_status),
            true,
        )
    }

    fn complete_agent_lint_repair_attestation_inner(
        &self,
        task_id: &str,
        operation_digest: &str,
        expected_descriptor_digest: Option<&str>,
        terminal_result_digest: &str,
        terminal_result_json: Option<&str>,
        terminal_task_status: Option<&str>,
        allow_cancelled: bool,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        require_non_empty_repair_receipt_value(terminal_result_digest)?;
        if terminal_result_json.is_some_and(str::is_empty) {
            return Err(agent_lint_repair_attestation_state_invalid());
        }
        if terminal_task_status.is_some_and(|status| {
            !matches!(status, "succeeded" | "failed" | "cancelled" | "interrupted")
        }) {
            return Err(agent_lint_repair_attestation_state_invalid());
        }
        self.update_agent_lint_repair_attestation(task_id, operation_digest, |item| {
            if item.lifecycle == AgentLintRepairAttestationLifecycle::Completed {
                if item.descriptor_digest.as_deref() == expected_descriptor_digest
                    && item.terminal_result_digest.as_deref() == Some(terminal_result_digest)
                    && (terminal_result_json.is_none()
                        || item.terminal_result_json.as_deref() == terminal_result_json)
                    && (terminal_task_status.is_none()
                        || item.terminal_task_status.as_deref() == terminal_task_status)
                {
                    return Ok(());
                }
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            if item.lifecycle != AgentLintRepairAttestationLifecycle::Dispatched
                && !(allow_cancelled
                    && item.lifecycle == AgentLintRepairAttestationLifecycle::Cancelled)
            {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            if item.descriptor_digest.as_deref() != expected_descriptor_digest
                || item.mutation_journal.as_ref().is_some_and(|journal| {
                    journal.phase != AgentLintRepairMutationPhase::Finalizing
                        || journal.final_commit.is_none()
                })
            {
                return Err(agent_lint_repair_attestation_state_invalid());
            }
            item.lifecycle = AgentLintRepairAttestationLifecycle::Completed;
            item.terminal_result_digest = Some(terminal_result_digest.to_string());
            item.terminal_result_json = terminal_result_json.map(str::to_string);
            item.terminal_task_status = terminal_task_status.map(str::to_string);
            item.mutation_journal = None;
            Ok(())
        })
    }

    fn update_agent_lint_repair_attestation(
        &self,
        task_id: &str,
        operation_digest: &str,
        update: impl FnOnce(&mut AgentLintRepairAttestation) -> Result<(), BackendError>,
    ) -> Result<AgentLintRepairAttestation, BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        let item = settings
            .agent_lint_repair_attestations
            .iter_mut()
            .find(|item| item.task_id == task_id && item.operation_digest == operation_digest)
            .ok_or_else(agent_lint_repair_attestation_required)?;
        update(item)?;
        let updated = item.clone();
        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &settings,
        )?;
        Ok(updated)
    }

    pub fn revoke_agent_lint_repair_attestation(&self, task_id: &str) -> Result<(), BackendError> {
        let _guard = self.lock_global_settings()?;
        let mut settings = self.read_global_settings()?;
        if settings.agent_lint_repair_attestations.iter().any(|item| {
            item.task_id == task_id
                && (item.lifecycle == AgentLintRepairAttestationLifecycle::Completed
                    || item.mutation_journal.is_some())
        }) {
            return Err(agent_lint_repair_attestation_state_invalid());
        }
        let old_len = settings.agent_lint_repair_attestations.len();
        settings
            .agent_lint_repair_attestations
            .retain(|item| item.task_id != task_id);
        if old_len == settings.agent_lint_repair_attestations.len() {
            return Ok(());
        }
        let store = FileStore;
        store.ensure_absolute_dir(self.config_write_root(), &self.config_dir)?;
        store.write_json_atomic_absolute(
            self.config_write_root(),
            &self.global_settings_path(),
            &settings,
        )
    }

    fn global_settings_path(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    fn config_write_root(&self) -> &Path {
        self.config_dir.parent().unwrap_or(&self.config_dir)
    }

    fn lock_global_settings(&self) -> Result<std::sync::MutexGuard<'_, ()>, BackendError> {
        global_settings_lock().lock().map_err(|_| {
            BackendError::new(
                "SETTINGS_LOCKED",
                "Settings are currently unavailable.",
                true,
                false,
            )
        })
    }
}

fn require_dispatched_repair_attestation(
    item: &AgentLintRepairAttestation,
) -> Result<(), BackendError> {
    if item.lifecycle != AgentLintRepairAttestationLifecycle::Dispatched
        || item.terminal_result_digest.is_some()
    {
        return Err(agent_lint_repair_attestation_state_invalid());
    }
    Ok(())
}

fn require_valid_repair_mutation_journal_inputs(
    descriptor_digest: &str,
    checkpoint_hash: &str,
    affected_path_hashes: &BTreeMap<String, Option<String>>,
) -> Result<(), BackendError> {
    require_non_empty_repair_receipt_value(descriptor_digest)?;
    require_non_empty_repair_receipt_value(checkpoint_hash)?;
    if affected_path_hashes.is_empty()
        || affected_path_hashes.len() > MAX_AGENT_LINT_REPAIR_JOURNAL_PATHS
        || affected_path_hashes.iter().any(|(path, digest)| {
            path.trim().is_empty()
                || digest
                    .as_deref()
                    .is_some_and(|value| value.trim().is_empty())
        })
    {
        return Err(agent_lint_repair_attestation_state_invalid());
    }
    Ok(())
}

fn require_non_empty_repair_receipt_value(value: &str) -> Result<(), BackendError> {
    if value.trim().is_empty() {
        return Err(agent_lint_repair_attestation_state_invalid());
    }
    Ok(())
}

fn agent_lint_repair_attestation_required() -> BackendError {
    BackendError::new(
        "LINT_REPAIR_ATTESTATION_REQUIRED",
        "The Agent lint repair has no exact app-owned approval attestation.",
        true,
        true,
    )
}

fn agent_lint_repair_attestation_state_invalid() -> BackendError {
    BackendError::new(
        "LINT_REPAIR_ATTESTATION_STATE_INVALID",
        "The Agent lint repair app-owned receipt no longer matches this transition.",
        true,
        true,
    )
}

fn prune_agent_lint_repair_attestations(
    attestations: &mut Vec<AgentLintRepairAttestation>,
    protected_task_id: &str,
) -> Result<(), BackendError> {
    let retirement_cutoff =
        chrono::Utc::now() - chrono::Duration::minutes(AGENT_LINT_REPAIR_CANCEL_TOMBSTONE_MINUTES);
    attestations.retain(|item| {
        if item.task_id == protected_task_id
            || item.lifecycle != AgentLintRepairAttestationLifecycle::Cancelled
        {
            return true;
        }
        item.confirmed_at
            .parse::<chrono::DateTime<chrono::Utc>>()
            .map(|cancelled_at| cancelled_at > retirement_cutoff)
            .unwrap_or(true)
    });
    while attestations.len() > MAX_AGENT_LINT_REPAIR_ATTESTATIONS {
        let Some(index) = attestations.iter().position(|item| {
            item.task_id != protected_task_id
                && matches!(
                    item.lifecycle,
                    AgentLintRepairAttestationLifecycle::QueuedAuthorized
                        | AgentLintRepairAttestationLifecycle::Completed
                )
                && item.mutation_journal.is_none()
        }) else {
            return Err(BackendError::new(
                "LINT_REPAIR_ATTESTATION_CAPACITY_REACHED",
                "Active or cancelled Agent lint repair receipts fill the bounded app-owned receipt store.",
                true,
                true,
            ));
        };
        attestations.remove(index);
    }
    Ok(())
}

fn project_state_path<'a>(
    _context: &ProjectContext,
    path: Option<&'a str>,
    feature: &str,
) -> Result<&'a str, BackendError> {
    path.ok_or_else(|| {
        BackendError::new(
            "PROJECT_LAYOUT_STATE_UNAVAILABLE",
            format!(
                "Project {feature} state is unavailable until compatible features are enabled."
            ),
            true,
            true,
        )
    })
}

fn global_settings_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn default_config_dir() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("llm-wiki-desktop");
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("llm-wiki-desktop");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("llm-wiki-desktop");
    }
    std::env::temp_dir().join("llm-wiki-desktop")
}

fn project_root_fingerprint(path: &Path) -> String {
    use sha2::{Digest, Sha256};

    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let digest = Sha256::digest(normalized.as_bytes());
    digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use serde_json::Value;

    use super::SettingsService;
    use crate::models::agent::{AgentConfig, AgentKind};
    use crate::models::llm::{LlmProviderConfig, LlmProviderKind, ProviderCredentialBinding};
    use crate::models::paths::ProjectContext;
    use crate::models::settings::{
        AgentLintRepairAttestationLifecycle, AgentLintRepairMutationPhase,
    };
    use crate::models::settings::{
        CloseBehavior, GlobalSettingsFile, GlobalUiPreferences, Settings, ThemePreference,
    };
    use crate::services::{AgentService, FileStore, SecretService};

    fn tmp_paths(suffix: &str) -> (ProjectContext, PathBuf, PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("llm-wiki-settings-{stamp}-{suffix}"));
        let config =
            std::env::temp_dir().join(format!("llm-wiki-settings-config-{stamp}-{suffix}"));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        (ProjectContext::new("project-1", root.clone()), root, config)
    }

    #[test]
    fn reads_defaults_when_project_and_global_settings_are_missing() {
        let (context, root, config_dir) = tmp_paths("defaults");
        let service = SettingsService::with_config_dir(config_dir.clone());

        let settings = service.read_settings(&context).unwrap();

        assert_eq!(settings.language, "en");
        assert_eq!(settings.theme.as_str(), "auto");
        assert_eq!(settings.close_behavior.as_str(), "minimize_to_tray");
        assert!(settings.check_updates);

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn saves_project_settings_without_secrets_and_keeps_global_settings_separate() {
        let (context, root, config_dir) = tmp_paths("save");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let secrets = SecretService::memory();
        secrets
            .set(LlmProviderKind::OpenAi, "sk-secret-1234")
            .unwrap();

        let mut settings = service.read_settings(&context).unwrap();
        settings.language = "zh-CN".into();
        settings.check_updates = false;
        settings.agent_default = Some(AgentKind::Codex);
        let provider = LlmProviderConfig {
            provider: LlmProviderKind::OpenAi,
            model: "gpt-4.1".into(),
            base_url: "https://api.openai.com".into(),
            context_window: 128_000,
            enabled: true,
        };

        service.save_settings(&context, &settings).unwrap();
        let config_id = uuid::Uuid::new_v4().to_string();
        service
            .save_provider_with_binding(
                &context,
                provider,
                ProviderCredentialBinding {
                    credential_account_id: SecretService::provider_binding_account_id(
                        &context,
                        LlmProviderKind::OpenAi,
                        &config_id,
                        "https://api.openai.com",
                        1,
                    )
                    .unwrap(),
                    config_id,
                    provider_kind: LlmProviderKind::OpenAi,
                    canonical_origin: "https://api.openai.com".into(),
                    approved_at: None,
                    revision: 1,
                },
            )
            .unwrap();

        let project_value: Value = FileStore.read_json(&context, ".app/settings.json").unwrap();
        let global_value: Value = FileStore
            .read_json_file(&config_dir.join("settings.json"))
            .unwrap();

        assert!(project_value.get("llmProviders").is_some());
        assert!(project_value.get("agentDefault").is_some());
        assert!(project_value.get("language").is_none());
        assert!(project_value.get("theme").is_none());
        assert!(project_value.to_string().contains("gpt-4.1"));
        assert!(!project_value.to_string().contains("sk-secret-1234"));
        assert_eq!(global_value["language"], "zh-CN");
        assert_eq!(global_value["checkUpdates"], false);

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn generic_settings_save_cannot_replace_backend_provider_state() {
        let (context, root, config_dir) = tmp_paths("protected-provider-binding");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let provider = LlmProviderConfig {
            provider: LlmProviderKind::Custom,
            model: "model".into(),
            base_url: "https://provider.example".into(),
            context_window: 8_000,
            enabled: true,
        };
        let binding = ProviderCredentialBinding {
            config_id: uuid::Uuid::new_v4().to_string(),
            provider_kind: LlmProviderKind::Custom,
            canonical_origin: "https://provider.example".into(),
            credential_account_id: "backend-owned-account".into(),
            approved_at: None,
            revision: 3,
        };
        service
            .save_provider_with_binding(&context, provider.clone(), binding.clone())
            .unwrap();

        let mut submitted = service.read_settings(&context).unwrap();
        submitted.llm_providers[0].base_url = "https://attacker.example".into();
        submitted.provider_credential_bindings[0].config_id = "attacker-controlled".into();
        submitted.provider_credential_bindings[0].canonical_origin =
            "https://attacker.example".into();
        service.save_settings(&context, &submitted).unwrap();

        assert_eq!(
            service
                .provider_credential_binding(&context, LlmProviderKind::Custom)
                .unwrap(),
            Some(binding)
        );
        assert_eq!(service.list_providers(&context).unwrap(), vec![provider]);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn agent_config_is_canonical_when_legacy_settings_disagree() {
        let (context, root, config_dir) = tmp_paths("agent-canonical");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let mut settings = service.read_settings(&context).unwrap();
        settings.agent_default = Some(AgentKind::Claude);
        service.save_settings(&context, &settings).unwrap();
        AgentService::save_config(
            &context,
            &AgentConfig {
                default_agent: Some(AgentKind::Codex),
            },
        )
        .unwrap();

        let reloaded = service.read_settings(&context).unwrap();

        assert_eq!(reloaded.agent_default, Some(AgentKind::Codex));
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn save_agent_default_synchronizes_project_files() {
        let (context, root, config_dir) = tmp_paths("agent-save");
        let service = SettingsService::with_config_dir(config_dir.clone());

        let saved = service
            .save_agent_default(&context, Some(AgentKind::Codex))
            .unwrap();

        assert_eq!(saved.default_agent, Some(AgentKind::Codex));
        assert_eq!(
            service.read_settings(&context).unwrap().agent_default,
            Some(AgentKind::Codex)
        );
        let project: Value = FileStore.read_json(&context, ".app/settings.json").unwrap();
        assert_eq!(project["agentDefault"], "codex");
        assert_eq!(
            AgentService::load_config(&context).unwrap().default_agent,
            Some(AgentKind::Codex)
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn reads_close_behavior_from_global_settings() {
        let (_context, _root, config_dir) = tmp_paths("close-behavior");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let store = FileStore;
        store
            .ensure_absolute_dir(config_dir.parent().unwrap(), &config_dir)
            .unwrap();
        store
            .write_json_atomic_absolute(
                config_dir.parent().unwrap(),
                &config_dir.join("settings.json"),
                &GlobalSettingsFile {
                    close_behavior: CloseBehavior::Quit,
                    ..GlobalSettingsFile::default()
                },
            )
            .unwrap();

        assert_eq!(service.read_close_behavior(), CloseBehavior::Quit);

        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn read_language_returns_global_preference_or_english_default() {
        // The tray menu reads language through read_language() with no project
        // context, so it must reflect the global settings file.
        let (_context, _root, config_dir) = tmp_paths("language");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let store = FileStore;
        store
            .ensure_absolute_dir(config_dir.parent().unwrap(), &config_dir)
            .unwrap();
        store
            .write_json_atomic_absolute(
                config_dir.parent().unwrap(),
                &config_dir.join("settings.json"),
                &GlobalSettingsFile {
                    language: "zh-CN".into(),
                    ..GlobalSettingsFile::default()
                },
            )
            .unwrap();

        assert_eq!(service.read_language(), "zh-CN");

        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn read_language_defaults_to_english_when_settings_missing() {
        let (_context, _root, config_dir) = tmp_paths("language-default");
        let service = SettingsService::with_config_dir(config_dir.clone());
        // No settings file written.
        assert_eq!(service.read_language(), "en");
    }

    #[test]
    fn chat_convenience_authorization_is_global_only() {
        let (context, root, config_dir) = tmp_paths("chat-convenience-auth");
        let service = SettingsService::with_config_dir(config_dir.clone());

        let saved = service
            .set_chat_convenience_authorization(&context, true)
            .unwrap();

        assert!(saved.enabled);
        assert_eq!(saved.project_id, context.project_id);
        assert!(saved.root_path_fingerprint.len() >= 16);

        let global: serde_json::Value = FileStore
            .read_json_file(&config_dir.join("settings.json"))
            .unwrap();
        assert!(global["chatConvenienceAuthorizations"].is_array());
        assert!(!context
            .resolve_project_path(".app/settings.json")
            .unwrap()
            .exists());

        let loaded = service
            .get_chat_convenience_authorization(&context)
            .unwrap();
        assert!(loaded.enabled);

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn chat_convenience_authorization_can_be_revoked_for_project() {
        let (context, root, config_dir) = tmp_paths("chat-convenience-revoke");
        let service = SettingsService::with_config_dir(config_dir.clone());

        service
            .set_chat_convenience_authorization(&context, true)
            .unwrap();
        let revoked = service
            .set_chat_convenience_authorization(&context, false)
            .unwrap();

        assert!(!revoked.enabled);
        assert!(
            !service
                .get_chat_convenience_authorization(&context)
                .unwrap()
                .enabled
        );
        let global: serde_json::Value = FileStore
            .read_json_file(&config_dir.join("settings.json"))
            .unwrap();
        assert_eq!(
            global["chatConvenienceAuthorizations"]
                .as_array()
                .unwrap()
                .len(),
            0
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn save_settings_preserves_chat_convenience_authorizations() {
        let (context, root, config_dir) = tmp_paths("chat-convenience-save-preserves");
        let service = SettingsService::with_config_dir(config_dir.clone());

        service
            .set_chat_convenience_authorization(&context, true)
            .unwrap();
        let mut settings = service.read_settings(&context).unwrap();
        settings.language = "zh-CN".into();

        service.save_settings(&context, &settings).unwrap();

        assert!(
            service
                .get_chat_convenience_authorization(&context)
                .unwrap()
                .enabled
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn repair_attestation_is_global_exact_and_survives_settings_save() {
        let (context, root, config_dir) = tmp_paths("repair-attestation");
        let service = SettingsService::with_config_dir(config_dir.clone());

        service
            .record_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        assert!(service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
            )
            .unwrap());
        assert!(!service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "forged-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
            )
            .unwrap());
        service
            .transition_agent_lint_repair_attestation(
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
                AgentLintRepairAttestationLifecycle::Dispatched,
            )
            .unwrap();
        assert!(!service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
            )
            .unwrap());
        assert!(service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::Dispatched],
            )
            .unwrap());

        service
            .save_settings(&context, &service.read_settings(&context).unwrap())
            .unwrap();
        assert!(service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::Dispatched],
            )
            .unwrap());
        let project: serde_json::Value =
            FileStore.read_json(&context, ".app/settings.json").unwrap();
        assert!(project.get("agentLintRepairAttestations").is_none());

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn repair_attestation_h4b_transitions_reject_exact_digest_mismatches() {
        let (_context, root, config_dir) = tmp_paths("repair-attestation-h4b-mismatch");
        let service = SettingsService::with_config_dir(config_dir.clone());
        service
            .record_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        service
            .transition_agent_lint_repair_attestation(
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
                AgentLintRepairAttestationLifecycle::Dispatched,
            )
            .unwrap();

        service
            .bind_agent_lint_repair_descriptor_digest(
                "task-1",
                "operation-digest",
                None,
                "descriptor-v1",
            )
            .unwrap();
        assert_eq!(
            service
                .bind_agent_lint_repair_descriptor_digest(
                    "task-1",
                    "operation-digest",
                    Some("forged-prior"),
                    "descriptor-v2",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        assert_eq!(
            service
                .begin_agent_lint_repair_mutation_journal(
                    "task-1",
                    "operation-digest",
                    "forged-descriptor",
                    "checkpoint-1",
                    BTreeMap::from([("wiki/page.md".into(), Some("post-hash".into()))]),
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );

        let exact = service
            .get_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        assert_eq!(exact.descriptor_digest.as_deref(), Some("descriptor-v1"));
        assert_eq!(exact.mutation_journal, None);

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn repair_mutation_journal_survives_a_new_settings_service_instance() {
        let (_context, root, config_dir) = tmp_paths("repair-attestation-h4b-journal");
        let service = SettingsService::with_config_dir(config_dir.clone());
        service
            .record_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        service
            .transition_agent_lint_repair_attestation(
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
                AgentLintRepairAttestationLifecycle::Dispatched,
            )
            .unwrap();
        service
            .bind_agent_lint_repair_descriptor_digest(
                "task-1",
                "operation-digest",
                None,
                "descriptor-v1",
            )
            .unwrap();
        service
            .begin_agent_lint_repair_mutation_journal_with_pre_hashes(
                "task-1",
                "operation-digest",
                "descriptor-v1",
                "checkpoint-1",
                BTreeMap::from([
                    ("wiki/deleted.md".into(), Some("deleted-before".into())),
                    ("wiki/page.md".into(), Some("pre-hash".into())),
                ]),
                BTreeMap::from([
                    ("wiki/deleted.md".into(), None),
                    ("wiki/page.md".into(), Some("post-hash".into())),
                ]),
            )
            .unwrap();
        service
            .begin_agent_lint_repair_mutation_journal(
                "task-1",
                "operation-digest",
                "descriptor-v1",
                "checkpoint-1",
                BTreeMap::from([
                    ("wiki/deleted.md".into(), None),
                    ("wiki/page.md".into(), Some("post-hash".into())),
                    ("wiki/round-two.md".into(), Some("round-two-hash".into())),
                ]),
            )
            .unwrap();
        assert_eq!(
            service
                .begin_agent_lint_repair_mutation_journal(
                    "task-1",
                    "operation-digest",
                    "descriptor-v1",
                    "checkpoint-1",
                    BTreeMap::from([("wiki/round-two.md".into(), Some("round-two-hash".into()),)]),
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        service
            .mark_agent_lint_repair_mutation_finalizing(
                "task-1",
                "operation-digest",
                "descriptor-v1",
                "checkpoint-1",
            )
            .unwrap();

        let reopened = SettingsService::with_config_dir(config_dir.clone());
        let receipt = reopened
            .get_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        let journal = receipt.mutation_journal.unwrap();
        assert_eq!(journal.checkpoint_hash, "checkpoint-1");
        assert_eq!(journal.phase, AgentLintRepairMutationPhase::Finalizing);
        assert_eq!(
            journal.pre_mutation_path_hashes["wiki/page.md"].as_deref(),
            Some("post-hash")
        );
        assert_eq!(journal.affected_path_hashes["wiki/deleted.md"], None);
        assert_eq!(
            journal.affected_path_hashes["wiki/page.md"].as_deref(),
            Some("post-hash")
        );
        assert_eq!(
            journal.affected_path_hashes["wiki/round-two.md"].as_deref(),
            Some("round-two-hash")
        );
        reopened
            .cancel_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        let cancelled = reopened
            .get_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        assert_eq!(
            cancelled.lifecycle,
            AgentLintRepairAttestationLifecycle::Cancelled
        );
        assert!(cancelled.mutation_journal.is_some());
        assert_eq!(
            reopened
                .bind_agent_lint_repair_descriptor_digest(
                    "task-1",
                    "operation-digest",
                    Some("descriptor-v1"),
                    "descriptor-v2",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        let recovered = reopened
            .clear_agent_lint_repair_mutation_journal(
                "task-1",
                "operation-digest",
                "descriptor-v1",
                "checkpoint-1",
            )
            .unwrap();
        assert_eq!(recovered.mutation_journal, None);
        assert_eq!(
            recovered.lifecycle,
            AgentLintRepairAttestationLifecycle::Cancelled
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn completed_repair_attestation_terminal_digest_cannot_be_rewritten() {
        let (_context, root, config_dir) = tmp_paths("repair-attestation-h4b-terminal");
        let service = SettingsService::with_config_dir(config_dir.clone());
        service
            .record_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        service
            .transition_agent_lint_repair_attestation(
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
                AgentLintRepairAttestationLifecycle::Dispatched,
            )
            .unwrap();
        service
            .bind_agent_lint_repair_descriptor_digest(
                "task-1",
                "operation-digest",
                None,
                "descriptor-v1",
            )
            .unwrap();
        service
            .begin_agent_lint_repair_mutation_journal(
                "task-1",
                "operation-digest",
                "descriptor-v1",
                "checkpoint-1",
                BTreeMap::from([("wiki/page.md".into(), Some("post-hash".into()))]),
            )
            .unwrap();
        assert_eq!(
            service
                .complete_agent_lint_repair_attestation(
                    "task-1",
                    "operation-digest",
                    Some("descriptor-v1"),
                    "terminal-v1",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        service
            .mark_agent_lint_repair_mutation_finalizing(
                "task-1",
                "operation-digest",
                "descriptor-v1",
                "checkpoint-1",
            )
            .unwrap();
        assert_eq!(
            service
                .complete_agent_lint_repair_attestation(
                    "task-1",
                    "operation-digest",
                    Some("descriptor-v1"),
                    "terminal-v1",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        service
            .mark_agent_lint_repair_final_commit(
                "task-1",
                "operation-digest",
                "descriptor-v1",
                "checkpoint-1",
                "final-commit-1",
            )
            .unwrap();
        assert_eq!(
            service
                .mark_agent_lint_repair_final_commit(
                    "task-1",
                    "operation-digest",
                    "descriptor-v1",
                    "checkpoint-1",
                    "forged-final-commit",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        let completed = service
            .complete_agent_lint_repair_attestation(
                "task-1",
                "operation-digest",
                Some("descriptor-v1"),
                "terminal-v1",
            )
            .unwrap();
        assert_eq!(
            completed.lifecycle,
            AgentLintRepairAttestationLifecycle::Completed
        );
        assert_eq!(
            completed.terminal_result_digest.as_deref(),
            Some("terminal-v1")
        );
        assert_eq!(completed.mutation_journal, None);
        assert_eq!(
            service
                .complete_agent_lint_repair_attestation(
                    "task-1",
                    "operation-digest",
                    Some("descriptor-v1"),
                    "terminal-v2",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        assert_eq!(
            service
                .revoke_agent_lint_repair_attestation("task-1")
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        assert_eq!(
            service
                .transition_agent_lint_repair_attestation(
                    "task-1",
                    "operation-digest",
                    &[AgentLintRepairAttestationLifecycle::Completed],
                    AgentLintRepairAttestationLifecycle::Cancelled,
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn cancelled_receipt_rejects_success_but_accepts_terminal_settlement() {
        let (_context, root, config_dir) = tmp_paths("repair-cancel-success-race");
        let service = SettingsService::with_config_dir(config_dir.clone());
        service
            .record_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        service
            .transition_agent_lint_repair_attestation(
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
                AgentLintRepairAttestationLifecycle::Dispatched,
            )
            .unwrap();
        service
            .cancel_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();

        assert_eq!(
            service
                .complete_agent_lint_repair_success_attestation(
                    "task-1",
                    "operation-digest",
                    None,
                    "success-digest",
                    "{\"outcome\":\"succeeded\"}",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        let completed = service
            .complete_agent_lint_repair_terminal_attestation(
                "task-1",
                "operation-digest",
                None,
                "cancelled-digest",
                "{\"outcome\":\"cancelled\"}",
                "cancelled",
            )
            .unwrap();
        assert_eq!(
            completed.lifecycle,
            AgentLintRepairAttestationLifecycle::Completed
        );
        assert_eq!(
            completed.terminal_result_json.as_deref(),
            Some("{\"outcome\":\"cancelled\"}")
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn repair_cancellation_tombstone_blocks_late_authorization_and_supports_exact_undo() {
        let (_context, root, config_dir) = tmp_paths("repair-cancel-tombstone");
        let service = SettingsService::with_config_dir(config_dir.clone());

        service
            .cancel_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
            )
            .unwrap();
        assert_eq!(
            service
                .record_agent_lint_repair_attestation(
                    "identity-key",
                    "identity-revision",
                    "task-1",
                    "operation-digest",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );
        assert!(service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::Cancelled],
            )
            .unwrap());

        service
            .transition_agent_lint_repair_attestation(
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::Cancelled],
                AgentLintRepairAttestationLifecycle::QueuedAuthorized,
            )
            .unwrap();
        assert!(service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "task-1",
                "operation-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
            )
            .unwrap());

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn repair_cancellation_tombstone_survives_bounded_receipt_churn() {
        let (_context, root, config_dir) = tmp_paths("repair-cancel-capacity");
        let service = SettingsService::with_config_dir(config_dir.clone());

        service
            .cancel_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "cancelled-task",
                "cancelled-digest",
            )
            .unwrap();
        for index in 0..super::MAX_AGENT_LINT_REPAIR_ATTESTATIONS {
            service
                .record_agent_lint_repair_attestation(
                    "identity-key",
                    "identity-revision",
                    &format!("task-{index}"),
                    &format!("digest-{index}"),
                )
                .unwrap();
        }

        assert!(service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "cancelled-task",
                "cancelled-digest",
                &[AgentLintRepairAttestationLifecycle::Cancelled],
            )
            .unwrap());
        assert_eq!(
            service
                .record_agent_lint_repair_attestation(
                    "identity-key",
                    "identity-revision",
                    "cancelled-task",
                    "cancelled-digest",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_STATE_INVALID"
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn fresh_cancelled_receipt_capacity_fails_closed_then_expired_receipts_retire() {
        let (_context, root, config_dir) = tmp_paths("repair-cancel-retirement");
        let service = SettingsService::with_config_dir(config_dir.clone());
        let mut settings = GlobalSettingsFile::default();
        let fresh = chrono::Utc::now().to_rfc3339();
        for index in 0..super::MAX_AGENT_LINT_REPAIR_ATTESTATIONS {
            settings.agent_lint_repair_attestations.push(
                crate::models::settings::AgentLintRepairAttestation {
                    canonical_identity_key: "identity-key".into(),
                    identity_revision: "identity-revision".into(),
                    task_id: format!("cancelled-{index}"),
                    operation_digest: format!("cancelled-digest-{index}"),
                    confirmed_at: fresh.clone(),
                    lifecycle: AgentLintRepairAttestationLifecycle::Cancelled,
                    descriptor_digest: None,
                    mutation_journal: None,
                    terminal_result_digest: None,
                    terminal_result_json: None,
                    terminal_task_status: None,
                },
            );
        }
        FileStore
            .write_json_atomic_absolute(
                service.config_write_root(),
                &service.global_settings_path(),
                &settings,
            )
            .unwrap();

        assert_eq!(
            service
                .record_agent_lint_repair_attestation(
                    "identity-key",
                    "identity-revision",
                    "new-task",
                    "new-digest",
                )
                .unwrap_err()
                .code,
            "LINT_REPAIR_ATTESTATION_CAPACITY_REACHED"
        );
        assert!(!service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "new-task",
                "new-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
            )
            .unwrap());

        settings
            .agent_lint_repair_attestations
            .iter_mut()
            .for_each(|item| {
                item.confirmed_at = (chrono::Utc::now()
                    - chrono::Duration::minutes(
                        super::AGENT_LINT_REPAIR_CANCEL_TOMBSTONE_MINUTES + 1,
                    ))
                .to_rfc3339();
            });
        FileStore
            .write_json_atomic_absolute(
                service.config_write_root(),
                &service.global_settings_path(),
                &settings,
            )
            .unwrap();
        service
            .record_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "new-task",
                "new-digest",
            )
            .unwrap();
        assert!(service
            .has_agent_lint_repair_attestation(
                "identity-key",
                "identity-revision",
                "new-task",
                "new-digest",
                &[AgentLintRepairAttestationLifecycle::QueuedAuthorized],
            )
            .unwrap());

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn remote_provider_disclosure_is_global_versioned_and_survives_ui_saves() {
        let (context, root, config_dir) = tmp_paths("workflow-remote-disclosure");
        let service = SettingsService::with_config_dir(config_dir.clone());

        assert!(!service
            .is_remote_provider_disclosure_acknowledged("workflow-remote-provider-v1")
            .unwrap());
        service
            .acknowledge_remote_provider_disclosure("workflow-remote-provider-v1")
            .unwrap();
        assert!(service
            .is_remote_provider_disclosure_acknowledged("workflow-remote-provider-v1")
            .unwrap());
        assert!(!service
            .is_remote_provider_disclosure_acknowledged("workflow-remote-provider-v2")
            .unwrap());

        let mut settings = service.read_settings(&context).unwrap();
        settings.language = "zh-CN".into();
        service.save_settings(&context, &settings).unwrap();
        assert!(service
            .is_remote_provider_disclosure_acknowledged("workflow-remote-provider-v1")
            .unwrap());
        let project_settings: serde_json::Value =
            FileStore.read_json(&context, ".app/settings.json").unwrap();
        assert!(project_settings
            .get("remoteProviderDisclosureRevision")
            .is_none());

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn saves_global_ui_preferences_without_project_context() {
        let (_context, root, config_dir) = tmp_paths("global-ui-preferences");
        let service = SettingsService::with_config_dir(config_dir.clone());

        let saved = service
            .save_global_ui_preferences(GlobalUiPreferences {
                language: "zh-CN".into(),
                theme: ThemePreference::Dark,
            })
            .unwrap();

        assert_eq!(saved.language, "zh-CN");
        assert_eq!(saved.theme, ThemePreference::Dark);
        assert_eq!(
            service.read_global_ui_preferences().unwrap(),
            GlobalUiPreferences {
                language: "zh-CN".into(),
                theme: ThemePreference::Dark,
            }
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }

    #[test]
    fn save_settings_does_not_overwrite_unreadable_global_settings() {
        let (context, root, config_dir) = tmp_paths("chat-convenience-corrupt-global");
        let service = SettingsService::with_config_dir(config_dir.clone());
        std::fs::create_dir_all(&config_dir).unwrap();
        let global_path = config_dir.join("settings.json");
        std::fs::write(&global_path, "{not-json").unwrap();
        let settings = Settings::default();

        let error = service
            .save_settings(&context, &settings)
            .expect_err("corrupt global settings must not be overwritten");

        assert_eq!(error.code, "JSON_PARSE_FAILED");
        assert_eq!(std::fs::read_to_string(&global_path).unwrap(), "{not-json");

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(config_dir).unwrap();
    }
}
