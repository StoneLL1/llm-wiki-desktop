use std::{
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    errors::BackendError,
    models::{
        import_v2::ImportItem, import_v2_web::RemoteMediaRetentionPlan, paths::ProjectContext,
    },
    utils::safe_project_dir::BoundProjectMutationRoot,
};

const WORKING_SPACE_RESERVE_BYTES: u64 = 256 * 1024 * 1024;
const PARTIAL_SCHEMA_VERSION: u32 = 2;
const PARTIAL_MANIFEST_MAX_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMediaPartialJournal {
    pub schema_version: u32,
    pub canonical_url_sha256: String,
    pub locator_identity_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub completed_ranges: Vec<RemoteMediaByteRange>,
    pub range_supported: bool,
    pub partial_sha256: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMediaByteRange {
    pub start: u64,
    pub end_exclusive: u64,
}

#[derive(Debug, Clone)]
pub struct RemoteMediaPartialBinding {
    pub payload_path: PathBuf,
    pub journal_path: PathBuf,
    pub journal: RemoteMediaPartialJournal,
}

pub fn remote_media_identity_sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub fn load_remote_media_partial(
    project_root: &Path,
    staging: &Path,
    canonical_url: &str,
    locator_identity: &str,
) -> Result<Option<RemoteMediaPartialBinding>, BackendError> {
    let root = staging.join("media-download");
    let payload_path = root.join("partial.bin");
    let journal_path = root.join("partial-v2.json");
    if !payload_path.exists() && !journal_path.exists() {
        return Ok(None);
    }
    let invalid = || {
        BackendError::new(
            "IMPORT_WEB_PARTIAL_INVALID",
            "The saved remote-media partial could not be verified and will be downloaded again.",
            true,
            true,
        )
    };
    let binding =
        BoundProjectMutationRoot::bind(project_root, &journal_path).map_err(|_| invalid())?;
    let manifest = match binding.open_regular(&journal_path) {
        Ok(manifest) => manifest,
        Err(_) => {
            clear_remote_media_partial(project_root, staging)?;
            return Ok(None);
        }
    };
    if manifest.metadata().map_err(|_| invalid())?.len() > PARTIAL_MANIFEST_MAX_BYTES {
        clear_remote_media_partial(project_root, staging)?;
        return Ok(None);
    }
    let journal: RemoteMediaPartialJournal = match serde_json::from_reader(manifest) {
        Ok(journal) => journal,
        Err(_) => {
            clear_remote_media_partial(project_root, staging)?;
            return Ok(None);
        }
    };
    let payload = match binding.open_regular(&payload_path) {
        Ok(payload) => payload,
        Err(_) => {
            clear_remote_media_partial(project_root, staging)?;
            return Ok(None);
        }
    };
    let payload_len = payload.metadata().map_err(|_| invalid())?.len();
    let expected_url = remote_media_identity_sha256(canonical_url);
    let expected_locator = remote_media_identity_sha256(locator_identity);
    if journal.schema_version != PARTIAL_SCHEMA_VERSION
        || journal.canonical_url_sha256 != expected_url
        || journal.locator_identity_sha256 != expected_locator
        || journal.downloaded_bytes == 0
        || journal
            .total_bytes
            .is_some_and(|total| total < journal.downloaded_bytes)
        || payload_len != journal.downloaded_bytes
        || journal.completed_ranges
            != vec![RemoteMediaByteRange {
                start: 0,
                end_exclusive: journal.downloaded_bytes,
            }]
        || hash_reader(payload)? != journal.partial_sha256
    {
        clear_remote_media_partial(project_root, staging)?;
        return Ok(None);
    }
    Ok(Some(RemoteMediaPartialBinding {
        payload_path,
        journal_path,
        journal,
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn checkpoint_remote_media_partial(
    project_root: &Path,
    staging: &Path,
    canonical_url: &str,
    locator_identity: &str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    etag: Option<String>,
    last_modified: Option<String>,
    range_supported: bool,
    partial_sha256: String,
) -> Result<RemoteMediaPartialJournal, BackendError> {
    let root = staging.join("media-download");
    let journal_path = root.join("partial-v2.json");
    let (binding, _) = BoundProjectMutationRoot::ensure_and_bind(project_root, &journal_path)
        .map_err(|_| partial_io_error())?;
    let journal = RemoteMediaPartialJournal {
        schema_version: PARTIAL_SCHEMA_VERSION,
        canonical_url_sha256: remote_media_identity_sha256(canonical_url),
        locator_identity_sha256: remote_media_identity_sha256(locator_identity),
        etag,
        last_modified,
        total_bytes,
        downloaded_bytes,
        completed_ranges: (downloaded_bytes > 0)
            .then_some(RemoteMediaByteRange {
                start: 0,
                end_exclusive: downloaded_bytes,
            })
            .into_iter()
            .collect(),
        range_supported,
        partial_sha256,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    let bytes = serde_json::to_vec_pretty(&journal).map_err(|_| partial_io_error())?;
    let pending = binding
        .write_synced_temp(&journal_path, &bytes)
        .map_err(|_| partial_io_error())?;
    binding
        .install_prepared(&pending, &journal_path)
        .map_err(|_| partial_io_error())?;
    Ok(journal)
}

pub fn clear_remote_media_partial(project_root: &Path, staging: &Path) -> Result<(), BackendError> {
    let root = staging.join("media-download");
    let binding = match BoundProjectMutationRoot::bind(project_root, &root.join("partial.bin")) {
        Ok(binding) => binding,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(partial_io_error()),
    };
    for path in [root.join("partial.bin"), root.join("partial-v2.json")] {
        match binding.remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(partial_io_error()),
        }
    }
    Ok(())
}

fn hash_reader(mut reader: std::fs::File) -> Result<String, BackendError> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(|_| partial_io_error())?;
        if read == 0 {
            return Ok(format!("{:x}", hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}

fn partial_io_error() -> BackendError {
    BackendError::new(
        "IMPORT_WEB_PARTIAL_IO_FAILED",
        "The remote-media resume journal could not be updated safely.",
        true,
        false,
    )
}

pub fn build_remote_media_retention_plan(
    context: &ProjectContext,
    session_id: &str,
    item: &ImportItem,
) -> Result<RemoteMediaRetentionPlan, BackendError> {
    let estimated_bytes = measured_remote_media_bytes(context, session_id, &item.item_id)
        .ok_or_else(|| {
            BackendError::new(
                "IMPORT_WEB_MEDIA_SIZE_UNAVAILABLE",
                "The remote provider did not expose a verified media size. Refresh the preview before retaining the original media.",
                true,
                true,
            )
        })?;
    let available_disk_bytes = available_disk_bytes(&context.root);
    let enough_disk = enough_disk_for_remote_media(available_disk_bytes, estimated_bytes);
    Ok(RemoteMediaRetentionPlan {
        item_id: item.item_id.clone(),
        estimated_bytes,
        available_disk_bytes,
        enough_disk,
        quality: "best_available".into(),
    })
}

fn enough_disk_for_remote_media(available: Option<u64>, estimated_bytes: u64) -> Option<bool> {
    available
        .map(|available| available >= estimated_bytes.saturating_add(WORKING_SPACE_RESERVE_BYTES))
}

fn measured_remote_media_bytes(
    context: &ProjectContext,
    session_id: &str,
    item_id: &str,
) -> Option<u64> {
    let staging = context
        .layout
        .import_paths()
        .and_then(|paths| paths.item_staging(session_id, item_id))
        .and_then(|path| context.resolve_project_path(&path))
        .ok()?;
    for relative in ["media-download/manifest.json", "metadata.json"] {
        let Ok(bytes) = std::fs::read(staging.join(relative)) else {
            continue;
        };
        if bytes.len() > 1024 * 1024 {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let complete = value
            .get("complete")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        let measured = value
            .get("byteLen")
            .or_else(|| value.get("mediaSizeBytes"))
            .and_then(serde_json::Value::as_u64)
            .filter(|size| complete && *size > 0);
        if measured.is_some() {
            return measured;
        }
    }
    None
}

#[cfg(windows)]
pub(crate) fn available_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut available = 0_u64;
    (unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } != 0)
        .then_some(available)
}

#[cfg(unix)]
pub(crate) fn available_disk_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::zeroed;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { zeroed() };
    (unsafe { libc::statvfs(path.as_ptr(), &mut stats) } == 0)
        .then(|| (stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

#[cfg(not(any(windows, unix)))]
pub(crate) fn available_disk_bytes(_: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use crate::models::import_v2::{ImportInput, ImportInputKind, ImportItem};

    use super::*;

    #[test]
    fn plan_uses_measured_bytes_and_reserves_working_space() {
        let root = tempfile::tempdir().unwrap();
        let context = ProjectContext::new("project-a", root.path().to_path_buf());
        let item = ImportItem::queued(
            "item-a",
            ImportInput {
                kind: ImportInputKind::Url,
                display_name: "Measured video".into(),
                locator: "import-web-target:opaque".into(),
                normalized_locator: Some("https://www.bilibili.com/video/BV1measured".into()),
                source_identity: None,
                media_save_mode: Default::default(),
            },
        );
        let staging = root
            .path()
            .join(".app/import-sessions/session-a/items/item-a/staging");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("metadata.json"),
            r#"{"mediaPresent":true,"mediaSizeBytes":73400320}"#,
        )
        .unwrap();

        let plan = build_remote_media_retention_plan(&context, "session-a", &item).unwrap();
        assert_eq!(plan.estimated_bytes, 73_400_320);
        assert_eq!(
            enough_disk_for_remote_media(Some(73_400_320), 73_400_320),
            Some(false)
        );
        assert_eq!(
            enough_disk_for_remote_media(
                Some(73_400_320 + WORKING_SPACE_RESERVE_BYTES),
                73_400_320,
            ),
            Some(true)
        );
    }

    #[test]
    fn partial_journal_resumes_only_for_matching_url_locator_and_hash() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        let media_root = staging.join("media-download");
        std::fs::create_dir_all(&media_root).unwrap();
        let bytes = b"verified partial bytes";
        std::fs::write(media_root.join("partial.bin"), bytes).unwrap();
        checkpoint_remote_media_partial(
            root.path(),
            &staging,
            "https://media.example/video.mp4",
            "import-web-target:opaque-a",
            bytes.len() as u64,
            Some(100),
            Some("\"etag-a\"".into()),
            None,
            true,
            format!("{:x}", Sha256::digest(bytes)),
        )
        .unwrap();

        let loaded = load_remote_media_partial(
            root.path(),
            &staging,
            "https://media.example/video.mp4",
            "import-web-target:opaque-a",
        )
        .unwrap()
        .unwrap();
        assert_eq!(loaded.journal.downloaded_bytes, bytes.len() as u64);

        std::fs::write(media_root.join("partial.bin"), b"corrupted partial data").unwrap();
        let corrupted = load_remote_media_partial(
            root.path(),
            &staging,
            "https://media.example/video.mp4",
            "import-web-target:opaque-a",
        )
        .unwrap();
        assert!(corrupted.is_none());
        std::fs::write(media_root.join("partial.bin"), bytes).unwrap();
        checkpoint_remote_media_partial(
            root.path(),
            &staging,
            "https://media.example/video.mp4",
            "import-web-target:opaque-a",
            bytes.len() as u64,
            Some(100),
            Some("\"etag-a\"".into()),
            None,
            true,
            format!("{:x}", Sha256::digest(bytes)),
        )
        .unwrap();

        let rejected = load_remote_media_partial(
            root.path(),
            &staging,
            "https://media.example/video.mp4",
            "import-web-target:opaque-b",
        )
        .unwrap();
        assert!(rejected.is_none());
        assert!(!media_root.join("partial.bin").exists());
        assert!(!media_root.join("partial-v2.json").exists());
    }
}
