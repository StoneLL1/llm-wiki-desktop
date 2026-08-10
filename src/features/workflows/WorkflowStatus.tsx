import {
  AlertTriangle,
  Ban,
  CheckCircle2,
  CircleDashed,
  Clock3,
  LoaderCircle,
  XCircle,
} from "lucide-react";
import { useTranslation } from "react-i18next";

import type { WorkflowDisplayStatus, WorkflowOverviewState } from "../../types/workflow";
import { workflowStatusKey } from "./workflowPresentation";

type WorkflowStatusValue = WorkflowDisplayStatus | WorkflowOverviewState;

const icons = {
  ready: CheckCircle2,
  needs_prerequisite: AlertTriangle,
  queued: Clock3,
  running: LoaderCircle,
  waiting_for_confirmation: AlertTriangle,
  completed: CheckCircle2,
  failed: XCircle,
  cancelled: Ban,
  interrupted: CircleDashed,
  up_to_date: CheckCircle2,
} satisfies Record<WorkflowStatusValue, typeof CheckCircle2>;

export function WorkflowStatus({
  status,
  className,
}: {
  status: WorkflowStatusValue;
  className?: string;
}) {
  const { t } = useTranslation();
  const Icon = icons[status];
  return (
    <span
      className={`workflow-status is-${status.replaceAll("_", "-")}${className ? ` ${className}` : ""}`}
      data-workflow-status={status}
    >
      <Icon
        aria-hidden="true"
        className={status === "running" ? "animate-spin" : undefined}
        size={12}
      />
      <span>{t(workflowStatusKey(status))}</span>
    </span>
  );
}
