use llm_wiki_desktop_lib::services::import_v2::pdf_router::{
    inspect_pdf, plan_pdf_pages, prepare_selective_ocr, PdfInspection, PdfInspectionError,
    PdfPageRoute, PdfRouteCapabilities,
};
use llm_wiki_desktop_lib::{
    models::{
        import_v2::{ImportInput, ImportInputKind, ImportResourceMode, SourceIdentity},
        paths::ProjectContext,
        task::TaskType,
    },
    services::{import_v2::ImportV2Service, FileStore, SecretService},
    tasks::TaskService,
};
use lopdf::{dictionary, Document, Object};
use sha2::{Digest, Sha256};

fn inspection(characters: &[u32]) -> PdfInspection {
    PdfInspection {
        page_count: characters.len() as u32,
        text_characters_per_page: characters.to_vec(),
        image_only_pages: characters
            .iter()
            .enumerate()
            .filter_map(|(index, count)| (*count == 0).then_some(index as u32))
            .collect(),
        encrypted: false,
        active_actions: false,
        estimated_ocr_pages: characters.iter().filter(|count| **count < 32).count() as u32,
    }
}

#[test]
fn mixed_pdf_routes_text_layout_ocr_then_agent_per_page() {
    let plans = plan_pdf_pages(
        &inspection(&[2_000, 140, 0, 12]),
        PdfRouteCapabilities {
            document_layout: true,
            ocr: true,
            agent: true,
        },
    )
    .unwrap();
    assert_eq!(plans.len(), 4);
    assert_eq!(plans[0].route, PdfPageRoute::TextLayer);
    assert_eq!(plans[1].route, PdfPageRoute::TextLayer);
    assert_eq!(plans[2].route, PdfPageRoute::SelectiveOcr);
    assert_eq!(plans[3].route, PdfPageRoute::TextLayer);
    assert!(plans.iter().all(|plan| !plan.reason.is_empty()));
}

#[test]
fn missing_deterministic_capabilities_yields_explicit_agent_or_waiting_route() {
    let agent = plan_pdf_pages(
        &inspection(&[0]),
        PdfRouteCapabilities {
            document_layout: false,
            ocr: false,
            agent: true,
        },
    )
    .unwrap();
    assert_eq!(agent[0].route, PdfPageRoute::AgentEligible);

    let waiting = plan_pdf_pages(
        &inspection(&[0]),
        PdfRouteCapabilities {
            document_layout: false,
            ocr: false,
            agent: false,
        },
    )
    .unwrap();
    assert_eq!(waiting[0].route, PdfPageRoute::WaitingCapability);
}

#[test]
fn encryption_and_active_actions_fail_closed_before_any_route_executes() {
    let mut encrypted = inspection(&[100]);
    encrypted.encrypted = true;
    let error = plan_pdf_pages(&encrypted, PdfRouteCapabilities::default()).unwrap_err();
    assert_eq!(
        error,
        PdfInspectionError::PasswordRequired {
            user_action_required: true
        }
    );

    let mut active = inspection(&[100]);
    active.active_actions = true;
    let error = plan_pdf_pages(&active, PdfRouteCapabilities::default()).unwrap_err();
    assert_eq!(error, PdfInspectionError::ActiveContentRejected);
}

#[test]
fn clean_text_quality_contract_requires_exact_pages_and_98_percent_coverage() {
    let report = inspection(&[1_000, 1_000]);
    assert!(report.meets_quality_contract(2, 0.98));
    assert!(!report.meets_quality_contract(1, 1.0));
    assert!(!report.meets_quality_contract(2, 0.979));
}

#[test]
fn pdf_types_are_json_protocol_safe_and_never_contain_a_password_field() {
    let json = serde_json::to_string(&inspection(&[10, 0])).unwrap();
    assert!(json.contains("textCharactersPerPage"));
    assert!(!json.to_ascii_lowercase().contains("password"));
}

fn save_one_page_pdf(path: &std::path::Path, active: bool) {
    let mut document = Document::with_version("1.5");
    let pages = document.new_object_id();
    let page = document.new_object_id();
    let catalog = document.new_object_id();
    document.objects.insert(
        page,
        Object::Dictionary(dictionary! {
            "Type" => "Page", "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! {},
        }),
    );
    document.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
        }),
    );
    let mut catalog_dictionary = dictionary! { "Type" => "Catalog", "Pages" => pages };
    if active {
        catalog_dictionary.set(
            "OpenAction",
            dictionary! { "S" => "JavaScript", "JS" => "noop" },
        );
    }
    document
        .objects
        .insert(catalog, Object::Dictionary(catalog_dictionary));
    document.trailer.set("Root", catalog);
    document.compress();
    document.save(path).unwrap();
}

#[test]
fn passive_inspection_counts_every_page_and_never_executes_actions() {
    let temp = tempfile::tempdir().unwrap();
    let safe = temp.path().join("safe.pdf");
    save_one_page_pdf(&safe, false);
    let report = inspect_pdf(&safe, None).unwrap();
    assert_eq!(report.page_count, 1);
    assert_eq!(report.text_characters_per_page.len(), 1);
    assert!(report.image_only_pages.is_empty());

    let active = temp.path().join("active.pdf");
    save_one_page_pdf(&active, true);
    assert_eq!(
        inspect_pdf(&active, None),
        Err(PdfInspectionError::ActiveContentRejected)
    );
}

#[test]
fn document_layout_pack_is_pinned_cross_platform_and_offline_at_runtime() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("capabilities/document-layout/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["packId"], "document-layout");
    assert_eq!(manifest["version"], "2.48.0");
    assert_eq!(
        manifest["licenseExpression"],
        "MIT AND Apache-2.0 AND CDLA-Permissive-2.0 AND PSF-2.0 AND MPL-2.0 AND LicenseRef-Bundled-Third-Party-Notices"
    );
    // PyTorch ships no x86_64 macOS wheels, so document-layout publishes exactly
    // three targets and Intel macOS keeps the document-standard fallback.
    assert_eq!(
        manifest["targetTriples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "x86_64-pc-windows-msvc",
            "aarch64-apple-darwin",
            "x86_64-unknown-linux-gnu",
        ]
    );
    let runner =
        std::fs::read_to_string(root.join("capabilities/document-layout/runner/docling_pack.py"))
            .unwrap();
    assert!(runner.contains("DocumentConverter"));
    assert!(runner.contains("options.enable_remote_services = False"));
    assert!(runner.contains("options.allow_external_plugins = False"));
    assert!(!runner.contains("pip install"));
}

fn batch3_pdf(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/import-v2/local/batch3")
        .join(name)
}

#[test]
fn real_mixed_pdf_routes_only_the_scanned_page_to_ocr() {
    let report = inspect_pdf(&batch3_pdf("mixed-text-scan.pdf"), None).unwrap();
    assert_eq!(report.page_count, 2);
    assert!(report.text_characters_per_page[0] > 500);
    assert_eq!(report.text_characters_per_page[1], 0);
    let plan = plan_pdf_pages(
        &report,
        PdfRouteCapabilities {
            document_layout: false,
            ocr: true,
            agent: false,
        },
    )
    .unwrap();
    assert_eq!(plan[0].route, PdfPageRoute::TextLayer);
    assert_eq!(plan[1].route, PdfPageRoute::SelectiveOcr);
}

#[test]
fn selective_pdf_ocr_stages_only_planned_scan_pages_in_original_order() {
    let pdf = batch3_pdf("mixed-text-scan.pdf");
    let report = inspect_pdf(&pdf, None).unwrap();
    let plan = plan_pdf_pages(
        &report,
        PdfRouteCapabilities {
            document_layout: false,
            ocr: true,
            agent: false,
        },
    )
    .unwrap();
    let staging = tempfile::tempdir().unwrap();
    let prepared = prepare_selective_ocr(&pdf, staging.path(), &plan).unwrap();
    assert_eq!(prepared.temporary_input_paths.len(), 1);
    assert!(prepared.temporary_input_paths[0].ends_with("page-002.pdf"));
    assert!(staging
        .path()
        .join(&prepared.temporary_input_paths[0])
        .is_file());
    let rendered_page =
        Document::load(staging.path().join(&prepared.temporary_input_paths[0])).unwrap();
    assert_eq!(rendered_page.get_pages().len(), 1);
    assert!(!rendered_page
        .get_page_content(*rendered_page.get_pages().values().next().unwrap())
        .unwrap()
        .is_empty());
    let first = prepared.markdown.find("## Page 1").unwrap();
    let second = prepared.markdown.find("## Page 2").unwrap();
    assert!(first < second);
    assert!(prepared.markdown.contains("<!-- OCR_PAGE_002 -->"));
    assert!(!prepared.markdown.contains("<!-- OCR_PAGE_001 -->"));
}

#[test]
fn short_text_with_uri_link_is_readable_and_scan_page_number_still_needs_ocr() {
    use lopdf::Stream;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("短文 with link.pdf");
    let mut doc = Document::load(batch3_pdf("mixed-text-scan.pdf")).unwrap();
    let pages = doc.get_pages();
    let font = doc.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let text = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /FShort 12 Tf 50 700 Td (A short readable page with a normal hyperlink.) Tj ET"
            .to_vec(),
    ));
    let link = doc.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Link", "Rect" => vec![0.into(),0.into(),100.into(),20.into()], "A" => dictionary!{ "S" => "URI", "URI" => Object::string_literal("https://example.org/article") } });
    let page = doc
        .get_object_mut(pages[&1])
        .unwrap()
        .as_dict_mut()
        .unwrap();
    page.set("Contents", text);
    page.set(
        "Resources",
        dictionary! { "Font" => dictionary! { "FShort" => font } },
    );
    page.set("Annots", vec![Object::Reference(link)]);
    // Retain the scanned page's painting operators, including any nested Form,
    // and add a page number that must not masquerade as the page's body.
    let mut content = doc.get_page_content(pages[&2]).unwrap();
    content.extend_from_slice(b"\nBT /FNumber 12 Tf 50 30 Td (2) Tj ET");
    let stream = doc.add_object(Stream::new(dictionary! {}, content));
    let mut resources = doc
        .get_page_resources(pages[&2])
        .unwrap()
        .0
        .unwrap()
        .clone();
    resources.set("Font", dictionary! { "FNumber" => font });
    // Exercise inherited resources and Form XObjects by wrapping the scan.
    let form = doc.add_object(Stream::new(dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(),0.into(),612.into(),792.into()], "Resources" => resources }, doc.get_page_content(pages[&2]).unwrap()));
    let mut wrapped = b"q /ScanForm Do Q\nBT /FNumber 12 Tf 50 30 Td (2) Tj ET".to_vec();
    let wrapped_stream = doc.add_object(Stream::new(dictionary! {}, std::mem::take(&mut wrapped)));
    let page = doc
        .get_object_mut(pages[&2])
        .unwrap()
        .as_dict_mut()
        .unwrap();
    page.set("Contents", wrapped_stream);
    page.set("Resources", dictionary! { "Font" => dictionary! { "FNumber" => font }, "XObject" => dictionary! { "ScanForm" => form } });
    let _ = stream;
    doc.save(&path).unwrap();
    let report = inspect_pdf(&path, None).unwrap();
    assert!(report.text_characters_per_page[0] < 100);
    assert!(report.text_characters_per_page[0] > 0);
    assert_eq!(report.image_only_pages, vec![1]);
    let plan = plan_pdf_pages(&report, PdfRouteCapabilities::default()).unwrap();
    assert_eq!(plan[0].route, PdfPageRoute::TextLayer);
    assert_eq!(plan[1].route, PdfPageRoute::WaitingCapability);
}

#[test]
fn encrypted_real_pdf_fails_before_any_raw_or_source_write() {
    assert!(matches!(
        inspect_pdf(&batch3_pdf("encrypted.pdf"), None),
        Err(PdfInspectionError::PasswordRequired { .. })
    ));

    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".app")).unwrap();
    let input = root.path().join("inputs/encrypted.pdf");
    std::fs::create_dir_all(input.parent().unwrap()).unwrap();
    std::fs::copy(batch3_pdf("encrypted.pdf"), &input).unwrap();
    let bytes = std::fs::read(&input).unwrap();
    let canonical = input.canonicalize().unwrap();
    let context = ProjectContext::new("encrypted-pdf", root.path().to_path_buf());
    let files = FileStore;
    let service = ImportV2Service::with_secret_service(SecretService::memory());
    let session = service
        .create_session(&context, &files, ImportResourceMode::Balanced)
        .unwrap();
    let session = service
        .add_inputs(
            &context,
            &files,
            &session.session_id,
            vec![ImportInput {
                kind: ImportInputKind::File,
                display_name: "encrypted.pdf".into(),
                locator: input.to_string_lossy().into_owned(),
                normalized_locator: Some(format!(
                    "file:{}",
                    input.to_string_lossy().replace('\\', "/")
                )),
                source_identity: Some(SourceIdentity {
                    canonical_path: canonical.to_string_lossy().into_owned(),
                    size_bytes: bytes.len() as u64,
                    modified_nanos: None,
                    file_id: None,
                    sha256: format!("{:x}", Sha256::digest(&bytes)),
                    magic: format!("{:x}", Sha256::digest(&bytes[..bytes.len().min(8192)])),
                }),
                media_save_mode: Default::default(),
            }],
        )
        .unwrap();
    let tasks = TaskService::default();
    let task = tasks
        .create_project_task(
            TaskType::Import,
            context.project_id.clone(),
            root.path().to_path_buf(),
            "encrypted PDF".into(),
            true,
        )
        .unwrap();
    let error = service
        .run_item(
            &context,
            &files,
            &tasks,
            &session.session_id,
            &session.items[0].item_id,
            &task.id,
        )
        .unwrap_err();
    assert_eq!(error.code, "IMPORT_PDF_ENCRYPTED_UNSUPPORTED");
    assert!(!root.path().join("raw").exists());
    assert!(!root.path().join("wiki").exists());
    assert!(!root.path().join(".app/sources").exists());
}
