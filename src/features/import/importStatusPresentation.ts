import type {
  ImportItem,
  ImportItemStatus,
  ImportPrimaryAction,
  ImportRecoveryAction,
  ImportUserState,
  UserIssue,
} from "../../types/importV2";

export type ImportItemTone = "neutral" | "accent" | "warning" | "danger";
export type ImportItemProgressMode = "none" | "indeterminate" | "measured";
export type ImportItemIcon =
  | "queue"
  | "scan"
  | "capability"
  | "login"
  | "shield"
  | "ready"
  | "merge"
  | "commit"
  | "completed"
  | "pause"
  | "cancelled"
  | "skipped"
  | "failed";
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
  | "select_subtitle"
  | "cancel"
  | "preview_markdown"
  | "preserve_remote_media"
  | "begin_login"
  | "authorize_private_target"
  | "view_capability"
  | "invoke_local_agent"
  | "view_log"
  | "compare_candidate"
  | "discard_candidate"
  | "resolve_merge"
  | "open_result";

export interface ImportItemPresentation {
  userState: ImportUserState;
  tone: ImportItemTone;
  labelKey: string;
  detailLabelKey: string;
  icon: ImportItemIcon;
  progressMode: ImportItemProgressMode;
  progressValue: number | null;
  progressLabel: string | null;
  primaryAction: ImportItemAction | null;
  secondaryActions: readonly ImportItemAction[];
  /** Compatibility aggregate for dialogs that still inspect action availability. */
  actions: readonly ImportItemAction[];
  selectable: boolean;
  committable: boolean;
  userIssue: UserIssue | null;
}

interface UserStatePresentation {
  tone: ImportItemTone;
  labelKey: string;
  icon: ImportItemIcon;
}

const USER_STATE_PRESENTATION: Record<ImportUserState, UserStatePresentation> = {
  discovering: { tone: "neutral", labelKey: "importV2.userState.discovering", icon: "queue" },
  processing: { tone: "accent", labelKey: "importV2.userState.processing", icon: "scan" },
  needs_action: { tone: "warning", labelKey: "importV2.userState.needsAction", icon: "shield" },
  ready: { tone: "accent", labelKey: "importV2.userState.ready", icon: "ready" },
  committing: { tone: "accent", labelKey: "importV2.userState.committing", icon: "commit" },
  committed: { tone: "accent", labelKey: "importV2.userState.committed", icon: "completed" },
  failed: { tone: "danger", labelKey: "importV2.userState.failed", icon: "failed" },
};

const STATUS_DETAIL_KEYS: Record<ImportItemStatus, string> = {
  queued: "importV2.itemStatus.queued",
  inspecting: "importV2.itemStatus.inspecting",
  waiting_capability: "importV2.itemStatus.waitingCapability",
  waiting_login: "importV2.itemStatus.waitingLogin",
  waiting_authorization: "importV2.itemStatus.waitingAuthorization",
  extracting: "importV2.itemStatus.extracting",
  validating: "importV2.itemStatus.validating",
  preview_ready: "importV2.itemStatus.previewReady",
  needs_merge: "importV2.itemStatus.needsMerge",
  committing: "importV2.itemStatus.committing",
  completed: "importV2.itemStatus.completed",
  paused: "importV2.itemStatus.paused",
  cancelled: "importV2.itemStatus.cancelled",
  skipped: "importV2.itemStatus.skipped",
  failed: "importV2.itemStatus.failed",
};

const STATUS_USER_STATE: Record<ImportItemStatus, ImportUserState> = {
  queued: "discovering",
  inspecting: "processing",
  waiting_capability: "needs_action",
  waiting_login: "needs_action",
  waiting_authorization: "needs_action",
  extracting: "processing",
  validating: "processing",
  preview_ready: "ready",
  needs_merge: "needs_action",
  committing: "committing",
  completed: "committed",
  paused: "needs_action",
  cancelled: "failed",
  skipped: "committed",
  failed: "failed",
};

const STATUS_ACTIONS: Record<ImportItemStatus, readonly ImportItemAction[]> = {
  queued: ["start", "cancel"],
  inspecting: ["cancel"],
  waiting_capability: ["cancel"],
  waiting_login: ["cancel"],
  waiting_authorization: ["cancel"],
  extracting: ["cancel"],
  validating: ["cancel"],
  preview_ready: ["preview_markdown"],
  needs_merge: ["resolve_merge", "compare_candidate", "discard_candidate"],
  committing: ["cancel"],
  completed: ["open_result", "preview_markdown"],
  paused: ["retry", "cancel"],
  cancelled: ["retry"],
  skipped: ["retry"],
  failed: ["retry"],
};

const RECOVERY_ACTION_TO_ITEM_ACTION: Readonly<Partial<Record<ImportRecoveryAction, ImportItemAction>>> = {
  retry: "retry",
  retry_route: "retry_route",
  switch_route: "switch_route",
  switch_parser: "switch_parser",
  enable_ocr: "enable_ocr",
  skip: "skip",
  authorize_local_asr: "authorize_local_asr",
  select_subtitle: "select_subtitle",
  begin_login: "begin_login",
  authorize_private_target: "authorize_private_target",
  install_capability: "view_capability",
  install_browser_capability: "view_capability",
  install_media_capability: "view_capability",
  install_ocr_capability: "view_capability",
  invoke_agent: "invoke_local_agent",
  view_log: "view_log",
};

const PRIMARY_ACTION_PRIORITY: readonly ImportItemAction[] = [
  "begin_login",
  "view_capability",
  "authorize_local_asr",
  "enable_ocr",
  "authorize_private_target",
  "select_subtitle",
  "resolve_merge",
  "preview_markdown",
  "open_result",
  "retry",
  "retry_route",
  "switch_route",
  "switch_parser",
  "start",
];

const ITEM_TO_ISSUE_ACTION: Readonly<Partial<Record<ImportItemAction, ImportPrimaryAction>>> = {
  retry: "retry",
  retry_route: "retry",
  switch_route: "retry",
  switch_parser: "retry",
  begin_login: "sign_in",
  authorize_private_target: "authorize",
  view_capability: "install_capability",
  enable_ocr: "enable_ocr",
  authorize_local_asr: "authorize_local_asr",
  invoke_local_agent: "invoke_local_agent",
  preview_markdown: "review",
  open_result: "review",
  resolve_merge: "resolve",
};

export const IMPORT_PROGRESS_LABEL_KEYS: Record<string, string> = {
  "Inspecting input": "importV2.itemStatus.inspecting",
  "Extracting source": "importV2.itemStatus.extracting",
  "Validating preview": "importV2.itemStatus.validating",
  "Preview ready": "importV2.itemStatus.previewReady",
  "Discovering files": "importV2.discovery.stage",
  "media.downloading": "importV2.itemStatus.mediaDownloading",
  "images.downloading": "importV2.itemStatus.imagesDownloading",
  "ocr.recognizing": "importV2.itemStatus.ocrRecognizing",
  "asr.preparing": "importV2.itemStatus.asrPreparing",
  "asr.checking_subtitles": "importV2.itemStatus.asrCheckingSubtitles",
  "asr.decoding": "importV2.itemStatus.asrDecoding",
  "asr.recognizing": "importV2.itemStatus.asrRecognizing",
  "asr.finalizing": "importV2.itemStatus.asrFinalizing",
};

const STAGE_PROGRESS_LABELS = new Set([
  "Inspecting input",
  "Extracting source",
  "Validating preview",
  "Preview ready",
]);

export function isMeasuredImportProgress(
  progress: { total: number | null; label: string | null } | null,
): boolean {
  return Boolean(
    progress
      && progress.total
      && progress.total > 0
      && !STAGE_PROGRESS_LABELS.has(progress.label ?? ""),
  );
}

function issueCopyKey(primaryAction: ImportPrimaryAction | null, status: ImportItemStatus): string {
  if (primaryAction === "sign_in") return "signIn";
  if (primaryAction === "install_capability") return "capability";
  if (primaryAction === "enable_ocr") return "ocr";
  if (primaryAction === "authorize_local_asr") return "asr";
  if (primaryAction === "authorize") return "authorization";
  if (primaryAction === "resolve") return "merge";
  if (status === "paused") return "paused";
  return "failed";
}

function toUserIssue(
  item: ImportItem,
  primaryAction: ImportItemAction | null,
): UserIssue | null {
  if (!item.issue) return null;
  const typedPrimaryAction = primaryAction ? ITEM_TO_ISSUE_ACTION[primaryAction] ?? null : null;
  const copyKey = issueCopyKey(typedPrimaryAction, item.status);
  const latestAttempt = item.attempts.at(-1);
  return {
    code: copyKey,
    title: `importV2.issue.${copyKey}.title`,
    dataSafety: `importV2.issue.${copyKey}.dataSafety`,
    primaryAction: typedPrimaryAction,
    detail: {
      technicalCode: item.issue.code,
      technicalMessage: item.issue.message,
      route: latestAttempt?.route,
      engineId: latestAttempt
        ? `${latestAttempt.engineId} ${latestAttempt.engineVersion}`
        : undefined,
      artifactPath: item.preview?.markdown.relativePath,
      contentHash: item.preview?.markdown.sha256,
    },
  };
}

function uniqueActions(actions: readonly ImportItemAction[]): ImportItemAction[] {
  return [...new Set(actions)];
}

export function presentImportItem(item: ImportItem): ImportItemPresentation {
  const exactDuplicate = item.preview?.resolution?.kind === "exact_duplicate";
  const exactDuplicateCommitted = exactDuplicate
    && (item.status === "completed" || item.status === "skipped");
  const exactDuplicateAutoFinalizing = exactDuplicate
    && item.status === "preview_ready"
    && !item.restrictedContent;
  const restrictedDuplicateAwaitingConfirmation = exactDuplicate
    && item.status === "preview_ready"
    && Boolean(item.restrictedContent);
  const resolvedMerge =
    item.status === "needs_merge"
    && item.preview?.resolution?.kind === "needs_three_way_merge"
    && Boolean(item.preview.resolution.defaultResolution);
  const userState: ImportUserState = exactDuplicateCommitted || exactDuplicateAutoFinalizing
    ? "committed"
    : resolvedMerge
      ? "ready"
      : STATUS_USER_STATE[item.status];
  const statePresentation = USER_STATE_PRESENTATION[userState];
  const actions = [...STATUS_ACTIONS[item.status]];

  for (const recoveryAction of item.issue?.recoveryActions ?? []) {
    const action = RECOVERY_ACTION_TO_ITEM_ACTION[recoveryAction];
    if (action) actions.push(action);
  }
  if (item.issue?.availableActions.includes("invoke_local_agent")) {
    actions.push("invoke_local_agent");
  }
  if (item.issue && !item.issue.retryable) {
    const retryIndex = actions.indexOf("retry");
    if (retryIndex >= 0) actions.splice(retryIndex, 1);
  }
  if (
    item.input.kind === "url"
    && item.input.mediaSaveMode !== "preserve_original"
    && (item.status === "preview_ready" || item.status === "failed" || item.status === "paused")
  ) {
    actions.push("preserve_remote_media");
  }

  const normalizedActions = uniqueActions(actions);
  const primaryAction = PRIMARY_ACTION_PRIORITY.find((action) =>
    normalizedActions.includes(action)) ?? null;
  const secondaryActions = normalizedActions.filter((action) => action !== primaryAction);
  const total = item.progress?.total;
  const progressValue = total && total > 0
    ? Math.max(0, Math.min(100, Math.round(((item.progress?.current ?? 0) / total) * 100)))
    : null;
  const baseProgressMode: ImportItemProgressMode =
    userState === "processing" || userState === "committing" ? "indeterminate" : "none";
  const progressMode =
    baseProgressMode === "indeterminate" && isMeasuredImportProgress(item.progress)
      ? "measured"
      : baseProgressMode;

  return {
    userState,
    ...statePresentation,
    detailLabelKey: exactDuplicate
      ? "importV2.preview.disposition.duplicate"
      : resolvedMerge
        ? "importV2.itemStatus.mergeResolved"
        : STATUS_DETAIL_KEYS[item.status],
    progressMode,
    progressValue: progressMode === "measured" ? progressValue : null,
    progressLabel: item.progress?.label ?? null,
    primaryAction,
    secondaryActions,
    actions: primaryAction ? [primaryAction, ...secondaryActions] : secondaryActions,
    selectable: (item.status === "preview_ready" && !exactDuplicate) || resolvedMerge,
    committable:
      (item.status === "preview_ready" && !exactDuplicate)
      || restrictedDuplicateAwaitingConfirmation
      || resolvedMerge,
    userIssue:
      resolvedMerge
      || exactDuplicateCommitted
      || exactDuplicateAutoFinalizing
      || restrictedDuplicateAwaitingConfirmation
        ? null
        : toUserIssue(item, primaryAction),
  };
}
