use std::path::Path;

use crate::{
    errors::BackendError,
    models::{
        import_v2::ImportItem, import_v2_web::RemoteMediaRetentionPlan, paths::ProjectContext,
    },
};

const WORKING_SPACE_RESERVE_BYTES: u64 = 256 * 1024 * 1024;

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
        .resolve_project_path(&format!(
            ".app/import-sessions/{session_id}/items/{item_id}/staging"
        ))
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
fn available_disk_bytes(path: &Path) -> Option<u64> {
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
fn available_disk_bytes(path: &Path) -> Option<u64> {
    use std::ffi::CString;
    use std::mem::zeroed;
    use std::os::unix::ffi::OsStrExt;

    let path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut stats: libc::statvfs = unsafe { zeroed() };
    (unsafe { libc::statvfs(path.as_ptr(), &mut stats) } == 0)
        .then(|| (stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

#[cfg(not(any(windows, unix)))]
fn available_disk_bytes(_: &Path) -> Option<u64> {
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
}
