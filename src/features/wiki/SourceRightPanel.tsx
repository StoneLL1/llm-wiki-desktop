import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  ChevronRight,
  Clipboard,
  FileDiff,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  ScanText,
  Sparkles,
  Subtitles,
  Trash2,
} from "lucide-react";

import type {
  SourceCandidateKind,
  SourcePrimaryAction,
} from "../../types/source";
import { useSourceStore } from "./sourceStore";
import { useWikiStore } from "./wikiStore";

type SourceReprocessKind = Exclude<SourceCandidateKind, "ai_organize">;

interface SourceRightPanelProps {
  projectId: string;
  rootPath: string;
  sourceId: string;
  onOpenPage: (path: string) => void;
  onMutation: (path?: string) => void;
}

export function SourceRightPanel({
  projectId,
  rootPath,
  sourceId,
  onOpenPage,
  onMutation,
}: SourceRightPanelProps) {
  const { t } = useTranslation();
  const detail = useSourceStore((state) => state.detail);
  const updatePreview = useSourceStore((state) => state.updatePreview);
  const loading = useSourceStore((state) => state.loading);
  const mutating = useSourceStore((state) => state.mutating);
  const error = useSourceStore(
    (state) => state.errorsBySourceId[sourceId] ?? null,
  );
  const loadDetail = useSourceStore((state) => state.loadDetail);
  const reprocess = useSourceStore((state) => state.reprocess);
  const previewCandidate = useSourceStore((state) => state.previewCandidate);
  const applyCandidate = useSourceStore((state) => state.applyCandidate);
  const discardCandidate = useSourceStore((state) => state.discardCandidate);
  const restoreVersion = useSourceStore((state) => state.restoreVersion);
  const previewDelete = useSourceStore((state) => state.previewDelete);
  const wikiPage = useWikiStore((state) => state.page);
  const wikiMode = useWikiStore((state) => state.mode);
  const wikiDraft = useWikiStore((state) => state.draft);
  const [mergeDraft, setMergeDraft] = useState("");
  const [technicalOpen, setTechnicalOpen] = useState(false);
  const subtitlePaths = useMemo(
    () =>
      detail?.evidence
        .filter((artifact) => artifact.kind.toLowerCase().includes("subtitle"))
        .map((artifact) => artifact.path) ?? [],
    [detail?.evidence],
  );
  const [selectedSubtitlePath, setSelectedSubtitlePath] = useState("");

  useEffect(() => {
    void loadDetail(projectId, rootPath, sourceId);
  }, [loadDetail, projectId, rootPath, sourceId]);

  useEffect(() => {
    if (updatePreview?.mode === "three_way") {
      setMergeDraft(updatePreview.currentMarkdown);
    } else {
      setMergeDraft("");
    }
  }, [updatePreview?.candidateId, updatePreview?.currentMarkdown, updatePreview?.mode]);

  useEffect(() => {
    setSelectedSubtitlePath(subtitlePaths[0] ?? "");
  }, [detail?.sourceId, detail?.versionId, subtitlePaths]);

  const action = useMemo(() => {
    if (!detail) return null;
    if (detail.candidate) {
      return {
        label: t("source.action.reviewCandidate"),
        icon: <FileDiff size={13} />,
        run: () =>
          void previewCandidate(projectId, rootPath, detail.candidate!.candidateId),
      };
    }
    const kind = primaryCandidateKind(detail.primaryAction);
    if (!kind) return null;
    return {
      label: actionLabel(kind, t),
      icon: actionIcon(kind),
      run: () => void reprocess(projectId, rootPath, kind),
    };
  }, [detail, previewCandidate, projectId, reprocess, rootPath, t]);
  const hasUnsavedEdits =
    wikiMode === "edit" &&
    wikiPage?.meta.sourceBinding?.sourceId === sourceId &&
    wikiDraft !== wikiPage.rawMarkdown;

  if (loading && detail?.sourceId !== sourceId) {
    return (
      <div className="flex min-h-[120px] items-center justify-center text-[var(--text-muted)]">
        <LoaderCircle size={16} className="animate-spin" />
      </div>
    );
  }

  if (!detail || detail.sourceId !== sourceId) {
    return (
      <div className="px-4 py-4 text-[12px] leading-5 text-[var(--text-muted)]">
        {error ?? t("source.detail.unavailable")}
      </div>
    );
  }

  const apply = () => {
    const draftAtStart = useWikiStore.getState().draft;
    const merged =
      updatePreview?.mode === "three_way" ? mergeDraft : undefined;
    void applyCandidate(projectId, rootPath, merged).then((result) => {
      if (!result) return;
      const wikiAfterApply = useWikiStore.getState();
      const draftChangedDuringApply =
        wikiAfterApply.mode === "edit" &&
        wikiAfterApply.page?.meta.sourceBinding?.sourceId === sourceId &&
        wikiAfterApply.draft !== draftAtStart;
      if (draftChangedDuringApply) {
        const message = t("source.candidate.draftChangedDuringApply");
        useSourceStore.setState({
          error: message,
          errorSourceId: sourceId,
          errorsBySourceId: {
            ...useSourceStore.getState().errorsBySourceId,
            [sourceId]: message,
          },
        });
        return;
      }
      onMutation(result.wikiPath);
    });
  };

  return (
    <div className="divide-y divide-[var(--border-subtle)]">
      <SourceSection order={1} title={t("source.section.status")}>
        <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-[12px]">
          <dt className="text-[var(--text-muted)]">{t("source.field.kind")}</dt>
          <dd className="m-0 truncate text-[var(--text-primary)]">{detail.sourceKind}</dd>
          <dt className="text-[var(--text-muted)]">{t("source.field.status")}</dt>
          <dd className="m-0 text-[var(--text-primary)]">{t(`source.status.${detail.status}`)}</dd>
          <dt className="text-[var(--text-muted)]">{t("source.field.version")}</dt>
          <dd className="m-0 truncate font-mono text-[11px] text-[var(--text-secondary)]">
            {detail.versionId}
          </dd>
        </dl>
      </SourceSection>

      <SourceSection order={2} title={t("source.section.primaryAction")}>
        {action ? (
          <button
            type="button"
            disabled={mutating}
            onClick={action.run}
            className="inline-flex h-[28px] w-full items-center justify-center gap-1.5 rounded-[var(--radius-md)] bg-[var(--foreground)] px-3 text-[12px] font-medium text-[var(--text-inverse)] disabled:opacity-40"
          >
            {mutating ? <LoaderCircle size={13} className="animate-spin" /> : action.icon}
            {action.label}
          </button>
        ) : (
          <p className="m-0 text-[12px] text-[var(--text-muted)]">
            {t("source.action.none")}
          </p>
        )}
      </SourceSection>

      <SourceSection order={3} title={t("source.section.candidate")}>
        {!detail.candidate ? (
          <p className="m-0 text-[12px] text-[var(--text-muted)]">
            {t("source.candidate.empty")}
          </p>
        ) : updatePreview ? (
          <div className="space-y-2">
            {updatePreview.mode === "three_way" ? (
              <>
                <div className="flex items-start gap-2 rounded-[var(--radius-md)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11.5px] leading-4">
                  <AlertTriangle size={13} className="mt-0.5 shrink-0 text-[var(--warning)]" />
                  <span>{t("source.candidate.threeWay")}</span>
                </div>
                <div className="space-y-2">
                  {[
                    [t("source.candidate.base"), updatePreview.baseMarkdown],
                    [t("source.candidate.current"), updatePreview.currentMarkdown],
                    [t("source.candidate.proposed"), updatePreview.candidateMarkdown],
                  ].map(([label, markdown]) => (
                    <div key={label}>
                      <div className="mb-1 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">
                        {label}
                      </div>
                      <pre
                        tabIndex={0}
                        className="max-h-[120px] overflow-auto whitespace-pre-wrap rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-muted)] p-2 font-mono text-[10.5px] leading-4 text-[var(--text-secondary)]"
                      >
                        {markdown}
                      </pre>
                    </div>
                  ))}
                </div>
              </>
            ) : null}
            <pre
              tabIndex={0}
              aria-label={t("source.candidate.diff")}
              className="max-h-[180px] overflow-auto whitespace-pre-wrap rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--surface-muted)] p-2 font-mono text-[10.5px] leading-4 text-[var(--text-secondary)]"
            >
              {updatePreview.diff}
            </pre>
            {updatePreview.mode === "three_way" ? (
              <label className="block">
                <span className="mb-1 block text-[10.5px] text-[var(--text-muted)]">
                  {t("source.candidate.mergeHint")}
                </span>
                <textarea
                  aria-label={t("source.candidate.mergeDraft")}
                  value={mergeDraft}
                  onChange={(event) => setMergeDraft(event.target.value)}
                  className="min-h-[160px] w-full resize-y rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] p-2 font-mono text-[11px] outline-none focus:border-[var(--accent)]"
                />
              </label>
            ) : null}
            {hasUnsavedEdits ? (
              <div
                role="alert"
                className="flex items-start gap-2 rounded-[var(--radius-md)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11.5px] leading-4"
              >
                <AlertTriangle size={13} className="mt-0.5 shrink-0 text-[var(--warning)]" />
                <span>{t("source.candidate.unsaved")}</span>
              </div>
            ) : null}
            <div className="flex flex-wrap gap-2">
              <button
                type="button"
                disabled={
                  mutating ||
                  hasUnsavedEdits ||
                  (updatePreview.mode === "three_way" && !mergeDraft.trim())
                }
                onClick={apply}
                className="h-[28px] rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px] font-medium hover:bg-[var(--surface-muted)] disabled:opacity-40"
              >
                {t("source.candidate.apply")}
              </button>
              <button
                type="button"
                disabled={mutating}
                onClick={() => void discardCandidate(projectId, rootPath)}
                className="inline-flex h-[28px] items-center gap-1.5 rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px] hover:bg-[var(--surface-muted)] disabled:opacity-40"
              >
                <Trash2 size={13} />
                {t("source.candidate.discard")}
              </button>
            </div>
          </div>
        ) : (
          <button
            type="button"
            onClick={() =>
              void previewCandidate(projectId, rootPath, detail.candidate!.candidateId)
            }
            className="inline-flex h-[28px] items-center gap-1.5 rounded-[var(--radius-md)] border border-[var(--border)] px-3 text-[12px] hover:bg-[var(--surface-muted)]"
          >
            <FileDiff size={13} />
            {t("source.action.reviewCandidate")}
          </button>
        )}
      </SourceSection>

      <SourceSection order={4} title={t("source.section.paths")}>
        <p className="m-0 break-all font-mono text-[11px] leading-4 text-[var(--text-primary)]">
          {detail.targetPath}
        </p>
        <p className="mb-0 mt-2 text-[11.5px] leading-5 text-[var(--text-muted)]">
          {t(`source.evidence.${detail.evidenceRetention}`)}
        </p>
        <ul className="mt-2 space-y-1">
          {detail.evidence.slice(0, 6).map((artifact) => (
            <li key={`${artifact.kind}-${artifact.path}`} className="truncate font-mono text-[10.5px] text-[var(--text-secondary)]">
              {artifact.path}
            </li>
          ))}
        </ul>
      </SourceSection>

      <SourceSection order={5} title={t("source.section.quality")}>
        <p className="m-0 text-[12px] font-medium text-[var(--text-primary)]">
          {t(`source.quality.${detail.quality.level}`)}
        </p>
        {detail.quality.warnings.length ? (
          <ul className="mt-2 space-y-1.5 text-[11.5px] leading-4 text-[var(--warning)]">
            {detail.quality.warnings.map((warning) => <li key={warning}>{warning}</li>)}
          </ul>
        ) : (
          <p className="mb-0 mt-1 text-[11.5px] text-[var(--text-muted)]">
            {t("source.quality.noIssues")}
          </p>
        )}
      </SourceSection>

      <SourceSection order={6} title={t("source.section.original")}>
        <pre
          tabIndex={0}
          className="max-h-[220px] overflow-auto whitespace-pre-wrap rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-2 font-mono text-[10.5px] leading-4 text-[var(--text-secondary)]"
        >
          {detail.originalDraft}
        </pre>
        {detail.originalDraftTruncated ? (
          <p className="mb-0 mt-1 text-[10.5px] text-[var(--text-muted)]">
            {t("source.original.truncated")}
          </p>
        ) : null}
      </SourceSection>

      <SourceSection order={7} title={t("source.section.timeline")}>
        <ol className="space-y-2">
          {detail.timeline.map((event) => (
            <li key={event.eventId} className="border-l border-[var(--border)] pl-2.5">
              <div className="text-[11.5px] font-medium text-[var(--text-primary)]">
                {t(`source.timeline.${event.kind}`)}
              </div>
              <div className="mt-0.5 text-[10.5px] text-[var(--text-muted)]">
                {new Date(event.createdAt).toLocaleString()}
              </div>
              {event.restorable && event.versionId && event.versionId !== detail.versionId ? (
                <button
                  type="button"
                  disabled={mutating}
                  onClick={() =>
                    void restoreVersion(projectId, rootPath, event.versionId!).then((result) => {
                      if (result) onMutation(result.wikiPath);
                    })
                  }
                  className="mt-1 inline-flex h-[24px] items-center gap-1 rounded-[var(--radius-sm)] text-[11px] text-[var(--accent-hover)] disabled:opacity-40"
                >
                  <RotateCcw size={11} />
                  {t("source.timeline.restore")}
                </button>
              ) : null}
            </li>
          ))}
        </ol>
      </SourceSection>

      <SourceSection order={8} title={t("source.section.technical")}>
        <button
          type="button"
          aria-expanded={technicalOpen}
          onClick={() => setTechnicalOpen((value) => !value)}
          className="flex h-[28px] w-full items-center gap-1.5 rounded-[var(--radius-sm)] text-left text-[12px] hover:bg-[var(--surface-muted)]"
        >
          <ChevronRight
            size={13}
            className={`transition-transform ${technicalOpen ? "rotate-90" : ""}`}
          />
          {t("source.technical.toggle")}
        </button>
        {technicalOpen ? (
          <div className="mt-2 space-y-2">
            <pre className="overflow-auto whitespace-pre-wrap rounded-[var(--radius-md)] bg-[var(--surface-muted)] p-2 font-mono text-[10.5px] leading-4">
              {JSON.stringify(detail.technicalDetails, null, 2)}
            </pre>
            <button
              type="button"
              onClick={() =>
                void navigator.clipboard?.writeText(
                  JSON.stringify(detail.technicalDetails, null, 2),
                )
              }
              className="inline-flex h-[26px] items-center gap-1.5 rounded-[var(--radius-sm)] border border-[var(--border)] px-2 text-[11px]"
            >
              <Clipboard size={11} />
              {t("source.technical.copy")}
            </button>
            <div className="flex flex-wrap gap-1.5 pt-1">
              {detail.availableActions.includes("subtitle") && subtitlePaths.length ? (
                <label className="w-full text-[10.5px] text-[var(--text-muted)]">
                  <span className="mb-1 block">{t("source.subtitle.select")}</span>
                  <select
                    value={selectedSubtitlePath}
                    onChange={(event) => setSelectedSubtitlePath(event.target.value)}
                    className="h-[28px] w-full rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--background)] px-2 font-mono text-[10.5px]"
                  >
                    {subtitlePaths.map((path) => (
                      <option key={path} value={path}>{path}</option>
                    ))}
                  </select>
                </label>
              ) : null}
              {detail.availableActions
                .filter(isReprocessKind)
                .filter((kind) => kind !== primaryCandidateKind(detail.primaryAction))
                .map((kind) => (
                <button
                  key={kind}
                  type="button"
                  disabled={mutating || (kind === "subtitle" && !selectedSubtitlePath)}
                  onClick={() =>
                    void reprocess(
                      projectId,
                      rootPath,
                      kind,
                      kind === "subtitle" ? selectedSubtitlePath : undefined,
                    )
                  }
                  className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--border)] px-2 text-[11px] disabled:opacity-40"
                >
                  {actionLabel(kind, t)}
                </button>
                ))}
              <button
                type="button"
                onClick={() => void previewDelete(projectId, rootPath)}
                className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--danger)] px-2 text-[11px] text-[var(--danger)]"
              >
                {t("source.action.delete")}
              </button>
            </div>
            {detail.relatedWikiPaths.length ? (
              <div>
                <p className="mb-1 text-[10.5px] uppercase tracking-[0.08em] text-[var(--text-muted)]">
                  {t("source.related.title")}
                </p>
                {detail.relatedWikiPaths.map((path) => (
                  <button
                    type="button"
                    key={path}
                    onClick={() => onOpenPage(path)}
                    className="block w-full truncate py-1 text-left font-mono text-[10.5px] text-[var(--accent-hover)]"
                  >
                    {path}
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
        {error ? (
          <p role="alert" className="mb-0 mt-2 text-[11.5px] leading-4 text-[var(--danger)]">
            {error}
          </p>
        ) : null}
      </SourceSection>
    </div>
  );
}

function SourceSection({
  order,
  title,
  children,
}: {
  order: number;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section aria-labelledby={`source-section-${order}`} className="px-4 py-3">
      <h3
        id={`source-section-${order}`}
        className="mb-2 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]"
      >
        {order}. {title}
      </h3>
      {children}
    </section>
  );
}

function actionIcon(kind: SourceCandidateKind) {
  switch (kind) {
    case "ocr":
      return <ScanText size={13} />;
    case "asr":
    case "subtitle":
      return <Subtitles size={13} />;
    case "refresh":
      return <RefreshCw size={13} />;
    case "ai_organize":
      return <Sparkles size={13} />;
  }
}

function primaryCandidateKind(
  action: SourcePrimaryAction,
): SourceReprocessKind | null {
  switch (action) {
    case "reprocess_ocr":
      return "ocr";
    case "reprocess_asr":
      return "asr";
    case "refresh_source":
      return "refresh";
    case "review_candidate":
    case "none":
      return null;
  }
}

function isReprocessKind(kind: SourceCandidateKind): kind is SourceReprocessKind {
  return kind !== "ai_organize";
}

function actionLabel(
  kind: SourceCandidateKind,
  t: (key: string) => string,
) {
  return t(`source.action.${kind}`);
}
