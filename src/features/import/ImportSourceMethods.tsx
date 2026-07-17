import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { FolderOpen, Link, LoaderCircle, Plus, Upload } from "lucide-react";

import { pickDirectory, selectImportFiles } from "./nativeFilePicker";
import { subscribeToDragDrop } from "./dragDrop";

export interface ImportSourceMethodsProps {
  onAddPaths: (paths: string[]) => void | Promise<unknown>;
  onAddUrl: (url: string) => void | Promise<unknown>;
  addingPaths?: boolean;
  addingUrl?: boolean;
  sessionSyncing?: boolean;
  onError?: (error: unknown) => void;
  platforms?: readonly { label: string; available: boolean; reasonCode?: string | null }[];
}

function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function isUnsupportedLocalUrl(value: string): boolean {
  const normalized = value.trim().toLowerCase();
  return (
    normalized.startsWith("file:") ||
    normalized.startsWith("data:") ||
    normalized.startsWith("javascript:") ||
    /^https?:\/\/(localhost|127(?:\.\d{1,3}){3}|0\.0\.0\.0|\[::1\])(?:[:/]|$)/.test(normalized)
  );
}

function isValidPublicHttpUrl(value: string): boolean {
  try {
    const parsed = new URL(value);
    return (parsed.protocol === "http:" || parsed.protocol === "https:") && !isUnsupportedLocalUrl(value);
  } catch {
    return false;
  }
}

export function ImportSourceMethods({
  onAddPaths,
  onAddUrl,
  addingPaths = false,
  addingUrl = false,
  sessionSyncing = false,
  onError,
  platforms = [
    { label: "HTTP", available: true },
    { label: "WeChat", available: true },
    { label: "Zhihu", available: false },
    { label: "Bilibili", available: false },
    { label: "Xiaohongshu", available: false },
    { label: "X", available: false },
  ],
}: ImportSourceMethodsProps) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [dropActive, setDropActive] = useState(false);
  const [submittingUrl, setSubmittingUrl] = useState(false);
  const [pickingPaths, setPickingPaths] = useState(false);
  const [inputError, setInputError] = useState<"files" | "url" | "invalid_url" | null>(null);
  const urlInputRef = useRef<HTMLInputElement>(null);
  const unsupportedLocalUrl = useMemo(() => isUnsupportedLocalUrl(url), [url]);
  const invalidUrl = useMemo(() => Boolean(url.trim()) && !unsupportedLocalUrl && !isValidPublicHttpUrl(url.trim()), [unsupportedLocalUrl, url]);
  const pathBusy = sessionSyncing || addingPaths || pickingPaths;

  const submitPaths = useCallback(async (paths: string[]) => {
    if (paths.length === 0 || pathBusy) return;
    setInputError(null);
    setPickingPaths(true);
    try {
      await onAddPaths(paths);
    } catch (error) {
      setInputError("files");
      onError?.(error);
    } finally {
      setPickingPaths(false);
    }
  }, [onAddPaths, onError, pathBusy]);

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

  const addFiles = async () => {
    if (pathBusy) return;
    setInputError(null);
    setPickingPaths(true);
    try {
      const paths = await selectImportFiles();
      await onAddPaths(paths);
    } catch (error) {
      setInputError("files");
      onError?.(error);
    } finally {
      setPickingPaths(false);
    }
  };

  const addFolder = async () => {
    if (pathBusy) return;
    setInputError(null);
    setPickingPaths(true);
    try {
      const path = await pickDirectory();
      if (path) await onAddPaths([path]);
    } catch (error) {
      setInputError("files");
      onError?.(error);
    } finally {
      setPickingPaths(false);
    }
  };

  const submitUrl = async () => {
    const value = url.trim();
    if (!value || unsupportedLocalUrl || invalidUrl || submittingUrl || addingUrl || sessionSyncing) return;
    setInputError(null);
    setSubmittingUrl(true);
    try {
      await onAddUrl(value);
      setUrl("");
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
            void submitUrl();
          }}
        >
          <label className="sr-only" htmlFor="import-v2-url">{t("importV2.url.label")}</label>
          <div className="input-group">
            <span className="input-group__lead"><Link size={14} /></span>
            <input
              id="import-v2-url"
              ref={urlInputRef}
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
                  void submitUrl();
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
        <div className="flex flex-wrap gap-1" aria-label={t("importV2.url.platforms")}>
          {platforms.map((platform) => (
            <span
              key={platform.label}
              className={`import-v2-platform-chip ${platform.available ? "is-available" : "is-unavailable"}`}
              aria-label={`${platform.label}: ${t(platform.available ? "importV2.platform.available" : "importV2.platform.unavailable")}${!platform.available && platform.reasonCode ? `, ${t(`importV2.platform.reason.${platform.reasonCode}`, { defaultValue: t("importV2.platform.unavailable") })}` : ""}`}
              title={!platform.available && platform.reasonCode ? t(`importV2.platform.reason.${platform.reasonCode}`, { defaultValue: t("importV2.platform.unavailable") }) : undefined}
            >
              {platform.label}
            </span>
          ))}
        </div>
      </article>
    </section>
  );
}
