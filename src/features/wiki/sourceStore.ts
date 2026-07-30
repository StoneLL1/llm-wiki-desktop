import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

import { captureProjectScope, isProjectScopeCurrent } from "../../stores/projectScope";
import { useTaskStore } from "../../stores/taskStore";
import type {
  DeleteSourcePreview,
  MoveSourcePreview,
  SourceCandidateKind,
  SourceCandidateSummary,
  SourceDetail,
  SourceAiOrganizeBinding,
  SourceMutationResult,
  SourceUpdatePreview,
  StartSourceAiOrganizeInput,
} from "../../types/source";
import type { BackendTask } from "../../types/task";

type SourceReprocessKind = Exclude<SourceCandidateKind, "ai_organize">;

interface SourceState {
  detail: SourceDetail | null;
  updatePreview: SourceUpdatePreview | null;
  deletePreview: DeleteSourcePreview | null;
  movePreview: MoveSourcePreview | null;
  loading: boolean;
  mutating: boolean;
  aiOrganizeStarting: boolean;
  aiOrganizeStartToken: number | null;
  error: string | null;
  errorSourceId: string | null;
  errorsBySourceId: Record<string, string>;
  requestEpoch: number;
  loadDetail: (
    projectId: string,
    rootPath: string,
    sourceId: string,
    refreshToken?: string,
  ) => Promise<void>;
  reprocess: (
    projectId: string,
    rootPath: string,
    kind: SourceReprocessKind,
    subtitlePath?: string,
  ) => Promise<SourceCandidateSummary | null>;
  startAiOrganize: (
    projectId: string,
    rootPath: string,
    input: StartSourceAiOrganizeInput,
    binding?: SourceAiOrganizeBinding,
  ) => Promise<BackendTask | null>;
  retryAiOrganize: (
    projectId: string,
    rootPath: string,
    taskId: string,
  ) => Promise<BackendTask | null>;
  previewCandidate: (
    projectId: string,
    rootPath: string,
    candidateId: string,
    sourceId?: string,
  ) => Promise<SourceUpdatePreview | null>;
  applyCandidate: (
    projectId: string,
    rootPath: string,
    mergedMarkdown?: string,
    candidatePreview?: SourceUpdatePreview,
  ) => Promise<SourceMutationResult | null>;
  discardCandidate: (
    projectId: string,
    rootPath: string,
    sourceId?: string,
    candidateId?: string,
  ) => Promise<boolean>;
  restoreVersion: (
    projectId: string,
    rootPath: string,
    versionId: string,
  ) => Promise<SourceMutationResult | null>;
  previewMove: (
    projectId: string,
    rootPath: string,
    newWikiPath: string,
  ) => Promise<void>;
  confirmMove: (
    projectId: string,
    rootPath: string,
  ) => Promise<SourceMutationResult | null>;
  previewDelete: (projectId: string, rootPath: string) => Promise<void>;
  confirmDelete: (
    projectId: string,
    rootPath: string,
    confirmationText: string,
  ) => Promise<SourceMutationResult | null>;
  clearUpdatePreview: () => void;
  clearMovePreview: () => void;
  clearDeletePreview: () => void;
  reset: () => void;
}

const initial = {
  detail: null as SourceDetail | null,
  updatePreview: null as SourceUpdatePreview | null,
  deletePreview: null as DeleteSourcePreview | null,
  movePreview: null as MoveSourcePreview | null,
  loading: false,
  mutating: false,
  aiOrganizeStarting: false,
  aiOrganizeStartToken: null as number | null,
  error: null as string | null,
  errorSourceId: null as string | null,
  errorsBySourceId: {} as Record<string, string>,
  requestEpoch: 0,
};

let aiOrganizeStartSerial = 0;
const nextAiOrganizeStartToken = () => {
  aiOrganizeStartSerial += 1;
  return aiOrganizeStartSerial;
};
// Keep the existing epoch semantics for stale responses while coalescing
// StrictMode's duplicate mount effects into one backend detail read.
interface ActiveSourceDetailRequest {
  promise: Promise<SourceDetail>;
  refreshToken: string | null;
}

const sourceDetailRequests = new Map<string, ActiveSourceDetailRequest>();

function requestSourceDetail(
  projectId: string,
  rootPath: string,
  sourceId: string,
  refreshToken?: string,
): Promise<SourceDetail> {
  const key = JSON.stringify([projectId, rootPath, sourceId]);
  const existing = sourceDetailRequests.get(key);
  // A completion token represents a newer backend snapshot. It replaces an
  // older active read, while StrictMode replays carrying the same token still
  // share one request. Ordinary callers always join the newest active read.
  if (
    existing &&
    (refreshToken === undefined || existing.refreshToken === refreshToken)
  ) {
    return existing.promise;
  }

  const request = invoke<SourceDetail>("get_source_detail", {
    request: { projectId, projectRootPath: rootPath, sourceId },
  });
  const active = {
    promise: request,
    refreshToken: refreshToken ?? null,
  };
  sourceDetailRequests.set(key, active);
  const clear = () => {
    if (sourceDetailRequests.get(key) === active) {
      sourceDetailRequests.delete(key);
    }
  };
  void request.then(clear, clear);
  return request;
}

export const useSourceStore = create<SourceState>((set, get) => ({
  ...initial,
  loadDetail: async (projectId, rootPath, sourceId, refreshToken) => {
    const scope = captureProjectScope();
    const epoch = get().requestEpoch + 1;
    set({
      requestEpoch: epoch,
      loading: true,
      ...scopedSourceError(get().errorsBySourceId, sourceId, null),
      detail: null,
      updatePreview: null,
      movePreview: null,
      deletePreview: null,
    });
    try {
      const detail = await requestSourceDetail(
        projectId,
        rootPath,
        sourceId,
        refreshToken,
      );
      if (!isProjectScopeCurrent(scope) || get().requestEpoch !== epoch) return;
      set({ detail, loading: false });
    } catch (error) {
      if (!isProjectScopeCurrent(scope) || get().requestEpoch !== epoch) return;
      set({
        loading: false,
        ...scopedSourceError(
          get().errorsBySourceId,
          sourceId,
          errorMessage(error),
        ),
      });
    }
  },
  reprocess: async (projectId, rootPath, kind, subtitlePath) => {
    const scope = captureProjectScope();
    const detail = get().detail;
    if (!detail) return null;
    const command = {
      ocr: "reprocess_source_ocr",
      asr: "reprocess_source_asr",
      subtitle: "reprocess_source_subtitle",
      refresh: "refresh_source",
    }[kind];
    set({
      mutating: true,
      ...scopedSourceError(get().errorsBySourceId, detail.sourceId, null),
    });
    try {
      const candidate = await invoke<SourceCandidateSummary>(command, {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: detail.sourceId,
          expectedMarkdownHash: detail.currentMarkdownHash,
          subtitlePath: subtitlePath ?? null,
        },
      });
      if (!isProjectScopeCurrent(scope) || get().detail?.sourceId !== detail.sourceId) {
        return null;
      }
      set({
        detail: { ...detail, candidate, status: "candidate_ready" },
        mutating: false,
      });
      await get().previewCandidate(projectId, rootPath, candidate.candidateId);
      return candidate;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({
        mutating: false,
        ...scopedSourceError(
          get().errorsBySourceId,
          detail.sourceId,
          errorMessage(error),
        ),
      });
      return null;
    }
  },
  startAiOrganize: async (projectId, rootPath, input, binding) => {
    const scope = captureProjectScope();
    const detail = get().detail;
    const target =
      binding ??
      (detail
        ? {
            sourceId: detail.sourceId,
            versionId: detail.versionId,
            markdownHash: detail.currentMarkdownHash,
          }
        : null);
    if (!target) return null;
    const token = nextAiOrganizeStartToken();
    set({
      aiOrganizeStarting: true,
      aiOrganizeStartToken: token,
      ...scopedSourceError(get().errorsBySourceId, target.sourceId, null),
    });
    try {
      const task = await invoke<BackendTask>("start_source_ai_organize", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: target.sourceId,
          expectedVersionId: target.versionId,
          expectedMarkdownHash: target.markdownHash,
          route: input.route,
          agent: input.agent,
          provider: input.provider,
          customInstructions: input.customInstructions,
        },
      });
      if (isProjectScopeCurrent(scope)) {
        useTaskStore.getState().upsertTask(task);
      }
      return task;
    } catch (error) {
      if (isProjectScopeCurrent(scope)) {
        set(
          scopedSourceError(
            get().errorsBySourceId,
            target.sourceId,
            errorMessage(error),
          ),
        );
      }
      return null;
    } finally {
      if (get().aiOrganizeStartToken === token) {
        set({ aiOrganizeStarting: false, aiOrganizeStartToken: null });
      }
    }
  },
  retryAiOrganize: async (projectId, rootPath, taskId) => {
    const scope = captureProjectScope();
    const reference = useTaskStore
      .getState()
      .tasks.find((task) => task.id === taskId)?.result?.reference;
    const sourceId =
      reference?.type === "source_ai_organize" ? reference.sourceId : null;
    const token = nextAiOrganizeStartToken();
    set({
      aiOrganizeStarting: true,
      aiOrganizeStartToken: token,
      ...scopedSourceError(get().errorsBySourceId, sourceId, null),
    });
    try {
      const task = await invoke<BackendTask>("retry_source_ai_organize", {
        request: {
          projectId,
          projectRootPath: rootPath,
          taskId,
        },
      });
      if (isProjectScopeCurrent(scope)) {
        useTaskStore.getState().upsertTask(task);
      }
      return task;
    } catch (error) {
      if (isProjectScopeCurrent(scope)) {
        set(
          scopedSourceError(
            get().errorsBySourceId,
            sourceId,
            errorMessage(error),
          ),
        );
      }
      return null;
    } finally {
      if (get().aiOrganizeStartToken === token) {
        set({ aiOrganizeStarting: false, aiOrganizeStartToken: null });
      }
    }
  },
  previewCandidate: async (projectId, rootPath, candidateId, sourceId) => {
    const scope = captureProjectScope();
    const detail = get().detail;
    const targetSourceId = sourceId ?? detail?.sourceId;
    if (!targetSourceId) return null;
    if (detail?.sourceId === targetSourceId) {
      set({
        loading: true,
        ...scopedSourceError(get().errorsBySourceId, targetSourceId, null),
      });
    }
    try {
      const updatePreview = await invoke<SourceUpdatePreview>("preview_source_update", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: targetSourceId,
          candidateId,
        },
      });
      if (!isProjectScopeCurrent(scope)) return null;
      if (get().detail?.sourceId === targetSourceId) {
        set({ updatePreview, loading: false });
      }
      return updatePreview;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      if (get().detail?.sourceId === targetSourceId) {
        set({
          loading: false,
          ...scopedSourceError(
            get().errorsBySourceId,
            targetSourceId,
            errorMessage(error),
          ),
        });
      } else {
        set(
          scopedSourceError(
            get().errorsBySourceId,
            targetSourceId,
            errorMessage(error),
          ),
        );
      }
      return null;
    }
  },
  applyCandidate: async (
    projectId,
    rootPath,
    mergedMarkdown,
    candidatePreview,
  ) => {
    const scope = captureProjectScope();
    const preview = candidatePreview ?? get().updatePreview;
    if (!preview) return null;
    const targetSourceId = preview.sourceId;
    set({
      mutating: true,
      ...scopedSourceError(get().errorsBySourceId, targetSourceId, null),
    });
    try {
      const result = await invoke<SourceMutationResult>("apply_source_candidate", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: targetSourceId,
          candidateId: preview.candidateId,
          guardToken: preview.guardToken,
          mergedMarkdown: mergedMarkdown ?? null,
        },
      });
      if (!isProjectScopeCurrent(scope)) return null;
      if (get().detail?.sourceId === targetSourceId) {
        set({ updatePreview: null });
        await get().loadDetail(projectId, rootPath, result.sourceId);
        if (!isProjectScopeCurrent(scope)) return null;
      }
      set({ mutating: false });
      return result;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({
        mutating: false,
        ...scopedSourceError(
          get().errorsBySourceId,
          targetSourceId,
          errorMessage(error),
        ),
      });
      return null;
    }
  },
  discardCandidate: async (
    projectId,
    rootPath,
    sourceId,
    candidateIdOverride,
  ) => {
    const scope = captureProjectScope();
    const detail = get().detail;
    const targetSourceId = sourceId ?? detail?.sourceId;
    const candidateId =
      candidateIdOverride ??
      get().updatePreview?.candidateId ??
      detail?.candidate?.candidateId;
    if (!targetSourceId || !candidateId) return false;
    set({
      mutating: true,
      ...scopedSourceError(get().errorsBySourceId, targetSourceId, null),
    });
    try {
      await invoke("discard_source_candidate", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: targetSourceId,
          candidateId,
        },
      });
      if (!isProjectScopeCurrent(scope)) return false;
      if (get().detail?.sourceId === targetSourceId) {
        set({ mutating: false, updatePreview: null });
        await get().loadDetail(projectId, rootPath, targetSourceId);
      } else {
        set({ mutating: false });
      }
      return true;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return false;
      set({
        mutating: false,
        ...scopedSourceError(
          get().errorsBySourceId,
          targetSourceId,
          errorMessage(error),
        ),
      });
      return false;
    }
  },
  restoreVersion: async (projectId, rootPath, versionId) => {
    const scope = captureProjectScope();
    const detail = get().detail;
    if (!detail) return null;
    set({
      mutating: true,
      ...scopedSourceError(get().errorsBySourceId, detail.sourceId, null),
    });
    try {
      const result = await invoke<SourceMutationResult>("restore_source_version", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: detail.sourceId,
          versionId,
          expectedMarkdownHash: detail.currentMarkdownHash,
        },
      });
      if (!isProjectScopeCurrent(scope)) return null;
      set({ mutating: false });
      await get().loadDetail(projectId, rootPath, result.sourceId);
      return result;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({
        mutating: false,
        ...scopedSourceError(
          get().errorsBySourceId,
          detail.sourceId,
          errorMessage(error),
        ),
      });
      return null;
    }
  },
  previewMove: async (projectId, rootPath, newWikiPath) => {
    const scope = captureProjectScope();
    const detail = get().detail;
    if (!detail) return;
    set({
      loading: true,
      ...scopedSourceError(get().errorsBySourceId, detail.sourceId, null),
    });
    try {
      const movePreview = await invoke<MoveSourcePreview>("preview_move_source", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: detail.sourceId,
          newWikiPath,
        },
      });
      if (!isProjectScopeCurrent(scope) || get().detail?.sourceId !== detail.sourceId) return;
      set({ movePreview, loading: false });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({
        loading: false,
        ...scopedSourceError(
          get().errorsBySourceId,
          detail.sourceId,
          errorMessage(error),
        ),
      });
    }
  },
  confirmMove: async (projectId, rootPath) => {
    const scope = captureProjectScope();
    const preview = get().movePreview;
    if (!preview) return null;
    set({
      mutating: true,
      ...scopedSourceError(get().errorsBySourceId, preview.sourceId, null),
    });
    try {
      const result = await invoke<SourceMutationResult>("move_source", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: preview.sourceId,
          newWikiPath: preview.newWikiPath,
          guardToken: preview.guardToken,
        },
      });
      if (!isProjectScopeCurrent(scope)) return null;
      set({ mutating: false, movePreview: null });
      await get().loadDetail(projectId, rootPath, result.sourceId);
      return result;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({
        mutating: false,
        ...scopedSourceError(
          get().errorsBySourceId,
          preview.sourceId,
          errorMessage(error),
        ),
      });
      return null;
    }
  },
  previewDelete: async (projectId, rootPath) => {
    const scope = captureProjectScope();
    const detail = get().detail;
    if (!detail) return;
    set({
      loading: true,
      ...scopedSourceError(get().errorsBySourceId, detail.sourceId, null),
    });
    try {
      const deletePreview = await invoke<DeleteSourcePreview>("preview_delete_source", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: detail.sourceId,
        },
      });
      if (!isProjectScopeCurrent(scope) || get().detail?.sourceId !== detail.sourceId) return;
      set({ deletePreview, loading: false });
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return;
      set({
        loading: false,
        ...scopedSourceError(
          get().errorsBySourceId,
          detail.sourceId,
          errorMessage(error),
        ),
      });
    }
  },
  confirmDelete: async (projectId, rootPath, confirmationText) => {
    const scope = captureProjectScope();
    const preview = get().deletePreview;
    if (!preview) return null;
    set({
      mutating: true,
      ...scopedSourceError(get().errorsBySourceId, preview.sourceId, null),
    });
    try {
      const result = await invoke<SourceMutationResult>("delete_source", {
        request: {
          projectId,
          projectRootPath: rootPath,
          sourceId: preview.sourceId,
          guardToken: preview.guardToken,
          confirmationText,
        },
      });
      if (!isProjectScopeCurrent(scope)) return null;
      set({ ...initial });
      return result;
    } catch (error) {
      if (!isProjectScopeCurrent(scope)) return null;
      set({
        mutating: false,
        ...scopedSourceError(
          get().errorsBySourceId,
          preview.sourceId,
          errorMessage(error),
        ),
      });
      return null;
    }
  },
  clearUpdatePreview: () => set({ updatePreview: null }),
  clearMovePreview: () => set({ movePreview: null }),
  clearDeletePreview: () => set({ deletePreview: null }),
  reset: () => {
    sourceDetailRequests.clear();
    set((state) => ({
      ...initial,
      requestEpoch: state.requestEpoch + 1,
    }));
  },
}));

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

function scopedSourceError(
  current: Record<string, string>,
  sourceId: string | null,
  error: string | null,
): Pick<SourceState, "error" | "errorSourceId" | "errorsBySourceId"> {
  const errorsBySourceId = { ...current };
  if (sourceId) {
    if (error) {
      errorsBySourceId[sourceId] = error;
    } else {
      delete errorsBySourceId[sourceId];
    }
  }
  return {
    error,
    errorSourceId: sourceId,
    errorsBySourceId,
  };
}
