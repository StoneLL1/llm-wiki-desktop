use llm_wiki_desktop_lib::models::import_v2_file::{
    DiscoveredFile, FileFormat, FileIdentity,
};

#[test]
fn file_contract_serializes_stable_wire_names() {
    let value = serde_json::to_value(DiscoveredFile {
        source_path: r"C:\资料\报告.docx".into(),
        relative_path: "资料/报告.docx".into(),
        display_name: "报告.docx".into(),
        format: FileFormat::Docx,
        size_bytes: 42,
        identity: FileIdentity {
            extension: "docx".into(),
            magic: "zip_ooxml".into(),
            mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into(),
        },
    })
    .unwrap();

    assert_eq!(value["format"], "docx");
    assert_eq!(value["relativePath"], "资料/报告.docx");
}
