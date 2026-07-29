import { Check, ChevronRight, CircleAlert, LoaderCircle, Sparkles, Wrench } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { TaskActivity, TaskStatus } from "../../types/task";

interface AgentActivityTimelineProps {
  activities: TaskActivity[];
  compact?: boolean;
  taskStatus?: TaskStatus;
}

interface ActivityRow {
  activity: TaskActivity;
  result?: Extract<TaskActivity, { kind: "tool_result" }>;
}

export function AgentActivityTimeline({ activities, compact = false, taskStatus }: AgentActivityTimelineProps) {
  const { t } = useTranslation();
  if (activities.length === 0) return null;
  const runSettled = taskStatus === "succeeded" || taskStatus === "failed" || taskStatus === "cancelled";
  const runFailed = taskStatus === "failed" || taskStatus === "cancelled";

  const rows: ActivityRow[] = [];
  const pairedActivityIndexes = new Set<number>();
  for (let index = 0; index < activities.length; index += 1) {
    if (pairedActivityIndexes.has(index)) continue;
    const activity = activities[index];
    if (activity.kind === "thinking" && activity.status === "started") {
      const completedIndex = activities.findIndex(
        (candidate, candidateIndex) =>
          candidateIndex > index &&
          candidate.kind === "thinking" &&
          candidate.status !== "started",
      );
      const completed =
        completedIndex >= 0 ? activities[completedIndex] : undefined;
      rows.push({ activity: completed ?? activity });
      if (completedIndex >= 0) pairedActivityIndexes.add(completedIndex);
      continue;
    }
    if (activity.kind === "phase" && activity.status === "started") {
      const completedIndex = activities.findIndex(
        (candidate, candidateIndex) =>
          candidateIndex > index &&
          candidate.kind === "phase" &&
          candidate.name === activity.name &&
          candidate.status !== "started",
      );
      const completed =
        completedIndex >= 0 ? activities[completedIndex] : undefined;
      rows.push({ activity: completed ?? activity });
      if (completedIndex >= 0) pairedActivityIndexes.add(completedIndex);
      continue;
    }
    if (activity.kind === "tool_result") continue;
    if (activity.kind === "tool_call") {
      rows.push({
        activity,
        result: activities.find(
          (candidate): candidate is Extract<TaskActivity, { kind: "tool_result" }> =>
            candidate.kind === "tool_result" && candidate.callId === activity.callId,
        ),
      });
      continue;
    }
    rows.push({ activity });
  }

  return (
    <div className={`agent-activity-timeline${compact ? " agent-activity-timeline--compact" : ""}`}>
      {rows.map((row, index) => {
        const { activity, result } = row;
        if (activity.kind === "thinking") {
          const failed = activity.status === "failed" || (runFailed && activity.status === "started");
          const done = activity.status === "completed" || (runSettled && !failed);
          const thinkingLabel = activity.durationMs && done
            ? t("agent.activity.thinkingDoneDuration", { seconds: Math.max(1, Math.round(activity.durationMs / 1000)) })
            : failed
              ? t("agent.activity.thinkingFailed")
              : done
                ? t("agent.activity.thinkingDone")
                : t("agent.activity.thinkingActive");
          return (
            <div className={`agent-activity-row agent-activity-row--thinking${failed ? " agent-activity-row--failed" : ""}`} key={`thinking-${index}`}>
              <span className={`agent-activity-icon agent-activity-icon--thinking${failed ? " agent-activity-icon--failed" : ""}`} aria-hidden="true">
                {failed ? <CircleAlert size={14} /> : done ? <Sparkles size={14} /> : <LoaderCircle className="animate-spin" size={14} />}
              </span>
              <span className="agent-activity-label">
                {thinkingLabel}
              </span>
              {done ? <span className="agent-activity-chevron">›</span> : null}
            </div>
          );
        }
        if (activity.kind === "tool_call") {
          const failed = result?.success === false || (!result && runFailed);
          const complete = Boolean(result) || runSettled;
          const detail = activity.detail === "controlled-command"
            ? t("agent.activity.controlledCommand")
            : activity.detail;
          return (
            <details className="agent-activity-details" key={`${activity.callId}-${index}`} open={!complete}>
              <summary className="agent-activity-row agent-activity-row--tool">
                <span className={`agent-activity-icon ${failed ? "agent-activity-icon--failed" : "agent-activity-icon--tool"}`} aria-hidden="true">
                  {failed ? <CircleAlert size={14} /> : complete ? <Check size={14} /> : <Wrench size={14} />}
                </span>
                <span className="agent-activity-tool-name">{activity.name}</span>
                {detail ? <span className="agent-activity-detail">{detail}</span> : null}
                {!complete ? <LoaderCircle className="agent-activity-spinner animate-spin" size={12} /> : null}
                <ChevronRight className="agent-activity-chevron agent-activity-chevron--details" size={14} aria-hidden="true" />
              </summary>
              {result || runSettled ? (
                <div className={`agent-activity-result${failed ? " agent-activity-result--failed" : ""}`}>
                  {failed ? t("agent.activity.toolFailed") : t("agent.activity.toolDone")}
                </div>
              ) : null}
            </details>
          );
        }
        if (activity.kind === "phase") {
          const failed = activity.status === "failed" || (runFailed && activity.status === "started");
          const complete = activity.status === "completed" || (runSettled && !failed);
          const phaseKey = {
            retrieval: "agent.activity.phase.retrieval",
            generation: "agent.activity.phase.generation",
            agent: "agent.activity.phase.agent",
            "import-agent": "agent.activity.phase.importAgent",
            "source-ai-organize": "agent.activity.phase.sourceAiOrganize",
            "source-ai-provider": "agent.activity.phase.sourceAiProvider",
          }[activity.name];
          return (
            <div className={`agent-activity-row agent-activity-row--phase${failed ? " agent-activity-row--failed" : ""}`} key={`phase-${activity.name}-${index}`}>
              <span className={`agent-activity-icon agent-activity-icon--phase${failed ? " agent-activity-icon--failed" : ""}`} aria-hidden="true">
                {failed ? <CircleAlert size={13} /> : complete ? <Check size={13} /> : <LoaderCircle className="animate-spin" size={13} />}
              </span>
              <span className="agent-activity-label">{phaseKey ? t(phaseKey) : activity.label ?? activity.name}</span>
              <span className="agent-activity-status">{failed ? t("agent.activity.failed") : complete ? t("agent.activity.done") : t("agent.activity.active")}</span>
            </div>
          );
        }
        return null;
      })}
    </div>
  );
}
