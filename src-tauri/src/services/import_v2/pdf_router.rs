use std::path::Path;

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

use crate::errors::BackendError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PdfInspection {
    pub page_count: u32,
    pub text_characters_per_page: Vec<u32>,
    pub image_only_pages: Vec<u32>,
    pub encrypted: bool,
    pub active_actions: bool,
    pub estimated_ocr_pages: u32,
}

impl PdfInspection {
    pub fn meets_quality_contract(
        &self,
        extracted_page_count: u32,
        normalized_coverage: f64,
    ) -> bool {
        self.page_count == extracted_page_count
            && self.text_characters_per_page.len() == self.page_count as usize
            && !self.active_actions
            && normalized_coverage >= 0.98
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "code")]
pub enum PdfInspectionError {
    PasswordRequired { user_action_required: bool },
    InvalidPassword { user_action_required: bool },
    ActiveContentRejected,
    CorruptInput,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PdfRouteCapabilities {
    pub document_layout: bool,
    pub ocr: bool,
    pub agent: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PdfPageRoute {
    TextLayer,
    DocumentLayout,
    SelectiveOcr,
    AgentEligible,
    WaitingCapability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PdfPagePlan {
    pub page_index: u32,
    pub route: PdfPageRoute,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfSelectiveOcrPreparation {
    pub markdown: String,
    pub temporary_input_paths: Vec<String>,
}

/// Performs only passive parsing. PDF actions, JavaScript, launch targets, and
/// external objects are detected but never evaluated or followed.
pub fn inspect_pdf(
    path: &Path,
    password: Option<&str>,
) -> Result<PdfInspection, PdfInspectionError> {
    let mut document = Document::load(path).map_err(|_| PdfInspectionError::CorruptInput)?;
    let encrypted = document.is_encrypted();
    if encrypted {
        let password = password.ok_or(PdfInspectionError::PasswordRequired {
            user_action_required: true,
        })?;
        document
            .decrypt(password)
            .map_err(|_| PdfInspectionError::InvalidPassword {
                user_action_required: true,
            })?;
    }
    let active_actions = document.objects.values().any(has_active_content);
    if active_actions {
        return Err(PdfInspectionError::ActiveContentRejected);
    }
    let pages = document.get_pages();
    if pages.is_empty() {
        return Err(PdfInspectionError::CorruptInput);
    }
    let mut counts = Vec::with_capacity(pages.len());
    let mut image_only_pages = Vec::new();
    for page_number in pages.keys().copied() {
        let count = document
            .extract_text(&[page_number])
            .unwrap_or_default()
            .chars()
            .filter(|character| !character.is_whitespace())
            .count() as u32;
        // Inspect painting operations, not only top-level image resources: a
        // scan may be inside a Form XObject or use inherited resources. A small
        // page number does not make the underlying scanned page readable.
        let painted = document
            .get_page_content(pages[&page_number])
            .and_then(|bytes| lopdf::content::Content::decode(&bytes))
            .map(|content| {
                content
                    .operations
                    .iter()
                    .any(|operation| matches!(operation.operator.as_str(), "Do" | "BI" | "ID"))
            })
            .unwrap_or(true);
        if count < 32 && painted {
            image_only_pages.push(page_number - 1);
        }
        counts.push(count);
    }
    let estimated_ocr_pages = image_only_pages.len() as u32;
    Ok(PdfInspection {
        page_count: pages.len() as u32,
        text_characters_per_page: counts,
        image_only_pages,
        encrypted,
        active_actions: false,
        estimated_ocr_pages,
    })
}

pub fn plan_pdf_pages(
    inspection: &PdfInspection,
    capabilities: PdfRouteCapabilities,
) -> Result<Vec<PdfPagePlan>, PdfInspectionError> {
    if inspection.encrypted {
        return Err(PdfInspectionError::PasswordRequired {
            user_action_required: true,
        });
    }
    if inspection.active_actions {
        return Err(PdfInspectionError::ActiveContentRejected);
    }
    Ok(inspection
        .text_characters_per_page
        .iter()
        .enumerate()
        .map(|(index, _count)| {
            let needs_ocr = inspection.image_only_pages.contains(&(index as u32));
            let (route, reason) = if !needs_ocr {
                (
                    PdfPageRoute::TextLayer,
                    "preserve readable text or a blank page",
                )
            } else if capabilities.ocr {
                (
                    PdfPageRoute::SelectiveOcr,
                    "page has insufficient text and requires selective OCR",
                )
            } else if capabilities.document_layout {
                (
                    PdfPageRoute::DocumentLayout,
                    "OCR unavailable; preserve page through layout pack",
                )
            } else if capabilities.agent {
                (
                    PdfPageRoute::AgentEligible,
                    "deterministic capabilities unavailable",
                )
            } else {
                (
                    PdfPageRoute::WaitingCapability,
                    "required local document capability is not installed",
                )
            };
            PdfPagePlan {
                page_index: index as u32,
                route,
                reason: reason.into(),
            }
        })
        .collect())
}

pub fn prepare_selective_ocr(
    path: &Path,
    staging: &Path,
    page_plan: &[PdfPagePlan],
) -> Result<PdfSelectiveOcrPreparation, BackendError> {
    let document = Document::load(path)
        .map_err(|_| pdf_stage_error("The PDF could not be reopened for selective OCR."))?;
    let pages = document.get_pages().into_iter().collect::<Vec<_>>();
    if pages.len() != page_plan.len() {
        return Err(pdf_stage_error(
            "The PDF page list changed during selective OCR preparation.",
        ));
    }
    let workspace =
        crate::services::import_v2::media_router::TemporaryMediaWorkspace::create_unique(
            staging,
            ".ocr-input",
        )?;
    let mut markdown = String::new();
    let mut temporary_input_paths = Vec::new();
    for ((page_number, _page_id), plan) in pages.iter().zip(page_plan) {
        markdown.push_str(&format!("## Page {page_number}\n\n"));
        if plan.route == PdfPageRoute::SelectiveOcr {
            // Preserve the entire page, including transforms, overlays and all
            // images. The existing OCR runtime renders this one-page PDF with
            // PDFium; the largest embedded image is not a page rendering.
            let output = workspace.path().join(format!("page-{page_number:03}.pdf"));
            let mut page_document = document.clone();
            let other_pages = pages
                .iter()
                .map(|(number, _)| *number)
                .filter(|number| number != page_number)
                .collect::<Vec<_>>();
            page_document.delete_pages(&other_pages);
            page_document.prune_objects();
            page_document
                .save(&output)
                .map_err(|_| pdf_stage_error("The PDF OCR page could not be staged."))?;
            let relative = output
                .strip_prefix(staging)
                .map_err(|_| pdf_stage_error("The PDF OCR page path escaped staging."))?
                .to_string_lossy()
                .replace('\\', "/");
            temporary_input_paths.push(relative);
            markdown.push_str(&format!("<!-- OCR_PAGE_{page_number:03} -->\n\n"));
        } else {
            let text = document.extract_text(&[*page_number]).unwrap_or_default();
            if !text.trim().is_empty() {
                markdown.push_str(text.trim());
                markdown.push_str("\n\n");
            }
        }
    }
    let retained = workspace.retain();
    debug_assert!(retained.starts_with(staging));
    Ok(PdfSelectiveOcrPreparation {
        markdown,
        temporary_input_paths,
    })
}

fn pdf_stage_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_PDF_SELECTIVE_OCR_FAILED", message, true, true)
}

fn has_active_content(object: &Object) -> bool {
    match object {
        Object::Dictionary(dictionary) => dictionary.iter().any(|(key, value)| {
            matches!(
                key.as_slice(),
                b"OpenAction"
                    | b"AA"
                    | b"JS"
                    | b"JavaScript"
                    | b"Launch"
                    | b"RichMedia"
                    | b"EmbeddedFiles"
            ) || (key.as_slice() == b"S"
                && value.as_name().is_ok_and(|name| {
                    matches!(
                        name,
                        b"JavaScript" | b"Launch" | b"SubmitForm" | b"ImportData"
                    )
                }))
                || has_active_content(value)
        }),
        Object::Stream(stream) => has_active_content(&Object::Dictionary(stream.dict.clone())),
        Object::Array(values) => values.iter().any(has_active_content),
        _ => false,
    }
}
