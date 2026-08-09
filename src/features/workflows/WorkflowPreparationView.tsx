import { ArrowLeft, Check, ShieldAlert } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { useWorkflowStore, workflowOperationPending } from "../../stores/workflowStore";
import type { WorkflowArtifactType, WorkflowPreparation, WorkflowPrerequisiteAction, WorkflowRouteSelection, WorkflowScope } from "../../types/workflow";
import { workflowKindKey } from "./workflowPresentation";

const MAX_VISIBLE_SCOPE_OPTIONS = 200;

function sourceVersionKey(source: { sourceId: string; versionId: string }): string {
  return `${source.sourceId}\0${source.versionId}`;
}

export function workflowRouteSelectionKey(route: WorkflowRouteSelection): string {
  return route.kind === "agent" ? `agent:${route.agent}` : `byok:${route.provider}`;
}

function stringSetEqual(left: readonly string[], right: readonly string[]): boolean {
  if (left.length !== right.length) return false;
  const expected = new Set(left);
  return right.every((value) => expected.has(value));
}

function workflowScopeEqual(left: WorkflowScope, right: WorkflowScope): boolean {
  if (left.kind !== right.kind) return false;
  if (left.kind === "health_check" && right.kind === "health_check") return left.mode === right.mode;
  if (left.kind === "update_wiki" && right.kind === "update_wiki") {
    return left.mode === right.mode
      && stringSetEqual(left.sourceVersions.map(sourceVersionKey), right.sourceVersions.map(sourceVersionKey));
  }
  if (left.kind === "generate_content" && right.kind === "generate_content") {
    return left.artifactType === right.artifactType
      && left.outputPath === right.outputPath
      && stringSetEqual(left.pagePaths, right.pagePaths);
  }
  return false;
}

export function WorkflowPreparationView({ preparation, onBack, onStart, onPrerequisite, onReprepare }: {
  preparation: WorkflowPreparation;
  onBack: () => void;
  onStart: (restricted: boolean, remote: boolean) => void;
  onPrerequisite: (action: WorkflowPrerequisiteAction) => void;
  onReprepare: (scope: WorkflowScope, route: WorkflowRouteSelection | null) => void;
}) {
  const { t } = useTranslation();
  const operations = useWorkflowStore((state) => state.operations);
  const preparePending = workflowOperationPending(operations, `prepare:${preparation.kind}`);
  const startPending = workflowOperationPending(operations, `start:${preparation.preparationId}`);
  const prerequisitePending = workflowOperationPending(operations, "prerequisite:project:");
  const [restricted, setRestricted] = useState(false);
  const [remote, setRemote] = useState(false);
  const [scope, setScope] = useState<WorkflowScope>(preparation.scope);
  const [routeChoice, setRouteChoice] = useState("auto");
  const [scopeQuery, setScopeQuery] = useState("");
  const [scopePage, setScopePage] = useState(0);

  useEffect(() => {
    setScope(preparation.scope);
    setRouteChoice("auto");
    setRestricted(false);
    setRemote(false);
    setScopeQuery("");
    setScopePage(0);
  }, [preparation.preparationRevision, preparation.scope]);

  const routeSelection = useMemo<WorkflowRouteSelection | null>(() => {
    if (routeChoice === "auto") return null;
    return preparation.availableRoutes?.find((route) => workflowRouteSelectionKey(route) === routeChoice) ?? null;
  }, [preparation.availableRoutes, routeChoice]);
  const scopeChanged = !workflowScopeEqual(scope, preparation.scope) || routeChoice !== "auto";
  const sourceOptions = preparation.availableSourceVersions ?? (preparation.scope.kind === "update_wiki" ? preparation.scope.sourceVersions : []);
  const pageOptions = preparation.availableWikiPages ?? (preparation.scope.kind === "generate_content" ? preparation.scope.pagePaths : []);
  const normalizedQuery = scopeQuery.trim().toLocaleLowerCase();
  const filteredSourceOptions = useMemo(() => sourceOptions.filter((source) =>
    !normalizedQuery
    || source.sourceId.toLocaleLowerCase().includes(normalizedQuery)
    || source.versionId.toLocaleLowerCase().includes(normalizedQuery),
  ), [normalizedQuery, sourceOptions]);
  const filteredPageOptions = useMemo(() => pageOptions.filter((path) =>
    !normalizedQuery || path.toLocaleLowerCase().includes(normalizedQuery),
  ), [normalizedQuery, pageOptions]);
  const selectedSourceKeys = useMemo(() => new Set(
    scope.kind === "update_wiki" ? scope.sourceVersions.map(sourceVersionKey) : [],
  ), [scope]);
  const selectedPagePaths = useMemo(() => new Set(
    scope.kind === "generate_content" ? scope.pagePaths : [],
  ), [scope]);
  const selectableCount = scope.kind === "update_wiki" ? sourceOptions.length : scope.kind === "generate_content" ? pageOptions.length : 0;
  const filteredCount = scope.kind === "update_wiki" ? filteredSourceOptions.length : scope.kind === "generate_content" ? filteredPageOptions.length : 0;
  const scopePageCount = Math.max(1, Math.ceil(filteredCount / MAX_VISIBLE_SCOPE_OPTIONS));
  const boundedScopePage = Math.min(scopePage, scopePageCount - 1);
  const scopePageStart = boundedScopePage * MAX_VISIBLE_SCOPE_OPTIONS;
  const selectedCount = scope.kind === "update_wiki" ? scope.sourceVersions.length : scope.kind === "generate_content" ? scope.pagePaths.length : 0;
  const selectFiltered = () => {
    if (scope.kind === "update_wiki") {
      const selected = new Map(scope.sourceVersions.map((source) => [sourceVersionKey(source), source]));
      for (const source of filteredSourceOptions) selected.set(sourceVersionKey(source), source);
      setScope({ ...scope, sourceVersions: [...selected.values()] });
    } else if (scope.kind === "generate_content") {
      setScope({ ...scope, pagePaths: [...new Set([...scope.pagePaths, ...filteredPageOptions])] });
    }
  };
  const clearSelection = () => {
    if (scope.kind === "update_wiki") setScope({ ...scope, sourceVersions: [] });
    else if (scope.kind === "generate_content") setScope({ ...scope, pagePaths: [] });
  };
  const acknowledgementActions: WorkflowPrerequisiteAction[] = ["acknowledge_restricted_content", "acknowledge_remote_provider"];
  const blocking = preparation.prerequisites.some((item) => item.blocking && !acknowledgementActions.includes(item.action));
  const needsRestricted = preparation.prerequisites.some((item) => item.action === "acknowledge_restricted_content");
  const needsRemote = preparation.prerequisites.some((item) => item.action === "acknowledge_remote_provider");
  const canStart = !blocking && (!needsRestricted || restricted) && (!needsRemote || remote);

  return (
    <div className="workflow-preparation">
      <button className="workflow-back" onClick={onBack} type="button"><ArrowLeft aria-hidden="true" size={14} />{t("workflows.action.back")}</button>
      <div className="workflows-intro">
        <h2>{t("workflows.preparation.title", { workflow: t(workflowKindKey(preparation.kind)) })}</h2>
        <p>{t("workflows.preparation.description")}</p>
      </div>
      <dl className="workflow-facts">
        <div><dt>{t("workflows.preparation.scope")}</dt><dd>{t(`workflows.scope.${preparation.scope.kind}`)}</dd></div>
        <div><dt>{t("workflows.preparation.count")}</dt><dd>{preparation.baseline.itemCount}</dd></div>
        <div><dt>{t("workflows.preparation.route")}</dt><dd>{preparation.route ? t(`workflows.route.${preparation.route.kind}`) : t("workflows.route.none")}</dd></div>
        <div><dt>{t("workflows.preparation.output")}</dt><dd className="font-mono">{preparation.output.location ?? t("workflows.output.session")}</dd></div>
        <div><dt>{t("workflows.preparation.git")}</dt><dd>{t(`workflows.git.${preparation.gitPolicy}`)}</dd></div>
        <div><dt>{t("workflows.preparation.wikiWrite")}</dt><dd>{preparation.output.mayChangeWiki ? t("workflows.preparation.wikiWriteYes") : t("workflows.preparation.wikiWriteNo")}</dd></div>
        <div><dt>{t("workflows.preparation.baseline")}</dt><dd className="font-mono">{preparation.baseline.fingerprint.slice(0, 12)}</dd></div>
      </dl>
      <section className="workflow-scope-editor" aria-labelledby="workflow-scope-title">
        <h3 id="workflow-scope-title">{t("workflows.preparation.configureScope")}</h3>
        {scope.kind === "update_wiki" ? <>
          <div className="workflow-option-row" role="radiogroup" aria-label={t("workflows.preparation.updateMode")}><label><input checked={scope.mode === "changed_sources"} name="update-mode" onChange={() => setScope({ ...scope, mode: "changed_sources" })} type="radio" />{t("workflows.mode.changedSources")}</label><label><input checked={scope.mode === "full_recompile"} name="update-mode" onChange={() => setScope({ ...scope, mode: "full_recompile", sourceVersions: sourceOptions })} type="radio" />{t("workflows.mode.fullRecompile")}</label></div>
          <ScopeOptionToolbar query={scopeQuery} onQueryChange={(value) => { setScopeQuery(value); setScopePage(0); }} onSelect={selectFiltered} onClear={clearSelection} selectedCount={selectedCount} filteredCount={filteredCount} totalCount={selectableCount} page={boundedScopePage} pageCount={scopePageCount} onPageChange={setScopePage} />
          <div className="workflow-scope-items">{filteredSourceOptions.slice(scopePageStart, scopePageStart + MAX_VISIBLE_SCOPE_OPTIONS).map((source) => { const key = sourceVersionKey(source); const selected = selectedSourceKeys.has(key); return <label key={key}><input aria-label={`${source.sourceId}:${source.versionId}`} checked={selected} onChange={(event) => setScope({ ...scope, sourceVersions: event.target.checked ? [...scope.sourceVersions, source] : scope.sourceVersions.filter((item) => sourceVersionKey(item) !== key) })} type="checkbox" /><code>{source.sourceId}</code><span>{source.versionId}</span></label>; })}</div>
        </> : scope.kind === "health_check" ? <div className="workflow-option-row" role="radiogroup" aria-label={t("workflows.preparation.healthMode")}><label><input checked={scope.mode === "local_quick"} name="health-mode" onChange={() => setScope({ ...scope, mode: "local_quick" })} type="radio" />{t("workflows.mode.localQuick")}</label><label><input checked={scope.mode === "complete"} name="health-mode" onChange={() => setScope({ ...scope, mode: "complete" })} type="radio" />{t("workflows.mode.complete")}</label></div> : <>
          <label className="workflow-field">{t("workflows.preparation.artifactType")}<select onChange={(event) => setScope({ ...scope, artifactType: event.target.value as WorkflowArtifactType })} value={scope.artifactType}><option value="beautiful_read">{t("workflows.artifact.beautifulRead")}</option><option value="knowledge_card">{t("workflows.artifact.knowledgeCard")}</option><option value="concept_map">{t("workflows.artifact.conceptMap")}</option><option value="project_report">{t("workflows.artifact.projectReport")}</option></select></label>
          <ScopeOptionToolbar query={scopeQuery} onQueryChange={(value) => { setScopeQuery(value); setScopePage(0); }} onSelect={selectFiltered} onClear={clearSelection} selectedCount={selectedCount} filteredCount={filteredCount} totalCount={selectableCount} page={boundedScopePage} pageCount={scopePageCount} onPageChange={setScopePage} />
          <div className="workflow-scope-items">{filteredPageOptions.slice(scopePageStart, scopePageStart + MAX_VISIBLE_SCOPE_OPTIONS).map((path) => { const selected = selectedPagePaths.has(path); return <label key={path}><input aria-label={path} checked={selected} onChange={(event) => setScope({ ...scope, pagePaths: event.target.checked ? [...scope.pagePaths, path] : scope.pagePaths.filter((item) => item !== path) })} type="checkbox" /><code>{path}</code></label>; })}</div>
          <label className="workflow-field">{t("workflows.preparation.outputPath")}<input onChange={(event) => setScope({ ...scope, outputPath: event.target.value || null })} type="text" value={scope.outputPath ?? ""} /></label>
        </>}
        {(preparation.availableRoutes?.length ?? 0) > 0 ? <label className="workflow-field">{t("workflows.preparation.routeOverride")}<select onChange={(event) => setRouteChoice(event.target.value)} value={routeChoice}><option value="auto">{t("workflows.route.auto")}</option>{preparation.availableRoutes?.map((route) => { const key = workflowRouteSelectionKey(route); return <option key={key} value={key}>{route.kind === "agent" ? `${t("workflows.route.agent")} · ${route.agent}` : `${t("workflows.route.byok")} · ${route.provider}`}</option>; })}</select></label> : null}
        <button className="btn btn--secondary" disabled={!scopeChanged || preparePending || startPending} onClick={() => onReprepare(scope, routeSelection)} type="button">{t("workflows.action.updatePreparation")}</button>
      </section>
      <section className="workflow-prerequisites" aria-label={t("workflows.preparation.prerequisites")}>
        <h3>{t("workflows.preparation.prerequisites")}</h3>
        {preparation.prerequisites.length === 0 ? (
          <p className="workflow-check"><Check aria-hidden="true" size={14} />{t("workflows.preparation.ready")}</p>
        ) : preparation.prerequisites.map((item) => (
          <div className={item.blocking ? "workflow-prerequisite is-blocking" : "workflow-prerequisite"} key={item.code}>
            <ShieldAlert aria-hidden="true" size={14} />
            <span>{t(item.messageKey)}</span>
            {!acknowledgementActions.includes(item.action) ? <button className="btn btn--ghost btn--sm ml-auto" disabled={prerequisitePending} onClick={() => onPrerequisite(item.action)} type="button">{t("workflows.action.resolve")}</button> : null}
          </div>
        ))}
      </section>
      {needsRestricted ? <label className="workflow-confirm"><input checked={restricted} onChange={(event) => setRestricted(event.target.checked)} type="checkbox" />{t("workflows.confirm.restricted")}</label> : null}
      {needsRemote ? <label className="workflow-confirm"><input checked={remote} onChange={(event) => setRemote(event.target.checked)} type="checkbox" />{t("workflows.confirm.remote")}</label> : null}
      <details className="workflow-execution-details"><summary>{t("workflows.preparation.executionDetails")}</summary><dl><dt>{t("workflows.preparation.route")}</dt><dd>{preparation.route ? t(`workflows.route.${preparation.route.kind}`) : t("workflows.route.none")}</dd><dt>{t("workflows.preparation.dataBoundary")}</dt><dd>{t(`workflows.boundary.${preparation.scope.kind}`)}</dd></dl></details>
      <div className="workflow-actions"><button className="btn btn--primary" disabled={!canStart || scopeChanged || startPending || preparePending} onClick={() => onStart(restricted, remote)} type="button">{preparation.quickRerunEligible ? t("workflows.action.runAgain") : t("workflows.action.start")}</button></div>
    </div>
  );
}

function ScopeOptionToolbar({ query, onQueryChange, onSelect, onClear, selectedCount, filteredCount, totalCount, page, pageCount, onPageChange }: {
  query: string;
  onQueryChange: (value: string) => void;
  onSelect: () => void;
  onClear: () => void;
  selectedCount: number;
  filteredCount: number;
  totalCount: number;
  page: number;
  pageCount: number;
  onPageChange: (page: number) => void;
}) {
  const { t } = useTranslation();
  return <div className="workflow-scope-toolbar">
    <label className="workflow-field">
      {t("workflows.preparation.searchOptions")}
      <input onChange={(event) => onQueryChange(event.target.value)} type="search" value={query} />
    </label>
    <div className="workflow-scope-toolbar__actions">
      <span aria-live="polite">{t("workflows.preparation.selectionCount", { selected: selectedCount, filtered: filteredCount, total: totalCount })}</span>
      <button className="btn btn--ghost btn--sm" disabled={filteredCount === 0} onClick={onSelect} type="button">{t("workflows.preparation.selectResults")}</button>
      <button className="btn btn--ghost btn--sm" disabled={selectedCount === 0} onClick={onClear} type="button">{t("workflows.preparation.clearSelection")}</button>
      {pageCount > 1 ? <>
        <button className="btn btn--ghost btn--sm" disabled={page === 0} onClick={() => onPageChange(page - 1)} type="button">{t("workflows.preparation.previousOptions")}</button>
        <span>{t("workflows.preparation.optionPage", { current: page + 1, total: pageCount })}</span>
        <button className="btn btn--ghost btn--sm" disabled={page + 1 >= pageCount} onClick={() => onPageChange(page + 1)} type="button">{t("workflows.preparation.nextOptions")}</button>
      </> : null}
    </div>
  </div>;
}
