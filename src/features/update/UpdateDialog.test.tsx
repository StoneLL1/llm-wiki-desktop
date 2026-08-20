import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { useUpdateStore } from "../../stores/updateStore";
import { UpdateDialog } from "./UpdateDialog";

const offer = {
  offerId: "offer-1",
  currentVersion: "0.1.0",
  version: "0.2.0",
  target: "windows",
  arch: "x86_64",
  notes: "[Click me](javascript:alert(1)) <script>bad()</script>",
  publishedAt: "2026-08-21T00:00:00Z",
  createdAtUnixSeconds: 1,
  expiresAtUnixSeconds: 2,
};

beforeEach(() => {
  useUpdateStore.getState().resetForTests();
  useUpdateStore.setState({
    dialogOpen: true,
    initialized: true,
    uiStatus: "available",
    backendState: {
      phase: "available",
      offer,
      downloadedBytes: 0,
      totalBytes: null,
      error: null,
    },
  });
});

describe("UpdateDialog", () => {
  it("renders untrusted release notes as inert plain text", () => {
    render(<UpdateDialog />);

    expect(screen.getByText(offer.notes)).toBeVisible();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(document.querySelector("script")).toBeNull();
  });

  it("uses an accessible modal and Escape closes it", () => {
    render(<UpdateDialog />);
    expect(screen.getByRole("dialog", { name: "Updates" })).toBeVisible();

    fireEvent.keyDown(document, { key: "Escape" });
    expect(useUpdateStore.getState().dialogOpen).toBe(false);
  });

  it("disables explicit installation consent while a blocker remains", () => {
    useUpdateStore.setState({
      uiStatus: "ready_to_install",
      backendState: {
        phase: "downloaded",
        offer,
        downloadedBytes: 100,
        totalBytes: 100,
        error: null,
      },
      installReviewIntent: "install",
      installGuard: {
        blockers: ["unsaved_editor"],
        safeRunningTaskCount: 0,
        request: {
          unsavedEditor: true,
          importCommitActive: false,
          pendingUserConfirmation: false,
        },
      },
    });

    render(<UpdateDialog />);
    expect(screen.getByRole("button", { name: "Install and restart" })).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent("Save or discard the current wiki draft first");
  });

  it("labels download progress for screen readers", () => {
    useUpdateStore.setState({
      uiStatus: "downloading",
      backendState: {
        phase: "downloading",
        offer,
        downloadedBytes: 50,
        totalBytes: 100,
        error: null,
      },
    });

    render(<UpdateDialog />);
    expect(screen.getByRole("progressbar", { name: "Download progress" })).toHaveValue(50);
  });

  it("shows the current version as latest after an up-to-date check", () => {
    useUpdateStore.setState({
      uiStatus: "up_to_date",
      backendState: {
        phase: "idle",
        offer: null,
        downloadedBytes: 0,
        totalBytes: null,
        error: null,
      },
    });

    render(<UpdateDialog />);
    expect(screen.getAllByText("0.1.0")).toHaveLength(2);
  });
});
