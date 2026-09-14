use std::env;
use std::path::PathBuf;

#[allow(dead_code)]
#[path = "src/services/import_v2/capability_embed.rs"]
mod capability_embed;
#[allow(dead_code)]
#[path = "src/services/import_v2/product_capability.rs"]
mod product_capability;

const CATALOG_MODE_ENV: &str = "LLM_WIKI_CAPABILITY_CATALOG_MODE";
const STAGING_DIR_ENV: &str = "LLM_WIKI_CAPABILITY_STAGING_DIR";

fn main() {
    println!("cargo:rerun-if-env-changed={CATALOG_MODE_ENV}");
    println!("cargo:rerun-if-env-changed={STAGING_DIR_ENV}");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is provided by Cargo"));
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is provided"));
    let source_root = manifest_dir
        .parent()
        .expect("the manifest directory always has a repository parent")
        .join("capabilities");
    let mode = env::var(CATALOG_MODE_ENV).unwrap_or_default();
    let development_catalog = manifest_dir
        .parent()
        .unwrap()
        .join(".dev-capabilities/catalog");
    let staging = env::var_os(STAGING_DIR_ENV).map(PathBuf::from).or_else(|| {
        (matches!(mode.as_str(), "" | "source" | "development")
            && development_catalog.join("install-catalog.json").is_file())
        .then_some(development_catalog.clone())
    });
    println!("cargo:rerun-if-changed={}", development_catalog.display());
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
        validate_distributable_target(&product_manifest)
            .unwrap_or_else(|error| panic!("distributable capability inputs are invalid: {error}"));
    }
    #[cfg(feature = "gui")]
    tauri_build::build();
}

fn validate_distributable_target(
    product_manifest: &product_capability::ProductCapabilityManifest,
) -> Result<(), String> {
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
    Ok(())
}
