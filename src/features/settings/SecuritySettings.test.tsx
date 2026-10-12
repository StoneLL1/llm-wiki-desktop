import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSettingsStore } from "../../stores/settingsStore";

const invokeMock = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { SecuritySettings } from "./SecuritySettings";

const authorization = { enabled: true, confirmedAt: "2026-10-11", projectId: "p1", rootPathFingerprint: "fingerprint" };

beforeEach(() => {
  invokeMock.mockReset().mockRejectedValue(new Error("Permission could not be revoked"));
  useSettingsStore.setState({ chatConvenienceAuthorization: authorization, chatConvenienceSaving: false, error: null });
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
});

afterEach(() => { Reflect.deleteProperty(window, "__TAURI_INTERNALS__"); });

describe("security action failures", () => {
  it.each(["Revoke project", "Revoke all"])("does not report success when %s fails", async (action) => {
    render(<SecuritySettings providers={[]} chatConvenienceAuthorization={authorization} onDeleteSecret={vi.fn()}
      onRevokeChatConvenience={() => useSettingsStore.getState().setChatConvenienceAuthorization("p1", "/p1", false)}
      onRevokeAllChatConvenience={() => useSettingsStore.getState().revokeAllChatConvenienceAuthorizations()} />);
    fireEvent.click(screen.getByRole("button", { name: action }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Permission could not be revoked");
    await waitFor(() => expect(screen.getByRole("button", { name: action })).toBeEnabled());
    expect(screen.queryByText("Action completed")).not.toBeInTheDocument();
    expect(useSettingsStore.getState().chatConvenienceAuthorization?.enabled).toBe(true);
  });
});
