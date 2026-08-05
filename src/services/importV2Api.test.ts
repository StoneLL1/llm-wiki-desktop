import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { importV2Api } from "./importV2Api";

describe("Import V2 presentation API", () => {
  beforeEach(() => invoke.mockReset());

  it("freezes one explicit command name for every presentation operation", () => {
    expect(importV2Api.commandNames).toEqual({
      createSession: "create_import_session_v2",
      getSession: "get_import_session_v2",
      getHistorySession: "get_import_history_session_v2",
      getCompletion: "get_import_completion_v2",
      addItems: "add_import_items_v2",
      addPaths: "start_add_import_paths_v2",
      addText: "add_import_text_v2",
      getScanResult: "get_import_scan_result_v2",
      acceptScan: "accept_import_scan_v2",
      discardScan: "discard_import_scan_v2",
      addUrl: "add_import_url_v2",
      discoverCollection: "discover_import_collection_v2",
      loadCollectionPage: "load_import_collection_page_v2",
      addCollectionItems: "add_import_collection_items_v2",
      getRemoteMediaRetentionPlan: "get_remote_media_retention_plan_v2",
      confirmRemoteMediaRetention: "confirm_remote_media_retention_v2",
      setSelection: "set_import_item_selection_v2",
      startItems: "start_import_items_v2",
      startBatch: "start_import_batch_v2",
      cancelBatch: "cancel_import_batch_v2",
      cancelItem: "cancel_import_item_v2",
      skipItem: "skip_import_item_v2",
      authorizeLocalAsr: "authorize_local_asr_v2",
      authorizeLocalOcr: "authorize_local_ocr_v2",
      selectSubtitle: "select_import_subtitle_v2",
      confirmSession: "confirm_import_session_v2",
      getPreviewContent: "get_import_preview_content_v2",
      getMergeContext: "get_import_merge_context_v2",
      setItemResolution: "set_import_item_resolution_v2",
      stageManualMerge: "stage_import_manual_merge_v2",
      getReadiness: "get_import_frontend_readiness_v2",
      getWorkbenchPreferences: "get_import_workbench_preferences_v2",
      saveWorkbenchPreferences: "save_import_workbench_preferences_v2",
      getRestrictedContentStatus: "get_import_restricted_content_status_v2",
      listHistory: "list_import_history_v2",
      authorizePrivateTarget: "authorize_import_private_target_v2",
      beginLogin: "begin_import_login_v2",
      completeLogin: "complete_import_login_v2",
      revokeLogin: "revoke_import_login_v2",
      getCapabilityRequirement: "get_import_capability_requirement_v2",
      getAsrEnablementPlan: "get_import_asr_enablement_plan_v2",
      installCapability: "install_import_capability_v2",
      startAgentAssistance: "start_import_agent_assistance_v2",
      acceptAgentCandidate: "accept_import_agent_candidate_v2",
      selectAgentCandidate: "select_import_agent_candidate_v2",
      discardAgentCandidate: "discard_import_agent_candidate_v2",
      scanMigration: "scan_import_v2_migration",
      planMigration: "plan_import_v2_migration",
      applyMigration: "apply_import_v2_migration",
      getMigrationStatus: "get_import_v2_migration_status",
      resumeMigration: "resume_import_v2_migration",
      activate: "activate_import_v2",
      getActivation: "get_import_backend_activation",
    });
    expect("invokeCommand" in importV2Api).toBe(false);
  });

  it("forwards a typed request envelope without adding filesystem fields", async () => {
    const request = {
      projectId: "project-1",
      projectRootPath: "D:/Wiki/中文项目",
      resourceMode: "balanced" as const,
    };
    invoke.mockResolvedValueOnce({});

    await importV2Api.createSession(request);

    expect(invoke).toHaveBeenLastCalledWith("create_import_session_v2", { request });
    expect(request).not.toHaveProperty("absolutePath");
    expect(request).not.toHaveProperty("targetPath");
  });

  it("passes preview and readiness requests by identity only", async () => {
    const previewRequest = {
      projectId: "project-1",
      projectRootPath: "D:/Wiki/项目",
      sessionId: "session-1",
      itemId: "item-1",
      candidateId: null,
    };
    invoke.mockResolvedValueOnce({});
    await importV2Api.getPreviewContent(previewRequest);
    expect(invoke).toHaveBeenLastCalledWith("get_import_preview_content_v2", {
      request: previewRequest,
    });

    const readinessRequest = {
      projectId: "project-1",
      projectRootPath: "D:/Wiki/项目",
    };
    invoke.mockResolvedValueOnce({});
    await importV2Api.getReadiness(readinessRequest);
    expect(invoke).toHaveBeenLastCalledWith("get_import_frontend_readiness_v2", {
      request: readinessRequest,
    });
  });

  it("loads the persisted scan result for per-file skip details", async () => {
    const request = {
      projectId: "project-1",
      projectRootPath: "D:/Wiki/project-1",
      sessionId: "session-1",
      taskId: "scan-1",
    };
    invoke.mockResolvedValueOnce({ files: [], skipped: [], truncated: false });

    await importV2Api.getScanResult(request);

    expect(invoke).toHaveBeenLastCalledWith("get_import_scan_result_v2", { request });
  });

  it("keeps legacy item start while exposing operation and saved-scan commands", async () => {
    const batchRequest = {
      projectId: "project-1",
      projectRootPath: "D:/Wiki/project-1",
      sessionId: "session-1",
      itemIds: ["item-1", "item-2"],
    };
    invoke.mockResolvedValueOnce({});
    await importV2Api.startBatch(batchRequest);
    expect(invoke).toHaveBeenLastCalledWith("start_import_batch_v2", { request: batchRequest });

    const scanRequest = {
      projectId: "project-1",
      projectRootPath: "D:/Wiki/project-1",
      sessionId: "session-1",
      taskId: "scan-1",
      confirmationToken: "token-1",
    };
    invoke.mockResolvedValueOnce({});
    await importV2Api.acceptScan(scanRequest);
    expect(invoke).toHaveBeenLastCalledWith("accept_import_scan_v2", { request: scanRequest });
    invoke.mockResolvedValueOnce({});
    await importV2Api.discardScan(scanRequest);
    expect(invoke).toHaveBeenLastCalledWith("discard_import_scan_v2", { request: scanRequest });
  });

  it("forwards batch cancellation with the session and backend batch identity", async () => {
    const request = {
      projectId: "project-1",
      projectRootPath: "D:/Wiki/project-1",
      sessionId: "session-1",
      batchId: "batch-1",
    };
    invoke.mockResolvedValueOnce([]);

    await importV2Api.cancelBatch(request);

    expect(invoke).toHaveBeenLastCalledWith("cancel_import_batch_v2", { request });
  });
});
