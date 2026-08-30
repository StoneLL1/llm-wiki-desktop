use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::errors::BackendError;
use crate::models::import_v2_file::CapabilityRequirement;
use crate::services::{BlockingWorkClass, BlockingWorkCoordinator};
use crate::tasks::task_model::CancellationToken;
use crate::utils::safe_project_dir::BoundProjectMutationRoot;

use super::capability_pack::CapabilityPackManager;

const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_INSTALLED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 20_000;
const MAX_PARTIAL_METADATA_BYTES: u64 = 64 * 1024;
const MAX_REAPER_PREFIX_VERIFY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE_DEPTH: usize = 24;
const IO_BUFFER_BYTES: usize = 64 * 1024;
const PARTIAL_SCHEMA_VERSION: u32 = 1;
const PARTIAL_CHECKPOINT_BYTES: u64 = 4 * 1024 * 1024;
const PARTIAL_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const DOWNLOADS_DIR: &str = ".downloads";
const PENDING_ACTIVATION_PREFIX: &str = ".pending-activation-";

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartialPaths {
    archive: PathBuf,
    metadata: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PartialDownloadMetadata {
    schema_version: u32,
    capability_id: String,
    version: String,
    target_triple: String,
    catalog_url: String,
    response_url: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
    expected_length: u64,
    expected_sha256: String,
    downloaded_bytes: u64,
    prefix_sha256: String,
    owner_task_id: String,
    updated_at_unix_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PendingActivation {
    schema_version: u32,
    identity: String,
    capability_id: String,
    version: String,
    owner_task_id: String,
    #[serde(default)]
    phase: PendingActivationPhase,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PendingActivationPhase {
    #[default]
    Prepared,
    Probed,
    Activated,
}

impl PartialDownloadMetadata {
    fn new(entry: &CapabilityCatalogEntry, owner_task_id: &str, downloaded_bytes: u64) -> Self {
        Self {
            schema_version: PARTIAL_SCHEMA_VERSION,
            capability_id: entry.capability_id.clone(),
            version: entry.version.clone(),
            target_triple: entry.target_triple.clone(),
            catalog_url: entry.url.clone(),
            response_url: None,
            etag: None,
            last_modified: None,
            expected_length: entry.compressed_bytes,
            expected_sha256: entry.archive_sha256.to_ascii_lowercase(),
            downloaded_bytes,
            prefix_sha256: format!("{:x}", Sha256::digest([])),
            owner_task_id: owner_task_id.into(),
            updated_at_unix_seconds: unix_seconds(),
        }
    }

    fn matches_entry(&self, entry: &CapabilityCatalogEntry) -> bool {
        self.schema_version == PARTIAL_SCHEMA_VERSION
            && self.capability_id == entry.capability_id
            && self.version == entry.version
            && self.target_triple == entry.target_triple
            && self.catalog_url == entry.url
            && self.expected_length == entry.compressed_bytes
            && self
                .expected_sha256
                .eq_ignore_ascii_case(&entry.archive_sha256)
            && self.downloaded_bytes <= self.expected_length
            && !self.owner_task_id.trim().is_empty()
            && self.prefix_sha256.len() == 64
            && self
                .prefix_sha256
                .chars()
                .all(|value| value.is_ascii_hexdigit())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityInstallRecovery {
    pub preserved_partials: usize,
    pub removed_orphans: usize,
    pub removed_staging: usize,
    pub rolled_back_pending: usize,
    pub rolled_back_prepared: usize,
    pub rolled_back_probed: usize,
    pub rolled_back_activated: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallCatalog {
    schema_version: u32,
    entries: Vec<CapabilityCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalogEntry {
    pub capability_id: String,
    pub version: String,
    pub target_triple: String,
    pub url: String,
    pub archive_sha256: String,
    pub manifest_sha256: String,
    pub compressed_bytes: u64,
    pub installed_bytes: u64,
    #[serde(default)]
    pub model_bytes: Option<u64>,
    pub license: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityCatalogAvailability {
    Available,
    CatalogUnavailable,
}

pub fn catalog_availability() -> CapabilityCatalogAvailability {
    let catalog = serde_json::from_str::<InstallCatalog>(include_str!(concat!(
        env!("OUT_DIR"),
        "/capabilities/install-catalog.json"
    )));
    match catalog {
        Ok(catalog) if catalog.schema_version == 1 && !catalog.entries.is_empty() => {
            CapabilityCatalogAvailability::Available
        }
        _ => CapabilityCatalogAvailability::CatalogUnavailable,
    }
}

pub fn catalog_entry(capability_id: &str, target_triple: &str) -> Option<CapabilityCatalogEntry> {
    let catalog: InstallCatalog = serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/capabilities/install-catalog.json"
    )))
    .ok()?;
    if catalog.schema_version != 1 {
        return None;
    }
    select_catalog_entry(catalog.entries, capability_id, target_triple)
}

fn select_catalog_entry(
    entries: Vec<CapabilityCatalogEntry>,
    capability_id: &str,
    target_triple: &str,
) -> Option<CapabilityCatalogEntry> {
    entries
        .into_iter()
        .filter(|entry| {
            entry.capability_id == capability_id
                && entry.target_triple == target_triple
                && valid_catalog_entry(entry)
        })
        .max_by_key(|entry| semver::Version::parse(&entry.version).ok())
}

pub async fn install_catalog_entry(
    blocking_work: &BlockingWorkCoordinator,
    install_root: &Path,
    entry: &CapabilityCatalogEntry,
    owner_task_id: &str,
    token: &CancellationToken,
    mut progress: impl FnMut(CapabilityInstallPhase, u64, u64),
) -> Result<CapabilityInstallOutcome, BackendError> {
    let preflight_root = install_root.to_path_buf();
    let preflight_entry = entry.clone();
    let preflight_owner = owner_task_id.to_owned();
    let (install_root, paths, install_identity, staging_root, release_lock) = blocking_work
        .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
            if !valid_catalog_entry(&preflight_entry) {
                return Err(install_error("Capability catalog entry is invalid."));
            }
            std::fs::create_dir_all(&preflight_root)
                .map_err(|_| install_error("Capability install directory is unavailable."))?;
            let install_root = preflight_root
                .canonicalize()
                .map_err(|_| install_error("Capability install directory cannot be resolved."))?;
            let paths = partial_paths(&install_root, &preflight_entry)?;
            let install_identity = release_identity(&preflight_entry);
            let staging_root = install_root.join(format!(".installing-{install_identity}"));
            let release_lock =
                acquire_release_lock(&install_root, &install_identity, &preflight_owner)?;
            recover_pending_activation_for_release(
                &install_root,
                &install_identity,
                &preflight_entry,
            )?;
            Ok((
                install_root,
                paths,
                install_identity,
                staging_root,
                release_lock,
            ))
        })
        .await?;
    download_archive(
        blocking_work,
        entry,
        &paths,
        owner_task_id,
        token,
        &mut progress,
    )
    .await?;
    if token.is_cancelled() {
        if !token.is_pause_requested() {
            let paths_for_cleanup = paths.clone();
            blocking_work
                .run(BlockingWorkClass::HeavyIo, move || {
                    remove_partial(&paths_for_cleanup);
                    Ok(())
                })
                .await?;
        }
        return Err(stopped(token));
    }
    progress(
        CapabilityInstallPhase::Verifying,
        entry.compressed_bytes,
        entry.compressed_bytes,
    );
    progress(
        CapabilityInstallPhase::Installing,
        entry.compressed_bytes,
        entry.compressed_bytes,
    );
    let install_entry = entry.clone();
    let install_owner = owner_task_id.to_owned();
    let install_token = token.clone();
    blocking_work
        .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
            let cleanup = InstallCleanup {
                install_root: install_root.clone(),
                staging_root: staging_root.clone(),
            };
            extract_and_verify(
                &paths.archive,
                &staging_root,
                &install_entry,
                &install_token,
            )?;
            if install_token.is_cancelled() {
                if !install_token.is_pause_requested() {
                    remove_partial(&paths);
                }
                return Err(stopped(&install_token));
            }
            let staged_version = staging_root
                .join(&install_entry.capability_id)
                .join(&install_entry.version);
            let final_parent = install_root.join(&install_entry.capability_id);
            let final_path = final_parent.join(&install_entry.version);
            ensure_destination_parent(&install_root, &final_parent)?;
            let mut pending_activation = None;
            let created_by_this_call = match std::fs::symlink_metadata(&final_path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink()
                        || is_reparse(&metadata)
                        || !metadata.is_dir()
                    {
                        return Err(install_error("Capability destination is unsafe."));
                    }
                    let pack =
                        verify_installed_root(&install_root, &install_entry).map_err(|_| {
                            install_error(
                                "Capability version collides with a different signed release.",
                            )
                        })?;
                    restore_executable_permissions(&pack)?;
                    false
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let pending_path = pending_activation_path(&install_root, &install_identity);
                    write_pending_activation(
                        &install_root,
                        &pending_path,
                        &PendingActivation {
                            schema_version: 1,
                            identity: install_identity.clone(),
                            capability_id: install_entry.capability_id.clone(),
                            version: install_entry.version.clone(),
                            owner_task_id: install_owner.clone(),
                            phase: PendingActivationPhase::Prepared,
                        },
                    )?;
                    pending_activation = Some(pending_path.clone());
                    let source = BoundProjectMutationRoot::bind(&install_root, &staged_version)
                        .map_err(|_| install_error("Capability staging directory is unsafe."))?;
                    let destination = BoundProjectMutationRoot::bind(&install_root, &final_path)
                        .map_err(|_| install_error("Capability destination is unsafe."))?;
                    if source
                        .rename_directory_to_no_replace(&staged_version, &destination, &final_path)
                        .is_err()
                    {
                        let _ = remove_pending_activation(&install_root, &pending_path);
                        return Err(install_error(
                            "Capability could not be installed atomically.",
                        ));
                    }
                    let post_install = verify_installed_root(&install_root, &install_entry)
                        .and_then(|pack| restore_executable_permissions(&pack));
                    if let Err(error) = post_install {
                        let _ = rollback_installed_directory(&install_root, &final_path);
                        let _ = remove_pending_activation(&install_root, &pending_path);
                        return Err(error);
                    }
                    true
                }
                Err(_) => return Err(install_error("Capability destination cannot be inspected.")),
            };
            remove_partial(&paths);
            drop(cleanup);
            Ok(CapabilityInstallOutcome {
                created_by_this_call,
                release_lock,
                pending_activation,
            })
        })
        .await
}

pub fn discard_catalog_partial(
    install_root: &Path,
    entry: &CapabilityCatalogEntry,
) -> Result<(), BackendError> {
    let install_root = install_root
        .canonicalize()
        .map_err(|_| install_error("Capability install directory cannot be resolved."))?;
    let paths = partial_paths(&install_root, entry)?;
    remove_partial(&paths);
    Ok(())
}

pub struct CapabilityInstallOutcome {
    created_by_this_call: bool,
    #[allow(dead_code)]
    release_lock: ReleaseInstallLock,
    pending_activation: Option<PathBuf>,
}

impl CapabilityInstallOutcome {
    pub fn mark_probed(&self, install_root: &Path) -> Result<(), BackendError> {
        if let Some(path) = self.pending_activation.as_ref() {
            update_pending_activation_phase(install_root, path, PendingActivationPhase::Probed)?;
        }
        Ok(())
    }

    pub fn activate(&mut self, install_root: &Path) -> Result<(), BackendError> {
        if let Some(path) = self.pending_activation.as_ref() {
            update_pending_activation_phase(install_root, path, PendingActivationPhase::Activated)?;
            remove_pending_activation(install_root, path)?;
        }
        self.pending_activation = None;
        Ok(())
    }

    pub fn rollback(
        self,
        install_root: &Path,
        entry: &CapabilityCatalogEntry,
    ) -> Result<(), BackendError> {
        if !self.created_by_this_call {
            return Ok(());
        }
        let version_root = install_root.join(&entry.capability_id).join(&entry.version);
        rollback_installed_directory(install_root, &version_root)?;
        if let Some(path) = self.pending_activation.as_ref() {
            remove_pending_activation(install_root, path)?;
        }
        Ok(())
    }

    pub fn rollback_with_receipt(
        self,
        install_root: &Path,
        entry: &CapabilityCatalogEntry,
        cause: BackendError,
    ) -> BackendError {
        let restored_previous_snapshot = self.created_by_this_call;
        match self.rollback(install_root, entry) {
            Ok(()) if restored_previous_snapshot => cause.with_details(serde_json::json!({
                "rollbackRestored": true,
                "failedCapabilityId": entry.capability_id,
                "failedVersion": entry.version,
            })),
            Ok(()) => cause,
            Err(rollback_error) => rollback_error.with_details(serde_json::json!({
                "rollbackRestored": false,
                "failedCapabilityId": entry.capability_id,
                "failedVersion": entry.version,
                "causeCode": cause.code,
            })),
        }
    }
}

fn rollback_installed_directory(
    install_root: &Path,
    version_root: &Path,
) -> Result<(), BackendError> {
    match std::fs::symlink_metadata(version_root) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => {
            return Err(install_error(
                "Capability rollback path cannot be inspected.",
            ))
        }
    }
    let binding = BoundProjectMutationRoot::bind(install_root, &version_root)
        .map_err(|_| install_error("Capability rollback path is unsafe."))?;
    binding
        .remove_directory_tree(&version_root)
        .map_err(|_| install_error("Unhealthy capability version could not be rolled back."))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityInstallPhase {
    Downloading,
    Verifying,
    Installing,
}

async fn download_archive(
    blocking_work: &BlockingWorkCoordinator,
    entry: &CapabilityCatalogEntry,
    paths: &PartialPaths,
    owner_task_id: &str,
    token: &CancellationToken,
    progress: &mut impl FnMut(CapabilityInstallPhase, u64, u64),
) -> Result<(), BackendError> {
    let result =
        download_archive_inner(blocking_work, entry, paths, owner_task_id, token, progress).await;
    if result
        .as_ref()
        .err()
        .is_some_and(|error| error.code == crate::errors::IMPORT_V2_CANCELLED)
    {
        remove_partial_background(blocking_work, paths).await?;
    }
    result
}

async fn download_archive_inner(
    blocking_work: &BlockingWorkCoordinator,
    entry: &CapabilityCatalogEntry,
    paths: &PartialPaths,
    owner_task_id: &str,
    token: &CancellationToken,
    progress: &mut impl FnMut(CapabilityInstallPhase, u64, u64),
) -> Result<(), BackendError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(30 * 60))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| install_error("Capability downloader is unavailable."))?;
    let download_root = paths
        .archive
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| install_error("Capability download path is invalid."))?;
    let download_root = download_root.to_path_buf();
    let archive_for_binding = paths.archive.clone();
    let (download_binding, _) = blocking_work
        .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
            BoundProjectMutationRoot::ensure_and_bind(&download_root, &archive_for_binding).map_err(
                |_| install_error("Capability download directory cannot be created safely."),
            )
        })
        .await?;
    let binding_for_resume = download_binding
        .try_clone()
        .map_err(|_| install_error("Capability partial cannot be pinned."))?;
    let paths_for_resume = paths.clone();
    let entry_for_resume = entry.clone();
    let owner_for_resume = owner_task_id.to_owned();
    let token_for_resume = token.clone();
    let (mut metadata, mut hasher) = blocking_work
        .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
            load_verified_partial(
                &binding_for_resume,
                &paths_for_resume,
                &entry_for_resume,
                &owner_for_resume,
                &token_for_resume,
            )
        })
        .await?;
    if metadata.downloaded_bytes == entry.compressed_bytes {
        let archive_for_hash = paths.archive.clone();
        let binding_for_hash = download_binding
            .try_clone()
            .map_err(|_| install_error("Capability partial cannot be pinned."))?;
        let token_for_hash = token.clone();
        let actual_sha256 = blocking_work
            .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
                hash_bound_file_cancellable(
                    &binding_for_hash,
                    &archive_for_hash,
                    Some(&token_for_hash),
                )
            })
            .await?;
        if actual_sha256.eq_ignore_ascii_case(&entry.archive_sha256) {
            progress(
                CapabilityInstallPhase::Downloading,
                entry.compressed_bytes,
                entry.compressed_bytes,
            );
            return Ok(());
        }
        remove_partial_background(blocking_work, paths).await?;
        metadata = PartialDownloadMetadata::new(entry, owner_task_id, 0);
        hasher = Sha256::new();
    }
    let mut request = client.get(&entry.url);
    if metadata.downloaded_bytes > 0 {
        request = request.header(
            reqwest::header::RANGE,
            format!("bytes={}-", metadata.downloaded_bytes),
        );
        if let Some(value) = metadata.etag.as_ref().or(metadata.last_modified.as_ref()) {
            request = request.header(reqwest::header::IF_RANGE, value);
        }
    }
    let mut response = send_cancellable(request, token, "Capability download failed.").await?;
    let mut response_url = response.url().as_str().to_owned();
    let mut response_etag = response_header(&response, reqwest::header::ETAG);
    let mut response_last_modified = response_header(&response, reqwest::header::LAST_MODIFIED);
    let mut resumed = metadata.downloaded_bytes > 0
        && response.status() == reqwest::StatusCode::PARTIAL_CONTENT
        && content_range_starts_at(&response, metadata.downloaded_bytes, entry.compressed_bytes)
        && validators_compatible(
            &metadata,
            response_etag.as_deref(),
            response_last_modified.as_deref(),
        );
    let restart = metadata.downloaded_bytes > 0 && !resumed;
    if restart {
        metadata = PartialDownloadMetadata::new(entry, owner_task_id, 0);
        hasher = Sha256::new();
        if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            response = send_cancellable(
                client.get(&entry.url),
                token,
                "Capability download restart failed.",
            )
            .await?;
            response_url = response.url().as_str().to_owned();
            response_etag = response_header(&response, reqwest::header::ETAG);
            response_last_modified = response_header(&response, reqwest::header::LAST_MODIFIED);
            resumed = false;
        }
    }
    let accepted_status = if metadata.downloaded_bytes == 0 {
        response.status() == reqwest::StatusCode::OK
    } else {
        resumed
    };
    if !accepted_status {
        return Err(install_error("Capability download response is invalid."));
    }
    let binding_for_open = download_binding
        .try_clone()
        .map_err(|_| install_error("Capability partial cannot be pinned."))?;
    let archive_for_open = paths.archive.clone();
    let standard_file = blocking_work
        .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
            let mut file = binding_for_open
                .open_regular_mutate_or_create(&archive_for_open, !resumed)
                .map_err(|_| install_error("Capability download file cannot be created."))?;
            if resumed {
                file.seek(SeekFrom::End(0))
                    .map_err(|_| install_error("Capability partial cannot be resumed."))?;
            }
            Ok(file)
        })
        .await?;
    let mut file = tokio::fs::File::from_std(standard_file);
    metadata.response_url = Some(response_url);
    metadata.etag = response_etag;
    metadata.last_modified = response_last_modified;
    metadata.owner_task_id = owner_task_id.into();
    let mut downloaded = metadata.downloaded_bytes;
    let mut last_checkpoint = downloaded;
    metadata.updated_at_unix_seconds = unix_seconds();
    write_partial_metadata_background(blocking_work, &paths.metadata, &metadata, token).await?;
    let mut stream = response.bytes_stream();
    loop {
        let next = loop {
            if token.is_cancelled() {
                if !token.is_pause_requested() {
                    remove_partial_background(blocking_work, paths).await?;
                }
                return Err(stopped(token));
            }
            match tokio::time::timeout(Duration::from_millis(250), stream.next()).await {
                Ok(next) => break next,
                Err(_) => continue,
            }
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|_| install_error("Capability download failed."))?;
        downloaded = downloaded
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| install_error("Capability download is too large."))?;
        if downloaded > entry.compressed_bytes || downloaded > MAX_ARCHIVE_BYTES {
            return Err(install_error("Capability download is too large."));
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|_| install_error("Capability download could not be saved."))?;
        progress(
            CapabilityInstallPhase::Downloading,
            downloaded,
            entry.compressed_bytes,
        );
        if downloaded.saturating_sub(last_checkpoint) >= PARTIAL_CHECKPOINT_BYTES {
            metadata.downloaded_bytes = downloaded;
            metadata.prefix_sha256 = format!("{:x}", hasher.clone().finalize());
            metadata.updated_at_unix_seconds = unix_seconds();
            file.sync_data()
                .await
                .map_err(|_| install_error("Capability download could not be checkpointed."))?;
            write_partial_metadata_background(blocking_work, &paths.metadata, &metadata, token)
                .await?;
            last_checkpoint = downloaded;
        }
    }
    file.flush()
        .await
        .map_err(|_| install_error("Capability download could not be finalized."))?;
    metadata.downloaded_bytes = downloaded;
    metadata.prefix_sha256 = format!("{:x}", hasher.finalize());
    metadata.updated_at_unix_seconds = unix_seconds();
    file.sync_data()
        .await
        .map_err(|_| install_error("Capability download could not be checkpointed."))?;
    write_partial_metadata_background(blocking_work, &paths.metadata, &metadata, token).await?;
    let archive_for_hash = paths.archive.clone();
    let binding_for_hash = download_binding
        .try_clone()
        .map_err(|_| install_error("Capability archive cannot be pinned."))?;
    let token_for_hash = token.clone();
    let actual_sha256 = blocking_work
        .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
            hash_bound_file_cancellable(&binding_for_hash, &archive_for_hash, Some(&token_for_hash))
        })
        .await?;
    if downloaded != entry.compressed_bytes
        || !actual_sha256.eq_ignore_ascii_case(&entry.archive_sha256)
    {
        remove_partial_background(blocking_work, paths).await?;
        return Err(install_error("Capability archive integrity check failed."));
    }
    Ok(())
}

async fn remove_partial_background(
    blocking_work: &BlockingWorkCoordinator,
    paths: &PartialPaths,
) -> Result<(), BackendError> {
    let paths = paths.clone();
    blocking_work
        .run(BlockingWorkClass::HeavyIo, move || {
            remove_partial(&paths);
            Ok(())
        })
        .await
}

async fn write_partial_metadata_background(
    blocking_work: &BlockingWorkCoordinator,
    path: &Path,
    metadata: &PartialDownloadMetadata,
    token: &CancellationToken,
) -> Result<(), BackendError> {
    let path = path.to_path_buf();
    let metadata = metadata.clone();
    blocking_work
        .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
            write_partial_metadata(&path, &metadata)
        })
        .await
}

async fn send_cancellable(
    request: reqwest::RequestBuilder,
    token: &CancellationToken,
    failure_message: &'static str,
) -> Result<reqwest::Response, BackendError> {
    let send = request.send();
    tokio::pin!(send);
    loop {
        tokio::select! {
            response = &mut send => {
                return response.map_err(|_| install_error(failure_message));
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if token.is_cancelled() {
                    return Err(stopped(token));
                }
            }
        }
    }
}

fn partial_paths(
    install_root: &Path,
    entry: &CapabilityCatalogEntry,
) -> Result<PartialPaths, BackendError> {
    if !valid_catalog_entry(entry) {
        return Err(install_error("Capability catalog entry is invalid."));
    }
    let base = install_root
        .join(DOWNLOADS_DIR)
        .join(format!("{}.partial", release_identity(entry)));
    Ok(PartialPaths {
        metadata: base.with_extension("partial.json"),
        archive: base,
    })
}

fn release_identity(entry: &CapabilityCatalogEntry) -> String {
    let identity = format!(
        "{}\0{}\0{}\0{}",
        entry.capability_id,
        entry.version,
        entry.target_triple,
        entry.archive_sha256.to_ascii_lowercase()
    );
    format!("{:x}", Sha256::digest(identity.as_bytes()))
}

fn pending_activation_path(install_root: &Path, identity: &str) -> PathBuf {
    install_root.join(format!("{PENDING_ACTIVATION_PREFIX}{identity}.json"))
}

fn write_pending_activation(
    install_root: &Path,
    path: &Path,
    pending: &PendingActivation,
) -> Result<(), BackendError> {
    let bytes = serde_json::to_vec(pending)
        .map_err(|_| install_error("Capability activation journal is invalid."))?;
    let binding = BoundProjectMutationRoot::bind(install_root, path)
        .map_err(|_| install_error("Capability activation journal path is unsafe."))?;
    binding
        .write_atomic_create_new(path, &bytes)
        .map_err(|_| install_error("Capability activation journal could not be created."))
}

fn update_pending_activation_phase(
    install_root: &Path,
    path: &Path,
    phase: PendingActivationPhase,
) -> Result<(), BackendError> {
    let binding = BoundProjectMutationRoot::bind(install_root, path)
        .map_err(|_| install_error("Capability activation journal path is unsafe."))?;
    let mut pending = read_bounded_regular(&binding, path)
        .and_then(|bytes| serde_json::from_slice::<PendingActivation>(&bytes).ok())
        .filter(|pending| pending.schema_version == 1)
        .ok_or_else(|| install_error("Capability activation journal is invalid."))?;
    pending.phase = phase;
    let bytes = serde_json::to_vec(&pending)
        .map_err(|_| install_error("Capability activation journal is invalid."))?;
    binding
        .write_atomic_replace(path, &bytes)
        .map_err(|_| install_error("Capability activation journal could not be updated."))
}

fn recover_pending_activation_for_release(
    install_root: &Path,
    identity: &str,
    entry: &CapabilityCatalogEntry,
) -> Result<(), BackendError> {
    let path = pending_activation_path(install_root, identity);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => {
            return Err(install_error(
                "Capability activation journal cannot be inspected.",
            ))
        }
    }
    let binding = BoundProjectMutationRoot::bind(install_root, &path)
        .map_err(|_| install_error("Capability activation journal path is unsafe."))?;
    let pending = read_bounded_regular(&binding, &path)
        .and_then(|bytes| serde_json::from_slice::<PendingActivation>(&bytes).ok())
        .filter(|pending| {
            pending.schema_version == 1
                && pending.identity == identity
                && pending.capability_id == entry.capability_id
                && pending.version == entry.version
                && !pending.owner_task_id.trim().is_empty()
        })
        .ok_or_else(|| {
            install_error("Capability activation journal is invalid and requires manual recovery.")
        })?;
    debug_assert_eq!(pending.identity, identity);
    let version_root = install_root.join(&entry.capability_id).join(&entry.version);
    rollback_installed_directory(install_root, &version_root)?;
    remove_pending_activation(install_root, &path)
}

fn remove_pending_activation(install_root: &Path, path: &Path) -> Result<(), BackendError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => {
            return Err(install_error(
                "Capability activation journal cannot be inspected.",
            ))
        }
    }
    let binding = BoundProjectMutationRoot::bind(install_root, path)
        .map_err(|_| install_error("Capability activation journal path is unsafe."))?;
    binding
        .remove_file(path)
        .map_err(|_| install_error("Capability activation journal could not be cleared."))
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_partial_metadata(
    path: &Path,
    metadata: &PartialDownloadMetadata,
) -> Result<(), BackendError> {
    let bytes = serde_json::to_vec_pretty(metadata)
        .map_err(|_| install_error("Capability download metadata is invalid."))?;
    let install_root = path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| install_error("Capability download metadata path is invalid."))?;
    let binding = BoundProjectMutationRoot::bind(install_root, path)
        .map_err(|_| install_error("Capability download metadata directory is unsafe."))?;
    binding
        .write_atomic_replace(path, &bytes)
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                binding.write_atomic_create_new(path, &bytes)
            } else {
                Err(error)
            }
        })
        .map_err(|_| install_error("Capability download metadata could not be saved."))
}

fn load_verified_partial(
    download_binding: &BoundProjectMutationRoot,
    paths: &PartialPaths,
    entry: &CapabilityCatalogEntry,
    owner_task_id: &str,
    token: &CancellationToken,
) -> Result<(PartialDownloadMetadata, Sha256), BackendError> {
    let fresh = || {
        (
            PartialDownloadMetadata::new(entry, owner_task_id, 0),
            Sha256::new(),
        )
    };
    if !paths.archive.exists() || !paths.metadata.exists() {
        remove_partial(paths);
        return Ok(fresh());
    }
    let metadata_binding = BoundProjectMutationRoot::bind_read(
        paths
            .archive
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| install_error("Capability download metadata path is invalid."))?,
        &paths.metadata,
    );
    let loaded = metadata_binding
        .ok()
        .and_then(|binding| read_partial_metadata(&binding, &paths.metadata));
    let Some(mut metadata) = loaded.filter(|metadata| metadata.matches_entry(entry)) else {
        remove_partial(paths);
        return Ok(fresh());
    };
    let file_metadata = match download_binding.open_regular(&paths.archive) {
        Ok(value) => value
            .metadata()
            .map_err(|_| install_error("Capability partial cannot be inspected."))?,
        _ => {
            remove_partial(paths);
            return Ok(fresh());
        }
    };
    if file_metadata.len() < metadata.downloaded_bytes {
        remove_partial(paths);
        return Ok(fresh());
    }
    if file_metadata.len() > metadata.downloaded_bytes {
        download_binding
            .open_regular_mutate_or_create(&paths.archive, false)
            .and_then(|file| file.set_len(metadata.downloaded_bytes))
            .map_err(|_| {
                install_error("Capability partial could not be restored to its checkpoint.")
            })?;
    }
    let (hasher, digest) = hash_prefix_cancellable(
        download_binding,
        &paths.archive,
        metadata.downloaded_bytes,
        Some(token),
    )?;
    if !digest.eq_ignore_ascii_case(&metadata.prefix_sha256) {
        remove_partial(paths);
        return Ok(fresh());
    }
    metadata.owner_task_id = owner_task_id.into();
    metadata.updated_at_unix_seconds = unix_seconds();
    write_partial_metadata(&paths.metadata, &metadata)?;
    Ok((metadata, hasher))
}

fn hash_prefix(
    binding: &BoundProjectMutationRoot,
    path: &Path,
    expected_bytes: u64,
) -> Result<(Sha256, String), BackendError> {
    hash_prefix_cancellable(binding, path, expected_bytes, None)
}

fn hash_prefix_cancellable(
    binding: &BoundProjectMutationRoot,
    path: &Path,
    expected_bytes: u64,
    token: Option<&CancellationToken>,
) -> Result<(Sha256, String), BackendError> {
    let mut file = binding
        .open_regular(path)
        .map_err(|_| install_error("Capability partial cannot be read."))?;
    let mut hasher = Sha256::new();
    let mut remaining = expected_bytes;
    let mut buffer = [0_u8; IO_BUFFER_BYTES];
    while remaining > 0 {
        if let Some(token) = token.filter(|token| token.is_cancelled()) {
            return Err(stopped(token));
        }
        let take = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
        let read = file
            .read(&mut buffer[..take])
            .map_err(|_| install_error("Capability partial cannot be read."))?;
        if read == 0 {
            return Err(install_error(
                "Capability partial ended before its checkpoint.",
            ));
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    let digest = format!("{:x}", hasher.clone().finalize());
    Ok((hasher, digest))
}

fn validators_compatible(
    metadata: &PartialDownloadMetadata,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> bool {
    (metadata.etag.is_some() || metadata.last_modified.is_some())
        && metadata
            .etag
            .as_deref()
            .is_none_or(|expected| etag == Some(expected))
        && metadata
            .last_modified
            .as_deref()
            .is_none_or(|expected| last_modified == Some(expected))
}

fn response_header(
    response: &reqwest::Response,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn content_range_starts_at(
    response: &reqwest::Response,
    expected_start: u64,
    expected_total: u64,
) -> bool {
    let Some(value) = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some((range, total)) = value
        .strip_prefix("bytes ")
        .and_then(|value| value.split_once('/'))
    else {
        return false;
    };
    let Some((start, _)) = range.split_once('-') else {
        return false;
    };
    start.parse::<u64>().ok() == Some(expected_start)
        && total.parse::<u64>().ok() == Some(expected_total)
}

fn remove_partial(paths: &PartialPaths) {
    let Some(install_root) = paths.archive.parent().and_then(Path::parent) else {
        return;
    };
    for path in [&paths.archive, &paths.metadata] {
        if let Ok(binding) = BoundProjectMutationRoot::bind(install_root, path) {
            let _ = binding.remove_file(path);
        }
    }
}

pub fn recover_install_root(
    install_root: &Path,
) -> Result<CapabilityInstallRecovery, BackendError> {
    std::fs::create_dir_all(install_root)
        .map_err(|_| install_error("Capability install directory is unavailable."))?;
    let mut result = CapabilityInstallRecovery::default();
    let root_binding = BoundProjectMutationRoot::bind(
        install_root,
        &install_root.join(".activation-reaper-binding-probe"),
    )
    .map_err(|_| install_error("Capability install directory is unsafe."))?;
    for entry in std::fs::read_dir(install_root)
        .map_err(|_| install_error("Capability install directory is unavailable."))?
        .flatten()
        .take(MAX_ARCHIVE_FILES)
    {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(identity) = name
            .strip_prefix(PENDING_ACTIVATION_PREFIX)
            .and_then(|value| value.strip_suffix(".json"))
        else {
            continue;
        };
        let _release_lock = acquire_release_lock(install_root, identity, "startup-reaper")
            .map_err(|_| install_error("Capability activation journal is currently locked."))?;
        let pending = read_bounded_regular(&root_binding, &path)
            .and_then(|bytes| serde_json::from_slice::<PendingActivation>(&bytes).ok())
            .filter(|pending| {
                pending.schema_version == 1
                    && pending.identity == identity
                    && !pending.owner_task_id.trim().is_empty()
                    && !pending.capability_id.is_empty()
                    && pending
                        .capability_id
                        .chars()
                        .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
                    && semver::Version::parse(&pending.version).is_ok()
            })
            .ok_or_else(|| {
                install_error(
                    "Capability activation journal is invalid and requires manual recovery.",
                )
            })?;
        let version_root = install_root
            .join(&pending.capability_id)
            .join(&pending.version);
        rollback_installed_directory(install_root, &version_root)?;
        result.rolled_back_pending += 1;
        match pending.phase {
            PendingActivationPhase::Prepared => result.rolled_back_prepared += 1,
            PendingActivationPhase::Probed => result.rolled_back_probed += 1,
            PendingActivationPhase::Activated => result.rolled_back_activated += 1,
        }
        remove_pending_activation(install_root, &path)?;
    }
    let downloads = install_root.join(DOWNLOADS_DIR);
    let download_binding =
        BoundProjectMutationRoot::bind(install_root, &downloads.join(".reaper-binding-probe"));
    if let (Ok(download_binding), Ok(entries)) = (download_binding, std::fs::read_dir(&downloads)) {
        let mut valid_archives = std::collections::HashSet::new();
        for entry in entries.flatten().take(MAX_ARCHIVE_FILES) {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let archive = path.with_extension("");
            let Some(identity) = partial_identity_from_metadata_path(&path) else {
                let _ = download_binding.remove_file(&path);
                result.removed_orphans += 1;
                continue;
            };
            let Some(_release_lock) =
                acquire_release_lock(install_root, identity, "startup-reaper").ok()
            else {
                valid_archives.insert(archive);
                continue;
            };
            let metadata = read_partial_metadata(&download_binding, &path);
            let keep = metadata.as_ref().is_some_and(|metadata| {
                valid_partial_metadata_shape(metadata)
                    && unix_seconds().saturating_sub(metadata.updated_at_unix_seconds)
                        <= PARTIAL_MAX_AGE.as_secs()
                    && download_binding
                        .open_regular(&archive)
                        .ok()
                        .and_then(|value| value.metadata().ok())
                        .is_some_and(|value| value.len() >= metadata.downloaded_bytes)
                    && (metadata.downloaded_bytes > MAX_REAPER_PREFIX_VERIFY_BYTES
                        || hash_prefix(&download_binding, &archive, metadata.downloaded_bytes)
                            .ok()
                            .is_some_and(|(_, digest)| {
                                digest.eq_ignore_ascii_case(&metadata.prefix_sha256)
                            }))
            });
            if keep {
                valid_archives.insert(archive);
                result.preserved_partials += 1;
            } else {
                let _ = download_binding.remove_file(&path);
                let _ = download_binding.remove_file(&archive);
                result.removed_orphans += 1;
            }
        }
        if let Ok(entries) = std::fs::read_dir(&downloads) {
            for entry in entries.flatten().take(MAX_ARCHIVE_FILES) {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) == Some("partial")
                    && !valid_archives.contains(&path)
                {
                    let identity = path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .and_then(|value| value.strip_suffix(".partial"));
                    if let Some(identity) = identity {
                        if let Ok(_release_lock) =
                            acquire_release_lock(install_root, identity, "startup-reaper")
                        {
                            let _ = download_binding.remove_file(&path);
                            result.removed_orphans += 1;
                        }
                    }
                }
            }
        }
    }
    for entry in std::fs::read_dir(install_root)
        .map_err(|_| install_error("Capability install directory is unavailable."))?
        .flatten()
        .take(MAX_ARCHIVE_FILES)
    {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".installing-") {
            let identity = name.trim_start_matches(".installing-");
            if let Ok(_release_lock) =
                acquire_release_lock(install_root, identity, "startup-reaper")
            {
                if remove_recovered_staging(install_root, &path) {
                    result.removed_staging += 1;
                }
            }
        } else if entry.file_type().ok().is_some_and(|value| value.is_dir())
            && name != DOWNLOADS_DIR
        {
            if let Ok(children) = std::fs::read_dir(&path) {
                for child in children.flatten().take(MAX_ARCHIVE_FILES) {
                    let child_name = child.file_name().to_string_lossy().into_owned();
                    if child_name.starts_with(".installing-")
                        && child.file_type().ok().is_some_and(|value| value.is_dir())
                    {
                        let identity = child_name.trim_start_matches(".installing-");
                        if let Ok(_release_lock) =
                            acquire_release_lock(install_root, identity, "startup-reaper")
                        {
                            if remove_recovered_staging(install_root, &child.path()) {
                                result.removed_staging += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

fn partial_identity_from_metadata_path(path: &Path) -> Option<&str> {
    path.file_name()
        .and_then(|value| value.to_str())
        .and_then(|value| value.strip_suffix(".partial.json"))
        .filter(|value| !value.is_empty())
}

fn read_partial_metadata(
    binding: &BoundProjectMutationRoot,
    path: &Path,
) -> Option<PartialDownloadMetadata> {
    let bytes = read_bounded_regular(binding, path)?;
    serde_json::from_slice(&bytes).ok()
}

fn read_bounded_regular(binding: &BoundProjectMutationRoot, path: &Path) -> Option<Vec<u8>> {
    let mut file = binding.open_regular(path).ok()?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_PARTIAL_METADATA_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_PARTIAL_METADATA_BYTES {
        return None;
    }
    Some(bytes)
}

fn valid_partial_metadata_shape(metadata: &PartialDownloadMetadata) -> bool {
    metadata.schema_version == PARTIAL_SCHEMA_VERSION
        && !metadata.capability_id.is_empty()
        && metadata
            .capability_id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
        && semver::Version::parse(&metadata.version).is_ok()
        && !metadata.target_triple.trim().is_empty()
        && url::Url::parse(&metadata.catalog_url)
            .ok()
            .is_some_and(|url| url.scheme() == "https" && url.host_str().is_some())
        && metadata.response_url.as_ref().is_none_or(|value| {
            url::Url::parse(value)
                .ok()
                .is_some_and(|url| url.scheme() == "https" && url.host_str().is_some())
        })
        && metadata.expected_length > 0
        && metadata.expected_length <= MAX_ARCHIVE_BYTES
        && metadata.expected_sha256.len() == 64
        && metadata
            .expected_sha256
            .chars()
            .all(|value| value.is_ascii_hexdigit())
        && !metadata.expected_sha256.chars().all(|value| value == '0')
        && metadata.downloaded_bytes <= metadata.expected_length
        && metadata.prefix_sha256.len() == 64
        && metadata
            .prefix_sha256
            .chars()
            .all(|value| value.is_ascii_hexdigit())
        && !metadata.owner_task_id.trim().is_empty()
}

fn remove_recovered_staging(install_root: &Path, path: &Path) -> bool {
    BoundProjectMutationRoot::bind(install_root, path)
        .and_then(|binding| binding.remove_directory_tree(path))
        .is_ok()
}

fn extract_and_verify(
    archive_path: &Path,
    staging_root: &Path,
    entry: &CapabilityCatalogEntry,
    token: &CancellationToken,
) -> Result<(), BackendError> {
    extract_and_verify_with_keys_cancellable(
        archive_path,
        staging_root,
        entry,
        trusted_keys(),
        Some(token),
    )
}

fn extract_and_verify_with_keys(
    archive_path: &Path,
    staging_root: &Path,
    entry: &CapabilityCatalogEntry,
    keys: HashMap<String, Vec<u8>>,
) -> Result<(), BackendError> {
    extract_and_verify_with_keys_cancellable(archive_path, staging_root, entry, keys, None)
}

fn extract_and_verify_with_keys_cancellable(
    archive_path: &Path,
    staging_root: &Path,
    entry: &CapabilityCatalogEntry,
    keys: HashMap<String, Vec<u8>>,
    token: Option<&CancellationToken>,
) -> Result<(), BackendError> {
    let install_root = staging_root
        .parent()
        .ok_or_else(|| install_error("Capability staging path is invalid."))?;
    prepare_staging_root(install_root, staging_root)?;
    let target = staging_root.join(&entry.capability_id).join(&entry.version);
    BoundProjectMutationRoot::ensure_and_bind(staging_root, &target.join(".binding-probe"))
        .map_err(|_| install_error("Capability staging directory is unavailable."))?;
    let archive_binding = BoundProjectMutationRoot::bind_read(install_root, archive_path)
        .map_err(|_| install_error("Capability archive path is unsafe."))?;
    let mut archive_file = archive_binding
        .open_regular(archive_path)
        .map_err(|_| install_error("Capability archive is unavailable."))?;
    let actual_archive_sha256 = hash_reader_cancellable(&mut archive_file, token)?;
    if !actual_archive_sha256.eq_ignore_ascii_case(&entry.archive_sha256) {
        return Err(install_error("Capability archive integrity check failed."));
    }
    archive_file
        .seek(SeekFrom::Start(0))
        .map_err(|_| install_error("Capability archive cannot be rewound."))?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|_| install_error("Capability archive is invalid."))?;
    if archive.len() == 0 || archive.len() > MAX_ARCHIVE_FILES {
        return Err(install_error("Capability archive file count is invalid."));
    }
    let mut installed = 0_u64;
    for index in 0..archive.len() {
        if let Some(token) = token.filter(|token| token.is_cancelled()) {
            return Err(stopped(token));
        }
        let mut file = archive
            .by_index(index)
            .map_err(|_| install_error("Capability archive is invalid."))?;
        let relative = file
            .enclosed_name()
            .ok_or_else(|| install_error("Capability archive contains an unsafe path."))?
            .to_path_buf();
        if relative.components().count() > MAX_ARCHIVE_DEPTH {
            return Err(install_error(
                "Capability archive path depth exceeds its limit.",
            ));
        }
        if let Some(kind) = file.unix_mode().map(|mode| mode & 0o170000) {
            if !matches!(kind, 0 | 0o040000 | 0o100000) {
                return Err(install_error(
                    "Capability archive contains a symbolic link or special file.",
                ));
            }
        }
        let output = target.join(relative);
        if file.is_dir() {
            BoundProjectMutationRoot::ensure_and_bind(staging_root, &output.join(".binding-probe"))
                .map_err(|_| install_error("Capability directory could not be extracted."))?;
            continue;
        }
        let file_size = file.size();
        installed = installed
            .checked_add(file_size)
            .ok_or_else(|| install_error("Capability archive expands beyond its limit."))?;
        if installed > entry.installed_bytes || installed > MAX_INSTALLED_BYTES {
            return Err(install_error(
                "Capability archive expands beyond its limit.",
            ));
        }
        let (output_binding, _) = BoundProjectMutationRoot::ensure_and_bind(staging_root, &output)
            .map_err(|_| install_error("Capability directory could not be extracted."))?;
        let mut output_file = output_binding
            .create_regular_new(&output)
            .map_err(|_| install_error("Capability file could not be extracted."))?;
        let mut written = 0_u64;
        let mut buffer = [0_u8; IO_BUFFER_BYTES];
        loop {
            if let Some(token) = token.filter(|token| token.is_cancelled()) {
                return Err(stopped(token));
            }
            let read = file
                .read(&mut buffer)
                .map_err(|_| install_error("Capability file could not be extracted."))?;
            if read == 0 {
                break;
            }
            let next_written = written
                .checked_add(read as u64)
                .ok_or_else(|| install_error("Capability archive expands beyond its limit."))?;
            let actual_installed = installed
                .checked_sub(file_size)
                .and_then(|value| value.checked_add(next_written))
                .ok_or_else(|| install_error("Capability archive expands beyond its limit."))?;
            if next_written > file_size
                || actual_installed > entry.installed_bytes
                || actual_installed > MAX_INSTALLED_BYTES
            {
                return Err(install_error(
                    "Capability archive expands beyond its declared size.",
                ));
            }
            std::io::Write::write_all(&mut output_file, &buffer[..read])
                .map_err(|_| install_error("Capability file could not be extracted."))?;
            written = next_written;
        }
        if written != file_size {
            return Err(install_error(
                "Capability file size changed during extraction.",
            ));
        }
    }
    if installed != entry.installed_bytes {
        return Err(install_error(
            "Capability installed size does not match the catalog.",
        ));
    }
    let pack = verify_installed_root_with_keys(staging_root, entry, keys)?;
    restore_executable_permissions(&pack)?;
    Ok(())
}

fn prepare_staging_root(install_root: &Path, staging_root: &Path) -> Result<(), BackendError> {
    let binding = BoundProjectMutationRoot::bind(install_root, staging_root)
        .map_err(|_| install_error("Capability staging directory is unsafe."))?;
    match std::fs::symlink_metadata(staging_root) {
        Ok(_) => binding
            .remove_directory_tree(staging_root)
            .map_err(|_| install_error("Capability staging directory cannot be recovered."))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            return Err(install_error(
                "Capability staging directory cannot be inspected.",
            ))
        }
    }
    binding
        .create_directory(staging_root)
        .map_err(|_| install_error("Capability staging directory is unavailable."))
}

fn verify_installed_root(
    install_root: &Path,
    entry: &CapabilityCatalogEntry,
) -> Result<super::capability_pack::ResolvedCapabilityPack, BackendError> {
    verify_installed_root_with_keys(install_root, entry, trusted_keys())
}

fn verify_installed_root_with_keys(
    install_root: &Path,
    entry: &CapabilityCatalogEntry,
    keys: HashMap<String, Vec<u8>>,
) -> Result<super::capability_pack::ResolvedCapabilityPack, BackendError> {
    let manager = CapabilityPackManager::new(install_root.to_path_buf(), keys);
    let requirement = CapabilityRequirement {
        capability_id: entry.capability_id.clone(),
        minimum_version: Some(entry.version.clone()),
        protocol_version: "2".into(),
        target_triple: entry.target_triple.clone(),
        accepted_license_expressions: vec![entry.license.clone()],
    };
    let pack = manager.resolve_version(&requirement, &entry.version)?;
    let manifest_path = pack.root.join("manifest.json");
    if !hash_file(&manifest_path)?.eq_ignore_ascii_case(&entry.manifest_sha256) {
        return Err(install_error(
            "Installed capability manifest does not match the catalog entry.",
        ));
    }
    let legacy_measurements_match = pack.manifest.schema_version != 1
        || (pack.manifest.archive_sha256 == entry.archive_sha256
            && pack.manifest.compressed_bytes == entry.compressed_bytes
            && pack.manifest.installed_bytes == entry.installed_bytes);
    if !legacy_measurements_match {
        return Err(install_error(
            "Installed capability does not match the signed catalog entry.",
        ));
    }
    Ok(pack)
}

fn ensure_destination_parent(install_root: &Path, final_parent: &Path) -> Result<(), BackendError> {
    match std::fs::symlink_metadata(final_parent) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || is_reparse(&metadata) || !metadata.is_dir() {
                return Err(install_error("Capability destination is unsafe."));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(final_parent)
                .map_err(|_| install_error("Capability destination is unavailable."))?;
        }
        Err(_) => return Err(install_error("Capability destination cannot be inspected.")),
    }
    let canonical = final_parent
        .canonicalize()
        .map_err(|_| install_error("Capability destination cannot be resolved."))?;
    if canonical.parent() != Some(install_root) {
        return Err(install_error(
            "Capability destination escaped the install root.",
        ));
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, BackendError> {
    let mut file = std::fs::File::open(path)
        .map_err(|_| install_error("Capability manifest cannot be read."))?;
    hash_reader(&mut file)
}

fn hash_bound_file_cancellable(
    binding: &BoundProjectMutationRoot,
    path: &Path,
    token: Option<&CancellationToken>,
) -> Result<String, BackendError> {
    let mut file = binding
        .open_regular(path)
        .map_err(|_| install_error("Capability archive cannot be read."))?;
    hash_reader_cancellable(&mut file, token)
}

fn hash_reader(file: &mut std::fs::File) -> Result<String, BackendError> {
    hash_reader_cancellable(file, None)
}

fn hash_reader_cancellable(
    file: &mut std::fs::File,
    token: Option<&CancellationToken>,
) -> Result<String, BackendError> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; IO_BUFFER_BYTES];
    loop {
        if let Some(token) = token.filter(|token| token.is_cancelled()) {
            return Err(stopped(token));
        }
        let read = file
            .read(&mut buffer)
            .map_err(|_| install_error("Capability manifest cannot be read."))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(windows)]
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse(_: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn restore_executable_permissions(
    pack: &super::capability_pack::ResolvedCapabilityPack,
) -> Result<(), BackendError> {
    use std::os::unix::fs::PermissionsExt;
    let mut executables = pack.manifest.executable_files.clone();
    if !executables
        .iter()
        .any(|path| path == &pack.manifest.entrypoint)
    {
        executables.push(pack.manifest.entrypoint.clone());
    }
    for relative in executables {
        let path = pack.root.join(Path::new(&relative));
        let mut permissions = std::fs::metadata(&path)
            .map_err(|_| install_error("Capability executable permissions are unavailable."))?
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(path, permissions).map_err(|_| {
            install_error("Capability executable permissions could not be restored.")
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn restore_executable_permissions(
    _: &super::capability_pack::ResolvedCapabilityPack,
) -> Result<(), BackendError> {
    Ok(())
}

fn trusted_keys() -> HashMap<String, Vec<u8>> {
    let encoded: HashMap<String, String> = serde_json::from_str(include_str!(concat!(
        env!("OUT_DIR"),
        "/capabilities/trusted-keys.json"
    )))
    .unwrap_or_default();
    encoded
        .into_iter()
        .filter_map(|(id, value)| decode_hex(&value).map(|key| (id, key)))
        .collect()
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

fn valid_catalog_entry(entry: &CapabilityCatalogEntry) -> bool {
    !entry.capability_id.is_empty()
        && entry
            .capability_id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || value == '-' || value == '_')
        && semver::Version::parse(&entry.version).is_ok()
        && !entry.target_triple.is_empty()
        && url::Url::parse(&entry.url)
            .ok()
            .is_some_and(|url| url.scheme() == "https" && url.host_str().is_some())
        && entry.archive_sha256.len() == 64
        && entry
            .archive_sha256
            .chars()
            .all(|value| value.is_ascii_hexdigit())
        && !entry.archive_sha256.chars().all(|value| value == '0')
        && entry.manifest_sha256.len() == 64
        && entry
            .manifest_sha256
            .chars()
            .all(|value| value.is_ascii_hexdigit())
        && !entry.manifest_sha256.chars().all(|value| value == '0')
        && entry.compressed_bytes > 0
        && entry.compressed_bytes <= MAX_ARCHIVE_BYTES
        && entry.installed_bytes > 0
        && entry.installed_bytes <= MAX_INSTALLED_BYTES
        && !entry.license.trim().is_empty()
}

struct ReleaseInstallLock {
    file: Option<std::fs::File>,
}

fn acquire_release_lock(
    install_root: &Path,
    identity: &str,
    owner_task_id: &str,
) -> Result<ReleaseInstallLock, BackendError> {
    let path = install_root.join(format!(".install-lock-{identity}"));
    let binding = BoundProjectMutationRoot::bind(install_root, &path)
        .map_err(|_| install_error("Capability install lock path is unsafe."))?;
    let mut file = binding
        .open_regular_mutate_or_create(&path, false)
        .map_err(|_| install_error("Capability install lock is unavailable."))?;
    try_lock_release_file(&file).map_err(|_| {
        install_error("This capability release is already being installed by another task.")
    })?;
    file.set_len(0)
        .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
        .and_then(|()| {
            std::io::Write::write_all(
                &mut file,
                format!("{}\n{}\n", std::process::id(), owner_task_id).as_bytes(),
            )
        })
        .and_then(|()| file.sync_all())
        .map_err(|_| install_error("Capability install lock owner could not be recorded."))?;
    Ok(ReleaseInstallLock { file: Some(file) })
}

#[cfg(unix)]
fn try_lock_release_file(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn try_lock_release_file(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        LockFileEx, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped = unsafe { std::mem::zeroed::<OVERLAPPED>() };
    let result = unsafe {
        LockFileEx(
            file.as_raw_handle() as _,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    };
    if result != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

impl Drop for ReleaseInstallLock {
    fn drop(&mut self) {
        // Keep the stable lock inode. Unlinking after unlock lets a waiter lock
        // the old inode while a third installer creates and locks a new one.
        if let Some(file) = self.file.take() {
            release_lock_file(file);
        }
    }
}

#[cfg(unix)]
fn release_lock_file(file: std::fs::File) {
    use std::os::fd::AsRawFd;

    // A concurrent fork can briefly inherit this open-file description before
    // CLOEXEC closes it. Explicitly unlock before closing so that inherited
    // duplicates cannot extend the single-flight lease past this guard.
    let _ = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    drop(file);
}

#[cfg(not(unix))]
fn release_lock_file(file: std::fs::File) {
    drop(file);
}

struct InstallCleanup {
    install_root: PathBuf,
    staging_root: PathBuf,
}

impl Drop for InstallCleanup {
    fn drop(&mut self) {
        if let Ok(binding) = BoundProjectMutationRoot::bind(&self.install_root, &self.staging_root)
        {
            let _ = binding.remove_directory_tree(&self.staging_root);
        }
    }
}

fn install_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_CAPABILITY_INSTALL_FAILED", message, true, true)
}

fn cancelled() -> BackendError {
    BackendError::new(
        crate::errors::IMPORT_V2_CANCELLED,
        "Capability installation was cancelled.",
        true,
        true,
    )
}

fn stopped(token: &CancellationToken) -> BackendError {
    if token.is_pause_requested() {
        BackendError::new(
            "APP_CAPABILITY_INSTALL_PAUSED",
            "Capability installation was paused and can be resumed.",
            true,
            false,
        )
    } else {
        cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::import_v2::capability_pack::{CapabilityPackFile, CapabilityPackManifest};
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    struct SignedFixture {
        root: PathBuf,
        archive: PathBuf,
        entry: CapabilityCatalogEntry,
        keys: HashMap<String, Vec<u8>>,
    }

    impl SignedFixture {
        fn new(runtime: &[u8], key_id: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "llm-wiki-installer-fixture-{}",
                uuid::Uuid::new_v4()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
            let pair = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
            let mut manifest = CapabilityPackManifest {
                schema_version: 2,
                pack_id: "fixture".into(),
                version: "1.0.0".into(),
                protocol_version: "2".into(),
                target_triples: vec!["x86_64-pc-windows-msvc".into()],
                archive_sha256: String::new(),
                license_expression: "MIT".into(),
                entrypoint: "runner.bin".into(),
                entrypoint_args: Vec::new(),
                executable_files: vec!["runner.bin".into()],
                compressed_bytes: 0,
                installed_bytes: 0,
                signing_key_id: key_id.into(),
                signature: String::new(),
                files: vec![CapabilityPackFile {
                    path: "runner.bin".into(),
                    sha256: format!("{:x}", Sha256::digest(runtime)),
                    bytes: runtime.len() as u64,
                }],
            };
            manifest.signature = pair
                .sign(&manifest.signing_payload().unwrap())
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
            manifest_bytes.push(b'\n');
            let archive = root.join("fixture.zip");
            let file = std::fs::File::create(&archive).unwrap();
            let mut writer = ZipWriter::new(file);
            let options = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .unix_permissions(0o644);
            writer.start_file("manifest.json", options).unwrap();
            writer.write_all(&manifest_bytes).unwrap();
            writer.start_file("runner.bin", options).unwrap();
            writer.write_all(runtime).unwrap();
            writer.finish().unwrap();
            let entry = CapabilityCatalogEntry {
                capability_id: "fixture".into(),
                version: "1.0.0".into(),
                target_triple: "x86_64-pc-windows-msvc".into(),
                url: "https://example.test/fixture.zip".into(),
                archive_sha256: hash_file(&archive).unwrap(),
                manifest_sha256: format!("{:x}", Sha256::digest(&manifest_bytes)),
                compressed_bytes: std::fs::metadata(&archive).unwrap().len(),
                installed_bytes: (manifest_bytes.len() + runtime.len()) as u64,
                model_bytes: None,
                license: "MIT".into(),
            };
            Self {
                root,
                archive,
                entry,
                keys: HashMap::from([(key_id.into(), pair.public_key().as_ref().to_vec())]),
            }
        }

        fn install_root(&self) -> PathBuf {
            self.root.join("installed")
        }
    }

    impl Drop for SignedFixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).ok();
        }
    }

    #[test]
    fn embedded_catalog_matches_its_build_record_and_release_gate() {
        let record: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("OUT_DIR"),
            "/capabilities/embed-record.json"
        )))
        .unwrap();
        let catalog: InstallCatalog = serde_json::from_str(include_str!(concat!(
            env!("OUT_DIR"),
            "/capabilities/install-catalog.json"
        )))
        .unwrap();
        let mode = record["mode"].as_str().unwrap();
        assert!(matches!(mode, "development" | "distributable"));
        assert_eq!(catalog.schema_version, 1);
        assert_eq!(
            catalog.entries.len() as u64,
            record["entryCount"].as_u64().unwrap()
        );
        assert!(catalog.entries.iter().all(valid_catalog_entry));
        assert!(catalog
            .entries
            .iter()
            .all(|entry| !entry.url.contains("placeholder")));
        if mode == "distributable" {
            assert!(
                !catalog.entries.is_empty(),
                "release builds cannot embed an empty capability catalog"
            );
            assert_eq!(
                catalog_availability(),
                CapabilityCatalogAvailability::Available
            );
        } else if catalog.entries.is_empty() {
            assert_eq!(
                catalog_availability(),
                CapabilityCatalogAvailability::CatalogUnavailable
            );
        }
    }

    #[test]
    fn catalog_rejects_insecure_or_unbounded_entries() {
        let entry = CapabilityCatalogEntry {
            capability_id: "asr-sensevoice-small".into(),
            version: "1.0.0".into(),
            target_triple: "x86_64-pc-windows-msvc".into(),
            url: "http://example.test/pack.zip".into(),
            archive_sha256: "a".repeat(64),
            manifest_sha256: "b".repeat(64),
            compressed_bytes: 1,
            installed_bytes: 1,
            model_bytes: None,
            license: "Apache-2.0".into(),
        };
        assert!(!valid_catalog_entry(&entry));
        assert!(catalog_entry("asr-sensevoice-small", &entry.target_triple).is_none());
    }

    #[test]
    fn catalog_selects_the_latest_semver_for_a_capability_target() {
        let entry = |version: &str| CapabilityCatalogEntry {
            capability_id: "browser-runtime".into(),
            version: version.into(),
            target_triple: "x86_64-pc-windows-msvc".into(),
            url: format!("https://example.test/browser-runtime-{version}.zip"),
            archive_sha256: "ab".repeat(32),
            manifest_sha256: "cd".repeat(32),
            compressed_bytes: 1,
            installed_bytes: 1,
            model_bytes: None,
            license: "Apache-2.0".into(),
        };
        let selected = select_catalog_entry(
            vec![entry("1.2.0"), entry("2.0.0"), entry("1.10.0")],
            "browser-runtime",
            "x86_64-pc-windows-msvc",
        )
        .unwrap();
        assert_eq!(selected.version, "2.0.0");
    }

    #[test]
    fn schema_v2_extraction_verifies_manifest_inventory_and_catalog_binding() {
        let fixture = SignedFixture::new(b"verified-runtime", "release-a");
        let install_root = fixture.install_root();
        extract_and_verify_with_keys(
            &fixture.archive,
            &install_root,
            &fixture.entry,
            fixture.keys.clone(),
        )
        .unwrap();
        verify_installed_root_with_keys(&install_root, &fixture.entry, fixture.keys.clone())
            .unwrap();
    }

    #[test]
    fn same_version_with_a_different_signed_manifest_is_a_collision() {
        let first = SignedFixture::new(b"first-runtime", "release-a");
        let second = SignedFixture::new(b"second-runtime", "release-b");
        let install_root = first.install_root();
        let mut keys = first.keys.clone();
        keys.extend(second.keys.clone());
        extract_and_verify_with_keys(&first.archive, &install_root, &first.entry, keys.clone())
            .unwrap();
        let error =
            verify_installed_root_with_keys(&install_root, &second.entry, keys).unwrap_err();
        assert!(error.message.contains("manifest does not match"));
    }

    #[test]
    fn extraction_rejects_parent_traversal_before_writing_outside_staging() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let unsafe_archive = fixture.root.join("unsafe.zip");
        let file = std::fs::File::create(&unsafe_archive).unwrap();
        let mut writer = ZipWriter::new(file);
        writer
            .start_file("../escape", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"nope").unwrap();
        writer.finish().unwrap();
        let error = extract_and_verify_with_keys(
            &unsafe_archive,
            &fixture.install_root(),
            &CapabilityCatalogEntry {
                compressed_bytes: std::fs::metadata(&unsafe_archive).unwrap().len(),
                installed_bytes: 4,
                archive_sha256: hash_file(&unsafe_archive).unwrap(),
                ..fixture.entry.clone()
            },
            fixture.keys.clone(),
        )
        .unwrap_err();
        assert!(error.message.contains("unsafe path"));
        assert!(!fixture.root.join("escape").exists());
    }

    #[cfg(unix)]
    #[test]
    fn extraction_restores_only_signed_executable_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        extract_and_verify_with_keys(
            &fixture.archive,
            &install_root,
            &fixture.entry,
            fixture.keys.clone(),
        )
        .unwrap();
        let mode = std::fs::metadata(install_root.join("fixture/1.0.0/runner.bin"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn destination_parent_cannot_redirect_through_a_symlink() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("llm-wiki-installer-root-{}", uuid::Uuid::new_v4()));
        let outside = std::env::temp_dir().join(format!(
            "llm-wiki-installer-outside-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("fixture")).unwrap();
        let canonical_root = root.canonicalize().unwrap();
        assert!(ensure_destination_parent(&canonical_root, &root.join("fixture")).is_err());
        std::fs::remove_dir_all(root).ok();
        std::fs::remove_dir_all(outside).ok();
    }

    #[test]
    fn partial_identity_is_deterministic_and_binds_the_signed_release() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let first = partial_paths(&fixture.install_root(), &fixture.entry).unwrap();
        let second = partial_paths(&fixture.install_root(), &fixture.entry).unwrap();
        assert_eq!(first, second);
        assert!(first
            .archive
            .starts_with(fixture.install_root().join(".downloads")));

        let mut next = fixture.entry.clone();
        next.archive_sha256 = "ef".repeat(32);
        assert_ne!(
            first,
            partial_paths(&fixture.install_root(), &next).unwrap()
        );
    }

    #[test]
    fn startup_recovery_preserves_valid_partial_but_reaps_corrupt_and_staging_state() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        std::fs::create_dir_all(&install_root).unwrap();
        let paths = partial_paths(&install_root, &fixture.entry).unwrap();
        std::fs::create_dir_all(paths.archive.parent().unwrap()).unwrap();
        std::fs::write(&paths.archive, b"part").unwrap();
        let mut metadata = PartialDownloadMetadata::new(&fixture.entry, "task-old", 4);
        metadata.prefix_sha256 = format!("{:x}", Sha256::digest(b"part"));
        write_partial_metadata(&paths.metadata, &metadata).unwrap();
        let corrupt = paths.archive.parent().unwrap().join("corrupt.partial.json");
        std::fs::write(&corrupt, b"not-json").unwrap();
        let staging = install_root.join("fixture/.installing-crashed");
        std::fs::create_dir_all(&staging).unwrap();

        let recovered = recover_install_root(&install_root).unwrap();

        assert_eq!(recovered.preserved_partials, 1);
        assert!(paths.archive.exists());
        assert!(paths.metadata.exists());
        assert!(!corrupt.exists());
        assert!(!staging.exists());
    }

    #[test]
    fn release_lock_is_single_flight_and_reaper_skips_active_staging() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        std::fs::create_dir_all(&install_root).unwrap();
        let identity = release_identity(&fixture.entry);
        let staging = install_root.join(format!(".installing-{identity}"));
        std::fs::create_dir_all(&staging).unwrap();

        let lock = acquire_release_lock(&install_root, &identity, "task-a").unwrap();
        assert!(acquire_release_lock(&install_root, &identity, "task-b").is_err());
        recover_install_root(&install_root).unwrap();
        assert!(staging.exists());

        drop(lock);
        recover_install_root(&install_root).unwrap();
        assert!(!staging.exists());
    }

    #[cfg(unix)]
    #[test]
    fn release_lock_unlocks_before_an_inherited_descriptor_closes() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        std::fs::create_dir_all(&install_root).unwrap();
        let identity = release_identity(&fixture.entry);

        let lock = acquire_release_lock(&install_root, &identity, "task-a").unwrap();
        let inherited = lock.file.as_ref().unwrap().try_clone().unwrap();
        drop(lock);

        let reacquired = acquire_release_lock(&install_root, &identity, "task-b").unwrap();
        drop(reacquired);
        drop(inherited);
    }

    #[test]
    fn startup_recovery_reaps_a_partial_with_a_wrong_prefix_digest() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        std::fs::create_dir_all(&install_root).unwrap();
        let paths = partial_paths(&install_root, &fixture.entry).unwrap();
        std::fs::create_dir_all(paths.archive.parent().unwrap()).unwrap();
        std::fs::write(&paths.archive, b"part").unwrap();
        let mut metadata = PartialDownloadMetadata::new(&fixture.entry, "task-old", 4);
        metadata.prefix_sha256 = "ab".repeat(32);
        write_partial_metadata(&paths.metadata, &metadata).unwrap();

        let recovered = recover_install_root(&install_root).unwrap();

        assert_eq!(recovered.removed_orphans, 1);
        assert!(!paths.archive.exists());
        assert!(!paths.metadata.exists());
    }

    #[test]
    fn rollback_removes_only_a_directory_created_by_that_install_outcome() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        let version_root = install_root.join("fixture/1.0.0");
        std::fs::create_dir_all(&version_root).unwrap();
        let identity = release_identity(&fixture.entry);
        let release_lock = acquire_release_lock(&install_root, &identity, "task-existing").unwrap();
        CapabilityInstallOutcome {
            created_by_this_call: false,
            release_lock,
            pending_activation: None,
        }
        .rollback(&install_root, &fixture.entry)
        .unwrap();
        assert!(version_root.exists());

        let release_lock = acquire_release_lock(&install_root, &identity, "task-created").unwrap();
        CapabilityInstallOutcome {
            created_by_this_call: true,
            release_lock,
            pending_activation: None,
        }
        .rollback(&install_root, &fixture.entry)
        .unwrap();
        assert!(!version_root.exists());
    }

    #[test]
    fn rollback_receipt_is_emitted_only_when_this_install_removed_the_failed_version() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        let version_root = install_root.join("fixture/1.0.0");
        std::fs::create_dir_all(&version_root).unwrap();
        let identity = release_identity(&fixture.entry);
        let release_lock = acquire_release_lock(&install_root, &identity, "task-created").unwrap();
        let error = CapabilityInstallOutcome {
            created_by_this_call: true,
            release_lock,
            pending_activation: None,
        }
        .rollback_with_receipt(
            &install_root,
            &fixture.entry,
            install_error("Capability health check failed."),
        );

        assert_eq!(
            error
                .details
                .as_ref()
                .and_then(|details| details["rollbackRestored"].as_bool()),
            Some(true)
        );
        assert!(!version_root.exists());
    }

    #[test]
    fn startup_recovery_rolls_back_a_version_left_pending_before_health() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        let version_root = install_root.join("fixture/1.0.0");
        std::fs::create_dir_all(&version_root).unwrap();
        let identity = release_identity(&fixture.entry);
        let pending_path = pending_activation_path(&install_root, &identity);
        write_pending_activation(
            &install_root,
            &pending_path,
            &PendingActivation {
                schema_version: 1,
                identity,
                capability_id: "fixture".into(),
                version: "1.0.0".into(),
                owner_task_id: "crashed-task".into(),
                phase: PendingActivationPhase::Prepared,
            },
        )
        .unwrap();

        let recovered = recover_install_root(&install_root).unwrap();

        assert_eq!(recovered.rolled_back_pending, 1);
        assert_eq!(recovered.rolled_back_prepared, 1);
        assert!(!version_root.exists());
        assert!(!pending_path.exists());
    }

    #[test]
    fn startup_recovery_reports_probed_and_activated_journal_rollbacks() {
        for phase in [
            PendingActivationPhase::Probed,
            PendingActivationPhase::Activated,
        ] {
            let fixture = SignedFixture::new(b"runtime", "release-a");
            let install_root = fixture.install_root();
            let version_root = install_root.join("fixture/1.0.0");
            std::fs::create_dir_all(&version_root).unwrap();
            let identity = release_identity(&fixture.entry);
            let pending_path = pending_activation_path(&install_root, &identity);
            write_pending_activation(
                &install_root,
                &pending_path,
                &PendingActivation {
                    schema_version: 1,
                    identity,
                    capability_id: "fixture".into(),
                    version: "1.0.0".into(),
                    owner_task_id: "crashed-task".into(),
                    phase,
                },
            )
            .unwrap();

            let recovered = recover_install_root(&install_root).unwrap();

            assert_eq!(recovered.rolled_back_pending, 1);
            assert_eq!(
                recovered.rolled_back_probed,
                usize::from(phase == PendingActivationPhase::Probed)
            );
            assert_eq!(
                recovered.rolled_back_activated,
                usize::from(phase == PendingActivationPhase::Activated)
            );
            assert!(!version_root.exists());
            assert!(!pending_path.exists());
        }
    }

    #[test]
    fn same_process_retry_fails_closed_on_a_damaged_activation_journal() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        let version_root = install_root.join("fixture/1.0.0");
        std::fs::create_dir_all(&version_root).unwrap();
        let identity = release_identity(&fixture.entry);
        let pending_path = pending_activation_path(&install_root, &identity);
        std::fs::write(&pending_path, b"damaged-journal").unwrap();

        let error =
            recover_pending_activation_for_release(&install_root, &identity, &fixture.entry)
                .unwrap_err();

        assert_eq!(error.code, "IMPORT_V2_CAPABILITY_INSTALL_FAILED");
        assert!(version_root.exists());
        assert!(pending_path.exists());
    }

    #[test]
    fn startup_recovery_fails_closed_on_a_truncated_activation_journal() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let install_root = fixture.install_root();
        let version_root = install_root.join("fixture/1.0.0");
        std::fs::create_dir_all(&version_root).unwrap();
        let identity = release_identity(&fixture.entry);
        let pending_path = pending_activation_path(&install_root, &identity);
        std::fs::write(&pending_path, br#"{"schemaVersion":1,"identity":"#).unwrap();

        let error = recover_install_root(&install_root).unwrap_err();

        assert_eq!(error.code, "IMPORT_V2_CAPABILITY_INSTALL_FAILED");
        assert!(version_root.exists());
        assert!(pending_path.exists());
    }

    #[test]
    fn extraction_rejects_paths_beyond_the_depth_limit() {
        let fixture = SignedFixture::new(b"runtime", "release-a");
        let unsafe_archive = fixture.root.join("too-deep.zip");
        let file = std::fs::File::create(&unsafe_archive).unwrap();
        let mut writer = ZipWriter::new(file);
        let path = (0..=MAX_ARCHIVE_DEPTH)
            .map(|index| format!("d{index}"))
            .collect::<Vec<_>>()
            .join("/");
        writer
            .start_file(format!("{path}/payload"), SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"nope").unwrap();
        writer.finish().unwrap();
        let error = extract_and_verify_with_keys(
            &unsafe_archive,
            &fixture.install_root(),
            &CapabilityCatalogEntry {
                compressed_bytes: std::fs::metadata(&unsafe_archive).unwrap().len(),
                installed_bytes: 4,
                archive_sha256: hash_file(&unsafe_archive).unwrap(),
                ..fixture.entry.clone()
            },
            fixture.keys.clone(),
        )
        .unwrap_err();
        assert!(error.message.contains("depth"));
    }

    #[tokio::test]
    async fn verified_partial_resumes_with_range_and_rechecks_the_full_archive_hash() {
        use std::sync::{Arc, Mutex};
        use tokio::io::{AsyncReadExt, AsyncWriteExt as _};

        let fixture = SignedFixture::new(b"runtime-for-range", "release-a");
        let bytes = std::fs::read(&fixture.archive).unwrap();
        let split = bytes.len() / 2;
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let url = format!(
            "http://127.0.0.1:{}/fixture.zip",
            listener.local_addr().unwrap().port()
        );
        let observed_request = Arc::new(Mutex::new(String::new()));
        let observed = Arc::clone(&observed_request);
        let remaining = bytes[split..].to_vec();
        let total = bytes.len();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 8 * 1024];
            let read = stream.read(&mut request).await.unwrap();
            *observed.lock().unwrap() = String::from_utf8_lossy(&request[..read]).into_owned();
            let header = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nETag: \"release-a\"\r\nConnection: close\r\n\r\n",
                remaining.len(),
                split,
                total - 1,
                total,
            );
            stream.write_all(header.as_bytes()).await.unwrap();
            stream.write_all(&remaining).await.unwrap();
        });
        let mut entry = fixture.entry.clone();
        entry.url = url.clone();
        let paths = PartialPaths {
            archive: fixture.install_root().join(".downloads/range.partial"),
            metadata: fixture.install_root().join(".downloads/range.partial.json"),
        };
        std::fs::create_dir_all(paths.archive.parent().unwrap()).unwrap();
        std::fs::write(&paths.archive, &bytes[..split]).unwrap();
        let mut metadata = PartialDownloadMetadata::new(&entry, "old-task", split as u64);
        metadata.response_url = Some(url);
        metadata.etag = Some("\"release-a\"".into());
        metadata.prefix_sha256 = format!("{:x}", Sha256::digest(&bytes[..split]));
        write_partial_metadata(&paths.metadata, &metadata).unwrap();

        let blocking_work = BlockingWorkCoordinator::default();
        download_archive(
            &blocking_work,
            &entry,
            &paths,
            "new-task",
            &CancellationToken::new(),
            &mut |_, _, _| {},
        )
        .await
        .unwrap();
        server.await.unwrap();

        assert!(observed_request
            .lock()
            .unwrap()
            .to_ascii_lowercase()
            .contains(&format!("range: bytes={split}-")));
        assert_eq!(std::fs::read(paths.archive).unwrap(), bytes);
    }

    #[tokio::test]
    async fn range_ignored_by_server_discards_the_old_prefix_and_restarts_safely() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt as _};

        let fixture = SignedFixture::new(b"runtime-for-restart", "release-a");
        let bytes = std::fs::read(&fixture.archive).unwrap();
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let url = format!(
            "http://127.0.0.1:{}/fixture.zip",
            listener.local_addr().unwrap().port()
        );
        let response_bytes = bytes.clone();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 8 * 1024];
            let _ = stream.read(&mut request).await.unwrap();
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"new-release\"\r\nConnection: close\r\n\r\n",
                response_bytes.len(),
            );
            stream.write_all(header.as_bytes()).await.unwrap();
            stream.write_all(&response_bytes).await.unwrap();
        });
        let mut entry = fixture.entry.clone();
        entry.url = url.clone();
        let paths = PartialPaths {
            archive: fixture.install_root().join(".downloads/restart.partial"),
            metadata: fixture
                .install_root()
                .join(".downloads/restart.partial.json"),
        };
        std::fs::create_dir_all(paths.archive.parent().unwrap()).unwrap();
        std::fs::write(&paths.archive, b"old-prefix").unwrap();
        let mut metadata = PartialDownloadMetadata::new(&entry, "old-task", 10);
        metadata.response_url = Some(url);
        metadata.etag = Some("\"old-release\"".into());
        metadata.prefix_sha256 = format!("{:x}", Sha256::digest(b"old-prefix"));
        write_partial_metadata(&paths.metadata, &metadata).unwrap();

        let blocking_work = BlockingWorkCoordinator::default();
        download_archive(
            &blocking_work,
            &entry,
            &paths,
            "new-task",
            &CancellationToken::new(),
            &mut |_, _, _| {},
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(std::fs::read(paths.archive).unwrap(), bytes);
    }

    #[test]
    fn full_archive_hash_honors_an_active_cancellation_token() {
        let fixture = SignedFixture::new(b"runtime-for-cancelled-hash", "release-a");
        let binding = BoundProjectMutationRoot::bind_read(&fixture.root, &fixture.archive).unwrap();
        let token = CancellationToken::new();
        token.cancel();

        let error =
            hash_bound_file_cancellable(&binding, &fixture.archive, Some(&token)).unwrap_err();

        assert_eq!(error.code, crate::errors::IMPORT_V2_CANCELLED);
    }

    #[tokio::test]
    async fn active_cancel_while_waiting_for_headers_removes_the_partial() {
        use tokio::io::AsyncReadExt;

        let fixture = SignedFixture::new(b"runtime-for-active-cancel", "release-a");
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let mut entry = fixture.entry.clone();
        entry.url = format!(
            "http://127.0.0.1:{}/fixture.zip",
            listener.local_addr().unwrap().port()
        );
        let paths = PartialPaths {
            archive: fixture.install_root().join(".downloads/cancel.partial"),
            metadata: fixture
                .install_root()
                .join(".downloads/cancel.partial.json"),
        };
        std::fs::create_dir_all(paths.archive.parent().unwrap()).unwrap();
        std::fs::write(&paths.archive, b"x").unwrap();
        let mut metadata = PartialDownloadMetadata::new(&entry, "old-task", 1);
        metadata.prefix_sha256 = format!("{:x}", Sha256::digest(b"x"));
        write_partial_metadata(&paths.metadata, &metadata).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 8 * 1024];
            let _ = stream.read(&mut request).await;
            std::future::pending::<()>().await;
        });
        let token = CancellationToken::new();
        let cancellation = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancellation.cancel();
        });

        let blocking_work = BlockingWorkCoordinator::default();
        let error = download_archive(
            &blocking_work,
            &entry,
            &paths,
            "new-task",
            &token,
            &mut |_, _, _| {},
        )
        .await
        .unwrap_err();
        server.abort();

        assert_eq!(error.code, crate::errors::IMPORT_V2_CANCELLED);
        assert!(!paths.archive.exists());
        assert!(!paths.metadata.exists());
    }
}
