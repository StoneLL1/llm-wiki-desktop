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
use crate::models::task::{TaskResult, TaskStatus};
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
        self.mutate_item(context, files, session_id, item_id, |item| {
            item.task_id = Some(task_id.to_string());
            Ok(())
        })?;
        if tasks.is_cancelled(task_id) {
            self.mutate_item(context, files, session_id, item_id, |item| {
                transition_item(item, ImportItemStatus::Cancelled)
            })?;
            return Err(cancelled_error());
        }
        task_call(tasks.transition_status(task_id, TaskStatus::Running))?;
        task_call(tasks.update_progress(task_id, 0, Some(4), Some("Inspecting input".into())))?;
        self.mutate_item(context, files, session_id, item_id, |item| {
            transition_item(item, ImportItemStatus::Inspecting)
        })?;
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
        let _ = tasks.append_log(task_id, LogLevel::Error, "Import engine failed.".into());
        let _ = tasks.set_error(task_id, issue_safe_error(&error));
        let _ = tasks.transition_status(task_id, TaskStatus::Failed);
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
    use std::sync::Arc;

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
}
