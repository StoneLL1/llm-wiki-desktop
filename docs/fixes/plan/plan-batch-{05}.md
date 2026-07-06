# Project Task Health UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement batch 05 from `docs/fixes/05-project-task-health-ux.md`: task log time sorting, durable Lint history, a compact project start page, and native folder pickers for opening and creating projects.

**Architecture:** Keep React as the owner of view state and short-lived UI preferences, while all project file writes continue through typed Tauri commands and Rust services. Persist task data under `.app/tasks/`, Lint health snapshots under `.app/lint-history.json` and `.app/lint-reports/`, and recent-project metadata in the existing global recent-projects file; do not introduce a database or write these app-state files into `wiki/`.

**Tech Stack:** React 19, TypeScript, Zustand, Tailwind v4 token CSS in `src/styles.css`, Lucide React, react-i18next, Tauri v2/Rust services, Vitest + Testing Library, Rust `cargo check --lib --tests`.

---

## Read Context

- Product and architecture: `SPEC/PRD.md`, `SPEC/SPEC.md`, `SPEC/APP_flow.md`, `SPEC/TECH_STACK.md`, `SPEC/BACKEND_STRUCTURE.md`.
- Frontend design constraints: `SPEC/FRONTEND_GUIDELINES.md`, `SPEC/DESIGN.md`, `UI-Frontend-design/launch.html`, `UI-Frontend-design/dashboard.html`, `UI-Frontend-design/assets/app.css`.
- Audit and batch spec: `docs/fixes/00-codebase-audit.md`, `docs/fixes/05-project-task-health-ux.md`.
- Task code: `src/components/app/TaskLogDrawer.tsx`, `src/components/app/TaskLogDrawer.test.tsx`, `src/stores/taskStore.ts`, `src/types/task.ts`, `src-tauri/src/tasks/task_service.rs`, `src-tauri/src/commands/task_commands.rs`, `src-tauri/src/models/task.rs`.
- Lint code: `src/features/lint/LintView.tsx`, `src/features/lint/LintIssueList.tsx`, `src/features/lint/LintIssueDetails.tsx`, `src/stores/lintStore.ts`, `src/types/lint.ts`, `src-tauri/src/models/lint.rs`, `src-tauri/src/services/lint_service.rs`, `src-tauri/src/commands/lint_commands.rs`, `src-tauri/src/lib.rs`.
- Project start and picker code: `src/features/project/ProjectStartView.tsx`, `src/stores/projectStore.ts`, `src/types/project.ts`, `src/features/import/nativeFilePicker.ts`, `src/features/import/nativeFilePicker.test.ts`, `src/features/import/OpenFolderAsProjectDialog.tsx`, `src-tauri/capabilities/main.json`.
- Project backend code: `src-tauri/src/models/project.rs`, `src-tauri/src/services/project_service.rs`, `src-tauri/src/commands/project_commands.rs`, `src-tauri/src/app_state.rs`.
- Shared resources: `src/i18n/locales/en.json`, `src/i18n/locales/zh-CN.json`, `src/styles.css`, `src/app/App.test.tsx`, `src/test/ui-css-contracts.test.ts`.

## Clarification Status

No blocking questions remain. The batch spec and current code define the product intent, persistence boundaries, UI direction, and safety constraints clearly enough to proceed.

## Key Decisions

- Do not change backend task ordering. `TaskService::list_tasks` already returns `updated_at desc`; the visible bug is the frontend drawer overriding that order with status-only sorting.
- Add a pure task sorting module and keep `TASK_STATUS_ORDER` only for the explicit "status" sort mode.
- Keep the task sort UI preference out of project files. Store it in `localStorage["llm-wiki-desktop.taskSortMode.v1"]` with a safe fallback to `execution_time`.
- Keep `run_local_lint` returning `LintReport` for compatibility, but persist the report as a side effect and refresh the history list after the run.
- Use `.app/lint-history.json` as a compact index and `.app/lint-reports/{report_id}.json` as the report body. Local report ids use `local-{uuid}`; deep report ids reuse the existing `task_id` so old task links stay understandable.
- Store new Lint reports in a wrapper shape, while `get_deep_lint_report` remains backward compatible with legacy raw `DeepLintReport` files.
- Limit the history index to the newest 50 entries. Do not delete older report files in this batch; cleanup can be a later maintenance feature.
- For project creation, the UI collects `name + parentPath`; the frontend displays `rootPath = parentPath + sanitized folder name`, and the backend still validates the final root path.
- Use `@tauri-apps/plugin-dialog.open({ directory: true, multiple: false })` through a shared `pickDirectory` helper. `src-tauri/capabilities/main.json` already contains `dialog:allow-open`; tests should lock that in.
- Remove manual project-path entry from the launch page. Both "Open existing project" and "Open folder as project" use the system directory picker and then call `projectStore.openProject(path)`.
- Enrich recent projects in `list_recent_projects` and mark missing paths instead of silently deleting or mutating the recent-project file during listing.
- Do not edit `UI-Frontend-design/`; it remains the design reference, not application source.

## File Structure Map

**Create**

- `src/components/app/taskSort.ts`  
  Pure sort mode types, preference helpers, and `sortTasks`.
- `src/components/app/taskSort.test.ts`  
  Unit tests for execution-time, updated-time, status sorting, missing timestamps, and corrupt localStorage preferences.
- `src/features/lint/LintHistoryList.tsx`  
  Compact history list for local/deep reports, including active, loading, corrupt/error, and empty states.
- `src/features/project/projectPath.ts`  
  Pure project-folder-name sanitization and cross-platform display path join helpers.
- `src/features/project/projectPath.test.ts`  
  Tests for Windows, POSIX, trailing separators, CJK names, invalid path characters, and empty names.

**Modify**

- `src/components/app/TaskLogDrawer.tsx`  
  Add sort segmented control, use `sortTasks`, preserve selected task and log polling.
- `src/components/app/TaskLogDrawer.test.tsx`  
  Add ordering and sort-mode interaction tests.
- `src/types/task.ts`  
  Keep `TASK_STATUS_ORDER`; no DTO change expected.
- `src-tauri/src/tasks/task_service.rs`  
  Add a focused test proving backend list order remains `updated_at desc`; no production behavior change expected.
- `src-tauri/src/models/lint.rs`  
  Add Lint history DTOs and serde/default tests.
- `src-tauri/src/services/lint_service.rs`  
  Add report persistence, history index read/write, report read, legacy deep-report compatibility, and history trimming.
- `src-tauri/src/commands/lint_commands.rs`  
  Persist local/deep reports and expose `list_lint_history` plus `read_lint_history_report`.
- `src-tauri/src/lib.rs`  
  Register new lint commands in `tauri::generate_handler!`.
- `src/types/lint.ts`  
  Mirror lint history DTOs and command response types.
- `src/stores/lintStore.ts`  
  Add history state, load/open actions, selected history id, and non-fatal history errors.
- `src/features/lint/LintView.tsx`  
  Load history on mount, show history list, open latest report after project reopen, refresh history after local/deep runs.
- `src/features/lint/lintView.test.tsx`, `src/stores/lintStore.test.ts`  
  Add history restore, corrupt report, and command-payload tests.
- `src/features/import/nativeFilePicker.ts`  
  Add `pickDirectory`.
- `src/features/import/nativeFilePicker.test.ts`  
  Add directory picker and cancel tests.
- `src/features/project/ProjectStartView.tsx`  
  Remove manual path form and import note, add three main entries, wire directory picker, update new project dialog.
- `src/stores/projectStore.ts`  
  Keep existing `openProject/createProject` command boundaries; accept enriched recent-project DTOs.
- `src/types/project.ts`  
  Expand `RecentProject` with summary fields and missing flag.
- `src-tauri/src/models/project.rs`  
  Expand `RecentProject` with serde defaults.
- `src-tauri/src/services/project_service.rs`  
  Enrich recent-project rows from lightweight scans and mark missing paths.
- `src-tauri/src/commands/project_commands.rs`  
  Ensure remembered recents write `missing: false` and summary fields when available.
- `src-tauri/capabilities/main.json`  
  Keep `dialog:allow-open`; only modify if a contract test reveals it is absent.
- `src/i18n/locales/en.json`, `src/i18n/locales/zh-CN.json`  
  Add task sort, lint history, launch action, dialog, recent metadata, and picker text.
- `src/styles.css`  
  Add compact task sort header, lint history list, launch action/recent metadata, and path preview styling without changing design tokens.
- Existing tests: `src/app/App.test.tsx`, `src/test/ui-css-contracts.test.ts`, Rust lint/project/task tests.

---

## Task 1: Task Log Timeline Sorting

**Files:**
- Create: `src/components/app/taskSort.ts`
- Create: `src/components/app/taskSort.test.ts`
- Modify: `src/components/app/TaskLogDrawer.tsx`
- Modify: `src/components/app/TaskLogDrawer.test.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles.css`
- Test: `src-tauri/src/tasks/task_service.rs`

- [ ] **Step 1: Write failing sort tests**

Create `src/components/app/taskSort.test.ts`:

```ts
import { beforeEach, describe, expect, it } from "vitest";
import type { BackendTask } from "../../types/task";
import {
  DEFAULT_TASK_SORT_MODE,
  readTaskSortModePreference,
  sortTasks,
  writeTaskSortModePreference,
} from "./taskSort";

const baseTask = (overrides: Partial<BackendTask>): BackendTask => ({
  id: overrides.id ?? "task",
  taskType: "import",
  projectId: "project-1",
  title: overrides.title ?? "Task",
  status: overrides.status ?? "succeeded",
  progress: null,
  startedAt: overrides.startedAt ?? "2026-07-04T00:00:00Z",
  updatedAt: overrides.updatedAt ?? "2026-07-04T00:00:00Z",
  completedAt: overrides.completedAt ?? null,
  cancellable: true,
  logPath: null,
  result: null,
  error: null,
});

describe("sortTasks", () => {
  it("defaults to latest execution time using startedAt before updatedAt", () => {
    const tasks = [
      baseTask({ id: "old-running", status: "running", startedAt: "2026-07-04T01:00:00Z", updatedAt: "2026-07-04T05:00:00Z" }),
      baseTask({ id: "new-failed", status: "failed", startedAt: "2026-07-04T03:00:00Z", updatedAt: "2026-07-04T03:01:00Z" }),
      baseTask({ id: "legacy", status: "succeeded", startedAt: "", updatedAt: "2026-07-04T04:00:00Z" }),
    ];

    expect(sortTasks(tasks, "execution_time").map((task) => task.id)).toEqual([
      "legacy",
      "new-failed",
      "old-running",
    ]);
  });

  it("can sort by latest update time", () => {
    const tasks = [
      baseTask({ id: "started-new", startedAt: "2026-07-04T05:00:00Z", updatedAt: "2026-07-04T05:00:00Z" }),
      baseTask({ id: "updated-new", startedAt: "2026-07-04T01:00:00Z", updatedAt: "2026-07-04T06:00:00Z" }),
    ];

    expect(sortTasks(tasks, "updated_time").map((task) => task.id)).toEqual([
      "updated-new",
      "started-new",
    ]);
  });

  it("keeps status sorting as an explicit mode with updated time as tie-breaker", () => {
    const tasks = [
      baseTask({ id: "failed", status: "failed", updatedAt: "2026-07-04T06:00:00Z" }),
      baseTask({ id: "running-old", status: "running", updatedAt: "2026-07-04T01:00:00Z" }),
      baseTask({ id: "running-new", status: "running", updatedAt: "2026-07-04T02:00:00Z" }),
    ];

    expect(sortTasks(tasks, "status").map((task) => task.id)).toEqual([
      "running-new",
      "running-old",
      "failed",
    ]);
  });
});

describe("task sort preference", () => {
  beforeEach(() => window.localStorage.clear());

  it("falls back to execution time for missing or corrupt stored values", () => {
    window.localStorage.setItem("llm-wiki-desktop.taskSortMode.v1", "bad");
    expect(readTaskSortModePreference()).toBe(DEFAULT_TASK_SORT_MODE);
  });

  it("round-trips a valid mode", () => {
    writeTaskSortModePreference("status");
    expect(readTaskSortModePreference()).toBe("status");
  });
});
```

Run: `npm run test -- src/components/app/taskSort.test.ts`  
Expected: FAIL because `taskSort.ts` does not exist.

- [ ] **Step 2: Implement `taskSort.ts`**

Create `src/components/app/taskSort.ts`:

```ts
import type { BackendTask, TaskStatus } from "../../types/task";
import { TASK_STATUS_ORDER } from "../../types/task";

export type TaskSortMode = "execution_time" | "updated_time" | "status";

export const DEFAULT_TASK_SORT_MODE: TaskSortMode = "execution_time";
export const TASK_SORT_STORAGE_KEY = "llm-wiki-desktop.taskSortMode.v1";

const TASK_SORT_MODES: TaskSortMode[] = ["execution_time", "updated_time", "status"];

function timeValue(value: string | null | undefined): number {
  if (!value) return 0;
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function executionTime(task: BackendTask): number {
  return timeValue(task.startedAt) || timeValue(task.updatedAt);
}

function updatedTime(task: BackendTask): number {
  return timeValue(task.updatedAt);
}

function statusOrder(status: TaskStatus): number {
  return TASK_STATUS_ORDER[status] ?? 99;
}

export function isTaskSortMode(value: string | null): value is TaskSortMode {
  return TASK_SORT_MODES.includes(value as TaskSortMode);
}

export function readTaskSortModePreference(): TaskSortMode {
  try {
    const stored = window.localStorage.getItem(TASK_SORT_STORAGE_KEY);
    return isTaskSortMode(stored) ? stored : DEFAULT_TASK_SORT_MODE;
  } catch {
    return DEFAULT_TASK_SORT_MODE;
  }
}

export function writeTaskSortModePreference(mode: TaskSortMode): void {
  try {
    window.localStorage.setItem(TASK_SORT_STORAGE_KEY, mode);
  } catch {
    /* Preference persistence is best-effort only. */
  }
}

export function sortTasks(tasks: BackendTask[], mode: TaskSortMode): BackendTask[] {
  return [...tasks].sort((a, b) => {
    if (mode === "status") {
      return (
        statusOrder(a.status) - statusOrder(b.status) ||
        updatedTime(b) - updatedTime(a) ||
        a.id.localeCompare(b.id)
      );
    }
    const left = mode === "execution_time" ? executionTime(a) : updatedTime(a);
    const right = mode === "execution_time" ? executionTime(b) : updatedTime(b);
    return right - left || updatedTime(b) - updatedTime(a) || a.id.localeCompare(b.id);
  });
}
```

Run: `npm run test -- src/components/app/taskSort.test.ts`  
Expected: PASS.

- [ ] **Step 3: Add the drawer sort segmented control**

Modify `src/components/app/TaskLogDrawer.tsx`:

```tsx
import {
  DEFAULT_TASK_SORT_MODE,
  readTaskSortModePreference,
  sortTasks,
  type TaskSortMode,
  writeTaskSortModePreference,
} from "./taskSort";
```

Replace the status-only `sorted` memo with:

```tsx
const [sortMode, setSortMode] = useState<TaskSortMode>(() => {
  if (typeof window === "undefined") return DEFAULT_TASK_SORT_MODE;
  return readTaskSortModePreference();
});

const sorted = useMemo(() => sortTasks(tasks, sortMode), [tasks, sortMode]);

const selectSortMode = (mode: TaskSortMode) => {
  setSortMode(mode);
  writeTaskSortModePreference(mode);
};
```

In the drawer header, keep the 44px height and add a compact `.seg` group:

```tsx
<div className="flex min-w-0 items-center gap-2">
  <span className="text-[12px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
    {t("task.drawer.title")}
  </span>
  <div className="seg task-drawer__sort" role="group" aria-label={t("task.sort.label")}>
    {(["execution_time", "updated_time", "status"] as TaskSortMode[]).map((mode) => (
      <button
        key={mode}
        type="button"
        aria-pressed={sortMode === mode}
        className={sortMode === mode ? "is-active" : ""}
        onClick={() => selectSortMode(mode)}
      >
        {t(`task.sort.${mode}`)}
      </button>
    ))}
  </div>
</div>
```

Add English keys:

```json
{
  "task.sort.label": "Task sort",
  "task.sort.execution_time": "Executed",
  "task.sort.updated_time": "Updated",
  "task.sort.status": "Status"
}
```

Add Chinese keys:

```json
{
  "task.sort.label": "任务排序",
  "task.sort.execution_time": "执行时间",
  "task.sort.updated_time": "更新时间",
  "task.sort.status": "状态"
}
```

Add CSS:

```css
.task-drawer__sort {
  flex-shrink: 0;
}

.task-drawer__sort button {
  max-width: 72px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
```

Run: `npm run test -- src/components/app/taskSort.test.ts`  
Expected: PASS.

- [ ] **Step 4: Extend drawer interaction tests**

Add tests to `src/components/app/TaskLogDrawer.test.tsx`:

```tsx
it("shows latest execution tasks first by default regardless of status", () => {
  const oldRunning = { ...runningTask, id: "old-running", title: "Old running", status: "running" as const, startedAt: "2026-07-04T01:00:00Z", updatedAt: "2026-07-04T05:00:00Z" };
  const newFailed = { ...runningTask, id: "new-failed", title: "New failed", status: "failed" as const, startedAt: "2026-07-04T03:00:00Z", updatedAt: "2026-07-04T03:01:00Z", completedAt: "2026-07-04T03:02:00Z" };
  useTaskStore.setState({ tasks: [oldRunning, newFailed], drawerOpen: true, selectedTaskId: null });

  render(<TaskLogDrawer />);

  const buttons = screen.getAllByRole("button").map((button) => button.textContent ?? "");
  expect(buttons.indexOf("New failed")).toBeLessThan(buttons.indexOf("Old running"));
});

it("switches to status sort without changing the selected task", () => {
  const oldRunning = { ...runningTask, id: "old-running", title: "Old running", status: "running" as const, startedAt: "2026-07-04T01:00:00Z", updatedAt: "2026-07-04T01:00:00Z" };
  const newFailed = { ...runningTask, id: "new-failed", title: "New failed", status: "failed" as const, startedAt: "2026-07-04T03:00:00Z", updatedAt: "2026-07-04T03:00:00Z", completedAt: "2026-07-04T03:01:00Z" };
  useTaskStore.setState({ tasks: [newFailed, oldRunning], drawerOpen: true, selectedTaskId: "new-failed" });

  render(<TaskLogDrawer />);
  fireEvent.click(screen.getByRole("button", { name: "task.sort.status" }));

  expect(useTaskStore.getState().selectedTaskId).toBe("new-failed");
});
```

Run: `npm run test -- src/components/app/TaskLogDrawer.test.tsx src/components/app/taskSort.test.ts`  
Expected: PASS.

- [ ] **Step 5: Add a backend ordering regression test**

In `src-tauri/src/tasks/task_service.rs`, add a focused test near `test_list_tasks_with_filter`:

```rust
#[test]
fn test_list_tasks_orders_by_updated_time_desc() {
    let (service, _events) = make_service();
    let old = service.create_task(TaskType::Import, None, "Old".to_string(), true);
    let new = service.create_task(TaskType::Export, None, "New".to_string(), true);

    service
        .transition_status(&old.id, TaskStatus::Running)
        .unwrap();
    service
        .append_log(&old.id, LogLevel::Info, "old update".to_string())
        .unwrap();
    service
        .transition_status(&new.id, TaskStatus::Running)
        .unwrap();
    service
        .append_log(&new.id, LogLevel::Info, "new update".to_string())
        .unwrap();

    let listed = service.list_tasks(None);
    assert_eq!(listed[0].id, new.id);
}
```

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: PASS.

---

## Task 2: Lint History Backend Persistence

**Files:**
- Modify: `src-tauri/src/models/lint.rs`
- Modify: `src-tauri/src/services/lint_service.rs`
- Modify: `src-tauri/src/commands/lint_commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: Rust lint model/service/command compile checks

- [ ] **Step 1: Add failing DTO serde tests**

In `src-tauri/src/models/lint.rs`, add DTOs and tests in one patch, then run before implementing service methods.

Add model shapes:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LintReportKind {
    Local,
    Deep,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintHistoryEntry {
    pub id: String,
    pub kind: LintReportKind,
    pub created_at: String,
    pub issue_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    #[serde(default)]
    pub scanned_pages: Option<usize>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub route: Option<CompileRoutePreference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LintHistoryFile {
    #[serde(default = "lint_history_version")]
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<LintHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedLintReport {
    pub entry: LintHistoryEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_report: Option<LintReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deep_report: Option<DeepLintReport>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadLintHistoryReportRequest {
    pub project_id: String,
    pub project_root_path: String,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListLintHistoryRequest {
    pub project_id: String,
    pub project_root_path: String,
}

fn lint_history_version() -> u32 {
    1
}
```

Implement `Default` for `LintReportKind` explicitly:

```rust
impl Default for LintReportKind {
    fn default() -> Self {
        Self::Local
    }
}
```

Add tests:

```rust
#[test]
fn lint_history_file_defaults_version_and_entries() {
    let file: LintHistoryFile = serde_json::from_str("{}").unwrap();
    assert_eq!(file.version, 1);
    assert!(file.entries.is_empty());
}

#[test]
fn persisted_lint_report_omits_missing_report_bodies() {
    let entry = LintHistoryEntry {
        id: "local-1".into(),
        kind: LintReportKind::Local,
        created_at: "2026-07-04T00:00:00Z".into(),
        issue_count: 0,
        error_count: 0,
        warning_count: 0,
        info_count: 0,
        scanned_pages: Some(10),
        task_id: None,
        route: None,
    };
    let value = serde_json::to_value(PersistedLintReport {
        entry,
        local_report: None,
        deep_report: None,
    })
    .unwrap();

    assert_eq!(value["entry"]["kind"], serde_json::json!("local"));
    assert!(value.get("localReport").is_none());
    assert!(value.get("deepReport").is_none());
}
```

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: FAIL until imports and enum defaults are correct, then PASS.

- [ ] **Step 2: Implement history persistence helpers in `LintService`**

In `src-tauri/src/services/lint_service.rs`, add constants:

```rust
const LINT_HISTORY_PATH: &str = ".app/lint-history.json";
const LINT_REPORTS_DIR: &str = ".app/lint-reports";
const LINT_HISTORY_LIMIT: usize = 50;
```

Move the command-layer `LINT_REPORTS_DIR` constant into the service or duplicate only as a private command alias pointing to the service constant. Prefer service ownership because both local and deep persistence need it.

Add public methods:

```rust
pub fn persist_local_report(
    &self,
    context: &ProjectContext,
    report: &LintReport,
) -> Result<LintHistoryEntry, BackendError> {
    let id = format!("local-{}", uuid::Uuid::new_v4());
    let entry = lint_history_entry_for_local(&id, report);
    let persisted = PersistedLintReport {
        entry: entry.clone(),
        local_report: Some(report.clone()),
        deep_report: None,
    };
    self.file_store.ensure_dir(context, LINT_REPORTS_DIR)?;
    self.file_store.write_json_atomic(
        context,
        &format!("{LINT_REPORTS_DIR}/{id}.json"),
        &persisted,
    )?;
    self.record_history_entry(context, entry.clone())?;
    Ok(entry)
}

pub fn persist_deep_report(
    &self,
    context: &ProjectContext,
    task_id: &str,
    route: CompileRoutePreference,
    report: &DeepLintReport,
) -> Result<LintHistoryEntry, BackendError> {
    let entry = lint_history_entry_for_deep(task_id, route, report);
    let persisted = PersistedLintReport {
        entry: entry.clone(),
        local_report: None,
        deep_report: Some(report.clone()),
    };
    self.file_store.ensure_dir(context, LINT_REPORTS_DIR)?;
    self.file_store.write_json_atomic(
        context,
        &format!("{LINT_REPORTS_DIR}/{task_id}.json"),
        &persisted,
    )?;
    self.record_history_entry(context, entry.clone())?;
    Ok(entry)
}

pub fn list_lint_history(
    &self,
    context: &ProjectContext,
) -> Result<LintHistoryFile, BackendError> {
    Ok(self.load_history(context))
}

pub fn read_lint_history_report(
    &self,
    context: &ProjectContext,
    id: &str,
) -> Result<PersistedLintReport, BackendError> {
    reject_report_id(id)?;
    let path = format!("{LINT_REPORTS_DIR}/{id}.json");
    match self.file_store.read_json::<PersistedLintReport>(context, &path) {
        Ok(report) => Ok(report),
        Err(wrapper_error) => {
            let legacy = self.file_store.read_json::<DeepLintReport>(context, &path);
            legacy.map(|deep_report| PersistedLintReport {
                entry: lint_history_entry_for_deep(id, CompileRoutePreference::Auto, &deep_report),
                local_report: None,
                deep_report: Some(deep_report),
            }).map_err(|_| wrapper_error)
        }
    }
}
```

Add private helpers:

```rust
fn load_history(&self, context: &ProjectContext) -> LintHistoryFile {
    match self.file_store.read_json::<LintHistoryFile>(context, LINT_HISTORY_PATH) {
        Ok(mut file) => {
            file.version = 1;
            file.entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            file.entries.truncate(LINT_HISTORY_LIMIT);
            file
        }
        Err(err) if err.code == "FILE_READ_FAILED" => LintHistoryFile { version: 1, entries: Vec::new() },
        Err(err) => {
            eprintln!("[lint] ignoring unreadable {LINT_HISTORY_PATH}: {}", err.message);
            LintHistoryFile { version: 1, entries: Vec::new() }
        }
    }
}

fn record_history_entry(
    &self,
    context: &ProjectContext,
    entry: LintHistoryEntry,
) -> Result<(), BackendError> {
    let mut file = self.load_history(context);
    file.entries.retain(|existing| existing.id != entry.id);
    file.entries.insert(0, entry);
    file.entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    file.entries.truncate(LINT_HISTORY_LIMIT);
    self.file_store.write_json_atomic(context, LINT_HISTORY_PATH, &file)
}

fn reject_report_id(id: &str) -> Result<(), BackendError> {
    if id.is_empty() || id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(BackendError::new(
            "LINT_HISTORY_ID_INVALID",
            "Lint report id is invalid.",
            true,
            true,
        ).with_details(serde_json::json!({ "id": id })));
    }
    Ok(())
}
```

Add count helpers:

```rust
fn count_issue_severities(issues: &[LintIssue]) -> (usize, usize, usize) {
    let mut errors = 0;
    let mut warnings = 0;
    let mut infos = 0;
    for issue in issues {
        match issue.severity {
            LintSeverity::Error => errors += 1,
            LintSeverity::Warning => warnings += 1,
            LintSeverity::Info => infos += 1,
        }
    }
    (errors, warnings, infos)
}
```

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: PASS after imports compile.

- [ ] **Step 3: Add Rust service tests**

Add tests to `lint_service.rs`:

```rust
#[test]
fn local_lint_report_is_persisted_with_history_index() {
    let (context, root) = tmp_context("history-local");
    write_file(&context, "wiki/index.md", "# Index\n");
    write_file(&context, "wiki/log.md", "# Log\n");

    let service = LintService::default();
    let report = LintReport {
        issues: Vec::new(),
        generated_at: "2026-07-04T00:00:00Z".into(),
        scanned_pages: 2,
    };

    let entry = service.persist_local_report(&context, &report).unwrap();
    let history = service.list_lint_history(&context).unwrap();
    let persisted = service.read_lint_history_report(&context, &entry.id).unwrap();

    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.entries[0].id, entry.id);
    assert!(persisted.local_report.is_some());
    assert!(context.app_dir.join("lint-history.json").exists());
    assert!(context.app_dir.join("lint-reports").join(format!("{}.json", entry.id)).exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn lint_history_is_limited_to_newest_fifty_entries() {
    let (context, root) = tmp_context("history-limit");
    let service = LintService::default();
    for index in 0..55 {
        let report = LintReport {
            issues: Vec::new(),
            generated_at: format!("2026-07-04T00:{index:02}:00Z"),
            scanned_pages: 1,
        };
        service.persist_local_report(&context, &report).unwrap();
    }
    let history = service.list_lint_history(&context).unwrap();
    assert_eq!(history.entries.len(), 50);
    assert!(history.entries[0].created_at > history.entries[49].created_at);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn corrupt_single_lint_report_returns_a_report_error_not_a_history_crash() {
    let (context, root) = tmp_context("history-corrupt-report");
    write_file(&context, ".app/lint-history.json", r#"{"version":1,"entries":[{"id":"bad","kind":"local","createdAt":"2026-07-04T00:00:00Z","issueCount":1,"errorCount":1,"warningCount":0,"infoCount":0}]}"#);
    write_file(&context, ".app/lint-reports/bad.json", "{ not valid json");

    let service = LintService::default();
    let history = service.list_lint_history(&context).unwrap();
    let err = service.read_lint_history_report(&context, "bad").expect_err("bad report should fail only when opened");

    assert_eq!(history.entries.len(), 1);
    assert_eq!(err.code, "JSON_PARSE_FAILED");
    std::fs::remove_dir_all(root).unwrap();
}
```

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: PASS.

- [ ] **Step 4: Wire commands and deep lint side effects**

Modify `src-tauri/src/commands/lint_commands.rs`:

```rust
#[tauri::command]
pub fn run_local_lint(
    state: State<'_, AppState>,
    request: RunLocalLintRequest,
) -> Result<LintReport, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    let report = state
        .lint_service
        .run_local_lint(&context, &state.search_service)?;
    state.lint_service.persist_local_report(&context, &report)?;
    Ok(report)
}
```

Replace the deep report write in `run_deep_lint`:

```rust
let report = DeepLintReport {
    issues,
    raw_output: raw,
    generated_at: crate::utils::time_utils::now_rfc3339(),
};
let entry = state
    .lint_service
    .persist_deep_report(context, task_id, request.route, &report)?;
let report_path = format!(".app/lint-reports/{}.json", entry.id);
```

Update `get_deep_lint_report`:

```rust
let persisted = state
    .lint_service
    .read_lint_history_report(&context, &request.task_id)?;
persisted.deep_report.ok_or_else(|| {
    BackendError::new(
        "LINT_DEEP_REPORT_MISSING",
        "The selected lint history report is not a deep lint report.",
        true,
        true,
    )
})
```

Add commands:

```rust
#[tauri::command]
pub fn list_lint_history(
    state: State<'_, AppState>,
    request: ListLintHistoryRequest,
) -> Result<LintHistoryFile, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.lint_service.list_lint_history(&context)
}

#[tauri::command]
pub fn read_lint_history_report(
    state: State<'_, AppState>,
    request: ReadLintHistoryReportRequest,
) -> Result<PersistedLintReport, BackendError> {
    let context = state.resolve_project_context(&request.project_id, &request.project_root_path)?;
    state.lint_service.read_lint_history_report(&context, &request.id)
}
```

Register both commands in `src-tauri/src/lib.rs`:

```rust
commands::lint_commands::list_lint_history,
commands::lint_commands::read_lint_history_report,
```

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: PASS.

---

## Task 3: Lint History Frontend Restore UX

**Files:**
- Modify: `src/types/lint.ts`
- Modify: `src/stores/lintStore.ts`
- Create: `src/features/lint/LintHistoryList.tsx`
- Modify: `src/features/lint/LintView.tsx`
- Modify: `src/features/lint/lintView.test.tsx`
- Modify: `src/stores/lintStore.test.ts`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles.css`

- [ ] **Step 1: Add TypeScript DTOs**

Modify `src/types/lint.ts`:

```ts
export type LintReportKind = "local" | "deep";

export interface LintHistoryEntry {
  id: string;
  kind: LintReportKind;
  createdAt: string;
  issueCount: number;
  errorCount: number;
  warningCount: number;
  infoCount: number;
  scannedPages?: number | null;
  taskId?: string | null;
  route?: LintRoutePreference | null;
}

export interface LintHistoryFile {
  version: number;
  entries: LintHistoryEntry[];
}

export interface PersistedLintReport {
  entry: LintHistoryEntry;
  localReport?: LintReport | null;
  deepReport?: DeepLintReport | null;
}

export interface ListLintHistoryRequest {
  projectId: string;
  projectRootPath: string;
}

export interface ReadLintHistoryReportRequest {
  projectId: string;
  projectRootPath: string;
  id: string;
}
```

Run: `npm run test -- src/features/lint/lintView.test.tsx`  
Expected: PASS because the new types are unused.

- [ ] **Step 2: Extend `lintStore`**

In `src/stores/lintStore.ts`, import the new types and extend `LintState`:

```ts
history: LintHistoryEntry[];
historyLoading: boolean;
historyError: string | null;
activeHistoryId: string | null;
loadHistory: (request: ListLintHistoryRequest) => Promise<LintHistoryEntry[]>;
openHistoryReport: (request: ReadLintHistoryReportRequest) => Promise<PersistedLintReport | null>;
```

Add initial state:

```ts
history: [],
historyLoading: false,
historyError: null,
activeHistoryId: null,
```

Add actions:

```ts
loadHistory: async (request) => {
  if (!hasTauri()) return [];
  const scope = captureProjectScope();
  set({ historyLoading: true, historyError: null });
  try {
    const file = await invoke<LintHistoryFile>("list_lint_history", { request });
    if (!isProjectScopeCurrent(scope)) return [];
    const history = file.entries ?? [];
    set({ history, historyLoading: false });
    return history;
  } catch (error) {
    if (!isProjectScopeCurrent(scope)) return [];
    set({ historyLoading: false, historyError: errorMessage(error) });
    return [];
  }
},

openHistoryReport: async (request) => {
  if (!hasTauri()) return null;
  const scope = captureProjectScope();
  set({ historyError: null });
  try {
    const persisted = await invoke<PersistedLintReport>("read_lint_history_report", { request });
    if (!isProjectScopeCurrent(scope)) return null;
    set({
      localReport: persisted.localReport ?? null,
      deepReport: persisted.deepReport ?? null,
      selectedIssueId: null,
      activeHistoryId: persisted.entry.id,
      mode: persisted.entry.kind === "local" ? "local" : "agent",
    });
    return persisted;
  } catch (error) {
    if (!isProjectScopeCurrent(scope)) return null;
    set({ historyError: errorMessage(error) });
    return null;
  }
},
```

Update `runLocalLint` success branch:

```ts
set({ localReport: report, deepReport: null, activeHistoryId: null, loadingLocal: false });
void get().loadHistory({ projectId, projectRootPath: rootPath });
```

Update `loadDeepReport` success branch:

```ts
set({ deepReport: report, localReport: null, activeHistoryId: request.taskId, runningDeep: false });
void get().loadHistory({ projectId: request.projectId, projectRootPath: request.projectRootPath });
```

Run: `npm run test -- src/stores/lintStore.test.ts src/features/lint/lintView.test.tsx`  
Expected: PASS after updating tests for the new state.

- [ ] **Step 3: Create `LintHistoryList`**

Create `src/features/lint/LintHistoryList.tsx`:

```tsx
import { Clock3, FileSearch, ShieldCheck, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { LintHistoryEntry } from "../../types/lint";

interface LintHistoryListProps {
  entries: LintHistoryEntry[];
  activeId: string | null;
  loading: boolean;
  error: string | null;
  onOpen: (id: string) => void;
}

function formatHistoryTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

export function LintHistoryList({
  entries,
  activeId,
  loading,
  error,
  onOpen,
}: LintHistoryListProps) {
  const { t } = useTranslation();

  return (
    <section className="lint-history" aria-label={t("lint.history.title")}>
      <header className="lint-history__head">
        <span>{t("lint.history.title")}</span>
        {loading ? <span className="text-[11px] text-[var(--text-muted)]">{t("lint.history.loading")}</span> : null}
      </header>
      {error ? (
        <div className="lint-history__error" role="status">
          <TriangleAlert size={13} aria-hidden />
          <span>{error}</span>
        </div>
      ) : null}
      {entries.length === 0 && !loading ? (
        <div className="lint-history__empty">{t("lint.history.empty")}</div>
      ) : (
        <div className="lint-history__list">
          {entries.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className={`lint-history__row ${activeId === entry.id ? "is-active" : ""}`}
              onClick={() => onOpen(entry.id)}
            >
              {entry.kind === "local" ? <ShieldCheck size={14} aria-hidden /> : <FileSearch size={14} aria-hidden />}
              <span className="lint-history__copy">
                <span className="lint-history__main">
                  {t(`lint.history.kind.${entry.kind}`)}
                  <span className="lint-history__count">{entry.issueCount}</span>
                </span>
                <span className="lint-history__meta">
                  <Clock3 size={11} aria-hidden />
                  {formatHistoryTime(entry.createdAt)}
                </span>
              </span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
```

Add CSS:

```css
.lint-history {
  border-bottom: 1px solid var(--border);
  background: var(--surface);
}

.lint-history__head {
  display: flex;
  min-height: 32px;
  align-items: center;
  justify-content: space-between;
  gap: var(--sp-2);
  padding: 0 var(--sp-4);
  font-size: 10.5px;
  font-weight: 500;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-muted);
}

.lint-history__list {
  display: grid;
  max-height: 132px;
  overflow-y: auto;
}

.lint-history__row {
  display: flex;
  min-height: 36px;
  min-width: 0;
  align-items: center;
  gap: var(--sp-2);
  padding: var(--sp-2) var(--sp-4);
  border-top: 1px solid var(--border-subtle);
  color: var(--text-secondary);
  text-align: left;
}

.lint-history__row:hover,
.lint-history__row.is-active {
  background: var(--surface-muted);
  color: var(--text-primary);
}

.lint-history__copy {
  display: grid;
  min-width: 0;
  gap: 2px;
}

.lint-history__main,
.lint-history__meta {
  display: flex;
  min-width: 0;
  align-items: center;
  gap: 6px;
  font-size: 12px;
}

.lint-history__meta {
  font-family: var(--font-mono);
  font-size: 10.5px;
  color: var(--text-muted);
}

.lint-history__count {
  border-radius: var(--radius-pill);
  background: var(--surface-raised);
  padding: 1px 6px;
  font-family: var(--font-mono);
  font-size: 10.5px;
}

.lint-history__empty,
.lint-history__error {
  padding: var(--sp-2) var(--sp-4) var(--sp-3);
  font-size: 12px;
  color: var(--text-muted);
}

.lint-history__error {
  display: flex;
  gap: var(--sp-2);
  color: var(--danger);
}
```

- [ ] **Step 4: Integrate history into `LintView`**

In `LintView.tsx`, read state/actions:

```tsx
const history = useLintStore((state) => state.history);
const historyLoading = useLintStore((state) => state.historyLoading);
const historyError = useLintStore((state) => state.historyError);
const activeHistoryId = useLintStore((state) => state.activeHistoryId);
const loadHistory = useLintStore((state) => state.loadHistory);
const openHistoryReport = useLintStore((state) => state.openHistoryReport);
```

Add mount effect:

```tsx
useEffect(() => {
  let cancelled = false;
  void loadHistory({ projectId, projectRootPath: rootPath }).then((entries) => {
    if (cancelled || localReport || deepReport) return;
    const latest = entries[0];
    if (latest) {
      void openHistoryReport({ projectId, projectRootPath: rootPath, id: latest.id });
    }
  });
  return () => {
    cancelled = true;
  };
}, [projectId, rootPath, loadHistory, openHistoryReport]);
```

Render below the toolbar:

```tsx
<LintHistoryList
  entries={history}
  activeId={activeHistoryId}
  loading={historyLoading}
  error={historyError}
  onOpen={(id) => void openHistoryReport({ projectId, projectRootPath: rootPath, id })}
/>
```

Run: `npm run test -- src/features/lint/lintView.test.tsx`  
Expected: update existing empty-state tests to account for the history section, then PASS.

- [ ] **Step 5: Add i18n keys**

English:

```json
{
  "lint.history.title": "History",
  "lint.history.loading": "Loading",
  "lint.history.empty": "No lint reports yet.",
  "lint.history.kind.local": "Local lint",
  "lint.history.kind.deep": "Deep lint"
}
```

Chinese:

```json
{
  "lint.history.title": "历史记录",
  "lint.history.loading": "加载中",
  "lint.history.empty": "还没有 Lint 报告。",
  "lint.history.kind.local": "本地检查",
  "lint.history.kind.deep": "深度检查"
}
```

- [ ] **Step 6: Add frontend history tests**

Extend `src/features/lint/lintView.test.tsx`:

```tsx
it("loads lint history and opens the latest report on mount", async () => {
  const invokeMock = (await import("@tauri-apps/api/core")).invoke as unknown as ReturnType<typeof vi.fn>;
  invokeMock.mockImplementation((command: string) => {
    if (command === "list_lint_ignores") return Promise.resolve({ ignored: [] });
    if (command === "list_lint_history") {
      return Promise.resolve({
        version: 1,
        entries: [
          {
            id: "local-1",
            kind: "local",
            createdAt: "2026-07-04T00:00:00Z",
            issueCount: 1,
            errorCount: 1,
            warningCount: 0,
            infoCount: 0,
            scannedPages: 3,
            taskId: null,
            route: null,
          },
        ],
      });
    }
    if (command === "read_lint_history_report") {
      return Promise.resolve({
        entry: {
          id: "local-1",
          kind: "local",
          createdAt: "2026-07-04T00:00:00Z",
          issueCount: 1,
          errorCount: 1,
          warningCount: 0,
          infoCount: 0,
        },
        localReport: {
          issues: [],
          generatedAt: "2026-07-04T00:00:00Z",
          scannedPages: 3,
        },
        deepReport: null,
      });
    }
    return Promise.resolve({ ignored: [] });
  });
  useLintStore.getState().reset();
  useProjectStore.setState({ currentProject: PROJECT } as never);

  render(<LintView />);

  expect(await screen.findByRole("button", { name: /Local lint/i })).toBeInTheDocument();
  await vi.waitFor(() => expect(useLintStore.getState().localReport?.scannedPages).toBe(3));
});
```

Add a corrupt-report test:

```tsx
it("keeps the history list visible when one history report cannot be opened", async () => {
  const invokeMock = (await import("@tauri-apps/api/core")).invoke as unknown as ReturnType<typeof vi.fn>;
  invokeMock.mockImplementation((command: string) => {
    if (command === "list_lint_ignores") return Promise.resolve({ ignored: [] });
    if (command === "list_lint_history") {
      return Promise.resolve({
        version: 1,
        entries: [{
          id: "bad",
          kind: "local",
          createdAt: "2026-07-04T00:00:00Z",
          issueCount: 1,
          errorCount: 1,
          warningCount: 0,
          infoCount: 0,
        }],
      });
    }
    if (command === "read_lint_history_report") return Promise.reject({ message: "bad json" });
    return Promise.resolve({ ignored: [] });
  });

  useLintStore.getState().reset();
  useProjectStore.setState({ currentProject: PROJECT } as never);
  render(<LintView />);

  const row = await screen.findByRole("button", { name: /Local lint/i });
  fireEvent.click(row);
  expect(await screen.findByRole("status")).toHaveTextContent("bad json");
});
```

Run:

```powershell
npm run test -- src/features/lint/lintView.test.tsx src/stores/lintStore.test.ts
```

Expected: PASS.

---

## Task 4: Native Directory Picker and New Project Parent Path

**Files:**
- Modify: `src/features/import/nativeFilePicker.ts`
- Modify: `src/features/import/nativeFilePicker.test.ts`
- Create: `src/features/project/projectPath.ts`
- Create: `src/features/project/projectPath.test.ts`
- Modify: `src/features/project/ProjectStartView.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src-tauri/capabilities/main.json` only if permission is absent

- [ ] **Step 1: Extend native picker tests**

Modify `src/features/import/nativeFilePicker.test.ts`:

```ts
import { normalizeSelectedPaths, pickDirectory, selectImportFiles } from "./nativeFilePicker";

describe("pickDirectory", () => {
  it("opens the native dialog in single-directory mode", async () => {
    const open = vi.fn().mockResolvedValue("D:\\资料库");

    await expect(pickDirectory({ title: "Choose folder" }, open)).resolves.toBe("D:\\资料库");
    expect(open).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
      title: "Choose folder",
    });
  });

  it("returns null when directory selection is cancelled", async () => {
    const open = vi.fn().mockResolvedValue(null);

    await expect(pickDirectory({}, open)).resolves.toBeNull();
  });
});
```

Run: `npm run test -- src/features/import/nativeFilePicker.test.ts`  
Expected: FAIL because `pickDirectory` does not exist.

- [ ] **Step 2: Implement `pickDirectory`**

Modify `src/features/import/nativeFilePicker.ts`:

```ts
export interface PickDirectoryOptions {
  title?: string;
}

export async function pickDirectory(
  options: PickDirectoryOptions = {},
  openDialog?: OpenDialog,
): Promise<string | null> {
  const open = openDialog ?? (await import("@tauri-apps/plugin-dialog")).open;
  const selection = await open({
    directory: true,
    multiple: false,
    ...(options.title ? { title: options.title } : {}),
  });
  const paths = normalizeSelectedPaths(selection);
  return paths[0] ?? null;
}
```

Run: `npm run test -- src/features/import/nativeFilePicker.test.ts`  
Expected: PASS.

- [ ] **Step 3: Add project path helper tests**

Create `src/features/project/projectPath.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { buildProjectRootPath, sanitizeProjectFolderName } from "./projectPath";

describe("sanitizeProjectFolderName", () => {
  it("keeps CJK names and trims whitespace", () => {
    expect(sanitizeProjectFolderName("  知识库 项目  ")).toBe("知识库 项目");
  });

  it("removes path separators and Windows-invalid filename characters", () => {
    expect(sanitizeProjectFolderName("agent/wiki:2026?")).toBe("agentwiki2026");
  });

  it("returns an empty string when no valid folder characters remain", () => {
    expect(sanitizeProjectFolderName("///:::")).toBe("");
  });
});

describe("buildProjectRootPath", () => {
  it("joins Windows parent paths with backslashes", () => {
    expect(buildProjectRootPath("D:\\资料", "知识库")).toBe("D:\\资料\\知识库");
  });

  it("joins POSIX parent paths with slashes", () => {
    expect(buildProjectRootPath("/Users/aletta/wiki", "agent")).toBe("/Users/aletta/wiki/agent");
  });

  it("does not duplicate trailing separators", () => {
    expect(buildProjectRootPath("D:\\资料\\", "知识库")).toBe("D:\\资料\\知识库");
    expect(buildProjectRootPath("/tmp/wiki/", "agent")).toBe("/tmp/wiki/agent");
  });
});
```

Run: `npm run test -- src/features/project/projectPath.test.ts`  
Expected: FAIL because `projectPath.ts` does not exist.

- [ ] **Step 4: Implement `projectPath.ts`**

Create `src/features/project/projectPath.ts`:

```ts
const INVALID_FOLDER_CHARS = /[<>:"/\\|?*\u0000-\u001f]/g;

export function sanitizeProjectFolderName(value: string): string {
  return value
    .trim()
    .replace(INVALID_FOLDER_CHARS, "")
    .replace(/[. ]+$/g, "")
    .trim();
}

export function buildProjectRootPath(parentPath: string, projectName: string): string {
  const parent = parentPath.trim();
  const folder = sanitizeProjectFolderName(projectName);
  if (!parent || !folder) return "";
  const separator = parent.includes("\\") && !parent.includes("/") ? "\\" : "/";
  return `${parent.replace(/[\\/]+$/g, "")}${separator}${folder}`;
}
```

Run:

```powershell
npm run test -- src/features/project/projectPath.test.ts src/features/import/nativeFilePicker.test.ts
```

Expected: PASS.

- [ ] **Step 5: Rewrite `NewProjectDialog` state**

Modify `src/features/project/ProjectStartView.tsx` imports:

```tsx
import { pickDirectory } from "../import/nativeFilePicker";
import { buildProjectRootPath, sanitizeProjectFolderName } from "./projectPath";
```

Change `NewProjectDialog` local state:

```tsx
const [name, setName] = useState("");
const [parentPath, setParentPath] = useState("");
const rootPath = buildProjectRootPath(parentPath, name);
const folderName = sanitizeProjectFolderName(name);
const canCreate = Boolean(name.trim() && parentPath.trim() && folderName && rootPath);
```

Add browse handler:

```tsx
const chooseParent = async () => {
  try {
    const selected = await pickDirectory({ title: t("launch.dialog.chooseParent") });
    if (selected) setParentPath(selected);
  } catch (error) {
    setParentPath("");
  }
};
```

Replace the full root path input with read-only parent path plus browse button:

```tsx
<div className="input-group">
  <span className="input-group__lead"><FolderOpen size={14} aria-hidden="true" /></span>
  <input
    className="input input--mono"
    readOnly
    value={parentPath}
    placeholder={t("launch.dialog.parentPlaceholder")}
    aria-label={t("launch.dialog.parent")}
  />
  <span className="input-group__trail">
    <button type="button" className="btn btn--sm btn--ghost" onClick={chooseParent}>
      {t("launch.dialog.browse")}
    </button>
  </span>
</div>
{rootPath ? (
  <div className="project-path-preview" aria-label={t("launch.dialog.fullPath")}>
    {rootPath}
  </div>
) : null}
```

Submit with:

```tsx
if (!canCreate) return;
onCreate({ rootPath, name: name.trim(), template, initGit });
```

Run: `npm run test -- src/app/App.test.tsx src/features/project/projectPath.test.ts`  
Expected: update App tests for the changed dialog fields, then PASS.

- [ ] **Step 6: Add i18n keys**

English:

```json
{
  "launch.dialog.parent": "Parent folder",
  "launch.dialog.parentPlaceholder": "Choose a parent folder...",
  "launch.dialog.chooseParent": "Choose where to save the project",
  "launch.dialog.browse": "Browse",
  "launch.dialog.fullPath": "Project path",
  "launch.dialog.locationHint": "Choose a parent folder. The app creates the project folder from the project name."
}
```

Chinese:

```json
{
  "launch.dialog.parent": "父目录",
  "launch.dialog.parentPlaceholder": "选择父目录...",
  "launch.dialog.chooseParent": "选择项目保存位置",
  "launch.dialog.browse": "浏览",
  "launch.dialog.fullPath": "将创建的项目路径",
  "launch.dialog.locationHint": "选择父目录，应用会用项目名称创建最终项目文件夹。"
}
```

- [ ] **Step 7: Add CSS for generated path preview**

Add to `src/styles.css`:

```css
.project-path-preview {
  margin-top: var(--sp-2);
  overflow-wrap: anywhere;
  border-radius: var(--radius-md);
  background: var(--surface-muted);
  padding: var(--sp-2) var(--sp-3);
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--text-muted);
}
```

Run:

```powershell
npm run test -- src/features/project/projectPath.test.ts src/features/import/nativeFilePicker.test.ts src/app/App.test.tsx
```

Expected: PASS.

---

## Task 5: Compact Project Start Page and Enriched Recents

**Files:**
- Modify: `src/features/project/ProjectStartView.tsx`
- Modify: `src/stores/projectStore.ts`
- Modify: `src/types/project.ts`
- Modify: `src-tauri/src/models/project.rs`
- Modify: `src-tauri/src/services/project_service.rs`
- Modify: `src-tauri/src/commands/project_commands.rs`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles.css`
- Test: `src/app/App.test.tsx`, Rust project tests

- [ ] **Step 1: Expand RecentProject DTOs**

Modify `src/types/project.ts`:

```ts
export interface RecentProject {
  projectId: string;
  name: string;
  rootPath: string;
  template: ProjectTemplate;
  openedAt: string;
  wikiPageCount: number;
  sourceCount: number;
  taskCount: number;
  indexState: IndexState;
  graphState: GraphState;
  missing: boolean;
}
```

Update `src-tauri/src/models/project.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub project_id: String,
    pub name: String,
    pub root_path: String,
    #[serde(default)]
    pub template: ProjectTemplate,
    pub opened_at: String,
    #[serde(default)]
    pub wiki_page_count: usize,
    #[serde(default)]
    pub source_count: usize,
    #[serde(default)]
    pub task_count: usize,
    #[serde(default)]
    pub index_state: IndexState,
    #[serde(default)]
    pub graph_state: GraphState,
    #[serde(default)]
    pub missing: bool,
}
```

Implement `Default` for `IndexState` and `GraphState`:

```rust
impl Default for IndexState {
    fn default() -> Self {
        Self::Missing
    }
}

impl Default for GraphState {
    fn default() -> Self {
        Self::Missing
    }
}
```

Add serde test:

```rust
#[test]
fn recent_project_legacy_json_defaults_summary_fields() {
    let raw = r#"{
        "projectId":"p",
        "name":"Project",
        "rootPath":"D:/wiki",
        "template":"general",
        "openedAt":"2026-07-04T00:00:00Z"
    }"#;
    let recent: RecentProject = serde_json::from_str(raw).unwrap();
    assert_eq!(recent.wiki_page_count, 0);
    assert_eq!(recent.index_state, IndexState::Missing);
    assert!(!recent.missing);
}
```

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: PASS after DTO defaults compile.

- [ ] **Step 2: Enrich recent projects in `ProjectService`**

Modify `list_recent_projects`:

```rust
pub fn list_recent_projects(&self) -> Result<Vec<RecentProject>, BackendError> {
    let path = self.recent_projects_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let store = FileStore;
    match store.read_json_file::<RecentProjectsFile>(&path) {
        Ok(file) => Ok(file
            .projects
            .into_iter()
            .map(|entry| self.enrich_recent_project(entry))
            .collect()),
        Err(_) => Ok(Vec::new()),
    }
}
```

Add:

```rust
fn enrich_recent_project(&self, mut entry: RecentProject) -> RecentProject {
    let root = PathBuf::from(&entry.root_path);
    if !root.exists() {
        entry.missing = true;
        entry.wiki_page_count = 0;
        entry.source_count = 0;
        entry.task_count = 0;
        entry.index_state = IndexState::Missing;
        entry.graph_state = GraphState::Missing;
        return entry;
    }
    let context = ProjectContext::new(entry.project_id.clone(), root);
    let summary = self.scan_project(&context, Some(&entry.name));
    entry.name = summary.name;
    entry.root_path = summary.root_path;
    entry.template = summary.template;
    entry.wiki_page_count = summary.wiki_page_count;
    entry.source_count = summary.source_count;
    entry.task_count = summary.task_count;
    entry.index_state = summary.index_state;
    entry.graph_state = summary.graph_state;
    entry.missing = false;
    entry
}
```

Update `remember_recent_project` call sites to provide `missing: false` and summary fields. In command code, use the just-created/opened `summary`:

```rust
RecentProject {
    project_id: summary.project_id.clone(),
    name: summary.name.clone(),
    root_path: summary.root_path.clone(),
    template: summary.template,
    opened_at: now_rfc3339(),
    wiki_page_count: summary.wiki_page_count,
    source_count: summary.source_count,
    task_count: summary.task_count,
    index_state: summary.index_state.clone(),
    graph_state: summary.graph_state.clone(),
    missing: false,
}
```

Add Rust tests:

```rust
#[test]
fn list_recent_projects_marks_missing_paths_without_deleting_them() {
    let (service, config) = service_in_temp();
    let missing = config.join("missing-project");
    service.remember_recent_project(RecentProject {
        project_id: "missing".into(),
        name: "Missing".into(),
        root_path: missing.to_string_lossy().to_string(),
        template: ProjectTemplate::General,
        opened_at: "2026-07-04T00:00:00Z".into(),
        wiki_page_count: 0,
        source_count: 0,
        task_count: 0,
        index_state: IndexState::Missing,
        graph_state: GraphState::Missing,
        missing: false,
    }).unwrap();

    let listed = service.list_recent_projects().unwrap();
    assert_eq!(listed[0].project_id, "missing");
    assert!(listed[0].missing);
    assert!(service.recent_projects_path().exists());
    fs::remove_dir_all(config).ok();
}
```

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: PASS.

- [ ] **Step 3: Replace launch quick actions**

Modify `ProjectStartView.tsx`:

- Remove `openPath` state.
- Remove the manual `<form>` with `id="launch-open-input"`.
- Remove the "Import materials into existing project" quickaction note.
- Keep search/filter and right setup panel.
- Add three action handlers:

```tsx
const [pendingLaunchIntent, setPendingLaunchIntent] = useState<"open_existing" | "open_folder_as_project" | null>(null);

const chooseAndOpenProject = async (intent: "open_existing" | "open_folder_as_project") => {
  setPendingLaunchIntent(intent);
  const selected = await pickDirectory({
    title: t(intent === "open_existing" ? "launch.quick.openExistingPicker" : "launch.quick.openFolderPicker"),
  });
  if (!selected) return;
  await run(() => openProject(selected));
};
```

Render actions:

```tsx
<button type="button" className="quickaction" onClick={() => setNewDialogOpen(true)}>
  <span className="quickaction__icon"><FolderPlus size={20} aria-hidden="true" /></span>
  <h3 className="quickaction__title">{t("launch.quick.new")}</h3>
  <p className="quickaction__desc">{t("launch.quick.newDesc")}</p>
</button>
<button type="button" className="quickaction" onClick={() => void chooseAndOpenProject("open_folder_as_project")}>
  <span className="quickaction__icon"><FolderOpen size={20} aria-hidden="true" /></span>
  <h3 className="quickaction__title">{t("launch.quick.openFolderAsProject")}</h3>
  <p className="quickaction__desc">{t("launch.quick.openFolderAsProjectDesc")}</p>
</button>
<button type="button" className="quickaction" onClick={() => void chooseAndOpenProject("open_existing")}>
  <span className="quickaction__icon"><FolderOpen size={20} aria-hidden="true" /></span>
  <h3 className="quickaction__title">{t("launch.quick.openExisting")}</h3>
  <p className="quickaction__desc">{t("launch.quick.openExistingDesc")}</p>
</button>
```

Decorate the pending confirmation copy:

```tsx
const pendingActionForDisplay = pendingAction && pendingLaunchIntent === "open_existing"
  ? {
      ...pendingAction,
      title: t("launch.confirm.notExistingTitle"),
      message: t("launch.confirm.notExistingMessage"),
    }
  : pendingAction;
```

Pass `pendingActionForDisplay` to `ConfirmationDialog`; keep `confirmPendingAction` using the backend action id.

Run: `npm run test -- src/app/App.test.tsx`  
Expected: update tests for the three action labels and absence of the manual textbox, then PASS.

- [ ] **Step 4: Render enriched recent project metadata**

Modify recent card content:

```tsx
<div className="projcard__meta">
  <span className="pill pill--active">{t(`launch.filter.${templateOf(entry)}`)}</span>
  {entry.missing ? <span className="badge badge--danger">{t("launch.recent.missing")}</span> : null}
  <span>{t("launch.recent.pages", { count: entry.wikiPageCount })}</span>
  <span>·</span>
  <span>{t("launch.recent.sources", { count: entry.sourceCount })}</span>
  <span>·</span>
  <span>{t(`status.indexState.${entry.indexState}`)}</span>
  <span>·</span>
  <span>{t(`launch.recent.graphState.${entry.graphState}`)}</span>
  <span>·</span>
  <span>{relativeTime(entry.openedAt, t)}</span>
</div>
```

Disable opening missing recents and show a clear title:

```tsx
disabled={initializing || busy || entry.missing}
title={entry.missing ? t("launch.recent.missingTitle") : entry.rootPath}
```

Add graph-state keys:

```json
{
  "launch.recent.pages": "{{count}} pages",
  "launch.recent.sources": "{{count}} sources",
  "launch.recent.missing": "Missing",
  "launch.recent.missingTitle": "This project folder was not found.",
  "launch.recent.graphState.cached": "Graph cached",
  "launch.recent.graphState.stale": "Graph stale",
  "launch.recent.graphState.missing": "Graph missing"
}
```

Chinese:

```json
{
  "launch.recent.pages": "{{count}} 页",
  "launch.recent.sources": "{{count}} 来源",
  "launch.recent.missing": "失效",
  "launch.recent.missingTitle": "找不到这个项目文件夹。",
  "launch.recent.graphState.cached": "图谱已缓存",
  "launch.recent.graphState.stale": "图谱待刷新",
  "launch.recent.graphState.missing": "图谱缺失"
}
```

Run: `npm run test -- src/app/App.test.tsx`  
Expected: PASS.

- [ ] **Step 5: Add launch UX regression tests**

Update `src/app/App.test.tsx` start-flow test:

```tsx
expect(screen.getByRole("button", { name: "New empty project" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "Open folder as project" })).toBeInTheDocument();
expect(screen.getByRole("button", { name: "Open existing project" })).toBeInTheDocument();
expect(screen.queryByRole("textbox", { name: /project path|open path|local file/i })).not.toBeInTheDocument();
expect(screen.queryByText(/Import materials into existing project/i)).not.toBeInTheDocument();
```

Mock directory picker:

```ts
const openDialogMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: openDialogMock,
}));
```

Add:

```tsx
it("opens existing projects through the native directory picker", async () => {
  useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", rootPath: "" }));
  openDialogMock.mockResolvedValue("D:\\知识库\\agent");
  invokeMock.mockResolvedValue({ kind: "opened", summary: sampleProject({ rootPath: "D:/知识库/agent" }) });

  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "Open existing project" }));

  await waitFor(() => expect(openDialogMock).toHaveBeenCalledWith(expect.objectContaining({ directory: true, multiple: false })));
  expect(invokeMock).toHaveBeenCalledWith("open_project", { request: { path: "D:\\知识库\\agent" } });
});
```

Add new project dialog test:

```tsx
it("creates a project from a parent folder and project name", async () => {
  useProjectStore.getState().setCurrentProject(sampleProject({ projectId: "", rootPath: "" }));
  openDialogMock.mockResolvedValue("D:\\资料库");
  invokeMock.mockResolvedValue(sampleProject({ rootPath: "D:/资料库/中文知识库", name: "中文知识库" }));

  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "New empty project" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Project name" }), { target: { value: "中文知识库" } });
  fireEvent.click(screen.getByRole("button", { name: "Browse" }));
  await screen.findByText("D:\\资料库\\中文知识库");
  fireEvent.click(screen.getByRole("button", { name: "Create project" }));

  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("create_project", {
    request: {
      rootPath: "D:\\资料库\\中文知识库",
      name: "中文知识库",
      template: "general",
    },
  }));
});
```

Run:

```powershell
npm run test -- src/app/App.test.tsx src/features/project/projectPath.test.ts src/features/import/nativeFilePicker.test.ts
```

Expected: PASS.

- [ ] **Step 6: Add CSS contract tests**

Update `src/test/ui-css-contracts.test.ts`:

```ts
it("defines launch metadata and generated project path styles", () => {
  expect(styles).toContain(".project-path-preview");
  expect(styles).toContain(".projcard__meta");
  expect(styles).toContain(".quickaction");
});
```

Run: `npm run test -- src/test/ui-css-contracts.test.ts`  
Expected: PASS.

---

## Task 6: Regression Coverage, Review, and Quality Gates

**Files:**
- Modify tests listed in prior tasks
- Modify: `SPEC/progress.txt`
- Modify: `SPEC/gotchas.txt` only if a subtle or recurring issue is hit

- [ ] **Step 1: Run focused frontend tests**

Run:

```powershell
npm run test -- src/components/app/taskSort.test.ts src/components/app/TaskLogDrawer.test.tsx src/features/import/nativeFilePicker.test.ts src/features/project/projectPath.test.ts src/features/lint/lintView.test.tsx src/stores/lintStore.test.ts src/app/App.test.tsx src/test/ui-css-contracts.test.ts
```

Expected: PASS.

- [ ] **Step 2: Run full frontend checks**

Run:

```powershell
npm run test
npm run lint
```

Expected: PASS.

- [ ] **Step 3: Verify imports and TypeScript build**

Run:

```powershell
npm run build
```

Expected: PASS. This validates TypeScript import paths and Vite module resolution.

- [ ] **Step 4: Confirm no unintended console logging remains**

Because `rg.exe` is blocked on this machine, use PowerShell:

```powershell
Get-ChildItem -LiteralPath src -Recurse -File | Select-String -Pattern 'console\.log'
```

Expected: no matches.

- [ ] **Step 5: Run Rust validation**

Run:

```powershell
cargo check --manifest-path src-tauri/Cargo.toml --lib --tests
```

Expected: PASS. Optional `cargo test` is allowed after `cargo check`, but a Windows loader `0xc0000139` failure is a known environment issue recorded in `SPEC/gotchas.txt`.

- [ ] **Step 6: Review workflow**

After implementation:

- Launch Subagent A with shared context to review design intent, logic, consistency, and integration with `docs/fixes/05-project-task-health-ux.md`.
- Launch Subagent B with fresh context to review blind spots, missing tests, unclear behavior, and regression risk.
- If subagents are unavailable, perform both reviews manually and label them "shared-context review" and "fresh-context review".
- Fix valid issues.
- Rerun `npm run test`, `npm run lint`, `npm run build`, and `cargo check --manifest-path src-tauri/Cargo.toml --lib --tests`.

- [ ] **Step 7: Progress logging**

Append a new record to `SPEC/progress.txt` after implementation lands:

```text
[2026-07-04] Project/Task/Health UX — Implemented task timeline sorting, durable Lint history, compact launch actions, enriched recents, and native directory picking — Lint snapshots persist under `.app/lint-history.json` + `.app/lint-reports/`; launch page no longer accepts hand-written project paths.
```

Only add `SPEC/gotchas.txt` entries for subtle or recurring errors.

---

## Acceptance Criteria

### Task Log Sorting

- WHEN the user opens the task drawer THEN the system SHALL show tasks ordered by execution time descending using `startedAt ?? updatedAt`.
- WHEN a legacy task has no valid `startedAt` THEN the system SHALL sort that task by `updatedAt` in execution-time mode.
- WHEN the user switches the drawer sort to "Updated" THEN the system SHALL show tasks ordered by `updatedAt` descending.
- WHEN the user switches the drawer sort to "Status" THEN the system SHALL group tasks by `TASK_STATUS_ORDER` and use `updatedAt` descending within the same status.
- WHEN a task event upserts a running or completed task THEN the system SHALL reapply the active sort mode without mutating `taskStore.tasks`.
- WHEN the user changes sort mode THEN the system SHALL preserve `selectedTaskId`.
- WHEN the selected task remains selected after sorting THEN the system SHALL continue polling logs for that task if it is not terminal.
- WHEN the user closes and reopens the task drawer in the same app session THEN the system SHALL keep the last selected sort mode.
- WHEN localStorage contains an unknown task sort value THEN the system SHALL fall back to execution-time sorting.
- WHEN Chinese or English sort labels render in the drawer header THEN the system SHALL keep labels inside the header without overlapping the close button.

### Lint History Persistence

- WHEN the user runs local Lint THEN the system SHALL write a report body under `.app/lint-reports/local-*.json`.
- WHEN the user runs local Lint THEN the system SHALL add a matching entry to `.app/lint-history.json`.
- WHEN a deep Lint task succeeds THEN the system SHALL write a deep report body under `.app/lint-reports/{task_id}.json`.
- WHEN a deep Lint task succeeds THEN the system SHALL add the deep report to the same `.app/lint-history.json` index as local reports.
- WHEN the Lint history index has more than 50 entries THEN the system SHALL keep the newest 50 entries in the index.
- WHEN old report files exist outside the newest 50 history entries THEN the system SHALL leave those files untouched in this batch.
- WHEN `.app/lint-history.json` is missing THEN the system SHALL return an empty history list.
- WHEN `.app/lint-history.json` is corrupt THEN the system SHALL not crash LintView and SHALL show an empty or error-visible history state.
- WHEN one `.app/lint-reports/{id}.json` file is corrupt THEN the system SHALL keep the history list visible and show a non-fatal error only when that report is opened.
- WHEN the user reopens a project and enters LintView THEN the system SHALL load the history list from `.app/lint-history.json`.
- WHEN the reopened project has at least one valid history report THEN the system SHALL open the newest report automatically.
- WHEN the user clicks a local history entry THEN the system SHALL show that local report's issues and summary.
- WHEN the user clicks a deep history entry THEN the system SHALL show that deep report's issues and raw-output-backed details.
- WHEN a report is persisted THEN the system SHALL not write Lint history to `wiki/` or to any database.
- WHEN report ids contain path separators or `..` THEN the backend SHALL reject the read request.

### Project Start Page

- WHEN the app starts without an active project THEN the system SHALL show a compact project selection surface, not a marketing landing page.
- WHEN the launch page renders THEN the system SHALL show exactly three primary entries: New empty project, Open folder as project, and Open existing project.
- WHEN the launch page renders THEN the system SHALL not show a manual project path textbox.
- WHEN the launch page renders THEN the system SHALL not show "Import materials into existing project" as a primary start-page entry.
- WHEN the user clicks Open existing project THEN the system SHALL open the native directory picker.
- WHEN the user selects an existing wiki-like folder through Open existing project THEN the system SHALL call `open_project` and enter the project.
- WHEN the user selects a normal folder through Open existing project THEN the system SHALL show a PendingAction confirmation instead of silently initializing it.
- WHEN the user clicks Open folder as project THEN the system SHALL open the native directory picker.
- WHEN the user selects a normal folder through Open folder as project THEN the system SHALL show the backend PendingAction confirmation before moving or organizing files.
- WHEN a PendingAction confirmation is cancelled THEN the system SHALL leave the selected folder unchanged.
- WHEN recent project metadata is available THEN the system SHALL show project name, compact path, page count, source count, index state, graph state, and last-opened time.
- WHEN a recent project path no longer exists THEN the system SHALL show a missing badge and SHALL not silently delete the row.
- WHEN recent project names or paths contain CJK characters THEN the system SHALL keep card text within its container and preserve the full path in `title`.

### New Project Directory Picker

- WHEN the user opens New Project THEN the system SHALL ask for project name, parent folder, template, and Git initialization state.
- WHEN the user chooses a parent folder THEN the system SHALL show the selected parent path in a read-only field.
- WHEN the user enters a project name after choosing a parent folder THEN the system SHALL display the generated full project path.
- WHEN the user clicks Browse in New Project THEN the system SHALL call `@tauri-apps/plugin-dialog.open` with `directory: true` and `multiple: false`.
- WHEN the user cancels the directory picker THEN the system SHALL keep the dialog open and SHALL not crash.
- WHEN the project name contains CJK characters THEN the system SHALL preserve those characters in the generated path.
- WHEN the project name contains path separators or Windows-invalid folder characters THEN the system SHALL remove those characters from the generated folder name.
- WHEN the generated folder name is empty THEN the system SHALL disable Create.
- WHEN the user clicks Create THEN the system SHALL call `create_project` with the generated `rootPath`, the trimmed display `name`, and the selected template.
- WHEN the backend rejects the generated root path THEN the system SHALL show the existing project-store error without attempting a frontend filesystem write.

### General Safety and Quality

- WHEN implementation changes frontend code THEN the system SHALL pass `npm run test` and `npm run lint`.
- WHEN implementation changes imports, DTOs, or CSS THEN the system SHALL pass `npm run build`.
- WHEN implementation changes Rust DTOs, commands, or services THEN the system SHALL pass `cargo check --manifest-path src-tauri/Cargo.toml --lib --tests`.
- WHEN checks complete THEN the system SHALL verify that no unintended `console.log` remains under `src/`.
- WHEN a subtle or recurring issue is discovered THEN the system SHALL add one entry to `SPEC/gotchas.txt` using the required symptom/root-cause/avoidance format.
- WHEN the feature lands THEN the system SHALL add a progress entry to `SPEC/progress.txt` using the required date/module/summary/decision format.

## Out Of Scope For Batch 05

- Changing task persistence format under `.app/tasks/`.
- Adding task filters beyond the three requested sort modes.
- Building a Lint history cleanup UI or deleting old report files.
- Persisting Lint reports into `wiki/`, `raw/`, `exports/`, or a database.
- Reworking Lint issue grouping, fix semantics, or Agent prompt quality beyond report history restore.
- Adding manual recent-project removal or relocation flows.
- Replacing backend project validation with frontend-only path validation.
- Editing `UI-Frontend-design/`.

## Execution Recommendation

Use Subagent-Driven execution for this batch:

1. Task 1: task sorting pure function, drawer UI, and focused tests.
2. Task 2: Lint backend history DTOs, service persistence, commands, and handler registration.
3. Task 3: Lint frontend history restore UI and store wiring.
4. Task 4: directory picker helper and new project dialog path generation.
5. Task 5: launch page entry redesign and enriched recent-project metadata.
6. Task 6: full verification, dual review, and progress logging.

This order gives each subsystem a working, testable surface before the next layer depends on it.
