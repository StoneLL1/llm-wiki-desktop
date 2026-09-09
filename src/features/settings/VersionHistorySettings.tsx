import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowLeft, ChevronRight, FileText, History, LoaderCircle, RefreshCw, RotateCcw, ShieldCheck } from "lucide-react";

import { LazyActionableErrorNotice } from "../../components/app/LazyActionableErrorNotice";
import { captureProjectScope, invalidateProjectResources, isProjectScopeCurrent } from "../../stores/projectScope";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { refreshProjectFacts } from "../../stores/projectFactsStore";
import type { PendingAction } from "../../types/backend";
import type { ProjectSummary } from "../../types/project";
import {
  confirmVersionAction, getVersion, getVersionDiff, getVersionStatus, listVersions, prepareVersionAction,
  type VersionFileDiff, type VersionOperation, type VersionPage, type VersionStatus,
} from "../../services/versionHistoryApi";

/** The settings dialog owns focus. Confirmation is an inline step, not a second modal. */
export function VersionHistorySettings({ project }: { project: ProjectSummary }) {
  const { t, i18n } = useTranslation();
  const target = useNavigationStore((state) => state.versionHistoryTarget);
  const request = useMemo(() => ({ projectId: project.projectId, projectRootPath: project.rootPath }), [project.projectId, project.rootPath]);
  const [status, setStatus] = useState<VersionStatus | null>(null);
  const [page, setPage] = useState<VersionPage | null>(null);
  const [cursors, setCursors] = useState<string[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(target?.operationId ?? null);
  const [detail, setDetail] = useState<VersionOperation | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [diff, setDiff] = useState<VersionFileDiff | null>(null);
  const [savingVersion, setSavingVersion] = useState(false);
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [diffLoading, setDiffLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const mutationPending = useRef(false);
  const pendingRef = useRef<PendingAction | null>(null);
  const confirmationHeading = useRef<HTMLHeadingElement>(null);
  const backButton = useRef<HTMLButtonElement>(null);
  const rows = useRef(new Map<string, HTMLButtonElement>());
  const lastSelection = useRef<string | null>(null);
  const mounted = useRef(true);
  const actionEpoch = useRef(0);
  const cursor = cursors.at(-1);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false; actionEpoch.current += 1;
      if (pendingRef.current && !mutationPending.current) void confirmVersionAction(pendingRef.current.id, false).catch(() => {});
    };
  }, []);

  useEffect(() => { pendingRef.current = pending; }, [pending]);
  useEffect(() => {
    if (pending) confirmationHeading.current?.focus();
    else if (selectedId) { lastSelection.current = selectedId; backButton.current?.focus(); }
    else if (lastSelection.current) rows.current.get(lastSelection.current)?.focus();
  }, [pending, selectedId]);

  const guard = () => {
    const scope = captureProjectScope();
    const authority = useProjectStore.getState().authority;
    return () => {
      const current = useProjectStore.getState();
      return mounted.current && isProjectScopeCurrent(scope)
        && current.currentProject.projectId === request.projectId
        && current.currentProject.rootPath === request.projectRootPath
        && current.authority?.canonicalIdentityKey === authority?.canonicalIdentityKey
        && current.authority?.identityRevision === authority?.identityRevision;
    };
  };

  useEffect(() => {
    let active = true;
    const scope = captureProjectScope();
    setLoading(true);
    setError(null);
    void listVersions({ ...request, cursor, limit: 50 }).then((next) => {
      if (active && isProjectScopeCurrent(scope)) setPage(next);
    }).catch((failure: unknown) => {
      if (active && isProjectScopeCurrent(scope)) setError(failure);
    }).finally(() => { if (active && isProjectScopeCurrent(scope)) setLoading(false); });
    return () => { active = false; };
  }, [request, cursor, revision]);

  useEffect(() => {
    let active = true;
    const scope = captureProjectScope();
    void getVersionStatus(request).then((next) => {
      if (active && isProjectScopeCurrent(scope)) setStatus(next);
    }).catch((failure: unknown) => {
      if (active && isProjectScopeCurrent(scope)) setError(failure);
    });
    return () => { active = false; };
  }, [request, revision]);

  useEffect(() => {
    if (target) setSelectedId(target.operationId);
  }, [target]);

  useEffect(() => {
    let active = true;
    const scope = captureProjectScope();
    setDetail(null);
    setSelectedPath(null);
    setDiff(null);
    setPending(null);
    actionEpoch.current += 1;
    if (!selectedId) { setDetailLoading(false); return; }
    setDetailLoading(true);
    void getVersion({ ...request, operationId: selectedId }).then((next) => {
      if (active && isProjectScopeCurrent(scope)) {
        setDetail(next);
        setSelectedPath(Object.keys(next.beforeHashes)[0] ?? null);
      }
    }).catch((failure: unknown) => {
      if (active && isProjectScopeCurrent(scope)) setError(failure);
    }).finally(() => { if (active && isProjectScopeCurrent(scope)) setDetailLoading(false); });
    return () => { active = false; };
  }, [selectedId, request, revision]);

  useEffect(() => {
    let active = true;
    const scope = captureProjectScope();
    setDiff(null);
    if (!selectedId || !selectedPath) { setDiffLoading(false); return; }
    setDiffLoading(true);
    void getVersionDiff({ ...request, operationId: selectedId, path: selectedPath }).then((next) => {
      if (active && isProjectScopeCurrent(scope)) setDiff(next);
    }).catch((failure: unknown) => {
      if (active && isProjectScopeCurrent(scope)) setError(failure);
    }).finally(() => { if (active && isProjectScopeCurrent(scope)) setDiffLoading(false); });
    return () => { active = false; };
  }, [request, selectedId, selectedPath, revision]);

  const prepare = async (operationId?: string, save = false) => {
    if (mutationPending.current) return;
    mutationPending.current = true;
    setBusy(true); setError(null); setSavingVersion(save);
    const current = guard();
    const epoch = ++actionEpoch.current;
    try {
      const action = await prepareVersionAction({ ...request, operationId, save });
      if (current() && epoch === actionEpoch.current) setPending(action);
      else await confirmVersionAction(action.id, false);
    } catch (failure) { if (current() && epoch === actionEpoch.current) setError(failure); }
    finally { mutationPending.current = false; if (current()) setBusy(false); }
  };

  const confirm = async (confirmed: boolean) => {
    if (!pending || mutationPending.current) return;
    mutationPending.current = true;
    setBusy(true); setError(null);
    const action = pending;
    const current = guard();
    try {
      await confirmVersionAction(action.id, confirmed);
      if (confirmed) {
        invalidateProjectResources({ projectId: request.projectId, rootPath: request.projectRootPath }, ["wiki", "graph"]);
        void refreshProjectFacts({ projectId: request.projectId, rootPath: request.projectRootPath }, ["git"]);
      }
      if (current()) { setPending(null); setRevision((value) => value + 1); }
    } catch (failure) { if (current()) setError(failure); }
    finally { mutationPending.current = false; if (current()) setBusy(false); }
  };

  const date = (value: string, timeOnly = false) => new Date(value).toLocaleString(i18n.resolvedLanguage, timeOnly
    ? { hour: "2-digit", minute: "2-digit" }
    : { year: "numeric", month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  const canRestore = detail && (detail.sourceDeletion || ["lint_fix", "manual_snapshot", "page_change", "chat_edit", "restore"].includes(detail.summary.kind)) && !["restored", "aborted"].includes(detail.summary.state);
  const paths = detail ? Object.keys(detail.beforeHashes) : [];
  const restoringSaved = detail?.summary.kind === "manual_snapshot";
  const beforeLabel = t(restoringSaved ? "versions.current" : "versions.before");
  const afterLabel = t(restoringSaved ? "versions.saved" : "versions.after");
  const errorDetails = error && typeof error === "object" && "details" in error ? error.details : null;
  const conflictPaths = errorDetails && typeof errorDetails === "object" && "paths" in errorDetails && Array.isArray(errorDetails.paths)
    ? errorDetails.paths.filter((path): path is string => typeof path === "string").slice(0, 100) : [];

  return <section className="version-history settings-view__section" aria-busy={busy}>
    <header className="version-history__heading">
      <div>
        <h2 className="settings-view__section-title">{t("versions.title")}</h2>
        <p className="settings-view__section-desc" title={project.rootPath}>{t("versions.project", { name: project.name })}</p>
      </div>
      <button className="icon-button" type="button" disabled={loading || busy} onClick={() => setRevision((value) => value + 1)} aria-label={t("versions.refresh")} title={t("versions.refresh")}><RefreshCw size={14} /></button>
    </header>

    {error ? <LazyActionableErrorNotice error={error} /> : null}
    {conflictPaths.length > 0 ? <ul className="version-history__conflicts" aria-label={t("versions.conflictPaths")}>{conflictPaths.map((path) => <li key={path}>{path}</li>)}</ul> : null}

    {pending ? <div className="version-history__confirmation">
      <h3 ref={confirmationHeading} tabIndex={-1}>{t(savingVersion ? "versions.save" : restoringSaved ? "versions.restoreSaved" : detail?.sourceDeletion ? "versions.recoverSource" : selectedId ? "versions.confirmRestore" : "versions.enable")}</h3>
      <p>{t(savingVersion ? "versions.saveDescription" : restoringSaved ? "versions.restoreSavedDescription" : selectedId ? "versions.confirmRestoreDescription" : "versions.enableDescription")}</p>
      {selectedId || savingVersion ? <ul>{pending.affectedPaths.map((path) => <li key={path}>{path}</li>)}</ul> : null}
      <div className="version-history__actions">
        <button className="btn btn--secondary" type="button" disabled={busy} onClick={() => void confirm(false)}>{t("confirmation.cancel")}</button>
        <button className="btn btn--primary" type="button" disabled={busy} onClick={() => void confirm(true)}>{busy ? t("versions.working") : t(savingVersion ? "versions.save" : selectedId ? "versions.restoreConfirm" : "versions.enableConfirm")}</button>
      </div>
    </div> : selectedId ? <>
      <button ref={backButton} className="btn btn--ghost btn--sm version-history__back" type="button" disabled={busy} onClick={() => { setSelectedId(null); setError(null); }}><ArrowLeft size={14} />{t("versions.back")}</button>
      {detailLoading ? <p className="version-history__loading"><LoaderCircle size={14} className="animate-spin" />{t("versions.loading")}</p> : detail ? <>
        <div className="version-history__operation-head">
          <div><h3>{detail.sourceDeletion ? t("versions.restoreSource", { title: detail.sourceDeletion.title }) : t(`versions.kind.${detail.summary.kind}`)}</h3><p>{date(detail.summary.createdAt)} · {t("versions.files", { count: detail.summary.fileCount })}</p></div>
          {canRestore ? <button className="btn btn--secondary btn--sm" type="button" disabled={busy} onClick={() => void prepare(detail.summary.operationId)}><RotateCcw size={13} />{t(restoringSaved ? "versions.restoreSaved" : detail.sourceDeletion ? "versions.recoverSource" : detail.summary.state === "applied" ? "versions.undo" : "versions.recover")}</button> : <span className="version-history__state">{t(`versions.state.${detail.summary.state}`)}</span>}
        </div>
        {detail.summary.state === "prepared" || detail.summary.state === "restoring" ? <p className="version-history__notice">{t("versions.interrupted")}</p> : null}
        {detail.restorationId ? <button className="btn btn--ghost btn--sm" type="button" onClick={() => setSelectedId(detail.restorationId!)}>{t("versions.viewRestoration")}</button> : null}
        <div className="version-history__files" aria-label={t("versions.affectedFiles")}>
          {paths.map((path) => <button className={selectedPath === path ? "is-active" : ""} type="button" key={path} onClick={() => setSelectedPath(path)} title={path} aria-pressed={selectedPath === path}><FileText size={13} /><span>{path}</span><ChevronRight size={12} /></button>)}
        </div>
        {diffLoading ? <p className="version-history__loading">{t("versions.loadingDiff")}</p> : diff ? <div className="version-history__diff">
          <div className="version-history__diff-labels"><span>{beforeLabel}</span><span>{afterLabel}</span></div>
          {diff.binary ? <p className="version-history__notice">{t("versions.binary", { before: diff.beforeBytes, after: diff.afterBytes })}</p> : <div className="version-history__diff-columns">
            <pre aria-label={beforeLabel}>{diff.beforeText ?? t("versions.absent")}</pre><pre aria-label={afterLabel}>{diff.afterText ?? t("versions.absent")}</pre>
          </div>}
          {diff.truncated ? <p className="version-history__notice">{t("versions.truncated")}</p> : null}
        </div> : null}
      </> : null}
    </> : <>
      <div className="version-history__protection">
        <ShieldCheck size={17} aria-hidden="true" />
        <div><h3>{t(status?.enabled ? "versions.enabled" : status ? "versions.notEnabled" : "versions.checking")}</h3><p>{t("versions.description")}</p></div>
        {status?.enabled ? <button className="btn btn--secondary btn--sm" type="button" disabled={busy} onClick={() => void prepare(undefined, true)}>{t("versions.save")}</button> : null}
        {status && !status.enabled ? <button className="btn btn--secondary btn--sm" type="button" disabled={busy} onClick={() => void prepare()}>{t("versions.enable")}</button> : null}
      </div>
      {loading && !page ? <p className="version-history__loading"><LoaderCircle size={14} className="animate-spin" />{t("versions.loading")}</p> : page?.operations.length ? <div className="version-history__list">
        {page.operations.map((operation, index) => {
          const day = new Date(operation.createdAt).toLocaleDateString(i18n.resolvedLanguage);
          const priorDay = index > 0 ? new Date(page.operations[index - 1].createdAt).toLocaleDateString(i18n.resolvedLanguage) : null;
          return <div key={operation.operationId}>
            {day !== priorDay ? <h3 className="version-history__date">{day}</h3> : null}
            <button ref={(element) => { if (element) rows.current.set(operation.operationId, element); else rows.current.delete(operation.operationId); }} className="version-history__row" type="button" onClick={() => { setSelectedId(operation.operationId); setError(null); }}>
              <time dateTime={operation.createdAt}>{date(operation.createdAt, true)}</time>
              <span className="version-history__row-title">{t(`versions.kind.${operation.kind}`)}{operation.state !== "applied" ? <small>{t(`versions.state.${operation.state}`)}</small> : null}</span>
              <span className="version-history__count">{t("versions.files", { count: operation.fileCount })}</span><ChevronRight size={14} />
            </button>
          </div>;
        })}
      </div> : <div className="version-history__empty"><History size={23} aria-hidden="true" /><p>{t("versions.empty")}</p></div>}
      {page?.unreadableCount ? <p className="version-history__notice">{t("versions.unreadable", { count: page.unreadableCount })}</p> : null}
      {(cursors.length > 0 || page?.nextCursor) ? <div className="version-history__pagination">
        <button className="btn btn--ghost btn--sm" type="button" disabled={!cursors.length || loading} onClick={() => setCursors((items) => items.slice(0, -1))}>{t("versions.previous")}</button>
        <button className="btn btn--ghost btn--sm" type="button" disabled={!page?.nextCursor || loading} onClick={() => { if (page?.nextCursor) setCursors((items) => [...items, page.nextCursor!]); }}>{t("versions.next")}</button>
      </div> : null}
      <details className="version-history__advanced"><summary>{t("versions.advanced")}</summary><dl>
        <dt>{t("versions.location")}</dt><dd>{project.rootPath}</dd>
        <dt>Git</dt><dd>{status?.gitVersion ?? t("versions.unavailable")}</dd>
        <dt>{t("versions.branch")}</dt><dd>{status?.git.branch ?? "—"}</dd>
        <dt>HEAD</dt><dd>{status?.git.head ?? "—"}</dd>
        <dt>{t("versions.worktree")}</dt><dd>{status ? t(status.git.hasChanges ? "versions.hasChanges" : "versions.noChanges") : "—"}</dd>
      </dl></details>
      <p className="version-history__footnote">{t("versions.localOnly")}</p>
    </>}
  </section>;
}
