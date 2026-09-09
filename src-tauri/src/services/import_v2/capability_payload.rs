//! Content-addressed transport for signed ZIPs. Model/runtime members keep their
//! own chunk boundaries across runner updates; old catalogs use the full downloader.
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::BackendError;
use crate::tasks::task_model::CancellationToken;
use crate::utils::safe_project_dir::BoundProjectMutationRoot;

pub const MAX_CHUNK_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveChunk {
    pub offset: u64,
    pub bytes: u64,
    pub sha256: String,
}

pub fn valid_chunks(chunks: &[ArchiveChunk], total: u64) -> bool {
    if chunks.is_empty() {
        return true;
    }
    if chunks.len() > 4096 {
        return false;
    }
    let mut offset = 0u64;
    for chunk in chunks {
        if chunk.offset != offset
            || chunk.bytes == 0
            || chunk.bytes > MAX_CHUNK_BYTES
            || chunk.sha256.len() != 64
            || !chunk.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return false;
        }
        let Some(next) = offset.checked_add(chunk.bytes) else {
            return false;
        };
        offset = next;
    }
    offset == total
}

/// ZIP members larger than 1 MiB start independent sequences. No new release
/// assets or new signing authority are needed; the full archive is still verified.
pub fn archive_chunks(path: &Path) -> Result<Vec<ArchiveChunk>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let total = file.metadata().map_err(|e| e.to_string())?.len();
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut boundaries = vec![0, total];
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.compressed_size() >= 1024 * 1024 {
            boundaries.extend([
                entry.header_start(),
                entry.data_start() + entry.compressed_size(),
            ]);
        }
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut file = zip.into_inner();
    let mut chunks = Vec::new();
    for pair in boundaries.windows(2) {
        let mut offset = pair[0];
        while offset < pair[1] {
            let bytes = MAX_CHUNK_BYTES.min(pair[1] - offset);
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| e.to_string())?;
            let mut hash = Sha256::new();
            let mut part = (&mut file).take(bytes);
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let read = part.read(&mut buffer).map_err(|e| e.to_string())?;
                if read == 0 {
                    break;
                }
                hash.update(&buffer[..read]);
            }
            chunks.push(ArchiveChunk {
                offset,
                bytes,
                sha256: format!("{:x}", hash.finalize()),
            });
            offset += bytes;
        }
    }
    Ok(chunks)
}

fn error(message: impl Into<String>) -> BackendError {
    BackendError::new("APP_CAPABILITY_NETWORK_UNAVAILABLE", message, true, false)
}

fn invalid(message: impl Into<String>) -> BackendError {
    BackendError::new("IMPORT_V2_CAPABILITY_INVALID", message, false, true)
}

/// `false` asks the existing resumable downloader to handle a server without Range.
pub(super) async fn download_chunks(
    root: &Path,
    entry: &super::capability_installer::CapabilityCatalogEntry,
    destination: &Path,
    token: &CancellationToken,
    progress: &mut impl FnMut(super::capability_installer::CapabilityInstallPhase, u64, u64),
) -> Result<bool, BackendError> {
    if entry.archive_chunks.is_empty() {
        return Ok(false);
    }
    let cache = root.join(".payload-cache");
    let (binding, _) = BoundProjectMutationRoot::ensure_and_bind(root, &cache.join("probe"))
        .map_err(|e| error(e.to_string()))?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(90))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| error(e.to_string()))?;
    let mut completed = 0;
    for chunk in &entry.archive_chunks {
        if token.is_cancelled() {
            return Err(super::capability_installer::stopped(token));
        }
        let cached = cache.join(&chunk.sha256);
        // Each cache entry is small and untrusted until its digest is checked.
        let reusable = binding
            .open_regular(&cached)
            .ok()
            .and_then(|mut f| {
                if f.metadata().ok()?.len() != chunk.bytes {
                    return None;
                }
                let (sha, _) = super::artifact::hash_reader(&mut f).ok()?;
                Some(sha == chunk.sha256)
            })
            .unwrap_or(false);
        if !reusable {
            let mut bytes = Vec::new();
            for attempt in 0..=3 {
                let response = tokio::select! {
                    result = client.get(&entry.url).header(reqwest::header::RANGE,
                        format!("bytes={}-{}", chunk.offset, chunk.offset + chunk.bytes - 1)).send() => result,
                    _ = wait_cancelled(token) => return Err(super::capability_installer::stopped(token)),
                };
                let result = async {
                    let mut response = response.map_err(|e| error(e.to_string()))?;
                    if response.status() == reqwest::StatusCode::OK {
                        return Ok(false);
                    }
                    if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
                        let message =
                            format!("Capability server returned HTTP {}", response.status());
                        if response.status().is_server_error() || response.status().as_u16() == 429
                        {
                            let seconds = response
                                .headers()
                                .get(reqwest::header::RETRY_AFTER)
                                .and_then(|v| v.to_str().ok())
                                .and_then(|v| v.parse::<u64>().ok());
                            return Err(error(message).with_details(
                                serde_json::json!({ "retryAfterSeconds": seconds }),
                            ));
                        }
                        return Err(invalid(message));
                    }
                    bytes.clear();
                    while let Some(part) =
                        response.chunk().await.map_err(|e| error(e.to_string()))?
                    {
                        if bytes.len() as u64 + part.len() as u64 > chunk.bytes {
                            return Err(invalid("Invalid capability range size"));
                        }
                        bytes.extend_from_slice(&part);
                    }
                    if bytes.len() as u64 != chunk.bytes
                        || format!("{:x}", Sha256::digest(&bytes)) != chunk.sha256
                    {
                        return Err(invalid("Capability range digest mismatch"));
                    }
                    Ok(true)
                };
                let result = tokio::select! {
                    result = result => result,
                    _ = wait_cancelled(token) => return Err(super::capability_installer::stopped(token)),
                };
                match result {
                    Ok(false) => return Ok(false),
                    Ok(true) => break,
                    Err(e) if attempt == 3 || e.code != "APP_CAPABILITY_NETWORK_UNAVAILABLE" => {
                        return Err(e)
                    }
                    Err(e) => {
                        let seconds = e
                            .details
                            .as_ref()
                            .and_then(|d| d.get("retryAfterSeconds"))
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(1 << attempt);
                        if seconds > 120 {
                            return Err(e);
                        }
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(seconds)) => {},
                            _ = wait_cancelled(token) => return Err(super::capability_installer::stopped(token)),
                        }
                    }
                }
            }
            // Atomic cache replacement also handles concurrent preparation of
            // different capabilities sharing this same chunk.
            binding
                .write_atomic_replace(&cached, &bytes)
                .map_err(|e| error(e.to_string()))?;
        }
        completed += chunk.bytes;
        progress(
            super::capability_installer::CapabilityInstallPhase::Downloading,
            completed,
            entry.compressed_bytes,
        );
    }
    let root = root.to_path_buf();
    let destination = destination.to_path_buf();
    let chunks = entry.archive_chunks.clone();
    let token = token.clone();
    tokio::task::spawn_blocking(move || -> Result<(), BackendError> {
        let (output_binding, _) = BoundProjectMutationRoot::ensure_and_bind(&root, &destination)
            .map_err(|e| error(e.to_string()))?;
        let mut output = output_binding
            .open_regular_mutate_or_create(&destination, true)
            .map_err(|e| error(e.to_string()))?;
        for chunk in &chunks {
            if token.is_cancelled() {
                return Err(super::capability_installer::stopped(&token));
            }
            let mut file = binding
                .open_regular(&cache.join(&chunk.sha256))
                .map_err(|e| error(e.to_string()))?;
            let mut verified =
                super::artifact::VerifiedReader::new(&mut file, &chunk.sha256, chunk.bytes);
            std::io::copy(&mut verified, &mut output).map_err(|e| invalid(e.to_string()))?;
        }
        output
            .flush()
            .and_then(|_| output.sync_all())
            .map_err(|e| error(e.to_string()))?;
        Ok(())
    })
    .await
    .map_err(|e| error(e.to_string()))??;

    Ok(true)
}

async fn wait_cancelled(token: &CancellationToken) {
    while !token.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Immutable large payloads share physical storage across packs and versions.
/// Every path still appears in its own signed inventory, so execution remains
/// bound to a complete, independently verifiable pack.
pub(super) fn share_installed_payloads(
    root: &Path,
    pack: &super::capability_pack::ResolvedCapabilityPack,
) -> std::io::Result<u64> {
    let objects = root.join(".payload-objects");
    let (cache, _) = BoundProjectMutationRoot::ensure_and_bind(root, &objects.join("probe"))?;
    let mut reused = 0;
    for item in pack
        .manifest
        .files
        .iter()
        .filter(|file| file.bytes >= 1024 * 1024)
    {
        let source = pack.root.join(&item.path);
        let source_binding = BoundProjectMutationRoot::bind(root, &source)?;
        let executable = pack.manifest.executable_files.contains(&item.path);
        let object = objects.join(format!("{}-{}", item.sha256, u8::from(executable)));
        if source_binding
            .hard_link_to(&source, &cache, &object)
            .is_ok()
        {
            continue;
        }
        let mut file = cache.open_regular(&object)?;
        let (sha, bytes) = super::artifact::hash_reader(&mut file)?;
        if sha != item.sha256 || bytes != item.bytes {
            continue;
        }
        let temporary = source.with_file_name(format!(".shared-{}", uuid::Uuid::new_v4()));
        cache.hard_link_to(&object, &source_binding, &temporary)?;
        if let Err(error) = source_binding.replace_existing(&temporary, &source) {
            let _ = source_binding.remove_file(&temporary);
            return Err(error);
        }
        reused += bytes;
    }
    Ok(reused)
}

#[cfg(test)]
mod tests {
    use super::super::capability_installer::CapabilityCatalogEntry;
    use super::*;

    #[test]
    fn model_chunks_survive_runner_update_and_move_within_archive() {
        let root = tempfile::tempdir().unwrap();
        let mut seed = 1234567u64;
        let model: Vec<u8> = (0..2 * 1024 * 1024)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect();
        let make = |name: &str, runner: &[u8]| {
            let file = root.path().join(name);
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("runner/index.mjs", options).unwrap();
            zip.write_all(runner).unwrap();
            zip.start_file("models/model.onnx", options).unwrap();
            zip.write_all(&model).unwrap();
            zip.finish().unwrap();
            archive_chunks(&file).unwrap()
        };
        let before = make("before.zip", b"runner version one");
        let after = make(
            "after.zip",
            b"runner version two has a different implementation",
        );
        let reused: u64 = after
            .iter()
            .filter(|new| before.iter().any(|old| old.sha256 == new.sha256))
            .map(|c| c.bytes)
            .sum();
        assert!(reused >= 2 * 1024 * 1024);
        assert!(after
            .iter()
            .any(|new| !before.iter().any(|old| old.sha256 == new.sha256)));
        assert!(valid_chunks(&after, after.iter().map(|c| c.bytes).sum()));
        eprintln!("model_update reused_transport_bytes={reused}");
    }

    #[cfg(unix)]
    #[test]
    fn identical_models_share_storage_while_versions_keep_verified_paths() {
        use super::super::capability_pack::{CapabilityPackManifest, ResolvedCapabilityPack};
        use std::os::unix::fs::MetadataExt;
        let root = tempfile::tempdir().unwrap();
        let payload = vec![37u8; 1024 * 1024];
        let sha = format!("{:x}", Sha256::digest(&payload));
        let make_pack = |name: &str| {
            let directory = root.path().join(name);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(directory.join("model.onnx"), &payload).unwrap();
            ResolvedCapabilityPack {
                manifest: serde_json::from_value::<CapabilityPackManifest>(serde_json::json!({
                    "schemaVersion":2,"packId":name,"version":"1.0.0","protocolVersion":"2","targetTriples":[],
                    "archiveSha256":"", "licenseExpression":"MIT", "entrypoint":"model.onnx", "compressedBytes":0,"installedBytes":payload.len(),
                    "signingKeyId":"fixture", "signature":"", "files":[{"path":"model.onnx","sha256":sha,"bytes":payload.len()}]
                })).unwrap(),
                root: directory.clone(), entrypoint: directory.join("model.onnx"), entrypoint_sha256: sha.clone(),
            }
        };
        let first = make_pack("first");
        let second = make_pack("second");
        assert_eq!(share_installed_payloads(root.path(), &first).unwrap(), 0);
        assert_eq!(
            share_installed_payloads(root.path(), &second).unwrap(),
            payload.len() as u64
        );
        assert_eq!(
            std::fs::metadata(&first.entrypoint).unwrap().ino(),
            std::fs::metadata(&second.entrypoint).unwrap().ino()
        );
        std::fs::remove_dir_all(&first.root).unwrap();
        assert_eq!(std::fs::read(&second.entrypoint).unwrap(), payload);
    }

    #[tokio::test]
    async fn temporary_failure_retries_then_reuses_verified_chunks_offline() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let bytes = b"shared-model-and-runtime";
        let root = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/pack.zip", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0u8; 4096];
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(String::from_utf8_lossy(&buffer[..count]).contains("range: bytes=0-23"));
                if attempt == 0 {
                    stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
                } else {
                    stream.write_all(format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes 0-{}/{}\r\nConnection: close\r\n\r\n", bytes.len(), bytes.len()-1, bytes.len()).as_bytes()).await.unwrap();
                    stream.write_all(bytes).await.unwrap();
                }
            }
        });
        let mut entry = CapabilityCatalogEntry {
            capability_id: "fixture".into(),
            version: "1.0.0".into(),
            target_triple: "aarch64-apple-darwin".into(),
            url,
            archive_sha256: format!("{:x}", Sha256::digest(bytes)),
            manifest_sha256: "a".repeat(64),
            signing_key_id: "fixture".into(),
            compressed_bytes: bytes.len() as u64,
            installed_bytes: 24,
            model_bytes: None,
            license: "MIT".into(),
            archive_chunks: vec![ArchiveChunk {
                offset: 0,
                bytes: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
            }],
        };
        let output = root.path().join(".downloads/result.zip");
        assert!(download_chunks(
            root.path(),
            &entry,
            &output,
            &CancellationToken::default(),
            &mut |_, _, _| {}
        )
        .await
        .unwrap());
        server.await.unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
        entry.url = "http://127.0.0.1:1/unavailable".into();
        assert!(download_chunks(
            root.path(),
            &entry,
            &output,
            &CancellationToken::default(),
            &mut |_, _, _| {}
        )
        .await
        .unwrap());
    }
}
