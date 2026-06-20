import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
  invokeMock.mockReset();
});

afterEach(() => cleanup());

/** The workspace header is the only <header> nested inside <main>, and it owns
 *  the per-view primary/secondary action buttons (distinct from sidebar nav). */
function headerButtons() {
  const header = document.querySelector("main section > header");
  if (!header) throw new Error("workspace header not rendered");
  return within(header as HTMLElement);
}

describe("AppShell header action buttons", () => {
  it("dashboard primary action navigates to the import view", () => {
    render(<AppShell />);
    fireEvent.click(headerButtons().getByRole("button", { name: "Import" }));
    expect(useNavigationStore.getState().activeView).toBe("import");
  });

  it("chat primary action surfaces a visible toast in browser mode instead of a silent no-op", async () => {
    useNavigationStore.getState().setActiveView("chat");
    render(<AppShell />);
    // The chat header primary is "New chat" — a Tauri-only action. With no
    // __TAURI_INTERNALS__ the store call is skipped and a warning toast appears.
    fireEvent.click(headerButtons().getByRole("button", { name: "New chat" }));
    await waitFor(() => {
      expect(screen.getByRole("status")).toBeTruthy();
    });
    expect(screen.getByRole("status").textContent).toMatch(/desktop app/i);
  });
});
