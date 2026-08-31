use std::env;
use std::fs;
use std::path::PathBuf;

#[allow(dead_code)]
#[path = "src/services/import_v2/capability_embed.rs"]
mod capability_embed;
#[allow(dead_code)]
#[path = "src/services/import_v2/product_capability.rs"]
mod product_capability;

const CATALOG_MODE_ENV: &str = "LLM_WIKI_CAPABILITY_CATALOG_MODE";
const STAGING_DIR_ENV: &str = "LLM_WIKI_CAPABILITY_STAGING_DIR";
const DISTRIBUTION_TAG_ENV: &str = "LLM_WIKI_DISTRIBUTION_TAG";
const DISTRIBUTION_COMMIT_ENV: &str = "LLM_WIKI_DISTRIBUTION_COMMIT";
const DISTRIBUTION_RUN_ID_ENV: &str = "LLM_WIKI_DISTRIBUTION_RUN_ID";

fn main() {
    println!("cargo:rerun-if-env-changed={CATALOG_MODE_ENV}");
    println!("cargo:rerun-if-env-changed={STAGING_DIR_ENV}");
    println!("cargo:rerun-if-env-changed={DISTRIBUTION_TAG_ENV}");
    println!("cargo:rerun-if-env-changed={DISTRIBUTION_COMMIT_ENV}");
    println!("cargo:rerun-if-env-changed={DISTRIBUTION_RUN_ID_ENV}");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is provided by Cargo"));
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is provided"));
    let source_root = manifest_dir
        .parent()
        .expect("the manifest directory always has a repository parent")
        .join("capabilities");
    let mode = env::var(CATALOG_MODE_ENV).unwrap_or_default();
    let staging = env::var_os(STAGING_DIR_ENV).map(PathBuf::from);
    let product_manifest = product_capability::ProductCapabilityManifest::embedded()
        .unwrap_or_else(|error| panic!("product capability manifest is invalid: {error}"));
    capability_embed::stage_embed_inputs(&source_root, staging.as_deref(), &out_dir, &mode)
        .unwrap_or_else(|error| panic!("capability embed inputs are invalid: {error}"));
    let (catalog_source, keys_source, _) =
        capability_embed::resolve_embed_sources(&source_root, staging.as_deref(), &mode)
            .unwrap_or_else(|error| panic!("capability embed inputs are invalid: {error}"));
    println!("cargo:rerun-if-changed={}", catalog_source.display());
    println!("cargo:rerun-if-changed={}", keys_source.display());
    println!(
        "cargo:rerun-if-changed={}",
        source_root.join("product-manifest.json").display()
    );
    if matches!(mode.as_str(), "release" | "distributable") {
        validate_distributable_identity(&product_manifest, staging.as_deref(), &out_dir)
            .unwrap_or_else(|error| panic!("distributable capability inputs are invalid: {error}"));
    }
    #[cfg(feature = "gui")]
    tauri_build::build();
}

fn validate_distributable_identity(
    product_manifest: &product_capability::ProductCapabilityManifest,
    staging: Option<&std::path::Path>,
    out_dir: &std::path::Path,
) -> Result<(), String> {
    let staging = staging.ok_or("distributable mode requires capability staging")?;
    let release_tag = required_env(DISTRIBUTION_TAG_ENV)?;
    let commit = required_env(DISTRIBUTION_COMMIT_ENV)?;
    let run_id = required_env(DISTRIBUTION_RUN_ID_ENV)?;
    let package_version = env::var("CARGO_PKG_VERSION")
        .map_err(|_| "Cargo did not provide CARGO_PKG_VERSION".to_string())?;
    let stable_tag = format!("app-v{package_version}");
    let valid_tag = release_tag == stable_tag
        || release_tag
            .strip_prefix(&(stable_tag.clone() + "-rc."))
            .and_then(|suffix| suffix.parse::<u64>().ok().map(|number| (suffix, number)))
            .is_some_and(|(suffix, number)| number > 0 && suffix == number.to_string());
    if !valid_tag {
        return Err(format!(
            "{DISTRIBUTION_TAG_ENV} must match the Cargo package version"
        ));
    }
    if commit.len() != 40
        || !commit
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "{DISTRIBUTION_COMMIT_ENV} must be a 40-character commit SHA"
        ));
    }
    if run_id.is_empty() || !run_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{DISTRIBUTION_RUN_ID_ENV} must be numeric"));
    }
    let provenance_path = staging.join("catalog-provenance.json");
    let provenance_text = fs::read_to_string(&provenance_path)
        .map_err(|error| format!("cannot read {}: {error}", provenance_path.display()))?;
    let provenance: serde_json::Value = serde_json::from_str(&provenance_text)
        .map_err(|error| format!("catalog provenance is not valid JSON: {error}"))?;
    if provenance
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || provenance
            .get("releaseTag")
            .and_then(serde_json::Value::as_str)
            != Some(release_tag.as_str())
        || provenance
            .get("commitSha")
            .and_then(serde_json::Value::as_str)
            != Some(commit.as_str())
        || provenance
            .get("workflowRunId")
            .and_then(serde_json::Value::as_str)
            != Some(run_id.as_str())
    {
        return Err("catalog provenance does not match the exact distribution identity".into());
    }
    let catalog_text = fs::read_to_string(out_dir.join("capabilities/install-catalog.json"))
        .map_err(|error| format!("cannot read staged install catalog: {error}"))?;
    product_manifest.validate_catalog_for_tag(&catalog_text, true, Some(&release_tag))?;
    let target = env::var("TARGET").map_err(|_| "Cargo did not provide TARGET".to_string())?;
    if !product_manifest
        .supported_targets
        .iter()
        .any(|supported| supported == &target)
    {
        return Err(format!(
            "current target {target} is not in the product capability manifest"
        ));
    }
    let destination = out_dir.join("capabilities/catalog-provenance.json");
    fs::write(&destination, provenance_text)
        .map_err(|error| format!("cannot stage {}: {error}", destination.display()))?;
    println!("cargo:rerun-if-changed={}", provenance_path.display());
    Ok(())
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("{name} is required in distributable mode"))
}
