import type { FavoriteSidebarItem } from "../../types/bookmark";
import type { ExportRecord } from "../../types/export";
import type { WikiPageMeta } from "../../types/wiki";

export function selectFavoriteSidebarItems(
  pages: WikiPageMeta[],
  records: ExportRecord[],
  missingExportRecordIds: Set<string> = new Set(),
): FavoriteSidebarItem[] {
  const wikiItems = pages
    .filter((page) => page.bookmarked)
    .sort((a, b) => a.title.localeCompare(b.title) || a.path.localeCompare(b.path))
    .map<FavoriteSidebarItem>((page) => ({
      id: `wiki:${page.path}`,
      kind: "wiki_page",
      title: page.title,
      path: page.path,
    }));

  const exportItems = records
    .filter((record) => record.status === "succeeded" && record.bookmarked)
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt) || a.title.localeCompare(b.title))
    .map<FavoriteSidebarItem>((record) => ({
      id: `export:${record.id}`,
      kind: "export_html",
      title: record.title,
      path: record.outputPath,
      exportRecordId: record.id,
      missing: missingExportRecordIds.has(record.id) || undefined,
    }));

  return [...wikiItems, ...exportItems];
}
