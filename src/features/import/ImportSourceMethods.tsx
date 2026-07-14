import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { FolderOpen, Link, LoaderCircle, Plus, Upload } from "lucide-react";

import { pickDirectory, selectImportFiles } from "./nativeFilePicker";
import { subscribeToDragDrop } from "./dragDrop";

export interface ImportSourceMethodsProps {
  onAddPaths: (paths: string[]) => void;
  onAddUrl: (url: string) => void | Promise<unknown>;
  addingPaths?: boolean;
  addingUrl?: boolean;
  onError?: (error: unknown) => void;
  platforms?: readonly { label: string; available: boolean }[];
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

export function ImportSourceMethods({
  onAddPaths,
  onAddUrl,
  addingPaths = false,
  addingUrl = false,
  onError,
  platforms = [
    { label: "HTTP", available: true },
    // Connector readiness is not exposed by the current typed backend DTO.
    // Keep platform-specific routes fail-closed until a signed capability
    // status is supplied by the workflow rather than implying production
    // support from the connector names alone.
    { label: "WeChat", available: false },
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
  const unsupportedLocalUrl = useMemo(() => isUnsupportedLocalUrl(url), [url]);

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
            if (paths.length > 0) onAddPaths(paths);
          },
        });
      } catch (error) {
        if (!cancelled) onError?.(error);
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [onAddPaths, onError]);

  const addFiles = async () => {
    try {
      const paths = await selectImportFiles();
      if (paths.length > 0) onAddPaths(paths);
    } catch (error) {
      onError?.(error);
    }
  };

  const addFolder = async () => {
    try {
      const path = await pickDirectory();
      if (path) onAddPaths([path]);
    } catch (error) {
      onError?.(error);
    }
  };

  const submitUrl = async () => {
    const value = url.trim();
    if (!value || unsupportedLocalUrl || submittingUrl || addingUrl) return;
    setSubmittingUrl(true);
    try {
      await onAddUrl(value);
      setUrl("");
    } catch (error) {
      onError?.(error);
    } finally {
      setSubmittingUrl(false);
    }
  };

  return (
    <section className="import-v2-methods" aria-label={t("importV2.methods.label")}>
      <article className="import-v2-method-pane">
        <p className="import-v2-method-pane__eyebrow">{t("importV2.files.eyebrow")}</p>
        <h2 className="m-0 text-[16px] font-semibold">{t("importV2.files.title")}</h2>
        <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("importV2.files.description")}</p>
        <button
          type="button"
          className={`import-v2-dropzone ${dropActive ? "is-active" : ""}`}
          aria-label={t("importV2.files.drop")}
          onClick={() => void addFiles()}
          onDragOver={(event) => event.preventDefault()}
          onDragEnter={() => setDropActive(true)}
          onDragLeave={() => setDropActive(false)}
          onDrop={(event) => {
            event.preventDefault();
            setDropActive(false);
          }}
          disabled={addingPaths}
        >
          {addingPaths ? <LoaderCircle className="animate-spin" size={18} /> : <Upload size={18} />}
          <span>{t("importV2.files.drop")}</span>
        </button>
        <div className="flex flex-wrap gap-2">
          <button type="button" className="btn btn--sm btn--primary" onClick={() => void addFiles()} disabled={addingPaths}>
            <Upload size={14} />
            {addingPaths ? t("importV2.status.adding") : t("importV2.files.choose")}
          </button>
          <button type="button" className="btn btn--sm" onClick={() => void addFolder()} disabled={addingPaths}>
            <FolderOpen size={14} />
            {t("importV2.files.chooseFolder")}
          </button>
        </div>
        <p className="m-0 font-mono text-[11px] text-[var(--text-muted)]">{t("importV2.files.formats")}</p>
        <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.files.hint")}</p>
      </article>

      <article className="import-v2-method-pane">
        <p className="import-v2-method-pane__eyebrow">{t("importV2.url.eyebrow")}</p>
        <h2 className="m-0 text-[16px] font-semibold">{t("importV2.url.title")}</h2>
        <p className="m-0 text-[12px] text-[var(--text-secondary)]">{t("importV2.url.description")}</p>
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
              type="url"
              className="input input--mono"
              aria-label={t("importV2.url.label")}
              placeholder={t("importV2.url.placeholder")}
              value={url}
              onChange={(event) => setUrl(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void submitUrl();
                }
              }}
            />
            <button type="submit" className="btn btn--sm btn--primary" disabled={!url.trim() || unsupportedLocalUrl || submittingUrl || addingUrl}>
              {submittingUrl || addingUrl ? <LoaderCircle className="animate-spin" size={14} /> : <Plus size={14} />}
              {t("importV2.url.submit")}
            </button>
          </div>
          {unsupportedLocalUrl ? <p role="alert" className="m-0 text-[11px] text-[var(--danger)]">{t("importV2.url.localUnsupported")}</p> : null}
        </form>
        <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.url.hint")}</p>
        <p className="m-0 text-[11px] text-[var(--text-muted)]">{t("importV2.url.phaseTwo")}</p>
        <div className="flex flex-wrap gap-1" aria-label={t("importV2.url.platforms")}>
          {platforms.map((platform) => (
            <span
              key={platform.label}
              className={`import-v2-platform-chip ${platform.available ? "is-available" : "is-unavailable"}`}
              aria-label={`${platform.label}: ${t(platform.available ? "importV2.platform.available" : "importV2.platform.unavailable")}`}
            >
              {platform.label}
            </span>
          ))}
        </div>
      </article>
    </section>
  );
}
