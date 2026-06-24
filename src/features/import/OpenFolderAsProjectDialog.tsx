import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Folder, FolderOpen, X } from "lucide-react";
import type { OpenProjectResponse, ProjectTemplate } from "../../types/project";
import { useProjectStore } from "../../stores/projectStore";
import { useToastStore } from "../../stores/toastStore";
import { useModalDialog } from "../../hooks/useModalDialog";

function describeError(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

interface OpenFolderAsProjectDialogProps {
  open: boolean;
  onClose: () => void;
}

const TEMPLATES: ProjectTemplate[] = ["research", "general", "reading"];

export function OpenFolderAsProjectDialog({ open, onClose }: OpenFolderAsProjectDialogProps) {
  const { t } = useTranslation();
  const [path, setPath] = useState("");
  const [template, setTemplate] = useState<ProjectTemplate>("general");
  const [byType, setByType] = useState(true);
  const [rename, setRename] = useState(true);
  const [initGit, setInitGit] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useModalDialog({ open, onClose, initialFocusRef: inputRef });

  const setPendingAction = useProjectStore((state) => state.setPendingAction);
  const setCurrentProject = useProjectStore((state) => state.setCurrentProject);
  const pushToast = useToastStore((state) => state.pushToast);

  useEffect(() => {
    if (!open) return;
    setPath("");
    setSubmitting(false);
  }, [open]);

  if (!open) return null;

  const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  const submit = async () => {
    const trimmed = path.trim();
    if (!trimmed) return;
    if (!hasTauri) {
      pushToast("info", t("view.import.actionPrimary"));
      onClose();
      return;
    }
    setSubmitting(true);
    try {
      const response = await invoke<OpenProjectResponse>("preview_open_folder_as_project", {
        request: { path: trimmed },
      });
      if (response.kind === "needs_confirmation" && response.pendingAction) {
        setPendingAction(response.pendingAction);
      } else if (response.summary) {
        setCurrentProject(response.summary);
        pushToast("info", t("import.folderDialog.title"));
      }
      onClose();
    } catch (error) {
      pushToast("error", t("import.previewError", { message: describeError(error) }));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      ref={dialogRef}
      tabIndex={-1}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="import-folder-dialog-title"
    >
      <section className="w-full max-w-[680px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--surface-raised)] shadow-lg">
        <header className="flex min-h-[52px] items-center justify-between border-b border-[var(--border)] px-4">
          <h2 id="import-folder-dialog-title" className="m-0 text-[16px] font-semibold text-[var(--text-primary)]">
            {t("import.folderDialog.title")}
          </h2>
          <button type="button" className="btn btn--ghost btn--icon btn--sm" aria-label={t("import.actions.cancel")} onClick={onClose}>
            <X size={16} />
          </button>
        </header>
        <div className="space-y-4 px-4 py-4 text-[13px]">
          <div className="rounded-[var(--radius-md)] border border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-2 text-[12.5px] text-[var(--warning-text)]">
            <strong>{t("import.folderDialog.warnStrong")}</strong> · {t("import.folderDialog.warn")}
          </div>

          <div className="formrow">
            <div><div className="formrow__label">{t("import.folderDialog.path")}</div></div>
            <div className="formrow__control">
              <div className="input-group">
                <span className="input-group__lead"><Folder size={14} /></span>
                <input
                  ref={inputRef}
                  className="input input--mono"
                  placeholder={t("import.folderDialog.pathPlaceholder")}
                  value={path}
                  onChange={(event) => setPath(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      event.preventDefault();
                      void submit();
                    }
                  }}
                />
              </div>
            </div>
          </div>

          <div className="formrow">
            <div><div className="formrow__label">{t("import.folderDialog.template")}</div></div>
            <div className="formrow__control">
              <select
                className="input max-w-[240px]"
                value={template}
                onChange={(event) => setTemplate(event.target.value as ProjectTemplate)}
                aria-label={t("import.folderDialog.template")}
                disabled
                title={t("import.folderDialog.strategy.disabledHint")}
              >
                {TEMPLATES.map((tpl) => (
                  <option key={tpl} value={tpl}>{t(`import.folderDialog.template.${tpl}`)}</option>
                ))}
              </select>
            </div>
          </div>

          <div className="formrow">
            <div><div className="formrow__label">{t("import.folderDialog.strategy")}</div></div>
            <div className="formrow__control">
              <label className="checkbox">
                <input type="checkbox" checked={byType} onChange={(event) => setByType(event.target.checked)} disabled />
                {t("import.folderDialog.strategy.byType")}
              </label>
              <div className="mt-1">
                <label className="checkbox">
                  <input type="checkbox" checked={rename} onChange={(event) => setRename(event.target.checked)} disabled />
                  {t("import.folderDialog.strategy.rename")}
                </label>
              </div>
              <div className="mt-1">
                <label className="checkbox">
                  <input type="checkbox" checked={initGit} onChange={(event) => setInitGit(event.target.checked)} disabled />
                  {t("import.folderDialog.strategy.git")}
                </label>
              </div>
              <p className="m-0 mt-2 text-[11px] text-[var(--text-muted)]">{t("import.folderDialog.strategy.disabledHint")}</p>
            </div>
          </div>
        </div>
        <footer className="flex min-h-[52px] items-center justify-end gap-2 border-t border-[var(--border)] px-4">
          <button type="button" className="btn btn--sm" onClick={onClose}>{t("import.actions.cancel")}</button>
          <button type="button" className="btn btn--sm btn--primary" disabled={!path.trim() || submitting} onClick={() => void submit()}>
            {submitting ? (
              <>{t("import.folderDialog.previewing")}</>
            ) : (
              <>
                <FolderOpen size={14} className="mr-1 inline-block align-[-2px]" />
                {t("import.folderDialog.confirm")}
              </>
            )}
          </button>
        </footer>
      </section>
    </div>
  );
}
