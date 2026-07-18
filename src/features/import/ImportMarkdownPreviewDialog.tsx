import { useEffect, useMemo, useState } from "react";
import { Copy, FileText, LoaderCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

import { useModalDialog } from "../../hooks/useModalDialog";
import type { ImportPreviewContent } from "../../types/importV2Presentation";

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

function safeExternalUrl(value: string | undefined): string | null {
  if (!value) return null;
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:" || url.protocol === "mailto:" ? url.toString() : null;
  } catch {
    return null;
  }
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
  const [error, setError] = useState<string | null>(null);
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
        setError(reason instanceof Error ? reason.message : String(reason));
        setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [identityKey, loadContent, open]);

  const title = content?.title ?? identity?.itemId ?? t("importV2.preview.title");
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
            <p className="m-0 font-mono text-[10.5px] text-[var(--text-muted)]">{identity.sessionId} / {identity.itemId}{identity.candidateId ? ` / ${identity.candidateId}` : ""}</p>
          </div>
          <button type="button" className="icon-button" aria-label={t("importV2.preview.close")} title={t("importV2.preview.close")} onClick={onClose}><X size={16} aria-hidden="true" /></button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          {loading ? <div role="status" className="flex items-center gap-2 text-[12px] text-[var(--text-muted)]"><LoaderCircle size={15} className="animate-spin" aria-hidden="true" />{t("importV2.preview.loading")}</div> : null}
          {error ? <div role="alert" className="rounded-[var(--radius-md)] border border-[var(--danger)] bg-[var(--danger-soft)] px-3 py-2 text-[12px] text-[var(--danger-text)]">{t("importV2.preview.error", { message: error })}</div> : null}
          {content ? (
            <>
              <div className="mb-3 flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-[var(--border-subtle)] pb-3 font-mono text-[10.5px] text-[var(--text-muted)]">
                <span>{content.totalBytes} {t("importV2.preview.bytes")}</span>
                <span title={content.sha256}>sha256:{content.sha256}</span>
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
                    img({ alt }) {
                      return <span className="text-[11px] text-[var(--text-muted)]">[{t("importV2.preview.imageOmitted")}{alt ? `: ${alt}` : ""}]</span>;
                    },
                  }}
                >
                  {rendered}
                </ReactMarkdown>
              </div>
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
