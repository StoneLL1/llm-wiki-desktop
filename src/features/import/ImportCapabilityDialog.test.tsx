import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { useAppCapabilityStore } from "../../stores/appCapabilityStore";
import { useTaskStore } from "../../stores/taskStore";
import type { AppCapabilityView } from "../../types/appCapability";
import type { ImportCapabilityRequirement } from "../../types/importV2Presentation";
import { ImportCapabilityDialog } from "./ImportCapabilityDialog";

const requirement: ImportCapabilityRequirement = {
  requirement: {
    capabilityId: "browser-runtime",
    minimumVersion: "1.4.0",
    protocolVersion: "2",
    targetTriple: "x86_64-pc-windows-msvc",
    acceptedLicenseExpressions: ["Apache-2.0"],
  },
  route: "web.generic.browser",
  available: false,
  installable: true,
  compressedBytes: 12_000,
  installedBytes: 45_000,
  modelBytes: null,
  license: "Apache-2.0",
  fallback: "Install the signed pack from a release, then retry.",
  unavailableReasonCode: null,
  requirementRevision: "ab".repeat(32),
};

const globalCapability: AppCapabilityView = {
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
  currentProjectWaitingCount: 2,
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
  useAppCapabilityStore.getState().resetForTests();
  useAppCapabilityStore.setState({ capabilities: [globalCapability], initialized: true });
  useTaskStore.setState({ taskById: {}, tasks: [] });
});

describe("ImportCapabilityDialog", () => {
  it("shows exact signed-package facts and continuation behavior", () => {
    render(<ImportCapabilityDialog open requirement={requirement} onInstall={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.getByText("Interactive web reader")).toBeVisible();
    expect(screen.getByText("Read dynamic web sources")).toBeVisible();
    expect(screen.getByText("x86_64-pc-windows-msvc")).toBeVisible();
    expect(screen.getByText("llm-wiki-capability-v1")).toBeVisible();
    expect(screen.getByText("github.com")).toBeVisible();
    expect(screen.getByText(/Including this import, 2 waiting item/)).toBeVisible();
    expect(screen.getByText(/verifies the pinned version, SHA-256 digest, publisher signature/i)).toBeVisible();
  });

  it("requires explicit fact acknowledgement before registering the import install", async () => {
    const onInstall = vi.fn().mockResolvedValue(undefined);
    render(<ImportCapabilityDialog open requirement={requirement} onInstall={onInstall} onCancel={vi.fn()} />);

    const install = screen.getByRole("button", { name: "Install" });
    expect(install).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox", { name: /reviewed version 1\.4\.0/i }));
    fireEvent.click(install);
    await waitFor(() => expect(onInstall).toHaveBeenCalledWith("browser-runtime"));
  });

  it("does not manufacture an install action when the target is unpublished", () => {
    const unavailable = {
      ...globalCapability,
      installAllowed: false,
      targetVersion: undefined,
      acknowledgementVersion: undefined,
      distribution: { state: "not_published_for_target" as const },
      displayState: "not_published_for_target",
    };
    render(<ImportCapabilityDialog origin="management" open capability={unavailable} intent="details" onCancel={vi.fn()} />);

    expect(screen.queryByRole("button", { name: "Install" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeVisible();
  });

  it("reports resumed and review-required continuations from the typed task result", () => {
    const task = {
      id: "capability-task-a",
      taskType: "capability_install" as const,
      projectId: null,
      operation: {
        kind: "app_capability_install" as const,
        capabilityId: "browser-runtime",
        version: "1.4.0",
        targetTriple: "x86_64-pc-windows-msvc",
        archiveIdentity: "archive-a",
      },
      title: "Install browser runtime",
      status: "succeeded" as const,
      progress: { current: 12_000, total: 12_000, label: "capability.activating" },
      startedAt: "2026-08-30T00:00:00Z",
      updatedAt: "2026-08-30T00:01:00Z",
      completedAt: "2026-08-30T00:01:00Z",
      cancellable: false,
      logPath: null,
      result: {
        summary: "Installed browser-runtime 1.4.0",
        affectedPaths: [],
        reference: {
          type: "app_capability_install" as const,
          capabilityId: "browser-runtime",
          version: "1.4.0",
          resumedContinuations: 1,
          deferredContinuations: 1,
          failedContinuations: 1,
        },
      },
      error: null,
    };
    useTaskStore.getState().upsertTask(task);
    useAppCapabilityStore.setState({
      capabilities: [{
        ...globalCapability,
        installation: { state: "healthy", healthyVersion: "1.4.0" },
        operation: { state: "succeeded", taskId: task.id },
        activeTaskId: undefined,
        displayState: "installed",
      }],
    });

    render(<ImportCapabilityDialog open requirement={requirement} onInstall={vi.fn()} onCancel={vi.fn()} />);

    expect(screen.getByText("Capability installed. Continued 1 unchanged import item(s).")).toBeVisible();
    expect(screen.getByText(/2 item\(s\) changed or could not be resumed/)).toBeVisible();
    expect(screen.queryByText(/11\.7 KiB \/ 11\.7 KiB/)).not.toBeInTheDocument();
  });

  it("shows a stable reason code when details are unavailable", () => {
    const unavailable = {
      ...globalCapability,
      installAllowed: false,
      distribution: { state: "unsupported" as const, errorCode: "APP_CAPABILITY_UNSUPPORTED" },
      installBlockedReasonCode: "APP_CAPABILITY_UNSUPPORTED",
      displayState: "unsupported",
    };

    render(<ImportCapabilityDialog origin="management" open capability={unavailable} intent="details" onCancel={vi.fn()} />);

    expect(screen.getByText("APP_CAPABILITY_UNSUPPORTED")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("requires a different app version");
  });
});
