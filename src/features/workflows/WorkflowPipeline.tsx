import { Check, Circle, Clock3, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { WorkflowDisplayStatus, WorkflowStage } from "../../types/workflow";
import { workflowDurationMs, workflowStageStatusClass } from "./workflowPresentation";

export function WorkflowPipeline({
  stages,
  currentStageId = null,
  displayStatus = "running",
}: {
  stages: WorkflowStage[];
  currentStageId?: string | null;
  displayStatus?: WorkflowDisplayStatus;
}) {
  const { t, i18n } = useTranslation();
  const language = i18n?.resolvedLanguage ?? i18n?.language ?? "en";
  const currentStage = stages.find((stage) => stage.id === currentStageId)
    ?? stages.find((stage) => stage.status === "running" || stage.status === "waiting" || stage.status === "failed")
    ?? null;
  const completedStages = stages.filter((stage) => stage.status === "completed" || stage.status === "skipped").length;
  const measurableProgress = currentStage?.progress?.total !== null && currentStage?.progress?.total !== undefined
    ? Math.min(Math.max(currentStage.progress.current / Math.max(currentStage.progress.total, 1), 0), 1)
    : null;
  const overallValue = displayStatus === "completed"
    ? stages.length
    : measurableProgress === null
      ? null
      : Math.min(completedStages + measurableProgress, stages.length);
  const overallValueText = currentStage
    ? t("workflows.pipeline.overallValue", {
        current: currentStage.ordinal,
        total: stages.length,
        stage: t(currentStage.labelKey),
      })
    : t("workflows.pipeline.overallIdle", { total: stages.length });

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
