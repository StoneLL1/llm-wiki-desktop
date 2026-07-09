# Settings AI Configuration Polish Design

Date: 2026-07-09
Status: Approved for implementation

## Context

Settings is now a floating modal control surface. The current implementation has grouped settings navigation and separate Agent / LLM provider sections, but the Agent CLI and BYOK configuration experience is still visually plain and split across two pages. The user wants a combined AI configuration page modeled after the supplied reference image: a segmented switch between local CLI and BYOK, polished provider cards, official logos where available, and a broader visual cleanup for the rest of Settings.

## Goals

- Combine Agent CLI and BYOK into one settings section with a top segmented control: Local CLI / BYOK.
- Make Agent CLI choices feel like selectable product cards with logos, status, version/path details, default selection, and action controls.
- Make BYOK providers use the same card language, including brand marks, model/base URL metadata, keychain status, masked-key hints, test actions, and an expanded editor for the selected provider.
- Beautify the remaining settings sections with consistent compact headers, rows, cards, controls, and status notes.
- Stay close to the Codex desktop visual language: dense, calm, token-driven, no decorative gradients, no nested cards, no marketing layout.

## Non-Goals

- No backend behavior changes unless a frontend type/test reveals an existing integration mismatch.
- No secrets in files, logs, screenshots, or test fixtures.
- No silent Agent install actions.
- No changes to `UI-Frontend-design/`.
- No database or new persistence model.

## Design

### Information Architecture

The left Settings nav keeps three groups: Application, AI, System. The current `Agent` and `LLM providers` entries become one `AI` entry. Inside the AI section, a segmented control switches between:

- Local CLI: detected Agent CLIs and default route selection.
- BYOK: provider configuration and credential status.

This keeps the user's primary decision, local CLI vs BYOK, in one place while preserving project constraints: Agent CLI is an enhancement and BYOK remains a core route.

### Agent CLI Panel

The Local CLI tab renders detected agents as selectable cards. Each card includes:

- Logo mark for known tools: Codex CLI, Claude Code, OpenCode/OpenClaw, Hermes.
- Product name and vendor label.
- Installed/missing/failed badge.
- Version, executable path, or error/install guidance.
- Default-state badge when selected.
- Primary action to set as default when installed.
- Secondary clear/default action when applicable.

Selecting a card expands a detail area beneath the row with model/default-route notes and a test/refresh affordance. Logo assets should use reliable official web sources where available; if no safe logo is found, use a restrained CSS/letter fallback rather than inventing a brand mark.

### BYOK Panel

The BYOK tab renders provider cards for Anthropic, OpenAI, Google Gemini, Ollama, and Custom OpenAI-compatible endpoints. Each card includes:

- Provider mark/logo.
- Provider name and route description.
- Configured/unconfigured/service state badge.
- Model and base URL summary.
- Masked secret status only, never raw key text.
- Test or add/edit action.

The active provider opens an inline edit area with model, base URL, provider save, key save/delete, and test controls. The editor should read as part of the active card group, not a disconnected form.

### Remaining Settings Polish

General, Appearance, Language, Security, Background tasks, and Updates should share a more consistent settings surface:

- Compact section headers and descriptions.
- Token-based rows with label, hint, and control.
- Subtle bordered cards only for repeated choices or summaries.
- Segmented controls for mode choices.
- Small badges and monospace metadata for paths, versions, and exact settings.
- Responsive behavior that keeps Chinese and English labels fitting inside the modal.

### Styling

Use `src/styles.css` as the single style home for reusable Settings classes. Reuse or extend existing `.apikey-row`, `.cli-row`, `.seg`, `.formrow`, `.badge`, `.settings-view__*`, and appearance preset styles. Avoid adding conflicting `display` declarations to `.settings-view-layout`; the current cascade has a known gotcha.

Use CSS variables, not raw hex colors, except for small brand-logo artwork or unavoidable inline SVG logo fills.

## Data Flow

Frontend state remains local to Settings components:

- `SettingsView` owns the active section.
- The combined AI settings component owns the active tab and selected provider/agent.
- Agent default changes call the existing `onChangeDefault`.
- Provider config, secret save/delete, and test actions call existing props.
- Secret status continues to be read through `get_provider_secret_status` and provider DTOs.

No filesystem, Git, Agent process, or secret-storage logic moves into React.

## Error Handling

- Provider test status appears inline as role `status`.
- Failed provider tests show the returned message without exposing secrets.
- Missing or failed CLI rows remain visible with a clear badge and detail text.
- Disabled actions remain disabled for missing CLIs or unreachable Ollama.

## Testing

Add or update focused frontend tests for:

- BYOK secret save clears the input and never echoes secret text.
- Provider test uses the selected provider's saved endpoint and model.
- AI section exposes Local CLI / BYOK segmented tabs.
- Agent rows show brand labels and default selection controls.
- Layout-sensitive CSS contract checks for the new Settings card/row classes where useful.

Full completion still requires `npm run check`.

## Open Issue

Logo sourcing may vary by availability. If official raster assets are unavailable or licensing is unclear, use compact text/geometric fallbacks and keep the UI polished without pretending they are official marks.
