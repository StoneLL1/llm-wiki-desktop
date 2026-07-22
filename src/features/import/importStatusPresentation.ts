import type { AgentRecoveryAction } from "../../types/importV2Agent";
import type { ImportItem, ImportItemStatus, ImportRecoveryAction } from "../../types/importV2";
import { isSupportedMediaPlatformUrl } from "./importLocator";

export type ImportItemTone = "neutral" | "accent" | "warning" | "danger";
export type ImportItemProgressMode = "none" | "indeterminate" | "measured";
export type ImportItemIcon = "queue" | "scan" | "capability" | "login" | "shield" | "ready" | "merge" | "commit" | "completed" | "pause" | "cancelled" | "skipped" | "failed";
export type ImportItemAction =
  | "inspect"
  | "start"
  | "retry"
  | "retry_route"
  | "switch_route"
  | "switch_parser"
  | "enable_ocr"
  | "skip"
  | "authorize_local_asr"
  | "cancel"
  | "preview_markdown"
  | "begin_login"
  | "authorize_private_target"
  | "view_capability"
  | "invoke_local_agent"
  | "request_byok"
  | "view_log"
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
  preview_ready: { tone: "accent", labelKey: "importV2.itemStatus.previewReady", icon: "ready", progressMode: "none", actions: ["preview_markdown"], selectable: true, committable: true },
  // A merge candidate must be resolved before it can become a commit decision.
  // Keeping the checkbox hidden prevents a selection that confirm() cannot use.
  needs_merge: { tone: "warning", labelKey: "importV2.itemStatus.needsMerge", icon: "merge", progressMode: "none", actions: ["compare_candidate", "resolve_merge", "discard_candidate"], selectable: false, committable: false },
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

const RECOVERY_ACTION_TO_ITEM_ACTION: readonly [ImportRecoveryAction, ImportItemAction][] = [
  ["retry_route", "retry_route"],
  ["switch_route", "switch_route"],
  ["switch_parser", "switch_parser"],
  ["enable_ocr", "enable_ocr"],
  ["skip", "skip"],
  ["authorize_local_asr", "authorize_local_asr"],
];

function isRecoveryActionApplicable(item: ImportItem, action: ImportRecoveryAction): boolean {
  if (action === "skip" || action === "retry_route") return true;
  if (item.input.kind === "url") {
    return action === "switch_route"
      || (action === "enable_ocr" && isSupportedMediaPlatformUrl(item.input.normalizedLocator ?? item.input.locator));
  }
  const extension = item.input.locator.split(/[\\/.]/).pop()?.toLowerCase() ?? "";
  if (action === "enable_ocr") return extension === "pdf";
  if (action === "switch_parser") return ["doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf"].includes(extension);
  if (action === "switch_route") return ["doc", "docx", "xls", "xlsx", "ppt", "pptx", "pdf"].includes(extension);
  return true;
}

export const IMPORT_PROGRESS_LABEL_KEYS: Record<string, string> = {
  "Inspecting input": "importV2.itemStatus.inspecting",
  "Extracting source": "importV2.itemStatus.extracting",
  "Validating preview": "importV2.itemStatus.validating",
  "Preview ready": "importV2.itemStatus.previewReady",
  "Discovering files": "importV2.discovery.stage",
};

const STAGE_PROGRESS_LABELS = new Set(["Inspecting input", "Extracting source", "Validating preview", "Preview ready"]);

export function isMeasuredImportProgress(progress: { total: number | null; label: string | null } | null): boolean {
  return Boolean(progress && progress.total && progress.total > 0 && !STAGE_PROGRESS_LABELS.has(progress.label ?? ""));
}

export function presentImportItem(item: ImportItem): ImportItemPresentation {
  const base = STATUS_PRESENTATION[item.status];
  const actions = [...base.actions];
  if (item.status === "failed" && item.issue && !item.issue.retryable) {
    const retryIndex = actions.indexOf("retry");
    if (retryIndex >= 0) actions.splice(retryIndex, 1);
  }
  for (const action of OPTIONAL_RECOVERY_ACTIONS) {
    if (item.issue?.availableActions.includes(action) && !actions.includes(action)) actions.push(action);
  }
  if (item.issue?.recoveryActions.includes("authorize_private_target") && !actions.includes("authorize_private_target")) {
    actions.push("authorize_private_target");
  }
  if (item.issue) {
    for (const [recoveryAction, itemAction] of RECOVERY_ACTION_TO_ITEM_ACTION) {
      if (item.issue.recoveryActions.includes(recoveryAction) && isRecoveryActionApplicable(item, recoveryAction) && !actions.includes(itemAction)) actions.push(itemAction);
    }
    if (
      (item.issue.recoveryActions.includes("install_capability")
        || item.issue.recoveryActions.includes("install_browser_capability")
          || item.issue.recoveryActions.includes("install_media_capability")
          || item.issue.recoveryActions.includes("install_ocr_capability"))
      && !actions.includes("view_capability")
    ) {
      actions.push("view_capability");
    }
    if (item.issue.recoveryActions.includes("begin_login") && !actions.includes("begin_login")) actions.push("begin_login");
  }
  if (item.input.kind === "url"
    && isSupportedMediaPlatformUrl(item.input.normalizedLocator ?? item.input.locator)
    && (item.status === "preview_ready" || item.status === "failed")
    && !actions.includes("enable_ocr")) {
    actions.push("enable_ocr");
  }
  if (item.taskId && item.issue?.recoveryActions.includes("view_log") && !actions.includes("view_log")) {
    actions.push("view_log");
  }
  const total = item.progress?.total;
  const progressValue = total && total > 0
    ? Math.max(0, Math.min(100, Math.round((item.progress?.current ?? 0) / total * 100)))
    : null;
  // The current import pipeline reports four named stages. Only show a
  // percentage for an explicit non-stage metric such as page or byte counts.
  const progressMode = base.progressMode === "indeterminate" && isMeasuredImportProgress(item.progress) ? "measured" : base.progressMode;
  const displayProgressValue = progressMode === "measured" ? progressValue : null;
  return {
    ...base,
    progressMode,
    progressValue: displayProgressValue,
    progressLabel: item.progress?.label ?? null,
    actions,
  };
}
