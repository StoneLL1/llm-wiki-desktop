# Schema

> Page types and rules for a reading-notes wiki.

## Page Types

| Type | Folder | Purpose |
|---|---|---|
| entity | `wiki/entities/` | Authors, series, publishers |
| concept | `wiki/concepts/` | Recurring themes and mental models from reading |
| source | `wiki/sources/` | One book, essay, or long-form piece |
| query | `wiki/queries/` | Questions raised by the reading, with cited answers |
| synthesis | `wiki/synthesis/` | Themes spanning multiple books |
| comparison | `wiki/comparisons/` | Contrasting books on the same topic |

## Page Rules

- Each `source` page records title, author, and key takeaways.
- Use frontmatter `tags` for genre and topic.
- Quote sparingly; favor distilled notes linked by `[[wikilinks]]`.
- `wiki/synthesis/` pages cite every book they reference.
