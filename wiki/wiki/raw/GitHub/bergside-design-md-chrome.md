---
title: "TypeUI DESIGN.md Extractor (Chrome Extension)"
repo: "bergside/design-md-chrome"
url: "https://github.com/bergside/design-md-chrome"
homepage: "https://chromewebstore.google.com/detail/designmd-style-extractor/ogpdnchdjiibhobphelbbkemnnemkfma"
author: "bergside"
stars: 1227
forks: 168
language: "JavaScript"
license: "MIT"
topics: ["ai", "chrome", "chrome-extension", "claude", "claude-design", "codex", "design-md", "design-skills", "extension", "gemini", "open-source", "skills", "skills-ai", "typeui"]
created: 2026-04-14
fetched: 2026-04-27
---

# TypeUI DESIGN.md Extractor (Chrome Extension)

> Chrome extension to extract styles from any website and generate DESIGN.md files and design skills for AI based on TypeUI.

This Chrome extension extract styles and information from any given site and generates a `DESIGN.md` or `SKILL.md` file that you can use with tools such as Google Stitch, Claude Code, Codex, and others to build websites with a given design system blueprint. The file is based on the open-source [TypeUI DESIGN.md](https://www.typeui.sh/design-md) format.

## Getting started

Load the extension in Chrome:

1. Open `chrome://extensions`
2. Enable **Developer mode**
3. Click **Load unpacked**
4. Select this project folder

## Curated design skills

Check out curated design systems at [typeui.sh/design-skills](https://www.typeui.sh/design-skills).

## Available actions

| Action | Description |
| --- | --- |
| Auto-extract | Reads styles from the active tab (typography, colors, spacing, radius, shadows, motion). |
| Generate `DESIGN.md` | Produces design-system documentation markdown from extracted signals. |
| Generate `SKILL.md` | Produces agent-ready skill markdown from extracted signals. |
| Refresh | Re-runs extraction for the current page state. |
| Download | Saves generated output as `DESIGN.md` or `SKILL.md`. |
| Explain (`?`) | Shows how the file was generated, with TypeUI reference. |

## Generated file structure

| Section | What it does |
| --- | --- |
| `Mission` | Defines the design-system objective for the extracted site. |
| `Brand` | Captures product/brand context, URL, audience, and product surface. |
| `Style Foundations` | Lists inferred visual tokens and foundations. |
| `Accessibility` | Applies WCAG 2.2 AA requirements and interaction constraints. |
| `Writing Tone` | Sets guidance tone for implementation-ready output. |
| `Rules: Do` | Lists required implementation practices. |
| `Rules: Don't` | Lists anti-patterns and prohibited behavior. |
| `Guideline Authoring Workflow` | Defines ordered guideline authoring steps. |
| `Required Output Structure` | Enforces consistent output sections. |
| `Component Rule Expectations` | Defines required interaction/state details. |
| `Quality Gates` | Adds testable quality and consistency checks. |

## Local development

Run tests locally:

```bash
node tests/run-tests.mjs
```

## License

This project is open-source under the MIT License.
