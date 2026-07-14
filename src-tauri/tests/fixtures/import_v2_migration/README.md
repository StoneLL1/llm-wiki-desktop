# Import V2 migration fixtures

This corpus is intentionally hostile. It is test evidence, not a project to
open or mutate. A migration scanner must preserve every existing byte and
timestamp, must not follow links or reparse points, and must route uncertainty
to `legacy_unmanaged` or `conflict`.

The fixture names cover clean metadata, ambiguous identities, corrupt records,
Unicode/CJK names, Windows/POSIX separators, case-only collisions, missing
content, interrupted journals, and externally edited Markdown.
