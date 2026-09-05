use std::fs;
use std::path::Path;

use super::product_capability::ProductCapabilityManifest;

const DISTRIBUTABLE_MODE: &str = "distributable";
const DEVELOPMENT_MODE: &str = "development";

pub fn stage_embed_inputs(
    source_root: &Path,
    staging: Option<&Path>,
    out_dir: &Path,
    mode: &str,
) -> Result<(), String> {
    let (catalog_source, keys_source, mode_label) =
        resolve_embed_sources(source_root, staging, mode)?;
    let catalog_text = read_embed_input(&catalog_source)?;
    let keys_text = read_embed_input(&keys_source)?;
    let product_text = read_embed_input(&source_root.join("product-manifest.json"))?;
    let product = ProductCapabilityManifest::parse(&product_text)?;
    let entry_count = product.validate_catalog(&catalog_text, mode_label == DISTRIBUTABLE_MODE)?;
    let key_count = parse_embed_trusted_keys(&keys_text)?;
    if mode_label == DISTRIBUTABLE_MODE && entry_count == 0 {
        return Err("release builds cannot embed an empty capability catalog".into());
    }
    if mode_label == DISTRIBUTABLE_MODE && key_count == 0 {
        return Err("release builds require at least one trusted capability key".into());
    }
    let destination = out_dir.join("capabilities");
    fs::create_dir_all(&destination)
        .map_err(|error| format!("cannot create {}: {error}", destination.display()))?;
    fs::write(destination.join("install-catalog.json"), &catalog_text)
        .map_err(|error| format!("cannot stage the embedded install catalog: {error}"))?;
    fs::write(destination.join("trusted-keys.json"), &keys_text)
        .map_err(|error| format!("cannot stage the embedded trusted keys: {error}"))?;
    fs::write(destination.join("product-manifest.json"), &product_text)
        .map_err(|error| format!("cannot stage the embedded product manifest: {error}"))?;
    fs::write(
        destination.join("embed-record.json"),
        embed_record(mode_label, entry_count, key_count),
    )
    .map_err(|error| format!("cannot write the embed record: {error}"))?;
    Ok(())
}

pub fn resolve_embed_sources(
    source_root: &Path,
    staging: Option<&Path>,
    mode: &str,
) -> Result<(std::path::PathBuf, std::path::PathBuf, &'static str), String> {
    match mode {
        "" | "source" | "development" => Ok((
            source_root.join("install-catalog.json"),
            source_root.join("trusted-keys.json"),
            DEVELOPMENT_MODE,
        )),
        "release" | "distributable" => {
            let staging = staging.ok_or("release mode requires LLM_WIKI_CAPABILITY_STAGING_DIR")?;
            Ok((
                staging.join("install-catalog.json"),
                staging.join("trusted-keys.json"),
                DISTRIBUTABLE_MODE,
            ))
        }
        other => Err(format!(
            "unsupported catalog mode {other:?}; expected development or distributable"
        )),
    }
}

fn read_embed_input(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("cannot read {}: {error}", path.display()))
}

pub fn parse_embed_trusted_keys(text: &str) -> Result<usize, String> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|error| format!("trusted keys are not valid JSON: {error}"))?;
    let keys = value
        .as_object()
        .ok_or("trusted keys must be a JSON object")?;
    for (key_id, key) in keys {
        let valid = key.as_str().is_some_and(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                && hex.bytes().any(|byte| byte != b'0')
        });
        if !valid {
            return Err(format!(
                "trusted key {key_id} must be a non-zero lowercase 32-byte hex key"
            ));
        }
    }
    Ok(keys.len())
}

pub fn embed_record(mode: &str, entry_count: usize, key_count: usize) -> String {
    format!(
        "{}\n",
        serde_json::json!({
            "mode": mode,
            "entryCount": entry_count,
            "trustedKeyCount": key_count,
        })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "llm-wiki-embed-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|value| value.as_nanos())
                    .unwrap_or(0),
            ));
            fs::create_dir_all(root.join("capabilities")).unwrap();
            fs::write(
                root.join("capabilities/product-manifest.json"),
                super::super::product_capability::PRODUCT_MANIFEST_JSON,
            )
            .unwrap();
            Self(root)
        }

        fn source_root(&self) -> PathBuf {
            self.0.join("capabilities")
        }

        fn staging_dir(&self) -> PathBuf {
            let staging = self.0.join("staging");
            fs::create_dir_all(&staging).unwrap();
            staging
        }

        fn out_dir(&self) -> PathBuf {
            self.0.join("out")
        }

        fn write(&self, path: &Path, value: &serde_json::Value) {
            fs::write(path, serde_json::to_string(value).unwrap()).unwrap();
        }

        fn record(&self) -> serde_json::Value {
            serde_json::from_str(
                &fs::read_to_string(self.out_dir().join("capabilities/embed-record.json")).unwrap(),
            )
            .unwrap()
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    fn json_text(value: &serde_json::Value) -> String {
        serde_json::to_string(value).unwrap()
    }

    fn release_catalog() -> serde_json::Value {
        let manifest = ProductCapabilityManifest::embedded().unwrap();
        let entries = manifest
            .published_definitions()
            .flat_map(|definition| {
                definition.supported_targets.iter().map(move |target| {
                    serde_json::json!({
                        "capabilityId": definition.capability_id,
                        "version": "1.0.0",
                        "targetTriple": target,
                        "url": format!(
                            "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v1.0.0/{}-1.0.0-{}.zip",
                            definition.capability_id, target
                        ),
                        "archiveSha256": "a".repeat(64),
                        "manifestSha256": "b".repeat(64),
                        "signingKeyId": "release",
                        "compressedBytes": 1,
                        "installedBytes": 1,
                        "modelBytes": definition.size_sources.model_bytes.as_ref().map(|_| 1),
                        "license": definition.license_policy.expression,
                    })
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({"schemaVersion": 1, "entries": entries})
    }

    #[test]
    fn trusted_keys_must_be_well_formed() {
        assert_eq!(parse_embed_trusted_keys("{}").unwrap(), 0);
        assert_eq!(
            parse_embed_trusted_keys(&json_text(&serde_json::json!({
                "release": "a".repeat(64)
            })))
            .unwrap(),
            1
        );
        assert!(parse_embed_trusted_keys(&json_text(&serde_json::json!({
            "release": "abc"
        })))
        .is_err());
        assert!(parse_embed_trusted_keys(&json_text(&serde_json::json!({
            "release": "0".repeat(64)
        })))
        .is_err());
        assert!(parse_embed_trusted_keys(&json_text(&serde_json::json!({
            "release": "A".repeat(64)
        })))
        .is_err());
        assert!(parse_embed_trusted_keys("[]").is_err());
        assert!(parse_embed_trusted_keys("not json").is_err());
    }

    #[test]
    fn source_mode_copies_the_repository_fallback_and_records_it() {
        let root = TempRoot::new("source");
        let catalog = serde_json::json!({"schemaVersion": 1, "entries": []});
        let keys = serde_json::json!({});
        root.write(&root.source_root().join("install-catalog.json"), &catalog);
        root.write(&root.source_root().join("trusted-keys.json"), &keys);

        stage_embed_inputs(&root.source_root(), None, &root.out_dir(), "source").unwrap();

        let staged_catalog =
            fs::read_to_string(root.out_dir().join("capabilities/install-catalog.json")).unwrap();
        assert_eq!(staged_catalog, json_text(&catalog));
        let record = root.record();
        assert_eq!(record["mode"], "development");
        assert_eq!(record["entryCount"], 0);
    }

    #[test]
    fn release_mode_requires_a_non_empty_staged_catalog() {
        let root = TempRoot::new("release-empty");
        let staging = root.staging_dir();
        root.write(
            &staging.join("install-catalog.json"),
            &serde_json::json!({"schemaVersion": 1, "entries": []}),
        );
        root.write(
            &staging.join("trusted-keys.json"),
            &serde_json::json!({"release": "a".repeat(64)}),
        );

        let error = stage_embed_inputs(
            &root.source_root(),
            Some(&staging),
            &root.out_dir(),
            "release",
        )
        .unwrap_err();
        assert!(error.contains("exact catalog matrix"));
    }

    #[test]
    fn release_mode_requires_at_least_one_trusted_key() {
        let root = TempRoot::new("release-no-keys");
        let staging = root.staging_dir();
        root.write(&staging.join("install-catalog.json"), &release_catalog());
        root.write(&staging.join("trusted-keys.json"), &serde_json::json!({}));

        let error = stage_embed_inputs(
            &root.source_root(),
            Some(&staging),
            &root.out_dir(),
            "release",
        )
        .unwrap_err();
        assert!(error.contains("trusted capability key"));
    }

    #[test]
    fn release_mode_copies_the_staged_catalog_and_trusted_keys() {
        let root = TempRoot::new("release");
        let staging = root.staging_dir();
        root.write(&staging.join("install-catalog.json"), &release_catalog());
        root.write(
            &staging.join("trusted-keys.json"),
            &serde_json::json!({"release": "b".repeat(64)}),
        );

        stage_embed_inputs(
            &root.source_root(),
            Some(&staging),
            &root.out_dir(),
            "release",
        )
        .unwrap();

        let record = root.record();
        assert_eq!(record["mode"], "distributable");
        assert_eq!(
            record["entryCount"].as_u64(),
            Some(
                ProductCapabilityManifest::embedded()
                    .unwrap()
                    .expected_release_entry_count() as u64
            )
        );
        assert_eq!(record["trustedKeyCount"], 1);
    }

    #[test]
    fn release_mode_rejects_a_count_complete_but_unusable_catalog() {
        let root = TempRoot::new("release-unusable");
        let staging = root.staging_dir();
        let mut catalog = release_catalog();
        catalog["entries"][0].as_object_mut().unwrap().remove("url");
        root.write(&staging.join("install-catalog.json"), &catalog);
        root.write(
            &staging.join("trusted-keys.json"),
            &serde_json::json!({"release": "b".repeat(64)}),
        );

        let error = stage_embed_inputs(
            &root.source_root(),
            Some(&staging),
            &root.out_dir(),
            "distributable",
        )
        .unwrap_err();
        assert!(error.contains("missing field `url`"));
    }

    #[test]
    fn unsupported_modes_and_missing_staging_fail_closed() {
        let root = TempRoot::new("modes");
        assert!(stage_embed_inputs(&root.source_root(), None, &root.out_dir(), "dev").is_err());
        assert!(stage_embed_inputs(&root.source_root(), None, &root.out_dir(), "release").is_err());
    }
}
