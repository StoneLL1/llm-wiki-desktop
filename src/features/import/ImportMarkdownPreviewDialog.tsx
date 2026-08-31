import { useEffect, useMemo, useState } from "react";
import { Copy, FileText, LoaderCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { useModalDialog } from "../../hooks/useModalDialog";
import { normalizeBackendError, type NormalizedBackendError } from "../../lib/backendError";
import type { ImportPreviewContent } from "../../types/importV2Presentation";
import { previewImageUrl, safeExternalUrl } from "./importPreviewUrl";

export interface ImportPreviewIdentity {
  sessionId: string;
  itemId: string;
  candidateId: string | null;
  historyBatchId?: string | null;
}

export interface ImportMarkdownPreviewDialogProps {
  open: boolean;
  identity: ImportPreviewIdentity | null;
  loadContent: (identity: ImportPreviewIdentity) => Promise<ImportPreviewContent>;
  onClose: () => void;
  onCopyMarkdown?: (markdown: string) => Promise<void> | void;
}

export function ImportMarkdownPreviewDialog({
  open,
  identity,
  loadContent,
  onClose,
  onCopyMarkdown,
}: ImportMarkdownPreviewDialogProps) {
  const { t } = useTranslation();
  const dialogRef = useModalDialog({ open, onClose });
  const [content, setContent] = useState<ImportPreviewContent | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<NormalizedBackendError | null>(null);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  const identityKey = identity ? `${identity.sessionId}\0${identity.itemId}\0${identity.candidateId ?? ""}\0${identity.historyBatchId ?? ""}` : null;

  useEffect(() => {
    if (!open || !identity || !identityKey) {
      setContent(null);
      setError(null);
      setLoading(false);
      return;
    }
    let current = true;
    setContent(null);
    setError(null);
    setCopyState("idle");
    setLoading(true);
    void loadContent(identity)
      .then((next) => {
        if (!current) return;
        setContent(next);
        setLoading(false);
      })
      .catch((reason: unknown) => {
        if (!current) return;
        setError(normalizeBackendError(reason, {
          defaultSummaryKey: "backendError.summary.import",
          defaultRecoverable: true,
          defaultActionKind: "retry",
        }));
        setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [identityKey, loadAttempt, loadContent, open]);

  const title = content?.title ?? t("importV2.preview.title");
  const markdown = content?.markdown ?? "";
  const rendered = useMemo(() => markdown, [markdown]);

  async function copyMarkdown() {
    if (!content) return;
    try {
      if (onCopyMarkdown) await onCopyMarkdown(content.markdown);
      else if (navigator.clipboard) await navigator.clipboard.writeText(content.markdown);
      else throw new Error("Clipboard unavailable");
      setCopyState("copied");
    } catch {
      setCopyState("failed");
    }
  }

  if (!open || !identity) return null;

  return (
    <div ref={dialogRef} tabIndex={-1} className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4" role="dialog" aria-modal="true" aria-labelledby="import-preview-title">
      <section className="flex max-h-[84vh] w-full max-w-[860px] flex-col rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center gap-3 border-b border-[var(--border)] px-4">
          <FileText size={17} className="shrink-0 text-[var(--accent)]" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <h2 id="import-preview-title" className="truncate text-[15px] font-semibold text-[var(--text-primary)]" title={title}>{title}</h2>
            <p className="m-0 text-[10.5px] text-[var(--text-muted)]">{t("importV2.preview.subtitle")}</p>
          </div>
          <button type="button" className="icon-button" aria-label={t("importV2.preview.close")} title={t("importV2.preview.close")} onClick={onClose}><X size={16} aria-hidden="true" /></button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {loading ? <div role="status" className="flex items-center gap-2 text-[12px] text-[var(--text-muted)]"><LoaderCircle size={15} className="animate-spin" aria-hidden="true" />{t("importV2.preview.loading")}</div> : null}
          {error ? (
            <ActionableErrorNotice error={error} onAction={() => setLoadAttempt((attempt) => attempt + 1)} />
          ) : null}
          {content ? (
            <>
              <div className="mb-3 flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-[var(--border-subtle)] pb-3 text-[10.5px] text-[var(--text-muted)]">
                <span>{content.totalBytes} {t("importV2.preview.bytes")}</span>
                {content.truncated ? <strong className="text-[var(--warning-text)]">{t("importV2.preview.truncated")}</strong> : null}
              </div>
              <div className="import-v2-markdown-preview prose prose-sm max-w-none text-[var(--text-primary)]">
                <ReactMarkdown
                  remarkPlugins={[remarkGfm, remarkMath]}
                  rehypePlugins={[rehypeKatex, rehypeHighlight]}
                  components={{
                    a({ href, children }) {
                      const safe = safeExternalUrl(href);
                      return safe ? <a href={safe} target="_blank" rel="noreferrer">{children}</a> : <span className="text-[var(--text-secondary)]">{children}</span>;
                    },
                    img({ src, alt }) {
                      const safe = previewImageUrl(src, content.resources ?? []);
                      return safe ? (
                        <img
                          src={safe}
                          alt={alt ?? ""}
                          className="my-3 max-h-[420px] max-w-full rounded-[var(--radius-md)] border border-[var(--border-subtle)] object-contain"
                        />
                      ) : (
                        <span className="text-[11px] text-[var(--text-muted)]">
                          [{t("importV2.preview.imageUnavailable")}{alt ? `: ${alt}` : ""}]
                        </span>
                      );
                    },
                  }}
                >
                  {rendered}
                </ReactMarkdown>
              </div>
              {content.comparison ? (
                <section className="mt-4" aria-labelledby="import-preview-comparison-title">
                  <h3 id="import-preview-comparison-title" className="mb-2 text-[12px] font-semibold text-[var(--text-primary)]">
                    {t("importV2.preview.comparison")}
                  </h3>
                  <div className="import-v2-preview-comparison">
                    <section>
                      <h4>{t("importV2.merge.current")}</h4>
                      <pre>{content.comparison.currentMarkdown}</pre>
                    </section>
                    <section>
                      <h4>{t("importV2.merge.imported")}</h4>
                      <pre>{content.markdown}</pre>
                    </section>
                    {content.comparison.mergedMarkdown ? (
                      <section>
                        <h4>{t("importV2.merge.merged")}</h4>
                        <pre>{content.comparison.mergedMarkdown}</pre>
                      </section>
                    ) : null}
                  </div>
                </section>
              ) : null}
              <div className="import-v2-preview-metadata">
                <section>
                  <h3>{t("importV2.preview.target")}</h3>
                  <p>{t(`importV2.preview.disposition.${content.target?.disposition ?? "new_source"}`)}</p>
                  {content.target?.wikiPath ? <p>{content.target.wikiPath}</p> : null}
                  {content.target?.sourceId ? (
                    <p>{t("importV2.preview.existingSourceVersion")}</p>
                  ) : null}
                </section>
                <section>
                  <h3>{t("importV2.preview.quality")}</h3>
                  <p>{t(`importV2.quality.${content.quality?.level ?? "pass"}`)}</p>
                  <p>{t("importV2.preview.qualitySummary", {
                    metrics: content.quality?.metrics.length ?? 0,
                    warnings: content.quality?.warnings.length ?? 0,
                  })}</p>
                </section>
                <section>
                  <h3>{t("importV2.preview.resources")}</h3>
                  {(content.resources ?? []).length > 0 ? (
                    <ul>
                      {(content.resources ?? []).map((resource) => (
                        <li key={`${resource.source}:${resource.kind}`}>
                          <span>{resource.name}</span>
                          <span>{t(`importV2.preview.resourceKind.${resource.kind}`)} · {resource.sizeBytes} B</span>
                        </li>
                      ))}
                    </ul>
                  ) : <p>{t("importV2.preview.noResources")}</p>}
                </section>
                <section>
                  <h3>{t("importV2.preview.rawSource")}</h3>
                  <p>{content.rawLabel ?? title}</p>
                </section>
              </div>
              <details className="import-v2-technical-details">
                <summary>{t("importV2.preview.technicalDetails")}</summary>
                <dl>
                  <dt>{t("importV2.preview.session")}</dt><dd>{identity.sessionId}</dd>
                  <dt>{t("importV2.preview.item")}</dt><dd>{identity.itemId}</dd>
                  {identity.candidateId ? <><dt>{t("importV2.preview.candidate")}</dt><dd>{identity.candidateId}</dd></> : null}
                  {content.target?.sourceId ? <><dt>{t("importV2.preview.source")}</dt><dd>{content.target.sourceId}</dd></> : null}
                  {content.target?.versionId ? <><dt>{t("importV2.preview.version")}</dt><dd>{content.target.versionId}</dd></> : null}
                  <dt>{t("importV2.preview.hash")}</dt><dd>{content.sha256}</dd>
                </dl>
              </details>
            </>
          ) : null}
        </div>

        <footer className="flex min-h-[52px] items-center justify-between gap-2 border-t border-[var(--border)] px-4">
          <div role="status" className="text-[11px] text-[var(--text-muted)]">{copyState === "copied" ? t("importV2.preview.copied") : copyState === "failed" ? t("importV2.preview.copyFailed") : ""}</div>
          <div className="flex items-center gap-2">
            <button type="button" className="rounded-[var(--radius-md)] border border-[var(--border)] px-2.5 py-1.5 text-[11.5px] text-[var(--text-primary)] disabled:opacity-50" onClick={() => void copyMarkdown()} disabled={!content || loading}>
              <span className="inline-flex items-center gap-1.5"><Copy size={13} aria-hidden="true" />{t("importV2.preview.copy")}</span>
            </button>
            <button type="button" className="rounded-[var(--radius-md)] bg-[var(--text-primary)] px-2.5 py-1.5 text-[11.5px] text-[var(--surface)]" onClick={onClose}>{t("importV2.preview.close")}</button>
          </div>
        </footer>
      </section>
    </div>
  );
}
