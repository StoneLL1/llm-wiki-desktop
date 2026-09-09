import { ArrowLeft, Bot, Zap, BookOpen, FileChartColumn, FileOutput, Network, PanelsTopLeft, Play, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { getWorkflowFormCatalog } from "../../services/workflowApi";
import { useWorkflowStore, workflowOperationPending, captureWorkflowRequestGuard, workflowRequestGuardMatches } from "../../stores/workflowStore";
import { useNavigationStore } from "../../stores/navigationStore";
import type { ProjectSummary } from "../../types/project";
import type { WorkflowFormCatalog, WorkflowPreparationDraft, WorkflowHealthContextSummary, WorkflowScope, WorkflowRouteSelection } from "../../types/workflow";

import { WorkflowChoice, WorkflowPageLabel, WorkflowSearchField, workflowRouteLabel } from "./WorkflowFormControls";

import { WorkflowOutputPathPicker } from "./WorkflowOutputPathPicker";

export type DraftWorkflowKind = "health_check" | "generate_content";
const routeKey = (route: WorkflowRouteSelection) => route.kind === "agent" ? `agent:${route.agent}` : `byok:${route.provider}`;
const artifacts = [
  { type: "beautiful_read", label: "beautifulRead", icon: BookOpen },
  { type: "knowledge_card", label: "knowledgeCard", icon: PanelsTopLeft },
  { type: "concept_map", label: "conceptMap", icon: Network },
  { type: "project_report", label: "projectReport", icon: FileChartColumn },
] as const;
const PAGE_SIZE = 100;
export function defaultWorkflowDraft(kind: DraftWorkflowKind): WorkflowPreparationDraft {
  return { scope: kind === "health_check" ? { kind, mode: "local_quick" }
    : { kind, artifactType: "beautiful_read", pagePaths: [], outputPath: null }, routeSelection: null };
}

/** Choices exist before discovery; only Start requests a validated preparation. */
export function WorkflowDraftForm({ kind, project, onStart, onBack, lastHealth, onOpenLastHealth }: {
  kind: DraftWorkflowKind;
  project: Pick<ProjectSummary, "projectId" | "rootPath">;
  onStart: (draft: WorkflowPreparationDraft) => Promise<void>;
  onBack: () => void;
  lastHealth?: WorkflowHealthContextSummary | null;
  onOpenLastHealth: (taskId: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const saved = useWorkflowStore((state) => state.drafts[kind]);
  const draft = saved ?? defaultWorkflowDraft(kind);
  const { scope } = draft;
  const starting = useWorkflowStore((state) => workflowOperationPending(state.operations, `draft:start:${kind}`));
  const epoch = useWorkflowStore((state) => state.requestEpoch);
  const owner = useWorkflowStore((state) => `${state.identityGuard.canonicalIdentityKey}\0${state.identityGuard.identityRevision}`);
  const settingsOpen = useNavigationStore((state) => state.settingsOpen);
  const [catalog, setCatalog] = useState<WorkflowFormCatalog | null>(null);
  const [catalogError, setCatalogError] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(0);
  const setDraft = (value: WorkflowPreparationDraft) => useWorkflowStore.getState().setDraft(kind, value);
  const setScope = (value: WorkflowScope) => setDraft({ scope: value, routeSelection: draft.routeSelection });
  useEffect(() => {
    if (settingsOpen) return;
    let active = true;
    const guard = captureWorkflowRequestGuard();
    const current = () => active && workflowRequestGuardMatches(guard);
    setCatalogError(false);
    void getWorkflowFormCatalog({ projectId: project.projectId, projectRootPath: project.rootPath, kind }).then((value) => {
      if (!current()) return;
      setCatalog(value);
      // Late preferences never replace a choice made while this query was pending.
      if (!useWorkflowStore.getState().drafts[kind] && value.rememberedDraft?.scope.kind === kind) {
        useWorkflowStore.getState().setDraft(kind, value.rememberedDraft);
      }
    }).catch(() => { if (current()) setCatalogError(true); });
    return () => { active = false; };
  }, [kind, project.projectId, project.rootPath, refresh, settingsOpen, epoch, owner]);
  const pages = useMemo(() => (catalog?.wikiPages ?? []).filter((path) => path.toLocaleLowerCase().includes(query.toLocaleLowerCase())), [catalog, query]);
  const pageCount = Math.max(1, Math.ceil(pages.length / PAGE_SIZE));
  const boundedPage = Math.min(page, pageCount - 1);
  const local = scope.kind === "health_check" && scope.mode === "local_quick";
  const projectReport = scope.kind === "generate_content" && scope.artifactType === "project_report";
  const invalid = scope.kind === "generate_content" && (
    (scope.outputPath !== null && !scope.outputPath.trim())
    || (!projectReport && (scope.pagePaths.length === 0 || (scope.artifactType === "beautiful_read" && scope.pagePaths.length !== 1)))
  );
  const Icon = kind === "health_check" ? ShieldCheck : FileOutput;
  return <div className="workflow-preparation">
    <div className="workflow-panel-heading">
      <Icon size={17} aria-hidden="true" /><h2 data-workflow-surface-title tabIndex={-1}>{t(`workflows.kind.${kind}`)}</h2>
      <button type="button" className="workflow-back" onClick={onBack}><ArrowLeft size={14} aria-hidden="true" />{t("workflows.action.back")}</button>
    </div>
    <div className="workflow-panel-body">
      <fieldset className="workflow-preparation-controls" disabled={starting}>
        {scope.kind === "health_check" && <>
          <h3 className="workflow-form-label">{t("workflows.preparation.healthMode")}</h3>
          <div className="workflow-mode-options" role="radiogroup" aria-label={t("workflows.preparation.healthMode")}>
            <WorkflowChoice name="health-mode" label={t("workflows.mode.localQuick")} description={t("workflows.design.localHint")} icon={Zap} checked={local} onChange={() => setScope({ ...scope, mode: "local_quick" })} />
            <WorkflowChoice name="health-mode" label={t("workflows.mode.complete")} description={t("workflows.design.completeHint")} icon={Bot} checked={!local} onChange={() => setScope({ ...scope, mode: "complete" })} />
          </div>
          <p className="workflow-scope-state">{t("workflows.health.currentAtStart")}</p>
        </>}
        {scope.kind === "generate_content" && <>
          <h3 className="workflow-form-label">{t("workflows.preparation.artifactType")}</h3>
          <div className="workflow-artifact-options" role="radiogroup" aria-label={t("workflows.preparation.artifactType")}>
            {artifacts.map(({ type, label, icon }) => <WorkflowChoice key={type} name="artifact" label={t(`workflows.artifact.${label}`)} description={t(`workflows.preparation.generate.${type}`)} icon={icon} checked={scope.artifactType === type} onChange={() => {
              setScope({ ...scope, artifactType: type, pagePaths: type === "project_report" ? [] : type === "beautiful_read" ? scope.pagePaths.slice(0, 1) : scope.pagePaths }); setPage(0);
            }} />)}
          </div>
          <div className="workflow-scope-toolbar">
            <h3 className="workflow-form-label">{t("workflows.preparation.inputScope")}</h3>
            {projectReport ? <p className="workflow-scope-state">{t("workflows.draft.wholeWiki")}</p> : <>
              <WorkflowSearchField aria-label={t("workflows.draft.findPages")} placeholder={t("workflows.draft.findPages")} value={query} onChange={(event) => { setQuery(event.target.value); setPage(0); }} />
              <div className="workflow-scope-toolbar__actions">
                <span>{t("workflows.preparation.scopeCount", { selected: scope.pagePaths.length, total: catalog?.wikiPages.length ?? 0 })}</span>
                {scope.artifactType !== "beautiful_read" && <button type="button" className="btn btn--ghost btn--sm" disabled={!catalog || pages.length === 0} onClick={() => setScope({ ...scope, pagePaths: [...new Set([...scope.pagePaths, ...pages])] })}>{t("workflows.draft.selectFiltered")}</button>}
                <button type="button" className="btn btn--ghost btn--sm" disabled={scope.pagePaths.length === 0} onClick={() => setScope({ ...scope, pagePaths: [] })}>{t("workflows.update.clear")}</button>
              </div>
              {!catalog && !catalogError && <p role="status" className="workflow-scope-state">{t("workflows.draft.loadingPages")}</p>}
              <div className="workflow-scope-items">{pages.slice(boundedPage * PAGE_SIZE, (boundedPage + 1) * PAGE_SIZE).map((path) => <label key={path}>
                <input type={scope.artifactType === "beautiful_read" ? "radio" : "checkbox"} name={scope.artifactType === "beautiful_read" ? "reading-page" : undefined} aria-label={path} checked={scope.pagePaths.includes(path)} onChange={(event) => setScope({ ...scope, pagePaths: event.target.checked
                  ? scope.artifactType === "beautiful_read" ? [path] : [...scope.pagePaths, path]
                  : scope.pagePaths.filter((selected) => selected !== path) })} /><WorkflowPageLabel path={path} />
              </label>)}</div>
              {catalog && pages.length === 0 && <p className="workflow-scope-state">{t("workflows.draft.noPages")}</p>}
              {scope.pagePaths.filter((path) => catalog && !catalog.wikiPages.includes(path)).map((path) => <p className="workflow-scope-state" key={path}>{t("workflows.draft.missingPage", { path })}</p>)}
              {pageCount > 1 && <div className="workflow-option-row">
                <button type="button" className="btn btn--ghost btn--sm" disabled={boundedPage === 0} onClick={() => setPage(boundedPage - 1)}>{t("workflows.update.previous")}</button>
                <button type="button" className="btn btn--ghost btn--sm" disabled={boundedPage + 1 >= pageCount} onClick={() => setPage(boundedPage + 1)}>{t("workflows.update.next")}</button>
              </div>}
            </>}
          </div>
          <section className="workflow-save-section">
            <h3 className="workflow-form-label">{t("workflows.preparation.saveMode")}</h3>
            <div className="workflow-segmented" role="radiogroup" aria-label={t("workflows.preparation.saveMode")}>
              <label><input type="radio" name="save-mode" checked={scope.outputPath === null} onChange={() => setScope({ ...scope, outputPath: null })} />{t("workflows.preparation.createArtifact")}</label>
              <label><input type="radio" name="save-mode" checked={scope.outputPath !== null} onChange={() => setScope({ ...scope, outputPath: scope.outputPath ?? "" })} />{t("workflows.preparation.explicitTarget")}</label>
            </div>
            <p className="workflow-scope-state">{t(scope.outputPath === null ? "workflows.preparation.createArtifactHint" : "workflows.preparation.explicitTargetHint")}</p>
            {scope.outputPath !== null && <WorkflowOutputPathPicker key={scope.artifactType} project={project} value={scope.outputPath} onChange={(outputPath) => setScope({ ...scope, outputPath })} />}
          </section>
        </>}
        {!local && <details className="workflow-execution-details">
          <summary><span>{t("workflows.preparation.executionDetails")}</span><span className="workflow-disclosure-value">{(draft.routeSelection ?? catalog?.defaultRoute) ? workflowRouteLabel((draft.routeSelection ?? catalog?.defaultRoute)!, t) : t("workflows.route.auto")}</span></summary>
          <label className="workflow-field">{t("workflows.preparation.routeOverride")}
            <select value={draft.routeSelection ? routeKey(draft.routeSelection) : "auto"} onChange={(event) => setDraft({ scope, routeSelection: catalog?.routes.find((route) => routeKey(route) === event.target.value) ?? null })}>
              <option value="auto">{t("workflows.route.auto")}{catalog?.defaultRoute ? ` · ${workflowRouteLabel(catalog.defaultRoute, t)}` : ""}</option>
              {catalog?.routes.map((route) => <option value={routeKey(route)} key={routeKey(route)}>{workflowRouteLabel(route, t)}</option>)}
            </select>
          </label>
          <button type="button" className="btn btn--secondary btn--sm" onClick={() => useNavigationStore.getState().openSettings("ai")}>{t("workflows.action.openSettings")}</button>
        </details>}
        {catalogError && <p role="status">{t("workflows.draft.catalogError")} <button type="button" className="btn btn--secondary btn--sm" onClick={() => setRefresh((value) => value + 1)}>{t("workflows.action.retry")}</button></p>}
        <div className="workflow-start-bar"><span className="workflow-start-summary" aria-live="polite">{invalid ? t(scope.kind === "generate_content" && scope.outputPath !== null && !scope.outputPath.trim() ? "workflows.preparation.invalid.outputPathEmpty" : scope.kind === "generate_content" && scope.artifactType === "beautiful_read" ? "workflows.preparation.invalid.beautifulReadScope" : "workflows.preparation.invalid.generatedScopeEmpty") : t(`workflows.boundary.${kind}`)}</span>
          <button type="button" className="btn btn--primary" aria-busy={starting} disabled={starting || invalid} onClick={() => void onStart({ scope, routeSelection: draft.routeSelection })}><Play size={14} aria-hidden="true" />{t(starting ? "workflows.action.starting" : "workflows.action.start")}</button>
        </div>
      </fieldset>
      {kind === "health_check" && lastHealth && <section className="workflow-last-report">
        <h3>{t("workflows.context.lastHealth")}</h3>
        <p><time dateTime={lastHealth.completedAt}>{new Date(lastHealth.completedAt).toLocaleString(i18n.resolvedLanguage)}</time> · {t("workflows.context.healthSummary", { errors: lastHealth.errorCount, warnings: lastHealth.warningCount, info: lastHealth.infoCount })}</p>
        <button type="button" className="btn btn--secondary btn--sm" onClick={() => onOpenLastHealth(lastHealth.taskId)}>{t("workflows.action.openResult")}</button>
      </section>}
    </div>
  </div>;
}
