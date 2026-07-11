use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::errors::{
    BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ITEM_NOT_FOUND, IMPORT_V2_STATE_INVALID,
};
use crate::models::import_v2::{
    ImportInput, ImportIssue, ImportItem, ImportItemStatus, ImportResourceMode, ImportSession,
    ImportSessionStatus, ImportStage,
};
use crate::models::paths::ProjectContext;
use crate::models::task::{TaskResult, TaskStatus, TaskType};
use crate::services::import_v2::engine::{
    validate_engine_result, EngineOperation, EngineRegistry, EngineRequest, ImportEngine,
};
use crate::services::import_v2::quality_gate::QualityGate;
use crate::services::import_v2::SessionStore;
use crate::services::FileStore;
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

pub struct ImportV2Service {
    sessions: SessionStore,
    engines: EngineRegistry,
    quality: QualityGate,
    mutation_lock: Mutex<()>,
}

impl Default for ImportV2Service {
    fn default() -> Self {
        Self {
            sessions: SessionStore::default(),
            engines: EngineRegistry::default(),
            quality: QualityGate::default(),
            mutation_lock: Mutex::new(()),
        }
    }
}

impl ImportV2Service {
    pub fn create_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        mode: ImportResourceMode,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.sessions.create(context, files, mode)
    }
    pub fn add_inputs(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        inputs: Vec<ImportInput>,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        self.sessions.add_inputs(context, files, session_id, inputs)
    }
    pub fn load_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
    ) -> Result<ImportSession, BackendError> {
        self.sessions.load(context, files, session_id)
    }
    pub fn recover_session(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
    ) -> Result<ImportSession, BackendError> {
        let _guard = self.lock()?;
        let mut session = self.sessions.load(context, files, session_id)?;
        for item in &mut session.items {
            if matches!(
                item.status,
                ImportItemStatus::Inspecting
                    | ImportItemStatus::Extracting
                    | ImportItemStatus::Validating
            ) {
                let interrupted = item
                    .task_id
                    .as_deref()
                    .and_then(|id| tasks.get_task(id))
                    .is_none_or(|task| task.status == TaskStatus::Failed);
                if interrupted {
                    transition_item(item, ImportItemStatus::Failed)?;
                    item.issue = Some(ImportIssue {
                        code: "TASK_RECOVERY".into(),
                        message: "Import was interrupted and can be retried.".into(),
                        stage: ImportStage::Extract,
                        retryable: true,
                        user_action_required: false,
                    });
                }
            }
        }
        session.status = derive_session_status(&session.items);
        session.updated_at = chrono::Utc::now().to_rfc3339();
        self.sessions.save(context, files, &session)?;
        Ok(session)
    }
    pub fn register_engine(&self, engine: Arc<dyn ImportEngine>) -> Result<(), BackendError> {
        self.engines.register(engine)
    }
    pub fn set_item_selected(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        session_id: &str,
        item_id: &str,
        selected: bool,
    ) -> Result<ImportItem, BackendError> {
        let _guard = self.lock()?;
        let mut session = self.sessions.load(context, files, session_id)?;
        let item = find_item_mut(&mut session, item_id)?;
        item.selected = selected;
        let item = item.clone();
        persist_derived(&self.sessions, context, files, session)?;
        Ok(item)
    }

    pub fn run_item(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItem, BackendError> {
        let task = tasks
            .get_task(task_id)
            .ok_or_else(|| task_error("Import task was not found."))?;
        if task.task_type != TaskType::Import
            || task.project_id.as_deref() != Some(context.project_id.as_str())
            || !matches!(task.status, TaskStatus::Queued | TaskStatus::Cancelled)
        {
            return Err(task_error("Task is not compatible with this import item."));
        }
        let pre_cancelled = tasks.is_cancelled(task_id);
        self.mutate_item(context, files, session_id, item_id, |item| {
            if !matches!(
                item.status,
                ImportItemStatus::Queued | ImportItemStatus::Failed
            ) || item
                .task_id
                .as_deref()
                .is_some_and(|bound| bound != task_id && item.status != ImportItemStatus::Failed)
            {
                return Err(task_error(
                    "Import item is already claimed by another task.",
                ));
            }
            item.task_id = Some(task_id.to_string());
            item.issue = None;
            transition_item(
                item,
                if pre_cancelled {
                    ImportItemStatus::Cancelled
                } else {
                    ImportItemStatus::Inspecting
                },
            )
        })?;
        if pre_cancelled {
            return Err(cancelled_error());
        }
        task_call(tasks.transition_status(task_id, TaskStatus::Running))?;
        task_call(tasks.update_progress(task_id, 0, Some(4), Some("Inspecting input".into())))?;
        let input = self
            .load_session(context, files, session_id)?
            .items
            .into_iter()
            .find(|item| item.item_id == item_id)
            .ok_or_else(item_not_found)?
            .input;
        let engine = match self.engines.resolve(&input) {
            Ok(engine) => engine,
            Err(error) => {
                self.mutate_item(context, files, session_id, item_id, |item| {
                    transition_item(item, ImportItemStatus::WaitingCapability)?;
                    item.issue = Some(issue_from_engine_error(&error, ImportStage::Route));
                    Ok(())
                })?;
                task_call(tasks.append_log(
                    task_id,
                    LogLevel::Warn,
                    "No available import engine supports this input.".into(),
                ))?;
                task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
                return Err(error);
            }
        };
        self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::Extracting)
        })?;
        task_call(tasks.update_progress(task_id, 1, Some(4), Some("Extracting source".into())))?;
        let staging_root = format!(".app/import-sessions/{session_id}/items/{item_id}/staging");
        let request = EngineRequest {
            protocol_version: "2".into(),
            request_id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            item_id: item_id.into(),
            task_id: task_id.into(),
            operation: EngineOperation::Extract,
            input,
            staging_root: staging_root.clone(),
        };
        let token = tasks
            .get_cancellation_token(task_id)
            .ok_or_else(|| task_error("Task cancellation state is unavailable."))?;
        let result = match engine.execute(&request, &token) {
            Ok(result) if !token.is_cancelled() => result,
            Ok(_) => {
                return self.finish_cancelled(context, files, tasks, session_id, item_id, task_id)
            }
            Err(_) if token.is_cancelled() => {
                return self.finish_cancelled(context, files, tasks, session_id, item_id, task_id)
            }
            Err(error) => {
                return self.finish_failed(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    error,
                    ImportStage::Extract,
                )
            }
        };
        if let Err(error) = validate_engine_result(&staging_root, &result) {
            return self.finish_failed(
                context,
                files,
                tasks,
                session_id,
                item_id,
                task_id,
                error,
                ImportStage::Extract,
            );
        }
        self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::Validating)
        })?;
        task_call(tasks.update_progress(task_id, 3, Some(4), Some("Validating preview".into())))?;
        let preview = match self
            .quality
            .evaluate(&context.root.join(Path::new(&staging_root)), &result)
        {
            Ok(preview) => preview,
            Err(error) => {
                return self.finish_failed(
                    context,
                    files,
                    tasks,
                    session_id,
                    item_id,
                    task_id,
                    error,
                    ImportStage::Validate,
                )
            }
        };
        let item = self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::PreviewReady)?;
            item.preview = Some(preview);
            item.issue = None;
            Ok(())
        })?;
        task_call(tasks.update_progress(task_id, 4, Some(4), Some("Preview ready".into())))?;
        task_call(tasks.set_result(
            task_id,
            TaskResult {
                summary: format!("Import preview ready for session {session_id}, item {item_id}"),
                affected_paths: vec![staging_root],
                pending_action: None,
            },
        ))?;
        task_call(tasks.transition_status(task_id, TaskStatus::WaitingForConfirmation))?;
        Ok(item)
    }

    fn finish_cancelled(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::Cancelled)
        })?;
        if tasks
            .get_task(task_id)
            .is_some_and(|task| task.status != TaskStatus::Cancelled)
        {
            task_call(tasks.cancel_task(task_id))?;
        }
        Err(cancelled_error())
    }
    fn finish_failed(
        &self,
        context: &ProjectContext,
        files: &FileStore,
        tasks: &TaskService,
        session_id: &str,
        item_id: &str,
        task_id: &str,
        error: BackendError,
        stage: ImportStage,
    ) -> Result<ImportItem, BackendError> {
        self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::Failed)?;
            item.issue = Some(issue_from_engine_error(&error, stage));
            Ok(())
        })?;
        task_call(tasks.append_log(task_id, LogLevel::Error, "Import engine failed.".into()))?;
        task_call(tasks.set_error(task_id, issue_safe_error(&error)))?;
        task_call(tasks.transition_status(task_id, TaskStatus::Failed))?;
        Err(issue_safe_error(&error))
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
        let mut session = self.sessions.load(context, files, session_id)?;
        let item = find_item_mut(&mut session, item_id)?;
        mutation(item)?;
        let item = item.clone();
        persist_derived(&self.sessions, context, files, session)?;
        Ok(item)
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, BackendError> {
        self.mutation_lock
            .lock()
            .map_err(|_| task_error("Import session mutation lock is unavailable."))
    }
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
fn task_error(message: &str) -> BackendError {
    BackendError::new(IMPORT_V2_STATE_INVALID, message, true, false)
}
fn task_call<T>(result: Result<T, String>) -> Result<T, BackendError> {
    result.map_err(|_| task_error("Import task state could not be updated."))
}
fn cancelled_error() -> BackendError {
    BackendError::new(IMPORT_V2_CANCELLED, "Import was cancelled.", true, false)
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
    ImportIssue {
        code: error.code.clone(),
        message: "Import engine failed.".into(),
        stage,
        retryable: error.recoverable,
        user_action_required: error.user_action_required,
    }
}
fn derive_session_status(items: &[ImportItem]) -> ImportSessionStatus {
    use ImportItemStatus::*;
    let has =
        |statuses: &[ImportItemStatus]| items.iter().any(|item| statuses.contains(&item.status));
    if has(&[Inspecting, Extracting, Validating, Committing]) {
        ImportSessionStatus::Processing
    } else if has(&[Completed]) && has(&[Failed, Cancelled]) {
        ImportSessionStatus::PartiallyCommitted
    } else if has(&[Completed])
        && items
            .iter()
            .all(|item| matches!(item.status, Completed | Skipped))
    {
        ImportSessionStatus::Completed
    } else if !items.is_empty() && items.iter().all(|item| item.status == Cancelled) {
        ImportSessionStatus::Cancelled
    } else if has(&[
        PreviewReady,
        NeedsMerge,
        WaitingCapability,
        WaitingLogin,
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
    use std::sync::{Arc, Condvar, Mutex};

    use crate::errors::{
        BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_ENGINE_UNAVAILABLE, IMPORT_V2_STATE_INVALID,
    };
    use crate::models::import_v2::{
        ImportInput, ImportItem, ImportItemStatus, ImportResourceMode, ImportSession,
        ImportSessionStatus, ImportStage,
    };
    use crate::models::paths::ProjectContext;
    use crate::models::task::{BackendTask, TaskStatus, TaskType};
    use crate::services::import_v2::engine::{
        EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
    };
    use crate::services::import_v2::test_support::{test_context, test_file_input};
    use crate::services::FileStore;
    use crate::tasks::task_model::CancellationToken;
    use crate::tasks::TaskService;

    use super::*;

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
                route: "fixture".into(),
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
                title: "Fixture".into(),
                text_coverage: Some(1.0),
                table_cell_accuracy: None,
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
    impl ImportEngine for FailingEngine {
        fn descriptor(&self) -> EngineDescriptor {
            EngineDescriptor {
                engine_id: "failing".into(),
                engine_version: "1".into(),
                route: "fixture".into(),
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
            let session = self
                .service
                .create_session(&self.context, &self.files, ImportResourceMode::Balanced)
                .unwrap();
            let session = self
                .service
                .add_inputs(
                    &self.context,
                    &self.files,
                    &session.session_id,
                    vec![test_file_input("a.pdf")],
                )
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
                .next()
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
    fn run_item_records_engine_unavailable_without_losing_session() {
        let fixture = OrchestratorFixture::new("no-engine");
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
    fn transition_helper_rejects_preview_to_completed() {
        let mut item = ImportItem::queued("item-1", test_file_input("a.pdf"));
        item.status = ImportItemStatus::PreviewReady;
        let error = transition_item(&mut item, ImportItemStatus::Completed).unwrap_err();
        assert_eq!(error.code, IMPORT_V2_STATE_INVALID);
    }

    #[test]
    fn engine_error_is_reduced_to_a_secret_free_issue() {
        let error = BackendError::new("ENGINE_FAILED", "Authorization: Bearer secret", true, false)
            .with_details(serde_json::json!({ "path": "C:/Users/Aletta/private.pdf" }));
        let issue = issue_from_engine_error(&error, ImportStage::Extract);
        assert_eq!(issue.message, "Import engine failed.");
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
            assert_eq!(reconciled.items[0].status, ImportItemStatus::Failed);
            assert_eq!(
                reconciled.items[0].issue.as_ref().unwrap().code,
                "TASK_RECOVERY"
            );
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
        let session = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let session = service
            .add_inputs(
                &context,
                &files,
                &session.session_id,
                vec![test_file_input("a.pdf")],
            )
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
}
