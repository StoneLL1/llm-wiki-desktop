import { describe, expect, it } from "vitest";

import type { ExportRecord } from "../../types/export";
import type { WikiPageMeta } from "../../types/wiki";
import { selectFavoriteSidebarItems } from "./bookmarkSelectors";

function page(overrides: Partial<WikiPageMeta>): WikiPageMeta {
  return {
    path: "wiki/concepts/agent.md",
    title: "Agent",
    pageType: "concept",
    tags: [],
    sources: [],
    aliases: [],
    created: null,
    updated: null,
    starred: false,
    bookmarked: false,
    wordCount: 1,
    fileSize: 1,
    modifiedTime: "2026-07-04T00:00:00Z",
    hash: "hash",
    wikilinks: [],
    ...overrides,
  };
}

function record(overrides: Partial<ExportRecord>): ExportRecord {
  return {
    id: "export-1",
    exportType: "beautiful_read",
    title: "Agent HTML",
    sourcePath: "wiki/concepts/agent.md",
    outputPath: "exports/html/agent.html",
    createdAt: "2026-07-04T00:00:00Z",
    route: "byok",
    status: "succeeded",
    bookmarked: false,
    ...overrides,
  };
}

describe("bookmark selectors", () => {
  it("turns app-bookmarked wiki pages into sidebar items", () => {
    const items = selectFavoriteSidebarItems([
      page({ bookmarked: true }),
      page({ path: "wiki/concepts/starred.md", title: "Starred", starred: true }),
    ], []);

    expect(items).toEqual([
      {
        id: "wiki:wiki/concepts/agent.md",
        kind: "wiki_page",
        title: "Agent",
        path: "wiki/concepts/agent.md",
      },
    ]);
  });

  it("turns bookmarked succeeded exports into sidebar items", () => {
    const items = selectFavoriteSidebarItems([], [
      record({ id: "export-1", bookmarked: true }),
      record({ id: "export-2", status: "failed", bookmarked: true }),
    ]);

    expect(items).toHaveLength(1);
    expect(items[0]).toMatchObject({
      kind: "export_html",
      title: "Agent HTML",
      path: "exports/html/agent.html",
      exportRecordId: "export-1",
    });
  });

  it("sorts wiki favorites first and exports newest first", () => {
    const items = selectFavoriteSidebarItems([
      page({ path: "wiki/b.md", title: "Beta", bookmarked: true }),
      page({ path: "wiki/a.md", title: "Alpha", bookmarked: true }),
    ], [
      record({ id: "old", title: "Old", outputPath: "exports/html/old.html", bookmarked: true, createdAt: "2026-01-01T00:00:00Z" }),
      record({ id: "new", title: "New", outputPath: "exports/html/new.html", bookmarked: true, createdAt: "2026-07-04T00:00:00Z" }),
    ]);

    expect(items.map((item) => item.title)).toEqual(["Alpha", "Beta", "New", "Old"]);
  });

  it("keeps missing bookmarked exports visible", () => {
    const items = selectFavoriteSidebarItems([], [
      record({ id: "missing", bookmarked: true }),
    ], new Set(["missing"]));

    expect(items[0]?.missing).toBe(true);
  });
});
