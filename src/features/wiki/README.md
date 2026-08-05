# Wiki Feature

Markdown file tree, reading surface, editor shell, metadata, citations, and
related pages live here. Page roots and capabilities come from backend project
layout/access DTOs so native, compatible, restricted, read-only, and recovery
projects share this surface. React must not infer writability from a path or
assume every project has root `purpose.md`, `schema.md`, or a native `wiki/`.

Committed Sources are read inside this Wiki/Reader surface; there is no separate
top-level Sources app. Any readable Source or Wiki Markdown page is enough to
enter the reader. Generate Content actions carry the current page into the
shared Workflows preparation flow; this feature does not launch a separate
Skill/export dialog.
