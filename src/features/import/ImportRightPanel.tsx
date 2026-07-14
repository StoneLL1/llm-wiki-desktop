import { Eye, FileCode2, Globe2, Link2, Package, ShieldAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ReactNode } from "react";

import { RightPanelHeader } from "../../components/app/RightPanelHeader";
import { compactPath } from "../../lib/pathDisplay";
import type { ImportItem } from "../../types/importV2";
import { presentImportItem } from "./importStatusPresentation";
import { ImportAttemptTimeline } from "./ImportAttemptTimeline";
import { ImportItemStatus } from "./ImportItemStatus";
import { ImportQualitySummary } from "./ImportQualitySummary";

export interface ImportRightPanelProps {
  selectedItem?: ImportItem | null;
  onPreviewMarkdown: (itemId: string) => void;
}

function urlHost(locator: string): string {
  try {
    return new URL(locator).host || locator;
  } catch {
    return locator;
  }
}

function routeFor(item: ImportItem): string {
  if (item.input.kind !== "url") return item.attempts.at(-1)?.route ?? "local_file";
  const host = urlHost(item.input.normalizedLocator ?? item.input.locator).toLowerCase();
  if (host.includes("bilibili")) return "bilibili";
  if (host.includes("wechat") || host.includes("weixin")) return "wechat";
  if (host.includes("zhihu")) return "zhihu";
  return "generic_http";
}

function InspectorHeading({ children }: { children: ReactNode }) {
  return <h3 className="import-v2-inspector-heading">{children}</h3>;
}

export function ImportRightPanel({ selectedItem = null, onPreviewMarkdown }: ImportRightPanelProps) {
  const { t } = useTranslation();
  return (
    <aside id="right-context-panel" aria-label={t("importV2.inspector.title")} className="right-panel">
      <RightPanelHeader title={t("importV2.inspector.title")} />
      <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
        {!selectedItem ? (
          <p className="px-4 py-3 text-[12px] text-[var(--text-muted)]">{t("importV2.inspector.empty")}</p>
        ) : (
          <SelectedSource item={selectedItem} onPreviewMarkdown={onPreviewMarkdown} />
        )}
      </div>
    </aside>
  );
}

function SelectedSource({ item, onPreviewMarkdown }: ImportRightPanelProps & { item: ImportItem }) {
  const { t } = useTranslation();
  const presentation = presentImportItem(item);
  const route = routeFor(item);
  const preview = item.preview;
  const sourceIcon = item.input.kind === "url" ? Globe2 : FileCode2;
  const SourceIcon = sourceIcon;
  const latestAttempt = item.attempts.at(-1);

  return (
    <>
      <section className="border-b border-[var(--border-subtle)] px-4 py-3" aria-labelledby="import-source-title">
        <div className="mb-2 flex items-start gap-2">
          <SourceIcon size={16} className="mt-0.5 shrink-0 text-[var(--accent)]" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <h3 id="import-source-title" className="truncate text-[13px] font-semibold text-[var(--text-primary)]" title={item.input.displayName}>
              {item.input.displayName}
            </h3>
            <div className="truncate font-mono text-[11px] text-[var(--text-muted)]" title={item.input.locator}>
              {item.input.kind === "url" ? urlHost(item.input.normalizedLocator ?? item.input.locator) : compactPath(item.input.locator)}
            </div>
          </div>
        </div>
        <div className="mb-3"><ImportItemStatus item={item} presentation={presentation} /></div>
        <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-[11.5px]">
          <dt className="text-[var(--text-muted)]">{t("importV2.inspector.kind")}</dt>
          <dd className="m-0 text-[var(--text-primary)]">{item.input.kind === "url" ? route : t("importV2.inspector.localSource")}</dd>
          <dt className="text-[var(--text-muted)]">{t("importV2.inspector.route")}</dt>
          <dd className="m-0 font-mono text-[var(--text-primary)]">{route}</dd>
          {latestAttempt ? (
            <>
              <dt className="text-[var(--text-muted)]">{t("importV2.inspector.engine")}</dt>
              <dd className="m-0 font-mono text-[var(--text-primary)]">{latestAttempt.engineId} {latestAttempt.engineVersion}</dd>
            </>
          ) : null}
        </dl>
        {item.issue ? (
          <div role="alert" className="mt-3 flex gap-2 rounded-[var(--radius-md)] border border-[var(--danger)] bg-[var(--danger-soft)] px-3 py-2 text-[11.5px] text-[var(--danger-text)]">
            <ShieldAlert size={14} className="mt-0.5 shrink-0" aria-hidden="true" />
            <div><strong>{item.issue.code}</strong><div>{item.issue.message}</div></div>
          </div>
        ) : null}
        {preview && presentation.actions.includes("preview_markdown") ? (
          <button type="button" className="mt-3 inline-flex items-center gap-1.5 rounded-[var(--radius-md)] border border-[var(--border)] px-2.5 py-1.5 text-[11.5px] text-[var(--text-primary)] hover:bg-[var(--surface-muted)]" onClick={() => onPreviewMarkdown(item.itemId)}>
            <Eye size={14} aria-hidden="true" />
            {t("importV2.inspector.previewMarkdown")}
          </button>
        ) : null}
      </section>

      <section className="border-b border-[var(--border-subtle)] px-4 py-3" aria-labelledby="import-provenance-title">
        <InspectorHeading><span id="import-provenance-title">{t("importV2.inspector.provenance")}</span></InspectorHeading>
        {preview ? (
          <dl className="m-0 space-y-1.5 text-[11.5px]">
            <ArtifactRow label={t("importV2.inspector.output")} path={preview.markdown.relativePath} sha256={preview.markdown.sha256} sizeBytes={preview.markdown.sizeBytes} icon={Package} />
            <ArtifactRow label={t("importV2.inspector.sourceSnapshot")} path={preview.sourceSnapshot.relativePath} sha256={preview.sourceSnapshot.sha256} sizeBytes={preview.sourceSnapshot.sizeBytes} icon={Link2} />
            <div className="flex items-center justify-between gap-3 text-[var(--text-secondary)]"><dt>{t("importV2.inspector.assets")}</dt><dd className="m-0">{t("importV2.inspector.assetCount", { count: preview.assets.length })}</dd></div>
          </dl>
        ) : <p className="m-0 text-[12px] text-[var(--text-muted)]">{t("importV2.inspector.provenanceUnavailable")}</p>}
      </section>

      <ImportQualitySummary quality={preview?.quality ?? null} />
      <ImportAttemptTimeline attempts={item.attempts} />
    </>
  );
}

function ArtifactRow({ label, path, sha256, sizeBytes, icon: Icon }: { label: string; path: string; sha256: string; sizeBytes: number; icon: typeof Package }) {
  return (
    <div className="min-w-0">
      <div className="flex items-center gap-1.5 text-[var(--text-muted)]"><Icon size={12} aria-hidden="true" /><span>{label}</span><span className="ml-auto font-mono text-[10.5px]">{sizeBytes} B</span></div>
      <div className="truncate font-mono text-[10.5px] text-[var(--text-primary)]" title={path}>{path}</div>
      <div className="truncate font-mono text-[10px] text-[var(--text-muted)]" title={sha256}>sha256:{sha256}</div>
    </div>
  );
}
