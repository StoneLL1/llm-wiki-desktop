import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import {
  cancelWorkflowRun,
  confirmWorkflowAction,
  continueQueuedWorkflows,
  discardWorkflowResult,
  getWorkflowRun,
  getWorkflowFileDiff,
  getWorkflowsOverview,
  listWorkflowRuns,
  prepareWorkflow,
  reorderQueuedWorkflow,
  retryWorkflow,
  startWorkflow,
  undoCancelQueuedWorkflow,
} from "./workflowApi";
import {
  toWorkflowDisplayStatus,
  WORKFLOW_SCHEMA_VERSION,
  type WorkflowRun,
} from "../types/workflow";

const project = {
  projectId: "project-中文",
  projectRootPath: "D:/知识库/研究",
};
const runRequest = { ...project, taskId: "task-1" };

const rustRunFixture = {
  schemaVersion: WORKFLOW_SCHEMA_VERSION,
  taskId: "task-1",
  projectId: project.projectId,
  canonicalIdentityKey: "identity-1",
  identityRevision: "revision-1",
  kind: "update_wiki",
  displayStatus: "waiting_for_confirmation",
  scope: {
    kind: "update_wiki",
    mode: "changed_sources",
    sourceVersions: [{ sourceId: "source-中文", versionId: "version-1" }],
  },
  route: {
    kind: "byok",
    provider: "open_ai",
    model: "configured-model",
    routeRevision: "route-1",
  },
  fingerprint: "fingerprint-1",
  baselineFingerprint: "baseline-1",
  stages: [
    {
      id: "review",
      ordinal: 1,
      status: "waiting",
      labelKey: "workflows.stage.review",
      startedAt: "2026-07-30T00:00:00Z",
      completedAt: null,
      currentItem: "wiki/概念.md",
      progress: { current: 1, total: 2 },
      decision: {
        id: "action-1",
        actionType: "merge_conflict",
        riskLevel: "high",
        affectedPaths: ["wiki/概念.md"],
        candidate: { kind: "task_owned", candidateId: "candidate-1" },
        expiresAt: null,
        checkpointHash: "checkpoint-1",
      },
    },
  ],
  currentStageId: "review",
  queuePosition: null,
  continuationRequired: false,
  retry: { attemptOf: "task-0", attemptNumber: 2 },
  pendingAction: null,
  result: null,
  error: null,
  startedAt: "2026-07-30T00:00:00Z",
  updatedAt: "2026-07-30T00:01:00Z",
  completedAt: null,
} satisfies WorkflowRun;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

const isOneOf = (value: unknown, choices: readonly string[]): value is string =>
  typeof value === "string" && choices.includes(value);

const isNullableString = (value: unknown): value is string | null =>
  value === null || typeof value === "string";

const isStringArray = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((item) => typeof item === "string");

const parseCandidate = (value: unknown): void => {
  if (!isRecord(value)) throw new Error("invalid candidate reference");
  if (value.kind === "task_owned" && typeof value.candidateId === "string") return;
  if (value.kind === "project_relative" && typeof value.path === "string") return;
  throw new Error("invalid candidate reference");
};

const parsePendingAction = (value: unknown): void => {
  if (!isRecord(value)) throw new Error("invalid pending action");
  if (
    typeof value.id !== "string" ||
    !isOneOf(value.actionType, [
      "delete_file",
      "overwrite_file",
      "batch_rewrite",
      "merge_conflict",
      "agent_auto_fix",
      "install_agent",
      "run_skill",
    ]) ||
    !isOneOf(value.riskLevel, ["low", "medium", "high", "destructive"]) ||
    !isStringArray(value.affectedPaths) ||
    !isNullableString(value.expiresAt) ||
    !isNullableString(value.checkpointHash)
  ) {
    throw new Error("invalid pending action");
  }
  if (value.candidate !== null) parseCandidate(value.candidate);
};

const parseScope = (kind: unknown, value: unknown): void => {
  if (!isRecord(value) || value.kind !== kind) {
    throw new Error("scope must be tagged with the workflow kind");
  }
  if (value.kind === "update_wiki") {
    if (
      !isOneOf(value.mode, ["changed_sources", "full_recompile"]) ||
      !Array.isArray(value.sourceVersions) ||
      !value.sourceVersions.every(
        (item) =>
          isRecord(item) &&
          typeof item.sourceId === "string" &&
          typeof item.versionId === "string",
      )
    ) {
      throw new Error("invalid update_wiki scope");
    }
    return;
  }
  if (value.kind === "health_check") {
    if (!isOneOf(value.mode, ["local_quick", "complete"])) {
      throw new Error("invalid health_check scope");
    }
    return;
  }
  if (value.kind === "generate_content") {
    if (
      !isOneOf(value.artifactType, [
        "beautiful_read",
        "knowledge_card",
        "concept_map",
        "project_report",
      ]) ||
      !isStringArray(value.pagePaths) ||
      !isNullableString(value.outputPath)
    ) {
      throw new Error("invalid generate_content scope");
    }
    return;
  }
  throw new Error("invalid workflow scope");
};

const parseRoute = (value: unknown): void => {
  if (value === null) return;
  if (!isRecord(value) || typeof value.routeRevision !== "string") {
    throw new Error("invalid route union");
  }
  if (value.kind === "local") return;
  if (value.kind === "agent") {
    if (
      !isOneOf(value.agent, ["claude", "codex", "openclaw", "hermes"]) ||
      !isNullableString(value.model)
    ) {
      throw new Error("invalid agent route");
    }
    return;
  }
  if (value.kind === "byok") {
    if (
      !isOneOf(value.provider, ["open_ai", "anthropic", "google", "ollama", "custom"]) ||
      typeof value.model !== "string"
    ) {
      throw new Error("invalid byok route");
    }
    return;
  }
  throw new Error("invalid route union");
};

const parseResult = (workflowKind: unknown, value: unknown): void => {
  if (value === null) return;
  if (!isRecord(value) || value.kind !== workflowKind) {
    throw new Error("result must be tagged with the workflow kind");
  }
  if (value.kind === "update_wiki") {
    for (const key of ["created", "updated", "skipped", "deleted", "conflicted"]) {
      if (typeof value[key] !== "number") throw new Error("invalid update_wiki result");
    }
    if (
      !isStringArray(value.affectedPaths) ||
      !isNullableString(value.checkpointHash) ||
      !isNullableString(value.finalCommit)
    ) {
      throw new Error("invalid update_wiki result");
    }
    return;
  }
  if (value.kind === "health_check") {
    if (
      !isNullableString(value.reportId) ||
      typeof value.persistent !== "boolean" ||
      typeof value.errorCount !== "number" ||
      typeof value.warningCount !== "number" ||
      typeof value.infoCount !== "number"
    ) {
      throw new Error("invalid health_check result");
    }
    return;
  }
  if (value.kind === "generate_content") {
    if (
      !isOneOf(value.artifactType, [
        "beautiful_read",
        "knowledge_card",
        "concept_map",
        "project_report",
      ]) ||
      !isNullableString(value.recordId) ||
      !isStringArray(value.outputPaths) ||
      typeof value.validationPassed !== "boolean"
    ) {
      throw new Error("invalid generate_content result");
    }
    return;
  }
  throw new Error("invalid workflow result");
};

const parseRustWorkflowRun = (value: unknown): WorkflowRun => {
  if (!isRecord(value)) throw new Error("workflow run must be an object");
  for (const key of [
    "taskId",
    "projectId",
    "canonicalIdentityKey",
    "identityRevision",
    "fingerprint",
    "baselineFingerprint",
    "startedAt",
    "updatedAt",
  ]) {
    if (typeof value[key] !== "string") throw new Error(`missing string field ${key}`);
  }
  if (value.schemaVersion !== WORKFLOW_SCHEMA_VERSION) {
    throw new Error("unsupported schemaVersion");
  }
  if (!isOneOf(value.kind, ["update_wiki", "health_check", "generate_content"])) {
    throw new Error("invalid workflow kind");
  }
  if (
    !isOneOf(value.displayStatus, [
      "queued",
      "running",
      "waiting_for_confirmation",
      "completed",
      "failed",
      "cancelled",
      "interrupted",
    ])
  ) {
    throw new Error("invalid displayStatus");
  }
  parseScope(value.kind, value.scope);
  parseRoute(value.route);
  if (!Array.isArray(value.stages)) throw new Error("stages must be an array");
  for (const stage of value.stages) {
    if (
      !isRecord(stage) ||
      typeof stage.id !== "string" ||
      typeof stage.ordinal !== "number" ||
      typeof stage.labelKey !== "string" ||
      !isNullableString(stage.startedAt) ||
      !isNullableString(stage.completedAt) ||
      !isNullableString(stage.currentItem)
    ) {
      throw new Error("invalid workflow stage");
    }
    if (
      !isOneOf(stage.status, ["pending", "running", "completed", "failed", "waiting", "skipped"])
    ) {
      throw new Error("invalid workflow stage status");
    }
    if (
      stage.progress !== null &&
      (!isRecord(stage.progress) ||
        typeof stage.progress.current !== "number" ||
        !(stage.progress.total === null || typeof stage.progress.total === "number"))
    ) {
      throw new Error("invalid workflow stage progress");
    }
    if (stage.decision !== null) parsePendingAction(stage.decision);
  }
  if (
    !isNullableString(value.currentStageId) ||
    !(value.queuePosition === null || typeof value.queuePosition === "number") ||
    typeof value.continuationRequired !== "boolean" ||
    !isNullableString(value.completedAt)
  ) {
    throw new Error("invalid workflow run state fields");
  }
  if (value.retry !== null) {
    if (
      !isRecord(value.retry) ||
      typeof value.retry.attemptOf !== "string" ||
      typeof value.retry.attemptNumber !== "number"
    ) {
      throw new Error("invalid retry link");
    }
  }
  if (value.pendingAction !== null) parsePendingAction(value.pendingAction);
  parseResult(value.kind, value.result);
  if (value.error !== null) {
    if (
      !isRecord(value.error) ||
      typeof value.error.code !== "string" ||
      typeof value.error.messageKey !== "string" ||
      typeof value.error.recoverable !== "boolean" ||
      typeof value.error.userActionRequired !== "boolean" ||
      !(
        value.error.suggestedAction === null ||
        isOneOf(value.error.suggestedAction, [
          "open_or_create_project",
          "trust_project",
          "make_writable",
          "configure_git",
          "resolve_dirty_git",
          "import_sources",
          "update_wiki",
          "configure_execution_route",
          "choose_execution_route",
          "prepare_again",
          "acknowledge_remote_provider",
        ])
      )
    ) {
      throw new Error("invalid workflow error");
    }
  }
  return value as unknown as WorkflowRun;
};

describe("workflow API", () => {
  beforeEach(() => {
    invoke.mockReset();
    invoke.mockResolvedValue({});
  });

  it("uses one explicit request envelope for every frozen command", async () => {
    const calls = [
      [getWorkflowsOverview, project, "get_workflows_overview"],
      [
        prepareWorkflow,
        {
          ...project,
          kind: "health_check" as const,
          scope: { kind: "health_check" as const, mode: "local_quick" as const },
          routeSelection: null,
        },
        "prepare_workflow",
      ],
      [
        startWorkflow,
        { ...project, preparationId: "prep-1", preparationRevision: "1" },
        "start_workflow",
      ],
      [
        listWorkflowRuns,
        {
          ...project,
          workflowKind: null,
          displayStatus: null,
          cursor: null,
          limit: 25,
        },
        "list_workflow_runs",
      ],
      [getWorkflowRun, runRequest, "get_workflow_run"],
      [
        getWorkflowFileDiff,
        { ...runRequest, pendingActionId: "action-1", fileId: "file-00000000", cursor: null, limitBytes: 65536 },
        "get_workflow_file_diff",
      ],
      [cancelWorkflowRun, runRequest, "cancel_workflow_run"],
      [undoCancelQueuedWorkflow, runRequest, "undo_cancel_queued_workflow"],
      [
        reorderQueuedWorkflow,
        { ...runRequest, beforeTaskId: null },
        "reorder_queued_workflow",
      ],
      [continueQueuedWorkflows, project, "continue_queued_workflows"],
      [retryWorkflow, runRequest, "retry_workflow"],
      [
        confirmWorkflowAction,
        { ...runRequest, actionId: "action-1" },
        "confirm_workflow_action",
      ],
      [discardWorkflowResult, runRequest, "discard_workflow_result"],
    ] as const;

    for (const [call, request, command] of calls) {
      await (call as (value: never) => Promise<unknown>)(request as never);
      expect(invoke).toHaveBeenLastCalledWith(command, { request });
    }
  });

  it("keeps the frozen overview envelope while representing no active project", async () => {
    await getWorkflowsOverview({ projectId: "", projectRootPath: "" });
    expect(invoke).toHaveBeenCalledWith("get_workflows_overview", {
      request: { projectId: "", projectRootPath: "" },
    });
  });

  it("accepts a Rust-shaped fixture as the discriminated workflow union", () => {
    const parsed = parseRustWorkflowRun(JSON.parse(JSON.stringify(rustRunFixture)));

    expect(parsed.kind).toBe("update_wiki");
    if (parsed.scope.kind !== "update_wiki") {
      throw new Error("expected update_wiki scope");
    }
    expect(parsed.scope.sourceVersions[0]).toEqual({
      sourceId: "source-中文",
      versionId: "version-1",
    });
    expect(parsed.route?.kind).toBe("byok");
    expect(parsed.stages[0].decision?.candidate).toEqual({
      kind: "task_owned",
      candidateId: "candidate-1",
    });
  });

  it("validates every scope, route, result, and candidate union branch", () => {
    const health = {
      ...rustRunFixture,
      kind: "health_check",
      scope: { kind: "health_check", mode: "complete" },
      route: { kind: "local", routeRevision: "local-1" },
      stages: [],
      retry: null,
      result: {
        kind: "health_check",
        reportId: null,
        persistent: false,
        errorCount: 1,
        warningCount: 2,
        infoCount: 3,
      },
    };
    expect(parseRustWorkflowRun(health).result?.kind).toBe("health_check");

    const generate = {
      ...rustRunFixture,
      kind: "generate_content",
      scope: {
        kind: "generate_content",
        artifactType: "knowledge_card",
        pagePaths: ["wiki/概念.md"],
        outputPath: null,
      },
      route: {
        kind: "agent",
        agent: "codex",
        model: null,
        routeRevision: "agent-1",
      },
      stages: [],
      retry: null,
      pendingAction: {
        ...rustRunFixture.stages[0].decision,
        candidate: { kind: "project_relative", path: "exports/html/card.html" },
      },
      result: {
        kind: "generate_content",
        artifactType: "knowledge_card",
        recordId: "record-1",
        outputPaths: ["exports/html/card.html"],
        validationPassed: true,
      },
    };
    expect(parseRustWorkflowRun(generate).result?.kind).toBe("generate_content");

    const update = {
      ...rustRunFixture,
      result: {
        kind: "update_wiki",
        created: 1,
        updated: 2,
        skipped: 3,
        deleted: 0,
        conflicted: 0,
        affectedPaths: ["wiki/概念.md"],
        checkpointHash: "checkpoint-1",
        finalCommit: null,
      },
    };
    expect(parseRustWorkflowRun(update).result?.kind).toBe("update_wiki");
  });

  it("rejects missing camelCase fields and unknown union tags in Rust-shaped JSON", () => {
    const snakeCase = {
      ...rustRunFixture,
      displayStatus: undefined,
      display_status: "waiting_for_confirmation",
    };
    expect(() => parseRustWorkflowRun(snakeCase)).toThrow("invalid displayStatus");

    const unknownRoute = {
      ...rustRunFixture,
      route: { kind: "auto", routeRevision: "route-1" },
    };
    expect(() => parseRustWorkflowRun(unknownRoute)).toThrow("invalid route union");

    expect(() =>
      parseRustWorkflowRun({
        ...rustRunFixture,
        kind: "health_check",
        scope: { kind: "health_check", mode: "bogus" },
      }),
    ).toThrow("invalid health_check scope");
    expect(() =>
      parseRustWorkflowRun({
        ...rustRunFixture,
        route: { kind: "byok", provider: "open_ai", routeRevision: "route-1" },
      }),
    ).toThrow("invalid byok route");
    expect(() => parseRustWorkflowRun({ ...rustRunFixture, retry: undefined })).toThrow(
      "invalid retry link",
    );
    expect(() =>
      parseRustWorkflowRun({
        ...rustRunFixture,
        stages: [{ ...rustRunFixture.stages[0], ordinal: undefined }],
      }),
    ).toThrow("invalid workflow stage");
  });

  it.each([
    ["queued", "queued"],
    ["running", "running"],
    ["cancelling", "running"],
    ["waiting_for_confirmation", "waiting_for_confirmation"],
    ["succeeded", "completed"],
    ["failed", "failed"],
    ["cancelled", "cancelled"],
    ["interrupted", "interrupted"],
  ] as const)("maps task status %s to display status %s", (task, display) => {
    expect(toWorkflowDisplayStatus(task)).toBe(display);
  });
});
