use crate::errors::BackendError;
use crate::models::import_v2::{ImportInput, ImportInputKind, ImportSession, SourceIdentity};
use crate::models::import_v2_file::{
    DiscoveredFile, FileDetectionMethod, FileFormat, FileIdentity, FileScanPolicy, FileScanResult,
    FileSkipReason, LargeDataEstimate, SkippedFile,
};
use crate::models::paths::ProjectContext;
use crate::services::import_v2::markdown_normalizer::decode_text;
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const PREFIX_BYTES: u64 = 8192;
const OOXML_MAX_ENTRIES: usize = 4096;
const OOXML_MAX_UNCOMPRESSED: u64 = 64 * 1024 * 1024;
const LARGE_DATA_BYTES: u64 = 8 * 1024 * 1024;
const LARGE_DATA_ROWS: u64 = 10_000;
const LARGE_DATA_ROWS_PER_FILE: u64 = 5_000;

#[derive(Default)]
pub struct FileDiscoveryService;

impl FileDiscoveryService {
    pub fn scan<B, C>(
        &self,
        context: &ProjectContext,
        roots: &[PathBuf],
        policy: FileScanPolicy,
        mut on_batch: B,
        is_cancelled: C,
    ) -> Result<FileScanResult, BackendError>
    where
        B: FnMut(&[DiscoveredFile]),
        C: Fn() -> bool,
    {
        let project_root = fs::canonicalize(&context.root).ok();
        let mut queue = VecDeque::new();
        for root in roots {
            queue.push_back((root.clone(), root.clone(), 0u32));
        }
        let mut result = FileScanResult {
            files: Vec::new(),
            skipped: Vec::new(),
            truncated: false,
        };
        let mut visited_dirs = HashSet::new();
        let mut visited_files = HashSet::new();

        while let Some((path, scan_root, depth)) = queue.pop_front() {
            if is_cancelled() {
                return Err(error(
                    "IMPORT_FILE_SCAN_CANCELLED",
                    "File discovery was cancelled.",
                ));
            }
            let relative = relative_string(&path, &scan_root);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(value) => value,
                Err(err) => {
                    skip(
                        &mut result,
                        &path,
                        relative,
                        FileSkipReason::Unreadable,
                        err.to_string(),
                    );
                    continue;
                }
            };
            if metadata.file_type().is_symlink() || is_windows_reparse(&metadata) {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::SymlinkOrReparsePoint,
                    "Links and reparse points are not followed.".into(),
                );
                continue;
            }
            if metadata.is_dir() && is_ignored_directory(&path) {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::IgnoredDirectory,
                    "Generated dependency and temporary directories are excluded.".into(),
                );
                continue;
            }
            if !policy.include_hidden && is_hidden_or_system(&path, &metadata) {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::HiddenOrSystem,
                    "Hidden and system entries are excluded.".into(),
                );
                continue;
            }
            let canonical = match fs::canonicalize(&path) {
                Ok(value) => value,
                Err(err) => {
                    skip(
                        &mut result,
                        &path,
                        relative,
                        FileSkipReason::InvalidPath,
                        err.to_string(),
                    );
                    continue;
                }
            };
            if project_root
                .as_ref()
                .is_some_and(|root| canonical.starts_with(root))
            {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::ProjectInternal,
                    "Current project content cannot be imported into itself.".into(),
                );
                continue;
            }
            if metadata.is_dir() {
                if !visited_dirs.insert(path_key(&canonical)) {
                    skip(
                        &mut result,
                        &path,
                        relative,
                        FileSkipReason::CycleDetected,
                        "Directory was already visited.".into(),
                    );
                    continue;
                }
                if depth > policy.max_depth {
                    result.truncated = true;
                    skip(
                        &mut result,
                        &path,
                        relative,
                        FileSkipReason::DepthLimitExceeded,
                        format!("Maximum depth is {}.", policy.max_depth),
                    );
                    continue;
                }
                let mut children = match fs::read_dir(&path) {
                    Ok(entries) => entries
                        .filter_map(Result::ok)
                        .map(|entry| entry.path())
                        .collect::<Vec<_>>(),
                    Err(err) => {
                        skip(
                            &mut result,
                            &path,
                            relative,
                            FileSkipReason::Unreadable,
                            err.to_string(),
                        );
                        continue;
                    }
                };
                children.sort_by_key(|child| path_key(child));
                for child in children {
                    queue.push_back((child, scan_root.clone(), depth + 1));
                }
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            let key = canonical.to_string_lossy().into_owned();
            if !visited_files.insert(key.clone()) {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::Duplicate,
                    "File was selected more than once.".into(),
                );
                continue;
            }
            if metadata.len() > policy.max_file_bytes {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::FileTooLarge,
                    format!("File is larger than {} bytes.", policy.max_file_bytes),
                );
                continue;
            }
            if result.files.len() >= policy.max_files as usize {
                result.truncated = true;
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::FileLimitExceeded,
                    format!("Maximum file count is {}.", policy.max_files),
                );
                continue;
            }
            let prefix = read_prefix(&path)?;
            let (format, identity) = match identify_file(&path, &prefix) {
                Ok(value) => value,
                Err(err) => {
                    skip(
                        &mut result,
                        &path,
                        relative,
                        FileSkipReason::UnsupportedFormat,
                        err.message,
                    );
                    continue;
                }
            };
            let Some(source_path) = path.to_str().map(str::to_owned) else {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::InvalidPath,
                    "Source path is not valid UTF-8.".into(),
                );
                continue;
            };
            let Some(display_name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned)
            else {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::InvalidPath,
                    "File name is not valid UTF-8.".into(),
                );
                continue;
            };
            let file = DiscoveredFile {
                source_path,
                relative_path: relative.unwrap_or_else(|| display_name.clone()),
                display_name,
                format,
                content_kind: format.content_kind(),
                size_bytes: metadata.len(),
                identity,
                source_identity: source_identity(&canonical, &metadata)?,
                large_data: estimate_large_data(&path, format, metadata.len())?,
            };
            result.files.push(file.clone());
            on_batch(std::slice::from_ref(&file));
        }
        Ok(result)
    }
}

pub fn identify_file(
    path: &Path,
    prefix: &[u8],
) -> Result<(FileFormat, FileIdentity), BackendError> {
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let detected = identify_binary(path, prefix, &extension)?
        .or_else(|| identify_structured_text(prefix, &extension));
    let Some((format, magic, mime, method)) = detected else {
        return Err(error(
            "IMPORT_FILE_UNSUPPORTED",
            "The file content does not identify a supported document or media format.",
        ));
    };
    let mismatch = !extension.is_empty() && !format_accepts_extension(format, &extension);
    Ok((
        format,
        FileIdentity {
            extension,
            magic: magic.into(),
            mime: mime.into(),
            detection_method: method,
            extension_mismatch: mismatch,
        },
    ))
}

pub fn new_import_inputs(
    session: &ImportSession,
    files: impl IntoIterator<Item = DiscoveredFile>,
) -> Vec<ImportInput> {
    let mut seen = session
        .items
        .iter()
        .map(|item| {
            item.input
                .normalized_locator
                .as_deref()
                .unwrap_or(&item.input.locator)
                .to_owned()
        })
        .collect::<HashSet<_>>();
    files
        .into_iter()
        .filter_map(|file| {
            let normalized = normalize_locator(Path::new(&file.source_path));
            seen.insert(normalized.clone()).then_some(ImportInput {
                kind: ImportInputKind::File,
                // Folder imports retain their relative path as the stable visible identity.
                display_name: file.relative_path,
                locator: file.source_path,
                normalized_locator: Some(normalized),
                source_identity: Some(file.source_identity),
                media_save_mode: Default::default(),
            })
        })
        .collect()
}

fn source_identity(path: &Path, metadata: &fs::Metadata) -> Result<SourceIdentity, BackendError> {
    let bytes = fs::read(path).map_err(io_error)?;
    let modified_nanos = metadata.modified().ok().and_then(|value| {
        value
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_nanos())
    });
    Ok(SourceIdentity {
        canonical_path: path.to_string_lossy().into_owned(),
        size_bytes: metadata.len(),
        modified_nanos,
        file_id: file_id(metadata),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        magic: magic_fingerprint(&bytes),
    })
}

fn magic_fingerprint(bytes: &[u8]) -> String {
    format!(
        "{:x}",
        Sha256::digest(&bytes[..bytes.len().min(PREFIX_BYTES as usize)])
    )
}

#[cfg(unix)]
fn file_id(metadata: &fs::Metadata) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    Some(format!("{}:{}", metadata.dev(), metadata.ino()))
}
#[cfg(windows)]
fn file_id(_: &fs::Metadata) -> Option<String> {
    None
}
#[cfg(not(any(unix, windows)))]
fn file_id(_: &fs::Metadata) -> Option<String> {
    None
}

fn normalize_locator(path: &Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical.to_string_lossy().replace('\\', "/")
}

fn identify_binary(
    path: &Path,
    prefix: &[u8],
    extension: &str,
) -> Result<Option<(FileFormat, &'static str, &'static str, FileDetectionMethod)>, BackendError> {
    let magic = |format, label, mime| Some((format, label, mime, FileDetectionMethod::Magic));
    if prefix.starts_with(b"%PDF-") {
        return Ok(magic(FileFormat::Pdf, "pdf", "application/pdf"));
    }
    if prefix.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        let (format, method) = identify_ole(path, extension)?;
        return Ok(Some((format, "ole-cfb", mime_for(format), method)));
    }
    if prefix.starts_with(b"PK\x03\x04")
        || prefix.starts_with(b"PK\x05\x06")
        || prefix.starts_with(b"PK\x07\x08")
    {
        let format = validate_ooxml(path)?;
        return Ok(Some((
            format,
            "ooxml-zip",
            mime_for(format),
            FileDetectionMethod::Container,
        )));
    }
    if prefix.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(magic(FileFormat::Png, "png", "image/png"));
    }
    if prefix.starts_with(&[0xff, 0xd8, 0xff]) {
        return Ok(magic(FileFormat::Jpeg, "jpeg", "image/jpeg"));
    }
    if prefix.starts_with(b"BM") {
        return Ok(magic(FileFormat::Bmp, "bmp", "image/bmp"));
    }
    if prefix.starts_with(b"II*\0") || prefix.starts_with(b"MM\0*") {
        return Ok(magic(FileFormat::Tiff, "tiff", "image/tiff"));
    }
    if prefix.starts_with(b"GIF87a") || prefix.starts_with(b"GIF89a") {
        let bytes = fs::read(path).map_err(io_error)?;
        if is_animated_gif(&bytes) {
            return Ok(magic(FileFormat::AnimatedGif, "animated-gif", "image/gif"));
        }
        return Err(error(
            "IMPORT_FILE_UNSUPPORTED",
            "Static GIF is not in the supported image matrix.",
        ));
    }
    if prefix.len() >= 12 && &prefix[..4] == b"RIFF" {
        return Ok(match &prefix[8..12] {
            b"WEBP" => magic(FileFormat::Webp, "webp", "image/webp"),
            b"WAVE" => magic(FileFormat::Wav, "wave", "audio/wav"),
            b"AVI " => magic(FileFormat::Avi, "avi", "video/x-msvideo"),
            _ => None,
        });
    }
    if prefix.starts_with(b"fLaC") {
        return Ok(magic(FileFormat::Flac, "flac", "audio/flac"));
    }
    if prefix.starts_with(b"OggS") {
        let format = if contains_ascii(prefix, b"OpusHead") {
            FileFormat::Opus
        } else {
            FileFormat::Ogg
        };
        return Ok(magic(format, "ogg", mime_for(format)));
    }
    if looks_like_adts(prefix) {
        return Ok(magic(FileFormat::Aac, "adts", "audio/aac"));
    }
    if prefix.starts_with(b"ID3") || looks_like_mp3_frame(prefix) {
        return Ok(magic(FileFormat::Mp3, "mpeg-audio", "audio/mpeg"));
    }
    if prefix.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        let format = if contains_ascii_case_insensitive(prefix, b"webm") {
            FileFormat::Webm
        } else {
            FileFormat::Mkv
        };
        return Ok(magic(format, "matroska-ebml", mime_for(format)));
    }
    if prefix.starts_with(&[
        0x30, 0x26, 0xb2, 0x75, 0x8e, 0x66, 0xcf, 0x11, 0xa6, 0xd9, 0x00, 0xaa, 0x00, 0x62, 0xce,
        0x6c,
    ]) {
        let format = if extension == "wma" {
            FileFormat::Wma
        } else {
            FileFormat::Wmv
        };
        return Ok(Some((
            format,
            "asf",
            mime_for(format),
            FileDetectionMethod::Container,
        )));
    }
    if prefix.len() >= 12 && &prefix[4..8] == b"ftyp" {
        let brand = &prefix[8..12];
        let format = match brand {
            b"heic" | b"heix" | b"hevc" | b"hevx" => FileFormat::Heic,
            b"heif" | b"heim" | b"heis" | b"mif1" | b"msf1" => FileFormat::Heif,
            b"qt  " => FileFormat::Mov,
            b"M4A " | b"M4B " | b"M4P " => FileFormat::M4a,
            _ if extension == "m4a" => FileFormat::M4a,
            _ if extension == "mov" => FileFormat::Mov,
            _ if extension == "m4v" => FileFormat::M4v,
            _ => FileFormat::Mp4,
        };
        return Ok(Some((
            format,
            "iso-bmff",
            mime_for(format),
            FileDetectionMethod::Container,
        )));
    }
    Ok(None)
}

fn identify_structured_text(
    prefix: &[u8],
    extension: &str,
) -> Option<(FileFormat, &'static str, &'static str, FileDetectionMethod)> {
    let decoded = decode_text(prefix).ok()?;
    let text = decoded.trim_start_matches('\u{feff}').trim_start();
    if text.is_empty() {
        return text_extension_fallback(extension);
    }
    let lower = text
        .chars()
        .take(1024)
        .collect::<String>()
        .to_ascii_lowercase();
    let structured = if text.starts_with("WEBVTT") {
        Some((FileFormat::Vtt, "webvtt"))
    } else if lower.contains("[script info]") || lower.contains("[events]") {
        Some((FileFormat::Ass, "advanced-substation-alpha"))
    } else if looks_like_srt(text) {
        Some((FileFormat::Srt, "subrip"))
    } else if looks_like_lrc(text) {
        Some((FileFormat::Lrc, "lrc"))
    } else if looks_like_html(&lower) {
        Some((FileFormat::Html, "html"))
    } else if looks_like_csv(text) {
        Some((FileFormat::Csv, "delimited-text"))
    } else if looks_like_markdown(text) {
        Some((FileFormat::Markdown, "markdown"))
    } else {
        None
    };
    structured
        .map(|(format, label)| {
            (
                format,
                label,
                mime_for(format),
                FileDetectionMethod::StructuredText,
            )
        })
        .or_else(|| text_extension_fallback(extension))
}

fn text_extension_fallback(
    extension: &str,
) -> Option<(FileFormat, &'static str, &'static str, FileDetectionMethod)> {
    let format = match extension {
        "md" | "markdown" | "mdx" | "mkd" | "mkdn" | "mdown" | "mdwn" | "rmd" => {
            FileFormat::Markdown
        }
        "txt" => FileFormat::Text,
        "csv" => FileFormat::Csv,
        "html" | "htm" => FileFormat::Html,
        "srt" => FileFormat::Srt,
        "vtt" => FileFormat::Vtt,
        "ass" | "ssa" => FileFormat::Ass,
        "lrc" => FileFormat::Lrc,
        _ => return None,
    };
    Some((
        format,
        "utf-8",
        mime_for(format),
        FileDetectionMethod::ExtensionFallback,
    ))
}

fn identify_ole(
    path: &Path,
    extension: &str,
) -> Result<(FileFormat, FileDetectionMethod), BackendError> {
    let bytes = fs::read(path).map_err(io_error)?;
    let format = if contains_ascii_case_insensitive(&bytes, b"workbook")
        || contains_utf16le_ascii(&bytes, "Workbook")
    {
        Some(FileFormat::Xls)
    } else if contains_ascii_case_insensitive(&bytes, b"powerpoint document")
        || contains_utf16le_ascii(&bytes, "PowerPoint Document")
    {
        Some(FileFormat::Ppt)
    } else if contains_ascii_case_insensitive(&bytes, b"worddocument")
        || contains_utf16le_ascii(&bytes, "WordDocument")
    {
        Some(FileFormat::Doc)
    } else {
        None
    };
    if let Some(format) = format {
        return Ok((format, FileDetectionMethod::Container));
    }
    match extension {
        "doc" => Ok((FileFormat::Doc, FileDetectionMethod::ExtensionFallback)),
        "xls" => Ok((FileFormat::Xls, FileDetectionMethod::ExtensionFallback)),
        "ppt" => Ok((FileFormat::Ppt, FileDetectionMethod::ExtensionFallback)),
        _ => Err(error(
            "IMPORT_FILE_AMBIGUOUS_OLE",
            "The legacy Office container type could not be identified safely.",
        )),
    }
}

fn validate_ooxml(path: &Path) -> Result<FileFormat, BackendError> {
    let mut archive = ZipArchive::new(File::open(path).map_err(io_error)?).map_err(|_| {
        error(
            "IMPORT_FILE_INVALID_OOXML",
            "OOXML container is not a valid ZIP archive.",
        )
    })?;
    if archive.len() > OOXML_MAX_ENTRIES {
        return Err(error(
            "IMPORT_FILE_INVALID_OOXML",
            "OOXML archive has too many entries.",
        ));
    }
    let mut total = 0u64;
    let mut content_types = false;
    let mut word_root = false;
    let mut workbook_root = false;
    let mut presentation_root = false;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| {
            error(
                "IMPORT_FILE_INVALID_OOXML",
                "OOXML central directory is invalid.",
            )
        })?;
        total = total.checked_add(entry.size()).ok_or_else(|| {
            error(
                "IMPORT_FILE_INVALID_OOXML",
                "OOXML expanded size overflowed.",
            )
        })?;
        if total > OOXML_MAX_UNCOMPRESSED {
            return Err(error(
                "IMPORT_FILE_INVALID_OOXML",
                "OOXML expanded content is too large.",
            ));
        }
        let name = entry.name();
        let portable_name = name.replace('\\', "/");
        let drive_absolute = portable_name.as_bytes().get(1) == Some(&b':');
        let invalid = portable_name.starts_with('/')
            || drive_absolute
            || portable_name.split('/').any(|part| part == "..")
            || Path::new(&portable_name).components().any(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            });
        if invalid {
            return Err(error(
                "IMPORT_FILE_INVALID_OOXML",
                "OOXML archive contains an unsafe path.",
            ));
        }
        content_types |= name == "[Content_Types].xml";
        word_root |= name == "word/document.xml";
        workbook_root |= name == "xl/workbook.xml";
        presentation_root |= name == "ppt/presentation.xml";
    }
    let roots = [word_root, workbook_root, presentation_root]
        .into_iter()
        .filter(|present| *present)
        .count();
    if !content_types || roots != 1 {
        return Err(error(
            "IMPORT_FILE_INVALID_OOXML",
            "OOXML archive does not contain exactly one supported package root.",
        ));
    }
    Ok(if word_root {
        FileFormat::Docx
    } else if workbook_root {
        FileFormat::Xlsx
    } else {
        FileFormat::Pptx
    })
}

fn mime_for(format: FileFormat) -> &'static str {
    match format {
        FileFormat::Markdown => "text/markdown",
        FileFormat::Text => "text/plain",
        FileFormat::Html => "text/html",
        FileFormat::Csv => "text/csv",
        FileFormat::Doc => "application/msword",
        FileFormat::Docx => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        }
        FileFormat::Xls => "application/vnd.ms-excel",
        FileFormat::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        FileFormat::Ppt => "application/vnd.ms-powerpoint",
        FileFormat::Pptx => {
            "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        }
        FileFormat::Pdf => "application/pdf",
        FileFormat::Png => "image/png",
        FileFormat::Jpeg => "image/jpeg",
        FileFormat::Webp => "image/webp",
        FileFormat::Bmp => "image/bmp",
        FileFormat::Tiff => "image/tiff",
        FileFormat::Heic => "image/heic",
        FileFormat::Heif => "image/heif",
        FileFormat::AnimatedGif => "image/gif",
        FileFormat::Mp3 => "audio/mpeg",
        FileFormat::Wav => "audio/wav",
        FileFormat::M4a => "audio/mp4",
        FileFormat::Aac => "audio/aac",
        FileFormat::Flac => "audio/flac",
        FileFormat::Ogg => "audio/ogg",
        FileFormat::Opus => "audio/opus",
        FileFormat::Wma => "audio/x-ms-wma",
        FileFormat::Mp4 => "video/mp4",
        FileFormat::Mov => "video/quicktime",
        FileFormat::Mkv => "video/x-matroska",
        FileFormat::Webm => "video/webm",
        FileFormat::Avi => "video/x-msvideo",
        FileFormat::M4v => "video/x-m4v",
        FileFormat::Wmv => "video/x-ms-wmv",
        FileFormat::Srt => "application/x-subrip",
        FileFormat::Vtt => "text/vtt",
        FileFormat::Ass => "text/x-ssa",
        FileFormat::Lrc => "text/x-lrc",
    }
}

fn format_accepts_extension(format: FileFormat, extension: &str) -> bool {
    match format {
        FileFormat::Markdown => matches!(
            extension,
            "md" | "markdown" | "mdx" | "mkd" | "mkdn" | "mdown" | "mdwn" | "rmd"
        ),
        FileFormat::Html => matches!(extension, "html" | "htm"),
        FileFormat::Jpeg => matches!(extension, "jpg" | "jpeg"),
        FileFormat::Tiff => matches!(extension, "tif" | "tiff"),
        FileFormat::Ass => matches!(extension, "ass" | "ssa"),
        FileFormat::AnimatedGif => extension == "gif",
        _ => extension == format.canonical_extension(),
    }
}

fn looks_like_html(lower: &str) -> bool {
    lower.starts_with("<!doctype html")
        || lower.starts_with("<html")
        || (lower.contains("<body") && lower.contains("</"))
}

fn looks_like_markdown(text: &str) -> bool {
    text.lines().take(12).any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("# ")
            || trimmed.starts_with("## ")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("```")
            || (trimmed.starts_with('[') && trimmed.contains("]("))
    })
}

fn looks_like_csv(text: &str) -> bool {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty()).take(4);
    let Some(first) = lines.next() else {
        return false;
    };
    [',', '\t', ';'].into_iter().any(|delimiter| {
        let columns = first.split(delimiter).count();
        columns > 1
            && lines
                .clone()
                .take(2)
                .all(|line| line.split(delimiter).count() == columns)
    })
}

fn looks_like_srt(text: &str) -> bool {
    text.lines()
        .take(12)
        .any(|line| line.contains(" --> ") && line.contains(','))
}

fn looks_like_lrc(text: &str) -> bool {
    text.lines().take(12).any(|line| {
        let bytes = line.as_bytes();
        bytes.first() == Some(&b'[')
            && bytes.get(3) == Some(&b':')
            && line.find(']').is_some_and(|end| end >= 6)
    })
}

fn looks_like_mp3_frame(prefix: &[u8]) -> bool {
    prefix.len() >= 2 && prefix[0] == 0xff && prefix[1] & 0xe0 == 0xe0
}

fn looks_like_adts(prefix: &[u8]) -> bool {
    prefix.len() >= 2 && prefix[0] == 0xff && prefix[1] & 0xf6 == 0xf0
}

fn is_animated_gif(prefix: &[u8]) -> bool {
    prefix.iter().filter(|byte| **byte == 0x2c).take(2).count() > 1
        || contains_ascii(prefix, b"NETSCAPE2.0")
}

fn contains_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn contains_utf16le_ascii(haystack: &[u8], needle: &str) -> bool {
    let bytes = needle
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    contains_ascii(haystack, &bytes)
}

fn estimate_large_data(
    path: &Path,
    format: FileFormat,
    size_bytes: u64,
) -> Result<Option<LargeDataEstimate>, BackendError> {
    if format != FileFormat::Csv {
        return Ok(None);
    }
    let row_count = fs::read(path)
        .map_err(io_error)?
        .iter()
        .filter(|byte| **byte == b'\n')
        .count() as u64
        + 1;
    let requires_confirmation = size_bytes >= LARGE_DATA_BYTES || row_count >= LARGE_DATA_ROWS;
    Ok(Some(LargeDataEstimate {
        row_count,
        estimated_output_files: if requires_confirmation {
            ((row_count + LARGE_DATA_ROWS_PER_FILE - 1) / LARGE_DATA_ROWS_PER_FILE)
                .saturating_add(1)
                .min(u32::MAX as u64) as u32
        } else {
            1
        },
        total_bytes: size_bytes,
        requires_confirmation,
    }))
}
fn read_prefix(path: &Path) -> Result<Vec<u8>, BackendError> {
    let file = File::open(path).map_err(io_error)?;
    let mut bytes = Vec::new();
    file.take(PREFIX_BYTES)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    Ok(bytes)
}
fn relative_string(path: &Path, root: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .filter(|p| !p.as_os_str().is_empty())
        .and_then(|p| p.to_str())
        .map(|v| v.replace('\\', "/"))
}
fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
fn skip(
    result: &mut FileScanResult,
    path: &Path,
    relative_path: Option<String>,
    reason: FileSkipReason,
    detail: String,
) {
    result.skipped.push(SkippedFile {
        source_path: path.to_string_lossy().into_owned(),
        relative_path,
        reason,
        detail: Some(detail),
    });
}
fn error(code: &str, message: &str) -> BackendError {
    BackendError::new(code, message, true, true)
}
fn io_error(err: std::io::Error) -> BackendError {
    error("IMPORT_FILE_IO", &err.to_string())
}

#[cfg(windows)]
fn windows_attrs(metadata: &fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes()
}
#[cfg(not(windows))]
fn windows_attrs(_: &fs::Metadata) -> u32 {
    0
}
fn is_windows_reparse(metadata: &fs::Metadata) -> bool {
    windows_attrs(metadata) & 0x400 != 0
}
fn is_hidden_or_system(path: &Path, metadata: &fs::Metadata) -> bool {
    let attrs = windows_attrs(metadata);
    attrs & (0x2 | 0x4) != 0
        || path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'))
}

fn is_ignored_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name.to_ascii_lowercase().as_str(),
                "node_modules"
                    | "target"
                    | "dist"
                    | "build"
                    | "__pycache__"
                    | ".cache"
                    | ".tmp"
                    | "tmp"
                    | "temp"
            )
        })
}

#[cfg(test)]
mod batch9_portable_path_tests {
    use super::{normalize_locator, path_key, relative_string};
    use std::path::Path;

    #[test]
    fn portable_path_identity_normalizes_separators_without_collapsing_real_names() {
        assert_ne!(
            path_key(Path::new("Café/研究笔记.MD")),
            path_key(Path::new("Cafe\u{301}/研究笔记.md")),
            "case-sensitive and decomposition-sensitive files must not collide"
        );
        assert_eq!(
            normalize_locator(Path::new(r"C:\资料\研究笔记.MD")),
            "C:/资料/研究笔记.MD"
        );
        assert_ne!(
            normalize_locator(Path::new("/workspace/资料/A.md")),
            normalize_locator(Path::new("/workspace/资料/a.md"))
        );
        assert_eq!(
            relative_string(
                Path::new("/workspace/资料/子目录/研究笔记.md"),
                Path::new("/workspace/资料")
            )
            .as_deref(),
            Some("子目录/研究笔记.md")
        );
    }
}
