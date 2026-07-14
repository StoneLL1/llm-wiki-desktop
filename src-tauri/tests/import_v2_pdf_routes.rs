use llm_wiki_desktop_lib::services::import_v2::pdf_router::{
    inspect_pdf, plan_pdf_pages, PdfInspection, PdfInspectionError, PdfPageRoute,
    PdfRouteCapabilities,
};
use lopdf::{dictionary, Document, Object};

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
    assert_eq!(plans[1].route, PdfPageRoute::DocumentLayout);
    assert_eq!(plans[2].route, PdfPageRoute::SelectiveOcr);
    assert_eq!(plans[3].route, PdfPageRoute::SelectiveOcr);
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
    assert_eq!(report.image_only_pages, vec![0]);

    let active = temp.path().join("active.pdf");
    save_one_page_pdf(&active, true);
    assert_eq!(
        inspect_pdf(&active, None),
        Err(PdfInspectionError::ActiveContentRejected)
    );
}

#[test]
fn document_layout_pack_is_pinned_cross_platform_and_cannot_install_at_runtime() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("capabilities/document-layout/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["packId"], "document-layout");
    assert_eq!(manifest["version"], "2.48.0");
    assert_eq!(manifest["licenseExpression"], "MIT");
    assert_eq!(manifest["targetTriples"].as_array().unwrap().len(), 4);
    let runner =
        std::fs::read_to_string(root.join("capabilities/document-layout/runner/docling_pack.py"))
            .unwrap();
    assert!(runner.contains("waiting_capability"));
    assert!(!runner.contains("pip install"));
}
