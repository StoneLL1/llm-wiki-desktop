export type BookmarkResourceKind = "wiki_page" | "export_html";

export interface FavoriteSidebarItem {
  id: string;
  kind: BookmarkResourceKind;
  title: string;
  path: string;
  exportRecordId?: string;
  missing?: boolean;
}
