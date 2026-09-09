import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import "../../i18n";
import { useProjectStore } from "../../stores/projectStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { VersionHistorySettings } from "./VersionHistorySettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mockedInvoke = vi.mocked(invoke);
const project = { projectId: "versions", rootPath: "/knowledge", name: "Knowledge" };
const summary = { operationId: "repair", kind: "lint_fix", createdAt: "2026-09-09T12:00:00Z", state: "applied", fileCount: 2, taskId: null };
const operation = { summary, before: "before", after: "after", beforeHashes: { "wiki/a.md": "a", "wiki/b.md": "b" }, expectedHashes: {} };
const calls = (name: string) => mockedInvoke.mock.calls.filter(([command]) => command === name);

beforeEach(() => {
  useProjectStore.setState({ currentProject: project, authority: null } as never);
  useNavigationStore.setState({ versionHistoryTarget: null });
  mockedInvoke.mockReset();
  mockedInvoke.mockImplementation(async (command) => {
    if (command === "get_version_history_status") return { enabled: true, git: { isRepository: true }, gitVersion: "git test" };
    if (command === "list_version_operations") return { operations: [summary], nextCursor: null, unreadableCount: 0 };
    if (command === "get_version_operation") return operation;
    if (command === "get_version_file_diff") return { beforeText: "original", afterText: "updated", binary: false, truncated: false };
    if (command === "prepare_version_action") return { id: "confirmation", affectedPaths: ["wiki/a.md", "wiki/b.md"] };
    return undefined;
  });
});

it("loads summaries first and only reads the selected file, then focuses confirmation", async () => {
  render(<VersionHistorySettings project={project as never} />);
  fireEvent.click(await screen.findByRole("button", { name: /Health check repair/ }));
  expect(calls("get_version_file_diff")).toHaveLength(0);
  expect(await screen.findByText("original")).toBeInTheDocument();
  expect(calls("get_version_file_diff")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "wiki/b.md" }));
  await waitFor(() => expect(calls("get_version_file_diff")).toHaveLength(2));
  fireEvent.click(screen.getByRole("button", { name: "Undo operation" }));
  await waitFor(() => expect(screen.getByRole("heading", { name: "Undo this operation?" })).toHaveFocus());
  expect(calls("confirm_version_action")).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("confirm_version_action", { request: { actionId: "confirmation", status: "cancelled" } }));
});

it("keeps a restore conflict read-only and exposes the error", async () => {
  mockedInvoke.mockImplementation(async (command) => {
    if (command === "get_version_operation") return operation;
    if (command === "prepare_version_action") throw { code: "VERSION_RESTORE_CONFLICT", details: { paths: ["wiki/a.md"] } };
    return null;
  });
  useNavigationStore.setState({ versionHistoryTarget: { operationId: "repair" } });
  render(<VersionHistorySettings project={project as never} />);
  fireEvent.click(await screen.findByRole("button", { name: "Undo operation" }));
  expect(await screen.findByText(/Some files have newer edits/)).toBeInTheDocument();
  expect(calls("confirm_version_action")).toHaveLength(0);
});

it("cancels a prepared action that arrives after a project switch", async () => {
  let finish!: (value: unknown) => void;
  const base = mockedInvoke.getMockImplementation()!;
  mockedInvoke.mockImplementation((command, args) => command === "prepare_version_action"
    ? new Promise((resolve) => { finish = resolve; }) as never
    : base(command, args));
  const view = render(<VersionHistorySettings project={project as never} />);
  fireEvent.click(await screen.findByRole("button", { name: "Save current version" }));
  useProjectStore.setState({ currentProject: { projectId: "other", rootPath: "/other" } } as never);
  view.unmount();
  await act(async () => { finish({ id: "stale", affectedPaths: [] }); });
  await waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("confirm_version_action", { request: { actionId: "stale", status: "cancelled" } }));
  expect(calls("confirm_version_action")).toHaveLength(1);
});

it("cancels unsubmitted confirmation when its panel closes", async () => {
  const view = render(<VersionHistorySettings project={project as never} />);
  fireEvent.click(await screen.findByRole("button", { name: "Save current version" }));
  await screen.findByRole("button", { name: "Cancel" });
  view.unmount();
  await waitFor(() => expect(mockedInvoke).toHaveBeenCalledWith("confirm_version_action", { request: { actionId: "confirmation", status: "cancelled" } }));
});
