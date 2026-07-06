# Export Workflow Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` or `superpowers:executing-plans` to implement this plan task-by-task.

**Goal:** Implement the batch 04 export workflow preview improvements: successful export rows can preview by click/keyboard, preview can maximize in-app, generated HTML can open in the browser, and row-level file actions sit beside the filename.

**Architecture:** React owns only UI state and event orchestration. Zustand stores hold export preview mode and global workspace focus. Tauri commands remain the only bridge for filesystem reads and OS process opening. Rust services validate export HTML paths before any read/open operation, using the project folder as source of truth and keeping all user content as Markdown/JSON/local files.

**Tech Stack:** React 19, TypeScript, Vite, Tailwind CSS v4-compatible global CSS tokens, Zustand, react-i18next, Lucide React, Tauri v2, Rust services.

---

## Read Context

- `docs/fixes/00-codebase-audit.md`
- `docs/fixes/04-export-workflow-preview.md`
- `docs/fixes/plan/plan-batch-{01}.md`
- `SPEC/PRD.md`
- `SPEC/SPEC.md`
- `SPEC/APP_flow.md`
- `SPEC/TECH_STACK.md`
- `SPEC/BACKEND_STRUCTURE.md`
- `SPEC/FRONTEND_GUIDELINES.md`
- `SPEC/DESIGN.md`
- `SPEC/plans/exports.md`
- `SPEC/gotchas.txt`
- `UI-Frontend-design/exports.html`
- `UI-Frontend-design/assets/app.css`
- `src/features/exports/ExportsView.tsx`
- `src/features/exports/HtmlPreviewPane.tsx`
- `src/features/exports/exportsView.test.tsx`
- `src/stores/exportStore.ts`
- `src/stores/navigationStore.ts`
- `src/components/app/AppShell.tsx`
- `src/features/wiki/HtmlPreviewPane.tsx`
- `src/types/export.ts`
- `src/styles.css`
- `src/i18n/locales/en.json`
- `src/i18n/locales/zh-CN.json`
- `src-tauri/src/models/export.rs`
- `src-tauri/src/commands/export_commands.rs`
- `src-tauri/src/services/export_service.rs`
- `src-tauri/src/models/paths.rs`
- `src-tauri/src/services/file_store.rs`
- `src-tauri/src/lib.rs`

## Clarification Status

No blocking clarification questions remain. The plan proceeds with these explicit assumptions:

- The enlarged preview is an in-app focus mode, not native OS fullscreen.
- Focus mode keeps the left sidebar, topbar, and statusbar visible.
- Focus mode hides the export list pane and the global right context panel.
- Opening in browser means opening an existing generated `.html` file with the OS default handler through a backend command.
- Only `succeeded` export records are previewable from row click or keyboard activation.
- The `UI-Frontend-design/` folder remains read-only reference material.

## Key Decisions

- Add `previewMode: "split" | "maximized"` to `exportStore`; add `workspaceFocus: null | "exportPreview"` to `navigationStore`.
- Preserve the previous `rightPanelOpen` value when entering preview focus mode and restore it when exiting.
- Keep path validation in Rust. React may show buttons, but Rust decides whether `outputPath` is safe and valid.
- Add a shared export HTML resolver in `ExportService` and use it from `read_export_preview`, `open_export_folder`, and the new `open_export_in_browser`.
- Use direct argv process spawning for file-manager and browser opening. Do not use shell strings, `cmd /C start`, or interpolated command text.
- Reuse existing `.html-preview*`, `.panel`, `.table`, `.icon-button`, and token-based CSS patterns. Do not introduce card-heavy or hero-like UI.

## File Map

- `src/types/export.ts`: add `ExportPreviewMode` and `OpenExportInBrowserRequest`.
- `src/stores/exportStore.ts`: add preview mode state, `setPreviewMode`, and `openInBrowser`.
- `src/stores/navigationStore.ts`: add workspace focus state and enter/exit actions.
- `src/components/app/AppShell.tsx`: hide right context panel in workspace focus; make Escape exit focus first.
- `src/features/exports/ExportsView.tsx`: add clickable rows, keyboard activation, inline actions, selected state, browser-open handler, and preview focus toggle.
- `src/features/exports/HtmlPreviewPane.tsx`: add toolbar for open browser, maximize/restore, clear preview, output path, and empty state.
- `src/styles.css`: add focus-mode and export-preview layout selectors using existing tokens.
- `src/i18n/locales/en.json` and `src/i18n/locales/zh-CN.json`: add accessible labels/tooltips.
- `src/features/exports/exportsView.test.tsx`: extend UI behavior tests.
- `src/stores/exportStore.test.ts`: add focused store tests.
- `src/stores/navigationStore.test.ts`: add focused workspace focus tests.
- `src-tauri/src/models/export.rs`: add `OpenExportInBrowserRequest`.
- `src-tauri/src/services/export_service.rs`: add shared existing-export-HTML resolver and tests.
- `src-tauri/src/commands/export_commands.rs`: add browser-open command and reuse resolver.
- `src-tauri/src/lib.rs`: register the new command.

## Implementation Tasks

### Task 1: Backend DTO And Shared Export HTML Resolver

- [ ] Add the request DTO to `src-tauri/src/models/export.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenExportInBrowserRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub output_path: String,
}
```

- [ ] Add a serde test near existing export model tests:

```rust
#[test]
fn open_export_in_browser_request_serializes_camel_case() {
    let request = OpenExportInBrowserRequest {
        project_id: "project-1".into(),
        project_root_path: "D:/wiki".into(),
        output_path: "exports/html/报告.html".into(),
    };

    let value = serde_json::to_value(&request).expect("serialize request");

    assert_eq!(value["projectId"], "project-1");
    assert_eq!(value["projectRootPath"], "D:/wiki");
    assert_eq!(value["outputPath"], "exports/html/报告.html");
}
```

- [ ] Add a shared resolver to `src-tauri/src/services/export_service.rs`:

```rust
pub fn resolve_existing_html_export(
    &self,
    context: &ProjectContext,
    output_path: &str,
) -> Result<PathBuf, BackendError> {
    let normalized = normalize_project_path(output_path);

    if !normalized.starts_with("exports/html/")
        || normalized.contains("..")
        || !normalized.to_ascii_lowercase().ends_with(".html")
    {
        return Err(BackendError::new(
            "EXPORT_PATH_INVALID",
            "Export preview path must point to an HTML file under exports/html",
        ));
    }

    let absolute = context.resolve_project_path(&normalized)?;

    if !absolute.is_file() {
        return Err(BackendError::new(
            "EXPORT_FILE_NOT_FOUND",
            "Export HTML file was not found",
        ));
    }

    Ok(absolute)
}
```

- [ ] Add required imports without duplicating existing ones:

```rust
use std::path::PathBuf;
use crate::models::paths::ProjectContext;
use crate::utils::path_utils::normalize_project_path;
```

- [ ] Add service tests. Adapt helper names to the existing `export_service.rs` test fixture style:

```rust
#[test]
fn resolve_existing_html_export_accepts_valid_cjk_path() {
    let (temp_dir, context, service) = test_export_service();
    let output = temp_dir.path().join("exports/html/报告 index.html");
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(&output, "<!doctype html><html></html>").unwrap();

    let resolved = service
        .resolve_existing_html_export(&context, "exports/html/报告 index.html")
        .expect("valid export html resolves");

    assert_eq!(resolved, output);
}

#[test]
fn resolve_existing_html_export_rejects_non_export_path() {
    let (_temp_dir, context, service) = test_export_service();
    let error = service
        .resolve_existing_html_export(&context, "wiki/page.html")
        .expect_err("non-export path should fail");

    assert_eq!(error.code, "EXPORT_PATH_INVALID");
}

#[test]
fn resolve_existing_html_export_rejects_non_html_path() {
    let (_temp_dir, context, service) = test_export_service();
    let error = service
        .resolve_existing_html_export(&context, "exports/html/report.txt")
        .expect_err("non-html path should fail");

    assert_eq!(error.code, "EXPORT_PATH_INVALID");
}

#[test]
fn resolve_existing_html_export_rejects_traversal() {
    let (_temp_dir, context, service) = test_export_service();
    let error = service
        .resolve_existing_html_export(&context, "exports/html/../../wiki/secret.html")
        .expect_err("traversal should fail");

    assert_eq!(error.code, "EXPORT_PATH_INVALID");
}

#[test]
fn resolve_existing_html_export_rejects_missing_file() {
    let (_temp_dir, context, service) = test_export_service();
    let error = service
        .resolve_existing_html_export(&context, "exports/html/missing.html")
        .expect_err("missing file should fail");

    assert_eq!(error.code, "EXPORT_FILE_NOT_FOUND");
}
```

### Task 2: Backend Browser-Open Command

- [ ] Update `src-tauri/src/commands/export_commands.rs` imports:

```rust
use std::path::Path;
use std::process::Command;
use crate::models::export::OpenExportInBrowserRequest;
```

- [ ] Modify `read_export_preview` to use the shared resolver:

```rust
#[tauri::command]
pub async fn read_export_preview(
    state: State<'_, AppState>,
    request: ReadExportPreviewRequest,
) -> Result<String, BackendError> {
    let context = ProjectContext::new(request.project_id, request.project_root_path)?;
    let absolute = state
        .export_service
        .resolve_existing_html_export(&context, &request.output_path)?;

    std::fs::read_to_string(&absolute).map_err(|err| {
        BackendError::new("FILE_READ_FAILED", "Failed to read export preview")
            .with_details(format!("{}: {err}", absolute.display()))
    })
}
```

- [ ] Modify `open_export_folder` to use the same resolver:

```rust
#[tauri::command]
pub async fn open_export_folder(
    state: State<'_, AppState>,
    request: OpenExportFolderRequest,
) -> Result<(), BackendError> {
    let context = ProjectContext::new(request.project_id, request.project_root_path)?;
    let absolute = state
        .export_service
        .resolve_existing_html_export(&context, &request.output_path)?;

    reveal_in_file_manager(&absolute)
}
```

- [ ] Add `open_export_in_browser`:

```rust
#[tauri::command]
pub async fn open_export_in_browser(
    state: State<'_, AppState>,
    request: OpenExportInBrowserRequest,
) -> Result<(), BackendError> {
    let context = ProjectContext::new(request.project_id, request.project_root_path)?;
    let absolute = state
        .export_service
        .resolve_existing_html_export(&context, &request.output_path)?;

    open_in_default_browser(&absolute)
}
```

- [ ] Add a direct-argv opener helper:

```rust
fn open_in_default_browser(path: &Path) -> Result<(), BackendError> {
    let spawn_result = if cfg!(target_os = "windows") {
        Command::new("explorer").arg(path).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else {
        Command::new("xdg-open").arg(path).spawn()
    };

    spawn_result.map(|_| ()).map_err(|err| {
        BackendError::new("EXPORT_OPEN_FAILED", "Failed to open export in browser")
            .with_details(format!("{}: {err}", path.display()))
    })
}
```

- [ ] Ensure existing Windows file-manager reveal stays direct-argv:

```rust
Command::new("explorer")
    .args(["/select,"])
    .arg(path)
    .spawn()
```

- [ ] Register the command in `src-tauri/src/lib.rs`:

```rust
commands::export_commands::open_export_in_browser,
```

### Task 3: Export Store And Navigation Focus State

- [ ] Update `src/types/export.ts`:

```ts
export type ExportPreviewMode = "split" | "maximized";

export interface OpenExportInBrowserRequest {
  projectId: string;
  projectRootPath: string;
  outputPath: string;
}
```

- [ ] Update `src/stores/exportStore.ts`:

```ts
interface ExportState {
  records: ExportRecord[];
  loading: boolean;
  runningTaskId: string | null;
  previewHtml: string | null;
  previewId: string | null;
  previewMode: ExportPreviewMode;
  error: string | null;
  setPreviewMode: (mode: ExportPreviewMode) => void;
  openInBrowser: (request: OpenExportInBrowserRequest) => Promise<void>;
}
```

```ts
previewMode: "split",
setPreviewMode: (previewMode) => set({ previewMode }),
openInBrowser: async (request) => {
  try {
    set({ error: null });
    await invoke("open_export_in_browser", { request });
  } catch (error) {
    set({ error: normalizeError(error, "Failed to open export in browser") });
  }
},
```

- [ ] Ensure `reset` restores:

```ts
previewMode: "split",
previewHtml: null,
previewId: null,
error: null,
```

- [ ] Create `src/stores/exportStore.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { useExportStore } from "./exportStore";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("exportStore", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    useExportStore.getState().reset();
  });

  it("tracks and resets preview mode", () => {
    useExportStore.getState().setPreviewMode("maximized");
    expect(useExportStore.getState().previewMode).toBe("maximized");

    useExportStore.getState().reset();
    expect(useExportStore.getState().previewMode).toBe("split");
  });

  it("opens export html in browser through tauri", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useExportStore.getState().openInBrowser({
      projectId: "p1",
      projectRootPath: "D:/wiki",
      outputPath: "exports/html/report.html",
    });

    expect(invoke).toHaveBeenCalledWith("open_export_in_browser", {
      request: {
        projectId: "p1",
        projectRootPath: "D:/wiki",
        outputPath: "exports/html/report.html",
      },
    });
  });
});
```

- [ ] Update `src/stores/navigationStore.ts`:

```ts
export type WorkspaceFocus = null | "exportPreview";

interface NavigationState {
  activeView: AppView;
  rightPanelOpen: boolean;
  workspaceFocus: WorkspaceFocus;
  rightPanelOpenBeforeFocus: boolean | null;
  setActiveView: (view: AppView) => void;
  setRightPanelOpen: (open: boolean) => void;
  enterWorkspaceFocus: (focus: Exclude<WorkspaceFocus, null>) => void;
  exitWorkspaceFocus: () => void;
}
```

```ts
enterWorkspaceFocus: (workspaceFocus) =>
  set((state) => {
    if (state.workspaceFocus === workspaceFocus) {
      return state;
    }

    return {
      workspaceFocus,
      rightPanelOpenBeforeFocus: state.rightPanelOpen,
      rightPanelOpen: false,
    };
  }),
exitWorkspaceFocus: () =>
  set((state) => {
    if (state.workspaceFocus === null) {
      return state;
    }

    return {
      workspaceFocus: null,
      rightPanelOpen: state.rightPanelOpenBeforeFocus ?? state.rightPanelOpen,
      rightPanelOpenBeforeFocus: null,
    };
  }),
```

- [ ] Create `src/stores/navigationStore.test.ts`:

```ts
import { beforeEach, describe, expect, it } from "vitest";
import { useNavigationStore } from "./navigationStore";

describe("navigationStore workspace focus", () => {
  beforeEach(() => {
    useNavigationStore.setState({
      activeView: "dashboard",
      rightPanelOpen: true,
      workspaceFocus: null,
      rightPanelOpenBeforeFocus: null,
    });
  });

  it("hides and restores the right panel around export preview focus", () => {
    useNavigationStore.getState().enterWorkspaceFocus("exportPreview");

    expect(useNavigationStore.getState().workspaceFocus).toBe("exportPreview");
    expect(useNavigationStore.getState().rightPanelOpen).toBe(false);

    useNavigationStore.getState().exitWorkspaceFocus();

    expect(useNavigationStore.getState().workspaceFocus).toBeNull();
    expect(useNavigationStore.getState().rightPanelOpen).toBe(true);
  });

  it("restores a previously closed right panel as closed", () => {
    useNavigationStore.setState({ rightPanelOpen: false });

    useNavigationStore.getState().enterWorkspaceFocus("exportPreview");
    useNavigationStore.getState().exitWorkspaceFocus();

    expect(useNavigationStore.getState().rightPanelOpen).toBe(false);
  });
});
```

### Task 4: AppShell Workspace Focus Integration

- [ ] Update `src/components/app/AppShell.tsx` selectors:

```tsx
const rightPanelOpen = useNavigationStore((state) => state.rightPanelOpen);
const workspaceFocus = useNavigationStore((state) => state.workspaceFocus);
const exitWorkspaceFocus = useNavigationStore((state) => state.exitWorkspaceFocus);
const showRightPanel = rightPanelOpen && workspaceFocus === null;
```

- [ ] Update shell classes:

```tsx
<div
  className={[
    "app-shell",
    showRightPanel ? "is-right-open" : "is-right-collapsed",
    workspaceFocus ? "is-workspace-focused" : "",
  ]
    .filter(Boolean)
    .join(" ")}
>
```

- [ ] Render right context and backdrop only when `showRightPanel` is true:

```tsx
{showRightPanel ? <RightContextPanel /> : null}
{showRightPanel ? (
  <button
    type="button"
    className="app-shell__backdrop"
    aria-label={t("app.rightPanel.close")}
    onClick={() => setRightPanelOpen(false)}
  />
) : null}
```

- [ ] Make Escape exit focus before closing the right panel:

```tsx
if (event.key === "Escape") {
  if (workspaceFocus !== null) {
    exitWorkspaceFocus();
    return;
  }

  if (rightPanelOpen) {
    setRightPanelOpen(false);
  }
}
```

- [ ] Hide the right-panel reopen button while `workspaceFocus !== null`:

```tsx
{!rightPanelOpen && workspaceFocus === null ? (
  <button type="button" className="icon-button" onClick={() => setRightPanelOpen(true)}>
    ...
  </button>
) : null}
```

- [ ] Add AppShell/App tests:
  - With `workspaceFocus: "exportPreview"`, the right context panel is not rendered.
  - Pressing Escape exits focus mode and restores the previous right-panel state.

### Task 5: Export Rows, Inline Actions, And Preview Toolbar

- [ ] Expand `src/features/exports/HtmlPreviewPane.tsx` props:

```ts
interface HtmlPreviewPaneProps {
  html: string | null;
  outputPath: string | null;
  title: string;
  previewMode: ExportPreviewMode;
  onTogglePreviewMode: () => void;
  onOpenInBrowser: () => void;
  onClearPreview: () => void;
}
```

- [ ] Render a preview shell with toolbar controls:

```tsx
export function HtmlPreviewPane({
  html,
  outputPath,
  title,
  previewMode,
  onTogglePreviewMode,
  onOpenInBrowser,
  onClearPreview,
}: HtmlPreviewPaneProps) {
  const { t } = useTranslation();
  const hasPreview = Boolean(html);

  return (
    <section className="html-preview exports-preview" aria-label={t("exports.preview.title")}>
      <div className="html-preview__bar">
        <div className="html-preview__meta">
          <span className="html-preview__label">{title}</span>
          <span className="html-preview__path">{outputPath ?? t("exports.preview.empty")}</span>
        </div>
        <div className="html-preview__actions">
          <button
            type="button"
            className="html-preview__icon-button"
            onClick={onOpenInBrowser}
            disabled={!hasPreview || !outputPath}
            aria-label={t("exports.actions.openBrowser")}
            title={t("exports.actions.openBrowser")}
          >
            <ExternalLink aria-hidden="true" size={14} />
          </button>
          <button
            type="button"
            className="html-preview__icon-button"
            onClick={onTogglePreviewMode}
            disabled={!hasPreview}
            aria-label={
              previewMode === "maximized"
                ? t("exports.actions.restorePreview")
                : t("exports.actions.maximizePreview")
            }
            title={
              previewMode === "maximized"
                ? t("exports.actions.restorePreview")
                : t("exports.actions.maximizePreview")
            }
          >
            {previewMode === "maximized" ? (
              <Minimize2 aria-hidden="true" size={14} />
            ) : (
              <Maximize2 aria-hidden="true" size={14} />
            )}
          </button>
          <button
            type="button"
            className="html-preview__icon-button"
            onClick={onClearPreview}
            disabled={!hasPreview}
            aria-label={t("exports.preview.clear")}
            title={t("exports.preview.clear")}
          >
            <X aria-hidden="true" size={14} />
          </button>
        </div>
      </div>

      <div className="html-preview__frame-wrap">
        {html ? (
          <iframe
            title={t("exports.preview.iframeTitle")}
            srcDoc={html}
            sandbox=""
            className="html-preview__iframe"
          />
        ) : (
          <div className="empty-state">
            <FileText aria-hidden="true" size={18} />
            <p>{t("exports.preview.empty")}</p>
          </div>
        )}
      </div>
    </section>
  );
}
```

- [ ] Update `src/features/exports/ExportsView.tsx` selectors:

```tsx
const previewMode = useExportStore((state) => state.previewMode);
const setPreviewMode = useExportStore((state) => state.setPreviewMode);
const openInBrowser = useExportStore((state) => state.openInBrowser);
const enterWorkspaceFocus = useNavigationStore((state) => state.enterWorkspaceFocus);
const exitWorkspaceFocus = useNavigationStore((state) => state.exitWorkspaceFocus);
```

- [ ] Derive selected preview metadata:

```tsx
const selectedRecord = records.find((record) => record.id === previewId) ?? null;
const selectedOutputPath = selectedRecord?.outputPath ?? null;
const selectedTitle = selectedRecord?.title ?? t("exports.preview.title");
```

- [ ] Add preview mode handlers:

```tsx
const handleTogglePreviewMode = () => {
  if (!previewHtml) {
    return;
  }

  if (previewMode === "maximized") {
    setPreviewMode("split");
    exitWorkspaceFocus();
    return;
  }

  setPreviewMode("maximized");
  enterWorkspaceFocus("exportPreview");
};

const handleClearPreview = () => {
  clearPreview();
  setPreviewMode("split");
  exitWorkspaceFocus();
};
```

- [ ] Add browser handlers:

```tsx
const handleOpenInBrowser = async (record: ExportRecord) => {
  if (!project || record.status !== "succeeded") {
    return;
  }

  await openInBrowser({
    projectId: project.id,
    projectRootPath: rootPath,
    outputPath: record.outputPath,
  });
};

const handleOpenSelectedInBrowser = async () => {
  if (!selectedRecord) {
    return;
  }

  await handleOpenInBrowser(selectedRecord);
};
```

- [ ] Reset focus mode on unmount or project change:

```tsx
useEffect(() => {
  return () => {
    setPreviewMode("split");
    exitWorkspaceFocus();
  };
}, [exitWorkspaceFocus, setPreviewMode, project?.id]);
```

- [ ] Make successful rows clickable and keyboard-activatable:

```tsx
const canPreview = record.status === "succeeded";
const isSelected = previewId === record.id;

<tr
  key={record.id}
  className={isSelected ? "exports-record-row is-selected" : "exports-record-row"}
  role={canPreview ? "button" : undefined}
  tabIndex={canPreview ? 0 : undefined}
  aria-current={isSelected ? "true" : undefined}
  onClick={canPreview ? () => void handlePreview(record) : undefined}
  onKeyDown={
    canPreview
      ? (event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            void handlePreview(record);
          }
        }
      : undefined
  }
>
```

- [ ] Move successful row actions next to filename:

```tsx
<td>
  <div className="exports-filecell">
    <FileOutput aria-hidden="true" size={14} />
    <div className="exports-filecell__text">
      <strong>{record.title}</strong>
      <span>{record.outputPath}</span>
    </div>
    {record.status === "succeeded" ? (
      <div className="exports-inline-actions" aria-label={t("exports.table.actions")}>
        <button
          type="button"
          className="icon-button"
          onClick={(event) => {
            event.stopPropagation();
            void handlePreview(record);
          }}
          aria-label={t("exports.actions.preview")}
          title={t("exports.actions.preview")}
        >
          <Eye aria-hidden="true" size={14} />
        </button>
        <button
          type="button"
          className="icon-button"
          onClick={(event) => {
            event.stopPropagation();
            void handleOpenInBrowser(record);
          }}
          aria-label={t("exports.actions.openBrowser")}
          title={t("exports.actions.openBrowser")}
        >
          <ExternalLink aria-hidden="true" size={14} />
        </button>
        <button
          type="button"
          className="icon-button"
          onClick={(event) => {
            event.stopPropagation();
            void handleOpenFolder(record);
          }}
          aria-label={t("exports.actions.openFolder")}
          title={t("exports.actions.openFolder")}
        >
          <FolderOpen aria-hidden="true" size={14} />
        </button>
      </div>
    ) : null}
  </div>
</td>
```

- [ ] Use the maximized layout modifier:

```tsx
<div
  className={[
    "exports-view-layout",
    previewMode === "maximized" ? "exports-view-layout--preview-maximized" : "",
  ]
    .filter(Boolean)
    .join(" ")}
>
  {previewMode === "split" ? <section className="panel exports-list-pane">...</section> : null}
  <HtmlPreviewPane
    html={previewHtml}
    outputPath={selectedOutputPath}
    title={selectedTitle}
    previewMode={previewMode}
    onTogglePreviewMode={handleTogglePreviewMode}
    onOpenInBrowser={handleOpenSelectedInBrowser}
    onClearPreview={handleClearPreview}
  />
</div>
```

### Task 6: CSS And I18n

- [ ] Add CSS to `src/styles.css` using existing tokens:

```css
.app-shell.is-workspace-focused .app-shell__workbench {
  grid-template-columns: var(--sidebar-w) minmax(0, 1fr);
}

.exports-view-layout--preview-maximized {
  grid-template-columns: minmax(0, 1fr);
}

.exports-view-layout--preview-maximized .exports-list-pane {
  display: none;
}

.exports-preview {
  min-height: 0;
}

.exports-record-row[role="button"] {
  cursor: pointer;
}

.exports-record-row[role="button"]:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.exports-record-row.is-selected {
  background: var(--accent-soft);
}

.exports-filecell {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: var(--sp-2);
}

.exports-filecell__text {
  display: grid;
  min-width: 0;
  gap: 2px;
}

.exports-filecell__text strong,
.exports-filecell__text span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.exports-inline-actions {
  display: inline-flex;
  flex: none;
  align-items: center;
  gap: var(--sp-1);
  margin-left: auto;
}
```

- [ ] If `src/test/ui-css-contracts.test.ts` asserts required selectors, add checks for:

```ts
expect(css).toContain(".app-shell.is-workspace-focused");
expect(css).toContain(".exports-view-layout--preview-maximized");
expect(css).toContain(".exports-inline-actions");
expect(css).toContain(".exports-record-row");
```

- [ ] Add English locale keys:

```json
"exports.actions.openBrowser": "Open in browser",
"exports.actions.maximizePreview": "Maximize preview",
"exports.actions.restorePreview": "Restore preview",
"exports.preview.iframeTitle": "Export HTML preview",
"exports.preview.clear": "Clear preview",
"exports.preview.selected": "Selected export"
```

- [ ] Add Chinese locale keys:

```json
"exports.actions.openBrowser": "在浏览器中打开",
"exports.actions.maximizePreview": "放大预览",
"exports.actions.restorePreview": "还原预览",
"exports.preview.iframeTitle": "导出 HTML 预览",
"exports.preview.clear": "清除预览",
"exports.preview.selected": "已选导出"
```

### Task 7: Frontend Regression Tests

- [ ] Extend `src/features/exports/exportsView.test.tsx` with one successful record and one failed record.

- [ ] Mock `invoke` by command:

```ts
vi.mocked(invoke).mockImplementation(async (command) => {
  if (command === "list_exports") {
    return [successfulRecord, failedRecord];
  }
  if (command === "read_export_preview") {
    return "<!doctype html><html><body>Preview</body></html>";
  }
  if (command === "open_export_folder" || command === "open_export_in_browser") {
    return undefined;
  }
  return undefined;
});
```

- [ ] Add these behavior tests:
  - Successful row click calls `read_export_preview`.
  - Enter and Space on a focused successful row call `read_export_preview`.
  - Failed row click does not call `read_export_preview`.
  - Inline Open Folder calls `open_export_folder` and does not call preview.
  - Inline Open in Browser calls `open_export_in_browser`.
  - Maximize hides the export list pane and marks the layout as maximized.
  - Restore returns to split mode.
  - Clear preview exits maximized mode and disables preview-only controls.

### Task 8: Backend Verification

- [ ] Run Rust compile checks:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected output:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in ...
```

- [ ] Run export tests when the local Rust test runner can execute:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml export
```

Expected output:

```text
test result: ok. ... passed; 0 failed
```

Known caveat from `SPEC/gotchas.txt`: `cargo test` can fail on this Windows runner with `0xc0000139`. If that recurs, cite it and use `cargo check --lib --tests` plus targeted/frontend tests as the practical verification gate.

### Task 9: Required Project Checks And Review Workflow

- [ ] Run focused frontend tests:

```powershell
npm run test -- src/stores/navigationStore.test.ts src/stores/exportStore.test.ts src/features/exports/exportsView.test.tsx src/test/ui-css-contracts.test.ts
```

- [ ] Run all required checks:

```powershell
npm run test
npm run lint
```

- [ ] Verify import paths resolve:

```powershell
npm run build
```

- [ ] Confirm no unintended `console.log` remains:

```powershell
Get-ChildItem -LiteralPath src -Recurse -File |
  Select-String -Pattern 'console\.log'
```

Expected output for the console scan is empty.

- [ ] Run the mandated review workflow:
  - Subagent A with shared context reviews design intent, logic, consistency, and integration with existing docs.
  - Subagent B with fresh context reviews blind spots, missing tests, and unclear behavior.
  - Merge both review results.
  - Fix all valid issues.
  - Rerun checks from the beginning.
  - If subagents are unavailable, perform both reviews manually and state that in the final report.

- [ ] Append a newest-on-top entry to `SPEC/progress.txt` after implementation:

```text
[YYYY-MM-DD] exports/preview-focus — Implemented batch 04 export workflow preview — Added row preview, maximized preview focus mode, browser opener with export HTML path validation, inline actions, tests, and verification results.
```

## Acceptance Criteria

- WHEN a user clicks a successful export record row THEN the system SHALL load that record's HTML preview and mark the row as selected.
- WHEN a user focuses a successful export record row and presses Enter THEN the system SHALL load that record's HTML preview.
- WHEN a user focuses a successful export record row and presses Space THEN the system SHALL load that record's HTML preview and prevent page scrolling.
- WHEN a user clicks a failed export record row THEN the system SHALL NOT load an HTML preview.
- WHEN a user clicks the inline Preview button beside a filename THEN the system SHALL preview that record and SHALL NOT trigger any other row action.
- WHEN a user clicks the inline Open in Browser button beside a filename THEN the system SHALL call the backend browser-open command for that record and SHALL NOT trigger row preview.
- WHEN a user clicks the inline Open Folder button beside a filename THEN the system SHALL reveal that export file in the file manager and SHALL NOT trigger row preview.
- WHEN a preview is loaded and the user clicks Maximize Preview THEN the system SHALL hide the export list pane, hide the global right context panel, preserve the left sidebar/topbar/statusbar, and enlarge the HTML preview within the work surface.
- WHEN a maximized preview is active and the user clicks Restore Preview THEN the system SHALL return to the split list-plus-preview layout and restore the previous right-panel open/closed state.
- WHEN a maximized preview is active and the user presses Escape THEN the system SHALL exit preview focus mode before applying normal right-panel Escape behavior.
- WHEN the user clears the active preview THEN the system SHALL remove the preview HTML, clear selected row state, and exit maximized preview mode.
- WHEN no preview is loaded THEN the system SHALL disable browser-open and maximize controls while still showing a readable empty preview state.
- WHEN the frontend requests browser preview for an export THEN the backend SHALL validate that the path points to an existing `.html` file under `exports/html/` for the current project before opening it.
- WHEN the requested export path is outside `exports/html/` THEN the backend SHALL reject the request with `EXPORT_PATH_INVALID`.
- WHEN the requested export path contains traversal or is absolute/rooted THEN the backend SHALL reject the request before spawning any process.
- WHEN the requested export path is not an `.html` file THEN the backend SHALL reject the request with `EXPORT_PATH_INVALID`.
- WHEN the requested export HTML file no longer exists THEN the backend SHALL reject the request with `EXPORT_FILE_NOT_FOUND`.
- WHEN the export filename contains Chinese characters, spaces, or shell-sensitive characters THEN the backend SHALL pass the path as a direct process argument and SHALL NOT build a shell command string.
- WHEN the browser opener fails to spawn THEN the backend SHALL return `EXPORT_OPEN_FAILED` with non-secret diagnostic details.
- WHEN the implementation changes React code THEN `npm run test` SHALL pass.
- WHEN the implementation changes React code THEN `npm run lint` SHALL pass.
- WHEN the implementation changes imports or module boundaries THEN `npm run build` SHALL complete without import resolution errors.
- WHEN final verification runs THEN the console-log scan SHALL find no unintended `console.log` entries.
- WHEN implementation finishes THEN the agent SHALL run the two-review workflow, fix valid issues, rerun checks, and cite verification results.

## Out Of Scope

- Changing export generation semantics, templates, routes, or BYOK/Agent fallback.
- Adding a database or changing Markdown/JSON/local-file storage rules.
- Native OS fullscreen.
- Resizable panes or persisted pane widths.
- Editing generated HTML from the preview.
- Deleting, replacing, or batch-rewriting export files.
- Modifying anything under `UI-Frontend-design/`.
- Adding PDF export or print workflows.

## Self-Review Notes

- The plan keeps filesystem and process logic behind Tauri commands, matching `BACKEND_STRUCTURE.md`.
- The plan enforces path boundaries with `ProjectContext` plus export-specific checks.
- The plan preserves local-first storage and does not introduce a database.
- The plan uses existing visual density, app-shell, table, and `.html-preview` concepts instead of adding marketing/card UI.
- The plan includes keyboard and propagation behavior because row-click changes often regress accessibility.
- The plan includes CJK/spaces path coverage because the project explicitly requires Unicode and Windows path safety.
- The plan names verification commands and the known Rust test runner caveat from `SPEC/gotchas.txt`.
