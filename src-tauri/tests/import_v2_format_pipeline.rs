use llm_wiki_desktop_lib::{
    models::{
        import_v2::{
            CommitImportSessionRequest, CommitItemDecision, ImportAsrProfile, ImportItemStatus,
            ImportMediaAuthorizationKind, ImportResourceMode,
        },
        import_v2_file::FileScanPolicy,
        paths::ProjectContext,
        task::TaskType,
    },
    services::{
        import_v2::{
            capability_pack::{CapabilityPackManifest, ResolvedCapabilityPack},
            engine::{EngineRequest, EngineResult},
            file_discovery::{identify_file, new_import_inputs, FileDiscoveryService},
            pack_protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse},
            routes_for_format,
            source_registry::SourceManifest,
            ImportV2Service,
        },
        FileStore, GitService, SecretService,
    },
    tasks::TaskService,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const PRODUCTION_FORMAT_CASES: &[(&str, &str)] = &[
    ("note.md", "file.native"),
    ("note.txt", "file.native"),
    ("page.html", "file.native"),
    ("small.csv", "file.csv-package"),
    ("legacy.doc", "pack.office-legacy"),
    ("document.docx", "office.modern.docx"),
    ("legacy.xls", "pack.office-legacy"),
    ("workbook.xlsx", "office.modern.xlsx"),
    ("legacy.ppt", "pack.office-legacy"),
    ("presentation.pptx", "office.modern.pptx"),
    ("document.pdf", "pdf.text"),
    ("image.png", "ocr.cjk-accurate"),
    ("image.jpg", "ocr.cjk-accurate"),
    ("image.webp", "ocr.cjk-accurate"),
    ("image.bmp", "ocr.cjk-accurate"),
    ("image.tiff", "ocr.cjk-accurate"),
    ("media.heic", "ocr.cjk-accurate"),
    ("media.heif", "ocr.cjk-accurate"),
    ("animated.gif", "media.companion"),
    ("audio.mp3", "media.companion"),
    ("audio.wav", "media.companion"),
    ("media.m4a", "media.companion"),
    ("audio.aac", "media.companion"),
    ("audio.flac", "media.companion"),
    ("audio.ogg", "media.companion"),
    ("audio.opus", "media.companion"),
    ("audio.wma", "media.companion"),
    ("media.mp4", "media.companion"),
    ("media.mov", "media.companion"),
    ("video.mkv", "media.companion"),
    ("video.webm", "media.companion"),
    ("video.avi", "media.companion"),
    ("media.m4v", "media.companion"),
    ("video.wmv", "media.companion"),
    ("subtitle.srt", "media.subtitle"),
    ("subtitle.vtt", "media.subtitle"),
    ("subtitle.ass", "media.subtitle"),
    ("subtitle.lrc", "media.subtitle"),
];

fn install_batch9_runtime_pack(root: &Path) -> ResolvedCapabilityPack {
    let pack_root = root.join("batch9-runtime-fixture");
    std::fs::create_dir_all(&pack_root).unwrap();
    let source_executable = std::env::current_exe().unwrap();
    let executable_name = if cfg!(windows) {
        "batch9-capability-runner.exe"
    } else {
        "batch9-capability-runner"
    };
    let entrypoint = pack_root.join(executable_name);
    std::fs::copy(&source_executable, &entrypoint).unwrap();
    let executable_bytes = std::fs::read(&entrypoint).unwrap();
    ResolvedCapabilityPack {
        manifest: CapabilityPackManifest {
            schema_version: 2,
            pack_id: "batch9-runtime-fixture".into(),
            version: "1.0.0".into(),
            protocol_version: "2".into(),
            target_triples: vec![],
            archive_sha256: String::new(),
            license_expression: "MIT".into(),
            entrypoint: executable_name.into(),
            entrypoint_args: vec![
                "--ignored".into(),
                "--exact".into(),
                "batch9_capability_runner_process".into(),
                "--nocapture".into(),
            ],
            executable_files: Vec::new(),
            compressed_bytes: executable_bytes.len() as u64,
            installed_bytes: executable_bytes.len() as u64,
            signing_key_id: "batch9-test-only".into(),
            signature: String::new(),
            files: Vec::new(),
        },
        root: pack_root.canonicalize().unwrap(),
        entrypoint: entrypoint.canonicalize().unwrap(),
        entrypoint_sha256: format!("{:x}", Sha256::digest(&executable_bytes)),
    }
}

#[test]
#[ignore = "spawned only by PackProcessEngine to exercise the capability process boundary"]
fn batch9_capability_runner_process() {
    let mut request_json = String::new();
    std::io::stdin().read_to_string(&mut request_json).unwrap();
    let rpc: JsonRpcRequest<EngineRequest> = serde_json::from_str(request_json.trim()).unwrap();
    assert_eq!(rpc.jsonrpc, "2.0");
    assert_eq!(rpc.method, "import.execute");
    let response = match run_batch9_capability(&rpc.params) {
        Ok(result) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: rpc.id,
            result: Some(result),
            error: None,
        },
        Err(code) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: rpc.id,
            result: None,
            error: Some(JsonRpcError {
                code: -32020,
                message: "The Batch 9 capability fixture reported a typed failure.".into(),
                data: Some(serde_json::json!({ "code": code })),
            }),
        },
    };
    serde_json::to_writer(std::io::stdout(), &response).unwrap();
    println!();
}

fn run_batch9_capability(request: &EngineRequest) -> Result<EngineResult, &'static str> {
    let input = Path::new(&request.input.locator);
    let bytes = std::fs::read(input).map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    let extension = input
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let staging = PathBuf::from(&request.project_root).join(&request.staging_root);
    std::fs::create_dir_all(&staging).map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    match extension.as_str() {
        "doc" | "xls" | "ppt" => run_legacy_office_capability(request, &staging, &bytes),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" | "heic" | "heif" => {
            run_ocr_capability(request, &staging, &bytes)
        }
        _ if request.asr_probe_only => Err("IMPORT_EMBEDDED_SUBTITLE_UNAVAILABLE"),
        _ if request.local_asr_authorized => run_asr_capability(request, &staging, &bytes),
        _ => Err("IMPORT_ASR_AUTHORIZATION_REQUIRED"),
    }
}

fn run_legacy_office_capability(
    request: &EngineRequest,
    staging: &Path,
    bytes: &[u8],
) -> Result<EngineResult, &'static str> {
    if !bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        return Err("IMPORT_OFFICE_INVALID_OLE");
    }
    let text = extract_ole_fixture_text(bytes).ok_or("IMPORT_OFFICE_NO_TEXT")?;
    let extension = Path::new(&request.input.locator)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let converted_extension = match extension.as_str() {
        "doc" => "docx",
        "xls" => "xlsx",
        "ppt" => "pptx",
        _ => return Err("IMPORT_OFFICE_INVALID_OLE"),
    };
    let converted_relative = format!(
        "converted/{}.{}",
        Path::new(&request.input.display_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy(),
        converted_extension
    );
    let converted = staging.join(&converted_relative);
    std::fs::create_dir_all(converted.parent().unwrap())
        .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    let package = build_converted_ooxml(converted_extension, &text)?;
    std::fs::write(&converted, package).map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    std::fs::write(staging.join("source.bin"), bytes)
        .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    std::fs::write(
        staging.join("candidate.md"),
        format!(
            "# {}\n\nLegacy Office conversion produced validated OOXML; structured extraction continues through the built-in modern reader.\n",
            request.input.display_name
        ),
    )
    .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    std::fs::write(
        staging.join("metadata.json"),
        serde_json::to_vec(&serde_json::json!({
            "route": "pack.office-legacy",
            "convertedFormat": converted_extension,
            "inputBytes": bytes.len(),
            "oleText": text,
        }))
        .unwrap(),
    )
    .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    Ok(EngineResult {
        source_snapshot_path: "source.bin".into(),
        markdown_path: "candidate.md".into(),
        asset_paths: vec![converted_relative],
        metadata_path: Some("metadata.json".into()),
        title: request.input.display_name.clone(),
        text_coverage: Some(1.0),
        table_cell_accuracy: None,
        sheet_count_exact: (extension == "xls").then_some(1.0),
        slide_count_exact: (extension == "ppt").then_some(1.0),
        non_empty_cell_coverage: (extension == "xls").then_some(1.0),
        formula_value_pairs: (extension == "xls").then_some(1.0),
        meaningful_image_coverage: (extension == "ppt").then_some(1.0),
        continuation: None,
        warnings: vec!["BATCH9_FIXTURE_CAPABILITY_RUNTIME".into()],
    })
}

fn run_ocr_capability(
    request: &EngineRequest,
    staging: &Path,
    bytes: &[u8],
) -> Result<EngineResult, &'static str> {
    let (format, _) = identify_file(
        Path::new(&request.input.locator),
        &bytes[..bytes.len().min(8192)],
    )
    .map_err(|_| "IMPORT_OCR_INVALID_IMAGE")?;
    if routes_for_format(format).first().copied() != Some("ocr.cjk-accurate") {
        return Err("IMPORT_OCR_INVALID_IMAGE");
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    std::fs::write(staging.join("source.bin"), bytes)
        .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    std::fs::write(
        staging.join("candidate.md"),
        format!(
            "# {}\n\nBatch 9 OCR capability runtime decoded `{format:?}` input `{}` (sha256 `{}`).\n",
            request.input.display_name,
            request.input.display_name,
            &digest[..16]
        ),
    )
    .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    std::fs::write(
        staging.join("metadata.json"),
        serde_json::to_vec(&serde_json::json!({
            "confidence": 0.99,
            "format": format!("{format:?}"),
            "inputBytes": bytes.len(),
            "sha256": digest,
            "blocks": [{
                "confidence": 0.99,
                "coordinates": { "x": 0, "y": 0, "width": 100, "height": 100 }
            }]
        }))
        .unwrap(),
    )
    .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    Ok(EngineResult {
        source_snapshot_path: "source.bin".into(),
        markdown_path: "candidate.md".into(),
        asset_paths: Vec::new(),
        metadata_path: Some("metadata.json".into()),
        title: request.input.display_name.clone(),
        text_coverage: Some(0.99),
        table_cell_accuracy: None,
        sheet_count_exact: None,
        slide_count_exact: None,
        non_empty_cell_coverage: None,
        formula_value_pairs: None,
        meaningful_image_coverage: None,
        continuation: None,
        warnings: vec!["BATCH9_FIXTURE_CAPABILITY_RUNTIME".into()],
    })
}

fn run_asr_capability(
    request: &EngineRequest,
    staging: &Path,
    bytes: &[u8],
) -> Result<EngineResult, &'static str> {
    let (format, _) = identify_file(
        Path::new(&request.input.locator),
        &bytes[..bytes.len().min(8192)],
    )
    .map_err(|_| "IMPORT_ASR_DECODE_FAILED")?;
    let routes = routes_for_format(format);
    if !routes.contains(&"media.asr") {
        return Err("IMPORT_ASR_DECODE_FAILED");
    }
    let digest = format!("{:x}", Sha256::digest(bytes));
    let relative = ".sensevoice-output-batch9";
    let output = staging.join(relative);
    std::fs::create_dir_all(&output).map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    std::fs::write(
        output.join("source.json"),
        serde_json::to_vec(&serde_json::json!({
            "format": format!("{format:?}"),
            "inputBytes": bytes.len(),
            "sha256": digest,
        }))
        .unwrap(),
    )
    .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    std::fs::write(
        output.join("candidate.md"),
        format!(
            "# {}\n\n## [00:00:00.000]\n\nBatch 9 authorized ASR runtime decoded `{format:?}` input `{}`.\n",
            request.input.display_name, request.input.display_name
        ),
    )
    .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    std::fs::write(
        output.join("metadata.json"),
        serde_json::to_vec(&serde_json::json!({
            "engine": "batch9-runtime-fixture",
            "format": format!("{format:?}"),
            "sha256": digest,
        }))
        .unwrap(),
    )
    .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    Ok(EngineResult {
        source_snapshot_path: format!("{relative}/source.json"),
        markdown_path: format!("{relative}/candidate.md"),
        asset_paths: Vec::new(),
        metadata_path: Some(format!("{relative}/metadata.json")),
        title: request.input.display_name.clone(),
        text_coverage: Some(0.99),
        table_cell_accuracy: None,
        sheet_count_exact: None,
        slide_count_exact: None,
        non_empty_cell_coverage: None,
        formula_value_pairs: None,
        meaningful_image_coverage: None,
        continuation: None,
        warnings: vec!["BATCH9_FIXTURE_CAPABILITY_RUNTIME".into()],
    })
}

fn extract_ole_fixture_text(bytes: &[u8]) -> Option<String> {
    let mut candidates = Vec::new();
    let mut ascii = String::new();
    for byte in bytes {
        if matches!(byte, b' '..=b'~') {
            ascii.push(char::from(*byte));
        } else {
            if ascii.len() >= 8 {
                candidates.push(std::mem::take(&mut ascii));
            }
            ascii.clear();
        }
    }
    if ascii.len() >= 8 {
        candidates.push(ascii);
    }
    for offset in 0..2 {
        let mut utf16 = String::new();
        let mut index = offset;
        while index + 1 < bytes.len() {
            let value = u16::from_le_bytes([bytes[index], bytes[index + 1]]);
            if (0x20..=0x7e).contains(&value) {
                utf16.push(char::from_u32(u32::from(value)).unwrap());
            } else {
                if utf16.len() >= 8 {
                    candidates.push(std::mem::take(&mut utf16));
                }
                utf16.clear();
            }
            index += 2;
        }
        if utf16.len() >= 8 {
            candidates.push(utf16);
        }
    }
    candidates
        .iter()
        .filter(|value| value.contains("Batch"))
        .max_by_key(|value| value.len())
        .or_else(|| candidates.iter().max_by_key(|value| value.len()))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_converted_ooxml(extension: &str, text: &str) -> Result<Vec<u8>, &'static str> {
    let escaped = xml_escape(text);
    let mut entries = vec![(
        "[Content_Types].xml".to_string(),
        br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"/>"#.to_vec(),
    )];
    match extension {
        "docx" => entries.push((
            "word/document.xml".into(),
            format!(
                r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>{escaped}</w:t></w:r></w:p></w:body></w:document>"#
            )
            .into_bytes(),
        )),
        "xlsx" => {
            entries.extend([
                (
                    "xl/workbook.xml".into(),
                    r#"<workbook xmlns:r="r"><sheets><sheet name="Legacy 数据" sheetId="7" r:id="rIdSheet"/></sheets></workbook>"#.as_bytes().to_vec(),
                ),
                (
                    "xl/_rels/workbook.xml.rels".into(),
                    br#"<Relationships><Relationship Id="rIdSheet" Type="x/worksheet" Target="worksheets/sheet7.xml"/></Relationships>"#.to_vec(),
                ),
                (
                    "xl/worksheets/sheet7.xml".into(),
                    format!(
                        r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Legacy text</t></is></c><c r="B1" t="inlineStr"><is><t>Formula</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>{escaped}</t></is></c><c r="B2"><f>2*2</f><v>4</v></c></row></sheetData></worksheet>"#
                    )
                    .into_bytes(),
                ),
            ]);
        }
        "pptx" => {
            let image =
                include_bytes!("../../tests/fixtures/import-v2/local/batch3/matrix/image.png");
            entries.extend([
                (
                    "ppt/presentation.xml".into(),
                    br#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId r:id="rIdSlide"/></p:sldIdLst></p:presentation>"#.to_vec(),
                ),
                (
                    "ppt/_rels/presentation.xml.rels".into(),
                    br#"<Relationships><Relationship Id="rIdSlide" Type="x/slide" Target="slides/slide7.xml"/></Relationships>"#.to_vec(),
                ),
                (
                    "ppt/slides/slide7.xml".into(),
                    format!(
                        r#"<p:sld xmlns:p="p" xmlns:a="a" xmlns:r="r"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{escaped}</a:t></a:r></a:p></p:txBody></p:sp><p:pic><p:blipFill><a:blip r:embed="rIdImage"/></p:blipFill></p:pic></p:spTree></p:cSld></p:sld>"#
                    )
                    .into_bytes(),
                ),
                (
                    "ppt/slides/_rels/slide7.xml.rels".into(),
                    br#"<Relationships><Relationship Id="rIdImage" Type="x/image" Target="../media/image1.png"/></Relationships>"#.to_vec(),
                ),
                ("ppt/media/image1.png".into(), image.to_vec()),
            ]);
        }
        _ => return Err("IMPORT_OFFICE_INVALID_OLE"),
    }
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in entries {
        archive
            .start_file(name, options)
            .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
        archive
            .write_all(&bytes)
            .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")?;
    }
    archive
        .finish()
        .map(Cursor::into_inner)
        .map_err(|_| "IMPORT_CAPABILITY_FIXTURE_IO")
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[test]
fn every_supported_local_format_runs_discovery_route_execution_candidate_and_commit() {
    let temp = tempfile::tempdir().unwrap();
    let project_root = temp.path().join("project");
    std::fs::create_dir_all(project_root.join(".app")).unwrap();
    let context = ProjectContext::new("batch9-format-pipeline", project_root.clone());
    let files = FileStore;
    let git = GitService;
    let tasks = TaskService::default();
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    // Built-in routes execute their production parsers. Installable routes use
    // a deterministic subprocess payload, but cross the actual immutable-pack,
    // JSON-RPC, process-tree, staging, sanitization, and result-validation
    // boundary through PackProcessEngine.
    let runtime_pack = install_batch9_runtime_pack(temp.path());
    for (route, extensions) in [
        ("pack.office-legacy", vec!["doc", "xls", "ppt"]),
        (
            "ocr.cjk-accurate",
            vec![
                "png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff", "heic", "heif",
            ],
        ),
        (
            "media.asr",
            vec![
                "gif", "mp3", "wav", "m4a", "aac", "flac", "ogg", "opus", "wma", "mp4", "mov",
                "mkv", "webm", "avi", "m4v", "wmv",
            ],
        ),
    ] {
        service
            .register_capability_pack(
                runtime_pack.clone(),
                route.into(),
                extensions.into_iter().map(str::to_string).collect(),
                Duration::from_secs(30),
            )
            .unwrap();
    }

    let matrix_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/import-v2/local/batch3/matrix");
    let discovered = FileDiscoveryService::default()
        .scan(
            &context,
            &[matrix_root],
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    assert!(
        discovered.skipped.is_empty(),
        "the supported matrix must not contain skipped fixtures: {:?}",
        discovered.skipped
    );
    assert_eq!(discovered.files.len(), PRODUCTION_FORMAT_CASES.len());

    let expected = PRODUCTION_FORMAT_CASES
        .iter()
        .copied()
        .collect::<BTreeMap<_, _>>();
    let discovery_session = llm_wiki_desktop_lib::models::import_v2::ImportSession::new(
        "batch9-discovery",
        &context.project_id,
        ImportResourceMode::Balanced,
    );
    let discovered_inputs = new_import_inputs(&discovery_session, discovered.files)
        .into_iter()
        .map(|input| (input.display_name.clone(), input))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(discovered_inputs.len(), expected.len());

    let mut committed_total = 0usize;
    for (chunk_index, chunk) in PRODUCTION_FORMAT_CASES.chunks(4).enumerate() {
        let draft = service
            .create_session(&context, &files, ImportResourceMode::Balanced)
            .unwrap();
        let inputs = chunk
            .iter()
            .map(|(fixture, _)| discovered_inputs.get(*fixture).unwrap().clone())
            .collect();
        let session = service
            .add_inputs(&context, &files, &draft.session_id, inputs)
            .unwrap();
        assert_eq!(session.items.len(), chunk.len());

        for item in &session.items {
            let route = expected
                .get(item.input.display_name.as_str())
                .unwrap_or_else(|| panic!("unexpected fixture {}", item.input.display_name));
            let bytes = std::fs::read(&item.input.locator).unwrap();
            let (format, _) = identify_file(
                PathBuf::from(&item.input.locator).as_path(),
                &bytes[..bytes.len().min(8192)],
            )
            .unwrap();
            assert_eq!(
                routes_for_format(format).first().copied(),
                Some(*route),
                "{} did not retain its production route",
                item.input.display_name
            );

            let task = tasks
                .create_project_task(
                    TaskType::Import,
                    context.project_id.clone(),
                    project_root.clone(),
                    format!("Batch 9 format {}", item.input.display_name),
                    true,
                )
                .unwrap();
            let mut result = service
                .run_item(
                    &context,
                    &files,
                    &tasks,
                    &session.session_id,
                    &item.item_id,
                    &task.id,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "{} failed before authorization: {error:?}",
                        item.input.display_name
                    )
                });
            let authorization = match *route {
                "ocr.cjk-accurate" => Some(ImportMediaAuthorizationKind::Ocr),
                "media.companion" => Some(ImportMediaAuthorizationKind::Asr),
                _ => None,
            };
            if let Some(authorization) = authorization {
                assert_eq!(result.status, ImportItemStatus::WaitingAuthorization);
                let asr_profile = (authorization == ImportMediaAuthorizationKind::Asr)
                    .then_some(ImportAsrProfile::Balanced);
                service
                    .authorize_media_for_session(
                        &context,
                        &files,
                        &session.session_id,
                        &item.item_id,
                        authorization,
                        asr_profile,
                        None,
                    )
                    .unwrap();
                let authorized_task = tasks
                    .create_project_task(
                        TaskType::Import,
                        context.project_id.clone(),
                        project_root.clone(),
                        format!("Batch 9 authorized {}", item.input.display_name),
                        true,
                    )
                    .unwrap();
                result = service
                    .run_item(
                        &context,
                        &files,
                        &tasks,
                        &session.session_id,
                        &item.item_id,
                        &authorized_task.id,
                    )
                    .unwrap();
            }
            assert_eq!(
                result.status,
                ImportItemStatus::PreviewReady,
                "{} did not produce a candidate: {:?}; attempts: {:?}",
                item.input.display_name,
                result.issue,
                result.attempts
            );
            let expected_engine = match *route {
                "file.native" => "builtin.native-file",
                "file.csv-package" => "builtin.csv-package",
                "office.modern.docx" => "builtin.office-docx",
                "office.modern.xlsx" => "builtin.office-xlsx",
                "office.modern.pptx" => "builtin.office-pptx",
                "pdf.text" => "builtin.pdf-text",
                "media.companion" => "builtin.local-media-companion",
                "media.subtitle" => "builtin.local-subtitle",
                "pack.office-legacy" => "pack.batch9-runtime-fixture.pack.office-legacy",
                "ocr.cjk-accurate" => "pack.batch9-runtime-fixture.ocr.cjk-accurate",
                _ => unreachable!("unexpected production route {route}"),
            };
            assert!(
                result
                    .attempts
                    .iter()
                    .any(|attempt| attempt.route == *route && attempt.engine_id == expected_engine),
                "{} did not execute the selected production route: {:?}",
                item.input.display_name,
                result.attempts
            );
            if *route == "media.companion" {
                assert!(
                    result.attempts.iter().any(|attempt| {
                        attempt.route == "media.asr"
                            && attempt.engine_id == "pack.batch9-runtime-fixture.media.asr"
                    }),
                    "{} did not cross the explicit external ASR capability boundary: {:?}",
                    item.input.display_name,
                    result.attempts
                );
            }
        }

        let ready = service
            .load_session(&context, &files, &session.session_id)
            .unwrap();
        let input_paths_by_item = ready
            .items
            .iter()
            .map(|item| {
                let locator = PathBuf::from(&item.input.locator);
                let input_path = if locator.is_absolute() {
                    locator
                } else {
                    project_root.join(locator)
                };
                (item.item_id.clone(), input_path)
            })
            .collect::<BTreeMap<_, _>>();
        let decisions = ready
            .items
            .iter()
            .map(|item| CommitItemDecision {
                item_id: item.item_id.clone(),
                resolution: item
                    .preview
                    .as_ref()
                    .and_then(|preview| preview.resolution.as_ref())
                    .and_then(|resolution| resolution.default_resolution.clone()),
            })
            .collect();
        let batch = service
            .commit_items(
                &context,
                &files,
                &git,
                &CommitImportSessionRequest {
                    project_id: context.project_id.clone(),
                    project_root_path: project_root.to_string_lossy().into(),
                    session_id: session.session_id,
                    batch_task_id: None,
                    acknowledge_restricted_content: false,
                    expected_selection_revision: None,
                    expected_confirmation_digest: None,
                    decisions,
                },
            )
            .unwrap_or_else(|error| panic!("chunk {chunk_index} failed: {error:?}"));
        assert_eq!(batch.committed_count, chunk.len() as u32);
        assert_eq!(batch.failed_count, 0, "{batch:?}");
        assert_eq!(batch.items.len(), chunk.len());
        for item in &batch.items {
            assert!(item.committed, "{item:?}");
            assert!(project_root
                .join(item.wiki_path.as_deref().unwrap())
                .is_file());
            let manifest_path = project_root.join(format!(
                ".app/sources/{}.json",
                item.source_id.as_deref().unwrap()
            ));
            let manifest: SourceManifest =
                serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
            let version = manifest
                .versions
                .iter()
                .find(|version| version.version_id == manifest.current_version_id)
                .unwrap();
            let raw_snapshot = version
                .raw_evidence
                .iter()
                .find(|artifact| artifact.kind == "source_snapshot")
                .unwrap();
            let fixture_bytes =
                std::fs::read(input_paths_by_item.get(&item.item_id).unwrap()).unwrap();
            let committed_bytes = std::fs::read(project_root.join(&raw_snapshot.path)).unwrap();
            assert_eq!(
                committed_bytes, fixture_bytes,
                "{} did not preserve its original fixture bytes",
                item.item_id
            );
            assert_eq!(
                raw_snapshot.sha256,
                format!("{:x}", Sha256::digest(&fixture_bytes))
            );
            assert_eq!(raw_snapshot.size_bytes, fixture_bytes.len() as u64);
        }
        assert!(project_root
            .join(format!(".app/import-history/{}.json", batch.batch_id))
            .is_file());
        committed_total += batch.items.len();
    }
    assert_eq!(committed_total, PRODUCTION_FORMAT_CASES.len());
    assert!(!project_root.join(".app/compile").exists());
}
