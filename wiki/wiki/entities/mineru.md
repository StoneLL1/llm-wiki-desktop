---
title: MinerU
created: 2026-04-23
updated: 2026-05-23
type: entity
tags: [tool, data, open-source]
sources:
  - raw/articles/2026-04-18-mineru-pdf-conversion-tool.md
  - raw/articles/mineru-pdf-conversion-tool.md
---

# MinerU

## Overview

**MinerU** is an open-source PDF-to-markdown conversion tool developed by **OpenDataLab**. It transforms PDF documents into machine-readable formats (Markdown, JSON), preserving document structure while extracting text, images, tables, and mathematical formulas. It is a critical tool in the AI knowledge base construction pipeline, often used as a preprocessing step before feeding documents into LLM-based systems like [[autowiki]].

GitHub: [opendatalab/MinerU](https://github.com/opendatalab/MinerU)

## Key Features

### Document Structure Preservation

Unlike simpler tools like Microsoft's [[markitdown]], MinerU preserves the logical structure of documents:
- **Headings, paragraphs, lists** — extracted in human-reading order
- **Images** — extracted with captions and descriptions
- **Tables** — recognized and converted to HTML format, with titles and footnotes
- **Mathematical formulas** — auto-detected and converted to LaTeX format, including extra-long formulas

### Intelligent Cleanup

- **Removes redundant elements**: Headers, footers, footnotes, page numbers automatically stripped to ensure semantic coherence
- **OCR support**: Auto-detects scanned PDFs and garbled text, enabling OCR for 84 languages

### Multi-Format Output

- **Markdown** — primary output, ideal for LLM consumption
- **JSON** — structured data output for programmatic processing

### Platform Support

- Compatible with **Windows, Linux, macOS**
- Supports **CPU, GPU, NPU** acceleration
- Docker deployment available (requires ≥8GB GPU VRAM for full acceleration)

## Comparison with MarkItDown

| Feature | MinerU | MarkItDown |
|---------|--------|------------|
| Developer | OpenDataLab | Microsoft |
| Structure preservation | Yes | Limited |
| Image extraction | Yes | Basic |
| Table recognition | HTML format | Basic |
| Formula recognition | LaTeX format | No |
| OCR support | 84 languages | Limited |
| GPU acceleration | Yes | No |
| PDF layout analysis | Advanced (doclayout_yolo) | Basic |

MinerU is recommended when document structure, tables, and formulas matter — particularly for academic papers and technical documents. [[markitdown]] is simpler and faster for basic text extraction.

## Installation

### Docker (GPU)
```bash
wget https://gcore.jsdelivr.net/gh/opendatalab/MinerU@master/docker/china/Dockerfile -O Dockerfile
docker build -t mineru:latest .
docker run --rm -it --gpus=all mineru:latest /bin/bash
```

### CPU (pip)
```bash
conda create -n MinerU python=3.10
conda activate MinerU
pip install -U "magic-pdf[full]"
```

### Model Download
```bash
pip install modelscope
wget https://gcore.jsdelivr.net/gh/opendatalab/MinerU@master/scripts/download_models.py -O download_models.py
python download_models.py
```

### Configuration

The `magic-pdf.json` config file allows enabling/disabling features:
```json
{
    "layout-config": { "model": "doclayout_yolo" },
    "formula-config": { "enable": true },
    "table-config": { "enable": true }
}
```

## Online Demos

- **OpenDataLab Demo**: mineru.net
- **ModelScope Demo**: modelscope.cn
- **HuggingFace Demo**: huggingface.co/spaces/opendatalab/MinerU

## Role in Knowledge Compilation

MinerU plays a key role in the [[knowledge-compilation]] pipeline:

1. **PDF → Markdown conversion**: Raw academic papers are converted to structured Markdown
2. **Structure preservation**: Headings, formulas, tables remain intact for LLM analysis
3. **LLM ingestion**: The Markdown output feeds into systems like [[autowiki]] for wiki compilation

This makes MinerU the bridge between raw academic literature and AI-processable knowledge bases.

## Relationships

- Developed by **OpenDataLab**
- Key preprocessing tool for [[knowledge-compilation]] pipelines
- Complementary to [[autowiki]] for paper knowledge base construction
- Alternative/supplement to Microsoft's [[markitdown]]
- Part of the broader [[knowledge-compilation]] tooling ecosystem for AI

## See Also

- [[autowiki]] — paper knowledge base tool that benefits from MinerU preprocessing
- [[knowledge-compilation]] — the paradigm MinerU supports
- [[markitdown]] — Microsoft's simpler document-to-markdown tool
- [[claude-md]] — project configuration for LLM-based knowledge tools
- [[chandra|Chandra OCR]]
