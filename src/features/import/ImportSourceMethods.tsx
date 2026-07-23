import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Circle, FileText, FolderOpen, Image, Link, LoaderCircle, MessageSquareText, Mic2, Plus, ScanText, Upload } from "lucide-react";

import { pickDirectory, selectImportFiles } from "./nativeFilePicker";
import { subscribeToDragDrop } from "./dragDrop";
import type { MediaSaveMode } from "../../types/importV2";
import {
  isMediaCandidateUrl,
  isUnsupportedImportUrl,
  isValidPublicHttpImportUrl,
} from "./importLocator";

export interface ImportSourceMethodsProps {
  onAddPaths: (paths: string[]) => void | Promise<unknown>;
  onAddUrl: (url: string, mediaSaveMode?: MediaSaveMode) => void | Promise<unknown>;
  addingPaths?: boolean;
  addingUrl?: boolean;
  sessionSyncing?: boolean;
  onError?: (error: unknown) => void;
  files?: readonly { id: string; label: string; available: boolean; reasonCode?: string | null }[];
  platforms?: readonly { id: string; label: string; available: boolean; reasonCode?: string | null }[];
  abilities?: readonly { id: string; label: string; available: boolean; reasonCode?: string | null }[];
}

function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function ImportSourceMethods({
  onAddPaths,
  onAddUrl,
  addingPaths = false,
  addingUrl = false,
  sessionSyncing = false,
  onError,
  files = ["DOCX", "PDF", "XLSX", "PPT", "MD", "TXT", "CSV"].map((label) => ({ id: label.toLowerCase(), label, available: true })),
  platforms = [
    { id: "http", label: "HTTP", available: true },
    { id: "wechat", label: "WeChat", available: true },
    { id: "zhihu", label: "Zhihu", available: false },
    { id: "bilibili", label: "Bilibili", available: false },
    { id: "xiaohongshu", label: "Xiaohongshu", available: false },
    { id: "douyin", label: "Douyin", available: false },
    { id: "x", label: "X", available: false },
  ],
  abilities = [],
}: ImportSourceMethodsProps) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [dropActive, setDropActive] = useState(false);
  const [submittingUrl, setSubmittingUrl] = useState(false);
  const [pickingPaths, setPickingPaths] = useState(false);
  const [inputError, setInputError] = useState<"files" | "url" | "invalid_url" | null>(null);
  const [pendingUrl, setPendingUrl] = useState<string | null>(null);
  const unsupportedLocalUrl = useMemo(() => isUnsupportedImportUrl(url), [url]);
  const invalidUrl = useMemo(() => Boolean(url.trim()) && !unsupportedLocalUrl && !isValidPublicHttpImportUrl(url.trim()), [unsupportedLocalUrl, url]);
  const pathBusy = sessionSyncing || addingPaths || pickingPaths;

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
    if (isMediaCandidateUrl(value)) {
      setPendingUrl(value);
      return;
    }
    setSubmittingUrl(true);
    void Promise.resolve(onAddUrl(value))
      .then(() => setUrl(""))
      .catch((error) => {
        setInputError("url");
        onError?.(error);
      })
      .finally(() => setSubmittingUrl(false));
  };

  const confirmUrl = async (mediaSaveMode: MediaSaveMode) => {
    if (!pendingUrl || submittingUrl) return;
    setSubmittingUrl(true);
    try {
      await onAddUrl(pendingUrl, mediaSaveMode);
      setUrl("");
      setPendingUrl(null);
    } catch (error) {
      setInputError("url");
      onError?.(error);
    } finally {
      setSubmittingUrl(false);
    }
  };

  return (
    <section className="import-v2-methods" aria-label={t("importV2.methods.label")}>
      <article className="import-v2-method-pane">
        <h2 className="m-0 text-[16px] font-semibold">{t("importV2.files.title")}</h2>
        <div
          className={`import-v2-dropzone ${dropActive ? "is-active" : ""}`}
          role="region"
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
        >
          {pathBusy ? <LoaderCircle className="animate-spin" size={18} /> : <Upload size={18} />}
          <span>{pathBusy ? t("importV2.status.adding") : t("importV2.files.drop")}</span>
        </div>
        <div className="flex flex-wrap gap-2">
          <button type="button" className="btn btn--sm btn--primary" aria-label={t("importV2.files.choose")} onClick={() => void addFiles()} disabled={pathBusy}>
            {pathBusy ? <LoaderCircle className="animate-spin" size={14} /> : <Upload size={14} />}
            {pathBusy ? t("importV2.status.adding") : t("importV2.files.choose")}
          </button>
          <button type="button" className="btn btn--sm" onClick={() => void addFolder()} disabled={pathBusy}>
            {pathBusy ? <LoaderCircle className="animate-spin" size={14} /> : <FolderOpen size={14} />}
            {t("importV2.files.chooseFolder")}
          </button>
        </div>
        {inputError === "files" ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.files.error")}</p> : null}
      </article>

      <article className="import-v2-method-pane">
        <h2 className="m-0 text-[16px] font-semibold">{t("importV2.url.title")}</h2>
        <form
          className="space-y-2"
          onSubmit={(event) => {
            event.preventDefault();
            submitUrl();
          }}
        >
          <label className="sr-only" htmlFor="import-v2-url">{t("importV2.url.label")}</label>
          <div className="input-group">
            <span className="input-group__lead"><Link size={14} /></span>
            <input
              id="import-v2-url"
              type="url"
              className="input input--mono"
              aria-label={t("importV2.url.label")}
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
            <button type="submit" className="btn btn--sm btn--primary" disabled={!url.trim() || unsupportedLocalUrl || invalidUrl || submittingUrl || addingUrl || sessionSyncing}>
              {submittingUrl || addingUrl ? <LoaderCircle className="animate-spin" size={14} /> : <Plus size={14} />}
              {t("importV2.url.submit")}
            </button>
          </div>
          {unsupportedLocalUrl ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.url.localUnsupported")}</p> : null}
          {invalidUrl ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.url.invalid")}</p> : null}
          {inputError === "url" ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.url.error")}</p> : null}
        </form>
      </article>

      <div className="import-v2-source-matrix" aria-label={t("importV2.matrix.label")}>
        <CapabilityRow
          label={t("importV2.matrix.files")}
          entries={files.map((file) => ({ ...file, tone: file.available ? "type" as const : "off" as const }))}
          icon={FileText}
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
        />
      </div>

      {pendingUrl ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 p-4" role="dialog" aria-modal="true" aria-labelledby="import-url-media-choice-title">
          <div className="w-full max-w-[480px] border border-[var(--border)] bg-[var(--surface)] p-5 shadow-xl">
            <div className="mb-4">
              <h2 id="import-url-media-choice-title" className="m-0 text-[16px] font-semibold">{t("importV2.url.mediaChoice.title")}</h2>
              <p className="mt-2 mb-0 break-all text-[12px] text-[var(--text-secondary)]">{pendingUrl}</p>
              <p className="mt-2 mb-0 text-[12px] text-[var(--text-secondary)]">{t("importV2.url.mediaChoice.description")}</p>
            </div>
            <div className="grid gap-2 sm:grid-cols-2">
              <button type="button" className="btn btn--sm btn--primary h-auto min-h-[64px] justify-start whitespace-normal text-left" disabled={submittingUrl} onClick={() => void confirmUrl("preserve_original")}>
                <span><strong className="block">{t("importV2.url.mediaChoice.preserve")}</strong><small className="text-[11px] text-[var(--text-secondary)]">{t("importV2.url.mediaChoice.preserveHint")}</small></span>
              </button>
              <button type="button" className="btn btn--sm h-auto min-h-[64px] justify-start whitespace-normal text-left" disabled={submittingUrl} onClick={() => void confirmUrl("extract_only")}>
                <span><strong className="block">{t("importV2.url.mediaChoice.extractOnly")}</strong><small className="text-[11px] text-[var(--text-secondary)]">{t("importV2.url.mediaChoice.extractOnlyHint")}</small></span>
              </button>
            </div>
            <div className="mt-4 flex justify-end">
              <button type="button" className="btn btn--sm" disabled={submittingUrl} onClick={() => setPendingUrl(null)}>{t("confirmation.cancel")}</button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}

type MatrixTone = "type" | "ok" | "login" | "pack" | "off";

function CapabilityRow({ label, entries, icon: DefaultIcon, icons = {} }: {
  label: string;
  entries: readonly { id: string; label: string; available: boolean; reasonCode?: string | null; tone: MatrixTone }[];
  icon: typeof Circle;
  icons?: Record<string, typeof Circle>;
}) {
  const { t } = useTranslation();
  return (
    <div className="import-v2-source-matrix__row">
      <span className="import-v2-source-matrix__label">{label}</span>
      <div className="import-v2-source-matrix__tiles">
        {entries.map((entry) => {
          const Icon = icons[entry.id] ?? DefaultIcon;
          const reason = entry.reasonCode
            ? t(`importV2.platform.reason.${entry.reasonCode}`, { defaultValue: t(entry.available ? "importV2.platform.available" : "importV2.platform.unavailable") })
            : t(entry.available ? "importV2.platform.available" : "importV2.platform.unavailable");
          return (
            <span key={entry.id} className={`import-v2-source-tile is-${entry.tone}`} title={reason} aria-label={`${entry.label}: ${reason}`}>
              <Icon size={14} aria-hidden="true" />
              <span>{entry.label}</span>
              {entry.tone !== "type" ? <i aria-hidden="true" /> : null}
            </span>
          );
        })}
      </div>
    </div>
  );
}
