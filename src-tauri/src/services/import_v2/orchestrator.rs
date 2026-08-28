use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use crate::app_state::{ProjectExecutionLease, ProjectTaskMutationPermit, ProjectWritePermit};
use crate::errors::{
    BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_PANICKED, IMPORT_V2_ITEM_NOT_FOUND,
    IMPORT_V2_STATE_INVALID, IMPORT_V2_WORK_ITEM_STALE,
};
use crate::models::import_v2::{
    AttemptOutcome, AttemptRecord, ImportAsrProfile, ImportInput, ImportInputKind, ImportIssue,
    ImportItem, ImportItemAuthorizationSnapshot, ImportItemPage, ImportItemPageFilter,
    ImportItemStatus, ImportMediaAuthorization, ImportMediaAuthorizationKind, ImportRecoveryAction,
    ImportResourceMode, ImportSession, ImportSessionOverview, ImportSessionStatus, ImportStage,
    ImportWorkItemSnapshot, SourceIdentity,
};
use crate::models::import_v2_agent::AgentAssistanceTrigger;
use crate::models::import_v2_agent::AgentCandidate;
use crate::models::import_v2_file::FileFormat;
use crate::models::paths::ProjectContext;
use crate::models::task::{BackendTask, TaskResult, TaskResultReference, TaskStatus, TaskType};
use crate::services::import_v2::capability_pack::ResolvedCapabilityPack;
use crate::services::import_v2::engine::{
    describe_engine, engine_panicked_error, execute_engine, execute_engine_with_progress,
    validate_engine_result, EngineContinuation, EngineOperation, EngineProgress, EngineRegistry,
    EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::file_router::{
    AttemptOutcome as RouteOutcome, CapabilitySnapshot, FileRoutePlanner, QualityFloor,
};
use crate::services::import_v2::generic_web_engine::GenericWebEngine;
use crate::services::import_v2::local_media_engine::{
    NativeMediaCompanionEngine, NativeSubtitleEngine,
};
use crate::services::import_v2::native_file_engine::{
    NativeCsvPackageEngine, NativeFileEngine, NativeStructuredFileEngine,
};
use crate::services::import_v2::pack_engine::PackProcessEngine;
use crate::services::import_v2::quality_gate::QualityGate;
use crate::services::import_v2::transaction::FileTransaction;
use crate::services::import_v2::web_target_store::WebTargetStore;
use crate::services::import_v2::wechat_web_engine::WechatWebEngine;
use crate::services::import_v2::SessionStore;
use crate::services::FileStore;
use crate::services::SecretService;
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;
use crate::utils::safe_project_dir::{remove_project_file, BoundProjectMutationRoot};
use sha2::{Digest, Sha256};

pub(crate) fn import_batch_operation_session_id(task: &BackendTask) -> Option<&str> {
    task.import_operation_session_id()
}

pub(crate) fn is_import_batch_operation_task(task: &BackendTask) -> bool {
    task.is_import_operation()
}

#[cfg(feature = "performance-observers")]
#[derive(Debug, Clone, Default, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportLockWaitSnapshot {
    pub acquisitions: u64,
    pub total_wait_nanos: u64,
    pub max_wait_nanos: u64,
    pub waits_over_50_ms: u64,
}

#[cfg(feature = "performance-observers")]
pub struct ImportLockWaitObservation {
    snapshot: Arc<Mutex<ImportLockWaitSnapshot>>,
}

#[cfg(feature = "performance-observers")]
impl ImportLockWaitObservation {
    pub fn snapshot(&self) -> ImportLockWaitSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

pub struct ImportV2Service {
    pub(super) sessions: SessionStore,
    pub(super) engines: EngineRegistry,
    quality: QualityGate,
    pub(super) mutation_lock: Mutex<()>,
    agent_candidate_action_lock: Mutex<()>,
    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    source_ai_active: Mutex<HashSet<String>>,
    pub(super) web_targets: Arc<WebTargetStore>,
    connector_profiles_root: Arc<RwLock<Option<PathBuf>>>,
    target_reservation_registry: Mutex<
        HashMap<
            String,
            std::sync::Weak<Mutex<crate::services::import_v2::NewSourceTargetReservations>>,
        >,
    >,
    #[cfg(feature = "performance-observers")]
    lock_wait_observer: Mutex<Option<Arc<Mutex<ImportLockWaitSnapshot>>>>,
}

pub(crate) struct ImportSessionRecovery {
    pub session: ImportSession,
    pub changed_items: Vec<ImportItem>,
}

#[derive(Debug, Clone)]
pub struct BatchOperationPreparation {
    pub replaced_task_ids: Vec<String>,
    pub snapshots: Vec<ImportWorkItemSnapshot>,
    pub(crate) target_reservations:
        Arc<Mutex<crate::services::import_v2::NewSourceTargetReservations>>,
}

fn work_item_snapshots(
    session: &ImportSession,
    item_ids: &[String],
    revision_increment: u64,
) -> Result<Vec<ImportWorkItemSnapshot>, BackendError> {
    let item_by_id = session
        .items
        .iter()
        .map(|item| (item.item_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let authorization_by_item = session.media_authorizations.iter().fold(
        HashMap::<&str, ImportItemAuthorizationSnapshot>::new(),
        |mut index, authorization| {
            let entry = index
                .entry(authorization.item_id.as_str())
                .or_insert_with(|| ImportItemAuthorizationSnapshot {
                    local_ocr_authorized: false,
                    asr_profile: None,
                    recognition_language: None,
                    local_asr_authorized: false,
                });
            match authorization.kind {
                ImportMediaAuthorizationKind::Ocr => entry.local_ocr_authorized = true,
                ImportMediaAuthorizationKind::Asr => {
                    entry.local_asr_authorized = true;
                    entry.asr_profile.clone_from(&authorization.asr_profile);
                    entry
                        .recognition_language
                        .clone_from(&authorization.language);
                }
            }
            index
        },
    );
    item_ids
        .iter()
        .map(|item_id| {
            let item = item_by_id
                .get(item_id.as_str())
                .copied()
                .ok_or_else(item_not_found)?;
            Ok(ImportWorkItemSnapshot {
                item_id: item.item_id.clone(),
                expected_item_revision: item.item_revision.saturating_add(revision_increment),
                input: item.input.clone(),
                selected_subtitle: item.selected_subtitle.clone(),
                media_authorization: authorization_by_item
                    .get(item_id.as_str())
                    .cloned()
                    .unwrap_or(ImportItemAuthorizationSnapshot {
                        local_ocr_authorized: false,
                        asr_profile: None,
                        recognition_language: None,
                        local_asr_authorized: false,
                    }),
                authenticated_retry: item.authenticated_retry,
                resource_mode: session.resource_mode.clone(),
            })
        })
        .collect()
}

#[derive(Clone, PartialEq, Eq)]
struct NewSourceReservationFingerprint {
    input: ImportInput,
    title: String,
    markdown: crate::models::import_v2::ImportArtifact,
    source_snapshot: crate::models::import_v2::ImportArtifact,
    assets: Vec<crate::models::import_v2::ImportArtifact>,
}

fn new_source_reservation_fingerprint(
    item: &ImportItem,
) -> Option<NewSourceReservationFingerprint> {
    let preview = item.preview.as_ref()?;
    if !item.selected
        || item.status != ImportItemStatus::PreviewReady
        || !preview.resolution.as_ref().is_some_and(|resolution| {
            resolution.kind == crate::models::import_v2::ImportResolutionKind::NewSource
        })
    {
        return None;
    }
    Some(NewSourceReservationFingerprint {
        input: item.input.clone(),
        title: preview.title.clone(),
        markdown: preview.markdown.clone(),
        source_snapshot: preview.source_snapshot.clone(),
        assets: preview.assets.clone(),
    })
}

impl Default for ImportV2Service {
    fn default() -> Self {
        Self::with_secret_service(SecretService::default())
    }
}

impl ImportV2Service {
    fn authorize_media_for_session_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        kind: ImportMediaAuthorizationKind,
        asr_profile: Option<ImportAsrProfile>,
        language: Option<String>,
        expected_item: Option<&ImportItem>,
    ) -> Result<(), BackendError> {
        let language = language
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        if language.as_ref().is_some_and(|value| {
            value.len() > 32
                || !value
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        }) {
            return Err(BackendError::new(
                IMPORT_V2_STATE_INVALID,
                "Recognition language must be a short language tag.",
                false,
                true,
            ));
        }
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item = session
            .items
            .iter()
            .find(|item| item.item_id == item_id)
            .ok_or_else(item_not_found)?;
        if expected_item.is_some_and(|expected| item != expected) {
            return Err(BackendError::new(
                "IMPORT_V2_CAPABILITY_REQUIREMENT_STALE",
                "The import item changed before media authorization could be committed.",
                true,
                false,
            ));
        }
        if !matches!(
            item.status,
            ImportItemStatus::WaitingAuthorization
                | ImportItemStatus::WaitingCapability
                | ImportItemStatus::Failed
        ) {
            return Err(BackendError::new(
                IMPORT_V2_STATE_INVALID,
                "Media recognition can be authorized only while this import item is waiting.",
                false,
                true,
            ));
        }
        session
            .media_authorizations
            .retain(|authorization| authorization.item_id != item_id || authorization.kind != kind);
        session.media_authorizations.push(ImportMediaAuthorization {
            item_id: item_id.to_string(),
            kind: kind.clone(),
            authorized_at: chrono::Utc::now().to_rfc3339(),
            asr_profile: (kind == ImportMediaAuthorizationKind::Asr)
                .then_some(asr_profile.unwrap_or_default()),
            language: (kind == ImportMediaAuthorizationKind::Asr)
                .then_some(language)
                .flatten(),
        });
        persist_derived(&self.sessions, context, files, session)
    }

    pub fn find_unfinished_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
    ) -> Result<Option<String>, BackendError> {
        self.sessions.find_unfinished_session(context, files)
    }

    pub fn with_secret_service(secrets: SecretService) -> Self {
        let engines = EngineRegistry::default();
        let web_targets = Arc::new(WebTargetStore::new(secrets));
        let connector_profiles_root = Arc::new(RwLock::new(None));
        engines
            .register(Arc::new(NativeFileEngine::default()))
            .expect("the built-in native file engine identifier is unique");
        engines
            .register(Arc::new(NativeCsvPackageEngine))
            .expect("the built-in CSV package engine identifier is unique");
        engines
            .register(Arc::new(NativeSubtitleEngine))
            .expect("the built-in local subtitle engine identifier is unique");
        engines
            .register(Arc::new(NativeMediaCompanionEngine))
            .expect("the built-in local media companion engine identifier is unique");
        for (engine_id, route) in [
            ("builtin.pdf-text", "pdf.text"),
            ("builtin.office-docx", "office.modern.docx"),
            ("builtin.office-xlsx", "office.modern.xlsx"),
            ("builtin.office-pptx", "office.modern.pptx"),
        ] {
            engines
                .register(Arc::new(NativeStructuredFileEngine::new(engine_id, route)))
                .expect("the built-in structured file engine identifier is unique");
        }
        for (engine_id, route) in [
            ("builtin.web-http", "web.generic.readability"),
            ("builtin.web-xiaohongshu", "web.xiaohongshu.note"),
            ("builtin.web-douyin", "web.douyin.video"),
            ("builtin.web-bilibili", "web.bilibili.video"),
        ] {
            engines
                .register(Arc::new(GenericWebEngine::new(
                    web_targets.clone(),
                    engine_id,
                    route,
                )))
                .expect("the built-in generic web engine identifier is unique");
        }
        engines
            .register(Arc::new(WechatWebEngine::new(web_targets.clone())))
            .expect("the built-in WeChat web engine identifier is unique");
        Self {
            sessions: SessionStore::default(),
            engines,
            quality: QualityGate::default(),
            mutation_lock: Mutex::new(()),
            agent_candidate_action_lock: Mutex::new(()),
            source_ai_active: Mutex::new(HashSet::new()),
            web_targets,
            connector_profiles_root,
            target_reservation_registry: Mutex::new(HashMap::new()),
            #[cfg(feature = "performance-observers")]
            lock_wait_observer: Mutex::new(None),
        }
    }

    pub(crate) fn target_reservations_for_session(
        &self,
        session: &ImportSession,
    ) -> Result<Arc<Mutex<crate::services::import_v2::NewSourceTargetReservations>>, BackendError>
    {
        let mut registry = self
            .target_reservation_registry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.retain(|_, reservations| reservations.strong_count() > 0);
        if let Some(reservations) = registry
            .get(&session.session_id)
            .and_then(std::sync::Weak::upgrade)
        {
            reservations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .absorb_session(session)?;
            return Ok(reservations);
        }
        let reservations = Arc::new(Mutex::new(
            crate::services::import_v2::NewSourceTargetReservations::from_session(session)?,
        ));
        registry.insert(session.session_id.clone(), Arc::downgrade(&reservations));
        Ok(reservations)
    }

    #[cfg(feature = "performance-observers")]
    pub fn observe_lock_waits(&self) -> ImportLockWaitObservation {
        let snapshot = Arc::new(Mutex::new(ImportLockWaitSnapshot::default()));
        *self
            .lock_wait_observer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(snapshot.clone());
        ImportLockWaitObservation { snapshot }
    }

    #[cfg(feature = "performance-observers")]
    pub fn hold_mutation_lock_for_observation(
        &self,
        duration: std::time::Duration,
        acquired: Option<&std::sync::Barrier>,
    ) {
        let _guard = self
            .lock()
            .expect("synthetic observer lock must be available");
        if let Some(acquired) = acquired {
            acquired.wait();
        }
        std::thread::sleep(duration);
    }

    /// Configure the app-owned persistent connector profile root once the
    /// Tauri app data directory is available. Capability engines receive the
    /// same handle, so installed browser packs can reuse a profile across
    /// imports and app restarts without putting cookies in project files.
    pub fn set_connector_profiles_root(&self, root: PathBuf) {
        if let Ok(mut current) = self.connector_profiles_root.write() {
            *current = Some(root);
        }
    }
    pub fn store_web_target(
        &self,
        target: &crate::services::import_v2::url_policy::SessionWebTarget,
    ) -> Result<String, BackendError> {
        self.web_targets.store(target)
    }
    pub fn delete_web_target(&self, reference: &str) -> Result<(), BackendError> {
        self.web_targets.delete(reference)
    }
    pub fn resolve_web_target(
        &self,
        locator: &str,
        public: Option<&str>,
    ) -> Result<crate::services::import_v2::url_policy::SessionWebTarget, BackendError> {
        self.web_targets.resolve(locator, public)
    }
    pub fn store_web_collection(
        &self,
        project_id: &str,
        session_id: &str,
        source_url: String,
        platform: String,
        title: String,
        items: Vec<(
            String,
            crate::services::import_v2::url_policy::SessionWebTarget,
            String,
        )>,
    ) -> Result<
        (
            String,
            crate::services::import_v2::web_target_store::CollectionPage,
        ),
        BackendError,
    > {
        self.web_targets
            .store_collection(project_id, session_id, source_url, platform, title, items)
    }
    pub fn load_web_collection_page(
        &self,
        collection_ref: &str,
        project_id: &str,
        session_id: &str,
        cursor: &str,
        load_all: bool,
    ) -> Result<crate::services::import_v2::web_target_store::CollectionPage, BackendError> {
        self.web_targets.load_collection_page(
            collection_ref,
            project_id,
            session_id,
            cursor,
            load_all,
        )
    }
    pub fn resolve_web_collection_selection(
        &self,
        collection_ref: &str,
        project_id: &str,
        session_id: &str,
        selected_item_refs: &[String],
    ) -> Result<crate::services::import_v2::web_target_store::CollectionSelection, BackendError>
    {
        self.web_targets.resolve_collection_selection(
            collection_ref,
            project_id,
            session_id,
            selected_item_refs,
        )
    }
    pub fn delete_web_collection(&self, collection_ref: &str) -> Result<(), BackendError> {
        self.web_targets.delete_collection(collection_ref)
    }
    pub fn authorize_private_target(
        &self,
        grant: crate::services::import_v2::url_policy::PrivateTargetGrant,
    ) -> Result<String, BackendError> {
        self.web_targets.authorize_private(grant)
    }
    pub fn authorize_bilibili_asr(
        &self,
        grant: crate::services::import_v2::web_target_store::BilibiliAsrGrant,
    ) -> Result<(), BackendError> {
        self.web_targets.authorize_bilibili_asr(grant)
    }
    pub fn take_bilibili_asr(
        &self,
        project_id: &str,
        session_id: &str,
        item_id: &str,
        expected_request_url: &str,
    ) -> Result<Option<crate::services::import_v2::web_target_store::BilibiliAsrGrant>, BackendError>
    {
        self.web_targets
            .take_bilibili_asr(project_id, session_id, item_id, expected_request_url)
    }
    pub fn bind_authenticated_profiles(
        &self,
        project_id: &str,
        session_id: &str,
        item_ids: &[String],
        profile: &std::path::Path,
    ) -> Result<(), BackendError> {
        self.web_targets
            .bind_authenticated_profiles(project_id, session_id, item_ids, profile)
    }

    pub fn unbind_authenticated_profiles(
        &self,
        project_id: &str,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<(), BackendError> {
        self.web_targets
            .unbind_authenticated_profiles(project_id, session_id, item_ids)
    }

    fn enable_remote_media_retention_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.input.kind != ImportInputKind::Url
                || matches!(
                    item.status,
                    ImportItemStatus::Inspecting
                        | ImportItemStatus::Extracting
                        | ImportItemStatus::Validating
                        | ImportItemStatus::Committing
                        | ImportItemStatus::Completed
                )
            {
                return Err(BackendError::new(
                    crate::errors::IMPORT_V2_STATE_INVALID,
                    "Remote media retention is unavailable in the current item state.",
                    false,
                    true,
                ));
            }
            item.input.media_save_mode = crate::models::import_v2::MediaSaveMode::PreserveOriginal;
            item.issue = None;
            Ok(())
        })
    }

    fn mark_authenticated_login_group_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_ids: &[String],
        account_summary: Option<&str>,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        if item_ids.is_empty() {
            return Err(BackendError::new(
                crate::errors::IMPORT_V2_STATE_INVALID,
                "An authenticated login group cannot be empty.",
                false,
                true,
            ));
        }
        let summary = account_summary
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        for item_id in item_ids {
            let item = session
                .items
                .iter_mut()
                .find(|item| item.item_id == *item_id)
                .ok_or_else(|| {
                    BackendError::new(
                        crate::errors::IMPORT_V2_STATE_INVALID,
                        "An authenticated login group item was not found.",
                        false,
                        true,
                    )
                })?;
            if item.status != ImportItemStatus::WaitingLogin
                || item.input.kind != ImportInputKind::Url
            {
                return Err(BackendError::new(
                    crate::errors::IMPORT_V2_STATE_INVALID,
                    "Only waiting-login URL items can join an authenticated login group.",
                    false,
                    true,
                ));
            }
            item.authenticated_retry = true;
            item.authenticated_identity_summary.clone_from(&summary);
        }
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.sessions.save(context, files, &session)?;
        Ok(session)
    }

    fn clear_authenticated_login_group_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        for item in session
            .items
            .iter_mut()
            .filter(|item| item_ids.contains(&item.item_id))
        {
            item.authenticated_retry = false;
            item.authenticated_identity_summary = None;
        }
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.sessions.save(context, files, &session)?;
        Ok(session)
    }

    fn cancel_queued_item_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.status != ImportItemStatus::Queued {
                return Err(BackendError::new(
                    crate::errors::IMPORT_V2_STATE_INVALID,
                    "Only queued import items can be cancelled before a task starts.",
                    false,
                    true,
                ));
            }
            transition_item(item, ImportItemStatus::Cancelled)?;
            item.task_id = None;
            item.progress = None;
            Ok(())
        })
    }

    /// A batch item owns only its persisted item fact.  It must never cancel
    /// the shared operation token used by its siblings.
    fn cancel_batch_item_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if matches!(
                item.status,
                ImportItemStatus::Completed | ImportItemStatus::Committing
            ) {
                return Err(BackendError::new(
                    IMPORT_V2_STATE_INVALID,
                    "This import item can no longer be cancelled.",
                    false,
                    true,
                ));
            }
            transition_item(item, ImportItemStatus::Cancelled)?;
            item.task_id = None;
            item.progress = None;
            Ok(())
        })
    }

    fn skip_item_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item = find_item_mut(&mut session, item_id)?;
        if !matches!(
            item.status,
            ImportItemStatus::Queued
                | ImportItemStatus::WaitingCapability
                | ImportItemStatus::WaitingLogin
                | ImportItemStatus::PreviewReady
                | ImportItemStatus::NeedsMerge
                | ImportItemStatus::Failed
        ) {
            return Err(BackendError::new(
                IMPORT_V2_STATE_INVALID,
                "Only queued, waiting, ready, or failed import items can be skipped.",
                false,
                true,
            ));
        }
        if let Some(task_id) = item.task_id.as_deref() {
            if !is_batch_operation_task(tasks, task_id)
                && tasks.get_task(task_id).is_some_and(|task| {
                    !matches!(
                        task.status,
                        TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::Cancelled
                    )
                })
            {
                task_call(tasks.cancel_task(task_id))?;
            }
        }
        transition_item(item, ImportItemStatus::Skipped)?;
        item.selected = false;
        item.task_id = None;
        item.progress = None;
        item.preview = None;
        item.issue = None;
        crate::services::import_v2::commit::refresh_new_source_wiki_targets(
            context,
            files,
            &mut session,
        )?;
        let item = session
            .items
            .iter()
            .find(|item| item.item_id == item_id)
            .cloned()
            .ok_or_else(item_not_found)?;
        persist_derived(&self.sessions, context, files, session)?;
        remove_clipboard_session_input(context, session_id, &item.input);
        Ok(item)
    }
    fn create_session_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        mode: ImportResourceMode,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        if let Some(session_id) = self.find_unfinished_session(context, files)? {
            return self.sessions.load(context, files, &session_id);
        }
        self.sessions.create(context, files, mode)
    }
    fn add_inputs_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        inputs: Vec<ImportInput>,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        self.sessions.add_inputs(context, files, session_id, inputs)
    }

    pub fn completed_collection_fingerprints(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        source_url: &str,
        platform: &str,
    ) -> std::collections::HashMap<String, String> {
        self.sessions
            .completed_collection_fingerprints(context, files, source_url, platform)
    }

    fn add_collection_inputs_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        inputs: Vec<crate::services::import_v2::session_store::CollectionImportInput>,
        source_url: String,
        platform: String,
        title: String,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        self.sessions.add_collection_inputs(
            context, files, session_id, inputs, source_url, platform, title,
        )
    }

    /// Stage user-provided text inside the V2 session workspace and register
    /// it as a normal immutable file input. The session copy is deleted on
    /// cancel/skip/commit; a confirmed import retains only the normal raw
    /// Source evidence and its content-addressed clipboard origin.
    fn add_text_input_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        display_name: &str,
        content: &str,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let session = self.sessions.load(context, files, session_id)?;
        SessionStore::ensure_accepts_new_items(&session)?;
        let name = if display_name.trim().is_empty() {
            "text-import.md"
        } else {
            display_name.trim()
        };
        let extension = Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("md");
        let relative = format!(
            "{}/inputs/{}.{}",
            session_relative_root(context, session_id)?,
            uuid::Uuid::new_v4(),
            extension
        );
        let path = context.resolve_project_path(&relative)?;
        let bytes = content.as_bytes();
        let mut transaction = FileTransaction::new_for_project(&context.root);
        transaction.write_new(&path, bytes)?;
        transaction.commit()?;
        let canonical_path = path.canonicalize().map_err(|error| {
            BackendError::new(
                "IMPORT_V2_TEXT_STAGE_FAILED",
                error.to_string(),
                true,
                false,
            )
        })?;
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            BackendError::new(
                "IMPORT_V2_TEXT_STAGE_FAILED",
                error.to_string(),
                true,
                false,
            )
        })?;
        let modified_nanos = metadata.modified().ok().and_then(|value| {
            value
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|duration| duration.as_nanos())
        });
        let digest = Sha256::digest(bytes);
        let content_locator = format!("clipboard:sha256:{digest:x}");
        let magic = Sha256::digest(&bytes[..bytes.len().min(8192)]);
        self.sessions.add_inputs(
            context,
            files,
            session_id,
            vec![ImportInput {
                kind: ImportInputKind::ClipboardText,
                display_name: name.to_string(),
                locator: relative.clone(),
                normalized_locator: Some(content_locator),
                source_identity: Some(SourceIdentity {
                    canonical_path: canonical_path.to_string_lossy().into_owned(),
                    size_bytes: metadata.len(),
                    modified_nanos,
                    file_id: None,
                    sha256: format!("{digest:x}"),
                    magic: format!("{magic:x}"),
                }),
                media_save_mode: Default::default(),
            }],
        )
    }
    pub fn load_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        if crate::services::import_v2::commit::backfill_missing_new_source_wiki_targets(
            context,
            files,
            &mut session,
        )? {
            self.sessions.save(context, files, &session)?;
        }
        // The record is a coarse checkpoint; individual item JSON is the
        // authoritative lifecycle fact and can advance between summary writes.
        session.status = derive_session_status(&session.items);
        Ok(session)
    }

    /// Read the durable session snapshot without migration, recovery, or
    /// timestamp side effects. Maintenance belongs to an explicit task.
    pub fn read_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
    ) -> Result<ImportSession, BackendError> {
        let mut session = self.sessions.load(context, files, session_id)?;
        session.status = derive_session_status(&session.items);
        Ok(session)
    }

    pub fn read_session_overview(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
    ) -> Result<ImportSessionOverview, BackendError> {
        self.sessions.read_overview(context, files, session_id)
    }

    pub fn list_session_items(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        filter: ImportItemPageFilter,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<ImportItemPage, BackendError> {
        self.sessions
            .list_items(context, files, session_id, filter, cursor, limit)
    }

    /// Focused item read for worker safe points. It deliberately avoids
    /// reconstructing a complete session for every queued operation.
    pub fn load_item(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.sessions.load_item(context, files, session_id, item_id)
    }

    pub fn ensure_session_accepts_inputs(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
    ) -> Result<(), BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let session = self.sessions.load(context, files, session_id)?;
        SessionStore::ensure_accepts_new_items(&session)
    }

    fn set_discovery_task_id_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        task_id: Option<String>,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        if task_id.is_some() {
            SessionStore::ensure_accepts_new_items(&session)?;
        }
        session.discovery_task_id = task_id;
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.sessions.save(context, files, &session)?;
        Ok(session)
    }

    /// Persist the task-to-item relationship before the worker is spawned.
    /// This closes the small restart window in which a task already exists
    /// but the item still looks unclaimed in the durable session file.
    fn bind_item_task_ids_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        bindings: &[(String, String)],
    ) -> Result<Vec<ImportWorkItemSnapshot>, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let index = session
            .items
            .iter()
            .enumerate()
            .map(|(position, item)| (item.item_id.clone(), position))
            .collect::<HashMap<_, _>>();
        self.bind_item_task_ids_from_snapshot(
            context,
            files,
            session_id,
            &mut session,
            &index,
            bindings,
        )
    }

    fn bind_item_task_ids_from_snapshot(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        session: &mut ImportSession,
        index: &HashMap<String, usize>,
        bindings: &[(String, String)],
    ) -> Result<Vec<ImportWorkItemSnapshot>, BackendError> {
        self.bind_item_task_ids_from_snapshot_with_cancel(
            context,
            files,
            session_id,
            session,
            index,
            bindings,
            || false,
        )
    }

    fn bind_item_task_ids_from_snapshot_with_cancel<F>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        session: &mut ImportSession,
        index: &HashMap<String, usize>,
        bindings: &[(String, String)],
        mut should_cancel: F,
    ) -> Result<Vec<ImportWorkItemSnapshot>, BackendError>
    where
        F: FnMut() -> bool,
    {
        let mut seen = HashSet::with_capacity(bindings.len());
        for (item_id, task_id) in bindings {
            if should_cancel() {
                return Err(cancelled_error());
            }
            if !seen.insert(item_id.as_str()) {
                return Err(task_error("Import item bindings must be unique."));
            }
            let position = *index.get(item_id).ok_or_else(item_not_found)?;
            let item = &mut session.items[position];
            if !is_batch_claimable(item) {
                return Err(task_error(
                    "Import item is already claimed by another task.",
                ));
            }
            if item.status == ImportItemStatus::Queued
                && item
                    .task_id
                    .as_deref()
                    .is_some_and(|bound| bound != task_id)
            {
                return Err(task_error(
                    "Import item is already claimed by another task.",
                ));
            }
        }

        let mut originals = Vec::with_capacity(bindings.len());
        let mut replacements = Vec::with_capacity(bindings.len());
        for (item_id, task_id) in bindings {
            if should_cancel() {
                return Err(cancelled_error());
            }
            let position = *index.get(item_id).expect("bindings were validated above");
            let item = &mut session.items[position];
            let before = item.clone();
            item.task_id = Some(task_id.clone());
            originals.push(before);
            replacements.push(item.clone());
        }
        self.sessions.write_item_cohort_if_unchanged_with_cancel(
            context,
            files,
            session_id,
            &originals,
            &replacements,
            should_cancel,
        )?;
        work_item_snapshots(
            session,
            &bindings
                .iter()
                .map(|(item_id, _)| item_id.clone())
                .collect::<Vec<_>>(),
            1,
        )
    }

    /// Create the persisted operation before expensive cohort preparation so
    /// callers can return an observable, cancellable task immediately.
    fn create_batch_operation_task_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<BackendTask, BackendError> {
        if item_ids.is_empty() {
            return Err(BackendError::new(
                "IMPORT_BATCH_EMPTY",
                "Choose at least one import item before starting a batch.",
                false,
                true,
            ));
        }
        let source_label = if item_ids.len() == 1 {
            Some(
                self.sessions
                    .load_item(context, files, session_id, &item_ids[0])?
                    .input
                    .display_name,
            )
        } else {
            None
        };
        let title = source_label
            .as_ref()
            .map(|label| format!("Import {label}"))
            .unwrap_or_else(|| format!("Import {} sources", item_ids.len()));
        tasks
            .create_project_import_operation_task(
                context.project_id.clone(),
                context.root.clone(),
                import_operation_task_state_root(context)?,
                title,
                session_id.to_string(),
                item_ids.len() as u64,
                source_label,
            )
            .map_err(|error| task_error(&error))
    }

    /// Validate and atomically claim an already-created operation cohort.
    /// Replaced task ids are returned so callers can settle superseded login
    /// or attention operations after the new claim is durable.
    fn prepare_batch_operation_unchecked<F>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        task_id: &str,
        item_ids: &[String],
        mut should_cancel: F,
    ) -> Result<BatchOperationPreparation, BackendError>
    where
        F: FnMut() -> bool,
    {
        if should_cancel() {
            return Err(cancelled_error());
        }
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        if should_cancel() {
            return Err(cancelled_error());
        }
        let index = session
            .items
            .iter()
            .enumerate()
            .map(|(position, item)| (item.item_id.clone(), position))
            .collect::<HashMap<_, _>>();
        let mut unique = HashSet::with_capacity(item_ids.len());
        let mut replaced_task_ids = HashSet::new();
        for item_id in item_ids {
            if should_cancel() {
                return Err(cancelled_error());
            }
            if !unique.insert(item_id.as_str()) {
                return Err(task_error("Import item ids must be unique."));
            }
            let Some(position) = index.get(item_id) else {
                return Err(item_not_found());
            };
            let item = &session.items[*position];
            if !is_batch_claimable(item) {
                return Err(task_error(
                    "Import item is already claimed by another operation.",
                ));
            }
            if let Some(previous) = item.task_id.as_ref().filter(|id| id.as_str() != task_id) {
                if let Some(previous_task) = tasks.get_task(previous) {
                    if !matches!(
                        previous_task.status,
                        TaskStatus::WaitingForConfirmation
                            | TaskStatus::Succeeded
                            | TaskStatus::Failed
                            | TaskStatus::Cancelled
                            | TaskStatus::Interrupted
                    ) {
                        return Err(task_error(
                            "Import item is owned by another active operation.",
                        ));
                    }
                }
                replaced_task_ids.insert(previous.clone());
            }
        }
        // A waiting operation may own a mixed cohort while login/capability
        // recovery resumes only a subset. Settle the old task only when every
        // item it still owns is part of this new atomic claim; otherwise it
        // remains the truthful attention record for the untouched items.
        replaced_task_ids.retain(|previous| {
            session.items.iter().all(|candidate| {
                candidate.task_id.as_deref() != Some(previous.as_str())
                    || unique.contains(candidate.item_id.as_str())
            })
        });
        let bindings = item_ids
            .iter()
            .cloned()
            .map(|item_id| (item_id, task_id.to_string()))
            .collect::<Vec<_>>();
        let target_reservations = self.target_reservations_for_session(&session)?;
        let snapshots = self.bind_item_task_ids_from_snapshot_with_cancel(
            context,
            files,
            session_id,
            &mut session,
            &index,
            &bindings,
            should_cancel,
        )?;
        Ok(BatchOperationPreparation {
            replaced_task_ids: replaced_task_ids.into_iter().collect(),
            snapshots,
            target_reservations,
        })
    }

    /// Create the one persistent control-plane task for a bulk import and
    /// atomically claim the requested item cohort before any worker can be
    /// queued. Item JSON remains the state machine; `task_id` is only this
    /// operation's claim token.
    fn begin_batch_operation_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<BackendTask, BackendError> {
        if item_ids.is_empty() {
            return Err(BackendError::new(
                "IMPORT_BATCH_EMPTY",
                "Choose at least one import item before starting a batch.",
                false,
                true,
            ));
        }
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let index = session
            .items
            .iter()
            .enumerate()
            .map(|(position, item)| (item.item_id.clone(), position))
            .collect::<HashMap<_, _>>();
        let mut unique = HashSet::with_capacity(item_ids.len());
        for item_id in item_ids {
            if !unique.insert(item_id.as_str()) {
                return Err(task_error("Import item ids must be unique."));
            }
            let Some(position) = index.get(item_id) else {
                return Err(item_not_found());
            };
            if !is_batch_claimable(&session.items[*position]) {
                return Err(task_error(
                    "Import item is already claimed by another operation.",
                ));
            }
        }

        let source_label = (item_ids.len() == 1).then(|| {
            session.items[*index.get(&item_ids[0]).expect("validated item")]
                .input
                .display_name
                .clone()
        });
        let title = source_label
            .as_ref()
            .map(|label| format!("Import {label}"))
            .unwrap_or_else(|| format!("Import {} sources", item_ids.len()));
        let task = tasks
            .create_project_import_operation_task(
                context.project_id.clone(),
                context.root.clone(),
                import_operation_task_state_root(context)?,
                title,
                session_id.to_string(),
                item_ids.len() as u64,
                source_label,
            )
            .map_err(|error| task_error(&error))?;
        let bindings = item_ids
            .iter()
            .cloned()
            .map(|item_id| (item_id, task.id.clone()))
            .collect::<Vec<_>>();
        if let Err(error) = self.bind_item_task_ids_from_snapshot(
            context,
            files,
            session_id,
            &mut session,
            &index,
            &bindings,
        ) {
            let _ = tasks.set_error(&task.id, error.clone());
            let _ = tasks.transition_status(&task.id, TaskStatus::Failed);
            return Err(error);
        }
        Ok(task)
    }

    pub(super) fn begin_agent_assistance_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        trigger: AgentAssistanceTrigger,
        agent_kind: crate::models::agent::AgentKind,
        max_attempts: u8,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            let agent_attempts = item
                .attempts
                .iter()
                .filter(|attempt| attempt.route.starts_with("agent_assistance/"))
                .collect::<Vec<_>>();
            if agent_attempts.len() >= usize::from(max_attempts) {
                return Err(task_error(
                    "The Agent assistance attempt budget is exhausted for this item.",
                ));
            }
            if agent_attempts
                .iter()
                .any(|attempt| attempt.completed_at.is_none())
            {
                return Err(task_error(
                    "Another Agent assistance task is already active for this item.",
                ));
            }
            match trigger {
                AgentAssistanceTrigger::QualityOptimization
                    if item.status != ImportItemStatus::PreviewReady =>
                {
                    return Err(task_error(
                        "Quality optimization requires a deterministic preview.",
                    ));
                }
                AgentAssistanceTrigger::Manual
                    if !matches!(
                        item.status,
                        ImportItemStatus::Failed | ImportItemStatus::PreviewReady
                    ) =>
                {
                    return Err(task_error(
                        "Manual Agent assistance requires a failed item or preview.",
                    ));
                }
                _ => {}
            }
            item.task_id = Some(task_id.to_string());
            item.attempts.push(AttemptRecord {
                route: format!("agent_assistance/{task_id}"),
                engine_id: format!("{:?}", agent_kind).to_ascii_lowercase(),
                engine_version: "detected-local-cli".into(),
                stage: ImportStage::Extract,
                started_at: chrono::Utc::now().to_rfc3339(),
                completed_at: None,
                outcome: AttemptOutcome::Failed,
                error_code: None,
                warnings: Vec::new(),
            });
            Ok(())
        })
    }

    pub(super) fn finish_agent_assistance_attempt_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        outcome: AttemptOutcome,
        warnings: Vec<String>,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            let local_route = format!("agent_assistance/{task_id}");
            let attempt = item
                .attempts
                .iter_mut()
                .rev()
                .find(|attempt| attempt.route == local_route)
                .ok_or_else(|| task_error("Agent assistance attempt was not found."))?;
            attempt.completed_at = Some(chrono::Utc::now().to_rfc3339());
            attempt.outcome = outcome;
            attempt.warnings = warnings;
            Ok(())
        })
    }

    pub(super) fn register_agent_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        candidate: AgentCandidate,
        needs_three_way_merge: bool,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) || candidate.task_id != task_id {
                return Err(task_error(
                    "Agent candidate is not bound to this import item task.",
                ));
            }
            item.status = if needs_three_way_merge {
                ImportItemStatus::NeedsMerge
            } else {
                ImportItemStatus::PreviewReady
            };
            Ok(())
        })
    }

    pub(super) fn begin_agent_candidate_validation_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItemStatus, BackendError> {
        let mut previous = ImportItemStatus::Failed;
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error(
                    "Agent candidate validation is not bound to this item task.",
                ));
            }
            if !matches!(
                item.status,
                ImportItemStatus::Failed
                    | ImportItemStatus::PreviewReady
                    | ImportItemStatus::NeedsMerge
                    | ImportItemStatus::Validating
            ) {
                return Err(task_error(
                    "This import item cannot enter Agent candidate validation.",
                ));
            }
            previous = item.status.clone();
            item.status = ImportItemStatus::Validating;
            Ok(())
        })?;
        Ok(previous)
    }

    pub(super) fn fail_agent_candidate_validation_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        previous: ImportItemStatus,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error(
                    "Agent candidate validation failure is not bound to this item task.",
                ));
            }
            item.status = match previous {
                ImportItemStatus::PreviewReady | ImportItemStatus::NeedsMerge => previous,
                _ if item.preview.is_some() => ImportItemStatus::PreviewReady,
                _ => ImportItemStatus::Failed,
            };
            Ok(())
        })
    }

    pub(super) fn mark_agent_candidate_rejected(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error(
                    "Rejected Agent candidate is not bound to this item task.",
                ));
            }
            let local_route = format!("agent_assistance/{task_id}");
            let attempt = item
                .attempts
                .iter_mut()
                .rev()
                .find(|attempt| attempt.route == local_route)
                .ok_or_else(|| task_error("Agent assistance attempt was not found."))?;
            if !attempt
                .warnings
                .iter()
                .any(|warning| warning == "AGENT_CANDIDATE_REJECTED")
            {
                attempt.warnings.push("AGENT_CANDIDATE_REJECTED".into());
            }
            Ok(())
        })
    }

    pub(super) fn reject_agent_candidate_validation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        previous: ImportItemStatus,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            if item.task_id.as_deref() != Some(task_id) {
                return Err(task_error(
                    "Rejected Agent candidate is not bound to this item task.",
                ));
            }
            item.status = match previous {
                ImportItemStatus::PreviewReady | ImportItemStatus::NeedsMerge => previous,
                _ if item.preview.is_some() => ImportItemStatus::PreviewReady,
                _ => ImportItemStatus::Failed,
            };
            let local_route = format!("agent_assistance/{task_id}");
            let attempt = item
                .attempts
                .iter_mut()
                .rev()
                .find(|attempt| attempt.route == local_route)
                .ok_or_else(|| task_error("Agent assistance attempt was not found."))?;
            if !attempt
                .warnings
                .iter()
                .any(|warning| warning == "AGENT_CANDIDATE_REJECTED")
            {
                attempt.warnings.push("AGENT_CANDIDATE_REJECTED".into());
            }
            Ok(())
        })
    }

    pub(super) fn select_agent_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        preview: crate::models::import_v2::ImportPreviewArtifact,
        explicit_merge_current_hash: Option<&str>,
    ) -> Result<ImportItem, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item_position = session
            .items
            .iter()
            .position(|item| item.item_id == item_id)
            .ok_or_else(item_not_found)?;
        ensure_agent_candidate_item_is_mutable(&session, &session.items[item_position])?;
        if session.items[item_position].task_id.as_deref() != Some(task_id) {
            return Err(task_error(
                "Agent candidate selection is not bound to this item task.",
            ));
        }

        let mut resolution_item = session.items[item_position].clone();
        resolution_item.preview = Some(preview.clone());
        let mut resolution =
            self.derive_resolution_context(context, files, session_id, &resolution_item)?;
        if let Some(expected_current_hash) = explicit_merge_current_hash {
            let current_hash = resolution
                .binding
                .as_ref()
                .map(|binding| binding.current_hash.as_str());
            if current_hash != Some(expected_current_hash) {
                return Err(BackendError::new(
                    "IMPORT_AGENT_MERGE_STALE",
                    "Current Wiki changed after the Agent merge was reviewed.",
                    false,
                    true,
                ));
            }
        }
        let needs_unresolved_merge = resolution.kind
            == crate::models::import_v2::ImportResolutionKind::NeedsThreeWayMerge
            && explicit_merge_current_hash.is_none();
        resolution_item
            .preview
            .as_mut()
            .expect("preview was installed")
            .resolution = Some(resolution.clone());
        session.items[item_position] = resolution_item;
        if resolution.kind == crate::models::import_v2::ImportResolutionKind::NewSource {
            resolution.target_wiki_path =
                crate::services::import_v2::commit::planned_new_source_wiki_path(
                    context, files, &session, item_id,
                )?;
        } else if resolution.kind
            == crate::models::import_v2::ImportResolutionKind::NeedsThreeWayMerge
            && explicit_merge_current_hash.is_some()
        {
            let binding = resolution
                .binding
                .as_ref()
                .ok_or_else(|| task_error("Selected Agent merge is missing its Source binding."))?;
            resolution.default_resolution = Some(
                crate::models::import_v2::ImportItemResolution::ApplyImportCandidate {
                    source_id: binding.source_id.clone(),
                    candidate_hash: binding.candidate_hash.clone(),
                    current_hash: binding.current_hash.clone(),
                    target_version_id: binding.target_version_id.clone(),
                },
            );
        }

        let mut preview = preview;
        preview.resolution = Some(resolution);
        let item = &mut session.items[item_position];
        item.preview = Some(preview);
        item.status = if needs_unresolved_merge {
            ImportItemStatus::NeedsMerge
        } else {
            ImportItemStatus::PreviewReady
        };
        crate::services::import_v2::commit::refresh_new_source_wiki_targets(
            context,
            files,
            &mut session,
        )?;
        let item = session.items[item_position].clone();
        persist_derived(&self.sessions, context, files, session)?;
        Ok(item)
    }

    pub(super) fn discard_agent_candidate(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        deterministic_preview: Option<crate::models::import_v2::ImportPreviewArtifact>,
    ) -> Result<ImportItem, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item_position = session
            .items
            .iter()
            .position(|item| item.item_id == item_id)
            .ok_or_else(item_not_found)?;
        ensure_agent_candidate_item_is_mutable(&session, &session.items[item_position])?;
        let item = &mut session.items[item_position];
        if item.task_id.as_deref() != Some(task_id) {
            return Err(task_error(
                "Agent candidate discard is not bound to this item task.",
            ));
        }
        item.preview = deterministic_preview;
        item.status = if item.preview.is_some() {
            ImportItemStatus::PreviewReady
        } else {
            ImportItemStatus::Failed
        };
        let local_route = format!("agent_assistance/{task_id}");
        if let Some(attempt) = item
            .attempts
            .iter_mut()
            .rev()
            .find(|attempt| attempt.route == local_route)
        {
            if !attempt
                .warnings
                .iter()
                .any(|warning| warning == "AGENT_CANDIDATE_DISCARDED")
            {
                attempt.warnings.push("AGENT_CANDIDATE_DISCARDED".into());
            }
        }
        crate::services::import_v2::commit::refresh_new_source_wiki_targets(
            context,
            files,
            &mut session,
        )?;
        let item = session.items[item_position].clone();
        persist_derived(&self.sessions, context, files, session)?;
        Ok(item)
    }
    pub(crate) fn authorize_media_for_session_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        kind: ImportMediaAuthorizationKind,
        asr_profile: Option<ImportAsrProfile>,
        language: Option<String>,
    ) -> Result<(), BackendError> {
        self.authorize_media_for_session_unchecked(
            permit.context(),
            files,
            session_id,
            item_id,
            kind,
            asr_profile,
            language,
            None,
        )
    }

    pub(crate) fn authorize_media_for_session_if_unchanged_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        kind: ImportMediaAuthorizationKind,
        asr_profile: Option<ImportAsrProfile>,
        language: Option<String>,
        expected_item: &ImportItem,
    ) -> Result<(), BackendError> {
        self.authorize_media_for_session_unchecked(
            permit.context(),
            files,
            session_id,
            item_id,
            kind,
            asr_profile,
            language,
            Some(expected_item),
        )
    }

    pub(crate) fn enable_remote_media_retention_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.enable_remote_media_retention_unchecked(permit.context(), files, session_id, item_id)
    }

    pub(crate) fn mark_authenticated_login_group_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_ids: &[String],
        account_summary: Option<&str>,
    ) -> Result<ImportSession, BackendError> {
        self.mark_authenticated_login_group_unchecked(
            permit.context(),
            files,
            session_id,
            item_ids,
            account_summary,
        )
    }

    pub(crate) fn clear_authenticated_login_group_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<ImportSession, BackendError> {
        self.clear_authenticated_login_group_unchecked(
            permit.context(),
            files,
            session_id,
            item_ids,
        )
    }

    pub(crate) fn cancel_queued_item_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.cancel_queued_item_unchecked(permit.context(), files, session_id, item_id)
    }

    pub(crate) fn cancel_batch_item_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.cancel_batch_item_unchecked(permit.context(), files, session_id, item_id)
    }

    pub(crate) fn cancel_batch_item_for_task_authorized(
        &self,
        permit: &ProjectTaskMutationPermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        if permit.workflow_access().persistence
            != crate::models::workflow::WorkflowPersistenceMode::Persistent
        {
            return Err(BackendError::new(
                "PROJECT_TASK_PERSISTENCE_REVOKED",
                "Project task persistence is no longer available.",
                true,
                false,
            ));
        }
        self.cancel_batch_item_unchecked(permit.context(), files, session_id, item_id)
    }

    pub(crate) fn skip_item_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.skip_item_unchecked(permit.context(), files, tasks, session_id, item_id)
    }

    pub(crate) fn create_session_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        mode: ImportResourceMode,
    ) -> Result<ImportSession, BackendError> {
        self.create_session_unchecked(permit.context(), files, mode)
    }

    pub(crate) fn create_batch_operation_task_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<BackendTask, BackendError> {
        self.create_batch_operation_task_unchecked(
            permit.context(),
            files,
            tasks,
            session_id,
            item_ids,
        )
    }

    pub(crate) fn prepare_batch_operation_authorized<F>(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        task_id: &str,
        item_ids: &[String],
        should_cancel: F,
    ) -> Result<BatchOperationPreparation, BackendError>
    where
        F: FnMut() -> bool,
    {
        self.prepare_batch_operation_unchecked(
            permit.context(),
            files,
            tasks,
            session_id,
            task_id,
            item_ids,
            should_cancel,
        )
    }

    pub(crate) fn add_inputs_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        inputs: Vec<ImportInput>,
    ) -> Result<ImportSession, BackendError> {
        self.add_inputs_unchecked(permit.context(), files, session_id, inputs)
    }

    pub(crate) fn add_collection_inputs_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        inputs: Vec<crate::services::import_v2::session_store::CollectionImportInput>,
        source_url: String,
        platform: String,
        title: String,
    ) -> Result<ImportSession, BackendError> {
        self.add_collection_inputs_unchecked(
            permit.context(),
            files,
            session_id,
            inputs,
            source_url,
            platform,
            title,
        )
    }

    pub(crate) fn add_text_input_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        display_name: &str,
        content: &str,
    ) -> Result<ImportSession, BackendError> {
        self.add_text_input_unchecked(permit.context(), files, session_id, display_name, content)
    }

    pub(crate) fn set_discovery_task_id_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        task_id: Option<String>,
    ) -> Result<ImportSession, BackendError> {
        self.set_discovery_task_id_unchecked(permit.context(), files, session_id, task_id)
    }

    pub(crate) fn bind_item_task_ids_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        bindings: &[(String, String)],
    ) -> Result<Vec<ImportWorkItemSnapshot>, BackendError> {
        self.bind_item_task_ids_unchecked(permit.context(), files, session_id, bindings)
    }

    pub(crate) fn bind_item_task_ids_if_unchanged_authorized<F>(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        bindings: &[(String, String)],
        expected_items: &[ImportItem],
        mut should_cancel: F,
    ) -> Result<Vec<ImportWorkItemSnapshot>, BackendError>
    where
        F: FnMut() -> bool,
    {
        let _guard = self.lock()?;
        self.preflight_locked(permit.context())?;
        let mut session = self.sessions.load(permit.context(), files, session_id)?;
        if should_cancel() {
            return Err(cancelled_error());
        }
        for expected in expected_items {
            if !session
                .items
                .iter()
                .any(|current| current.item_id == expected.item_id && current == expected)
            {
                return Err(BackendError::new(
                    "IMPORT_V2_CAPABILITY_REQUIREMENT_STALE",
                    "The import item changed before continuation could be committed.",
                    true,
                    false,
                ));
            }
        }
        let index = session
            .items
            .iter()
            .enumerate()
            .map(|(position, item)| (item.item_id.clone(), position))
            .collect::<HashMap<_, _>>();
        self.bind_item_task_ids_from_snapshot_with_cancel(
            permit.context(),
            files,
            session_id,
            &mut session,
            &index,
            bindings,
            should_cancel,
        )
    }

    pub(crate) fn recover_session_report_with_cancel_authorized<F, P>(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        should_cancel: F,
        on_progress: P,
    ) -> Result<ImportSessionRecovery, BackendError>
    where
        F: FnMut() -> bool,
        P: FnMut(u64, u64),
    {
        self.recover_session_report_with_cancel_unchecked(
            permit.context(),
            files,
            tasks,
            session_id,
            should_cancel,
            on_progress,
        )
    }

    pub(crate) fn set_item_selected_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        selected: bool,
    ) -> Result<ImportItem, BackendError> {
        self.set_item_selected_unchecked(permit.context(), files, session_id, item_id, selected)
    }

    pub(crate) fn select_subtitle_for_session_authorized(
        &self,
        permit: &ProjectWritePermit<'_>,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        file_name: &str,
    ) -> Result<ImportItem, BackendError> {
        self.select_subtitle_for_session_unchecked(
            permit.context(),
            files,
            session_id,
            item_id,
            file_name,
        )
    }

    #[cfg(debug_assertions)]
    pub fn authorize_media_for_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        kind: ImportMediaAuthorizationKind,
        asr_profile: Option<ImportAsrProfile>,
        language: Option<String>,
    ) -> Result<(), BackendError> {
        self.authorize_media_for_session_unchecked(
            context,
            files,
            session_id,
            item_id,
            kind,
            asr_profile,
            language,
            None,
        )
    }

    #[cfg(debug_assertions)]
    pub fn enable_remote_media_retention(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.enable_remote_media_retention_unchecked(context, files, session_id, item_id)
    }

    #[cfg(debug_assertions)]
    pub fn mark_authenticated_login_group(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_ids: &[String],
        account_summary: Option<&str>,
    ) -> Result<ImportSession, BackendError> {
        self.mark_authenticated_login_group_unchecked(
            context,
            files,
            session_id,
            item_ids,
            account_summary,
        )
    }

    #[cfg(debug_assertions)]
    pub fn clear_authenticated_login_group(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<ImportSession, BackendError> {
        self.clear_authenticated_login_group_unchecked(context, files, session_id, item_ids)
    }

    #[cfg(debug_assertions)]
    pub fn cancel_queued_item(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.cancel_queued_item_unchecked(context, files, session_id, item_id)
    }

    #[cfg(debug_assertions)]
    pub fn cancel_batch_item(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.cancel_batch_item_unchecked(context, files, session_id, item_id)
    }

    #[cfg(debug_assertions)]
    pub fn skip_item(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.skip_item_unchecked(context, files, tasks, session_id, item_id)
    }

    #[cfg(debug_assertions)]
    pub fn create_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        mode: ImportResourceMode,
    ) -> Result<ImportSession, BackendError> {
        self.create_session_unchecked(context, files, mode)
    }

    #[cfg(debug_assertions)]
    pub fn create_batch_operation_task(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<BackendTask, BackendError> {
        self.create_batch_operation_task_unchecked(context, files, tasks, session_id, item_ids)
    }

    #[cfg(debug_assertions)]
    pub fn prepare_batch_operation<F>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        task_id: &str,
        item_ids: &[String],
        should_cancel: F,
    ) -> Result<BatchOperationPreparation, BackendError>
    where
        F: FnMut() -> bool,
    {
        self.prepare_batch_operation_unchecked(
            context,
            files,
            tasks,
            session_id,
            task_id,
            item_ids,
            should_cancel,
        )
    }

    #[cfg(debug_assertions)]
    pub fn begin_batch_operation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_ids: &[String],
    ) -> Result<BackendTask, BackendError> {
        self.begin_batch_operation_unchecked(context, files, tasks, session_id, item_ids)
    }

    #[cfg(debug_assertions)]
    pub fn add_inputs(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        inputs: Vec<ImportInput>,
    ) -> Result<ImportSession, BackendError> {
        self.add_inputs_unchecked(context, files, session_id, inputs)
    }

    #[cfg(debug_assertions)]
    pub fn add_collection_inputs(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        inputs: Vec<crate::services::import_v2::session_store::CollectionImportInput>,
        source_url: String,
        platform: String,
        title: String,
    ) -> Result<ImportSession, BackendError> {
        self.add_collection_inputs_unchecked(
            context, files, session_id, inputs, source_url, platform, title,
        )
    }

    #[cfg(debug_assertions)]
    pub fn add_text_input(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        display_name: &str,
        content: &str,
    ) -> Result<ImportSession, BackendError> {
        self.add_text_input_unchecked(context, files, session_id, display_name, content)
    }

    #[cfg(debug_assertions)]
    pub fn set_discovery_task_id(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        task_id: Option<String>,
    ) -> Result<ImportSession, BackendError> {
        self.set_discovery_task_id_unchecked(context, files, session_id, task_id)
    }

    #[cfg(debug_assertions)]
    pub fn bind_item_task_ids(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        bindings: &[(String, String)],
    ) -> Result<Vec<ImportWorkItemSnapshot>, BackendError> {
        self.bind_item_task_ids_unchecked(context, files, session_id, bindings)
    }

    #[cfg(debug_assertions)]
    pub fn recover_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
    ) -> Result<ImportSession, BackendError> {
        self.recover_session_unchecked(context, files, tasks, session_id)
    }

    #[cfg(debug_assertions)]
    pub fn recover_session_with_cancel<F>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        should_cancel: F,
    ) -> Result<ImportSession, BackendError>
    where
        F: FnMut() -> bool,
    {
        self.recover_session_with_cancel_unchecked(context, files, tasks, session_id, should_cancel)
    }

    #[cfg(debug_assertions)]
    pub fn set_item_selected(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        selected: bool,
    ) -> Result<ImportItem, BackendError> {
        self.set_item_selected_unchecked(context, files, session_id, item_id, selected)
    }

    #[cfg(debug_assertions)]
    pub fn select_subtitle_for_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        file_name: &str,
    ) -> Result<ImportItem, BackendError> {
        self.select_subtitle_for_session_unchecked(context, files, session_id, item_id, file_name)
    }

    #[cfg(debug_assertions)]
    pub fn begin_agent_assistance(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        trigger: AgentAssistanceTrigger,
        agent_kind: crate::models::agent::AgentKind,
        max_attempts: u8,
    ) -> Result<ImportItem, BackendError> {
        self.begin_agent_assistance_unchecked(
            context,
            files,
            session_id,
            item_id,
            task_id,
            trigger,
            agent_kind,
            max_attempts,
        )
    }

    #[cfg(debug_assertions)]
    pub fn finish_agent_assistance_attempt(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        outcome: AttemptOutcome,
        warnings: Vec<String>,
    ) -> Result<ImportItem, BackendError> {
        self.finish_agent_assistance_attempt_unchecked(
            context, files, session_id, item_id, task_id, outcome, warnings,
        )
    }

    #[cfg(debug_assertions)]
    pub fn begin_agent_candidate_validation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItemStatus, BackendError> {
        self.begin_agent_candidate_validation_unchecked(
            context, files, session_id, item_id, task_id,
        )
    }

    #[cfg(debug_assertions)]
    pub fn fail_agent_candidate_validation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        previous: ImportItemStatus,
    ) -> Result<ImportItem, BackendError> {
        self.fail_agent_candidate_validation_unchecked(
            context, files, session_id, item_id, task_id, previous,
        )
    }

    fn recover_session_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
    ) -> Result<ImportSession, BackendError> {
        self.recover_session_with_cancel_unchecked(context, files, tasks, session_id, || false)
    }

    fn recover_session_with_cancel_unchecked<F>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        should_cancel: F,
    ) -> Result<ImportSession, BackendError>
    where
        F: FnMut() -> bool,
    {
        self.recover_session_report_with_cancel_unchecked(
            context,
            files,
            tasks,
            session_id,
            should_cancel,
            |_, _| {},
        )
        .map(|report| report.session)
    }

    fn recover_session_report_with_cancel_unchecked<F, P>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        mut should_cancel: F,
        mut on_progress: P,
    ) -> Result<ImportSessionRecovery, BackendError>
    where
        F: FnMut() -> bool,
        P: FnMut(u64, u64),
    {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let needs_index_rebuild = matches!(
            self.sessions
                .read_overview(context, files, session_id)?
                .index_state,
            crate::models::import_v2::ImportSessionIndexState::RebuildRequired
        );
        let mut session = self.sessions.load(context, files, session_id)?;
        let before_session = session.clone();
        let mut staging_to_reconcile = Vec::new();
        let total_items = session.items.len() as u64;
        let progress_stride = (total_items.saturating_add(99) / 100).max(1);
        for (position, item) in session.items.iter_mut().enumerate() {
            if should_cancel() {
                return Err(BackendError::new(
                    crate::errors::IMPORT_V2_CANCELLED,
                    "Import recovery was cancelled.",
                    true,
                    false,
                ));
            }
            for attempt in &mut item.attempts {
                let Some(task_id) = attempt
                    .route
                    .strip_prefix("agent_assistance/")
                    .filter(|_| attempt.completed_at.is_none())
                    .map(str::to_owned)
                else {
                    continue;
                };
                let task = tasks.get_task(&task_id);
                let sealed_agent_output = task.as_ref().is_some_and(|task| {
                    task.status == TaskStatus::Failed
                        && task.task_type == TaskType::AgentRun
                        && matches!(
                            task.result.as_ref().and_then(|result| result.reference.as_ref()),
                            Some(TaskResultReference::ImportPreview {
                                session_id: bound_session,
                                item_id: bound_item,
                            }) if bound_session == session_id && bound_item == &item.item_id
                        )
                });
                let task_status = task.map(|task| task.status);
                if matches!(
                    task_status,
                    Some(
                        TaskStatus::Queued
                            | TaskStatus::Running
                            | TaskStatus::WaitingForConfirmation
                            | TaskStatus::Cancelling
                    )
                ) {
                    continue;
                }
                attempt.completed_at = Some(chrono::Utc::now().to_rfc3339());
                match task_status {
                    Some(TaskStatus::Succeeded) => {
                        attempt.outcome = AttemptOutcome::Succeeded;
                        attempt.warnings.clear();
                    }
                    Some(TaskStatus::Cancelled) => {
                        attempt.outcome = AttemptOutcome::Cancelled;
                        attempt.warnings = vec![
                            "Agent assistance was cancelled before recovery completed.".into(),
                        ];
                    }
                    _ if sealed_agent_output => {
                        attempt.outcome = AttemptOutcome::Succeeded;
                        attempt.warnings.clear();
                    }
                    _ => {
                        attempt.outcome = AttemptOutcome::Failed;
                        attempt.warnings =
                            vec!["Interrupted Agent assistance was closed during recovery.".into()];
                    }
                }
            }
            if item.status == ImportItemStatus::Queued {
                let recovered_status = item
                    .task_id
                    .as_deref()
                    .and_then(|id| tasks.get_task(id))
                    .map(|task| task.status);
                if matches!(
                    recovered_status,
                    Some(TaskStatus::Failed | TaskStatus::Cancelled) | None
                ) {
                    // A pre-bound queued task can be persisted before its
                    // worker claims the item. If that task was interrupted,
                    // release the stale identity so the normal retry path
                    // can bind a fresh task after restart.
                    item.task_id = None;
                    item.progress = None;
                }
            }
            if matches!(
                item.status,
                ImportItemStatus::WaitingCapability
                    | ImportItemStatus::WaitingLogin
                    | ImportItemStatus::WaitingAuthorization
            ) {
                let recovered_status = item
                    .task_id
                    .as_deref()
                    .and_then(|id| tasks.get_task(id))
                    .map(|task| task.status);
                match recovered_status {
                    Some(TaskStatus::Cancelled)
                        if !item
                            .task_id
                            .as_deref()
                            .is_some_and(|id| is_batch_operation_task(tasks, id)) => {
                        transition_item(item, ImportItemStatus::Cancelled)?;
                        item.issue = None;
                        item.task_id = None;
                        item.progress = None;
                    }
                    // An operation cancellation applies only to still queued
                    // or in-flight claims. Waiting item facts remain available
                    // for their explicit prerequisite/retry actions.
                    Some(TaskStatus::Cancelled) => {}
                    Some(TaskStatus::Failed) | None => {
                        item.task_id = None;
                        item.progress = None;
                        if item.issue.is_none() {
                            item.issue = Some(ImportIssue {
                                code: "TASK_RECOVERY".into(),
                                message: "This import is still waiting for the requested action."
                                    .into(),
                                stage: ImportStage::Route,
                                retryable: true,
                                user_action_required: true,
                                recovery_actions: vec![
                                    crate::models::import_v2::ImportRecoveryAction::Retry,
                                    crate::models::import_v2::ImportRecoveryAction::ViewLog,
                                ],
                                available_actions: Vec::new(),
                                subtitle_candidates: Vec::new(),
                            });
                        }
                    }
                    Some(
                        TaskStatus::Queued
                        | TaskStatus::Running
                        | TaskStatus::WaitingForConfirmation
                        | TaskStatus::Cancelling
                        | TaskStatus::Succeeded
                        // Batch 0 freezes the wire state only. Import V2 does not emit
                        // Interrupted; workflow recovery semantics land in Batch 1.
                        | TaskStatus::Interrupted,
                    ) => {}
                }
            }
            if matches!(
                item.status,
                ImportItemStatus::Inspecting
                    | ImportItemStatus::Extracting
                    | ImportItemStatus::Validating
            ) {
                let recovered_status = item
                    .task_id
                    .as_deref()
                    .and_then(|id| tasks.get_task(id))
                    .map(|task| task.status);
                if recovered_status == Some(TaskStatus::Cancelled) {
                    let staging = context.resolve_project_path(&item_staging_relative_path(
                        context,
                        session_id,
                        &item.item_id,
                    )?)?;
                    staging_to_reconcile.push(staging);
                    transition_item(item, ImportItemStatus::Cancelled)?;
                    item.task_id = None;
                    item.progress = None;
                    continue;
                }
                let interrupted = recovered_status.is_none_or(|status| {
                    matches!(
                        status,
                        TaskStatus::Failed | TaskStatus::Succeeded | TaskStatus::Interrupted
                    )
                });
                if interrupted {
                    let staging = context.resolve_project_path(&item_staging_relative_path(
                        context,
                        session_id,
                        &item.item_id,
                    )?)?;
                    staging_to_reconcile.push(staging);
                    transition_item(item, ImportItemStatus::Paused)?;
                    item.task_id = None;
                    item.progress = None;
                    item.issue = Some(ImportIssue {
                        code: "TASK_PAUSED".into(),
                        message: "Import was paused after the app stopped and can continue.".into(),
                        stage: ImportStage::Extract,
                        retryable: true,
                        user_action_required: false,
                        recovery_actions: vec![
                            crate::models::import_v2::ImportRecoveryAction::Retry,
                            crate::models::import_v2::ImportRecoveryAction::ViewLog,
                        ],
                        available_actions: Vec::new(),
                        subtitle_candidates: Vec::new(),
                    });
                }
            }
            let completed = position as u64 + 1;
            if completed == total_items || completed % progress_stride == 0 {
                on_progress(completed, total_items);
            }
        }
        session.status = derive_session_status(&session.items);
        let mut originals = Vec::new();
        let mut replacements = Vec::new();
        for (before, after) in before_session.items.iter().zip(&session.items) {
            if before != after {
                originals.push(before.clone());
                replacements.push(after.clone());
            }
        }
        let session_record_changed =
            before_session.status != session.status || !originals.is_empty();
        if !session_record_changed {
            if needs_index_rebuild {
                if should_cancel() {
                    return Err(BackendError::new(
                        crate::errors::IMPORT_V2_CANCELLED,
                        "Import recovery was cancelled.",
                        true,
                        false,
                    ));
                }
                self.sessions.rebuild_sidecars(context, files, &session)?;
            }
            return Ok(ImportSessionRecovery {
                session,
                changed_items: Vec::new(),
            });
        }
        if should_cancel() {
            return Err(BackendError::new(
                crate::errors::IMPORT_V2_CANCELLED,
                "Import recovery was cancelled.",
                true,
                false,
            ));
        }
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.sessions
            .write_recovery_cohort_if_unchanged_with_cancel(
                context,
                files,
                &before_session,
                &session,
                &originals,
                &replacements,
                should_cancel,
            )?;
        for staging in staging_to_reconcile {
            crate::services::import_v2::media_router::recover_item_staging_temporary_workspaces(
                &staging,
            )?;
        }
        Ok(ImportSessionRecovery {
            session,
            changed_items: replacements,
        })
    }
    pub fn register_engine(&self, engine: Arc<dyn ImportEngine>) -> Result<(), BackendError> {
        self.engines.register(engine)
    }

    pub fn registered_engine_routes(&self) -> Result<Vec<String>, BackendError> {
        self.engines.registered_routes()
    }

    pub fn register_capability_pack(
        &self,
        pack: ResolvedCapabilityPack,
        route: String,
        supported_extensions: Vec<String>,
        timeout: std::time::Duration,
    ) -> Result<(), BackendError> {
        self.engines
            .ensure_registered(Arc::new(PackProcessEngine::new(
                pack,
                route,
                supported_extensions,
                timeout,
                self.web_targets.clone(),
                self.connector_profiles_root.clone(),
            )))
    }

    pub(crate) fn replace_capability_pack(
        &self,
        pack: ResolvedCapabilityPack,
        route: String,
        supported_extensions: Vec<String>,
        timeout: std::time::Duration,
    ) -> Result<(), BackendError> {
        self.engines
            .replace_registered(Arc::new(PackProcessEngine::new(
                pack,
                route,
                supported_extensions,
                timeout,
                self.web_targets.clone(),
                self.connector_profiles_root.clone(),
            )))
    }
    fn set_item_selected_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        selected: bool,
    ) -> Result<ImportItem, BackendError> {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        find_item_mut(&mut session, item_id)?.selected = selected;
        crate::services::import_v2::commit::refresh_new_source_wiki_targets(
            context,
            files,
            &mut session,
        )?;
        let item = session
            .items
            .iter()
            .find(|item| item.item_id == item_id)
            .cloned()
            .ok_or_else(item_not_found)?;
        persist_derived(&self.sessions, context, files, session)?;
        Ok(item)
    }

    fn select_subtitle_for_session_unchecked(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        file_name: &str,
    ) -> Result<ImportItem, BackendError> {
        let file_name = file_name.trim();
        if file_name.is_empty()
            || file_name.len() > 255
            || matches!(file_name, "." | "..")
            || file_name.contains(['/', '\\'])
        {
            return Err(BackendError::new(
                IMPORT_V2_STATE_INVALID,
                "Subtitle selection is invalid.",
                false,
                true,
            ));
        }
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item = find_item_mut(&mut session, item_id)?;
        let issue = item.issue.as_ref().ok_or_else(|| {
            BackendError::new(
                IMPORT_V2_STATE_INVALID,
                "Subtitle selection is not currently required.",
                false,
                true,
            )
        })?;
        if item.status != ImportItemStatus::WaitingAuthorization
            || issue.code != "IMPORT_FILE_SUBTITLE_AMBIGUOUS"
            || !issue
                .subtitle_candidates
                .iter()
                .any(|candidate| candidate == file_name)
        {
            return Err(BackendError::new(
                IMPORT_V2_STATE_INVALID,
                "Subtitle selection does not match the current import item.",
                false,
                true,
            ));
        }
        item.selected_subtitle = Some(file_name.to_string());
        let item = item.clone();
        persist_derived(&self.sessions, context, files, session)?;
        Ok(item)
    }

    /// Compatibility surface for integration and service tests. Production
    /// workers must enter through an execution-lease-bearing method.
    #[cfg(debug_assertions)]
    pub fn run_item(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.run_item_with_recovery_mode(
            context, files, tasks, session_id, item_id, task_id, None, true,
        )
    }

    #[cfg(debug_assertions)]
    pub fn run_item_with_recovery(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        recovery_action: Option<&ImportRecoveryAction>,
    ) -> Result<ImportItem, BackendError> {
        self.run_item_with_recovery_mode(
            context,
            files,
            tasks,
            session_id,
            item_id,
            task_id,
            recovery_action,
            true,
        )
    }

    pub(crate) fn run_item_with_recovery_authorized(
        &self,
        execution: &ProjectExecutionLease,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        task_id: &str,
        snapshot: ImportWorkItemSnapshot,
        target_reservations: &Arc<Mutex<crate::services::import_v2::NewSourceTargetReservations>>,
        recovery_action: Option<&ImportRecoveryAction>,
    ) -> Result<ImportItem, BackendError> {
        self.run_work_item_snapshot_mode(
            execution.task_context(task_id)?,
            files,
            tasks,
            session_id,
            task_id,
            snapshot,
            target_reservations,
            recovery_action,
            true,
        )
    }

    /// Batch operations share one persistent task.  Item state remains the
    /// source of truth, so this path never turns the operation task into an
    /// item-sized WaitingForConfirmation/Failed/Succeeded state machine.
    #[cfg(debug_assertions)]
    pub fn run_item_with_recovery_in_batch(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        operation_id: &str,
        recovery_action: Option<&ImportRecoveryAction>,
    ) -> Result<ImportItem, BackendError> {
        self.run_item_with_recovery_mode(
            context,
            files,
            tasks,
            session_id,
            item_id,
            operation_id,
            recovery_action,
            false,
        )
    }

    pub(crate) fn run_item_with_recovery_in_batch_authorized(
        &self,
        execution: &ProjectExecutionLease,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        operation_id: &str,
        snapshot: ImportWorkItemSnapshot,
        target_reservations: &Arc<Mutex<crate::services::import_v2::NewSourceTargetReservations>>,
        recovery_action: Option<&ImportRecoveryAction>,
    ) -> Result<ImportItem, BackendError> {
        self.run_work_item_snapshot_mode(
            execution.task_context(operation_id)?,
            files,
            tasks,
            session_id,
            operation_id,
            snapshot,
            target_reservations,
            recovery_action,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_item_with_recovery_mode(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        recovery_action: Option<&ImportRecoveryAction>,
        task_lifecycle: bool,
    ) -> Result<ImportItem, BackendError> {
        self.validate_worker_task(context, tasks, task_id, task_lifecycle)?;
        let session = self.load_session(context, files, session_id)?;
        let snapshot = self
            .bind_item_task_ids_unchecked(
                context,
                files,
                session_id,
                &[(item_id.to_string(), task_id.to_string())],
            )?
            .pop()
            .ok_or_else(item_not_found)?;
        let target_reservations = self.target_reservations_for_session(&session)?;
        self.run_work_item_snapshot_mode(
            context,
            files,
            tasks,
            session_id,
            task_id,
            snapshot,
            &target_reservations,
            recovery_action,
            task_lifecycle,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn run_work_item_snapshot_mode(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        task_id: &str,
        snapshot: ImportWorkItemSnapshot,
        target_reservations: &Arc<Mutex<crate::services::import_v2::NewSourceTargetReservations>>,
        recovery_action: Option<&ImportRecoveryAction>,
        task_lifecycle: bool,
    ) -> Result<ImportItem, BackendError> {
        let item_id = snapshot.item_id.clone();
        let worker_revision = Cell::new(snapshot.expected_item_revision);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.run_work_item_snapshot_inner(
                context,
                files,
                tasks,
                session_id,
                task_id,
                snapshot,
                &worker_revision,
                target_reservations,
                recovery_action,
                task_lifecycle,
            )
        }))
        .unwrap_or_else(|_| Err(engine_panicked_error()));
        if let Err(error) = &result {
            if error.code == IMPORT_V2_ENGINE_PANICKED {
                self.terminalize_in_flight_worker_error(
                    context,
                    files,
                    tasks,
                    session_id,
                    &item_id,
                    task_id,
                    worker_revision.get(),
                    error,
                );
            }
        }
        self.web_targets
            .release_private_operation(&item_id, task_id)?;
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn run_work_item_snapshot_inner(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        task_id: &str,
        snapshot: ImportWorkItemSnapshot,
        worker_revision: &Cell<u64>,
        target_reservations: &Arc<Mutex<crate::services::import_v2::NewSourceTargetReservations>>,
        recovery_action: Option<&ImportRecoveryAction>,
        task_lifecycle: bool,
    ) -> Result<ImportItem, BackendError> {
        self.validate_worker_task(context, tasks, task_id, task_lifecycle)?;
        let pre_cancelled = tasks.is_cancelled(task_id);
        let mut snapshot =
            self.claim_item_for_run(context, files, session_id, task_id, snapshot, pre_cancelled)?;
        worker_revision.set(snapshot.expected_item_revision);
        let item_id_owned = snapshot.item_id.clone();
        let item_id = item_id_owned.as_str();
        if pre_cancelled {
            return self.finish_cancelled(
                context,
                files,
                tasks,
                session_id,
                item_id,
                task_id,
                snapshot.expected_item_revision,
            );
        }
        // A private-target confirmation is single-use. Consume it once at the
        // operation boundary, then let only this operation's bounded route
        // fallback chain reuse the active grant.
        self.web_targets
            .claim_private_for_operation(item_id, task_id)?;
        if task_lifecycle {
            self.start_claimed_task(
                context,
                files,
                tasks,
                session_id,
                item_id,
                task_id,
                snapshot.expected_item_revision,
            )?;
            task_call(tasks.update_progress(
                task_id,
                0,
                Some(100),
                Some("Inspecting input".into()),
            ))?;
        }
        let input = snapshot.input.clone();
        if matches!(recovery_action, Some(ImportRecoveryAction::EnableOcr))
            && input.kind == crate::models::import_v2::ImportInputKind::Url
            && !self
                .engines
                .registered_routes()?
                .iter()
                .any(|route| route == "ocr.cjk-accurate" || route == "ocr.basic")
        {
            return self.finish_failed(
                context,
                files,
                tasks,
                session_id,
                item_id,
                task_id,
                snapshot.expected_item_revision,
                ocr_unavailable(),
                ImportStage::Extract,
            );
        }
        let planned_routes = self.planned_routes(context, &input, recovery_action)?;
        let mut engines = Vec::with_capacity(planned_routes.len());
        for attempt in &planned_routes {
            let route_input = route_resolution_input(attempt.0, &input);
            match self.engines.resolve_route(attempt.0, &route_input) {
                Ok(engine) => engines.push((attempt, engine)),
                Err(error) if error.code == IMPORT_V2_ENGINE_PANICKED => return Err(error),
                Err(_) => {}
            }
        }
        if engines.is_empty() {
            let x_capability_missing = planned_routes
                .iter()
                .any(|(route, _)| *route == "web.x.post");
            let error = BackendError::new(
                if x_capability_missing {
                    "IMPORT_WEB_PLATFORM_CAPABILITY_MISSING"
                } else {
                    crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE
                },
                if x_capability_missing {
                    "The X/Twitter import capability is not installed."
                } else {
                    "No planned import route is installed."
                },
                true,
                true,
            );
            self.mutate_claimed_item(
                context,
                files,
                session_id,
                item_id,
                task_id,
                snapshot.expected_item_revision,
                |item| {
                    transition_item(item, ImportItemStatus::WaitingCapability)?;
                    let mut issue = issue_from_engine_error_for_input(
                        &error,
                        ImportStage::Route,
                        &item.input.kind,
                    );
                    if x_capability_missing {
                        issue.available_actions = vec![
                            crate::models::import_v2_agent::AgentRecoveryAction::InvokeLocalAgent,
                        ];
                    }
                    item.issue = Some(issue);
                    Ok(())
                },
            )?;
            if task_lifecycle {
                task_call(tasks.append_log(
                    task_id,
                    LogLevel::Warn,
                    "No available import engine supports this input.".into(),
                ))?;
                task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
            }
            return Err(error);
        }
        let extracting_item = self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            snapshot.expected_item_revision,
            |item| transition_item(item, ImportItemStatus::Extracting),
        )?;
        snapshot.expected_item_revision = extracting_item.item_revision;
        worker_revision.set(snapshot.expected_item_revision);
        if task_lifecycle {
            task_call(tasks.update_progress(
                task_id,
                5,
                Some(100),
                Some("Extracting source".into()),
            ))?;
        }
        let staging_root = item_staging_relative_path(context, session_id, item_id)?;
        let local_asr_authorized = snapshot.media_authorization.local_asr_authorized;
        let local_ocr_authorized = snapshot.media_authorization.local_ocr_authorized;
        let selected_subtitle = snapshot.selected_subtitle.clone();
        let authenticated_retry = snapshot.authenticated_retry;
        let media_save_mode = input.media_save_mode.clone();
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            project_id: context.project_id.clone(),
            session_id: session_id.into(),
            item_id: item_id.into(),
            task_id: task_id.into(),
            operation: EngineOperation::Extract,
            input,
            project_root: context.root.to_string_lossy().into_owned(),
            staging_root: staging_root.clone(),
            chained_input: None,
            local_asr_authorized,
            asr_probe_only: false,
            asr_profile: snapshot.media_authorization.asr_profile.clone(),
            recognition_language: snapshot.media_authorization.recognition_language.clone(),
            selected_subtitle,
            local_ocr_authorized,
            media_save_mode,
        };
        let token = tasks
            .get_cancellation_token(task_id)
            .ok_or_else(|| task_error("Task cancellation state is unavailable."))?;
        let mut selected = None;
        let mut last_error = None;
        let mut recovery_error = None;
        let mut terminal_web_error = None;
        let mut request = request;
        let max_task_progress = Cell::new(5_u64);
        for ((_, quality_floor), engine) in engines {
            let descriptor = describe_engine(engine.as_ref())?;
            if matches!(descriptor.route.as_str(), "ocr.cjk-accurate" | "ocr.basic")
                && !request.local_ocr_authorized
            {
                return self.finish_waiting_local_ocr(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    snapshot.expected_item_revision,
                    ocr_unavailable(),
                    ImportStage::Extract,
                );
            }
            if is_capability_route(&descriptor.route)
                && !descriptor.engine_id.starts_with("builtin.")
                && request.input.source_identity.is_some()
            {
                request.input =
                    materialize_capability_input(context, &staging_root, &request.input)?;
            }
            let started_at = chrono::Utc::now().to_rfc3339();
            let report_engine_progress = |progress: EngineProgress| {
                let mapped = engine_progress_on_task_scale(&progress);
                if mapped < max_task_progress.get() {
                    return Ok(());
                }
                max_task_progress.set(mapped);
                if task_lifecycle {
                    task_call(tasks.update_progress(
                        task_id,
                        mapped,
                        Some(100),
                        Some(progress.label),
                    ))
                    .map(|_| ())
                } else {
                    Ok(())
                }
            };
            let mut candidate = match execute_engine_with_progress(
                engine.as_ref(),
                &request,
                &token,
                &report_engine_progress,
            ) {
                Ok(result) if !token.is_cancelled() => result,
                Ok(_) => {
                    return self.finish_cancelled(
                        context,
                        files,
                        tasks,
                        session_id,
                        item_id,
                        task_id,
                        snapshot.expected_item_revision,
                    )
                }
                Err(_) if token.is_cancelled() => {
                    return self.finish_cancelled(
                        context,
                        files,
                        tasks,
                        session_id,
                        item_id,
                        task_id,
                        snapshot.expected_item_revision,
                    )
                }
                Err(error) => {
                    snapshot.expected_item_revision = self
                        .record_attempt_claimed(
                            context,
                            files,
                            session_id,
                            item_id,
                            task_id,
                            snapshot.expected_item_revision,
                            &descriptor,
                            started_at,
                            crate::models::import_v2::AttemptOutcome::Failed,
                            Some(error.code.clone()),
                            Vec::new(),
                        )?
                        .item_revision;
                    worker_revision.set(snapshot.expected_item_revision);
                    if is_web_user_wait(&error) {
                        if authenticated_retry {
                            return self.finish_failed(
                                context,
                                files,
                                tasks,
                                session_id,
                                item_id,
                                task_id,
                                snapshot.expected_item_revision,
                                BackendError::new(
                                    "IMPORT_WEB_ACCOUNT_PERMISSION_DENIED",
                                    "The current account cannot access this content.",
                                    false,
                                    true,
                                ),
                                ImportStage::Extract,
                            );
                        }
                        return self.finish_waiting_login(
                            context,
                            files,
                            tasks,
                            session_id,
                            item_id,
                            task_id,
                            snapshot.expected_item_revision,
                            error,
                            ImportStage::Extract,
                        );
                    }
                    if error.code == "IMPORT_WEB_SUBTITLE_UNAVAILABLE"
                        && !is_bilibili_import_input(&request.input)
                    {
                        return self.finish_waiting_local_asr(
                            context,
                            files,
                            tasks,
                            session_id,
                            item_id,
                            task_id,
                            snapshot.expected_item_revision,
                            error,
                            ImportStage::Extract,
                        );
                    }
                    if error.code == "IMPORT_LOCAL_SUBTITLE_AMBIGUOUS" {
                        return self.finish_waiting_subtitle_selection(
                            context,
                            files,
                            tasks,
                            session_id,
                            item_id,
                            task_id,
                            snapshot.expected_item_revision,
                            error,
                            ImportStage::Extract,
                        );
                    }
                    if error.code == "IMPORT_WEB_OCR_UNAVAILABLE" {
                        return self.finish_waiting_local_ocr(
                            context,
                            files,
                            tasks,
                            session_id,
                            item_id,
                            task_id,
                            snapshot.expected_item_revision,
                            error,
                            ImportStage::Extract,
                        );
                    }
                    if error.code == "IMPORT_WEB_SUBTITLE_UNAVAILABLE" {
                        recovery_error.get_or_insert_with(|| error.clone());
                    }
                    if error.code == "IMPORT_WEB_CONTENT_REMOVED"
                        && is_bilibili_import_input(&request.input)
                    {
                        terminal_web_error.get_or_insert(error);
                        continue;
                    }
                    if is_non_fallback_error(&error) {
                        return self.finish_failed(
                            context,
                            files,
                            tasks,
                            session_id,
                            item_id,
                            task_id,
                            snapshot.expected_item_revision,
                            error,
                            ImportStage::Extract,
                        );
                    }
                    let route_record = FileRoutePlanner::record(
                        descriptor.route.clone(),
                        RouteOutcome::Failed(classify_route_failure(&error)),
                    );
                    last_error = Some(error);
                    if route_record.allows_fallback() {
                        continue;
                    }
                    break;
                }
            };
            if matches!(descriptor.route.as_str(), "ocr.cjk-accurate" | "ocr.basic") {
                if candidate.text_coverage.unwrap_or_default() <= 0.0
                    || candidate
                        .warnings
                        .iter()
                        .any(|warning| warning == "IMPORT_OCR_NO_TEXT")
                {
                    return self.finish_failed(
                        context,
                        files,
                        tasks,
                        session_id,
                        item_id,
                        task_id,
                        snapshot.expected_item_revision,
                        ocr_no_text(),
                        ImportStage::Extract,
                    );
                }
                let authorized_prefix = format!("{staging_root}/");
                if let Some(source_snapshot_path) = request
                    .input
                    .normalized_locator
                    .as_deref()
                    .and_then(|path| path.strip_prefix(&authorized_prefix))
                {
                    candidate.source_snapshot_path = source_snapshot_path.to_string();
                }
            }
            if let Err(error) = validate_engine_result(&staging_root, &candidate) {
                snapshot.expected_item_revision = self
                    .record_attempt_claimed(
                        context,
                        files,
                        session_id,
                        item_id,
                        task_id,
                        snapshot.expected_item_revision,
                        &descriptor,
                        started_at.clone(),
                        crate::models::import_v2::AttemptOutcome::Failed,
                        Some(error.code.clone()),
                        candidate.warnings.clone(),
                    )?
                    .item_revision;
                worker_revision.set(snapshot.expected_item_revision);
                last_error = Some(error);
                continue;
            }
            if candidate.continuation.is_some() {
                let continuation = candidate.continuation.clone();
                candidate = match self.execute_local_continuation(
                    context,
                    files,
                    session_id,
                    item_id,
                    &staging_root,
                    &request,
                    candidate,
                    &token,
                    tasks,
                    task_id,
                    &max_task_progress,
                    &mut snapshot.expected_item_revision,
                    worker_revision,
                ) {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        if token.is_cancelled() || error.code == crate::errors::IMPORT_V2_CANCELLED
                        {
                            return self.finish_cancelled(
                                context,
                                files,
                                tasks,
                                session_id,
                                item_id,
                                task_id,
                                snapshot.expected_item_revision,
                            );
                        }
                        if requires_explicit_video_frame_ocr(&error.code) {
                            return self.finish_waiting_local_ocr(
                                context,
                                files,
                                tasks,
                                session_id,
                                item_id,
                                task_id,
                                snapshot.expected_item_revision,
                                error,
                                ImportStage::Extract,
                            );
                        }
                        if matches!(
                            error.code.as_str(),
                            "IMPORT_WEB_OCR_UNAVAILABLE"
                                | crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE
                        ) && matches!(&continuation, Some(EngineContinuation::LocalOcr { .. }))
                        {
                            return self.finish_waiting_local_ocr(
                                context,
                                files,
                                tasks,
                                session_id,
                                item_id,
                                task_id,
                                snapshot.expected_item_revision,
                                error,
                                ImportStage::Extract,
                            );
                        }
                        if matches!(
                            error.code.as_str(),
                            "IMPORT_WEB_SUBTITLE_UNAVAILABLE"
                                | "IMPORT_ASR_ENGINE_UNAVAILABLE"
                                | crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE
                        ) && matches!(&continuation, Some(EngineContinuation::LocalAsr { .. }))
                        {
                            return self.finish_waiting_local_asr(
                                context,
                                files,
                                tasks,
                                session_id,
                                item_id,
                                task_id,
                                snapshot.expected_item_revision,
                                error,
                                ImportStage::Extract,
                            );
                        }
                        return self.finish_failed(
                            context,
                            files,
                            tasks,
                            session_id,
                            item_id,
                            task_id,
                            snapshot.expected_item_revision,
                            error,
                            ImportStage::Extract,
                        );
                    }
                };
            }
            if let Err(error) = validate_engine_result(&staging_root, &candidate) {
                snapshot.expected_item_revision = self
                    .record_attempt_claimed(
                        context,
                        files,
                        session_id,
                        item_id,
                        task_id,
                        snapshot.expected_item_revision,
                        &descriptor,
                        started_at.clone(),
                        crate::models::import_v2::AttemptOutcome::Failed,
                        Some(error.code.clone()),
                        candidate.warnings.clone(),
                    )?
                    .item_revision;
                worker_revision.set(snapshot.expected_item_revision);
                last_error = Some(error);
                continue;
            }
            // Attempt-level precheck selects a candidate; the formal QualityGate still runs once.
            let required_coverage = quality_floor.requirements().minimum_text_coverage as f64;
            if !candidate_meets_floor(&request.input, &candidate, *quality_floor) {
                snapshot.expected_item_revision = self
                    .record_attempt_claimed(
                        context,
                        files,
                        session_id,
                        item_id,
                        task_id,
                        snapshot.expected_item_revision,
                        &descriptor,
                        started_at.clone(),
                        crate::models::import_v2::AttemptOutcome::Failed,
                        Some(crate::errors::IMPORT_V2_QUALITY_FAILED.into()),
                        candidate.warnings.clone(),
                    )?
                    .item_revision;
                worker_revision.set(snapshot.expected_item_revision);
                last_error = Some(BackendError::new(
                    crate::errors::IMPORT_V2_QUALITY_FAILED,
                    "Candidate failed the route precheck.",
                    true,
                    true,
                ));
                let record = FileRoutePlanner::record(
                    descriptor.route.clone(),
                    RouteOutcome::QualityRejected {
                        actual: candidate.text_coverage.unwrap_or_default() as f32,
                        required: required_coverage as f32,
                    },
                );
                if record.allows_fallback() {
                    continue;
                }
            }
            if descriptor.route == "pack.office-legacy" {
                if let Some(converted) = candidate
                    .asset_paths
                    .iter()
                    .find(|path| path.starts_with("converted/"))
                {
                    snapshot.expected_item_revision = self
                        .record_attempt_claimed(
                            context,
                            files,
                            session_id,
                            item_id,
                            task_id,
                            snapshot.expected_item_revision,
                            &descriptor,
                            started_at,
                            crate::models::import_v2::AttemptOutcome::Succeeded,
                            None,
                            candidate.warnings.clone(),
                        )?
                        .item_revision;
                    worker_revision.set(snapshot.expected_item_revision);
                    request.chained_input = Some(converted.clone());
                    continue;
                }
            }
            selected = Some((descriptor, started_at, candidate));
            break;
        }
        let Some((descriptor, started_at, result)) = selected else {
            if let Some(error) = recovery_error {
                return self.finish_waiting_local_asr(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    snapshot.expected_item_revision,
                    error,
                    ImportStage::Extract,
                );
            }
            if let Some(error) = terminal_web_error {
                return self.finish_failed(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    snapshot.expected_item_revision,
                    error,
                    ImportStage::Extract,
                );
            }
            return self.finish_failed(
                context,
                files,
                tasks,
                session_id,
                item_id,
                task_id,
                snapshot.expected_item_revision,
                last_error.unwrap_or_else(|| {
                    BackendError::new(
                        crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE,
                        "No planned route produced a candidate.",
                        true,
                        true,
                    )
                }),
                ImportStage::Extract,
            );
        };
        let current = self
            .sessions
            .load_item(context, files, session_id, item_id)?;
        if current.item_revision != snapshot.expected_item_revision
            || current.task_id.as_deref() != Some(task_id)
        {
            return Err(work_item_stale_error());
        }
        let validating_item = self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            snapshot.expected_item_revision,
            |item| transition_item(item, ImportItemStatus::Validating),
        )?;
        snapshot.expected_item_revision = validating_item.item_revision;
        worker_revision.set(snapshot.expected_item_revision);
        if task_lifecycle {
            task_call(tasks.update_progress(
                task_id,
                95,
                Some(100),
                Some("Validating preview".into()),
            ))?;
        }
        let mut preview = match self
            .quality
            .evaluate(&context.root.join(Path::new(&staging_root)), &result)
        {
            Ok(preview) => preview,
            Err(error) => {
                snapshot.expected_item_revision = self
                    .record_attempt_claimed(
                        context,
                        files,
                        session_id,
                        item_id,
                        task_id,
                        snapshot.expected_item_revision,
                        &descriptor,
                        started_at,
                        crate::models::import_v2::AttemptOutcome::Failed,
                        Some(error.code.clone()),
                        result.warnings.clone(),
                    )?
                    .item_revision;
                worker_revision.set(snapshot.expected_item_revision);
                return self.finish_failed(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    snapshot.expected_item_revision,
                    error,
                    ImportStage::Validate,
                );
            }
        };
        let mut resolution_item = validating_item;
        resolution_item.preview = Some(preview.clone());
        let mut resolution =
            self.derive_resolution_context(context, files, session_id, &resolution_item)?;
        let needs_merge =
            resolution.kind == crate::models::import_v2::ImportResolutionKind::NeedsThreeWayMerge;
        resolution_item
            .preview
            .as_mut()
            .expect("preview was installed")
            .resolution = Some(resolution.clone());
        if resolution.kind == crate::models::import_v2::ImportResolutionKind::NewSource {
            resolution_item
                .preview
                .as_mut()
                .expect("preview was installed")
                .resolution = Some(resolution.clone());
            resolution.target_wiki_path = target_reservations
                .lock()
                .map_err(|_| task_error("Import target reservation index is unavailable."))?
                .reserve(context, files, session_id, &resolution_item)?;
        }
        preview.resolution = Some(resolution);
        let restricted_content = web_result_marks_restricted_content(
            &context.root.join(Path::new(&staging_root)),
            result.metadata_path.as_deref(),
        );
        let item = self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            snapshot.expected_item_revision,
            |item| {
                transition_item(
                    item,
                    if needs_merge {
                        ImportItemStatus::NeedsMerge
                    } else {
                        ImportItemStatus::PreviewReady
                    },
                )?;
                item.preview = Some(preview);
                item.issue = None;
                if restricted_content {
                    item.restricted_content = true;
                    item.restricted_identity_summary
                        .clone_from(&item.authenticated_identity_summary);
                }
                item.attempts.push(crate::models::import_v2::AttemptRecord {
                    route: descriptor.route.clone(),
                    engine_id: descriptor.engine_id.clone(),
                    engine_version: descriptor.engine_version.clone(),
                    stage: ImportStage::Validate,
                    started_at,
                    completed_at: Some(chrono::Utc::now().to_rfc3339()),
                    outcome: crate::models::import_v2::AttemptOutcome::Succeeded,
                    error_code: None,
                    warnings: result.warnings.clone(),
                });
                Ok(())
            },
        )?;
        if task_lifecycle {
            task_call(tasks.update_progress(
                task_id,
                100,
                Some(100),
                Some("Preview ready".into()),
            ))?;
            task_call(tasks.set_result(
                task_id,
                TaskResult {
                    summary: "Import preview ready.".into(),
                    affected_paths: Vec::new(),
                    reference: Some(TaskResultReference::ImportPreview {
                        session_id: session_id.into(),
                        item_id: item_id.into(),
                    }),
                    pending_action: None,
                },
            ))?;
            task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
        }
        Ok(item)
    }

    fn validate_worker_task(
        &self,
        context: &ProjectContext,
        tasks: &TaskService,
        task_id: &str,
        task_lifecycle: bool,
    ) -> Result<(), BackendError> {
        let task = tasks
            .get_task(task_id)
            .ok_or_else(|| task_error("Import task was not found."))?;
        if task.task_type != TaskType::Import
            || task.project_id.as_deref() != Some(context.project_id.as_str())
            || !(matches!(task.status, TaskStatus::Queued | TaskStatus::Cancelled)
                || (!task_lifecycle
                    && matches!(task.status, TaskStatus::Running | TaskStatus::Cancelling)))
        {
            return Err(task_error("Task is not compatible with this import item."));
        }
        Ok(())
    }

    fn execute_local_continuation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        staging_root: &str,
        request: &EngineRequest,
        web_result: EngineResult,
        token: &crate::tasks::task_model::CancellationToken,
        tasks: &TaskService,
        task_id: &str,
        max_task_progress: &Cell<u64>,
        expected_item_revision: &mut u64,
        worker_revision: &Cell<u64>,
    ) -> Result<EngineResult, BackendError> {
        match web_result.continuation.as_ref() {
            Some(EngineContinuation::LocalAsr { .. }) => {
                let result = self.execute_local_asr_continuation(
                    context,
                    files,
                    session_id,
                    item_id,
                    staging_root,
                    request,
                    web_result,
                    token,
                    tasks,
                    task_id,
                    max_task_progress,
                    expected_item_revision,
                    worker_revision,
                )?;
                if matches!(
                    &result.continuation,
                    Some(EngineContinuation::LocalOcr { .. })
                ) {
                    self.execute_local_ocr_continuation(
                        context,
                        files,
                        session_id,
                        item_id,
                        staging_root,
                        request,
                        result,
                        token,
                        tasks,
                        task_id,
                        max_task_progress,
                        expected_item_revision,
                        worker_revision,
                    )
                } else {
                    Ok(result)
                }
            }
            Some(EngineContinuation::LocalOcr { .. }) => self.execute_local_ocr_continuation(
                context,
                files,
                session_id,
                item_id,
                staging_root,
                request,
                web_result,
                token,
                tasks,
                task_id,
                max_task_progress,
                expected_item_revision,
                worker_revision,
            ),
            None => Ok(web_result),
        }
    }

    fn execute_local_asr_continuation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        staging_root: &str,
        request: &EngineRequest,
        mut web_result: EngineResult,
        token: &crate::tasks::task_model::CancellationToken,
        tasks: &TaskService,
        task_id: &str,
        max_task_progress: &Cell<u64>,
        expected_item_revision: &mut u64,
        worker_revision: &Cell<u64>,
    ) -> Result<EngineResult, BackendError> {
        let Some(EngineContinuation::LocalAsr {
            temporary_input_path,
            ..
        }) = web_result.continuation.take()
        else {
            return Ok(web_result);
        };
        let staging = context.root.join(staging_root);
        let media_path = staging.join(&temporary_input_path);
        let canonical_staging = staging.canonicalize().map_err(|_| asr_unavailable())?;
        let media_metadata =
            std::fs::symlink_metadata(&media_path).map_err(|_| asr_unavailable())?;
        if media_metadata.file_type().is_symlink() || !media_metadata.is_file() {
            return Err(asr_unavailable());
        }
        let canonical_media = media_path.canonicalize().map_err(|_| asr_unavailable())?;
        let media_workspace = canonical_media.parent().ok_or_else(asr_unavailable)?;
        if !canonical_media.starts_with(&canonical_staging)
            || media_workspace.parent() != Some(canonical_staging.as_path())
            || !media_workspace
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".asr-input-"))
        {
            return Err(asr_unavailable());
        }
        let _cleanup =
            crate::services::import_v2::media_router::TemporaryMediaWorkspace::adopt_existing(
                media_workspace,
            )?;
        let asr_input = ImportInput {
            kind: crate::models::import_v2::ImportInputKind::File,
            display_name: canonical_media
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            locator: canonical_media.to_string_lossy().into_owned(),
            normalized_locator: None,
            source_identity: None,
            media_save_mode: Default::default(),
        };
        let probe_embedded = request.input.kind == ImportInputKind::File;
        let companion_fallback = staging.join("transcripts/companion-fallback.md");
        let engine = match self
            .engines
            .resolve_media_asr(&asr_input, request.asr_profile.as_ref())
        {
            Ok(engine) => engine,
            Err(_) if companion_fallback.is_file() => {
                return apply_companion_transcript_fallback(context, files, &staging, web_result)
            }
            Err(error) => return Err(error),
        };
        if !request.local_asr_authorized && !probe_embedded {
            return Err(asr_unavailable());
        }
        let descriptor = describe_engine(engine.as_ref())?;
        let shard_key = asr_shard_key(&canonical_media, &descriptor)?;
        let shard_root = staging.join("asr-shards");
        let started_at = chrono::Utc::now().to_rfc3339();
        let mut asr_request = request.clone();
        asr_request.request_id = uuid::Uuid::new_v4().to_string();
        asr_request.input = asr_input;
        // Capability runners receive staging artifacts through the dedicated
        // relative-path field. Passing Rust's canonical Windows path here can
        // introduce a `\\?\` prefix that Node treats as a different root and
        // rejects with IMPORT_ASR_POLICY_BLOCKED.
        asr_request.chained_input = Some(temporary_input_path);
        let outcome = (|| -> Result<(EngineResult, Vec<String>), BackendError> {
            if let Some(cached) = load_completed_asr_shard(
                &shard_root,
                &shard_key,
                &descriptor,
                &staging,
                request.local_asr_authorized,
            )? {
                let base_path = staging.join(&web_result.markdown_path);
                let mut base =
                    std::fs::read_to_string(&base_path).map_err(|_| asr_unavailable())?;
                let transcript_root = staging.join("transcripts");
                files
                    .write_project_bytes_absolute(
                        context,
                        &transcript_root.join("local-asr.md"),
                        cached.transcript.as_bytes(),
                    )
                    .map_err(|_| asr_unavailable())?;
                web_result
                    .asset_paths
                    .push("transcripts/local-asr.md".into());
                if let Some(metadata) = cached.metadata {
                    files
                        .write_project_bytes_absolute(
                            context,
                            &transcript_root.join("local-asr.metadata.json"),
                            &metadata,
                        )
                        .map_err(|_| asr_unavailable())?;
                    web_result
                        .asset_paths
                        .push("transcripts/local-asr.metadata.json".into());
                }
                base.push_str(if cached.continuation.is_some() {
                    "\n\n## Video text probe\n\n"
                } else {
                    "\n\n## Local ASR Transcript\n\n"
                });
                base.push_str(&cached.transcript);
                files
                    .write_project_bytes_absolute(context, &base_path, base.as_bytes())
                    .map_err(|_| asr_unavailable())?;
                web_result.warnings.push(format!(
                    "local_asr:{}:{}",
                    descriptor.engine_id, descriptor.engine_version
                ));
                web_result
                    .warnings
                    .push("local_asr:reused-complete-shard".into());
                web_result.continuation = cached.continuation;
                return Ok((web_result, cached.warnings));
            }
            let report_asr_progress = |progress: EngineProgress| {
                let mapped = engine_progress_on_task_scale(&progress);
                if mapped < max_task_progress.get() {
                    return Ok(());
                }
                max_task_progress.set(mapped);
                if is_batch_operation_task(tasks, task_id) {
                    Ok(())
                } else {
                    task_call(tasks.update_progress(
                        task_id,
                        mapped,
                        Some(100),
                        Some(progress.label),
                    ))
                    .map(|_| ())
                }
            };
            let (mut asr_result, authorization_required) = if probe_embedded {
                asr_request.asr_probe_only = true;
                match execute_engine_with_progress(
                    engine.as_ref(),
                    &asr_request,
                    token,
                    &report_asr_progress,
                ) {
                    Ok(result) => (result, false),
                    Err(error) if error.code == "IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE" => {
                        if companion_fallback.is_file() {
                            return apply_companion_transcript_fallback(
                                context, files, &staging, web_result,
                            )
                            .map(|result| {
                                (result, vec!["IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE".into()])
                            });
                        }
                        if !request.local_asr_authorized {
                            return Err(asr_unavailable());
                        }
                        asr_request.asr_probe_only = false;
                        (
                            execute_engine_with_progress(
                                engine.as_ref(),
                                &asr_request,
                                token,
                                &report_asr_progress,
                            )?,
                            true,
                        )
                    }
                    Err(error) => return Err(error),
                }
            } else {
                asr_request.asr_probe_only = false;
                (
                    execute_engine_with_progress(
                        engine.as_ref(),
                        &asr_request,
                        token,
                        &report_asr_progress,
                    )?,
                    true,
                )
            };
            validate_engine_result(staging_root, &asr_result)?;
            let chained_continuation = asr_result.continuation.take();
            if chained_continuation.as_ref().is_some_and(|continuation| {
                !matches!(continuation, EngineContinuation::LocalOcr { .. })
            }) {
                return Err(asr_unavailable());
            }
            let output_path = staging
                .join(&asr_result.markdown_path)
                .canonicalize()
                .map_err(|_| asr_unavailable())?;
            let output_workspace = output_path.parent().ok_or_else(asr_unavailable)?;
            let output_metadata =
                std::fs::symlink_metadata(staging.join(&asr_result.markdown_path))
                    .map_err(|_| asr_unavailable())?;
            if output_metadata.file_type().is_symlink()
                || !output_metadata.is_file()
                || !output_path.starts_with(&canonical_staging)
                || !is_allowed_local_asr_output_workspace(&canonical_staging, output_workspace)
            {
                return Err(asr_unavailable());
            }
            let _output_cleanup =
                crate::services::import_v2::media_router::TemporaryMediaWorkspace::adopt_existing(
                    output_workspace,
                )?;
            if asr_result.markdown_path == web_result.markdown_path
                || asr_result.source_snapshot_path == web_result.source_snapshot_path
            {
                return Err(asr_unavailable());
            }
            let base_path = staging.join(&web_result.markdown_path);
            let transcript_path = staging.join(&asr_result.markdown_path);
            let mut base = std::fs::read_to_string(&base_path).map_err(|_| asr_unavailable())?;
            let transcript =
                std::fs::read_to_string(&transcript_path).map_err(|_| asr_unavailable())?;
            let transcript_metadata = if let Some(metadata_path) = &asr_result.metadata_path {
                Some(std::fs::read(staging.join(metadata_path)).map_err(|_| asr_unavailable())?)
            } else {
                None
            };
            store_completed_asr_shard(
                &context.root,
                &shard_root,
                &shard_key,
                &descriptor,
                &transcript,
                transcript_metadata.as_deref(),
                &asr_result.warnings,
                chained_continuation.as_ref(),
                authorization_required,
            )?;
            let transcript_root = staging.join("transcripts");
            let durable_transcript = transcript_root.join("local-asr.md");
            files
                .write_project_bytes_absolute(context, &durable_transcript, transcript.as_bytes())
                .map_err(|_| asr_unavailable())?;
            web_result
                .asset_paths
                .push("transcripts/local-asr.md".into());
            if let Some(metadata) = transcript_metadata {
                files
                    .write_project_bytes_absolute(
                        context,
                        &transcript_root.join("local-asr.metadata.json"),
                        &metadata,
                    )
                    .map_err(|_| asr_unavailable())?;
                web_result
                    .asset_paths
                    .push("transcripts/local-asr.metadata.json".into());
            }
            base.push_str(if chained_continuation.is_some() {
                "\n\n## Video text probe\n\n"
            } else {
                "\n\n## Local ASR Transcript\n\n"
            });
            base.push_str(&transcript);
            files
                .write_project_bytes_absolute(context, &base_path, base.as_bytes())
                .map_err(|_| asr_unavailable())?;
            for relative in std::iter::once(&asr_result.markdown_path)
                .chain(std::iter::once(&asr_result.source_snapshot_path))
                .chain(asr_result.metadata_path.iter())
                .chain(asr_result.asset_paths.iter())
            {
                let path = staging.join(relative);
                let _ = remove_project_file(&context.root, &path);
            }
            web_result.warnings.push(format!(
                "local_asr:{}:{}",
                descriptor.engine_id, descriptor.engine_version
            ));
            web_result.continuation = chained_continuation;
            Ok((web_result, asr_result.warnings))
        })();
        let (outcome_kind, warnings) = match &outcome {
            Ok((_, warnings)) => (
                crate::models::import_v2::AttemptOutcome::Succeeded,
                warnings.clone(),
            ),
            Err(_) => (crate::models::import_v2::AttemptOutcome::Failed, Vec::new()),
        };
        let error_code = outcome.as_ref().err().map(|error| error.code.clone());
        *expected_item_revision = self
            .record_attempt_claimed(
                context,
                files,
                session_id,
                item_id,
                task_id,
                *expected_item_revision,
                &descriptor,
                started_at,
                outcome_kind,
                error_code,
                warnings,
            )?
            .item_revision;
        worker_revision.set(*expected_item_revision);
        outcome.map(|(result, _)| result)
    }

    fn execute_local_ocr_continuation(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        staging_root: &str,
        request: &EngineRequest,
        mut web_result: EngineResult,
        token: &crate::tasks::task_model::CancellationToken,
        tasks: &TaskService,
        task_id: &str,
        max_task_progress: &Cell<u64>,
        expected_item_revision: &mut u64,
        worker_revision: &Cell<u64>,
    ) -> Result<EngineResult, BackendError> {
        let Some(EngineContinuation::LocalOcr {
            temporary_input_paths,
        }) = web_result.continuation.take()
        else {
            return Ok(web_result);
        };
        if temporary_input_paths.is_empty() || !request.local_ocr_authorized {
            return Err(ocr_unavailable());
        }
        let staging = context.root.join(staging_root);
        let canonical_staging = staging.canonicalize().map_err(|_| ocr_unavailable())?;
        let canonical_project_root = context.root.canonicalize().map_err(|_| ocr_unavailable())?;
        let base_path = staging.join(&web_result.markdown_path);
        let mut base = std::fs::read_to_string(&base_path).map_err(|_| ocr_unavailable())?;
        let durable_root = staging.join("ocr");
        BoundProjectMutationRoot::ensure_and_bind(
            &context.root,
            &durable_root.join(".wiki-ocr-directory-binding-probe"),
        )
        .map_err(|_| ocr_unavailable())?;

        let image_total = temporary_input_paths.len() as u64;
        let mut successful_ocr = 0usize;
        let mut first_ocr_error = None;
        let mut protected_workspaces = HashSet::new();
        let mut cleanup_guards = Vec::new();
        for (index, temporary_input_path) in temporary_input_paths.iter().enumerate() {
            if token.is_cancelled() {
                return Err(cancelled_error());
            }
            update_continuation_progress(
                tasks,
                task_id,
                max_task_progress,
                EngineProgress {
                    current: index as u64,
                    total: Some(image_total),
                    label: "ocr.recognizing".into(),
                },
            )?;
            let input_path = staging.join(temporary_input_path);
            let input_metadata =
                std::fs::symlink_metadata(&input_path).map_err(|_| ocr_unavailable())?;
            if input_metadata.file_type().is_symlink() || !input_metadata.is_file() {
                return Err(ocr_unavailable());
            }
            let canonical_input = input_path.canonicalize().map_err(|_| ocr_unavailable())?;
            let source_image_number = ocr_source_image_number(&canonical_input, index);
            let workspace = canonical_input.parent().ok_or_else(ocr_unavailable)?;
            if !canonical_input.starts_with(&canonical_staging)
                || workspace.parent() != Some(canonical_staging.as_path())
                || !workspace
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(".ocr-input-"))
            {
                return Err(ocr_unavailable());
            }
            if protected_workspaces.insert(workspace.to_path_buf()) {
                cleanup_guards.push(
                    crate::services::import_v2::media_router::TemporaryMediaWorkspace::adopt_existing(
                        workspace,
                    )?,
                );
            }
            let ocr_input = ImportInput {
                kind: crate::models::import_v2::ImportInputKind::File,
                display_name: canonical_input
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                locator: canonical_input.to_string_lossy().into_owned(),
                normalized_locator: None,
                source_identity: None,
                media_save_mode: crate::models::import_v2::MediaSaveMode::ExtractOnly,
            };
            let engine = self
                .engines
                .resolve_route("ocr.cjk-accurate", &ocr_input)
                .or_else(|_| self.engines.resolve_route("ocr.basic", &ocr_input))?;
            let descriptor = describe_engine(engine.as_ref())?;
            let started_at = chrono::Utc::now().to_rfc3339();
            let shard_key = ocr_shard_key(&canonical_input, &descriptor)?;
            let shard_root = staging.join("ocr-shards");
            let workspace_relative = workspace
                .strip_prefix(&canonical_project_root)
                .map_err(|_| ocr_unavailable())?
                .to_string_lossy()
                .replace('\\', "/");
            let mut ocr_request = request.clone();
            ocr_request.request_id = uuid::Uuid::new_v4().to_string();
            ocr_request.input = ocr_input;
            ocr_request.staging_root = workspace_relative.clone();
            ocr_request.chained_input = None;
            ocr_request.local_ocr_authorized = false;
            let outcome = (|| -> Result<(String, EngineResult), BackendError> {
                if let Some((markdown, metadata)) =
                    load_completed_ocr_shard(&shard_root, &shard_key, &descriptor)?
                {
                    files
                        .write_project_bytes_absolute(
                            context,
                            &workspace.join("reused-candidate.md"),
                            markdown.as_bytes(),
                        )
                        .and_then(|_| {
                            files.write_project_bytes_absolute(
                                context,
                                &workspace.join("reused-source.json"),
                                br#"{"provenance":"reused-complete-ocr-shard"}"#,
                            )
                        })
                        .map_err(|_| ocr_unavailable())?;
                    let metadata_path = if let Some(ref metadata) = metadata {
                        files
                            .write_project_bytes_absolute(
                                context,
                                &workspace.join("reused-metadata.json"),
                                metadata,
                            )
                            .map_err(|_| ocr_unavailable())?;
                        Some("reused-metadata.json".into())
                    } else {
                        None
                    };
                    let confidence_warnings = if descriptor.route == "ocr.cjk-accurate" {
                        validate_ocr_confidence_metadata(
                            metadata.as_deref().ok_or_else(ocr_low_confidence)?,
                            source_image_number,
                        )?
                    } else {
                        Vec::new()
                    };
                    let result = EngineResult {
                        source_snapshot_path: "reused-source.json".into(),
                        markdown_path: "reused-candidate.md".into(),
                        asset_paths: Vec::new(),
                        metadata_path,
                        title: format!("OCR image {source_image_number}"),
                        text_coverage: Some(1.0),
                        table_cell_accuracy: None,
                        sheet_count_exact: None,
                        slide_count_exact: None,
                        non_empty_cell_coverage: None,
                        formula_value_pairs: None,
                        meaningful_image_coverage: None,
                        continuation: None,
                        warnings: std::iter::once("IMPORT_OCR_REUSED_COMPLETE_SHARD".into())
                            .chain(confidence_warnings)
                            .collect(),
                    };
                    validate_engine_result(&workspace_relative, &result)?;
                    return Ok((markdown, result));
                }
                let mut result = execute_engine(engine.as_ref(), &ocr_request, token)?;
                validate_engine_result(&workspace_relative, &result)?;
                if result.continuation.is_some() {
                    return Err(ocr_unavailable());
                }
                let output_path = workspace
                    .join(&result.markdown_path)
                    .canonicalize()
                    .map_err(|_| ocr_unavailable())?;
                if !output_path.starts_with(workspace) || !output_path.is_file() {
                    return Err(ocr_unavailable());
                }
                let markdown =
                    std::fs::read_to_string(output_path).map_err(|_| ocr_unavailable())?;
                if markdown.trim().is_empty()
                    || result.text_coverage.unwrap_or_default() <= 0.0
                    || result
                        .warnings
                        .iter()
                        .any(|warning| warning == "IMPORT_OCR_NO_TEXT")
                {
                    return Err(ocr_no_text());
                }
                let metadata = result
                    .metadata_path
                    .as_ref()
                    .map(|path| std::fs::read(workspace.join(path)))
                    .transpose()
                    .map_err(|_| ocr_unavailable())?;
                if descriptor.route == "ocr.cjk-accurate" {
                    let confidence_warnings = validate_ocr_confidence_metadata(
                        metadata.as_deref().ok_or_else(ocr_low_confidence)?,
                        source_image_number,
                    )?;
                    result.warnings.extend(confidence_warnings);
                }
                store_completed_ocr_shard(
                    &context.root,
                    &shard_root,
                    &shard_key,
                    &descriptor,
                    &markdown,
                    metadata.as_deref(),
                )?;
                Ok((markdown, result))
            })();
            let (ocr_markdown, ocr_result) = match outcome {
                Ok(value) => {
                    *expected_item_revision = self
                        .record_attempt_claimed(
                            context,
                            files,
                            session_id,
                            item_id,
                            task_id,
                            *expected_item_revision,
                            &descriptor,
                            started_at,
                            crate::models::import_v2::AttemptOutcome::Succeeded,
                            None,
                            value.1.warnings.clone(),
                        )?
                        .item_revision;
                    worker_revision.set(*expected_item_revision);
                    value
                }
                Err(error) => {
                    *expected_item_revision = self
                        .record_attempt_claimed(
                            context,
                            files,
                            session_id,
                            item_id,
                            task_id,
                            *expected_item_revision,
                            &descriptor,
                            started_at,
                            crate::models::import_v2::AttemptOutcome::Failed,
                            Some(error.code.clone()),
                            Vec::new(),
                        )?
                        .item_revision;
                    worker_revision.set(*expected_item_revision);
                    first_ocr_error.get_or_insert_with(|| error.clone());
                    web_result.warnings.push(format!(
                        "Local OCR failed for source image {source_image_number}: {}",
                        error.code
                    ));
                    continue;
                }
            };
            successful_ocr += 1;
            let durable_markdown = format!("ocr/image-{source_image_number:03}.md");
            files
                .write_project_bytes_absolute(
                    context,
                    &staging.join(&durable_markdown),
                    ocr_markdown.as_bytes(),
                )
                .map_err(|_| ocr_unavailable())?;
            web_result.asset_paths.push(durable_markdown);
            if let Some(metadata_path) = &ocr_result.metadata_path {
                let metadata =
                    std::fs::read(workspace.join(metadata_path)).map_err(|_| ocr_unavailable())?;
                let durable_metadata = format!("ocr/image-{source_image_number:03}.metadata.json");
                files
                    .write_project_bytes_absolute(
                        context,
                        &staging.join(&durable_metadata),
                        &metadata,
                    )
                    .map_err(|_| ocr_unavailable())?;
                web_result.asset_paths.push(durable_metadata);
            }
            let pdf_placeholder = format!("<!-- OCR_PAGE_{source_image_number:03} -->");
            if base.contains(&pdf_placeholder) {
                base = base.replace(
                    &pdf_placeholder,
                    &format!(
                        "> 本页文字由本地 OCR {} {} 提取。\n\n{}",
                        descriptor.engine_id, descriptor.engine_version, ocr_markdown
                    ),
                );
            } else {
                base.push_str(&format!(
                    "\n\n## 图片文字 / OCR — 第 {source_image_number} 张\n\n\
                     > 来源：本地 OCR · {} {}\n\n",
                    descriptor.engine_id, descriptor.engine_version
                ));
                base.push_str(&ocr_markdown);
            }
            web_result.warnings.push(format!(
                "local_ocr:{}:{}",
                descriptor.engine_id, descriptor.engine_version
            ));
        }
        update_continuation_progress(
            tasks,
            task_id,
            max_task_progress,
            EngineProgress {
                current: image_total,
                total: Some(image_total),
                label: "ocr.recognizing".into(),
            },
        )?;
        if successful_ocr != temporary_input_paths.len() || base.contains("<!-- OCR_PAGE_") {
            return Err(first_ocr_error.unwrap_or_else(ocr_unavailable));
        }
        files
            .write_project_bytes_absolute(context, &base_path, base.as_bytes())
            .map_err(|_| ocr_unavailable())?;
        web_result.text_coverage = Some(1.0);
        Ok(web_result)
    }

    fn planned_routes(
        &self,
        context: &ProjectContext,
        input: &ImportInput,
        recovery_action: Option<&ImportRecoveryAction>,
    ) -> Result<Vec<(&'static str, QualityFloor)>, BackendError> {
        let Some(format) = detect_input_format(context, input)? else {
            return Ok(reorder_routes(
                explicit_routes(input)
                    .into_iter()
                    .map(|route| {
                        let floor = match route {
                            "pdf.text" | "pdf.layout" => QualityFloor::DeterministicDocument,
                            _ => QualityFloor::ComparisonFallback,
                        };
                        (route, floor)
                    })
                    .collect(),
                recovery_action,
            ));
        };
        if !matches!(
            format,
            FileFormat::Doc
                | FileFormat::Docx
                | FileFormat::Xls
                | FileFormat::Xlsx
                | FileFormat::Ppt
                | FileFormat::Pptx
        ) {
            return Ok(reorder_routes(
                routes_for_format(format)
                    .into_iter()
                    .map(|route| {
                        let floor = match route {
                            "pdf.text" | "pdf.layout" => QualityFloor::DeterministicDocument,
                            _ => QualityFloor::ComparisonFallback,
                        };
                        (route, floor)
                    })
                    .collect(),
                recovery_action,
            ));
        }
        let routes = self.engines.registered_routes()?;
        let has = |route: &str| routes.iter().any(|registered| registered == route);
        let capabilities = CapabilitySnapshot {
            document_standard: has("pack.markitdown"),
            office_legacy: has("pack.office-legacy"),
            office_oxide_installed: has("pack.office-oxide"),
            office_oxide_qualified: has("pack.office-oxide"),
            agent_available: has("agent.office"),
        };
        Ok(reorder_routes(
            FileRoutePlanner::plan(format, capabilities)
                .into_iter()
                .map(|attempt| -> Result<_, BackendError> {
                    let route = attempt.route;
                    let floor = match self
                        .engines
                        .resolve_route(route, &route_resolution_input(route, input))
                    {
                        Ok(engine)
                            if describe_engine(engine.as_ref())?
                                .engine_id
                                .starts_with("builtin.") =>
                        {
                            QualityFloor::DeterministicDocument
                        }
                        Ok(_) => attempt.quality_floor,
                        Err(error) if error.code == IMPORT_V2_ENGINE_PANICKED => return Err(error),
                        Err(_) => attempt.quality_floor,
                    };
                    Ok((route, floor))
                })
                .collect::<Result<Vec<_>, _>>()?,
            recovery_action,
        ))
    }

    fn record_attempt(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        descriptor: &crate::services::import_v2::engine::EngineDescriptor,
        started_at: String,
        outcome: crate::models::import_v2::AttemptOutcome,
        error_code: Option<String>,
        warnings: Vec<String>,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            item.attempts.push(crate::models::import_v2::AttemptRecord {
                route: descriptor.route.clone(),
                engine_id: descriptor.engine_id.clone(),
                engine_version: descriptor.engine_version.clone(),
                stage: ImportStage::Extract,
                started_at,
                completed_at: Some(chrono::Utc::now().to_rfc3339()),
                outcome,
                error_code,
                warnings,
            });
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn record_attempt_claimed(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
        descriptor: &crate::services::import_v2::engine::EngineDescriptor,
        started_at: String,
        outcome: crate::models::import_v2::AttemptOutcome,
        error_code: Option<String>,
        warnings: Vec<String>,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            expected_item_revision,
            |item| {
                item.attempts.push(crate::models::import_v2::AttemptRecord {
                    route: descriptor.route.clone(),
                    engine_id: descriptor.engine_id.clone(),
                    engine_version: descriptor.engine_version.clone(),
                    stage: ImportStage::Extract,
                    started_at,
                    completed_at: Some(chrono::Utc::now().to_rfc3339()),
                    outcome,
                    error_code,
                    warnings,
                });
                Ok(())
            },
        )
    }

    fn claim_item_for_run(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        task_id: &str,
        mut snapshot: ImportWorkItemSnapshot,
        pre_cancelled: bool,
    ) -> Result<ImportWorkItemSnapshot, BackendError> {
        let item_id = snapshot.item_id.clone();
        let claimed = self.mutate_claimed_item(
            context,
            files,
            session_id,
            &item_id,
            task_id,
            snapshot.expected_item_revision,
            |item| {
                if !matches!(
                    item.status,
                    ImportItemStatus::Queued
                        | ImportItemStatus::Failed
                        | ImportItemStatus::WaitingCapability
                        | ImportItemStatus::WaitingLogin
                        | ImportItemStatus::WaitingAuthorization
                        | ImportItemStatus::Cancelled
                        | ImportItemStatus::Skipped
                        | ImportItemStatus::Paused
                        | ImportItemStatus::PreviewReady
                ) || !matches!(
                    item.status,
                    ImportItemStatus::Failed
                        | ImportItemStatus::WaitingCapability
                        | ImportItemStatus::WaitingLogin
                        | ImportItemStatus::WaitingAuthorization
                ) && item
                    .task_id
                    .as_deref()
                    .is_some_and(|bound| bound != task_id)
                {
                    return Err(task_error(
                        "Import item is already claimed by another task.",
                    ));
                }
                item.task_id = Some(task_id.to_string());
                item.issue = None;
                if item.status == ImportItemStatus::Skipped {
                    item.selected = true;
                }
                if pre_cancelled {
                    // Keep the claim live until `finish_cancelled` performs the
                    // single terminal transition and cleanup with this revised
                    // snapshot. Clearing the claim here would make that CAS
                    // look stale and misreport a pre-cancelled task.
                    return Ok(());
                }
                transition_item(item, ImportItemStatus::Inspecting)?;
                Ok(())
            },
        )?;
        snapshot.expected_item_revision = claimed.item_revision;
        Ok(snapshot)
    }

    fn start_claimed_task(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
    ) -> Result<(), BackendError> {
        if tasks.is_cancelled(task_id) {
            return self
                .finish_cancelled(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    expected_item_revision,
                )
                .map(|_| ());
        }
        if let Err(error) = tasks.transition_status(task_id, TaskStatus::Running) {
            if tasks.is_cancelled(task_id)
                || tasks
                    .get_task(task_id)
                    .is_some_and(|task| task.status == TaskStatus::Cancelled)
            {
                return self
                    .finish_cancelled(
                        context,
                        files,
                        tasks,
                        session_id,
                        item_id,
                        task_id,
                        expected_item_revision,
                    )
                    .map(|_| ());
            }
            return Err(task_call::<()>(Err(error)).unwrap_err());
        }
        Ok(())
    }

    fn finish_cancelled(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
    ) -> Result<ImportItem, BackendError> {
        let staging = context
            .resolve_project_path(&item_staging_relative_path(context, session_id, item_id)?)?;
        let batch_operation = is_batch_operation_task(tasks, task_id);
        if let Err(error) = cleanup_terminal_item_staging(&context.root, &staging) {
            if batch_operation {
                // The operation-level summary is published by the worker
                // cohort; item cleanup warnings remain durable item facts.
            } else {
                task_call(tasks.append_log(
                    task_id,
                    LogLevel::Warn,
                    format!(
                        "Import cancellation could not fully remove temporary media: {}",
                        error.code
                    ),
                ))?;
            }
        }
        let item = self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            expected_item_revision,
            |item| {
                if item.status == ImportItemStatus::Skipped {
                    return Ok(());
                }
                if item.status != ImportItemStatus::Cancelled {
                    transition_item(item, ImportItemStatus::Cancelled)?;
                }
                item.task_id = None;
                item.progress = None;
                Ok(())
            },
        )?;
        remove_clipboard_session_input(context, session_id, &item.input);
        if !batch_operation
            && tasks
                .get_task(task_id)
                .is_some_and(|task| task.status != TaskStatus::Cancelled)
        {
            task_call(tasks.cancel_task(task_id))?;
        }
        Err(cancelled_error())
    }

    fn terminalize_in_flight_worker_error(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
        error: &BackendError,
    ) {
        let item = self
            .sessions
            .load_item(context, files, session_id, item_id)
            .ok();
        let Some(item) = item else {
            return;
        };
        if item.item_revision != expected_item_revision || item.task_id.as_deref() != Some(task_id)
        {
            return;
        }
        let batch_operation = is_batch_operation_task(tasks, task_id);
        if matches!(
            item.status,
            ImportItemStatus::Inspecting
                | ImportItemStatus::Extracting
                | ImportItemStatus::Validating
        ) {
            if tasks.is_cancelled(task_id) || error.code == IMPORT_V2_CANCELLED {
                let _ = self.finish_cancelled(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    expected_item_revision,
                );
            } else {
                let _ = self.finish_failed(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    expected_item_revision,
                    error.clone(),
                    ImportStage::Extract,
                );
            }
        } else if !batch_operation
            && item.status == ImportItemStatus::Failed
            && tasks.get_task(task_id).is_some_and(|task| {
                !matches!(
                    task.status,
                    TaskStatus::Succeeded | TaskStatus::Failed | TaskStatus::Cancelled
                )
            })
        {
            // A panic can occur after the item mutation but before the task
            // transition. Repair that half-written terminal state here.
            let _ = tasks.set_error(task_id, issue_safe_error(error));
            let _ = tasks.transition_status(task_id, TaskStatus::Failed);
        }
    }

    fn finish_failed(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
        error: BackendError,
        stage: ImportStage,
    ) -> Result<ImportItem, BackendError> {
        let staging = context
            .resolve_project_path(&item_staging_relative_path(context, session_id, item_id)?)?;
        let batch_operation = is_batch_operation_task(tasks, task_id);
        if let Err(cleanup_error) = cleanup_terminal_item_staging(&context.root, &staging) {
            if !batch_operation {
                task_call(tasks.append_log(
                    task_id,
                    LogLevel::Warn,
                    format!(
                        "Import failure could not fully remove temporary media: {}",
                        cleanup_error.code
                    ),
                ))?;
            }
        }
        self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            expected_item_revision,
            |item| {
                transition_item(item, ImportItemStatus::Failed)?;
                let mut issue = issue_from_engine_error_for_input(&error, stage, &item.input.kind);
                if is_agent_eligible_failure(&error.code, &issue) {
                    issue.available_actions =
                        vec![crate::models::import_v2_agent::AgentRecoveryAction::InvokeLocalAgent];
                }
                item.issue = Some(issue);
                Ok(())
            },
        )?;
        if !batch_operation {
            task_call(tasks.append_log(task_id, LogLevel::Error, "Import engine failed.".into()))?;
            task_call(tasks.set_error(task_id, issue_safe_error(&error)))?;
            task_call(tasks.transition_status(task_id, TaskStatus::Failed))?;
        }
        Err(issue_safe_error(&error))
    }
    fn finish_waiting_login(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
        error: BackendError,
        stage: ImportStage,
    ) -> Result<ImportItem, BackendError> {
        let item = self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            expected_item_revision,
            |item| {
                transition_item(item, ImportItemStatus::WaitingLogin)?;
                item.issue = Some(ImportIssue::for_web_code(&error.code, stage));
                Ok(())
            },
        )?;
        if !is_batch_operation_task(tasks, task_id) {
            task_call(tasks.append_log(
                task_id,
                LogLevel::Warn,
                "Web import is waiting for user authentication.".into(),
            ))?;
            task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
        }
        Ok(item)
    }

    fn finish_waiting_local_asr(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
        error: BackendError,
        stage: ImportStage,
    ) -> Result<ImportItem, BackendError> {
        let asr_available = self
            .engines
            .registered_routes()?
            .iter()
            .any(|route| route == "media.asr");
        let item = self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            expected_item_revision,
            |item| {
                transition_item(
                    item,
                    if asr_available {
                        ImportItemStatus::WaitingAuthorization
                    } else {
                        ImportItemStatus::WaitingCapability
                    },
                )?;
                let mut issue = ImportIssue::for_web_code(&error.code, stage);
                issue.recovery_actions.retain(|action| {
                    if asr_available {
                        !matches!(action, ImportRecoveryAction::InstallMediaCapability)
                    } else {
                        !matches!(action, ImportRecoveryAction::AuthorizeLocalAsr)
                    }
                });
                item.issue = Some(issue);
                Ok(())
            },
        )?;
        if !is_batch_operation_task(tasks, task_id) {
            task_call(tasks.append_log(
                task_id,
                LogLevel::Warn,
                if asr_available {
                    "Media import is waiting for explicit local ASR authorization.".into()
                } else {
                    "Media import is waiting for the local ASR capability.".into()
                },
            ))?;
            task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
        }
        Ok(item)
    }

    fn finish_waiting_subtitle_selection(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
        error: BackendError,
        stage: ImportStage,
    ) -> Result<ImportItem, BackendError> {
        let item = self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            expected_item_revision,
            |item| {
                transition_item(item, ImportItemStatus::WaitingAuthorization)?;
                item.selected_subtitle = None;
                item.issue = Some(issue_from_engine_error(&error, stage));
                Ok(())
            },
        )?;
        if !is_batch_operation_task(tasks, task_id) {
            task_call(tasks.append_log(
                task_id,
                LogLevel::Warn,
                "Media import is waiting for an explicit subtitle selection.".into(),
            ))?;
            task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
        }
        Ok(item)
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_waiting_local_ocr(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
        error: BackendError,
        stage: ImportStage,
    ) -> Result<ImportItem, BackendError> {
        let ocr_available = self
            .engines
            .registered_routes()?
            .iter()
            .any(|route| route == "ocr.cjk-accurate" || route == "ocr.basic");
        let item = self.mutate_claimed_item(
            context,
            files,
            session_id,
            item_id,
            task_id,
            expected_item_revision,
            |item| {
                transition_item(
                    item,
                    if ocr_available {
                        ImportItemStatus::WaitingAuthorization
                    } else {
                        ImportItemStatus::WaitingCapability
                    },
                )?;
                let mut issue = ImportIssue::for_web_code(&error.code, stage);
                issue.recovery_actions.retain(|action| {
                    if ocr_available {
                        !matches!(action, ImportRecoveryAction::InstallOcrCapability)
                    } else {
                        !matches!(action, ImportRecoveryAction::EnableOcr)
                    }
                });
                if ocr_available
                    && !issue
                        .recovery_actions
                        .contains(&ImportRecoveryAction::EnableOcr)
                {
                    issue
                        .recovery_actions
                        .insert(0, ImportRecoveryAction::EnableOcr);
                }
                item.issue = Some(issue);
                Ok(())
            },
        )?;
        if !is_batch_operation_task(tasks, task_id) {
            task_call(tasks.append_log(
                task_id,
                LogLevel::Warn,
                "The import is waiting for the local OCR capability.".into(),
            ))?;
            task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
        }
        Ok(item)
    }

    fn mutate_item<F>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        mutation: F,
    ) -> Result<ImportItem, BackendError>
    where
        F: FnOnce(&mut ImportItem) -> Result<(), BackendError>,
    {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut item = self
            .sessions
            .load_item(context, files, session_id, item_id)?;
        let before = new_source_reservation_fingerprint(&item);
        mutation(&mut item)?;
        let should_refresh_targets = before != new_source_reservation_fingerprint(&item);
        if should_refresh_targets {
            let mut session = self.sessions.load(context, files, session_id)?;
            *find_item_mut(&mut session, item_id)? = item.clone();
            crate::services::import_v2::commit::refresh_new_source_wiki_targets(
                context,
                files,
                &mut session,
            )?;
            let item = find_item_mut(&mut session, item_id)?.clone();
            persist_derived(&self.sessions, context, files, session)?;
            return Ok(item);
        }
        self.sessions
            .write_item(context, files, session_id, &item)?;
        self.sessions.load_item(context, files, session_id, item_id)
    }

    #[allow(clippy::too_many_arguments)]
    fn mutate_claimed_item<F>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
        mutation: F,
    ) -> Result<ImportItem, BackendError>
    where
        F: FnOnce(&mut ImportItem) -> Result<(), BackendError>,
    {
        self.mutate_claimed_item_with_before_write(
            context,
            files,
            session_id,
            item_id,
            task_id,
            expected_item_revision,
            mutation,
            |_| Ok(()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn mutate_claimed_item_with_before_write<F, H>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        expected_item_revision: u64,
        mutation: F,
        before_write: H,
    ) -> Result<ImportItem, BackendError>
    where
        F: FnOnce(&mut ImportItem) -> Result<(), BackendError>,
        H: FnOnce(&ImportItem) -> Result<(), BackendError>,
    {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let before = self
            .sessions
            .load_item(context, files, session_id, item_id)?;
        if before.item_revision != expected_item_revision
            || before.task_id.as_deref() != Some(task_id)
        {
            return Err(work_item_stale_error());
        }
        let mut item = before.clone();
        mutation(&mut item)?;
        before_write(&before)?;
        if let Err(error) = self.sessions.write_item_cohort_if_unchanged(
            context,
            files,
            session_id,
            std::slice::from_ref(&before),
            std::slice::from_ref(&item),
        ) {
            if error.code == crate::errors::IMPORT_V2_COMMIT_CONFLICT {
                return Err(work_item_stale_error());
            }
            return Err(error);
        }
        self.sessions.load_item(context, files, session_id, item_id)
    }

    fn mutate_preview_item<F>(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        mutation: F,
    ) -> Result<ImportItem, BackendError>
    where
        F: FnOnce(&mut ImportItem) -> Result<(), BackendError>,
    {
        let _guard = self.lock()?;
        self.preflight_locked(context)?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item_position = session
            .items
            .iter()
            .position(|item| item.item_id == item_id)
            .ok_or_else(item_not_found)?;
        mutation(&mut session.items[item_position])?;
        crate::services::import_v2::commit::refresh_new_source_wiki_targets(
            context,
            files,
            &mut session,
        )?;
        let item = session.items[item_position].clone();
        persist_derived(&self.sessions, context, files, session)?;
        Ok(item)
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, BackendError> {
        // This lock serializes durable filesystem transactions; it does not
        // protect an in-memory invariant. FileTransaction rolls back during
        // unwind, so recovering a poisoned guard keeps later imports usable
        // after a worker panic without exposing a half-applied cohort.
        #[cfg(feature = "performance-observers")]
        let wait_started = std::time::Instant::now();
        let guard = self
            .mutation_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(feature = "performance-observers")]
        {
            let wait = wait_started.elapsed();
            let observer = self
                .lock_wait_observer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            if let Some(observer) = observer {
                let mut snapshot = observer
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let nanos = wait.as_nanos().min(u64::MAX as u128) as u64;
                snapshot.acquisitions += 1;
                snapshot.total_wait_nanos = snapshot.total_wait_nanos.saturating_add(nanos);
                snapshot.max_wait_nanos = snapshot.max_wait_nanos.max(nanos);
                snapshot.waits_over_50_ms +=
                    u64::from(wait >= std::time::Duration::from_millis(50));
            }
        }
        Ok(guard)
    }

    pub(crate) fn with_agent_candidate_action_lock<T>(
        &self,
        action: impl FnOnce() -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        let _guard = self
            .agent_candidate_action_lock
            .lock()
            .map_err(|_| task_error("Agent candidate action lock is unavailable."))?;
        action()
    }

    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    pub(crate) fn reserve_source_ai(&self, key: String) -> Result<(), BackendError> {
        let mut active = self
            .source_ai_active
            .lock()
            .map_err(|_| task_error("Source AI task registry is unavailable."))?;
        if !active.insert(key) {
            return Err(BackendError::new(
                "SOURCE_AI_ALREADY_RUNNING",
                "An AI organization task is already active for this Source.",
                true,
                true,
            ));
        }
        Ok(())
    }

    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    pub(crate) fn release_source_ai(&self, key: &str) {
        if let Ok(mut active) = self.source_ai_active.lock() {
            active.remove(key);
        }
    }

    #[cfg(test)]
    pub(crate) fn has_source_ai_reservation(&self, key: &str) -> bool {
        self.source_ai_active
            .lock()
            .is_ok_and(|active| active.contains(key))
    }

    fn preflight_locked(&self, context: &ProjectContext) -> Result<(), BackendError> {
        FileTransaction::reconcile_project(&context.root)
    }

    pub(crate) fn acquire_migration_lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, ()>, BackendError> {
        self.lock()
    }

    pub(crate) fn preflight_migration_locked(
        &self,
        context: &ProjectContext,
    ) -> Result<(), BackendError> {
        self.preflight_locked(context)
    }
}

fn is_agent_eligible_failure(original_code: &str, issue: &ImportIssue) -> bool {
    let original_is_eligible = matches!(
        original_code,
        crate::errors::IMPORT_V2_ENGINE_OUTPUT_INVALID
            | crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE
            | crate::errors::IMPORT_V2_CAPABILITY_UNAVAILABLE
            | crate::errors::IMPORT_V2_QUALITY_FAILED
            | "IMPORT_WEB_STRUCTURE_CHANGED"
            | "IMPORT_WEB_SUBTITLE_UNAVAILABLE"
            | "IMPORT_ASR_TIMEOUT"
            | "IMPORT_ASR_ENGINE_FAILED"
            | "IMPORT_ASR_OUTPUT_INVALID"
    ) || original_code.contains("PARSE")
        || original_code.contains("CORRUPT")
        || original_code.contains("CONVERSION");
    original_is_eligible
        && matches!(
            issue.code.as_str(),
            "IMPORT_FILE_PARSE_FAILED"
                | "IMPORT_FILE_CORRUPT"
                | "IMPORT_FILE_CONVERSION_FAILED"
                | "IMPORT_FILE_QUALITY_FAILED"
                | "IMPORT_FILE_CAPABILITY_MISSING"
                | "IMPORT_WEB_STRUCTURE_CHANGED"
                | "IMPORT_WEB_SUBTITLE_UNAVAILABLE"
                | "IMPORT_ASR_TIMEOUT"
                | "IMPORT_ASR_ENGINE_FAILED"
                | "IMPORT_ASR_OUTPUT_INVALID"
        )
}

fn reorder_routes(
    mut routes: Vec<(&'static str, QualityFloor)>,
    recovery_action: Option<&ImportRecoveryAction>,
) -> Vec<(&'static str, QualityFloor)> {
    match recovery_action {
        Some(ImportRecoveryAction::SwitchRoute) if routes.len() > 1 => routes.rotate_left(1),
        Some(ImportRecoveryAction::SwitchParser) if routes.len() > 1 => routes.rotate_left(1),
        Some(ImportRecoveryAction::EnableOcr) => {
            routes.sort_by_key(|(route, _)| {
                if route.starts_with("ocr.") {
                    0
                } else if *route == "pdf.layout" {
                    1
                } else {
                    2
                }
            });
        }
        Some(ImportRecoveryAction::RetryRoute) | _ => {}
    }
    routes
}

fn detect_input_format(
    context: &ProjectContext,
    input: &ImportInput,
) -> Result<Option<FileFormat>, BackendError> {
    if input.kind == ImportInputKind::Url {
        return Ok(None);
    }
    let locator = Path::new(&input.locator);
    let path = if locator.is_absolute() {
        locator.to_path_buf()
    } else {
        context.root.join(locator)
    };
    let prefix = std::fs::File::open(&path)
        .and_then(|file| {
            use std::io::Read;
            let mut bytes = Vec::new();
            file.take(8192).read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .map_err(|error| {
            BackendError::new(
                "IMPORT_FILE_IO",
                format!("The selected source could not be inspected: {error}"),
                true,
                true,
            )
        })?;
    crate::services::import_v2::file_discovery::identify_file(&path, &prefix)
        .map(|(format, _)| Some(format))
}

/// Canonical Batch 3 route contract. Discovery and contract tests share this
/// function so adding a supported local format cannot silently diverge from
/// the production orchestrator.
pub fn routes_for_format(format: FileFormat) -> Vec<&'static str> {
    match format {
        FileFormat::Markdown | FileFormat::Text | FileFormat::Html => vec!["file.native"],
        FileFormat::Csv => vec!["file.csv-package"],
        FileFormat::Docx => vec![
            "office.modern.docx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        FileFormat::Xlsx => vec![
            "office.modern.xlsx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        FileFormat::Pptx => vec![
            "office.modern.pptx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        FileFormat::Doc | FileFormat::Xls | FileFormat::Ppt => {
            vec!["pack.office-legacy", "pack.office-oxide", "agent.office"]
        }
        FileFormat::Pdf => vec![
            "pdf.text",
            "pdf.layout",
            "ocr.cjk-accurate",
            "ocr.basic",
            "agent.pdf",
        ],
        FileFormat::Srt | FileFormat::Vtt | FileFormat::Ass | FileFormat::Lrc => {
            vec!["media.subtitle"]
        }
        FileFormat::Mp3
        | FileFormat::Wav
        | FileFormat::M4a
        | FileFormat::Aac
        | FileFormat::Flac
        | FileFormat::Ogg
        | FileFormat::Opus
        | FileFormat::Wma
        | FileFormat::Mp4
        | FileFormat::Mov
        | FileFormat::Mkv
        | FileFormat::Webm
        | FileFormat::Avi
        | FileFormat::M4v
        | FileFormat::Wmv
        | FileFormat::AnimatedGif => vec!["media.companion", "media.asr"],
        FileFormat::Png
        | FileFormat::Jpeg
        | FileFormat::Webp
        | FileFormat::Bmp
        | FileFormat::Tiff
        | FileFormat::Heic
        | FileFormat::Heif => vec!["ocr.cjk-accurate", "ocr.basic"],
    }
}

fn explicit_routes(input: &ImportInput) -> Vec<&'static str> {
    if input.kind == crate::models::import_v2::ImportInputKind::Url {
        let host = url::Url::parse(
            input
                .normalized_locator
                .as_deref()
                .unwrap_or(&input.locator),
        )
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
        if host == "xiaohongshu.com"
            || host.ends_with(".xiaohongshu.com")
            || host == "xhslink.com"
            || host.ends_with(".xhslink.com")
            || host == "xhslink.cn"
            || host.ends_with(".xhslink.cn")
        {
            return vec![
                "web.generic.browser",
                "web.xiaohongshu.note",
                "web.generic.readability",
            ];
        }
        if host == "douyin.com"
            || host.ends_with(".douyin.com")
            || host == "iesdouyin.com"
            || host.ends_with(".iesdouyin.com")
        {
            return vec![
                "web.generic.browser",
                "web.douyin.video",
                "web.generic.readability",
            ];
        }
        if host == "x.com"
            || host.ends_with(".x.com")
            || host == "twitter.com"
            || host.ends_with(".twitter.com")
        {
            return vec!["web.x.post"];
        }
        if host == "bilibili.com" || host.ends_with(".bilibili.com") || host == "b23.tv" {
            return vec![
                "web.bilibili.video",
                "web.bilibili.metadata",
                "web.generic.browser",
            ];
        }
        let platform = if host == "mp.weixin.qq.com" {
            Some("web.wechat.article")
        } else if host == "zhihu.com" || host.ends_with(".zhihu.com") {
            Some("web.zhihu.content")
        } else {
            None
        };
        let mut routes = platform.into_iter().collect::<Vec<_>>();
        routes.extend(["web.generic.readability", "web.generic.browser"]);
        return routes;
    }
    let extension = Path::new(&input.locator)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown" | "txt" | "html" | "htm" => vec!["file.native"],
        "csv" => vec!["file.csv-package"],
        "docx" => vec![
            "office.modern.docx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        "xlsx" => vec![
            "office.modern.xlsx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        "pptx" => vec![
            "office.modern.pptx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        "doc" | "xls" | "ppt" => vec!["pack.office-legacy", "pack.office-oxide", "agent.office"],
        "pdf" => vec![
            "pdf.text",
            "pdf.layout",
            "ocr.cjk-accurate",
            "ocr.basic",
            "agent.pdf",
        ],
        "srt" | "vtt" | "lrc" | "ass" | "ssa" => vec!["media.subtitle"],
        "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" | "opus" | "wma" | "mp4" | "mov" | "mkv"
        | "webm" | "avi" | "m4v" | "wmv" | "gif" => {
            vec!["media.companion", "media.asr"]
        }
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" => {
            vec!["ocr.cjk-accurate", "ocr.basic"]
        }
        _ => Vec::new(),
    }
}

fn is_bilibili_import_input(input: &ImportInput) -> bool {
    if input.kind != crate::models::import_v2::ImportInputKind::Url {
        return false;
    }
    url::Url::parse(
        input
            .normalized_locator
            .as_deref()
            .unwrap_or(&input.locator),
    )
    .ok()
    .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
    .is_some_and(|host| {
        host == "bilibili.com" || host.ends_with(".bilibili.com") || host == "b23.tv"
    })
}

fn asr_unavailable() -> BackendError {
    BackendError::new(
        "IMPORT_WEB_SUBTITLE_UNAVAILABLE",
        "A verified subtitle is unavailable and local ASR could not complete.",
        true,
        true,
    )
}

fn cleanup_terminal_item_staging(project_root: &Path, staging: &Path) -> Result<(), BackendError> {
    let binding = match BoundProjectMutationRoot::bind(project_root, staging) {
        Ok(binding) => binding,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(BackendError::new(
                "IMPORT_MEDIA_CLEANUP_FAILED",
                error.to_string(),
                true,
                false,
            ))
        }
    };
    binding.remove_directory_tree(staging).map_err(|_| {
        BackendError::new(
            "IMPORT_MEDIA_CLEANUP_FAILED",
            "Temporary import media could not be fully removed.",
            true,
            false,
        )
    })
}

fn apply_companion_transcript_fallback(
    context: &ProjectContext,
    files: &FileStore,
    staging: &Path,
    mut result: EngineResult,
) -> Result<EngineResult, BackendError> {
    let relative = "transcripts/companion-fallback.md";
    let transcript =
        std::fs::read_to_string(staging.join(relative)).map_err(|_| asr_unavailable())?;
    if transcript.trim().is_empty() {
        return Err(asr_unavailable());
    }
    let base_path = staging.join(&result.markdown_path);
    let mut base = std::fs::read_to_string(&base_path).map_err(|_| asr_unavailable())?;
    base.push_str("\n\n## Companion transcript\n\n");
    base.push_str(transcript.trim());
    base.push('\n');
    files
        .write_project_bytes_absolute(context, &base_path, base.as_bytes())
        .map_err(|_| asr_unavailable())?;
    if !result.asset_paths.iter().any(|path| path == relative) {
        result.asset_paths.push(relative.into());
    }
    result.text_coverage = Some(1.0);
    result.continuation = None;
    result
        .warnings
        .push("IMPORT_COMPANION_TRANSCRIPT_SELECTED_AFTER_EMBEDDED_PROBE".into());
    Ok(result)
}

fn is_allowed_local_asr_output_workspace(staging: &Path, workspace: &Path) -> bool {
    let Some(name) = workspace.file_name().map(|value| value.to_string_lossy()) else {
        return false;
    };
    let runtime_temp = staging.join("runtime-temp");
    (workspace.parent() == Some(staging) && name.starts_with(".sensevoice-output-"))
        || (workspace.parent() == Some(runtime_temp.as_path()) && name.starts_with("asr-output-"))
}

fn ocr_unavailable() -> BackendError {
    BackendError::new(
        "IMPORT_WEB_OCR_UNAVAILABLE",
        "Verified local OCR could not complete for the imported image.",
        true,
        true,
    )
}

fn ocr_no_text() -> BackendError {
    BackendError::new(
        "IMPORT_OCR_NO_TEXT",
        "Local OCR completed but did not find readable text.",
        false,
        true,
    )
}

fn ocr_low_confidence() -> BackendError {
    BackendError::new(
        "IMPORT_OCR_LOW_CONFIDENCE",
        "Local OCR completed, but the recognized text was below the minimum confidence threshold.",
        true,
        true,
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OcrConfidenceMetadata {
    confidence: f64,
    blocks: Vec<OcrConfidenceBlock>,
}

#[derive(serde::Deserialize)]
struct OcrConfidenceBlock {
    confidence: f64,
    #[serde(default)]
    coordinates: Option<OcrConfidenceCoordinates>,
}

#[derive(serde::Deserialize)]
struct OcrConfidenceCoordinates {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn validate_ocr_confidence_metadata(
    bytes: &[u8],
    source_image_number: usize,
) -> Result<Vec<String>, BackendError> {
    const MINIMUM_MEAN_CONFIDENCE: f64 = 0.75;
    const MINIMUM_READABLE_BLOCK_CONFIDENCE: f64 = 0.50;

    let metadata: OcrConfidenceMetadata =
        serde_json::from_slice(bytes).map_err(|_| ocr_low_confidence())?;
    if metadata.blocks.is_empty()
        || !metadata.confidence.is_finite()
        || !(0.0..=1.0).contains(&metadata.confidence)
        || metadata.confidence < MINIMUM_MEAN_CONFIDENCE
        || !metadata.blocks.iter().any(|block| {
            block.confidence.is_finite()
                && block.confidence >= MINIMUM_READABLE_BLOCK_CONFIDENCE
                && block.confidence <= 1.0
        })
    {
        return Err(ocr_low_confidence());
    }
    let warnings = metadata
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| {
            !block.confidence.is_finite()
                || block.confidence < MINIMUM_MEAN_CONFIDENCE
                || block.confidence > 1.0
        })
        .map(|(index, block)| {
            let location = block
                .coordinates
                .as_ref()
                .map(|coordinates| {
                    format!(
                        ":x{}:y{}:w{}:h{}",
                        coordinates.x, coordinates.y, coordinates.width, coordinates.height
                    )
                })
                .unwrap_or_default();
            format!(
                "IMPORT_OCR_LOW_CONFIDENCE_BLOCK:image-{source_image_number}:block-{}{}",
                index + 1,
                location
            )
        })
        .collect();
    Ok(warnings)
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompletedAsrShard {
    schema_version: u32,
    complete: bool,
    engine_id: String,
    engine_version: String,
    transcript_sha256: String,
    metadata_sha256: Option<String>,
    warnings: Vec<String>,
    continuation: Option<EngineContinuation>,
    authorization_required: bool,
}

struct CachedAsrShard {
    transcript: String,
    metadata: Option<Vec<u8>>,
    warnings: Vec<String>,
    continuation: Option<EngineContinuation>,
}

fn asr_shard_key(
    input: &Path,
    descriptor: &crate::services::import_v2::engine::EngineDescriptor,
) -> Result<String, BackendError> {
    let bytes = std::fs::read(input).map_err(|_| asr_unavailable())?;
    let mut hasher = Sha256::new();
    hasher.update(b"asr-shard-v1\0");
    hasher.update(descriptor.engine_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(descriptor.engine_version.as_bytes());
    hasher.update(b"\0");
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn load_completed_asr_shard(
    root: &Path,
    key: &str,
    descriptor: &crate::services::import_v2::engine::EngineDescriptor,
    staging: &Path,
    local_asr_authorized: bool,
) -> Result<Option<CachedAsrShard>, BackendError> {
    let marker_bytes = match std::fs::read(root.join(format!("{key}.complete.json"))) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(asr_unavailable()),
    };
    let marker: CompletedAsrShard = match serde_json::from_slice(&marker_bytes) {
        Ok(marker) => marker,
        Err(_) => return Ok(None),
    };
    if marker.schema_version != 1
        || !marker.complete
        || marker.engine_id != descriptor.engine_id
        || marker.engine_version != descriptor.engine_version
        || (marker.authorization_required && !local_asr_authorized)
    {
        return Ok(None);
    }
    if let Some(EngineContinuation::LocalOcr {
        temporary_input_paths,
    }) = &marker.continuation
    {
        let canonical_staging = staging.canonicalize().map_err(|_| asr_unavailable())?;
        for relative in temporary_input_paths {
            let relative_path = Path::new(relative);
            if relative_path.is_absolute()
                || relative_path.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::ParentDir
                            | std::path::Component::RootDir
                            | std::path::Component::Prefix(_)
                    )
                })
            {
                return Ok(None);
            }
            let candidate = staging.join(relative_path);
            let metadata = match std::fs::symlink_metadata(&candidate) {
                Ok(metadata) => metadata,
                Err(_) => return Ok(None),
            };
            let canonical = match candidate.canonicalize() {
                Ok(canonical) => canonical,
                Err(_) => return Ok(None),
            };
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || !canonical.starts_with(&canonical_staging)
            {
                return Ok(None);
            }
        }
    }
    let transcript_bytes = match std::fs::read(root.join(format!("{key}.md"))) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    if format!("{:x}", Sha256::digest(&transcript_bytes)) != marker.transcript_sha256 {
        return Ok(None);
    }
    let metadata = if let Some(expected) = marker.metadata_sha256 {
        let bytes = match std::fs::read(root.join(format!("{key}.metadata.json"))) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };
        if format!("{:x}", Sha256::digest(&bytes)) != expected {
            return Ok(None);
        }
        Some(bytes)
    } else {
        None
    };
    let transcript = String::from_utf8(transcript_bytes).map_err(|_| asr_unavailable())?;
    Ok(Some(CachedAsrShard {
        transcript,
        metadata,
        warnings: marker.warnings,
        continuation: marker.continuation,
    }))
}

#[allow(clippy::too_many_arguments)]
fn store_completed_asr_shard(
    project_root: &Path,
    root: &Path,
    key: &str,
    descriptor: &crate::services::import_v2::engine::EngineDescriptor,
    transcript: &str,
    metadata: Option<&[u8]>,
    warnings: &[String],
    continuation: Option<&EngineContinuation>,
    authorization_required: bool,
) -> Result<(), BackendError> {
    let transcript_path = root.join(format!("{key}.md"));
    let (binding, _) = BoundProjectMutationRoot::ensure_and_bind(project_root, &transcript_path)
        .map_err(|_| asr_unavailable())?;
    binding
        .write_atomic_replace(&transcript_path, transcript.as_bytes())
        .map_err(|_| asr_unavailable())?;
    if let Some(metadata) = metadata {
        binding
            .write_atomic_replace(&root.join(format!("{key}.metadata.json")), metadata)
            .map_err(|_| asr_unavailable())?;
    }
    let marker = CompletedAsrShard {
        schema_version: 1,
        complete: true,
        engine_id: descriptor.engine_id.clone(),
        engine_version: descriptor.engine_version.clone(),
        transcript_sha256: format!("{:x}", Sha256::digest(transcript.as_bytes())),
        metadata_sha256: metadata.map(|bytes| format!("{:x}", Sha256::digest(bytes))),
        warnings: warnings.to_vec(),
        continuation: continuation.cloned(),
        authorization_required,
    };
    let marker = serde_json::to_vec(&marker).map_err(|_| asr_unavailable())?;
    binding
        .write_atomic_replace(&root.join(format!("{key}.complete.json")), &marker)
        .map_err(|_| asr_unavailable())
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompletedOcrShard {
    schema_version: u32,
    complete: bool,
    engine_id: String,
    engine_version: String,
    markdown_sha256: String,
    metadata_sha256: Option<String>,
}

fn ocr_shard_key(
    input: &Path,
    descriptor: &crate::services::import_v2::engine::EngineDescriptor,
) -> Result<String, BackendError> {
    let bytes = std::fs::read(input).map_err(|_| ocr_unavailable())?;
    let mut hasher = Sha256::new();
    hasher.update(b"ocr-shard-v1\0");
    hasher.update(descriptor.engine_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(descriptor.engine_version.as_bytes());
    hasher.update(b"\0");
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn load_completed_ocr_shard(
    root: &Path,
    key: &str,
    descriptor: &crate::services::import_v2::engine::EngineDescriptor,
) -> Result<Option<(String, Option<Vec<u8>>)>, BackendError> {
    let marker_bytes = match std::fs::read(root.join(format!("{key}.complete.json"))) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ocr_unavailable()),
    };
    let marker: CompletedOcrShard = match serde_json::from_slice(&marker_bytes) {
        Ok(marker) => marker,
        Err(_) => return Ok(None),
    };
    if marker.schema_version != 1
        || !marker.complete
        || marker.engine_id != descriptor.engine_id
        || marker.engine_version != descriptor.engine_version
    {
        return Ok(None);
    }
    let markdown_bytes = match std::fs::read(root.join(format!("{key}.md"))) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    if format!("{:x}", Sha256::digest(&markdown_bytes)) != marker.markdown_sha256 {
        return Ok(None);
    }
    let metadata = if let Some(expected) = marker.metadata_sha256 {
        let bytes = match std::fs::read(root.join(format!("{key}.metadata.json"))) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };
        if format!("{:x}", Sha256::digest(&bytes)) != expected {
            return Ok(None);
        }
        Some(bytes)
    } else {
        None
    };
    let markdown = String::from_utf8(markdown_bytes).map_err(|_| ocr_unavailable())?;
    Ok(Some((markdown, metadata)))
}

fn store_completed_ocr_shard(
    project_root: &Path,
    root: &Path,
    key: &str,
    descriptor: &crate::services::import_v2::engine::EngineDescriptor,
    markdown: &str,
    metadata: Option<&[u8]>,
) -> Result<(), BackendError> {
    let markdown_path = root.join(format!("{key}.md"));
    let (binding, _) = BoundProjectMutationRoot::ensure_and_bind(project_root, &markdown_path)
        .map_err(|_| ocr_unavailable())?;
    binding
        .write_atomic_replace(&markdown_path, markdown.as_bytes())
        .map_err(|_| ocr_unavailable())?;
    if let Some(metadata) = metadata {
        binding
            .write_atomic_replace(&root.join(format!("{key}.metadata.json")), metadata)
            .map_err(|_| ocr_unavailable())?;
    }
    let marker = CompletedOcrShard {
        schema_version: 1,
        complete: true,
        engine_id: descriptor.engine_id.clone(),
        engine_version: descriptor.engine_version.clone(),
        markdown_sha256: format!("{:x}", Sha256::digest(markdown.as_bytes())),
        metadata_sha256: metadata.map(|bytes| format!("{:x}", Sha256::digest(bytes))),
    };
    let marker = serde_json::to_vec(&marker).map_err(|_| ocr_unavailable())?;
    binding
        .write_atomic_replace(&root.join(format!("{key}.complete.json")), &marker)
        .map_err(|_| ocr_unavailable())
}

fn is_capability_route(route: &str) -> bool {
    route.starts_with("pack.")
        || route.starts_with("pdf.")
        || route.starts_with("ocr.")
        || route.starts_with("media.")
}

fn is_non_fallback_error(error: &BackendError) -> bool {
    error.code == crate::errors::IMPORT_V2_CANCELLED
        || error.code == "IMPORT_PDF_ENCRYPTED_UNSUPPORTED"
        || error.code == "IMPORT_PDF_ACTIVE_CONTENT_REJECTED"
        || error.code.contains("PASSWORD")
        || error.code.contains("LOGIN")
        || error.code.contains("CAPTCHA")
        || error.code == "IMPORT_WEB_CONTENT_REMOVED"
        || error.code == "IMPORT_LOCAL_SUBTITLE_AMBIGUOUS"
}
fn is_web_user_wait(error: &BackendError) -> bool {
    matches!(
        error.code.as_str(),
        "IMPORT_WEB_LOGIN_REQUIRED"
            | "IMPORT_WEB_CHALLENGE_DETECTED"
            | "IMPORT_WEB_CAPTCHA_REQUIRED"
    )
}

fn classify_route_failure(
    error: &BackendError,
) -> crate::services::import_v2::file_router::RouteFailure {
    use crate::services::import_v2::file_router::RouteFailure;
    if error.code.contains("CAPABILITY") || error.code.contains("UNAVAILABLE") {
        RouteFailure::CapabilityUnavailable
    } else if error.code.contains("CORRUPT") || error.code.contains("PARSE") {
        RouteFailure::CorruptInput
    } else if error.code.contains("RESOURCE") || error.code.contains("LIMIT") {
        RouteFailure::ResourceLimit
    } else if error.code.contains("UNSUPPORTED") {
        RouteFailure::UnsupportedFeature {
            feature: error.code.clone(),
        }
    } else {
        RouteFailure::EngineFailure {
            code: error.code.clone(),
        }
    }
}

fn candidate_meets_floor(
    input: &ImportInput,
    result: &crate::services::import_v2::engine::EngineResult,
    floor: QualityFloor,
) -> bool {
    let requirements = floor.requirements();
    if result
        .text_coverage
        .is_some_and(|coverage| coverage < requirements.minimum_text_coverage as f64)
    {
        return false;
    }
    if floor != QualityFloor::ModernOffice {
        return true;
    }
    let extension = Path::new(&input.locator)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "xls" | "xlsx" => {
            result.sheet_count_exact == Some(1.0)
                && result
                    .non_empty_cell_coverage
                    .is_some_and(|value| value >= 0.95)
                && result.formula_value_pairs == Some(1.0)
        }
        "ppt" | "pptx" => {
            result.slide_count_exact == Some(1.0)
                && result
                    .meaningful_image_coverage
                    .is_some_and(|value| value >= 0.95)
        }
        _ => true,
    }
}

fn materialize_capability_input(
    context: &ProjectContext,
    staging_root: &str,
    input: &ImportInput,
) -> Result<ImportInput, BackendError> {
    let identity = input.source_identity.as_ref().ok_or_else(|| {
        BackendError::new(
            "IMPORT_FILE_SOURCE_CHANGED",
            "The selected source must be scanned again before capability execution.",
            true,
            true,
        )
    })?;
    let source = Path::new(&input.locator);
    let bytes = crate::services::import_v2::native_file_engine::safe_read_source(source, identity)?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bin");
    let authorized_relative = format!("{staging_root}/authorized");
    let authorized_root = context.resolve_project_path(&authorized_relative)?;
    reset_authorized_directory(&context.root, &authorized_root)?;
    let relative = format!("{authorized_relative}/source.{extension}");
    let destination = context.resolve_project_path(&relative)?;
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| {
            BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
        })?;
    std::io::Write::write_all(&mut output, &bytes).map_err(|error| {
        BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
    })?;
    output.sync_all().map_err(|error| {
        BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
    })?;
    let mut authorized = input.clone();
    authorized.locator = destination.to_string_lossy().into_owned();
    authorized.normalized_locator = Some(relative.replace('\\', "/"));
    authorized.source_identity = None;
    Ok(authorized)
}

fn reset_authorized_directory(
    project_root: &Path,
    authorized_root: &Path,
) -> Result<(), BackendError> {
    let parent = authorized_root.parent().ok_or_else(|| {
        BackendError::new(
            "IMPORT_FILE_STAGE_FAILED",
            "The staging path is invalid.",
            false,
            false,
        )
    })?;
    let relative_parent = parent.strip_prefix(project_root).map_err(|_| {
        BackendError::new(
            "IMPORT_FILE_STAGE_FAILED",
            "The staging path escaped the project.",
            false,
            false,
        )
    })?;
    let mut current = project_root.to_path_buf();
    for component in relative_parent.components() {
        current.push(component.as_os_str());
        if let Ok(metadata) = std::fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() || staging_reparse(&metadata) {
                return Err(BackendError::new(
                    "IMPORT_FILE_STAGE_FAILED",
                    "The staging path contains a link or reparse point.",
                    false,
                    true,
                ));
            }
        }
    }
    if let Ok(metadata) = std::fs::symlink_metadata(authorized_root) {
        if metadata.file_type().is_symlink() || staging_reparse(&metadata) || !metadata.is_dir() {
            return Err(BackendError::new(
                "IMPORT_FILE_STAGE_FAILED",
                "The authorized staging directory is not a regular directory.",
                false,
                true,
            ));
        }
        BoundProjectMutationRoot::bind(project_root, authorized_root)
            .and_then(|binding| binding.remove_directory_tree(authorized_root))
            .map_err(|error| {
                BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
            })?;
    }
    let (_binding, created) = BoundProjectMutationRoot::ensure_and_bind(
        project_root,
        &authorized_root.join(".wiki-authorized-directory-binding-probe"),
    )
    .map_err(|error| {
        BackendError::new("IMPORT_FILE_STAGE_FAILED", error.to_string(), true, false)
    })?;
    if !created
        .iter()
        .any(|directory| directory.path() == authorized_root)
    {
        return Err(BackendError::new(
            "IMPORT_FILE_STAGE_FAILED",
            "The authorized staging directory changed while it was being reset.",
            true,
            true,
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn staging_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn staging_reparse(_: &std::fs::Metadata) -> bool {
    false
}

fn route_resolution_input(route: &str, input: &ImportInput) -> ImportInput {
    let original_extension = Path::new(&input.locator)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = match route {
        "pdf.text" | "pdf.layout" => Some("pdf"),
        "office.modern.docx" => Some("docx"),
        "office.modern.xlsx" => Some("xlsx"),
        "office.modern.pptx" => Some("pptx"),
        "file.csv-package" => Some("csv"),
        "ocr.cjk-accurate" | "ocr.basic" => Some("png"),
        "pack.markitdown" => match original_extension.as_str() {
            "doc" => Some("docx"),
            "xls" => Some("xlsx"),
            "ppt" => Some("pptx"),
            _ => None,
        },
        _ => None,
    };
    let Some(extension) = extension else {
        return input.clone();
    };
    let mut routed = input.clone();
    routed.locator = format!("converted.{extension}");
    routed.display_name = routed.locator.clone();
    routed
}

fn persist_derived(
    store: &SessionStore,
    context: &ProjectContext,
    files: &FileStore,
    mut session: ImportSession,
) -> Result<(), BackendError> {
    session.status = derive_session_status(&session.items);
    session.updated_at = chrono::Utc::now().to_rfc3339();
    store.save(context, files, &session)
}
fn find_item_mut<'a>(
    session: &'a mut ImportSession,
    item_id: &str,
) -> Result<&'a mut ImportItem, BackendError> {
    session
        .items
        .iter_mut()
        .find(|item| item.item_id == item_id)
        .ok_or_else(item_not_found)
}
fn item_not_found() -> BackendError {
    BackendError::new(
        IMPORT_V2_ITEM_NOT_FOUND,
        "Import session item was not found.",
        true,
        false,
    )
}
fn work_item_stale_error() -> BackendError {
    BackendError::new(
        IMPORT_V2_WORK_ITEM_STALE,
        "The import item changed after this worker snapshot was claimed.",
        true,
        false,
    )
}
fn task_error(message: &str) -> BackendError {
    BackendError::new(IMPORT_V2_STATE_INVALID, message, true, false)
}

fn ensure_agent_candidate_item_is_mutable(
    session: &ImportSession,
    item: &ImportItem,
) -> Result<(), BackendError> {
    if matches!(
        session.status,
        ImportSessionStatus::Completed | ImportSessionStatus::Cancelled
    ) || !matches!(
        item.status,
        ImportItemStatus::Failed | ImportItemStatus::PreviewReady | ImportItemStatus::NeedsMerge
    ) {
        return Err(task_error(
            "Agent candidate action is stale for the current Import item state.",
        ));
    }
    Ok(())
}
fn engine_progress_on_task_scale(progress: &EngineProgress) -> u64 {
    let normalized = progress
        .total
        .filter(|total| *total > 0)
        .map(|total| progress.current.min(total).saturating_mul(10_000) / total)
        .unwrap_or(0);
    let (start, end) = if progress.label == "media.downloading" {
        (5_u64, 20_u64)
    } else if progress.label == "images.downloading" {
        (5_u64, 60_u64)
    } else if progress.label.starts_with("ocr.") {
        (60_u64, 90_u64)
    } else if progress.label.starts_with("asr.") {
        (20_u64, 90_u64)
    } else {
        (5_u64, 90_u64)
    };
    start + normalized.saturating_mul(end - start) / 10_000
}
fn update_continuation_progress(
    tasks: &TaskService,
    task_id: &str,
    max_task_progress: &Cell<u64>,
    progress: EngineProgress,
) -> Result<(), BackendError> {
    let mapped = engine_progress_on_task_scale(&progress);
    if mapped < max_task_progress.get() {
        return Ok(());
    }
    max_task_progress.set(mapped);
    task_call(tasks.update_progress(task_id, mapped, Some(100), Some(progress.label))).map(|_| ())
}
fn ocr_source_image_number(path: &Path, fallback_index: usize) -> usize {
    path.file_stem()
        .and_then(|value| value.to_str())
        .and_then(|value| {
            value
                .strip_prefix("image-")
                .or_else(|| value.strip_prefix("page-"))
        })
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback_index + 1)
}
fn import_operation_task_state_root(context: &ProjectContext) -> Result<PathBuf, BackendError> {
    let relative = context.layout.task_state_root.as_deref().ok_or_else(|| {
        task_error("The project does not provide a writable task state root for import.")
    })?;
    context.resolve_project_path(relative)
}
fn task_call<T>(result: Result<T, String>) -> Result<T, BackendError> {
    result.map_err(|_| task_error("Import task state could not be updated."))
}

fn is_batch_operation_task(tasks: &TaskService, task_id: &str) -> bool {
    tasks
        .get_task(task_id)
        .is_some_and(|task| is_import_batch_operation_task(&task))
}

fn is_batch_claimable(item: &ImportItem) -> bool {
    matches!(
        item.status,
        ImportItemStatus::Queued
            | ImportItemStatus::Failed
            | ImportItemStatus::WaitingCapability
            | ImportItemStatus::WaitingLogin
            | ImportItemStatus::WaitingAuthorization
            | ImportItemStatus::Cancelled
            | ImportItemStatus::Skipped
            | ImportItemStatus::Paused
            | ImportItemStatus::PreviewReady
    )
}

fn item_staging_relative_path(
    context: &ProjectContext,
    session_id: &str,
    item_id: &str,
) -> Result<String, BackendError> {
    Ok(format!(
        "{}/items/{item_id}/staging",
        session_relative_root(context, session_id)?
    ))
}

fn session_relative_root(
    context: &ProjectContext,
    session_id: &str,
) -> Result<String, BackendError> {
    let root = context.layout.import_state_root.as_deref().ok_or_else(|| {
        BackendError::new(
            IMPORT_V2_STATE_INVALID,
            "Import state is unavailable for this project layout.",
            true,
            false,
        )
    })?;
    Ok(format!("{root}/{session_id}"))
}

fn web_result_marks_restricted_content(staging: &Path, metadata_path: Option<&str>) -> bool {
    let Some(metadata_path) = metadata_path else {
        return false;
    };
    std::fs::read(staging.join(metadata_path))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| {
            value
                .get("restrictedContent")
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(false)
}

fn cancelled_error() -> BackendError {
    BackendError::new(IMPORT_V2_CANCELLED, "Import was cancelled.", true, false)
}

fn requires_explicit_video_frame_ocr(error_code: &str) -> bool {
    error_code == "IMPORT_VIDEO_FRAME_OCR_REQUIRED"
}
fn issue_safe_error(error: &BackendError) -> BackendError {
    BackendError::new(
        error.code.clone(),
        "Import engine failed.",
        error.recoverable,
        error.user_action_required,
    )
}

fn transition_item(item: &mut ImportItem, next: ImportItemStatus) -> Result<(), BackendError> {
    if !item.status.can_transition_to(&next) {
        return Err(BackendError::new(
            IMPORT_V2_STATE_INVALID,
            format!(
                "Invalid import item transition: {:?} -> {:?}",
                item.status, next
            ),
            false,
            true,
        ));
    }
    item.status = next;
    Ok(())
}
fn issue_from_engine_error(error: &BackendError, stage: ImportStage) -> ImportIssue {
    if error.code.starts_with("IMPORT_WEB_")
        || error.code.starts_with("IMPORT_ASR_")
        || error.code.starts_with("IMPORT_V2_URL_")
        || error.code.starts_with("IMPORT_V2_REDIRECT_")
        || error.code.starts_with("IMPORT_V2_RESPONSE_")
        || error.code == "IMPORT_V2_CONNECTOR_RATE_LIMITED"
    {
        return ImportIssue::for_web_code(&error.code, stage);
    }
    let code = stable_file_error_code(&error.code);
    let mut issue = ImportIssue::for_file_code(code, stage);
    if code == "IMPORT_FILE_SUBTITLE_AMBIGUOUS" {
        issue.subtitle_candidates = error
            .details
            .as_ref()
            .and_then(|details| details.get("subtitleCandidates"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect();
    }
    issue
}

fn issue_from_engine_error_for_input(
    error: &BackendError,
    stage: ImportStage,
    input_kind: &ImportInputKind,
) -> ImportIssue {
    if *input_kind == ImportInputKind::Url {
        let code = if error.code == crate::errors::IMPORT_V2_ENGINE_OUTPUT_INVALID {
            "IMPORT_WEB_STRUCTURE_CHANGED"
        } else {
            &error.code
        };
        let mut issue = ImportIssue::for_web_code(code, stage);
        if let Some(message) = safe_web_error_message(&error.code) {
            issue.message = message.into();
        }
        return issue;
    }
    issue_from_engine_error(error, stage)
}

fn safe_web_error_message(code: &str) -> Option<&'static str> {
    match code {
        "IMPORT_V2_URL_REJECTED" => Some("URL was rejected by the import safety policy."),
        "IMPORT_V2_REDIRECT_REJECTED" => {
            Some("A redirect left the permitted URL or host boundary.")
        }
        "IMPORT_V2_PRIVATE_TARGET_BLOCKED" => Some(
            "The target resolved to a private or reserved network address and needs explicit authorization.",
        ),
        "IMPORT_V2_DNS_FAILED" => Some("DNS resolution failed."),
        "IMPORT_V2_TLS_OR_FETCH_FAILED" => Some("TLS or HTTP connection failed."),
        "IMPORT_V2_FETCH_FAILED" => Some("The web request could not be started."),
        "IMPORT_V2_RESPONSE_FAILED" => {
            Some("The remote service returned an unsuccessful status.")
        }
        "IMPORT_V2_RESPONSE_TOO_LARGE" => {
            Some("The response exceeded the configured byte limit.")
        }
        "IMPORT_V2_CONTENT_REJECTED" => {
            Some("The response content type is not allowed for this import route.")
        }
        "IMPORT_V2_CONNECTOR_RATE_LIMITED" => {
            Some("The remote service is temporarily unavailable or rate limited.")
        }
        _ => None,
    }
}

fn stable_file_error_code(code: &str) -> &'static str {
    match code {
        crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE
        | crate::errors::IMPORT_V2_CAPABILITY_UNAVAILABLE => "IMPORT_FILE_CAPABILITY_MISSING",
        crate::errors::IMPORT_V2_CANCELLED => "IMPORT_FILE_CANCELLED",
        crate::errors::IMPORT_V2_QUALITY_FAILED => "IMPORT_FILE_QUALITY_FAILED",
        crate::errors::IMPORT_V2_ENGINE_OUTPUT_INVALID => "IMPORT_FILE_PARSE_FAILED",
        "IMPORT_LOCAL_SUBTITLE_AMBIGUOUS" => "IMPORT_FILE_SUBTITLE_AMBIGUOUS",
        _ if code.contains("PASSWORD") => "IMPORT_FILE_PASSWORD_REQUIRED",
        _ if code.contains("CORRUPT") => "IMPORT_FILE_CORRUPT",
        _ if code.contains("RESOURCE") || code.contains("LIMIT") => "IMPORT_FILE_RESOURCE_LIMIT",
        _ if code.contains("CONVERSION") => "IMPORT_FILE_CONVERSION_FAILED",
        _ => "IMPORT_FILE_PARSE_FAILED",
    }
}

fn remove_clipboard_session_input(context: &ProjectContext, session_id: &str, input: &ImportInput) {
    if input.kind != ImportInputKind::ClipboardText {
        return;
    }
    let Ok(expected_root) = session_relative_root(context, session_id) else {
        return;
    };
    let expected_prefix = format!("{expected_root}/inputs/");
    if !input
        .locator
        .replace('\\', "/")
        .starts_with(&expected_prefix)
    {
        return;
    }
    if let Ok(path) = context.resolve_project_path(&input.locator) {
        let _ = remove_project_file(&context.root, &path);
    }
}

pub(super) fn derive_session_status(items: &[ImportItem]) -> ImportSessionStatus {
    use ImportItemStatus::*;
    let has =
        |statuses: &[ImportItemStatus]| items.iter().any(|item| statuses.contains(&item.status));
    if has(&[Inspecting, Extracting, Validating, Committing]) {
        ImportSessionStatus::Processing
    } else if has(&[Completed]) && has(&[Failed, Cancelled]) {
        ImportSessionStatus::PartiallyCommitted
    } else if !items.is_empty()
        && items
            .iter()
            .all(|item| matches!(item.status, Completed | Skipped | Cancelled))
        && has(&[Completed, Skipped])
    {
        ImportSessionStatus::Completed
    } else if !items.is_empty() && items.iter().all(|item| item.status == Cancelled) {
        ImportSessionStatus::Cancelled
    } else if has(&[
        PreviewReady,
        NeedsMerge,
        WaitingCapability,
        WaitingLogin,
        WaitingAuthorization,
        Failed,
    ]) {
        ImportSessionStatus::WaitingForConfirmation
    } else {
        ImportSessionStatus::Draft
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    };

    use crate::errors::{
        BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_PANICKED, IMPORT_V2_ENGINE_UNAVAILABLE,
        IMPORT_V2_STATE_INVALID,
    };
    use crate::models::import_v2::{
        CommitImportSessionRequest, CommitItemDecision, ImportInput, ImportItem,
        ImportItemResolution, ImportItemStatus, ImportResolutionKind, ImportResourceMode,
        ImportSession, ImportSessionStatus, ImportStage,
    };
    use crate::models::paths::ProjectContext;
    use crate::models::task::{BackendTask, TaskOperation, TaskStatus, TaskType};
    use crate::services::import_v2::engine::{
        EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
    };
    use crate::services::import_v2::source_registry::{SourceIndex, SourcePointer};
    use crate::services::import_v2::test_support::{test_context, test_file_input};
    use crate::services::{FileStore, GitService};
    use crate::tasks::task_model::CancellationToken;
    use crate::tasks::TaskService;

    use super::*;

    fn url_input(url: &str) -> ImportInput {
        ImportInput {
            kind: crate::models::import_v2::ImportInputKind::Url,
            display_name: url.into(),
            locator: url.into(),
            normalized_locator: Some(url.into()),
            source_identity: None,
            media_save_mode: crate::models::import_v2::MediaSaveMode::ExtractOnly,
        }
    }

    #[test]
    fn url_network_failures_use_canonical_public_messages() {
        let error = BackendError::new(
            "IMPORT_V2_URL_REJECTED",
            "sensitive://secret-bearing-internal-detail",
            false,
            true,
        );

        let issue = issue_from_engine_error_for_input(
            &error,
            ImportStage::Extract,
            &crate::models::import_v2::ImportInputKind::Url,
        );

        assert_eq!(issue.code, error.code);
        assert_eq!(
            issue.message,
            "URL was rejected by the import safety policy."
        );
        assert!(!issue.message.contains("sensitive"));

        let rate_limited = issue_from_engine_error_for_input(
            &BackendError::new(
                "IMPORT_V2_CONNECTOR_RATE_LIMITED",
                "do not persist this producer detail",
                true,
                false,
            ),
            ImportStage::Extract,
            &crate::models::import_v2::ImportInputKind::Url,
        );
        assert_eq!(
            rate_limited.message,
            "The remote service is temporarily unavailable or rate limited."
        );
    }

    #[test]
    fn non_import_tasks_cannot_impersonate_legacy_or_typed_import_operations() {
        let tasks = TaskService::default();
        let mut malformed = tasks.create_task(TaskType::Export, None, "export".into(), true);
        malformed.batch_id = Some("import-v2-operation:session-1".into());
        assert!(!is_import_batch_operation_task(&malformed));

        malformed.operation = Some(TaskOperation::ImportBatch {
            session_id: "session-1".into(),
            item_count: 1,
            source_label: None,
        });
        assert!(!is_import_batch_operation_task(&malformed));
    }

    #[test]
    fn a_panicked_mutation_does_not_disable_later_imports() {
        let service = ImportV2Service::default();
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = service.mutation_lock.lock().unwrap();
            panic!("simulated mutation panic");
        }));
        assert!(poisoned.is_err());
        assert!(service.mutation_lock.is_poisoned());

        let (context, root) = test_context("mutation-lock-panic-recovery");
        let session = service
            .create_session(
                &context,
                &FileStore::default(),
                ImportResourceMode::Balanced,
            )
            .unwrap();

        assert!(!session.session_id.is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_second_operation_cannot_steal_an_active_item_claim() {
        let service = ImportV2Service::default();
        let tasks = TaskService::default();
        let files = FileStore::default();
        let (context, root) = test_context("active-operation-claim");
        let session = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = service
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![test_file_input("one.pdf")],
            )
            .unwrap();
        let item_id = session.items[0].item_id.clone();
        let first = service
            .create_batch_operation_task(
                &context,
                &files,
                &tasks,
                &session.session_id,
                std::slice::from_ref(&item_id),
            )
            .unwrap();
        tasks
            .transition_status(&first.id, TaskStatus::Running)
            .unwrap();
        service
            .prepare_batch_operation(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &first.id,
                std::slice::from_ref(&item_id),
                || false,
            )
            .unwrap();

        let second = service
            .create_batch_operation_task(
                &context,
                &files,
                &tasks,
                &session.session_id,
                std::slice::from_ref(&item_id),
            )
            .unwrap();
        tasks
            .transition_status(&second.id, TaskStatus::Running)
            .unwrap();
        let error = service
            .prepare_batch_operation(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &second.id,
                std::slice::from_ref(&item_id),
                || false,
            )
            .unwrap_err();

        assert!(error.message.contains("another active operation"));
        let reopened = service
            .load_session(&context, &files, &session.session_id)
            .unwrap();
        assert_eq!(
            reopened.items[0].task_id.as_deref(),
            Some(first.id.as_str())
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_can_claim_one_item_from_a_drained_mixed_operation() {
        let service = ImportV2Service::default();
        let tasks = TaskService::default();
        let files = FileStore::default();
        let (context, root) = test_context("mixed-operation-recovery");
        let session = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = service
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![test_file_input("login.pdf"), test_file_input("ready.pdf")],
            )
            .unwrap();
        let item_ids = session
            .items
            .iter()
            .map(|item| item.item_id.clone())
            .collect::<Vec<_>>();
        let first = service
            .create_batch_operation_task(&context, &files, &tasks, &session.session_id, &item_ids)
            .unwrap();
        tasks
            .transition_status(&first.id, TaskStatus::Running)
            .unwrap();
        service
            .prepare_batch_operation(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &first.id,
                &item_ids,
                || false,
            )
            .unwrap();
        let mut drained = service
            .load_session(&context, &files, &session.session_id)
            .unwrap();
        drained.items[0].status = ImportItemStatus::WaitingLogin;
        drained.items[1].status = ImportItemStatus::PreviewReady;
        service.sessions.save(&context, &files, &drained).unwrap();
        tasks
            .finish_running_operation(
                &first.id,
                TaskResult {
                    summary: "waiting".into(),
                    affected_paths: Vec::new(),
                    reference: None,
                    pending_action: None,
                },
                TaskStatus::WaitingForConfirmation,
                None,
            )
            .unwrap();

        let resumed_id = item_ids[0].clone();
        let second = service
            .create_batch_operation_task(
                &context,
                &files,
                &tasks,
                &session.session_id,
                std::slice::from_ref(&resumed_id),
            )
            .unwrap();
        tasks
            .transition_status(&second.id, TaskStatus::Running)
            .unwrap();
        let fully_replaced = service
            .prepare_batch_operation(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &second.id,
                std::slice::from_ref(&resumed_id),
                || false,
            )
            .unwrap();

        assert!(fully_replaced.replaced_task_ids.is_empty());
        assert_eq!(fully_replaced.snapshots.len(), 1);
        let reopened = service
            .load_session(&context, &files, &session.session_id)
            .unwrap();
        assert_eq!(
            reopened.items[0].task_id.as_deref(),
            Some(second.id.as_str())
        );
        assert_eq!(
            reopened.items[1].task_id.as_deref(),
            Some(first.id.as_str())
        );
        assert_eq!(
            tasks.get_task(&first.id).unwrap().status,
            TaskStatus::WaitingForConfirmation
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_asr_outputs_accept_only_the_declared_sensevoice_and_whisper_workspaces() {
        let staging = PathBuf::from("D:/project/staging");
        assert!(is_allowed_local_asr_output_workspace(
            &staging,
            &staging.join(".sensevoice-output-fixture")
        ));
        assert!(is_allowed_local_asr_output_workspace(
            &staging,
            &staging.join("runtime-temp/asr-output-fixture")
        ));
        assert!(!is_allowed_local_asr_output_workspace(
            &staging,
            &staging.join("asr-output-fixture")
        ));
        assert!(!is_allowed_local_asr_output_workspace(
            &staging,
            &staging.join("runtime-temp/other-output-fixture")
        ));
        assert!(!is_allowed_local_asr_output_workspace(
            &staging,
            &staging.join("runtime-temp/nested/asr-output-fixture")
        ));
    }

    #[test]
    fn supported_media_platforms_have_explicit_routes_with_bilibili_api_first() {
        for (url, expected) in [
            (
                "https://www.xiaohongshu.com/explore/abc",
                vec![
                    "web.generic.browser",
                    "web.xiaohongshu.note",
                    "web.generic.readability",
                ],
            ),
            (
                "http://xhslink.cn/o/abc",
                vec![
                    "web.generic.browser",
                    "web.xiaohongshu.note",
                    "web.generic.readability",
                ],
            ),
            (
                "https://www.douyin.com/video/123",
                vec![
                    "web.generic.browser",
                    "web.douyin.video",
                    "web.generic.readability",
                ],
            ),
            (
                "https://www.bilibili.com/video/BV1xx411c7mD",
                vec![
                    "web.bilibili.video",
                    "web.bilibili.metadata",
                    "web.generic.browser",
                ],
            ),
            ("https://x.com/alice/status/123", vec!["web.x.post"]),
        ] {
            assert_eq!(explicit_routes(&url_input(url)), expected, "{url}");
        }
    }

    #[test]
    fn one_authenticated_platform_retries_the_waiting_group_without_marking_public_content_restricted(
    ) {
        let (context, root) = test_context("restricted-login-group");
        let files = FileStore::default();
        let service = ImportV2Service::default();
        let session = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = service
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![
                    url_input("https://www.bilibili.com/video/BV1first"),
                    url_input("https://www.bilibili.com/video/BV2second"),
                ],
            )
            .unwrap();
        let item_ids = session
            .items
            .iter()
            .map(|item| item.item_id.clone())
            .collect::<Vec<_>>();
        for item_id in &item_ids {
            service
                .mutate_item(&context, &files, &session.session_id, item_id, |item| {
                    item.status = ImportItemStatus::WaitingLogin;
                    Ok(())
                })
                .unwrap();
        }

        let marked = service
            .mark_authenticated_login_group(
                &context,
                &files,
                &session.session_id,
                &item_ids,
                Some("Reader — @reader"),
            )
            .unwrap();
        assert!(marked.items.iter().all(|item| {
            item.authenticated_retry
                && !item.restricted_content
                && item.authenticated_identity_summary.as_deref() == Some("Reader — @reader")
                && item.restricted_identity_summary.is_none()
        }));
        let persisted = serde_json::to_string(&marked).unwrap();
        assert!(!persisted.to_ascii_lowercase().contains("cookie"));
        assert!(!persisted.to_ascii_lowercase().contains("profilepath"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn agent_eligibility_uses_stable_issue_codes_and_excludes_access_failures() {
        let invalid = BackendError::new(
            crate::errors::IMPORT_V2_ENGINE_OUTPUT_INVALID,
            "invalid",
            true,
            false,
        );
        let stable = issue_from_engine_error(&invalid, ImportStage::Extract);
        assert_eq!(stable.code, "IMPORT_FILE_PARSE_FAILED");
        let web = issue_from_engine_error_for_input(
            &invalid,
            ImportStage::Extract,
            &ImportInputKind::Url,
        );
        assert_eq!(web.code, "IMPORT_WEB_STRUCTURE_CHANGED");
        assert_eq!(web.message, "Web import could not be completed.");
        assert!(is_agent_eligible_failure(&invalid.code, &stable));
        assert!(!is_agent_eligible_failure(
            "IMPORT_WEB_LOGIN_REQUIRED",
            &ImportIssue::for_web_code("IMPORT_WEB_LOGIN_REQUIRED", ImportStage::Extract),
        ));
        assert!(!is_agent_eligible_failure(
            "IMPORT_FILE_PASSWORD_REQUIRED",
            &ImportIssue::for_file_code("IMPORT_FILE_PASSWORD_REQUIRED", ImportStage::Extract),
        ));
        assert!(!is_agent_eligible_failure(
            "PATH_OUTSIDE_PROJECT",
            &ImportIssue::for_file_code("IMPORT_FILE_PARSE_FAILED", ImportStage::Extract),
        ));
    }

    #[test]
    fn authorized_staging_rejects_a_residual_non_directory() {
        let root = std::env::temp_dir().join(format!("authorized-stage-{}", uuid::Uuid::new_v4()));
        let authorized = root.join(".app/import-staging/session/item/authorized");
        std::fs::create_dir_all(authorized.parent().unwrap()).unwrap();
        std::fs::write(&authorized, b"must not be followed or overwritten").unwrap();
        let error = reset_authorized_directory(&root, &authorized).unwrap_err();
        assert_eq!(error.code, "IMPORT_FILE_STAGE_FAILED");
        assert_eq!(
            std::fs::read(&authorized).unwrap(),
            b"must not be followed or overwritten"
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn unfinished_session_includes_empty_discovery_session() {
        let (context, root) = test_context("unfinished-empty-discovery");
        let files = FileStore::default();
        let service = ImportV2Service::default();
        let session = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        service
            .set_discovery_task_id(
                &context,
                &files,
                &session.session_id,
                Some("scan-task-1".into()),
            )
            .unwrap();

        assert_eq!(
            service.find_unfinished_session(&context, &files).unwrap(),
            Some(session.session_id)
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn create_session_reuses_the_single_active_session() {
        let (context, root) = test_context("single-active-session");
        let files = FileStore::default();
        let service = ImportV2Service::default();
        let first = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let second = service
            .create_session(&context, &files, ImportResourceMode::Saver)
            .unwrap();

        assert_eq!(second.session_id, first.session_id);
        assert_eq!(second.resource_mode, ImportResourceMode::Balanced);
        let session_dirs = std::fs::read_dir(context.app_dir.join("import-sessions"))
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .count();
        assert_eq!(session_dirs, 1);
        std::fs::remove_dir_all(root).ok();
    }

    struct FixtureEngine {
        project_root: PathBuf,
        markdown: &'static [u8],
    }
    impl FixtureEngine {
        fn success(project_root: PathBuf) -> Self {
            Self {
                project_root,
                markdown: b"# Fixture\n\nBody",
            }
        }
    }
    impl ImportEngine for FixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "fixture".into(),
                engine_version: "1.0.0".into(),
                route: "pdf.text".into(),
            }
        }
        fn supports(&self, _input: &ImportInput) -> bool {
            true
        }
        fn execute(
            &self,
            request: &EngineRequest,
            _cancellation: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            let root = self.project_root.join(
                request
                    .staging_root
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join("source.bin"), b"source").unwrap();
            std::fs::write(root.join("candidate.md"), self.markdown).unwrap();
            Ok(EngineResult {
                source_snapshot_path: "source.bin".into(),
                markdown_path: "candidate.md".into(),
                asset_paths: Vec::new(),
                metadata_path: None,
                title: "Fixture".into(),
                text_coverage: Some(1.0),
                table_cell_accuracy: None,
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
                continuation: None,
                warnings: Vec::new(),
            })
        }
    }

    struct BlockingEngine {
        inner: FixtureEngine,
        entered: Arc<(Mutex<bool>, Condvar)>,
        release: Arc<(Mutex<bool>, Condvar)>,
    }
    impl ImportEngine for BlockingEngine {
        fn descriptor(&self) -> EngineDescriptor {
            self.inner.descriptor()
        }
        fn supports(&self, input: &ImportInput) -> bool {
            self.inner.supports(input)
        }
        fn execute(
            &self,
            request: &EngineRequest,
            cancellation: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            let (lock, signal) = &*self.entered;
            *lock.lock().unwrap() = true;
            signal.notify_all();
            let (lock, signal) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = signal.wait(released).unwrap();
            }
            self.inner.execute(request, cancellation)
        }
    }

    struct FailingEngine {
        root: PathBuf,
        sabotage_task_store: bool,
    }

    struct PanickingEngine;

    struct RouteFixtureEngine {
        root: PathBuf,
        id: &'static str,
        route: &'static str,
        coverage: f64,
    }

    struct PoorVideoAsrFixtureEngine;

    struct CountingAsrShardFixtureEngine {
        root: PathBuf,
        executions: Arc<AtomicUsize>,
    }

    impl ImportEngine for CountingAsrShardFixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "sensevoice-asr-shard.fixture".into(),
                engine_version: "1.0.0".into(),
                route: "media.asr".into(),
            }
        }

        fn supports(&self, _: &ImportInput) -> bool {
            true
        }

        fn execute(
            &self,
            request: &EngineRequest,
            _: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            let staging = self.root.join(
                request
                    .staging_root
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            let workspace = staging.join(".sensevoice-output-fixture");
            std::fs::create_dir_all(&workspace).unwrap();
            std::fs::write(workspace.join("source.bin"), b"asr-source").unwrap();
            std::fs::write(
                workspace.join("transcript.md"),
                "# Stable transcript\n\nASR shard reuse text.\n",
            )
            .unwrap();
            Ok(EngineResult {
                source_snapshot_path: ".sensevoice-output-fixture/source.bin".into(),
                markdown_path: ".sensevoice-output-fixture/transcript.md".into(),
                asset_paths: Vec::new(),
                metadata_path: None,
                title: "Stable transcript".into(),
                text_coverage: Some(1.0),
                table_cell_accuracy: None,
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
                continuation: None,
                warnings: vec!["fixture-asr-warning".into()],
            })
        }
    }

    impl ImportEngine for PoorVideoAsrFixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "poor-video-asr.fixture".into(),
                engine_version: "1".into(),
                route: "media.asr".into(),
            }
        }

        fn supports(&self, _: &ImportInput) -> bool {
            true
        }

        fn execute(
            &self,
            request: &EngineRequest,
            _: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            if request.asr_probe_only {
                return Err(BackendError::new(
                    "IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE",
                    "The fixture has no embedded subtitle.",
                    true,
                    true,
                ));
            }
            assert!(request.local_asr_authorized);
            Err(BackendError::new(
                "IMPORT_VIDEO_FRAME_OCR_REQUIRED",
                "The transcript is too poor; frame OCR requires separate authorization.",
                true,
                true,
            ))
        }
    }

    impl ImportEngine for RouteFixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: self.id.into(),
                engine_version: "1".into(),
                route: self.route.into(),
            }
        }
        fn supports(&self, _: &ImportInput) -> bool {
            true
        }
        fn execute(
            &self,
            request: &EngineRequest,
            _: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            let staging = self.root.join(
                request
                    .staging_root
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            std::fs::create_dir_all(&staging).unwrap();
            std::fs::write(staging.join("source.bin"), b"source").unwrap();
            std::fs::write(staging.join("candidate.md"), b"# Candidate\n\nBody").unwrap();
            Ok(EngineResult {
                source_snapshot_path: "source.bin".into(),
                markdown_path: "candidate.md".into(),
                asset_paths: vec![],
                metadata_path: None,
                title: self.id.into(),
                text_coverage: Some(self.coverage),
                table_cell_accuracy: None,
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
                continuation: None,
                warnings: vec![],
            })
        }
    }

    struct MultiOcrFixtureEngine {
        root: PathBuf,
        empty_second: bool,
        low_confidence_second: bool,
    }

    struct EmbeddedSubtitleProbeFixtureEngine {
        root: PathBuf,
        probe_seen: Arc<std::sync::atomic::AtomicBool>,
        embedded_available: bool,
    }

    impl ImportEngine for EmbeddedSubtitleProbeFixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "embedded-subtitle-probe.fixture".into(),
                engine_version: "1".into(),
                route: "media.asr".into(),
            }
        }

        fn supports(&self, _: &ImportInput) -> bool {
            true
        }

        fn execute(
            &self,
            request: &EngineRequest,
            _: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            if !request.asr_probe_only || request.local_asr_authorized {
                return Err(BackendError::new(
                    "IMPORT_TEST_UNEXPECTED_ASR",
                    "The fixture must only perform a pre-authorization embedded subtitle probe.",
                    false,
                    false,
                ));
            }
            self.probe_seen
                .store(true, std::sync::atomic::Ordering::SeqCst);
            if !self.embedded_available {
                return Err(BackendError::new(
                    "IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE",
                    "The fixture has no embedded subtitle track.",
                    true,
                    true,
                ));
            }
            let staging = self.root.join(
                request
                    .staging_root
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            let output = staging.join(".sensevoice-output-fixture");
            std::fs::create_dir_all(&output).unwrap();
            std::fs::write(
                output.join("candidate.md"),
                "# Transcript\n\n## [00:00:00.000]\n\nembedded subtitle text\n",
            )
            .unwrap();
            std::fs::write(
                output.join("source.json"),
                br#"{"provenance":"local-embedded-subtitle"}"#,
            )
            .unwrap();
            Ok(EngineResult {
                source_snapshot_path: ".sensevoice-output-fixture/source.json".into(),
                markdown_path: ".sensevoice-output-fixture/candidate.md".into(),
                asset_paths: vec![],
                metadata_path: None,
                title: "Embedded transcript".into(),
                text_coverage: Some(1.0),
                table_cell_accuracy: None,
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
                continuation: None,
                warnings: vec![],
            })
        }
    }

    impl ImportEngine for MultiOcrFixtureEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "ocr.cjk-accurate.fixture".into(),
                engine_version: "1".into(),
                route: "ocr.cjk-accurate".into(),
            }
        }

        fn supports(&self, _: &ImportInput) -> bool {
            true
        }

        fn execute(
            &self,
            request: &EngineRequest,
            _: &CancellationToken,
        ) -> Result<EngineResult, BackendError> {
            let staging = self.root.join(
                request
                    .staging_root
                    .replace('/', std::path::MAIN_SEPARATOR_STR),
            );
            std::fs::create_dir_all(&staging).unwrap();
            let second = request.input.display_name.contains("002");
            let markdown = if second && self.empty_second {
                " \n"
            } else if second {
                "# OCR 2\n\nsecond recognized page"
            } else {
                "# OCR 1\n\nfirst recognized page"
            };
            let confidence = if second && self.low_confidence_second {
                0.20
            } else {
                0.98
            };
            std::fs::write(staging.join("source.json"), br#"{"kind":"ocr"}"#).unwrap();
            std::fs::write(staging.join("candidate.md"), markdown).unwrap();
            std::fs::write(
                staging.join("metadata.json"),
                serde_json::to_vec(&serde_json::json!({
                    "confidence": confidence,
                    "blocks": [{
                        "confidence": confidence,
                        "coordinates": { "x": 1, "y": 2, "width": 30, "height": 12 }
                    }]
                }))
                .unwrap(),
            )
            .unwrap();
            Ok(EngineResult {
                source_snapshot_path: "source.json".into(),
                markdown_path: "candidate.md".into(),
                asset_paths: vec![],
                metadata_path: Some("metadata.json".into()),
                title: request.input.display_name.clone(),
                text_coverage: Some(if markdown.trim().is_empty() { 0.0 } else { 1.0 }),
                table_cell_accuracy: None,
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
                continuation: None,
                warnings: vec![],
            })
        }
    }

    #[test]
    fn quality_rejected_route_falls_back_and_persists_attempt_order() {
        let fixture = OrchestratorFixture::new("route-fallback");
        fixture
            .service
            .register_engine(Arc::new(RouteFixtureEngine {
                root: fixture.root.clone(),
                id: "low",
                route: "pdf.text",
                coverage: 0.1,
            }))
            .unwrap();
        fixture
            .service
            .register_engine(Arc::new(RouteFixtureEngine {
                root: fixture.root.clone(),
                id: "good",
                route: "pdf.layout",
                coverage: 1.0,
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let result = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        assert_eq!(result.attempts.len(), 2, "attempts: {:?}", result.attempts);
        assert_eq!(result.attempts[0].engine_id, "low");
        assert_eq!(
            result.attempts[0].outcome,
            crate::models::import_v2::AttemptOutcome::Failed
        );
        assert_eq!(result.attempts[1].engine_id, "good");
        assert_eq!(
            result.attempts[1].outcome,
            crate::models::import_v2::AttemptOutcome::Succeeded
        );
        assert!(result.preview.is_some());
    }

    #[test]
    fn modern_workbook_floor_requires_exact_structure_metrics() {
        let input = ImportInput {
            kind: crate::models::import_v2::ImportInputKind::File,
            display_name: "book.xlsx".into(),
            locator: "book.xlsx".into(),
            normalized_locator: None,
            source_identity: None,
            media_save_mode: Default::default(),
        };
        let mut result = EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: vec![],
            metadata_path: None,
            title: "book".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: Some(1.0),
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: None,
            warnings: vec![],
        };
        assert!(!candidate_meets_floor(
            &input,
            &result,
            QualityFloor::ModernOffice
        ));
        result.sheet_count_exact = Some(1.0);
        result.non_empty_cell_coverage = Some(0.95);
        result.formula_value_pairs = Some(1.0);
        assert!(candidate_meets_floor(
            &input,
            &result,
            QualityFloor::ModernOffice
        ));
    }
    impl ImportEngine for FailingEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "failing".into(),
                engine_version: "1".into(),
                route: "pdf.text".into(),
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
            if self.sabotage_task_store {
                let tasks = self.root.join(".app/tasks");
                std::fs::remove_dir_all(&tasks).unwrap();
                std::fs::write(&tasks, b"blocked").unwrap();
            }
            Err(BackendError::new(
                "ENGINE_SECRET",
                "Bearer private-token C:/Users/Aletta/a.pdf",
                true,
                false,
            ))
        }
    }

    impl ImportEngine for PanickingEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "panicking".into(),
                engine_version: "1".into(),
                route: "pdf.text".into(),
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
            panic!("injected engine panic");
        }
    }

    struct DescriptorPanicsAfterRegistration {
        calls: AtomicUsize,
    }

    impl ImportEngine for DescriptorPanicsAfterRegistration {
        fn descriptor(&self) -> EngineDescriptor {
            if self.calls.fetch_add(1, Ordering::SeqCst) > 0 {
                panic!("injected descriptor panic");
            }
            EngineDescriptor {
                engine_id: "panicking-descriptor".into(),
                engine_version: "1".into(),
                route: "pdf.text".into(),
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
            unreachable!("metadata panic must stop the import before execution")
        }
    }

    struct OrchestratorFixture {
        root: PathBuf,
        context: ProjectContext,
        files: FileStore,
        tasks: TaskService,
        service: ImportV2Service,
    }
    impl OrchestratorFixture {
        fn new(suffix: &str) -> Self {
            let (context, root) = test_context(suffix);
            Self {
                root,
                context,
                files: FileStore::default(),
                tasks: TaskService::default(),
                service: ImportV2Service::default(),
            }
        }
        fn seed_one_item(&self) -> (ImportSession, ImportItem, BackendTask) {
            self.seed_one_item_named("a.pdf")
        }
        fn seed_one_item_named(
            &self,
            source_name: &str,
        ) -> (ImportSession, ImportItem, BackendTask) {
            let source_path = self.root.join("fixtures").join(source_name);
            std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
            let fixture_bytes: &[u8] = match Path::new(source_name)
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "pdf" => include_bytes!(
                    "../../../../tests/fixtures/import-v2/local/batch3/text-only.pdf"
                ),
                "png" => include_bytes!(
                    "../../../../tests/fixtures/import-v2/local/batch3/image-no-text.png"
                ),
                "doc" => b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1orchestrator fixture",
                "mp4" => b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00isomiso2",
                _ => b"orchestrator fixture",
            };
            std::fs::write(&source_path, fixture_bytes).unwrap();
            let source_path = source_path.canonicalize().unwrap();
            let mut input = test_file_input(source_name);
            input.locator = source_path.to_string_lossy().into_owned();
            input.normalized_locator = Some(
                source_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_lowercase(),
            );
            input.source_identity = Some(SourceIdentity {
                canonical_path: source_path.to_string_lossy().into_owned(),
                size_bytes: fixture_bytes.len() as u64,
                modified_nanos: None,
                file_id: None,
                sha256: format!("{:x}", Sha256::digest(fixture_bytes)),
                magic: format!(
                    "{:x}",
                    Sha256::digest(&fixture_bytes[..fixture_bytes.len().min(8192)])
                ),
            });
            let session = self
                .service
                .create_session(&self.context, &self.files, ImportResourceMode::Balanced)
                .unwrap();
            let session = self
                .service
                .add_inputs(&self.context, &self.files, &session.session_id, vec![input])
                .unwrap();
            let item = session.items[0].clone();
            let task = self
                .tasks
                .create_project_task(
                    TaskType::Import,
                    self.context.project_id.clone(),
                    self.root.clone(),
                    "Fixture import".into(),
                    true,
                )
                .unwrap();
            (session, item, task)
        }
        fn reopen(&self) -> ImportSession {
            let sessions = std::fs::read_dir(self.context.app_dir.join("import-sessions")).unwrap();
            let session_id = sessions
                .flatten()
                .find(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
                .unwrap()
                .file_name()
                .to_string_lossy()
                .into_owned();
            self.service
                .load_session(&self.context, &self.files, &session_id)
                .unwrap()
        }
    }
    impl Drop for OrchestratorFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn run_item_persists_preview_ready_after_fixture_engine_succeeds() {
        let fixture = OrchestratorFixture::new("success");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let result = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        assert_eq!(result.status, ImportItemStatus::PreviewReady);
        assert!(result.preview.is_some());
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::PreviewReady
        );
        assert_eq!(
            fixture.tasks.get_task(&task.id).unwrap().status,
            TaskStatus::WaitingForConfirmation
        );
    }

    #[test]
    #[cfg(feature = "performance-observers")]
    fn production_snapshot_worker_performs_no_full_session_loads() {
        let fixture = OrchestratorFixture::new("snapshot-worker-no-full-load");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let snapshot = fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), task.id.clone())],
            )
            .unwrap()
            .remove(0);
        let canonical = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &session.session_id)
            .unwrap();
        let reservations = fixture
            .service
            .target_reservations_for_session(&canonical)
            .unwrap();
        let worker_revision = Cell::new(snapshot.expected_item_revision);

        crate::services::import_v2::session_store::reset_full_session_load_observer();
        let result = fixture
            .service
            .run_work_item_snapshot_inner(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &task.id,
                snapshot,
                &worker_revision,
                &reservations,
                None,
                true,
            )
            .unwrap();

        assert_eq!(result.status, ImportItemStatus::PreviewReady);
        assert_eq!(
            crate::services::import_v2::session_store::observed_full_session_loads(),
            0
        );
    }

    #[test]
    fn target_reservations_are_shared_for_concurrent_operations_in_one_session() {
        let fixture = OrchestratorFixture::new("shared-target-reservations");
        let (session, _, _) = fixture.seed_one_item();

        let first = fixture
            .service
            .target_reservations_for_session(&session)
            .unwrap();
        let second = fixture
            .service
            .target_reservations_for_session(&session)
            .unwrap();

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn frozen_worker_snapshot_rejects_stale_revision_without_overwriting_user_state() {
        let fixture = OrchestratorFixture::new("stale-worker-snapshot");
        let (session, item, task) = fixture.seed_one_item();
        let snapshot = fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), task.id.clone())],
            )
            .unwrap()
            .into_iter()
            .next()
            .unwrap();

        let mut concurrent = fixture
            .service
            .sessions
            .load_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
            )
            .unwrap();
        concurrent.selected = false;
        fixture
            .service
            .sessions
            .write_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &concurrent,
            )
            .unwrap();
        let changed = fixture
            .service
            .sessions
            .load_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
            )
            .unwrap();
        assert!(
            changed.item_revision > snapshot.expected_item_revision,
            "changed={} snapshot={}",
            changed.item_revision,
            snapshot.expected_item_revision
        );

        let error = fixture
            .service
            .claim_item_for_run(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &task.id,
                snapshot,
                false,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_WORK_ITEM_STALE);
        let persisted = fixture
            .service
            .sessions
            .load_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
            )
            .unwrap();
        assert!(!persisted.selected);
        assert_eq!(persisted.item_revision, changed.item_revision);
    }

    #[test]
    fn claimed_worker_compare_and_swap_rejects_a_concurrent_item_change() {
        let fixture = OrchestratorFixture::new("claimed-worker-cas");
        let (session, item, task) = fixture.seed_one_item();
        let snapshot = fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), task.id.clone())],
            )
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let claimed = fixture
            .service
            .claim_item_for_run(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &task.id,
                snapshot,
                false,
            )
            .unwrap();
        let mut concurrent = fixture
            .service
            .sessions
            .load_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
            )
            .unwrap();
        concurrent.selected = false;
        fixture
            .service
            .sessions
            .write_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &concurrent,
            )
            .unwrap();
        let changed = fixture
            .service
            .sessions
            .load_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
            )
            .unwrap();

        let error = fixture
            .service
            .mutate_claimed_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &task.id,
                claimed.expected_item_revision,
                |item| {
                    item.selected = true;
                    Ok(())
                },
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_WORK_ITEM_STALE);
        let persisted = fixture
            .service
            .sessions
            .load_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
            )
            .unwrap();
        assert!(!persisted.selected);
        assert_eq!(persisted.item_revision, changed.item_revision);
    }

    #[test]
    fn claimed_worker_cas_rejects_an_edit_between_read_and_transaction() {
        let fixture = OrchestratorFixture::new("claimed-worker-cas-interleaving");
        let (session, item, task) = fixture.seed_one_item();
        let snapshot = fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), task.id.clone())],
            )
            .unwrap()
            .remove(0);
        let claimed = fixture
            .service
            .claim_item_for_run(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &task.id,
                snapshot,
                false,
            )
            .unwrap();

        let error = fixture
            .service
            .mutate_claimed_item_with_before_write(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &task.id,
                claimed.expected_item_revision,
                |item| transition_item(item, ImportItemStatus::Extracting),
                |before| {
                    let mut concurrent = before.clone();
                    concurrent.selected = false;
                    fixture.service.sessions.write_item(
                        &fixture.context,
                        &fixture.files,
                        &session.session_id,
                        &concurrent,
                    )
                },
            )
            .unwrap_err();

        assert_eq!(error.code, IMPORT_V2_WORK_ITEM_STALE);
        let persisted = fixture
            .service
            .sessions
            .load_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
            )
            .unwrap();
        assert!(!persisted.selected);
        assert_eq!(persisted.status, ImportItemStatus::Inspecting);
    }

    #[test]
    fn unrelated_attempt_metadata_does_not_rebind_a_preview_target() {
        let fixture = OrchestratorFixture::new("stable-preview-target");
        let engine = FixtureEngine::success(fixture.root.clone());
        let descriptor = engine.descriptor();
        fixture.service.register_engine(Arc::new(engine)).unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let result = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        let target = result
            .preview
            .as_ref()
            .and_then(|preview| preview.resolution.as_ref())
            .and_then(|resolution| resolution.target_wiki_path.clone())
            .expect("preview target");
        let occupied = fixture.root.join(&target);
        std::fs::create_dir_all(occupied.parent().unwrap()).unwrap();
        std::fs::write(&occupied, "external content").unwrap();

        fixture
            .service
            .record_attempt(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &descriptor,
                "2026-07-27T00:00:00Z".into(),
                AttemptOutcome::Succeeded,
                None,
                vec!["metadata-only warning".into()],
            )
            .unwrap();

        let reopened = fixture.reopen();
        assert_eq!(
            reopened.items[0]
                .preview
                .as_ref()
                .and_then(|preview| preview.resolution.as_ref())
                .and_then(|resolution| resolution.target_wiki_path.as_deref()),
            Some(target.as_str())
        );
    }

    #[test]
    fn selected_agent_candidate_binds_a_new_source_target_and_rejects_external_collision() {
        let fixture = OrchestratorFixture::new("agent-candidate-target");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let original = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        let mut candidate = original.preview.expect("deterministic preview");
        candidate.resolution = None;
        let selected = fixture
            .service
            .select_agent_candidate(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &task.id,
                candidate,
                None,
            )
            .unwrap();
        let resolution = selected
            .preview
            .as_ref()
            .and_then(|preview| preview.resolution.as_ref())
            .expect("selected candidate resolution");
        assert_eq!(resolution.kind, ImportResolutionKind::NewSource);
        let target = resolution
            .target_wiki_path
            .as_ref()
            .expect("selected candidate target")
            .clone();

        let occupied = fixture.root.join(&target);
        std::fs::create_dir_all(occupied.parent().unwrap()).unwrap();
        std::fs::write(&occupied, "external content").unwrap();
        let git = GitService;
        git.initialize_repository(&fixture.context, "Initial fixture")
            .unwrap();
        let result = fixture
            .service
            .commit_items(
                &fixture.context,
                &fixture.files,
                &git,
                &CommitImportSessionRequest {
                    project_id: fixture.context.project_id.clone(),
                    project_root_path: fixture.root.to_string_lossy().into_owned(),
                    session_id: session.session_id,
                    batch_task_id: None,
                    acknowledge_restricted_content: false,
                    expected_selection_revision: None,
                    expected_confirmation_digest: None,
                    decisions: vec![CommitItemDecision {
                        item_id: item.item_id,
                        resolution: Some(ImportItemResolution::NewSource),
                    }],
                },
            )
            .unwrap();
        assert_eq!(result.committed_count, 0);
        assert_eq!(result.failed_count, 1);
        assert_eq!(
            result.items[0].error_code.as_deref(),
            Some(crate::errors::IMPORT_V2_COMMIT_CONFLICT)
        );
        assert_eq!(
            std::fs::read_to_string(occupied).unwrap(),
            "external content"
        );
    }

    #[test]
    fn selected_agent_candidate_fails_closed_when_current_wiki_changed_after_review() {
        let fixture = OrchestratorFixture::new("agent-candidate-stale-current");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let original = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        let mut candidate = original.preview.expect("deterministic preview");
        candidate.resolution = None;

        let baseline = "# Baseline\n";
        let current = "# Edited after Agent review\n";
        let baseline_hash = format!("{:x}", Sha256::digest(baseline.as_bytes()));
        let old_source = b"old source";
        let old_content_hash = format!("{:x}", Sha256::digest(old_source));
        let wiki_path = "wiki/sources/local/a.md";
        let baseline_path = ".app/source-artifacts/source-old/version-old/baseline.md";
        let raw_path = "raw/sources/source-old/version-old/original.bin";
        for path in [wiki_path, baseline_path, raw_path] {
            let absolute = fixture.root.join(path);
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        }
        std::fs::write(fixture.root.join(wiki_path), current).unwrap();
        std::fs::write(fixture.root.join(baseline_path), baseline).unwrap();
        std::fs::write(fixture.root.join(raw_path), old_source).unwrap();
        let pointer = SourcePointer {
            source_id: "source-old".into(),
            version_id: "version-old".into(),
        };
        let mut index = SourceIndex::default_v2();
        index.by_locator.insert(
            item.input.normalized_locator.clone().unwrap(),
            pointer.clone(),
        );
        index
            .by_content_hash
            .insert(old_content_hash.clone(), pointer);
        fixture
            .files
            .write_json_atomic(&fixture.context, ".app/source-index-v2.json", &index)
            .unwrap();
        fixture
            .files
            .write_json_atomic(
                &fixture.context,
                ".app/sources/source-old.json",
                &serde_json::json!({
                    "schemaVersion": 3,
                    "sourceId": "source-old",
                    "sourceKind": "local_document",
                    "currentVersionId": "version-old",
                    "wikiPath": wiki_path,
                    "origins": [item.input.normalized_locator.clone().unwrap()],
                    "title": "a",
                    "importedAt": chrono::Utc::now().to_rfc3339(),
                    "versions": [{
                        "versionId": "version-old",
                        "contentHash": old_content_hash.clone(),
                        "rawEvidence": [{
                            "path": raw_path,
                            "sha256": old_content_hash.clone(),
                            "sizeBytes": old_source.len(),
                            "kind": "source_snapshot"
                        }],
                        "assets": [],
                        "baselinePath": baseline_path,
                        "candidate": {
                            "markdownHash": baseline_hash.clone(),
                            "title": "a",
                            "sourceKind": "local_document"
                        },
                        "provenance": {
                            "locator": item.input.normalized_locator.clone().unwrap(),
                            "route": "native",
                            "engineId": "fixture",
                            "engineVersion": "1"
                        },
                        "quality": {
                            "level": "pass",
                            "metrics": [],
                            "warnings": []
                        },
                        "createdAt": chrono::Utc::now().to_rfc3339(),
                        "humanEditHash": baseline_hash.clone()
                    }]
                }),
            )
            .unwrap();

        let stale = fixture
            .service
            .select_agent_candidate(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &task.id,
                candidate.clone(),
                Some(&baseline_hash),
            )
            .unwrap_err();
        assert_eq!(stale.code, "IMPORT_AGENT_MERGE_STALE");

        let selected = fixture
            .service
            .select_agent_candidate(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &task.id,
                candidate,
                None,
            )
            .unwrap();
        assert_eq!(selected.status, ImportItemStatus::NeedsMerge);
        let resolution = selected
            .preview
            .as_ref()
            .and_then(|preview| preview.resolution.as_ref())
            .expect("stale candidate resolution");
        assert_eq!(resolution.kind, ImportResolutionKind::NeedsThreeWayMerge);
        assert!(resolution.default_resolution.is_none());
        assert_eq!(resolution.target_wiki_path.as_deref(), Some(wiki_path));
    }

    #[test]
    fn committed_item_rejects_stale_agent_candidate_select_and_discard() {
        let fixture = OrchestratorFixture::new("agent-candidate-commit-wins");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let preview = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap()
            .preview
            .expect("deterministic preview");
        let mut completed = fixture.reopen();
        completed.items[0].status = ImportItemStatus::Completed;
        completed.status = ImportSessionStatus::Completed;
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &completed)
            .unwrap();

        let select_error = fixture
            .service
            .select_agent_candidate(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &task.id,
                preview.clone(),
                None,
            )
            .unwrap_err();
        assert_eq!(select_error.code, IMPORT_V2_STATE_INVALID);
        let discard_error = fixture
            .service
            .discard_agent_candidate(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &task.id,
                Some(preview),
            )
            .unwrap_err();
        assert_eq!(discard_error.code, IMPORT_V2_STATE_INVALID);
        let reopened = fixture.reopen();
        assert_eq!(reopened.status, ImportSessionStatus::Completed);
        assert_eq!(reopened.items[0].status, ImportItemStatus::Completed);
    }

    #[test]
    fn agent_candidate_action_lock_serializes_select_finalize_and_discard() {
        let service = Arc::new(ImportV2Service::default());
        let (first_entered_tx, first_entered_rx) = std::sync::mpsc::channel();
        let (release_first_tx, release_first_rx) = std::sync::mpsc::channel();
        let first_service = Arc::clone(&service);
        let first = std::thread::spawn(move || {
            first_service
                .with_agent_candidate_action_lock(|| {
                    first_entered_tx.send(()).unwrap();
                    release_first_rx.recv().unwrap();
                    Ok(())
                })
                .unwrap();
        });
        first_entered_rx.recv().unwrap();

        let (second_attempted_tx, second_attempted_rx) = std::sync::mpsc::channel();
        let (second_entered_tx, second_entered_rx) = std::sync::mpsc::channel();
        let second_service = Arc::clone(&service);
        let second = std::thread::spawn(move || {
            second_attempted_tx.send(()).unwrap();
            second_service
                .with_agent_candidate_action_lock(|| {
                    second_entered_tx.send(()).unwrap();
                    Ok(())
                })
                .unwrap();
        });
        second_attempted_rx.recv().unwrap();
        assert!(second_entered_rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());

        release_first_tx.send(()).unwrap();
        second_entered_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        first.join().unwrap();
        second.join().unwrap();
    }

    #[test]
    fn selected_agent_exact_duplicate_is_finalized_with_alias_history_and_completion() {
        let fixture = OrchestratorFixture::new("agent-candidate-exact-duplicate");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let original = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        let mut candidate = original.preview.expect("deterministic preview");
        candidate.resolution = None;

        let baseline = "# Existing Source\n";
        let old_source = b"old source";
        let old_content_hash = format!("{:x}", Sha256::digest(old_source));
        let wiki_path = "wiki/sources/local/existing.md";
        let baseline_path = ".app/source-artifacts/source-old/version-old/baseline.md";
        let raw_path = "raw/sources/source-old/version-old/original.bin";
        for path in [wiki_path, baseline_path, raw_path] {
            let absolute = fixture.root.join(path);
            std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        }
        std::fs::write(fixture.root.join(wiki_path), baseline).unwrap();
        std::fs::write(fixture.root.join(baseline_path), baseline).unwrap();
        std::fs::write(fixture.root.join(raw_path), old_source).unwrap();
        let pointer = SourcePointer {
            source_id: "source-old".into(),
            version_id: "version-old".into(),
        };
        let locator = item.input.normalized_locator.clone().unwrap();
        let mut index = SourceIndex::default_v2();
        index.by_locator.insert(locator.clone(), pointer.clone());
        index
            .by_content_hash
            .insert(old_content_hash.clone(), pointer.clone());
        fixture
            .files
            .write_json_atomic(&fixture.context, ".app/source-index-v2.json", &index)
            .unwrap();
        fixture
            .files
            .write_json_atomic(
                &fixture.context,
                ".app/sources/source-old.json",
                &serde_json::json!({
                    "schemaVersion": 2,
                    "sourceId": "source-old",
                    "origins": ["https://legacy.example/source-old"],
                    "versions": [{
                        "versionId": "version-old",
                        "contentHash": old_content_hash.clone(),
                        "rawPath": raw_path,
                        "extractedPath": "",
                        "baselinePath": baseline_path,
                        "createdAt": chrono::Utc::now().to_rfc3339(),
                        "route": "native",
                        "engineId": "fixture",
                        "engineVersion": "1",
                        "quality": {
                            "level": "pass",
                            "metrics": [],
                            "warnings": []
                        }
                    }],
                    "currentVersionId": "version-old",
                    "wikiPath": wiki_path
                }),
            )
            .unwrap();

        let mut resolution_item = item.clone();
        resolution_item.preview = Some(candidate.clone());
        let candidate_hash = fixture
            .service
            .derive_resolution_context(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &resolution_item,
            )
            .unwrap()
            .binding
            .expect("temporary locator binding")
            .candidate_hash;
        index.by_locator.clear();
        index
            .by_content_hash
            .insert(candidate_hash, pointer.clone());
        fixture
            .files
            .write_json_atomic(&fixture.context, ".app/source-index-v2.json", &index)
            .unwrap();

        let selected = fixture
            .service
            .select_agent_candidate(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &task.id,
                candidate,
                None,
            )
            .unwrap();
        assert_eq!(
            selected
                .preview
                .as_ref()
                .and_then(|preview| preview.resolution.as_ref())
                .map(|resolution| resolution.kind),
            Some(ImportResolutionKind::ExactDuplicate)
        );
        let git = GitService;
        git.initialize_repository(&fixture.context, "Initial fixture")
            .unwrap();
        let batch = fixture
            .service
            .finalize_exact_duplicate(
                &fixture.context,
                &fixture.files,
                &git,
                &session.session_id,
                &item.item_id,
                false,
                || Ok(()),
            )
            .unwrap()
            .expect("exact duplicate batch");
        assert_eq!(batch.committed_count, 1);
        let completion = batch.completion.as_ref().expect("duplicate completion");
        assert_eq!(completion.duplicate_skips.len(), 1);
        assert_eq!(completion.duplicate_skips[0].source_id, "source-old");
        let history = batch.history_snapshot.as_ref().expect("history snapshot");
        assert_eq!(history.items[0].status, ImportItemStatus::Completed);
        assert!(fixture
            .root
            .join(format!(".app/import-history/{}.json", batch.batch_id))
            .is_file());
        let manifest: serde_json::Value = fixture
            .files
            .read_json(&fixture.context, ".app/sources/source-old.json")
            .unwrap();
        assert!(manifest["origins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|origin| origin.as_str() == Some(locator.as_str())));
    }

    #[test]
    fn run_item_records_engine_unavailable_without_losing_session() {
        let fixture = OrchestratorFixture::new("no-engine");
        let (session, item, task) = fixture.seed_one_item_named("a.doc");
        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_ENGINE_UNAVAILABLE);
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::WaitingCapability
        );
    }

    #[test]
    fn run_item_honors_a_pre_cancelled_task() {
        let fixture = OrchestratorFixture::new("cancelled");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        fixture.tasks.cancel_task(&task.id).unwrap();
        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_CANCELLED);
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::Cancelled
        );
        assert_eq!(
            fixture.tasks.get_task(&task.id).unwrap().status,
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn queued_item_cancellation_prevents_a_later_worker_from_executing() {
        let fixture = OrchestratorFixture::new("queued-cancelled");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        fixture.tasks.cancel_task(&task.id).unwrap();
        fixture
            .service
            .cancel_queued_item(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
            )
            .unwrap();

        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();

        assert_eq!(error.code, IMPORT_V2_CANCELLED);
        let reopened = fixture.reopen();
        assert_eq!(reopened.items[0].status, ImportItemStatus::Cancelled);
        assert!(reopened.items[0].attempts.is_empty());
        assert_eq!(
            fixture.tasks.get_task(&task.id).unwrap().status,
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn transition_helper_rejects_preview_to_completed() {
        let mut item = ImportItem::queued("item-1", test_file_input("a.pdf"));
        item.status = ImportItemStatus::PreviewReady;
        let error = transition_item(&mut item, ImportItemStatus::Completed).unwrap_err();
        assert_eq!(error.code, IMPORT_V2_STATE_INVALID);
    }

    #[test]
    fn ended_session_rejects_text_and_discovery_before_any_side_effect() {
        let (context, root) = test_context("ended-session-add-preflight");
        let files = FileStore::default();
        let service = ImportV2Service::default();
        let mut session = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        session.status = ImportSessionStatus::Completed;
        service.sessions.save(&context, &files, &session).unwrap();

        let text_error = service
            .add_text_input(
                &context,
                &files,
                &session.session_id,
                "ended.md",
                "# must not stage",
            )
            .unwrap_err();
        assert_eq!(text_error.code, IMPORT_V2_STATE_INVALID);
        assert!(!root
            .join(format!(
                ".app/import-sessions/{}/inputs",
                session.session_id
            ))
            .exists());

        let discovery_error = service
            .set_discovery_task_id(
                &context,
                &files,
                &session.session_id,
                Some("must-not-bind".into()),
            )
            .unwrap_err();
        assert_eq!(discovery_error.code, IMPORT_V2_STATE_INVALID);
        assert!(service
            .sessions
            .load(&context, &files, &session.session_id)
            .unwrap()
            .discovery_task_id
            .is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_actions_change_route_priority_without_losing_fallbacks() {
        let routes = vec![
            ("pdf.text", QualityFloor::DeterministicDocument),
            ("pdf.layout", QualityFloor::DeterministicDocument),
            ("ocr.cjk-accurate", QualityFloor::ComparisonFallback),
            ("agent.pdf", QualityFloor::AgentCandidate),
        ];

        let switched = reorder_routes(routes.clone(), Some(&ImportRecoveryAction::SwitchRoute));
        assert_eq!(
            switched.iter().map(|(route, _)| *route).collect::<Vec<_>>(),
            vec!["pdf.layout", "ocr.cjk-accurate", "agent.pdf", "pdf.text"]
        );

        let ocr = reorder_routes(routes, Some(&ImportRecoveryAction::EnableOcr));
        assert_eq!(
            ocr.iter().map(|(route, _)| *route).collect::<Vec<_>>(),
            vec!["ocr.cjk-accurate", "pdf.layout", "pdf.text", "agent.pdf"]
        );
    }

    #[test]
    fn media_ocr_authorization_is_explicit_and_scoped_to_one_session_item() {
        let mut session =
            ImportSession::new("session-1", "project-1", ImportResourceMode::Balanced);
        assert!(!session.has_media_authorization("item-1", ImportMediaAuthorizationKind::Ocr));
        session.media_authorizations.push(ImportMediaAuthorization {
            item_id: "item-1".into(),
            kind: ImportMediaAuthorizationKind::Ocr,
            authorized_at: "2026-07-26T00:00:00Z".into(),
            asr_profile: None,
            language: None,
        });
        assert!(session.has_media_authorization("item-1", ImportMediaAuthorizationKind::Ocr));
        assert!(!session.has_media_authorization("item-2", ImportMediaAuthorizationKind::Ocr));
    }

    #[test]
    fn poor_video_asr_stops_the_production_pipeline_for_explicit_frame_ocr_authorization() {
        assert!(requires_explicit_video_frame_ocr(
            "IMPORT_VIDEO_FRAME_OCR_REQUIRED"
        ));
        assert!(!requires_explicit_video_frame_ocr(
            "IMPORT_ASR_OUTPUT_INVALID"
        ));

        let fixture = OrchestratorFixture::new("poor-video-asr");
        fixture
            .service
            .register_engine(Arc::new(PoorVideoAsrFixtureEngine))
            .unwrap();
        fixture
            .service
            .register_engine(Arc::new(RouteFixtureEngine {
                root: fixture.root.clone(),
                id: "frame-ocr.fixture",
                route: "ocr.basic",
                coverage: 1.0,
            }))
            .unwrap();
        let (session, item, first_task) = fixture.seed_one_item_named("poor-asr.mp4");
        let waiting_asr = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &first_task.id,
            )
            .unwrap();
        assert_eq!(waiting_asr.status, ImportItemStatus::WaitingAuthorization);
        fixture
            .service
            .authorize_media_for_session(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                ImportMediaAuthorizationKind::Asr,
                Some(crate::models::import_v2::ImportAsrProfile::Balanced),
                None,
            )
            .unwrap();
        let second_task = fixture
            .tasks
            .create_project_task(
                TaskType::Import,
                fixture.context.project_id.clone(),
                fixture.root.clone(),
                "Authorized poor ASR".into(),
                true,
            )
            .unwrap();
        let waiting_ocr = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &second_task.id,
            )
            .unwrap();
        assert_eq!(waiting_ocr.status, ImportItemStatus::WaitingAuthorization);
        assert_eq!(
            waiting_ocr.issue.as_ref().map(|issue| issue.code.as_str()),
            Some("IMPORT_VIDEO_FRAME_OCR_REQUIRED")
        );
        let reopened = fixture.reopen();
        assert!(reopened.has_media_authorization(&item.item_id, ImportMediaAuthorizationKind::Asr));
        assert!(!reopened.has_media_authorization(&item.item_id, ImportMediaAuthorizationKind::Ocr));
    }
    #[test]
    fn engine_error_is_reduced_to_a_secret_free_issue() {
        let error = BackendError::new("ENGINE_FAILED", "Authorization: Bearer secret", true, false)
            .with_details(serde_json::json!({ "path": "C:/Users/Aletta/private.pdf" }));
        let issue = issue_from_engine_error(&error, ImportStage::Extract);
        assert_eq!(issue.message, "File import could not be completed.");
        let value = serde_json::to_string(&issue).unwrap();
        assert!(!value.contains("secret"));
        assert!(!value.contains("Aletta"));
    }

    #[test]
    fn session_status_priority_is_derived_from_items() {
        use ImportItemStatus::*;
        let cases = [
            (vec![Completed, Extracting], ImportSessionStatus::Processing),
            (
                vec![Completed, Failed],
                ImportSessionStatus::PartiallyCommitted,
            ),
            (vec![Completed, Skipped], ImportSessionStatus::Completed),
            (vec![Skipped, Skipped], ImportSessionStatus::Completed),
            (vec![Cancelled, Cancelled], ImportSessionStatus::Cancelled),
            (
                vec![PreviewReady, Queued],
                ImportSessionStatus::WaitingForConfirmation,
            ),
            (vec![Queued, Skipped], ImportSessionStatus::Draft),
        ];
        for (statuses, expected) in cases {
            let items: Vec<_> = statuses
                .into_iter()
                .enumerate()
                .map(|(i, status)| {
                    let mut item =
                        ImportItem::queued(&format!("item-{i}"), test_file_input("a.pdf"));
                    item.status = status;
                    item
                })
                .collect();
            assert_eq!(derive_session_status(&items), expected);
        }
    }

    #[test]
    fn set_item_selected_persists_the_choice() {
        let fixture = OrchestratorFixture::new("selection");
        let (session, item, _) = fixture.seed_one_item();
        let changed = fixture
            .service
            .set_item_selected(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                false,
            )
            .unwrap();
        assert!(!changed.selected);
        assert!(!fixture.reopen().items[0].selected);
    }

    #[test]
    fn prebind_item_task_id_survives_before_worker_claims_item() {
        let fixture = OrchestratorFixture::new("prebind-task");
        let (session, item, task) = fixture.seed_one_item();

        fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), task.id.clone())],
            )
            .unwrap();

        let reopened = fixture.reopen();
        assert_eq!(reopened.items[0].status, ImportItemStatus::Queued);
        assert_eq!(reopened.items[0].task_id.as_deref(), Some(task.id.as_str()));
    }

    #[test]
    fn recovery_releases_interrupted_prebound_queued_item() {
        let fixture = OrchestratorFixture::new("prebind-recovery");
        let (session, item, task) = fixture.seed_one_item();
        fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), task.id.clone())],
            )
            .unwrap();

        let recovered_tasks = TaskService::default();
        recovered_tasks.recover_tasks(&fixture.root).unwrap();
        assert_eq!(
            recovered_tasks.get_task(&task.id).unwrap().status,
            TaskStatus::Failed
        );

        let recovered = ImportV2Service::default()
            .recover_session(
                &fixture.context,
                &fixture.files,
                &recovered_tasks,
                &session.session_id,
            )
            .unwrap();
        assert_eq!(recovered.items[0].status, ImportItemStatus::Queued);
        assert!(recovered.items[0].task_id.is_none());
    }

    #[test]
    fn recovery_cancels_waiting_authorization_item_with_cancelled_task() {
        let fixture = OrchestratorFixture::new("waiting-authorization-cancel");
        let (session, _item, task) = fixture.seed_one_item();
        let mut persisted = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &session.session_id)
            .unwrap();
        persisted.items[0].status = ImportItemStatus::WaitingAuthorization;
        persisted.items[0].task_id = Some(task.id.clone());
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &persisted)
            .unwrap();
        fixture.tasks.cancel_task(&task.id).unwrap();

        let recovered = fixture
            .service
            .recover_session(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
            )
            .unwrap();
        assert_eq!(recovered.items[0].status, ImportItemStatus::Cancelled);
        assert!(recovered.items[0].task_id.is_none());
    }

    #[test]
    fn quality_failure_never_becomes_preview_ready() {
        let fixture = OrchestratorFixture::new("quality-fail");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine {
                project_root: fixture.root.clone(),
                markdown: b"",
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_QUALITY_FAILED);
        let reopened = fixture.reopen();
        assert_eq!(reopened.items[0].status, ImportItemStatus::Failed);
        assert!(reopened.items[0].preview.is_none());
        assert!(reopened.items[0].attempts.iter().any(|attempt| {
            attempt.error_code.as_deref() == Some(crate::errors::IMPORT_V2_QUALITY_FAILED)
        }));
        assert_eq!(
            fixture.tasks.get_task(&task.id).unwrap().status,
            TaskStatus::Failed
        );
    }

    #[test]
    fn restart_reconciles_in_flight_items_and_allows_retry() {
        for status in [
            ImportItemStatus::Inspecting,
            ImportItemStatus::Extracting,
            ImportItemStatus::Validating,
        ] {
            let fixture = OrchestratorFixture::new(&format!("recovery-{status:?}"));
            let (session, item, task) = fixture.seed_one_item();
            fixture
                .tasks
                .transition_status(&task.id, TaskStatus::Running)
                .unwrap();
            let mut persisted = fixture
                .service
                .load_session(&fixture.context, &fixture.files, &session.session_id)
                .unwrap();
            persisted.items[0].status = status;
            persisted.items[0].task_id = Some(task.id.clone());
            fixture
                .service
                .sessions
                .save(&fixture.context, &fixture.files, &persisted)
                .unwrap();
            let staging = fixture.root.join(format!(
                ".app/import-sessions/{}/items/{}/staging",
                session.session_id, item.item_id
            ));
            std::fs::create_dir_all(staging.join("media-download")).unwrap();
            std::fs::write(staging.join("media-download/payload.bin"), b"downloaded").unwrap();
            std::fs::write(staging.join("media-download/complete.json"), b"{}").unwrap();
            std::fs::create_dir_all(staging.join("asr-shards")).unwrap();
            std::fs::write(staging.join("asr-shards/0001.json"), b"{}").unwrap();
            std::fs::create_dir_all(staging.join(".asr-input-interrupted")).unwrap();
            std::fs::write(
                staging.join(".asr-input-interrupted/temporary.wav"),
                b"temporary",
            )
            .unwrap();
            let recovered_tasks = TaskService::default();
            recovered_tasks.recover_tasks(&fixture.root).unwrap();
            assert_eq!(
                recovered_tasks.get_task(&task.id).unwrap().status,
                TaskStatus::Failed
            );
            let restarted = ImportV2Service::default();
            let reconciled = restarted
                .recover_session(
                    &fixture.context,
                    &fixture.files,
                    &recovered_tasks,
                    &session.session_id,
                )
                .unwrap();
            assert_eq!(reconciled.items[0].status, ImportItemStatus::Paused);
            assert_eq!(
                reconciled.items[0].issue.as_ref().unwrap().code,
                "TASK_PAUSED"
            );
            assert!(staging.join("media-download/payload.bin").is_file());
            assert!(staging.join("media-download/complete.json").is_file());
            assert!(staging.join("asr-shards/0001.json").is_file());
            assert!(!staging.join(".asr-input-interrupted").exists());
            restarted
                .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
                .unwrap();
            let retry = recovered_tasks
                .create_project_task(
                    TaskType::Import,
                    fixture.context.project_id.clone(),
                    fixture.root.clone(),
                    "Retry".into(),
                    true,
                )
                .unwrap();
            assert_eq!(
                restarted
                    .run_item(
                        &fixture.context,
                        &fixture.files,
                        &recovered_tasks,
                        &session.session_id,
                        &item.item_id,
                        &retry.id
                    )
                    .unwrap()
                    .status,
                ImportItemStatus::PreviewReady
            );
        }
    }

    #[test]
    fn concurrent_run_cannot_overwrite_claim_or_start_second_task() {
        let (context, root) = test_context("concurrent-claim");
        let files = Arc::new(FileStore::default());
        let tasks = Arc::new(TaskService::default());
        let service = Arc::new(ImportV2Service::default());
        let source_path = root.join("concurrent.pdf");
        let source_bytes =
            include_bytes!("../../../../tests/fixtures/import-v2/local/batch3/text-only.pdf");
        std::fs::write(&source_path, source_bytes).unwrap();
        let source_path = source_path.canonicalize().unwrap();
        let mut input = test_file_input("a.pdf");
        input.locator = source_path.to_string_lossy().into_owned();
        input.normalized_locator = Some(
            source_path
                .to_string_lossy()
                .replace('\\', "/")
                .to_lowercase(),
        );
        input.source_identity = Some(SourceIdentity {
            canonical_path: source_path.to_string_lossy().into_owned(),
            size_bytes: source_bytes.len() as u64,
            modified_nanos: None,
            file_id: None,
            sha256: format!("{:x}", Sha256::digest(source_bytes)),
            magic: format!("{:x}", Sha256::digest(source_bytes)),
        });
        let session = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = service
            .add_inputs(&context, &files, &session.session_id, vec![input])
            .unwrap();
        let item_id = session.items[0].item_id.clone();
        let first = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                root.clone(),
                "First".into(),
                true,
            )
            .unwrap();
        let second = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                root.clone(),
                "Second".into(),
                true,
            )
            .unwrap();
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        service
            .register_engine(Arc::new(BlockingEngine {
                inner: FixtureEngine::success(root.clone()),
                entered: entered.clone(),
                release: release.clone(),
            }))
            .unwrap();
        let worker = {
            let service = service.clone();
            let tasks = tasks.clone();
            let files = files.clone();
            let context = context.clone();
            let session_id = session.session_id.clone();
            let item_id = item_id.clone();
            let task_id = first.id.clone();
            std::thread::spawn(move || {
                service.run_item(&context, &files, &tasks, &session_id, &item_id, &task_id)
            })
        };
        let (lock, signal) = &*entered;
        let mut seen = lock.lock().unwrap();
        while !*seen {
            seen = signal.wait(seen).unwrap();
        }
        drop(seen);
        let error = service
            .run_item(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &item_id,
                &second.id,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_STATE_INVALID);
        assert_eq!(
            tasks.get_task(&second.id).unwrap().status,
            TaskStatus::Queued
        );
        assert_eq!(
            service
                .load_session(&context, &files, &session.session_id)
                .unwrap()
                .items[0]
                .task_id
                .as_deref(),
            Some(first.id.as_str())
        );
        let (lock, signal) = &*release;
        *lock.lock().unwrap() = true;
        signal.notify_all();
        worker.join().unwrap().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_incompatible_tasks_before_binding() {
        let fixture = OrchestratorFixture::new("task-validation");
        let (session, item, _) = fixture.seed_one_item();
        let wrong_type = fixture
            .tasks
            .create_project_task(
                TaskType::Export,
                fixture.context.project_id.clone(),
                fixture.root.clone(),
                "Wrong".into(),
                true,
            )
            .unwrap();
        let wrong_project = fixture
            .tasks
            .create_project_task(
                TaskType::Import,
                "other-project".into(),
                fixture.root.clone(),
                "Wrong project".into(),
                true,
            )
            .unwrap();
        let running = fixture
            .tasks
            .create_project_task(
                TaskType::Import,
                fixture.context.project_id.clone(),
                fixture.root.clone(),
                "Running".into(),
                true,
            )
            .unwrap();
        fixture
            .tasks
            .transition_status(&running.id, TaskStatus::Running)
            .unwrap();
        for task_id in [&wrong_type.id, &wrong_project.id, &running.id] {
            assert_eq!(
                fixture
                    .service
                    .run_item(
                        &fixture.context,
                        &fixture.files,
                        &fixture.tasks,
                        &session.session_id,
                        &item.item_id,
                        task_id
                    )
                    .unwrap_err()
                    .code,
                IMPORT_V2_STATE_INVALID
            );
            assert!(fixture.reopen().items[0].task_id.is_none());
        }
    }

    #[test]
    fn inspecting_is_persisted_before_task_running() {
        let fixture = OrchestratorFixture::new("ordering");
        let (session, item, task) = fixture.seed_one_item();
        let task_store = fixture.root.join(".app/tasks");
        std::fs::remove_dir_all(&task_store).unwrap();
        std::fs::write(&task_store, b"blocked").unwrap();
        assert_eq!(
            fixture
                .service
                .run_item(
                    &fixture.context,
                    &fixture.files,
                    &fixture.tasks,
                    &session.session_id,
                    &item.item_id,
                    &task.id
                )
                .unwrap_err()
                .code,
            IMPORT_V2_STATE_INVALID
        );
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::Inspecting
        );
    }

    #[test]
    fn finish_failed_propagates_task_persistence_errors() {
        let fixture = OrchestratorFixture::new("failure-persistence");
        fixture
            .service
            .register_engine(Arc::new(FailingEngine {
                root: fixture.root.clone(),
                sabotage_task_store: true,
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_STATE_INVALID);
    }

    #[test]
    fn finish_failed_persists_redacted_error_log_and_terminal_task() {
        let fixture = OrchestratorFixture::new("failure-durable");
        fixture
            .service
            .register_engine(Arc::new(FailingEngine {
                root: fixture.root.clone(),
                sabotage_task_store: false,
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        assert_eq!(
            fixture
                .service
                .run_item(
                    &fixture.context,
                    &fixture.files,
                    &fixture.tasks,
                    &session.session_id,
                    &item.item_id,
                    &task.id
                )
                .unwrap_err()
                .code,
            "ENGINE_SECRET"
        );
        let restarted = TaskService::default();
        restarted.recover_tasks(&fixture.root).unwrap();
        let recovered = restarted.get_task(&task.id).unwrap();
        assert_eq!(recovered.status, TaskStatus::Failed);
        assert_eq!(recovered.error.unwrap().message, "Import engine failed.");
        assert_eq!(
            restarted.get_logs(&task.id).unwrap()[0].message,
            "Import engine failed."
        );
    }

    #[test]
    fn engine_panic_terminalizes_the_item_and_task() {
        let fixture = OrchestratorFixture::new("engine-panic");
        fixture
            .service
            .register_engine(Arc::new(PanickingEngine))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();

        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();

        assert_eq!(error.code, IMPORT_V2_ENGINE_PANICKED);
        let reopened = fixture.reopen();
        assert_eq!(reopened.items[0].status, ImportItemStatus::Failed);
        assert_eq!(reopened.items[0].attempts.len(), 1);
        let failed_task = fixture.tasks.get_task(&task.id).unwrap();
        assert_eq!(failed_task.status, TaskStatus::Failed);
        assert_eq!(failed_task.error.unwrap().code, IMPORT_V2_ENGINE_PANICKED);
    }

    #[test]
    fn engine_descriptor_panic_terminalizes_the_item_and_task() {
        let fixture = OrchestratorFixture::new("engine-descriptor-panic");
        fixture
            .service
            .register_engine(Arc::new(DescriptorPanicsAfterRegistration {
                calls: AtomicUsize::new(0),
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();

        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap_err();

        assert_eq!(error.code, IMPORT_V2_ENGINE_PANICKED);
        let reopened = fixture.reopen();
        assert_eq!(reopened.items[0].status, ImportItemStatus::Failed);
        assert!(reopened.items[0].attempts.is_empty());
        let failed_task = fixture.tasks.get_task(&task.id).unwrap();
        assert_eq!(failed_task.status, TaskStatus::Failed);
        assert_eq!(failed_task.error.unwrap().code, IMPORT_V2_ENGINE_PANICKED);
    }

    #[test]
    fn restart_reconciles_in_flight_item_with_cancelled_task() {
        let fixture = OrchestratorFixture::new("recover-cancelled");
        let (session, _, task) = fixture.seed_one_item();
        fixture
            .tasks
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        let mut persisted = fixture
            .service
            .load_session(&fixture.context, &fixture.files, &session.session_id)
            .unwrap();
        persisted.items[0].status = ImportItemStatus::Extracting;
        persisted.items[0].task_id = Some(task.id.clone());
        fixture
            .service
            .sessions
            .save(&fixture.context, &fixture.files, &persisted)
            .unwrap();
        fixture.tasks.cancel_task(&task.id).unwrap();
        let restarted_tasks = TaskService::default();
        restarted_tasks.recover_tasks(&fixture.root).unwrap();
        let recovered = ImportV2Service::default()
            .recover_session(
                &fixture.context,
                &fixture.files,
                &restarted_tasks,
                &session.session_id,
            )
            .unwrap();
        assert_eq!(recovered.items[0].status, ImportItemStatus::Cancelled);
        assert_eq!(recovered.status, ImportSessionStatus::Cancelled);
    }

    #[test]
    fn cancellation_after_claim_cannot_leave_item_processing() {
        let fixture = OrchestratorFixture::new("cancel-race");
        let (session, item, task) = fixture.seed_one_item();
        let snapshot = fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), task.id.clone())],
            )
            .unwrap()
            .remove(0);
        let claimed = fixture
            .service
            .claim_item_for_run(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &task.id,
                snapshot,
                false,
            )
            .unwrap();
        fixture.tasks.cancel_task(&task.id).unwrap();
        let error = fixture
            .service
            .start_claimed_task(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
                claimed.expected_item_revision,
            )
            .unwrap_err();
        assert_eq!(error.code, IMPORT_V2_CANCELLED);
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::Cancelled
        );
        assert_eq!(fixture.reopen().status, ImportSessionStatus::Cancelled);
    }

    #[test]
    fn cancelled_item_can_retry_with_a_new_task_id() {
        let fixture = OrchestratorFixture::new("cancel-retry");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, first_task) = fixture.seed_one_item();
        let snapshot = fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), first_task.id.clone())],
            )
            .unwrap()
            .remove(0);
        let claimed = fixture
            .service
            .claim_item_for_run(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &first_task.id,
                snapshot,
                false,
            )
            .unwrap();
        fixture.tasks.cancel_task(&first_task.id).unwrap();
        fixture
            .service
            .start_claimed_task(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &first_task.id,
                claimed.expected_item_revision,
            )
            .unwrap_err();
        assert_eq!(
            fixture.reopen().items[0].status,
            ImportItemStatus::Cancelled
        );
        assert!(fixture.reopen().items[0].task_id.is_none());

        let retry_task = fixture
            .tasks
            .create_project_task(
                TaskType::Import,
                fixture.context.project_id.clone(),
                fixture.root.clone(),
                "Retry import".into(),
                true,
            )
            .unwrap();
        let retried = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &retry_task.id,
            )
            .unwrap();
        assert_eq!(retried.status, ImportItemStatus::PreviewReady);
    }

    #[test]
    fn preview_task_result_serializes_typed_reference() {
        let fixture = OrchestratorFixture::new("typed-result");
        fixture
            .service
            .register_engine(Arc::new(FixtureEngine::success(fixture.root.clone())))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item();
        fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        let value = serde_json::to_value(fixture.tasks.get_task(&task.id).unwrap().result.unwrap())
            .unwrap();
        assert_eq!(value["reference"]["type"], "import_preview");
        assert_eq!(value["reference"]["sessionId"], session.session_id);
        assert_eq!(value["reference"]["itemId"], item.item_id);
        assert_eq!(value["affectedPaths"], serde_json::json!([]));
    }

    #[test]
    fn engine_progress_uses_monotonic_task_stage_ranges() {
        assert_eq!(
            engine_progress_on_task_scale(&EngineProgress {
                current: 50,
                total: Some(100),
                label: "media.downloading".into(),
            }),
            12
        );
        assert_eq!(
            engine_progress_on_task_scale(&EngineProgress {
                current: 50,
                total: Some(100),
                label: "asr.recognizing".into(),
            }),
            55
        );
        assert_eq!(
            engine_progress_on_task_scale(&EngineProgress {
                current: 50,
                total: Some(100),
                label: "images.downloading".into(),
            }),
            32
        );
        assert_eq!(
            engine_progress_on_task_scale(&EngineProgress {
                current: 50,
                total: Some(100),
                label: "ocr.recognizing".into(),
            }),
            75
        );
        assert_eq!(
            engine_progress_on_task_scale(&EngineProgress {
                current: 50,
                total: None,
                label: "media.downloading".into(),
            }),
            5
        );
    }

    #[test]
    fn ocr_sections_keep_the_original_image_number() {
        assert_eq!(
            ocr_source_image_number(Path::new(".ocr-input-a/image-007.png"), 1),
            7
        );
        assert_eq!(
            ocr_source_image_number(Path::new(".ocr-input-a/input.png"), 1),
            2
        );
    }

    #[test]
    fn completed_ocr_shards_are_reused_only_with_valid_atomic_markers() {
        let root = tempfile::tempdir().unwrap();
        let descriptor = EngineDescriptor {
            engine_id: "ocr.fixture".into(),
            engine_version: "1.0.0".into(),
            route: "ocr.basic".into(),
        };
        store_completed_ocr_shard(
            root.path(),
            root.path(),
            "fixture-key",
            &descriptor,
            "# OCR text\n",
            Some(br#"{"page":1}"#),
        )
        .unwrap();
        let reused = load_completed_ocr_shard(root.path(), "fixture-key", &descriptor).unwrap();
        assert_eq!(
            reused,
            Some(("# OCR text\n".into(), Some(br#"{"page":1}"#.to_vec())))
        );

        std::fs::write(root.path().join("fixture-key.md"), "# partial\n").unwrap();
        assert_eq!(
            load_completed_ocr_shard(root.path(), "fixture-key", &descriptor).unwrap(),
            None
        );
    }

    #[test]
    fn completed_asr_shards_are_reused_only_with_valid_atomic_markers() {
        let fixture = OrchestratorFixture::new("asr-shard-reuse");
        let executions = Arc::new(AtomicUsize::new(0));
        fixture
            .service
            .register_engine(Arc::new(CountingAsrShardFixtureEngine {
                root: fixture.root.clone(),
                executions: Arc::clone(&executions),
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item_named("reuse.mp4");
        let mut expected_item_revision = fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), task.id.clone())],
            )
            .unwrap()
            .remove(0)
            .expected_item_revision;
        fixture
            .tasks
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        let staging_root = format!(
            ".app/import-sessions/{}/items/{}/staging",
            session.session_id, item.item_id
        );
        let staging = fixture.root.join(&staging_root);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("source.bin"), b"web-source").unwrap();
        let media_relative = ".asr-input-cache/input.wav";
        let media_path = staging.join(media_relative);
        std::fs::create_dir_all(media_path.parent().unwrap()).unwrap();
        std::fs::write(&media_path, b"stable-media-bytes").unwrap();
        let descriptor = EngineDescriptor {
            engine_id: "sensevoice-asr-shard.fixture".into(),
            engine_version: "1.0.0".into(),
            route: "media.asr".into(),
        };
        let shard_key = asr_shard_key(&media_path, &descriptor).unwrap();
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            project_id: fixture.context.project_id.clone(),
            session_id: session.session_id.clone(),
            item_id: item.item_id.clone(),
            task_id: task.id.clone(),
            operation: EngineOperation::Extract,
            input: url_input("https://example.com/media"),
            project_root: fixture.root.to_string_lossy().into_owned(),
            staging_root: staging_root.clone(),
            chained_input: None,
            local_asr_authorized: true,
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: None,
            selected_subtitle: None,
            local_ocr_authorized: false,
            media_save_mode: crate::models::import_v2::MediaSaveMode::ExtractOnly,
        };
        let web_result = || EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: Vec::new(),
            metadata_path: None,
            title: "Base media".into(),
            text_coverage: Some(0.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: Some(EngineContinuation::LocalAsr {
                temporary_input_path: media_relative.into(),
                media_kind: "audio".into(),
            }),
            warnings: Vec::new(),
        };
        let token = fixture.tasks.get_cancellation_token(&task.id).unwrap();

        std::fs::write(staging.join("candidate.md"), "# Base media\n").unwrap();
        let first = fixture
            .service
            .execute_local_asr_continuation(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &staging_root,
                &request,
                web_result(),
                &token,
                &fixture.tasks,
                &task.id,
                &Cell::new(0),
                &mut expected_item_revision,
                &Cell::new(0),
            )
            .unwrap();
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert!(first
            .asset_paths
            .contains(&"transcripts/local-asr.md".into()));
        assert!(staging
            .join(format!("asr-shards/{shard_key}.complete.json"))
            .is_file());

        std::fs::create_dir_all(media_path.parent().unwrap()).unwrap();
        std::fs::write(&media_path, b"stable-media-bytes").unwrap();
        std::fs::write(staging.join("candidate.md"), "# Base media\n").unwrap();
        let reused = fixture
            .service
            .execute_local_asr_continuation(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &staging_root,
                &request,
                web_result(),
                &token,
                &fixture.tasks,
                &task.id,
                &Cell::new(0),
                &mut expected_item_revision,
                &Cell::new(0),
            )
            .unwrap();
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert!(reused
            .warnings
            .contains(&"local_asr:reused-complete-shard".into()));

        std::fs::write(
            staging.join(format!("asr-shards/{shard_key}.md")),
            "# corrupted transcript\n",
        )
        .unwrap();
        std::fs::create_dir_all(media_path.parent().unwrap()).unwrap();
        std::fs::write(&media_path, b"stable-media-bytes").unwrap();
        std::fs::write(staging.join("candidate.md"), "# Base media\n").unwrap();
        fixture
            .service
            .execute_local_asr_continuation(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                &staging_root,
                &request,
                web_result(),
                &token,
                &fixture.tasks,
                &task.id,
                &Cell::new(0),
                &mut expected_item_revision,
                &Cell::new(0),
            )
            .unwrap();
        assert_eq!(executions.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn local_media_uses_embedded_subtitles_before_companion_and_without_asr_authorization() {
        let fixture = OrchestratorFixture::new("embedded-before-companion");
        let probe_seen = Arc::new(std::sync::atomic::AtomicBool::new(false));
        fixture
            .service
            .register_engine(Arc::new(EmbeddedSubtitleProbeFixtureEngine {
                root: fixture.root.clone(),
                probe_seen: Arc::clone(&probe_seen),
                embedded_available: true,
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item_named("interview.mp4");
        std::fs::write(
            fixture.root.join("fixtures/interview.srt"),
            "1\n00:00:00,000 --> 00:00:02,000\ncompanion subtitle text\n",
        )
        .unwrap();
        let result = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        assert_eq!(result.status, ImportItemStatus::PreviewReady);
        assert!(probe_seen.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!session.has_media_authorization(&item.item_id, ImportMediaAuthorizationKind::Asr));
        let preview = result.preview.unwrap();
        let staging = fixture.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging",
            session.session_id, item.item_id
        ));
        let markdown =
            std::fs::read_to_string(staging.join(preview.markdown.relative_path)).unwrap();
        assert!(markdown.contains("embedded subtitle text"));
        assert!(!markdown.contains("companion subtitle text"));
    }

    #[test]
    fn local_media_uses_companion_before_asr_when_embedded_probe_is_empty() {
        let fixture = OrchestratorFixture::new("companion-before-asr");
        let probe_seen = Arc::new(std::sync::atomic::AtomicBool::new(false));
        fixture
            .service
            .register_engine(Arc::new(EmbeddedSubtitleProbeFixtureEngine {
                root: fixture.root.clone(),
                probe_seen: Arc::clone(&probe_seen),
                embedded_available: false,
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item_named("interview.mp4");
        std::fs::write(
            fixture.root.join("fixtures/interview.srt"),
            "1\n00:00:00,000 --> 00:00:02,000\ncompanion subtitle text\n",
        )
        .unwrap();
        let result = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        assert_eq!(result.status, ImportItemStatus::PreviewReady);
        assert!(probe_seen.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!session.has_media_authorization(&item.item_id, ImportMediaAuthorizationKind::Asr));
        let preview = result.preview.unwrap();
        let staging = fixture.root.join(format!(
            ".app/import-sessions/{}/items/{}/staging",
            session.session_id, item.item_id
        ));
        let markdown =
            std::fs::read_to_string(staging.join(preview.markdown.relative_path)).unwrap();
        assert!(markdown.contains("## Companion transcript"));
        assert!(markdown.contains("companion subtitle text"));
        assert!(!markdown.contains("## Local ASR Transcript"));
    }

    fn execute_two_image_ocr(
        fixture: &OrchestratorFixture,
        empty_second: bool,
        low_confidence_second: bool,
    ) -> Result<(EngineResult, PathBuf), BackendError> {
        fixture
            .service
            .register_engine(Arc::new(MultiOcrFixtureEngine {
                root: fixture.root.clone(),
                empty_second,
                low_confidence_second,
            }))
            .unwrap();
        let (session, item, task) = fixture.seed_one_item_named("two-images.png");
        let mut expected_item_revision = fixture
            .service
            .bind_item_task_ids(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &[(item.item_id.clone(), task.id.clone())],
            )
            .unwrap()
            .remove(0)
            .expected_item_revision;
        fixture
            .tasks
            .transition_status(&task.id, TaskStatus::Running)
            .unwrap();
        let staging_root = format!(
            ".app/import-sessions/{}/items/{}/staging",
            session.session_id, item.item_id
        );
        let staging = fixture.root.join(&staging_root);
        let workspace = staging.join(".ocr-input-shared");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("image-001.png"), b"first").unwrap();
        std::fs::write(workspace.join("image-002.png"), b"second").unwrap();
        std::fs::write(staging.join("source.bin"), b"source").unwrap();
        std::fs::write(staging.join("candidate.md"), "# Base\n").unwrap();
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            project_id: fixture.context.project_id.clone(),
            session_id: session.session_id.clone(),
            item_id: item.item_id.clone(),
            task_id: task.id.clone(),
            operation: EngineOperation::Extract,
            input: item.input.clone(),
            project_root: fixture.root.to_string_lossy().into_owned(),
            staging_root: staging_root.clone(),
            chained_input: None,
            local_asr_authorized: false,
            asr_probe_only: false,
            asr_profile: None,
            recognition_language: None,
            selected_subtitle: None,
            local_ocr_authorized: true,
            media_save_mode: crate::models::import_v2::MediaSaveMode::ExtractOnly,
        };
        let result = EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "candidate.md".into(),
            asset_paths: vec![],
            metadata_path: None,
            title: "Two images".into(),
            text_coverage: Some(0.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: None,
            continuation: Some(EngineContinuation::LocalOcr {
                temporary_input_paths: vec![
                    ".ocr-input-shared/image-001.png".into(),
                    ".ocr-input-shared/image-002.png".into(),
                ],
            }),
            warnings: vec![],
        };
        let token = fixture.tasks.get_cancellation_token(&task.id).unwrap();
        let result = fixture.service.execute_local_ocr_continuation(
            &fixture.context,
            &fixture.files,
            &session.session_id,
            &item.item_id,
            &staging_root,
            &request,
            result,
            &token,
            &fixture.tasks,
            &task.id,
            &Cell::new(0),
            &mut expected_item_revision,
            &Cell::new(0),
        )?;
        Ok((result, staging))
    }

    #[test]
    fn multi_image_ocr_keeps_shared_workspace_until_every_input_completes() {
        let fixture = OrchestratorFixture::new("ocr-shared-workspace");
        let (result, staging) = execute_two_image_ocr(&fixture, false, false).unwrap();
        let markdown = std::fs::read_to_string(staging.join("candidate.md")).unwrap();
        let first_boundary = markdown
            .find("## 图片文字 / OCR — 第 1 张")
            .expect("first image OCR boundary must be retained");
        let first_text = markdown
            .find("first recognized page")
            .expect("first image OCR text must be retained");
        let second_boundary = markdown
            .find("## 图片文字 / OCR — 第 2 张")
            .expect("second image OCR boundary must be retained");
        let second_text = markdown
            .find("second recognized page")
            .expect("second image OCR text must be retained");
        assert!(
            first_boundary < first_text
                && first_text < second_boundary
                && second_boundary < second_text,
            "image OCR sections must preserve source order and per-image boundaries"
        );
        assert!(result
            .asset_paths
            .iter()
            .any(|path| path == "ocr/image-001.md"));
        assert!(result
            .asset_paths
            .iter()
            .any(|path| path == "ocr/image-002.md"));
        assert_eq!(
            result
                .asset_paths
                .iter()
                .filter(|path| path.ends_with(".md"))
                .count(),
            2
        );
        assert!(!staging.join(".ocr-input-shared").exists());
    }

    #[test]
    fn partial_multi_image_ocr_failure_never_promotes_a_partial_candidate() {
        let fixture = OrchestratorFixture::new("ocr-partial-failure");
        let error = execute_two_image_ocr(&fixture, true, false).unwrap_err();
        assert_eq!(error.code, "IMPORT_OCR_NO_TEXT");
        assert!(!fixture.root.join("raw").exists());
        assert!(!fixture.root.join("wiki").exists());
    }

    #[test]
    fn accurate_ocr_rejects_nonempty_text_below_the_confidence_floor() {
        let fixture = OrchestratorFixture::new("ocr-low-confidence");
        let error = execute_two_image_ocr(&fixture, false, true).unwrap_err();
        assert_eq!(error.code, "IMPORT_OCR_LOW_CONFIDENCE");
        assert!(!fixture.root.join("raw").exists());
        assert!(!fixture.root.join("wiki").exists());
    }

    #[test]
    fn accurate_ocr_accepts_the_exact_mean_and_readable_block_boundaries() {
        let warnings = validate_ocr_confidence_metadata(
            br#"{"confidence":0.75,"blocks":[{"confidence":0.50},{"confidence":0.75}]}"#,
            1,
        )
        .unwrap();
        assert_eq!(
            warnings,
            vec!["IMPORT_OCR_LOW_CONFIDENCE_BLOCK:image-1:block-1"]
        );
    }

    #[test]
    fn ocr_source_number_preserves_pdf_page_and_ordered_image_bindings() {
        assert_eq!(ocr_source_image_number(Path::new("page-002.png"), 0), 2);
        assert_eq!(ocr_source_image_number(Path::new("image-007.png"), 0), 7);
        assert_eq!(ocr_source_image_number(Path::new("unlabeled.png"), 4), 5);
    }

    #[test]
    fn standalone_image_with_no_ocr_text_fails_without_creating_source_or_raw() {
        let fixture = OrchestratorFixture::new("ocr-no-text");
        fixture
            .service
            .register_engine(Arc::new(RouteFixtureEngine {
                root: fixture.root.clone(),
                id: "ocr-empty",
                route: "ocr.basic",
                coverage: 0.0,
            }))
            .unwrap();
        let (session, item, first_task) = fixture.seed_one_item_named("image-no-text.png");
        let waiting = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &first_task.id,
            )
            .unwrap();
        assert_eq!(waiting.status, ImportItemStatus::WaitingAuthorization);
        fixture
            .service
            .authorize_media_for_session(
                &fixture.context,
                &fixture.files,
                &session.session_id,
                &item.item_id,
                ImportMediaAuthorizationKind::Ocr,
                None,
                None,
            )
            .unwrap();
        let second_task = fixture
            .tasks
            .create_project_task(
                TaskType::Import,
                fixture.context.project_id.clone(),
                fixture.root.clone(),
                "Authorized OCR".into(),
                true,
            )
            .unwrap();
        let error = fixture
            .service
            .run_item(
                &fixture.context,
                &fixture.files,
                &fixture.tasks,
                &session.session_id,
                &item.item_id,
                &second_task.id,
            )
            .unwrap_err();
        assert_eq!(error.code, "IMPORT_OCR_NO_TEXT");
        assert_eq!(fixture.reopen().items[0].status, ImportItemStatus::Failed);
        assert!(!fixture.root.join("raw").exists());
        assert!(!fixture.root.join("wiki").exists());
        assert!(!fixture.root.join(".app/sources").exists());
    }
}
