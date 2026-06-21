import { useTranslation } from "react-i18next";
import { ArrowLeft, Copy, FolderOpen, LoaderCircle, RefreshCw } from "lucide-react";

interface HtmlPreviewPaneProps {
  html: string | null;
  outputPath: string | null;
  templateLabel: string;
  busy: boolean;
  onBack: () => void;
  onRegenerate: () => void;
  onOpenFolder: () => void;
  onCopyPath: () => void;
}

export function HtmlPreviewPane({
  html,
  outputPath,
  templateLabel,
  busy,
  onBack,
  onRegenerate,
  onOpenFolder,
  onCopyPath,
}: HtmlPreviewPaneProps) {
  const { t } = useTranslation();

  return (
    <div className="html-preview">
      <div className="html-preview__bar">
        <button type="button" onClick={onBack} className="html-preview__button">
          <ArrowLeft size={13} /> {t("wiki.html.backToRead")}
        </button>
        <span className="html-preview__separator" />
        <strong className="text-[12px] text-[var(--text-primary)]">{templateLabel}</strong>
        <span className="font-mono text-[10.5px] text-[var(--text-muted)]">{outputPath ?? t("wiki.html.waiting")}</span>
        <div className="ml-auto flex items-center gap-1.5">
          <button type="button" onClick={onRegenerate} disabled={busy} className="html-preview__button">
            {busy ? <LoaderCircle size={13} className="animate-spin" /> : <RefreshCw size={13} />} {t("wiki.html.regenerate")}
          </button>
          <button type="button" onClick={onOpenFolder} disabled={!outputPath} className="html-preview__button">
            <FolderOpen size={13} /> {t("wiki.html.openFolder")}
          </button>
          <button type="button" onClick={onCopyPath} disabled={!outputPath} className="html-preview__icon-button" aria-label={t("wiki.html.copyPath")} title={t("wiki.html.copyPath")}>
            <Copy size={13} />
          </button>
        </div>
      </div>
      <div className="html-preview__frame-wrap">
        <div className="html-preview__chrome">
          <span className="html-preview__dot html-preview__dot--danger" />
          <span className="html-preview__dot html-preview__dot--warning" />
          <span className="html-preview__dot html-preview__dot--success" />
          <span className="ml-2 truncate">{outputPath ?? t("wiki.html.waiting")}</span>
        </div>
        {html ? (
          <iframe title={t("wiki.html.previewTitle")} srcDoc={html} sandbox="" className="html-preview__iframe" />
        ) : (
          <div className="flex flex-1 items-center justify-center gap-2 text-[12px] text-[var(--text-muted)]">
            {busy ? <LoaderCircle size={16} className="animate-spin" /> : null}
            {t("wiki.html.previewEmpty")}
          </div>
        )}
      </div>
    </div>
  );
}
