use llm_wiki_desktop_lib::{
    models::{
        agent::AgentKind,
        import_v2::{
            AttemptOutcome, AttemptRecord, CommitImportSessionRequest, CommitItemDecision,
            ImportInput, ImportInputKind, ImportItem, ImportItemResolution, ImportItemStatus,
            ImportResolutionKind, ImportResourceMode, ImportSession, ImportStage,
        },
        import_v2_agent::{
            AgentAssistanceTrigger, AgentAuditRecord, AgentCandidateManifest, AgentToolGrant,
        },
        paths::ProjectContext,
        task::{TaskResult, TaskResultReference, TaskStatus, TaskType},
    },
    services::{
        import_v2::{
            agent_candidate::AgentCandidateService,
            agent_workspace::AgentTaskBundle,
            source_registry::{SourceIndex, SourcePointer},
            ImportV2Service, SessionStore,
        },
        FileStore, GitService,
    },
    tasks::TaskService,
};
use sha2::{Digest, Sha256};

#[test]
fn agent_candidate_contract_requires_declared_provenance() {
    let manifest: Result<AgentCandidateManifest, _> = serde_json::from_value(serde_json::json!({
        "markdownPath": "candidate.md",
        "assetPaths": [],
        "markdownSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "assetSha256": {},
        "processingSummary": "AI-assisted extraction",
        "toolsUsed": ["tool-free-local-agent"],
        "uncertainties": ["Formatting may differ from the source."],
        "warnings": ["Review before selection."]
    }));
    assert!(manifest.is_ok());
    let _service_type = std::any::type_name::<AgentCandidateService<'static>>();
}

#[test]
fn agent_candidate_manifest_rejects_unknown_fields() {
    let manifest: Result<AgentCandidateManifest, _> = serde_json::from_value(serde_json::json!({
        "markdownPath": "candidate.md",
        "assetPaths": [],
        "markdownSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "assetSha256": {},
        "processingSummary": "AI-assisted extraction",
        "toolsUsed": ["unknown-network-model"],
        "uncertainties": ["None stated."],
        "warnings": ["Review before selection."],
        "command": "git status"
    }));
    assert!(manifest.is_err());
}

#[test]
fn accepts_staged_candidate_with_exact_hashes_and_preserves_baseline() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("project-a", root.path().to_path_buf());
    let files = FileStore;
    let imports = ImportV2Service::default();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::AgentRun,
            "project-a".into(),
            root.path().to_path_buf(),
            "Agent candidate".into(),
            true,
        )
        .unwrap();
    let mut item = ImportItem::queued(
        "item-a",
        ImportInput {
            kind: ImportInputKind::Url,
            display_name: "Example".into(),
            locator: "https://example.com".into(),
            normalized_locator: Some("https://example.com/".into()),
            source_identity: None,
            media_save_mode: Default::default(),
        },
    );
    item.status = ImportItemStatus::Failed;
    item.task_id = Some(task.id.clone());
    item.attempts.push(AttemptRecord {
        route: format!("agent_assistance/{}", task.id),
        engine_id: "claude".into(),
        engine_version: "test".into(),
        stage: ImportStage::Extract,
        started_at: chrono::Utc::now().to_rfc3339(),
        completed_at: Some(chrono::Utc::now().to_rfc3339()),
        outcome: AttemptOutcome::Succeeded,
        error_code: None,
        warnings: Vec::new(),
    });
    let mut session = ImportSession::new("session-a", "project-a", ImportResourceMode::Balanced);
    session.items.push(item);
    SessionStore::default()
        .save(&context, &files, &session)
        .unwrap();
    let baseline = "# Committed baseline\n\nOriginal.\n";
    let current = "# User-edited Wiki\n\nKeep this edit.\n";
    let baseline_hash = format!("{:x}", Sha256::digest(baseline.as_bytes()));
    let baseline_path = ".app/source-artifacts/source-old/version-old/baseline.md";
    let wiki_path = "wiki/sources/web/example.md";
    std::fs::create_dir_all(
        root.path()
            .join(".app/source-artifacts/source-old/version-old"),
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("wiki/sources/web")).unwrap();
    std::fs::create_dir_all(root.path().join(".app/sources")).unwrap();
    std::fs::create_dir_all(root.path().join("raw/sources/source-old/version-old")).unwrap();
    std::fs::write(root.path().join(baseline_path), baseline).unwrap();
    std::fs::write(root.path().join(wiki_path), current).unwrap();
    let old_source = b"old source";
    let old_content_hash = format!("{:x}", Sha256::digest(old_source));
    std::fs::write(
        root.path()
            .join("raw/sources/source-old/version-old/original.bin"),
        old_source,
    )
    .unwrap();
    let pointer = SourcePointer {
        source_id: "source-old".into(),
        version_id: "version-old".into(),
    };
    let mut index = SourceIndex::default_v2();
    index
        .by_content_hash
        .insert(old_content_hash.clone(), pointer.clone());
    index
        .by_locator
        .insert("https://example.com/".into(), pointer);
    files
        .write_json_atomic(&context, ".app/source-index-v2.json", &index)
        .unwrap();
    files
        .write_json_atomic(
            &context,
            ".app/sources/source-old.json",
            &serde_json::json!({
                "schemaVersion": 3,
                "sourceId": "source-old",
                "sourceKind": "web_page",
                "currentVersionId": "version-old",
                "wikiPath": wiki_path,
                "origins": ["https://example.com/"],
                "canonicalUrl": "https://example.com/",
                "title": "Example",
                "importedAt": chrono::Utc::now().to_rfc3339(),
                "versions": [{
                    "versionId": "version-old",
                    "contentHash": old_content_hash,
                    "rawEvidence": [{
                        "path": "raw/sources/source-old/version-old/original.bin",
                        "sha256": old_content_hash,
                        "sizeBytes": old_source.len(),
                        "kind": "source_snapshot"
                    }],
                    "assets": [],
                    "baselinePath": baseline_path,
                    "candidate": {
                        "markdownHash": baseline_hash,
                        "title": "Example",
                        "sourceKind": "web_page",
                        "canonicalUrl": "https://example.com/"
                    },
                    "provenance": {
                        "locator": "https://example.com/",
                        "route": "generic_web",
                        "engineId": "deterministic",
                        "engineVersion": "1"
                    },
                    "createdAt": chrono::Utc::now().to_rfc3339(),
                    "humanEditHash": baseline_hash,
                    "quality": {
                        "level": "pass",
                        "metrics": [],
                        "warnings": []
                    }
                }]
            }),
        )
        .unwrap();

    let relative_workspace =
        format!(".app/import-sessions/session-a/items/item-a/staging/agent/workspace-a");
    let workspace = root.path().join(&relative_workspace);
    for directory in ["source", "deterministic", "logs", "output"] {
        std::fs::create_dir_all(workspace.join(directory)).unwrap();
    }
    let source = b"untrusted source snapshot";
    let source_hash = format!("{:x}", Sha256::digest(source));
    std::fs::write(workspace.join("source/source.bin"), source).unwrap();
    let agent = "# Agent candidate\n\nImproved structure.\n";
    std::fs::write(workspace.join("output/candidate.md"), agent).unwrap();
    let asset = b"agent asset notes";
    std::fs::create_dir_all(workspace.join("output/assets")).unwrap();
    std::fs::write(workspace.join("output/assets/notes.txt"), asset).unwrap();
    let manifest = AgentCandidateManifest {
        markdown_path: "candidate.md".into(),
        asset_paths: vec!["assets/notes.txt".into()],
        markdown_sha256: format!("{:x}", Sha256::digest(agent.as_bytes())),
        asset_sha256: [(
            "assets/notes.txt".into(),
            format!("{:x}", Sha256::digest(asset)),
        )]
        .into_iter()
        .collect(),
        processing_summary: "AI-assisted extraction".into(),
        tools_used: vec!["tool-free-local-agent".into()],
        uncertainties: vec!["Formatting may differ.".into()],
        warnings: vec!["Review the Diff.".into()],
    };
    std::fs::write(
        workspace.join("output/manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let bundle = AgentTaskBundle {
        schema_version: 1,
        session_id: "session-a".into(),
        item_id: "item-a".into(),
        trigger: AgentAssistanceTrigger::Manual,
        public_source: "Example".into(),
        input_hashes: vec![source_hash.clone()],
        allowed_tools: vec![AgentToolGrant::ValidateCandidate],
        required_outputs: vec![
            "output/manifest.json".into(),
            "output/candidate.md".into(),
            "output/assets/notes.txt".into(),
        ],
        untrusted_source_material: vec!["source/source.bin".into()],
    };
    std::fs::write(
        workspace.join("task.json"),
        serde_json::to_vec_pretty(&bundle).unwrap(),
    )
    .unwrap();
    tasks
        .transition_status(&task.id, TaskStatus::Running)
        .unwrap();
    tasks
        .complete_running_with_result(
            &task.id,
            TaskResult {
                summary: "staged".into(),
                affected_paths: vec![format!("{relative_workspace}/output")],
                reference: Some(TaskResultReference::ImportPreview {
                    session_id: "session-a".into(),
                    item_id: "item-a".into(),
                }),
                pending_action: None,
            },
        )
        .unwrap();
    files
        .write_json_atomic(
            &context,
            &format!(
                ".app/import-sessions/session-a/items/item-a/agent-audit/{}.json",
                task.id
            ),
            &AgentAuditRecord {
                audit_id: "audit-a".into(),
                task_id: task.id.clone(),
                session_id: "session-a".into(),
                item_id: "item-a".into(),
                trigger: AgentAssistanceTrigger::Manual,
                route: "local/claude".into(),
                agent_kind: Some(AgentKind::Claude),
                agent_version: "test-version".into(),
                prompt_template_version: "import-recovery/local-v1".into(),
                approved_cost_micros: None,
                tool_calls: vec![],
                approved_scope_sha256: None,
                workspace_relative_path: relative_workspace.clone(),
                granted_tools: vec![AgentToolGrant::ValidateCandidate],
                input_hashes: vec![source_hash.clone()],
                output_hashes: vec![format!("{:x}", Sha256::digest(agent.as_bytes()))],
                started_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                outcome: "output_staged".into(),
                warnings: vec![],
            },
        )
        .unwrap();

    let service = AgentCandidateService::new(&imports, &files, &tasks);
    let candidate = service
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .unwrap();
    assert_eq!(candidate.source_snapshot_sha256, source_hash);
    assert_eq!(candidate.audit_id, "audit-a");
    assert_eq!(candidate.agent_kind, Some(AgentKind::Claude));
    assert_eq!(candidate.agent_version, "test-version");
    assert_eq!(
        candidate.prompt_template_version,
        "import-recovery/local-v1"
    );
    let reconciled_audit: AgentAuditRecord = files
        .read_json(
            &context,
            &format!(
                ".app/import-sessions/session-a/items/item-a/agent-audit/{}.json",
                task.id
            ),
        )
        .unwrap();
    assert_eq!(reconciled_audit.outcome, "succeeded");
    assert_eq!(
        candidate.markdown.sha256,
        format!("{:x}", Sha256::digest(agent.as_bytes()))
    );
    let (_, diff) = service
        .load_candidate(&context, "session-a", "item-a", &candidate.candidate_id)
        .unwrap();
    assert_eq!(diff.baseline_markdown, baseline);
    assert_eq!(diff.current_markdown.as_deref(), Some(current));
    let current_hash = format!("{:x}", Sha256::digest(current.as_bytes()));
    assert_eq!(
        diff.current_markdown_sha256.as_deref(),
        Some(current_hash.as_str())
    );
    assert!(diff.needs_three_way_merge);
    assert_eq!(diff.agent_markdown, agent);
    assert!(diff.unified_diff.contains("Agent candidate"));
    assert!(candidate
        .quality
        .warnings
        .iter()
        .any(|warning| warning == "AGENT_QUALITY_NOT_MEASURED"));
    assert!(candidate.quality.metrics.is_empty());
    assert_eq!(
        imports
            .load_session(&context, &files, "session-a")
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::NeedsMerge
    );
    assert!(!workspace.join("deterministic/candidate.md").exists());
    assert_eq!(
        service
            .accept_staged_output(&context, "session-a", "item-a", &task.id)
            .unwrap()
            .candidate_id,
        candidate.candidate_id
    );

    imports
        .fail_agent_candidate_validation(
            &context,
            &files,
            "session-a",
            "item-a",
            &task.id,
            ImportItemStatus::Failed,
        )
        .unwrap();
    assert_eq!(
        service
            .recover_completed_outputs(&context, "session-a")
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::NeedsMerge
    );
    let candidate_record = root.path().join(format!(
        ".app/import-sessions/session-a/items/item-a/staging/agent-candidates/{}/candidate.json",
        candidate.candidate_id
    ));
    std::fs::remove_file(&candidate_record).unwrap();
    imports
        .fail_agent_candidate_validation(
            &context,
            &files,
            "session-a",
            "item-a",
            &task.id,
            ImportItemStatus::Failed,
        )
        .unwrap();
    assert_eq!(
        service
            .recover_completed_outputs(&context, "session-a")
            .unwrap()
            .items[0]
            .status,
        ImportItemStatus::NeedsMerge
    );
    assert!(candidate_record.is_file());

    let candidate_root = candidate_record.parent().unwrap();
    for (relative, expected) in [
        ("source.bin", source.as_slice()),
        ("candidate.md", agent.as_bytes()),
        ("assets/notes.txt", asset.as_slice()),
    ] {
        std::fs::remove_file(&candidate_record).unwrap();
        std::fs::write(candidate_root.join(relative), b"truncated").unwrap();
        imports
            .fail_agent_candidate_validation(
                &context,
                &files,
                "session-a",
                "item-a",
                &task.id,
                ImportItemStatus::Failed,
            )
            .unwrap();
        for _ in 0..2 {
            assert_eq!(
                service
                    .recover_completed_outputs(&context, "session-a")
                    .unwrap()
                    .items[0]
                    .status,
                ImportItemStatus::NeedsMerge
            );
        }
        assert_eq!(
            std::fs::read(candidate_root.join(relative)).unwrap(),
            expected
        );
        assert!(candidate_record.is_file());
        assert!(!imports
            .load_session(&context, &files, "session-a")
            .unwrap()
            .items[0]
            .attempts
            .iter()
            .flat_map(|attempt| attempt.warnings.iter())
            .any(|warning| warning == "AGENT_CANDIDATE_REJECTED"));
    }

    std::fs::write(workspace.join("output/payload.exe"), b"MZ").unwrap();
    assert!(service
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .is_err());
    std::fs::remove_file(workspace.join("output/payload.exe")).unwrap();

    std::fs::write(workspace.join("source/source.bin"), b"changed source").unwrap();
    assert!(service
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .is_err());
    std::fs::write(workspace.join("source/source.bin"), source).unwrap();

    let unsafe_markdown = b"# Candidate\n\n<script>alert(1)</script>";
    std::fs::write(workspace.join("output/candidate.md"), unsafe_markdown).unwrap();
    let mut unsafe_manifest = manifest.clone();
    unsafe_manifest.markdown_sha256 = format!("{:x}", Sha256::digest(unsafe_markdown));
    std::fs::write(
        workspace.join("output/manifest.json"),
        serde_json::to_vec_pretty(&unsafe_manifest).unwrap(),
    )
    .unwrap();
    assert!(service
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .is_err());

    std::fs::remove_file(workspace.join("output/manifest.json")).unwrap();
    assert!(service
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .is_err());
    let recovered = service
        .recover_completed_outputs(&context, "session-a")
        .unwrap();
    assert_eq!(recovered.items[0].status, ImportItemStatus::NeedsMerge);
    assert_eq!(
        recovered.items[0].attempts[0]
            .warnings
            .iter()
            .filter(|warning| warning.as_str() == "AGENT_CANDIDATE_REJECTED")
            .count(),
        1
    );

    std::fs::write(workspace.join("output/candidate.md"), agent).unwrap();
    std::fs::write(
        workspace.join("output/manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let candidate = service
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .unwrap();
    let current_hash = format!("{:x}", Sha256::digest(current.as_bytes()));
    let stale = service
        .select_candidate(
            &context,
            "session-a",
            "item-a",
            &candidate.candidate_id,
            Some("# Explicit merge\n"),
            Some(&"0".repeat(64)),
        )
        .unwrap_err();
    assert_eq!(stale.code, "IMPORT_AGENT_MERGE_STALE");
    let selected = service
        .select_candidate(
            &context,
            "session-a",
            "item-a",
            &candidate.candidate_id,
            Some("# Explicit merge\n"),
            Some(&current_hash),
        )
        .unwrap();
    let sibling_workspace = root
        .path()
        .join(".app/import-sessions/session-a/items/item-a/staging/agent/new-workspace");
    std::fs::create_dir_all(&sibling_workspace).unwrap();
    std::fs::write(sibling_workspace.join("keep.txt"), b"new task").unwrap();
    assert_eq!(selected.status, ImportItemStatus::PreviewReady);
    assert!(selected
        .preview
        .as_ref()
        .unwrap()
        .markdown
        .relative_path
        .contains("merged-"));
    let selected_resolution = selected
        .preview
        .as_ref()
        .and_then(|preview| preview.resolution.as_ref())
        .expect("selected Agent candidate resolution");
    assert_eq!(
        selected_resolution.kind,
        ImportResolutionKind::NeedsThreeWayMerge
    );
    assert_eq!(
        selected_resolution.target_wiki_path.as_deref(),
        Some(wiki_path)
    );
    let selected_binding = selected_resolution
        .binding
        .as_ref()
        .expect("selected Agent candidate binding");
    assert_eq!(selected_binding.source_id, "source-old");
    assert_eq!(selected_binding.target_version_id, "version-old");
    assert_eq!(selected_binding.current_hash, current_hash);
    assert!(matches!(
        selected_resolution.default_resolution.as_ref(),
        Some(ImportItemResolution::ApplyImportCandidate { .. })
    ));
    assert_eq!(
        std::fs::read_to_string(root.path().join(wiki_path)).unwrap(),
        current
    );
    let discarded = service
        .discard_candidate(&context, "session-a", "item-a", &candidate.candidate_id)
        .unwrap();
    assert_eq!(discarded.status, ImportItemStatus::Failed);
    assert!(discarded.preview.is_none());
    assert!(!workspace.exists());
    assert!(sibling_workspace.join("keep.txt").is_file());
    assert_eq!(
        std::fs::read_to_string(root.path().join(wiki_path)).unwrap(),
        current
    );

    for directory in ["source", "deterministic", "logs", "output/assets"] {
        std::fs::create_dir_all(workspace.join(directory)).unwrap();
    }
    std::fs::write(workspace.join("source/source.bin"), source).unwrap();
    std::fs::write(workspace.join("output/candidate.md"), agent).unwrap();
    std::fs::write(workspace.join("output/assets/notes.txt"), asset).unwrap();
    std::fs::write(
        workspace.join("output/manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(
        workspace.join("task.json"),
        serde_json::to_vec_pretty(&bundle).unwrap(),
    )
    .unwrap();
    let candidate = service
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .unwrap();
    service
        .select_candidate(
            &context,
            "session-a",
            "item-a",
            &candidate.candidate_id,
            Some("# Explicit merge\n"),
            Some(&current_hash),
        )
        .unwrap();
    assert!(!workspace.exists());
    let selected_session = imports.load_session(&context, &files, "session-a").unwrap();
    let selected_item = &selected_session.items[0];
    let preview = selected_item.preview.as_ref().unwrap();
    let default_resolution = preview
        .resolution
        .as_ref()
        .and_then(|resolution| resolution.default_resolution.clone())
        .expect("ordinary Import decisions can commit the selected Agent merge");
    assert!(matches!(
        &default_resolution,
        ImportItemResolution::ApplyImportCandidate { .. }
    ));
    let git = GitService;
    git.initialize_repository(&context, "Initial fixture")
        .unwrap();
    let committed = imports
        .commit_items(
            &context,
            &files,
            &git,
            &CommitImportSessionRequest {
                project_id: "project-a".into(),
                project_root_path: root.path().to_string_lossy().into(),
                session_id: "session-a".into(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                decisions: vec![CommitItemDecision {
                    item_id: "item-a".into(),
                    resolution: Some(default_resolution),
                }],
            },
        )
        .unwrap();
    assert_eq!(committed.committed_count, 1, "{committed:?}");
    assert_eq!(committed.items[0].wiki_path.as_deref(), Some(wiki_path));
    let final_source = std::fs::read_to_string(root.path().join(wiki_path)).unwrap();
    assert!(final_source.contains("sourceId: \"source-old\""));
    assert!(final_source.contains("# Explicit merge"));
    let migrated: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join(".app/sources/source-old.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(migrated["schemaVersion"], 3);
    assert!(migrated["versions"][0]["candidate"].is_object());
    assert!(migrated["timeline"].is_array());
}

#[test]
fn cancelled_or_wrong_type_task_cannot_accept_output() {
    let root = tempfile::tempdir().unwrap();
    let context = ProjectContext::new("project-a", root.path().to_path_buf());
    let files = FileStore;
    let imports = ImportV2Service::default();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            "project-a".into(),
            root.path().to_path_buf(),
            "not an Agent task".into(),
            true,
        )
        .unwrap();
    tasks.cancel_task(&task.id).unwrap();
    let service = AgentCandidateService::new(&imports, &files, &tasks);
    assert!(service
        .accept_staged_output(&context, "session-a", "item-a", &task.id)
        .is_err());
}
