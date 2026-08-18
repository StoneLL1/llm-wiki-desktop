use std::env;
use std::path::PathBuf;

#[path = "src/services/import_v2/capability_embed.rs"]
mod capability_embed;

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
    let staging = env::var_os(STAGING_DIR_ENV).map(PathBuf::from);
    capability_embed::stage_embed_inputs(&source_root, staging.as_deref(), &out_dir, &mode)
        .unwrap_or_else(|error| panic!("capability embed inputs are invalid: {error}"));
    let (catalog_source, keys_source, _) =
        capability_embed::resolve_embed_sources(&source_root, staging.as_deref(), &mode)
            .unwrap_or_else(|error| panic!("capability embed inputs are invalid: {error}"));
    println!("cargo:rerun-if-changed={}", catalog_source.display());
    println!("cargo:rerun-if-changed={}", keys_source.display());
    #[cfg(feature = "gui")]
    tauri_build::build();
}
