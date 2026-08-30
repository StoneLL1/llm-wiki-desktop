use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use llm_wiki_desktop_lib::services::import_v2::capability_installer::CapabilityCatalogEntry;
use llm_wiki_desktop_lib::services::import_v2::capability_pack::{
    CapabilityPackFile, CapabilityPackManifest,
};
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const PRIVATE_KEY_ENV: &str = "LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX";
const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_INSTALLED_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 20_000;
const SUPPORTED_TARGETS: &[&str] = &[
    "x86_64-pc-windows-msvc",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseTemplate {
    pack_id: String,
    version: String,
    protocol_version: String,
    license_expression: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogFragment {
    schema_version: u32,
    entries: Vec<CapabilityCatalogEntry>,
}

struct AssembleOptions {
    template: PathBuf,
    payload: PathBuf,
    target: String,
    entrypoint: String,
    entrypoint_args: Vec<String>,
    output: PathBuf,
    base_url: String,
    trusted_keys: PathBuf,
    key_id: String,
    private_key_pkcs8: Vec<u8>,
    model_bytes: Option<u64>,
}

#[derive(Debug)]
struct AssembleResult {
    archive_path: PathBuf,
    fragment_path: PathBuf,
    entry: CapabilityCatalogEntry,
}

fn main() {
    if let Err(message) = run(env::args().skip(1).collect()) {
        eprintln!("capability-release: {message}");
        std::process::exit(1);
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    let (command, tail) = arguments
        .split_first()
        .ok_or_else(|| usage("missing command"))?;
    match command.as_str() {
        "assemble" => {
            let options = parse_assemble(tail)?;
            let result = assemble(&options)?;
            println!("archive={}", result.archive_path.display());
            println!("catalog={}", result.fragment_path.display());
            println!("sha256={}", result.entry.archive_sha256);
            Ok(())
        }
        "merge-catalog" => {
            let values = parse_options(tail, &[])?;
            reject_unknown_options(
                &values,
                &["input", "output", "trusted-keys", "expected-tag"],
            )?;
            let input = required_path(&values, "input")?;
            let output = required_path(&values, "output")?;
            let trusted_keys = required_path(&values, "trusted-keys")?;
            let expected_tag = required(&values, "expected-tag")?;
            merge_catalog(&input, &output, &trusted_keys, &expected_tag)?;
            println!("catalog={}", output.display());
            Ok(())
        }
        "public-key" => {
            let private_key = env::var(PRIVATE_KEY_ENV).map_err(|_| {
                format!("{PRIVATE_KEY_ENV} is required and must contain hex PKCS#8 bytes")
            })?;
            let key_pair = Ed25519KeyPair::from_pkcs8(&decode_hex(&private_key)?)
                .map_err(|_| "the signing key is not valid Ed25519 PKCS#8".to_string())?;
            println!("{}", encode_hex(key_pair.public_key().as_ref()));
            Ok(())
        }
        _ => Err(usage("unknown command")),
    }
}

fn usage(reason: &str) -> String {
    format!(
        "{reason}. Use `assemble --template FILE --payload DIR --target TRIPLE --entrypoint PATH \
         [--entrypoint-arg VALUE] --output DIR --base-url HTTPS_URL --trusted-keys FILE \
         --key-id ID [--model-bytes N]` or `merge-catalog --input DIR --output FILE`. \
         The signing key is read only from {PRIVATE_KEY_ENV}. Use `public-key` to derive only the \
         raw public key hex for trusted-keys.json."
    )
}

fn parse_assemble(arguments: &[String]) -> Result<AssembleOptions, String> {
    let values = parse_options(arguments, &["entrypoint-arg"])?;
    reject_unknown_options(
        &values,
        &[
            "template",
            "payload",
            "target",
            "entrypoint",
            "entrypoint-arg",
            "output",
            "base-url",
            "trusted-keys",
            "key-id",
            "model-bytes",
        ],
    )?;
    let private_key = env::var(PRIVATE_KEY_ENV)
        .map_err(|_| format!("{PRIVATE_KEY_ENV} is required and must contain hex PKCS#8 bytes"))?;
    Ok(AssembleOptions {
        template: required_path(&values, "template")?,
        payload: required_path(&values, "payload")?,
        target: required(&values, "target")?,
        entrypoint: required(&values, "entrypoint")?,
        entrypoint_args: values.get("entrypoint-arg").cloned().unwrap_or_default(),
        output: required_path(&values, "output")?,
        base_url: required(&values, "base-url")?,
        trusted_keys: required_path(&values, "trusted-keys")?,
        key_id: required(&values, "key-id")?,
        private_key_pkcs8: decode_hex(&private_key)?,
        model_bytes: values
            .get("model-bytes")
            .and_then(|items| items.last())
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| "--model-bytes must be an unsigned integer".to_string())
            })
            .transpose()?,
    })
}

fn parse_options(
    arguments: &[String],
    repeatable: &[&str],
) -> Result<BTreeMap<String, Vec<String>>, String> {
    if arguments.len() % 2 != 0 {
        return Err(usage("every option requires one value"));
    }
    let mut output: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pair in arguments.chunks_exact(2) {
        let key = pair[0]
            .strip_prefix("--")
            .ok_or_else(|| usage("options must start with --"))?
            .to_string();
        if output.contains_key(&key) && !repeatable.contains(&key.as_str()) {
            return Err(format!("--{key} may only be provided once"));
        }
        output.entry(key).or_default().push(pair[1].clone());
    }
    Ok(output)
}

fn required(values: &BTreeMap<String, Vec<String>>, name: &str) -> Result<String, String> {
    values
        .get(name)
        .and_then(|items| items.last())
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| usage(&format!("--{name} is required")))
}

fn reject_unknown_options(
    values: &BTreeMap<String, Vec<String>>,
    allowed: &[&str],
) -> Result<(), String> {
    if let Some(name) = values.keys().find(|name| !allowed.contains(&name.as_str())) {
        return Err(format!("unknown option --{name}"));
    }
    Ok(())
}

fn required_path(values: &BTreeMap<String, Vec<String>>, name: &str) -> Result<PathBuf, String> {
    required(values, name).map(PathBuf::from)
}

fn assemble(options: &AssembleOptions) -> Result<AssembleResult, String> {
    let template: ReleaseTemplate = serde_json::from_slice(
        &fs::read(&options.template)
            .map_err(|error| format!("cannot read {}: {error}", options.template.display()))?,
    )
    .map_err(|error| format!("invalid release template: {error}"))?;
    validate_template(&template, options)?;

    let key_pair = Ed25519KeyPair::from_pkcs8(&options.private_key_pkcs8)
        .map_err(|_| "the signing key is not valid Ed25519 PKCS#8".to_string())?;
    verify_trusted_key(
        &options.trusted_keys,
        &options.key_id,
        key_pair.public_key().as_ref(),
    )?;

    let files = collect_payload_files(&options.payload)?;
    if !files.iter().any(|file| file.path == options.entrypoint) {
        return Err("the declared entrypoint is not present in the payload".into());
    }
    let runtime_bytes = files.iter().try_fold(0_u64, |total, file| {
        total
            .checked_add(file.bytes)
            .ok_or_else(|| "payload size overflowed the release limit".to_string())
    })?;
    if runtime_bytes > MAX_INSTALLED_BYTES {
        return Err("payload exceeds the installed release size limit".into());
    }
    let mut executable_files = collect_executable_files(&options.payload, &files)?;
    if !executable_files
        .iter()
        .any(|path| path == &options.entrypoint)
    {
        executable_files.push(options.entrypoint.clone());
        executable_files.sort();
    }

    let mut manifest = CapabilityPackManifest {
        schema_version: 2,
        pack_id: template.pack_id.clone(),
        version: template.version.clone(),
        protocol_version: template.protocol_version.clone(),
        target_triples: vec![options.target.clone()],
        archive_sha256: String::new(),
        license_expression: template.license_expression.clone(),
        entrypoint: options.entrypoint.clone(),
        entrypoint_args: options.entrypoint_args.clone(),
        executable_files,
        compressed_bytes: 0,
        installed_bytes: 0,
        signing_key_id: options.key_id.clone(),
        signature: String::new(),
        files,
    };
    manifest.signature = encode_hex(
        key_pair
            .sign(&manifest.signing_payload().map_err(|error| error.message)?)
            .as_ref(),
    );
    let mut manifest_bytes =
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    manifest_bytes.push(b'\n');
    let installed_bytes = runtime_bytes
        .checked_add(manifest_bytes.len() as u64)
        .ok_or_else(|| "installed size overflowed the release limit".to_string())?;
    if installed_bytes > MAX_INSTALLED_BYTES {
        return Err("payload exceeds the installed release size limit".into());
    }
    let base_url = normalized_base_url(&options.base_url)?;

    fs::create_dir_all(&options.output)
        .map_err(|error| format!("cannot create output directory: {error}"))?;
    let file_name = format!(
        "{}-{}-{}.zip",
        template.pack_id, template.version, options.target
    );
    let archive_path = options.output.join(&file_name);
    write_archive(&archive_path, &options.payload, &manifest, &manifest_bytes)?;
    let compressed_bytes = fs::metadata(&archive_path)
        .map_err(|error| format!("cannot inspect release archive: {error}"))?
        .len();
    if compressed_bytes > MAX_ARCHIVE_BYTES {
        fs::remove_file(&archive_path).ok();
        return Err("release archive exceeds the compressed size limit".into());
    }
    let entry = CapabilityCatalogEntry {
        capability_id: template.pack_id,
        version: template.version,
        target_triple: options.target.clone(),
        url: format!("{base_url}/{file_name}"),
        archive_sha256: sha256_file(&archive_path)?,
        manifest_sha256: encode_hex(&Sha256::digest(&manifest_bytes)),
        signing_key_id: options.key_id.clone(),
        compressed_bytes,
        installed_bytes,
        model_bytes: options.model_bytes,
        license: template.license_expression,
    };
    verify_archive(&archive_path, &manifest, &manifest_bytes, &entry)?;

    let fragment_path = options.output.join(format!(
        "{}-{}-{}.catalog.json",
        entry.capability_id, entry.version, entry.target_triple
    ));
    write_json(
        &fragment_path,
        &CatalogFragment {
            schema_version: 1,
            entries: vec![entry.clone()],
        },
    )?;
    Ok(AssembleResult {
        archive_path,
        fragment_path,
        entry,
    })
}

fn validate_template(template: &ReleaseTemplate, options: &AssembleOptions) -> Result<(), String> {
    if template.pack_id.is_empty()
        || !template
            .pack_id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
    {
        return Err("template packId is invalid".into());
    }
    Version::parse(&template.version).map_err(|_| "template version is not semver".to_string())?;
    if template.protocol_version != "2" {
        return Err("template protocolVersion must be 2".into());
    }
    if template.license_expression.trim().is_empty() {
        return Err("template licenseExpression is empty".into());
    }
    if !SUPPORTED_TARGETS.contains(&options.target.as_str()) {
        return Err("target is not one of the four supported desktop triples".into());
    }
    validate_relative(&options.entrypoint)?;
    if options.entrypoint_args.len() > 32
        || options
            .entrypoint_args
            .iter()
            .any(|value| value.len() > 4_096 || value.contains('\0'))
    {
        return Err("entrypoint arguments exceed protocol limits".into());
    }
    if options.key_id.trim().is_empty() || options.key_id == "release-build-placeholder" {
        return Err("a non-placeholder --key-id is required".into());
    }
    Ok(())
}

fn verify_trusted_key(path: &Path, key_id: &str, public_key: &[u8]) -> Result<(), String> {
    let trusted: HashMap<String, String> = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?,
    )
    .map_err(|error| format!("invalid trusted key file: {error}"))?;
    let expected = trusted
        .get(key_id)
        .ok_or_else(|| "the signing key ID is not present in trusted-keys.json".to_string())?;
    if decode_hex(expected)? != public_key {
        return Err("the signing private key does not match the trusted public key".into());
    }
    Ok(())
}

fn collect_payload_files(root: &Path) -> Result<Vec<CapabilityPackFile>, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("payload directory is unavailable: {error}"))?;
    if !root.is_dir() {
        return Err("payload must be a directory".into());
    }
    let mut paths = Vec::new();
    collect_paths(&root, &root, &mut paths)?;
    paths.sort_by(|left, right| left.0.cmp(&right.0));
    if paths.is_empty() {
        return Err("payload contains no files".into());
    }
    if paths.len() + 1 > MAX_ARCHIVE_FILES {
        return Err("payload exceeds the release file-count limit".into());
    }
    paths
        .into_iter()
        .map(|(relative, path)| {
            let bytes = fs::read(&path)
                .map_err(|error| format!("cannot read payload file {relative}: {error}"))?;
            Ok(CapabilityPackFile {
                path: relative,
                sha256: encode_hex(&Sha256::digest(&bytes)),
                bytes: bytes.len() as u64,
            })
        })
        .collect()
}

#[cfg(unix)]
fn collect_executable_files(
    root: &Path,
    files: &[CapabilityPackFile],
) -> Result<Vec<String>, String> {
    use std::os::unix::fs::PermissionsExt;
    let mut output = Vec::new();
    for file in files {
        let metadata = fs::metadata(root.join(Path::new(&file.path)))
            .map_err(|error| format!("cannot inspect executable {}: {error}", file.path))?;
        if metadata.permissions().mode() & 0o111 != 0 {
            output.push(file.path.clone());
        }
    }
    Ok(output)
}

#[cfg(not(unix))]
fn collect_executable_files(_: &Path, _: &[CapabilityPackFile]) -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

fn collect_paths(
    root: &Path,
    directory: &Path,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot enumerate payload: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot enumerate payload: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cannot inspect payload file: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("payload symbolic links are not allowed".into());
        }
        if metadata.is_dir() {
            collect_paths(root, &path, output)?;
        } else if metadata.is_file() {
            let relative = portable_relative(
                path.strip_prefix(root)
                    .map_err(|_| "payload file escaped its root".to_string())?,
            )?;
            if matches!(relative.as_str(), "manifest.json" | "pack.archive") {
                return Err(format!("payload may not contain reserved file {relative}"));
            }
            output.push((relative, path));
        } else {
            return Err("payload special files are not allowed".into());
        }
    }
    Ok(())
}

fn portable_relative(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            return Err("payload path is not relative".into());
        };
        parts.push(
            value
                .to_str()
                .ok_or_else(|| "payload path is not valid UTF-8".to_string())?,
        );
    }
    if parts.is_empty() {
        return Err("payload path is empty".into());
    }
    Ok(parts.join("/"))
}

fn validate_relative(value: &str) -> Result<(), String> {
    if value.contains('\\') {
        return Err("entrypoint must use portable forward slashes".into());
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("entrypoint must be a contained relative path".into());
    }
    Ok(())
}

fn write_archive(
    destination: &Path,
    payload_root: &Path,
    manifest: &CapabilityPackManifest,
    manifest_bytes: &[u8],
) -> Result<(), String> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| format!("cannot create release archive: {error}"))?;
    let mut writer = ZipWriter::new(file);
    let regular = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    writer
        .start_file("manifest.json", regular)
        .map_err(|error| format!("cannot add manifest to archive: {error}"))?;
    writer
        .write_all(manifest_bytes)
        .map_err(|error| format!("cannot write manifest to archive: {error}"))?;
    for item in &manifest.files {
        let permissions = if item.path == manifest.entrypoint {
            0o755
        } else {
            0o644
        };
        writer
            .start_file(
                &item.path,
                SimpleFileOptions::default()
                    .compression_method(CompressionMethod::Deflated)
                    .unix_permissions(permissions),
            )
            .map_err(|error| format!("cannot add {} to archive: {error}", item.path))?;
        let mut source = File::open(payload_root.join(Path::new(&item.path)))
            .map_err(|error| format!("cannot open payload file {}: {error}", item.path))?;
        std::io::copy(&mut source, &mut writer)
            .map_err(|error| format!("cannot archive payload file {}: {error}", item.path))?;
    }
    writer
        .finish()
        .map_err(|error| format!("cannot finalize release archive: {error}"))?;
    Ok(())
}

fn verify_archive(
    path: &Path,
    manifest: &CapabilityPackManifest,
    manifest_bytes: &[u8],
    entry: &CapabilityCatalogEntry,
) -> Result<(), String> {
    if manifest.signing_key_id != entry.signing_key_id {
        return Err("catalog signing key differs from the signed manifest".into());
    }
    let file = File::open(path).map_err(|error| format!("cannot verify archive: {error}"))?;
    let mut archive = ZipArchive::new(file).map_err(|error| format!("invalid archive: {error}"))?;
    if archive.len() != manifest.files.len() + 1 {
        return Err("archive file count differs from the signed inventory".into());
    }
    let mut seen = HashSet::new();
    let mut installed_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| format!("cannot inspect archive: {error}"))?;
        let name = file.name().to_string();
        if !seen.insert(name.clone()) {
            return Err("archive contains duplicate paths".into());
        }
        let member_bytes = file.size();
        installed_bytes = installed_bytes
            .checked_add(member_bytes)
            .ok_or_else(|| "archive installed size overflowed".to_string())?;
        if name == "manifest.json" {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|error| format!("cannot verify archive manifest: {error}"))?;
            if bytes != manifest_bytes {
                return Err("archive manifest changed after signing".into());
            }
            if !encode_hex(&Sha256::digest(&bytes)).eq_ignore_ascii_case(&entry.manifest_sha256) {
                return Err("archive manifest digest differs from the catalog entry".into());
            }
        } else {
            let expected = manifest
                .files
                .iter()
                .find(|item| item.path == name)
                .ok_or_else(|| {
                    "archive contains a file outside the signed inventory".to_string()
                })?;
            let mut hasher = Sha256::new();
            let mut buffer = [0_u8; 1024 * 1024];
            loop {
                let read = file
                    .read(&mut buffer)
                    .map_err(|error| format!("cannot verify archive member: {error}"))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            if member_bytes != expected.bytes || encode_hex(&hasher.finalize()) != expected.sha256 {
                return Err(format!("archive member {name} failed its signed digest"));
            }
        }
    }
    if installed_bytes != entry.installed_bytes {
        return Err("archive installed size differs from the catalog entry".into());
    }
    Ok(())
}

fn merge_catalog(
    input: &Path,
    output: &Path,
    trusted_keys_path: &Path,
    expected_tag: &str,
) -> Result<(), String> {
    let mut fragments = fs::read_dir(input)
        .map_err(|error| format!("cannot enumerate catalog fragments: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot enumerate catalog fragments: {error}"))?;
    fragments.sort_by_key(|entry| entry.file_name());
    let mut entries = Vec::new();
    for fragment in fragments {
        let path = fragment.path();
        if !path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".catalog.json"))
        {
            continue;
        }
        let value: CatalogFragment = serde_json::from_slice(
            &fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?,
        )
        .map_err(|error| format!("invalid catalog fragment {}: {error}", path.display()))?;
        if value.schema_version != 1 || value.entries.len() != 1 {
            return Err(format!(
                "catalog fragment {} must contain exactly one schema v1 entry",
                path.display()
            ));
        }
        entries.extend(value.entries);
    }
    if entries.is_empty() {
        return Err("no catalog fragments were found".into());
    }
    entries.sort_by(|left, right| {
        (&left.capability_id, &left.target_triple, &left.version).cmp(&(
            &right.capability_id,
            &right.target_triple,
            &right.version,
        ))
    });
    let mut identities = HashSet::new();
    for entry in &entries {
        validate_catalog_entry(entry)?;
        let identity = (
            entry.capability_id.clone(),
            entry.target_triple.clone(),
            entry.version.clone(),
        );
        if !identities.insert(identity) {
            return Err("catalog contains a duplicate capability target version".into());
        }
    }
    validate_expected_tag(expected_tag)?;
    let trusted: HashMap<String, String> = serde_json::from_slice(
        &fs::read(trusted_keys_path)
            .map_err(|error| format!("cannot read {}: {error}", trusted_keys_path.display()))?,
    )
    .map_err(|error| format!("invalid trusted key file: {error}"))?;
    let mut keys = HashMap::new();
    for (key_id, value) in trusted {
        keys.insert(
            key_id.clone(),
            decode_hex(&value)
                .map_err(|error| format!("trusted key {key_id} is invalid: {error}"))?,
        );
    }
    for entry in &entries {
        verify_release_entry(input, entry, &keys, expected_tag)?;
    }
    write_json(
        output,
        &CatalogFragment {
            schema_version: 1,
            entries,
        },
    )
}

fn validate_expected_tag(tag: &str) -> Result<(), String> {
    let version = tag.strip_prefix("app-v");
    let valid = version.is_some_and(|version| {
        let (core, prerelease) = version.split_once('-').unwrap_or((version, ""));
        let core_valid = core.split('.').count() == 3
            && core.split('.').all(|component| {
                !component.is_empty()
                    && component.bytes().all(|value| value.is_ascii_digit())
                    && (component == "0" || !component.starts_with('0'))
            });
        let prerelease_valid = prerelease.is_empty()
            || prerelease.strip_prefix("rc.").is_some_and(|number| {
                !number.is_empty()
                    && number.bytes().all(|value| value.is_ascii_digit())
                    && !number.starts_with('0')
            });
        core_valid && prerelease_valid
    });
    if valid {
        Ok(())
    } else {
        Err("expected tag must match the frozen app-v grammar".into())
    }
}

fn verify_release_entry(
    input: &Path,
    entry: &CapabilityCatalogEntry,
    keys: &HashMap<String, Vec<u8>>,
    expected_tag: &str,
) -> Result<(), String> {
    let file_name = format!(
        "{}-{}-{}.zip",
        entry.capability_id, entry.version, entry.target_triple
    );
    let archive_path = input.join(&file_name);
    if !archive_path.is_file() {
        return Err(format!(
            "catalog entry {file_name} is missing its release archive"
        ));
    }
    let expected_url = format!(
        "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/{expected_tag}/{file_name}"
    );
    if entry.url != expected_url {
        return Err(format!(
            "catalog entry {file_name} must use the exact immutable url {expected_url}"
        ));
    }
    let archive_sha256 = sha256_file(&archive_path)?;
    if !archive_sha256.eq_ignore_ascii_case(&entry.archive_sha256) {
        return Err(format!(
            "archive {file_name} digest differs from the catalog entry"
        ));
    }
    let compressed_bytes = fs::metadata(&archive_path)
        .map_err(|error| format!("cannot inspect archive {file_name}: {error}"))?
        .len();
    if compressed_bytes != entry.compressed_bytes {
        return Err(format!(
            "archive {file_name} compressed size differs from the catalog entry"
        ));
    }
    let file =
        File::open(&archive_path).map_err(|error| format!("cannot open {file_name}: {error}"))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("archive {file_name} is invalid: {error}"))?;
    let mut manifest_bytes = Vec::new();
    archive
        .by_name("manifest.json")
        .map_err(|error| format!("archive {file_name} has no manifest: {error}"))?
        .read_to_end(&mut manifest_bytes)
        .map_err(|error| format!("cannot read manifest of {file_name}: {error}"))?;
    if !encode_hex(&Sha256::digest(&manifest_bytes)).eq_ignore_ascii_case(&entry.manifest_sha256) {
        return Err(format!(
            "archive {file_name} manifest digest differs from the catalog entry"
        ));
    }
    let manifest: CapabilityPackManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("manifest of {file_name} is invalid: {error}"))?;
    if manifest.schema_version != 2 {
        return Err(format!("manifest of {file_name} must use schema v2"));
    }
    if manifest.pack_id != entry.capability_id
        || manifest.version != entry.version
        || manifest.signing_key_id != entry.signing_key_id
        || manifest.license_expression != entry.license
        || !manifest
            .target_triples
            .iter()
            .any(|target| target == &entry.target_triple)
    {
        return Err(format!(
            "manifest of {file_name} does not match the catalog entry identity"
        ));
    }
    if !manifest.archive_sha256.is_empty()
        || manifest.compressed_bytes != 0
        || manifest.installed_bytes != 0
    {
        return Err(format!(
            "manifest of {file_name} must not carry self-referential archive measurements"
        ));
    }
    let key = keys.get(&manifest.signing_key_id).ok_or_else(|| {
        format!(
            "manifest signing key {} of {file_name} is not a trusted application key",
            manifest.signing_key_id
        )
    })?;
    let signature = decode_hex(&manifest.signature)
        .map_err(|error| format!("signature of {file_name} is invalid: {error}"))?;
    let payload = manifest.signing_payload().map_err(|error| {
        format!(
            "cannot rebuild signing payload of {file_name}: {}",
            error.message
        )
    })?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(&payload, &signature)
        .map_err(|_| format!("manifest signature of {file_name} failed verification"))?;
    verify_archive(&archive_path, &manifest, &manifest_bytes, entry)?;
    Ok(())
}

fn validate_catalog_entry(entry: &CapabilityCatalogEntry) -> Result<(), String> {
    let parsed_url =
        url::Url::parse(&entry.url).map_err(|_| "catalog entry URL is invalid".to_string())?;
    let valid = !entry.capability_id.is_empty()
        && entry
            .capability_id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
        && Version::parse(&entry.version).is_ok()
        && SUPPORTED_TARGETS.contains(&entry.target_triple.as_str())
        && parsed_url.scheme() == "https"
        && parsed_url.host_str().is_some()
        && parsed_url.username().is_empty()
        && parsed_url.password().is_none()
        && parsed_url.query().is_none()
        && parsed_url.fragment().is_none()
        && entry.archive_sha256.len() == 64
        && entry
            .archive_sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
        && !entry.archive_sha256.bytes().all(|value| value == b'0')
        && entry.manifest_sha256.len() == 64
        && entry
            .manifest_sha256
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
        && !entry.manifest_sha256.bytes().all(|value| value == b'0')
        && !entry.signing_key_id.trim().is_empty()
        && (1..=MAX_ARCHIVE_BYTES).contains(&entry.compressed_bytes)
        && (1..=MAX_INSTALLED_BYTES).contains(&entry.installed_bytes)
        && entry
            .model_bytes
            .is_none_or(|value| value > 0 && value <= entry.installed_bytes)
        && !entry.license.trim().is_empty();
    valid
        .then_some(())
        .ok_or_else(|| "catalog entry failed release validation".to_string())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    file.write_all(&bytes)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn normalized_base_url(value: &str) -> Result<String, String> {
    let parsed = url::Url::parse(value.trim())
        .map_err(|_| "--base-url must be an absolute HTTPS URL".to_string())?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(
            "--base-url must be a public HTTPS URL without credentials, query, or fragment".into(),
        );
    }
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("cannot hash {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("cannot hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(encode_hex(&hasher.finalize()))
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 || value.is_empty() {
        return Err("hex value has an invalid length".into());
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| "hex value contains invalid characters".to_string())
        })
        .collect()
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;

    struct Fixture {
        root: PathBuf,
        template: PathBuf,
        payload: PathBuf,
        trusted: PathBuf,
        output: PathBuf,
        key: Vec<u8>,
    }

    impl Fixture {
        fn new() -> Self {
            let root = env::temp_dir().join(format!(
                "llm-wiki-capability-release-{}",
                uuid::Uuid::new_v4()
            ));
            let payload = root.join("payload");
            let output = root.join("output");
            fs::create_dir_all(payload.join("bin")).unwrap();
            fs::write(payload.join("bin/runner"), b"runner").unwrap();
            let template = root.join("manifest.json");
            fs::write(
                &template,
                br#"{"packId":"fixture","version":"1.2.3","protocolVersion":"2","licenseExpression":"MIT"}"#,
            )
            .unwrap();
            let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
            let pair = Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap();
            let trusted = root.join("trusted-keys.json");
            fs::write(
                &trusted,
                serde_json::to_vec(&serde_json::json!({
                    "release-test": encode_hex(pair.public_key().as_ref())
                }))
                .unwrap(),
            )
            .unwrap();
            Self {
                root,
                template,
                payload,
                trusted,
                output,
                key: document.as_ref().to_vec(),
            }
        }

        fn options(&self) -> AssembleOptions {
            AssembleOptions {
                template: self.template.clone(),
                payload: self.payload.clone(),
                target: "x86_64-pc-windows-msvc".into(),
                entrypoint: "bin/runner".into(),
                entrypoint_args: vec!["runner/index.mjs".into()],
                output: self.output.clone(),
                base_url:
                    "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v1.2.3"
                        .into(),
                trusted_keys: self.trusted.clone(),
                key_id: "release-test".into(),
                private_key_pkcs8: self.key.clone(),
                model_bytes: Some(123),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).ok();
        }
    }

    #[test]
    fn assembles_schema_v2_without_self_referential_archive_measurements() {
        let fixture = Fixture::new();
        let result = assemble(&fixture.options()).unwrap();
        assert!(result.archive_path.is_file());
        assert_eq!(result.entry.model_bytes, Some(123));
        assert_ne!(result.entry.archive_sha256, "0".repeat(64));
        assert_ne!(result.entry.manifest_sha256, "0".repeat(64));

        let mut archive = ZipArchive::new(File::open(result.archive_path).unwrap()).unwrap();
        let manifest: CapabilityPackManifest =
            serde_json::from_reader(archive.by_name("manifest.json").unwrap()).unwrap();
        assert_eq!(manifest.schema_version, 2);
        assert!(manifest.archive_sha256.is_empty());
        assert_eq!(manifest.compressed_bytes, 0);
        assert_eq!(manifest.installed_bytes, 0);
        assert_eq!(manifest.entrypoint_args, ["runner/index.mjs"]);
        assert_eq!(manifest.files.len(), 1);
    }

    #[test]
    fn assembled_archive_resolves_after_extraction_with_the_release_public_key() {
        let fixture = Fixture::new();
        let result = assemble(&fixture.options()).unwrap();
        let install_root = fixture.root.join("installed");
        let version_root = install_root.join("fixture/1.2.3");
        fs::create_dir_all(&version_root).unwrap();
        ZipArchive::new(File::open(result.archive_path).unwrap())
            .unwrap()
            .extract(&version_root)
            .unwrap();
        let trusted: HashMap<String, String> =
            serde_json::from_slice(&fs::read(&fixture.trusted).unwrap()).unwrap();
        let manager =
            llm_wiki_desktop_lib::services::import_v2::capability_pack::CapabilityPackManager::new(
                install_root,
                HashMap::from([(
                    "release-test".to_string(),
                    decode_hex(&trusted["release-test"]).unwrap(),
                )]),
            );
        let requirement = llm_wiki_desktop_lib::models::import_v2_file::CapabilityRequirement {
            capability_id: "fixture".into(),
            minimum_version: Some("1.2.3".into()),
            protocol_version: "2".into(),
            target_triple: "x86_64-pc-windows-msvc".into(),
            accepted_license_expressions: vec!["MIT".into()],
        };
        let pack = manager.resolve_version(&requirement, "1.2.3").unwrap();
        assert_eq!(pack.manifest.entrypoint_args, ["runner/index.mjs"]);
    }

    #[test]
    fn refuses_a_signing_key_that_is_not_in_the_application_trust_store() {
        let fixture = Fixture::new();
        fs::write(&fixture.trusted, b"{}").unwrap();
        assert!(assemble(&fixture.options())
            .unwrap_err()
            .contains("not present"));
    }

    #[test]
    fn merge_catalog_is_deterministic_and_rejects_duplicates() {
        let fixture = Fixture::new();
        let result = assemble(&fixture.options()).unwrap();
        let merged = fixture.root.join("catalog.json");
        merge_catalog(&fixture.output, &merged, &fixture.trusted, "app-v1.2.3").unwrap();
        let catalog: CatalogFragment = serde_json::from_slice(&fs::read(merged).unwrap()).unwrap();
        assert_eq!(catalog.entries, vec![result.entry]);

        let duplicate = fixture.output.join("duplicate.catalog.json");
        fs::copy(result.fragment_path, duplicate).unwrap();
        assert!(merge_catalog(
            &fixture.output,
            &fixture.root.join("invalid.json"),
            &fixture.trusted,
            "app-v1.2.3"
        )
        .unwrap_err()
        .contains("duplicate"));
    }

    #[test]
    fn merge_catalog_rejects_tampered_archives_and_manifest_digests() {
        let fixture = Fixture::new();
        let result = assemble(&fixture.options()).unwrap();

        let mut tampered = Vec::new();
        fs::File::open(&result.archive_path)
            .unwrap()
            .read_to_end(&mut tampered)
            .unwrap();
        tampered.push(0);
        fs::write(&result.archive_path, &tampered).unwrap();
        assert!(merge_catalog(
            &fixture.output,
            &fixture.root.join("catalog.json"),
            &fixture.trusted,
            "app-v1.2.3"
        )
        .unwrap_err()
        .contains("digest differs"));

        let restored = &tampered[..tampered.len() - 1];
        fs::write(&result.archive_path, restored).unwrap();
        let mut fragment: serde_json::Value =
            serde_json::from_slice(&fs::read(&result.fragment_path).unwrap()).unwrap();
        fragment["entries"][0]["manifestSha256"] = serde_json::json!("f".repeat(64));
        fs::write(
            &result.fragment_path,
            serde_json::to_vec(&fragment).unwrap(),
        )
        .unwrap();
        assert!(merge_catalog(
            &fixture.output,
            &fixture.root.join("catalog.json"),
            &fixture.trusted,
            "app-v1.2.3"
        )
        .unwrap_err()
        .contains("manifest digest differs"));
    }

    #[test]
    fn merge_catalog_rejects_untrusted_keys_wrong_tags_and_missing_archives() {
        let fixture = Fixture::new();
        let result = assemble(&fixture.options()).unwrap();
        let untrusted = fixture.root.join("untrusted-keys.json");
        fs::write(&untrusted, b"{}").unwrap();
        assert!(merge_catalog(
            &fixture.output,
            &fixture.root.join("catalog.json"),
            &untrusted,
            "app-v1.2.3"
        )
        .unwrap_err()
        .contains("not a trusted application key"));

        assert!(merge_catalog(
            &fixture.output,
            &fixture.root.join("catalog.json"),
            &fixture.trusted,
            "app-v9.9.9"
        )
        .unwrap_err()
        .contains("exact immutable url"));

        fs::remove_file(&result.archive_path).unwrap();
        assert!(merge_catalog(
            &fixture.output,
            &fixture.root.join("catalog.json"),
            &fixture.trusted,
            "app-v1.2.3"
        )
        .unwrap_err()
        .contains("missing its release archive"));
    }

    #[test]
    fn expected_tag_uses_the_same_frozen_release_grammar_as_catalog_urls() {
        for tag in ["app-v0.1.0", "app-v1.2.3", "app-v1.2.3-rc.1"] {
            validate_expected_tag(tag).unwrap();
        }
        for tag in [
            "v1.2.3",
            "app-v1.2",
            "app-v01.2.3",
            "app-v1.2.3-rc.01",
            "app-v1.2.3-preview.1",
            "app-v1.2.3_unsafe",
        ] {
            assert!(
                validate_expected_tag(tag).is_err(),
                "expected {tag} to be rejected"
            );
        }
    }
}
