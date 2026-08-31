use llm_wiki_desktop_lib::{
    errors::{
        BackendError, IMPORT_V2_CANCELLED, IMPORT_V2_COMMIT_CONFLICT,
        IMPORT_V2_ENGINE_OUTPUT_INVALID,
    },
    models::{
        import_v2::{
            CommitImportSessionRequest, CommitItemDecision, ImportBatchResult, ImportInput,
            ImportInputKind, ImportItemResolution, ImportItemStatus, ImportRecoveryReason,
            ImportResourceMode, ImportSession, SourceIdentity,
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
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
};

const SECRET: &str = "sk-fixture-must-never-persist";

#[test]
fn application_temp_preview_uses_non_persistent_tasks_and_leaves_read_only_project_untouched() {
    let fixture = CoreIntegrationFixture::new();
    let read_only_project = std::env::temp_dir().join(format!(
        "llm-wiki-import-v2-read-only-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&read_only_project).unwrap();
    std::fs::write(read_only_project.join("现有 笔记.md"), "# Existing\n").unwrap();
    let before = std::fs::read(read_only_project.join("现有 笔记.md")).unwrap();

    let session = fixture.create_two_item_session();
    let item = &session.items[0];
    let task = fixture
        .tasks
        .create_memory_project_task(
            TaskType::Import,
            fixture.context.project_id.clone(),
            read_only_project.clone(),
            "Temporary preview".into(),
            true,
        )
        .unwrap();
    fixture
        .service
        .bind_item_task_ids(
            &fixture.context,
            &fixture.files,
            &session.session_id,
            &[(item.item_id.clone(), task.id.clone())],
        )
        .unwrap();
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
        .unwrap();

    assert_eq!(preview.status, ImportItemStatus::PreviewReady);
    assert!(fixture
        .tasks
        .task_belongs_to_root(&task.id, &read_only_project));
    assert!(!fixture.root.join(".app/tasks").exists());
    assert_eq!(
        std::fs::read(read_only_project.join("现有 笔记.md")).unwrap(),
        before
    );
    assert_eq!(std::fs::read_dir(&read_only_project).unwrap().count(), 1);
    std::fs::remove_dir_all(read_only_project).ok();
}

#[test]
fn compatible_layout_preview_commit_and_history_never_backfill_native_roots() {
    let mut fixture = CoreIntegrationFixture::new();
    std::fs::create_dir_all(fixture.root.join("Notes/Sources")).unwrap();
    let layout = &mut fixture.context.layout;
    layout.app_state_root = Some(".app/compat".into());
    layout.import_state_root = Some(".app/compat/import-sessions".into());
    layout.source_state_root = Some(".app/compat/sources".into());
    layout.task_state_root = Some(".app/compat/tasks".into());
    layout.evidence_root = Some(".app/compat/evidence".into());
    layout.source_write_root = Some("Notes/Sources".into());
    layout.wiki_write_root = Some("Notes".into());

    let session = fixture.create_two_item_session();
    let item_id = session.items[0].item_id.clone();
    fixture.run_item(&session.session_id, &item_id).unwrap();
    let batch = fixture.commit_selected(&fixture.service, &session.session_id, &item_id);
    assert_eq!(batch.committed_count, 1, "compatible commit: {batch:#?}");
    assert!(fixture.root.join(".app/compat/sources").is_dir());
    assert!(fixture
        .root
        .join(".app/compat/source-index-v2.json")
        .is_file());
    assert!(fixture.root.join(".app/compat/import-history").is_dir());
    assert!(fixture.root.join(".app/compat/evidence/sources").is_dir());
    assert!(fixture
        .root
        .join("Notes/Sources/local/研究报告.md")
        .is_file());
    assert!(!fixture.root.join(".app/sources").exists());
    assert!(!fixture.root.join(".app/tasks").exists());
    assert!(!fixture.root.join("raw/sources").exists());
    assert!(!fixture.root.join("wiki/sources").exists());
}

#[test]
fn overview_reports_independent_filesystem_recovery_facts() {
    let fixture = CoreIntegrationFixture::new();
    let session = fixture.create_two_item_session();
    let import_paths = fixture.context.layout.import_paths().unwrap();
    let journal_root = fixture
        .context
        .resolve_project_path(&import_paths.recovery_journal_root())
        .unwrap();
    std::fs::create_dir_all(&journal_root).unwrap();
    let recovery_probe = import_paths
        .item_record(&session.session_id, &session.items[0].item_id)
        .unwrap();
    std::fs::write(
        journal_root.join("interrupted.json"),
        serde_json::to_vec(&serde_json::json!({
            "state": "InProgress",
            "entries": [{
                "relative_path": recovery_probe,
                "previous": null,
                "desired_hash": "fixture",
                "desired_absent": false
            }],
            "recovery_artifacts": []
        }))
        .unwrap(),
    )
    .unwrap();
    let staging = fixture
        .context
        .resolve_project_path(
            &import_paths
                .item_staging(&session.session_id, &session.items[0].item_id)
                .unwrap(),
        )
        .unwrap();
    std::fs::create_dir_all(staging.join("remote")).unwrap();
    std::fs::write(staging.join("remote/chunk.part"), b"partial").unwrap();

    let overview = fixture
        .service
        .read_session_overview(&fixture.context, &fixture.files, &session.session_id)
        .unwrap();

    assert!(overview.recovery_required);
    assert!(overview
        .recovery_reasons
        .contains(&ImportRecoveryReason::IncompleteJournal));
    assert!(overview
        .recovery_reasons
        .contains(&ImportRecoveryReason::PartialRemoteDownload));
}

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
    source_root: PathBuf,
    context: ProjectContext,
    files: FileStore,
    git: GitService,
    tasks: TaskService,
    service: ImportV2Service,
    engine: Arc<FixtureEngine>,
}

impl CoreIntegrationFixture {
    fn new() -> Self {
        let fixture_id = uuid::Uuid::new_v4();
        let root =
            std::env::temp_dir().join(format!("llm-wiki-import-v2-core-{}-资料", fixture_id));
        let source_root =
            std::env::temp_dir().join(format!("llm-wiki-import-v2-core-inputs-{fixture_id}"));
        std::fs::create_dir_all(root.join(".app")).unwrap();
        std::fs::create_dir_all(root.join("raw/legacy")).unwrap();
        std::fs::create_dir_all(&source_root).unwrap();
        let docx =
            include_bytes!("../../tests/fixtures/import-v2/local/batch3/matrix/document.docx");
        let pdf = include_bytes!("../../tests/fixtures/import-v2/local/batch3/matrix/document.pdf");
        for name in ["report.docx", "escape.docx", "secret.docx"] {
            std::fs::write(source_root.join(name), docx).unwrap();
        }
        std::fs::write(source_root.join("failed.pdf"), pdf).unwrap();
        std::fs::write(source_root.join("cancel.md"), b"# cancel fixture\n").unwrap();
        std::fs::write(root.join(".app/source-index.json"), b"legacy-index").unwrap();
        std::fs::write(root.join("raw/legacy/untouched.bin"), b"legacy-raw").unwrap();
        let context = ProjectContext::new("project-core-资料", root.clone());
        let engine = Arc::new(FixtureEngine::new(root.clone()));
        let service = ImportV2Service::default();
        register_fixture_routes(&service, &engine);
        Self {
            root,
            source_root,
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
                    input(
                        "研究报告.docx",
                        &self.source_root.join("report.docx").to_string_lossy(),
                    ),
                    input(
                        "失败项.pdf",
                        &self.source_root.join("failed.pdf").to_string_lossy(),
                    ),
                ],
            )
            .unwrap()
    }
    fn run_item(&self, session: &str, item: &str) -> Result<(), BackendError> {
        let task_state_root = self
            .context
            .layout
            .task_state_root
            .as_deref()
            .map(|relative| self.context.resolve_project_path(relative).unwrap())
            .unwrap_or_else(|| self.root.join(".app/tasks"));
        let task = self
            .tasks
            .create_project_task_at(
                TaskType::Import,
                self.context.project_id.clone(),
                self.root.clone(),
                task_state_root,
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
        let resolution = service
            .load_session(&self.context, &self.files, session)
            .unwrap()
            .items
            .into_iter()
            .find(|candidate| candidate.item_id == item)
            .and_then(|candidate| candidate.preview)
            .and_then(|preview| preview.resolution)
            .and_then(|resolution| resolution.default_resolution);
        self.commit(service, session, item, resolution)
    }
    fn commit(
        &self,
        service: &ImportV2Service,
        session: &str,
        item: &str,
        resolution: Option<ImportItemResolution>,
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
                    batch_task_id: None,
                    acknowledge_restricted_content: false,
                    expected_selection_revision: None,
                    expected_confirmation_digest: None,
                    decisions: vec![CommitItemDecision {
                        item_id: item.into(),
                        resolution,
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
        self.commit_selected(&self.service, &s, &i).committed_count == 1
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
        let session = self
            .service
            .load_session(&self.context, &self.files, &s)
            .unwrap();
        let preview = session.items[0].preview.as_ref().unwrap();
        let result = self.commit(
            &self.service,
            &s,
            &i,
            Some(ImportItemResolution::ApplyImportCandidate {
                source_id: pointer.source_id.clone(),
                candidate_hash: preview.source_snapshot.sha256.clone(),
                current_hash: "stale-hash".into(),
                target_version_id: manifest.current_version_id.clone(),
            }),
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
        let _ = std::fs::remove_dir_all(&self.source_root);
    }
}

fn input(name: &str, locator: &str) -> ImportInput {
    let path = Path::new(locator).canonicalize().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    ImportInput {
        source_identity: Some(SourceIdentity {
            canonical_path: path.to_string_lossy().into_owned(),
            size_bytes: bytes.len() as u64,
            modified_nanos: None,
            file_id: None,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
            magic: format!("{:x}", Sha256::digest(&bytes[..bytes.len().min(8192)])),
        }),
        kind: ImportInputKind::File,
        display_name: name.into(),
        locator: path.to_string_lossy().into_owned(),
        normalized_locator: Some(path.to_string_lossy().replace('\\', "/")),
        media_save_mode: Default::default(),
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
    reopened_service
        .skip_item(
            &fixture.context,
            &fixture.files,
            &fixture.tasks,
            &session.session_id,
            &reopened.items[1].item_id,
        )
        .unwrap();
    let locator = fixture
        .source_root
        .join("report.docx")
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(fixture.reimport_same_bytes_is_exact_duplicate(&locator));
    assert!(fixture.same_locator_new_version_is_recorded(&locator));
    assert!(fixture.external_edit_update_is_blocked(&locator));
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
fn import_v2_text_input_is_staged_before_identity_is_recorded() {
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
        .add_text_input(
            &fixture.context,
            &fixture.files,
            &session.session_id,
            "clipboard-note.md",
            "# Clipboard note\n",
        )
        .unwrap();

    let input = &session.items[0].input;
    let staged = fixture.root.join(&input.locator);
    assert_eq!(
        std::fs::read_to_string(staged).unwrap(),
        "# Clipboard note\n"
    );
    assert!(input.source_identity.is_some());
    assert!(!fixture.root.join("raw/sources").exists());
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
            vec![input(
                "escape.docx",
                &fixture.source_root.join("escape.docx").to_string_lossy(),
            )],
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
    fixture
        .service
        .skip_item(
            &fixture.context,
            &fixture.files,
            &fixture.tasks,
            &session.session_id,
            &session.items[0].item_id,
        )
        .unwrap();
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
            vec![input(
                "secret.docx",
                &fixture.source_root.join("secret.docx").to_string_lossy(),
            )],
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
            vec![input(
                "取消.md",
                &fixture.source_root.join("cancel.md").to_string_lossy(),
            )],
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
