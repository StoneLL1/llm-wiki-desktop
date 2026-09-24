import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { i18next } from "../../i18n";
import { useImportStore } from "../../stores/importStore";
import { useTaskStore } from "../../stores/taskStore";
import type { ImportItem, ImportSession } from "../../types/importV2";
import type { ImportCapabilityRequirement } from "../../types/importV2Presentation";
import { ImportV2Dialogs } from "./ImportV2Dialogs";
import type { ImportWorkflow } from "./importWorkflow";

const entry: ImportItem = {
  itemId: "article", input: { kind: "url", displayName: "Article", locator: "https://www.zhihu.com/question/123", normalizedLocator: null },
  status: "waiting_login", selected: false, taskId: null, progress: null, attempts: [], preview: null, issue: null,
};
const session: ImportSession = { schemaVersion: 2, sessionId: "session", projectId: "project", status: "draft", resourceMode: "balanced", createdAt: "", updatedAt: "", items: [entry] };
const requirement: ImportCapabilityRequirement = {
  requirement: { capabilityId: "browser-runtime", protocolVersion: "2", targetTriple: "aarch64-apple-darwin", acceptedLicenseExpressions: ["MIT"] },
  route: "web.generic.browser", available: false, installable: true, requirementRevision: "revision", compressedBytes: 100, installedBytes: 100, modelBytes: null, license: "MIT", fallback: null, unavailableReasonCode: "not_installed",
};
function setup(overrides: Partial<ImportWorkflow> = {}) {
  const workflow = {
    projectKey: "project\0/tmp/test", session, collectionPreview: null, remoteMediaRetentionPlan: null,
    getCapabilityRequirement: vi.fn().mockResolvedValue(requirement), beginLogin: vi.fn().mockResolvedValue(null),
    getAsrEnablementPlan: vi.fn(), installCapability: vi.fn().mockResolvedValue({ id: "install-browser" }), ...overrides,
  } as unknown as ImportWorkflow;
  const props = { workflow, privateItem: null, asrItem: null, subtitleItem: null, candidateView: null, onCloseCandidate: vi.fn(), onCandidateIntent: vi.fn(), onClosePrivate: vi.fn(), onCloseAsr: vi.fn(), onCloseSubtitle: vi.fn() };
  return { workflow, props };
}
beforeEach(async () => {
  await i18next.changeLanguage("en");
  useImportStore.getState().reset();
  useImportStore.getState().attachSession("project\0/tmp/test", session);
  useTaskStore.setState({ tasks: [], taskById: {} });
});
it("keeps failed requirement queries visible and retries the same item", async () => {
  const query = vi.fn().mockRejectedValueOnce(new Error("offline")).mockResolvedValue(requirement);
  const { props } = setup({ getCapabilityRequirement: query });
  useImportStore.getState().openCapability(entry.itemId);
  render(<ImportV2Dialogs {...props} />);
  expect(await screen.findByRole("alert")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: /retry/i }));
  await waitFor(() => expect(query).toHaveBeenCalledTimes(2));
  expect(await screen.findByRole("button", { name: /prepare and continue/i })).toBeEnabled();
});
it("prepares the full browser from the login item and continues login after installation", async () => {
  const query = vi.fn().mockResolvedValueOnce(requirement).mockResolvedValue({ ...requirement, available: true });
  const { workflow, props } = setup({ getCapabilityRequirement: query });
  useImportStore.getState().openLogin(entry.itemId);
  const previousInstall = { id: "older-install", status: "succeeded", updatedAt: "2026-09-18T00:00:00Z", operation: { kind: "app_capability_install", capabilityId: "browser-runtime" } } as never;
  useTaskStore.setState({ tasks: [previousInstall] });
  render(<ImportV2Dialogs {...props} />);
  fireEvent.click(await screen.findByRole("button", { name: /prepare and continue/i }));
  await waitFor(() => expect(workflow.installCapability).toHaveBeenCalledWith("article", "browser-runtime", "revision"));
  expect(workflow.beginLogin).not.toHaveBeenCalled();
  act(() => useTaskStore.setState({ tasks: [previousInstall, { id: "install-browser", status: "succeeded", updatedAt: "2026-09-20T00:00:00Z", operation: { kind: "app_capability_install", capabilityId: "browser-runtime" } } as never] }));
  await waitFor(() => expect(workflow.beginLogin).toHaveBeenCalledWith("article", "zhihu"));
  expect(await screen.findByRole("heading", { name: /continue connector login/i })).toBeInTheDocument();
});
it("does not close ASR authorization after a failed item", async () => {
  const asrPlan = { requirementRevision: "asr", recommendedProfile: "balanced", availableMemoryBytes: null, availableDiskBytes: null, mediaDurationSeconds: null, installLocation: null, localOnly: true, profiles: [{ profile: "balanced", capabilityId: "asr-sensevoice-small", available: true, installable: false, dependencies: [], engineName: "SenseVoice", modelName: "SenseVoice", device: "cpu" }] };
  const { props } = setup({ getAsrEnablementPlan: vi.fn().mockResolvedValue(asrPlan), authorizeLocalAsr: vi.fn().mockResolvedValue(false) });
  render(<ImportV2Dialogs {...props} asrItem={entry} />);
  fireEvent.click(await screen.findByRole("button", { name: /enable and continue/i }));
  await waitFor(() => expect(props.workflow.authorizeLocalAsr).toHaveBeenCalled());
  expect(props.onCloseAsr).not.toHaveBeenCalled();
});
