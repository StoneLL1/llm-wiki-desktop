import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18next } from "../../i18n";
import { useNavigationStore } from "../../stores/navigationStore";
import { AppShell } from "./AppShell";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

beforeEach(async () => {
  await i18next.changeLanguage("en");
  useNavigationStore.getState().setActiveView("dashboard");
  useNavigationStore.getState().setRightPanelOpen(true);
  invokeMock.mockReset();
});

afterEach(() => cleanup());

function workspaceHeader() {
  const header = document.querySelector("main section > header");
  if (!header) throw new Error("workspace header not rendered");
  return within(header as HTMLElement);
}

describe("AppShell workspace header", () => {
  it("keeps only the view title and context control in shared chrome", () => {
    render(<AppShell />);

    expect(workspaceHeader().getByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
    expect(workspaceHeader().queryByText("Health · recent activity · quick actions")).not.toBeInTheDocument();
    expect(workspaceHeader().queryByRole("button", { name: "Import" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Collapse context panel" })).toBeInTheDocument();
  });

  it("does not repeat a generic settings title inside the settings section", () => {
    useNavigationStore.getState().setActiveView("settings");
    render(<AppShell />);

    expect(
      screen.queryByText("Global preferences, project-scoped provider settings, and secure key status."),
    ).not.toBeInTheDocument();
  });
});
