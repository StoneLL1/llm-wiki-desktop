import type { SourceBinding, SourceStatus } from "./source";
import type { QualityReport } from "./importV2";

export type WikiPageType =
  | "entity"
  | "concept"
  | "source"
  | "synthesis"
  | "comparison"
  | "query"
  | "index"
  | "overview"
  | "log"
  | "other";

export type WikiTreeNodeKind = "folder" | "file";

export interface WikiPageMeta {
  path: string;
  title: string;
  pageType: WikiPageType;
  tags: string[];
  sources: string[];
  aliases: string[];
  created: string | null;
  updated: string | null;
  starred: boolean;
  bookmarked: boolean;
  wordCount: number;
  fileSize: number;
  modifiedTime: string;
  hash: string;
  wikilinks: string[];
  sourceBinding?: SourceBinding | null;
  sourceId?: string | null;
  versionId?: string | null;
  sourceStatus?: SourceStatus | null;
  quality?: QualityReport | null;
}

export interface WikiTreeNode {
  name: string;
  kind: WikiTreeNodeKind;
  path: string;
  type?: WikiPageType;
  title?: string;
  starred: boolean;
  bookmarked: boolean;
  fileCount: number;
  children: WikiTreeNode[];
}

export interface WikiTree {
  root: WikiTreeNode;
  pages: WikiPageMeta[];
  totalPages: number;
}

export interface WikiPageContent {
  meta: WikiPageMeta;
  rawMarkdown: string;
  bodyMarkdown: string;
  frontmatterYaml: string | null;
}

export interface ScanWikiRequest {
  projectId: string;
  projectRootPath: string;
}

export interface ReadWikiPageRequest {
  projectId: string;
  projectRootPath: string;
  relativePath: string;
}

export interface SaveWikiPageRequest {
  projectId: string;
  projectRootPath: string;
  relativePath: string;
  contents: string;
  expectedHash: string | null;
}

export interface SaveWikiPageResponse {
  relativePath: string;
  hash: string;
  savedAt: string;
  graphCacheInvalidated: boolean;
}

export interface CreateWikiPageInput {
  relativePath: string;
  title: string | null;
  pageType: WikiPageType | null;
}

export interface RenameWikiPageResponse {
  relativePath: string;
  hash: string;
  savedAt: string;
  updatedReferences: string[];
  graphCacheInvalidated: boolean;
}

export interface SearchRequest {
  projectId: string;
  projectRootPath: string;
  query?: string;
  pageTypes?: WikiPageType[];
  tags?: string[];
  source?: string;
  limit?: number;
}

export interface SearchResult {
  path: string;
  title: string;
  pageType: WikiPageType;
  starred: boolean;
  matchedFields: string[];
  snippet?: string;
  score: number;
}

export interface SearchResponse {
  results: SearchResult[];
  total: number;
}

export const WIKI_PAGE_TYPES: WikiPageType[] = [
  "entity",
  "concept",
  "source",
  "synthesis",
  "comparison",
  "query",
];

export const CREATABLE_WIKI_PAGE_TYPES = WIKI_PAGE_TYPES.filter(
  (type) => type !== "source",
);

export const PAGE_TYPE_LABEL_KEYS: Record<WikiPageType, string> = {
  entity: "wiki.type.entity",
  concept: "wiki.type.concept",
  source: "wiki.type.source",
  synthesis: "wiki.type.synthesis",
  comparison: "wiki.type.comparison",
  query: "wiki.type.query",
  index: "wiki.type.index",
  overview: "wiki.type.overview",
  log: "wiki.type.log",
  other: "wiki.type.other",
};
