import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

import type { TaskLauncher } from "../../hooks/useTaskLauncher";
import { waitForTaskTerminal } from "../../lib/waitForTaskTerminal";
import { useImportStore } from "../../stores/importStore";
import type { AppView } from "../../stores/navigationStore";
import { useProjectStore } from "../../stores/projectStore";
import { useTaskStore } from "../../stores/taskStore";
import { useToastStore } from "../../stores/toastStore";
import type { PendingAction } from "../../types/backend";
import type {
  ConfirmedImport,
  FetchedImportUrl,
  ImportedSource,
  ImportPreview,
} from "../../types/import";
import type { ProjectSummary } from "../../types/project";
import type { BackendTask } from "../../types/task";
import { useWikiStore } from "../wiki/wikiStore";

export interface ImportWorkflow {
  importedSources: ImportedSource[];
  isConfirming: boolean;
  requestPreview: (paths: string[]) => void;
  requestClipboard: (content: string) => Promise<void>;
  requestUrl: (url: string) => Promise<void>;
  requestDeleteSource: (path: string) => Promise<void>;
  requestReplaceSource: (
    path: string,
    replacementPath: string,
  ) => Promise<void>;
  confirm: (options: {
    createCheckpoint: boolean;
    compileAfterImport: boolean;
  }) => void;
}

const hasTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

export function useImportWorkflow(
  project: ProjectSummary,
  activeView: AppView,
  taskLauncher: TaskLauncher,
): ImportWorkflow {
  const { t } = useTranslation();
  const projectId = project.projectId;
  const rootPath = project.rootPath;
  const sourceCount = project.sourceCount;
  const projectKey = `${projectId}\0${rootPath}`;
  const latestProjectKey = useRef(projectKey);
  const previewEpoch = useRef(0);
  if (latestProjectKey.current !== projectKey) {
    previewEpoch.current += 1;
  }
  latestProjectKey.current = projectKey;
  const importedSources = useImportStore((state) => state.importedSources);
  const isConfirming = useImportStore((state) => state.isConfirming);
  const setPreview = useImportStore((state) => state.setPreview);
  const setImportedSources = useImportStore((state) => state.setImportedSources);
  const setIsConfirming = useImportStore((state) => state.setIsConfirming);
  const setPendingAction = useProjectStore((state) => state.setPendingAction);
  const upsertTask = useTaskStore((state) => state.upsertTask);
  const openTaskDrawer = useTaskStore((state) => state.openDrawer);
  const pushToast = useToastStore((state) => state.pushToast);
  const startCompile = taskLauncher.startCompile;

  const isCurrent = useCallback(
    (requestKey: string) => latestProjectKey.current === requestKey,
    [],
  );
  const beginPreview = useCallback(
    () => ({ requestKey: projectKey, epoch: ++previewEpoch.current }),
    [projectKey],
  );
  const isPreviewCurrent = useCallback(
    (requestKey: string, epoch: number) =>
      latestProjectKey.current === requestKey && previewEpoch.current === epoch,
    [],
  );

  useEffect(() => {
    useImportStore.getState().reset();
    setImportedSources([]);
  }, [projectKey, setImportedSources]);

  useEffect(() => {
    if (!hasTauri() || activeView !== "import" || !projectId) return;
    const requestKey = projectKey;
    void invoke<ImportedSource[]>("list_imported_sources", {
      request: { projectId, projectRootPath: rootPath },
    })
      .then((sources) => {
        if (isCurrent(requestKey)) setImportedSources(sources);
      })
      .catch((error) => {
        if (isCurrent(requestKey)) {
          pushToast(
            "error",
            t("import.sourceListError", { message: errorMessage(error) }),
          );
        }
      });
  }, [
    activeView,
    isCurrent,
    projectId,
    projectKey,
    pushToast,
    rootPath,
    setImportedSources,
    sourceCount,
    t,
  ]);

  const requestPreview = useCallback(
    (paths: string[]) => {
      const sourcePaths = paths
        .map((path) => path.trim())
        .filter((path) => path.length > 0);
      if (sourcePaths.length === 0) {
        previewEpoch.current += 1;
        setPreview(null);
        return;
      }

      const { requestKey, epoch } = beginPreview();
      void (async () => {
        try {
          const started = await invoke<BackendTask>("preview_import", {
            request: {
              projectId,
              projectRootPath: rootPath,
              sourcePaths,
              allowDuplicates: false,
              linkDuplicates: false,
            },
          });
          upsertTask(started);
          if (!isPreviewCurrent(requestKey, epoch)) return;
          openTaskDrawer(started.id);

          const terminal = await waitForTaskTerminal(started);
          upsertTask(terminal);
          if (!isPreviewCurrent(requestKey, epoch)) return;
          if (terminal.status !== "succeeded") {
            throw new Error(
              terminal.error?.message ?? `Import preview ${terminal.status}.`,
            );
          }

          const nextPreview = await invoke<ImportPreview>("get_import_preview", {
            request: {
              projectId,
              projectRootPath: rootPath,
              taskId: terminal.id,
            },
          });
          if (isPreviewCurrent(requestKey, epoch)) setPreview(nextPreview);
        } catch (error) {
          if (!isPreviewCurrent(requestKey, epoch)) return;
          setPreview(null);
          pushToast(
            "error",
            t("import.previewError", { message: errorMessage(error) }),
          );
        }
      })();
    },
    [
      beginPreview,
      isPreviewCurrent,
      openTaskDrawer,
      projectId,
      projectKey,
      pushToast,
      rootPath,
      setPreview,
      t,
      upsertTask,
    ],
  );

  const requestTextPreview = useCallback(
    async (kind: "clipboard" | "url", value: string) => {
      const { requestKey, epoch } = beginPreview();
      try {
        let content = value;
        let sourceName = "clipboard-import";
        let title: string | null = null;
        let author: string | null = null;

        if (kind === "url") {
          const fetched = await invoke<FetchedImportUrl>("fetch_import_url", {
            request: { projectId, projectRootPath: rootPath, url: value },
          });
          if (!isPreviewCurrent(requestKey, epoch)) return;
          const { extractArticleFromHtml, articleToMarkdown } = await import(
            "../../lib/readability"
          );
          if (!isPreviewCurrent(requestKey, epoch)) return;
          const article = extractArticleFromHtml(fetched.html, fetched.url);
          if (!article) throw new Error(t("import.readabilityError"));
          content = articleToMarkdown(article, fetched.url);
          sourceName = article.title || new URL(fetched.url).hostname;
          title = article.title || null;
          author = article.byline;
        }

        const nextPreview = await invoke<ImportPreview>("preview_text_import", {
          request: {
            projectId,
            projectRootPath: rootPath,
            kind,
            sourceName,
            content,
            title,
            author,
          },
        });
        if (isPreviewCurrent(requestKey, epoch)) setPreview(nextPreview);
      } catch (error) {
        if (!isPreviewCurrent(requestKey, epoch)) return;
        setPreview(null);
        pushToast(
          "error",
          t("import.previewError", { message: errorMessage(error) }),
        );
      }
    },
    [beginPreview, isPreviewCurrent, projectId, pushToast, rootPath, setPreview, t],
  );

  const requestSourceAction = useCallback(
    async (
      kind: "delete" | "replace",
      targetPath: string,
      replacementPath?: string,
    ) => {
      const requestKey = projectKey;
      try {
        const action = await invoke<PendingAction>(
          kind === "delete"
            ? "request_delete_source"
            : "request_replace_source",
          {
            request: {
              projectId,
              projectRootPath: rootPath,
              targetPath,
              ...(kind === "replace" ? { replacementPath } : {}),
            },
          },
        );
        if (isCurrent(requestKey)) setPendingAction(action);
      } catch (error) {
        if (isCurrent(requestKey)) {
          pushToast(
            "error",
            t("import.sourceActionError", { message: errorMessage(error) }),
          );
        }
      }
    },
    [
      isCurrent,
      projectId,
      projectKey,
      pushToast,
      rootPath,
      setPendingAction,
      t,
    ],
  );

  const confirm = useCallback(
    (options: {
      createCheckpoint: boolean;
      compileAfterImport: boolean;
    }) => {
      const stagedPreview = useImportStore.getState().preview;
      if (!stagedPreview) return;
      const requestKey = projectKey;
      setIsConfirming(true);

      void (async () => {
        try {
          await invoke<ConfirmedImport>("confirm_import_preview", {
            request: {
              projectId,
              projectRootPath: rootPath,
              preview: stagedPreview,
              createCheckpoint: options.createCheckpoint,
            },
          });
          if (!isCurrent(requestKey)) return;
          setPreview(null);

          await useWikiStore.getState().scan(projectId, rootPath);
          if (!isCurrent(requestKey)) return;
          if (options.compileAfterImport) {
            await startCompile();
          }
        } catch (error) {
          if (isCurrent(requestKey)) {
            pushToast(
              "error",
              t("import.confirmError", { message: errorMessage(error) }),
            );
          }
        } finally {
          if (isCurrent(requestKey)) setIsConfirming(false);
        }
      })();
    },
    [
      isCurrent,
      projectId,
      projectKey,
      pushToast,
      rootPath,
      setIsConfirming,
      setPreview,
      startCompile,
      t,
    ],
  );

  return {
    importedSources,
    isConfirming,
    requestPreview,
    requestClipboard: (content) => requestTextPreview("clipboard", content),
    requestUrl: (url) => requestTextPreview("url", url),
    requestDeleteSource: (path) => requestSourceAction("delete", path),
    requestReplaceSource: (path, replacementPath) =>
      requestSourceAction("replace", path, replacementPath),
    confirm,
  };
}
