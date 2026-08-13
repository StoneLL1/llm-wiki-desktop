import {
  lazy,
  memo,
  type CSSProperties,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Book, Edit2, FileOutput, LoaderCircle, MessageSquareText, Sparkles, Star } from "lucide-react";

import { ResizableSplitter } from "../../components/app/ResizableSplitter";
import { ViewErrorBoundary } from "../../components/app/ViewErrorBoundary";
import { ViewFallback } from "../../components/app/ViewFallback";
import { PANE_WIDTH_LIMITS } from "../../hooks/useResizablePane";
import type { AiCapabilitiesWorkflow } from "../../hooks/useAiCapabilities";
import { useExportStore } from "../../stores/exportStore";
import { useNavigationStore } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { fetchTaskById, useTaskStore } from "../../stores/taskStore";
import { ConfirmationDialog } from "../../components/app/ConfirmationDialog";
import type { PendingAction } from "../../types/backend";
import {
  DEFAULT_EXPORT_OPTIONS,
  SINGLE_PAGE_EXPORT_TYPES,
  type ExportRecord,
  type ExportRestrictedContentStatus,
  type ExportType,
} from "../../types/export";
import type { SourceAiOrganizeBinding } from "../../types/source";
import { isTerminalStatus, type BackendTask } from "../../types/task";
import type { CreateWikiPageInput, WikiPageContent, WikiPageMeta } from "../../types/wiki";
import { MarkdownReader } from "./MarkdownReader";
import { ConflictDiffDialog } from "./ConflictDiffDialog";
import { GenerateHtmlDialog } from "./GenerateHtmlDialog";
import { HtmlPreviewPane } from "./HtmlPreviewPane";
import { WikiPageFormDialog } from "./WikiPageFormDialog";
import { WikiTree } from "./WikiTree";
import { useWikiStore } from "./wikiStore";
import { useSourceStore } from "./sourceStore";
import { SourceLifecycleDialogs, SourceMovePathDialog } from "./SourceLifecycleDialogs";
import { SourceAiOrganizeDialog } from "./SourceAiOrganizeDialog";
import { ExportRestrictedContentDialog } from "../exports/ExportRestrictedContentDialog";

// Milkdown + ProseMirror is the heaviest wiki dependency and is only needed
// when the user enters edit mode. Read/preview modes never load it.
const WikiEditor = lazy(() =>
  import("./WikiEditor").then((m) => ({ default: m.WikiEditor })),
);

export function selectWikiPreviewRecord(
  records: ExportRecord[],
  previewId: string | null,
  selectedPath: string | null,
): ExportRecord | null {
  const belongsToPage = (record: ExportRecord) =>
    record.exportType === "project_report" || record.sourcePath === selectedPath;
  const selected = records.find((record) => record.id === previewId);
  if (selected && belongsToPage(selected)) return selected;
  return (
    records
      .filter(belongsToPage)
      .sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0] ?? null
  );
}

interface WikiViewProps {
  capabilities: Pick<AiCapabilitiesWorkflow, "agents" | "providers">;
}

interface SourceAiWorkbenchScope {
  projectId: string;
  projectRootPath: string;
  sourceId: string;
  sourceTitle: string;
  pagePath: string;
  binding: SourceAiOrganizeBinding;
  initialTaskId: string | null;
  initialCandidateId: string | null;
}

export interface PendingWikiQuickExport {
  taskId: string;
  projectId: string;
  projectRootPath: string;
  pagePath: string;
  exportType: Exclude<ExportType, "project_report">;
  autoPreview: boolean;
}

interface WikiQuickExportRequest {
  scope: Omit<PendingWikiQuickExport, "taskId">;
  record: ExportRecord | null;
}

interface RestrictedWikiQuickExportConfirmation {
  request: WikiQuickExportRequest;
  count: number;
}

export function WikiView({ capabilities }: WikiViewProps) {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const paneSizes = useNavigationStore((state) => state.paneSizes);
  const setPaneSize = useNavigationStore((state) => state.setPaneSize);
  const resetPaneSize = useNavigationStore((state) => state.resetPaneSize);
  const rightPanelMode = useNavigationStore((state) => state.rightPanelMode);
  const openWikiAssistant = useNavigationStore((state) => state.openWikiAssistant);
  const requestWorkflowLaunch = useNavigationStore((state) => state.requestWorkflowLaunch);
  const setWikiAssistantPagePath = useNavigationStore((state) => state.setWikiAssistantPagePath);
  const [pageForm, setPageForm] = useState<
    { mode: "create" | "rename"; path: string } | null
  >(null);
  const [pendingLifecycle, setPendingLifecycle] = useState<
    | { kind: "rename"; action: PendingAction; oldPath: string; newPath: string }
    | { kind: "delete"; action: PendingAction }
    | null
  >(null);
  const [htmlDialogOpen, setHtmlDialogOpen] = useState(false);
  const [htmlTemplate, setHtmlTemplate] = useState<ExportType>("beautiful_read");
  const [exportStartInFlight, setExportStartInFlight] = useState(false);
  const [pendingWikiQuickExport, setPendingWikiQuickExport] =
    useState<PendingWikiQuickExport | null>(null);
  const [restrictedExportConfirmation, setRestrictedExportConfirmation] =
    useState<RestrictedWikiQuickExportConfirmation | null>(null);
  const quickExportMountedRef = useRef(true);
  const quickExportInFlightRef = useRef(false);
  const quickExportRequestEpochRef = useRef(0);
  const [conflictDialogOpen, setConflictDialogOpen] = useState(true);
  const [sourceMovePath, setSourceMovePath] = useState<string | null>(null);
  const [sourceAiWorkbench, setSourceAiWorkbench] =
    useState<SourceAiWorkbenchScope | null>(null);

  const tree = useWikiStore((state) => state.tree);
  const loadingTree = useWikiStore((state) => state.loadingTree);
  const selectedPath = useWikiStore((state) => state.selectedPath);
  const page = useWikiStore((state) => state.page);
  const mode = useWikiStore((state) => state.mode);
  const draft = useWikiStore((state) => state.draft);
  const saveState = useWikiStore((state) => state.saveState);
  const conflict = useWikiStore((state) => state.conflict);
  const loadingPage = useWikiStore((state) => state.loadingPage);
  const wikiError = useWikiStore((state) => state.error);
  const scan = useWikiStore((state) => state.scan);
  const openPage = useWikiStore((state) => state.openPage);
  const startEdit = useWikiStore((state) => state.startEdit);
  const setMode = useWikiStore((state) => state.setMode);
  const setDraft = useWikiStore((state) => state.setDraft);
  const save = useWikiStore((state) => state.save);
  const resolveConflict = useWikiStore((state) => state.resolveConflict);
  const cancelEdit = useWikiStore((state) => state.cancelEdit);
  const reload = useWikiStore((state) => state.reload);
  const toggleBookmark = useWikiStore((state) => state.toggleBookmark);
  const createPage = useWikiStore((state) => state.createPage);
  const renamePage = useWikiStore((state) => state.renamePage);
  const requestDeletePage = useWikiStore((state) => state.requestDeletePage);
  const confirmDeletePage = useWikiStore((state) => state.confirmDeletePage);
  const cancelPendingAction = useWikiStore((state) => state.cancelPendingAction);
  const requestedExportType = useWikiStore((state) => state.requestedExportType);
  const consumeExportRequest = useWikiStore((state) => state.consumeExportRequest);
  const loadSourceDetail = useSourceStore((state) => state.loadDetail);
  const previewSourceMove = useSourceStore((state) => state.previewMove);
  const previewSourceDelete = useSourceStore((state) => state.previewDelete);
  const sourceErrors = useSourceStore((state) => state.errorsBySourceId);
  const sourceMutating = useSourceStore((state) => state.mutating);
  const sourceUpdatePreview = useSourceStore((state) => state.updatePreview);
  const aiOrganizeStarting = useSourceStore((state) => state.aiOrganizeStarting);
  const startSourceAiOrganize = useSourceStore((state) => state.startAiOrganize);
  const retrySourceAiOrganize = useSourceStore((state) => state.retryAiOrganize);
  const previewSourceCandidate = useSourceStore((state) => state.previewCandidate);
  const applySourceCandidate = useSourceStore((state) => state.applyCandidate);
  const discardSourceCandidate = useSourceStore((state) => state.discardCandidate);

  const exportRecords = useExportStore((state) => state.records);
  const runningExportTaskId = useExportStore((state) => state.runningTaskId);
  const exportError = useExportStore((state) => state.error);
  const previewHtml = useExportStore((state) => state.previewHtml);
  const previewId = useExportStore((state) => state.previewId);
  const loadExports = useExportStore((state) => state.loadExports);
  const startExport = useExportStore((state) => state.startExport);
  const regenerateExport = useExportStore((state) => state.regenerateExport);
  const clearRunningTask = useExportStore((state) => state.clearRunningTask);
  const loadPreview = useExportStore((state) => state.loadPreview);
  const openFolder = useExportStore((state) => state.openFolder);
  const tasks = useTaskStore((state) => state.tasks);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);

  const { projectId, rootPath } = currentProject;
  const layoutStyle = {
    "--wiki-tree-w-current": `${paneSizes.wikiTree}px`,
  } as CSSProperties;

  const isCurrentQuickExportPresentation = (
    scope: Pick<PendingWikiQuickExport, "projectId" | "projectRootPath" | "pagePath">,
  ) => quickExportMountedRef.current && isCurrentWikiQuickExport(scope);

  useEffect(() => {
    quickExportMountedRef.current = true;
    return () => {
      quickExportMountedRef.current = false;
      quickExportRequestEpochRef.current += 1;
      quickExportInFlightRef.current = false;
    };
  }, []);

  useEffect(() => {
    setSourceAiWorkbench((current) =>
      current &&
      (current.projectId !== projectId ||
        !sameProjectRoot(current.projectRootPath, rootPath))
        ? null
        : current,
    );
  }, [projectId, rootPath]);

  useEffect(() => {
    void scan(projectId, rootPath);
  }, [projectId, rootPath, scan]);

  useEffect(() => {
    void loadExports(projectId, rootPath);
  }, [projectId, rootPath, loadExports]);

  useEffect(() => {
    if (!requestedExportType || !page) return;
    const nextType = SINGLE_PAGE_EXPORT_TYPES.includes(requestedExportType)
      ? requestedExportType
      : "beautiful_read";
    setHtmlTemplate(nextType);
    setHtmlDialogOpen(true);
    consumeExportRequest();
  }, [consumeExportRequest, page, requestedExportType]);

  useEffect(() => {
    quickExportRequestEpochRef.current += 1;
    quickExportInFlightRef.current = false;
    setExportStartInFlight(false);
    setPendingWikiQuickExport((current) =>
      current &&
      current.projectId === projectId &&
      sameProjectRoot(current.projectRootPath, rootPath) &&
      current.pagePath === selectedPath
        ? current
        : null,
    );
    setRestrictedExportConfirmation((current) =>
      current &&
      current.request.scope.projectId === projectId &&
      sameProjectRoot(current.request.scope.projectRootPath, rootPath) &&
      current.request.scope.pagePath === selectedPath
        ? current
        : null,
    );
  }, [projectId, rootPath, selectedPath]);

  useEffect(() => {
    if (conflict) setConflictDialogOpen(true);
  }, [conflict?.currentHash]);

  useEffect(() => {
    if (rightPanelMode === "wikiAssistant" && page?.meta.path) {
      setWikiAssistantPagePath(page.meta.path);
    }
  }, [rightPanelMode, page?.meta.path, setWikiAssistantPagePath]);

  const currentPendingWikiQuickExport =
    pendingWikiQuickExport &&
    pendingWikiQuickExport.projectId === projectId &&
    sameProjectRoot(pendingWikiQuickExport.projectRootPath, rootPath) &&
    pendingWikiQuickExport.pagePath === selectedPath
      ? pendingWikiQuickExport
      : null;
  const currentRunningExportTaskId =
    currentPendingWikiQuickExport?.taskId === runningExportTaskId
      ? runningExportTaskId
      : null;
  const runningExportTask = currentRunningExportTaskId
    ? tasks.find((task) => task.id === currentRunningExportTaskId) ?? null
    : null;
  const selectedSourceId = page?.meta.sourceBinding?.sourceId ?? null;
  const selectedSourceAiTasks = selectedSourceId
    ? tasks.filter(
        (task) =>
          task.taskType === "source_ai_organize" &&
          task.projectId === projectId &&
          task.result?.reference?.type === "source_ai_organize" &&
          sameProjectRoot(task.result.reference.projectRootPath, rootPath) &&
          task.result.reference.sourceId === selectedSourceId,
      )
    : [];
  const orderedSourceAiTasks = [...selectedSourceAiTasks].sort((left, right) =>
    right.updatedAt.localeCompare(left.updatedAt),
  );
  const latestSourceAiTask = orderedSourceAiTasks[0] ?? null;
  const activeSourceAiTask =
    latestSourceAiTask && !isTerminalStatus(latestSourceAiTask.status)
      ? latestSourceAiTask
      : null;
  const completedSourceAiTask =
    orderedSourceAiTasks.find(
      (task) =>
        task.status === "succeeded" &&
        task.result?.reference?.type === "source_ai_organize" &&
        Boolean(task.result.reference.candidateId),
    ) ?? null;
  const completedSourceAiTaskId = completedSourceAiTask?.id ?? null;
  const completedSourceAiCandidateId =
    completedSourceAiTask?.result?.reference?.type === "source_ai_organize"
      ? completedSourceAiTask.result.reference.candidateId ?? null
      : null;
  const sourceAiWorkbenchTask = sourceAiWorkbench?.initialTaskId
    ? tasks.find((task) => task.id === sourceAiWorkbench.initialTaskId) ?? null
    : null;
  const sourceAiWorkbenchFailedTask =
    sourceAiWorkbenchTask?.status === "failed" ||
    sourceAiWorkbenchTask?.status === "cancelled"
      ? sourceAiWorkbenchTask
      : null;
  const scopedSourcePreview =
    sourceAiWorkbench &&
    sourceUpdatePreview?.sourceId === sourceAiWorkbench.sourceId &&
    sourceUpdatePreview.candidateId === sourceAiWorkbench.initialCandidateId
      ? sourceUpdatePreview
      : null;

  useEffect(() => {
    if (
      !selectedSourceId ||
      !completedSourceAiTaskId ||
      !completedSourceAiCandidateId
    ) {
      return;
    }
    void loadSourceDetail(
      projectId,
      rootPath,
      selectedSourceId,
      `${completedSourceAiTaskId}:${completedSourceAiCandidateId}`,
    );
  }, [
    completedSourceAiCandidateId,
    completedSourceAiTaskId,
    loadSourceDetail,
    projectId,
    rootPath,
    selectedSourceId,
  ]);

  useEffect(() => {
    if (
      !runningExportTask ||
      !currentPendingWikiQuickExport ||
      !isTerminalStatus(runningExportTask.status)
    ) return;
    const finishedTask = runningExportTask;
    const exportScope = currentPendingWikiQuickExport;
    void loadExports(projectId, rootPath).then(() => {
      if (
        !quickExportMountedRef.current ||
        !isCurrentWikiQuickExport(exportScope)
      ) return;
      if (useExportStore.getState().runningTaskId !== finishedTask.id) return;
      clearRunningTask();
      if (finishedTask.status !== "succeeded") return;
      const latest = useExportStore
        .getState()
        .records.filter(
          (record) => record.sourcePath === useWikiStore.getState().selectedPath,
        )
        .sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0];
      if (!latest) return;
      void loadPreview(
        { projectId, projectRootPath: rootPath, outputPath: latest.outputPath },
        latest.id,
        () =>
          quickExportMountedRef.current &&
          isCurrentWikiQuickExport(exportScope),
      );
    });
  }, [
    currentPendingWikiQuickExport,
    runningExportTask,
    projectId,
    rootPath,
    loadExports,
    clearRunningTask,
    loadPreview,
  ]);

  const handleOpen = useCallback((path: string) => {
    void openPage(projectId, rootPath, path);
  }, [openPage, projectId, rootPath]);

  const breadcrumbs = selectedPath ? selectedPath.split("/") : [];
  const previewRecord = selectWikiPreviewRecord(exportRecords, previewId, selectedPath);
  const pagePreviewHtml = previewRecord?.id === previewId ? previewHtml : null;

  const showExportTask = (taskId: string, scope: PendingWikiQuickExport) => {
    void fetchTaskById(taskId).catch(() => undefined);
    if (isCurrentQuickExportPresentation(scope)) openTaskDrawer(taskId);
  };

  const startWikiQuickExport = async (
    request: WikiQuickExportRequest,
    acknowledgeRestrictedContent: boolean,
    requestEpoch: number,
  ) => {
    let taskId: string | null = null;
    try {
      taskId = request.record
        ? await regenerateExport(projectId, rootPath, request.record, {
            route: "auto",
            options: DEFAULT_EXPORT_OPTIONS,
            acknowledgeRestrictedContent,
          })
        : await startExport(
            projectId,
            rootPath,
            request.scope.exportType,
            request.scope.pagePath,
            {
              route: "auto",
              options: DEFAULT_EXPORT_OPTIONS,
              acknowledgeRestrictedContent,
            },
          );
    } catch {
      // The store owns the normal backend error path. This guard keeps a
      // mocked or unexpected rejection from leaving the quick-export lock.
      taskId = null;
    }

    const stillCurrent =
      requestEpoch === quickExportRequestEpochRef.current &&
      isCurrentQuickExportPresentation(request.scope);
    if (!stillCurrent) return;

    quickExportInFlightRef.current = false;
    setExportStartInFlight(false);

    if (!taskId || !stillCurrent) {
      if (!taskId && stillCurrent && request.record === null) {
        setHtmlDialogOpen(true);
      }
      return;
    }

    const pending: PendingWikiQuickExport = {
      ...request.scope,
      taskId,
    };
    setPendingWikiQuickExport(pending);
    setRestrictedExportConfirmation(null);
    setHtmlDialogOpen(false);
    setMode("preview");
    showExportTask(taskId, pending);
  };

  const requestWikiQuickExport = (
    scope: WikiQuickExportRequest["scope"],
    record: ExportRecord | null,
  ) => {
    if (
      quickExportInFlightRef.current ||
      !isCurrentQuickExportPresentation(scope)
    ) return;

    quickExportInFlightRef.current = true;
    setExportStartInFlight(true);
    const requestEpoch = ++quickExportRequestEpochRef.current;
    const request: WikiQuickExportRequest = { scope, record };

    void (async () => {
      let status: ExportRestrictedContentStatus | null = null;
      try {
        status = await invoke<ExportRestrictedContentStatus>(
          "get_export_restricted_content_status",
          {
            request: {
              projectId: scope.projectId,
              projectRootPath: scope.projectRootPath,
              exportType: scope.exportType,
              sourcePath: scope.pagePath,
            },
          },
        );
      } catch {
        // The backend remains the authorization source. If the advisory
        // lookup is unavailable, let the direct command return its decision.
      }

      if (
        requestEpoch !== quickExportRequestEpochRef.current ||
        !isCurrentQuickExportPresentation(scope)
      ) {
        if (requestEpoch === quickExportRequestEpochRef.current) {
          quickExportInFlightRef.current = false;
          setExportStartInFlight(false);
        }
        return;
      }

      if (status?.containsRestrictedContent) {
        quickExportInFlightRef.current = false;
        setExportStartInFlight(false);
        setHtmlDialogOpen(false);
        setRestrictedExportConfirmation({
          request,
          count: status.restrictedSourceCount,
        });
        return;
      }

      await startWikiQuickExport(request, false, requestEpoch);
    })();
  };

  const handleGenerateHtml = (type: ExportType) => {
    if (!page) return;
    if (!SINGLE_PAGE_EXPORT_TYPES.includes(type)) return;
    setHtmlTemplate(type);
    setHtmlDialogOpen(true);
  };

  const handleHtmlDialogCancel = () => {
    quickExportRequestEpochRef.current += 1;
    quickExportInFlightRef.current = false;
    setExportStartInFlight(false);
    setHtmlDialogOpen(false);
  };

  const handleDialogGenerate = (type: ExportType) => {
    if (!page || !SINGLE_PAGE_EXPORT_TYPES.includes(type)) return;
    setHtmlTemplate(type);
    requestWikiQuickExport(
      {
        projectId,
        projectRootPath: rootPath,
        pagePath: page.meta.path,
        exportType: type as PendingWikiQuickExport["exportType"],
        autoPreview: true,
      },
      null,
    );
  };

  const handleRegenerateHtml = () => {
    if (!page) return;
    if (!previewRecord) {
      handleGenerateHtml("beautiful_read");
      return;
    }
    if (!SINGLE_PAGE_EXPORT_TYPES.includes(previewRecord.exportType)) {
      requestWorkflowLaunch({
        projectId,
        projectRootPath: rootPath,
        kind: "generate_content",
        origin: "wiki",
        scopePreset: {
          kind: "generate_content",
          artifactType: previewRecord.exportType,
          pagePaths: previewRecord.sourcePath
            ? [previewRecord.sourcePath]
            : [page.meta.path],
          outputPath: previewRecord.outputPath,
        },
      });
      return;
    }
    setHtmlTemplate(previewRecord.exportType);
    requestWikiQuickExport(
      {
        projectId,
        projectRootPath: rootPath,
        pagePath: page.meta.path,
        exportType: previewRecord.exportType as PendingWikiQuickExport["exportType"],
        autoPreview: true,
      },
      previewRecord,
    );
  };

  const handleRestrictedExportConfirm = () => {
    const confirmation = restrictedExportConfirmation;
    if (!confirmation || quickExportInFlightRef.current) return;
    if (!isCurrentQuickExportPresentation(confirmation.request.scope)) {
      setRestrictedExportConfirmation(null);
      return;
    }
    quickExportInFlightRef.current = true;
    setExportStartInFlight(true);
    setRestrictedExportConfirmation(null);
    const requestEpoch = ++quickExportRequestEpochRef.current;
    void startWikiQuickExport(confirmation.request, true, requestEpoch);
  };

  const handleRestrictedExportCancel = () => {
    const request = restrictedExportConfirmation?.request ?? null;
    setRestrictedExportConfirmation(null);
    if (!request || !isCurrentQuickExportPresentation(request.scope)) return;
    if (request.record === null) setHtmlDialogOpen(true);
  };

  const handlePageFormSubmit = (input: CreateWikiPageInput) => {
    if (pageForm?.mode === "create") {
      setPageForm(null);
      void createPage(projectId, rootPath, input);
      return;
    }
    if (pageForm?.mode !== "rename") return;
    const oldPath = pageForm.path;
    const action: PendingAction = {
      id: `rename-${oldPath}`,
      actionType: "batch_rewrite",
      title: t("wiki.rename.confirmTitle"),
      message: t("wiki.rename.confirmMessage", {
        oldPath,
        newPath: input.relativePath,
      }),
      riskLevel: "high",
      affectedPaths: [oldPath, input.relativePath],
      preview: {
        summary: t("wiki.rename.confirmSummary"),
        before: oldPath,
        after: input.relativePath,
        diff: null,
      },
      expiresAt: null,
      checkpointHash: null,
    };
    setPageForm(null);
    setPendingLifecycle({
      kind: "rename",
      action,
      oldPath,
      newPath: input.relativePath,
    });
  };

  const handleDeleteRequest = (path: string) => {
    void requestDeletePage(projectId, rootPath, path).then((action) => {
      if (action) setPendingLifecycle({ kind: "delete", action });
    });
  };

  const handleSourceMoveRequest = (sourceId: string, path: string) => {
    void loadSourceDetail(projectId, rootPath, sourceId).then(() => {
      setSourceMovePath(path);
    });
  };

  const handleSourceDeleteRequest = (sourceId: string) => {
    void loadSourceDetail(projectId, rootPath, sourceId).then(() => {
      void previewSourceDelete(projectId, rootPath);
    });
  };

  const handleLifecycleCancel = () => {
    const pending = pendingLifecycle;
    setPendingLifecycle(null);
    if (pending?.kind === "delete") {
      void cancelPendingAction(pending.action);
    }
  };

  const handleLifecycleConfirm = () => {
    const pending = pendingLifecycle;
    if (!pending) return;
    setPendingLifecycle(null);
    if (pending.kind === "delete") {
      void confirmDeletePage(projectId, rootPath, pending.action);
    } else {
      void renamePage(projectId, rootPath, pending.oldPath, pending.newPath);
    }
  };

  return (
    <div className="wiki-view-layout" style={layoutStyle}>
      {tree ? (
        <WikiTree
          root={tree.root}
          pages={tree.pages}
          selectedPath={selectedPath}
          onSelect={handleOpen}
          onRefresh={() => void reload(projectId, rootPath)}
          onCreate={() => setPageForm({ mode: "create", path: "wiki/" })}
          onRename={(path) => setPageForm({ mode: "rename", path })}
          onDelete={handleDeleteRequest}
          onSourceRename={handleSourceMoveRequest}
          onSourceDelete={handleSourceDeleteRequest}
        />
      ) : (
        <div className="wiki-tree items-center justify-center text-[12px] text-[var(--text-muted)]">
          {loadingTree ? (
            <LoaderCircle size={16} className="animate-spin" />
          ) : (
            t("wiki.tree.empty")
          )}
        </div>
      )}

      <ResizableSplitter
        paneId="wikiTree"
        label={t("shell.splitter.wikiTree")}
        min={PANE_WIDTH_LIMITS.wikiTree.min}
        max={PANE_WIDTH_LIMITS.wikiTree.max}
        value={paneSizes.wikiTree}
        onChange={(value) => setPaneSize("wikiTree", value)}
        onReset={() => resetPaneSize("wikiTree")}
      />

      <div className="flex min-w-0 flex-1 flex-col bg-[var(--background)]">
        <div className="view-toolbar border-b border-[var(--border)] px-5">
          <div className="flex min-w-0 items-center gap-1.5 font-mono text-[12px] text-[var(--text-muted)]">
            {breadcrumbs.length === 0 ? (
              <span>{t("wiki.content.noSelection")}</span>
            ) : (
              breadcrumbs.map((segment, index) => (
                <span key={`${segment}-${index}`} className="flex items-center gap-1.5">
                  <span
                    className={
                      index === breadcrumbs.length - 1
                        ? "font-medium text-[var(--text-primary)]"
                        : ""
                    }
                  >
                    {segment}
                  </span>
                  {index < breadcrumbs.length - 1 ? (
                    <span className="text-[var(--text-disabled)]">/</span>
                  ) : null}
                </span>
              ))
            )}
          </div>
          <div className="ml-auto flex items-center gap-2">
            <span
              className={`hidden items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-medium sm:inline-flex ${
                saveState === "saved"
                  ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
                  : saveState === "conflict" || saveState === "error"
                    ? "bg-[var(--warning-soft)] text-[var(--warning)]"
                    : "bg-[var(--surface-muted)] text-[var(--text-muted)]"
              }`}
            >
              <span
                className={`inline-block h-[6px] w-[6px] rounded-full ${
                  saveState === "saved"
                    ? "bg-[var(--accent)]"
                    : saveState === "conflict" || saveState === "error"
                      ? "bg-[var(--warning)]"
                      : "bg-[var(--text-muted)]"
                }`}
              />
              {t(`wiki.editor.saveState.${saveState}`)}
            </span>
            <div className="flex overflow-hidden rounded-[var(--radius-sm)] border border-[var(--border)]">
              <ModeButton
                active={mode === "read"}
                onClick={() => setMode("read")}
                icon={<Book size={13} />}
                label={t("wiki.mode.read")}
              />
              <ModeButton
                active={mode === "edit"}
                onClick={() => startEdit()}
                icon={<Edit2 size={13} />}
                label={t("wiki.mode.edit")}
              />
              <ModeButton
                active={mode === "preview"}
                onClick={() => {
                  if (pagePreviewHtml) {
                    setMode("preview");
                  } else if (previewRecord) {
                    void loadPreview(
                      {
                        projectId,
                        projectRootPath: rootPath,
                        outputPath: previewRecord.outputPath,
                      },
                      previewRecord.id,
                    );
                    setMode("preview");
                  } else if (page) {
                    handleGenerateHtml("beautiful_read");
                  }
                }}
                icon={<FileOutput size={13} />}
                label={t("wiki.mode.preview")}
              />
            </div>
            <button
              type="button"
              disabled={
                !page ||
                exportStartInFlight ||
                Boolean(runningExportTaskId)
              }
              onClick={() => handleGenerateHtml("beautiful_read")}
              className="inline-flex h-[28px] items-center gap-1.5 rounded-[var(--radius-sm)] bg-[var(--foreground)] px-3 text-[11.5px] font-medium text-[var(--text-inverse)] disabled:opacity-40"
            >
              <FileOutput size={13} />
              {t("wiki.html.generate")}
            </button>
            {page?.meta.pageType === "source" && page.meta.sourceBinding ? (
              <button
                type="button"
                onClick={() => {
                  const sourceId = page.meta.sourceBinding!.sourceId;
                  const pagePath = page.meta.path;
                  const pageTitle = page.meta.title;
                  void loadSourceDetail(projectId, rootPath, sourceId).then(() => {
                    const currentPage = useWikiStore.getState().page;
                    const detail = useSourceStore.getState().detail;
                    if (
                      currentPage?.meta.sourceBinding?.sourceId !== sourceId ||
                      detail?.sourceId !== sourceId
                    ) return;
                    const candidateId =
                      detail.candidate?.kind === "ai_organize"
                        ? detail.candidate.candidateId
                        : null;
                    const initialTask = selectSourceAiWorkbenchTask(
                      useTaskStore.getState().tasks,
                      projectId,
                      rootPath,
                      sourceId,
                      detail.versionId,
                      detail.currentMarkdownHash,
                      candidateId,
                    );
                    setSourceAiWorkbench({
                      projectId,
                      projectRootPath: rootPath,
                      sourceId,
                      sourceTitle: detail.title || pageTitle,
                      pagePath,
                      binding: {
                        sourceId,
                        versionId: detail.versionId,
                        markdownHash: detail.currentMarkdownHash,
                      },
                      initialTaskId: initialTask?.id ?? null,
                      initialCandidateId: candidateId,
                    });
                  });
                }}
                title={
                  activeSourceAiTask
                    ? t("source.aiOrganize.running")
                    : t("source.aiOrganize.description")
                }
                aria-label={t("source.aiOrganize.label")}
                className="inline-flex h-[28px] items-center gap-1.5 rounded-[var(--radius-sm)] border border-[var(--border)] px-2.5 text-[11.5px] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
              >
                {activeSourceAiTask ? (
                  <LoaderCircle size={13} className="animate-spin" />
                ) : (
                  <Sparkles size={13} />
                )}
                {t("source.aiOrganize.label")}
              </button>
            ) : (
              <button
                type="button"
                disabled={!page}
                onClick={() => {
                  if (page) openWikiAssistant(page.meta.path);
                }}
                title={t("wiki.actions.askAi")}
                aria-label={t("wiki.actions.askAi")}
                className="grid h-[28px] w-[28px] place-items-center rounded-[var(--radius-sm)] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)] disabled:opacity-40"
              >
                <MessageSquareText size={14} />
              </button>
            )}
            <button
              type="button"
              disabled={!page}
              onClick={() => {
                if (page) void toggleBookmark(projectId, rootPath);
              }}
              title={t("wiki.content.star")}
              aria-label={t("wiki.content.star")}
              aria-pressed={page?.meta.bookmarked ?? false}
              className="grid h-[28px] w-[28px] place-items-center rounded-[var(--radius-sm)] text-[var(--text-secondary)] hover:bg-[var(--surface-muted)] disabled:opacity-40"
            >
              <Star
                size={14}
                className={
                  page?.meta.bookmarked
                    ? "fill-[var(--accent)] text-[var(--accent)]"
                    : ""
                }
              />
            </button>
          </div>
        </div>

        {wikiError ? (
          <div role="alert" className="shrink-0 border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
            {wikiError}
          </div>
        ) : null}
        {exportError ? (
          <div role="alert" className="shrink-0 border-b border-[var(--border-subtle)] bg-[var(--warning-soft)] px-4 py-2 text-[12px] text-[var(--text-primary)]">
            {exportError}
          </div>
        ) : null}

        <div className="min-h-0 flex-1 overflow-y-auto">
          {!page ? (
            <div className="flex h-full items-center justify-center text-[13px] text-[var(--text-muted)]">
              {loadingPage ? (
                <LoaderCircle size={16} className="animate-spin" />
              ) : (
                t("wiki.content.noSelection")
              )}
            </div>
          ) : mode === "edit" ? (
            <div className="mx-auto flex h-full max-w-[760px] flex-col px-8 py-6">
              <ViewErrorBoundary>
                <Suspense fallback={<ViewFallback />}>
                  <WikiEditor
                    key={selectedPath ?? undefined}
                    draft={draft}
                    saveState={saveState}
                    onDraftChange={setDraft}
                    onSave={() => void save(projectId, rootPath)}
                    onCancel={cancelEdit}
                    onReload={() => void reload(projectId, rootPath)}
                    onReviewConflict={() => setConflictDialogOpen(true)}
                    disabled={sourceMutating && page.meta.pageType === "source"}
                  />
                </Suspense>
              </ViewErrorBoundary>
            </div>
          ) : mode === "preview" ? (
            <HtmlPreviewPane
              html={pagePreviewHtml}
              outputPath={previewRecord?.outputPath ?? null}
              templateLabel={t(`wiki.html.template.${previewRecord?.exportType ?? "beautiful_read"}.title`)}
              busy={Boolean(currentRunningExportTaskId) || exportStartInFlight}
              onBack={() => setMode("read")}
              onRegenerate={handleRegenerateHtml}
              onOpenFolder={() => {
                if (!previewRecord) return;
                void openFolder({
                  projectId,
                  projectRootPath: rootPath,
                  outputPath: previewRecord.outputPath,
                });
              }}
              onCopyPath={() => {
                if (previewRecord) void navigator.clipboard.writeText(previewRecord.outputPath);
              }}
            />
          ) : (
            <ReadingPane
              page={page}
              pages={tree?.pages ?? []}
              onOpenPage={handleOpen}
              projectId={projectId}
              projectRootPath={rootPath}
            />
          )}
        </div>
      </div>
      {pageForm ? (
        <WikiPageFormDialog
          mode={pageForm.mode}
          initialPath={pageForm.path}
          onCancel={() => setPageForm(null)}
          onSubmit={handlePageFormSubmit}
        />
      ) : null}

      {sourceMovePath ? (
        <SourceMovePathDialog
          currentPath={sourceMovePath}
          onCancel={() => setSourceMovePath(null)}
          onPreview={(path) => {
            setSourceMovePath(null);
            void previewSourceMove(projectId, rootPath, path);
          }}
        />
      ) : null}

      <SourceLifecycleDialogs
        projectId={projectId}
        rootPath={rootPath}
        onMoved={(path) => {
          void reload(projectId, rootPath).then(() => {
            void openPage(projectId, rootPath, path);
          });
        }}
        onDeleted={() => {
          useWikiStore.setState({
            selectedPath: null,
            page: null,
            draft: "",
            mode: "read",
          });
          void scan(projectId, rootPath);
        }}
      />
      {sourceAiWorkbench ? (
        <SourceAiOrganizeDialog
        open
        sourceTitle={sourceAiWorkbench.sourceTitle}
        unsavedEdits={
          mode === "edit" &&
          page?.meta.sourceBinding?.sourceId === sourceAiWorkbench.sourceId &&
          draft !== page.rawMarkdown
        }
        busy={aiOrganizeStarting}
        running={Boolean(
          sourceAiWorkbenchTask &&
            !isTerminalStatus(sourceAiWorkbenchTask.status),
        )}
        agents={capabilities.agents}
        providers={capabilities.providers}
        failedTask={sourceAiWorkbenchFailedTask}
        task={sourceAiWorkbenchTask}
        preview={scopedSourcePreview}
        candidateId={sourceAiWorkbench.initialCandidateId}
        mutating={sourceMutating}
        projectId={sourceAiWorkbench.projectId}
        projectRootPath={sourceAiWorkbench.projectRootPath}
        pagePath={sourceAiWorkbench.pagePath}
        error={
          sourceErrors[sourceAiWorkbench.sourceId] ?? null
        }
        onClose={() => setSourceAiWorkbench(null)}
        onOpenTask={(taskId) => {
          setSourceAiWorkbench(null);
          openTaskDrawer(taskId);
        }}
        onStart={(input) =>
          isWorkbenchProjectCurrent(sourceAiWorkbench)
            ? startSourceAiOrganize(
                sourceAiWorkbench.projectId,
                sourceAiWorkbench.projectRootPath,
                input,
                sourceAiWorkbench.binding,
              )
            : Promise.resolve(null)
        }
        onRetry={(taskId) =>
          isWorkbenchProjectCurrent(sourceAiWorkbench)
            ? retrySourceAiOrganize(
                sourceAiWorkbench.projectId,
                sourceAiWorkbench.projectRootPath,
                taskId,
              )
            : Promise.resolve(null)
        }
        onPreviewCandidate={(candidateId) =>
          isWorkbenchProjectCurrent(sourceAiWorkbench)
            ? previewSourceCandidate(
                sourceAiWorkbench.projectId,
                sourceAiWorkbench.projectRootPath,
                candidateId,
                sourceAiWorkbench.sourceId,
              )
            : Promise.resolve(null)
        }
        onApply={async (candidatePreview) => {
          if (!isWorkbenchProjectCurrent(sourceAiWorkbench)) return false;
          const draftAtStart = useWikiStore.getState().draft;
          const result = await applySourceCandidate(
            sourceAiWorkbench.projectId,
            sourceAiWorkbench.projectRootPath,
            undefined,
            candidatePreview,
          );
          if (!result) return false;
          if (!isWorkbenchProjectCurrent(sourceAiWorkbench)) return true;
          const wikiAfterApply = useWikiStore.getState();
          const draftChangedDuringApply =
            wikiAfterApply.mode === "edit" &&
            wikiAfterApply.page?.meta.sourceBinding?.sourceId ===
              sourceAiWorkbench.sourceId &&
            wikiAfterApply.draft !== draftAtStart;
          if (draftChangedDuringApply) {
            useSourceStore.setState({
              error: t("source.candidate.draftChangedDuringApply"),
              errorSourceId: sourceAiWorkbench.sourceId,
              errorsBySourceId: {
                ...useSourceStore.getState().errorsBySourceId,
                [sourceAiWorkbench.sourceId]: t(
                  "source.candidate.draftChangedDuringApply",
                ),
              },
            });
            return true;
          }
          if (
            wikiAfterApply.page?.meta.sourceBinding?.sourceId ===
            sourceAiWorkbench.sourceId
          ) {
            await reload(
              sourceAiWorkbench.projectId,
              sourceAiWorkbench.projectRootPath,
            );
            await openPage(
              sourceAiWorkbench.projectId,
              sourceAiWorkbench.projectRootPath,
              result.wikiPath,
            );
          } else {
            await scan(
              sourceAiWorkbench.projectId,
              sourceAiWorkbench.projectRootPath,
            );
          }
          return true;
        }}
        onDiscard={(candidateId) =>
          isWorkbenchProjectCurrent(sourceAiWorkbench)
            ? discardSourceCandidate(
                sourceAiWorkbench.projectId,
                sourceAiWorkbench.projectRootPath,
                sourceAiWorkbench.sourceId,
                candidateId,
              )
            : Promise.resolve(false)
        }
      />
      ) : null}
      {pendingLifecycle ? (
        <ConfirmationDialog
          action={pendingLifecycle.action}
          checkpointExists={pendingLifecycle.action.checkpointHash != null}
          onCancel={handleLifecycleCancel}
          onConfirm={handleLifecycleConfirm}
        />
      ) : null}
      {conflict && conflictDialogOpen ? (
        <ConflictDiffDialog
          conflict={conflict}
          onCancel={() => setConflictDialogOpen(false)}
          onKeepCurrent={() => void resolveConflict(projectId, rootPath, "keep_current")}
          onUseIncoming={() => void resolveConflict(projectId, rootPath, "use_incoming")}
          onManualMerge={(content) =>
            void resolveConflict(projectId, rootPath, "manual_merge", content)
          }
        />
      ) : null}
      {htmlDialogOpen && page ? (
        <GenerateHtmlDialog
          pagePath={page.meta.path}
          initialType={htmlTemplate}
          onCancel={handleHtmlDialogCancel}
          onGenerate={handleDialogGenerate}
        />
      ) : null}
      {restrictedExportConfirmation ? (
        <ExportRestrictedContentDialog
          count={restrictedExportConfirmation.count}
          onCancel={handleRestrictedExportCancel}
          onConfirm={handleRestrictedExportConfirm}
        />
      ) : null}
    </div>
  );
}

const ReadingPane = memo(function ReadingPane({
  page,
  pages,
  onOpenPage,
  projectId,
  projectRootPath,
}: {
  page: WikiPageContent;
  pages: WikiPageMeta[];
  onOpenPage: (path: string) => void;
  projectId: string;
  projectRootPath: string;
}) {
  return (
    <div className="flex justify-center px-8 py-6">
      <div className="w-full max-w-[760px]">
        <MarkdownReader
          bodyMarkdown={page.bodyMarkdown}
          frontmatterYaml={page.frontmatterYaml}
          pages={pages}
          onOpenPage={onOpenPage}
          projectId={projectId}
          projectRootPath={projectRootPath}
          pagePath={page.meta.path}
        />
      </div>
    </div>
  );
});

function ModeButton({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={`inline-flex h-[26px] items-center gap-1 px-2 text-[11.5px] font-medium transition-colors ${
        active
          ? "bg-[var(--surface-muted)] text-[var(--text-primary)]"
          : "bg-transparent text-[var(--text-muted)] hover:text-[var(--text-secondary)]"
      }`}
    >
      {icon}
      {label}
    </button>
  );
}

export function sameProjectRoot(
  stored: string | null | undefined,
  current: string,
): boolean {
  if (!stored) return false;
  const normalize = (value: string) => {
    const normalized = value.replaceAll("\\", "/").replace(/\/+$/, "");
    return /^[a-z]:\//i.test(normalized) || normalized.startsWith("//")
      ? normalized.toLocaleLowerCase("en-US")
      : normalized;
  };
  return normalize(stored) === normalize(current);
}

function isCurrentWikiQuickExport(
  scope: Pick<PendingWikiQuickExport, "projectId" | "projectRootPath" | "pagePath">,
): boolean {
  const project = useProjectStore.getState().currentProject;
  const page = useWikiStore.getState().page;
  const selectedPath = useWikiStore.getState().selectedPath;
  return (
    project.projectId === scope.projectId &&
    sameProjectRoot(project.rootPath, scope.projectRootPath) &&
    page?.meta.path === scope.pagePath &&
    selectedPath === scope.pagePath
  );
}

export function selectSourceAiWorkbenchTask(
  tasks: BackendTask[],
  projectId: string,
  projectRootPath: string,
  sourceId: string,
  currentVersionId: string,
  currentMarkdownHash: string,
  candidateId: string | null,
): BackendTask | null {
  const matchingTasks = tasks
    .filter(
      (task) =>
        task.taskType === "source_ai_organize" &&
        task.projectId === projectId &&
        task.result?.reference?.type === "source_ai_organize" &&
        sameProjectRoot(
          task.result.reference.projectRootPath,
          projectRootPath,
        ) &&
        task.result.reference.sourceId === sourceId,
    )
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  return (
    matchingTasks.find((task) => !isTerminalStatus(task.status)) ??
    matchingTasks.find(
      (task) =>
        (task.status === "failed" || task.status === "cancelled") &&
        task.result?.reference?.type === "source_ai_organize" &&
        task.result.reference.baseVersionId === currentVersionId &&
        task.result.reference.baseMarkdownHash === currentMarkdownHash,
    ) ??
    matchingTasks.find(
      (task) =>
        task.status === "succeeded" &&
        task.result?.reference?.type === "source_ai_organize" &&
        task.result.reference.candidateId === candidateId,
    ) ??
    null
  );
}

function isWorkbenchProjectCurrent(scope: SourceAiWorkbenchScope): boolean {
  const current = useProjectStore.getState().currentProject;
  return (
    current.projectId === scope.projectId &&
    sameProjectRoot(current.rootPath, scope.projectRootPath)
  );
}
