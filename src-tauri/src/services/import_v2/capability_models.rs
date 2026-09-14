//! Platform-independent model data. Official hashes are checked on download or
//! import; normal use loads the installed data without re-hashing large models.
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::errors::BackendError;
use crate::services::{BlockingWorkClass, BlockingWorkCoordinator};
use crate::tasks::task_model::CancellationToken;
use crate::utils::safe_project_dir::BoundProjectMutationRoot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelFile {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    #[serde(default)]
    pub urls: Vec<String>,
}

pub fn valid_model_files(files: &[ModelFile]) -> bool {
    let mut paths = HashSet::new();
    files.len() <= 1024
        && files
            .iter()
            .try_fold(0u64, |sum, file| sum.checked_add(file.bytes))
            .is_some_and(|sum| sum <= 16 * 1024 * 1024 * 1024)
        && files.iter().all(|file| {
            let path = Path::new(&file.path);
            file.path.starts_with("models/")
                && !file.path.contains(['\\', ':'])
                && !file
                    .path
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
                && path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_)))
                && paths.insert(file.path.to_lowercase())
                && matches!(
                    path.extension().and_then(|v| v.to_str()),
                    Some("onnx" | "bin" | "txt" | "json" | "safetensors" | "md")
                )
                && file.bytes > 0
                && file.bytes <= 8 * 1024 * 1024 * 1024
                && file.sha256.len() == 64
                && file
                    .sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                && file.sha256.bytes().any(|c| c != b'0')
                && !file.urls.is_empty()
                && file.urls.len() <= 8
                && file.urls.iter().all(|value| {
                    reqwest::Url::parse(value).is_ok_and(|url| {
                        url.scheme() == "https"
                            && url.host_str().is_some()
                            && url.username().is_empty()
                            && url.password().is_none()
                            && url.query().is_none()
                            && url.fragment().is_none()
                    })
                })
        })
}

fn failure(message: &str) -> BackendError {
    BackendError::new("IMPORT_V2_CAPABILITY_INSTALL_FAILED", message, true, true)
}

fn check_cancelled(token: &CancellationToken) -> Result<(), BackendError> {
    if token.is_cancelled() {
        Err(super::capability_installer::stopped(token))
    } else {
        Ok(())
    }
}

async fn regular_file(path: &Path, bytes: u64) -> bool {
    tokio::fs::symlink_metadata(path)
        .await
        .is_ok_and(|meta| meta.is_file() && meta.len() == bytes)
}

// Poll during a stalled request as CancellationToken intentionally has no async notifier.
async fn cancellable<T>(
    future: impl std::future::Future<Output = T>,
    token: &CancellationToken,
) -> Result<T, BackendError> {
    tokio::pin!(future);
    loop {
        tokio::select! {
            value = &mut future => return Ok(value),
            _ = tokio::time::sleep(Duration::from_millis(100)) => check_cancelled(token)?,
        }
    }
}

async fn stream_model(
    source: Option<&Path>,
    url: Option<&str>,
    destination: &Path,
    model: &ModelFile,
    token: &CancellationToken,
    progress: &mut impl FnMut(u64),
) -> Result<(), BackendError> {
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .await
        .map_err(|_| failure("Cannot create temporary model file."))?;
    let mut digest = Sha256::new();
    let mut count = 0u64;
    if let Some(source) = source {
        if !regular_file(source, model.bytes).await {
            return Err(failure(
                "Offline model size does not match the selected model.",
            ));
        }
        let mut input = tokio::fs::File::open(source)
            .await
            .map_err(|_| failure("Cannot read offline model."))?;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            check_cancelled(token)?;
            let read = input
                .read(&mut buffer)
                .await
                .map_err(|_| failure("Cannot read offline model."))?;
            if read == 0 {
                break;
            }
            count += read as u64;
            if count > model.bytes {
                return Err(failure("Model exceeds the expected size."));
            }
            digest.update(&buffer[..read]);
            output
                .write_all(&buffer[..read])
                .await
                .map_err(|_| failure("Cannot save model."))?;
            progress(count);
        }
    } else {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(60))
            .https_only(true)
            .build()
            .map_err(|_| failure("Cannot create model downloader."))?;
        let response = cancellable(
            client
                .get(url.ok_or_else(|| failure("Model has no download source."))?)
                .send(),
            token,
        )
        .await?
        .map_err(|_| {
            failure("Model download failed. Check the network or import the offline model.")
        })?;
        if !response.status().is_success() {
            return Err(failure(&format!(
                "Model download returned HTTP {}.",
                response.status().as_u16()
            )));
        }
        if response
            .content_length()
            .is_some_and(|bytes| bytes != model.bytes)
        {
            return Err(failure("Model download size does not match the catalog."));
        }
        let mut stream = response.bytes_stream();
        while let Some(chunk) = cancellable(stream.next(), token).await? {
            let chunk = chunk.map_err(|_| failure("Model download was interrupted."))?;
            count += chunk.len() as u64;
            if count > model.bytes {
                return Err(failure("Model exceeds the expected size."));
            }
            digest.update(&chunk);
            output
                .write_all(&chunk)
                .await
                .map_err(|_| failure("Cannot save model."))?;
            progress(count);
        }
    }
    if count != model.bytes
        || !format!("{:x}", digest.finalize()).eq_ignore_ascii_case(&model.sha256)
    {
        return Err(failure(
            "Model checksum does not match. Download or select the complete official model.",
        ));
    }
    output
        .sync_all()
        .await
        .map_err(|_| failure("Cannot finish saving model."))?;
    check_cancelled(token)
}

/// `offline_dir` is the selected archive's parent. Its presence forbids all network
/// fallback. Complete offline bundles place data at models/<sha256>/<filename>.
#[allow(clippy::too_many_arguments)]
pub async fn materialize_models(
    blocking_work: &BlockingWorkCoordinator,
    install_root: &Path,
    pack_root: &Path,
    files: &[ModelFile],
    offline_dir: Option<&Path>,
    token: &CancellationToken,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), BackendError> {
    if !valid_model_files(files) {
        return Err(failure("Model catalog is invalid."));
    }
    if files.is_empty() {
        return Ok(());
    }
    let cache = install_root.join(".models");
    tokio::fs::create_dir_all(&cache)
        .await
        .map_err(|_| failure("Cannot create model directory."))?;
    let install_root = tokio::fs::canonicalize(install_root)
        .await
        .map_err(|_| failure("Cannot resolve installation directory."))?;
    let cache = tokio::fs::canonicalize(cache)
        .await
        .map_err(|_| failure("Cannot resolve model directory."))?;
    let pack_root = tokio::fs::canonicalize(pack_root)
        .await
        .map_err(|_| failure("Cannot resolve capability directory."))?;
    if !cache.starts_with(&install_root) || !pack_root.starts_with(&install_root) {
        return Err(failure(
            "Model directory escapes the installation directory.",
        ));
    }
    let total = files.iter().map(|file| file.bytes).sum();
    let mut complete = 0;
    for model in files {
        check_cancelled(token)?;
        let object = cache.join(model.sha256.to_lowercase());
        if !regular_file(&object, model.bytes).await {
            let temporary = cache.join(format!(".part-{}", uuid::Uuid::new_v4()));
            let source = if let Some(directory) = offline_dir {
                let name = Path::new(&model.path)
                    .file_name()
                    .ok_or_else(|| failure("Invalid model name."))?;
                let candidates = [
                    directory.join("models").join(&model.sha256).join(name),
                    directory.join(&model.path),
                ];
                let root = tokio::fs::canonicalize(directory)
                    .await
                    .map_err(|_| failure("Offline model directory is unavailable."))?;
                let mut selected = None;
                for candidate in candidates {
                    if regular_file(&candidate, model.bytes).await
                        && tokio::fs::canonicalize(&candidate)
                            .await
                            .is_ok_and(|path| path.starts_with(&root))
                    {
                        selected = Some(candidate);
                        break;
                    }
                }
                Some(selected.ok_or_else(|| failure("Offline bundle is missing model files. Keep the models folder beside the selected capability ZIP."))?)
            } else {
                None
            };
            let result = if let Some(source) = &source {
                stream_model(Some(source), None, &temporary, model, token, &mut |bytes| {
                    progress(complete + bytes, total)
                })
                .await
            } else {
                let mut result = Err(failure(
                    "Model has no available download source. Import an offline bundle.",
                ));
                for url in &model.urls {
                    result =
                        stream_model(None, Some(url), &temporary, model, token, &mut |bytes| {
                            progress(complete + bytes, total)
                        })
                        .await;
                    if result.is_ok() {
                        break;
                    }
                    let _ = tokio::fs::remove_file(&temporary).await;
                    check_cancelled(token)?;
                }
                result
            };
            if let Err(error) = result {
                let _ = tokio::fs::remove_file(&temporary).await;
                return Err(error);
            }
            // Rename works on filesystems without hard links. Concurrent downloads
            // have the same verified digest and may safely converge on one object.
            if !regular_file(&object, model.bytes).await {
                if tokio::fs::symlink_metadata(&object).await.is_ok() {
                    let _ = tokio::fs::remove_file(&object).await;
                }
                if tokio::fs::rename(&temporary, &object).await.is_err()
                    && !regular_file(&object, model.bytes).await
                {
                    let _ = tokio::fs::remove_file(&temporary).await;
                    return Err(failure("Cannot publish downloaded model."));
                }
            }
            let _ = tokio::fs::remove_file(&temporary).await;
        }
        let destination = pack_root.join(&model.path);
        let source = object.clone();
        let root = install_root.clone();
        let pack = pack_root.clone();
        let copy_token = token.clone();
        blocking_work
            .run_cancellable(BlockingWorkClass::HeavyIo, token.clone(), move || {
                install_model_file(&root, &pack, &source, &destination, &copy_token, true)
            })
            .await?;
        check_cancelled(token)?;
        complete += model.bytes;
        progress(complete, total);
    }
    Ok(())
}

fn install_model_file(
    install_root: &Path,
    pack_root: &Path,
    source: &Path,
    destination: &Path,
    token: &CancellationToken,
    try_link: bool,
) -> Result<(), BackendError> {
    check_cancelled(token)?;
    let source_binding = BoundProjectMutationRoot::bind_read(install_root, source)
        .map_err(|_| failure("Cannot open cached model."))?;
    let (destination_binding, _) =
        BoundProjectMutationRoot::ensure_and_bind(pack_root, destination)
            .map_err(|_| failure("Cannot create safe model destination."))?;
    if try_link
        && source_binding
            .hard_link_to(source, &destination_binding, destination)
            .is_ok()
    {
        return check_cancelled(token);
    }
    let mut input = source_binding
        .open_regular(source)
        .map_err(|_| failure("Cannot read cached model."))?;
    let mut output = destination_binding
        .create_regular_new(destination)
        .map_err(|_| failure("Cannot create installed model file."))?;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        check_cancelled(token)?;
        let count = input
            .read(&mut buffer)
            .map_err(|_| failure("Cannot copy cached model."))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|_| failure("Cannot save installed model."))?;
    }
    output
        .sync_all()
        .map_err(|_| failure("Cannot finish installed model."))?;
    check_cancelled(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(bytes: &[u8]) -> ModelFile {
        ModelFile {
            path: "models/中文模型.onnx".into(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            bytes: bytes.len() as u64,
            urls: vec!["https://example.invalid/model.onnx".into()],
        }
    }

    #[test]
    fn catalog_only_accepts_data_paths_and_https_sources() {
        let valid = model(b"model");
        assert!(valid_model_files(std::slice::from_ref(&valid)));
        for path in [
            "models/../program.exe",
            "models/script.py",
            "models/C:/model.bin",
            "models/a\\b.onnx",
            "models//x.onnx",
        ] {
            let mut invalid = valid.clone();
            invalid.path = path.into();
            assert!(!valid_model_files(&[invalid]), "{path}");
        }
        let mut invalid = valid.clone();
        invalid.urls = vec!["http://example.com/model".into()];
        assert!(!valid_model_files(&[invalid]));
        assert!(!valid_model_files(&[valid.clone(), valid]));
    }

    #[tokio::test]
    async fn offline_import_checks_once_and_reuses_data_across_program_versions() {
        let root = tempfile::tempdir().unwrap();
        let offline = tempfile::tempdir().unwrap();
        let file = model(b"valid ONNX fixture data");
        let source = offline.path().join(&file.path);
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"valid ONNX fixture data").unwrap();
        let first = root.path().join("program-v1");
        std::fs::create_dir(&first).unwrap();
        let coordinator = BlockingWorkCoordinator::default();
        materialize_models(
            &coordinator,
            root.path(),
            &first,
            std::slice::from_ref(&file),
            Some(offline.path()),
            &CancellationToken::new(),
            |_, _| {},
        )
        .await
        .unwrap();
        std::fs::remove_file(source).unwrap();
        let second = root.path().join("program-v2");
        std::fs::create_dir(&second).unwrap();
        materialize_models(
            &coordinator,
            root.path(),
            &second,
            std::slice::from_ref(&file),
            Some(offline.path()),
            &CancellationToken::new(),
            |_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(second.join(file.path)).unwrap(),
            b"valid ONNX fixture data"
        );
    }

    #[tokio::test]
    async fn invalid_or_missing_offline_model_never_falls_back_to_network() {
        let root = tempfile::tempdir().unwrap();
        let offline = tempfile::tempdir().unwrap();
        let file = model(b"model");
        let source = offline.path().join(&file.path);
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"wrong").unwrap();
        let pack = root.path().join("pack");
        std::fs::create_dir(&pack).unwrap();
        let coordinator = BlockingWorkCoordinator::default();
        let error = materialize_models(
            &coordinator,
            root.path(),
            &pack,
            std::slice::from_ref(&file),
            Some(offline.path()),
            &CancellationToken::new(),
            |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(error.message.contains("checksum"));
        assert!(!pack.join(&file.path).exists());
        assert_eq!(std::fs::read(&source).unwrap(), b"wrong");
        std::fs::remove_file(source).unwrap();
        let error = materialize_models(
            &coordinator,
            root.path(),
            &pack,
            &[file],
            Some(offline.path()),
            &CancellationToken::new(),
            |_, _| {},
        )
        .await
        .unwrap_err();
        assert!(error.message.contains("missing model"));
        assert_eq!(
            std::fs::read_dir(root.path().join(".models"))
                .unwrap()
                .count(),
            0
        );
    }

    #[tokio::test]
    async fn cancellation_does_not_publish_model() {
        let root = tempfile::tempdir().unwrap();
        let pack = root.path().join("pack");
        std::fs::create_dir(&pack).unwrap();
        let token = CancellationToken::new();
        token.cancel();
        let error = materialize_models(
            &BlockingWorkCoordinator::default(),
            root.path(),
            &pack,
            &[model(b"model")],
            None,
            &token,
            |_, _| {},
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, crate::errors::IMPORT_V2_CANCELLED);
        assert_eq!(
            std::fs::read_dir(root.path().join(".models"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn filesystems_without_hard_links_copy_models() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("cached-model");
        let pack = root.path().join("pack");
        std::fs::create_dir(&pack).unwrap();
        std::fs::write(&source, b"model bytes").unwrap();
        let destination = pack.join("models/nested/model.onnx");
        install_model_file(
            root.path(),
            &pack,
            &source,
            &destination,
            &CancellationToken::new(),
            false,
        )
        .unwrap();
        assert_eq!(std::fs::read(destination).unwrap(), b"model bytes");
    }

    #[test]
    #[cfg(unix)]
    fn model_destination_cannot_create_directories_through_symlink() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let source = root.path().join("cached-model");
        std::fs::write(&source, b"model bytes").unwrap();
        let pack = root.path().join("pack");
        std::fs::create_dir(&pack).unwrap();
        std::os::unix::fs::symlink(outside.path(), pack.join("models")).unwrap();
        assert!(install_model_file(
            root.path(),
            &pack,
            &source,
            &pack.join("models/nested/model.onnx"),
            &CancellationToken::new(),
            false
        )
        .is_err());
        assert!(!outside.path().join("nested").exists());
    }
}
