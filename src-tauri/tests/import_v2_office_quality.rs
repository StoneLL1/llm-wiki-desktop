use llm_wiki_desktop_lib::services::import_v2::office_postprocess::{
    Cell, PresentationPlan, Sheet, Slide, SlideImage, WorkbookOutputMode, WorkbookPlan,
    EXCEL_MAX_COLUMNS,
};
use llm_wiki_desktop_lib::services::import_v2::{engine::EngineResult, quality_gate::QualityGate};

#[test]
fn workbook_modes_preserve_identity_and_never_silently_truncate() {
    let sheet = Sheet {
        name: "隐藏 数据".into(),
        hidden: true,
        rows: vec![vec![Cell::formula("=A1*2", "42")]; 5],
        declared_columns: EXCEL_MAX_COLUMNS,
    };
    for mode in [
        WorkbookOutputMode::SinglePage,
        WorkbookOutputMode::OverviewAndSheets,
        WorkbookOutputMode::Chunked { rows_per_chunk: 2 },
    ] {
        let output = WorkbookPlan::new("source-1", "version-1", mode, vec![sheet.clone()])
            .render()
            .unwrap();
        assert_eq!(
            (output.source_id.as_str(), output.version_id.as_str()),
            ("source-1", "version-1")
        );
        assert!(output.markdown.contains("隐藏 数据 (hidden)"));
        assert!(output.markdown.contains("`=A1*2` → 42"));
        if matches!(mode, WorkbookOutputMode::Chunked { .. }) {
            assert!(output
                .warnings
                .contains(&"WORKBOOK_RANGE_CHUNKED".to_string()));
            assert_eq!(output.csv_fallbacks.len(), 1);
            assert!(output.csv_fallbacks[0].content.lines().count() >= 5);
        }
    }
}

#[test]
fn workbook_rejects_columns_beyond_xfd() {
    let sheet = Sheet {
        name: "bad".into(),
        hidden: false,
        rows: vec![],
        declared_columns: EXCEL_MAX_COLUMNS + 1,
    };
    assert!(
        WorkbookPlan::new("s", "v", WorkbookOutputMode::SinglePage, vec![sheet])
            .render()
            .is_err()
    );
}

#[test]
fn presentation_is_one_ordered_candidate_with_notes_and_meaningful_images() {
    let slides = vec![
        Slide {
            number: 1,
            title: "一".into(),
            body: vec!["正文".into()],
            notes: Some("第一备注".into()),
            images: vec![
                SlideImage {
                    path: "decor.png".into(),
                    width_px: 8,
                    height_px: 8,
                    decorative: true,
                },
                SlideImage {
                    path: "chart.png".into(),
                    width_px: 640,
                    height_px: 480,
                    decorative: false,
                },
            ],
        },
        Slide {
            number: 2,
            title: "二".into(),
            body: vec![],
            notes: Some("第二备注".into()),
            images: vec![],
        },
    ];
    let output = PresentationPlan::new("s", "v", slides).render().unwrap();
    assert_eq!(output.candidates.len(), 1);
    let markdown = &output.candidates[0];
    assert!(
        markdown.find("<a id=\"slide-1\"").unwrap() < markdown.find("<a id=\"slide-2\"").unwrap()
    );
    assert!(markdown.contains("第一备注") && markdown.contains("第二备注"));
    assert!(markdown.contains("chart.png"));
    assert!(!markdown.contains("decor.png"));
}

#[test]
fn office_metrics_extend_the_gate_without_bypassing_core_checks() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("source.bin"), b"source").unwrap();
    std::fs::write(temp.path().join("document.md"), "# Safe").unwrap();
    let result = EngineResult {
        source_snapshot_path: "source.bin".into(),
        markdown_path: "document.md".into(),
        asset_paths: vec![],
        metadata_path: None,
        title: "Office".into(),
        text_coverage: Some(1.0),
        table_cell_accuracy: Some(1.0),
        sheet_count_exact: Some(1.0),
        slide_count_exact: Some(1.0),
        non_empty_cell_coverage: Some(0.90),
        formula_value_pairs: Some(1.0),
        meaningful_image_coverage: Some(0.95),
        warnings: vec![],
    };
    let preview = QualityGate::default()
        .evaluate(temp.path(), &result)
        .unwrap();
    assert!(preview
        .quality
        .metrics
        .iter()
        .any(|metric| metric.code == "SHEET_COUNT_EXACT" && metric.passed));
    assert!(preview
        .quality
        .metrics
        .iter()
        .any(|metric| metric.code == "NON_EMPTY_CELL_COVERAGE" && !metric.passed));
    assert!(preview
        .quality
        .warnings
        .contains(&"LOW_NON_EMPTY_CELL_COVERAGE".to_string()));

    std::fs::write(temp.path().join("document.md"), "<script>bad</script>").unwrap();
    assert!(QualityGate::default()
        .evaluate(temp.path(), &result)
        .is_err());
}

#[test]
fn old_engine_json_defaults_new_metrics() {
    let value = serde_json::json!({
        "sourceSnapshotPath":"source.bin", "markdownPath":"document.md", "assetPaths":[],
        "title":"old", "textCoverage":1.0, "tableCellAccuracy":1.0, "warnings":[]
    });
    let result: EngineResult = serde_json::from_value(value).unwrap();
    assert_eq!(result.sheet_count_exact, None);
}
