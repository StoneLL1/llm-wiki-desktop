use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Component, Path};

use crate::errors::BackendError;
use crate::models::agent::{AgentDetectionState, AgentKind};
use crate::models::compile::{
    CompileAction, CompileCandidate, CompileChangeSummary, CompileConflictResolution, CompileFile,
    CompileManifest, CompilePageType, CompilePlan, CompilePlanItem, CompileRoutePreference,
    ResolvedCompileRoute, SourceVersionRef,
};
use crate::models::llm::{LlmProviderConfig, LlmProviderKind};
use crate::models::paths::ProjectContext;
use crate::services::import_v2::source_registry::SourceRegistry;
use crate::services::{
    AgentService, CompileLegacyAdapter, CompilePromptRoute, FileStore, LlmService, SecretService,
    SettingsService, WriteMode,
};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;
use crate::utils::markdown_utils::{parse_frontmatter, split_frontmatter};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileApplyOutcome {
    pub affected_paths: Vec<String>,
    pub conflicts: Vec<String>,
}

pub struct CompileBackup {
    entries: Vec<(String, Option<Vec<u8>>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileSourceRegistry {
    V2,
    Legacy,
}

#[derive(Debug, Clone)]
pub struct ResolvedCompileSource {
    pub reference: SourceVersionRef,
    pub project_path: String,
    pub workspace_path: String,
    pub absolute_path: std::path::PathBuf,
    pub already_consumed: bool,
    pub registry: CompileSourceRegistry,
}

pub struct CompileExecutionServices<'a> {
    pub agent_service: &'a AgentService,
    pub llm_service: &'a LlmService,
    pub secret_service: &'a SecretService,
    pub settings_service: &'a SettingsService,
    pub task_service: &'a TaskService,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileGenerationPolicy {
    LegacyNoDeletes,
    WorkflowReviewableDeletes,
}

impl CompileGenerationPolicy {
    fn allows_reviewable_deletions(self) -> bool {
        matches!(self, Self::WorkflowReviewableDeletes)
    }
}

pub trait CompileGenerationObserver: Send {
    fn begin_candidate_generation(&mut self) -> Result<(), BackendError> {
        Ok(())
    }

    fn begin_validation(&mut self) -> Result<(), BackendError> {
        Ok(())
    }
}

pub struct NoopCompileGenerationObserver;

impl CompileGenerationObserver for NoopCompileGenerationObserver {}

#[derive(Default)]
pub struct CompileService;

impl CompileService {
    pub fn provider_prompt(workspace: &Path, language: &str) -> Result<String, BackendError> {
        let mut prompt = String::from("Return only JSON matching {files:[{path,content}],deletions:[],summary}.\nCreate real DERIVED content pages under wiki/ — entity pages in wiki/entities/, concept pages in wiki/concepts/, and synthesis/comparison pages as schema.md dictates, that synthesize ACROSS the sources. Do NOT return only the index files. wiki/sources/ already holds the verbatim imported originals: READ them as authoritative and CITE them, but NEVER create, modify, or delete any file under wiki/sources/. Cite sources on every derived page two ways: (1) a frontmatter `sources: [\"<original-source-filename>\"]` array, and (2) a human-readable `> Sources:` line linking to the originals. Paths must be wiki/*.md (never wiki/sources/*) and must include wiki/index.md, wiki/overview.md, wiki/log.md. Never delete pages.\n");
        prompt.push_str(&crate::services::render_compile_core_instructions());
        prompt.push('\n');
        prompt.push_str(&crate::utils::i18n::language_instruction(language));
        prompt.push_str(" Write each page's prose body in that language; keep frontmatter keys, paths, and this JSON structure in English.\n");
        for name in ["purpose.md", "schema.md"] {
            let content = std::fs::read_to_string(workspace.join(name)).map_err(|error| {
                BackendError::new("COMPILE_INPUT_READ_FAILED", error.to_string(), true, false)
            })?;
            prompt.push_str(&format!("\n--- {name} ---\n{content}"));
        }
        for root in [workspace.join("raw/extracted"), workspace.join("wiki")] {
            for file in FileStore.list_markdown_files(&root)? {
                let relative = file
                    .strip_prefix(workspace)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let content = std::fs::read_to_string(&file).map_err(|error| {
                    BackendError::new("COMPILE_INPUT_READ_FAILED", error.to_string(), true, false)
                })?;
                prompt.push_str(&format!("\n--- {relative} ---\n{content}"));
            }
        }
        Ok(prompt)
    }

    pub fn provider_plan_prompt(workspace: &Path, language: &str) -> Result<String, BackendError> {
        Self::provider_plan_prompt_with_policy(
            workspace,
            language,
            CompileGenerationPolicy::LegacyNoDeletes,
        )
    }

    fn provider_plan_prompt_with_policy(
        workspace: &Path,
        language: &str,
        policy: CompileGenerationPolicy,
    ) -> Result<String, BackendError> {
        let mut prompt = crate::services::render_compile_prompt_header_with_policy(
            CompilePromptRoute::ByokPlan,
            language,
            policy.allows_reviewable_deletions(),
        );
        prompt.push('\n');
        Self::append_workspace_markdown(&mut prompt, workspace)?;
        Ok(prompt)
    }

    pub fn provider_manifest_prompt(
        workspace: &Path,
        language: &str,
        accepted_plan: Option<&CompilePlan>,
    ) -> Result<String, BackendError> {
        Self::provider_manifest_prompt_with_policy(
            workspace,
            language,
            accepted_plan,
            CompileGenerationPolicy::LegacyNoDeletes,
        )
    }

    fn provider_manifest_prompt_with_policy(
        workspace: &Path,
        language: &str,
        accepted_plan: Option<&CompilePlan>,
        policy: CompileGenerationPolicy,
    ) -> Result<String, BackendError> {
        let mut prompt = crate::services::render_compile_prompt_header_with_policy(
            CompilePromptRoute::ByokManifest,
            language,
            policy.allows_reviewable_deletions(),
        );
        if policy.allows_reviewable_deletions() {
            prompt.push_str("\nDeletion policy: deletions may list existing derived wiki/*.md pages when the accepted plan requires a rename or removal. Never delete wiki/sources/* or non-Markdown paths. Every deletion is review-only and will not be applied without explicit confirmation.\n");
        }
        if let Some(plan) = accepted_plan {
            let plan_json = serde_json::to_string_pretty(plan).map_err(|error| {
                BackendError::new("COMPILE_PLAN_INVALID", error.to_string(), true, false)
            })?;
            prompt.push_str("\n\n--- Accepted CompilePlan ---\n");
            prompt.push_str(&plan_json);
        }
        prompt.push('\n');
        Self::append_workspace_markdown(&mut prompt, workspace)?;
        Ok(prompt)
    }

    pub fn resolve_legacy_route(
        context: &ProjectContext,
        preference: CompileRoutePreference,
        explicit_agent: Option<AgentKind>,
        explicit_provider: Option<LlmProviderKind>,
        services: &CompileExecutionServices<'_>,
    ) -> Result<ResolvedCompileRoute, BackendError> {
        let agent_config = AgentService::load_config(context)?;
        let selected_agent = explicit_agent.or(agent_config.default_agent);
        let usable_agent = selected_agent.filter(|agent| {
            services
                .agent_service
                .detect_agents(Some(*agent))
                .iter()
                .any(|info| info.kind == *agent && info.state == AgentDetectionState::Installed)
        });
        let providers = LlmService::list_providers(context)?;
        let selected_provider =
            select_compile_provider(explicit_provider, &providers, services.secret_service)?;
        match preference {
            CompileRoutePreference::Agent => usable_agent
                .map(|agent| ResolvedCompileRoute::Agent { agent, model: None })
                .ok_or_else(|| {
                    BackendError::new(
                        "AGENT_UNAVAILABLE",
                        "Selected Agent is not available.",
                        true,
                        true,
                    )
                }),
            CompileRoutePreference::Byok => selected_provider
                .map(|provider| ResolvedCompileRoute::Byok {
                    provider: provider.provider,
                    model: provider.model,
                })
                .ok_or_else(|| {
                    BackendError::new(
                        "LLM_PROVIDER_MISSING",
                        "No enabled BYOK provider is available.",
                        true,
                        true,
                    )
                }),
            CompileRoutePreference::Auto => {
                if let Some(agent) = usable_agent {
                    Ok(ResolvedCompileRoute::Agent { agent, model: None })
                } else {
                    selected_provider
                        .map(|provider| ResolvedCompileRoute::Byok {
                            provider: provider.provider,
                            model: provider.model,
                        })
                        .ok_or_else(|| {
                            BackendError::new(
                                "LLM_PROVIDER_MISSING",
                                "No enabled BYOK provider is available.",
                                true,
                                true,
                            )
                        })
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn generate_candidate(
        context: &ProjectContext,
        workspace: &Path,
        task_id: &str,
        baseline: &HashMap<String, String>,
        sources: &[ResolvedCompileSource],
        protected_sources: &HashMap<String, String>,
        route: ResolvedCompileRoute,
        policy: CompileGenerationPolicy,
        services: &CompileExecutionServices<'_>,
        observer: &mut dyn CompileGenerationObserver,
    ) -> Result<CompileCandidate, BackendError> {
        let language = services
            .settings_service
            .read_settings(context)
            .map(|settings| settings.language)
            .unwrap_or_else(|_| "en".to_string());
        let reviewable_workspace_paths = if policy.allows_reviewable_deletions() {
            Some(Self::workspace_candidate_paths(workspace)?)
        } else {
            None
        };
        let (plan, manifest) = match &route {
            ResolvedCompileRoute::Agent { agent, .. } => {
                let installed = services
                    .agent_service
                    .detect_agents(Some(*agent))
                    .iter()
                    .any(|info| {
                        info.kind == *agent && info.state == AgentDetectionState::Installed
                    });
                if !installed {
                    return Err(BackendError::new(
                        "AGENT_UNAVAILABLE",
                        "The prepared Agent route is no longer available.",
                        true,
                        true,
                    ));
                }
                services
                    .task_service
                    .append_log(
                        task_id,
                        LogLevel::Info,
                        format!("Running {}", agent.command()),
                    )
                    .map_err(task_operation_error)?;
                let invocation = AgentService::invocation(
                    *agent,
                    workspace,
                    &Self::compile_prompt_with_policy(workspace, &language, policy),
                )?;
                services.agent_service.run_task_streaming(
                    &invocation,
                    services.task_service,
                    task_id,
                )?;
                let plan = read_agent_compile_plan(workspace)?;
                validate_compile_plan(context, &plan, baseline, sources)?;
                observer.begin_candidate_generation()?;
                observer.begin_validation()?;
                let manifest = Self::manifest_from_workspace_protected_with_policy(
                    workspace,
                    baseline,
                    protected_sources,
                    policy,
                    reviewable_workspace_paths.as_ref(),
                )?;
                let known_sources = Self::known_source_refs_for_sources(sources);
                Self::validate_manifest_semantics_with_policy(
                    context,
                    &manifest,
                    Some(&plan),
                    &known_sources,
                    policy.allows_reviewable_deletions(),
                )?;
                const SCAFFOLD: [&str; 3] = ["wiki/index.md", "wiki/overview.md", "wiki/log.md"];
                if !manifest
                    .files
                    .iter()
                    .any(|file| !SCAFFOLD.contains(&file.path.as_str()))
                    && !sources.is_empty()
                {
                    return Err(BackendError::new(
                        "COMPILE_EMPTY_OUTPUT",
                        "Agent finished but wrote no wiki pages. This usually means the agent lacked write permission or hit an upstream API error.",
                        true,
                        false,
                    ));
                }
                (plan, manifest)
            }
            ResolvedCompileRoute::Byok { provider, model } => {
                let providers = LlmService::list_providers(context)?;
                let config =
                    select_compile_provider(Some(*provider), &providers, services.secret_service)?
                        .filter(|config| config.model == *model)
                        .ok_or_else(|| {
                            BackendError::new(
                                "WORKFLOW_ROUTE_STALE",
                                "The prepared Provider or model changed before execution.",
                                true,
                                true,
                            )
                        })?;
                let secret = services.secret_service.get(*provider)?;
                services
                    .task_service
                    .append_log(
                        task_id,
                        LogLevel::Info,
                        format!("Calling {:?} for compile plan", provider),
                    )
                    .map_err(task_operation_error)?;
                let plan_prompt =
                    Self::provider_plan_prompt_with_policy(workspace, &language, policy)?;
                let raw_plan = crate::tasks::byok_progress::poll_with_progress(
                    services.task_service,
                    task_id,
                    "Planning",
                    services
                        .llm_service
                        .complete(&config, secret.as_deref(), &plan_prompt),
                )
                .await
                .map_err(|_| {
                    crate::tasks::byok_progress::cancelled_error(
                        "COMPILE_CANCELLED",
                        "Wiki compile was cancelled.",
                    )
                })??;
                let plan = Self::parse_plan(&raw_plan)?;
                validate_compile_plan(context, &plan, baseline, sources)?;
                observer.begin_candidate_generation()?;
                services
                    .task_service
                    .append_log(
                        task_id,
                        LogLevel::Info,
                        format!("Calling {:?} for compile manifest", provider),
                    )
                    .map_err(task_operation_error)?;
                let manifest_prompt = Self::provider_manifest_prompt_with_policy(
                    workspace,
                    &language,
                    Some(&plan),
                    policy,
                )?;
                let raw_manifest = crate::tasks::byok_progress::poll_with_progress(
                    services.task_service,
                    task_id,
                    "Generating",
                    services
                        .llm_service
                        .complete(&config, secret.as_deref(), &manifest_prompt),
                )
                .await
                .map_err(|_| {
                    crate::tasks::byok_progress::cancelled_error(
                        "COMPILE_CANCELLED",
                        "Wiki compile was cancelled.",
                    )
                })??;
                observer.begin_validation()?;
                let manifest = Self::parse_manifest_with_policy(
                    &raw_manifest,
                    policy.allows_reviewable_deletions(),
                )?;
                let known_sources = Self::known_source_refs_for_sources(sources);
                Self::validate_manifest_semantics_with_policy(
                    context,
                    &manifest,
                    Some(&plan),
                    &known_sources,
                    policy.allows_reviewable_deletions(),
                )?;
                (plan, manifest)
            }
        };
        Ok(CompileCandidate {
            route,
            plan,
            manifest,
        })
    }

    fn append_workspace_markdown(
        prompt: &mut String,
        workspace: &Path,
    ) -> Result<(), BackendError> {
        for name in ["purpose.md", "schema.md"] {
            let content = std::fs::read_to_string(workspace.join(name)).map_err(|error| {
                BackendError::new("COMPILE_INPUT_READ_FAILED", error.to_string(), true, false)
            })?;
            prompt.push_str(&format!("\n--- {name} ---\n{content}"));
        }
        for root in [workspace.join("raw/extracted"), workspace.join("wiki")] {
            for file in FileStore.list_markdown_files(&root)? {
                let relative = file
                    .strip_prefix(workspace)
                    .unwrap_or(&file)
                    .to_string_lossy()
                    .replace('\\', "/");
                let content = std::fs::read_to_string(&file).map_err(|error| {
                    BackendError::new("COMPILE_INPUT_READ_FAILED", error.to_string(), true, false)
                })?;
                prompt.push_str(&format!("\n--- {relative} ---\n{content}"));
            }
        }
        Ok(())
    }

    pub fn extracted_markdown_files(
        context: &ProjectContext,
    ) -> Result<Vec<std::path::PathBuf>, BackendError> {
        let references = Self::list_source_versions(context)?;
        if references.is_empty() {
            if CompileLegacyAdapter::exists(context)
                && !context.app_dir.join("source-index-v2.json").is_file()
            {
                let diagnostics = CompileLegacyAdapter::diagnostics(context)?;
                return Err(compile_input_empty_error(
                    true,
                    &diagnostics.confirmed_sources,
                    &diagnostics.markdown_paths,
                    &diagnostics.empty_markdown_paths,
                ));
            }
            return Err(compile_input_empty_error(false, &[], &[], &[]));
        }
        Ok(Self::resolve_source_versions(context, &references)?
            .into_iter()
            .map(|source| source.absolute_path)
            .collect())
    }

    pub fn list_source_versions(
        context: &ProjectContext,
    ) -> Result<Vec<SourceVersionRef>, BackendError> {
        if context.app_dir.join("source-index-v2.json").is_file() {
            let files = FileStore;
            let index = SourceRegistry::read_index(context, &files)?;
            let mut source_ids = index
                .by_content_hash
                .values()
                .chain(index.by_locator.values())
                .map(|pointer| pointer.source_id.clone())
                .collect::<Vec<_>>();
            source_ids.sort();
            source_ids.dedup();
            let mut references = Vec::with_capacity(source_ids.len());
            for source_id in source_ids {
                validate_source_identity(&source_id)?;
                let manifest = SourceRegistry::read_manifest(
                    context,
                    &files,
                    &format!(".app/sources/{source_id}.json"),
                )?;
                let version = manifest
                    .versions
                    .iter()
                    .find(|version| version.version_id == manifest.current_version_id)
                    .ok_or_else(invalid_source_version)?;
                references.push(SourceVersionRef {
                    source_id,
                    version_id: version.version_id.clone(),
                    content_hash: version.content_hash.clone(),
                });
            }
            return Ok(references);
        }
        Ok(CompileLegacyAdapter::list(context)?
            .into_iter()
            .map(|source| source.reference)
            .collect())
    }

    pub fn resolve_source_versions(
        context: &ProjectContext,
        requested: &[SourceVersionRef],
    ) -> Result<Vec<ResolvedCompileSource>, BackendError> {
        if requested.is_empty() {
            return Err(BackendError::new(
                "COMPILE_SOURCE_SELECTION_EMPTY",
                "Select at least one Source version before updating the Wiki.",
                true,
                true,
            ));
        }
        let unique = requested.iter().cloned().collect::<HashSet<_>>();
        if unique.len() != requested.len() {
            return Err(BackendError::new(
                "COMPILE_SOURCE_SELECTION_DUPLICATE",
                "Selected Source versions must be unique.",
                true,
                true,
            ));
        }
        let mut source_ids = HashSet::new();
        if requested
            .iter()
            .any(|reference| !source_ids.insert(reference.source_id.as_str()))
        {
            return Err(BackendError::new(
                "COMPILE_SOURCE_SELECTION_AMBIGUOUS",
                "Only one version of each Source can be compiled at a time.",
                true,
                true,
            ));
        }
        if !context.app_dir.join("source-index-v2.json").is_file() {
            return Ok(CompileLegacyAdapter::resolve(context, requested)?
                .into_iter()
                .map(|source| ResolvedCompileSource {
                    workspace_path: source.project_path.clone(),
                    project_path: source.project_path,
                    absolute_path: source.absolute_path,
                    reference: source.reference,
                    already_consumed: source.already_consumed,
                    registry: CompileSourceRegistry::Legacy,
                })
                .collect());
        }
        let files = FileStore;
        let index = SourceRegistry::read_index(context, &files)?;
        let mut resolved = Vec::with_capacity(requested.len());
        for reference in requested {
            let validated =
                SourceRegistry::resolve_compile_source_version(context, &files, &index, reference)?;
            let manifest = validated.manifest;
            let project_path = validated.project_path;
            let absolute_path = context.resolve_project_path(&project_path)?;
            let workspace_path = manifest.wiki_path.clone();
            let already_consumed = manifest.compiled_consumptions.iter().any(|consumption| {
                consumption.version_id == reference.version_id
                    && consumption.content_hash == reference.content_hash
            });
            resolved.push(ResolvedCompileSource {
                reference: reference.clone(),
                project_path,
                workspace_path,
                absolute_path,
                already_consumed,
                registry: CompileSourceRegistry::V2,
            });
        }
        Ok(resolved)
    }

    pub fn resolve_conflict_manifest(
        manifest: &CompileManifest,
        conflict_paths: &[String],
        resolution: CompileConflictResolution,
        manual_files: &[CompileFile],
    ) -> Result<CompileManifest, BackendError> {
        Self::validate_manifest(manifest)?;
        let conflicts: HashSet<&str> = conflict_paths.iter().map(String::as_str).collect();
        let manifest_paths: HashSet<&str> = manifest
            .files
            .iter()
            .map(|file| file.path.as_str())
            .chain(manifest.deletions.iter().map(String::as_str))
            .collect();
        if conflicts.iter().any(|path| !manifest_paths.contains(path)) {
            return Err(BackendError::new(
                "COMPILE_CONFLICT_PATH_INVALID",
                "A conflict path is not part of the generated manifest.",
                false,
                true,
            ));
        }
        if resolution == CompileConflictResolution::UseGenerated {
            return Ok(manifest.clone());
        }
        let mut resolved = CompileManifest {
            files: manifest
                .files
                .iter()
                .filter(|file| !conflicts.contains(file.path.as_str()))
                .cloned()
                .collect(),
            deletions: manifest
                .deletions
                .iter()
                .filter(|path| !conflicts.contains(path.as_str()))
                .cloned()
                .collect(),
            summary: manifest.summary.clone(),
        };
        if resolution == CompileConflictResolution::KeepCurrent {
            return Ok(resolved);
        }

        let mut manual_by_path = HashMap::new();
        for file in manual_files {
            if !conflicts.contains(file.path.as_str())
                || manual_by_path.insert(file.path.as_str(), file).is_some()
            {
                return Err(BackendError::new(
                    "COMPILE_MANUAL_MERGE_INVALID",
                    "Manual merge files must map one-to-one to conflicting paths.",
                    true,
                    true,
                ));
            }
        }
        if manual_by_path.len() != conflicts.len() {
            return Err(BackendError::new(
                "COMPILE_MANUAL_MERGE_INCOMPLETE",
                "Manual merge content is required for every conflicting path.",
                true,
                true,
            ));
        }
        for path in conflict_paths {
            resolved
                .files
                .push((*manual_by_path[path.as_str()]).clone());
        }
        Self::validate_manifest(&resolved)?;
        Ok(resolved)
    }

    pub fn parse_manifest(raw: &str) -> Result<CompileManifest, BackendError> {
        Self::parse_manifest_with_policy(raw, false)
    }

    fn parse_manifest_with_policy(
        raw: &str,
        allow_reviewable_deletions: bool,
    ) -> Result<CompileManifest, BackendError> {
        let trimmed = raw.trim();
        let json = if let Some(start) = trimmed.find("```json") {
            let rest = &trimmed[start + 7..];
            let end = rest.find("```").ok_or_else(|| {
                BackendError::new(
                    "COMPILE_OUTPUT_INVALID",
                    "Unclosed JSON code fence.",
                    true,
                    false,
                )
            })?;
            rest[..end].trim()
        } else if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
            &trimmed[start..=end]
        } else {
            trimmed
        };
        let manifest: CompileManifest = serde_json::from_str(json).map_err(|error| {
            BackendError::new("COMPILE_OUTPUT_INVALID", error.to_string(), true, false)
        })?;
        Self::validate_manifest_with_policy(&manifest, allow_reviewable_deletions)?;
        Ok(manifest)
    }

    pub fn parse_plan(raw: &str) -> Result<CompilePlan, BackendError> {
        let json = extract_json_object(raw, "COMPILE_PLAN_INVALID")?;
        let plan: CompilePlan = serde_json::from_str(json).map_err(|error| {
            BackendError::new("COMPILE_PLAN_INVALID", error.to_string(), true, false)
        })?;
        if plan.summary.trim().is_empty() || plan.items.is_empty() {
            return Err(BackendError::new(
                "COMPILE_PLAN_INVALID",
                "CompilePlan must include a summary and at least one item.",
                true,
                false,
            ));
        }
        Ok(plan)
    }

    pub fn create_workspace_for_sources(
        context: &ProjectContext,
        task_id: &str,
        sources: &[ResolvedCompileSource],
    ) -> Result<std::path::PathBuf, BackendError> {
        let workspace = std::env::temp_dir().join("llm-wiki-desktop").join(task_id);
        if workspace.exists() {
            std::fs::remove_dir_all(&workspace)
                .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &workspace))?;
        }
        std::fs::create_dir_all(&workspace)
            .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &workspace))?;
        let result = Self::populate_workspace_for_sources(context, &workspace, sources);
        if let Err(error) = result {
            let _ = std::fs::remove_dir_all(&workspace);
            return Err(error);
        }
        Ok(workspace)
    }

    fn populate_workspace_for_sources(
        context: &ProjectContext,
        workspace: &Path,
        sources: &[ResolvedCompileSource],
    ) -> Result<(), BackendError> {
        for name in ["purpose.md", "schema.md"] {
            let source = context.root.join(name);
            if !source.is_file() {
                return Err(BackendError::new(
                    "COMPILE_INPUT_MISSING",
                    format!("Required input is missing: {name}"),
                    true,
                    true,
                ));
            }
            copy_workspace_file(&source, &workspace.join(name))?;
        }
        let selected_refs = Self::known_source_refs_for_sources(sources);
        for source in sources {
            copy_workspace_file(
                &source.absolute_path,
                &workspace.join(&source.workspace_path),
            )?;
        }
        for absolute in FileStore.list_markdown_files(&context.wiki_dir)? {
            let relative = context.to_project_relative(&absolute)?;
            if is_compile_protected_path(&relative) {
                continue;
            }
            let include = is_structural_page(&relative)
                || std::fs::read_to_string(&absolute)
                    .map(|content| {
                        selected_refs
                            .iter()
                            .any(|source_ref| content.contains(source_ref))
                    })
                    .unwrap_or(false);
            if include {
                copy_workspace_file(&absolute, &workspace.join(&relative))?;
            }
        }
        let skill_dir = workspace.join("skills/wiki-ingest");
        std::fs::create_dir_all(&skill_dir)
            .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &skill_dir))?;
        std::fs::write(
            skill_dir.join("SKILL.md"),
            include_str!("../../templates/skills/wiki-ingest/SKILL.md"),
        )
        .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, &skill_dir))?;
        Ok(())
    }

    pub fn manifest_from_workspace(
        workspace: &Path,
        baseline: &HashMap<String, String>,
    ) -> Result<CompileManifest, BackendError> {
        let protected = Self::snapshot_workspace_sources(workspace)?;
        Self::manifest_from_workspace_protected(workspace, baseline, &protected)
    }

    pub fn snapshot_workspace_sources(
        workspace: &Path,
    ) -> Result<HashMap<String, String>, BackendError> {
        let wiki_root = workspace.join("wiki");
        let mut hashes = HashMap::new();
        for absolute in FileStore.list_markdown_files(&wiki_root)? {
            let relative = absolute
                .strip_prefix(workspace)
                .map_err(|_| invalid_source_version())?
                .to_string_lossy()
                .replace('\\', "/");
            if is_compile_protected_path(&relative) {
                hashes.insert(relative, hash_file(&absolute)?);
            }
        }
        Ok(hashes)
    }

    pub fn manifest_from_workspace_protected(
        workspace: &Path,
        baseline: &HashMap<String, String>,
        protected_sources: &HashMap<String, String>,
    ) -> Result<CompileManifest, BackendError> {
        Self::manifest_from_workspace_protected_with_policy(
            workspace,
            baseline,
            protected_sources,
            CompileGenerationPolicy::LegacyNoDeletes,
            None,
        )
    }

    fn manifest_from_workspace_protected_with_policy(
        workspace: &Path,
        baseline: &HashMap<String, String>,
        protected_sources: &HashMap<String, String>,
        policy: CompileGenerationPolicy,
        deletion_candidates: Option<&HashSet<String>>,
    ) -> Result<CompileManifest, BackendError> {
        if &Self::snapshot_workspace_sources(workspace)? != protected_sources {
            return Err(BackendError::new(
                "COMPILE_SOURCE_MUTATION_FORBIDDEN",
                "Compile attempted to create, modify, or delete an import-owned Source.",
                false,
                true,
            ));
        }
        let wiki = workspace.join("wiki");
        let mut files = Vec::new();
        for absolute in FileStore.list_markdown_files(&wiki)? {
            let relative = absolute
                .strip_prefix(workspace)
                .map_err(|_| {
                    BackendError::new(
                        "COMPILE_PATH_INVALID",
                        "Candidate path escaped workspace.",
                        false,
                        false,
                    )
                })?
                .to_string_lossy()
                .replace('\\', "/");
            if is_compile_protected_path(&relative) {
                continue;
            }
            let content = std::fs::read_to_string(&absolute)
                .map_err(|error| io_error("COMPILE_OUTPUT_READ_FAILED", error, &absolute))?;
            if baseline
                .get(&relative)
                .map(|hash| hash == &hash_bytes(content.as_bytes()))
                .unwrap_or(false)
            {
                continue;
            }
            files.push(crate::models::compile::CompileFile::new(relative, content));
        }
        let mut deletions = Vec::new();
        if policy.allows_reviewable_deletions() {
            for path in deletion_candidates.into_iter().flatten() {
                if is_safe_wiki_markdown(path)
                    && !is_compile_protected_path(path)
                    && !workspace.join(path).exists()
                {
                    deletions.push(path.clone());
                }
            }
            deletions.sort();
            deletions.dedup();
        }
        let manifest = CompileManifest {
            files,
            deletions,
            summary: "Agent wiki compile".into(),
        };
        Self::validate_manifest_with_policy(&manifest, policy.allows_reviewable_deletions())?;
        Ok(manifest)
    }

    fn workspace_candidate_paths(workspace: &Path) -> Result<HashSet<String>, BackendError> {
        FileStore
            .list_markdown_files(&workspace.join("wiki"))?
            .into_iter()
            .map(|absolute| {
                absolute
                    .strip_prefix(workspace)
                    .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                    .map_err(|_| {
                        BackendError::new(
                            "COMPILE_PATH_INVALID",
                            "Candidate path escaped workspace.",
                            false,
                            true,
                        )
                    })
            })
            .filter(|result| {
                result
                    .as_ref()
                    .map_or(true, |path| !is_compile_protected_path(path))
            })
            .collect()
    }

    pub fn compile_prompt(workspace: &Path, language: &str) -> String {
        Self::compile_prompt_with_policy(
            workspace,
            language,
            CompileGenerationPolicy::LegacyNoDeletes,
        )
    }

    fn compile_prompt_with_policy(
        workspace: &Path,
        language: &str,
        policy: CompileGenerationPolicy,
    ) -> String {
        let deletion_rule = if policy.allows_reviewable_deletions() {
            "You may delete an existing derived wiki/*.md page only when the CompilePlan explicitly requires a rename or removal. Never delete wiki/sources/*; deletions are captured as review-only candidates."
        } else {
            "Never delete existing pages."
        };
        let mut prompt = format!(
            "Compile this local Markdown wiki. Workspace root: {}.\nFollow skills/wiki-ingest/SKILL.md and schema.md. Do exactly this:\n\
             1. Read purpose.md, schema.md, every original in wiki/sources/ (the verbatim imported sources) and legacy raw/extracted/, plus the existing wiki/ tree.\n\
             2. wiki/sources/ is IMPORT-OWNED: it holds the verbatim extracted originals. READ them as authoritative and CITE them. NEVER create, modify, or delete any file under wiki/sources/.\n\
             3. CREATE derived Markdown pages under wiki/ — entity pages in wiki/entities/, concept pages in wiki/concepts/, and synthesis/comparison pages as the sources warrant. Synthesize ACROSS sources; do NOT write one page per source, and do NOT summarize or copy a source into another page. Name each derived page after the concept it covers, not after a source filename. You MUST produce real content pages — touching only the index files is a failure.\n\
             4. On every derived page, cite sources two ways: a frontmatter `sources: [\"<original-source-filename>\"]` array (machine join key for the graph), and a human-readable `> Sources:` line of Markdown links to ../sources/<page>.md (or [[sources/<page>]]).\n\
             5. UPDATE wiki/index.md and wiki/overview.md to list and summarize the pages you created, and append a short entry to wiki/log.md. Cascade: after writing a page, update any other page materially affected by the new information.\n\
             6. Use project-relative Markdown links and [[wikilinks]]. {deletion_rule} Work only inside this workspace; do not access or modify anything outside it. Any shell commands you run must operate only within this workspace root and must never affect files, directories, or systems outside it.",
            workspace.to_string_lossy(),
        );
        prompt.push_str("\nBefore candidate writes are accepted, write CompilePlan JSON to compile-plan.json with summary, items, action, targetPath, pageType, sourceIds, affectedExistingPages, reason, riskFlags, and globalRiskFlags.\n");
        prompt.push_str(
            &crate::services::render_compile_core_instructions_with_policy(
                policy.allows_reviewable_deletions(),
            ),
        );
        // Steer generated wiki page prose to the user's language; structural
        // fields (frontmatter keys, file paths, section headings used by
        // lint/graph parsing) stay English so schema is unaffected.
        prompt.push('\n');
        prompt.push_str(&crate::utils::i18n::language_instruction(language));
        prompt.push_str(" Write each page's prose body in that language; keep frontmatter keys, file paths, and section headings in English.");
        prompt
    }

    pub fn validate_manifest(manifest: &CompileManifest) -> Result<(), BackendError> {
        Self::validate_manifest_with_policy(manifest, false)
    }

    pub fn validate_workflow_manifest(manifest: &CompileManifest) -> Result<(), BackendError> {
        Self::validate_manifest_with_policy(manifest, true)
    }

    fn validate_manifest_with_policy(
        manifest: &CompileManifest,
        allow_reviewable_deletions: bool,
    ) -> Result<(), BackendError> {
        let mut seen = HashSet::new();
        for path in manifest
            .files
            .iter()
            .map(|file| file.path.as_str())
            .chain(manifest.deletions.iter().map(String::as_str))
        {
            if is_compile_protected_path(path) {
                return Err(BackendError::new(
                    "COMPILE_PROTECTED_PATH",
                    "wiki/sources/ is import-owned; compile may read and cite these originals but cannot create, modify, or delete them.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": path })));
            }
            if !is_safe_wiki_markdown(path) || !seen.insert(path.to_string()) {
                return Err(BackendError::new(
                    "COMPILE_PATH_INVALID",
                    "Compile output contains an unsafe or duplicate path.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": path })));
            }
        }
        if !allow_reviewable_deletions && !manifest.deletions.is_empty() {
            return Err(BackendError::new(
                "COMPILE_DELETE_FORBIDDEN",
                "Compile cannot delete pages. Record obsolete pages in wiki/log.md for user review instead.",
                true,
                true,
            )
            .with_details(serde_json::json!({ "deletions": manifest.deletions })));
        }

        for required in ["wiki/index.md", "wiki/overview.md", "wiki/log.md"] {
            if !manifest.files.iter().any(|file| file.path == required) {
                return Err(BackendError::new(
                    "COMPILE_CORE_PAGE_MISSING",
                    "Compile output must include index, overview, and log pages.",
                    true,
                    false,
                )
                .with_details(serde_json::json!({ "path": required })));
            }
        }
        Ok(())
    }

    pub fn classify_workflow_changes(
        context: &ProjectContext,
        manifest: &CompileManifest,
        plan: &CompilePlan,
        baseline: &HashMap<String, String>,
        broad_rewrite: bool,
    ) -> Result<CompileChangeSummary, BackendError> {
        Self::validate_workflow_manifest(manifest)?;
        let store = FileStore;
        let mut summary = CompileChangeSummary::default();
        let baseline_casefold = baseline
            .keys()
            .map(|path| (path.to_ascii_lowercase(), path.as_str()))
            .collect::<HashMap<_, _>>();
        let global_risk = broad_rewrite || !plan.global_risk_flags.is_empty();
        for file in &manifest.files {
            let target = context.resolve_project_path(&file.path)?;
            let expected = baseline.get(&file.path);
            if expected.is_none()
                && baseline_casefold
                    .get(&file.path.to_ascii_lowercase())
                    .is_some_and(|existing| *existing != file.path.as_str())
            {
                summary.conflicted.push(file.path.clone());
                continue;
            }
            match expected {
                Some(expected) => {
                    if !target.is_file() || store.file_hash(context, &file.path)? != *expected {
                        summary.conflicted.push(file.path.clone());
                        continue;
                    }
                    let current = std::fs::read_to_string(&target)
                        .map_err(|error| io_error("COMPILE_INPUT_READ_FAILED", error, &target))?;
                    if current == file.content {
                        summary.skipped.push(file.path.clone());
                    } else {
                        summary.updated.push(file.path.clone());
                    }
                }
                None if target.exists() => summary.conflicted.push(file.path.clone()),
                None => summary.created.push(file.path.clone()),
            }
            let item_risk = plan
                .items
                .iter()
                .find(|item| item.target_path == file.path)
                .is_some_and(|item| match (expected.is_some(), item.action) {
                    (_, CompileAction::Merge) => true,
                    (true, CompileAction::Create) | (false, CompileAction::Update) => true,
                    _ => !item.risk_flags.is_empty(),
                });
            if global_risk || item_risk {
                summary.high_risk.push(file.path.clone());
            }
        }
        for deletion in &manifest.deletions {
            let target = context.resolve_project_path(deletion)?;
            match baseline.get(deletion) {
                Some(expected)
                    if target.is_file() && store.file_hash(context, deletion)? == *expected =>
                {
                    summary.deleted.push(deletion.clone());
                    summary.high_risk.push(deletion.clone());
                }
                Some(_) => summary.conflicted.push(deletion.clone()),
                None if target.exists() => summary.conflicted.push(deletion.clone()),
                None => summary.skipped.push(deletion.clone()),
            }
        }
        for paths in [
            &mut summary.created,
            &mut summary.updated,
            &mut summary.skipped,
            &mut summary.deleted,
            &mut summary.conflicted,
            &mut summary.high_risk,
        ] {
            paths.sort();
            paths.dedup();
        }
        Ok(summary)
    }

    pub fn validate_plan(
        _context: &ProjectContext,
        plan: &CompilePlan,
        existing_pages: &[String],
        known_sources: &HashSet<String>,
    ) -> Result<(), BackendError> {
        if plan.summary.trim().is_empty() || plan.items.is_empty() {
            return Err(plan_error(
                "CompilePlan must include a summary and at least one item.",
            ));
        }
        if plan
            .items
            .iter()
            .all(|item| is_structural_page(&item.target_path))
        {
            return Err(plan_error(
                "CompilePlan must include at least one derived page, not only structural pages.",
            ));
        }
        let existing: HashSet<&str> = existing_pages.iter().map(String::as_str).collect();
        let mut seen = HashSet::new();
        for item in &plan.items {
            if is_compile_protected_path(&item.target_path) {
                return Err(protected_path_error(&item.target_path));
            }
            if !is_safe_wiki_markdown(&item.target_path) || !seen.insert(item.target_path.clone()) {
                return Err(plan_error_with_path(
                    "CompilePlan contains an unsafe or duplicate target path.",
                    &item.target_path,
                ));
            }
            if item.reason.trim().is_empty() {
                return Err(plan_error_with_path(
                    "Every CompilePlan item must include a reason.",
                    &item.target_path,
                ));
            }
            if !is_structural_page(&item.target_path) && item.source_ids.is_empty() {
                return Err(plan_error_with_path(
                    "Derived CompilePlan items must include sourceIds.",
                    &item.target_path,
                ));
            }
            for source in &item.source_ids {
                if !source_ref_known(source, known_sources) {
                    return Err(source_ref_error(&item.target_path, source));
                }
            }
            if item.action == CompileAction::Merge
                && !item
                    .affected_existing_pages
                    .iter()
                    .any(|path| path == &item.target_path && existing.contains(path.as_str()))
            {
                return Err(plan_error_with_path(
                    "Merge plan items must name the existing target page in affectedExistingPages.",
                    &item.target_path,
                ));
            }
        }
        Ok(())
    }

    pub fn validate_manifest_semantics(
        context: &ProjectContext,
        manifest: &CompileManifest,
        accepted_plan: Option<&CompilePlan>,
        known_sources: &HashSet<String>,
    ) -> Result<(), BackendError> {
        Self::validate_manifest_semantics_with_policy(
            context,
            manifest,
            accepted_plan,
            known_sources,
            false,
        )
    }

    pub fn validate_workflow_manifest_semantics(
        context: &ProjectContext,
        manifest: &CompileManifest,
        accepted_plan: Option<&CompilePlan>,
        known_sources: &HashSet<String>,
    ) -> Result<(), BackendError> {
        Self::validate_manifest_semantics_with_policy(
            context,
            manifest,
            accepted_plan,
            known_sources,
            true,
        )
    }

    fn validate_manifest_semantics_with_policy(
        _context: &ProjectContext,
        manifest: &CompileManifest,
        accepted_plan: Option<&CompilePlan>,
        known_sources: &HashSet<String>,
        allow_reviewable_deletions: bool,
    ) -> Result<(), BackendError> {
        Self::validate_manifest_with_policy(manifest, allow_reviewable_deletions)?;
        let planned_items: HashMap<&str, &CompilePlanItem> = accepted_plan
            .map(|plan| {
                plan.items
                    .iter()
                    .map(|item| (item.target_path.as_str(), item))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(plan) = accepted_plan {
            let manifest_paths: HashSet<&str> = manifest
                .files
                .iter()
                .map(|file| file.path.as_str())
                .collect();
            let planned_derived: Vec<&CompilePlanItem> = plan
                .items
                .iter()
                .filter(|item| !is_structural_page(&item.target_path))
                .collect();
            if planned_derived.is_empty()
                || planned_derived
                    .iter()
                    .any(|item| !manifest_paths.contains(item.target_path.as_str()))
            {
                return Err(manifest_semantic_error(
                    "Manifest must include every non-structural target from the accepted CompilePlan.",
                    planned_derived
                        .first()
                        .map(|item| item.target_path.as_str())
                        .unwrap_or("wiki/index.md"),
                ));
            }
        }
        for file in &manifest.files {
            if is_structural_page(&file.path) {
                continue;
            }
            let planned_item = planned_items.get(file.path.as_str()).copied();
            if accepted_plan.is_some() && planned_item.is_none() {
                return Err(manifest_semantic_error(
                    "Manifest contains a derived page not present in the accepted CompilePlan.",
                    &file.path,
                ));
            }
            let split = split_frontmatter(&file.content);
            let Some(raw_frontmatter) = split.frontmatter.as_deref() else {
                return Err(manifest_semantic_error(
                    "Derived pages must include YAML frontmatter.",
                    &file.path,
                ));
            };
            let frontmatter = parse_frontmatter(raw_frontmatter);
            let page_type = frontmatter
                .get_scalar("type")
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    manifest_semantic_error(
                        "Derived pages must include frontmatter type.",
                        &file.path,
                    )
                })?;
            if !is_known_page_type(&page_type) {
                return Err(manifest_semantic_error(
                    "Derived page frontmatter type is not supported.",
                    &file.path,
                ));
            }
            if let Some(item) = planned_item {
                if page_type != page_type_as_str(item.page_type) {
                    return Err(manifest_semantic_error(
                        "Manifest page type does not match the accepted CompilePlan.",
                        &file.path,
                    ));
                }
            }
            let sources: Vec<String> = frontmatter
                .get_list("sources")
                .into_iter()
                .filter(|source| !source.trim().is_empty())
                .collect();
            if sources.is_empty() {
                return Err(manifest_semantic_error(
                    "Derived pages must include non-empty frontmatter sources.",
                    &file.path,
                ));
            }
            for source in &sources {
                if !source_ref_known(source, known_sources) {
                    return Err(source_ref_error(&file.path, source));
                }
            }
            if let Some(item) = planned_item {
                let manifest_sources = canonical_source_set(&sources, known_sources);
                let planned_sources = canonical_source_set(&item.source_ids, known_sources);
                if manifest_sources != planned_sources {
                    return Err(manifest_semantic_error(
                        "Manifest sources do not match the accepted CompilePlan sourceIds.",
                        &file.path,
                    ));
                }
            }
            if !has_human_readable_sources_section(&split.body) {
                return Err(manifest_semantic_error(
                    "Derived pages must include a human-readable > Sources: section.",
                    &file.path,
                ));
            }
            if source_mirror_risk(&file.path, &split.body, &sources) {
                return Err(BackendError::new(
                    "COMPILE_SOURCE_MIRROR_RISK",
                    "Compile output appears to mirror a source instead of creating a derived synthesis page.",
                    true,
                    true,
                )
                .with_details(serde_json::json!({ "path": file.path })));
            }
        }
        Ok(())
    }

    pub fn snapshot_wiki(
        context: &ProjectContext,
    ) -> Result<HashMap<String, String>, BackendError> {
        let store = FileStore;
        let mut hashes = HashMap::new();
        for absolute in store.list_markdown_files(&context.wiki_dir)? {
            let relative = context.to_project_relative(&absolute)?;
            hashes.insert(relative.clone(), store.file_hash(context, &relative)?);
        }
        Ok(hashes)
    }

    pub fn apply_manifest(
        context: &ProjectContext,
        manifest: &CompileManifest,
        accepted_plan: Option<&CompilePlan>,
        baseline: &HashMap<String, String>,
    ) -> Result<CompileApplyOutcome, BackendError> {
        let known_sources = Self::known_source_refs(context)?;
        Self::validate_manifest_semantics(context, manifest, accepted_plan, &known_sources)?;
        let store = FileStore;
        let mut affected_paths = Vec::new();
        let mut conflicts = Vec::new();
        for file in &manifest.files {
            let target = context.resolve_project_path(&file.path)?;
            if let Some(expected) = baseline.get(&file.path) {
                let needs_confirmation =
                    if !target.exists() || store.file_hash(context, &file.path)? != *expected {
                        true
                    } else {
                        std::fs::read_to_string(&target).map_err(|error| {
                            io_error("COMPILE_INPUT_READ_FAILED", error, &target)
                        })? != file.content
                    };
                if needs_confirmation {
                    conflicts.push(file.path.clone());
                }
            } else if target.exists() {
                conflicts.push(file.path.clone());
            }
        }
        for deletion in &manifest.deletions {
            conflicts.push(deletion.clone());
        }
        affected_paths.sort();
        conflicts.sort();
        conflicts.dedup();
        if !conflicts.is_empty() {
            return Ok(CompileApplyOutcome {
                affected_paths,
                conflicts,
            });
        }
        for file in &manifest.files {
            let mode = baseline
                .get(&file.path)
                .map(|expected| WriteMode::OverwriteIfHashMatches(expected.clone()))
                .unwrap_or(WriteMode::CreateNew);
            store.write_markdown_checked(context, &file.path, &file.content, mode)?;
            affected_paths.push(file.path.clone());
        }
        affected_paths.sort();
        Ok(CompileApplyOutcome {
            affected_paths,
            conflicts,
        })
    }

    pub fn apply_confirmed_manifest(
        context: &ProjectContext,
        manifest: &CompileManifest,
        accepted_plan: Option<&CompilePlan>,
        expected_current_hashes: &HashMap<String, String>,
    ) -> Result<Vec<String>, BackendError> {
        Self::apply_confirmed_manifest_with_policy(
            context,
            manifest,
            accepted_plan,
            expected_current_hashes,
            false,
        )
    }

    pub fn apply_confirmed_workflow_manifest(
        context: &ProjectContext,
        manifest: &CompileManifest,
        accepted_plan: Option<&CompilePlan>,
        expected_current_hashes: &HashMap<String, String>,
    ) -> Result<Vec<String>, BackendError> {
        Self::apply_confirmed_manifest_with_policy(
            context,
            manifest,
            accepted_plan,
            expected_current_hashes,
            true,
        )
    }

    fn apply_confirmed_manifest_with_policy(
        context: &ProjectContext,
        manifest: &CompileManifest,
        accepted_plan: Option<&CompilePlan>,
        expected_current_hashes: &HashMap<String, String>,
        allow_reviewable_deletions: bool,
    ) -> Result<Vec<String>, BackendError> {
        // Defense in depth: even on the confirmed-apply path, refuse any
        // write or deletion under the compile-protected wiki/sources/ subtree.
        let known_sources = Self::known_source_refs(context)?;
        Self::validate_manifest_semantics_with_policy(
            context,
            manifest,
            accepted_plan,
            &known_sources,
            allow_reviewable_deletions,
        )?;
        let store = FileStore;
        let mut affected = Vec::new();
        for file in &manifest.files {
            let target = context.resolve_project_path(&file.path)?;
            match expected_current_hashes.get(&file.path) {
                Some(expected)
                    if !target.exists() || store.file_hash(context, &file.path)? != *expected =>
                {
                    return Err(BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        "A confirmed page changed again.",
                        true,
                        true,
                    ));
                }
                None if target.exists() => {
                    return Err(BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        "A confirmed new page now exists.",
                        true,
                        true,
                    ));
                }
                _ => {}
            }
        }
        for deletion in &manifest.deletions {
            let target = context.resolve_project_path(deletion)?;
            match expected_current_hashes.get(deletion) {
                Some(expected)
                    if !target.exists() || store.file_hash(context, deletion)? != *expected =>
                {
                    return Err(BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        "A confirmed page changed again.",
                        true,
                        true,
                    ));
                }
                None if target.exists() => {
                    return Err(BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        "A confirmed deletion target appeared after review.",
                        true,
                        true,
                    ));
                }
                _ => {}
            }
        }
        for file in &manifest.files {
            if !is_safe_wiki_markdown(&file.path) {
                return Err(BackendError::new(
                    "COMPILE_PATH_INVALID",
                    "Compile output contains an unsafe path.",
                    false,
                    true,
                ));
            }
            let target = context.resolve_project_path(&file.path)?;
            let mode = match expected_current_hashes.get(&file.path) {
                Some(expected) => WriteMode::OverwriteIfHashMatches(expected.clone()),
                None if !target.exists() => WriteMode::CreateNew,
                None => unreachable!("confirmed paths were preflighted"),
            };
            if let Err(error) =
                store.write_markdown_checked(context, &file.path, &file.content, mode)
            {
                return Err(Self::apply_error_with_journal(error, &affected));
            }
            affected.push(file.path.clone());
        }
        for deletion in &manifest.deletions {
            if !is_safe_wiki_markdown(deletion) {
                return Err(BackendError::new(
                    "COMPILE_PATH_INVALID",
                    "Compile deletion contains an unsafe path.",
                    false,
                    true,
                ));
            }
            let target = context.resolve_project_path(deletion)?;
            let Some(expected) = expected_current_hashes.get(deletion) else {
                debug_assert!(!target.exists());
                continue;
            };
            let Some(parent) = target.parent() else {
                return Err(Self::apply_error_with_journal(
                    BackendError::new(
                        "COMPILE_PATH_INVALID",
                        "Compile deletion has no parent directory.",
                        false,
                        true,
                    ),
                    &affected,
                ));
            };
            let staged = parent.join(format!(".llm-wiki-delete-{}.tmp", uuid::Uuid::new_v4()));
            if let Err(error) = std::fs::rename(&target, &staged) {
                return Err(Self::apply_error_with_journal(
                    BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        format!("A confirmed deletion target could not be claimed safely: {error}"),
                        true,
                        true,
                    ),
                    &affected,
                ));
            }
            let staged_hash = std::fs::read(&staged)
                .map(|bytes| hash_bytes(&bytes))
                .map_err(|error| io_error("FILE_READ_FAILED", error, &staged));
            if staged_hash.as_deref() != Ok(expected.as_str()) {
                let restored = !target.exists() && std::fs::rename(&staged, &target).is_ok();
                let error = if restored {
                    BackendError::new(
                        "CONFIRMATION_STATE_MISMATCH",
                        "A confirmed deletion target changed immediately before deletion.",
                        true,
                        true,
                    )
                } else {
                    BackendError::new(
                        "WORKFLOW_APPLY_ROLLBACK_FAILED",
                        "A deletion race was detected and the claimed file was preserved for recovery.",
                        false,
                        true,
                    )
                    .with_details(serde_json::json!({
                        "path": deletion,
                        "recoveryPath": staged.to_string_lossy(),
                    }))
                };
                return Err(Self::apply_error_with_journal(error, &affected));
            }
            if let Err(error) = std::fs::remove_file(&staged) {
                if !target.exists() && std::fs::rename(&staged, &target).is_ok() {
                    return Err(Self::apply_error_with_journal(
                        BackendError::new("FILE_DELETE_FAILED", error.to_string(), true, false),
                        &affected,
                    ));
                }
                return Err(Self::apply_error_with_journal(
                    BackendError::new(
                        "WORKFLOW_APPLY_ROLLBACK_FAILED",
                        format!("The claimed deletion could not be removed or restored: {error}"),
                        false,
                        true,
                    )
                    .with_details(serde_json::json!({
                        "path": deletion,
                        "recoveryPath": staged.to_string_lossy(),
                    })),
                    &affected,
                ));
            }
            affected.push(deletion.clone());
        }
        affected.sort();
        Ok(affected)
    }

    fn apply_error_with_journal(mut error: BackendError, applied_paths: &[String]) -> BackendError {
        let original_details = error.details.take();
        error.details = Some(serde_json::json!({
            "appliedPaths": applied_paths,
            "originalDetails": original_details,
        }));
        error
    }

    pub fn candidate_diff(manifest: &CompileManifest) -> String {
        let mut diff = String::from("```diff\n");
        for file in &manifest.files {
            diff.push_str(&format!(
                "--- {} (current)\n+++ {} (candidate)\n",
                file.path, file.path
            ));
            for line in file.content.lines() {
                diff.push_str(&format!("+{line}\n"));
            }
        }
        for path in &manifest.deletions {
            diff.push_str(&format!("--- {path}\n+++ /dev/null\n"));
        }
        diff.push_str("```");
        diff
    }

    pub fn backup_outputs(
        context: &ProjectContext,
        manifest: &CompileManifest,
    ) -> Result<CompileBackup, BackendError> {
        Self::backup_workflow_outputs(context, manifest, &[])
    }

    pub fn backup_workflow_outputs(
        context: &ProjectContext,
        manifest: &CompileManifest,
        extra_paths: &[String],
    ) -> Result<CompileBackup, BackendError> {
        let mut paths: Vec<String> = manifest
            .files
            .iter()
            .map(|file| file.path.clone())
            .chain(manifest.deletions.iter().cloned())
            .chain(std::iter::once(".app/graph-cache.json".to_string()))
            .chain(extra_paths.iter().cloned())
            .collect();
        paths.sort();
        paths.dedup();
        let mut entries = Vec::with_capacity(paths.len());
        for relative in paths {
            let absolute = context.resolve_project_path(&relative)?;
            let bytes = if absolute.exists() {
                Some(
                    std::fs::read(&absolute)
                        .map_err(|error| io_error("COMPILE_BACKUP_FAILED", error, &absolute))?,
                )
            } else {
                None
            };
            entries.push((relative, bytes));
        }
        Ok(CompileBackup { entries })
    }

    pub fn restore_outputs(
        context: &ProjectContext,
        backup: &CompileBackup,
    ) -> Result<(), BackendError> {
        for (relative, bytes) in &backup.entries {
            let absolute = context.resolve_project_path(relative)?;
            match bytes {
                Some(bytes) => {
                    if let Some(parent) = absolute.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|error| io_error("COMPILE_ROLLBACK_FAILED", error, parent))?;
                    }
                    std::fs::write(&absolute, bytes)
                        .map_err(|error| io_error("COMPILE_ROLLBACK_FAILED", error, &absolute))?;
                }
                None if absolute.exists() => {
                    std::fs::remove_file(&absolute)
                        .map_err(|error| io_error("COMPILE_ROLLBACK_FAILED", error, &absolute))?;
                }
                None => {}
            }
        }
        Ok(())
    }

    pub fn restore_workflow_outputs_if_unchanged(
        context: &ProjectContext,
        backup: &CompileBackup,
        manifest: &CompileManifest,
        applied_paths: &[String],
    ) -> Result<(), BackendError> {
        let applied = applied_paths
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let generated = manifest
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.content.as_bytes()))
            .collect::<HashMap<_, _>>();
        let deletions = manifest
            .deletions
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        for (relative, baseline) in &backup.entries {
            if !applied.contains(relative.as_str()) {
                continue;
            }
            let absolute = context.resolve_project_path(relative)?;
            if relative.starts_with(".app/") {
                restore_backup_path(&absolute, baseline.as_deref())?;
                continue;
            }
            let workflow_value = if deletions.contains(relative.as_str()) {
                None
            } else {
                generated.get(relative.as_str()).copied()
            };
            rollback_wiki_path(relative, &absolute, baseline.as_deref(), workflow_value)?;
        }
        Ok(())
    }

    pub fn known_source_refs(context: &ProjectContext) -> Result<HashSet<String>, BackendError> {
        let references = Self::list_source_versions(context)?;
        let sources = if references.is_empty() {
            Vec::new()
        } else {
            Self::resolve_source_versions(context, &references)?
        };
        Ok(Self::known_source_refs_for_sources(&sources))
    }

    pub fn known_source_refs_for_sources(sources: &[ResolvedCompileSource]) -> HashSet<String> {
        let mut refs = HashSet::new();
        for source in sources {
            refs.insert(source.workspace_path.clone());
            if let Some(name) = Path::new(&source.workspace_path)
                .file_name()
                .and_then(|name| name.to_str())
            {
                refs.insert(name.to_string());
            }
        }
        refs
    }
}

fn read_agent_compile_plan(workspace: &Path) -> Result<CompilePlan, BackendError> {
    let plan_path = workspace.join("compile-plan.json");
    let raw = std::fs::read_to_string(&plan_path).map_err(|error| {
        BackendError::new(
            "COMPILE_PLAN_MISSING",
            format!(
                "Agent compile must write compile-plan.json before candidate files are accepted: {error}"
            ),
            true,
            false,
        )
        .with_details(serde_json::json!({ "path": plan_path.to_string_lossy() }))
    })?;
    CompileService::parse_plan(&raw)
}

fn validate_compile_plan(
    context: &ProjectContext,
    plan: &CompilePlan,
    baseline: &HashMap<String, String>,
    sources: &[ResolvedCompileSource],
) -> Result<(), BackendError> {
    let known_sources = CompileService::known_source_refs_for_sources(sources);
    let existing_pages = baseline.keys().cloned().collect::<Vec<_>>();
    CompileService::validate_plan(context, plan, &existing_pages, &known_sources)
}

fn select_compile_provider(
    explicit: Option<LlmProviderKind>,
    providers: &[LlmProviderConfig],
    secrets: &SecretService,
) -> Result<Option<LlmProviderConfig>, BackendError> {
    if let Some(kind) = explicit {
        let provider = providers
            .iter()
            .find(|provider| provider.enabled && provider.provider == kind)
            .cloned()
            .ok_or_else(|| {
                BackendError::new(
                    "LLM_PROVIDER_MISSING",
                    "The selected BYOK provider is not enabled.",
                    true,
                    true,
                )
            })?;
        if provider.provider.requires_secret() && secrets.get(provider.provider)?.is_none() {
            return Err(BackendError::new(
                "LLM_SECRET_MISSING",
                "The selected provider has no configured secret.",
                true,
                true,
            ));
        }
        return Ok(Some(provider));
    }
    for provider in providers.iter().filter(|provider| provider.enabled) {
        if !provider.provider.requires_secret() || secrets.get(provider.provider)?.is_some() {
            return Ok(Some(provider.clone()));
        }
    }
    Ok(None)
}

fn task_operation_error(message: String) -> BackendError {
    BackendError::new("TASK_OPERATION_FAILED", message, true, false)
}

fn extract_json_object<'a>(raw: &'a str, error_code: &str) -> Result<&'a str, BackendError> {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find("```json") {
        let rest = &trimmed[start + 7..];
        let end = rest.find("```").ok_or_else(|| {
            BackendError::new(error_code, "Unclosed JSON code fence.", true, false)
        })?;
        return Ok(rest[..end].trim());
    }
    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        return Ok(&trimmed[start..=end]);
    }
    Ok(trimmed)
}

fn validate_source_identity(value: &str) -> Result<(), BackendError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid_source_version());
    }
    Ok(())
}

fn invalid_source_version() -> BackendError {
    BackendError::new(
        "COMPILE_SOURCE_VERSION_INVALID",
        "A selected Source version is missing or its content hash no longer matches.",
        true,
        true,
    )
}

fn io_error(code: &str, error: std::io::Error, path: &Path) -> BackendError {
    BackendError::new(code, error.to_string(), true, false)
        .with_details(serde_json::json!({ "path": path.to_string_lossy() }))
}

fn compile_input_empty_error(
    index_exists: bool,
    confirmed_sources: &[String],
    relative_paths: &[String],
    empty_on_disk: &[String],
) -> BackendError {
    let (summary, details) = if !index_exists || confirmed_sources.is_empty() {
        (
            "No extracted Markdown originals were found under wiki/sources (or legacy \
             raw/extracted). No imports have been confirmed yet — confirm an import \
             preview before compiling."
                .to_string(),
            serde_json::json!({
                "stage": "no_confirmed_imports",
                "index_exists": index_exists,
                "confirmed_sources": confirmed_sources,
                "hint": "Run an import (raw/sources → preview → confirm); originals are promoted to wiki/sources/*.md.",
            }),
        )
    } else if relative_paths.is_empty() {
        (
            "No extracted Markdown originals were found under wiki/sources (or legacy \
             raw/extracted). Confirmed sources produced no source page (likely \
             image-only or unsupported sources, or extraction failed silently)."
                .to_string(),
            serde_json::json!({
                "stage": "no_extracted_markdown",
                "confirmed_sources": confirmed_sources,
                "hint": "Re-import these sources or add textual sources; verify wiki/sources/ is populated.",
            }),
        )
    } else {
        (
            "No extracted Markdown originals were found under wiki/sources (or legacy \
             raw/extracted). Confirmed source pages exist but are empty on disk."
                .to_string(),
            serde_json::json!({
                "stage": "extracted_files_empty",
                "confirmed_sources": confirmed_sources,
                "empty_on_disk": empty_on_disk,
                "hint": "Re-import these sources; their wiki/sources/*.md (or raw/extracted/*.md) pages are empty.",
            }),
        )
    };

    BackendError::new("COMPILE_INPUT_EMPTY", summary, true, true).with_details(details)
}

fn copy_workspace_file(source: &Path, target: &Path) -> Result<(), BackendError> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, parent))?;
    }
    std::fs::copy(source, target)
        .map(|_| ())
        .map_err(|error| io_error("COMPILE_WORKSPACE_FAILED", error, source))
}

fn hash_file(path: &Path) -> Result<String, BackendError> {
    let bytes =
        std::fs::read(path).map_err(|error| io_error("COMPILE_INPUT_READ_FAILED", error, path))?;
    Ok(hash_bytes(&bytes))
}

/// `wiki/sources/` is import-owned: it holds the verbatim extracted originals.
/// Compile may read and cite them but must never create, modify, or delete them.
fn is_compile_protected_path(raw: &str) -> bool {
    let normalized = raw.replace('\\', "/");
    normalized.eq_ignore_ascii_case("wiki/sources")
        || normalized
            .get(.."wiki/sources/".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("wiki/sources/"))
}

fn is_structural_page(raw: &str) -> bool {
    matches!(raw, "wiki/index.md" | "wiki/overview.md" | "wiki/log.md")
}

fn is_known_page_type(raw: &str) -> bool {
    matches!(
        raw,
        "entity" | "concept" | "synthesis" | "comparison" | "query" | "overview" | "index" | "log"
    )
}

fn page_type_as_str(page_type: CompilePageType) -> &'static str {
    match page_type {
        CompilePageType::Entity => "entity",
        CompilePageType::Concept => "concept",
        CompilePageType::Synthesis => "synthesis",
        CompilePageType::Comparison => "comparison",
        CompilePageType::Query => "query",
        CompilePageType::Overview => "overview",
        CompilePageType::Index => "index",
        CompilePageType::Log => "log",
    }
}

fn source_ref_known(raw: &str, known_sources: &HashSet<String>) -> bool {
    canonical_source_ref(raw, known_sources).is_some()
}

fn canonical_source_ref(raw: &str, known_sources: &HashSet<String>) -> Option<String> {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.is_empty() {
        return None;
    }
    if known_sources.contains(&normalized) {
        return Some(normalized);
    }
    let source_prefixed = format!("wiki/sources/{normalized}");
    if known_sources.contains(&source_prefixed) {
        return Some(source_prefixed);
    }
    let legacy_prefixed = format!("raw/extracted/{normalized}");
    if known_sources.contains(&legacy_prefixed) {
        return Some(legacy_prefixed);
    }
    None
}

fn canonical_source_set(raw: &[String], known_sources: &HashSet<String>) -> HashSet<String> {
    raw.iter()
        .filter_map(|source| canonical_source_ref(source, known_sources))
        .collect()
}

fn has_human_readable_sources_section(body: &str) -> bool {
    body.lines()
        .any(|line| line.trim_start().starts_with("> Sources:"))
}

fn source_mirror_risk(path: &str, body: &str, sources: &[String]) -> bool {
    let target_stem = Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if target_stem.is_empty() || sources.len() != 1 {
        return false;
    }
    let source_stem = Path::new(&sources[0])
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    target_stem == source_stem && body.to_ascii_lowercase().contains("summary")
}

fn protected_path_error(path: &str) -> BackendError {
    BackendError::new(
        "COMPILE_PROTECTED_PATH",
        "wiki/sources/ is import-owned; compile may read and cite these originals but cannot create, modify, or delete them.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "path": path }))
}

fn plan_error(message: &str) -> BackendError {
    BackendError::new("COMPILE_PLAN_INVALID", message, true, false)
}

fn plan_error_with_path(message: &str, path: &str) -> BackendError {
    plan_error(message).with_details(serde_json::json!({ "path": path }))
}

fn manifest_semantic_error(message: &str, path: &str) -> BackendError {
    BackendError::new("COMPILE_MANIFEST_SEMANTIC_INVALID", message, true, true)
        .with_details(serde_json::json!({ "path": path }))
}

fn source_ref_error(path: &str, source: &str) -> BackendError {
    BackendError::new(
        "COMPILE_SOURCE_REF_INVALID",
        "Compile output references a source that does not exist in wiki/sources/ or legacy raw/extracted/.",
        true,
        true,
    )
    .with_details(serde_json::json!({ "path": path, "source": source }))
}

fn restore_backup_path(path: &Path, baseline: Option<&[u8]>) -> Result<(), BackendError> {
    match baseline {
        Some(bytes) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| io_error("COMPILE_ROLLBACK_FAILED", error, parent))?;
            }
            std::fs::write(path, bytes)
                .map_err(|error| io_error("COMPILE_ROLLBACK_FAILED", error, path))
        }
        None if path.exists() => std::fs::remove_file(path)
            .map_err(|error| io_error("COMPILE_ROLLBACK_FAILED", error, path)),
        None => Ok(()),
    }
}

fn rollback_wiki_path(
    relative: &str,
    target: &Path,
    baseline: Option<&[u8]>,
    workflow_value: Option<&[u8]>,
) -> Result<(), BackendError> {
    let conflict = |recovery_path: Option<&Path>| {
        let mut details = serde_json::json!({ "path": relative });
        if let Some(path) = recovery_path {
            details["recoveryPath"] = serde_json::json!(path.to_string_lossy());
        }
        BackendError::new(
            "WORKFLOW_ROLLBACK_CONFLICT",
            "A Wiki file changed again while Update Wiki was rolling back; recovery material was preserved.",
            true,
            true,
        )
        .with_details(details)
    };

    let Some(expected) = workflow_value else {
        if target.exists() {
            return Err(conflict(None));
        }
        return match baseline {
            Some(bytes) => write_create_new(target, bytes).map_err(|_| conflict(None)),
            None => Ok(()),
        };
    };
    if !target.is_file() {
        return Err(conflict(None));
    }
    let parent = target.parent().ok_or_else(|| conflict(None))?;
    let staged = parent.join(format!(".llm-wiki-rollback-{}.tmp", uuid::Uuid::new_v4()));
    std::fs::rename(target, &staged).map_err(|_| conflict(None))?;
    let staged_matches = std::fs::read(&staged)
        .map(|bytes| bytes == expected)
        .unwrap_or(false);
    if !staged_matches {
        if !target.exists() && std::fs::rename(&staged, target).is_ok() {
            return Err(conflict(None));
        }
        return Err(conflict(Some(&staged)));
    }

    if let Some(bytes) = baseline {
        if write_create_new(target, bytes).is_err() {
            if !target.exists() && std::fs::rename(&staged, target).is_ok() {
                return Err(conflict(None));
            }
            return Err(conflict(Some(&staged)));
        }
    }
    if let Err(error) = std::fs::remove_file(&staged) {
        return Err(BackendError::new(
            "WORKFLOW_APPLY_ROLLBACK_FAILED",
            format!("Rollback restored the formal Wiki path but could not remove staging: {error}"),
            true,
            true,
        )
        .with_details(serde_json::json!({
            "path": relative,
            "recoveryPath": staged.to_string_lossy(),
        })));
    }
    Ok(())
}

fn write_create_new(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn is_safe_wiki_markdown(raw: &str) -> bool {
    if raw.contains('\\') || !raw.starts_with("wiki/") || !raw.ends_with(".md") {
        return false;
    }
    // Defense in depth: wiki/sources/ is compile-protected (see
    // is_compile_protected_path). Even without the dedicated check, the
    // generic safety predicate must refuse these paths.
    if is_compile_protected_path(raw) {
        return false;
    }
    let path = Path::new(raw);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::compile::{
        CompileAction, CompileFile, CompileManifest, CompilePageType, CompilePlan, CompilePlanItem,
        SourceVersionRef,
    };
    use crate::models::import_v2::{QualityLevel, QualityReport};
    use crate::models::paths::ProjectContext;
    use crate::services::import_v2::source_finalization::{
        finalize_source, CandidateMetadata, FinalizationInput,
    };
    use crate::services::import_v2::source_registry::{
        SourceArtifactRecord, SourceCandidateRecord, SourceIndex, SourceManifest, SourcePointer,
        SourceProvenance, SourceVersion, SOURCE_REGISTRY_SCHEMA_VERSION,
    };
    use std::collections::BTreeMap;
    use std::fs;

    #[test]
    fn compile_rejects_an_empty_extracted_markdown_directory() {
        let root = std::env::temp_dir().join(format!("compile-empty-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        let context = ProjectContext::new("project", root.clone());

        let error = CompileService::extracted_markdown_files(&context).unwrap_err();

        assert_eq!(error.code, "COMPILE_INPUT_EMPTY");
        assert!(error.message.contains("raw/extracted"));
        let details = error.details.expect("details present");
        assert_eq!(
            details.get("stage").and_then(|v| v.as_str()),
            Some("no_confirmed_imports")
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compile_ignores_unconfirmed_orphan_extracted_markdown() {
        let root = std::env::temp_dir().join(format!("compile-orphan-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(root.join("raw/extracted/confirmed.md"), "confirmed").unwrap();
        fs::write(root.join("raw/extracted/cancelled-preview.md"), "orphan").unwrap();
        fs::write(
            root.join(".app/source-index.json"),
            r#"{"sources":{"raw/sources/markdown/source.txt":["raw/extracted/confirmed.md"]}}"#,
        )
        .unwrap();
        let context = ProjectContext::new("project", root.clone());

        let files = CompileService::extracted_markdown_files(&context).unwrap();

        assert_eq!(files, vec![root.join("raw/extracted/confirmed.md")]);
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compile_rejects_confirmed_extracted_markdown_when_all_content_is_blank() {
        let root = std::env::temp_dir().join(format!("compile-blank-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(root.join("raw/extracted/blank.md"), " \n\t").unwrap();
        fs::write(
            root.join(".app/source-index.json"),
            r#"{"sources":{"raw/sources/markdown/blank.txt":["raw/extracted/blank.md"]}}"#,
        )
        .unwrap();
        let context = ProjectContext::new("project", root.clone());

        let error = CompileService::extracted_markdown_files(&context).unwrap_err();

        assert_eq!(error.code, "COMPILE_INPUT_EMPTY");
        let details = error.details.expect("details present");
        assert_eq!(
            details.get("stage").and_then(|v| v.as_str()),
            Some("extracted_files_empty")
        );
        let empty_count = details
            .get("empty_on_disk")
            .and_then(|v| v.as_array())
            .map(Vec::len);
        assert_eq!(empty_count, Some(1));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn compile_reports_when_confirmed_sources_have_no_extracted_markdown() {
        let root = std::env::temp_dir().join(format!("compile-no-md-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(
            root.join(".app/source-index.json"),
            // Source confirmed but points only at an image artifact, no *.md.
            r#"{"sources":{"raw/sources/image-only/source.pdf":["raw/extracted/source.png"]}}"#,
        )
        .unwrap();
        let context = ProjectContext::new("project", root.clone());

        let error = CompileService::extracted_markdown_files(&context).unwrap_err();

        assert_eq!(error.code, "COMPILE_INPUT_EMPTY");
        let details = error.details.expect("details present");
        assert_eq!(
            details.get("stage").and_then(|v| v.as_str()),
            Some("no_extracted_markdown")
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn byok_prompt_reads_cjk_extracted_markdown_from_compile_workspace() {
        let root = std::env::temp_dir().join(format!("compile-input-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(root.join("purpose.md"), "# Purpose").unwrap();
        fs::write(root.join("schema.md"), "# Schema").unwrap();
        fs::write(root.join("raw/extracted/资料.md"), "# 提取内容\n\n关键事实").unwrap();
        fs::write(
            root.join(".app/source-index.json"),
            r#"{"sources":{"raw/sources/markdown/资料.txt":["raw/extracted/资料.md"]}}"#,
        )
        .unwrap();
        let context = ProjectContext::new("project", root.clone());
        let references = CompileService::list_source_versions(&context).unwrap();
        let sources = CompileService::resolve_source_versions(&context, &references).unwrap();
        let workspace = CompileService::create_workspace_for_sources(
            &context,
            &format!("prompt-test-{}", uuid::Uuid::new_v4()),
            &sources,
        )
        .unwrap();

        let prompt = CompileService::provider_prompt(&workspace, "zh-CN").unwrap();

        assert!(prompt.contains("raw/extracted/资料.md"));
        assert!(prompt.contains("# 提取内容"));
        assert!(prompt.contains("关键事实"));
        fs::remove_dir_all(root).ok();
        fs::remove_dir_all(workspace).ok();
    }

    #[test]
    fn apply_manifest_rejects_external_edits_before_writing_any_candidate() {
        let root = std::env::temp_dir().join(format!("compile-conflict-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/index.md"), "before").unwrap();
        fs::write(root.join("wiki/overview.md"), "overview").unwrap();
        fs::write(root.join("wiki/log.md"), "log").unwrap();
        let context = ProjectContext::new("project", root.clone());
        let baseline = CompileService::snapshot_wiki(&context).unwrap();
        fs::write(root.join("wiki/index.md"), "external").unwrap();

        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "candidate"),
                CompileFile::new("wiki/overview.md", "overview 2"),
                CompileFile::new("wiki/log.md", "log 2"),
            ],
            deletions: vec![],
            summary: "compile".into(),
        };
        let result = CompileService::apply_manifest(&context, &manifest, None, &baseline).unwrap();
        assert_eq!(
            result.conflicts,
            vec!["wiki/index.md", "wiki/log.md", "wiki/overview.md"]
        );
        assert_eq!(
            fs::read_to_string(root.join("wiki/index.md")).unwrap(),
            "external"
        );
        assert_eq!(
            fs::read_to_string(root.join("wiki/overview.md")).unwrap(),
            "overview"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn apply_manifest_requires_confirmation_before_overwriting_existing_page() {
        let root = std::env::temp_dir().join(format!("compile-overwrite-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/index.md"), "current index").unwrap();
        fs::write(root.join("wiki/overview.md"), "current overview").unwrap();
        fs::write(root.join("wiki/log.md"), "current log").unwrap();
        let context = ProjectContext::new("project", root.clone());
        let baseline = CompileService::snapshot_wiki(&context).unwrap();
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "generated index"),
                CompileFile::new("wiki/overview.md", "current overview"),
                CompileFile::new("wiki/log.md", "current log"),
            ],
            deletions: vec![],
            summary: "compile".into(),
        };

        let outcome = CompileService::apply_manifest(&context, &manifest, None, &baseline).unwrap();

        assert_eq!(outcome.conflicts, vec!["wiki/index.md"]);
        assert_eq!(
            fs::read_to_string(root.join("wiki/index.md")).unwrap(),
            "current index"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn parses_json_manifest_from_fenced_provider_response() {
        let raw = "result:\n```json\n{\"files\":[{\"path\":\"wiki/index.md\",\"content\":\"i\"},{\"path\":\"wiki/overview.md\",\"content\":\"o\"},{\"path\":\"wiki/log.md\",\"content\":\"l\"}],\"deletions\":[],\"summary\":\"ok\"}\n```";
        let manifest = CompileService::parse_manifest(raw).unwrap();
        assert_eq!(manifest.summary, "ok");
        assert_eq!(manifest.files.len(), 3);
    }

    #[test]
    fn validate_manifest_rejects_deletions() {
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "# Index"),
                CompileFile::new("wiki/overview.md", "# Overview"),
                CompileFile::new("wiki/log.md", "# Log"),
            ],
            deletions: vec!["wiki/concepts/old.md".into()],
            summary: "delete".into(),
        };

        let error = CompileService::validate_manifest(&manifest)
            .expect_err("compile deletions are forbidden");

        assert_eq!(error.code, "COMPILE_DELETE_FORBIDDEN");
    }

    #[test]
    fn workflow_policy_accepts_reviewable_derived_deletions_without_weakening_legacy_compile() {
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "# Index"),
                CompileFile::new("wiki/overview.md", "# Overview"),
                CompileFile::new("wiki/log.md", "# Log"),
            ],
            deletions: vec!["wiki/concepts/old.md".into()],
            summary: "rename".into(),
        };

        CompileService::validate_workflow_manifest(&manifest)
            .expect("workflow deletions remain gated by review");
        assert_eq!(
            CompileService::validate_manifest(&manifest)
                .expect_err("legacy compile must still reject deletions")
                .code,
            "COMPILE_DELETE_FORBIDDEN"
        );

        let workspace = Path::new("compile-workspace");
        let workflow_prompt = CompileService::compile_prompt_with_policy(
            workspace,
            "en",
            CompileGenerationPolicy::WorkflowReviewableDeletes,
        );
        let legacy_prompt = CompileService::compile_prompt(workspace, "en");
        assert!(workflow_prompt.contains("review-only candidates"));
        assert!(legacy_prompt.contains("Never delete existing pages"));
    }

    #[test]
    fn workflow_rollback_restores_only_paths_recorded_as_applied() {
        let root =
            std::env::temp_dir().join(format!("compile-rollback-journal-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("wiki/concepts")).unwrap();
        fs::write(root.join("wiki/index.md"), "# Index\n").unwrap();
        fs::write(root.join("wiki/overview.md"), "# Overview\n").unwrap();
        fs::write(root.join("wiki/log.md"), "# Log\n").unwrap();
        fs::write(root.join("wiki/concepts/old.md"), "# Old\n").unwrap();
        let context = ProjectContext::new("project", root.clone());
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "# New index\n"),
                CompileFile::new("wiki/overview.md", "# Overview\n"),
                CompileFile::new("wiki/log.md", "# Log\n"),
            ],
            deletions: vec!["wiki/concepts/old.md".into()],
            summary: "delete".into(),
        };
        let backup = CompileService::backup_outputs(&context, &manifest).unwrap();

        fs::remove_file(root.join("wiki/concepts/old.md")).unwrap();
        CompileService::restore_workflow_outputs_if_unchanged(&context, &backup, &manifest, &[])
            .unwrap();

        assert!(!root.join("wiki/concepts/old.md").exists());
        fs::write(root.join("wiki/index.md"), "# New index\n").unwrap();
        CompileService::restore_workflow_outputs_if_unchanged(
            &context,
            &backup,
            &manifest,
            &["wiki/index.md".into(), "wiki/concepts/old.md".into()],
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(root.join("wiki/index.md")).unwrap(),
            "# Index\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("wiki/concepts/old.md")).unwrap(),
            "# Old\n"
        );
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn parses_json_plan_from_fenced_provider_response() {
        let raw = "plan:\n```json\n{\"summary\":\"compile plan\",\"items\":[{\"action\":\"create\",\"targetPath\":\"wiki/concepts/agent-memory.md\",\"pageType\":\"concept\",\"sourceIds\":[\"wiki/sources/source-a.md\"],\"affectedExistingPages\":[],\"reason\":\"New concept from source.\"}],\"globalRiskFlags\":[]}\n```";

        let plan = CompileService::parse_plan(raw).unwrap();

        assert_eq!(plan.summary, "compile plan");
        assert_eq!(plan.items[0].action, CompileAction::Create);
        assert_eq!(plan.items[0].page_type, CompilePageType::Concept);
    }

    #[test]
    fn validate_plan_rejects_no_source_create_and_merge_without_target() {
        let context = temp_project_context("compile-plan-invalid");
        let known_sources = HashSet::from(["wiki/sources/source-a.md".to_string()]);
        let no_source = CompilePlan {
            summary: "bad".into(),
            items: vec![CompilePlanItem {
                action: CompileAction::Create,
                target_path: "wiki/concepts/no-source.md".into(),
                page_type: CompilePageType::Concept,
                source_ids: vec![],
                affected_existing_pages: vec![],
                reason: "Missing source evidence.".into(),
                risk_flags: vec![],
            }],
            global_risk_flags: vec![],
        };

        let error = CompileService::validate_plan(&context, &no_source, &[], &known_sources)
            .expect_err("derived create without source ids must fail");
        assert_eq!(error.code, "COMPILE_PLAN_INVALID");

        let merge_without_existing = CompilePlan {
            summary: "bad merge".into(),
            items: vec![CompilePlanItem {
                action: CompileAction::Merge,
                target_path: "wiki/concepts/existing.md".into(),
                page_type: CompilePageType::Concept,
                source_ids: vec!["wiki/sources/source-a.md".into()],
                affected_existing_pages: vec![],
                reason: "Same thesis as existing page.".into(),
                risk_flags: vec![],
            }],
            global_risk_flags: vec![],
        };

        let error = CompileService::validate_plan(
            &context,
            &merge_without_existing,
            &["wiki/concepts/existing.md".into()],
            &known_sources,
        )
        .expect_err("merge must name an existing target in affected pages");
        assert_eq!(error.code, "COMPILE_PLAN_INVALID");
        fs::remove_dir_all(context.root).ok();
    }

    #[test]
    fn validate_plan_rejects_protected_paths_and_structural_only_plan() {
        let context = temp_project_context("compile-plan-paths");
        let known_sources = HashSet::from(["wiki/sources/source-a.md".to_string()]);
        let protected = CompilePlan {
            summary: "bad path".into(),
            items: vec![CompilePlanItem {
                action: CompileAction::Update,
                target_path: "wiki/sources/source-a.md".into(),
                page_type: CompilePageType::Entity,
                source_ids: vec!["wiki/sources/source-a.md".into()],
                affected_existing_pages: vec!["wiki/sources/source-a.md".into()],
                reason: "Should never touch originals.".into(),
                risk_flags: vec![],
            }],
            global_risk_flags: vec![],
        };
        let error = CompileService::validate_plan(&context, &protected, &[], &known_sources)
            .expect_err("wiki/sources plan target must fail");
        assert_eq!(error.code, "COMPILE_PROTECTED_PATH");

        let structural_only = CompilePlan {
            summary: "only navigation".into(),
            items: ["wiki/index.md", "wiki/overview.md", "wiki/log.md"]
                .iter()
                .map(|target| CompilePlanItem {
                    action: CompileAction::Update,
                    target_path: (*target).into(),
                    page_type: if *target == "wiki/log.md" {
                        CompilePageType::Log
                    } else if *target == "wiki/index.md" {
                        CompilePageType::Index
                    } else {
                        CompilePageType::Overview
                    },
                    source_ids: vec![],
                    affected_existing_pages: vec![(*target).into()],
                    reason: "Navigation refresh.".into(),
                    risk_flags: vec![],
                })
                .collect(),
            global_risk_flags: vec![],
        };
        let error = CompileService::validate_plan(&context, &structural_only, &[], &known_sources)
            .expect_err("compile cannot plan only structural pages");
        assert_eq!(error.code, "COMPILE_PLAN_INVALID");
        fs::remove_dir_all(context.root).ok();
    }

    #[test]
    fn validate_manifest_semantics_rejects_missing_frontmatter_and_sources() {
        let context = temp_project_context("compile-manifest-frontmatter");
        let known_sources = HashSet::from(["wiki/sources/source-a.md".to_string()]);
        let manifest = valid_manifest_with(CompileFile::new(
            "wiki/concepts/no-frontmatter.md",
            "# No Frontmatter\n\n> Sources: [[sources/source-a]]",
        ));

        let error =
            CompileService::validate_manifest_semantics(&context, &manifest, None, &known_sources)
                .expect_err("derived pages require frontmatter");
        assert_eq!(error.code, "COMPILE_MANIFEST_SEMANTIC_INVALID");

        let manifest = valid_manifest_with(CompileFile::new(
            "wiki/concepts/no-sources.md",
            "---\ntype: concept\nsources: []\n---\n# No Sources\n\n> Sources: [[sources/source-a]]",
        ));
        let error =
            CompileService::validate_manifest_semantics(&context, &manifest, None, &known_sources)
                .expect_err("derived pages require non-empty sources");
        assert_eq!(error.code, "COMPILE_MANIFEST_SEMANTIC_INVALID");
        fs::remove_dir_all(context.root).ok();
    }

    #[test]
    fn validate_manifest_semantics_rejects_bad_source_refs_missing_source_section_and_mirror_risk()
    {
        let context = temp_project_context("compile-manifest-source-refs");
        let known_sources = HashSet::from(["wiki/sources/source-a.md".to_string()]);
        let bad_ref = valid_manifest_with(CompileFile::new(
            "wiki/concepts/bad-ref.md",
            "---\ntype: concept\nsources: [missing.md]\n---\n# Bad Ref\n\n> Sources: [[sources/missing]]",
        ));
        let error =
            CompileService::validate_manifest_semantics(&context, &bad_ref, None, &known_sources)
                .expect_err("unknown source refs must fail");
        assert_eq!(error.code, "COMPILE_SOURCE_REF_INVALID");

        let missing_section = valid_manifest_with(CompileFile::new(
            "wiki/concepts/missing-section.md",
            "---\ntype: concept\nsources: [source-a.md]\n---\n# Missing Source Section\n\nBody.",
        ));
        let error = CompileService::validate_manifest_semantics(
            &context,
            &missing_section,
            None,
            &known_sources,
        )
        .expect_err("human-readable Sources section is required");
        assert_eq!(error.code, "COMPILE_MANIFEST_SEMANTIC_INVALID");

        let mirror = valid_manifest_with(CompileFile::new(
            "wiki/concepts/source-a.md",
            "---\ntype: concept\nsources: [source-a.md]\n---\n# Source A\n\nThis page is a summary of source-a.\n\n> Sources: [[sources/source-a]]",
        ));
        let error =
            CompileService::validate_manifest_semantics(&context, &mirror, None, &known_sources)
                .expect_err("source mirror pages must fail");
        assert_eq!(error.code, "COMPILE_SOURCE_MIRROR_RISK");
        fs::remove_dir_all(context.root).ok();
    }

    #[test]
    fn accepted_plan_requires_manifest_coverage_type_and_source_match() {
        let context = temp_project_context("compile-plan-manifest-match");
        let known_sources = HashSet::from([
            "wiki/sources/source-a.md".to_string(),
            "wiki/sources/source-b.md".to_string(),
        ]);
        let plan = CompilePlan {
            summary: "create concept".into(),
            items: vec![CompilePlanItem {
                action: CompileAction::Create,
                target_path: "wiki/concepts/agent-memory.md".into(),
                page_type: CompilePageType::Concept,
                source_ids: vec!["wiki/sources/source-a.md".into()],
                affected_existing_pages: vec![],
                reason: "New concept.".into(),
                risk_flags: vec![],
            }],
            global_risk_flags: vec![],
        };
        let structural_only = CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "# Index"),
                CompileFile::new("wiki/overview.md", "# Overview"),
                CompileFile::new("wiki/log.md", "# Log"),
            ],
            deletions: vec![],
            summary: "bad".into(),
        };
        let error = CompileService::validate_manifest_semantics(
            &context,
            &structural_only,
            Some(&plan),
            &known_sources,
        )
        .expect_err("accepted plan requires planned derived files");
        assert_eq!(error.code, "COMPILE_MANIFEST_SEMANTIC_INVALID");

        let wrong_type = valid_manifest_with(CompileFile::new(
            "wiki/concepts/agent-memory.md",
            "---\ntype: entity\nsources: [source-a.md]\n---\n# Agent Memory\n\n> Sources: [[sources/source-a]]",
        ));
        let error = CompileService::validate_manifest_semantics(
            &context,
            &wrong_type,
            Some(&plan),
            &known_sources,
        )
        .expect_err("manifest type must match plan pageType");
        assert_eq!(error.code, "COMPILE_MANIFEST_SEMANTIC_INVALID");

        let wrong_source = valid_manifest_with(CompileFile::new(
            "wiki/concepts/agent-memory.md",
            "---\ntype: concept\nsources: [source-b.md]\n---\n# Agent Memory\n\n> Sources: [[sources/source-b]]",
        ));
        let error = CompileService::validate_manifest_semantics(
            &context,
            &wrong_source,
            Some(&plan),
            &known_sources,
        )
        .expect_err("manifest sources must match plan sourceIds");
        assert_eq!(error.code, "COMPILE_MANIFEST_SEMANTIC_INVALID");
        fs::remove_dir_all(context.root).ok();
    }

    #[test]
    fn apply_manifest_rejects_semantic_failure_before_writing_any_file() {
        let root = std::env::temp_dir().join(format!("compile-no-write-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::write(root.join("wiki/index.md"), "current index").unwrap();
        fs::write(root.join("wiki/overview.md"), "current overview").unwrap();
        fs::write(root.join("wiki/log.md"), "current log").unwrap();
        let context = ProjectContext::new("project", root.clone());
        let baseline = CompileService::snapshot_wiki(&context).unwrap();
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "generated index"),
                CompileFile::new("wiki/overview.md", "generated overview"),
                CompileFile::new("wiki/log.md", "generated log"),
                CompileFile::new(
                    "wiki/concepts/bad.md",
                    "---\ntype: concept\nsources: []\n---\n# Bad\n\n> Sources: none",
                ),
            ],
            deletions: vec![],
            summary: "bad compile".into(),
        };

        let error = CompileService::apply_manifest(&context, &manifest, None, &baseline)
            .expect_err("semantic failure must abort before conflict or write handling");

        assert_eq!(error.code, "COMPILE_MANIFEST_SEMANTIC_INVALID");
        assert_eq!(
            fs::read_to_string(root.join("wiki/index.md")).unwrap(),
            "current index"
        );
        assert!(!root.join("wiki/concepts/bad.md").exists());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn byok_agent_and_skill_include_shared_decision_rules() {
        let root = temp_compile_workspace("compile-prompt-contract");
        let byok_plan = CompileService::provider_plan_prompt(&root, "en").unwrap();
        let byok_manifest = CompileService::provider_manifest_prompt(&root, "en", None).unwrap();
        let agent = CompileService::compile_prompt(&root, "en");
        let skill = include_str!("../../templates/skills/wiki-ingest/SKILL.md");

        for text in [&byok_plan, &byok_manifest, &agent, skill] {
            assert!(text.contains("Decision Rules"));
            assert!(text.contains("create"));
            assert!(text.contains("update"));
            assert!(text.contains("merge"));
            assert!(text.contains("see-also"));
            assert!(text.contains("conflict"));
            assert!(text.contains("Cascade"));
            assert!(text.contains("wiki/sources/"));
            assert!(text.contains("> Sources:"));
            assert!(text.contains("same core thesis"));
            assert!(text.contains("never after a source filename"));
        }
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn known_source_refs_only_admits_confirmed_legacy_extracted_sources() {
        let root = std::env::temp_dir().join(format!("compile-sources-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("raw/extracted")).unwrap();
        fs::create_dir_all(root.join("wiki/sources")).unwrap();
        fs::create_dir_all(root.join(".app")).unwrap();
        fs::write(root.join("raw/extracted/confirmed.md"), "# Confirmed").unwrap();
        fs::write(root.join("raw/extracted/orphan.md"), "# Orphan").unwrap();
        fs::write(root.join("wiki/sources/promoted.md"), "# Promoted").unwrap();
        fs::write(
            root.join(".app/source-index.json"),
            r#"{"sources":{"raw/sources/a.txt":["raw/extracted/confirmed.md"]}}"#,
        )
        .unwrap();
        let context = ProjectContext::new("project", root.clone());

        let refs = CompileService::known_source_refs(&context).unwrap();

        assert!(refs.contains("raw/extracted/confirmed.md"));
        assert!(refs.contains("confirmed.md"));
        assert!(refs.contains("wiki/sources/promoted.md"));
        assert!(refs.contains("promoted.md"));
        assert!(!refs.contains("raw/extracted/orphan.md"));
        assert!(!refs.contains("orphan.md"));
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn conflict_resolution_keeps_current_paths_but_applies_uncontested_candidates() {
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/conflict.md", "generated conflict"),
                CompileFile::new("wiki/safe.md", "generated safe"),
                CompileFile::new("wiki/index.md", "# Index"),
                CompileFile::new("wiki/overview.md", "# Overview"),
                CompileFile::new("wiki/log.md", "# Log"),
            ],
            deletions: vec![],
            summary: "compile".to_string(),
        };

        let resolved = CompileService::resolve_conflict_manifest(
            &manifest,
            &["wiki/conflict.md".to_string()],
            crate::models::compile::CompileConflictResolution::KeepCurrent,
            &[],
        )
        .unwrap();

        assert!(resolved
            .files
            .iter()
            .any(|file| file.path == "wiki/safe.md"));
        assert!(!resolved
            .files
            .iter()
            .any(|file| file.path == "wiki/conflict.md"));
        assert!(resolved.deletions.is_empty());
    }

    #[test]
    fn manual_conflict_resolution_requires_content_for_every_conflicting_path() {
        let manifest = CompileManifest {
            files: vec![
                CompileFile::new("wiki/conflict.md", "generated"),
                CompileFile::new("wiki/index.md", "# Index"),
                CompileFile::new("wiki/overview.md", "# Overview"),
                CompileFile::new("wiki/log.md", "# Log"),
            ],
            deletions: vec![],
            summary: "compile".to_string(),
        };
        let conflicts = vec!["wiki/conflict.md".to_string()];

        let missing = CompileService::resolve_conflict_manifest(
            &manifest,
            &conflicts,
            crate::models::compile::CompileConflictResolution::ManualMerge,
            &[],
        )
        .expect_err("manual merge must cover deletions too");
        assert_eq!(missing.code, "COMPILE_MANUAL_MERGE_INCOMPLETE");

        let resolved = CompileService::resolve_conflict_manifest(
            &manifest,
            &conflicts,
            crate::models::compile::CompileConflictResolution::ManualMerge,
            &[CompileFile::new("wiki/conflict.md", "merged")],
        )
        .unwrap();
        assert_eq!(resolved.files.len(), 4);
        assert!(resolved.deletions.is_empty());
        assert_eq!(
            resolved
                .files
                .iter()
                .find(|file| file.path == "wiki/conflict.md")
                .map(|file| file.content.as_str()),
            Some("merged")
        );
    }

    fn temp_project_context(label: &str) -> ProjectContext {
        let root = std::env::temp_dir().join(format!("{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("wiki")).unwrap();
        ProjectContext::new("project", root)
    }

    fn temp_compile_workspace(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("wiki")).unwrap();
        fs::create_dir_all(root.join("wiki/sources")).unwrap();
        fs::write(root.join("purpose.md"), "# Purpose").unwrap();
        fs::write(root.join("schema.md"), "# Schema").unwrap();
        fs::write(root.join("wiki/index.md"), "# Index").unwrap();
        fs::write(root.join("wiki/overview.md"), "# Overview").unwrap();
        fs::write(root.join("wiki/log.md"), "# Log").unwrap();
        fs::write(root.join("wiki/sources/source-a.md"), "# Source A").unwrap();
        root
    }

    fn valid_manifest_with(file: CompileFile) -> CompileManifest {
        CompileManifest {
            files: vec![
                CompileFile::new("wiki/index.md", "# Index"),
                CompileFile::new("wiki/overview.md", "# Overview"),
                CompileFile::new("wiki/log.md", "# Log"),
                file,
            ],
            deletions: vec![],
            summary: "compile".into(),
        }
    }

    fn seed_v2_source(
        context: &ProjectContext,
        source_id: &str,
        version_id: &str,
        file_name: &str,
        content: &str,
    ) -> SourceVersionRef {
        let files = FileStore;
        let content_hash = hash_bytes(content.as_bytes());
        let wiki_path = format!("wiki/sources/local/{file_name}");
        let baseline_path = format!(".app/source-artifacts/{source_id}/{version_id}/baseline.md");
        let raw_path = format!("raw/sources/{source_id}/{version_id}/original.txt");
        let quality = QualityReport {
            level: QualityLevel::Pass,
            metrics: Vec::new(),
            warnings: Vec::new(),
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
        };
        let candidate = CandidateMetadata {
            source_kind: "local_document".into(),
            title: file_name.into(),
            canonical_url: None,
            platform: None,
            platform_content_id: None,
            author: None,
            published_at: None,
            language: Some("zh-CN".into()),
        };
        let finalized = finalize_source(FinalizationInput {
            candidate_markdown: content.as_bytes(),
            candidate: &candidate,
            source_id,
            version_id,
            content_hash: &content_hash,
            imported_at: "2026-07-26T00:00:00Z",
            quality: &quality,
            restricted: false,
        })
        .unwrap();
        let wiki_absolute = context.resolve_project_path(&wiki_path).unwrap();
        fs::create_dir_all(wiki_absolute.parent().unwrap()).unwrap();
        fs::write(&wiki_absolute, &finalized.bytes).unwrap();
        let baseline_absolute = context.resolve_project_path(&baseline_path).unwrap();
        fs::create_dir_all(baseline_absolute.parent().unwrap()).unwrap();
        fs::write(&baseline_absolute, &finalized.bytes).unwrap();
        let raw_absolute = context.resolve_project_path(&raw_path).unwrap();
        fs::create_dir_all(raw_absolute.parent().unwrap()).unwrap();
        fs::write(&raw_absolute, content).unwrap();
        let manifest = SourceManifest {
            schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
            source_id: source_id.into(),
            source_kind: "local_document".into(),
            current_version_id: version_id.into(),
            wiki_path: wiki_path.clone(),
            aliases: Vec::new(),
            origins: vec![format!("file:/{file_name}")],
            canonical_url: None,
            platform: None,
            platform_content_id: None,
            title: file_name.into(),
            author: None,
            published_at: None,
            imported_at: "2026-07-26T00:00:00Z".into(),
            language: Some("zh-CN".into()),
            versions: vec![SourceVersion {
                version_id: version_id.into(),
                content_hash: content_hash.clone(),
                raw_evidence: vec![SourceArtifactRecord {
                    path: raw_path,
                    sha256: content_hash.clone(),
                    size_bytes: content.len() as u64,
                    kind: "source_snapshot".into(),
                }],
                assets: Vec::new(),
                baseline_path,
                candidate: SourceCandidateRecord {
                    markdown_hash: content_hash.clone(),
                    title: file_name.into(),
                    source_kind: "local_document".into(),
                    canonical_url: None,
                    platform: None,
                    platform_content_id: None,
                    author: None,
                    published_at: None,
                    language: Some("zh-CN".into()),
                },
                provenance: SourceProvenance {
                    locator: format!("file:/{file_name}"),
                    route: "native".into(),
                    engine_id: "fixture".into(),
                    engine_version: "1".into(),
                },
                quality,
                created_at: "2026-07-26T00:00:00Z".into(),
                human_edit_hash: Some(finalized.human_edit_hash),
                checkpoint: None,
            }],
            compiled_consumptions: Vec::new(),
            restricted_content: false,
            restricted_identity_summary: None,
            timeline: Vec::new(),
        };
        files
            .write_json_atomic(
                context,
                &format!(".app/sources/{source_id}.json"),
                &manifest,
            )
            .unwrap();
        SourceVersionRef {
            source_id: source_id.into(),
            version_id: version_id.into(),
            content_hash,
        }
    }

    #[test]
    fn v2_compile_resolves_only_explicit_hash_bound_sources() {
        let context = temp_project_context("compile-v2-explicit");
        fs::write(context.root.join("purpose.md"), "# Purpose").unwrap();
        fs::write(context.root.join("schema.md"), "# Schema").unwrap();
        let selected = seed_v2_source(&context, "source-a", "version-a", "资料甲.md", "# 甲");
        let unselected = seed_v2_source(&context, "source-b", "version-b", "other.md", "# B");
        fs::write(
            context.root.join("wiki/related.md"),
            "sources:\n  - wiki/sources/local/资料甲.md",
        )
        .unwrap();
        fs::write(
            context.root.join("wiki/unrelated.md"),
            "sources:\n  - wiki/sources/local/other.md",
        )
        .unwrap();
        let pointer_a = SourcePointer {
            source_id: selected.source_id.clone(),
            version_id: selected.version_id.clone(),
        };
        let pointer_b = SourcePointer {
            source_id: unselected.source_id.clone(),
            version_id: unselected.version_id.clone(),
        };
        FileStore
            .write_json_atomic(
                &context,
                ".app/source-index-v2.json",
                &SourceIndex {
                    schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
                    by_content_hash: BTreeMap::from([
                        (selected.content_hash.clone(), pointer_a.clone()),
                        (unselected.content_hash.clone(), pointer_b.clone()),
                    ]),
                    by_locator: BTreeMap::from([
                        ("file:/a".into(), pointer_a),
                        ("file:/b".into(), pointer_b),
                    ]),
                },
            )
            .unwrap();

        let resolved =
            CompileService::resolve_source_versions(&context, std::slice::from_ref(&selected))
                .unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].reference, selected);
        let workspace =
            CompileService::create_workspace_for_sources(&context, "explicit-test", &resolved)
                .unwrap();
        assert!(workspace.join("wiki/sources/local/资料甲.md").is_file());
        assert!(!workspace.join("wiki/sources/local/other.md").exists());
        assert!(workspace.join("wiki/related.md").is_file());
        assert!(!workspace.join("wiki/unrelated.md").exists());

        let mut wrong_hash = selected.clone();
        wrong_hash.content_hash = "0".repeat(64);
        let error = CompileService::resolve_source_versions(&context, &[wrong_hash]).unwrap_err();
        assert_eq!(error.code, "COMPILE_SOURCE_VERSION_INVALID");
        fs::write(&resolved[0].absolute_path, "# externally changed").unwrap();
        let error =
            CompileService::resolve_source_versions(&context, std::slice::from_ref(&selected))
                .unwrap_err();
        assert_eq!(error.code, "COMPILE_SOURCE_VERSION_INVALID");
        fs::remove_dir_all(&context.root).ok();
        fs::remove_dir_all(workspace).ok();
    }

    #[test]
    fn agent_source_mutation_is_an_explicit_compile_failure() {
        for case in ["modify", "delete", "add", "case_variant_add"] {
            let workspace = temp_compile_workspace(&format!("compile-protected-{case}"));
            let baseline = HashMap::new();
            let protected = CompileService::snapshot_workspace_sources(&workspace).unwrap();
            match case {
                "modify" => {
                    fs::write(workspace.join("wiki/sources/source-a.md"), "# Mutated").unwrap();
                }
                "delete" => {
                    fs::remove_file(workspace.join("wiki/sources/source-a.md")).unwrap();
                }
                "add" => {
                    fs::write(workspace.join("wiki/sources/injected.md"), "# Injected").unwrap();
                }
                "case_variant_add" => {
                    fs::create_dir_all(workspace.join("wiki/SOURCES")).unwrap();
                    fs::write(workspace.join("wiki/SOURCES/injected.md"), "# Injected").unwrap();
                }
                _ => unreachable!(),
            }

            let error = CompileService::manifest_from_workspace_protected(
                &workspace, &baseline, &protected,
            )
            .unwrap_err();
            assert_eq!(
                error.code, "COMPILE_SOURCE_MUTATION_FORBIDDEN",
                "case {case} must fail explicitly"
            );
            fs::remove_dir_all(workspace).ok();
        }
    }

    #[test]
    fn compile_source_guard_is_separator_and_ascii_case_insensitive() {
        for (path, protected) in [
            ("wiki/sources/a.md", true),
            ("wiki\\sources\\a.md", true),
            ("wiki/SOURCES/a.md", true),
            ("WIKI/Sources/a.md", true),
            ("wiki/source/a.md", false),
            ("wiki/sources-old/a.md", false),
        ] {
            assert_eq!(
                is_compile_protected_path(path),
                protected,
                "unexpected protection decision for {path}"
            );
        }
    }
}
