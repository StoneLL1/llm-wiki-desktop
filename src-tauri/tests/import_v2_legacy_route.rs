use llm_wiki_desktop_lib::models::import::SourceFileType;
use llm_wiki_desktop_lib::models::import_v2::{
    ArtifactKind, ImportArtifact, ImportInput, ImportInputKind, ImportItem, ImportItemStatus,
    ImportPreviewArtifact, ImportResourceMode, ImportSession, QualityLevel, QualityReport,
};
use llm_wiki_desktop_lib::services::import_v2::legacy_route::LegacyPreviewAdapter;

#[test]
fn projects_v2_session_into_the_existing_preview_shape() {
    let mut session = ImportSession::new("session-cjk", "project-1", ImportResourceMode::Balanced);
    session.items.push(ImportItem {
        item_id: "item-1".into(),
        input: ImportInput {
            kind: ImportInputKind::File,
            display_name: "资料.md".into(),
            locator: ".app/import-sessions/session-cjk/inputs/资料.md".into(),
            normalized_locator: Some(
                "c:/project/.app/import-sessions/session-cjk/inputs/资料.md".into(),
            ),
            source_identity: None,
        },
        status: ImportItemStatus::PreviewReady,
        selected: true,
        task_id: None,
        progress: None,
        attempts: Vec::new(),
        preview: Some(ImportPreviewArtifact {
            markdown: ImportArtifact {
                kind: ArtifactKind::Markdown,
                relative_path: "wiki/sources/files/资料.md".into(),
                sha256: "markdown-hash".into(),
                size_bytes: 21,
            },
            assets: Vec::new(),
            source_snapshot: ImportArtifact {
                kind: ArtifactKind::SourceSnapshot,
                relative_path: ".app/import-sessions/session-cjk/items/item-1/staging/source.bin"
                    .into(),
                sha256: "source-hash".into(),
                size_bytes: 21,
            },
            quality: QualityReport {
                level: QualityLevel::Pass,
                metrics: Vec::new(),
                warnings: Vec::new(),
                sheet_count_exact: None,
                slide_count_exact: None,
                non_empty_cell_coverage: None,
                formula_value_pairs: None,
                meaningful_image_coverage: None,
            },
            title: "资料".into(),
        }),
        issue: None,
    });

    let projected = LegacyPreviewAdapter::from_session(&session).unwrap();
    assert_eq!(projected.v2_session_id.as_deref(), Some("session-cjk"));
    assert_eq!(
        projected.files[0].source_path,
        ".app/import-sessions/session-cjk/inputs/资料.md"
    );
    assert_eq!(projected.files[0].file_type, SourceFileType::Markdown);
    assert_eq!(
        projected.files[0].archived_path,
        "wiki/sources/files/资料.md"
    );
    assert!(projected.files[0].extracted_assets.is_empty());
}
