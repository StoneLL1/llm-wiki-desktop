//! Legacy ZIP chunk metadata remains readable for published catalogs. Installation
//! uses one resumable stream; verified installed payloads can still share storage.
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    use super::*;
    use std::io::Write;

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
}
