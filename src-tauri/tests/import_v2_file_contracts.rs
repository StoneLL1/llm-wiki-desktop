use llm_wiki_desktop_lib::models::import_v2_file::{
    DiscoveredFile, FileContentKind, FileDetectionMethod, FileFormat, FileIdentity,
};

#[test]
fn file_contract_serializes_stable_wire_names() {
    let value = serde_json::to_value(DiscoveredFile {
        source_identity: llm_wiki_desktop_lib::models::import_v2::SourceIdentity {
            canonical_path: "C:/sources/readme.md".into(),
            size_bytes: 12,
            modified_nanos: None,
            file_id: None,
            sha256: "00".repeat(32),
            magic: "11".repeat(32),
        },
        source_path: r"C:\资料\报告.docx".into(),
        relative_path: "资料/报告.docx".into(),
        display_name: "报告.docx".into(),
        format: FileFormat::Docx,
        content_kind: FileContentKind::Document,
        size_bytes: 42,
        identity: FileIdentity {
            extension: "docx".into(),
            magic: "zip_ooxml".into(),
            mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into(),
            detection_method: FileDetectionMethod::Container,
            extension_mismatch: false,
        },
        large_data: None,
    })
    .unwrap();

    assert_eq!(value["format"], "docx");
    assert_eq!(value["relativePath"], "资料/报告.docx");
}
