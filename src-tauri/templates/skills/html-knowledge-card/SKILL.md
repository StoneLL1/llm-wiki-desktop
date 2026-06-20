---
name: html-knowledge-card
description: Render a single wiki page as a compact, self-contained HTML knowledge card.
---

# HTML Knowledge Card

Turn one Markdown wiki page into a single self-contained HTML "knowledge card" — a compact, scannable summary.

- Work only from the source page supplied in the prompt; do not modify any project files.
- Emit a complete standalone HTML document: one `<!doctype html>` with a single `<style>` block inlining all CSS.
- Do NOT load external stylesheets, web fonts, images, or scripts. Inline SVG only.
- Card contents: title, page type, tags, 3–6 distilled key facts as bullets, and a one-line source attribution (the page path).
- Stay tight: this is a card, not an article. Favor crisp phrasing; drop filler.
- You may wrap the entire document in a fenced ```html block. Any prose outside the document is discarded.
- Output the document only.
