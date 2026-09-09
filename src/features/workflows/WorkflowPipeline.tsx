import { Check, Circle, Clock3, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import type { WorkflowDisplayStatus, WorkflowKind, WorkflowStage } from "../../types/workflow";
import { workflowDurationMs, workflowStageStatusClass } from "./workflowPresentation";

export function WorkflowPipeline({
  stages,
  kind,
  currentStageId = null,
  displayStatus = "running",
}: {
  stages: WorkflowStage[];
  kind?: WorkflowKind;
  currentStageId?: string | null;
  displayStatus?: WorkflowDisplayStatus;
}) {
  const { t, i18n } = useTranslation();
  const [technicalStagesOpen, setTechnicalStagesOpen] = useState(false);
  const language = i18n?.resolvedLanguage ?? i18n?.language ?? "en";
  const currentStage = stages.find((stage) => stage.id === currentStageId)
    ?? stages.find((stage) => stage.status === "running" || stage.status === "waiting" || stage.status === "failed")
    ?? null;
  const overallValue = displayStatus === "completed" ? stages.length : null;
  const overallValueText = currentStage
    ? t("workflows.pipeline.overallValue", {
        current: currentStage.ordinal,
        total: stages.length,
        stage: t(currentStage.labelKey),
      })
    : t("workflows.pipeline.overallIdle", { total: stages.length });

  if (kind) {
    const groupsByKind = {
      update_wiki: [
        { labelKey: "workflows.updatePhase.prepare", ids: ["analyze_sources", "create_checkpoint"] },
        { labelKey: "workflows.updatePhase.generate", ids: ["plan_updates", "generate_candidates"] },
        { labelKey: "workflows.updatePhase.review", ids: ["validate_structure", "review_risk"] },
        { labelKey: "workflows.updatePhase.save", ids: ["apply_changes", "refresh_indexes", "record_result"] },
      ],
      health_check: [
        { labelKey: "workflows.healthPhase.read", ids: ["read_markdown"] },
        { labelKey: "workflows.healthPhase.local", ids: ["check_markdown", "check_links"] },
        { labelKey: "workflows.healthPhase.deep", ids: ["deep_check"] },
        { labelKey: "workflows.healthPhase.report", ids: ["merge_findings", "classify_findings", "write_report", "complete"] },
      ],
      generate_content: [
        { labelKey: "workflows.exportPhase.prepare", ids: ["confirm_scope", "read_wiki", "load_template"] },
        { labelKey: "workflows.exportPhase.generate", ids: ["generate_content", "assemble_artifact"] },
        { labelKey: "workflows.exportPhase.save", ids: ["validate_artifact", "write_export"] },
        { labelKey: "workflows.exportPhase.result", ids: ["generate_preview", "complete"] },
      ],
    };
    const groups = groupsByKind[kind];
    return <div className="workflow-pipeline-shell workflow-grouped-pipeline">
      <ol className="workflow-pipeline">
        {groups.map((group, index) => {
          const members = stages.filter((stage) => group.ids.includes(stage.id));
          const active = members.find((stage) => ["running", "waiting", "failed"].includes(stage.status));
          const status = active?.status ?? (members.length > 0 && members.every((stage) => stage.status === "skipped")
            ? "skipped"
            : members.length > 0 && members.every((stage) => ["completed", "skipped"].includes(stage.status)) ? "completed" : "pending");
          return <li key={group.labelKey} className={workflowStageStatusClass(status)}>
            <span className="workflow-phase-node" aria-hidden="true">
              {status === "completed" ? <Check size={13} /> : status === "failed" ? <X size={13} /> : index + 1}
            </span>
            <div className="workflow-pipeline__heading" aria-current={active ? "step" : undefined}>
              <span>{t(group.labelKey)}</span><span>{t(`workflows.stageStatus.${status}`)}</span>
            </div>
            {active ? <div className="workflow-pipeline__body">
              {active.currentItem ? <code>{active.currentItem}</code> : null}
              {active.progress ? <span>{active.progress.total !== null
                ? t("workflows.progress.count", { current: active.progress.current, total: active.progress.total })
                : t("workflows.progress.current", { count: active.progress.current })}</span> : null}
              {status === "running" ? <progress aria-label={t(group.labelKey)}
                {...(active.progress?.total != null ? { max: Math.max(active.progress.total, 1), value: active.progress.current } : {})} /> : null}
            </div> : null}
          </li>;
        })}
      </ol>
      <details className="workflow-execution-details" onToggle={(event) => setTechnicalStagesOpen(event.currentTarget.open)}><summary>{t("workflows.pipeline.technicalStages")}</summary>
        {technicalStagesOpen ? <WorkflowPipeline stages={stages} currentStageId={currentStageId} displayStatus={displayStatus} /> : null}
      </details>
    </div>;
  }
  return (
    <div className={`workflow-pipeline-wrap is-${displayStatus.replaceAll("_", "-")}`}>
      <div className="workflow-pipeline-overall">
        <div className="workflow-pipeline-overall__copy">
          <span>{t("workflows.pipeline.overallProgress")}</span>
          <span>{overallValueText}</span>
        </div>
        <progress
          aria-label={t("workflows.pipeline.overallProgress")}
          aria-valuetext={overallValueText}
          max={Math.max(stages.length, 1)}
          {...(overallValue === null ? {} : { value: overallValue })}
        />
      </div>
      <ol className="workflow-pipeline">
        {stages.map((stage) => {
          const Icon = stage.status === "completed"
            ? Check
            : stage.status === "failed"
              ? X
              : stage.status === "waiting"
                ? Clock3
                : Circle;
          const expanded = stage.id === currentStage?.id
            || stage.status === "running"
            || stage.status === "waiting"
            || stage.status === "failed";
          const duration = workflowDurationMs(stage.startedAt, stage.completedAt);
          return (
            <li className={workflowStageStatusClass(stage.status)} key={stage.id}>
              <details data-stage-status={stage.status} open={expanded}>
                <summary aria-current={stage.id === currentStage?.id ? "step" : undefined}>
                  <span className="workflow-pipeline__marker"><Icon aria-hidden="true" size={12} /></span>
                  <span className="workflow-pipeline__heading">
                    <span className="font-medium">{t(stage.labelKey)}</span>
                    <span className="workflow-pipeline__meta">
                      {duration !== null ? <span>{formatDuration(duration, language, t)}</span> : null}
                      <span>{t(`workflows.stageStatus.${stage.status}`)}</span>
                    </span>
                  </span>
                </summary>
                <div className="workflow-pipeline__body">
                  {stage.status === "waiting" || stage.decision ? (
                    <p className="workflow-pipeline__decision"><Clock3 aria-hidden="true" size={13} />{t("workflows.pipeline.decisionNode")}</p>
                  ) : null}
                  {stage.currentItem ? (
                    <p className="workflow-pipeline__current-item" title={stage.currentItem}>
                      <span>{t("workflows.pipeline.currentItem")}</span>
                      <code title={stage.currentItem}>{stage.currentItem}</code>
                    </p>
                  ) : null}
                  {stage.progress ? stage.progress.total === null ? (
                    <span aria-label={t("workflows.progress.current", { count: stage.progress.current })} className="workflow-progress-count">
                      {t("workflows.progress.current", { count: stage.progress.current })}
                    </span>
                  ) : (
                    <div className="workflow-stage-progress">
                      <span>{t("workflows.progress.count", { current: stage.progress.current, total: stage.progress.total })}</span>
                      <progress aria-label={t(stage.labelKey)} max={Math.max(stage.progress.total, 1)} value={stage.progress.current} />
                    </div>
                  ) : null}
                </div>
              </details>
            </li>
          );
        })}
      </ol>
    </div>
  );
}

function formatDuration(
  milliseconds: number,
  language: string,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  const seconds = Math.max(0, Math.round(milliseconds / 1_000));
  const formatter = new Intl.NumberFormat(language);
  if (seconds < 60) return t("workflows.duration.seconds", { count: formatter.format(seconds) });
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return remainingSeconds === 0
    ? t("workflows.duration.minutes", { count: formatter.format(minutes) })
    : t("workflows.duration.minutesSeconds", {
        minutes: formatter.format(minutes),
        seconds: formatter.format(remainingSeconds),
      });
}
