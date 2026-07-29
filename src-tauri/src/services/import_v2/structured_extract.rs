use crate::errors::BackendError;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Seek};
use zip::ZipArchive;

const MAX_OOXML_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OOXML_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OOXML_ENTRIES: usize = 4_096;

pub(crate) fn extract_pdf_markdown_from_bytes(bytes: &[u8]) -> Result<String, BackendError> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(bytes).map_err(|error| {
        BackendError::new(
            "IMPORT_FILE_PARSE_FAILED",
            format!("PDF parsing failed: {error}"),
            true,
            true,
        )
    })?;
    let text = pages
        .iter()
        .map(|page| page.trim())
        .filter(|page| !page.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    if text.trim().is_empty() {
        return Err(BackendError::new(
            "IMPORT_FILE_QUALITY_FAILED",
            "The PDF has no extractable text layer; OCR or layout assistance is required.",
            true,
            true,
        ));
    }
    Ok(normalize_extracted_markdown(&text))
}

pub(crate) fn extract_ooxml_markdown_from_bytes(
    extension: &str,
    bytes: &[u8],
) -> Result<String, BackendError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        BackendError::new(
            "IMPORT_FILE_PARSE_FAILED",
            format!("Office archive could not be opened: {error}"),
            true,
            true,
        )
    })?;
    validate_archive_limits(&mut archive)?;
    let text = match extension {
        "docx" => read_docx_text(&mut archive)?,
        "xlsx" => read_xlsx_text(&mut archive)?,
        "pptx" => read_pptx_text(&mut archive)?.0,
        _ => {
            return Err(BackendError::new(
                "IMPORT_FILE_PARSE_FAILED",
                "The built-in Office reader does not support this extension.",
                false,
                true,
            ));
        }
    };
    if text.trim().is_empty() {
        return Err(BackendError::new(
            "IMPORT_FILE_QUALITY_FAILED",
            "The Office file contains no extractable text.",
            true,
            true,
        ));
    }
    Ok(normalize_extracted_markdown(&text))
}

fn normalize_extracted_markdown(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    format!("{}\n", normalized.trim_end_matches('\n'))
}

fn read_docx_text<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<String, BackendError> {
    let mut output = String::new();
    for name in [
        "word/document.xml",
        "word/footnotes.xml",
        "word/endnotes.xml",
    ] {
        if let Ok(mut entry) = archive.by_name(name) {
            ensure_entry_size(&entry)?;
            let mut xml = String::new();
            entry.read_to_string(&mut xml).map_err(io_read_err)?;
            let markdown = docx_xml_to_markdown(&xml)?;
            if !markdown.trim().is_empty() {
                if !output.is_empty() {
                    output.push_str("\n\n");
                }
                output.push_str(markdown.trim());
            }
        }
    }
    Ok(output)
}

fn docx_xml_to_markdown(xml: &str) -> Result<String, BackendError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut output = String::new();
    let mut paragraph = String::new();
    let mut heading_level = None;
    let mut is_list = false;
    let mut in_text = false;
    let mut in_table = false;
    let mut in_cell = false;
    let mut cell = String::new();
    let mut row = Vec::new();
    let mut table = Vec::new();
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"p" => {
                    paragraph.clear();
                    heading_level = None;
                    is_list = false;
                }
                b"pStyle" => heading_level = heading_level_from_attributes(&event),
                b"numPr" => is_list = true,
                b"t" => in_text = true,
                b"tbl" => {
                    in_table = true;
                    table.clear();
                }
                b"tr" => row.clear(),
                b"tc" => {
                    in_cell = true;
                    cell.clear();
                }
                _ => {}
            },
            Ok(Event::Empty(event)) => match local_name(event.name().as_ref()) {
                b"pStyle" => heading_level = heading_level_from_attributes(&event),
                b"numPr" => is_list = true,
                _ => {}
            },
            Ok(Event::Text(text)) if in_text => {
                paragraph.push_str(&text.unescape().map_err(xml_err)?);
            }
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"t" => in_text = false,
                b"p" => {
                    let value = paragraph.trim();
                    if in_cell {
                        if !cell.is_empty() && !value.is_empty() {
                            cell.push_str("<br>");
                        }
                        cell.push_str(value);
                    } else if !in_table && !value.is_empty() {
                        if let Some(level) = heading_level {
                            output.push_str(&"#".repeat(level));
                            output.push(' ');
                        } else if is_list {
                            output.push_str("- ");
                        }
                        output.push_str(value);
                        output.push_str("\n\n");
                    }
                }
                b"tc" => {
                    row.push(markdown_table_cell(&cell));
                    in_cell = false;
                }
                b"tr" => {
                    if !row.is_empty() {
                        table.push(std::mem::take(&mut row));
                    }
                }
                b"tbl" => {
                    output.push_str(&rows_to_markdown_table(&table));
                    output.push('\n');
                    in_table = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(output.trim().to_string())
}

fn heading_level_from_attributes(event: &BytesStart<'_>) -> Option<usize> {
    let value = event.attributes().flatten().find_map(|attribute| {
        (local_name(attribute.key.as_ref()) == b"val")
            .then(|| String::from_utf8_lossy(attribute.value.as_ref()).into_owned())
    })?;
    let lower = value.to_ascii_lowercase();
    let digits = lower.strip_prefix("heading")?.trim();
    digits.parse::<usize>().ok().map(|level| level.clamp(1, 6))
}

fn read_pptx_text<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<(String, u32), BackendError> {
    let names = archive_names(archive);
    let mut fallback_slides = names
        .iter()
        .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
        .cloned()
        .collect::<Vec<_>>();
    fallback_slides.sort_by_key(|name| numbered_part(name, "ppt/slides/slide", ".xml"));
    let slides = presentation_slide_parts(archive)?.unwrap_or(fallback_slides);
    let slide_count = slides.len() as u32;
    let mut output = String::new();
    for (index, name) in slides.into_iter().enumerate() {
        let xml = read_required_archive_text(archive, &name)?;
        output.push_str(&format!("## Slide {}\n", index + 1));
        output.push_str(pptx_slide_to_markdown(&xml)?.trim());
        if let Some(notes_part) = slide_notes_part(archive, &name)? {
            let notes_xml = read_required_archive_text(archive, &notes_part)?;
            let notes = pptx_slide_to_markdown(&notes_xml)?
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            if !notes.is_empty() {
                output.push_str(&format!(
                    "\n\n> Speaker notes (slide {}): {notes}",
                    index + 1
                ));
            }
        }
        output.push_str("\n\n");
    }
    Ok((output, slide_count))
}

#[derive(Debug)]
struct OoxmlRelationship {
    id: String,
    target: String,
    relationship_type: String,
}

fn presentation_slide_parts<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<Option<Vec<String>>, BackendError> {
    let Some(presentation) = read_optional_archive_text(archive, "ppt/presentation.xml")? else {
        return Ok(None);
    };
    let Some(relationships) =
        read_optional_archive_text(archive, "ppt/_rels/presentation.xml.rels")?
    else {
        return Ok(None);
    };
    let relationships = read_ooxml_relationships(&relationships)?;
    let mut reader = Reader::from_str(&presentation);
    let mut slide_parts = Vec::new();
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if local_name(event.name().as_ref()) == b"sldId" =>
            {
                let relationship_id =
                    event
                        .attributes()
                        .flatten()
                        .find_map(|attribute| {
                            attribute.key.as_ref().ends_with(b":id").then(|| {
                                String::from_utf8_lossy(attribute.value.as_ref()).into_owned()
                            })
                        })
                        .ok_or_else(|| {
                            BackendError::new(
                                "IMPORT_FILE_PARSE_FAILED",
                                "A PPTX slide declaration has no relationship identifier.",
                                true,
                                true,
                            )
                        })?;
                let relationship = relationships
                    .iter()
                    .find(|relationship| {
                        relationship.id == relationship_id
                            && relationship.relationship_type.ends_with("/slide")
                    })
                    .ok_or_else(|| {
                        BackendError::new(
                            "IMPORT_FILE_PARSE_FAILED",
                            "A PPTX slide declaration has no matching slide relationship.",
                            true,
                            true,
                        )
                    })?;
                let part = resolve_ooxml_target("ppt/presentation.xml", &relationship.target)
                    .ok_or_else(|| {
                        BackendError::new(
                            "IMPORT_FILE_PARSE_FAILED",
                            "A PPTX slide relationship contains an unsafe target.",
                            true,
                            true,
                        )
                    })?;
                slide_parts.push(part);
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(Some(slide_parts))
}

fn slide_notes_part<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    slide_part: &str,
) -> Result<Option<String>, BackendError> {
    let relationship_part = relationship_part_path(slide_part);
    let Some(xml) = read_optional_archive_text(archive, &relationship_part)? else {
        return Ok(None);
    };
    let relationship = read_ooxml_relationships(&xml)?
        .into_iter()
        .find(|relationship| relationship.relationship_type.ends_with("/notesSlide"));
    relationship
        .map(|relationship| {
            resolve_ooxml_target(slide_part, &relationship.target).ok_or_else(|| {
                BackendError::new(
                    "IMPORT_FILE_PARSE_FAILED",
                    "A PPTX notes relationship contains an unsafe target.",
                    true,
                    true,
                )
            })
        })
        .transpose()
}

fn read_ooxml_relationships(xml: &str) -> Result<Vec<OoxmlRelationship>, BackendError> {
    let mut reader = Reader::from_str(xml);
    let mut relationships = Vec::new();
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if local_name(event.name().as_ref()) == b"Relationship" =>
            {
                let mut id = None;
                let mut target = None;
                let mut relationship_type = None;
                for attribute in event.attributes().flatten() {
                    let value = String::from_utf8_lossy(attribute.value.as_ref()).into_owned();
                    match local_name(attribute.key.as_ref()) {
                        b"Id" => id = Some(value),
                        b"Target" => target = Some(value),
                        b"Type" => relationship_type = Some(value),
                        _ => {}
                    }
                }
                if let (Some(id), Some(target), Some(relationship_type)) =
                    (id, target, relationship_type)
                {
                    relationships.push(OoxmlRelationship {
                        id,
                        target,
                        relationship_type,
                    });
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(relationships)
}

fn relationship_part_path(part: &str) -> String {
    let (directory, name) = part.rsplit_once('/').unwrap_or(("", part));
    if directory.is_empty() {
        format!("_rels/{name}.rels")
    } else {
        format!("{directory}/_rels/{name}.rels")
    }
}

fn resolve_ooxml_target(source_part: &str, target: &str) -> Option<String> {
    let mut components = if target.starts_with('/') {
        Vec::new()
    } else {
        source_part
            .rsplit_once('/')
            .map(|(directory, _)| directory.split('/').map(str::to_owned).collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let normalized_target = target.trim_start_matches('/').replace('\\', "/");
    for component in normalized_target.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            value
                if value.contains(':')
                    || value.chars().any(char::is_control)
                    || value == "."
                    || value == ".." =>
            {
                return None;
            }
            value => components.push(value.to_string()),
        }
    }
    (!components.is_empty()).then(|| components.join("/"))
}

fn read_required_archive_text<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String, BackendError> {
    read_optional_archive_text(archive, name)?.ok_or_else(|| {
        BackendError::new(
            "IMPORT_FILE_PARSE_FAILED",
            "A referenced Office XML part is missing.",
            true,
            true,
        )
    })
}

fn read_optional_archive_text<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Option<String>, BackendError> {
    let mut entry = match archive.by_name(name) {
        Ok(entry) => entry,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(error) => return Err(zip_read_err(error)),
    };
    ensure_entry_size(&entry)?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml).map_err(io_read_err)?;
    Ok(Some(xml))
}

fn pptx_slide_to_markdown(xml: &str) -> Result<String, BackendError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut output = String::new();
    let mut paragraph = String::new();
    let mut in_text = false;
    let mut is_bullet = false;
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"p" => {
                    paragraph.clear();
                    is_bullet = false;
                }
                b"t" => in_text = true,
                b"buChar" | b"buAutoNum" => is_bullet = true,
                _ => {}
            },
            Ok(Event::Empty(event))
                if matches!(local_name(event.name().as_ref()), b"buChar" | b"buAutoNum") =>
            {
                is_bullet = true;
            }
            Ok(Event::Text(text)) if in_text => {
                paragraph.push_str(&text.unescape().map_err(xml_err)?);
            }
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"t" => in_text = false,
                b"p" if !paragraph.trim().is_empty() => {
                    if is_bullet {
                        output.push_str("- ");
                    }
                    output.push_str(paragraph.trim());
                    output.push('\n');
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(output)
}

fn read_xlsx_text<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<String, BackendError> {
    let mut shared = Vec::new();
    if let Ok(mut entry) = archive.by_name("xl/sharedStrings.xml") {
        ensure_entry_size(&entry)?;
        let mut xml = String::new();
        entry.read_to_string(&mut xml).map_err(io_read_err)?;
        shared = read_shared_strings(&xml)?;
    }

    let sheets = workbook_sheet_parts(archive)?.unwrap_or_else(|| {
        let names = archive_names(archive);
        let mut fallback = names
            .into_iter()
            .filter(|name| name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml"))
            .map(|part| WorkbookSheetPart {
                name: format!(
                    "Sheet {}",
                    numbered_part(&part, "xl/worksheets/sheet", ".xml")
                ),
                part,
            })
            .collect::<Vec<_>>();
        fallback.sort_by_key(|sheet| numbered_part(&sheet.part, "xl/worksheets/sheet", ".xml"));
        fallback
    });

    let mut output = String::new();
    for sheet in sheets {
        let xml = read_required_archive_text(archive, &sheet.part)?;
        let rows = read_xlsx_rows(&xml, &shared)?;
        output.push_str(&format!("## {}\n\n", sheet.name.replace(['\r', '\n'], " ")));
        output.push_str(&rows_to_markdown_table(&rows));
        output.push('\n');
    }
    Ok(output)
}

struct WorkbookSheetPart {
    name: String,
    part: String,
}

fn workbook_sheet_parts<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<Option<Vec<WorkbookSheetPart>>, BackendError> {
    let Some(workbook_xml) = read_optional_archive_text(archive, "xl/workbook.xml")? else {
        return Ok(None);
    };
    let Some(relationships_xml) =
        read_optional_archive_text(archive, "xl/_rels/workbook.xml.rels")?
    else {
        return Ok(None);
    };
    let relationships = read_ooxml_relationships(&relationships_xml)?
        .into_iter()
        .filter(|relationship| relationship.relationship_type.ends_with("/worksheet"))
        .map(|relationship| (relationship.id, relationship.target))
        .collect::<BTreeMap<_, _>>();
    let mut reader = Reader::from_str(&workbook_xml);
    let mut sheets = Vec::new();
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if local_name(event.name().as_ref()) == b"sheet" =>
            {
                let mut name = None;
                let mut relationship_id = None;
                for attribute in event.attributes().flatten() {
                    let value = String::from_utf8_lossy(attribute.value.as_ref()).into_owned();
                    match local_name(attribute.key.as_ref()) {
                        b"name" => name = Some(value),
                        b"id" => relationship_id = Some(value),
                        _ => {}
                    }
                }
                let (Some(name), Some(relationship_id)) = (name, relationship_id) else {
                    return Err(xlsx_parse_error(
                        "An XLSX workbook sheet declaration is incomplete.",
                    ));
                };
                let target = relationships.get(&relationship_id).ok_or_else(|| {
                    xlsx_parse_error("An XLSX workbook sheet relationship could not be resolved.")
                })?;
                let part = resolve_ooxml_target("xl/workbook.xml", target).ok_or_else(|| {
                    xlsx_parse_error("An XLSX workbook contains an unsafe worksheet target.")
                })?;
                sheets.push(WorkbookSheetPart { name, part });
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(Some(sheets))
}

fn read_shared_strings(xml: &str) -> Result<Vec<String>, BackendError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_si = false;
    let mut in_text = false;
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"si" => {
                    in_si = true;
                    current.clear();
                }
                b"t" if in_si => in_text = true,
                _ => {}
            },
            Ok(Event::Text(text)) if in_text => {
                current.push_str(&text.unescape().map_err(xml_err)?);
            }
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"t" => in_text = false,
                b"si" => {
                    strings.push(std::mem::take(&mut current));
                    in_si = false;
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(strings)
}

fn read_xlsx_rows(xml: &str, shared: &[String]) -> Result<Vec<Vec<String>>, BackendError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell_column = 0usize;
    let mut cell_type = String::new();
    let mut cell_reference = String::new();
    let mut cell_value = String::new();
    let mut cell_has_value = false;
    let mut cell_formula = String::new();
    let mut cell_has_formula = false;
    let mut cell_shared_formula = None::<String>;
    let mut shared_formulas = BTreeMap::<String, (String, String)>::new();
    let mut in_value = false;
    let mut in_formula = false;
    let mut in_inline_text = false;
    let mut event_buf = Vec::new();
    loop {
        match reader.read_event_into(&mut event_buf) {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"row" => row.clear(),
                b"c" => {
                    cell_type.clear();
                    cell_reference.clear();
                    cell_value.clear();
                    cell_has_value = false;
                    cell_formula.clear();
                    cell_has_formula = false;
                    cell_shared_formula = None;
                    cell_column = row.len();
                    for attribute in event.attributes().flatten() {
                        match local_name(attribute.key.as_ref()) {
                            b"r" => {
                                cell_reference =
                                    String::from_utf8_lossy(attribute.value.as_ref()).into_owned();
                                cell_column = cell_column_index(&cell_reference)?;
                            }
                            b"t" => {
                                cell_type =
                                    String::from_utf8_lossy(attribute.value.as_ref()).into_owned();
                            }
                            _ => {}
                        }
                    }
                }
                b"v" => {
                    cell_has_value = true;
                    in_value = true;
                }
                b"f" => {
                    cell_has_formula = true;
                    cell_shared_formula = shared_formula_index(&event)?;
                    in_formula = true;
                }
                b"t" => in_inline_text = true,
                _ => {}
            },
            Ok(Event::Empty(event)) => match local_name(event.name().as_ref()) {
                b"f" => {
                    cell_has_formula = true;
                    cell_shared_formula = shared_formula_index(&event)?;
                }
                b"v" => cell_has_value = true,
                _ => {}
            },
            Ok(Event::Text(text)) if in_value || in_formula || in_inline_text => {
                let text = text.unescape().map_err(xml_err)?;
                if in_formula {
                    cell_formula.push_str(&text);
                } else {
                    cell_value.push_str(&text);
                }
            }
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"v" => in_value = false,
                b"f" => in_formula = false,
                b"t" => in_inline_text = false,
                b"c" => {
                    row.resize(cell_column + 1, String::new());
                    let value = if cell_type == "s" {
                        let index = cell_value.trim().parse::<usize>().map_err(|_| {
                            BackendError::new(
                                "IMPORT_FILE_PARSE_FAILED",
                                "An XLSX shared-string cell contains an invalid index.",
                                true,
                                true,
                            )
                        })?;
                        shared.get(index).cloned().ok_or_else(|| {
                            BackendError::new(
                                "IMPORT_FILE_PARSE_FAILED",
                                "An XLSX shared-string index is out of range.",
                                true,
                                true,
                            )
                        })?
                    } else {
                        cell_value.clone()
                    };
                    let formula = match cell_shared_formula.as_deref() {
                        Some(index) if !cell_formula.trim().is_empty() => {
                            if cell_reference.is_empty() {
                                return Err(xlsx_parse_error(
                                    "An XLSX shared-formula anchor has no cell reference.",
                                ));
                            }
                            let formula = cell_formula.trim().to_string();
                            shared_formulas.insert(
                                index.to_string(),
                                (cell_reference.clone(), formula.clone()),
                            );
                            Some(formula)
                        }
                        Some(index) => {
                            let (anchor, formula) =
                                shared_formulas.get(index).ok_or_else(|| {
                                    xlsx_parse_error(
                                        "An XLSX shared-formula follower has no preceding anchor.",
                                    )
                                })?;
                            if cell_reference.is_empty() {
                                return Err(xlsx_parse_error(
                                    "An XLSX shared-formula follower has no cell reference.",
                                ));
                            }
                            Some(translate_shared_formula(formula, anchor, &cell_reference)?)
                        }
                        None if cell_formula.trim().is_empty() && !cell_has_formula => None,
                        None if cell_formula.trim().is_empty() => {
                            return Err(xlsx_parse_error(
                                "An XLSX formula cell has no recoverable formula text.",
                            ));
                        }
                        None => Some(cell_formula.trim().to_string()),
                    };
                    row[cell_column] = if let Some(formula) = formula {
                        if !cell_has_value {
                            return Err(xlsx_parse_error(
                                "An XLSX formula cell has no cached display value.",
                            ));
                        }
                        markdown_table_cell(&format!("`={}` → {}", formula, value))
                    } else {
                        markdown_table_cell(&value)
                    };
                }
                b"row" => rows.push(std::mem::take(&mut row)),
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(error) => return Err(xml_err(error)),
            _ => {}
        }
        event_buf.clear();
    }
    Ok(rows)
}

fn shared_formula_index(event: &BytesStart<'_>) -> Result<Option<String>, BackendError> {
    let mut is_shared = false;
    let mut index = None;
    for attribute in event.attributes().flatten() {
        let value = String::from_utf8_lossy(attribute.value.as_ref()).into_owned();
        match local_name(attribute.key.as_ref()) {
            b"t" => is_shared = value == "shared",
            b"si" => index = Some(value),
            _ => {}
        }
    }
    if is_shared {
        index
            .map(Some)
            .ok_or_else(|| xlsx_parse_error("An XLSX shared formula has no shared index."))
    } else {
        Ok(None)
    }
}

fn translate_shared_formula(
    formula: &str,
    anchor: &str,
    follower: &str,
) -> Result<String, BackendError> {
    let (anchor_column, anchor_row) = cell_coordinates(anchor)?;
    let (follower_column, follower_row) = cell_coordinates(follower)?;
    let column_delta = follower_column - anchor_column;
    let row_delta = follower_row - anchor_row;
    let bytes = formula.as_bytes();
    let mut output = String::with_capacity(formula.len());
    let mut index = 0usize;
    let mut in_string = false;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            output.push('"');
            if in_string && bytes.get(index + 1) == Some(&b'"') {
                output.push('"');
                index += 2;
                continue;
            }
            in_string = !in_string;
            index += 1;
            continue;
        }
        if in_string || (!bytes[index].is_ascii_alphabetic() && bytes[index] != b'$') {
            let character = formula[index..]
                .chars()
                .next()
                .ok_or_else(|| xlsx_parse_error("An XLSX formula contains invalid text."))?;
            output.push(character);
            index += character.len_utf8();
            continue;
        }
        let start = index;
        let absolute_column = bytes[index] == b'$';
        if absolute_column {
            index += 1;
        }
        let column_start = index;
        while index < bytes.len() && bytes[index].is_ascii_alphabetic() {
            index += 1;
        }
        let column_end = index;
        let absolute_row = bytes.get(index) == Some(&b'$');
        if absolute_row {
            index += 1;
        }
        let row_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if column_start == column_end
            || row_start == index
            || start
                .checked_sub(1)
                .and_then(|prior| bytes.get(prior))
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
            || bytes
                .get(index)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'('))
        {
            output.push_str(&formula[start..index]);
            continue;
        }
        let Some(column) = parse_excel_column(&formula[column_start..column_end]) else {
            output.push_str(&formula[start..index]);
            continue;
        };
        let Some(row) = formula[row_start..index].parse::<i64>().ok() else {
            output.push_str(&formula[start..index]);
            continue;
        };
        let translated_column = if absolute_column {
            column
        } else {
            column + column_delta
        };
        let translated_row = if absolute_row { row } else { row + row_delta };
        if !(1..=16_384).contains(&translated_column) || !(1..=1_048_576).contains(&translated_row)
        {
            return Err(xlsx_parse_error(
                "An XLSX shared formula translates outside worksheet bounds.",
            ));
        }
        if absolute_column {
            output.push('$');
        }
        output.push_str(&excel_column_name(translated_column));
        if absolute_row {
            output.push('$');
        }
        output.push_str(&translated_row.to_string());
    }
    Ok(output)
}

fn cell_coordinates(reference: &str) -> Result<(i64, i64), BackendError> {
    let bytes = reference.as_bytes();
    let column_end = bytes
        .iter()
        .position(|byte| !byte.is_ascii_alphabetic())
        .unwrap_or(bytes.len());
    let column = parse_excel_column(&reference[..column_end])
        .ok_or_else(|| xlsx_parse_error("An XLSX cell reference is invalid."))?;
    let row = reference[column_end..]
        .parse::<i64>()
        .ok()
        .filter(|row| (1..=1_048_576).contains(row))
        .ok_or_else(|| xlsx_parse_error("An XLSX cell reference is invalid."))?;
    Ok((column, row))
}

fn parse_excel_column(value: &str) -> Option<i64> {
    if value.is_empty() {
        return None;
    }
    let column = value.bytes().try_fold(0_i64, |total, byte| {
        byte.is_ascii_alphabetic()
            .then(|| total * 26 + i64::from(byte.to_ascii_uppercase() - b'A' + 1))
    })?;
    (1..=16_384).contains(&column).then_some(column)
}

fn excel_column_name(mut column: i64) -> String {
    let mut reversed = Vec::new();
    while column > 0 {
        column -= 1;
        reversed.push((b'A' + (column % 26) as u8) as char);
        column /= 26;
    }
    reversed.into_iter().rev().collect()
}

fn xlsx_parse_error(message: &str) -> BackendError {
    BackendError::new("IMPORT_FILE_PARSE_FAILED", message, true, true)
}

fn cell_column_index(reference: &str) -> Result<usize, BackendError> {
    let letters = reference
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .collect::<Vec<_>>();
    let column = letters.iter().try_fold(0usize, |value, byte| {
        value
            .checked_mul(26)
            .and_then(|value| value.checked_add(usize::from(byte.to_ascii_uppercase() - b'A' + 1)))
    });
    match column {
        Some(1..=16_384) if !letters.is_empty() => Ok(column.unwrap() - 1),
        _ => Err(BackendError::new(
            "IMPORT_FILE_PARSE_FAILED",
            "An XLSX cell reference exceeds Excel column limits.",
            true,
            true,
        )),
    }
}

fn rows_to_markdown_table(rows: &[Vec<String>]) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 {
        return String::new();
    }
    let mut normalized = rows.to_vec();
    for row in &mut normalized {
        row.resize(width, String::new());
    }
    let header = if normalized.is_empty() {
        (1..=width).map(|index| format!("Column {index}")).collect()
    } else {
        normalized.remove(0)
    };
    let mut markdown = String::new();
    push_markdown_row(&mut markdown, &header);
    push_markdown_row(&mut markdown, &vec!["---".to_string(); width]);
    for row in normalized {
        push_markdown_row(&mut markdown, &row);
    }
    markdown
}

fn push_markdown_row(output: &mut String, row: &[String]) {
    output.push_str("| ");
    output.push_str(&row.join(" | "));
    output.push_str(" |\n");
}

fn markdown_table_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
        .trim()
        .to_string()
}

fn validate_archive_limits<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<(), BackendError> {
    if archive.len() > MAX_OOXML_ENTRIES {
        return Err(resource_limit(
            "The Office archive contains too many entries.",
        ));
    }
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(zip_read_err)?;
        ensure_entry_size(&entry)?;
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| resource_limit("The Office archive size overflowed the limit."))?;
        if total > MAX_OOXML_TOTAL_BYTES {
            return Err(resource_limit(
                "The Office archive expands beyond the 64 MiB limit.",
            ));
        }
    }
    Ok(())
}

fn ensure_entry_size(entry: &zip::read::ZipFile<'_>) -> Result<(), BackendError> {
    if entry.size() > MAX_OOXML_ENTRY_BYTES {
        return Err(resource_limit(
            "An Office XML part is too large to extract safely.",
        ));
    }
    Ok(())
}

fn archive_names<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Vec<String> {
    (0..archive.len())
        .filter_map(|index| {
            archive
                .by_index(index)
                .ok()
                .map(|entry| entry.name().to_string())
        })
        .collect()
}

fn numbered_part(name: &str, prefix: &str, suffix: &str) -> u32 {
    name.strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(suffix))
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn io_read_err(error: std::io::Error) -> BackendError {
    BackendError::new("IMPORT_FILE_PARSE_FAILED", error.to_string(), true, true)
}

fn zip_read_err(error: zip::result::ZipError) -> BackendError {
    BackendError::new("IMPORT_FILE_PARSE_FAILED", error.to_string(), true, true)
}

fn xml_err(error: quick_xml::Error) -> BackendError {
    BackendError::new("IMPORT_FILE_PARSE_FAILED", error.to_string(), true, true)
}

fn resource_limit(message: &'static str) -> BackendError {
    BackendError::new("IMPORT_FILE_RESOURCE_LIMIT", message, true, true)
}

#[cfg(test)]
mod tests {
    use super::{extract_ooxml_markdown_from_bytes, read_xlsx_rows};
    use std::io::{Cursor, Write};

    #[test]
    fn xlsx_workbook_relationships_preserve_unicode_order_and_shared_formulas() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, xml) in [
            (
                "xl/workbook.xml",
                r#"<workbook xmlns:r="r"><sheets><sheet name="数据总览" sheetId="7" r:id="rIdB"/><sheet name="第二页" sheetId="3" r:id="rIdA"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rIdA" Type="x/worksheet" Target="worksheets/sheet3.xml"/><Relationship Id="rIdB" Type="x/worksheet" Target="worksheets/sheet7.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet7.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Input</t></is></c><c r="B1" t="inlineStr"><is><t>Formula</t></is></c></row><row r="2"><c r="A2"><v>5</v></c><c r="B2"><f t="shared" si="0" ref="B2:B3">A2*2</f><v>10</v></c></row><row r="3"><c r="A3"><v>7</v></c><c r="B3"><f t="shared" si="0"/><v>14</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet3.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Second logical sheet</t></is></c></row></sheetData></worksheet>"#,
            ),
        ] {
            archive.start_file(name, options).unwrap();
            archive.write_all(xml.as_bytes()).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();

        let markdown = extract_ooxml_markdown_from_bytes("xlsx", &bytes).unwrap();

        let first = markdown.find("## 数据总览").unwrap();
        let second = markdown.find("## 第二页").unwrap();
        assert!(first < second, "{markdown}");
        assert!(markdown.contains("`=A2*2` → 10"), "{markdown}");
        assert!(markdown.contains("`=A3*2` → 14"), "{markdown}");
        assert!(markdown.contains("Second logical sheet"), "{markdown}");
    }

    #[test]
    fn xlsx_formula_evidence_fails_closed_without_anchor_or_cached_value() {
        let missing_anchor = r#"<worksheet><sheetData><row r="2"><c r="B2"><f t="shared" si="0"/><v>4</v></c></row></sheetData></worksheet>"#;
        let missing_value = r#"<worksheet><sheetData><row r="1"><c r="B1"><f>2*2</f></c></row></sheetData></worksheet>"#;

        let anchor_error = read_xlsx_rows(missing_anchor, &[]).unwrap_err();
        let value_error = read_xlsx_rows(missing_value, &[]).unwrap_err();

        assert_eq!(anchor_error.code, "IMPORT_FILE_PARSE_FAILED");
        assert!(anchor_error.message.contains("no preceding anchor"));
        assert_eq!(value_error.code, "IMPORT_FILE_PARSE_FAILED");
        assert!(value_error.message.contains("no cached display value"));
    }

    #[test]
    fn pptx_presentation_relationships_define_slide_order_and_notes_binding() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, xml) in [
            (
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="700" r:id="rIdB"/><p:sldId id="300" r:id="rIdA"/></p:sldIdLst></p:presentation>"#,
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships><Relationship Id="rIdA" Type="x/slide" Target="slides/slide3.xml"/><Relationship Id="rIdB" Type="x/slide" Target="slides/slide7.xml"/></Relationships>"#,
            ),
            (
                "ppt/slides/slide3.xml",
                r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>Second logical slide</a:t></a:r></a:p></p:sld>"#,
            ),
            (
                "ppt/slides/slide7.xml",
                r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>First logical slide</a:t></a:r></a:p></p:sld>"#,
            ),
            (
                "ppt/slides/_rels/slide3.xml.rels",
                r#"<Relationships><Relationship Id="n9" Type="x/notesSlide" Target="../notesSlides/notesSlide9.xml"/></Relationships>"#,
            ),
            (
                "ppt/slides/_rels/slide7.xml.rels",
                r#"<Relationships><Relationship Id="n4" Type="x/notesSlide" Target="../notesSlides/notesSlide4.xml"/></Relationships>"#,
            ),
            (
                "ppt/notesSlides/notesSlide4.xml",
                r#"<p:notes xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>Notes bound to first</a:t></a:r></a:p></p:notes>"#,
            ),
            (
                "ppt/notesSlides/notesSlide9.xml",
                r#"<p:notes xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>Notes bound to second</a:t></a:r></a:p></p:notes>"#,
            ),
        ] {
            archive.start_file(name, options).unwrap();
            archive.write_all(xml.as_bytes()).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();

        let markdown = extract_ooxml_markdown_from_bytes("pptx", &bytes).unwrap();
        let first = markdown.find("First logical slide").unwrap();
        let first_notes = markdown.find("Notes bound to first").unwrap();
        let second = markdown.find("Second logical slide").unwrap();
        let second_notes = markdown.find("Notes bound to second").unwrap();
        assert!(
            first < first_notes && first_notes < second && second < second_notes,
            "presentation order and slide-to-notes relationships must win over part numbering:\n{markdown}"
        );
        assert!(markdown.contains("> Speaker notes (slide 1): Notes bound to first"));
        assert!(markdown.contains("> Speaker notes (slide 2): Notes bound to second"));
    }

    #[test]
    fn pptx_missing_slide_relationship_fails_closed_without_partial_candidate() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, xml) in [
            (
                "ppt/presentation.xml",
                r#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rIdA"/><p:sldId id="257" r:id="rIdMissing"/></p:sldIdLst></p:presentation>"#,
            ),
            (
                "ppt/_rels/presentation.xml.rels",
                r#"<Relationships><Relationship Id="rIdA" Type="x/slide" Target="slides/slide3.xml"/></Relationships>"#,
            ),
            (
                "ppt/slides/slide3.xml",
                r#"<p:sld xmlns:p="p" xmlns:a="a"><a:p><a:r><a:t>Partial content must not escape</a:t></a:r></a:p></p:sld>"#,
            ),
        ] {
            archive.start_file(name, options).unwrap();
            archive.write_all(xml.as_bytes()).unwrap();
        }
        let bytes = archive.finish().unwrap().into_inner();

        let error = extract_ooxml_markdown_from_bytes("pptx", &bytes).unwrap_err();
        assert_eq!(error.code, "IMPORT_FILE_PARSE_FAILED");
        assert!(error.message.contains("no matching slide relationship"));
    }
}
