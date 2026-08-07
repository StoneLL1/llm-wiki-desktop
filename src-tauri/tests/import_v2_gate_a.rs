use llm_wiki_desktop_lib::{
    errors::BackendError,
    models::{
        import_v2::{
            CommitImportSessionRequest, CommitItemDecision, ImportInput, ImportInputKind,
            ImportItemStatus, ImportMediaAuthorizationKind, ImportResourceMode, SourceIdentity,
        },
        paths::ProjectContext,
        source_package::SourcePackageManifest,
        task::TaskType,
    },
    services::{
        import_v2::{
            engine::{EngineDescriptor, EngineRequest, EngineResult, ImportEngine},
            generic_web_engine::{GenericWebEngine, WebArtifactSource},
            platform_provider::{extract_platform_collection, Platform},
            source_finalization::parse_final_source,
            source_registry::SourceRegistry,
            url_policy::{PrivateTargetGrant, SessionWebTarget, UrlPolicy},
            web_fetch::{WebFetchArtifact, WebFetchPolicy},
            web_target_store::WebTargetStore,
            CollectionImportInput, ImportV2Service,
        },
        FileStore, GitService, SecretService,
    },
    tasks::{task_model::CancellationToken, TaskService},
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use url::Url;

#[derive(Clone, Copy)]
struct LocalFixtureAsset {
    path: &'static str,
    bytes: &'static [u8],
}

#[derive(Clone, Copy)]
struct WebFixtureResponse {
    url: &'static str,
    content_type: &'static str,
    bytes: &'static [u8],
}

#[derive(Clone)]
struct GateACase {
    name: &'static str,
    route: &'static str,
    engine_id: &'static str,
    kind: ImportInputKind,
    display_name: &'static str,
    locator: &'static str,
    local_bytes: Option<&'static [u8]>,
    local_assets: &'static [LocalFixtureAsset],
    web_responses: &'static [WebFixtureResponse],
    needs_local_ocr: bool,
    expected_source_kind: &'static str,
    expected_wiki_prefix: &'static str,
    body_marker: &'static str,
    portable_asset_links: &'static [&'static str],
    expected_package_members: Option<usize>,
    package_body_marker: Option<&'static str>,
    expected_relative_path: Option<&'static str>,
}

const CJK_ASSET: &[u8] = include_bytes!("../../tests/fixtures/import-v2/local/assets/示意图.svg");
const XIAOHONGSHU_OCR: &str =
    include_str!("../../tests/fixtures/import-v2/web/xiaohongshu/image-note-ocr.txt");

const GATE_A_CASES: &[GateACase] = &[
    GateACase {
        name: "local_cjk_markdown",
        route: "file.native",
        engine_id: "builtin.native-file",
        kind: ImportInputKind::File,
        display_name: "资料/研究笔记.md",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/研究笔记.md"
        )),
        local_assets: &[LocalFixtureAsset {
            path: "assets/示意图.svg",
            bytes: CJK_ASSET,
        }],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "带相对资源",
        portable_asset_links: &["assets/assets/示意图.svg"],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("资料/研究笔记.md"),
    },
    GateACase {
        name: "clipboard_markdown_text",
        route: "file.native",
        engine_id: "builtin.native-file",
        kind: ImportInputKind::ClipboardText,
        display_name: "剪贴板摘录.md",
        locator: "",
        local_bytes: Some("# 剪贴板摘录\n\n隐私边界内的正文。".as_bytes()),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_text",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "隐私边界内的正文",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("剪贴板摘录.md"),
    },
    GateACase {
        name: "local_plain_text",
        route: "file.native",
        engine_id: "builtin.native-file",
        kind: ImportInputKind::File,
        display_name: "资料/说明.txt",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/matrix/note.txt"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Matrix plain text fixture",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("资料/说明.txt"),
    },
    GateACase {
        name: "local_sanitized_html",
        route: "file.native",
        engine_id: "builtin.native-file",
        kind: ImportInputKind::File,
        display_name: "离线网页.html",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/matrix/page.html"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Matrix HTML",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("离线网页.html"),
    },
    GateACase {
        name: "local_small_csv",
        route: "file.csv-package",
        engine_id: "builtin.csv-package",
        kind: ImportInputKind::File,
        display_name: "小表.csv",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/matrix/small.csv"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "alpha",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("小表.csv"),
    },
    GateACase {
        name: "local_large_csv_package",
        route: "file.csv-package",
        engine_id: "builtin.csv-package",
        kind: ImportInputKind::File,
        display_name: "超大数据.csv",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/large.csv"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "10052 rows",
        portable_asset_links: &[],
        expected_package_members: Some(4),
        package_body_marker: Some("记录 10051"),
        expected_relative_path: Some("超大数据.csv"),
    },
    GateACase {
        name: "local_docx",
        route: "office.modern.docx",
        engine_id: "builtin.office-docx",
        kind: ImportInputKind::File,
        display_name: "结构化文档.docx",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/structured-document.docx"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Research summary",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("结构化文档.docx"),
    },
    GateACase {
        name: "local_xlsx_package",
        route: "office.modern.xlsx",
        engine_id: "builtin.office-xlsx",
        kind: ImportInputKind::File,
        display_name: "多表公式.xlsx",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/unicode-multi-sheet-formulas.xlsx"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "3 non-empty sheets",
        portable_asset_links: &[],
        expected_package_members: Some(4),
        package_body_marker: Some("Alpha | 3 | 7 | 21"),
        expected_relative_path: Some("多表公式.xlsx"),
    },
    GateACase {
        name: "local_pptx_notes_and_image",
        route: "office.modern.pptx",
        engine_id: "builtin.office-pptx",
        kind: ImportInputKind::File,
        display_name: "演示与备注.pptx",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/notes-and-image.pptx"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Slide",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("演示与备注.pptx"),
    },
    GateACase {
        name: "local_pptx_second_slide_image",
        route: "office.modern.pptx",
        engine_id: "builtin.office-pptx",
        kind: ImportInputKind::File,
        display_name: "第二页图片.pptx",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/image-on-second-slide.pptx"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "![Slide 2 image](",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("第二页图片.pptx"),
    },
    GateACase {
        name: "local_text_pdf",
        route: "pdf.text",
        engine_id: "builtin.pdf-text",
        kind: ImportInputKind::File,
        display_name: "文本页.pdf",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/text-only.pdf"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "trustworthy native text layer",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("文本页.pdf"),
    },
    GateACase {
        name: "local_mixed_pdf_selective_ocr",
        route: "pdf.text",
        engine_id: "builtin.pdf-text",
        kind: ImportInputKind::File,
        display_name: "混合文本扫描.pdf",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/mixed-text-scan.pdf"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: true,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "OCR",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("混合文本扫描.pdf"),
    },
    GateACase {
        name: "local_image_text_ocr",
        route: "ocr.cjk-accurate",
        engine_id: "gate-a-fixture-ocr",
        kind: ImportInputKind::File,
        display_name: "图片文字.png",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/image-with-text.png"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: true,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "OCR",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: None,
    },
    GateACase {
        name: "local_standalone_srt",
        route: "media.subtitle",
        engine_id: "builtin.local-subtitle",
        kind: ImportInputKind::File,
        display_name: "字幕.srt",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/matrix/subtitle.srt"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Matrix SRT",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("字幕.srt"),
    },
    GateACase {
        name: "local_standalone_vtt",
        route: "media.subtitle",
        engine_id: "builtin.local-subtitle",
        kind: ImportInputKind::File,
        display_name: "字幕.vtt",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/matrix/subtitle.vtt"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Matrix VTT",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("字幕.vtt"),
    },
    GateACase {
        name: "local_standalone_ass",
        route: "media.subtitle",
        engine_id: "builtin.local-subtitle",
        kind: ImportInputKind::File,
        display_name: "字幕.ass",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/matrix/subtitle.ass"
        )),
        local_assets: &[],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Matrix ASS",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("字幕.ass"),
    },
    GateACase {
        name: "local_speech_audio_with_srt_companion",
        route: "media.companion",
        engine_id: "builtin.local-media-companion",
        kind: ImportInputKind::File,
        display_name: "speech.wav",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/speech.wav"
        )),
        local_assets: &[LocalFixtureAsset {
            path: "speech.srt",
            bytes: include_bytes!("../../tests/fixtures/import-v2/local/batch3/companion-srt.srt"),
        }],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Companion SRT transcript",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("speech.wav"),
    },
    GateACase {
        name: "local_media_vtt_companion",
        route: "media.companion",
        engine_id: "builtin.local-media-companion",
        kind: ImportInputKind::File,
        display_name: "companion-vtt.wav",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/companion-vtt.wav"
        )),
        local_assets: &[LocalFixtureAsset {
            path: "companion-vtt.vtt",
            bytes: include_bytes!("../../tests/fixtures/import-v2/local/batch3/companion-vtt.vtt"),
        }],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Companion VTT transcript",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("companion-vtt.wav"),
    },
    GateACase {
        name: "local_media_ass_companion",
        route: "media.companion",
        engine_id: "builtin.local-media-companion",
        kind: ImportInputKind::File,
        display_name: "companion-ass.wav",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/companion-ass.wav"
        )),
        local_assets: &[LocalFixtureAsset {
            path: "companion-ass.ass",
            bytes: include_bytes!("../../tests/fixtures/import-v2/local/batch3/companion-ass.ass"),
        }],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Companion ASS transcript",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("companion-ass.wav"),
    },
    GateACase {
        name: "local_video_with_companion_subtitle",
        route: "media.companion",
        engine_id: "builtin.local-media-companion",
        kind: ImportInputKind::File,
        display_name: "video.mp4",
        locator: "",
        local_bytes: Some(include_bytes!(
            "../../tests/fixtures/import-v2/local/batch3/matrix/media.mp4"
        )),
        local_assets: &[LocalFixtureAsset {
            path: "video.srt",
            bytes: include_bytes!(
                "../../tests/fixtures/import-v2/local/batch3/matrix/subtitle.srt"
            ),
        }],
        web_responses: &[],
        needs_local_ocr: false,
        expected_source_kind: "local_document",
        expected_wiki_prefix: "wiki/sources/local/",
        body_marker: "Matrix SRT",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: Some("video.mp4"),
    },
    GateACase {
        name: "ordinary_web_page",
        route: "web.generic.readability",
        engine_id: "gate-a-production-generic",
        kind: ImportInputKind::Url,
        display_name: "普通网页夹具",
        locator: "https://example.com/articles/gate-a",
        local_bytes: None,
        local_assets: &[],
        web_responses: &[WebFixtureResponse {
            url: "https://example.com/articles/gate-a",
            content_type: "text/html; charset=utf-8",
            bytes: include_bytes!("../../tests/fixtures/import-v2/web/generic/article.html"),
        }],
        needs_local_ocr: false,
        expected_source_kind: "web_page",
        expected_wiki_prefix: "wiki/sources/web/example.com/",
        body_marker: "无需登录",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: None,
    },
    GateACase {
        name: "bilibili_with_subtitles",
        route: "web.bilibili.video",
        engine_id: "gate-a-production-bilibili",
        kind: ImportInputKind::Url,
        display_name: "Gate A 字幕视频",
        locator: "https://www.bilibili.com/video/BV1GateA",
        local_bytes: None,
        local_assets: &[],
        web_responses: &[
            WebFixtureResponse {
                url: "https://www.bilibili.com/video/BV1GateA",
                content_type: "text/html; charset=utf-8",
                bytes: include_bytes!(
                    "../../tests/fixtures/import-v2/web/bilibili/gate-a-video.html"
                ),
            },
            WebFixtureResponse {
                url: "https://aisubtitle.hdslb.com/sub.vtt",
                content_type: "text/vtt; charset=utf-8",
                bytes: include_bytes!("../../tests/fixtures/import-v2/web/bilibili/subtitles.vtt"),
            },
        ],
        needs_local_ocr: false,
        expected_source_kind: "web_media",
        expected_wiki_prefix: "wiki/sources/web/www.bilibili.com/",
        body_marker: "字幕正文必须在提交后仍然可读",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: None,
    },
    GateACase {
        name: "xiaohongshu_image_text",
        route: "web.xiaohongshu.note",
        engine_id: "gate-a-production-xiaohongshu",
        kind: ImportInputKind::Url,
        display_name: "Gate A 图文笔记",
        locator: "https://www.xiaohongshu.com/explore/67f00abc1234",
        local_bytes: None,
        local_assets: &[],
        web_responses: &[
            WebFixtureResponse {
                url: "https://www.xiaohongshu.com/explore/67f00abc1234",
                content_type: "text/html; charset=utf-8",
                bytes: include_bytes!(
                    "../../tests/fixtures/import-v2/web/xiaohongshu/image-note.html"
                ),
            },
            WebFixtureResponse {
                url: "https://sns-webpic-qc.xhscdn.com/001.jpg",
                content_type: "image/svg+xml",
                bytes: CJK_ASSET,
            },
            WebFixtureResponse {
                url: "https://sns-webpic-qc.xhscdn.com/002.jpg",
                content_type: "image/svg+xml",
                bytes: CJK_ASSET,
            },
        ],
        needs_local_ocr: true,
        expected_source_kind: "web_image_text",
        expected_wiki_prefix: "wiki/sources/web/www.xiaohongshu.com/",
        body_marker: "产品流程说明",
        portable_asset_links: &[],
        expected_package_members: None,
        package_body_marker: None,
        expected_relative_path: None,
    },
];

struct FixtureWebArtifactSource {
    responses: &'static [WebFixtureResponse],
}

impl WebArtifactSource for FixtureWebArtifactSource {
    fn fetch(
        &self,
        target: SessionWebTarget,
        _policy: WebFetchPolicy,
        _private_grant: Option<&PrivateTargetGrant>,
        _item_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<WebFetchArtifact, BackendError> {
        if cancellation.is_cancelled() {
            return Err(BackendError::new(
                "IMPORT_V2_CANCELLED",
                "The Gate A fixture fetch was cancelled.",
                false,
                false,
            ));
        }
        let response = self
            .responses
            .iter()
            .find(|response| same_web_resource(response.url, target.request_url.as_str()))
            .ok_or_else(|| {
                BackendError::new(
                    "IMPORT_V2_FIXTURE_MISSING",
                    format!(
                        "Gate A has no deterministic response for {}.",
                        target.public.public_url
                    ),
                    false,
                    false,
                )
            })?;
        Ok(WebFetchArtifact {
            bytes: response.bytes.to_vec(),
            byte_len: response.bytes.len() as u64,
            final_public_url: target.public.public_url.clone(),
            final_session_target: target,
            content_type: response.content_type.into(),
            sanitized_headers: BTreeMap::new(),
            redirects: Vec::new(),
            elapsed_ms: 0,
        })
    }

    fn supports_live_platform_api(&self) -> bool {
        false
    }
}

struct FixtureOcrEngine;

impl ImportEngine for FixtureOcrEngine {
    fn descriptor(&self) -> EngineDescriptor {
        EngineDescriptor {
            engine_id: "gate-a-fixture-ocr".into(),
            engine_version: "1.0.0".into(),
            route: "ocr.cjk-accurate".into(),
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
        if cancellation.is_cancelled() {
            return Err(BackendError::new(
                "IMPORT_V2_CANCELLED",
                "The Gate A OCR fixture was cancelled.",
                false,
                false,
            ));
        }
        let workspace = PathBuf::from(&request.project_root).join(&request.staging_root);
        std::fs::copy(&request.input.locator, workspace.join("source.bin"))
            .and_then(|_| std::fs::write(workspace.join("ocr.md"), XIAOHONGSHU_OCR))
            .and_then(|_| {
                std::fs::write(
                    workspace.join("ocr-metadata.json"),
                    br#"{"confidence":0.99,"blocks":[{"confidence":0.99,"coordinates":{"x":0,"y":0,"width":100,"height":100}}]}"#,
                )
            })
            .map_err(|error| {
                BackendError::new(
                    "IMPORT_V2_FIXTURE_WRITE_FAILED",
                    error.to_string(),
                    false,
                    false,
                )
            })?;
        Ok(EngineResult {
            source_snapshot_path: "source.bin".into(),
            markdown_path: "ocr.md".into(),
            asset_paths: Vec::new(),
            metadata_path: Some("ocr-metadata.json".into()),
            title: "Gate A OCR".into(),
            text_coverage: Some(1.0),
            table_cell_accuracy: None,
            sheet_count_exact: None,
            slide_count_exact: None,
            non_empty_cell_coverage: None,
            formula_value_pairs: None,
            meaningful_image_coverage: Some(1.0),
            continuation: None,
            warnings: Vec::new(),
        })
    }
}

#[test]
fn gate_a_success_contract_is_table_driven_for_every_supported_input() {
    for case in GATE_A_CASES {
        run_gate_a_case(case.clone());
    }
}

#[test]
fn real_collection_selection_runs_children_through_candidate_and_commit_in_source_order() {
    const COLLECTION_URL: &str = "https://www.bilibili.com/medialist/play/123";
    const COLLECTION_RESPONSES: &[WebFixtureResponse] = &[
        WebFixtureResponse {
            url: "https://www.bilibili.com/video/BV1first",
            content_type: "text/html; charset=utf-8",
            bytes: br#"<script>window.__INITIAL_STATE__={"data":{"bvid":"BV1first","title":"First fixture lesson","owner":{"name":"Fixture UP"},"subtitles":[{"url":"https://aisubtitle.hdslb.com/sub.vtt","automatic":false,"language":"zh-CN"}]}};</script>"#,
        },
        WebFixtureResponse {
            url: "https://www.bilibili.com/video/BV2second",
            content_type: "text/html; charset=utf-8",
            bytes: br#"<script>window.__INITIAL_STATE__={"data":{"bvid":"BV2second","title":"Second fixture lesson","owner":{"name":"Fixture UP"},"subtitles":[{"url":"https://aisubtitle.hdslb.com/sub.vtt","automatic":false,"language":"zh-CN"}]}};</script>"#,
        },
        WebFixtureResponse {
            url: "https://aisubtitle.hdslb.com/sub.vtt",
            content_type: "text/vtt; charset=utf-8",
            bytes: include_bytes!("../../tests/fixtures/import-v2/web/bilibili/subtitles.vtt"),
        },
    ];

    let root = std::env::temp_dir().join(format!(
        "import-v2-gate-a-collection-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new("gate-a-collection", root.clone());
    let files = FileStore;
    let git = GitService;
    let tasks = TaskService::default();
    let secrets = SecretService::memory();
    let service = ImportV2Service::with_secret_service(secrets.clone());
    service
        .register_engine(Arc::new(GenericWebEngine::new_with_artifact_source(
            Arc::new(WebTargetStore::new(secrets)),
            "gate-a-collection-bilibili",
            "web.bilibili.video",
            Arc::new(FixtureWebArtifactSource {
                responses: COLLECTION_RESPONSES,
            }),
        )))
        .unwrap();
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let html = include_str!("../../tests/fixtures/import-v2/web/bilibili/collection.html");
    let collection = extract_platform_collection(Platform::Bilibili, html, COLLECTION_URL).unwrap();
    assert_eq!(collection.items.len(), 3);
    let stored = collection
        .items
        .iter()
        .map(|item| {
            (
                item.title.clone(),
                UrlPolicy.normalize_for_session(&item.url).unwrap(),
                item.discovery_fingerprint.clone(),
            )
        })
        .collect();
    let (collection_ref, page) = service
        .store_web_collection(
            &context.project_id,
            &session.session_id,
            COLLECTION_URL.into(),
            collection.platform.clone(),
            collection.title.clone(),
            stored,
        )
        .unwrap();
    let selected_refs = page
        .items
        .iter()
        .take(2)
        .map(|item| item.item_ref.clone())
        .collect::<Vec<_>>();
    let selection = service
        .resolve_web_collection_selection(
            &collection_ref,
            &context.project_id,
            &session.session_id,
            &selected_refs,
        )
        .unwrap();
    assert_eq!(
        selection
            .targets
            .iter()
            .map(|item| item.target.public.public_url.as_str())
            .collect::<Vec<_>>(),
        [
            "https://www.bilibili.com/video/BV1first",
            "https://www.bilibili.com/video/BV2second",
        ]
    );
    let inputs = selection
        .targets
        .into_iter()
        .map(|selected| {
            let target = selected.target;
            let locator = service.store_web_target(&target).unwrap();
            CollectionImportInput {
                input: ImportInput {
                    kind: ImportInputKind::Url,
                    display_name: target.public.host,
                    locator,
                    normalized_locator: Some(target.public.public_url),
                    source_identity: None,
                    media_save_mode: Default::default(),
                },
                discovery_fingerprint: selected.discovery_fingerprint,
            }
        })
        .collect();
    let session = service
        .add_collection_inputs(
            &context,
            &files,
            &session.session_id,
            inputs,
            selection.source_url,
            selection.platform,
            selection.title,
        )
        .unwrap();
    assert_eq!(session.items.len(), 2);
    for item in &session.items {
        let task = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                root.clone(),
                format!("Gate A collection {}", item.item_id),
                true,
            )
            .unwrap();
        let processed = service
            .run_item(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &item.item_id,
                &task.id,
            )
            .unwrap();
        assert_eq!(processed.status, ImportItemStatus::PreviewReady);
        assert!(processed.attempts.iter().any(|attempt| {
            attempt.route == "web.bilibili.video"
                && attempt.engine_id == "gate-a-collection-bilibili"
        }));
    }
    let processed = service
        .load_session(&context, &files, &session.session_id)
        .unwrap();
    let decisions = processed
        .items
        .iter()
        .map(|item| CommitItemDecision {
            item_id: item.item_id.clone(),
            resolution: item
                .preview
                .as_ref()
                .and_then(|preview| preview.resolution.as_ref())
                .and_then(|resolution| resolution.default_resolution.clone()),
        })
        .collect();
    let batch = service
        .commit_items(
            &context,
            &files,
            &git,
            &CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: root.to_string_lossy().into(),
                session_id: session.session_id,
                batch_task_id: None,
                acknowledge_restricted_content: false,
                decisions,
            },
        )
        .unwrap();
    assert_eq!(batch.committed_count, 2, "{batch:?}");
    assert_eq!(batch.failed_count, 0, "{batch:?}");
    assert_eq!(batch.items.len(), 2);
    for item in &batch.items {
        let wiki_path = item.wiki_path.as_deref().unwrap();
        let source = std::fs::read_to_string(root.join(wiki_path)).unwrap();
        assert!(source.contains("字幕正文必须在提交后仍然可读"));
        assert!(item.source_id.is_some());
        assert!(item.version_id.is_some());
    }
    assert!(root
        .join(format!(".app/import-history/{}.json", batch.batch_id))
        .is_file());
    assert!(!root.join(".app/compile").exists());
    std::fs::remove_dir_all(root).ok();
}

fn run_gate_a_case(case: GateACase) {
    let root = std::env::temp_dir().join(format!(
        "import-v2-gate-a-{}-{}",
        case.name,
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(root.join(".app")).unwrap();
    let context = ProjectContext::new(format!("gate-a-{}", case.name), root.clone());
    let files = FileStore;
    let git = GitService;
    let tasks = TaskService::default();
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    configure_case_engines(&service, &case);

    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = if case.kind == ImportInputKind::ClipboardText {
        service
            .add_text_input(
                &context,
                &files,
                &session.session_id,
                case.display_name,
                std::str::from_utf8(case.local_bytes.unwrap()).unwrap(),
            )
            .unwrap()
    } else {
        let input = create_case_input(&root, &case);
        service
            .add_inputs(&context, &files, &session.session_id, vec![input])
            .unwrap()
    };
    let clipboard_session_copy = (case.kind == ImportInputKind::ClipboardText).then(|| {
        context
            .resolve_project_path(&session.items[0].input.locator)
            .unwrap()
    });
    let item_id = session.items[0].item_id.clone();
    assert_eq!(session.items[0].input.kind, case.kind, "{}", case.name);
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            root.clone(),
            format!("Gate A {}", case.name),
            true,
        )
        .unwrap();
    let first_result = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &item_id,
            &task.id,
        )
        .unwrap_or_else(|error| panic!("{}: run failed: {error:?}", case.name));
    if case.needs_local_ocr {
        assert_eq!(
            first_result.status,
            ImportItemStatus::WaitingAuthorization,
            "{}: OCR must stop for explicit session authorization",
            case.name
        );
        service
            .authorize_media_for_session(
                &context,
                &files,
                &session.session_id,
                &item_id,
                ImportMediaAuthorizationKind::Ocr,
                None,
                None,
            )
            .unwrap();
        assert!(
            service
                .load_session(&context, &files, &session.session_id)
                .unwrap()
                .has_media_authorization(&item_id, ImportMediaAuthorizationKind::Ocr),
            "{}: OCR authorization was not persisted",
            case.name
        );
        let authorized_task = tasks
            .create_project_task(
                TaskType::Import,
                context.project_id.clone(),
                root.clone(),
                format!("Gate A {} authorized OCR", case.name),
                true,
            )
            .unwrap();
        service
            .run_item(
                &context,
                &files,
                &tasks,
                &session.session_id,
                &item_id,
                &authorized_task.id,
            )
            .unwrap_or_else(|error| panic!("{}: authorized OCR run failed: {error:?}", case.name));
    }

    let processed = service
        .load_session(&context, &files, &session.session_id)
        .unwrap();
    let processed_item = processed
        .items
        .iter()
        .find(|item| item.item_id == item_id)
        .unwrap();
    assert_eq!(
        processed_item.status,
        ImportItemStatus::PreviewReady,
        "{}: item did not reach a committable preview: {:?}",
        case.name,
        (processed_item.issue.as_ref(), &processed_item.attempts)
    );
    assert!(
        processed_item
            .attempts
            .iter()
            .any(|attempt| attempt.route == case.route && attempt.engine_id == case.engine_id),
        "{}: production route was not executed: {:?}",
        case.name,
        processed_item.attempts
    );
    let resolution = processed_item
        .preview
        .as_ref()
        .and_then(|preview| preview.resolution.as_ref())
        .and_then(|resolution| resolution.default_resolution.clone());
    let batch = service
        .commit_items(
            &context,
            &files,
            &git,
            &CommitImportSessionRequest {
                project_id: context.project_id.clone(),
                project_root_path: root.to_string_lossy().into(),
                session_id: session.session_id.clone(),
                batch_task_id: None,
                acknowledge_restricted_content: false,
                decisions: vec![CommitItemDecision {
                    item_id: item_id.clone(),
                    resolution,
                }],
            },
        )
        .unwrap();

    assert_eq!(batch.committed_count, 1, "{}: {batch:?}", case.name);
    assert_eq!(batch.failed_count, 0, "{}: {batch:?}", case.name);
    let result = &batch.items[0];
    assert!(result.committed, "{}: {result:?}", case.name);
    let source_id = result.source_id.as_deref().unwrap();
    let version_id = result.version_id.as_deref().unwrap();
    let wiki_path = result.wiki_path.as_deref().unwrap();
    assert!(
        wiki_path.starts_with(case.expected_wiki_prefix),
        "{}: {wiki_path}",
        case.name
    );

    let manifest =
        SourceRegistry::read_manifest(&context, &files, &format!(".app/sources/{source_id}.json"))
            .unwrap();
    SourceRegistry::validate_manifest_contract(&manifest).unwrap();
    assert_eq!(manifest.schema_version, 3);
    assert_eq!(manifest.current_version_id, version_id);
    assert_eq!(manifest.wiki_path, wiki_path);
    assert_eq!(manifest.source_kind, case.expected_source_kind);
    if case.kind == ImportInputKind::ClipboardText {
        assert!(manifest
            .origins
            .iter()
            .all(|origin| origin.starts_with("clipboard:sha256:")));
        assert!(!serde_json::to_string(&manifest)
            .unwrap()
            .contains("隐私边界内的正文"));
        assert!(
            !clipboard_session_copy.unwrap().exists(),
            "confirmed clipboard input retained its session copy"
        );
    }
    let version = manifest
        .versions
        .iter()
        .find(|version| version.version_id == version_id)
        .unwrap();
    assert!(!version.raw_evidence.is_empty());
    for record in version.raw_evidence.iter().chain(&version.assets) {
        assert!(
            root.join(record.path.replace('/', std::path::MAIN_SEPARATOR_STR))
                .is_file(),
            "{}: missing {}",
            case.name,
            record.path
        );
    }
    if let Some(expected_relative_path) = case.expected_relative_path {
        let relative_paths = version
            .raw_evidence
            .iter()
            .filter(|record| record.kind == "metadata")
            .filter_map(|metadata| {
                let bytes = std::fs::read(
                    root.join(metadata.path.replace('/', std::path::MAIN_SEPARATOR_STR)),
                )
                .ok()?;
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()?
                    .get("relativePath")?
                    .as_str()
                    .map(str::to_string)
            })
            .collect::<Vec<_>>();
        assert!(
            relative_paths
                .iter()
                .any(|path| path == expected_relative_path),
            "{}: folder-relative identity was not preserved",
            case.name
        );
    }
    let final_source = std::fs::read_to_string(root.join(wiki_path)).unwrap();
    let (frontmatter, body) = parse_final_source(&final_source).unwrap();
    assert_eq!(frontmatter.source_id, source_id);
    assert_eq!(frontmatter.version_id, version_id);
    assert_eq!(frontmatter.content_hash, version.content_hash);
    assert_eq!(frontmatter.source_kind, manifest.source_kind);
    assert_eq!(frontmatter.title, manifest.title);
    assert!(body.contains(case.body_marker), "{}: {body}", case.name);
    for asset_link in case.portable_asset_links {
        assert!(
            final_source.contains(asset_link),
            "{}: Source lost portable asset link {asset_link}",
            case.name
        );
        let resolved =
            SourceRegistry::resolve_wiki_asset_path(&context, &files, wiki_path, asset_link)
                .unwrap();
        assert!(
            resolved.is_file(),
            "{}: portable asset did not resolve: {}",
            case.name,
            resolved.display()
        );
    }
    for forbidden in ["cookie", "token", "staging", "sessionId", "engineId"] {
        assert!(
            !final_source.contains(forbidden),
            "{}: final Source leaked {forbidden}",
            case.name
        );
    }
    for forbidden in ["<script", "onload=", "<iframe", "steal()"] {
        assert!(
            !final_source.to_ascii_lowercase().contains(forbidden),
            "{}: unsafe local HTML survived sanitization: {forbidden}",
            case.name
        );
    }
    if case.route == "office.modern.pptx" {
        assert!(
            !version.assets.is_empty(),
            "{}: meaningful PPTX image was not retained",
            case.name
        );
    }
    if case.name == "local_pptx_notes_and_image" {
        assert!(final_source.contains("preserve this note beside slide one"));
    }
    match case.expected_package_members {
        Some(expected_count) => {
            let package_record = version
                .raw_evidence
                .iter()
                .find(|record| record.kind == "source_package_manifest")
                .unwrap_or_else(|| panic!("{}: package descriptor is missing", case.name));
            let package: SourcePackageManifest = serde_json::from_slice(
                &std::fs::read(
                    root.join(
                        package_record
                            .path
                            .replace('/', std::path::MAIN_SEPARATOR_STR),
                    ),
                )
                .unwrap(),
            )
            .unwrap();
            package.validate_committed().unwrap();
            assert_eq!(package.schema_version, 1);
            assert_eq!(package.source_id, source_id);
            assert_eq!(package.version_id, version_id);
            assert_eq!(package.entry_wiki_path, wiki_path);
            assert_eq!(package.members.len(), expected_count);
            let package_text = package
                .members
                .iter()
                .map(|member| {
                    assert!(root
                        .join(member.wiki_path.replace('/', std::path::MAIN_SEPARATOR_STR))
                        .is_file());
                    assert!(root
                        .join(
                            member
                                .baseline_path
                                .replace('/', std::path::MAIN_SEPARATOR_STR)
                        )
                        .is_file());
                    std::fs::read_to_string(
                        root.join(member.wiki_path.replace('/', std::path::MAIN_SEPARATOR_STR)),
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>()
                .join("\n");
            if let Some(marker) = case.package_body_marker {
                assert!(
                    package_text.contains(marker),
                    "{}: package lost marker {marker}",
                    case.name
                );
            }
            if case.route == "office.modern.xlsx" {
                assert!(
                    !package_text.contains("`="),
                    "{}: formulas leaked into readable Sheet pages",
                    case.name
                );
                let evidence = version
                    .raw_evidence
                    .iter()
                    .filter(|record| record.kind == "source_evidence")
                    .map(|record| {
                        std::fs::read_to_string(
                            root.join(record.path.replace('/', std::path::MAIN_SEPARATOR_STR)),
                        )
                        .unwrap()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    evidence.contains("\"formula\": \"=B2*C2\""),
                    "{}: formula evidence was not retained",
                    case.name
                );
            }
        }
        None => assert!(
            version
                .raw_evidence
                .iter()
                .all(|record| record.kind != "source_package_manifest"),
            "{}: single-page input unexpectedly became a Source package",
            case.name
        ),
    }
    assert!(root
        .join(format!(".app/import-history/{}.json", batch.batch_id))
        .is_file());
    assert!(!root.join(".app/compile").exists());

    let staging = root.join(format!(
        ".app/import-sessions/{}/items/{item_id}/staging",
        session.session_id
    ));
    std::fs::remove_dir_all(staging).unwrap();
    assert!(std::fs::read_to_string(root.join(wiki_path))
        .unwrap()
        .contains(case.body_marker));
    std::fs::remove_dir_all(root).ok();
}

fn configure_case_engines(service: &ImportV2Service, case: &GateACase) {
    if case.kind == ImportInputKind::Url {
        let source = Arc::new(FixtureWebArtifactSource {
            responses: case.web_responses,
        });
        service
            .register_engine(Arc::new(GenericWebEngine::new_with_artifact_source(
                Arc::new(WebTargetStore::new(SecretService::memory())),
                case.engine_id,
                case.route,
                source,
            )))
            .unwrap();
    }
    if case.needs_local_ocr {
        service.register_engine(Arc::new(FixtureOcrEngine)).unwrap();
    }
}

fn create_case_input(root: &std::path::Path, case: &GateACase) -> ImportInput {
    if case.kind == ImportInputKind::Url {
        let target = UrlPolicy.normalize_for_session(case.locator).unwrap();
        return ImportInput {
            source_identity: None,
            kind: ImportInputKind::Url,
            display_name: case.display_name.into(),
            locator: target.request_url.to_string(),
            normalized_locator: Some(target.public.public_url),
            media_save_mode: Default::default(),
        };
    }

    assert_ne!(case.kind, ImportInputKind::ClipboardText);
    let input_path = root.join("inputs").join(case.display_name);
    std::fs::create_dir_all(input_path.parent().unwrap()).unwrap();
    std::fs::write(&input_path, case.local_bytes.unwrap()).unwrap();
    for asset in case.local_assets {
        let asset_path = input_path.parent().unwrap().join(asset.path);
        std::fs::create_dir_all(asset_path.parent().unwrap()).unwrap();
        std::fs::write(asset_path, asset.bytes).unwrap();
    }
    let source_bytes = std::fs::read(&input_path).unwrap();
    let canonical_path = input_path.canonicalize().unwrap();
    ImportInput {
        source_identity: Some(SourceIdentity {
            canonical_path: canonical_path.to_string_lossy().into_owned(),
            size_bytes: source_bytes.len() as u64,
            modified_nanos: None,
            file_id: None,
            sha256: format!("{:x}", Sha256::digest(&source_bytes)),
            magic: format!(
                "{:x}",
                Sha256::digest(&source_bytes[..source_bytes.len().min(8192)])
            ),
        }),
        kind: ImportInputKind::File,
        display_name: case.display_name.into(),
        locator: input_path.to_string_lossy().into(),
        normalized_locator: Some(format!(
            "file:{}",
            input_path.to_string_lossy().replace('\\', "/")
        )),
        media_save_mode: Default::default(),
    }
}

fn same_web_resource(expected: &str, actual: &str) -> bool {
    let Ok(expected) = Url::parse(expected) else {
        return false;
    };
    let Ok(actual) = Url::parse(actual) else {
        return false;
    };
    expected.scheme() == actual.scheme()
        && expected.host_str() == actual.host_str()
        && expected.port_or_known_default() == actual.port_or_known_default()
        && expected.path() == actual.path()
}
