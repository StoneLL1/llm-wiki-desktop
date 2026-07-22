use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::errors::BackendError;
use crate::models::import_v2_file::CapabilityRequirement;
use crate::tasks::task_model::CancellationToken;

use super::capability_pack::CapabilityPackManager;

const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_INSTALLED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 20_000;

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

pub fn catalog_entry(capability_id: &str, target_triple: &str) -> Option<CapabilityCatalogEntry> {
    let catalog: InstallCatalog = serde_json::from_str(include_str!(
        "../../../../capabilities/install-catalog.json"
    ))
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
    install_root: &Path,
    entry: &CapabilityCatalogEntry,
    token: &CancellationToken,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf, BackendError> {
    if !valid_catalog_entry(entry) {
        return Err(install_error("Capability catalog entry is invalid."));
    }
    std::fs::create_dir_all(install_root)
        .map_err(|_| install_error("Capability install directory is unavailable."))?;
    let install_root = install_root
        .canonicalize()
        .map_err(|_| install_error("Capability install directory cannot be resolved."))?;
    let nonce = uuid::Uuid::new_v4().to_string();
    let archive_path = install_root.join(format!(".download-{nonce}.zip"));
    let staging_root = install_root.join(format!(".installing-{nonce}"));
    let cleanup = InstallCleanup {
        archive_path: archive_path.clone(),
        staging_root: staging_root.clone(),
    };
    download_archive(entry, &archive_path, token, &mut progress).await?;
    if token.is_cancelled() {
        return Err(cancelled());
    }
    let entry_for_extract = entry.clone();
    let archive_for_extract = archive_path.clone();
    let staging_for_extract = staging_root.clone();
    tokio::task::spawn_blocking(move || {
        extract_and_verify(
            &archive_for_extract,
            &staging_for_extract,
            &entry_for_extract,
        )
    })
    .await
    .map_err(|_| install_error("Capability verification worker failed."))??;
    if token.is_cancelled() {
        return Err(cancelled());
    }
    let staged_version = staging_root.join(&entry.capability_id).join(&entry.version);
    let final_parent = install_root.join(&entry.capability_id);
    let final_path = final_parent.join(&entry.version);
    ensure_destination_parent(&install_root, &final_parent)?;
    if std::fs::symlink_metadata(&final_path).is_ok() {
        let metadata = std::fs::symlink_metadata(&final_path)
            .map_err(|_| install_error("Capability destination is unavailable."))?;
        if metadata.file_type().is_symlink() || is_reparse(&metadata) || !metadata.is_dir() {
            return Err(install_error("Capability destination is unsafe."));
        }
        let pack = verify_installed_root(&install_root, entry).map_err(|_| {
            install_error("Capability version collides with a different signed release.")
        })?;
        restore_executable_permissions(&pack)?;
    } else {
        std::fs::rename(&staged_version, &final_path)
            .map_err(|_| install_error("Capability could not be installed atomically."))?;
        let pack = verify_installed_root(&install_root, entry)?;
        restore_executable_permissions(&pack)?;
    }
    drop(cleanup);
    Ok(final_path)
}

async fn download_archive(
    entry: &CapabilityCatalogEntry,
    archive_path: &Path,
    token: &CancellationToken,
    progress: &mut impl FnMut(u64, u64),
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
    let response = client
        .get(&entry.url)
        .send()
        .await
        .map_err(|_| install_error("Capability download failed."))?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length != entry.compressed_bytes)
    {
        return Err(install_error("Capability download response is invalid."));
    }
    let mut file = tokio::fs::File::create(archive_path)
        .await
        .map_err(|_| install_error("Capability download file cannot be created."))?;
    let mut stream = response.bytes_stream();
    let mut hasher = Sha256::new();
    let mut downloaded = 0_u64;
    while let Some(chunk) = stream.next().await {
        if token.is_cancelled() {
            return Err(cancelled());
        }
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
        progress(downloaded, entry.compressed_bytes);
    }
    file.flush()
        .await
        .map_err(|_| install_error("Capability download could not be finalized."))?;
    let actual_sha256 = format!("{:x}", hasher.finalize());
    if downloaded != entry.compressed_bytes
        || !actual_sha256.eq_ignore_ascii_case(&entry.archive_sha256)
    {
        return Err(install_error("Capability archive integrity check failed."));
    }
    Ok(())
}

fn extract_and_verify(
    archive_path: &Path,
    staging_root: &Path,
    entry: &CapabilityCatalogEntry,
) -> Result<(), BackendError> {
    extract_and_verify_with_keys(archive_path, staging_root, entry, trusted_keys())
}

fn extract_and_verify_with_keys(
    archive_path: &Path,
    staging_root: &Path,
    entry: &CapabilityCatalogEntry,
    keys: HashMap<String, Vec<u8>>,
) -> Result<(), BackendError> {
    let target = staging_root.join(&entry.capability_id).join(&entry.version);
    std::fs::create_dir_all(&target)
        .map_err(|_| install_error("Capability staging directory is unavailable."))?;
    let archive_file = std::fs::File::open(archive_path)
        .map_err(|_| install_error("Capability archive is unavailable."))?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|_| install_error("Capability archive is invalid."))?;
    if archive.len() == 0 || archive.len() > MAX_ARCHIVE_FILES {
        return Err(install_error("Capability archive file count is invalid."));
    }
    let mut installed = 0_u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| install_error("Capability archive is invalid."))?;
        let relative = file
            .enclosed_name()
            .ok_or_else(|| install_error("Capability archive contains an unsafe path."))?
            .to_path_buf();
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(install_error(
                "Capability archive contains a symbolic link.",
            ));
        }
        let output = target.join(relative);
        if file.is_dir() {
            std::fs::create_dir_all(&output)
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
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| install_error("Capability directory could not be extracted."))?;
        }
        let mut output_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)
            .map_err(|_| install_error("Capability file could not be extracted."))?;
        let written = std::io::copy(&mut file, &mut output_file)
            .map_err(|_| install_error("Capability file could not be extracted."))?;
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
    if let Ok(metadata) = std::fs::symlink_metadata(final_parent) {
        if metadata.file_type().is_symlink() || is_reparse(&metadata) || !metadata.is_dir() {
            return Err(install_error("Capability destination is unsafe."));
        }
    } else {
        std::fs::create_dir(final_parent)
            .map_err(|_| install_error("Capability destination is unavailable."))?;
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
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
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
    let encoded: HashMap<String, String> =
        serde_json::from_str(include_str!("../../../../capabilities/trusted-keys.json"))
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

struct InstallCleanup {
    archive_path: PathBuf,
    staging_root: PathBuf,
}

impl Drop for InstallCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.archive_path);
        let _ = std::fs::remove_dir_all(&self.staging_root);
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
    fn source_catalog_is_valid_and_never_exposes_placeholder_downloads() {
        let catalog: InstallCatalog = serde_json::from_str(include_str!(
            "../../../../capabilities/install-catalog.json"
        ))
        .unwrap();
        assert_eq!(catalog.schema_version, 1);
        assert!(catalog.entries.iter().all(valid_catalog_entry));
        assert!(catalog
            .entries
            .iter()
            .all(|entry| !entry.url.contains("placeholder")));
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
}
