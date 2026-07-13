import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { importV2Api } from "./importV2Api";

describe("Import V2 presentation API", () => {
  beforeEach(() => invoke.mockReset());

  it("freezes one explicit command name for every presentation operation", () => {
    expect(importV2Api.commandNames).toEqual({
      createSession: "create_import_session_v2",
      getSession: "get_import_session_v2",
      addItems: "add_import_items_v2",
      addPaths: "start_add_import_paths_v2",
      addUrl: "add_import_url_v2",
      setSelection: "set_import_item_selection_v2",
      startItems: "start_import_items_v2",
      confirmSession: "confirm_import_session_v2",
      getPreviewContent: "get_import_preview_content_v2",
      getReadiness: "get_import_frontend_readiness_v2",
      listHistory: "list_import_history_v2",
      authorizePrivateTarget: "authorize_import_private_target_v2",
      beginLogin: "begin_import_login_v2",
      completeLogin: "complete_import_login_v2",
      revokeLogin: "revoke_import_login_v2",
      getCapabilityRequirement: "get_import_capability_requirement_v2",
      installCapability: "install_import_capability_v2",
      getAgentPolicy: "get_import_agent_policy_v2",
      setAgentPolicy: "set_import_agent_policy_v2",
      startAgentAssistance: "start_import_agent_assistance_v2",
      previewByokScope: "preview_import_byok_scope_v2",
      approveByokAssistance: "approve_import_byok_assistance_v2",
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
});
