import { FolderOpen, Plus, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { LazyActionableErrorNotice } from "../../components/app/LazyActionableErrorNotice";
import {
  normalizeBackendError,
  type NormalizedBackendError,
} from "../../lib/backendError";
import { useModalDialog } from "../../hooks/useModalDialog";
import type { ProjectTemplate } from "../../types/project";
import { pickDirectory } from "../import/nativeFilePicker";
import { buildProjectRootPath, sanitizeProjectFolderName, validateProjectName } from "./projectPath";

const LAST_PROJECT_PARENT_STORAGE_KEY = "llm-wiki-desktop.lastProjectParent";

function readLastProjectParent(): string {
  try {
    return window.localStorage.getItem(LAST_PROJECT_PARENT_STORAGE_KEY) ?? "";
  } catch {
    return "";
  }
}

function rememberProjectParent(path: string) {
  try {
    window.localStorage.setItem(LAST_PROJECT_PARENT_STORAGE_KEY, path);
  } catch {
    // The selected parent remains usable even when browser storage is unavailable.
  }
}

const TEMPLATES: Array<{ key: ProjectTemplate; titleKey: string; descKey: string }> = [
  { key: "general", titleKey: "launch.template.general", descKey: "launch.template.generalDesc" },
  { key: "research", titleKey: "launch.template.research", descKey: "launch.template.researchDesc" },
  { key: "reading", titleKey: "launch.template.reading", descKey: "launch.template.readingDesc" },
  { key: "personal-growth", titleKey: "launch.template.personal", descKey: "launch.template.personalDesc" },
  { key: "business", titleKey: "launch.template.business", descKey: "launch.template.businessDesc" },
];

export interface NewProjectPayload {
  rootPath: string;
  name: string;
  template: ProjectTemplate;
}

interface NewProjectDialogProps {
  busy: boolean;
  error?: unknown;
  onClose: () => void;
  onCreate: (payload: NewProjectPayload) => void;
}

export function NewProjectDialog({ busy, error, onClose, onCreate }: NewProjectDialogProps) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [parentPath, setParentPath] = useState(readLastProjectParent);
  const [template, setTemplate] = useState<ProjectTemplate>("general");
  const [pickerError, setPickerError] = useState<NormalizedBackendError | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const dialogRef = useModalDialog({ onClose, initialFocusRef: nameRef });
  const rootPath = buildProjectRootPath(parentPath, name);
  const folderName = sanitizeProjectFolderName(name);
  const nameValidation = validateProjectName(name);
  const nameError = nameValidation && name.trim() ? t("noProject.create.invalidName") : null;
  const canCreate = Boolean(!nameValidation && parentPath.trim() && folderName && rootPath);

  useEffect(() => {
    if (parentPath) return;
    let disposed = false;
    void invoke<unknown>("prepare_default_project_parent")
      .then((path) => {
        if (!disposed && typeof path === "string" && path.trim()) {
          setParentPath(path);
        }
      })
      .catch((error) => {
        if (!disposed) {
          setPickerError(normalizeBackendError(error, {
            defaultSummaryKey: "backendError.summary.project",
          }));
        }
      });
    return () => {
      disposed = true;
    };
  }, [parentPath]);

  const chooseParent = async () => {
    setPickerError(null);
    try {
      const selected = await pickDirectory({ title: t("launch.dialog.chooseParent") });
      if (selected) {
        setParentPath(selected);
        rememberProjectParent(selected);
      }
    } catch (error) {
      setPickerError(normalizeBackendError(error, {
        defaultSummaryKey: "backendError.summary.project",
      }));
    }
  };

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    if (!canCreate) return;
    onCreate({ rootPath, name: name.trim(), template });
  };

  return (
    <div
      ref={dialogRef}
      aria-labelledby="new-project-title"
      aria-modal="true"
      className="fixed inset-0 z-[100] grid place-items-center bg-black/40 p-4"
      onClick={onClose}
      role="dialog"
      tabIndex={-1}
    >
      <form
        className="dialog--wide w-[640px] max-w-full rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--background)] shadow-xl"
        onClick={(event) => event.stopPropagation()}
        onSubmit={submit}
      >
        <div className="flex items-center justify-between border-b border-[var(--border-subtle)] px-5 py-3">
          <h2 id="new-project-title" className="m-0 text-[15px] font-semibold">{t("launch.dialog.title")}</h2>
          <button aria-label={t("launch.dialog.close")} className="btn btn--ghost btn--icon btn--sm" onClick={onClose} type="button">
            <X aria-hidden="true" size={16} />
          </button>
        </div>

        <div className="px-5 py-4">
          <div className="formrow">
            <div>
              <div className="formrow__label">{t("launch.dialog.name")}</div>
              <div className="formrow__hint">{t("launch.dialog.nameHint")}</div>
            </div>
            <div className="formrow__control">
              <input
                ref={nameRef}
                aria-invalid={Boolean(nameError)}
                aria-label={t("launch.dialog.name")}
                className="input"
                onChange={(event) => setName(event.target.value)}
                value={name}
              />
              {nameError ? <p className="m-0 mt-1 text-[11px] text-[var(--danger)]" role="alert">{nameError}</p> : null}
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("launch.dialog.parent")}</div>
              <div className="formrow__hint">{t("launch.dialog.locationHint")}</div>
            </div>
            <div className="formrow__control">
              <div className="input-group">
                <span className="input-group__lead"><FolderOpen aria-hidden="true" size={14} /></span>
                <input
                  aria-label={t("launch.dialog.parent")}
                  className="input input--mono"
                  placeholder={t("launch.dialog.parentPlaceholder")}
                  readOnly
                  value={parentPath}
                />
                <span className="input-group__trail">
                  <button className="btn btn--sm btn--ghost" onClick={() => void chooseParent()} type="button">
                    {t("launch.dialog.browse")}
                  </button>
                </span>
              </div>
              {rootPath ? <div aria-label={t("launch.dialog.fullPath")} className="project-path-preview">{rootPath}</div> : null}
              {pickerError ? <LazyActionableErrorNotice className="mt-1" error={pickerError} /> : null}
              {error ? <LazyActionableErrorNotice className="mt-1" error={error} /> : null}
            </div>
          </div>

          <div className="formrow">
            <div>
              <div className="formrow__label">{t("launch.dialog.template")}</div>
              <div className="formrow__hint">{t("launch.dialog.templateHint")}</div>
            </div>
            <div className="formrow__control">
              <div aria-label={t("launch.dialog.template")} className="seg" role="group">
                {TEMPLATES.map((entry) => (
                  <button
                    className={template === entry.key ? "is-active" : ""}
                    key={entry.key}
                    onClick={() => setTemplate(entry.key)}
                    type="button"
                  >
                    {t(entry.titleKey)}
                  </button>
                ))}
              </div>
              <p className="m-0 mt-2 text-[11px] text-[var(--text-muted)]">
                {t(TEMPLATES.find((entry) => entry.key === template)?.descKey ?? "")}
              </p>
            </div>
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 border-t border-[var(--border-subtle)] px-5 py-3">
          <button className="btn" onClick={onClose} type="button">{t("launch.dialog.cancel")}</button>
          <button className="btn btn--primary" disabled={busy || !canCreate} type="submit">
            <Plus aria-hidden="true" size={14} />
            {t("launch.dialog.create")}
          </button>
        </div>
      </form>
    </div>
  );
}
