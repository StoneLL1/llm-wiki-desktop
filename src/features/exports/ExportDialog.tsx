import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { FileOutput, FileSearch, LockKeyhole, X } from "lucide-react";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { WikiTree } from "../../types/wiki";
import {
  DEFAULT_EXPORT_OPTIONS,
  EXPORT_TYPE_ORDER,
  SINGLE_PAGE_EXPORT_TYPES,
  type ExportContentOptions,
  type ExportRoutePreference,
  type ExportRestrictedContentStatus,
  type ExportType,
} from "../../types/export";

export interface ExportDialogResult {
  type: ExportType;
  sourcePath: string;
  route: ExportRoutePreference;
  template: string | null;
  options: ExportContentOptions;
  openPreview: boolean;
  acknowledgeRestrictedContent: boolean;
}

interface ExportDialogProps {
  open: boolean;
  initialType?: ExportType;
  initialSourcePath?: string;
  projectId: string;
  rootPath: string;
  onClose: () => void;
  onGenerate: (result: ExportDialogResult) => void;
}

const TEMPLATE_OPTIONS = [
  { value: "default-serif", labelKey: "exports.dialog.template.default" },
  { value: "modern-sans", labelKey: "exports.dialog.template.modern" },
  { value: "editorial-magazine", labelKey: "exports.dialog.template.editorial" },
] as const;

const ROUTE_OPTIONS: { value: ExportRoutePreference; labelKey: string }[] = [
  { value: "auto", labelKey: "exports.dialog.route.auto" },
  { value: "agent", labelKey: "exports.dialog.route.agent" },
  { value: "byok", labelKey: "exports.dialog.route.byok" },
];

const DEFAULT_TEMPLATE = "default-serif";

interface BrowsePage {
  path: string;
  title: string;
}

export function ExportDialog({
  open,
  initialType = "beautiful_read",
  initialSourcePath = "",
  projectId,
  rootPath,
  onClose,
  onGenerate,
}: ExportDialogProps) {
  const { t } = useTranslation();
  const containerRef = useModalDialog({ open, onClose });

  const [type, setType] = useState<ExportType>(initialType);
  const [sourcePath, setSourcePath] = useState(initialSourcePath);
  const [template, setTemplate] = useState<string>(DEFAULT_TEMPLATE);
  const [route, setRoute] = useState<ExportRoutePreference>("auto");
  const [options, setOptions] = useState<ExportContentOptions>(DEFAULT_EXPORT_OPTIONS);
  const [openPreview, setOpenPreview] = useState(true);
  const [restrictedStatus, setRestrictedStatus] = useState<ExportRestrictedContentStatus | null>(null);
  const [acknowledgeRestrictedContent, setAcknowledgeRestrictedContent] = useState(false);

  const [browseOpen, setBrowseOpen] = useState(false);
  const [browseLoading, setBrowseLoading] = useState(false);
  const [pages, setPages] = useState<BrowsePage[] | null>(null);
  const [query, setQuery] = useState("");

  // Reset the form each time the dialog opens so stale state from a prior
  // submission never carries over.
  useEffect(() => {
    if (open) {
      setType(initialType);
      setSourcePath(initialSourcePath);
      setTemplate(DEFAULT_TEMPLATE);
      setRoute("auto");
      setOptions(DEFAULT_EXPORT_OPTIONS);
      setOpenPreview(true);
      setRestrictedStatus(null);
      setAcknowledgeRestrictedContent(false);
      setBrowseOpen(false);
      setQuery("");
    }
  }, [open, initialType, initialSourcePath]);

  const needsSource = SINGLE_PAGE_EXPORT_TYPES.includes(type);
  const canGenerate = (!needsSource || sourcePath.trim().length > 0)
    && (!restrictedStatus?.containsRestrictedContent || acknowledgeRestrictedContent);

  useEffect(() => {
    if (!open || (needsSource && sourcePath.trim().length === 0)) {
      setRestrictedStatus(null);
      setAcknowledgeRestrictedContent(false);
      return;
    }
    let current = true;
    setRestrictedStatus(null);
    setAcknowledgeRestrictedContent(false);
    void invoke<ExportRestrictedContentStatus>("get_export_restricted_content_status", {
      request: {
        projectId,
        projectRootPath: rootPath,
        exportType: type,
        sourcePath: needsSource ? sourcePath.trim() : null,
      },
    }).then((status) => {
      if (current) setRestrictedStatus(status);
    }).catch(() => {
      if (current) setRestrictedStatus(null);
    });
    return () => {
      current = false;
    };
  }, [needsSource, open, projectId, rootPath, sourcePath, type]);

  const openBrowse = async () => {
    setBrowseOpen(true);
    if (pages === null) {
      setBrowseLoading(true);
      try {
        const tree = await invoke<WikiTree>("scan_wiki", {
          request: { projectId, projectRootPath: rootPath },
        });
        setPages(tree.pages.map((page) => ({ path: page.path, title: page.title })));
      } catch {
        setPages([]);
      } finally {
        setBrowseLoading(false);
      }
    }
  };

  const filteredPages = useMemo(() => {
    if (!pages) return [];
    const needle = query.trim().toLowerCase();
    if (!needle) return pages;
    return pages.filter(
      (page) =>
        page.path.toLowerCase().includes(needle) ||
        page.title.toLowerCase().includes(needle),
    );
  }, [pages, query]);

  if (!open) return null;

  const handleSubmit = () => {
    if (!canGenerate) return;
    onGenerate({
      type,
      sourcePath: needsSource ? sourcePath.trim() : "",
      route,
      template,
      options,
      openPreview,
      acknowledgeRestrictedContent,
    });
  };

  return (
    <div
      ref={containerRef}
      aria-modal="true"
      className="dialog-overlay"
      role="dialog"
      aria-labelledby="export-dialog-title"
      tabIndex={-1}
    >
      <div className="dialog dialog--wide">
        <header className="dialog__head">
          <h2 id="export-dialog-title" className="dialog__title">
            {t("exports.dialog.title")}
          </h2>
          <button
            type="button"
            aria-label={t("exports.actions.cancel")}
            className="btn btn--ghost btn--icon btn--sm"
            onClick={onClose}
            style={{ marginLeft: "auto" }}
          >
            <X size={16} aria-hidden />
          </button>
        </header>

        <div className="dialog__body">
          <div className="formrow">
            <div>
              <div className="formrow__label">{t("exports.dialog.type")}</div>
              <div className="formrow__hint">{t("exports.dialog.typeHint")}</div>
            </div>
            <div className="formrow__control">
              <div className="seg" role="radiogroup" aria-label={t("exports.dialog.type")}>
                {EXPORT_TYPE_ORDER.map((value) => (
                  <button
                    key={value}
                    type="button"
                    role="radio"
                    aria-checked={type === value}
                    className={type === value ? "is-active" : undefined}
                    onClick={() => setType(value)}
                  >
                    {t(`exports.type.${value}`)}
                  </button>
                ))}
              </div>
            </div>
          </div>

          {restrictedStatus?.containsRestrictedContent ? (
            <div className="rounded-[var(--radius-md)] border border-[var(--warning-border)] bg-[var(--warning-subtle)] px-3 py-2.5 text-[12px] text-[var(--warning-text)]" role="alert">
              <div className="flex items-start gap-2">
                <LockKeyhole size={15} className="mt-0.5 shrink-0" aria-hidden="true" />
                <div className="space-y-2">
                  <p className="m-0 leading-5">
                    {t("exports.restricted.warning", { count: restrictedStatus.restrictedSourceCount })}
                  </p>
                  <label className="flex cursor-pointer items-start gap-2 text-[11px] leading-4">
                    <input
                      type="checkbox"
                      className="mt-0.5"
                      checked={acknowledgeRestrictedContent}
                      onChange={(event) => setAcknowledgeRestrictedContent(event.target.checked)}
                    />
                    <span>{t("exports.restricted.acknowledge")}</span>
                  </label>
                </div>
              </div>
            </div>
          ) : null}

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("exports.dialog.source")}</div>
              <div className="formrow__hint">
                {needsSource
                  ? t("exports.dialog.sourceHint")
                  : t("exports.dialog.sourceProjectHint")}
              </div>
            </div>
            <div className="formrow__control">
              {needsSource ? (
                <>
                  <div className="input-group">
                    <span className="input-group__lead">
                      <FileOutput size={14} strokeWidth={1.5} aria-hidden />
                    </span>
                    <input
                      className="input input--mono"
                      type="text"
                      value={sourcePath}
                      onChange={(event) => setSourcePath(event.target.value)}
                      placeholder={t("exports.sourcePlaceholder")}
                      aria-label={t("exports.dialog.source")}
                    />
                    <span className="input-group__trail">
                      <button
                        type="button"
                        className="btn btn--sm"
                        onClick={() => void openBrowse()}
                        aria-expanded={browseOpen}
                      >
                        <FileSearch size={12} strokeWidth={1.5} aria-hidden />
                        {t("exports.dialog.browse")}
                      </button>
                    </span>
                  </div>
                  {browseOpen ? (
                    <div
                      className="mt-2 rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--surface)]"
                      role="listbox"
                      aria-label={t("exports.dialog.browseTitle")}
                    >
                      <div className="border-b border-[var(--border-subtle)] px-2 py-1.5">
                        <input
                          className="input input--mono h-[26px] w-full"
                          type="text"
                          value={query}
                          onChange={(event) => setQuery(event.target.value)}
                          placeholder={t("exports.dialog.browseSearch")}
                          aria-label={t("exports.dialog.browseSearch")}
                        />
                      </div>
                      <div className="max-h-[180px] overflow-auto py-1">
                        {filteredPages.length === 0 ? (
                          <div className="px-3 py-2 text-[11px] text-[var(--text-muted)]">
                            {browseLoading
                              ? t("exports.dialog.browseLoading")
                              : t("exports.dialog.browseEmpty")}
                          </div>
                        ) : (
                          filteredPages.map((page) => (
                            <button
                              key={page.path}
                              type="button"
                              role="option"
                              aria-selected={sourcePath === page.path}
                              onClick={() => {
                                setSourcePath(page.path);
                                setBrowseOpen(false);
                                setQuery("");
                              }}
                              className="flex w-full flex-col items-start px-3 py-1.5 text-left hover:bg-[var(--surface-muted)]"
                            >
                              <span className="text-[12px] text-[var(--text-primary)]">
                                {page.title}
                              </span>
                              <span className="font-mono text-[10.5px] text-[var(--text-muted)]">
                                {page.path}
                              </span>
                            </button>
                          ))
                        )}
                      </div>
                    </div>
                  ) : null}
                </>
              ) : (
                <span className="text-[12px]" style={{ color: "var(--text-muted)" }}>
                  {t("exports.dialog.sourceProjectHint")}
                </span>
              )}
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("exports.dialog.template")}</div>
              <div className="formrow__hint">{t("exports.dialog.templateHint")}</div>
            </div>
            <div className="formrow__control">
              <select
                className="select"
                style={{ maxWidth: 260 }}
                value={template}
                onChange={(event) => setTemplate(event.target.value)}
                aria-label={t("exports.dialog.template")}
              >
                {TEMPLATE_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {t(option.labelKey)}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("exports.dialog.route")}</div>
              <div className="formrow__hint">{t("exports.dialog.routeHint")}</div>
            </div>
            <div className="formrow__control">
              <div className="seg" role="radiogroup" aria-label={t("exports.dialog.route")}>
                {ROUTE_OPTIONS.map((option) => (
                  <button
                    key={option.value}
                    type="button"
                    role="radio"
                    aria-checked={route === option.value}
                    className={route === option.value ? "is-active" : undefined}
                    onClick={() => setRoute(option.value)}
                  >
                    {t(option.labelKey)}
                  </button>
                ))}
              </div>
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("exports.dialog.options")}</div>
            </div>
            <div
              className="formrow__control"
              style={{ display: "flex", flexDirection: "column", gap: 6 }}
            >
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={options.includeFrontmatter}
                  onChange={(event) =>
                    setOptions((prev) => ({ ...prev, includeFrontmatter: event.target.checked }))
                  }
                />
                <span>{t("exports.dialog.options.frontmatter")}</span>
              </label>
              <label className="checkbox" title={t("exports.dialog.options.embedCssLocked")}>
                <input type="checkbox" checked disabled readOnly />
                <span>{t("exports.dialog.options.embedCss")}</span>
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={options.embedImages}
                  onChange={(event) =>
                    setOptions((prev) => ({ ...prev, embedImages: event.target.checked }))
                  }
                />
                <span>{t("exports.dialog.options.embedImages")}</span>
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={openPreview}
                  onChange={(event) => setOpenPreview(event.target.checked)}
                />
                <span>{t("exports.dialog.options.openPreview")}</span>
              </label>
            </div>
          </div>
        </div>

        <footer className="dialog__foot">
          <button type="button" className="btn" onClick={onClose}>
            {t("exports.actions.cancel")}
          </button>
          <button
            type="button"
            className="btn btn--primary"
            onClick={handleSubmit}
            disabled={!canGenerate}
          >
            <FileOutput size={14} aria-hidden />
            {t("exports.actions.generate")}
          </button>
        </footer>
      </div>
    </div>
  );
}
