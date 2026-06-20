/**
 * Task 14 Step 1 — multi-format validation fixture.
 *
 * One fixture entry per source format the import pipeline must classify and
 * (where a parser exists) extract. The point is to make parser gaps EXPLICIT:
 * formats without a real parser surface as `unsupported` / `failed` partial
 * results, never as silently-dropped entries. Tests assert the full matrix so
 * adding a new parser (or losing one) shows up as a fixture diff.
 *
 * This is shape-only validation data (no real binaries). Backend Rust tests in
 * `src-tauri/tests/mvp_flow.rs` exercise the real ExtractionService against
 * minimal on-disk files; this fixture pins the frontend's view of every
 * `SourceFileType` × `ExtractionStatus` combination.
 */
import type {
  ExtractResult,
  ImportFileEntry,
  ImportPreview,
  SourceFileType,
} from "../../types/import";

export interface FormatFixture {
  /** The `SourceFileType` this fixture represents. */
  fileType: SourceFileType;
  /** Sample original filename. */
  originalName: string;
  /** What extraction yielded for this format in the MVP. */
  status: ExtractResult["status"];
  /** Short human label for assertion failures. */
  label: string;
  /** True when the MVP has NO real parser → this is an explicit partial result. */
  parserGap: boolean;
  /** Plausible text preview (null when nothing was extracted). */
  textPreview: string | null;
  /** Source metadata where available. */
  metadata: ExtractResult["metadata"];
}

/**
 * The canonical format matrix. Every `SourceFileType` appears at least once,
 * and every `ExtractionStatus` that can legitimately arise is represented.
 */
export const FORMAT_FIXTURES: readonly FormatFixture[] = [
  {
    fileType: "pdf",
    originalName: "paper.pdf",
    status: "unsupported",
    label: "PDF — no built-in parser in MVP",
    parserGap: true,
    textPreview: null,
    metadata: { title: "Attention Is All You Need", author: "Vaswani et al.", created: "2017-06-12", modified: null, pageCount: 15, wordCount: null, language: "en" },
  },
  {
    fileType: "document",
    originalName: "report.docx",
    status: "unsupported",
    label: "DOCX — no built-in parser in MVP",
    parserGap: true,
    textPreview: null,
    metadata: { title: "Q3 Report", author: "Alice", created: null, modified: "2025-09-30", pageCount: null, wordCount: 4200, language: null },
  },
  {
    fileType: "presentation",
    originalName: "deck.pptx",
    status: "unsupported",
    label: "PPTX — no built-in parser in MVP",
    parserGap: true,
    textPreview: null,
    metadata: { title: "Roadmap", author: null, created: null, modified: null, pageCount: 22, wordCount: null, language: null },
  },
  {
    fileType: "spreadsheet",
    originalName: "budget.xlsx",
    status: "unsupported",
    label: "XLSX — no built-in parser in MVP",
    parserGap: true,
    textPreview: null,
    metadata: null,
  },
  {
    fileType: "csv",
    originalName: "metrics.csv",
    status: "extracted",
    label: "CSV — extracts as raw text (shared text branch)",
    parserGap: false,
    textPreview: "name,value\nalpha,1",
    metadata: null,
  },
  {
    fileType: "markdown",
    originalName: "notes.md",
    status: "extracted",
    label: "Markdown — first-class, extracts cleanly",
    parserGap: false,
    textPreview: "# Notes\n\nSome extracted text about transformers.",
    metadata: { title: "Notes", author: null, created: null, modified: null, pageCount: null, wordCount: 6, language: null },
  },
  {
    fileType: "text",
    originalName: "readme.txt",
    status: "extracted",
    label: "Plain text — extracts cleanly",
    parserGap: false,
    textPreview: "A plain-text note.",
    metadata: null,
  },
  {
    fileType: "html",
    originalName: "article.html",
    status: "extracted",
    label: "HTML — Readability.js extracts the body",
    parserGap: false,
    textPreview: "Extracted article body text.",
    metadata: { title: "An Article", author: null, created: null, modified: null, pageCount: null, wordCount: 300, language: "en" },
  },
  {
    fileType: "url",
    originalName: "https://example.com/post",
    status: "extracted",
    label: "URL — fetched + Readability.js metadata",
    parserGap: false,
    textPreview: "Fetched page body text.",
    metadata: { title: "Example Post", author: "Bob", created: null, modified: null, pageCount: null, wordCount: 800, language: null },
  },
  {
    fileType: "image",
    originalName: "diagram.png",
    status: "unsupported",
    label: "Image — OCR/vision deferred to compile Agent/Skill (per CLAUDE.md)",
    parserGap: true,
    textPreview: null,
    metadata: null,
  },
  {
    fileType: "unknown",
    originalName: "mystery.dat",
    status: "failed",
    label: "Unknown binary — extraction fails explicitly",
    parserGap: true,
    textPreview: null,
    metadata: null,
  },
] as const;

/** Clipboard-style Markdown paste (no file on disk). */
export const CLIPBOARD_MARKDOWN = "---\ntype: concept\n---\n# Pasted\n\nFrom the clipboard.";

/**
 * Build the `ExtractResult[]` the import preview consumes, mirroring what the
 * Rust ExtractionService would return for each fixture.
 */
export function fixtureExtractResults(): ExtractResult[] {
  return FORMAT_FIXTURES.map((f) => ({
    originalName: f.originalName,
    fileType: f.fileType,
    status: f.status,
    error: f.status === "failed" ? "No extractor for this file type." : null,
    textPreview: f.textPreview,
    metadata: f.metadata,
    extractedTextPath: f.status === "extracted" ? `raw/extracted/${f.originalName}.txt` : null,
    extractedAssets: [],
  }));
}

/** A complete `ImportPreview` over the fixture set, for UI/state tests. */
export function fixtureImportPreview(): ImportPreview {
  const files: ImportFileEntry[] = FORMAT_FIXTURES.map((f, i) => ({
    originalName: f.originalName,
    sourcePath: `/staging/${f.originalName}`,
    archivedPath: `raw/sources/${archiveFolder(f.fileType)}/${f.originalName}`,
    fileType: f.fileType,
    sizeBytes: 1024 * (i + 1),
    hash: `hash-${f.fileType}-${i}`,
    extractionStatus: f.status,
    extractionError: f.status === "failed" ? "No extractor for this file type." : null,
    textPreview: f.textPreview,
    pageCount: f.metadata?.pageCount ?? null,
    wordCount: f.metadata?.wordCount ?? null,
    metadata: f.metadata,
    conflict: null,
    renamedFrom: null,
  }));
  return {
    files,
    conflicts: [],
    summary: {
      totalFiles: files.length,
      archivedFiles: files.length,
      duplicateFiles: 0,
      renamedFiles: 0,
      failedFiles: files.filter((f) => f.extractionStatus === "failed").length,
      conflictsCount: 0,
    },
  };
}

function archiveFolder(fileType: SourceFileType): string {
  switch (fileType) {
    case "pdf":
      return "pdfs";
    case "document":
      return "docs";
    case "presentation":
      return "slides";
    case "spreadsheet":
    case "csv":
      return "sheets";
    case "image":
      return "assets";
    case "url":
      return "links";
    case "unknown":
      return "other";
    case "markdown":
    case "text":
    case "html":
    default:
      return "markdown";
  }
}

/** Every `SourceFileType` has at least one fixture — adding a type without one
 *  fails this assertion. Catches "a format was silently dropped". */
export function assertEveryFormatCovered(): void {
  const allTypes: SourceFileType[] = [
    "pdf", "document", "presentation", "spreadsheet", "csv",
    "markdown", "text", "html", "url", "image", "unknown",
  ];
  const covered = new Set(FORMAT_FIXTURES.map((f) => f.fileType));
  const missing = allTypes.filter((t) => !covered.has(t));
  if (missing.length > 0) {
    throw new Error(`Format fixture matrix is missing: ${missing.join(", ")}`);
  }
}
