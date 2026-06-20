---
name: html-project-report
description: Render a whole-wiki project report as a single standalone HTML document.
---

# HTML Project Report

Turn the whole wiki into a single self-contained HTML project report.

- Work only from the page list and purpose supplied in the prompt; do not modify any project files.
- Emit a complete standalone HTML document: one `<!doctype html>` with a single `<style>` block inlining all CSS.
- Do NOT load external stylesheets, web fonts, images, or scripts. Inline SVG only.
- Structure the report:
  - Title and the wiki purpose.
  - An index of pages grouped by page type, each linking to its in-wiki path (shown as text, since this document is standalone).
  - A short highlights section (3–6 notable pages or themes), each with a one-line rationale.
  - A structural notes section: observations you can infer from the page list (coverage gaps, dense hubs, isolated clusters).
- Keep claims grounded in the supplied pages; do not invent pages or paths.
- You may wrap the entire document in a fenced ```html block. Any prose outside the document is discarded.
- Output the document only.
