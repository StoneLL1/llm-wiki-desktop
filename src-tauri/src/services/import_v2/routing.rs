//! Route ownership: platform targets and local formats share one ordered policy.
use super::file_router::QualityFloor;
use crate::errors::BackendError;
use crate::models::{
    import_v2::{ImportInput, ImportInputKind, ImportRecoveryAction},
    import_v2_file::FileFormat,
    paths::ProjectContext,
};
use std::path::Path;

pub(super) fn reorder_routes(
    mut routes: Vec<(&'static str, QualityFloor)>,
    recovery_action: Option<&ImportRecoveryAction>,
) -> Vec<(&'static str, QualityFloor)> {
    match recovery_action {
        Some(ImportRecoveryAction::SwitchRoute) if routes.len() > 1 => routes.rotate_left(1),
        Some(ImportRecoveryAction::SwitchParser) if routes.len() > 1 => routes.rotate_left(1),
        Some(ImportRecoveryAction::EnableOcr) => {
            routes.sort_by_key(|(route, _)| {
                if *route == "pdf.text" {
                    0
                } else if route.starts_with("ocr.") {
                    1
                } else if *route == "pdf.layout" {
                    2
                } else {
                    3
                }
            });
        }
        Some(ImportRecoveryAction::RetryRoute) | _ => {}
    }
    routes
}

pub(super) fn detect_input_format(
    context: &ProjectContext,
    input: &ImportInput,
) -> Result<Option<FileFormat>, BackendError> {
    if input.kind == ImportInputKind::Url {
        return Ok(None);
    }
    let locator = Path::new(&input.locator);
    let path = if locator.is_absolute() {
        locator.to_path_buf()
    } else {
        context.root.join(locator)
    };
    let prefix = std::fs::File::open(&path)
        .and_then(|file| {
            use std::io::Read;
            let mut bytes = Vec::new();
            file.take(8192).read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .map_err(|error| {
            BackendError::new(
                "IMPORT_FILE_IO",
                format!("The selected source could not be inspected: {error}"),
                true,
                true,
            )
        })?;
    crate::services::import_v2::file_discovery::identify_file(&path, &prefix)
        .map(|(format, _)| Some(format))
}

/// Canonical Batch 3 route contract. Discovery and contract tests share this
/// function so adding a supported local format cannot silently diverge from
/// the production orchestrator.
pub fn routes_for_format(format: FileFormat) -> Vec<&'static str> {
    match format {
        FileFormat::Markdown | FileFormat::Text | FileFormat::Html => vec!["file.native"],
        FileFormat::Csv => vec!["file.csv-package"],
        FileFormat::Docx => vec![
            "office.modern.docx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        FileFormat::Xlsx => vec![
            "office.modern.xlsx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        FileFormat::Pptx => vec![
            "office.modern.pptx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        FileFormat::Doc | FileFormat::Xls | FileFormat::Ppt => {
            vec![
                "pack.office-legacy",
                "pack.markitdown",
                "pack.office-oxide",
                "agent.office",
            ]
        }
        FileFormat::Pdf => vec![
            "pdf.text",
            "pdf.layout",
            "ocr.cjk-accurate",
            "ocr.basic",
            "agent.pdf",
        ],
        FileFormat::Srt | FileFormat::Vtt | FileFormat::Ass | FileFormat::Lrc => {
            vec!["media.subtitle"]
        }
        FileFormat::Mp3
        | FileFormat::Wav
        | FileFormat::M4a
        | FileFormat::Aac
        | FileFormat::Flac
        | FileFormat::Ogg
        | FileFormat::Opus
        | FileFormat::Wma
        | FileFormat::Mp4
        | FileFormat::Mov
        | FileFormat::Mkv
        | FileFormat::Webm
        | FileFormat::Avi
        | FileFormat::M4v
        | FileFormat::Wmv
        | FileFormat::AnimatedGif => {
            vec![
                "media.companion",
                "media.subtitle",
                "media.keyframes",
                "media.asr",
            ]
        }
        FileFormat::Png
        | FileFormat::Jpeg
        | FileFormat::Webp
        | FileFormat::Bmp
        | FileFormat::Tiff
        | FileFormat::Heic
        | FileFormat::Heif => vec!["ocr.cjk-accurate", "ocr.basic"],
    }
}

pub(super) fn explicit_routes(input: &ImportInput) -> Vec<&'static str> {
    if input.kind == crate::models::import_v2::ImportInputKind::Url {
        let host = url::Url::parse(
            input
                .normalized_locator
                .as_deref()
                .unwrap_or(&input.locator),
        )
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
        if host == "xiaohongshu.com"
            || host.ends_with(".xiaohongshu.com")
            || host == "xhslink.com"
            || host.ends_with(".xhslink.com")
            || host == "xhslink.cn"
            || host.ends_with(".xhslink.cn")
        {
            return vec![
                "web.xiaohongshu.note",
                "web.generic.readability",
                "web.generic.browser",
            ];
        }
        if host == "douyin.com"
            || host.ends_with(".douyin.com")
            || host == "iesdouyin.com"
            || host.ends_with(".iesdouyin.com")
        {
            return vec![
                "web.douyin.video",
                "web.generic.readability",
                "web.generic.browser",
            ];
        }
        if host == "x.com"
            || host.ends_with(".x.com")
            || host == "twitter.com"
            || host.ends_with(".twitter.com")
        {
            return vec!["web.x.post"];
        }
        if host == "bilibili.com" || host.ends_with(".bilibili.com") || host == "b23.tv" {
            return vec![
                "web.bilibili.video",
                "web.bilibili.metadata",
                "web.generic.browser",
            ];
        }
        let platform = if host == "mp.weixin.qq.com" {
            Some("web.wechat.article")
        } else if host == "zhihu.com" || host.ends_with(".zhihu.com") {
            Some("web.zhihu.content")
        } else {
            None
        };
        let mut routes = platform.into_iter().collect::<Vec<_>>();
        routes.extend(["web.generic.readability", "web.generic.browser"]);
        return routes;
    }
    let extension = Path::new(&input.locator)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "md" | "markdown" | "txt" | "html" | "htm" => vec!["file.native"],
        "csv" => vec!["file.csv-package"],
        "docx" => vec![
            "office.modern.docx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        "xlsx" => vec![
            "office.modern.xlsx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        "pptx" => vec![
            "office.modern.pptx",
            "pack.markitdown",
            "pack.office-oxide",
            "agent.office",
        ],
        "doc" | "xls" | "ppt" => vec!["pack.office-legacy", "pack.office-oxide", "agent.office"],
        "pdf" => vec![
            "pdf.text",
            "pdf.layout",
            "ocr.cjk-accurate",
            "ocr.basic",
            "agent.pdf",
        ],
        "srt" | "vtt" | "lrc" | "ass" | "ssa" => vec!["media.subtitle"],
        "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" | "opus" | "wma" | "mp4" | "mov" | "mkv"
        | "webm" | "avi" | "m4v" | "wmv" | "gif" => {
            vec!["media.companion", "media.asr"]
        }
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" => {
            vec!["ocr.cjk-accurate", "ocr.basic"]
        }
        _ => Vec::new(),
    }
}

pub(super) fn is_bilibili_import_input(input: &ImportInput) -> bool {
    if input.kind != crate::models::import_v2::ImportInputKind::Url {
        return false;
    }
    url::Url::parse(
        input
            .normalized_locator
            .as_deref()
            .unwrap_or(&input.locator),
    )
    .ok()
    .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
    .is_some_and(|host| {
        host == "bilibili.com" || host.ends_with(".bilibili.com") || host == "b23.tv"
    })
}
