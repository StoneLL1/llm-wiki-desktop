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
const LARGE_DATA_OUTPUT_FILES: u32 = 2_000;
const IMAGE_MAX_BYTES: u64 = 512 * 1024 * 1024;
const AUDIO_MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const VIDEO_MAX_BYTES: u64 = 32 * 1024 * 1024 * 1024;
const MEDIA_CONFIRMATION_BYTES: u64 = 64 * 1024 * 1024;
const MEDIA_WORKING_SPACE_RESERVE_BYTES: u64 = 256 * 1024 * 1024;
pub const DISCOVERY_BATCH_SIZE: usize = 128;

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
            scan_identity: None,
            totals: Default::default(),
            confirmation_token: None,
            accepted_at: None,
            aggregate_confirmed_at: None,
            discarded_at: None,
        };
        let mut visited_dirs = HashSet::new();
        let mut visited_files = HashSet::new();
        let mut pending = Vec::with_capacity(DISCOVERY_BATCH_SIZE);

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
            let format_limit = format_size_limit(format, policy.max_file_bytes);
            if metadata.len() > format_limit {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::FileTooLarge,
                    format!(
                        "{} input requires {} bytes but this format is limited to {} bytes.",
                        content_kind_label(format.content_kind()),
                        metadata.len(),
                        format_limit
                    ),
                );
                continue;
            }
            if let Some((required, available)) =
                media_disk_requirement(format.content_kind(), metadata.len(), &context.root)
            {
                if available < required {
                    skip(
                        &mut result,
                        &path,
                        relative,
                        FileSkipReason::InsufficientDisk,
                        format!(
                            "This {} requires about {} bytes of working space; {} bytes are available.",
                            content_kind_label(format.content_kind()),
                            required,
                            available
                        ),
                    );
                    continue;
                }
            }
            let Some(source_path) = path.to_str().map(str::to_owned) else {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::NonUtf8Path,
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
                    FileSkipReason::NonUtf8Path,
                    "File name is not valid UTF-8.".into(),
                );
                continue;
            };
            enforce_file_count_limit(result.files.len(), policy.max_files)?;
            let file = DiscoveredFile {
                source_path,
                relative_path: relative.unwrap_or_else(|| display_name.clone()),
                display_name,
                format,
                content_kind: format.content_kind(),
                size_bytes: metadata.len(),
                identity,
                source_identity: source_identity(&canonical, &metadata, &is_cancelled)?,
                large_data: estimate_large_data(&path, format, metadata.len())?,
            };
            result.files.push(file.clone());
            pending.push(file);
            if pending.len() == DISCOVERY_BATCH_SIZE {
                on_batch(&pending);
                pending.clear();
            }
        }
        if !pending.is_empty() {
            on_batch(&pending);
        }
        Ok(result)
    }

    pub fn revalidate_discovered_file(&self, file: &DiscoveredFile) -> Result<(), BackendError> {
        let path = PathBuf::from(&file.source_path);
        let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
        if metadata.file_type().is_symlink() || is_windows_reparse(&metadata) || !metadata.is_file()
        {
            return Err(error(
                "IMPORT_SCAN_SOURCE_CHANGED",
                "A discovered source is no longer the same regular file.",
            ));
        }
        let canonical = fs::canonicalize(&path).map_err(io_error)?;
        let current = source_identity(&canonical, &metadata, &|| false)?;
        if current != file.source_identity {
            return Err(error(
                "IMPORT_SCAN_SOURCE_CHANGED",
                "A discovered source changed after confirmation was requested. Scan it again.",
            ));
        }
        Ok(())
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

fn source_identity<C>(
    path: &Path,
    metadata: &fs::Metadata,
    is_cancelled: &C,
) -> Result<SourceIdentity, BackendError>
where
    C: Fn() -> bool,
{
    let (sha256, magic) = stream_file_hashes(path, metadata.len(), is_cancelled)?;
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
        sha256,
        magic,
    })
}

fn magic_fingerprint(bytes: &[u8]) -> String {
    format!(
        "{:x}",
        Sha256::digest(&bytes[..bytes.len().min(PREFIX_BYTES as usize)])
    )
}

fn stream_file_hashes<C>(
    path: &Path,
    expected_len: u64,
    is_cancelled: &C,
) -> Result<(String, String), BackendError>
where
    C: Fn() -> bool,
{
    let mut file = File::open(path).map_err(io_error)?;
    let mut full = Sha256::new();
    let mut prefix = Vec::with_capacity(PREFIX_BYTES as usize);
    let mut buffer = [0_u8; 1024 * 1024];
    let mut read_total = 0_u64;
    loop {
        if is_cancelled() {
            return Err(error(
                "IMPORT_FILE_SCAN_CANCELLED",
                "File discovery was cancelled.",
            ));
        }
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        read_total = read_total.saturating_add(read as u64);
        full.update(&buffer[..read]);
        if prefix.len() < PREFIX_BYTES as usize {
            let remaining = PREFIX_BYTES as usize - prefix.len();
            prefix.extend_from_slice(&buffer[..read.min(remaining)]);
        }
    }
    if read_total != expected_len {
        return Err(error(
            "IMPORT_SCAN_SOURCE_CHANGED",
            "A source changed while its identity was being recorded. Scan it again.",
        ));
    }
    Ok((format!("{:x}", full.finalize()), magic_fingerprint(&prefix)))
}

fn format_size_limit(format: FileFormat, document_limit: u64) -> u64 {
    match format.content_kind() {
        crate::models::import_v2_file::FileContentKind::Document
        | crate::models::import_v2_file::FileContentKind::Subtitle => document_limit,
        crate::models::import_v2_file::FileContentKind::Image => IMAGE_MAX_BYTES,
        crate::models::import_v2_file::FileContentKind::Audio => AUDIO_MAX_BYTES,
        crate::models::import_v2_file::FileContentKind::Video => VIDEO_MAX_BYTES,
    }
}

fn content_kind_label(kind: crate::models::import_v2_file::FileContentKind) -> &'static str {
    match kind {
        crate::models::import_v2_file::FileContentKind::Document => "Document",
        crate::models::import_v2_file::FileContentKind::Image => "Image",
        crate::models::import_v2_file::FileContentKind::Audio => "Audio",
        crate::models::import_v2_file::FileContentKind::Video => "Video",
        crate::models::import_v2_file::FileContentKind::Subtitle => "Subtitle",
    }
}

fn media_disk_requirement(
    kind: crate::models::import_v2_file::FileContentKind,
    source_bytes: u64,
    project_root: &Path,
) -> Option<(u64, u64)> {
    let multiplier = match kind {
        crate::models::import_v2_file::FileContentKind::Audio => 2,
        crate::models::import_v2_file::FileContentKind::Video => 3,
        _ => return None,
    };
    let required = source_bytes
        .saturating_mul(multiplier)
        .saturating_add(MEDIA_WORKING_SPACE_RESERVE_BYTES);
    crate::services::import_v2::remote_media_retention::available_disk_bytes(project_root)
        .map(|available| (required, available))
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
        if is_animated_gif_file(path)? {
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
    let format = if file_contains_any_case_insensitive(
        path,
        &[
            b"workbook",
            &"Workbook"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        ],
    )? {
        Some(FileFormat::Xls)
    } else if file_contains_any_case_insensitive(
        path,
        &[
            b"powerpoint document",
            &"PowerPoint Document"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        ],
    )? {
        Some(FileFormat::Ppt)
    } else if file_contains_any_case_insensitive(
        path,
        &[
            b"worddocument",
            &"WordDocument"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        ],
    )? {
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

fn file_contains_any_case_insensitive(
    path: &Path,
    needles: &[&[u8]],
) -> Result<bool, BackendError> {
    let mut file = File::open(path).map_err(io_error)?;
    let overlap = needles.iter().map(|needle| needle.len()).max().unwrap_or(1) - 1;
    let mut carry = Vec::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            return Ok(false);
        }
        carry.extend_from_slice(&buffer[..read]);
        if needles
            .iter()
            .any(|needle| contains_ascii_case_insensitive(&carry, needle))
        {
            return Ok(true);
        }
        if carry.len() > overlap {
            carry.drain(..carry.len() - overlap);
        }
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

fn estimate_large_data(
    path: &Path,
    format: FileFormat,
    size_bytes: u64,
) -> Result<Option<LargeDataEstimate>, BackendError> {
    match format {
        format
            if matches!(
                format.content_kind(),
                crate::models::import_v2_file::FileContentKind::Audio
                    | crate::models::import_v2_file::FileContentKind::Video
            ) && size_bytes > MEDIA_CONFIRMATION_BYTES =>
        {
            Ok(Some(LargeDataEstimate {
                row_count: 0,
                sheet_count: None,
                estimated_output_files: 1,
                total_bytes: size_bytes,
                requires_confirmation: true,
                estimate_complete: false,
            }))
        }
        FileFormat::Csv => {
            let row_count = count_byte_streaming(path, b'\n')?.saturating_add(1);
            let requires_confirmation =
                size_bytes >= LARGE_DATA_BYTES || row_count >= LARGE_DATA_ROWS;
            Ok(Some(LargeDataEstimate {
                row_count,
                sheet_count: None,
                estimated_output_files: if requires_confirmation {
                    ((row_count + LARGE_DATA_ROWS_PER_FILE - 1) / LARGE_DATA_ROWS_PER_FILE)
                        .saturating_add(1)
                        .min(u32::MAX as u64) as u32
                } else {
                    1
                },
                total_bytes: size_bytes,
                requires_confirmation,
                estimate_complete: true,
            }))
        }
        FileFormat::Xlsx => {
            let sheet_count = estimate_xlsx_sheet_count(path)?;
            let estimated_output_files = sheet_count.saturating_add(1).max(1);
            Ok(Some(LargeDataEstimate {
                row_count: 0,
                sheet_count: Some(sheet_count),
                estimated_output_files,
                total_bytes: size_bytes,
                requires_confirmation: size_bytes >= LARGE_DATA_BYTES
                    || estimated_output_files > LARGE_DATA_OUTPUT_FILES,
                estimate_complete: true,
            }))
        }
        FileFormat::Xls => Ok(Some(LargeDataEstimate {
            row_count: 0,
            sheet_count: None,
            estimated_output_files: 1,
            total_bytes: size_bytes,
            // The built-in discovery layer cannot safely enumerate BIFF
            // worksheets without parsing the OLE stream. Keep the estimate
            // explicitly incomplete and require acknowledgement instead of
            // presenting a false one-output guarantee.
            requires_confirmation: true,
            estimate_complete: false,
        })),
        _ => Ok(None),
    }
}

fn estimate_xlsx_sheet_count(path: &Path) -> Result<u32, BackendError> {
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
    let mut sheets = 0u32;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|_| {
            error(
                "IMPORT_FILE_INVALID_OOXML",
                "OOXML central directory is invalid.",
            )
        })?;
        let name = entry.name().replace('\\', "/");
        if name.starts_with("xl/worksheets/") && name.ends_with(".xml") {
            sheets = sheets.saturating_add(1);
        }
    }
    Ok(sheets)
}

fn enforce_file_count_limit(discovered: usize, max_files: u32) -> Result<(), BackendError> {
    if discovered < max_files as usize {
        return Ok(());
    }
    Err(BackendError::new(
        "IMPORT_FILE_HARD_LIMIT_EXCEEDED",
        format!(
            "The selection contains more than {max_files} supported files. Select fewer files or a smaller folder and try again."
        ),
        true,
        true,
    )
    .with_details(serde_json::json!({
        "limit": max_files,
        "discovered": discovered.saturating_add(1),
    })))
}
fn read_prefix(path: &Path) -> Result<Vec<u8>, BackendError> {
    let file = File::open(path).map_err(io_error)?;
    let mut bytes = Vec::new();
    file.take(PREFIX_BYTES)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    Ok(bytes)
}

pub(crate) fn read_file_prefix(path: &Path) -> Result<Vec<u8>, BackendError> {
    read_prefix(path)
}

fn count_byte_streaming(path: &Path, needle: u8) -> Result<u64, BackendError> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut count = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            return Ok(count);
        }
        count = count.saturating_add(
            buffer[..read]
                .iter()
                .filter(|byte| **byte == needle)
                .count() as u64,
        );
    }
}

fn is_animated_gif_file(path: &Path) -> Result<bool, BackendError> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut frames = 0_u8;
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            return Ok(false);
        }
        for byte in &buffer[..read] {
            if *byte == 0x2c {
                frames = frames.saturating_add(1);
                if frames >= 2 {
                    return Ok(true);
                }
            }
        }
    }
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
    use super::{
        format_size_limit, normalize_locator, path_key, relative_string, AUDIO_MAX_BYTES,
        IMAGE_MAX_BYTES, VIDEO_MAX_BYTES,
    };
    use crate::models::import_v2_file::FileFormat;
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

    #[test]
    fn media_hard_limits_cannot_be_raised_by_the_document_policy() {
        let untrusted_policy_limit = 100 * 1024 * 1024 * 1024;
        assert_eq!(
            format_size_limit(FileFormat::Png, untrusted_policy_limit),
            IMAGE_MAX_BYTES
        );
        assert_eq!(
            format_size_limit(FileFormat::Mp3, untrusted_policy_limit),
            AUDIO_MAX_BYTES
        );
        assert_eq!(
            format_size_limit(FileFormat::Mp4, untrusted_policy_limit),
            VIDEO_MAX_BYTES
        );
    }
}
