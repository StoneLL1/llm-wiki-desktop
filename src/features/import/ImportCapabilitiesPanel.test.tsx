import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { i18next } from "../../i18n";
import { useAppCapabilityStore } from "../../stores/appCapabilityStore";
import type { AppCapabilityView } from "../../types/appCapability";
import { ImportCapabilitiesPanel } from "./ImportCapabilitiesPanel";

function capability(overrides: Partial<AppCapabilityView> = {}): AppCapabilityView {
  return {
    capabilityId: "browser-runtime",
    nameKey: "importV2.capabilityName.browser-runtime",
    purposeKey: "importV2.capabilityPurpose.web",
    category: "web",
    routes: ["web.generic.browser"],
    formats: ["html"],
    platformContentTypes: ["web_page"],
    targetTriple: "x86_64-pc-windows-msvc",
    publisherKeyId: "llm-wiki-capability-v1",
    sourceDomain: "github.com",
    targetVersion: "1.4.0",
    acknowledgementVersion: "ack-v1",
    installAllowed: true,
    distribution: { state: "published" },
    installation: { state: "absent" },
    operation: {},
    update: { state: "none" },
    displayState: "install_available",
    compressedBytes: 12_000,
    installedBytes: 45_000,
    modelBytes: 0,
    licenseExpression: "Apache-2.0",
    thirdPartyNotices: [],
    runtimeNetwork: true,
    runtimeSubprocess: true,
    runtimeFilesystem: ["app-capability-dir"],
    currentProjectWaitingCount: 0,
    ...overrides,
  };
}

beforeEach(async () => {
  await i18next.changeLanguage("en");
  useAppCapabilityStore.getState().resetForTests();
});

describe("ImportCapabilitiesPanel", () => {
  it("renders an app-global continuous inventory with summary facts", () => {
    useAppCapabilityStore.setState({
      initialized: true,
      capabilities: [
        capability(),
        capability({
          capabilityId: "ocr-cjk-accurate",
          nameKey: "importV2.capabilityName.ocr-cjk-accurate",
          purposeKey: "importV2.capabilityPurpose.ocr",
          category: "ocr",
          routes: ["document.ocr"],
          formats: ["png", "jpg"],
          installAllowed: false,
          distribution: { state: "not_published_for_target" },
          displayState: "not_published_for_target",
          targetVersion: undefined,
          acknowledgementVersion: undefined,
        }),
      ],
    });

    render(<ImportCapabilitiesPanel />);

    expect(screen.getByRole("table")).toBeVisible();
    expect(screen.getByText("Interactive web reader")).toBeVisible();
    expect(screen.getByText("CJK OCR")).toBeVisible();
    expect(screen.getByText("html · web_page · web.generic.browser")).toBeVisible();
    expect(screen.getByText("Not published for this platform")).toBeVisible();
    expect(screen.getAllByRole("button", { name: "Install" })).toHaveLength(1);
    expect(screen.getAllByRole("button", { name: "Details" })).toHaveLength(1);
    expect(screen.getByLabelText("Capability summary")).toHaveTextContent("1Installable");
    expect(screen.getByLabelText("Capability summary")).toHaveTextContent("1Target unpublished");
  });

  it("filters by category and search without changing backend state", () => {
    useAppCapabilityStore.setState({
      initialized: true,
      capabilities: [
        capability(),
        capability({
          capabilityId: "document-standard",
          nameKey: "importV2.capabilityName.document-standard",
          purposeKey: "importV2.capabilityPurpose.documents",
          category: "documents",
          routes: ["document.standard"],
          formats: ["docx"],
        }),
      ],
    });

    render(<ImportCapabilitiesPanel />);
    fireEvent.change(screen.getByLabelText("Category"), { target: { value: "documents" } });
    expect(screen.queryByText("Interactive web reader")).not.toBeInTheDocument();
    expect(screen.getByText("Standard documents")).toBeVisible();

    fireEvent.change(screen.getByLabelText("Search capability packs"), { target: { value: "missing" } });
    expect(screen.getByText("No capability packs match these filters.")).toBeVisible();
  });

  it("announces true download bytes and exposes cancellation for the global task", () => {
    useAppCapabilityStore.setState({
      initialized: true,
      capabilities: [capability({
        operation: {
          state: "downloading",
          taskId: "capability-task-a",
          progressCurrent: 6_000,
          progressTotal: 12_000,
        },
        activeTaskId: "capability-task-a",
        displayState: "downloading",
      })],
    });

    render(<ImportCapabilitiesPanel />);

    expect(screen.getByRole("progressbar", { name: "Interactive web reader: 5.9 KiB of 12 KiB downloaded" })).toHaveAttribute("value", "6000");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("Interactive web reader: Downloading 50%");
  });

  it("keeps full route coverage accessible and distinguishes installed from target versions", () => {
    const routes = ["route.one", "route.two", "route.three", "route.four", "route.five", "route.six"];
    useAppCapabilityStore.setState({
      initialized: true,
      capabilities: [capability({
        routes,
        formats: [],
        platformContentTypes: [],
        installation: { state: "healthy", healthyVersion: "1.3.0" },
        targetVersion: "1.4.0",
        update: { state: "available", availableVersion: "1.4.0" },
        operation: { state: "succeeded" },
        displayState: "update_available",
      })],
    });

    render(<ImportCapabilitiesPanel />);

    expect(screen.getByLabelText(routes.join(" · "))).toHaveTextContent("+1");
    expect(screen.getByText("installed 1.3.0 · target 1.4.0")).toBeVisible();
    expect(screen.getByText("Update available")).toBeVisible();
  });
});
