use super::*;
use crate::models::import_v2::{QualityReport, SourcePageType};
use crate::models::source::{
    ApplySourceCandidateRequest, DeleteSourceRequest, MoveSourceRequest,
    PreviewDeleteSourceRequest, PreviewMoveSourceRequest, ReprocessSourceRequest,
    SourceAiOrganizeCandidateMeta, SourceAiOrganizeRoute,
};
use crate::models::source_package::{SourcePackageMember, SOURCE_PACKAGE_SCHEMA_VERSION};
use crate::models::wiki::{WikiPageContent, WikiPageMeta, WikiTreeNodeKind};
use crate::services::import_v2::engine::{
    EngineDescriptor, EngineRequest, EngineResult, ImportEngine,
};
use crate::services::import_v2::source_registry::SOURCE_REGISTRY_SCHEMA_VERSION;
use crate::services::import_v2::transaction::set_fail_next_candidate_install;
use crate::services::SecretService;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

struct Fixture {
    context: ProjectContext,
    root: PathBuf,
    source_id: String,
    version_id: String,
    wiki_path: String,
    child_path: String,
    markdown: String,
    child_markdown: String,
    manifest_path: String,
}

struct ReprocessFixtureEngine;

impl ImportEngine for ReprocessFixtureEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "fixture.source-reprocess".into(),
            engine_version: "1".into(),
            route: "media.asr".into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::File
    }

    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        assert!(!cancellation.is_cancelled());
        let staging = Path::new(&request.project_root).join(&request.staging_root);
        fs::create_dir_all(&staging).unwrap();
        write(
            Path::new(&request.project_root),
            &format!("{}/document.md", request.staging_root),
            "# 新转录\n\n这是实际引擎重新处理后的文本。\n",
        );
        write(
            Path::new(&request.project_root),
            &format!("{}/transcript.json", request.staging_root),
            r#"{"segments":[{"text":"这是实际引擎重新处理后的文本。"}]}"#,
        );
        Ok(EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "document.md".into(),
            asset_paths: vec!["transcript.json".into()],
            metadata_path: None,
            title: "新转录".into(),
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

struct RefreshFixtureEngine;

impl ImportEngine for RefreshFixtureEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "fixture.source-refresh".into(),
            engine_version: "1".into(),
            route: "fixture.refresh".into(),
        }
    }

    fn supports(&self, input: &ImportInput) -> bool {
        input.kind == ImportInputKind::Url
    }

    fn execute(
        &self,
        request: &EngineRequest,
        cancellation: &CancellationToken,
    ) -> Result<EngineResult, BackendError> {
        assert!(!cancellation.is_cancelled());
        write(
            Path::new(&request.project_root),
            &format!("{}/document.md", request.staging_root),
            "# 刷新结果\n\n远端内容已重新获取。\n",
        );
        Ok(EngineResult {
            source_snapshot_path: "snapshot.html".into(),
            markdown_path: "document.md".into(),
            asset_paths: Vec::new(),
            metadata_path: None,
            title: "刷新结果".into(),
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

fn quality() -> QualityReport {
    serde_json::from_value(serde_json::json!({
        "level": "pass",
        "metrics": [],
        "warnings": []
    }))
    .unwrap()
}

fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn package_fixture(suffix: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "llm-wiki-source-lifecycle-{suffix}-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).unwrap();
    let context = ProjectContext::new(format!("project-{suffix}"), root.clone());
    let source_id = "source-package".to_string();
    let version_id = "version-1".to_string();
    let title = "数据集";
    let wiki_path = "wiki/sources/local/数据集/index.md".to_string();
    let child_path = "wiki/sources/local/数据集/表一.md".to_string();
    let body = "# 数据集\n\n忠实原稿。\n";
    let body_hash = digest(body.as_bytes());
    let imported_at = "2026-07-27T00:00:00Z".to_string();
    let frontmatter = SourceFrontmatter {
        page_type: SourcePageType::Source,
        source_id: source_id.clone(),
        version_id: version_id.clone(),
        source_kind: "spreadsheet".into(),
        title: title.into(),
        imported_at: imported_at.clone(),
        content_hash: body_hash.clone(),
        platform: None,
        canonical_url: None,
        platform_content_id: None,
        author: None,
        published_at: None,
        language: Some("zh-CN".into()),
        quality: quality(),
        restricted: false,
    };
    let markdown = render_source_markdown(&frontmatter, body).unwrap();
    let markdown_hash = digest(markdown.as_bytes());
    let child_markdown = "# 表一\n\n| 名称 | 数量 |\n|---|---:|\n| 示例 | 1 |\n".to_string();
    let child_hash = digest(child_markdown.as_bytes());
    let baseline_path = format!(".app/source-artifacts/{source_id}/{version_id}/baseline.md");
    let child_baseline = format!(".app/source-artifacts/{source_id}/{version_id}/package/表一.md");
    let transcript_path = format!("raw/sources/{source_id}/{version_id}/transcripts/original.md");
    let transcript = "# 新转录\n\n这是重新处理后的文本。\n";
    let snapshot_path = format!("raw/sources/{source_id}/{version_id}/original/interview.wav");
    let snapshot = b"fixture-media";
    let asset_path = format!("raw/assets/{source_id}/{version_id}/cover.png");
    write(&root, &wiki_path, markdown.as_bytes());
    write(&root, &child_path, child_markdown.as_bytes());
    write(&root, &baseline_path, markdown.as_bytes());
    write(&root, &child_baseline, child_markdown.as_bytes());
    write(&root, &transcript_path, transcript.as_bytes());
    write(&root, &snapshot_path, snapshot);
    write(&root, &asset_path, b"retained-image");

    let package = SourcePackageManifest {
        schema_version: SOURCE_PACKAGE_SCHEMA_VERSION,
        source_id: source_id.clone(),
        version_id: version_id.clone(),
        entry_wiki_path: wiki_path.clone(),
        members: vec![
            SourcePackageMember {
                order: 0,
                role: SourcePackageMemberRole::Index,
                title: title.into(),
                staging_path: "package/index.md".into(),
                wiki_path: wiki_path.clone(),
                baseline_path: baseline_path.clone(),
                content_hash: markdown_hash.clone(),
                human_edit_hash: markdown_hash.clone(),
            },
            SourcePackageMember {
                order: 1,
                role: SourcePackageMemberRole::Sheet,
                title: "表一".into(),
                staging_path: "package/表一.md".into(),
                wiki_path: child_path.clone(),
                baseline_path: child_baseline,
                content_hash: child_hash.clone(),
                human_edit_hash: child_hash,
            },
        ],
    };
    package.validate_committed().unwrap();
    let package_bytes = pretty_json(&package).unwrap();
    let package_path = format!("raw/sources/{source_id}/{version_id}/derived/source-package.json");
    write(&root, &package_path, &package_bytes);

    let version = SourceVersion {
        version_id: version_id.clone(),
        content_hash: body_hash.clone(),
        raw_evidence: vec![
            SourceArtifactRecord {
                path: snapshot_path,
                sha256: digest(snapshot),
                size_bytes: snapshot.len() as u64,
                kind: "source_snapshot".into(),
            },
            SourceArtifactRecord {
                path: transcript_path,
                sha256: digest(transcript.as_bytes()),
                size_bytes: transcript.len() as u64,
                kind: "transcript".into(),
            },
            SourceArtifactRecord {
                path: package_path,
                sha256: digest(&package_bytes),
                size_bytes: package_bytes.len() as u64,
                kind: "source_package_manifest".into(),
            },
        ],
        assets: vec![SourceArtifactRecord {
            path: asset_path,
            sha256: digest(b"retained-image"),
            size_bytes: b"retained-image".len() as u64,
            kind: "image".into(),
        }],
        baseline_path,
        candidate: SourceCandidateRecord {
            markdown_hash,
            title: title.into(),
            source_kind: "spreadsheet".into(),
            canonical_url: None,
            platform: None,
            platform_content_id: None,
            author: None,
            published_at: None,
            language: Some("zh-CN".into()),
        },
        provenance: SourceProvenance {
            locator: "file:D:/fixtures/数据集.csv".into(),
            route: "file.spreadsheet".into(),
            engine_id: "fixture-engine".into(),
            engine_version: "1".into(),
        },
        quality: quality(),
        created_at: imported_at.clone(),
        human_edit_hash: Some(digest(markdown.as_bytes())),
        checkpoint: Some("checkpoint-import".into()),
    };
    let manifest = SourceManifest {
        schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
        source_id: source_id.clone(),
        source_kind: "spreadsheet".into(),
        current_version_id: version_id.clone(),
        wiki_path: wiki_path.clone(),
        aliases: Vec::new(),
        origins: vec!["file:D:/fixtures/数据集.csv".into()],
        canonical_url: None,
        platform: None,
        platform_content_id: None,
        title: title.into(),
        author: None,
        published_at: None,
        imported_at: imported_at.clone(),
        language: Some("zh-CN".into()),
        versions: vec![version],
        compiled_consumptions: Vec::new(),
        restricted_content: false,
        restricted_identity_summary: None,
        timeline: vec![
            SourceTimelineEvent {
                event_id: "event-import".into(),
                kind: "imported".into(),
                version_id: Some(version_id.clone()),
                created_at: imported_at.clone(),
                checkpoint: Some("checkpoint-import".into()),
            },
            SourceTimelineEvent {
                event_id: "event-keystroke".into(),
                kind: "keystroke".into(),
                version_id: Some(version_id.clone()),
                created_at: imported_at,
                checkpoint: None,
            },
        ],
    };
    SourceRegistry::validate_manifest_contract(&manifest).unwrap();
    let manifest_path = manifest_path(&source_id);
    write(&root, &manifest_path, pretty_json(&manifest).unwrap());
    let pointer = SourcePointer {
        source_id: source_id.clone(),
        version_id: version_id.clone(),
    };
    let index = SourceIndex {
        schema_version: SOURCE_REGISTRY_SCHEMA_VERSION,
        by_content_hash: BTreeMap::from([(body_hash, pointer.clone())]),
        by_locator: BTreeMap::from([("file:D:/fixtures/数据集.csv".into(), pointer)]),
    };
    write(&root, SOURCE_INDEX_PATH, pretty_json(&index).unwrap());
    write(
        &root,
        "wiki/concepts/引用.md",
        "# 引用\n\n请看 [[数据集]]；派生页必须保留。\n",
    );
    Fixture {
        context,
        root,
        source_id,
        version_id,
        wiki_path,
        child_path,
        markdown,
        child_markdown,
        manifest_path,
    }
}

fn page_meta(path: &str) -> WikiPageMeta {
    WikiPageMeta {
        path: path.into(),
        title: "页面".into(),
        page_type: WikiPageType::Source,
        tags: Vec::new(),
        sources: Vec::new(),
        aliases: Vec::new(),
        created: None,
        updated: None,
        starred: false,
        bookmarked: false,
        word_count: 1,
        file_size: 1,
        modified_time: "2026-07-27T00:00:00Z".into(),
        hash: "a".repeat(64),
        wikilinks: Vec::new(),
        source_binding: None,
        source_id: None,
        version_id: None,
        source_status: None,
        quality: None,
    }
}

#[test]
fn source_mode_requires_source_type_and_a_valid_registry_binding() {
    let fixture = package_fixture("binding");
    let mut tree = WikiTree {
        root: WikiTreeNode {
            name: "wiki".into(),
            kind: WikiTreeNodeKind::Folder,
            path: "wiki".into(),
            page_type: None,
            title: None,
            starred: false,
            bookmarked: false,
            file_count: 2,
            children: Vec::new(),
        },
        pages: vec![
            page_meta(&fixture.wiki_path),
            page_meta("wiki/concepts/fake-source.md"),
        ],
        total_pages: 2,
    };
    apply_validated_source_bindings(&fixture.context, &FileStore, &mut tree).unwrap();
    assert_eq!(
        tree.pages[0].source_id.as_deref(),
        Some(fixture.source_id.as_str())
    );
    assert_eq!(
        tree.pages[0].version_id.as_deref(),
        Some(fixture.version_id.as_str())
    );
    assert!(tree.pages[0].source_binding.is_some());
    assert_eq!(
        tree.pages[0]
            .quality
            .as_ref()
            .map(|report| report.level.clone()),
        Some(QualityLevel::Pass)
    );
    assert!(tree.pages[1].source_binding.is_none());
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn page_binding_uses_the_returned_markdown_snapshot_and_only_its_exact_source() {
    let fixture = package_fixture("page-binding-snapshot");
    let (_, body) = parse_final_source(&fixture.markdown).unwrap();
    let mut page = WikiPageContent {
        meta: page_meta(&fixture.wiki_path),
        raw_markdown: fixture.markdown.clone(),
        body_markdown: body,
        frontmatter_yaml: None,
    };

    apply_validated_page_binding(&fixture.context, &FileStore, &mut page).unwrap();
    assert_eq!(
        page.meta.source_id.as_deref(),
        Some(fixture.source_id.as_str())
    );

    page.meta = page_meta(&fixture.wiki_path);
    page.raw_markdown = fixture.markdown.replacen(
        &format!("sourceId: \"{}\"", fixture.source_id),
        "sourceId: \"source-forged\"",
        1,
    );
    apply_validated_page_binding(&fixture.context, &FileStore, &mut page).unwrap();
    assert!(
        page.meta.source_binding.is_none(),
        "a valid on-disk Source must not promote a different returned snapshot"
    );

    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn page_and_tree_binding_both_reject_missing_or_corrupt_package_manifests() {
    for case in ["missing", "corrupt"] {
        let fixture = package_fixture(&format!("binding-package-{case}"));
        let package_path = fixture.root.join(format!(
            "raw/sources/{}/{}/derived/source-package.json",
            fixture.source_id, fixture.version_id
        ));
        if case == "missing" {
            fs::remove_file(&package_path).unwrap();
        } else {
            fs::write(&package_path, b"{}").unwrap();
        }

        let mut tree = WikiTree {
            root: WikiTreeNode {
                name: "wiki".into(),
                kind: WikiTreeNodeKind::Folder,
                path: "wiki".into(),
                page_type: None,
                title: None,
                starred: false,
                bookmarked: false,
                file_count: 1,
                children: Vec::new(),
            },
            pages: vec![page_meta(&fixture.wiki_path)],
            total_pages: 1,
        };
        apply_validated_source_bindings(&fixture.context, &FileStore, &mut tree).unwrap();
        assert!(
            tree.pages[0].source_binding.is_none(),
            "tree binding must reject a {case} package manifest"
        );

        let (_, body) = parse_final_source(&fixture.markdown).unwrap();
        let mut page = WikiPageContent {
            meta: page_meta(&fixture.wiki_path),
            raw_markdown: fixture.markdown.clone(),
            body_markdown: body,
            frontmatter_yaml: None,
        };
        apply_validated_page_binding(&fixture.context, &FileStore, &mut page).unwrap();
        assert!(
            page.meta.source_binding.is_none(),
            "targeted binding must match tree behavior for a {case} package manifest"
        );

        fs::remove_dir_all(fixture.root).unwrap();
    }
}

#[test]
fn generic_wiki_operations_reject_source_namespace_and_package_members() {
    let fixture = package_fixture("generic-guards");
    assert_eq!(
        reject_generic_source_path(&fixture.context, &FileStore, &fixture.child_path)
            .unwrap_err()
            .code,
        "SOURCE_DEDICATED_ACTION_REQUIRED"
    );
    assert_eq!(
        reject_generic_source_create("wiki/concepts/fake.md", Some("source"), None)
            .unwrap_err()
            .code,
        "SOURCE_DEDICATED_ACTION_REQUIRED"
    );
    assert!(reject_generic_source_create("wiki/concepts/normal.md", Some("concept"), None).is_ok());
    fs::remove_dir_all(fixture.root).unwrap();
}

fn extend_source_for_ai(fixture: &Fixture) -> String {
    let mut markdown = fixture.markdown.clone();
    markdown.push_str(
        "\n\n## 访谈记录\n\n张三说明该项目会在 2026-07-24 处理 42 个条目，并保留“忠实原话”。\
         这些现有材料用于验证结构整理、段落重排与列表规范化，但不允许添加任何外部事实。\
         当前 Source 的文字已经足够完整，因此不需要重新读取原始附件，也不需要再次执行 OCR 或 ASR。\n",
    );
    markdown
}

#[test]
fn source_ai_input_is_current_source_only_bounded_and_prompts_for_ocr_when_insufficient() {
    let fixture = package_fixture("ai-input");
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    let insufficient = service
        .prepare_source_ai_organize_input(
            &fixture.context,
            &FileStore,
            &fixture.source_id,
            &fixture.version_id,
            &digest(fixture.markdown.as_bytes()),
            None,
        )
        .unwrap_err();
    assert_eq!(insufficient.code, "SOURCE_AI_CONTENT_INSUFFICIENT");
    assert_eq!(
        insufficient.details.as_ref().unwrap()["suggestedAction"],
        "asr"
    );

    let extended = extend_source_for_ai(&fixture);
    write(&fixture.root, &fixture.wiki_path, extended.as_bytes());
    let input = service
        .prepare_source_ai_organize_input(
            &fixture.context,
            &FileStore,
            &fixture.source_id,
            &fixture.version_id,
            &digest(extended.as_bytes()),
            Some("保留现有引文"),
        )
        .unwrap();
    assert_eq!(input.current_markdown, extended);
    assert_eq!(input.custom_instructions.as_deref(), Some("保留现有引文"));
    assert!(input.retained_text_evidence.iter().all(|evidence| {
        let lower = evidence.kind.to_ascii_lowercase();
        lower.contains("transcript")
            || lower.contains("subtitle")
            || lower.contains("ocr")
            || lower.contains("caption")
    }));
    assert_eq!(input.media_references.len(), 1);
    assert!(input.media_references[0].ends_with("/cover.png"));
    assert!(!input
        .media_references
        .iter()
        .any(|reference| reference.ends_with(".wav") || reference.ends_with(".json")));
    assert!(!input
        .retained_text_evidence
        .iter()
        .any(|evidence| evidence.path.contains("source.bin")));
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn source_ai_candidate_stays_staged_then_applies_checkpointed_version_and_restores_original() {
    let fixture = package_fixture("ai-apply");
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    let files = FileStore;
    let git = GitService;
    let extended = extend_source_for_ai(&fixture);
    write(&fixture.root, &fixture.wiki_path, extended.as_bytes());
    let input = service
        .prepare_source_ai_organize_input(
            &fixture.context,
            &files,
            &fixture.source_id,
            &fixture.version_id,
            &digest(extended.as_bytes()),
            None,
        )
        .unwrap();
    let (_, body) = parse_final_source(&extended).unwrap();
    let rewritten_body = body
        .replace(
            "张三说明该项目会在 2026-07-24 处理 42 个条目",
            "李四确认该项目已在 2026-07-25 处理 43 个条目",
        )
        .replace("“忠实原话”", "“修订后的引文”");
    let raw = serde_json::json!({
        "overview": "李四确认项目已处理 43 个条目；这些事实变化需要在 Diff 中由用户审阅。",
        "bodyMarkdown": rewritten_body,
    })
    .to_string();
    let candidate_markdown =
        source_ai_organize::build_candidate_markdown(&extended, "数据集", &raw).unwrap();
    let candidate = service
        .store_source_ai_organize_candidate(
            &fixture.context,
            &files,
            &input,
            "task-ai-1",
            SourceAiOrganizeRoute::Byok,
            "open_ai".into(),
            "fixture-model".into(),
            None,
            candidate_markdown,
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(fixture.root.join(&fixture.wiki_path)).unwrap(),
        extended,
        "unconfirmed AI output must never overwrite the current Source"
    );
    assert!(fixture
        .root
        .join(format!(
            ".app/source-candidates/{}/{}.json",
            fixture.source_id, candidate.candidate_id
        ))
        .is_file());
    let preview = service
        .preview_source_update(
            &fixture.context,
            &files,
            &fixture.source_id,
            &candidate.candidate_id,
        )
        .unwrap();
    assert_eq!(preview.mode, SourceUpdateMode::TwoWay);
    assert_eq!(preview.candidate_markdown.matches("## 内容概览").count(), 1);
    assert!(preview.diff.contains("42"));
    assert!(preview.diff.contains("43"));
    assert!(preview.diff.contains("张三"));
    assert!(preview.diff.contains("李四"));
    assert!(
        fs::read_to_string(fixture.root.join(&fixture.wiki_path))
            .unwrap()
            .contains("42 个条目"),
        "fact changes must remain staged until the user explicitly applies the candidate"
    );

    git.initialize_repository(&fixture.context, "fixture baseline")
        .unwrap();
    let applied = service
        .apply_source_candidate(
            &fixture.context,
            &files,
            &git,
            &ApplySourceCandidateRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
                candidate_id: candidate.candidate_id,
                guard_token: preview.guard_token,
                merged_markdown: None,
            },
        )
        .unwrap();
    assert!(applied.checkpoint.is_some());
    let manifest: SourceManifest = files
        .read_json(&fixture.context, &fixture.manifest_path)
        .unwrap();
    assert_eq!(manifest.current_version_id, applied.version_id);
    let version = manifest
        .versions
        .iter()
        .find(|version| version.version_id == applied.version_id)
        .unwrap();
    assert_eq!(version.provenance.route, "source_ai_byok");
    assert_eq!(version.provenance.engine_id, "open_ai");
    assert_eq!(version.provenance.engine_version, "fixture-model");
    assert!(manifest.timeline.iter().any(|event| {
        event.kind == "ai_organize_applied"
            && event.version_id.as_deref() == Some(applied.version_id.as_str())
    }));
    assert!(
        fs::read_to_string(fixture.root.join(&fixture.wiki_path))
            .unwrap()
            .contains("43 个条目"),
        "the reviewed fact change should be written only after explicit apply"
    );
    let current_hash = files
        .file_hash(&fixture.context, &fixture.wiki_path)
        .unwrap();
    service
        .restore_source_version(
            &fixture.context,
            &files,
            &git,
            &fixture.source_id,
            &fixture.version_id,
            &current_hash,
        )
        .unwrap();
    let restored = fs::read_to_string(fixture.root.join(&fixture.wiki_path)).unwrap();
    assert_eq!(
        parse_final_source(&restored).unwrap().1,
        parse_final_source(&fixture.markdown).unwrap().1,
        "the faithful original body must remain restorable as a new version"
    );
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn source_ai_generation_survives_external_edit_but_stale_apply_requires_rediff() {
    let fixture = package_fixture("ai-external-edit");
    let service = ImportV2Service::default();
    let files = FileStore;
    let extended = extend_source_for_ai(&fixture);
    write(&fixture.root, &fixture.wiki_path, extended.as_bytes());
    let input = service
        .prepare_source_ai_organize_input(
            &fixture.context,
            &files,
            &fixture.source_id,
            &fixture.version_id,
            &digest(extended.as_bytes()),
            None,
        )
        .unwrap();
    let (_, body) = parse_final_source(&extended).unwrap();
    let raw = serde_json::json!({
        "overview": "忠实概览。",
        "bodyMarkdown": body,
    })
    .to_string();
    let candidate_markdown =
        source_ai_organize::build_candidate_markdown(&extended, "数据集", &raw).unwrap();
    let during_generation_edit = format!("{extended}\n\n人工外部补充，必须保留。\n");
    write(
        &fixture.root,
        &fixture.wiki_path,
        during_generation_edit.as_bytes(),
    );
    let candidate = service
        .store_source_ai_organize_candidate(
            &fixture.context,
            &files,
            &input,
            "task-ai-external",
            SourceAiOrganizeRoute::Agent,
            "codex".into(),
            "cli-default".into(),
            Some("fixture-cli-version".into()),
            candidate_markdown.clone(),
        )
        .unwrap();
    let ai_organize = candidate.ai_organize.as_ref().unwrap();
    assert_eq!(ai_organize.model, "cli-default");
    assert_eq!(
        ai_organize.engine_version.as_deref(),
        Some("fixture-cli-version")
    );
    let preview = service
        .preview_source_update(
            &fixture.context,
            &files,
            &fixture.source_id,
            &candidate.candidate_id,
        )
        .unwrap();
    assert_eq!(preview.mode, SourceUpdateMode::ThreeWay);
    assert!(preview.current_markdown.contains("人工外部补充"));

    let next_external_edit = format!("{during_generation_edit}\n第二次外部编辑。\n");
    write(
        &fixture.root,
        &fixture.wiki_path,
        next_external_edit.as_bytes(),
    );
    let error = service
        .apply_source_candidate(
            &fixture.context,
            &files,
            &GitService,
            &ApplySourceCandidateRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
                candidate_id: candidate.candidate_id.clone(),
                guard_token: preview.guard_token,
                merged_markdown: Some(preview.candidate_markdown),
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "SOURCE_CHANGED");
    let refreshed = service
        .preview_source_update(
            &fixture.context,
            &files,
            &fixture.source_id,
            &candidate.candidate_id,
        )
        .unwrap();
    assert_eq!(refreshed.mode, SourceUpdateMode::ThreeWay);
    assert!(refreshed.current_markdown.contains("第二次外部编辑"));
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn source_ai_reservation_is_single_per_source_and_independent_across_sources() {
    let service = ImportV2Service::default();
    service
        .reserve_source_ai("project\0source-a".into())
        .unwrap();
    assert_eq!(
        service
            .reserve_source_ai("project\0source-a".into())
            .unwrap_err()
            .code,
        "SOURCE_AI_ALREADY_RUNNING"
    );
    service
        .reserve_source_ai("project\0source-b".into())
        .unwrap();
    assert!(service.has_source_ai_reservation("project\0source-a"));
    assert!(service.has_source_ai_reservation("project\0source-b"));
    service.release_source_ai("project\0source-a");
    service
        .reserve_source_ai("project\0source-a".into())
        .unwrap();
}

#[test]
fn legacy_agent_candidate_metadata_is_normalized_without_losing_the_cli_version() {
    let mut metadata = Some(
        serde_json::from_value::<SourceAiOrganizeCandidateMeta>(serde_json::json!({
            "taskId": "legacy-task",
            "route": "agent",
            "engine": "codex",
            "model": "codex-cli 1.2.3"
        }))
        .unwrap(),
    );

    normalize_ai_organize_metadata(&mut metadata);

    let metadata = metadata.unwrap();
    assert_eq!(metadata.model, "cli-default");
    assert_eq!(metadata.engine_version.as_deref(), Some("codex-cli 1.2.3"));
}

#[test]
fn cancelled_source_ai_completion_can_remove_only_its_staged_candidate() {
    let fixture = package_fixture("ai-cancel-cleanup");
    let service = ImportV2Service::default();
    let files = FileStore;
    let extended = extend_source_for_ai(&fixture);
    write(&fixture.root, &fixture.wiki_path, extended.as_bytes());
    let input = service
        .prepare_source_ai_organize_input(
            &fixture.context,
            &files,
            &fixture.source_id,
            &fixture.version_id,
            &digest(extended.as_bytes()),
            None,
        )
        .unwrap();
    let (_, body) = parse_final_source(&extended).unwrap();
    let raw = serde_json::json!({
        "overview": "忠实概览。",
        "bodyMarkdown": body,
    })
    .to_string();
    let candidate_markdown =
        source_ai_organize::build_candidate_markdown(&extended, "数据集", &raw).unwrap();
    let candidate = service
        .store_source_ai_organize_candidate(
            &fixture.context,
            &files,
            &input,
            "task-ai-cancel",
            SourceAiOrganizeRoute::Agent,
            "codex".into(),
            "cli-default".into(),
            Some("fixture-cli-version".into()),
            candidate_markdown.clone(),
        )
        .unwrap();
    service
        .discard_source_ai_organize_candidate(
            &fixture.context,
            &files,
            &fixture.source_id,
            &candidate.candidate_id,
            "task-ai-cancel",
        )
        .unwrap();
    assert!(!fixture
        .root
        .join(format!(
            ".app/source-candidates/{}/{}.json",
            fixture.source_id, candidate.candidate_id
        ))
        .exists());
    assert_eq!(
        fs::read_to_string(fixture.root.join(&fixture.wiki_path)).unwrap(),
        extended
    );
    let user_discarded = service
        .store_source_ai_organize_candidate(
            &fixture.context,
            &files,
            &input,
            "task-ai-user-discard",
            SourceAiOrganizeRoute::Byok,
            "open_ai".into(),
            "fixture".into(),
            None,
            candidate_markdown,
        )
        .unwrap();
    service
        .discard_source_candidate(
            &fixture.context,
            &files,
            &fixture.source_id,
            &user_discarded.candidate_id,
        )
        .unwrap();
    assert!(!fixture
        .root
        .join(format!(
            ".app/source-candidates/{}/{}.json",
            fixture.source_id, user_discarded.candidate_id
        ))
        .exists());
    assert_eq!(
        fs::read_to_string(fixture.root.join(&fixture.wiki_path)).unwrap(),
        extended,
        "discarding a staged candidate must not alter the current Source"
    );
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn candidate_diff_apply_external_three_way_and_reliable_package_restore_work() {
    let fixture = package_fixture("candidate-apply");
    let service = ImportV2Service::default();
    service
        .register_engine(Arc::new(ReprocessFixtureEngine))
        .unwrap();
    let files = FileStore;
    let git = GitService;
    let candidate = service
        .reprocess_source(
            &fixture.context,
            &files,
            &ReprocessSourceRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
                expected_markdown_hash: digest(fixture.markdown.as_bytes()),
                subtitle_path: None,
            },
            SourceCandidateKind::Asr,
            &CancellationToken::default(),
        )
        .unwrap();
    let preview = service
        .preview_source_update(
            &fixture.context,
            &files,
            &fixture.source_id,
            &candidate.candidate_id,
        )
        .unwrap();
    assert_eq!(preview.mode, SourceUpdateMode::TwoWay);
    assert!(!preview.diff.trim().is_empty());
    git.initialize_repository(&fixture.context, "fixture baseline")
        .unwrap();
    let applied = service
        .apply_source_candidate(
            &fixture.context,
            &files,
            &git,
            &ApplySourceCandidateRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
                candidate_id: candidate.candidate_id,
                guard_token: preview.guard_token,
                merged_markdown: None,
            },
        )
        .unwrap();
    assert!(applied.checkpoint.is_some());
    assert_ne!(applied.version_id, fixture.version_id);
    let applied_manifest: SourceManifest = FileStore
        .read_json(&fixture.context, &fixture.manifest_path)
        .unwrap();
    let applied_version = applied_manifest
        .versions
        .iter()
        .find(|version| version.version_id == applied.version_id)
        .unwrap();
    assert!(applied_version
        .raw_evidence
        .iter()
        .any(|artifact| artifact.kind == "source_snapshot"));
    assert!(applied_version
        .raw_evidence
        .iter()
        .any(|artifact| artifact.kind == "transcript"));
    let processing_evidence = applied_version
        .raw_evidence
        .iter()
        .find(|artifact| artifact.path.contains("/derived/processing-evidence-"))
        .expect("the actual processing evidence must be versioned with the applied Source");
    assert_eq!(processing_evidence.kind, "transcript");
    assert_eq!(
        fs::read(fixture.root.join(&processing_evidence.path)).unwrap(),
        r#"{"segments":[{"text":"这是实际引擎重新处理后的文本。"}]}"#.as_bytes()
    );
    write(
        &fixture.root,
        &fixture.child_path,
        b"# external child edit\n",
    );
    let current_hash = files
        .file_hash(&fixture.context, &fixture.wiki_path)
        .unwrap();
    service
        .restore_source_version(
            &fixture.context,
            &files,
            &git,
            &fixture.source_id,
            &fixture.version_id,
            &current_hash,
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(fixture.root.join(&fixture.child_path)).unwrap(),
        fixture.child_markdown
    );
    fs::remove_dir_all(fixture.root).unwrap();

    let edited_fixture = package_fixture("candidate-three-way");
    let edited_service = ImportV2Service::default();
    edited_service
        .register_engine(Arc::new(ReprocessFixtureEngine))
        .unwrap();
    let edited = edited_fixture
        .markdown
        .replace("忠实原稿。", "忠实原稿。\n\n人工补充。");
    write(
        &edited_fixture.root,
        &edited_fixture.wiki_path,
        edited.as_bytes(),
    );
    let candidate = edited_service
        .reprocess_source(
            &edited_fixture.context,
            &files,
            &ReprocessSourceRequest {
                project_id: edited_fixture.context.project_id.clone(),
                project_root_path: edited_fixture.root.to_string_lossy().into_owned(),
                source_id: edited_fixture.source_id.clone(),
                expected_markdown_hash: digest(edited.as_bytes()),
                subtitle_path: None,
            },
            SourceCandidateKind::Asr,
            &CancellationToken::default(),
        )
        .unwrap();
    let preview = edited_service
        .preview_source_update(
            &edited_fixture.context,
            &files,
            &edited_fixture.source_id,
            &candidate.candidate_id,
        )
        .unwrap();
    assert_eq!(preview.mode, SourceUpdateMode::ThreeWay);
    assert!(preview.current_markdown.contains("人工补充"));
    fs::remove_dir_all(edited_fixture.root).unwrap();
}

#[test]
fn dedicated_package_move_updates_manifest_wikilinks_and_checkpoint_with_cjk_paths() {
    let fixture = package_fixture("move");
    let service = ImportV2Service::default();
    let git = GitService;
    git.initialize_repository(&fixture.context, "fixture baseline")
        .unwrap();
    let new_path = "wiki/sources/local/移动数据/index.md";
    let preview = service
        .preview_move_source(
            &fixture.context,
            &FileStore,
            &PreviewMoveSourceRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
                new_wiki_path: new_path.into(),
            },
        )
        .unwrap();
    let result = service
        .move_source(
            &fixture.context,
            &FileStore,
            &git,
            &MoveSourceRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
                new_wiki_path: new_path.into(),
                guard_token: preview.guard_token,
            },
        )
        .unwrap();
    assert!(result.checkpoint.is_some());
    assert!(!fixture.root.join(&fixture.wiki_path).exists());
    assert!(!fixture.root.join(&fixture.child_path).exists());
    assert!(fixture.root.join(new_path).exists());
    assert!(fixture
        .root
        .join("wiki/sources/local/移动数据/表一.md")
        .exists());
    let manifest: SourceManifest = FileStore
        .read_json(&fixture.context, &fixture.manifest_path)
        .unwrap();
    assert_eq!(manifest.wiki_path, new_path);
    assert!(
        fs::read_to_string(fixture.root.join("wiki/concepts/引用.md"))
            .unwrap()
            .contains("[[移动数据]]")
    );
    let detail = service
        .get_source_detail(&fixture.context, &FileStore, &fixture.source_id)
        .unwrap();
    assert!(!detail
        .timeline
        .iter()
        .any(|event| event.kind == "manual_checkpoint"));
    write(
        &fixture.root,
        &fixture.wiki_path,
        "# 另一来源\n\n此路径已由其他来源占用。\n",
    );
    let delete_preview = service
        .preview_delete_source(
            &fixture.context,
            &FileStore,
            &PreviewDeleteSourceRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
            },
        )
        .unwrap();
    assert!(!delete_preview
        .paths
        .iter()
        .any(|entry| entry.path == fixture.wiki_path));
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn package_delete_lists_and_removes_every_owned_artifact_but_keeps_references() {
    let fixture = package_fixture("delete");
    let candidate_evidence = format!(
        ".app/source-candidate-evidence/{}/pending/0.bin",
        fixture.source_id
    );
    write(&fixture.root, &candidate_evidence, b"pending evidence");
    let service = ImportV2Service::default();
    let git = GitService;
    git.initialize_repository(&fixture.context, "fixture baseline")
        .unwrap();
    let preview = service
        .preview_delete_source(
            &fixture.context,
            &FileStore,
            &PreviewDeleteSourceRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
            },
        )
        .unwrap();
    let paths = preview
        .paths
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&fixture.manifest_path.as_str()));
    assert!(paths.iter().any(|path| path.starts_with("raw/sources/")));
    assert!(paths.iter().any(|path| path.starts_with("raw/assets/")));
    assert!(paths.iter().any(|path| path.contains("source-artifacts")));
    assert!(paths.contains(&candidate_evidence.as_str()));
    assert!(paths.contains(&fixture.wiki_path.as_str()));
    assert!(paths.contains(&fixture.child_path.as_str()));
    assert_eq!(preview.reference_count, 1);
    assert!(preview.expected_freed_bytes > 0);
    let result = service
        .delete_source(
            &fixture.context,
            &FileStore,
            &git,
            &DeleteSourceRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
                guard_token: preview.guard_token,
                confirmation_text: DELETE_CONFIRMATION_TEXT.into(),
            },
        )
        .unwrap();
    assert!(result.checkpoint.is_some());
    assert!(preview
        .paths
        .iter()
        .all(|entry| !fixture.root.join(&entry.path).exists()));
    assert!(!fixture
        .root
        .join(".app/source-candidate-evidence")
        .join(&fixture.source_id)
        .exists());
    assert!(fixture.root.join("wiki/concepts/引用.md").exists());
    let index = SourceRegistry::read_index(&fixture.context, &FileStore).unwrap();
    assert!(index.by_content_hash.is_empty());
    assert!(index.by_locator.is_empty());
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn package_delete_rolls_back_every_path_when_atomic_audit_install_fails() {
    let fixture = package_fixture("delete-rollback");
    let service = ImportV2Service::default();
    let git = GitService;
    git.initialize_repository(&fixture.context, "fixture baseline")
        .unwrap();
    let preview = service
        .preview_delete_source(
            &fixture.context,
            &FileStore,
            &PreviewDeleteSourceRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
            },
        )
        .unwrap();
    let before = preview
        .paths
        .iter()
        .map(|entry| {
            (
                entry.path.clone(),
                fs::read(fixture.root.join(&entry.path)).unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let index_before = fs::read(fixture.root.join(SOURCE_INDEX_PATH)).unwrap();
    set_fail_next_candidate_install();
    assert!(service
        .delete_source(
            &fixture.context,
            &FileStore,
            &git,
            &DeleteSourceRequest {
                project_id: fixture.context.project_id.clone(),
                project_root_path: fixture.root.to_string_lossy().into_owned(),
                source_id: fixture.source_id.clone(),
                guard_token: preview.guard_token,
                confirmation_text: DELETE_CONFIRMATION_TEXT.into(),
            },
        )
        .is_err());
    for (path, bytes) in before {
        assert_eq!(fs::read(fixture.root.join(path)).unwrap(), bytes);
    }
    assert_eq!(
        fs::read(fixture.root.join(SOURCE_INDEX_PATH)).unwrap(),
        index_before
    );
    assert!(fixture.root.join("wiki/concepts/引用.md").exists());
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn product_timeline_exposes_only_the_six_meaningful_categories() {
    let fixture = package_fixture("timeline");
    let detail = ImportV2Service::default()
        .get_source_detail(&fixture.context, &FileStore, &fixture.source_id)
        .unwrap();
    assert_eq!(detail.timeline.len(), 1);
    assert_eq!(detail.timeline[0].kind, "source_imported");
    assert_eq!(
        detail.timeline[0].restorable, detail.versions[0].restorable,
        "the timeline and version list must reuse one restorability decision"
    );
    for raw in [
        "imported",
        "ocr_reprocessed",
        "ai_organize_applied",
        "manual_checkpoint",
        "source_refreshed",
        "version_restored",
    ] {
        assert!(product_timeline_kind(raw).is_some());
    }
    assert!(product_timeline_kind("keystroke").is_none());
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn package_restore_requires_every_child_baseline_and_low_level_writes_keep_source_guards() {
    let fixture = package_fixture("restore-integrity");
    let child_baseline = format!(
        ".app/source-artifacts/{}/{}/package/表一.md",
        fixture.source_id, fixture.version_id
    );
    fs::remove_file(fixture.root.join(&child_baseline)).unwrap();
    let detail = ImportV2Service::default()
        .get_source_detail(&fixture.context, &FileStore, &fixture.source_id)
        .unwrap();
    assert_eq!(detail.versions.len(), 1);
    assert!(!detail.versions[0].restorable);

    let file_commands = include_str!("../../commands/file_commands.rs");
    let write_body = file_commands
        .split("pub fn write_markdown_file")
        .nth(1)
        .and_then(|body| body.split("#[tauri::command]").next())
        .unwrap();
    assert!(write_body.contains("reject_generic_source_path"));
    assert!(write_body.contains("reject_generic_source_create"));
    fs::remove_dir_all(fixture.root).unwrap();
}

#[test]
fn refresh_executes_the_registered_route_instead_of_reusing_current_markdown() {
    let fixture = package_fixture("refresh-route");
    let mut loaded = load_source(&fixture.context, &FileStore, &fixture.source_id).unwrap();
    loaded.manifest.canonical_url = Some("https://example.com/source".into());
    loaded.version.provenance.route = "fixture.refresh".into();
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    service
        .register_engine(Arc::new(RefreshFixtureEngine))
        .unwrap();
    let refreshed = execute_source_processing(
        &service,
        &fixture.context,
        &loaded,
        &SourceCandidateKind::Refresh,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(refreshed.markdown.contains("远端内容已重新获取"));
    assert_ne!(refreshed.markdown, fixture.markdown);
    fs::remove_dir_all(fixture.root).unwrap();
}
