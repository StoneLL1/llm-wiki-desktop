import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Circle, ClipboardPaste, FileText, FolderOpen, Image, Link, LoaderCircle, MessageSquareText, Mic2, Plus, ScanText, Upload } from "lucide-react";

import { pickDirectory, selectImportFiles } from "./nativeFilePicker";
import { subscribeToDragDrop } from "./dragDrop";
import {
  isUnsupportedImportUrl,
  isValidPublicHttpImportUrl,
} from "./importLocator";

export interface ImportSourceMethodsProps {
  onAddPaths: (paths: string[]) => void | Promise<unknown>;
  onAddText?: (content: string, sourceName: string) => void | Promise<unknown>;
  onAddUrl: (url: string) => void | Promise<unknown>;
  addingPaths?: boolean;
  addingText?: boolean;
  addingUrl?: boolean;
  sessionSyncing?: boolean;
  onError?: (error: unknown) => void;
  files?: readonly { id: string; label: string; available: boolean; reasonCode?: string | null }[];
  platforms?: readonly { id: string; label: string; available: boolean; reasonCode?: string | null }[];
  abilities?: readonly { id: string; label: string; available: boolean; reasonCode?: string | null }[];
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  matrixExpanded?: boolean;
  onMatrixExpandedChange?: (expanded: boolean) => void;
  onManageCapabilities?: () => void;
  readinessUnavailable?: boolean;
  readinessRetrying?: boolean;
  onRetryReadiness?: () => void | Promise<void>;
}

function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function ImportSourceMethods({
  onAddPaths,
  onAddText,
  onAddUrl,
  addingPaths = false,
  addingText = false,
  addingUrl = false,
  sessionSyncing = false,
  onError,
  files = [],
  platforms = [],
  abilities = [],
  expanded: controlledExpanded,
  onExpandedChange,
  matrixExpanded: controlledMatrixExpanded,
  onMatrixExpandedChange,
  onManageCapabilities,
  readinessUnavailable = false,
  readinessRetrying = false,
  onRetryReadiness,
}: ImportSourceMethodsProps) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [text, setText] = useState("");
  const [textSourceName, setTextSourceName] = useState("clipboard.md");
  const [dropActive, setDropActive] = useState(false);
  const [submittingUrl, setSubmittingUrl] = useState(false);
  const [submittingText, setSubmittingText] = useState(false);
  const [pickingPaths, setPickingPaths] = useState(false);
  const [textComposerOpen, setTextComposerOpen] = useState(false);
  const [internalExpanded, setInternalExpanded] = useState(true);
  const [internalMatrixExpanded, setInternalMatrixExpanded] = useState(false);
  const [inputError, setInputError] = useState<"files" | "url" | "invalid_url" | "text" | "image_clipboard" | null>(null);
  const expanded = controlledExpanded ?? internalExpanded;
  const matrixExpanded = controlledMatrixExpanded ?? internalMatrixExpanded;
  const unsupportedLocalUrl = useMemo(() => isUnsupportedImportUrl(url), [url]);
  const invalidUrl = useMemo(() => Boolean(url.trim()) && !unsupportedLocalUrl && !isValidPublicHttpImportUrl(url.trim()), [unsupportedLocalUrl, url]);
  const hasUrlFeedback = unsupportedLocalUrl || invalidUrl || inputError === "url";
  const pathBusy = sessionSyncing || addingPaths || addingText || pickingPaths;
  const textBusy = sessionSyncing || addingPaths || addingUrl || addingText || submittingText;
  const textTitle = useMemo(() => {
    const heading = text.split(/\r?\n/).find((line) => /^#\s+\S/.test(line));
    if (heading) return heading.replace(/^#\s+/, "").trim();
    return textSourceName.replace(/\.(md|markdown|txt)$/i, "") || t("importV2.clipboard.fallbackTitle");
  }, [t, text, textSourceName]);
  const pastedUrl = useMemo(() => {
    const value = text.trim();
    return value && !/\s/u.test(value) && isValidPublicHttpImportUrl(value) ? value : null;
  }, [text]);

  const importPathsFrom = useCallback(async (selectPaths: () => Promise<string[]>) => {
    if (pathBusy) return;
    setInputError(null);
    setPickingPaths(true);
    try {
      const paths = await selectPaths();
      if (paths.length > 0) await onAddPaths(paths);
    } catch (error) {
      setInputError("files");
      onError?.(error);
    } finally {
      setPickingPaths(false);
    }
  }, [onAddPaths, onError, pathBusy]);

  const submitPaths = useCallback(
    (paths: string[]) => importPathsFrom(() => Promise.resolve(paths)),
    [importPathsFrom],
  );

  useEffect(() => {
    if (!hasTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        unlisten = await subscribeToDragDrop({
          listen: (handler) => getCurrentWebview().onDragDropEvent(handler),
          isCancelled: () => cancelled,
          onActive: setDropActive,
          onPaths: (paths) => {
            void submitPaths(paths);
          },
        });
      } catch (error) {
        if (!cancelled) {
          setInputError("files");
          onError?.(error);
        }
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onError, submitPaths]);

  const addFiles = () => importPathsFrom(selectImportFiles);

  const addFolder = () => importPathsFrom(async () => {
      const path = await pickDirectory();
      return path ? [path] : [];
    });

  const submitUrl = () => {
    const value = url.trim();
    if (!value || unsupportedLocalUrl || invalidUrl || submittingUrl || addingUrl || sessionSyncing) return;
    setInputError(null);
    setSubmittingUrl(true);
    void Promise.resolve(onAddUrl(value))
      .then(() => setUrl(""))
      .catch((error) => {
        setInputError("url");
        onError?.(error);
      })
      .finally(() => setSubmittingUrl(false));
  };

  const submitText = () => {
    if ((!onAddText && !pastedUrl) || !text.trim() || textBusy) return;
    setInputError(null);
    setSubmittingText(true);
    void Promise.resolve(pastedUrl ? onAddUrl(pastedUrl) : onAddText!(text, textSourceName))
      .then(() => {
        setText("");
        setTextSourceName("clipboard.md");
      })
      .catch((error) => {
        setInputError("text");
        onError?.(error);
      })
      .finally(() => setSubmittingText(false));
  };

  const setMatrixExpanded = (nextExpanded: boolean) => {
    setInternalMatrixExpanded(nextExpanded);
    onMatrixExpandedChange?.(nextExpanded);
  };

  const setExpanded = (nextExpanded: boolean) => {
    setInternalExpanded(nextExpanded);
    onExpandedChange?.(nextExpanded);
  };

  const matrixGroups = [
    {
      id: "files",
      label: t("importV2.matrix.files"),
      icon: FileText,
      entries: files,
    },
    {
      id: "platforms",
      label: t("importV2.matrix.platforms"),
      icon: Circle,
      entries: platforms,
    },
    {
      id: "abilities",
      label: t("importV2.matrix.abilities"),
      icon: MessageSquareText,
      entries: abilities,
    },
  ] as const;

  return (
    <section className="import-v2-methods" aria-label={t("importV2.methods.label")}>
      <header className="import-v2-methods__header">
        <div>
          <h2>{t("importV2.methods.title")}</h2>
          <p>{t("importV2.methods.description")}</p>
        </div>
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          aria-expanded={expanded}
          onClick={() => setExpanded(!expanded)}
        >
          {t(expanded ? "importV2.methods.collapse" : "importV2.methods.expand")}
          <ChevronDown
            size={14}
            aria-hidden="true"
            className={expanded ? "rotate-180" : undefined}
          />
        </button>
      </header>
      {expanded ? (
        <>
          <div className="import-v2-entry-grid">
            <section className="import-v2-entry-block import-v2-entry-block--files" aria-labelledby="import-v2-files-entry-title">
              <div className="import-v2-entry-block__heading">
                <span className="import-v2-entry-block__eyebrow">{t("importV2.files.eyebrow")}</span>
                <h3 id="import-v2-files-entry-title">{t("importV2.files.title")}</h3>
                <p>{t("importV2.files.description")}</p>
              </div>
              <div
                className={`import-v2-dropzone ${dropActive ? "is-active" : ""}`}
                role="button"
                tabIndex={pathBusy ? -1 : 0}
                aria-label={t("importV2.files.drop")}
                aria-busy={pathBusy}
                aria-disabled={pathBusy}
                onDragOver={(event) => event.preventDefault()}
                onDragEnter={() => { if (!pathBusy) setDropActive(true); }}
                onDragLeave={() => setDropActive(false)}
                onDrop={(event) => {
                  event.preventDefault();
                  setDropActive(false);
                  if (pathBusy) return;
                  const paths = Array.from(event.dataTransfer.files)
                    .map((file) => (file as File & { path?: string }).path)
                    .filter((path): path is string => Boolean(path));
                  if (paths.length > 0) void submitPaths(paths);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    void addFiles();
                  }
                }}
              >
                {pathBusy ? <LoaderCircle className="animate-spin" size={14} /> : <Upload size={14} />}
                <span>{pathBusy ? t("importV2.status.adding") : t("importV2.files.drop")}</span>
              </div>
              {inputError === "files" ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.files.error")}</p> : null}
              <div className="import-v2-entry-actions">
                <button type="button" className="btn btn--sm btn--primary" aria-label={t("importV2.files.choose")} onClick={() => void addFiles()} disabled={pathBusy}>
                  {pathBusy ? <LoaderCircle className="animate-spin" size={14} /> : <Upload size={14} />}
                  {pathBusy ? t("importV2.status.adding") : t("importV2.files.choose")}
                </button>
                <button type="button" className="btn btn--sm" onClick={() => void addFolder()} disabled={pathBusy}>
                  <FolderOpen size={14} aria-hidden="true" />
                  {t("importV2.files.chooseFolder")}
                </button>
              </div>
              <span className="import-v2-entry-block__meta">{t("importV2.files.formats")}</span>
            </section>
            <section className="import-v2-entry-block import-v2-entry-block--remote" aria-labelledby="import-v2-url-entry-title">
              <div className="import-v2-entry-block__heading">
                <span className="import-v2-entry-block__eyebrow">{t("importV2.url.eyebrow")}</span>
                <h3 id="import-v2-url-entry-title">{t("importV2.url.title")}</h3>
                <p>{t("importV2.url.description")}</p>
              </div>
              <form
                className={`import-v2-compact-url ${hasUrlFeedback ? "is-invalid" : ""}`}
                onSubmit={(event) => {
                  event.preventDefault();
                  submitUrl();
                }}
              >
                <label className="sr-only" htmlFor="import-v2-url">{t("importV2.url.label")}</label>
                <span className="input-group__lead"><Link size={14} aria-hidden="true" /></span>
                <input
                  id="import-v2-url"
                  type="url"
                  className="input input--mono"
                  aria-label={t("importV2.url.label")}
                  aria-describedby={hasUrlFeedback ? "import-v2-url-feedback" : undefined}
                  aria-invalid={unsupportedLocalUrl || invalidUrl || inputError === "url" ? "true" : undefined}
                  disabled={sessionSyncing || addingUrl || submittingUrl || pathBusy}
                  placeholder={t("importV2.url.placeholder")}
                  value={url}
                  onChange={(event) => { setUrl(event.target.value); setInputError(null); }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      submitUrl();
                    }
                  }}
                />
                <button type="submit" className="btn btn--sm btn--primary" aria-label={t("importV2.url.submit")} disabled={!url.trim() || unsupportedLocalUrl || invalidUrl || submittingUrl || addingUrl || pathBusy}>
                  {submittingUrl || addingUrl ? <LoaderCircle className="animate-spin" size={14} /> : <Plus size={14} />}
                  <span>{t("importV2.url.submit")}</span>
                </button>
              </form>
              {hasUrlFeedback ? (
                <div id="import-v2-url-feedback">
                  {unsupportedLocalUrl ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.url.localUnsupported")}</p> : null}
                  {invalidUrl ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.url.invalid")}</p> : null}
                  {inputError === "url" ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.url.error")}</p> : null}
                </div>
              ) : null}
              <button
                type="button"
                className="btn btn--sm import-v2-entry-actions__secondary"
                aria-expanded={textComposerOpen}
                aria-controls="import-v2-text-composer"
                onClick={() => setTextComposerOpen((open) => !open)}
                disabled={!onAddText}
              >
                <ClipboardPaste size={14} aria-hidden="true" />
                {t("importV2.clipboard.title")}
              </button>
            </section>
          </div>
          {textComposerOpen ? (
            <section id="import-v2-text-composer" className="import-v2-text-composer" aria-label={t("importV2.clipboard.title")}>
              <div className="import-v2-clipboard-fields">
                <div className="min-w-0">
                  <label className="mb-1 block text-[11px] text-[var(--text-secondary)]" htmlFor="import-v2-clipboard-text">
                    {t("importV2.clipboard.content")}
                  </label>
                  <textarea
                    id="import-v2-clipboard-text"
                    className="input min-h-[72px] w-full resize-y font-mono text-[12px]"
                    value={text}
                    disabled={textBusy}
                    placeholder={t("importV2.clipboard.placeholder")}
                    aria-invalid={inputError === "text" || inputError === "image_clipboard" ? "true" : undefined}
                    onPaste={(event) => {
                      const hasImage = Array.from(event.clipboardData.items).some(
                        (item) => item.kind === "file" && item.type.startsWith("image/"),
                      );
                      if (hasImage) {
                        event.preventDefault();
                        setInputError("image_clipboard");
                      }
                    }}
                    onChange={(event) => {
                      setText(event.target.value);
                      setInputError(null);
                    }}
                  />
                </div>
                <div className="min-w-0">
                  <label className="mb-1 block text-[11px] text-[var(--text-secondary)]" htmlFor="import-v2-clipboard-name">
                    {t("importV2.clipboard.name")}
                  </label>
                  <input
                    id="import-v2-clipboard-name"
                    className="input input--mono w-full"
                    value={textSourceName}
                    maxLength={160}
                    disabled={textBusy || Boolean(pastedUrl)}
                    onChange={(event) => setTextSourceName(event.target.value)}
                  />
                  <p className="m-0 mt-2 text-[10.5px] text-[var(--text-muted)]">{t("importV2.clipboard.privacy")}</p>
                </div>
              </div>
              {text.trim() ? (
                <section className="import-v2-clipboard-preview" aria-label={t("importV2.clipboard.preview")}>
                  <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-[var(--border-subtle)] px-3 py-2 text-[11px]">
                    <span className="font-medium text-[var(--text-primary)]">{pastedUrl ?? textTitle}</span>
                    <span className="text-[var(--text-secondary)]">{t(pastedUrl ? "importV2.clipboard.routeUrl" : "importV2.clipboard.route")}</span>
                  </div>
                  <pre className="m-0 max-h-24 overflow-auto whitespace-pre-wrap break-words px-3 py-2 font-mono text-[11px] text-[var(--text-secondary)]">{text}</pre>
                </section>
              ) : null}
              {inputError === "image_clipboard" ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.clipboard.imageUnsupported")}</p> : null}
              {inputError === "text" ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.clipboard.error")}</p> : null}
              <button
                type="button"
                className="btn btn--sm btn--primary self-start"
                disabled={(!onAddText && !pastedUrl) || !text.trim() || textBusy}
                aria-busy={submittingText || addingText}
                onClick={submitText}
              >
                {submittingText || addingText ? <LoaderCircle className="animate-spin" size={14} aria-hidden="true" /> : <Plus size={14} aria-hidden="true" />}
                {t(submittingText || addingText ? "importV2.status.adding" : "importV2.clipboard.confirm")}
              </button>
            </section>
          ) : null}
        </>
      ) : null}
      <div
        className={`import-v2-source-matrix ${matrixExpanded ? "is-expanded" : "is-collapsed"}`}
        aria-label={t("importV2.matrix.label")}
      >
        {readinessUnavailable ? (
          <div role="status" className="flex items-center justify-between gap-3 border-b border-[var(--border-subtle)] px-3 py-2 text-[11px] text-[var(--warning-text)]">
            <span>{t("importV2.matrix.unavailable")}</span>
            {onRetryReadiness ? <button type="button" className="btn btn--sm" disabled={readinessRetrying} onClick={() => void onRetryReadiness()}>{t(readinessRetrying ? "importV2.matrix.retrying" : "importV2.matrix.retry")}</button> : null}
          </div>
        ) : null}
        <button
          type="button"
          className="import-v2-source-matrix__summary"
          aria-label={`${t(matrixExpanded ? "importV2.matrix.collapse" : "importV2.matrix.expand")}. ${matrixGroups
            .map((group) => t("importV2.matrix.summary", {
              label: group.label,
              available: group.entries.filter((entry) => entry.available).length,
              total: group.entries.length,
            }))
            .join("; ")}`}
          aria-expanded={matrixExpanded}
          aria-controls="import-v2-source-matrix-content"
          onClick={() => setMatrixExpanded(!matrixExpanded)}
        >
          <span className="import-v2-source-matrix__title">{t("importV2.matrix.label")}</span>
          <span className="import-v2-source-matrix__metrics">
            {matrixGroups.map((group) => {
              const available = group.entries.filter((entry) => entry.available).length;
              const Icon = group.icon;
              return (
                <span
                  key={group.id}
                  className="import-v2-source-matrix__metric"
                  aria-label={t("importV2.matrix.summary", {
                    label: group.label,
                    available,
                    total: group.entries.length,
                  })}
                >
                  <Icon size={13} aria-hidden="true" />
                  <span>{group.label}</span>
                  <strong>{available}/{group.entries.length}</strong>
                </span>
              );
            })}
          </span>
          <span className="import-v2-source-matrix__toggle">
            {t(matrixExpanded ? "importV2.matrix.collapse" : "importV2.matrix.expand")}
            <ChevronDown
              size={14}
              aria-hidden="true"
              className={matrixExpanded ? "rotate-180" : undefined}
            />
          </span>
        </button>
        {matrixExpanded ? (
          <div id="import-v2-source-matrix-content" className="import-v2-source-matrix__content">
            <CapabilityRow
              label={t("importV2.matrix.files")}
              entries={files.map((file) => ({ ...file, tone: file.available ? "type" as const : "off" as const }))}
              icon={FileText}
              idPrefix="files"
              onManageCapabilities={onManageCapabilities}
            />
            <CapabilityRow
              label={t("importV2.matrix.platforms")}
              entries={platforms.map((platform) => ({
                ...platform,
                tone: platform.available ? "ok" as const
                  : platform.reasonCode === "capability_missing" ? "pack" as const
                    : platform.reasonCode === "login_required" ? "login" as const : "off" as const,
              }))}
              icon={Circle}
              idPrefix="platforms"
              onManageCapabilities={onManageCapabilities}
            />
            <CapabilityRow
              label={t("importV2.matrix.abilities")}
              entries={abilities.map((ability) => ({
                ...ability,
                tone: ability.available ? "ok" as const
                  : ability.reasonCode === "capability_missing" ? "pack" as const : "off" as const,
              }))}
              icon={MessageSquareText}
              icons={{ local_asr: Mic2, ocr: ScanText, keyframes: Image }}
              idPrefix="abilities"
              onManageCapabilities={onManageCapabilities}
            />
          </div>
        ) : null}
      </div>
    </section>
  );
}

type MatrixTone = "type" | "ok" | "login" | "pack" | "off";

function CapabilityRow({ label, entries, icon: DefaultIcon, icons = {}, idPrefix, onManageCapabilities }: {
  label: string;
  entries: readonly { id: string; label: string; available: boolean; reasonCode?: string | null; tone: MatrixTone }[];
  icon: typeof Circle;
  icons?: Record<string, typeof Circle>;
  idPrefix: string;
  onManageCapabilities?: () => void;
}) {
  const { t } = useTranslation();
  const [pinnedId, setPinnedId] = useState<string | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const rowRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!pinnedId) return;
    const closeOutside = (event: PointerEvent) => {
      if (!rowRef.current?.contains(event.target as Node)) setPinnedId(null);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        const id = pinnedId;
        setPinnedId(null);
        Array.from(rowRef.current?.querySelectorAll<HTMLButtonElement>("[data-capability-id]") ?? [])
          .find((button) => button.dataset.capabilityId === id)
          ?.focus();
      }
    };
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [pinnedId]);

  const statusReason = (entry: { available: boolean; reasonCode?: string | null }) => entry.reasonCode
    ? t(`importV2.platform.reason.${entry.reasonCode}`, { defaultValue: t(entry.available ? "importV2.platform.available" : "importV2.platform.unavailable") })
    : t(entry.available ? "importV2.platform.available" : "importV2.platform.unavailable");
  const activeId = pinnedId ?? hoveredId;
  const activeEntry = entries.find((entry) => entry.id === activeId) ?? null;
  const activeSummaryId = activeEntry ? `import-capability-summary-${idPrefix}-${activeEntry.id}` : null;

  return (
    <div className="import-v2-source-matrix__row" ref={rowRef}>
      <span className="import-v2-source-matrix__label">{label}</span>
      <div className="import-v2-source-matrix__tiles">
        {entries.map((entry) => {
          const Icon = icons[entry.id] ?? DefaultIcon;
          const summaryId = `import-capability-summary-${idPrefix}-${entry.id}`;
          const reason = statusReason(entry);
          return (
            <span key={entry.id} className="import-v2-source-tile-wrap">
              <button
                type="button"
                data-capability-id={entry.id}
                className={`import-v2-source-tile is-${entry.tone}`}
                aria-label={`${entry.label}: ${reason}`}
                aria-expanded={pinnedId === entry.id}
                aria-describedby={
                  pinnedId === entry.id || hoveredId === entry.id
                    ? summaryId
                    : undefined
                }
                onMouseEnter={() => setHoveredId(entry.id)}
                onMouseLeave={() => setHoveredId(null)}
                onFocus={() => setHoveredId(entry.id)}
                onBlur={() => setHoveredId(null)}
                onClick={() => setPinnedId((current) => current === entry.id ? null : entry.id)}
              >
                <Icon size={14} aria-hidden="true" />
                <span>{entry.label}</span>
                <i aria-hidden="true" />
              </button>
            </span>
          );
        })}
      </div>
      {activeEntry && activeSummaryId ? (
        <span
          id={activeSummaryId}
          className="import-v2-source-popover"
          role={pinnedId === activeEntry.id ? "dialog" : "tooltip"}
          aria-label={pinnedId === activeEntry.id ? activeEntry.label : undefined}
        >
          <strong>{activeEntry.label}</strong>
          <span>{statusReason(activeEntry)}</span>
          {!activeEntry.available && pinnedId === activeEntry.id && onManageCapabilities ? (
            <button
              type="button"
              onClick={() => {
                setPinnedId(null);
                onManageCapabilities();
              }}
            >
              {t("importV2.matrix.manage")}
            </button>
          ) : null}
        </span>
      ) : null}
    </div>
  );
}
