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
    inspect_pdf_with_text(path, password).map(|(inspection, _)| inspection)
}

pub(crate) fn inspect_pdf_with_text(
    path: &Path,
    password: Option<&str>,
) -> Result<(PdfInspection, Vec<String>), PdfInspectionError> {
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
    let mut page_texts = Vec::with_capacity(pages.len());
    let mut image_only_pages = Vec::new();
    for page_number in pages.keys().copied() {
        let text = document.extract_text(&[page_number]).unwrap_or_default();
        let count = text.chars().filter(|c| !c.is_whitespace()).count() as u32;
        let content = document
            .get_page_content(pages[&page_number])
            .and_then(|bytes| lopdf::content::Content::decode(&bytes));
        let (painted, vector_painted) = content
            .as_ref()
            .map(|content| {
                (
                    content
                        .operations
                        .iter()
                        .any(|op| matches!(op.operator.as_str(), "Do" | "BI" | "ID")),
                    content.operations.iter().any(|op| {
                        matches!(
                            op.operator.as_str(),
                            "f" | "F" | "f*" | "S" | "s" | "B" | "B*"
                        )
                    }),
                )
            })
            .unwrap_or((true, false));
        // A header/watermark over a scan is not a readable text layer. Text
        // quality and actual painting evidence matter, not a character quota.
        let meaningful_lines = text
            .lines()
            .filter(|line| line.chars().any(char::is_alphanumeric))
            .count();
        let invalid_text = text.chars().any(|c| {
            c == '\u{fffd}'
                || ('\u{e000}'..='\u{f8ff}').contains(&c)
                || (c.is_control() && !c.is_whitespace())
        });
        let readable = text.chars().any(char::is_alphabetic) && !invalid_text;
        let large_image = page_has_large_image(&document, pages[&page_number]);
        if (count > 0 && !readable)
            || (painted && (!readable || (large_image && meaningful_lines <= 2)))
            || (count == 0 && vector_painted)
        {
            image_only_pages.push(page_number - 1);
        }
        counts.push(count);
        page_texts.push(text);
    }
    let estimated_ocr_pages = image_only_pages.len() as u32;
    Ok((
        PdfInspection {
            page_count: pages.len() as u32,
            text_characters_per_page: counts,
            image_only_pages,
            encrypted,
            active_actions: false,
            estimated_ocr_pages,
        },
        page_texts,
    ))
}

// Inspect the painted image footprint, including images nested in Forms. A
// small logo beside a short paragraph must not turn a readable page into OCR.
fn page_has_large_image(document: &Document, page: lopdf::ObjectId) -> bool {
    let mut node = Some(page);
    let mut seen = std::collections::HashSet::new();
    let mut page_area = None;
    while let Some(id) = node.filter(|id| seen.insert(*id)) {
        let Ok(dictionary) = document.get_dictionary(id) else {
            break;
        };
        if let Ok(bounds) = dictionary.get(b"MediaBox").and_then(Object::as_array) {
            let values = bounds
                .iter()
                .filter_map(|v| v.as_float().ok())
                .collect::<Vec<_>>();
            if values.len() == 4 {
                page_area = Some(((values[2] - values[0]) * (values[3] - values[1])).abs());
            }
            break;
        }
        node = dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .ok();
    }
    let Some(area) = page_area.filter(|area| *area > 0.0) else {
        return false;
    };
    let Ok((direct, inherited)) = document.get_page_resources(page) else {
        return false;
    };
    let resources = direct
        .into_iter()
        .chain(
            inherited
                .iter()
                .filter_map(|id| document.get_dictionary(*id).ok()),
        )
        .collect::<Vec<_>>();
    let Ok(content) = document.get_page_content(page) else {
        return false;
    };
    large_image_in_content(
        document,
        &content,
        &resources,
        [1.0, 0.0, 0.0, 1.0],
        area * 0.5,
        0,
    )
}

fn large_image_in_content(
    document: &Document,
    bytes: &[u8],
    resources: &[&lopdf::Dictionary],
    initial: [f32; 4],
    minimum_area: f32,
    depth: usize,
) -> bool {
    if depth > 8 {
        return false;
    }
    let Ok(content) = lopdf::content::Content::decode(bytes) else {
        return false;
    };
    let mut matrix = initial;
    let mut stack = Vec::new();
    for operation in content.operations {
        match operation.operator.as_str() {
            "q" => stack.push(matrix),
            "Q" => matrix = stack.pop().unwrap_or(initial),
            "cm" if operation.operands.len() == 6 => {
                let values = operation
                    .operands
                    .iter()
                    .filter_map(|v| v.as_float().ok())
                    .collect::<Vec<_>>();
                if values.len() == 6 {
                    matrix = multiply_linear(matrix, [values[0], values[1], values[2], values[3]]);
                }
            }
            "Do" => {
                let Some(name) = operation.operands.first().and_then(|v| v.as_name().ok()) else {
                    continue;
                };
                let object = resources.iter().find_map(|resource| {
                    let (_, map) = document.dereference(resource.get(b"XObject").ok()?).ok()?;
                    document
                        .dereference(map.as_dict().ok()?.get(name).ok()?)
                        .ok()
                        .map(|(_, value)| value)
                });
                let Some(stream) = object.and_then(|value| value.as_stream().ok()) else {
                    continue;
                };
                match stream.dict.get(b"Subtype").and_then(Object::as_name).ok() {
                    Some(b"Image")
                        if (matrix[0] * matrix[3] - matrix[1] * matrix[2]).abs()
                            >= minimum_area =>
                    {
                        return true
                    }
                    Some(b"Form") => {
                        let form_matrix = stream
                            .dict
                            .get(b"Matrix")
                            .and_then(Object::as_array)
                            .ok()
                            .and_then(|v| {
                                let values = v
                                    .iter()
                                    .filter_map(|v| v.as_float().ok())
                                    .collect::<Vec<_>>();
                                (values.len() == 6)
                                    .then(|| [values[0], values[1], values[2], values[3]])
                            })
                            .unwrap_or([1.0, 0.0, 0.0, 1.0]);
                        let form_resources = stream
                            .dict
                            .get(b"Resources")
                            .ok()
                            .and_then(|value| document.dereference(value).ok())
                            .and_then(|(_, value)| value.as_dict().ok());
                        let combined = form_resources
                            .into_iter()
                            .chain(resources.iter().copied())
                            .collect::<Vec<_>>();
                        let bytes = stream
                            .decompressed_content()
                            .unwrap_or_else(|_| stream.content.clone());
                        if large_image_in_content(
                            document,
                            &bytes,
                            &combined,
                            multiply_linear(matrix, form_matrix),
                            minimum_area,
                            depth + 1,
                        ) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    false
}

fn multiply_linear(left: [f32; 4], right: [f32; 4]) -> [f32; 4] {
    [
        left[0] * right[0] + left[2] * right[1],
        left[1] * right[0] + left[3] * right[1],
        left[0] * right[2] + left[2] * right[3],
        left[1] * right[2] + left[3] * right[3],
    ]
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
    prepare_selective_ocr_with_text(path, staging, page_plan, None)
}

pub(crate) fn prepare_selective_ocr_with_text(
    path: &Path,
    staging: &Path,
    page_plan: &[PdfPagePlan],
    page_texts: Option<&[String]>,
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
        } else if plan.route != PdfPageRoute::TextLayer {
            markdown.push_str(&format!(
                "> 第 {page_number} 页正文尚未识别 / Page {page_number} needs OCR.\n\n"
            ));
        } else {
            let text = page_texts
                .and_then(|texts| texts.get((*page_number - 1) as usize))
                .cloned()
                .unwrap_or_else(|| document.extract_text(&[*page_number]).unwrap_or_default());
            if !text.trim().is_empty() {
                markdown.push_str(text.trim());
                markdown.push_str("\n\n");
            }
        }
    }
    if !temporary_input_paths.is_empty() {
        let retained = workspace.retain();
        debug_assert!(retained.starts_with(staging));
    }
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
                b"AA" | b"JS" | b"JavaScript" | b"Launch" | b"RichMedia" | b"EmbeddedFiles"
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
