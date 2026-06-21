# LLM Wiki Desktop Frontend Guidelines

> Purpose: Define the frontend design system for LLM Wiki Desktop.
> Visual reference: `DESIGN.md` plus the Codex desktop app. The target is a highly Codex-like desktop workbench, adapted to a local wiki product.
> Stack target: React 19, TypeScript, Tailwind CSS v4, shadcn/ui, Lucide React.

## 1. Design Intent

LLM Wiki Desktop should feel like a quiet local knowledge workbench: precise, readable, fast, and respectful of user files. The interface should be recognizably close to the Codex desktop app: a calm agent workspace with a compact shell, restrained surfaces, clear task state, and very little decorative styling.

The product is not a landing page, a chat-only surface, or a decorative AI dashboard. It is a multi-pane workspace for importing sources, reading and editing Markdown, running local agents, inspecting diffs, viewing graphs, and exporting knowledge artifacts.

Core qualities:

- Calm, near-monochrome UI with one restrained teal accent.
- Dense but legible information layout for repeated daily use.
- Strong spatial model: left navigation, central work area, right context panel, bottom status bar.
- Editorial typography for reading surfaces, compact sans typography for controls.
- Clear long-task feedback, explicit confirmations, and visible safety boundaries.
- No ornamental gradients, floating decoration, oversized hero sections, or card-heavy marketing composition.

### 1.1 Codex Similarity Target

When choosing between a generic shadcn dashboard pattern and a Codex-like desktop pattern, choose the Codex-like pattern.

Codex-like means:

- A compact desktop shell, not a web landing page.
- A strong left sidebar with primary workspace modes.
- A central conversation, editor, terminal, graph, or document surface that does the real work.
- A right context area for files, citations, diffs, task logs, metadata, and review controls.
- A visible bottom or lower status area for environment, task, and agent state.
- Minimal chromatic expression: white, near-black, gray, hairline borders, and a small amount of teal.
- Subtle selected states, quiet hover states, and nearly flat panels.
- Direct language, tool-like controls, and no marketing copy.
- Agent activity shown as a first-class workflow, with logs, progress, and cancel controls.

Avoid designs that look like generic admin templates, SaaS analytics dashboards, Notion clones, or AI landing pages. The closest mental model is: "Codex desktop, but the task domain is local wiki building instead of code editing."

## 2. Product Personality

The interface should feel:

- Local-first: files, paths, checkpoints, and tasks are visible when they matter.
- Capable: users can see what the app is doing, what changed, and what needs confirmation.
- Quiet: controls stay out of the way until they are needed.
- Trustworthy: destructive actions, Agent changes, and API configuration never feel hidden.
- Literary: Markdown reading and generated wiki pages should feel polished and comfortable.

The interface should not feel:

- Futuristic for its own sake.
- SaaS-marketing oriented.
- Chatbot-only.
- Overly colorful.
- Decorative at the cost of scanability.

## 3. Layout Model

### 3.1 Global Shell

Use a stable desktop shell across the main application:

```text
Top Bar
+-- Project switcher
+-- Global search / command input
+-- Task activity
+-- Language switch
+-- Settings

Main Area
+-- Left Sidebar
|   +-- Primary navigation
|   +-- Project file tree shortcuts
|   +-- Recent / pinned items when useful
+-- Center Workspace
|   +-- Current view content
+-- Right Context Panel
    +-- Metadata, sources, related pages, logs, preview, or diff controls

Bottom Status Bar
+-- Current project path
+-- Agent / BYOK route
+-- Background task summary
+-- Wiki page count / index state
```

### 3.2 Layout Dimensions

- App minimum target width: 1120px.
- Left sidebar: 240px default, 56px collapsed.
- Right panel: 320px default, 380px for diff or source-heavy views.
- Top bar height: 48px.
- Bottom status bar height: 28px.
- Main content padding: 16px for dense tool views, 24px for reading and dashboard views.
- Splitter handles must be visible on hover and keyboard accessible.
- Treat panes as part of the application frame, not as floating cards.
- Keep the outer app chrome flat and continuous, similar to Codex desktop.

### 3.3 Responsive Behavior

This is a desktop-first app. Smaller windows should preserve the workbench model rather than transform into a mobile landing layout.

- Under 960px: right panel collapses into a drawer.
- Under 760px: left sidebar collapses to icons, and global search becomes a command button.
- Never scale font size with viewport width.
- Avoid layout shifts when task labels, file names, or status text update.

## 4. Color System

Use `DESIGN.md` as the source palette, adapted for an app shell.

### 4.1 Core Tokens

| Token | Value | Use |
|---|---:|---|
| `--background` | `#ffffff` | Main canvas |
| `--foreground` | `#0d0d0d` | Primary text |
| `--surface` | `#fafafa` | App chrome, sidebar, subtle bands |
| `--surface-raised` | `#ffffff` | Panels, menus, dialogs |
| `--surface-muted` | `#f5f5f5` | Inputs, secondary controls, selected rows |
| `--border` | `#e5e5e5` | Standard hairline border |
| `--border-subtle` | `#ededed` | Low-emphasis separators |
| `--text-primary` | `#0d0d0d` | Headings and main labels |
| `--text-secondary` | `#3c3c3c` | Body text |
| `--text-muted` | `#6e6e6e` | Metadata and secondary labels |
| `--text-disabled` | `#9b9b9b` | Disabled text |
| `--accent` | `#10a37f` | Links, selected state, success path |
| `--accent-hover` | `#0a7a5e` | Accent hover |
| `--accent-soft` | `#e8f5f0` | Accent selected background |
| `--danger` | `#ef4146` | Destructive action, validation |
| `--warning` | `#f5a623` | Risk, partial success |
| `--info` | `#2563eb` | Rare informational state |

### 4.2 Dark Theme

Dark theme is required by product scope, but the first implementation may prioritize light mode if documented. When implemented, keep it quiet and high contrast:

| Token | Value | Use |
|---|---:|---|
| `--background` | `#111111` | Main canvas |
| `--foreground` | `#f4f4f4` | Primary text |
| `--surface` | `#171717` | App chrome |
| `--surface-raised` | `#1f1f1f` | Panels, menus, dialogs |
| `--surface-muted` | `#262626` | Inputs and selected rows |
| `--border` | `#303030` | Standard border |
| `--text-secondary` | `#d4d4d4` | Body text |
| `--text-muted` | `#a3a3a3` | Metadata |
| `--accent` | `#10a37f` | Same accent |

Dark mode must not become a blue-black neon theme. Keep saturation low and use teal only for active state and meaningful highlights.

### 4.3 Color Rules

- Use teal sparingly. It marks active navigation, links, primary progress, and successful states.
- Use black or white for the main call to action depending on surface contrast.
- Do not use gradients as backgrounds.
- Do not use decorative color blobs, bokeh, or ornamental glows.
- Semantic colors must be paired with labels or icons, never color alone.

## 5. Typography

### 5.1 Font Families

```css
--font-ui: Inter, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
--font-display: "Source Serif Pro", Georgia, serif;
--font-mono: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
```

If Sohne or Signifier are legally available in the implementation environment, they may replace Inter and Source Serif Pro. Do not block implementation on proprietary fonts.

### 5.2 Type Scale

| Role | Size | Weight | Line Height | Use |
|---|---:|---:|---:|---|
| App title | 24px | 600 | 1.2 | Project landing, major empty states |
| View title | 20px | 600 | 1.3 | Dashboard, Chat, Import, Lint |
| Section title | 16px | 600 | 1.35 | Panel groups |
| Body | 14px | 400 | 1.55 | Default UI text |
| Body small | 13px | 400 | 1.45 | Dense rows, metadata |
| Label | 12px | 500 | 1.35 | Control labels, badges |
| Caption | 11px | 500 | 1.35 | Status bar and tiny metadata |
| Reading body | 16px | 400 | 1.7 | Markdown article reading |
| Reading heading | 28px | 600 | 1.2 | Markdown H1 rendered content |
| Code | 13px | 400 | 1.55 | Code, paths, terminal output |

### 5.3 Typography Rules

- Use sans-serif for all app chrome and controls.
- Use serif only inside reading, report preview, or editorial empty states.
- Keep letter spacing at `0` unless using tiny uppercase labels, where `0.02em` is acceptable.
- Avoid weights above 600.
- Use monospace for file paths, CLI output, code blocks, JSON snippets, and exact command names.
- Chinese and English text must both fit naturally. Avoid narrow fixed-width controls for translated labels.

## 6. Spacing, Radius, and Borders

### 6.1 Spacing Scale

Use a 4px base scale:

```text
4, 8, 12, 16, 20, 24, 32, 40, 48, 64
```

Rules:

- Use 8px gaps inside compact controls.
- Use 12px gaps inside rows and form groups.
- Use 16px gaps between related panel sections.
- Use 24px padding for reading surfaces and dashboard summaries.
- Avoid large empty hero spacing inside tool views.

### 6.2 Radius

| Token | Value | Use |
|---|---:|---|
| `--radius-sm` | 4px | Inputs inside dense tables, badges |
| `--radius-md` | 6px | Buttons, list rows, menu items |
| `--radius-lg` | 8px | Panels, dialogs, repeated cards |
| `--radius-pill` | 9999px | Tags, segmented controls, status pills |

Keep card and panel radius at 8px or below. This keeps the app closer to a professional desktop tool than a marketing card layout.

### 6.3 Borders and Shadows

- Prefer 1px borders over shadows.
- Use shadows only for floating menus, popovers, and dialogs.
- Default panel border: `1px solid var(--border)`.
- Hover row border should not cause layout shift.
- Dividers should be subtle and functional.

## 7. Core Components

### 7.0 Codex-Like Component Bias

Default to components that feel native to a desktop agent workspace:

- Toolbar rows instead of hero action areas.
- Pane headers instead of large card headers.
- Inline status text instead of oversized metric cards.
- Split views instead of stacked dashboard blocks.
- Menus, popovers, and drawers for secondary controls.
- Compact tables and lists for files, tasks, findings, and sessions.
- Terminal-like log panes for Agent output and background tasks.
- Subtle badges for execution route, status, and risk level.

Use cards only for repeated items, dialogs, and genuinely framed summaries. If a section can be represented as a pane, table, list, toolbar, or inspector, do that first.

### 7.1 Buttons

Use shadcn-style buttons with Lucide icons where an icon improves scanning.

Primary:

- Background: `#0d0d0d`
- Text: `#ffffff`
- Radius: 6px
- Height: 32px or 36px
- Use for the main action in a view, such as Import, Save, Run Lint, Start Compile.

Secondary:

- Background: `#ffffff`
- Border: `#e5e5e5`
- Text: `#0d0d0d`
- Hover: `#fafafa`
- Use for normal actions.

Ghost:

- Background: transparent
- Hover: `#f5f5f5`
- Use for toolbar icons, row actions, panel toggles.

Danger:

- Use red only for confirmed destructive flows.
- Destructive buttons must appear with clear copy in dialogs.

Rules:

- Prefer icon-only buttons for common tools: save, edit, refresh, filter, search, more, collapse, copy, open folder.
- Add tooltips to icon-only buttons.
- Button labels must not wrap awkwardly. If translated labels are long, allow the button to grow or move the label out of the button.

### 7.2 Inputs and Search

Inputs:

- Height: 32px for dense tool inputs, 36px for forms.
- Radius: 6px.
- Border: `#e5e5e5`.
- Focus ring: `0 0 0 3px rgba(16, 163, 127, 0.12)`.

Global search:

- Place in the top bar.
- Behaves like a command/search hybrid.
- Empty input hint should mention project search, not AI chat.
- Search must not trigger model calls automatically.

### 7.3 Navigation

Primary navigation belongs in the left sidebar:

- Dashboard
- Wiki
- Chat
- Graph
- Agent
- Import
- Lint
- Exports
- Settings

Use Lucide icons plus text in expanded mode, icons only in collapsed mode. Active state uses `--accent-soft` background and `--accent` icon/text. Keep navigation rows 32px high with 6px radius.

### 7.4 Panels

Panels are functional surfaces, not decorative cards.

- Use panels for sidebar, right context, task drawer, metadata inspector, source list, diff controls.
- Avoid cards inside cards.
- Panel headers should contain title, optional count, and at most two actions.
- Panel content should scroll independently when needed.

### 7.5 Tables and Lists

Use compact list rows for files, imports, lint findings, tasks, and agent runs.

Row anatomy:

- Leading icon or status dot.
- Primary label.
- Secondary metadata.
- Optional badge.
- Trailing action menu.

Rules:

- Row height: 36px compact, 44px standard.
- Selected row: `--accent-soft`.
- Hover row: `--surface-muted`.
- Use truncation for long paths, with tooltip for full path.
- Keep checkboxes aligned in batch operation lists.

### 7.6 Badges and Status Pills

Badges should communicate state quickly:

- Success: teal soft background.
- Warning: warm soft background.
- Error: red soft background.
- Neutral: muted surface.

Use short labels such as `Ready`, `Running`, `Needs review`, `Conflict`, `Indexed`, `BYOK`, `Agent`.

### 7.7 Dialogs and Drawers

Use dialogs for blocking decisions:

- Delete page.
- Overwrite file.
- Apply Agent changes.
- Initialize a normal folder as project.
- Store or replace provider credentials.

Use drawers for non-blocking inspection:

- Task logs.
- Source metadata.
- Related pages.
- Export preview options.

Destructive dialogs must include:

- What will change.
- Whether a Git checkpoint will be created.
- The exact user choice.
- Cancel as the visually safer option.

## 8. View Guidelines

### 8.1 Project Start and Dashboard

The first screen should be a usable project surface, not a marketing page.

Dashboard should show:

- Current project health.
- Recent pages.
- Import status.
- Agent and BYOK availability.
- Index and graph state.
- Recent tasks.

Use compact summary blocks, not large decorative cards. Empty states should offer direct actions: create project, open folder, import files.

### 8.2 Wiki Browser and Reader

The Wiki view is the core reading and editing workspace.

Layout:

- Left: file tree or page list.
- Center: Markdown reader or editor.
- Right: metadata, citations, related pages, backlinks when available.

Reading surface:

- White background.
- Max reading width around 760px.
- Markdown body uses 16px text and generous line height.
- Code blocks use muted background and copy action.
- Wikilinks should be visually distinct but not loud.

Editor surface:

- Keep toolbar compact and icon-driven.
- Save state must be visible.
- External modification conflicts must interrupt save with a clear diff path.

### 8.3 Chat

Chat is a knowledge query surface, not the whole app.

Layout:

- Conversation list or session switcher on the left when useful.
- Message stream in center.
- Sources and citations in the right panel.
- Composer fixed to bottom of the center workspace.

Rules:

- Assistant answers must show source links or source cards when available.
- Streaming state should be calm: subtle cursor, progress text, and cancel button.
- Saved answers should clearly indicate the target `wiki/queries/` path.
- Empty state should suggest asking about the current wiki, not generic AI prompts.

### 8.4 Graph

Graph view should feel analytical, not decorative.

Layout:

- Full central canvas.
- Left or top controls for filters, layout, fit-to-screen, search.
- Right panel for selected node details.

Rules:

- Use restrained node colors with enough contrast.
- Community colors may expand beyond teal, but keep saturation moderate.
- Provide fit-to-screen, zoom, and reset layout controls as icon buttons.
- Show build progress and allow cancellation for long graph builds.
- Avoid text labels that overlap heavily; use hover and selection for detail.

### 8.5 Import

Import is a review workflow.

Layout:

- Drop zone or file picker at top.
- Parsed file list below.
- Preview and metadata in right panel.
- Confirm action fixed near the top or bottom of the workflow, depending on view density.

Rules:

- Show file name, type, size, parse status, extracted text preview, and errors.
- Preserve original files visibly.
- Do not start wiki compilation until user confirms.
- Conflicts and renames must be explicit.

### 8.6 Agent Panel

Agent panel should communicate capability and process state.

Show:

- Detected CLIs and versions.
- Default Agent.
- BYOK fallback availability.
- Running tasks.
- stdout / stderr logs.
- Cancel controls.

Rules:

- Do not present install actions as automatic.
- Logs use monospace and a dark or muted terminal surface.
- Running state must survive navigation away from the view.

### 8.7 Lint

Lint is a diagnosis and repair workflow.

Layout:

- Summary counts at top.
- Issue list grouped by severity and type.
- Selected issue detail and suggested fix in right panel.

Rules:

- Local deterministic checks and Agent deep lint must be visually distinct.
- High-risk fixes require confirmation.
- Batch fix actions must show checkpoint behavior.

### 8.8 Settings

Settings should be plain and predictable.

Groups:

- General.
- Appearance.
- Language.
- Agent.
- LLM providers.
- Security.
- Background tasks.
- Updates.

Rules:

- API keys are never displayed in full by default.
- Provider testing should show clear success or failure.
- Theme controls should preview without requiring restart.

## 9. Interaction Patterns

### 9.1 Selection

Use selection consistently:

- Single selected page in Wiki.
- Single selected node in Graph.
- Single selected issue in Lint.
- Multiple selection only for batch import or batch fixes.

Selected state should be visible without relying on color alone.

### 9.2 Long Tasks

Long tasks include import parsing, wiki compilation, graph building, deep lint, and export generation.

Every long task must provide:

- Status label.
- Progress when measurable.
- Log or detail view.
- Cancel action when technically possible.
- Completion or failure notification.

Do not block the whole UI for long tasks.

### 9.3 Confirmations

Require confirmation for:

- Deleting pages.
- Replacing original sources.
- Batch rewrites.
- Applying Agent-generated diffs.
- Initializing a normal folder as a project.
- Saving credentials.

Confirmation copy should be specific and calm. Avoid vague labels such as `Are you sure?`.

### 9.4 Empty States

Empty states should be compact and useful:

- One sentence explaining the state.
- One primary action.
- Optional secondary action.
- No illustrations unless they communicate the product object directly.

### 9.5 Errors

Error messages should include:

- What failed.
- Why, if known.
- What the user can do next.
- Link to logs when available.

Avoid raw stack traces in primary UI. Put detailed logs in expandable panels.

## 10. Motion

Motion should support orientation, not spectacle.

- Hover and focus: 120-160ms.
- Panel open or drawer transition: 180-240ms.
- Toast entry: 160-220ms.
- Use `cubic-bezier(0.16, 1, 0.3, 1)` for subtle ease-out.
- Respect reduced motion settings.

Allowed:

- Fade.
- Small translate.
- Progress shimmer for loading rows.

Avoid:

- Parallax.
- Scroll-jacking.
- Large bouncing transitions.
- Decorative animation loops.

## 11. Icons

Use Lucide React for UI icons.

Suggested mappings:

| Concept | Icon |
|---|---|
| Dashboard | `LayoutDashboard` |
| Wiki | `BookOpenText` |
| Chat | `MessageSquare` |
| Graph | `Network` |
| Agent | `Bot` |
| Import | `Upload` |
| Lint | `ShieldCheck` |
| Exports | `FileOutput` |
| Settings | `Settings` |
| Search | `Search` |
| Save | `Save` |
| Edit | `Pencil` |
| Refresh | `RefreshCw` |
| Open folder | `FolderOpen` |
| Diff | `GitCompare` |
| Task running | `LoaderCircle` |
| Warning | `TriangleAlert` |
| Error | `CircleAlert` |

Rules:

- Use 16px icons in dense controls.
- Use 18px icons in navigation.
- Use 20px icons only for empty states or larger section headers.
- Do not mix icon families.

## 11.1 Codex-Like Visual Checklist

A screen is sufficiently Codex-like when:

- It looks useful before it looks beautiful.
- The main surface is a working area, not a presentation area.
- Navigation, task status, and context stay visible during work.
- Most controls are compact, aligned, and quiet.
- The design relies on spacing, typography, borders, and selection state rather than decoration.
- Agent or background execution is visible through logs, progress, and state labels.
- The user can understand where files live, what changed, and what process is running.

If a screen could be mistaken for a generic SaaS dashboard screenshot, revise it toward the Codex desktop shell.

## 12. Accessibility

Minimum requirements:

- All controls keyboard reachable.
- Focus rings visible.
- Icon-only buttons have accessible labels and tooltips.
- Text contrast meets WCAG AA.
- State is not communicated by color alone.
- Resizable panes remain operable by keyboard or have accessible alternatives.
- Toasts and task updates should not steal focus.
- Dialogs trap focus and return it to the trigger on close.

## 13. Tailwind and shadcn Implementation Notes

Use semantic tokens through CSS variables, then map them into Tailwind. Avoid scattering raw colors across components.

Recommended token categories:

```css
:root {
  --background: #ffffff;
  --foreground: #0d0d0d;
  --surface: #fafafa;
  --surface-raised: #ffffff;
  --surface-muted: #f5f5f5;
  --border: #e5e5e5;
  --border-subtle: #ededed;
  --accent: #10a37f;
  --accent-hover: #0a7a5e;
  --accent-soft: #e8f5f0;
  --danger: #ef4146;
  --warning: #f5a623;
  --info: #2563eb;
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 8px;
}
```

Component rules:

- Wrap shadcn primitives in local app components when they encode product behavior.
- Keep generic UI primitives in `components/ui`.
- Keep app-specific composites in `components/app` or feature folders.
- Prefer named variants over one-off class strings for repeated patterns.
- Do not create a new component abstraction for a single use unless it clarifies a complex flow.

## 14. Content and Language

The app supports Chinese and English. UI writing should be short, direct, and action-oriented.

Rules:

- Use verbs for commands: `Import`, `Save`, `Run Lint`, `Apply Diff`.
- Use nouns for navigation: `Wiki`, `Chat`, `Graph`, `Agent`, `Settings`.
- Avoid hype words such as `magic`, `revolutionary`, or `supercharged`.
- Explain risky actions in plain language.
- File paths and generated filenames should be shown exactly.
- Chinese labels should not be forced into narrow English-sized controls.

## 15. Do and Do Not

Do:

- Build the usable app surface first.
- Make the app visually close to Codex desktop: compact, pane-based, quiet, and agent-aware.
- Keep the interface quiet and information-dense.
- Show the current project and local file boundaries clearly.
- Make background tasks visible and cancellable.
- Use right panels for context instead of modal overload.
- Use Git checkpoint and diff language consistently.
- Keep Markdown reading beautiful and calm.

Do not:

- Turn the app into a landing page.
- Drift into generic shadcn dashboard styling.
- Use oversized decorative cards for primary workflows.
- Hide Agent or file-system side effects.
- Let search silently call a model.
- Use gradients, decorative blobs, or glossy AI visuals.
- Nest cards inside cards.
- Use color as the only signal for risk or state.
- Display full API keys by default.

## 16. Acceptance Checklist

Before considering a frontend view complete, verify:

- The view fits the global shell model.
- Text fits in Chinese and English.
- Primary and secondary actions are visually distinct.
- Loading, empty, error, success, and disabled states exist.
- Long-running actions show progress and can be inspected.
- Destructive or high-risk actions require confirmation.
- Icon-only controls have tooltips and accessible labels.
- The view works in light theme and does not block future dark theme.
- No layout depends on viewport-scaled font sizes.
- No decorative gradients, blobs, or nested cards are introduced.
