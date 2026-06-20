---
name: html-concept-map
description: Render a concept map of a page's neighbourhood (or the whole wiki) as a standalone HTML page with inline SVG.
---

# HTML Concept Map

Render a concept map as a single self-contained HTML document using inline SVG.

- Work only from the page(s) supplied in the prompt; do not modify any project files.
- Emit a complete standalone HTML document: one `<!doctype html>` with a single `<style>` block inlining all CSS, and the map drawn as inline `<svg>`.
- Do NOT load external stylesheets, web fonts, images, or scripts. The map must be static and render with zero JavaScript.
- For a single-source map: centre on that page and draw its wikilinks as 1-hop neighbour nodes, edges labelled "related".
- For a whole-wiki map: lay out the supplied pages as nodes and connect pages whose `links` reference each other. Avoid clutter — cap visible nodes and keep the layout readable.
- Each node shows the page title; edges are hairline "related" connections.
- You may wrap the entire document in a fenced ```html block. Any prose outside the document is discarded.
- Output the document only.
