use std::path::Path;

use llm_wiki_desktop_lib::models::chat::ChatSourceRef;
use llm_wiki_desktop_lib::models::import::SourceFileType;
use llm_wiki_desktop_lib::services::{
    classify_file, ChatService, ImportService, LintService, RetrievalContext, SearchService,
};

#[test]
fn service_facades_keep_their_public_construction_contract() {
    let _import = ImportService;
    let _lint = LintService::default();
    let _chat = ChatService::default();
    let _search = SearchService::default();
    let _: Option<RetrievalContext> = None;
}

#[test]
fn import_classification_remains_reexported() {
    assert_eq!(
        classify_file(Path::new("研究报告.PDF")),
        SourceFileType::Pdf
    );
}

#[test]
fn chat_citation_parser_remains_on_the_facade() {
    let sources = vec![ChatSourceRef {
        id: "S1".into(),
        page_path: "wiki/a.md".into(),
        title: "A".into(),
        excerpt: Some("alpha".into()),
        score: 10,
        is_pinned: false,
    }];
    let parsed = ChatService::parse_model_citations("Answer [S1]", &sources);
    assert_eq!(parsed.citations.len(), 1);
}
