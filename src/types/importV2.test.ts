import { describe, expect, it } from "vitest";
import type { AddImportItemsV2Request, CommitImportSessionRequest, CreateImportSessionV2Request, GetImportSessionV2Request, ImportSession, SetImportItemSelectionV2Request, StartImportItemsV2Request } from "./importV2";
import type { TaskResult, TaskResultReference } from "./task";

describe("Import V2 contract", () => {
  it("mirrors the typed Rust import preview task reference", () => {
    const reference = { type: "import_preview", sessionId: "session-1", itemId: "item-1" } satisfies TaskResultReference;
    const result = { summary: "Preview ready", affectedPaths: [], reference } satisfies TaskResult;
    expect(result.reference).toEqual(reference);
  });

  it("keeps completed V2 commit results bound to their history batch", () => {
    const reference = { type: "import_v2_session_preview", sessionId: "session-1", batchId: "batch-1" } satisfies TaskResultReference;
    expect(reference.batchId).toBe("batch-1");
  });
  it("accepts the Rust camelCase session shape without writable target paths", () => {
    const session = { schemaVersion: 2, sessionId: "session-1", projectId: "project-1", status: "draft", resourceMode: "balanced", createdAt: "2026-07-11T00:00:00Z", updatedAt: "2026-07-11T00:00:00Z", items: [] } satisfies ImportSession;
    expect(session.schemaVersion).toBe(2);
    expect("targetPath" in session).toBe(false);
    expect("wikiPath" in session).toBe(false);
  });

  it("keeps an optional discovery task identity compatible with older sessions", () => {
    const session = { schemaVersion: 2, sessionId: "session-1", projectId: "project-1", status: "draft", resourceMode: "balanced", createdAt: "2026-07-11T00:00:00Z", updatedAt: "2026-07-11T00:00:00Z", items: [] } satisfies ImportSession;
    const recovered = { ...session, discoveryTaskId: "scan-task-1" } satisfies ImportSession;
    expect(recovered.discoveryTaskId).toBe("scan-task-1");
  });

  it("accepts Rust request fixtures without frontend-selected output paths", () => {
    const add = { projectId: "project-1", projectRootPath: "fixture/project", sessionId: "session-1", inputs: [{ kind: "file", displayName: "note.md", locator: "fixture/input/note.md", normalizedLocator: null }] } satisfies AddImportItemsV2Request;
    const commit = { projectId: "project-1", projectRootPath: "fixture/project", sessionId: "session-1", decisions: [{ itemId: "item-1", conflictAction: null, expectedWikiHash: null }] } satisfies CommitImportSessionRequest;
    const create = { projectId: "project-1", projectRootPath: "fixture/project", resourceMode: "balanced" } satisfies CreateImportSessionV2Request;
    const get = { projectId: "project-1", projectRootPath: "fixture/project", sessionId: "session-1" } satisfies GetImportSessionV2Request;
    const select = { ...get, itemId: "item-1", selected: true } satisfies SetImportItemSelectionV2Request;
    const start = { ...get, itemIds: ["item-1"] } satisfies StartImportItemsV2Request;
    for (const request of [add, commit, create, get, select, start]) {
      expect("targetPath" in request).toBe(false);
      expect("wikiPath" in request).toBe(false);
    }
  });
});
