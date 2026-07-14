use std::fmt::Write;

pub const EXCEL_MAX_COLUMNS: u32 = 16_384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbookOutputMode {
    SinglePage,
    OverviewAndSheets,
    Chunked { rows_per_chunk: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub formula: Option<String>,
    pub value: String,
}
impl Cell {
    pub fn value(value: impl Into<String>) -> Self {
        Self {
            formula: None,
            value: value.into(),
        }
    }
    pub fn formula(formula: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            formula: Some(formula.into()),
            value: value.into(),
        }
    }
    fn markdown(&self) -> String {
        self.formula
            .as_ref()
            .map_or_else(|| self.value.clone(), |f| format!("`{f}` → {}", self.value))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub name: String,
    pub hidden: bool,
    pub rows: Vec<Vec<Cell>>,
    pub declared_columns: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvFallback {
    pub sheet_name: String,
    pub content: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbookOutput {
    pub source_id: String,
    pub version_id: String,
    pub markdown: String,
    pub warnings: Vec<String>,
    pub csv_fallbacks: Vec<CsvFallback>,
}
pub struct WorkbookPlan {
    source_id: String,
    version_id: String,
    mode: WorkbookOutputMode,
    sheets: Vec<Sheet>,
}
impl WorkbookPlan {
    pub fn new(
        source_id: impl Into<String>,
        version_id: impl Into<String>,
        mode: WorkbookOutputMode,
        sheets: Vec<Sheet>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            version_id: version_id.into(),
            mode,
            sheets,
        }
    }
    pub fn render(&self) -> Result<WorkbookOutput, &'static str> {
        if self
            .sheets
            .iter()
            .any(|s| s.declared_columns > EXCEL_MAX_COLUMNS)
        {
            return Err("WORKBOOK_COLUMN_LIMIT");
        }
        if matches!(self.mode, WorkbookOutputMode::Chunked { rows_per_chunk: 0 }) {
            return Err("WORKBOOK_INVALID_CHUNK_SIZE");
        }
        let mut markdown = String::new();
        let mut warnings = Vec::new();
        let mut csv_fallbacks = Vec::new();
        writeln!(markdown, "# Workbook\n\n{} sheets", self.sheets.len()).unwrap();
        for sheet in &self.sheets {
            let label = if sheet.hidden {
                format!("{} (hidden)", sheet.name)
            } else {
                sheet.name.clone()
            };
            writeln!(markdown, "\n## {label}").unwrap();
            let chunk = match self.mode {
                WorkbookOutputMode::Chunked { rows_per_chunk } => rows_per_chunk as usize,
                _ => usize::MAX,
            };
            for (index, rows) in sheet.rows.chunks(chunk).enumerate() {
                if chunk != usize::MAX {
                    writeln!(
                        markdown,
                        "\n### Rows {}–{}",
                        index * chunk + 1,
                        index * chunk + rows.len()
                    )
                    .unwrap();
                }
                for row in rows {
                    writeln!(
                        markdown,
                        "| {} |",
                        row.iter()
                            .map(Cell::markdown)
                            .collect::<Vec<_>>()
                            .join(" | ")
                    )
                    .unwrap();
                }
            }
            if chunk != usize::MAX && sheet.rows.len() > chunk {
                if !warnings.iter().any(|w| w == "WORKBOOK_RANGE_CHUNKED") {
                    warnings.push("WORKBOOK_RANGE_CHUNKED".into());
                }
                let mut writer = csv::Writer::from_writer(Vec::new());
                for row in &sheet.rows {
                    writer
                        .write_record(row.iter().map(|cell| cell.value.as_str()))
                        .map_err(|_| "CSV_FALLBACK_FAILED")?;
                }
                let bytes = writer.into_inner().map_err(|_| "CSV_FALLBACK_FAILED")?;
                csv_fallbacks.push(CsvFallback {
                    sheet_name: sheet.name.clone(),
                    content: String::from_utf8(bytes).map_err(|_| "CSV_FALLBACK_FAILED")?,
                });
            }
        }
        Ok(WorkbookOutput {
            source_id: self.source_id.clone(),
            version_id: self.version_id.clone(),
            markdown,
            warnings,
            csv_fallbacks,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideImage {
    pub path: String,
    pub width_px: u32,
    pub height_px: u32,
    pub decorative: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slide {
    pub number: u32,
    pub title: String,
    pub body: Vec<String>,
    pub notes: Option<String>,
    pub images: Vec<SlideImage>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresentationOutput {
    pub source_id: String,
    pub version_id: String,
    pub candidates: Vec<String>,
    pub meaningful_images: usize,
}
pub struct PresentationPlan {
    source_id: String,
    version_id: String,
    slides: Vec<Slide>,
}
impl PresentationPlan {
    pub fn new(
        source_id: impl Into<String>,
        version_id: impl Into<String>,
        slides: Vec<Slide>,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            version_id: version_id.into(),
            slides,
        }
    }
    pub fn render(&self) -> Result<PresentationOutput, &'static str> {
        if self
            .slides
            .windows(2)
            .any(|pair| pair[0].number >= pair[1].number)
        {
            return Err("PRESENTATION_SLIDE_ORDER");
        }
        let mut markdown = String::new();
        let mut meaningful_images = 0;
        for slide in &self.slides {
            writeln!(
                markdown,
                "<a id=\"slide-{}\"></a>\n\n## Slide {} — {}",
                slide.number, slide.number, slide.title
            )
            .unwrap();
            for body in &slide.body {
                writeln!(markdown, "\n{body}").unwrap();
            }
            for image in slide
                .images
                .iter()
                .filter(|image| !image.decorative && image.width_px >= 32 && image.height_px >= 32)
            {
                meaningful_images += 1;
                writeln!(
                    markdown,
                    "\n![Slide {} image]({})",
                    slide.number, image.path
                )
                .unwrap();
            }
            if let Some(notes) = &slide.notes {
                writeln!(
                    markdown,
                    "\n> Speaker notes (slide {}): {notes}",
                    slide.number
                )
                .unwrap();
            }
        }
        Ok(PresentationOutput {
            source_id: self.source_id.clone(),
            version_id: self.version_id.clone(),
            candidates: vec![markdown],
            meaningful_images,
        })
    }
}
