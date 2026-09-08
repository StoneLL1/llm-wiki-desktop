import { ArrowLeft, Play, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getUpdateWikiOptions, listUpdateWikiSources } from "../../services/workflowApi";
import { normalizeBackendError } from "../../lib/backendError";
import { useWorkflowStore, workflowOperationPending } from "../../stores/workflowStore";
import { useNavigationStore } from "../../stores/navigationStore";
import type { ProjectSummary } from "../../types/project";
import type { UpdateWikiOptions, UpdateWikiRequest, UpdateWikiSourcePage, WorkflowRouteSelection } from "../../types/workflow";

const routeKey = (route: WorkflowRouteSelection) => route.kind === "agent" ? `agent:${route.agent}` : `byok:${route.provider}`;

/** A draft is editable independently of directory and environment query lifetimes. */
export function UpdateWikiForm({ project, onBack, onStart }: {
  project: Pick<ProjectSummary, "projectId" | "rootPath">;
  onBack: () => void;
  onStart: (intent: UpdateWikiRequest) => Promise<void>;
}) {
  const { t } = useTranslation();
  const draft = useWorkflowStore((state) => state.updateDraft);
  const setDraft = useWorkflowStore((state) => state.setUpdateDraft);
  const starting = useWorkflowStore((state) => workflowOperationPending(state.operations, "update:start"));
  const settingsOpen = useNavigationStore((state) => state.settingsOpen);
  const [options, setOptions] = useState<UpdateWikiOptions | null>(null);
  const [optionsError, setOptionsError] = useState(false);
  const [page, setPage] = useState<UpdateWikiSourcePage | null>(null);
  const [query, setQuery] = useState("");
  const [search, setSearch] = useState("");
  const [offset, setOffset] = useState(0);
  const [listing, setListing] = useState(false);
  const [listError, setListError] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);
  const [remote, setRemote] = useState(false);
  const submission = useRef<{ body: string; requestId: string } | null>(null);
  const manual = draft.selection.kind === "selected";

  useEffect(() => {
    if (settingsOpen) return;
    let active = true;
    const request = { projectId: project.projectId, projectRootPath: project.rootPath };
    void getUpdateWikiOptions(request).then((value) => { if (active) { setOptions(value); setOptionsError(false); } }).catch(() => { if (active) setOptionsError(true); });
    return () => { active = false; };
  }, [project.projectId, project.rootPath, refresh, settingsOpen]);

  useEffect(() => {
    if (!manual) return;
    let active = true;
    setListing(true);
    setListError(null);
    void listUpdateWikiSources({ projectId: project.projectId, projectRootPath: project.rootPath, query: search, offset }).then((value) => {
      if (active) setPage(value);
    }).catch((error: unknown) => {
      if (active) setListError(normalizeBackendError(error).technicalDetails ?? String(error));
    }).finally(() => { if (active) setListing(false); });
    return () => { active = false; };
  }, [manual, project.projectId, project.rootPath, search, offset, refresh]);

  const selected = draft.selection.kind === "selected" ? draft.selection.sourceVersions : [];
  const chosen = draft.routeSelection ?? options?.defaultRoute;
  const start = () => {
    const body = JSON.stringify({ ...draft, acknowledgeRemoteProvider: remote });
    if (submission.current?.body !== body) submission.current = { body, requestId: crypto.randomUUID() };
    void onStart({ ...draft, requestId: submission.current.requestId, acknowledgeRemoteProvider: remote });
  };
  const openSettings = () => useNavigationStore.getState().openSettings("ai");

  return <div className="workflow-preparation">
    <div className="workflow-panel-heading">
      <RefreshCw aria-hidden="true" size={17} />
      <h2 data-workflow-surface-title tabIndex={-1}>{t("workflows.kind.update_wiki")}</h2>
      <button className="workflow-back" onClick={onBack} type="button"><ArrowLeft size={14} aria-hidden="true" />{t("workflows.action.back")}</button>
    </div>
    <div className="workflow-panel-body">
      <fieldset className="workflow-preparation-controls" disabled={starting}>
        <div className="workflow-preparation-step">
          <div className="workflow-preparation-step__label">{t("workflows.preparation.updateMode")}</div>
          <div className="workflow-option-row">
            {(["changed_sources", "full_recompile"] as const).map((mode) => <label key={mode}>
              <input type="radio" name="update-mode" checked={draft.mode === mode} onChange={() => setDraft({ ...draft, mode })} />
              {t(mode === "changed_sources" ? "workflows.mode.changedSources" : "workflows.mode.fullRecompile")}
            </label>)}
          </div>
        </div>
        <div className="workflow-preparation-step">
          <div className="workflow-preparation-step__label">{t("workflows.update.sources")}</div>
          <div>
          <div className="workflow-option-row">
            <label><input type="radio" name="update-selection" checked={!manual} onChange={() => setDraft({ ...draft, selection: { kind: "automatic" } })} />{t("workflows.update.automatic")}</label>
            <label><input type="radio" name="update-selection" checked={manual} onChange={() => setDraft({ ...draft, selection: { kind: "selected", sourceVersions: [] } })} />{t("workflows.update.manual")}</label>
          </div>
          {!manual && <p className="workflow-scope-state">{t("workflows.update.automaticHint")}</p>}
          {manual && <>
            <div className="workflow-option-row">
              <input className="input" placeholder={t("workflows.update.search")} aria-label={t("workflows.update.search")} value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") { setSearch(query); setOffset(0); setRefresh((value) => value + 1); } }} />
              <button className="btn btn--secondary btn--sm" type="button" onClick={() => { setSearch(query); setOffset(0); setRefresh((value) => value + 1); }}>{t("workflows.update.search")}</button>
              <span>{t("workflows.update.selected", { count: selected.length })}</span>
              <button className="btn btn--secondary btn--sm" type="button" onClick={() => setDraft({ ...draft, selection: { kind: "selected", sourceVersions: [] } })}>{t("workflows.update.clear")}</button>
            </div>
            {listing && <p role="status" className="workflow-scope-state">{t("workflows.update.loadingSources")}</p>}
            {listError && <p role="alert">{t("workflows.update.sourceError")} <button type="button" onClick={() => setRefresh((value) => value + 1)}>{t("workflows.action.retry")}</button></p>}
            <div className="workflow-scope-items">{!listing && page?.sources.map((source) => {
              const checked = selected.some((item) => item.sourceId === source.sourceId && item.versionId === source.versionId);
              return <label key={`${source.sourceId}:${source.versionId}`}>
                <input type="checkbox" checked={checked} disabled={draft.mode === "changed_sources" && source.consumed} onChange={(event) => {
                  const remaining = selected.filter((item) => item.sourceId !== source.sourceId);
                  setDraft({ ...draft, selection: { kind: "selected", sourceVersions: event.target.checked ? [...remaining, { sourceId: source.sourceId, versionId: source.versionId }] : remaining } });
                }} />{source.title}{source.consumed ? ` · ${t("workflows.update.consumed")}` : ""}
              </label>;
            })}</div>
            {!listing && page?.total === 0 && <p className="workflow-scope-state">{t("workflows.update.empty")}</p>}
            {!!page?.unavailable && <p className="workflow-scope-state">{t("workflows.update.unavailable", { count: page.unavailable })}</p>}
            <div className="workflow-option-row">
              <button type="button" className="btn btn--secondary btn--sm" disabled={listing || offset === 0} onClick={() => setOffset(Math.max(0, offset - 100))}>{t("workflows.update.previous")}</button>
              <button type="button" className="btn btn--secondary btn--sm" disabled={listing || page?.nextOffset == null} onClick={() => setOffset(page?.nextOffset ?? 0)}>{t("workflows.update.next")}</button>
            </div>
          </>}
          </div>
        </div>
        <div className="workflow-preparation-step">
          <div className="workflow-preparation-step__label">{t("workflows.preparation.routeOverride")}</div>
          <div>
          <label className="workflow-field"><span className="sr-only">{t("workflows.preparation.routeOverride")}</span>
            <select value={draft.routeSelection ? routeKey(draft.routeSelection) : "auto"} onChange={(event) => { setRemote(false); setDraft({ ...draft, routeSelection: options?.routes.find((route) => routeKey(route) === event.target.value) ?? null }); }}>
              <option value="auto">{t("workflows.route.auto")}{options?.defaultRoute ? ` · ${routeKey(options.defaultRoute)}` : ""}</option>
              {options?.routes.map((route) => <option key={routeKey(route)} value={routeKey(route)}>{routeKey(route)}</option>)}
            </select>
          </label>
          {optionsError && <p role="status">{t("workflows.update.optionsError")} <button type="button" onClick={() => setRefresh((value) => value + 1)}>{t("workflows.action.retry")}</button></p>}
          <button className="btn btn--secondary btn--sm" type="button" onClick={openSettings}>{t("workflows.action.openSettings")}</button>
          {chosen?.kind === "byok" && <div className="workflow-option-row"><label><input type="checkbox" checked={remote} onChange={(event) => setRemote(event.target.checked)} />{t("workflows.update.remote")}</label></div>}
          </div>
        </div>
        <div className="workflow-start-bar">
          <span className="workflow-start-summary">{t("workflows.git.automaticUpdateHistory")}</span>
          <button type="button" className="btn btn--primary" aria-busy={starting} disabled={starting || (manual && selected.length === 0)} onClick={start}><Play size={14} aria-hidden="true" />{t(starting ? "workflows.action.starting" : "workflows.action.start")}</button>
        </div>
      </fieldset>
    </div>
  </div>;
}
