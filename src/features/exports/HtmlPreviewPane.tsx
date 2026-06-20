import { useTranslation } from "react-i18next";
import { FileText } from "lucide-react";

interface HtmlPreviewPaneProps {
  html: string | null;
}

/**
 * Renders an exported HTML document inside a sandboxed iframe. `sandbox=""`
 * (no allow-scripts, no top-navigation) keeps the preview static: even if a
 * Skill or model emitted a `<script>`, it cannot execute or reach the app.
 * Uses `srcDoc` so no custom protocol or asset URL is required.
 */
export function HtmlPreviewPane({ html }: HtmlPreviewPaneProps) {
  const { t } = useTranslation();
  if (!html) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 text-[12px] text-[var(--text-muted)]">
        <FileText size={20} strokeWidth={1.5} />
        <span>{t("exports.preview.empty")}</span>
      </div>
    );
  }
  return (
    <iframe
      title="export-preview"
      srcDoc={html}
      sandbox=""
      className="h-full w-full border-0 bg-white"
    />
  );
}
