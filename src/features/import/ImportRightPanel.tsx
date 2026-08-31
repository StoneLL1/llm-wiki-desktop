import { Eye, FileCode2, Globe2, LoaderCircle, ShieldCheck } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

import { RightPanelHeader } from "../../components/app/RightPanelHeader";
import { ActionableErrorNotice } from "../../components/app/ActionableErrorNotice";
import { normalizeBackendError, type NormalizedBackendError } from "../../lib/backendError";
import { compactPath } from "../../lib/pathDisplay";
import { importV2Api } from "../../services/importV2Api";
import type { ImportItem } from "../../types/importV2";
import type { ImportPreviewContent } from "../../types/importV2Presentation";
import { presentImportItem, type ImportItemAction } from "./importStatusPresentation";
import { ImportItemStatus } from "./ImportItemStatus";
import { previewImageUrl } from "./importPreviewUrl";
import { displayHostForImportLocator } from "./importLocator";
import { ImportQualitySummary } from "./ImportQualitySummary";
import { ImportAttemptTimeline } from "./ImportAttemptTimeline";

export interface ImportRightPanelProps {
  selectedItem?: ImportItem | null;
  sessionId?: string | null;
  projectId?: string;
  projectRootPath?: string;
  onPreviewMarkdown: (itemId: string) => void;
  onPrimaryAction?: (action: ImportItemAction, itemId: string) => void;
}

function InspectorHeading({ children }: { children: ReactNode }) {
  return <h3 className="import-v2-inspector-heading">{children}</h3>;
}

export function ImportRightPanel({
  selectedItem = null,
  sessionId = null,
  projectId = "",
  projectRootPath = "",
  onPreviewMarkdown,
  onPrimaryAction,
}: ImportRightPanelProps) {
  const { t } = useTranslation();
  const [content, setContent] = useState<ImportPreviewContent | null>(null);
  const [previewState, setPreviewState] = useState<"idle" | "loading" | "error">("idle");
  const [previewError, setPreviewError] = useState<NormalizedBackendError | null>(null);
  const [previewAttempt, setPreviewAttempt] = useState(0);

  useEffect(() => {
    if (!selectedItem?.preview || !sessionId || !projectId || !projectRootPath) {
      setContent(null);
      setPreviewState("idle");
      setPreviewError(null);
      return;
    }
    let current = true;
    setContent(null);
    setPreviewState("loading");
    setPreviewError(null);
    void importV2Api.getPreviewContent({
      projectId,
      projectRootPath,
      sessionId,
      itemId: selectedItem.itemId,
      candidateId: null,
    }).then((next) => {
      if (!current) return;
      setContent(next);
      setPreviewState("idle");
    }).catch((error) => {
      if (!current) return;
      setPreviewError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.import",
        defaultRecoverable: true,
        defaultActionKind: "retry",
      }));
      setPreviewState("error");
    });
    return () => {
      current = false;
    };
  }, [previewAttempt, projectId, projectRootPath, selectedItem?.itemId, selectedItem?.preview, sessionId]);

  return (
    <aside id="right-context-panel" aria-label={t("importV2.inspector.title")} className="right-panel">
      <RightPanelHeader title={t("importV2.inspector.title")} />
      <div className="app-pane-scrollbar min-h-0 flex-1 overflow-y-auto">
        {!selectedItem ? (
          <div className="import-v2-inspector-empty" role="status">
            <span className="import-v2-inspector-empty__icon" aria-hidden="true"><Eye size={17} /></span>
            <strong>{t("importV2.inspector.emptyTitle")}</strong>
            <p>{t("importV2.inspector.empty")}</p>
          </div>
        ) : (
          <SelectedSource
            item={selectedItem}
            content={content}
            previewState={previewState}
            previewError={previewError}
            onRetryPreview={() => setPreviewAttempt((attempt) => attempt + 1)}
            onPreviewMarkdown={onPreviewMarkdown}
            onPrimaryAction={onPrimaryAction}
          />
        )}
      </div>
    </aside>
  );
}

function SelectedSource({
  item,
  content,
  previewState,
  previewError,
  onRetryPreview,
  onPreviewMarkdown,
  onPrimaryAction,
}: {
  item: ImportItem;
  content: ImportPreviewContent | null;
  previewState: "idle" | "loading" | "error";
  previewError: NormalizedBackendError | null;
  onRetryPreview: () => void;
  onPreviewMarkdown: (itemId: string) => void;
  onPrimaryAction?: (action: ImportItemAction, itemId: string) => void;
}) {
  const { t } = useTranslation();
  const presentation = presentImportItem(item);
  const SourceIcon = item.input.kind === "url" ? Globe2 : FileCode2;
  const primaryAction = presentation.primaryAction;

  return (
    <>
      <section className="border-b border-[var(--border-subtle)] px-4 py-3" aria-labelledby="import-source-title">
        <div className="mb-2 flex items-start gap-2">
          <SourceIcon size={16} className="mt-0.5 shrink-0 text-[var(--accent)]" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <h3 id="import-source-title" className="truncate text-[13px] font-semibold text-[var(--text-primary)]" title={item.input.displayName}>
              {item.input.displayName}
            </h3>
            <div className="truncate text-[11px] text-[var(--text-muted)]">
              {item.input.kind === "url"
                ? displayHostForImportLocator(item.input.normalizedLocator ?? item.input.locator)
                : compactPath(item.input.locator)}
            </div>
          </div>
        </div>
        <ImportItemStatus item={item} presentation={presentation} />
        {presentation.userIssue ? (
          <div role="status" className="mt-3 rounded-[var(--radius-md)] border border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-2 text-[11.5px]">
            <strong>{t(presentation.userIssue.title)}</strong>
            <p className="m-0 mt-1 text-[var(--text-secondary)]">
              {t(presentation.userIssue.dataSafety)}
            </p>
          </div>
        ) : null}
        {primaryAction && onPrimaryAction ? (
          <button
            type="button"
            className="btn btn--sm btn--primary mt-3 w-full justify-center"
            onClick={() => onPrimaryAction(primaryAction, item.itemId)}
          >
            {t(`importV2.action.${primaryAction}`)}
          </button>
        ) : null}
      </section>

      <section className="border-b border-[var(--border-subtle)] px-4 py-3">
        <InspectorHeading>{t("importV2.inspector.quickPreview")}</InspectorHeading>
        {previewState === "loading" ? (
          <p role="status" className="flex items-center gap-2 text-[11px] text-[var(--text-muted)]">
            <LoaderCircle size={13} className="animate-spin" aria-hidden="true" />
            {t("importV2.preview.loading")}
          </p>
        ) : previewState === "error" && previewError ? (
          <ActionableErrorNotice error={previewError} role="status" onAction={() => onRetryPreview()} />
        ) : content ? (
          <>
            <div className="import-v2-quick-preview">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={{
                  img({ src, alt }) {
                    const safe = previewImageUrl(src, content.resources ?? []);
                    return safe
                      ? <img src={safe} alt={alt ?? ""} />
                      : <span>[{t("importV2.preview.imageUnavailable")}]</span>;
                  },
                }}
              >
                {content.markdown}
              </ReactMarkdown>
            </div>
            {primaryAction !== "preview_markdown" ? (
              <button
                type="button"
                className="btn btn--sm mt-2"
                onClick={() => onPreviewMarkdown(item.itemId)}
              >
                <Eye size={13} aria-hidden="true" />
                {t("importV2.inspector.previewMarkdown")}
              </button>
            ) : null}
          </>
        ) : (
          <p className="m-0 text-[11px] text-[var(--text-muted)]">
            {t(previewState === "error" ? "importV2.inspector.previewUnavailable" : "importV2.inspector.previewPending")}
          </p>
        )}
      </section>

      <section className="border-b border-[var(--border-subtle)] px-4 py-3">
        <InspectorHeading>{t("importV2.inspector.target")}</InspectorHeading>
        <p className="m-0 text-[11.5px] text-[var(--text-primary)]">
          {t(`importV2.preview.disposition.${content?.target?.disposition ?? "new_source"}`)}
        </p>
        {content?.target?.wikiPath ? (
          <p className="m-0 mt-1 break-all text-[10.5px] text-[var(--text-muted)]">
            {content.target.wikiPath}
          </p>
        ) : null}
        {content?.target?.sourceId ? (
          <p className="m-0 mt-1 text-[10.5px] text-[var(--text-muted)]">
            {t("importV2.preview.existingSourceVersion")}
          </p>
        ) : null}
      </section>

      <ImportQualitySummary quality={item.preview?.quality ?? null} />

      <section className="border-b border-[var(--border-subtle)] px-4 py-3">
        <InspectorHeading>{t("importV2.inspector.rawSource")}</InspectorHeading>
        <p className="m-0 flex items-center gap-2 text-[11px] text-[var(--text-secondary)]">
          <ShieldCheck size={13} aria-hidden="true" />
          {t("importV2.inspector.rawSafe")}
        </p>
        <p className="m-0 mt-1 break-all text-[10.5px] text-[var(--text-muted)]">
          {content?.rawLabel ?? item.input.displayName}
        </p>
      </section>

      <details className="import-v2-inspector-technical">
        <summary>{t("importV2.inspector.technicalDetails")}</summary>
        <dl>
          <dt>{t("importV2.inspector.itemId")}</dt><dd>{item.itemId}</dd>
          {content?.target?.sourceId ? <><dt>{t("importV2.preview.source")}</dt><dd>{content.target.sourceId}</dd></> : null}
          {content?.target?.versionId ? <><dt>{t("importV2.preview.version")}</dt><dd>{content.target.versionId}</dd></> : null}
          {item.preview ? (
            <>
              <dt>{t("importV2.inspector.output")}</dt><dd>{item.preview.markdown.relativePath}</dd>
              <dt>{t("importV2.preview.hash")}</dt><dd>{item.preview.markdown.sha256}</dd>
            </>
          ) : null}
          {presentation.userIssue?.detail?.technicalCode ? (
            <>
              <dt>{t("importV2.inspector.errorCode")}</dt>
              <dd>{presentation.userIssue.detail.technicalCode}</dd>
            </>
          ) : null}
          {presentation.userIssue?.detail?.technicalMessage ? (
            <>
              <dt>{t("importV2.inspector.technicalMessage")}</dt>
              <dd>{presentation.userIssue.detail.technicalMessage}</dd>
            </>
          ) : null}
        </dl>
        <ImportAttemptTimeline attempts={item.attempts} />
      </details>
    </>
  );
}
