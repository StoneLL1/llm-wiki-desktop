import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { CreateWikiPageInput, WikiPageType } from "../../types/wiki";
import { WIKI_PAGE_TYPES } from "../../types/wiki";

interface WikiPageFormDialogProps {
  mode: "create" | "rename";
  initialPath: string;
  onCancel: () => void;
  onSubmit: (input: CreateWikiPageInput) => void;
}

function normalizeWikiPath(value: string): string {
  let path = value.trim().replace(/\\/g, "/").replace(/^\/+/, "");
  if (!path.startsWith("wiki/")) path = `wiki/${path}`;
  if (!path.toLowerCase().endsWith(".md")) path += ".md";
  return path;
}

function inferPageType(path: string): WikiPageType | null {
  const folder = normalizeWikiPath(path).split("/")[1];
  const singular = folder?.endsWith("s") ? folder.slice(0, -1) : folder;
  return WIKI_PAGE_TYPES.includes(singular as WikiPageType)
    ? (singular as WikiPageType)
    : null;
}

export function WikiPageFormDialog({
  mode,
  initialPath,
  onCancel,
  onSubmit,
}: WikiPageFormDialogProps) {
  const { t } = useTranslation();
  const pathRef = useRef<HTMLInputElement>(null);
  const [path, setPath] = useState(initialPath);
  const [title, setTitle] = useState("");
  const [pageType, setPageType] = useState<WikiPageType | null>(
    inferPageType(initialPath),
  );
  const normalizedPath = useMemo(() => normalizeWikiPath(path), [path]);
  const valid = normalizedPath.startsWith("wiki/") && normalizedPath !== "wiki/.md";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="wiki-page-form-title"
      onKeyDown={(event) => {
        if (event.key === "Escape") onCancel();
      }}
    >
      <form
        className="w-full max-w-[480px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg"
        onSubmit={(event) => {
          event.preventDefault();
          if (!valid) return;
          onSubmit({
            relativePath: normalizedPath,
            title: mode === "create" && title.trim() ? title.trim() : null,
            pageType: mode === "create" ? pageType : null,
          });
        }}
      >
        <header className="flex h-[52px] items-center border-b border-[var(--border)] px-4">
          <h2 id="wiki-page-form-title" className="text-[16px] font-semibold text-[var(--text-primary)]">
            {t(`wiki.pageForm.${mode}.title`)}
          </h2>
        </header>
        <div className="space-y-4 px-4 py-4">
          <label className="block text-[12px] text-[var(--text-secondary)]">
            <span className="mb-1 block">{t("wiki.pageForm.path")}</span>
            <input
              ref={pathRef}
              autoFocus
              value={path}
              onChange={(event) => {
                setPath(event.target.value);
                if (mode === "create") setPageType(inferPageType(event.target.value));
              }}
              className="h-[32px] w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] px-2 font-mono text-[12px] text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
            />
          </label>
          {mode === "create" ? (
            <>
              <label className="block text-[12px] text-[var(--text-secondary)]">
                <span className="mb-1 block">{t("wiki.pageForm.titleLabel")}</span>
                <input
                  value={title}
                  onChange={(event) => setTitle(event.target.value)}
                  className="h-[32px] w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] px-2 text-[12px] text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                />
              </label>
              <label className="block text-[12px] text-[var(--text-secondary)]">
                <span className="mb-1 block">{t("wiki.pageForm.template")}</span>
                <select
                  value={pageType ?? ""}
                  onChange={(event) => setPageType((event.target.value || null) as WikiPageType | null)}
                  className="h-[32px] w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] px-2 text-[12px] text-[var(--text-primary)] outline-none focus:border-[var(--accent)]"
                >
                  <option value="">{t("wiki.pageForm.template.blank")}</option>
                  {WIKI_PAGE_TYPES.map((type) => (
                    <option value={type} key={type}>{t(`wiki.type.${type}`)}</option>
                  ))}
                </select>
              </label>
            </>
          ) : null}
        </div>
        <footer className="flex h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <button type="button" onClick={onCancel} className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px]">
            {t("confirmation.cancel")}
          </button>
          <button type="submit" disabled={!valid} className="h-[28px] rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] disabled:opacity-40">
            {t(`wiki.pageForm.${mode}.submit`)}
          </button>
        </footer>
      </form>
    </div>
  );
}
