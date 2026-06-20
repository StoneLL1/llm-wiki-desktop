import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { FileText } from "lucide-react";

import type { WikiPageMeta } from "../../types/wiki";
import { PAGE_TYPE_LABEL_KEYS } from "../../types/wiki";

interface RelatedPagesPanelProps {
  page: WikiPageMeta | null;
  pages: WikiPageMeta[];
  onOpenPage: (path: string) => void;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function RelatedPagesPanel({ page, pages, onOpenPage }: RelatedPagesPanelProps) {
  const { t } = useTranslation();

  const backlinks = useMemo(() => {
    if (!page) return [] as WikiPageMeta[];
    const stems = new Set<string>();
    const fileStem = page.path.split("/").pop()?.replace(/\.md$/i, "").toLowerCase();
    if (fileStem) stems.add(fileStem);
    if (page.title) stems.add(page.title.toLowerCase());
    for (const alias of page.aliases) {
      if (alias) stems.add(alias.toLowerCase());
    }
    return pages
      .filter((candidate) => candidate.path !== page.path)
      .filter((candidate) =>
        candidate.wikilinks.some((link) => stems.has(link.toLowerCase())),
      );
  }, [page, pages]);

  if (!page) {
    return (
      <div className="flex h-full items-center justify-center px-4 text-center text-[12px] text-[var(--text-muted)]">
        {t("wiki.related.empty")}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-3 text-[12px]">
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
            {backlinks.length}
          </span>
        </h4>
        {backlinks.length === 0 ? (
          <p className="text-[11.5px] text-[var(--text-muted)]">{t("wiki.related.noBacklinks")}</p>
        ) : (
          <div className="flex flex-col gap-0.5">
            {backlinks.map((link) => (
              <button
                key={link.path}
                type="button"
                onClick={() => onOpenPage(link.path)}
                className="flex items-center gap-2 rounded-[var(--radius-sm)] px-2 py-1.5 text-left text-[12.5px] text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
              >
                <FileText size={13} className="shrink-0 text-[var(--text-muted)]" />
                <span className="flex-1 truncate">{link.title}</span>
              </button>
            ))}
          </div>
        )}
      </section>

      {page.sources.length > 0 ? (
        <section>
          <h4 className="mb-1.5 text-[10.5px] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {t("wiki.related.sources")}
          </h4>
          <ul className="flex flex-col gap-1 text-[11px]">
            {page.sources.map((source) => (
              <li key={source} className="break-all font-mono text-[var(--text-secondary)]">
                {source}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
