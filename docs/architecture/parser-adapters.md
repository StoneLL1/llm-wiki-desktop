# Parser Adapter Selection

## Overview

The extraction pipeline uses a pluggable adapter model. Each supported binary format (PDF, DOCX, PPTX, XLSX) gets a Rust adapter implementing `Extractor` trait, while text-based formats (MD, TXT, CSV, HTML) use direct text extraction in `ExtractionService`.

## Current MVP Status

| Format | MVP Status | Adapter |
|--------|-----------|---------|
| MD / TXT / CSV / HTML | **Implemented** | Direct text extraction in `ExtractionService` |
| PDF | **Unsupported** | Returns `ExtractionStatus::Unsupported` with clear error; per-file failure, batch continues |
| DOCX / ODT / RTF | **Unsupported** | Returns `ExtractionStatus::Unsupported`; per-file failure, batch continues |
| PPTX / ODP | **Unsupported** | Returns `ExtractionStatus::Unsupported`; per-file failure, batch continues |
| XLSX / ODS | **Unsupported** | Returns `ExtractionStatus::Unsupported`; per-file failure, batch continues |
| Images | **No extraction** | `ExtractionStatus::Unsupported`; OCR is deferred to Agent/Skill |
| URL (article) | **Frontend adapter** | `src/lib/readability.ts` wrapping `@mozilla/readability` |

## MVP Blocking Conditions

Before any parser adapter can graduate from "unsupported" to "MVP-ready," the following must be met:

### Per-Adapter Acceptance

1. **Valid fixture extraction**: A representative valid fixture file (not empty, not truncated, common real-world use) must extract **text**, **metadata** (title, author, page count, word count), and **preview statistics**.
2. **Corrupt file handling**: A corrupt/truncated fixture must produce a **per-file failure** — `ExtractionStatus::Failed` with a descriptive `error` string — and must **not abort the batch**.
3. **Batch continuity**: A mixed batch of valid and corrupt files must produce N results, one per input; no file past the corrupt one is skipped.
4. **CJK filename support**: Fixture files with CJK characters in the filename (e.g., `研究论文.pdf`, `數據報告.xlsx`) must not cause adapter failures or garbled output paths.

### Adapter Candidates

#### PDF

| Crate | Pros | Cons |
|-------|------|------|
| `pdf-extract` | Simple API, text extraction | Limited metadata, no table extraction |
| `pdf` | Raw parsing, full control | Low-level, significant implementation effort |
| `lopdf` | Page-level access, metadata | Need separate text extraction logic |
| **Recommendation**: `pdf-extract` for initial MVP text + metadata; evaluate `lopdf` if page-level features needed. |

#### DOCX

| Crate | Pros | Cons |
|-------|------|------|
| `docx-rs` | Full DOCX spec coverage | Heavy, complex API |
| `quick-xml` + custom | Lightweight, precise control | Requires OOXML spec knowledge |
| **Recommendation**: `docx-rs` for MVP; skip OLE `.doc` in MVP, document as unsupported. |

#### PPTX

| Crate | Pros | Cons |
|-------|------|------|
| `pptx-rs` | Purpose-built for PPTX | Limited maintenance history |
| `quick-xml` + custom | Same approach as DOCX | Requires OOXML slide spec work |
| **Recommendation**: Evaluate `pptx-rs` first; fall back to `quick-xml` extraction if `pptx-rs` is unmaintained or missing slide text APIs. |

#### XLSX

| Crate | Pros | Cons |
|-------|------|------|
| `calamine` | Mature, handles XLSX/XLS/ODS | Returns cell data; text assembly is manual |
| `xlsx` | Purpose-built | Less mature |
| **Recommendation**: `calamine` for cell-level extraction; assemble cell text into a flat preview in `ExtractionService`. |

## Integration Pattern

All adapters implement:

```rust
pub trait Extractor {
    fn supports(&self, file_type: &SourceFileType) -> bool;
    fn extract(&self, source_path: &Path, output_dir: &Path) -> Result<ExtractResult, BackendError>;
}
```

The `ExtractionService` dispatches by file type. Each adapter returns `ExtractResult` with text, metadata, and preview stats. Failures are per-file and never abort batches.

## When to Activate

Adapters are activated when `ExtractionService` is updated to register and dispatch them. Until then, all binary formats return `Unsupported`. The UI shows `unsupported` status with a clear message.
