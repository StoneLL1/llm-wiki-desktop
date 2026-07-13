import { describe, expect, it } from "vitest";
import { AGENT_RECOVERY_ACTIONS, balancedAgentAssistancePolicy } from "./importV2Agent";
import type { ImportIssue } from "./importV2";
import type { SelectImportAgentCandidateRequest } from "./importV2";

describe("Import V2 Agent assistance contracts", () => {
  it("freezes recovery action wire names", () => {
    expect(AGENT_RECOVERY_ACTIONS).toEqual([
      "invoke_local_agent",
      "request_byok",
      "compare_candidate",
      "discard_candidate",
    ]);
  });

  it("keeps cloud and low-quality automation disabled in balanced policy", () => {
    expect(balancedAgentAssistancePolicy(true)).toEqual({
      autoLocalOnHardFailure: true,
      autoLocalOnQualityWarning: false,
      autoByok: false,
      maxAttemptsPerItem: 1,
    });
  });

  it("adds available actions without replacing Core recovery actions", () => {
    const issue: ImportIssue = {
      code: "IMPORT_V2_ENGINE_FAILED",
      message: "failed",
      stage: "extract",
      retryable: true,
      userActionRequired: false,
      recoveryActions: ["retry", "invoke_agent"],
      availableActions: ["invoke_local_agent", "request_byok"],
    };
    expect(issue.recoveryActions).toEqual(["retry", "invoke_agent"]);
    expect(issue.availableActions).toEqual(["invoke_local_agent", "request_byok"]);
  });

  it("requires a current Wiki hash for explicit three-way merge payloads", () => {
    const request: SelectImportAgentCandidateRequest = {
      projectId: "project-a",
      projectRootPath: "C:/wiki",
      sessionId: "session-a",
      itemId: "item-a",
      candidateId: "candidate-a",
      mergedMarkdown: "# Explicit merge",
      expectedCurrentWikiSha256: "a".repeat(64),
    };
    expect(request.expectedCurrentWikiSha256).toHaveLength(64);
  });
});
