---
name: html-beautiful-read
description: Render a single wiki page as a standalone, long-form readable HTML article.
---

# HTML Beautiful Read

Turn one Markdown wiki page into a single self-contained HTML document optimized for long-form reading.

- Work only from the source page supplied in the prompt; do not modify any project files.
- Emit a complete standalone HTML document: one `<!doctype html>` with a single `<style>` block inlining all CSS.
- Do NOT load external stylesheets, web fonts, images, or scripts. Inline SVG only.
- Preserve the page's meaning: heading hierarchy, lists, blockquotes, tables, inline code, and fenced code blocks all rendered.
- Reading-first styling: serif body, comfortable line length (~70ch), generous line-height, clear heading scale, styled blockquotes and code.
- Include a small header with the page title and its source path as a subtitle.
- You may wrap the entire document in a fenced ```html block. Any prose outside the document is discarded.
- Output the document only.
