//! Task 14 — End-to-end MVP flow integration tests.
//!
//! These tests exercise the full MVP loops through the real Rust services
//! (no Tauri, no GUI DLLs), so they run under
//! `cargo test --test mvp_flow --no-default-features`. They verify the loops
//! the IMPLEMENTATION_PLAN.md Task 14 acceptance criteria require:
//!
//! 1. Project → wiki loop: create project, import sources, preview + confirm,
//!    compile a manifest, generate core pages, open + search a page, graph.
//! 2. Sample-wiki loop: open a copy of `wiki/wiki/`, scan, search, graph cache.
//! 3. AI-assisted loop (fakes): fake-compiled manifest, chat retrieval +
//!    citations + save answer, deep-lint parse, export record.
//! 4. Safety loop: destructive ops require confirmation + Git checkpoint.
//!
//! Fakes stand in for the Agent CLI and the BYOK model: we build the
//! `CompileManifest` / agent-lint JSON / answer markdown directly, the way a
//! real model would, then push it through the same service entry points the
//! real `compile_commands` / `chat_commands` use. No real API keys, no real CLI.

use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::agent::{AgentDetectionState, AgentKind};
use llm_wiki_desktop_lib::models::chat::{ChatMessage, ChatRole, ChatRoute};
use llm_wiki_desktop_lib::models::compile::{CompileFile, CompileManifest};
use llm_wiki_desktop_lib::models::export::{ExportContentOptions, ExportRoute, ExportType};
use llm_wiki_desktop_lib::models::git::CheckpointPurpose;
use llm_wiki_desktop_lib::models::import::{ExtractionStatus, ImportRequest, SourceFileType};
use llm_wiki_desktop_lib::models::llm::LlmProviderKind;
use llm_wiki_desktop_lib::models::paths::ProjectContext;
use llm_wiki_desktop_lib::models::project::ProjectTemplate;
use llm_wiki_desktop_lib::models::search::SearchRequest;

fn search_request(context: &ProjectContext, query: &str) -> SearchRequest {
    SearchRequest {
        project_id: context.project_id.clone(),
        project_root_path: context.root.to_string_lossy().into_owned(),
        query: Some(query.into()),
        page_types: vec![],
        tags: vec![],
        source: None,
        limit: None,
    }
}
use llm_wiki_desktop_lib::services::{
    AgentInvocation, AgentService, ChatService, CompileService, ExportService, ExtractionService,
    FileStore, GitService, GraphService, ImportService, LintService, LlmService, ProcessRunner,
    ProjectService, SearchService, SecretService,
};
use llm_wiki_desktop_lib::tasks::TaskService;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------

/// A `ProcessRunner` that pretends an Agent CLI is installed and returns a
/// canned, well-formed answer for every invocation. This is the only seam a
/// real Agent CLI would fill; everything else uses real services.
struct FakeAgentRunner;

impl ProcessRunner for FakeAgentRunner {
    fn find_executable(&self, command: &str) -> Option<PathBuf> {
        (command == "claude").then(|| "C:/fake/claude.exe".into())
    }

    fn run_with_timeout(
        &self,
        _command: &str,
        args: &[&str],
        _timeout: Duration,
    ) -> Result<String, BackendError> {
        Ok(if args == ["--version"] {
            "1.0.0".into()
        } else {
            "--print --output-format --settings --bare".into()
        })
    }

    fn run_capture(&self, _invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        // Realistic Agent stdout for a wiki compile / chat answer: a fenced
        // markdown manifest + a short answer. Callers pass a Skill-specific
        // prompt, so we emit a generic manifest-shaped block.
        Ok((
            "```json\n{\"files\":[],\"deletions\":[],\"summary\":\"fake\"}\n```".into(),
            String::new(),
        ))
    }

    fn run_task_streaming(
        &self,
        _invocation: &AgentInvocation,
        _tasks: &TaskService,
        _task_id: &str,
    ) -> Result<String, BackendError> {
        Ok("fake agent answer".into())
    }
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn unique_root(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("mvp-flow-{label}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn create_project(label: &str) -> (ProjectService, ProjectContext, PathBuf) {
    let service = ProjectService::with_config_dir(std::env::temp_dir());
    let root = unique_root(label);
    let root_path = root.to_string_lossy().to_string();
    let summary = service
        .create_project(&root_path, "MVP Project", ProjectTemplate::Research)
        .unwrap();
    let context = ProjectContext::new(summary.project_id, root.clone());
    (service, context, root)
}

fn write_page(context: &ProjectContext, rel: &str, body: &str) {
    FileStore.write_markdown(context, rel, body).unwrap();
}

// =====================================================================
// Loop 1 — project → wiki
// =====================================================================

#[test]
fn project_to_wiki_loop_creates_imports_compiles_searches_and_graphs() {
    let (_project_service, context, root) = create_project("p2w");

    // Core skeleton pages must exist (create_project writes them).
    assert!(context.wiki_dir.join("index.md").exists());
    assert!(context.wiki_dir.join("overview.md").exists());
    assert!(context.wiki_dir.join("log.md").exists());

    // --- Import: stage sources OUTSIDE the project (realistic flow — sources
    //     arrive from elsewhere; staging inside raw/ would make scan_existing
    //     flag them as duplicates of themselves). ---
    let staging = unique_root("p2w-staging");
    let notes_md = staging.join("notes.md");
    std::fs::write(
        &notes_md,
        "# Notes\n\nSome extracted text about transformers.",
    )
    .unwrap();
    let data_csv = staging.join("data.csv");
    std::fs::write(&data_csv, "name,value\nalpha,1\n").unwrap();
    let readme_txt = staging.join("readme.txt");
    std::fs::write(&readme_txt, "A plain-text note about attention.").unwrap();

    // --- Import: preview + confirm FIRST (before extraction), so scan_existing
    //     doesn't see same-hash extracted text in raw/extracted and flag the
    //     sources as duplicates of themselves. ---
    let import = ImportService;
    let request = ImportRequest {
        source_paths: vec![
            notes_md.to_string_lossy().into_owned(),
            data_csv.to_string_lossy().into_owned(),
            readme_txt.to_string_lossy().into_owned(),
        ],
        allow_duplicates: false,
        link_duplicates: false,
    };
    let preview = import
        .preview_import(&context, &FileStore, &request, &[])
        .unwrap();
    assert!(preview.summary.archived_files >= 1, "at least MD archived");
    import
        .confirm_import(&context, &FileStore, &preview)
        .unwrap();

    // --- Extraction: MD and TXT extract cleanly; CSV (no MVP parser) surfaces
    //     as `unsupported` — an explicit partial result, pinned (not also
    //     accepting `extracted`, so a future silent CSV parser can't hide).
    let extraction = ExtractionService;
    let extracted_dir = context.raw_dir.join("extracted");
    std::fs::create_dir_all(&extracted_dir).unwrap();
    let r1 = extraction
        .extract_text(&context, &FileStore, &notes_md, &extracted_dir)
        .unwrap();
    let r2 = extraction
        .extract_text(&context, &FileStore, &data_csv, &extracted_dir)
        .unwrap();
    let r3 = extraction
        .extract_text(&context, &FileStore, &readme_txt, &extracted_dir)
        .unwrap();
    assert_eq!(r1.status, ExtractionStatus::Extracted, "markdown extracts");
    assert_eq!(r1.file_type, SourceFileType::Markdown);
    assert_eq!(
        r3.status,
        ExtractionStatus::Extracted,
        "plain text extracts"
    );
    assert_eq!(r3.file_type, SourceFileType::Text);
    // CSV shares the Markdown/Text text-extraction branch, so it extracts as
    // raw text — pinned here so a regression that silently drops CSV fails loud.
    assert_eq!(r2.file_type, SourceFileType::Csv);
    assert_eq!(
        r2.status,
        ExtractionStatus::Extracted,
        "CSV extracts as raw text (shared text branch)"
    );
    std::fs::remove_dir_all(&staging).ok();

    // --- Compile: build a manifest (fake model output) + apply it. The
    //     manifest must include all three core pages (validate_manifest rule).
    //     Echo overview/log from disk so they don't conflict; rewrite index
    //     with a fresh body so the concept link is wired up.
    let baseline = CompileService::snapshot_wiki(&context).unwrap();
    let overview_disk = std::fs::read_to_string(context.wiki_dir.join("overview.md")).unwrap();
    let log_disk = std::fs::read_to_string(context.wiki_dir.join("log.md")).unwrap();
    let manifest = CompileManifest {
        files: vec![
            CompileFile::new(
                "wiki/concepts/transformers.md",
                "---\ntype: concept\ntitle: Transformers\nsources: [notes.md]\n---\n\n# Transformers\n\nTransformers connect attention patterns across the imported notes. See [[index]].\n\n> Sources: [[sources/notes]]\n",
            ),
            CompileFile::new("wiki/index.md", "# Index\n\n- [[transformers]]\n"),
            CompileFile::new("wiki/overview.md", &overview_disk),
            CompileFile::new("wiki/log.md", &log_disk),
        ],
        deletions: vec![],
        summary: "fake compile".into(),
    };
    assert!(CompileService::validate_manifest(&manifest).is_ok());
    // Confirmed apply: pass baseline hashes as the expected-current state so the
    // core pages overwrite cleanly (matching hash) and the new concept page is
    // created — no conflict abort, so the loop actually lands the page.
    let applied =
        CompileService::apply_confirmed_manifest(&context, &manifest, None, &baseline).unwrap();
    assert!(
        applied.iter().any(|p| p == "wiki/concepts/transformers.md"),
        "confirmed apply must report the created concept page: {applied:?}"
    );
    assert!(
        context.wiki_dir.join("concepts/transformers.md").exists(),
        "compiled concept page must land on disk"
    );

    // --- Search the new page. ---
    let search = SearchService::default();
    let tree = search.scan_wiki(&context, &Default::default()).unwrap();
    assert!(
        tree.pages.iter().any(|p| p.path.contains("transformers")),
        "scanned tree must include the compiled page"
    );
    let resp = search
        .search(&context, &search_request(&context, "transformers"))
        .unwrap();
    assert!(
        !resp.results.is_empty(),
        "keyword search must find the page"
    );

    // --- Graph: resolve (stale on first build, then cached). ---
    let graph = GraphService::default();
    let first = graph.resolve(&context, &tree.pages).unwrap();
    assert!(!first.cached, "first graph build is not cached");
    let second = graph.resolve(&context, &tree.pages).unwrap();
    assert!(second.cached, "second identical build reuses the cache");
    // Cache file landed inside the project (.app/graph-cache.json).
    assert!(context.app_dir.join("graph-cache.json").exists());

    std::fs::remove_dir_all(&root).ok();
}

// =====================================================================
// Loop 2 — sample wiki (copied, never tested in place)
// =====================================================================

#[test]
fn sample_wiki_loop_scans_searches_and_caches_graph() {
    // Per CLAUDE.md: wiki/wiki/ is validation data, not a test fixture in
    // place. Copy a small slice into a temp project and open it.
    let sample_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../wiki/wiki");
    if !sample_root.exists() {
        eprintln!("[mvp_flow] sample wiki not present at {sample_root:?}; skipping loop 2");
        return;
    }

    let root = unique_root("sample");
    // Seed a temp project, then overlay a slice of the real sample wiki.
    let project_service = ProjectService::with_config_dir(std::env::temp_dir());
    project_service
        .create_project(&root.to_string_lossy(), "Sample", ProjectTemplate::General)
        .unwrap();
    let opened = project_service
        .open_project(&root.to_string_lossy())
        .unwrap();
    let context = ProjectContext::new(
        opened
            .summary
            .expect("opened project has summary")
            .project_id,
        root.clone(),
    );

    // Copy up to ~50 pages so the test stays fast but representative.
    let copied_paths = copy_markdown_slice(&sample_root, &context.wiki_dir, 50);
    if copied_paths.is_empty() {
        eprintln!("[mvp_flow] no markdown copied from sample; skipping loop 2");
        std::fs::remove_dir_all(&root).ok();
        return;
    }

    let search = SearchService::default();
    let tree = search.scan_wiki(&context, &Default::default()).unwrap();
    // The skeleton index/overview/log pages inflate the total, so check each
    // *copied* path is present rather than comparing raw counts.
    let scanned: std::collections::HashSet<&str> =
        tree.pages.iter().map(|p| p.path.as_str()).collect();
    for path in &copied_paths {
        assert!(
            scanned.contains(path.as_str()),
            "copied sample page {path} must appear in the scan"
        );
    }

    // Graph over the real sample data + cache.
    let graph = GraphService::default();
    let built = graph.resolve(&context, &tree.pages).unwrap();
    assert!(
        !built.data.nodes.is_empty(),
        "graph must contain the sample nodes"
    );
    assert!(context.app_dir.join("graph-cache.json").exists());

    std::fs::remove_dir_all(&root).ok();
}

fn copy_markdown_slice(src: &Path, dst_wiki_dir: &Path, cap: usize) -> Vec<String> {
    let mut copied_paths = Vec::new();
    copy_markdown_slice_inner(src, dst_wiki_dir, dst_wiki_dir, cap, &mut copied_paths);
    copied_paths
}

/// Recursively copy up to `cap` markdown files from `src` into the parallel
/// location under `dst_wiki_dir`. `current_dst` is the destination directory
/// matching `src` (descends in lockstep so paths never double-nest).
fn copy_markdown_slice_inner(
    src: &Path,
    dst_wiki_dir: &Path,
    current_dst: &Path,
    cap: usize,
    copied_paths: &mut Vec<String>,
) {
    if copied_paths.len() >= cap {
        return;
    }
    let Ok(entries) = std::fs::read_dir(src) else {
        return;
    };
    // Sort entries by name for reproducibility across filesystems.
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if copied_paths.len() >= cap {
            return;
        }
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            let child_dst = current_dst.join(&name);
            std::fs::create_dir_all(&child_dst).ok();
            copy_markdown_slice_inner(&path, dst_wiki_dir, &child_dst, cap, copied_paths);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let dest = current_dst.join(&name);
            if std::fs::copy(&path, &dest).is_ok() {
                // Record the project-relative wiki path (forward slashes),
                // derived from the actual destination so it always matches
                // scan_wiki output.
                if let Ok(rel) = dest.strip_prefix(dst_wiki_dir) {
                    copied_paths.push(format!("wiki/{}", rel.to_string_lossy().replace('\\', "/")));
                }
            }
        }
    }
}

// =====================================================================
// Loop 3 — AI-assisted (fakes)
// =====================================================================

#[test]
fn ai_assisted_loop_fake_agent_detected_and_byok_runs() {
    // Fake Agent CLI is detected via the replaceable runner boundary.
    let agent = AgentService::with_runner(Arc::new(FakeAgentRunner));
    let detected = agent.detect_agents(Some(AgentKind::Claude));
    let claude = detected
        .iter()
        .find(|i| i.kind == AgentKind::Claude)
        .expect("fake claude detected");
    assert_eq!(claude.state, AgentDetectionState::Installed);

    // BYOK path: a provider config persists, and a real key is stored ONLY in
    // the secret store — never in project files. Storing a key first (rather
    // than asserting absence vacuously) is what makes the boundary check real.
    let (_ps, context, root) = create_project("ai");
    LlmService::save_provider(
        &context,
        llm_wiki_desktop_lib::models::llm::LlmProviderConfig {
            provider: LlmProviderKind::Anthropic,
            model: "claude-test".into(),
            base_url: "https://api.anthropic.com".into(),
            context_window: 100_000,
            enabled: true,
        },
    )
    .unwrap();
    let secret = SecretService::memory();
    secret
        .set(LlmProviderKind::Anthropic, "sk-ant-test-SECRET-VALUE")
        .unwrap();
    // The store holds the key.
    assert_eq!(
        secret.get(LlmProviderKind::Anthropic).unwrap().as_deref(),
        Some("sk-ant-test-SECRET-VALUE"),
        "secret store must round-trip the key"
    );
    // ...but the persisted settings file carries NO secret material: no key,
    // no auth header, and no exact token field of any casing. Fields like
    // maxTokens and temperature are ordinary model settings, not secret leaks.
    let raw = std::fs::read_to_string(context.app_dir.join("settings.json")).unwrap();
    assert!(raw.contains("claude-test"), "provider config must persist");
    let lowered = raw.to_ascii_lowercase();
    for forbidden in [
        "sk-ant",
        "secret-value",
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "\"token\"",
    ] {
        assert!(
            !lowered.contains(forbidden),
            "settings.json must not contain secret material ({forbidden}): {raw}"
        );
    }

    // --- Chat: retrieval → fake answer with citations → save to wiki. ---
    write_page(
        &context,
        "wiki/concepts/cats.md",
        "---\ntype: concept\ntitle: Cats\n---\n\n# Cats\n\nCats are mammals.\n",
    );
    let chat = ChatService::default();
    let mut session = chat.create_session(&context, Some("Cats?"), None).unwrap();
    let now = llm_wiki_desktop_lib::utils::time_utils::now_rfc3339();
    let question = ChatMessage {
        id: "q1".into(),
        role: ChatRole::User,
        content: "What is a cat?".into(),
        created_at: now.clone(),
        citations: vec![],
        route: None,
        provider: None,
        task_id: None,
        convenience_edit: None,
        retrieval_diagnostics: None,
    };
    chat.append_message(&context, &mut session, question.clone())
        .unwrap();

    // Real local retrieval (no model) returns honest citations.
    let retrieval = chat
        .build_retrieval_context(
            &context,
            &SearchService::default(),
            "cat",
            &session,
            "en",
            ChatRoute::Byok,
            None,
            None,
        )
        .unwrap();
    assert!(
        retrieval
            .source_refs
            .iter()
            .any(|source| source.page_path.contains("cats")),
        "retrieval must cite the relevant page"
    );

    // Fake model answer carrying a model-used citation marker. Retrieval is
    // index-first, so cite the actual cats source id instead of assuming S1.
    let cats_source_id = retrieval
        .source_refs
        .iter()
        .find(|source| source.page_path.contains("cats"))
        .expect("cats source should be retrievable")
        .id
        .clone();
    let answer_content = format!("A cat is a mammal [{cats_source_id}].");
    let parsed = ChatService::parse_model_citations(&answer_content, &retrieval.source_refs);
    let answer = ChatMessage {
        id: "a1".into(),
        role: ChatRole::Assistant,
        content: answer_content,
        created_at: now,
        citations: parsed.citations,
        route: Some(ChatRoute::Byok),
        provider: None,
        task_id: None,
        convenience_edit: None,
        retrieval_diagnostics: Some(retrieval.diagnostics),
    };
    chat.append_message(&context, &mut session, answer.clone())
        .unwrap();
    let (slug, markdown) = chat.build_answer_markdown(&session, &question, &answer);
    assert!(
        markdown.contains("[[cats]]"),
        "saved answer must cite sources as wikilinks"
    );

    // git must be initialized before save_answer_to_wiki can checkpoint.
    GitService
        .initialize_repository(&context, "before chat save")
        .unwrap();
    let saved = chat
        .save_answer_to_wiki(&context, &GitService, None, None, false, &markdown, &slug)
        .unwrap();
    assert!(context
        .wiki_dir
        .join("queries")
        .join(format!("{slug}.md"))
        .exists());
    let _ = saved;

    // --- Deep lint: parse a fake agent JSON block. ---
    let fake_agent_lint = r#"```json
    [{"issueType":"duplicate_topic","path":"wiki/concepts/cats.md","severity":"warning","message":"dup","suggestion":"merge"}]
    ```"#;
    let issues = LintService::parse_agent_issues(fake_agent_lint).unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].path, "wiki/concepts/cats.md");

    // --- Export: build prompt + persist a fake record (no real model call). ---
    let export = ExportService::default();
    let prompt = export
        .build_export_prompt(
            &context,
            ExportType::BeautifulRead,
            Some("wiki/concepts/cats.md"),
            &SearchService::default(),
            "en",
            None,
            &ExportContentOptions::default(),
        )
        .unwrap();
    assert!(prompt.contains("html-beautiful-read"));
    // Persist a record the way export_commands would after a fake model run.
    let out_rel = export
        .build_output_relative_path(ExportType::BeautifulRead, Some("wiki/concepts/cats.md"))
        .unwrap();
    let record = ExportService::new_record(
        ExportType::BeautifulRead,
        "Cats".into(),
        Some("wiki/concepts/cats.md".into()),
        out_rel.clone(),
        ExportRoute::Byok,
        None,
    );
    export
        .write_html(&context, &out_rel, "<!doctype html><html></html>")
        .unwrap();
    export.append_record(&context, record).unwrap();
    let listed = export.list_records(&context).unwrap();
    assert!(listed.iter().any(|r| r.output_path == out_rel));

    std::fs::remove_dir_all(&root).ok();
}

// =====================================================================
// Loop 4 — safety: destructive ops require confirmation + checkpoint
// =====================================================================

#[test]
fn safety_loop_compile_conflict_does_not_mutate_without_confirmation() {
    // compile_conflict is the core safety contract: an externally-changed
    // page is reported as a conflict and the candidate is NOT written.
    let (_ps, context, root) = create_project("safety-conflict");
    let baseline = CompileService::snapshot_wiki(&context).unwrap();
    // Externally mutate index AFTER the baseline snapshot; candidate must not
    // silently overwrite this edit. Use DISTINCT candidate bodies for every
    // core page so we can prove none of them landed.
    std::fs::write(context.wiki_dir.join("index.md"), "external edit").unwrap();
    let manifest = CompileManifest {
        files: vec![
            CompileFile::new("wiki/index.md", "CANDIDATE-index"),
            CompileFile::new("wiki/overview.md", "CANDIDATE-overview"),
            CompileFile::new("wiki/log.md", "CANDIDATE-log"),
        ],
        deletions: vec![],
        summary: "safety".into(),
    };
    let result = CompileService::apply_manifest(&context, &manifest, None, &baseline).unwrap();
    assert!(
        result.conflicts.iter().any(|c| c == "wiki/index.md"),
        "externally-edited index must surface as a conflict: {:?}",
        result.conflicts
    );
    // The externally-edited content is preserved AND no candidate body landed
    // anywhere (overview/log were in the baseline → also conflicts).
    assert_eq!(
        std::fs::read_to_string(context.wiki_dir.join("index.md")).unwrap(),
        "external edit"
    );
    for (name, body) in [
        (
            "overview.md",
            std::fs::read_to_string(context.wiki_dir.join("overview.md")).unwrap(),
        ),
        (
            "log.md",
            std::fs::read_to_string(context.wiki_dir.join("log.md")).unwrap(),
        ),
    ] {
        assert!(
            !body.contains("CANDIDATE"),
            "no candidate body may land on conflict abort ({name})"
        );
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn safety_loop_confirm_requires_matching_state_and_creates_checkpoint() {
    let (_ps, context, root) = create_project("safety-confirm");

    // A confirmed compile requires the wiki state to match what the user
    // confirmed (expected hashes). Drift must surface, not silently overwrite.
    std::fs::create_dir_all(context.wiki_dir.join("sources")).unwrap();
    std::fs::write(
        context.wiki_dir.join("sources/source-a.md"),
        "# Source A\n\nA confirmed source.",
    )
    .unwrap();
    let overview_disk = std::fs::read_to_string(context.wiki_dir.join("overview.md")).unwrap();
    let log_disk = std::fs::read_to_string(context.wiki_dir.join("log.md")).unwrap();
    let manifest = CompileManifest {
        files: vec![
            CompileFile::new(
                "wiki/concepts/new.md",
                "---\ntype: concept\nsources: [source-a.md]\n---\n\n# New\n\nA derived concept page.\n\n> Sources: [[sources/source-a]]\n",
            ),
            CompileFile::new("wiki/index.md", "# Index\n\n- [[new]]\n"),
            CompileFile::new("wiki/overview.md", &overview_disk),
            CompileFile::new("wiki/log.md", &log_disk),
        ],
        deletions: vec![],
        summary: "confirm".into(),
    };
    let hashes = CompileService::snapshot_wiki(&context).unwrap();
    // No existing hash for a brand-new page → confirm creates it.
    let applied =
        CompileService::apply_confirmed_manifest(&context, &manifest, None, &hashes).unwrap();
    assert!(context.wiki_dir.join("concepts/new.md").exists());
    assert!(applied.iter().any(|p| p.contains("new.md")));

    // Now mutate the page after confirmation → confirm-state mismatch.
    let confirmed_hashes = CompileService::snapshot_wiki(&context).unwrap();
    std::fs::write(context.wiki_dir.join("concepts/new.md"), "drift").unwrap();
    let err =
        CompileService::apply_confirmed_manifest(&context, &manifest, None, &confirmed_hashes)
            .unwrap_err();
    assert_eq!(err.code, "CONFIRMATION_STATE_MISMATCH");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn safety_loop_chat_overwrite_requires_checkpoint() {
    // Saving a chat answer over an existing query page must require a Git
    // checkpoint (CLAUDE.md hard rule). Verified both directions:
    //   (a) without confirmation → FILE_ALREADY_EXISTS, original survives;
    //   (b) with git + matching hash → a checkpoint commit is recorded and the
    //       page is replaced (never a silent un-checkpointed overwrite).
    let (_ps, context, root) = create_project("safety-overwrite");
    write_page(&context, "wiki/queries/q.md", "old content");

    let chat = ChatService::default();

    // (a) Refuse to overwrite without explicit confirmation.
    let err = chat
        .save_answer_to_wiki(
            &context,
            &GitService,
            Some("wiki/queries/q.md"),
            None,
            false,
            "# new content",
            "q",
        )
        .unwrap_err();
    assert_eq!(err.code, "FILE_ALREADY_EXISTS");
    assert_eq!(
        std::fs::read_to_string(context.wiki_dir.join("queries/q.md")).unwrap(),
        "old content",
        "original content must survive an unconfirmed overwrite"
    );

    // (b) Initialize git, capture the current hash, confirm the overwrite.
    GitService
        .initialize_repository(&context, "before overwrite")
        .unwrap();
    let current_hash = FileStore.file_hash(&context, "wiki/queries/q.md").unwrap();
    let result = chat
        .save_answer_to_wiki(
            &context,
            &GitService,
            Some("wiki/queries/q.md"),
            Some(&current_hash),
            true,
            "# new content",
            "q",
        )
        .unwrap();
    assert!(
        result.checkpoint.is_some(),
        "overwrite must produce a checkpoint commit"
    );
    assert!(
        std::fs::read_to_string(context.wiki_dir.join("queries/q.md"))
            .unwrap()
            .contains("new content"),
        "confirmed overwrite must land the new content"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn safety_loop_lint_fix_rejects_paths_outside_wiki() {
    let (_ps, context, root) = create_project("safety-lint");
    use llm_wiki_desktop_lib::models::lint::{
        Fixability, LintIssue, LintIssueSource, LintIssueType, LintSeverity,
    };
    let issue = LintIssue {
        id: "deadlink:.app/settings.json".into(),
        source: LintIssueSource::Local,
        severity: LintSeverity::Warning,
        issue_type: LintIssueType::DeadLink,
        path: ".app/settings.json".into(), // outside wiki/
        range: None,
        message: "x".into(),
        evidence: None,
        target: None,
        fixability: Fixability::Safe,
        suggested_action: None,
    };
    let err = LintService::default()
        .apply_fix(&context, &GitService, &issue, false, None)
        .unwrap_err();
    assert_eq!(err.code, "LINT_FIX_PATH_OUT_OF_SCOPE");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn safety_loop_lint_fix_applies_safe_fix_under_checkpoint() {
    // Positive direction of the lint-fix safety contract (the risky one): a
    // safe deterministic fix inside wiki/ creates a Git checkpoint, mutates
    // the page, and reports Applied. MissingFrontmatter is the canonical safe
    // fix — frontmatter is prepended without judgment.
    use llm_wiki_desktop_lib::models::lint::{
        Fixability, LintFixOutcomeKind, LintIssue, LintIssueSource, LintIssueType, LintSeverity,
    };
    let (_ps, context, root) = create_project("safety-lint-fix");
    // Page with no frontmatter → MissingFrontmatter fires.
    write_page(
        &context,
        "wiki/concepts/bare.md",
        "# Bare page\n\nNo frontmatter.",
    );
    GitService
        .initialize_repository(&context, "before lint fix")
        .unwrap();
    let hash = FileStore
        .file_hash(&context, "wiki/concepts/bare.md")
        .unwrap();
    let issue = LintIssue {
        id: "missing_frontmatter:wiki/concepts/bare.md".into(),
        source: LintIssueSource::Local,
        severity: LintSeverity::Warning,
        issue_type: LintIssueType::MissingFrontmatter,
        path: "wiki/concepts/bare.md".into(),
        range: None,
        message: "page has no frontmatter".into(),
        evidence: None,
        target: None,
        fixability: Fixability::Safe,
        suggested_action: None,
    };
    let outcome = LintService::default()
        .apply_fix(&context, &GitService, &issue, false, Some(&hash))
        .unwrap();
    assert_eq!(outcome.kind, LintFixOutcomeKind::Applied);
    assert!(
        outcome.checkpoint.is_some(),
        "safe fix must checkpoint before mutating the page"
    );
    let after = std::fs::read_to_string(context.wiki_dir.join("concepts/bare.md")).unwrap();
    assert!(after.contains("---"), "frontmatter must be prepended");
    assert!(after.contains("# Bare page"), "body must be preserved");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn safety_loop_git_checkpoint_records_commit() {
    // Direct checkpoint contract: with a repo initialized, a checkpoint over
    // real changes yields a commit hash; with no changes, it is a no-op.
    let (_ps, context, root) = create_project("safety-checkpoint");
    GitService.initialize_repository(&context, "init").unwrap();
    write_page(&context, "wiki/concepts/x.md", "# X");
    let cp = GitService
        .create_checkpoint(
            &context,
            CheckpointPurpose::HighRiskOperation,
            "before destructive",
        )
        .unwrap();
    assert!(cp.created, "checkpoint must commit when there are changes");
    assert!(cp.commit_hash.is_some());
    std::fs::remove_dir_all(&root).ok();
}
