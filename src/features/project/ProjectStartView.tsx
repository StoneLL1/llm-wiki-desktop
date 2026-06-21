import { useState } from "react";
import { useTranslation } from "react-i18next";
import { FolderOpen, Plus } from "lucide-react";

import { useProjectStore } from "../../stores/projectStore";
import type { ProjectTemplate } from "../../types/project";
import { ConfirmationDialog } from "../../components/app/ConfirmationDialog";

const templates: ProjectTemplate[] = [
  "general",
  "research",
  "reading",
  "personal-growth",
  "business",
];

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message: unknown }).message;
    if (typeof message === "string") return message;
  }
  return String(error);
}

export function ProjectStartView() {
  const { t } = useTranslation();
  const recentProjects = useProjectStore((state) => state.recentProjects);
  const pendingAction = useProjectStore((state) => state.pendingAction);
  const initializing = useProjectStore((state) => state.initializing);
  const storeError = useProjectStore((state) => state.error);
  const openProject = useProjectStore((state) => state.openProject);
  const createProject = useProjectStore((state) => state.createProject);
  const confirmPendingAction = useProjectStore((state) => state.confirmPendingAction);
  const cancelPendingAction = useProjectStore((state) => state.cancelPendingAction);
  const [openPath, setOpenPath] = useState("");
  const [rootPath, setRootPath] = useState("");
  const [name, setName] = useState("");
  const [template, setTemplate] = useState<ProjectTemplate>("general");
  const [busy, setBusy] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const run = async (operation: () => Promise<unknown>) => {
    setBusy(true);
    setLocalError(null);
    try {
      await operation();
    } catch (error) {
      setLocalError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="grid min-h-screen place-items-center bg-[var(--surface)] p-8 text-[13px]">
      <section className="w-full max-w-[760px] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--background)] p-6">
        <div className="mb-6 flex items-center gap-3 border-b border-[var(--border-subtle)] pb-4">
          <div className="grid h-8 w-8 place-items-center rounded-[var(--radius-md)] bg-[var(--foreground)] font-mono text-[11px] font-semibold text-[var(--text-inverse)]">LW</div>
          <div>
            <h1 className="m-0 text-[20px] font-semibold">{t("projectStart.title")}</h1>
            <p className="m-0 mt-1 text-[12px] text-[var(--text-muted)]">{t("projectStart.description")}</p>
          </div>
        </div>

        <div className="grid grid-cols-2 gap-4">
          <form
            className="grid gap-3 rounded-[var(--radius-md)] border border-[var(--border)] p-4"
            onSubmit={(event) => {
              event.preventDefault();
              if (openPath.trim()) void run(() => openProject(openPath.trim()));
            }}
          >
            <div className="flex items-center gap-2 font-semibold"><FolderOpen size={16} aria-hidden="true" />{t("projectStart.open.title")}</div>
            <label className="grid gap-1 text-[12px] text-[var(--text-muted)]">
              {t("projectStart.path")}
              <input className="settings-input font-mono" value={openPath} onChange={(event) => setOpenPath(event.target.value)} placeholder={t("projectStart.open.placeholder")} disabled={initializing || busy} />
            </label>
            <button className="settings-button" type="submit" disabled={initializing || busy || !openPath.trim()}>{t("projectStart.open.action")}</button>
          </form>

          <form
            className="grid gap-3 rounded-[var(--radius-md)] border border-[var(--border)] p-4"
            onSubmit={(event) => {
              event.preventDefault();
              if (rootPath.trim() && name.trim()) {
                void run(() => createProject({ rootPath: rootPath.trim(), name: name.trim(), template }));
              }
            }}
          >
            <div className="flex items-center gap-2 font-semibold"><Plus size={16} aria-hidden="true" />{t("projectStart.create.title")}</div>
            <label className="grid gap-1 text-[12px] text-[var(--text-muted)]">{t("projectStart.name")}<input className="settings-input" value={name} onChange={(event) => setName(event.target.value)} disabled={initializing || busy} /></label>
            <label className="grid gap-1 text-[12px] text-[var(--text-muted)]">{t("projectStart.path")}<input className="settings-input font-mono" value={rootPath} onChange={(event) => setRootPath(event.target.value)} placeholder={t("projectStart.create.placeholder")} disabled={initializing || busy} /></label>
            <label className="grid gap-1 text-[12px] text-[var(--text-muted)]">{t("projectStart.template")}<select className="settings-input" value={template} onChange={(event) => setTemplate(event.target.value as ProjectTemplate)} disabled={initializing || busy}>{templates.map((value) => <option key={value} value={value}>{t(`projectStart.templates.${value}`)}</option>)}</select></label>
            <button className="settings-button" type="submit" disabled={initializing || busy || !rootPath.trim() || !name.trim()}>{t("projectStart.create.action")}</button>
          </form>
        </div>

        <section className="mt-5">
          <h2 className="m-0 text-[11px] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]">{t("projectStart.recent")}</h2>
          {recentProjects.length === 0 ? <p className="text-[12px] text-[var(--text-muted)]">{initializing ? t("projectStart.loading") : t("projectStart.noRecent")}</p> : <div className="mt-2 grid gap-1">{recentProjects.map((project) => <button key={`${project.projectId}:${project.rootPath}`} type="button" className="flex h-9 items-center justify-between rounded-[var(--radius-md)] px-2 text-left hover:bg-[var(--surface-muted)]" disabled={initializing || busy} onClick={() => void run(() => openProject(project.rootPath))}><span className="font-medium">{project.name}</span><span className="max-w-[65%] truncate font-mono text-[11px] text-[var(--text-muted)]">{project.rootPath}</span></button>)}</div>}
        </section>

        {localError || storeError ? <p role="alert" className="mt-4 text-[12px] text-[var(--danger)]">{localError ?? storeError}</p> : null}
      </section>

      {pendingAction ? <ConfirmationDialog action={pendingAction} checkpointExists={false} onCancel={() => void run(cancelPendingAction)} onConfirm={() => void run(confirmPendingAction)} /> : null}
    </main>
  );
}
