import { ArrowLeft, Check, ShieldAlert } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useWorkflowStore, workflowOperationPending } from "../../stores/workflowStore";
import type {
  WorkflowArtifactType,
  WorkflowPreparation,
  WorkflowPreparationDraft,
  WorkflowPrerequisiteAction,
  WorkflowRoute,
  WorkflowRouteSelection,
  WorkflowScope,
} from "../../types/workflow";
import { workflowKindDescriptionKey, workflowKindKey } from "./workflowPresentation";

const MAX_VISIBLE_SCOPE_OPTIONS = 200;

const ARTIFACT_SKILL_IDS: Record<WorkflowArtifactType, string> = {
  beautiful_read: "html-beautiful-read",
  knowledge_card: "html-knowledge-card",
  concept_map: "html-concept-map",
  project_report: "html-project-report",
};

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

function scopeValidationKey(scope: WorkflowScope, updateAutoDetect: boolean): string | null {
  if (scope.kind === "health_check") return null;
  if (scope.kind === "update_wiki") {
    return scope.sourceVersions.length === 0 && !updateAutoDetect
      ? "workflows.preparation.invalid.updateWikiEmpty"
      : null;
  }
  if (scope.artifactType === "beautiful_read" && scope.pagePaths.length !== 1) {
    return "workflows.preparation.invalid.beautifulReadScope";
  }
  if (
    (scope.artifactType === "knowledge_card" || scope.artifactType === "concept_map")
    && scope.pagePaths.length === 0
  ) {
    return "workflows.preparation.invalid.generatedScopeEmpty";
  }
  if (scope.artifactType === "project_report" && scope.pagePaths.length !== 0) {
    return "workflows.preparation.invalid.projectReportScope";
  }
  return null;
}

function routeDisplay(
  route: WorkflowRoute | null,
  t: (key: string) => string,
): string {
  if (!route) return t("workflows.route.none");
  if (route.kind === "local") return t("workflows.route.local");
  if (route.kind === "agent") return `${t("workflows.route.agent")} · ${route.agent}`;
  return `${t("workflows.route.byok")} · ${route.provider}`;
}

export function WorkflowPreparationView({ preparation, onBack, onStart, onPrerequisite, onReprepare }: {
  preparation: WorkflowPreparation;
  onBack: () => void;
  onStart: (restricted: boolean, remote: boolean) => void;
  onPrerequisite: (action: WorkflowPrerequisiteAction, draft?: WorkflowPreparationDraft) => void;
  onReprepare: (scope: WorkflowScope, route: WorkflowRouteSelection | null) => void;
}) {
  const { t } = useTranslation();
  const operations = useWorkflowStore((state) => state.operations);
  const preparePending = workflowOperationPending(operations, `prepare:${preparation.kind}`);
  const startPending = workflowOperationPending(operations, `start:${preparation.preparationId}`);
  const prerequisitePending = workflowOperationPending(operations, "prerequisite:project:");
  const [restricted, setRestricted] = useState(false);
  const [remote, setRemote] = useState(false);
  const [scopeConfirmed, setScopeConfirmed] = useState(false);
  const [scope, setScope] = useState<WorkflowScope>(preparation.scope);
  const [routeChoice, setRouteChoice] = useState("auto");
  const [preparedRouteChoice, setPreparedRouteChoice] = useState("auto");
  const [updateAutoDetect, setUpdateAutoDetect] = useState(false);
  const [scopeQuery, setScopeQuery] = useState("");
  const [scopePage, setScopePage] = useState(0);
  const executionDetailsRef = useRef<HTMLDetailsElement>(null);
  const routeSelectRef = useRef<HTMLSelectElement>(null);
  const pendingRouteChoiceRef = useRef<string | null>(null);

  useEffect(() => {
    setScope(preparation.scope);
    const nextRouteChoice = pendingRouteChoiceRef.current ?? "auto";
    pendingRouteChoiceRef.current = null;
    setRouteChoice(nextRouteChoice);
    setPreparedRouteChoice(nextRouteChoice);
    setUpdateAutoDetect(false);
    setRestricted(false);
    setRemote(false);
    setScopeConfirmed(false);
    setScopeQuery("");
    setScopePage(0);
  }, [preparation.preparationRevision, preparation.scope]);

  const visibleRoutes = useMemo(() => preparation.availableRoutes ?? [], [preparation.availableRoutes]);
  const routeSelection = useMemo<WorkflowRouteSelection | null>(() => {
    if (routeChoice === "auto") return null;
    return visibleRoutes.find((route) => workflowRouteSelectionKey(route) === routeChoice) ?? null;
  }, [routeChoice, visibleRoutes]);
  const scopeChanged = !workflowScopeEqual(scope, preparation.scope) || routeChoice !== preparedRouteChoice;
  const outputScopeChanged = scope.kind === "generate_content"
    && preparation.scope.kind === "generate_content"
    && !workflowScopeEqual(scope, preparation.scope);
  useEffect(() => {
    if (scopeChanged) setScopeConfirmed(false);
  }, [scopeChanged]);
  const sourceOptions = preparation.availableSourceVersions
    ?? (preparation.scope.kind === "update_wiki" ? preparation.scope.sourceVersions : []);
  const pageOptions = preparation.availableWikiPages
    ?? (preparation.scope.kind === "generate_content" ? preparation.scope.pagePaths : []);
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
  const isProjectReport = scope.kind === "generate_content" && scope.artifactType === "project_report";
  const selectableCount = scope.kind === "update_wiki"
    ? sourceOptions.length
    : scope.kind === "generate_content" && !isProjectReport
      ? pageOptions.length
      : 0;
  const filteredCount = scope.kind === "update_wiki"
    ? filteredSourceOptions.length
    : scope.kind === "generate_content" && !isProjectReport
      ? filteredPageOptions.length
      : 0;
  const scopePageCount = Math.max(1, Math.ceil(filteredCount / MAX_VISIBLE_SCOPE_OPTIONS));
  const boundedScopePage = Math.min(scopePage, scopePageCount - 1);
  const scopePageStart = boundedScopePage * MAX_VISIBLE_SCOPE_OPTIONS;
  const selectedCount = scope.kind === "update_wiki"
    ? scope.sourceVersions.length
    : scope.kind === "generate_content" && !isProjectReport
      ? scope.pagePaths.length
      : preparation.baseline.itemCount;
  const preparedNoChanges = preparation.kind === "update_wiki"
    && preparation.scope.kind === "update_wiki"
    && preparation.scope.mode === "changed_sources"
    && preparation.scope.sourceVersions.length === 0
    && preparation.baseline.itemCount === 0
    && workflowScopeEqual(scope, preparation.scope);
  const validationKey = preparedNoChanges ? null : scopeValidationKey(scope, updateAutoDetect);

  const selectFiltered = () => {
    if (scope.kind === "update_wiki") {
      setUpdateAutoDetect(false);
      const selected = new Map(scope.sourceVersions.map((source) => [sourceVersionKey(source), source]));
      for (const source of filteredSourceOptions) selected.set(sourceVersionKey(source), source);
      setScope({ ...scope, sourceVersions: [...selected.values()] });
    } else if (scope.kind === "generate_content" && scope.artifactType !== "project_report") {
      const selected = [...new Set([...scope.pagePaths, ...filteredPageOptions])];
      setScope({
        ...scope,
        pagePaths: scope.artifactType === "beautiful_read" ? selected.slice(0, 1) : selected,
      });
    }
  };
  const clearSelection = () => {
    if (scope.kind === "update_wiki") {
      setUpdateAutoDetect(false);
      setScope({ ...scope, sourceVersions: [] });
    }
    else if (scope.kind === "generate_content") setScope({ ...scope, pagePaths: [] });
  };
  const acknowledgementActions: WorkflowPrerequisiteAction[] = [
    "acknowledge_restricted_content",
    "acknowledge_remote_provider",
  ];
  const blocking = preparation.prerequisites.some((item) =>
    item.blocking && !acknowledgementActions.includes(item.action),
  );
  const needsRestricted = preparation.prerequisites.some((item) =>
    item.action === "acknowledge_restricted_content",
  );
  const needsRemote = preparation.prerequisites.some((item) =>
    item.action === "acknowledge_remote_provider",
  );
  const canStart = !blocking
    && !preparedNoChanges
    && !validationKey
    && (!preparation.requiresScopeConfirmation || scopeConfirmed)
    && (!needsRestricted || restricted)
    && (!needsRemote || remote);
  const outputLocation = scope.kind === "generate_content"
    ? scope.outputPath ?? (outputScopeChanged ? t("workflows.output.defaultPending") : preparation.output.location)
    : preparation.output.location;

  const prerequisiteActionLabel = (action: WorkflowPrerequisiteAction): string => {
    if (
      preparation.kind !== "health_check"
      && action === "configure_execution_route"
    ) {
      return t("workflows.action.openSettings");
    }
    return t("workflows.action.resolve");
  };

  const draft = (): WorkflowPreparationDraft => ({ scope, routeSelection });

  const handlePrerequisite = (action: WorkflowPrerequisiteAction): void => {
    if (action === "choose_execution_route" && visibleRoutes.length > 0) {
      if (executionDetailsRef.current) executionDetailsRef.current.open = true;
      routeSelectRef.current?.focus();
      return;
    }
    if (action === "configure_execution_route" || action === "import_sources") {
      pendingRouteChoiceRef.current = routeChoice;
      if (scopeChanged) onPrerequisite(action, draft());
      else onPrerequisite(action);
      return;
    }
    onPrerequisite(action);
  };

  return (
    <div className="workflow-preparation">
      <button className="workflow-back" onClick={onBack} type="button">
        <ArrowLeft aria-hidden="true" size={14} />
        {t("workflows.action.back")}
      </button>
      <div className="workflows-intro">
        <h2 data-workflow-surface-title tabIndex={-1}>{t("workflows.preparation.title", { workflow: t(workflowKindKey(preparation.kind)) })}</h2>
        <p>{t("workflows.preparation.description")}</p>
      </div>

      <ol className="workflow-decision-sequence">
        <li className="workflow-preparation-step" data-decision-step="1">
          <div className="workflow-preparation-step__label">{t("workflows.preparation.whatWillHappen")}</div>
          <p>{t(workflowKindDescriptionKey(preparation.kind))}</p>
        </li>
        <li className="workflow-preparation-step is-expanded" data-decision-step="2">
          <div className="workflow-preparation-step__label">{t("workflows.preparation.inputScope")}</div>
          <p>
            {isProjectReport
              && preparation.scope.kind === "generate_content"
              && preparation.scope.artifactType !== "project_report"
              ? t("workflows.preparation.fixedScopePending")
              : scope.kind === "health_check" || isProjectReport
              ? t("workflows.preparation.fixedScopeCount", { count: preparation.baseline.itemCount })
              : t("workflows.preparation.scopeCount", { selected: selectedCount, total: selectableCount })}
          </p>
          {preparedNoChanges ? (
            <p className="workflow-scope-state is-empty">{t("workflows.preparation.noChanges")}</p>
          ) : scope.kind === "update_wiki" && updateAutoDetect ? (
            <p className="workflow-scope-state">{t("workflows.preparation.autoDetectChanges")}</p>
          ) : scope.kind === "update_wiki" ? (
            <>
              <ScopeOptionToolbar
                filteredCount={filteredCount}
                onClear={clearSelection}
                onPageChange={setScopePage}
                onQueryChange={(value) => { setScopeQuery(value); setScopePage(0); }}
                onSelect={selectFiltered}
                page={boundedScopePage}
                pageCount={scopePageCount}
                query={scopeQuery}
                selectedCount={selectedCount}
                totalCount={selectableCount}
              />
              <div className="workflow-scope-items">
                {filteredSourceOptions.slice(scopePageStart, scopePageStart + MAX_VISIBLE_SCOPE_OPTIONS).map((source) => {
                  const key = sourceVersionKey(source);
                  const selected = selectedSourceKeys.has(key);
                  return (
                    <label key={key}>
                      <input
                        aria-label={`${source.sourceId}:${source.versionId}`}
                        checked={selected}
                        onChange={(event) => {
                          setUpdateAutoDetect(false);
                          setScope({
                            ...scope,
                            sourceVersions: event.target.checked
                              ? [...scope.sourceVersions, source]
                              : scope.sourceVersions.filter((item) => sourceVersionKey(item) !== key),
                          });
                        }}
                        type="checkbox"
                      />
                      <code>{source.sourceId}</code>
                      <span>{source.versionId}</span>
                    </label>
                  );
                })}
              </div>
            </>
          ) : scope.kind === "generate_content" && scope.artifactType === "project_report" ? (
            <p className="workflow-scope-state">{t("workflows.preparation.generate.project_report")}</p>
          ) : scope.kind === "generate_content" ? (
            <>
              <p className="workflow-scope-state">{t(`workflows.preparation.generate.${scope.artifactType}`)}</p>
              <ScopeOptionToolbar
                filteredCount={filteredCount}
                onClear={clearSelection}
                onPageChange={setScopePage}
                onQueryChange={(value) => { setScopeQuery(value); setScopePage(0); }}
                onSelect={selectFiltered}
                page={boundedScopePage}
                pageCount={scopePageCount}
                query={scopeQuery}
                selectedCount={selectedCount}
                totalCount={selectableCount}
              />
              <div className="workflow-scope-items">
                {filteredPageOptions.slice(scopePageStart, scopePageStart + MAX_VISIBLE_SCOPE_OPTIONS).map((path) => {
                  const selected = selectedPagePaths.has(path);
                  return (
                    <label key={path}>
                      <input
                        aria-label={path}
                        checked={selected}
                        onChange={(event) => setScope({
                          ...scope,
                          pagePaths: event.target.checked
                            ? scope.artifactType === "beautiful_read"
                              ? [path]
                              : [...scope.pagePaths, path]
                            : scope.pagePaths.filter((item) => item !== path),
                        })}
                        type="checkbox"
                      />
                      <code>{path}</code>
                    </label>
                  );
                })}
              </div>
            </>
          ) : null}
          {validationKey ? <p className="workflow-scope-state is-invalid" role="alert">{t(validationKey)}</p> : null}
        </li>
        <li className="workflow-preparation-step" data-decision-step="3">
          <div className="workflow-preparation-step__label">{t("workflows.preparation.output")}</div>
          <div className="workflow-preparation-step__value font-mono">
            {outputLocation ?? t("workflows.output.session")}
          </div>
          {scope.kind === "generate_content" ? (
            <label className="workflow-field">
              {t("workflows.preparation.outputPath")}
              <input
                onChange={(event) => setScope({ ...scope, outputPath: event.target.value || null })}
                type="text"
                value={scope.outputPath ?? ""}
              />
            </label>
          ) : null}
        </li>
        <li className="workflow-preparation-step" data-decision-step="4">
          <div className="workflow-preparation-step__label">{t("workflows.preparation.wikiWrite")}</div>
          <div className="workflow-preparation-step__value">
            {preparation.output.mayChangeWiki
              ? t("workflows.preparation.wikiWriteYes")
              : t("workflows.preparation.wikiWriteNo")}
          </div>
        </li>
        <li className="workflow-preparation-step" data-decision-step="5">
          <div className="workflow-preparation-step__label">{t("workflows.preparation.git")}</div>
          <div className="workflow-preparation-step__value">{t(`workflows.git.${preparation.gitPolicy}`)}</div>
        </li>
        <li className="workflow-preparation-step" data-decision-step="6">
          <div className="workflow-preparation-step__label">{t("workflows.preparation.route")}</div>
          <div className="workflow-preparation-step__value">
            {routeDisplay(preparation.route, t)}
          </div>
        </li>
        <li className="workflow-preparation-step is-expanded" data-decision-step="7">
          <div className="workflow-preparation-step__label">{t("workflows.preparation.structuredOptions")}</div>
          {scope.kind === "update_wiki" ? (
            <div className="workflow-option-row" role="radiogroup" aria-label={t("workflows.preparation.updateMode")}>
              <label>
                <input
                  checked={scope.mode === "changed_sources"}
                  name="update-mode"
                  onChange={() => {
                    const switchingFromFull = scope.mode === "full_recompile";
                    setUpdateAutoDetect(switchingFromFull);
                    setScope({
                      ...scope,
                      mode: "changed_sources",
                      sourceVersions: preparation.scope.kind === "update_wiki" && preparation.scope.mode === "changed_sources"
                        ? preparation.scope.sourceVersions
                        : [],
                    });
                  }}
                  type="radio"
                />
                {t("workflows.mode.changedSources")}
              </label>
              <label>
                <input
                  checked={scope.mode === "full_recompile"}
                  name="update-mode"
                  onChange={() => {
                    setUpdateAutoDetect(false);
                    setScope({ ...scope, mode: "full_recompile", sourceVersions: sourceOptions });
                  }}
                  type="radio"
                />
                {t("workflows.mode.fullRecompile")}
              </label>
            </div>
          ) : scope.kind === "health_check" ? (
            <div className="workflow-option-row" role="radiogroup" aria-label={t("workflows.preparation.healthMode")}>
              <label>
                <input checked={scope.mode === "local_quick"} name="health-mode" onChange={() => setScope({ ...scope, mode: "local_quick" })} type="radio" />
                {t("workflows.mode.localQuick")}
              </label>
              <label>
                <input checked={scope.mode === "complete"} name="health-mode" onChange={() => setScope({ ...scope, mode: "complete" })} type="radio" />
                {t("workflows.mode.complete")}
              </label>
            </div>
          ) : (
            <label className="workflow-field">
              {t("workflows.preparation.artifactType")}
              <select
                onChange={(event) => {
                  const artifactType = event.target.value as WorkflowArtifactType;
                  setScope({
                    ...scope,
                    artifactType,
                    pagePaths: artifactType === "project_report"
                      ? []
                      : artifactType === "beautiful_read"
                        ? scope.pagePaths.slice(0, 1)
                        : scope.pagePaths,
                  });
                  setScopePage(0);
                }}
                value={scope.artifactType}
              >
                <option value="beautiful_read">{t("workflows.artifact.beautifulRead")}</option>
                <option value="knowledge_card">{t("workflows.artifact.knowledgeCard")}</option>
                <option value="concept_map">{t("workflows.artifact.conceptMap")}</option>
                <option value="project_report">{t("workflows.artifact.projectReport")}</option>
              </select>
            </label>
          )}
        </li>
      </ol>

      <section className="workflow-prerequisites" aria-label={t("workflows.preparation.prerequisites")}>
        <h3>{t("workflows.preparation.prerequisites")}</h3>
        {preparation.prerequisites.length === 0 ? (
          <p className="workflow-check"><Check aria-hidden="true" size={14} />{t("workflows.preparation.ready")}</p>
        ) : preparation.prerequisites.map((item) => (
          <div className={item.blocking ? "workflow-prerequisite is-blocking" : "workflow-prerequisite"} key={item.code}>
            <ShieldAlert aria-hidden="true" size={14} />
            <span>{t(item.messageKey)}</span>
            {!acknowledgementActions.includes(item.action)
              && !(item.action === "choose_execution_route" && visibleRoutes.length === 0) ? (
              <button
                className="btn btn--ghost btn--sm ml-auto"
                disabled={prerequisitePending}
                onClick={() => handlePrerequisite(item.action)}
                type="button"
              >
                {item.action === "choose_execution_route"
                  ? t("workflows.action.chooseRoute")
                  : prerequisiteActionLabel(item.action)}
              </button>
            ) : null}
          </div>
        ))}
      </section>

      {preparation.requiresScopeConfirmation ? (
        <label className="workflow-confirm">
          <input checked={scopeConfirmed} onChange={(event) => setScopeConfirmed(event.target.checked)} type="checkbox" />
          {t("workflows.confirm.scope")}
        </label>
      ) : null}
      {needsRestricted ? (
        <label className="workflow-confirm">
          <input checked={restricted} onChange={(event) => setRestricted(event.target.checked)} type="checkbox" />
          {t("workflows.confirm.restricted")}
        </label>
      ) : null}
      {needsRemote ? (
        <label className="workflow-confirm">
          <input checked={remote} onChange={(event) => setRemote(event.target.checked)} type="checkbox" />
          {t("workflows.confirm.remote")}
        </label>
      ) : null}

      <details className="workflow-execution-details" ref={executionDetailsRef}>
        <summary>{t("workflows.preparation.executionDetails")}</summary>
        <dl>
          <dt>{t("workflows.preparation.baseline")}</dt>
          <dd className="font-mono">{preparation.baseline.fingerprint}</dd>
          <dt>{t("workflows.preparation.baselineCaptured")}</dt>
          <dd>{preparation.baseline.capturedAt}</dd>
          <dt>{t("workflows.preparation.route")}</dt>
          <dd>{routeDisplay(preparation.route, t)}</dd>
          {preparation.route?.kind === "agent" ? (
            <>
              <dt>{t("workflows.preparation.agent")}</dt>
              <dd>{preparation.route.agent}</dd>
              {preparation.route.model ? <><dt>{t("workflows.preparation.model")}</dt><dd>{preparation.route.model}</dd></> : null}
            </>
          ) : null}
          {preparation.route?.kind === "byok" ? (
            <>
              <dt>{t("workflows.preparation.provider")}</dt>
              <dd>{preparation.route.provider}</dd>
              <dt>{t("workflows.preparation.model")}</dt>
              <dd>{preparation.route.model}</dd>
            </>
          ) : null}
          {scope.kind === "generate_content" ? (
            <>
              <dt>{t("workflows.preparation.skillId")}</dt>
              <dd className="font-mono">{ARTIFACT_SKILL_IDS[scope.artifactType]}</dd>
            </>
          ) : null}
          <dt>{t("workflows.preparation.dataBoundary")}</dt>
          <dd>{t(`workflows.boundary.${preparation.scope.kind}`)}</dd>
        </dl>
        {visibleRoutes.length > 0 ? (
          <label className="workflow-field">
            {t("workflows.preparation.routeOverride")}
            <select onChange={(event) => setRouteChoice(event.target.value)} ref={routeSelectRef} value={routeChoice}>
              <option value="auto">{t("workflows.route.auto")}</option>
              {visibleRoutes.map((route) => {
                const key = workflowRouteSelectionKey(route);
                return (
                  <option key={key} value={key}>
                    {route.kind === "agent"
                      ? `${t("workflows.route.agent")} · ${route.agent}`
                      : `${t("workflows.route.byok")} · ${route.provider}`}
                  </option>
                );
              })}
            </select>
          </label>
        ) : null}
      </details>

      <div className="workflow-preparation-update">
        <button
          className="btn btn--secondary btn--sm"
          disabled={!scopeChanged || Boolean(validationKey) || preparePending || startPending}
          onClick={() => {
            pendingRouteChoiceRef.current = routeChoice;
            onReprepare(scope, routeSelection);
          }}
          type="button"
        >
          {preparePending ? t("workflows.action.updatingPreparation") : t("workflows.action.updatePreparation")}
        </button>
      </div>

      <div className="workflow-actions" data-decision-step="8">
        <button
          aria-busy={startPending}
          className="btn btn--primary"
          disabled={!canStart || scopeChanged || startPending || preparePending}
          onClick={() => onStart(restricted, remote)}
          type="button"
        >
          {startPending
            ? t("workflows.action.starting")
            : preparation.quickRerunEligible
              ? t("workflows.action.runAgain")
              : t("workflows.action.start")}
        </button>
      </div>
    </div>
  );
}

function ScopeOptionToolbar({
  query,
  onQueryChange,
  onSelect,
  onClear,
  selectedCount,
  filteredCount,
  totalCount,
  page,
  pageCount,
  onPageChange,
}: {
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
  return (
    <div className="workflow-scope-toolbar">
      <label className="workflow-field">
        {t("workflows.preparation.searchOptions")}
        <input onChange={(event) => onQueryChange(event.target.value)} type="search" value={query} />
      </label>
      <div className="workflow-scope-toolbar__actions">
        <span aria-live="polite">
          {t("workflows.preparation.selectionCount", { selected: selectedCount, filtered: filteredCount, total: totalCount })}
        </span>
        <button className="btn btn--ghost btn--sm" disabled={filteredCount === 0} onClick={onSelect} type="button">
          {t("workflows.preparation.selectResults")}
        </button>
        <button className="btn btn--ghost btn--sm" disabled={selectedCount === 0} onClick={onClear} type="button">
          {t("workflows.preparation.clearSelection")}
        </button>
        {pageCount > 1 ? (
          <>
            <button className="btn btn--ghost btn--sm" disabled={page === 0} onClick={() => onPageChange(page - 1)} type="button">
              {t("workflows.preparation.previousOptions")}
            </button>
            <span>{t("workflows.preparation.optionPage", { current: page + 1, total: pageCount })}</span>
            <button className="btn btn--ghost btn--sm" disabled={page + 1 >= pageCount} onClick={() => onPageChange(page + 1)} type="button">
              {t("workflows.preparation.nextOptions")}
            </button>
          </>
        ) : null}
      </div>
    </div>
  );
}
