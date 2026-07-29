# Import And Chat Reliability Design

> Historical design, partially superseded. Current source identity, deduplication, Source commit and test-project repair rules are defined by [`2026-07-24-import-source-media-flow-design.md`](2026-07-24-import-source-media-flow-design.md).

## Goal

Repair two confirmed failures without widening product behavior:

1. An external Markdown source must not be classified as a duplicate of the `raw/extracted/*.md` artifact created during its own preview.
2. A user-selected default Agent must remain the same value across the Agent panel, Settings, persisted project files, and Chat routing. Chat task failures must be visible in the conversation.

## Confirmed Causes

- `ImportService::scan_existing` scans all of `raw/`. Preview extraction runs first, so text formats can be byte-identical to their new `raw/extracted/` artifact and are then incorrectly resolved as `ExactDuplicate -> Skip`. Confirmation records no source and compile reports `COMPILE_INPUT_EMPTY`.
- Agent configuration is mirrored in `.app/settings.json` and `.app/agent-config.json`, but `set_default_agent` updates only one file and does not refresh the frontend settings store. A later full settings save can restore a stale `null` value.
- Several UI surfaces treat the first installed Agent as the default even when `AgentInfo.isDefault` is false. The backend does not use that fallback, so the display and Chat route disagree.
- Chat clears a terminal send task after reloading the session but discards `BackendTask.error`, leaving a failed request with no visible answer or explanation.

## Design

### Import duplicate boundary

Duplicate detection scans retained original-source roots only: `raw/sources/` and `raw/assets/`. Derived artifacts under `raw/extracted/` never participate in source duplicate detection. Existing duplicate behavior between two retained originals remains unchanged.

### Default Agent consistency

`.app/agent-config.json` is the canonical default-Agent value. `SettingsService::read_settings` always overlays this value when the file exists. A focused `save_agent_default` operation writes both the canonical file and the project-settings mirror in one backend call; it does not rewrite unrelated global settings.

The existing `set_default_agent` IPC command delegates to that operation. After the command succeeds, the frontend reloads the settings store before refreshing detected capabilities. UI surfaces label an Agent as default only when `isDefault` is true; an installed-but-unselected Agent remains merely available.

### Chat terminal failures

When the active send task becomes terminal, Chat reloads the targeted session first. It then clears the in-flight state while preserving the backend error message for failed tasks. The existing compact error banner displays it. Successful and cancelled flows retain their current behavior.

## Non-goals

- Do not automatically select the first installed Agent.
- Do not change Agent authentication, BYOK providers, secret storage, streaming, retrieval, citations, or task persistence.
- Do not migrate or silently modify user project data. Re-previewing the affected Markdown source after the app update repairs the existing test project through the normal confirmation flow.
- Do not change UI layout or design tokens.

## Tests

- Rust import regression: an identical `raw/extracted` artifact does not make an external Markdown source a duplicate; an identical retained source still does.
- Rust settings regression: saving a default Agent synchronizes both project files, and reads prefer the canonical Agent config if legacy files disagree.
- Frontend regression: installed-but-not-default Agents are not displayed as default.
- Frontend regression: a failed terminal Chat task reloads the session and exposes its backend error.
