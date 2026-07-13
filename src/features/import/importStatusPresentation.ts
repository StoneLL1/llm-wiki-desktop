import type { AgentRecoveryAction } from "../../types/importV2Agent";
import type { ImportItem, ImportItemStatus } from "../../types/importV2";

export type ImportItemTone = "neutral" | "accent" | "warning" | "danger";
export type ImportItemProgressMode = "none" | "indeterminate" | "measured";
export type ImportItemIcon = "queue" | "scan" | "capability" | "login" | "shield" | "ready" | "merge" | "commit" | "completed" | "pause" | "cancelled" | "skipped" | "failed";
export type ImportItemAction =
  | "inspect"
  | "start"
  | "retry"
  | "cancel"
  | "preview_markdown"
  | "begin_login"
  | "authorize_private_target"
  | "view_capability"
  | "invoke_local_agent"
  | "request_byok"
  | "compare_candidate"
  | "discard_candidate"
  | "resolve_merge"
  | "open_result";

export interface ImportItemPresentation {
  tone: ImportItemTone;
  labelKey: string;
  icon: ImportItemIcon;
  progressMode: ImportItemProgressMode;
  progressValue: number | null;
  progressLabel: string | null;
  actions: readonly ImportItemAction[];
  selectable: boolean;
  committable: boolean;
}

interface StaticPresentation {
  tone: ImportItemTone;
  labelKey: string;
  icon: ImportItemIcon;
  progressMode: ImportItemProgressMode;
  actions: readonly ImportItemAction[];
  selectable: boolean;
  committable: boolean;
}

const STATUS_PRESENTATION: Record<ImportItemStatus, StaticPresentation> = {
  queued: { tone: "neutral", labelKey: "importV2.itemStatus.queued", icon: "queue", progressMode: "none", actions: ["start", "cancel"], selectable: false, committable: false },
  inspecting: { tone: "accent", labelKey: "importV2.itemStatus.inspecting", icon: "scan", progressMode: "indeterminate", actions: ["cancel"], selectable: false, committable: false },
  waiting_capability: { tone: "warning", labelKey: "importV2.itemStatus.waitingCapability", icon: "capability", progressMode: "none", actions: ["view_capability", "cancel"], selectable: false, committable: false },
  waiting_login: { tone: "warning", labelKey: "importV2.itemStatus.waitingLogin", icon: "login", progressMode: "none", actions: ["begin_login", "cancel"], selectable: false, committable: false },
  extracting: { tone: "accent", labelKey: "importV2.itemStatus.extracting", icon: "scan", progressMode: "indeterminate", actions: ["cancel"], selectable: false, committable: false },
  validating: { tone: "accent", labelKey: "importV2.itemStatus.validating", icon: "shield", progressMode: "indeterminate", actions: ["cancel"], selectable: false, committable: false },
  preview_ready: { tone: "accent", labelKey: "importV2.itemStatus.previewReady", icon: "ready", progressMode: "none", actions: ["inspect", "preview_markdown"], selectable: true, committable: true },
  needs_merge: { tone: "warning", labelKey: "importV2.itemStatus.needsMerge", icon: "merge", progressMode: "none", actions: ["compare_candidate", "resolve_merge", "discard_candidate"], selectable: true, committable: true },
  committing: { tone: "accent", labelKey: "importV2.itemStatus.committing", icon: "commit", progressMode: "indeterminate", actions: ["cancel"], selectable: false, committable: false },
  completed: { tone: "accent", labelKey: "importV2.itemStatus.completed", icon: "completed", progressMode: "none", actions: ["open_result", "preview_markdown"], selectable: false, committable: false },
  paused: { tone: "warning", labelKey: "importV2.itemStatus.paused", icon: "pause", progressMode: "none", actions: ["retry", "cancel"], selectable: false, committable: false },
  cancelled: { tone: "neutral", labelKey: "importV2.itemStatus.cancelled", icon: "cancelled", progressMode: "none", actions: ["retry"], selectable: false, committable: false },
  skipped: { tone: "neutral", labelKey: "importV2.itemStatus.skipped", icon: "skipped", progressMode: "none", actions: ["retry"], selectable: false, committable: false },
  failed: { tone: "danger", labelKey: "importV2.itemStatus.failed", icon: "failed", progressMode: "none", actions: ["retry"], selectable: false, committable: false },
};

const OPTIONAL_RECOVERY_ACTIONS: readonly AgentRecoveryAction[] = [
  "invoke_local_agent",
  "request_byok",
];

export function presentImportItem(item: ImportItem): ImportItemPresentation {
  const base = STATUS_PRESENTATION[item.status];
  const actions = [...base.actions];
  for (const action of OPTIONAL_RECOVERY_ACTIONS) {
    if (item.issue?.availableActions.includes(action) && !actions.includes(action)) actions.push(action);
  }
  if (item.issue?.recoveryActions.includes("authorize_private_target") && !actions.includes("authorize_private_target")) {
    actions.push("authorize_private_target");
  }
  const total = item.progress?.total;
  const progressValue = total && total > 0
    ? Math.max(0, Math.min(100, Math.round((item.progress?.current ?? 0) / total * 100)))
    : null;
  const progressMode = base.progressMode === "indeterminate" && progressValue !== null ? "measured" : base.progressMode;
  return {
    ...base,
    progressMode,
    progressValue,
    progressLabel: item.progress?.label ?? null,
    actions,
  };
}
