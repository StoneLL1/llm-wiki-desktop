import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { useTranslation } from "react-i18next";
import {
  AlertTriangle,
  Check,
  FileDiff,
  ListTree,
  LoaderCircle,
  Maximize2,
  Minimize2,
  RotateCcw,
  Sparkles,
  Square,
  X,
} from "lucide-react";

import { AgentActivityTimeline } from "../../components/agent/AgentActivityTimeline";
import {
  cancelTaskRequest,
  fetchTaskActivities,
  fetchTaskLogs,
  selectTaskById,
  useTaskStore,
} from "../../stores/taskStore";
import type { AgentInfo } from "../../types/agent";
import type { LlmProviderKind, ProviderStatus } from "../../types/llm";
import type {
  SourceUpdatePreview,
  StartSourceAiOrganizeInput,
} from "../../types/source";
import type { BackendTask, LogLine, TaskActivity } from "../../types/task";
import { isTerminalStatus } from "../../types/task";
import { MarkdownReader } from "./MarkdownReader";

type Route = StartSourceAiOrganizeInput["route"];
type ResultTab = "final" | "diff" | "process";

interface SourceAiOrganizeDialogProps {
  open: boolean;
  sourceTitle: string;
  unsavedEdits: boolean;
  busy: boolean;
  running: boolean;
  agents: AgentInfo[];
  providers: ProviderStatus[];
  failedTask: BackendTask | null;
  task?: BackendTask | null;
  preview?: SourceUpdatePreview | null;
  candidateId?: string | null;
  mutating?: boolean;
  projectId?: string;
  projectRootPath?: string;
  pagePath?: string;
  error: string | null;
  onClose: () => void;
  onOpenTask: (taskId: string) => void;
  onStart: (input: StartSourceAiOrganizeInput) => Promise<BackendTask | null>;
  onRetry: (taskId: string) => Promise<BackendTask | null>;
  onPreviewCandidate?: (
    candidateId: string,
  ) => Promise<SourceUpdatePreview | null>;
  onApply?: (preview: SourceUpdatePreview) => Promise<boolean>;
  onDiscard?: (candidateId: string) => Promise<boolean>;
}

interface WorkbenchFrame {
  x: number;
  y: number;
  width: number;
  height: number;
}

const PROVIDER_LABELS: Record<LlmProviderKind, string> = {
  open_ai: "OpenAI",
  anthropic: "Anthropic",
  google: "Google",
  ollama: "Ollama",
  custom: "Custom",
};

const FRAME_STORAGE_KEY = "llm-wiki-source-ai-workbench-frame-v1";
const MIN_WIDTH = 420;
const MIN_HEIGHT = 280;
const EMPTY_ACTIVITIES: TaskActivity[] = [];
const EMPTY_LOGS: LogLine[] = [];
const RESULT_TABS: ResultTab[] = ["final", "diff", "process"];

export function SourceAiOrganizeDialog({
  open,
  sourceTitle,
  unsavedEdits,
  busy,
  running,
  agents,
  providers,
  failedTask,
  task = null,
  preview = null,
  candidateId = null,
  mutating = false,
  projectId,
  projectRootPath,
  pagePath,
  error,
  onClose,
  onOpenTask,
  onStart,
  onRetry,
  onPreviewCandidate,
  onApply,
  onDiscard,
}: SourceAiOrganizeDialogProps) {
  const { t } = useTranslation();
  const panelRef = useRef<HTMLDivElement>(null);
  const cancelTriggerRef = useRef<HTMLButtonElement>(null);
  const keepRunningRef = useRef<HTMLButtonElement>(null);
  const previousCancelArmedRef = useRef(false);
  const previousStatusRef = useRef<string | null>(null);
  const dragRef = useRef<{
    pointerId: number;
    startX: number;
    startY: number;
    originX: number;
    originY: number;
  } | null>(null);
  const [route, setRoute] = useState<Route>("auto");
  const [provider, setProvider] = useState<LlmProviderKind | null>(null);
  const [instructions, setInstructions] = useState("");
  const [configureAgain, setConfigureAgain] = useState(false);
  const [resultTab, setResultTab] = useState<ResultTab>("final");
  const [minimized, setMinimized] = useState(false);
  const [cancelArmed, setCancelArmed] = useState(false);
  const [ownedTaskId, setOwnedTaskId] = useState<string | null>(null);
  const [candidatePreview, setCandidatePreview] =
    useState<SourceUpdatePreview | null>(null);
  const [previewLoadState, setPreviewLoadState] = useState<
    "idle" | "loading" | "failed"
  >("idle");
  const [frame, setFrame] = useState<WorkbenchFrame>(() => initialFrame());
  const previewAttemptedRef = useRef<string | null>(null);
  const previewRequestSerialRef = useRef(0);

  const ownedTask = useTaskStore((state) => selectTaskById(state, ownedTaskId));
  const currentTask = ownedTask ?? task ?? failedTask;
  const taskId = currentTask?.id ?? null;
  const taskActivities = useTaskStore((state) =>
    taskId ? state.activities[taskId] : undefined,
  ) ?? EMPTY_ACTIVITIES;
  const taskLogs = useTaskStore((state) =>
    taskId ? state.logs[taskId] : undefined,
  ) ?? EMPTY_LOGS;

  const availableProviders = useMemo(
    () =>
      providers.filter(
        ({ config, hasSecret }) =>
          config.enabled && (hasSecret || config.provider === "ollama"),
      ),
    [providers],
  );
  const defaultAgent = agents.find((entry) => entry.isDefault) ?? null;
  const agentReady = defaultAgent?.state === "installed";
  const selectedProvider =
    availableProviders.find((entry) => entry.config.provider === provider) ??
    availableProviders[0] ??
    null;
  const providerReady = selectedProvider != null;
  const taskActive =
    running || Boolean(currentTask && !isTerminalStatus(currentTask.status));
  const taskStopped =
    currentTask?.status === "failed" || currentTask?.status === "cancelled";
  const taskCandidateId =
    currentTask?.result?.reference?.type === "source_ai_organize"
      ? currentTask.result.reference.candidateId ?? null
      : null;
  const resolvedCandidateId = taskCandidateId ?? candidateId;
  const resolvedPreview =
    candidatePreview?.candidateId === resolvedCandidateId
      ? candidatePreview
      : preview?.candidateId === resolvedCandidateId
        ? preview
        : null;
  const candidateReady =
    currentTask?.status === "succeeded" && Boolean(resolvedCandidateId);
  const showSetup =
    configureAgain ||
    (!taskActive && !taskStopped && !candidateReady && !currentTask);
  const canStart =
    !busy &&
    !taskActive &&
    !unsavedEdits &&
    (route !== "auto" || agentReady || providerReady) &&
    (route !== "agent" || agentReady) &&
    (route !== "byok" || providerReady);

  useEffect(() => {
    if (!provider && availableProviders[0]) {
      setProvider(availableProviders[0].config.provider);
    }
  }, [availableProviders, provider]);

  useEffect(() => {
    if (!open || !taskId) return;
    void Promise.all([fetchTaskLogs(taskId), fetchTaskActivities(taskId)]);
  }, [open, taskId]);

  useEffect(() => {
    if (!open || !candidateReady || !resolvedCandidateId) return;
    if (resolvedPreview) {
      setPreviewLoadState("idle");
      return;
    }
    if (
      !onPreviewCandidate ||
      previewAttemptedRef.current === resolvedCandidateId
    ) {
      return;
    }
    const requestSerial = previewRequestSerialRef.current + 1;
    previewRequestSerialRef.current = requestSerial;
    previewAttemptedRef.current = resolvedCandidateId;
    setCandidatePreview(null);
    setPreviewLoadState("loading");
    void onPreviewCandidate(resolvedCandidateId)
      .then((loadedPreview) => {
        if (previewRequestSerialRef.current !== requestSerial) return;
        if (loadedPreview?.candidateId === resolvedCandidateId) {
          setCandidatePreview(loadedPreview);
          setPreviewLoadState("idle");
        } else {
          setPreviewLoadState("failed");
        }
      })
      .catch(() => {
        if (previewRequestSerialRef.current === requestSerial) {
          setPreviewLoadState("failed");
        }
      });
  }, [
    candidateReady,
    onPreviewCandidate,
    open,
    resolvedCandidateId,
    resolvedPreview,
  ]);

  useEffect(() => {
    const previous = previousStatusRef.current;
    const next = currentTask?.status ?? null;
    previousStatusRef.current = next;
    if (!open || next !== "succeeded" || previous === "succeeded") return;
    setResultTab("final");
    if (!minimized) {
      setFrame((current) => expandedFrame(current));
    }
  }, [currentTask?.status, minimized, open]);

  useEffect(() => {
    if (open || currentTask) return;
    setRoute("auto");
    setProvider(availableProviders[0]?.config.provider ?? null);
    setInstructions("");
    setConfigureAgain(false);
    setResultTab("final");
    setCancelArmed(false);
    setOwnedTaskId(null);
    setCandidatePreview(null);
    setPreviewLoadState("idle");
    previewAttemptedRef.current = null;
    previewRequestSerialRef.current += 1;
  }, [availableProviders, currentTask, open]);

  useEffect(() => {
    if (!open || minimized || typeof ResizeObserver === "undefined") return;
    const element = panelRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => {
      const bounds = element.getBoundingClientRect();
      const width = Math.round(bounds.width);
      const height = Math.round(bounds.height);
      setFrame((current) => {
        if (
          Math.abs(current.width - width) < 2 &&
          Math.abs(current.height - height) < 2
        ) {
          return current;
        }
        return clampFrame({ ...current, width, height });
      });
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [minimized, open]);

  useEffect(() => {
    if (!open || minimized) return;
    persistFrame(frame);
  }, [frame, minimized, open]);

  useEffect(() => {
    if (!open) return;
    const handleResize = () => setFrame((current) => clampFrame(current));
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, [open]);

  useEffect(() => {
    if (cancelArmed) {
      keepRunningRef.current?.focus();
    } else if (previousCancelArmedRef.current) {
      cancelTriggerRef.current?.focus();
    }
    previousCancelArmedRef.current = cancelArmed;
  }, [cancelArmed]);

  if (!open) return null;

  const submit = async () => {
    if (!canStart) return;
    const startedTask = await onStart({
      route,
      agent: route === "agent" ? defaultAgent?.kind ?? null : null,
      provider:
        route === "byok" ? selectedProvider?.config.provider ?? null : null,
      customInstructions: instructions.trim() || null,
    });
    if (startedTask) {
      setOwnedTaskId(startedTask.id);
      setCandidatePreview(null);
      setPreviewLoadState("idle");
      previewAttemptedRef.current = null;
      setConfigureAgain(false);
      setCancelArmed(false);
    }
  };

  const retry = async () => {
    if (!taskId) return;
    const startedTask = await onRetry(taskId);
    if (startedTask) {
      setOwnedTaskId(startedTask.id);
      setCandidatePreview(null);
      setPreviewLoadState("idle");
      previewAttemptedRef.current = null;
      setConfigureAgain(false);
      setCancelArmed(false);
    }
  };

  const regenerate = async () => {
    if (!resolvedCandidateId || !onDiscard) return;
    const reference =
      currentTask?.result?.reference?.type === "source_ai_organize"
        ? currentTask.result.reference
        : null;
    const discarded = await onDiscard(resolvedCandidateId);
    if (!discarded) return;
    setCandidatePreview(null);
    setPreviewLoadState("idle");
    previewAttemptedRef.current = null;
    setResultTab("final");
    if (!reference) {
      setConfigureAgain(true);
      return;
    }
    const restartedTask = await onStart({
      route: reference.route ?? "auto",
      agent: reference.agent ?? null,
      provider: reference.provider ?? null,
      customInstructions: reference.customInstructions ?? null,
    });
    if (restartedTask) {
      setOwnedTaskId(restartedTask.id);
      setConfigureAgain(false);
      setCancelArmed(false);
    } else {
      setConfigureAgain(true);
    }
  };

  const discardAndClose = async () => {
    if (!onDiscard || !resolvedCandidateId) return;
    if (await onDiscard(resolvedCandidateId)) onClose();
  };

  const applyAndClose = async () => {
    if (!onApply || !resolvedPreview) return;
    if (await onApply(resolvedPreview)) onClose();
  };

  const retryCandidatePreview = () => {
    if (!resolvedCandidateId || !onPreviewCandidate) return;
    previewAttemptedRef.current = null;
    setPreviewLoadState("idle");
    const requestSerial = previewRequestSerialRef.current + 1;
    previewRequestSerialRef.current = requestSerial;
    previewAttemptedRef.current = resolvedCandidateId;
    setPreviewLoadState("loading");
    void onPreviewCandidate(resolvedCandidateId)
      .then((loadedPreview) => {
        if (previewRequestSerialRef.current !== requestSerial) return;
        if (loadedPreview?.candidateId === resolvedCandidateId) {
          setCandidatePreview(loadedPreview);
          setPreviewLoadState("idle");
        } else {
          setPreviewLoadState("failed");
        }
      })
      .catch(() => {
        if (previewRequestSerialRef.current === requestSerial) {
          setPreviewLoadState("failed");
        }
      });
  };

  const handleDragStart = (event: ReactPointerEvent<HTMLElement>) => {
    if (minimized || (event.target as HTMLElement).closest("button")) return;
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      originX: frame.x,
      originY: frame.y,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const handleDragMove = (event: ReactPointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    setFrame((current) =>
      clampFrame({
        ...current,
        x: drag.originX + event.clientX - drag.startX,
        y: drag.originY + event.clientY - drag.startY,
      }),
    );
  };

  const handleDragEnd = (event: ReactPointerEvent<HTMLElement>) => {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  if (minimized) {
    return (
      <div
        role="dialog"
        aria-modal="false"
        aria-labelledby="source-ai-organize-title"
        className="source-ai-workbench source-ai-workbench--minimized"
      >
        <div className="source-ai-workbench__compact-status">
          {taskActive ? (
            <LoaderCircle size={13} className="animate-spin" aria-hidden />
          ) : candidateReady ? (
            <Check size={13} aria-hidden />
          ) : (
            <Sparkles size={13} aria-hidden />
          )}
          <span id="source-ai-organize-title" className="truncate">
            {sourceTitle}
          </span>
          <span className="sr-only" role="status" aria-live="polite">
            {t(
              taskActive
                ? "task.status.running"
                : candidateReady
                  ? "task.status.succeeded"
                  : "source.aiOrganize.label",
            )}
          </span>
        </div>
        <button
          type="button"
          className="btn btn--ghost btn--icon btn--sm"
          aria-label={t("source.aiOrganize.workbench.restore")}
          title={t("source.aiOrganize.workbench.restore")}
          onClick={() => setMinimized(false)}
        >
          <Maximize2 size={13} />
        </button>
        <button
          type="button"
          className="btn btn--ghost btn--icon btn--sm"
          aria-label={t("source.aiOrganize.dialog.close")}
          title={t("source.aiOrganize.dialog.close")}
          onClick={onClose}
        >
          <X size={13} />
        </button>
      </div>
    );
  }

  const panelStyle = {
    left: frame.x,
    top: frame.y,
    width: frame.width,
    height: frame.height,
  } as CSSProperties;

  return (
    <div
      ref={panelRef}
      role="dialog"
      aria-modal="false"
      aria-labelledby="source-ai-organize-title"
      className={`source-ai-workbench ${
        candidateReady ? "source-ai-workbench--result" : ""
      }`}
      style={panelStyle}
    >
      <header
        className="source-ai-workbench__head"
        onPointerDown={handleDragStart}
        onPointerMove={handleDragMove}
        onPointerUp={handleDragEnd}
        onPointerCancel={handleDragEnd}
      >
        <div className="min-w-0">
          <div className="flex min-w-0 items-center gap-2">
            <Sparkles size={14} className="shrink-0 text-[var(--accent-hover)]" />
            <h2
              id="source-ai-organize-title"
              className="m-0 truncate text-[13px] font-semibold"
            >
              {t("source.aiOrganize.dialog.title")}
            </h2>
            <TaskStatus task={currentTask} active={taskActive} />
          </div>
          <p className="mb-0 mt-0.5 truncate text-[11px] text-[var(--text-muted)]">
            {sourceTitle}
          </p>
        </div>
        <div className="ml-auto flex shrink-0 items-center gap-1">
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label={t("source.aiOrganize.workbench.minimize")}
            title={t("source.aiOrganize.workbench.minimize")}
            onClick={() => setMinimized(true)}
          >
            <Minimize2 size={13} />
          </button>
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label={t("source.aiOrganize.dialog.close")}
            title={t("source.aiOrganize.dialog.close")}
            onClick={onClose}
          >
            <X size={14} />
          </button>
        </div>
      </header>

      {candidateReady && !configureAgain ? (
        <nav
          className="source-ai-workbench__tabs"
          role="tablist"
          aria-label={t("source.aiOrganize.workbench.reviewViews")}
        >
          {RESULT_TABS.map((tab) => (
            <button
              key={tab}
              id={`source-ai-tab-${tab}`}
              type="button"
              role="tab"
              aria-selected={resultTab === tab}
              aria-controls={`source-ai-panel-${tab}`}
              tabIndex={resultTab === tab ? 0 : -1}
              className={resultTab === tab ? "is-active" : undefined}
              onClick={() => setResultTab(tab)}
              onKeyDown={(event) => {
                if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
                  return;
                }
                event.preventDefault();
                const direction = event.key === "ArrowRight" ? 1 : -1;
                const currentIndex = RESULT_TABS.indexOf(tab);
                const nextTab =
                  RESULT_TABS[
                    (currentIndex + direction + RESULT_TABS.length) %
                      RESULT_TABS.length
                  ];
                setResultTab(nextTab);
                document.getElementById(`source-ai-tab-${nextTab}`)?.focus();
              }}
            >
              {tab === "final" ? (
                <Sparkles size={13} />
              ) : tab === "diff" ? (
                <FileDiff size={13} />
              ) : (
                <ListTree size={13} />
              )}
              {t(`source.aiOrganize.workbench.tab.${tab}`)}
            </button>
          ))}
        </nav>
      ) : null}

      <div className="source-ai-workbench__body">
        {error && !showSetup ? (
          <div className="px-3 pt-3">
            <InlineAlert>{error}</InlineAlert>
          </div>
        ) : null}
        {showSetup ? (
          <SetupView
            route={route}
            setRoute={setRoute}
            provider={provider}
            setProvider={setProvider}
            instructions={instructions}
            setInstructions={setInstructions}
            agents={agents}
            availableProviders={availableProviders}
            selectedProvider={selectedProvider}
            defaultAgent={defaultAgent}
            agentReady={agentReady}
            unsavedEdits={unsavedEdits}
            error={error}
          />
        ) : taskStopped ? (
          <FailureView
            task={currentTask}
            activities={taskActivities}
            logs={taskLogs}
            onOpenTask={() => {
              if (taskId) onOpenTask(taskId);
            }}
          />
        ) : candidateReady ? (
          RESULT_TABS.map((tab) => (
            <div
              key={tab}
              id={`source-ai-panel-${tab}`}
              role="tabpanel"
              aria-labelledby={`source-ai-tab-${tab}`}
              className="h-full min-h-0"
              hidden={resultTab !== tab}
            >
              <ResultView
                tab={tab}
                preview={resolvedPreview}
                previewFailed={previewLoadState === "failed"}
                onRetryPreview={retryCandidatePreview}
                activities={taskActivities}
                logs={taskLogs}
                projectId={projectId}
                projectRootPath={projectRootPath}
                pagePath={pagePath}
                onOpenTask={() => {
                  if (taskId) onOpenTask(taskId);
                }}
              />
            </div>
          ))
        ) : (
          <RunningView
            task={currentTask}
            activities={taskActivities}
            logs={taskLogs}
          />
        )}
      </div>

      <footer className="source-ai-workbench__foot">
        {showSetup ? (
          <>
            <span className="mr-auto text-[10.5px] text-[var(--text-muted)]">
              {t("source.aiOrganize.workbench.backgroundHint")}
            </span>
            <button type="button" className="btn btn--sm" onClick={onClose}>
              {t("source.aiOrganize.dialog.cancel")}
            </button>
            <button
              type="button"
              className="btn btn--primary btn--sm"
              disabled={!canStart}
              onClick={() => void submit()}
            >
              {busy ? (
                <LoaderCircle size={13} className="animate-spin" />
              ) : (
                <Sparkles size={13} />
              )}
              {busy
                ? t("source.aiOrganize.dialog.starting")
                : t("source.aiOrganize.dialog.generate")}
            </button>
          </>
        ) : taskActive ? (
          cancelArmed ? (
            <>
              <span
                role="alert"
                className="mr-auto text-[11.5px] text-[var(--danger)]"
              >
                {t("source.aiOrganize.workbench.cancelConfirm")}
              </span>
              <button
                ref={keepRunningRef}
                type="button"
                className="btn btn--sm"
                onClick={() => setCancelArmed(false)}
              >
                {t("source.aiOrganize.workbench.keepRunning")}
              </button>
              <button
                type="button"
                className="btn btn--danger btn--sm"
                onClick={() => {
                  if (taskId) void cancelTaskRequest(taskId);
                  setCancelArmed(false);
                }}
              >
                {t("source.aiOrganize.workbench.confirmCancel")}
              </button>
            </>
          ) : (
            <>
              <span className="mr-auto text-[10.5px] text-[var(--text-muted)]">
                {t("source.aiOrganize.workbench.closeKeepsRunning")}
              </span>
              <button
                ref={cancelTriggerRef}
                type="button"
                className="btn btn--sm text-[var(--danger)]"
                onClick={() => setCancelArmed(true)}
              >
                <Square size={11} />
                {t("task.action.cancel")}
              </button>
            </>
          )
        ) : taskStopped ? (
          <>
            <button
              type="button"
              className="btn btn--sm"
              onClick={() => setConfigureAgain(true)}
            >
              {t("source.aiOrganize.workbench.changeSettings")}
            </button>
            <button
              type="button"
              className="btn btn--primary btn--sm"
              disabled={
                busy ||
                unsavedEdits ||
                currentTask?.status !== "failed" ||
                !currentTask.error?.recoverable
              }
              onClick={() => void retry()}
            >
              <RotateCcw size={13} />
              {t("source.aiOrganize.dialog.retry")}
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              className="btn btn--sm"
              disabled={mutating || busy || unsavedEdits}
              onClick={() => void regenerate()}
            >
              <RotateCcw size={13} />
              {t("source.aiOrganize.workbench.regenerate")}
            </button>
            <span className="flex-1" />
            <button
              type="button"
              className="btn btn--sm"
              disabled={mutating || !onDiscard}
              onClick={() => void discardAndClose()}
            >
              {t("source.candidate.discard")}
            </button>
            <button
              type="button"
              className="btn btn--primary btn--sm"
              disabled={
                mutating ||
                unsavedEdits ||
                !resolvedPreview ||
                resolvedPreview.mode === "three_way" ||
                !onApply
              }
              onClick={() => void applyAndClose()}
            >
              {mutating ? (
                <LoaderCircle size={13} className="animate-spin" />
              ) : (
                <Check size={13} />
              )}
              {t("source.candidate.apply")}
            </button>
          </>
        )}
      </footer>
    </div>
  );
}

function SetupView({
  route,
  setRoute,
  provider,
  setProvider,
  instructions,
  setInstructions,
  availableProviders,
  selectedProvider,
  defaultAgent,
  agentReady,
  unsavedEdits,
  error,
}: {
  route: Route;
  setRoute: (route: Route) => void;
  provider: LlmProviderKind | null;
  setProvider: (provider: LlmProviderKind) => void;
  instructions: string;
  setInstructions: (instructions: string) => void;
  agents: AgentInfo[];
  availableProviders: ProviderStatus[];
  selectedProvider: ProviderStatus | null;
  defaultAgent: AgentInfo | null;
  agentReady: boolean;
  unsavedEdits: boolean;
  error: string | null;
}) {
  const { t } = useTranslation();
  return (
    <div className="source-ai-workbench__setup">
      <div className="formrow">
        <div>
          <div className="formrow__label">{t("source.aiOrganize.dialog.task")}</div>
          <div className="formrow__hint">
            {t("source.aiOrganize.dialog.taskHint")}
          </div>
        </div>
        <div className="formrow__control text-[12px] font-medium">
          {t("source.aiOrganize.dialog.fixedTask")}
        </div>
      </div>

      <div className="formrow">
        <div>
          <div className="formrow__label">{t("source.aiOrganize.dialog.route")}</div>
          <div className="formrow__hint">
            {t("source.aiOrganize.dialog.routeHint")}
          </div>
        </div>
        <div className="formrow__control space-y-2">
          <div
            className="seg"
            role="radiogroup"
            aria-label={t("source.aiOrganize.dialog.route")}
          >
            {(["auto", "agent", "byok"] as const).map((value) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={route === value}
                className={route === value ? "is-active" : undefined}
                onClick={() => setRoute(value)}
              >
                {t(`source.aiOrganize.route.${value}`)}
              </button>
            ))}
          </div>
          <div className="space-y-1 text-[11px] text-[var(--text-muted)]">
            <p className="m-0">
              {t("source.aiOrganize.dialog.currentAgent", {
                value: defaultAgent
                  ? agentReady
                    ? defaultAgent.kind
                    : t("source.aiOrganize.dialog.unavailable", {
                        value: defaultAgent.kind,
                      })
                  : t("source.aiOrganize.dialog.notConfigured"),
              })}
            </p>
            <p className="m-0">
              {t("source.aiOrganize.dialog.currentProvider", {
                value: selectedProvider
                  ? `${PROVIDER_LABELS[selectedProvider.config.provider]} / ${selectedProvider.config.model}`
                  : t("source.aiOrganize.dialog.notConfigured"),
              })}
            </p>
          </div>
          {route === "byok" && availableProviders.length > 1 ? (
            <select
              aria-label={t("source.aiOrganize.dialog.provider")}
              value={selectedProvider?.config.provider ?? provider ?? ""}
              onChange={(event) =>
                setProvider(event.target.value as LlmProviderKind)
              }
              className="select"
            >
              {availableProviders.map(({ config }) => (
                <option key={config.provider} value={config.provider}>
                  {PROVIDER_LABELS[config.provider]} / {config.model}
                </option>
              ))}
            </select>
          ) : null}
          {!agentReady && availableProviders.length === 0 ? (
            <p role="status" className="m-0 text-[11px] text-[var(--danger)]">
              {t("source.aiOrganize.dialog.noAvailableRoute")}
            </p>
          ) : null}
        </div>
      </div>

      <div className="formrow">
        <div>
          <div className="formrow__label">{t("source.aiOrganize.dialog.scope")}</div>
          <div className="formrow__hint">
            {t("source.aiOrganize.dialog.scopeHint")}
          </div>
        </div>
        <ul className="formrow__control m-0 space-y-1 pl-4 text-[11.5px] leading-5 text-[var(--text-secondary)]">
          <li>{t("source.aiOrganize.dialog.scopeMarkdown")}</li>
          <li>{t("source.aiOrganize.dialog.scopeEvidence")}</li>
          <li>{t("source.aiOrganize.dialog.scopeExcluded")}</li>
          <li>{t("source.aiOrganize.dialog.credentialUse")}</li>
        </ul>
      </div>

      <label className="block">
        <span className="mb-1 block text-[11.5px] font-medium">
          {t("source.aiOrganize.dialog.instructions")}
        </span>
        <textarea
          value={instructions}
          maxLength={4000}
          onChange={(event) => setInstructions(event.target.value)}
          placeholder={t("source.aiOrganize.dialog.instructionsPlaceholder")}
          className="min-h-[88px] w-full resize-y rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--background)] p-2 text-[12px] outline-none focus:border-[var(--accent)]"
        />
        <span className="mt-1 block text-right text-[10.5px] text-[var(--text-muted)]">
          {instructions.length}/4000
        </span>
      </label>

      {unsavedEdits ? (
        <InlineAlert>{t("source.aiOrganize.dialog.unsaved")}</InlineAlert>
      ) : null}
      {error ? <p className="m-0 text-[11.5px] text-[var(--danger)]">{error}</p> : null}
    </div>
  );
}

function RunningView({
  task,
  activities,
  logs,
}: {
  task: BackendTask | null;
  activities: TaskActivity[];
  logs: LogLine[];
}) {
  const { t } = useTranslation();
  return (
    <div className="source-ai-workbench__running">
      <div className="source-ai-workbench__pulse" aria-hidden>
        <Sparkles size={18} />
      </div>
      <div className="min-w-0">
        <h3 className="m-0 text-[14px] font-semibold">
          {t("source.aiOrganize.workbench.runningTitle")}
        </h3>
        <p className="mb-0 mt-1 text-[11.5px] leading-5 text-[var(--text-muted)]">
          {task?.progress?.label ?? t("source.aiOrganize.workbench.waitingProvider")}
        </p>
      </div>
      <div className="col-span-2 min-h-0 overflow-y-auto pt-3">
        {activities.length ? (
          <AgentActivityTimeline
            activities={activities}
            taskStatus={task?.status ?? "running"}
            compact
          />
        ) : (
          <p role="status" className="m-0 text-[11.5px] text-[var(--text-muted)]">
            {t("source.aiOrganize.workbench.noEventsYet")}
          </p>
        )}
        <LogSummary logs={logs} />
      </div>
    </div>
  );
}

function FailureView({
  task,
  activities,
  logs,
  onOpenTask,
}: {
  task: BackendTask | null;
  activities: TaskActivity[];
  logs: LogLine[];
  onOpenTask: () => void;
}) {
  const { t } = useTranslation();
  const cancelled = task?.status === "cancelled";
  return (
    <div className="space-y-4">
      <div className="source-ai-workbench__failure" role="alert">
        <AlertTriangle size={17} className="mt-0.5 shrink-0" />
        <div className="min-w-0">
          <h3 className="m-0 text-[13px] font-semibold">
            {t(
              cancelled
                ? "source.aiOrganize.workbench.cancelledTitle"
                : "source.aiOrganize.dialog.failureTitle",
            )}
          </h3>
          <p className="mb-0 mt-1 break-words text-[11.5px] leading-5">
            {cancelled
              ? t("source.aiOrganize.workbench.cancelledBody")
              : task?.error?.message ??
                t("source.aiOrganize.dialog.failureUnknown")}
          </p>
          {task?.error?.code ? (
            <p className="mb-0 mt-1 font-mono text-[10.5px] opacity-75">
              {t("source.aiOrganize.dialog.failureCode", {
                code: task.error.code,
              })}
            </p>
          ) : null}
        </div>
      </div>
      {activities.length ? (
        <AgentActivityTimeline
          activities={activities}
          taskStatus={task?.status ?? "failed"}
          compact
        />
      ) : null}
      <LogSummary logs={logs} />
      <button type="button" className="btn btn--sm" onClick={onOpenTask}>
        {t("source.aiOrganize.dialog.viewTask")}
      </button>
    </div>
  );
}

function ResultView({
  tab,
  preview,
  previewFailed,
  onRetryPreview,
  activities,
  logs,
  projectId,
  projectRootPath,
  pagePath,
  onOpenTask,
}: {
  tab: ResultTab;
  preview: SourceUpdatePreview | null;
  previewFailed: boolean;
  onRetryPreview: () => void;
  activities: TaskActivity[];
  logs: LogLine[];
  projectId?: string;
  projectRootPath?: string;
  pagePath?: string;
  onOpenTask: () => void;
}) {
  const { t } = useTranslation();
  if (tab === "process") {
    return (
      <div className="space-y-3">
        {activities.length ? (
          <AgentActivityTimeline
            activities={activities}
            taskStatus="succeeded"
            compact
          />
        ) : (
          <p className="m-0 text-[11.5px] text-[var(--text-muted)]">
            {t("source.aiOrganize.workbench.noEventsYet")}
          </p>
        )}
        <LogSummary logs={logs} expanded />
        <button type="button" className="btn btn--sm" onClick={onOpenTask}>
          {t("source.aiOrganize.dialog.viewTask")}
        </button>
      </div>
    );
  }
  if (!preview) {
    if (previewFailed) {
      return (
        <div className="grid h-full place-items-center">
          <div className="max-w-[360px] space-y-3 text-center">
            <InlineAlert>
              {t("source.aiOrganize.workbench.previewFailed")}
            </InlineAlert>
            <button
              type="button"
              className="btn btn--sm"
              onClick={onRetryPreview}
            >
              <RotateCcw size={13} />
              {t("source.aiOrganize.workbench.retryCandidate")}
            </button>
          </div>
        </div>
      );
    }
    return (
      <div className="grid h-full place-items-center text-[12px] text-[var(--text-muted)]">
        <span className="inline-flex items-center gap-2">
          <LoaderCircle size={14} className="animate-spin" />
          {t("source.aiOrganize.workbench.loadingCandidate")}
        </span>
      </div>
    );
  }
  if (tab === "final") {
    const { body, frontmatter } = splitFrontmatter(preview.candidateMarkdown);
    return (
      <div className="source-ai-workbench__final">
        {preview.mode === "three_way" ? (
          <InlineAlert>{t("source.candidate.threeWay")}</InlineAlert>
        ) : null}
        <MarkdownReader
          bodyMarkdown={body}
          frontmatterYaml={frontmatter}
          pages={[]}
          onOpenPage={() => undefined}
          projectId={projectId}
          projectRootPath={projectRootPath}
          pagePath={pagePath}
        />
      </div>
    );
  }
  if (tab === "diff") {
    return (
      <pre
        tabIndex={0}
        className="source-ai-workbench__diff"
        aria-label={t("source.candidate.diff")}
      >
        {preview.diff}
      </pre>
    );
  }
  return null;
}

function TaskStatus({
  task,
  active,
}: {
  task: BackendTask | null;
  active: boolean;
}) {
  const { t } = useTranslation();
  const status = active ? "running" : task?.status;
  if (!status) return null;
  return (
    <span className={`source-ai-workbench__status source-ai-workbench__status--${status}`}>
      {active ? <LoaderCircle size={10} className="animate-spin" /> : null}
      {t(`task.status.${status}`)}
    </span>
  );
}

function InlineAlert({ children }: { children: string }) {
  return (
    <div
      role="alert"
      className="flex items-start gap-2 rounded-[var(--radius-md)] bg-[var(--warning-soft)] px-2.5 py-2 text-[11.5px] leading-4"
    >
      <AlertTriangle
        size={13}
        className="mt-0.5 shrink-0 text-[var(--warning)]"
      />
      <span>{children}</span>
    </div>
  );
}

function LogSummary({
  logs,
  expanded = false,
}: {
  logs: LogLine[];
  expanded?: boolean;
}) {
  const visible = expanded ? logs : logs.slice(-6);
  if (!visible.length) return null;
  return (
    <div className="source-ai-workbench__logs" role="log" aria-live="polite">
      {visible.map((line, index) => (
        <div key={`${line.timestamp}-${index}`} className={`is-${line.level}`}>
          <time>{new Date(line.timestamp).toLocaleTimeString()}</time>
          <span>{line.message}</span>
        </div>
      ))}
    </div>
  );
}

function splitFrontmatter(markdown: string): {
  frontmatter: string | null;
  body: string;
} {
  const normalized = markdown.replace(/\r\n/g, "\n");
  if (!normalized.startsWith("---\n")) {
    return { frontmatter: null, body: normalized };
  }
  const end = normalized.indexOf("\n---\n", 4);
  if (end < 0) return { frontmatter: null, body: normalized };
  return {
    frontmatter: normalized.slice(4, end),
    body: normalized.slice(end + 5),
  };
}

function initialFrame(): WorkbenchFrame {
  if (typeof window === "undefined") {
    return { x: 240, y: 64, width: 720, height: 560 };
  }
  try {
    const saved = window.localStorage.getItem(FRAME_STORAGE_KEY);
    if (saved) {
      const parsed = JSON.parse(saved) as Partial<WorkbenchFrame>;
      if (
        [parsed.x, parsed.y, parsed.width, parsed.height].every(
          (value) => typeof value === "number" && Number.isFinite(value),
        )
      ) {
        return clampFrame(parsed as WorkbenchFrame);
      }
    }
  } catch {
    // A blocked/corrupt localStorage entry should not prevent the workbench.
  }
  const width = Math.min(720, Math.max(MIN_WIDTH, window.innerWidth - 48));
  const height = Math.min(560, Math.max(MIN_HEIGHT, window.innerHeight - 96));
  return clampFrame({
    x: window.innerWidth - width - 24,
    y: 60,
    width,
    height,
  });
}

function expandedFrame(current: WorkbenchFrame): WorkbenchFrame {
  if (typeof window === "undefined") return current;
  const width = Math.min(920, Math.max(current.width, window.innerWidth * 0.72));
  const height = Math.min(760, Math.max(current.height, window.innerHeight * 0.78));
  return clampFrame({
    ...current,
    x: Math.min(current.x, window.innerWidth - width - 24),
    y: Math.min(current.y, Math.max(48, (window.innerHeight - height) / 2)),
    width,
    height,
  });
}

function clampFrame(frame: WorkbenchFrame): WorkbenchFrame {
  if (typeof window === "undefined") return frame;
  const width = Math.min(
    Math.max(MIN_WIDTH, frame.width),
    Math.max(MIN_WIDTH, window.innerWidth - 24),
  );
  const height = Math.min(
    Math.max(MIN_HEIGHT, frame.height),
    Math.max(MIN_HEIGHT, window.innerHeight - 48),
  );
  return {
    width,
    height,
    x: Math.min(Math.max(0, frame.x), Math.max(0, window.innerWidth - width)),
    y: Math.min(Math.max(0, frame.y), Math.max(0, window.innerHeight - height)),
  };
}

function persistFrame(frame: WorkbenchFrame) {
  try {
    window.localStorage.setItem(FRAME_STORAGE_KEY, JSON.stringify(frame));
  } catch {
    // Persistence is a convenience; private storage modes may reject it.
  }
}
