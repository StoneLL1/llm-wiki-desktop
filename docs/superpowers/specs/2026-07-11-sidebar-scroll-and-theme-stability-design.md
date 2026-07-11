# Sidebar Scroll and Theme Stability Design

## Goal

Make the full upper portion of the left sidebar scroll as one region while keeping the Agent status row fixed, give both sidebar and right-context scrollbars a quieter compact treatment, and prevent appearance changes from shifting the settings content vertically.

## Root Causes

- `LeftSidebar` assigns `overflow-y-auto` only to the Recent Pages section while the sidebar itself hides overflow. On short windows, this creates a nested scrollbar that controls only the last section.
- `SettingsView` conditionally inserts the loading/saving status before the active settings section. Toggling appearance sets `saving`, so that transient row enters normal document flow and pushes all following content down until persistence completes.

## Design

### Left sidebar scrolling

Wrap Main Views, Workflow, Favorites, and Recent Pages in one shrink-safe scroll region. Remove flex growth and overflow ownership from Recent Pages. Keep the Agent status row outside that region so it remains fixed at the bottom.

Collapsed-sidebar behavior remains unchanged: section labels, Favorites, Recent Pages, navigation labels, counts, and Agent name continue to hide at the existing width threshold.

### Scrollbar appearance

Use one reusable class for the new left-sidebar scroll region and every scrollable right-context-panel body. The scrollbar track is transparent. The thumb is approximately 4px wide in Chromium/WebView, uses a very light theme-token-derived color at rest, and becomes darker when the scroll region or thumb is hovered. Firefox receives `scrollbar-width: thin` and matching token-derived colors.

Only the left sidebar and right context panel adopt this treatment; unrelated editor, dialog, and list scrollbars are not changed.

### Settings save feedback

Keep the loading/saving status visible, but position it outside normal content flow at the upper-right of the settings content container. Its appearance and disappearance must not alter the position of the active settings section. The content container provides the positioning boundary.

## Testing

- A component test verifies the four upper sidebar sections share one scroll region and the Agent row remains outside it.
- CSS contract tests verify Recent Pages no longer owns scrolling, left and right bodies use the shared scrollbar class, and the compact idle/hover scrollbar rules exist.
- A CSS contract test verifies the settings status is absolutely positioned inside a relative content boundary so save feedback cannot reflow the page.
- Run the focused tests first, then the repository-required `npm run check`.

## Scope

No navigation, persistence, theme-token values, settings APIs, or backend behavior changes. No files under `UI-Frontend-design/` are modified.
