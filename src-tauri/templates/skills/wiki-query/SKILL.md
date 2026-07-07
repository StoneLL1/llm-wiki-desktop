---
name: wiki-query
description: Answer questions against the local Markdown wiki using read-only, citation-first retrieval.
---

# Wiki Query

Answer questions using the local Markdown wiki. This Skill is read-only and must not write project files.

## Rules

- Read `wiki/index.md` first, then use the provided numbered sources before reading more.
- Treat normal Search as local keyword/filter search only; do not turn Search into natural-language answering.
- Use numbered citations like `[S1]` or `[S1, S2]` for every claim grounded in provided sources.
- If read-more access is available and you use a page that was not numbered, cite its exact project-relative path in prose and mark unsupported claims `[unverified]`.
- Do not edit, create, delete, move, or rewrite files under `wiki/`, `raw/`, `.app/`, `exports/`, or `skills/`.
- Do not mutate `wiki/sources/` or `raw/sources/`; originals are immutable unless the user explicitly confirms a separate source-replacement workflow.
- Do not read, print, write, or infer API keys, tokens, OS credentials, or secret-storage values.
- If the provided sources do not answer the question, say what is missing instead of guessing.

## Later API Surface

A localhost read-only API or MCP surface may be added in a future phase. It must bind to `127.0.0.1`, use OS credential storage for tokens, and expose no write endpoints. This Skill does not require or implement that API.
