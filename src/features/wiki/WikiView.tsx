import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Book, Edit2, LoaderCircle, Star } from "lucide-react";

import { useProjectStore } from "../../stores/projectStore";
import { ConfirmationDialog } from "../../components/app/ConfirmationDialog";
import type { PendingAction } from "../../types/backend";
import type { CreateWikiPageInput, WikiPageContent, WikiPageMeta } from "../../types/wiki";
import { MarkdownReader } from "./MarkdownReader";
import { WikiEditor } from "./WikiEditor";
import { WikiPageFormDialog } from "./WikiPageFormDialog";
import { WikiTree } from "./WikiTree";
import { useWikiStore } from "./wikiStore";

export function WikiView() {
  const { t } = useTranslation();
  const currentProject = useProjectStore((state) => state.currentProject);
  const [pageForm, setPageForm] = useState<
    { mode: "create" | "rename"; path: string } | null
  >(null);
  const [pendingLifecycle, setPendingLifecycle] = useState<
    | { kind: "rename"; action: PendingAction; oldPath: string; newPath: string }
    | { kind: "delete"; action: PendingAction }
    | null
  >(null);

  const tree = useWikiStore((state) => state.tree);
  const loadingTree = useWikiStore((state) => state.loadingTree);
  const selectedPath = useWikiStore((state) => state.selectedPath);
  const page = useWikiStore((state) => state.page);
  const mode = useWikiStore((state) => state.mode);
  const draft = useWikiStore((state) => state.draft);
  const saveState = useWikiStore((state) => state.saveState);
  const loadingPage = useWikiStore((state) => state.loadingPage);
  const scan = useWikiStore((state) => state.scan);
  const openPage = useWikiStore((state) => state.openPage);
  const startEdit = useWikiStore((state) => state.startEdit);
  const setMode = useWikiStore((state) => state.setMode);
  const setDraft = useWikiStore((state) => state.setDraft);
  const save = useWikiStore((state) => state.save);
  const cancelEdit = useWikiStore((state) => state.cancelEdit);
  const reload = useWikiStore((state) => state.reload);
  const toggleBookmark = useWikiStore((state) => state.toggleBookmark);
  const createPage = useWikiStore((state) => state.createPage);
  const renamePage = useWikiStore((state) => state.renamePage);
  const requestDeletePage = useWikiStore((state) => state.requestDeletePage);
  const confirmDeletePage = useWikiStore((state) => state.confirmDeletePage);
  const cancelPendingAction = useWikiStore((state) => state.cancelPendingAction);

  const { projectId, rootPath } = currentProject;

  useEffect(() => {
    void scan(projectId, rootPath);
  }, [projectId, rootPath, scan]);

  const handleOpen = (path: string) => {
    void openPage(projectId, rootPath, path);
  };

  const breadcrumbs = selectedPath ? selectedPath.split("/") : [];

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
    <div className="flex h-full min-h-0">
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
        />
      ) : (
        <div className="flex w-[260px] flex-col items-center justify-center border-r border-[var(--border)] bg-[var(--surface)] text-[12px] text-[var(--text-muted)]">
          {loadingTree ? (
            <LoaderCircle size={16} className="animate-spin" />
          ) : (
            t("wiki.tree.empty")
          )}
        </div>
      )}

      <div className="flex min-w-0 flex-1 flex-col bg-[var(--background)]">
        <div className="flex h-[44px] shrink-0 items-center gap-2 border-b border-[var(--border)] px-5">
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
            </div>
            <button
              type="button"
              disabled={!page}
              onClick={() => {
                if (page) void toggleBookmark(projectId, rootPath);
              }}
              title={t("wiki.content.star")}
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
              <WikiEditor
                key={selectedPath ?? undefined}
                draft={draft}
                saveState={saveState}
                onDraftChange={setDraft}
                onSave={() => void save(projectId, rootPath)}
                onCancel={cancelEdit}
                onReload={() => void reload(projectId, rootPath)}
              />
            </div>
          ) : (
            <ReadingPane
              page={page}
              pages={tree?.pages ?? []}
              onOpenPage={handleOpen}
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
      {pendingLifecycle ? (
        <ConfirmationDialog
          action={pendingLifecycle.action}
          checkpointExists={pendingLifecycle.action.checkpointHash != null}
          onCancel={handleLifecycleCancel}
          onConfirm={handleLifecycleConfirm}
        />
      ) : null}
    </div>
  );
}

function ReadingPane({
  page,
  pages,
  onOpenPage,
}: {
  page: WikiPageContent;
  pages: WikiPageMeta[];
  onOpenPage: (path: string) => void;
}) {
  return (
    <div className="flex justify-center px-8 py-6">
      <div className="w-full max-w-[760px]">
        <MarkdownReader
          bodyMarkdown={page.bodyMarkdown}
          frontmatterYaml={page.frontmatterYaml}
          pages={pages}
          onOpenPage={onOpenPage}
        />
      </div>
    </div>
  );
}

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
