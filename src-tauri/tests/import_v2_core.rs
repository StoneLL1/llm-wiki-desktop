use llm_wiki_desktop_lib::{
    errors::{
        BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_COMMIT_CONFLICT,
        IMPORT_V2_ENGINE_OUTPUT_INVALID,
    },
    models::{
        import_v2::{
            CommitConflictAction, CommitImportSessionRequest, CommitItemDecision,
            ImportBatchResult, ImportInput, ImportInputKind, ImportItemStatus, ImportResourceMode,
            ImportSession,
        },
        paths::ProjectContext,
        task::TaskType,
    },
    services::{
        import_v2::{
            engine::{EngineDescriptor, EngineRequest, EngineResult, ImportEngine},
            source_registry::{SourceIndex, SourceManifest},
            ImportV2Service,
        },
        FileStore, GitService,
    },
    tasks::{task_model::CancellationToken, TaskService},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};

const SECRET: &str = "sk-fixture-must-never-persist";

#[derive(Default)]
struct FixtureEngine {
    root: PathBuf,
    fail_next: AtomicBool,
    escape_next: AtomicBool,
    secret_next: AtomicBool,
    contents: RwLock<BTreeMap<String, Vec<u8>>>,
}

impl FixtureEngine {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            ..Self::default()
        }
    }
    fn set_content(&self, locator: &str, bytes: &[u8]) {
        self.contents
            .write()
            .unwrap()
            .insert(locator.into(), bytes.into());
    }
}

impl ImportEngine for FixtureEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "fixture-core".into(),
            engine_version: "1.0.0".into(),
            route: "office.modern.docx".into(),
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
        if self.fail_next.swap(false, Ordering::SeqCst) {
            return Err(BackendError::new(
                "FIXTURE_ENGINE_FAILED",
                "Fixture failure.",
                true,
                false,
            ));
        }
        if self.secret_next.swap(false, Ordering::SeqCst) {
            return Err(BackendError::new(
                "FIXTURE_SECRET_FAILURE",
                format!("token={SECRET}"),
                true,
                false,
            ));
        }
        if self.escape_next.swap(false, Ordering::SeqCst) {
            return Ok(result("../escaped.md", "escape"));
        }
        let staging = self.root.join(
            request
                .staging_root
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        std::fs::create_dir_all(&staging).unwrap();
        let bytes = self
            .contents
            .read()
            .unwrap()
            .get(&request.input.locator)
            .cloned()
            .unwrap_or_else(|| request.input.display_name.as_bytes().to_vec());
        std::fs::write(staging.join("source.bin"), &bytes).unwrap();
        std::fs::write(
            staging.join("candidate.md"),
            format!("# {}\n\n{} bytes", request.input.display_name, bytes.len()),
        )
        .unwrap();
        Ok(result("candidate.md", &request.input.display_name))
    }
}

struct FixtureRouteEngine {
    inner: Arc<FixtureEngine>,
    engine_id: &'static str,
    route: &'static str,
}

impl ImportEngine for FixtureRouteEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: self.engine_id.into(),
            engine_version: "1.0.0".into(),
            route: self.route.into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        self.inner.supports(input)
    }

    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        self.inner.execute(request, cancellation)
    }
}

fn register_fixture_routes(service: &ImportV2Service, engine: &Arc<FixtureEngine>) {
    for (engine_id, route) in [
        ("fixture-core-docx", "office.modern.docx"),
        ("fixture-core-pdf", "pdf.text"),
    ] {
        service
            .register_engine(Arc::new(FixtureRouteEngine {
                inner: engine.clone(),
                engine_id,
                route,
            }))
            .unwrap();
    }
}

fn result(markdown: &str, title: &str) -> EngineResult {
    EngineResult {
        source_snapshot_path: "source.bin".into(),
        markdown_path: markdown.into(),
        asset_paths: vec![],
        metadata_path: None,
        title: title.into(),
        text_coverage: Some(1.0),
        table_cell_accuracy: None,
        sheet_count_exact: None,
        slide_count_exact: None,
        non_empty_cell_coverage: None,
        formula_value_pairs: None,
        meaningful_image_coverage: None,
        continuation: None,
        warnings: vec![],
    }
}

struct CoreIntegrationFixture {
    root: PathBuf,
    context: ProjectContext,
    files: FileStore,
    git: GitService,
    tasks: TaskService,
    service: ImportV2Service,
    engine: Arc<FixtureEngine>,
}

impl CoreIntegrationFixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "llm-wiki-import-v2-core-{}-资料",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(root.join(".app")).unwrap();
        std::fs::create_dir_all(root.join("raw/legacy")).unwrap();
        std::fs::write(root.join(".app/source-index.json"), b"legacy-index").unwrap();
        std::fs::write(root.join("raw/legacy/untouched.bin"), b"legacy-raw").unwrap();
        let context = ProjectContext::new("project-core-资料", root.clone());
        let engine = Arc::new(FixtureEngine::new(root.clone()));
        let service = ImportV2Service::default();
        register_fixture_routes(&service, &engine);
        Self {
            root,
            context,
            files: FileStore,
            git: GitService,
            tasks: TaskService::default(),
            service,
            engine,
        }
    }
    fn create_two_item_session(&self) -> ImportSession {
        let session = self
            .service
            .create_session(&self.context, &self.files, ImportResourceMode::Balanced)
            .unwrap();
        self.service
            .add_inputs(
                &self.context,
                &self.files,
                &session.session_id,
                vec![
                    input("研究报告.docx", r"D:\资料\研究报告.docx"),
                    input("失败项.pdf", r"D:\资料\失败项.pdf"),
                ],
            )
            .unwrap()
    }
    fn run_item(&self, session: &str, item: &str) -> Result<(), BackendError> {
        let task = self
            .tasks
            .create_project_task(
                TaskType::Import,
                self.context.project_id.clone(),
                self.root.clone(),
                "fixture import".into(),
                true,
            )
            .unwrap();
        self.service
            .run_item(
                &self.context,
                &self.files,
                &self.tasks,
                session,
                item,
                &task.id,
            )
            .map(|_| ())
    }
    fn engine_fail_next(&self) {
        self.engine.fail_next.store(true, Ordering::SeqCst);
    }
    fn reopen_service(&self) -> ImportV2Service {
        let service = ImportV2Service::default();
        register_fixture_routes(&service, &self.engine);
        service
    }
    fn commit_selected(
        &self,
        service: &ImportV2Service,
        session: &str,
        item: &str,
    ) -> ImportBatchResult {
        self.commit(service, session, item, None, None)
    }
    fn commit(
        &self,
        service: &ImportV2Service,
        session: &str,
        item: &str,
        action: Option<CommitConflictAction>,
        hash: Option<&str>,
    ) -> ImportBatchResult {
        service
            .commit_items(
                &self.context,
                &self.files,
                &self.git,
                &CommitImportSessionRequest {
                    project_id: self.context.project_id.clone(),
                    project_root_path: self.root.to_string_lossy().into(),
                    session_id: session.into(),
                    decisions: vec![CommitItemDecision {
                        item_id: item.into(),
                        conflict_action: action,
                        expected_wiki_hash: hash.map(str::to_string),
                    }],
                },
            )
            .unwrap()
    }
    fn raw_version_count(&self) -> usize {
        walk(&self.root.join("raw/sources"))
            .iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("original."))
            })
            .count()
    }
    fn wiki_page_count(&self) -> usize {
        walk(&self.root.join("wiki/sources"))
            .iter()
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .count()
    }
    fn import_one(&self, locator: &str) -> (String, String) {
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
                vec![input("研究报告.docx", locator)],
            )
            .unwrap();
        let item = session.items[0].item_id.clone();
        self.run_item(&session.session_id, &item).unwrap();
        (session.session_id, item)
    }
    fn reimport_same_bytes_is_exact_duplicate(&self, locator: &str) -> bool {
        let before = self.raw_version_count();
        let (s, i) = self.import_one(locator);
        self.commit_selected(&self.service, &s, &i).committed_count == 1
            && self.raw_version_count() == before
    }
    fn same_locator_new_version_is_recorded(&self, locator: &str) -> bool {
        self.engine.set_content(locator, b"second version");
        let (s, i) = self.import_one(locator);
        self.commit(
            &self.service,
            &s,
            &i,
            Some(CommitConflictAction::KeepWiki),
            None,
        )
        .committed_count
            == 1
            && self.raw_version_count() == 2
    }
    fn external_edit_update_is_blocked(&self, locator: &str) -> bool {
        let index: SourceIndex = self
            .files
            .read_json(&self.context, ".app/source-index-v2.json")
            .unwrap();
        let normalized_locator = locator.replace('\\', "/");
        let pointer = &index.by_locator[&normalized_locator];
        let manifest: SourceManifest = self
            .files
            .read_json(
                &self.context,
                &format!(".app/sources/{}.json", pointer.source_id),
            )
            .unwrap();
        std::fs::write(self.root.join(&manifest.wiki_path), "external edit").unwrap();
        self.engine.set_content(locator, b"third version");
        let before = self.raw_version_count();
        let (s, i) = self.import_one(locator);
        let result = self.commit(
            &self.service,
            &s,
            &i,
            Some(CommitConflictAction::ApplyMergedCandidate),
            Some("stale-hash"),
        );
        result.items[0].error_code.as_deref() == Some(IMPORT_V2_COMMIT_CONFLICT)
            && self.raw_version_count() == before
            && std::fs::read_to_string(self.root.join(manifest.wiki_path)).unwrap()
                == "external edit"
    }
}
impl Drop for CoreIntegrationFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn input(name: &str, locator: &str) -> ImportInput {
    ImportInput {
        source_identity: None,
        kind: ImportInputKind::File,
        display_name: name.into(),
        locator: locator.into(),
        normalized_locator: Some(locator.replace('\\', "/")),
    }
}
fn walk(root: &Path) -> Vec<PathBuf> {
    if !root.is_dir() {
        return vec![];
    }
    let (mut out, mut dirs) = (vec![], vec![root.to_path_buf()]);
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                dirs.push(p)
            } else {
                out.push(p)
            }
        }
    }
    out
}

#[test]
fn import_v2_core_is_resumable_partial_atomic_and_deduplicated() {
    let fixture = CoreIntegrationFixture::new();
    fixture
        .git
        .initialize_repository(&fixture.context, "initial")
        .unwrap();
    let session = fixture.create_two_item_session();
    fixture
        .run_item(&session.session_id, &session.items[0].item_id)
        .unwrap();
    fixture.engine_fail_next();
    assert!(fixture
        .run_item(&session.session_id, &session.items[1].item_id)
        .is_err());
    let reopened_service = fixture.reopen_service();
    let reopened = reopened_service
        .load_session(&fixture.context, &fixture.files, &session.session_id)
        .unwrap();
    assert_eq!(reopened.items[0].status, ImportItemStatus::PreviewReady);
    assert_eq!(reopened.items[1].status, ImportItemStatus::Failed);
    let batch = fixture.commit_selected(
        &reopened_service,
        &session.session_id,
        &reopened.items[0].item_id,
    );
    assert_eq!(batch.committed_count, 1);
    assert_eq!(fixture.raw_version_count(), 1);
    assert_eq!(fixture.wiki_page_count(), 1);
    let locator = r"D:\资料\研究报告.docx";
    assert!(fixture.reimport_same_bytes_is_exact_duplicate(locator));
    assert!(fixture.same_locator_new_version_is_recorded(locator));
    assert!(fixture.external_edit_update_is_blocked(locator));
    assert_eq!(
        std::fs::read(fixture.root.join(".app/source-index.json")).unwrap(),
        b"legacy-index"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("raw/legacy/untouched.bin")).unwrap(),
        b"legacy-raw"
    );
    assert!(fixture.root.join(".app/source-index-v2.json").is_file());
}

#[test]
fn import_v2_core_rejects_escape_and_redacts_secrets() {
    let fixture = CoreIntegrationFixture::new();
    let session = fixture
        .service
        .create_session(
            &fixture.context,
            &fixture.files,
            ImportResourceMode::Balanced,
        )
        .unwrap();
    let session = fixture
        .service
        .add_inputs(
            &fixture.context,
            &fixture.files,
            &session.session_id,
            vec![input("escape.docx", r"C:\safe\escape.docx")],
        )
        .unwrap();
    fixture.engine.escape_next.store(true, Ordering::SeqCst);
    assert_eq!(
        fixture
            .run_item(&session.session_id, &session.items[0].item_id)
            .unwrap_err()
            .code,
        IMPORT_V2_ENGINE_OUTPUT_INVALID
    );
    assert!(!fixture.root.join("escaped.md").exists());
    let session = fixture
        .service
        .create_session(
            &fixture.context,
            &fixture.files,
            ImportResourceMode::Balanced,
        )
        .unwrap();
    let session = fixture
        .service
        .add_inputs(
            &fixture.context,
            &fixture.files,
            &session.session_id,
            vec![input("secret.docx", r"C:\safe\secret.docx")],
        )
        .unwrap();
    fixture.engine.secret_next.store(true, Ordering::SeqCst);
    fixture
        .run_item(&session.session_id, &session.items[0].item_id)
        .unwrap_err();
    for path in walk(&fixture.root) {
        if let Ok(bytes) = std::fs::read(&path) {
            assert!(
                !String::from_utf8_lossy(&bytes).contains(SECRET),
                "secret leaked into {}",
                path.display()
            );
        }
    }
}

#[test]
fn import_v2_core_honors_cancellation() {
    let fixture = CoreIntegrationFixture::new();
    let session = fixture
        .service
        .create_session(
            &fixture.context,
            &fixture.files,
            ImportResourceMode::Balanced,
        )
        .unwrap();
    let session = fixture
        .service
        .add_inputs(
            &fixture.context,
            &fixture.files,
            &session.session_id,
            vec![input("取消.md", r"D:\资料\取消.md")],
        )
        .unwrap();
    let task = fixture
        .tasks
        .create_project_task(
            TaskType::Import,
            fixture.context.project_id.clone(),
            fixture.root.clone(),
            "cancel".into(),
            true,
        )
        .unwrap();
    fixture.tasks.cancel_task(&task.id).unwrap();
    let error = fixture
        .service
        .run_item(
            &fixture.context,
            &fixture.files,
            &fixture.tasks,
            &session.session_id,
            &session.items[0].item_id,
            &task.id,
        )
        .unwrap_err();
    assert_eq!(error.code, IMPORT_V2_CANCELLED);
    assert_eq!(
        fixture
            .service
            .load_session(&fixture.context, &fixture.files, &session.session_id)
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::Cancelled
    );
    assert_eq!(fixture.raw_version_count(), 0);
    assert_eq!(fixture.wiki_page_count(), 0);
}
