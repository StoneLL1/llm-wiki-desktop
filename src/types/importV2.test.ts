import { describe, expect, it } from "vitest";
import type { AddImportItemsV2Request, CommitImportSessionRequest, CreateImportSessionV2Request, GetImportSessionV2Request, ImportSession, SetImportItemSelectionV2Request, StartImportItemsV2Request } from "./importV2";

describe("Import V2 contract", () => {
  it("accepts the Rust camelCase session shape without writable target paths", () => {
    const session = { schemaVersion: 2, sessionId: "session-1", projectId: "project-1", status: "draft", resourceMode: "balanced", createdAt: "2026-07-11T00:00:00Z", updatedAt: "2026-07-11T00:00:00Z", items: [] } satisfies ImportSession;
    expect(session.schemaVersion).toBe(2);
    expect("targetPath" in session).toBe(false);
    expect("wikiPath" in session).toBe(false);
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
