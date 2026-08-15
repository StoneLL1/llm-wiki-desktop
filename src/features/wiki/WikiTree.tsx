import { useMemo, useState, type Ref } from "react";
import { useTranslation } from "react-i18next";
import {
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderOpen,
  History,
  List,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Star,
  Trash2,
} from "lucide-react";

import type { WikiPageMeta, WikiPageType, WikiTreeNode } from "../../types/wiki";
import { WIKI_PAGE_TYPES, PAGE_TYPE_LABEL_KEYS } from "../../types/wiki";

/** Matches UI-Frontend-design/wiki.html: index.md → list, log.md → history. */
function fileIconFor(node: WikiTreeNode) {
  const name = node.name.toLowerCase();
  if (name === "index.md") {
    return <List size={14} className="shrink-0 text-[var(--accent)]" />;
  }
  if (name === "log.md") {
    return <History size={14} className="shrink-0 text-[var(--text-muted)]" />;
  }
  return <FileText size={14} className="shrink-0 text-[var(--text-muted)]" />;
}

interface WikiTreeProps {
  root: WikiTreeNode;
  pages: WikiPageMeta[];
  selectedPath: string | null;
  onSelect: (path: string) => void;
  onRefresh: () => void;
  onCreate: () => void;
  onRename: (path: string) => void;
  onDelete: (path: string) => void;
  onSourceRename?: (sourceId: string, path: string) => void;
  onSourceDelete?: (sourceId: string) => void;
  scrollRef?: Ref<HTMLDivElement>;
}

type TypeFilter = WikiPageType | "all";

export function WikiTree({
  root,
  pages,
  selectedPath,
  onSelect,
  onRefresh,
  onCreate,
  onRename,
  onDelete,
  onSourceRename = () => {},
  onSourceDelete = () => {},
  scrollRef,
}: WikiTreeProps) {
  const { t } = useTranslation();
  const [filterText, setFilterText] = useState("");
  const [typeFilter, setTypeFilter] = useState<TypeFilter>("all");

  const allowedPaths = useMemo(() => {
    const query = filterText.trim().toLowerCase();
    const set = new Set<string>();
    for (const page of pages) {
      if (typeFilter !== "all" && page.pageType !== typeFilter) continue;
      if (!query) {
        set.add(page.path);
        continue;
      }
      const haystack = [
        page.title,
        page.path,
        ...page.tags,
        ...page.aliases,
      ]
        .join(" ")
        .toLowerCase();
      if (haystack.includes(query)) set.add(page.path);
    }
    return set;
  }, [pages, filterText, typeFilter]);
  const sourceIdsByPath = useMemo(
    () =>
      new Map(
        pages
          .filter((page) => page.pageType === "source" && page.sourceBinding?.sourceId)
          .map((page) => [page.path, page.sourceBinding!.sourceId]),
      ),
    [pages],
  );

  return (
    <div className="wiki-tree">
      <div className="flex h-[44px] items-center gap-2 border-b border-[var(--border-subtle)] px-3">
        <div className="flex h-[26px] flex-1 items-center gap-1.5 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--background)] px-2">
          <Search size={13} className="shrink-0 text-[var(--text-muted)]" />
          <input
            className="w-full border-none bg-transparent text-[12px] text-[var(--text-primary)] outline-none"
            placeholder={t("wiki.tree.filterPlaceholder")}
            value={filterText}
            onChange={(event) => setFilterText(event.target.value)}
          />
        </div>
        <button
          type="button"
          title={t("wiki.tree.newPage")}
          aria-label={t("wiki.tree.newPage")}
          onClick={onCreate}
          className="grid h-[26px] w-[26px] place-items-center rounded-[var(--radius-sm)] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
        >
          <Plus size={14} />
        </button>
        <button
          type="button"
          title={t("wiki.tree.refresh")}
          aria-label={t("wiki.tree.refresh")}
          onClick={onRefresh}
          className="grid h-[26px] w-[26px] place-items-center rounded-[var(--radius-sm)] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
        >
          <RefreshCw size={14} />
        </button>
      </div>

      <div className="flex gap-1 overflow-x-auto border-b border-[var(--border-subtle)] px-3 py-1.5">
        <FilterPill
          active={typeFilter === "all"}
          label={t("wiki.tree.filterAll")}
          onClick={() => setTypeFilter("all")}
        />
        {WIKI_PAGE_TYPES.map((type) => (
          <FilterPill
            key={type}
            active={typeFilter === type}
            label={t(PAGE_TYPE_LABEL_KEYS[type])}
            onClick={() => setTypeFilter(type === typeFilter ? "all" : type)}
          />
        ))}
      </div>

      <div ref={scrollRef} className="flex-1 overflow-y-auto px-2 py-2" data-testid="wiki-tree-scroll">
        <div className="px-2 pb-1 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
          wiki/
        </div>
        {pages.length === 0 ? (
          <p className="px-2 py-4 text-[12px] text-[var(--text-muted)]">
            {t("wiki.tree.empty")}
          </p>
        ) : (
          <TreeChildren
            node={root}
            allowedPaths={allowedPaths}
            selectedPath={selectedPath}
            onSelect={onSelect}
            onRename={onRename}
            onDelete={onDelete}
            onSourceRename={onSourceRename}
            onSourceDelete={onSourceDelete}
            sourceIdsByPath={sourceIdsByPath}
            depth={0}
          />
        )}
      </div>
    </div>
  );
}

function FilterPill({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`h-[20px] shrink-0 rounded-full px-2 text-[10.5px] font-medium transition-colors ${
        active
          ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
          : "bg-[var(--surface-muted)] text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
      }`}
    >
      {label}
    </button>
  );
}

function TreeChildren({
  node,
  allowedPaths,
  selectedPath,
  onSelect,
  onRename,
  onDelete,
  onSourceRename,
  onSourceDelete,
  sourceIdsByPath,
  depth,
}: {
  node: WikiTreeNode;
  allowedPaths: Set<string>;
  selectedPath: string | null;
  onSelect: (path: string) => void;
  onRename: (path: string) => void;
  onDelete: (path: string) => void;
  onSourceRename: (sourceId: string, path: string) => void;
  onSourceDelete: (sourceId: string) => void;
  sourceIdsByPath: Map<string, string>;
  depth: number;
}) {
  return (
    <div>
      {node.children.map((child) => (
        <TreeRow
          key={child.path}
          node={child}
          allowedPaths={allowedPaths}
          selectedPath={selectedPath}
          onSelect={onSelect}
          onRename={onRename}
          onDelete={onDelete}
          onSourceRename={onSourceRename}
          onSourceDelete={onSourceDelete}
          sourceIdsByPath={sourceIdsByPath}
          depth={depth}
        />
      ))}
    </div>
  );
}

function TreeRow({
  node,
  allowedPaths,
  selectedPath,
  onSelect,
  onRename,
  onDelete,
  onSourceRename,
  onSourceDelete,
  sourceIdsByPath,
  depth,
}: {
  node: WikiTreeNode;
  allowedPaths: Set<string>;
  selectedPath: string | null;
  onSelect: (path: string) => void;
  onRename: (path: string) => void;
  onDelete: (path: string) => void;
  onSourceRename: (sourceId: string, path: string) => void;
  onSourceDelete: (sourceId: string) => void;
  sourceIdsByPath: Map<string, string>;
  depth: number;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(depth < 1);
  const [menuOpen, setMenuOpen] = useState(false);
  const isFile = node.kind === "file";
  const sourceId = sourceIdsByPath.get(node.path);
  const sourceLike = node.type === "source";

  if (isFile) {
    if (!allowedPaths.has(node.path)) return null;
    const selected = node.path === selectedPath;
    return (
      <div className="group relative flex items-center">
        <button
          type="button"
          onClick={() => onSelect(node.path)}
          className={`flex min-w-0 flex-1 items-center gap-1.5 rounded-[var(--radius-sm)] py-[3px] pr-7 text-left text-[12.5px] transition-colors ${
            selected
              ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
              : "text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
          }`}
          style={{ paddingLeft: depth * 12 + 8 }}
        >
          <span className="w-[14px] shrink-0" />
          {fileIconFor(node)}
          <span className="flex-1 truncate">{node.name}</span>
          <span className="grid w-[18px] shrink-0 place-items-center">
            {node.bookmarked || node.starred ? (
              <Star size={11} fill="currentColor" className="text-[var(--accent)]" />
            ) : null}
          </span>
        </button>
        {!sourceLike || sourceId ? (
          <button
            type="button"
            aria-label={t("wiki.tree.pageActions", { name: node.name })}
            title={t("wiki.tree.pageActions", { name: node.name })}
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen((value) => !value)}
            className="absolute right-0.5 grid h-[24px] w-[24px] place-items-center rounded-[var(--radius-sm)] text-[var(--text-muted)] opacity-0 hover:bg-[var(--surface-muted)] group-hover:opacity-100 focus:opacity-100"
          >
            <MoreHorizontal size={13} />
          </button>
        ) : null}
        {menuOpen ? (
          <div className="absolute right-0 top-[26px] z-20 min-w-[132px] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-raised)] p-1 shadow-lg">
            <button
              type="button"
              onClick={() => {
                setMenuOpen(false);
                if (sourceId) onSourceRename(sourceId, node.path);
                else onRename(node.path);
              }}
              className="flex h-[28px] w-full items-center gap-2 rounded-[var(--radius-sm)] px-2 text-left text-[12px] text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
            >
              <Pencil size={13} />
              {sourceId ? t("source.action.move") : t("wiki.tree.rename")}
            </button>
            <button
              type="button"
              onClick={() => {
                setMenuOpen(false);
                if (sourceId) onSourceDelete(sourceId);
                else onDelete(node.path);
              }}
              className="flex h-[28px] w-full items-center gap-2 rounded-[var(--radius-sm)] px-2 text-left text-[12px] text-[var(--danger)] hover:bg-[var(--surface-muted)]"
            >
              <Trash2 size={13} />
              {sourceId ? t("source.action.delete") : t("wiki.tree.delete")}
            </button>
          </div>
        ) : null}
      </div>
    );
  }

  const visibleLeaves = countVisibleLeaves(node, allowedPaths);
  if (visibleLeaves === 0) return null;

  return (
    <div>
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="flex w-full items-center gap-1.5 rounded-[var(--radius-sm)] py-[3px] pr-2 text-left text-[12.5px] text-[var(--text-primary)] transition-colors hover:bg-[var(--surface-muted)]"
        style={{ paddingLeft: depth * 12 + 4 }}
      >
        {open ? (
          <ChevronDown size={13} className="shrink-0 text-[var(--text-muted)]" />
        ) : (
          <ChevronRight size={13} className="shrink-0 text-[var(--text-muted)]" />
        )}
        {open ? (
          <FolderOpen size={14} className="shrink-0 text-[var(--text-muted)]" />
        ) : (
          <Folder size={14} className="shrink-0 text-[var(--text-muted)]" />
        )}
        <span className="flex-1 truncate">{node.name}</span>
        <span className="shrink-0 font-mono text-[10.5px] text-[var(--text-muted)]">
          {node.fileCount}
        </span>
      </button>
      {open ? (
        <TreeChildren
          node={node}
          allowedPaths={allowedPaths}
          selectedPath={selectedPath}
          onSelect={onSelect}
          onRename={onRename}
          onDelete={onDelete}
          onSourceRename={onSourceRename}
          onSourceDelete={onSourceDelete}
          sourceIdsByPath={sourceIdsByPath}
          depth={depth + 1}
        />
      ) : null}
    </div>
  );
}

function countVisibleLeaves(node: WikiTreeNode, allowedPaths: Set<string>): number {
  if (node.kind === "file") {
    return allowedPaths.has(node.path) ? 1 : 0;
  }
  let total = 0;
  for (const child of node.children) {
    total += countVisibleLeaves(child, allowedPaths);
  }
  return total;
}
