use crate::errors::BackendError;
use crate::models::import_v2::{ImportInput, ImportInputKind, ImportSession, SourceIdentity};
use crate::models::import_v2_file::{
    DiscoveredFile, FileFormat, FileIdentity, FileScanPolicy, FileScanResult, FileSkipReason,
    SkippedFile,
};
use crate::models::paths::ProjectContext;
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;
use zip::ZipArchive;

const PREFIX_BYTES: u64 = 8192;
const OOXML_MAX_ENTRIES: usize = 4096;
const OOXML_MAX_UNCOMPRESSED: u64 = 64 * 1024 * 1024;

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
        let mut unicode_names = HashSet::new();
        let mut portable_names = HashSet::new();

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
            let unicode = path.to_string_lossy().nfc().collect::<String>();
            if !unicode_names.insert(unicode.clone()) {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::UnicodeNormalizationCollision,
                    "Path collides after Unicode normalization.".into(),
                );
                continue;
            }
            let portable = unicode.to_lowercase();
            if !portable_names.insert(portable) {
                skip(
                    &mut result,
                    &path,
                    relative,
                    FileSkipReason::CaseCollision,
                    "Path collides after portable case folding.".into(),
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
            let identity = match identify_file(&path, &prefix) {
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
            let format = format_from_identity(&identity).expect("identified MIME has a format");
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
                size_bytes: metadata.len(),
                identity,
                source_identity: source_identity(&canonical, &metadata)?,
            };
            result.files.push(file.clone());
            on_batch(std::slice::from_ref(&file));
        }
        Ok(result)
    }
}

pub fn identify_file(path: &Path, prefix: &[u8]) -> Result<FileIdentity, BackendError> {
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (magic, mime) =
        match extension.as_str() {
            "md" | "markdown" | "mdx" | "mkd" | "mkdn" | "mdown" | "mdwn" | "rmd"
            | "txt" | "csv" | "html" | "htm" => {
                ("utf-8", text_mime(&extension))
            }
            "pdf" if prefix.starts_with(b"%PDF-") => ("pdf", "application/pdf"),
            "doc" | "xls" | "ppt"
                if prefix.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) =>
            {
                ("ole-cfb", legacy_mime(&extension))
            }
            "docx" | "xlsx" | "pptx" if prefix.starts_with(b"PK\x03\x04") => {
                validate_ooxml(path, &extension)?;
                ("ooxml-zip", ooxml_mime(&extension))
            }
            _ => return Err(error(
                "IMPORT_FILE_UNSUPPORTED",
                "Extension, magic bytes, and container structure do not identify a supported file.",
            )),
        };
    Ok(FileIdentity {
        extension,
        magic: magic.into(),
        mime: mime.into(),
    })
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
                display_name: file.display_name,
                locator: file.source_path,
                normalized_locator: Some(normalized),
                source_identity: Some(file.source_identity),
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
    canonical
        .to_string_lossy()
        .replace('\\', "/")
        .nfc()
        .collect::<String>()
        .to_lowercase()
}

fn validate_ooxml(path: &Path, extension: &str) -> Result<(), BackendError> {
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
    let required = match extension {
        "docx" => "word/document.xml",
        "xlsx" => "xl/workbook.xml",
        _ => "ppt/presentation.xml",
    };
    let mut total = 0u64;
    let mut content_types = false;
    let mut root = false;
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
        root |= name == required;
    }
    if !content_types || !root {
        return Err(error(
            "IMPORT_FILE_INVALID_OOXML",
            "OOXML archive is missing required package roots.",
        ));
    }
    Ok(())
}

fn format_from_identity(identity: &FileIdentity) -> Option<FileFormat> {
    Some(match identity.extension.as_str() {
        "md" | "markdown" | "mdx" | "mkd" | "mkdn" | "mdown" | "mdwn" | "rmd" | "txt"
        | "csv" | "html" | "htm" => FileFormat::Markdown,
        "pdf" => FileFormat::Pdf,
        "doc" => FileFormat::Doc,
        "docx" => FileFormat::Docx,
        "xls" => FileFormat::Xls,
        "xlsx" => FileFormat::Xlsx,
        "ppt" => FileFormat::Ppt,
        "pptx" => FileFormat::Pptx,
        _ => return None,
    })
}
fn text_mime(ext: &str) -> &'static str {
    match ext {
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "txt" => "text/plain",
        _ => "text/markdown",
    }
}
fn legacy_mime(ext: &str) -> &'static str {
    match ext {
        "doc" => "application/msword",
        "xls" => "application/vnd.ms-excel",
        _ => "application/vnd.ms-powerpoint",
    }
}
fn ooxml_mime(ext: &str) -> &'static str {
    match ext {
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    }
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
    path.to_string_lossy()
        .nfc()
        .collect::<String>()
        .to_lowercase()
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
