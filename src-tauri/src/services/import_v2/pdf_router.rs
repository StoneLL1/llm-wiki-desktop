use std::path::Path;

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

const TEXT_LAYER_CHARACTERS: u32 = 500;
const LOW_TEXT_CHARACTERS: u32 = 32;

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
        if count == 0 {
            image_only_pages.push(page_number - 1);
        }
        counts.push(count);
    }
    let estimated_ocr_pages = counts
        .iter()
        .filter(|count| **count < LOW_TEXT_CHARACTERS)
        .count() as u32;
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
        .map(|(index, count)| {
            let (route, reason) = if *count >= TEXT_LAYER_CHARACTERS {
                (
                    PdfPageRoute::TextLayer,
                    "safe text layer has sufficient content",
                )
            } else if *count >= LOW_TEXT_CHARACTERS && capabilities.document_layout {
                (
                    PdfPageRoute::DocumentLayout,
                    "layout recovery required for sparse or complex text",
                )
            } else if *count < LOW_TEXT_CHARACTERS && capabilities.ocr {
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

fn has_active_content(object: &Object) -> bool {
    match object {
        Object::Dictionary(dictionary) => dictionary.iter().any(|(key, value)| {
            matches!(
                key.as_slice(),
                b"OpenAction"
                    | b"AA"
                    | b"A"
                    | b"JS"
                    | b"JavaScript"
                    | b"Launch"
                    | b"RichMedia"
                    | b"EmbeddedFiles"
            ) || has_active_content(value)
        }),
        Object::Stream(stream) => has_active_content(&Object::Dictionary(stream.dict.clone())),
        Object::Array(values) => values.iter().any(has_active_content),
        _ => false,
    }
}
