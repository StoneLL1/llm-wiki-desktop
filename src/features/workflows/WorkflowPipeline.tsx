import { Check, Circle, Clock3, X } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { WorkflowStage } from "../../types/workflow";
import { workflowStageStatusClass } from "./workflowPresentation";

export function WorkflowPipeline({ stages }: { stages: WorkflowStage[] }) {
  const { t } = useTranslation();
  return <ol className="workflow-pipeline">
    {stages.map((stage) => {
      const Icon = stage.status === "completed" ? Check : stage.status === "failed" ? X : stage.status === "waiting" ? Clock3 : Circle;
      return <li className={workflowStageStatusClass(stage.status)} key={stage.id}>
        <span className="workflow-pipeline__marker"><Icon aria-hidden="true" size={12} /></span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-3"><span className="font-medium">{t(stage.labelKey)}</span><span className="text-[11px] text-[var(--text-muted)]">{t(`workflows.stageStatus.${stage.status}`)}</span></div>
          {stage.currentItem ? <p className="m-0 mt-1 truncate font-mono text-[11px] text-[var(--text-muted)]">{stage.currentItem}</p> : null}
          {stage.progress ? stage.progress.total === null ? <span aria-label={t("workflows.progress.current", { count: stage.progress.current })} className="workflow-progress-count">{t("workflows.progress.current", { count: stage.progress.current })}</span> : <progress aria-label={t(stage.labelKey)} max={Math.max(stage.progress.total, 1)} value={stage.progress.current} /> : null}
        </div>
      </li>;
    })}
  </ol>;
}
