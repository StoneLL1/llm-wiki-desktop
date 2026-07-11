import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { FileOutput, FileText, Image, Link, Network } from "lucide-react";

import type { WikiPageMeta } from "../../types/wiki";
import { PAGE_TYPE_LABEL_KEYS } from "../../types/wiki";

interface RelatedPagesPanelProps {
  page: WikiPageMeta | null;
  pages: WikiPageMeta[];
  onOpenPage: (path: string) => void;
  onViewAllBacklinks: () => void;
  onGenerateHtml: () => void;
  onGenerateCard: () => void;
  onViewInGraph: () => void;
  onCopyWikilink: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function resolveSourcePage(source: string, pages: WikiPageMeta[]): WikiPageMeta | null {
  const normalized = source.toLowerCase().replace(/\.md$/i, "");
  return (
    pages.find((candidate) => {
      const path = candidate.path.toLowerCase().replace(/\.md$/i, "");
      const stem = path.split("/").pop();
      return path === normalized || stem === normalized || candidate.title.toLowerCase() === normalized;
    }) ?? null
  );
}

export function RelatedPagesPanel({
  page,
  pages,
  onOpenPage,
  onViewAllBacklinks,
  onGenerateHtml,
  onGenerateCard,
  onViewInGraph,
  onCopyWikilink,
}: RelatedPagesPanelProps) {
  const { t } = useTranslation();

  const backlinks = useMemo(() => {
    if (!page) return [] as Array<{ page: WikiPageMeta; count: number }>;
    const stems = new Set<string>();
    const fileStem = page.path.split("/").pop()?.replace(/\.md$/i, "").toLowerCase();
    if (fileStem) stems.add(fileStem);
    if (page.title) stems.add(page.title.toLowerCase());
    for (const alias of page.aliases) {
      if (alias) stems.add(alias.toLowerCase());
    }
    return pages
      .filter((candidate) => candidate.path !== page.path)
      .map((candidate) => ({
        page: candidate,
        count: candidate.wikilinks.filter((link) => stems.has(link.toLowerCase())).length,
      }))
      .filter((candidate) => candidate.count > 0);
  }, [page, pages]);
  const backlinkCount = backlinks.reduce((total, item) => total + item.count, 0);

  if (!page) {
    return (
      <div className="flex h-full items-center justify-center px-4 text-center text-[12px] text-[var(--text-muted)]">
        {t("wiki.related.empty")}
      </div>
    );
  }

  return (
    <div className="app-pane-scrollbar flex h-full flex-col gap-4 overflow-y-auto p-3 text-[12px]">
      <section>
        <h4 className="mb-1.5 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("wiki.related.metadata")}
        </h4>
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[12px]">
          <dt className="text-[var(--text-muted)]">{t("wiki.related.path")}</dt>
          <dd className="break-all font-mono text-[11px] text-[var(--text-primary)]">{page.path}</dd>
          <dt className="text-[var(--text-muted)]">{t("wiki.related.type")}</dt>
          <dd className="text-[var(--text-primary)]">{t(PAGE_TYPE_LABEL_KEYS[page.pageType])}</dd>
          {page.created ? (
            <>
              <dt className="text-[var(--text-muted)]">{t("wiki.related.created")}</dt>
              <dd className="text-[var(--text-primary)]">{page.created}</dd>
            </>
          ) : null}
          {page.updated ? (
            <>
              <dt className="text-[var(--text-muted)]">{t("wiki.related.updated")}</dt>
              <dd className="text-[var(--text-primary)]">{page.updated}</dd>
            </>
          ) : null}
          <dt className="text-[var(--text-muted)]">{t("wiki.related.words")}</dt>
          <dd className="text-[var(--text-primary)]">{page.wordCount.toLocaleString()}</dd>
          <dt className="text-[var(--text-muted)]">{t("wiki.related.citationCount")}</dt>
          <dd className="font-mono text-[var(--text-primary)]">{page.sources.length}</dd>
          <dt className="text-[var(--text-muted)]">{t("wiki.related.backlinkCount")}</dt>
          <dd className="font-mono text-[var(--text-primary)]">{backlinkCount}</dd>
          <dt className="text-[var(--text-muted)]">{t("wiki.related.size")}</dt>
          <dd className="text-[var(--text-primary)]">{formatBytes(page.fileSize)}</dd>
        </dl>
      </section>

      {page.tags.length > 0 ? (
        <section>
          <h4 className="mb-1.5 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {t("wiki.related.tags")}
          </h4>
          <div className="flex flex-wrap gap-1.5">
            {page.tags.map((tag) => (
              <span
                key={tag}
                className="h-[20px] rounded-full bg-[var(--surface-muted)] px-2 text-[10.5px] leading-[20px] text-[var(--text-secondary)]"
              >
                {tag}
              </span>
            ))}
          </div>
        </section>
      ) : null}

      <section>
        <h4 className="mb-1.5 flex items-center gap-1 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("wiki.related.backlinks")}
          <span className="font-mono text-[10.5px] font-normal normal-case tracking-normal text-[var(--text-muted)]">
            {backlinkCount}
          </span>
        </h4>
        {backlinks.length === 0 ? (
          <p className="text-[11.5px] text-[var(--text-muted)]">{t("wiki.related.noBacklinks")}</p>
        ) : (
          <div className="flex flex-col gap-0.5">
            {backlinks.map(({ page: link, count }) => (
              <button
                key={link.path}
                type="button"
                onClick={() => onOpenPage(link.path)}
                className="relpage"
              >
                <FileText size={13} className="relpage__icon" />
                <span className="relpage__title">{link.title}</span>
                <span className="relpage__count">{count}</span>
              </button>
            ))}
            <button
              type="button"
              onClick={onViewAllBacklinks}
              aria-label={t("wiki.related.viewAllBacklinks", { count: backlinkCount })}
              className="mt-1 h-[28px] text-center text-[11.5px] text-[var(--accent-hover)] hover:underline"
            >
              {t("wiki.related.viewAllBacklinks", { count: backlinkCount })} →
            </button>
          </div>
        )}
      </section>

      {page.sources.length > 0 ? (
        <section>
          <h4 className="mb-1.5 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {t("wiki.related.sources")}
          </h4>
          <ul className="flex flex-col gap-1.5">
            {page.sources.map((source, index) => {
              const sourcePage = resolveSourcePage(source, pages);
              return (
                <li key={`${source}-${index}`}>
                  <button
                    id={`citation-${index + 1}`}
                    type="button"
                    disabled={!sourcePage}
                    onClick={() => {
                      if (sourcePage) onOpenPage(sourcePage.path);
                    }}
                    className="citation w-full text-left disabled:cursor-default"
                  >
                    <span className="citation__idx">{index + 1}</span>
                    <span className="citation__title">{source}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      ) : null}

      <section>
        <h4 className="mb-1.5 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
          {t("wiki.related.actions")}
        </h4>
        <div className="flex flex-col gap-1.5">
          <ActionButton icon={<FileOutput size={13} />} label={t("wiki.related.generateHtml")} onClick={onGenerateHtml} />
          <ActionButton icon={<Image size={13} />} label={t("wiki.related.generateCard")} onClick={onGenerateCard} />
          <ActionButton icon={<Network size={13} />} label={t("wiki.related.viewInGraph")} onClick={onViewInGraph} />
          <ActionButton icon={<Link size={13} />} label={t("wiki.related.copyWikilink")} onClick={onCopyWikilink} />
        </div>
      </section>
    </div>
  );
}

function ActionButton({
  icon,
  label,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex h-[28px] w-full items-center justify-start gap-2 rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--background)] px-2 text-[12px] text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
    >
      {icon}
      {label}
    </button>
  );
}
